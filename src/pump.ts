/**
 * A measurement is "pumped" when it was taken after a workout started on the same day; muscles
 * measure bigger for a while after training. Times are local "YYYY-MM-DD HH:MM:SS" strings.
 */
export function isPumped(at: string, workoutStarts: string[]): boolean {
  const day = at.slice(0, 10);
  return workoutStarts.some((w) => w.slice(0, 10) === day && w <= at);
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
