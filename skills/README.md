# Skills

Agent skills for working in this repository. Each lives in `skills/<skill-id>/SKILL.md`.

| Skill | Use |
|---|---|
| [`rust-engineer`](rust-engineer/SKILL.md) | Writing, reviewing and fixing production Rust: the gate, types and enums, errors, docs, tests, review lens, lessons |
| [`rust-performance-reviewer`](rust-performance-reviewer/SKILL.md) | Rust performance review (Abseil Performance Hints): allocations, layout, containers, query plans, scheduled work |
| [`rust-concurrency-patterns`](rust-concurrency-patterns/SKILL.md) | Shared state, lock choice, per-entity locks, locks and `.await`, write batching, exactly-once delivery |
| [`rust-criterion-benchmarking`](rust-criterion-benchmarking/SKILL.md) | Writing, running and reading Criterion benchmarks |
| [`rust-fuzzing`](rust-fuzzing/SKILL.md) | cargo-fuzz targets that assert invariants, corpus and crash handling, regression tests |
| [`swift-engineer`](swift-engineer/SKILL.md) | Writing, reviewing and fixing native macOS Swift, SwiftUI and AppKit code |
| [`swift-performance-reviewer`](swift-performance-reviewer/SKILL.md) | Swift and macOS performance review: hot paths, Instruments, copy-on-write, main-actor hops, JSON and SQLite |

These are verbatim copies of the maintainer's shared skill set. They are refreshed by copying the folders over and are never edited here.

## Project skills

Written for this repository and edited here, unlike the shared copies above.

| Skill | Use |
|---|---|
| [`tokenmon-rust-desktop`](tokenmon-rust-desktop/SKILL.md) | The Windows port and the planned Linux port (Rust + Slint): the `make clippy` gate, where logic may live, test layout, 90% coverage on lines/regions/functions/branches, the macOS parity ledger, Windows traps, the `common/` extraction plan, how agents commit and hand off. Load with `rust-engineer`. |

Project-specific guidance lives next to the code: [`macos/docs/DEVELOPMENT.md`](../macos/docs/DEVELOPMENT.md) for the macOS app and [`windows/docs/`](../windows/docs/) for the Windows port.
