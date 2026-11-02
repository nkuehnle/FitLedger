//! Extracts set/rep targets such as "3x8-10" from free-text exercise notes.
//!
//! Notes are a mix of targets, setup info and training log ("Add 5 next week"), so the parser is
//! conservative: it marks a result `certain` only when the target is at the start of the note and
//! reads unambiguously as sets × reps. Anything else is returned with `certain = false` for review.

use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Range {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedTarget {
    pub sets: Option<Range>,
    pub reps: Option<Range>,
    /// The note with the target removed (candidate setup note). Empty if nothing remains.
    pub remainder: String,
    pub certain: bool,
}

const MAX_SETS: u32 = 6;
const MAX_REPS: u32 = 50;

fn range(c: &regex::Captures, lo: usize, hi: usize) -> Range {
    let min: u32 = c[lo].parse().unwrap_or(0);
    let max = c.get(hi).and_then(|m| m.as_str().parse().ok()).unwrap_or(min);
    Range { min, max }
}

fn clean_remainder(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '-')).to_string()
}

/// Returns `None` when the note contains nothing that looks like a target.
pub fn parse_target(note: &str) -> Option<ParsedTarget> {
    static SXR: OnceLock<Regex> = OnceLock::new();
    static REPS: OnceLock<Regex> = OnceLock::new();
    static LOOKS_LIKE: OnceLock<Regex> = OnceLock::new();
    let r = r"(\d{1,3})(?:\s*-\s*(\d{1,3}))?";
    let sxr = SXR.get_or_init(|| Regex::new(&format!(r"(?i){r}\s*[x×]\s*{r}\b")).unwrap());
    let reps_only = REPS.get_or_init(|| Regex::new(r"^\s*(\d{1,2})\s*-\s*(\d{1,2})\b").unwrap());
    let looks_like = LOOKS_LIKE.get_or_init(|| Regex::new(r"(?i)^\s*\d+\s*[x×]").unwrap());

    let sets_x_reps = |c: &regex::Captures| {
        let m = c.get(0).unwrap();
        let first = range(&c, 1, 2);
        let second = range(&c, 3, 4);
        let remainder = clean_remainder(&format!("{} {}", &note[..m.start()], &note[m.end()..]));
        let at_start = note[..m.start()].trim().is_empty();
        let unreadable = ParsedTarget { sets: None, reps: None, remainder: note.trim().into(), certain: false };
        let valid = |r: Range| r.min > 0 && r.min <= r.max;
        if !valid(first) || !valid(second) {
            return unreadable;
        }
        // Normally sets × reps. "6-8 x 3-4" reads more like reps × sets: swap, but flag it.
        let (sets, reps, plausible) = if first.max <= MAX_SETS && second.max <= MAX_REPS {
            (first, second, true)
        } else if second.max <= MAX_SETS && first.max <= MAX_REPS {
            (second, first, false)
        } else {
            return unreadable;
        };
        ParsedTarget { sets: Some(sets), reps: Some(reps), remainder, certain: plausible && at_start }
    };

    // 0. weight × sets × reps at the start ("150x3x25 machine 2"): the first number is a load.
    static WSR: OnceLock<Regex> = OnceLock::new();
    let wsr = WSR.get_or_init(|| Regex::new(r"(?i)^\s*\d+(?:\.\d+)?\s*[x×]\s*(\d)\s*[x×]\s*(\d{1,3})(?:\s*-\s*(\d{1,3}))?\b").unwrap());
    if let Some(c) = wsr.captures(note) {
        let sets = range(&c, 1, 1);
        let reps = range(&c, 2, 3);
        if sets.min > 0 && sets.max <= MAX_SETS && reps.min > 0 && reps.min <= reps.max && reps.max <= MAX_REPS {
            let remainder = clean_remainder(&note[c.get(0).unwrap().end()..]);
            return Some(ParsedTarget { sets: Some(sets), reps: Some(reps), remainder, certain: true });
        }
    }

    let sxr_match = sxr.captures(note);
    // 1. sets × reps at the start of the note
    if let Some(c) = sxr_match.as_ref().filter(|c| note[..c.get(0).unwrap().start()].trim().is_empty()) {
        return Some(sets_x_reps(c));
    }
    // 2. a rep range at the start ("8-10. Warmed up 2x15 at 60": the 2x15 is a log, not the target)
    if let Some(c) = reps_only.captures(note) {
        let reps = range(&c, 1, 2);
        if reps.min > 0 && reps.min < reps.max && reps.max <= MAX_REPS {
            let remainder = clean_remainder(&note[c.get(0).unwrap().end()..]);
            return Some(ParsedTarget { sets: None, reps: Some(reps), remainder, certain: true });
        }
    }
    // 3. sets × reps somewhere later in the note: never certain
    if let Some(c) = &sxr_match {
        return Some(sets_x_reps(c));
    }
    // Starts like a target but couldn't be read ("3xWhatever feels right").
    looks_like.is_match(note).then(|| ParsedTarget {
        sets: None,
        reps: None,
        remainder: note.trim().into(),
        certain: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(min: u32, max: u32) -> Option<Range> {
        Some(Range { min, max })
    }

    fn check(note: &str, sets: Option<Range>, reps: Option<Range>, remainder: &str, certain: bool) {
        let p = parse_target(note).unwrap_or_else(|| panic!("no target in {note:?}"));
        assert_eq!(
            (p.sets, p.reps, p.remainder.as_str(), p.certain),
            (sets, reps, remainder, certain),
            "{note:?}"
        );
    }

    #[test]
    fn clear_targets() {
        check("3x8-10", r(3, 3), r(8, 10), "", true);
        check("3 x 8-12 ", r(3, 3), r(8, 12), "", true);
        check("2-3 x 8-12 slow negatives", r(2, 3), r(8, 12), "slow negatives", true);
        check("3 x 6 -10 with 3 dropsets", r(3, 3), r(6, 10), "with 3 dropsets", true);
        check("4x8 with warmup", r(4, 4), r(8, 8), "with warmup", true);
        check("3x20-25. Use the blue band", r(3, 3), r(20, 25), "Use the blue band", true);
        check("3-4 x 12 light ball ", r(3, 4), r(12, 12), "light ball", true);
    }

    #[test]
    fn weight_sets_reps() {
        check("150x3x25 machine 2", r(3, 3), r(25, 25), "machine 2", true);
        check("62.5 x 3 x 8-10", r(3, 3), r(8, 10), "", true);
    }

    #[test]
    fn reps_only() {
        check("6-8", None, r(6, 8), "", true);
        check("8-10. Warmed up 2x15 at 60", None, r(8, 10), "Warmed up 2x15 at 60", true);
    }

    #[test]
    fn uncertain() {
        // reps × sets order
        check("6-8 x 3-4", r(3, 4), r(6, 8), "", false);
        // target not at the start
        check("Seated 3-4 x 6-8 try 80", r(3, 4), r(6, 8), "Seated try 80", false);
        // looks like a target, isn't one
        check("3xWhatever feels right", None, None, "3xWhatever feels right", false);
    }

    #[test]
    fn no_target() {
        for n in ["Rope attachment", "200 felt heavy", "Add 5 next week", "Weight is per hand"] {
            assert_eq!(parse_target(n), None, "{n:?}");
        }
    }
}
