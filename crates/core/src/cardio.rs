//! Cardio: runs, rides and rows are exercises measured in distance and time. A session is one
//! exercise in a finished workout; its segments (sets) add up. Sessions can also be logged on
//! their own, e.g. a run outside, as a one-exercise workout.

use chrono::NaiveDateTime;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::Result;

/// One cardio session: totals over its segments.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardioSession {
    pub workout_id: i64,
    pub exercise_id: i64,
    pub name: String,
    pub at: String,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    /// Average over the segments that have one.
    pub avg_hr: Option<f64>,
    pub kcal: Option<f64>,
    pub segments: i64,
}

/// A cardio session logged by hand.
#[derive(Debug, Clone, Deserialize)]
pub struct CardioEntry {
    pub exercise_id: i64,
    pub at: String,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub avg_hr: Option<i64>,
    pub kcal: Option<f64>,
    pub notes: Option<String>,
}

impl Db {
    /// Every finished cardio session (distance + time exercises), oldest first.
    pub fn cardio_sessions(&self) -> Result<Vec<CardioSession>> {
        let mut stmt = self.conn().prepare(
            "SELECT w.id, e.id, e.name, w.started_at_local, sum(s.distance_m), sum(s.duration_s), avg(s.avg_hr), sum(s.kcal), count(s.id)
             FROM workout w
             JOIN workout_exercise we ON we.workout_id = w.id
             JOIN exercise e ON e.id = we.exercise_id
             JOIN workout_set s ON s.workout_exercise_id = we.id
             WHERE w.finished = 1 AND e.kind = 'distance_duration' AND s.completed = 1 AND s.set_type != 'warmup'
             GROUP BY we.id
             HAVING sum(s.distance_m) > 0 OR sum(s.duration_s) > 0
             ORDER BY w.started_at_local, we.position",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CardioSession {
                workout_id: r.get(0)?,
                exercise_id: r.get(1)?,
                name: r.get(2)?,
                at: r.get(3)?,
                distance_m: r.get(4)?,
                duration_s: r.get(5)?,
                avg_hr: r.get(6)?,
                kcal: r.get(7)?,
                segments: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Saves a session done elsewhere as a finished one-exercise workout; returns its id.
    pub fn log_cardio(&mut self, e: &CardioEntry) -> Result<i64> {
        let at = NaiveDateTime::parse_from_str(&e.at, "%Y-%m-%d %H:%M:%S")
            .map_err(|_| crate::Error::Format(format!("bad time {:?}", e.at)))?;
        let tx = self.conn_mut().transaction()?;
        let name: String = tx.query_row("SELECT name FROM exercise WHERE id = ?1", [e.exercise_id], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO workout (name, started_at_local, duration_s, notes, finished) VALUES (?1, ?2, ?3, ?4, 1)",
            params![name, at.format("%Y-%m-%d %H:%M:%S").to_string(), e.duration_s.map(|d| d.round() as i64), e.notes],
        )?;
        let w = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)",
            params![w, name, e.exercise_id],
        )?;
        let we = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO workout_set (workout_exercise_id, position, set_type, distance_m, duration_s, avg_hr, kcal, completed)
             VALUES (?1, 0, 'normal', ?2, ?3, ?4, ?5, 1)",
            params![we, e.distance_m, e.duration_s, e.avg_hr, e.kcal],
        )?;
        tx.commit()?;
        self.rebuild_prs()?;
        Ok(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_runs_add_up_and_set_prs() {
        let mut db = Db::open_in_memory().unwrap();
        let run = db.exercise_id("running").unwrap().or(db.exercise_id("running-treadmill").unwrap()).unwrap();
        let entry = |at: &str, d: f64, t: f64| CardioEntry {
            exercise_id: run,
            at: at.into(),
            distance_m: Some(d),
            duration_s: Some(t),
            avg_hr: Some(150),
            kcal: None,
            notes: None,
        };
        db.log_cardio(&entry("2026-11-01 07:00:00", 5000.0, 1800.0)).unwrap();
        let w = db.log_cardio(&entry("2026-11-03 07:00:00", 6000.0, 2000.0)).unwrap();
        let s = db.cardio_sessions().unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[1].distance_m, s[1].duration_s, s[1].avg_hr), (Some(6000.0), Some(2000.0), Some(150.0)));
        let detail = db.workout_detail(w).unwrap().unwrap();
        let mut kinds: Vec<_> = detail.prs.iter().map(|p| p.kind.as_str()).collect();
        kinds.sort();
        assert_eq!(kinds, ["distance", "duration", "pace"]);
    }
}
