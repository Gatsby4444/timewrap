//! Cœur métier de Timewrap.
//!
//! Tout ce qui est risqué et testable vit ici : lecture iCalendar, expansion des
//! récurrences, arithmétique de fuseaux, stockage, moteur de règles visuelles et
//! moteur de chevauchements. L'interface Android (Kotlin/Compose) consomme ce
//! module via les bindings générés par UniFFI, et une future application iOS
//! réutilisera le même code via les bindings Swift.
//!
//! Une seule règle d'architecture, mais tenue : aucune décision métier ne se
//! prend au-dessus de cette frontière. L'interface affiche et demande ; elle ne
//! recalcule jamais une couleur, ni un conflit.

uniffi::setup_scaffolding!();

mod conflict;
mod error;
mod ics;
mod model;
mod rules;
mod store;

#[cfg(test)]
mod tests;

pub use error::TimewrapError;
pub use model::{
    Calendar, CalendarKind, CalendarSummary, Category, Conflict, ConflictPair, ConflictScope,
    DayAgenda, EventDraft, EventOrigin, ImportReport, NowView, Occurrence, Resolution, Rule,
    RuleField, RuleMatch, RuleSuggestion, SaveOutcome,
};

use std::sync::{Arc, Mutex};

use chrono::Utc;

use error::Result;
use store::{Scope, Store};

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

    // ---------------------------------------------------------------- agendas

    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        self.store()?.calendars()
    }

    /// Une tuile par agenda pour l'écran d'accueil : ce qui vient, et ce qui
    /// cloche.
    pub fn calendar_summaries(&self) -> Result<Vec<CalendarSummary>> {
        self.store()?.calendar_summaries(now())
    }

    /// Crée un agenda vide — un dossier que l'on remplit à la main.
    pub fn create_calendar(&self, name: String, color: Option<u32>) -> Result<Calendar> {
        self.store()?
            .create_calendar(&name, CalendarKind::Local, "", color, now())
    }

    /// Importe un flux `.ics` dans un nouvel agenda.
    ///
    /// `source` garde la trace de l'origine — chemin du fichier ou URL
    /// d'abonnement — pour pouvoir resynchroniser plus tard.
    pub fn import_ics(
        &self,
        name: String,
        kind: CalendarKind,
        source: String,
        ics_text: String,
    ) -> Result<ImportReport> {
        self.store()?
            .import_ics(&name, kind, &source, &ics_text, now())
    }

    /// Remplace le contenu d'un agenda par une version plus récente du flux.
    pub fn reimport_ics(&self, calendar_id: String, ics_text: String) -> Result<ImportReport> {
        self.store()?.reimport_ics(&calendar_id, &ics_text, now())
    }

    pub fn set_calendar_visible(&self, calendar_id: String, visible: bool) -> Result<()> {
        self.store()?.set_visible(&calendar_id, visible)
    }

    pub fn set_calendar_color(&self, calendar_id: String, color: u32) -> Result<()> {
        self.store()?.set_color(&calendar_id, color)
    }

    pub fn rename_calendar(&self, calendar_id: String, name: String) -> Result<()> {
        self.store()?.rename_calendar(&calendar_id, &name)
    }

    /// Déplace un agenda à la position `target` de l'écran d'accueil.
    pub fn move_calendar(&self, calendar_id: String, target: i32) -> Result<()> {
        self.store()?.move_calendar(&calendar_id, target)
    }

    pub fn delete_calendar(&self, calendar_id: String) -> Result<()> {
        self.store()?.delete_calendar(&calendar_id)
    }

    // --------------------------------------------------------------- requêtes
    //
    // `scope` restreint la vue à une liste d'agendas. Absent ou vide, la vue
    // montre tous les agendas non masqués ; renseigné, il l'emporte sur la
    // visibilité, parce qu'ouvrir un dossier doit le montrer.

    /// Occurrences chevauchant l'intervalle, en secondes Unix.
    pub fn occurrences_between(
        &self,
        from_utc: i64,
        to_utc: i64,
        scope: Scope,
    ) -> Result<Vec<Occurrence>> {
        self.store()?.occurrences_between(from_utc, to_utc, &scope)
    }

    /// Une journée, `epoch_day` étant compté comme `LocalDate.toEpochDay()`.
    pub fn day(&self, epoch_day: i64, scope: Scope) -> Result<DayAgenda> {
        self.store()?.day(epoch_day, &scope)
    }

    /// `days` journées consécutives — la vue Semaine en demande sept.
    pub fn days(&self, epoch_day: i64, days: u32, scope: Scope) -> Result<Vec<DayAgenda>> {
        self.store()?.days(epoch_day, days, &scope)
    }

    /// Où j'en suis maintenant, et ce qui vient après.
    pub fn now_view(&self, scope: Scope) -> Result<NowView> {
        self.store()?.now_view(now(), &scope)
    }

    pub fn occurrence(&self, id: String) -> Result<Occurrence> {
        self.store()?.occurrence(&id)
    }

    /// Re-développe les récurrences si l'horizon devient trop proche.
    /// À appeler au démarrage ; ne fait rien la plupart du temps.
    pub fn ensure_horizon(&self) -> Result<bool> {
        self.store()?.ensure_horizon(now())
    }

    // ---------------------------------------------------- événements et conflits

    /// Ce qui heurterait un créneau, avant même de l'écrire.
    ///
    /// À appeler pendant la saisie : c'est ce qui permet à l'écran d'édition de
    /// prévenir en direct, sans attendre l'enregistrement.
    pub fn check_conflicts(&self, draft: EventDraft) -> Result<Vec<Conflict>> {
        self.store()?.conflicts_for(
            draft.id.as_deref(),
            &draft.calendar_id,
            draft.start_utc,
            draft.end_utc,
        )
    }

    /// Écrit un événement.
    ///
    /// Avec `Resolution::Cancel` — le défaut — rien n'est écrit tant qu'un
    /// chevauchement subsiste : le résultat revient `blocked`, la liste des
    /// conflits en main, et l'interface pose la question. Rappeler ensuite la
    /// même méthode avec la résolution choisie.
    pub fn save_event(&self, draft: EventDraft, resolution: Resolution) -> Result<SaveOutcome> {
        self.store()?.save_event(&draft, resolution, now())
    }

    pub fn delete_event(&self, id: String) -> Result<()> {
        self.store()?.delete_event(&id)
    }

    /// Masque une séance précise sans toucher à son agenda — la sortie de
    /// secours quand deux créneaux importés se disputent la même heure.
    pub fn set_occurrence_muted(&self, id: String, muted: bool) -> Result<()> {
        self.store()?.set_muted(&id, muted)
    }

    /// Force la catégorie d'une séance. `None` rend la main aux règles.
    pub fn set_occurrence_category(&self, id: String, category_id: Option<String>) -> Result<()> {
        self.store()?.set_occurrence_category(&id, category_id)
    }

    /// Tous les chevauchements déjà présents sur une fenêtre, pour le
    /// gestionnaire de conflits.
    pub fn conflicts_between(
        &self,
        from_utc: i64,
        to_utc: i64,
        scope: Scope,
    ) -> Result<Vec<ConflictPair>> {
        self.store()?.conflicts_between(from_utc, to_utc, &scope)
    }

    /// Les séances masquées à la main sur une fenêtre — celles qu'un
    /// « Remplacer » a écartées, et que l'on doit pouvoir rappeler.
    pub fn muted_occurrences(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        self.store()?.muted_between(from_utc, to_utc)
    }

    // ------------------------------------------------- catégories et règles

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

    /// Enregistre une règle et la rejoue aussitôt sur toute la base.
    /// Un identifiant vide crée une nouvelle règle.
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

    /// Ce que le cœur propose de classer, déduit des intitulés importés.
    pub fn rule_suggestions(&self, scope: Scope) -> Result<Vec<RuleSuggestion>> {
        self.store()?.rule_suggestions(&scope)
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

    // ------------------------------------------------------------ diagnostics

    /// Vérifie que chaque dépendance native répond, sur l'appareil.
    pub fn self_test(&self) -> Result<SelfTest> {
        let store = self.store()?;
        let now = now();
        Ok(SelfTest {
            core_version: core_version(),
            display_timezone: store.display_timezone().name().to_string(),
            calendars: store.calendars()?.len() as u32,
            occurrences: store
                .occurrences_between(now - 86_400, now + 86_400, &None)?
                .len() as u32,
            categories: store.categories()?.len() as u32,
            rules: store.rules()?.len() as u32,
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
    pub calendars: u32,
    pub occurrences: u32,
    pub categories: u32,
    pub rules: u32,
}

/// Version de la crate, telle que déclarée dans `Cargo.toml`.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn now() -> i64 {
    Utc::now().timestamp()
}
