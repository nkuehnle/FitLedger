import * as d3 from "d3";
import { useEffect, useRef, useState } from "react";
import { useChartTheme } from "./theme";

export interface ScatterPoint {
  x: number;
  y: number;
  label: string;
}

/** Up to three series (fixed categorical slots validate all-pairs only for three). */
export function ScatterChart({
  series,
  xLabel,
  yLabel,
  yDomain,
  height = 240,
}: {
  series: { id: string; label: string; points: ScatterPoint[] }[];
  xLabel: string;
  yLabel: string;
  yDomain?: [number, number];
  height?: number;
}) {
  const theme = useChartTheme();
  const wrap = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  const [hover, setHover] = useState<{ p: ScatterPoint; s: string; color: string } | null>(null);
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver((e) => setWidth(Math.max(240, Math.floor(e[0].contentRect.width))));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, []);
  const m = { top: 24, right: 40, bottom: 34, left: 8 };
  const all = series.flatMap((s) => s.points);
  const x = d3
    .scaleLinear()
    .domain(d3.extent(all, (p) => p.x) as [number, number])
    .nice()
    .range([m.left, width - m.right]);
  const y = d3
    .scaleLinear()
    .domain(yDomain ?? (d3.extent(all, (p) => p.y) as [number, number]))
    .nice()
    .range([height - m.bottom, m.top]);
  return (
    <div ref={wrap} className="chart-plot" style={{ position: "relative" }}>
      {series.length > 1 && (
        <div className="chart-legend small">
          {series.map((s, i) => (
            <span key={s.id}>
              <i className="dot" style={{ background: theme.categorical[i] }} />
              {s.label}
            </span>
          ))}
        </div>
      )}
      <svg
        width={width}
        height={height}
        style={{ display: "block", width: "100%", height: "auto" }}
        viewBox={`0 0 ${width} ${height}`}
        fontFamily="system-ui, sans-serif"
      >
        {y.ticks(5).map((v) => (
          <g key={v}>
            <line x1={m.left} x2={width - m.right} y1={y(v)} y2={y(v)} stroke={theme.grid} />
            <text x={width - m.right + 6} y={y(v)} dy="0.32em" fontSize={11} fill={theme.muted}>
              {v}
            </text>
          </g>
        ))}
        {x.ticks(5).map((v) => (
          <text key={v} x={x(v)} y={height - 18} fontSize={11} fill={theme.muted} textAnchor="middle">
            {d3.format(",~r")(v)}
          </text>
        ))}
        <text x={(m.left + width - m.right) / 2} y={height - 3} fontSize={11} fill={theme.muted} textAnchor="middle">
          {xLabel}
        </text>
        <text x={width - 4} y={m.top - 12} fontSize={11} fill={theme.muted} textAnchor="end" dy="-0.2em">
          {yLabel}
        </text>
        {series.map((s, si) =>
          s.points.map((p, i) => (
            <circle
              key={`${si}-${i}`}
              cx={x(p.x)}
              cy={y(p.y) + (si - (series.length - 1) / 2) * 3}
              r={5}
              fill={theme.categorical[si]}
              stroke={theme.surface}
              strokeWidth={2}
              onPointerEnter={() => setHover({ p, s: s.label, color: theme.categorical[si] })}
              onPointerLeave={() => setHover(null)}
            />
          )),
        )}
      </svg>
      {hover && (
        <div
          className="chart-tooltip"
          style={{ left: `${(x(hover.p.x) / width) * 100}%`, top: y(hover.p.y) - 50, transform: "translateX(12px)" }}
        >
          <div className="muted">{hover.p.label}</div>
          <strong>
            {hover.s}: {hover.p.y}
          </strong>
        </div>
      )}
    </div>
  );
}
