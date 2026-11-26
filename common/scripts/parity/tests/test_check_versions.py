"""Tests for check_versions.py. Run: python3 -m unittest discover -s common/scripts/parity/tests"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_versions as cv  # noqa: E402

REPO = Path(__file__).resolve().parents[4]


def write_repo(root: Path, mac: str | None, win: str | None) -> None:
    (root / "macos").mkdir(parents=True)
    if mac is not None:
        (root / "macos" / "project.yml").write_text(
            f'targets:\n  TokenMon:\n    settings:\n      base:\n        MARKETING_VERSION: "{mac}"\n',
            encoding="utf-8",
        )
    if win is not None:
        (root / "Cargo.toml").write_text(
            f'[workspace]\nmembers = []\n\n[workspace.package]\nversion = "{win}"\n',
            encoding="utf-8",
        )


class CheckVersionsTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_equal_versions_pass(self) -> None:
        write_repo(self.root, "1.13.2", "1.13.2")
        self.assertIsNone(cv.check(self.root))
        self.assertEqual(cv.main(["--root", str(self.root)]), 0)

    def test_drift_is_reported_with_both_numbers_and_the_fix(self) -> None:
        write_repo(self.root, "1.13.2", "0.1.0")
        message = cv.check(self.root) or ""
        self.assertIn("1.13.2", message)
        self.assertIn("0.1.0", message)
        self.assertIn("Cargo.toml", message)
        self.assertEqual(cv.main(["--root", str(self.root)]), 1)

    def test_missing_files_and_keys_are_errors_not_passes(self) -> None:
        write_repo(self.root, None, "1.0.0")
        with self.assertRaises(cv.VersionError):
            cv.mac_version(self.root)
        self.assertEqual(cv.main(["--root", str(self.root)]), 2)
        other = self.root / "other"
        write_repo(other, "1.0.0", None)
        with self.assertRaises(cv.VersionError):
            cv.windows_version(other)

    def test_this_repository_ships_one_version(self) -> None:
        """The real check: macos/project.yml and the root Cargo.toml agree."""
        self.assertIsNone(cv.check(REPO), cv.check(REPO))


if __name__ == "__main__":
    unittest.main()
