import { useEffect, useState } from "react";
import { api, type HealthConnectStatus, type WearableSummary } from "../api";
import { formatDate, formatTime, parseLocal } from "../format";
import { IS_ANDROID, fileToBase64 } from "../platform";

/** Plain-language result of an import. */
export function summaryText(s: WearableSummary): string {
  const parts = [
    s.activities && `${s.activities} cardio session${s.activities === 1 ? "" : "s"}`,
    s.workouts_hr && `heart rate on ${s.workouts_hr} workout${s.workouts_hr === 1 ? "" : "s"}`,
    s.weights && `${s.weights} weigh-in${s.weights === 1 ? "" : "s"}`,
    s.days && `${s.days} day${s.days === 1 ? "" : "s"} of sleep/recovery`,
  ].filter(Boolean);
  return parts.length ? `Added ${parts.join(", ")}.` : "Nothing new.";
}

/** Sync from Health Connect at most hourly, after the app opens (Android, once connected). */
export async function autoSyncHealthConnect() {
  if (!IS_ANDROID) return;
  const s = await api.healthConnectStatus();
  if (s.available !== "yes" || s.granted === 0) return;
  if (s.synced_at && Date.now() - parseLocal(s.synced_at).getTime() < 3600_000) return;
  await api.healthConnectSync();
}

function HealthConnectSection() {
  const [status, setStatus] = useState<HealthConnectStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const refresh = () => api.healthConnectStatus().then(setStatus, (e) => setMsg(String(e)));
  useEffect(() => {
    refresh();
  }, []);
  if (!status) return msg ? <p className="small warn">{msg}</p> : null;

  async function run(f: () => Promise<unknown>) {
    setBusy(true);
    setMsg(null);
    try {
      await f();
    } catch (e) {
      setMsg(String(e));
    }
    setBusy(false);
    refresh();
  }
  const sync = () => run(async () => setMsg(summaryText(await api.healthConnectSync())));

  if (status.available === "no")
    return <p className="small muted">Health Connect isn't available on this phone (it's built into Android 14 and later; install it from Google Play on older phones).</p>;
  if (status.available === "update")
    return <p className="small warn">Update Health Connect from Google Play, then come back here.</p>;
  return (
    <>
      <div className="section-label">Health Connect</div>
      {status.granted === 0 ? (
        <p className="small muted">Not connected. FitLedger only reads; nothing leaves this phone.</p>
      ) : (
        <p className="small">
          Connected ({status.granted} of {status.total} kinds of data)
          {status.synced_at && (
            <span className="muted">
              {" "}
              · synced {formatDate(status.synced_at)} {formatTime(status.synced_at)}
            </span>
          )}
        </p>
      )}
      <div className="row action-row">
        {status.granted === 0 ? (
          <button className="button primary" disabled={busy} onClick={() => run(async () => (await api.healthConnectConnect()).granted > 0 && sync())}>
            Connect
          </button>
        ) : (
          <>
            <button className="button" disabled={busy} onClick={sync}>
              {busy ? "Syncing…" : "Sync now"}
            </button>
            <button className="button" disabled={busy} onClick={() => run(api.healthConnectConnect)}>
              Choose data
            </button>
          </>
        )}
        <button className="button" onClick={() => run(api.healthConnectOpen)}>
          Health Connect settings
        </button>
      </div>
      {msg && <p className="small">{msg}</p>}
      <p className="small muted">
        Your watch's app has to share with Health Connect too. In Garmin Connect, look under Settings › Connected Apps › Health Connect;
        Fitbit and Oura have the same switch in their settings. FitLedger syncs when you open it, at most once an hour.
      </p>
    </>
  );
}

function FitImport() {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  async function pick(files: FileList | null) {
    if (!files?.length) return;
    setBusy(true);
    setMsg(null);
    const total: WearableSummary = { weights: 0, activities: 0, workouts_hr: 0, days: 0 };
    const errors: string[] = [];
    for (const f of Array.from(files)) {
      try {
        const s = await api.importFit(await fileToBase64(f));
        total.weights += s.weights;
        total.activities += s.activities;
        total.workouts_hr += s.workouts_hr;
      } catch (e) {
        errors.push(`${f.name}: ${e}`);
      }
    }
    setMsg([summaryText(total), ...errors].join(" "));
    setBusy(false);
  }
  return (
    <>
      <div className="section-label">Garmin .fit files</div>
      <label className="button">
        {busy ? "Importing…" : "Import .fit or .zip files"}
        <input type="file" accept=".fit,.zip" multiple hidden disabled={busy} onChange={(e) => pick(e.target.files).then(() => (e.target.value = ""))} />
      </label>
      {msg && <p className="small">{msg}</p>}
      <p className="small muted">
        From Garmin Connect on the web: an activity's ⚙ menu › Export Original, or your whole account data export (the .zip). Good for
        history from before Health Connect, and on desktop.
      </p>
    </>
  );
}

/** Settings: watches and health apps. */
export function WearablesCard() {
  return (
    <section className="card">
      <h2 className="section-title">Watch & health apps</h2>
      <p className="small muted">
        Runs, rides and other cardio become cardio sessions. Heart rate from your watch is added to workouts you log here, weigh-ins go to
        your weight, and sleep, resting heart rate, HRV and steps show on the Today card. Something you already logged here is never
        doubled.
      </p>
      {IS_ANDROID && <HealthConnectSection />}
      <FitImport />
    </section>
  );
}
