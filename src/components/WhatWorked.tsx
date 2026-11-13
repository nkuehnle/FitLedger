import { useEffect, useState } from "react";
import { api, type Phase, type PhaseSummary } from "../api";
import { usePrefs } from "../App";
import { today } from "../charts/RangePicker";
import { formatDate, parseLocal, toUnit } from "../format";

const MIN_WEEKS = 4;
const signed = (v: number, digits: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toFixed(digits)}`;

function longEnough(p: Phase): boolean {
  const days = (parseLocal(p.end_date ?? today()).getTime() - parseLocal(p.start_date).getTime()) / 86400000 + 1;
  return days >= MIN_WEEKS * 7;
}

const span = (p: Phase) => `${p.name} (${formatDate(p.start_date)})`;

/** Two phases side by side. */
export function WhatWorked({ phases }: { phases: Phase[] }) {
  const { unit } = usePrefs();
  const options = phases.filter(longEnough);
  const [a, setA] = useState<number | null>(null);
  const [b, setB] = useState<number | null>(null);
  const [result, setResult] = useState<[PhaseSummary, PhaseSummary] | null>(null);
  const first = a ?? options[options.length - 2]?.id ?? null;
  const second = b ?? options[options.length - 1]?.id ?? null;
  useEffect(() => {
    if (first != null && second != null && first !== second) api.phaseCompare(first, second).then(setResult);
    else setResult(null);
  }, [first, second]);
  if (options.length < 2) return null;

  const picker = (value: number | null, onChange: (id: number) => void, label: string) => (
    <select className="select" aria-label={label} value={value ?? ""} onChange={(e) => onChange(Number(e.target.value))}>
      {options.map((p) => (
        <option key={p.id} value={p.id!}>
          {span(p)}
        </option>
      ))}
    </select>
  );
  const rows: [string, (s: PhaseSummary) => string][] = [
    ["Length", (s) => `${s.weeks.toFixed(0)} weeks`],
    ["Sessions a week", (s) => s.sessions_per_week.toFixed(1)],
    ["Sets a week", (s) => s.sets_per_week.toFixed(0)],
    ["Strength, start to end", (s) => (s.median_change == null ? "–" : `${signed(s.median_change * 100, 1)}%`)],
    ["Bodyweight", (s) => (s.bodyweight_change_kg == null ? "–" : `${signed(toUnit(s.bodyweight_change_kg, unit), 1)} ${unit}`)],
    ["Alongside", (s) => s.overlaps.map(([n, w]) => `${n} ${w.toFixed(0)} wk`).join(", ") || "–"],
  ];
  return (
    <section className="card span-2">
      <h2 className="section-title">What worked</h2>
      <div className="row">
        {picker(first, setA, "First phase")}
        {picker(second, setB, "Second phase")}
      </div>
      {result && (
        <>
          <table className="sets small phase-compare">
            <tbody>
              {rows.map(([label, f]) => (
                <tr key={label}>
                  <th scope="row">{label}</th>
                  <td>{f(result[0])}</td>
                  <td>{f(result[1])}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted small">
            Strength is the typical change across the {result[0].lifts_compared} {result[0].lifts_compared === 1 ? "lift" : "lifts"} done in
            both. A comparison isn't proof of cause: diet and program changes overlap.
          </p>
        </>
      )}
    </section>
  );
}
