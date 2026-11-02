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
ongoing notification (rest countdown / workout clock) and sets an exact alarm for the end of each
rest, so the vibration and alert arrive with the screen off, in another app, or after the app was
swiped away.

### Loading data without the UI

```sh
cargo run -p liftbook-core --example import_strong -- <path/to/liftbook.db> strong_workouts.csv strong_weight.csv
```

The app's database is `liftbook.db` in the platform app-data folder (Linux:
`~/.local/share/io.github.njk639.liftbook/`).

## Development

```sh
cargo test -p liftbook-core        # core tests (some use your real Strong export, see below)
pnpm test                          # frontend unit tests (LOESS)
pnpm build                         # type-check and build the UI
pnpm format                        # format the frontend
python3 scripts/build_library.py   # rebuild data/library/exercises.json after editing curated.psv
```

Some tests (`crates/core/tests/real_data.rs`, kept out of this public repo via `.git/info/exclude`) run against a real Strong export. Put `strong_workouts_*.csv` and
`strong_weight_*.csv` in `~/my_data` (or set `LIFTBOOK_TEST_DATA`); those tests are skipped
otherwise. Never commit personal exports.

See [docs/PLAN.md](docs/PLAN.md) for architecture and milestones.
