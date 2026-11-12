import { plateaus } from "../charts/stall";
import { useEffect, useMemo, useState } from "react";
import { api, type CardioSession, type ExerciseInfo, type ExerciseStats, type Muscle, type Phase, type WeighInRow } from "../api";
import { navigate, usePrefs } from "../App";
import { RangePicker, inRange, useSavedRange } from "../charts/RangePicker";
import { CardioActivity } from "./CardioPage";
import { TimeSeriesChart, type Band, type Marker, type Point } from "../charts/TimeSeriesChart";
import { ExerciseEditor } from "../components/ExerciseEditor";
import { ShareButton } from "../share/ShareDialog";
import { EQUIPMENT_LABELS, formatDate, formatVolume, formatWeight, parseLocal, toUnit } from "../format";
import { prText } from "./HistoryPage";
import { ProgressHeader } from "../components/ProgressHeader";
import { ExerciseComparisons, ProfileHint, useComparable, useNormProfile } from "../components/Standing";

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

export default function ExercisesPage({ selectedId }: { selectedId: number | null }) {
  return (
    <div className={selectedId ? "split has-detail" : "split"}>
      <section className="split-list">
        <ProgressHeader current="exercises" />
        <Library selectedId={selectedId} />
      </section>
      <section className="split-detail">
        {selectedId ? (
          <ExerciseDetail key={selectedId} id={selectedId} />
        ) : (
          <p className="empty">Pick an exercise to see its history, records and charts.</p>
        )}
      </section>
    </div>
  );
}

function Library({ selectedId }: { selectedId: number | null }) {
  const [all, setAll] = useState<ExerciseInfo[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [q, setQ] = useState("");
  const [usedOnly, setUsedOnly] = useState(true);
  const [equipment, setEquipment] = useState("");
  const [muscle, setMuscle] = useState("");
  const [creating, setCreating] = useState(false);
  const load = () => api.listExercises(false).then(setAll);
  useEffect(() => {
    load();
    api.muscles().then((m) => setMuscles(m.muscles));
  }, []);
  const shown = useMemo(() => {
    const nq = norm(q);
    return all
      .filter((e) => !usedOnly || e.sessions > 0)
      .filter((e) => !equipment || e.equipment === equipment)
      .filter((e) => !muscle || e.primary.includes(muscle) || e.secondary.includes(muscle))
      .filter((e) => !nq || norm(e.name).includes(nq) || e.aliases.some((a) => norm(a).includes(nq)))
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [all, q, usedOnly, equipment, muscle]);
  const name = (id: string) => muscles.find((m) => m.id === id)?.name ?? id;

  return (
    <>
      <input className="search" type="search" placeholder="Search exercises…" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="row small">
        <label className="check">
          <input type="checkbox" checked={usedOnly} onChange={(e) => setUsedOnly(e.target.checked)} /> Done before
        </label>
        <select className="select inline" value={equipment} onChange={(e) => setEquipment(e.target.value)}>
          <option value="">Any equipment</option>
          {Object.entries(EQUIPMENT_LABELS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
        <select className="select inline" value={muscle} onChange={(e) => setMuscle(e.target.value)}>
          <option value="">Any muscle</option>
          {muscles.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
        <button className="link-btn" onClick={() => setCreating(true)}>
          + New
        </button>
      </div>
      <div className="muted small" style={{ margin: "6px 0" }}>
        {shown.length} exercises
      </div>
      <div className="pick-list">
        {shown.slice(0, 300).map((e) => (
          <button
            key={e.id}
            className={e.id === selectedId ? "pick-row selected" : "pick-row"}
            onClick={() => navigate(`/exercises/${e.id}`)}
          >
            <span>
              {e.name}
              <span className="muted small">
                {" "}
                · {[EQUIPMENT_LABELS[e.equipment ?? ""] ?? e.equipment, ...e.primary.map(name)].filter(Boolean).join(" · ")}
              </span>
            </span>
            <span className="muted small">{e.sessions > 0 ? `${e.sessions}×` : ""}</span>
          </button>
        ))}
      </div>
      {creating && <ExerciseEditor onClose={() => setCreating(false)} onSaved={(id) => load().then(() => navigate(`/exercises/${id}`))} />}
    </>
  );
}

type Metric = "e1rm" | "weight" | "volume" | "reps";

function ExerciseDetail({ id }: { id: number }) {
  const { unit } = usePrefs();
  const [info, setInfo] = useState<ExerciseInfo | null>(null);
  const [stats, setStats] = useState<ExerciseStats | null>(null);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [phases, setPhases] = useState<Phase[]>([]);
  const [weights, setWeights] = useState<WeighInRow[]>([]);
  const [range, setRange] = useSavedRange("exercise");
  const [metric, setMetric] = useState<Metric>("e1rm");
  const [editing, setEditing] = useState(false);
  const [sessionsShown, setSessionsShown] = useState(10);
  const [tab, setTab] = useState<"about" | "history" | "charts" | "records">("history");

  const load = () => {
    api.exerciseInfo(id).then(setInfo);
    api.exerciseStats(id).then((s) => {
      setStats(s);
      if (!s.sessions.some((x) => x.best_e1rm)) setMetric("reps");
    });
  };
  useEffect(() => {
    load();
    api.muscles().then((m) => setMuscles(m.muscles));
    api.listPhases().then(setPhases);
    api.weighIns(true).then(setWeights);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const latestKg = weights.length ? weights[weights.length - 1].weight_kg : null;
  const { who, loaded } = useNormProfile(latestKg);
  const comparable = useComparable(info?.slug ? [info.slug] : []);

  if (!info || !stats) return <p className="muted">Loading…</p>;
  const firstE1rm = stats.sessions.find((x) => x.best_e1rm)?.best_e1rm;
  const sessionReps = (s: (typeof stats.sessions)[number]) => Math.max(0, ...s.sets.map((x) => x.reps ?? 0));
  const firstReps = stats.sessions.map(sessionReps).find((r) => r > 0);
  const ends = (first: number | undefined, best: number | undefined) => (best ? (first && first !== best ? [first, best] : [best]) : []);
  const name = (m: string) => muscles.find((x) => x.id === m)?.name ?? m;
  const sessions = stats.sessions.filter((s) => inRange(s.at, range));
  const bodyweightKind = info.kind === "bodyweight_reps" || info.kind === "assisted_bodyweight";
  const w = (kg: number) => toUnit(kg, unit);

  const series: Point[] = sessions
    .map((s) => {
      const v =
        metric === "e1rm"
          ? s.best_e1rm && w(s.best_e1rm)
          : metric === "weight"
            ? s.max_weight && w(s.max_weight)
            : metric === "volume"
              ? s.volume_kg
                ? w(s.volume_kg)
                : null
              : Math.max(0, ...s.sets.map((x) => x.reps ?? 0)) || null;
      return v == null ? null : { t: parseLocal(s.at), v };
    })
    .filter((p): p is Point => p !== null);
  const prKind = { e1rm: "e1rm", weight: "weight", volume: "volume", reps: "reps" }[metric];
  const markers: Marker[] = stats.prs
    .filter((p) => p.kind === prKind && inRange(p.achieved_at, range))
    .map((p) => ({ t: parseLocal(p.achieved_at), v: metric === "reps" ? p.value : w(p.value), label: prText(p, unit) }));
  const bands: Band[] = phases.map((p) => ({
    start: parseLocal(p.start_date),
    end: p.end_date ? parseLocal(p.end_date) : null,
    label: p.name,
    lane: p.lane,
  }));
  // Plateaus come from the whole history (a range could cut one short), then are clipped to the range.
  const shownPlateaus =
    metric === "e1rm"
      ? plateaus(stats.sessions.filter((x) => x.best_e1rm).map((x) => ({ t: parseLocal(x.at), v: w(x.best_e1rm!) }))).filter(
          (p) => series.length > 0 && p.to >= series[0].t && p.from <= series[series.length - 1].t,
        )
      : [];
  const bwPoints: Point[] = weights.filter((x) => inRange(x.at, range)).map((x) => ({ t: parseLocal(x.at), v: w(x.weight_kg) }));
  const metrics: [Metric, string][] = [
    ["e1rm", "Est. 1RM"],
    ["weight", "Heaviest"],
    ["volume", "Volume"],
    ["reps", "Most reps"],
  ];

  return (
    <article className="detail">
      <button className="back-link" onClick={() => navigate("/exercises")}>
        ‹ Progress
      </button>
      <div className="title-row">
        <h2 className="detail-title">{info.name}</h2>
        <div className="row" style={{ marginTop: 0 }}>
          <ShareButton kind="exercise" exerciseId={id} />
          <button className="button small" onClick={() => setEditing(true)}>
            Edit
          </button>
        </div>
      </div>
      <div className="segmented small exercise-tabs">
        {(["history", "charts", "records", "about"] as const).map((t) => (
          <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
            {{ about: "About", history: "History", charts: "Charts", records: "Records" }[t]}
          </button>
        ))}
      </div>
      {tab === "about" && (
        <>
          <div className="chips">
            {info.equipment && <span className="chip">{EQUIPMENT_LABELS[info.equipment] ?? info.equipment}</span>}
            {info.load_mode === "per_hand" && <span className="chip">weights per hand</span>}
            {info.primary.map((m) => (
              <span key={m} className="chip strong">
                {name(m)}
              </span>
            ))}
            {info.secondary.map((m) => (
              <span key={m} className="chip">
                {name(m)} ½
              </span>
            ))}
            {info.source === "free-exercise-db" && !info.user_modified && <span className="chip warn">muscles auto-mapped</span>}
          </div>
          {info.setup_note && <p className="setup-note small">⚙ {info.setup_note}</p>}
          {info.aliases.length > 0 && <p className="muted small">Also known as: {info.aliases.join(", ")}</p>}

          <p className="muted small">
            {info.sessions} sessions logged. Use Edit to change muscles, equipment, sticky note or whether weights are per hand.
          </p>
        </>
      )}

      {stats.sessions.length === 0 ? (
        <p className="empty">You haven't done this exercise yet.</p>
      ) : (
        <>
          {tab === "records" && (
            <div className="record-grid">
              {stats.best_e1rm && (
                <Record
                  label="Best est. 1RM"
                  value={`${formatWeight(stats.best_e1rm.value, unit)} ${unit}`}
                  sub={`${formatWeight(stats.best_e1rm.weight_kg, unit)} × ${stats.best_e1rm.reps} · ${formatDate(stats.best_e1rm.at)}`}
                />
              )}
              {stats.heaviest && (
                <Record
                  label="Heaviest"
                  value={`${formatWeight(stats.heaviest.value, unit)} ${unit}`}
                  sub={`× ${stats.heaviest.reps} · ${formatDate(stats.heaviest.at)}`}
                />
              )}
              {stats.best_volume && (
                <Record
                  label="Best session volume"
                  value={formatVolume(stats.best_volume.value, unit)}
                  sub={formatDate(stats.best_volume.at)}
                />
              )}
              {stats.most_reps && (
                <Record
                  label="Most reps"
                  value={String(stats.most_reps.value)}
                  sub={`${stats.most_reps.weight_kg ? `at ${formatWeight(stats.most_reps.weight_kg, unit)} ${unit} · ` : ""}${formatDate(stats.most_reps.at)}`}
                />
              )}
              <Record label="Sessions" value={String(stats.sessions.length)} sub={`total ${formatVolume(stats.total_volume_kg, unit)}`} />
            </div>
          )}
          {tab === "records" && comparable.has(info.slug) && loaded && (
            <section className="card">
              <h2 className="section-title">How you compare</h2>
              {who && latestKg ? (
                <ExerciseComparisons
                  slug={info.slug}
                  who={who}
                  weight={ends(firstE1rm ?? undefined, stats.best_e1rm?.value)}
                  reps={ends(firstReps, stats.most_reps?.value)}
                  note={`Your best ever (first: hollow dot), at your latest bodyweight (${Math.round(toUnit(latestKg, unit))} ${unit}). The reference numbers are actual maxes; yours is estimated from your sets.`}
                  perHand={info.load_mode === "per_hand"}
                  dumbbell={info.equipment === "dumbbell"}
                />
              ) : who ? (
                <p className="muted small">Log your bodyweight to compare.</p>
              ) : (
                <ProfileHint />
              )}
            </section>
          )}
          {tab === "records" && (
            <div className="record-grid">
              <Record label="Lifetime reps" value={stats.sessions.reduce((n, x) => n + x.total_reps, 0).toLocaleString()} />
              <Record label="Lifetime volume" value={formatVolume(stats.total_volume_kg, unit)} />
            </div>
          )}
          {tab === "charts" && info.kind === "distance_duration" && <CardioCharts exerciseId={id} />}
          {tab === "charts" && info.kind !== "distance_duration" && (
            <section className="card">
              <div className="chips-row" style={{ marginBottom: 8 }}>
                {metrics
                  .filter(([m]) => m !== "e1rm" || stats.best_e1rm)
                  .map(([m, l]) => (
                    <button key={m} className={metric === m ? "chip-btn active" : "chip-btn"} onClick={() => setMetric(m)}>
                      {l}
                    </button>
                  ))}
              </div>
              <RangePicker value={range} onChange={setRange} phases={phases.filter((p) => p.lane === "program")} />
              <TimeSeriesChart
                series={[{ id: metric, label: metrics.find((m) => m[0] === metric)![1], points: series }]}
                unit={metric === "reps" ? "reps" : unit}
                markers={markers}
                bands={bands}
                plateaus={shownPlateaus}
                zero={metric === "volume"}
              />
              {shownPlateaus.length > 0 && (
                <p className="muted small">
                  Dashed lines are plateaus: a level you stayed at (within 5% under, less than 1% over) for four more sessions and four weeks.
                </p>
              )}
              {bodyweightKind && bwPoints.length > 0 && (
                <>
                  <div className="section-label">Your bodyweight over the same period</div>
                  <TimeSeriesChart series={[{ id: "bw", label: "Bodyweight", points: bwPoints }]} unit={unit} height={140} />
                </>
              )}
            </section>
          )}
          {tab === "records" && stats.rep_maxes.length > 0 && (
            <section className="card">
              <div className="section-label">Rep maxes (heaviest for at least N reps)</div>
              <div className="record-grid compact">
                {stats.rep_maxes.map((r) => (
                  <Record
                    key={r.reps}
                    label={`${r.reps}${r.reps === 12 ? "+" : ""} RM`}
                    value={`${formatWeight(r.weight_kg, unit)}`}
                    sub={formatDate(r.at)}
                  />
                ))}
              </div>
              <details>
                <summary className="small muted">Most reps at each weight</summary>
                <table className="sets small">
                  <tbody>
                    {stats.reps_at_weight.map((r) => (
                      <tr key={r.weight_kg}>
                        <td>
                          {formatWeight(r.weight_kg, unit)} {unit}
                        </td>
                        <td>{r.reps} reps</td>
                        <td className="muted right">{formatDate(r.at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </details>
            </section>
          )}

          {tab === "records" && stats.prs.length > 0 && (
            <details className="card" open>
              <summary className="section-label">PR history ({stats.prs.length})</summary>
              {[...stats.prs].reverse().map((p, i) => (
                <div key={i} className="small pr-row">
                  <span className="muted">{formatDate(p.achieved_at)}</span> <a href={`#/history/${p.workout_id}`}>{prText(p, unit)}</a>
                </div>
              ))}
            </details>
          )}

          {tab === "history" && (
            <section>
              {[...stats.sessions]
                .reverse()
                .slice(0, sessionsShown)
                .map((s) => (
                  <a key={s.workout_id + s.at} className="card session-card" href={`#/history/${s.workout_id}`}>
                    <div className="card-head">
                      <span>
                        <strong>{formatDate(s.at)}</strong> <span className="muted small">{s.workout_name}</span>
                      </span>
                      {s.prs.length > 0 && <span className="pr-badge">🏆 {s.prs.join(", ")}</span>}
                    </div>
                    <div className="small">
                      {s.sets
                        .map(
                          (x) =>
                            (x.reps != null ? (x.weight_kg ? `${formatWeight(x.weight_kg, unit)}×${x.reps}` : `${x.reps}`) : "•") +
                            (x.set_type !== "normal" ? x.set_type[0].toUpperCase() : ""),
                        )
                        .join("  ")}
                    </div>
                    {s.notes && <div className="muted small">“{s.notes}”</div>}
                  </a>
                ))}
              {stats.sessions.length > sessionsShown && (
                <button className="button wide-btn" onClick={() => setSessionsShown(sessionsShown + 20)}>
                  Show more
                </button>
              )}
            </section>
          )}
        </>
      )}
      {editing && <ExerciseEditor id={id} onClose={() => setEditing(false)} onSaved={load} />}
    </article>
  );
}

function Record({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="record">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {sub && <div className="muted small">{sub}</div>}
    </div>
  );
}

/** Charts for a cardio exercise: pace and totals rather than weights and reps. */
function CardioCharts({ exerciseId }: { exerciseId: number }) {
  const [sessions, setSessions] = useState<CardioSession[] | null>(null);
  useEffect(() => {
    api.cardioSessions().then((all) => setSessions(all.filter((x) => x.exercise_id === exerciseId)));
  }, [exerciseId]);
  if (!sessions) return null;
  return sessions.length ? <CardioActivity sessions={sessions} /> : <p className="empty">No sessions with a distance or time yet.</p>;
}
