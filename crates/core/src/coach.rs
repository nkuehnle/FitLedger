//! Reads your logs and says what's moving, what's stuck and whether the week is on course. It only
//! ever suggests: your own rating from last session outranks every rule here.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Duration, NaiveDate};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::db::Db;
use crate::health::{IMPLAUSIBLE_CHANGE, STRENGTH_DAYS, strength_point, strength_score};
use crate::planning::Goal;

/// Sessions a lift needs in the last 6 weeks, and in the 6 before, to be judged. The older window
/// asks for less so that a short break doesn't hide nearly every lift for weeks.
const MIN_SESSIONS: usize = 3;
const MIN_SESSIONS_BEFORE: usize = 2;
const DISMISSED_KEY: &str = "lift_status_dismissed";
/// Weeks of sessions a goal's projection is fitted to, and how many sessions that needs.
const PROJECTION_WEEKS: i64 = 12;
const PROJECTION_MIN_SESSIONS: usize = 6;
const PROJECTION_MAX_DAYS: f64 = 365.0;
/// Phases shorter than this say too little to compare.
pub const MIN_PHASE_DAYS: i64 = 28;
const PHASE_END_DAYS: i64 = 21;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LiftState {
    Moving,
    Flat,
    Sliding,
    /// Too big a change to be strength: the exercise or its logging probably changed.
    Check,
}

/// What `assess` needs to know about one lift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftFacts<'a> {
    /// Best e1RM, last 6 weeks vs the 6 before.
    pub change: f64,
    /// The same comparison one window earlier, if there was one.
    pub prior_change: Option<f64>,
    pub sets_now: usize,
    pub sets_before: usize,
    /// ↑ = ↓ from last session: "up", "hold" or "down".
    pub rating: Option<&'a str>,
    pub goal: Option<Goal>,
}

/// The lift's state and at most one suggestion. Your rating comes first, then the first rule that fits.
pub fn assess(f: LiftFacts) -> (LiftState, Option<String>) {
    let score = strength_score(f.change, f.goal);
    let state = if f.change.abs() > IMPLAUSIBLE_CHANGE {
        LiftState::Check
    } else if score <= -0.4 {
        LiftState::Sliding
    } else if f.change >= 0.01 {
        LiftState::Moving
    } else {
        LiftState::Flat
    };
    let rule = match (f.rating, state) {
        (Some("hold"), _) => Some("You chose to hold. Nothing suggested.".to_string()),
        (Some("down"), _) => Some("You chose to go down. Nothing suggested.".to_string()),
        (_, LiftState::Check) => Some("That's a big jump. Check for a different machine or a logging change.".to_string()),
        (_, LiftState::Moving) => None,
        _ if f.sets_now as f64 <= 0.75 * f.sets_before as f64 => {
            Some(format!("You did fewer sets ({} vs {}), which may explain it.", f.sets_now, f.sets_before))
        }
        (_, LiftState::Flat) if f.goal == Some(Goal::Cut) => Some("Holding strength on a cut is a win.".to_string()),
        (_, LiftState::Sliding) => Some("Consider a lighter week, or check sleep and food.".to_string()),
        (_, LiftState::Flat) if f.goal != Some(Goal::Maintain) && f.prior_change.is_some_and(|c| c < 0.01) => {
            Some("Flat for a while. Try a different rep range, or add a set.".to_string())
        }
        _ => None,
    };
    (state, rule)
}

#[derive(Debug, Clone, Serialize)]
pub struct LiftStatus {
    pub exercise_id: i64,
    pub name: String,
    pub state: LiftState,
    pub change: f64,
    pub last_at: String,
    /// Last session's working sets: (weight, reps).
    pub last_sets: Vec<(f64, i64)>,
    pub rating: Option<String>,
    pub suggestion: Option<String>,
    pub goal: Option<LiftGoalView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiftsView {
    /// Needs a look first.
    pub lifts: Vec<LiftStatus>,
    /// Exercises trained lately that can't be judged: too few sessions, or not weight × reps.
    pub left_out: usize,
}

#[derive(Default)]
struct LiftWindows {
    best: [f64; 3],
    sets: [usize; 3],
    sessions: [BTreeSet<i64>; 3],
    last_workout: i64,
    last_at: String,
    last_sets: Vec<(f64, i64)>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WeekShortfall {
    pub muscle_id: String,
    pub name: String,
    pub done: f64,
    pub planned: f64,
    pub target: f64,
    pub short: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    Reached,
    OnPace,
    Behind,
    /// No target date, so there's no pace to be on.
    Open,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiftGoalView {
    pub exercise_id: i64,
    pub target_kg: f64,
    pub target_date: Option<String>,
    /// Best e1RM in the last 12 weeks.
    pub current_kg: Option<f64>,
    /// When the trend reaches the target, if it clearly does within a year.
    pub projected: Option<String>,
    pub state: GoalState,
}

/// Where a straight line through `(day, value)` reaches `target`, counted in days after `now`.
pub fn days_to_target(points: &[(f64, f64)], target: f64, now: f64) -> Option<f64> {
    if points.len() < PROJECTION_MIN_SESSIONS {
        return None;
    }
    let n = points.len() as f64;
    let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
    let var: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let slope = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / var;
    if !slope.is_finite() || slope <= 0.0 {
        return None;
    }
    let days = (mx + (target - my) / slope - now).max(0.0);
    (days <= PROJECTION_MAX_DAYS).then_some(days)
}

fn goal_state(current: Option<f64>, target: f64, projected: Option<NaiveDate>, by: Option<NaiveDate>) -> GoalState {
    if current.is_some_and(|c| c >= target) {
        return GoalState::Reached;
    }
    match (by, projected) {
        (None, _) => GoalState::Open,
        (Some(by), Some(p)) if p <= by => GoalState::OnPace,
        _ => GoalState::Behind,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PhaseSummary {
    pub id: i64,
    pub name: String,
    pub lane: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub weeks: f64,
    pub sessions_per_week: f64,
    pub sets_per_week: f64,
    /// Typical e1RM change, start to end of the phase, over lifts done in both compared phases.
    pub median_change: Option<f64>,
    pub lifts_compared: usize,
    pub bodyweight_change_kg: Option<f64>,
    /// Phases in the other lane that overlap this one: (name, weeks), most first.
    pub overlaps: Vec<(String, f64)>,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[m - 1] + v[m]) / 2.0 } else { v[m] })
}

impl Db {
    pub fn lift_status(&self, today: NaiveDate) -> Result<LiftsView> {
        let goal = self.current_goal(today)?;
        let formula = self.formula()?;
        let (split, split2) = (today - Duration::days(STRENGTH_DAYS), today - Duration::days(2 * STRENGTH_DAYS));
        let from = (split2 - Duration::days(STRENGTH_DAYS)).to_string();
        let mut lifts: BTreeMap<i64, LiftWindows> = BTreeMap::new();
        for s in self.work_sets(None, Some(&from), Some(&today.to_string()))? {
            let Some(ex) = s.exercise_id else { continue };
            let d = s.date();
            let w = usize::from(d <= split) + usize::from(d < split2);
            let l = lifts.entry(ex).or_default();
            if s.set_type != "warmup" {
                if l.last_workout != s.workout_id {
                    (l.last_workout, l.last_at, l.last_sets) = (s.workout_id, s.at.clone(), Vec::new());
                }
                if let (Some(kg), Some(r)) = (s.weight_kg, s.reps) {
                    l.last_sets.push((kg, r));
                }
            }
            if let Some((_, e)) = strength_point(&s, formula) {
                l.best[w] = l.best[w].max(e);
                l.sets[w] += 1;
                l.sessions[w].insert(s.workout_id);
            }
        }

        let mut stmt = self.conn().prepare("SELECT id, name, kind FROM exercise")?;
        let info: HashMap<i64, (String, Option<String>)> =
            stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?.collect::<rusqlite::Result<_>>()?;
        let dismissed = self.dismissed_lifts()?;
        let (mut out, mut left_out) = (Vec::new(), 0);
        for (ex, l) in lifts {
            let Some((name, kind)) = info.get(&ex) else { continue };
            let judged = kind.as_deref() == Some("weight_reps") && l.sessions[0].len() >= MIN_SESSIONS && l.sessions[1].len() >= MIN_SESSIONS_BEFORE;
            if !judged {
                left_out += usize::from(l.last_at.get(..10).is_some_and(|d| d > split.to_string().as_str()));
                continue;
            }
            let rating: Option<String> = self
                .conn()
                .query_row(
                    "SELECT next_intent FROM workout_exercise WHERE workout_id = ?1 AND exercise_id = ?2 AND next_intent IS NOT NULL LIMIT 1",
                    params![l.last_workout, ex],
                    |r| r.get(0),
                )
                .optional()?;
            let change = l.best[0] / l.best[1] - 1.0;
            let (state, suggestion) = assess(LiftFacts {
                change,
                prior_change: (l.best[2] > 0.0).then(|| l.best[1] / l.best[2] - 1.0),
                sets_now: l.sets[0],
                sets_before: l.sets[1],
                rating: rating.as_deref(),
                goal,
            });
            let hidden = dismissed.get(&ex).is_some_and(|until| *until >= today);
            out.push(LiftStatus {
                exercise_id: ex,
                name: name.clone(),
                state,
                change,
                last_at: l.last_at,
                last_sets: l.last_sets,
                rating,
                suggestion: suggestion.filter(|_| !hidden),
                goal: self.lift_goal(ex, today)?,
            });
        }
        let rank = |s: LiftState| match s {
            LiftState::Sliding => 0,
            LiftState::Check => 1,
            LiftState::Flat => 2,
            LiftState::Moving => 3,
        };
        out.sort_by(|a, b| (rank(a.state), a.suggestion.is_none()).cmp(&(rank(b.state), b.suggestion.is_none())).then(a.change.total_cmp(&b.change)));
        Ok(LiftsView { lifts: out, left_out })
    }

    fn dismissed_lifts(&self) -> Result<HashMap<i64, NaiveDate>> {
        Ok(self.setting(DISMISSED_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    /// "Looks fine to me": hides a lift's suggestion for one 6-week window.
    pub fn dismiss_lift(&mut self, exercise_id: i64, today: NaiveDate) -> Result<()> {
        let mut map = self.dismissed_lifts()?;
        map.retain(|_, until| *until >= today);
        map.insert(exercise_id, today + Duration::days(STRENGTH_DAYS));
        self.set_setting(DISMISSED_KEY, &serde_json::to_string(&map).expect("json serializes"))
    }

    /// Muscles the plan should reach this week but won't if things stay as they are, worst first.
    /// A plan that never reaches a target isn't listed: that's a plan choice, not a slipped week.
    pub fn week_progress(&self, today: NaiveDate) -> Result<Vec<WeekShortfall>> {
        let Some(s) = self.active_schedule()? else { return Ok(Vec::new()) };
        let len = if s.kind == "weekly" { 7 } else { s.cycle_length.max(1) };
        let start = s.window_start(today);
        let end = if s.kind == "weekly" { crate::stats::week_start(today) + Duration::days(6) } else { start + Duration::days(len - 1) };
        let done = self.done_templates(start)?;
        // What's still ahead: today unless it's done, then later days. A missed day has gone.
        let today_open = s.planned(today).filter(|t| !done.contains(&(today, *t)));
        let later = (1..=(end - today).num_days()).filter_map(|i| s.planned(today + Duration::days(i)));
        let remaining: Vec<i64> = today_open.into_iter().chain(later).collect();

        let so_far: HashMap<String, f64> =
            self.muscle_sets(Some(&start.to_string()), Some(&today.to_string()))?.into_iter().map(|m| (m.muscle_id, m.sets)).collect();
        let planned = self.template_muscle_sets(&remaining, 1.0)?;
        let projected: HashMap<String, f64> = self.projected_weekly_sets(None)?.into_iter().collect();
        let mut stmt = self.conn().prepare("SELECT id, name FROM muscle")?;
        let names: HashMap<String, String> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut out = Vec::new();
        for t in self.muscle_targets(today)? {
            let weekly_plan = projected.get(&t.muscle_id).copied().unwrap_or(0.0);
            if t.min_sets <= 0.0 || weekly_plan < t.min_sets {
                continue;
            }
            let target = t.min_sets * len as f64 / 7.0;
            let d = so_far.get(&t.muscle_id).copied().unwrap_or(0.0);
            let p = planned.get(&t.muscle_id).copied().unwrap_or(0.0);
            if d + p < target {
                let name = names.get(&t.muscle_id).cloned().unwrap_or_default();
                out.push(WeekShortfall { muscle_id: t.muscle_id, name, done: d, planned: p, target, short: target - d - p });
            }
        }
        out.sort_by(|a, b| b.short.total_cmp(&a.short));
        Ok(out)
    }

    pub fn set_lift_goal(&mut self, exercise_id: i64, target_kg: f64, target_date: Option<&str>) -> Result<()> {
        self.conn().execute(
            "INSERT INTO lift_goal (exercise_id, target_kg, target_date) VALUES (?1, ?2, ?3)
             ON CONFLICT (exercise_id) DO UPDATE SET target_kg = excluded.target_kg, target_date = excluded.target_date",
            params![exercise_id, target_kg, target_date],
        )?;
        Ok(())
    }

    pub fn delete_lift_goal(&mut self, exercise_id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM lift_goal WHERE exercise_id = ?1", [exercise_id])?;
        Ok(())
    }

    pub fn lift_goal(&self, exercise_id: i64, today: NaiveDate) -> Result<Option<LiftGoalView>> {
        let goal: Option<(f64, Option<String>)> = self
            .conn()
            .query_row("SELECT target_kg, target_date FROM lift_goal WHERE exercise_id = ?1", [exercise_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((target_kg, target_date)) = goal else { return Ok(None) };
        let formula = self.formula()?;
        let from = (today - Duration::weeks(PROJECTION_WEEKS)).to_string();
        let mut best: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for s in self.work_sets(Some(exercise_id), Some(&from), Some(&today.to_string()))? {
            if let Some((_, e)) = strength_point(&s, formula) {
                let b = best.entry(s.date()).or_default();
                *b = b.max(e);
            }
        }
        let current_kg = best.values().copied().reduce(f64::max);
        let points: Vec<(f64, f64)> = best.iter().map(|(d, e)| ((*d - today).num_days() as f64, *e)).collect();
        let projected = days_to_target(&points, target_kg, 0.0).map(|d| today + Duration::days(d.ceil() as i64));
        let by = target_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let state = goal_state(current_kg, target_kg, projected, by);
        Ok(Some(LiftGoalView { exercise_id, target_kg, target_date, current_kg, projected: projected.map(|d| d.to_string()), state }))
    }

    /// Two phases side by side. Strength compares only lifts done in both, from the start of each
    /// phase to its end.
    pub fn phase_compare(&self, a: i64, b: i64, today: NaiveDate) -> Result<(PhaseSummary, PhaseSummary)> {
        let phases = self.list_phases()?;
        let find = |id: i64| phases.iter().find(|p| p.id == Some(id)).ok_or_else(|| crate::Error::Format("phase not found".into()));
        let formula = self.formula()?;
        let mut sides = Vec::new();
        for id in [a, b] {
            let p = find(id)?;
            let date = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| crate::Error::Format("bad phase date".into()));
            let (start, end) = (date(&p.start_date)?, p.end_date.as_deref().map(date).transpose()?.unwrap_or(today).min(today));
            let days = ((end - start).num_days() + 1).max(1);
            let edge = PHASE_END_DAYS.min(days / 2);
            let sets = self.work_sets(None, Some(&start.to_string()), Some(&end.to_string()))?;
            let (mut sessions, mut working) = (BTreeSet::new(), 0usize);
            let mut ends: HashMap<i64, (f64, f64)> = HashMap::new();
            for s in &sets {
                if s.set_type != "warmup" {
                    sessions.insert(s.workout_id);
                    working += 1;
                }
                if let Some((ex, e)) = strength_point(s, formula) {
                    let d = s.date();
                    let v = ends.entry(ex).or_default();
                    if (d - start).num_days() < edge {
                        v.0 = v.0.max(e);
                    }
                    if (end - d).num_days() < edge {
                        v.1 = v.1.max(e);
                    }
                }
            }
            let weeks = days as f64 / 7.0;
            let bodyweight_change_kg = self.bodyweight_at(end)?.zip(self.bodyweight_at(start)?).map(|(e, s)| e - s);
            let mut shared_weeks: BTreeMap<String, f64> = BTreeMap::new();
            for o in phases.iter().filter(|o| o.lane != p.lane) {
                let (os, oe) = (date(&o.start_date)?, o.end_date.as_deref().map(date).transpose()?.unwrap_or(today).min(today));
                let shared = (end.min(oe) - start.max(os)).num_days() + 1;
                if shared > 0 {
                    *shared_weeks.entry(o.name.clone()).or_default() += shared as f64 / 7.0;
                }
            }
            let mut overlaps: Vec<(String, f64)> = shared_weeks.into_iter().collect();
            overlaps.sort_by(|x, y| y.1.total_cmp(&x.1));
            let summary = PhaseSummary {
                id,
                name: p.name.clone(),
                lane: p.lane.clone(),
                start_date: p.start_date.clone(),
                end_date: p.end_date.clone(),
                weeks,
                sessions_per_week: sessions.len() as f64 / weeks,
                sets_per_week: working as f64 / weeks,
                median_change: None,
                lifts_compared: 0,
                bodyweight_change_kg,
                overlaps,
            };
            sides.push((summary, ends));
        }
        let (mut sb, eb) = sides.pop().expect("two sides");
        let (mut sa, ea) = sides.pop().expect("two sides");
        let change = |ends: &HashMap<i64, (f64, f64)>, ex: &i64| {
            let (s, e) = *ends.get(ex)?;
            let c = e / s - 1.0;
            (s > 0.0 && e > 0.0 && c.abs() <= IMPLAUSIBLE_CHANGE).then_some(c)
        };
        let both: Vec<(f64, f64)> = ea.keys().filter_map(|ex| Some((change(&ea, ex)?, change(&eb, ex)?))).collect();
        sa.median_change = median(both.iter().map(|c| c.0).collect());
        sb.median_change = median(both.iter().map(|c| c.1).collect());
        (sa.lifts_compared, sb.lifts_compared) = (both.len(), both.len());
        Ok((sa, sb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::Schedule;

    fn facts(change: f64, goal: Option<Goal>) -> LiftFacts<'static> {
        LiftFacts { change, prior_change: None, sets_now: 12, sets_before: 12, rating: None, goal }
    }
    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn your_rating_beats_the_rules() {
        let f = LiftFacts { rating: Some("hold"), ..facts(-0.1, Some(Goal::Gain)) };
        let (state, text) = assess(f);
        assert_eq!(state, LiftState::Sliding);
        assert_eq!(text.as_deref(), Some("You chose to hold. Nothing suggested."));
        assert!(assess(LiftFacts { rating: Some("down"), ..facts(0.2, None) }).1.unwrap().contains("go down"));
    }

    #[test]
    fn the_goal_decides_what_counts_as_sliding() {
        assert_eq!(assess(facts(-0.02, Some(Goal::Gain))).0, LiftState::Sliding);
        assert_eq!(assess(facts(-0.02, Some(Goal::Maintain))).0, LiftState::Flat);
        let (state, text) = assess(facts(-0.02, Some(Goal::Cut)));
        assert_eq!(state, LiftState::Flat);
        assert_eq!(text.as_deref(), Some("Holding strength on a cut is a win."));
        assert_eq!(assess(facts(-0.06, Some(Goal::Cut))).0, LiftState::Sliding);
    }

    #[test]
    fn huge_changes_are_a_data_question() {
        for change in [-0.8, 0.5] {
            assert_eq!(assess(facts(change, None)).0, LiftState::Check);
        }
    }

    #[test]
    fn suggestions_follow_the_order() {
        let fewer = LiftFacts { sets_now: 6, sets_before: 12, ..facts(-0.05, None) };
        assert!(assess(fewer).1.unwrap().contains("fewer sets (6 vs 12)"));
        assert!(assess(facts(-0.05, None)).1.unwrap().contains("lighter week"));
        assert_eq!(assess(facts(0.03, None)).1, None);
        assert_eq!(assess(facts(0.0, None)).1, None, "one flat window says nothing");
        let stuck = LiftFacts { prior_change: Some(0.0), ..facts(0.0, Some(Goal::Gain)) };
        assert!(assess(stuck).1.unwrap().contains("rep range"));
        assert_eq!(assess(LiftFacts { prior_change: Some(0.0), ..facts(0.0, Some(Goal::Maintain)) }).1, None);
    }

    #[test]
    fn projection_needs_a_clear_upward_trend() {
        let rising: Vec<(f64, f64)> = (0..6).map(|i| (i as f64 * -7.0, 100.0 - i as f64)).collect();
        let days = days_to_target(&rising, 105.0, 0.0).unwrap();
        assert!((days - 35.0).abs() < 1e-6, "{days}");
        assert_eq!(days_to_target(&rising[..5], 105.0, 0.0), None, "too few sessions");
        let flat: Vec<(f64, f64)> = (0..6).map(|i| (i as f64 * -7.0, 100.0)).collect();
        assert_eq!(days_to_target(&flat, 105.0, 0.0), None);
        let falling: Vec<(f64, f64)> = (0..6).map(|i| (i as f64 * -7.0, 100.0 + i as f64)).collect();
        assert_eq!(days_to_target(&falling, 105.0, 0.0), None);
        assert_eq!(days_to_target(&rising, 500.0, 0.0), None, "more than a year out");
    }

    #[test]
    fn goal_states() {
        let by = Some(d("2027-03-01"));
        assert_eq!(goal_state(Some(101.0), 100.0, None, by), GoalState::Reached);
        assert_eq!(goal_state(Some(90.0), 100.0, Some(d("2027-02-01")), by), GoalState::OnPace);
        assert_eq!(goal_state(Some(90.0), 100.0, Some(d("2027-04-01")), by), GoalState::Behind);
        assert_eq!(goal_state(Some(90.0), 100.0, None, by), GoalState::Behind);
        assert_eq!(goal_state(Some(90.0), 100.0, None, None), GoalState::Open);
    }

    /// One finished workout of `sets` × (weight, reps) on `slug`.
    fn log(db: &Db, day: NaiveDate, slug: &str, sets: usize, weight: f64, reps: i64) {
        let ex = db.exercise_id(slug).unwrap().unwrap();
        db.conn().execute("INSERT INTO workout (name, started_at_local) VALUES ('W', ?1)", [format!("{day} 10:00:00")]).unwrap();
        let w = db.conn().last_insert_rowid();
        db.conn().execute("INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)", params![w, slug, ex]).unwrap();
        let we = db.conn().last_insert_rowid();
        for i in 0..sets {
            db.conn()
                .execute("INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps) VALUES (?1, ?2, 'normal', ?3, ?4)", params![we, i as i64, weight, reps])
                .unwrap();
        }
    }

    #[test]
    fn lifts_are_judged_on_two_windows_and_can_be_dismissed() {
        let mut db = Db::open_in_memory().unwrap();
        let today = d("2026-06-30");
        for week in 0..12 {
            let day = today - Duration::weeks(week);
            log(&db, day, "bench-press-barbell", 3, if week < 6 { 90.0 } else { 100.0 }, 5);
        }
        log(&db, today, "squat-barbell", 3, 100.0, 5);
        let view = db.lift_status(today).unwrap();
        assert_eq!(view.left_out, 1, "one squat session can't be judged");
        assert_eq!(view.lifts.len(), 1);
        let bench = &view.lifts[0];
        assert_eq!(bench.state, LiftState::Sliding);
        assert_eq!(bench.last_sets, vec![(90.0, 5); 3]);
        assert!(bench.suggestion.is_some());

        db.dismiss_lift(bench.exercise_id, today).unwrap();
        assert_eq!(db.lift_status(today).unwrap().lifts[0].suggestion, None);
    }

    #[test]
    fn no_schedule_means_no_finish_line() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.week_progress(d("2026-06-30")).unwrap().is_empty());
    }

    /// Bench on the given days of a schedule, 12 sets a time, with the chest wanting 20 a week.
    fn planned_bench(db: &mut Db, kind: &str, start: &str, days: Vec<bool>) -> String {
        let ex = db.exercise_id("bench-press-barbell").unwrap().unwrap();
        db.conn().execute("INSERT INTO template (name) VALUES ('A')", []).unwrap();
        let t = db.conn().last_insert_rowid();
        db.conn().execute("INSERT INTO template_exercise (template_id, position, exercise_id, target_sets) VALUES (?1, 0, ?2, 12)", params![t, ex]).unwrap();
        let cycle_length = days.len() as i64;
        let days = days.into_iter().map(|on| on.then_some(t)).collect();
        db.save_schedule(&Schedule { id: None, name: "S".into(), kind: kind.into(), start_date: start.into(), cycle_length, days }).unwrap();
        let muscle: String =
            db.conn().query_row("SELECT muscle_id FROM exercise_muscle WHERE exercise_id = ?1 AND role = 'primary' LIMIT 1", [ex], |r| r.get(0)).unwrap();
        db.set_muscle_target(&muscle, 20.0, 30.0).unwrap();
        muscle
    }

    fn done_template(db: &Db, day: NaiveDate) {
        log(db, day, "bench-press-barbell", 12, 100.0, 5);
        db.conn().execute("UPDATE workout SET template_id = (SELECT max(id) FROM template) WHERE id = (SELECT max(id) FROM workout)", []).unwrap();
    }

    #[test]
    fn finish_line_counts_what_is_left_of_the_week() {
        let mut db = Db::open_in_memory().unwrap();
        // Monday and Thursday. 2026-06-29 is a Monday.
        let muscle = planned_bench(&mut db, "weekly", "2026-01-05", vec![true, false, false, true, false, false, false]);
        let short = |db: &Db, day: &str| db.week_progress(d(day)).unwrap().into_iter().find(|w| w.muscle_id == muscle);
        assert_eq!(short(&db, "2026-06-29"), None, "both sessions still ahead");
        done_template(&db, d("2026-06-29"));
        assert_eq!(short(&db, "2026-06-29"), None, "today done, Thursday ahead");
        assert_eq!(short(&db, "2026-07-02"), None, "today is still open");
        let w = short(&db, "2026-07-03").expect("Thursday was missed");
        assert_eq!((w.done, w.planned, w.target, w.short), (12.0, 0.0, 20.0, 8.0));
    }

    #[test]
    fn finish_line_scales_to_a_cycle_and_skips_plans_that_never_reach_the_target() {
        let mut db = Db::open_in_memory().unwrap();
        // 14-day cycle with sessions on days 0, 4 and 9: 18 sets a week against 20.
        let mut days = vec![false; 14];
        (days[0], days[4], days[9]) = (true, true, true);
        let muscle = planned_bench(&mut db, "cycle", "2026-06-01", days);
        assert!(db.week_progress(d("2026-06-14")).unwrap().iter().all(|w| w.muscle_id != muscle), "the plan itself is under target");

        db.set_muscle_target(&muscle, 16.0, 30.0).unwrap();
        // Nothing logged by the last day of the cycle: 32 sets due over 14 days.
        let w = db.week_progress(d("2026-06-14")).unwrap().into_iter().find(|w| w.muscle_id == muscle).unwrap();
        assert_eq!((w.done, w.planned, w.target), (0.0, 0.0, 32.0));
    }

    #[test]
    fn phases_compare_only_lifts_done_in_both() {
        let mut db = Db::open_in_memory().unwrap();
        let today = d("2026-06-30");
        let phase = |db: &mut Db, name: &str, lane: &str, start: &str, end: Option<&str>| {
            db.save_phase(&crate::planning::Phase {
                id: None,
                name: name.into(),
                lane: lane.into(),
                start_date: start.into(),
                end_date: end.map(String::from),
                color: None,
                notes: None,
            })
            .unwrap()
        };
        let a = phase(&mut db, "A", "program", "2026-01-05", Some("2026-02-15"));
        let b = phase(&mut db, "B", "program", "2026-05-01", None);
        phase(&mut db, "Cut", "nutrition", "2026-05-15", None);
        for week in 0..6 {
            log(&db, d("2026-01-05") + Duration::weeks(week), "bench-press-barbell", 3, 100.0 + week as f64 * 2.0, 5);
        }
        for week in 0..9 {
            log(&db, d("2026-05-04") + Duration::weeks(week), "bench-press-barbell", 3, 100.0, 5);
            log(&db, d("2026-05-04") + Duration::weeks(week), "squat-barbell", 3, 100.0 + week as f64 * 5.0, 5);
        }
        let (sa, sb) = db.phase_compare(a, b, today).unwrap();
        assert_eq!((sa.lifts_compared, sb.lifts_compared), (1, 1), "squat is only in B");
        assert!(sa.median_change.unwrap() > 0.05, "{:?}", sa.median_change);
        assert_eq!(sb.median_change, Some(0.0));
        assert_eq!(sb.overlaps[0].0, "Cut");
        assert!(sa.overlaps.is_empty());
        assert!(sb.weeks > 8.0, "open-ended phases run to today: {}", sb.weeks);
    }

    #[test]
    fn median_handles_even_and_empty() {
        assert_eq!(median(vec![]), None);
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![1.0, 2.0]), Some(1.5));
    }
}
