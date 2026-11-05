import { useEffect, useState } from "react";
import {
  api,
  MISSING_WEIGHT_UNIT,
  type BodyweightPreview,
  type MeasurementPreview,
  type Counts,
  type WeightUnit,
  type WorkoutPreview,
} from "../api";
import { open, save } from "@tauri-apps/plugin-dialog";
import { type BackupInfo } from "../api";
import { formatDate } from "../format";
import { IS_ANDROID, fileToBase64 } from "../platform";
import { today } from "../charts/RangePicker";

type Pending =
  | { kind: "workouts"; fileName: string; text: string; unit?: WeightUnit; preview: WorkoutPreview }
  | { kind: "bodyweight"; fileName: string; text: string; unit?: WeightUnit; preview: BodyweightPreview }
  | { kind: "measurements"; fileName: string; text: string; preview: MeasurementPreview }
  | { kind: "needs-unit"; fileName: string; text: string; target: "workouts" | "bodyweight" };

/** Strong workout exports have an Exercise column, bodyweight exports a Weight column; anything
 *  else (e.g. "Date,Neck (in.)") is a body measurement export. */
function detectKind(text: string): "workouts" | "bodyweight" | "measurements" {
  const header = text.split(/\r?\n/, 1)[0].toLowerCase();
  return header.includes("exercise") ? "workouts" : header.includes("weight") ? "bodyweight" : "measurements";
}

export default function DataPage() {
  const [counts, setCounts] = useState<Counts | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [needsReview, setNeedsReview] = useState(0);

  const refresh = () => {
    api.counts().then(setCounts);
    api.nameReview().then((r) => setNeedsReview(r.filter((x) => x.needs_review).length));
  };
  useEffect(refresh, []);

  async function preview(fileName: string, text: string, kind: "workouts" | "bodyweight" | "measurements", unit?: WeightUnit) {
    setError(null);
    setMessage(null);
    try {
      if (kind === "measurements") {
        setPending({ kind, fileName, text, preview: await api.previewMeasurements(text) });
      } else if (kind === "workouts") {
        setPending({ kind, fileName, text, unit, preview: await api.previewStrongWorkouts(text, unit) });
      } else {
        setPending({ kind, fileName, text, unit, preview: await api.previewBodyweight(text, unit) });
      }
    } catch (e) {
      if (e === MISSING_WEIGHT_UNIT && kind !== "measurements") setPending({ kind: "needs-unit", fileName, text, target: kind });
      else setError(String(e));
    }
  }

  async function onFiles(list: FileList | null) {
    const files = [...(list ?? [])];
    if (files.length === 1) {
      const text = await files[0].text();
      await preview(files[0].name, text, detectKind(text));
      return;
    }
    if (!files.length) return;
    // Strong exports each body measurement on its own, so several of those import in one go.
    setError(null);
    setMessage(null);
    setPending(null);
    const texts = await Promise.all(files.map((f) => f.text()));
    if (texts.some((t) => detectKind(t) !== "measurements")) {
      setError("Pick several files only for body measurements; workouts and bodyweight go one file at a time.");
      return;
    }
    setBusy(true);
    try {
      let inserted = 0;
      let skipped = 0;
      for (const [i, text] of texts.entries()) {
        const r = await api.importMeasurements(text, files[i].name);
        inserted += r.inserted;
        skipped += r.skipped;
      }
      setMessage(`Imported ${inserted} measurements from ${files.length} files` + (skipped ? `, skipped ${skipped} already imported` : "") + ".");
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function commit() {
    if (!pending || pending.kind === "needs-unit") return;
    setBusy(true);
    try {
      if (pending.kind === "workouts") {
        const r = await api.importStrongWorkouts(pending.text, pending.fileName, pending.unit);
        setMessage(
          `Imported ${r.outcome.inserted} workouts (${r.outcome.inserted_sets.toLocaleString()} sets)` +
            (r.outcome.skipped ? `, skipped ${r.outcome.skipped} already imported` : "") +
            `. ${r.auto_mapped} exercise names matched automatically.`,
        );
      } else if (pending.kind === "measurements") {
        const r = await api.importMeasurements(pending.text, pending.fileName);
        setMessage(`Imported ${r.inserted} measurements` + (r.skipped ? `, skipped ${r.skipped} already imported` : "") + ".");
      } else {
        const r = await api.importBodyweight(pending.text, pending.fileName, pending.unit);
        setMessage(`Imported ${r.inserted} weigh-ins` + (r.skipped ? `, skipped ${r.skipped} already imported` : "") + ".");
      }
      setPending(null);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page narrow">
      <a className="back-link" href="#/settings">
        ‹ Settings
      </a>
      <h1 className="page-title">Data</h1>

      {counts && (
        <div className="stats-row">
          <Stat label="Workouts" value={counts.workouts} />
          <Stat label="Sets" value={counts.sets} />
          <Stat label="Exercises" value={counts.exercise_names} />
          <Stat label="Weigh-ins" value={counts.weigh_ins} />
        </div>
      )}

      {needsReview > 0 && (
        <a className="card callout" href="#/review">
          <strong>{needsReview === 1 ? "1 exercise name needs review" : `${needsReview} exercise names need review`}</strong>
          <span className="muted small">Confirm which library exercise each imported name means.</span>
        </a>
      )}

      <section className="card">
        <h2 className="section-title">Strong export</h2>
        <p className="muted small">
          In Strong: Settings → Export data. Choose the workouts, bodyweight or body measurement CSVs (several measurement files at
          once is fine); FitLedger tells them apart. Importing the same file again won't create duplicates.
        </p>
        <label className="button primary file-button">
          Choose CSV files
          <input type="file" accept=".csv,text/csv" multiple onChange={(e) => onFiles(e.target.files)} hidden />
        </label>
      </section>

      {error && <p className="error">{error}</p>}
      {message && (
        <p className="success">
          {message} <a href="#/history">View history</a>
          {needsReview > 0 && (
            <>
              {" · "}
              <a href="#/review">Review exercise names</a>
            </>
          )}
        </p>
      )}

      {pending?.kind === "needs-unit" && (
        <section className="card">
          <h2 className="section-title">{pending.fileName}</h2>
          <p>This file doesn't say which unit its weights are in.</p>
          <div className="row">
            {(["lb", "kg"] as const).map((u) => (
              <button key={u} className="button" onClick={() => preview(pending.fileName, pending.text, pending.target, u)}>
                Weights are in {u}
              </button>
            ))}
          </div>
        </section>
      )}

      {pending && pending.kind !== "needs-unit" && (
        <section className="card">
          <h2 className="section-title">{pending.fileName}</h2>
          {pending.kind === "workouts" ? (
            <dl className="facts">
              <dt>Workouts</dt>
              <dd>
                {pending.preview.workouts}
                {pending.preview.already_imported > 0 && (
                  <span className="muted"> ({pending.preview.already_imported} already imported)</span>
                )}
              </dd>
              <dt>Sets</dt>
              <dd>{pending.preview.sets.toLocaleString()}</dd>
              <dt>Exercise names</dt>
              <dd>{pending.preview.exercise_names}</dd>
              <dt>Dates</dt>
              <dd>
                {pending.preview.first && formatDate(pending.preview.first)} – {pending.preview.last && formatDate(pending.preview.last)}
              </dd>
              <dt>Weight unit</dt>
              <dd>{pending.preview.weight_unit ?? "per row"}</dd>
            </dl>
          ) : pending.kind === "measurements" ? (
            <dl className="facts">
              <dt>Measurements</dt>
              <dd>{pending.preview.entries}</dd>
              <dt>Body parts</dt>
              <dd>{pending.preview.parts.map((p) => p.replace(/_/g, " ")).join(", ")}</dd>
              <dt>Dates</dt>
              <dd>
                {pending.preview.first && formatDate(pending.preview.first)} – {pending.preview.last && formatDate(pending.preview.last)}
              </dd>
            </dl>
          ) : (
            <dl className="facts">
              <dt>Weigh-ins</dt>
              <dd>{pending.preview.entries}</dd>
              <dt>Dates</dt>
              <dd>
                {pending.preview.first && formatDate(pending.preview.first)} – {pending.preview.last && formatDate(pending.preview.last)}
              </dd>
              <dt>Weight unit</dt>
              <dd>{pending.preview.weight_unit}</dd>
            </dl>
          )}
          {pending.preview.warnings.length > 0 && (
            <details className="warnings">
              <summary>{pending.preview.warnings.length} warnings</summary>
              <ul>
                {pending.preview.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            </details>
          )}
          <div className="row">
            <button className="button primary" disabled={busy} onClick={commit}>
              {busy ? "Importing…" : "Import"}
            </button>
            <button className="button" onClick={() => setPending(null)}>
              Cancel
            </button>
          </div>
        </section>
      )}
      <ExportBackup />
    </div>
  );
}

function ExportBackup() {
  const [last, setLast] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<{ path: string; backup: BackupInfo; local: BackupInfo } | null>(null);
  const [shareExports, setShareExports] = useState(false);
  const [auto, setAuto] = useState<{ on: boolean; last: string | null; days: string; error: string | null; dir: string | null } | null>(null);
  useEffect(() => {
    api.getSetting("last_backup_at").then(setLast);
    Promise.all(["auto_backup", "last_auto_backup_at", "auto_backup_days", "last_auto_backup_error", "auto_backup_dir"].map(api.getSetting)).then(
      ([on, last, days, error, dir]) => setAuto({ on: on !== "off", last, days: days === "1" ? "1" : "7", error, dir }),
    );
  }, []);
  const run = async (f: () => Promise<string | null>) => {
    setError(null);
    setMsg(null);
    try {
      setMsg(await f());
    } catch (e) {
      setError(String(e));
    }
  };
  const exportTo = (kind: "strong" | "bodyweight" | "inbody" | "diet" | "measurements" | "json", name: string, ext: string) =>
    run(async () => {
      if (IS_ANDROID) {
        const saved = await api.shareExport(kind, !shareExports);
        return saved && `Saved to ${saved}`;
      }
      const path = await save({ defaultPath: `${name}-${today()}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
      if (!path) return null;
      await api.exportFile(kind, path);
      return `Saved ${path}`;
    });

  return (
    <>
      <section className="card">
        <h2 className="section-title">Back up</h2>
        {IS_ANDROID && (
          <p className="small success">
            Android backs FitLedger up to your Google account automatically and restores it when you install the app on a new phone (check
            Settings › Google › Backup is on). Use the file below to move your data to a computer or keep your own copy.
          </p>
        )}
        <p className="muted small">
          One file with everything: workouts, templates, schedule, body data and settings. Restore it on any device (phone ↔ desktop).
          {last ? ` Last backup: ${formatDate(last)} ${last.slice(11, 16)}.` : " No backup yet."}
        </p>
        <div className="row">
          <button
            className="button primary"
            onClick={() =>
              run(async () => {
                if (IS_ANDROID) {
                  const [info, saved] = await api.shareBackup(true);
                  setLast(await api.getSetting("last_backup_at"));
                  return `Saved ${info.workouts} workouts to ${saved}. Restore… can open it from there.`;
                }
                const path = await save({
                  defaultPath: `fitledger-${today()}.fitledger`,
                  filters: [{ name: "FitLedger backup", extensions: ["fitledger"] }],
                });
                if (!path) return null;
                const info = await api.backupTo(path);
                setLast(await api.getSetting("last_backup_at"));
                return `Backed up ${info.workouts} workouts to ${path}`;
              })
            }
          >
            {IS_ANDROID ? "Save to Downloads" : "Back up now"}
          </button>
          {IS_ANDROID && (
            <button
              className="button"
              onClick={() =>
                run(async () => {
                  const [info] = await api.shareBackup(false);
                  setLast(await api.getSetting("last_backup_at"));
                  return `Backup of ${info.workouts} workouts ready: pick where to keep it (Drive, Files…).`;
                })
              }
            >
              Share…
            </button>
          )}
          {IS_ANDROID ? (
            <label className="button">
              Restore…
              <input
                type="file"
                hidden
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f)
                    run(async () => {
                      const staged = await api.stageBackup(await fileToBase64(f));
                      setPending({ path: staged.path, backup: staged.info, local: await api.dbInfo() });
                      return null;
                    });
                }}
              />
            </label>
          ) : (
            <button
              className="button"
              onClick={() =>
                run(async () => {
                  const path = await open({
                    multiple: false,
                    filters: [{ name: "FitLedger backup", extensions: ["fitledger", "liftbook", "db", "sqlite"] }],
                  });
                  if (!path || Array.isArray(path)) return null;
                  setPending({ path, backup: await api.inspectBackup(path), local: await api.dbInfo() });
                  return null;
                })
              }
            >
              Restore…
            </button>
          )}
        </div>
        {auto && (
          <>
            <label className="check small">
              <input
                type="checkbox"
                checked={auto.on}
                onChange={(e) => {
                  setAuto({ ...auto, on: e.target.checked });
                  api.setSetting("auto_backup", e.target.checked ? "on" : "off");
                }}
              />
              Back up automatically to {IS_ANDROID ? "Download/FitLedger" : auto.dir || "the app's auto-backups folder"}
            </label>
            {auto.on && (
              <>
                <div className="segmented">
                  {[
                    ["7", "Weekly (keeps 4)"],
                    ["1", "Daily (keeps 7)"],
                  ].map(([v, l]) => (
                    <button
                      key={v}
                      className={auto.days === v ? "active" : ""}
                      onClick={() => {
                        setAuto({ ...auto, days: v });
                        api.setSetting("auto_backup_days", v);
                      }}
                    >
                      {l}
                    </button>
                  ))}
                </div>
                {!IS_ANDROID && (
                  <div className="row">
                    <button
                      className="button small"
                      onClick={async () => {
                        const dir = await open({ directory: true });
                        if (typeof dir !== "string") return;
                        await api.setSetting("auto_backup_dir", dir);
                        setAuto({ ...auto, dir });
                      }}
                    >
                      Choose folder…
                    </button>
                    {auto.dir && (
                      <button
                        className="button small"
                        onClick={async () => {
                          await api.setSetting("auto_backup_dir", "");
                          setAuto({ ...auto, dir: null });
                        }}
                      >
                        Use default folder
                      </button>
                    )}
                  </div>
                )}
                {auto.error && <p className="warn small">⚠ The last automatic backup failed: {auto.error}</p>}
                <p className="muted small">
                  {auto.last ? `Last automatic backup: ${formatDate(auto.last)}.` : "The first one runs the next time the app starts."} It runs
                  when you open the app or finish a workout.
                </p>
              </>
            )}
          </>
        )}
        {pending && (
          <div className="restore-confirm">
            <table className="sets small">
              <tbody>
                <tr>
                  <td />
                  <th>This device</th>
                  <th>Backup</th>
                </tr>
                <tr>
                  <td>Workouts</td>
                  <td>{pending.local.workouts}</td>
                  <td>{pending.backup.workouts}</td>
                </tr>
                <tr>
                  <td>Latest workout</td>
                  <td>{pending.local.last_workout ? formatDate(pending.local.last_workout) : "—"}</td>
                  <td>{pending.backup.last_workout ? formatDate(pending.backup.last_workout) : "—"}</td>
                </tr>
                <tr>
                  <td>Weigh-ins</td>
                  <td>{pending.local.weigh_ins}</td>
                  <td>{pending.backup.weigh_ins}</td>
                </tr>
              </tbody>
            </table>
            {(pending.local.last_workout ?? "") > (pending.backup.last_workout ?? "") && (
              <p className="warn small">⚠ This device has newer workouts than the backup. They'll be replaced.</p>
            )}
            <p className="muted small">A safety copy of this device's data is saved first.</p>
            <div className="row">
              <button
                className="button danger"
                onClick={() =>
                  run(async () => {
                    const copy = await api.restoreFrom(pending.path);
                    setPending(null);
                    setTimeout(() => window.location.reload(), 4000);
                    return `Restored. Safety copy: ${copy}. Reloading…`;
                  })
                }
              >
                Replace everything on this device
              </button>
              <button className="button" onClick={() => setPending(null)}>
                Cancel
              </button>
            </div>
          </div>
        )}
      </section>
      <section className="card">
        <h2 className="section-title">Export</h2>
        <div className="export-list">
          <button className="start-row" onClick={() => exportTo("strong", "fitledger-workouts", "csv")}>
            <span>Workouts (Strong CSV format)</span>
            <span className="muted">›</span>
          </button>
          <button className="start-row" onClick={() => exportTo("bodyweight", "fitledger-bodyweight", "csv")}>
            <span>Bodyweight CSV</span>
            <span className="muted">›</span>
          </button>
          <button className="start-row" onClick={() => exportTo("inbody", "fitledger-body-composition", "csv")}>
            <span>Body composition (InBody) CSV</span>
            <span className="muted">›</span>
          </button>
          <button className="start-row" onClick={() => exportTo("diet", "fitledger-diet", "csv")}>
            <span>Diet CSV</span>
            <span className="muted">›</span>
          </button>
          <button className="start-row" onClick={() => exportTo("measurements", "fitledger-measurements", "csv")}>
            <span>Body measurements CSV</span>
            <span className="muted">›</span>
          </button>
          <button className="start-row" onClick={() => exportTo("json", "fitledger-export", "json")}>
            <span>Everything (JSON)</span>
            <span className="muted">›</span>
          </button>
        </div>
        {IS_ANDROID && (
          <label className="check small muted">
            <input type="checkbox" checked={shareExports} onChange={(e) => setShareExports(e.target.checked)} />
            Open the share sheet instead of saving to Downloads/FitLedger
          </label>
        )}
      </section>
      {msg && <p className="success small">{msg}</p>}
      {error && <p className="error">{error}</p>}
    </>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="stat">
      <div className="stat-value">{value.toLocaleString()}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}
