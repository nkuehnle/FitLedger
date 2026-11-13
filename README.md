# FitLedger

**The fitness app you control.** Track lifting, cardio, bodyweight and body composition on your
phone (Android) and desktop (Linux, Windows).

- **Your data stays yours:** no account, no server, no subscription. Everything lives on your
  device, and you can export all of it at any time (Strong-format CSV, JSON, full backups).
- **Open code:** every line is in this repository, free to use, change and share for any
  non-commercial purpose ([license](#license)). Nothing is hidden, and nothing phones home.

<p>
  <img src="docs/screenshots/workouts.png" width="270" alt="Workouts: today's suggestion and recent workouts">
  <img src="docs/screenshots/live-workout.png" width="270" alt="Live workout with pre-filled sets">
  <img src="docs/screenshots/exercise-chart.png" width="270" alt="Exercise chart: estimated 1RM over two years with program phases">
</p>
<p>
  <img src="docs/screenshots/body-map.png" width="270" alt="Body map of muscles trained">
  <img src="docs/screenshots/bodyweight.png" width="270" alt="Bodyweight chart with goals, phases and InBody scans">
  <img src="docs/screenshots/body-composition.png" width="270" alt="Body fat from InBody and tape estimates">
</p>

<sub>Screenshots use made-up demo data (`cargo run -p fitledger-core --example demo_data`).</sub>

## Install

**Android:** download `fitledger-<version>.apk` from the latest
[release](https://github.com/nkuehnle/FitLedger/releases), open it on your phone, and allow
installing from your browser or Files app when Android asks. New versions install over the old one,
and your data stays. (The APK is for 64-bit ARM phones, which is nearly every phone made since about
2017.)

**Coming from Strong:** export your data in Strong (Settings → Export data), then import the CSV in
FitLedger under **Settings → Import, export & backup**. Body measurements from Strong import too.

**Coming from Liftbook:** FitLedger installs as a new app. Restore your latest `.liftbook` backup
from **Settings → Import, export & backup → Restore…**.

**Desktop:** there are no desktop downloads yet; build it from source (see
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)).

## Features

### Workouts

- **Strong-style live workout:** every set comes pre-filled from your last session of that
  workout. Tick sets in any order, swap or reorder exercises, and add workout or exercise notes.
- **Supersets** are shown with a colored bar and can be dragged as a unit.
- **Rest timer** with a chime and countdown buzz that work with the screen off or in another app,
  plus an ongoing notification with the rest countdown and the workout clock.
- **Suggested weights** fill the empty boxes. Progression hints (go up, repeat, or deload after
  repeated misses) and a next-time "up / hold / down" note for each exercise.
- **Templates, routines and starter programs**, with a weekly or custom-length schedule. The
  New workout list puts today's planned workout, or one you missed this week, first.
- **Shareable routines:** send a schedule or a single template as a `.fitroutine.json` file, custom
  exercises included. The file holds your plan only, never your logged workouts or weights.
- **Routine files you can read:** every routine file explains its own format, and `resources.json`
  lists every exercise with its equipment and muscles. To add a routine from text, copy it and tap
  **Paste routine**.
- **Perform again:** repeat any past workout. When you finish, the app asks whether to update the
  template.
- **Cardio:** runs, rides and rows with live pace (or speed, or /500 m), average heart rate,
  calories, a stopwatch, and cardio PRs.

### Progress

- **Per-exercise charts:** estimated 1RM, heaviest weight, volume and reps, with PR markers,
  rep maxes and PR history.
- **Strength tiers** (Beginner to Elite, plus a percentile) against competitors of your sex, age
  and bodyweight: squat, bench and deadlift (raw, drug-tested powerlifters), snatch and clean & jerk
  (weightlifters), plus CrossFit athletes' self-reported lifts (squat, deadlift, Olympic lifts,
  pull-ups) as an everyday-lifter comparison. Close variations are compared through published
  ratios and marked as estimates: dumbbell, Smith machine, incline and close-grip bench, front and
  Smith squat, trap bar deadlift. About 115 more exercises (overhead presses, rows, pulldowns,
  flies, curls, leg press, machines, …) get a rough estimate, converted to the bench, squat or
  deadlift with ratios fitted to Strength Level's community standards. Shown on each exercise's
  Records tab and in workout summaries.
- **Body map heat map** of the muscles you trained, colored against weekly set targets, plus a
  muscle-by-week grid and a recovery view.
- **Training calendar**, streaks, workout duration and weekly minutes of cardio.
- **Program phases** (e.g. "Upper / Lower", "Cut") drawn as bands behind every chart.
- **Lifts:** each regular lift marked moving, flat or sliding against your last 6 weeks, judged
  by your goal, so holding strength on a cut counts as a win. Tap one for your last session, your
  rating and any suggestion. Suggestions never override your own rating.
- **Week finish line:** the Workouts tab tells you when a muscle is on course to miss its weekly
  set target.
- **Lift goals:** set a target estimated 1RM with an optional date, see it on the chart, and see
  when your trend reaches it.
- **What worked:** compare any two phases side by side, including sessions, sets, strength and
  bodyweight, plus what else overlapped them.
- **Routine review** (Plan → Review): checks the schedule you follow against your weekly set
  targets and recent training: muscles short of or over their sets, lifts that have stopped
  moving, near-identical exercises, lifts slipping late in a session and joints that keep
  complaining. Most findings come with a change you can apply to the template in one tap, and undo.
- **Is it growing?** The Health view also says whether each muscle is likely growing, holding or
  slipping, from strength in the same rep range, the tape, InBody lean mass, weekly sets and food,
  with how sure it is and the evidence behind it.
- **Strength balance:** which muscles and movements are strong or weak next to the rest of you,
  and left against right from the tape and InBody.
- **Sets vs strength gained:** for each muscle, the weekly sets of every 6 weeks against the
  strength they came with.
- **Fatigue check:** a note on Today when several lifts slip together and your watch's HRV,
  resting heart rate or sleep agree.

### Body

- **Bodyweight** with a 7-day average, change per week, a goal (cut, maintain or gain) with a
  weekly target, and weigh-in reminders on the days you choose.
- **Body parts:** log any measurement (waist, neck, arms, …) and chart it. Readings taken within 6
  hours after a workout that trained that muscle are charted as "pumped", separately from rested ones, and once you have a
  few of each, your own pump size fills in estimates for both lines.
- **Body composition:** InBody scans by hand, from a CSV, or **read straight from a photo of an
  InBody 580 result sheet** with on-device OCR. Body fat, lean mass and fat mass are charted
  alongside US Navy and relative-fat-mass tape estimates.
- **How you compare:** percentiles against people of your sex and age for body fat, fat mass and
  lean mass indexes (US body-scan reference data, the same kind InBody shows), weight, BMI, waist
  and upper arm. Each shows a strip with where you started and where you are now.
- **Calories and protein (optional):** energy balance from your weight trend and an adaptive
  TDEE estimate.

### Charts

- **Raw or Trend** (LOESS smoothing). Your choice and the amount of smoothing are remembered. Raw
  shows every data point.
- **Goal pace** on the bodyweight chart: where your weight would be at the goal's target rate.
- **Compare phases:** every cut, gain or maintenance from its own first day, on one chart, and how
  much of a phase's weight change was lean and how much fat.
- **Plateaus** marked on each exercise's estimated 1RM chart.
- **Full screen,** turned sideways on a phone. Drag along the line to read the date and value.

### Sharing

- **Share any chart, workout, exercise or body map as an image:** square (1080×1080) or story
  (1080×1920), dark or light, with an option to hide exact numbers.
- **Save to Photos** or use the Android share sheet (Instagram Stories, Discord, Messages, …).

<img src="docs/screenshots/share-card.png" width="320" alt="A workout shared as an image card">

### Watches and health apps

- **Health Connect** sync on Android: FitLedger reads what your watch or ring app shares with
  Android's Health Connect (Garmin Connect, Fitbit, Oura and others can). That covers weight,
  workouts, heart rate, sleep, resting heart rate, HRV and steps, as far as each app shares them.
  There are no direct connections to Garmin, Fitbit or Oura accounts.
- **Garmin .fit file import** on any platform (single activities or a full account export).
- Heart rate on the workout screen and a recovery line on Today.

### Your data

- **Offline first.** The only network use is a one-time 12 MB download of the InBody sheet reader
  the first time you read a sheet.
- **Backups:** manual backups, a weekly (newest 4 kept) or daily (newest 7 kept) automatic backup
  to `Download/FitLedger`, and Android's own account backup, which restores your data on a new
  phone. On desktop you can choose the folder, such as one that Dropbox or Syncthing watches.
  Automatic backups run when you open the app or finish a workout, and a failed one shows a warning.
- **Safe restores:** a safety copy of your current data is saved before any restore.
- **Exports:** Strong-format workout CSV, bodyweight, InBody, measurements, diet, a full JSON
  export, and a written progress report covering 4, 8 or 12 weeks.
- **kg or lb**, light or dark theme.

## License

[PolyForm Noncommercial 1.0.0](LICENSE.md): free to use, change and share for any non-commercial
purpose (personal use, hobby projects, research, education, charities). Commercial use needs
permission from the author.

## Credits

InBody sheet reading uses the [Ocrs](https://github.com/robertknight/ocrs) engine. Its models
are trained on Google Research's HierText dataset and licensed CC BY-SA 4.0 (see
[crates/ocr/models/README.md](crates/ocr/models/README.md)).

Comparison data: body composition from Kelly, Wilson & Heymsfield (2009), "Dual Energy X-Ray
Absorptiometry Body Composition Reference Values from NHANES", PLoS ONE 4(9): e7038 (CC BY);
weight, BMI, waist and arm from the CDC's Anthropometric Reference Data, NHANES 2015–2018 (public
domain); strength from the [OpenPowerlifting](https://www.openpowerlifting.org) and
[OpenWeightlifting](https://www.openweightlifting.org) projects (public domain); CrossFit athletes
from Ulrik Thyge Pedersen's Kaggle dataset
[Crossfit Athletes](https://www.kaggle.com/datasets/ulrikthygepedersen/crossfit-athletes)
(CC BY 4.0; only percentile tables derived from it are bundled). Rough conversions for other
exercises are fitted to the community standards published by
[Strength Level](https://strengthlevel.com/strength-standards) (only the fitted ratios are kept).
Variation ratios come from Saeterbakken et al. 2011, Cotterman et al. 2005,
Gullett et al. 2009, Atkins & Richards 2016, Swinton et al. 2011 and Larsen et al. 2020 (details in
`crates/core/src/norms.rs`).

For building, testing and releasing, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
