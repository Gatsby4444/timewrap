//! Tests du cœur, adossés à un export d'ENT anonymisé.
//!
//! Le fixture reproduit ce qu'on trouve réellement dans un export ADE : une
//! série hebdomadaire traversant le changement d'heure, une séance retirée par
//! `EXDATE`, une séance déplacée par `RECURRENCE-ID`, un événement sans heure
//! de fin, une journée entière, une séance annulée, une heure flottante, une
//! ligne repliée, et des descriptions à champs — « Type : … », « Matière : … ».
//! Chacun de ces cas a déjà cassé un lecteur d'iCalendar.

use chrono::{TimeZone, Utc};
use chrono_tz::Europe::Paris;

use crate::ics;
use crate::model::{CalendarKind, EventDraft, EventOrigin, Resolution, Rule, RuleField, RuleMatch};
use crate::properties;
use crate::store::Store;

const FIXTURE: &str = include_str!("../tests/fixtures/ent_ade.ics");

/// Lundi 21 septembre 2026, en jours depuis l'époque.
const LUNDI: i64 = 20_717;
/// Mardi 22 septembre 2026.
const MARDI: i64 = 20_718;

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

/// L'instant de référence de tous les tests de stockage.
fn now() -> i64 {
    utc(2026, 9, 15, 12, 0)
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

// ------------------------------------------------------- champs structurés

#[test]
fn les_champs_de_la_description_sont_lus() {
    let props =
        properties::parse("Intervenant : MARTIN Camille\nType : Cours magistral\nSalle : B");
    assert_eq!(props.len(), 3);
    assert_eq!(props[1].key, "type");
    assert_eq!(props[1].label, "Type");
    assert_eq!(props[1].value, "Cours magistral");
}

#[test]
fn une_cle_accentuee_se_compare_sans_accent() {
    let props = properties::parse("Matière : Analyse");
    assert_eq!(props[0].key, "matiere", "la clé sert à comparer");
    assert_eq!(props[0].label, "Matière", "l'étiquette sert à afficher");
}

#[test]
fn une_ligne_sans_deux_points_nest_pas_un_champ() {
    let props = properties::parse("Seance deplacee\nType : TD\nvoir https://exemple.fr/x");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].key, "type");
}

#[test]
fn une_cle_repetee_garde_sa_premiere_valeur() {
    let props = properties::parse("Salle : A\nSalle : B");
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].value, "A");
}

// ---------------------------------------------------------------- stockage

fn store_with_fixture() -> Store {
    let store = Store::open(":memory:", "Europe/Paris").expect("base en mémoire");
    store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            now(),
        )
        .expect("import");
    store
}

/// Un brouillon d'événement, pour ne pas répéter huit champs par test.
fn draft(title: &str, start: i64, end: i64) -> EventDraft {
    EventDraft {
        id: None,
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
fn import_puis_reimport_laisse_le_meme_etat() {
    let store = store_with_fixture();
    let premier = store.occurrences_between(window().0, window().1).unwrap();

    store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            now(),
        )
        .expect("réimport");
    let second = store.occurrences_between(window().0, window().1).unwrap();

    assert_eq!(premier.len(), second.len());
    assert_eq!(
        premier.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        second.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        "les identifiants de séance doivent survivre à un réimport"
    );
}

#[test]
fn il_ny_a_jamais_quun_emploi_du_temps() {
    let store = store_with_fixture();
    let premier = store.timetable().unwrap().unwrap();

    store
        .import_ics("Autre", CalendarKind::IcsFile, "autre.ics", FIXTURE, now())
        .unwrap();
    let second = store.timetable().unwrap().unwrap();

    assert_eq!(
        premier.id, second.id,
        "réimporter remplace le contenu, pas l'emploi du temps"
    );
    assert_eq!(second.name, "Autre");
}

#[test]
fn la_vue_jour_regroupe_selon_le_fuseau_local() {
    let store = store_with_fixture();
    let jour = store.day(LUNDI, now()).unwrap();
    assert_eq!(jour.epoch_day, LUNDI);
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
fn la_vue_maintenant_designe_le_cours_en_cours() {
    let store = store_with_fixture();
    // Lundi 21 septembre 2026, 08h30 à Paris : en plein CM de 08h00 à 10h00.
    let instant = paris(2026, 9, 21, 8, 30);
    let vue = store.now_view(instant).unwrap();

    let current = vue.current.expect("un cours est en cours");
    assert!(current.title.contains("DEV WEB"));
    assert_eq!(vue.minutes_remaining, Some(90));
    assert!(vue.next.expect("un prochain cours existe").start_utc > instant);
}

#[test]
fn effacer_lemploi_du_temps_garde_les_choses_a_faire() {
    let store = store_with_fixture();
    store.add_task("Rendre le TP", LUNDI, now()).unwrap();

    store.clear_timetable().unwrap();

    assert!(store.timetable().unwrap().is_none());
    assert!(
        store
            .occurrences_between(window().0, window().1)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.pending_tasks(LUNDI).unwrap().len(), 1);
}

#[test]
fn un_evenement_saisi_survit_a_un_reimport() {
    let store = store_with_fixture();
    let saved = store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 26, 10, 0),
                paris(2026, 9, 26, 11, 0),
            ),
            Resolution::Cancel,
            now(),
        )
        .unwrap()
        .saved
        .expect("écrit");

    store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            now(),
        )
        .unwrap();

    assert_eq!(store.occurrence(&saved.id).unwrap().title, "Dentiste");
    assert_eq!(saved.origin, EventOrigin::Local);
}

// ---------------------------------------------------- couleurs par champ

#[test]
fn les_champs_importes_sont_repertories() {
    let store = store_with_fixture();
    let keys = store.property_keys().unwrap();
    let noms: Vec<&str> = keys.iter().map(|k| k.key.as_str()).collect();

    assert!(noms.contains(&"type"), "obtenu : {noms:?}");
    assert!(noms.contains(&"matiere"), "obtenu : {noms:?}");

    let matiere = keys.iter().find(|k| k.key == "matiere").unwrap();
    assert_eq!(matiere.label, "Matière", "l'étiquette garde son accent");
    assert_eq!(matiere.distinct_values, 2);
}

#[test]
fn les_valeurs_dun_champ_sont_comptees() {
    let store = store_with_fixture();
    let valeurs = store.property_values("type").unwrap();

    let tp = valeurs
        .iter()
        .find(|v| v.value == "Travaux pratiques")
        .expect("le TP de BDD");
    assert_eq!(tp.occurrences, 15);
    assert!(!tp.colored, "aucune couleur posée pour l'instant");

    let cm = valeurs
        .iter()
        .find(|v| v.value == "Cours magistral")
        .expect("le CM de DEV WEB");
    // 11 séances de DEV WEB, mais celle qu'un RECURRENCE-ID a déplacée porte sa
    // propre description — « Seance deplacee » — sans champ Type. Un export
    // d'ENT est comme ça : le remplacement d'une occurrence est réécrit à la
    // main, et perd la moitié de ses champs.
    assert_eq!(cm.occurrences, 10);
}

#[test]
fn colorier_un_type_teint_toutes_ses_seances() {
    let store = store_with_fixture();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();

    let lundi = store.day(LUNDI, now()).unwrap();
    let cm = &lundi.occurrences[0];
    assert_eq!(cm.color, 0xFF11_2233);
    assert_eq!(cm.category_name, "Cours magistral");

    // Le TP du mardi, lui, garde la couleur de l'emploi du temps.
    let mardi = store.day(MARDI, now()).unwrap();
    assert!(mardi.occurrences.iter().all(|o| o.color != 0xFF11_2233));
}

#[test]
fn colorier_une_matiere_teint_toutes_ses_seances() {
    let store = store_with_fixture();
    store
        .set_property_color("Matière", "Bases de donnees", 0xFF44_5566, now())
        .unwrap();

    let mardi = store.day(MARDI, now()).unwrap();
    assert_eq!(mardi.occurrences[0].color, 0xFF44_5566);
    assert_eq!(
        mardi.occurrences[0].category_name, "Bases de donnees",
        "la catégorie porte le nom de la matière"
    );
}

#[test]
fn recolorier_une_valeur_ne_cree_pas_de_doublon() {
    let store = store_with_fixture();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();
    store
        .set_property_color("type", "Cours magistral", 0xFF99_8877, now())
        .unwrap();

    assert_eq!(store.categories().unwrap().len(), 1);
    assert_eq!(store.rules().unwrap().len(), 1);
    assert_eq!(
        store.day(LUNDI, now()).unwrap().occurrences[0].color,
        0xFF99_8877
    );
}

#[test]
fn colorier_dun_coup_donne_une_couleur_a_chaque_valeur() {
    let store = store_with_fixture();
    let posees = store.auto_color_property("matiere", now()).unwrap();
    assert_eq!(posees, 2, "deux matières dans le fixture");

    let valeurs = store.property_values("matiere").unwrap();
    assert!(valeurs.iter().all(|v| v.colored));
    let couleurs: Vec<u32> = valeurs.iter().map(|v| v.color).collect();
    assert_ne!(couleurs[0], couleurs[1], "deux matières, deux couleurs");
}

#[test]
fn retirer_la_couleur_rend_son_apparence_a_la_seance() {
    let store = store_with_fixture();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();
    store
        .clear_property_color("type", "Cours magistral")
        .unwrap();

    assert!(store.categories().unwrap().is_empty());
    assert!(store.rules().unwrap().is_empty());
    let cm = &store.day(LUNDI, now()).unwrap().occurrences[0];
    assert_ne!(cm.color, 0xFF11_2233);
    assert!(cm.category_id.is_none());
}

#[test]
fn une_couleur_par_champ_survit_a_un_reimport() {
    let store = store_with_fixture();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();

    store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            now(),
        )
        .unwrap();

    assert_eq!(
        store.day(LUNDI, now()).unwrap().occurrences[0].color,
        0xFF11_2233,
        "un réimport ne doit pas décolorier l'emploi du temps"
    );
}

#[test]
fn la_fiche_dune_seance_montre_ses_champs() {
    let store = store_with_fixture();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();

    let cm = store.day(LUNDI, now()).unwrap().occurrences[0].clone();
    let champs = store.occurrence_properties(&cm.id).unwrap();

    let type_ = champs.iter().find(|p| p.key == "type").unwrap();
    assert_eq!(type_.value, "Cours magistral");
    assert!(type_.colored);
    assert!(champs.iter().any(|p| p.key == "matiere"));
}

#[test]
fn les_suggestions_partent_des_champs_avant_les_titres() {
    let store = store_with_fixture();
    let suggestions = store.rule_suggestions().unwrap();
    assert!(!suggestions.is_empty());
    assert!(
        suggestions.iter().all(|s| s.field == RuleField::Property),
        "les champs structurés priment sur l'analyse des intitulés"
    );
    assert!(
        suggestions
            .iter()
            .any(|s| s.property.as_deref() == Some("type")),
        "obtenu : {:?}",
        suggestions.iter().map(|s| &s.label).collect::<Vec<_>>()
    );
}

// ----------------------------------------------- règles écrites à la main

fn rule(pattern: &str) -> Rule {
    Rule {
        id: String::new(),
        name: String::new(),
        field: RuleField::Title,
        property: None,
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
fn une_regle_de_titre_compte_ses_seances() {
    let store = store_with_fixture();
    let posee = store.save_rule(&rule("CM"), now()).unwrap();
    assert_eq!(posee.match_count, 11);
}

#[test]
fn le_mot_entier_ne_saccroche_pas_a_un_autre_mot() {
    let store = store_with_fixture();
    // « TD » ne doit pas reconnaître « BDD ».
    let posee = store.save_rule(&rule("TD"), now()).unwrap();
    assert_eq!(posee.match_count, 1, "seule la séance de RESEAUX est un TD");
}

#[test]
fn une_regle_renomme_sans_perdre_lintitule_dorigine() {
    let store = store_with_fixture();
    store
        .save_rule(
            &Rule {
                rename_to: Some("★ {}".to_string()),
                ..rule("CM")
            },
            now(),
        )
        .unwrap();

    let cm = &store.day(LUNDI, now()).unwrap().occurrences[0];
    assert!(cm.title.starts_with("★ "), "titre obtenu : {}", cm.title);
    assert_eq!(cm.raw_title, "R3.01 DEV WEB - CM (Gr A)");
}

#[test]
fn une_regle_de_masquage_retire_les_seances_des_vues() {
    let store = store_with_fixture();
    store
        .save_rule(
            &Rule {
                hide: true,
                ..rule("CM")
            },
            now(),
        )
        .unwrap();
    assert!(store.day(LUNDI, now()).unwrap().occurrences.is_empty());

    let regle = store.rules().unwrap().remove(0);
    store
        .save_rule(
            &Rule {
                enabled: false,
                ..regle
            },
            now(),
        )
        .unwrap();
    assert_eq!(store.day(LUNDI, now()).unwrap().occurrences.len(), 1);
}

#[test]
fn une_regle_sur_un_champ_exige_de_dire_lequel() {
    let store = store_with_fixture();
    let orpheline = Rule {
        field: RuleField::Property,
        property: None,
        ..rule("TD")
    };
    assert!(store.save_rule(&orpheline, now()).is_err());
}

#[test]
fn une_categorie_posee_a_la_main_resiste_aux_regles() {
    let store = store_with_fixture();
    let choisie = store
        .create_category("Important", "!", Some(0xFF00_FF00), now())
        .unwrap();

    let cm = store.day(LUNDI, now()).unwrap().occurrences[0].clone();
    store
        .set_occurrence_category(&cm.id, Some(choisie.id.clone()))
        .unwrap();
    store
        .set_property_color("type", "Cours magistral", 0xFF11_2233, now())
        .unwrap();

    let apres = &store.day(LUNDI, now()).unwrap().occurrences[0];
    assert_eq!(apres.category_id, Some(choisie.id));
    assert_eq!(apres.color, 0xFF00_FF00);
}

// ------------------------------------------------------- moteur de conflits

#[test]
fn un_chevauchement_bloque_lecriture_et_sexplique() {
    let store = store_with_fixture();
    // Lundi 21 septembre, 08h00-10h00 : le CM de DEV WEB occupe le créneau.
    let outcome = store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Cancel,
            now(),
        )
        .unwrap();

    assert!(outcome.blocked, "rien ne doit être écrit sans arbitrage");
    assert!(outcome.saved.is_none());
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].overlap_minutes, 60);
    assert!(!outcome.conflicts[0].other_deletable);
}

#[test]
fn ignorer_ecrit_malgre_le_chevauchement() {
    let store = store_with_fixture();
    let outcome = store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Ignore,
            now(),
        )
        .unwrap();
    assert!(!outcome.blocked);
    assert!(outcome.saved.is_some());
    assert_eq!(outcome.conflicts.len(), 1, "le conflit reste signalé");
}

#[test]
fn remplacer_libere_le_creneau() {
    let store = store_with_fixture();
    let outcome = store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Replace,
            now(),
        )
        .unwrap();

    assert_eq!(outcome.removed, 0, "rien de local à supprimer");
    assert_eq!(outcome.hidden, 1, "la séance importée est masquée");

    let lundi = store.day(LUNDI, now()).unwrap();
    assert!(
        lundi
            .occurrences
            .iter()
            .all(|o| !o.title.contains("DEV WEB"))
    );
    assert!(lundi.occurrences.iter().any(|o| o.title == "Dentiste"));
}

#[test]
fn decaler_pousse_levenement_apres_le_conflit() {
    let store = store_with_fixture();
    let outcome = store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 0),
            ),
            Resolution::ShiftAfter,
            now(),
        )
        .unwrap();

    let saved = outcome.saved.expect("l'événement décalé est écrit");
    assert_eq!(saved.start_utc, paris(2026, 9, 21, 10, 0));
    assert_eq!(outcome.shifted_minutes, 60);
}

#[test]
fn deux_creneaux_qui_senchainent_ne_sont_pas_en_conflit() {
    let store = store_with_fixture();
    let outcome = store
        .save_event(
            &draft("Café", paris(2026, 9, 21, 10, 0), paris(2026, 9, 21, 11, 0)),
            Resolution::Cancel,
            now(),
        )
        .unwrap();
    assert!(!outcome.blocked);
    assert!(outcome.conflicts.is_empty());
}

#[test]
fn le_gestionnaire_liste_les_chevauchements_existants() {
    let store = store_with_fixture();
    store
        .save_event(
            &draft(
                "Dentiste",
                paris(2026, 9, 21, 9, 0),
                paris(2026, 9, 21, 10, 30),
            ),
            Resolution::Ignore,
            now(),
        )
        .unwrap();

    let paires = store
        .conflicts_between(paris(2026, 9, 21, 0, 0), paris(2026, 9, 22, 0, 0))
        .unwrap();
    assert_eq!(paires.len(), 1);
    assert_eq!(paires[0].overlap_minutes, 60);
}

#[test]
fn une_seance_importee_ne_se_supprime_pas_mais_se_masque() {
    let store = store_with_fixture();
    let importee = store.day(LUNDI, now()).unwrap().occurrences[0].clone();
    assert!(store.delete_event(&importee.id).is_err());

    store.set_muted(&importee.id, true).unwrap();
    assert!(store.day(LUNDI, now()).unwrap().occurrences.is_empty());
    assert_eq!(
        store
            .muted_between(paris(2026, 9, 21, 0, 0), paris(2026, 9, 22, 0, 0))
            .unwrap()
            .len(),
        1
    );

    store.set_muted(&importee.id, false).unwrap();
    assert_eq!(store.day(LUNDI, now()).unwrap().occurrences.len(), 1);
}

#[test]
fn une_seance_masquee_le_reste_apres_reimport() {
    let store = store_with_fixture();
    let importee = store.day(LUNDI, now()).unwrap().occurrences[0].clone();
    store.set_muted(&importee.id, true).unwrap();

    store
        .import_ics(
            "Emploi du temps",
            CalendarKind::IcsFile,
            "ent_ade.ics",
            FIXTURE,
            now(),
        )
        .unwrap();

    assert!(store.day(LUNDI, now()).unwrap().occurrences.is_empty());
}

// ---------------------------------------------------------- choses à faire

fn store_vide() -> Store {
    Store::open(":memory:", "Europe/Paris").expect("base en mémoire")
}

#[test]
fn une_tache_se_pose_sur_un_jour_sans_heure() {
    let store = store_vide();
    let tache = store.add_task("Rendre le TP", LUNDI, now()).unwrap();
    assert_eq!(tache.title, "Rendre le TP");
    assert!(!tache.done);
    assert_eq!(tache.days_late, 0);

    let liste = store.tasks_for_day(LUNDI, LUNDI).unwrap();
    assert_eq!(liste.len(), 1);
}

#[test]
fn une_tache_non_faite_se_reporte_avec_son_retard() {
    let store = store_vide();
    store.add_task("Rendre le TP", LUNDI, now()).unwrap();

    // Deux jours plus tard, elle est toujours là, et elle le dit.
    let liste = store.tasks_for_day(LUNDI + 2, LUNDI + 2).unwrap();
    assert_eq!(liste.len(), 1);
    assert_eq!(liste[0].days_late, 2);
    assert_eq!(
        liste[0].planned_day, LUNDI,
        "son jour d'origine ne bouge pas"
    );
}

#[test]
fn une_tache_cochee_cesse_de_se_reporter() {
    let store = store_vide();
    let tache = store.add_task("Rendre le TP", LUNDI, now()).unwrap();
    store.set_task_done(&tache.id, true, LUNDI + 1).unwrap();

    // Elle apparaît le jour où elle a été faite...
    let faite = store.tasks_for_day(LUNDI + 1, LUNDI + 1).unwrap();
    assert_eq!(faite.len(), 1);
    assert!(faite[0].done);
    assert_eq!(faite[0].days_late, 0);

    // ...et plus le lendemain.
    assert!(
        store
            .tasks_for_day(LUNDI + 2, LUNDI + 2)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn decocher_une_tache_la_remet_en_retard() {
    let store = store_vide();
    let tache = store.add_task("Rendre le TP", LUNDI, now()).unwrap();
    store.set_task_done(&tache.id, true, LUNDI).unwrap();
    let remise = store.set_task_done(&tache.id, false, LUNDI + 3).unwrap();

    assert!(!remise.done);
    assert_eq!(remise.days_late, 3);
    assert_eq!(store.tasks_for_day(LUNDI + 3, LUNDI + 3).unwrap().len(), 1);
}

#[test]
fn un_jour_a_venir_ne_montre_que_ses_propres_taches() {
    let store = store_vide();
    store.add_task("En retard", LUNDI, now()).unwrap();
    store.add_task("Pour jeudi", LUNDI + 3, now()).unwrap();

    // Consulté depuis lundi, jeudi ne montre pas ce qui traîne : sans quoi la
    // vue Semaine répéterait sept fois la même liste.
    let jeudi = store.tasks_for_day(LUNDI + 3, LUNDI).unwrap();
    assert_eq!(jeudi.len(), 1);
    assert_eq!(jeudi[0].title, "Pour jeudi");

    // Aujourd'hui, en revanche, montre tout ce qui est dû.
    assert_eq!(store.tasks_for_day(LUNDI, LUNDI).unwrap().len(), 1);
}

#[test]
fn repousser_une_tache_change_son_jour_de_reference() {
    let store = store_vide();
    let tache = store.add_task("Rendre le TP", LUNDI, now()).unwrap();
    store.move_task(&tache.id, LUNDI + 5).unwrap();

    assert!(
        store
            .tasks_for_day(LUNDI + 1, LUNDI + 1)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.tasks_for_day(LUNDI + 5, LUNDI + 5).unwrap().len(), 1);
}

#[test]
fn la_vue_maintenant_compte_ce_quil_reste_a_faire() {
    let store = store_with_fixture();
    let today = store.epoch_day_of(now()).unwrap();
    store.add_task("Rendre le TP", today - 2, now()).unwrap();
    store.add_task("Acheter un cahier", today, now()).unwrap();

    let vue = store.now_view(now()).unwrap();
    assert_eq!(vue.pending_tasks, 2);
    assert_eq!(vue.late_tasks, 1, "une seule traîne depuis la veille");
}

#[test]
fn la_journee_porte_ses_seances_et_ses_taches() {
    let store = store_with_fixture();
    store.add_task("Réviser", LUNDI, now()).unwrap();

    let jour = store.day(LUNDI, paris(2026, 9, 21, 8, 0)).unwrap();
    assert_eq!(jour.occurrences.len(), 1);
    assert_eq!(jour.tasks.len(), 1);
}

// ------------------------------------------------ synchronisation et rappels

/// Un mini-calendrier à un seul cours, pour observer les différences.
fn one_course(hour: u32, location: &str, cancelled: bool) -> String {
    let status = if cancelled {
        "STATUS:CANCELLED\r\n"
    } else {
        ""
    };
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Timewrap//Test//FR\r\n\
         BEGIN:VEVENT\r\nUID:T-1@timewrap\r\n\
         DTSTART;TZID=Europe/Paris:20261001T{hour:02}0000\r\n\
         DTEND;TZID=Europe/Paris:20261001T{end:02}0000\r\n\
         SUMMARY:Analyse\r\nLOCATION:{location}\r\n\
         DESCRIPTION:Type : TD\r\n{status}END:VEVENT\r\nEND:VCALENDAR\r\n",
        end = hour + 2
    )
}

fn store_with(text: &str) -> Store {
    let store = store_vide();
    store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            text,
            now(),
        )
        .unwrap();
    store
}

#[test]
fn un_premier_import_nannonce_pas_de_changement() {
    let store = store_vide();
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            &one_course(10, "C204", false),
            now(),
        )
        .unwrap();
    assert_eq!(rapport.import.occurrences, 1);
    assert!(
        rapport.changes.iter().all(|c| c.summary.contains("ajouté")),
        "tout est nouveau, mais rien n'a bougé"
    );
}

#[test]
fn un_cours_deplace_est_annonce_en_clair() {
    let store = store_with(&one_course(10, "C204", false));
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            &one_course(14, "C204", false),
            now(),
        )
        .unwrap();

    assert_eq!(rapport.changes.len(), 1);
    let phrase = &rapport.changes[0].summary;
    assert!(phrase.contains("Analyse"), "obtenu : {phrase}");
    assert!(phrase.contains("10:00"), "obtenu : {phrase}");
    assert!(phrase.contains("14:00"), "obtenu : {phrase}");
    assert!(phrase.contains("jeudi 1 octobre"), "obtenu : {phrase}");
}

#[test]
fn un_changement_de_salle_est_annonce() {
    let store = store_with(&one_course(10, "C204", false));
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            &one_course(10, "D101", false),
            now(),
        )
        .unwrap();

    assert_eq!(rapport.changes.len(), 1);
    let phrase = &rapport.changes[0].summary;
    assert!(phrase.contains("change de salle"), "obtenu : {phrase}");
    assert!(phrase.contains("D101"), "obtenu : {phrase}");
}

#[test]
fn une_annulation_est_annoncee() {
    let store = store_with(&one_course(10, "C204", false));
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            &one_course(10, "C204", true),
            now(),
        )
        .unwrap();

    assert_eq!(rapport.changes.len(), 1);
    assert!(rapport.changes[0].summary.contains("annulé"));
}

#[test]
fn un_cours_retire_est_annonce() {
    let store = store_with(&one_course(10, "C204", false));
    let vide =
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Timewrap//Test//FR\r\nEND:VCALENDAR\r\n";
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            vide,
            now(),
        )
        .unwrap();

    assert_eq!(rapport.changes.len(), 1);
    assert!(rapport.changes[0].summary.contains("retiré"));
}

#[test]
fn un_import_identique_nannonce_rien() {
    let store = store_with(&one_course(10, "C204", false));
    let rapport = store
        .import_ics(
            "EDT",
            CalendarKind::IcsUrl,
            "https://ent/edt.ics",
            &one_course(10, "C204", false),
            now(),
        )
        .unwrap();
    assert!(rapport.changes.is_empty());
}

#[test]
fn les_rappels_se_posent_avant_le_cours() {
    let store = store_with_fixture();
    let reglages = store.settings().unwrap();
    store
        .update_settings(&crate::model::Settings {
            reminders_enabled: true,
            reminder_lead_minutes: 15,
            ..reglages
        })
        .unwrap();

    let rappels = store.reminders(paris(2026, 9, 21, 6, 0), 1, 10).unwrap();
    let premier = rappels.first().expect("le CM du lundi");
    assert_eq!(premier.start_utc, paris(2026, 9, 21, 8, 0));
    assert_eq!(premier.trigger_utc, paris(2026, 9, 21, 7, 45));
}

#[test]
fn sans_rappels_actives_rien_nest_programme() {
    let store = store_with_fixture();
    assert!(
        store
            .reminders(paris(2026, 9, 21, 6, 0), 1, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn une_seance_masquee_ne_declenche_pas_de_rappel() {
    let store = store_with_fixture();
    let reglages = store.settings().unwrap();
    store
        .update_settings(&crate::model::Settings {
            reminders_enabled: true,
            ..reglages
        })
        .unwrap();

    let cm = store.day(LUNDI, now()).unwrap().occurrences[0].clone();
    store.set_muted(&cm.id, true).unwrap();

    let rappels = store.reminders(paris(2026, 9, 21, 6, 0), 1, 10).unwrap();
    assert!(rappels.iter().all(|r| r.occurrence_id != cm.id));
}

#[test]
fn les_reglages_se_relisent_et_se_bornent() {
    let store = store_vide();
    let defaut = store.settings().unwrap();
    assert!(!defaut.sync_enabled);
    assert_eq!(defaut.reminder_lead_minutes, 15);

    let enregistres = store
        .update_settings(&crate::model::Settings {
            source_url: "https://ent.exemple.fr/edt.ics".into(),
            sync_enabled: true,
            sync_interval_hours: 999,
            reminder_lead_minutes: 999,
            ..defaut
        })
        .unwrap();

    assert_eq!(enregistres.source_url, "https://ent.exemple.fr/edt.ics");
    assert!(enregistres.sync_enabled);
    assert_eq!(enregistres.sync_interval_hours, 168, "une semaine au plus");
    assert_eq!(
        enregistres.reminder_lead_minutes, 240,
        "quatre heures au plus"
    );
}

#[test]
fn un_import_par_url_retient_ladresse() {
    let store = store_with(&one_course(10, "C204", false));
    assert_eq!(
        store.settings().unwrap().source_url,
        "https://ent/edt.ics",
        "la synchronisation automatique doit savoir où retourner"
    );
    assert!(store.settings().unwrap().last_sync_utc.is_some());
}
