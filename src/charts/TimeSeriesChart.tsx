import * as d3 from "d3";
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { ShareButton } from "../share/ShareDialog";
import { loess } from "./loess";
import { useChartTheme, type ChartTheme } from "./theme";

export interface Point {
  t: Date;
  v: number;
}

export interface Series {
  id: string;
  label: string;
  points: Point[];
  /** Defaults to the theme line color (first series) or categorical slots. */
  color?: string;
  /** "points": draw markers only (e.g. InBody weights on the bodyweight chart). */
  style?: "line" | "points";
}

export interface Band {
  start: Date;
  end: Date | null;
  label: string;
  lane: "program" | "nutrition";
}

export interface Marker {
  t: Date;
  v: number;
  label: string;
}

export interface TimeSeriesProps {
  series: Series[];
  /** Appended to y labels, e.g. "lb". */
  unit?: string;
  yFormat?: (v: number) => string;
  markers?: Marker[];
  bands?: Band[];
  height?: number;
  /** Fixed width (for image export); otherwise fills the container. */
  width?: number;
  theme?: ChartTheme;
  /** Hide y values (sharing). */
  hideNumbers?: boolean;
  /** Controlled smoothing; when omitted the chart shows its own toggle. */
  smooth?: { on: boolean; span: number };
  showControls?: boolean;
  /** Include zero on the y axis (counts, volume). */
  zero?: boolean;
  /** Values above 0 and below 0 mean different things (surplus/deficit): keep 0 centred and
   *  color the two sides with the diverging pair, labelled. */
  diverging?: { aboveLabel: string; belowLabel: string };
  /** Gridline spacing in data units (e.g. 250 kcal); gridlines are dashed and labels thinned to fit. */
  yStep?: number;
  fontScale?: number;
  /** Card title when the chart is shared as an image (defaults to the first series' label). */
  title?: string;
  /** Show a Share button (off where the card already shares this chart). */
  share?: boolean;
  /** Touch drags move the readout instead of scrolling (full screen). */
  scrub?: boolean;
}

/** Raw vs Trend is remembered across charts and restarts. */
const SMOOTH_KEY = "fitledger.chart.trend";
function storedTrend(): boolean {
  try {
    return localStorage.getItem(SMOOTH_KEY) === "1";
  } catch {
    return false;
  }
}

const DAY = 86_400_000;

export function TimeSeriesChart(props: TimeSeriesProps) {
  const [smoothOn, setSmoothOnState] = useState(storedTrend);
  const setSmoothOn = (on: boolean) => {
    setSmoothOnState(on);
    try {
      localStorage.setItem(SMOOTH_KEY, on ? "1" : "0");
    } catch {
      // Storage unavailable: the choice lasts until the chart closes.
    }
  };
  const [span, setSpan] = useState(0.3);
  const [showBands, setShowBands] = useState(true);
  const [full, setFull] = useState(false);
  const smooth = props.smooth ?? { on: smoothOn, span };
  const controls = props.showControls ?? !props.smooth;
  const hasLine = props.series.some((s) => s.style !== "points" && s.points.length > 2);
  const hasData = props.series.some((s) => s.points.length > 0);

  const toolbar = (inFull: boolean) => (
    <div className="chart-controls">
      {controls && hasLine && (
        <div className="segmented small">
          <button className={!smooth.on ? "active" : ""} onClick={() => setSmoothOn(false)}>
            Raw
          </button>
          <button className={smooth.on ? "active" : ""} onClick={() => setSmoothOn(true)}>
            Trend
          </button>
        </div>
      )}
      {controls && smooth.on && hasLine && (
        <label className="span-slider small muted">
          Smoothing
          <input type="range" min={0.05} max={1} step={0.05} value={span} onChange={(e) => setSpan(Number(e.target.value))} />
        </label>
      )}
      {controls && (props.bands?.length ?? 0) > 0 && (
        <label className="check small muted">
          <input type="checkbox" checked={showBands} onChange={(e) => setShowBands(e.target.checked)} />
          Phases
        </label>
      )}
      {hasData && props.share !== false && (
        <ShareButton
          kind="chart"
          title={props.title ?? props.series[0]?.label ?? "Chart"}
          chart={{ ...props, smooth, bands: showBands ? props.bands : undefined }}
        />
      )}
      {hasData && !inFull && (
        <button className="icon-btn chart-expand" title="Full screen" aria-label="Full screen" onClick={() => setFull(true)}>
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
          </svg>
        </button>
      )}
    </div>
  );
  const body = (height?: number) => (
    <>
      {props.series.length > 1 && <Legend series={props.series} theme={props.theme} />}
      <TimeSeriesSvg
        {...props}
        height={height ?? props.height}
        smooth={smooth}
        bands={showBands ? props.bands : undefined}
        scrub={height !== undefined}
      />
    </>
  );

  return (
    <div className="chart">
      {toolbar(false)}
      {body()}
      {full && <FullscreenChart onClose={() => setFull(false)} toolbar={toolbar(true)} render={body} />}
    </div>
  );
}

/** Chart filling the window. On a phone held upright the overlay is turned sideways, so time runs
 *  along the long edge whatever the rotation lock says; "Upright" turns it back. */
function FullscreenChart({ onClose, toolbar, render }: { onClose: () => void; toolbar: ReactNode; render: (height: number) => ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const [height, setHeight] = useState(0);
  const [sideways, setSideways] = useState(true);
  const [portrait, setPortrait] = useState(() => window.innerHeight > window.innerWidth);
  useEffect(() => {
    if (!box.current) return;
    const ro = new ResizeObserver((e) => setHeight(Math.floor(e[0].contentRect.height)));
    ro.observe(box.current);
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const resize = () => setPortrait(window.innerHeight > window.innerWidth);
    window.addEventListener("keydown", key);
    window.addEventListener("resize", resize);
    return () => {
      ro.disconnect();
      window.removeEventListener("keydown", key);
      window.removeEventListener("resize", resize);
    };
  }, [onClose]);
  return createPortal(
    <div className={`chart-full${sideways && portrait ? " sideways" : ""}`} role="dialog" aria-modal="true">
      <div className="chart-full-head">
        {toolbar}
        <div className="chart-full-actions">
          {portrait && (
            <button className="button small" onClick={() => setSideways(!sideways)}>
              {sideways ? "Upright" : "Sideways"}
            </button>
          )}
          <button className="button small" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
      <div className="chart-full-body" ref={box}>
        {height > 0 && render(Math.max(160, height - 40))}
      </div>
    </div>,
    document.body,
  );
}

function Legend({ series, theme }: { series: Series[]; theme?: ChartTheme }) {
  const auto = useChartTheme();
  const t = theme ?? auto;
  return (
    <div className="chart-legend small">
      {series.map((s, i) => (
        <span key={s.id}>
          <i className={s.style === "points" ? "dot" : "line"} style={{ background: seriesColor(s, i, t) }} />
          {s.label}
        </span>
      ))}
    </div>
  );
}

function seriesColor(s: Series, i: number, t: ChartTheme): string {
  return s.color ?? (i === 0 ? t.line : t.categorical[(i - 1) % t.categorical.length]);
}

/** The plot itself; exported for image rendering. */
export function TimeSeriesSvg(props: TimeSeriesProps & { smooth: { on: boolean; span: number } }) {
  const auto = useChartTheme();
  const theme = props.theme ?? auto;
  const wrap = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const [measured, setMeasured] = useState(600);
  const width = props.width ?? measured;
  const height = props.height ?? 240;
  const fs = props.fontScale ?? 1;
  const gid = "ts" + useId().replace(/[^a-zA-Z0-9]/g, "");
  const [hover, setHover] = useState<{ x: number; y: number; t: Date; rows: { label: string; v: number; color: string }[] } | null>(null);

  useEffect(() => {
    if (props.width || !wrap.current) return;
    const ro = new ResizeObserver((e) => setMeasured(Math.max(240, Math.floor(e[0].contentRect.width))));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, [props.width]);

  const fmt = props.yFormat ?? ((v: number) => d3.format(",.4~r")(v));
  // Right margin fits the widest y label (labels sit on the right).
  const labelChars = useMemo(() => {
    const vals = props.series.flatMap((s) => s.points.map((p) => p.v));
    if (!vals.length) return 6;
    const [lo, hi] = d3.extent(vals) as [number, number];
    const ticks = d3
      .scaleLinear()
      .domain([props.zero ? Math.min(0, lo) : lo, hi])
      .nice(5)
      .ticks(5);
    return Math.max(...ticks.map((t) => fmt(t).length)) + (props.unit ? props.unit.length + 1 : 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.series, props.unit, props.zero]);
  const margin = { top: 12 * fs, right: props.hideNumbers ? 12 * fs : (labelChars * 6.6 + 14) * fs, bottom: 24 * fs, left: 8 * fs };

  const model = useMemo(() => {
    const all = props.series.flatMap((s) => s.points);
    if (all.length === 0) return null;
    const [t0, t1] = d3.extent(all, (p) => p.t) as [Date, Date];
    const tPad = t0.getTime() === t1.getTime() ? DAY : 0;
    const x = d3
      .scaleTime()
      .domain([new Date(t0.getTime() - tPad), new Date(t1.getTime() + tPad)])
      .range([margin.left, width - margin.right]);
    const smoothed = props.series.map((s) => {
      if (!props.smooth.on || s.style === "points" || s.points.length < 4) return null;
      const xs = s.points.map((p) => p.t.getTime() / DAY);
      return loess(
        xs,
        s.points.map((p) => p.v),
        props.smooth.span,
      );
    });
    let [y0, y1] = d3.extent(all, (p) => p.v) as [number, number];
    if (props.zero) y0 = Math.min(0, y0);
    if (props.diverging) {
      let m = Math.max(Math.abs(y0), Math.abs(y1)) || 1;
      if (props.yStep) m = Math.ceil(m / props.yStep) * props.yStep;
      y0 = -m;
      y1 = m;
    }
    if (y0 === y1) {
      y0 -= 1;
      y1 += 1;
    }
    const y = d3
      .scaleLinear()
      .domain([y0, y1])
      .nice(props.yStep ? 1 : 5)
      .range([height - margin.bottom, margin.top]);
    return { x, y, smoothed };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.series, props.smooth.on, props.smooth.span, width, height, props.zero, props.diverging, props.yStep, props.hideNumbers, fs]);

  if (!model) {
    return (
      <div ref={wrap} className="chart-empty muted small" style={{ height }}>
        No data in this range.
      </div>
    );
  }
  const { x, y, smoothed } = model;
  // Diverging pair (validated palette): blue above 0, red below.
  const div = props.diverging ? { above: theme.categorical[0], below: theme.categorical[7] } : null;
  const zeroAt = div ? (y(0) - margin.top) / (height - margin.top - margin.bottom) : 0;
  const [d0, d1] = x.domain();
  const spanDays = (d1.getTime() - d0.getTime()) / DAY;
  const years = d3.timeYear.range(d3.timeYear.ceil(d0), d1);
  const months = d3.timeMonth.range(d3.timeMonth.ceil(d0), d1).filter((m) => m.getMonth() !== 0);
  // Sparse x labels: years over long ranges, months when zoomed in.
  const xLabels: { t: Date; text: string }[] =
    spanDays > 420
      ? years.map((t) => ({ t, text: d3.timeFormat("%Y")(t) }))
      : spanDays > 60
        ? d3.timeMonth
            .range(d3.timeMonth.ceil(d0), d1)
            .filter((_, i, a) => a.length <= 8 || i % Math.ceil(a.length / 8) === 0)
            .map((t) => ({ t, text: t.getMonth() === 0 ? d3.timeFormat("%Y")(t) : d3.timeFormat("%b")(t) }))
        : x.ticks(Math.max(2, Math.floor(width / 110))).map((t) => ({ t, text: d3.timeFormat("%b %-d")(t) }));
  const yTicks = props.yStep
    ? d3.range(Math.ceil(y.domain()[0] / props.yStep) * props.yStep, y.domain()[1] + 1e-9, props.yStep)
    : y.ticks(5);
  // With a fixed step, label every n-th line so labels stay ≥ 18px apart (always including 0).
  const labelEvery = props.yStep ? Math.max(1, Math.ceil(18 / Math.abs(y(0) - y(props.yStep)))) : 1;
  const line = d3
    .line<[number, number]>()
    .x((d) => d[0])
    .y((d) => d[1]);

  function onMove(e: React.PointerEvent<SVGSVGElement>) {
    const rect = svgRef.current!.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * width;
    const t = x.invert(px);
    const rows: { label: string; v: number; color: string }[] = [];
    let nearest: Point | null = null;
    props.series.forEach((s, si) => {
      if (s.points.length === 0) return;
      const i = d3.bisector((p: Point) => p.t).center(s.points, t);
      const p = s.points[i];
      if (Math.abs(x(p.t) - px) > 40) return;
      const v = smoothed[si] ? smoothed[si]![i] : p.v;
      if (!nearest) nearest = { t: p.t, v };
      rows.push({ label: s.label, v, color: seriesColor(s, si, theme) });
    });
    const n = nearest as Point | null;
    setHover(n ? { x: x(n.t), y: y(n.v), t: n.t, rows } : null);
  }

  return (
    <div ref={wrap} className="chart-plot" style={{ position: "relative" }}>
      <svg
        ref={svgRef}
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        style={{ display: "block", width: props.width ? width : "100%", height: "auto", touchAction: props.scrub ? "none" : "pan-y" }}
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
        fontFamily="system-ui, -apple-system, Segoe UI, Roboto, sans-serif"
      >
        {props.bands?.map((b, i) => {
          const a = Math.max(x(b.start), margin.left);
          const z = Math.min(x(b.end ?? d1), width - margin.right);
          if (z <= a) return null;
          const lane = b.lane === "nutrition" ? 1 : 0;
          return (
            <g key={i}>
              <rect x={a} width={z - a} y={margin.top} height={height - margin.top - margin.bottom} fill={theme.band[lane]} />
              {/* Only label bands wide enough for their name. */}
              {z - a > b.label.length * 6 * fs + 8 && (
                <text x={a + 4} y={margin.top + 11 * fs + lane * 13 * fs} fontSize={10 * fs} fill={theme.muted}>
                  {b.label}
                </text>
              )}
              <title>{b.label}</title>
            </g>
          );
        })}
        {months.map((m) => (
          <line key={+m} x1={x(m)} x2={x(m)} y1={margin.top} y2={height - margin.bottom} stroke={theme.grid} strokeDasharray="2 4" />
        ))}
        {years.map((yv) => (
          <line key={+yv} x1={x(yv)} x2={x(yv)} y1={margin.top} y2={height - margin.bottom} stroke={theme.gridStrong} />
        ))}
        {yTicks.map((v) => (
          <g key={v}>
            <line
              x1={margin.left}
              x2={width - margin.right}
              y1={y(v)}
              y2={y(v)}
              stroke={props.yStep ? theme.gridStrong : theme.grid}
              strokeDasharray={props.yStep ? "3 4" : undefined}
            />
            {!props.hideNumbers && (!props.yStep || Math.round(v / props.yStep) % labelEvery === 0) && (
              <text x={width - margin.right + 8 * fs} y={y(v)} dy="0.32em" fontSize={11 * fs} fill={theme.muted}>
                {fmt(v)}
                {props.unit ? ` ${props.unit}` : ""}
              </text>
            )}
          </g>
        ))}
        {xLabels.map((l) => (
          <text key={+l.t} x={x(l.t)} y={height - 6 * fs} fontSize={11 * fs} fill={theme.muted} textAnchor="middle">
            {l.text}
          </text>
        ))}

        {div && (
          <>
            <defs>
              <linearGradient id={`${gid}-line`} gradientUnits="userSpaceOnUse" x1={0} x2={0} y1={margin.top} y2={height - margin.bottom}>
                <stop offset={zeroAt} stopColor={div.above} />
                <stop offset={zeroAt} stopColor={div.below} />
              </linearGradient>
            </defs>
            {/* A clear, solid zero line. */}
            <line
              x1={margin.left}
              x2={width - margin.right}
              y1={y(0)}
              y2={y(0)}
              stroke={theme.text}
              strokeOpacity={0.75}
              strokeWidth={1.5 * fs}
            />
            <text x={width - margin.right - 6} y={margin.top + 12 * fs} fontSize={11 * fs} fill={theme.muted} textAnchor="end">
              <tspan fill={div.above}>■</tspan> {props.diverging!.aboveLabel} ↑
            </text>
            <text x={width - margin.right - 6} y={height - margin.bottom - 6 * fs} fontSize={11 * fs} fill={theme.muted} textAnchor="end">
              <tspan fill={div.below}>■</tspan> {props.diverging!.belowLabel} ↓
            </text>
          </>
        )}
        {props.series.map((s, si) => {
          const color = div && s.style !== "points" ? `url(#${gid}-line)` : seriesColor(s, si, theme);
          // Diverging charts shade the area between the line and 0 in the side's color.
          const shade = (vals: number[]) =>
            div ? (
              <path
                d={d3
                  .area<[number, number]>()
                  .x((d) => d[0])
                  .y0(y(0))
                  .y1((d) => d[1])(s.points.map((p, i) => [x(p.t), y(vals[i])]))!}
                fill={`url(#${gid}-line)`}
                opacity={0.18}
              />
            ) : null;
          const pts: [number, number][] = s.points.map((p) => [x(p.t), y(p.v)]);
          const sm = smoothed[si];
          if (s.style === "points") {
            return (
              <g key={s.id}>
                {pts.map(([px, py], i) => (
                  <rect
                    key={i}
                    x={px - 4}
                    y={py - 4}
                    width={8}
                    height={8}
                    transform={`rotate(45 ${px} ${py})`}
                    fill={theme.surface}
                    stroke={color}
                    strokeWidth={2}
                  />
                ))}
              </g>
            );
          }
          if (sm) {
            return (
              <g key={s.id}>
                {shade(sm)}
                {pts.map(([px, py], i) => (
                  <circle key={i} cx={px} cy={py} r={2.5 * fs} fill={color} opacity={0.22} />
                ))}
                <path
                  d={line(s.points.map((p, i) => [x(p.t), y(sm[i])]))!}
                  fill="none"
                  stroke={color}
                  strokeWidth={2 * fs}
                  strokeLinejoin="round"
                  strokeLinecap="round"
                />
              </g>
            );
          }
          // A marker on every point: hollow when there's room, small solid dots when dense.
          const gap = (width - margin.left - margin.right) / Math.max(1, pts.length - 1);
          const r = Math.max(1.75, Math.min(4, gap / 2.5)) * fs;
          const hollow = r >= 3 * fs;
          return (
            <g key={s.id}>
              {shade(s.points.map((p) => p.v))}
              <path d={line(pts)!} fill="none" stroke={color} strokeWidth={(hollow ? 2 : 1.25) * fs} strokeLinejoin="round" strokeLinecap="round" />
              {pts.map(([px, py], i) => (
                <circle key={i} cx={px} cy={py} r={r} fill={hollow ? theme.surface : color} stroke={hollow ? color : "none"} strokeWidth={2 * fs} />
              ))}
            </g>
          );
        })}

        {props.markers?.map((m, i) => (
          <g key={i}>
            <path
              d={`M${x(m.t)},${y(m.v) - 9 * fs} l${4 * fs},${-6 * fs} h${-8 * fs} z`}
              fill={theme.categorical[3]}
              stroke={theme.surface}
              strokeWidth={1}
            >
              <title>{m.label}</title>
            </path>
          </g>
        ))}

        {hover && (
          <g pointerEvents="none">
            <line x1={hover.x} x2={hover.x} y1={margin.top} y2={height - margin.bottom} stroke={theme.muted} strokeOpacity={0.5} />
            <circle cx={hover.x} cy={hover.y} r={5} fill={theme.surface} stroke={hover.rows[0]?.color ?? theme.line} strokeWidth={2} />
          </g>
        )}
      </svg>
      {hover && (
        <div
          className="chart-tooltip"
          style={{
            left: `${(hover.x / width) * 100}%`,
            transform: hover.x > width * 0.6 ? "translateX(calc(-100% - 12px))" : "translateX(12px)",
          }}
        >
          <div className="muted">{d3.timeFormat("%a, %b %-d, %Y")(hover.t)}</div>
          {hover.rows.map((r) => (
            <div key={r.label}>
              {hover.rows.length > 1 && <i className="line" style={{ background: r.color }} />}
              <strong>
                {props.hideNumbers ? "•••" : fmt(r.v)}
                {props.unit && !props.hideNumbers ? ` ${props.unit}` : ""}
              </strong>
              {hover.rows.length > 1 && <span className="muted"> {r.label}</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
