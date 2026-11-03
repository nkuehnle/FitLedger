import * as d3 from "d3";
import { useMemo, useState, type ReactNode } from "react";
import { LOAD_RAMP, useChartTheme, type ChartTheme } from "./theme";

// Front/back figures, 200 × 430 each, drawn on one continuous body outline. Muscle regions are
// polygons keyed by muscle id, smoothed into curves; "mirror" shapes are drawn on the figure's left
// half and mirrored. Proportions follow a lean athletic build but stay readable at thumbnail size.
type Pt = [number, number];
interface Region {
  muscle: string;
  pts: Pt[];
  mirror?: boolean;
}
const R = (muscle: string, pts: Pt[], mirror = true): Region => ({ muscle, pts, mirror });

const FRONT: Region[] = [
  R("neck", [[93, 50], [107, 50], [108, 64], [100, 67], [92, 64]], false),
  R("traps", [[91, 58], [92, 66], [80, 70], [70, 72], [78, 65]]),
  R("side_delts", [[62, 73], [55, 77], [50, 89], [50, 103], [55, 101], [55, 86]]),
  R("front_delts", [[69, 72], [62, 75], [57, 86], [57, 101], [62, 107], [67, 97], [71, 84]]),
  R("upper_chest", [[74, 75], [98, 74], [98, 91], [80, 92], [71, 88], [72, 80]]),
  R("lower_chest", [[71, 93], [98, 94], [98, 113], [90, 120], [79, 119], [72, 111], [69, 101]]),
  R("biceps", [[57, 110], [64, 112], [66, 128], [63, 145], [56, 148], [51, 133], [52, 118]]),
  R("forearm_flexors", [[48, 156], [61, 155], [60, 176], [56, 203], [48, 203], [45, 180]]),
  R("abs", [[91, 122], [109, 122], [111, 168], [109, 199], [100, 207], [91, 199], [89, 168]], false),
  R("obliques", [[76, 122], [88, 125], [87, 170], [89, 197], [81, 189], [75, 162], [72, 137]]),
  R("abductors", [[70, 204], [79, 199], [80, 220], [73, 238], [67, 224]]),
  R("quads", [[80, 208], [91, 223], [93, 252], [92, 290], [89, 313], [80, 318], [73, 302], [69, 270], [70, 241]]),
  R("adductors", [[93, 226], [99, 233], [98, 262], [95, 282], [94, 252]]),
  R("calves", [[74, 340], [81, 338], [82, 368], [80, 392], [76, 382], [72, 357]]),
  R("calves", [[88, 340], [91, 345], [90, 372], [87, 388], [86, 360]]),
];

const BACK: Region[] = [
  R("neck", [[93, 50], [107, 50], [108, 60], [92, 60]], false),
  R("traps", [[92, 56], [100, 57], [100, 121], [92, 110], [82, 92], [69, 75], [84, 65]]),
  R("side_delts", [[62, 73], [55, 77], [50, 89], [50, 103], [55, 101], [55, 86]]),
  R("rear_delts", [[68, 74], [62, 75], [57, 86], [57, 101], [62, 104], [68, 92], [71, 82]]),
  R("upper_back", [[71, 86], [80, 94], [90, 111], [86, 119], [75, 113], [69, 101]]),
  R("lats", [[69, 104], [75, 115], [88, 123], [95, 140], [92, 168], [81, 180], [75, 162], [71, 136]]),
  R("lower_back", [[95, 140], [100, 128], [100, 197], [91, 197], [87, 182], [93, 166]]),
  R("triceps", [[52, 106], [62, 108], [65, 125], [62, 146], [55, 148], [50, 130]]),
  R("forearm_extensors", [[48, 156], [61, 155], [60, 176], [56, 203], [48, 203], [45, 180]]),
  R("abductors", [[71, 197], [86, 192], [89, 205], [77, 214], [69, 209]]),
  R("glutes", [[76, 215], [89, 206], [99, 212], [99, 246], [88, 253], [75, 247], [70, 228]]),
  R("hamstrings", [[72, 254], [88, 257], [93, 264], [93, 300], [88, 318], [78, 318], [73, 297], [69, 272]]),
  R("adductors", [[95, 257], [99, 255], [98, 282], [95, 290]]),
  R("calves", [[76, 338], [85, 334], [91, 341], [91, 368], [87, 389], [80, 389], [74, 366]]),
];

// Left half of the body outline, from the neck down the arm, back up to the armpit, down the
// torso and leg, and up the inner leg to the crotch; mirrored for the right half.
const OUTLINE_LEFT: Pt[] = [
  [92, 47], [91, 58], [78, 64], [64, 70], [55, 76], [50, 88], [49, 104], [48, 122], [47, 140], [46, 154],
  [43, 172], [42, 192], [44, 207], [41, 222], [45, 236], [52, 237], [55, 222], [56, 208], [59, 190],
  [62, 170], [64, 152], [66, 134], [68, 114], [70, 132], [72, 156], [74, 176], [71, 196], [67, 216],
  [66, 242], [67, 272], [72, 302], [74, 322], [74, 338], [71, 356], [73, 378], [78, 398], [80, 408],
  [74, 418], [77, 426], [93, 426], [94, 412], [92, 400], [92, 378], [93, 352], [93, 332], [95, 312],
  [96, 288], [98, 258], [100, 234],
];
const OUTLINE: Pt[] = [...OUTLINE_LEFT, ...[...OUTLINE_LEFT].reverse().map(([x, y]) => [200 - x, y] as Pt)];
const HEAD: Pt[] = d3.range(0, 2 * Math.PI, Math.PI / 12).map((a) => [100 + 15 * Math.cos(a), 31 + 19 * Math.sin(a)] as Pt);

/** Untrained body: the head and the whole outline, under the muscles. */
const SILHOUETTE: Pt[][] = [HEAD, OUTLINE];

/** Definition lines (ab segments, linea alba, spine, kneecaps), drawn in the surface color. */
const DETAIL_FRONT = [
  "M100,124 L100,203",
  "M91,141 Q100,144 109,141",
  "M90,158 Q100,161 110,158",
  "M90,176 Q100,179 110,176",
  "M78,332 Q84,326 90,332 M122,332 Q116,326 110,332",
  // rectus femoris / vastus lateralis
  "M82,222 Q87,262 85,304 M118,222 Q113,262 115,304",
];
const DETAIL_BACK = [
  "M100,122 L100,196",
  "M100,212 L100,246",
  // biceps femoris / semitendinosus
  "M82,262 Q84,290 83,314 M118,262 Q116,290 117,314",
];

const mirror = (pts: Pt[]): Pt[] => pts.map(([x, y]) => [200 - x, y]);
// Closed Catmull-Rom curves through the polygon corners: rounded, organic outlines.
const smooth = d3.line().curve(d3.curveCatmullRomClosed.alpha(0.5));
const toPath = (pts: Pt[]) => smooth(pts) ?? "";

function regionsOf(view: Region[]) {
  return view.flatMap((r) =>
    r.mirror
      ? [
          { ...r, d: toPath(r.pts) },
          { ...r, d: toPath(mirror(r.pts)) },
        ]
      : [{ ...r, d: toPath(r.pts) }],
  );
}

const FRONT_PATHS = regionsOf(FRONT);
const BACK_PATHS = regionsOf(BACK);

export interface BodyMapProps {
  /** muscle id -> value (sets or volume) */
  values: Record<string, number>;
  /** Scale maximum; defaults to the largest value (pass a shared max when comparing maps). */
  max?: number;
  selected?: string | null;
  onSelect?: (muscle: string) => void;
  theme?: ChartTheme;
  /** Figure height in px. */
  height?: number;
  showLegend?: boolean;
  legendLabel?: string;
  muscleNames?: Record<string, string>;
  hideNumbers?: boolean;
  /** Weekly set targets (min, max) per muscle; values must then be sets per week. Colors become
   *  under / in / over target, and muscles over target get a dashed outline. */
  targets?: Record<string, [number, number]>;
  /** Precomputed fill per muscle (overrides values/targets coloring), with its own legend. */
  fills?: Record<string, string>;
  legend?: ReactNode;
}

const loadInterp = d3
  .scaleLinear<string>()
  .domain(LOAD_RAMP.map((r) => r[0]))
  .range(LOAD_RAMP.map((r) => r[1]))
  .interpolate(d3.interpolateLab)
  .clamp(true);

/** Where a weekly set count sits against its target: 0–0.5 under the minimum, 0.5–0.8 inside the
 *  range, 0.8–1 above the maximum (1 = half again over it). */
export function loadPosition(v: number, [lo, hi]: [number, number]): number {
  if (v <= 0) return 0;
  if (v < lo) return 0.5 * (v / lo);
  if (v <= hi) return 0.5 + 0.3 * (hi > lo ? (v - lo) / (hi - lo) : 1);
  return 0.8 + 0.2 * Math.min(1, (v - hi) / (0.5 * hi || 1));
}
export const loadColor = (t: number) => loadInterp(t);

/** Plain magnitude (no targets): white → deep red, stopping short of the "too much" black. */
export function useHeatScale(_theme: ChartTheme, max: number) {
  return useMemo(() => (v: number) => (v <= 0 ? null : loadInterp(0.03 + 0.77 * Math.min(1, v / (max || 1)))), [max]);
}

/** Color ramp legend for body maps (also used alone under side-by-side maps). */
export function HeatLegend({
  max,
  theme: forced,
  label,
  hideNumbers,
}: {
  max: number;
  theme?: ChartTheme;
  label?: string;
  hideNumbers?: boolean;
}) {
  const auto = useChartTheme();
  const theme = forced ?? auto;
  const color = useHeatScale(theme, max);
  return (
    <div className="heat-legend small muted">
      <span>0</span>
      <span
        className="heat-ramp"
        style={{ background: `linear-gradient(to right, ${[0.02, 0.25, 0.5, 0.75, 1].map((f) => color(f * (max || 1))).join(",")})` }}
      />
      <span>{hideNumbers ? "most" : `${Math.round(max * 10) / 10}`}</span>
      {label && <span>{label}</span>}
    </div>
  );
}

/** Legend for the target-relative scale (segments match loadPosition: 50% / 30% / 20%). */
export function LoadLegend() {
  const stops = [0.02, 0.25, 0.5, 0.65, 0.8, 0.9, 1].map((t) => `${loadColor(t)} ${t * 100}%`).join(",");
  return (
    <div className="load-legend small muted">
      <span className="load-ramp" style={{ background: `linear-gradient(to right, ${stops})` }} />
      <div className="load-labels">
        <span>under target</span>
        <span>in target</span>
        <span>over</span>
      </div>
    </div>
  );
}

export function BodyMap(props: BodyMapProps) {
  const auto = useChartTheme();
  const theme = props.theme ?? auto;
  const max = props.max ?? Math.max(0, ...Object.values(props.values));
  const color = useHeatScale(theme, max);
  const [hover, setHover] = useState<string | null>(null);
  const height = props.height ?? 320;
  const base = theme.mode === "dark" ? "#2a2f37" : "#e3e6eb";
  const active = hover ?? props.selected ?? null;

  const figure = (paths: typeof FRONT_PATHS, label: string, dx: number) => (
    <g transform={`translate(${dx},0)`}>
      {SILHOUETTE.map((p, i) => (
        <path key={i} d={toPath(p)} fill={base} stroke={base} strokeWidth={2} strokeLinejoin="round" />
      ))}
      {paths.map((r, i) => {
        const v = props.values[r.muscle] ?? 0;
        const t = props.targets?.[r.muscle];
        const fill = props.fills ? (props.fills[r.muscle] ?? null) : t ? (v > 0 ? loadColor(loadPosition(v, t)) : null) : color(v);
        const over = !!t && v > t[1];
        return (
          <path
            key={i}
            d={r.d}
            fill={fill ?? base}
            stroke={active === r.muscle || over ? theme.text : theme.surface}
            strokeWidth={active === r.muscle ? 2 : 1.5}
            strokeDasharray={over && active !== r.muscle ? "3 2" : undefined}
            strokeLinejoin="round"
            style={{ cursor: props.onSelect ? "pointer" : undefined }}
            onPointerEnter={() => setHover(r.muscle)}
            onPointerLeave={() => setHover(null)}
            onClick={() => props.onSelect?.(r.muscle)}
          >
            <title>{(props.muscleNames?.[r.muscle] ?? r.muscle) + (props.hideNumbers ? "" : `: ${Math.round(v * 10) / 10}`)}</title>
          </path>
        );
      })}
      <g fill="none" stroke={theme.surface} strokeWidth={1.2} strokeLinecap="round" pointerEvents="none" opacity={0.9}>
        {(label === "Front" ? DETAIL_FRONT : DETAIL_BACK).map((d) => (
          <path key={d} d={d} />
        ))}
      </g>
      <text x={100} y={430} textAnchor="middle" fontSize={11} fill={theme.muted}>
        {label}
      </text>
    </g>
  );

  return (
    <div className="body-map">
      <svg
        viewBox="0 0 410 440"
        height={height}
        style={{ maxWidth: "100%", height: "auto", display: "block", margin: "0 auto" }}
        fontFamily="system-ui, sans-serif"
      >
        {figure(FRONT_PATHS, "Front", 0)}
        {figure(BACK_PATHS, "Back", 210)}
      </svg>
      {props.showLegend !== false &&
        (props.legend ??
          (props.targets ? (
            <LoadLegend />
          ) : (
            <HeatLegend max={max} theme={theme} label={props.legendLabel} hideNumbers={props.hideNumbers} />
          )))}
      {hover && props.muscleNames && (
        <div className="small" style={{ textAlign: "center", minHeight: "1.4em" }}>
          {props.muscleNames[hover]}
          {!props.hideNumbers && `: ${Math.round((props.values[hover] ?? 0) * 10) / 10}`}
          {!props.hideNumbers && props.targets?.[hover] && ` / week (target ${props.targets[hover][0]}–${props.targets[hover][1]})`}
        </div>
      )}
    </div>
  );
}
