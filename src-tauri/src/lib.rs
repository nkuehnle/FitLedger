//! Tauri shell: thin commands over `liftbook-core`. Every command locks the database, runs one
//! core operation and maps errors to strings for the UI.

use std::collections::HashSet;
use std::sync::Mutex;

use base64::Engine;
use chrono::{Local, NaiveDate, NaiveDateTime};
use liftbook_core::body::{CsvMapping, DietDay, EnergyPoint, InBodyCsvPreview, InBodyScan, MeasurementRow, WeighInRow, BODY_PARTS, INBODY_FIELDS};
use liftbook_core::db::{Counts, Db, ImportOutcome};
use liftbook_core::exercises::{ExerciseEdit, ExerciseInfo, PerHandSuspect};
use liftbook_core::export::BackupInfo;
use liftbook_core::history::{WorkoutCard, WorkoutDetail};
use liftbook_core::library::{library, Muscle, MuscleGroup};
use liftbook_core::live::{ExerciseUpdate, OrderEntry, Previous, SetUpdate, StartFrom, WorkoutUpdate};
use liftbook_core::planning::{Goal, GoalEntry, HistoryProposal, MuscleTarget, Phase, Program, Schedule, Suggestion};
use liftbook_core::review::{NameReview, SuggestionView};
use liftbook_core::stats::{DiscomfortPoint, ExerciseStats, LastTrained, MuscleStat, PeriodCount, Streak, WeekMuscleSets, WorkoutPoint};
use liftbook_core::strong::{self, ImportOptions};
use liftbook_core::templates::{CreatedTemplate, Template};
use liftbook_core::units::{DistanceUnit, WeightUnit};
use serde::Serialize;
use tauri::{Manager, State};

struct AppState(Mutex<Db>);

type CmdResult<T> = Result<T, String>;

fn err(e: liftbook_core::Error) -> String {
    match e {
        // The UI looks for this code to ask the user for a unit.
        liftbook_core::Error::MissingWeightUnit => "MISSING_WEIGHT_UNIT".into(),
        e => e.to_string(),
    }
}

fn with<T>(state: &State<AppState>, f: impl FnOnce(&mut Db) -> liftbook_core::Result<T>) -> CmdResult<T> {
    let mut db = state.0.lock().map_err(|_| "database lock poisoned".to_string())?;
    f(&mut db).map_err(err)
}

fn date(s: &str) -> CmdResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("bad date {s:?}"))
}

fn now() -> (NaiveDateTime, i32) {
    let n = Local::now();
    (n.naive_local(), n.offset().local_minus_utc() / 60)
}

fn opts(weight_unit: Option<WeightUnit>, distance_unit: Option<DistanceUnit>) -> ImportOptions {
    ImportOptions { weight_unit, distance_unit }
}

fn unit_setting(db: &Db) -> liftbook_core::Result<WeightUnit> {
    Ok(if db.setting("weight_unit")?.as_deref() == Some("kg") { WeightUnit::Kg } else { WeightUnit::Lb })
}

/// Remembers the file's unit as the display unit the first time anything is imported.
fn default_display_unit(db: &Db, unit: Option<WeightUnit>) -> liftbook_core::Result<()> {
    if let (None, Some(u)) = (db.setting("weight_unit")?, unit) {
        db.set_setting("weight_unit", if u == WeightUnit::Kg { "kg" } else { "lb" })?;
    }
    Ok(())
}

// ---- Import -----------------------------------------------------------------------------------

#[derive(Serialize)]
struct WorkoutPreview {
    workouts: usize,
    sets: usize,
    exercise_names: usize,
    first: Option<String>,
    last: Option<String>,
    already_imported: usize,
    weight_unit: Option<WeightUnit>,
    warnings: Vec<String>,
}

#[tauri::command]
fn preview_strong_workouts(state: State<AppState>, text: String, weight_unit: Option<WeightUnit>, distance_unit: Option<DistanceUnit>) -> CmdResult<WorkoutPreview> {
    let imp = strong::parse_workouts(&text, opts(weight_unit, distance_unit)).map_err(err)?;
    let names: HashSet<_> = imp.workouts.iter().flat_map(|w| &w.exercises).map(|e| e.name.as_str()).collect();
    let already_imported = with(&state, |db| db.count_existing_workouts(&imp))?;
    Ok(WorkoutPreview {
        workouts: imp.workouts.len(),
        sets: imp.set_count(),
        exercise_names: names.len(),
        first: imp.workouts.first().map(|w| w.started_at_local.to_string()),
        last: imp.workouts.last().map(|w| w.started_at_local.to_string()),
        already_imported,
        weight_unit: imp.weight_unit,
        warnings: imp.warnings.clone(),
    })
}

#[derive(Serialize)]
struct WorkoutImportResult {
    outcome: ImportOutcome,
    auto_mapped: usize,
    needs_review: usize,
}

#[tauri::command]
fn import_strong_workouts(state: State<AppState>, text: String, file_name: String, weight_unit: Option<WeightUnit>, distance_unit: Option<DistanceUnit>) -> CmdResult<WorkoutImportResult> {
    let imp = strong::parse_workouts(&text, opts(weight_unit, distance_unit)).map_err(err)?;
    with(&state, |db| {
        let outcome = db.import_strong_workouts(&imp, &file_name)?;
        let auto_mapped = db.auto_map_names()?;
        default_display_unit(db, imp.weight_unit)?;
        db.rebuild_prs()?;
        let needs_review = db.name_review()?.iter().filter(|r| r.needs_review).count();
        Ok(WorkoutImportResult { outcome, auto_mapped, needs_review })
    })
}

#[derive(Serialize)]
struct BodyweightPreview {
    entries: usize,
    first: Option<String>,
    last: Option<String>,
    weight_unit: WeightUnit,
    warnings: Vec<String>,
}

#[tauri::command]
fn preview_bodyweight(text: String, weight_unit: Option<WeightUnit>) -> CmdResult<BodyweightPreview> {
    let imp = strong::parse_bodyweight(&text, opts(weight_unit, None)).map_err(err)?;
    Ok(BodyweightPreview {
        entries: imp.entries.len(),
        first: imp.entries.first().map(|e| e.measured_at_local.to_string()),
        last: imp.entries.last().map(|e| e.measured_at_local.to_string()),
        weight_unit: imp.weight_unit,
        warnings: imp.warnings,
    })
}

#[tauri::command]
fn import_bodyweight(state: State<AppState>, text: String, file_name: String, weight_unit: Option<WeightUnit>) -> CmdResult<ImportOutcome> {
    let imp = strong::parse_bodyweight(&text, opts(weight_unit, None)).map_err(err)?;
    with(&state, |db| {
        let out = db.import_bodyweight(&imp, &file_name, "strong")?;
        default_display_unit(db, Some(imp.weight_unit))?;
        Ok(out)
    })
}

#[derive(Serialize)]
struct MeasurementPreview {
    entries: usize,
    parts: Vec<String>,
    first: Option<String>,
    last: Option<String>,
    warnings: Vec<String>,
}

#[tauri::command]
fn preview_measurements(text: String) -> CmdResult<MeasurementPreview> {
    let imp = strong::parse_measurements(&text).map_err(err)?;
    Ok(MeasurementPreview {
        entries: imp.entries.len(),
        first: imp.entries.first().map(|e| e.measured_at_local.to_string()),
        last: imp.entries.last().map(|e| e.measured_at_local.to_string()),
        parts: imp.parts,
        warnings: imp.warnings,
    })
}

#[tauri::command]
fn import_measurements(state: State<AppState>, text: String, file_name: String) -> CmdResult<ImportOutcome> {
    let imp = strong::parse_measurements(&text).map_err(err)?;
    with(&state, |db| db.import_measurements(&imp, &file_name))
}

// ---- Name review --------------------------------------------------------------------------------

#[tauri::command]
fn name_review(state: State<AppState>) -> CmdResult<Vec<NameReview>> {
    with(&state, |db| db.name_review())
}

#[tauri::command]
fn set_name_mapping(state: State<AppState>, source_name: String, from_date: Option<String>, to_date: Option<String>, slug: String) -> CmdResult<()> {
    with(&state, |db| {
        db.set_name_mapping(&source_name, from_date.as_deref(), to_date.as_deref(), &slug)?;
        db.rebuild_prs().map(|_| ())
    })
}

#[tauri::command]
fn remove_name_mapping(state: State<AppState>, rule_id: i64) -> CmdResult<()> {
    with(&state, |db| {
        db.remove_name_mapping(rule_id)?;
        db.rebuild_prs().map(|_| ())
    })
}

#[tauri::command]
fn confirm_name_mapping(state: State<AppState>, rule_id: i64) -> CmdResult<()> {
    with(&state, |db| db.confirm_name_mapping(rule_id))
}

#[tauri::command]
fn search_exercises(state: State<AppState>, query: String) -> CmdResult<Vec<SuggestionView>> {
    with(&state, |db| db.search_exercises(&query, 20))
}

#[tauri::command]
fn per_hand_suspects(state: State<AppState>) -> CmdResult<Vec<PerHandSuspect>> {
    with(&state, |db| db.per_hand_suspects())
}

#[tauri::command]
fn set_session_load_mode(state: State<AppState>, workout_exercise_id: i64, mode: Option<String>) -> CmdResult<()> {
    with(&state, |db| db.set_session_load_mode(workout_exercise_id, mode.as_deref()))
}

// ---- History ------------------------------------------------------------------------------------

#[tauri::command]
fn history_page(state: State<AppState>, before_time: Option<String>, before_id: Option<i64>, limit: usize) -> CmdResult<Vec<WorkoutCard>> {
    with(&state, |db| db.history_page(before_time.as_deref().zip(before_id), limit))
}

#[tauri::command]
fn workout_detail(state: State<AppState>, id: i64) -> CmdResult<Option<WorkoutDetail>> {
    with(&state, |db| db.workout_detail(id))
}

#[tauri::command]
fn delete_workout(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.delete_workout(id))
}

#[tauri::command]
fn update_workout(state: State<AppState>, id: i64, update: WorkoutUpdate) -> CmdResult<()> {
    with(&state, |db| db.update_workout(id, &update))
}

#[tauri::command]
fn rebuild_prs(state: State<AppState>) -> CmdResult<usize> {
    with(&state, |db| db.rebuild_prs())
}

// ---- Live workout -------------------------------------------------------------------------------

#[tauri::command]
fn active_workout(state: State<AppState>) -> CmdResult<Option<i64>> {
    with(&state, |db| db.active_workout())
}

#[tauri::command]
fn start_workout(state: State<AppState>, from: StartFrom) -> CmdResult<i64> {
    let (t, off) = now();
    with(&state, |db| db.start_workout(&from, t, Some(off)))
}

#[tauri::command]
fn add_exercise(state: State<AppState>, workout_id: i64, exercise_id: i64) -> CmdResult<i64> {
    with(&state, |db| db.add_exercise(workout_id, exercise_id))
}

#[tauri::command]
fn remove_exercise(state: State<AppState>, workout_exercise_id: i64) -> CmdResult<()> {
    with(&state, |db| db.remove_exercise(workout_exercise_id))
}

#[tauri::command]
fn swap_exercise(state: State<AppState>, workout_exercise_id: i64, exercise_id: i64) -> CmdResult<()> {
    with(&state, |db| db.swap_exercise(workout_exercise_id, exercise_id))
}

#[tauri::command]
fn reorder_exercises(state: State<AppState>, workout_id: i64, order: Vec<OrderEntry>) -> CmdResult<()> {
    with(&state, |db| db.reorder_exercises(workout_id, &order))
}

#[tauri::command]
fn add_set(state: State<AppState>, workout_exercise_id: i64) -> CmdResult<i64> {
    with(&state, |db| db.add_set(workout_exercise_id))
}

#[tauri::command]
fn add_warmup_set(state: State<AppState>, workout_exercise_id: i64) -> CmdResult<i64> {
    with(&state, |db| db.add_warmup_set(workout_exercise_id))
}

#[tauri::command]
fn set_sticky_note(state: State<AppState>, exercise_id: i64, note: Option<String>) -> CmdResult<()> {
    with(&state, |db| db.set_sticky_note(exercise_id, note.as_deref()))
}

#[tauri::command]
fn remove_set(state: State<AppState>, set_id: i64) -> CmdResult<()> {
    with(&state, |db| db.remove_set(set_id))
}

#[tauri::command]
fn update_set(state: State<AppState>, set_id: i64, update: SetUpdate) -> CmdResult<()> {
    with(&state, |db| db.update_set(set_id, &update))
}

#[tauri::command]
fn update_workout_exercise(state: State<AppState>, workout_exercise_id: i64, update: ExerciseUpdate) -> CmdResult<()> {
    with(&state, |db| db.update_workout_exercise(workout_exercise_id, &update))
}

#[tauri::command]
fn set_rest_timer(state: State<AppState>, workout_id: i64, ends_at: Option<String>, total_s: Option<i64>) -> CmdResult<()> {
    with(&state, |db| db.set_rest_timer(workout_id, ends_at.as_deref(), total_s))
}

#[derive(Serialize)]
struct FinishResult {
    differs_from_template: bool,
}

#[tauri::command]
fn finish_workout(state: State<AppState>, workout_id: i64) -> CmdResult<FinishResult> {
    let (t, _) = now();
    with(&state, |db| {
        db.finish_workout(workout_id, t)?;
        Ok(FinishResult { differs_from_template: db.workout_differs_from_template(workout_id)? })
    })
}

#[tauri::command]
fn previous_for_workout(state: State<AppState>, workout_id: i64) -> CmdResult<Vec<Previous>> {
    with(&state, |db| db.previous_for_workout(workout_id))
}

#[tauri::command]
fn update_template_from_workout(state: State<AppState>, workout_id: i64) -> CmdResult<()> {
    with(&state, |db| db.update_template_from_workout(workout_id))
}

#[tauri::command]
fn similar_exercises(state: State<AppState>, exercise_id: i64) -> CmdResult<Vec<ExerciseInfo>> {
    with(&state, |db| db.similar_exercises(exercise_id, 12))
}

// ---- Library and stats ----------------------------------------------------------------------------

#[derive(Serialize)]
struct MuscleList {
    groups: Vec<MuscleGroup>,
    muscles: Vec<Muscle>,
}

#[tauri::command]
fn muscles() -> MuscleList {
    MuscleList { groups: library().groups.clone(), muscles: library().muscles.clone() }
}

#[tauri::command]
fn list_exercises(state: State<AppState>, only_used: bool) -> CmdResult<Vec<ExerciseInfo>> {
    with(&state, |db| db.list_exercises(only_used))
}

#[tauri::command]
fn exercise_info(state: State<AppState>, id: i64) -> CmdResult<Option<ExerciseInfo>> {
    with(&state, |db| db.exercise_info(id))
}

#[tauri::command]
fn update_exercise(state: State<AppState>, id: i64, edit: ExerciseEdit) -> CmdResult<()> {
    with(&state, |db| db.update_exercise(id, &edit))
}

#[tauri::command]
fn create_exercise(state: State<AppState>, edit: ExerciseEdit) -> CmdResult<i64> {
    with(&state, |db| db.create_exercise(&edit))
}

#[tauri::command]
fn exercise_stats(state: State<AppState>, id: i64) -> CmdResult<ExerciseStats> {
    with(&state, |db| db.exercise_stats(id))
}

#[tauri::command]
fn muscle_sets(state: State<AppState>, from: Option<String>, to: Option<String>) -> CmdResult<Vec<MuscleStat>> {
    with(&state, |db| db.muscle_sets(from.as_deref(), to.as_deref()))
}

#[tauri::command]
fn weekly_muscle_sets(state: State<AppState>, from: Option<String>, to: Option<String>) -> CmdResult<Vec<WeekMuscleSets>> {
    with(&state, |db| db.weekly_muscle_sets(from.as_deref(), to.as_deref()))
}

#[tauri::command]
fn last_trained(state: State<AppState>, today: String) -> CmdResult<Vec<LastTrained>> {
    let d = date(&today)?;
    with(&state, |db| db.last_trained(d))
}

#[tauri::command]
fn workouts_per_period(state: State<AppState>, monthly: bool, today: String) -> CmdResult<Vec<PeriodCount>> {
    let d = date(&today)?;
    with(&state, |db| db.workouts_per_period(monthly, d))
}

#[tauri::command]
fn streak(state: State<AppState>, today: String) -> CmdResult<Streak> {
    let d = date(&today)?;
    with(&state, |db| db.streak(d))
}

#[tauri::command]
fn workout_series(state: State<AppState>) -> CmdResult<Vec<WorkoutPoint>> {
    with(&state, |db| db.workout_series())
}

#[tauri::command]
fn discomfort_points(state: State<AppState>) -> CmdResult<Vec<DiscomfortPoint>> {
    with(&state, |db| db.discomfort_points())
}

// ---- Planning -----------------------------------------------------------------------------------

#[tauri::command]
fn list_templates(state: State<AppState>) -> CmdResult<Vec<Template>> {
    with(&state, |db| db.list_templates())
}

#[tauri::command]
fn save_template(state: State<AppState>, template: Template) -> CmdResult<i64> {
    with(&state, |db| db.save_template(&template))
}

#[tauri::command]
fn archive_template(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.archive_template(id))
}

#[tauri::command]
fn template_from_workout(state: State<AppState>, workout_id: i64, name: String) -> CmdResult<CreatedTemplate> {
    with(&state, |db| db.template_from_workout(workout_id, &name))
}

#[tauri::command]
fn active_schedule(state: State<AppState>) -> CmdResult<Option<Schedule>> {
    with(&state, |db| db.active_schedule())
}

#[tauri::command]
fn save_schedule(state: State<AppState>, schedule: Schedule) -> CmdResult<i64> {
    with(&state, |db| db.save_schedule(&schedule))
}

#[tauri::command]
fn suggestion(state: State<AppState>, today: String) -> CmdResult<Option<Suggestion>> {
    let d = date(&today)?;
    with(&state, |db| db.suggestion(d))
}

#[tauri::command]
fn projected_weekly_sets(state: State<AppState>, edited: Option<Template>) -> CmdResult<Vec<(String, f64)>> {
    with(&state, |db| db.projected_weekly_sets(edited.as_ref()))
}

#[tauri::command]
fn muscle_targets(state: State<AppState>, today: String) -> CmdResult<Vec<MuscleTarget>> {
    let d = date(&today)?;
    with(&state, |db| db.muscle_targets(d))
}

#[derive(Serialize)]
struct GoalInfo {
    goal: Option<Goal>,
    /// Default target weekly bodyweight change in % (low, high) for the goal.
    rate_pct: Option<(f64, f64)>,
    /// The user's own weekly target (kg/week), if set.
    weekly_rate_kg: Option<f64>,
    start_date: Option<String>,
}

#[tauri::command]
fn current_goal(state: State<AppState>, today: String) -> CmdResult<GoalInfo> {
    let d = date(&today)?;
    let entry = with(&state, |db| db.goal_on(d))?;
    Ok(GoalInfo {
        goal: entry.as_ref().map(|e| e.goal),
        rate_pct: entry.as_ref().map(|e| e.goal.weekly_rate_pct()),
        weekly_rate_kg: entry.as_ref().and_then(|e| e.weekly_rate_kg),
        start_date: entry.map(|e| e.start_date),
    })
}

#[tauri::command]
fn progress_health(state: State<AppState>, today: String) -> CmdResult<Vec<liftbook_core::health::MuscleHealth>> {
    let d = date(&today)?;
    with(&state, |db| db.progress_health(d))
}

#[tauri::command]
fn goal_history(state: State<AppState>) -> CmdResult<Vec<GoalEntry>> {
    with(&state, |db| db.goal_history())
}

#[tauri::command]
fn set_goal(state: State<AppState>, goal: Goal, start: String, weekly_rate_kg: Option<f64>) -> CmdResult<()> {
    let d = date(&start)?;
    with(&state, |db| db.set_goal(goal, d, weekly_rate_kg))
}

#[tauri::command]
fn set_muscle_target(state: State<AppState>, muscle_id: String, min_sets: f64, max_sets: f64) -> CmdResult<()> {
    with(&state, |db| db.set_muscle_target(&muscle_id, min_sets, max_sets))
}

#[tauri::command]
fn list_phases(state: State<AppState>) -> CmdResult<Vec<Phase>> {
    with(&state, |db| db.list_phases())
}

#[tauri::command]
fn save_phase(state: State<AppState>, phase: Phase) -> CmdResult<i64> {
    with(&state, |db| db.save_phase(&phase))
}

#[tauri::command]
fn delete_phase(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.delete_phase(id))
}

#[tauri::command]
fn propose_from_history(state: State<AppState>) -> CmdResult<HistoryProposal> {
    with(&state, |db| db.propose_from_history())
}

#[tauri::command]
fn apply_history_proposal(state: State<AppState>, proposal: HistoryProposal, start: String) -> CmdResult<Vec<(String, String)>> {
    let d = date(&start)?;
    with(&state, |db| db.apply_history_proposal(&proposal, d))
}

#[tauri::command]
fn programs() -> Vec<Program> {
    liftbook_core::planning::programs()
}

#[tauri::command]
fn install_program(state: State<AppState>, program_id: String, start: String) -> CmdResult<Vec<i64>> {
    let d = date(&start)?;
    with(&state, |db| db.install_program(&program_id, d))
}

// ---- Body -----------------------------------------------------------------------------------------

#[tauri::command]
fn weigh_ins(state: State<AppState>, include_inbody: bool) -> CmdResult<Vec<WeighInRow>> {
    with(&state, |db| db.weigh_ins(include_inbody))
}

#[tauri::command]
fn add_weigh_in(app: tauri::AppHandle, state: State<AppState>, at: String, weight_kg: f64) -> CmdResult<i64> {
    let t = strong_time(&at)?;
    let id = with(&state, |db| db.add_weigh_in(t, weight_kg))?;
    // Lets the Android weigh-in reminder skip a day that's already logged.
    native_call(&app, "weighedIn", serde_json::json!({ "date": t.date().to_string() }))?;
    Ok(id)
}

fn strong_time(s: &str) -> CmdResult<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .map_err(|_| format!("bad date/time {s:?}"))
}

#[tauri::command]
fn update_weigh_in(state: State<AppState>, id: i64, at: String, weight_kg: f64) -> CmdResult<()> {
    let t = strong_time(&at)?;
    with(&state, |db| db.update_weigh_in(id, t, weight_kg))
}

#[tauri::command]
fn update_measurement(state: State<AppState>, id: i64, at: String, value: f64) -> CmdResult<()> {
    let t = strong_time(&at)?;
    with(&state, |db| db.update_measurement(id, t, value))
}

#[tauri::command]
fn delete_weigh_in(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.delete_weigh_in(id))
}

#[derive(Serialize)]
struct BodyPart {
    id: &'static str,
    label: &'static str,
    unit: &'static str,
}

#[tauri::command]
fn body_parts() -> Vec<BodyPart> {
    BODY_PARTS.iter().map(|&(id, label, unit)| BodyPart { id, label, unit }).collect()
}

#[tauri::command]
fn measurements(state: State<AppState>, part: Option<String>) -> CmdResult<Vec<MeasurementRow>> {
    with(&state, |db| db.measurements(part.as_deref()))
}

#[tauri::command]
fn add_measurement(state: State<AppState>, at: String, part: String, value: f64) -> CmdResult<i64> {
    let t = strong_time(&at)?;
    with(&state, |db| db.add_measurement(t, &part, value))
}

#[tauri::command]
fn delete_measurement(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.delete_measurement(id))
}

#[derive(Serialize)]
struct InBodyField {
    column: &'static str,
    label: &'static str,
    unit: &'static str,
}

#[tauri::command]
fn inbody_fields() -> Vec<InBodyField> {
    INBODY_FIELDS.iter().map(|&(column, label, unit)| InBodyField { column, label, unit }).collect()
}

#[tauri::command]
fn inbody_scans(state: State<AppState>) -> CmdResult<Vec<InBodyScan>> {
    with(&state, |db| db.inbody_scans())
}

#[tauri::command]
fn save_inbody_scan(state: State<AppState>, scan: InBodyScan) -> CmdResult<i64> {
    with(&state, |db| db.save_inbody_scan(&scan))
}

#[tauri::command]
fn delete_inbody_scan(state: State<AppState>, id: i64) -> CmdResult<()> {
    with(&state, |db| db.delete_inbody_scan(id))
}

#[tauri::command]
fn preview_inbody_csv(state: State<AppState>, text: String, mapping: Option<CsvMapping>) -> CmdResult<InBodyCsvPreview> {
    with(&state, |db| db.preview_inbody_csv(&text, mapping))
}

#[tauri::command]
fn import_inbody_csv(state: State<AppState>, text: String, mapping: CsvMapping) -> CmdResult<usize> {
    with(&state, |db| db.import_inbody_csv(&text, mapping))
}

#[tauri::command]
fn diet_days(state: State<AppState>) -> CmdResult<Vec<DietDay>> {
    with(&state, |db| db.diet_days())
}

#[tauri::command]
fn save_diet_day(state: State<AppState>, day: DietDay) -> CmdResult<()> {
    with(&state, |db| db.save_diet_day(&day))
}

#[tauri::command]
fn energy_balance(state: State<AppState>, window_days: i64) -> CmdResult<Vec<EnergyPoint>> {
    with(&state, |db| db.energy_balance(window_days))
}

// ---- Export, backup and files ----------------------------------------------------------------------

/// The text of an export ("strong", "bodyweight", "inbody", "diet" or "json").
fn export_text(state: &State<AppState>, kind: &str) -> CmdResult<String> {
    with(state, |db| {
        let unit = unit_setting(db)?;
        match kind {
            "strong" => db.export_strong_csv(unit),
            "bodyweight" => db.export_bodyweight_csv(unit),
            "inbody" => db.export_inbody_csv(unit),
            "diet" => db.export_diet_csv(),
            "measurements" => db.export_measurements_csv(unit),
            "json" => Ok(serde_json::to_string_pretty(&db.export_json()?).expect("json serializes")),
            other => Err(liftbook_core::Error::Format(format!("unknown export {other:?}"))),
        }
    })
}

/// Writes an export to `path` (desktop save dialog).
#[tauri::command]
fn export_file(state: State<AppState>, kind: String, path: String) -> CmdResult<()> {
    let text = export_text(&state, &kind)?;
    std::fs::write(&path, text).map_err(|e| format!("couldn't write {path}: {e}"))
}

// ---- Sharing files on Android ---------------------------------------------------------------------
//
// Android's file pickers hand out content:// URIs, so instead of "save as" the app writes the file
// to its cache folder and opens the share sheet (Drive, Files, Messages, Instagram, ...).

fn share_dir(app: &tauri::AppHandle) -> CmdResult<std::path::PathBuf> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("share");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn share_file(app: &tauri::AppHandle, path: &std::path::Path, mime: &str, title: &str) -> CmdResult<()> {
    native_call(app, "shareFile", serde_json::json!({ "path": path.to_string_lossy(), "mime": mime, "title": title }))
}

/// Shares the file, or with `to_downloads` copies it into Downloads/Liftbook and says where.
fn share_or_save(app: &tauri::AppHandle, path: &std::path::Path, mime: &str, title: &str, to_downloads: bool) -> CmdResult<Option<String>> {
    if !to_downloads {
        return share_file(app, path, mime, title).map(|_| None);
    }
    native_call(app, "saveDownload", serde_json::json!({ "path": path.to_string_lossy(), "mime": mime, "title": "" }))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Some(format!("Download/Liftbook/{name}")))
}

fn today_stamp() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[tauri::command]
fn share_export(app: tauri::AppHandle, state: State<AppState>, kind: String, to_downloads: bool) -> CmdResult<Option<String>> {
    let text = export_text(&state, &kind)?;
    let (name, mime) = match kind.as_str() {
        "json" => ("liftbook-export.json", "application/json"),
        "strong" => ("liftbook-workouts.csv", "text/csv"),
        "bodyweight" => ("liftbook-bodyweight.csv", "text/csv"),
        "inbody" => ("liftbook-body-composition.csv", "text/csv"),
        "measurements" => ("liftbook-measurements.csv", "text/csv"),
        _ => ("liftbook-diet.csv", "text/csv"),
    };
    let path = share_dir(&app)?.join(name.replace("liftbook-", &format!("liftbook-{}-", today_stamp())));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    share_or_save(&app, &path, mime, "Export", to_downloads)
}

#[tauri::command]
fn share_backup(app: tauri::AppHandle, state: State<AppState>, to_downloads: bool) -> CmdResult<(BackupInfo, Option<String>)> {
    let path = share_dir(&app)?.join(format!("liftbook-{}.liftbook", today_stamp()));
    let info = backup_to(state, path.to_string_lossy().into_owned())?;
    let saved = share_or_save(&app, &path, "application/octet-stream", "Save backup", to_downloads)?;
    Ok((info, saved))
}

#[tauri::command]
fn share_png(app: tauri::AppHandle, name: String, data: String) -> CmdResult<()> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
    let path = share_dir(&app)?.join(format!("{safe}.png"));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    share_file(&app, &path, "image/png", "Share image")
}

#[derive(Serialize)]
struct InBodyImageRead {
    measured_at: Option<String>,
    /// Field column → value in storage units (kg, L, %, …).
    values: std::collections::BTreeMap<String, f64>,
    doubtful: Vec<String>,
    mass_unit: Option<&'static str>,
}

/// Reads an InBody 580 result sheet image (base64) with on-device OCR. Takes a few seconds, so
/// it runs off the main thread. Masses (and water, printed in the same unit) are converted to kg / L.
#[tauri::command]
async fn read_inbody_image(data: String, mass_unit: Option<String>) -> CmdResult<InBodyImageRead> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let r = tauri::async_runtime::spawn_blocking(move || liftbook_ocr::read_inbody580(&bytes))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let unit = r.mass_unit.or(match mass_unit.as_deref() {
        Some("kg") => Some("kg"),
        _ => Some("lb"),
    });
    let factor = if unit == Some("lb") { 0.45359237 } else { 1.0 };
    let values = r
        .values
        .into_iter()
        .map(|(k, v)| {
            let kind = liftbook_core::body::INBODY_FIELDS.iter().find(|f| f.0 == k).map(|f| f.2).unwrap_or("");
            let v = if kind == "kg" || kind == "l" { v * factor } else { v };
            (k, v)
        })
        .collect();
    Ok(InBodyImageRead { measured_at: r.measured_at, values, doubtful: r.doubtful, mass_unit: r.mass_unit })
}

/// Android: saves a PNG (sent as base64) to the gallery, Pictures/Liftbook.
#[tauri::command]
fn save_png_to_photos(app: tauri::AppHandle, name: String, data: String) -> CmdResult<()> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
    let path = share_dir(&app)?.join(format!("{safe}.png"));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    native_call(&app, "saveImage", serde_json::json!({ "path": path.to_string_lossy(), "mime": "image/png", "title": "" }))
}

#[derive(Serialize)]
struct StagedBackup {
    path: String,
    info: BackupInfo,
}

/// Saves a backup file the user picked (sent as base64) so it can be inspected and restored.
#[tauri::command]
fn stage_backup(app: tauri::AppHandle, data: String) -> CmdResult<StagedBackup> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let path = share_dir(&app)?.join("restore.liftbook");
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let info = Db::inspect_backup(&path).map_err(err)?;
    Ok(StagedBackup { path: path.to_string_lossy().into_owned(), info })
}

#[tauri::command]
fn backup_to(state: State<AppState>, path: String) -> CmdResult<BackupInfo> {
    with(&state, |db| {
        let info = db.backup_to(&path)?;
        db.set_setting("last_backup_at", &Local::now().naive_local().format("%Y-%m-%d %H:%M:%S").to_string())?;
        Ok(info)
    })
}

#[tauri::command]
fn inspect_backup(path: String) -> CmdResult<BackupInfo> {
    Db::inspect_backup(&path).map_err(err)
}

#[tauri::command]
fn restore_from(state: State<AppState>, path: String) -> CmdResult<BackupInfo> {
    with(&state, |db| db.restore_from(&path))
}

#[tauri::command]
fn db_info(state: State<AppState>) -> CmdResult<BackupInfo> {
    with(&state, |db| db.info())
}

/// Saves base64-encoded bytes (e.g. a PNG made in the UI) to `path`.
#[tauri::command]
fn save_base64(path: String, data: String) -> CmdResult<()> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("couldn't write {path}: {e}"))
}

/// Copies a PNG (base64) to the system clipboard as an image.
#[tauri::command]
fn copy_png(app: tauri::AppHandle, data: String) -> CmdResult<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("unexpected indexed PNG".into()),
    };
    let image = tauri::image::Image::new_owned(rgba, info.width, info.height);
    app.clipboard().write_image(&image).map_err(|e| e.to_string())
}

// ---- Android rest timer -------------------------------------------------------------------------
//
// The native side (gen/android/.../RestTimerPlugin.kt) shows an ongoing notification and sets an
// exact alarm for the end of each rest, so the alert works with the screen off or the app closed.
// On other platforms these commands do nothing; the in-app timer and desktop notification cover it.

#[cfg(target_os = "android")]
struct RestTimer(tauri::plugin::PluginHandle<tauri::Wry>);

fn rest_timer_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("resttimer")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                let handle = _api.register_android_plugin("io.github.njk639.liftbook", "RestTimerPlugin")?;
                _app.manage(RestTimer(handle));
            }
            Ok(())
        })
        .build()
}

#[allow(unused_variables)]
fn native_call(app: &tauri::AppHandle, method: &str, args: serde_json::Value) -> CmdResult<()> {
    #[cfg(target_os = "android")]
    if let Some(t) = app.try_state::<RestTimer>() {
        t.0.run_mobile_plugin::<serde_json::Value>(method, args).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Ongoing "workout in progress" notification counting up from the workout start.
#[tauri::command]
fn rest_android_status(app: tauri::AppHandle, title: String, text: String, workout_start_ms: i64) -> CmdResult<()> {
    native_call(&app, "status", serde_json::json!({ "title": title, "text": text, "whenMs": workout_start_ms, "countdown": false }))
}

/// Rest countdown in the notification plus an alarm when it ends.
#[tauri::command]
fn rest_android_rest(app: tauri::AppHandle, title: String, next: String, ends_at_ms: i64, workout_start_ms: i64) -> CmdResult<()> {
    native_call(&app, "rest", serde_json::json!({ "title": title, "next": next, "endsAt": ends_at_ms, "workoutStart": workout_start_ms }))
}

#[tauri::command]
fn rest_android_clear(app: tauri::AppHandle) -> CmdResult<()> {
    native_call(&app, "clear", serde_json::json!({}))
}

/// Android weigh-in reminder: weekdays (0 = Sunday) at hour:minute. No days turns it off.
#[tauri::command]
fn weigh_in_reminder(app: tauri::AppHandle, days: Vec<u8>, hour: u8, minute: u8) -> CmdResult<()> {
    native_call(&app, "weighInReminder", serde_json::json!({ "days": days, "hour": hour, "minute": minute }))
}

// ---- Settings ---------------------------------------------------------------------------------------

#[tauri::command]
fn counts(state: State<AppState>) -> CmdResult<Counts> {
    with(&state, |db| db.counts())
}

#[tauri::command]
fn get_setting(state: State<AppState>, key: String) -> CmdResult<Option<String>> {
    with(&state, |db| db.setting(&key))
}

#[tauri::command]
fn set_setting(state: State<AppState>, key: String, value: String) -> CmdResult<()> {
    with(&state, |db| {
        db.set_setting(&key, &value)?;
        // These change how PRs are computed.
        if matches!(key.as_str(), "e1rm_formula" | "count_warmups") {
            db.rebuild_prs()?;
        }
        Ok(())
    })
}

// ---- Android account backup ------------------------------------------------------------------------
//
// Android backs up app data to the user's Google account by itself (no sign-in, no setup) and
// restores it when the app is installed on a new phone. Backing up the live SQLite files could
// catch them mid-write, so the app keeps a consistent snapshot in `backup/` and the backup rules
// (gen/android/app/src/main/res/xml/*backup_rules.xml) include only that folder.

fn snapshot_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("backup").join("liftbook.snapshot")
}

/// On a fresh install restored by Android there is a snapshot but no database yet: use it.
fn restore_account_backup_if_fresh(dir: &std::path::Path) -> std::io::Result<()> {
    let db = dir.join("liftbook.db");
    let snap = snapshot_path(dir);
    if !db.exists() && snap.exists() {
        std::fs::copy(&snap, &db)?;
    }
    Ok(())
}

/// Writes a snapshot atomically (temp file, then rename).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn write_snapshot(db: &Db, dir: &std::path::Path) -> CmdResult<()> {
    let snap = snapshot_path(dir);
    std::fs::create_dir_all(snap.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = snap.with_extension("tmp");
    db.backup_to(&tmp).map_err(err)?;
    std::fs::rename(&tmp, &snap).map_err(|e| e.to_string())
}

/// Refreshes the snapshot within a minute of any change to the data.
#[cfg(target_os = "android")]
fn start_account_backup_snapshots(app: tauri::AppHandle, dir: std::path::PathBuf) {
    std::thread::spawn(move || {
        let mut written_at: Option<u64> = None;
        loop {
            {
                let state = app.state::<AppState>();
                if let Ok(db) = state.0.lock() {
                    let changes = db.conn().total_changes();
                    if written_at != Some(changes) || !snapshot_path(&dir).exists() {
                        if write_snapshot(&db, &dir).is_ok() {
                            written_at = Some(changes);
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_backup_snapshot_round_trip() {
        let dir = std::env::temp_dir().join(format!("liftbook-snap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = Db::open(dir.join("liftbook.db")).unwrap();
        db.add_weigh_in(NaiveDateTime::parse_from_str("2026-11-01 07:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), 80.0).unwrap();
        write_snapshot(&db, &dir).unwrap();
        drop(db);

        // A new phone: Android restored backup/ but there's no database yet.
        let phone = dir.join("new-phone");
        std::fs::create_dir_all(phone.join("backup")).unwrap();
        std::fs::copy(snapshot_path(&dir), snapshot_path(&phone)).unwrap();
        restore_account_backup_if_fresh(&phone).unwrap();
        let restored = Db::open(phone.join("liftbook.db")).unwrap();
        assert_eq!(restored.weigh_ins(false).unwrap().len(), 1);

        // An existing database is never overwritten by a snapshot.
        let mut other = Db::open(phone.join("liftbook.db")).unwrap();
        other.add_weigh_in(NaiveDateTime::parse_from_str("2026-11-02 07:00:00", "%Y-%m-%d %H:%M:%S").unwrap(), 81.0).unwrap();
        drop(other);
        restore_account_backup_if_fresh(&phone).unwrap();
        assert_eq!(Db::open(phone.join("liftbook.db")).unwrap().weigh_ins(false).unwrap().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(rest_timer_plugin())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            restore_account_backup_if_fresh(&dir)?;
            let db = Db::open(dir.join("liftbook.db"))?;
            app.manage(AppState(Mutex::new(db)));
            #[cfg(target_os = "android")]
            start_account_backup_snapshots(app.handle().clone(), dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            preview_strong_workouts, import_strong_workouts, preview_bodyweight, import_bodyweight, preview_measurements, import_measurements,
            name_review, set_name_mapping, remove_name_mapping, confirm_name_mapping, search_exercises,
            per_hand_suspects, set_session_load_mode,
            history_page, workout_detail, delete_workout, update_workout, rebuild_prs,
            active_workout, start_workout, add_exercise, remove_exercise, swap_exercise, reorder_exercises,
            add_set, add_warmup_set, set_sticky_note, remove_set, update_set, update_workout_exercise, set_rest_timer, finish_workout,
            previous_for_workout, update_template_from_workout, similar_exercises,
            muscles, list_exercises, exercise_info, update_exercise, create_exercise, exercise_stats,
            muscle_sets, weekly_muscle_sets, last_trained, workouts_per_period, streak, workout_series, discomfort_points,
            list_templates, save_template, archive_template, template_from_workout, active_schedule, save_schedule,
            suggestion, projected_weekly_sets, muscle_targets, set_muscle_target, current_goal, set_goal, goal_history, progress_health, list_phases, save_phase, delete_phase,
            propose_from_history, apply_history_proposal, programs, install_program,
            weigh_ins, add_weigh_in, update_weigh_in, update_measurement, delete_weigh_in, inbody_fields, inbody_scans, save_inbody_scan, delete_inbody_scan,
            preview_inbody_csv, import_inbody_csv, diet_days, save_diet_day, energy_balance,
            body_parts, measurements, add_measurement, delete_measurement,
            export_file, backup_to, inspect_backup, restore_from, db_info, save_base64, copy_png,
            rest_android_status, rest_android_rest, rest_android_clear, weigh_in_reminder,
            share_export, share_backup, share_png, save_png_to_photos, read_inbody_image, stage_backup,
            counts, get_setting, set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
