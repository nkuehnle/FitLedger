import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { api, type WorkoutDetail } from "../api";
import { formatClock, parseLocal } from "../format";
import { IS_ANDROID } from "../platform";

export interface RestTimerState {
  endsAt: number | null;
  total: number;
  remaining: number;
  /** `next` describes the upcoming set, e.g. "Next: Chest Press · set 2". */
  start: (seconds: number, next?: string) => void;
  adjust: (delta: number) => void;
  stop: () => void;
  /** Workout finished or cancelled: remove the Android notification. */
  clear: () => void;
}

// Native rest timer (Android): ongoing notification + exact alarm. No-ops elsewhere.
const native = {
  status: (title: string, text: string, workoutStartMs: number) =>
    invoke("rest_android_status", { title, text, workoutStartMs }).catch(() => {}),
  rest: (title: string, next: string, endsAtMs: number, workoutStartMs: number) =>
    invoke("rest_android_rest", { title, next, endsAtMs, workoutStartMs }).catch(() => {}),
  clear: () => invoke("rest_android_clear").catch(() => {}),
};

/** Plays a short two-tone beep (works without any audio files). */
function beep() {
  try {
    const ctx = new AudioContext();
    [0, 0.25].forEach((t, i) => {
      const o = ctx.createOscillator();
      const g = ctx.createGain();
      o.frequency.value = i ? 1046 : 784;
      g.gain.setValueAtTime(0.0001, ctx.currentTime + t);
      g.gain.exponentialRampToValueAtTime(0.3, ctx.currentTime + t + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + t + 0.22);
      o.connect(g).connect(ctx.destination);
      o.start(ctx.currentTime + t);
      o.stop(ctx.currentTime + t + 0.25);
    });
  } catch {
    /* no audio available */
  }
}

async function notifyRestOver() {
  // On Android the native alarm vibrates and alerts (even in the background); don't double up.
  if (IS_ANDROID) return;
  beep();
  navigator.vibrate?.([300, 120, 300]);
  // A system notification matters when the window isn't in front; it degrades to nothing if denied.
  if (document.hasFocus()) return;
  try {
    let ok = await isPermissionGranted();
    if (!ok) ok = (await requestPermission()) === "granted";
    if (ok) sendNotification({ title: "Rest is over", body: "Time for your next set." });
  } catch {
    /* notifications unavailable on this platform */
  }
}

/** Rest timer for a live workout; the end time is stored so it survives reloads. */
export function useRestTimer(w: WorkoutDetail | null): RestTimerState {
  const [endsAt, setEndsAt] = useState<number | null>(null);
  const [total, setTotal] = useState(0);
  const [now, setNow] = useState(Date.now());
  const fired = useRef<number | null>(null);
  const next = useRef("Time for your next set");
  const workoutId = w?.id ?? null;
  const title = w?.name ?? "Workout";
  const startMs = w ? parseLocal(w.started_at_local).getTime() : 0;

  useEffect(() => {
    if (!w) return;
    const e = w.rest_ends_at ? Number(w.rest_ends_at) : null;
    const resting = e && e > Date.now();
    setEndsAt(resting ? e : null);
    setTotal(w.rest_total_s ?? 0);
    if (resting) native.rest(title, next.current, e!, startMs);
    else native.status(title, "Workout in progress", startMs);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [w?.id]);

  const persist = useCallback(
    (e: number | null, t: number) => {
      if (workoutId) api.setRestTimer(workoutId, e ? String(e) : null, e ? t : null);
    },
    [workoutId],
  );

  useEffect(() => {
    if (!endsAt) return;
    const tick = setInterval(() => setNow(Date.now()), 250);
    const done = setTimeout(
      () => {
        if (fired.current !== endsAt) {
          fired.current = endsAt;
          notifyRestOver();
        }
        setEndsAt(null);
        persist(null, 0);
      },
      Math.max(0, endsAt - Date.now()),
    );
    return () => {
      clearInterval(tick);
      clearTimeout(done);
    };
  }, [endsAt, persist]);

  return {
    endsAt,
    total,
    remaining: endsAt ? Math.max(0, Math.ceil((endsAt - now) / 1000)) : 0,
    start: (s, nextSet) => {
      const e = Date.now() + s * 1000;
      if (nextSet) next.current = nextSet;
      setNow(Date.now());
      setEndsAt(e);
      setTotal(s);
      persist(e, s);
      native.rest(title, next.current, e, startMs);
    },
    adjust: (d) => {
      if (!endsAt) return;
      const e = Math.max(Date.now() + 1000, endsAt + d * 1000);
      setEndsAt(e);
      setTotal((t) => Math.max(1, t + d));
      persist(e, total + d);
      native.rest(title, next.current, e, startMs);
    },
    stop: () => {
      setEndsAt(null);
      persist(null, 0);
      native.status(title, "Ready for your next set", startMs);
    },
    clear: () => {
      setEndsAt(null);
      persist(null, 0);
      native.clear();
    },
  };
}

export function RestTimer({ timer }: { timer: RestTimerState }) {
  if (!timer.endsAt) return null;
  const pct = timer.total ? Math.min(100, (timer.remaining / timer.total) * 100) : 0;
  return (
    <div className="rest-timer" role="timer" aria-live="polite">
      <div className="rest-progress" style={{ width: `${pct}%` }} />
      <button className="rest-btn" onClick={() => timer.adjust(-15)}>
        −15
      </button>
      <div className="rest-time">
        <span className="small muted">Rest</span>
        <strong>{formatClock(timer.remaining)}</strong>
      </div>
      <button className="rest-btn" onClick={() => timer.adjust(15)}>
        +15
      </button>
      <button className="rest-btn skip" onClick={timer.stop}>
        Skip
      </button>
    </div>
  );
}
