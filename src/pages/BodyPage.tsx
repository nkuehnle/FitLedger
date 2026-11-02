import { ask } from "../components/Modal";
import { useEffect, useMemo, useState } from "react";
import {
  api,
  type CsvMapping,
  type DietDay,
  type EnergyPoint,
  type InBodyCsvPreview,
  type InBodyField,
  type InBodyScan,
  type BodyPart,
  type Goal,
  type MeasurementRow,
  type Phase,
  type WeighInRow,
} from "../api";
import { usePrefs } from "../App";
import { RangePicker, inRange, isoDate, presetRange, today, type Range } from "../charts/RangePicker";
import { TimeSeriesChart, type Band, type Point } from "../charts/TimeSeriesChart";
import { useChartTheme } from "../charts/theme";
import { Modal } from "../components/Modal";
import { ShareButton } from "../share/ShareDialog";
import { formatDate, formatTime, parseLocal, toUnit } from "../format";

const KG_PER_LB = 0.45359237;
type Tab = "weight" | "inbody" | "diet" | "parts";

export default function BodyPage({ tab, part }: { tab: Tab; part?: string }) {
  return (
    <div className="page">
      <h1 className="page-title">Measure</h1>
      <GoalToggle />
      <div className="segmented tabs-scroll">
        {(["weight", "parts", "inbody", "diet"] as Tab[]).map((t) => (
          <a key={t} href={`#/body/${t}`} className={tab === t ? "seg-link active" : "seg-link"}>
            {{ weight: "Weight", parts: "Body parts", inbody: "Body comp", diet: "Calories" }[t]}
          </a>
        ))}
      </div>
      {tab === "weight" && <BodyweightTab />}
      {tab === "parts" && <BodyPartsTab part={part} />}
      {tab === "inbody" && <InBodyTab />}
      {tab === "diet" && <DietTab />}
    </div>
  );
}

// ---- Goal -------------------------------------------------------------------------------------------

const GOAL_TEXT: Record<Goal, string> = { cut: "Cut", maintain: "Maintain", gain: "Gain" };

/** Cut / maintain / gain. Choosing one starts a nutrition phase from today (shaded on charts). */
function GoalToggle() {
  const [goal, setGoal] = useState<Goal | null | undefined>(undefined);
  useEffect(() => {
    api.currentGoal().then((g) => setGoal(g.goal));
  }, []);
  if (goal === undefined) return null;
  return (
    <div className="goal-row">
      <span className="small muted">Goal</span>
      <div className="segmented small">
        {(["cut", "maintain", "gain"] as Goal[]).map((g) => (
          <button
            key={g}
            className={goal === g ? "active" : ""}
            onClick={() => {
              if (goal === g) return;
              api.setGoal(g).then(() => {
                setGoal(g);
                window.dispatchEvent(new Event("goal-changed"));
              });
            }}
          >
            {GOAL_TEXT[g]}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Target vs actual weekly rate of weight change for the current goal. */
function GoalRate({ rows, unit }: { rows: WeighInRow[]; unit: "kg" | "lb" }) {
  const [info, setInfo] = useState<{ goal: Goal | null; rate_pct: [number, number] | null } | null>(null);
  useEffect(() => {
    const load = () => api.currentGoal().then(setInfo);
    load();
    window.addEventListener("goal-changed", load);
    return () => window.removeEventListener("goal-changed", load);
  }, []);
  if (!info?.goal || !info.rate_pct) return null;
  // Actual: change over the last 4 weeks of weigh-ins, as % of bodyweight per week.
  const recent = rows.filter((r) => r.source !== "inbody" && parseLocal(r.at).getTime() > Date.now() - 28 * 86400000);
  const avg = (rs: WeighInRow[]) => rs.reduce((n, r) => n + r.weight_kg, 0) / rs.length;
  const firstWeek = recent.filter((r) => parseLocal(r.at).getTime() < Date.now() - 21 * 86400000);
  const lastWeek = recent.filter((r) => parseLocal(r.at).getTime() > Date.now() - 7 * 86400000);
  const actual = firstWeek.length && lastWeek.length ? ((avg(lastWeek) - avg(firstWeek)) / avg(firstWeek) / 3) * 100 : null;
  const [lo, hi] = info.rate_pct;
  const bw = rows.length ? rows[rows.length - 1].weight_kg : null;
  const amount = (pct: number) => (bw ? `${Math.abs(toUnit((bw * pct) / 100, unit)).toFixed(1)}` : "");
  const sign = (v: number) => (v > 0 ? "+" : v < 0 ? "−" : "±");
  const aim =
    info.goal === "maintain"
      ? `stay within ±${hi}% a week (±${amount(hi)} ${unit})`
      : `${info.goal === "cut" ? "lose" : "gain"} ${Math.abs(info.goal === "cut" ? hi : lo)}–${Math.abs(info.goal === "cut" ? lo : hi)}% of bodyweight a week (${amount(Math.abs(lo))}–${amount(Math.abs(hi))} ${unit})`;
  const onTrack = actual != null && actual >= lo && actual <= hi;
  return (
    <p className="small goal-rate">
      <strong>{GOAL_TEXT[info.goal]}:</strong> aim to {aim}.{" "}
      {actual == null ? (
        <span className="muted">Weigh in over a few weeks to see your rate.</span>
      ) : (
        <span>
          Last 3 weeks: {sign(actual)}
          {Math.abs(actual).toFixed(2)}% a week{onTrack ? " (on target)." : "."}
        </span>
      )}
    </p>
  );
}

// ---- Body parts --------------------------------------------------------------------------------------

function BodyPartsTab({ part }: { part?: string }) {
  const { unit } = usePrefs();
  const inches = unit === "lb";
  const [parts, setParts] = useState<BodyPart[]>([]);
  const [rows, setRows] = useState<MeasurementRow[]>([]);
  const [entry, setEntry] = useState("");
  const [editing, setEditing] = useState<MeasurementRow | null>(null);
  const load = () => api.measurements().then(setRows);
  useEffect(() => {
    api.bodyParts().then(setParts);
    load();
  }, []);
  const show = (p: BodyPart, v: number) =>
    p.unit === "pct" ? `${v.toFixed(1)}%` : inches ? `${(v / 2.54).toFixed(1)} in` : `${v.toFixed(1)} cm`;
  const sel = parts.find((p) => p.id === part);

  if (!sel)
    return (
      <section className="card">
        {parts.map((p) => {
          const mine = rows.filter((r) => r.part === p.id);
          const last = mine[mine.length - 1];
          return (
            <a key={p.id} className="start-row" href={`#/body/parts/${p.id}`}>
              <span>{p.label}</span>
              <span className="muted">{last ? `${show(p, last.value)} · ${formatDate(last.at)}` : "—"} ›</span>
            </a>
          );
        })}
      </section>
    );

  const mine = rows.filter((r) => r.part === sel.id);
  const toDisplay = (v: number) => (sel.unit === "cm" && inches ? v / 2.54 : v);
  const unitLabel = sel.unit === "pct" ? "%" : inches ? "in" : "cm";
  async function add() {
    const v = Number(entry);
    if (!v) return;
    await api.addMeasurement(nowLocal(), sel!.id, sel!.unit === "cm" && inches ? v * 2.54 : v);
    setEntry("");
    load();
  }
  return (
    <>
      <a className="back-link" href="#/body/parts">
        ‹ Body parts
      </a>
      <section className="card">
        <h2 className="section-title">{sel.label}</h2>
        <div className="row" style={{ marginTop: 0 }}>
          <input
            className="num big-input"
            inputMode="decimal"
            placeholder={`New (${unitLabel})`}
            value={entry}
            onChange={(e) => setEntry(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <button className="button primary" onClick={add} disabled={!Number(entry)}>
            + Add
          </button>
        </div>
        {mine.length > 1 && (
          <TimeSeriesChart
            series={[{ id: sel.id, label: sel.label, points: mine.map((r) => ({ t: parseLocal(r.at), v: toDisplay(r.value) })) }]}
            unit={unitLabel}
            height={200}
          />
        )}
      </section>
      <section className="card">
        {mine.length === 0 && <p className="muted small">No measurements yet.</p>}
        {[...mine].reverse().map((r) => (
          <button key={r.id} className="start-row" onClick={() => setEditing(r)}>
            <span>
              {formatDate(r.at)} <span className="muted small">{formatTime(r.at)}</span>
            </span>
            <strong>{show(sel, r.value)}</strong>
          </button>
        ))}
      </section>
      {editing && (
        <EntryEditor
          title={`Edit ${sel.label.toLowerCase()}`}
          unitLabel={unitLabel}
          value={toDisplay(editing.value)}
          at={editing.at}
          onClose={() => setEditing(null)}
          onSave={(v, at) => api.updateMeasurement(editing.id, at, sel.unit === "cm" && inches ? v * 2.54 : v).then(load)}
          onDelete={() => api.deleteMeasurement(editing.id).then(load)}
        />
      )}
    </>
  );
}

function bandsOf(phases: Phase[]): Band[] {
  return phases.map((p) => ({
    start: parseLocal(p.start_date),
    end: p.end_date ? parseLocal(p.end_date) : null,
    label: p.name,
    lane: p.lane,
  }));
}

function nowLocal(): string {
  const d = new Date();
  return `${isoDate(d)} ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:00`;
}

// ---- Bodyweight ---------------------------------------------------------------------------------------

function BodyweightTab() {
  const { unit } = usePrefs();
  const [rows, setRows] = useState<WeighInRow[]>([]);
  const [phases, setPhases] = useState<Phase[]>([]);
  const [range, setRange] = useState<Range>(presetRange("1y"));
  const [showInbody, setShowInbody] = useState(true);
  const [entry, setEntry] = useState("");
  const [hide, setHide] = useState(false);
  const load = () => api.weighIns(true).then(setRows);
  useEffect(() => {
    load();
    api.listPhases().then(setPhases);
  }, []);

  const inR = rows.filter((r) => inRange(r.at, range));
  const regular = inR.filter((r) => r.source !== "inbody");
  const inbody = inR.filter((r) => r.source === "inbody");
  const pts = (rs: WeighInRow[]): Point[] => rs.map((r) => ({ t: parseLocal(r.at), v: toUnit(r.weight_kg, unit) }));
  const series = [{ id: "bw", label: "Weigh-ins", points: pts(regular) }];
  if (showInbody && inbody.length) series.push({ id: "inbody", label: "InBody scans", points: pts(inbody), style: "points" } as never);

  // Weekly average (last 7 days) and change over the range.
  const last7 = regular.filter((r) => parseLocal(r.at).getTime() > Date.now() - 7 * 86400000);
  const avg7 = last7.length ? last7.reduce((s, r) => s + r.weight_kg, 0) / last7.length : null;
  const first = regular[0];
  const last = regular[regular.length - 1];
  const change = first && last ? last.weight_kg - first.weight_kg : null;
  const weeks = first && last ? Math.max(1, (parseLocal(last.at).getTime() - parseLocal(first.at).getTime()) / (7 * 86400000)) : 1;

  async function add() {
    const v = Number(entry);
    if (!v) return;
    await api.addWeighIn(nowLocal(), unit === "lb" ? v * KG_PER_LB : v);
    setEntry("");
    load();
  }

  return (
    <>
      <section className="card">
        <div className="row" style={{ marginTop: 0 }}>
          <input
            className="num big-input"
            inputMode="decimal"
            placeholder={`Today's weight (${unit})`}
            value={entry}
            onChange={(e) => setEntry(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && add()}
          />
          <button className="button primary" onClick={add} disabled={!Number(entry)}>
            Log
          </button>
        </div>
      </section>
      <section className="card">
        <div className="title-row">
          <h2 className="section-title">Bodyweight</h2>
          <ShareButton kind="bodyweight" range={range} />
        </div>
        <div className="stats-row">
          {last && <Stat label="Latest" value={`${toUnit(last.weight_kg, unit).toFixed(1)} ${unit}`} />}
          {avg7 != null && <Stat label="7-day average" value={`${toUnit(avg7, unit).toFixed(1)} ${unit}`} />}
          {change != null && (
            <Stat
              label={`Change (${range.label.toLowerCase()})`}
              value={`${change >= 0 ? "+" : "−"}${Math.abs(toUnit(change, unit)).toFixed(1)} ${unit}`}
            />
          )}
          {change != null && (
            <Stat label="Per week" value={`${change >= 0 ? "+" : "−"}${Math.abs(toUnit(change / weeks, unit)).toFixed(2)} ${unit}`} />
          )}
        </div>
        <GoalRate rows={rows} unit={unit} />
        <RangePicker value={range} onChange={setRange} phases={phases} />
        <div className="row small" style={{ marginTop: 0 }}>
          <label className="check">
            <input type="checkbox" checked={showInbody} onChange={(e) => setShowInbody(e.target.checked)} /> Show InBody weights
          </label>
          <label className="check">
            <input type="checkbox" checked={hide} onChange={(e) => setHide(e.target.checked)} /> Hide numbers
          </label>
        </div>
        <TimeSeriesChart
          series={series}
          unit={unit}
          bands={bandsOf(phases)}
          height={280}
          hideNumbers={hide}
          yFormat={(v) => v.toFixed(0)}
        />
      </section>
      <WeighInList rows={rows} unit={unit} onChanged={load} />
    </>
  );
}

/** Every weigh-in, newest first; tap one to fix it. */
function WeighInList({ rows, unit, onChanged }: { rows: WeighInRow[]; unit: "kg" | "lb"; onChanged: () => void }) {
  const [shown, setShown] = useState(30);
  const [editing, setEditing] = useState<WeighInRow | null>(null);
  const list = [...rows].reverse();
  return (
    <section className="card">
      <div className="section-label">Weigh-ins ({rows.length}) · tap one to edit</div>
      {list.slice(0, shown).map((r) => (
        <button
          key={`${r.source}${r.id}`}
          className="start-row"
          disabled={r.source === "inbody"}
          title={r.source === "inbody" ? "From an InBody scan: edit it under Body comp" : "Edit"}
          onClick={() => setEditing(r)}
        >
          <span>
            {formatDate(r.at)} <span className="muted small">{formatTime(r.at)}</span>
            {r.source === "inbody" && <span className="chip">InBody</span>}
          </span>
          <strong>
            {toUnit(r.weight_kg, unit).toFixed(1)} {unit}
          </strong>
        </button>
      ))}
      {list.length > shown && (
        <button className="button wide-btn" onClick={() => setShown(shown + 60)}>
          Show more
        </button>
      )}
      {editing && (
        <EntryEditor
          title="Edit weigh-in"
          unitLabel={unit}
          value={toUnit(editing.weight_kg, unit)}
          at={editing.at}
          onClose={() => setEditing(null)}
          onSave={(v, at) => api.updateWeighIn(editing.id, at, unit === "lb" ? v * KG_PER_LB : v).then(onChanged)}
          onDelete={() => api.deleteWeighIn(editing.id).then(onChanged)}
        />
      )}
    </section>
  );
}

/** Fix the value and date/time of a logged entry, or delete it. */
function EntryEditor({
  title,
  unitLabel,
  value,
  at,
  onClose,
  onSave,
  onDelete,
}: {
  title: string;
  unitLabel: string;
  value: number;
  at: string;
  onClose: () => void;
  onSave: (value: number, at: string) => Promise<unknown>;
  onDelete: () => Promise<unknown>;
}) {
  const [v, setV] = useState(String(Math.round(value * 10) / 10));
  const [when, setWhen] = useState(at.slice(0, 16).replace(" ", "T"));
  const [error, setError] = useState<string | null>(null);
  async function save() {
    const n = Number(v.replace(",", "."));
    if (!(n > 0)) return setError("Enter a number above 0.");
    try {
      await onSave(n, when.replace("T", " ") + ":00");
      onClose();
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <Modal title={title} onClose={onClose}>
      <div className="form">
        <label>
          Value ({unitLabel})
          <input
            className="big-input"
            inputMode="decimal"
            value={v}
            autoFocus
            onChange={(e) => setV(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
          />
        </label>
        <label>
          Date & time
          <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
        </label>
        {error && <p className="error small">{error}</p>}
        <div className="row">
          <button className="button primary" onClick={save}>
            Save
          </button>
          <button
            className="button danger"
            onClick={() =>
              ask("Delete this entry?").then((ok) => {
                if (ok) onDelete().then(onClose);
              })
            }
          >
            Delete
          </button>
          <button className="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
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

// ---- InBody -------------------------------------------------------------------------------------------

function InBodyTab() {
  const { unit } = usePrefs();
  const [fields, setFields] = useState<InBodyField[]>([]);
  const [scans, setScans] = useState<InBodyScan[]>([]);
  const [phases, setPhases] = useState<Phase[]>([]);
  const [editing, setEditing] = useState<InBodyScan | null>(null);
  const [csv, setCsv] = useState<{ text: string; preview: InBodyCsvPreview } | null>(null);
  const [metric, setMetric] = useState("smm_kg");
  const [msg, setMsg] = useState<string | null>(null);
  const load = () => api.inbodyScans().then(setScans);
  useEffect(() => {
    load();
    api.inbodyFields().then(setFields);
    api.listPhases().then(setPhases);
  }, []);

  const conv = (f: InBodyField | undefined, v: number) => (f?.unit === "kg" ? toUnit(v, unit) : v);
  const fieldOf = (c: string) => fields.find((f) => f.column === c);
  const unitLabel = (f?: InBodyField) =>
    !f ? "" : f.unit === "kg" ? unit : f.unit === "pct" ? "%" : f.unit === "l" ? "L" : f.unit === "kcal" ? "kcal" : "";
  const series = (c: string): Point[] =>
    scans.filter((s) => s.values[c] != null).map((s) => ({ t: parseLocal(s.measured_at_local), v: conv(fieldOf(c), s.values[c]) }));
  const latest = scans[scans.length - 1];
  const available = fields.filter(
    (f) => scans.some((s) => s.values[f.column] != null) && !f.column.startsWith("lean_") && !f.column.startsWith("fat_"),
  );

  async function onFile(file?: File) {
    if (!file) return;
    const text = await file.text();
    setCsv({ text, preview: await api.previewInbodyCsv(text) });
  }

  return (
    <>
      <section className="card">
        <div className="row" style={{ marginTop: 0 }}>
          <button
            className="button primary"
            onClick={() => setEditing({ id: null, measured_at_local: nowLocal(), values: {}, device: null, notes: null })}
          >
            + Add scan
          </button>
          <label className="button">
            Import CSV
            <input type="file" accept=".csv,text/csv" hidden onChange={(e) => onFile(e.target.files?.[0])} />
          </label>
        </div>
        {msg && <p className="success small">{msg}</p>}
      </section>
      {scans.length === 0 ? (
        <p className="empty">No scans yet. Add one from your InBody result sheet, or import a CSV from LookinBody.</p>
      ) : (
        <>
          <section className="card">
            <div className="title-row">
              <h2 className="section-title">Muscle vs fat</h2>
              <ShareButton kind="inbody" />
            </div>
            <TimeSeriesChart
              series={[
                { id: "smm", label: "Skeletal muscle mass", points: series("smm_kg") },
                { id: "bfm", label: "Body fat mass", points: series("body_fat_mass_kg") },
              ]}
              unit={unit}
              bands={bandsOf(phases)}
              height={240}
            />
          </section>
          <section className="card">
            <h2 className="section-title">Any metric</h2>
            <select className="select" value={metric} onChange={(e) => setMetric(e.target.value)}>
              {available.map((f) => (
                <option key={f.column} value={f.column}>
                  {f.label}
                </option>
              ))}
            </select>
            <TimeSeriesChart
              series={[{ id: metric, label: fieldOf(metric)?.label ?? metric, points: series(metric) }]}
              unit={unitLabel(fieldOf(metric))}
              bands={bandsOf(phases)}
              height={220}
            />
          </section>
          {latest && <Segmental scan={latest} unit={unit} />}
          <section className="card">
            <div className="section-label">Scans</div>
            {[...scans].reverse().map((s) => (
              <div key={s.id} className="start-row" onClick={() => setEditing(s)}>
                <span>
                  {formatDate(s.measured_at_local)}
                  <span className="muted small">
                    {" "}
                    · {s.values.weight_kg != null && `${toUnit(s.values.weight_kg, unit).toFixed(1)} ${unit}`}
                    {s.values.pbf != null && ` · ${s.values.pbf}% fat`}
                    {s.values.smm_kg != null && ` · SMM ${toUnit(s.values.smm_kg, unit).toFixed(1)}`}
                  </span>
                </span>
                <span className="muted">Edit ›</span>
              </div>
            ))}
          </section>
        </>
      )}
      {editing && (
        <ScanForm
          scan={editing}
          fields={fields}
          unit={unit}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
      {csv && (
        <CsvMapper
          text={csv.text}
          initial={csv.preview}
          fields={fields}
          onClose={() => setCsv(null)}
          onImported={(n) => {
            setCsv(null);
            setMsg(`Imported ${n} scans. The column mapping is remembered for files like this.`);
            load();
          }}
        />
      )}
    </>
  );
}

const SEGMENTS: [string, string][] = [
  ["right_arm", "Right arm"],
  ["left_arm", "Left arm"],
  ["trunk", "Trunk"],
  ["right_leg", "Right leg"],
  ["left_leg", "Left leg"],
];

/** Segmental lean mass on a simple figure (viewer's left is the body's right, as on the InBody sheet). */
function Segmental({ scan, unit }: { scan: InBodyScan; unit: "kg" | "lb" }) {
  const theme = useChartTheme();
  const [kind, setKind] = useState<"lean" | "fat">("lean");
  const val = (seg: string) => scan.values[`${kind}_${seg}_kg`];
  const pct = (seg: string) => scan.values[`${kind}_${seg}_pct`];
  const has = SEGMENTS.some(([s]) => val(s) != null || pct(s) != null);
  if (!has) return null;
  // Color by % of ideal when present, else relative to the largest segment of the same type.
  const level = (seg: string) => {
    const p = pct(seg);
    if (p != null) return Math.max(0, Math.min(1, (p - 70) / 60));
    const limbs = seg === "trunk" ? [val("trunk")] : SEGMENTS.filter(([s]) => s !== "trunk").map(([s]) => val(s));
    const max = Math.max(...limbs.filter((x): x is number => x != null));
    return val(seg) != null ? val(seg)! / max : 0;
  };
  const steps = theme.sequential.slice(2);
  const color = (seg: string) => steps[Math.round(level(seg) * (steps.length - 1))];
  const label = (seg: string) =>
    [val(seg) != null ? `${toUnit(val(seg)!, unit).toFixed(1)} ${unit}` : null, pct(seg) != null ? `${pct(seg)}%` : null]
      .filter(Boolean)
      .join(" · ");
  const diff = (a: string, b: string) => (val(a) != null && val(b) != null ? toUnit(val(a)! - val(b)!, unit) : null);
  const armDiff = diff("right_arm", "left_arm");
  const legDiff = diff("right_leg", "left_leg");
  const shapes: Record<string, string> = {
    right_arm: "M40,60 L58,58 L56,150 L42,150 Z",
    left_arm: "M160,60 L142,58 L144,150 L158,150 Z",
    trunk: "M62,52 L138,52 L132,150 L68,150 Z",
    right_leg: "M70,154 L98,154 L94,280 L74,280 Z",
    left_leg: "M130,154 L102,154 L106,280 L126,280 Z",
  };
  return (
    <section className="card">
      <div className="title-row">
        <h2 className="section-title">
          Segmental {kind === "lean" ? "lean" : "fat"} mass · {formatDate(scan.measured_at_local)}
        </h2>
        <div className="segmented small">
          <button className={kind === "lean" ? "active" : ""} onClick={() => setKind("lean")}>
            Lean
          </button>
          <button className={kind === "fat" ? "active" : ""} onClick={() => setKind("fat")}>
            Fat
          </button>
        </div>
      </div>
      <div className="segmental">
        <svg viewBox="0 0 200 300" height={260}>
          <circle cx={100} cy={30} r={18} fill={theme.grid} />
          {SEGMENTS.map(([s]) => (
            <path
              key={s}
              d={shapes[s]}
              fill={val(s) != null || pct(s) != null ? color(s) : theme.grid}
              stroke={theme.surface}
              strokeWidth={2}
            />
          ))}
          <text x={20} y={296} fontSize={10} fill={theme.muted}>
            R
          </text>
          <text x={174} y={296} fontSize={10} fill={theme.muted}>
            L
          </text>
        </svg>
        <ul className="seg-list small">
          {SEGMENTS.map(([s, n]) => (
            <li key={s}>
              <span>{n}</span>
              <strong>{label(s) || "—"}</strong>
            </li>
          ))}
          {armDiff != null && (
            <li className="muted">
              Arms: right {armDiff >= 0 ? "+" : "−"}
              {Math.abs(armDiff).toFixed(2)} {unit} vs left
            </li>
          )}
          {legDiff != null && (
            <li className="muted">
              Legs: right {legDiff >= 0 ? "+" : "−"}
              {Math.abs(legDiff).toFixed(2)} {unit} vs left
            </li>
          )}
        </ul>
      </div>
    </section>
  );
}

function ScanForm({
  scan,
  fields,
  unit,
  onClose,
  onSaved,
}: {
  scan: InBodyScan;
  fields: InBodyField[];
  unit: "kg" | "lb";
  onClose: () => void;
  onSaved: () => void;
}) {
  const toDisplay = (f: InBodyField, v: number) => (f.unit === "kg" ? Math.round(toUnit(v, unit) * 100) / 100 : v);
  const [vals, setVals] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      Object.entries(scan.values).map(([k, v]) => [
        k,
        String(
          toDisplay(
            fields.find((f) => f.column === k)!,
            v,
          ),
        ),
      ]),
    ),
  );
  const [when, setWhen] = useState(scan.measured_at_local.slice(0, 16).replace(" ", "T"));
  const [device, setDevice] = useState(scan.device ?? "");
  const [notes, setNotes] = useState(scan.notes ?? "");
  const [error, setError] = useState<string | null>(null);
  const label = (f: InBodyField) =>
    `${f.label}${f.unit === "kg" ? ` (${unit})` : f.unit === "pct" ? " (%)" : f.unit === "l" ? " (L)" : f.unit === "kcal" ? " (kcal)" : ""}`;
  const groups: [string, (f: InBodyField) => boolean][] = [
    ["Body composition", (f) => !f.column.startsWith("lean_") && !f.column.startsWith("fat_")],
    ["Segmental lean", (f) => f.column.startsWith("lean_")],
    ["Segmental fat", (f) => f.column.startsWith("fat_")],
  ];

  async function save() {
    const values: Record<string, number> = {};
    for (const [k, v] of Object.entries(vals)) {
      if (v.trim() === "") continue;
      const n = Number(v.replace(",", "."));
      if (Number.isNaN(n)) return setError(`${k}: not a number`);
      const f = fields.find((x) => x.column === k)!;
      values[k] = f.unit === "kg" && unit === "lb" ? n * KG_PER_LB : n;
    }
    try {
      await api.saveInbodyScan({
        id: scan.id,
        measured_at_local: when.replace("T", " ") + ":00",
        values,
        device: device || null,
        notes: notes || null,
      });
      onSaved();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <Modal title={scan.id ? "Edit scan" : "New InBody scan"} onClose={onClose} wide>
      <div className="form">
        <div className="form-row">
          <label>
            Date & time
            <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
          </label>
          <label>
            Device
            <input value={device} placeholder="InBody 570" onChange={(e) => setDevice(e.target.value)} />
          </label>
        </div>
        <p className="muted small">Every field is optional; fill in what's on your result sheet.</p>
        {groups.map(([title, pred]) => (
          <details key={title} open={title === "Body composition"}>
            <summary className="section-label">{title}</summary>
            <div className="field-grid">
              {fields.filter(pred).map((f) => (
                <label key={f.column}>
                  {label(f)}
                  <input
                    inputMode="decimal"
                    value={vals[f.column] ?? ""}
                    onChange={(e) => setVals({ ...vals, [f.column]: e.target.value })}
                  />
                </label>
              ))}
            </div>
          </details>
        ))}
        <label>
          Notes
          <textarea rows={2} value={notes} onChange={(e) => setNotes(e.target.value)} />
        </label>
        {error && <p className="error">{error}</p>}
        <div className="row">
          <button className="button primary" onClick={save}>
            Save
          </button>
          {scan.id && (
            <button
              className="button danger"
              onClick={() =>
                ask("Delete this scan?").then((ok) => {
                  if (ok) api.deleteInbodyScan(scan.id!).then(onSaved);
                })
              }
            >
              Delete
            </button>
          )}
          <button className="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  );
}

function CsvMapper({
  text,
  initial,
  fields,
  onClose,
  onImported,
}: {
  text: string;
  initial: InBodyCsvPreview;
  fields: InBodyField[];
  onClose: () => void;
  onImported: (n: number) => void;
}) {
  const [mapping, setMapping] = useState<CsvMapping>(initial.mapping);
  const [preview, setPreview] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api
      .previewInbodyCsv(text, mapping)
      .then(setPreview)
      .catch((e) => setError(String(e)));
  }, [mapping, text]);
  const fieldFor = (h: string) => mapping.columns[h] ?? "";
  const setField = (h: string, f: string) => {
    const columns = { ...mapping.columns };
    if (f) columns[h] = f;
    else delete columns[h];
    setMapping({ ...mapping, columns });
  };
  return (
    <Modal title="Map InBody CSV columns" onClose={onClose} wide>
      {initial.remembered && <p className="success small">Using the mapping you saved for files with these columns.</p>}
      <div className="form-row">
        <label>
          Date column
          <select className="select" value={mapping.date_column} onChange={(e) => setMapping({ ...mapping, date_column: e.target.value })}>
            {preview.headers.map((h) => (
              <option key={h}>{h}</option>
            ))}
          </select>
        </label>
        <label>
          Masses in the file are in
          <select
            className="select"
            value={mapping.mass_unit}
            onChange={(e) => setMapping({ ...mapping, mass_unit: e.target.value as "kg" | "lb" })}
          >
            <option value="kg">kg</option>
            <option value="lb">lb</option>
          </select>
        </label>
      </div>
      <table className="sets small">
        <tbody>
          {preview.headers
            .filter((h) => h !== mapping.date_column)
            .map((h) => (
              <tr key={h}>
                <td>{h}</td>
                <td>
                  <select className="select inline" value={fieldFor(h)} onChange={(e) => setField(h, e.target.value)}>
                    <option value="">— ignore —</option>
                    {fields.map((f) => (
                      <option key={f.column} value={f.column}>
                        {f.label}
                      </option>
                    ))}
                  </select>
                </td>
              </tr>
            ))}
        </tbody>
      </table>
      <p className="small">
        {preview.scans.length} scans ready{preview.errors.length > 0 && <span className="warn"> · {preview.errors.length} problems</span>}
      </p>
      {preview.errors.length > 0 && (
        <details className="warnings">
          <summary className="small">Problems</summary>
          <ul>
            {preview.errors.slice(0, 50).map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
        </details>
      )}
      {error && <p className="error">{error}</p>}
      <div className="row">
        <button
          className="button primary"
          disabled={!preview.scans.length}
          onClick={() =>
            api
              .importInbodyCsv(text, mapping)
              .then(onImported)
              .catch((e) => setError(String(e)))
          }
        >
          Import {preview.scans.length} scans
        </button>
        <button className="button" onClick={onClose}>
          Cancel
        </button>
      </div>
    </Modal>
  );
}

// ---- Diet ----------------------------------------------------------------------------------------------

function DietTab() {
  const { unit } = usePrefs();
  const [days, setDays] = useState<DietDay[]>([]);
  const [energy, setEnergy] = useState<EnergyPoint[]>([]);
  const [window_, setWindow] = useState(14);
  const [range, setRange] = useState<Range>(presetRange("6m"));
  const [phases, setPhases] = useState<Phase[]>([]);
  const [date, setDate] = useState(today());
  const load = () => {
    api.dietDays().then(setDays);
    api.energyBalance(window_).then(setEnergy);
  };
  useEffect(() => {
    load();
    api.listPhases().then(setPhases);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [window_]);
  const day = days.find((d) => d.date === date);
  const [cal, setCal] = useState("");
  const [protein, setProtein] = useState("");
  useEffect(() => {
    setCal(day?.calories != null ? String(day.calories) : "");
    setProtein(day?.protein_g != null ? String(day.protein_g) : "");
  }, [date, day?.calories, day?.protein_g]);

  async function save() {
    await api.saveDietDay({
      date,
      calories: cal ? Number(cal) : null,
      protein_g: protein ? Number(protein) : null,
      notes: day?.notes ?? null,
    });
    load();
  }

  const e = energy.filter((p) => inRange(p.date, range));
  const latest = [...energy].reverse().find((p) => p.balance_kcal != null);
  const latestTdee = [...energy].reverse().find((p) => p.tdee_kcal != null);
  const d = (s: string) => parseLocal(s);
  const balance = e.filter((p) => p.balance_kcal != null).map((p) => ({ t: d(p.date), v: Math.round(p.balance_kcal!) }));
  const tdee = e.filter((p) => p.tdee_kcal != null).map((p) => ({ t: d(p.date), v: Math.round(p.tdee_kcal!) }));
  const intake = days.filter((x) => x.calories != null && inRange(x.date, range)).map((x) => ({ t: d(x.date), v: x.calories! }));
  const proteinPts = days.filter((x) => x.protein_g != null && inRange(x.date, range)).map((x) => ({ t: d(x.date), v: x.protein_g! }));
  const trend = e.map((p) => ({ t: d(p.date), v: toUnit(p.trend_kg, unit) }));
  const bands = useMemo(() => bandsOf(phases), [phases]);

  return (
    <>
      <section className="card">
        <p className="muted small">Optional. Log whatever you track; days don't need to match weigh-ins.</p>
        <div className="form-row">
          <label>
            Date
            <input type="date" value={date} onChange={(x) => setDate(x.target.value)} />
          </label>
          <label>
            Calories
            <input inputMode="numeric" value={cal} onChange={(x) => setCal(x.target.value)} placeholder="kcal" />
          </label>
          <label>
            Protein
            <input inputMode="numeric" value={protein} onChange={(x) => setProtein(x.target.value)} placeholder="g" />
          </label>
          <button className="button primary" style={{ alignSelf: "end" }} onClick={save}>
            Save
          </button>
        </div>
      </section>
      <section className="card">
        <h2 className="section-title">Energy balance</h2>
        <div className="stats-row">
          {latest?.balance_kcal != null && (
            <Stat
              label={latest.balance_kcal >= 0 ? "Estimated surplus" : "Estimated deficit"}
              value={`${Math.abs(Math.round(latest.balance_kcal))} kcal/day`}
            />
          )}
          {latestTdee?.tdee_kcal != null && <Stat label="Estimated TDEE" value={`${Math.round(latestTdee.tdee_kcal)} kcal`} />}
          {latest && <Stat label="Trend weight" value={`${toUnit(latest.trend_kg, unit).toFixed(1)} ${unit}`} />}
        </div>
        <p className="muted small">
          From your weight trend: each {unit === "lb" ? "lb" : "kg"} gained or lost ≈ {unit === "lb" ? "3,500" : "7,700"} kcal. TDEE needs
          calories logged on at least half the days in the window.
        </p>
        <div className="row small" style={{ marginTop: 0 }}>
          Window
          <div className="segmented small">
            {[7, 14, 28].map((w) => (
              <button key={w} className={window_ === w ? "active" : ""} onClick={() => setWindow(w)}>
                {w} days
              </button>
            ))}
          </div>
        </div>
        <RangePicker value={range} onChange={setRange} phases={phases} />
        <div className="section-label">Surplus (+) / deficit (−) from weight trend</div>
        <TimeSeriesChart
          series={[{ id: "bal", label: "Balance", points: balance }]}
          unit="kcal"
          bands={bands}
          height={220}
          diverging={{ aboveLabel: "Surplus", belowLabel: "Deficit" }}
          yStep={250}
          yFormat={(v) => (v > 0 ? "+" : v < 0 ? "−" : "") + Math.abs(Math.round(v)).toLocaleString()}
        />
        <div className="section-label">Trend weight</div>
        <TimeSeriesChart
          series={[{ id: "trend", label: "Trend", points: trend }]}
          unit={unit}
          bands={bands}
          height={160}
          showControls={false}
          smooth={{ on: false, span: 0.3 }}
        />
        {tdee.length > 0 && (
          <>
            <div className="section-label">Estimated TDEE and logged intake</div>
            <TimeSeriesChart
              series={[
                { id: "tdee", label: "TDEE", points: tdee },
                { id: "in", label: "Intake", points: intake, style: "points" },
              ]}
              unit="kcal"
              height={200}
            />
          </>
        )}
        {proteinPts.length > 0 && (
          <>
            <div className="section-label">Protein</div>
            <TimeSeriesChart series={[{ id: "p", label: "Protein", points: proteinPts }]} unit="g" height={160} />
          </>
        )}
      </section>
    </>
  );
}
