import type { InBodyScan, MeasurementRow, WeighInRow } from "./api";
import { bodyFatSeries, type Pt, type Sex } from "./bodyfat";
import { parseLocal } from "./format";

/** How a stretch's weight change divides into lean and fat, from two body-fat readings. */
export interface PhaseSplit {
  /** Where the body-fat readings came from. */
  method: "inbody" | "navy" | "logged" | "rfm";
  /** The two readings the split is between (they may not span the whole stretch). */
  from: Date;
  to: Date;
  /** kg between the two readings. */
  weightKg: number;
  fatKg: number;
  leanKg: number;
}

const DAY = 86_400_000;
/** Readings up to this many days outside the stretch still count for it. */
const EDGE_DAYS = 14;
/** The two readings must be at least this far apart for the difference to mean anything. */
export const MIN_SPAN_DAYS = 21;
/** A body-fat reading needs a weigh-in within this many days. */
const WEIGHT_DAYS = 10;

/** Most to least trustworthy. RFM (height and waist only) is a last resort. */
const METHODS = ["inbody", "navy", "logged", "rfm"] as const;

export const METHOD_TEXT: Record<PhaseSplit["method"], string> = {
  inbody: "InBody scans",
  navy: "the Navy tape method (waist and neck)",
  logged: "body fat you logged",
  rfm: "waist and height (RFM)",
};

function nearest(rows: Pt[], t: Date, maxDays: number): number | null {
  let best: Pt | null = null;
  for (const r of rows) if (!best || Math.abs(+r.t - +t) < Math.abs(+best.t - +t)) best = r;
  return best && Math.abs(+best.t - +t) <= maxDays * DAY ? best.v : null;
}

/**
 * Lean and fat change over a stretch (`start`–`end`, local dates; `end` null = still running),
 * from the first and last body-fat readings of one method in it. One method for both ends, since
 * methods disagree with each other by more than a phase changes you. Null without two readings
 * `MIN_SPAN_DAYS` apart, each with a weigh-in nearby.
 */
export function phaseSplit(
  start: string,
  end: string | null,
  now: Date,
  measurements: MeasurementRow[],
  scans: InBodyScan[],
  weighIns: WeighInRow[],
  sex: Sex | null,
  heightCm: number | null,
): PhaseSplit | null {
  const s = bodyFatSeries(measurements, scans, weighIns, sex, heightCm);
  const series: Record<PhaseSplit["method"], Pt[]> = { inbody: s.measured, navy: s.navy, logged: s.logged, rfm: s.rfm };
  const lo = parseLocal(start).getTime() - EDGE_DAYS * DAY;
  const hi = (end ? parseLocal(end).getTime() + DAY : now.getTime()) + EDGE_DAYS * DAY;
  const scale: Pt[] = weighIns.filter((w) => w.source !== "inbody").map((w) => ({ t: parseLocal(w.at), v: w.weight_kg }));
  const scanWeight: Pt[] = scans
    .filter((x) => x.values.weight_kg != null)
    .map((x) => ({ t: parseLocal(x.measured_at_local), v: x.values.weight_kg }));
  for (const method of METHODS) {
    // A scan weighs you itself; other readings borrow the nearest weigh-in.
    const weightAt = (t: Date) => (method === "inbody" ? nearest(scanWeight, t, 0.5) : null) ?? nearest(scale, t, WEIGHT_DAYS);
    const pts = series[method]
      .filter((p) => +p.t >= lo && +p.t <= hi)
      .sort((a, b) => +a.t - +b.t)
      .map((p) => ({ t: p.t, bf: p.v / 100, kg: weightAt(p.t) }))
      .filter((p): p is { t: Date; bf: number; kg: number } => p.kg != null);
    if (pts.length < 2) continue;
    const [a, b] = [pts[0], pts[pts.length - 1]];
    if ((+b.t - +a.t) / DAY < MIN_SPAN_DAYS) continue;
    const fatKg = b.kg * b.bf - a.kg * a.bf;
    return { method, from: a.t, to: b.t, weightKg: b.kg - a.kg, fatKg, leanKg: b.kg - a.kg - fatKg };
  }
  return null;
}
