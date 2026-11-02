//! Workout templates.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::targets::parse_target;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateExercise {
    pub exercise_id: i64,
    #[serde(default)]
    pub name: String,
    pub superset: Option<i64>,
    pub target_sets: Option<i64>,
    pub rep_min: Option<i64>,
    pub rep_max: Option<i64>,
    pub rest_s: Option<i64>,
    pub setup_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: Option<i64>,
    pub name: String,
    pub notes: Option<String>,
    pub color: Option<String>,
    pub exercises: Vec<TemplateExercise>,
    #[serde(default)]
    pub last_done: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedTemplate {
    pub template_id: i64,
    /// Notes that looked like a target but couldn't be read with confidence.
    pub uncertain_notes: Vec<(String, String)>,
}

/// Colors handed to new templates in turn (also used for calendar days). The UI swaps in the
/// matching dark-mode step; this is the validated categorical palette in its fixed order.
pub const TEMPLATE_COLORS: [&str; 8] = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];

impl Db {
    pub fn list_templates(&self) -> Result<Vec<Template>> {
        let mut stmt = self.conn().prepare(
            "SELECT t.id, t.name, t.notes, t.color,
                    (SELECT max(substr(w.started_at_local, 1, 10)) FROM workout w WHERE w.finished = 1 AND (w.template_id = t.id OR w.name = t.name))
             FROM template t WHERE t.archived = 0 ORDER BY t.name",
        )?;
        let heads: Vec<(i64, String, Option<String>, Option<String>, Option<String>)> =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        heads
            .into_iter()
            .map(|(id, name, notes, color, last_done)| {
                Ok(Template { id: Some(id), name, notes, color, exercises: self.template_exercises(id)?, last_done })
            })
            .collect()
    }

    pub fn template(&self, id: i64) -> Result<Option<Template>> {
        Ok(self.list_templates()?.into_iter().find(|t| t.id == Some(id)))
    }

    fn template_exercises(&self, id: i64) -> Result<Vec<TemplateExercise>> {
        let mut stmt = self.conn().prepare_cached(
            "SELECT te.exercise_id, e.name, te.superset, te.target_sets, te.rep_min, te.rep_max, te.rest_s, te.setup_note
             FROM template_exercise te JOIN exercise e ON e.id = te.exercise_id WHERE te.template_id = ?1 ORDER BY te.position",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(TemplateExercise {
                exercise_id: r.get(0)?,
                name: r.get(1)?,
                superset: r.get(2)?,
                target_sets: r.get(3)?,
                rep_min: r.get(4)?,
                rep_max: r.get(5)?,
                rest_s: r.get(6)?,
                setup_note: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Creates (`id: None`) or replaces a template. Returns its id.
    pub fn save_template(&mut self, t: &Template) -> Result<i64> {
        if t.name.trim().is_empty() {
            return Err(Error::Format("template name can't be empty".into()));
        }
        let tx = self.conn_mut().transaction()?;
        let id = match t.id {
            Some(id) => {
                tx.execute("UPDATE template SET name = ?2, notes = ?3, color = ?4 WHERE id = ?1", params![id, t.name.trim(), t.notes, t.color])?;
                tx.execute("DELETE FROM template_exercise WHERE template_id = ?1", [id])?;
                id
            }
            None => {
                let n: i64 = tx.query_row("SELECT COUNT(*) FROM template", [], |r| r.get(0))?;
                let color = t.color.clone().unwrap_or_else(|| TEMPLATE_COLORS[n as usize % TEMPLATE_COLORS.len()].to_string());
                tx.execute("INSERT INTO template (name, notes, color) VALUES (?1, ?2, ?3)", params![t.name.trim(), t.notes, color])?;
                tx.last_insert_rowid()
            }
        };
        for (pos, e) in t.exercises.iter().enumerate() {
            tx.execute(
                "INSERT INTO template_exercise (template_id, position, superset, exercise_id, target_sets, rep_min, rep_max, rest_s, setup_note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, pos as i64, e.superset, e.exercise_id, e.target_sets, e.rep_min, e.rep_max, e.rest_s, e.setup_note],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn archive_template(&mut self, id: i64) -> Result<()> {
        self.conn().execute("UPDATE template SET archived = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// The template structure a finished workout implies, with targets parsed from its notes.
    fn template_from(&self, workout_id: i64, keep_targets_from: Option<&Template>) -> Result<(Vec<TemplateExercise>, Vec<(String, String)>)> {
        let mut stmt = self.conn().prepare(
            "SELECT we.id, we.exercise_id, e.name, we.superset, we.rest_s, we.notes, we.target_sets, we.rep_min, we.rep_max
             FROM workout_exercise we JOIN exercise e ON e.id = we.exercise_id
             WHERE we.workout_id = ?1 ORDER BY we.position",
        )?;
        type Row = (i64, i64, String, Option<i64>, Option<i64>, Option<String>, Option<i64>, Option<i64>, Option<i64>);
        let rows: Vec<Row> = stmt
            .query_map([workout_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let mut uncertain = Vec::new();
        let mut out = Vec::new();
        for (we, ex, name, superset, rest, notes, ts, rmin, rmax) in rows {
            let sets = self.conn().query_row(
                "SELECT COUNT(*) FROM workout_set WHERE workout_exercise_id = ?1 AND set_type IN ('normal', 'failure')",
                [we],
                |r| r.get::<_, i64>(0),
            )?;
            let existing = keep_targets_from.and_then(|t| t.exercises.iter().find(|e| e.exercise_id == ex));
            let mut te = TemplateExercise {
                exercise_id: ex,
                name,
                superset,
                target_sets: ts.or(Some(sets)),
                rep_min: rmin,
                rep_max: rmax,
                rest_s: rest,
                setup_note: existing.and_then(|e| e.setup_note.clone()),
            };
            if let Some(e) = existing {
                te.rep_min = te.rep_min.or(e.rep_min);
                te.rep_max = te.rep_max.or(e.rep_max);
            } else if let Some(note) = notes.filter(|_| te.rep_max.is_none()) {
                match parse_target(&note) {
                    Some(t) if t.certain => {
                        if let Some(s) = t.sets {
                            te.target_sets = Some(s.max as i64);
                        }
                        if let Some(r) = t.reps {
                            te.rep_min = Some(r.min as i64);
                            te.rep_max = Some(r.max as i64);
                        }
                        te.setup_note = (!t.remainder.is_empty()).then_some(t.remainder);
                    }
                    Some(_) => {
                        uncertain.push((te.name.clone(), note.clone()));
                        te.setup_note = Some(note);
                    }
                    None => te.setup_note = Some(note),
                }
            }
            out.push(te);
        }
        Ok((out, uncertain))
    }

    /// New template from a finished workout (e.g. the latest "Torso A").
    pub fn template_from_workout(&mut self, workout_id: i64, name: &str) -> Result<CreatedTemplate> {
        let (exercises, uncertain_notes) = self.template_from(workout_id, None)?;
        let id = self.save_template(&Template { id: None, name: name.into(), notes: None, color: None, exercises, last_done: None })?;
        self.conn().execute("UPDATE workout SET template_id = ?2 WHERE id = ?1", params![workout_id, id])?;
        Ok(CreatedTemplate { template_id: id, uncertain_notes })
    }

    /// Whether the workout's exercises, order, supersets, set counts or rest differ from its template.
    pub fn workout_differs_from_template(&self, workout_id: i64) -> Result<bool> {
        let Some(tid): Option<i64> = self
            .conn()
            .query_row("SELECT template_id FROM workout WHERE id = ?1", [workout_id], |r| r.get(0))
            .optional()?
            .flatten()
        else {
            return Ok(false);
        };
        let Some(t) = self.template(tid)? else { return Ok(false) };
        let (now, _) = self.template_from(workout_id, Some(&t))?;
        let key = |e: &TemplateExercise| (e.exercise_id, e.superset, e.target_sets, e.rest_s);
        Ok(now.iter().map(key).ne(t.exercises.iter().map(key)))
    }

    /// Rewrites the workout's template from what was actually done, keeping rep targets and
    /// setup notes for exercises that stayed.
    pub fn update_template_from_workout(&mut self, workout_id: i64) -> Result<()> {
        let tid: Option<i64> = self.conn().query_row("SELECT template_id FROM workout WHERE id = ?1", [workout_id], |r| r.get(0))?;
        let tid = tid.ok_or_else(|| Error::Format("workout has no template".into()))?;
        let mut t = self.template(tid)?.ok_or_else(|| Error::Format("template not found".into()))?;
        let (exercises, _) = self.template_from(workout_id, Some(&t))?;
        t.exercises = exercises;
        self.save_template(&t)?;
        Ok(())
    }
}
