import { useEffect, useState } from "react";
import { api, type DayHealth, type EnergyPoint, type FatigueReport, type WeighInRow } from "../api";
import { recoveryText } from "../recovery";
import { usePrefs } from "../App";
import { today } from "../charts/RangePicker";
import { nowLocal, parseLocal, toUnit } from "../format";
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
  const [days, setDays] = useState<DayHealth[]>([]);
  const [backupFailed, setBackupFailed] = useState(false);
  const [fatigue, setFatigue] = useState<FatigueReport | null>(null);
  const load = () => {
    api.dayHealth().then(setDays);
    api.fatigue().then(setFatigue);
    api.weighIns(false).then(setRows);
    api.energyBalance(14).then(setEnergy);
    api.currentGoal().then(setGoal);
  };
  useEffect(() => {
    load();
    loadWeighInPlan().then(setPlan);
    Promise.all([api.getSetting("auto_backup"), api.getSetting("last_auto_backup_error")]).then(([on, e]) => setBackupFailed(on !== "off" && !!e));
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
    s && (s.targetKg[0] === s.targetKg[1] ? signed(u(s.targetKg[0]), 2) : `${signed(u(s.targetKg[0]))}…${signed(u(s.targetKg[1]))}`);
  const kcal = (v: number) => Math.round(v).toLocaleString();
  const close = s?.offKcal != null && Math.abs(s.offKcal) <= 100;
  const stale = last && parseLocal(last.at).getTime() < Date.now() - 10 * 86400000;
  const recovery = recoveryText(days, today());
  return (
    <section className="card today-card">
      {backupFailed && (
        <a className="small warn" href="#/data">
          Automatic backup failed. Check it ›
        </a>
      )}
      <div className="today-grid">
        <a className="today-stat" href="#/body/weight" title={stale ? "No weigh-in for over a week" : undefined}>
          <span className={`small ${stale ? "warn" : "muted"}`}>{last ? parseLocal(last.at).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "Weight"}</span>
          <strong>{last ? `${u(last.weight_kg).toFixed(1)} ${unit}` : "—"}</strong>
          {s && (
            <span className="small">
              {s.actualKg != null && (
                <span className={s.onTrack ? "success" : "warn"}>{signed(u(s.actualKg), 2)}/wk</span>
              )}
              <span className="muted">
                {s.actualKg == null ? GOAL_TEXT[s.goal] : " · goal"} {target}
                {s.actualKg == null ? ` ${unit}/wk` : ""}
              </span>
            </span>
          )}
        </a>
        <a className="today-stat" href={goal && !goal.goal ? "#/body/weight" : "#/body/diet"}>
          {goal && !goal.goal ? (
            <>
              <span className="small muted">No goal set</span>
              <strong className="warn">Set your goal ›</strong>
              <span className="small muted">cut, maintain or gain</span>
            </>
          ) : s?.eatKcal != null ? (
            <>
              <span className="small muted">Eat for goal</span>
              <strong>{kcal(s.eatKcal)} kcal</strong>
              <span className={`small ${close ? "success" : "warn"}`}>
                {close ? "on track" : `${signed(Math.round(s.offKcal!), 0)}/day`}
                <span className="muted"> · eating {kcal(s.intakeKcal!)}</span>
              </span>
            </>
          ) : s?.offKcal != null ? (
            <>
              <span className="small muted">Trend vs goal</span>
              <strong className={close ? "success" : "warn"}>
                {close ? "On track" : `${signed(Math.round(s.offKcal), 0)} kcal/d`}
              </strong>
              <span className="small muted">log kcal for a target</span>
            </>
          ) : (
            <>
              <span className="small muted">Calories</span>
              <span className="small muted">needs 3+ weeks of weigh-ins</span>
            </>
          )}
        </a>
        <div className="today-stat">
          <span className={`small ${weighDay && !loggedToday ? "warn" : "muted"}`}>
            {loggedToday ? "Weighed in today ✓" : weighDay ? "Weigh-in day" : "Weigh-in"}
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
      {recovery && <div className="small muted today-recovery">{recovery}</div>}
      {fatigue && fatigue.level !== "ok" && (
        <details className="small today-fatigue">
          <summary>
            <strong className="warn">{fatigue.level === "deload" ? "Time for a lighter week?" : "Running a little flat"}</strong>{" "}
            {fatigue.summary}
          </summary>
          <ul>
            {fatigue.lifts
              .filter((l) => l.down)
              .map((l) => (
                <li key={l.exercise_id}>
                  <a href={`#/exercises/${l.exercise_id}`}>{l.name}</a> <span className="muted">−{Math.abs(l.change * 100).toFixed(1)}%</span>
                </li>
              ))}
            {fatigue.recovery
              .filter((r) => !r.off)
              .map((r) => (
                <li key={r.kind} className="muted">
                  As usual: {r.detail}
                </li>
              ))}
          </ul>
        </details>
      )}
    </section>
  );
}
