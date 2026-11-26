---
name: swift-performance-reviewer
description: >
  Disciplined performance review of native macOS Swift, SwiftUI, and AppKit
  code, adapted from Jeff Dean and Sanjay Ghemawat's Abseil Performance Hints:
  hot-path classification, estimation, measurement with Instruments, and
  severity-graded findings across algorithms, memory representation and
  copy-on-write, allocations, avoided work, compiler help, code size, Swift
  Concurrency and main-actor hops, Codable/JSON parsing, containers, SQLite,
  and polling/network ordering, with a standard report template. Use when
  asked for a performance review, micro-optimization, or "make it faster",
  when Instruments shows a hotspot or a flat profile, when the UI lags or the
  CPU spins, or before merging changes to hot poll, render, parse, or query
  paths. Load swift-engineer alongside it for the general house style.
triggers:
  - performance review
  - micro-optimize
  - make it faster
  - hot path
  - allocation profile
  - reduce allocations
  - cache miss
  - instruments
  - time profiler
  - copy-on-write
  - codable slow
  - JSONDecoder slow
  - DateFormatter slow
  - SQLite slow
  - main actor hop
  - flat profile
  - SwiftUI lag
  - N+1
tags:
  - swift
  - macos
  - performance
  - micro-optimization
  - allocations
  - cow
  - codable
  - sqlite
  - instruments
  - main-actor
  - abseil
---

# Swift Performance Reviewer

Disciplined performance review of Swift and macOS code (apps, menu bar agents,
CLI tools, libraries) through the lens of the Abseil *Performance Hints*,
translated to idiomatic Swift, SwiftUI, and AppKit. Scope: CPU, memory,
copy-on-write behavior, allocations, cache, locks, async and actor hops, and
API shape. Not distributed systems or ML hardware tuning.

Companion: load `swift-engineer` for the general house style. This skill
assumes it.

## When to use

- The user asks for a performance review, micro-optimizations, or "make it
  faster".
- Instruments shows a hotspot, an allocation spike, or a flat profile after the
  low-hanging fruit is gone.
- Designing APIs (types, parsers, models) that many call sites will use.
- Pre-merge audit of hot poll, render, parse, encode, or query paths.
- Investigating high RSS, retain churn, main-actor stalls, or laggy SwiftUI or
  AppKit updates.

**Do not** micro-optimize test-only code beyond asymptotic complexity, or
one-shot scripts. Do not sacrifice correctness or introduce dual mechanisms.

**Correctness bugs are often the performance story.** A feature that "never
fires" (an always-false flag, a wrong comparison, a stale cached value) looks
like lag or missing work. Inspect live state before rewriting loops.

## Principles

**Knuth, full quote:** forget about small efficiencies about 97% of the time,
*but do not pass up the critical 3%*. A 12% easily obtained gain is never
marginal in quality software. Prefer the faster alternative when it does not
significantly hurt readability.

**Evidence before claims.** Every finding cites `file:line`, states whether the
code is on a **hot path**, and proposes one concrete fix. Unmeasured "this
looks slow" off a cold path is Minor at most.

1. **Think about performance while writing**, not only after profiles go flat.
   A flat profile means cost is smeared everywhere and is hard to attack.
2. **Library and model code is high leverage.** Callers cannot fix your
   internals. Choose good defaults (value types, lean decoding, bulk APIs)
   with low local complexity.
3. **Estimate before implementing.** Back-of-envelope with known costs;
   discard alternatives that cannot win.
4. **Measure before claiming.** Instruments for systems; XCTest `measure` or a
   benchmark package for micro work; the Allocations instrument for the
   allocator.
5. **Many 1% wins compound.** Twenty small clean improvements beat one heroic
   rewrite, given stable benchmarks.
6. **Deep modules.** Keep public interfaces narrow so layout and allocation
   changes stay inside encapsulation boundaries.
7. **Do not pay for what you do not use.** Thread safety, generality, stats,
   `@MainActor` isolation, and logging on hot paths have real costs.

### Rough cost table (order of magnitude, modern x86/ARM)

| Operation | Approximate cost |
|---|---|
| L1 reference | 0.5 ns |
| L2 reference | 3 ns |
| Branch mispredict | 5 ns |
| Uncontended lock/unlock (`os_unfair_lock`) | 15 ns |
| Main memory reference | 50 ns |
| Compress 1 KB (Snappy class) | 1 µs |
| SSD 4 KB read | 20 µs |
| Same-datacenter network round trip | 50 µs |
| 1 MB sequential DRAM read | 64 µs |
| 1 MB over a 100 Gbps network | 100 µs |
| Disk seek | 5 ms |
| Transcontinental round trip | 150 ms |

Track your own higher-level costs too: one SQLite point read, one HTTP hop, one
`JSONSerialization` parse of a typical payload, one `DateFormatter.string`, one
`NSImage` render. Without numbers you cannot estimate.

**Worked example:** aggregating 100k rows from SQLite, each needing
`String(cString:)`, then `.data(using: .utf8)`, then `JSONSerialization`, then
key lookups. That is about 4 allocations plus a full JSON tree per row to read
3 fields. Floor estimate: 100k × (1 to 5 µs per parse) is 0.1 to 0.5 s, plus
autorelease pressure. Extracting the 3 fields in SQL (`json_extract`) or with a
lean scan cuts both parse time and allocation count; if per-row cost dominates
the profile, make that fix before touching the query plan.

## Review method

1. **Scope.** Whole target, module, hot function, or diff? Hot path or
   init/setup?
2. **Classify the code.**
   - Test-only: asymptotics and test runtime only.
   - App-specific: is it per poll, per render, per item, per request?
   - Library or multi-caller: apply techniques aggressively when cheap.
3. **Estimate** the dominant operations: allocations, COW copies, retains,
   syscalls, locks, actor hops, branches, bytes moved.
4. **Measure** when tradeoffs are not obvious: Instruments (Time Profiler,
   Allocations, SwiftUI), XCTest `measure`, `os_signpost` around candidates,
   `MetricKit` for field data.
5. **Apply the lenses in order:** algorithmic, memory representation,
   allocations, avoided work, compiler help, code size, concurrency,
   serialization, containers, SQLite, polling and network ordering.
6. **Report** with severity, evidence, impact class, and one fix (template
   below).
7. **Gate claims** with a profile or benchmark delta for Important and above.

### Severity

- **Critical**: pathological complexity on a production path (O(n²) per poll),
  unbounded memory growth (retaining a huge `String`, `Data`, or `Substring`,
  an ever-growing cache), a lock or the main actor held across network I/O,
  allocator thrash that stalls the UI.
- **Important**: measurable hot-path waste: needless allocation or copy per
  poll or render, `DateFormatter` or `NSRegularExpression` created per call,
  `[UInt8](data)` copies in a parse loop, full JSON parsing per row to read 3
  fields, `firstIndex(of:)` inside a sort comparator, missing
  `reserveCapacity`, eager string building for filtered logs, needless
  `@MainActor` hops, N+1 SQLite opens or queries.
- **Minor**: cold-path micro-optimizations, copy cleanup of style-level
  significance, tiny layout polish, speculative caching. Batch only with
  approval.

### Impact classes

- **Structural / algorithmic**: often 2× to 10× or more
- **Representation / cache / COW**: often 10% to 50% when data is large
- **Allocation reduction**: often 10% to 30% on allocation-heavy paths
- **Avoided work / fast path**: highly variable, can be huge
- **Lock / actor-hop reduction**: main-thread stalls and multi-core wins
- **Death by 1000 cuts**: many 1% changes after the profile is flat

## Lens 1: Algorithmic improvements

Highest leverage. Prefer these over micro-optimizations.

- [ ] Wrong asymptotics: nested loops, repeated full scans, sorting where
      hashing works
- [ ] Linear `contains` / `firstIndex` on an array in a loop: use `Set` or
      `Dictionary` membership
- [ ] `firstIndex(of:)` **inside a sort comparator** (O(k² log k)): build
      `let rank = Dictionary(uniqueKeysWithValues: ids.enumerated().map { ($1, $0) })`
      once and compare ranks
- [ ] `Dictionary` where a `Set` suffices; an ordered structure used only for
      equality lookups
- [ ] Grouping with nested loops or `if dict[k] == nil` insertion: use
      `Dictionary(grouping:by:)`
- [ ] Building a dictionary by inserting in a loop: use
      `Dictionary(uniqueKeysWithValues:)` or `init(_:uniquingKeysWith:)`
- [ ] `reduce` building a collection: use `reduce(into:)` (no intermediates)

Notes:
- For sorted lookups on moderate N, binary search on a sorted `Array`
  (`partitioningIndex(where:)` from swift-algorithms, or a small extension)
  often beats `Dictionary` on cache behavior.
- `remove(at: 0)` is O(n); for front removal use `Deque` (swift-collections),
  `ArraySlice`, or index math.
- `sort()` is in place; prefer it to `sorted()` when the original can be
  consumed.

## Lens 2: Memory representation (including COW)

Touch fewer cache lines and move fewer bytes.

**Value types and copy-on-write**
- Structs are cheap to pass but **expensive to mutate when shared**: mutating a
  `var` that shares storage with other copies triggers a full copy. Keep shared
  data in `let`; when a large buffer must be mutated, make sure it has one
  owner, or use `withUnsafeMutableBufferPointer`.
- `String`, `Array`, `Dictionary`, and `Data` are COW. Sharing is free until
  someone mutates.
- A retained `Substring` keeps the **entire parent** alive; copy with
  `String(substring)` when the slice outlives the parent.
- `ContiguousArray<T>` guarantees contiguous storage (useful when bridging or
  pointer work dominates).
- `let` plus derived values makes COW a non-issue because nothing mutates
  shared storage.

**Compact data**
- Smaller integer types when the domain fits (`Int32`, `Int16`, `UInt8`).
- An enum with payloads is as large as its largest payload plus a tag; put
  large cold payloads behind `indirect` or a `final class` reference.
- A `UInt64` bitmask or `[Bool]` instead of `Set<SmallEnum>` for tiny, hot
  domains.
- A dense `Array` indexed by small integer ids instead of `[String: V]` when
  ids are bounded: no hashing, perfect locality.
- Avoid double storage (a `String` and a `Data` copy of the same payload).

**Layout**
- Field order affects size through padding; group same-size fields. Verify
  with `MemoryLayout<T>.size` and the Allocations instrument, do not guess.
- Keep hot read-mostly fields together and hot mutable fields apart in
  multi-threaded code.

**Indices instead of references**
- In a graph of many small objects, arrays of structs plus integer indices beat
  a web of `class` references: fewer allocations, better locality, no ARC
  traffic.
- ARC has real cost: hot loops over class instances pay retain/release; value
  collections mostly do not.

## Lens 3: Reduce allocations

Allocation cost is allocator time, init and deinit, and **cache footprint**.

- [ ] `reserveCapacity(_:)` when the count is known; never grow one element at a
      time in a loop without reserving
- [ ] `Dictionary(minimumCapacity:)` to avoid rehash churn
- [ ] `append` with reserved capacity over `+=` concatenation in loops
- [ ] Strings built with `joined(separator:)` or `reduce(into:)`, never
      `reduce("") { $0 + $1 }` (O(n²)) or `String(format:)` in a loop
- [ ] `DateFormatter`, `NumberFormatter`, and `NSRegularExpression` are
      expensive to construct: `static let` or a lock-guarded cache
- [ ] `String(n)` or interpolation, not `String(format: "%d", n)` (bridges
      through `CVarArg` and `NSString`)
- [ ] Full-tree JSON parses replaced by targeted extraction when only a few
      fields are read
- [ ] No `[UInt8](data)` copies in parse loops: use `data.withUnsafeBytes` with
      pointer or index math
- [ ] `Data.subdata(in:)` copies; prefer slices or `withUnsafeBytes` on hot
      paths
- [ ] Fonts, colors, buffers, and `[NSAttributedString.Key: Any]` hoisted out
      of loops and render functions
- [ ] Big AppKit/Foundation loops wrapped in `autoreleasepool { }`
- [ ] Foundation-to-Swift bridging done once at the edge, not per element
- [ ] `hasPrefix`, `range(of:)`, or `contains` instead of a regex when a regex
      is not needed
- [ ] `DateFormatter` and `NumberFormatter` are not safe to share across
      threads without confinement; `Date.FormatStyle` is a value type and safe
      to reuse

Each of these is a finding on a hot path:
- `DateFormatter()` created inside a render, row, or bucketing function
- `NSRegularExpression(pattern:)` created per call
- `String(format: "%.1f", ...)` on a hot log line
- `xs.map { String($0) }.joined()` when one pass suffices
- `JSONSerialization.jsonObject` per database row to read two fields
- `String(cString:)` plus `.data(using: .utf8)` plus full JSON per row in a
  SQLite loop
- `items.map { ... }.joined(separator:)` built eagerly for a log line the level
  may discard

## Lens 4: Avoid unnecessary work

Often the biggest win: do not do it.

- **Fast paths for common cases:** `guard` the cheap nil, disabled, or hidden
  cases first; keep the common path branch-predictable and allocation-free;
  keep slow paths separate.
- **Precompute once:** expensive properties, lookup tables, rank maps, regexes,
  formatters as `static let` (lazy and thread-safe in Swift).
- **Hoist from loops and renders:** date formats, config flags, fonts, colors,
  attribute dictionaries, ordering lookups.
- **Defer:** do not compute series, legends, or derived views until a consumer
  needs them; keep debug dumps inside `#if DEBUG`.
- **Specialize:** a hot call site may not need full generality: `hasPrefix`
  over a regex, `Set` over `[String]`, a manual parse over `JSONDecoder` when
  the shape is unstable and few fields are needed.
- **Cache:** key expensive renders by a fingerprint of their inputs. **The key
  must encode every input that changes the output**, including appearance,
  scale, locale, and time zone, or the cache must be cleared when one changes.
  Bound caches (`NSCache` or an explicit limit).
- **Logging and stats:** `os.Logger` interpolation is evaluated eagerly even
  when the level is filtered; do not build expensive strings for it. Gate with
  a level check or `#if DEBUG`, log a summary instead of per item, and sample
  stats (1 in N) rather than recording every event.

## Lens 5: Help the compiler

Only when profiles show pain; the optimizer is usually good. Inspect with Time
Profiler and, for critical functions, the generated assembly (Xcode "Show
Assembly", `swiftc -emit-assembly`, or Compiler Explorer).

- Concrete types or generics over existentials (`any P`) in hot loops;
  existentials box and dispatch dynamically.
- `some` for opaque returns; no `Any` casts in loops.
- Small, inlinable hot kernels; `@inline(__always)` sparingly (helps tiny
  getters, hurts when code size explodes); `@usableFromInline` for internal
  hot helpers across modules.
- Large synthesized `Codable` conformances generate a lot of code; hand-written
  decoding can win when measured.

**Build settings (cheap, system-wide)**
- Release: whole-module compilation and `-O`. `-Ounchecked` drops runtime
  checks; use only where provably safe.
- `-Osize` when binary size or I-cache matters more than a few percent.
- Profile-guided optimization: generate a profile from a representative run,
  then enable it.
- Dead stripping in the linker. Concurrency checking settings are about
  correctness, not speed.

## Lens 6: Code size (I-cache and compile time)

Large code means longer builds, fatter binaries, I-cache pressure, and worse
branch prediction.

- Measure binary size with Xcode's size report or `size -m`.
- Watch heavy `Codable` conformances and broad generic constraints in hot
  modules.
- SwiftUI view splitting is for maintainability, not speed; do not over-split
  for "performance".
- Prefer a `static let` table over large `switch`-built attribute tables on hot
  paths.

## Lens 7: Concurrency and synchronization

- **The main actor is precious.** Every `await` into `@MainActor` code can hop.
  Parse, query, and aggregate off the main actor; publish one coalesced
  snapshot to the UI.
- Never hold a lock, or block the main actor, across network I/O.
- `TaskGroup` / `withThrowingTaskGroup` for batching independent I/O; avoid a
  `Task {}` per item when a loop inside one task suffices.
- `OSAllocatedUnfairLock` (macOS 13+) over `NSLock` for short critical
  sections, never across an `await`; it also suits plain counters.
- `Sendable` structs are cheap to pass between domains; `@unchecked Sendable`
  only with proven safety.
- Actor reentrancy lets state change between `await`s; snapshot or re-check. A
  reentrancy bug often reads as "lag".
- Reuse one configured `URLSession`; do not create one per request or poll.
- Coalesce `objectWillChange` / observation churn: one notification per update,
  not one per field.

## Lens 8: Serialization and parsing

Schema frameworks are convenient and expensive. Tiers, slowest to fastest:

1. **`Codable` / `JSONDecoder`**: cleanest, slowest per byte. Use for cold or
   rarely called decoding and for stable shapes. Confine each decoder instance
   to one actor or thread.
2. **`JSONSerialization`**: faster for ad hoc `[String: Any]` probing when the
   shape is unstable or must accept several server shapes.
3. **Manual scanning**: fastest when the wire format is unstable but only a few
   fields are needed: `range(of:)` plus index math, `String` slicing, or a small
   state machine over `withUnsafeBytes`. Keep it well tested with fixtures.

Rules:
- Decode once and keep the typed value; never re-encode and re-decode the same
  blob.
- `Data` to `String` to `Data` round trips are pure waste.
- Dates: static `ISO8601DateFormatter` instances for parsing;
  `Date.FormatStyle` / `.formatted()` for display.
- Do not "upgrade" a small, correct hand-rolled parser to a framework without
  evidence; that is often a regression in size and complexity.

## Lens 9: Containers and idioms (Abseil to Swift)

| Abseil / C++ idea | Swift default | Faster or denser alternative |
|---|---|---|
| `std::vector` | `Array` | `reserveCapacity`, `ContiguousArray`, slices |
| `absl::InlinedVector` | `Array` | `ArraySlice`; small `[(K, V)]` linear scan |
| `absl::flat_hash_map` | `Dictionary` | `minimumCapacity`; small `[(K, V)]` for N up to about 8 |
| `std::map` | none | sorted `Array` plus binary search; `OrderedDictionary` |
| bit sets | `Set<SmallEnum>` | `UInt64` bitmask or `[Bool]` |
| `std::deque` | none | `Deque` (swift-collections) |
| arenas | none | `ContiguousArray` of structs plus `Int` indices; `ManagedBuffer` |
| `absl::Span` / `string_view` | `ArraySlice`, `Substring`, `UnsafeBufferPointer` | accept views in APIs |
| faster mutex | `NSLock` | `OSAllocatedUnfairLock` |
| stats counters | lock-guarded `var` | `OSAllocatedUnfairLock<Int>`, atomics |

| Need | Reach for |
|---|---|
| Sliced view | `Substring` (copy when it outlives the parent) |
| Byte scanning | `withUnsafeBytes` plus pointers; `range(of:)` on `Data` |
| Number formatting | `String(n)` or interpolation, never `String(format:)` when hot |
| Regex once | `static let`, or `hasPrefix` / `range(of:)` |

API design:
- **Bulk APIs:** batch decode, batch reads, batch lookups to amortize locking
  and boundary costs.
- **View types:** accept `Substring`, `ArraySlice`, `some Collection` or
  `Sequence` rather than forcing copies.
- **Caller-provided resources:** let callers pass formatters, buffers, and
  scratch space they already own.
- **Default to `Sendable` value types** and confined mutable state.

## Lens 10: SQLite

- [ ] `=` not `LIKE` for exact keys
- [ ] `EXPLAIN QUERY PLAN` on every new or changed query against a realistic
      database
- [ ] Composite indexes that match real predicates; single-column indexes on
      low-selectivity columns are often dead weight
- [ ] Statements prepared once, then `reset` and re-bound in the loop
- [ ] One transaction per write batch; `PRAGMA query_only=ON` on readers
- [ ] `PRAGMA busy_timeout` on every connection (the default of 0 fails with
      `database is locked` while a writer holds the lock)
- [ ] Read-only opens (`SQLITE_OPEN_READONLY`) for readers alongside a WAL
      writer
- [ ] No `DISTINCT` / `GROUP BY` when a bare filter returns one row per key
- [ ] N+1 point queries replaced by one `IN (...)` or one ranged scan
- [ ] One connection per fetch, not one per query
- [ ] No full JSON parse per row: select fields with `json_extract` in SQL
- [ ] Re-read only when the database changes, not once per view or poll
- [ ] Backup before schema changes; `ANALYZE` after

## Lens 11: Polling loops and network ordering

Cadence multiplies waste.

- [ ] Order work cheapest to dearest: settings, throttle, and signed-in gates
      before network I/O; check `Task.isCancelled`
- [ ] Fail fast on auth: a 401 or 403 surfaces re-auth, it does not fall
      through a list of candidate endpoints
- [ ] Keep candidate or fallback probing off the common path, and bound its
      total time
- [ ] Do not pay N× external I/O over a whole queue when one poll advances one
      item
- [ ] Never poll hot against a throttling server: back off exponentially with
      jitter on failure and honor `Retry-After`; make sure real failures are
      reported as failures so the backoff engages
- [ ] Sleep the full interval instead of ticking a timer; park loops that are
      not needed with no timer, and wake them only when an input changes
- [ ] Cache discovery results (ids, endpoints) instead of re-resolving per poll
- [ ] Log a summary per poll; per-item `info` logs × N polls are disk and
      I-cache tax
- [ ] Handle system sleep and wake: pause loops on sleep and resume once the
      network path is satisfied, so a wake does not fail every request at once

## Quick hot-path checklist

1. Per poll, per render, per row, per request? If not, deprioritize.
2. **Structural first:** wrong query plan, N+1 SQL or HTTP, unbounded queue or
   retention, sequential probing, missing auth fail-fast.
3. SQLite: `=` not `LIKE`, matching index, one connection, prepared
   statements, `busy_timeout`, query plan checked, no full JSON per row.
4. Allocation or copy that could be a view or reuse? `reserveCapacity`?
   Formatters or regexes per call? `[UInt8](data)` in a parse loop?
5. A `Set` / `Dictionary` that could replace a linear scan, or a comparator
   that needs a rank map?
6. Main actor: parsing and I/O off it? Change notifications coalesced? Locks
   held over I/O or `await`? A task per item?
7. Rendering: cache key complete (appearance, scale, locale)? Fonts, colors,
   and attributes hoisted? `autoreleasepool` around big Foundation loops?
8. Logging: eager `String(format:)` or `map().joined()` in logger calls?
   Per-item logs × N?
9. Is the predicate that gates the work correct (flags, comparisons, cached
   state)? Check live state, not only CPU.
10. Measured or estimated before recommending Important or higher?

## Report template

```markdown
## Performance review: <scope>

### Summary
- Hot paths identified: ...
- Profile/bench evidence: ... (or "static review only")
- Top opportunities: ...

### Findings

#### [Critical|Important|Minor] <title>
- **Where:** `path/File.swift:LINE`
- **Evidence:** quote, and why it is hot
- **Mechanism:** allocation / COW / cache / algorithm / actor hop / avoided work / ...
- **Impact class:** structural | representation | allocation | avoided work | lock/actor | 1000 cuts
- **Fix:** minimal concrete change (code sketch OK)
- **Validate:** Instruments template / XCTest measure / estimate suffices

### Non-findings / accepted costs
- ... (generality or readability tradeoffs explicitly accepted)

### Suggested bench plan
- ...
```

Push back on premature micro-optimizations that hurt clarity with no hot-path
evidence, and on "optimize everything": prioritize the critical 3%. Record
accepted costs explicitly (for example a bounded cache that already makes a
render cheap, or a small payload where a hand parser beats a framework) so the
next reviewer does not reopen them.

## Anti-patterns

- Claiming a win without a profile, benchmark, or explicit estimate
- Rewriting loops before checking whether the gating predicate or cached state
  is simply wrong
- Micro-optimizing cold paths, tests, or one-shot scripts
- Adding a second (fast) mechanism alongside the old one instead of replacing it
- Caches with incomplete keys or no bound
- Moving work onto the main actor, or holding a lock across `await` or I/O
- Replacing a small, correct hand parser with a heavy framework, or the
  reverse, without measurement
- `@inline(__always)` or `-Ounchecked` sprinkled without evidence
- Sacrificing type safety or readability for an unmeasured gain
- Findings without `file:line`, hot-path classification, or a concrete fix

## Version history

- **2026-11-07**: Created by splitting the "Performance Review (Deep Audits)"
  material and the review report template out of the token_monitor repo's own
  swift-engineer skill, made general for any native macOS Swift repository.
  Repo-specific hot paths and performance field notes stay in that repo's
  skill. Principles and structure adapted from Jeffrey Dean and Sanjay
  Ghemawat, *Performance Hints*, 2025, https://abseil.io/fast/hints.html.
