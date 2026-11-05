import { createContext, useContext, useEffect, useState } from "react";
import { autoSyncHealthConnect } from "./components/WearablesCard";
import { api, type WeightUnit } from "./api";
import BodyPage from "./pages/BodyPage";
import DataPage from "./pages/DataPage";
import ExercisesPage from "./pages/ExercisesPage";
import HistoryPage from "./pages/HistoryPage";
import PlanPage from "./pages/PlanPage";
import ReviewPage from "./pages/ReviewPage";
import SettingsPage from "./pages/SettingsPage";
import StatsPage from "./pages/StatsPage";
import CardioPage from "./pages/CardioPage";
import { loadWeighInPlan, syncWeighInReminder } from "./weighIn";
import WorkoutPage from "./pages/WorkoutPage";

export type Theme = "system" | "light" | "dark";

interface Prefs {
  unit: WeightUnit;
  theme: Theme;
  setUnit: (u: WeightUnit) => void;
  setTheme: (t: Theme) => void;
}

const PrefsContext = createContext<Prefs>({ unit: "lb", theme: "system", setUnit: () => {}, setTheme: () => {} });
export const usePrefs = () => useContext(PrefsContext);

// Routes live in the URL hash so Android's back button and desktop history work.
export function navigate(path: string) {
  window.location.hash = path;
}

function useRoute(): string {
  const read = () => window.location.hash.replace(/^#/, "") || "/today";
  const [route, setRoute] = useState(read);
  useEffect(() => {
    const on = () => setRoute(read());
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return route;
}

const ICONS: Record<string, string> = {
  today: "M4 6h16M4 12h16M4 18h10",
  workout: "M6 8v8M18 8v8M3 10v4M21 10v4M6 12h12",
  history: "M6 8v8M18 8v8M3 10v4M21 10v4M6 12h12",
  stats: "M4 20V10M10 20V4M16 20v-7M22 20H2",
  body: "M3 7h18v10H3zM7 7v4M11 7v6M15 7v4M19 7v6",
  exercises: "M4 6h16M4 12h16M4 18h16M8 3v6M16 9v6M10 15v6",
  plan: "M8 2v4M16 2v4M3 9h18M5 5h14a2 2 0 0 1 2 2v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z",
  data: "M12 3v12m0 0-4-4m4 4 4-4M5 21h14",
  settings:
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM4 12h2m12 0h2M12 4v2m0 12v2M6.3 6.3l1.4 1.4m8.6 8.6 1.4 1.4m0-11.4-1.4 1.4m-8.6 8.6-1.4 1.4",
  more: "M5 12h.01M12 12h.01M19 12h.01",
};

// Four tabs, the same on every screen size. Each route belongs to one tab.
const TABS = ["history", "stats", "body", "settings"];
const LABELS: Record<string, string> = {
  history: "Workouts",
  stats: "Progress",
  body: "Measure",
  settings: "Settings",
};
const TAB_OF: Record<string, string> = {
  history: "history",
  workout: "history",
  today: "history",
  stats: "stats",
  exercises: "stats",
  cardio: "stats",
  body: "body",
  settings: "settings",
  plan: "settings",
  data: "settings",
  review: "settings",
  more: "settings",
};

export default function App() {
  const route = useRoute();
  const [unit, setUnitState] = useState<WeightUnit>("lb");
  const [theme, setThemeState] = useState<Theme>("system");
  const [active, setActive] = useState<number | null>(null);

  useEffect(() => {
    api.getSetting("weight_unit").then((u) => u && setUnitState(u as WeightUnit));
    api.getSetting("theme").then((t) => t && setThemeState(t as Theme));
    loadWeighInPlan().then(syncWeighInReminder);
    // Automatic backup, a few seconds after start so it doesn't slow the first screen.
    const t = setTimeout(() => api.autoBackup(false).catch(() => {}), 5000);
    // New data from the watch (Health Connect), once connected.
    const t2 = setTimeout(() => autoSyncHealthConnect().catch(() => {}), 3000);
    return () => {
      clearTimeout(t);
      clearTimeout(t2);
    };
  }, []);
  useEffect(() => {
    api.activeWorkout().then(setActive);
  }, [route]);
  // Opening the app (or tapping the rest notification) with a workout in progress goes straight to it.
  useEffect(() => {
    api.activeWorkout().then((id) => {
      if (id && (window.location.hash === "" || window.location.hash === "#/" || window.location.hash === "#/history"))
        navigate("/workout");
    });
  }, []);

  useEffect(() => {
    if (theme === "system") document.documentElement.removeAttribute("data-theme");
    else document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  const prefs: Prefs = {
    unit,
    theme,
    setUnit: (u) => {
      setUnitState(u);
      api.setSetting("weight_unit", u);
    },
    setTheme: (t) => {
      setThemeState(t);
      api.setSetting("theme", t);
    },
  };

  const [section, a, b] = route.split("/").filter(Boolean);
  let page;
  switch (section) {
    case "workout":
      page = active ? <WorkoutPage key={active} id={active} mode="live" /> : <HistoryPage selectedId={null} view="detail" />;
      break;
    case "history":
      if (a && b === "edit") page = <WorkoutPage key={`e${a}`} id={Number(a)} mode="edit" />;
      else page = <HistoryPage selectedId={a ? Number(a) : null} view={b === "summary" ? "summary" : "detail"} />;
      break;
    case "exercises":
      page = <ExercisesPage selectedId={a ? Number(a) : null} />;
      break;
    case "stats":
      page = <StatsPage />;
      break;
    case "cardio":
      page = <CardioPage log={a === "log"} />;
      break;
    case "body":
      page = <BodyPage tab={((a === "fat" ? "inbody" : a) as "weight" | "inbody" | "diet" | "parts") ?? "weight"} part={b} />;
      break;
    case "plan":
      page = (
        <PlanPage
          tab={a === "template" ? "templates" : ((a as "templates") ?? "templates")}
          templateId={a === "template" ? (b === "new" ? "new" : Number(b)) : null}
        />
      );
      break;
    case "data":
      page = <DataPage />;
      break;
    case "review":
      page = <ReviewPage />;
      break;
    case "settings":
      page = <SettingsPage />;
      break;
    default:
      page = <HistoryPage selectedId={null} view="detail" />;
  }
  const current = TAB_OF[section ?? "history"] ?? "history";

  return (
    <PrefsContext.Provider value={prefs}>
      <div className="shell">
        <nav className="nav desktop-nav" aria-label="Main">
          <div className="brand">FitLedger</div>
          {TABS.map((t) => (
            <a key={t} href={`#/${t}`} className={current === t ? "nav-item active" : "nav-item"}>
              <NavIcon d={ICONS[t]} />
              <span>{LABELS[t]}</span>
            </a>
          ))}
        </nav>
        <main className="main">
          {active && section !== "workout" && current !== "history" && (
            <a className="live-banner" href="#/workout">
              Workout in progress · tap to resume
            </a>
          )}
          {page}
        </main>
        <nav className="nav phone-nav" aria-label="Main">
          {TABS.map((t) => (
            <a key={t} href={`#/${t}`} className={current === t ? "nav-item active" : "nav-item"}>
              <NavIcon d={ICONS[t]} />
              <span>{LABELS[t]}</span>
            </a>
          ))}
        </nav>
      </div>
    </PrefsContext.Provider>
  );
}

function NavIcon({ d }: { d: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}
