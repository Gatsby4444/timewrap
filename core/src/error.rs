//! Erreurs traversant la frontière FFI.
//!
//! Les messages sont rédigés pour être affichables tels quels dans l'interface :
//! l'utilisateur qui importe un `.ics` cassé doit comprendre ce qui cloche.

/// Toute opération publique du cœur échoue avec cette erreur.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TimewrapError {
    #[error("base de données : {0}")]
    Database(String),

    #[error("fichier iCalendar illisible : {0}")]
    Parse(String),

    #[error("agenda introuvable : {0}")]
    CalendarNotFound(String),

    #[error("introuvable : {0}")]
    NotFound(String),

    #[error("{0}")]
    InvalidEvent(String),

    #[error("fuseau horaire inconnu : {0}")]
    UnknownTimezone(String),
}

impl From<rusqlite::Error> for TimewrapError {
    fn from(e: rusqlite::Error) -> Self {
        TimewrapError::Database(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, TimewrapError>;
