import { describe, expect, it } from "vitest";
import type { WeighInRow } from "../api";
import { goalCorridors, phaseLines } from "./corridor";

const row = (at: string, weight_kg: number, source = "manual"): WeighInRow => ({ id: 0, at, weight_kg, source }) as WeighInRow;
const same = (kg: number) => kg;

describe("goalCorridors", () => {
  const rows = [row("2026-10-31 08:00:00", 80.2), row("2026-11-02 08:00:00", 79.8), row("2026-11-09 08:00:00", 80.4)];

  it("starts at the weigh-ins around the goal's first day and widens at the default rate", () => {
    const [c] = goalCorridors(
      [{ goal: "gain", start_date: "2026-11-02", end_date: null, weekly_rate_kg: null }],
      rows,
      same,
      new Date(2026, 10, 30),
    );
    expect(c.v0).toBeCloseTo(80, 6);
    // +0.25 to +0.5 % of 80 kg a week.
    expect(c.lo * 7).toBeCloseTo(0.2, 6);
    expect(c.hi * 7).toBeCloseTo(0.4, 6);
    expect(c.to).toEqual(new Date(2026, 10, 30));
  });

  it("uses a custom weekly rate with a tolerance, in display units", () => {
    const toLb = (kg: number) => kg * 2.20462;
    const [c] = goalCorridors(
      [{ goal: "gain", start_date: "2026-11-02", end_date: "2026-12-01", weekly_rate_kg: 0.25 }],
      rows,
      toLb,
      new Date(),
    );
    expect(c.lo * 7).toBeCloseTo(toLb(0.15), 6);
    expect(c.hi * 7).toBeCloseTo(toLb(0.35), 6);
    expect(c.v0).toBeCloseTo(toLb(80), 6);
  });

  it("draws maintenance as a level band", () => {
    const [c] = goalCorridors(
      [{ goal: "maintain", start_date: "2026-11-02", end_date: null, weekly_rate_kg: null }],
      rows,
      same,
      new Date(),
    );
    expect([c.lo, c.hi]).toEqual([0, 0]);
    expect(c.half).toBeCloseTo(0.8, 6);
  });

  it("skips a goal with no weigh-in near its start, and ignores InBody weights", () => {
    const goals = [{ goal: "cut" as const, start_date: "2026-06-01", end_date: "2026-08-01", weekly_rate_kg: null }];
    expect(goalCorridors(goals, [...rows, row("2026-06-01 08:00:00", 85, "inbody")], same, new Date())).toEqual([]);
  });
});

describe("phaseLines", () => {
  it("lines up each stretch of a goal at its own start, as weekly averages", () => {
    const rows = [
      row("2026-01-01 08:00:00", 80),
      row("2026-01-09 08:00:00", 80.4),
      row("2026-01-10 08:00:00", 80.6),
      row("2026-01-16 08:00:00", 81),
      row("2026-06-01 08:00:00", 78),
      row("2026-06-09 08:00:00", 78.3),
    ];
    const goals = [
      { goal: "gain" as const, start_date: "2026-01-01", end_date: "2026-02-01", weekly_rate_kg: null },
      { goal: "cut" as const, start_date: "2026-02-02", end_date: "2026-05-31", weekly_rate_kg: null },
      { goal: "gain" as const, start_date: "2026-06-01", end_date: null, weekly_rate_kg: null },
    ];
    const lines = phaseLines(goals, rows, "gain", new Date(2026, 5, 12));
    expect(lines.map((l) => l.current)).toEqual([false, true]);
    // Week 1 of the first gain averages 80.5, half a kilo up on its start.
    expect(lines[0].points[2].y).toBeCloseTo(0.5, 6);
    expect(lines[0].points[2].x).toBeCloseTo((8.5 + 1 / 3) / 7, 6);
    expect(lines[1].points.at(-1)!.y).toBeCloseTo(0.3, 6);
    expect(phaseLines(goals, rows, "cut", new Date(2026, 5, 12))).toEqual([]);
  });
});
