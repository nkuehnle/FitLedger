//! Strength balance: which muscles and movements are strong or weak relative to the rest of you.
//!
//! Each lift you've trained lately that has reference data (`norms::links`) is put on one scale:
//! your best recent estimated 1RM divided by the median competitor's, at your sex, age and
//! bodyweight, converted to your exercise. The absolute number matters little (competitors are a
//! strong crowd); what matters is how your lifts compare with each other on it. A muscle's
//! standing is the average of the lifts that train it, against the middle of all your lifts.
//!
//! The comparisons are only as good as the conversions behind them, so they are weighted: a
//! secondary muscle counts half, a rough conversion (`Reference::rough`) half, and a machine or
//! cable exercise half again, since those differ from gym to gym. One lift can't swing a muscle
//! by more than `MAX_SWING`, and the centre is a median so an odd machine doesn't move it.
//!
//! Left/right differences come straight from your own tape measurements and InBody scans.
//!
//! The cut-offs (10 % for a muscle or movement to count as behind or ahead, 3 % for a tape
//! difference and 5 % for lean mass to count as noticeable) are choices made here, not from a
//! study.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use crate::Result;
use crate::db::Db;
use crate::library::library;
use crate::norms::{Sex, Who, exercise_references};
use crate::stats::e1rm;

/// Lifts count if trained within this many days.
const RECENT_DAYS: i64 = 90;
/// Sets above this many reps make a poor 1RM estimate.
const MAX_REPS: i64 = 12;
/// The most one lift counts as ahead or behind when averaged into a muscle or movement.
const MAX_SWING: f64 = 0.5;
/// A muscle or movement this far from your average is behind or ahead.
pub const NOTABLE: f64 = 0.10;
const TAPE_NOTABLE: f64 = 0.03;
const LEAN_NOTABLE: f64 = 0.05;
/// Left and right tape readings must be taken within this many days of each other.
const PAIR_DAYS: i64 = 14;
/// Dumbbell exercises done with one dumbbell: the logged weight is the whole load.
const ONE_DUMBBELL: &[&str] = &[
    "bent-over-one-arm-row-dumbbell",
    "concentration-curl-dumbbell",
    "pullover-dumbbell",
    "overhead-triceps-extension-dumbbell",
    "single-leg-romanian-deadlift-dumbbell",
    "goblet-squat-dumbbell",
];

#[derive(Debug, Clone, Serialize)]
pub struct LiftStanding {
    pub exercise_id: i64,
    pub name: String,
    pub pattern: Option<String>,
    /// Your best recent estimated 1RM ÷ the median competitor's.
    pub ratio: f64,
    /// That against the middle of all your lifts: 0.1 = 10 % ahead.
    pub balance: f64,
    pub percentile: Option<f64>,
    /// The comparison rests on a rough conversion.
    pub rough: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MuscleStanding {
    pub muscle_id: String,
    /// Against the average of all your lifts: −0.1 = 10 % behind.
    pub balance: f64,
    /// "behind" | "even" | "ahead"
    pub standing: &'static str,
    /// The lifts behind the number, most telling first.
    pub lifts: Vec<String>,
}

/// Two movement groups side by side, each against the average of all your lifts.
#[derive(Debug, Clone, Serialize)]
pub struct Pair {
    pub a: &'static str,
    pub b: &'static str,
    pub a_balance: f64,
    pub b_balance: f64,
    pub a_lifts: usize,
    pub b_lifts: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Side {
    /// "Bicep", "Arm lean mass", …
    pub label: String,
    /// "tape" (cm) or "inbody" (kg)
    pub source: &'static str,
    pub left: f64,
    pub right: f64,
    /// (right − left) ÷ their mean: positive = right is bigger.
    pub diff: f64,
    pub notable: bool,
    pub date: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StrengthBalance {
    /// Why there are no lifts to compare: "profile" (sex or birth year not set), "bodyweight"
    /// (no weigh-ins) or "lifts" (fewer than three recent lifts with reference data).
    pub missing: Option<&'static str>,
    pub lifts: Vec<LiftStanding>,
    pub muscles: Vec<MuscleStanding>,
    pub pairs: Vec<Pair>,
    pub sides: Vec<Side>,
}

fn standing(balance: f64) -> &'static str {
    if balance <= -NOTABLE {
        "behind"
    } else if balance >= NOTABLE {
        "ahead"
    } else {
        "even"
    }
}

/// Mean of (value, weight) pairs.
fn weighted(items: &[(f64, f64)]) -> Option<f64> {
    let total: f64 = items.iter().map(|x| x.1).sum();
    (total > 0.0).then(|| items.iter().map(|x| x.0 * x.1).sum::<f64>() / total)
}

/// Median of (value, weight) pairs: the value where half the weight lies below.
fn weighted_median(items: &[(f64, f64)]) -> Option<f64> {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = sorted.iter().map(|x| x.1).sum::<f64>() / 2.0;
    let mut seen = 0.0;
    sorted.into_iter().find(|x| {
        seen += x.1;
        seen >= half
    }).map(|x| x.0)
}

/// (right − left) ÷ their mean.
pub fn side_diff(left: f64, right: f64) -> f64 {
    (right - left) / ((left + right) / 2.0)
}

impl Db {
    fn who(&self, today: NaiveDate) -> Result<std::result::Result<Who, &'static str>> {
        let sex = match self.setting("sex")?.as_deref() {
            Some("male") => Sex::Male,
            Some("female") => Sex::Female,
            _ => return Ok(Err("profile")),
        };
        let Some(year) = self.setting("birth_year")?.and_then(|y| y.parse::<i32>().ok()) else { return Ok(Err("profile")) };
        let Some(bw) = self.bodyweight_at(today)? else { return Ok(Err("bodyweight")) };
        Ok(Ok(Who { sex, age: (today.year() - year) as f64, bodyweight_kg: Some(bw) }))
    }

    fn sides(&self, today: NaiveDate) -> Result<Vec<Side>> {
        let mut out = Vec::new();
        // Tape: the latest left and right readings of each paired part.
        let mut latest: HashMap<String, (NaiveDate, f64)> = HashMap::new();
        for m in self.measurements(None)? {
            let Ok(d) = NaiveDate::parse_from_str(&m.at[..10], "%Y-%m-%d") else { continue };
            if d <= today && latest.get(&m.part).is_none_or(|x| x.0 <= d) {
                latest.insert(m.part, (d, m.value));
            }
        }
        for part in ["bicep", "forearm", "thigh", "calf"] {
            let (Some(&(dl, l)), Some(&(dr, r))) = (latest.get(&format!("left_{part}")), latest.get(&format!("right_{part}"))) else { continue };
            if (dl - dr).num_days().abs() > PAIR_DAYS || l <= 0.0 || r <= 0.0 {
                continue;
            }
            let diff = side_diff(l, r);
            out.push(Side {
                label: part[..1].to_uppercase() + &part[1..],
                source: "tape",
                left: l,
                right: r,
                diff,
                notable: diff.abs() >= TAPE_NOTABLE,
                date: dl.max(dr).to_string(),
            });
        }
        // InBody: the latest scan with both sides.
        let scans = self.inbody_scans()?;
        for (label, l, r) in [("Arm lean mass", "lean_left_arm_kg", "lean_right_arm_kg"), ("Leg lean mass", "lean_left_leg_kg", "lean_right_leg_kg")] {
            let Some((scan, lv, rv)) = scans
                .iter()
                .filter(|s| s.measured_at_local[..10] <= *today.to_string())
                .filter_map(|s| Some((s, *s.values.get(l)?, *s.values.get(r)?)))
                .filter(|x| x.1 > 0.0 && x.2 > 0.0)
                .max_by(|a, b| a.0.measured_at_local.cmp(&b.0.measured_at_local))
            else {
                continue;
            };
            let diff = side_diff(lv, rv);
            out.push(Side {
                label: label.to_string(),
                source: "inbody",
                left: lv,
                right: rv,
                diff,
                notable: diff.abs() >= LEAN_NOTABLE,
                date: scan.measured_at_local[..10].to_string(),
            });
        }
        Ok(out)
    }

    pub fn strength_balance(&self, today: NaiveDate) -> Result<StrengthBalance> {
        let sides = self.sides(today)?;
        let who = match self.who(today)? {
            Ok(w) => w,
            Err(missing) => return Ok(StrengthBalance { missing: Some(missing), sides, ..Default::default() }),
        };

        // Best recent estimated 1RM per exercise.
        let formula = self.formula()?;
        let from = (today - Duration::days(RECENT_DAYS)).to_string();
        let mut best: HashMap<i64, f64> = HashMap::new();
        for s in self.work_sets(None, Some(&from), Some(&today.to_string()))? {
            let (Some(ex), Some(w), Some(r)) = (s.exercise_id, s.weight_kg, s.reps) else { continue };
            if s.set_type == "warmup" || r > MAX_REPS {
                continue;
            }
            if let Some(e) = e1rm(w, r, formula) {
                let b = best.entry(ex).or_insert(0.0);
                *b = b.max(e);
            }
        }

        struct Lift {
            standing: LiftStanding,
            primary: Vec<String>,
            secondary: Vec<String>,
            weight: f64,
        }
        let mut lifts: Vec<Lift> = Vec::new();
        for e in self.list_exercises(true)? {
            let Some(&top) = best.get(&e.id) else { continue };
            // Per-hand weights are compared as both hands together, as the references measure.
            let both = if e.load_mode == "per_hand" && !ONE_DUMBBELL.contains(&e.slug.as_str()) { 2.0 } else { 1.0 };
            let value = top * both;
            // The competition reference (it has tiers), not self-reported gym bests.
            let Some(r) = exercise_references(&e.slug, &who, &[value], &[]).into_iter().find(|r| r.unit == "kg" && !r.tiers.is_empty()) else {
                continue;
            };
            let Some(median) = r.points.iter().find(|p| p.0 == 50.0).map(|p| p.1).filter(|m| *m > 0.0) else { continue };
            lifts.push(Lift {
                standing: LiftStanding {
                    exercise_id: e.id,
                    name: e.name,
                    pattern: e.pattern,
                    ratio: value / median,
                    balance: 0.0,
                    percentile: r.ranks.first().copied().flatten(),
                    rough: r.rough,
                },
                primary: e.primary,
                secondary: e.secondary,
                weight: if r.rough { 0.5 } else { 1.0 } * if matches!(e.equipment.as_deref(), Some("machine" | "cable")) { 0.5 } else { 1.0 },
            });
        }
        if lifts.len() < 3 {
            return Ok(StrengthBalance { missing: Some("lifts"), sides, ..Default::default() });
        }
        let overall = weighted_median(&lifts.iter().map(|l| (l.standing.ratio, l.weight)).collect::<Vec<_>>()).expect("has lifts");
        for l in &mut lifts {
            l.standing.balance = l.standing.ratio / overall - 1.0;
        }
        let swing = |l: &Lift| l.standing.balance.clamp(-MAX_SWING, MAX_SWING);

        // Muscles: lifts that train it, a secondary muscle counting half. One secondary lift alone
        // says too little.
        let mut muscles = Vec::new();
        for m in &library().muscles {
            let mut items: Vec<(&Lift, f64)> = lifts
                .iter()
                .filter_map(|l| {
                    let role = if l.primary.contains(&m.id) {
                        1.0
                    } else if l.secondary.contains(&m.id) {
                        0.5
                    } else {
                        return None;
                    };
                    Some((l, role * l.weight))
                })
                .collect();
            if items.iter().map(|x| x.1).sum::<f64>() < 0.5 {
                continue;
            }
            items.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.standing.name.cmp(&b.0.standing.name)));
            let balance = weighted(&items.iter().map(|(l, w)| (swing(l), *w)).collect::<Vec<_>>()).expect("has weight");
            muscles.push(MuscleStanding {
                muscle_id: m.id.clone(),
                balance,
                standing: standing(balance),
                lifts: items.iter().take(4).map(|(l, _)| l.standing.name.clone()).collect(),
            });
        }

        // Movements.
        let mut by_pattern: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
        for l in &lifts {
            if let Some(p) = l.standing.pattern.as_deref() {
                by_pattern.entry(p).or_default().push((swing(l), l.weight));
            }
        }
        let group = |patterns: &[&str]| -> Option<(f64, usize)> {
            let items: Vec<(f64, f64)> = patterns.iter().filter_map(|p| by_pattern.get(p)).flatten().copied().collect();
            weighted(&items).map(|r| (r, items.len()))
        };
        let mut pairs = Vec::new();
        for (a, pa, b, pb) in [
            ("Push", &["horizontal_push", "vertical_push"][..], "Pull", &["horizontal_pull", "vertical_pull"][..]),
            ("Upper body", &["horizontal_push", "vertical_push", "horizontal_pull", "vertical_pull"][..], "Lower body", &["squat", "hinge", "lunge"][..]),
            ("Squat", &["squat", "lunge"][..], "Hinge", &["hinge"][..]),
        ] {
            if let (Some((a_balance, a_lifts)), Some((b_balance, b_lifts))) = (group(pa), group(pb)) {
                pairs.push(Pair { a, b, a_balance, b_balance, a_lifts, b_lifts });
            }
        }

        let mut lifts: Vec<LiftStanding> = lifts.into_iter().map(|l| l.standing).collect();
        lifts.sort_by(|a, b| a.balance.total_cmp(&b.balance));
        Ok(StrengthBalance { missing: None, lifts, muscles, pairs, sides })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    const TODAY: &str = "2026-11-09";

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn log(db: &Db, date: &str, slug: &str, kg: f64, reps: i64) {
        let ex = db.exercise_id(slug).unwrap().unwrap_or_else(|| panic!("no exercise {slug}"));
        let c = db.conn();
        c.execute("INSERT INTO workout (name, started_at_local, finished) VALUES ('W', ?1, 1)", [format!("{date} 08:00:00")]).unwrap();
        let w = c.last_insert_rowid();
        c.execute("INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)", params![w, slug, ex]).unwrap();
        let we = c.last_insert_rowid();
        c.execute(
            "INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps, completed) VALUES (?1, 0, 'normal', ?2, ?3, 1)",
            params![we, kg, reps],
        )
        .unwrap();
    }

    fn profile(db: &Db) {
        db.set_setting("sex", "male").unwrap();
        db.set_setting("birth_year", "1991").unwrap();
        db.conn().execute("INSERT INTO bodyweight (measured_at_local, weight_kg, source) VALUES ('2026-11-01 08:00:00', 85.0, 'manual')", []).unwrap();
    }

    #[test]
    fn says_what_is_missing() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.strength_balance(d(TODAY)).unwrap().missing, Some("profile"));
        db.set_setting("sex", "male").unwrap();
        db.set_setting("birth_year", "1991").unwrap();
        assert_eq!(db.strength_balance(d(TODAY)).unwrap().missing, Some("bodyweight"));
        profile(&db);
        assert_eq!(db.strength_balance(d(TODAY)).unwrap().missing, Some("lifts"));
    }

    #[test]
    fn a_weak_squat_puts_the_legs_behind() {
        let db = Db::open_in_memory().unwrap();
        profile(&db);
        // A decent bench and deadlift for 85 kg, and a squat far below either.
        log(&db, "2026-10-20", "bench-press-barbell", 100.0, 5);
        log(&db, "2026-10-21", "deadlift-barbell", 170.0, 5);
        log(&db, "2026-10-22", "squat-barbell", 70.0, 5);
        // Too old to count.
        log(&db, "2026-05-01", "squat-barbell", 160.0, 5);
        let b = db.strength_balance(d(TODAY)).unwrap();
        assert_eq!(b.missing, None);
        assert_eq!(b.lifts.len(), 3);
        // Most behind first.
        assert_eq!(b.lifts[0].name, "Squat (Barbell)");
        assert!(b.lifts[0].balance < -NOTABLE);
        let muscle = |id: &str| b.muscles.iter().find(|m| m.muscle_id == id).unwrap_or_else(|| panic!("no {id}"));
        assert_eq!(muscle("quads").standing, "behind");
        assert!(muscle("lower_chest").balance > muscle("quads").balance + NOTABLE);
        // Balances are against your own average, so they straddle zero.
        assert!(b.lifts.iter().any(|l| l.balance > 0.0) && b.lifts.iter().any(|l| l.balance < 0.0));
        let legs = b.pairs.iter().find(|p| p.a == "Squat").unwrap();
        assert!(legs.a_balance < legs.b_balance);
    }

    #[test]
    fn left_and_right() {
        assert!((side_diff(38.0, 40.0) - 2.0 / 39.0).abs() < 1e-9);
        let db = Db::open_in_memory().unwrap();
        let add = |part: &str, at: &str, v: f64| {
            db.conn().execute("INSERT INTO body_measurement (measured_at_local, part, value) VALUES (?1, ?2, ?3)", params![at, part, v]).unwrap();
        };
        add("left_bicep", "2026-11-04 08:00:00", 38.0);
        add("right_bicep", "2026-11-04 08:00:00", 40.0);
        // A left reading with no right one near it in time is left out.
        add("left_calf", "2026-11-04 08:00:00", 38.0);
        add("right_calf", "2026-06-01 08:00:00", 38.5);
        let b = db.strength_balance(d(TODAY)).unwrap();
        assert_eq!(b.sides.len(), 1);
        assert_eq!((b.sides[0].label.as_str(), b.sides[0].notable), ("Bicep", true));
        assert!(b.sides[0].diff > 0.0);
    }
}
