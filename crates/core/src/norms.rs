//! Where a measurement stands against published reference data (percentiles), and strength tiers.
//!
//! Tables in `data/norms/` are built by `scripts/build_norms.py`:
//! - body composition: Kelly et al. 2009, NHANES DXA (US adults 1999-2004), LMS curves;
//! - body size: CDC anthropometric reference data, NHANES 2015-2018, percentiles by age band;
//! - strength: OpenPowerlifting (raw, drug-tested) and OpenWeightlifting competitors, each lifter's
//!   best per age band.
//!
//! CrossFit athletes' self-reported lifts aren't bundled (the data's licence is unclear): the app
//! downloads them on request and builds the same kind of table on the device ([`crossfit_tables`]).
//!
//! Other exercises are compared through a published ratio to one of those lifts ([`links`]).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const BODY_COMPOSITION: &str = include_str!("../../../data/norms/body_composition.json");
const BODY_SIZE: &str = include_str!("../../../data/norms/body_size.json");
const STRENGTH: &str = include_str!("../../../data/norms/strength.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    Male,
    Female,
}

/// What to compare against.
#[derive(Debug, Clone, Deserialize)]
pub struct Who {
    pub sex: Sex,
    pub age: f64,
    /// Needed for strength.
    pub bodyweight_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    /// (percentile, value) points to draw, ascending.
    pub points: Vec<(f64, f64)>,
    /// Percentile of each requested value; `None` when the value is missing or not positive.
    pub ranks: Vec<Option<f64>>,
    /// Strength tiers: (name, lowest value), ascending. Empty for body measures.
    pub tiers: Vec<(String, f64)>,
    /// Index into `tiers` for each requested value.
    pub tier_of: Vec<Option<usize>>,
    /// Who the reference people are, for display.
    pub population: String,
    pub source: String,
    /// Short name of the comparison group, mid-sentence ("powerlifting competitors"); empty for body measures.
    pub title: String,
    /// "kg" or "reps" for lifts; empty for body measures.
    pub unit: String,
    /// How your exercise was converted to the reference lift, when it was.
    pub conversion: Option<String>,
}

/// Measures with reference data.
pub const METRICS: &[&str] = &["fmi", "pbf", "lmi", "almi", "weight_kg", "bmi", "waist_cm", "arm_cm"];

/// Percentiles are reported between these; the tails of the tables aren't reliable.
const MIN_PCT: f64 = 1.0;
const MAX_PCT: f64 = 99.0;

pub fn reference(metric: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    match metric {
        "fmi" | "pbf" | "lmi" | "almi" => composition(metric, who, values),
        "weight_kg" | "bmi" | "waist_cm" | "arm_cm" => size(metric, who, values),
        _ => None,
    }
}

// ---------- LMS curves ----------

#[derive(Deserialize)]
struct CompositionTables {
    fmi: SexTables,
    pbf: SexTables,
    lmi: SexTables,
    almi: SexTables,
}

#[derive(Deserialize)]
struct SexTables {
    male: GroupTables,
    female: GroupTables,
}

/// Rows of [age, L, M, S].
#[derive(Deserialize)]
struct GroupTables {
    white: Vec<[f64; 4]>,
    black: Vec<[f64; 4]>,
    mexican_american: Vec<[f64; 4]>,
}

fn composition_tables() -> &'static CompositionTables {
    static T: OnceLock<CompositionTables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(BODY_COMPOSITION).expect("body_composition.json"))
}

/// L, M, S at an age (linear between the 5-year rows, clamped to 20-85).
fn lms_at(rows: &[[f64; 4]], age: f64) -> (f64, f64, f64) {
    let age = age.clamp(rows[0][0], rows[rows.len() - 1][0]);
    let i = rows.iter().rposition(|r| r[0] <= age).unwrap().min(rows.len() - 2);
    let (a, b) = (rows[i], rows[i + 1]);
    let t = (age - a[0]) / (b[0] - a[0]);
    let lerp = |k: usize| a[k] + (b[k] - a[k]) * t;
    (lerp(1), lerp(2), lerp(3))
}

fn lms_z(x: f64, (l, m, s): (f64, f64, f64)) -> f64 {
    if l.abs() < 1e-6 {
        (x / m).ln() / s
    } else {
        ((x / m).powf(l) - 1.0) / (l * s)
    }
}

fn composition(metric: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    let t = composition_tables();
    let by_sex = match metric {
        "fmi" => &t.fmi,
        "pbf" => &t.pbf,
        "lmi" => &t.lmi,
        _ => &t.almi,
    };
    let g = match who.sex {
        Sex::Male => &by_sex.male,
        Sex::Female => &by_sex.female,
    };
    // The study publishes separate curves for the three groups it sampled; use the average percentile.
    let curves: Vec<_> = [&g.white, &g.black, &g.mexican_american].iter().map(|rows| lms_at(rows, who.age)).collect();
    let pct = |x: f64| curves.iter().map(|c| normal_cdf(lms_z(x, *c))).sum::<f64>() / curves.len() as f64 * 100.0;
    let ranks = values.iter().map(|&x| (x > 0.0).then(|| pct(x).clamp(MIN_PCT, MAX_PCT))).collect();
    // Points: invert the averaged percentile by bisection.
    let points = [3.0, 5.0, 10.0, 25.0, 50.0, 75.0, 90.0, 95.0, 97.0]
        .iter()
        .map(|&p| {
            let (mut lo, mut hi) = (1e-3, curves.iter().map(|c| c.1).fold(0.0, f64::max) * 5.0);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if pct(mid) < p {
                    lo = mid
                } else {
                    hi = mid
                }
            }
            (p, (lo + hi) / 2.0)
        })
        .collect();
    Some(Reference {
        points,
        ranks,
        tiers: vec![],
        tier_of: vec![None; values.len()],
        population: format!("US {} aged about {:.0}, measured by DXA scan", sex_word(who.sex), who.age.clamp(20.0, 85.0)),
        source: "Kelly, Wilson & Heymsfield 2009, NHANES 1999–2004 (PLoS ONE 4: e7038)".into(),
        title: String::new(),
        unit: String::new(),
        conversion: None,
    })
}

/// Standard normal CDF (Abramowitz & Stegun 26.2.17, error < 7.5e-8).
fn normal_cdf(z: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.2316419 * z.abs());
    let poly = t * (0.319381530 + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    let p = 1.0 - (-z * z / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt() * poly;
    if z >= 0.0 {
        p
    } else {
        1.0 - p
    }
}

// ---------- Percentile tables ----------

/// Percentile of `x` from ascending (percentile, value) points, linear between them.
/// Below the first point or above the last it reports the end percentile.
fn rank_in(points: &[(f64, f64)], x: f64) -> f64 {
    if x <= points[0].1 {
        return points[0].0;
    }
    for w in points.windows(2) {
        let ((p0, v0), (p1, v1)) = (w[0], w[1]);
        if x <= v1 {
            return if v1 > v0 { p0 + (p1 - p0) * (x - v0) / (v1 - v0) } else { p1 };
        }
    }
    points[points.len() - 1].0
}

#[derive(Deserialize)]
struct SizeTables {
    percentiles: Vec<f64>,
    age_from: Vec<f64>,
    weight_kg: SizeSex,
    bmi: SizeSex,
    waist_cm: SizeSex,
    arm_cm: SizeSex,
}

#[derive(Deserialize)]
struct SizeSex {
    male: Vec<Vec<f64>>,
    female: Vec<Vec<f64>>,
}

fn size_tables() -> &'static SizeTables {
    static T: OnceLock<SizeTables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(BODY_SIZE).expect("body_size.json"))
}

fn band_of(starts: &[f64], x: f64) -> usize {
    starts.iter().rposition(|&s| x >= s).unwrap_or(0)
}

fn size(metric: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    let t = size_tables();
    let by_sex = match metric {
        "weight_kg" => &t.weight_kg,
        "bmi" => &t.bmi,
        "waist_cm" => &t.waist_cm,
        _ => &t.arm_cm,
    };
    let rows = match who.sex {
        Sex::Male => &by_sex.male,
        Sex::Female => &by_sex.female,
    };
    let band = band_of(&t.age_from, who.age);
    let points: Vec<(f64, f64)> = t.percentiles.iter().copied().zip(rows[band].iter().copied()).collect();
    let ranks = values.iter().map(|&x| (x > 0.0).then(|| rank_in(&points, x))).collect();
    let from = t.age_from[band];
    let ages = if band + 1 < t.age_from.len() { format!("{}–{}", from, t.age_from[band + 1] - 1.0) } else { format!("{from}+") };
    Some(Reference {
        points,
        ranks,
        tiers: vec![],
        tier_of: vec![None; values.len()],
        population: format!("US {} aged {ages}", sex_word(who.sex)),
        source: "CDC, Anthropometric Reference Data, NHANES 2015–2018 (Vital Health Stat 3(46), 2021)".into(),
        title: String::new(),
        unit: String::new(),
        conversion: None,
    })
}

// ---------- Strength ----------

/// Percentile grids for lifts: by sex, age band and 10 kg bodyweight bin. `strength.json`, and the
/// CrossFit table built on the device, have this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftTables {
    pub percentiles: Vec<f64>,
    pub min_n: usize,
    pub lifts: BTreeMap<String, LiftTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftTable {
    pub age_from: Vec<f64>,
    pub bw_from: Vec<f64>,
    /// [age band][bodyweight bin]; `None` with too few lifters.
    pub male: Vec<Vec<Option<Cell>>>,
    pub female: Vec<Vec<Option<Cell>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub n: usize,
    /// Values at `LiftTables::percentiles`.
    pub p: Vec<f64>,
}

pub fn bundled_lifts() -> &'static LiftTables {
    static T: OnceLock<LiftTables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(STRENGTH).expect("strength.json"))
}

/// Tier names and how each starts, from the competitor percentiles:
/// below half the median competitor is Beginner; Novice up to the 10th percentile; Intermediate
/// to the median; Advanced to the 90th; Elite above.
pub const TIERS: &[&str] = &["Beginner", "Novice", "Intermediate", "Advanced", "Elite"];

/// Who a reference lift's data describes.
struct Group {
    title: &'static str,
    people: &'static str,
    source: &'static str,
    /// Tiers only make sense against competitors.
    tiers: bool,
    reps: bool,
}

fn group(lift: &str) -> Group {
    let pl = Group {
        title: "powerlifting competitors",
        people: "in raw, drug-tested powerlifting competitions",
        source: "OpenPowerlifting (openpowerlifting.org), best competition lift per lifter",
        tiers: true,
        reps: false,
    };
    let wl = Group {
        title: "weightlifting competitors",
        people: "in weightlifting competitions (adult categories)",
        source: "OpenWeightlifting (openweightlifting.org), best competition lift per lifter",
        tiers: true,
        reps: false,
    };
    let cf = Group {
        title: "CrossFit athletes",
        people: if lift == "cf_pullups" {
            "who entered the CrossFit Open, from their self-reported best (2017; CrossFit pull-ups usually allow kipping)"
        } else {
            "who entered the CrossFit Open, from their self-reported bests (2017)"
        },
        source: "CrossFit athlete profiles, Kaggle dataset ulrikthygepedersen/crossfit-athletes (downloaded on this device)",
        tiers: false,
        reps: lift == "cf_pullups",
    };
    match lift {
        "snatch" | "clean_jerk" => wl,
        l if l.starts_with("cf_") => cf,
        _ => pl,
    }
}

const LIFT_NAMES: &[(&str, &str)] = &[
    ("squat", "back squat"),
    ("bench", "bench press"),
    ("deadlift", "deadlift"),
    ("snatch", "snatch"),
    ("clean_jerk", "clean and jerk"),
    ("cf_squat", "back squat"),
    ("cf_deadlift", "deadlift"),
    ("cf_snatch", "snatch"),
    ("cf_clean_jerk", "clean and jerk"),
    ("cf_pullups", "pull-ups"),
];

/// Percentiles (and tiers, against competitors) of `values` for a reference lift.
pub fn lift_reference(tables: &LiftTables, lift: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    let t = tables.lifts.get(lift)?;
    let bw = who.bodyweight_kg?;
    let grid = match who.sex {
        Sex::Male => &t.male,
        Sex::Female => &t.female,
    };
    let band = band_of(&t.age_from, who.age.max(t.age_from[0]));
    let row = &grid[band];
    // Blend the two nearest bodyweight bins with data, by distance to their middles.
    let mid = |i: usize| t.bw_from[i] + 5.0;
    let filled: Vec<usize> = (0..row.len()).filter(|&i| row[i].is_some()).collect();
    let below = filled.iter().rev().find(|&&i| mid(i) <= bw).copied();
    let above = filled.iter().find(|&&i| mid(i) > bw).copied();
    let p: Vec<f64> = match (below, above) {
        (Some(a), Some(b)) => {
            let w = (bw - mid(a)) / (mid(b) - mid(a));
            let (pa, pb) = (&row[a].as_ref()?.p, &row[b].as_ref()?.p);
            pa.iter().zip(pb).map(|(x, y)| x + (y - x) * w).collect()
        }
        (Some(i), None) | (None, Some(i)) => row[i].as_ref()?.p.clone(),
        (None, None) => return None,
    };
    let points: Vec<(f64, f64)> = tables.percentiles.iter().copied().zip(p.iter().copied()).collect();
    let g = group(lift);
    let tiers: Vec<(String, f64)> = if g.tiers {
        let at = |pct: f64| points.iter().find(|q| q.0 == pct).map(|q| q.1).unwrap();
        let median = at(50.0);
        TIERS.iter().zip([0.0, median * 0.5, at(10.0), median, at(90.0)]).map(|(n, v)| (n.to_string(), v)).collect()
    } else {
        vec![]
    };
    let tier_of = values
        .iter()
        .map(|&x| (x > 0.0 && !tiers.is_empty()).then(|| tiers.iter().rposition(|(_, from)| x >= *from).unwrap_or(0)))
        .collect();
    let ranks = values.iter().map(|&x| (x > 0.0).then(|| rank_in(&points, x))).collect();
    let from = t.age_from[band];
    let ages = if t.age_from.len() == 1 {
        "18+".to_string()
    } else if band + 1 < t.age_from.len() {
        format!("{}–{}", from, t.age_from[band + 1] - 1.0)
    } else {
        format!("{from}+")
    };
    Some(Reference {
        points,
        ranks,
        tiers,
        tier_of,
        population: format!("{} {}, aged {ages}, around {:.0} kg bodyweight", sex_word(who.sex), g.people, bw),
        source: g.source.into(),
        title: g.title.into(),
        unit: if g.reps { "reps" } else { "kg" }.into(),
        conversion: None,
    })
}

/// How one of your exercises maps to a reference lift: your lift ≈ a + b × the reference lift
/// (`female` overrides a, b for women).
#[derive(Debug, Clone, Copy)]
pub struct Link {
    pub lift: &'static str,
    pub a: f64,
    pub b: f64,
    pub female: Option<(f64, f64)>,
    /// Where the ratio comes from; `None` for the same lift.
    pub basis: Option<&'static str>,
}

const fn same(lift: &'static str) -> Link {
    Link { lift, a: 0.0, b: 1.0, female: None, basis: None }
}

const fn ratio(lift: &'static str, b: f64, basis: &'static str) -> Link {
    Link { lift, a: 0.0, b, female: None, basis: Some(basis) }
}

const DUMBBELL_BENCH: &str =
    "dumbbell bench 1RM (both dumbbells) was 17 % below barbell (Saeterbakken, van den Tillaar & Fimland 2011, J Sports Sci)";
const SMITH_BENCH: &str = "Smith machine bench 1RM was about 3 % below barbell (Saeterbakken et al. 2011)";
const INCLINE: &str = "flat bench 1RM is 21.5 % above a 25° incline in elite lifters and 28.6 % above 30° in active men; 0.80 sits between";
const FRONT_SQUAT: &str = "front squat 1RM was about 78 % of back squat (Gullett et al. 2009, J Strength Cond Res) and 71 % (Atkins & Richards 2016)";

/// Reference lifts for an exercise (by library slug), with the conversion used. Exercises not
/// listed have no comparison: no study gives a usable ratio for them.
pub fn links(slug: &str) -> Vec<Link> {
    match slug {
        "squat-barbell" => vec![same("squat"), same("cf_squat")],
        "deadlift-barbell" | "sumo-deadlift-barbell" => vec![same("deadlift"), same("cf_deadlift")],
        "bench-press-barbell" | "fedb-bench-press-powerlifting" => vec![same("bench")],
        "snatch-barbell" | "fedb-snatch" => vec![same("snatch"), same("cf_snatch")],
        "clean-and-jerk-barbell" | "fedb-clean-and-jerk" => vec![same("clean_jerk"), same("cf_clean_jerk")],
        "pull-up" => vec![same("cf_pullups")],
        "bench-press-dumbbell" => vec![ratio("bench", 0.83, DUMBBELL_BENCH)],
        "bench-press-smith-machine" => vec![ratio("bench", 0.97, SMITH_BENCH)],
        "incline-bench-press-barbell" => vec![ratio("bench", 0.80, INCLINE)],
        "incline-bench-press-dumbbell" => vec![ratio("bench", 0.80 * 0.83, "the incline (0.80) and dumbbell (0.83) ratios combined")],
        "incline-bench-press-smith-machine" => vec![ratio("bench", 0.80 * 0.97, "the incline (0.80) and Smith machine (0.97) ratios combined")],
        "close-grip-bench-press-barbell" => vec![ratio(
            "bench",
            0.95,
            "trained men bench about 5–6 % more with a wide grip than a close grip (Larsen et al. 2020)",
        )],
        "front-squat-barbell" | "fedb-front-squat-clean-grip" => vec![ratio("squat", 0.75, FRONT_SQUAT)],
        "trap-bar-deadlift" => vec![ratio(
            "deadlift",
            1.08,
            "powerlifters' hex bar 1RM was 265 vs 245 kg straight bar (Swinton et al. 2011); other studies found +15 % and no difference",
        )],
        "squat-smith-machine" => vec![Link {
            lift: "squat",
            a: 0.0,
            b: 1.0,
            female: Some((28.3, 0.73)),
            basis: Some(
                "men's Smith machine and free squat 1RMs didn't differ; for women, Smith = 28.3 kg + 0.73 × free (Cotterman, Darby & Skelly 2005)",
            ),
        }],
        _ => vec![],
    }
}

/// Comparisons for one exercise: every linked reference lift with data (CrossFit only once
/// downloaded). `weight` values are estimated 1RMs (kg), `reps` values most reps; oldest first.
pub fn exercise_references(slug: &str, who: &Who, weight: &[f64], reps: &[f64], crossfit: Option<&LiftTables>) -> Vec<Reference> {
    links(slug)
        .into_iter()
        .filter_map(|l| {
            let tables = if l.lift.starts_with("cf_") { crossfit? } else { bundled_lifts() };
            let (a, b) = match (who.sex, l.female) {
                (Sex::Female, Some(ab)) => ab,
                _ => (l.a, l.b),
            };
            let values = if group(l.lift).reps { reps } else { weight };
            let base: Vec<f64> = values.iter().map(|v| (v - a) / b).collect();
            let mut r = lift_reference(tables, l.lift, who, &base)?;
            // Show the reference in your exercise's terms.
            let back = |v: f64| a + b * v;
            r.points.iter_mut().for_each(|p| p.1 = back(p.1));
            r.tiers.iter_mut().for_each(|t| t.1 = back(t.1));
            if let Some(basis) = l.basis {
                let name = LIFT_NAMES.iter().find(|n| n.0 == l.lift).map_or(l.lift, |n| n.1);
                let how = if a == 0.0 { format!("{b:.2} × their {name}") } else { format!("{a} kg + {b} × their {name}") };
                r.conversion = Some(format!("Estimated: compared as {how}, because {basis}."));
            }
            Some(r)
        })
        .collect()
}

/// Whether an exercise has a CrossFit comparison (available once that data is downloaded).
pub fn has_crossfit(slug: &str) -> bool {
    links(slug).iter().any(|l| l.lift.starts_with("cf_"))
}

// ---------- CrossFit (built on the device) ----------

/// Where the CrossFit data is downloaded from, and the SHA-256 of the `athletes.csv` inside it.
pub const CROSSFIT_URL: &str = "https://www.kaggle.com/api/v1/datasets/download/ulrikthygepedersen/crossfit-athletes";
pub const CROSSFIT_CSV_SHA256: &str = "e6a019bd1d2b6c29bf02a62693d22443850b12a1c475be9f68c34892bfc6ff83";

/// `athletes.csv` from the downloaded zip.
pub fn crossfit_csv_from_zip(zip_bytes: &[u8]) -> crate::Result<Vec<u8>> {
    use std::io::Read;
    let bad = |e: &dyn std::fmt::Display| crate::Error::Format(format!("CrossFit download: {e}"));
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| bad(&e))?;
    let mut f = z.by_name("athletes.csv").map_err(|e| bad(&e))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).map_err(|e| bad(&e))?;
    Ok(out)
}

const AGE_FROM: [f64; 5] = [18.0, 40.0, 50.0, 60.0, 70.0];
const MIN_N: usize = 100;
const LB: f64 = 0.45359237;

/// Builds percentile tables from the CrossFit athletes CSV (`athletes.csv`: pounds and reps,
/// self-reported). Implausible entries are dropped.
pub fn crossfit_tables(csv_text: impl std::io::Read) -> crate::Result<LiftTables> {
    // (column, lift, lowest and highest plausible value in the file's units)
    const COLS: &[(&str, &str, f64, f64)] = &[
        ("backsq", "cf_squat", 45.0, 1000.0),
        ("deadlift", "cf_deadlift", 45.0, 1100.0),
        ("snatch", "cf_snatch", 20.0, 500.0),
        ("candj", "cf_clean_jerk", 30.0, 600.0),
        ("pullups", "cf_pullups", 1.0, 100.0),
    ];
    let mut r = csv::Reader::from_reader(csv_text);
    let head = r.headers()?.clone();
    let col = |name: &str| head.iter().position(|h| h == name).ok_or_else(|| crate::Error::Format(format!("CrossFit data has no {name} column")));
    let (gender, age, weight) = (col("gender")?, col("age")?, col("weight")?);
    let cols: Vec<(usize, &str, f64, f64)> = COLS.iter().map(|&(c, l, lo, hi)| Ok((col(c)?, l, lo, hi))).collect::<crate::Result<_>>()?;
    let bw_from: Vec<f64> = (4..16).map(|i| i as f64 * 10.0).collect();
    let mut cells: BTreeMap<(&str, Sex, usize, usize), Vec<f64>> = BTreeMap::new();
    for rec in r.records() {
        let rec = rec?;
        let num = |i: usize| rec.get(i).and_then(|v| v.parse::<f64>().ok());
        let sex = match rec.get(gender) {
            Some("Male") => Sex::Male,
            Some("Female") => Sex::Female,
            _ => continue,
        };
        let (Some(a), Some(w)) = (num(age), num(weight)) else { continue };
        let kg = w * LB;
        if !(18.0..=90.0).contains(&a) || !(35.0..=200.0).contains(&kg) || kg < bw_from[0] {
            continue;
        }
        let (ab, wb) = (band_of(&AGE_FROM, a), band_of(&bw_from, kg));
        for &(c, lift, lo, hi) in &cols {
            if let Some(v) = num(c).filter(|v| (lo..=hi).contains(v)) {
                let v = if lift == "cf_pullups" { v } else { v * LB };
                cells.entry((lift, sex, ab, wb)).or_default().push(v);
            }
        }
    }
    let percentiles: Vec<f64> = (1..20).map(|i| i as f64 * 5.0).collect();
    let mut lifts = BTreeMap::new();
    for &(_, lift, _, _) in COLS {
        let grid = |sex: Sex| -> Vec<Vec<Option<Cell>>> {
            (0..AGE_FROM.len())
                .map(|ab| {
                    (0..bw_from.len())
                        .map(|wb| {
                            let mut v = cells.get(&(lift, sex, ab, wb))?.clone();
                            if v.len() < MIN_N {
                                return None;
                            }
                            v.sort_by(f64::total_cmp);
                            let q = |p: f64| v[((p / 100.0 * v.len() as f64) as usize).min(v.len() - 1)];
                            Some(Cell { n: v.len(), p: percentiles.iter().map(|&p| (q(p) * 10.0).round() / 10.0).collect() })
                        })
                        .collect()
                })
                .collect()
        };
        lifts.insert(
            lift.to_string(),
            LiftTable { age_from: AGE_FROM.to_vec(), bw_from: bw_from.clone(), male: grid(Sex::Male), female: grid(Sex::Female) },
        );
    }
    Ok(LiftTables { percentiles, min_n: MIN_N, lifts })
}

fn sex_word(s: Sex) -> &'static str {
    match s {
        Sex::Male => "men",
        Sex::Female => "women",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(sex: Sex, age: f64) -> Who {
        Who { sex, age, bodyweight_kg: Some(85.0) }
    }

    fn white_men(metric: &str) -> &'static [[f64; 4]] {
        let t = composition_tables();
        let by_sex = match metric {
            "fmi" => &t.fmi,
            _ => &t.lmi,
        };
        &by_sex.male.white
    }

    #[test]
    fn lms_median_is_50th_percentile() {
        // Kelly Table S1, white men aged 20: FMI median 5.95.
        let c = lms_at(white_men("fmi"), 20.0);
        assert!((normal_cdf(lms_z(5.95, c)) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn lms_matches_published_sigma() {
        // Table S5, white men aged 20: LMI M 18.98, sigma 2.50, L -1.115. The value one S*M
        // above in the Box-Cox scale is z = 1 by construction.
        let (l, m, s): (f64, f64, f64) = (-1.115, 18.98, 2.50 / 18.98);
        let x = m * (1.0 + l * s).powf(1.0 / l);
        assert!((normal_cdf(lms_z(x, lms_at(white_men("lmi"), 20.0))) - 0.8413).abs() < 5e-4);
    }

    #[test]
    fn lms_interpolates_age() {
        // Table S1: the median at 22.5 lies between the published medians at 20 (5.95) and 25 (6.37).
        let m = lms_at(white_men("fmi"), 22.5).1;
        assert!((m - (5.95 + 6.37) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn composition_averages_the_three_curves() {
        let g = &composition_tables().almi.male;
        let mean = [&g.white, &g.black, &g.mexican_american].iter().map(|rows| normal_cdf(lms_z(8.5, lms_at(rows, 30.0)))).sum::<f64>()
            / 3.0
            * 100.0;
        let r = reference("almi", &who(Sex::Male, 30.0), &[8.5]).unwrap();
        assert!((r.ranks[0].unwrap() - mean).abs() < 1e-9);
    }

    #[test]
    fn cdc_table_points() {
        // Table 5: men 20-29, weight 50th percentile 81.3 kg, 95th 128.1 kg.
        let r = reference("weight_kg", &who(Sex::Male, 25.0), &[81.3, 128.1, 200.0, 40.0]).unwrap();
        assert_eq!(r.ranks, vec![Some(50.0), Some(95.0), Some(95.0), Some(5.0)]);
        // Table 19: women 40-49 waist, 25th 85.9, 50th 96.1: halfway is the 37.5th.
        let r = reference("waist_cm", &who(Sex::Female, 45.0), &[(85.9 + 96.1) / 2.0]).unwrap();
        assert!((r.ranks[0].unwrap() - 37.5).abs() < 1e-9);
        assert!(r.population.contains("40–49"));
    }

    #[test]
    fn strength_tiers_ascend() {
        let r = lift_reference(bundled_lifts(), "squat", &who(Sex::Male, 28.0), &[60.0, 150.0, 200.0, 300.0]).unwrap();
        assert_eq!(r.tiers.len(), 5);
        assert!(r.tiers.windows(2).all(|w| w[0].1 < w[1].1));
        let tiers: Vec<_> = r.tier_of.iter().map(|t| TIERS[t.unwrap()]).collect();
        assert_eq!(tiers[0], "Beginner");
        assert_eq!(tiers[3], "Elite");
        assert!(r.ranks[2].unwrap() > 40.0 && r.ranks[2].unwrap() < 60.0, "{:?}", r.ranks);
    }

    #[test]
    fn strength_needs_bodyweight() {
        let mut w = who(Sex::Female, 30.0);
        w.bodyweight_kg = None;
        assert!(lift_reference(bundled_lifts(), "bench", &w, &[50.0]).is_none());
    }

    #[test]
    fn bundled_lifts_resolve() {
        for lift in ["squat", "bench", "deadlift", "snatch", "clean_jerk"] {
            for sex in [Sex::Male, Sex::Female] {
                let r = lift_reference(bundled_lifts(), lift, &who(sex, 30.0), &[50.0]).unwrap();
                assert!(r.points.windows(2).all(|w| w[0].1 <= w[1].1), "{lift} {sex:?}");
                assert_eq!(r.tiers.len(), 5);
            }
        }
    }

    #[test]
    fn variations_convert_both_ways() {
        // A 100 kg dumbbell bench ranks like an 100/0.83 kg barbell bench, and the strip is in
        // dumbbell terms.
        let w = who(Sex::Male, 30.0);
        let db = &exercise_references("bench-press-dumbbell", &w, &[100.0], &[], None)[0];
        let bb = lift_reference(bundled_lifts(), "bench", &w, &[100.0 / 0.83]).unwrap();
        assert!((db.ranks[0].unwrap() - bb.ranks[0].unwrap()).abs() < 1e-9);
        assert!((db.points[9].1 - bb.points[9].1 * 0.83).abs() < 1e-9);
        assert!(db.conversion.as_deref().unwrap().contains("Saeterbakken"));
        // Same lift: no conversion; CrossFit skipped until downloaded.
        let sq = exercise_references("squat-barbell", &w, &[150.0], &[], None);
        assert_eq!(sq.len(), 1);
        assert!(sq[0].conversion.is_none());
        assert!(has_crossfit("squat-barbell") && !has_crossfit("bench-press-dumbbell"));
        // Women's Smith squat uses the linear equation.
        let f = who(Sex::Female, 30.0);
        let smith = &exercise_references("squat-smith-machine", &f, &[28.3 + 0.73 * 80.0], &[], None)[0];
        let free = lift_reference(bundled_lifts(), "squat", &f, &[80.0]).unwrap();
        assert!((smith.ranks[0].unwrap() - free.ranks[0].unwrap()).abs() < 1e-9);
        // Unlinked exercises have nothing.
        assert!(exercise_references("romanian-deadlift-dumbbell", &w, &[100.0], &[], None).is_empty());
    }

    #[test]
    fn crossfit_tables_from_csv() {
        let mut csv = String::from("athlete_id,gender,age,weight,backsq,deadlift,candj,snatch,pullups\n");
        for i in 0..300 {
            // 180 lb men aged 30, squats 200-499 lb, pull-ups 1-30; plus junk rows.
            csv += &format!("{i},Male,30,180,{},,,,{}\n", 200 + i, 1 + i % 30);
        }
        csv += "900,Male,30,180,5000,,,,\n901,,30,180,300,,,,\n902,Female,12,100,100,,,,\n";
        let t = crossfit_tables(csv.as_bytes()).unwrap();
        let w = Who { sex: Sex::Male, age: 30.0, bodyweight_kg: Some(180.0 * LB) };
        let sq = exercise_references("squat-barbell", &w, &[349.5 * LB], &[], Some(&t));
        assert_eq!(sq.len(), 2);
        let cf = &sq[1];
        assert_eq!(cf.title, "CrossFit athletes");
        assert!(cf.tiers.is_empty());
        assert!((cf.ranks[0].unwrap() - 50.0).abs() < 1.0, "{:?}", cf.ranks);
        let pull = &exercise_references("pull-up", &w, &[], &[15.0], Some(&t))[0];
        assert_eq!(pull.unit, "reps");
        assert!(pull.ranks[0].is_some());
    }

    #[test]
    fn all_metrics_resolve() {
        for m in METRICS {
            for sex in [Sex::Male, Sex::Female] {
                for age in [18.0, 30.0, 55.0, 90.0] {
                    let r = reference(m, &who(sex, age), &[1.0]).unwrap();
                    assert!(r.points.windows(2).all(|w| w[0].1 <= w[1].1), "{m} {sex:?} {age}");
                }
            }
        }
    }
}
