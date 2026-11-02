import * as d3 from "d3";
import { useEffect, useMemo, useRef, useState } from "react";
import type { WeekMuscleSets } from "../api";
import { LoadLegend, loadColor, loadPosition } from "./BodyMap";
import { useChartTheme } from "./theme";

/** Muscles × weeks, each cell colored by weekly sets against that muscle's target: shows trends,
 *  gaps and stretches of too much work at a glance. */
export function MuscleWeekGrid({
  weekly,
  muscles,
  names,
  targets,
}: {
  weekly: WeekMuscleSets[];
  /** Row order (muscle ids). */
  muscles: string[];
  names: Record<string, string>;
  targets: Record<string, [number, number]>;
}) {
  const theme = useChartTheme();
  const wrap = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  const [hover, setHover] = useState<{ m: string; w: string; v: number; x: number; y: number } | null>(null);
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver((e) => setWidth(Math.max(280, Math.floor(e[0].contentRect.width))));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, []);

  const { weeks, cell, rows } = useMemo(() => {
    const byKey = new Map(weekly.map((w) => [`${w.muscle_id}|${w.week_start}`, w.sets]));
    const starts = weekly.map((w) => w.week_start).sort();
    const weeks: string[] = [];
    if (starts.length) {
      const end = d3.timeParse("%Y-%m-%d")(starts[starts.length - 1])!;
      for (let d = d3.timeParse("%Y-%m-%d")(starts[0])!; d <= end; d = d3.timeDay.offset(d, 7)) weeks.push(d3.timeFormat("%Y-%m-%d")(d));
    }
    const rows = muscles.filter((m) => weekly.some((w) => w.muscle_id === m));
    return { weeks, cell: (m: string, w: string) => byKey.get(`${m}|${w}`) ?? 0, rows };
  }, [weekly, muscles]);

  if (!weeks.length) return <p className="empty">No training in this range.</p>;
  const labelW = Math.min(120, width * 0.32);
  const rowH = 16;
  const top = 18;
  const cw = (width - labelW) / weeks.length;
  const gap = cw >= 6 ? 1 : 0;
  const height = top + rows.length * rowH;
  // Year (or month, for short ranges) ticks along the top.
  const tickFmt = weeks.length > 30 ? "%Y" : "%b";
  const ticks = weeks.filter((w, i) => i === 0 || d3.timeFormat(tickFmt)(new Date(w)) !== d3.timeFormat(tickFmt)(new Date(weeks[i - 1])));

  return (
    <div ref={wrap} className="muscle-grid" style={{ position: "relative" }}>
      <svg
        width={width}
        height={height}
        style={{ display: "block" }}
        fontFamily="system-ui, sans-serif"
        onPointerLeave={() => setHover(null)}
      >
        {ticks.map((w) => {
          const x = labelW + weeks.indexOf(w) * cw;
          return (
            <g key={w}>
              <line x1={x} x2={x} y1={top - 4} y2={height} stroke={theme.gridStrong} />
              <text x={x + 3} y={top - 6} fontSize={10.5} fill={theme.muted}>
                {d3.timeFormat(tickFmt)(d3.timeParse("%Y-%m-%d")(w)!)}
              </text>
            </g>
          );
        })}
        {rows.map((m, ri) => {
          const y = top + ri * rowH;
          const t = targets[m];
          return (
            <g key={m}>
              <text x={labelW - 6} y={y + rowH / 2 + 4} fontSize={11} fill={theme.text} textAnchor="end">
                {(names[m] ?? m).length > 18 ? (names[m] ?? m).slice(0, 17) + "…" : (names[m] ?? m)}
              </text>
              {weeks.map((w, wi) => {
                const v = cell(m, w);
                if (v <= 0) return null;
                return (
                  <rect
                    key={w}
                    x={labelW + wi * cw}
                    y={y + 1}
                    width={Math.max(1, cw - gap)}
                    height={rowH - 2}
                    fill={t ? loadColor(loadPosition(v, t)) : loadColor(0.65)}
                    onPointerEnter={() => setHover({ m, w, v, x: labelW + wi * cw, y })}
                  />
                );
              })}
            </g>
          );
        })}
      </svg>
      {hover && (
        <div className="chart-tooltip small" style={{ left: Math.min(hover.x, width - 180), top: hover.y + rowH + 4 }}>
          <strong>{names[hover.m] ?? hover.m}</strong> · week of {hover.w}
          <br />
          {Math.round(hover.v * 10) / 10} sets
          {targets[hover.m] && ` (target ${targets[hover.m][0]}–${targets[hover.m][1]})`}
        </div>
      )}
      <LoadLegend />
    </div>
  );
}
