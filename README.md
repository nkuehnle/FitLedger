# FitLedger
The last fitness app I'll ever use.

## Running

### Desktop (Linux/Windows)

```sh
pnpm install
pnpm tauri dev          # development build with hot reload
pnpm tauri build        # installable bundles in target/release/bundle/
```

On Linux this needs the WebKitGTK 4.1 development packages (see the
[Tauri prerequisites](https://tauri.app/start/prerequisites/)).

### Android

This machine has everything installed under `~/Android` (JDK 21, SDK, NDK 28.2, an emulator
called `fitledger`). Set the environment first:

```sh
export JAVA_HOME=~/Android/jdk ANDROID_HOME=~/Android/Sdk NDK_HOME=~/Android/Sdk/ndk/28.2.13676358
```

From a VS Code terminal (VS Code is a snap), the snap's GTK and XDG variables break the build and
the desktop app; use a normal terminal.

- **Emulator:** `~/Android/Sdk/emulator/emulator -avd fitledger &`, then `pnpm tauri android dev`.
- **Your phone:** turn on Developer options (tap Build number 7 times) and USB debugging, plug it
  in, accept the prompt, check `~/Android/Sdk/platform-tools/adb devices`, then
  `pnpm tauri android dev` (or install the release APK below).
- **Installable APK:** `pnpm tauri android build --apk --target aarch64` gives a signed
  `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`.
  Install it with `adb install -r <apk>`, or copy it to the phone and open it. Debug and release
  builds have different signatures, so switching between them means uninstalling first (back up
  your data from the app first).

Release signing uses `~/Android/fitledger-release.jks`; its passwords are in
`~/Android/fitledger-keystore.properties` (copied to `src-tauri/gen/android/keystore.properties`,
which git ignores). Keep both safe: app updates must be signed with the same key.

Backups on Android: the app keeps a consistent database snapshot in `backup/` (refreshed within a
minute of any change) and Android's own Auto Backup copies it to the user's Google account,
restoring it when the app is installed on a new phone. No sign-in or server is involved. Restore
requires the same signing key, which is one more reason to keep the keystore safe. To test on an
emulator: `adb shell bmgr transport com.android.localtransport/.LocalTransport`,
`adb shell bmgr backupnow io.github.njk639.fitledger`, then uninstall and reinstall.

Rest timer on Android: `RestTimerPlugin.kt` in `src-tauri/gen/android/.../fitledger/` shows an
ongoing notification (rest countdown / workout clock). During each rest `RestChimeService.kt`
runs as a media-playback foreground service: it buzzes at 3, 2 and 1 s left and plays a chime on
the media stream (media volume, ducking music) at 0, with the screen off or in another app. It is
started while the app is in front, because Android mutes audio from a service started in the
background; an exact alarm 3 s before the end restarts it if it was killed.

Exports and backups on Android save to `Download/FitLedger` (or open the share sheet). A weekly
automatic backup (`fitledger-auto-<date>.fitledger`, newest 4 kept) goes to the same folder, or to
`auto-backups/` in the app data folder on desktop; it can be turned off on the Data page.

### Loading data without the UI

```sh
cargo run -p fitledger-core --example import_strong -- <path/to/fitledger.db> strong_workouts.csv strong_weight.csv
```

The app's database is `fitledger.db` in the platform app-data folder (Linux:
`~/.local/share/io.github.njk639.fitledger/`).

### InBody result sheets

Measure → Body comp → **Read sheet images** reads InBody 580 result sheets saved as PNG/JPEG,
entirely on the device (`crates/ocr`, using the [Ocrs](https://github.com/robertknight/ocrs)
engine with its models embedded). Each value is read from its known place on the sheet and the
totals are cross-checked; anything doubtful opens in the scan form for a quick check. Ocrs models
are trained on Google Research's HierText dataset and licensed CC BY-SA 4.0 (see
`crates/ocr/models/README.md`).

### Cardio

Runs, rides and rows are distance + time exercises (`crates/core/src/cardio.rs`, `src/cardio.ts`).
In a workout each segment shows its pace (or speed for cycling, /500 m for rowing) as you type,
with optional average heart rate and calories, and a stopwatch fills in the time. Progress →
Cardio shows weekly minutes against a goal (default 150), each activity's pace trend against your
usual, and logs sessions done outside a workout. Cardio PRs: longest distance, longest time and
fastest pace (sessions of at least 400 m).

### Body fat estimates

Measure → Body comp plots body fat from InBody scans, body fat logged in Strong, and two tape
estimates: the US Navy method (waist and neck, plus hips for women, measured within two weeks)
and relative fat mass (height and waist). At each estimate, weight at the nearest weigh-in gives
lean mass (weight × (1 − body fat)), fat mass and skeletal muscle (lean × the muscle share of
lean mass on your InBody scans, else 0.56), charted next to the InBody readings (`src/bodyfat.ts`).

## Development

```sh
cargo test -p fitledger-core        # core tests (some use your real Strong export, see below)
cargo test --release -p fitledger-ocr   # OCR tests (real sheets in ~/my_data/inbody, if present)
pnpm test                          # frontend unit tests (LOESS)
pnpm build                         # type-check and build the UI
pnpm format                        # format the frontend
python3 scripts/build_library.py   # rebuild data/library/exercises.json after editing curated.psv
```

Some tests (`crates/core/tests/real_data.rs`, kept out of this public repo via `.git/info/exclude`) run against a real Strong export. Put `strong_workouts_*.csv` and
`strong_weight_*.csv` in `~/my_data` (or set `FITLEDGER_TEST_DATA`); those tests are skipped
otherwise. Never commit personal exports.

See [docs/PLAN.md](docs/PLAN.md) for architecture and milestones.
