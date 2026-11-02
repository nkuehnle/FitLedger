//! SQLite storage. Weights are kg, distances metres, local times as `YYYY-MM-DD HH:MM:SS`.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::Result;
use crate::strong::{BodyweightImport, StrongImport};

const TIME_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// Each entry is applied once, in order; `PRAGMA user_version` records how many have run.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE muscle_group (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    position INTEGER NOT NULL
);

CREATE TABLE muscle (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    group_id TEXT NOT NULL REFERENCES muscle_group (id),
    position INTEGER NOT NULL
);

CREATE TABLE exercise (
    id             INTEGER PRIMARY KEY,
    slug           TEXT NOT NULL UNIQUE,
    name           TEXT NOT NULL,
    equipment      TEXT,
    pattern        TEXT,
    kind           TEXT NOT NULL DEFAULT 'weight_reps',
    load_mode      TEXT NOT NULL DEFAULT 'total' CHECK (load_mode IN ('total', 'per_hand')),
    default_rest_s INTEGER,
    setup_note     TEXT,
    is_custom      INTEGER NOT NULL DEFAULT 0,
    hidden         INTEGER NOT NULL DEFAULT 0,
    -- 'liftbook', 'free-exercise-db' or 'custom'
    source         TEXT NOT NULL DEFAULT 'custom',
    -- set once the user edits a built-in exercise; library updates then leave it alone
    user_modified  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE exercise_alias (
    exercise_id INTEGER NOT NULL REFERENCES exercise (id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    PRIMARY KEY (exercise_id, alias)
);

CREATE TABLE exercise_muscle (
    exercise_id INTEGER NOT NULL REFERENCES exercise (id) ON DELETE CASCADE,
    muscle_id   TEXT NOT NULL REFERENCES muscle (id),
    role        TEXT NOT NULL CHECK (role IN ('primary', 'secondary')),
    PRIMARY KEY (exercise_id, muscle_id)
);

-- Maps an imported exercise name to a library exercise, optionally only for a date range
-- (inclusive local dates). Ranged rules take precedence over whole-history ones.
CREATE TABLE name_mapping (
    id          INTEGER PRIMARY KEY,
    source_name TEXT NOT NULL,
    from_date   TEXT,
    to_date     TEXT,
    exercise_id INTEGER NOT NULL REFERENCES exercise (id),
    confidence  REAL,
    confirmed   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX name_mapping_name ON name_mapping (source_name);

CREATE TABLE workout (
    id               INTEGER PRIMARY KEY,
    source_key       TEXT UNIQUE,
    name             TEXT NOT NULL,
    started_at_local TEXT NOT NULL,
    utc_offset_min   INTEGER,
    tz_name          TEXT,
    duration_s       INTEGER,
    notes            TEXT,
    created_at       TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX workout_started ON workout (started_at_local);

CREATE TABLE workout_exercise (
    id          INTEGER PRIMARY KEY,
    workout_id  INTEGER NOT NULL REFERENCES workout (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    superset    INTEGER,
    source_name TEXT NOT NULL,
    exercise_id INTEGER REFERENCES exercise (id),
    notes       TEXT,
    rest_s      INTEGER,
    -- per-session override of exercise.load_mode
    load_mode   TEXT CHECK (load_mode IN ('total', 'per_hand'))
);
CREATE INDEX workout_exercise_workout ON workout_exercise (workout_id);
CREATE INDEX workout_exercise_exercise ON workout_exercise (exercise_id);

CREATE TABLE workout_set (
    id                  INTEGER PRIMARY KEY,
    workout_exercise_id INTEGER NOT NULL REFERENCES workout_exercise (id) ON DELETE CASCADE,
    position            INTEGER NOT NULL,
    set_type            TEXT NOT NULL CHECK (set_type IN ('normal', 'warmup', 'drop', 'failure')),
    weight_kg           REAL,
    reps                INTEGER,
    rpe                 REAL,
    distance_m          REAL,
    duration_s          REAL,
    rest_s              INTEGER
);
CREATE INDEX workout_set_exercise ON workout_set (workout_exercise_id);

CREATE TABLE bodyweight (
    id                INTEGER PRIMARY KEY,
    measured_at_local TEXT NOT NULL,
    weight_kg         REAL NOT NULL,
    source            TEXT NOT NULL,
    UNIQUE (source, measured_at_local)
);

CREATE TABLE import_batch (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,
    file_name   TEXT,
    imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    rows        INTEGER NOT NULL,
    inserted    INTEGER NOT NULL,
    skipped     INTEGER NOT NULL
);
"#, r#"
-- Migration 2: live workouts, PRs, templates, schedule, phases, muscle targets.

CREATE TABLE template (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    notes      TEXT,
    color      TEXT,
    archived   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE template_exercise (
    id          INTEGER PRIMARY KEY,
    template_id INTEGER NOT NULL REFERENCES template (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    superset    INTEGER,
    exercise_id INTEGER NOT NULL REFERENCES exercise (id),
    target_sets INTEGER,
    rep_min     INTEGER,
    rep_max     INTEGER,
    rest_s      INTEGER,
    setup_note  TEXT
);
CREATE INDEX template_exercise_template ON template_exercise (template_id);

ALTER TABLE workout ADD COLUMN finished INTEGER NOT NULL DEFAULT 1;
ALTER TABLE workout ADD COLUMN template_id INTEGER REFERENCES template (id) ON DELETE SET NULL;
-- live rest timer, so it survives the app being closed
ALTER TABLE workout ADD COLUMN rest_ends_at TEXT;
ALTER TABLE workout ADD COLUMN rest_total_s INTEGER;

ALTER TABLE workout_exercise ADD COLUMN next_intent TEXT CHECK (next_intent IN ('up', 'hold', 'down'));
ALTER TABLE workout_exercise ADD COLUMN elbow INTEGER CHECK (elbow BETWEEN 1 AND 5);
ALTER TABLE workout_exercise ADD COLUMN knee INTEGER CHECK (knee BETWEEN 1 AND 5);
ALTER TABLE workout_exercise ADD COLUMN shoulder INTEGER CHECK (shoulder BETWEEN 1 AND 5);
ALTER TABLE workout_exercise ADD COLUMN target_sets INTEGER;
ALTER TABLE workout_exercise ADD COLUMN rep_min INTEGER;
ALTER TABLE workout_exercise ADD COLUMN rep_max INTEGER;

ALTER TABLE workout_set ADD COLUMN completed INTEGER NOT NULL DEFAULT 1;

-- Derived; rebuilt from history by rebuild_prs().
CREATE TABLE pr_event (
    id                  INTEGER PRIMARY KEY,
    workout_id          INTEGER NOT NULL REFERENCES workout (id) ON DELETE CASCADE,
    workout_exercise_id INTEGER NOT NULL REFERENCES workout_exercise (id) ON DELETE CASCADE,
    set_id              INTEGER REFERENCES workout_set (id) ON DELETE CASCADE,
    exercise_id         INTEGER NOT NULL REFERENCES exercise (id),
    kind                TEXT NOT NULL,
    reps                INTEGER,
    value               REAL NOT NULL,
    previous            REAL,
    achieved_at         TEXT NOT NULL
);
CREATE INDEX pr_event_workout ON pr_event (workout_id);
CREATE INDEX pr_event_exercise ON pr_event (exercise_id);

CREATE TABLE schedule (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('weekly', 'cycle')),
    start_date   TEXT NOT NULL,
    cycle_length INTEGER NOT NULL DEFAULT 7,
    active       INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE schedule_slot (
    schedule_id INTEGER NOT NULL REFERENCES schedule (id) ON DELETE CASCADE,
    day_index   INTEGER NOT NULL,
    template_id INTEGER REFERENCES template (id) ON DELETE CASCADE,
    PRIMARY KEY (schedule_id, day_index)
);

CREATE TABLE phase (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    lane       TEXT NOT NULL CHECK (lane IN ('program', 'nutrition')),
    start_date TEXT NOT NULL,
    end_date   TEXT,
    color      TEXT,
    notes      TEXT
);

CREATE TABLE muscle_target (
    muscle_id TEXT PRIMARY KEY REFERENCES muscle (id),
    min_sets  REAL NOT NULL,
    max_sets  REAL NOT NULL
);
"#, r#"
-- Migration 3: body composition, diet, remembered CSV mappings.

CREATE TABLE inbody_scan (
    id                  INTEGER PRIMARY KEY,
    measured_at_local   TEXT NOT NULL,
    weight_kg           REAL,
    smm_kg              REAL,
    body_fat_mass_kg    REAL,
    pbf                 REAL,
    lean_body_mass_kg   REAL,
    fat_free_mass_kg    REAL,
    total_body_water_l  REAL,
    icw_l               REAL,
    ecw_l               REAL,
    ecw_tbw             REAL,
    protein_kg          REAL,
    minerals_kg         REAL,
    bmi                 REAL,
    bmr_kcal            REAL,
    visceral_fat_level  REAL,
    inbody_score        REAL,
    lean_right_arm_kg   REAL,
    lean_left_arm_kg    REAL,
    lean_trunk_kg       REAL,
    lean_right_leg_kg   REAL,
    lean_left_leg_kg    REAL,
    lean_right_arm_pct  REAL,
    lean_left_arm_pct   REAL,
    lean_trunk_pct      REAL,
    lean_right_leg_pct  REAL,
    lean_left_leg_pct   REAL,
    fat_right_arm_kg    REAL,
    fat_left_arm_kg     REAL,
    fat_trunk_kg        REAL,
    fat_right_leg_kg    REAL,
    fat_left_leg_kg     REAL,
    fat_right_arm_pct   REAL,
    fat_left_arm_pct    REAL,
    fat_trunk_pct       REAL,
    fat_right_leg_pct   REAL,
    fat_left_leg_pct    REAL,
    device              TEXT,
    notes               TEXT,
    UNIQUE (measured_at_local)
);

CREATE TABLE diet_day (
    date      TEXT PRIMARY KEY,
    calories  REAL,
    protein_g REAL,
    notes     TEXT
);

CREATE TABLE csv_mapping (
    kind      TEXT NOT NULL,
    signature TEXT NOT NULL,
    mapping   TEXT NOT NULL,
    PRIMARY KEY (kind, signature)
);
"#, r#"
-- Migration 4: body-part measurements (tape measure). Lengths in cm.
CREATE TABLE body_measurement (
    id                INTEGER PRIMARY KEY,
    measured_at_local TEXT NOT NULL,
    part              TEXT NOT NULL,
    value             REAL NOT NULL
);
CREATE INDEX body_measurement_part ON body_measurement (part, measured_at_local);
"#];

pub(crate) const MIGRATION_COUNT: usize = MIGRATIONS.len();

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ImportOutcome {
    pub inserted: usize,
    /// Already present from an earlier import.
    pub skipped: usize,
    pub inserted_sets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub workouts: usize,
    pub sets: usize,
    pub exercise_names: usize,
    pub weigh_ins: usize,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut db = Self { conn };
        db.migrate()?;
        db.seed_library()?;
        db.refresh_derived()?;
        Ok(db)
    }

    /// Rebuilds derived data (PRs) when the rules that produce it have changed since it was built.
    fn refresh_derived(&mut self) -> Result<()> {
        let v = crate::stats::PR_RULES_VERSION.to_string();
        if self.setting("pr_rules_version")?.as_deref() != Some(v.as_str()) {
            self.rebuild_prs()?;
            self.set_setting("pr_rules_version", &v)?;
        }
        Ok(())
    }

    /// Brings a freshly restored database up to date.
    pub(crate) fn reinit(&mut self) -> Result<()> {
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        self.migrate()?;
        self.seed_library()?;
        self.rebuild_prs()?;
        self.set_setting("pr_rules_version", &crate::stats::PR_RULES_VERSION.to_string())?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// How many workouts in `import` are already in the database.
    pub fn count_existing_workouts(&self, import: &StrongImport) -> Result<usize> {
        let mut stmt = self.conn.prepare_cached("SELECT 1 FROM workout WHERE source_key = ?1")?;
        let mut n = 0;
        for w in &import.workouts {
            if stmt.query_row([&w.source_key], |_| Ok(())).optional()?.is_some() {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Inserts workouts not already present (matched by source key). Existing ones are left
    /// untouched, so edits made in the app survive a re-import.
    pub fn import_strong_workouts(&mut self, import: &StrongImport, file_name: &str) -> Result<ImportOutcome> {
        let tx = self.conn.transaction()?;
        let mut out = ImportOutcome { inserted: 0, skipped: 0, inserted_sets: 0 };
        {
            let mut ins_workout = tx.prepare(
                "INSERT OR IGNORE INTO workout (source_key, name, started_at_local, utc_offset_min, tz_name, duration_s, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            let mut ins_exercise = tx.prepare(
                "INSERT INTO workout_exercise (workout_id, position, superset, source_name, notes, rest_s)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut ins_set = tx.prepare(
                "INSERT INTO workout_set (workout_exercise_id, position, set_type, weight_kg, reps, rpe, distance_m, duration_s, rest_s)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for w in &import.workouts {
                let changed = ins_workout.execute(params![
                    w.source_key,
                    w.name,
                    w.started_at_local.format(TIME_FMT).to_string(),
                    w.utc_offset_min,
                    w.tz_name,
                    w.duration_s,
                    w.notes,
                ])?;
                if changed == 0 {
                    out.skipped += 1;
                    continue;
                }
                out.inserted += 1;
                let workout_id = tx.last_insert_rowid();
                for (pos, e) in w.exercises.iter().enumerate() {
                    ins_exercise.execute(params![
                        workout_id,
                        pos as i64,
                        e.superset,
                        e.name,
                        e.notes,
                        mode(e.sets.iter().filter_map(|s| s.rest_s)),
                    ])?;
                    let we_id = tx.last_insert_rowid();
                    for (spos, s) in e.sets.iter().enumerate() {
                        ins_set.execute(params![
                            we_id,
                            spos as i64,
                            s.set_type.as_str(),
                            s.weight_kg,
                            s.reps,
                            s.rpe,
                            s.distance_m,
                            s.duration_s,
                            s.rest_s,
                        ])?;
                        out.inserted_sets += 1;
                    }
                }
            }
        }
        record_batch(&tx, "strong_workouts", file_name, import.row_count, out)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn import_bodyweight(&mut self, import: &BodyweightImport, file_name: &str, source: &str) -> Result<ImportOutcome> {
        let tx = self.conn.transaction()?;
        let mut out = ImportOutcome { inserted: 0, skipped: 0, inserted_sets: 0 };
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO bodyweight (measured_at_local, weight_kg, source) VALUES (?1, ?2, ?3)",
            )?;
            for e in &import.entries {
                if ins.execute(params![e.measured_at_local.format(TIME_FMT).to_string(), e.weight_kg, source])? == 0 {
                    out.skipped += 1;
                } else {
                    out.inserted += 1;
                }
            }
        }
        record_batch(&tx, "bodyweight", file_name, import.row_count, out)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn counts(&self) -> Result<Counts> {
        let one = |sql: &str| -> Result<usize> { Ok(self.conn.query_row(sql, [], |r| r.get::<_, i64>(0))? as usize) };
        Ok(Counts {
            workouts: one("SELECT COUNT(*) FROM workout")?,
            sets: one("SELECT COUNT(*) FROM workout_set")?,
            exercise_names: one("SELECT COUNT(DISTINCT source_name) FROM workout_exercise")?,
            weigh_ins: one("SELECT COUNT(*) FROM bodyweight")?,
        })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn record_batch(tx: &rusqlite::Transaction, kind: &str, file_name: &str, rows: usize, out: ImportOutcome) -> Result<()> {
    tx.execute(
        "INSERT INTO import_batch (kind, file_name, rows, inserted, skipped) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![kind, file_name, rows as i64, out.inserted as i64, out.skipped as i64],
    )?;
    Ok(())
}

/// Most common value; ties go to the larger value.
fn mode(values: impl Iterator<Item = i64>) -> Option<i64> {
    let mut counts = std::collections::HashMap::new();
    for v in values {
        *counts.entry(v).or_insert(0usize) += 1;
    }
    counts.into_iter().max_by_key(|&(v, n)| (n, v)).map(|(v, _)| v)
}
