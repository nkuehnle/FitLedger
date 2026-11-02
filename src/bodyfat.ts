import type { InBodyScan, MeasurementRow, WeighInRow } from "./api";
import { parseLocal } from "./format";

export type Sex = "male" | "female";

/** US Navy circumference method (cm). Women also need hips. */
export function navyBodyFat(sex: Sex, heightCm: number, waistCm: number, neckCm: number, hipsCm?: number): number | null {
  const log = Math.log10;
  if (sex === "male") {
    if (waistCm <= neckCm) return null;
    return 495 / (1.0324 - 0.19077 * log(waistCm - neckCm) + 0.15456 * log(heightCm)) - 450;
  }
  if (hipsCm == null || waistCm + hipsCm <= neckCm) return null;
  return 495 / (1.29579 - 0.35004 * log(waistCm + hipsCm - neckCm) + 0.221 * log(heightCm)) - 450;
}

/** Relative fat mass (Woolcott & Bergman 2018): height and waist only. */
export function relativeFatMass(sex: Sex, heightCm: number, waistCm: number): number {
  return 64 - (20 * heightCm) / waistCm + (sex === "female" ? 12 : 0);
}

export interface Pt {
  t: Date;
  v: number;
}

export interface BodyFatSeries {
  measured: Pt[];
  logged: Pt[];
  navy: Pt[];
  rfm: Pt[];
  /** Lean (fat-free) mass in kg: weight × (1 − body fat), and InBody's own reading. */
  leanEstimated: Pt[];
  leanMeasured: Pt[];
  smm: Pt[];
}

const DAY = 86400000;

/** The value of the reading closest in time to `t`, if one is within `maxDays`. */
function nearest(rows: Pt[], t: Date, maxDays: number): number | null {
  let best: Pt | null = null;
  for (const r of rows) if (!best || Math.abs(+r.t - +t) < Math.abs(+best.t - +t)) best = r;
  return best && Math.abs(+best.t - +t) <= maxDays * DAY ? best.v : null;
}

/**
 * Body fat over time from every source there is: InBody scans, body fat logged in Strong, and the
 * Navy and RFM estimates from tape measurements (each waist reading, with the nearest neck/hips
 * within two weeks). Lean mass uses the nearest weigh-in within a week of each estimate.
 */
export function bodyFatSeries(
  measurements: MeasurementRow[],
  scans: InBodyScan[],
  weighIns: WeighInRow[],
  sex: Sex | null,
  heightCm: number | null,
): BodyFatSeries {
  const part = (p: string): Pt[] =>
    measurements.filter((m) => m.part === p).map((m) => ({ t: parseLocal(m.at), v: m.value }));
  const scanPts = (k: string): Pt[] =>
    scans.filter((s) => s.values[k] != null).map((s) => ({ t: parseLocal(s.measured_at_local), v: s.values[k] }));
  const weights = weighIns.map((w) => ({ t: parseLocal(w.at), v: w.weight_kg }));

  const navy: Pt[] = [];
  const rfm: Pt[] = [];
  if (sex && heightCm) {
    const neck = part("neck");
    const hips = part("hips");
    for (const w of part("waist")) {
      rfm.push({ t: w.t, v: relativeFatMass(sex, heightCm, w.v) });
      const n = nearest(neck, w.t, 14);
      const h = nearest(hips, w.t, 14);
      const bf = n == null ? null : navyBodyFat(sex, heightCm, w.v, n, h ?? undefined);
      if (bf != null && bf > 2 && bf < 60) navy.push({ t: w.t, v: bf });
    }
  }
  const logged = part("body_fat_pct");
  const leanEstimated: Pt[] = [];
  for (const p of [...navy, ...logged].sort((a, b) => +a.t - +b.t)) {
    const kg = nearest(weights, p.t, 7);
    if (kg != null) leanEstimated.push({ t: p.t, v: kg * (1 - p.v / 100) });
  }
  const by = (a: Pt, b: Pt) => +a.t - +b.t;
  return {
    measured: scanPts("pbf").sort(by),
    logged: logged.sort(by),
    navy,
    rfm,
    leanEstimated,
    leanMeasured: scanPts("fat_free_mass_kg").sort(by),
    smm: scanPts("smm_kg").sort(by),
  };
}
