#!/usr/bin/env python3
"""Pick the cargo-fuzz targets a change can affect.

    fuzz_select.py --root windows --changed-files FILE [--all]

``FILE`` lists changed paths relative to the repository root, one per line
(``-`` reads standard input). Prints the selected target names, one per line;
prints nothing when the change affects no target. ``--all`` skips the
analysis and prints every target (pushes to master and manual runs).

A target is affected when it imports a crate that changed or a crate that
depends on one that changed (a change in ``tokenmon-provider-cursor`` reaches
the grokbot targets through the dependency). Every target is selected when a
change can reach all of them: any file of ``tokenmon-core``, the fuzz crate
(``windows/fuzz/``), the fuzz scripts, the workspace manifest and lock file,
the toolchain pin, or the workflows that run fuzzing.

Standard library only (Python 3.11+).
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

CORE_CRATE = "tokenmon-core"

# Paths (prefixes, or exact files) that can change every target's outcome.
EVERYTHING = (
    "windows/fuzz/",
    "windows/scripts/ci/fuzz_",
    "windows/Cargo.toml",
    "windows/Cargo.lock",
    "windows/rust-toolchain",
    "windows/.cargo/",
    ".github/workflows/windows-ci.yml",
    ".github/workflows/windows-fuzz-nightly.yml",
)


def all_targets(root: Path) -> list[str]:
    bins = root / "fuzz" / "src" / "bin"
    return sorted(p.stem for p in bins.glob("*.rs"))


def crate_dependencies(root: Path) -> dict[str, set[str]]:
    """Map each crate under ``crates/`` to the workspace crates it depends on."""
    crates_dir = root / "crates"
    names = {p.name for p in crates_dir.iterdir() if (p / "Cargo.toml").is_file()} if crates_dir.is_dir() else set()
    deps: dict[str, set[str]] = {}
    for name in names:
        manifest = tomllib.loads((crates_dir / name / "Cargo.toml").read_text(encoding="utf-8"))
        found: set[str] = set()
        for table in ("dependencies", "build-dependencies"):
            found |= {d for d in manifest.get(table, {}) if d in names}
        deps[name] = found
    return deps


def affected_crates(changed: set[str], deps: dict[str, set[str]]) -> set[str]:
    """The changed crates plus every crate that depends on one, transitively."""
    reached = set(changed)
    grew = True
    while grew:
        grew = False
        for crate, uses in deps.items():
            if crate not in reached and uses & reached:
                reached.add(crate)
                grew = True
    return reached


def changed_crates(paths: list[str], known: set[str]) -> set[str]:
    found: set[str] = set()
    for path in paths:
        parts = path.split("/")
        if len(parts) > 3 and parts[:2] == ["windows", "crates"] and parts[2] in known:
            found.add(parts[2])
    return found


def imports(source: str, crate: str) -> bool:
    ident = crate.replace("-", "_")
    return re.search(rf"\b{re.escape(ident)}\b", source) is not None


def select(root: Path, paths: list[str]) -> list[str]:
    targets = all_targets(root)
    paths = [p.strip() for p in paths if p.strip()]
    if any(p.startswith(EVERYTHING) for p in paths):
        return targets
    deps = crate_dependencies(root)
    changed = changed_crates(paths, set(deps))
    if CORE_CRATE in changed:
        return targets
    reached = affected_crates(changed, deps)
    if not reached:
        return []
    bins = root / "fuzz" / "src" / "bin"
    chosen = []
    for target in targets:
        source = (bins / f"{target}.rs").read_text(encoding="utf-8")
        if any(imports(source, crate) for crate in reached):
            chosen.append(target)
    return chosen


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", type=Path, default=Path("windows"), help="the windows/ directory")
    parser.add_argument("--changed-files", help="file with changed paths, or - for stdin")
    parser.add_argument("--all", action="store_true", help="select every target")
    args = parser.parse_args(argv)
    if args.all:
        chosen = all_targets(args.root)
    elif args.changed_files:
        text = sys.stdin.read() if args.changed_files == "-" else Path(args.changed_files).read_text(encoding="utf-8")
        chosen = select(args.root, text.splitlines())
    else:
        parser.error("give --changed-files or --all")
    if chosen:
        print("\n".join(chosen))
    return 0


if __name__ == "__main__":
    sys.exit(main())
