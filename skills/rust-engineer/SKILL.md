---
name: rust-engineer
description: >
  House standards for writing, reviewing and fixing production Rust in any
  repository: the gate, types and enums, error handling, design bias, docs,
  tests, the review lens and hard-earned lessons. Use when implementing a
  feature, fixing a bug, adding or changing a public API, refactoring, or
  reviewing Rust changes. Also load rust-concurrency-patterns for shared
  state and locks, rust-criterion-benchmarking for benches and performance
  claims, rust-fuzzing for parsers and untrusted input, and
  rust-performance-reviewer for hot-path audits.
triggers:
  - rust engineer
  - rust coding style
  - implement rust
  - fix rust bug
  - refactor rust
  - clippy
  - public API
  - write tests
  - review rust
  - enum design
  - error handling
---

# Rust Engineer

Standards for writing and reviewing production Rust in any repository; the repo's own rules win where they are stricter.

## Companion skills

| Skill | Load when |
|-------|-----------|
| `rust-concurrency-patterns` | Shared mutable state, locks, lock order, guards and `.await`, `Notify` wakeups, write buffering, WAL |
| `rust-criterion-benchmarking` | Writing or reading `cargo bench` runs, baselines, throughput vs latency, backing a performance claim |
| `rust-fuzzing` | Parsers, decoders, file loaders, wire bodies, state machines driven by outside callers |
| `rust-performance-reviewer` | Hot-path audits, allocation and cache footprint, flat profiles, API design for speed |

Do not re-derive concurrency, benchmarking, fuzzing or performance
conventions from memory. Open the companion skill and follow it.

## Non-negotiables

1. **Run the repo's full gate; warnings are errors.** Use the gate the repo
   defines (`make check`, `make ci`, `make clippy`, or whatever its
   AGENTS.md names), not a subset you picked. If the repo has no gate, run:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
   cargo test --workspace --all-features --locked
   RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace
   ```
   Treat every Clippy warning as a hard error. Do not paper over one with
   `#[allow]` unless the reason is written next to it and unavoidable. If the
   repo builds in release mode only, or checks MSRV, comment style or a
   lockfile, those are part of the gate too.

2. **Follow the repo's test layout.** Where the repo has none, put tests
   under `tests/` as integration tests that use the public API, and keep
   `#[cfg(test)] mod tests` out of `src/` except for small pure tests of
   private helpers that cannot be reached through the public surface. Where
   the repo already keeps unit tests next to the code, follow it.

3. **Every bug fix needs a regression test.** Reproduce the failure first,
   then fix, then prove the test fails without the fix and passes with it.

4. **The public surface is a manual.** Every exported type, function, trait,
   enum variant and struct field gets rustdoc. Prefer short prose plus a
   runnable `# Examples` block over long narrative. Document errors,
   invariants, and when *not* to use the API, with `# Errors`, `# Panics` and
   `# Safety` sections where they apply.

5. **Parameters and types are correct by construction.** Prefer enums and
   newtypes over free-form strings. Validate at construction time. Fail early
   with specific error enums, not opaque `String` errors.

6. **Follow the repo's changelog convention if it has one.** If the repo
   keeps a changelog, add an entry under its topmost unreleased section, in
   its existing style, written so a reader understands the change without the
   diff. Do not invent a release header unless you are cutting a release. If
   the convention orders entries by impact, re-sort after adding yours
   (security and data loss first, docs and chores last). If the repo has no
   changelog, do not create one.

7. **Pin contract-bearing dependencies exactly.** A dependency whose wire
   format, schema, hash or protocol is part of your contract (`=x.y.z`, a git
   tag, a vendored release) moves only in a deliberate change that says what
   changed and how compatibility was checked. Every other dependency must
   earn its compile time; do not add one casually.

## Core philosophy

**Immutability is the default. Mutability is the last resort.**

- Prefer immutable data and iterator pipelines (`map`, `filter`, `fold`)
  over mutable state.
- Use `let`, not `let mut`, unless you can justify the mutation.
- When mutation is unavoidable, keep it in the smallest scope and say why.
- Prefer returning new values over modifying existing ones.

**Strongly typed domains remove whole classes of bugs.**

- Never use raw `String` constants as parameters, control values or
  discriminators.
- Model each domain concept with an enum or newtype that encodes the valid
  states at compile time.
- Make those types easy to use at the boundary with `FromStr`, `Display` and
  serde.

**Simplicity and composability over cleverness.**

- Design types first (structs, enums, errors), then logic, then tests.
- Short functions that do one thing (typically under 30 lines), composed
  rather than written as monoliths.
- Explicit over implicit: no trait magic or macros unless they genuinely
  remove code.
- If a function needs a comment to explain *what* it does, split it into
  well-named parts.

## Domain modelling (enum design)

1. Derive at least `Debug, Clone, PartialEq, Eq`, plus `Serialize,
   Deserialize` when the type crosses a boundary.
2. Add `Copy` when no variant carries heap data.
3. Implement `Display` for human-readable output and `FromStr` for parsing,
   each with a dedicated error type, never `String`.
4. Use `#[serde(rename_all = "snake_case")]` (or the repo's existing casing)
   so the wire form is consistent and independent of Rust names.
5. `strum::EnumString` / `strum::Display` are fine for boilerplate if the
   repo already uses `strum`.
6. Test every enum's `Display` to `FromStr` round trip and its rejection of
   invalid input.
7. Newtypes for identifiers and bounded values (`AgentName`, `ChainKey`,
   `Score`) validate in their constructor, so every value in the program is
   already valid.

```rust
impl std::str::FromStr for Mode {
    type Err = ParseModeError; // a dedicated type, never String

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(Self::Plain),
            "tui" => Ok(Self::Tui),
            other => Err(ParseModeError(other.to_owned())),
        }
    }
}
```

## Error handling

- `thiserror` (or hand-rolled enums, if that is the repo's pattern) for
  library and domain errors; `anyhow` only in binaries, tools and tests.
- One specific error enum per module or concern; never `Box<dyn Error>` in
  library code.
- Reserve `unwrap()` and `expect()` for provably infallible cases, and give
  every `expect()` a message saying why it cannot fail. No `unwrap()` on
  request paths, and none inside a lock section (`parking_lot` locks do not
  poison, so a panic there leaves state half-updated).
- Propagate with `?`; avoid match chains that only re-wrap an error.
- Error variants carry enough context to say *which* input failed and why,
  without embedding secrets or raw untrusted bodies.
- At a protocol boundary, keep the protocol's split between "the request was
  malformed" and "the operation failed": domain failures go back as the
  protocol's failure result, not as a transport error.

## Design bias

- **One mechanism, no speculative fallbacks.** Ship exactly one way to do
  something; a fallback is a second mechanism to test, document and debug.
- **Keep it simple.** Small functions, obvious control flow, no clever
  macros unless they remove real duplication.
- **Co-locate what is used together.** Related types, validation and
  persistence live in one coherent module, not scattered helpers.
- **Be intentional with memory.** Avoid needless clones and large temporary
  allocations on hot paths; borrow and stream where it stays readable.
  Correctness and durability beat micro-optimizations; measure first.
- **Integrity first for persistent data.** Serialization and persistence
  changes keep every explicit integrity check (hashes, checksums, frozen
  fixtures, version pins) intact.
- **Respect dependency direction.** If the repo says crate A must not depend
  on crate B, that is a hard rule, not a style preference.
- **Lock granularity over cleverness.** Every shared lock is a serialization
  point. Prefer per-entity locks over one global lock.
- **Comment only what is not obvious.** Prefer better names and types over
  narrating code; comment *why* for security, concurrency, compatibility or
  surprising invariants.

## Concurrency quick rules

Summary only; open `rust-concurrency-patterns` for patterns and code.

1. No `std::sync` or `parking_lot` guard alive at an `.await`. Copy what you
   need (usually `Arc`s) out, drop the guard, then await. Use
   `tokio::sync` locks only when a guard truly must cross an `.await`.
2. Maps of entities: `DashMap<K, Arc<Entity>>` or a briefly held outer lock
   that only hands out `Arc`s. A `DashMap` `Ref` is a lock; clone the `Arc`
   out before locking the value or awaiting.
3. More than one lock: one documented order, on every path.
4. Read-then-update steps that must be atomic happen in one critical section
   with no `.await` inside.
5. Waiting on `Notify`: enable the `Notified` before checking the condition,
   then loop and re-check after every wake.
6. Buffered writers flush on `Drop`; batching is an operator knob, never a
   silent default that loses data.

## Benchmarking quick rules

Summary only; open `rust-criterion-benchmarking` for setup and gotchas.

1. Benches live under `benches/` with `harness = false`; tests stay in
   `tests/`.
2. `std::hint::black_box` results; `Throughput::Elements` for batches;
   setup in `iter_batched`, not in the timed closure.
3. Capture with `cargo bench 2>&1 | tee ...`; a bare `>` swallows output.
4. Compare absolute medians against a named baseline; Criterion's
   "regressed" label drifts with machine load.
5. End-to-end HTTP load is a harness-free bench binary or a script against a
   running service, not a Criterion micro-bench.

## Fuzzing quick rules

Summary only; open `rust-fuzzing` for layout, harness rules and CI.

1. Every trust boundary has a target under `fuzz/fuzz_targets/`: name and id
   validation, request and frame decoding, file and snapshot loading, query
   parsing, and stateful APIs as sequences of operations checked against a
   reference model.
2. Each target asserts an invariant (round trip, bounds, model agreement,
   typed rejection), not just "no panic".
3. Every crash is minimized into a named stable regression test, and the
   input goes into `fuzz/corpus/<target>/`.
4. `fuzz/` is its own nightly-only package with its own `[workspace]`; the
   stable gate, MSRV check and `cargo publish` never build it.

## Performance quick rules

Summary only; open `rust-performance-reviewer` for the lenses.

1. Classify first: is the code per request or per item? If not, it is not a
   hot path; do not micro-optimize it.
2. Structural fixes (algorithm, query plan, full scans, N+1 calls, lock per
   item) before allocation polish.
3. Every Important or Critical finding cites `file:line` and is backed by a
   bench, profile or an explicit estimate.
4. A performance claim in a commit or changelog carries before and after
   numbers.

## Documentation style

```rust
//! Module overview: what this is, when to use it, its storage or state model.

/// One-line summary of the type or function.
///
/// Longer explanation of policy, invariants and failure modes.
///
/// # Examples
///
/// ```
/// use mycrate::names::AgentName;
///
/// let name: AgentName = "grok".parse()?;
/// assert_eq!(name.as_str(), "grok");
/// # Ok::<(), mycrate::names::ParseNameError>(())
/// ```
pub struct Example {
    /// Human-readable field documentation.
    pub alias: String,
}
```

- Document every public field.
- Document error variants with their operator-facing meaning.
- Keep examples compiling under `cargo test --doc`.
- Comments and rustdoc state what the code does now, in the present tense.
  Never narrate history ("used to", "previously", "no longer", "changed
  from", "fixed", "removed"); history belongs in commit messages and PRs.
  Remove such comments when you edit the code around them.
- Required rustdoc on public items is API documentation; inline comments
  still cover only what is not obvious.
- Follow the repo's comment-style rules on top of this (some repos lint for
  dashes, task markers or divider banners).

## Implementation checklist

| Step | Action |
|------|--------|
| 1 | Read the repo's AGENTS.md, Makefile and neighbouring modules for naming, error and layout patterns |
| 2 | Extend an existing type before adding a parallel abstraction |
| 3 | Design types and error enums first; implement `Display` and `Error` |
| 4 | Never log secrets, tokens or message bodies; log ids, names and byte lengths. Compare secrets in constant time |
| 5 | Shared state: check lock scope and order (`rust-concurrency-patterns`) |
| 6 | Add or update tests for the behaviour change, in the repo's test layout |
| 7 | Run the repo's full gate |
| 8 | Hot path changed: consult `rust-performance-reviewer` and add or update a bench |
| 9 | Parses external input or adds an operation callers can drive: add or extend a fuzz target (`rust-fuzzing`) |
| 10 | Public contract changed: update rustdoc, and any pinned fixtures or schema through the repo's documented regeneration step |
| 11 | Changelog entry if the repo keeps one |
| 12 | Granular commits: one logical change per commit |

## Testing conventions

- Behavioural test names: `a_read_message_is_pulled_once`, not `test_3`.
  A `///` line on each test says which behaviour it pins.
- Shared helpers live in `tests/common/mod.rs`.
- If a private helper needs a direct test and the repo keeps tests in
  `tests/`, make it `pub` and document it rather than adding a
  `#[cfg(test)]` escape hatch.
- Temp directories and unique paths; never touch the user's real data or
  config directories; always clean up.
- Cover the success path, the denial path, and restart or reopen cases for
  anything persistent. For wire formats, cover upgrade and compatibility.
- Auth and security: test missing credential, wrong credential, wrong scope,
  revoked credential, and the happy path with the *minimal* privilege that
  should work.
- Frozen fixtures and golden files are regenerated only by the repo's
  documented command after a deliberate change, and every changed value is
  read in the diff. Never hand-edit them.
- Process-global state (env vars, listeners, hooks) is serialized: one lock
  per test binary, or sequential scenarios inside one test under RAII
  guards.
- Assert ordering with deterministic capture (channels, a recording
  listener), never wall-clock timing.
- Pin external contract limits with production-shaped payloads, not toy
  fixtures.
- Concurrency: multi-thread runtime races for exactly-once and lost-wakeup
  cases (`rust-concurrency-patterns`).
- Hermetic suites miss real-world shape. Run one live smoke against the real
  consumer before calling a release done.
- Performance claims need numbers from Criterion or a load run.

## Review lens

Use when auditing a bug or a change.

1. **Correctness:** is the happy path right for the least-privileged caller?
2. **Fail modes:** do errors say which input failed and why, and distinguish
   "missing" from "wrong" from "not allowed"?
3. **Defaults:** what happens when optional parameters are omitted?
4. **Scope leakage:** can a scoped credential or tenant reach another one
   through nested fields, default keys or a misclassified operation?
5. **Delivery and integrity:** can this drop, duplicate or tear data? Does a
   persisted format still pass its integrity checks?
6. **Regression surface:** is there a test that would have caught this?
7. **Concurrency:** a lock across `.await`, locks out of order, a lost
   wakeup, a global lock on the hot path?
8. **Simplicity:** is there a smaller fix that keeps the contract?
9. **Release notes:** if the repo keeps a changelog, does it describe this?

Review type design before line-by-line logic, and failure modes and tests
before micro-optimizations. Prefer a small refactor that improves type
safety over a large stylistic rewrite.

## Hard-earned lessons

Distilled from real production mistakes. They override generic best
practice when they conflict.

1. **Verify claims before asserting them.** Assumptions about external
   systems die on live verification. Verify at the deepest level the
   pipeline depends on.
2. **Match the tool to the job.** Copying an existing pattern without
   checking its fit is how a broken core feature ships.
3. **Simulate the exact external contract before shipping.** A transport
   success (curl returns 200) does not verify rendering or protocol
   semantics.
4. **Timeouts cover every wait, including library internals and cleanup
   paths.** Wrap whole external operations in a watchdog, not only the steps
   that take a timeout parameter.
5. **"Success" on empty input is data loss.** Pre-flight any invariant that a
   downstream trim, queue or buffer can silently violate, and fail loudly.
6. **Persisted or displayed error strings never embed raw response bodies.**
   The parse-failure path is exactly when a body may carry live secrets.
   Summarize untrusted bodies with char-boundary-safe truncation.
7. **Never hold a `std::sync` or `parking_lot` guard across `.await`.**
   Clippy's `await_holding_lock` rejects it under `-D warnings`.
8. **Pin bogus credentials in "hermetic" tests.** Dev shells export real API
   keys; set them to junk so network stages cannot fire.
9. **Reachability is not 2xx.** A probe for "is this service present" treats
   401 and 403 as present. In clients where non-2xx is an `Err`,
   `call().is_ok()` reads a gated endpoint as absent.
10. **Evolve wire protocols with `#[serde(default)]` optional fields** and an
    explicit, tested story for how old peers degrade. Never change the
    meaning of an existing field.
11. **A library fix ships nowhere until every binary that links it is rebuilt
    and reinstalled.** Keep the binary matrix in mind on shared-crate
    changes.
12. **Never hardcode secrets in scripts.** Source them from one env or props
    file of truth; hardcoded copies drift into auth outages.
13. **Follow the existing naming convention before inventing one.** Grep the
    existing env vars, flags and error codes first.
14. **Verify topology before theorizing.** In any "the service is broken"
    mystery, first check which process listens on the port, which binary is
    installed, and which config it read. A wrong mental model is the most
    expensive bug.
15. **Narrate before touching a running service.** Even safe actions (stop,
    restart, wiping a data directory) alarm the operator when they arrive
    unexplained. Say what will run and why, and get the OK.
16. **After a surprising edit success, re-read the file.** An edit that
    should not have matched and did can leave a duplicated brace only the
    compiler catches.
17. **HTTP 200 is not "usable".** Check content type and body shape, not only
    status.
18. **Detection helpers fail silently.** Verify them live on the real host
    class; env overrides are not a substitute for checking the real path
    once.
19. **Expose one public function rather than copying a routine across
    crates.** Two copies always drift.
20. **Every fallback layer re-validates its output** before accepting it.
21. **Platform caps are part of the contract.** Dynamic content placed in a
    size-capped field gets a test pinning the limit with production-shaped
    payloads.

## Anti-patterns

- Stringly-typed states, scopes, roles or policies where an enum fits
- Tests in a layout the repo does not use
- `unwrap()` on request paths or inside lock sections
- Vague errors that do not say which input failed or why, or that say
  "missing" when the credential was present but the scope was wrong
- Silent fall-through to a permissive default without a documented reason
- `Arc<RwLock<HashMap<..>>>` held while working on a value
- A `std::sync` or `parking_lot` guard, or a `DashMap` `Ref`, alive at an
  `.await`
- Two locks taken in different orders on two paths
- Buffered writers without a `Drop` flush
- Drive-by refactors unrelated to the task
- `#[allow(dead_code)]` or `#[allow(clippy::...)]` without a written reason
- A performance claim without a bench or load number
- A new parser of untrusted input with no fuzz target, or a fuzz finding
  fixed without a regression test
- Hand-edited golden files or frozen fixtures
- A floating version on a dependency whose format is part of your contract
- Changelog lines (where the repo keeps one) that only restate the commit
  subject
- Timing-based ordering assertions in async tests
- Speculative fallbacks: a second mechanism nobody asked for

## Project notes

Before changing code, read the repo's `AGENTS.md` (and nested ones for the
area you touch), its `Makefile` or task runner, its changelog if any, and any
repo-local skills. They win wherever they are stricter or more specific than
this skill: the exact gate command, release-mode builds, MSRV, comment-style
lint, test layout, frozen rules or fixtures and how to regenerate them,
dependency direction between crates, pinned versions, env var prefixes,
ports and data paths.

## Version history

- **2026-11-07**: Shared general skill. Merged from the MentisChat
  `rust-engineer` (2026-10-30, itself a merge of the MentisDB and
  DiarioBitcoin skills) and the MentisDB `rust-engineer` (2026-10-30).
  Project specifics (Makefile targets, release-mode gate, changelog file and
  ordering, pinned MCP crate, tool counts, ports, env prefixes, data paths,
  mailbox semantics) replaced with general rules; test layout now defers to
  the repo.
