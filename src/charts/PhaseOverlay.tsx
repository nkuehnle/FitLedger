import * as d3 from "d3";
import { useEffect, useRef, useState } from "react";
import type { PhaseLine } from "./corridor";
import { useChartTheme } from "./theme";

/** Stretches of the same goal on one axis of weeks since each began. The running one is drawn bold. */
export function PhaseOverlay({
  lines,
  unit,
  toUnit,
  height = 220,
}: {
  lines: PhaseLine[];
  unit: string;
  toUnit: (kg: number) => number;
  height?: number;
}) {
  const theme = useChartTheme();
  const wrap = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver((e) => setWidth(Math.max(240, Math.floor(e[0].contentRect.width))));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, []);
  const m = { top: 10, right: 48, bottom: 34, left: 8 };
  const all = lines.flatMap((l) => l.points);
  const x = d3
    .scaleLinear()
    .domain([0, Math.max(4, d3.max(all, (p) => p.x) ?? 4)])
    .range([m.left, width - m.right]);
  const [lo, hi] = d3.extent(all, (p) => toUnit(p.y)) as [number, number];
  const y = d3
    .scaleLinear()
    .domain([Math.min(0, lo), Math.max(0, hi)])
    .nice()
    .range([height - m.bottom, m.top]);
  const path = d3
    .line<{ x: number; y: number }>()
    .x((p) => x(p.x))
    .y((p) => y(toUnit(p.y)));
  // The running stretch takes the line color; earlier ones take the categorical slots, oldest first.
  const past = lines.filter((l) => !l.current);
  const color = (l: PhaseLine) => (l.current ? theme.line : theme.categorical[past.indexOf(l) % theme.categorical.length]);
  const signed = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(v)}`;
  return (
    <div ref={wrap} className="chart-plot">
      <div className="chart-legend small">
        {lines.map((l) => (
          <span key={l.id}>
            <i className="line" style={{ background: color(l), height: l.current ? 4 : 2 }} />
            {l.label}
            {l.current ? " (now)" : ""}
          </span>
        ))}
      </div>
      <svg
        width={width}
        height={height}
        style={{ display: "block", width: "100%", height: "auto" }}
        viewBox={`0 0 ${width} ${height}`}
        fontFamily="system-ui, sans-serif"
      >
        {y.ticks(5).map((v) => (
          <g key={v}>
            <line x1={m.left} x2={width - m.right} y1={y(v)} y2={y(v)} stroke={v === 0 ? theme.gridStrong : theme.grid} />
            <text x={width - m.right + 6} y={y(v)} dy="0.32em" fontSize={11} fill={theme.muted}>
              {signed(v)} {unit}
            </text>
          </g>
        ))}
        {x.ticks(Math.min(8, Math.floor(width / 60))).map((v) => (
          <text key={v} x={x(v)} y={height - 18} fontSize={11} fill={theme.muted} textAnchor="middle">
            {v}
          </text>
        ))}
        <text x={(m.left + width - m.right) / 2} y={height - 3} fontSize={11} fill={theme.muted} textAnchor="middle">
          Weeks since the phase began
        </text>
        {[...past, ...lines.filter((l) => l.current)].map((l) => (
          <path
            key={l.id}
            d={path(l.points)!}
            fill="none"
            stroke={color(l)}
            strokeWidth={l.current ? 3 : 1.75}
            strokeOpacity={l.current ? 1 : 0.8}
            strokeLinejoin="round"
            strokeLinecap="round"
          >
            <title>{l.label}</title>
          </path>
        ))}
      </svg>
    </div>
  );
}
