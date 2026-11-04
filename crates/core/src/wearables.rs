//! Watches and health apps: weigh-ins, activities, heart rate during workouts and daily recovery
//! numbers (sleep, resting HR, HRV, steps) from Android Health Connect, which Garmin Connect, Fitbit
//! and Oura all write to, and from Garmin `.fit` activity files.
//!
//! An activity that overlaps a workout already in FitLedger only adds its heart rate to that
//! workout, so a watch "Strength" recording, or the same run from two sources, is never doubled.
//! Other cardio activities become finished one-exercise workouts keyed by `source_key`, so
//! re-imports are idempotent.

use std::io::{Cursor, Read};

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

const TIME_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// A weigh-in from a scale or health app.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WeightSample {
    /// Local wall-clock time.
    pub at: String,
    pub weight_kg: f64,
    /// "garmin", "fitbit", "oura", … (the app that recorded it).
    pub source: String,
}

/// A recorded activity (a run, a ride, a strength session, …).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Activity {
    /// Unique per activity and source, e.g. "hc:<record id>" or "fit:<start epoch>".
    pub key: String,
    pub source: String,
    /// Library slug of the matching cardio exercise; None for non-cardio (strength, yoga, …),
    /// which only lends its heart rate to an overlapping workout.
    pub slug: Option<String>,
    /// Local wall-clock start.
    pub start: String,
    pub duration_s: f64,
    pub distance_m: Option<f64>,
    pub avg_hr: Option<i64>,
    pub max_hr: Option<i64>,
    pub kcal: Option<f64>,
}

/// Recovery numbers for one day. Sleep is the night that ended that morning.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DayHealth {
    pub date: String,
    pub source: String,
    pub sleep_s: Option<i64>,
    pub resting_hr: Option<i64>,
    pub hrv_ms: Option<f64>,
    pub steps: Option<i64>,
}

/// Heart rate over a workout's time, from all-day heart rate samples.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WorkoutHr {
    pub workout_id: i64,
    pub avg_hr: Option<i64>,
    pub max_hr: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WearableImport {
    #[serde(default)]
    pub weights: Vec<WeightSample>,
    #[serde(default)]
    pub activities: Vec<Activity>,
    #[serde(default)]
    pub days: Vec<DayHealth>,
    #[serde(default)]
    pub workout_hr: Vec<WorkoutHr>,
}

/// What an import added.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct WearableSummary {
    pub weights: usize,
    /// New cardio sessions.
    pub activities: usize,
    /// Workouts that got heart rate.
    pub workouts_hr: usize,
    pub days: usize,
}

/// A finished workout without heart rate, for reading heart rate samples over its time.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HrWindow {
    pub workout_id: i64,
    pub start: String,
    pub end: String,
    pub utc_offset_min: Option<i64>,
}

fn parse_time(s: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, TIME_FMT).map_err(|_| Error::Format(format!("bad time {s:?}")))
}

impl Db {
    /// Adds what a watch or health app recorded. Safe to repeat with overlapping data.
    pub fn import_wearable(&mut self, data: &WearableImport) -> Result<WearableSummary> {
        let mut out = WearableSummary::default();
        let tx = self.conn_mut().transaction()?;
        for w in &data.weights {
            if !(w.weight_kg > 0.0) {
                continue;
            }
            let at = parse_time(&w.at)?.format(TIME_FMT).to_string();
            out.weights += tx.execute(
                "INSERT OR IGNORE INTO bodyweight (measured_at_local, weight_kg, source) VALUES (?1, ?2, ?3)",
                params![at, w.weight_kg, w.source],
            )?;
        }
        for a in &data.activities {
            let start = parse_time(&a.start)?;
            let end = start + Duration::seconds(a.duration_s.max(0.0).round() as i64);
            let known: bool = tx.query_row("SELECT count(*) > 0 FROM workout WHERE source_key = ?1", [&a.key], |r| r.get(0))?;
            if known {
                continue;
            }
            // Any workout whose time overlaps this activity (a workout without a duration counts
            // as one minute long).
            let overlap: Option<(i64, Option<i64>)> = tx
                .query_row(
                    "SELECT id, avg_hr FROM workout
                     WHERE finished = 1 AND started_at_local < ?2
                       AND datetime(started_at_local, '+' || max(coalesce(duration_s, 60), 60) || ' seconds') > ?1
                     ORDER BY abs(julianday(started_at_local) - julianday(?1)) LIMIT 1",
                    params![start.format(TIME_FMT).to_string(), end.format(TIME_FMT).to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((id, hr)) = overlap {
                if hr.is_none() && a.avg_hr.is_some() {
                    tx.execute("UPDATE workout SET avg_hr = ?2, max_hr = ?3 WHERE id = ?1", params![id, a.avg_hr, a.max_hr])?;
                    out.workouts_hr += 1;
                }
                continue;
            }
            let Some(slug) = &a.slug else { continue };
            let ex: Option<(i64, String)> =
                tx.query_row("SELECT id, name FROM exercise WHERE slug = ?1", [slug], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            let Some((exercise_id, name)) = ex else { continue };
            tx.execute(
                "INSERT INTO workout (source_key, name, started_at_local, duration_s, finished, avg_hr, max_hr)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
                params![a.key, name, start.format(TIME_FMT).to_string(), a.duration_s.round() as i64, a.avg_hr, a.max_hr],
            )?;
            let w = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO workout_exercise (workout_id, position, source_name, exercise_id) VALUES (?1, 0, ?2, ?3)",
                params![w, name, exercise_id],
            )?;
            let we = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO workout_set (workout_exercise_id, position, set_type, distance_m, duration_s, avg_hr, kcal, completed)
                 VALUES (?1, 0, 'normal', ?2, ?3, ?4, ?5, 1)",
                params![we, a.distance_m.filter(|d| *d > 0.0), a.duration_s, a.avg_hr, a.kcal],
            )?;
            out.activities += 1;
        }
        for h in &data.workout_hr {
            if h.avg_hr.is_none() {
                continue;
            }
            out.workouts_hr += tx.execute(
                "UPDATE workout SET avg_hr = ?2, max_hr = ?3 WHERE id = ?1 AND avg_hr IS NULL",
                params![h.workout_id, h.avg_hr, h.max_hr],
            )?;
        }
        for d in &data.days {
            NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").map_err(|_| Error::Format(format!("bad date {:?}", d.date)))?;
            if d.sleep_s.is_none() && d.resting_hr.is_none() && d.hrv_ms.is_none() && d.steps.is_none() {
                continue;
            }
            out.days += tx.execute(
                "INSERT INTO day_health (date, source, sleep_s, resting_hr, hrv_ms, steps) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (date, source) DO UPDATE SET
                   sleep_s = coalesce(excluded.sleep_s, sleep_s), resting_hr = coalesce(excluded.resting_hr, resting_hr),
                   hrv_ms = coalesce(excluded.hrv_ms, hrv_ms), steps = coalesce(excluded.steps, steps)",
                params![d.date, d.source, d.sleep_s, d.resting_hr, d.hrv_ms, d.steps],
            )?;
        }
        tx.commit()?;
        if out.activities > 0 {
            self.rebuild_prs()?;
        }
        Ok(out)
    }

    /// Finished workouts since `since` (local time) that have a duration but no heart rate yet.
    pub fn hr_windows(&self, since: &str) -> Result<Vec<HrWindow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, started_at_local, datetime(started_at_local, '+' || duration_s || ' seconds'), utc_offset_min
             FROM workout WHERE finished = 1 AND avg_hr IS NULL AND duration_s > 0 AND started_at_local >= ?1
             ORDER BY started_at_local",
        )?;
        let rows = stmt.query_map([since], |r| Ok(HrWindow { workout_id: r.get(0)?, start: r.get(1)?, end: r.get(2)?, utc_offset_min: r.get(3)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Daily recovery numbers, oldest first.
    pub fn day_health(&self) -> Result<Vec<DayHealth>> {
        let mut stmt = self.conn().prepare("SELECT date, source, sleep_s, resting_hr, hrv_ms, steps FROM day_health ORDER BY date, source")?;
        let rows = stmt.query_map([], |r| {
            Ok(DayHealth { date: r.get(0)?, source: r.get(1)?, sleep_s: r.get(2)?, resting_hr: r.get(3)?, hrv_ms: r.get(4)?, steps: r.get(5)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

// ---- Garmin .fit files --------------------------------------------------------------------------

/// Library slug for a FIT sport / sub-sport, None when it isn't cardio FitLedger tracks.
fn fit_slug(sport: &str, sub: &str) -> Option<&'static str> {
    Some(match (sport, sub) {
        ("running", "treadmill" | "indoor_running") => "running-treadmill",
        ("running", "trail") => "fedb-trail-running-walking",
        ("running", _) => "running",
        ("walking", "treadmill" | "indoor_walking") => "fedb-walking-treadmill",
        ("walking", _) => "walking",
        ("hiking", _) => "hiking",
        ("cycling", "indoor_cycling" | "spin" | "virtual_activity") => "cycling-indoor",
        ("cycling" | "e_biking", _) => "cycling",
        ("rowing", "indoor_rowing") | ("fitness_equipment", "indoor_rowing") => "rowing-machine",
        ("swimming", _) => "swimming",
        ("fitness_equipment", "elliptical") => "elliptical",
        ("fitness_equipment", "stair_climbing") | ("floor_climbing", _) => "stair-climber",
        ("cross_country_skiing", "indoor_skiing") | ("fitness_equipment", "indoor_skiing") => "ski-erg",
        ("jumprope", _) => "fedb-rope-jumping",
        _ => return None,
    })
}

/// Seconds since the FIT epoch (1989-12-31 00:00) that fitparser turned into `t`, undoing its
/// conversion to this machine's time zone. `local` is a FIT local_timestamp.
fn fit_raw(t: &DateTime<Local>, local: bool) -> i64 {
    let epoch = NaiveDate::from_ymd_opt(1989, 12, 31).unwrap().and_hms_opt(0, 0, 0).unwrap();
    let base = if local { Local.from_local_datetime(&epoch).unwrap().timestamp() } else { epoch.and_utc().timestamp() };
    t.timestamp() - base
}

fn num(v: &fitparser::Value) -> Option<f64> {
    use fitparser::Value::*;
    Some(match v {
        Byte(x) | Enum(x) | UInt8(x) | UInt8z(x) => *x as f64,
        SInt8(x) => *x as f64,
        UInt16(x) | UInt16z(x) => *x as f64,
        SInt16(x) => *x as f64,
        UInt32(x) | UInt32z(x) => *x as f64,
        SInt32(x) => *x as f64,
        SInt64(x) => *x as f64,
        UInt64(x) | UInt64z(x) => *x as f64,
        Float32(x) => *x as f64,
        Float64(x) => *x,
        _ => return None,
    })
}

/// Activities (one per session) and weigh-ins in a `.fit` file, or in a `.zip` of them as
/// Garmin Connect's "Export Original" and account data export give.
pub fn parse_fit(bytes: &[u8]) -> Result<WearableImport> {
    let mut out = WearableImport::default();
    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Format(format!("can't read the zip: {e}")))?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| Error::Format(format!("can't read the zip: {e}")))?;
            let name = f.name().to_ascii_lowercase();
            if !(name.ends_with(".fit") || name.ends_with(".zip")) {
                continue;
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).map_err(|e| Error::Format(format!("can't read {name}: {e}")))?;
            // Skip files that aren't readable rather than failing a whole account export.
            if let Ok(part) = parse_fit(&buf) {
                out.weights.extend(part.weights);
                out.activities.extend(part.activities);
            }
        }
        return Ok(out);
    }
    let records = fitparser::from_bytes(bytes).map_err(|e| Error::Format(format!("not a readable .fit file: {e}")))?;
    let field = |r: &fitparser::FitDataRecord, name: &str| r.fields().iter().find(|f| f.name() == name).map(|f| f.value().clone());
    let time = |v: Option<fitparser::Value>| match v {
        Some(fitparser::Value::Timestamp(t)) => Some(t),
        _ => None,
    };
    let text = |v: Option<fitparser::Value>| match v {
        Some(fitparser::Value::String(s)) => s,
        _ => String::new(),
    };
    let manufacturer = records
        .iter()
        .find(|r| r.kind() == fitparser::profile::MesgNum::FileId)
        .map(|r| text(field(r, "manufacturer")))
        .filter(|m| !m.is_empty() && m != "development")
        .unwrap_or_else(|| "garmin".into());
    // Offset of local time from UTC, in seconds, from the activity message.
    let offset_s = records.iter().filter(|r| r.kind() == fitparser::profile::MesgNum::Activity).find_map(|r| {
        let utc = time(field(r, "timestamp"))?;
        let local = time(field(r, "local_timestamp"))?;
        Some(fit_raw(&local, true) - fit_raw(&utc, false))
    });
    let to_local = |t: &DateTime<Local>| match offset_s {
        Some(off) => (t.naive_utc() + Duration::seconds(off)).format(TIME_FMT).to_string(),
        None => t.naive_local().format(TIME_FMT).to_string(),
    };
    for r in &records {
        match r.kind() {
            fitparser::profile::MesgNum::Session => {
                let Some(start) = time(field(r, "start_time")) else { continue };
                let Some(duration_s) = field(r, "total_timer_time").or(field(r, "total_elapsed_time")).as_ref().and_then(num) else {
                    continue;
                };
                let sport = text(field(r, "sport"));
                let sub = text(field(r, "sub_sport"));
                let hr = |name| field(r, name).as_ref().and_then(num).filter(|v| *v > 0.0 && *v < 255.0).map(|v| v.round() as i64);
                out.activities.push(Activity {
                    key: format!("fit:{}:{}", fit_raw(&start, false), sport),
                    source: manufacturer.clone(),
                    slug: fit_slug(&sport, &sub).map(Into::into),
                    start: to_local(&start),
                    duration_s,
                    distance_m: field(r, "total_distance").as_ref().and_then(num).filter(|d| *d > 0.0),
                    avg_hr: hr("avg_heart_rate"),
                    max_hr: hr("max_heart_rate"),
                    kcal: field(r, "total_calories").as_ref().and_then(num).filter(|k| *k > 0.0),
                });
            }
            fitparser::profile::MesgNum::WeightScale => {
                let (Some(t), Some(w)) = (time(field(r, "timestamp")), field(r, "weight").as_ref().and_then(num)) else { continue };
                // fitparser leaves this field unscaled (hundredths of a kg) when it's an integer.
                let kg = match field(r, "weight") {
                    Some(fitparser::Value::Float32(_) | fitparser::Value::Float64(_)) => w,
                    _ => w / 100.0,
                };
                if kg > 0.0 && kg < 655.0 {
                    out.weights.push(WeightSample { at: to_local(&t), weight_kg: kg, source: manufacturer.clone() });
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(key: &str, slug: Option<&str>, start: &str, dur: f64, hr: Option<i64>) -> Activity {
        Activity {
            key: key.into(),
            source: "garmin".into(),
            slug: slug.map(Into::into),
            start: start.into(),
            duration_s: dur,
            distance_m: Some(5000.0),
            avg_hr: hr,
            max_hr: hr.map(|h| h + 20),
            kcal: Some(300.0),
        }
    }

    #[test]
    fn activities_become_cardio_once_and_lend_hr_to_overlapping_workouts() {
        let mut db = Db::open_in_memory().unwrap();
        // A strength workout logged in FitLedger, 18:00–19:00.
        db.conn()
            .execute("INSERT INTO workout (name, started_at_local, duration_s, finished) VALUES ('Push', '2026-11-01 18:00:00', 3600, 1)", [])
            .unwrap();
        let push: i64 = db.conn().last_insert_rowid();
        let data = WearableImport {
            weights: vec![WeightSample { at: "2026-11-01 07:00:00".into(), weight_kg: 80.0, source: "garmin".into() }],
            activities: vec![
                act("hc:1", Some("running"), "2026-11-02 07:00:00", 1800.0, Some(150)),
                // The watch's strength recording during Push.
                act("hc:2", None, "2026-11-01 18:05:00", 3000.0, Some(110)),
                // Yoga on its own: not tracked.
                act("hc:3", None, "2026-11-03 07:00:00", 1800.0, Some(90)),
            ],
            days: vec![DayHealth { date: "2026-11-02".into(), source: "health_connect".into(), sleep_s: Some(27000), resting_hr: Some(52), hrv_ms: None, steps: Some(9000) }],
            workout_hr: vec![],
        };
        let s = db.import_wearable(&data).unwrap();
        assert_eq!(s, WearableSummary { weights: 1, activities: 1, workouts_hr: 1, days: 1 });
        let d = db.workout_detail(push).unwrap().unwrap();
        assert_eq!((d.avg_hr, d.max_hr), (Some(110), Some(130)));
        let runs = db.cardio_sessions().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!((runs[0].distance_m, runs[0].avg_hr), (Some(5000.0), Some(150.0)));

        // Again, plus the same run from a .fit file under another key: nothing new.
        let mut again = data.clone();
        again.activities.push(act("fit:1:running", Some("running"), "2026-11-02 07:00:10", 1790.0, Some(151)));
        again.days[0] = DayHealth { hrv_ms: Some(60.0), resting_hr: None, ..again.days[0].clone() };
        let s = db.import_wearable(&again).unwrap();
        assert_eq!((s.weights, s.activities, s.workouts_hr), (0, 0, 0));
        assert_eq!(db.cardio_sessions().unwrap().len(), 1);
        let day = &db.day_health().unwrap()[0];
        assert_eq!((day.resting_hr, day.hrv_ms, day.steps), (Some(52), Some(60.0), Some(9000)));
    }

    #[test]
    fn hr_windows_and_samples() {
        let mut db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO workout (name, started_at_local, duration_s, utc_offset_min, finished) VALUES ('Pull', '2026-11-01 18:00:00', 3600, -360, 1)",
                [],
            )
            .unwrap();
        let id = db.conn().last_insert_rowid();
        let w = db.hr_windows("2026-10-01 00:00:00").unwrap();
        assert_eq!(w, [HrWindow { workout_id: id, start: "2026-11-01 18:00:00".into(), end: "2026-11-01 19:00:00".into(), utc_offset_min: Some(-360) }]);
        let s = db
            .import_wearable(&WearableImport { workout_hr: vec![WorkoutHr { workout_id: id, avg_hr: Some(120), max_hr: Some(160) }], ..Default::default() })
            .unwrap();
        assert_eq!(s.workouts_hr, 1);
        assert!(db.hr_windows("2026-10-01 00:00:00").unwrap().is_empty());
    }

    #[test]
    fn reads_fit_activities_and_weigh_ins() {
        let bike = parse_fit(include_bytes!("../tests/fixtures/garmin-fenix-5-bike.fit")).unwrap();
        let a = &bike.activities[0];
        assert_eq!(a.slug.as_deref(), Some("cycling"));
        assert_eq!(a.source, "garmin");
        // 16:09:22 UTC, recorded in California (UTC−7) per the file's own local timestamp.
        assert_eq!(a.start, "2017-06-12 09:09:22");
        assert_eq!((a.avg_hr, a.max_hr, a.kcal), (Some(101), Some(114), Some(7.0)));
        assert!((a.duration_s - 60.363).abs() < 1e-6 && (a.distance_m.unwrap() - 459.52).abs() < 1e-6);

        let scale = parse_fit(include_bytes!("../tests/fixtures/WeightScaleSingleUser.fit")).unwrap();
        let kg: Vec<f64> = scale.weights.iter().map(|w| w.weight_kg).collect();
        assert_eq!(kg, [75.8, 76.09]);

        let mut db = Db::open_in_memory().unwrap();
        let s = db.import_wearable(&bike).unwrap();
        assert_eq!(s.activities, 1);
    }
}
