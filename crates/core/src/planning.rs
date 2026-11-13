//! Schedules, workout suggestions, program phases and weekly set targets.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::stats::week_start;
use crate::templates::Template;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
    pub id: Option<i64>,
    pub name: String,
    /// "weekly" (day 0 = Monday) or "cycle" (day 0 = start date).
    pub kind: String,
    pub start_date: String,
    pub cycle_length: i64,
    /// Template per day index; `None` = rest.
    pub days: Vec<Option<i64>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggested {
    pub template_id: i64,
    /// "today", "missed" or "upcoming".
    pub reason: String,
    pub planned_date: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    pub is_rest_day: bool,
    /// Today's planned workout is already done.
    pub done_today: bool,
    /// Best first.
    pub items: Vec<Suggested>,
    /// Next planned workout after today (shown on rest days).
    pub next: Option<Suggested>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub id: Option<i64>,
    pub name: String,
    /// "program" or "nutrition"
    pub lane: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub color: Option<String>,
    pub notes: Option<String>,
}

/// One stretch of cut / maintain / gain (a nutrition phase), with an optional weekly target.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GoalEntry {
    pub phase_id: i64,
    pub goal: Goal,
    pub start_date: String,
    pub end_date: Option<String>,
    /// Target change in bodyweight per week (kg; negative = losing). `None` uses the default
    /// research-based % range for the goal.
    pub weekly_rate_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuscleTarget {
    pub muscle_id: String,
    pub min_sets: f64,
    pub max_sets: f64,
}

/// What "set up from history" proposes: phases from recurring workout names, and for the most
/// recent phase, templates (one per name, from its latest workout) and a weekly schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryProposal {
    pub phases: Vec<ProposedPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedPhase {
    pub name: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub workout_names: Vec<String>,
    /// Weekday (0 = Monday) each name is usually done on, by name.
    pub usual_weekday: Vec<(String, u32)>,
}

/// What the user is currently trying to do with bodyweight. Stored as the active nutrition phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Goal {
    Cut,
    Maintain,
    Gain,
}

impl Goal {
    pub fn label(self) -> &'static str {
        match self {
            Goal::Cut => "Cut",
            Goal::Maintain => "Maintain",
            Goal::Gain => "Gain",
        }
    }

    fn from_phase_name(name: &str) -> Option<Goal> {
        match name.trim().to_lowercase().as_str() {
            "cut" | "cutting" | "diet" | "fat loss" => Some(Goal::Cut),
            "maintain" | "maintenance" => Some(Goal::Maintain),
            "gain" | "bulk" | "bulking" | "lean bulk" => Some(Goal::Gain),
            _ => None,
        }
    }

    /// Target rate of bodyweight change, % of bodyweight per week (low, high).
    /// - Cut: −0.5 to −1 %/week (Helms, Aragon & Fitschen 2014, JISSN 11:20).
    /// - Gain: +0.25 to +0.5 %/week (Iraki et al. 2019, Sports 7:154).
    /// - Maintain: within ±0.25 %/week (a tolerance band chosen here, not from a study).
    pub fn weekly_rate_pct(self) -> (f64, f64) {
        match self {
            Goal::Cut => (-1.0, -0.5),
            Goal::Maintain => (-0.25, 0.25),
            Goal::Gain => (0.25, 0.5),
        }
    }
}

/// Default weekly set targets (fractional sets: primary 1, secondary ½).
///
/// Based on hypertrophy dose-response research rather than anyone's training:
/// - Schoenfeld, Ogborn & Krieger 2017 (J Sports Sci): ≥10 weekly sets per muscle grew more than
///   fewer sets.
/// - Baz-Valle et al. 2022 (J Hum Kinet): 12–20 weekly sets per muscle suggested for trained
///   lifters, with little extra from going above 20.
/// - Pelland et al. (Sports Med 2025; SportRxiv 2024): gains keep rising with volume but with
///   diminishing returns; counting indirect sets as ½ fit the hypertrophy data best.
/// - On a cut the aim is keeping muscle, which takes much less: size was maintained with as
///   little as 1 set per exercise once a week when load stayed heavy (Spiering et al. 2021,
///   J Strength Cond Res), so the minimum drops to 6.
///
/// The research doesn't give separate numbers per muscle, so every muscle gets the same range;
/// neck work is optional (minimum 0).
pub fn default_target(muscle: &str, goal: Option<Goal>) -> (f64, f64) {
    let min = if goal == Some(Goal::Cut) { 6.0 } else { 10.0 };
    match muscle {
        "neck" => (0.0, 20.0),
        _ => (min, 20.0),
    }
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").map_err(|_| Error::Format(format!("bad date {s:?}")))
}

impl Schedule {
    /// Template planned on `d` (None = rest or before the start).
    pub fn planned(&self, d: NaiveDate) -> Option<i64> {
        let start = parse_date(&self.start_date).ok()?;
        if d < start {
            return None;
        }
        let idx = match self.kind.as_str() {
            "weekly" => d.weekday().num_days_from_monday() as usize,
            _ => ((d - start).num_days() % self.cycle_length.max(1)) as usize,
        };
        self.days.get(idx).copied().flatten()
    }

    /// First day of the window `d` falls in: its week (weekly) or cycle iteration.
    pub fn window_start(&self, d: NaiveDate) -> NaiveDate {
        let start = parse_date(&self.start_date).unwrap_or(d);
        match self.kind.as_str() {
            "weekly" => week_start(d).max(start),
            _ => {
                let len = self.cycle_length.max(1);
                let k = (d - start).num_days().div_euclid(len);
                start + Duration::days(k * len)
            }
        }
    }
}

/// The suggestion rule. `done` lists (date, template id) of finished workouts.
///
/// Within the current window (week or cycle): planned workouts not yet done are open. Today's
/// open workout comes first (the expected rest happened), then missed ones, oldest first. On a
/// rest day missed workouts come first. Misses never carry over into the next window.
pub fn suggest(schedule: &Schedule, today: NaiveDate, done: &[(NaiveDate, i64)]) -> Suggestion {
    let ws = schedule.window_start(today);
    let mut open: Vec<(NaiveDate, i64)> = Vec::new();
    let mut d = ws;
    while d <= today {
        if let Some(t) = schedule.planned(d) {
            open.push((d, t));
        }
        d += Duration::days(1);
    }
    let mut done_in_window: Vec<(NaiveDate, i64)> = done.iter().copied().filter(|(dd, _)| *dd >= ws && *dd <= today).collect();
    done_in_window.sort();
    let today_planned = schedule.planned(today);
    let mut done_today = false;
    for (dd, t) in done_in_window {
        // A workout done today satisfies today's plan first; otherwise the earliest open one.
        let pos = if dd == today && today_planned == Some(t) && open.iter().any(|o| o.0 == today) {
            open.iter().position(|o| o.0 == today && o.1 == t)
        } else {
            open.iter().position(|o| o.1 == t)
        };
        if let Some(p) = pos {
            if open[p].0 == today {
                done_today = true;
            }
            open.remove(p);
        }
    }
    let item = |(d, t): (NaiveDate, i64), reason: &str| Suggested { template_id: t, reason: reason.into(), planned_date: d.to_string() };
    let mut items: Vec<Suggested> = open.iter().filter(|o| o.0 == today).map(|&o| item(o, "today")).collect();
    items.extend(open.iter().filter(|o| o.0 < today).map(|&o| item(o, "missed")));

    let mut next = None;
    for i in 1..=60 {
        let d = today + Duration::days(i);
        if let Some(t) = schedule.planned(d) {
            next = Some(item((d, t), "upcoming"));
            break;
        }
    }
    Suggestion { is_rest_day: today_planned.is_none(), done_today, items, next }
}

impl Db {
    pub fn active_schedule(&self) -> Result<Option<Schedule>> {
        let head: Option<(i64, String, String, String, i64)> = self
            .conn()
            .query_row(
                "SELECT id, name, kind, start_date, cycle_length FROM schedule WHERE active = 1 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let Some((id, name, kind, start_date, cycle_length)) = head else { return Ok(None) };
        let len = if kind == "weekly" { 7 } else { cycle_length };
        let mut days = vec![None; len as usize];
        let mut stmt = self.conn().prepare("SELECT day_index, template_id FROM schedule_slot WHERE schedule_id = ?1")?;
        for row in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)))? {
            let (i, t) = row?;
            if let Some(slot) = days.get_mut(i as usize) {
                *slot = t;
            }
        }
        Ok(Some(Schedule { id: Some(id), name, kind, start_date, cycle_length: len, days }))
    }

    /// Saves a schedule and makes it the active one.
    pub fn save_schedule(&mut self, s: &Schedule) -> Result<i64> {
        if !matches!(s.kind.as_str(), "weekly" | "cycle") {
            return Err(Error::Format(format!("unknown schedule kind {:?}", s.kind)));
        }
        parse_date(&s.start_date)?;
        let len = if s.kind == "weekly" { 7 } else { s.cycle_length.max(1) };
        let tx = self.conn_mut().transaction()?;
        tx.execute("UPDATE schedule SET active = 0", [])?;
        let id = match s.id {
            Some(id) => {
                tx.execute(
                    "UPDATE schedule SET name = ?2, kind = ?3, start_date = ?4, cycle_length = ?5, active = 1 WHERE id = ?1",
                    params![id, s.name, s.kind, s.start_date, len],
                )?;
                tx.execute("DELETE FROM schedule_slot WHERE schedule_id = ?1", [id])?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO schedule (name, kind, start_date, cycle_length, active) VALUES (?1, ?2, ?3, ?4, 1)",
                    params![s.name, s.kind, s.start_date, len],
                )?;
                tx.last_insert_rowid()
            }
        };
        for (i, t) in s.days.iter().enumerate().take(len as usize) {
            if let Some(t) = t {
                tx.execute("INSERT INTO schedule_slot (schedule_id, day_index, template_id) VALUES (?1, ?2, ?3)", params![id, i as i64, t])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    /// Finished workouts as (date, template id). Imported workouts without a template count
    /// for the template with the same name.
    pub(crate) fn done_templates(&self, from: NaiveDate) -> Result<Vec<(NaiveDate, i64)>> {
        let mut stmt = self.conn().prepare(
            "SELECT substr(w.started_at_local, 1, 10), coalesce(w.template_id, (SELECT t.id FROM template t WHERE t.name = w.name AND t.archived = 0))
             FROM workout w WHERE w.finished = 1 AND substr(w.started_at_local, 1, 10) >= ?1",
        )?;
        let rows = stmt.query_map([from.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (d, t) = row?;
            if let Some(t) = t {
                out.push((parse_date(&d)?, t));
            }
        }
        Ok(out)
    }

    pub fn suggestion(&self, today: NaiveDate) -> Result<Option<Suggestion>> {
        let Some(s) = self.active_schedule()? else { return Ok(None) };
        let done = self.done_templates(s.window_start(today))?;
        Ok(Some(suggest(&s, today, &done)))
    }

    /// Weekly sets per muscle the active schedule produces, optionally with one template
    /// replaced by an unsaved edit. Cycles are scaled to 7 days.
    pub fn projected_weekly_sets(&self, edited: Option<&Template>) -> Result<Vec<(String, f64)>> {
        let Some(s) = self.active_schedule()? else { return Ok(Vec::new()) };
        let scale = 7.0 / s.days.len().max(1) as f64;
        let mut templates: HashMap<i64, Template> = self.list_templates()?.into_iter().filter_map(|t| Some((t.id?, t))).collect();
        if let Some(e) = edited {
            if let Some(id) = e.id {
                templates.insert(id, e.clone());
            }
        }
        let ids: Vec<i64> = s.days.iter().flatten().copied().collect();
        Ok(self.sets_from_templates(&templates, &ids, scale)?.into_iter().collect())
    }

    /// Sets per muscle if each of `ids` is done once as planned.
    pub(crate) fn template_muscle_sets(&self, ids: &[i64], scale: f64) -> Result<BTreeMap<String, f64>> {
        let templates: HashMap<i64, Template> = self.list_templates()?.into_iter().filter_map(|t| Some((t.id?, t))).collect();
        self.sets_from_templates(&templates, ids, scale)
    }

    fn sets_from_templates(&self, templates: &HashMap<i64, Template>, ids: &[i64], scale: f64) -> Result<BTreeMap<String, f64>> {
        let secondary = self.secondary_set_weight()?;
        let muscles = self.muscle_map()?;
        let mut acc: BTreeMap<String, f64> = BTreeMap::new();
        for t in ids {
            let Some(t) = templates.get(t) else { continue };
            for e in &t.exercises {
                let sets = e.target_sets.unwrap_or(3) as f64;
                for (m, primary) in muscles.get(&e.exercise_id).into_iter().flatten() {
                    *acc.entry(m.clone()).or_default() += sets * scale * if *primary { 1.0 } else { secondary };
                }
            }
        }
        Ok(acc)
    }

    /// Every cut / maintain / gain stretch, oldest first.
    pub fn goal_history(&self) -> Result<Vec<GoalEntry>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, name, start_date, end_date, weekly_rate_kg FROM phase WHERE lane = 'nutrition' ORDER BY start_date, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<f64>>(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (phase_id, name, start_date, end_date, weekly_rate_kg) = row?;
            if let Some(goal) = Goal::from_phase_name(&name) {
                out.push(GoalEntry { phase_id, goal, start_date, end_date, weekly_rate_kg });
            }
        }
        Ok(out)
    }

    /// The goal stretch active on `day`, if any.
    pub fn goal_on(&self, day: NaiveDate) -> Result<Option<GoalEntry>> {
        let d = day.to_string();
        Ok(self
            .goal_history()?
            .into_iter()
            .filter(|g| g.start_date.as_str() <= d.as_str() && g.end_date.as_deref().map_or(true, |e| e >= d.as_str()))
            .next_back())
    }

    pub fn current_goal(&self, today: NaiveDate) -> Result<Option<Goal>> {
        Ok(self.goal_on(today)?.map(|g| g.goal))
    }

    /// Records `goal` from `start` on. A goal stretch covering `start` ends the day before; if a
    /// later stretch exists the new one ends the day before it, so the history stays in order.
    /// Setting a goal on a day that already starts one replaces it.
    pub fn set_goal(&mut self, goal: Goal, start: NaiveDate, weekly_rate_kg: Option<f64>) -> Result<()> {
        let d = start.to_string();
        let day_before = (start - Duration::days(1)).to_string();
        let tx = self.conn_mut().transaction()?;
        tx.execute("DELETE FROM phase WHERE lane = 'nutrition' AND start_date = ?1", [&d])?;
        tx.execute(
            "UPDATE phase SET end_date = ?2 WHERE lane = 'nutrition' AND start_date < ?1 AND (end_date IS NULL OR end_date >= ?1)",
            params![d, day_before],
        )?;
        let next: Option<String> = tx
            .query_row(
                "SELECT min(start_date) FROM phase WHERE lane = 'nutrition' AND start_date > ?1",
                [&d],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let end = next.map(|n| (parse_date(&n).expect("stored dates are valid") - Duration::days(1)).to_string());
        tx.execute(
            "INSERT INTO phase (name, lane, start_date, end_date, weekly_rate_kg) VALUES (?1, 'nutrition', ?2, ?3, ?4)",
            params![goal.label(), d, end, weekly_rate_kg],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn muscle_targets(&self, today: NaiveDate) -> Result<Vec<MuscleTarget>> {
        let goal = self.current_goal(today)?;
        let mut stmt = self.conn().prepare(
            "SELECT m.id, t.min_sets, t.max_sets FROM muscle m LEFT JOIN muscle_target t ON t.muscle_id = m.id ORDER BY m.position",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, Option<f64>>(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (m, lo, hi) = row?;
            let (dlo, dhi) = default_target(&m, goal);
            out.push(MuscleTarget { min_sets: lo.unwrap_or(dlo), max_sets: hi.unwrap_or(dhi), muscle_id: m });
        }
        Ok(out)
    }

    pub fn set_muscle_target(&mut self, muscle_id: &str, min_sets: f64, max_sets: f64) -> Result<()> {
        if min_sets < 0.0 || max_sets < min_sets {
            return Err(Error::Format("target range must be 0 ≤ min ≤ max".into()));
        }
        self.conn().execute(
            "INSERT INTO muscle_target (muscle_id, min_sets, max_sets) VALUES (?1, ?2, ?3)
             ON CONFLICT (muscle_id) DO UPDATE SET min_sets = excluded.min_sets, max_sets = excluded.max_sets",
            params![muscle_id, min_sets, max_sets],
        )?;
        Ok(())
    }

    pub fn list_phases(&self) -> Result<Vec<Phase>> {
        let mut stmt = self.conn().prepare("SELECT id, name, lane, start_date, end_date, color, notes FROM phase ORDER BY start_date")?;
        let rows = stmt.query_map([], |r| {
            Ok(Phase {
                id: r.get(0)?,
                name: r.get(1)?,
                lane: r.get(2)?,
                start_date: r.get(3)?,
                end_date: r.get(4)?,
                color: r.get(5)?,
                notes: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn save_phase(&mut self, p: &Phase) -> Result<i64> {
        if !matches!(p.lane.as_str(), "program" | "nutrition") {
            return Err(Error::Format(format!("unknown phase lane {:?}", p.lane)));
        }
        parse_date(&p.start_date)?;
        if let Some(e) = &p.end_date {
            if parse_date(e)? < parse_date(&p.start_date)? {
                return Err(Error::Format("phase ends before it starts".into()));
            }
        }
        match p.id {
            Some(id) => {
                self.conn().execute(
                    "UPDATE phase SET name = ?2, lane = ?3, start_date = ?4, end_date = ?5, color = ?6, notes = ?7 WHERE id = ?1",
                    params![id, p.name, p.lane, p.start_date, p.end_date, p.color, p.notes],
                )?;
                Ok(id)
            }
            None => {
                self.conn().execute(
                    "INSERT INTO phase (name, lane, start_date, end_date, color, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![p.name, p.lane, p.start_date, p.end_date, p.color, p.notes],
                )?;
                Ok(self.conn().last_insert_rowid())
            }
        }
    }

    pub fn delete_phase(&mut self, id: i64) -> Result<()> {
        self.conn().execute("DELETE FROM phase WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Groups recurring workout names (4+ workouts) whose date ranges overlap into phases.
    pub fn propose_from_history(&self) -> Result<HistoryProposal> {
        let mut stmt = self.conn().prepare(
            "SELECT name, min(substr(started_at_local, 1, 10)), max(substr(started_at_local, 1, 10)), COUNT(*)
             FROM workout WHERE finished = 1 GROUP BY name HAVING COUNT(*) >= 4 ORDER BY 2",
        )?;
        let names: Vec<(String, String, String, i64)> =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        // A name only belongs to a program if it's done at least a quarter as often as the
        // program's most frequent workout; occasional ones ("Evening Workout") are ad hoc.
        let mut groups: Vec<(Vec<String>, String, String)> = Vec::new();
        let mut sorted = names.clone();
        sorted.sort_by(|a, b| b.3.cmp(&a.3));
        let mut assigned: Vec<(String, String, String, i64)> = Vec::new();
        for n in &names {
            // Overlapping, much more frequent names decide whether this one is ad hoc.
            let dominant = sorted.iter().filter(|o| o.1 <= n.2 && n.1 <= o.2).map(|o| o.3).max().unwrap_or(n.3);
            if n.3 * 4 >= dominant {
                assigned.push(n.clone());
            }
        }
        for (name, first, last, _) in assigned {
            match groups.last_mut() {
                // Overlaps the current group: same program.
                Some(g) if first <= g.2 => {
                    g.0.push(name);
                    if last > g.2 {
                        g.2 = last;
                    }
                }
                _ => groups.push((vec![name], first, last)),
            }
        }
        let n = groups.len();
        let mut phases = Vec::new();
        for (i, (names, start, end)) in groups.into_iter().enumerate() {
            let mut usual = Vec::new();
            for name in &names {
                let mut stmt = self.conn().prepare_cached(
                    "SELECT substr(started_at_local, 1, 10) FROM workout WHERE name = ?1 AND finished = 1 ORDER BY started_at_local DESC LIMIT 8",
                )?;
                let mut counts = [0usize; 7];
                for d in stmt.query_map([name], |r| r.get::<_, String>(0))? {
                    counts[parse_date(&d?)?.weekday().num_days_from_monday() as usize] += 1;
                }
                let best = (0..7).max_by_key(|&i| (counts[i], std::cmp::Reverse(i))).unwrap();
                usual.push((name.clone(), best as u32));
            }
            usual.sort_by_key(|u| u.1);
            phases.push(ProposedPhase {
                name: phase_name(&names),
                start_date: start,
                // The latest program is still running.
                end_date: (i + 1 < n).then_some(end),
                workout_names: names,
                usual_weekday: usual,
            });
        }
        Ok(HistoryProposal { phases })
    }

    /// Applies a proposal: creates program phases, templates for the latest phase's workouts
    /// (from each name's most recent workout) and a weekly schedule from their usual weekdays.
    pub fn apply_history_proposal(&mut self, p: &HistoryProposal, schedule_start: NaiveDate) -> Result<Vec<(String, String)>> {
        let mut uncertain = Vec::new();
        for ph in &p.phases {
            self.save_phase(&Phase {
                id: None,
                name: ph.name.clone(),
                lane: "program".into(),
                start_date: ph.start_date.clone(),
                end_date: ph.end_date.clone(),
                color: None,
                notes: None,
            })?;
        }
        let Some(latest) = p.phases.last() else { return Ok(uncertain) };
        let mut days = vec![None; 7];
        for (name, weekday) in &latest.usual_weekday {
            let wid: i64 = self.conn().query_row(
                "SELECT id FROM workout WHERE name = ?1 AND finished = 1 ORDER BY started_at_local DESC LIMIT 1",
                [name],
                |r| r.get(0),
            )?;
            let created = self.template_from_workout(wid, name)?;
            uncertain.extend(created.uncertain_notes);
            days[*weekday as usize] = Some(created.template_id);
        }
        self.save_schedule(&Schedule {
            id: None,
            name: latest.name.clone(),
            kind: "weekly".into(),
            start_date: schedule_start.to_string(),
            cycle_length: 7,
            days,
        })?;
        Ok(uncertain)
    }
}

/// "PHUL Day1, PHUL Day2" -> "PHUL"; "Upper A, Lower A" -> "Upper / Lower".
fn phase_name(names: &[String]) -> String {
    let first = &names[0];
    let common = names.iter().skip(1).fold(first.len(), |n, s| first.chars().zip(s.chars()).take_while(|(a, b)| a == b).count().min(n));
    let prefix = first[..common].trim();
    let prefix = prefix.trim_end_matches(|c: char| c.is_ascii_digit()).trim();
    let prefix = prefix.strip_suffix(" Day").or(prefix.strip_suffix("Day")).unwrap_or(prefix).trim();
    if names.len() > 1 && prefix.len() >= 3 {
        return prefix.to_string();
    }
    let mut words: Vec<&str> = Vec::new();
    for n in names {
        let w = n.split_whitespace().next().unwrap_or(n);
        if !words.contains(&w) {
            words.push(w);
        }
    }
    words.join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const LIMB_A: i64 = 1;
    const TORSO_A: i64 = 2;
    const LIMB_B: i64 = 3;
    const TORSO_B: i64 = 4;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Mon Limb A, Tue Torso A, Thu Limb B, Fri Torso B.
    fn weekly() -> Schedule {
        Schedule {
            id: None,
            name: "Torso/Limb".into(),
            kind: "weekly".into(),
            start_date: "2026-06-01".into(),
            cycle_length: 7,
            days: vec![Some(LIMB_A), Some(TORSO_A), None, Some(LIMB_B), Some(TORSO_B), None, None],
        }
    }

    fn ids(s: &Suggestion) -> Vec<(i64, &str)> {
        s.items.iter().map(|i| (i.template_id, i.reason.as_str())).collect()
    }

    // 2026-11-02 is a Monday.
    #[test]
    fn on_schedule() {
        let s = suggest(&weekly(), d("2026-11-03"), &[(d("2026-11-02"), LIMB_A)]);
        assert_eq!(ids(&s), [(TORSO_A, "today")]);
        assert!(!s.is_rest_day);
    }

    #[test]
    fn rest_day_shows_next() {
        let done = [(d("2026-11-02"), LIMB_A), (d("2026-11-03"), TORSO_A)];
        let s = suggest(&weekly(), d("2026-11-04"), &done);
        assert!(s.is_rest_day && s.items.is_empty());
        assert_eq!(s.next.unwrap(), Suggested { template_id: LIMB_B, reason: "upcoming".into(), planned_date: "2026-11-05".into() });
    }

    #[test]
    fn missed_day_on_rest_day_comes_first() {
        // Missed Tuesday's Torso A; Wednesday is a rest day.
        let s = suggest(&weekly(), d("2026-11-04"), &[(d("2026-11-02"), LIMB_A)]);
        assert!(s.is_rest_day);
        assert_eq!(ids(&s), [(TORSO_A, "missed")]);
    }

    #[test]
    fn todays_workout_beats_missed_one() {
        // Missed Tuesday, back on Thursday: Limb B first, Torso A second.
        let s = suggest(&weekly(), d("2026-11-05"), &[(d("2026-11-02"), LIMB_A)]);
        assert_eq!(ids(&s), [(LIMB_B, "today"), (TORSO_A, "missed")]);
    }

    #[test]
    fn misses_do_not_carry_into_next_week() {
        // Skipped Thu and Fri; next Monday only suggests Monday's workout.
        let done = [(d("2026-11-02"), LIMB_A), (d("2026-11-03"), TORSO_A)];
        let s = suggest(&weekly(), d("2026-11-09"), &done);
        assert_eq!(ids(&s), [(LIMB_A, "today")]);
    }

    #[test]
    fn catching_up_satisfies_the_missed_slot() {
        // Did Torso A on Wednesday (a day late): Thursday is back to normal.
        let done = [(d("2026-11-02"), LIMB_A), (d("2026-11-04"), TORSO_A)];
        let s = suggest(&weekly(), d("2026-11-05"), &done);
        assert_eq!(ids(&s), [(LIMB_B, "today")]);
    }

    #[test]
    fn done_today() {
        let done = [(d("2026-11-02"), LIMB_A)];
        let s = suggest(&weekly(), d("2026-11-02"), &done);
        assert!(s.done_today && s.items.is_empty());
        assert_eq!(s.next.unwrap().template_id, TORSO_A);
    }

    #[test]
    fn custom_cycle() {
        // 6-day cycle starting Sunday 2026-11-01: Torso / Limb / Rest / Torso / Limb / Rest.
        let s = Schedule {
            id: None,
            name: "6-day".into(),
            kind: "cycle".into(),
            start_date: "2026-11-01".into(),
            cycle_length: 6,
            days: vec![Some(TORSO_A), Some(LIMB_A), None, Some(TORSO_B), Some(LIMB_B), None],
        };
        assert_eq!(s.planned(d("2026-11-01")), Some(TORSO_A));
        assert_eq!(s.planned(d("2026-11-03")), None);
        assert_eq!(s.planned(d("2026-11-07")), Some(TORSO_A), "second cycle");
        assert_eq!(s.planned(d("2026-10-31")), None, "before start");
        assert_eq!(s.window_start(d("2026-11-09")), d("2026-11-07"));
        // Missed Limb A on day 2; on day 4 (Torso B) both are offered, today's first.
        let r = suggest(&s, d("2026-11-04"), &[(d("2026-11-01"), TORSO_A)]);
        assert_eq!(ids(&r), [(TORSO_B, "today"), (LIMB_A, "missed")]);
        // Next cycle: the old miss is gone.
        let r = suggest(&s, d("2026-11-07"), &[(d("2026-11-01"), TORSO_A)]);
        assert_eq!(ids(&r), [(TORSO_A, "today")]);
    }

    #[test]
    fn goals_are_nutrition_phases() {
        let mut db = Db::open_in_memory().unwrap();
        assert_eq!(db.current_goal(d("2026-11-01")).unwrap(), None);
        db.set_goal(Goal::Gain, d("2026-09-01"), None).unwrap();
        db.set_goal(Goal::Cut, d("2026-11-01"), None).unwrap();
        assert_eq!(db.current_goal(d("2026-10-15")).unwrap(), Some(Goal::Gain));
        assert_eq!(db.current_goal(d("2026-11-02")).unwrap(), Some(Goal::Cut));
        let phases = db.list_phases().unwrap();
        assert_eq!((phases[0].name.as_str(), phases[0].end_date.as_deref()), ("Gain", Some("2026-10-31")));
        // Changing your mind the same day replaces the phase rather than leaving a 0-day one.
        db.set_goal(Goal::Maintain, d("2026-11-01"), None).unwrap();
        assert_eq!(db.list_phases().unwrap().len(), 2);
        assert_eq!(db.current_goal(d("2026-11-01")).unwrap(), Some(Goal::Maintain));

        // A cut lowers the default minimum; a saved target wins over defaults.
        db.set_goal(Goal::Cut, d("2026-11-01"), None).unwrap();
        let t = db.muscle_targets(d("2026-11-01")).unwrap();
        let quads = t.iter().find(|x| x.muscle_id == "quads").unwrap();
        assert_eq!((quads.min_sets, quads.max_sets), (6.0, 20.0));
        db.set_muscle_target("quads", 12.0, 18.0).unwrap();
        let quads = db.muscle_targets(d("2026-11-01")).unwrap().into_iter().find(|x| x.muscle_id == "quads").unwrap();
        assert_eq!((quads.min_sets, quads.max_sets), (12.0, 18.0));
        // Imported "Bulk" phases count as gaining.
        db.save_phase(&Phase { id: None, name: "Bulk".into(), lane: "nutrition".into(), start_date: "2027-01-01".into(), end_date: None, color: None, notes: None }).unwrap();
        assert_eq!(db.current_goal(d("2027-01-02")).unwrap(), Some(Goal::Gain));
    }

    #[test]
    fn goal_history_with_backdating_and_weekly_targets() {
        let mut db = Db::open_in_memory().unwrap();
        db.set_goal(Goal::Gain, d("2026-01-01"), Some(0.2)).unwrap();
        db.set_goal(Goal::Maintain, d("2026-09-01"), None).unwrap();
        // Recorded later: there was a cut from March to August.
        db.set_goal(Goal::Cut, d("2026-03-01"), Some(-0.45)).unwrap();
        let h: Vec<_> = db
            .goal_history()
            .unwrap()
            .into_iter()
            .map(|g| (g.goal, g.start_date, g.end_date, g.weekly_rate_kg))
            .collect();
        assert_eq!(
            h,
            [
                (Goal::Gain, "2026-01-01".to_string(), Some("2026-02-28".to_string()), Some(0.2)),
                (Goal::Cut, "2026-03-01".to_string(), Some("2026-08-31".to_string()), Some(-0.45)),
                (Goal::Maintain, "2026-09-01".to_string(), None, None),
            ]
        );
        assert_eq!(db.goal_on(d("2026-05-01")).unwrap().unwrap().weekly_rate_kg, Some(-0.45));
        assert_eq!(db.current_goal(d("2026-11-02")).unwrap(), Some(Goal::Maintain));
        // Program phases are separate and untouched.
        assert!(db.list_phases().unwrap().iter().all(|p| p.lane == "nutrition"));
    }

    #[test]
    fn phase_names() {
        let n = |v: &[&str]| phase_name(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(n(&["PHUL Day1", "PHUL Day2", "PHUL Day 3", "PHUL Day4"]), "PHUL");
        assert_eq!(n(&["Lower A", "Upper A", "Lower B", "Upper B"]), "Lower / Upper");
        assert_eq!(n(&["Limb A", "Torso A"]), "Limb / Torso");
    }
}

// ---- Built-in starter programs

const PROGRAMS_JSON: &str = include_str!("../../../data/programs.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Template name per day, Monday first; null = rest. 7 days repeat weekly; 14 days alternate
    /// two weeks (e.g. A/B/A then B/A/B).
    pub days: Vec<Option<String>>,
    /// Template name -> [slug, sets, rep min, rep max, optional superset tag].
    pub templates: BTreeMap<String, Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct ProgramsFile {
    programs: Vec<Program>,
}

pub fn programs() -> Vec<Program> {
    serde_json::from_str::<ProgramsFile>(PROGRAMS_JSON).expect("embedded programs.json is valid").programs
}
