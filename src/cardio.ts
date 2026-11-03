import type { CardioSession } from "./api";
import { formatClock, parseLocal } from "./format";

/** How an activity's speed reads best: runners think in pace, cyclists in speed, rowers in /500 m. */
export type PaceMode = "pace" | "speed" | "split500";

export function paceMode(name: string): PaceMode {
  const n = name.toLowerCase();
  if (/cycl|bike|spin|elliptical/.test(n)) return "speed";
  if (/row|ski ?erg/.test(n)) return "split500";
  return "pace";
}

const M_PER_MI = 1609.344;

/** Distance in the display unit: miles with lb, km with kg. */
export function distText(m: number, unit: "kg" | "lb"): string {
  const v = unit === "lb" ? m / M_PER_MI : m / 1000;
  return `${v >= 10 ? v.toFixed(1) : v.toFixed(2)} ${unit === "lb" ? "mi" : "km"}`;
}

/** "8:13 /mi", "17.5 mph" or "2:05 /500m"; null without both distance and time. */
export function paceText(distanceM: number | null, durationS: number | null, mode: PaceMode, unit: "kg" | "lb"): string | null {
  if (!distanceM || !durationS || distanceM <= 0 || durationS <= 0) return null;
  if (mode === "speed") {
    const perHour = (distanceM / durationS) * 3600;
    return unit === "lb" ? `${(perHour / M_PER_MI).toFixed(1)} mph` : `${(perHour / 1000).toFixed(1)} km/h`;
  }
  if (mode === "split500") return `${formatClock((durationS / distanceM) * 500)} /500m`;
  const per = unit === "lb" ? M_PER_MI : 1000;
  return `${formatClock((durationS / distanceM) * per)} /${unit === "lb" ? "mi" : "km"}`;
}

/** Seconds per mile/km (or per 500 m), for charts where lower is faster; speed modes give mph/km/h. */
export function paceValue(distanceM: number, durationS: number, mode: PaceMode, unit: "kg" | "lb"): number {
  if (mode === "speed") return (distanceM / durationS) * 3600 / (unit === "lb" ? M_PER_MI : 1000);
  if (mode === "split500") return (durationS / distanceM) * 500;
  return (durationS / distanceM) * (unit === "lb" ? M_PER_MI : 1000);
}

/** Monday of the week a session falls in, YYYY-MM-DD. */
function weekOf(at: string): string {
  const d = parseLocal(at);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export interface CardioWeek {
  week: string;
  minutes: number;
  distanceM: number;
  sessions: number;
}

/** Minutes, distance and sessions per week for the last `weeks` weeks (empty weeks included). */
export function weeklyCardio(sessions: CardioSession[], weeks: number, today = new Date()): CardioWeek[] {
  const by = new Map<string, CardioWeek>();
  for (const s of sessions) {
    const k = weekOf(s.at);
    const w = by.get(k) ?? { week: k, minutes: 0, distanceM: 0, sessions: 0 };
    w.minutes += (s.duration_s ?? 0) / 60;
    w.distanceM += s.distance_m ?? 0;
    w.sessions += 1;
    by.set(k, w);
  }
  const p = (n: number) => String(n).padStart(2, "0");
  const out: CardioWeek[] = [];
  const monday = new Date(today);
  monday.setHours(0, 0, 0, 0);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  for (let i = weeks - 1; i >= 0; i--) {
    const d = new Date(monday);
    d.setDate(d.getDate() - 7 * i);
    const k = `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
    out.push(by.get(k) ?? { week: k, minutes: 0, distanceM: 0, sessions: 0 });
  }
  return out;
}
