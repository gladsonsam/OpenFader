#!/usr/bin/env python3
"""Ratchet guard: keep Flutter screens on the app/lib/ui library.

Counts raw Material widgets, numeric font sizes, color literals and numeric
breakpoints per file against scripts/ui_guard_baseline.json. Counts may only
go down. Commands: check (default), update [--allow-increase], report.
"""
import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "app" / "lib"
BASELINE = Path(__file__).resolve().parent / "ui_guard_baseline.json"
EXCLUDE = ("ui/", "theme.dart", "palette.dart", "design.dart", "bridge/generated/")

RULES = {
    "raw-button": (
        r"\b(?:TextButton|FilledButton|ElevatedButton|OutlinedButton)(?:\.icon|\.tonal|\.tonalIcon)?\(",
        "use ConsoleButton(variant:, size:)",
    ),
    "raw-icon-button": (r"\bIconButton(?:\.filled|\.outlined)?\(", "use ConsoleIconButton"),
    "raw-dialog": (
        r"\b(?:AlertDialog|ConsoleAlertDialog|SimpleDialog)\(|\bshowDialog\b",
        "use ConsoleDialog / showConsoleDialog / showConsoleConfirmation / showConsoleChoice / showConsoleTextInput",
    ),
    "numeric-font-size": (
        r"fontSize:\s*[0-9]|AppDesign\.type\(\s*[0-9]",
        "use ConsoleText / consoleTextStyle roles",
    ),
    "color-literal": (r"\bColor\(0x|\bColors\.(?!transparent\b)\w+", "use ConsolePalette"),
    "numeric-breakpoint": (
        r"(?:sizeOf\(context\)\.width|maxWidth|\.width)\s*[<>]=?\s*[0-9]{3,4}\b",
        "use AppDesign breakpoint tokens",
    ),
}
PATTERNS = {k: re.compile(v[0]) for k, v in RULES.items()}


def excluded(rel):
    return any(rel == e or (e.endswith("/") and rel.startswith(e)) for e in EXCLUDE)


def files():
    for p in sorted(LIB.rglob("*.dart")):
        rel = p.relative_to(LIB).as_posix()
        if not excluded(rel):
            yield p, "app/lib/" + rel


def scan():
    """Return {path: {rule: [(line, text)]}}."""
    out = {}
    for p, rel in files():
        hits = {}
        for n, line in enumerate(p.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            code = line.split("//", 1)[0]
            for rule, pat in PATTERNS.items():
                for m in pat.finditer(code):
                    hits.setdefault(rule, []).append((n, m.group(0)))
        if hits:
            out[rel] = hits
    return out


def counts(found):
    return {f: {r: len(v) for r, v in sorted(h.items())} for f, h in sorted(found.items())}


def load():
    return json.loads(BASELINE.read_text()) if BASELINE.exists() else {}


def increases(cur, base):
    return [(f, r, base.get(f, {}).get(r, 0), n) for f, rs in cur.items() for r, n in rs.items()
            if n > base.get(f, {}).get(r, 0)]


def check():
    found = scan()
    cur, base = counts(found), load()
    bad = increases(cur, base)
    for f, r, old, new in bad:
        print(f"{f}: [{r}] {new} found, baseline {old} ({RULES[r][1]})")
        for n, text in found[f][r]:
            print(f"{f}:{n}: [{r}] {text} - {RULES[r][1]}")
    lower = sum(1 for f, rs in base.items() for r, n in rs.items() if cur.get(f, {}).get(r, 0) < n)
    gone = [f for f in base if not (ROOT / f).exists()]
    if gone:
        print(f"note: baseline lists missing files: {', '.join(gone)}")
    if lower:
        print(f"note: {lower} count(s) below baseline; run `python3 scripts/ui_guard.py update` to lower it")
    if bad:
        print("ui_guard: FAILED (new violations; use the ui/ library)")
        return 1
    print("ui_guard: ok")
    return 0


def update(allow):
    cur = counts(scan())
    bad = increases(cur, load())
    if bad and not allow:
        for f, r, old, new in bad:
            print(f"{f}: [{r}] {old} -> {new}")
        print("ui_guard: refusing to raise the baseline (--allow-increase to override)")
        return 1
    BASELINE.write_text(json.dumps(cur, indent=1, sort_keys=True) + "\n")
    print(f"ui_guard: baseline written ({sum(len(v) for v in cur.values())} entries)")
    return 0


def report():
    cur = counts(scan())
    totals = Counter()
    per_file = Counter()
    for f, rs in cur.items():
        for r, n in rs.items():
            totals[r] += n
            per_file[f] += n
    for r in RULES:
        print(f"{r:20} {totals[r]}")
    print(f"{'total':20} {sum(totals.values())}\n\ntop files:")
    for f, n in per_file.most_common(15):
        print(f"{n:5}  {f}  {cur[f]}")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cmd", nargs="?", default="check", choices=["check", "update", "report"])
    ap.add_argument("--allow-increase", action="store_true")
    a = ap.parse_args()
    sys.exit({"check": check, "update": lambda: update(a.allow_increase), "report": report}[a.cmd]())


if __name__ == "__main__":
    main()
