//! Importer tests on small synthetic files committed to the repo.

use fitledger_core::SetType;
use fitledger_core::db::Db;
use fitledger_core::strong::{ImportOptions, parse_bodyweight, parse_workouts};
use fitledger_core::units::{DistanceUnit, WeightUnit};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn current_format_kg() {
    let imp = parse_workouts(&fixture("strong_current_kg.csv"), ImportOptions::default()).unwrap();
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
    assert_eq!(imp.workouts.len(), 2);
    assert_eq!(imp.set_count(), 12);

    let push = &imp.workouts[0];
    assert_eq!(push.name, "Push Day");
    assert_eq!(push.duration_s, Some(3600));
    assert_eq!(push.utc_offset_min, Some(60));
    assert_eq!(push.tz_name.as_deref(), Some("Europe/Berlin"));
    assert_eq!(push.notes.as_deref(), Some("Felt good"));
    let names: Vec<_> = push.exercises.iter().map(|e| (e.name.as_str(), e.superset)).collect();
    assert_eq!(
        names,
        [
            ("Bench Press (Barbell)", None),
            ("Lateral Raise (Dumbbell)", Some(0)),
            ("Triceps Pushdown (Cable - Straight Bar)", Some(0)),
            ("Pull Up", None),
        ]
    );

    let bench = &push.exercises[0];
    assert_eq!(bench.notes.as_deref(), Some("3x5"), "note stored once");
    let types: Vec<_> = bench.sets.iter().map(|s| s.set_type).collect();
    assert_eq!(types, [SetType::Warmup, SetType::Normal, SetType::Normal, SetType::Failure]);
    assert_eq!(bench.sets[1].weight_kg, Some(80.0));
    assert_eq!(bench.sets[1].rpe, Some(8.0));
    assert_eq!(push.exercises[2].sets[2].set_type, SetType::Drop);
    assert_eq!(push.exercises[3].sets[0].weight_kg, Some(0.0), "bodyweight sets keep weight 0");
    assert_eq!(push.exercises[3].sets[1].weight_kg, Some(10.0), "added weight");

    let run = &imp.workouts[1].exercises[0].sets[0];
    assert_eq!((run.distance_m, run.duration_s, run.reps, run.weight_kg), (Some(5000.0), Some(1500.0), None, None));
}

#[test]
fn old_semicolon_format_needs_unit() {
    let text = fixture("strong_old_semicolon.csv");
    assert!(
        matches!(parse_workouts(&text, ImportOptions::default()), Err(fitledger_core::Error::MissingWeightUnit)),
        "no unit in file and none given"
    );

    let opts = ImportOptions { weight_unit: Some(WeightUnit::Kg), distance_unit: Some(DistanceUnit::Km) };
    let imp = parse_workouts(&text, opts).unwrap();
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
    let w = &imp.workouts[0];
    assert_eq!((w.name.as_str(), w.duration_s), ("Leg Day", Some(4200)));
    assert_eq!(w.exercises[0].sets.len(), 2);
    let plank = &w.exercises[1].sets[0];
    assert_eq!((plank.duration_s, plank.distance_m), (Some(60.0), None));
}

#[test]
fn bodyweight_kg() {
    let imp = parse_bodyweight(&fixture("strong_weight_kg.csv"), ImportOptions::default()).unwrap();
    assert_eq!(imp.entries.len(), 2);
    assert_eq!(imp.warnings.len(), 1, "blank weight row is reported");
    assert!(imp.entries[0].measured_at_local < imp.entries[1].measured_at_local, "sorted");
    assert_eq!(imp.entries[0].weight_kg, 81.0);
}

#[test]
fn reimport_is_idempotent() {
    let imp = parse_workouts(&fixture("strong_current_kg.csv"), ImportOptions::default()).unwrap();
    let mut db = Db::open_in_memory().unwrap();
    assert_eq!(db.count_existing_workouts(&imp).unwrap(), 0);
    let first = db.import_strong_workouts(&imp, "a.csv").unwrap();
    assert_eq!((first.inserted, first.skipped, first.inserted_sets), (2, 0, 12));
    assert_eq!(db.count_existing_workouts(&imp).unwrap(), 2);
    let second = db.import_strong_workouts(&imp, "a.csv").unwrap();
    assert_eq!((second.inserted, second.skipped, second.inserted_sets), (0, 2, 0));
    let c = db.counts().unwrap();
    assert_eq!((c.workouts, c.sets, c.exercise_names), (2, 12, 5));
}

#[test]
fn every_builtin_program_installs() {
    let start = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    for p in fitledger_core::planning::programs() {
        let mut db = Db::open_in_memory().unwrap();
        let ids = db.install_program(&p.id, start).unwrap();
        assert_eq!(ids.len(), p.templates.len(), "{}", p.id);
        let s = db.active_schedule().unwrap().unwrap();
        assert_eq!(s.days.iter().flatten().count(), p.days.iter().flatten().count());
        assert!(!db.projected_weekly_sets(None).unwrap().is_empty());
    }
}

#[test]
fn two_week_programs_alternate_on_the_same_weekdays() {
    // Installed on a Wednesday: the cycle counts from that Monday, A/B/A then B/A/B.
    let d = |day| chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
    let mut db = Db::open_in_memory().unwrap();
    db.install_program("stronglifts-5x5", d(4)).unwrap();
    let s = db.active_schedule().unwrap().unwrap();
    assert_eq!((s.kind.as_str(), s.cycle_length, s.start_date.as_str()), ("cycle", 14, "2026-11-02"));
    let name = |day| {
        let id = s.planned(d(day))?;
        db.list_templates().unwrap().into_iter().find(|t| t.id == Some(id)).map(|t| t.name)
    };
    let week1: Vec<_> = [2, 3, 4, 6].map(name).into();
    let week2: Vec<_> = [9, 11, 13].map(name).into();
    assert_eq!(week1, [Some("SL A".into()), None, Some("SL B".into()), Some("SL A".into())]);
    assert_eq!(week2, [Some("SL B".into()), Some("SL A".into()), Some("SL B".into())]);
    assert_eq!(name(16), Some("SL A".into()), "the third week starts over");
}

#[test]
fn inbody_csv_mapping_is_guessed_then_remembered() {
    let text = fixture("inbody_sample.csv");
    let mut db = Db::open_in_memory().unwrap();
    let p = db.preview_inbody_csv(&text, None).unwrap();
    assert!(!p.remembered);
    assert_eq!(p.mapping.date_column, "Test Date / Time");
    assert_eq!(p.mapping.columns.get("Skeletal Muscle Mass(lb)").map(String::as_str), Some("smm_kg"));
    assert_eq!(p.mapping.columns.get("Right Arm Lean Mass(lb)").map(String::as_str), Some("lean_right_arm_kg"));
    assert!(!p.mapping.columns.contains_key("Comments"));

    let mut mapping = p.mapping.clone();
    mapping.mass_unit = WeightUnit::Lb;
    assert_eq!(db.import_inbody_csv(&text, mapping).unwrap(), 2);
    assert_eq!(db.import_inbody_csv(&text, db.preview_inbody_csv(&text, None).unwrap().mapping).unwrap(), 0, "no duplicates");

    let again = db.preview_inbody_csv(&text, None).unwrap();
    assert!(again.remembered, "same columns next time");
    assert_eq!(again.mapping.mass_unit, WeightUnit::Lb);

    let scans = db.inbody_scans().unwrap();
    assert!((WeightUnit::Lb.from_kg(scans[0].values["smm_kg"]) - 88.6).abs() < 1e-9);
    assert_eq!(scans[1].values["pbf"], 12.1);
    // Scan weights can show on the bodyweight chart.
    assert_eq!(db.weigh_ins(true).unwrap().iter().filter(|w| w.source == "inbody").count(), 2);
    assert!(db.weigh_ins(false).unwrap().is_empty());
}

#[test]
fn strong_export_round_trips() {
    let imp = parse_workouts(&fixture("strong_current_kg.csv"), ImportOptions::default()).unwrap();
    let mut db = Db::open_in_memory().unwrap();
    db.import_strong_workouts(&imp, "a.csv").unwrap();
    db.auto_map_names().unwrap();
    for unit in [WeightUnit::Kg, WeightUnit::Lb] {
        let csv = db.export_strong_csv(unit).unwrap();
        let back = parse_workouts(&csv, ImportOptions::default()).unwrap();
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_eq!((back.workouts.len(), back.set_count()), (2, 12));
        let a = &imp.workouts[0];
        let b = &back.workouts[0];
        assert_eq!((a.started_at_local, a.duration_s, a.utc_offset_min, &a.notes), (b.started_at_local, b.duration_s, b.utc_offset_min, &b.notes));
        for (x, y) in a.exercises.iter().zip(&b.exercises) {
            assert_eq!((x.superset, &x.notes, x.sets.len()), (y.superset, &y.notes, y.sets.len()));
            for (s, t) in x.sets.iter().zip(&y.sets) {
                assert_eq!((s.set_type, s.reps), (t.set_type, t.reps));
                assert!((s.weight_kg.unwrap_or(0.0) - t.weight_kg.unwrap_or(0.0)).abs() < 0.01);
            }
        }
        let run = &back.workouts[1].exercises[0].sets[0];
        assert!((run.distance_m.unwrap() - 5000.0).abs() < 0.1);
    }
}

#[test]
fn backup_and_restore() {
    let dir = std::env::temp_dir().join(format!("fitledger-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let backup = dir.join("backup.fitledger");

    let imp = parse_workouts(&fixture("strong_current_kg.csv"), ImportOptions::default()).unwrap();
    let mut a = Db::open_in_memory().unwrap();
    a.import_strong_workouts(&imp, "a.csv").unwrap();
    a.set_setting("weight_unit", "kg").unwrap();
    let info = a.backup_to(&backup).unwrap();
    assert_eq!(info.workouts, 2);
    assert_eq!(Db::inspect_backup(&backup).unwrap().workouts, 2);

    // Restore on a "different device" with other data: local data is replaced.
    let mut b = Db::open(dir.join("other.db")).unwrap();
    b.import_bodyweight(&parse_bodyweight(&fixture("strong_weight_kg.csv"), ImportOptions::default()).unwrap(), "w", "strong").unwrap();
    b.restore_from(&backup).unwrap();
    let c = b.counts().unwrap();
    assert_eq!((c.workouts, c.sets, c.weigh_ins), (2, 12, 0));
    assert_eq!(b.setting("weight_unit").unwrap().as_deref(), Some("kg"));
    assert!(b.export_json().unwrap()["tables"]["workout_set"].as_array().unwrap().len() == 12);

    std::fs::write(dir.join("junk.fitledger"), b"not a database").unwrap();
    assert!(Db::inspect_backup(dir.join("junk.fitledger")).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn warmup_sets_and_sticky_notes() {
    use fitledger_core::live::StartFrom;
    let mut db = Db::open_in_memory().unwrap();
    let now = chrono::NaiveDateTime::parse_from_str("2026-11-02 17:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let w = db.start_workout(&StartFrom::Empty, now, None).unwrap();
    let bench = db.exercise_id("bench-press-barbell").unwrap().unwrap();
    let we = db.add_exercise(w, bench).unwrap();
    db.add_warmup_set(we).unwrap();
    let sets = &db.workout_detail(w).unwrap().unwrap().exercises[0].sets;
    assert_eq!(sets.iter().map(|s| s.set_type.as_str()).collect::<Vec<_>>(), ["warmup", "normal", "normal", "normal"]);

    db.set_sticky_note(bench, Some("  Bench at notch 4  ")).unwrap();
    assert_eq!(db.previous_for_workout(w).unwrap()[0].setup_note.as_deref(), Some("Bench at notch 4"));
    db.set_sticky_note(bench, Some("")).unwrap();
    assert_eq!(db.previous_for_workout(w).unwrap()[0].setup_note, None);
}

#[test]
fn superset_from_template_gets_even_rounds() {
    use fitledger_core::live::StartFrom;
    use fitledger_core::templates::{Template, TemplateExercise};
    let mut db = Db::open_in_memory().unwrap();
    let ex = |db: &Db, slug, sets| TemplateExercise {
        exercise_id: db.exercise_id(slug).unwrap().unwrap(),
        name: String::new(),
        superset: Some(0),
        target_sets: Some(sets),
        rep_min: None,
        rep_max: None,
        rest_s: None,
        setup_note: None,
    };
    let exercises = vec![ex(&db, "bicep-curl-dumbbell", 3), ex(&db, "triceps-pushdown-rope", 4)];
    let t = db.save_template(&Template { id: None, name: "Arms".into(), notes: None, color: None, exercises, last_done: None }).unwrap();
    let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap().and_hms_opt(18, 0, 0).unwrap();
    let w = db.start_workout(&StartFrom::Template { template_id: t }, now, None).unwrap();
    let d = db.workout_detail(w).unwrap().unwrap();
    assert_eq!(d.exercises.iter().map(|e| e.sets.len()).collect::<Vec<_>>(), [4, 4]);
}

#[test]
fn perform_again_keeps_warmups_as_warmups() {
    use fitledger_core::live::{SetUpdate, StartFrom};
    let mut db = Db::open_in_memory().unwrap();
    let t = |h| chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap().and_hms_opt(h, 0, 0).unwrap();
    let w = db.start_workout(&StartFrom::Empty, t(9), None).unwrap();
    let squat = db.exercise_id("squat-barbell").unwrap().unwrap();
    db.add_exercise(w, squat).unwrap();
    let sets: Vec<i64> = db.workout_detail(w).unwrap().unwrap().exercises[0].sets.iter().map(|s| s.id).collect();
    // 1 warm-up and 2 working sets.
    for (i, id) in sets.iter().enumerate() {
        let u = SetUpdate {
            set_type: Some(if i == 0 { "warmup" } else { "normal" }.into()),
            weight_kg: Some(Some(if i == 0 { 60.0 } else { 84.0 })),
            reps: Some(Some(if i == 0 { 10 } else { 8 })),
            completed: Some(true),
            ..Default::default()
        };
        db.update_set(*id, &u).unwrap();
    }
    db.finish_workout(w, t(10)).unwrap();
    let again = db.start_workout(&StartFrom::Repeat { workout_id: w }, t(18), None).unwrap();
    let d = db.workout_detail(again).unwrap().unwrap();
    let types: Vec<&str> = d.exercises[0].sets.iter().map(|s| s.set_type.as_str()).collect();
    assert_eq!(types, ["warmup", "normal", "normal"]);
}
