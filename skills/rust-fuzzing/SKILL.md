---
name: rust-fuzzing
description: >
  Coverage-guided fuzzing for Rust crates with cargo-fuzz (libFuzzer) and
  arbitrary: choosing targets, writing harnesses that assert real invariants
  (round trip, differential, model-based), corpus and crash handling,
  turning every finding into a stable regression test, Makefile targets and
  CI. Use whenever code parses bytes it did not produce, or exposes a
  stateful API that outside callers drive in arbitrary order. Load with
  rust-engineer; findings that involve locks or waiters also need
  rust-concurrency-patterns.
triggers:
  - fuzz
  - fuzzing
  - cargo fuzz
  - libfuzzer
  - arbitrary
  - fuzz target
  - corpus
  - crash input
  - parser hardening
  - untrusted input
---

# Rust Fuzzing

Fuzz every trust boundary in a Rust crate with cargo-fuzz, and turn every finding into an ordinary regression test.

Tests prove the cases you thought of. Fuzzing finds the ones you did not:
the panic on a 0-byte body, the `usize` overflow on a hostile length prefix,
the state machine that loses an item after `join, leave, join`.

## When to use

- Adding or changing a parser, decoder, validator or loader of external
  input: socket bodies, frames, files on disk, imports, snapshots, CLI
  arguments, env vars, config, query strings, tokens.
- Adding an operation to a stateful API (a store, queue, bus, session
  machine) that callers can drive in any order.
- Reviewing a crate that has trust boundaries but no `fuzz/` directory.
- Triaging a crash from a fuzz run.

## Non-negotiables

1. **Fuzz every trust boundary.** A function that takes bytes, strings or
   JSON from a socket, a file, a CLI argument, an environment variable or
   another process gets a target: name and id validators, wire and body
   decoders, snapshot and file loaders, query and cursor parsers, frame
   decoders, config parsers.
2. **Assert an invariant, not just "no panic".** "No panic" is the floor.
   Each target also checks at least one property: round trip, a size or
   resource bound, agreement with a reference model, or a typed error
   instead of a crash (see *Invariants*).
3. **Every crash becomes a regression test.** Minimize the input
   (`cargo fuzz tmin`) and paste it as a byte literal into a named test that
   runs on stable. Prove the test fails before the fix and passes after it.
   Add the input to the corpus, and a changelog line if the repo keeps one.
   The fuzz job finds bugs; `cargo test` keeps them fixed.
4. **The harness goes through the public API.** The fuzz crate is a separate
   package and sees only `pub` items, the same as `tests/`. If a parser
   must be fuzzed directly, make it `pub` and document it. Never add
   `#[cfg(test)]` or fuzz-only escape hatches.
5. **Fuzzing never touches the stable gates.** The fuzz crate needs nightly
   and lives outside the main package and workspace. `cargo check
   --all-targets`, clippy, the MSRV check and `cargo publish` must not see
   it.

## Layout

```text
fuzz/
  Cargo.toml              # package <crate>-fuzz, publish = false, own [workspace]
  fuzz_targets/
    names.rs              # one file per target, named for what it fuzzes
    snapshot_restore.rs
    store_ops.rs
  corpus/<target>/        # committed seeds plus cmin-minimized corpus
  artifacts/<target>/     # crashes (gitignored; copy them into tests)
  dicts/<target>.dict     # optional libFuzzer dictionary (JSON keys, tokens)
```

`fuzz/Cargo.toml`:

```toml
[package]
name = "mycrate-fuzz"
version = "0.0.0"
publish = false
edition = "2021"

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
arbitrary = { version = "1", features = ["derive"] }
mycrate = { path = ".." }

# Its own workspace: keeps the fuzz crate out of the parent package's and
# workspace's builds, clippy runs, MSRV checks and published tarball.
[workspace]
members = ["."]

[[bin]]
name = "names"
path = "fuzz_targets/names.rs"
test = false
doc = false
bench = false
```

- In a Cargo workspace, either give each crate that needs one its own
  `fuzz/` with its own `[workspace]` table, or put one `fuzz/` at the
  workspace root and add `exclude = ["fuzz"]` to the root `[workspace]`.
- Add `fuzz/target`, `fuzz/artifacts` and `fuzz/coverage` to `.gitignore`.
- Add `fuzz/` to `exclude` in a published crate's `[package]` so the corpus
  does not ship.

## Writing a target

Raw bytes, for parsers:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use mycrate::names::AgentName;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    if let Ok(name) = s.parse::<AgentName>() {
        // Round trip: what we accept, we print and accept again unchanged.
        let again: AgentName = name.to_string().parse().expect("round trip");
        assert_eq!(name, again);
        assert!(name.as_str().len() <= mycrate::names::MAX_NAME_LEN);
    }
});
```

Structured input with `arbitrary`, for APIs and state machines. This is
model-based fuzzing: compare the real thing with a reference model simple
enough to be obviously correct.

```rust
#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Op {
    Open(u8),              // small ids: many collisions, more interesting states
    Put { id: u8, value: String },
    Take { id: u8, limit: u8 },
    Close(u8),
}

fuzz_target!(|ops: Vec<Op>| {
    let real = mycrate::Store::in_memory();
    let mut model = Model::default(); // a HashMap and a VecDeque, about 50 lines
    for op in ops.iter().take(256) {  // bound the work per input
        let got = apply_real(&real, op);
        let want = model.apply(op);
        assert_eq!(got, want, "diverged at {op:?}");
    }
    real.check_invariants(); // counters equal what the structures hold, etc.
});
```

Rules for harnesses:

- **Deterministic.** No wall clock, no randomness, no racing threads, no
  network, no sleeps. Inject a clock if the code reads time. A crash that
  does not reproduce is worth nothing.
- **No state shared between iterations.** Build fresh state per input. If
  setup is expensive (a Tokio runtime), build it once in a `OnceLock` and
  keep nothing else that carries over.
- **Fast.** Aim for thousands of executions per second. Cap loop counts,
  collection sizes and `-max_len`. A slow target finds nothing.
- **Async code.** Fuzz the synchronous core where possible. Otherwise use a
  `current_thread` runtime from a `OnceLock` and `block_on`. A multi-thread
  runtime makes runs non-reproducible.
- **Disk.** Load from bytes (`from_slice`, or `from_reader` on a `&[u8]`)
  rather than files. If the API only takes a path, write into one temp
  directory created once and overwritten each iteration.
- **Avoid `cfg(fuzzing)`.** cargo-fuzz sets `--cfg fuzzing`. Use it only to
  skip checks the fuzzer cannot satisfy, such as signatures and checksums,
  and say why in a comment. Any other behaviour change means you are fuzzing
  a different program.

## Invariants worth asserting

| Kind | Example |
|------|---------|
| Round trip | `parse(display(x)) == x`; `deserialize(serialize(v)) == v` |
| Typed rejection | Bad input yields `Err(SpecificError)`, never a panic or `Ok` with garbage |
| Bounds | Accepted names within max length; a page within its byte cap; memory charged within budget |
| Differential | New parser agrees with the old one or with `serde_json`; fast path agrees with slow path |
| Model-based | Operation sequences give the same observable results as a naive reference model |
| Conservation | Items written = delivered + pending + evicted; counters equal the contents of the structures |
| Exactly once | No item is delivered twice; no acknowledgement covers an id that was never written |
| Idempotence | `normalize(normalize(x)) == normalize(x)`; restoring a snapshot twice equals restoring it once |
| Integrity | A decoded file either passes its hash or checksum checks or yields a typed error |

## Running

```bash
cargo install cargo-fuzz                      # once
rustup toolchain install nightly --profile minimal

cargo +nightly fuzz list
cargo +nightly fuzz build                     # all targets compile (CI does this)
cargo +nightly fuzz run names -- -max_total_time=60
cargo +nightly fuzz run names fuzz/corpus/names -- -runs=0   # replay corpus only
cargo +nightly fuzz run store_ops -- -max_len=4096 -rss_limit_mb=2048
cargo +nightly fuzz run names -j8 -- -max_total_time=600     # parallel jobs
cargo +nightly fuzz run json_body -- -dict=fuzz/dicts/json_body.dict
```

- Builds are optimized with debug assertions and overflow checks **on**. An
  overflow panic in a fuzz run is a real finding even if release builds wrap
  silently.
- The default sanitizer is AddressSanitizer. For crates with no `unsafe`,
  `--sanitizer none` runs about twice as fast and loses nothing. Keep ASan
  where `unsafe` or FFI is involved.
- Seed each corpus with real inputs: the payloads the tests already use,
  captured wire bodies, a valid file or snapshot. Coverage grows much faster
  from realistic seeds than from nothing.
- Measure what the fuzzer reaches with `cargo +nightly fuzz coverage
  <target>`. If a branch you care about is never hit, the target or the
  seeds are wrong.

## Handling a finding

1. Reproduce: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-...`.
2. Minimize: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-...`.
3. Print it. `cargo +nightly fuzz fmt <target> <file>` shows the
   `Arbitrary` value for structured targets.
4. Write the regression test on stable, in the repo's test layout, named for
   the behaviour (`a_name_of_only_combining_marks_is_rejected`, not
   `fuzz_crash_3`), with the input as a literal.
5. Fix the root cause everywhere the pattern occurs, not just for this input.
6. Add the minimized input to `fuzz/corpus/<target>/`, add a changelog line
   if the repo keeps one, and commit the test, fix and corpus file together.
7. After a long run, shrink the corpus before committing:
   `cargo +nightly fuzz cmin <target>`. Keep the committed corpus small (a
   few MB per repo).

## Makefile and CI

```make
FUZZ_SECONDS?=60
fuzz-build: ## Build every fuzz target (needs nightly and cargo-fuzz)
	cargo +nightly fuzz build
fuzz-replay: ## Replay each committed corpus once (fast regression pass)
	for t in $$(cargo +nightly fuzz list); do cargo +nightly fuzz run $$t fuzz/corpus/$$t -- -runs=0 || exit 1; done
fuzz: ## Fuzz TARGET for FUZZ_SECONDS (default 60)
	cargo +nightly fuzz run $(TARGET) -- -max_total_time=$(FUZZ_SECONDS)
```

- Where CI runs, a nightly job on every push and pull request runs
  `fuzz-build` and `fuzz-replay`. It takes seconds and catches harness rot
  and regressions on old findings. Where CI is off, run both locally before
  merging a change to a fuzzed surface.
- Time-boxed fuzzing (for example 5 minutes per target) runs as a separate
  scheduled or manual job, not on every push. Keep the artifacts when it
  fails.
- Never wire fuzzing into the repo's stable gate (`make check`, `make ci`,
  `make test`) or the MSRV check.

## Fuzzing, proptest and tests

- **Tests** pin behaviour you know about. They run on stable, every time.
- **proptest** (stable) states properties over generated inputs and shrinks
  failures. Use it for algebraic properties of pure functions where coverage
  guidance adds little.
- **cargo-fuzz** is coverage-guided and runs for minutes to hours. It finds
  inputs that reach deep branches. Use it for parsers, decoders and
  operation sequences.

Do not write the same property in both proptest and a fuzz target. Pick the
one that fits.

## Review lens

1. Does every new parser, decoder or loader of external input have a target,
   or a written reason why not?
2. Does the target assert something beyond "no panic"?
3. Is the harness deterministic, bounded and fast?
4. Is each past finding pinned by a stable regression test?
5. Do the stable gates (check, clippy, MSRV, publish) still ignore `fuzz/`?

## Anti-patterns

- Fuzz targets that only call the function and assert nothing
- A crash fixed without a regression test, so only the corpus remembers it
- Harnesses that read the clock, spawn threads or hit the network
  (non-reproducible)
- Fuzz-only `pub` shims or `cfg(fuzzing)` branches that change behaviour
- Committing `fuzz/artifacts/` or an unminimized multi-hundred-MB corpus
- Putting the fuzz crate in the main workspace, which breaks the stable gate
  and MSRV
- Unbounded `Vec<Op>` loops, where one input runs for minutes and the fuzzer
  stalls
- Running long fuzz campaigns on every push, which slows CI without finding
  more

## Version history

- **2026-11-07**: Shared general skill from the identical MentisChat and
  MentisDB `rust-fuzzing` (2026-10-30). Made project-neutral: project lists
  and workspace paths removed, example targets renamed, changelog and CI
  steps made conditional on the repo having them, integrity invariant
  added.
