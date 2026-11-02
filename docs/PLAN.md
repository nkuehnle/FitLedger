# Liftbook plan

Personal, offline lift tracker built around supersets. Tauri 2 (Android, Windows, Linux),
Rust + SQLite backend, React + TypeScript + D3 frontend.

## Layout

- `crates/core`: domain logic with no Tauri dependency (importers, matching, schedule, stats,
  storage). Tested with `cargo test -p liftbook-core`.
- `src-tauri`: Tauri shell, thin commands over `liftbook-core`, platform plugins.
- `src`: React UI.

Storage: weights in kg, distances in metres, local wall-clock times plus UTC offset. Display units
are a setting.

## Decisions

- **Imports.** Strong CSV (current and older formats, units from headers or chosen by the user).
  Re-import is idempotent by source key (start time + workout name); existing rows are never
  overwritten. Workout names are kept as-is. Creating templates from repeated names is an optional
  helper, not an assumption.
- **Exercise mapping.** Strong names map to library exercises, optionally per date range, since the
  same name can mean different exercises over time. Low-confidence matches go to a review screen
  that shows each name's sessions, date span and notes.
- **Exercise notes.** Kept per session exactly as written. Targets ("3x8-12") are parsed only
  when building templates; unclear ones are flagged. Setup notes are a separate editable field.
- **Load mode.** Each exercise is `total` or `per_hand`, with a per-session override. Sessions
  whose weight is about half of neighbouring sessions are flagged for review. Equipment labels
  (barbell, dumbbell, cable, machine, smith, landmine, kettlebell, band, bodyweight, cardio) are shown
  and filterable.
- **Bodyweight exercises.** Added weight is logged (0 or more). Bodyweight at the time is
  interpolated from weigh-ins, for overlays and an optional "bodyweight + added" load. Volume
  excludes bodyweight by default.
- **Live workout (Strong-like).** The whole workout loads from the last session of that template,
  with values pre-filled. Sets can be ticked in any order; ticking starts that exercise's remembered
  rest timer. Supersets are drawn with a colored bar and drag to reorder as a unit. On finish, ask
  whether to update the template. Any past workout can be repeated. Per exercise there is a
  next-time intent (up / hold / down) separate from notes, plus workout and exercise notes.
- **Suggesting the next workout.** The New workout list puts one suggestion first: today's planned
  workout, then workouts missed earlier this week. On a rest day, a missed workout comes first.
  Misses never carry into the next week. Custom-length cycles use the cycle as the window.
- **Programs.** Built-in generic starter programs (editable) plus user-made templates and routines.
- **Weeks** start on Monday.
- **Charts.** Shared time-series style: right-side y-axis with units, sparse year/month x labels,
  faint grid (solid at years, dashed at months), a single 2px line with sparse hollow markers, no
  chrome for single series. Raw / LOESS toggle (LOESS line with faint raw points). Own palette,
  designed separately for light and dark.
- **Diet (optional).** Daily calorie and protein totals, each optional. Adaptive TDEE from sliding-window intake averages and
  the change in trend weight. Estimated surplus/deficit from the weight trend alone. Nothing in
  the app requires weight or calorie data.
- **Cloud backup** is deferred until after the app has been used. Local backup/restore and exports
  are in scope.
- **Personal data never enters git.** Real-data tests read `$LIFTBOOK_TEST_DATA` (default
  `~/my_data`) and skip when it's absent.

## Milestones

| # | Milestone | Status |
|---|---|---|
| 1a | Core importer: Strong CSV (both formats), bodyweight, note targets, idempotent SQLite import | Done |
| 1b | Exercise library (845, 250 curated), matcher, date-ranged name mapping, review screen, history feed | Done |
| 2 | Stats: e1RM (Epley/Brzycki), PRs, rep maxes, volume, fractional sets per muscle, streaks | Done |
| 3 | Live workout: supersets, pre-fill, rest timer, next-time intent, joint ratings, swap/reorder, summary | Done (desktop) |
| 4 | Templates, starter programs, weekly/cycle schedule, suggestion rule, phases, targets, Today | Done |
| 5 | Charts (raw/LOESS, phases, PR markers), calendar, body map heat map + compare/animate, bodyweight | Done |
| 6 | InBody: manual form, CSV import with remembered column mapping, charts, segmental view | Done |
| 7 | Diet: calories/protein log, energy balance from weight trend, adaptive TDEE | Done |
| 8 | Sharing: PNG cards 1080×1080 / 1080×1920, dark/light, hide numbers, save + copy | Done (desktop) |
| 9 | Export (Strong CSV, bodyweight, InBody, diet, JSON) and local backup/restore | Done |
| 10 | Android: build, rest alarm + ongoing notification, share sheet, restore, layout | Done (emulator) |
| 11 | UI restructure: 4 tabs, Perform again, exercise ⋯ menu, sticky notes, goals, body parts | Done |
| — | Cloud backup (Google Drive, others later) | Deferred |

## Still to do

- Test on the user's phone (real vibration, Doze, battery optimisation settings).
- Optional muscle additions discussed: hip flexors, gastrocnemius/soleus, tibialis, triceps
  long head.
- Android account backup (Android Auto Backup of a DB snapshot): done, tested with uninstall/reinstall.
- Optional Google Drive backups (visible folder, after each workout, phone ↔ desktop): needs a
  one-time OAuth client setup by the publisher; not started.
- Play Store release ($25 one-time developer fee; privacy policy page needed).

## Notes

- PR rules are versioned (`stats::PR_RULES_VERSION`); bumping it rebuilds PRs on next launch.
- A set only creates a rep-range PR in its own range (largest of 1/3/5/8/10/12+ not above its
  reps); the records table still shows "heaviest for at least N reps".
- Clippy and rustfmt aren't installed on the dev machine; `pnpm format` formats the frontend.
