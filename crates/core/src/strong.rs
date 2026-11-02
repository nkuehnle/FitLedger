//! Parser for Strong app CSV exports (workouts and bodyweight).
//!
//! Handles the current export format (`Date,Timezone,Workout,...,Weight (lb),...`) and the older
//! one (`Date;Workout Name;Duration;Exercise Name;Set Order;Weight;...`, sometimes with
//! `Weight Unit`/`Distance Unit` columns). Units are taken from the header, a per-row unit column,
//! or the caller's fallback, in that order.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use regex::Regex;
use serde::Serialize;

use crate::units::{DistanceUnit, WeightUnit};
use crate::{Error, Result, SetType};

/// Units to assume when the file doesn't say.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportOptions {
    pub weight_unit: Option<WeightUnit>,
    pub distance_unit: Option<DistanceUnit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedSet {
    pub set_type: SetType,
    pub weight_kg: Option<f64>,
    pub reps: Option<i64>,
    pub rpe: Option<f64>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub rest_s: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedExercise {
    /// Exercise name exactly as written in the file.
    pub name: String,
    /// Superset index within the workout (0 = first group seen), `None` for standalone.
    pub superset: Option<u32>,
    pub notes: Option<String>,
    pub sets: Vec<ImportedSet>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedWorkout {
    /// Stable identity used to make re-imports idempotent.
    pub source_key: String,
    pub started_at_local: NaiveDateTime,
    pub tz_name: Option<String>,
    pub utc_offset_min: Option<i32>,
    pub name: String,
    pub duration_s: Option<i64>,
    pub notes: Option<String>,
    pub exercises: Vec<ImportedExercise>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrongImport {
    pub workouts: Vec<ImportedWorkout>,
    /// Weight unit used by the file (or the fallback the caller supplied).
    pub weight_unit: Option<WeightUnit>,
    pub row_count: usize,
    pub warnings: Vec<String>,
}

impl StrongImport {
    pub fn set_count(&self) -> usize {
        self.workouts.iter().flat_map(|w| &w.exercises).map(|e| e.sets.len()).sum()
    }
}

/// Per-exercise-name summary, used by the import preview and to pick default rest times.
#[derive(Debug, Clone, Serialize)]
pub struct ExerciseNameStats {
    pub name: String,
    pub sessions: usize,
    pub sets: usize,
    pub first: NaiveDateTime,
    pub last: NaiveDateTime,
    /// Most common rest timer value, if any were recorded.
    pub rest_mode_s: Option<i64>,
}

pub fn exercise_name_stats(import: &StrongImport) -> Vec<ExerciseNameStats> {
    struct Acc {
        sessions: usize,
        sets: usize,
        first: NaiveDateTime,
        last: NaiveDateTime,
        rests: HashMap<i64, usize>,
    }
    let mut by_name: HashMap<&str, Acc> = HashMap::new();
    for w in &import.workouts {
        for e in &w.exercises {
            let t = w.started_at_local;
            let acc = by_name.entry(&e.name).or_insert(Acc {
                sessions: 0,
                sets: 0,
                first: t,
                last: t,
                rests: HashMap::new(),
            });
            acc.sessions += 1;
            acc.sets += e.sets.len();
            acc.first = acc.first.min(t);
            acc.last = acc.last.max(t);
            for r in e.sets.iter().filter_map(|s| s.rest_s) {
                *acc.rests.entry(r).or_default() += 1;
            }
        }
    }
    let mut out: Vec<_> = by_name
        .into_iter()
        .map(|(name, a)| ExerciseNameStats {
            name: name.to_string(),
            sessions: a.sessions,
            sets: a.sets,
            first: a.first,
            last: a.last,
            // Ties go to the larger rest value so the result is deterministic.
            rest_mode_s: a.rests.into_iter().max_by_key(|&(v, n)| (n, v)).map(|(v, _)| v),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

// ---------------------------------------------------------------------------------------------
// Header handling

/// Splits `"Weight (lb)"` into `("weight", Some("lb"))`.
fn split_header(h: &str) -> (String, Option<String>) {
    let h = h.trim().trim_start_matches('\u{feff}');
    match (h.rfind('('), h.ends_with(')')) {
        (Some(i), true) => (
            h[..i].trim().to_ascii_lowercase(),
            Some(h[i + 1..h.len() - 1].trim().to_string()),
        ),
        _ => (h.to_ascii_lowercase(), None),
    }
}

#[derive(Default)]
struct Cols {
    date: Option<usize>,
    tz: Option<usize>,
    workout: Option<usize>,
    workout_duration: Option<usize>,
    group: Option<usize>,
    exercise: Option<usize>,
    set: Option<usize>,
    weight: Option<usize>,
    weight_unit_col: Option<usize>,
    reps: Option<usize>,
    rpe: Option<usize>,
    distance: Option<usize>,
    distance_unit_col: Option<usize>,
    set_duration: Option<usize>,
    rest: Option<usize>,
    workout_notes: Option<usize>,
    exercise_notes: Option<usize>,
}

fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    if first.matches(';').count() > first.matches(',').count() { b';' } else { b',' }
}

fn reader(text: &str) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .delimiter(sniff_delimiter(text))
        .flexible(true)
        .from_reader(text.as_bytes())
}

// ---------------------------------------------------------------------------------------------
// Field parsing

fn field<'a>(rec: &'a csv::StringRecord, col: Option<usize>) -> &'a str {
    col.and_then(|i| rec.get(i)).map(str::trim).unwrap_or("")
}

fn opt_text(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn num(s: &str) -> Option<f64> {
    if s.is_empty() { None } else { s.parse().ok() }
}

pub(crate) fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
}

/// Parses `"America/Chicago (-05:00) [device]"` into a zone name and UTC offset in minutes.
fn parse_timezone(s: &str) -> (Option<String>, Option<i32>) {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^([^\s(\[]+)?\s*(?:\(([+-])(\d{1,2}):(\d{2})\))?").unwrap());
    let Some(c) = re.captures(s) else { return (None, None) };
    let name = c.get(1).map(|m| m.as_str().to_string());
    let offset = c.get(2).map(|sign| {
        let h: i32 = c[3].parse().unwrap();
        let m: i32 = c[4].parse().unwrap();
        let v = h * 60 + m;
        if sign.as_str() == "-" { -v } else { v }
    });
    (name, offset)
}

/// Workout duration: plain seconds (`"4200"`) or Strong's older text form (`"1h 10m"`).
fn parse_duration(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    if let Ok(v) = s.parse::<f64>() {
        return Some(v.round() as i64);
    }
    let mut total = 0i64;
    let mut found = false;
    for part in s.split_whitespace() {
        let (n, unit) = part.split_at(part.find(|c: char| !c.is_ascii_digit())?);
        let n: i64 = n.parse().ok()?;
        total += n * match unit {
            "h" => 3600,
            "m" | "min" => 60,
            "s" => 1,
            _ => return None,
        };
        found = true;
    }
    found.then_some(total)
}

fn parse_set_type(s: &str) -> Option<SetType> {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        return Some(SetType::Normal);
    }
    match s.to_ascii_uppercase().as_str() {
        "W" => Some(SetType::Warmup),
        "D" => Some(SetType::Drop),
        "F" => Some(SetType::Failure),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Workouts

pub fn parse_workouts(text: &str, opts: ImportOptions) -> Result<StrongImport> {
    let mut rdr = reader(text);
    let headers = rdr.headers()?.clone();

    let mut cols = Cols::default();
    let mut header_weight_unit = None;
    let mut header_distance_unit = None;
    let split: Vec<_> = headers.iter().map(split_header).collect();
    let has = |name: &str| split.iter().any(|(b, _)| b == name);
    // In the old format "Duration" is the workout duration and "Seconds" the set duration;
    // in the current one they are "Workout Duration" and "Duration".
    let old_duration_style = !has("workout duration");
    for (i, (base, unit)) in split.iter().enumerate() {
        let slot = match base.as_str() {
            "date" => &mut cols.date,
            "timezone" => &mut cols.tz,
            "workout" | "workout name" => &mut cols.workout,
            "workout duration" => &mut cols.workout_duration,
            "duration" if old_duration_style => &mut cols.workout_duration,
            "duration" | "seconds" => &mut cols.set_duration,
            "group" => &mut cols.group,
            "exercise" | "exercise name" => &mut cols.exercise,
            "set" | "set order" => &mut cols.set,
            "weight" => {
                header_weight_unit = unit.as_deref().and_then(WeightUnit::parse);
                &mut cols.weight
            }
            "weight unit" => &mut cols.weight_unit_col,
            "reps" => &mut cols.reps,
            "rpe" => &mut cols.rpe,
            "distance" => {
                header_distance_unit = unit.as_deref().and_then(DistanceUnit::parse);
                &mut cols.distance
            }
            "distance unit" => &mut cols.distance_unit_col,
            "rest timer" => &mut cols.rest,
            "workout notes" => &mut cols.workout_notes,
            "exercise notes" | "notes" => &mut cols.exercise_notes,
            _ => continue,
        };
        *slot = Some(i);
    }
    for (name, col) in [
        ("Date", cols.date),
        ("Workout", cols.workout),
        ("Exercise", cols.exercise),
        ("Set", cols.set),
    ] {
        if col.is_none() {
            return Err(Error::Format(format!(
                "this doesn't look like a Strong workouts export: no \"{name}\" column"
            )));
        }
    }
    let weight_unit = header_weight_unit.or(opts.weight_unit);
    let distance_unit = header_distance_unit.or(opts.distance_unit);
    if cols.weight.is_some() && weight_unit.is_none() && cols.weight_unit_col.is_none() {
        return Err(Error::MissingWeightUnit);
    }

    let mut warnings = Vec::new();
    let mut workouts: Vec<ImportedWorkout> = Vec::new();
    let mut workout_idx: HashMap<(NaiveDateTime, String), usize> = HashMap::new();
    // Per workout: (exercise name, group letter) -> exercise index, and group letter -> superset index.
    let mut exercise_idx: Vec<HashMap<(String, String), usize>> = Vec::new();
    let mut group_idx: Vec<HashMap<String, u32>> = Vec::new();
    let mut notes_conflicts = 0usize;
    let mut row_count = 0usize;

    for (row_no, rec) in rdr.records().enumerate() {
        let rec = rec?;
        row_count += 1;
        let line = row_no + 2;
        let Some(started) = parse_datetime(field(&rec, cols.date)) else {
            warnings.push(format!("row {line}: unreadable date {:?}, skipped", field(&rec, cols.date)));
            continue;
        };
        let wname = field(&rec, cols.workout).to_string();
        let ex_name = field(&rec, cols.exercise).to_string();
        if ex_name.is_empty() {
            warnings.push(format!("row {line}: no exercise name, skipped"));
            continue;
        }
        let set_label = field(&rec, cols.set);
        let Some(set_type) = parse_set_type(set_label) else {
            warnings.push(format!("row {line}: unrecognised set label {set_label:?}, skipped"));
            continue;
        };

        let wi = *workout_idx.entry((started, wname.clone())).or_insert_with(|| {
            let (tz_name, utc_offset_min) = parse_timezone(field(&rec, cols.tz));
            workouts.push(ImportedWorkout {
                source_key: format!("strong:{}|{}", started.format("%Y-%m-%d %H:%M:%S"), wname),
                started_at_local: started,
                tz_name,
                utc_offset_min,
                name: wname.clone(),
                duration_s: parse_duration(field(&rec, cols.workout_duration)),
                notes: None,
                exercises: Vec::new(),
            });
            exercise_idx.push(HashMap::new());
            group_idx.push(HashMap::new());
            workouts.len() - 1
        });
        let workout = &mut workouts[wi];
        if workout.notes.is_none() {
            workout.notes = opt_text(field(&rec, cols.workout_notes));
        }

        let group = field(&rec, cols.group).to_string();
        let superset = (!group.is_empty()).then(|| {
            let groups = &mut group_idx[wi];
            let next = groups.len() as u32;
            *groups.entry(group.clone()).or_insert(next)
        });
        let ei = *exercise_idx[wi].entry((ex_name.clone(), group)).or_insert_with(|| {
            workout.exercises.push(ImportedExercise {
                name: ex_name.clone(),
                superset,
                notes: None,
                sets: Vec::new(),
            });
            workout.exercises.len() - 1
        });
        let exercise = &mut workout.exercises[ei];
        // Strong repeats the exercise note on every set row: keep it once.
        if let Some(note) = opt_text(field(&rec, cols.exercise_notes)) {
            match &exercise.notes {
                None => exercise.notes = Some(note),
                Some(existing) if *existing != note => notes_conflicts += 1,
                _ => {}
            }
        }

        let row_weight_unit = WeightUnit::parse(field(&rec, cols.weight_unit_col)).or(weight_unit);
        let row_distance_unit =
            DistanceUnit::parse(field(&rec, cols.distance_unit_col)).or(distance_unit);
        let weight = num(field(&rec, cols.weight));
        // Older exports write 0 rather than leaving distance/duration blank.
        let distance = num(field(&rec, cols.distance)).filter(|&d| d != 0.0);
        let distance_m = match (distance, row_distance_unit) {
            (Some(d), Some(u)) => Some(u.to_m(d)),
            (Some(_), None) => {
                warnings.push(format!("row {line}: distance without a unit, ignored"));
                None
            }
            _ => None,
        };
        exercise.sets.push(ImportedSet {
            set_type,
            // A weight column without a unit was rejected above, so the unit is known here.
            weight_kg: weight.zip(row_weight_unit).map(|(w, u)| u.to_kg(w)),
            reps: num(field(&rec, cols.reps)).map(|r| r.round() as i64),
            rpe: num(field(&rec, cols.rpe)),
            distance_m,
            duration_s: num(field(&rec, cols.set_duration)).filter(|&d| d != 0.0),
            rest_s: num(field(&rec, cols.rest)).map(|r| r.round() as i64),
        });
    }

    if notes_conflicts > 0 {
        warnings.push(format!(
            "{notes_conflicts} set rows had a different exercise note from earlier rows of the same exercise; the first note was kept"
        ));
    }
    workouts.sort_by(|a, b| a.started_at_local.cmp(&b.started_at_local).then(a.name.cmp(&b.name)));
    Ok(StrongImport { workouts, weight_unit, row_count, warnings })
}

// ---------------------------------------------------------------------------------------------
// Bodyweight

#[derive(Debug, Clone, Serialize)]
pub struct WeighIn {
    pub measured_at_local: NaiveDateTime,
    pub weight_kg: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BodyweightImport {
    pub entries: Vec<WeighIn>,
    pub weight_unit: WeightUnit,
    pub row_count: usize,
    pub warnings: Vec<String>,
}

pub fn parse_bodyweight(text: &str, opts: ImportOptions) -> Result<BodyweightImport> {
    let mut rdr = reader(text);
    let headers = rdr.headers()?.clone();
    let mut date_col = None;
    let mut weight_col = None;
    let mut unit = opts.weight_unit;
    for (i, h) in headers.iter().enumerate() {
        match split_header(h) {
            (b, _) if b == "date" => date_col = Some(i),
            (b, u) if b == "weight" || b == "body weight" || b == "bodyweight" => {
                weight_col = Some(i);
                if let Some(u) = u.as_deref().and_then(WeightUnit::parse) {
                    unit = Some(u);
                }
            }
            _ => {}
        }
    }
    let (Some(date_col), Some(weight_col)) = (date_col, weight_col) else {
        return Err(Error::Format("expected \"Date\" and \"Weight\" columns".into()));
    };
    let unit = unit.ok_or(Error::MissingWeightUnit)?;

    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut row_count = 0;
    for (row_no, rec) in rdr.records().enumerate() {
        let rec = rec?;
        row_count += 1;
        let date = field(&rec, Some(date_col));
        let weight = field(&rec, Some(weight_col));
        match (parse_datetime(date), num(weight)) {
            (Some(t), Some(w)) if w > 0.0 => entries.push(WeighIn { measured_at_local: t, weight_kg: unit.to_kg(w) }),
            _ => warnings.push(format!("row {}: couldn't read {date:?} / {weight:?}, skipped", row_no + 2)),
        }
    }
    entries.sort_by_key(|e| e.measured_at_local);
    Ok(BodyweightImport { entries, weight_unit: unit, row_count, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timezone() {
        assert_eq!(
            parse_timezone("America/Chicago (-05:00) [device]"),
            (Some("America/Chicago".into()), Some(-300))
        );
        assert_eq!(parse_timezone("Asia/Kolkata (+05:30)"), (Some("Asia/Kolkata".into()), Some(330)));
        assert_eq!(parse_timezone(""), (None, None));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("4200"), Some(4200));
        assert_eq!(parse_duration("1h 10m"), Some(4200));
        assert_eq!(parse_duration("45m"), Some(2700));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("soon"), None);
    }

    #[test]
    fn set_types() {
        assert_eq!(parse_set_type("3"), Some(SetType::Normal));
        assert_eq!(parse_set_type("w"), Some(SetType::Warmup));
        assert_eq!(parse_set_type("D"), Some(SetType::Drop));
        assert_eq!(parse_set_type("F"), Some(SetType::Failure));
        assert_eq!(parse_set_type("Rest Timer"), None);
    }

    #[test]
    fn headers() {
        assert_eq!(split_header("Weight (lb)"), ("weight".into(), Some("lb".into())));
        assert_eq!(split_header("\u{feff}Date"), ("date".into(), None));
    }
}
