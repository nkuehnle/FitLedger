import { describe, expect, it } from "vitest";
import { loess } from "./loess";

describe("loess", () => {
  it("reproduces a straight line exactly", () => {
    const xs = Array.from({ length: 50 }, (_, i) => i * 3 + (i % 7));
    const ys = xs.map((x) => 2 * x + 5);
    loess(xs, ys, 0.3).forEach((v, i) => expect(v).toBeCloseTo(ys[i], 6));
  });

  it("smooths noise toward the underlying trend", () => {
    const xs = Array.from({ length: 200 }, (_, i) => i);
    const noise = xs.map((i) => ((i * 7919) % 13) - 6); // deterministic, mean ~0
    const ys = xs.map((x, i) => 0.1 * x + noise[i]);
    const sm = loess(xs, ys, 0.3);
    const rawErr = ys.reduce((s, y, i) => s + Math.abs(y - 0.1 * xs[i]), 0);
    const smErr = sm.reduce((s, y, i) => s + Math.abs(y - 0.1 * xs[i]), 0);
    expect(smErr).toBeLessThan(rawErr / 3);
  });

  it("handles tiny and duplicate-x inputs", () => {
    expect(loess([1, 2], [3, 4], 0.5)).toEqual([3, 4]);
    const sm = loess([1, 1, 1, 2, 2, 2], [1, 2, 3, 4, 5, 6], 1);
    sm.forEach((v) => expect(Number.isFinite(v)).toBe(true));
  });

  it("larger spans are smoother", () => {
    const xs = Array.from({ length: 100 }, (_, i) => i);
    const ys = xs.map((i) => Math.sin(i / 3) * 5 + i * 0.05);
    const rough = (a: number[]) => a.slice(1).reduce((s, v, i) => s + Math.abs(v - a[i]), 0);
    expect(rough(loess(xs, ys, 0.8))).toBeLessThan(rough(loess(xs, ys, 0.1)));
  });
});
