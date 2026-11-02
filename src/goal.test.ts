import { describe, expect, it } from "vitest";
import type { WeighInRow } from "./api";
import { goalStatus } from "./goal";

const daysAgo = (n: number): string => {
  const d = new Date(Date.now() - n * 86400000);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} 08:00:00`;
};
const row = (n: number, kg: number): WeighInRow => ({ id: n, at: daysAgo(n), weight_kg: kg, source: "manual" });
// Lost 1.5 kg over 3 weeks: −0.5 kg/week at ~100 kg.
const rows = [row(25, 101.5), row(24, 101.5), row(3, 100), row(2, 100)];
const cut = { goal: "cut" as const, rate_pct: [-1, -0.5] as [number, number], weekly_rate_kg: null, start_date: "2026-01-01" };

describe("goalStatus", () => {
  it("compares the 3-week rate with the default % range", () => {
    const s = goalStatus(cut, rows, [])!;
    expect(s.targetKg).toEqual([-1, -0.5]);
    expect(s.actualKg).toBeCloseTo(-0.5);
    expect(s.onTrack).toBe(true);
    expect(s.offKcal).toBeNull();
  });

  it("uses the user's own weekly target and the energy balance", () => {
    const s = goalStatus({ ...cut, weekly_rate_kg: -0.9 }, rows, [
      { date: "2026-11-01", trend_kg: 100, balance_kcal: -400, avg_intake_kcal: 2000, tdee_kcal: 2400 },
    ])!;
    expect(s.targetKg).toEqual([-0.9, -0.9]);
    expect(s.onTrack).toBe(false);
    expect(s.targetKcal).toBeCloseTo(-990);
    expect(s.offKcal).toBeCloseTo(590);
  });

  it("needs a goal and weigh-ins", () => {
    expect(goalStatus({ ...cut, goal: null, rate_pct: null }, rows, [])).toBeNull();
    expect(goalStatus(cut, [], [])).toBeNull();
  });
});
