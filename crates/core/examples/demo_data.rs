//! Builds a restorable backup full of made-up data (two years of workouts, weigh-ins, cardio,
//! InBody scans, measurements, calories, goals, phases, templates and a schedule), for screenshots
//! and demos. Deterministic: the same output every run. Nothing here is anyone's real data.
//! Usage: cargo run -p fitledger-core --example demo_data -- <out.fitledger> [end date, default today]
use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use fitledger_core::body::DietDay;
use fitledger_core::planning::{Goal, Phase, Schedule};

/// xorshift64*: tiny deterministic PRNG.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Roughly normal, mean 0, sd 1.
    fn normal(&mut self) -> f64 {
        (0..6).map(|_| self.next()).sum::<f64>() * std::f64::consts::SQRT_2 - 6.0 * std::f64::consts::FRAC_1_SQRT_2
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }
}

/// One exercise in a day: Strong name, sets, rep range, starting working weight (lb), increment.
struct Ex {
    name: &'static str,
    sets: u32,
    lo: u32,
    hi: u32,
    start: f64,
    inc: f64,
    warmup: bool,
}
const fn ex(name: &'static str, sets: u32, lo: u32, hi: u32, start: f64, inc: f64, warmup: bool) -> Ex {
    Ex { name, sets, lo, hi, start, inc, warmup }
}

const UPPER_A: &[Ex] = &[
    ex("Bench Press (Barbell)", 3, 5, 8, 155.0, 5.0, true),
    ex("Bent Over Row (Barbell)", 3, 8, 10, 135.0, 5.0, false),
    ex("Overhead Press (Dumbbell)", 3, 8, 12, 40.0, 5.0, false),
    ex("Lat Pulldown (Cable)", 3, 10, 12, 120.0, 5.0, false),
    ex("Lateral Raise (Dumbbell)", 3, 12, 15, 15.0, 2.5, false),
    ex("Bicep Curl (Dumbbell)", 3, 10, 12, 25.0, 2.5, false),
    ex("Triceps Pushdown (Cable - Straight Bar)", 3, 10, 12, 50.0, 5.0, false),
];
const LOWER_A: &[Ex] = &[
    ex("Squat (Barbell)", 3, 5, 8, 185.0, 5.0, true),
    ex("Romanian Deadlift (Dumbbell)", 3, 8, 10, 55.0, 5.0, false),
    ex("Leg Press", 3, 10, 12, 270.0, 10.0, false),
    ex("Lying Leg Curl (Machine)", 3, 10, 12, 90.0, 5.0, false),
    ex("Seated Calf Raise (Plate Loaded)", 3, 12, 15, 90.0, 10.0, false),
    ex("Hanging Leg Raise", 3, 10, 15, 0.0, 0.0, false),
];
const UPPER_B: &[Ex] = &[
    ex("Incline Bench Press (Dumbbell)", 3, 8, 10, 55.0, 5.0, true),
    ex("Seated Wide-Grip Row (Cable)", 3, 10, 12, 130.0, 5.0, false),
    ex("Chest Press (Machine)", 3, 10, 12, 140.0, 10.0, false),
    ex("Lat Pulldown (Single Arm)", 3, 10, 12, 50.0, 5.0, false),
    ex("Face Pull (Cable)", 3, 12, 15, 40.0, 5.0, false),
    ex("Incline Curl (Dumbbell)", 3, 10, 12, 20.0, 2.5, false),
    ex("Triceps Extension (Cable)", 3, 10, 12, 40.0, 5.0, false),
];
const LOWER_B: &[Ex] = &[
    ex("Deadlift (Barbell)", 3, 3, 5, 245.0, 10.0, true),
    ex("Lunge (Barbell)", 3, 8, 10, 95.0, 5.0, false),
    ex("Leg Extension (Machine)", 3, 12, 15, 110.0, 5.0, false),
    ex("Standing Calf Raise (Smith Machine)", 3, 10, 12, 135.0, 10.0, false),
    ex("Decline Crunch", 3, 12, 15, 0.0, 0.0, false),
];
const TORSO_A: &[Ex] = &[
    ex("Bench Press (Barbell)", 4, 5, 8, 155.0, 5.0, true),
    ex("Bent Over Row (Barbell)", 3, 8, 10, 135.0, 5.0, false),
    ex("Overhead Press (Dumbbell)", 3, 8, 12, 40.0, 5.0, false),
    ex("Lat Pulldown (Cable)", 3, 10, 12, 120.0, 5.0, false),
    ex("Pec Deck (Machine)", 3, 12, 15, 100.0, 10.0, false),
    ex("Face Pull (Cable)", 3, 12, 15, 40.0, 5.0, false),
];
const LIMB_A: &[Ex] = &[
    ex("Squat (Barbell)", 4, 5, 8, 185.0, 5.0, true),
    ex("Lying Leg Curl (Machine)", 3, 10, 12, 90.0, 5.0, false),
    ex("Leg Extension (Machine)", 3, 12, 15, 110.0, 5.0, false),
    ex("Bicep Curl (Dumbbell)", 3, 10, 12, 25.0, 2.5, false),
    ex("Triceps Pushdown (Cable - Straight Bar)", 3, 10, 12, 50.0, 5.0, false),
    ex("Seated Calf Raise (Plate Loaded)", 3, 12, 15, 90.0, 10.0, false),
];
const TORSO_B: &[Ex] = &[
    ex("Incline Bench Press (Dumbbell)", 4, 8, 10, 55.0, 5.0, true),
    ex("Seated Wide-Grip Row (Cable)", 3, 10, 12, 130.0, 5.0, false),
    ex("Arnold Press (Dumbbell)", 3, 8, 12, 35.0, 5.0, false),
    ex("Lat Pulldown (Single Arm)", 3, 10, 12, 50.0, 5.0, false),
    ex("Shrug (Dumbbell)", 3, 10, 12, 60.0, 5.0, false),
];
const LIMB_B: &[Ex] = &[
    ex("Deadlift (Barbell)", 3, 3, 5, 245.0, 10.0, true),
    ex("Lunge (Barbell)", 3, 8, 10, 95.0, 5.0, false),
    ex("Leg Press", 3, 10, 12, 270.0, 10.0, false),
    ex("Incline Curl (Dumbbell)", 3, 10, 12, 20.0, 2.5, false),
    ex("Triceps Extension (Machine)", 3, 10, 12, 70.0, 5.0, false),
    ex("Standing Calf Raise (Smith Machine)", 3, 10, 12, 135.0, 10.0, false),
];

/// Nutrition goals: start date, goal, weekly rate (kg, signed).
const GOALS: &[(&str, Goal, f64)] = &[
    ("2024-11-04", Goal::Gain, 0.25),
    ("2025-05-05", Goal::Cut, -0.6),
    ("2025-07-14", Goal::Gain, 0.3),
    ("2026-03-02", Goal::Cut, -0.55),
    ("2026-07-27", Goal::Maintain, 0.0),
];
const SPLIT_CHANGE: &str = "2025-12-01";

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn goal_on(d: NaiveDate) -> (Goal, f64) {
    let (_, g, r) = GOALS.iter().rev().find(|(s, ..)| date(s) <= d).unwrap_or(&GOALS[0]);
    (*g, *r)
}

/// Working weight and current rep target per exercise (double progression).
struct Progress {
    weight: f64,
    reps: u32,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("output path");
    let end = args.next().map(|s| date(&s)).unwrap_or_else(|| Local::now().date_naive() - Duration::days(1));
    let start = date(GOALS[0].0);
    let mut rng = Rng(0x5eed_f17_1ed6e5);

    // Bodyweight trend (kg) per day following the goals, with day-to-day noise.
    let mut trend = 80.0;
    let mut weights = String::from("Date,Weight (kg)\n");
    let mut trend_by_day = std::collections::BTreeMap::new();
    let mut d = start;
    while d <= end {
        let (_, rate) = goal_on(d);
        trend += rate / 7.0 + rng.normal() * 0.004;
        trend_by_day.insert(d, trend);
        if rng.chance(0.72) {
            let w = trend + rng.normal() * 0.45;
            let minute = 5 + (rng.next() * 50.0) as u32;
            weights.push_str(&format!("{d} 07:{minute:02}:00,{w:.1}\n"));
        }
        d += Duration::days(1);
    }

    // Workouts: Mon/Tue/Thu/Fri, a Saturday run most weeks.
    let mut csv = String::from(
        "Date,Timezone,Workout,Workout Duration (s),Group,Exercise,Set,Weight (lb),Reps,RPE,Distance (km),Duration (s),Rest Timer (s),Workout Notes,Exercise Notes\n",
    );
    let mut progress: std::collections::HashMap<&str, Progress> = std::collections::HashMap::new();
    let mut run_pace = 6.6; // min/km
    let mut d = start;
    while d <= end {
        let new_split = d >= date(SPLIT_CHANGE);
        let day: Option<(&str, &[Ex])> = match (d.weekday(), new_split) {
            (Weekday::Mon, false) => Some(("Upper A", UPPER_A)),
            (Weekday::Tue, false) => Some(("Lower A", LOWER_A)),
            (Weekday::Thu, false) => Some(("Upper B", UPPER_B)),
            (Weekday::Fri, false) => Some(("Lower B", LOWER_B)),
            (Weekday::Mon, true) => Some(("Torso A", TORSO_A)),
            (Weekday::Tue, true) => Some(("Limb A", LIMB_A)),
            (Weekday::Thu, true) => Some(("Torso B", TORSO_B)),
            (Weekday::Fri, true) => Some(("Limb B", LIMB_B)),
            _ => None,
        };
        let (goal, _) = goal_on(d);
        let hour = 17 + (rng.next() * 3.0) as u32;
        let minute = (rng.next() * 60.0) as u32;
        let stamp = format!("{d} {hour:02}:{minute:02}:00");
        if let Some((name, exercises)) = day.filter(|_| !rng.chance(0.08)) {
            let duration = 3000 + (rng.next() * 1500.0) as u32;
            let note = if rng.chance(0.06) { ["Felt strong", "Short on sleep", "Busy gym"][(rng.next() * 3.0) as usize] } else { "" };
            for e in exercises {
                let p = progress.entry(e.name).or_insert(Progress { weight: e.start, reps: e.lo });
                if e.warmup {
                    let w = (p.weight * 0.5 / 5.0).round() * 5.0;
                    csv.push_str(&format!("{stamp},America/Chicago (-06:00),{name},{duration},,{},W,{w},8,,,,90,{note},\n", e.name));
                }
                let mut all_top = true;
                for set in 1..=e.sets {
                    // Cuts cost a rep now and then; the last set sometimes falls one short.
                    // Sessions vary: a rep more or less than the target now and then.
                    let mut reps = (p.reps + u32::from(rng.chance(0.35))).saturating_sub(u32::from(rng.chance(0.25)));
                    if goal == Goal::Cut && rng.chance(0.3) || set == e.sets && rng.chance(0.25) {
                        reps = reps.saturating_sub(1);
                    }
                    let reps = reps.clamp(e.lo.saturating_sub(1).max(1), e.hi);
                    all_top &= reps >= e.hi;
                    let rest = if e.lo <= 6 { 180 } else { 90 };
                    csv.push_str(&format!(
                        "{stamp},America/Chicago (-06:00),{name},{duration},,{},{set},{},{reps},,,,{rest},{note},\n",
                        e.name, p.weight
                    ));
                }
                // Weight goes up only some of the times every set reaches the top of the range,
                // more often for the big lifts, so two years add a believable amount.
                let bump = if e.warmup { 0.4 } else { 0.18 };
                if all_top && e.inc > 0.0 && goal != Goal::Cut && rng.chance(bump) {
                    p.weight += e.inc;
                    p.reps = e.lo;
                } else if p.reps < e.hi && rng.chance(0.5) {
                    p.reps += 1;
                }
            }
        }
        if d.weekday() == Weekday::Sat && rng.chance(0.6) {
            let km = ((3.0 + rng.next() * 4.0) * 10.0).round() / 10.0;
            run_pace = (run_pace - 0.004 + rng.normal() * 0.03).max(5.1);
            let secs = (km * run_pace * 60.0) as u32;
            csv.push_str(&format!(
                "{d} 09:{minute:02}:00,America/Chicago (-06:00),Saturday Run,{secs},,Running,1,,,,{km},{secs},,,\n"
            ));
        }
        d += Duration::days(1);
    }

    // InBody scans every few months (lb), from the weight trend.
    let mut inbody = String::from(
        "Test Date / Time,Weight(lb),Skeletal Muscle Mass(lb),Body Fat Mass(lb),Percent Body Fat(%),ECW/TBW,Visceral Fat Level,Right Arm Lean Mass(lb),Left Arm Lean Mass(lb),Trunk Lean Mass(lb),Right Leg Lean Mass(lb),Left Leg Lean Mass(lb),InBody Score,Comments\n",
    );
    for (i, s) in ["2025-02-08", "2025-05-03", "2025-07-19", "2026-02-28", "2026-07-25", "2026-10-24"].iter().enumerate() {
        let d = date(s);
        let Some(kg) = trend_by_day.get(&d) else { continue };
        let lb = kg / 0.45359237;
        let pbf = [15.8, 17.9, 13.6, 18.4, 12.9, 13.4][i];
        let fat = lb * pbf / 100.0;
        let lean = lb - fat;
        let smm = lean * 0.57;
        let (arm, trunk, leg) = (lean * 0.058, lean * 0.47, lean * 0.165);
        inbody.push_str(&format!(
            "{s} 08:15:00,{lb:.1},{smm:.1},{fat:.1},{pbf:.1},0.37{},{},{arm:.1},{:.1},{trunk:.1},{leg:.1},{:.1},{},\n",
            6 + i % 3,
            if pbf > 16.0 { 5 } else { 4 },
            arm - 0.2,
            leg - 0.3,
            80 + (20.0 - pbf) as i32
        ));
    }

    // Monthly tape measurements (in), waist following body weight.
    let mut tape = String::from("Date,Neck (in.),Waist (in.),Chest (in.),Right Bicep (in.)\n");
    let mut d = start + Duration::days(5);
    while d <= end {
        let kg = trend_by_day[&d];
        tape.push_str(&format!(
            "{d} 07:30:00,{:.1},{:.1},{:.1},{:.1}\n",
            14.8 + (kg - 80.0) * 0.03,
            32.0 + (kg - 80.0) * 0.32 + rng.normal() * 0.15,
            40.0 + (kg - 80.0) * 0.2,
            14.5 + (kg - 80.0) * 0.06
        ));
        d += Duration::days(30);
    }

    let tmp = std::env::temp_dir().join(format!("fitledger-demo-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut db = fitledger_core::db::Db::open(&tmp).unwrap();
    db.set_setting("weight_unit", "lb").unwrap();
    db.set_setting("height_cm", "180").unwrap();
    db.set_setting("sex", "male").unwrap();

    let imp = fitledger_core::strong::parse_workouts(&csv, Default::default()).unwrap();
    println!("workouts: {:?}", db.import_strong_workouts(&imp, "demo-workouts.csv").unwrap());
    println!("auto-mapped {} names", db.auto_map_names().unwrap());
    let imp = fitledger_core::strong::parse_bodyweight(&weights, Default::default()).unwrap();
    println!("weigh-ins: {:?}", db.import_bodyweight(&imp, "demo-weight.csv", "strong").unwrap());
    let imp = fitledger_core::strong::parse_measurements(&tape).unwrap();
    println!("measurements: {:?}", db.import_measurements(&imp, "demo-measurements.csv").unwrap());
    let mut mapping = db.preview_inbody_csv(&inbody, None).unwrap().mapping;
    mapping.mass_unit = fitledger_core::units::WeightUnit::Lb; // as picked on the import screen
    let preview = db.preview_inbody_csv(&inbody, Some(mapping)).unwrap();
    assert!(preview.errors.is_empty(), "{:?}", preview.errors);
    println!("InBody scans: {}", db.import_inbody_csv(&inbody, preview.mapping).unwrap());

    for (s, g, r) in GOALS {
        db.set_goal(*g, date(s), (*r != 0.0).then_some(*r)).unwrap();
    }
    for (name, from, to) in [("Upper / Lower", GOALS[0].0, Some("2025-11-30")), ("Torso / Limb", SPLIT_CHANGE, None)] {
        db.save_phase(&Phase {
            id: None,
            name: name.into(),
            lane: "program".into(),
            start_date: from.into(),
            end_date: to.map(Into::into),
            color: None,
            notes: None,
        })
        .unwrap();
    }

    // Calories and protein for the last five months, matching the goal.
    let mut d = end - Duration::days(150);
    while d <= end {
        if !rng.chance(0.12) {
            let base = match goal_on(d).0 {
                Goal::Cut => 2150.0,
                Goal::Maintain => 2600.0,
                Goal::Gain => 2950.0,
            };
            db.save_diet_day(&DietDay {
                date: d.to_string(),
                calories: Some((base + rng.normal() * 220.0).round()),
                protein_g: Some((165.0 + rng.normal() * 18.0).round()),
                notes: None,
            })
            .unwrap();
        }
        d += Duration::days(1);
    }

    // Templates from the latest Torso / Limb workouts, on a weekly schedule.
    let recent = db.history_page(None, 12).unwrap();
    let mut days: Vec<Option<i64>> = vec![None; 7];
    for (slot, name) in [(0, "Torso A"), (1, "Limb A"), (3, "Torso B"), (4, "Limb B")] {
        if let Some(w) = recent.iter().find(|w| w.name == name) {
            days[slot] = Some(db.template_from_workout(w.id, name).unwrap().template_id);
        }
    }
    db.save_schedule(&Schedule {
        id: None,
        name: "Torso / Limb".into(),
        kind: "weekly".into(),
        start_date: SPLIT_CHANGE.into(),
        cycle_length: 7,
        days,
    })
    .unwrap();

    println!("{:?}", db.backup_to(&out).unwrap());
    drop(db);
    let _ = std::fs::remove_file(&tmp);
}
