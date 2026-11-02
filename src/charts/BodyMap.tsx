import * as d3 from "d3";
import { useMemo, useState, type ReactNode } from "react";
import { LOAD_RAMP, useChartTheme, type ChartTheme } from "./theme";

// Stylized front/back figures, 200 × 430 each. Regions are polygons keyed by muscle id; "L" shapes
// are drawn on the figure's left half and mirrored. Shapes are deliberately simple and readable at
// thumbnail size rather than anatomically exact.
type Pt = [number, number];
interface Region {
  muscle: string;
  pts: Pt[];
  mirror?: boolean;
}

const FRONT: Region[] = [
  {
    muscle: "neck",
    pts: [
      [92, 50],
      [108, 50],
      [110, 66],
      [90, 66],
    ],
  },
  {
    muscle: "traps",
    mirror: true,
    pts: [
      [90, 58],
      [90, 67],
      [74, 70],
    ],
  },
  {
    muscle: "front_delts",
    mirror: true,
    pts: [
      [72, 70],
      [62, 72],
      [57, 84],
      [58, 97],
      [64, 100],
      [70, 88],
      [75, 77],
    ],
  },
  {
    muscle: "side_delts",
    mirror: true,
    pts: [
      [62, 72],
      [53, 76],
      [48, 88],
      [50, 101],
      [58, 97],
      [57, 84],
    ],
  },
  {
    muscle: "upper_chest",
    mirror: true,
    pts: [
      [98, 72],
      [77, 73],
      [71, 86],
      [74, 95],
      [99, 95],
    ],
  },
  {
    muscle: "lower_chest",
    mirror: true,
    pts: [
      [99, 95],
      [74, 95],
      [72, 108],
      [80, 120],
      [99, 122],
    ],
  },
  {
    muscle: "biceps",
    mirror: true,
    pts: [
      [64, 100],
      [58, 97],
      [50, 101],
      [47, 120],
      [49, 141],
      [56, 147],
      [63, 137],
      [66, 118],
    ],
  },
  {
    muscle: "forearm_flexors",
    mirror: true,
    pts: [
      [56, 149],
      [49, 143],
      [45, 160],
      [43, 184],
      [46, 201],
      [53, 201],
      [57, 180],
      [60, 161],
    ],
  },
  {
    muscle: "abs",
    pts: [
      [88, 125],
      [112, 125],
      [113, 160],
      [111, 199],
      [100, 209],
      [89, 199],
      [87, 160],
    ],
  },
  {
    muscle: "obliques",
    mirror: true,
    pts: [
      [86, 125],
      [76, 121],
      [72, 140],
      [74, 170],
      [78, 197],
      [88, 200],
      [86, 160],
    ],
  },
  {
    muscle: "abductors",
    mirror: true,
    pts: [
      [76, 201],
      [70, 212],
      [67, 240],
      [72, 252],
      [77, 226],
    ],
  },
  {
    muscle: "quads",
    mirror: true,
    pts: [
      [79, 205],
      [78, 226],
      [73, 252],
      [72, 290],
      [78, 318],
      [90, 320],
      [95, 296],
      [95, 262],
      [91, 226],
      [89, 210],
    ],
  },
  {
    muscle: "adductors",
    mirror: true,
    pts: [
      [99, 213],
      [92, 226],
      [96, 262],
      [98, 282],
      [99.5, 280],
    ],
  },
  {
    muscle: "calves",
    mirror: true,
    pts: [
      [77, 336],
      [72, 360],
      [74, 391],
      [80, 404],
      [84, 391],
      [83, 360],
      [81, 337],
    ],
  },
];

const BACK: Region[] = [
  {
    muscle: "neck",
    pts: [
      [92, 50],
      [108, 50],
      [110, 60],
      [90, 60],
    ],
  },
  {
    muscle: "traps",
    mirror: true,
    pts: [
      [99.5, 53],
      [90, 58],
      [70, 70],
      [84, 80],
      [99.5, 108],
    ],
  },
  {
    muscle: "rear_delts",
    mirror: true,
    pts: [
      [70, 70],
      [61, 74],
      [56, 86],
      [59, 98],
      [66, 92],
      [77, 80],
    ],
  },
  {
    muscle: "side_delts",
    mirror: true,
    pts: [
      [61, 74],
      [52, 78],
      [47, 90],
      [50, 102],
      [59, 98],
      [56, 86],
    ],
  },
  {
    muscle: "upper_back",
    mirror: true,
    pts: [
      [84, 82],
      [77, 86],
      [79, 110],
      [99.5, 121],
      [99.5, 110],
    ],
  },
  {
    muscle: "lats",
    mirror: true,
    pts: [
      [76, 86],
      [66, 96],
      [70, 130],
      [78, 160],
      [87, 168],
      [95, 150],
      [97, 130],
      [99.5, 124],
      [78, 112],
    ],
  },
  {
    muscle: "lower_back",
    mirror: true,
    pts: [
      [99.5, 131],
      [97, 150],
      [89, 169],
      [89, 195],
      [99.5, 200],
    ],
  },
  {
    muscle: "triceps",
    mirror: true,
    pts: [
      [59, 99],
      [50, 103],
      [46, 122],
      [48, 143],
      [56, 147],
      [62, 136],
      [66, 112],
    ],
  },
  {
    muscle: "forearm_extensors",
    mirror: true,
    pts: [
      [56, 149],
      [49, 143],
      [45, 160],
      [43, 184],
      [46, 201],
      [53, 201],
      [57, 180],
      [60, 161],
    ],
  },
  {
    muscle: "abductors",
    mirror: true,
    pts: [
      [88, 196],
      [76, 193],
      [70, 204],
      [72, 214],
      [87, 207],
    ],
  },
  {
    muscle: "glutes",
    mirror: true,
    pts: [
      [99.5, 207],
      [88, 207],
      [73, 216],
      [70, 234],
      [78, 251],
      [96, 253],
      [99.5, 242],
    ],
  },
  {
    muscle: "hamstrings",
    mirror: true,
    pts: [
      [78, 254],
      [74, 281],
      [78, 316],
      [90, 318],
      [94, 290],
      [95, 257],
    ],
  },
  {
    muscle: "adductors",
    mirror: true,
    pts: [
      [96, 257],
      [94, 290],
      [98, 296],
      [99.5, 292],
      [99.5, 255],
    ],
  },
  {
    muscle: "calves",
    mirror: true,
    pts: [
      [77, 336],
      [72, 357],
      [76, 384],
      [84, 400],
      [92, 384],
      [95, 357],
      [90, 336],
    ],
  },
];

/** Untrained body parts drawn as plain silhouette. */
const SILHOUETTE: Pt[][] = [
  // head
  d3.range(0, 2 * Math.PI, Math.PI / 12).map((a) => [100 + 17 * Math.cos(a), 30 + 21 * Math.sin(a)] as Pt),
  // hands
  [
    [46, 203],
    [53, 203],
    [55, 222],
    [47, 226],
    [42, 214],
  ],
  [
    [154, 203],
    [147, 203],
    [145, 222],
    [153, 226],
    [158, 214],
  ],
  // knees
  [
    [78, 320],
    [90, 322],
    [92, 334],
    [78, 335],
  ],
  [
    [122, 320],
    [110, 322],
    [108, 334],
    [122, 335],
  ],
  // shins (inner lower leg)
  [
    [83, 337],
    [92, 336],
    [93, 370],
    [88, 404],
    [84, 404],
    [85, 360],
  ],
  [
    [117, 337],
    [108, 336],
    [107, 370],
    [112, 404],
    [116, 404],
    [115, 360],
  ],
  // feet
  [
    [78, 406],
    [90, 406],
    [92, 422],
    [74, 424],
  ],
  [
    [122, 406],
    [110, 406],
    [108, 422],
    [126, 424],
  ],
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
      {/* One continuous body: every part drawn fat in the base color, then the parts on top. */}
      <g fill={base} stroke={base} strokeWidth={7} strokeLinejoin="round">
        {SILHOUETTE.map((p, i) => (
          <path key={i} d={toPath(p)} />
        ))}
        {paths.map((r, i) => (
          <path key={i} d={r.d} />
        ))}
      </g>
      {SILHOUETTE.map((p, i) => (
        <path key={i} d={toPath(p)} fill={base} />
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
