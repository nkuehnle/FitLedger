import { useEffect, useState } from "react";
import { api, type MappingRule, type NameReview, type PerHandSuspect, type SuggestionView } from "../api";
import { usePrefs } from "../App";
import { EQUIPMENT_LABELS, formatDate, formatWeight } from "../format";

export default function ReviewPage() {
  const [items, setItems] = useState<NameReview[] | null>(null);
  const [showAll, setShowAll] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = () =>
    api
      .nameReview()
      .then(setItems)
      .catch((e) => setError(String(e)));
  useEffect(() => {
    reload();
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!items) return <p className="muted page">Loading…</p>;

  const pending = items.filter((i) => i.needs_review).length;
  const shown = showAll || pending === 0 ? items : items.filter((i) => i.needs_review);

  return (
    <div className="page narrow">
      <a className="back-link" href="#/data">
        ‹ Data
      </a>
      <h1 className="page-title">Exercise names</h1>
      <p className="muted small">
        Each imported name is linked to a library exercise so stats and muscle counts work. If a name meant different exercises at different
        times, add a rule for just that date range. The notes you wrote can help you tell.
      </p>
      <div className="segmented" role="tablist">
        <button className={!showAll ? "active" : ""} onClick={() => setShowAll(false)}>
          Needs review ({pending})
        </button>
        <button className={showAll ? "active" : ""} onClick={() => setShowAll(true)}>
          All ({items.length})
        </button>
      </div>
      <PerHand />
      {shown.length === 0 && <p className="empty">Nothing needs review.</p>}
      {shown.map((item) => (
        <ReviewItem key={item.source_name} item={item} onChange={reload} />
      ))}
    </div>
  );
}

function rangeText(r: { from_date: string | null; to_date: string | null }): string {
  if (r.from_date && r.to_date) return `${r.from_date} to ${r.to_date}`;
  if (r.from_date) return `from ${r.from_date}`;
  if (r.to_date) return `until ${r.to_date}`;
  return "all dates";
}

function ReviewItem({ item, onChange }: { item: NameReview; onChange: () => void }) {
  const [editing, setEditing] = useState(item.needs_review);
  const [ranged, setRanged] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SuggestionView[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!query.trim()) return setResults([]);
    const t = setTimeout(() => api.searchExercises(query).then(setResults), 150);
    return () => clearTimeout(t);
  }, [query]);

  async function choose(slug: string) {
    setError(null);
    try {
      await api.setNameMapping(item.source_name, slug, ranged && from ? from : undefined, ranged && to ? to : undefined);
      setQuery("");
      setEditing(false);
      onChange();
    } catch (e) {
      setError(String(e));
    }
  }

  const act = (f: () => Promise<void>) => () => f().then(onChange, (e) => setError(String(e)));

  return (
    <section className={item.needs_review ? "card review-item attention" : "card review-item"}>
      <header>
        <div className="card-title">{item.source_name}</div>
        <div className="muted small">
          {item.sessions} sessions · {item.first_date} – {item.last_date}
        </div>
      </header>

      {item.rules.map((r: MappingRule) => (
        <div key={r.id} className="rule">
          <div>
            → <strong>{r.exercise_name}</strong> <span className="muted small">({rangeText(r)})</span>
            {r.confirmed ? (
              <span className="chip ok">confirmed</span>
            ) : (
              <span className="chip">auto · {Math.round((r.confidence ?? 0) * 100)}%</span>
            )}
          </div>
          <div className="row">
            {!r.confirmed && (
              <button className="button small" onClick={act(() => api.confirmNameMapping(r.id))}>
                Confirm
              </button>
            )}
            <button className="button small" onClick={act(() => api.removeNameMapping(r.id))}>
              Remove
            </button>
          </div>
        </div>
      ))}
      {item.unmapped_sessions > 0 && <p className="warn small">{item.unmapped_sessions} sessions aren't linked to an exercise yet.</p>}

      {item.note_eras.some((e) => e.note) && (
        <details className="eras">
          <summary className="small">Your notes over time</summary>
          <ul>
            {item.note_eras
              .filter((e) => e.note)
              .map((e, i) => (
                <li key={i} className="small">
                  <span className="muted">
                    {e.from === e.to ? e.from : `${e.from} – ${e.to}`} · {e.sessions}×
                  </span>{" "}
                  “{e.note}”
                </li>
              ))}
          </ul>
        </details>
      )}

      {!editing ? (
        <button className="button small" onClick={() => setEditing(true)}>
          Change…
        </button>
      ) : (
        <div className="mapper">
          <label className="check small">
            <input type="checkbox" checked={ranged} onChange={(e) => setRanged(e.target.checked)} />
            Only for a date range
          </label>
          {ranged && (
            <div className="row small">
              <label>
                From <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
              </label>
              <label>
                To <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
              </label>
            </div>
          )}
          <div className="muted small">Suggestions</div>
          <div className="suggestions">
            {item.suggestions.map((s) => (
              <SuggestionButton key={s.slug} s={s} onPick={choose} />
            ))}
          </div>
          <input
            className="search"
            type="search"
            placeholder="Search the library…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {results.length > 0 && (
            <div className="suggestions">
              {results.map((s) => (
                <SuggestionButton key={s.slug} s={s} onPick={choose} />
              ))}
            </div>
          )}
        </div>
      )}
      {error && <p className="error small">{error}</p>}
    </section>
  );
}

function SuggestionButton({ s, onPick }: { s: SuggestionView; onPick: (slug: string) => void }) {
  return (
    <button className="suggestion" onClick={() => onPick(s.slug)}>
      <span>{s.name}</span>
      <span className="muted small">
        {EQUIPMENT_LABELS[s.equipment] ?? s.equipment} · {Math.round(s.confidence * 100)}%
      </span>
    </button>
  );
}

/** Dumbbell sessions that look logged per hand (about half the usual weight). */
function PerHand() {
  const { unit } = usePrefs();
  const [items, setItems] = useState<PerHandSuspect[]>([]);
  const load = () => api.perHandSuspects().then(setItems);
  useEffect(() => {
    load();
  }, []);
  if (items.length === 0) return null;
  const decide = (s: PerHandSuspect, mode: "total" | "per_hand") => api.setSessionLoadMode(s.workout_exercise_id, mode).then(load);
  return (
    <section className="card review-item attention">
      <div className="card-title">Possibly logged per hand</div>
      <p className="muted small">
        These sessions are about half your usual weight. If you entered one dumbbell's weight, mark them “per hand” so volume and records
        count both.
      </p>
      {items.map((s) => (
        <div key={s.workout_exercise_id} className="rule">
          <div>
            <a href={`#/history/${s.workout_id}`}>{s.exercise_name}</a> · {formatDate(s.date)}{" "}
            <span className="muted small">
              {formatWeight(s.max_weight_kg, unit)} {unit} vs usual {formatWeight(s.neighbor_median_kg, unit)}
            </span>
          </div>
          <div className="row">
            <button className="button small" onClick={() => decide(s, "per_hand")}>
              Per hand
            </button>
            <button className="button small" onClick={() => decide(s, "total")}>
              Really lighter
            </button>
          </div>
        </div>
      ))}
    </section>
  );
}
