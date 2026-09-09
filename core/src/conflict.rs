//! Le moteur de chevauchements.
//!
//! Un emploi du temps qui mêle plusieurs agendas finit toujours par se
//! contredire : un rendez-vous personnel posé sur un TP, deux créneaux importés
//! qui se recouvrent après un changement de salle. Le cœur ne tranche jamais
//! seul — il constate, chiffre le recouvrement, et propose. La décision reste à
//! l'utilisateur, et c'est `Resolution` qui la transporte.
//!
//! Comme le moteur de règles, ce module ne connaît pas la base : il raisonne
//! sur des occurrences déjà chargées.

use crate::model::{Conflict, ConflictPair, ConflictScope, EventOrigin, Occurrence};

/// Recouvrement de deux intervalles, en secondes. Nul si les créneaux se
/// touchent sans se chevaucher — finir à 10:00 et commencer à 10:00 n'est pas
/// un conflit, c'est un enchaînement.
pub fn overlap_seconds(a_start: i64, a_end: i64, b_start: i64, b_end: i64) -> i64 {
    (a_end.min(b_end) - a_start.max(b_start)).max(0)
}

/// Confronte un projet d'événement à ce qui existe déjà.
///
/// `candidates` doit contenir les occurrences de la fenêtre visée, l'événement
/// lui-même compris s'il s'agit d'une modification : il est écarté ici, sans
/// quoi tout déplacement se heurterait à sa propre version précédente.
pub fn against_draft(
    draft_id: Option<&str>,
    draft_calendar: &str,
    start_utc: i64,
    end_utc: i64,
    candidates: &[Occurrence],
) -> Vec<Conflict> {
    let mut conflicts: Vec<Conflict> = candidates
        .iter()
        .filter(|other| Some(other.id.as_str()) != draft_id)
        .filter(|other| !other.all_day && !other.cancelled && !other.hidden)
        .filter_map(|other| {
            let overlap = overlap_seconds(start_utc, end_utc, other.start_utc, other.end_utc);
            if overlap == 0 {
                return None;
            }
            Some(Conflict {
                scope: scope_of(draft_calendar, &other.calendar_id),
                overlap_minutes: (overlap / 60).max(1),
                other_deletable: other.origin == EventOrigin::Local,
                other: other.clone(),
            })
        })
        .collect();

    // Le même agenda d'abord : c'est le heurt le plus gênant, celui qu'on veut
    // lire en premier dans la boîte de dialogue.
    conflicts.sort_by(|a, b| {
        a.scope
            .cmp_priority(b.scope)
            .then(a.other.start_utc.cmp(&b.other.start_utc))
    });
    conflicts
}

/// Tous les chevauchements d'un ensemble d'occurrences, deux à deux.
///
/// Balayage par ordre de début : une occurrence n'est comparée qu'à celles qui
/// commencent avant qu'elle ne finisse, ce qui évite le carré du nombre de
/// séances sur une année entière.
pub fn pairs(occurrences: &[Occurrence]) -> Vec<ConflictPair> {
    let mut sorted: Vec<&Occurrence> = occurrences
        .iter()
        .filter(|o| !o.all_day && !o.cancelled && !o.hidden)
        .collect();
    sorted.sort_by_key(|o| (o.start_utc, o.end_utc));

    let mut out = Vec::new();
    for (index, first) in sorted.iter().enumerate() {
        for second in sorted.iter().skip(index + 1) {
            if second.start_utc >= first.end_utc {
                break;
            }
            let overlap = overlap_seconds(
                first.start_utc,
                first.end_utc,
                second.start_utc,
                second.end_utc,
            );
            if overlap == 0 {
                continue;
            }
            out.push(ConflictPair {
                scope: scope_of(&first.calendar_id, &second.calendar_id),
                first: (*first).clone(),
                second: (*second).clone(),
                overlap_minutes: (overlap / 60).max(1),
            });
        }
    }
    out
}

/// Instant auquel décaler un événement pour qu'il passe après tous ses
/// conflits, sa durée conservée.
pub fn shift_after(conflicts: &[Conflict]) -> Option<i64> {
    conflicts.iter().map(|c| c.other.end_utc).max()
}

fn scope_of(a: &str, b: &str) -> ConflictScope {
    if a == b {
        ConflictScope::SameCalendar
    } else {
        ConflictScope::CrossCalendar
    }
}

impl ConflictScope {
    fn cmp_priority(self, other: Self) -> std::cmp::Ordering {
        fn rank(scope: ConflictScope) -> u8 {
            match scope {
                ConflictScope::SameCalendar => 0,
                ConflictScope::CrossCalendar => 1,
            }
        }
        rank(self).cmp(&rank(other))
    }
}
