#!/usr/bin/env python3
"""Build data/norms/conversions.json: rough conversions from other exercises to a lift with
reference data, fitted to Strength Level's published community standards.

    python3 scripts/build_conversions.py <cache dir>

For each exercise, Strength Level (strengthlevel.com/strength-standards/<page>/kg) lists the
whole community's Beginner, Novice, Intermediate, Advanced and Elite 1RM for men and women. The
same five levels of the anchor lift (bench, squat, deadlift or clean and jerk) are fitted with
exercise ≈ a + b × anchor by least squares, per sex. Only a and b are kept, not the standards.
Strength Level lists dumbbell weights per dumbbell; for two-dumbbell exercises they're doubled so
the conversion is in both dumbbells together, as the app compares them.

Exercises that already have a study-based conversion (crates/core/src/norms.rs) aren't listed.
Pages are cached in <cache dir> (fetched once, about one a second).
"""

import html
import json
import os
import re
import sys
import time
import urllib.request

ANCHORS = {"bench": "bench-press", "squat": "squat", "deadlift": "deadlift", "clean_jerk": "clean-and-jerk"}

# Library slug: (anchor lift, Strength Level page, two dumbbells listed per dumbbell)
EXERCISES = {
    "decline-bench-press-barbell": ("bench", "decline-bench-press", False),
    "floor-press-barbell": ("bench", "floor-press", False),
    "decline-bench-press-dumbbell": ("bench", "decline-dumbbell-bench-press", True),
    "chest-press-machine": ("bench", "chest-press", False),
    "landmine-press": ("bench", "landmine-press", False),
    "chest-fly-dumbbell": ("bench", "dumbbell-fly", True),
    "incline-chest-fly-dumbbell": ("bench", "incline-dumbbell-fly", True),
    "pec-deck-machine": ("bench", "machine-chest-fly", False),
    "cable-crossover": ("bench", "cable-fly", False),
    "cable-fly-low-to-high": ("bench", "cable-fly", False),
    "cable-fly-high-to-low": ("bench", "cable-fly", False),
    "seated-cable-fly": ("bench", "cable-fly", False),
    "pullover-dumbbell": ("bench", "dumbbell-pullover", False),
    "overhead-press-barbell": ("bench", "overhead-press", False),
    "seated-overhead-press-barbell": ("bench", "seated-shoulder-press", False),
    "push-press-barbell": ("bench", "push-press", False),
    "overhead-press-dumbbell": ("bench", "dumbbell-shoulder-press", True),
    "seated-overhead-press-dumbbell": ("bench", "seated-dumbbell-shoulder-press", True),
    "arnold-press-dumbbell": ("bench", "arnold-press", True),
    "shoulder-press-machine": ("bench", "machine-shoulder-press", False),
    "lateral-raise-dumbbell": ("bench", "dumbbell-lateral-raise", True),
    "seated-lateral-raise-dumbbell": ("bench", "dumbbell-lateral-raise", True),
    "lateral-raise-cable": ("bench", "cable-lateral-raise", False),
    "lateral-raise-machine": ("bench", "machine-lateral-raise", False),
    "upright-row-barbell": ("bench", "upright-row", False),
    "upright-row-cable": ("bench", "cable-upright-row", False),
    "upright-row-dumbbell": ("bench", "dumbbell-upright-row", True),
    "front-raise-dumbbell": ("bench", "dumbbell-front-raise", True),
    "reverse-fly-dumbbell": ("bench", "dumbbell-reverse-fly", True),
    "reverse-fly-machine": ("bench", "machine-reverse-fly", False),
    "reverse-fly-cable": ("bench", "cable-reverse-fly", False),
    "face-pull-cable": ("bench", "face-pull", False),
    "lat-pulldown-cable": ("bench", "lat-pulldown", False),
    "lat-pulldown-machine": ("bench", "lat-pulldown", False),
    "close-grip-lat-pulldown-cable": ("bench", "close-grip-lat-pulldown", False),
    "reverse-grip-lat-pulldown-cable": ("bench", "reverse-grip-lat-pulldown", False),
    "single-arm-lat-pulldown-cable": ("bench", "one-arm-lat-pulldown", False),
    "straight-arm-pulldown-cable": ("bench", "straight-arm-pulldown", False),
    "bent-over-row-barbell": ("bench", "bent-over-row", False),
    "pendlay-row-barbell": ("bench", "pendlay-row", False),
    "seal-row-barbell": ("bench", "seal-row", False),
    "bent-over-row-dumbbell": ("bench", "bent-over-dumbbell-row", True),
    "bent-over-one-arm-row-dumbbell": ("bench", "dumbbell-row", False),
    "chest-supported-row-dumbbell": ("bench", "chest-supported-dumbbell-row", True),
    "t-bar-row": ("bench", "t-bar-row", False),
    "meadows-row": ("bench", "meadows-row", False),
    "seated-row-cable": ("bench", "seated-cable-row", False),
    "seated-wide-grip-row-cable": ("bench", "seated-cable-row", False),
    "seated-row-machine": ("bench", "machine-row", False),
    "shrug-barbell": ("deadlift", "barbell-shrug", False),
    "shrug-dumbbell": ("deadlift", "dumbbell-shrug", True),
    "shrug-smith-machine": ("deadlift", "smith-machine-shrug", False),
    "shrug-machine": ("deadlift", "machine-shrug", False),
    "shrug-trap-bar": ("deadlift", "hex-bar-shrug", False),
    "back-extension-machine": ("deadlift", "machine-back-extension", False),
    "good-morning-barbell": ("deadlift", "good-morning", False),
    "romanian-deadlift-barbell": ("deadlift", "romanian-deadlift", False),
    "romanian-deadlift-dumbbell": ("deadlift", "dumbbell-romanian-deadlift", True),
    "stiff-leg-deadlift-barbell": ("deadlift", "stiff-leg-deadlift", False),
    "single-leg-romanian-deadlift-dumbbell": ("deadlift", "single-leg-romanian-deadlift", False),
    "hip-thrust-barbell": ("deadlift", "hip-thrust", False),
    "glute-bridge-barbell": ("deadlift", "barbell-glute-bridge", False),
    "cable-pull-through": ("deadlift", "cable-pull-through", False),
    "glute-kickback-cable": ("squat", "cable-kickback", False),
    "box-squat-barbell": ("squat", "box-squat", False),
    "pause-squat-barbell": ("squat", "pause-squat", False),
    "goblet-squat-dumbbell": ("squat", "goblet-squat", False),
    "goblet-squat-kettlebell": ("squat", "goblet-squat", False),
    "hack-squat-machine": ("squat", "hack-squat", False),
    "belt-squat-machine": ("squat", "belt-squat", False),
    "leg-press": ("squat", "sled-leg-press", False),
    "single-leg-leg-press": ("squat", "single-leg-press", False),
    "landmine-squat": ("squat", "landmine-squat", False),
    "thruster-barbell": ("squat", "thruster", False),
    "lunge-barbell": ("squat", "barbell-lunge", False),
    "lunge-dumbbell": ("squat", "dumbbell-lunge", True),
    "bulgarian-split-squat-dumbbell": ("squat", "dumbbell-bulgarian-split-squat", True),
    "bulgarian-split-squat-barbell": ("squat", "bulgarian-split-squat", False),
    "leg-extension-machine": ("squat", "leg-extension", False),
    "lying-leg-curl-machine": ("squat", "lying-leg-curl", False),
    "seated-leg-curl-machine": ("squat", "seated-leg-curl", False),
    "standing-leg-curl-machine": ("squat", "standing-leg-curl", False),
    "hip-adductor-machine": ("squat", "hip-adduction", False),
    "hip-abductor-machine": ("squat", "hip-abduction", False),
    "standing-calf-raise-machine": ("squat", "machine-calf-raise", False),
    "standing-calf-raise-dumbbell": ("squat", "dumbbell-calf-raise", True),
    "standing-calf-raise-barbell": ("squat", "barbell-calf-raise", False),
    "seated-calf-raise-machine": ("squat", "seated-calf-raise", False),
    "calf-press-on-leg-press": ("squat", "leg-press-calf-raise", False),
    "bicep-curl-barbell": ("bench", "barbell-curl", False),
    "bicep-curl-ez-bar": ("bench", "ez-bar-curl", False),
    "bicep-curl-dumbbell": ("bench", "dumbbell-curl", True),
    "bicep-curl-cable": ("bench", "cable-bicep-curl", False),
    "bicep-curl-machine": ("bench", "machine-bicep-curl", False),
    "hammer-curl-dumbbell": ("bench", "hammer-curl", True),
    "hammer-curl-cable": ("bench", "cable-hammer-curl", False),
    "incline-curl-dumbbell": ("bench", "incline-dumbbell-curl", True),
    "spider-curl-dumbbell": ("bench", "spider-curl", True),
    "preacher-curl-barbell": ("bench", "preacher-curl", False),
    "concentration-curl-dumbbell": ("bench", "dumbbell-concentration-curl", False),
    "reverse-curl-barbell": ("bench", "reverse-barbell-curl", False),
    "triceps-pushdown-rope": ("bench", "tricep-rope-pushdown", False),
    "triceps-pushdown-straight-bar": ("bench", "tricep-pushdown", False),
    "triceps-pushdown-v-bar": ("bench", "tricep-pushdown", False),
    "overhead-triceps-extension-cable": ("bench", "cable-overhead-tricep-extension", False),
    "overhead-triceps-extension-dumbbell": ("bench", "dumbbell-tricep-extension", False),
    "triceps-extension-machine": ("bench", "machine-tricep-extension", False),
    "skullcrusher-barbell": ("bench", "lying-tricep-extension", False),
    "triceps-kickback-dumbbell": ("bench", "dumbbell-tricep-kickback", True),
    "jm-press-barbell": ("bench", "jm-press", False),
    "wrist-curl-barbell": ("bench", "wrist-curl", False),
    "wrist-curl-dumbbell": ("bench", "dumbbell-wrist-curl", True),
    "reverse-wrist-curl-barbell": ("bench", "reverse-wrist-curl", False),
    "reverse-wrist-curl-dumbbell": ("bench", "dumbbell-reverse-wrist-curl", True),
    "power-clean-barbell": ("clean_jerk", "power-clean", False),
    "hang-clean-barbell": ("clean_jerk", "hang-clean", False),
}

LEVELS = r"Beginner ([\d.]+) kg Novice ([\d.]+) kg Intermediate ([\d.]+) kg Advanced ([\d.]+) kg Elite ([\d.]+) kg"


def standards(cache, page):
    path = os.path.join(cache, page + ".html")
    if not os.path.exists(path):
        time.sleep(1)
        req = urllib.request.Request(f"https://strengthlevel.com/strength-standards/{page}/kg", headers={"User-Agent": "Mozilla/5.0"})
        with open(path, "w") as f:
            f.write(urllib.request.urlopen(req, timeout=30).read().decode())
    t = open(path).read()
    t = re.sub(r"<script.*?</script>|<style.*?</style>", "", t, flags=re.S)
    t = re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", t)))
    title = re.search(r"(.*?) Standards for Men and Women", t).group(1).strip()
    n = int(re.search(r"based on ([\d,]+) lifts", t).group(1).replace(",", ""))
    # The men's table comes first, then the women's.
    male = re.search(r"Male \S.{0,60}? Standards \(kg\) Entire Community .*?" + LEVELS, t)
    female = re.search(r"Female \S.{0,60}? Standards \(kg\) Entire Community .*?" + LEVELS, t[male.end() :])
    return title, n, [float(x) for x in male.groups()], [float(x) for x in female.groups()]


def fit(x, y):
    mx, my = sum(x) / len(x), sum(y) / len(y)
    b = sum((a - mx) * (c - my) for a, c in zip(x, y)) / sum((a - mx) ** 2 for a in x)
    a = my - b * mx
    ss = sum((c - my) ** 2 for c in y)
    r2 = 1 - sum((c - (a + b * v)) ** 2 for v, c in zip(x, y)) / ss
    return round(a, 1), round(b, 3), r2


def main(cache):
    os.makedirs(cache, exist_ok=True)
    anchors = {k: standards(cache, v) for k, v in ANCHORS.items()}
    out = {}
    for slug, (anchor, page, pair) in EXERCISES.items():
        title, n, male, female = standards(cache, page)
        if pair:
            male, female = [v * 2 for v in male], [v * 2 for v in female]
        am, af = fit(anchors[anchor][2], male), fit(anchors[anchor][3], female)
        assert am[1] > 0 and af[1] > 0 and min(am[2], af[2]) > 0.95, (slug, am, af)
        out[slug] = {"lift": anchor, "male": am[:2], "female": af[:2], "title": title, "anchor": anchors[anchor][0], "n": n, "pair": pair}
    path = os.path.join(os.path.dirname(__file__), "..", "data", "norms", "conversions.json")
    with open(path, "w") as f:
        json.dump(out, f, separators=(",", ":"), ensure_ascii=False)
        f.write("\n")
    print(len(out), "conversions")


if __name__ == "__main__":
    main(sys.argv[1])
