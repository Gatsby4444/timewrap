//! Types du domaine, tels que l'interface les consomme.
//!
//! Les instants voyagent en secondes depuis l'époque Unix, en UTC : c'est la
//! seule représentation qui ne se prête à aucune ambiguïté à la traversée de la
//! frontière FFI. La conversion vers l'heure locale est faite par le cœur pour
//! le regroupement par jour, et par l'interface pour l'affichage.

/// D'où viennent les événements d'un agenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CalendarKind {
    /// Fichier `.ics` importé une fois.
    IcsFile,
    /// URL d'abonnement, re-téléchargeable.
    IcsUrl,
    /// Agenda local, édité dans l'application.
    Local,
}

impl CalendarKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CalendarKind::IcsFile => "ics_file",
            CalendarKind::IcsUrl => "ics_url",
            CalendarKind::Local => "local",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "ics_url" => CalendarKind::IcsUrl,
            "local" => CalendarKind::Local,
            _ => CalendarKind::IcsFile,
        }
    }
}

/// D'où vient une occurrence précise.
///
/// La distinction compte au moment de résoudre un conflit : une séance locale
/// se supprime, une séance importée ne peut être que masquée — le prochain
/// import la ferait revenir de toute façon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EventOrigin {
    /// Développée depuis un flux iCalendar.
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

/// Un agenda : un dossier d'événements, que l'on peut consulter seul ou mêlé
/// aux autres.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Calendar {
    pub id: String,
    pub name: String,
    pub kind: CalendarKind,
    /// Chemin d'origine ou URL d'abonnement, vide pour un agenda local.
    pub source: String,
    /// Couleur ARGB.
    pub color: u32,
    pub visible: bool,
    /// Date de la dernière importation, en secondes Unix.
    pub last_sync: Option<i64>,
    pub event_count: u32,
    /// Ordre d'affichage sur l'écran d'accueil.
    pub position: i32,
}

/// Une catégorie visuelle : « CM », « TD », « Examen »…
///
/// C'est elle qui porte la couleur et la pastille affichées ; les règles ne
/// font que décider quelle occurrence en reçoit une.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// Étiquette courte affichée sur les blocs — « TD », « TP », « ★ ».
    pub label: String,
    pub color: u32,
    pub position: i32,
    /// Occurrences actuellement classées ici.
    pub occurrence_count: u32,
}

/// Le champ d'une occurrence sur lequel une règle se prononce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RuleField {
    Title,
    Location,
    Description,
    /// Les trois à la fois : pratique quand l'ENT range le type du cours
    /// tantôt dans l'intitulé, tantôt dans la description.
    Any,
}

impl RuleField {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RuleField::Title => "title",
            RuleField::Location => "location",
            RuleField::Description => "description",
            RuleField::Any => "any",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "location" => RuleField::Location,
            "description" => RuleField::Description,
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
    /// s'accrocher à « STDI ».
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

/// Une règle visuelle : « si l'intitulé contient TD, classe en TD ».
///
/// Les règles sont appliquées dans l'ordre de `priority` croissante, et chacune
/// n'écrase que ce qu'elle renseigne : une règle qui ne fait que masquer laisse
/// intacte la catégorie posée par une règle précédente.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Rule {
    pub id: String,
    pub name: String,
    /// Restreinte à un agenda, ou appliquée partout si absent.
    pub calendar_id: Option<String>,
    pub field: RuleField,
    pub match_kind: RuleMatch,
    pub pattern: String,
    pub case_sensitive: bool,
    /// Catégorie posée sur les occurrences correspondantes.
    pub category_id: Option<String>,
    /// Titre de remplacement. `{}` y est remplacé par le titre d'origine.
    pub rename_to: Option<String>,
    /// Retire l'occurrence de toutes les vues sans toucher à l'import.
    pub hide: bool,
    pub priority: i32,
    pub enabled: bool,
    /// Occurrences que cette règle touche actuellement.
    pub match_count: u32,
}

/// Ce qu'une règle décide pour une occurrence.
#[derive(Debug, Clone, Default)]
pub(crate) struct RuleOutcome {
    pub category_id: Option<String>,
    pub display_title: Option<String>,
    pub hidden: bool,
}

/// Une occurrence concrète : un cours à une date et une heure précises.
///
/// C'est l'unité que manipule l'interface. Les récurrences sont déjà
/// développées et les règles visuelles déjà appliquées.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Occurrence {
    /// Identifiant stable au fil des réimports : c'est lui qui permet aux
    /// personnalisations de survivre à une resynchronisation.
    pub id: String,
    pub calendar_id: String,
    pub calendar_name: String,
    /// Couleur retenue pour l'affichage : celle de la catégorie si une règle
    /// en a posé une, celle de l'agenda sinon.
    pub color: u32,
    pub uid: String,
    /// Titre affiché, renommage appliqué.
    pub title: String,
    /// Titre tel qu'il est arrivé dans le `.ics`, pour les écrans de réglage.
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

/// Le contenu d'une journée.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DayAgenda {
    /// Nombre de jours depuis le 1er janvier 1970, tel que `LocalDate.toEpochDay()`.
    pub epoch_day: i64,
    pub occurrences: Vec<Occurrence>,
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
}

/// Bilan d'une importation, affiché après avoir chargé un `.ics`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ImportReport {
    pub calendar_id: String,
    /// Événements distincts lus dans le fichier (séries comprises).
    pub events: u32,
    /// Occurrences produites sur la fenêtre courante.
    pub occurrences: u32,
    /// Composants ignorés faute d'être exploitables, avec la raison.
    pub skipped: Vec<String>,
    pub first_start_utc: Option<i64>,
    pub last_start_utc: Option<i64>,
}

/// Ce qu'affiche la tuile d'un agenda sur l'écran d'accueil.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CalendarSummary {
    pub calendar: Calendar,
    /// Séances à venir dans les sept prochains jours.
    pub upcoming_week: u32,
    /// La prochaine séance de cet agenda, quelle que soit sa date.
    pub next: Option<Occurrence>,
    /// Chevauchements internes non résolus.
    pub conflicts: u32,
}

/// Un événement à créer ou à modifier.
///
/// `id` absent signifie création ; renseigné, il désigne l'occurrence locale
/// que l'on remplace.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EventDraft {
    pub id: Option<String>,
    pub calendar_id: String,
    pub title: String,
    pub location: String,
    pub description: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub all_day: bool,
    pub category_id: Option<String>,
}

/// D'où vient le chevauchement, du point de vue de l'événement examiné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConflictScope {
    /// Deux créneaux du même agenda se marchent dessus.
    SameCalendar,
    /// Le heurt vient d'un autre agenda.
    CrossCalendar,
}

/// Un chevauchement constaté entre un projet d'événement et l'existant.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Conflict {
    pub scope: ConflictScope,
    /// L'occurrence déjà en place.
    pub other: Occurrence,
    /// Durée du recouvrement, en minutes.
    pub overlap_minutes: i64,
    /// Vrai si l'occurrence en place peut être supprimée (séance locale) ;
    /// faux si elle ne peut être que masquée (séance importée).
    pub other_deletable: bool,
}

/// Un chevauchement entre deux occurrences déjà enregistrées, tel que le
/// gestionnaire de conflits le présente.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ConflictPair {
    pub scope: ConflictScope,
    pub first: Occurrence,
    pub second: Occurrence,
    pub overlap_minutes: i64,
}

/// Ce que l'utilisateur décide face à un chevauchement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Resolution {
    /// Ne rien écrire tant que le conflit tient : c'est le mode par défaut,
    /// celui qui déclenche la question.
    Cancel,
    /// Écrire quand même : deux cours peuvent légitimement se chevaucher.
    Ignore,
    /// Faire place nette : les séances en conflit sont supprimées si elles
    /// sont locales, masquées si elles viennent d'un import.
    Replace,
    /// Décaler le nouvel événement juste après le dernier conflit, à durée
    /// constante.
    ShiftAfter,
}

/// Résultat d'une écriture d'événement.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SaveOutcome {
    /// L'occurrence écrite, absente si l'écriture a été retenue.
    pub saved: Option<Occurrence>,
    /// Les chevauchements rencontrés, qu'ils aient été résolus ou non.
    pub conflicts: Vec<Conflict>,
    /// Vrai si rien n'a été écrit et qu'il faut interroger l'utilisateur.
    pub blocked: bool,
    /// Occurrences supprimées par un remplacement.
    pub removed: u32,
    /// Occurrences masquées par un remplacement.
    pub hidden: u32,
    /// Décalage appliqué, en minutes, si la résolution était `ShiftAfter`.
    pub shifted_minutes: i64,
}

/// Une règle que le cœur propose de créer, déduite de ce qui a été importé.
#[derive(Debug, Clone, uniffi::Record)]
pub struct RuleSuggestion {
    /// Intitulé lisible : « Les cours marqués TD ».
    pub label: String,
    pub field: RuleField,
    pub match_kind: RuleMatch,
    pub pattern: String,
    /// Occurrences que la règle toucherait.
    pub occurrences: u32,
    /// Quelques titres concernés, pour que l'utilisateur vérifie avant.
    pub samples: Vec<String>,
    /// Couleur proposée, prise dans la palette et encore inutilisée.
    pub suggested_color: u32,
}
