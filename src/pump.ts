import { parseLocal } from "./format";

/** How long after a workout starts a measurement still counts as pumped. */
export const PUMP_HOURS = 6;

/**
 * Muscles under each tape measurement (library muscle ids). A part is only pumped by a workout that
 * trained one of them; parts not listed (waist, abs, body fat) are never counted as pumped.
 */
export const PUMP_MUSCLES: Record<string, string[]> = {
  neck: ["neck", "traps"],
  shoulders: ["front_delts", "side_delts", "rear_delts"],
  // The tape goes around the back too.
  chest: ["upper_chest", "lower_chest", "lats", "upper_back"],
  // The tape goes around the triceps too.
  left_bicep: ["biceps", "triceps"],
  right_bicep: ["biceps", "triceps"],
  left_forearm: ["forearm_flexors", "forearm_extensors"],
  right_forearm: ["forearm_flexors", "forearm_extensors"],
  hips: ["glutes"],
  left_thigh: ["quads", "hamstrings", "adductors", "abductors"],
  right_thigh: ["quads", "hamstrings", "adductors", "abductors"],
  left_calf: ["calves"],
  right_calf: ["calves"],
};

/**
 * A measurement of `part` is "pumped" when it was taken within `PUMP_HOURS` after the start of a
 * workout that trained a muscle under the tape. Times are local "YYYY-MM-DD HH:MM:SS" strings.
 */
export function isPumped(at: string, part: string, workouts: [string, number | null, string[]][]): boolean {
  const muscles = PUMP_MUSCLES[part];
  if (!muscles) return false;
  const t = +parseLocal(at);
  return workouts.some(([start, , trained]) => {
    const s = +parseLocal(start);
    return t >= s && t <= s + PUMP_HOURS * 3600000 && trained.some((m) => muscles.includes(m));
  });
}

interface Reading {
  t: Date;
  v: number;
}

const DAY = 86400000;

/**
 * Your own pump, from your data: the median of pumped ÷ nearest not-pumped reading of the same part
 * taken within `days` days. Null with fewer than `minPairs` pairs; published studies don't give a
 * usable typical figure for tape measurements.
 */
export function pumpRatio(rested: Reading[], pumped: Reading[], days = 7, minPairs = 3): { ratio: number; pairs: number } | null {
  const ratios: number[] = [];
  for (const p of pumped) {
    let best: Reading | null = null;
    for (const r of rested) if (!best || Math.abs(+r.t - +p.t) < Math.abs(+best.t - +p.t)) best = r;
    if (best && Math.abs(+best.t - +p.t) <= days * DAY && best.v > 0) ratios.push(p.v / best.v);
  }
  if (ratios.length < minPairs) return null;
  ratios.sort((a, b) => a - b);
  const m = ratios.length >> 1;
  return { ratio: ratios.length % 2 ? ratios[m] : (ratios[m - 1] + ratios[m]) / 2, pairs: ratios.length };
}
