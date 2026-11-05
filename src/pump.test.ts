import { describe, expect, it } from "vitest";
import { isPumped, pumpRatio } from "./pump";

describe("isPumped", () => {
  const workouts = ["2026-11-02 17:30:00", "2026-11-04 07:00:00"];
  it("is pumped after a workout the same day", () => {
    expect(isPumped("2026-11-02 18:45:00", workouts)).toBe(true);
    expect(isPumped("2026-11-04 07:00:00", workouts)).toBe(true);
  });
  it("is not pumped before that day's workout, or on another day", () => {
    expect(isPumped("2026-11-02 07:10:00", workouts)).toBe(false);
    expect(isPumped("2026-11-03 09:00:00", workouts)).toBe(false);
    expect(isPumped("2026-11-05 06:00:00", workouts)).toBe(false);
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
