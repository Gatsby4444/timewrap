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

/// Un agenda : une source d'événements que l'on peut afficher ou masquer.
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
}

/// Une occurrence concrète : un cours à une date et une heure précises.
///
/// C'est l'unité que manipule l'interface. Les récurrences sont déjà développées.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Occurrence {
    /// Identifiant stable au fil des réimports : c'est lui qui permettra aux
    /// notes et personnalisations de survivre à une resynchronisation.
    pub id: String,
    pub calendar_id: String,
    pub calendar_name: String,
    pub color: u32,
    pub uid: String,
    pub title: String,
    pub location: String,
    pub description: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub all_day: bool,
    pub cancelled: bool,
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
