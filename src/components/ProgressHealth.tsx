import * as d3 from "d3";
import { useEffect, useMemo, useState } from "react";
import { api, type Goal, type MuscleHealth } from "../api";
import { BodyMap } from "../charts/BodyMap";
import { useChartTheme, type ChartTheme } from "../charts/theme";
import { GOAL_TEXT } from "../goal";

const SIGNAL_TEXT: Record<MuscleHealth["signals"][number]["kind"], string> = {
  volume: "Volume",
  strength: "Strength",
  size: "Measurements",
  lean_mass: "Lean mass",
};
const GOAL_MEANS: Record<Goal, string> = {
  cut: "On a cut, holding strength and size counts as success and a shrinking waist is good.",
  maintain: "On maintenance, holding steady counts as success; small gains are a bonus.",
  gain: "On a gain, strength and size should be going up.",
};

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

/** Body map of how each muscle is progressing for the current goal, from whatever data exists. */
export function ProgressHealth({ names }: { names: Record<string, string> }) {
  const theme = useChartTheme();
  const [health, setHealth] = useState<MuscleHealth[]>([]);
  const [goal, setGoal] = useState<Goal | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    api.progressHealth().then(setHealth);
    api.currentGoal().then((g) => setGoal(g.goal));
  }, []);
  const scale = useMemo(() => healthScale(theme), [theme]);
  const fills = Object.fromEntries(health.filter((h) => h.score != null).map((h) => [h.muscle_id, scale(h.score!)]));
  const values = Object.fromEntries(health.map((h) => [h.muscle_id, h.score ?? 0]));
  const sel = health.find((h) => h.muscle_id === selected);
  const ranked = health.filter((h) => h.score != null).sort((a, b) => a.score! - b.score!);

  const legend = (
    <div className="load-legend small muted">
      <span
        className="load-ramp"
        style={{ background: `linear-gradient(to right, ${[-1, -0.5, 0, 0.5, 1].map((v) => scale(v)).join(",")})` }}
      />
      <div className="health-labels">
        <span>struggling</span>
        <span>steady</span>
        <span>thriving</span>
      </div>
    </div>
  );

  return (
    <div>
      <p className="muted small">
        How each muscle is doing{goal ? ` for your goal (${GOAL_TEXT[goal]})` : ""}: weekly volume against target, strength trend, and tape
        measurements and InBody lean mass when you have them. Based on the last 4–12 weeks, whatever range is picked above.{" "}
        {goal && GOAL_MEANS[goal]}
      </p>
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
              <button className="link-btn small" onClick={() => setSelected(null)}>
                All muscles
              </button>
            </div>
          ) : (
            <>
              <p className="muted small">Tap a muscle to see why. Needs attention first:</p>
              <ul className="days-since small">
                {ranked.map((h) => (
                  <li key={h.muscle_id} onClick={() => setSelected(h.muscle_id)}>
                    <span>
                      <span className="health-dot" style={{ background: scale(h.score!) }} /> {names[h.muscle_id] ?? h.muscle_id}
                    </span>
                    <span className="muted">{label(h.score!)}</span>
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
