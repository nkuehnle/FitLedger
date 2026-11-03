import { useEffect, useMemo, useState } from "react";
import { api, type CardioSession, type ExerciseInfo } from "../api";
import { usePrefs } from "../App";
import { BarChart } from "../charts/BarChart";
import { TimeSeriesChart } from "../charts/TimeSeriesChart";
import { distText, paceMode, paceText, paceValue, weeklyCardio } from "../cardio";
import { ProgressHeader } from "../components/ProgressHeader";
import { formatClock, formatDate, nowLocal, parseLocal } from "../format";

const M_PER_MI = 1609.344;
const DEFAULT_GOAL_MIN = 150; // a common weekly guideline for moderate cardio

/** "25:30", "1:05:00", "45s" or plain minutes ("25") → seconds. */
export function parseTime(t: string): number | null {
  const parts = t.trim().replace(/s$/i, "").split(":").map(Number);
  if (!t.trim() || parts.some((n) => Number.isNaN(n))) return null;
  if (parts.length === 1) return /s$/i.test(t.trim()) ? parts[0] : parts[0] * 60;
  return parts.reduce((acc, v) => acc * 60 + v, 0);
}

/** Progress → Cardio: weekly minutes against a goal, each activity's pace trend, and quick logging. */
export default function CardioPage({ log }: { log?: boolean }) {
  const { unit } = usePrefs();
  const [sessions, setSessions] = useState<CardioSession[] | null>(null);
  const [goal, setGoal] = useState(DEFAULT_GOAL_MIN);
  const [logging, setLogging] = useState(!!log);
  const load = () => api.cardioSessions().then(setSessions);
  useEffect(() => {
    load();
    api.getSetting("cardio_goal_min").then((g) => g && setGoal(Number(g)));
  }, []);
  if (!sessions) return null;

  const weeks = weeklyCardio(sessions, 12);
  const thisWeek = weeks[weeks.length - 1];
  const activities = [...new Set([...sessions].reverse().map((s) => s.exercise_id))];
  const pct = Math.min(1, thisWeek.minutes / Math.max(1, goal));

  return (
    <div className="page stats-page">
      <ProgressHeader current="cardio" />
      <section className="card">
        <div className="title-row">
          <h2 className="section-title">This week</h2>
          <button className="button small primary" onClick={() => setLogging(!logging)}>
            {logging ? "Close" : "+ Log a session"}
          </button>
        </div>
        {logging && (
          <LogCardio
            onSaved={() => {
              setLogging(false);
              load();
            }}
          />
        )}
        <div className="stats-row">
          <div className="stat">
            <div className="stat-value">
              {Math.round(thisWeek.minutes)} <span className="muted small">/ {goal} min</span>
            </div>
            <div className="stat-label">Cardio minutes</div>
          </div>
          <div className="stat">
            <div className="stat-value">{thisWeek.distanceM ? distText(thisWeek.distanceM, unit) : "—"}</div>
            <div className="stat-label">Distance</div>
          </div>
          <div className="stat">
            <div className="stat-value">{thisWeek.sessions}</div>
            <div className="stat-label">Sessions</div>
          </div>
        </div>
        <div className="goal-bar" aria-label={`${Math.round(pct * 100)}% of weekly goal`}>
          <div style={{ width: `${pct * 100}%` }} />
        </div>
        <label className="small muted row">
          Weekly goal
          <input
            className="input mini-input"
            inputMode="numeric"
            defaultValue={goal}
            onBlur={(e) => {
              const v = Number(e.target.value);
              if (v > 0) {
                setGoal(v);
                api.setSetting("cardio_goal_min", String(v));
              }
            }}
          />
          min
        </label>
      </section>

      <section className="card">
        <h2 className="section-title">Minutes per week</h2>
        <BarChart
          bars={weeks.map((w) => ({
            key: w.week,
            label: formatDate(w.week).replace(/^\w+, /, "").replace(/, \d{4}$/, ""),
            value: Math.round(w.minutes),
            tooltip: `${Math.round(w.minutes)} min · ${w.sessions} session${w.sessions === 1 ? "" : "s"}${w.distanceM ? ` · ${distText(w.distanceM, unit)}` : ""}`,
          }))}
          unit="min"
          goal={goal}
          labelEvery={2}
          height={180}
        />
        <p className="muted small">The dashed line is your weekly goal.</p>
      </section>

      {sessions.length === 0 ? (
        <p className="empty">No cardio yet. Log a run, ride or row above, or add one to a workout.</p>
      ) : (
        activities.map((id) => <CardioActivity key={id} sessions={sessions.filter((s) => s.exercise_id === id)} />)
      )}
    </div>
  );
}

/** One activity: latest session against your usual pace, best pace, and pace/distance over time. */
export function CardioActivity({ sessions }: { sessions: CardioSession[] }) {
  const { unit } = usePrefs();
  const name = sessions[0]?.name ?? "";
  const mode = paceMode(name);
  const timed = sessions.filter((s) => s.distance_m && s.duration_s);
  const last = sessions[sessions.length - 1];
  const value = (s: CardioSession) => paceValue(s.distance_m!, s.duration_s!, mode, unit);
  // Faster is lower for pace and /500 m, higher for speed.
  const better = (a: number, b: number) => (mode === "speed" ? a > b : a < b);
  const best = timed.reduce<CardioSession | null>((b, s) => (!b || better(value(s), value(b)) ? s : b), null);
  const recent = timed.slice(-6, -1);
  const usual = recent.length ? recent.reduce((n, s) => n + value(s), 0) / recent.length : null;
  const lastV = last?.distance_m && last.duration_s ? value(last) : null;
  const diff = lastV != null && usual != null ? lastV - usual : null;
  const diffText =
    diff == null || Math.abs(diff) < (mode === "speed" ? 0.05 : 1)
      ? null
      : mode === "speed"
        ? `${diff > 0 ? "+" : "−"}${Math.abs(diff).toFixed(1)} ${unit === "lb" ? "mph" : "km/h"} vs your usual`
        : `${formatClock(Math.abs(diff))} ${diff < 0 ? "faster" : "slower"} than your usual`;
  const total = sessions.reduce((n, s) => n + (s.distance_m ?? 0), 0);
  const minutes = sessions.reduce((n, s) => n + (s.duration_s ?? 0), 0) / 60;
  const paceLabel = mode === "speed" ? (unit === "lb" ? "mph" : "km/h") : mode === "split500" ? "/500m" : unit === "lb" ? "/mi" : "/km";

  return (
    <section className="card">
      <h2 className="section-title">{name}</h2>
      {last && (
        <p className="small">
          Last: <strong>{[last.distance_m ? distText(last.distance_m, unit) : null, last.duration_s ? formatClock(last.duration_s) : null, paceText(last.distance_m, last.duration_s, mode, unit)].filter(Boolean).join(" · ")}</strong>
          {last.avg_hr ? ` · ♥ ${Math.round(last.avg_hr)}` : ""} <span className="muted">({formatDate(last.at)})</span>
          {diffText && (
            <>
              <br />
              <span className={diff != null && better(diff, 0) ? "success" : "muted"}>{diffText}</span>
            </>
          )}
        </p>
      )}
      <div className="stats-row small">
        <span>
          {sessions.length} session{sessions.length === 1 ? "" : "s"}
        </span>
        {total > 0 && <span>{distText(total, unit)} total</span>}
        <span>{Math.round(minutes)} min</span>
        {best && (
          <span>
            Best {paceText(best.distance_m, best.duration_s, mode, unit)} ({formatDate(best.at)})
          </span>
        )}
      </div>
      {timed.length >= 2 && (
        <TimeSeriesChart
          series={[{ id: "pace", label: mode === "speed" ? "Speed" : "Pace", points: timed.map((s) => ({ t: parseLocal(s.at), v: value(s) })) }]}
          unit={paceLabel}
          yFormat={(v) => (mode === "speed" ? v.toFixed(1) : formatClock(v))}
          height={180}
        />
      )}
      {timed.length >= 2 && mode !== "speed" && <p className="muted small">Lower is faster.</p>}
    </section>
  );
}

/** Logs a session done outside a workout (a run outside, a class), as its own entry. */
function LogCardio({ onSaved }: { onSaved: () => void }) {
  const { unit } = usePrefs();
  const [options, setOptions] = useState<ExerciseInfo[]>([]);
  const [f, setF] = useState({ exercise: 0, at: nowLocal().slice(0, 16).replace(" ", "T"), dist: "", time: "", hr: "", kcal: "", notes: "" });
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    Promise.all([api.listExercises(true), api.listExercises(false)]).then(([used, all]) => {
      const cardio = (xs: ExerciseInfo[]) => xs.filter((x) => x.kind === "distance_duration");
      const usedIds = new Set(cardio(used).map((x) => x.id));
      const list = [...cardio(used), ...cardio(all).filter((x) => !usedIds.has(x.id))];
      setOptions(list);
      if (list.length) setF((v) => ({ ...v, exercise: list[0].id }));
    });
  }, []);
  const mode = useMemo(() => paceMode(options.find((o) => o.id === f.exercise)?.name ?? ""), [options, f.exercise]);
  const distM = f.dist.trim() ? Number(f.dist) * (unit === "lb" ? M_PER_MI : 1000) : null;
  const secs = parseTime(f.time);
  const pace = paceText(distM, secs, mode, unit);
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF({ ...f, [k]: e.target.value });

  async function save() {
    setError(null);
    if (!f.exercise) return setError("Pick an activity.");
    if (distM == null && secs == null) return setError("Enter a distance or a time.");
    try {
      await api.logCardio({
        exercise_id: f.exercise,
        at: f.at.replace("T", " ") + ":00",
        distance_m: distM,
        duration_s: secs,
        avg_hr: f.hr ? Number(f.hr) : null,
        kcal: f.kcal ? Number(f.kcal) : null,
        notes: f.notes.trim() || null,
      });
      onSaved();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="cardio-log">
      <div className="form-grid">
        <label>
          Activity
          <select className="select" value={f.exercise} onChange={(e) => setF({ ...f, exercise: Number(e.target.value) })}>
            {options.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          When
          <input className="input" type="datetime-local" value={f.at} onChange={set("at")} />
        </label>
        <label>
          Distance ({unit === "lb" ? "mi" : "km"})
          <input className="input" inputMode="decimal" value={f.dist} onChange={set("dist")} placeholder="3.1" />
        </label>
        <label>
          Time
          <input className="input" inputMode="text" value={f.time} onChange={set("time")} placeholder="25:30" />
        </label>
        <label>
          Avg heart rate
          <input className="input" inputMode="numeric" value={f.hr} onChange={set("hr")} placeholder="optional" />
        </label>
        <label>
          Calories
          <input className="input" inputMode="numeric" value={f.kcal} onChange={set("kcal")} placeholder="optional" />
        </label>
      </div>
      <input className="input" value={f.notes} onChange={set("notes")} placeholder="Notes (route, how it felt…)" />
      <div className="row">
        <button className="button primary" onClick={save}>
          Save
        </button>
        {pace && <span className="muted small">{pace}</span>}
      </div>
      {error && <p className="error small">{error}</p>}
    </div>
  );
}
