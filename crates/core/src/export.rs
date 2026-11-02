//! Exports (Strong-format CSV, InBody/bodyweight/diet CSV, full JSON) and backup/restore.

use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use serde::Serialize;

use crate::body::INBODY_FIELDS;
use crate::db::Db;
use crate::units::WeightUnit;
use crate::{Error, Result};

/// Tables with user data, in an order that satisfies foreign keys on import.
const USER_TABLES: &[&str] = &[
    "setting", "exercise", "exercise_alias", "exercise_muscle", "name_mapping", "template", "template_exercise",
    "workout", "workout_exercise", "workout_set", "bodyweight", "inbody_scan", "diet_day", "schedule",
    "schedule_slot", "phase", "muscle_target", "csv_mapping", "body_measurement",
];

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub schema_version: i64,
    pub workouts: i64,
    pub last_workout: Option<String>,
    pub weigh_ins: i64,
    pub inbody_scans: i64,
}

fn backup_info(conn: &Connection) -> Result<BackupInfo> {
    Ok(BackupInfo {
        schema_version: conn.pragma_query_value(None, "user_version", |r| r.get(0))?,
        workouts: conn.query_row("SELECT COUNT(*) FROM workout WHERE finished = 1", [], |r| r.get(0)).unwrap_or(0),
        last_workout: conn.query_row("SELECT max(started_at_local) FROM workout WHERE finished = 1", [], |r| r.get(0)).unwrap_or(None),
        weigh_ins: conn.query_row("SELECT COUNT(*) FROM bodyweight", [], |r| r.get(0)).unwrap_or(0),
        inbody_scans: conn.query_row("SELECT COUNT(*) FROM inbody_scan", [], |r| r.get(0)).unwrap_or(0),
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn num(v: Option<f64>) -> String {
    v.map(|x| {
        let r = (x * 100.0).round() / 100.0;
        if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
    })
    .unwrap_or_default()
}

impl Db {
    /// Workouts as a Strong workouts CSV (current Strong format), in `unit`.
    pub fn export_strong_csv(&self, unit: WeightUnit) -> Result<String> {
        let u = if unit == WeightUnit::Kg { "kg" } else { "lb" };
        let dist = if unit == WeightUnit::Kg { ("km", 1000.0) } else { ("ft", 0.3048) };
        let mut out = format!(
            "Date,Timezone,Workout,Workout Duration (s),Group,Exercise,Set,Weight ({u}),Reps,RPE,Distance ({}),Duration (s),Rest Timer (s),Workout Notes,Exercise Notes\n",
            dist.0
        );
        let mut stmt = self.conn().prepare(
            "SELECT w.started_at_local, w.tz_name, w.utc_offset_min, w.name, w.duration_s, we.superset, coalesce(e.name, we.source_name),
                    s.set_type, s.position, s.weight_kg, s.reps, s.rpe, s.distance_m, s.duration_s, s.rest_s, w.notes, we.notes, we.id
             FROM workout_set s JOIN workout_exercise we ON we.id = s.workout_exercise_id JOIN workout w ON w.id = we.workout_id
             LEFT JOIN exercise e ON e.id = we.exercise_id
             WHERE w.finished = 1 AND s.completed = 1
             ORDER BY w.started_at_local, w.id, we.position, s.position",
        )?;
        let mut rows = stmt.query([])?;
        let mut last_we = -1;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            let we: i64 = r.get(17)?;
            if we != last_we {
                n = 0;
                last_we = we;
            }
            let set_type: String = r.get(7)?;
            let label = match set_type.as_str() {
                "warmup" => "W".to_string(),
                "drop" => "D".to_string(),
                "failure" => "F".to_string(),
                _ => {
                    n += 1;
                    n.to_string()
                }
            };
            let tz = match (r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?) {
                (name, Some(off)) => {
                    let sign = if off < 0 { '-' } else { '+' };
                    format!("{} ({sign}{:02}:{:02})", name.unwrap_or_default(), off.abs() / 60, off.abs() % 60).trim().to_string()
                }
                (name, None) => name.unwrap_or_default(),
            };
            let group = r.get::<_, Option<i64>>(5)?.map(|g| ((b'A' + (g % 26) as u8) as char).to_string()).unwrap_or_default();
            let fields = [
                r.get::<_, String>(0)?,
                tz,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?.map(|v| v.to_string()).unwrap_or_default(),
                group,
                r.get::<_, String>(6)?,
                label,
                num(r.get::<_, Option<f64>>(9)?.map(|kg| unit.from_kg(kg))),
                r.get::<_, Option<i64>>(10)?.map(|v| v.to_string()).unwrap_or_default(),
                num(r.get(11)?),
                num(r.get::<_, Option<f64>>(12)?.map(|m| m / dist.1)),
                num(r.get(13)?),
                r.get::<_, Option<i64>>(14)?.map(|v| v.to_string()).unwrap_or_default(),
                r.get::<_, Option<String>>(15)?.unwrap_or_default(),
                r.get::<_, Option<String>>(16)?.unwrap_or_default(),
            ];
            out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        Ok(out)
    }

    pub fn export_bodyweight_csv(&self, unit: WeightUnit) -> Result<String> {
        let u = if unit == WeightUnit::Kg { "kg" } else { "lb" };
        let mut out = format!("Date,Weight ({u}),Source\n");
        for w in self.weigh_ins(false)? {
            out.push_str(&format!("{},{},{}\n", w.at, num(Some(unit.from_kg(w.weight_kg))), w.source));
        }
        Ok(out)
    }

    pub fn export_inbody_csv(&self, unit: WeightUnit) -> Result<String> {
        let u = if unit == WeightUnit::Kg { "kg" } else { "lb" };
        let header: Vec<String> = INBODY_FIELDS
            .iter()
            .map(|(_, label, kind)| match *kind {
                "kg" => format!("{label} ({u})"),
                "l" => format!("{label} (L)"),
                "pct" => format!("{label} (%)"),
                "kcal" => format!("{label} (kcal)"),
                _ => label.to_string(),
            })
            .collect();
        let mut out = format!("Date,{},Device,Notes\n", header.iter().map(|h| csv_field(h)).collect::<Vec<_>>().join(","));
        for s in self.inbody_scans()? {
            let vals: Vec<String> = INBODY_FIELDS
                .iter()
                .map(|(c, _, kind)| num(s.values.get(*c).map(|&v| if *kind == "kg" { unit.from_kg(v) } else { v })))
                .collect();
            out.push_str(&format!(
                "{},{},{},{}\n",
                s.measured_at_local,
                vals.join(","),
                csv_field(s.device.as_deref().unwrap_or("")),
                csv_field(s.notes.as_deref().unwrap_or(""))
            ));
        }
        Ok(out)
    }

    /// Tape measurements, lengths in cm (or inches when `unit` is lb, matching the app's display).
    pub fn export_measurements_csv(&self, unit: WeightUnit) -> Result<String> {
        let inches = unit == WeightUnit::Lb;
        let mut out = format!("Date,Body part,Value,Unit\n");
        for m in self.measurements(None)? {
            let (label, kind) = crate::body::BODY_PARTS.iter().find(|p| p.0 == m.part).map(|p| (p.1, p.2)).unwrap_or((m.part.as_str(), "cm"));
            let (v, u) = match kind {
                "pct" => (m.value, "%"),
                _ if inches => (m.value / 2.54, "in"),
                _ => (m.value, "cm"),
            };
            out.push_str(&format!("{},{},{},{}\n", m.at, csv_field(label), num(Some(v)), u));
        }
        Ok(out)
    }

    pub fn export_diet_csv(&self) -> Result<String> {
        let mut out = String::from("Date,Calories (kcal),Protein (g),Notes\n");
        for d in self.diet_days()? {
            out.push_str(&format!("{},{},{},{}\n", d.date, num(d.calories), num(d.protein_g), csv_field(d.notes.as_deref().unwrap_or(""))));
        }
        Ok(out)
    }

    /// Every user table as `{ table: [ {column: value} ] }`, plus metadata.
    pub fn export_json(&self) -> Result<serde_json::Value> {
        let mut tables = serde_json::Map::new();
        for t in USER_TABLES {
            let mut stmt = self.conn().prepare(&format!("SELECT * FROM {t}"))?;
            let cols: Vec<String> = stmt.column_names().into_iter().map(|c| c.to_string()).collect();
            let mut rows = stmt.query([])?;
            let mut arr = Vec::new();
            while let Some(r) = rows.next()? {
                let mut obj = serde_json::Map::new();
                for (i, c) in cols.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        ValueRef::Null => serde_json::Value::Null,
                        ValueRef::Integer(n) => n.into(),
                        ValueRef::Real(f) => serde_json::Number::from_f64(f).map(Into::into).unwrap_or(serde_json::Value::Null),
                        ValueRef::Text(s) => String::from_utf8_lossy(s).into_owned().into(),
                        ValueRef::Blob(_) => serde_json::Value::Null,
                    };
                    obj.insert(c.clone(), v);
                }
                arr.push(serde_json::Value::Object(obj));
            }
            tables.insert(t.to_string(), arr.into());
        }
        Ok(serde_json::json!({
            "format": "liftbook-export",
            "schema_version": backup_info(self.conn())?.schema_version,
            "units": { "weight": "kg", "distance": "m", "times": "local wall-clock YYYY-MM-DD HH:MM:SS" },
            "tables": tables,
        }))
    }

    /// Writes a consistent snapshot of the whole database to `path`.
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<BackupInfo> {
        let path = path.as_ref();
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| Error::Format(format!("can't replace {}: {e}", path.display())))?;
        }
        self.conn().execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        backup_info(&Connection::open(path)?)
    }

    /// What a backup file contains, to compare with local data before restoring.
    pub fn inspect_backup(path: impl AsRef<Path>) -> Result<BackupInfo> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(Error::Format(format!("backup file is damaged: {ok}")));
        }
        let tables: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'workout'", [], |r| r.get(0))?;
        if tables == 0 {
            return Err(Error::Format("not a Liftbook backup".into()));
        }
        backup_info(&conn)
    }

    pub fn info(&self) -> Result<BackupInfo> {
        backup_info(self.conn())
    }

    /// Replaces all local data with the backup, then upgrades it to the current schema.
    pub fn restore_from(&mut self, path: impl AsRef<Path>) -> Result<BackupInfo> {
        let info = Self::inspect_backup(&path)?;
        let current = MIGRATION_COUNT as i64;
        if info.schema_version > current {
            return Err(Error::Format("this backup was made by a newer version of Liftbook; update the app first".into()));
        }
        let src = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        {
            let backup = rusqlite::backup::Backup::new(&src, self.conn_mut())?;
            backup.run_to_completion(256, std::time::Duration::from_millis(0), None)?;
        }
        self.reinit()?;
        self.info()
    }
}

use crate::db::MIGRATION_COUNT;
