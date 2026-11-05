//! Where a measurement stands against published reference data (percentiles), and strength tiers.
//!
//! Tables in `data/norms/` are built by `scripts/build_norms.py`:
//! - body composition: Kelly et al. 2009, NHANES DXA (US adults 1999-2004), LMS curves;
//! - body size: CDC anthropometric reference data, NHANES 2015-2018, percentiles by age band;
//! - strength: OpenPowerlifting, raw drug-tested competitors, each lifter's best per age band.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const BODY_COMPOSITION: &str = include_str!("../../../data/norms/body_composition.json");
const BODY_SIZE: &str = include_str!("../../../data/norms/body_size.json");
const STRENGTH: &str = include_str!("../../../data/norms/strength.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Kelly reference group ("white", "black", "mexican_american"); none averages all three.
    pub group: Option<String>,
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
}

/// Measures with reference data.
pub const METRICS: &[&str] = &["fmi", "pbf", "lmi", "almi", "weight_kg", "bmi", "waist_cm", "arm_cm", "squat", "bench", "deadlift"];

/// Percentiles are reported between these; the tails of the tables aren't reliable.
const MIN_PCT: f64 = 1.0;
const MAX_PCT: f64 = 99.0;

pub fn reference(metric: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    match metric {
        "fmi" | "pbf" | "lmi" | "almi" => composition(metric, who, values),
        "weight_kg" | "bmi" | "waist_cm" | "arm_cm" => size(metric, who, values),
        "squat" | "bench" | "deadlift" => strength(metric, who, values),
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
    let groups: Vec<&Vec<[f64; 4]>> = match who.group.as_deref() {
        Some("white") => vec![&g.white],
        Some("black") => vec![&g.black],
        Some("mexican_american") => vec![&g.mexican_american],
        _ => vec![&g.white, &g.black, &g.mexican_american],
    };
    let curves: Vec<_> = groups.iter().map(|rows| lms_at(rows, who.age)).collect();
    // Without a group: the average percentile over the three groups.
    let pct = |x: f64| curves.iter().map(|c| normal_cdf(lms_z(x, *c))).sum::<f64>() / curves.len() as f64 * 100.0;
    let ranks = values.iter().map(|&x| (x > 0.0).then(|| pct(x).clamp(MIN_PCT, MAX_PCT))).collect();
    // Points: invert the (averaged) percentile by bisection.
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
    let group = match who.group.as_deref() {
        Some("white") => "white ",
        Some("black") => "Black ",
        Some("mexican_american") => "Mexican American ",
        _ => "",
    };
    Some(Reference {
        points,
        ranks,
        tiers: vec![],
        tier_of: vec![None; values.len()],
        population: format!("{group}US {} aged about {:.0}, measured by DXA scan", sex_word(who.sex), who.age.clamp(20.0, 85.0)),
        source: "Kelly, Wilson & Heymsfield 2009, NHANES 1999–2004 (PLoS ONE 4: e7038)".into(),
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
    })
}

// ---------- Strength ----------

#[derive(Deserialize)]
struct StrengthTables {
    percentiles: Vec<f64>,
    age_from: Vec<f64>,
    bw_from: Vec<f64>,
    squat: StrengthSex,
    bench: StrengthSex,
    deadlift: StrengthSex,
}

#[derive(Deserialize)]
struct StrengthSex {
    male: Vec<Vec<Option<Cell>>>,
    female: Vec<Vec<Option<Cell>>>,
}

#[derive(Deserialize, Clone)]
struct Cell {
    p: Vec<f64>,
}

fn strength_tables() -> &'static StrengthTables {
    static T: OnceLock<StrengthTables> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(STRENGTH).expect("strength.json"))
}

/// Tier names and how each starts, from the competitor percentiles:
/// below half the median competitor is Beginner; Novice up to the 10th percentile; Intermediate
/// to the median; Advanced to the 90th; Elite above.
pub const TIERS: &[&str] = &["Beginner", "Novice", "Intermediate", "Advanced", "Elite"];

fn strength(metric: &str, who: &Who, values: &[f64]) -> Option<Reference> {
    let t = strength_tables();
    let bw = who.bodyweight_kg?;
    let by_sex = match metric {
        "squat" => &t.squat,
        "bench" => &t.bench,
        _ => &t.deadlift,
    };
    let grid = match who.sex {
        Sex::Male => &by_sex.male,
        Sex::Female => &by_sex.female,
    };
    let row = &grid[band_of(&t.age_from, who.age.max(t.age_from[0]))];
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
    let points: Vec<(f64, f64)> = t.percentiles.iter().copied().zip(p.iter().copied()).collect();
    let at = |pct: f64| points.iter().find(|q| q.0 == pct).map(|q| q.1).unwrap();
    let median = at(50.0);
    let tiers: Vec<(String, f64)> =
        TIERS.iter().zip([0.0, median * 0.5, at(10.0), median, at(90.0)]).map(|(n, v)| (n.to_string(), v)).collect();
    let tier_of = values
        .iter()
        .map(|&x| (x > 0.0).then(|| tiers.iter().rposition(|(_, from)| x >= *from).unwrap_or(0)))
        .collect();
    let ranks = values.iter().map(|&x| (x > 0.0).then(|| rank_in(&points, x))).collect();
    let from = t.age_from[band_of(&t.age_from, who.age.max(t.age_from[0]))];
    let ages = match t.age_from.iter().position(|&a| a == from) {
        Some(i) if i + 1 < t.age_from.len() => format!("{}–{}", from, t.age_from[i + 1] - 1.0),
        _ => format!("{from}+"),
    };
    Some(Reference {
        points,
        ranks,
        tiers,
        tier_of,
        population: format!(
            "{} in raw, drug-tested powerlifting competitions, aged {ages}, around {:.0} kg bodyweight",
            sex_word(who.sex),
            bw
        ),
        source: "OpenPowerlifting (openpowerlifting.org), best competition lift per lifter".into(),
    })
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

    fn who(sex: Sex, age: f64, group: Option<&str>) -> Who {
        Who { sex, age, group: group.map(String::from), bodyweight_kg: Some(85.0) }
    }

    #[test]
    fn lms_median_is_50th_percentile() {
        // Kelly Table S1, white men aged 20: FMI median 5.95.
        let r = reference("fmi", &who(Sex::Male, 20.0, Some("white")), &[5.95]).unwrap();
        assert!((r.ranks[0].unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn lms_matches_published_sigma() {
        // Table S5, white men aged 20: LMI M 18.98, sigma 2.50, L -1.115. The value one S*M
        // above in the Box-Cox scale is z = 1 by construction.
        let (l, m, s): (f64, f64, f64) = (-1.115, 18.98, 2.50 / 18.98);
        let x = m * (1.0 + l * s).powf(1.0 / l);
        let r = reference("lmi", &who(Sex::Male, 20.0, Some("white")), &[x]).unwrap();
        assert!((r.ranks[0].unwrap() - 84.13).abs() < 0.05, "{:?}", r.ranks);
    }

    #[test]
    fn lms_interpolates_age() {
        // Table S2: the median at 22.5 lies between the published medians at 20 and 25.
        let w = |age| reference("pbf", &who(Sex::Female, age, Some("white")), &[]).unwrap().points[4].1;
        let (a, b, mid) = (w(20.0), w(25.0), w(22.5));
        assert!(mid > a.min(b) && mid < a.max(b));
    }

    #[test]
    fn group_average_lies_between_groups() {
        let one = |g| reference("almi", &who(Sex::Male, 30.0, Some(g)), &[8.5]).unwrap().ranks[0].unwrap();
        let all = reference("almi", &who(Sex::Male, 30.0, None), &[8.5]).unwrap().ranks[0].unwrap();
        let rs = [one("white"), one("black"), one("mexican_american")];
        assert!((all - rs.iter().sum::<f64>() / 3.0).abs() < 1e-9);
    }

    #[test]
    fn cdc_table_points() {
        // Table 5: men 20-29, weight 50th percentile 81.3 kg, 95th 128.1 kg.
        let r = reference("weight_kg", &who(Sex::Male, 25.0, None), &[81.3, 128.1, 200.0, 40.0]).unwrap();
        assert_eq!(r.ranks, vec![Some(50.0), Some(95.0), Some(95.0), Some(5.0)]);
        // Table 19: women 40-49 waist, 25th 85.9, 50th 96.1: halfway is the 37.5th.
        let r = reference("waist_cm", &who(Sex::Female, 45.0, None), &[(85.9 + 96.1) / 2.0]).unwrap();
        assert!((r.ranks[0].unwrap() - 37.5).abs() < 1e-9);
        assert!(r.population.contains("40–49"));
    }

    #[test]
    fn strength_tiers_ascend() {
        let r = reference("squat", &who(Sex::Male, 28.0, None), &[60.0, 150.0, 200.0, 300.0]).unwrap();
        assert_eq!(r.tiers.len(), 5);
        assert!(r.tiers.windows(2).all(|w| w[0].1 < w[1].1));
        let tiers: Vec<_> = r.tier_of.iter().map(|t| TIERS[t.unwrap()]).collect();
        assert_eq!(tiers[0], "Beginner");
        assert_eq!(tiers[3], "Elite");
        assert!(r.ranks[2].unwrap() > 40.0 && r.ranks[2].unwrap() < 60.0, "{:?}", r.ranks);
    }

    #[test]
    fn strength_needs_bodyweight() {
        let mut w = who(Sex::Female, 30.0, None);
        w.bodyweight_kg = None;
        assert!(reference("bench", &w, &[50.0]).is_none());
    }

    #[test]
    fn all_metrics_resolve() {
        for m in METRICS {
            for sex in [Sex::Male, Sex::Female] {
                for age in [18.0, 30.0, 55.0, 90.0] {
                    let r = reference(m, &who(sex, age, None), &[1.0]).unwrap();
                    assert!(r.points.windows(2).all(|w| w[0].1 <= w[1].1), "{m} {sex:?} {age}");
                }
            }
        }
    }
}
