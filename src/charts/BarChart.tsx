import * as d3 from "d3";
import { useEffect, useRef, useState } from "react";
import { useChartTheme, type ChartTheme } from "./theme";

export interface Bar {
  key: string;
  label: string;
  value: number;
  /** Optional target band drawn behind the bar. */
  target?: [number, number];
  tooltip?: string;
  color?: string;
}

/** Vertical bars on a zero baseline (counts per period, sets per muscle). */
export function BarChart({
  bars,
  height = 200,
  width: fixedWidth,
  theme: forced,
  unit,
  hideNumbers,
  labelEvery,
  fontScale = 1,
  onSelect,
  goal,
}: {
  bars: Bar[];
  height?: number;
  width?: number;
  theme?: ChartTheme;
  unit?: string;
  hideNumbers?: boolean;
  /** Show every n-th x label (dense time bars). */
  labelEvery?: number;
  fontScale?: number;
  onSelect?: (key: string) => void;
  /** A dashed reference line across the chart (e.g. a weekly goal). */
  goal?: number;
}) {
  const auto = useChartTheme();
  const theme = forced ?? auto;
  const wrap = useRef<HTMLDivElement>(null);
  const [measured, setMeasured] = useState(600);
  const [hover, setHover] = useState<number | null>(null);
  const width = fixedWidth ?? measured;
  useEffect(() => {
    if (fixedWidth || !wrap.current) return;
    const ro = new ResizeObserver((e) => setMeasured(Math.max(240, Math.floor(e[0].contentRect.width))));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, [fixedWidth]);

  const fs = fontScale;
  const m = { top: 10 * fs, right: (hideNumbers ? 8 : 40) * fs, bottom: 28 * fs, left: 4 * fs };
  const max = Math.max(goal ?? 0, d3.max(bars, (b) => Math.max(b.value, b.target?.[1] ?? 0)) ?? 1);
  const x = d3
    .scaleBand()
    .domain(bars.map((b) => b.key))
    .range([m.left, width - m.right])
    .paddingInner(bars.length > 30 ? 0.15 : 0.3);
  const y = d3
    .scaleLinear()
    .domain([0, max || 1])
    .nice(4)
    .range([height - m.bottom, m.top]);
  const every = labelEvery ?? Math.max(1, Math.ceil(bars.length / Math.max(4, width / 56)));
  const bw = x.bandwidth();
  const r = Math.min(4, bw / 2);

  return (
    <div ref={wrap} className="chart-plot" style={{ position: "relative" }}>
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        style={{ display: "block", width: fixedWidth ? width : "100%", height: "auto" }}
        fontFamily="system-ui, sans-serif"
      >
        {y.ticks(4).map((v) => (
          <g key={v}>
            <line x1={m.left} x2={width - m.right} y1={y(v)} y2={y(v)} stroke={theme.grid} />
            {!hideNumbers && (
              <text x={width - m.right + 6 * fs} y={y(v)} dy="0.32em" fontSize={11 * fs} fill={theme.muted}>
                {d3.format(",~r")(v)}
              </text>
            )}
          </g>
        ))}
        {goal != null && (
          <line x1={m.left} x2={width - m.right} y1={y(goal)} y2={y(goal)} stroke={theme.muted} strokeDasharray="4 3" strokeWidth={1.5} />
        )}
        {bars.map((b, i) => {
          const bx = x(b.key)!;
          const top = y(b.value);
          const h = y(0) - top;
          return (
            <g
              key={b.key}
              onPointerEnter={() => setHover(i)}
              onPointerLeave={() => setHover(null)}
              onClick={() => onSelect?.(b.key)}
              style={{ cursor: onSelect ? "pointer" : undefined }}
            >
              <rect x={bx - 2} width={bw + 4} y={m.top} height={height - m.top - m.bottom} fill="transparent" />
              {b.target && <rect x={bx} width={bw} y={y(b.target[1])} height={y(b.target[0]) - y(b.target[1])} fill={theme.grid} rx={2} />}
              {h > 0 && (
                // Rounded data end, square at the baseline.
                <path
                  d={`M${bx},${y(0)} V${top + r} Q${bx},${top} ${bx + r},${top} H${bx + bw - r} Q${bx + bw},${top} ${bx + bw},${top + r} V${y(0)} Z`}
                  fill={b.color ?? theme.line}
                  opacity={hover === null || hover === i ? 1 : 0.55}
                />
              )}
              {i % every === 0 && (
                <text x={bx + bw / 2} y={height - 10 * fs} fontSize={10.5 * fs} fill={theme.muted} textAnchor="middle">
                  {b.label}
                </text>
              )}
            </g>
          );
        })}
        <line x1={m.left} x2={width - m.right} y1={y(0)} y2={y(0)} stroke={theme.gridStrong} />
      </svg>
      {hover !== null && bars[hover] && (
        <div
          className="chart-tooltip"
          style={{ left: `${((x(bars[hover].key)! + bw / 2) / width) * 100}%`, transform: "translateX(-50%)", top: 0 }}
        >
          <div className="muted">{bars[hover].tooltip ?? bars[hover].label}</div>
          <strong>
            {hideNumbers ? "•••" : d3.format(",~r")(Math.round(bars[hover].value * 10) / 10)}
            {unit ? ` ${unit}` : ""}
          </strong>
          {bars[hover].target && (
            <div className="muted small">
              target {bars[hover].target![0]}–{bars[hover].target![1]}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
