//! Le moteur de règles visuelles.
//!
//! Un emploi du temps d'ENT arrive avec des intitulés bruts — « CM - Algèbre
//! linéaire (Amphi B) » — et rien qui distingue un amphi d'un TP. Plutôt que de
//! recolorier chaque séance à la main, on décrit une fois la règle qui les
//! reconnaît, et le cœur la rejoue sur tout ce qui entre.
//!
//! Le module est volontairement sans état ni base de données : il décide, le
//! stockage écrit. C'est ce qui le rend testable ligne à ligne.

use std::collections::HashMap;

use crate::model::{Rule, RuleField, RuleMatch, RuleOutcome, RuleSuggestion};

/// Les champs d'une occurrence soumis aux règles.
#[derive(Debug, Clone, Copy)]
pub struct Fields<'a> {
    pub calendar_id: &'a str,
    pub title: &'a str,
    pub location: &'a str,
    pub description: &'a str,
}

/// Vrai si la règle reconnaît cette occurrence.
pub fn matches(rule: &Rule, fields: &Fields<'_>) -> bool {
    if !rule.enabled || rule.pattern.trim().is_empty() {
        return false;
    }
    if let Some(scope) = &rule.calendar_id
        && scope != fields.calendar_id
    {
        return false;
    }

    let haystacks: &[&str] = match rule.field {
        RuleField::Title => &[fields.title],
        RuleField::Location => &[fields.location],
        RuleField::Description => &[fields.description],
        RuleField::Any => &[fields.title, fields.location, fields.description],
    };

    haystacks
        .iter()
        .any(|text| matches_text(text, &rule.pattern, rule.match_kind, rule.case_sensitive))
}

fn matches_text(text: &str, pattern: &str, kind: RuleMatch, case_sensitive: bool) -> bool {
    let (text, pattern) = if case_sensitive {
        (text.to_string(), pattern.to_string())
    } else {
        (text.to_lowercase(), pattern.to_lowercase())
    };
    let text = text.trim();
    let pattern = pattern.trim();

    match kind {
        RuleMatch::Contains => text.contains(pattern),
        RuleMatch::StartsWith => text.starts_with(pattern),
        RuleMatch::EndsWith => text.ends_with(pattern),
        RuleMatch::Equals => text == pattern,
        RuleMatch::Word => tokenize(text).any(|token| token == pattern),
    }
}

/// Découpe un intitulé en mots, en tenant les séparateurs qu'affectionnent les
/// ENT : tirets, deux-points, parenthèses, barres obliques.
pub fn tokenize(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
}

/// Rejoue toutes les règles sur une occurrence et retient la décision finale.
///
/// L'ordre est celui de `priority` croissante ; chaque règle n'écrase que ce
/// qu'elle renseigne, si bien qu'une règle de couleur et une règle de masquage
/// se cumulent au lieu de se disputer.
pub fn apply(rules: &[Rule], fields: &Fields<'_>) -> RuleOutcome {
    let mut outcome = RuleOutcome::default();

    for rule in rules {
        if !matches(rule, fields) {
            continue;
        }
        if rule.category_id.is_some() {
            outcome.category_id = rule.category_id.clone();
        }
        if let Some(template) = &rule.rename_to
            && !template.trim().is_empty()
        {
            outcome.display_title = Some(render_title(template, fields.title));
        }
        if rule.hide {
            outcome.hidden = true;
        }
    }

    outcome
}

/// `{}` dans un modèle de renommage reprend le titre d'origine, ce qui permet
/// de préfixer sans retaper l'intitulé : « ★ {} ».
fn render_title(template: &str, original: &str) -> String {
    if template.contains("{}") {
        template.replace("{}", original)
    } else {
        template.to_string()
    }
}

/// Un titre observé dans les données, matière première des suggestions.
#[derive(Debug, Clone)]
pub struct Sample {
    pub title: String,
}

/// Combien d'occurrences au minimum pour qu'un motif mérite une règle.
const MIN_OCCURRENCES: usize = 2;
/// Au-delà, la liste devient un mur : on garde les motifs les plus porteurs.
const MAX_SUGGESTIONS: usize = 12;

/// Mots qui reviennent partout sans rien classer.
const STOP_WORDS: [&str; 23] = [
    "de", "du", "des", "la", "le", "les", "et", "en", "au", "aux", "un", "une", "sur", "pour",
    "avec", "par", "dans", "gr", "groupe", "salle", "cours", "semaine", "s",
];

/// Marqueurs de type reconnus d'emblée, même isolés dans un intitulé.
const KNOWN_TYPES: [(&str, &str); 13] = [
    ("cm", "cours magistral"),
    ("td", "travaux dirigés"),
    ("tp", "travaux pratiques"),
    ("ds", "devoir surveillé"),
    ("cc", "contrôle continu"),
    ("examen", "examen"),
    ("partiel", "partiel"),
    ("projet", "projet"),
    ("soutenance", "soutenance"),
    ("stage", "stage"),
    ("amphi", "amphi"),
    ("conference", "conférence"),
    ("conférence", "conférence"),
];

/// Déduit des règles plausibles de ce qui a été importé.
///
/// La logique tient en une phrase : un mot qui revient dans plusieurs séances
/// mais pas dans toutes découpe l'emploi du temps, donc mérite une couleur. Les
/// motifs déjà couverts par une règle existante sont écartés — proposer deux
/// fois la même chose ferait perdre confiance dans la liste.
pub fn suggest(samples: &[Sample], existing: &[Rule], palette: &[u32]) -> Vec<RuleSuggestion> {
    if samples.is_empty() {
        return Vec::new();
    }

    let covered: Vec<String> = existing
        .iter()
        .map(|rule| rule.pattern.trim().to_lowercase())
        .collect();

    let mut counts: HashMap<String, (usize, Vec<String>)> = HashMap::new();
    for sample in samples {
        // Un même mot répété dans un titre ne compte qu'une fois : on cherche
        // combien de séances il touche, pas combien de fois il est écrit.
        let mut seen: Vec<String> = Vec::new();
        for token in tokenize(&sample.title) {
            let key = token.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());

            if !is_candidate(token, &key) {
                continue;
            }
            let entry = counts.entry(key).or_insert_with(|| (0, Vec::new()));
            entry.0 += 1;
            if entry.1.len() < 3 && !entry.1.contains(&sample.title) {
                entry.1.push(sample.title.clone());
            }
        }
    }

    let total = samples.len();
    let mut ranked: Vec<(String, usize, Vec<String>)> = counts
        .into_iter()
        .filter(|(key, (count, _))| {
            *count >= MIN_OCCURRENCES && *count < total && !covered.contains(key)
        })
        .map(|(key, (count, samples))| (key, count, samples))
        .collect();

    // Un marqueur de type prime sur un mot de matière à effectif égal : il
    // classe l'emploi du temps entier, là où « Algèbre » ne classe qu'un cours.
    ranked.sort_by(|a, b| {
        let type_a = is_known_type(&a.0);
        let type_b = is_known_type(&b.0);
        type_b.cmp(&type_a).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0))
    });
    ranked.truncate(MAX_SUGGESTIONS);

    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (pattern, count, samples))| RuleSuggestion {
            label: label_for(&pattern),
            field: RuleField::Title,
            match_kind: RuleMatch::Word,
            pattern: pattern.clone(),
            occurrences: count as u32,
            samples,
            suggested_color: palette[index % palette.len().max(1)],
        })
        .collect()
}

/// Un mot mérite d'être proposé s'il ressemble à un marqueur de type, ou à une
/// matière : un sigle court en majuscules, ou un mot assez long pour être parlant.
fn is_candidate(token: &str, lowered: &str) -> bool {
    if STOP_WORDS.contains(&lowered) {
        return false;
    }
    if lowered.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if is_known_type(lowered) {
        return true;
    }
    let acronym = token
        .chars()
        .all(|c| c.is_uppercase() || c.is_ascii_digit());
    match token.chars().count() {
        0..=1 => false,
        2..=3 => acronym,
        _ => true,
    }
}

fn is_known_type(lowered: &str) -> bool {
    KNOWN_TYPES.iter().any(|(key, _)| *key == lowered)
}

fn label_for(pattern: &str) -> String {
    match KNOWN_TYPES.iter().find(|(key, _)| *key == pattern) {
        Some((_, human)) => format!("Les séances de {human}"),
        None => format!("Les séances marquées « {pattern} »"),
    }
}
