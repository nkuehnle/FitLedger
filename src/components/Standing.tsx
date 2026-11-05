import { useEffect, useState } from "react";
import { api, type NormReference, type NormWho } from "../api";

/** Profile settings used for comparisons: `who` is null until sex and birth year are set. */
export function useNormProfile(bodyweightKg: number | null = null): { who: NormWho | null; heightCm: number | null; loaded: boolean } {
  const [p, setP] = useState<(string | null)[] | null>(null);
  useEffect(() => {
    Promise.all(["sex", "birth_year", "norm_group", "height_cm"].map((k) => api.getSetting(k))).then(setP);
  }, []);
  if (!p) return { who: null, heightCm: null, loaded: false };
  const [sex, birth, group, height] = p;
  return { who: whoOf(sex, birth, group, bodyweightKg), heightCm: Number(height) || null, loaded: true };
}

/** Shown where comparisons would be, when the profile isn't set. */
export function ProfileHint() {
  return (
    <p className="muted small">
      Set sex, height and birth year under <a href="#/body/inbody">Measure → Body comp</a> to see how you compare with other adults.
    </p>
  );
}

export function whoOf(sex: string | null, birthYear: string | null, group: string | null, bodyweightKg: number | null): NormWho | null {
  const year = Number(birthYear);
  if ((sex !== "male" && sex !== "female") || !year) return null;
  return { sex, age: new Date().getFullYear() - year, group: group || null, bodyweight_kg: bodyweightKg };
}

export const ordinal = (n: number) => {
  const r = Math.round(n);
  const s = r % 100 >= 11 && r % 100 <= 13 ? "th" : (({ 1: "st", 2: "nd", 3: "rd" } as Record<number, string>)[r % 10] ?? "th");
  return `${r}${s}`;
};

/**
 * Where a value stands against reference data: the percentile (and tier, for lifts) and a strip
 * showing the spread of the reference group, with the first value (hollow) and the latest (filled).
 * `values` are in storage units (kg, cm), oldest first; `show` formats one for display.
 */
export function Standing({
  metric,
  label,
  values,
  who,
  show,
  note,
}: {
  metric: string;
  label: string;
  values: number[];
  who: NormWho;
  show: (v: number) => string;
  note?: string;
}) {
  const [ref, setRef] = useState<NormReference | null>(null);
  const [open, setOpen] = useState(false);
  const key = JSON.stringify([metric, who, values]);
  useEffect(() => {
    api.normReference(metric, who, values).then(setRef);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  if (!ref || !values.length) return null;

  const latest = values[values.length - 1];
  const first = values.length > 1 ? values[0] : null;
  const rank = ref.ranks[ref.ranks.length - 1];
  const tier = ref.tier_of[ref.tier_of.length - 1];
  const [pLo, pHi] = [ref.points[0][0], ref.points[ref.points.length - 1][0]];
  const rankText =
    rank == null
      ? ""
      : rank <= pLo
        ? `below ${ordinal(pLo)} percentile`
        : rank >= pHi
          ? `above ${ordinal(pHi)} percentile`
          : `${ordinal(rank)} percentile`;

  // Scale: the reference spread (tiers: from well below Novice to above Elite), widened to fit your values.
  const tiers = ref.tiers;
  let lo = tiers.length ? tiers[1][1] * 0.6 : ref.points[0][1];
  let hi = tiers.length ? tiers[tiers.length - 1][1] * 1.15 : ref.points[ref.points.length - 1][1];
  for (const v of values) {
    lo = Math.min(lo, v);
    hi = Math.max(hi, v);
  }
  const pad = (hi - lo) * 0.04;
  lo -= pad;
  hi += pad;
  const W = 320;
  const x = (v: number) => 6 + ((v - lo) / (hi - lo)) * (W - 12);
  const bar = { y: 16, h: 12 };
  const mid = bar.y + bar.h / 2;

  const segments: { from: number; to: number; opacity: number; name?: string }[] = tiers.length
    ? tiers.map(([name, from], i) => ({
        from: i === 0 ? lo : from,
        to: i + 1 < tiers.length ? tiers[i + 1][1] : hi,
        opacity: 0.1 + 0.16 * i,
        name,
      }))
    : [
        { from: lo, to: ref.points[0][1], opacity: 0.06 },
        ...ref.points.slice(1).map(([p, v], i) => {
          const [p0, v0] = ref.points[i];
          return { from: v0, to: v, opacity: 0.12 + 0.4 * (1 - Math.abs((p + p0) / 2 - 50) / 50) };
        }),
        { from: ref.points[ref.points.length - 1][1], to: hi, opacity: 0.06 },
      ];
  const ticks: { v: number; top: string; bottom: string }[] = tiers.length
    ? []
    : ref.points
        .filter(([p]) => p === 5 || p === 50 || p === 95)
        .map(([p, v]) => ({ v, top: p === 50 ? "median" : ordinal(p), bottom: show(v) }));

  return (
    <div className="standing">
      <div className="standing-head">
        <span>{label}</span>
        <span>
          {tier != null && <strong>{tiers[tier][0]} · </strong>}
          <span className={tier != null ? "muted" : ""}>{tier != null ? rankText : <strong>{rankText}</strong>}</span>
          <button className="icon-btn standing-info" aria-label="About this comparison" aria-expanded={open} onClick={() => setOpen(!open)}>
            ⓘ
          </button>
        </span>
      </div>
      <svg className="standing-strip" viewBox={`0 0 ${W} 46`} role="img" aria-label={`${label}: ${show(latest)}, ${rankText}`}>
        <rect x={6} y={bar.y} width={W - 12} height={bar.h} rx={bar.h / 2} fill="var(--surface-2)" />
        {segments.map((s, i) => (
          <rect
            key={i}
            x={x(s.from)}
            y={bar.y}
            width={Math.max(0, x(s.to) - x(s.from))}
            height={bar.h}
            fill="var(--accent)"
            opacity={s.opacity}
          />
        ))}
        {segments.map(
          (s, i) =>
            s.name &&
            x(s.to) - x(s.from) > 44 && (
              <text key={`n${i}`} x={(x(s.from) + x(s.to)) / 2} y={42} textAnchor="middle" className="standing-label">
                {s.name}
              </text>
            ),
        )}
        {tiers.slice(1).map(([name, from]) => (
          <text key={name} x={x(from)} y={10} textAnchor="middle" className="standing-label">
            {show(from)}
          </text>
        ))}
        {ticks.map((t) => (
          <g key={t.top}>
            <line x1={x(t.v)} x2={x(t.v)} y1={bar.y - 2} y2={bar.y + bar.h + 2} stroke="var(--muted)" strokeWidth={1} />
            <text x={x(t.v)} y={10} textAnchor="middle" className="standing-label">
              {t.top}
            </text>
            <text x={x(t.v)} y={42} textAnchor="middle" className="standing-label">
              {t.bottom}
            </text>
          </g>
        ))}
        {first != null && (
          <>
            <line x1={x(first)} x2={x(latest)} y1={mid} y2={mid} stroke="var(--text)" strokeWidth={1.5} opacity={0.5} />
            <circle cx={x(first)} cy={mid} r={4} fill="var(--surface)" stroke="var(--text)" strokeWidth={1.5} opacity={0.7} />
          </>
        )}
        <circle cx={x(latest)} cy={mid} r={6.5} fill="var(--accent)" stroke="var(--surface)" strokeWidth={2} />
      </svg>
      {open && (
        <p className="muted small">
          You: {show(latest)}
          {first != null && ` (first: ${show(first)}, hollow dot)`}. Compared with {ref.population}.{" "}
          {tiers.length > 0 &&
            "Tiers: Novice from half the median competitor, Intermediate from the 10th percentile, Advanced from the median, Elite from the 90th. "}
          {note && `${note} `}Source: {ref.source}.
        </p>
      )}
    </div>
  );
}
