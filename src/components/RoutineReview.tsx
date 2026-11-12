import { useEffect, useState } from "react";
import { api, type RoutineFinding, type RoutineReview, type Template } from "../api";
import { tell } from "./Modal";

const SEVERITY_TEXT: Record<RoutineFinding["severity"], string> = { high: "Worth fixing", medium: "Worth a look", low: "Minor" };

/** Plan → Review: what the active schedule is short on or stuck at, each with a change you can apply. */
export function RoutineReviewTab() {
  const [review, setReview] = useState<RoutineReview | null>(null);
  // The last applied change: what it did, and the template as it was.
  const [applied, setApplied] = useState<{ label: string; before: Template } | null>(null);
  const [busy, setBusy] = useState(false);
  const load = () => api.routineReview().then(setReview);
  useEffect(() => {
    load();
  }, []);

  async function run(work: () => Promise<void>) {
    setBusy(true);
    try {
      await work();
      await load();
    } catch (e) {
      tell(String(e));
    } finally {
      setBusy(false);
    }
  }
  const apply = (f: RoutineFinding) =>
    run(async () => {
      const before = await api.applyRoutineFix(f.fix!);
      setApplied({ label: f.fix_label!, before });
    });
  const undo = () =>
    run(async () => {
      await api.saveTemplate(applied!.before);
      setApplied(null);
    });

  if (!review) return null;
  if (!review.schedule) {
    return (
      <section className="card">
        <p className="muted small">
          The review checks the routine you're following. Set one up under <a href="#/plan/schedule">Schedule</a> first.
        </p>
      </section>
    );
  }
  return (
    <>
      <section className="card">
        <p className="muted small" style={{ margin: 0 }}>
          A check of <strong>{review.schedule}</strong> against your <a href="#/plan/targets">weekly set targets</a> and your recent
          training: muscles short of or over their sets, lifts that have stopped moving, near-identical exercises, exercises slipping late
          in a session, and joints that keep complaining. Each change edits one template, and you can undo it.
        </p>
      </section>
      {applied && (
        <section className="card callout review-applied">
          <span className="small">
            <strong>Done:</strong> {applied.label}
          </span>
          <button className="button small" disabled={busy} onClick={undo}>
            Undo
          </button>
        </section>
      )}
      {review.findings.length === 0 && (
        <section className="card">
          <p className="empty">Nothing to change: every muscle is inside its target and nothing looks stuck.</p>
        </section>
      )}
      {review.findings.map((f, i) => (
        <section key={`${f.kind}-${f.muscle_id}-${f.exercise_id}-${f.template_id}-${i}`} className="card review-finding">
          <div className={`review-severity small ${f.severity}`}>{SEVERITY_TEXT[f.severity]}</div>
          <h2 className="section-title">{f.title}</h2>
          <p className="small">{f.detail}</p>
          {f.fix && (
            <button className="button small" disabled={busy} onClick={() => apply(f)}>
              {f.fix_label}
            </button>
          )}
          {!f.fix && f.template_id != null && (
            <a className="link-btn small" href={`#/plan/template/${f.template_id}`}>
              Open the template ›
            </a>
          )}
        </section>
      ))}
    </>
  );
}
