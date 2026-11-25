#!/usr/bin/env python3
"""Fail unless the Windows app version equals the macOS app version.

The macOS version is MARKETING_VERSION in project.yml. The Windows version is
[workspace.package] version in windows/Cargo.toml, which every crate and the
MSI inherit. The two apps ship the same version number, so a macOS bump needs
the same bump under windows/ (and the reverse).

    python3 -I windows/scripts/parity/check_versions.py [--root DIR]

Standard library only. Exit 0 when equal, 1 when not, 2 when a file cannot be read.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

MARKETING = re.compile(r'^\s*MARKETING_VERSION:\s*"([^"]+)"', re.MULTILINE)


class VersionError(Exception):
    """A version could not be read."""


def mac_version(root: Path) -> str:
    """MARKETING_VERSION from project.yml."""
    try:
        text = (root / "project.yml").read_text(encoding="utf-8")
    except OSError as e:
        raise VersionError(f"cannot read project.yml: {e}") from e
    match = MARKETING.search(text)
    if not match:
        raise VersionError("project.yml has no MARKETING_VERSION")
    return match.group(1)


def windows_version(root: Path) -> str:
    """[workspace.package] version from windows/Cargo.toml."""
    try:
        data = tomllib.loads((root / "windows" / "Cargo.toml").read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise VersionError(f"cannot read windows/Cargo.toml: {e}") from e
    version = data.get("workspace", {}).get("package", {}).get("version")
    if not isinstance(version, str):
        raise VersionError("windows/Cargo.toml has no [workspace.package] version")
    return version


def check(root: Path) -> str | None:
    """None when the versions match, else the message explaining the drift."""
    mac, win = mac_version(root), windows_version(root)
    if mac == win:
        return None
    return (
        f"Version drift: macOS is {mac} (MARKETING_VERSION in project.yml) but Windows is {win} "
        f"([workspace.package] version in windows/Cargo.toml). Set both to the same number "
        f"and refresh windows/Cargo.lock (`cargo update --workspace` in windows/)."
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    args = parser.parse_args(argv)
    try:
        problem = check(args.root)
    except VersionError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    if problem:
        print(f"error: {problem}", file=sys.stderr)
        return 1
    print(f"OK: macOS and Windows are both {mac_version(args.root)}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
