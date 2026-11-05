import { useEffect, useMemo, useState } from "react";
import { api, type InBodyScan, type MeasurementRow, type WeighInRow } from "../api";
import { usePrefs } from "../App";
import { TimeSeriesChart, type Series } from "../charts/TimeSeriesChart";
import { bodyFatSeries, type Pt, type Sex } from "../bodyfat";
import { toUnit } from "../format";
import { Standing, whoOf } from "./Standing";

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
  const [birthYear, setBirthYear] = useState("");
  const inches = unit === "lb";

  useEffect(() => {
    Promise.all([api.measurements(), api.inbodyScans(), api.weighIns(true)]).then(([m, scans, w]) => setData({ m, scans, w }));
    api.getSetting("sex").then((s) => setSex(s === "male" || s === "female" ? s : null));
    api.getSetting("birth_year").then((y) => setBirthYear(y ?? ""));
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
  const saveBirthYear = (text: string) => {
    setBirthYear(text);
    if (/^(19|20)\d\d$/.test(text)) api.setSetting("birth_year", text);
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
  const who = whoOf(sex, /^(19|20)\d\d$/.test(birthYear) ? birthYear : null, null);
  const compare = who && heightCm ? comparisons(data!.scans, s, heightCm) : [];

  return (
    <>
      <section className="card">
        <div className="row">
          <span className="small muted">Your profile:</span>
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
          <label className="row small">
            Born
            <input
              className="input small-input"
              inputMode="numeric"
              placeholder="year"
              value={birthYear}
              onChange={(e) => saveBirthYear(e.target.value.trim())}
            />
          </label>
        </div>
        <p className="muted small">
          Navy uses waist and neck{sex === "female" ? " and hips" : ""} measured within two weeks of each other; RFM uses waist
          only. Both are estimates, typically within ±3–4 % of a scan. Birth year enables the percentile comparisons.
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
      {compare.length > 0 && who && (
        <section className="card">
          <h2 className="section-title">How you compare</h2>
          {compare.map((c) => (
            <Standing key={c.metric} {...c} who={who} />
          ))}
          <p className="muted small">
            Against US adults of your sex and age, measured by DXA scan. InBody and tape estimates read differently from DXA (often a few
            points lower body fat), so treat these as a rough guide.
          </p>
        </section>
      )}
      {!who && (s.measured.length > 0 || s.navy.length > 0) && (
        <p className="muted small">Set sex, height and birth year above to see how you compare with other adults.</p>
      )}
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

/** Bone mineral as a share of fat-free mass in adults (DXA): roughly 4.5 %. */
const BONE_SHARE = 0.045;

/**
 * Index comparisons from InBody scans (or tape estimates for body fat), oldest and latest, in the
 * units of the reference data: % fat, and kg/m² of fat mass, lean soft tissue (DXA "lean", which
 * excludes bone) and arm + leg lean.
 */
function comparisons(scans: InBodyScan[], s: ReturnType<typeof bodyFatSeries>, heightCm: number) {
  const h2 = (heightCm / 100) ** 2;
  const ends = (xs: number[]) => (xs.length > 1 ? [xs[0], xs[xs.length - 1]] : xs);
  const sorted = [...scans].sort((a, b) => a.measured_at_local.localeCompare(b.measured_at_local));
  const fromScans = (f: (v: Record<string, number>) => number | null) =>
    ends(sorted.map((x) => f(x.values)).filter((v): v is number => v != null && v > 0));
  const index = (v: number) => `${v.toFixed(1)} kg/m²`;
  const pbf = s.measured.length ? s.measured : s.navy;
  const fatMass = s.fatMeasured.length ? s.fatMeasured : s.fatEstimated;
  const out = [
    {
      metric: "pbf",
      label: "Body fat",
      values: ends(pbf.map((p) => p.v)),
      show: (v: number) => `${v.toFixed(1)} %`,
      note: s.measured.length ? "From your InBody scans." : "From your Navy tape estimates.",
    },
    {
      metric: "fmi",
      label: "Fat mass index",
      values: ends(fatMass.map((p) => p.v / h2)),
      show: index,
      note: "Fat mass ÷ height².",
    },
    {
      metric: "lmi",
      label: "Lean mass index",
      values: fromScans((v) => {
        // Fat-free mass, unambiguous; "lean body mass" columns are fat-free mass on some exports and soft lean on others.
        const ffm = v.fat_free_mass_kg ?? v.lean_body_mass_kg;
        return ffm ? (ffm * (1 - BONE_SHARE)) / h2 : null;
      }),
      show: index,
      note: "Lean mass without bone ÷ height², to match DXA: your InBody fat-free mass less about 4.5 % for bone.",
    },
    {
      metric: "almi",
      label: "Arm and leg lean index",
      values: fromScans((v) => {
        const parts = ["lean_right_arm_kg", "lean_left_arm_kg", "lean_right_leg_kg", "lean_left_leg_kg"].map((k) => v[k]);
        return parts.every((x) => x > 0) ? parts.reduce((a, b) => a + b, 0) / h2 : null;
      }),
      show: index,
      note: "Lean mass of both arms and legs ÷ height², the usual measure of muscle on the limbs.",
    },
  ];
  return out.filter((c) => c.values.length > 0);
}
