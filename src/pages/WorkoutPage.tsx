import { ask } from "../components/Modal";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, type ExerciseView, type Muscle, type Previous, type SetView, type WorkoutDetail } from "../api";
import { navigate, usePrefs } from "../App";
import { ExercisePicker } from "../components/ExercisePicker";
import { Modal } from "../components/Modal";
import { RestTimer, useRestTimer } from "../components/RestTimer";
import { EQUIPMENT_LABELS, formatClock, formatWeight, parseLocal, setLabel } from "../format";

const KG_PER_LB = 0.45359237;

/** Live workout (`mode="live"`) or editing a finished workout from history (`mode="edit"`). */
export default function WorkoutPage({ id, mode }: { id: number; mode: "live" | "edit" }) {
  const [w, setW] = useState<WorkoutDetail | null>(null);
  const [prev, setPrev] = useState<Map<number, Previous>>(new Map());
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [picker, setPicker] = useState<{ kind: "add" } | { kind: "swap"; we: ExerciseView } | null>(null);
  const [reordering, setReordering] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    const d = await api.workoutDetail(id);
    setW(d);
    const p = await api.previousForWorkout(id);
    setPrev(new Map(p.map((x) => [x.workout_exercise_id, x])));
  }, [id]);

  useEffect(() => {
    reload();
    api.muscles().then((m) => setMuscles(m.muscles));
  }, [reload]);

  const timer = useRestTimer(mode === "live" ? w : null);

  const run = (f: () => Promise<unknown>) =>
    f()
      .then(reload)
      .catch((e) => setError(String(e)));

  if (!w) return <p className="muted page">Loading…</p>;
  if (mode === "live" && w.finished) {
    navigate(`/history/${w.id}`);
    return null;
  }

  async function finish() {
    const open = w!.exercises.flatMap((e) => e.sets).filter((s) => !s.completed).length;
    const done = w!.exercises.flatMap((e) => e.sets).filter((s) => s.completed).length;
    if (done === 0) {
      if (await ask("No sets are logged. Discard this workout?")) {
        await api.deleteWorkout(w!.id);
        navigate("/today");
      }
      return;
    }
    if (open > 0 && !(await ask(`${open} set${open > 1 ? "s" : ""} not ticked will be dropped. Finish?`))) return;
    const r = await api.finishWorkout(w!.id);
    timer.clear();
    if (r.differs_from_template && (await ask(`Update the “${w!.name}” template with today's exercises, order, sets and rest times?`))) {
      await api.updateTemplateFromWorkout(w!.id);
    }
    navigate(`/history/${w!.id}/summary`);
  }

  async function discard() {
    if (!(await ask("Cancel this workout? Logged sets will be deleted."))) return;
    timer.clear();
    await api.deleteWorkout(w!.id);
    navigate("/today");
  }

  async function doneEditing() {
    await api.rebuildPrs();
    navigate(`/history/${w!.id}`);
  }

  const groups = groupSupersets(w.exercises);
  const nextOf = (e: ExerciseView) => {
    const i = w.exercises.findIndex((x) => x.id === e.id);
    const n = w.exercises[i + 1];
    return n && (e.superset === null || n.superset !== e.superset) ? n : null;
  };

  return (
    <div className="page narrow workout-page">
      <header className="workout-head">
        <div className="workout-title-row">
          <input
            className="title-input"
            defaultValue={w.name}
            aria-label="Workout name"
            onBlur={(e) =>
              e.target.value.trim() && e.target.value !== w.name && run(() => api.updateWorkout(w.id, { name: e.target.value }))
            }
          />
          {mode === "live" ? (
            <button className="button primary" onClick={finish}>
              Finish
            </button>
          ) : (
            <button className="button primary" onClick={doneEditing}>
              Done
            </button>
          )}
        </div>
        <div className="row small muted">
          {mode === "live" ? <Elapsed start={w.started_at_local} /> : <EditTime w={w} onSaved={reload} />}
          <button className="link-btn" onClick={() => setReordering(true)}>
            Reorder
          </button>
          {mode === "live" && (
            <button className="link-btn danger" onClick={discard}>
              Cancel workout
            </button>
          )}
        </div>
        <textarea
          className="notes-input"
          rows={1}
          placeholder="Workout notes"
          defaultValue={w.notes ?? ""}
          onBlur={(e) => (e.target.value || null) !== w.notes && run(() => api.updateWorkout(w.id, { notes: e.target.value || null }))}
        />
      </header>

      {error && <p className="error">{error}</p>}

      {groups.map((g, gi) => (
        <section key={gi} className={g.superset !== null ? "card superset-card superset-row" : "card"} data-group={g.superset ?? undefined}>
          {g.superset !== null && (
            <div className="superset-label small">
              Superset · rounds of {g.items.map((_, i) => `${String.fromCharCode(65 + (g.superset! % 26))}${i + 1}`).join(" → ")}
            </div>
          )}
          {g.items.map((e) => (
            <ExerciseBlock
              key={e.id}
              e={e}
              prev={prev.get(e.id)}
              muscles={muscles}
              live={mode === "live"}
              onChanged={reload}
              onError={setError}
              onCompleted={(rest) => mode === "live" && rest > 0 && timer.start(rest, nextSetLabel(w, e.id))}
              onSwap={() => setPicker({ kind: "swap", we: e })}
              onSuperset={nextOf(e) ? () => run(() => api.reorderExercises(w.id, groupWithNext(w.exercises, e.id))) : undefined}
              onUnsuperset={e.superset !== null ? () => run(() => api.reorderExercises(w.id, ungroup(w.exercises, e.id))) : undefined}
            />
          ))}
        </section>
      ))}

      <button className="button wide-btn" onClick={() => setPicker({ kind: "add" })}>
        + Add exercise
      </button>

      {mode === "live" && <RestTimer timer={timer} />}

      {picker && (
        <ExercisePicker
          title={picker.kind === "add" ? "Add exercise" : `Swap ${picker.we.name}`}
          similarTo={picker.kind === "swap" ? picker.we.exercise_id : null}
          onClose={() => setPicker(null)}
          onPick={(ex) => {
            const p = picker;
            setPicker(null);
            run(() => (p.kind === "add" ? api.addExercise(w.id, ex.id) : api.swapExercise(p.we.id, ex.id)));
          }}
        />
      )}
      {reordering && (
        <ReorderDialog
          exercises={w.exercises}
          onClose={() => setReordering(false)}
          onSave={(order) => {
            setReordering(false);
            run(() => api.reorderExercises(w.id, order));
          }}
        />
      )}
    </div>
  );
}

type Order = { workout_exercise_id: number; superset: number | null }[];

/** What comes after ticking a set of exercise `justDone`: in a superset the partner, otherwise the
 *  next unticked set in order. Counts the set being ticked as done (state hasn't reloaded yet). */
function nextSetLabel(w: WorkoutDetail, justDone: number): string {
  const ex = w.exercises;
  const i = ex.findIndex((e) => e.id === justDone);
  const open = (e: ExerciseView, extra: number) => e.sets.filter((s) => !s.completed).length - extra;
  const label = (e: ExerciseView, extra: number) => `Next: ${e.name} · set ${e.sets.length - open(e, extra) + 1}`;
  const cur = ex[i];
  if (cur && cur.superset !== null) {
    const group = ex.filter((e) => e.superset === cur.superset);
    const after = [...group.slice(group.indexOf(cur) + 1), ...group.slice(0, group.indexOf(cur) + 1)];
    const partner = after.find((e) => open(e, e.id === justDone ? 1 : 0) > 0);
    if (partner) return label(partner, partner.id === justDone ? 1 : 0);
  }
  for (const e of [...ex.slice(i), ...ex.slice(0, i)]) {
    const extra = e.id === justDone ? 1 : 0;
    if (open(e, extra) > 0) return label(e, extra);
  }
  return "Last set done. Finish when you're ready";
}

/** Puts an exercise in a superset with the one after it (joining its group if it has one). */
function groupWithNext(exercises: ExerciseView[], id: number): Order {
  const i = exercises.findIndex((x) => x.id === id);
  const fresh = Math.max(-1, ...exercises.map((x) => x.superset ?? -1)) + 1;
  const g = exercises[i].superset ?? exercises[i + 1]?.superset ?? fresh;
  return exercises.map((x, j) => ({ workout_exercise_id: x.id, superset: j === i || j === i + 1 ? g : x.superset }));
}

function ungroup(exercises: ExerciseView[], id: number): Order {
  return exercises.map((x) => ({ workout_exercise_id: x.id, superset: x.id === id ? null : x.superset }));
}

function groupSupersets(exercises: ExerciseView[]) {
  const out: { superset: number | null; items: ExerciseView[] }[] = [];
  for (const e of exercises) {
    const last = out[out.length - 1];
    if (e.superset !== null && last && last.superset === e.superset) last.items.push(e);
    else out.push({ superset: e.superset, items: [e] });
  }
  return out;
}

function Elapsed({ start }: { start: string }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return <span className="elapsed">⏱ {formatClock(Math.max(0, (now - parseLocal(start).getTime()) / 1000))}</span>;
}

function EditTime({ w, onSaved }: { w: WorkoutDetail; onSaved: () => void }) {
  return (
    <>
      <input
        type="datetime-local"
        defaultValue={w.started_at_local.slice(0, 16).replace(" ", "T")}
        onBlur={(e) =>
          e.target.value && api.updateWorkout(w.id, { started_at_local: e.target.value.replace("T", " ") + ":00" }).then(onSaved)
        }
      />
      <label>
        <input
          type="number"
          inputMode="numeric"
          className="mini-input"
          defaultValue={w.duration_s ? Math.round(w.duration_s / 60) : ""}
          onBlur={(e) => api.updateWorkout(w.id, { duration_s: e.target.value ? Number(e.target.value) * 60 : null }).then(onSaved)}
        />{" "}
        min
      </label>
    </>
  );
}

// ---- One exercise ---------------------------------------------------------------------------------

function ExerciseBlock({
  e,
  prev,
  muscles,
  live,
  onChanged,
  onError,
  onCompleted,
  onSwap,
  onSuperset,
  onUnsuperset,
}: {
  e: ExerciseView;
  prev?: Previous;
  muscles: Muscle[];
  live: boolean;
  onChanged: () => void;
  onError: (m: string) => void;
  onCompleted: (restS: number) => void;
  onSwap: () => void;
  onSuperset?: () => void;
  onUnsuperset?: () => void;
}) {
  const { unit } = usePrefs();
  const [more, setMore] = useState(false);
  const [editingSticky, setEditingSticky] = useState(false);
  const name = (id: string) => muscles.find((m) => m.id === id)?.name ?? id;
  const run = (f: () => Promise<unknown>) =>
    f()
      .then(onChanged)
      .catch((x) => onError(String(x)));
  const update = (u: Parameters<typeof api.updateWorkoutExercise>[1]) => run(() => api.updateWorkoutExercise(e.id, u));
  const perHand = e.load_mode === "per_hand";
  const kind = e.kind ?? "weight_reps";
  const reps = e.rep_min && e.rep_max && e.rep_min !== e.rep_max ? `${e.rep_min}–${e.rep_max}` : (e.rep_max ?? e.rep_min);
  const target =
    e.target_sets && reps ? `${e.target_sets} × ${reps}` : e.target_sets ? `${e.target_sets} sets` : reps ? `${reps} reps` : null;
  let normal = 0;

  return (
    <div className="exercise-live">
      <header className="exercise-live-head">
        <div>
          <h3>{e.name}</h3>
          <div className="chips">
            {target && <span className="chip strong">{target}</span>}
            {e.equipment && <span className="chip">{EQUIPMENT_LABELS[e.equipment] ?? e.equipment}</span>}
            {perHand && <span className="chip">per hand</span>}
            {prev?.primary.map((m) => (
              <span key={m} className="chip">
                {name(m)}
              </span>
            ))}
          </div>
        </div>
        <select
          className="menu-select"
          value=""
          aria-label="Exercise options"
          onChange={(x) => {
            const v = x.target.value;
            x.target.value = "";
            if (v === "swap") onSwap();
            if (v === "superset") onSuperset?.();
            if (v === "unsuperset") onUnsuperset?.();
            if (v === "warmup") run(() => api.addWarmupSet(e.id));
            if (v === "note") setMore(true);
            if (v === "sticky") setEditingSticky(true);
            if (v === "remove")
              ask(`Remove ${e.name}?`).then((ok) => {
                if (ok) run(() => api.removeExercise(e.id));
              });
            if (v === "info" && e.exercise_id) navigate(`/exercises/${e.exercise_id}`);
            if (v === "perhand") run(() => api.updateWorkoutExercise(e.id, { load_mode: perHand ? "total" : "per_hand" }));
          }}
        >
          <option value="">⋯</option>
          <option value="swap">Replace exercise</option>
          {onSuperset && <option value="superset">Create superset with next</option>}
          {onUnsuperset && <option value="unsuperset">Remove from superset</option>}
          <option value="warmup">Add warm-up set</option>
          <option value="note">Add note (this workout)</option>
          {e.exercise_id && <option value="sticky">Sticky note (every time)</option>}
          <option value="perhand">{perHand ? "Weights are total" : "Weights are per hand"}</option>
          <option value="info">History & charts</option>
          <option value="remove">Remove exercise</option>
        </select>
      </header>

      {editingSticky && e.exercise_id ? (
        <StickyEditor
          initial={prev?.setup_note ?? ""}
          onDone={(note) => {
            setEditingSticky(false);
            if (note !== null) run(() => api.setStickyNote(e.exercise_id!, note));
          }}
        />
      ) : (
        prev?.setup_note && (
          <button
            className="sticky-note small"
            onClick={() => setEditingSticky(true)}
            title="Sticky note: shown every time you do this exercise"
          >
            📌 {prev.setup_note}
          </button>
        )
      )}
      {prev && (prev.suggest_increase || prev.next_intent || prev.notes) && (
        <div className="last-time small">
          {prev.suggest_increase && <div className="hint">↑ Every set hit the top of the range last time. Try more weight.</div>}
          {prev.next_intent && (
            <div>Last time you planned to {{ up: "go up ↑", hold: "hold =", down: "go down ↓" }[prev.next_intent]}.</div>
          )}
          {prev.notes && <div className="muted">“{prev.notes}”</div>}
        </div>
      )}

      <table className="set-table">
        <thead>
          <tr className="muted small">
            <th>Set</th>
            <th>Previous</th>
            {kind === "distance_duration" ? (
              <>
                <th>{unit === "lb" ? "mi" : "km"}</th>
                <th>Time</th>
              </>
            ) : kind === "duration" ? (
              <th>Seconds</th>
            ) : (
              <>
                <th>
                  {kind === "bodyweight_reps" ? "+" : kind === "assisted_bodyweight" ? "−" : ""}
                  {unit}
                  {perHand ? "/hand" : ""}
                </th>
                <th>Reps</th>
              </>
            )}
            <th aria-label="Done" />
          </tr>
        </thead>
        <tbody>
          {e.sets.map((s, i) => {
            if (s.set_type === "normal" || s.set_type === "failure") normal++;
            return (
              <SetRow
                key={s.id}
                s={s}
                label={setLabel(s.set_type, normal)}
                p={prev?.sets[i]}
                kind={kind}
                live={live}
                onChanged={onChanged}
                onError={onError}
                onCompleted={() => onCompleted(e.rest_s ?? 0)}
              />
            );
          })}
        </tbody>
      </table>

      <div className="exercise-actions">
        <button className="link-btn" onClick={() => run(() => api.addSet(e.id))}>
          + Set
        </button>
        <label className="small muted">
          Rest{" "}
          <select className="select inline" value={e.rest_s ?? 0} onChange={(x) => update({ rest_s: Number(x.target.value) })}>
            {[0, 15, 30, 45, 60, 75, 90, 105, 120, 150, 180, 240, 300]
              .concat(e.rest_s && ![0, 15, 30, 45, 60, 75, 90, 105, 120, 150, 180, 240, 300].includes(e.rest_s) ? [e.rest_s] : [])
              .map((r) => (
                <option key={r} value={r}>
                  {r === 0 ? "off" : formatClock(r)}
                </option>
              ))}
          </select>
        </label>
        <div className="intent" role="group" aria-label="Next time">
          <span className="small muted">Next time</span>
          {(["down", "hold", "up"] as const).map((v) => (
            <button
              key={v}
              className={e.next_intent === v ? "intent-btn active" : "intent-btn"}
              title={{ up: "Go up", hold: "Hold", down: "Go down" }[v]}
              onClick={() => update({ next_intent: e.next_intent === v ? null : v })}
            >
              {{ up: "↑", hold: "=", down: "↓" }[v]}
            </button>
          ))}
        </div>
        <button className="link-btn" onClick={() => setMore(!more)}>
          {more ? "Less" : e.notes ? "Note ✎" : "Note & joints"}
        </button>
      </div>

      {more && (
        <div className="exercise-more">
          <textarea
            rows={2}
            placeholder="Notes for this exercise today"
            defaultValue={e.notes ?? ""}
            onBlur={(x) => (x.target.value || null) !== e.notes && update({ notes: x.target.value || null })}
          />
          {(["elbow", "knee", "shoulder"] as const).map((j) => (
            <div key={j} className="rating-row small">
              <span>{j[0].toUpperCase() + j.slice(1)} discomfort</span>
              {[1, 2, 3, 4, 5].map((n) => (
                <button key={n} className={e[j] === n ? "rating active" : "rating"} onClick={() => update({ [j]: e[j] === n ? null : n })}>
                  {n}
                </button>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function SetRow({
  s,
  label,
  p,
  kind,
  live,
  onChanged,
  onError,
  onCompleted,
}: {
  s: SetView;
  label: string;
  p?: { weight_kg: number | null; reps: number | null; distance_m: number | null; duration_s: number | null };
  kind: string;
  live: boolean;
  onChanged: () => void;
  onError: (m: string) => void;
  onCompleted: () => void;
}) {
  const { unit } = usePrefs();
  const distUnit = unit === "lb" ? 1609.344 : 1000;
  const fmtNum = (v: number | null, d = 2) => (v == null ? "" : String(Math.round(v * 10 ** d) / 10 ** d));
  const [a, setA] = useState(""); // weight / distance / seconds
  const [b, setB] = useState(""); // reps / time
  const first = useRef(true);
  useEffect(() => {
    if (kind === "distance_duration") {
      setA(fmtNum(s.distance_m != null ? s.distance_m / distUnit : null));
      setB(s.duration_s != null ? formatClock(s.duration_s) : "");
    } else if (kind === "duration") {
      setA(fmtNum(s.duration_s, 0));
    } else {
      setA(s.weight_kg == null ? "" : formatWeight(s.weight_kg, unit));
      setB(s.reps == null ? "" : String(s.reps));
    }
    first.current = false;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [s.id, s.weight_kg, s.reps, s.distance_m, s.duration_s, unit, kind]);

  const placeholderA =
    kind === "distance_duration"
      ? fmtNum(p?.distance_m != null ? p.distance_m / distUnit : null)
      : kind === "duration"
        ? fmtNum(p?.duration_s ?? null, 0)
        : p?.weight_kg != null
          ? formatWeight(p.weight_kg, unit)
          : "";
  const placeholderB =
    kind === "distance_duration" ? (p?.duration_s != null ? formatClock(p.duration_s) : "") : p?.reps != null ? String(p.reps) : "";
  const prevText = !p
    ? "—"
    : kind === "distance_duration"
      ? `${placeholderA} · ${placeholderB}`
      : kind === "duration"
        ? `${placeholderA}s`
        : p.weight_kg
          ? `${formatWeight(p.weight_kg, unit)} × ${p.reps ?? "?"}`
          : `${p.reps ?? "?"} reps`;

  const parseClock = (t: string): number | null => {
    if (!t.trim()) return null;
    const parts = t.split(":").map(Number);
    if (parts.some(Number.isNaN)) return null;
    return parts.reduce((acc, v) => acc * 60 + v, 0);
  };

  function values(useFallback: boolean) {
    const va = a.trim() || (useFallback ? placeholderA : "");
    const vb = b.trim() || (useFallback ? placeholderB : "");
    if (kind === "distance_duration") return { distance_m: va ? Number(va) * distUnit : null, duration_s: parseClock(vb) };
    if (kind === "duration") return { duration_s: va ? Number(va) : null };
    const w = va === "" ? (kind === "weight_reps" ? null : 0) : Number(va);
    return { weight_kg: w == null ? null : unit === "lb" ? w * KG_PER_LB : w, reps: vb === "" ? null : Math.round(Number(vb)) };
  }

  const save = (extra: object = {}) =>
    api
      .updateSet(s.id, { ...values(false), ...extra })
      .then(onChanged)
      .catch((x) => onError(String(x)));

  async function toggle() {
    if (s.completed) return save({ completed: false });
    const v = values(true);
    const missing =
      kind === "distance_duration"
        ? v.distance_m == null && v.duration_s == null
        : kind === "duration"
          ? v.duration_s == null
          : (v as { reps: number | null }).reps == null;
    if (missing) return onError("Enter the set first (nothing to copy from last time).");
    await api.updateSet(s.id, { ...v, completed: true }).catch((x) => onError(String(x)));
    onChanged();
    if (live) onCompleted();
  }

  return (
    <tr className={s.completed ? `done set-${s.set_type}` : `set-${s.set_type}`}>
      <td>
        <select
          className="set-type"
          value={s.set_type}
          aria-label="Set type"
          onChange={(x) => {
            if (x.target.value === "delete") api.removeSet(s.id).then(onChanged);
            else api.updateSet(s.id, { set_type: x.target.value }).then(onChanged);
          }}
        >
          <option value="normal">{label}</option>
          <option value="warmup">W · warm-up</option>
          <option value="drop">D · drop</option>
          <option value="failure">F · failure</option>
          <option value="delete">Delete set</option>
        </select>
      </td>
      <td className="muted small prev-cell">{prevText}</td>
      <td>
        <input
          inputMode="decimal"
          className="num"
          value={a}
          placeholder={placeholderA}
          onChange={(x) => setA(x.target.value)}
          onBlur={() => !first.current && save()}
        />
      </td>
      {kind !== "duration" && (
        <td>
          <input
            inputMode={kind === "distance_duration" ? "text" : "numeric"}
            className="num"
            value={b}
            placeholder={placeholderB}
            onChange={(x) => setB(x.target.value)}
            onBlur={() => save()}
          />
        </td>
      )}
      <td>
        <button className={s.completed ? "check done" : "check"} onClick={toggle} aria-label={s.completed ? "Mark not done" : "Done"}>
          ✓
        </button>
      </td>
    </tr>
  );
}

// ---- Reorder and superset grouping ------------------------------------------------------------------

function ReorderDialog({
  exercises,
  onClose,
  onSave,
}: {
  exercises: ExerciseView[];
  onClose: () => void;
  onSave: (order: { workout_exercise_id: number; superset: number | null }[]) => void;
}) {
  const [items, setItems] = useState(exercises.map((e) => ({ id: e.id, name: e.name, superset: e.superset })));
  const [drag, setDrag] = useState<{ index: number; y: number; startY: number } | null>(null);
  const rowH = 52;

  const move = (from: number, to: number) => {
    if (to < 0 || to >= items.length || from === to) return;
    const next = items.slice();
    const [it] = next.splice(from, 1);
    next.splice(to, 0, it);
    setItems(next);
  };

  // Link item i with the one after it into a superset (or unlink).
  const linked = (i: number) => items[i].superset !== null && items[i].superset === items[i + 1]?.superset;
  const toggleLink = (i: number) => {
    const next = items.map((x) => ({ ...x }));
    if (linked(i)) {
      // Split: everything after i in this group gets a new id.
      const g = next[i].superset;
      const fresh = Math.max(-1, ...next.map((x) => x.superset ?? -1)) + 1;
      for (let j = i + 1; j < next.length && next[j].superset === g; j++) next[j].superset = fresh;
    } else {
      const g = next[i].superset ?? next[i + 1].superset ?? Math.max(-1, ...next.map((x) => x.superset ?? -1)) + 1;
      const old = next[i + 1].superset;
      next[i].superset = g;
      for (let j = i + 1; j < next.length && (j === i + 1 || (old !== null && next[j].superset === old)); j++) next[j].superset = g;
    }
    setItems(next);
  };

  const ordered = useMemo(() => items, [items]);

  return (
    <Modal title="Reorder & supersets" onClose={onClose}>
      <p className="muted small">Drag ≡ to reorder. Tap ⛓ between two exercises to make (or break) a superset.</p>
      <div
        className="reorder-list"
        onPointerMove={(e) => {
          if (!drag) return;
          const dy = e.clientY - drag.startY;
          const target = Math.max(0, Math.min(items.length - 1, drag.index + Math.round(dy / rowH)));
          if (target !== drag.index) {
            move(drag.index, target);
            setDrag({ index: target, y: e.clientY, startY: drag.startY + (target - drag.index) * rowH });
          } else setDrag({ ...drag, y: e.clientY });
        }}
        onPointerUp={() => setDrag(null)}
        onPointerCancel={() => setDrag(null)}
      >
        {ordered.map((it, i) => (
          <div key={it.id}>
            <div
              className={drag?.index === i ? "reorder-row dragging superset-row" : "reorder-row superset-row"}
              data-group={it.superset ?? undefined}
              style={{ height: rowH - 8 }}
            >
              <span
                className="handle"
                onPointerDown={(e) => {
                  (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
                  setDrag({ index: i, y: e.clientY, startY: e.clientY });
                }}
              >
                ≡
              </span>
              <span className="ex-name">{it.name}</span>
              <span className="row" style={{ marginTop: 0 }}>
                <button className="icon-btn" onClick={() => move(i, i - 1)} aria-label="Move up">
                  ↑
                </button>
                <button className="icon-btn" onClick={() => move(i, i + 1)} aria-label="Move down">
                  ↓
                </button>
              </span>
            </div>
            {i < ordered.length - 1 && (
              <button className={linked(i) ? "link-toggle on" : "link-toggle"} onClick={() => toggleLink(i)}>
                ⛓ {linked(i) ? "superset" : ""}
              </button>
            )}
          </div>
        ))}
      </div>
      <div className="row">
        <button className="button primary" onClick={() => onSave(items.map((x) => ({ workout_exercise_id: x.id, superset: x.superset })))}>
          Save order
        </button>
        <button className="button" onClick={onClose}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}

function StickyEditor({ initial, onDone }: { initial: string; onDone: (note: string | null) => void }) {
  const [v, setV] = useState(initial);
  return (
    <div className="sticky-editor">
      <div className="small muted">Sticky note: shown every time you log this exercise (seat height, cues, reminders).</div>
      <textarea rows={2} value={v} autoFocus onChange={(x) => setV(x.target.value)} />
      <div className="row" style={{ marginTop: 4 }}>
        <button className="button small primary" onClick={() => onDone(v)}>
          Save
        </button>
        <button className="button small" onClick={() => onDone(null)}>
          Cancel
        </button>
      </div>
    </div>
  );
}
