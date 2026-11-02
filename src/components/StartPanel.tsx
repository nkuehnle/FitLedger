import { useEffect, useState } from "react";
import { api, type StartFrom, type Suggestion, type Template } from "../api";
import { navigate } from "../App";
import { today } from "../charts/RangePicker";
import { templateColor, useChartTheme } from "../charts/theme";
import { parseLocal } from "../format";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const dayName = (iso: string) => DAYS[(parseLocal(iso).getDay() + 6) % 7];

/** Top of the Workouts tab: resume, or start the suggested workout in one tap. */
export function StartPanel() {
  const theme = useChartTheme();
  const [active, setActive] = useState<number | null | undefined>(undefined);
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null);
  const [templates, setTemplates] = useState<Template[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.activeWorkout().then(setActive);
    api.suggestion(today()).then(setSuggestion);
    api.listTemplates().then(setTemplates);
  }, []);

  async function start(from: StartFrom) {
    setError(null);
    try {
      await api.startWorkout(from);
      navigate("/workout");
    } catch (e) {
      setError(String(e));
    }
  }

  if (active === undefined) return null;
  if (active)
    return (
      <button className="card start-panel resume" onClick={() => navigate("/workout")}>
        <span className="muted small">In progress</span>
        <span className="start-title">Resume workout ›</span>
      </button>
    );

  const tpl = (id: number) => templates.find((t) => t.id === id);
  const items = suggestion?.items.filter((i) => tpl(i.template_id)) ?? [];
  const top = items[0];
  const next = !top && suggestion?.next && tpl(suggestion.next.template_id) ? suggestion.next : null;
  const main = top ?? next;
  const mainTpl = main && tpl(main.template_id);
  const label = !main
    ? null
    : main.reason === "missed"
      ? `Missed on ${dayName(main.planned_date)}`
      : main.reason === "today"
        ? "Today"
        : `${suggestion?.done_today ? "Done for today · " : "Rest day · "}next on ${dayName(main.planned_date)}`;

  return (
    <section className="card start-panel" style={mainTpl ? { borderColor: templateColor(mainTpl.color, theme) } : undefined}>
      {mainTpl ? (
        <div className="start-main">
          <div>
            <div className="muted small">{label}</div>
            <div className="start-title">{mainTpl.name}</div>
          </div>
          <button className="button primary" onClick={() => start({ kind: "template", template_id: mainTpl.id! })}>
            Start
          </button>
        </div>
      ) : (
        <div className="start-main">
          <div className="muted small">No schedule yet. Open any past workout and tap “Perform again”, or start empty.</div>
          <button className="button primary" onClick={() => start({ kind: "empty" })}>
            Start empty
          </button>
        </div>
      )}
      <details className="start-more">
        <summary className="small">Something else</summary>
        {items.slice(1).map((s) => (
          <button key={`s${s.template_id}`} className="start-row" onClick={() => start({ kind: "template", template_id: s.template_id })}>
            <span>
              {tpl(s.template_id)!.name} <span className="muted small">· missed {dayName(s.planned_date)}</span>
            </span>
            <span className="muted">Start ›</span>
          </button>
        ))}
        {templates
          .filter((t) => t.id !== mainTpl?.id && !items.slice(1).some((i) => i.template_id === t.id))
          .map((t, i) => (
            <button key={t.id} className="start-row" onClick={() => start({ kind: "template", template_id: t.id! })}>
              <span>
                <i className="dot" style={{ background: templateColor(t.color, theme, i), marginRight: 8 }} />
                {t.name}
              </span>
              <span className="muted">Start ›</span>
            </button>
          ))}
        <button className="start-row" onClick={() => start({ kind: "empty" })}>
          <span>Empty workout</span>
          <span className="muted">Start ›</span>
        </button>
        <a className="start-row" href="#/plan/schedule">
          <span>Edit plan & schedule</span>
          <span className="muted">›</span>
        </a>
      </details>
      {error && <p className="error small">{error}</p>}
    </section>
  );
}
