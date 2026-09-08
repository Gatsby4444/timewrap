//! Cœur métier de Timewrap.
//!
//! Tout ce qui est risqué et testable vit ici : lecture iCalendar, expansion des
//! récurrences, arithmétique de fuseaux, stockage. L'interface Android
//! (Kotlin/Compose) consomme ce module via les bindings générés par UniFFI, et
//! une future application iOS réutilisera le même code via les bindings Swift.

uniffi::setup_scaffolding!();

mod error;
mod ics;
mod model;
mod store;

#[cfg(test)]
mod tests;

pub use error::TimewrapError;
pub use model::{Calendar, CalendarKind, DayAgenda, ImportReport, NowView, Occurrence};

use std::sync::{Arc, Mutex};

use chrono::Utc;

use error::Result;
use store::Store;

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

    pub fn delete_calendar(&self, calendar_id: String) -> Result<()> {
        self.store()?.delete_calendar(&calendar_id)
    }

    // --------------------------------------------------------------- requêtes

    /// Occurrences chevauchant l'intervalle, en secondes Unix.
    pub fn occurrences_between(&self, from_utc: i64, to_utc: i64) -> Result<Vec<Occurrence>> {
        self.store()?.occurrences_between(from_utc, to_utc)
    }

    /// Une journée, `epoch_day` étant compté comme `LocalDate.toEpochDay()`.
    pub fn day(&self, epoch_day: i64) -> Result<DayAgenda> {
        self.store()?.day(epoch_day)
    }

    /// `days` journées consécutives — la vue Semaine en demande sept.
    pub fn days(&self, epoch_day: i64, days: u32) -> Result<Vec<DayAgenda>> {
        self.store()?.days(epoch_day, days)
    }

    /// Où j'en suis maintenant, et ce qui vient après.
    pub fn now_view(&self) -> Result<NowView> {
        self.store()?.now_view(now())
    }

    /// Re-développe les récurrences si l'horizon devient trop proche.
    /// À appeler au démarrage ; ne fait rien la plupart du temps.
    pub fn ensure_horizon(&self) -> Result<bool> {
        self.store()?.ensure_horizon(now())
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
            occurrences: store.occurrences_between(now - 86_400, now + 86_400)?.len() as u32,
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
}

/// Version de la crate, telle que déclarée dans `Cargo.toml`.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn now() -> i64 {
    Utc::now().timestamp()
}
