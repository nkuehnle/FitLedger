import type { EnergyPoint, Goal, WeighInRow } from "./api";
import { parseLocal } from "./format";

export const GOAL_TEXT: Record<Goal, string> = { cut: "Cut", maintain: "Maintain", gain: "Gain" };
const KCAL_PER_KG = 7700;

export interface GoalInfo {
  goal: Goal | null;
  rate_pct: [number, number] | null;
  weekly_rate_kg: number | null;
  start_date: string | null;
}

export interface GoalStatus {
  goal: Goal;
  /** Target weekly change in kg (low, high); equal when the user set their own target. */
  targetKg: [number, number];
  /** Actual weekly change in kg over the last ~3 weeks, if there are enough weigh-ins. */
  actualKg: number | null;
  onTrack: boolean | null;
  /** Daily energy balance the target implies, and how far the latest balance is from it. */
  targetKcal: number;
  balanceKcal: number | null;
  offKcal: number | null;
  /** From logged calories + weight change: average intake, maintenance, and what to eat for the target. */
  intakeKcal: number | null;
  tdeeKcal: number | null;
  eatKcal: number | null;
}

/** Where the current goal stands, from weigh-ins (and the energy balance when diet is logged). */
export function goalStatus(info: GoalInfo | null, rows: WeighInRow[], energy: EnergyPoint[]): GoalStatus | null {
  if (!info?.goal || !info.rate_pct) return null;
  const scale = rows.filter((r) => r.source !== "inbody");
  if (!scale.length) return null;
  const bw = scale[scale.length - 1].weight_kg;
  const targetKg: [number, number] =
    info.weekly_rate_kg != null
      ? [info.weekly_rate_kg, info.weekly_rate_kg]
      : [(bw * info.rate_pct[0]) / 100, (bw * info.rate_pct[1]) / 100];

  const now = Date.now();
  const age = (r: WeighInRow) => now - parseLocal(r.at).getTime();
  const avg = (rs: WeighInRow[]) => rs.reduce((n, r) => n + r.weight_kg, 0) / rs.length;
  const first = scale.filter((r) => age(r) < 28 * 86400000 && age(r) >= 21 * 86400000);
  const last = scale.filter((r) => age(r) < 7 * 86400000);
  const actualKg = first.length && last.length ? (avg(last) - avg(first)) / 3 : null;
  // A custom target gets a ±0.1 kg/week tolerance.
  const [lo, hi] = targetKg[0] === targetKg[1] ? [targetKg[0] - 0.1, targetKg[0] + 0.1] : targetKg;
  const onTrack = actualKg == null ? null : actualKg >= lo && actualKg <= hi;

  const targetKcal = (((targetKg[0] + targetKg[1]) / 2) * KCAL_PER_KG) / 7;
  const latest = [...energy].reverse();
  // Without an energy-balance point, the 3-week weigh-in rate still gives the balance.
  const balanceKcal =
    latest.find((p) => p.balance_kcal != null)?.balance_kcal ?? (actualKg == null ? null : (actualKg * KCAL_PER_KG) / 7);
  // Maintenance needs recent intake; an estimate older than 3 weeks is stale.
  const withTdee = latest.find((p) => p.tdee_kcal != null && now - parseLocal(p.date).getTime() < 21 * 86400000);
  const tdeeKcal = withTdee?.tdee_kcal ?? null;
  return {
    goal: info.goal,
    targetKg,
    actualKg,
    onTrack,
    targetKcal,
    balanceKcal,
    offKcal: balanceKcal == null ? null : balanceKcal - targetKcal,
    intakeKcal: withTdee?.avg_intake_kcal ?? null,
    tdeeKcal,
    eatKcal: tdeeKcal == null ? null : tdeeKcal + targetKcal,
  };
}
