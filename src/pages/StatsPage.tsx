import { useEffect, useMemo, useState } from "react";
import {
  api,
  type DiscomfortPoint,
  type LastTrained,
  type Muscle,
  type MuscleGroup,
  type MuscleStat,
  type MuscleTarget,
  type PeriodCount,
  type ResponsePoint,
  type Phase,
  type Streak,
  type WeekMuscleSets,
  type WorkoutPoint,
} from "../api";
import { navigate, usePrefs } from "../App";
import { BarChart } from "../charts/BarChart";
import { BodyMap, HeatLegend, LoadLegend } from "../charts/BodyMap";
import { MuscleWeekGrid } from "../charts/MuscleWeekGrid";
import { ProgressHealth } from "../components/ProgressHealth";
import { CalendarHeatmap } from "../charts/CalendarHeatmap";
import { RangePicker, inRange, isoDate, phaseRange, presetRange, today, type Range, useSavedRange } from "../charts/RangePicker";
import { ScatterChart } from "../charts/ScatterChart";
import { TargetRange } from "../charts/TargetRange";
import { ProgressHeader } from "../components/ProgressHeader";
import { StrengthBalanceSection } from "../components/StrengthBalance";
import { TimeSeriesChart, type Band } from "../charts/TimeSeriesChart";
import { useChartTheme } from "../charts/theme";
import { ShareButton } from "../share/ShareDialog";
import { formatDate, formatVolume, parseLocal, toUnit } from "../format";

export default function StatsPage() {
  const [range, setRange] = useSavedRange("progress");
  const [phases, setPhases] = useState<Phase[]>([]);
  const [groups, setGroups] = useState<MuscleGroup[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  useEffect(() => {
    api.listPhases().then(setPhases);
    api.muscles().then((m) => {
      setGroups(m.groups);
      setMuscles(m.muscles);
    });
  }, []);
  const names = Object.fromEntries(muscles.map((m) => [m.id, m.name]));

  return (
    <div className="page stats-page">
      <ProgressHeader current="stats" />
      <RangePicker value={range} onChange={setRange} phases={phases} presets={["week", "4w", "3m", "6m", "1y", "all"]} />
      <div className="stats-grid">
        <MuscleMapSection range={range} phases={phases} names={names} muscles={muscles} />
        <TargetsSection range={range} names={names} />
        <GroupTrendSection range={range} groups={groups} muscles={muscles} phases={phases} />
        <StrengthBalanceSection names={names} />
        <FrequencySection phases={phases} />
        <DurationSection range={range} phases={phases} />
        <VolumeResponseSection muscles={muscles} />
        <DiscomfortSection />
      </div>
    </div>
  );
}

function bandsOf(phases: Phase[]): Band[] {
  return phases.map((p) => ({
    start: parseLocal(p.start_date),
    end: p.end_date ? parseLocal(p.end_date) : null,
    label: p.name,
    lane: p.lane,
  }));
}

// ---- Body map ----

/** Weekly set targets per muscle (they depend on the current goal). */
function useTargets(): Record<string, [number, number]> {
  const [targets, setTargets] = useState<MuscleTarget[]>([]);
  useEffect(() => {
    const load = () => api.muscleTargets().then(setTargets);
    load();
    window.addEventListener("goal-changed", load);
    return () => window.removeEventListener("goal-changed", load);
  }, []);
  return useMemo(
    () => Object.fromEntries(targets.filter((t) => t.max_sets > 0).map((t) => [t.muscle_id, [t.min_sets, t.max_sets]])),
    [targets],
  );
}

/** Length of a range in weeks (at least 1); all-time starts at the first workout. */
function weeksIn(range: Range, firstWorkout: string | null): number {
  const from = range.from ?? firstWorkout;
  if (!from) return 1;
  const to = range.to ? parseLocal(range.to) : new Date();
  return Math.max(1, (to.getTime() - parseLocal(from).getTime() + 86400000) / (7 * 86400000));
}

function WeeksGrid({
  range,
  names,
  muscles,
  targets,
}: {
  range: Range;
  names: Record<string, string>;
  muscles: Muscle[];
  targets: Record<string, [number, number]>;
}) {
  const [weekly, setWeekly] = useState<WeekMuscleSets[]>([]);
  useEffect(() => {
    api.weeklyMuscleSets(range.from ?? undefined, range.to ?? undefined).then(setWeekly);
  }, [range]);
  return (
    <div>
      <p className="muted small">
        Sets per week for each muscle, against its weekly target. Dark cells are weeks of more work than you can likely recover from.
      </p>
      <MuscleWeekGrid weekly={weekly} muscles={muscles.map((m) => m.id)} names={names} targets={targets} />
    </div>
  );
}

// ---- Body map: sections ----------------------------------------------------------------------------

function MuscleMapSection({
  range,
  phases,
  names,
  muscles,
}: {
  range: Range;
  phases: Phase[];
  names: Record<string, string>;
  muscles: Muscle[];
}) {
  const { unit } = usePrefs();
  const [metric, setMetric] = useState<"sets" | "volume">("sets");
  const [mode, setMode] = useState<"map" | "health" | "grid" | "compare" | "animate">("map");
  const [stats, setStats] = useState<MuscleStat[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [last, setLast] = useState<LastTrained[]>([]);

  const targets = useTargets();
  const [firstWorkout, setFirstWorkout] = useState<string | null>(null);
  useEffect(() => {
    api.muscleSets(range.from ?? undefined, range.to ?? undefined).then(setStats);
    api.lastTrained(today()).then(setLast);
  }, [range]);
  useEffect(() => {
    api.workoutsPerPeriod(false, today()).then((w) => setFirstWorkout(w[0]?.start ?? null));
  }, []);

  // Sets are shown per week so they can be judged against weekly targets.
  const weeks = weeksIn(range, firstWorkout);
  const value = (s: MuscleStat) => (metric === "sets" ? s.sets / weeks : toUnit(s.volume_kg, unit));
  const values = Object.fromEntries(stats.map((s) => [s.muscle_id, value(s)]));
  const mapTargets = metric === "sets" ? targets : undefined;
  const sel = stats.find((s) => s.muscle_id === selected);

  return (
    <section className="card span-2">
      <div className="title-row">
        <h2 className="section-title">Muscles trained · {range.label}</h2>
        <ShareButton kind="bodymap" range={range} />
      </div>
      <div className="row" style={{ marginTop: 0, marginBottom: 8 }}>
        <div className="segmented small">
          <button className={metric === "sets" ? "active" : ""} onClick={() => setMetric("sets")}>
            Sets
          </button>
          <button className={metric === "volume" ? "active" : ""} onClick={() => setMetric("volume")}>
            Volume
          </button>
        </div>
        <div className="segmented small">
          {(["map", "health", "grid", "compare", "animate"] as const).map((m) => (
            <button key={m} className={mode === m ? "active" : ""} onClick={() => setMode(m)}>
              {{ map: "Map", health: "Health", grid: "Grid", compare: "Compare", animate: "Replay" }[m]}
            </button>
          ))}
        </div>
      </div>
      {mode === "map" && (
        <div className="map-with-detail">
          <BodyMap
            values={values}
            selected={selected}
            onSelect={setSelected}
            legendLabel={metric === "sets" ? "sets" : unit}
            muscleNames={names}
            height={360}
            targets={mapTargets}
          />
          <div className="muscle-detail">
            {sel ? (
              <MuscleDetail
                stat={sel}
                name={names[sel.muscle_id]}
                range={range}
                unit={unit}
                last={last.find((l) => l.muscle_id === sel.muscle_id)}
              />
            ) : (
              <>
                <p className="muted small">Tap a muscle for its sets, exercises and trend.</p>
                <div className="section-label">Days since last trained</div>
                <ul className="days-since small">
                  {[...last]
                    .filter((l) => l.muscle_id !== "neck" || l.days_ago != null)
                    .sort((a, b) => (b.days_ago ?? 9999) - (a.days_ago ?? 9999))
                    .map((l) => (
                      <li key={l.muscle_id} onClick={() => setSelected(l.muscle_id)}>
                        <span>{names[l.muscle_id] ?? l.muscle_id}</span>
                        <span className={l.days_ago != null && l.days_ago > 7 ? "warn" : "muted"}>
                          {l.days_ago == null ? "never" : l.days_ago === 0 ? "today" : `${l.days_ago}d`}
                        </span>
                      </li>
                    ))}
                </ul>
              </>
            )}
          </div>
        </div>
      )}
      {mode === "health" && <ProgressHealth names={names} />}
      {mode === "grid" && <WeeksGrid range={range} names={names} muscles={muscles} targets={targets} />}
      {mode === "compare" && <CompareMaps phases={phases} metric={metric} names={names} unit={unit} targets={mapTargets} />}
      {mode === "animate" && <AnimatedWeeks range={range} metric={metric} names={names} unit={unit} targets={mapTargets} />}
      <details>
        <summary className="small muted">Table view</summary>
        <table className="sets small">
          <tbody>
            {muscles.map((m) => (
              <tr key={m.id}>
                <td>{m.name}</td>
                <td className="right">{Math.round((values[m.id] ?? 0) * 10) / 10}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </section>
  );
}

function MuscleDetail({
  stat,
  name,
  range,
  unit,
  last,
}: {
  stat: MuscleStat;
  name: string;
  range: Range;
  unit: "kg" | "lb";
  last?: LastTrained;
}) {
  const [weekly, setWeekly] = useState<WeekMuscleSets[]>([]);
  useEffect(() => {
    api
      .weeklyMuscleSets(range.from ?? undefined, range.to ?? undefined)
      .then((w) => setWeekly(w.filter((x) => x.muscle_id === stat.muscle_id)));
  }, [range, stat.muscle_id]);
  return (
    <div>
      <h3 style={{ margin: "0 0 4px" }}>{name}</h3>
      <div className="stats-row small">
        <span>
          <strong>{Math.round(stat.sets * 10) / 10}</strong> sets
        </span>
        <span>{formatVolume(stat.volume_kg, unit)}</span>
        {last?.days_ago != null && <span>last trained {last.days_ago === 0 ? "today" : `${last.days_ago} days ago`}</span>}
      </div>
      <div className="section-label">Exercises</div>
      <ul className="muscle-bars">
        {stat.exercises.slice(0, 8).map(([n, s]) => (
          <li key={n} className="small">
            <span>{n}</span>
            <span className="bar-track">
              <span className="bar-fill" style={{ width: `${(s / stat.exercises[0][1]) * 100}%` }} />
            </span>
            <span className="num-cell">{Math.round(s * 10) / 10}</span>
          </li>
        ))}
      </ul>
      {weekly.length > 1 && (
        <>
          <div className="section-label">Sets per week</div>
          <BarChart
            bars={weekly.map((w) => ({
              key: w.week_start,
              label: w.week_start.slice(5),
              value: w.sets,
              tooltip: `Week of ${w.week_start}`,
            }))}
            height={140}
          />
        </>
      )}
    </div>
  );
}

function CompareMaps({
  phases,
  metric,
  names,
  unit,
  targets,
}: {
  phases: Phase[];
  metric: "sets" | "volume";
  names: Record<string, string>;
  unit: "kg" | "lb";
  targets?: Record<string, [number, number]>;
}) {
  const options: Range[] = [presetRange("week"), presetRange("4w"), presetRange("3m"), ...phases.map(phaseRange)];
  // Default: last two phases if there are any, else the previous vs this 4 weeks.
  const prev4: Range = (() => {
    const a = new Date();
    a.setDate(a.getDate() - 55);
    const b = new Date();
    b.setDate(b.getDate() - 28);
    return { key: "prev4w", from: isoDate(a), to: isoDate(b), label: "Previous 4 weeks" };
  })();
  const all = [prev4, ...options];
  const [aKey, setA] = useState(phases.length >= 2 ? `phase-${phases[phases.length - 2].id}` : "prev4w");
  const [bKey, setB] = useState(phases.length >= 2 ? `phase-${phases[phases.length - 1].id}` : "4w");
  const [a, setAStats] = useState<MuscleStat[]>([]);
  const [b, setBStats] = useState<MuscleStat[]>([]);
  const ra = all.find((r) => r.key === aKey) ?? prev4;
  const rb = all.find((r) => r.key === bKey) ?? presetRange("4w");
  useEffect(() => {
    api.muscleSets(ra.from ?? undefined, ra.to ?? undefined).then(setAStats);
    api.muscleSets(rb.from ?? undefined, rb.to ?? undefined).then(setBStats);
  }, [ra.from, ra.to, rb.from, rb.to]);
  // Per week, so ranges of different lengths compare fairly.
  const weeks = (r: Range) =>
    Math.max(
      1,
      ((r.to ? parseLocal(r.to) : new Date()).getTime() - (r.from ? parseLocal(r.from) : new Date(2000, 0)).getTime()) / (7 * 86400000),
    );
  const vals = (s: MuscleStat[], r: Range) =>
    Object.fromEntries(s.map((x) => [x.muscle_id, (metric === "sets" ? x.sets : toUnit(x.volume_kg, unit)) / weeks(r)]));
  const va = vals(a, ra);
  const vb = vals(b, rb);
  const max = Math.max(0, ...Object.values(va), ...Object.values(vb));
  const select = (v: string, set: (k: string) => void) => (
    <select className="select" value={v} onChange={(e) => set(e.target.value)}>
      {all.map((r) => (
        <option key={r.key} value={r.key}>
          {r.label}
        </option>
      ))}
    </select>
  );
  return (
    <div>
      <p className="muted small">Average per week, same color scale on both.</p>
      <div className="compare-maps">
        <div>
          {select(aKey, setA)}
          <BodyMap values={va} max={max} height={260} muscleNames={names} showLegend={false} targets={targets} />
        </div>
        <div>
          {select(bKey, setB)}
          <BodyMap values={vb} max={max} height={260} muscleNames={names} showLegend={false} targets={targets} />
        </div>
      </div>
      {targets ? <LoadLegend /> : <HeatLegend max={max} label={`${metric === "sets" ? "sets" : unit} / week`} />}
    </div>
  );
}

function AnimatedWeeks({
  range,
  metric,
  names,
  unit,
  targets,
}: {
  range: Range;
  metric: "sets" | "volume";
  names: Record<string, string>;
  unit: "kg" | "lb";
  targets?: Record<string, [number, number]>;
}) {
  const [weekly, setWeekly] = useState<WeekMuscleSets[]>([]);
  const [i, setI] = useState(0);
  const [playing, setPlaying] = useState(false);
  useEffect(() => {
    const from = range.from ?? undefined;
    api.weeklyMuscleSets(from, range.to ?? undefined).then((w) => {
      setWeekly(w);
      setI(0);
    });
  }, [range]);
  const weeks = useMemo(() => [...new Set(weekly.map((w) => w.week_start))].sort(), [weekly]);
  const max = Math.max(0, ...weekly.map((w) => (metric === "sets" ? w.sets : toUnit(w.volume_kg, unit))));
  useEffect(() => {
    if (!playing) return;
    const t = setInterval(() => setI((x) => (x + 1 < weeks.length ? x + 1 : (setPlaying(false), x))), 700);
    return () => clearInterval(t);
  }, [playing, weeks.length]);
  if (weeks.length === 0) return <p className="empty">No training in this range.</p>;
  const wk = weeks[Math.min(i, weeks.length - 1)];
  const values = Object.fromEntries(
    weekly.filter((w) => w.week_start === wk).map((w) => [w.muscle_id, metric === "sets" ? w.sets : toUnit(w.volume_kg, unit)]),
  );
  return (
    <div>
      <div className="row" style={{ marginTop: 0 }}>
        <button className="button small" onClick={() => (i >= weeks.length - 1 ? (setI(0), setPlaying(true)) : setPlaying(!playing))}>
          {playing ? "Pause" : "Play"}
        </button>
        <input type="range" min={0} max={weeks.length - 1} value={i} onChange={(e) => setI(Number(e.target.value))} style={{ flex: 1 }} />
        <span className="small">Week of {wk}</span>
      </div>
      <BodyMap
        values={values}
        max={max}
        height={320}
        muscleNames={names}
        legendLabel={metric === "sets" ? "sets" : unit}
        targets={targets}
      />
    </div>
  );
}

// ---- Targets ------------------------------------------------------------------------------------

function TargetsSection({ range, names }: { range: Range; names: Record<string, string> }) {
  const [targets, setTargets] = useState<MuscleTarget[]>([]);
  const [stats, setStats] = useState<MuscleStat[]>([]);
  const [firstWorkout, setFirstWorkout] = useState<string | null>(null);
  useEffect(() => {
    api.muscleTargets().then(setTargets);
    api.muscleSets(range.from ?? undefined, range.to ?? undefined).then(setStats);
  }, [range]);
  useEffect(() => {
    // "All time" has no start date: average over the weeks since the first workout.
    api.workoutsPerPeriod(false, today()).then((w) => setFirstWorkout(w[0]?.start ?? null));
  }, []);
  const fromStr = range.from ?? firstWorkout;
  const from = fromStr ? parseLocal(fromStr) : null;
  const weeks = from
    ? Math.max(1, Math.round(((range.to ? parseLocal(range.to) : new Date()).getTime() - from.getTime()) / (7 * 86400000)))
    : null;
  if (!weeks) return null;
  const perWeek = (m: string) => (stats.find((s) => s.muscle_id === m)?.sets ?? 0) / weeks;
  const rows = targets.filter((t) => t.max_sets > 0);
  return (
    <section className="card">
      <h2 className="section-title">Weekly sets vs targets</h2>
      <p className="muted small">
        Average per week over {range.label.toLowerCase()} ({weeks} wk). <a href="#/plan/targets">Edit targets</a>
      </p>
      <TargetRange rows={rows} value={perWeek} names={names} />
    </section>
  );
}

// ---- Frequency ---------------------------------------------------------------------------------

function FrequencySection({ phases }: { phases: Phase[] }) {
  const [monthly, setMonthly] = useState(false);
  const [counts, setCounts] = useState<PeriodCount[]>([]);
  const [streak, setStreak] = useState<Streak | null>(null);
  const [series, setSeries] = useState<WorkoutPoint[]>([]);
  useEffect(() => {
    api.workoutsPerPeriod(monthly, today()).then(setCounts);
  }, [monthly]);
  useEffect(() => {
    api.streak(today()).then(setStreak);
    api.workoutSeries().then(setSeries);
  }, []);
  const shown = counts.slice(monthly ? -24 : -52);
  const days = useMemo(() => {
    const m = new Map<string, number>();
    series.forEach((p) => m.set(p.at.slice(0, 10), (m.get(p.at.slice(0, 10)) ?? 0) + 1));
    return m;
  }, [series]);
  const yearAgo = new Date();
  yearAgo.setFullYear(yearAgo.getFullYear() - 1);
  void phases;
  return (
    <section className="card span-2">
      <div className="title-row">
        <h2 className="section-title">Consistency</h2>
        <ShareButton kind="count" />
      </div>
      <div className="stats-row">
        <div className="stat">
          <div className="stat-value">{streak?.current_weeks ?? 0} wk</div>
          <div className="stat-label">Current streak</div>
        </div>
        <div className="stat">
          <div className="stat-value">{streak?.longest_weeks ?? 0} wk</div>
          <div className="stat-label">Longest streak</div>
        </div>
        <div className="stat">
          <div className="stat-value">{series.length}</div>
          <div className="stat-label">Workouts</div>
        </div>
      </div>
      <div className="segmented small">
        <button className={!monthly ? "active" : ""} onClick={() => setMonthly(false)}>
          Per week
        </button>
        <button className={monthly ? "active" : ""} onClick={() => setMonthly(true)}>
          Per month
        </button>
      </div>
      <BarChart
        bars={shown.map((c) => ({
          key: c.start,
          label: monthly ? parseLocal(c.start).toLocaleDateString(undefined, { month: "short" }) : c.start.slice(5),
          value: c.workouts,
          tooltip: monthly ? parseLocal(c.start).toLocaleDateString(undefined, { month: "long", year: "numeric" }) : `Week of ${c.start}`,
        }))}
        unit="workouts"
        height={180}
      />
      <div className="section-label">Training days, last 12 months</div>
      <CalendarHeatmap
        days={days}
        from={yearAgo}
        to={new Date()}
        onSelect={(d) => {
          const w = series.find((p) => p.at.startsWith(d));
          if (w) navigate(`/history/${w.workout_id}`);
        }}
      />
    </section>
  );
}

// ---- Muscle-group trend --------------------------------------------------------------------------

function GroupTrendSection({
  range,
  groups,
  muscles,
  phases,
}: {
  range: Range;
  groups: MuscleGroup[];
  muscles: Muscle[];
  phases: Phase[];
}) {
  const { unit } = usePrefs();
  const [weekly, setWeekly] = useState<WeekMuscleSets[]>([]);
  const [group, setGroup] = useState("chest");
  const [metric, setMetric] = useState<"sets" | "volume">("sets");
  useEffect(() => {
    api.weeklyMuscleSets(range.from ?? undefined, range.to ?? undefined).then(setWeekly);
  }, [range]);
  const inGroup = new Set(muscles.filter((m) => m.group === group).map((m) => m.id));
  const byWeek = new Map<string, number>();
  weekly
    .filter((w) => inGroup.has(w.muscle_id))
    .forEach((w) => byWeek.set(w.week_start, (byWeek.get(w.week_start) ?? 0) + (metric === "sets" ? w.sets : toUnit(w.volume_kg, unit))));
  const points = [...byWeek.entries()].sort().map(([d, v]) => ({ t: parseLocal(d), v }));
  return (
    <section className="card">
      <h2 className="section-title">Weekly {metric} by muscle group</h2>
      <div className="chips-row" style={{ marginBottom: 6 }}>
        {groups.map((g) => (
          <button key={g.id} className={group === g.id ? "chip-btn active" : "chip-btn"} onClick={() => setGroup(g.id)}>
            {g.name}
          </button>
        ))}
      </div>
      <div className="segmented small">
        <button className={metric === "sets" ? "active" : ""} onClick={() => setMetric("sets")}>
          Sets
        </button>
        <button className={metric === "volume" ? "active" : ""} onClick={() => setMetric("volume")}>
          Volume
        </button>
      </div>
      <TimeSeriesChart
        series={[{ id: group, label: group, points }]}
        unit={metric === "sets" ? "sets" : unit}
        bands={bandsOf(phases)}
        zero
        height={200}
      />
    </section>
  );
}

/** Your own dose-response: weekly sets for a muscle against the strength change they came with. */
function VolumeResponseSection({ muscles }: { muscles: Muscle[] }) {
  const [muscle, setMuscle] = useState("lower_chest");
  const [points, setPoints] = useState<ResponsePoint[]>([]);
  useEffect(() => {
    api.volumeResponse(muscle).then(setPoints);
  }, [muscle]);
  const kinds = [
    ["gain", "Gain"],
    ["maintain", "Maintain"],
    ["cut", "Cut"],
    [null, "No goal set"],
  ] as const;
  const series = kinds
    .map(([goal, label]) => ({
      id: String(goal),
      label,
      points: points
        .filter((p) => p.goal === goal)
        .map((p) => ({
          x: Math.round(p.sets_per_week * 10) / 10,
          y: Math.round(p.change * 1000) / 10,
          label: `${formatDate(p.start)} – ${formatDate(p.end)} · ${p.sets_per_week.toFixed(1)} sets/wk`,
        })),
    }))
    .filter((s) => s.points.length > 0);
  return (
    <section className="card">
      <h2 className="section-title">Sets vs strength gained</h2>
      <select className="select" value={muscle} onChange={(e) => setMuscle(e.target.value)}>
        {muscles.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </select>
      {points.length < 3 ? (
        <p className="muted small">This needs about four months of training a muscle to show a pattern.</p>
      ) : (
        <>
          <ScatterChart series={series} xLabel="Sets a week" yLabel="Strength change, %" height={260} />
          <p className="muted small">
            Each dot is 6 weeks: the muscle's average weekly sets, and how much stronger (or weaker) its main exercises were than in the 6
            weeks before, in the same rep ranges. Dots further right that aren't higher are sets that didn't pay. Food and sleep changed
            between dots too, so read it as a pattern, not proof.
          </p>
        </>
      )}
    </section>
  );
}

function DurationSection({ range, phases }: { range: Range; phases: Phase[] }) {
  const [series, setSeries] = useState<WorkoutPoint[]>([]);
  useEffect(() => {
    api.workoutSeries().then(setSeries);
  }, []);
  const points = series
    .filter((p) => p.duration_s && inRange(p.at, range))
    .map((p) => ({ t: parseLocal(p.at), v: Math.round(p.duration_s! / 60) }));
  return (
    <section className="card">
      <h2 className="section-title">Workout duration</h2>
      <TimeSeriesChart series={[{ id: "dur", label: "Duration", points }]} unit="min" bands={bandsOf(phases)} height={200} />
    </section>
  );
}

function DiscomfortSection() {
  const { unit } = usePrefs();
  const theme = useChartTheme();
  const [pts, setPts] = useState<DiscomfortPoint[]>([]);
  const [ex, setEx] = useState<number | null>(null);
  useEffect(() => {
    api.discomfortPoints().then((p) => {
      setPts(p);
      if (p.length) setEx(p[p.length - 1].exercise_id);
    });
  }, []);
  void theme;
  const exercises = [...new Map(pts.map((p) => [p.exercise_id, p.exercise_name])).entries()];
  const mine = pts.filter((p) => p.exercise_id === ex && p.max_weight_kg);
  const series = (["elbow", "knee", "shoulder"] as const)
    .map((j) => ({
      id: j,
      label: j[0].toUpperCase() + j.slice(1),
      points: mine
        .filter((p) => p[j] != null)
        .map((p) => ({ x: Math.round(toUnit(p.max_weight_kg!, unit)), y: p[j]!, label: p.at.slice(0, 10) })),
    }))
    .filter((s) => s.points.length > 0);
  return (
    <section className="card">
      <h2 className="section-title">Joint discomfort vs load</h2>
      {pts.length === 0 ? (
        <p className="muted small">
          Rate elbow, knee or shoulder discomfort (1–5) under “Notes & joints” during a workout and it will show up here against the weight
          you used.
        </p>
      ) : (
        <>
          <select className="select" value={ex ?? ""} onChange={(e) => setEx(Number(e.target.value))}>
            {exercises.map(([id, n]) => (
              <option key={id} value={id}>
                {n}
              </option>
            ))}
          </select>
          <ScatterChart series={series} xLabel={`Heaviest set (${unit})`} yLabel="Discomfort 1–5" yDomain={[1, 5]} />
        </>
      )}
    </section>
  );
}
