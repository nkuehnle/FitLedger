# LiftBook
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
called `liftbook`). Set the environment first:

```sh
export JAVA_HOME=~/Android/jdk ANDROID_HOME=~/Android/Sdk NDK_HOME=~/Android/Sdk/ndk/28.2.13676358
```

From a VS Code terminal (VS Code is a snap), the snap's GTK and XDG variables break the build and
the desktop app; use a normal terminal.

- **Emulator:** `~/Android/Sdk/emulator/emulator -avd liftbook &`, then `pnpm tauri android dev`.
- **Your phone:** turn on Developer options (tap Build number 7 times) and USB debugging, plug it
  in, accept the prompt, check `~/Android/Sdk/platform-tools/adb devices`, then
  `pnpm tauri android dev` (or install the release APK below).
- **Installable APK:** `pnpm tauri android build --apk --target aarch64` gives a signed
  `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`.
  Install it with `adb install -r <apk>`, or copy it to the phone and open it. Debug and release
  builds have different signatures, so switching between them means uninstalling first (back up
  your data from the app first).

Release signing uses `~/Android/liftbook-release.jks`; its passwords are in
`~/Android/liftbook-keystore.properties` (copied to `src-tauri/gen/android/keystore.properties`,
which git ignores). Keep both safe: app updates must be signed with the same key.

Backups on Android: the app keeps a consistent database snapshot in `backup/` (refreshed within a
minute of any change) and Android's own Auto Backup copies it to the user's Google account,
restoring it when the app is installed on a new phone. No sign-in or server is involved. Restore
requires the same signing key, which is one more reason to keep the keystore safe. To test on an
emulator: `adb shell bmgr transport com.android.localtransport/.LocalTransport`,
`adb shell bmgr backupnow io.github.njk639.liftbook`, then uninstall and reinstall.

Rest timer on Android: `RestTimerPlugin.kt` in `src-tauri/gen/android/.../liftbook/` shows an
ongoing notification (rest countdown / workout clock). During each rest `RestChimeService.kt`
runs as a media-playback foreground service: it buzzes at 3, 2 and 1 s left and plays a chime on
the media stream (media volume, ducking music) at 0, with the screen off or in another app. It is
started while the app is in front, because Android mutes audio from a service started in the
background; an exact alarm 3 s before the end restarts it if it was killed.

Exports and backups on Android save to `Download/Liftbook` (or open the share sheet).

### Loading data without the UI

```sh
cargo run -p liftbook-core --example import_strong -- <path/to/liftbook.db> strong_workouts.csv strong_weight.csv
```

The app's database is `liftbook.db` in the platform app-data folder (Linux:
`~/.local/share/io.github.njk639.liftbook/`).

### InBody result sheets

Measure → Body comp → **Read sheet images** reads InBody 580 result sheets saved as PNG/JPEG,
entirely on the device (`crates/ocr`, using the [Ocrs](https://github.com/robertknight/ocrs)
engine with its models embedded). Each value is read from its known place on the sheet and the
totals are cross-checked; anything doubtful opens in the scan form for a quick check. Ocrs models
are trained on Google Research's HierText dataset and licensed CC BY-SA 4.0 (see
`crates/ocr/models/README.md`).

## Development

```sh
cargo test -p liftbook-core        # core tests (some use your real Strong export, see below)
cargo test --release -p liftbook-ocr   # OCR tests (real sheets in ~/my_data/inbody, if present)
pnpm test                          # frontend unit tests (LOESS)
pnpm build                         # type-check and build the UI
pnpm format                        # format the frontend
python3 scripts/build_library.py   # rebuild data/library/exercises.json after editing curated.psv
```

Some tests (`crates/core/tests/real_data.rs`, kept out of this public repo via `.git/info/exclude`) run against a real Strong export. Put `strong_workouts_*.csv` and
`strong_weight_*.csv` in `~/my_data` (or set `LIFTBOOK_TEST_DATA`); those tests are skipped
otherwise. Never commit personal exports.

See [docs/PLAN.md](docs/PLAN.md) for architecture and milestones.
