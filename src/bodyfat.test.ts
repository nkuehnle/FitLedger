import { describe, expect, it } from "vitest";
import type { MeasurementRow, WeighInRow } from "./api";
import { bodyFatSeries, navyBodyFat, relativeFatMass } from "./bodyfat";

describe("body fat estimates", () => {
  it("Navy method matches the published formula", () => {
    // 178 cm, waist 86, neck 38 → ~17.2 %
    expect(navyBodyFat("male", 178, 86, 38)).toBeCloseTo(17.2, 1);
    expect(navyBodyFat("female", 165, 75, 33)).toBeNull(); // needs hips
    expect(navyBodyFat("female", 165, 75, 33, 100)).toBeCloseTo(29.9, 0);
  });

  it("RFM uses height and waist", () => {
    expect(relativeFatMass("male", 178, 86)).toBeCloseTo(22.6, 1);
    expect(relativeFatMass("female", 178, 86)).toBeCloseTo(34.6, 1);
  });

  it("pairs each waist with a nearby neck and weigh-in", () => {
    const m = (id: number, at: string, part: string, value: number): MeasurementRow => ({ id, at, part, value }) as MeasurementRow;
    const rows = [
      m(1, "2026-10-01 08:00:00", "waist", 86),
      m(2, "2026-10-03 08:00:00", "neck", 38),
      m(3, "2026-06-01 08:00:00", "waist", 90), // no neck within two weeks: RFM only
    ];
    const w: WeighInRow[] = [{ id: 1, at: "2026-10-02 08:00:00", weight_kg: 80, source: "manual" }];
    const s = bodyFatSeries(rows, [], w, "male", 178);
    expect(s.rfm).toHaveLength(2);
    expect(s.navy).toHaveLength(1);
    expect(s.leanEstimated[0].v).toBeCloseTo(80 * (1 - 0.172), 0);
  });
});
