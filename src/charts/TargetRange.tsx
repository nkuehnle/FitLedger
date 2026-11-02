import type { MuscleTarget } from "../api";

/**
 * Weekly sets vs target, one row per muscle: a grey band for the target range and one dot for
 * the actual value on a shared scale. Being outside the range is said in words, not color.
 */
export function TargetRange({
  rows,
  value,
  names,
}: {
  rows: MuscleTarget[];
  value: (muscle: string) => number;
  names: Record<string, string>;
}) {
  const top = Math.max(20, ...rows.map((t) => Math.max(t.max_sets, value(t.muscle_id))));
  const max = Math.ceil(top / 5) * 5;
  const pct = (v: number) => `${(Math.min(v, max) / max) * 100}%`;
  const ticks = Array.from({ length: max / 5 + 1 }, (_, i) => i * 5);
  return (
    <div className="target-range small">
      <div className="tr-legend muted">
        <span>
          <i className="tr-key-band" /> target range
        </span>
        <span>
          <i className="tr-key-dot" /> your sets per week
        </span>
      </div>
      <div className="tr-row tr-axis muted">
        <span />
        <span className="tr-track">
          {ticks.map((t) => (
            <span key={t} className="tr-tick" style={{ left: pct(t) }}>
              {t}
            </span>
          ))}
        </span>
        <span />
      </div>
      {rows.map((t) => {
        const v = value(t.muscle_id);
        const diff = v < t.min_sets ? t.min_sets - v : v > t.max_sets ? v - t.max_sets : 0;
        const note = diff === 0 ? "" : `${Math.round(diff * 10) / 10} ${v < t.min_sets ? "under" : "over"}`;
        return (
          <div
            key={t.muscle_id}
            className="tr-row"
            title={`${names[t.muscle_id]}: ${Math.round(v * 10) / 10} sets/week, target ${t.min_sets}–${t.max_sets}`}
          >
            <span className="tr-name">{names[t.muscle_id] ?? t.muscle_id}</span>
            <span className="tr-track">
              {ticks.map((x) => (
                <span key={x} className="tr-grid" style={{ left: pct(x) }} />
              ))}
              <span className="tr-band" style={{ left: pct(t.min_sets), width: `calc(${pct(t.max_sets)} - ${pct(t.min_sets)})` }} />
              <span className="tr-dot" style={{ left: pct(v) }} />
            </span>
            <span className="tr-value">
              {Math.round(v * 10) / 10}
              {note && <span className="tr-note"> · {note}</span>}
            </span>
          </div>
        );
      })}
    </div>
  );
}
