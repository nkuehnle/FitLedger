// Plateaus on an exercise chart: stretches where the line went flat. A plateau starts at a
// session and lasts while the sessions after it stay level with it: not PROGRESS_STEP above
// (that's progress) and not DROP below (that's a setback or a deload, and the climb back from
// one isn't a plateau either). It counts once it has STALL_SESSIONS sessions over STALL_DAYS days.
// (The routine review's "stopped moving" check in crates/core/src/trend.rs asks a different
// question, whether a lift is stuck right now, and so also looks back over the months before.)

export interface PlateauPoint {
  t: Date;
  v: number;
}

export interface Plateau {
  /** The session that set the level, and the last session still at it. */
  from: Date;
  to: Date;
  /** The level that wasn't beaten. */
  v: number;
  /** Sessions after the first. */
  sessions: number;
}

const PROGRESS_STEP = 0.01;
const DROP = 0.05;
const STALL_SESSIONS = 4;
const STALL_DAYS = 28;
const DAY = 86_400_000;

/** Every plateau in a history (oldest first). */
export function plateaus(points: PlateauPoint[]): Plateau[] {
  const out: Plateau[] = [];
  let i = 0;
  while (i < points.length) {
    const level = points[i].v;
    let k = i;
    while (k + 1 < points.length && points[k + 1].v < level * (1 + PROGRESS_STEP) && points[k + 1].v >= level * (1 - DROP)) k++;
    const sessions = k - i;
    if (sessions >= STALL_SESSIONS && (points[k].t.getTime() - points[i].t.getTime()) / DAY >= STALL_DAYS) {
      out.push({ from: points[i].t, to: points[k].t, v: level, sessions });
      i = k + 1;
    } else i++;
  }
  return out;
}
