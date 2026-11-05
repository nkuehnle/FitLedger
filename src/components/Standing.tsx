import { useEffect, useState } from "react";
import { api, type NormReference, type NormWho } from "../api";
import { usePrefs } from "../App";
import { parseLocal, toUnit } from "../format";

/** Profile settings used for comparisons: `who` is null until sex and birth year are set. */
export function useNormProfile(bodyweightKg: number | null = null): { who: NormWho | null; heightCm: number | null; loaded: boolean } {
  const [p, setP] = useState<(string | null)[] | null>(null);
  useEffect(() => {
    Promise.all(["sex", "birth_year", "height_cm"].map((k) => api.getSetting(k))).then(setP);
  }, []);
  if (!p) return { who: null, heightCm: null, loaded: false };
  const [sex, birth, height] = p;
  return { who: whoOf(sex, birth, bodyweightKg), heightCm: Number(height) || null, loaded: true };
}

/** Shown where comparisons would be, when the profile isn't set. */
export function ProfileHint() {
  return (
    <p className="muted small">
      Set sex, height and birth year under <a href="#/body/inbody">Measure → Body comp</a> to see how you compare with other adults.
    </p>
  );
}

export function whoOf(sex: string | null, birthYear: string | null, bodyweightKg: number | null): NormWho | null {
  const year = Number(birthYear);
  if ((sex !== "male" && sex !== "female") || !year) return null;
  return { sex, age: new Date().getFullYear() - year, bodyweight_kg: bodyweightKg };
}

/** Which of these exercise slugs have a comparison (see crates/core/src/norms.rs `links`). */
export function useComparable(slugs: string[]): Set<string> {
  const [set, setSet] = useState<Set<string>>(new Set());
  const key = slugs.join();
  useEffect(() => {
    if (slugs.length) api.comparableExercises(slugs).then((s) => setSet(new Set(s)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return set;
}

export const ordinal = (n: number) => {
  const r = Math.round(n);
  const s = r % 100 >= 11 && r % 100 <= 13 ? "th" : (({ 1: "st", 2: "nd", 3: "rd" } as Record<number, string>)[r % 10] ?? "th");
  return `${r}${s}`;
};

interface StandingProps {
  label: string;
  values: number[];
  show: (v: number) => string;
  note?: string;
}

/** A body measure against its reference data (fetched); see `StandingView`. */
export function Standing({ metric, who, ...props }: StandingProps & { metric: string; who: NormWho }) {
  const [ref, setRef] = useState<NormReference | null>(null);
  const key = JSON.stringify([metric, who, props.values]);
  useEffect(() => {
    api.normReference(metric, who, props.values).then(setRef);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return ref ? <StandingView reference={ref} {...props} /> : null;
}

/**
 * Where a value stands against reference data: the percentile (and tier, for lifts) and a strip
 * showing the spread of the reference group, with the first value (hollow) and the latest (filled).
 * `values` are in storage units (kg, cm, reps), oldest first; `show` formats one for display.
 */
export function StandingView({ reference: ref, label, values, show, note }: StandingProps & { reference: NormReference }) {
  const [open, setOpen] = useState(false);
  if (!values.length) return null;

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
          {ref.conversion && `${ref.conversion} `}
          {note && `${note} `}Source: {ref.source}.
        </p>
      )}
    </div>
  );
}

const ONE_DUMBBELL = new Set([
  "bent-over-one-arm-row-dumbbell",
  "concentration-curl-dumbbell",
  "pullover-dumbbell",
  "overhead-triceps-extension-dumbbell",
  "single-leg-romanian-deadlift-dumbbell",
  "goblet-squat-dumbbell",
]);

/**
 * How an exercise compares: every reference group with data (competitors, CrossFit athletes),
 * converted through a published or rough ratio for other exercises. `weight` are estimated
 * 1RMs in kg and `reps` most reps, oldest first.
 */
export function ExerciseComparisons({
  slug,
  who,
  weight,
  reps,
  label,
  note,
  perHand = false,
  dumbbell = false,
}: {
  slug: string;
  who: NormWho;
  weight: number[];
  reps: number[];
  /** Prefix for each strip's label (e.g. the exercise name in a workout summary). */
  label?: string;
  note?: string;
  /** Weights logged per dumbbell: compared (and shown) as both together, as the studies measure. */
  perHand?: boolean;
  /** A dumbbell exercise: say which way its weights were read. */
  dumbbell?: boolean;
}) {
  // Exercises done with one dumbbell are compared at that dumbbell's weight.
  if (ONE_DUMBBELL.has(slug)) {
    perHand = false;
    dumbbell = false;
  }
  weight = perHand ? weight.map((w) => w * 2) : weight;
  if (dumbbell)
    note = `${note ?? ""} ${
      perHand
        ? "Your weights are per dumbbell, so both together are compared."
        : "Your weights are read as both dumbbells together (this exercise is set to total load); if you log one dumbbell's weight, turn on “weights per hand” under Edit on the exercise."
    }`.trim();
  const { unit } = usePrefs();
  const [refs, setRefs] = useState<NormReference[] | null>(null);
  const key = JSON.stringify([slug, who, weight, reps]);
  useEffect(() => {
    api.exerciseComparison(slug, who, weight, reps).then(setRefs);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  if (!refs) return null;
  const showKg = (kg: number) => `${Math.round(toUnit(kg, unit))} ${unit}`;
  return (
    <>
      {refs.map((r) => {
        const reps_ = r.unit === "reps";
        const vals = reps_ ? reps : weight;
        const show = reps_ ? (v: number) => `${Math.round(v)} reps` : perHand ? (v: number) => `${showKg(v)} (both)` : showKg;
        const head = `${label ? `${label}: ${show(vals[vals.length - 1])} · ` : ""}vs ${r.title}${r.rough ? " (rough estimate)" : r.conversion ? " (estimated)" : ""}`;
        return <StandingView key={r.title} reference={r} label={head} values={vals} show={show} note={note} />;
      })}
    </>
  );
}

/**
 * Comparisons for the exercises in one workout that have reference data: that session's best
 * estimated 1RM (or most reps), at your bodyweight nearest the workout.
 */
export function WorkoutLifts({
  workoutId,
  at,
  exercises,
}: {
  workoutId: number;
  at: string;
  exercises: { exercise_id: number | null; exercise_slug: string | null; name: string; load_mode: string; equipment: string | null }[];
}) {
  const { unit } = usePrefs();
  const unique = exercises.filter((e, i) => e.exercise_id && e.exercise_slug && exercises.findIndex((x) => x.exercise_id === e.exercise_id) === i);
  const comparable = useComparable(unique.map((e) => e.exercise_slug!));
  const lifts = unique.filter((e) => comparable.has(e.exercise_slug!));
  const [rows, setRows] = useState<{ id: number; slug: string; name: string; e1rm: number[]; reps: number[]; perHand: boolean; dumbbell: boolean }[]>([]);
  const [bw, setBw] = useState<number | null>(null);
  const key = lifts.map((e) => e.exercise_id).join();
  useEffect(() => {
    if (!lifts.length) return;
    Promise.all(lifts.map((e) => api.exerciseStats(e.exercise_id!))).then((stats) =>
      setRows(
        lifts.flatMap((e, i) => {
          const s = stats[i].sessions.find((x) => x.workout_id === workoutId);
          if (!s) return [];
          const reps = Math.max(0, ...s.sets.map((x) => x.reps ?? 0));
          return [
            {
              id: e.exercise_id!,
              slug: e.exercise_slug!,
              name: e.name,
              e1rm: s.best_e1rm ? [s.best_e1rm] : [],
              reps: reps ? [reps] : [],
              perHand: e.load_mode === "per_hand",
              dumbbell: e.equipment === "dumbbell",
            },
          ];
        }),
      ),
    );
    api.weighIns(true).then((ws) => {
      const t = +parseLocal(at);
      let best: (typeof ws)[number] | null = null;
      for (const w of ws) if (!best || Math.abs(+parseLocal(w.at) - t) < Math.abs(+parseLocal(best.at) - t)) best = w;
      setBw(best?.weight_kg ?? null);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workoutId, key]);
  const { who, loaded } = useNormProfile(bw);
  if (!rows.length || !loaded) return null;
  return (
    <details className="card">
      <summary className="section-label">How your lifts compare</summary>
      {!who ? (
        <ProfileHint />
      ) : bw == null ? (
        <p className="muted small">Log your bodyweight to see strength comparisons.</p>
      ) : (
        <>
          {rows.map((r) => (
            <ExerciseComparisons
              key={r.id}
              slug={r.slug}
              who={who}
              weight={r.e1rm}
              reps={r.reps}
              label={r.name}
              perHand={r.perHand}
              dumbbell={r.dumbbell}
              note={`This session's best, at your bodyweight nearest the workout (${Math.round(toUnit(bw, unit))} ${unit}).`}
            />
          ))}
          <p className="muted small">Tap ⓘ for who each comparison is with and how variations are converted.</p>
        </>
      )}
    </details>
  );
}
