//! Strength trends from logged sets: each session's best estimated 1RM, comparisons that stay
//! within a rep range, and plateaus.
//!
//! Estimated 1RM formulas drift with reps (a 12-rep set and a 5-rep set of the same true strength
//! give different estimates), so a change of rep range can look like progress or a slide. Where it
//! can, a comparison here only sets a rep range against itself: sets of 5–7 with sets of 5–7.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::Result;
use crate::db::Db;
use crate::stats::{e1rm, rep_bucket};

/// Sets above this many reps say little about strength and are left out.
pub const MAX_TREND_REPS: i64 = 15;
/// A change this large between two windows means the exercise or its logging changed.
pub const MAX_PLAUSIBLE_CHANGE: f64 = 0.25;
/// A new best must beat the old one by this much to count as progress.
pub const PROGRESS_STEP: f64 = 0.01;
/// A plateau is this many sessions in a row without progress...
pub const STALL_SESSIONS: usize = 4;
/// ...spread over at least this many days.
pub const STALL_DAYS: i64 = 28;

/// One session of one exercise.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionBest {
    pub date: NaiveDate,
    /// Best estimated 1RM of the session's working sets (kg, in the exercise's load mode).
    pub e1rm: f64,
    /// Best estimated 1RM per rep range (see `stats::rep_bucket`).
    pub by_reps: BTreeMap<i64, f64>,
}

/// How strength moved between two groups of sessions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    /// Fraction: 0.02 = 2 % stronger.
    pub change: f64,
    /// Compared within shared rep ranges; `false` when the two groups had none in common and the
    /// overall bests were compared instead.
    pub matched: bool,
}

/// A run of sessions without a new best.
#[derive(Debug, Clone, PartialEq)]
pub struct Stall {
    /// The session that set the best the later ones haven't beaten.
    pub since: NaiveDate,
    /// Sessions after it.
    pub sessions: usize,
    pub peak: f64,
}

fn best_by_reps(sessions: &[SessionBest]) -> BTreeMap<i64, f64> {
    let mut out: BTreeMap<i64, f64> = BTreeMap::new();
    for s in sessions {
        for (&b, &v) in &s.by_reps {
            let e = out.entry(b).or_insert(0.0);
            *e = e.max(v);
        }
    }
    out
}

/// Strength change from `before` to `after`: the median change of the rep ranges both have, else
/// the change of the overall best. `None` without data on both sides, or when the change is too
/// large to be the muscle (see `MAX_PLAUSIBLE_CHANGE`).
pub fn matched_change(before: &[SessionBest], after: &[SessionBest]) -> Option<Change> {
    let (b, a) = (best_by_reps(before), best_by_reps(after));
    let mut shared: Vec<f64> = b.iter().filter_map(|(k, &v0)| a.get(k).filter(|_| v0 > 0.0).map(|&v1| v1 / v0 - 1.0)).collect();
    let (change, matched) = if shared.is_empty() {
        let top = |s: &[SessionBest]| s.iter().map(|x| x.e1rm).fold(0.0, f64::max);
        let (v0, v1) = (top(before), top(after));
        if v0 <= 0.0 || v1 <= 0.0 {
            return None;
        }
        (v1 / v0 - 1.0, false)
    } else {
        shared.sort_by(f64::total_cmp);
        let n = shared.len();
        (if n % 2 == 1 { shared[n / 2] } else { (shared[n / 2 - 1] + shared[n / 2]) / 2.0 }, true)
    };
    (change.abs() <= MAX_PLAUSIBLE_CHANGE).then_some(Change { change, matched })
}

/// The plateau the sessions end in, if any: at least `STALL_SESSIONS` sessions over `STALL_DAYS`
/// days since the best was last raised by `PROGRESS_STEP`. Pass a recent window (a few months),
/// oldest first, so that rebuilding after a break or a cut counts as progress.
pub fn stall(sessions: &[SessionBest]) -> Option<Stall> {
    // `mark` is the best at the last session that counted as progress, so a slow creep of small
    // gains still adds up to progress.
    let mut mark = 0.0_f64;
    let mut at = 0;
    for (i, s) in sessions.iter().enumerate() {
        if i == 0 || s.e1rm >= mark * (1.0 + PROGRESS_STEP) {
            at = i;
            mark = s.e1rm;
        }
    }
    let last = sessions.last()?;
    let after = &sessions[at + 1..];
    if after.len() < STALL_SESSIONS || (last.date - sessions[at].date).num_days() < STALL_DAYS {
        return None;
    }
    // Climbing back towards the old best (after a break, a deload or a cut) isn't a plateau.
    let best = |s: &[SessionBest]| s.iter().map(|x| x.e1rm).fold(0.0, f64::max);
    let (early, late) = after.split_at(after.len() / 2);
    if best(late) >= best(early) * (1.0 + PROGRESS_STEP) {
        return None;
    }
    Some(Stall { since: sessions[at].date, sessions: after.len(), peak: best(sessions) })
}

pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    Some(if n % 2 == 1 { values[n / 2] } else { (values[n / 2 - 1] + values[n / 2]) / 2.0 })
}

impl Db {
    /// Every exercise's sessions in a local date range (inclusive), oldest first. Warm-ups, sets
    /// without load and sets above `MAX_TREND_REPS` reps are left out.
    pub fn session_bests(&self, from: Option<&str>, to: Option<&str>) -> Result<HashMap<i64, Vec<SessionBest>>> {
        let formula = self.formula()?;
        let mut out: HashMap<i64, Vec<SessionBest>> = HashMap::new();
        let mut current: HashMap<i64, i64> = HashMap::new();
        for s in self.work_sets(None, from, to)? {
            let (Some(ex), Some(w), Some(r)) = (s.exercise_id, s.weight_kg, s.reps) else { continue };
            if s.set_type == "warmup" || r > MAX_TREND_REPS {
                continue;
            }
            let (Some(e), Some(bucket)) = (e1rm(w, r, formula), rep_bucket(r)) else { continue };
            let list = out.entry(ex).or_default();
            if current.insert(ex, s.workout_exercise_id) != Some(s.workout_exercise_id) {
                list.push(SessionBest { date: s.date(), e1rm: 0.0, by_reps: BTreeMap::new() });
            }
            let sess = list.last_mut().expect("just pushed");
            sess.e1rm = sess.e1rm.max(e);
            let b = sess.by_reps.entry(bucket).or_insert(0.0);
            *b = b.max(e);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn sess(date: &str, sets: &[(i64, f64)]) -> SessionBest {
        SessionBest {
            date: d(date),
            e1rm: sets.iter().map(|s| s.1).fold(0.0, f64::max),
            by_reps: sets.iter().copied().collect(),
        }
    }

    #[test]
    fn compares_within_a_rep_range() {
        // Moved from 5s to 12s: the 12-rep estimate reads higher, but the 5s only went up 2 %.
        let before = [sess("2026-08-01", &[(5, 100.0)])];
        let after = [sess("2026-09-01", &[(5, 102.0), (12, 115.0)])];
        let c = matched_change(&before, &after).unwrap();
        assert!(c.matched);
        assert!((c.change - 0.02).abs() < 1e-9);
    }

    #[test]
    fn falls_back_to_the_overall_best() {
        let before = [sess("2026-08-01", &[(5, 100.0)])];
        let after = [sess("2026-09-01", &[(10, 104.0)])];
        let c = matched_change(&before, &after).unwrap();
        assert!(!c.matched);
        assert!((c.change - 0.04).abs() < 1e-9);
        // Too big to be real, and nothing to compare.
        assert_eq!(matched_change(&before, &[sess("2026-09-01", &[(5, 140.0)])]), None);
        assert_eq!(matched_change(&before, &[]), None);
    }

    #[test]
    fn plateau_needs_sessions_and_time() {
        let run = |values: &[f64]| -> Vec<SessionBest> {
            values.iter().enumerate().map(|(i, &v)| SessionBest { date: d("2026-08-03") + chrono::Duration::days(7 * i as i64), e1rm: v, by_reps: BTreeMap::new() }).collect()
        };
        // Still climbing.
        assert_eq!(stall(&run(&[100.0, 102.0, 104.0, 106.0, 108.0])), None);
        // Four sessions and four weeks at or under the best (a 0.5 % wobble isn't progress).
        let s = stall(&run(&[100.0, 104.0, 104.0, 103.0, 104.5, 104.0])).unwrap();
        assert_eq!((s.since, s.sessions), (d("2026-08-10"), 4));
        assert!((s.peak - 104.5).abs() < 1e-9);
        // Three sessions isn't enough yet.
        assert_eq!(stall(&run(&[100.0, 104.0, 104.0, 103.0, 104.0])), None);
        // A slow creep adds up to progress.
        assert_eq!(stall(&run(&[100.0, 100.6, 101.2, 101.8, 102.4, 103.0])), None);
        // Building back after a drop is progress, not a plateau.
        assert_eq!(stall(&run(&[100.0, 90.0, 92.0, 94.0, 96.0, 98.0])), None);
    }
}
