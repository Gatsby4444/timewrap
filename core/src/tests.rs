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
use crate::model::{
    CalendarKind, ConflictScope, EventDraft, EventOrigin, Resolution, Rule, RuleField, RuleMatch,
};
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
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0), &None)
        .unwrap();

    store
        .reimport_ics(&calendar_id, FIXTURE, utc(2026, 9, 15, 12, 0))
        .expect("réimport");
    let second = store
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0), &None)
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
    let jour = store.day(epoch_day, &None).unwrap();
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
        .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0), &None)
        .unwrap();
    assert!(visibles.is_empty());
}

#[test]
fn la_vue_maintenant_designe_le_cours_en_cours() {
    let (store, _) = store_with_fixture();
    // Lundi 21 septembre 2026, 08h30 à Paris : en plein CM de 08h00 à 10h00.
    let now = paris(2026, 9, 21, 8, 30);
    let vue = store.now_view(now, &None).unwrap();

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
            .occurrences_between(utc(2026, 8, 1, 0, 0), utc(2027, 1, 1, 0, 0), &None)
            .unwrap()
            .is_empty()
    );
}

// ------------------------------------------------- agendas comme des dossiers

/// Un jeu de départ : l'emploi du temps importé, plus un agenda personnel vide.
fn store_with_folders() -> (Store, String, String) {
    let (store, ecole) = store_with_fixture();
    let perso = store
        .create_calendar(
            "Perso",
            CalendarKind::Local,
            "",
            None,
            utc(2026, 9, 15, 12, 0),
        )
        .expect("agenda local")
        .id;
    (store, ecole, perso)
}

/// Un brouillon d'événement, pour ne pas répéter dix champs par test.
fn draft(calendar_id: &str, title: &str, start: i64, end: i64) -> EventDraft {
    EventDraft {
        id: None,
        calendar_id: calendar_id.to_string(),
        title: title.to_string(),
        location: String::new(),
        description: String::new(),
        start_utc: start,
        end_utc: end,
        all_day: false,
        category_id: None,
    }
}

#[test]
fn un_agenda_local_se_cree_et_se_remplit() {
    let (store, _, perso) = store_with_folders();
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Piscine",
                paris(2026, 9, 22, 18, 0),
                paris(2026, 9, 22, 19, 0),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .expect("écriture");
    let saved = outcome.saved.expect("l'événement est écrit");
    assert_eq!(saved.title, "Piscine");
    assert_eq!(saved.origin, EventOrigin::Local);
    assert!(!outcome.blocked);
}

#[test]
fn la_portee_isole_un_agenda_des_autres() {
    let (store, ecole, perso) = store_with_folders();
    store
        .save_event(
            &draft(
                &perso,
                "Piscine",
                paris(2026, 9, 22, 18, 0),
                paris(2026, 9, 22, 19, 0),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    let seul_perso = store
        .occurrences_between(
            utc(2026, 9, 1, 0, 0),
            utc(2026, 10, 1, 0, 0),
            &Some(vec![perso.clone()]),
        )
        .unwrap();
    assert_eq!(seul_perso.len(), 1);
    assert_eq!(seul_perso[0].title, "Piscine");

    let seule_ecole = store
        .occurrences_between(
            utc(2026, 9, 1, 0, 0),
            utc(2026, 10, 1, 0, 0),
            &Some(vec![ecole]),
        )
        .unwrap();
    assert!(seule_ecole.iter().all(|o| o.title != "Piscine"));
}

#[test]
fn un_agenda_masque_reste_consultable_quand_on_louvre() {
    let (store, _, perso) = store_with_folders();
    store
        .save_event(
            &draft(
                &perso,
                "Piscine",
                paris(2026, 9, 22, 18, 0),
                paris(2026, 9, 22, 19, 0),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();
    store.set_visible(&perso, false).unwrap();

    // Absent de la vue d'ensemble...
    let ensemble = store
        .occurrences_between(utc(2026, 9, 1, 0, 0), utc(2026, 10, 1, 0, 0), &None)
        .unwrap();
    assert!(ensemble.iter().all(|o| o.title != "Piscine"));

    // ...mais présent quand on ouvre le dossier lui-même.
    let ouvert = store
        .occurrences_between(
            utc(2026, 9, 1, 0, 0),
            utc(2026, 10, 1, 0, 0),
            &Some(vec![perso]),
        )
        .unwrap();
    assert_eq!(ouvert.len(), 1);
}

#[test]
fn laccueil_resume_chaque_agenda() {
    let (store, _, perso) = store_with_folders();
    store
        .save_event(
            &draft(
                &perso,
                "Piscine",
                paris(2026, 9, 22, 18, 0),
                paris(2026, 9, 22, 19, 0),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    let resumes = store.calendar_summaries(paris(2026, 9, 21, 7, 0)).unwrap();
    assert_eq!(resumes.len(), 2);

    let piscine = resumes
        .iter()
        .find(|r| r.calendar.id == perso)
        .expect("l'agenda perso est résumé");
    assert_eq!(piscine.upcoming_week, 1);
    assert_eq!(
        piscine.next.as_ref().map(|o| o.title.as_str()),
        Some("Piscine")
    );
    assert_eq!(piscine.conflicts, 0);
}

// ------------------------------------------------------- moteur de conflits

#[test]
fn un_chevauchement_bloque_lecriture_et_sexplique() {
    let (store, _, perso) = store_with_folders();
    // Lundi 21 septembre, 08h00-10h00 : le CM de DEV WEB occupe déjà le créneau.
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    assert!(outcome.blocked, "rien ne doit être écrit sans arbitrage");
    assert!(outcome.saved.is_none());
    assert_eq!(outcome.conflicts.len(), 1);

    let conflit = &outcome.conflicts[0];
    assert_eq!(conflit.scope, ConflictScope::CrossCalendar);
    assert_eq!(conflit.overlap_minutes, 60);
    assert!(
        !conflit.other_deletable,
        "une séance importée ne se supprime pas"
    );
}

#[test]
fn deux_creneaux_du_meme_agenda_se_signalent_comme_tels() {
    let (store, _, perso) = store_with_folders();
    let now = utc(2026, 9, 15, 12, 0);
    store
        .save_event(
            &draft(
                &perso,
                "Sport",
                paris(2026, 9, 26, 10, 0),
                paris(2026, 9, 26, 12, 0),
            ),
            Resolution::Cancel,
            now,
        )
        .unwrap();

    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Courses",
                paris(2026, 9, 26, 11, 0),
                paris(2026, 9, 26, 11, 30),
            ),
            Resolution::Cancel,
            now,
        )
        .unwrap();

    assert!(outcome.blocked);
    assert_eq!(outcome.conflicts[0].scope, ConflictScope::SameCalendar);
    assert!(
        outcome.conflicts[0].other_deletable,
        "une séance locale, elle, peut être supprimée"
    );
}

#[test]
fn ignorer_ecrit_malgre_le_chevauchement() {
    let (store, _, perso) = store_with_folders();
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Ignore,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    assert!(!outcome.blocked);
    assert!(outcome.saved.is_some());
    assert_eq!(outcome.conflicts.len(), 1, "le conflit reste signalé");
}

#[test]
fn remplacer_libere_le_creneau() {
    let (store, _, perso) = store_with_folders();
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Replace,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    assert!(outcome.saved.is_some());
    assert_eq!(outcome.removed, 0, "rien de local à supprimer");
    assert_eq!(outcome.hidden, 1, "la séance importée est masquée");

    let lundi = store.day(20_717, &None).unwrap();
    assert!(
        lundi
            .occurrences
            .iter()
            .all(|o| !o.title.contains("DEV WEB")),
        "le CM masqué ne doit plus apparaître"
    );
    assert!(lundi.occurrences.iter().any(|o| o.title == "Dentiste"));
}

#[test]
fn remplacer_supprime_ce_qui_est_local() {
    let (store, _, perso) = store_with_folders();
    let now = utc(2026, 9, 15, 12, 0);
    store
        .save_event(
            &draft(
                &perso,
                "Sport",
                paris(2026, 9, 26, 10, 0),
                paris(2026, 9, 26, 12, 0),
            ),
            Resolution::Cancel,
            now,
        )
        .unwrap();

    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Courses",
                paris(2026, 9, 26, 11, 0),
                paris(2026, 9, 26, 11, 30),
            ),
            Resolution::Replace,
            now,
        )
        .unwrap();

    assert_eq!(outcome.removed, 1);
    assert_eq!(outcome.hidden, 0);
}

#[test]
fn decaler_pousse_levenement_apres_le_conflit() {
    let (store, _, perso) = store_with_folders();
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 0),
            ),
            Resolution::ShiftAfter,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    let saved = outcome.saved.expect("l'événement décalé est écrit");
    // Le CM finit à 10h00 : le rendez-vous d'une heure commence donc à 10h00.
    assert_eq!(saved.start_utc, paris(2026, 9, 21, 10, 0));
    assert_eq!(saved.end_utc - saved.start_utc, 3600);
    assert_eq!(outcome.shifted_minutes, 60);
}

#[test]
fn le_gestionnaire_liste_les_chevauchements_existants() {
    let (store, _, perso) = store_with_folders();
    store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Ignore,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    let paires = store
        .conflicts_between(paris(2026, 9, 21, 0, 0), paris(2026, 9, 22, 0, 0), &None)
        .unwrap();
    assert_eq!(paires.len(), 1);
    assert_eq!(paires[0].scope, ConflictScope::CrossCalendar);
    assert_eq!(paires[0].overlap_minutes, 60);
}

#[test]
fn deux_creneaux_qui_senchainent_ne_sont_pas_en_conflit() {
    let (store, _, perso) = store_with_folders();
    // 10h00-11h00, juste après le CM qui finit à 10h00.
    let outcome = store
        .save_event(
            &draft(
                &perso,
                "Café",
                paris(2026, 9, 21, 10, 0),
                paris(2026, 9, 21, 11, 0),
            ),
            Resolution::Cancel,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();
    assert!(!outcome.blocked);
    assert!(outcome.conflicts.is_empty());
}

#[test]
fn deplacer_un_evenement_ne_le_heurte_pas_lui_meme() {
    let (store, _, perso) = store_with_folders();
    let now = utc(2026, 9, 15, 12, 0);
    let saved = store
        .save_event(
            &draft(
                &perso,
                "Sport",
                paris(2026, 9, 26, 10, 0),
                paris(2026, 9, 26, 12, 0),
            ),
            Resolution::Cancel,
            now,
        )
        .unwrap()
        .saved
        .unwrap();

    let mut modifie = draft(
        &perso,
        "Sport",
        paris(2026, 9, 26, 10, 30),
        paris(2026, 9, 26, 12, 30),
    );
    modifie.id = Some(saved.id.clone());
    let outcome = store.save_event(&modifie, Resolution::Cancel, now).unwrap();

    assert!(
        !outcome.blocked,
        "un événement ne se heurte pas à sa propre version"
    );
    assert_eq!(
        outcome.saved.map(|o| o.start_utc),
        Some(paris(2026, 9, 26, 10, 30))
    );
}

#[test]
fn une_seance_importee_ne_se_supprime_pas() {
    let (store, _) = store_with_fixture();
    let lundi = store.day(20_717, &None).unwrap();
    let importee = &lundi.occurrences[0];
    assert!(store.delete_event(&importee.id).is_err());

    // Elle se masque, en revanche, et se réaffiche.
    store.set_muted(&importee.id, true).unwrap();
    assert!(store.day(20_717, &None).unwrap().occurrences.is_empty());
    store.set_muted(&importee.id, false).unwrap();
    assert_eq!(store.day(20_717, &None).unwrap().occurrences.len(), 1);
}

#[test]
fn une_seance_masquee_le_reste_apres_reimport() {
    let (store, calendar_id) = store_with_fixture();
    let importee = store.day(20_717, &None).unwrap().occurrences[0].clone();
    store.set_muted(&importee.id, true).unwrap();

    store
        .reimport_ics(&calendar_id, FIXTURE, utc(2026, 9, 15, 12, 0))
        .unwrap();

    assert!(
        store.day(20_717, &None).unwrap().occurrences.is_empty(),
        "le masquage doit survivre au réimport"
    );
}

// ------------------------------------------------ moteur de règles visuelles

/// Une règle nue, que chaque test précise ensuite.
fn rule(pattern: &str) -> Rule {
    Rule {
        id: String::new(),
        name: String::new(),
        calendar_id: None,
        field: RuleField::Title,
        match_kind: RuleMatch::Word,
        pattern: pattern.to_string(),
        case_sensitive: false,
        category_id: None,
        rename_to: None,
        hide: false,
        priority: 0,
        enabled: true,
        match_count: 0,
    }
}

#[test]
fn une_regle_colorie_dun_coup_toutes_les_seances_dun_type() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    let categorie = store
        .create_category("Cours magistral", "CM", Some(0xFF11_2233), now)
        .unwrap();

    let posee = store
        .save_rule(
            &Rule {
                category_id: Some(categorie.id.clone()),
                ..rule("CM")
            },
            now,
        )
        .unwrap();
    assert_eq!(posee.match_count, 11, "les 11 séances de DEV WEB");

    let lundi = store.day(20_717, &None).unwrap();
    let cm = &lundi.occurrences[0];
    assert_eq!(cm.color, 0xFF11_2233, "la couleur vient de la catégorie");
    assert_eq!(cm.category_label, "CM");
    assert_eq!(cm.category_name, "Cours magistral");

    // Le TP de BDD, lui, garde la couleur de son agenda.
    let mardi = store.day(20_718, &None).unwrap();
    assert!(mardi.occurrences.iter().all(|o| o.category_id.is_none()));
}

#[test]
fn retirer_une_regle_rend_leur_apparence_aux_seances() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    let categorie = store
        .create_category("CM", "CM", Some(0xFF11_2233), now)
        .unwrap();
    let posee = store
        .save_rule(
            &Rule {
                category_id: Some(categorie.id),
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    store.delete_rule(&posee.id).unwrap();
    let lundi = store.day(20_717, &None).unwrap();
    assert!(lundi.occurrences[0].category_id.is_none());
    assert_ne!(lundi.occurrences[0].color, 0xFF11_2233);
}

#[test]
fn une_regle_renomme_sans_perdre_lintitule_dorigine() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    store
        .save_rule(
            &Rule {
                rename_to: Some("★ {}".to_string()),
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    let cm = &store.day(20_717, &None).unwrap().occurrences[0];
    assert!(cm.title.starts_with("★ "), "titre obtenu : {}", cm.title);
    assert_eq!(
        cm.raw_title, "R3.01 DEV WEB - CM (Gr A)",
        "l'intitulé de l'ENT reste disponible"
    );
}

#[test]
fn une_regle_de_masquage_retire_les_seances_des_vues() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    store
        .save_rule(
            &Rule {
                hide: true,
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    assert!(store.day(20_717, &None).unwrap().occurrences.is_empty());

    // Mais les données restent, et la règle désactivée les ramène.
    let regle = store.rules().unwrap().remove(0);
    store
        .save_rule(
            &Rule {
                enabled: false,
                ..regle
            },
            now,
        )
        .unwrap();
    assert_eq!(store.day(20_717, &None).unwrap().occurrences.len(), 1);
}

#[test]
fn le_mot_entier_ne_saccroche_pas_a_un_autre_mot() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    // « TD » ne doit pas reconnaître « BDD ».
    let posee = store.save_rule(&rule("TD"), now).unwrap();
    assert_eq!(posee.match_count, 1, "seule la séance de RESEAUX est un TD");
}

#[test]
fn une_categorie_posee_a_la_main_resiste_aux_regles() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    let choisie = store
        .create_category("Important", "!", Some(0xFF00_FF00), now)
        .unwrap();
    let auto = store
        .create_category("CM", "CM", Some(0xFF11_2233), now)
        .unwrap();

    let cm = store.day(20_717, &None).unwrap().occurrences[0].clone();
    store
        .set_occurrence_category(&cm.id, Some(choisie.id.clone()))
        .unwrap();
    store
        .save_rule(
            &Rule {
                category_id: Some(auto.id),
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    let apres = store.day(20_717, &None).unwrap().occurrences[0].clone();
    assert_eq!(apres.category_id, Some(choisie.id));
    assert_eq!(apres.color, 0xFF00_FF00);
}

#[test]
fn une_regle_sapplique_aussi_a_ce_qui_arrive_apres() {
    let (store, ecole, _) = store_with_folders();
    let now = utc(2026, 9, 15, 12, 0);
    let categorie = store
        .create_category("CM", "CM", Some(0xFF11_2233), now)
        .unwrap();
    store
        .save_rule(
            &Rule {
                category_id: Some(categorie.id),
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    store.reimport_ics(&ecole, FIXTURE, now).unwrap();
    let cm = &store.day(20_717, &None).unwrap().occurrences[0];
    assert_eq!(
        cm.color, 0xFF11_2233,
        "un réimport ne doit pas décolorier l'emploi du temps"
    );
}

#[test]
fn les_suggestions_reperent_les_types_de_cours() {
    let (store, _) = store_with_fixture();
    let suggestions = store.rule_suggestions(&None).unwrap();
    let motifs: Vec<&str> = suggestions.iter().map(|s| s.pattern.as_str()).collect();

    assert!(motifs.contains(&"cm"), "obtenu : {motifs:?}");
    assert!(motifs.contains(&"tp"), "obtenu : {motifs:?}");
    // Les marqueurs de type passent devant les noms de matière.
    let rang_cm = motifs.iter().position(|m| *m == "cm").unwrap();
    let rang_bdd = motifs.iter().position(|m| *m == "bdd").unwrap();
    assert!(rang_cm < rang_bdd);

    let cm = suggestions.iter().find(|s| s.pattern == "cm").unwrap();
    assert_eq!(cm.occurrences, 11);
    assert!(!cm.samples.is_empty());
}

#[test]
fn accepter_une_suggestion_cree_categorie_et_regle() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    let suggestion = store
        .rule_suggestions(&None)
        .unwrap()
        .into_iter()
        .find(|s| s.pattern == "cm")
        .unwrap();

    let regle = store
        .accept_suggestion(&suggestion, "Cours magistral", "CM", None, now)
        .unwrap();
    assert_eq!(regle.match_count, 11);
    assert_eq!(store.categories().unwrap().len(), 1);

    let cm = &store.day(20_717, &None).unwrap().occurrences[0];
    assert_eq!(cm.category_label, "CM");

    // Une fois la règle posée, le motif ne doit plus être proposé.
    let restantes = store.rule_suggestions(&None).unwrap();
    assert!(restantes.iter().all(|s| s.pattern != "cm"));
}

#[test]
fn supprimer_une_categorie_ne_perd_pas_les_seances() {
    let (store, _) = store_with_fixture();
    let now = utc(2026, 9, 15, 12, 0);
    let categorie = store
        .create_category("CM", "CM", Some(0xFF11_2233), now)
        .unwrap();
    store
        .save_rule(
            &Rule {
                category_id: Some(categorie.id.clone()),
                ..rule("CM")
            },
            now,
        )
        .unwrap();

    store.delete_category(&categorie.id).unwrap();
    let lundi = store.day(20_717, &None).unwrap();
    assert_eq!(lundi.occurrences.len(), 1);
    assert!(lundi.occurrences[0].category_id.is_none());
}

#[test]
fn une_seance_masquee_reste_rappelable() {
    let (store, _, perso) = store_with_folders();
    store
        .save_event(
            &draft(
                &perso,
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Replace,
            utc(2026, 9, 15, 12, 0),
        )
        .unwrap();

    let masquees = store
        .muted_between(paris(2026, 9, 21, 0, 0), paris(2026, 9, 22, 0, 0))
        .unwrap();
    assert_eq!(masquees.len(), 1);
    assert!(masquees[0].title.contains("DEV WEB"));

    store.set_muted(&masquees[0].id, false).unwrap();
    assert!(
        store
            .day(20_717, &None)
            .unwrap()
            .occurrences
            .iter()
            .any(|o| o.title.contains("DEV WEB"))
    );
}
