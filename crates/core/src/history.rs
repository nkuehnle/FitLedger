//! Read models for the history feed and workout detail.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::db::Db;

#[derive(Debug, Clone, Serialize)]
pub struct BestSet {
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardExercise {
    pub name: String,
    pub superset: Option<i64>,
    pub sets: usize,
    pub best: Option<BestSet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkoutCard {
    pub id: i64,
    pub name: String,
    pub started_at_local: String,
    pub duration_s: Option<i64>,
    pub sets: usize,
    /// Sum of weight × reps over non-warm-up sets, using the full load (per-hand × 2).
    pub volume_kg: f64,
    pub prs: usize,
    pub exercises: Vec<CardExercise>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetView {
    pub id: i64,
    pub set_type: String,
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub rpe: Option<f64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub rest_s: Option<i64>,
    pub completed: bool,
    /// Cardio: average heart rate and calories for the segment, if logged.
    pub avg_hr: Option<i64>,
    pub kcal: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExerciseView {
    pub id: i64,
    pub exercise_id: Option<i64>,
    /// Per-session override (None follows the exercise).
    pub load_mode_override: Option<String>,
    pub kind: Option<String>,
    pub next_intent: Option<String>,
    pub elbow: Option<i64>,
    pub knee: Option<i64>,
    pub shoulder: Option<i64>,
    pub target_sets: Option<i64>,
    pub rep_min: Option<i64>,
    pub rep_max: Option<i64>,
    /// Library name when mapped, else the imported name.
    pub name: String,
    pub source_name: String,
    pub exercise_slug: Option<String>,
    pub equipment: Option<String>,
    pub load_mode: String,
    pub superset: Option<i64>,
    pub notes: Option<String>,
    pub rest_s: Option<i64>,
    pub sets: Vec<SetView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkoutDetail {
    pub id: i64,
    pub finished: bool,
    pub template_id: Option<i64>,
    pub rest_ends_at: Option<String>,
    pub rest_total_s: Option<i64>,
    pub prs: Vec<crate::stats::PrView>,
    pub muscle_sets: Vec<(String, f64)>,
    pub name: String,
    pub started_at_local: String,
    pub tz_name: Option<String>,
    pub duration_s: Option<i64>,
    pub notes: Option<String>,
    pub volume_kg: f64,
    pub exercises: Vec<ExerciseView>,
    /// From a watch, when one recorded the workout.
    pub avg_hr: Option<i64>,
    pub max_hr: Option<i64>,
}

/// Exercise rows of the given workouts, in order, with the resolved display name and load mode.
const EXERCISES_SQL: &str = "
    SELECT we.id, we.workout_id, coalesce(e.name, we.source_name), we.source_name, e.slug, e.equipment,
           coalesce(we.load_mode, e.load_mode, 'total'), we.superset, we.notes, we.rest_s,
           we.exercise_id, we.load_mode, e.kind, we.next_intent, we.elbow, we.knee, we.shoulder,
           we.target_sets, we.rep_min, we.rep_max
    FROM workout_exercise we LEFT JOIN exercise e ON e.id = we.exercise_id
    WHERE we.workout_id = ?1 ORDER BY we.position";

const SETS_SQL: &str = "
    SELECT id, set_type, weight_kg, reps, rpe, distance_m, duration_s, rest_s, completed, avg_hr, kcal
    FROM workout_set WHERE workout_exercise_id = ?1 ORDER BY position";

fn load_factor(load_mode: &str) -> f64 {
    if load_mode == "per_hand" { 2.0 } else { 1.0 }
}

fn volume(sets: &[SetView], load_mode: &str) -> f64 {
    sets.iter()
        .filter(|s| s.set_type != "warmup" && s.completed)
        .map(|s| s.weight_kg.unwrap_or(0.0) * s.reps.unwrap_or(0) as f64)
        .sum::<f64>()
        * load_factor(load_mode)
}

/// Heaviest working set (ties: more reps); for cardio/timed work, the session's total distance and time.
fn best_set(sets: &[SetView]) -> Option<BestSet> {
    let working = sets.iter().filter(|s| s.set_type != "warmup");
    if let Some(best) = working.clone().filter(|s| s.reps.is_some()).max_by(|a, b| {
        a.weight_kg.unwrap_or(0.0).total_cmp(&b.weight_kg.unwrap_or(0.0)).then(a.reps.cmp(&b.reps))
    }) {
        return Some(BestSet { weight_kg: best.weight_kg, reps: best.reps, distance_m: best.distance_m, duration_s: best.duration_s });
    }
    let sum = |f: fn(&SetView) -> Option<f64>| working.clone().filter_map(f).reduce(|a, b| a + b);
    let (distance_m, duration_s) = (sum(|s| s.distance_m), sum(|s| s.duration_s));
    (distance_m.is_some() || duration_s.is_some()).then_some(BestSet { weight_kg: None, reps: None, distance_m, duration_s })
}

impl Db {
    fn exercises_of(&self, workout_id: i64) -> Result<Vec<ExerciseView>> {
        let mut ex_stmt = self.conn().prepare_cached(EXERCISES_SQL)?;
        let mut set_stmt = self.conn().prepare_cached(SETS_SQL)?;
        let rows: Vec<ExerciseView> = ex_stmt
            .query_map([workout_id], |r| {
                Ok(ExerciseView {
                    id: r.get(0)?,
                    exercise_id: r.get(10)?,
                    load_mode_override: r.get(11)?,
                    kind: r.get(12)?,
                    next_intent: r.get(13)?,
                    elbow: r.get(14)?,
                    knee: r.get(15)?,
                    shoulder: r.get(16)?,
                    target_sets: r.get(17)?,
                    rep_min: r.get(18)?,
                    rep_max: r.get(19)?,
                    name: r.get(2)?,
                    source_name: r.get(3)?,
                    exercise_slug: r.get(4)?,
                    equipment: r.get(5)?,
                    load_mode: r.get(6)?,
                    superset: r.get(7)?,
                    notes: r.get(8)?,
                    rest_s: r.get(9)?,
                    sets: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = rows;
        for e in &mut out {
            e.sets = set_stmt
                .query_map([e.id], |r| {
                    Ok(SetView {
                        id: r.get(0)?,
                        set_type: r.get(1)?,
                        weight_kg: r.get(2)?,
                        reps: r.get(3)?,
                        rpe: r.get(4)?,
                        distance_m: r.get(5)?,
                        duration_s: r.get(6)?,
                        rest_s: r.get(7)?,
                        completed: r.get(8)?,
                        avg_hr: r.get(9)?,
                        kcal: r.get(10)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(out)
    }

    /// Start time, duration (seconds; none while in progress) and the muscles trained (primary or
    /// secondary, any exercise) of every workout, oldest first.
    pub fn workout_times(&self) -> Result<Vec<(String, Option<i64>, Vec<String>)>> {
        let mut stmt = self.conn().prepare_cached(
            "SELECT w.started_at_local, w.duration_s, group_concat(DISTINCT em.muscle_id)
             FROM workout w
             LEFT JOIN workout_exercise we ON we.workout_id = w.id
             LEFT JOIN exercise_muscle em ON em.exercise_id = we.exercise_id
             GROUP BY w.id ORDER BY w.started_at_local",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let muscles: Option<String> = r.get(2)?;
                Ok((r.get(0)?, r.get(1)?, muscles.map(|m| m.split(',').map(String::from).collect()).unwrap_or_default()))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Newest-first page of workout cards. Pass the last card's `(started_at_local, id)` as
    /// `before` to get the next page.
    pub fn history_page(&self, before: Option<(&str, i64)>, limit: usize) -> Result<Vec<WorkoutCard>> {
        let (t, id) = before.unwrap_or(("9999", i64::MAX));
        let mut stmt = self.conn().prepare_cached(
            "SELECT id, name, started_at_local, duration_s FROM workout
             WHERE finished = 1 AND (started_at_local, id) < (?1, ?2)
             ORDER BY started_at_local DESC, id DESC LIMIT ?3",
        )?;
        let heads: Vec<(i64, String, String, Option<i64>)> = stmt
            .query_map(params![t, id, limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let prs = self.pr_counts(&heads.iter().map(|h| h.0).collect::<Vec<_>>())?;
        let mut cards = Vec::with_capacity(heads.len());
        for (id, name, started_at_local, duration_s) in heads {
            let exercises = self.exercises_of(id)?;
            cards.push(WorkoutCard {
                id,
                name,
                started_at_local,
                duration_s,
                sets: exercises.iter().map(|e| e.sets.len()).sum(),
                volume_kg: exercises.iter().map(|e| volume(&e.sets, &e.load_mode)).sum(),
                prs: prs.get(&id).copied().unwrap_or(0),
                exercises: exercises
                    .iter()
                    .map(|e| CardExercise { name: e.name.clone(), superset: e.superset, sets: e.sets.len(), best: best_set(&e.sets) })
                    .collect(),
            });
        }
        Ok(cards)
    }

    pub fn workout_detail(&self, id: i64) -> Result<Option<WorkoutDetail>> {
        let head = self
            .conn()
            .query_row(
                "SELECT name, started_at_local, tz_name, duration_s, notes, finished, template_id, rest_ends_at, rest_total_s, avg_hr, max_hr
                 FROM workout WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?),
                        (r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?),
                        (r.get(9)?, r.get(10)?),
                    ))
                },
            )
            .optional()?;
        let Some(((name, started_at_local, tz_name, duration_s, notes), (finished, template_id, rest_ends_at, rest_total_s), (avg_hr, max_hr))) =
            head
        else {
            return Ok(None);
        };
        let exercises = self.exercises_of(id)?;
        Ok(Some(WorkoutDetail {
            id,
            finished,
            template_id,
            rest_ends_at,
            rest_total_s,
            prs: self.prs_for_workout(id)?,
            muscle_sets: self.workout_muscle_sets(id)?,
            name,
            started_at_local,
            tz_name,
            duration_s,
            notes,
            volume_kg: exercises.iter().map(|e| volume(&e.sets, &e.load_mode)).sum(),
            exercises,
            avg_hr,
            max_hr,
        }))
    }
}
