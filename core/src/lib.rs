//! Cœur métier de Timewrap.
//!
//! Tout ce qui est risqué et testable vit ici : lecture iCalendar, expansion des
//! récurrences, arithmétique de fuseaux, stockage, lecture des champs
//! structurés, règles visuelles, chevauchements, choses à faire et rappels.
//! L'interface Android (Kotlin/Compose) consomme ce module via les bindings
//! générés par UniFFI, et une future application iOS réutilisera le même code
//! via les bindings Swift.
//!
//! Une seule règle d'architecture, mais tenue : aucune décision métier ne se
//! prend au-dessus de cette frontière. L'interface affiche et demande ; elle ne
//! recalcule jamais une couleur, ni un conflit, ni la date d'un rappel.

uniffi::setup_scaffolding!();

mod conflict;
mod error;
mod ics;
mod model;
mod properties;
mod rules;
mod store;

#[cfg(test)]
mod tests;

pub use error::TimewrapError;
pub use model::{
    CalendarKind, Category, Change, ChangeKind, Conflict, ConflictPair, DayAgenda, EventDraft,
    EventOrigin, ImportReport, NowView, Occurrence, PropertyKey, PropertyValue, Reminder,
    Resolution, Rule, RuleField, RuleMatch, RuleSuggestion, SaveOutcome, Settings, SyncReport,
    Task, Timetable,
};

use std::sync::{Arc, Mutex};

use chrono::Utc;

use error::Result;
use store::Store;

/// Combien de jours d'avance le programmateur de rappels regarde.
const REMINDER_HORIZON_DAYS: i64 = 3;

/// Point d'entrée unique du cœur : une base ouverte, un fuseau d'affichage.
#[derive(uniffi::Object)]
pub struct Timewrap {
    store: Mutex<Store>,
}

#[uniffi::export]
impl Timewrap {
    /// Ouvre — ou crée — la base à `db_path`.
    ///
    /// `display_timezone` est un identifiant IANA (« Europe/Paris »). Il décide
    /// du découpage en journées, donc de ce qu'affiche la vue Jour.
    #[uniffi::constructor]
    pub fn open(db_path: String, display_timezone: String) -> Result<Arc<Self>> {
        let store = Store::open(&db_path, &display_timezone)?;
        Ok(Arc::new(Timewrap {
            store: Mutex::new(store),
        }))
    }

    pub fn set_display_timezone(&self, timezone: String) -> Result<()> {
        self.store()?.set_display_timezone(&timezone)
    }

    pub fn display_timezone(&self) -> Result<String> {
        Ok(self.store()?.display_timezone().name().to_string())
    }

    /// Le jour d'aujourd'hui, compté comme `LocalDate.toEpochDay()`.
    pub fn today(&self) -> Result<i64> {
        self.store()?.epoch_day_of(now())
    }

    // -------------------------------------------------------- emploi du temps

    /// L'emploi du temps, ou rien s'il n'a pas encore été importé.
    pub fn timetable(&self) -> Result<Option<Timetable>> {
        self.store()?.timetable()
    }

    /// Charge un `.ics` : premier import, ou remplacement du contenu.
    ///
    /// L'identité de l'emploi du temps ne change pas : les séances ajoutées à la
    /// main, les masquages et les couleurs survivent. Le rapport dit ce qui a
    /// bougé depuis la version précédente.
    pub fn import_ics(
        &self,
        name: String,
        kind: CalendarKind,
        source: String,
        ics_text: String,
    ) -> Result<SyncReport> {
        self.store()?
            .import_ics(&name, kind, &source, &ics_text, now())
    }

    pub fn rename_timetable(&self, name: String) -> Result<()> {
        self.store()?.rename(&name)
    }

    pub fn set_timetable_color(&self, color: u32) -> Result<()> {
        self.store()?.set_color(color)
    }

    /// Oublie l'emploi du temps. Les choses à faire ne bougent pas.
    pub fn clear_timetable(&self) -> Result<()> {
        self.store()?.clear_timetable()
    }

    /// Re-développe les récurrences si l'horizon devient trop proche.
    /// À appeler au démarrage ; ne fait rien la plupart du temps.
    pub fn ensure_horizon(&self) -> Result<bool> {
        self.store()?.ensure_horizon(now())
    }

    // --------------------------------------------------------------- requêtes

    /// Séances chevauchant l'intervalle, en secondes Unix.
    pub fn occurrences_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        self.store()?.occurrences_between(from_utc, to_utc)
    }

    /// Une journée : ses séances et ses choses à faire.
    pub fn day(&self, epoch_day: i64) -> Result<DayAgenda> {
        self.store()?.day(epoch_day, now())
    }

    /// `days` journées consécutives — la vue Semaine en demande sept.
    pub fn days(&self, epoch_day: i64, days: u32) -> Result<Vec<DayAgenda>> {
        self.store()?.days(epoch_day, days, now())
    }

    /// Où j'en suis maintenant, et ce qui vient après.
    pub fn now_view(&self) -> Result<NowView> {
        self.store()?.now_view(now())
    }

    pub fn occurrence(&self, id: String) -> Result<Occurrence> {
        self.store()?.occurrence(&id)
    }

    /// Les champs structurés d'une séance — « Type : TD », « Matière : … » —
    /// avec la couleur que chacun porte.
    pub fn occurrence_properties(&self, id: String) -> Result<Vec<PropertyValue>> {
        self.store()?.occurrence_properties(&id)
    }

    // ------------------------------------------------- événements et conflits

    /// Ce qui heurterait un créneau, avant même de l'écrire.
    pub fn check_conflicts(&self, draft: EventDraft) -> Result<Vec<Conflict>> {
        self.store()?
            .conflicts_for(draft.id.as_deref(), draft.start_utc, draft.end_utc)
    }

    /// Écrit un événement.
    ///
    /// Avec `Resolution::Cancel` — le défaut — rien n'est écrit tant qu'un
    /// chevauchement subsiste : le résultat revient `blocked`, la liste des
    /// conflits en main, et l'interface pose la question.
    pub fn save_event(&self, draft: EventDraft, resolution: Resolution) -> Result<SaveOutcome> {
        self.store()?.save_event(&draft, resolution, now())
    }

    pub fn delete_event(&self, id: String) -> Result<()> {
        self.store()?.delete_event(&id)
    }

    /// Masque une séance sans la supprimer — la sortie de secours quand deux
    /// créneaux importés se disputent la même heure.
    pub fn set_occurrence_muted(&self, id: String, muted: bool) -> Result<()> {
        self.store()?.set_muted(&id, muted)
    }

    /// Force la catégorie d'une séance. `None` rend la main aux règles.
    pub fn set_occurrence_category(&self, id: String, category_id: Option<String>) -> Result<()> {
        self.store()?.set_occurrence_category(&id, category_id)
    }

    pub fn conflicts_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<ConflictPair>> {
        self.store()?.conflicts_between(from_utc, to_utc)
    }

    /// Les séances masquées à la main sur une fenêtre.
    pub fn muted_occurrences(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        self.store()?.muted_between(from_utc, to_utc)
    }

    // ------------------------------------------------------- couleurs et règles

    /// Les champs structurés repérés dans l'emploi du temps : « Type »,
    /// « Matière », « Salle »… C'est par eux qu'on colorie.
    pub fn property_keys(&self) -> Result<Vec<PropertyKey>> {
        self.store()?.property_keys()
    }

    /// Les valeurs prises par un champ, chacune avec sa couleur.
    pub fn property_values(&self, key: String) -> Result<Vec<PropertyValue>> {
        self.store()?.property_values(&key)
    }

    /// Donne une couleur à une valeur — « Matière : Analyse » en vert.
    pub fn set_property_color(&self, key: String, value: String, color: u32) -> Result<Category> {
        self.store()?.set_property_color(&key, &value, color, now())
    }

    pub fn clear_property_color(&self, key: String, value: String) -> Result<()> {
        self.store()?.clear_property_color(&key, &value)
    }

    /// Colorie d'un coup toutes les valeurs d'un champ. Renvoie le nombre de
    /// couleurs posées.
    pub fn auto_color_property(&self, key: String) -> Result<u32> {
        self.store()?.auto_color_property(&key, now())
    }

    pub fn categories(&self) -> Result<Vec<Category>> {
        self.store()?.categories()
    }

    pub fn create_category(
        &self,
        name: String,
        label: String,
        color: Option<u32>,
    ) -> Result<Category> {
        self.store()?.create_category(&name, &label, color, now())
    }

    pub fn update_category(
        &self,
        id: String,
        name: String,
        label: String,
        color: u32,
    ) -> Result<Category> {
        self.store()?.update_category(&id, &name, &label, color)
    }

    pub fn delete_category(&self, id: String) -> Result<()> {
        self.store()?.delete_category(&id)
    }

    pub fn rules(&self) -> Result<Vec<Rule>> {
        self.store()?.rules()
    }

    /// Enregistre une règle et la rejoue aussitôt. Un identifiant vide en crée
    /// une nouvelle.
    pub fn save_rule(&self, rule: Rule) -> Result<Rule> {
        self.store()?.save_rule(&rule, now())
    }

    pub fn delete_rule(&self, id: String) -> Result<()> {
        self.store()?.delete_rule(&id)
    }

    /// Rejoue les règles sur toutes les séances. Renvoie le nombre de séances
    /// dont l'apparence a changé.
    pub fn reapply_rules(&self) -> Result<u32> {
        self.store()?.reapply_rules()
    }

    /// Ce que le cœur propose de colorier, déduit de ce qui a été importé.
    pub fn rule_suggestions(&self) -> Result<Vec<RuleSuggestion>> {
        self.store()?.rule_suggestions()
    }

    /// Crée d'un geste la catégorie et la règle correspondant à une suggestion.
    pub fn accept_suggestion(
        &self,
        suggestion: RuleSuggestion,
        name: String,
        label: String,
        color: Option<u32>,
    ) -> Result<Rule> {
        self.store()?
            .accept_suggestion(&suggestion, &name, &label, color, now())
    }

    // -------------------------------------------------------- choses à faire

    /// Ce qu'il y a à faire un jour donné, retards compris.
    pub fn tasks_for_day(&self, epoch_day: i64) -> Result<Vec<Task>> {
        let store = self.store()?;
        let today = store.epoch_day_of(now())?;
        store.tasks_for_day(epoch_day, today)
    }

    /// Tout ce qui reste ouvert, du plus ancien au plus récent.
    pub fn pending_tasks(&self) -> Result<Vec<Task>> {
        let store = self.store()?;
        let today = store.epoch_day_of(now())?;
        store.pending_tasks(today)
    }

    pub fn add_task(&self, title: String, epoch_day: i64) -> Result<Task> {
        self.store()?.add_task(&title, epoch_day, now())
    }

    /// Coche ou décoche. Une tâche cochée le reste sur le jour où elle l'a été.
    pub fn set_task_done(&self, id: String, done: bool) -> Result<Task> {
        let store = self.store()?;
        let today = store.epoch_day_of(now())?;
        store.set_task_done(&id, done, today)
    }

    pub fn update_task(&self, id: String, title: String, notes: String) -> Result<()> {
        self.store()?.update_task(&id, &title, &notes)
    }

    /// Repousse une tâche à un autre jour.
    pub fn move_task(&self, id: String, epoch_day: i64) -> Result<()> {
        self.store()?.move_task(&id, epoch_day)
    }

    pub fn delete_task(&self, id: String) -> Result<()> {
        self.store()?.delete_task(&id)
    }

    // ------------------------------------------------------ rappels et réglages

    /// Les rappels à programmer sur l'appareil, du plus proche au plus lointain.
    pub fn reminders(&self, limit: u32) -> Result<Vec<Reminder>> {
        self.store()?.reminders(now(), REMINDER_HORIZON_DAYS, limit)
    }

    pub fn settings(&self) -> Result<Settings> {
        self.store()?.settings()
    }

    pub fn update_settings(&self, settings: Settings) -> Result<Settings> {
        self.store()?.update_settings(&settings)
    }

    // ------------------------------------------------------------ diagnostics

    /// Vérifie que chaque dépendance native répond, sur l'appareil.
    pub fn self_test(&self) -> Result<SelfTest> {
        let store = self.store()?;
        let now = now();
        Ok(SelfTest {
            core_version: core_version(),
            display_timezone: store.display_timezone().name().to_string(),
            occurrences: store.occurrences_between(now - 86_400, now + 86_400)?.len() as u32,
            categories: store.categories()?.len() as u32,
            rules: store.rules()?.len() as u32,
            properties: store.property_keys()?.len() as u32,
            pending_tasks: store.pending_tasks(store.epoch_day_of(now)?)?.len() as u32,
        })
    }
}

// Hors du bloc exporté : UniFFI publierait sinon ces aides internes, dont le
// type de retour n'a aucun équivalent de l'autre côté de la frontière.
impl Timewrap {
    fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| TimewrapError::Database("verrou de base empoisonné".into()))
    }
}

/// État interne, affiché par l'écran de diagnostic.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SelfTest {
    pub core_version: String,
    pub display_timezone: String,
    pub occurrences: u32,
    pub categories: u32,
    pub rules: u32,
    pub properties: u32,
    pub pending_tasks: u32,
}

/// Version de la crate, telle que déclarée dans `Cargo.toml`.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn now() -> i64 {
    Utc::now().timestamp()
}
