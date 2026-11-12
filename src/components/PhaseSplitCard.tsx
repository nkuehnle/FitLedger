import { useEffect, useState } from "react";
import { api, type Goal, type InBodyScan, type MeasurementRow, type WeighInRow } from "../api";
import { usePrefs } from "../App";
import { formatDate, parseLocal, toUnit } from "../format";
import { GOAL_TEXT } from "../goal";
import { METHOD_TEXT, MIN_SPAN_DAYS, phaseSplit } from "../phaseSplit";

interface Stretch {
  phase_id: number;
  goal: Goal;
  start_date: string;
  end_date: string | null;
}

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** Measure → Weight: how much of a cut's or gain's weight change was lean and how much fat. */
export function PhaseSplitCard({ rows }: { rows: WeighInRow[] }) {
  const { unit } = usePrefs();
  const [goals, setGoals] = useState<Stretch[]>([]);
  const [scans, setScans] = useState<InBodyScan[]>([]);
  const [measurements, setMeasurements] = useState<MeasurementRow[]>([]);
  const [profile, setProfile] = useState<{ sex: "male" | "female" | null; heightCm: number | null }>({ sex: null, heightCm: null });
  const [picked, setPicked] = useState<number | null>(null);
  useEffect(() => {
    const load = () => api.goalHistory().then(setGoals);
    load();
    api.inbodyScans().then(setScans);
    api.measurements().then(setMeasurements);
    Promise.all([api.getSetting("sex"), api.getSetting("height_cm")]).then(([sex, h]) =>
      setProfile({ sex: sex === "male" || sex === "female" ? sex : null, heightCm: Number(h) || null }),
    );
    window.addEventListener("goal-changed", load);
    return () => window.removeEventListener("goal-changed", load);
  }, []);

  // Holding weight has nothing to split.
  const stretches = goals.filter((g) => g.goal !== "maintain");
  if (!stretches.length) return null;
  const now = new Date();
  const split = (g: Stretch) => phaseSplit(g.start_date, g.end_date, now, measurements, scans, rows, profile.sex, profile.heightCm);
  // The running phase if it's a cut or gain, else the latest one that can be split, else the latest.
  const latest = stretches[stretches.length - 1];
  const fallback = latest.end_date == null ? latest : ([...stretches].reverse().find((g) => split(g)) ?? latest);
  const g = stretches.find((x) => x.phase_id === picked) ?? fallback;
  const s = split(g);
  const signed = (kg: number) => `${kg >= 0 ? "+" : "−"}${Math.abs(toUnit(kg, unit)).toFixed(1)} ${unit}`;
  const label = (x: Stretch) =>
    `${GOAL_TEXT[x.goal]} · ${parseLocal(x.start_date).toLocaleDateString(undefined, { month: "short", year: "numeric" })}${x.end_date ? "" : " (now)"}`;
  const days = Math.round(((g.end_date ? parseLocal(g.end_date) : now).getTime() - parseLocal(g.start_date).getTime()) / 86_400_000);
  // Share of the change that was lean, when both parts moved the same way as the weight.
  const share = s && Math.abs(s.weightKg) >= 0.5 && s.leanKg * s.weightKg > 0 && s.fatKg * s.weightKg >= 0 ? s.leanKg / s.weightKg : null;
  return (
    <section className="card">
      <h2 className="section-title">Lean and fat in a phase</h2>
      <select className="select" value={g.phase_id} onChange={(e) => setPicked(Number(e.target.value))}>
        {[...stretches].reverse().map((x) => (
          <option key={x.phase_id} value={x.phase_id}>
            {label(x)}
          </option>
        ))}
      </select>
      {s ? (
        <>
          <div className="stats-row">
            <div className="stat">
              <div className="stat-value">{signed(s.weightKg)}</div>
              <div className="stat-label">Weight</div>
            </div>
            <div className="stat">
              <div className="stat-value">{signed(s.leanKg)}</div>
              <div className="stat-label">Lean</div>
            </div>
            <div className="stat">
              <div className="stat-value">{signed(s.fatKg)}</div>
              <div className="stat-label">Fat</div>
            </div>
          </div>
          <p className="muted small" style={{ marginBottom: 0 }}>
            {share != null && `About ${Math.round(share * 100)}% of the ${s.weightKg > 0 ? "gain" : "loss"} was lean. `}
            Between {formatDate(iso(s.from))} and {formatDate(iso(s.to))}, from {METHOD_TEXT[s.method]}.{" "}
            {s.method === "inbody"
              ? `Scans swing with hydration, so treat this as within ${unit === "lb" ? "a couple of pounds" : "a kilo or so"}; lean includes water and glycogen, which rise on a gain and fall on a cut.`
              : "Tape estimates are rough: good for the direction, not the decimal. Lean includes water and glycogen, which rise on a gain and fall on a cut."}
          </p>
        </>
      ) : (
        <p className="muted small" style={{ marginBottom: 0 }}>
          {days < MIN_SPAN_DAYS && !g.end_date ? `This phase is ${days} days old. ` : "Not enough body-fat readings in this phase. "}
          This needs two readings of the same kind at least 3 weeks apart: InBody scans, or waist and neck with a weigh-in the same week.{" "}
          {!g.end_date && (
            <>
              <a href="#/body/parts">Measure now</a> to have a starting point.
            </>
          )}
        </p>
      )}
    </section>
  );
}
