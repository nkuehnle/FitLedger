import { useEffect, useMemo, useState } from "react";
import { api, type InBodyScan, type MeasurementRow, type WeighInRow } from "../api";
import { usePrefs } from "../App";
import { TimeSeriesChart, type Series } from "../charts/TimeSeriesChart";
import { bodyFatSeries, type Pt, type Sex } from "../bodyfat";
import { toUnit } from "../format";

const CM_PER_IN = 2.54;
const signed = (v: number, digits = 1) => `${v > 0 ? "+" : v < 0 ? "−" : "±"}${Math.abs(v).toFixed(digits)}`;

const short = (t: Date) => t.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
const axis = (v: number) => String(Number(v.toFixed(1)));

/** Body fat % (InBody, logged, Navy and RFM estimates) and lean, muscle and fat mass, measured and estimated. */
export function BodyFatTab() {
  const { unit } = usePrefs();
  const [data, setData] = useState<{ m: MeasurementRow[]; scans: InBodyScan[]; w: WeighInRow[] } | null>(null);
  const [sex, setSex] = useState<Sex | null>(null);
  const [heightCm, setHeightCm] = useState<number | null>(null);
  const [heightText, setHeightText] = useState("");
  const inches = unit === "lb";

  useEffect(() => {
    Promise.all([api.measurements(), api.inbodyScans(), api.weighIns(true)]).then(([m, scans, w]) => setData({ m, scans, w }));
    api.getSetting("sex").then((s) => setSex(s === "male" || s === "female" ? s : null));
    api.getSetting("height_cm").then((h) => {
      const v = h ? Number(h) : null;
      setHeightCm(v);
      if (v) setHeightText(String(Math.round((inches ? v / CM_PER_IN : v) * 10) / 10));
    });
  }, []);

  const s = useMemo(() => data && bodyFatSeries(data.m, data.scans, data.w, sex, heightCm), [data, sex, heightCm]);
  if (!s) return null;

  const saveHeight = (text: string) => {
    setHeightText(text);
    const v = Number(text);
    if (!v) return;
    const cm = inches ? v * CM_PER_IN : v;
    setHeightCm(cm);
    api.setSetting("height_cm", String(cm));
  };
  const saveSex = (v: Sex) => {
    setSex(v);
    api.setSetting("sex", v);
  };

  const fat: Series[] = [
    { id: "inbody", label: "InBody", points: s.measured },
    { id: "navy", label: "Navy (tape)", points: s.navy },
    { id: "rfm", label: "RFM (waist)", points: s.rfm },
    { id: "logged", label: "Logged", points: s.logged, style: "points" },
  ].filter((x) => x.points.length) as Series[];
  const kg = (pts: Pt[]) => pts.map((p) => ({ t: p.t, v: toUnit(p.v, unit) }));
  const nonEmpty = (xs: Series[]) => xs.filter((x) => x.points.length);
  const lean = nonEmpty([
    { id: "ffm", label: "Lean (InBody)", points: kg(s.leanMeasured) },
    { id: "lean", label: "Lean (estimate)", points: kg(s.leanEstimated) },
    { id: "smm", label: "Muscle (InBody)", points: kg(s.smm) },
    { id: "smm-est", label: "Muscle (estimate)", points: kg(s.smmEstimated) },
  ]);
  const fatMass = nonEmpty([
    { id: "bfm", label: "Fat mass (InBody)", points: kg(s.fatMeasured) },
    { id: "bfm-est", label: "Fat mass (estimate)", points: kg(s.fatEstimated) },
  ]);
  // Latest reading of each kind, and its change over that source's own dates (scans and tape
  // estimates rarely overlap).
  const rows = [
    { label: "Lean mass", inbody: s.leanMeasured, est: s.leanEstimated },
    { label: "Skeletal muscle", inbody: s.smm, est: s.smmEstimated },
    { label: "Fat mass", inbody: s.fatMeasured, est: s.fatEstimated },
  ].filter((r) => r.inbody.length || r.est.length);
  const cell = (pts: Pt[]) =>
    pts.length ? (
      <>
        <strong>
          {toUnit(pts[pts.length - 1].v, unit).toFixed(1)} {unit}
        </strong>
        {pts.length >= 2 && <span className="muted"> ({signed(toUnit(pts[pts.length - 1].v - pts[0].v, unit))} since {short(pts[0].t)})</span>}
        <div className="muted">{short(pts[pts.length - 1].t)}</div>
      </>
    ) : (
      <span className="muted">—</span>
    );
  const needsProfile = !sex || !heightCm;

  return (
    <>
      <section className="card">
        <div className="row">
          <span className="small muted">For tape estimates:</span>
          <div className="segmented">
            {(["male", "female"] as Sex[]).map((v) => (
              <button key={v} className={sex === v ? "active" : ""} onClick={() => saveSex(v)}>
                {v === "male" ? "Male" : "Female"}
              </button>
            ))}
          </div>
          <label className="row small">
            Height
            <input className="input small-input" inputMode="decimal" value={heightText} onChange={(e) => saveHeight(e.target.value)} />
            {inches ? "in" : "cm"}
          </label>
        </div>
        <p className="muted small">
          Navy uses waist and neck{sex === "female" ? " and hips" : ""} measured within two weeks of each other; RFM uses waist
          only. Both are estimates, typically within ±3–4 % of a scan.
        </p>
      </section>
      <section className="card">
        <h2 className="section-title">Body fat %</h2>
        {fat.length ? (
          <TimeSeriesChart series={fat} unit="%" height={240} yFormat={axis} />
        ) : (
          <p className="muted small">
            {needsProfile
              ? "Set sex and height above, then log waist and neck under Body parts (or add an InBody scan)."
              : "Log waist and neck under Body parts, or add an InBody scan."}
          </p>
        )}
      </section>
      {rows.length > 0 && (
        <section className="card">
          <h2 className="section-title">Muscle and fat</h2>
          <table className="table small">
            <thead>
              <tr>
                <th />
                <th>InBody</th>
                <th>Estimate</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.label}>
                  <td>{r.label}</td>
                  <td>{cell(r.inbody)}</td>
                  <td>{cell(r.est)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      <section className="card">
        <h2 className="section-title">Lean and muscle mass</h2>
        {lean.length ? (
          <TimeSeriesChart series={lean} unit={unit} height={240} yFormat={axis} />
        ) : (
          <p className="muted small">Needs body fat readings (above) and weigh-ins within a week of them.</p>
        )}
      </section>
      {fatMass.length > 0 && (
        <section className="card">
          <h2 className="section-title">Fat mass</h2>
          <TimeSeriesChart series={fatMass} unit={unit} height={220} yFormat={axis} />
        </section>
      )}
      <p className="muted small">
        Estimates use your weight at each tape measurement: lean = weight × (1 − body fat %), fat = weight × body fat %, and
        muscle = lean × {Math.round(s.muscleShare * 100)} %
        {s.muscleShareFromScans ? " (your skeletal muscle share of lean mass on your InBody scans)" : " (a typical share; an InBody scan calibrates it)"}.
        Lean mass includes water and glycogen, so expect noise.
      </p>
    </>
  );
}
