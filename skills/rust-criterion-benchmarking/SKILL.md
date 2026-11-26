---
name: rust-criterion-benchmarking
description: >
  Writing, running and reading Criterion benchmarks for Rust crates: bench
  layout, choosing what to bench, sizing inputs, fixtures, async and
  contention benches, named baselines, output capture, reading results, and
  gotchas. Use when adding a bench, measuring a change, or backing a
  performance claim with numbers. Load with rust-engineer; use
  rust-performance-reviewer to decide what is worth measuring and
  rust-concurrency-patterns when benching locks or waiters.
triggers:
  - benchmark
  - cargo bench
  - criterion
  - throughput
  - latency
  - performance measurement
  - baseline
  - regression
---

# Rust Criterion Benchmarking

Measure Rust code with statistical rigor using Criterion; let the numbers, not intuition, decide.

## When to use

- Adding or changing code on a hot path.
- Claiming a performance win or ruling out a regression in a commit,
  changelog or review.
- Comparing two designs (container, lock shape, encoding) before choosing.
- Reading a `cargo bench` run someone else produced.

## Setup

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] } # add "async_tokio" for async benches
tempfile = "3" # disk-backed fixtures

[[bench]]
name = "store"
harness = false # REQUIRED: disables the default test harness
```

Use whatever Criterion version the repo already pins; do not bump it in a
bench-only change.

## Layout

```text
benches/
  store.rs   # one file per subsystem, each a [[bench]] with harness = false
tests/       # behaviour tests live here, never in benches/
```

- Benches use the crate's public API like an external user; no
  `#[cfg(bench)]` hooks in `src/`.
- A gate with `--all-targets` builds benches, so they must be warning-free.
- Each bench file opens with a `//!` doc saying what it measures and how to
  run it.

## Choosing what to bench

Bench the operations whose cost the product pays per request or per item,
not whatever is easy to call.

1. **Start from the hot path.** List the operations callers hit most (a
   write, a read, a lookup, a parse, an encode) and bench each one alone.
2. **Sweep the dimension that grows.** For every operation, pick the input
   that scales in production (batch size, entries returned, entity count,
   payload size, number of recipients) and bench several points, for example
   1, 10, 100, 1000. A single point hides the curve.
3. **Add contention when state is shared.** Bench the same operation at 1,
   2, 4 and 8 concurrent workers so scaling (or its absence) is visible.
4. **Bench latency that users feel.** Wake-up latency, time to first result,
   end-to-end of one request through the in-process stack.
5. **Bench maintenance work.** Compaction, retention sweeps, index rebuilds,
   snapshot and restore: bounded work per pass is a claim worth measuring.
6. **Bench the representation you are changing**, before and after, when
   swapping a container, hasher, lock or encoding.
7. **Size inputs like production.** Realistic payload lengths (hundreds of
   bytes to a few KiB, not one-byte toys), realistic key shapes, realistic
   entity counts.

Do not bench cold paths (startup, config parsing, one-shot CLI commands)
unless startup time is itself the product.

## Skeleton

```rust
//! Store benchmarks: write, read and lookup.
//!
//! Run: cargo bench --bench store
//! Report: target/criterion/report/index.html

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};

fn bench_lookup(c: &mut Criterion) {
    let store = store_with_entries(10_000); // shared, read-only fixture
    c.bench_function("lookup", |b| b.iter(|| black_box(store.get(black_box(&KEY)))));
}

fn bench_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("read");
    for n in [1usize, 10, 100, 1_000] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || store_with_unread(n),          // setup: not timed
                |store| black_box(read_all(&store)),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_lookup, bench_read);
criterion_main!(benches);
```

`store_with_entries`, `store_with_unread`, `read_all` and `KEY` stand for
helpers in the bench file.

## Throughput vs latency

- Batch operations: set `group.throughput(Throughput::Elements(n))` (or
  `Throughput::Bytes`) so Criterion reports elements or bytes per second.
- Single operations: `bench_function` and the time per iteration suffice.

## Fixtures and isolation

Disk-backed fixtures use a temp directory that lives as long as the bench:

```rust
fn make_fixture() -> (Store, tempfile::TempDir) {
    let dir = tempfile::Builder::new().prefix("bench-").tempdir().expect("temp dir");
    let store = Store::open(dir.path()).expect("open store");
    (store, dir) // return the dir: dropping it deletes the directory
}

b.iter_batched(
    make_fixture,
    |(mut store, _dir)| black_box(store.write(42)),
    BatchSize::SmallInput,
);
```

Use `BatchSize::SmallInput` or `LargeInput` when each iteration needs fresh
state; build read-only fixtures once outside the loop.

## Async and contention benches

- Async: build one runtime outside the loop and use `b.to_async(&rt)`
  (`async_tokio` feature). Never build a runtime per iteration.
- Contention: `b.iter_custom(|iters| { ... })` spawning T workers that each
  run `iters / T` operations, returning the total elapsed `Duration`. Report
  T = 1, 2, 4, 8.
- Wake latency: time from the event to the waiter returning, measured inside
  `iter_custom`. A timeout or sleep inside the timed region makes the number
  meaningless.

## Running

```bash
cargo bench                                          # everything
cargo bench --bench store                            # one file
cargo bench --bench store -- read                    # one group (regex)
cargo bench --bench store -- "read/100"              # one case
cargo bench --bench store -- --save-baseline before  # record a baseline
cargo bench --bench store -- --baseline before       # compare against it
cargo bench 2>&1 | tee bench.txt                     # capture: tee, not >
```

Use the repo's bench target (for example `make bench`) if it has one.
Criterion writes an HTML report under `target/criterion/` even when stdout is
lost.

## Reading results

```text
read/100  time:   [12.31 µs 12.50 µs 12.67 µs]
          thrpt:  [7.89 Melem/s 8.00 Melem/s 8.12 Melem/s]
          change: [-3.2% -1.5% +0.1%] (p = 0.12 > 0.05)
          No change in performance detected.
```

- The three numbers are the low, point estimate and high of the confidence
  interval.
- `change` is the delta against the previous run or the named baseline.
- `p > 0.05`: noise, not a change.
- "Performance has regressed" compares against the last run, which may have
  been on an idle machine. It is a prompt to look, not a verdict.

**Golden rule: compare absolute medians across meaningful runs (named
baselines, same machine, same load), not the delta label.**

When a change claims a win, put the before and after medians in the commit
message or changelog line.

## End-to-end load

Criterion is for in-process micro-benchmarks. HTTP or RPC load (many
concurrent clients, p99 latency) belongs in a harness-free `[[bench]]`
binary with Tokio tasks and an HTTP client, or a script against a running
service. A micro-bench that improves 30% and does not move the service under
realistic load is a footnote.

## Gotchas

1. **Output capture:** `cargo bench > file.txt` swallows Criterion's output;
   use `2>&1 | tee`.
2. **Baseline drift:** two runs of the same code under different load
   "regress". Save named baselines and stop other heavy work (builds, test
   suites, other agents' cargo jobs) before measuring.
3. **`black_box`:** wrap results, and inputs the compiler could
   constant-fold, in `std::hint::black_box`. Newer Criterion deprecates its
   own.
4. **`harness = false`:** without it the file does not build as a bench.
5. **Setup in the timed loop:** building fixtures inside `iter` measures
   allocation, not the operation. Use `iter_batched`.
6. **State that grows across iterations:** a write bench that keeps
   appending to one log measures an ever-larger log. Reset per batch or
   bench a bounded steady state.
7. **Temp dir lifetime:** dropping the `TempDir` before the loop deletes the
   directory mid-bench.
8. **Build profile:** `cargo bench` uses the `bench` profile (release
   settings). Timings from `cargo test` mean nothing. Keep RUSTFLAGS the
   same as production.

## Anti-patterns

- A performance claim with no bench or load number behind it
- One input size where the cost depends on size
- Benching a toy payload when production payloads are KiB
- Fixtures built inside the timed closure
- Trusting the "regressed" label instead of comparing medians against a
  named baseline
- Benches that reach into private internals through `#[cfg(bench)]` hooks
- Bumping the Criterion version in a bench-only change
- Using Criterion for HTTP load testing

## Version history

- **2026-11-07**: Shared general skill. Merged the MentisDB version
  (2026-07-09: setup, temp-dir fixtures, HTML report, throughput vs latency,
  HTTP harness note) and the MentisChat version (2026-10-30: async and
  contention benches, named baselines, `std::hint::black_box`, setup and
  growing-state gotchas). The MentisChat mailbox bench table became general
  guidance on choosing benches; project bench names and Makefile targets
  dropped.
