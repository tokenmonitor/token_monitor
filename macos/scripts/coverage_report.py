#!/usr/bin/env python3
"""Prints the app target's test coverage: totals, per folder and per file.

Usage:
  macos/scripts/coverage_report.py [--derived-data DIR] [--xcresult PATH]
                                   [--files N] [--markdown]

Run it after `make test` (or use `make coverage`, which does both). It reads
two views of the same run, both limited to sources under macos/TokenMon/:

* Lines and functions come from `xcrun xccov view --report --json` on the
  newest .xcresult bundle, the numbers Xcode shows.
* Regions come from `xcrun llvm-cov export -summary-only` on the raw
  Coverage.profdata against the Debug app binary. Swift emits no branch
  regions, so branches print as n/a unless the toolchain starts reporting
  them.

Folders are the first two path components under TokenMon/ (Core/Usage,
Providers/Claude, ...) plus the top level (Core, Providers, ...). Files are
sorted by uncovered lines, most first; --files limits how many are printed
(0 prints all). --markdown prints GitHub-flavoured tables for a CI step
summary instead of plain text.

Standard library only. Exits non-zero when no app source has coverage data.
"""
import argparse
import glob
import json
import os
import subprocess
import sys

APP_TARGET = "TokenMon.app"
DIMENSIONS = ("lines", "functions", "regions", "branches")


class Counter:
    """Covered and total counts for each dimension."""

    def __init__(self):
        self.covered = {d: 0 for d in DIMENSIONS}
        self.total = {d: 0 for d in DIMENSIONS}

    def add(self, dimension, covered, total):
        self.covered[dimension] += covered
        self.total[dimension] += total

    def merge(self, other):
        for d in DIMENSIONS:
            self.add(d, other.covered[d], other.total[d])

    def percent(self, dimension):
        total = self.total[dimension]
        return None if total == 0 else 100.0 * self.covered[dimension] / total

    def uncovered(self, dimension):
        return self.total[dimension] - self.covered[dimension]


def newest(pattern):
    matches = glob.glob(pattern)
    return max(matches, key=os.path.getmtime) if matches else None


def app_relative(path, app_root):
    real = os.path.realpath(path)
    return os.path.relpath(real, app_root) if real.startswith(app_root + os.sep) else None


def read_xccov(xcresult, app_root, files):
    report = json.loads(subprocess.check_output(["xcrun", "xccov", "view", "--report", "--json", xcresult]))
    for target in report.get("targets", []):
        if target.get("name") != APP_TARGET:
            continue
        for entry in target.get("files", []):
            rel = app_relative(entry["path"], app_root)
            if rel is None:
                continue
            counter = files.setdefault(rel, Counter())
            counter.add("lines", entry.get("coveredLines", 0), entry.get("executableLines", 0))
            functions = entry.get("functions", [])
            hit = sum(1 for f in functions if f.get("executionCount", 0) > 0)
            counter.add("functions", hit, len(functions))


def read_llvm_cov(profdata, binary, app_root, files):
    export = json.loads(
        subprocess.check_output(
            ["xcrun", "llvm-cov", "export", "-summary-only", "-instr-profile", profdata, binary]
        )
    )
    for data in export.get("data", []):
        for entry in data.get("files", []):
            rel = app_relative(entry["filename"], app_root)
            if rel is None:
                continue
            counter = files.setdefault(rel, Counter())
            summary = entry["summary"]
            for dimension in ("regions", "branches"):
                part = summary.get(dimension, {})
                counter.add(dimension, part.get("covered", 0), part.get("count", 0))


def folders_of(rel):
    parts = rel.split(os.sep)[:-1]
    return [os.sep.join(parts[:depth]) for depth in (1, 2) if len(parts) >= depth] or ["(root)"]


def fmt_percent(counter, dimension):
    value = counter.percent(dimension)
    return "n/a" if value is None else f"{value:.1f}%"


def fmt_cell(counter, dimension):
    if counter.total[dimension] == 0:
        return "n/a"
    return f"{fmt_percent(counter, dimension)} ({counter.covered[dimension]}/{counter.total[dimension]})"


def print_table(title, rows, markdown):
    """Rows are (name, Counter) pairs."""
    header = ["", "lines", "functions", "regions", "branches"]
    body = [[name] + [fmt_cell(c, d) for d in DIMENSIONS] for name, c in rows]
    if markdown:
        print(f"### {title}\n")
        print("| " + " | ".join(header) + " |")
        print("|---|" + "---:|" * (len(header) - 1))
        for row in body:
            print("| " + " | ".join(row) + " |")
        print()
        return
    widths = [max(len(str(r[i])) for r in [header] + body) for i in range(len(header))]
    print(title)
    print("  ".join(h.ljust(widths[0]) if i == 0 else h.rjust(widths[i]) for i, h in enumerate(header)))
    for row in body:
        print("  ".join(v.ljust(widths[0]) if i == 0 else v.rjust(widths[i]) for i, v in enumerate(row)))
    print()


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    macos_root = os.path.dirname(here)
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--derived-data", default=os.path.join(macos_root, "build", "DerivedData"))
    parser.add_argument("--xcresult", help="defaults to the newest bundle under <derived-data>/Logs/Test")
    parser.add_argument("--files", type=int, default=40, help="files to list, most uncovered lines first (0 = all)")
    parser.add_argument("--markdown", action="store_true", help="print Markdown tables")
    args = parser.parse_args()

    app_root = os.path.realpath(os.path.join(macos_root, "TokenMon"))
    derived = os.path.abspath(args.derived_data)
    xcresult = args.xcresult or newest(os.path.join(derived, "Logs", "Test", "*.xcresult"))
    if not xcresult:
        print(f"No .xcresult under {derived}/Logs/Test; run `make test` first.", file=sys.stderr)
        return 1

    files = {}
    read_xccov(xcresult, app_root, files)
    profdata = newest(os.path.join(derived, "Build", "ProfileData", "*", "Coverage.profdata"))
    binary = os.path.join(derived, "Build", "Products", "Debug", "TokenMon.app", "Contents", "MacOS", "TokenMon")
    if profdata and os.path.exists(binary):
        read_llvm_cov(profdata, binary, app_root, files)
    else:
        print("No Coverage.profdata or Debug app binary; regions and branches are n/a.", file=sys.stderr)

    if not files:
        print(f"No coverage for sources under {app_root} in {xcresult}", file=sys.stderr)
        return 1

    total = Counter()
    folders = {}
    for rel, counter in files.items():
        total.merge(counter)
        for folder in folders_of(rel):
            folders.setdefault(folder, Counter()).merge(counter)

    print_table("Total (macos/TokenMon, app target)", [("all", total)], args.markdown)
    top = sorted(f for f in folders if os.sep not in f)
    print_table("By top-level folder", [(f, folders[f]) for f in top], args.markdown)
    print_table("By folder", [(f, folders[f]) for f in sorted(folders) if os.sep in f], args.markdown)
    ranked = sorted(files.items(), key=lambda kv: (-kv[1].uncovered("lines"), kv[0]))
    if args.files > 0:
        ranked = ranked[: args.files]
    title = "Files by uncovered lines" + (f" (top {args.files})" if args.files > 0 else "")
    print_table(title, ranked, args.markdown)
    return 0


if __name__ == "__main__":
    sys.exit(main())
