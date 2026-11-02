import type { WeightUnit } from "./api";

const KG_PER_LB = 0.45359237;

export function toUnit(kg: number, unit: WeightUnit): number {
  return unit === "lb" ? kg / KG_PER_LB : kg;
}

/** "190", "27.5", "12.25" — at most two decimals, no trailing zeros. */
export function formatWeight(kg: number | null, unit: WeightUnit): string {
  if (kg == null) return "";
  return String(Math.round(toUnit(kg, unit) * 100) / 100);
}

export function formatVolume(kg: number, unit: WeightUnit): string {
  return `${Math.round(toUnit(kg, unit)).toLocaleString()} ${unit}`;
}

export function formatDuration(s: number | null): string {
  if (s == null) return "";
  const mins = Math.round(s / 60);
  const h = Math.floor(mins / 60);
  return h > 0 ? `${h}h ${mins % 60}m` : `${mins}m`;
}

/** Short durations for sets and cardio: "45s", "6:26", "1:02:05". */
export function formatClock(s: number | null): string {
  if (s == null) return "";
  const t = Math.round(s);
  if (t < 60) return `${t}s`;
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = String(t % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function formatDistance(m: number | null, unit: WeightUnit): string {
  if (m == null) return "";
  // Imperial weight users get miles, metric users km.
  return unit === "lb" ? `${(m / 1609.344).toFixed(2)} mi` : `${(m / 1000).toFixed(2)} km`;
}

/** Parses "YYYY-MM-DD HH:MM:SS" as a local wall-clock time. */
export function parseLocal(s: string): Date {
  const [d, t = "00:00:00"] = s.split(" ");
  const [y, mo, da] = d.split("-").map(Number);
  const [h, mi, se] = t.split(":").map(Number);
  return new Date(y, mo - 1, da, h, mi, se);
}

export function formatDate(s: string): string {
  return parseLocal(s).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: "numeric" });
}

export function formatTime(s: string): string {
  return parseLocal(s).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

export function setLabel(type: string, index: number): string {
  return { warmup: "W", drop: "D", failure: "F" }[type] ?? String(index);
}

export const EQUIPMENT_LABELS: Record<string, string> = {
  barbell: "Barbell",
  dumbbell: "Dumbbell",
  cable: "Cable",
  machine: "Machine",
  smith_machine: "Smith",
  landmine: "Landmine",
  kettlebell: "Kettlebell",
  band: "Band",
  bodyweight: "Bodyweight",
  cardio: "Cardio",
  ez_bar: "EZ bar",
  trap_bar: "Trap bar",
  plate: "Plate",
  medicine_ball: "Med ball",
  stability_ball: "Stability ball",
  other: "Other",
};

/** Now as "YYYY-MM-DD HH:MM:00" local time (the format weigh-ins are stored in). */
export function nowLocal(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:00`;
}
