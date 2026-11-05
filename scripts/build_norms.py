#!/usr/bin/env python3
"""Build the percentile reference tables in data/norms/ from their published sources.

    python3 scripts/build_norms.py --kelly <dir> --cdc <report.pdf> --opl <openpowerlifting.csv>

Sources (download them yourself, they are not kept in git):
  --kelly  Kelly TL, Wilson KE, Heymsfield SB (2009) "Dual Energy X-Ray Absorptiometry Body
           Composition Reference Values from NHANES", PLoS ONE 4(9): e7038, CC BY. Directory with
           the supplementary Word tables S1, S2, S5 and S6 (journal.pone.0007038.s021, s022,
           s025, s026), named s021, s022, s025, s026.
  --cdc    Fryar CD et al. (2021) "Anthropometric Reference Data for Children and Adults: United
           States, 2015-2018", Vital Health Stat 3(46), public domain.
           https://stacks.cdc.gov/view/cdc/100478/cdc_100478_DS1.pdf
  --opl    The OpenPowerlifting data (public domain),
           https://openpowerlifting.gitlab.io/opl-csv/files/openpowerlifting-latest.zip

Needs soffice (Word -> text) and pdftotext.
"""

import argparse
import csv
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import defaultdict

ROOT = os.path.join(os.path.dirname(__file__), "..", "data", "norms")

# ---------- Kelly et al. 2009: LMS curves (age 20-85, every 5 years) ----------

KELLY = [("s021", "fmi"), ("s022", "pbf"), ("s025", "lmi"), ("s026", "almi")]
GROUPS = ["white", "black", "mexican_american"]


def kelly(directory):
    out = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, key in KELLY:
            subprocess.run(
                ["soffice", "--headless", "--convert-to", "txt:Text", os.path.join(directory, name), "--outdir", tmp],
                check=True,
                capture_output=True,
            )
            text = open(os.path.join(tmp, name + ".txt"), encoding="utf-8-sig").read()
            toks = [t.strip() for t in text.split("\n") if t.strip()]
            sex, table = None, {"male": {}, "female": {}}
            i = 0
            while i < len(toks):
                t = toks[i]
                if t == "Males":
                    sex = "male"
                elif t == "Females":
                    sex = "female"
                elif sex and t.isdigit() and 20 <= int(t) <= 85:
                    vals = [float(x) for x in toks[i + 1 : i + 10]]
                    for g, j in zip(GROUPS, (0, 3, 6)):
                        m, sd, l = vals[j : j + 3]
                        # The tables give sigma = S * M; store L, M, S.
                        table[sex].setdefault(g, []).append([int(t), round(l, 3), m, round(sd / m, 5)])
                    i += 10
                    continue
                i += 1
            for s in table:
                for g in GROUPS:
                    assert len(table[s][g]) == 14, (name, s, g)
            out[key] = table
    return out


# ---------- CDC 2015-2018: percentiles by 10-year age band ----------

CDC = [("weight_kg", 3, 5), ("bmi", 14, 15), ("waist_cm", 19, 20), ("arm_cm", 22, 23)]
BANDS = ["20–29", "30–39", "40–49", "50–59", "60–69", "70–79", "80 and over"]


def cdc(pdf):
    text = subprocess.run(["pdftotext", "-layout", pdf, "-"], check=True, capture_output=True, text=True).stdout
    lines = text.split("\n")
    starts = {}
    for i, l in enumerate(lines):
        m = re.search(r"Table (\d+)\. ", l)
        if m and "adult" in l and int(m.group(1)) not in starts:
            starts[int(m.group(1))] = i
    out = {"percentiles": [5, 10, 15, 25, 50, 75, 85, 90, 95], "age_from": [20, 30, 40, 50, 60, 70, 80]}
    for key, female, male in CDC:
        out[key] = {}
        for sex, t in (("female", female), ("male", male)):
            rows = []
            # The first block of each table is "All race and Hispanic-origin groups".
            for l in lines[starts[t] : starts[t] + 40]:
                m = re.match(r"\s+(" + "|".join(BANDS) + r")\s+[\d,]+\s+(.*)", l)
                if m:
                    rows.append([float(x) for x in m.group(2).split()[2:]])
                if len(rows) == 7:
                    break
            assert len(rows) == 7 and all(len(r) == 9 for r in rows), (key, sex)
            out[key][sex] = rows
    return out


# ---------- OpenPowerlifting: raw, drug-tested competitors ----------

LIFTS = {"squat": "Best3SquatKg", "bench": "Best3BenchKg", "deadlift": "Best3DeadliftKg"}
AGE_FROM = [18, 40, 50, 60, 70]  # bands: 18-39, 40-49, 50-59, 60-69, 70+
BW_FROM = list(range(40, 160, 10))  # 10 kg bins: 40-49.9, ..., 150+ (below 40 dropped)
PCTS = list(range(5, 100, 5))
MIN_N = 100


def band(x, starts):
    b = None
    for i, s in enumerate(starts):
        if x >= s:
            b = i
    return b


def opl(path):
    # Best lift per lifter, lift and age band, with the bodyweight of that entry.
    best = {}
    csv.field_size_limit(sys.maxsize)
    with open(path, newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            if r["Equipment"] != "Raw" or r["Tested"] != "Yes" or r["Sex"] not in ("M", "F"):
                continue
            if not r["Place"].isdigit() or not r["Age"] or not r["BodyweightKg"]:
                continue
            age, bw = float(r["Age"]), float(r["BodyweightKg"])
            a, w = band(age, AGE_FROM), band(bw, BW_FROM)
            if a is None or w is None:
                continue
            for lift, col in LIFTS.items():
                v = float(r[col]) if r[col] else 0.0
                if v <= 0:
                    continue
                k = (r["Name"], r["Sex"], lift, a)
                if k not in best or v > best[k][0]:
                    best[k] = (v, w)
    cells = defaultdict(list)
    for (_, sex, lift, a), (v, w) in best.items():
        cells[(lift, sex, a, w)].append(v)
    out = {"percentiles": PCTS, "age_from": AGE_FROM, "bw_from": BW_FROM, "min_n": MIN_N}
    for lift in LIFTS:
        out[lift] = {}
        for sex, name in (("M", "male"), ("F", "female")):
            grid = []
            for a in range(len(AGE_FROM)):
                row = []
                for w in range(len(BW_FROM)):
                    vals = sorted(cells.get((lift, sex, a, w), []))
                    if len(vals) < MIN_N:
                        row.append(None)
                        continue
                    q = lambda p: vals[min(len(vals) - 1, int(p / 100 * len(vals)))]
                    row.append({"n": len(vals), "p": [round(q(p), 1) for p in PCTS]})
                grid.append(row)
            out[lift][name] = grid
    return out


def write(name, data):
    os.makedirs(ROOT, exist_ok=True)
    with open(os.path.join(ROOT, name), "w") as f:
        json.dump(data, f, separators=(",", ":"))
        f.write("\n")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--kelly")
    ap.add_argument("--cdc")
    ap.add_argument("--opl")
    args = ap.parse_args()
    if args.kelly:
        write("body_composition.json", kelly(args.kelly))
    if args.cdc:
        write("body_size.json", cdc(args.cdc))
    if args.opl:
        write("strength.json", opl(args.opl))
