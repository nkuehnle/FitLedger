/**
 * LOESS smoothing: local linear regression with tricube weights.
 * `span` is the fraction of points (0–1] in each local fit. O(n²), fine for a few thousand points.
 */
/** Smoothing slider top. 0 keeps each point's nearest neighbours only, so the line follows the data. */
export const SPAN_MAX = 1 / 3;
/** A long series tops out lower: a third of 200+ points smooths away nearly everything. */
export const SPAN_MAX_MANY = 0.25;
const FEW_POINTS = 20;
const MANY_POINTS = 200;
const MANY_POINTS_SPAN = 0.1;

/** The slider's top for a series of this many points. */
export function spanMax(points: number): number {
  return points >= MANY_POINTS ? SPAN_MAX_MANY : SPAN_MAX;
}

/**
 * Starting smoothing before you pick one: a third of the points for a short series, easing down
 * (by the log of the count) to 10 % for a long one, so a long history still shows its turns.
 */
export function autoSpan(points: number): number {
  if (points <= FEW_POINTS) return SPAN_MAX;
  if (points >= MANY_POINTS) return MANY_POINTS_SPAN;
  const f = Math.log(points / FEW_POINTS) / Math.log(MANY_POINTS / FEW_POINTS);
  return SPAN_MAX + f * (MANY_POINTS_SPAN - SPAN_MAX);
}

export function loess(xs: number[], ys: number[], span: number): number[] {
  const n = xs.length;
  if (n < 3) return ys.slice();
  const k = Math.min(n, Math.max(3, Math.ceil(span * n)));
  const out = new Array<number>(n);
  const dists = new Array<number>(n);
  for (let i = 0; i < n; i++) {
    const x0 = xs[i];
    for (let j = 0; j < n; j++) dists[j] = Math.abs(xs[j] - x0);
    // Distance to the k-th nearest neighbour (a bit beyond, so it keeps a small weight).
    const h = quickSelect(dists.slice(), k - 1) * 1.000001 || 1;
    let sw = 0,
      swx = 0,
      swy = 0,
      swxx = 0,
      swxy = 0;
    for (let j = 0; j < n; j++) {
      const u = dists[j] / h;
      if (u >= 1) continue;
      const w = (1 - u * u * u) ** 3;
      const x = xs[j] - x0; // centre for numerical stability
      sw += w;
      swx += w * x;
      swy += w * ys[j];
      swxx += w * x * x;
      swxy += w * x * ys[j];
    }
    const det = sw * swxx - swx * swx;
    // Intercept at x0 (centred), falling back to the weighted mean when the fit is degenerate.
    out[i] = Math.abs(det) < 1e-12 * Math.max(1, sw * swxx) ? swy / sw : (swy * swxx - swx * swxy) / det;
  }
  return out;
}

function quickSelect(a: number[], k: number): number {
  let lo = 0,
    hi = a.length - 1;
  while (lo < hi) {
    const pivot = a[(lo + hi) >> 1];
    let i = lo,
      j = hi;
    while (i <= j) {
      while (a[i] < pivot) i++;
      while (a[j] > pivot) j--;
      if (i <= j) {
        [a[i], a[j]] = [a[j], a[i]];
        i++;
        j--;
      }
    }
    if (k <= j) hi = j;
    else if (k >= i) lo = i;
    else return a[k];
  }
  return a[k];
}
