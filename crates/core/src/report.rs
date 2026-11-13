//! A Markdown progress report the user can share or copy. Sections with no data are left out.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use chrono::{Duration, NaiveDate};

use crate::Result;
use crate::body::BODY_PARTS;
use crate::db::Db;
use crate::stats::{Formula, WorkSet, e1rm, week_start};
use crate::units::WeightUnit;

const TOP_EXERCISES: usize = 10;
/// InBody fields worth a line, with their labels. Masses are in kg.
const SCAN_FIELDS: [(&str, &str, bool); 5] = [
    ("weight_kg", "Weight", true),
    ("smm_kg", "Skeletal muscle", true),
    ("body_fat_mass_kg", "Body fat mass", true),
    ("pbf", "Body fat %", false),
    ("lean_body_mass_kg", "Lean body mass", true),
];

/// Whole numbers stay whole: `100`, `82.5`.
fn n1(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r:.1}") }
}

fn signed(v: f64) -> String {
    let s = n1(v.abs());
    if s == "0" {
        s
    } else if v > 0.0 {
        format!("+{s}")
    } else {
        format!("-{s}")
    }
}

fn avg(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

struct Period {
    from: NaiveDate,
    to: NaiveDate,
    weeks: i64,
    unit: WeightUnit,
}

impl Period {
    fn has(&self, at: &str) -> bool {
        at.get(..10).is_some_and(|d| d >= self.from.to_string().as_str() && d <= self.to.to_string().as_str())
    }

    fn unit_name(&self) -> &'static str {
        if self.unit == WeightUnit::Kg { "kg" } else { "lb" }
    }

    fn weight(&self, kg: f64) -> String {
        format!("{} {}", n1(self.unit.from_kg(kg)), self.unit_name())
    }
}

impl Db {
    /// Training over the `weeks` ending on `today`. Without `include_body`, nothing about
    /// bodyweight, measurements, nutrition or recovery is included.
    pub fn progress_report(&self, today: NaiveDate, weeks: i64, unit: WeightUnit, include_body: bool) -> Result<String> {
        let weeks = weeks.max(1);
        let p = Period { from: today - Duration::days(weeks * 7 - 1), to: today, weeks, unit };
        let formula = self.formula()?;
        let mut out = format!(
            "# FitLedger training report\n\n\
             Period: {} to {} ({weeks} weeks). Weights are in {}. Estimated 1RM uses the {} formula.\n\n\
             Please review this training data. Tell me what's working, what's stalling and what to change over the next few weeks. \
             Say so if the numbers aren't enough to judge something.\n",
            p.from,
            p.to,
            p.unit_name(),
            if formula == Formula::Epley { "Epley" } else { "Brzycki" },
        );
        let sections = [
            self.plan_section(&p, include_body)?,
            self.consistency_section(&p)?,
            self.strength_section(&p, formula)?,
            self.volume_section(&p)?,
            self.discomfort_section(&p)?,
        ];
        let body = if include_body { [self.body_section(&p)?, self.nutrition_section(&p)?, self.recovery_section(&p)?] } else { [None, None, None] };
        let mut any = false;
        for s in sections.into_iter().chain(body).flatten() {
            any = true;
            let _ = write!(out, "\n{s}");
        }
        if !any {
            out.push_str("\nNo training data has been logged in this period.\n");
        }
        Ok(out)
    }

    fn plan_section(&self, p: &Period, include_body: bool) -> Result<Option<String>> {
        let mut lines = Vec::new();
        if let Some(g) = self.goal_on(p.to)? {
            let rate = g.weekly_rate_kg.filter(|_| include_body).map(|r| format!(", target {} {}/week", signed(p.unit.from_kg(r)), p.unit_name()));
            lines.push(format!("- Goal: {}{}", g.goal.label(), rate.unwrap_or_default()));
        }
        let today = p.to.to_string();
        for ph in self.list_phases()? {
            if ph.lane == "program" && ph.start_date <= today && ph.end_date.as_ref().is_none_or(|e| *e >= today) {
                lines.push(format!("- Current program phase: {} (since {})", ph.name, ph.start_date));
            }
        }
        if let Some(s) = self.active_schedule()? {
            let mut names = Vec::new();
            for (i, day) in s.days.iter().enumerate() {
                let Some(id) = day else { continue };
                let name = self.template(*id)?.map(|t| t.name).unwrap_or_default();
                let when = if s.kind == "weekly" { ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][i % 7].to_string() } else { format!("Day {}", i + 1) };
                names.push(format!("{when}: {name}"));
            }
            if !names.is_empty() {
                lines.push(format!("- Schedule \"{}\": {}", s.name, names.join(", ")));
            }
        }
        Ok((!lines.is_empty()).then(|| format!("## Goal and plan\n\n{}\n", lines.join("\n"))))
    }

    fn consistency_section(&self, p: &Period) -> Result<Option<String>> {
        let workouts: Vec<_> = self.workout_series()?.into_iter().filter(|w| p.has(&w.at)).collect();
        if workouts.is_empty() {
            return Ok(None);
        }
        let mut s = format!("## Consistency\n\n- Workouts: {} ({} per week)\n", workouts.len(), n1(workouts.len() as f64 / p.weeks as f64));
        let first_week = week_start(p.from).to_string();
        let per_week: Vec<String> = self.workouts_per_period(false, p.to)?.into_iter().filter(|c| c.start >= first_week).map(|c| c.workouts.to_string()).collect();
        let _ = writeln!(s, "- By week, oldest first: {}", per_week.join(", "));
        if let Some(sched) = self.active_schedule()? {
            // Only days since this schedule began can count as planned.
            let start = NaiveDate::parse_from_str(&sched.start_date, "%Y-%m-%d").map_or(p.from, |d| d.max(p.from));
            let planned = start.iter_days().take_while(|d| *d <= p.to).filter(|d| sched.planned(*d).is_some()).count();
            let done = workouts.iter().filter(|w| w.at.get(..10).is_some_and(|d| d >= start.to_string().as_str())).count();
            if planned > 0 {
                let _ = writeln!(s, "- Since {start}, on the current schedule: {done} done of {planned} planned");
            }
        }
        if let Some(mins) = avg(workouts.iter().filter_map(|w| w.duration_s).map(|d| d as f64 / 60.0)) {
            let _ = writeln!(s, "- Average duration: {} min", mins.round());
        }
        let streak = self.streak(p.to)?;
        let _ = writeln!(s, "- Streak: {} weeks (longest {})", streak.current_weeks, streak.longest_weeks);
        Ok(Some(s))
    }

    fn strength_section(&self, p: &Period, formula: Formula) -> Result<Option<String>> {
        let sets = self.work_sets(None, Some(&p.from.to_string()), Some(&p.to.to_string()))?;
        let mut by_exercise: HashMap<i64, Vec<&WorkSet>> = HashMap::new();
        for set in &sets {
            if let Some(id) = set.exercise_id {
                by_exercise.entry(id).or_default().push(set);
            }
        }
        let mut ranked: Vec<(i64, Vec<&WorkSet>)> = by_exercise.into_iter().collect();
        ranked.sort_by_key(|(id, sets)| (std::cmp::Reverse(sets.len()), *id));

        let mid = p.from + Duration::days(p.weeks * 7 / 2);
        let mut prs: HashMap<i64, usize> = HashMap::new();
        let mut workouts: Vec<i64> = sets.iter().map(|s| s.workout_id).collect();
        workouts.sort();
        workouts.dedup();
        for id in workouts {
            for pr in self.prs_for_workout(id)? {
                *prs.entry(pr.exercise_id).or_default() += 1;
            }
        }

        let mut rows = Vec::new();
        for (id, sets) in ranked {
            let Some(info) = self.exercise_info(id)? else { continue };
            if info.kind != "weight_reps" {
                continue;
            }
            let est = |s: &&WorkSet| Some((e1rm(s.weight_kg?, s.reps?, formula)?, s.weight_kg?, s.reps?));
            let best = |early: bool| sets.iter().filter(|s| (s.date() < mid) == early).filter_map(est).map(|e| e.0).fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b))));
            let Some(top) = sets.iter().filter_map(est).max_by(|a, b| a.0.total_cmp(&b.0)) else { continue };
            let (first, second) = (best(true), best(false));
            let change = match (first, second) {
                (Some(a), Some(b)) if a > 0.0 => format!("{}%", signed((b - a) / a * 100.0)),
                _ => "n/a".to_string(),
            };
            let shown = |v: Option<f64>| v.map_or("n/a".to_string(), |v| n1(p.unit.from_kg(v)));
            let name = if info.load_mode == "per_hand" { format!("{} (per hand)", info.name) } else { info.name };
            rows.push(format!(
                "| {name} | {} | {} | {} | {change} | {} × {} | {} |",
                sets.len(),
                shown(first),
                shown(second),
                p.weight(top.1),
                top.2,
                prs.get(&id).copied().unwrap_or(0),
            ));
            if rows.len() == TOP_EXERCISES {
                break;
            }
        }
        if rows.is_empty() {
            return Ok(None);
        }
        let u = p.unit_name();
        Ok(Some(format!(
            "## Strength\n\nMost-trained lifts. Each e1RM is the best in that half of the period.\n\n\
             | Exercise | Sets | e1RM first half ({u}) | e1RM second half ({u}) | Change | Best set | PRs |\n\
             |---|---|---|---|---|---|---|\n{}\n",
            rows.join("\n")
        )))
    }

    fn volume_section(&self, p: &Period) -> Result<Option<String>> {
        let done: HashMap<String, f64> =
            self.muscle_sets(Some(&p.from.to_string()), Some(&p.to.to_string()))?.into_iter().map(|m| (m.muscle_id, m.sets)).collect();
        if done.is_empty() {
            return Ok(None);
        }
        let mut stmt = self.conn().prepare("SELECT id, name FROM muscle")?;
        let names: HashMap<String, String> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        let last: HashMap<String, Option<i64>> = self.last_trained(p.to)?.into_iter().map(|l| (l.muscle_id, l.days_ago)).collect();
        let mut s = String::from("## Weekly volume\n\n| Muscle | Sets per week | Target | Last trained |\n|---|---|---|---|\n");
        for t in self.muscle_targets(p.to)? {
            let sets = done.get(&t.muscle_id).copied().unwrap_or(0.0) / p.weeks as f64;
            let ago = last.get(&t.muscle_id).copied().flatten().map_or("never".to_string(), |d| format!("{d} days ago"));
            let name = names.get(&t.muscle_id).unwrap_or(&t.muscle_id);
            let _ = writeln!(s, "| {name} | {} | {}–{} | {ago} |", n1(sets), n1(t.min_sets), n1(t.max_sets));
        }
        Ok(Some(s))
    }

    fn discomfort_section(&self, p: &Period) -> Result<Option<String>> {
        let points: Vec<_> = self.discomfort_points()?.into_iter().filter(|d| p.has(&d.at)).collect();
        let mut lines = Vec::new();
        let joints: [(&str, fn(&crate::stats::DiscomfortPoint) -> Option<i64>); 3] =
            [("Elbow", |d| d.elbow), ("Knee", |d| d.knee), ("Shoulder", |d| d.shoulder)];
        for (joint, rating) in joints {
            let rated: Vec<(i64, &str)> = points.iter().filter_map(|d| Some((rating(d)?, d.exercise_name.as_str()))).collect();
            let Some((worst, exercise)) = rated.iter().max_by_key(|r| r.0) else { continue };
            let mean = avg(rated.iter().map(|r| r.0 as f64)).unwrap_or(0.0);
            lines.push(format!("- {joint}: rated {} times, average {}, worst {worst} ({exercise})", rated.len(), n1(mean)));
        }
        Ok((!lines.is_empty()).then(|| format!("## Joint discomfort\n\nRated 1 to 5 during workouts.\n\n{}\n", lines.join("\n"))))
    }

    fn body_section(&self, p: &Period) -> Result<Option<String>> {
        let mut lines = Vec::new();
        let trend: Vec<_> = self.energy_balance(14)?.into_iter().filter(|e| p.has(&e.date)).collect();
        if let (Some(a), Some(b)) = (trend.first(), trend.last())
            && a.date != b.date
        {
            let days = date(&b.date).zip(date(&a.date)).map_or(1, |(b, a)| (b - a).num_days().max(1));
            let per_week = p.unit.from_kg(b.trend_kg - a.trend_kg) / days as f64 * 7.0;
            lines.push(format!("- Weight trend: {} to {} ({} {}/week)", p.weight(a.trend_kg), p.weight(b.trend_kg), signed(per_week), p.unit_name()));
        } else if let Some(w) = self.weigh_ins(true)?.iter().rev().find(|w| p.has(&w.at)) {
            lines.push(format!("- Latest weight: {} on {}", p.weight(w.weight_kg), &w.at[..10]));
        }

        let mut parts: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
        for m in self.measurements(None)?.iter().filter(|m| p.has(&m.at)) {
            let Some(i) = BODY_PARTS.iter().position(|b| b.0 == m.part) else { continue };
            parts.entry(i).or_insert((m.value, m.value)).1 = m.value;
        }
        for (i, (first, last)) in parts {
            let (_, label, kind) = BODY_PARTS[i];
            let (scale, unit) = match kind {
                "pct" => (1.0, "%"),
                _ if p.unit == WeightUnit::Lb => (1.0 / 2.54, " in"),
                _ => (1.0, " cm"),
            };
            lines.push(format!("- {label}: {}{unit} to {}{unit}", n1(first * scale), n1(last * scale)));
        }

        let scans = self.inbody_scans()?;
        if let Some(latest) = scans.last() {
            let previous = scans.len().checked_sub(2).map(|i| &scans[i]);
            let mut values = Vec::new();
            for (field, label, mass) in SCAN_FIELDS {
                let Some(v) = latest.values.get(field) else { continue };
                let shown = |v: f64| if mass { p.weight(v) } else { format!("{}%", n1(v)) };
                let was = previous.and_then(|s| s.values.get(field)).map(|v| format!(" (was {})", shown(*v))).unwrap_or_default();
                values.push(format!("{label} {}{was}", shown(*v)));
            }
            if !values.is_empty() {
                let since = previous.map(|s| format!(", compared with {}", &s.measured_at_local[..10])).unwrap_or_default();
                lines.push(format!("- InBody scan {}{since}: {}", &latest.measured_at_local[..10], values.join(", ")));
            }
        }
        Ok((!lines.is_empty()).then(|| format!("## Body\n\n{}\n", lines.join("\n"))))
    }

    fn nutrition_section(&self, p: &Period) -> Result<Option<String>> {
        let days: Vec<_> = self.diet_days()?.into_iter().filter(|d| p.has(&d.date)).collect();
        let mut lines = Vec::new();
        if let Some(kcal) = avg(days.iter().filter_map(|d| d.calories)) {
            lines.push(format!("- Average intake: {} kcal/day ({} days logged)", kcal.round(), days.iter().filter(|d| d.calories.is_some()).count()));
        }
        if let Some(protein) = avg(days.iter().filter_map(|d| d.protein_g)) {
            lines.push(format!("- Average protein: {} g/day", protein.round()));
        }
        if let Some(tdee) = self.energy_balance(14)?.iter().rev().filter(|e| p.has(&e.date)).find_map(|e| e.tdee_kcal) {
            lines.push(format!("- Estimated expenditure from weight trend and intake: {} kcal/day", tdee.round()));
        }
        Ok((!lines.is_empty()).then(|| format!("## Nutrition\n\n{}\n", lines.join("\n"))))
    }

    fn recovery_section(&self, p: &Period) -> Result<Option<String>> {
        let days: Vec<_> = self.day_health()?.into_iter().filter(|d| p.has(&d.date)).collect();
        let mut lines = Vec::new();
        if let Some(h) = avg(days.iter().filter_map(|d| d.sleep_s).map(|s| s as f64 / 3600.0)) {
            lines.push(format!("- Average sleep: {} hours", n1(h)));
        }
        if let Some(hr) = avg(days.iter().filter_map(|d| d.resting_hr).map(|v| v as f64)) {
            lines.push(format!("- Average resting heart rate: {} bpm", hr.round()));
        }
        if let Some(hrv) = avg(days.iter().filter_map(|d| d.hrv_ms)) {
            lines.push(format!("- Average HRV: {} ms", hrv.round()));
        }
        Ok((!lines.is_empty()).then(|| format!("## Recovery\n\n{}\n", lines.join("\n"))))
    }
}

fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::DietDay;
    use chrono::NaiveDateTime;
    use rusqlite::params;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    fn at(days_ago: i64) -> NaiveDateTime {
        (today() - Duration::days(days_ago)).and_hms_opt(9, 0, 0).unwrap()
    }

    fn log_bench(db: &Db, days_ago: i64, kg: f64, reps: i64) {
        let exercise = db.exercise_id("bench-press-barbell").unwrap().unwrap();
        let c = db.conn();
        c.execute(
            "INSERT INTO workout (name, started_at_local, duration_s) VALUES ('Push', ?1, 3600)",
            [at(days_ago).format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .unwrap();
        let w = c.last_insert_rowid();
        c.execute("INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id, elbow) VALUES (?1, 0, 'Bench', ?2, 3)", params![w, exercise]).unwrap();
        let we = c.last_insert_rowid();
        c.execute("INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps) VALUES (?1, 0, 'normal', ?2, ?3)", params![we, kg, reps]).unwrap();
    }

    fn logged() -> Db {
        let mut db = Db::open_in_memory().unwrap();
        log_bench(&db, 50, 100.0, 5);
        log_bench(&db, 10, 110.0, 5);
        for days_ago in [40, 20, 2] {
            db.add_weigh_in(at(days_ago), 80.0 - days_ago as f64 / 100.0).unwrap();
        }
        db.add_measurement(at(30), "waist", 90.0).unwrap();
        db.add_measurement(at(3), "waist", 88.0).unwrap();
        db.save_diet_day(&DietDay { date: (today() - Duration::days(1)).to_string(), calories: Some(2500.0), protein_g: Some(180.0), notes: None }).unwrap();
        db
    }

    #[test]
    fn empty_database_gives_a_short_valid_report() {
        let r = Db::open_in_memory().unwrap().progress_report(today(), 8, WeightUnit::Kg, true).unwrap();
        assert!(r.contains("No training data"), "{r}");
        assert_eq!(r.matches("## ").count(), 0, "{r}");
    }

    #[test]
    fn strength_compares_the_two_halves() {
        let r = logged().progress_report(today(), 8, WeightUnit::Kg, true).unwrap();
        // Epley: 100 × (1 + 5/30) = 116.7, 110 × (1 + 5/30) = 128.3.
        assert!(r.contains("| 2 | 116.7 | 128.3 | +10% | 110 kg × 5 |"), "{r}");
        assert!(r.contains("Workouts: 2 (0.3 per week)"), "{r}");
        assert!(r.contains("Elbow: rated 2 times, average 3, worst 3"), "{r}");
    }

    #[test]
    fn numbers_follow_the_weight_unit() {
        let r = logged().progress_report(today(), 8, WeightUnit::Lb, true).unwrap();
        assert!(r.contains("Weights are in lb"), "{r}");
        assert!(r.contains("242.5 lb × 5"), "{r}");
        assert!(r.contains("Waist: 35.4 in to 34.6 in"), "{r}");
    }

    #[test]
    fn body_data_can_be_left_out() {
        let with = logged().progress_report(today(), 8, WeightUnit::Kg, true).unwrap();
        assert!(with.contains("## Body") && with.contains("## Nutrition"), "{with}");
        let r = logged().progress_report(today(), 8, WeightUnit::Kg, false).unwrap();
        for word in ["## Body", "## Nutrition", "## Recovery", "Waist", "weight trend", "kcal"] {
            assert!(!r.to_lowercase().contains(&word.to_lowercase()), "{word} in {r}");
        }
        assert!(r.contains("## Strength"), "{r}");
    }

    #[test]
    fn old_data_stays_out_of_the_period() {
        let r = logged().progress_report(today(), 4, WeightUnit::Kg, true).unwrap();
        assert!(!r.contains("116.7"), "{r}");
        assert!(r.contains("| 1 | n/a | 128.3 | n/a |"), "{r}");
    }
}
