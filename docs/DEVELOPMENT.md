# Developing FitLedger

Tauri 2 app: Rust + SQLite backend (`crates/core`, no Tauri dependency), InBody OCR
(`crates/ocr`), a thin Tauri shell with Android plugins (`src-tauri`), and a React + TypeScript +
D3 UI (`src`). See [PLAN.md](PLAN.md) for architecture decisions and milestones.

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
  For a quick emulator build: `pnpm tauri android build --debug --apk --target x86_64`.
- **Your phone:** turn on Developer options (tap Build number 7 times) and USB debugging, plug it
  in, accept the prompt, check `~/Android/Sdk/platform-tools/adb devices`, then
  `pnpm tauri android dev` (or install the release APK below).
- **Installable APK:** `pnpm tauri android build --apk --target aarch64` gives a signed arm64
  `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`
  (about 17 MB; building every ABI makes it about 60 MB). Install it with `adb install -r <apk>`,
  or copy it to the phone and open it. Debug and release builds have different signatures, so
  switching between them means uninstalling first (back up your data from the app first).

Release signing uses `~/Android/fitledger-release.jks`; its passwords are in
`~/Android/fitledger-keystore.properties` (copied to `src-tauri/gen/android/keystore.properties`,
which git ignores). Keep both safe: app updates must be signed with the same key.

## Releasing

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`.
   Also bump `bundle.android.versionCode` in `tauri.conf.json`: Android only installs a higher
   code, and a pre-release version such as `1.0.0-beta` doesn't raise the code on its own.
2. Build the signed arm64 APK (above), commit on `main`, tag `v<version>`, and push both.
3. Create a GitHub pre-release for the tag with the APK attached as `fitledger-<version>.apk`.
   The APK is over GitHub's 100 MB file limit for git with all ABIs and doesn't belong in the repo
   anyway: always attach it to the release.

## How some things work

**Android native plugins.** `src-tauri/gen/android/.../fitledger/` holds `RestTimerPlugin.kt`
(notifications, alarms, share sheet, saving to Photos/Downloads) and `HealthConnectPlugin.kt`.
Each is registered under its own Tauri plugin (`resttimer`, `healthconnect` in
`src-tauri/src/lib.rs`): Android keys native plugins by the Tauri plugin's name, so two under one
name silently replace each other.

**Rest timer.** `RestTimerPlugin.kt` shows an ongoing notification (rest countdown / workout
clock). During each rest `RestChimeService.kt` runs as a media-playback foreground service: it
buzzes at 3, 2 and 1 s left and plays a chime on the media stream (media volume, ducking music) at
0, with the screen off or in another app. It is started while the app is in front, because
Android mutes audio from a service started in the background; an exact alarm 3 s before the end
restarts it if it was killed.

**Backups.** The app keeps a consistent database snapshot in `backup/` (refreshed within a minute
of any change) and Android's own Auto Backup copies it to the user's Google account, restoring it
when the app is installed on a new phone. No sign-in or server is involved. Restore requires the
same signing key, which is one more reason to keep the keystore safe. To test on an emulator:
`adb shell bmgr transport com.android.localtransport/.LocalTransport`,
`adb shell bmgr backupnow io.github.njk639.fitledger`, then uninstall and reinstall.

Exports and backups on Android save to `Download/FitLedger` (or open the share sheet). A weekly
automatic backup (`fitledger-auto-<date>.fitledger`, newest 4 kept) goes to the same folder, or to
`auto-backups/` in the app data folder on desktop; it can be turned off on the Data page.

**InBody result sheets.** Measure → Body comp → **Read sheet images** reads InBody 580 result
sheets saved as PNG/JPEG on the device (`crates/ocr`, using the
[Ocrs](https://github.com/robertknight/ocrs) engine). The models aren't bundled: the first read
downloads them (12 MB) into `ocr-models/` in the app data folder and checks their SHA-256
(`fitledger_ocr::ensure_models`). Each value is read from its known place on the sheet and the
totals are cross-checked; anything doubtful opens in the scan form for a quick check.

**Cardio.** Runs, rides and rows are distance + time exercises (`crates/core/src/cardio.rs`,
`src/cardio.ts`). Cardio PRs: longest distance, longest time and fastest pace (sessions of at
least 400 m). Weekly minutes are compared against a goal (default 150).

**Body fat estimates** (`src/bodyfat.ts`): US Navy method (waist and neck, plus hips for women,
measured within two weeks) and relative fat mass (height and waist). At each estimate, weight at
the nearest weigh-in gives lean mass (weight × (1 − body fat)), fat mass and skeletal muscle (lean
× the muscle share of lean mass on your InBody scans, else 0.56).

**Percentiles and strength tiers** (`crates/core/src/norms.rs`, `src/components/Standing.tsx`).
The tables in `data/norms/` (about 45 KB) are built by `scripts/build_norms.py` from the original
sources, which aren't kept in git (the script's header says where to get them):

- `body_composition.json`: Kelly et al. 2009 NHANES DXA LMS curves (L, M, S at ages 20–85 every 5
  years, by sex and group) for % fat, fat mass/height², lean mass/height² and arm + leg
  lean/height². Percentile = Φ(((x/M)^L − 1)/(L·S)). Without a group, the three groups' percentiles
  are averaged. DXA "lean" excludes bone, so the lean index uses InBody fat-free mass less 4.5 %.
- `body_size.json`: CDC 2015–2018 percentiles (5th–95th) by 10-year age band for weight, BMI, waist
  and mid-upper arm; linear between points.
- `strength.json`: OpenPowerlifting, raw and drug-tested, no DQs, ages 18+; each lifter's best per
  lift and age band (18–39, 40s, 50s, 60s, 70+), in 10 kg bodyweight bins with at least 100
  lifters, blended between the two nearest bins. Tiers: Novice from half the median, Intermediate
  from the 10th percentile, Advanced from the median, Elite from the 90th.

Age comes from the birth year and the profile (sex, height, birth year, group) is set on Measure →
Body comp.

## Demo data and screenshots

```sh
cargo run -p fitledger-core --example demo_data -- demo.fitledger [end date]
```

builds a restorable backup with two years of made-up workouts, weigh-ins, cardio, InBody scans,
measurements, calories, goals, phases, templates and a schedule (deterministic, nobody's real
data). Restore it in the app, or on the emulator copy it over the database while the app is
stopped: `adb push demo.fitledger /data/local/tmp/demo.db`, then
`adb shell "run-as io.github.njk639.fitledger sh -c 'cp /data/local/tmp/demo.db fitledger.db'"`.
The README screenshots in `docs/screenshots/` come from it, taken on the emulator.

The emulator's software renderers (`swiftshader_indirect`, `guest`) crashed here on chart-heavy
pages; `-gpu host -no-audio` was stable.

## Loading data without the UI

```sh
cargo run -p fitledger-core --example import_strong -- <path/to/fitledger.db> strong_workouts.csv strong_weight.csv
```

The app's database is `fitledger.db` in the platform app-data folder (Linux:
`~/.local/share/io.github.njk639.fitledger/`).

## Tests

```sh
cargo test -p fitledger-core            # core tests (some use your real Strong export, see below)
cargo test --release -p fitledger-ocr   # OCR tests (real sheets in ~/my_data/inbody, if present)
pnpm test                               # frontend unit tests
pnpm build                              # type-check and build the UI
pnpm format                             # format the frontend
python3 scripts/build_library.py        # rebuild data/library/exercises.json after editing curated.psv
```

Some tests (`crates/core/tests/real_data.rs`, `crates/ocr/tests/real_sheets.rs`, kept out of git
via `.git/info/exclude`) run against real exports. Put `strong_workouts_*.csv` and
`strong_weight_*.csv` in `~/my_data` (or set `FITLEDGER_TEST_DATA`); those tests are skipped
otherwise. Never commit personal exports.
