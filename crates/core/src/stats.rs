//! Stats engine: estimated 1RM, PRs, volume, sets per muscle, frequency and streaks.
//!
//! Conventions:
//! - Only completed sets of finished workouts count. Warm-ups are excluded unless the
//!   `count_warmups` setting is "true".
//! - Weights are compared in the exercise's own load mode: a session logged per hand for an
//!   exercise normally logged as a total is doubled first (and vice versa).
//! - Volume uses the full load (per hand × 2). Bodyweight is not added.
//! - Sets per muscle: 1 per primary muscle, `secondary_set_weight` (default 0.5) per secondary.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::params;
use serde::Serialize;

use crate::Result;
use crate::db::Db;

/// Bump when PR detection changes so existing databases rebuild their PR events.
pub const PR_RULES_VERSION: u32 = 2;

pub const REP_MAX_TARGETS: [i64; 6] = [1, 3, 5, 8, 10, 12];

/// The rep range a set belongs to for rep PRs: the largest target not above its reps
/// (6 reps → 5, 12+ → 12). Sets under 1 rep have none.
pub fn rep_bucket(reps: i64) -> Option<i64> {
    REP_MAX_TARGETS.iter().rev().copied().find(|&n| reps >= n)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Formula {
    Epley,
    Brzycki,
}

/// Estimated one-rep max. `None` for sets without load or reps.
pub fn e1rm(weight: f64, reps: i64, formula: Formula) -> Option<f64> {
    if weight <= 0.0 || reps < 1 {
        return None;
    }
    if reps == 1 {
        return Some(weight);
    }
    let r = reps as f64;
    match formula {
        Formula::Epley => Some(weight * (1.0 + r / 30.0)),
        Formula::Brzycki if reps < 37 => Some(weight * 36.0 / (37.0 - r)),
        Formula::Brzycki => None,
    }
}

fn factor(mode: &str) -> f64 {
    if mode == "per_hand" { 2.0 } else { 1.0 }
}

/// One completed set with everything the stats need.
#[derive(Debug, Clone)]
pub struct WorkSet {
    pub set_id: i64,
    pub workout_id: i64,
    pub workout_name: String,
    pub workout_exercise_id: i64,
    pub exercise_id: Option<i64>,
    pub set_type: String,
    /// Weight in the exercise's own load mode (comparable across sessions).
    pub weight_kg: Option<f64>,
    /// Full load moved (per hand × 2), for volume.
    pub load_kg: Option<f64>,
    pub reps: Option<i64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    /// Local start time of the workout, `YYYY-MM-DD HH:MM:SS`.
    pub at: String,
}

impl WorkSet {
    pub fn date(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.at[..10], "%Y-%m-%d").expect("stored dates are valid")
    }
    fn volume(&self) -> f64 {
        self.load_kg.unwrap_or(0.0) * self.reps.unwrap_or(0) as f64
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PrView {
    pub kind: String,
    pub reps: Option<i64>,
    pub value: f64,
    pub previous: Option<f64>,
    pub achieved_at: String,
    pub workout_id: i64,
    pub exercise_id: i64,
    pub exercise_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetBrief {
    pub set_type: String,
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub e1rm: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExerciseSession {
    pub workout_id: i64,
    pub workout_name: String,
    pub at: String,
    pub sets: Vec<SetBrief>,
    pub best_e1rm: Option<f64>,
    pub max_weight: Option<f64>,
    pub volume_kg: f64,
    pub total_reps: i64,
    pub notes: Option<String>,
    pub prs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub value: f64,
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepMax {
    pub reps: i64,
    pub weight_kg: f64,
    pub at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExerciseStats {
    pub exercise_id: i64,
    pub name: String,
    pub sessions: Vec<ExerciseSession>,
    pub best_e1rm: Option<Record>,
    pub heaviest: Option<Record>,
    pub best_volume: Option<Record>,
    pub most_reps: Option<Record>,
    pub total_volume_kg: f64,
    pub rep_maxes: Vec<RepMax>,
    /// Most reps achieved at each weight, heaviest first.
    pub reps_at_weight: Vec<RepMax>,
    pub prs: Vec<PrView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MuscleStat {
    pub muscle_id: String,
    pub sets: f64,
    pub volume_kg: f64,
    /// (exercise name, fractional sets) contributing to this muscle, most first.
    pub exercises: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekMuscleSets {
    pub week_start: String,
    pub muscle_id: String,
    pub sets: f64,
    pub volume_kg: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastTrained {
    pub muscle_id: String,
    pub last_date: Option<String>,
    pub days_ago: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodCount {
    pub start: String,
    pub workouts: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Streak {
    /// Consecutive weeks with at least one workout, ending this week (or last week if this
    /// week has none yet).
    pub current_weeks: usize,
    pub longest_weeks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkoutPoint {
    pub workout_id: i64,
    pub name: String,
    pub at: String,
    pub duration_s: Option<i64>,
    pub volume_kg: f64,
    pub sets: usize,
}

/// Joint discomfort ratings with that session's load, for the discomfort-vs-load chart.
#[derive(Debug, Clone, Serialize)]
pub struct DiscomfortPoint {
    pub at: String,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub elbow: Option<i64>,
    pub knee: Option<i64>,
    pub shoulder: Option<i64>,
    pub max_weight_kg: Option<f64>,
    pub volume_kg: f64,
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}

/// Muscles of each exercise: exercise id -> [(muscle id, is_primary)].
type MuscleMap = HashMap<i64, Vec<(String, bool)>>;

impl Db {
    pub fn formula(&self) -> Result<Formula> {
        Ok(match self.setting("e1rm_formula")?.as_deref() {
            Some("brzycki") => Formula::Brzycki,
            _ => Formula::Epley,
        })
    }

    pub fn secondary_set_weight(&self) -> Result<f64> {
        Ok(self.setting("secondary_set_weight")?.and_then(|v| v.parse().ok()).unwrap_or(0.5))
    }

    fn count_warmups(&self) -> Result<bool> {
        Ok(self.setting("count_warmups")?.as_deref() == Some("true"))
    }

    /// Completed sets of finished workouts, oldest first. Optionally one exercise and/or a
    /// local date range (inclusive).
    pub fn work_sets(&self, exercise_id: Option<i64>, from: Option<&str>, to: Option<&str>) -> Result<Vec<WorkSet>> {
        let warmups = self.count_warmups()?;
        let mut stmt = self.conn().prepare_cached(
            "SELECT s.id, w.id, w.name, we.id, we.exercise_id, s.set_type, s.weight_kg, s.reps, s.distance_m, s.duration_s,
                    w.started_at_local, coalesce(we.load_mode, e.load_mode, 'total'), coalesce(e.load_mode, 'total')
             FROM workout_set s
             JOIN workout_exercise we ON we.id = s.workout_exercise_id
             JOIN workout w ON w.id = we.workout_id
             LEFT JOIN exercise e ON e.id = we.exercise_id
             WHERE w.finished = 1 AND s.completed = 1
               AND (?1 IS NULL OR we.exercise_id = ?1)
               AND (?2 IS NULL OR substr(w.started_at_local, 1, 10) >= ?2)
               AND (?3 IS NULL OR substr(w.started_at_local, 1, 10) <= ?3)
               AND (?4 OR s.set_type != 'warmup')
             ORDER BY w.started_at_local, w.id, we.position, s.position",
        )?;
        let rows = stmt.query_map(params![exercise_id, from, to, warmups], |r| {
            let weight: Option<f64> = r.get(6)?;
            let session_mode: String = r.get(11)?;
            let exercise_mode: String = r.get(12)?;
            Ok(WorkSet {
                set_id: r.get(0)?,
                workout_id: r.get(1)?,
                workout_name: r.get(2)?,
                workout_exercise_id: r.get(3)?,
                exercise_id: r.get(4)?,
                set_type: r.get(5)?,
                weight_kg: weight.map(|w| w * factor(&session_mode) / factor(&exercise_mode)),
                load_kg: weight.map(|w| w * factor(&session_mode)),
                reps: r.get(7)?,
                distance_m: r.get(8)?,
                duration_s: r.get(9)?,
                at: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn muscle_map(&self) -> Result<MuscleMap> {
        let mut map: MuscleMap = HashMap::new();
        let mut stmt = self.conn().prepare_cached("SELECT exercise_id, muscle_id, role FROM exercise_muscle")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (e, m, role) = row?;
            map.entry(e).or_default().push((m, role == "primary"));
        }
        Ok(map)
    }

    fn exercise_names(&self) -> Result<HashMap<i64, String>> {
        let mut stmt = self.conn().prepare_cached("SELECT id, name FROM exercise")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Recomputes all PR events from history. Call after imports and any workout edit.
    pub fn rebuild_prs(&mut self) -> Result<usize> {
        let formula = self.formula()?;
        let sets = self.work_sets(None, None, None)?;
        let events = detect_prs(&sets, formula);
        let tx = self.conn_mut().transaction()?;
        tx.execute("DELETE FROM pr_event", [])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO pr_event (workout_id, workout_exercise_id, set_id, exercise_id, kind, reps, value, previous, achieved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for e in &events {
                ins.execute(params![
                    e.workout_id,
                    e.workout_exercise_id,
                    e.set_id,
                    e.exercise_id,
                    e.kind,
                    e.reps,
                    e.value,
                    e.previous,
                    e.at
                ])?;
            }
        }
        tx.commit()?;
        Ok(events.len())
    }

    pub fn prs_for_workout(&self, workout_id: i64) -> Result<Vec<PrView>> {
        self.pr_query("p.workout_id = ?1", workout_id)
    }

    fn pr_query(&self, filter: &str, id: i64) -> Result<Vec<PrView>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT p.kind, p.reps, p.value, p.previous, p.achieved_at, p.workout_id, p.exercise_id, e.name
             FROM pr_event p JOIN exercise e ON e.id = p.exercise_id WHERE {filter}
             ORDER BY p.achieved_at, p.id"
        ))?;
        let rows = stmt.query_map([id], |r| {
            Ok(PrView {
                kind: r.get(0)?,
                reps: r.get(1)?,
                value: r.get(2)?,
                previous: r.get(3)?,
                achieved_at: r.get(4)?,
                workout_id: r.get(5)?,
                exercise_id: r.get(6)?,
                exercise_name: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Number of PR events per workout, for history badges.
    pub fn pr_counts(&self, workout_ids: &[i64]) -> Result<HashMap<i64, usize>> {
        let mut out = HashMap::new();
        let mut stmt = self.conn().prepare_cached("SELECT COUNT(*) FROM pr_event WHERE workout_id = ?1")?;
        for &id in workout_ids {
            let n: i64 = stmt.query_row([id], |r| r.get(0))?;
            if n > 0 {
                out.insert(id, n as usize);
            }
        }
        Ok(out)
    }

    pub fn exercise_stats(&self, exercise_id: i64) -> Result<ExerciseStats> {
        let formula = self.formula()?;
        let name: String = self.conn().query_row("SELECT name FROM exercise WHERE id = ?1", [exercise_id], |r| r.get(0))?;
        let sets = self.work_sets(Some(exercise_id), None, None)?;
        let prs = self.pr_query("p.exercise_id = ?1", exercise_id)?;
        let mut notes: HashMap<i64, Option<String>> = HashMap::new();
        {
            let mut stmt = self.conn().prepare("SELECT id, notes FROM workout_exercise WHERE exercise_id = ?1")?;
            for row in stmt.query_map([exercise_id], |r| Ok((r.get(0)?, r.get(1)?)))? {
                let (id, n) = row?;
                notes.insert(id, n);
            }
        }

        let mut sessions: Vec<ExerciseSession> = Vec::new();
        let mut session_of: HashMap<i64, usize> = HashMap::new();
        for s in &sets {
            let idx = *session_of.entry(s.workout_exercise_id).or_insert_with(|| {
                sessions.push(ExerciseSession {
                    workout_id: s.workout_id,
                    workout_name: s.workout_name.clone(),
                    at: s.at.clone(),
                    sets: Vec::new(),
                    best_e1rm: None,
                    max_weight: None,
                    volume_kg: 0.0,
                    total_reps: 0,
                    notes: notes.get(&s.workout_exercise_id).cloned().flatten(),
                    prs: Vec::new(),
                });
                sessions.len() - 1
            });
            let sess = &mut sessions[idx];
            let est = s.weight_kg.zip(s.reps).and_then(|(w, r)| e1rm(w, r, formula));
            sess.sets.push(SetBrief {
                set_type: s.set_type.clone(),
                weight_kg: s.weight_kg,
                reps: s.reps,
                distance_m: s.distance_m,
                duration_s: s.duration_s,
                e1rm: est,
            });
            if s.set_type == "warmup" {
                continue;
            }
            sess.best_e1rm = max_opt(sess.best_e1rm, est);
            if s.reps.unwrap_or(0) > 0 {
                sess.max_weight = max_opt(sess.max_weight, s.weight_kg.filter(|&w| w > 0.0));
            }
            sess.volume_kg += s.volume();
            sess.total_reps += s.reps.unwrap_or(0);
        }
        for p in &prs {
            if let Some(sess) = sessions.iter_mut().find(|x| x.workout_id == p.workout_id) {
                let label = match p.reps {
                    Some(r) if p.kind == "rep_max" => format!("{r}RM"),
                    _ => p.kind.clone(),
                };
                if !sess.prs.contains(&label) {
                    sess.prs.push(label);
                }
            }
        }

        let working = || sets.iter().filter(|s| s.set_type != "warmup");
        let best_e1rm = first_max_by(
            working().filter_map(|s| s.weight_kg.zip(s.reps).and_then(|(w, r)| e1rm(w, r, formula)).map(|v| (v, s))),
            |a, b| a.0.total_cmp(&b.0),
        )
            .map(|(v, s)| Record { value: v, weight_kg: s.weight_kg, reps: s.reps, at: s.at.clone() });
        let heaviest = first_max_by(
            working().filter(|s| s.weight_kg.unwrap_or(0.0) > 0.0 && s.reps.unwrap_or(0) > 0),
            |a, b| a.weight_kg.unwrap().total_cmp(&b.weight_kg.unwrap()).then(a.reps.cmp(&b.reps)),
        )
            .map(|s| Record { value: s.weight_kg.unwrap(), weight_kg: s.weight_kg, reps: s.reps, at: s.at.clone() });
        let best_volume = first_max_by(sessions.iter().filter(|s| s.volume_kg > 0.0), |a, b| a.volume_kg.total_cmp(&b.volume_kg))
            .map(|s| Record { value: s.volume_kg, weight_kg: None, reps: None, at: s.at.clone() });
        let most_reps = first_max_by(working().filter(|s| s.reps.is_some()), |a, b| {
            a.reps.cmp(&b.reps).then(a.weight_kg.unwrap_or(0.0).total_cmp(&b.weight_kg.unwrap_or(0.0)))
        })
            .map(|s| Record { value: s.reps.unwrap() as f64, weight_kg: s.weight_kg, reps: s.reps, at: s.at.clone() });

        let rep_maxes = REP_MAX_TARGETS
            .iter()
            .filter_map(|&n| {
                first_max_by(working().filter(|s| s.reps.unwrap_or(0) >= n && s.weight_kg.unwrap_or(0.0) > 0.0), |a, b| {
                    a.weight_kg.unwrap().total_cmp(&b.weight_kg.unwrap())
                })
                    .map(|s| RepMax { reps: n, weight_kg: s.weight_kg.unwrap(), at: s.at.clone() })
            })
            .collect();

        let mut by_weight: BTreeMap<i64, RepMax> = BTreeMap::new();
        for s in working() {
            let (Some(w), Some(r)) = (s.weight_kg, s.reps) else { continue };
            if w <= 0.0 {
                continue;
            }
            let key = (w * 1000.0).round() as i64;
            let e = by_weight.entry(key).or_insert(RepMax { reps: r, weight_kg: w, at: s.at.clone() });
            if r > e.reps {
                *e = RepMax { reps: r, weight_kg: w, at: s.at.clone() };
            }
        }

        Ok(ExerciseStats {
            exercise_id,
            name,
            total_volume_kg: sessions.iter().map(|s| s.volume_kg).sum(),
            sessions,
            best_e1rm,
            heaviest,
            best_volume,
            most_reps,
            rep_maxes,
            reps_at_weight: by_weight.into_values().rev().collect(),
            prs,
        })
    }

    /// Fractional sets and volume per muscle over an inclusive local date range.
    pub fn muscle_sets(&self, from: Option<&str>, to: Option<&str>) -> Result<Vec<MuscleStat>> {
        let sets = self.work_sets(None, from, to)?;
        let muscles = self.muscle_map()?;
        let names = self.exercise_names()?;
        let secondary = self.secondary_set_weight()?;
        let mut acc: BTreeMap<String, (f64, f64, HashMap<i64, f64>)> = BTreeMap::new();
        for s in &sets {
            let Some(ex) = s.exercise_id else { continue };
            for (m, primary) in muscles.get(&ex).into_iter().flatten() {
                let w = if *primary { 1.0 } else { secondary };
                let e = acc.entry(m.clone()).or_default();
                e.0 += w;
                e.1 += w * s.volume();
                *e.2.entry(ex).or_default() += w;
            }
        }
        Ok(acc
            .into_iter()
            .map(|(muscle_id, (sets, volume_kg, ex))| {
                let mut exercises: Vec<(String, f64)> =
                    ex.into_iter().map(|(id, n)| (names.get(&id).cloned().unwrap_or_default(), n)).collect();
                exercises.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                MuscleStat { muscle_id, sets, volume_kg, exercises }
            })
            .collect())
    }

    /// Sets per muscle in one workout.
    pub fn workout_muscle_sets(&self, workout_id: i64) -> Result<Vec<(String, f64)>> {
        let muscles = self.muscle_map()?;
        let secondary = self.secondary_set_weight()?;
        let warmups = self.count_warmups()?;
        let mut stmt = self.conn().prepare(
            "SELECT we.exercise_id FROM workout_set s JOIN workout_exercise we ON we.id = s.workout_exercise_id
             WHERE we.workout_id = ?1 AND s.completed = 1 AND we.exercise_id IS NOT NULL AND (?2 OR s.set_type != 'warmup')",
        )?;
        let mut acc: BTreeMap<String, f64> = BTreeMap::new();
        for ex in stmt.query_map(params![workout_id, warmups], |r| r.get::<_, i64>(0))? {
            for (m, primary) in muscles.get(&ex?).into_iter().flatten() {
                *acc.entry(m.clone()).or_default() += if *primary { 1.0 } else { secondary };
            }
        }
        Ok(acc.into_iter().collect())
    }

    /// Sets and volume per muscle per Monday-starting week.
    pub fn weekly_muscle_sets(&self, from: Option<&str>, to: Option<&str>) -> Result<Vec<WeekMuscleSets>> {
        let sets = self.work_sets(None, from, to)?;
        let muscles = self.muscle_map()?;
        let secondary = self.secondary_set_weight()?;
        let mut acc: BTreeMap<(NaiveDate, String), (f64, f64)> = BTreeMap::new();
        for s in &sets {
            let Some(ex) = s.exercise_id else { continue };
            let wk = week_start(s.date());
            for (m, primary) in muscles.get(&ex).into_iter().flatten() {
                let w = if *primary { 1.0 } else { secondary };
                let e = acc.entry((wk, m.clone())).or_default();
                e.0 += w;
                e.1 += w * s.volume();
            }
        }
        Ok(acc
            .into_iter()
            .map(|((wk, muscle_id), (sets, volume_kg))| WeekMuscleSets {
                week_start: wk.to_string(),
                muscle_id,
                sets,
                volume_kg,
            })
            .collect())
    }

    /// When each muscle was last trained (as primary or secondary) on or before `today`.
    pub fn last_trained(&self, today: NaiveDate) -> Result<Vec<LastTrained>> {
        let muscles = self.muscle_map()?;
        let mut last: HashMap<String, NaiveDate> = HashMap::new();
        for s in self.work_sets(None, None, Some(&today.to_string()))? {
            let Some(ex) = s.exercise_id else { continue };
            for (m, _) in muscles.get(&ex).into_iter().flatten() {
                let d = s.date();
                last.entry(m.clone()).and_modify(|x| *x = (*x).max(d)).or_insert(d);
            }
        }
        let mut stmt = self.conn().prepare("SELECT id FROM muscle ORDER BY position")?;
        let ids: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(ids
            .into_iter()
            .map(|m| {
                let d = last.get(&m).copied();
                LastTrained {
                    last_date: d.map(|d| d.to_string()),
                    days_ago: d.map(|d| (today - d).num_days()),
                    muscle_id: m,
                }
            })
            .collect())
    }

    fn workout_dates(&self) -> Result<Vec<NaiveDate>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT substr(started_at_local, 1, 10) FROM workout WHERE finished = 1 ORDER BY started_at_local")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(NaiveDate::parse_from_str(&r?, "%Y-%m-%d").expect("stored dates are valid"));
        }
        Ok(out)
    }

    /// Workouts per week (`monthly = false`, Monday start) or per month, oldest first,
    /// including empty periods between the first workout and `today`.
    pub fn workouts_per_period(&self, monthly: bool, today: NaiveDate) -> Result<Vec<PeriodCount>> {
        let dates = self.workout_dates()?;
        let Some(&first) = dates.first() else { return Ok(Vec::new()) };
        let key = |d: NaiveDate| if monthly { month_start(d) } else { week_start(d) };
        let mut counts: BTreeMap<NaiveDate, usize> = BTreeMap::new();
        let mut p = key(first);
        while p <= key(today) {
            counts.insert(p, 0);
            p = if monthly {
                (p + Duration::days(32)).with_day(1).unwrap()
            } else {
                p + Duration::days(7)
            };
        }
        for d in dates {
            *counts.entry(key(d)).or_default() += 1;
        }
        Ok(counts.into_iter().map(|(d, n)| PeriodCount { start: d.to_string(), workouts: n }).collect())
    }

    pub fn streak(&self, today: NaiveDate) -> Result<Streak> {
        let weeks: std::collections::BTreeSet<NaiveDate> =
            self.workout_dates()?.into_iter().filter(|&d| d <= today).map(week_start).collect();
        let mut longest = 0;
        let mut run = 0;
        let mut prev: Option<NaiveDate> = None;
        for &w in &weeks {
            run = if prev == Some(w - Duration::days(7)) { run + 1 } else { 1 };
            longest = longest.max(run);
            prev = Some(w);
        }
        let this = week_start(today);
        let mut cur = if weeks.contains(&this) { this } else { this - Duration::days(7) };
        let mut current = 0;
        while weeks.contains(&cur) {
            current += 1;
            cur -= Duration::days(7);
        }
        Ok(Streak { current_weeks: current, longest_weeks: longest })
    }

    /// One point per finished workout, oldest first (duration, volume and set count charts).
    pub fn workout_series(&self) -> Result<Vec<WorkoutPoint>> {
        let sets = self.work_sets(None, None, None)?;
        let mut out: Vec<WorkoutPoint> = Vec::new();
        let mut stmt = self.conn().prepare(
            "SELECT id, name, started_at_local, duration_s FROM workout WHERE finished = 1 ORDER BY started_at_local, id",
        )?;
        let mut idx = HashMap::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))? {
            let (id, name, at, duration_s) = row?;
            idx.insert(id, out.len());
            out.push(WorkoutPoint { workout_id: id, name, at, duration_s, volume_kg: 0.0, sets: 0 });
        }
        for s in &sets {
            if let Some(&i) = idx.get(&s.workout_id) {
                out[i].volume_kg += s.volume();
                out[i].sets += 1;
            }
        }
        Ok(out)
    }

    pub fn discomfort_points(&self) -> Result<Vec<DiscomfortPoint>> {
        let mut stmt = self.conn().prepare(
            "SELECT w.started_at_local, we.exercise_id, e.name, we.elbow, we.knee, we.shoulder,
                    max(s.weight_kg), sum(coalesce(s.weight_kg, 0) * coalesce(s.reps, 0))
             FROM workout_exercise we JOIN workout w ON w.id = we.workout_id JOIN exercise e ON e.id = we.exercise_id
             LEFT JOIN workout_set s ON s.workout_exercise_id = we.id AND s.set_type != 'warmup'
             WHERE w.finished = 1 AND (we.elbow IS NOT NULL OR we.knee IS NOT NULL OR we.shoulder IS NOT NULL)
             GROUP BY we.id ORDER BY w.started_at_local",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DiscomfortPoint {
                at: r.get(0)?,
                exercise_id: r.get(1)?,
                exercise_name: r.get(2)?,
                elbow: r.get(3)?,
                knee: r.get(4)?,
                shoulder: r.get(5)?,
                max_weight_kg: r.get(6)?,
                volume_kg: r.get::<_, Option<f64>>(7)?.unwrap_or(0.0),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Bodyweight at a local date, interpolated linearly between the nearest weigh-ins
    /// (clamped to the first/last outside their range). `None` without any weigh-ins.
    pub fn bodyweight_at(&self, date: NaiveDate) -> Result<Option<f64>> {
        let d = date.to_string();
        let before: Option<(String, f64)> = self
            .conn()
            .query_row(
                "SELECT substr(measured_at_local, 1, 10), weight_kg FROM bodyweight
                 WHERE substr(measured_at_local, 1, 10) <= ?1 ORDER BY measured_at_local DESC LIMIT 1",
                [&d],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let after: Option<(String, f64)> = self
            .conn()
            .query_row(
                "SELECT substr(measured_at_local, 1, 10), weight_kg FROM bodyweight
                 WHERE substr(measured_at_local, 1, 10) >= ?1 ORDER BY measured_at_local LIMIT 1",
                [&d],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        Ok(match (before, after) {
            (Some((d0, w0)), Some((d1, w1))) => {
                let (a, b) = (parse(&d0), parse(&d1));
                let span = (b - a).num_days();
                if span == 0 { Some(w0) } else { Some(w0 + (w1 - w0) * (date - a).num_days() as f64 / span as f64) }
            }
            (Some((_, w)), None) | (None, Some((_, w))) => Some(w),
            (None, None) => None,
        })
    }
}

/// Like `Iterator::max_by` but keeps the first of equal items (records count from the first time).
fn first_max_by<T>(iter: impl Iterator<Item = T>, mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering) -> Option<T> {
    iter.fold(None, |best, x| match best {
        Some(b) if cmp(&x, &b) != std::cmp::Ordering::Greater => Some(b),
        _ => Some(x),
    })
}

fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, None) => x,
        (None, y) => y,
    }
}

#[derive(Debug, Clone)]
pub struct PrEvent {
    pub workout_id: i64,
    pub workout_exercise_id: i64,
    pub set_id: Option<i64>,
    pub exercise_id: i64,
    pub kind: &'static str,
    pub reps: Option<i64>,
    pub value: f64,
    pub previous: Option<f64>,
    pub at: String,
}

const EPS: f64 = 1e-6;

/// Walks history in order and emits an event whenever a session beats the previous best.
/// The first session of an exercise sets the baseline and is not a PR.
///
/// Kinds: `e1rm`, `weight` (heaviest set), `volume` (session), `rep_max` (heaviest weight for at
/// least N reps), `reps` (most reps in a set without added weight).
pub fn detect_prs(sets: &[WorkSet], formula: Formula) -> Vec<PrEvent> {
    #[derive(Default)]
    struct Best {
        e1rm: Option<f64>,
        weight: Option<f64>,
        volume: Option<f64>,
        reps_bw: Option<f64>,
        rep_max: HashMap<i64, f64>,
    }
    let mut best: HashMap<i64, Best> = HashMap::new();
    let mut events = Vec::new();

    // Sessions in order: sets are sorted by workout time, then exercise position.
    let mut i = 0;
    while i < sets.len() {
        let we = sets[i].workout_exercise_id;
        let mut j = i;
        while j < sets.len() && sets[j].workout_exercise_id == we {
            j += 1;
        }
        let session = &sets[i..j];
        i = j;
        let first = &session[0];
        let Some(ex) = first.exercise_id else { continue };
        let working: Vec<&WorkSet> = session.iter().filter(|s| s.set_type != "warmup").collect();
        if working.is_empty() {
            continue;
        }
        let b = best.entry(ex).or_default();
        let mut emit = |kind: &'static str, reps: Option<i64>, value: f64, prev: Option<f64>, set_id: Option<i64>| {
            if let Some(p) = prev {
                if value > p + EPS {
                    events.push(PrEvent {
                        workout_id: first.workout_id,
                        workout_exercise_id: we,
                        set_id,
                        exercise_id: ex,
                        kind,
                        reps,
                        value,
                        previous: Some(p),
                        at: first.at.clone(),
                    });
                }
            }
        };

        let loaded = || working.iter().filter(|s| s.weight_kg.unwrap_or(0.0) > 0.0 && s.reps.unwrap_or(0) > 0);
        if let Some((v, s)) = loaded()
            .filter_map(|s| e1rm(s.weight_kg.unwrap(), s.reps.unwrap(), formula).map(|v| (v, s)))
            .max_by(|a, b| a.0.total_cmp(&b.0))
        {
            emit("e1rm", None, v, b.e1rm, Some(s.set_id));
            b.e1rm = max_opt(b.e1rm, Some(v));
        }
        if let Some(s) = loaded().max_by(|a, b| a.weight_kg.unwrap().total_cmp(&b.weight_kg.unwrap())) {
            let v = s.weight_kg.unwrap();
            emit("weight", None, v, b.weight, Some(s.set_id));
            b.weight = max_opt(b.weight, Some(v));
        }
        let vol: f64 = working.iter().map(|s| s.volume()).sum();
        if vol > 0.0 {
            emit("volume", None, vol, b.volume, None);
            b.volume = max_opt(b.volume, Some(vol));
        }
        // Rep PRs: a set competes only in its own rep range, so one heavy set of 15 is a 12+
        // PR rather than a PR for every lower range too. The baseline per range is the heaviest
        // weight done for at least that many reps (a set of 15 at 100 sets the 3-rep bar too).
        for n in REP_MAX_TARGETS {
            if let Some(s) = loaded()
                .filter(|s| rep_bucket(s.reps.unwrap()) == Some(n))
                .max_by(|a, b| a.weight_kg.unwrap().total_cmp(&b.weight_kg.unwrap()))
            {
                emit("rep_max", Some(n), s.weight_kg.unwrap(), b.rep_max.get(&n).copied(), Some(s.set_id));
            }
            if let Some(best) = loaded().filter(|s| s.reps.unwrap() >= n).map(|s| s.weight_kg.unwrap()).reduce(f64::max) {
                let e = b.rep_max.entry(n).or_insert(best);
                *e = e.max(best);
            }
        }
        if let Some(s) = working
            .iter()
            .filter(|s| s.weight_kg.unwrap_or(0.0) <= 0.0 && s.reps.is_some())
            .max_by_key(|s| s.reps.unwrap())
        {
            let v = s.reps.unwrap() as f64;
            emit("reps", None, v, b.reps_bw, Some(s.set_id));
            b.reps_bw = max_opt(b.reps_bw, Some(v));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e1rm_formulas() {
        assert_eq!(e1rm(100.0, 1, Formula::Epley), Some(100.0));
        assert!((e1rm(100.0, 10, Formula::Epley).unwrap() - 133.333_333).abs() < 1e-3);
        assert!((e1rm(100.0, 10, Formula::Brzycki).unwrap() - 133.333_333).abs() < 1e-3);
        assert!((e1rm(100.0, 5, Formula::Brzycki).unwrap() - 112.5).abs() < 1e-9);
        assert_eq!(e1rm(0.0, 10, Formula::Epley), None);
        assert_eq!(e1rm(100.0, 0, Formula::Epley), None);
        assert_eq!(e1rm(100.0, 40, Formula::Brzycki), None);
    }

    #[test]
    fn rep_buckets() {
        assert_eq!(rep_bucket(0), None);
        assert_eq!(rep_bucket(1), Some(1));
        assert_eq!(rep_bucket(4), Some(3));
        assert_eq!(rep_bucket(6), Some(5));
        assert_eq!(rep_bucket(12), Some(12));
        assert_eq!(rep_bucket(25), Some(12));
    }

    #[test]
    fn weeks_start_monday() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(); // Saturday
        assert_eq!(week_start(d).to_string(), "2026-10-26");
        assert_eq!(week_start(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()).to_string(), "2026-10-26");
    }

    fn set(we: i64, ex: i64, w: f64, r: i64, at: &str) -> WorkSet {
        WorkSet {
            set_id: we * 10 + r,
            workout_id: we,
            workout_name: "W".into(),
            workout_exercise_id: we,
            exercise_id: Some(ex),
            set_type: "normal".into(),
            weight_kg: Some(w),
            load_kg: Some(w),
            reps: Some(r),
            distance_m: None,
            duration_s: None,
            at: at.into(),
        }
    }

    #[test]
    fn pr_detection() {
        let sets = vec![
            set(1, 7, 100.0, 5, "2025-01-01 10:00:00"),
            set(2, 7, 100.0, 5, "2025-01-08 10:00:00"), // equal: no PR
            set(3, 7, 105.0, 3, "2025-01-15 10:00:00"), // heavier, but lower e1RM and volume
            set(4, 7, 100.0, 8, "2025-01-22 10:00:00"), // more reps
        ];
        let ev = detect_prs(&sets, Formula::Epley);
        let kinds = |w: i64| -> Vec<String> {
            ev.iter()
                .filter(|e| e.workout_id == w)
                .map(|e| match e.reps {
                    Some(r) => format!("{}{}", e.kind, r),
                    None => e.kind.to_string(),
                })
                .collect()
        };
        assert!(kinds(1).is_empty(), "baseline");
        assert!(kinds(2).is_empty(), "ties aren't PRs");
        // 105×3 is a 3-rep PR; it doesn't also count as a 1-rep PR.
        assert_eq!(kinds(3), ["weight", "rep_max3"]);
        // 100×8: e1RM 126.7 > 116.7, volume 800 > 500; 5RM 100 not > 100; 8RM is a first.
        assert_eq!(kinds(4), ["e1rm", "volume"]);
    }
}
