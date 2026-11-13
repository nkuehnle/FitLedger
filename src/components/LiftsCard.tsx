import { useEffect, useState } from "react";
import { api, type LiftGoal, type LiftStatus, type LiftsView } from "../api";
import { navigate, usePrefs } from "../App";
import { formatDate, formatWeight } from "../format";

const SHOWN = 6;
const STATE_LABELS: Record<LiftStatus["state"], string> = { moving: "moving", flat: "flat", sliding: "sliding", check: "check the data" };
const RATINGS = { up: "↑", hold: "=", down: "↓" };

const percent = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v * 100).toFixed(1)}%`;

export function goalText(g: LiftGoal, unit: "kg" | "lb"): string {
  const target = `${formatWeight(g.target_kg, unit)} ${unit}`;
  if (g.state === "reached") return `Goal of ${target} reached`;
  const by = g.projected ? `trend reaches it around ${formatDate(g.projected)}` : "no clear trend yet";
  if (g.state === "open") return `Goal ${target}: ${by}`;
  return `Goal ${target}: ${g.state === "on_pace" ? "on pace" : "behind"}, ${by}`;
}

/** What's moving and what isn't, in one short row per lift. A tap shows the detail. */
export function LiftsCard() {
  const { unit } = usePrefs();
  const [view, setView] = useState<LiftsView | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [all, setAll] = useState(false);
  useEffect(() => {
    api.liftStatus().then(setView);
  }, []);
  if (!view || (view.lifts.length === 0 && view.left_out === 0)) return null;
  const lifts = all ? view.lifts : view.lifts.slice(0, SHOWN);
  return (
    <section className="card span-2">
      <h2 className="section-title">Lifts · last 6 weeks vs the 6 before</h2>
      {lifts.map((l) => (
        <div key={l.exercise_id} className="lift-row">
          <button
            className="lift-head"
            aria-expanded={open === l.exercise_id}
            onClick={() => setOpen(open === l.exercise_id ? null : l.exercise_id)}
          >
            <span className="lift-name">{l.name}</span>
            <span className={`small lift-state state-${l.state}`}>{STATE_LABELS[l.state]}</span>
            <span className="muted small">{percent(l.change)} ›</span>
          </button>
          {open === l.exercise_id && (
            <div className="lift-detail small">
              <div className="muted">
                {formatDate(l.last_at)}: {l.last_sets.map(([w, r]) => `${formatWeight(w, unit)} × ${r}`).join(", ")}
                {l.rating && ` · you rated ${RATINGS[l.rating]}`}
              </div>
              {l.goal && <div>{goalText(l.goal, unit)}</div>}
              {l.suggestion && <div>{l.suggestion}</div>}
              <div className="row">
                <button className="link-btn" onClick={() => navigate(`/exercises/${l.exercise_id}`)}>
                  Open exercise
                </button>
                {l.suggestion && l.rating !== "hold" && l.rating !== "down" && (
                  <button
                    className="link-btn"
                    onClick={() =>
                      api
                        .dismissLift(l.exercise_id)
                        .then(() => api.liftStatus())
                        .then(setView)
                    }
                  >
                    Looks fine to me
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      ))}
      {view.lifts.length > SHOWN && (
        <button className="link-btn" onClick={() => setAll(!all)}>
          {all ? "Show fewer" : `Show all ${view.lifts.length}`}
        </button>
      )}
      {view.left_out > 0 && (
        <p className="muted small">
          {view.left_out} other {view.left_out === 1 ? "exercise isn't" : "exercises aren't"} shown: too few sessions, or not weight and
          reps.
        </p>
      )}
    </section>
  );
}
