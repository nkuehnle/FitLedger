import type { DayHealth } from "./api";
import { isoDate } from "./charts/RangePicker";
import { parseLocal } from "./format";

/** "Slept 7h 12m · resting HR 52 (usual 54) · HRV 61 ms (usual 58) · 8,204 steps" for today, from the watch. */
export function recoveryText(days: DayHealth[], date: string): string | null {
  const d = days.find((x) => x.date === date);
  if (!d) return null;
  // The usual: average of the 4 weeks before today.
  const from = isoDate(new Date(parseLocal(date).getTime() - 28 * 86400000));
  const before = days.filter((x) => x.date < date && x.date >= from);
  const usual = (f: (x: DayHealth) => number | null) => {
    const v = before.map(f).filter((x): x is number => x != null);
    return v.length >= 7 ? Math.round(v.reduce((a, b) => a + b, 0) / v.length) : null;
  };
  const withUsual = (label: string, v: number, u: number | null, unit = "") => `${label} ${Math.round(v)}${unit}${u != null ? ` (usual ${u})` : ""}`;
  const parts = [
    d.sleep_s != null && `Slept ${Math.floor(d.sleep_s / 3600)}h ${String(Math.round((d.sleep_s % 3600) / 60)).padStart(2, "0")}m`,
    d.resting_hr != null && withUsual("resting HR", d.resting_hr, usual((x) => x.resting_hr)),
    d.hrv_ms != null && withUsual("HRV", d.hrv_ms, usual((x) => x.hrv_ms), " ms"),
    d.steps != null && `${d.steps.toLocaleString()} steps`,
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}
