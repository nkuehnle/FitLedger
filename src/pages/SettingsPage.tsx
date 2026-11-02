import { useEffect, useState } from "react";
import { usePrefs, type Theme } from "../App";
import { api, type WeightUnit } from "../api";
import { IS_ANDROID } from "../platform";
import { loadWeighInPlan, saveWeighInPlan, type WeighInPlan } from "../weighIn";

const WEEK: [number, string][] = [
  [1, "Mon"],
  [2, "Tue"],
  [3, "Wed"],
  [4, "Thu"],
  [5, "Fri"],
  [6, "Sat"],
  [0, "Sun"],
];

/** Weigh-in days and the morning reminder for them. */
function WeighInDays() {
  const [plan, setPlan] = useState<WeighInPlan | null>(null);
  useEffect(() => {
    loadWeighInPlan().then(setPlan);
  }, []);
  if (!plan) return null;
  const save = (p: WeighInPlan) => {
    setPlan(p);
    saveWeighInPlan(p);
  };
  return (
    <section className="card">
      <h2 className="section-title">Weigh-in days</h2>
      <div className="segmented week-picker">
        {WEEK.map(([d, l]) => (
          <button
            key={d}
            className={plan.days.includes(d) ? "active" : ""}
            onClick={() => save({ ...plan, days: plan.days.includes(d) ? plan.days.filter((x) => x !== d) : [...plan.days, d].sort() })}
          >
            {l}
          </button>
        ))}
      </div>
      <label className="row small">
        Remind me at
        <input
          className="input"
          type="time"
          value={plan.time}
          onChange={(e) => e.target.value && save({ ...plan, time: e.target.value })}
        />
      </label>
      <p className="muted small">
        {IS_ANDROID
          ? "A notification reminds you on these mornings, unless you've already logged a weight that day."
          : "The Workouts tab reminds you on these days. On Android you also get a notification."}
      </p>
    </section>
  );
}

export default function SettingsPage() {
  const { unit, setUnit, theme, setTheme } = usePrefs();
  const [formula, setFormula] = useState("epley");
  const [secondary, setSecondary] = useState("0.5");
  const [warmups, setWarmups] = useState(false);
  useEffect(() => {
    api.getSetting("e1rm_formula").then((v) => v && setFormula(v));
    api.getSetting("secondary_set_weight").then((v) => v && setSecondary(v));
    api.getSetting("count_warmups").then((v) => setWarmups(v === "true"));
  }, []);
  const seg = <T extends string>(value: T, options: [T, string][], onChange: (v: T) => void) => (
    <div className="segmented">
      {options.map(([v, l]) => (
        <button key={v} className={value === v ? "active" : ""} onClick={() => onChange(v)}>
          {l}
        </button>
      ))}
    </div>
  );
  return (
    <div className="page narrow">
      <h1 className="page-title">Settings</h1>
      <section className="card">
        <h2 className="section-title">Plan</h2>
        <div className="export-list">
          <a className="start-row" href="#/plan/schedule">
            <span>Schedule & starter programs</span>
            <span className="muted">›</span>
          </a>
          <a className="start-row" href="#/plan/templates">
            <span>Templates</span>
            <span className="muted">›</span>
          </a>
          <a className="start-row" href="#/plan/phases">
            <span>Phases</span>
            <span className="muted">›</span>
          </a>
          <a className="start-row" href="#/plan/targets">
            <span>Weekly set targets</span>
            <span className="muted">›</span>
          </a>
        </div>
      </section>
      <WeighInDays />
      <section className="card">
        <h2 className="section-title">Weight unit</h2>
        {seg<WeightUnit>(
          unit,
          [
            ["lb", "lb"],
            ["kg", "kg"],
          ],
          setUnit,
        )}
        <h2 className="section-title">Theme</h2>
        {seg<Theme>(
          theme,
          [
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ],
          setTheme,
        )}
      </section>
      <section className="card">
        <h2 className="section-title">Stats</h2>
        <div className="section-label">Estimated 1RM formula</div>
        {seg(
          formula,
          [
            ["epley", "Epley"],
            ["brzycki", "Brzycki"],
          ],
          (v) => {
            setFormula(v);
            api.setSetting("e1rm_formula", v);
          },
        )}
        <div className="section-label">A secondary muscle counts as</div>
        {seg(
          secondary,
          [
            ["0", "0"],
            ["0.25", "¼"],
            ["0.5", "½"],
            ["0.75", "¾"],
            ["1", "1"],
          ],
          (v) => {
            setSecondary(v);
            api.setSetting("secondary_set_weight", v);
          },
        )}
        <span className="muted small"> set (primary muscles always count 1)</span>
        <label className="check" style={{ marginTop: 12 }}>
          <input
            type="checkbox"
            checked={warmups}
            onChange={(e) => {
              setWarmups(e.target.checked);
              api.setSetting("count_warmups", String(e.target.checked));
            }}
          />
          Count warm-up sets in stats and PRs
        </label>
      </section>
      <section className="card">
        <h2 className="section-title">Data</h2>
        <div className="export-list">
          <a className="start-row" href="#/data">
            <span>Import, export & backup</span>
            <span className="muted">›</span>
          </a>
          <a className="start-row" href="#/review">
            <span>Imported exercise names</span>
            <span className="muted">›</span>
          </a>
        </div>
      </section>
      <section className="card">
        <h2 className="section-title">About</h2>
        <p className="small">Liftbook keeps everything on this device. No account, no server.</p>
        <p className="small muted">
          The exercise library includes exercises from{" "}
          <a href="https://github.com/yuhonas/free-exercise-db" target="_blank" rel="noreferrer">
            free-exercise-db
          </a>
          , released into the public domain under the Unlicense. Muscle mappings for those exercises were generated automatically; you can
          edit any of them.
        </p>
      </section>
    </div>
  );
}
