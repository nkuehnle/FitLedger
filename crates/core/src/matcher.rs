//! Matches exercise names from other apps (e.g. Strong's "Chest Press (Machine)") to the library.

use std::collections::HashSet;

use serde::Serialize;

/// Matches at or above this confidence are accepted without review.
pub const AUTO_ACCEPT: f64 = 0.9;

/// Something a name can be matched against: a library exercise with its names and equipment.
pub struct Candidate<'a> {
    pub slug: &'a str,
    pub names: Vec<&'a str>,
    pub equipment: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    pub slug: String,
    pub confidence: f64,
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Lowercase word tokens with light normalisation: plural "s" dropped, common spellings unified.
fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "with" | "the" | "a" | "on" | "of"))
        .map(|w| {
            let w = match w {
                "dumbell" | "db" => "dumbbell",
                "bb" => "barbell",
                "kb" => "kettlebell",
                "sl" => "single",
                "rdl" => "romanian",
                "ohp" => "overhead",
                "tricep" => "triceps",
                "bicep" => "biceps",
                "pushdown" | "pressdown" => "pushdown",
                "pulldowns" => "pulldown",
                "flye" | "flyes" => "fly",
                _ => w,
            };
            let w = if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") { &w[..w.len() - 1] } else { w };
            w.to_string()
        })
        .collect()
}

fn dice<T: Eq + std::hash::Hash>(a: &HashSet<T>, b: &HashSet<T>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    2.0 * a.intersection(b).count() as f64 / (a.len() + b.len()) as f64
}

fn trigrams(s: &str) -> HashSet<[u8; 3]> {
    let b: Vec<u8> = format!("  {s} ").into_bytes();
    b.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

/// Equipment named in parentheses or as a word, mapped to library equipment ids.
fn equipment_hint(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let checks: &[(&str, &str)] = &[
        ("smith", "smith_machine"),
        ("landmine", "landmine"),
        ("trap bar", "trap_bar"),
        ("hex bar", "trap_bar"),
        ("ez bar", "ez_bar"),
        ("kettlebell", "kettlebell"),
        ("dumbbell", "dumbbell"),
        ("dumbell", "dumbbell"),
        ("barbell", "barbell"),
        ("cable", "cable"),
        ("band", "band"),
        ("plate loaded", "machine"),
        ("machine", "machine"),
        ("bodyweight", "bodyweight"),
    ];
    checks.iter().find(|(k, _)| n.contains(k)).map(|&(_, v)| v)
}

struct PreparedName {
    norm: String,
    word_key: Vec<String>,
    tokens: HashSet<String>,
    trigrams: HashSet<[u8; 3]>,
}

impl PreparedName {
    fn new(name: &str) -> Self {
        let tokens: Vec<String> = tokens(name);
        let mut word_key = tokens.clone();
        word_key.sort();
        word_key.dedup();
        let norm = normalize(name);
        Self { trigrams: trigrams(&norm), norm, word_key, tokens: tokens.into_iter().collect() }
    }
}

struct Entry {
    slug: String,
    equipment: String,
    names: Vec<PreparedName>,
}

/// Candidates preprocessed once so many names can be matched quickly.
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    pub fn new(candidates: &[Candidate]) -> Self {
        let entries = candidates
            .iter()
            .map(|c| Entry {
                slug: c.slug.to_string(),
                equipment: c.equipment.to_string(),
                names: c.names.iter().map(|n| PreparedName::new(n)).collect(),
            })
            .collect();
        Self { entries }
    }

    /// Ranked suggestions for `name`, best first. Exact name/alias matches score 1.0.
    pub fn suggest(&self, name: &str, limit: usize) -> Vec<Suggestion> {
        let q = PreparedName::new(name);
        let hint = equipment_hint(name);
        let mut scored: Vec<Suggestion> = self
            .entries
            .iter()
            .map(|c| {
                let mut best = 0.0f64;
                for n in &c.names {
                    if n.norm == q.norm {
                        best = 1.0;
                        break;
                    }
                    let s = if n.word_key == q.word_key {
                        0.97
                    } else {
                        0.6 * dice(&q.tokens, &n.tokens) + 0.4 * dice(&q.trigrams, &n.trigrams)
                    };
                    best = best.max(s);
                }
                if best < 1.0 {
                    match hint {
                        Some(h) if h == c.equipment => best = (best + 0.05).min(0.99),
                        Some(_) => best *= 0.85,
                        None => {}
                    }
                }
                Suggestion { slug: c.slug.clone(), confidence: best }
            })
            .collect();
        scored.sort_by(|a, b| b.confidence.total_cmp(&a.confidence).then_with(|| a.slug.cmp(&b.slug)));
        scored.truncate(limit);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::library;

    fn candidates() -> Vec<Candidate<'static>> {
        library()
            .exercises
            .iter()
            .map(|e| Candidate {
                slug: &e.slug,
                names: std::iter::once(e.name.as_str()).chain(e.aliases.iter().map(String::as_str)).collect(),
                equipment: &e.equipment,
            })
            .collect()
    }

    fn top(name: &str) -> Suggestion {
        Index::new(&candidates()).suggest(name, 1).remove(0)
    }

    #[test]
    fn exact_and_alias() {
        assert_eq!(top("Chest Press (Machine)"), Suggestion { slug: "chest-press-machine".into(), confidence: 1.0 });
        assert_eq!(top("Swiss Ball Dead Bug").slug, "dead-bug");
        assert_eq!(top("ball rollout").slug, "stability-ball-rollout");
        assert_eq!(top("Landmine SL RDL").slug, "single-leg-romanian-deadlift-landmine");
        assert_eq!(top("Chest-Supported Row (Dumbell)").slug, "chest-supported-row-dumbbell");
    }

    #[test]
    fn reordered_words() {
        let s = top("Dumbbell Arnold Press");
        assert_eq!(s.slug, "arnold-press-dumbbell");
        assert!(s.confidence >= AUTO_ACCEPT);
    }

    #[test]
    fn fuzzy_names_from_other_exports() {
        for (name, slug) in [
            ("Incline Dumbell Bench Press", "incline-bench-press-dumbbell"),
            ("Lat Pulldown - Wide Grip (Cable)", "lat-pulldown-cable"),
            ("Bulgarian Split Squats", "bulgarian-split-squat-dumbbell"),
            ("Tricep Rope Pushdown", "triceps-pushdown-rope"),
            ("Seated Leg Curls (Machine)", "seated-leg-curl-machine"),
            ("Hammer Curls", "hammer-curl-dumbbell"),
        ] {
            assert_eq!(top(name).slug, slug, "{name}");
        }
    }

    #[test]
    fn equipment_disambiguates() {
        assert_eq!(top("Shrug (Barbell)").slug, "shrug-barbell");
        assert_eq!(top("Lateral Raise (Cable)").slug, "lateral-raise-cable");
        assert_eq!(top("Bicep Curl (Cable)").slug, "bicep-curl-cable");
    }

    #[test]
    fn ambiguous_names_need_review() {
        assert!(top("Triceps Extension").confidence < AUTO_ACCEPT);
        assert!(top("Evening stuff").confidence < 0.6);
    }
}
