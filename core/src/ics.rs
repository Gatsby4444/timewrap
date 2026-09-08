//! Lecture d'un flux iCalendar et développement des récurrences.
//!
//! L'objectif n'est pas la conformité exhaustive à la RFC 5545 mais la
//! tolérance : un export d'ENT contient des intitulés bruts, des propriétés
//! propriétaires, parfois des dates de fin manquantes. Un composant qu'on ne
//! sait pas lire est ignoré et signalé, jamais fatal pour l'import entier.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use icalendar::{
    Calendar as ICalendar, CalendarComponent, CalendarDateTime, Component, DatePerhapsTime,
    EventLike,
};
use rrule::Tz as RTz;

use crate::error::{Result, TimewrapError};

/// Une occurrence développée, avant écriture en base.
#[derive(Debug, Clone)]
pub struct ExpandedEvent {
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub description: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub all_day: bool,
    pub cancelled: bool,
}

/// Ce qu'a produit une passe de développement.
#[derive(Debug, Default)]
pub struct Expansion {
    pub occurrences: Vec<ExpandedEvent>,
    /// Nombre de VEVENT distincts lus, séries comprises.
    pub events: u32,
    /// Composants écartés, avec la raison, pour affichage après import.
    pub skipped: Vec<String>,
}

/// Durée retenue quand un événement n'a ni `DTEND` ni `DURATION`.
const DEFAULT_DURATION: Duration = Duration::hours(1);

/// Garde-fou : une règle pathologique ne doit pas produire un million de lignes.
const MAX_OCCURRENCES_PER_EVENT: u16 = 2000;

/// Lit un flux `.ics` et développe ses récurrences sur `[from_utc, to_utc)`.
///
/// `default_tz` sert de repli pour les dates flottantes, celles qui n'indiquent
/// ni `Z` ni `TZID` — la RFC dit qu'elles suivent le fuseau de l'observateur.
pub fn expand(
    ics: &str,
    from_utc: i64,
    to_utc: i64,
    default_tz: chrono_tz::Tz,
) -> Result<Expansion> {
    let calendar: ICalendar = ics
        .parse()
        .map_err(|e: String| TimewrapError::Parse(e.to_string()))?;

    let from = utc_from_timestamp(from_utc)?;
    let to = utc_from_timestamp(to_utc)?;

    let mut out = Expansion::default();

    // Les remplacements d'occurrence (RECURRENCE-ID) sont appliqués après coup,
    // une fois les séries développées : ils en corrigent une date précise.
    let mut overrides: Vec<(String, i64, Option<ExpandedEvent>)> = Vec::new();

    for component in &calendar.components {
        let CalendarComponent::Event(event) = component else {
            continue;
        };
        out.events += 1;

        let uid = event.get_uid().unwrap_or_default().to_string();
        if uid.is_empty() {
            out.skipped
                .push("un événement sans UID a été ignoré".to_string());
            continue;
        }

        let summary = event.get_summary().unwrap_or_default().trim().to_string();
        let location = event.get_location().unwrap_or_default().trim().to_string();
        let description = event
            .get_description()
            .unwrap_or_default()
            .trim()
            .to_string();
        let cancelled = event
            .property_value("STATUS")
            .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED"));

        let Some(start) = event.get_start() else {
            out.skipped
                .push(format!("« {summary} » ignoré : pas de date de début"));
            continue;
        };
        let all_day = matches!(start, DatePerhapsTime::Date(_));

        let Some(start_dt) = to_instant(&start, default_tz) else {
            out.skipped
                .push(format!("« {summary} » ignoré : date de début illisible"));
            continue;
        };

        let duration = event
            .get_end()
            .and_then(|end| to_instant(&end, default_tz))
            .map(|end| end - start_dt)
            .filter(|d| *d > Duration::zero())
            .unwrap_or(if all_day {
                Duration::days(1)
            } else {
                DEFAULT_DURATION
            });

        // Un remplacement ne se développe pas : il vise une occurrence unique.
        if let Some(recurrence_id) = event.get_recurrence_id() {
            let Some(target) = to_instant(&recurrence_id, default_tz) else {
                out.skipped
                    .push(format!("« {summary} » ignoré : RECURRENCE-ID illisible"));
                continue;
            };
            let replacement = ExpandedEvent {
                uid: uid.clone(),
                summary,
                location,
                description,
                start_utc: start_dt.timestamp(),
                end_utc: (start_dt + duration).timestamp(),
                all_day,
                cancelled,
            };
            overrides.push((uid, target.timestamp(), Some(replacement)));
            continue;
        }

        let make = |start: DateTime<Utc>| ExpandedEvent {
            uid: uid.clone(),
            summary: summary.clone(),
            location: location.clone(),
            description: description.clone(),
            start_utc: start.timestamp(),
            end_utc: (start + duration).timestamp(),
            all_day,
            cancelled,
        };

        // Un événement isolé est traité ici plutôt que confié au moteur de
        // récurrence : celui-ci ignore notre fuseau d'affichage, et se
        // tromperait donc sur les journées entières et les heures flottantes.
        let recurring = event.property_value("RRULE").is_some()
            || event.multi_properties().contains_key("RDATE");
        if !recurring {
            if start_dt >= from - duration && start_dt < to {
                out.occurrences.push(make(start_dt));
            }
            continue;
        }

        let set = match event.get_recurrence() {
            Ok(set) => set,
            Err(e) => {
                out.skipped
                    .push(format!("« {summary} » ignoré : récurrence illisible ({e})"));
                continue;
            }
        };

        // Une série en heure flottante est développée par le moteur dans le
        // fuseau du système ; on ne garde que l'heure murale, réancrée dans le
        // fuseau d'affichage, ce que demande la RFC 5545 §3.3.5.
        let floating = matches!(
            start,
            DatePerhapsTime::DateTime(CalendarDateTime::Floating(_))
        );

        // Une occurrence commencée avant la fenêtre mais qui déborde dedans doit
        // rester visible : on recule la borne basse de la durée de l'événement.
        let search_from = from - duration;
        let dates = set
            .after(search_from.with_timezone(&RTz::UTC))
            .before(to.with_timezone(&RTz::UTC))
            .all(MAX_OCCURRENCES_PER_EVENT);

        if dates.limited {
            out.skipped.push(format!(
                "« {summary} » tronqué à {MAX_OCCURRENCES_PER_EVENT} occurrences"
            ));
        }

        for date in dates.dates {
            let start_utc = if floating {
                match default_tz.from_local_datetime(&date.naive_local()).single() {
                    Some(dt) => dt.with_timezone(&Utc),
                    None => continue,
                }
            } else {
                date.with_timezone(&Utc)
            };
            out.occurrences.push(make(start_utc));
        }
    }

    apply_overrides(&mut out.occurrences, overrides);
    out.occurrences
        .sort_by_key(|o| (o.start_utc, o.uid.clone()));
    Ok(out)
}

/// Substitue les occurrences visées par un `RECURRENCE-ID`.
fn apply_overrides(
    occurrences: &mut Vec<ExpandedEvent>,
    overrides: Vec<(String, i64, Option<ExpandedEvent>)>,
) {
    for (uid, target_start, replacement) in overrides {
        occurrences.retain(|o| !(o.uid == uid && o.start_utc == target_start));
        if let Some(replacement) = replacement {
            occurrences.push(replacement);
        }
    }
}

/// Convertit une date iCalendar en instant UTC.
///
/// Une date flottante — sans `Z` ni `TZID` — est rattachée au fuseau
/// d'affichage, conformément à la RFC 5545 §3.3.5.
fn to_instant(value: &DatePerhapsTime, default_tz: chrono_tz::Tz) -> Option<DateTime<Utc>> {
    match value {
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(naive)) => default_tz
            .from_local_datetime(naive)
            .single()
            .map(|dt| dt.with_timezone(&Utc)),
        DatePerhapsTime::DateTime(other) => other.try_into_utc(),
        DatePerhapsTime::Date(date) => naive_date_to_utc(*date),
    }
}

/// Une journée entière est ancrée à minuit UTC : sa date est alors indépendante
/// du fuseau d'affichage, ce qui évite qu'elle change de jour en voyageant.
fn naive_date_to_utc(date: NaiveDate) -> Option<DateTime<Utc>> {
    date.and_hms_opt(0, 0, 0)
        .map(|naive| Utc.from_utc_datetime(&naive))
}

fn utc_from_timestamp(ts: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(ts, 0)
        .ok_or_else(|| TimewrapError::Parse(format!("horodatage hors limites : {ts}")))
}
