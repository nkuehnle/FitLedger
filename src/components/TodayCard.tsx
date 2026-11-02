import { useEffect, useState } from "react";
import { api, type EnergyPoint, type WeighInRow } from "../api";
import { usePrefs } from "../App";
import { today } from "../charts/RangePicker";
import { formatDate, nowLocal, parseLocal, toUnit } from "../format";
import { GOAL_TEXT, goalStatus, type GoalInfo } from "../goal";
import { loadWeighInPlan, type WeighInPlan } from "../weighIn";

const KG_PER_LB = 0.45359237;
const signed = (v: number, digits = 1) => `${v > 0 ? "+" : v < 0 ? "−" : "±"}${Math.abs(v).toFixed(digits)}`;

/** Top of the Workouts tab: how the weight goal is going, calories vs target, and a quick weigh-in. */
export function TodayCard() {
  const { unit } = usePrefs();
  const [rows, setRows] = useState<WeighInRow[] | null>(null);
  const [energy, setEnergy] = useState<EnergyPoint[]>([]);
  const [goal, setGoal] = useState<GoalInfo | null>(null);
  const [plan, setPlan] = useState<WeighInPlan | null>(null);
  const [entry, setEntry] = useState("");
  const load = () => {
    api.weighIns(false).then(setRows);
    api.energyBalance(14).then(setEnergy);
    api.currentGoal().then(setGoal);
  };
  useEffect(() => {
    load();
    loadWeighInPlan().then(setPlan);
  }, []);
  if (!rows) return null;

  const last = rows[rows.length - 1];
  const loggedToday = last && last.at.slice(0, 10) === today();
  const weighDay = plan?.days.includes(new Date().getDay()) ?? false;
  const s = goalStatus(goal, rows, energy);
  const u = (kg: number) => toUnit(kg, unit);

  async function add() {
    const v = Number(entry);
    if (!v) return;
    await api.addWeighIn(nowLocal(), unit === "lb" ? v * KG_PER_LB : v);
    setEntry("");
    load();
  }

  const target =
    s && (s.targetKg[0] === s.targetKg[1] ? signed(u(s.targetKg[0]), 2) : `${signed(u(s.targetKg[0]))} to ${signed(u(s.targetKg[1]))}`);
  return (
    <section className="card today-card">
      <div className="today-grid">
        <a className="today-stat" href="#/body/weight">
          <span className="muted small">Weight{last ? ` · ${formatDate(last.at)}` : ""}</span>
          <strong>{last ? `${u(last.weight_kg).toFixed(1)} ${unit}` : "—"}</strong>
          {s && (
            <span className="small">
              {s.actualKg == null ? (
                <span className="muted">
                  {GOAL_TEXT[s.goal]} {target} {unit}/wk
                </span>
              ) : (
                <>
                  <span className={s.onTrack ? "success" : "warn"}>
                    {signed(u(s.actualKg), 2)} {unit}/wk
                  </span>{" "}
                  <span className="muted">
                    vs {target} ({GOAL_TEXT[s.goal].toLowerCase()})
                  </span>
                </>
              )}
            </span>
          )}
        </a>
        <a className="today-stat" href="#/body/diet">
          <span className="muted small">Calories</span>
          {s?.offKcal != null ? (
            <>
              <strong className={Math.abs(s.offKcal) <= 100 ? "success" : "warn"}>
                {Math.abs(s.offKcal) <= 100 ? "On target" : `${signed(Math.round(s.offKcal), 0)} kcal/day`}
              </strong>
              <span className="small muted">
                {signed(Math.round(s.balanceKcal!), 0)} vs {signed(Math.round(s.targetKcal), 0)} target balance
              </span>
            </>
          ) : (
            <span className="small muted">Log calories to compare with your goal</span>
          )}
        </a>
        <div className="today-stat">
          <span className={`small ${weighDay && !loggedToday ? "warn" : "muted"}`}>
            {loggedToday ? "Weighed in today ✓" : weighDay ? "Weigh-in day" : "Quick weigh-in"}
          </span>
          <div className="row tight">
            <input
              className="input small-input"
              inputMode="decimal"
              placeholder={unit}
              value={entry}
              onChange={(e) => setEntry(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
            <button className="button" disabled={!Number(entry)} onClick={add}>
              Log
            </button>
          </div>
        </div>
      </div>
      {last && parseLocal(last.at).getTime() < Date.now() - 10 * 86400000 && (
        <p className="small muted">No weigh-in for over a week, so the trend may be stale.</p>
      )}
    </section>
  );
}
