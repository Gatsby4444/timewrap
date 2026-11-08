//! Reconnaître une épreuve.
//!
//! Un examen ne se rate pas : il mérite d'être vu de loin dans la semaine. Les
//! exports d'ENT ne le marquent pas d'un champ dédié, mais ils l'écrivent
//! presque toujours en clair — dans l'intitulé (« DS Analyse », « Partiel
//! Algèbre ») ou dans le type de cours (« Type : Examen »).
//!
//! La recherche se fait **par mot entier**, sans accent ni majuscule : « DS »
//! doit reconnaître « DS Analyse » mais pas « Méthodes » ni « ADSL », et
//! « exam » ne doit pas surgir au milieu de « examinateur ».

use crate::properties::normalize;

/// Mots qui désignent une épreuve, déjà normalisés.
const EXAM_WORDS: &[&str] = &[
    "examen",
    "examens",
    "exam",
    "exams",
    "partiel",
    "partiels",
    "ds",
    "dst",
    "cc",
    "controle",
    "interro",
    "interrogation",
    "evaluation",
    "soutenance",
    "qcm",
    "rattrapage",
    "epreuve",
    "concours",
];

/// Vrai si l'un des textes désigne une épreuve.
pub fn is_exam<'a>(texts: impl IntoIterator<Item = &'a str>) -> bool {
    texts.into_iter().any(|text| {
        normalize(text)
            .split(|c: char| !c.is_alphanumeric())
            // « DS2 », « CC1 » : un numéro collé ne change pas la nature.
            .map(|word| word.trim_end_matches(|c: char| c.is_ascii_digit()))
            .any(|word| EXAM_WORDS.contains(&word))
    })
}

#[cfg(test)]
mod tests {
    use super::is_exam;

    #[test]
    fn reconnait_les_intitules_courants() {
        for titre in [
            "DS Analyse",
            "Partiel d'Algèbre",
            "Examen final",
            "Contrôle continu",
            "Soutenance de stage",
            "Épreuve écrite",
            "QCM - Réseaux",
            "CC2 Probabilités",
        ] {
            assert!(is_exam([titre]), "« {titre} » est une épreuve");
        }
    }

    #[test]
    fn ne_reconnait_pas_un_mot_contenu_dans_un_autre() {
        for titre in [
            "Méthodes numériques",
            "ADSL et réseaux",
            "Examinateurs : réunion",
            "TD Analyse",
        ] {
            assert!(!is_exam([titre]), "« {titre} » n'est pas une épreuve");
        }
    }

    #[test]
    fn le_type_de_cours_suffit() {
        assert!(is_exam(["Analyse", "Examen"]));
    }
}
