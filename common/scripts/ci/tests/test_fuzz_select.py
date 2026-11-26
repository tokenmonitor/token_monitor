"""Tests for fuzz_select.py (which cargo-fuzz targets a pull request runs).

    python3 -m unittest discover -s common/scripts/ci/tests -v
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import fuzz_select as fs  # noqa: E402

CRATES = {
    "tokenmon-core": [],
    "tokenmon-provider-cursor": ["tokenmon-core"],
    "tokenmon-provider-grokbot": ["tokenmon-core", "tokenmon-provider-cursor"],
    "tokenmon-provider-grok": ["tokenmon-core"],
    "tokenmon-app": ["tokenmon-core", "tokenmon-provider-grok"],
}

TARGETS = {
    "core_day_key": "use tokenmon_core::DayKey;",
    "cursor_summary": "use tokenmon_provider_cursor::summary;",
    "grokbot_usage": "use tokenmon_provider_grokbot::usage;",
    "grok_csv": "use tokenmon_provider_grok::csv;",
}


def build_tree(root: Path) -> None:
    for name, deps in CRATES.items():
        manifest = root / "common" / "crates" / name / "Cargo.toml"
        manifest.parent.mkdir(parents=True)
        lines = ['[package]', f'name = "{name}"', "", "[dependencies]"]
        lines += [f"{dep} = {{ workspace = true }}" for dep in deps]
        lines += ["", "[dev-dependencies]", "tokenmon-app = { workspace = true }"]
        manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    bins = root / "common" / "fuzz" / "src" / "bin"
    bins.mkdir(parents=True)
    for target, source in TARGETS.items():
        (bins / f"{target}.rs").write_text(source + "\n", encoding="utf-8")


class SelectTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        build_tree(self.root)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def pick(self, *paths: str) -> list[str]:
        return fs.select(self.root, list(paths))

    def test_a_leaf_crate_selects_only_its_targets(self) -> None:
        self.assertEqual(self.pick("common/crates/tokenmon-provider-grok/src/csv.rs"), ["grok_csv"])

    def test_dependents_of_a_changed_crate_are_selected(self) -> None:
        self.assertEqual(
            self.pick("common/crates/tokenmon-provider-cursor/src/lib.rs"), ["cursor_summary", "grokbot_usage"]
        )

    def test_core_selects_every_target(self) -> None:
        self.assertEqual(self.pick("common/crates/tokenmon-core/src/lib.rs"), sorted(TARGETS))

    def test_fuzz_scripts_and_workflows_select_every_target(self) -> None:
        for path in (
            "common/fuzz/src/bin/core_day_key.rs",
            "common/fuzz/corpus/core_day_key/seed",
            "common/scripts/ci/fuzz_run.sh",
            "Cargo.lock",
            "rust-toolchain.toml",
            ".github/workflows/windows-ci.yml",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.pick(path), sorted(TARGETS))

    def test_changes_no_target_imports_select_nothing(self) -> None:
        self.assertEqual(self.pick("common/crates/tokenmon-app/src/main.rs"), [])
        self.assertEqual(self.pick("windows/README.md", "windows/docs/spec/01.md"), [])

    def test_paths_outside_the_rust_workspace_are_ignored(self) -> None:
        self.assertEqual(self.pick("macos/TokenMon/App/AppModel.swift", ".github/workflows/ci.yml"), [])

    def test_a_windows_only_crate_selects_the_targets_that_import_it(self) -> None:
        # Windows-only crates live in windows/crates; the fuzz targets that
        # import them are still found.
        manifest = self.root / "windows" / "crates" / "tokenmon-platform-windows" / "Cargo.toml"
        manifest.parent.mkdir(parents=True)
        manifest.write_text('[package]\nname = "tokenmon-platform-windows"\n', encoding="utf-8")
        (self.root / "common" / "fuzz" / "src" / "bin" / "pe_version.rs").write_text(
            "use tokenmon_platform_windows::install::read_pe_version;\n", encoding="utf-8"
        )
        self.assertEqual(self.pick("windows/crates/tokenmon-platform-windows/src/lib.rs"), ["pe_version"])

    def test_dev_dependencies_do_not_widen_the_selection(self) -> None:
        # Every crate lists tokenmon-app as a dev-dependency; a change there
        # must not reach the targets of the crates that only test with it.
        self.assertEqual(self.pick("common/crates/tokenmon-app/src/lib.rs"), [])

    def test_a_crate_name_inside_another_name_does_not_match(self) -> None:
        (self.root / "common" / "fuzz" / "src" / "bin" / "core_day_key.rs").write_text(
            "use tokenmon_core_extra::x;\n", encoding="utf-8"
        )
        self.assertEqual(self.pick("common/crates/tokenmon-provider-cursor/src/lib.rs"),
                         ["cursor_summary", "grokbot_usage"])

    def test_main_prints_one_target_per_line_and_nothing_for_none(self) -> None:
        import contextlib
        import io

        changed = self.root / "changed.txt"
        changed.write_text("common/crates/tokenmon-provider-grok/src/csv.rs\n", encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(fs.main(["--root", str(self.root), "--changed-files", str(changed)]), 0)
        self.assertEqual(out.getvalue(), "grok_csv\n")
        changed.write_text("windows/README.md\n", encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            fs.main(["--root", str(self.root), "--changed-files", str(changed)])
        self.assertEqual(out.getvalue(), "")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            fs.main(["--root", str(self.root), "--all"])
        self.assertEqual(out.getvalue().split(), sorted(TARGETS))


if __name__ == "__main__":
    unittest.main()
