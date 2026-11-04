import * as d3 from "d3";
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
import { RangePicker, inRange, today, useSavedRange } from "../charts/RangePicker";
import { TimeSeriesChart, type Band, type Point } from "../charts/TimeSeriesChart";
import { useChartTheme } from "../charts/theme";
import { BodyFatTab } from "../components/BodyFatTab";
import { Modal } from "../components/Modal";
import { ShareButton } from "../share/ShareDialog";
import { formatDate, formatTime, nowLocal, parseLocal, toUnit } from "../format";
import { GOAL_TEXT, goalStatus, type GoalInfo } from "../goal";
import { fileToBase64 } from "../platform";

const KG_PER_LB = 0.45359237;
type Tab = "weight" | "inbody" | "diet" | "parts";

export default function BodyPage({ tab, part }: { tab: Tab; part?: string }) {
  return (
    <div className="page">
      <h1 className="page-title">Measure</h1>
      <GoalPanel />
      <div className="segmented tabs-scroll">
        {(["weight", "parts", "inbody", "diet"] as Tab[]).map((t) => (
          <a key={t} href={`#/body/${t}`} className={tab === t ? "seg-link active" : "seg-link"}>
            {{ weight: "Weight", parts: "Body parts", inbody: "Body comp", diet: "Calories" }[t]}
          </a>
        ))}
      </div>
      {tab === "weight" && <BodyweightTab />}
      {tab === "parts" && <BodyPartsTab part={part} />}
      {/* Body comp: body fat and muscle/fat mass (measured and estimated), then InBody scans. */}
      {tab === "inbody" && (
        <>
          <BodyFatTab />
          <InBodyTab />
        </>
      )}
      {tab === "diet" && <DietTab />}
    </div>
  );
}

// ---- Goal -------------------------------------------------------------------------------------------

type GoalEntry = Awaited<ReturnType<typeof api.goalHistory>>[number];
const signed = (v: number, digits = 1) => `${v > 0 ? "+" : v < 0 ? "−" : "±"}${Math.abs(v).toFixed(digits)}`;

/** Weekly target input: blank means "use the default range for the goal". Stored as kg/week. */
function rateText(kg: number | null, unit: "kg" | "lb") {
  return kg == null ? "" : String(Math.round(toUnit(kg, unit) * 100) / 100);
}
function parseRate(text: string, goal: Goal, unit: "kg" | "lb"): number | null {
  const v = Number(text);
  if (!text.trim() || !isFinite(v)) return null;
  const kg = unit === "lb" ? v * KG_PER_LB : v;
  // Typing "1" on a cut means losing 1 a week.
  return goal === "cut" ? -Math.abs(kg) : goal === "gain" ? Math.abs(kg) : kg;
}

/** Cut / maintain / gain over time. Switching starts a new nutrition phase today (shaded on charts);
 *  earlier goals can be backdated, and each goal can have its own weekly target. */
function GoalPanel() {
  const { unit } = usePrefs();
  const [info, setInfo] = useState<GoalInfo | null>(null);
  const [history, setHistory] = useState<GoalEntry[]>([]);
  const [rate, setRate] = useState("");
  const [add, setAdd] = useState<{ goal: Goal; start: string; rate: string }>({ goal: "cut", start: today(), rate: "" });
  const load = () => {
    api.currentGoal().then((g) => {
      setInfo(g);
      setRate(rateText(g.weekly_rate_kg, unit));
    });
    api.goalHistory().then(setHistory);
  };
  useEffect(load, [unit]);
  const changed = () => {
    load();
    window.dispatchEvent(new Event("goal-changed"));
  };
  if (!info) return null;
  const goal = info.goal;
  return (
    <div className="goal-panel">
      <div className="goal-row">
        <span className="small muted">Goal</span>
        <div className="segmented small">
          {(["cut", "maintain", "gain"] as Goal[]).map((g) => (
            <button key={g} className={goal === g ? "active" : ""} onClick={() => goal !== g && api.setGoal(g).then(changed)}>
              {GOAL_TEXT[g]}
            </button>
          ))}
        </div>
        {/* Maintain always aims for no change (within a small band), so it has no target to set. */}
        {goal && goal !== "maintain" && (
          <label className="small goal-target">
            Target
            <input
              className="input small-input"
              inputMode="decimal"
              placeholder="default"
              value={rate}
              onChange={(e) => setRate(e.target.value)}
              onBlur={() => {
                const kg = parseRate(rate, goal, unit);
                if (kg !== info.weekly_rate_kg) api.setGoal(goal, info.start_date ?? undefined, kg).then(changed);
              }}
            />
            {unit}/week
          </label>
        )}
        {info.start_date && <span className="small muted">since {formatDate(info.start_date)}</span>}
      </div>
      <details className="goal-history">
        <summary className="small">Goal history</summary>
        <table className="table small">
          <tbody>
            {[...history].reverse().map((h) => (
              <tr key={h.phase_id}>
                <td>{GOAL_TEXT[h.goal]}</td>
                <td>
                  {formatDate(h.start_date)} – {h.end_date ? formatDate(h.end_date) : "now"}
                </td>
                <td className="muted">
                  {h.goal === "maintain" ? "±0" : h.weekly_rate_kg != null ? `${signed(toUnit(h.weekly_rate_kg, unit), 2)} ${unit}/wk` : "default"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="row small">
          <select className="input" value={add.goal} onChange={(e) => setAdd({ ...add, goal: e.target.value as Goal })}>
            {(["cut", "maintain", "gain"] as Goal[]).map((g) => (
              <option key={g} value={g}>
                {GOAL_TEXT[g]}
              </option>
            ))}
          </select>
          from
          <input className="input" type="date" value={add.start} onChange={(e) => setAdd({ ...add, start: e.target.value })} />
          {add.goal !== "maintain" && (
            <input
              className="input small-input"
              inputMode="decimal"
              placeholder={`${unit}/wk`}
              value={add.rate}
              onChange={(e) => setAdd({ ...add, rate: e.target.value })}
            />
          )}
          <button
            className="button"
            disabled={!add.start}
            onClick={() => api.setGoal(add.goal, add.start, add.goal === "maintain" ? null : parseRate(add.rate, add.goal, unit)).then(changed)}
          >
            Add
          </button>
        </div>
        <p className="small muted">Adding a goal with an earlier start date ends the goal before it on that date.</p>
      </details>
    </div>
  );
}

/** Target vs actual weekly rate of weight change for the current goal. */
function GoalRate({ rows, unit }: { rows: WeighInRow[]; unit: "kg" | "lb" }) {
  const [info, setInfo] = useState<GoalInfo | null>(null);
  useEffect(() => {
    const load = () => api.currentGoal().then(setInfo);
    load();
    window.addEventListener("goal-changed", load);
    return () => window.removeEventListener("goal-changed", load);
  }, []);
  const s = goalStatus(info, rows, []);
  if (!s) return null;
  const u = (kg: number) => toUnit(kg, unit);
  const [lo, hi] = s.targetKg;
  const aim =
    lo === hi
      ? `${signed(u(lo), 2)} ${unit} a week`
      : s.goal === "maintain"
        ? `within ±${u(hi).toFixed(1)} ${unit} a week`
        : `${signed(u(lo))} to ${signed(u(hi))} ${unit} a week`;
  return (
    <p className="small goal-rate">
      <strong>{GOAL_TEXT[s.goal]}:</strong> aim for {aim}.{" "}
      {s.actualKg == null ? (
        <span className="muted">Weigh in over a few weeks to see your rate.</span>
      ) : (
        <span>
          Last 3 weeks: {signed(u(s.actualKg), 2)} {unit} a week{s.onTrack ? " (on target)." : "."}
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

// ---- Bodyweight ---------------------------------------------------------------------------------------

function BodyweightTab() {
  const { unit } = usePrefs();
  const [rows, setRows] = useState<WeighInRow[]>([]);
  const [phases, setPhases] = useState<Phase[]>([]);
  const [range, setRange] = useSavedRange("bodyweight");
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
          share={false}
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
  const [flagged, setFlagged] = useState<string[]>([]);
  const [review, setReview] = useState<{ scan: InBodyScan; doubtful: string[] }[]>([]);
  const [reading, setReading] = useState<string | null>(null);
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
  const available = fields.filter(
    (f) => scans.some((s) => s.values[f.column] != null) && !f.column.startsWith("lean_") && !f.column.startsWith("fat_"),
  );

  async function onFile(file?: File) {
    if (!file) return;
    const text = await file.text();
    setCsv({ text, preview: await api.previewInbodyCsv(text) });
  }

  /** Result-sheet images: clean reads are saved, anything doubtful opens for a check. */
  async function onImages(files: FileList | null) {
    if (!files?.length) return;
    setMsg(null);
    const existing = new Set(scans.map((s) => s.measured_at_local.slice(0, 16)));
    let saved = 0;
    let skipped = 0;
    const toCheck: { scan: InBodyScan; doubtful: string[] }[] = [];
    const failed: string[] = [];
    for (const [i, file] of [...files].entries()) {
      setReading(`Reading sheet ${i + 1} of ${files.length}…`);
      try {
        const r = await api.readInbodyImage(await fileToBase64(file), unit);
        if (r.measured_at && existing.has(r.measured_at.slice(0, 16))) {
          skipped++;
          continue;
        }
        const scan: InBodyScan = {
          id: null,
          measured_at_local: r.measured_at ?? nowLocal(),
          values: r.values,
          device: "InBody 580",
          notes: null,
        };
        if (r.doubtful.length === 0 && r.measured_at) {
          await api.saveInbodyScan(scan);
          existing.add(r.measured_at.slice(0, 16));
          saved++;
        } else {
          toCheck.push({ scan, doubtful: r.measured_at ? r.doubtful : [...r.doubtful, "date"] });
        }
      } catch (e) {
        failed.push(`${file.name}: ${e}`);
      }
    }
    setReading(null);
    load();
    setMsg(
      [
        saved && `Saved ${saved} scan${saved > 1 ? "s" : ""}.`,
        skipped && `${skipped} already in your scans.`,
        toCheck.length && `${toCheck.length} need${toCheck.length > 1 ? "" : "s"} a quick check.`,
        failed.length && `Couldn't read: ${failed.join("; ")}`,
      ]
        .filter(Boolean)
        .join(" "),
    );
    if (toCheck.length) {
      setEditing(toCheck[0].scan);
      setFlagged(toCheck[0].doubtful);
      setReview(toCheck.slice(1));
    }
  }
  const nextReview = () => {
    const [next, ...rest] = review;
    setEditing(next?.scan ?? null);
    setFlagged(next?.doubtful ?? []);
    setReview(rest);
  };

  return (
    <>
      <section className="card">
        <div className="row" style={{ marginTop: 0 }}>
          <button
            className="button primary"
            onClick={() => {
              setFlagged([]);
              setEditing({ id: null, measured_at_local: nowLocal(), values: {}, device: null, notes: null });
            }}
          >
            + Add scan
          </button>
          <label className="button">
            Read sheet images
            <input type="file" accept="image/*" multiple hidden disabled={!!reading} onChange={(e) => onImages(e.target.files)} />
          </label>
          <label className="button">
            Import CSV
            <input type="file" accept=".csv,text/csv" hidden onChange={(e) => onFile(e.target.files?.[0])} />
          </label>
        </div>
        <p className="muted small">Sheet images: InBody 580 result sheets saved as PNG or JPEG, read on this device.</p>
        {reading && <p className="small">{reading}</p>}
        {msg && <p className="success small">{msg}</p>}
      </section>
      {scans.length === 0 ? (
        <p className="empty">No scans yet. Add one from your InBody result sheet, or import a CSV from LookinBody.</p>
      ) : (
        <>
          <section className="card">
            <div className="title-row">
              <h2 className="section-title">InBody: any metric</h2>
              <ShareButton kind="inbody" />
            </div>
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
          <Segmental scans={scans} unit={unit} />
          <section className="card">
            <div className="section-label">Scans</div>
            {[...scans].reverse().map((s) => (
              <div
                key={s.id}
                className="start-row"
                onClick={() => {
                  setFlagged([]);
                  setEditing(s);
                }}
              >
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
          key={editing.measured_at_local}
          scan={editing}
          fields={fields}
          unit={unit}
          flagged={flagged}
          onClose={nextReview}
          onSaved={() => {
            load();
            nextReview();
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

/** Segmental lean or fat mass of the latest scan: one bar per segment on a shared scale, with the
 *  normal range when the sheet gives "% of normal", and each segment's change since the first scan. */
function Segmental({ scans, unit }: { scans: InBodyScan[]; unit: "kg" | "lb" }) {
  const theme = useChartTheme();
  const [kind, setKind] = useState<"lean" | "fat">("lean");
  const kgOf = (x: InBodyScan, seg: string) => x.values[`${kind}_${seg}_kg`];
  const withData = scans.filter((x) => SEGMENTS.some(([seg]) => kgOf(x, seg) != null));
  if (!withData.length) return null;
  const scan = withData[withData.length - 1];
  const first = withData[0];
  const pct = (seg: string) => scan.values[`${kind}_${seg}_pct`];
  // Lean mass on the InBody sheet comes with "% of normal"; otherwise bars show kg.
  const usePct = SEGMENTS.every(([seg]) => pct(seg) != null);
  const rows = SEGMENTS.map(([seg, name]) => {
    const kg = kgOf(scan, seg);
    const was = first !== scan ? kgOf(first, seg) : undefined;
    return { seg, name, kg, pct: pct(seg), delta: kg != null && was != null ? kg - was : null };
  });
  const values = rows.map((r) => (usePct ? r.pct : r.kg)).filter((v): v is number => v != null);
  const domain: [number, number] = usePct
    ? [Math.min(60, d3.min(values)! - 10), Math.max(160, d3.max(values)! + 10)]
    : [0, d3.max(values)! * 1.1];
  const W = 340;
  const labelW = 74;
  const valueW = 112;
  const rowH = 30;
  const top = 20;
  const x = d3.scaleLinear(domain, [labelW, W - valueW]);
  const H = top + rows.length * rowH + 4;
  const ticks = usePct ? [80, 100, 120, 140].filter((t) => t > domain[0] && t < domain[1]) : x.ticks(4);
  const fill = kind === "lean" ? theme.categorical[0] : theme.categorical[1];
  const delta = (d: number | null) => (d == null || Math.abs(d) < 0.005 ? "" : ` (${d > 0 ? "+" : "−"}${Math.abs(toUnit(d, unit)).toFixed(1)})`);
  return (
    <section className="card">
      <div className="title-row">
        <h2 className="section-title">Segmental {kind} mass</h2>
        <div className="segmented small">
          <button className={kind === "lean" ? "active" : ""} onClick={() => setKind("lean")}>
            Lean
          </button>
          <button className={kind === "fat" ? "active" : ""} onClick={() => setKind("fat")}>
            Fat
          </button>
        </div>
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} width="100%" style={{ display: "block", maxWidth: 560 }} fontFamily="system-ui, sans-serif">
        {usePct && (
          <>
            {/* InBody's normal range for segmental lean mass is about 90–110 % of ideal. */}
            <rect x={x(90)} y={top - 4} width={x(110) - x(90)} height={H - top} fill={theme.grid} />
            <text x={(x(90) + x(110)) / 2} y={top - 8} fontSize={9} fill={theme.muted} textAnchor="middle">
              normal
            </text>
          </>
        )}
        {ticks.map((t) => (
          <g key={t}>
            <line x1={x(t)} x2={x(t)} y1={top - 4} y2={H} stroke={t === 100 && usePct ? theme.muted : theme.gridStrong} strokeDasharray={t === 100 && usePct ? undefined : "2 3"} />
            {!(usePct && t === 100) && (
              <text x={x(t)} y={top - 8} fontSize={9} fill={theme.muted} textAnchor="middle">
                {usePct ? `${t}%` : toUnit(t, unit).toFixed(0)}
              </text>
            )}
          </g>
        ))}
        {rows.map((r, i) => {
          const y = top + i * rowH;
          const v = usePct ? r.pct : r.kg;
          return (
            <g key={r.seg}>
              <text x={0} y={y + rowH / 2 + 4} fontSize={11.5} fill={theme.text}>
                {r.name}
              </text>
              {v != null && (
                <rect x={x(domain[0])} y={y + 7} width={Math.max(2, x(v) - x(domain[0]))} height={rowH - 14} rx={3} fill={fill} />
              )}
              <text x={W} y={y + rowH / 2 + 4} fontSize={11} fill={theme.text} textAnchor="end">
                {r.kg != null ? `${toUnit(r.kg, unit).toFixed(1)} ${unit}` : "—"}
                {usePct && r.pct != null ? ` · ${r.pct}%` : ""}
                <tspan fill={theme.muted}>{delta(r.delta)}</tspan>
              </text>
            </g>
          );
        })}
      </svg>
      <p className="muted small">
        {formatDate(scan.measured_at_local)}
        {first !== scan ? ` · changes in brackets since ${formatDate(first.measured_at_local)}` : ""}
        {usePct ? " · bars are % of the ideal for your height (InBody), shaded band is normal" : ""}
      </p>
    </section>
  );
}

function ScanForm({
  scan,
  fields,
  unit,
  flagged = [],
  onClose,
  onSaved,
}: {
  scan: InBodyScan;
  fields: InBodyField[];
  unit: "kg" | "lb";
  /** Fields read from an image that need checking (highlighted). */
  flagged?: string[];
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
        {flagged.length > 0 ? (
          <p className="warn small">
            Read from the sheet image. Check the highlighted {flagged.includes("date") ? "date and " : ""}fields against the sheet before
            saving.
          </p>
        ) : (
          <p className="muted small">Every field is optional; fill in what's on your result sheet.</p>
        )}
        {groups.map(([title, pred]) => (
          <details key={title} open={title === "Body composition" || fields.some((f) => pred(f) && flagged.includes(f.column))}>
            <summary className="section-label">{title}</summary>
            <div className="field-grid">
              {fields.filter(pred).map((f) => (
                <label key={f.column} className={flagged.includes(f.column) ? "flagged" : undefined}>
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
  const [range, setRange] = useSavedRange("diet");
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
