import * as d3 from "d3";
import { useEffect, useRef, useState } from "react";
import { useChartTheme } from "./theme";

/** GitHub-style year grid of training days: one cell per day, columns are weeks (Monday first). */
export function CalendarHeatmap({
  days,
  from,
  to,
  onSelect,
}: {
  /** date (YYYY-MM-DD) -> workouts that day */
  days: Map<string, number>;
  from: Date;
  to: Date;
  onSelect?: (date: string) => void;
}) {
  const theme = useChartTheme();
  const wrap = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  const [hover, setHover] = useState<{ d: string; x: number; y: number } | null>(null);
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver((e) => setWidth(Math.floor(e[0].contentRect.width)));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, []);
  const start = d3.timeMonday.floor(from);
  const weeks = d3.timeMonday.count(start, to) + 1;
  const left = 22;
  const cell = Math.max(6, Math.min(16, Math.floor((width - left) / weeks) - 2));
  const gap = 2;
  const h = 7 * (cell + gap) + 18;
  const fmt = d3.timeFormat("%Y-%m-%d");
  const cells = d3.timeDay.range(start, d3.timeDay.offset(to, 1));
  const color = (n: number) => (n === 0 ? theme.grid : theme.sequential[Math.min(theme.sequential.length - 1, 6 + n * 2)]);

  return (
    <div ref={wrap} style={{ position: "relative", overflowX: "auto" }}>
      <svg width={left + weeks * (cell + gap)} height={h} fontFamily="system-ui, sans-serif">
        {["M", "W", "F"].map((l, i) => (
          <text key={l} x={0} y={18 + i * 2 * (cell + gap) + cell * 0.8} fontSize={9} fill={theme.muted}>
            {l}
          </text>
        ))}
        {d3.timeMonth.range(d3.timeMonth.ceil(start), to).map((m) => (
          <text key={+m} x={left + d3.timeMonday.count(start, m) * (cell + gap)} y={10} fontSize={9} fill={theme.muted}>
            {m.getMonth() === 0 ? d3.timeFormat("%Y")(m) : d3.timeFormat("%b")(m)}
          </text>
        ))}
        {cells.map((d) => {
          const key = fmt(d);
          const n = days.get(key) ?? 0;
          const cx = left + d3.timeMonday.count(start, d) * (cell + gap);
          const cy = 16 + ((d.getDay() + 6) % 7) * (cell + gap);
          return (
            <rect
              key={key}
              x={cx}
              y={cy}
              width={cell}
              height={cell}
              rx={2}
              fill={color(n)}
              onPointerEnter={() => setHover({ d: key, x: cx, y: cy })}
              onPointerLeave={() => setHover(null)}
              onClick={() => n && onSelect?.(key)}
              style={{ cursor: n && onSelect ? "pointer" : undefined }}
            />
          );
        })}
      </svg>
      {hover && (
        <div className="chart-tooltip" style={{ left: hover.x + cell + 6, top: hover.y }}>
          <div className="muted">{d3.timeFormat("%a, %b %-d, %Y")(new Date(hover.d + "T00:00"))}</div>
          <strong>{days.get(hover.d) ? `${days.get(hover.d)} workout${days.get(hover.d)! > 1 ? "s" : ""}` : "Rest"}</strong>
        </div>
      )}
    </div>
  );
}
