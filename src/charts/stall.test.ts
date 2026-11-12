import { describe, expect, it } from "vitest";
import { plateaus } from "./stall";

const weekly = (values: number[], from = "2026-08-03") =>
  values.map((v, i) => ({ t: new Date(new Date(`${from}T12:00:00`).getTime() + i * 7 * 86_400_000), v }));

describe("plateaus", () => {
  it("needs four more sessions over four weeks at the same level", () => {
    const pts = weekly([100, 104, 104, 103, 104.5, 104]);
    const [p, ...rest] = plateaus(pts);
    expect(rest).toEqual([]);
    expect([p.from, p.to, p.v, p.sessions]).toEqual([pts[1].t, pts[5].t, 104, 4]);
    // One session short.
    expect(plateaus(weekly([100, 104, 104, 103, 104]))).toEqual([]);
    // Enough sessions, but inside four weeks.
    expect(plateaus(pts.map((q, i) => ({ ...q, t: new Date(pts[0].t.getTime() + i * 3 * 86_400_000) })))).toEqual([]);
  });

  it("ends at the next gain and finds the next one", () => {
    const pts = weekly([90, 95, 100, 100, 100, 100, 100, 104, 108, 108, 108, 108, 108.5]);
    const ps = plateaus(pts);
    expect(ps.map((p) => [p.from, p.to, p.v])).toEqual([
      [pts[2].t, pts[6].t, 100],
      [pts[8].t, pts[12].t, 108],
    ]);
  });

  it("is empty for steady progress, a slow creep, and a drop with its rebuild", () => {
    expect(plateaus(weekly([100, 102, 104, 106, 108, 110, 112]))).toEqual([]);
    expect(plateaus(weekly([100, 100.6, 101.2, 101.8, 102.4, 103, 103.6]))).toEqual([]);
    expect(plateaus(weekly([100, 80, 84, 88, 92, 96, 100]))).toEqual([]);
  });
});
