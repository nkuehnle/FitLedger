//! Reads the printed InBody 580 result sheet (as exported to PNG/JPEG). The layout is fixed, so
//! each value is read from its own box (relative to the page size), digits only, and the totals
//! are cross-checked.

use std::collections::BTreeMap;

use ocrs::{DecodeMethod, ImageSource, OcrEngine, OcrEngineParams};
use rten_tensor::prelude::*;
use rten_tensor::NdTensor;
use rten_imageproc::{RectF, RotatedRect};
use serde::Serialize;

use crate::{models, Error};

/// Reference page size the boxes were measured on.
const W: f32 = 1563.0;
const H: f32 = 2000.0;

/// (field, x0, y0, x1, y1) in reference pixels. Masses are in the sheet's unit (lb or kg).
const BOXES: &[(&str, f32, f32, f32, f32)] = &[
    ("icw_l", 285.0, 295.0, 440.0, 335.0),
    ("ecw_l", 285.0, 345.0, 440.0, 387.0),
    ("body_fat_mass_kg", 285.0, 447.0, 440.0, 490.0),
    ("total_body_water_l", 452.0, 320.0, 622.0, 356.0),
    ("fat_free_mass_kg", 628.0, 346.0, 797.0, 381.0),
    ("weight_kg", 803.0, 373.0, 975.0, 408.0),
    ("smm_kg", 280.0, 667.0, 968.0, 697.0),
    ("bmi", 280.0, 873.0, 968.0, 901.0),
    ("pbf", 280.0, 933.0, 968.0, 961.0),
    ("ecw_tbw", 280.0, 1510.0, 870.0, 1545.0),
    ("inbody_score", 1140.0, 312.0, 1270.0, 378.0),
    ("bmr_kcal", 1218.0, 1186.0, 1285.0, 1216.0),
];

/// Segmental rows, top to bottom (lean rows 68.8 px apart, fat rows 33.5 px apart).
const SEGMENTS: &[&str] = &["right_arm", "left_arm", "trunk", "right_leg", "left_leg"];

#[derive(Debug, Clone, Serialize)]
pub struct InBodyRead {
    /// "YYYY-MM-DD HH:MM:SS" when the test date could be read.
    pub measured_at: Option<String>,
    /// Field column (see the core INBODY_FIELDS) → value in the sheet's units.
    pub values: BTreeMap<String, f64>,
    /// Fields that couldn't be read or failed a cross-check.
    pub doubtful: Vec<String>,
    /// Unit of the masses on the sheet ("lb" or "kg"), from the printed "(lb)" labels.
    pub mass_unit: Option<&'static str>,
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32, sx: f32, sy: f32) -> (f32, f32, f32, f32) {
    (x0 * sx, y0 * sy, x1 * sx, y1 * sy)
}

/// The printed number in the text. The sheet prints each field with a fixed number of decimals,
/// so a decimal point the OCR missed ("439" for 43.9) can be put back.
pub fn number(text: &str, decimals: u32) -> Option<f64> {
    let tokens: Vec<String> = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|t| t.trim_matches('.').to_string())
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
        .collect();
    // Prefer a token that already has a decimal point when one is expected.
    let tok = if decimals > 0 { tokens.iter().find(|t| t.contains('.')).or(tokens.first()) } else { tokens.first() }?;
    if tok.contains('.') || decimals == 0 {
        return tok.parse().ok();
    }
    if tok.len() as u32 <= decimals {
        return None;
    }
    Some(tok.parse::<f64>().ok()? / 10f64.powi(decimals as i32))
}

/// Decimals each field is printed with (lb sheet; segmental lean mass has two).
fn decimals(field: &str) -> u32 {
    match field {
        "inbody_score" | "bmr_kcal" => 0,
        "ecw_tbw" => 3,
        f if f.starts_with("lean_") && f.ends_with("_kg") => 2,
        _ => 1,
    }
}

pub fn read_inbody580(image_bytes: &[u8]) -> Result<InBodyRead, Error> {
    let img = image::load_from_memory(image_bytes).map_err(|e| Error(format!("couldn't read the image: {e}")))?.into_rgb8();
    let img = &img;
    let (width, height) = img.dimensions();
    let aspect = width as f32 / height as f32;
    if (aspect - W / H).abs() > 0.03 {
        return Err(Error("this doesn't look like a full InBody 580 result sheet (unexpected page shape)".into()));
    }
    let (detection, recognition) = models()?;
    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection),
        recognition_model: Some(recognition),
        allowed_chars: Some("0123456789.:%()|lbkgcal ".into()),
        decode_method: DecodeMethod::BeamSearch { width: 20 },
        ..Default::default()
    })
    .map_err(|e| Error(e.to_string()))?;
    let (sx, sy) = (width as f32 / W, height as f32 / H);

    // Crops the box (plus a margin), scales it up and reads it on its own, so neighbouring
    // labels and bars can't merge into the number.
    let read_with = |b: (f32, f32, f32, f32), detect: bool| -> Option<String> {
        let pad = 3.0;
        let x0 = (b.0 - pad).max(0.0) as u32;
        let y0 = (b.1 - pad).max(0.0) as u32;
        let x1 = ((b.2 + pad) as u32).min(width);
        let y1 = ((b.3 + pad) as u32).min(height);
        let crop = image::imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image();
        let scale = (48.0 / (y1 - y0) as f32).clamp(1.0, 3.0);
        let crop = image::imageops::resize(
            &crop,
            ((x1 - x0) as f32 * scale) as u32,
            ((y1 - y0) as f32 * scale) as u32,
            image::imageops::FilterType::CatmullRom,
        );
        // White border so text doesn't touch the edge.
        let mut canvas = image::RgbImage::from_pixel(crop.width() + 40, crop.height() + 40, image::Rgb([255, 255, 255]));
        image::imageops::overlay(&mut canvas, &crop, 20, 20);
        let (cw, ch) = canvas.dimensions();
        let canvas_ink = canvas.clone();
        let tensor = NdTensor::from_data([ch as usize, cw as usize, 3], canvas.into_vec());
        let source = ImageSource::from_tensor(tensor.view(), ocrs::DimOrder::Hwc).ok()?;
        let input = engine.prepare_input(source).ok()?;
        if detect {
            let words = engine.detect_words(&input).ok()?;
            let lines = engine.find_text_lines(&input, &words);
            let texts = engine.recognize_text(&input, &lines).ok()?;
            let text: Vec<String> = texts.into_iter().flatten().map(|l| l.to_string()).collect();
            return (!text.is_empty()).then(|| text.join(" "));
        }
        // Fast path: the position is known, so skip text detection (the slow part) and
        // recognise the box around the ink directly.
        let (mut l, mut t, mut r, mut b) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y, p) in canvas_ink.enumerate_pixels() {
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 < 160 {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
            }
        }
        if r <= l || b <= t {
            return None;
        }
        let m = 3.0;
        let line = RotatedRect::from_rect(RectF::from_tlbr(t as f32 - m, l as f32 - m, b as f32 + m, r as f32 + m));
        let texts = engine.recognize_text(&input, &[vec![line]]).ok()?;
        let text: Vec<String> = texts.into_iter().flatten().map(|l| l.to_string()).collect();
        (!text.is_empty()).then(|| text.join(" "))
    };
    let read = |b: (f32, f32, f32, f32)| read_with(b, false);

    // Where a horizontal bar that starts at x0 ends, on the row through yc (reference coords).
    // Bars are solid over a few rows and drawn in segments with 2–3 px gaps; the printed value
    // follows 10+ px after the bar, so its digits aren't mistaken for bar.
    let bar_end = |x0: f32, yc: f32, x_max: f32| -> f32 {
        let dark = |x: u32, y: u32| {
            let p = img.get_pixel(x.min(width - 1), y.min(height - 1));
            (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 < 170
        };
        let solid = |x: u32| {
            let y = (yc * sy) as u32;
            (y.saturating_sub(3)..=y + 3).all(|y| dark(x, y))
        };
        let (start, stop) = ((x0 * sx) as u32, (x_max * sx) as u32);
        let max_gap = (5.0 * sx).ceil() as u32;
        let (mut last, mut x) = (start, start);
        while x < stop && x - last <= max_gap {
            if solid(x) {
                last = x;
            }
            x += 1;
        }
        last as f32 / sx
    };

    let mut values = BTreeMap::new();
    let mut doubtful = Vec::new();
    let debug = std::env::var_os("FITLEDGER_OCR_DEBUG").is_some();
    let mut boxes: BTreeMap<String, (f32, f32, f32, f32)> = BTreeMap::new();
    let parse = |field: &str, text: Option<String>, lo: f64, hi: f64| {
        if debug {
            eprintln!("{field}: {text:?}");
        }
        text.as_deref().and_then(|t| number(t, decimals(field))).filter(|v| *v >= lo && *v <= hi)
    };
    // Fast read first; if it doesn't give a plausible number, read again with text detection.
    let mut put = |field: &str, b: (f32, f32, f32, f32), lo: f64, hi: f64, values: &mut BTreeMap<String, f64>| {
        boxes.insert(field.to_string(), b);
        match parse(field, read(b), lo, hi).or_else(|| parse(field, read_with(b, true), lo, hi)) {
            Some(v) => {
                values.insert(field.to_string(), v);
            }
            None => doubtful.push(field.to_string()),
        }
    };
    for &(field, x0, y0, x1, y1) in BOXES {
        let (lo, hi) = match field {
            "ecw_tbw" => (0.3, 0.45),
            "bmi" => (12.0, 60.0),
            "pbf" => (2.0, 70.0),
            "inbody_score" => (30.0, 110.0),
            "bmr_kcal" => (800.0, 4000.0),
            _ => (5.0, 700.0),
        };
        put(field, rect(x0, y0, x1, y1, sx, sy), lo, hi, &mut values);
    }
    for (i, part) in SEGMENTS.iter().enumerate() {
        // Lean: kg bar (black) and % bar (gray) per row; each value is printed after its bar.
        let kg = 1080.5 + 68.8 * i as f32;
        let pct = 1102.8 + 68.8 * i as f32;
        let x = bar_end(282.0, kg, 772.0) + 4.0;
        put(&format!("lean_{part}_kg"), rect(x, kg - 9.0, 772.0, kg + 12.0, sx, sy), 1.0, 200.0, &mut values);
        let x = bar_end(282.0, pct, 772.0) + 4.0;
        put(&format!("lean_{part}_pct"), rect(x, pct - 9.0, 772.0, pct + 12.0, sx, sy), 40.0, 200.0, &mut values);
        // Fat: "(1.8 lb)" then a bar and its %.
        let fy = 1007.5 + 33.5 * i as f32;
        put(&format!("fat_{part}_kg"), rect(1160.0, fy - 14.0, 1252.0, fy + 14.0, sx, sy), 0.1, 100.0, &mut values);
        let x = bar_end(1253.0, fy, 1445.0) + 4.0;
        put(&format!("fat_{part}_pct"), rect(x, fy - 13.0, 1445.0, fy + 13.0, sx, sy), 10.0, 900.0, &mut values);
    }

    // Cross-checks: water and mass totals. A failing group is re-read with text detection once.
    let reread = |fields: &[&str], values: &mut BTreeMap<String, f64>| {
        for f in fields {
            if let Some(&b) = boxes.get(*f) {
                if let Some(v) = parse(f, read_with(b, true), 0.0, f64::MAX) {
                    values.insert(f.to_string(), v);
                }
            }
        }
    };
    let water_ok = |v: &BTreeMap<String, f64>| match (v.get("icw_l"), v.get("ecw_l"), v.get("total_body_water_l")) {
        (Some(a), Some(b), Some(t)) => (a + b - t).abs() <= 0.25,
        _ => true,
    };
    let mass_ok = |v: &BTreeMap<String, f64>| match (v.get("fat_free_mass_kg"), v.get("body_fat_mass_kg"), v.get("weight_kg")) {
        (Some(a), Some(b), Some(t)) => (a + b - t).abs() <= 0.25,
        _ => true,
    };
    if !water_ok(&values) {
        reread(&["icw_l", "ecw_l", "total_body_water_l"], &mut values);
    }
    if !mass_ok(&values) {
        reread(&["fat_free_mass_kg", "body_fat_mass_kg", "weight_kg"], &mut values);
    }
    let near = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol;
    let get = |k: &str| values.get(k).copied();
    if let (Some(icw), Some(ecw), Some(tbw)) = (get("icw_l"), get("ecw_l"), get("total_body_water_l")) {
        if !near(icw + ecw, tbw, 0.25) {
            doubtful.extend(["icw_l", "ecw_l", "total_body_water_l"].map(String::from));
        }
    }
    if let (Some(ffm), Some(bfm), Some(w)) = (get("fat_free_mass_kg"), get("body_fat_mass_kg"), get("weight_kg")) {
        if !near(ffm + bfm, w, 0.25) {
            doubtful.extend(["fat_free_mass_kg", "body_fat_mass_kg", "weight_kg"].map(String::from));
        }
    }
    if let (Some(bfm), Some(w), Some(pbf)) = (get("body_fat_mass_kg"), get("weight_kg"), get("pbf")) {
        if !near(bfm / w * 100.0, pbf, 0.3) {
            doubtful.push("pbf".into());
        }
    }
    doubtful.sort();
    doubtful.dedup();

    // Test date/time, e.g. "2025.04.20. 13:27".
    let measured_at = read(rect(688.0, 152.0, 915.0, 190.0, sx, sy)).and_then(|t| {
        let d: Vec<u32> = t.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
        match d[..] {
            [y, m, day, hh, mm, ..] if (2000..2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&day) && hh < 24 && mm < 60 => {
                Some(format!("{y:04}-{m:02}-{day:02} {hh:02}:{mm:02}:00"))
            }
            _ => None,
        }
    });
    let unit_text = read(rect(236.0, 298.0, 276.0, 330.0, sx, sy));
    if debug {
        eprintln!("unit: {unit_text:?}");
    }
    let mass_unit = unit_text.and_then(|t| {
        let t = t.to_ascii_lowercase();
        if t.contains("lb") || t.contains("1b") || t.contains("ib") {
            Some("lb")
        } else if t.contains("kg") {
            Some("kg")
        } else {
            None
        }
    });
    Ok(InBodyRead { measured_at, values, doubtful, mass_unit })
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn restores_missing_decimal_points() {
        assert_eq!(number("512", 1), Some(51.2));
        assert_eq!(number("(2.4 lb)", 1), Some(2.4));
        assert_eq!(number("1234", 2), Some(12.34));
        assert_eq!(number("0.381", 3), Some(0.381));
        assert_eq!(number("1700 kcal", 0), Some(1700.0));
        assert_eq!(number("7", 1), None);
    }
}
