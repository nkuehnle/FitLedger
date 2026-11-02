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

/** Body fat (InBody, logged, Navy and RFM estimates) and lean mass over time. */
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
  const lean: Series[] = [
    { id: "ffm", label: "Fat-free mass (InBody)", points: kg(s.leanMeasured) },
    { id: "lean", label: "Lean mass (estimated)", points: kg(s.leanEstimated) },
    { id: "smm", label: "Skeletal muscle (InBody)", points: kg(s.smm) },
  ].filter((x) => x.points.length);
  // Each source's change over its own dates: scans and tape estimates rarely overlap.
  const changes = [
    { label: "Fat-free mass (InBody)", pts: s.leanMeasured },
    { label: "Lean mass (tape estimate)", pts: s.leanEstimated },
    { label: "Skeletal muscle (InBody)", pts: s.smm },
  ].filter((c) => c.pts.length >= 2);
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
      <section className="card">
        <h2 className="section-title">Lean mass</h2>
        {changes.map(({ label, pts }) => (
          <p key={label} className="small">
            {label}: <strong>{signed(toUnit(pts[pts.length - 1].v - pts[0].v, unit))} {unit}</strong>{" "}
            <span className="muted">
              ({short(pts[0].t)} – {short(pts[pts.length - 1].t)})
            </span>
          </p>
        ))}
        {lean.length ? (
          <TimeSeriesChart series={lean} unit={unit} height={240} yFormat={axis} />
        ) : (
          <p className="muted small">Needs body fat readings (above) and weigh-ins within a week of them.</p>
        )}
        <p className="muted small">Estimated lean mass = weight × (1 − body fat %). It includes water and glycogen, so expect noise.</p>
      </section>
    </>
  );
}
