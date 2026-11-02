// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
// Types mirror the Rust structs in crates/core (serde field names).

import { invoke } from "@tauri-apps/api/core";

export type WeightUnit = "kg" | "lb";
export type DistanceUnit = "m" | "km" | "ft" | "mi";

export interface WorkoutPreview {
  workouts: number;
  sets: number;
  exercise_names: number;
  first: string | null;
  last: string | null;
  already_imported: number;
  weight_unit: WeightUnit | null;
  warnings: string[];
}

export interface ImportOutcome {
  inserted: number;
  skipped: number;
  inserted_sets: number;
}

export interface WorkoutImportResult {
  outcome: ImportOutcome;
  auto_mapped: number;
  needs_review: number;
}

export interface MeasurementPreview {
  entries: number;
  parts: string[];
  first: string | null;
  last: string | null;
  warnings: string[];
}

export interface BodyweightPreview {
  entries: number;
  first: string | null;
  last: string | null;
  weight_unit: WeightUnit;
  warnings: string[];
}

export interface NoteEra {
  note: string | null;
  from: string;
  to: string;
  sessions: number;
}

export interface MappingRule {
  id: number;
  from_date: string | null;
  to_date: string | null;
  exercise_slug: string;
  exercise_name: string;
  confidence: number | null;
  confirmed: boolean;
}

export interface SuggestionView {
  slug: string;
  name: string;
  equipment: string;
  confidence: number;
}

export interface NameReview {
  source_name: string;
  sessions: number;
  sets: number;
  first_date: string;
  last_date: string;
  note_eras: NoteEra[];
  rules: MappingRule[];
  suggestions: SuggestionView[];
  unmapped_sessions: number;
  needs_review: boolean;
}

export interface BestSet {
  weight_kg: number | null;
  reps: number | null;
  distance_m: number | null;
  duration_s: number | null;
}

export interface CardExercise {
  name: string;
  superset: number | null;
  sets: number;
  best: BestSet | null;
}

export interface WorkoutCard {
  id: number;
  name: string;
  started_at_local: string;
  duration_s: number | null;
  sets: number;
  volume_kg: number;
  prs: number;
  exercises: CardExercise[];
}

export interface SetView {
  id: number;
  set_type: "normal" | "warmup" | "drop" | "failure";
  weight_kg: number | null;
  reps: number | null;
  rpe: number | null;
  distance_m: number | null;
  duration_s: number | null;
  rest_s: number | null;
  completed: boolean;
}

export interface ExerciseView {
  id: number;
  exercise_id: number | null;
  load_mode_override: "total" | "per_hand" | null;
  kind: string | null;
  next_intent: "up" | "hold" | "down" | null;
  elbow: number | null;
  knee: number | null;
  shoulder: number | null;
  target_sets: number | null;
  rep_min: number | null;
  rep_max: number | null;
  name: string;
  source_name: string;
  exercise_slug: string | null;
  equipment: string | null;
  load_mode: "total" | "per_hand";
  superset: number | null;
  notes: string | null;
  rest_s: number | null;
  sets: SetView[];
}

export interface WorkoutDetail {
  id: number;
  finished: boolean;
  template_id: number | null;
  rest_ends_at: string | null;
  rest_total_s: number | null;
  prs: PrView[];
  muscle_sets: [string, number][];
  name: string;
  started_at_local: string;
  tz_name: string | null;
  duration_s: number | null;
  notes: string | null;
  volume_kg: number;
  exercises: ExerciseView[];
}

export interface Counts {
  workouts: number;
  sets: number;
  exercise_names: number;
  weigh_ins: number;
}

export interface PrView {
  kind: "e1rm" | "weight" | "volume" | "rep_max" | "reps";
  reps: number | null;
  value: number;
  previous: number | null;
  achieved_at: string;
  workout_id: number;
  exercise_id: number;
  exercise_name: string;
}

export interface PerHandSuspect {
  workout_exercise_id: number;
  workout_id: number;
  exercise_name: string;
  date: string;
  max_weight_kg: number;
  neighbor_median_kg: number;
}

export interface Muscle {
  id: string;
  name: string;
  group: string;
}
export interface MuscleGroup {
  id: string;
  name: string;
}

export interface ExerciseInfo {
  id: number;
  slug: string;
  name: string;
  aliases: string[];
  equipment: string | null;
  pattern: string | null;
  kind: string;
  load_mode: "total" | "per_hand";
  default_rest_s: number | null;
  setup_note: string | null;
  primary: string[];
  secondary: string[];
  source: string;
  is_custom: boolean;
  user_modified: boolean;
  sessions: number;
  last_done: string | null;
}

export type ExerciseEdit = Pick<
  ExerciseInfo,
  "name" | "equipment" | "pattern" | "kind" | "load_mode" | "default_rest_s" | "setup_note" | "primary" | "secondary"
>;

export interface SetBrief {
  set_type: string;
  weight_kg: number | null;
  reps: number | null;
  distance_m: number | null;
  duration_s: number | null;
  e1rm: number | null;
}

export interface ExerciseSession {
  workout_id: number;
  workout_name: string;
  at: string;
  sets: SetBrief[];
  best_e1rm: number | null;
  max_weight: number | null;
  volume_kg: number;
  total_reps: number;
  notes: string | null;
  prs: string[];
}

export interface RecordView {
  value: number;
  weight_kg: number | null;
  reps: number | null;
  at: string;
}

export interface RepMax {
  reps: number;
  weight_kg: number;
  at: string;
}

export interface ExerciseStats {
  exercise_id: number;
  name: string;
  sessions: ExerciseSession[];
  best_e1rm: RecordView | null;
  heaviest: RecordView | null;
  best_volume: RecordView | null;
  most_reps: RecordView | null;
  total_volume_kg: number;
  rep_maxes: RepMax[];
  reps_at_weight: RepMax[];
  prs: PrView[];
}

export interface MuscleStat {
  muscle_id: string;
  sets: number;
  volume_kg: number;
  exercises: [string, number][];
}

export interface WeekMuscleSets {
  week_start: string;
  muscle_id: string;
  sets: number;
  volume_kg: number;
}

export interface LastTrained {
  muscle_id: string;
  last_date: string | null;
  days_ago: number | null;
}

export interface PeriodCount {
  start: string;
  workouts: number;
}

export interface Streak {
  current_weeks: number;
  longest_weeks: number;
}

export interface WorkoutPoint {
  workout_id: number;
  name: string;
  at: string;
  duration_s: number | null;
  volume_kg: number;
  sets: number;
}

export interface DiscomfortPoint {
  at: string;
  exercise_id: number;
  exercise_name: string;
  elbow: number | null;
  knee: number | null;
  shoulder: number | null;
  max_weight_kg: number | null;
  volume_kg: number;
}

export interface PrevSet {
  set_type: string;
  weight_kg: number | null;
  reps: number | null;
  distance_m: number | null;
  duration_s: number | null;
}

export interface Previous {
  workout_exercise_id: number;
  at: string | null;
  sets: PrevSet[];
  notes: string | null;
  next_intent: "up" | "hold" | "down" | null;
  suggest_increase: boolean;
  setup_note: string | null;
  primary: string[];
  secondary: string[];
}

export type StartFrom = { kind: "empty" } | { kind: "template"; template_id: number } | { kind: "repeat"; workout_id: number };

export interface SetUpdate {
  set_type?: string;
  weight_kg?: number | null;
  reps?: number | null;
  distance_m?: number | null;
  duration_s?: number | null;
  rpe?: number | null;
  completed?: boolean;
}

export interface ExerciseUpdate {
  notes?: string | null;
  rest_s?: number | null;
  next_intent?: "up" | "hold" | "down" | null;
  elbow?: number | null;
  knee?: number | null;
  shoulder?: number | null;
  load_mode?: "total" | "per_hand" | null;
}

export interface WorkoutUpdate {
  name?: string;
  notes?: string | null;
  started_at_local?: string;
  duration_s?: number | null;
}

export interface TemplateExercise {
  exercise_id: number;
  name: string;
  superset: number | null;
  target_sets: number | null;
  rep_min: number | null;
  rep_max: number | null;
  rest_s: number | null;
  setup_note: string | null;
}

export interface Template {
  id: number | null;
  name: string;
  notes: string | null;
  color: string | null;
  exercises: TemplateExercise[];
  last_done?: string | null;
}

export interface Schedule {
  id: number | null;
  name: string;
  kind: "weekly" | "cycle";
  start_date: string;
  cycle_length: number;
  days: (number | null)[];
}

export interface Suggested {
  template_id: number;
  reason: "today" | "missed" | "upcoming";
  planned_date: string;
}

export interface Suggestion {
  is_rest_day: boolean;
  done_today: boolean;
  items: Suggested[];
  next: Suggested | null;
}

export interface MuscleTarget {
  muscle_id: string;
  min_sets: number;
  max_sets: number;
}

export interface Phase {
  id: number | null;
  name: string;
  lane: "program" | "nutrition";
  start_date: string;
  end_date: string | null;
  color: string | null;
  notes: string | null;
}

export interface ProposedPhase {
  name: string;
  start_date: string;
  end_date: string | null;
  workout_names: string[];
  usual_weekday: [string, number][];
}

export interface Program {
  id: string;
  name: string;
  description: string;
  days: (string | null)[];
  templates: Record<string, unknown[]>;
}

export interface WeighInRow {
  id: number;
  at: string;
  weight_kg: number;
  source: "manual" | "strong" | "inbody";
}

export interface InBodyField {
  column: string;
  label: string;
  unit: string;
}

export interface InBodyScan {
  id: number | null;
  measured_at_local: string;
  values: Record<string, number>;
  device: string | null;
  notes: string | null;
}

export interface CsvMapping {
  date_column: string;
  columns: Record<string, string>;
  mass_unit: WeightUnit;
}

export interface InBodyCsvPreview {
  headers: string[];
  mapping: CsvMapping;
  remembered: boolean;
  scans: InBodyScan[];
  errors: string[];
}

export interface DietDay {
  date: string;
  calories: number | null;
  protein_g: number | null;
  notes: string | null;
}

export interface EnergyPoint {
  date: string;
  trend_kg: number;
  balance_kcal: number | null;
  avg_intake_kcal: number | null;
  tdee_kcal: number | null;
}

export type Goal = "cut" | "maintain" | "gain";

export interface MuscleHealth {
  muscle_id: string;
  /** -1 (going badly) … 1 (going well); null without data. */
  score: number | null;
  signals: { kind: "volume" | "strength" | "size" | "lean_mass"; score: number; detail: string }[];
}

function localToday(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export interface BodyPart {
  id: string;
  label: string;
  unit: "cm" | "pct";
}

export interface MeasurementRow {
  id: number;
  at: string;
  part: string;
  value: number;
}

export interface BackupInfo {
  schema_version: number;
  workouts: number;
  last_workout: string | null;
  weigh_ins: number;
  inbody_scans: number;
}

/** Error code returned when a file has no unit information. */
export const MISSING_WEIGHT_UNIT = "MISSING_WEIGHT_UNIT";

export const api = {
  previewStrongWorkouts: (text: string, weightUnit?: WeightUnit) => invoke<WorkoutPreview>("preview_strong_workouts", { text, weightUnit }),
  importStrongWorkouts: (text: string, fileName: string, weightUnit?: WeightUnit) =>
    invoke<WorkoutImportResult>("import_strong_workouts", { text, fileName, weightUnit }),
  previewBodyweight: (text: string, weightUnit?: WeightUnit) => invoke<BodyweightPreview>("preview_bodyweight", { text, weightUnit }),
  importBodyweight: (text: string, fileName: string, weightUnit?: WeightUnit) =>
    invoke<ImportOutcome>("import_bodyweight", { text, fileName, weightUnit }),
  previewMeasurements: (text: string) => invoke<MeasurementPreview>("preview_measurements", { text }),
  importMeasurements: (text: string, fileName: string) => invoke<ImportOutcome>("import_measurements", { text, fileName }),
  nameReview: () => invoke<NameReview[]>("name_review"),
  setNameMapping: (sourceName: string, slug: string, fromDate?: string, toDate?: string) =>
    invoke<void>("set_name_mapping", { sourceName, slug, fromDate, toDate }),
  removeNameMapping: (ruleId: number) => invoke<void>("remove_name_mapping", { ruleId }),
  confirmNameMapping: (ruleId: number) => invoke<void>("confirm_name_mapping", { ruleId }),
  searchExercises: (query: string) => invoke<SuggestionView[]>("search_exercises", { query }),
  historyPage: (limit: number, before?: { time: string; id: number }) =>
    invoke<WorkoutCard[]>("history_page", { limit, beforeTime: before?.time, beforeId: before?.id }),
  workoutDetail: (id: number) => invoke<WorkoutDetail | null>("workout_detail", { id }),
  counts: () => invoke<Counts>("counts"),
  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  /** Android only (no-op elsewhere): weigh-in reminder on weekdays 0 = Sunday … 6 = Saturday. */
  setOrientation: (landscape: boolean) => invoke<void>("set_orientation", { landscape }).catch(() => {}),
  weighInReminder: (days: number[], hour: number, minute: number) => invoke<void>("weigh_in_reminder", { days, hour, minute }),

  perHandSuspects: () => invoke<PerHandSuspect[]>("per_hand_suspects"),
  setSessionLoadMode: (workoutExerciseId: number, mode: "total" | "per_hand" | null) =>
    invoke<void>("set_session_load_mode", { workoutExerciseId, mode }),
  deleteWorkout: (id: number) => invoke<void>("delete_workout", { id }),
  updateWorkout: (id: number, update: WorkoutUpdate) => invoke<void>("update_workout", { id, update }),
  rebuildPrs: () => invoke<number>("rebuild_prs"),

  activeWorkout: () => invoke<number | null>("active_workout"),
  startWorkout: (from: StartFrom) => invoke<number>("start_workout", { from }),
  addExercise: (workoutId: number, exerciseId: number) => invoke<number>("add_exercise", { workoutId, exerciseId }),
  removeExercise: (workoutExerciseId: number) => invoke<void>("remove_exercise", { workoutExerciseId }),
  swapExercise: (workoutExerciseId: number, exerciseId: number) => invoke<void>("swap_exercise", { workoutExerciseId, exerciseId }),
  reorderExercises: (workoutId: number, order: { workout_exercise_id: number; superset: number | null }[]) =>
    invoke<void>("reorder_exercises", { workoutId, order }),
  addSet: (workoutExerciseId: number) => invoke<number>("add_set", { workoutExerciseId }),
  removeSet: (setId: number) => invoke<void>("remove_set", { setId }),
  addWarmupSet: (workoutExerciseId: number) => invoke<number>("add_warmup_set", { workoutExerciseId }),
  setStickyNote: (exerciseId: number, note: string | null) => invoke<void>("set_sticky_note", { exerciseId, note }),
  updateSet: (setId: number, update: SetUpdate) => invoke<void>("update_set", { setId, update }),
  updateWorkoutExercise: (workoutExerciseId: number, update: ExerciseUpdate) =>
    invoke<void>("update_workout_exercise", { workoutExerciseId, update }),
  setRestTimer: (workoutId: number, endsAt: string | null, totalS: number | null) =>
    invoke<void>("set_rest_timer", { workoutId, endsAt, totalS }),
  finishWorkout: (workoutId: number) => invoke<{ differs_from_template: boolean }>("finish_workout", { workoutId }),
  previousForWorkout: (workoutId: number) => invoke<Previous[]>("previous_for_workout", { workoutId }),
  updateTemplateFromWorkout: (workoutId: number) => invoke<void>("update_template_from_workout", { workoutId }),
  similarExercises: (exerciseId: number) => invoke<ExerciseInfo[]>("similar_exercises", { exerciseId }),

  muscles: () => invoke<{ groups: MuscleGroup[]; muscles: Muscle[] }>("muscles"),
  listExercises: (onlyUsed: boolean) => invoke<ExerciseInfo[]>("list_exercises", { onlyUsed }),
  exerciseInfo: (id: number) => invoke<ExerciseInfo | null>("exercise_info", { id }),
  updateExercise: (id: number, edit: ExerciseEdit) => invoke<void>("update_exercise", { id, edit }),
  createExercise: (edit: ExerciseEdit) => invoke<number>("create_exercise", { edit }),
  exerciseStats: (id: number) => invoke<ExerciseStats>("exercise_stats", { id }),
  muscleSets: (from?: string, to?: string) => invoke<MuscleStat[]>("muscle_sets", { from, to }),
  weeklyMuscleSets: (from?: string, to?: string) => invoke<WeekMuscleSets[]>("weekly_muscle_sets", { from, to }),
  lastTrained: (today: string) => invoke<LastTrained[]>("last_trained", { today }),
  workoutsPerPeriod: (monthly: boolean, today: string) => invoke<PeriodCount[]>("workouts_per_period", { monthly, today }),
  streak: (today: string) => invoke<Streak>("streak", { today }),
  workoutSeries: () => invoke<WorkoutPoint[]>("workout_series"),
  discomfortPoints: () => invoke<DiscomfortPoint[]>("discomfort_points"),

  listTemplates: () => invoke<Template[]>("list_templates"),
  saveTemplate: (template: Template) => invoke<number>("save_template", { template }),
  archiveTemplate: (id: number) => invoke<void>("archive_template", { id }),
  templateFromWorkout: (workoutId: number, name: string) =>
    invoke<{ template_id: number; uncertain_notes: [string, string][] }>("template_from_workout", { workoutId, name }),
  activeSchedule: () => invoke<Schedule | null>("active_schedule"),
  saveSchedule: (schedule: Schedule) => invoke<number>("save_schedule", { schedule }),
  suggestion: (today: string) => invoke<Suggestion | null>("suggestion", { today }),
  projectedWeeklySets: (edited?: Template) => invoke<[string, number][]>("projected_weekly_sets", { edited }),
  muscleTargets: () => invoke<MuscleTarget[]>("muscle_targets", { today: localToday() }),
  currentGoal: () =>
    invoke<{ goal: Goal | null; rate_pct: [number, number] | null; weekly_rate_kg: number | null; start_date: string | null }>(
      "current_goal",
      {
        today: localToday(),
      },
    ),
  progressHealth: () => invoke<MuscleHealth[]>("progress_health", { today: localToday() }),
  goalHistory: () =>
    invoke<{ phase_id: number; goal: Goal; start_date: string; end_date: string | null; weekly_rate_kg: number | null }[]>("goal_history"),
  setGoal: (goal: Goal, start?: string, weeklyRateKg?: number | null) =>
    invoke<void>("set_goal", { goal, start: start ?? localToday(), weeklyRateKg: weeklyRateKg ?? null }),
  setMuscleTarget: (muscleId: string, minSets: number, maxSets: number) =>
    invoke<void>("set_muscle_target", { muscleId, minSets, maxSets }),
  listPhases: () => invoke<Phase[]>("list_phases"),
  savePhase: (phase: Phase) => invoke<number>("save_phase", { phase }),
  deletePhase: (id: number) => invoke<void>("delete_phase", { id }),
  proposeFromHistory: () => invoke<{ phases: ProposedPhase[] }>("propose_from_history"),
  applyHistoryProposal: (proposal: { phases: ProposedPhase[] }, start: string) =>
    invoke<[string, string][]>("apply_history_proposal", { proposal, start }),
  programs: () => invoke<Program[]>("programs"),
  installProgram: (programId: string, start: string) => invoke<number[]>("install_program", { programId, start }),

  weighIns: (includeInbody: boolean) => invoke<WeighInRow[]>("weigh_ins", { includeInbody }),
  addWeighIn: (at: string, weightKg: number) => invoke<number>("add_weigh_in", { at, weightKg }),
  deleteWeighIn: (id: number) => invoke<void>("delete_weigh_in", { id }),
  updateWeighIn: (id: number, at: string, weightKg: number) => invoke<void>("update_weigh_in", { id, at, weightKg }),
  updateMeasurement: (id: number, at: string, value: number) => invoke<void>("update_measurement", { id, at, value }),
  inbodyFields: () => invoke<InBodyField[]>("inbody_fields"),
  inbodyScans: () => invoke<InBodyScan[]>("inbody_scans"),
  /** On-device OCR of an InBody 580 result sheet image (base64); values in storage units. */
  readInbodyImage: (data: string, massUnit: "kg" | "lb") =>
    invoke<{ measured_at: string | null; values: Record<string, number>; doubtful: string[]; mass_unit: "kg" | "lb" | null }>(
      "read_inbody_image",
      { data, massUnit },
    ),
  saveInbodyScan: (scan: InBodyScan) => invoke<number>("save_inbody_scan", { scan }),
  deleteInbodyScan: (id: number) => invoke<void>("delete_inbody_scan", { id }),
  previewInbodyCsv: (text: string, mapping?: CsvMapping) => invoke<InBodyCsvPreview>("preview_inbody_csv", { text, mapping }),
  importInbodyCsv: (text: string, mapping: CsvMapping) => invoke<number>("import_inbody_csv", { text, mapping }),
  dietDays: () => invoke<DietDay[]>("diet_days"),
  saveDietDay: (day: DietDay) => invoke<void>("save_diet_day", { day }),
  energyBalance: (windowDays: number) => invoke<EnergyPoint[]>("energy_balance", { windowDays }),

  bodyParts: () => invoke<BodyPart[]>("body_parts"),
  measurements: (part?: string) => invoke<MeasurementRow[]>("measurements", { part }),
  addMeasurement: (at: string, part: string, value: number) => invoke<number>("add_measurement", { at, part, value }),
  deleteMeasurement: (id: number) => invoke<void>("delete_measurement", { id }),

  exportFile: (kind: "strong" | "bodyweight" | "inbody" | "diet" | "measurements" | "json", path: string) =>
    invoke<void>("export_file", { kind, path }),
  backupTo: (path: string) => invoke<BackupInfo>("backup_to", { path }),
  inspectBackup: (path: string) => invoke<BackupInfo>("inspect_backup", { path }),
  restoreFrom: (path: string) => invoke<BackupInfo>("restore_from", { path }),
  dbInfo: () => invoke<BackupInfo>("db_info"),
  saveBase64: (path: string, data: string) => invoke<void>("save_base64", { path, data }),
  copyPng: (data: string) => invoke<void>("copy_png", { data }),
  shareExport: (kind: "strong" | "bodyweight" | "inbody" | "diet" | "measurements" | "json") => invoke<void>("share_export", { kind }),
  shareBackup: () => invoke<BackupInfo>("share_backup"),
  sharePng: (name: string, data: string) => invoke<void>("share_png", { name, data }),
  savePngToPhotos: (name: string, data: string) => invoke<void>("save_png_to_photos", { name, data }),
  stageBackup: (data: string) => invoke<{ path: string; info: BackupInfo }>("stage_backup", { data }),
};
