import { ask, TextPrompt } from "../components/Modal";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  type BestSet,
  type Muscle,
  type PrView,
  type Template,
  type WeightUnit,
  type WorkoutCard,
  type WorkoutDetail,
  type WorkoutPoint,
} from "../api";
import { navigate, usePrefs } from "../App";
import { BodyMap } from "../charts/BodyMap";
import { isoDate } from "../charts/RangePicker";
import { templateColor, useChartTheme, type ChartTheme } from "../charts/theme";
import { ShareButton } from "../share/ShareDialog";
import { StartPanel } from "../components/StartPanel";
import { distText, paceMode, paceText } from "../cardio";
import { TodayCard } from "../components/TodayCard";
import {
  EQUIPMENT_LABELS,
  formatClock,
  formatDate,
  formatDistance,
  formatDuration,
  formatTime,
  formatVolume,
  formatWeight,
  setLabel,
} from "../format";

const PAGE = 20;

export default function HistoryPage({ selectedId, view }: { selectedId: number | null; view: "detail" | "summary" }) {
  const [mode, setMode] = useState<"feed" | "calendar">(() => (localStorageGet("history-mode") as "feed" | "calendar") ?? "feed");
  return (
    <div className={selectedId ? "split has-detail" : "split"}>
      <section className="split-list">
        <div className="title-row">
          <h1 className="page-title">Workouts</h1>
          <div className="segmented small">
            {(["feed", "calendar"] as const).map((m) => (
              <button
                key={m}
                className={mode === m ? "active" : ""}
                onClick={() => {
                  setMode(m);
                  localStorageSet("history-mode", m);
                }}
              >
                {m === "feed" ? "List" : "Calendar"}
              </button>
            ))}
          </div>
        </div>
        <TodayCard />
        <StartPanel />
        {mode === "feed" ? <Feed selectedId={selectedId} /> : <CalendarView selectedId={selectedId} />}
      </section>
      <section className="split-detail">
        {selectedId ? <Detail id={selectedId} summary={view === "summary"} /> : <p className="empty">Select a workout to see it here.</p>}
      </section>
    </div>
  );
}

function localStorageGet(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function localStorageSet(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* storage unavailable */
  }
}

function Feed({ selectedId }: { selectedId: number | null }) {
  const { unit } = usePrefs();
  const [cards, setCards] = useState<WorkoutCard[]>([]);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loading = useRef(false);
  const sentinel = useRef<HTMLDivElement>(null);

  const loadMore = useCallback(async () => {
    if (loading.current || done) return;
    loading.current = true;
    try {
      const last = cards[cards.length - 1];
      const page = await api.historyPage(PAGE, last && { time: last.started_at_local, id: last.id });
      setCards((c) => [...c, ...page]);
      if (page.length < PAGE) setDone(true);
    } catch (e) {
      setError(String(e));
    } finally {
      loading.current = false;
    }
  }, [cards, done]);

  useEffect(() => {
    const el = sentinel.current;
    if (!el) return;
    const obs = new IntersectionObserver((entries) => entries[0].isIntersecting && loadMore(), { rootMargin: "600px" });
    obs.observe(el);
    return () => obs.disconnect();
  }, [loadMore]);

  if (error) return <p className="error">{error}</p>;
  if (done && cards.length === 0)
    return (
      <div className="empty">
        <p>No workouts yet.</p>
        <a className="button" href="#/data">
          Import from Strong
        </a>
      </div>
    );

  return (
    <div className="feed">
      {cards.map((c) => (
        <button
          key={c.id}
          className={c.id === selectedId ? "card workout-card selected" : "card workout-card"}
          onClick={() => navigate(`/history/${c.id}`)}
        >
          <div className="card-head">
            <div>
              <div className="card-title">{c.name}</div>
              <div className="muted small">
                {formatDate(c.started_at_local)} · {formatTime(c.started_at_local)}
              </div>
            </div>
            {c.prs > 0 && (
              <span className="pr-badge">
                🏆 {c.prs} PR{c.prs > 1 ? "s" : ""}
              </span>
            )}
          </div>
          <div className="stats-row small">
            {c.duration_s != null && <span>{formatDuration(c.duration_s)}</span>}
            {c.volume_kg > 0 && <span>{formatVolume(c.volume_kg, unit)}</span>}
            <span>{c.sets} sets</span>
          </div>
          <ul className="card-exercises">
            {c.exercises.map((e, i) => (
              <li key={i} className="superset-row" data-group={e.superset ?? undefined}>
                <span className="ex-name">
                  {/* Cardio reads "Running" (one segment) rather than "1 × Running". */}
                  {e.best?.reps == null && e.best?.distance_m != null && e.sets === 1 ? "" : `${e.sets} × `}
                  {e.name}
                </span>
                <span className="muted">{e.best && bestText(e.best, unit, e.name)}</span>
              </li>
            ))}
          </ul>
        </button>
      ))}
      <div ref={sentinel} className="sentinel">
        {!done && <span className="muted small">Loading…</span>}
      </div>
    </div>
  );
}

/** Heaviest set, or for cardio the session's distance · time · pace. */
export function bestText(b: BestSet, unit: WeightUnit, name = ""): string {
  if (b.reps != null) return b.weight_kg ? `${formatWeight(b.weight_kg, unit)} ${unit} × ${b.reps}` : `${b.reps} reps`;
  return [formatDistance(b.distance_m, unit), formatClock(b.duration_s), paceText(b.distance_m, b.duration_s, paceMode(name), unit)]
    .filter(Boolean)
    .join(" · ");
}

// ---- Calendar ---------------------------------------------------------------------------------------

function useWorkoutColors(points: WorkoutPoint[], templates: Template[], theme: ChartTheme) {
  return useMemo(() => {
    const byName = new Map(templates.map((t) => [t.name, templateColor(t.color, theme)]));
    // Names without a template get the next categorical slot, most frequent first.
    const counts = new Map<string, number>();
    points.forEach((p) => counts.set(p.name, (counts.get(p.name) ?? 0) + 1));
    const others = [...counts.keys()].filter((n) => !byName.has(n)).sort((a, b) => counts.get(b)! - counts.get(a)!);
    others.forEach((n, i) => byName.set(n, i < 8 - templates.length ? theme.categorical[(templates.length + i) % 8] : theme.muted));
    return (name: string) => byName.get(name) ?? theme.muted;
  }, [points, templates, theme]);
}

function CalendarView({ selectedId }: { selectedId: number | null }) {
  const theme = useChartTheme();
  const [points, setPoints] = useState<WorkoutPoint[]>([]);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [span, setSpan] = useState<"month" | "week">("month");
  const [cursor, setCursor] = useState(() => new Date());
  useEffect(() => {
    api.workoutSeries().then(setPoints);
    api.listTemplates().then(setTemplates);
  }, []);
  const color = useWorkoutColors(points, templates, theme);
  const byDay = useMemo(() => {
    const m = new Map<string, WorkoutPoint[]>();
    points.forEach((p) => m.set(p.at.slice(0, 10), [...(m.get(p.at.slice(0, 10)) ?? []), p]));
    return m;
  }, [points]);

  const first = span === "month" ? new Date(cursor.getFullYear(), cursor.getMonth(), 1) : mondayOf(cursor);
  const gridStart = mondayOf(first);
  const days: Date[] = [];
  const count = span === "month" ? 42 : 7;
  for (let i = 0; i < count; i++) days.push(new Date(gridStart.getFullYear(), gridStart.getMonth(), gridStart.getDate() + i));
  const shift = (n: number) =>
    setCursor(
      span === "month"
        ? new Date(cursor.getFullYear(), cursor.getMonth() + n, 1)
        : new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 7 * n),
    );
  const title =
    span === "month"
      ? cursor.toLocaleDateString(undefined, { month: "long", year: "numeric" })
      : `${days[0].toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${days[6].toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
  const inView = days.filter((d) => span === "week" || d.getMonth() === first.getMonth());
  const total = inView.reduce((n, d) => n + (byDay.get(isoDate(d))?.length ?? 0), 0);
  const legend = [...new Set(inView.flatMap((d) => byDay.get(isoDate(d)) ?? []).map((p) => p.name))];

  return (
    <div className="calendar">
      <div className="calendar-nav">
        <button className="icon-btn" onClick={() => shift(-1)} aria-label="Previous">
          ‹
        </button>
        <div>
          <strong>{title}</strong>
          <div className="muted small">{total} workouts</div>
        </div>
        <button className="icon-btn" onClick={() => shift(1)} aria-label="Next">
          ›
        </button>
        <div className="segmented small">
          {(["month", "week"] as const).map((s) => (
            <button key={s} className={span === s ? "active" : ""} onClick={() => setSpan(s)}>
              {s === "month" ? "Month" : "Week"}
            </button>
          ))}
        </div>
        <button className="link-btn small" onClick={() => setCursor(new Date())}>
          Today
        </button>
      </div>
      <div className={span === "month" ? "cal-grid" : "cal-grid week"}>
        {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map((d) => (
          <div key={d} className="cal-dow muted small">
            {d}
          </div>
        ))}
        {days.map((d) => {
          const key = isoDate(d);
          const ws = byDay.get(key) ?? [];
          const out = span === "month" && d.getMonth() !== first.getMonth();
          const isToday = key === isoDate(new Date());
          return (
            <div key={key} className={`cal-day${out ? " out" : ""}${isToday ? " today" : ""}`}>
              <span className="cal-num small">{d.getDate()}</span>
              {ws.map((w) => (
                <button
                  key={w.workout_id}
                  className={w.workout_id === selectedId ? "cal-workout selected" : "cal-workout"}
                  style={{ background: color(w.name) }}
                  title={w.name}
                  onClick={() => navigate(`/history/${w.workout_id}`)}
                >
                  {span === "week" ? w.name : ""}
                </button>
              ))}
            </div>
          );
        })}
      </div>
      {legend.length > 0 && (
        <div className="chart-legend small" style={{ marginTop: 8 }}>
          {legend.map((n) => (
            <span key={n}>
              <i className="dot" style={{ background: color(n) }} />
              {n}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function mondayOf(d: Date) {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - ((d.getDay() + 6) % 7));
}

// ---- Detail and summary -------------------------------------------------------------------------------

/** PRs grouped by exercise, in workout order. */
export function groupPrs(prs: PrView[]): [string, PrView[]][] {
  const m = new Map<string, PrView[]>();
  prs.forEach((p) => m.set(p.exercise_name, [...(m.get(p.exercise_name) ?? []), p]));
  return [...m.entries()];
}

/** Short PR labels, in display order (like Strong's trophy tags). */
export const PR_TAG: Record<PrView["kind"], string> = {
  e1rm: "1RM",
  weight: "Weight",
  rep_max: "Reps",
  reps: "Reps",
  volume: "Vol",
  pace: "Pace",
  distance: "Distance",
  duration: "Time",
};

/** The distinct short tags for an exercise's PRs, in display order. */
export function prTags(ps: PrView[]): string[] {
  const order = Object.keys(PR_TAG);
  return [...new Set([...ps].sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind)).map((p) => PR_TAG[p.kind]))];
}

export function prText(p: PrView, unit: WeightUnit): string {
  const w = (kg: number) => `${formatWeight(kg, unit)} ${unit}`;
  switch (p.kind) {
    case "e1rm":
      return `Est. 1RM ${w(p.value)}`;
    case "weight":
      return `Heaviest ${w(p.value)}`;
    case "volume":
      return `Volume ${formatVolume(p.value, unit)}`;
    case "rep_max":
      return p.reps === 12 ? `12+ rep max ${w(p.value)}` : `${p.reps}-rep max ${w(p.value)}`;
    case "reps":
      return `${p.value} reps`;
    case "distance":
      return `Longest ${distText(p.value, unit)}`;
    case "duration":
      return `Longest ${formatClock(p.value)}`;
    case "pace":
      // Stored as speed (m/s); shown as pace, speed or /500 m for the activity.
      return `Fastest ${paceText(p.value * 1000, 1000, paceMode(p.exercise_name), unit)}`;
  }
}

function Detail({ id, summary }: { id: number; summary: boolean }) {
  const { unit } = usePrefs();
  const [w, setW] = useState<WorkoutDetail | null | undefined>(undefined);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const [naming, setNaming] = useState(false);
  const shareRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setW(undefined);
    setMsg(null);
    api.workoutDetail(id).then(setW);
    api.muscles().then((m) => setMuscles(m.muscles));
  }, [id]);

  if (w === undefined) return <p className="muted">Loading…</p>;
  if (w === null) return <p className="empty">Workout not found.</p>;
  if (!w.finished) {
    navigate("/workout");
    return null;
  }
  const names = Object.fromEntries(muscles.map((m) => [m.id, m.name]));
  const sets = w.exercises.reduce((n, e) => n + e.sets.length, 0);
  const muscleValues = Object.fromEntries(w.muscle_sets);

  async function repeat() {
    try {
      await api.startWorkout({ kind: "repeat", workout_id: w!.id });
      navigate("/workout");
    } catch (e) {
      setMsg(String(e));
    }
  }
  async function del() {
    if (!(await ask(`Delete “${w!.name}” on ${formatDate(w!.started_at_local)}? This can't be undone.`))) return;
    await api.deleteWorkout(w!.id);
    navigate("/history");
    window.location.reload();
  }
  async function saveTemplate(name: string | null) {
    setNaming(false);
    if (!name) return;
    const r = await api.templateFromWorkout(w!.id, name);
    setMsg(
      `Saved template “${name}”.` +
        (r.uncertain_notes.length ? ` Check targets for: ${r.uncertain_notes.map((u) => u[0]).join(", ")} (notes were unclear).` : ""),
    );
  }

  let normalIndex = 0;
  return (
    <article className="detail">
      <button className="back-link" onClick={() => navigate("/history")}>
        ‹ Workouts
      </button>
      {summary && <div className="summary-banner">Workout complete 🎉</div>}
      <div ref={shareRef}>
        <h2 className="detail-title">{w.name}</h2>
        <div className="muted">
          {formatDate(w.started_at_local)} · {formatTime(w.started_at_local)}
        </div>
        <div className="stats-row">
          {w.duration_s != null && <Stat label="Duration" value={formatDuration(w.duration_s)} />}
          <Stat label="Volume" value={formatVolume(w.volume_kg, unit)} />
          <Stat label="Sets" value={String(sets)} />
          <Stat label="PRs" value={String(w.prs.length)} />
        </div>
      </div>
      <div className="row action-row">
        <button className="button small" onClick={() => navigate(`/history/${w.id}/edit`)}>
          Edit
        </button>
        <button className="button small" onClick={() => setNaming(true)}>
          Save as template
        </button>
        {naming && <TextPrompt title="Template name" initial={w.name} onDone={saveTemplate} />}
        <ShareButton kind="workout" workoutId={w.id} />
        <button className="button small danger" onClick={del}>
          Delete
        </button>
      </div>
      {msg && <p className="success small">{msg}</p>}
      {w.notes && <p className="note">{w.notes}</p>}

      {w.prs.length > 0 && (
        <section className="card pr-list">
          <div className="section-label">Personal records</div>
          {/* Exercise + trophy tags; tap a row for the numbers. */}
          {groupPrs(w.prs).map(([name, ps]) => (
            <details key={name} className="small pr-row">
              <summary>
                <strong>{name}</strong>
                {prTags(ps).map((t) => (
                  <span key={t} className="pr-tag">
                    🏆 {t}
                  </span>
                ))}
              </summary>
              <span className="muted">{ps.map((p) => prText(p, unit)).join(" · ")}</span>
            </details>
          ))}
        </section>
      )}

      {(summary || w.muscle_sets.length > 0) && (
        <details className="card" open={summary}>
          <summary className="section-label">Muscles worked</summary>
          <div className="summary-muscles">
            <BodyMap values={muscleValues} height={220} legendLabel="sets" muscleNames={names} />
            <ul className="muscle-bars">
              {[...w.muscle_sets]
                .sort((a, b) => b[1] - a[1])
                .map(([m, n]) => (
                  <li key={m} className="small">
                    <span>{names[m] ?? m}</span>
                    <span className="bar-track">
                      <span className="bar-fill" style={{ width: `${(n / Math.max(...w.muscle_sets.map((x) => x[1]))) * 100}%` }} />
                    </span>
                    <span className="num-cell">{Math.round(n * 10) / 10}</span>
                  </li>
                ))}
            </ul>
          </div>
        </details>
      )}

      {w.exercises.map((e) => {
        normalIndex = 0;
        return (
          <section key={e.id} className="exercise-block superset-row" data-group={e.superset ?? undefined}>
            <header className="exercise-head">
              <h3>{e.exercise_id ? <a href={`#/exercises/${e.exercise_id}`}>{e.name}</a> : e.name}</h3>
              <div className="chips">
                {e.equipment && <span className="chip">{EQUIPMENT_LABELS[e.equipment] ?? e.equipment}</span>}
                {e.load_mode === "per_hand" && <span className="chip">per hand</span>}
                {!e.exercise_slug && <span className="chip warn">not mapped</span>}
                {e.next_intent && <span className="chip">next: {{ up: "↑ go up", hold: "= hold", down: "↓ go down" }[e.next_intent]}</span>}
                {(["elbow", "knee", "shoulder"] as const).map(
                  (j) =>
                    e[j] != null && (
                      <span key={j} className="chip warn">
                        {j} {e[j]}/5
                      </span>
                    ),
                )}
              </div>
              {e.source_name !== e.name && <div className="muted small">Imported as “{e.source_name}”</div>}
            </header>
            {e.notes && <p className="note small">{e.notes}</p>}
            <table className="sets">
              <tbody>
                {e.sets.map((s) => {
                  if (s.set_type === "normal" || s.set_type === "failure") normalIndex++;
                  return (
                    <tr key={s.id} className={`set-${s.set_type}`}>
                      <td className="set-label">{setLabel(s.set_type, normalIndex)}</td>
                      <td>
                        {s.reps != null
                          ? s.weight_kg
                            ? `${formatWeight(s.weight_kg, unit)} ${unit} × ${s.reps}`
                            : `${s.reps} reps`
                          : [formatDistance(s.distance_m, unit), formatClock(s.duration_s)].filter(Boolean).join(" · ")}
                      </td>
                      <td className="muted small right">{s.rest_s ? `rest ${s.rest_s}s` : ""}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </section>
        );
      })}
      <div className="perform-bar">
        <button className="button primary big" onClick={repeat}>
          Perform again
        </button>
      </div>
    </article>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <div className="stat-value">{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
