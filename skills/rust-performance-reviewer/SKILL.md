---
name: rust-performance-reviewer
description: >
  Disciplined performance review of Rust code through the lens of Jeff Dean
  and Sanjay Ghemawat's Performance Hints (Abseil), translated to idiomatic
  Rust: estimation, review method, severity, lenses for algorithms, memory
  representation, allocations, avoided work, compiler help, code size,
  synchronization, serialization, containers, in-memory stores, SQL
  pipelines and scheduled work, plus the measurement toolkit and report
  format. Use for hot-path audits, allocation or cache footprint review, API
  design for speed, Criterion-backed regressions, or flat profiles with no
  obvious hotspot. Load with rust-engineer; measure with
  rust-criterion-benchmarking; lock correctness lives in
  rust-concurrency-patterns.
triggers:
  - performance review
  - rust performance
  - micro-optimize
  - hot path
  - allocation profile
  - cache miss
  - flat profile
  - speed up rust
  - reduce allocations
  - lock contention
  - Arc clone
  - wake latency
  - pprof
  - perf
  - SmallVec
  - zero-copy
  - bulk API
  - sqlite slow
  - EXPLAIN QUERY PLAN
  - N+1
---

# Rust Performance Reviewer

Single-binary and library performance review of Rust code (CPU, memory, cache, allocations, locks, API shape), adapted from Jeffrey Dean and Sanjay Ghemawat, *Performance Hints*, 2025, https://abseil.io/fast/hints.html. Not distributed systems or ML hardware tuning.

**Core principle (Knuth, full quote):** forget about small efficiencies
about 97% of the time, *but do not pass up the critical 3%*. A 12% gain that
is easy to get is never marginal in quality software. Prefer the faster
alternative when it does not significantly hurt readability.

**Evidence before claims.** Every finding cites `file:line`, says whether
the code is on a **hot path**, and proposes one concrete fix. An unmeasured
"this looks slow" off a cold path is Minor at most.

## When to use

- Someone asks for a performance review, micro-optimizations, or "make it
  faster".
- A profile or Criterion run shows a hotspot, or a flat profile after the
  easy wins.
- Designing a library API that many call sites will use.
- Pre-merge audit of hot request, scrape, parse or encode paths.
- High RSS, allocator pressure, or cache-miss-bound code.
- A scheduled job or pipeline "lags" or "misses events".

**Do not** micro-optimize test-only code beyond asymptotic complexity, or
one-shot scripts. Never trade correctness for speed or add a second
mechanism.

**Correctness bugs often are the performance story.** A feature that never
fires (an always-false flag, a wrong comparison order) looks like lag or
missing work. Diagnose with live state before rewriting loops.

## Philosophy

1. **Think about performance while writing**, not only after profiles go
   flat. A flat profile means cost is smeared everywhere and hard to attack.
2. **Library code is high leverage.** Callers often cannot fix your
   internals. Choose good defaults (`SmallVec`, `&str`, bulk APIs) with low
   local complexity.
3. **Estimate before implementing.** Back-of-envelope with known costs;
   discard alternatives that cannot win.
4. **Measure before claiming.** Criterion for micro work; `perf` or `samply`
   CPU profiles for systems; allocation profiles when the allocator is
   suspect.
5. **Many 1% wins compound.** Twenty small clean improvements beat one
   heroic rewrite, given stable benches.
6. **Deep modules.** Keep public interfaces narrow so layout and allocation
   changes stay behind encapsulation boundaries.
7. **Do not pay for what you do not use.** Thread safety, generality, stats
   and logging on hot paths have real costs.

### Rough cost table (order of magnitude, modern x86)

| Operation | Cost |
|---|---|
| L1 reference | 0.5 ns |
| L2 reference | 3 ns |
| Branch mispredict | 5 ns |
| Uncontended mutex lock and unlock | 15 ns |
| Main memory reference | 50 ns |
| Compress 1 KB (Snappy class) | 1 µs |
| SSD 4 KB read | 20 µs |
| Same-datacenter network round trip | 50 µs |
| 1 MB sequential from DRAM | 64 µs |
| 1 MB over a 100 Gbps network | 100 µs |
| Disk seek | 5 ms |
| Transcontinental round trip | 150 ms |

Track *your* higher-level costs too: a lock section, a loopback HTTP hop,
serde of a typical payload, a SQL point read, an fsync. Without numbers you
cannot estimate.

**Worked example.** Scanning 1M strings for a prefix: about 50 MB of data,
so a memory-bound floor of about 50 MB / 16 GB/s, roughly 3 ms. If the
compare branch is data-dependent, about half of 1M branches mispredict:
0.5M × 5 ns, roughly 2.5 ms more. Bandwidth and mispredicts are the same
order, so (a) keep the data compact (an arena or `Box<str>`, not
`Vec<String>` with capacity slack) and (b) a `memchr`-style or SIMD scan
that removes the branch can roughly halve the time. The estimate picks the
fix *before* any code is written.

## Review method

1. **Scope.** Whole crate, module, hot function, or diff? Hot path or
   init and setup?
2. **Classify the code.**
   - Test-only: asymptotics and test runtime only.
   - Application-specific: is it per request or per item?
   - Library with many callers: apply techniques aggressively when cheap.
3. **Estimate** the dominant operations (allocations, copies, syscalls,
   locks, branches, bytes, queries).
4. **Measure** when tradeoffs are not obvious (Criterion; `perf record` or
   `samply`; `dhat` or allocation counters; `EXPLAIN QUERY PLAN` where SQL
   is involved).
5. **Apply the lenses** in order: structural and algorithmic, memory
   representation, allocations, avoided work, compiler help, code size,
   synchronization, serialization, containers; then the domain lenses
   (in-memory stores, SQL, scheduled work) that match the code.
6. **Report** with severity, evidence, impact class and one fix.
7. **Gate claims** with a bench or profile delta for Important and above.

### Severity

- **Critical:** pathological complexity on a production path (O(n²) per
  request, a full scan per read), unbounded memory growth, a lock held
  across network I/O, allocator thrash or uncapped parallel builds that OOM
  or freeze the host.
- **Important:** measurable hot-path waste: a needless allocation or copy
  per request, the wrong container, a missing `reserve`, a regex where a
  prefix match suffices, a missing bulk API forcing N crossings, a contended
  global lock, false sharing, a wrong SQL plan.
- **Minor:** cold-path micro-optimizations, `clone` cleanup, layout polish
  with a tiny footprint, speculative SIMD. Batch these only with approval.

### Impact classes

- **Structural or algorithmic:** often 2 to 10 times or more
- **Representation or cache:** often 10 to 50% system-wide when data is large
- **Allocation reduction:** often 10 to 30% on allocation-heavy paths
- **Avoided work or fast path:** highly variable, can be huge
- **Lock or parallel:** throughput wins on many cores
- **Death by a thousand cuts:** many 1% changes after the profile is flat

## Lens 1: Algorithmic improvements

Highest leverage. Prefer these over micro-optimizations.

- [ ] Wrong asymptotics (nested loops, repeated full scans, a sort where a
  hash works)?
- [ ] Sorted intersection that could be a hash-set lookup?
- [ ] Ordered map used only for equality lookups: `HashMap` / `HashSet`
- [ ] A structure built incrementally with per-edge checks that could be
  built in one shot?
- [ ] Hash quality good enough that expected O(1) is not accidentally O(n)?

Rust notes:

- Hot maps with internal (non-adversarial) keys: swap SipHash for
  `rustc_hash::FxHashMap` (rustc-hash 2.x) or `foldhash`. Keep SipHash for
  attacker-controlled keys.
- `HashMap::entry()` instead of `contains_key` plus `insert` (two hashes,
  two lookups).
- `Vec::swap_remove` (O(1)) instead of `remove` (O(n)) when order does not
  matter.
- `sort_unstable` when stability is not needed; `sort_by_cached_key` when
  the key is expensive.
- `VecDeque` for pop-front patterns, never `Vec::remove(0)`.
- Ordered lookups on moderate N: sorted `Vec` plus `binary_search` often
  beats `BTreeMap` on cache behaviour; `blart` (adaptive radix tree) for
  large ordered string keys.
- Build in bulk (`extend`, `from_iter`, `collect`) rather than repeated
  inserts with rehash churn.

## Lens 2: Better memory representation

Touch fewer cache lines and cut memory bus traffic (which also helps
neighbours on the machine).

### Compact data

- Smaller integer types when the domain fits (`u8`, `u16`, `u32` instead of
  `usize` or `u64`).
- `#[repr(u8)]` enums when the discriminant size matters.
- Bitflags, `bitvec` or manual bitsets instead of `HashSet<SmallId>`.
- Dense arrays indexed by small ids instead of maps.
- `Box<str>` / `Box<[T]>` for immutable stored data (drops the 8-byte
  capacity field): `String::into_boxed_str`, `Vec::into_boxed_slice`.
- `Arc<str>` / `Arc<[T]>` for shared immutable data: cheap clone, no double
  indirection as with `Arc<String>`.
- Enum size is the largest variant plus discriminant: `Box` large, cold
  variants so the common case stays small.

### Layout

- Order fields by access: hot together, cold last or behind a `Box` or in a
  separate struct-of-arrays.
- Separate hot read-mostly fields from hot mutable ones to reduce false
  sharing.
- Check sizes with `static_assertions::assert_eq_size!` in tests and
  `cargo +nightly rustc -- -Zprint-type-sizes` for full dumps.
- `#[repr(align(64))]` for false sharing is easy to overdo; measure.
- `#[repr(C)]` and manual packing only inside well-tested modules, validated
  with benches; packing can hurt through under-aligned hot loads.

### Indices instead of pointers

- On 64-bit, rich pointer graphs waste memory and scatter cache lines.
- Store `u32` indices into an arena or `Vec<T>` when cardinality fits;
  `slab` and `slotmap` (generational indices) are the standard crates.
- Contiguous storage beats pointer soup for traversal.

### Flat, batched, inline storage

- Avoid per-element heap nodes (`LinkedList`, naive trees) on hot data;
  prefer `Vec`, slabs, arenas, chunked structures and flat hash maps.
- When N is usually small: `smallvec::SmallVec<[T; N]>` (spills to the
  heap), `arrayvec::ArrayVec` (fallible push), `tinyvec`; `compact_str` or
  `smartstring` for short strings (inline up to 24 bytes). Large `T` or a
  large inline N bloats every instance.
- `HashMap<A, HashMap<B, V>>` is often better as `HashMap<(A, B), V>`;
  nested can win when the outer key is large and shared. Measure both.
- Arenas (`bumpalo`, `typed-arena`, or a `Vec` plus indices) for many
  short-lived related objects: fewer allocations, better locality, cheap
  bulk drop. A long-lived arena holding short-lived junk bloats memory.
- Small integer or enum domains: `[V; N]`, `Vec<Option<V>>` or a bitset
  instead of a map or set; set operations become word-parallel.

## Lens 3: Reduce allocations

An allocation costs allocator time, initialization and drop, and **cache
footprint** (in a long-running program each one tends to land on a new
cache line).

- [ ] Avoid allocating when a static or empty sentinel works (`&[]`,
  `Cow::Borrowed("")`).
- [ ] Stack storage (`ArrayVec`, small arrays) when the lifetime is scoped.
- [ ] `Vec::with_capacity` / `reserve` when the size is known; never grow
  one by one in a loop.
- [ ] `reserve` plus `push` over `resize` plus assign when construction is
  expensive.
- [ ] Move (`std::mem::take`, `swap`, ownership transfer) instead of
  `clone`.
- [ ] References, indices or `Arc` in transient structures instead of deep
  clones.
- [ ] Hoist buffers out of loops; reuse `Vec`, `String`, `BytesMut` with
  `clear`. Reused buffers keep their high-water capacity: `shrink_to` or
  recreate every N uses if sizes vary wildly.
- [ ] `Bytes` / `BytesMut` for shared I/O buffers instead of copying into a
  `Vec` and again.
- [ ] Iterator pipelines without an intermediate `collect` when one pass
  works.
- [ ] `Cow<'_, str>` / `Cow<'_, [T]>` when sometimes borrowed, sometimes
  owned.
- [ ] `Rc` instead of `Arc` in provably single-threaded code (atomic
  refcounts are not free).

Size-hint nuance: `iter.map(f).collect::<Vec<_>>()` reserves via
`size_hint`; `iter.filter(f).collect()` does **not**. Add `with_capacity` or
extend a reused buffer when the filtered count is predictable.

Smells (each is a finding on a hot path):

- `.clone()` on `String`, `Vec` or `HashMap` in a per-item loop.
- `format!` or `to_string()` just to pass something that takes
  `impl AsRef<str>`.
- `format!` chains in a loop instead of `write!` into one reused `String`
  (`use std::fmt::Write`).
- `to_lowercase() == other` instead of `eq_ignore_ascii_case` (ASCII only;
  flag the Unicode caveat).
- `Regex::new` inside a function body: compile once in a `LazyLock`, or
  better, use `starts_with`, `split_once` or `memchr` when a regex is not
  needed.
- Building the same `PathBuf` prefix repeatedly.
- `serde_json::to_vec` then parse, when a typed value already exists.
- `format!` for numbers on ultra-hot paths instead of `itoa` / `ryu`.
- `s.as_bytes().to_vec()` where `s.into_bytes()` moves the allocation.
- `Arc<Mutex<u64>>` or `Rc<RefCell<u64>>` for a counter instead of
  `AtomicU64` or `Cell<u64>`.

### Allocator choice

For allocation-heavy services, swapping the global allocator is often a
bigger win than any single code change: `mimalloc` or `tikv-jemallocator`
via `#[global_allocator]`. Gains are largest under multi-threaded churn, and
RSS behaviour changes too. One line; benchmark before and after on the real
workload.

## Lens 4: Avoid unnecessary work

Often the biggest win is not doing it.

- **Fast paths for common cases.** Make the common case branch-predictable
  and allocation-free; keep the slow path cold (`#[cold]`, a separate
  function) so it does not pollute the I-cache. Example: nothing new since
  the cursor, so return the empty response without building anything.
- **Precompute once.** Expensive properties, lookup tables, fingerprints of
  large blobs. Validate at module boundaries, not again deep inside.
  `OnceLock` / `LazyLock` for process-wide immutable tables.
- **Hoist from loops.** Bounds, date formatting, config flags, logger
  enabled checks.
- **Defer.** Do not compute stats, sharding or subtrees until a consumer
  needs them.
- **Specialize.** A hot call site may not need full generality:
  `starts_with` / `split_once` instead of a regex; `memchr` / `memmem` for
  byte and substring search; `aho-corasick` for many patterns; manual
  formatting instead of `format!`.
- **Cache.** Fingerprint-keyed caches for large serialized inputs.
- **Logging and stats on hot paths.** Even disabled logging costs a load and
  a branch and can block inlining. Check `tracing::enabled!` or a cached
  bool outside nested loops; use the `max_level_*` / `release_max_level_*`
  cargo features of `tracing` or `log` to compile disabled levels out;
  sample stats (1 in N) instead of every event; `AtomicU64` relaxed
  increments, not `Mutex<u64>`; delete counters nobody reads.

## Lens 5: Help the compiler

Only when profiles show pain; rustc and LLVM are usually good already.
Inspect assembly (`cargo-show-asm`, `perf annotate`, Godbolt) for critical
functions.

- Avoid `dyn Trait` in the hottest loops when monomorphization is
  acceptable.
- Move slow paths into `#[cold]` / `#[inline(never)]` functions so the hot
  path stays lean; `#[inline(never)]` on rare error paths.
- Iterate over slices instead of indexing (`for x in s`, `chunks_exact`) to
  elide bounds checks and enable autovectorization.
- Copy small hot data into locals to end aliasing doubts.
- Explicit SIMD: `std::simd` (nightly), `wide`, or `core::arch` intrinsics
  for bulk byte or number work. `packed_simd` is deprecated; do not
  recommend it.
- Process chunks of 4, 8 or 16 with `chunks_exact` when alignment allows.
- `#[inline]` helps tiny hot getters and hurts when code size explodes.
- Limit monomorphization blowup: trait objects or function pointers for
  cold, bulky code; keep hot kernels monomorphized.

Build flags (cheap, system-wide):

- `[profile.release]`: `lto = "thin"` (or `"fat"` for the final binary),
  `codegen-units = 1`, `panic = "abort"` where acceptable.
- `RUSTFLAGS="-C target-cpu=native"` on controlled deploy targets only, not
  for distributed binaries.
- PGO via `cargo-pgo` for mature hot binaries, typically 5 to 15%.
- `debug = 1` (line tables) in the profile used for profiling so `perf` and
  `samply` attribute correctly.

## Lens 6: Code size

Large code means longer builds, fatter binaries, I-cache pressure and worse
branch prediction. It matters most for widely used generics and macros.

- Measure monomorphization bloat with `cargo llvm-lines` and binary size with
  `cargo bloat`.
- Trim code that ends up inlined at many call sites; avoid heavy `format!`
  and `Display` machinery in tiny inlined helpers.
- Collapse repeated insert-based initialization into one `from` or
  `extend`.
- Move non-generic bulky logic into a non-generic inner function
  (`fn inner(x: &str)` called from `fn api<S: AsRef<str>>(s: S)`).
- `impl AsRef<str>` on a widely called API monomorphizes per caller;
  sometimes `&str` plus one conversion at the edge is smaller.
- Watch serde and proc-macro derives on huge types pulled into hot crates.

## Lens 7: Parallelism and synchronization

This lens is about cost. Correctness rules (lock order, no guard at
`.await`, lost wakeups) live in rust-concurrency-patterns.

- Parallelize independent items (`rayon` for CPU-bound work, batches of
  Tokio tasks for I/O-bound work) when spare cores and memory bandwidth
  exist. Measure; a saturated memory bus can make parallel code slower.
- Amortize locking: one acquisition per batch, not per item.
- Keep critical sections short; **never** hold a lock across `.await`, RPC
  or disk I/O.
- Shard contended maps (`DashMap`) or use concurrent maps, and check which
  hash bits pick the shard so the inner tables are not skewed.
- False sharing: `crossbeam_utils::CachePadded<T>` around per-thread
  counters rather than hand-rolled alignment.
- Counters and flags: atomics with `Ordering::Relaxed` instead of
  `Mutex<u64>`; `parking_lot` for smaller, faster locks in sync code.
- Bounded channels and batching instead of a spawned task per item.
- Lock-free only via proven structures (`crossbeam`, `arc-swap`).
- Library types default to thread-compatible (external synchronization);
  add internal synchronization only when typical use needs it, so
  uncontended callers do not pay.
- Wakeups: `notify_waiters` (or many `notify_one`s) on one shared `Notify`
  wakes every parked waiter to find nothing, a thundering herd. One
  `Notify` per waiter identity wakes only the one that has work.
- Cap parallelism on shared hosts: uncapped `cargo` builds or worker pools
  can freeze a production machine (use `--jobs` and `nice`, or build
  elsewhere).

## Lens 8: Serialization and heavy schema data

Heavy schema frameworks are convenient and **expensive** (Abseil's example:
a list of 1000 points is about 20 times faster as a plain struct vector than
as protobuf). Beware:

- `prost` / `protobuf` / large `serde` graphs in hot inner loops.
- Deep message hierarchies and map fields.
- Encoding or decoding the same blob repeatedly.

Prefer:

- Plain structs and `Vec` on internal hot paths; serialize at boundaries.
- `bytes::Bytes` to avoid copying large fields.
- Reusing parse buffers and message objects across iterations when the API
  allows.
- `simd-json` for proven hot JSON parsing (needs a mutable input buffer).
- `itoa` / `ryu` for number formatting.
- Compact encodings (`rkyv`, flatbuffers, a custom format) only when
  measured need justifies them.

## Lens 9: Containers and idioms (Abseil to Rust)

### Containers

| Abseil / C++ idea | Rust default | Faster or denser |
|---|---|---|
| `std::vector` | `Vec<T>` | pre-`reserve`; `SmallVec` / `ArrayVec` if small |
| `absl::InlinedVector` | none | `smallvec`, `arrayvec`, `tinyvec` |
| `std::unordered_map` | `HashMap` (hashbrown) | `FxHashMap` / `foldhash` for internal keys |
| `absl::flat_hash_map` | hashbrown (open addressing) | good hasher, bulk insert, `entry` API |
| `std::map` | `BTreeMap` | sorted `Vec` plus `binary_search`; `blart` for string keys |
| bit sets | `HashSet` of ids | `bitvec`, `fixedbitset`, integer masks |
| `gtl::small_map` | none | `SmallVec` of pairs with linear scan; `heapless::FnvIndexMap` |
| intrusive list | `LinkedList` (rare) | `intrusive-collections`; usually `Vec` plus indices |
| arenas | none | `bumpalo`, `typed-arena`, `slab`, `slotmap` |

### Strings and bytes

| Need | Reach for |
|---|---|
| Shared immutable string | `Arc<str>` (not `Arc<String>`) |
| Stored immutable string | `Box<str>` via `into_boxed_str` |
| Short strings | `compact_str`, `smartstring` |
| Conditional ownership | `Cow<'_, str>` |
| Substring or byte search | `memchr`, `memchr::memmem` |
| Many patterns | `aho-corasick` |
| Shared I/O buffers | `bytes::Bytes` / `BytesMut` |
| Number formatting | `itoa`, `ryu` |

### Sync and misc

| Abseil / C++ idea | Rust default | Alternative |
|---|---|---|
| `string_view` / `Span` | `&str`, `&[T]` | always prefer views in APIs |
| `FunctionRef` | `impl Fn` / `fn()` | avoid `Box<dyn Fn>` on hot calls |
| `alignas(64)` against false sharing | none | `crossbeam_utils::CachePadded` |
| faster mutex | `std::sync::Mutex` | `parking_lot` (sync code) |
| stats counters | `Mutex<u64>` | `AtomicU64` (relaxed) |
| allocator | system malloc | `mimalloc` / `tikv-jemallocator` |
| `Status` / `StatusOr` tax | `Result<T, E>` | do not force `Result` on ultra-hot infallible paths |

API design:

- **Bulk APIs** (`get_many`, `delete_many`, batch encode and decode)
  amortize locking and boundary costs. If callers cannot change, batch
  internally and cache.
- **View types:** accept `&str`, `&[T]`, `impl AsRef<Path>`, not an owned
  `String` unless ownership transfers.
- **Caller-provided resources:** let callers pass clocks, buffers and
  scratch space they already have.
- **Thread-compatible by default:** external synchronization unless typical
  use needs internal.

## Lens 10: In-memory stores and shared state

Applies to in-memory engines: append-only logs of `Arc`'d entries, per-reader
cursors, short lock sections, per-waiter `Notify`. Their costs are lock hold
time, copies and wakeups, not I/O.

- [ ] **Read cost is O(new), not O(log).** A read starts at the cursor; a
  scan of the whole log (or of every log) per read is Critical at scale.
- [ ] **No body copies.** Writes, reads, history and views clone `Arc<T>` or
  `Arc<str>`, never `String` bodies.
- [ ] **Short sections.** Under the lock: append, or read past the cursor
  and advance it. JSON, logging, formatting and timestamps happen after
  release.
- [ ] **One lock acquisition per batch.** A read of N entries takes the lock
  once, not N times.
- [ ] **Targeted wakeups.** Notify only the waiter that can have work.
- [ ] **Amortized retention.** Pruning is bounded per pass and compacts in
  chunks (`VecDeque::drain(..k)`), never one `remove(0)` at a time.
- [ ] **Bounded growth.** If entries do not expire on their own, caps (per
  identity, per payload, per process) are what bound memory; check they
  still hold.
- [ ] **Readers do not stall writers.** `RwLock` read sections or a
  snapshot of `Arc`s, released before serializing.
- [ ] **Measured** with benches at several sizes (1 to 1000 entries),
  contention at 1 to 8 workers, and wake latency.

## Lens 11: SQL and local database pipelines

Applies wherever SQLite (or another SQL store) is on the path. After pure
algorithms, this is the highest-leverage lens on store-backed services and
jobs.

- [ ] **`=` not `LIKE`** for exact keys. Measured on a 1.1M-row production
  table: `LIKE ?` 73.5 s serial vs `= ?` 0.97 s (about 75 times).
- [ ] **`EXPLAIN QUERY PLAN`** on every new or changed query against a
  production-sized database: `SEARCH`, not `SCAN`.
- [ ] **Composite indexes match real predicates.** Single-column indexes are
  often dead weight; prefer `(key, time)` composites.
- [ ] **One transaction per batch** (`BEGIN IMMEDIATE` ... `COMMIT`).
  Hundreds of autocommits mean hundreds of fsyncs.
- [ ] **Prepare statements once**; reset and re-bind in the loop.
- [ ] **WAL mode** for readers during a writer
  (`PRAGMA journal_mode=WAL`), and `busy_timeout` on **every** connection
  (the default 0 turns contention into instant `database is locked`
  errors).
- [ ] **Cap workers on shared hosts**; one job does all the work for an
  item rather than two fan-outs of connection opens.
- [ ] **No `DISTINCT` / `GROUP BY`** when the filter already returns one row
  per key.
- [ ] **N+1 point queries** become a bulk `JOIN` or a lookup of only the
  keys needed.
- [ ] Drop broken or unused indexes; `ANALYZE` after schema changes;
  `PRAGMA integrity_check` after index surgery.
- [ ] Back up before live schema changes; `CREATE INDEX IF NOT EXISTS` for
  idempotence.

## Lens 12: Scheduled and multi-stage work

Cadence multiplies waste: a periodic job that does N times the work before
noticing it has nothing to do is a production cost, not a micro-optimization.

- [ ] **Order work cheapest to dearest:** kill switch and nothing-to-do
  check, then throttles, then load state, then I/O or heavy computation.
- [ ] **Do not pay N times the work over the whole queue** when this pass
  advances one item.
- [ ] **Atomic publish** of artifacts consumers may read mid-write: write a
  temp sibling, then `rename`. Truncating with `File::create` mid-read
  corrupts readers.
- [ ] **Logging:** one summary line per pass; `debug` or `trace` per item.
  A per-item `info!` times hundreds of items per run is a log storm.
- [ ] **Reuse loaded inputs** across steps in the same process.
- [ ] **Timeouts on every external call**, so a hung peer cannot stall the
  pass.
- [ ] **Absolute paths** in anything run by cron, launchd or systemd; no
  CWD-relative paths.

## Field lessons

Measured patterns from production Rust services and jobs. Prefer them over
generic advice when auditing similar shapes.

1. **A SQL plan beats Rust micro-optimizations by orders of magnitude.**
   Measured before and after on production data: `LIKE` to `=` (73.5 s to
   0.97 s), removing a `DISTINCT` (1.2 s to 0.3 ms), a composite index
   (3.7 s to 4 ms), batching into one transaction with prepared statements
   (several seconds to about 0.6 s). On store-backed jobs, start with
   `EXPLAIN QUERY PLAN` and a serial full-workload timer **before** touching
   `SmallVec` or LTO.
2. **Correctness bugs masquerade as lag.** Always-false predicates
   (assigned-then-checked timestamps, a wrong comparison order) silently
   disable features while CPU looks busy. Inspect live state and the
   predicates, not only CPU, and pin the bug with a regression test.
3. **Parallelism without `busy_timeout` is a footgun.** N workers with fresh
   SQLite connections on a big database fail with `database is locked` when
   `busy_timeout` is 0. Set `busy_timeout`, `query_only=ON` on readers, WAL
   on the writer, and cap workers.
4. **Logging and non-atomic writes are production features.** See Lens 12.
5. **Queue physics beats micro-optimizations on scheduled pipelines.** Work
   ordering and quota-aware soft failures matter more than faster
   serialization.
6. **Measure loop.** Back up before schema changes; time binaries with
   production config (`/usr/bin/time` for elapsed time and max RSS);
   re-check query plans; record wins with numbers in the commit or
   changelog.

## Measurement toolkit

| Need | Tool |
|---|---|
| Microbench | `criterion` (load rust-criterion-benchmarking); `divan` as a lighter alternative |
| CI-stable regression gate | `iai-callgrind` (instruction counts, immune to noise) |
| CPU profile | `samply record`, `perf record` plus `cargo flamegraph` |
| Assembly | `cargo-show-asm`, Godbolt |
| Allocations | `dhat`, `heaptrack` |
| Cache and branch misses | `perf stat -e cache-misses,branches,branch-misses` |
| Async task and lock stalls | `tokio-console` |
| Binary size | `cargo bloat` (`twiggy` is for wasm) |
| Monomorphization bloat | `cargo llvm-lines` |
| Type layouts | `cargo +nightly rustc -- -Zprint-type-sizes` |
| PGO | `cargo-pgo` |
| SQL plans | `EXPLAIN QUERY PLAN` on production-sized data |
| Whole-binary cost | `/usr/bin/time` (elapsed, max RSS) with production config |

Rules:

- Bench in release mode with the same RUSTFLAGS as production.
- `black_box` inputs and outputs.
- Throughput metrics for batch work.
- Distrust single-run deltas; use named baselines.
- Microbenches can lie; validate important wins on realistic workloads.
  `format!` vs `write!` is noise next to a 75 times query-plan fix.

## Flat profile playbook

When the CPU profile has no tall towers:

1. Take the many 1% wins (layout, `reserve`, hoisting, cold paths), each
   with a bench.
2. In the flame graph, find loops near the top of stacks and restructure the
   whole loop.
3. Step up a level: structural and API changes beat instruction tweaks.
4. Replace overly general code (regex to prefix match, serde to a manual
   parser, generic to specialized).
5. Take an allocation profile and cut the top allocation sites.
6. Read hardware counters for cache and branch misses (`perf stat`,
   `perf mem`).
7. Check I-cache pressure: binary growth and over-inlining (`cargo bloat`,
   `cargo llvm-lines`).

## Review output format

```markdown
## Performance review: <scope>

### Summary
- Hot paths identified: ...
- Profile or bench evidence: ... (or "static review only")
- Top opportunities: ...

### Findings

#### [Critical|Important|Minor] <title>
- **Where:** `path/file.rs:LINE`
- **Evidence:** quote, and why it is hot
- **Mechanism:** allocation / cache / algorithm / lock / avoided work / query plan / ...
- **Impact class:** structural | representation | allocation | ...
- **Fix:** minimal concrete change (a code sketch is fine)
- **Validate:** Criterion case, perf command, or why the estimate suffices

### Non-findings and accepted costs
- ... (generality or readability tradeoffs that are explicitly fine)

### Suggested bench plan
- ...
```

Push back on micro-optimizations that hurt clarity with no hot-path
evidence, and on "optimize everything": prioritize the critical 3%.

## Quick hot-path checklist

1. Per item, per run or per request? If not, deprioritize.
2. **Structural first:** a full scan per read, N+1 lock acquisitions or
   queries, a wrong SQL plan, an unbounded queue, a full re-scan before the
   throttle? Fix those before allocation polish.
3. Shared state: `Arc` clones, not body copies? Work done after releasing
   the lock? Only the waiter with work woken?
4. Any allocation or copy that could be a borrow, move or reuse? Capacity
   reserved? Buffers hoisted? `write!` instead of `format!` in large loops?
   `entry` instead of a double lookup?
5. Any `HashMap`, `BTreeMap`, `Regex` or `format!` that could be simpler or
   denser?
6. Locks and pools: scope minimal, one acquisition per batch, sharded where
   contended, nothing held over I/O or `.await`, atomics for counters,
   `busy_timeout` and a worker cap where SQLite is shared?
7. API: could bulk operations and views help without breaking
   encapsulation?
8. Pipeline order: cheap gates before expensive I/O? Atomic publish?
9. Logging and stats on the path: per-item `info!`, levels compiled out,
   sampling, hoisted checks?
10. Layout: hot fields together, cold behind `Box`, enums not bloated by cold
    variants?
11. Compiler: cold paths marked, inlining not exploding size, iterators
    instead of indexing?
12. Is the predicate that gates the work actually correct (live state, not
    only CPU)?
13. Measured or estimated before recommending Important or Critical changes?

## Anti-patterns

- A finding with no `file:line`, no hot-path argument, or no fix
- Claiming a win without a bench, profile or written estimate
- Micro-optimizing cold paths, tests or one-shot scripts
- Polishing allocations while a full scan, N+1 or bad query plan sits on the
  same path
- Trading correctness or clarity for an unmeasured gain
- Adding a second mechanism (a "fast path" that can disagree with the slow
  one) without a differential test
- Hand-rolled lock-free code where a proven crate exists
- `target-cpu=native` on binaries that ship to other machines
- Uncapped parallel builds or worker pools on a shared production host
- Treating Criterion's "regressed" label as a verdict instead of comparing
  medians against a named baseline

## Version history

- **2026-11-07**: Shared general skill. Merged the MentisChat
  `rust-performance-reviewer` (2026-10-30) and the MentisDB
  `code-performance-reviewer` (both from the DiarioBitcoin skill). Kept the
  lens structure and the newer MentisChat text for the core lenses; kept the
  MentisDB quick-checklist items (capacity, bulk API, layout, compiler) that
  the newer version had dropped; generalized the mailbox lens to in-memory
  stores and the cron lens to scheduled work; dropped project-specific
  host constraints and incident references, keeping the general rule (cap
  parallelism on shared hosts); added Anti-patterns.
