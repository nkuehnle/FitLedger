import { useEffect, useState } from "react";
import { api, type StrengthBalance } from "../api";
import { useChartTheme } from "../charts/theme";
import { formatDate } from "../format";
import { ProfileHint } from "./Standing";

const pct = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(Math.round(v * 100))}%`;
/** Bars run from the middle; this far either way fills a side. */
const FULL = 0.4;

/** A bar from the centre line: left of it is behind your average, right of it ahead. */
function BalanceBar({ value, notable }: { value: number; notable: boolean }) {
  const theme = useChartTheme();
  const width = `${(Math.min(Math.abs(value), FULL) / FULL) * 50}%`;
  const color = !notable ? theme.muted : value < 0 ? theme.categorical[7] : theme.categorical[0];
  return (
    <span className="balance-track">
      <span className="balance-fill" style={{ width, background: color, [value < 0 ? "right" : "left"]: "50%" }} />
    </span>
  );
}

/** Progress → Strength balance: muscles and movements against your own average, and left against right. */
export function StrengthBalanceSection({ names }: { names: Record<string, string> }) {
  const [b, setB] = useState<StrengthBalance | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  useEffect(() => {
    api.strengthBalance().then(setB);
  }, []);
  if (!b) return null;
  const muscles = [...b.muscles].sort((x, y) => x.balance - y.balance);
  const behind = muscles.filter((m) => m.standing === "behind").map((m) => names[m.muscle_id] ?? m.muscle_id);
  return (
    <section className="card span-2">
      <h2 className="section-title">Strength balance</h2>
      {b.missing === "profile" && <ProfileHint />}
      {b.missing === "bodyweight" && <p className="muted small">Log your bodyweight to see how your lifts compare with each other.</p>}
      {b.missing === "lifts" && (
        <p className="muted small">
          This needs three or more exercises from the last 3 months that have comparison data (most barbell, dumbbell and common machine
          lifts do).
        </p>
      )}
      {!b.missing && (
        <>
          <p className="muted small">
            Which muscles are strong or weak for <em>you</em>. Each lift from the last 3 months is measured against the same yardstick (the
            median competitor at your sex, age and bodyweight), then each muscle is set against the middle of all your lifts.{" "}
            {behind.length > 0 ? `Furthest behind: ${behind.slice(0, 3).join(", ")}.` : "Nothing is more than 10% behind the rest."}
          </p>
          <div className="balance-scale small muted">
            <span>behind</span>
            <span>your average</span>
            <span>ahead</span>
          </div>
          <ul className="balance-rows small">
            {muscles.map((m) => (
              <li key={m.muscle_id}>
                <button className="balance-row" onClick={() => setOpen(open === m.muscle_id ? null : m.muscle_id)}>
                  <span>{names[m.muscle_id] ?? m.muscle_id}</span>
                  <BalanceBar value={m.balance} notable={m.standing !== "even"} />
                  <span className="num-cell">{pct(m.balance)}</span>
                </button>
                {open === m.muscle_id && <div className="muted balance-lifts">From {m.lifts.join(", ")}</div>}
              </li>
            ))}
          </ul>
          {b.pairs.length > 0 && (
            <>
              <div className="section-label">Movements</div>
              <ul className="balance-rows small">
                {b.pairs.flatMap((p) =>
                  (
                    [
                      [p.a, p.a_balance, p.a_lifts],
                      [p.b, p.b_balance, p.b_lifts],
                    ] as const
                  ).map(([label, value, n], i) => (
                    <li key={`${p.a}-${label}`} className={i === 1 ? "pair-end" : ""}>
                      <span className="balance-row">
                        <span>
                          {label} <span className="muted">({n})</span>
                        </span>
                        <BalanceBar value={value} notable={Math.abs(value) >= 0.1} />
                        <span className="num-cell">{pct(value)}</span>
                      </span>
                    </li>
                  )),
                )}
              </ul>
            </>
          )}
          <details>
            <summary className="small muted">Each lift ({b.lifts.length})</summary>
            <ul className="balance-rows small">
              {b.lifts.map((l) => (
                <li key={l.exercise_id}>
                  <a className="balance-row" href={`#/exercises/${l.exercise_id}`}>
                    <span>
                      {l.name}
                      {l.rough && <span className="muted"> ≈</span>}
                    </span>
                    <BalanceBar value={l.balance} notable={Math.abs(l.balance) >= 0.1} />
                    <span className="num-cell">{pct(l.balance)}</span>
                  </a>
                </li>
              ))}
            </ul>
            <p className="muted small">
              ≈ marks a rough comparison. Those count half, machine and cable lifts half again (machines differ from gym to gym), and no
              single lift can move a muscle by more than 50%.
            </p>
          </details>
        </>
      )}
      {b.sides.length > 0 && (
        <>
          <div className="section-label">Left and right</div>
          <ul className="days-since small" style={{ columns: 1 }}>
            {b.sides.map((s) => (
              <li key={s.label} style={{ cursor: "default" }}>
                <span>
                  {s.label}{" "}
                  <span className="muted">
                    · {s.source === "tape" ? "tape" : "InBody"}, {formatDate(s.date)}
                  </span>
                </span>
                <span className={s.notable ? "warn" : "muted"}>
                  {Math.abs(s.diff) < 0.005 ? "even" : `${s.diff > 0 ? "right" : "left"} ${(Math.abs(s.diff) * 100).toFixed(1)}% bigger`}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
