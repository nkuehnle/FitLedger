import { useEffect, useRef, useState, type ReactNode } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, type ExerciseStats, type InBodyScan, type Muscle, type WeighInRow, type WorkoutDetail } from "../api";
import { usePrefs } from "../App";
import { BodyMap, useHeatScale } from "../charts/BodyMap";
import { inRange, today, type Range } from "../charts/RangePicker";
import { TimeSeriesSvg, type TimeSeriesProps } from "../charts/TimeSeriesChart";
import { DARK, LIGHT, type ChartTheme } from "../charts/theme";
import { Modal } from "../components/Modal";
import { formatDate, formatDuration, formatVolume, formatWeight, parseLocal, toUnit } from "../format";
import { groupPrs, prTags } from "../pages/HistoryPage";
import { IS_ANDROID } from "../platform";

export type ShareKind =
  | { kind: "workout"; workoutId: number }
  | { kind: "exercise"; exerciseId: number }
  | { kind: "bodymap"; range: Range }
  | { kind: "bodyweight"; range: Range }
  | { kind: "inbody" }
  | { kind: "count" }
  /** Any time-series chart, as it's shown (series, smoothing, phases). */
  | { kind: "chart"; title: string; chart: TimeSeriesProps & { smooth: { on: boolean; span: number } } };

export function ShareButton(props: ShareKind) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button className="button small" onClick={() => setOpen(true)} aria-label="Share as image">
        Share
      </button>
      {open && <ShareDialog spec={props} onClose={() => setOpen(false)} />}
    </>
  );
}

const W = 1080;
const PAD = 72;
/** Cuts a line to fit the card at `size` px (≈0.55 em per character). Counts whole characters, so
 *  an emoji (two UTF-16 units) is never cut in half, which would make the SVG unencodable. */
export const fit = (text: string, size: number) => {
  const max = Math.floor((W - PAD * 2) / (size * 0.55));
  const chars = [...text];
  return chars.length > max ? chars.slice(0, max - 1).join("") + "…" : text;
};

interface CardContent {
  title: string;
  subtitle: string;
  /** Big stat lines under the title. */
  stats: { label: string; value: string }[];
  /** Extra text lines (PRs). */
  lines: string[];
  /** A chart or body map rendered off-screen; its <svg> elements are inlined into the card. */
  visual: ((theme: ChartTheme, width: number, height: number, hide: boolean) => ReactNode) | null;
}

function ShareDialog({ spec, onClose }: { spec: ShareKind; onClose: () => void }) {
  const { unit } = usePrefs();
  const [style, setStyle] = useState<"dark" | "light">(document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark");
  const [size, setSize] = useState<"square" | "story">("square");
  const [hide, setHide] = useState(false);
  const [content, setContent] = useState<CardContent | null>(null);
  const [title, setTitle] = useState("");
  const [subtitle, setSubtitle] = useState("");
  const [png, setPng] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const theme = style === "dark" ? DARK : LIGHT;
  const H = size === "square" ? 1080 : 1920;

  useEffect(() => {
    buildContent(spec, unit)
      .then((c) => {
        setContent(c);
        setTitle(c.title);
        setSubtitle(c.subtitle);
      })
      .catch((e) => setMsg(`Couldn't load the card: ${e}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Layout: header (title, subtitle, stats, lines) then the visual fills the rest.
  const statsH = content?.stats.length ? 150 : 0;
  const linesH = (content?.lines.length ?? 0) * 44;
  const top = PAD + 150 + statsH + linesH;
  const visualH = Math.max(200, H - top - PAD - 40);
  const visualW = W - PAD * 2;

  useEffect(() => {
    if (!content) return;
    setPng(null);
    // Wait for the off-screen visual to render, then compose and rasterize.
    const t = setTimeout(async () => {
      const svgs = stage.current ? Array.from(stage.current.querySelectorAll("svg")) : [];
      const svg = composeSvg({ theme, H, title, subtitle, content, hide, visuals: svgs, top, visualW, visualH });
      try {
        setPng(await rasterize(svg, W, H));
      } catch (e) {
        setMsg(`Couldn't draw the image: ${e instanceof Event ? "the browser rejected the SVG" : e}`);
      }
    }, 120);
    return () => clearTimeout(t);
  }, [content, style, size, hide, title, subtitle, theme, H, top, visualW, visualH]);

  async function saveFile() {
    if (!png) return;
    const path = await save({ defaultPath: `fitledger-${slug(title)}-${today()}.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
    if (!path) return;
    await api.saveBase64(path, png.split(",")[1]);
    setMsg(`Saved ${path}`);
  }
  async function copy() {
    if (!png) return;
    try {
      await api.copyPng(png.split(",")[1]);
      setMsg("Copied to clipboard.");
    } catch (e) {
      setMsg(`Couldn't copy: ${e}`);
    }
  }
  async function saveToPhotos() {
    if (!png) return;
    setMsg(null);
    await api
      .savePngToPhotos(`fitledger-${slug(title)}-${today()}`, png.split(",")[1])
      .then(() => setMsg("Saved to Photos (Pictures/FitLedger)."))
      .catch((e) => setMsg(String(e)));
  }
  async function shareSheet() {
    if (!png) return;
    if (IS_ANDROID) {
      await api.sharePng(`fitledger-${slug(title)}`, png.split(",")[1]).catch((e) => setMsg(String(e)));
      return;
    }
    const file = new File([base64ToBytes(png.split(",")[1])], `${slug(title)}.png`, { type: "image/png" });
    try {
      await navigator.share({ files: [file], title });
    } catch {
      /* cancelled */
    }
  }
  const canShare =
    typeof navigator.canShare === "function" && navigator.canShare({ files: [new File([""], "x.png", { type: "image/png" })] });

  return (
    <Modal title="Share as image" onClose={onClose} wide>
      <div className="share-layout">
        <div className="share-preview">{png ? <img src={png} alt="Preview" /> : <div className="muted small">{msg ?? "Rendering…"}</div>}</div>
        <div className="share-options">
          <div className="row">
            {/* Android's WebView has no navigator.share; the native share sheet is always there. */}
            {(IS_ANDROID || canShare) && (
              <button className="button primary" disabled={!png} onClick={shareSheet}>
                Share…
              </button>
            )}
            {IS_ANDROID && (
              <button className="button" disabled={!png} onClick={saveToPhotos}>
                Save to Photos
              </button>
            )}
            {!IS_ANDROID && (
              <>
                <button className={canShare ? "button" : "button primary"} disabled={!png} onClick={saveFile}>
                  Save PNG
                </button>
                <button className="button" disabled={!png} onClick={copy}>
                  Copy
                </button>
              </>
            )}
          </div>
          {msg && <p className="small success">{msg}</p>}
          <label>
            Title
            <input value={title} onChange={(e) => setTitle(e.target.value)} />
          </label>
          <label>
            Subtitle / date range
            <input value={subtitle} onChange={(e) => setSubtitle(e.target.value)} />
          </label>
          <div className="segmented small">
            <button className={style === "dark" ? "active" : ""} onClick={() => setStyle("dark")}>
              Dark
            </button>
            <button className={style === "light" ? "active" : ""} onClick={() => setStyle("light")}>
              Light
            </button>
          </div>
          <div className="segmented small">
            <button className={size === "square" ? "active" : ""} onClick={() => setSize("square")}>
              Square 1080×1080
            </button>
            <button className={size === "story" ? "active" : ""} onClick={() => setSize("story")}>
              Story 1080×1920
            </button>
          </div>
          <label className="check">
            <input type="checkbox" checked={hide} onChange={(e) => setHide(e.target.checked)} /> Hide exact numbers
          </label>
        </div>
      </div>
      {/* Off-screen render target for the visual. */}
      <div ref={stage} style={{ position: "fixed", left: -20000, top: 0, width: visualW }} aria-hidden>
        {content?.visual?.(theme, visualW, visualH, hide)}
      </div>
    </Modal>
  );
}

function slug(s: string) {
  return (
    s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "fitledger"
  );
}

function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const out = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function esc(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function composeSvg(o: {
  theme: ChartTheme;
  H: number;
  title: string;
  subtitle: string;
  content: CardContent;
  hide: boolean;
  visuals: SVGSVGElement[];
  top: number;
  visualW: number;
  visualH: number;
}) {
  const { theme, H, content } = o;
  const font = "system-ui, -apple-system, Segoe UI, Roboto, sans-serif";
  let y = PAD + 70;
  const parts: string[] = [];
  parts.push(`<rect width="${W}" height="${H}" fill="${theme.surface}"/>`);
  parts.push(
    `<text x="${PAD}" y="${y}" font-size="64" font-weight="700" fill="${theme.text}" font-family="${font}">${esc(fit(o.title, 64))}</text>`,
  );
  y += 56;
  parts.push(`<text x="${PAD}" y="${y}" font-size="34" fill="${theme.muted}" font-family="${font}">${esc(fit(o.subtitle, 34))}</text>`);
  y += 30;
  if (content.stats.length) {
    const colW = (W - PAD * 2) / content.stats.length;
    // Shrink the numbers so the longest value fits its column (≈0.58 em per character).
    const longest = Math.max(...content.stats.map((x) => x.value.length));
    const size = Math.min(60, Math.floor((colW - 24) / (longest * 0.58)));
    content.stats.forEach((s, i) => {
      const x = PAD + i * colW;
      const value = o.hide && /\d/.test(s.value) && !s.label.match(/workouts|sets|PRs/i) ? "•••" : s.value;
      parts.push(
        `<text x="${x}" y="${y + 80}" font-size="${size}" font-weight="700" fill="${theme.text}" font-family="${font}">${esc(value)}</text>`,
      );
      parts.push(`<text x="${x}" y="${y + 120}" font-size="28" fill="${theme.muted}" font-family="${font}">${esc(s.label)}</text>`);
    });
    y += 150;
  }
  content.lines.forEach((l) => {
    y += 44;
    parts.push(
      `<text x="${PAD}" y="${y}" font-size="30" fill="${theme.text}" font-family="${font}">${esc(fit(o.hide ? l.replace(/[\d.,]+\s*(lb|kg)/g, "•••") : l, 30))}</text>`,
    );
  });
  // Inline the rendered visual(s), stacked.
  let vy = o.top;
  for (const svg of o.visuals) {
    const vb = svg.getAttribute("viewBox") ?? `0 0 ${svg.getAttribute("width")} ${svg.getAttribute("height")}`;
    const [, , vw, vh] = vb.split(/\s+/).map(Number);
    const h = Math.min(o.visualH - (vy - o.top), (o.visualW / vw) * vh);
    if (h < 40) break;
    parts.push(
      `<svg x="${PAD}" y="${vy}" width="${o.visualW}" height="${h}" viewBox="${vb}" preserveAspectRatio="xMidYMid meet">${svg.innerHTML}</svg>`,
    );
    vy += h + 16;
  }
  parts.push(
    `<text x="${W - PAD}" y="${H - PAD / 2}" font-size="26" fill="${theme.muted}" text-anchor="end" font-family="${font}">FitLedger</text>`,
  );
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">${parts.join("")}</svg>`;
}

function rasterize(svg: string, w: number, h: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      c.width = w;
      c.height = h;
      c.getContext("2d")!.drawImage(img, 0, 0, w, h);
      resolve(c.toDataURL("image/png"));
    };
    img.onerror = reject;
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
  });
}

// ---- What each card shows ------------------------------------------------------------------------

async function buildContent(spec: ShareKind, unit: "kg" | "lb"): Promise<CardContent> {
  const w = (kg: number) => `${formatWeight(kg, unit)} ${unit}`;
  const muscles = (await api.muscles()).muscles;
  const names = Object.fromEntries(muscles.map((m: Muscle) => [m.id, m.name]));
  switch (spec.kind) {
    case "chart": {
      const all = spec.chart.series.flatMap((s) => s.points.map((p) => p.t.getTime()));
      const day = (ms: number) => new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
      const lo = all.reduce((a, b) => Math.min(a, b), Infinity);
      const hi = all.reduce((a, b) => Math.max(a, b), -Infinity);
      return {
        title: spec.title,
        subtitle: all.length ? `${day(lo)} – ${day(hi)}` : "",
        stats: [],
        lines: [],
        visual: (theme, width, h, hide) => (
          <TimeSeriesSvg {...spec.chart} theme={theme} width={width} height={h} hideNumbers={hide} fontScale={2} />
        ),
      };
    }
    case "workout": {
      const d = (await api.workoutDetail(spec.workoutId)) as WorkoutDetail;
      const values = Object.fromEntries(d.muscle_sets);
      return {
        title: d.name,
        subtitle: formatDate(d.started_at_local),
        stats: [
          ...(d.duration_s ? [{ label: "⏱ Time", value: formatDuration(d.duration_s) }] : []),
          { label: "🏋 Volume", value: formatVolume(d.volume_kg, unit) },
          { label: "Sets", value: String(d.exercises.reduce((n, e) => n + e.sets.length, 0)) },
          { label: "🏆 PRs", value: String(d.prs.length) },
        ],
        // Like Strong: the exercise, then a small trophy tag per kind of PR (no numbers).
        lines: groupPrs(d.prs)
          .slice(0, 6)
          .map(([n, ps]) => `${n}   ${prTags(ps).map((t) => `🏆 ${t}`).join("  ")}`),
        visual: (theme, _w, h, hide) => (
          <BodyMap values={values} theme={theme} height={h} showLegend={false} muscleNames={names} hideNumbers={hide} />
        ),
      };
    }
    case "exercise": {
      const s = (await api.exerciseStats(spec.exerciseId)) as ExerciseStats;
      const pts = s.sessions.filter((x) => x.best_e1rm).map((x) => ({ t: parseLocal(x.at), v: toUnit(x.best_e1rm!, unit) }));
      const yearStart = `${new Date().getFullYear()}-01-01`;
      const first = s.sessions.find((x) => x.at >= yearStart && x.best_e1rm);
      const now = s.best_e1rm;
      const gain = first && now ? now.value - first.best_e1rm! : null;
      return {
        title: `${s.name}`,
        subtitle: pts.length ? `${formatDate(s.sessions[0].at)} – ${formatDate(s.sessions[s.sessions.length - 1].at)}` : "",
        stats: [
          ...(now ? [{ label: "Best est. 1RM", value: w(now.value) }] : []),
          ...(gain != null
            ? [
                {
                  label: `since Jan ${new Date().getFullYear()}`,
                  value: `${gain >= 0 ? "+" : "−"}${formatWeight(Math.abs(gain), unit)} ${unit}`,
                },
              ]
            : []),
          { label: "Sessions", value: String(s.sessions.length) },
        ],
        lines: [],
        visual: (theme, width, h, hide) => (
          <TimeSeriesSvg
            series={[{ id: "e", label: "e1RM", points: pts }]}
            unit={unit}
            theme={theme}
            width={width}
            height={h}
            hideNumbers={hide}
            smooth={{ on: true, span: 0.3 }}
            fontScale={2}
          />
        ),
      };
    }
    case "bodymap": {
      const m = await api.muscleSets(spec.range.from ?? undefined, spec.range.to ?? undefined);
      const values = Object.fromEntries(m.map((x) => [x.muscle_id, x.sets]));
      const top3 = [...m].sort((a, b) => b.sets - a.sets).slice(0, 3);
      return {
        title: "Muscles trained",
        subtitle: spec.range.label,
        stats: [],
        lines: top3.map((x) => `${names[x.muscle_id]}: ${Math.round(x.sets)} sets`),
        visual: (theme, _w, h, hide) => <ShareBodyMap values={values} theme={theme} height={h} names={names} hide={hide} />,
      };
    }
    case "bodyweight": {
      const rows = ((await api.weighIns(false)) as WeighInRow[]).filter((r) => inRange(r.at, spec.range));
      const pts = rows.map((r) => ({ t: parseLocal(r.at), v: toUnit(r.weight_kg, unit) }));
      const change = rows.length > 1 ? rows[rows.length - 1].weight_kg - rows[0].weight_kg : null;
      return {
        title: "Bodyweight",
        subtitle: rows.length ? `${formatDate(rows[0].at)} – ${formatDate(rows[rows.length - 1].at)}` : spec.range.label,
        stats:
          change != null ? [{ label: "Change", value: `${change >= 0 ? "+" : "−"}${formatWeight(Math.abs(change), unit)} ${unit}` }] : [],
        lines: [],
        visual: (theme, width, h, hide) => (
          <TimeSeriesSvg
            series={[{ id: "bw", label: "Bodyweight", points: pts }]}
            unit={unit}
            theme={theme}
            width={width}
            height={h}
            hideNumbers={hide}
            smooth={{ on: true, span: 0.15 }}
            fontScale={2}
            yFormat={(v) => v.toFixed(0)}
          />
        ),
      };
    }
    case "inbody": {
      const scans = (await api.inbodyScans()) as InBodyScan[];
      const s = (c: string) =>
        scans.filter((x) => x.values[c] != null).map((x) => ({ t: parseLocal(x.measured_at_local), v: toUnit(x.values[c], unit) }));
      const last = scans[scans.length - 1];
      return {
        title: "Body composition",
        subtitle: scans.length ? `${formatDate(scans[0].measured_at_local)} – ${formatDate(last.measured_at_local)}` : "",
        stats: last
          ? [
              ...(last.values.smm_kg != null ? [{ label: "Muscle (SMM)", value: w(last.values.smm_kg) }] : []),
              ...(last.values.pbf != null ? [{ label: "Body fat", value: `${last.values.pbf}%` }] : []),
            ]
          : [],
        lines: [],
        visual: (theme, width, h, hide) => (
          <TimeSeriesSvg
            series={[
              { id: "smm", label: "Skeletal muscle mass", points: s("smm_kg") },
              { id: "bfm", label: "Body fat mass", points: s("body_fat_mass_kg") },
            ]}
            unit={unit}
            theme={theme}
            width={width}
            height={h}
            hideNumbers={hide}
            smooth={{ on: false, span: 0.3 }}
            fontScale={2}
          />
        ),
      };
    }
    case "count": {
      const t = today();
      const months = await api.workoutsPerPeriod(true, t);
      const year = new Date().getFullYear();
      const thisYear = months.filter((m) => m.start.startsWith(String(year))).reduce((n, m) => n + m.workouts, 0);
      const thisMonth = months[months.length - 1]?.workouts ?? 0;
      const weeks = await api.workoutsPerPeriod(false, t);
      const streak = await api.streak(t);
      return {
        title: `${thisYear} workouts in ${year}`,
        subtitle: new Date().toLocaleDateString(undefined, { month: "long", day: "numeric", year: "numeric" }),
        stats: [
          { label: "This month", value: String(thisMonth) },
          { label: "This week", value: String(weeks[weeks.length - 1]?.workouts ?? 0) },
          { label: "Week streak", value: String(streak.current_weeks) },
        ],
        lines: [],
        visual: null,
      };
    }
  }
}

/** Body map plus an SVG legend, so the whole thing survives being inlined into the card. */
function ShareBodyMap({
  values,
  theme,
  height,
  names,
  hide,
}: {
  values: Record<string, number>;
  theme: ChartTheme;
  height: number;
  names: Record<string, string>;
  hide: boolean;
}) {
  const max = Math.max(0, ...Object.values(values));
  const color = useHeatScale(theme, max);
  return (
    <>
      <BodyMap values={values} theme={theme} height={height - 60} showLegend={false} muscleNames={names} hideNumbers={hide} />
      <svg viewBox="0 0 600 60" width={600} height={60}>
        <defs>
          <linearGradient id="share-ramp">
            {[0.05, 0.25, 0.5, 0.75, 1].map((f) => (
              <stop key={f} offset={f} stopColor={color(f * (max || 1)) ?? theme.grid} />
            ))}
          </linearGradient>
        </defs>
        <text x={110} y={36} fontSize={22} fill={theme.muted} textAnchor="end" fontFamily="system-ui">
          fewer
        </text>
        <rect x={130} y={18} width={340} height={22} rx={11} fill="url(#share-ramp)" />
        <text x={490} y={36} fontSize={22} fill={theme.muted} fontFamily="system-ui">
          {hide ? "more sets" : `${Math.round(max)} sets`}
        </text>
      </svg>
    </>
  );
}
