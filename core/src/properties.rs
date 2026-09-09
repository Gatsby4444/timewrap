//! Lecture des champs structurés d'un événement.
//!
//! Un export d'ENT range l'essentiel dans la description, en clair et par
//! lignes : « Type : TD », « Matière : Analyse », « Salle : C204 ». C'est
//! l'information la plus fiable du fichier — bien plus que l'intitulé, qui
//! mélange code de module, groupe et abréviations — et c'est elle qui permet de
//! colorier un emploi du temps par type de cours ou par matière sans écrire une
//! règle par valeur.
//!
//! Le module ne fait que lire : il extrait des couples clé/valeur, les
//! normalise pour qu'on puisse les comparer, et laisse le stockage décider quoi
//! en faire.

/// Un champ lu dans un événement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// Clé normalisée — minuscules, sans accent : « matiere ».
    pub key: String,
    /// Clé telle qu'elle était écrite, pour l'afficher : « Matière ».
    pub label: String,
    pub value: String,
}

/// Longueur au-delà de laquelle une « clé » n'en est plus une : une phrase
/// contenant un deux-points ne doit pas devenir un champ.
const MAX_KEY_LENGTH: usize = 32;

/// Longueur au-delà de laquelle une valeur cesse d'être une étiquette.
const MAX_VALUE_LENGTH: usize = 120;

/// Extrait les champs d'une description d'événement.
///
/// Chaque ligne de la forme `Clé : Valeur` en produit un. Les lignes sans
/// deux-points sont ignorées : elles portent des commentaires libres, pas des
/// champs. Une clé vue deux fois garde sa première valeur — les exports qui
/// répètent « Salle » pour un cours sur deux salles décrivent une liste, et une
/// liste ne fait pas une bonne couleur.
pub fn parse(description: &str) -> Vec<Property> {
    let mut out: Vec<Property> = Vec::new();

    for line in description.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Une URL contient un deux-points sans être un champ : « voir
        // https://ent.exemple.fr » deviendrait sinon la clé « voir https ».
        if line.contains("://") {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };

        let label = raw_key.trim();
        let value = raw_value.trim();
        if label.is_empty() || value.is_empty() {
            continue;
        }
        if label.chars().count() > MAX_KEY_LENGTH || value.chars().count() > MAX_VALUE_LENGTH {
            continue;
        }
        // Une clé est un intitulé, pas une phrase : des chiffres ou de la
        // ponctuation en son sein trahissent un horaire ou une URL.
        if !label
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-' || c == '\'')
        {
            continue;
        }

        let key = normalize(label);
        if key.is_empty() || out.iter().any(|p| p.key == key) {
            continue;
        }
        out.push(Property {
            key,
            label: label.to_string(),
            value: value.to_string(),
        });
    }

    out
}

/// Réduit une clé à sa forme comparable : minuscules, sans accent, sans espaces
/// superflus. « Matière », « matiere » et « MATIÈRE » désignent le même champ.
pub fn normalize(text: &str) -> String {
    text.trim()
        .chars()
        .flat_map(|c| unaccent(c.to_ascii_lowercase()))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Les accents du français, ramenés à leur lettre nue.
///
/// Une table plutôt qu'une dépendance de normalisation Unicode : on compare des
/// intitulés d'ENT français, pas du texte arbitraire.
fn unaccent(c: char) -> impl Iterator<Item = char> {
    let replacement = match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => "a",
        'ç' => "c",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'î' | 'ï' | 'í' | 'ì' => "i",
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' => "o",
        'ù' | 'û' | 'ü' | 'ú' => "u",
        'ÿ' | 'ý' => "y",
        'ñ' => "n",
        'œ' => "oe",
        'æ' => "ae",
        _ => return OneOrTwo::One(Some(c)),
    };
    OneOrTwo::Many(replacement.chars())
}

/// Un caractère peut en donner deux — « œ » devient « oe ».
enum OneOrTwo {
    One(Option<char>),
    Many(std::str::Chars<'static>),
}

impl Iterator for OneOrTwo {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        match self {
            OneOrTwo::One(c) => c.take(),
            OneOrTwo::Many(chars) => chars.next(),
        }
    }
}
