import type { Phase } from "../api";

export interface Range {
  key: string;
  from: string | null; // YYYY-MM-DD inclusive
  to: string | null;
  label: string;
}

export function isoDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function today(): string {
  return isoDate(new Date());
}

function daysAgo(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return isoDate(d);
}

export function weekStart(d = new Date()): string {
  const x = new Date(d);
  x.setDate(x.getDate() - ((x.getDay() + 6) % 7));
  return isoDate(x);
}

export const PRESETS: Range[] = [
  { key: "week", from: weekStart(), to: null, label: "This week" },
  { key: "4w", from: daysAgo(27), to: null, label: "4 weeks" },
  { key: "3m", from: daysAgo(91), to: null, label: "3 months" },
  { key: "6m", from: daysAgo(182), to: null, label: "6 months" },
  { key: "1y", from: daysAgo(365), to: null, label: "1 year" },
  { key: "all", from: null, to: null, label: "All time" },
];

export function presetRange(key: string): Range {
  return PRESETS.find((p) => p.key === key) ?? PRESETS[PRESETS.length - 1];
}

export function phaseRange(p: Phase): Range {
  return { key: `phase-${p.id}`, from: p.start_date, to: p.end_date, label: p.name };
}

export function inRange(dateStr: string, r: Range): boolean {
  const d = dateStr.slice(0, 10);
  return (!r.from || d >= r.from) && (!r.to || d <= r.to);
}

/** One row of range chips (presets, phases, custom), shared by every chart page. */
export function RangePicker({
  value,
  onChange,
  phases = [],
  presets = ["4w", "3m", "6m", "1y", "all"],
}: {
  value: Range;
  onChange: (r: Range) => void;
  phases?: Phase[];
  presets?: string[];
}) {
  const custom = value.key === "custom";
  return (
    <div className="range-picker">
      <div className="chips-row">
        {presets.map((k) => {
          const p = presetRange(k);
          return (
            <button key={k} className={value.key === k ? "chip-btn active" : "chip-btn"} onClick={() => onChange(p)}>
              {p.label}
            </button>
          );
        })}
        {phases.map((p) => (
          <button
            key={p.id}
            className={value.key === `phase-${p.id}` ? "chip-btn active" : "chip-btn"}
            onClick={() => onChange(phaseRange(p))}
          >
            {p.name}
          </button>
        ))}
        <button
          className={custom ? "chip-btn active" : "chip-btn"}
          onClick={() => onChange({ key: "custom", from: value.from ?? daysAgo(90), to: value.to, label: "Custom" })}
        >
          Custom…
        </button>
      </div>
      {custom && (
        <div className="row small">
          <label>
            From <input type="date" value={value.from ?? ""} onChange={(e) => onChange({ ...value, from: e.target.value || null })} />
          </label>
          <label>
            To <input type="date" value={value.to ?? ""} onChange={(e) => onChange({ ...value, to: e.target.value || null })} />
          </label>
        </div>
      )}
    </div>
  );
}
