#!/usr/bin/env python3
"""Builds data/library/exercises.json from the curated list plus free-exercise-db.

free-exercise-db (https://github.com/yuhonas/free-exercise-db) is released under the Unlicense.
Its muscles are coarse ("shoulders", "chest"), so they are refined into Liftbook's muscle ids by
name rules. Those entries are marked source="free-exercise-db" so the app can flag them as
auto-mapped.

Usage: python3 scripts/build_library.py [path/to/free-exercise-db/exercises.json]
Without a path the pinned commit is downloaded.
"""

import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "data" / "library"
FEDB_COMMIT = "f00c92c7dcf1216a928a52c3706c7ce8e2f71ed5"
FEDB_URL = "https://raw.githubusercontent.com/yuhonas/free-exercise-db/{}/dist/exercises.json"

EQUIPMENT = set(
    "barbell dumbbell cable machine smith_machine landmine kettlebell band bodyweight cardio "
    "ez_bar trap_bar plate medicine_ball stability_ball other".split()
)
PATTERNS = set(
    "horizontal_push vertical_push horizontal_pull vertical_pull squat hinge lunge isolation core "
    "carry cardio olympic other".split()
)
KINDS = set(
    "weight_reps bodyweight_reps assisted_bodyweight duration distance_duration weight_distance".split()
)


def slugify(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def norm(name):
    return re.sub(r"[^a-z0-9]", "", name.lower())


def word_key(name):
    """Order-insensitive key: "Arnold Dumbbell Press" == "Arnold Press (Dumbbell)"."""
    words = re.findall(r"[a-z0-9]+", name.lower().replace("-", " "))
    words = {w[:-1] if len(w) > 3 and w.endswith("s") and not w.endswith("ss") else w for w in words}
    return " ".join(sorted(words - {"with", "the", "a", "on", "s"}))


def load_curated(muscle_ids):
    out = []
    for lineno, line in enumerate((LIB / "curated.psv").read_text().splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        cols = [c.strip() for c in line.split("|")]
        if len(cols) != 7:
            sys.exit(f"curated.psv:{lineno}: expected 7 columns, got {len(cols)}")
        name, equipment, pattern, kind, primary, secondary, aliases = cols
        split = lambda s, sep: [x.strip() for x in s.split(sep) if x.strip()]
        ex = {
            "slug": slugify(name),
            "name": name,
            "aliases": split(aliases, ";"),
            "equipment": equipment,
            "pattern": pattern,
            "kind": kind,
            "primary": split(primary, ","),
            "secondary": split(secondary, ","),
            "source": "liftbook",
        }
        problems = []
        if equipment not in EQUIPMENT:
            problems.append(f"equipment {equipment!r}")
        if pattern not in PATTERNS:
            problems.append(f"pattern {pattern!r}")
        if kind not in KINDS:
            problems.append(f"kind {kind!r}")
        for m in ex["primary"] + ex["secondary"]:
            if m not in muscle_ids:
                problems.append(f"muscle {m!r}")
        if set(ex["primary"]) & set(ex["secondary"]):
            problems.append("muscle listed as both primary and secondary")
        if not ex["primary"] and pattern != "cardio":
            problems.append("no primary muscle")
        if problems:
            sys.exit(f"curated.psv:{lineno}: unknown/invalid " + ", ".join(problems))
        out.append(ex)
    return out


# ---- free-exercise-db refinement

FEDB_CATEGORIES = {"strength", "powerlifting", "olympic weightlifting", "strongman", "cardio"}
FEDB_EQUIPMENT = {
    "barbell": "barbell",
    "dumbbell": "dumbbell",
    "cable": "cable",
    "machine": "machine",
    "body only": "bodyweight",
    "kettlebells": "kettlebell",
    "bands": "band",
    "e-z curl bar": "ez_bar",
    "medicine ball": "medicine_ball",
    "exercise ball": "stability_ball",
    "foam roll": "other",
    "other": "other",
    None: "other",
}


def has(name, *words):
    return any(w in name for w in words)


def refine_muscle(m, name):
    n = name.lower()
    if m == "chest":
        return "upper_chest" if has(n, "incline", "low-to-high", "low to high") else "lower_chest"
    if m == "shoulders":
        if has(n, "lateral", "side raise", "side lateral", "upright", "y-raise"):
            return "side_delts"
        if has(n, "rear", "reverse fly", "reverse flye", "face pull", "bent over raise", "bent-over raise", "pull apart"):
            return "rear_delts"
        return "front_delts"
    if m == "forearms":
        return "forearm_extensors" if has(n, "reverse", "extensor", "hammer", "palms-down", "palms down") else "forearm_flexors"
    if m == "abdominals":
        return "obliques" if has(n, "twist", "oblique", "side", "woodchop", "wood chop", "russian", "windmill", "pallof") else "abs"
    return {
        "lats": "lats",
        "middle back": "upper_back",
        "lower back": "lower_back",
        "traps": "traps",
        "biceps": "biceps",
        "triceps": "triceps",
        "quadriceps": "quads",
        "hamstrings": "hamstrings",
        "glutes": "glutes",
        "adductors": "adductors",
        "abductors": "abductors",
        "calves": "calves",
        "neck": "neck",
    }[m]


def fedb_pattern(e, primary_raw):
    n = e["name"].lower()
    if e["category"] == "cardio":
        return "cardio"
    if e["category"] == "olympic weightlifting":
        return "olympic"
    p = set(primary_raw)
    if "abdominals" in p:
        return "core"
    if has(n, "carry", "farmer", "walk"):
        return "carry"
    if e.get("mechanic") == "isolation":
        return "isolation"
    if has(n, "lunge", "split squat", "step-up", "step up"):
        return "lunge"
    if "quadriceps" in p:
        return "squat"
    if p & {"hamstrings", "glutes", "lower back"}:
        return "hinge"
    if e.get("force") == "push":
        return "vertical_push" if "shoulders" in p and "chest" not in p else "horizontal_push"
    if e.get("force") == "pull":
        if "lats" in p and has(n, "pulldown", "pull-up", "pullup", "chin"):
            return "vertical_pull"
        if p & {"lats", "middle back"}:
            return "horizontal_pull"
    return "isolation" if e.get("mechanic") == "isolation" else "other"


def fedb_equipment(e):
    n = e["name"].lower()
    if "smith" in n:
        return "smith_machine"
    if "landmine" in n:
        return "landmine"
    if "trap bar" in n or "hex bar" in n:
        return "trap_bar"
    if "plate" in n and e.get("equipment") in ("other", None):
        return "plate"
    if e["category"] == "cardio":
        return "cardio"
    return FEDB_EQUIPMENT[e.get("equipment")]


def fedb_kind(e, equipment):
    if e["category"] == "cardio":
        return "distance_duration"
    if e.get("force") == "static" and equipment == "bodyweight":
        return "duration"
    if equipment == "bodyweight":
        return "bodyweight_reps"
    return "weight_reps"


def load_fedb(path, taken_names):
    data = json.loads(Path(path).read_text()) if path else json.loads(
        urllib.request.urlopen(FEDB_URL.format(FEDB_COMMIT)).read()
    )
    out = []
    for e in data:
        if e.get("category") not in FEDB_CATEGORIES:
            continue
        if norm(e["name"]) in taken_names or word_key(e["name"]) in taken_names:
            continue
        name = e["name"]
        primary = []
        for m in e.get("primaryMuscles", []):
            r = refine_muscle(m, name)
            if r not in primary:
                primary.append(r)
        secondary = []
        for m in e.get("secondaryMuscles", []):
            r = refine_muscle(m, name)
            if r not in primary and r not in secondary:
                secondary.append(r)
        equipment = fedb_equipment(e)
        out.append(
            {
                "slug": "fedb-" + slugify(e["id"]),
                "name": name,
                "aliases": [],
                "equipment": equipment,
                "pattern": fedb_pattern(e, e.get("primaryMuscles", [])),
                "kind": fedb_kind(e, equipment),
                "primary": primary,
                "secondary": secondary,
                "source": "free-exercise-db",
            }
        )
        taken_names.update({norm(name), word_key(name)})
    return out


def main():
    muscles = json.loads((LIB / "muscles.json").read_text())
    muscle_ids = {m["id"] for m in muscles["muscles"]}
    curated = load_curated(muscle_ids)

    seen = {}
    for ex in curated:
        for n in [ex["name"], *ex["aliases"]]:
            k = norm(n)
            if k in seen and seen[k] != ex["name"]:
                sys.exit(f"name/alias {n!r} used by both {seen[k]!r} and {ex['name']!r}")
            seen[k] = ex["name"]
    slugs = [ex["slug"] for ex in curated]
    if len(slugs) != len(set(slugs)):
        sys.exit("duplicate slug in curated.psv")

    taken = set(seen) | {word_key(n) for ex in curated for n in [ex["name"], *ex["aliases"]]}
    extra = load_fedb(sys.argv[1] if len(sys.argv) > 1 else None, taken)
    exercises = curated + extra
    out = {
        "attribution": "Includes exercises from free-exercise-db (https://github.com/yuhonas/free-exercise-db), released under the Unlicense.",
        "exercises": exercises,
    }
    (LIB / "exercises.json").write_text(json.dumps(out, indent=0, ensure_ascii=False) + "\n")
    print(f"{len(curated)} curated + {len(extra)} from free-exercise-db = {len(exercises)} exercises")


if __name__ == "__main__":
    main()
