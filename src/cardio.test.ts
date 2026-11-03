import { describe, expect, it } from "vitest";
import type { CardioSession } from "./api";
import { distText, paceMode, paceText, weeklyCardio } from "./cardio";
import { parseTime } from "./pages/CardioPage";

const session = (at: string, distance_m: number | null, duration_s: number | null): CardioSession => ({
  workout_id: 1,
  exercise_id: 1,
  name: "Running",
  at,
  distance_m,
  duration_s,
  avg_hr: null,
  kcal: null,
  segments: 1,
});

describe("cardio", () => {
  it("picks pace, speed or /500m from the activity", () => {
    expect(paceMode("Running (Treadmill)")).toBe("pace");
    expect(paceMode("Cycling (Indoor)")).toBe("speed");
    expect(paceMode("Rowing (Machine)")).toBe("split500");
  });

  it("formats pace, speed and splits", () => {
    expect(paceText(5000, 1500, "pace", "kg")).toBe("5:00 /km");
    expect(paceText(1609.344, 480, "pace", "lb")).toBe("8:00 /mi");
    expect(paceText(16093.44, 3600, "speed", "lb")).toBe("10.0 mph");
    expect(paceText(2000, 480, "split500", "kg")).toBe("2:00 /500m");
    expect(paceText(null, 600, "pace", "kg")).toBeNull();
    expect(distText(5000, "kg")).toBe("5.00 km");
  });

  it("totals weeks, including empty ones", () => {
    const weeks = weeklyCardio(
      [session("2026-11-02 07:00:00", 5000, 1800), session("2026-11-04 07:00:00", 3000, 1200), session("2026-10-20 07:00:00", null, 600)],
      3,
      new Date(2026, 10, 5),
    );
    expect(weeks.map((w) => w.week)).toEqual(["2026-10-19", "2026-10-26", "2026-11-02"]);
    expect(weeks.map((w) => w.minutes)).toEqual([10, 0, 50]);
    expect(weeks[2].distanceM).toBe(8000);
    expect(weeks[2].sessions).toBe(2);
  });
});

describe("parseTime", () => {
  it("reads clock times, seconds and plain minutes", () => {
    expect(parseTime("25:30")).toBe(1530);
    expect(parseTime("1:05:00")).toBe(3900);
    expect(parseTime("45s")).toBe(45);
    expect(parseTime("25")).toBe(1500);
    expect(parseTime("")).toBeNull();
  });
});
