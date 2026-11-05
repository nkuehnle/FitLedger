//! Exercise library management: listing, editing, custom exercises and load modes.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseInfo {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub equipment: Option<String>,
    pub pattern: Option<String>,
    pub kind: String,
    pub load_mode: String,
    pub default_rest_s: Option<i64>,
    pub setup_note: Option<String>,
    pub primary: Vec<String>,
    pub secondary: Vec<String>,
    pub source: String,
    pub is_custom: bool,
    pub user_modified: bool,
    /// Sessions logged with this exercise.
    pub sessions: i64,
    pub last_done: Option<String>,
}

/// Fields the user can edit. Muscles are replaced wholesale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseEdit {
    pub name: String,
    pub equipment: Option<String>,
    pub pattern: Option<String>,
    pub kind: String,
    pub load_mode: String,
    pub default_rest_s: Option<i64>,
    pub setup_note: Option<String>,
    pub primary: Vec<String>,
    pub secondary: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerHandSuspect {
    pub workout_exercise_id: i64,
    pub workout_id: i64,
    pub exercise_name: String,
    pub date: String,
    pub max_weight_kg: f64,
    pub neighbor_median_kg: f64,
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

impl Db {
    /// Exercises, most used first then alphabetical. `only_used` limits to exercises with history.
    pub fn list_exercises(&self, only_used: bool) -> Result<Vec<ExerciseInfo>> {
        let mut muscles: HashMap<i64, (Vec<String>, Vec<String>)> = HashMap::new();
        {
            let mut stmt = self.conn().prepare(
                "SELECT em.exercise_id, em.muscle_id, em.role FROM exercise_muscle em JOIN muscle m ON m.id = em.muscle_id ORDER BY m.position",
            )?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
                let (id, m, role) = row?;
                let e = muscles.entry(id).or_default();
                if role == "primary" { e.0.push(m) } else { e.1.push(m) }
            }
        }
        let mut aliases: HashMap<i64, Vec<String>> = HashMap::new();
        {
            let mut stmt = self.conn().prepare("SELECT exercise_id, alias FROM exercise_alias ORDER BY alias")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
                let (id, a) = row?;
                aliases.entry(id).or_default().push(a);
            }
        }
        let mut stmt = self.conn().prepare(
            "SELECT e.id, e.slug, e.name, e.equipment, e.pattern, e.kind, e.load_mode, e.default_rest_s, e.setup_note,
                    e.source, e.is_custom, e.user_modified,
                    (SELECT COUNT(*) FROM workout_exercise we WHERE we.exercise_id = e.id),
                    (SELECT max(substr(w.started_at_local, 1, 10)) FROM workout_exercise we JOIN workout w ON w.id = we.workout_id WHERE we.exercise_id = e.id)
             FROM exercise e WHERE e.hidden = 0",
        )?;
        let mut out: Vec<ExerciseInfo> = stmt
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                Ok(ExerciseInfo {
                    id,
                    slug: r.get(1)?,
                    name: r.get(2)?,
                    aliases: Vec::new(),
                    equipment: r.get(3)?,
                    pattern: r.get(4)?,
                    kind: r.get(5)?,
                    load_mode: r.get(6)?,
                    default_rest_s: r.get(7)?,
                    setup_note: r.get(8)?,
                    primary: Vec::new(),
                    secondary: Vec::new(),
                    source: r.get(9)?,
                    is_custom: r.get(10)?,
                    user_modified: r.get(11)?,
                    sessions: r.get(12)?,
                    last_done: r.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        out.retain(|e| !only_used || e.sessions > 0);
        for e in &mut out {
            if let Some((p, s)) = muscles.remove(&e.id) {
                e.primary = p;
                e.secondary = s;
            }
            e.aliases = aliases.remove(&e.id).unwrap_or_default();
        }
        out.sort_by(|a, b| b.sessions.cmp(&a.sessions).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    }

    pub fn exercise_info(&self, id: i64) -> Result<Option<ExerciseInfo>> {
        Ok(self.list_exercises(false)?.into_iter().find(|e| e.id == id))
    }

    pub(crate) fn validate_exercise(&self, e: &ExerciseEdit) -> Result<()> {
        if e.name.trim().is_empty() {
            return Err(Error::Format("exercise name can't be empty".into()));
        }
        if !matches!(e.load_mode.as_str(), "total" | "per_hand") {
            return Err(Error::Format(format!("unknown load mode {:?}", e.load_mode)));
        }
        let mut stmt = self.conn().prepare_cached("SELECT 1 FROM muscle WHERE id = ?1")?;
        for m in e.primary.iter().chain(&e.secondary) {
            if stmt.query_row([m], |_| Ok(())).optional()?.is_none() {
                return Err(Error::Format(format!("unknown muscle {m:?}")));
            }
        }
        Ok(())
    }

    fn write_muscles(&self, id: i64, primary: &[String], secondary: &[String]) -> Result<()> {
        self.conn().execute("DELETE FROM exercise_muscle WHERE exercise_id = ?1", [id])?;
        let mut ins = self.conn().prepare_cached("INSERT OR REPLACE INTO exercise_muscle (exercise_id, muscle_id, role) VALUES (?1, ?2, ?3)")?;
        for m in secondary {
            ins.execute(params![id, m, "secondary"])?;
        }
        // Primary wins if a muscle is listed in both.
        for m in primary {
            ins.execute(params![id, m, "primary"])?;
        }
        Ok(())
    }

    /// Edits any exercise. Built-in ones are marked `user_modified` so library updates keep the edit.
    pub fn update_exercise(&mut self, id: i64, e: &ExerciseEdit) -> Result<()> {
        self.validate_exercise(e)?;
        self.conn().execute(
            "UPDATE exercise SET name = ?2, equipment = ?3, pattern = ?4, kind = ?5, load_mode = ?6, default_rest_s = ?7,
                 setup_note = ?8, user_modified = 1 WHERE id = ?1",
            params![id, e.name.trim(), e.equipment, e.pattern, e.kind, e.load_mode, e.default_rest_s, e.setup_note],
        )?;
        self.write_muscles(id, &e.primary, &e.secondary)?;
        self.rebuild_prs()?;
        Ok(())
    }

    pub fn create_exercise(&mut self, e: &ExerciseEdit) -> Result<i64> {
        self.validate_exercise(e)?;
        let base = format!("custom-{}", slugify(&e.name));
        let mut slug = base.clone();
        let mut n = 2;
        while self.exercise_id(&slug)?.is_some() {
            slug = format!("{base}-{n}");
            n += 1;
        }
        self.conn().execute(
            "INSERT INTO exercise (slug, name, equipment, pattern, kind, load_mode, default_rest_s, setup_note, is_custom, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 'custom')",
            params![slug, e.name.trim(), e.equipment, e.pattern, e.kind, e.load_mode, e.default_rest_s, e.setup_note],
        )?;
        let id = self.conn().last_insert_rowid();
        self.write_muscles(id, &e.primary, &e.secondary)?;
        Ok(id)
    }

    pub fn set_exercise_load_mode(&mut self, id: i64, mode: &str) -> Result<()> {
        if !matches!(mode, "total" | "per_hand") {
            return Err(Error::Format(format!("unknown load mode {mode:?}")));
        }
        self.conn().execute("UPDATE exercise SET load_mode = ?2, user_modified = 1 WHERE id = ?1", params![id, mode])?;
        self.rebuild_prs()?;
        Ok(())
    }

    /// Per-session override; `None` follows the exercise's mode.
    pub fn set_session_load_mode(&mut self, workout_exercise_id: i64, mode: Option<&str>) -> Result<()> {
        if let Some(m) = mode {
            if !matches!(m, "total" | "per_hand") {
                return Err(Error::Format(format!("unknown load mode {m:?}")));
            }
        }
        self.conn().execute("UPDATE workout_exercise SET load_mode = ?2 WHERE id = ?1", params![workout_exercise_id, mode])?;
        self.rebuild_prs()?;
        Ok(())
    }

    /// Dumbbell/kettlebell sessions whose heaviest set is under 62% of the median of up to three
    /// sessions either side: likely logged per hand instead of total. Sessions with an explicit
    /// load-mode override are skipped (the user already decided).
    pub fn per_hand_suspects(&self) -> Result<Vec<PerHandSuspect>> {
        let mut stmt = self.conn().prepare(
            "SELECT we.id, w.id, e.name, substr(w.started_at_local, 1, 10), max(s.weight_kg), we.exercise_id, we.load_mode IS NOT NULL
             FROM workout_exercise we
             JOIN exercise e ON e.id = we.exercise_id
             JOIN workout w ON w.id = we.workout_id
             JOIN workout_set s ON s.workout_exercise_id = we.id
             WHERE e.equipment IN ('dumbbell', 'kettlebell') AND s.set_type != 'warmup' AND s.weight_kg > 0 AND w.finished = 1
             GROUP BY we.id ORDER BY we.exercise_id, w.started_at_local",
        )?;
        let rows: Vec<(i64, i64, String, String, f64, i64, bool)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for group in rows.chunk_by(|a, b| a.5 == b.5) {
            for (i, row) in group.iter().enumerate() {
                if row.6 {
                    continue;
                }
                let mut neighbors: Vec<f64> = group[i.saturating_sub(3)..i]
                    .iter()
                    .chain(group[i + 1..(i + 4).min(group.len())].iter())
                    .map(|r| r.4)
                    .collect();
                if neighbors.len() < 3 {
                    continue;
                }
                neighbors.sort_by(f64::total_cmp);
                let n = neighbors.len();
                let median = if n % 2 == 1 { neighbors[n / 2] } else { (neighbors[n / 2 - 1] + neighbors[n / 2]) / 2.0 };
                if row.4 < 0.62 * median {
                    out.push(PerHandSuspect {
                        workout_exercise_id: row.0,
                        workout_id: row.1,
                        exercise_name: row.2.clone(),
                        date: row.3.clone(),
                        max_weight_kg: row.4,
                        neighbor_median_kg: median,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Exercises that work the same primary muscles as `exercise_id`, best matches first:
    /// shared primary muscles, then same movement pattern, then how often you've done it.
    pub fn similar_exercises(&self, exercise_id: i64, limit: usize) -> Result<Vec<ExerciseInfo>> {
        let all = self.list_exercises(false)?;
        let Some(base) = all.iter().find(|e| e.id == exercise_id).cloned() else { return Ok(Vec::new()) };
        let mut scored: Vec<(f64, ExerciseInfo)> = all
            .into_iter()
            .filter(|e| e.id != exercise_id)
            .filter_map(|e| {
                let shared = e.primary.iter().filter(|m| base.primary.contains(m)).count();
                if shared == 0 {
                    return None;
                }
                let mut score = shared as f64 / base.primary.len().max(e.primary.len()) as f64;
                if e.pattern == base.pattern {
                    score += 0.5;
                }
                if e.source == "liftbook" || e.is_custom {
                    score += 0.2;
                }
                score += (e.sessions as f64).ln_1p() * 0.1;
                Some((score, e))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        Ok(scored.into_iter().take(limit).map(|(_, e)| e).collect())
    }
}
