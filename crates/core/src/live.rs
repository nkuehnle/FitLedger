//! Live workouts and workout editing. A live workout is a `workout` row with `finished = 0`;
//! the same operations edit finished workouts from history.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

const TIME_FMT: &str = "%Y-%m-%d %H:%M:%S";
const DEFAULT_SETS: i64 = 3;
const DEFAULT_REST_S: i64 = 90;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartFrom {
    Empty,
    Template { template_id: i64 },
    Repeat { workout_id: i64 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetUpdate {
    pub set_type: Option<String>,
    /// `Some(None)` clears the value.
    #[serde(default, with = "double_option")]
    pub weight_kg: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub reps: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub distance_m: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub duration_s: Option<Option<f64>>,
    #[serde(default, with = "double_option")]
    pub rpe: Option<Option<f64>>,
    pub completed: Option<bool>,
}

/// Distinguishes a missing field (leave unchanged) from `null` (clear it).
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<T: Serialize, S: Serializer>(v: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(inner) => inner.serialize(s),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExerciseUpdate {
    #[serde(default, with = "double_option")]
    pub notes: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub rest_s: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub next_intent: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub elbow: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub knee: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub shoulder: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub load_mode: Option<Option<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkoutUpdate {
    pub name: Option<String>,
    #[serde(default, with = "double_option")]
    pub notes: Option<Option<String>>,
    pub started_at_local: Option<String>,
    #[serde(default, with = "double_option")]
    pub duration_s: Option<Option<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrevSet {
    pub set_type: String,
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
}

/// What happened last time for one exercise of the live workout.
#[derive(Debug, Clone, Serialize)]
pub struct Previous {
    pub workout_exercise_id: i64,
    pub at: Option<String>,
    pub sets: Vec<PrevSet>,
    pub notes: Option<String>,
    pub next_intent: Option<String>,
    /// Every working set last time reached the top of the target rep range.
    pub suggest_increase: bool,
    pub setup_note: Option<String>,
    pub primary: Vec<String>,
    pub secondary: Vec<String>,
}

/// One entry of a new exercise order: the exercise and its superset group (or none).
#[derive(Debug, Clone, Deserialize)]
pub struct OrderEntry {
    pub workout_exercise_id: i64,
    pub superset: Option<i64>,
}

impl Db {
    pub fn active_workout(&self) -> Result<Option<i64>> {
        Ok(self
            .conn()
            .query_row("SELECT id FROM workout WHERE finished = 0 ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .optional()?)
    }

    fn last_session_of(&self, exercise_id: i64, template_id: Option<i64>, before_workout: Option<i64>) -> Result<Option<i64>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT we.id FROM workout_exercise we JOIN workout w ON w.id = we.workout_id
                 WHERE we.exercise_id = ?1 AND w.finished = 1 AND (?2 IS NULL OR w.template_id = ?2)
                   AND (?3 IS NULL OR w.id != ?3)
                 ORDER BY w.started_at_local DESC LIMIT 1",
                params![exercise_id, template_id, before_workout],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn set_count(&self, workout_exercise_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM workout_set WHERE workout_exercise_id = ?1 AND set_type != 'warmup'",
            [workout_exercise_id],
            |r| r.get(0),
        )?)
    }

    fn insert_exercise(
        &self,
        workout_id: i64,
        position: i64,
        superset: Option<i64>,
        exercise_id: i64,
        rest_s: i64,
        target: (Option<i64>, Option<i64>, Option<i64>),
        sets: i64,
    ) -> Result<i64> {
        let name: String = self.conn().query_row("SELECT name FROM exercise WHERE id = ?1", [exercise_id], |r| r.get(0))?;
        self.conn().execute(
            "INSERT INTO workout_exercise (workout_id, position, superset, source_name, exercise_id, rest_s, target_sets, rep_min, rep_max)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![workout_id, position, superset, name, exercise_id, rest_s, target.0, target.1, target.2],
        )?;
        let we = self.conn().last_insert_rowid();
        for i in 0..sets.max(1) {
            self.conn().execute(
                "INSERT INTO workout_set (workout_exercise_id, position, set_type, completed) VALUES (?1, ?2, 'normal', 0)",
                params![we, i],
            )?;
        }
        Ok(we)
    }

    /// Rest for an exercise: last time in this template, else the given default, the exercise's
    /// default, or 90 s.
    fn remembered_rest(&self, exercise_id: i64, template_id: Option<i64>, fallback: Option<i64>) -> Result<i64> {
        if let Some(we) = self.last_session_of(exercise_id, template_id, None)? {
            let r: Option<i64> = self.conn().query_row("SELECT rest_s FROM workout_exercise WHERE id = ?1", [we], |r| r.get(0))?;
            if let Some(r) = r {
                return Ok(r);
            }
        }
        if let Some(f) = fallback {
            return Ok(f);
        }
        let d: Option<i64> = self.conn().query_row("SELECT default_rest_s FROM exercise WHERE id = ?1", [exercise_id], |r| r.get(0))?;
        Ok(d.unwrap_or(DEFAULT_REST_S))
    }

    /// Starts a workout. Fails if another one is in progress.
    pub fn start_workout(&mut self, from: &StartFrom, now: NaiveDateTime, utc_offset_min: Option<i32>) -> Result<i64> {
        if self.active_workout()?.is_some() {
            return Err(Error::Format("a workout is already in progress".into()));
        }
        let (name, template_id) = match from {
            StartFrom::Empty => ("Workout".to_string(), None),
            StartFrom::Template { template_id } => (
                self.conn().query_row("SELECT name FROM template WHERE id = ?1", [template_id], |r| r.get(0))?,
                Some(*template_id),
            ),
            StartFrom::Repeat { workout_id } => self.conn().query_row(
                "SELECT name, template_id FROM workout WHERE id = ?1",
                [workout_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
        };
        let tx_start = now.format(TIME_FMT).to_string();
        self.conn().execute(
            "INSERT INTO workout (name, started_at_local, utc_offset_min, finished, template_id) VALUES (?1, ?2, ?3, 0, ?4)",
            params![name, tx_start, utc_offset_min, template_id],
        )?;
        let workout_id = self.conn().last_insert_rowid();

        match from {
            StartFrom::Empty => {}
            StartFrom::Template { template_id } => {
                let mut stmt = self.conn().prepare(
                    "SELECT exercise_id, superset, target_sets, rep_min, rep_max, rest_s FROM template_exercise
                     WHERE template_id = ?1 ORDER BY position",
                )?;
                let rows: Vec<(i64, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> = stmt
                    .query_map([template_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                drop(stmt);
                let mut counts = Vec::with_capacity(rows.len());
                for &(ex, _, ts, ..) in &rows {
                    counts.push(match ts {
                        Some(n) => n,
                        None => match self.last_session_of(ex, Some(*template_id), None)? {
                            Some(we) => self.set_count(we)?,
                            None => DEFAULT_SETS,
                        },
                    });
                }
                // A superset is done in rounds: every exercise in it gets the group's largest set count.
                let mut rounds: HashMap<i64, i64> = HashMap::new();
                for (row, &n) in rows.iter().zip(&counts) {
                    if let Some(g) = row.1 {
                        let r = rounds.entry(g).or_insert(0);
                        *r = (*r).max(n);
                    }
                }
                for (pos, ((ex, superset, ts, rmin, rmax, rest), n)) in rows.into_iter().zip(counts).enumerate() {
                    let rest = self.remembered_rest(ex, Some(*template_id), rest)?;
                    let sets = superset.map_or(n, |g| rounds[&g]);
                    self.insert_exercise(workout_id, pos as i64, superset, ex, rest, (ts, rmin, rmax), sets)?;
                }
            }
            StartFrom::Repeat { workout_id: src } => {
                let mut stmt = self.conn().prepare(
                    "SELECT id, exercise_id, superset, rest_s, target_sets, rep_min, rep_max FROM workout_exercise
                     WHERE workout_id = ?1 AND exercise_id IS NOT NULL ORDER BY position",
                )?;
                let rows: Vec<(i64, i64, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> = stmt
                    .query_map([src], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                drop(stmt);
                for (pos, (we, ex, superset, rest, ts, rmin, rmax)) in rows.into_iter().enumerate() {
                    let rest = rest.unwrap_or(self.remembered_rest(ex, None, None)?);
                    let sets = self.set_count(we)?;
                    self.insert_exercise(workout_id, pos as i64, superset, ex, rest, (ts, rmin, rmax), sets)?;
                }
            }
        }
        Ok(workout_id)
    }

    fn next_position(&self, workout_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT coalesce(max(position) + 1, 0) FROM workout_exercise WHERE workout_id = ?1",
            [workout_id],
            |r| r.get(0),
        )?)
    }

    /// Appends an exercise with as many sets as last time (default 3).
    pub fn add_exercise(&mut self, workout_id: i64, exercise_id: i64) -> Result<i64> {
        let template_id: Option<i64> = self.conn().query_row("SELECT template_id FROM workout WHERE id = ?1", [workout_id], |r| r.get(0))?;
        let sets = match self.last_session_of(exercise_id, None, Some(workout_id))? {
            Some(we) => self.set_count(we)?,
            None => DEFAULT_SETS,
        };
        let rest = self.remembered_rest(exercise_id, template_id, None)?;
        let pos = self.next_position(workout_id)?;
        self.insert_exercise(workout_id, pos, None, exercise_id, rest, (None, None, None), sets)
    }

    pub fn remove_exercise(&mut self, workout_exercise_id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM workout_exercise WHERE id = ?1", [workout_exercise_id])?;
        Ok(())
    }

    /// Replaces the exercise but keeps sets, targets, rest and superset.
    pub fn swap_exercise(&mut self, workout_exercise_id: i64, exercise_id: i64) -> Result<()> {
        let name: String = self.conn().query_row("SELECT name FROM exercise WHERE id = ?1", [exercise_id], |r| r.get(0))?;
        self.conn().execute(
            "UPDATE workout_exercise SET exercise_id = ?2, source_name = ?3, load_mode = NULL WHERE id = ?1",
            params![workout_exercise_id, exercise_id, name],
        )?;
        Ok(())
    }

    /// Applies a new exercise order and superset grouping. Superset ids are renumbered 0, 1, ...
    /// by first appearance; a group with a single exercise becomes standalone.
    pub fn reorder_exercises(&mut self, workout_id: i64, order: &[OrderEntry]) -> Result<()> {
        let mut counts = std::collections::HashMap::new();
        for e in order {
            if let Some(g) = e.superset {
                *counts.entry(g).or_insert(0) += 1;
            }
        }
        let mut renumber = std::collections::HashMap::new();
        let tx = self.conn_mut().transaction()?;
        for (pos, e) in order.iter().enumerate() {
            let group = e.superset.filter(|g| counts[g] > 1).map(|g| {
                let next = renumber.len() as i64;
                *renumber.entry(g).or_insert(next)
            });
            tx.execute(
                "UPDATE workout_exercise SET position = ?2, superset = ?3 WHERE id = ?1 AND workout_id = ?4",
                params![e.workout_exercise_id, pos as i64, group, workout_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn add_set(&mut self, workout_exercise_id: i64) -> Result<i64> {
        let pos: i64 = self.conn().query_row(
            "SELECT coalesce(max(position) + 1, 0) FROM workout_set WHERE workout_exercise_id = ?1",
            [workout_exercise_id],
            |r| r.get(0),
        )?;
        self.conn().execute(
            "INSERT INTO workout_set (workout_exercise_id, position, set_type, completed) VALUES (?1, ?2, 'normal', 0)",
            params![workout_exercise_id, pos],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Inserts a warm-up set before the first set of an exercise.
    pub fn add_warmup_set(&mut self, workout_exercise_id: i64) -> Result<i64> {
        let tx = self.conn_mut().transaction()?;
        tx.execute("UPDATE workout_set SET position = position + 1 WHERE workout_exercise_id = ?1", [workout_exercise_id])?;
        tx.execute(
            "INSERT INTO workout_set (workout_exercise_id, position, set_type, completed) VALUES (?1, 0, 'warmup', 0)",
            [workout_exercise_id],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// The exercise's sticky note: shown every time you log it (seat height, cues, reminders).
    pub fn set_sticky_note(&mut self, exercise_id: i64, note: Option<&str>) -> Result<()> {
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        self.conn().execute("UPDATE exercise SET setup_note = ?2, user_modified = 1 WHERE id = ?1", params![exercise_id, note])?;
        Ok(())
    }

    pub fn remove_set(&mut self, set_id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM workout_set WHERE id = ?1", [set_id])?;
        Ok(())
    }

    pub fn update_set(&mut self, set_id: i64, u: &SetUpdate) -> Result<()> {
        if let Some(t) = &u.set_type {
            if !matches!(t.as_str(), "normal" | "warmup" | "drop" | "failure") {
                return Err(Error::Format(format!("unknown set type {t:?}")));
            }
            self.conn().execute("UPDATE workout_set SET set_type = ?2 WHERE id = ?1", params![set_id, t])?;
        }
        let c = self.conn();
        if let Some(v) = u.weight_kg {
            c.execute("UPDATE workout_set SET weight_kg = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        if let Some(v) = u.reps {
            c.execute("UPDATE workout_set SET reps = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        if let Some(v) = u.distance_m {
            c.execute("UPDATE workout_set SET distance_m = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        if let Some(v) = u.duration_s {
            c.execute("UPDATE workout_set SET duration_s = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        if let Some(v) = u.rpe {
            c.execute("UPDATE workout_set SET rpe = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        if let Some(v) = u.completed {
            c.execute("UPDATE workout_set SET completed = ?2 WHERE id = ?1", params![set_id, v])?;
        }
        Ok(())
    }

    pub fn update_workout_exercise(&mut self, workout_exercise_id: i64, u: &ExerciseUpdate) -> Result<()> {
        let c = self.conn();
        let id = workout_exercise_id;
        if let Some(v) = &u.notes {
            c.execute("UPDATE workout_exercise SET notes = ?2 WHERE id = ?1", params![id, v.as_deref().filter(|s| !s.trim().is_empty())])?;
        }
        if let Some(v) = u.rest_s {
            c.execute("UPDATE workout_exercise SET rest_s = ?2 WHERE id = ?1", params![id, v])?;
        }
        if let Some(v) = &u.next_intent {
            c.execute("UPDATE workout_exercise SET next_intent = ?2 WHERE id = ?1", params![id, v])?;
        }
        for (col, v) in [("elbow", u.elbow), ("knee", u.knee), ("shoulder", u.shoulder)] {
            if let Some(v) = v {
                c.execute(&format!("UPDATE workout_exercise SET {col} = ?2 WHERE id = ?1"), params![id, v])?;
            }
        }
        if let Some(v) = &u.load_mode {
            c.execute("UPDATE workout_exercise SET load_mode = ?2 WHERE id = ?1", params![id, v])?;
        }
        Ok(())
    }

    pub fn update_workout(&mut self, workout_id: i64, u: &WorkoutUpdate) -> Result<()> {
        let c = self.conn();
        if let Some(n) = &u.name {
            c.execute("UPDATE workout SET name = ?2 WHERE id = ?1", params![workout_id, n.trim()])?;
        }
        if let Some(v) = &u.notes {
            c.execute("UPDATE workout SET notes = ?2 WHERE id = ?1", params![workout_id, v.as_deref().filter(|s| !s.trim().is_empty())])?;
        }
        if let Some(t) = &u.started_at_local {
            if crate::strong::parse_datetime(t).is_none() {
                return Err(Error::Format(format!("bad date/time {t:?}")));
            }
            c.execute("UPDATE workout SET started_at_local = ?2 WHERE id = ?1", params![workout_id, t])?;
        }
        if let Some(v) = u.duration_s {
            c.execute("UPDATE workout SET duration_s = ?2 WHERE id = ?1", params![workout_id, v])?;
        }
        Ok(())
    }

    /// Records when the running rest timer ends (or clears it).
    pub fn set_rest_timer(&mut self, workout_id: i64, ends_at: Option<&str>, total_s: Option<i64>) -> Result<()> {
        self.conn().execute(
            "UPDATE workout SET rest_ends_at = ?2, rest_total_s = ?3 WHERE id = ?1",
            params![workout_id, ends_at, total_s],
        )?;
        Ok(())
    }

    /// Finishes a live workout: drops sets that were never completed and exercises left empty,
    /// records the duration and rebuilds PRs.
    pub fn finish_workout(&mut self, workout_id: i64, now: NaiveDateTime) -> Result<()> {
        let start: String = self.conn().query_row("SELECT started_at_local FROM workout WHERE id = ?1", [workout_id], |r| r.get(0))?;
        let start = crate::strong::parse_datetime(&start).ok_or_else(|| Error::Format("bad start time".into()))?;
        let tx = self.conn_mut().transaction()?;
        tx.execute(
            "DELETE FROM workout_set WHERE completed = 0 AND workout_exercise_id IN (SELECT id FROM workout_exercise WHERE workout_id = ?1)",
            [workout_id],
        )?;
        tx.execute(
            "DELETE FROM workout_exercise WHERE workout_id = ?1 AND NOT EXISTS (SELECT 1 FROM workout_set s WHERE s.workout_exercise_id = workout_exercise.id)",
            [workout_id],
        )?;
        tx.execute(
            "UPDATE workout SET finished = 1, duration_s = ?2, rest_ends_at = NULL, rest_total_s = NULL WHERE id = ?1",
            params![workout_id, (now - start).num_seconds().max(0)],
        )?;
        tx.commit()?;
        // Supersets may now have a single member.
        let order: Vec<OrderEntry> = {
            let mut stmt = self.conn().prepare("SELECT id, superset FROM workout_exercise WHERE workout_id = ?1 ORDER BY position")?;
            stmt.query_map([workout_id], |r| Ok(OrderEntry { workout_exercise_id: r.get(0)?, superset: r.get(1)? }))?
                .collect::<rusqlite::Result<_>>()?
        };
        self.reorder_exercises(workout_id, &order)?;
        self.rebuild_prs()?;
        Ok(())
    }

    pub fn delete_workout(&mut self, workout_id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM workout WHERE id = ?1", [workout_id])?;
        self.rebuild_prs()?;
        Ok(())
    }

    /// For each exercise in `workout_id`: the previous session's sets, notes and intent, plus
    /// the exercise's setup note and muscles.
    pub fn previous_for_workout(&self, workout_id: i64) -> Result<Vec<Previous>> {
        let mut stmt = self.conn().prepare(
            "SELECT we.id, we.exercise_id, e.setup_note FROM workout_exercise we LEFT JOIN exercise e ON e.id = we.exercise_id
             WHERE we.workout_id = ?1 ORDER BY we.position",
        )?;
        let rows: Vec<(i64, Option<i64>, Option<String>)> =
            stmt.query_map([workout_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let started: String = self.conn().query_row("SELECT started_at_local FROM workout WHERE id = ?1", [workout_id], |r| r.get(0))?;
        let mut out = Vec::new();
        for (we, ex, setup_note) in rows {
            let mut prev = Previous {
                workout_exercise_id: we,
                at: None,
                sets: Vec::new(),
                notes: None,
                next_intent: None,
                suggest_increase: false,
                setup_note,
                primary: Vec::new(),
                secondary: Vec::new(),
            };
            if let Some(ex) = ex {
                let mut ms = self.conn().prepare_cached("SELECT muscle_id, role FROM exercise_muscle em JOIN muscle m ON m.id = em.muscle_id WHERE exercise_id = ?1 ORDER BY m.position")?;
                for row in ms.query_map([ex], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                    let (m, role) = row?;
                    if role == "primary" { prev.primary.push(m) } else { prev.secondary.push(m) }
                }
                let last: Option<(i64, String, Option<String>, Option<String>, Option<i64>)> = self
                    .conn()
                    .query_row(
                        "SELECT we.id, w.started_at_local, we.notes, we.next_intent, we.rep_max FROM workout_exercise we
                         JOIN workout w ON w.id = we.workout_id
                         WHERE we.exercise_id = ?1 AND w.finished = 1 AND w.id != ?2 AND w.started_at_local <= ?3
                         ORDER BY w.started_at_local DESC LIMIT 1",
                        params![ex, workout_id, started],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                    )
                    .optional()?;
                if let Some((last_we, at, notes, intent, last_rep_max)) = last {
                    let mut ss = self.conn().prepare_cached(
                        "SELECT set_type, weight_kg, reps, distance_m, duration_s FROM workout_set WHERE workout_exercise_id = ?1 ORDER BY position",
                    )?;
                    prev.sets = ss
                        .query_map([last_we], |r| {
                            Ok(PrevSet { set_type: r.get(0)?, weight_kg: r.get(1)?, reps: r.get(2)?, distance_m: r.get(3)?, duration_s: r.get(4)? })
                        })?
                        .collect::<rusqlite::Result<_>>()?;
                    prev.at = Some(at);
                    prev.notes = notes;
                    prev.next_intent = intent;
                    // Target from this workout, else from last time.
                    let target_max: Option<i64> =
                        self.conn().query_row("SELECT rep_max FROM workout_exercise WHERE id = ?1", [we], |r| r.get(0))?;
                    if let Some(top) = target_max.or(last_rep_max) {
                        let working: Vec<_> = prev.sets.iter().filter(|s| s.set_type == "normal" || s.set_type == "failure").collect();
                        prev.suggest_increase = !working.is_empty() && working.iter().all(|s| s.reps.unwrap_or(0) >= top);
                    }
                }
            }
            out.push(prev);
        }
        Ok(out)
    }
}
