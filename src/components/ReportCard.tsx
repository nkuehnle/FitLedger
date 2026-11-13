import { useState } from "react";
import { api } from "../api";
import { today } from "../charts/RangePicker";
import { saveOrShareText } from "../platform";

const WEEKS = [4, 8, 12];

/** A written progress report to share or copy. */
export function ReportCard() {
  const [weeks, setWeeks] = useState(8);
  const [body, setBody] = useState(true);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = async (f: (report: string) => Promise<string | null>) => {
    setMsg(null);
    setError(null);
    try {
      setMsg(await f(await api.progressReport(weeks, body)));
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <section className="card">
      <h2 className="section-title">Progress report</h2>
      <p className="muted small">
        FitLedger never sends anything by itself. The report goes only where you send it, and includes health data if you leave that box
        ticked.
      </p>
      <div className="segmented">
        {WEEKS.map((w) => (
          <button key={w} className={weeks === w ? "active" : ""} onClick={() => setWeeks(w)}>
            {w} weeks
          </button>
        ))}
      </div>
      <label className="check small">
        <input type="checkbox" checked={body} onChange={(e) => setBody(e.target.checked)} />
        Include weight, measurements, nutrition and recovery
      </label>
      <div className="row">
        <button
          className="button primary"
          onClick={() =>
            run(async (report) => {
              const saved = await saveOrShareText(`fitledger-report-${today()}.md`, "text/plain", report);
              return saved && `Saved ${saved}`;
            })
          }
        >
          Share report
        </button>
        <button
          className="button"
          onClick={() =>
            run(async (report) => {
              await api.copyText(report);
              return "Report copied.";
            })
          }
        >
          Copy report
        </button>
      </div>
      {msg && <p className="success small">{msg}</p>}
      {error && <p className="error">{error}</p>}
    </section>
  );
}
