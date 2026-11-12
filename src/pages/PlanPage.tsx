import { ask, tell } from "../components/Modal";
import { useEffect, useState } from "react";
import {
  api,
  type Muscle,
  type MuscleTarget,
  type Phase,
  type Program,
  type RoutinePreview,
  type ProposedPhase,
  type Schedule,
  type Template,
  type TemplateExercise,
} from "../api";
import { navigate } from "../App";
import { today } from "../charts/RangePicker";
import { CATEGORICAL_LIGHT, templateColor, useChartTheme } from "../charts/theme";
import { ExercisePicker } from "../components/ExercisePicker";
import { RoutineReviewTab } from "../components/RoutineReview";
import { TargetRange } from "../charts/TargetRange";
import { formatClock, formatDate } from "../format";
import { saveOrShareText } from "../platform";

type Tab = "templates" | "schedule" | "review" | "phases" | "targets";
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export default function PlanPage({ tab, templateId }: { tab: Tab; templateId: number | "new" | null }) {
  if (templateId !== null) return <TemplateEditor id={templateId} />;
  return (
    <div className="page narrow">
      <a className="back-link" href="#/settings">
        ‹ Settings
      </a>
      <h1 className="page-title">Plan</h1>
      <div className="segmented plan-tabs">
        {(["templates", "schedule", "review", "phases", "targets"] as Tab[]).map((t) => (
          <a key={t} href={`#/plan/${t}`} className={tab === t ? "seg-link active" : "seg-link"}>
            {{ templates: "Templates", schedule: "Schedule", review: "Review", phases: "Phases", targets: "Targets" }[t]}
          </a>
        ))}
      </div>
      {tab === "templates" && <TemplatesTab />}
      {tab === "schedule" && <ScheduleTab />}
      {tab === "review" && <RoutineReviewTab />}
      {tab === "phases" && <PhasesTab />}
      {tab === "targets" && <TargetsTab />}
    </div>
  );
}

function TemplatesTab() {
  const theme = useChartTheme();
  const [templates, setTemplates] = useState<Template[]>([]);
  useEffect(() => {
    api.listTemplates().then(setTemplates);
  }, []);
  return (
    <>
      {templates.length === 0 && (
        <p className="empty">No templates yet. Create one, save one from a past workout, or install a starter program under Schedule.</p>
      )}
      {templates.map((t, i) => (
        <a
          key={t.id}
          className="card template-card"
          href={`#/plan/template/${t.id}`}
          style={{ borderLeft: `4px solid ${templateColor(t.color, theme, i)}` }}
        >
          <div className="card-title">{t.name}</div>
          <div className="muted small">
            {t.exercises.length} exercises{t.last_done && ` · last done ${formatDate(t.last_done)}`}
          </div>
          <div className="small">{t.exercises.map((e) => e.name).join(" · ")}</div>
        </a>
      ))}
      <a className="button wide-btn" href="#/plan/template/new">
        + New template
      </a>
    </>
  );
}

// ---- Template editor --------------------------------------------------------------------------------

function TemplateEditor({ id }: { id: number | "new" }) {
  const theme = useChartTheme();
  const [t, setT] = useState<Template | null>(id === "new" ? { id: null, name: "", notes: null, color: null, exercises: [] } : null);
  const [picker, setPicker] = useState<{ kind: "add" } | { kind: "swap"; index: number } | null>(null);
  const [projected, setProjected] = useState<Record<string, number>>({});
  const [targets, setTargets] = useState<MuscleTarget[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (id !== "new") api.listTemplates().then((all) => setT(all.find((x) => x.id === id) ?? null));
    api.muscleTargets().then(setTargets);
    api.muscles().then((m) => setMuscles(m.muscles));
  }, [id]);
  useEffect(() => {
    if (!t) return;
    api.projectedWeeklySets(t.id ? t : undefined).then((p) => setProjected(Object.fromEntries(p)));
  }, [t]);

  if (!t) return <p className="muted page">Loading…</p>;
  const set = (patch: Partial<Template>) => setT({ ...t, ...patch });
  const setEx = (i: number, patch: Partial<TemplateExercise>) =>
    set({ exercises: t.exercises.map((e, j) => (j === i ? { ...e, ...patch } : e)) });
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= t.exercises.length) return;
    const ex = t.exercises.slice();
    [ex[i], ex[j]] = [ex[j], ex[i]];
    set({ exercises: ex });
  };
  const linked = (i: number) => t.exercises[i].superset !== null && t.exercises[i].superset === t.exercises[i + 1]?.superset;
  const toggleLink = (i: number) => {
    const ex = t.exercises.map((e) => ({ ...e }));
    const fresh = Math.max(-1, ...ex.map((e) => e.superset ?? -1)) + 1;
    if (linked(i)) {
      const g = ex[i].superset;
      for (let j = i + 1; j < ex.length && ex[j].superset === g; j++) ex[j].superset = fresh;
    } else {
      const g = ex[i].superset ?? fresh;
      ex[i].superset = g;
      ex[i + 1].superset = g;
    }
    // Groups of one aren't supersets.
    const counts = new Map<number, number>();
    ex.forEach((e) => e.superset !== null && counts.set(e.superset, (counts.get(e.superset) ?? 0) + 1));
    ex.forEach((e) => {
      if (e.superset !== null && counts.get(e.superset)! < 2) e.superset = null;
    });
    set({ exercises: ex });
  };
  const num = (v: string) => (v === "" ? null : Math.max(0, Math.round(Number(v))));

  async function save() {
    setError(null);
    try {
      const newId = await api.saveTemplate(t!);
      navigate(`/plan/template/${newId}`);
      setT({ ...t!, id: newId });
    } catch (e) {
      setError(String(e));
    }
  }

  const targetRows = targets.filter((x) => x.max_sets > 0);

  return (
    <div className="page template-editor">
      <a className="back-link" href="#/plan/templates">
        ‹ Templates
      </a>
      <div className="editor-layout">
        <div>
          <div className="row" style={{ marginTop: 0 }}>
            <input className="title-input" placeholder="Template name" value={t.name} onChange={(e) => set({ name: e.target.value })} />
            <button className="button primary" onClick={save} disabled={!t.name.trim()}>
              Save
            </button>
          </div>
          <div className="row small">
            Color
            {CATEGORICAL_LIGHT.map((c, i) => (
              <button
                key={c}
                className={t.color === c ? "swatch active" : "swatch"}
                style={{ background: templateColor(c, theme, i) }}
                aria-label={`Color ${i + 1}`}
                onClick={() => set({ color: c })}
              />
            ))}
            {t.id && (
              <button className="link-btn" onClick={() => shareRoutine(t.name, t.id!).catch((e) => setError(String(e)))}>
                Share
              </button>
            )}
            {t.id && (
              <button
                className="link-btn danger"
                onClick={() =>
                  ask(`Archive “${t.name}”? Past workouts keep their history.`).then((ok) => {
                    if (ok) api.archiveTemplate(t.id!).then(() => navigate("/plan/templates"));
                  })
                }
              >
                Archive
              </button>
            )}
          </div>
          <textarea
            className="notes-input"
            rows={1}
            placeholder="Template notes"
            value={t.notes ?? ""}
            onChange={(e) => set({ notes: e.target.value || null })}
          />
          {error && <p className="error">{error}</p>}

          {t.exercises.map((e, i) => (
            <div key={i}>
              <div className="card template-ex superset-row" data-group={e.superset ?? undefined}>
                <div className="title-row">
                  <strong>{e.name}</strong>
                  <span className="row" style={{ marginTop: 0 }}>
                    <button className="icon-btn" onClick={() => move(i, -1)} aria-label="Move up">
                      ↑
                    </button>
                    <button className="icon-btn" onClick={() => move(i, 1)} aria-label="Move down">
                      ↓
                    </button>
                    <button className="icon-btn" onClick={() => setPicker({ kind: "swap", index: i })} aria-label="Swap">
                      ⇄
                    </button>
                    <button
                      className="icon-btn"
                      onClick={() => set({ exercises: t.exercises.filter((_, j) => j !== i) })}
                      aria-label="Remove"
                    >
                      ✕
                    </button>
                  </span>
                </div>
                <div className="form-row compact">
                  <label>
                    Sets
                    <input
                      inputMode="numeric"
                      value={e.target_sets ?? ""}
                      onChange={(x) => setEx(i, { target_sets: num(x.target.value) })}
                    />
                  </label>
                  <label>
                    Reps from
                    <input inputMode="numeric" value={e.rep_min ?? ""} onChange={(x) => setEx(i, { rep_min: num(x.target.value) })} />
                  </label>
                  <label>
                    to
                    <input inputMode="numeric" value={e.rep_max ?? ""} onChange={(x) => setEx(i, { rep_max: num(x.target.value) })} />
                  </label>
                  <label>
                    Rest
                    <select className="select" value={e.rest_s ?? ""} onChange={(x) => setEx(i, { rest_s: num(x.target.value) })}>
                      <option value="">default</option>
                      {[0, 15, 30, 45, 60, 75, 90, 105, 120, 150, 180, 240, 300].map((r) => (
                        <option key={r} value={r}>
                          {r === 0 ? "none" : formatClock(r)}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <input
                  className="setup-input small"
                  placeholder="Sticky note: seat, pin, cable height, cues…"
                  value={e.setup_note ?? ""}
                  onChange={(x) => setEx(i, { setup_note: x.target.value || null })}
                />
              </div>
              {i < t.exercises.length - 1 && (
                <button className={linked(i) ? "link-toggle on" : "link-toggle"} onClick={() => toggleLink(i)}>
                  ⛓ {linked(i) ? "superset" : ""}
                </button>
              )}
            </div>
          ))}
          <button className="button wide-btn" onClick={() => setPicker({ kind: "add" })}>
            + Add exercise
          </button>
        </div>

        <aside className="card projected">
          <div className="section-label">Weekly sets from your whole schedule</div>
          {Object.keys(projected).length === 0 ? (
            <p className="muted small">Put this template in your schedule to see weekly sets per muscle.</p>
          ) : (
            <TargetRange
              rows={targetRows}
              value={(m) => projected[m] ?? 0}
              names={Object.fromEntries(muscles.map((m) => [m.id, m.name]))}
            />
          )}
          <p className="muted small">Updates as you edit (before saving). Secondary muscles count ½.</p>
        </aside>
      </div>
      {picker && (
        <ExercisePicker
          title={picker.kind === "add" ? "Add exercise" : "Swap exercise"}
          similarTo={picker.kind === "swap" ? t.exercises[picker.index].exercise_id : null}
          onClose={() => setPicker(null)}
          onPick={(ex) => {
            const item: TemplateExercise = {
              exercise_id: ex.id,
              name: ex.name,
              superset: null,
              target_sets: 3,
              rep_min: 8,
              rep_max: 12,
              rest_s: ex.default_rest_s,
              setup_note: ex.setup_note,
            };
            if (picker.kind === "add") set({ exercises: [...t.exercises, item] });
            else setEx(picker.index, { exercise_id: ex.id, name: ex.name });
            setPicker(null);
          }}
        />
      )}
    </div>
  );
}

// ---- Sharing routines ------------------------------------------------------------------------------

/** Shares one template, or the active schedule without an id. */
async function shareRoutine(name: string, templateId?: number) {
  const file = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "routine";
  return saveOrShareText(`${file}.fitroutine.json`, "application/json", await api.routineText(templateId));
}

function RoutineSharing({ name, canExport, onImported }: { name: string; canExport: boolean; onImported: () => void }) {
  const [pending, setPending] = useState<{ text: string; preview: RoutinePreview } | null>(null);
  const [activate, setActivate] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = async (f: () => Promise<string | null>) => {
    setMsg(null);
    setError(null);
    try {
      setMsg(await f());
    } catch (e) {
      setError(String(e));
    }
  };
  const load = (getText: () => Promise<string>) =>
    run(async () => {
      const text = await getText();
      setPending({ text, preview: await api.previewRoutine(text) });
      setActivate(false);
      return null;
    });
  return (
    <section className="card">
      <h2 className="section-title">Share a routine</h2>
      <p className="muted small">A routine file holds your schedule, exercises, sets and rep ranges. It never includes your logged workouts or weights.</p>
      <p className="muted small">
        To get a new routine from Claude, share or copy this schedule into a chat and describe what you want. The file tells Claude how to
        write one. For a fuller choice of exercises, add resources.json, which lists them all. Then copy Claude's reply and tap Paste routine.
      </p>
      <div className="row">
        <button
          className="button"
          disabled={!canExport}
          onClick={() =>
            run(async () => {
              const saved = await shareRoutine(name);
              return saved && `Saved ${saved}`;
            })
          }
        >
          Share this schedule
        </button>
        <button
          className="button"
          disabled={!canExport}
          onClick={() =>
            run(async () => {
              await api.copyText(await api.routineText());
              return "Routine copied.";
            })
          }
        >
          Copy
        </button>
        <button className="button" onClick={() => load(api.pasteText)}>
          Paste routine
        </button>
        <button
          className="button"
          onClick={() =>
            run(async () => {
              const saved = await saveOrShareText("resources.json", "application/json", await api.routineResources());
              return saved && `Saved ${saved}`;
            })
          }
        >
          Share resources.json
        </button>
        <button
          className="button"
          onClick={() =>
            run(async () => {
              await api.copyText(await api.routineResources());
              return "resources.json copied.";
            })
          }
        >
          Copy resources.json
        </button>
        <label className="button file-button" style={{ marginTop: 0 }}>
          Import routine…
          <input
            type="file"
            hidden
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (f) load(() => f.text());
            }}
          />
        </label>
      </div>
      {pending && (
        <div className="restore-confirm">
          <strong>{pending.preview.name}</strong>
          {pending.preview.description && <div className="muted small">{pending.preview.description}</div>}
          <div className="small">
            Adds {pending.preview.templates.length === 1 ? "1 template" : `${pending.preview.templates.length} templates`}:{" "}
            {pending.preview.templates.map(([n, count]) => `${n} (${count})`).join(", ")}
          </div>
          {pending.preview.new_exercises.length > 0 && <div className="small">New exercises: {pending.preview.new_exercises.join(", ")}</div>}
          {pending.preview.has_schedule && (
            <label className="check small">
              <input type="checkbox" checked={activate} onChange={(e) => setActivate(e.target.checked)} />
              Make this my active schedule
            </label>
          )}
          <div className="row">
            <button
              className="button primary"
              onClick={() =>
                run(async () => {
                  await api.importRoutine(pending.text, activate, today());
                  const done = `${pending.preview.name} added.`;
                  setPending(null);
                  onImported();
                  return done;
                })
              }
            >
              Import
            </button>
            <button className="button" onClick={() => setPending(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}
      {msg && <p className="success small">{msg}</p>}
      {error && <p className="error">{error}</p>}
    </section>
  );
}

// ---- Schedule -------------------------------------------------------------------------------------

function ScheduleTab() {
  const [s, setS] = useState<Schedule | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [programs, setPrograms] = useState<Program[]>([]);
  const [proposal, setProposal] = useState<ProposedPhase[] | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const load = () => {
    api
      .activeSchedule()
      .then((x) =>
        setS(x ?? { id: null, name: "My schedule", kind: "weekly", start_date: today(), cycle_length: 7, days: Array(7).fill(null) }),
      );
    api.listTemplates().then(setTemplates);
  };
  useEffect(() => {
    load();
    api.programs().then(setPrograms);
  }, []);
  if (!s) return <p className="muted">Loading…</p>;
  const len = s.kind === "weekly" ? 7 : s.cycle_length;
  const days = Array.from({ length: len }, (_, i) => s.days[i] ?? null);

  async function save() {
    await api.saveSchedule({ ...s!, cycle_length: len, days });
    setMsg("Schedule saved.");
  }

  return (
    <>
      <section className="card">
        <div className="segmented small">
          <button className={s.kind === "weekly" ? "active" : ""} onClick={() => setS({ ...s, kind: "weekly" })}>
            Weekly
          </button>
          <button
            className={s.kind === "cycle" ? "active" : ""}
            onClick={() => setS({ ...s, kind: "cycle", cycle_length: s.kind === "cycle" ? s.cycle_length : 6 })}
          >
            Custom cycle
          </button>
        </div>
        <div className="form-row">
          <label>
            {s.kind === "weekly" ? "Starting" : "Cycle starts on"}
            <input type="date" value={s.start_date} onChange={(e) => setS({ ...s, start_date: e.target.value })} />
          </label>
          {s.kind === "cycle" && (
            <label>
              Days in cycle
              <input
                type="number"
                min={1}
                max={28}
                value={s.cycle_length}
                onChange={(e) => setS({ ...s, cycle_length: Math.max(1, Math.min(28, Number(e.target.value))) })}
              />
            </label>
          )}
        </div>
        <div className="schedule-days">
          {days.map((d, i) => (
            <label key={i} className="schedule-day">
              <span className="muted small">{s.kind === "weekly" ? DAYS[i] : `Day ${i + 1}`}</span>
              <select
                className="select"
                value={d ?? ""}
                onChange={(e) => setS({ ...s, days: days.map((x, j) => (j === i ? (e.target.value ? Number(e.target.value) : null) : x)) })}
              >
                <option value="">Rest</option>
                {templates.map((t) => (
                  <option key={t.id} value={t.id!}>
                    {t.name}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
        <button className="button primary" onClick={save}>
          Save schedule
        </button>
        {msg && <p className="success small">{msg}</p>}
      </section>

      <section className="card">
        <h2 className="section-title">Set up from your history</h2>
        <p className="muted small">
          Finds the programs in your imported workouts, creates phases for them, and makes templates and a weekly schedule from the most
          recent one.
        </p>
        {!proposal ? (
          <button className="button" onClick={() => api.proposeFromHistory().then((p) => setProposal(p.phases))}>
            Look at my history
          </button>
        ) : proposal.length === 0 ? (
          <p className="muted small">No repeated workouts found yet.</p>
        ) : (
          <>
            {proposal.map((p, i) => (
              <div key={i} className="proposal">
                <input
                  value={p.name}
                  onChange={(e) => setProposal(proposal.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))}
                />
                <span className="muted small">
                  {formatDate(p.start_date)} – {p.end_date ? formatDate(p.end_date) : "now"} · {p.workout_names.join(", ")}
                </span>
                {i === proposal.length - 1 && (
                  <div className="small">Schedule: {p.usual_weekday.map(([n, d]) => `${DAYS[d]} ${n}`).join(", ")}</div>
                )}
              </div>
            ))}
            <button
              className="button primary"
              onClick={async () => {
                const unclear = await api.applyHistoryProposal({ phases: proposal }, today());
                setProposal(null);
                load();
                setMsg(
                  `Created ${proposal.length} phases, templates and a schedule.` +
                    (unclear.length ? ` Check targets on: ${unclear.map((u) => u[0]).join(", ")}.` : ""),
                );
              }}
            >
              Create phases, templates & schedule
            </button>
          </>
        )}
      </section>

      <RoutineSharing name={s.name} canExport={s.id !== null} onImported={load} />
      <section className="card">
        <h2 className="section-title">Starter programs</h2>
        {programs.map((p) => (
          <div key={p.id} className="start-row">
            <span>
              <strong>{p.name}</strong>
              <div className="muted small">{p.description}</div>
            </span>
            <button
              className="button small"
              onClick={async () => {
                if (!(await ask(`Add ${p.name}? It creates its templates and becomes your active schedule.`))) return;
                await api.installProgram(p.id, today());
                load();
                setMsg(`${p.name} added. Edit its templates any time.`);
              }}
            >
              Use
            </button>
          </div>
        ))}
      </section>
    </>
  );
}

// ---- Phases ---------------------------------------------------------------------------------------

function PhasesTab() {
  const [phases, setPhases] = useState<Phase[]>([]);
  const [editing, setEditing] = useState<Phase | null>(null);
  const load = () => api.listPhases().then(setPhases);
  useEffect(() => {
    load();
  }, []);
  return (
    <>
      <p className="muted small">
        Named date ranges that split and shade your charts. Program phases (PHUL, Upper/Lower…) and nutrition phases (bulk, cut) can
        overlap.
      </p>
      {phases.map((p) => (
        <button key={p.id} className="start-row" onClick={() => setEditing(p)}>
          <span>
            {p.name} <span className="muted small">· {p.lane}</span>
          </span>
          <span className="muted small">
            {formatDate(p.start_date)} – {p.end_date ? formatDate(p.end_date) : "now"}
          </span>
        </button>
      ))}
      <button
        className="button wide-btn"
        onClick={() => setEditing({ id: null, name: "", lane: "nutrition", start_date: today(), end_date: null, color: null, notes: null })}
      >
        + Add phase
      </button>
      {editing && (
        <section className="card">
          <div className="form">
            <div className="form-row">
              <label>
                Name
                <input value={editing.name} placeholder="Bulk" onChange={(e) => setEditing({ ...editing, name: e.target.value })} />
              </label>
              <label>
                Kind
                <select
                  className="select"
                  value={editing.lane}
                  onChange={(e) => setEditing({ ...editing, lane: e.target.value as Phase["lane"] })}
                >
                  <option value="program">Program</option>
                  <option value="nutrition">Nutrition</option>
                </select>
              </label>
            </div>
            <div className="form-row">
              <label>
                Start
                <input type="date" value={editing.start_date} onChange={(e) => setEditing({ ...editing, start_date: e.target.value })} />
              </label>
              <label>
                End (blank = ongoing)
                <input
                  type="date"
                  value={editing.end_date ?? ""}
                  onChange={(e) => setEditing({ ...editing, end_date: e.target.value || null })}
                />
              </label>
            </div>
            <div className="row">
              <button
                className="button primary"
                disabled={!editing.name.trim()}
                onClick={() => api.savePhase(editing).then(() => (setEditing(null), load()))}
              >
                Save
              </button>
              {editing.id && (
                <button className="button danger" onClick={() => api.deletePhase(editing.id!).then(() => (setEditing(null), load()))}>
                  Delete
                </button>
              )}
              <button className="button" onClick={() => setEditing(null)}>
                Cancel
              </button>
            </div>
          </div>
        </section>
      )}
    </>
  );
}

// ---- Targets --------------------------------------------------------------------------------------

function TargetsTab() {
  const [targets, setTargets] = useState<MuscleTarget[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  useEffect(() => {
    api.muscleTargets().then(setTargets);
    api.muscles().then((m) => setMuscles(m.muscles));
  }, []);
  const save = (t: MuscleTarget) => api.setMuscleTarget(t.muscle_id, t.min_sets, t.max_sets);
  return (
    <section className="card">
      <p className="muted small">
        Weekly working sets per muscle; a secondary muscle counts ½ set. The default of 10–20 for every muscle comes from research on
        training volume and muscle growth, not from your own workouts: about 10+ weekly sets grew more than fewer (Schoenfeld et al., 2017),
        12–20 is a common recommendation for trained lifters (Baz-Valle et al., 2022), and gains keep rising with more sets but with
        diminishing returns (Pelland et al., 2025). The studies don't give different numbers per muscle, so adjust any you like.
      </p>
      <table className="sets targets-table">
        <tbody>
          {targets.map((t, i) => (
            <tr key={t.muscle_id}>
              <td>{muscles.find((m) => m.id === t.muscle_id)?.name ?? t.muscle_id}</td>
              {(["min_sets", "max_sets"] as const).map((k) => (
                <td key={k}>
                  <input
                    className="num"
                    inputMode="numeric"
                    value={t[k]}
                    onChange={(e) => setTargets(targets.map((x, j) => (j === i ? { ...x, [k]: Number(e.target.value) || 0 } : x)))}
                    onBlur={() => save(targets[i]).catch((e) => tell(String(e)))}
                  />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
