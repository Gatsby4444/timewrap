//! Cœur métier de Timewrap.
//!
//! Tout ce qui est risqué et testable vit ici : parsing iCalendar, expansion des
//! récurrences, arithmétique de fuseaux, stockage. L'UI Android (Kotlin/Compose)
//! consomme ce module via les bindings UniFFI.
//!
//! Phase 0 : la crate n'expose qu'un auto-diagnostic. Son rôle est de prouver que
//! l'intégralité de la pile native (SQLite compilé en C, chrono-tz, icalendar,
//! rrule) se compile pour Android et s'exécute réellement sur l'appareil.

uniffi::setup_scaffolding!();

use std::str::FromStr;

use chrono::Utc;
use chrono_tz::Europe::Paris;
use icalendar::{Calendar, CalendarComponent, Component, EventLike};
use rrule::RRuleSet;

/// Un `.ics` minimal mais représentatif d'un export d'ENT : fuseau nommé,
/// récurrence hebdomadaire, salle en `LOCATION`.
const SAMPLE_ICS: &str = concat!(
    "BEGIN:VCALENDAR\r\n",
    "VERSION:2.0\r\n",
    "PRODID:-//Timewrap//Auto-diagnostic//FR\r\n",
    "BEGIN:VEVENT\r\n",
    "UID:selftest-1@timewrap\r\n",
    "DTSTAMP:20260901T080000Z\r\n",
    "DTSTART;TZID=Europe/Paris:20260907T080000\r\n",
    "DTEND;TZID=Europe/Paris:20260907T100000\r\n",
    "SUMMARY:Cours de test\r\n",
    "LOCATION:Salle B204\r\n",
    "RRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=5\r\n",
    "END:VEVENT\r\n",
    "END:VCALENDAR\r\n",
);

const SAMPLE_RRULE: &str =
    "DTSTART;TZID=Europe/Paris:20260907T080000\nRRULE:FREQ=WEEKLY;BYDAY=MO;COUNT=5";

/// Résultat de l'auto-diagnostic, affiché tel quel par l'application.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SelfTest {
    pub core_version: String,
    pub now_utc: String,
    pub now_paris: String,
    pub sqlite_version: String,
    pub ics_event_count: u32,
    pub ics_first_summary: String,
    pub ics_first_location: String,
    pub rrule_occurrence_count: u32,
    pub rrule_first: String,
    pub rrule_last: String,
    /// Vide si tout va bien ; sinon une ligne lisible par sous-système en échec.
    pub failures: Vec<String>,
}

impl SelfTest {
    fn empty() -> Self {
        Self {
            core_version: core_version(),
            now_utc: String::new(),
            now_paris: String::new(),
            sqlite_version: String::new(),
            ics_event_count: 0,
            ics_first_summary: String::new(),
            ics_first_location: String::new(),
            rrule_occurrence_count: 0,
            rrule_first: String::new(),
            rrule_last: String::new(),
            failures: Vec::new(),
        }
    }
}

/// Version de la crate, telle que déclarée dans `Cargo.toml`.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Exerce chaque dépendance native et rend compte, sans jamais paniquer.
#[uniffi::export]
pub fn self_test() -> SelfTest {
    let mut out = SelfTest::empty();

    // Horloge et base de fuseaux (chrono + chrono-tz).
    let now = Utc::now();
    out.now_utc = now.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    out.now_paris = now
        .with_timezone(&Paris)
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string();

    // SQLite compilé depuis les sources C par le NDK.
    match sqlite_version() {
        Ok(v) => out.sqlite_version = v,
        Err(e) => out.failures.push(format!("sqlite: {e}")),
    }

    // Parsing iCalendar.
    match SAMPLE_ICS.parse::<Calendar>() {
        Ok(cal) => {
            let events: Vec<_> = cal
                .components
                .iter()
                .filter_map(|c| match c {
                    CalendarComponent::Event(e) => Some(e),
                    _ => None,
                })
                .collect();
            out.ics_event_count = events.len() as u32;
            if let Some(first) = events.first() {
                out.ics_first_summary = first.get_summary().unwrap_or_default().to_string();
                out.ics_first_location = first.get_location().unwrap_or_default().to_string();
            } else {
                out.failures.push("ics: aucun VEVENT trouvé".to_string());
            }
        }
        Err(e) => out.failures.push(format!("ics: {e}")),
    }

    // Expansion de récurrence.
    match RRuleSet::from_str(SAMPLE_RRULE) {
        Ok(set) => {
            let dates = set.all(50).dates;
            out.rrule_occurrence_count = dates.len() as u32;
            if let Some(d) = dates.first() {
                out.rrule_first = d.format("%Y-%m-%d %H:%M %Z").to_string();
            }
            if let Some(d) = dates.last() {
                out.rrule_last = d.format("%Y-%m-%d %H:%M %Z").to_string();
            }
            if dates.len() != 5 {
                out.failures.push(format!(
                    "rrule: 5 occurrences attendues, {} obtenues",
                    dates.len()
                ));
            }
        }
        Err(e) => out.failures.push(format!("rrule: {e}")),
    }

    out
}

fn sqlite_version() -> Result<String, rusqlite::Error> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passe_sur_lhote() {
        let r = self_test();
        assert!(r.failures.is_empty(), "échecs : {:?}", r.failures);
        assert_eq!(r.ics_event_count, 1);
        assert_eq!(r.ics_first_summary, "Cours de test");
        assert_eq!(r.ics_first_location, "Salle B204");
        assert_eq!(r.rrule_occurrence_count, 5);
        assert!(!r.sqlite_version.is_empty());
    }

    #[test]
    fn version_non_vide() {
        assert!(!core_version().is_empty());
    }
}
