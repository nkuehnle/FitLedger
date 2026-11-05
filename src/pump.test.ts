import { describe, expect, it } from "vitest";
import { isPumped, pumpRatio } from "./pump";

describe("isPumped", () => {
  const workouts: [string, number | null, string[]][] = [
    ["2026-11-02 17:30:00", 3600, ["biceps", "lats"]],
    ["2026-11-03 17:30:00", 3600, ["quads", "glutes"]],
    ["2026-11-04 07:00:00", null, ["triceps"]],
  ];
  it("is pumped up to 6 hours after a workout that trained the muscle", () => {
    expect(isPumped("2026-11-02 17:45:00", "left_bicep", workouts)).toBe(true);
    expect(isPumped("2026-11-02 20:30:00", "right_bicep", workouts)).toBe(true);
    expect(isPumped("2026-11-02 19:00:00", "chest", workouts)).toBe(true);
    expect(isPumped("2026-11-02 23:15:00", "left_bicep", workouts)).toBe(true);
    expect(isPumped("2026-11-04 08:00:00", "left_bicep", workouts)).toBe(true);
  });
  it("is not pumped by a workout for other muscles, over 6 hours later, before it, or for the waist", () => {
    expect(isPumped("2026-11-03 18:30:00", "neck", workouts)).toBe(false);
    expect(isPumped("2026-11-03 18:30:00", "left_thigh", workouts)).toBe(true);
    expect(isPumped("2026-11-02 23:45:00", "left_bicep", workouts)).toBe(false);
    expect(isPumped("2026-11-02 07:10:00", "left_bicep", workouts)).toBe(false);
    expect(isPumped("2026-11-02 18:00:00", "waist", workouts)).toBe(false);
  });
});

describe("pumpRatio", () => {
  const d = (day: number) => new Date(2026, 10, day, 8);
  const rested = [d(1), d(8), d(15), d(22)].map((t) => ({ t, v: 40 }));
  it("is the median of pumped ÷ nearest rested within a week", () => {
    const pumped = [
      { t: d(2), v: 41 },
      { t: d(9), v: 40.8 },
      { t: d(16), v: 42 },
    ];
    expect(pumpRatio(rested, pumped)!.ratio).toBeCloseTo(41 / 40);
    expect(pumpRatio(rested, pumped)!.pairs).toBe(3);
  });
  it("needs enough pairs close in time", () => {
    expect(pumpRatio(rested, [{ t: d(2), v: 41 }])).toBeNull();
    const far = [40, 50, 60].map((day) => ({ t: d(day), v: 41 }));
    expect(pumpRatio(rested, far)).toBeNull();
  });
});
