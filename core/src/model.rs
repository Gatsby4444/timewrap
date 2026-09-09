//! Types du domaine, tels que l'interface les consomme.
//!
//! Les instants voyagent en secondes depuis l'époque Unix, en UTC : c'est la
//! seule représentation qui ne se prête à aucune ambiguïté à la traversée de la
//! frontière FFI. La conversion vers l'heure locale est faite par le cœur pour
//! le regroupement par jour, et par l'interface pour l'affichage.

/// D'où viennent les événements de l'emploi du temps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CalendarKind {
    /// Fichier `.ics` importé une fois.
    IcsFile,
    /// URL d'abonnement, re-téléchargeable.
    IcsUrl,
}

impl CalendarKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CalendarKind::IcsFile => "ics_file",
            CalendarKind::IcsUrl => "ics_url",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "ics_url" => CalendarKind::IcsUrl,
            _ => CalendarKind::IcsFile,
        }
    }
}

/// D'où vient une occurrence précise.
///
/// La distinction compte au moment de résoudre un conflit : une séance saisie
/// ici se supprime, une séance importée ne peut être que masquée — le prochain
/// import la ferait revenir de toute façon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EventOrigin {
    /// Développée depuis le flux iCalendar.
    Ics,
    /// Saisie dans l'application.
    Local,
}

impl EventOrigin {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            EventOrigin::Ics => "ics",
            EventOrigin::Local => "local",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "local" => EventOrigin::Local,
            _ => EventOrigin::Ics,
        }
    }
}

/// L'emploi du temps : sa source, sa dernière mise à jour, sa taille.
///
/// Il n'y en a qu'un. Réimporter un fichier ou resynchroniser une URL remplace
/// son contenu sans changer son identité, ce qui laisse intactes les séances
/// ajoutées à la main et les personnalisations.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Timetable {
    pub id: String,
    pub name: String,
    pub kind: CalendarKind,
    /// Chemin d'origine ou URL d'abonnement.
    pub source: String,
    /// Couleur par défaut, quand aucune catégorie ne s'applique.
    pub color: u32,
    /// Date de la dernière importation, en secondes Unix.
    pub last_sync: Option<i64>,
    /// Cours distincts, séries comptées une fois.
    pub event_count: u32,
    /// Séances développées, toutes dates confondues.
    pub occurrence_count: u32,
}

/// Une catégorie visuelle : « TD », « Analyse », « Examen »…
///
/// C'est elle qui porte la couleur affichée ; les règles ne font que décider
/// quelle séance en reçoit une.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// Étiquette courte affichée sur les blocs — « TD », « TP ».
    pub label: String,
    pub color: u32,
    pub position: i32,
    /// Séances actuellement classées ici.
    pub occurrence_count: u32,
}

/// Un champ structuré repéré dans les descriptions importées.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PropertyKey {
    /// Forme comparable : « matiere ».
    pub key: String,
    /// Forme lisible, telle que l'ENT l'écrit : « Matière ».
    pub label: String,
    /// Valeurs distinctes prises par ce champ.
    pub distinct_values: u32,
    /// Séances qui le renseignent.
    pub occurrences: u32,
    /// Valeurs déjà associées à une couleur.
    pub colored_values: u32,
}

/// Une valeur d'un champ, et la couleur qu'on lui a donnée.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PropertyValue {
    pub key: String,
    pub value: String,
    pub occurrences: u32,
    /// Catégorie associée, absente tant qu'aucune couleur n'est posée.
    pub category_id: Option<String>,
    /// Couleur effective : celle de la catégorie, ou celle de l'emploi du temps.
    pub color: u32,
    pub colored: bool,
}

/// Le champ d'une séance sur lequel une règle se prononce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RuleField {
    Title,
    Location,
    Description,
    /// Un champ structuré de la description, nommé par `Rule::property`.
    Property,
    /// Titre, lieu et description à la fois.
    Any,
}

impl RuleField {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RuleField::Title => "title",
            RuleField::Location => "location",
            RuleField::Description => "description",
            RuleField::Property => "property",
            RuleField::Any => "any",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "location" => RuleField::Location,
            "description" => RuleField::Description,
            "property" => RuleField::Property,
            "any" => RuleField::Any,
            _ => RuleField::Title,
        }
    }
}

/// Comment le motif est comparé au champ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RuleMatch {
    Contains,
    StartsWith,
    EndsWith,
    Equals,
    /// Le motif doit apparaître comme un mot entier : « TD » ne doit pas
    /// s'accrocher à « BDD ».
    Word,
}

impl RuleMatch {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RuleMatch::Contains => "contains",
            RuleMatch::StartsWith => "starts_with",
            RuleMatch::EndsWith => "ends_with",
            RuleMatch::Equals => "equals",
            RuleMatch::Word => "word",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "starts_with" => RuleMatch::StartsWith,
            "ends_with" => RuleMatch::EndsWith,
            "equals" => RuleMatch::Equals,
            "word" => RuleMatch::Word,
            _ => RuleMatch::Contains,
        }
    }
}

/// Une règle visuelle : « si le champ Type vaut TD, colorie en orange ».
///
/// Les règles sont appliquées dans l'ordre de `priority` croissante, et chacune
/// n'écrase que ce qu'elle renseigne : une règle de couleur et une règle de
/// masquage se cumulent au lieu de se disputer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub field: RuleField,
    /// Champ visé quand `field` vaut `Property` — « type », « matiere ».
    pub property: Option<String>,
    pub match_kind: RuleMatch,
    pub pattern: String,
    pub case_sensitive: bool,
    /// Catégorie posée sur les séances correspondantes.
    pub category_id: Option<String>,
    /// Titre de remplacement. `{}` y est remplacé par le titre d'origine.
    pub rename_to: Option<String>,
    /// Retire la séance de toutes les vues sans toucher à l'import.
    pub hide: bool,
    pub priority: i32,
    pub enabled: bool,
    /// Séances que cette règle touche actuellement.
    pub match_count: u32,
}

/// Ce qu'une règle décide pour une séance.
#[derive(Debug, Clone, Default)]
pub(crate) struct RuleOutcome {
    pub category_id: Option<String>,
    pub display_title: Option<String>,
    pub hidden: bool,
}

/// Une séance concrète : un cours à une date et une heure précises.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Occurrence {
    /// Identifiant stable au fil des réimports : c'est lui qui permet aux
    /// personnalisations de survivre à une resynchronisation.
    pub id: String,
    /// Couleur retenue : celle de la catégorie si une règle en a posé une,
    /// celle de l'emploi du temps sinon.
    pub color: u32,
    pub uid: String,
    /// Titre affiché, renommage appliqué.
    pub title: String,
    /// Titre tel qu'il est arrivé dans le `.ics`.
    pub raw_title: String,
    pub location: String,
    pub description: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub all_day: bool,
    pub cancelled: bool,
    pub origin: EventOrigin,
    pub category_id: Option<String>,
    pub category_name: String,
    /// Étiquette courte de la catégorie, vide si aucune.
    pub category_label: String,
    pub hidden: bool,
}

impl Occurrence {
    pub fn duration_minutes(&self) -> i64 {
        (self.end_utc - self.start_utc) / 60
    }
}

/// Le contenu d'une journée : les séances, et ce qu'il reste à faire.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DayAgenda {
    /// Nombre de jours depuis le 1er janvier 1970, tel que `LocalDate.toEpochDay()`.
    pub epoch_day: i64,
    pub occurrences: Vec<Occurrence>,
    pub tasks: Vec<Task>,
}

/// Ce qu'il faut savoir en une seconde, en sortant d'un cours.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NowView {
    /// Le cours en train de se dérouler, s'il y en a un.
    pub current: Option<Occurrence>,
    /// Le prochain cours, aujourd'hui ou plus tard.
    pub next: Option<Occurrence>,
    /// Ce qu'il reste de la journée, cours courant exclu.
    pub rest_of_day: Vec<Occurrence>,
    /// Minutes restantes dans le cours courant.
    pub minutes_remaining: Option<i64>,
    /// Minutes avant le prochain cours.
    pub minutes_until_next: Option<i64>,
    /// Ce qu'il reste à faire aujourd'hui.
    pub pending_tasks: u32,
    /// Parmi elles, celles qui traînent depuis un jour au moins.
    pub late_tasks: u32,
}

/// Une chose à faire dans la journée, sans heure.
///
/// Non cochée le soir, elle reparaît le lendemain avec son retard affiché :
/// c'est tout l'intérêt d'une liste qui ne se vide pas toute seule.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub notes: String,
    /// Jour pour lequel elle était prévue.
    pub planned_day: i64,
    pub done: bool,
    /// Jour où elle a été cochée.
    pub done_day: Option<i64>,
    /// Jours de retard à la date consultée, zéro si elle est à sa place.
    pub days_late: i64,
    pub position: i32,
}

/// Bilan d'une importation.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ImportReport {
    /// Événements distincts lus dans le fichier (séries comprises).
    pub events: u32,
    /// Séances produites sur la fenêtre courante.
    pub occurrences: u32,
    /// Composants ignorés faute d'être exploitables, avec la raison.
    pub skipped: Vec<String>,
    pub first_start_utc: Option<i64>,
    pub last_start_utc: Option<i64>,
}

/// Ce qui a changé entre deux versions de l'emploi du temps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChangeKind {
    Added,
    Removed,
    Moved,
    Cancelled,
    Room,
}

/// Un changement, déjà rédigé pour être affiché tel quel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Change {
    pub kind: ChangeKind,
    pub title: String,
    /// Phrase prête à lire : « Le TD d'Analyse de mardi passe de 13:30 à 15:00 ».
    pub summary: String,
    /// Quand se tient la séance concernée, après changement.
    pub start_utc: i64,
}

/// Bilan d'une synchronisation : ce qui a été lu, et ce qui a bougé.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncReport {
    pub import: ImportReport,
    /// Changements à venir, du plus proche au plus lointain.
    pub changes: Vec<Change>,
}

/// Un rappel à programmer sur l'appareil.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Reminder {
    pub occurrence_id: String,
    pub title: String,
    pub location: String,
    pub start_utc: i64,
    /// Instant auquel la notification doit partir.
    pub trigger_utc: i64,
}

/// Les réglages persistés, tels que l'écran de configuration les manipule.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Settings {
    /// URL d'abonnement, vide si l'emploi du temps vient d'un fichier.
    pub source_url: String,
    pub sync_enabled: bool,
    /// Intervalle entre deux synchronisations automatiques.
    pub sync_interval_hours: u32,
    pub last_sync_utc: Option<i64>,
    /// Prévenir quand une séance change de créneau, de salle, ou disparaît.
    pub notify_changes: bool,
    pub reminders_enabled: bool,
    /// Combien de minutes avant le début d'un cours.
    pub reminder_lead_minutes: u32,
    /// Résumé du matin : ce qui vient et ce qu'il reste à faire.
    pub digest_enabled: bool,
    /// Heure du résumé, en minutes après minuit.
    pub digest_minutes: u32,
}

/// Un événement à créer ou à modifier dans l'emploi du temps.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EventDraft {
    pub id: Option<String>,
    pub title: String,
    pub location: String,
    pub description: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub all_day: bool,
    pub category_id: Option<String>,
}

/// Un chevauchement entre un projet d'événement et l'existant.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Conflict {
    /// La séance déjà en place.
    pub other: Occurrence,
    /// Durée du recouvrement, en minutes.
    pub overlap_minutes: i64,
    /// Vrai si elle peut être supprimée (séance saisie ici) ; faux si elle ne
    /// peut être que masquée (séance importée).
    pub other_deletable: bool,
}

/// Un chevauchement entre deux séances déjà enregistrées.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConflictPair {
    pub first: Occurrence,
    pub second: Occurrence,
    pub overlap_minutes: i64,
}

/// Ce que l'utilisateur décide face à un chevauchement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Resolution {
    /// Ne rien écrire tant que le conflit tient : le défaut, celui qui
    /// déclenche la question.
    Cancel,
    /// Écrire quand même : deux créneaux peuvent légitimement se chevaucher.
    Ignore,
    /// Faire place nette : les séances en conflit sont supprimées si elles ont
    /// été saisies ici, masquées si elles viennent de l'import.
    Replace,
    /// Décaler le nouvel événement juste après le dernier conflit, à durée
    /// constante.
    ShiftAfter,
}

/// Résultat d'une écriture d'événement.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SaveOutcome {
    /// La séance écrite, absente si l'écriture a été retenue.
    pub saved: Option<Occurrence>,
    /// Les chevauchements rencontrés, qu'ils aient été résolus ou non.
    pub conflicts: Vec<Conflict>,
    /// Vrai si rien n'a été écrit et qu'il faut interroger l'utilisateur.
    pub blocked: bool,
    /// Séances supprimées par un remplacement.
    pub removed: u32,
    /// Séances masquées par un remplacement.
    pub hidden: u32,
    /// Décalage appliqué, en minutes, si la résolution était `ShiftAfter`.
    pub shifted_minutes: i64,
}

/// Une règle que le cœur propose, déduite de ce qui a été importé.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RuleSuggestion {
    /// Intitulé lisible : « Colorier par Matière ».
    pub label: String,
    pub field: RuleField,
    pub property: Option<String>,
    pub match_kind: RuleMatch,
    pub pattern: String,
    /// Séances que la règle toucherait.
    pub occurrences: u32,
    /// Quelques exemples, pour que l'utilisateur vérifie avant.
    pub samples: Vec<String>,
    /// Couleur proposée, prise dans la palette et encore inutilisée.
    pub suggested_color: u32,
}
