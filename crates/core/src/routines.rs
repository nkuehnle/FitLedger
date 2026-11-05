//! Shareable routines: a program (templates plus an optional schedule) as a small JSON file. A
//! routine holds a plan only, never logged weights or history.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::exercises::ExerciseEdit;
use crate::planning::{Program, Schedule};
use crate::stats::week_start;
use crate::templates::{Template, TemplateExercise};
use crate::{Error, Result};

pub const ROUTINE_FORMAT: &str = "fitledger-routine";
pub const ROUTINE_VERSION: u32 = 1;
const MAX_ROUTINE_BYTES: usize = 1_000_000;
const MAX_CYCLE_DAYS: i64 = 366;
const EXERCISE_KINDS: [&str; 6] = ["weight_reps", "bodyweight_reps", "assisted_bodyweight", "duration", "distance_duration", "weight_distance"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Routine {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub description: Option<String>,
    pub schedule: Option<RoutineSchedule>,
    /// Template name -> exercises, in workout order.
    pub templates: BTreeMap<String, Vec<RoutineExercise>>,
    /// Custom exercises the templates use, so the file works on another device.
    #[serde(default)]
    pub custom_exercises: Vec<RoutineCustomExercise>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineSchedule {
    /// "weekly" (day 0 = Monday) or "cycle".
    pub kind: String,
    pub cycle_length: i64,
    /// Template name per day; `None` = rest.
    pub days: Vec<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineExercise {
    pub slug: String,
    pub sets: Option<i64>,
    pub rep_min: Option<i64>,
    pub rep_max: Option<i64>,
    pub superset: Option<i64>,
    pub rest_s: Option<i64>,
    pub setup_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineCustomExercise {
    pub slug: String,
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

/// What importing a routine would do. Nothing has been changed yet.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RoutinePreview {
    pub name: String,
    pub description: Option<String>,
    /// (template name, exercise count)
    pub templates: Vec<(String, usize)>,
    pub has_schedule: bool,
    /// Custom exercises that will be created, by name.
    pub new_exercises: Vec<String>,
}

enum Resolved {
    Existing(i64),
    New(RoutineCustomExercise),
}

/// Reads and checks a routine file. This doesn't touch the database.
pub fn parse_routine(text: &str) -> Result<Routine> {
    if text.len() > MAX_ROUTINE_BYTES {
        return Err(Error::Format("this file is too large to be a routine".into()));
    }
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| Error::Format("not a FitLedger routine".into()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(ROUTINE_FORMAT) {
        return Err(Error::Format("not a FitLedger routine".into()));
    }
    if value.get("version").and_then(|v| v.as_u64()).is_none_or(|v| v > ROUTINE_VERSION as u64) {
        return Err(Error::Format("this routine was made by a newer version of FitLedger; update the app first".into()));
    }
    let r: Routine = serde_json::from_value(value).map_err(|e| Error::Format(format!("this routine is damaged: {e}")))?;
    if r.name.trim().is_empty() || r.templates.is_empty() {
        return Err(Error::Format("this routine is empty".into()));
    }
    if r.templates.keys().any(|n| n.trim().is_empty()) {
        return Err(Error::Format("this routine has a template without a name".into()));
    }
    if let Some(s) = &r.schedule {
        let len = match s.kind.as_str() {
            "weekly" => 7,
            "cycle" => s.cycle_length,
            other => return Err(Error::Format(format!("unknown schedule kind {other:?}"))),
        };
        if !(1..=MAX_CYCLE_DAYS).contains(&len) || s.days.len() as i64 != len {
            return Err(Error::Format("this routine's schedule is damaged".into()));
        }
        if let Some(name) = s.days.iter().flatten().find(|n| !r.templates.contains_key(*n)) {
            return Err(Error::Format(format!("the schedule uses a missing template {name:?}")));
        }
    }
    if let Some(e) = r.custom_exercises.iter().find(|e| !EXERCISE_KINDS.contains(&e.kind.as_str())) {
        return Err(Error::Format(format!("unknown exercise kind {:?}", e.kind)));
    }
    Ok(r)
}

impl RoutineCustomExercise {
    fn edit(&self) -> ExerciseEdit {
        ExerciseEdit {
            name: self.name.clone(),
            equipment: self.equipment.clone(),
            pattern: self.pattern.clone(),
            kind: self.kind.clone(),
            load_mode: self.load_mode.clone(),
            default_rest_s: self.default_rest_s,
            setup_note: self.setup_note.clone(),
            primary: self.primary.clone(),
            secondary: self.secondary.clone(),
        }
    }
}

/// A built-in program as a routine, so both install the same way.
fn program_routine(p: &Program) -> Result<Routine> {
    let mut templates = BTreeMap::new();
    for (name, rows) in &p.templates {
        let mut tags: HashMap<String, i64> = HashMap::new();
        let mut exercises = Vec::new();
        for row in rows {
            let a = row.as_array().ok_or_else(|| Error::Format("bad program row".into()))?;
            let superset = a.get(4).and_then(|v| v.as_str()).map(|tag| {
                let next = tags.len() as i64;
                *tags.entry(tag.to_string()).or_insert(next)
            });
            exercises.push(RoutineExercise {
                slug: a[0].as_str().unwrap_or_default().to_string(),
                sets: a.get(1).and_then(|v| v.as_i64()),
                rep_min: a.get(2).and_then(|v| v.as_i64()),
                rep_max: a.get(3).and_then(|v| v.as_i64()),
                superset,
                rest_s: None,
                setup_note: None,
            });
        }
        templates.insert(name.clone(), exercises);
    }
    let weekly = p.days.len() == 7;
    Ok(Routine {
        format: ROUTINE_FORMAT.into(),
        version: ROUTINE_VERSION,
        name: p.name.clone(),
        description: Some(p.description.clone()),
        schedule: Some(RoutineSchedule { kind: if weekly { "weekly" } else { "cycle" }.into(), cycle_length: p.days.len() as i64, days: p.days.clone() }),
        templates,
        custom_exercises: Vec::new(),
    })
}

impl Db {
    /// The active schedule and its templates as a routine.
    pub fn export_active_routine(&self) -> Result<Routine> {
        let s = self.active_schedule()?.ok_or_else(|| Error::Format("there's no active schedule to export".into()))?;
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut templates = BTreeMap::new();
        let mut custom = BTreeMap::new();
        for id in s.days.iter().flatten() {
            if names.contains_key(id) {
                continue;
            }
            let t = self.template(*id)?.ok_or_else(|| Error::Format("the schedule uses a missing template".into()))?;
            // Two templates can share a name; the file is keyed by name.
            let mut name = t.name.clone();
            let mut n = 2;
            while templates.contains_key(&name) {
                name = format!("{} {n}", t.name);
                n += 1;
            }
            templates.insert(name.clone(), self.routine_exercises(&t, &mut custom)?);
            names.insert(*id, name);
        }
        let days = s.days.iter().map(|d| d.and_then(|id| names.get(&id).cloned())).collect();
        Ok(Routine {
            format: ROUTINE_FORMAT.into(),
            version: ROUTINE_VERSION,
            name: s.name,
            description: None,
            schedule: Some(RoutineSchedule { kind: s.kind, cycle_length: s.cycle_length, days }),
            templates,
            custom_exercises: custom.into_values().collect(),
        })
    }

    /// One template as a routine, with no schedule.
    pub fn export_template_routine(&self, id: i64) -> Result<Routine> {
        let t = self.template(id)?.ok_or_else(|| Error::Format("template not found".into()))?;
        let mut custom = BTreeMap::new();
        let exercises = self.routine_exercises(&t, &mut custom)?;
        Ok(Routine {
            format: ROUTINE_FORMAT.into(),
            version: ROUTINE_VERSION,
            name: t.name.clone(),
            description: None,
            schedule: None,
            templates: BTreeMap::from([(t.name, exercises)]),
            custom_exercises: custom.into_values().collect(),
        })
    }

    fn routine_exercises(&self, t: &Template, custom: &mut BTreeMap<String, RoutineCustomExercise>) -> Result<Vec<RoutineExercise>> {
        let mut out = Vec::new();
        for e in &t.exercises {
            let info = self.exercise_info(e.exercise_id)?.ok_or_else(|| Error::Format("template uses a missing exercise".into()))?;
            if info.is_custom {
                custom.insert(
                    info.slug.clone(),
                    RoutineCustomExercise {
                        slug: info.slug.clone(),
                        name: info.name,
                        equipment: info.equipment,
                        pattern: info.pattern,
                        kind: info.kind,
                        load_mode: info.load_mode,
                        default_rest_s: info.default_rest_s,
                        setup_note: info.setup_note,
                        primary: info.primary,
                        secondary: info.secondary,
                    },
                );
            }
            out.push(RoutineExercise {
                slug: info.slug,
                sets: e.target_sets,
                rep_min: e.rep_min,
                rep_max: e.rep_max,
                superset: e.superset,
                rest_s: e.rest_s,
                setup_note: e.setup_note.clone(),
            });
        }
        Ok(out)
    }

    /// Matches each exercise by slug, then a custom one by exact name. Anything else in
    /// `custom_exercises` will be created. Validates everything, so a later write can't fail
    /// halfway through because of the file's contents.
    fn resolve_exercises(&self, r: &Routine) -> Result<HashMap<String, Resolved>> {
        let mut out = HashMap::new();
        for slug in r.templates.values().flatten().map(|e| &e.slug) {
            if out.contains_key(slug) {
                continue;
            }
            let custom = r.custom_exercises.iter().find(|c| &c.slug == slug);
            let found = match (self.exercise_id(slug)?, custom) {
                (Some(id), _) => Resolved::Existing(id),
                (None, Some(c)) => match self.exercise_named(&c.name)? {
                    Some(id) => Resolved::Existing(id),
                    None => {
                        self.validate_exercise(&c.edit())?;
                        Resolved::New(c.clone())
                    }
                },
                (None, None) => return Err(Error::Format(format!("this routine uses an unknown exercise {slug:?}"))),
            };
            out.insert(slug.clone(), found);
        }
        Ok(out)
    }

    fn exercise_named(&self, name: &str) -> Result<Option<i64>> {
        Ok(self
            .conn()
            .query_row("SELECT id FROM exercise WHERE lower(name) = lower(?1) AND hidden = 0 ORDER BY is_custom LIMIT 1", [name.trim()], |r| r.get(0))
            .optional()?)
    }

    /// What importing `r` would do, without changing anything.
    pub fn preview_routine(&self, r: &Routine) -> Result<RoutinePreview> {
        let resolved = self.resolve_exercises(r)?;
        let mut new_exercises: Vec<String> = resolved
            .values()
            .filter_map(|e| match e {
                Resolved::New(c) => Some(c.name.clone()),
                Resolved::Existing(_) => None,
            })
            .collect();
        new_exercises.sort();
        Ok(RoutinePreview {
            name: r.name.clone(),
            description: r.description.clone(),
            templates: r.templates.iter().map(|(n, e)| (n.clone(), e.len())).collect(),
            has_schedule: r.schedule.is_some(),
            new_exercises,
        })
    }

    /// Adds the routine's templates (names get the routine's name appended if taken). With
    /// `activate`, its schedule becomes the active one from `start`. Existing data is never replaced.
    pub fn install_routine(&mut self, r: &Routine, activate: bool, start: NaiveDate) -> Result<Vec<i64>> {
        let resolved = self.resolve_exercises(r)?;
        let mut exercise_ids: HashMap<String, i64> = HashMap::new();
        for (slug, e) in resolved {
            let id = match e {
                Resolved::Existing(id) => id,
                Resolved::New(c) => self.create_exercise(&c.edit())?,
            };
            exercise_ids.insert(slug, id);
        }
        let mut taken: Vec<String> = self.list_templates()?.into_iter().map(|t| t.name).collect();
        let mut ids: HashMap<String, i64> = HashMap::new();
        for (name, rows) in &r.templates {
            let exercises = rows
                .iter()
                .map(|e| TemplateExercise {
                    exercise_id: exercise_ids[&e.slug],
                    name: String::new(),
                    superset: e.superset,
                    target_sets: e.sets,
                    rep_min: e.rep_min,
                    rep_max: e.rep_max,
                    rest_s: e.rest_s,
                    setup_note: e.setup_note.clone(),
                })
                .collect();
            let tname = free_name(name, &r.name, &taken);
            taken.push(tname.clone());
            let id = self.save_template(&Template { id: None, name: tname, notes: None, color: None, exercises, last_done: None })?;
            ids.insert(name.clone(), id);
        }
        if let (true, Some(s)) = (activate, &r.schedule) {
            let days = s.days.iter().map(|d| d.as_ref().and_then(|n| ids.get(n).copied())).collect();
            // A whole-weeks cycle counts from a Monday so its days still land on the right weekdays.
            let from = if s.kind == "cycle" && s.cycle_length % 7 == 0 { week_start(start) } else { start };
            self.save_schedule(&Schedule { id: None, name: r.name.clone(), kind: s.kind.clone(), start_date: from.to_string(), cycle_length: s.cycle_length, days })?;
        }
        Ok(ids.into_values().collect())
    }

    /// Creates a built-in program's templates and makes its schedule active from `start`.
    pub fn install_program(&mut self, program_id: &str, start: NaiveDate) -> Result<Vec<i64>> {
        let p = crate::planning::programs().into_iter().find(|p| p.id == program_id).ok_or_else(|| Error::Format(format!("unknown program {program_id:?}")))?;
        self.install_routine(&program_routine(&p)?, true, start)
    }
}

fn free_name(name: &str, routine: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_string();
    }
    let base = format!("{name} ({routine})");
    let mut candidate = base.clone();
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{name} ({routine} {n})");
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()
    }

    fn edit(name: &str) -> ExerciseEdit {
        ExerciseEdit {
            name: name.into(),
            equipment: Some("cable".into()),
            pattern: None,
            kind: "weight_reps".into(),
            load_mode: "per_hand".into(),
            default_rest_s: Some(75),
            setup_note: Some("pin 4".into()),
            primary: vec![],
            secondary: vec![],
        }
    }

    /// Bench and a custom exercise in a superset, on Monday, repeated Thursday.
    fn source() -> Db {
        let mut db = Db::open_in_memory().unwrap();
        let bench = db.exercise_id("bench-press-barbell").unwrap().unwrap();
        let fly = db.create_exercise(&edit("Banded Fly")).unwrap();
        let te = |exercise_id, superset, rest_s| TemplateExercise {
            exercise_id,
            name: String::new(),
            superset,
            target_sets: Some(3),
            rep_min: Some(6),
            rep_max: Some(10),
            rest_s,
            setup_note: Some("slow".into()),
        };
        let exercises = vec![te(bench, Some(0), Some(120)), te(fly, Some(0), None)];
        let id = db.save_template(&Template { id: None, name: "Push".into(), notes: None, color: None, exercises, last_done: None }).unwrap();
        let mut days = vec![None; 7];
        days[0] = Some(id);
        days[3] = Some(id);
        db.save_schedule(&Schedule { id: None, name: "My split".into(), kind: "weekly".into(), start_date: "2026-11-02".into(), cycle_length: 7, days }).unwrap();
        db
    }

    fn round_trip(r: &Routine) -> Routine {
        parse_routine(&serde_json::to_string(r).unwrap()).unwrap()
    }

    #[test]
    fn export_then_import_on_another_device() {
        let routine = round_trip(&source().export_active_routine().unwrap());
        let mut other = Db::open_in_memory().unwrap();
        let preview = other.preview_routine(&routine).unwrap();
        assert_eq!(preview.templates, vec![("Push".to_string(), 2)]);
        assert_eq!(preview.new_exercises, vec!["Banded Fly".to_string()]);
        assert_eq!(other.list_templates().unwrap().len(), 0, "preview changes nothing");

        other.install_routine(&routine, true, d()).unwrap();
        let t = &other.list_templates().unwrap()[0];
        let t = other.template(t.id.unwrap()).unwrap().unwrap();
        assert_eq!(t.exercises.len(), 2);
        assert_eq!((t.exercises[0].target_sets, t.exercises[0].rep_min, t.exercises[0].rep_max), (Some(3), Some(6), Some(10)));
        assert_eq!((t.exercises[0].superset, t.exercises[1].superset), (Some(0), Some(0)));
        assert_eq!(t.exercises[0].rest_s, Some(120));
        assert_eq!(t.exercises[0].setup_note.as_deref(), Some("slow"));

        let fly = other.exercise_info(t.exercises[1].exercise_id).unwrap().unwrap();
        assert_eq!((fly.name.as_str(), fly.load_mode.as_str()), ("Banded Fly", "per_hand"));
        let s = other.active_schedule().unwrap().unwrap();
        assert_eq!(s.name, "My split");
        assert_eq!(s.days.iter().flatten().count(), 2);
        assert_eq!((s.days[0], s.days[3]), (t.id, t.id));
    }

    #[test]
    fn importing_twice_keeps_one_custom_exercise_and_distinct_templates() {
        let routine = source().export_active_routine().unwrap();
        let mut other = Db::open_in_memory().unwrap();
        other.install_routine(&routine, false, d()).unwrap();
        assert!(other.preview_routine(&routine).unwrap().new_exercises.is_empty());
        other.install_routine(&routine, false, d()).unwrap();
        other.install_routine(&routine, false, d()).unwrap();
        let customs = other.list_exercises(false).unwrap().into_iter().filter(|e| e.name == "Banded Fly").count();
        assert_eq!(customs, 1);
        let mut names: Vec<String> = other.list_templates().unwrap().into_iter().map(|t| t.name).collect();
        names.sort();
        assert_eq!(names, vec!["Push", "Push (My split 2)", "Push (My split)"]);
    }

    #[test]
    fn schedule_only_changes_when_asked() {
        let routine = source().export_active_routine().unwrap();
        let mut other = source();
        let before = other.active_schedule().unwrap();
        other.install_routine(&routine, false, d()).unwrap();
        assert_eq!(other.active_schedule().unwrap(), before);
    }

    #[test]
    fn single_template_has_no_schedule() {
        let db = source();
        let id = db.list_templates().unwrap()[0].id.unwrap();
        let r = round_trip(&db.export_template_routine(id).unwrap());
        assert_eq!(r.schedule, None);
        assert_eq!(r.custom_exercises.len(), 1);
    }

    #[test]
    fn routines_hold_no_logged_data() {
        let text = serde_json::to_string(&source().export_active_routine().unwrap()).unwrap();
        for word in ["weight_kg", "last_done", "workout"] {
            assert!(!text.contains(word), "{word} in {text}");
        }
    }

    #[test]
    fn bad_files_change_nothing() {
        let good = serde_json::to_value(source().export_active_routine().unwrap()).unwrap();
        let change = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            f(&mut v);
            v.to_string()
        };
        assert!(parse_routine("not json").is_err());
        assert!(parse_routine("{}").is_err());
        assert!(parse_routine(&"x".repeat(MAX_ROUTINE_BYTES + 1)).is_err());
        assert!(parse_routine(&change(&|v| v["format"] = "strong".into())).is_err());
        let newer = parse_routine(&change(&|v| v["version"] = 99.into())).unwrap_err().to_string();
        assert!(newer.contains("newer version"), "{newer}");
        assert!(parse_routine(&change(&|v| v["schedule"]["days"][0] = "Nope".into())).is_err());
        assert!(parse_routine(&change(&|v| v["schedule"]["days"] = serde_json::json!([]))).is_err());
        assert!(parse_routine(&change(&|v| v["templates"] = serde_json::json!({}))).is_err());
        assert!(parse_routine(&change(&|v| v["custom_exercises"][0]["kind"] = "nope".into())).is_err());

        // These parse, but can't be installed: nothing may be left behind.
        let mut db = Db::open_in_memory().unwrap();
        let unknown = parse_routine(&change(&|v| v["custom_exercises"] = serde_json::json!([]))).unwrap();
        assert!(db.preview_routine(&unknown).is_err());
        assert!(db.install_routine(&unknown, true, d()).is_err());
        let muscle = parse_routine(&change(&|v| v["custom_exercises"][0]["primary"] = serde_json::json!(["nope"]))).unwrap();
        assert!(db.install_routine(&muscle, true, d()).is_err());
        assert_eq!(db.list_templates().unwrap().len(), 0);
        assert_eq!(db.active_schedule().unwrap(), None);
        assert!(db.list_exercises(false).unwrap().iter().all(|e| e.name != "Banded Fly"));
    }
}
