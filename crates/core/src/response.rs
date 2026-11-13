//! Volume against progress: for one muscle, how much strength moved in each stretch of training
//! and how many weekly sets it got in that stretch. Plotted, it shows your own dose-response:
//! where more sets came with more progress and where they stopped paying.
//!
//! History is cut into `BLOCK_WEEKS`-week blocks counting back from today. A block's progress is
//! the median strength change, against the block before, of the exercises that train the muscle
//! as a primary mover (compared within rep ranges, see `trend`). It is a description of what
//! happened, not a controlled comparison: diet, sleep and exercise choice changed between blocks
//! too, which is why each point carries the goal phase it fell in.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::Result;
use crate::db::Db;
use crate::planning::Goal;
use crate::trend::{SessionBest, matched_change, median};

pub const BLOCK_WEEKS: i64 = 6;

#[derive(Debug, Clone, Serialize)]
pub struct ResponsePoint {
    /// First and last day of the block.
    pub start: String,
    pub end: String,
    /// Average weekly sets for the muscle in the block (a secondary muscle counts as set in
    /// `secondary_set_weight`).
    pub sets_per_week: f64,
    /// Strength change against the block before: 0.02 = 2 % stronger.
    pub change: f64,
    /// Exercises the change is the median of.
    pub exercises: usize,
    /// The goal in force in the middle of the block.
    pub goal: Option<Goal>,
}

impl Db {
    /// One point per block that has a strength comparison, oldest first.
    pub fn volume_response(&self, muscle: &str, today: NaiveDate) -> Result<Vec<ResponsePoint>> {
        let days = BLOCK_WEEKS * 7;
        // Block 0 ends today; block k covers the `days` days before block k − 1.
        let block_of = |d: NaiveDate| (today - d).num_days().div_euclid(days);
        let to = today.to_string();

        let mut sets: HashMap<i64, f64> = HashMap::new();
        for w in self.weekly_muscle_sets(None, Some(&to))? {
            if w.muscle_id != muscle {
                continue;
            }
            // A week belongs to the block its Thursday falls in.
            let mid = NaiveDate::parse_from_str(&w.week_start, "%Y-%m-%d").expect("stored dates are valid") + Duration::days(3);
            *sets.entry(block_of(mid.min(today))).or_default() += w.sets;
        }

        let primary: Vec<i64> =
            self.muscle_map()?.into_iter().filter(|(_, ms)| ms.iter().any(|(m, p)| *p && m == muscle)).map(|(ex, _)| ex).collect();
        let bests = self.session_bests(None, Some(&to))?;
        let mut changes: HashMap<i64, Vec<f64>> = HashMap::new();
        for ex in primary {
            let Some(sessions) = bests.get(&ex) else { continue };
            let mut by_block: HashMap<i64, Vec<SessionBest>> = HashMap::new();
            for s in sessions {
                by_block.entry(block_of(s.date)).or_default().push(s.clone());
            }
            for (k, after) in &by_block {
                let Some(before) = by_block.get(&(k + 1)) else { continue };
                if let Some(c) = matched_change(before, after) {
                    changes.entry(*k).or_default().push(c.change);
                }
            }
        }

        let mut blocks: Vec<i64> = changes.keys().copied().collect();
        blocks.sort_by_key(|k| std::cmp::Reverse(*k));
        let mut out = Vec::new();
        for k in blocks {
            let end = today - Duration::days(k * days);
            let start = end - Duration::days(days - 1);
            let mut c = changes.remove(&k).expect("listed");
            let exercises = c.len();
            out.push(ResponsePoint {
                start: start.to_string(),
                end: end.to_string(),
                sets_per_week: sets.get(&k).copied().unwrap_or(0.0) / BLOCK_WEEKS as f64,
                change: median(&mut c).expect("not empty"),
                exercises,
                goal: self.goal_on(start + Duration::days(days / 2))?.map(|g| g.goal),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn log(db: &Db, date: NaiveDate, slug: &str, kg: f64, sets: usize) {
        let ex = db.exercise_id(slug).unwrap().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO workout (name, started_at_local, finished) VALUES ('W', ?1, 1)", [format!("{date} 08:00:00")]).unwrap();
        let w = c.last_insert_rowid();
        c.execute("INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)", params![w, slug, ex]).unwrap();
        let we = c.last_insert_rowid();
        for i in 0..sets {
            c.execute(
                "INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps, completed) VALUES (?1, ?2, 'normal', ?3, 8, 1)",
                params![we, i as i64, kg],
            )
            .unwrap();
        }
    }

    #[test]
    fn blocks_pair_weekly_sets_with_the_change_they_came_with() {
        let mut db = Db::open_in_memory().unwrap();
        let today = d("2026-11-08"); // a Sunday, so blocks are whole weeks
        // Three 6-week blocks of weekly curls: 3 sets at 20 kg, then 6 sets at 21 kg (+5 %),
        // then 6 sets still at 21 kg.
        for week in 0..18 {
            let date = today - Duration::days(7 * (17 - week) + 3);
            let (kg, sets) = if week < 6 { (20.0, 3) } else { (21.0, 6) };
            log(&db, date, "bicep-curl-dumbbell", kg, sets);
        }
        db.set_goal(Goal::Gain, today - Duration::days(70), None).unwrap();
        let r = db.volume_response("biceps", today).unwrap();
        // The first block has nothing before it to compare with.
        assert_eq!(r.len(), 2);
        assert!((r[0].sets_per_week - 6.0).abs() < 1e-9 && (r[0].change - 0.05).abs() < 1e-9);
        assert!((r[1].sets_per_week - 6.0).abs() < 1e-9 && r[1].change.abs() < 1e-9);
        assert_eq!((r[0].goal, r[1].goal), (Some(Goal::Gain), Some(Goal::Gain)));
        assert_eq!(r[1].end, "2026-11-08");
        assert_eq!(r[0].exercises, 1);
        // A muscle the curls don't train as a primary mover has no points.
        assert!(db.volume_response("quads", today).unwrap().is_empty());
    }
}
