//! Le moteur de règles visuelles.
//!
//! Un emploi du temps d'ENT arrive avec des intitulés bruts — « R3.01 DEV WEB -
//! CM (Gr A) » — mais aussi, dans sa description, avec les champs qui comptent
//! vraiment : « Type : Cours magistral », « Matière : Développement web ». Ce
//! sont eux que le moteur privilégie, parce qu'ils disent explicitement ce que
//! l'intitulé ne fait que suggérer.
//!
//! Une règle relie une condition à trois effets cumulables : poser une
//! catégorie — donc une couleur —, renommer, masquer. Le module est sans état
//! ni base de données : il décide, le stockage écrit.

use std::collections::HashMap;

use crate::model::{Rule, RuleField, RuleMatch, RuleOutcome, RuleSuggestion};
use crate::properties::{self, Property};

/// Les champs d'une séance soumis aux règles.
#[derive(Debug, Clone, Copy)]
pub struct Fields<'a> {
    pub title: &'a str,
    pub location: &'a str,
    pub description: &'a str,
    /// Champs structurés lus dans la description.
    pub properties: &'a [Property],
}

impl Fields<'_> {
    fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.value.as_str())
    }
}

/// Vrai si la règle reconnaît cette séance.
pub fn matches(rule: &Rule, fields: &Fields<'_>) -> bool {
    if !rule.enabled || rule.pattern.trim().is_empty() {
        return false;
    }

    let owned;
    let haystacks: &[&str] = match rule.field {
        RuleField::Title => &[fields.title],
        RuleField::Location => &[fields.location],
        RuleField::Description => &[fields.description],
        RuleField::Any => &[fields.title, fields.location, fields.description],
        RuleField::Property => {
            let Some(key) = &rule.property else {
                return false;
            };
            let Some(value) = fields.property(&properties::normalize(key)) else {
                return false;
            };
            owned = [value];
            &owned
        }
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

/// Rejoue toutes les règles sur une séance et retient la décision finale.
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

/// Une séance observée, matière première des suggestions.
#[derive(Debug, Clone)]
pub struct Sample {
    pub title: String,
    pub properties: Vec<Property>,
}

/// Combien de séances au minimum pour qu'un motif mérite une règle.
const MIN_OCCURRENCES: usize = 2;
/// Au-delà, la liste devient un mur : on garde les motifs les plus porteurs.
const MAX_SUGGESTIONS: usize = 12;
/// Un champ à trop de valeurs distinctes ne se colorie pas : on ne distingue
/// pas quinze couleurs d'un coup d'œil.
const MAX_DISTINCT_VALUES: usize = 12;

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
/// Les champs structurés passent avant tout : « Type : TD » dit ce qu'il est,
/// là où un mot d'intitulé ne fait que le laisser deviner. Ce n'est qu'à défaut
/// — un export qui ne renseigne rien — qu'on retombe sur l'analyse des titres.
pub fn suggest(samples: &[Sample], existing: &[Rule], palette: &[u32]) -> Vec<RuleSuggestion> {
    if samples.is_empty() {
        return Vec::new();
    }

    let from_properties = suggest_from_properties(samples, existing, palette);
    if !from_properties.is_empty() {
        return from_properties;
    }
    suggest_from_titles(samples, existing, palette)
}

/// Une valeur de champ structuré, et ce qu'on sait d'elle.
struct ValueStat {
    label: String,
    value: String,
    count: usize,
    samples: Vec<String>,
}

fn suggest_from_properties(
    samples: &[Sample],
    existing: &[Rule],
    palette: &[u32],
) -> Vec<RuleSuggestion> {
    // key -> (valeur normalisée -> statistiques)
    let mut by_key: HashMap<String, HashMap<String, ValueStat>> = HashMap::new();

    for sample in samples {
        for property in &sample.properties {
            let stats = by_key.entry(property.key.clone()).or_default();
            let entry = stats
                .entry(property.value.to_lowercase())
                .or_insert_with(|| ValueStat {
                    label: property.label.clone(),
                    value: property.value.clone(),
                    count: 0,
                    samples: Vec::new(),
                });
            entry.count += 1;
            if entry.samples.len() < 3 && !entry.samples.contains(&sample.title) {
                entry.samples.push(sample.title.clone());
            }
        }
    }

    let mut out: Vec<(usize, RuleSuggestion)> = Vec::new();
    for (key, values) in by_key {
        // Un champ qui vaut toujours la même chose ne découpe rien ; un champ
        // qui vaut autre chose à chaque séance ne se colorie pas.
        if values.len() < 2 || values.len() > MAX_DISTINCT_VALUES {
            continue;
        }
        let coverage: usize = values.values().map(|stat| stat.count).sum();

        for stat in values.into_values() {
            if stat.count < MIN_OCCURRENCES || covered(existing, Some(&key), &stat.value) {
                continue;
            }
            out.push((
                coverage,
                RuleSuggestion {
                    label: format!("{} : {}", stat.label, stat.value),
                    field: RuleField::Property,
                    property: Some(key.clone()),
                    match_kind: RuleMatch::Equals,
                    pattern: stat.value,
                    occurrences: stat.count as u32,
                    samples: stat.samples,
                    suggested_color: 0,
                },
            ));
        }
    }

    // Le champ qui couvre le plus de séances d'abord, puis ses valeurs les plus
    // fréquentes : la liste commence par ce qui change le plus l'écran.
    out.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.occurrences.cmp(&a.1.occurrences))
            .then(a.1.pattern.cmp(&b.1.pattern))
    });
    out.truncate(MAX_SUGGESTIONS);

    out.into_iter()
        .enumerate()
        .map(|(index, (_, mut suggestion))| {
            suggestion.suggested_color = pick_color(palette, index);
            suggestion
        })
        .collect()
}

fn suggest_from_titles(
    samples: &[Sample],
    existing: &[Rule],
    palette: &[u32],
) -> Vec<RuleSuggestion> {
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
            *count >= MIN_OCCURRENCES && *count < total && !covered(existing, None, key)
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
            property: None,
            match_kind: RuleMatch::Word,
            pattern,
            occurrences: count as u32,
            samples,
            suggested_color: pick_color(palette, index),
        })
        .collect()
}

/// Vrai si une règle existante traite déjà ce motif sur ce champ.
fn covered(existing: &[Rule], property: Option<&String>, pattern: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    existing.iter().any(|rule| {
        rule.pattern.trim().to_lowercase() == pattern
            && rule.property.as_deref() == property.map(|p| p.as_str())
    })
}

fn pick_color(palette: &[u32], index: usize) -> u32 {
    if palette.is_empty() {
        0xFF4C5FD5
    } else {
        palette[index % palette.len()]
    }
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
