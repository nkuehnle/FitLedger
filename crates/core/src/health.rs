//! "Progress health" per muscle: blends whatever signals there are (weekly volume against target,
//! strength trend, tape measurements, InBody segmental lean mass) into one score in [-1, 1].
//! What counts as success depends on the goal: on a cut, holding strength and size is a win;
//! on a gain, they should be going up.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::db::Db;
use crate::planning::Goal;
use crate::stats::{Formula, WorkSet, e1rm};
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct HealthSignal {
    /// "volume" | "strength" | "size" | "lean_mass"
    pub kind: &'static str,
    /// -1 (going badly) … 1 (going well).
    pub score: f64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MuscleHealth {
    pub muscle_id: String,
    /// Weighted mean of the signals; None when there's no data at all.
    pub score: Option<f64>,
    pub signals: Vec<HealthSignal>,
}

const VOLUME_WEEKS: i64 = 4;
/// Strength compares the best e1RM of the last 6 weeks with the 6 weeks before.
pub(crate) const STRENGTH_DAYS: i64 = 42;

fn clamp(v: f64) -> f64 {
    v.clamp(-1.0, 1.0)
}

/// Weekly sets against the (min, max) target: 1 inside, falling to -1 at 0 sets or 1.5× the max.
pub fn volume_score(per_week: f64, (lo, hi): (f64, f64)) -> f64 {
    if per_week < lo {
        clamp(2.0 * per_week / lo - 1.0)
    } else if per_week <= hi {
        1.0
    } else {
        clamp(1.0 - 2.0 * (per_week - hi) / (0.5 * hi))
    }
}

/// Strength change (fraction over ~6 weeks). Gain: +2.5% is great, flat is neutral. Maintain:
/// flat is good (0.5), +1.5% great. Cut: holding is good (0.6), −3% neutral, gaining great.
pub fn strength_score(change: f64, goal: Option<Goal>) -> f64 {
    match goal {
        Some(Goal::Gain) | None => clamp(change / 0.025),
        Some(Goal::Maintain) => clamp((change + 0.015) / 0.03),
        Some(Goal::Cut) => clamp((change + 0.03) / 0.05),
    }
}

/// Size change of a limb/torso measurement (fraction per month). Same idea as strength.
pub fn size_score(per_month: f64, goal: Option<Goal>) -> f64 {
    match goal {
        Some(Goal::Gain) | None => clamp(per_month / 0.01),
        Some(Goal::Maintain) => clamp((per_month + 0.01) / 0.01),
        Some(Goal::Cut) => clamp((per_month + 0.02) / 0.02),
    }
}

/// Waist (fraction per month): shrinking is the point of a cut; on a gain some growth is expected.
pub fn waist_score(per_month: f64, goal: Option<Goal>) -> f64 {
    match goal {
        Some(Goal::Cut) => clamp(-per_month / 0.01),
        Some(Goal::Maintain) => clamp(1.0 - per_month.abs() / 0.01),
        Some(Goal::Gain) | None => clamp(1.0 - per_month.max(0.0) / 0.015),
    }
}

/// Tape-measure parts that reflect each muscle.
fn tape_parts(muscle: &str) -> &'static [&'static str] {
    match muscle {
        "upper_chest" | "lower_chest" => &["chest"],
        "front_delts" | "side_delts" | "rear_delts" => &["shoulders"],
        "biceps" | "triceps" => &["left_bicep", "right_bicep"],
        "forearm_flexors" | "forearm_extensors" => &["left_forearm", "right_forearm"],
        "abs" | "obliques" => &["waist"],
        "glutes" => &["hips"],
        "quads" | "hamstrings" | "adductors" | "abductors" => &["left_thigh", "right_thigh"],
        "calves" => &["left_calf", "right_calf"],
        "neck" | "traps" => &["neck"],
        _ => &[],
    }
}

/// InBody segmental lean-mass fields for each muscle.
fn lean_fields(muscle: &str) -> &'static [&'static str] {
    match muscle {
        "biceps" | "triceps" | "forearm_flexors" | "forearm_extensors" => &["lean_left_arm_kg", "lean_right_arm_kg"],
        "quads" | "hamstrings" | "adductors" | "abductors" | "glutes" | "calves" => &["lean_left_leg_kg", "lean_right_leg_kg"],
        "upper_chest" | "lower_chest" | "lats" | "upper_back" | "traps" | "lower_back" | "abs" | "obliques" | "front_delts"
        | "side_delts" | "rear_delts" => &["lean_trunk_kg"],
        _ => &[],
    }
}

/// Monthly fractional change between the latest value and one at least 4 weeks earlier
/// (the latest must be from the last 120 days). Averages over the given series.
fn monthly_change(series: &[Vec<(NaiveDate, f64)>], today: NaiveDate) -> Option<f64> {
    let mut changes = Vec::new();
    for s in series {
        let Some(&(d1, v1)) = s.last() else { continue };
        if (today - d1).num_days() > 120 {
            continue;
        }
        // Baseline: the latest reading at least 28 days before, within a year.
        let Some(&(d0, v0)) = s.iter().rev().find(|(d, _)| (d1 - *d).num_days() >= 28 && (d1 - *d).num_days() <= 365) else {
            continue;
        };
        let months = (d1 - d0).num_days() as f64 / 30.4;
        changes.push((v1 / v0 - 1.0) / months);
    }
    (!changes.is_empty()).then(|| changes.iter().sum::<f64>() / changes.len() as f64)
}

/// A strength change bigger than this means the exercise or its logging changed, not the muscle.
pub(crate) const IMPLAUSIBLE_CHANGE: f64 = 0.25;

/// The exercise and estimated 1RM of a set that can show strength: not a warm-up or a high-rep set.
pub(crate) fn strength_point(s: &WorkSet, formula: Formula) -> Option<(i64, f64)> {
    let (ex, w, r) = (s.exercise_id?, s.weight_kg?, s.reps?);
    if r > 15 || s.set_type == "warmup" {
        return None;
    }
    Some((ex, e1rm(w, r, formula)?))
}

fn pct(v: f64) -> String {
    format!("{}{:.1}%", if v >= 0.0 { "+" } else { "−" }, (v * 100.0).abs())
}

impl Db {
    pub fn progress_health(&self, today: NaiveDate) -> Result<Vec<MuscleHealth>> {
        let goal = self.current_goal(today)?;
        let targets: HashMap<String, (f64, f64)> =
            self.muscle_targets(today)?.into_iter().map(|t| (t.muscle_id, (t.min_sets, t.max_sets))).collect();

        // Volume: average weekly sets over the last 4 weeks.
        let from = (today - Duration::days(VOLUME_WEEKS * 7 - 1)).to_string();
        let weekly: HashMap<String, f64> = self
            .muscle_sets(Some(&from), Some(&today.to_string()))?
            .into_iter()
            .map(|m| (m.muscle_id, m.sets / VOLUME_WEEKS as f64))
            .collect();

        // Strength: per primary exercise, best e1RM in the last 6 weeks vs the 6 before.
        let formula = self.formula()?;
        let split = today - Duration::days(STRENGTH_DAYS);
        let start = (split - Duration::days(STRENGTH_DAYS)).to_string();
        let mut best: HashMap<i64, (f64, f64)> = HashMap::new();
        for s in self.work_sets(None, Some(&start), Some(&today.to_string()))? {
            let Some((ex, e)) = strength_point(&s, formula) else { continue };
            let b = best.entry(ex).or_insert((0.0, 0.0));
            if s.date() > split {
                b.1 = b.1.max(e);
            } else {
                b.0 = b.0.max(e);
            }
        }
        let mut strength: HashMap<String, Vec<f64>> = HashMap::new();
        for (ex, muscles) in self.muscle_map()? {
            let Some(&(before, now)) = best.get(&ex) else { continue };
            if before <= 0.0 || now <= 0.0 {
                continue;
            }
            let change = now / before - 1.0;
            if change.abs() > IMPLAUSIBLE_CHANGE {
                continue;
            }
            for (m, primary) in muscles {
                if primary {
                    strength.entry(m).or_default().push(change);
                }
            }
        }

        // Tape measurements and InBody lean mass, by part/field.
        let mut tape: HashMap<String, Vec<(NaiveDate, f64)>> = HashMap::new();
        for m in self.measurements(None)? {
            if let Ok(d) = NaiveDate::parse_from_str(&m.at[..10], "%Y-%m-%d") {
                tape.entry(m.part).or_default().push((d, m.value));
            }
        }
        let mut lean: HashMap<String, Vec<(NaiveDate, f64)>> = HashMap::new();
        for scan in self.inbody_scans()? {
            let Ok(d) = NaiveDate::parse_from_str(&scan.measured_at_local[..10], "%Y-%m-%d") else { continue };
            for (k, v) in scan.values {
                if k.starts_with("lean_") && k.ends_with("_kg") {
                    lean.entry(k).or_default().push((d, v));
                }
            }
        }

        let mut out = Vec::new();
        let mut stmt = self.conn().prepare("SELECT id FROM muscle ORDER BY position")?;
        let muscles: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        for muscle in muscles {
            let mut signals = Vec::new();
            let v = weekly.get(&muscle).copied().unwrap_or(0.0);
            // A muscle with no minimum that isn't trained (e.g. neck) says nothing.
            if let Some(&(lo, hi)) = targets.get(&muscle).filter(|t| t.1 > 0.0 && (t.0 > 0.0 || v > 0.0)) {
                signals.push(HealthSignal {
                    kind: "volume",
                    score: volume_score(v, (lo, hi)),
                    detail: format!("{v:.1} sets/week over the last 4 weeks (target {lo}–{hi})"),
                });
            }
            if let Some(ch) = strength.get(&muscle).filter(|c| !c.is_empty()) {
                let mut c = ch.clone();
                c.sort_by(f64::total_cmp);
                let median = c[c.len() / 2];
                signals.push(HealthSignal {
                    kind: "strength",
                    score: strength_score(median, goal),
                    detail: format!("estimated 1RM {} vs the previous 6 weeks ({} exercises)", pct(median), c.len()),
                });
            }
            let parts = tape_parts(&muscle);
            let series: Vec<_> = parts.iter().filter_map(|p| tape.get(*p).cloned()).collect();
            if let Some(ch) = monthly_change(&series, today) {
                let waist = parts == ["waist"];
                signals.push(HealthSignal {
                    kind: "size",
                    score: if waist { waist_score(ch, goal) } else { size_score(ch, goal) },
                    detail: format!("{} {} a month", parts[0].replace("left_", "").replace('_', " "), pct(ch)),
                });
            }
            let series: Vec<_> = lean_fields(&muscle).iter().filter_map(|f| lean.get(*f).cloned()).collect();
            if let Some(ch) = monthly_change(&series, today) {
                signals.push(HealthSignal {
                    kind: "lean_mass",
                    score: size_score(ch, goal),
                    detail: format!("InBody lean mass {} a month", pct(ch)),
                });
            }
            let weight = |k: &str| if k == "strength" { 1.5 } else { 1.0 };
            let total: f64 = signals.iter().map(|s| weight(s.kind)).sum();
            let score = (total > 0.0).then(|| signals.iter().map(|s| s.score * weight(s.kind)).sum::<f64>() / total);
            out.push(MuscleHealth { muscle_id: muscle, score, signals });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_depend_on_goal() {
        // Holding strength: great on a cut, neutral on a gain.
        assert!((strength_score(0.0, Some(Goal::Cut)) - 0.6).abs() < 1e-9);
        assert_eq!(strength_score(0.02, Some(Goal::Cut)), 1.0);
        assert_eq!(strength_score(0.0, Some(Goal::Gain)), 0.0);
        assert_eq!(strength_score(0.025, Some(Goal::Gain)), 1.0);
        // A shrinking waist is good on a cut only.
        assert_eq!(waist_score(-0.01, Some(Goal::Cut)), 1.0);
        assert!(waist_score(-0.01, Some(Goal::Maintain)) < 0.5);
        assert_eq!(size_score(-0.02, Some(Goal::Cut)), 0.0);
    }

    #[test]
    fn volume_against_target() {
        assert_eq!(volume_score(12.0, (10.0, 20.0)), 1.0);
        assert_eq!(volume_score(0.0, (10.0, 20.0)), -1.0);
        assert_eq!(volume_score(5.0, (10.0, 20.0)), 0.0);
        assert_eq!(volume_score(30.0, (10.0, 20.0)), -1.0);
    }

    #[test]
    fn monthly_change_needs_a_baseline() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let today = d("2026-11-02");
        let s = vec![vec![(d("2026-08-01"), 40.0), (d("2026-10-31"), 41.0)]];
        let c = monthly_change(&s, today).unwrap();
        assert!((c - 0.025 / (91.0 / 30.4)).abs() < 1e-9);
        assert_eq!(monthly_change(&[vec![(d("2026-10-20"), 40.0), (d("2026-10-31"), 41.0)]], today), None);
    }
}
