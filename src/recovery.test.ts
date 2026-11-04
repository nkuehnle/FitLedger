import { describe, expect, it } from "vitest";
import type { DayHealth } from "./api";
import { recoveryText } from "./recovery";

const day = (date: string, d: Partial<DayHealth>): DayHealth => ({
  date,
  source: "health_connect",
  sleep_s: null,
  resting_hr: null,
  hrv_ms: null,
  steps: null,
  ...d,
});

describe("recoveryText", () => {
  it("shows today's numbers against the usual of the weeks before", () => {
    const before = Array.from({ length: 10 }, (_, i) => day(`2026-10-${String(20 + i).padStart(2, "0")}`, { resting_hr: 54, hrv_ms: 58 }));
    const today = day("2026-11-03", { sleep_s: 7 * 3600 + 5 * 60, resting_hr: 52, hrv_ms: 61.4, steps: 8204 });
    expect(recoveryText([...before, today], "2026-11-03")).toBe(
      `Slept 7h 05m · resting HR 52 (usual 54) · HRV 61 ms (usual 58) · ${(8204).toLocaleString()} steps`,
    );
  });
  it("leaves out the usual with under a week of history, and is empty without today", () => {
    expect(recoveryText([day("2026-11-02", { resting_hr: 50 }), day("2026-11-03", { resting_hr: 52 })], "2026-11-03")).toBe("resting HR 52");
    expect(recoveryText([day("2026-11-02", { resting_hr: 50 })], "2026-11-03")).toBeNull();
  });
});
