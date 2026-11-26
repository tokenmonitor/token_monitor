---
name: tokenmon-rust-desktop
description: >
  Project skill for TokenMon's Rust desktop ports: the Windows tray app in
  windows/ (Rust + Slint) and the planned Linux app sharing a common/ backend.
  Covers the gate (make clippy), where logic may live, test layout, the
  four-dimension coverage method, the macOS parity ledger and drift guard,
  Windows-only traps, the Linux extraction plan, and how agents commit and
  hand off. Load together with rust-engineer (house Rust standards) and, as
  needed, rust-fuzzing, rust-concurrency-patterns and rust-performance-reviewer.
triggers:
  - windows port
  - linux port
  - tokenmon rust
  - windows/crates
  - common crates
  - parity ledger
  - coverage gap
  - make clippy
---

# TokenMon Rust desktop ports

The macOS app (Swift, `macos/`) is the source of truth for behavior. The
Windows port (`windows/`, Rust + Slint) mirrors it test for test, and the
Linux port will share its backend through a `common/` folder (issue #71).
This skill is the project layer on top of `rust-engineer`; where they
disagree, this one wins for this repo.

## The gate (every push, no exceptions)

From `windows/`:

1. `make clippy`: `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`
   for the workspace **and** the separate `fuzz/` workspace (its harnesses are
   test code). Must exit 0.
2. `cargo test --workspace` (scope with `-p` while iterating; the full
   workspace before pushing).
3. From the repo root: `python3 -I windows/scripts/parity/check_parity.py`
   must print `Result: PASS`.

Rules about the gate:

- Run it on the commit you push, after the rebase. Re-running only the
  affected crate after a rebase is how a broken commit reached master once.
- Clippy on macOS/Linux never compiles `cfg(windows)` code. Windows-only
  warnings (an import used only by a `#[cfg(unix)]` test, say) surface only on
  the Windows CI runner. Scope platform-only imports inside the gated item.
- `cargo check --target x86_64-pc-windows-msvc` cannot build from macOS
  (aws-lc-sys needs the MSVC C toolchain). Windows-only code is verified by the
  `Tests (windows-latest)` and `Release build` CI jobs, so push and watch them.

## Where logic lives

- **Decisions in portable, pure, tested modules; OS calls in thin glue.**
  Placement math, theme mapping, message decoding, state machines, parsers:
  all portable. The glue that only calls Win32/WinRT/registry stays thin.
- The only code coverage may skip is listed in
  `windows/.config/coverage-excludes.txt`: whole files that are entirely
  `cfg(windows)`, each with a reason and a line budget the lint job enforces.
  Do not add entries to reach a number. Move logic out of glue instead.
- Strong types over strings: provider ids, limits, windows, sources are enums.
- Secrets: never written to a plaintext file, never logged, never embedded in
  error strings (summarize response bodies). DPAPI/Credential Manager on
  Windows; Secret Service on Linux. A refused secure store fails the sign-in.

## Tests

- Only in each crate's `tests/` folder, one `tests/test_<topic>.rs` per
  behavior (follow a crate's existing file naming if it differs). Never
  `#[cfg(test)]` in `src/`. To test a private helper, make it `pub`.
- Every test has a `///` comment saying the behavior it checks; descriptive
  names; `expect("why")` over bare `unwrap()`.
- Hermetic: injected `Clock`, tempdirs, `wiremock`, crafted SQLite files, no
  real network, home directory, registry or keychain. Pin junk credentials in
  env vars so a dev shell's real keys can never be used.
- Tests that drive the real binary use `env!("CARGO_BIN_EXE_TokenMon")` and
  forward `LLVM_PROFILE_FILE` to the child, or its coverage is lost.
- Tests that install a global tracing subscriber or mutate process env get
  their own test binary (file), or they race their neighbours.
- `cfg(windows)` tests live in files gated `#![cfg(windows)]` and run only on
  the Windows runner; clean up every registry/credential/file artifact.
- Slint UI: test view models and pure helpers; snapshot goldens change only
  with intended visual changes. Slint cannot measure text from Rust, so chip
  widths are estimated (documented divergence DK4).

## Coverage (target: 90% on all four dimensions, total and per crate)

Lines, regions, functions **and branches**, per crate, on both CI runners.
Branches need nightly. Locally, from `windows/`:

```sh
export CARGO_LLVM_COV_TARGET_DIR=$PWD/target/cov-nightly CARGO_INCREMENTAL=0
ignore=$(python3 -I scripts/ci/coverage_excludes.py regex | tr -d '\r')
cargo +nightly llvm-cov --workspace --branch --no-report
cargo +nightly llvm-cov report --branch --ignore-filename-regex "$ignore" --json --output-path cov.json
```

(`cargo llvm-cov` takes its target dir from `CARGO_LLVM_COV_TARGET_DIR`, not a
flag.) CI's coverage job runs a pinned nightly (`RUSTUP_TOOLCHAIN`) with
`--branch`, and `render_coverage.py` reports and gates every metric.

Honesty rules:

- Raise numbers with tests that assert behavior. No exclusions, no lowered
  floors, no tests that merely execute code.
- Deleting code is fine only when it is genuinely unreachable (a defensive
  arm for a constant shape) and behavior is provably identical; say so in the
  commit.
- llvm-cov counts every generic instantiation separately, so a generic helper
  called with different types shows "missed" branches that other calls take.
  Type-erase the core (`&mut dyn Trait`, a non-generic inner fn) instead of
  writing duplicate tests.
- Some branches are unreachable through the public API (an
  `Option == Option` artifact, a guard an event always clears). Leave them and
  list them in the report; do not contort the code.
- Windows-only code is measured only on the Windows runner; macOS numbers for
  `tokenmon-platform` and `tokenmon-app` overstate it. Read the
  `windows-coverage-summary-windows-latest` artifact before claiming a crate.
- Floors (`windows/.config/coverage-floors.toml`) only go up: 1 to 2 points
  under the lower runner, committed with the tests that earned them.

## Parity with macOS

- `windows/docs/PARITY.md` has exactly one row per Swift test:
  `| <swift test id> | behavior | ws-<stream> | ported/todo/n/a | rust test path | notes |`.
  Six cells. A row with an extra cell (`| todo | ported | path |`) still
  parses as `todo`. Check flips with `check_parity.py`, not by eye.
- When upstream merges, run `check_parity.py --print-missing-rows` for new
  Swift tests and delete rows whose Swift test was removed or renamed. A
  renamed Swift test usually means the behavior changed: update the Rust test
  the old row pointed at.
- The drift guard (`windows-parity.yml`) makes any macOS change carry Windows
  changes, PARITY.md rows, a `Windows-Parity: <reason>` trailer, or a
  reference to an open windows-parity issue.
- Behavior that cannot match (no native tooltips in Slint, SF Symbols vs
  Unicode glyphs) goes in PARITY.md's Divergences section with a reason.

## Windows traps already paid for

- The panel must re-anchor after it first shows and on every
  `Resized`/`ScaleFactorChanged`: before the native window exists, its size
  and position are not final (it once opened half under the taskbar).
- Defer placement out of winit callbacks with a zero-delay `slint::Timer`.
- Static CRT (`.cargo/config.toml`); CI fails if `TokenMon.exe` imports the
  VC++ runtime. Never set `RUSTFLAGS` for release builds.
- Update trust is SHA-256 from the release until code signing exists.

## Linux port (plan: issue #71)

- One Cargo workspace at the repo root: shared crates in `common/crates`
  (core, the providers, ui view models, platform traits and fakes, the app
  as a library), `windows/crates` (`tokenmon-platform-windows`,
  `tokenmon-windows` building `TokenMon.exe`) and the same pair under
  `linux/crates`.
- Phase 0 (extracting `common/`) is a pure move plus de-Windowsing core
  (update tag/asset names become data, app paths become OS-aware). No
  behavior change: tests, coverage, parity and CI must stay identical. Run it
  only when no other agent has work in flight in `windows/crates`, as small
  commits (`git mv` first, then fixes).
- Linux platform implementations sit behind the same traits as Windows and
  need the same hermetic tests and coverage bar.
- Known hard parts: Wayland cannot place the panel next to the tray;
  WebKitGTK sign-in needs its own process (GTK loop vs Slint/winit); GNOME
  shows no tray without the AppIndicator extension.

## Working as an agent here

- Commit straight to `master` in small logical commits and push each as soon
  as its gate passes: `git fetch origin && git rebase origin/master && git
  push origin HEAD:master`. Retry the rebase on rejection. Never force-push.
- Conventional subjects (`test(windows/grok): ...`, `fix(windows/app): ...`);
  the body says why. A bug a test finds gets its own `fix` commit with the
  regression test.
- Commit messages end with the trailer lines the session asks for (currently
  `Co-Authored-By: ...` then `Model: <provider/model-id>`).
- Write a MentisDB checkpoint (chain `token_monitor`, Summary/Checkpoint)
  after each push: SHA, numbers, what is left, bugs. Long tasks end with a
  Handoff note.
- Parallel agents own disjoint crates or files. CI cancels the coverage,
  release and fuzz jobs of older runs on each push, so only the newest run on
  master completes them; read results from that one.
- Verify a sub-agent's claims on the tip yourself (gate, ledger, numbers)
  before reporting them as done.
