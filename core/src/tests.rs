//! Tests du cœur, adossés à un export d'ENT anonymisé.
//!
//! Le fixture reproduit ce qu'on trouve réellement dans un export ADE : une
//! série hebdomadaire traversant le changement d'heure, une séance retirée par
//! `EXDATE`, une séance déplacée par `RECURRENCE-ID`, un événement sans heure
//! de fin, une journée entière, une séance annulée, une heure flottante et une
//! ligne repliée. Chacun de ces cas a déjà cassé un lecteur d'iCalendar.

use chrono::{TimeZone, Utc};
use chrono_tz::Europe::Paris;

use crate::ics;
use crate::model::CalendarKind;
use crate::store::Store;

const FIXTURE: &str = include_str!("../tests/fixtures/ent_ade.ics");

/// Un instant, exprimé à l'heure de Paris.
fn paris(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Paris
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .single()
        .expect("heure locale ambiguë")
        .timestamp()
}

fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0)
        .unwrap()
        .timestamp()
}

fn window() -> (i64, i64) {
    (utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0))
}

fn expand_fixture() -> ics::Expansion {
    let (from, to) = window();
    ics::expand(FIXTURE, from, to, Paris).expect("le fixture doit être lisible")
}

#[test]
fn lit_tous_les_evenements_du_fixture() {
    let result = expand_fixture();
    assert_eq!(result.events, 7, "7 VEVENT dans le fixture");
    assert!(
        result.skipped.is_empty(),
        "aucun composant ne devrait être écarté : {:?}",
        result.skipped
    );
    // 11 séances de DEV WEB (12 moins l'EXDATE), 15 de BDD, et 4 événements isolés.
    assert_eq!(result.occurrences.len(), 30);
}

#[test]
fn exdate_retire_la_seance_visee() {
    let result = expand_fixture();
    let exclue = paris(2026, 10, 12, 8, 0);
    assert!(
        !result.occurrences.iter().any(|o| o.start_utc == exclue),
        "la séance du 12 octobre est retirée par EXDATE"
    );
}

#[test]
fn le_changement_dheure_preserve_lheure_locale() {
    let result = expand_fixture();
    let starts: Vec<i64> = result
        .occurrences
        .iter()
        .filter(|o| o.uid.starts_with("ADE-0001"))
        .map(|o| o.start_utc)
        .collect();

    // Le 19 octobre est encore en heure d'été : 08h00 locales valent 06h00 UTC.
    assert!(starts.contains(&utc(2026, 10, 19, 6, 0)));
    // Le 26 octobre est passé en heure d'hiver : les mêmes 08h00 valent 07h00 UTC.
    assert!(starts.contains(&utc(2026, 10, 26, 7, 0)));
}

#[test]
fn recurrence_id_remplace_la_seance() {
    let result = expand_fixture();
    let deplacee = result
        .occurrences
        .iter()
        .find(|o| o.start_utc == paris(2026, 11, 2, 14, 0))
        .expect("la séance déplacée doit exister à 14h00");
    assert_eq!(deplacee.location, "SALLE C204");

    assert!(
        !result
            .occurrences
            .iter()
            .any(|o| o.uid.starts_with("ADE-0001") && o.start_utc == paris(2026, 11, 2, 8, 0)),
        "l'occurrence d'origine du 2 novembre est remplacée, pas dupliquée"
    );
}

#[test]
fn un_evenement_sans_heure_de_fin_dure_une_heure() {
    let result = expand_fixture();
    let reunion = result
        .occurrences
        .iter()
        .find(|o| o.summary == "Reunion de rentree")
        .expect("la réunion de rentrée doit être lue");
    assert_eq!(reunion.end_utc - reunion.start_utc, 3600);
}

#[test]
fn une_journee_entiere_est_ancree_a_minuit_utc() {
    let result = expand_fixture();
    let journee = result
        .occurrences
        .iter()
        .find(|o| o.all_day)
        .expect("la journée d'intégration doit être lue");
    assert_eq!(journee.start_utc, utc(2026, 9, 11, 0, 0));
    assert_eq!(journee.end_utc - journee.start_utc, 86_400);
}

#[test]
fn une_seance_annulee_est_marquee_sans_disparaitre() {
    let result = expand_fixture();
    let annulee = result
        .occurrences
        .iter()
        .find(|o| o.summary.contains("RESEAUX"))
        .expect("la séance annulée reste visible, marquée");
    assert!(annulee.cancelled);
}

#[test]
fn une_heure_flottante_suit_le_fuseau_daffichage() {
    let result = expand_fixture();
    let permanence = result
        .occurrences
        .iter()
        .find(|o| o.summary == "Permanence tutorat")
        .expect("la permanence doit être lue");
    // 11h00 sans fuseau, lues comme 11h00 à Paris, donc 09h00 UTC en septembre.
    assert_eq!(permanence.start_utc, utc(2026, 9, 14, 9, 0));
}

#[test]
fn la_ligne_repliee_est_recollee() {
    let result = expand_fixture();
    let cm = result
        .occurrences
        .iter()
        .find(|o| o.summary.contains("DEV WEB"))
        .expect("le CM doit être lu");
    assert!(
        cm.description.contains("Cours magistral"),
        "la ligne repliée doit être recollée, obtenu : {:?}",
        cm.description
    );
}

#[test]
fn crlf_et_lf_donnent_le_meme_resultat() {
    let (from, to) = window();
    let lf = ics::expand(&FIXTURE.replace("\r\n", "\n"), from, to, Paris).unwrap();
    let crlf = ics::expand(
        &FIXTURE.replace("\r\n", "\n").replace('\n', "\r\n"),
        from,
        to,
        Paris,
    )
    .unwrap();
    assert_eq!(lf.occurrences.len(), crlf.occurrences.len());
    assert_eq!(
        lf.occurrences.first().map(|o| o.start_utc),
        crlf.occurrences.first().map(|o| o.start_utc)
    );
}

// ---------------------------------------------------------------- stockage

fn store_with_fixture() -> (Store, String) {
    let store = Store::open(":memory:", "Europe/Paris").expect("base en mémoire");
    let report = store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            utc(2026, 9, 15, 12, 0),
        )
        .expect("import");
    (store, report.calendar_id)
}

#[test]
fn import_puis_reimport_laisse_le_meme_etat() {
    let (store, calendar_id) = store_with_fixture();
    let premier = store
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0))
        .unwrap();

    store
        .reimport_ics(&calendar_id, FIXTURE, utc(2026, 9, 15, 12, 0))
        .expect("réimport");
    let second = store
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0))
        .unwrap();

    assert_eq!(premier.len(), second.len());
    assert_eq!(
        premier.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        second.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        "les identifiants d'occurrence doivent survivre à un réimport"
    );
}

#[test]
fn la_vue_jour_regroupe_selon_le_fuseau_local() {
    let (store, _) = store_with_fixture();
    // Lundi 21 septembre 2026.
    let epoch_day = 20_717;
    let jour = store.day(epoch_day).unwrap();
    assert_eq!(jour.epoch_day, epoch_day);
    assert!(
        jour.occurrences.iter().any(|o| o.title.contains("DEV WEB")),
        "le CM du lundi doit apparaître, obtenu : {:?}",
        jour.occurrences
            .iter()
            .map(|o| &o.title)
            .collect::<Vec<_>>()
    );
}

#[test]
fn un_agenda_masque_disparait_des_requetes() {
    let (store, calendar_id) = store_with_fixture();
    store.set_visible(&calendar_id, false).unwrap();
    let visibles = store
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0))
        .unwrap();
    assert!(visibles.is_empty());
}

#[test]
fn la_vue_maintenant_designe_le_cours_en_cours() {
    let (store, _) = store_with_fixture();
    // Lundi 21 septembre 2026, 08h30 à Paris : en plein CM de 08h00 à 10h00.
    let now = paris(2026, 9, 21, 8, 30);
    let vue = store.now_view(now).unwrap();

    let current = vue.current.expect("un cours est en cours");
    assert!(current.title.contains("DEV WEB"));
    assert_eq!(vue.minutes_remaining, Some(90));

    let next = vue.next.expect("un prochain cours existe");
    assert!(next.start_utc > now);
}

#[test]
fn supprimer_un_agenda_emporte_ses_occurrences() {
    let (store, calendar_id) = store_with_fixture();
    store.delete_calendar(&calendar_id).unwrap();
    assert!(store.calendars().unwrap().is_empty());
    assert!(
        store
            .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0))
            .unwrap()
            .is_empty()
    );
}
