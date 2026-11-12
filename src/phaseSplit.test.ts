import { describe, expect, it } from "vitest";
import type { InBodyScan, MeasurementRow, WeighInRow } from "./api";
import { phaseSplit } from "./phaseSplit";

const weigh = (at: string, weight_kg: number): WeighInRow => ({ id: 0, at, weight_kg, source: "manual" }) as WeighInRow;
const tape = (at: string, part: string, value: number): MeasurementRow => ({ id: 0, at, part, value }) as MeasurementRow;
const scan = (at: string, weight_kg: number, pbf: number): InBodyScan =>
  ({ id: 1, measured_at_local: at, values: { weight_kg, pbf }, device: null, notes: null }) as InBodyScan;
const now = new Date(2026, 5, 1);

describe("phaseSplit", () => {
  it("splits a gain between two InBody scans, using the scans' own weights", () => {
    const scans = [scan("2026-01-05 09:00:00", 80, 15), scan("2026-04-20 09:00:00", 85, 16)];
    const r = phaseSplit("2026-01-01", "2026-04-30", now, [], scans, [], "male", 180)!;
    expect(r.method).toBe("inbody");
    expect(r.weightKg).toBeCloseTo(5, 6);
    // 12 kg of fat became 13.6.
    expect(r.fatKg).toBeCloseTo(1.6, 6);
    expect(r.leanKg).toBeCloseTo(3.4, 6);
  });

  it("falls back to the tape, and needs a weigh-in near each reading", () => {
    const m = [
      tape("2026-01-03 08:00:00", "waist", 84),
      tape("2026-01-03 08:00:00", "neck", 38),
      tape("2026-03-20 08:00:00", "waist", 86),
      tape("2026-03-20 08:00:00", "neck", 39),
    ];
    const w = [weigh("2026-01-02 08:00:00", 80), weigh("2026-03-21 08:00:00", 84)];
    const r = phaseSplit("2026-01-01", null, now, m, [], w, "male", 180)!;
    expect(r.method).toBe("navy");
    expect(r.weightKg).toBeCloseTo(4, 6);
    expect(r.fatKg + r.leanKg).toBeCloseTo(4, 6);
    // No weigh-in near the second reading: nothing to split.
    expect(phaseSplit("2026-01-01", null, now, m, [], w.slice(0, 1), "male", 180)).toBeNull();
  });

  it("needs readings three weeks apart, inside the stretch", () => {
    const scans = [scan("2026-01-05 09:00:00", 80, 15), scan("2026-01-15 09:00:00", 80.5, 15.1)];
    expect(phaseSplit("2026-01-01", "2026-04-30", now, [], scans, [], "male", 180)).toBeNull();
    const far = [scan("2025-06-01 09:00:00", 78, 15), scan("2026-04-20 09:00:00", 85, 16)];
    expect(phaseSplit("2026-01-01", "2026-04-30", now, [], far, [], "male", 180)).toBeNull();
  });
});
