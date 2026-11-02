import { useEffect, useState } from "react";

/** Colors a chart needs, resolved to hex so a chart can be serialized to an image. */
export interface ChartTheme {
  mode: "light" | "dark";
  surface: string;
  text: string;
  muted: string;
  grid: string;
  gridStrong: string;
  line: string;
  band: string[];
  categorical: string[];
  sequential: string[];
}

// Validated reference palette (dataviz skill): fixed categorical order, separate light/dark steps.
export const CATEGORICAL_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
export const CATEGORICAL_DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];
// Sequential blue, light → dark (near zero recedes toward the surface).
const SEQ = [
  "#cde2fb",
  "#b7d3f6",
  "#9ec5f4",
  "#86b6ef",
  "#6da7ec",
  "#5598e7",
  "#3987e5",
  "#2a78d6",
  "#256abf",
  "#1c5cab",
  "#184f95",
  "#104281",
  "#0d366b",
];

export const LIGHT: ChartTheme = {
  mode: "light",
  surface: "#ffffff",
  text: "#16191e",
  muted: "#5d6572",
  grid: "#eceef2",
  gridStrong: "#d5d9e0",
  line: "#5a4bd1",
  band: ["rgba(42,120,214,0.07)", "rgba(235,104,52,0.08)"],
  categorical: CATEGORICAL_LIGHT,
  sequential: SEQ,
};

export const DARK: ChartTheme = {
  mode: "dark",
  surface: "#181b21",
  text: "#e7e9ee",
  muted: "#98a0ac",
  grid: "#23272e",
  gridStrong: "#343a44",
  line: "#9a8fff",
  band: ["rgba(57,135,229,0.10)", "rgba(217,89,38,0.11)"],
  categorical: CATEGORICAL_DARK,
  // On dark, magnitude runs from the surface toward bright blue.
  sequential: [...SEQ].reverse(),
};

function currentMode(): "light" | "dark" {
  const attr = document.documentElement.getAttribute("data-theme");
  if (attr === "light" || attr === "dark") return attr;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Follows the app theme (system or chosen). */
export function useChartTheme(): ChartTheme {
  const [mode, setMode] = useState(currentMode);
  useEffect(() => {
    const update = () => setMode(currentMode());
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", update);
    const obs = new MutationObserver(update);
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      mq.removeEventListener("change", update);
      obs.disconnect();
    };
  }, []);
  return mode === "dark" ? DARK : LIGHT;
}

/** Template colors are stored as their light-mode hex; show the matching dark step in dark mode. */
export function templateColor(stored: string | null | undefined, theme: ChartTheme, fallbackIndex = 0): string {
  const i = stored ? CATEGORICAL_LIGHT.indexOf(stored.toLowerCase()) : -1;
  if (i >= 0) return theme.categorical[i];
  return stored ?? theme.categorical[fallbackIndex % theme.categorical.length];
}
