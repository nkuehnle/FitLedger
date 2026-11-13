import * as d3 from "d3";
import { useEffect, useMemo, useState } from "react";
import { api, type Goal, type MuscleGrowth, type MuscleHealth } from "../api";
import { BodyMap } from "../charts/BodyMap";
import { useChartTheme, type ChartTheme } from "../charts/theme";
import { GOAL_TEXT } from "../goal";

const SIGNAL_TEXT: Record<MuscleHealth["signals"][number]["kind"], string> = {
  volume: "Volume",
  strength: "Strength",
  size: "Measurements",
  lean_mass: "Lean mass",
};
const EVIDENCE_TEXT: Record<MuscleGrowth["evidence"][number]["kind"], string> = { ...SIGNAL_TEXT, food: "Food" };
const GOAL_MEANS: Record<Goal, string> = {
  cut: "On a cut, holding strength and size counts as success and a shrinking waist is good.",
  maintain: "On maintenance, holding steady counts as success; small gains are a bonus.",
  gain: "On a gain, strength and size should be going up.",
};
const CONFIDENCE_TEXT: Record<MuscleGrowth["confidence"], string> = {
  high: "two or more measures of the muscle agree",
  medium: "one measure of the muscle, or two that disagree",
  low: "only sets and food to go on",
};
/** How strongly a verdict is drawn on the map: an unsure one fades towards gray. */
const CONFIDENCE_STRENGTH: Record<MuscleGrowth["confidence"], number> = { high: 1, medium: 0.7, low: 0.4 };

/** Diverging: red (struggling) – gray (steady) – blue (thriving), the app's validated pair. */
function healthScale(theme: ChartTheme) {
  const mid = theme.mode === "dark" ? "#4a505a" : "#d5d9e0";
  return d3
    .scaleLinear<string>()
    .domain([-1, 0, 1])
    .range([theme.categorical[7], mid, theme.categorical[0]])
    .interpolate(d3.interpolateLab)
    .clamp(true);
}

const label = (s: number) => (s >= 0.6 ? "thriving" : s >= 0.25 ? "on track" : s > -0.3 ? "needs work" : "struggling");
const leanText = (v: number) => (v >= 0.2 ? "points to growth" : v <= -0.2 ? "points to loss" : "flat");
const arrow = (v: number) => (v >= 0.2 ? "↑" : v <= -0.2 ? "↓" : "→");

/** "Strength up, measurements down: they disagree" for a muscle whose outcomes point opposite ways. */
function disagreement(g: MuscleGrowth): string {
  const outcomes = g.evidence.filter((e) => e.outcome);
  const names = (es: typeof outcomes) => es.map((e) => EVIDENCE_TEXT[e.kind].toLowerCase()).join(" and ");
  const text = `${names(outcomes.filter((e) => e.lean >= 0.2))} up, ${names(outcomes.filter((e) => e.lean <= -0.2))} down: they disagree`;
  return text[0].toUpperCase() + text.slice(1);
}

/**
 * Body map of how each muscle is doing, from whatever data exists: against the current goal
 * ("For your goal"), or whether it's likely growing regardless of the goal ("Growth").
 */
export function ProgressHealth({ names }: { names: Record<string, string> }) {
  const theme = useChartTheme();
  const [health, setHealth] = useState<MuscleHealth[]>([]);
  const [goal, setGoal] = useState<Goal | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<"goal" | "growth">("goal");
  useEffect(() => {
    api.progressHealth().then(setHealth);
    api.currentGoal().then((g) => setGoal(g.goal));
  }, []);
  const scale = useMemo(() => healthScale(theme), [theme]);
  const growthFill = (g: MuscleGrowth) => scale(g.score * CONFIDENCE_STRENGTH[g.confidence]);
  const shown = health.filter((h) => (view === "goal" ? h.score != null : h.growth != null));
  const scoreOf = (h: MuscleHealth) => (view === "goal" ? h.score! : h.growth!.score);
  const fills = Object.fromEntries(shown.map((h) => [h.muscle_id, view === "goal" ? scale(h.score!) : growthFill(h.growth!)]));
  const values = Object.fromEntries(health.map((h) => [h.muscle_id, (view === "goal" ? h.score : h.growth?.score) ?? 0]));
  const sel = health.find((h) => h.muscle_id === selected);
  const ranked = [...shown].sort((a, b) => scoreOf(a) - scoreOf(b));

  const legend = (
    <div className="load-legend small muted">
      <span
        className="load-ramp"
        style={{ background: `linear-gradient(to right, ${[-1, -0.5, 0, 0.5, 1].map((v) => scale(v)).join(",")})` }}
      />
      <div className="health-labels">
        <span>{view === "goal" ? "struggling" : "slipping"}</span>
        <span>{view === "goal" ? "steady" : "holding"}</span>
        <span>{view === "goal" ? "thriving" : "growing"}</span>
      </div>
    </div>
  );

  return (
    <div>
      <div className="segmented small">
        <button className={view === "goal" ? "active" : ""} onClick={() => setView("goal")}>
          For your goal
        </button>
        <button className={view === "growth" ? "active" : ""} onClick={() => setView("growth")}>
          Growth
        </button>
      </div>
      {view === "goal" ? (
        <p className="muted small">
          How each muscle is doing{goal ? ` for your goal (${GOAL_TEXT[goal]})` : ""}: weekly volume against target, strength trend, and
          tape measurements and InBody lean mass when you have them. Based on the last 4–12 weeks, whatever range is picked above.{" "}
          {goal && GOAL_MEANS[goal]}
        </p>
      ) : (
        <p className="muted small">
          Whether each muscle is likely growing, holding or slipping, whatever your goal. Strength in the same rep range, the tape and
          InBody lean mass say what the muscle did; weekly sets and food say whether it had what it needs. Fainter colors are less certain.
          Based on the last 4–12 weeks, whatever range is picked above.
        </p>
      )}
      <div className="map-with-detail">
        <BodyMap
          values={values}
          fills={fills}
          legend={legend}
          selected={selected}
          onSelect={setSelected}
          muscleNames={names}
          height={360}
          hideNumbers
        />
        <div className="muscle-detail">
          {sel ? (
            <div>
              <h3 style={{ margin: "0 0 4px" }}>{names[sel.muscle_id] ?? sel.muscle_id}</h3>
              {view === "goal" ? (
                <>
                  {sel.score == null ? (
                    <p className="muted small">No data yet.</p>
                  ) : (
                    <p className="small">
                      Overall: <strong>{label(sel.score)}</strong>
                    </p>
                  )}
                  <ul className="health-signals small">
                    {sel.signals.map((s) => (
                      <li key={s.kind}>
                        <span className="health-dot" style={{ background: scale(s.score) }} />
                        <span>
                          <strong>{SIGNAL_TEXT[s.kind]}</strong> · {label(s.score)}
                          <br />
                          <span className="muted">{s.detail}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                </>
              ) : sel.growth == null ? (
                <p className="muted small">No data yet.</p>
              ) : (
                <>
                  <p className="small">
                    Likely <strong>{sel.growth.verdict}</strong> · {sel.growth.confidence} confidence
                    <br />
                    <span className="muted">
                      {sel.growth.agree
                        ? CONFIDENCE_TEXT[sel.growth.confidence][0].toUpperCase() + CONFIDENCE_TEXT[sel.growth.confidence].slice(1)
                        : disagreement(sel.growth)}
                      .
                    </span>
                  </p>
                  <ul className="health-signals small">
                    {sel.growth.evidence.map((e) => (
                      <li key={e.kind}>
                        <span className="health-dot" style={{ background: scale(e.lean) }} />
                        <span>
                          <strong>{EVIDENCE_TEXT[e.kind]}</strong> {arrow(e.lean)} {leanText(e.lean)}
                          <br />
                          <span className="muted">{e.detail}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              <button className="link-btn small" onClick={() => setSelected(null)}>
                All muscles
              </button>
            </div>
          ) : (
            <>
              <p className="muted small">Tap a muscle to see why. {view === "goal" ? "Needs attention first:" : "Slipping first:"}</p>
              <ul className="days-since small">
                {ranked.map((h) => (
                  <li key={h.muscle_id} onClick={() => setSelected(h.muscle_id)}>
                    <span>
                      <span className="health-dot" style={{ background: fills[h.muscle_id] }} /> {names[h.muscle_id] ?? h.muscle_id}
                    </span>
                    <span className="muted">{view === "goal" ? label(h.score!) : h.growth!.verdict}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
