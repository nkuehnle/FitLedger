//! Fatigue check: are many lifts down at once, and do the watch's recovery numbers agree?
//!
//! One bad session means little. Several lifts slipping together over two weeks, with heart-rate
//! variability, resting heart rate or sleep off their usual, is the pattern a lighter week is for.
//!
//! - **Lifts**: each exercise's sessions of the last `RECENT_DAYS` days against the
//!   `BASELINE_DAYS` before, as the typical (median) session best within a shared rep range. A
//!   lift is down at `LIFT_DOWN` or more, unless its latest session is already back up on the one
//!   before (you're rebuilding, e.g. after a lighter week).
//! - **Recovery**: the last 7 days against the 28 before, per measure, when the watch gives
//!   enough days of each.
//!
//! The thresholds and the rule that combines them are choices made here, not from a study.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::Result;
use crate::db::Db;
use crate::planning::Goal;
use crate::trend::{SessionBest, median};

const RECENT_DAYS: i64 = 14;
const BASELINE_DAYS: i64 = 42;
/// A lift's typical session this far below its baseline is down.
pub const LIFT_DOWN: f64 = -0.025;
/// Lifts with data needed before the share of them that is down means anything.
const MIN_LIFTS: usize = 4;
const HRV_DOWN: f64 = -0.08;
const RESTING_HR_UP: f64 = 3.0;
const SLEEP_DOWN_S: f64 = -45.0 * 60.0;

#[derive(Debug, Clone, Serialize)]
pub struct LiftChange {
    pub exercise_id: i64,
    pub name: String,
    /// Typical session of the last two weeks against the six before: −0.03 = 3 % down.
    pub change: f64,
    pub down: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoverySign {
    /// "hrv" | "resting_hr" | "sleep"
    pub kind: &'static str,
    pub off: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FatigueReport {
    /// "ok" | "watch" | "deload"
    pub level: &'static str,
    /// One sentence for the Today card; empty when `level` is "ok".
    pub summary: String,
    pub lifts_down: usize,
    pub lifts_tracked: usize,
    /// Most down first.
    pub lifts: Vec<LiftChange>,
    pub recovery: Vec<RecoverySign>,
}

/// Change in the typical session best from `before` to `after`, within the rep ranges both have.
fn typical_change(before: &[SessionBest], after: &[SessionBest]) -> Option<f64> {
    let levels = |s: &[SessionBest]| -> BTreeMap<i64, f64> {
        let mut by: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
        for x in s {
            for (&b, &v) in &x.by_reps {
                by.entry(b).or_default().push(v);
            }
        }
        by.into_iter().filter_map(|(b, mut v)| median(&mut v).map(|m| (b, m))).collect()
    };
    let (b, a) = (levels(before), levels(after));
    let mut changes: Vec<f64> = b.iter().filter_map(|(k, &v0)| a.get(k).filter(|_| v0 > 0.0).map(|&v1| v1 / v0 - 1.0)).collect();
    median(&mut changes)
}

/// The level from how many lifts are down and how many recovery measures are off.
pub fn fatigue_level(down: usize, tracked: usize, off: usize) -> &'static str {
    let share = if tracked >= MIN_LIFTS { down as f64 / tracked as f64 } else { 0.0 };
    if down >= 3 && (share >= 0.5 || (share >= 1.0 / 3.0 && off >= 1)) {
        "deload"
    } else if (down >= 2 && share >= 1.0 / 3.0) || off >= 2 {
        "watch"
    } else {
        "ok"
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

impl Db {
    pub fn fatigue(&self, today: NaiveDate) -> Result<FatigueReport> {
        // ---- Lifts ----
        let split = today - Duration::days(RECENT_DAYS);
        let from = (split - Duration::days(BASELINE_DAYS)).to_string();
        let bests = self.session_bests(Some(&from), Some(&today.to_string()))?;
        let mut names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        {
            let mut stmt = self.conn().prepare("SELECT id, name FROM exercise")?;
            for row in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))? {
                let (id, name) = row?;
                names.insert(id, name);
            }
        }
        let mut lifts = Vec::new();
        for (ex, sessions) in &bests {
            let (before, after): (Vec<SessionBest>, Vec<SessionBest>) = sessions.iter().cloned().partition(|s| s.date <= split);
            if before.len() < 2 || after.is_empty() {
                continue;
            }
            let Some(change) = typical_change(&before, &after) else { continue };
            // Already climbing again: the dip is behind you.
            let rebuilding = match &sessions[..] {
                [.., prev, last] => last.e1rm >= prev.e1rm * 1.01,
                _ => false,
            };
            lifts.push(LiftChange {
                exercise_id: *ex,
                name: names.get(ex).cloned().unwrap_or_default(),
                change,
                down: change <= LIFT_DOWN && !rebuilding,
            });
        }
        lifts.sort_by(|a, b| a.change.total_cmp(&b.change).then_with(|| a.name.cmp(&b.name)));
        let down = lifts.iter().filter(|l| l.down).count();
        let tracked = lifts.len();

        // ---- Recovery ----
        let week = (today - Duration::days(6)).to_string();
        let base = (today - Duration::days(34)).to_string();
        // One value per day and measure (the first source that has it).
        let mut recent: [Vec<f64>; 3] = Default::default();
        let mut usual: [Vec<f64>; 3] = Default::default();
        let mut seen: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
        for d in self.day_health()? {
            if d.date.as_str() < base.as_str() || d.date.as_str() > today.to_string().as_str() {
                continue;
            }
            for (i, v) in [d.hrv_ms, d.resting_hr.map(|x| x as f64), d.sleep_s.map(|x| x as f64)].into_iter().enumerate() {
                let Some(v) = v else { continue };
                if seen.insert((d.date.clone(), i)) {
                    if d.date.as_str() >= week.as_str() { recent[i].push(v) } else { usual[i].push(v) }
                }
            }
        }
        let mut recovery = Vec::new();
        let enough = |i: usize| recent[i].len() >= 4 && usual[i].len() >= 10;
        if enough(0) {
            let (r, u) = (mean(&recent[0]), mean(&usual[0]));
            let ch = r / u - 1.0;
            recovery.push(RecoverySign { kind: "hrv", off: ch <= HRV_DOWN, detail: format!("HRV {r:.0} ms this week, usually {u:.0}") });
        }
        if enough(1) {
            let (r, u) = (mean(&recent[1]), mean(&usual[1]));
            recovery.push(RecoverySign {
                kind: "resting_hr",
                off: r - u >= RESTING_HR_UP,
                detail: format!("resting heart rate {r:.0} this week, usually {u:.0}"),
            });
        }
        if enough(2) {
            let (r, u) = (mean(&recent[2]), mean(&usual[2]));
            let hm = |s: f64| format!("{}h {:02}m", (s / 3600.0) as i64, ((s % 3600.0) / 60.0) as i64);
            recovery.push(RecoverySign { kind: "sleep", off: r - u <= SLEEP_DOWN_S, detail: format!("sleep {} a night this week, usually {}", hm(r), hm(u)) });
        }
        let off: Vec<&RecoverySign> = recovery.iter().filter(|s| s.off).collect();

        let level = fatigue_level(down, tracked, off.len());
        let summary = if level == "ok" {
            String::new()
        } else {
            let mut parts = Vec::new();
            if down >= 2 {
                let typical = {
                    let mut c: Vec<f64> = lifts.iter().filter(|l| l.down).map(|l| l.change).collect();
                    median(&mut c).unwrap_or(0.0)
                };
                parts.push(format!("{down} of {tracked} lifts are down about {:.0}% over the last two weeks", (typical * 100.0).abs()));
            }
            parts.extend(off.iter().map(|s| s.detail.clone()));
            let mut text = parts.join("; ");
            if let Some(first) = text.get(..1).map(str::to_uppercase) {
                text.replace_range(..1, &first);
            }
            let advice = match (level, self.current_goal(today)?) {
                ("deload", Some(Goal::Cut)) => "Some of that is the cut, but a lighter week would help.",
                ("deload", _) => "A lighter week (about half the sets, a little less weight) usually brings them back.",
                (_, Some(Goal::Cut)) => "Expected on a cut; ease off if it keeps sliding.",
                _ => "Keep an eye on it; ease off if it keeps sliding.",
            };
            format!("{text}. {advice}")
        };
        Ok(FatigueReport { level, summary, lifts_down: down, lifts_tracked: tracked, lifts, recovery })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn level_needs_several_lifts_or_recovery_signs() {
        assert_eq!(fatigue_level(1, 8, 0), "ok");
        assert_eq!(fatigue_level(3, 8, 0), "watch");
        assert_eq!(fatigue_level(3, 8, 1), "deload");
        assert_eq!(fatigue_level(5, 8, 0), "deload");
        // Too few lifts to say; recovery numbers alone can only raise a watch.
        assert_eq!(fatigue_level(3, 3, 0), "ok");
        assert_eq!(fatigue_level(0, 0, 2), "watch");
    }

    fn log(db: &Db, date: NaiveDate, slug: &str, kg: f64) {
        let ex = db.exercise_id(slug).unwrap().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO workout (name, started_at_local, finished) VALUES ('W', ?1, 1)", [format!("{date} 08:00:00")]).unwrap();
        let w = c.last_insert_rowid();
        c.execute("INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)", params![w, slug, ex]).unwrap();
        let we = c.last_insert_rowid();
        c.execute(
            "INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps, completed) VALUES (?1, 0, 'normal', ?2, 8, 1)",
            params![we, kg],
        )
        .unwrap();
    }

    const LIFTS: [&str; 4] = ["bench-press-barbell", "squat-barbell", "deadlift-barbell", "bicep-curl-dumbbell"];

    /// Eight weekly sessions of four lifts ending the day before `today`; `last_two` scales the final two weeks.
    fn history(db: &Db, today: NaiveDate, last_two: [f64; 2]) {
        for week in 0..8 {
            let date = today - Duration::days(7 * (7 - week) + 1);
            let scale = if week >= 6 { last_two[week as usize - 6] } else { 1.0 };
            for (i, slug) in LIFTS.iter().enumerate() {
                log(db, date, slug, (40.0 + 20.0 * i as f64) * scale);
            }
        }
    }

    #[test]
    fn steady_training_is_ok() {
        let db = Db::open_in_memory().unwrap();
        history(&db, d("2026-11-09"), [1.0, 1.0]);
        let r = db.fatigue(d("2026-11-09")).unwrap();
        assert_eq!((r.level, r.lifts_tracked, r.lifts_down), ("ok", 4, 0));
        assert!(r.summary.is_empty() && r.recovery.is_empty());
    }

    #[test]
    fn lifts_sliding_together_with_low_hrv_call_for_a_deload() {
        let db = Db::open_in_memory().unwrap();
        let today = d("2026-11-09");
        history(&db, today, [0.96, 0.95]);
        for i in 0..35 {
            let date = (today - Duration::days(i)).to_string();
            let hrv = if i < 7 { 50.0 } else { 60.0 };
            db.conn().execute("INSERT INTO day_health (date, source, hrv_ms, resting_hr, sleep_s) VALUES (?1, 'watch', ?2, 52, 27000)", params![date, hrv]).unwrap();
        }
        let r = db.fatigue(today).unwrap();
        assert_eq!((r.level, r.lifts_down), ("deload", 4));
        assert!(r.summary.starts_with("4 of 4 lifts are down about 5% over the last two weeks; HRV 50 ms this week, usually 60."), "{}", r.summary);
        let off: Vec<&str> = r.recovery.iter().filter(|s| s.off).map(|s| s.kind).collect();
        assert_eq!(off, ["hrv"]);
    }

    #[test]
    fn a_dip_you_are_already_climbing_out_of_is_not_flagged() {
        let db = Db::open_in_memory().unwrap();
        // Two weeks ago everything dropped 10 %; last week it came back up 4 %.
        history(&db, d("2026-11-09"), [0.90, 0.94]);
        let r = db.fatigue(d("2026-11-09")).unwrap();
        assert_eq!((r.level, r.lifts_down), ("ok", 0));
        assert!(r.lifts.iter().all(|l| l.change < LIFT_DOWN));
    }
}
