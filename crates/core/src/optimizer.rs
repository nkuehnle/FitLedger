//! Routine review: checks the active schedule against the weekly set targets and what recent
//! training shows, and proposes small, specific changes to its templates.
//!
//! Every check is a plain rule over your own data; nothing leaves the device. Where a rule leans
//! on research it says so here, and where a threshold was simply chosen it says that too:
//!
//! - **Set targets**: the plan's weekly sets per muscle against the target range (see
//!   `planning::default_target` for where the defaults come from).
//! - **Once a week**: a muscle whose weekly sets all land on one day, with at least
//!   `CROWDED_DAY_SETS` on it. Frequency itself doesn't seem to add growth when weekly sets are
//!   equal (Schoenfeld, Grgic & Krieger 2019, J Sports Sci), so this is only about how many hard
//!   sets one session can hold; the threshold is a choice made here.
//! - **Plateaus**: `trend::stall` over the last `WINDOW_DAYS` days. On a gain only the sessions
//!   since the gain began count (flat lifts on the cut before it were the plan working), and on a
//!   cut or at maintenance a plateau is a note, not a problem. When `SHARED_STALL` of the plan's
//!   lifts have stalled together it's reported once, without swaps: that many at once points at
//!   food, sleep or progression, not at the exercises.
//! - **Near-duplicates**: exercises in one template with the same movement pattern and the same
//!   primary muscles.
//! - **Late and slipping**: strength gains are larger for the exercises done first in a session,
//!   while growth is about the same (Nunes et al. 2021, Eur J Sport Sci). An exercise in the last
//!   third of a template that is losing ground while the first third gains is flagged.
//! - **Joints**: discomfort ratings (1–5) that stay high or climb. The thresholds are a choice
//!   made here.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::exercises::ExerciseInfo;
use crate::library::library;
use crate::planning::Goal;
use crate::templates::{Template, TemplateExercise};
use crate::trend::{SessionBest, matched_change, median, stall};
use crate::{Error, Result};

/// Recent training looked at for plateaus and joint ratings.
const WINDOW_DAYS: i64 = 120;
/// A plateau only counts while the exercise is still being done.
const RECENT_DAYS: i64 = 21;
/// Sets suggested for one exercise stay within this range.
const MIN_SETS: i64 = 2;
const MAX_SETS: i64 = 5;
/// A target is missed by more than this many weekly sets before it's worth a change.
const TOLERANCE: f64 = 0.5;
/// Fractional sets for one muscle in one session before "all on one day" is flagged.
const CROWDED_DAY_SETS: f64 = 8.0;
/// Share of the plan's lifts (and at least `SHARED_STALL_MIN`) on a plateau before it's one finding.
const SHARED_STALL: f64 = 0.4;
const SHARED_STALL_MIN: usize = 5;
/// Late-and-slipping compares the last `ORDER_DAYS` days with the same stretch before.
const ORDER_DAYS: i64 = 56;

/// A change to one template that `Db::apply_routine_fix` can make. `index` is the exercise's
/// position in the template; `exercise_id` guards against the template having changed since.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fix {
    SetSets { template_id: i64, index: usize, exercise_id: i64, sets: i64 },
    Swap { template_id: i64, index: usize, exercise_id: i64, to_exercise_id: i64 },
    Add { template_id: i64, exercise_id: i64, sets: i64 },
    MoveFirst { template_id: i64, index: usize, exercise_id: i64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// "under_target" | "over_target" | "once_a_week" | "plateau" | "duplicate" | "late" | "joint"
    pub kind: &'static str,
    /// "high" | "medium" | "low"
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
    pub muscle_id: Option<String>,
    pub exercise_id: Option<i64>,
    pub template_id: Option<i64>,
    /// What the fix does, in words ("Add 1 set of Leg Curl in Limb A (3 → 4)").
    pub fix_label: Option<String>,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutineReview {
    /// Name of the active schedule; `None` when there isn't one (then there are no findings).
    pub schedule: Option<String>,
    pub findings: Vec<Finding>,
}

/// One exercise of one scheduled template.
struct Entry<'a> {
    template: &'a Template,
    template_id: i64,
    index: usize,
    ex: &'a TemplateExercise,
    info: &'a ExerciseInfo,
    /// Weekly sets one set of this exercise adds (the template may be scheduled more than once,
    /// and cycles are scaled to 7 days).
    per_set: f64,
}

impl Entry<'_> {
    fn sets(&self) -> i64 {
        self.ex.target_sets.unwrap_or(3)
    }
    fn compound(&self) -> bool {
        !matches!(self.info.pattern.as_deref(), None | Some("isolation" | "core" | "other" | "cardio"))
    }
}

fn pct(v: f64) -> String {
    format!("{}{:.1}%", if v >= 0.0 { "+" } else { "−" }, (v * 100.0).abs())
}

fn sets_text(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}") }
}

fn day(d: NaiveDate) -> String {
    d.format("%b %-d").to_string()
}

fn list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl Db {
    /// Reviews the active schedule. Findings come most serious first.
    pub fn routine_review(&self, today: NaiveDate) -> Result<RoutineReview> {
        let Some(schedule) = self.active_schedule()? else { return Ok(RoutineReview { schedule: None, findings: Vec::new() }) };
        let templates: HashMap<i64, Template> = self.list_templates()?.into_iter().filter_map(|t| Some((t.id?, t))).collect();
        let exercises: HashMap<i64, ExerciseInfo> = self.list_exercises(false)?.into_iter().map(|e| (e.id, e)).collect();
        let secondary = self.secondary_set_weight()?;
        let goal = self.current_goal(today)?;
        let muscle_name: HashMap<&str, &str> = library().muscles.iter().map(|m| (m.id.as_str(), m.name.as_str())).collect();
        let name_of = |m: &str| muscle_name.get(m).copied().unwrap_or(m).to_string();

        // The plan: each scheduled template once, with how often it comes up in a week.
        let scale = 7.0 / schedule.days.len().max(1) as f64;
        let mut times: BTreeMap<i64, usize> = BTreeMap::new();
        for t in schedule.days.iter().flatten() {
            *times.entry(*t).or_default() += 1;
        }
        let mut entries: Vec<Entry> = Vec::new();
        for (&template_id, &n) in &times {
            let Some(template) = templates.get(&template_id) else { continue };
            for (index, ex) in template.exercises.iter().enumerate() {
                let Some(info) = exercises.get(&ex.exercise_id) else { continue };
                entries.push(Entry { template, template_id, index, ex, info, per_set: n as f64 * scale });
            }
        }
        let in_plan: HashSet<i64> = entries.iter().map(|e| e.ex.exercise_id).collect();
        // Fractional sets one set of an exercise gives a muscle.
        let share = |info: &ExerciseInfo, m: &str| {
            if info.primary.iter().any(|x| x == m) {
                1.0
            } else if info.secondary.iter().any(|x| x == m) {
                secondary
            } else {
                0.0
            }
        };

        let projected: HashMap<String, f64> = self.projected_weekly_sets(None)?.into_iter().collect();
        let targets: HashMap<String, (f64, f64)> =
            self.muscle_targets(today)?.into_iter().filter(|t| t.max_sets > 0.0).map(|t| (t.muscle_id, (t.min_sets, t.max_sets))).collect();
        let from = (today - Duration::days(27)).to_string();
        let actual: HashMap<String, f64> =
            self.muscle_sets(Some(&from), Some(&today.to_string()))?.into_iter().map(|m| (m.muscle_id, m.sets / 4.0)).collect();
        let done = |m: &str| match actual.get(m) {
            Some(&a) => format!(" Over the last 4 weeks you averaged {}.", sets_text(a)),
            None => String::new(),
        };
        // Would `n` more (or fewer) weekly sets of an exercise keep every other muscle it trains
        // inside its range, or at least no further outside it?
        let others_ok = |info: &ExerciseInfo, muscle: &str, n: f64| {
            info.primary.iter().chain(&info.secondary).filter(|m| m.as_str() != muscle).all(|m| {
                let Some(&(lo, hi)) = targets.get(m) else { return true };
                let then = projected.get(m).copied().unwrap_or(0.0) + n * share(info, m);
                if n > 0.0 { then <= hi + TOLERANCE } else { then >= lo - TOLERANCE }
            })
        };

        let mut findings: Vec<Finding> = Vec::new();
        let mut under: Vec<String> = Vec::new();

        // ---- Weekly sets against targets ----
        let mut order: Vec<&String> = targets.keys().collect();
        order.sort_by_key(|m| library().muscles.iter().position(|x| &x.id == *m));
        for m in order {
            let (lo, hi) = targets[m];
            let p = projected.get(m).copied().unwrap_or(0.0);
            let range = format!("{}–{}", sets_text(lo), sets_text(hi));
            if lo > 0.0 && p < lo - TOLERANCE {
                under.push(m.clone());
                let deficit = lo - p;
                // Prefer adding sets to an exercise already there; the one with the fewest.
                let mut fix = None;
                let mut cands: Vec<&Entry> = entries.iter().filter(|e| e.info.primary.contains(m) && e.sets() < MAX_SETS).collect();
                cands.sort_by_key(|e| (e.sets(), e.index));
                for e in cands {
                    let mut n = ((deficit / e.per_set).ceil() as i64).clamp(1, MAX_SETS - e.sets());
                    while n > 0 && !others_ok(e.info, m, n as f64 * e.per_set) {
                        n -= 1;
                    }
                    if n > 0 {
                        let to = e.sets() + n;
                        fix = Some((
                            format!("Add {n} {} of {} in {} ({} → {to})", if n == 1 { "set" } else { "sets" }, e.ex.name, e.template.name, e.sets()),
                            Fix::SetSets { template_id: e.template_id, index: e.index, exercise_id: e.ex.exercise_id, sets: to },
                        ));
                        break;
                    }
                }
                if fix.is_none() {
                    // Nothing in the plan can take more sets: add an exercise you know, else a
                    // curated one, to the lightest day.
                    let lightest = times
                        .keys()
                        .filter_map(|id| templates.get(id).map(|t| (*id, t)))
                        .min_by_key(|(id, t)| (t.exercises.iter().map(|e| e.target_sets.unwrap_or(3)).sum::<i64>(), *id));
                    if let Some((template_id, t)) = lightest {
                        let per_set = times[&template_id] as f64 * scale;
                        let sets = ((deficit / per_set).ceil() as i64).clamp(MIN_SETS, 4);
                        let mut pool: Vec<&ExerciseInfo> = exercises
                            .values()
                            .filter(|e| e.primary.contains(m) && e.kind == "weight_reps" && !in_plan.contains(&e.id))
                            .filter(|e| others_ok(e, m, sets as f64 * per_set))
                            .collect();
                        pool.sort_by(|a, b| {
                            (b.sessions > 0, b.source == "liftbook" || b.is_custom, b.pattern.as_deref() == Some("isolation"), b.sessions)
                                .cmp(&(a.sessions > 0, a.source == "liftbook" || a.is_custom, a.pattern.as_deref() == Some("isolation"), a.sessions))
                                .then_with(|| a.name.cmp(&b.name))
                        });
                        if let Some(e) = pool.first() {
                            fix = Some((
                                format!("Add {} to {} ({sets} sets)", e.name, t.name),
                                Fix::Add { template_id, exercise_id: e.id, sets },
                            ));
                        }
                    }
                }
                let (fix_label, fix) = fix.unzip();
                findings.push(Finding {
                    kind: "under_target",
                    severity: if p < lo * 0.5 { "high" } else { "medium" },
                    title: format!("{} is under its weekly target", name_of(m)),
                    detail: format!("The plan gives {} sets a week; the target is {range}.{}", sets_text(p), done(m)),
                    muscle_id: Some(m.clone()),
                    exercise_id: None,
                    template_id: None,
                    fix_label,
                    fix,
                });
            } else if p > hi + TOLERANCE {
                let excess = p - hi;
                let mut fix = None;
                let mut cands: Vec<&Entry> = entries.iter().filter(|e| e.info.primary.contains(m) && e.sets() > MIN_SETS).collect();
                cands.sort_by_key(|e| (std::cmp::Reverse(e.sets()), std::cmp::Reverse(e.index)));
                for e in cands {
                    let mut n = ((excess / e.per_set).ceil() as i64).clamp(1, e.sets() - MIN_SETS);
                    while n > 0 && !others_ok(e.info, m, -(n as f64) * e.per_set) {
                        n -= 1;
                    }
                    if n > 0 {
                        let to = e.sets() - n;
                        fix = Some((
                            format!("Drop {n} {} of {} in {} ({} → {to})", if n == 1 { "set" } else { "sets" }, e.ex.name, e.template.name, e.sets()),
                            Fix::SetSets { template_id: e.template_id, index: e.index, exercise_id: e.ex.exercise_id, sets: to },
                        ));
                        break;
                    }
                }
                let (fix_label, fix) = fix.unzip();
                findings.push(Finding {
                    kind: "over_target",
                    severity: "low",
                    title: format!("{} is above its weekly target", name_of(m)),
                    detail: format!(
                        "The plan gives {} sets a week; the target is {range}. Sets past the top of the range add little and cost recovery.{}",
                        sets_text(p),
                        done(m)
                    ),
                    muscle_id: Some(m.clone()),
                    exercise_id: None,
                    template_id: None,
                    fix_label,
                    fix,
                });
            }
        }

        // ---- All of a muscle's sets on one day ----
        let mut order: Vec<&String> = targets.keys().collect();
        order.sort_by_key(|m| library().muscles.iter().position(|x| &x.id == *m));
        for m in order {
            let days: Vec<(&Template, f64)> = times
                .iter()
                .filter_map(|(id, _)| {
                    let sets: f64 = entries.iter().filter(|e| e.template_id == *id).map(|e| e.sets() as f64 * share(e.info, m)).sum();
                    (sets >= 2.0).then(|| (entries.iter().find(|e| e.template_id == *id).expect("has entries").template, sets))
                })
                .collect();
            let per_week: f64 = days.iter().map(|(t, _)| times[&t.id.expect("saved")] as f64 * scale).sum();
            if let [(t, sets)] = days[..] {
                if per_week < 1.5 && sets >= CROWDED_DAY_SETS {
                    findings.push(Finding {
                        kind: "once_a_week",
                        severity: "low",
                        title: format!("{} gets all its sets on one day", name_of(m)),
                        detail: format!(
                            "{} holds all {} of its weekly sets. The last sets of a long run for one muscle are worth less than the first; moving a few to another day keeps them harder. Weekly sets matter more than how they're split, so this is a small one.",
                            t.name,
                            sets_text(sets)
                        ),
                        muscle_id: Some(m.clone()),
                        exercise_id: None,
                        template_id: t.id,
                        fix_label: None,
                        fix: None,
                    });
                }
            }
        }

        // ---- Plateaus ----
        let window = (today - Duration::days(WINDOW_DAYS)).to_string();
        // On a gain, lifts that sat still through the cut or maintenance before it aren't stuck.
        let stall_from = match self.goal_on(today)? {
            Some(g) if g.goal == Goal::Gain => g.start_date.max(window.clone()),
            _ => window.clone(),
        };
        let bests = self.session_bests(Some(&stall_from), Some(&today.to_string()))?;
        let similar = |e: &Entry, same_pattern: bool| -> Result<Vec<ExerciseInfo>> {
            Ok(self
                .similar_exercises(e.ex.exercise_id, 12)?
                .into_iter()
                .filter(|s| s.kind == e.info.kind && !in_plan.contains(&s.id) && (s.pattern == e.info.pattern) == same_pattern)
                .collect())
        };
        let mut seen: HashSet<i64> = HashSet::new();
        let mut tracked = 0;
        let mut stalled: Vec<(&Entry, crate::trend::Stall)> = Vec::new();
        for e in &entries {
            if !seen.insert(e.ex.exercise_id) {
                continue;
            }
            let Some(sessions) = bests.get(&e.ex.exercise_id) else { continue };
            let Some(last) = sessions.last() else { continue };
            if (today - last.date).num_days() > RECENT_DAYS {
                continue;
            }
            tracked += 1;
            if let Some(st) = stall(sessions) {
                stalled.push((e, st));
            }
        }
        let easing = match goal {
            Some(Goal::Cut) => Some("On a cut, holding is the aim, so this may be fine."),
            Some(Goal::Maintain) => Some("At maintenance, holding is the aim, so this may be fine."),
            _ => None,
        };
        if stalled.len() >= SHARED_STALL_MIN && stalled.len() as f64 >= SHARED_STALL * tracked as f64 {
            let since = stalled.iter().map(|(_, st)| st.since).max().expect("not empty");
            let mut by_length: Vec<&(&Entry, crate::trend::Stall)> = stalled.iter().collect();
            by_length.sort_by_key(|(e, st)| (std::cmp::Reverse(st.sessions), e.index));
            let names: Vec<String> = by_length.iter().take(4).map(|(e, _)| e.ex.name.clone()).collect();
            findings.push(Finding {
                kind: "plateau",
                severity: if easing.is_some() { "low" } else { "medium" },
                title: format!("{} of {tracked} lifts have stopped moving", stalled.len()),
                detail: format!(
                    "None has a new best estimated 1RM since {} or earlier (longest: {}). {} When this many stall together it's rarely the exercises: check that you're eating and sleeping enough to progress, and that you add weight or a rep when a set gets easy.",
                    day(since),
                    list(&names),
                    easing.unwrap_or("Swapping exercises won't fix this.")
                ),
                muscle_id: None,
                exercise_id: None,
                template_id: None,
                fix_label: None,
                fix: None,
            });
        } else {
            for (e, st) in &stalled {
                let swaps = similar(e, true)?;
                let names: Vec<String> = swaps.iter().take(2).map(|s| s.name.clone()).collect();
                let (fix_label, fix) = swaps
                    .first()
                    .map(|s| {
                        (
                            format!("Swap {} for {} in {}", e.ex.name, s.name, e.template.name),
                            Fix::Swap { template_id: e.template_id, index: e.index, exercise_id: e.ex.exercise_id, to_exercise_id: s.id },
                        )
                    })
                    .unzip();
                findings.push(Finding {
                    kind: "plateau",
                    severity: if easing.is_some() { "low" } else { "medium" },
                    title: format!("{} has stopped moving", e.ex.name),
                    detail: format!(
                        "No new best estimated 1RM in {} sessions, since {}. {}{}",
                        st.sessions,
                        day(st.since),
                        match easing {
                            Some(text) => format!("{text} Otherwise you could "),
                            None => "You could ".to_string(),
                        },
                        if names.is_empty() {
                            "drop the weight about 10% and build back, or change the rep range.".to_string()
                        } else {
                            format!("drop the weight about 10% and build back, change the rep range, or swap it for something close such as {}.", list(&names))
                        }
                    ),
                    muscle_id: None,
                    exercise_id: Some(e.ex.exercise_id),
                    template_id: Some(e.template_id),
                    fix_label,
                    fix,
                });
            }
        }

        // ---- Near-duplicates within a template ----
        for id in times.keys() {
            let mut groups: BTreeMap<(String, Vec<String>), Vec<&Entry>> = BTreeMap::new();
            for e in entries.iter().filter(|e| e.template_id == *id && !e.info.primary.is_empty()) {
                let mut primary = e.info.primary.clone();
                primary.sort();
                groups.entry((e.info.pattern.clone().unwrap_or_default(), primary)).or_default().push(e);
            }
            for ((_, primary), group) in groups {
                let same_equipment = group.iter().all(|e| e.info.equipment == group[0].info.equipment);
                let dup = group.len() >= 3 || (group.len() == 2 && group[0].compound() && same_equipment);
                if !dup {
                    continue;
                }
                let names: Vec<String> = group.iter().map(|e| e.ex.name.clone()).collect();
                let muscles: Vec<String> = primary.iter().map(|m| name_of(m)).collect();
                let short: Vec<String> = under.iter().filter(|m| !primary.contains(m)).take(3).map(|m| name_of(m)).collect();
                findings.push(Finding {
                    kind: "duplicate",
                    severity: "low",
                    title: format!("{} repeats one movement", group[0].template.name),
                    detail: format!(
                        "{} train {} the same way. One of them could become a different angle{}.",
                        list(&names),
                        list(&muscles),
                        if short.is_empty() { String::new() } else { format!(", or work for {}, which {} short on sets", list(&short), if short.len() == 1 { "is" } else { "are" }) }
                    ),
                    muscle_id: None,
                    exercise_id: None,
                    template_id: Some(*id),
                    fix_label: None,
                    fix: None,
                });
            }
        }

        // ---- Late in the session and slipping ----
        let split = today - Duration::days(ORDER_DAYS);
        let long = self.session_bests(Some(&(split - Duration::days(ORDER_DAYS)).to_string()), Some(&today.to_string()))?;
        let change_of = |ex: i64| -> Option<f64> {
            let s = long.get(&ex)?;
            let (before, after): (Vec<SessionBest>, Vec<SessionBest>) = s.iter().cloned().partition(|x| x.date <= split);
            matched_change(&before, &after).map(|c| c.change)
        };
        for id in times.keys() {
            let day_entries: Vec<&Entry> = entries.iter().filter(|e| e.template_id == *id).collect();
            let n = day_entries.len();
            if n < 6 {
                continue;
            }
            let mut early: Vec<f64> = day_entries.iter().filter(|e| e.index < n / 3).filter_map(|e| change_of(e.ex.exercise_id)).collect();
            if early.len() < 2 {
                continue;
            }
            let early_med = median(&mut early).expect("not empty");
            let worst = day_entries
                .iter()
                .filter(|e| e.index >= n - n / 3 && !matches!(e.info.pattern.as_deref(), None | Some("core" | "other" | "cardio")))
                .filter_map(|e| change_of(e.ex.exercise_id).map(|c| (c, *e)))
                .filter(|(c, _)| *c <= -0.01 && *c <= early_med - 0.03)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let Some((c, e)) = worst else { continue };
            let movable = e.ex.superset.is_none();
            findings.push(Finding {
                kind: "late",
                severity: "low",
                title: format!("{} comes late and is slipping", e.ex.name),
                detail: format!(
                    "It's exercise {} of {n} in {}. Over the last 8 weeks it went {} while the first exercises of that session went {}. The lifts you do first gain the most strength, so if this one matters, do it earlier.",
                    e.index + 1,
                    e.template.name,
                    pct(c),
                    pct(early_med)
                ),
                muscle_id: None,
                exercise_id: Some(e.ex.exercise_id),
                template_id: Some(*id),
                fix_label: movable.then(|| format!("Move {} to the start of {}", e.ex.name, e.template.name)),
                fix: movable.then_some(Fix::MoveFirst { template_id: *id, index: e.index, exercise_id: e.ex.exercise_id }),
            });
        }

        // ---- Joint discomfort ----
        let mut ratings: HashMap<i64, Vec<[Option<i64>; 3]>> = HashMap::new();
        {
            let mut stmt = self.conn().prepare(
                "SELECT we.exercise_id, we.elbow, we.knee, we.shoulder
                 FROM workout_exercise we JOIN workout w ON w.id = we.workout_id
                 WHERE w.finished = 1 AND we.exercise_id IS NOT NULL AND substr(w.started_at_local, 1, 10) BETWEEN ?1 AND ?2
                   AND (we.elbow IS NOT NULL OR we.knee IS NOT NULL OR we.shoulder IS NOT NULL)
                 ORDER BY w.started_at_local",
            )?;
            for row in stmt.query_map([&window, &today.to_string()], |r| Ok((r.get::<_, i64>(0)?, [r.get(1)?, r.get(2)?, r.get(3)?])))? {
                let (ex, r) = row?;
                ratings.entry(ex).or_default().push(r);
            }
        }
        let mut seen: HashSet<i64> = HashSet::new();
        for e in &entries {
            if !seen.insert(e.ex.exercise_id) {
                continue;
            }
            let Some(rows) = ratings.get(&e.ex.exercise_id) else { continue };
            for (j, joint) in ["elbow", "knee", "shoulder"].iter().enumerate() {
                let v: Vec<f64> = rows.iter().filter_map(|r| r[j]).map(|x| x as f64).collect();
                if v.len() < 2 {
                    continue;
                }
                let avg = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
                let (earlier, last2) = v.split_at(v.len() - 2);
                let high = avg(last2) >= 4.0;
                let rising = !earlier.is_empty() && v[v.len() - 1] >= 3.0 && avg(last2) - avg(earlier) >= 1.0;
                if !high && !rising {
                    continue;
                }
                // Something for the same muscles that moves differently.
                let mut swaps = similar(e, false)?;
                swaps.extend(similar(e, true)?.into_iter().filter(|s| s.equipment != e.info.equipment));
                let (fix_label, fix) = swaps
                    .first()
                    .map(|s| {
                        (
                            format!("Swap {} for {} in {}", e.ex.name, s.name, e.template.name),
                            Fix::Swap { template_id: e.template_id, index: e.index, exercise_id: e.ex.exercise_id, to_exercise_id: s.id },
                        )
                    })
                    .unzip();
                findings.push(Finding {
                    kind: "joint",
                    severity: if high { "high" } else { "medium" },
                    title: format!("{} is bothering your {joint}", e.ex.name),
                    detail: format!(
                        "{} discomfort was {} over the last two sessions{}. Less load, a different grip or stance, or another exercise for the same muscles may settle it; pain that stays is worth getting looked at.",
                        joint[..1].to_uppercase() + &joint[1..],
                        last2.iter().map(|x| format!("{x:.0}")).collect::<Vec<_>>().join(" and "),
                        if rising && !earlier.is_empty() { format!(", up from an average of {:.1} before", avg(earlier)) } else { String::new() }
                    ),
                    muscle_id: None,
                    exercise_id: Some(e.ex.exercise_id),
                    template_id: Some(e.template_id),
                    fix_label,
                    fix,
                });
            }
        }

        let rank = |s: &str| match s {
            "high" => 0,
            "medium" => 1,
            _ => 2,
        };
        findings.sort_by_key(|f| rank(f.severity));
        Ok(RoutineReview { schedule: Some(schedule.name), findings })
    }

    /// Makes one suggested change to a template and returns the template as it was, so the change
    /// can be undone by saving that again.
    pub fn apply_routine_fix(&mut self, fix: &Fix) -> Result<Template> {
        let (template_id, at) = match *fix {
            Fix::SetSets { template_id, index, exercise_id, .. }
            | Fix::Swap { template_id, index, exercise_id, .. }
            | Fix::MoveFirst { template_id, index, exercise_id } => (template_id, Some((index, exercise_id))),
            Fix::Add { template_id, .. } => (template_id, None),
        };
        let before = self.template(template_id)?.ok_or_else(|| Error::Format("that template no longer exists".into()))?;
        if let Some((index, exercise_id)) = at {
            if before.exercises.get(index).map(|e| e.exercise_id) != Some(exercise_id) {
                return Err(Error::Format(format!("{} has changed since this was suggested; review it again", before.name)));
            }
        }
        let mut t = before.clone();
        let name_of = |db: &Db, id: i64| -> Result<String> {
            db.exercise_info(id)?.map(|e| e.name).ok_or_else(|| Error::Format("that exercise no longer exists".into()))
        };
        match *fix {
            Fix::SetSets { index, sets, .. } => {
                if !(1..=10).contains(&sets) {
                    return Err(Error::Format("sets must be between 1 and 10".into()));
                }
                t.exercises[index].target_sets = Some(sets);
            }
            Fix::Swap { index, to_exercise_id, .. } => {
                let e = &mut t.exercises[index];
                e.name = name_of(self, to_exercise_id)?;
                e.exercise_id = to_exercise_id;
                // The setup note belonged to the old exercise.
                e.setup_note = None;
            }
            Fix::Add { exercise_id, sets, .. } => {
                t.exercises.push(TemplateExercise {
                    exercise_id,
                    name: name_of(self, exercise_id)?,
                    superset: None,
                    target_sets: Some(sets),
                    rep_min: None,
                    rep_max: None,
                    rest_s: None,
                    setup_note: None,
                });
            }
            Fix::MoveFirst { index, .. } => {
                let e = t.exercises.remove(index);
                t.exercises.insert(0, e);
            }
        }
        self.save_template(&t)?;
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::Schedule;
    use rusqlite::params;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    const TODAY: &str = "2026-11-09"; // a Monday

    fn ex(db: &Db, slug: &str, sets: i64) -> TemplateExercise {
        let id = db.exercise_id(slug).unwrap().unwrap_or_else(|| panic!("no exercise {slug}"));
        TemplateExercise {
            exercise_id: id,
            name: db.exercise_info(id).unwrap().unwrap().name,
            superset: None,
            target_sets: Some(sets),
            rep_min: Some(8),
            rep_max: Some(12),
            rest_s: None,
            setup_note: None,
        }
    }

    /// A two-day weekly plan from (name, [(slug, sets)]).
    fn plan(db: &mut Db, days: &[(&str, &[(&str, i64)])]) -> Vec<i64> {
        let mut ids = Vec::new();
        for (name, exercises) in days {
            let t = Template {
                id: None,
                name: name.to_string(),
                notes: None,
                color: None,
                exercises: exercises.iter().map(|(s, n)| ex(db, s, *n)).collect(),
                last_done: None,
            };
            ids.push(db.save_template(&t).unwrap());
        }
        let mut slots = vec![None; 7];
        for (i, id) in ids.iter().enumerate() {
            slots[i * 3] = Some(*id);
        }
        db.save_schedule(&Schedule { id: None, name: "Test".into(), kind: "weekly".into(), start_date: "2026-06-01".into(), cycle_length: 7, days: slots }).unwrap();
        ids
    }

    /// Logs one finished session of an exercise: `sets` of `reps` at `kg`.
    fn log(db: &Db, date: &str, slug: &str, kg: f64, reps: i64, sets: usize, joints: [Option<i64>; 3]) {
        let ex = db.exercise_id(slug).unwrap().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO workout (name, started_at_local, finished) VALUES ('W', ?1, 1)", [format!("{date} 08:00:00")]).unwrap();
        let w = c.last_insert_rowid();
        c.execute(
            "INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id, elbow, knee, shoulder) VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6)",
            params![w, slug, ex, joints[0], joints[1], joints[2]],
        )
        .unwrap();
        let we = c.last_insert_rowid();
        for i in 0..sets {
            c.execute(
                "INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps, completed) VALUES (?1, ?2, 'normal', ?3, ?4, 1)",
                params![we, i as i64, kg, reps],
            )
            .unwrap();
        }
    }

    fn kinds(r: &RoutineReview) -> Vec<&str> {
        r.findings.iter().map(|f| f.kind).collect()
    }

    #[test]
    fn no_schedule_no_findings() {
        let db = Db::open_in_memory().unwrap();
        let r = db.routine_review(d(TODAY)).unwrap();
        assert!(r.schedule.is_none() && r.findings.is_empty());
    }

    #[test]
    fn under_target_adds_sets_and_can_be_undone() {
        let mut db = Db::open_in_memory().unwrap();
        // Biceps: 2 × 3 curls a week = 6, under the default minimum of 10.
        let ids = plan(&mut db, &[("A", &[("bicep-curl-dumbbell", 3)]), ("B", &[("bicep-curl-dumbbell", 3)])]);
        let r = db.routine_review(d(TODAY)).unwrap();
        let f = r.findings.iter().find(|f| f.kind == "under_target" && f.muscle_id.as_deref() == Some("biceps")).expect("biceps flagged");
        assert_eq!(f.severity, "medium");
        // 4 sets short, one set adds 1 a week, capped at 5 sets for the exercise.
        let curl = db.exercise_id("bicep-curl-dumbbell").unwrap().unwrap();
        assert_eq!(f.fix, Some(Fix::SetSets { template_id: ids[0], index: 0, exercise_id: curl, sets: 5 }));

        let before = db.apply_routine_fix(f.fix.as_ref().unwrap()).unwrap();
        assert_eq!(db.template(ids[0]).unwrap().unwrap().exercises[0].target_sets, Some(5));
        db.save_template(&before).unwrap();
        assert_eq!(db.template(ids[0]).unwrap().unwrap().exercises[0].target_sets, Some(3));
    }

    #[test]
    fn untrained_muscle_gets_an_exercise() {
        let mut db = Db::open_in_memory().unwrap();
        let ids = plan(&mut db, &[("A", &[("bicep-curl-dumbbell", 5)]), ("B", &[("bicep-curl-dumbbell", 5)])]);
        let r = db.routine_review(d(TODAY)).unwrap();
        let f = r.findings.iter().find(|f| f.muscle_id.as_deref() == Some("quads")).expect("quads flagged");
        assert_eq!(f.severity, "high");
        let Some(Fix::Add { template_id, exercise_id, sets }) = f.fix.clone() else { panic!("expected an added exercise, got {:?}", f.fix) };
        assert!(db.exercise_info(exercise_id).unwrap().unwrap().primary.contains(&"quads".to_string()));
        assert!((MIN_SETS..=4).contains(&sets));
        db.apply_routine_fix(f.fix.as_ref().unwrap()).unwrap();
        assert_eq!(db.template(template_id).unwrap().unwrap().exercises.len(), 2);
        assert!(ids.contains(&template_id));
    }

    #[test]
    fn over_target_drops_sets() {
        let mut db = Db::open_in_memory().unwrap();
        // 2 × (5 + 5 + 5) = 30 biceps sets a week against a maximum of 20.
        plan(
            &mut db,
            &[
                ("A", &[("bicep-curl-dumbbell", 5), ("hammer-curl-dumbbell", 5), ("preacher-curl-barbell", 5)]),
                ("B", &[("bicep-curl-dumbbell", 5), ("hammer-curl-dumbbell", 5), ("preacher-curl-barbell", 5)]),
            ],
        );
        let r = db.routine_review(d(TODAY)).unwrap();
        let f = r.findings.iter().find(|f| f.kind == "over_target" && f.muscle_id.as_deref() == Some("biceps")).expect("biceps flagged");
        let Some(Fix::SetSets { sets, .. }) = f.fix else { panic!("expected fewer sets") };
        assert_eq!(sets, MIN_SETS);
    }

    #[test]
    fn stale_fix_is_refused() {
        let mut db = Db::open_in_memory().unwrap();
        let ids = plan(&mut db, &[("A", &[("bicep-curl-dumbbell", 3)]), ("B", &[("bicep-curl-dumbbell", 3)])]);
        let other = db.exercise_id("hammer-curl-dumbbell").unwrap().unwrap();
        let fix = Fix::SetSets { template_id: ids[0], index: 0, exercise_id: other, sets: 4 };
        assert!(db.apply_routine_fix(&fix).is_err());
        assert_eq!(db.template(ids[0]).unwrap().unwrap().exercises[0].target_sets, Some(3));
    }

    #[test]
    fn plateau_suggests_a_swap() {
        let mut db = Db::open_in_memory().unwrap();
        let ids = plan(&mut db, &[("A", &[("bench-press-barbell", 3)]), ("B", &[("bicep-curl-dumbbell", 3)])]);
        // Bench: up once, then six flat weeks. Curls: climbing every week.
        for (i, kg) in [80.0, 82.5, 82.5, 82.5, 82.5, 82.5, 82.5, 82.5].iter().enumerate() {
            let date = (d("2026-09-14") + Duration::days(7 * i as i64)).to_string();
            log(&db, &date, "bench-press-barbell", *kg, 8, 3, [None; 3]);
            log(&db, &date, "bicep-curl-dumbbell", 10.0 + i as f64, 10, 3, [None; 3]);
        }
        let r = db.routine_review(d(TODAY)).unwrap();
        let stalls: Vec<&Finding> = r.findings.iter().filter(|f| f.kind == "plateau").collect();
        assert_eq!(stalls.len(), 1, "{:?}", kinds(&r));
        let bench = db.exercise_id("bench-press-barbell").unwrap().unwrap();
        assert_eq!(stalls[0].exercise_id, Some(bench));
        let Some(Fix::Swap { to_exercise_id, .. }) = stalls[0].fix else { panic!("expected a swap") };
        let to = db.exercise_info(to_exercise_id).unwrap().unwrap();
        assert_eq!(to.pattern.as_deref(), Some("horizontal_push"));
        db.apply_routine_fix(stalls[0].fix.as_ref().unwrap()).unwrap();
        assert_eq!(db.template(ids[0]).unwrap().unwrap().exercises[0].exercise_id, to_exercise_id);
    }

    #[test]
    fn many_plateaus_are_one_finding_and_a_new_gain_starts_fresh() {
        let mut db = Db::open_in_memory().unwrap();
        let slugs = ["bench-press-barbell", "squat-barbell", "bicep-curl-dumbbell", "hammer-curl-dumbbell", "preacher-curl-barbell", "skullcrusher-barbell"];
        let day: Vec<(&str, i64)> = slugs.iter().map(|s| (*s, 3)).collect();
        plan(&mut db, &[("A", &day[..3]), ("B", &day[3..])]);
        for i in 0..8 {
            let date = (d("2026-09-14") + Duration::days(7 * i)).to_string();
            for s in slugs {
                log(&db, &date, s, 40.0, 8, 3, [None; 3]);
            }
        }
        // At maintenance: one low finding for the lot, with nothing to apply.
        db.set_goal(Goal::Maintain, d("2026-08-01"), None).unwrap();
        let r = db.routine_review(d(TODAY)).unwrap();
        let stalls: Vec<&Finding> = r.findings.iter().filter(|f| f.kind == "plateau").collect();
        assert_eq!(stalls.len(), 1);
        assert_eq!(stalls[0].title, "6 of 6 lifts have stopped moving");
        assert_eq!((stalls[0].severity, &stalls[0].fix), ("low", &None));
        // A gain that began last week: the flat stretch before it doesn't count.
        db.set_goal(Goal::Gain, d("2026-11-02"), None).unwrap();
        let r = db.routine_review(d(TODAY)).unwrap();
        assert!(!kinds(&r).contains(&"plateau"), "{:?}", kinds(&r));
    }

    #[test]
    fn joint_ratings_that_climb_are_flagged() {
        let mut db = Db::open_in_memory().unwrap();
        plan(&mut db, &[("A", &[("skullcrusher-barbell", 3)]), ("B", &[("bicep-curl-dumbbell", 3)])]);
        for (i, elbow) in [1, 2, 3, 4].iter().enumerate() {
            let date = (d("2026-10-12") + Duration::days(7 * i as i64)).to_string();
            log(&db, &date, "skullcrusher-barbell", 30.0 + i as f64, 10, 3, [Some(*elbow), None, None]);
        }
        let r = db.routine_review(d(TODAY)).unwrap();
        let f = r.findings.iter().find(|f| f.kind == "joint").expect("elbow flagged");
        assert_eq!(f.severity, "medium");
        assert!(f.title.contains("elbow"));
        assert!(f.fix.is_some());
    }

    #[test]
    fn near_duplicates_in_one_template() {
        let mut db = Db::open_in_memory().unwrap();
        plan(
            &mut db,
            &[("A", &[("bicep-curl-dumbbell", 3), ("bicep-curl-barbell", 3), ("bicep-curl-cable", 3)]), ("B", &[("squat-barbell", 3)])],
        );
        let r = db.routine_review(d(TODAY)).unwrap();
        assert!(kinds(&r).contains(&"duplicate"), "{:?}", kinds(&r));
        // A fix that is serious sorts before the small stuff.
        let ranks: Vec<&str> = r.findings.iter().map(|f| f.severity).collect();
        let pos = |s: &str| ranks.iter().position(|x| *x == s);
        if let (Some(h), Some(l)) = (pos("high"), pos("low")) {
            assert!(h < l);
        }
    }
}
