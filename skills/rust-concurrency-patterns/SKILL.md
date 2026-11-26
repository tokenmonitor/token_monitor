---
name: rust-concurrency-patterns
description: >
  Concurrency patterns for Rust services and storage engines: shared core
  rules (lock granularity, no guard across .await, one lock order, choosing a
  lock), storage-engine patterns (sharded maps of per-entity locks, write
  buffering, read/write asymmetry, WAL, throughput reference) and
  async-service patterns (Arc-shared payloads, cursors for exactly-once
  delivery, Notify without lost wakeups, testing races). Use when designing
  or reviewing shared mutable state, locks, async tasks, waiters, or write
  throughput. Load with rust-engineer; measure with
  rust-criterion-benchmarking; audit cost with rust-performance-reviewer.
triggers:
  - concurrent access
  - lock contention
  - lock order
  - deadlock
  - parking_lot
  - RwLock
  - DashMap
  - mutex
  - Notify
  - lost wakeup
  - blocking pull
  - async shared state
  - write throughput
  - write buffering
  - WAL
  - high concurrency
---

# Rust Concurrency Patterns

Patterns for shared mutable state in Rust storage engines and async services: what to lock, how long, in what order, and how to wait.

## When to use

- Adding or changing state that a second task or thread can touch.
- A lock shows up in a profile, or throughput stops scaling with cores.
- Designing a blocking wait (long poll, blocking pull, condition wait).
- Designing a persistent write path where per-write fsync caps throughput.
- Reviewing code with `Mutex`, `RwLock`, `DashMap`, `Notify`, atomics or
  channels.

Part 1 applies everywhere. Part 2 is for storage engines (many entities,
disk writes). Part 3 is for async services (in-memory state, waiters,
delivery guarantees). Many repos need both.

## Part 1: Core rules

### The principle

Lock granularity sets the throughput ceiling; lock discipline sets the
correctness floor. Every shared lock is a serialization point, every pair of
locks is a potential deadlock, and every guard alive at an `.await` is a
stalled worker or a clippy error.

Write the invariants down (in module docs) before choosing locks: what must
be atomic, what may be stale, what must never be lost. The locking scheme
serves those invariants and can change; the invariants cannot.

### Rule 1: No sync guard across `.await`

A `std::sync` or `parking_lot` guard held across an `.await` blocks the
worker thread for as long as the future is parked, and `parking_lot` guards
are `!Send`, so the future cannot be spawned on the multi-thread runtime.
Clippy's `await_holding_lock` rejects both under `-D warnings`. Do the
synchronous work in a block, let the guard drop, then await.

```rust
// Bad: guard alive across the await
let log = conv.log.lock();
let body = render(&log[cursor..]).await;

// Good: copy out what you need (cheap Arc clones), drop the guard, then await
let batch: Vec<Arc<Message>> = {
    let log = conv.log.lock();
    log.since(cursor).cloned().collect()
};
let body = render(&batch).await;
```

Short synchronous sections with `std::sync` or `parking_lot` locks are fine
inside async code; it is holding them across a suspension point that is
wrong. Use a `tokio::sync` lock only when the guard genuinely must live
across an `.await` (for example, an entity whose operations do async I/O and
must be serialized). Never hold any lock across network I/O.

### Rule 2: One documented lock order

When a section takes more than one lock, it takes them in one order,
documented in the module docs, on every path (writes, reads, maintenance,
teardown).

- A `DashMap` shard lock counts as a lock. Holding a `Ref` while locking the
  entity inside it on one path, and the reverse on another, is a deadlock.
  Clone the per-entity `Arc` out of the map, drop the `Ref`, then lock the
  entity.
- Never call code you do not own (callbacks, logging that may block, another
  module's lock) while holding a lock, unless the order names it.
- A change that needs a new order updates the module docs in the same
  commit.

### Rule 3: Keep sections short and panic-free

- Under a lock: the mutation or the copy-out, nothing else. Serialization,
  formatting, logging and I/O happen after release.
- One lock acquisition per batch, not per item.
- `parking_lot` locks do not poison. A panic inside a section leaves state
  however far the section got, so keep sections panic-free: no `unwrap()`,
  no unproven indexing, compute first and mutate last. `std::sync` locks
  poison instead; decide how the code recovers rather than unwrapping the
  `PoisonError` everywhere.

### Rule 4: Choosing a lock

| Need | Use |
|---|---|
| Short sync section, no `.await` inside | `parking_lot::Mutex` / `RwLock` (or `std::sync`) |
| Many readers, rare writers, short sections | `parking_lot::RwLock` |
| Guard must live across `.await` | `tokio::sync::Mutex` / `RwLock` |
| Map of entities looked up by key | `DashMap<K, Arc<Entity>>`, or an outer lock held only to fetch the `Arc` |
| Counter or flag | `AtomicU64` / `AtomicBool`, `Ordering::Relaxed` unless it publishes other data |
| Read-mostly snapshot replaced whole | `arc_swap::ArcSwap` |
| Wake async waiters | `tokio::sync::Notify` |
| Hand work to another task | bounded `tokio::sync::mpsc` |

Lock-free structures only from proven crates (`crossbeam`, `arc-swap`), not
hand-rolled atomics.

## Part 2: Storage engines

For engines that hold many independent entities (chains, tables, logs,
tenants) and write them to disk.

### Pattern: sharded map of per-entity locks

One global `RwLock<State>`, or `Arc<RwLock<HashMap<K, V>>>` held while
working on a value, serializes every caller across every entity. Use a
sharded map whose values are per-entity locks, so work on entity A never
waits for entity B.

```rust
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

struct Service {
    entities: DashMap<EntityId, Arc<RwLock<Entity>>>,
}

impl Service {
    fn get_or_open(&self, id: &EntityId) -> Result<Arc<RwLock<Entity>>, OpenError> {
        if let Some(existing) = self.entities.get(id) {
            return Ok(existing.clone()); // shard read lock held only for the clone
        }
        let entry = self
            .entities
            .entry(id.clone())
            .or_try_insert_with(|| Entity::open(id).map(|e| Arc::new(RwLock::new(e))))?;
        Ok(entry.clone())
    }
}
```

The `tokio::sync::RwLock` here is deliberate: entity operations do async
disk I/O and must be serialized per entity. If they are purely in memory,
use a sync lock (Part 1, Rule 4). Write throughput then scales with the
number of distinct entities written concurrently. Keep this shape once a
repo has it; do not reintroduce a global map lock.

`Entity::open` runs under the shard's write lock; keep it fast, or open
outside the map and insert with `entry`, accepting that a racing opener may
do the work twice.

### Pattern: read/write asymmetry

Most engines read far more than they write.

- Take read locks for the minimum time: get the data, drop the lock, then
  process.
- Do not hold a write lock across slow work. Snapshot under a read lock,
  compute without a lock, re-acquire the write lock to apply, and re-check
  that the state you computed from still holds (a version number or the
  last hash).

```rust
// Bad: write lock held during slow work
let mut guard = entity.write().await;
let result = expensive(&guard).await;
guard.apply(result);

// Good: snapshot, compute, apply with a re-check
let (snapshot, version) = {
    let guard = entity.read().await;
    (guard.snapshot(), guard.version())
};
let result = expensive(&snapshot).await;
let mut guard = entity.write().await;
if guard.version() == version {
    guard.apply(result);
} // else retry or report a conflict
```

### Pattern: write buffering

Writing and flushing on every operation caps throughput at about one write
per disk latency. Buffer writes and flush every N operations, on a timer,
and always on `Drop`.

```rust
struct BufferedWriter {
    file: BufWriter<File>, // keep the file open: no reopen per write
    pending: usize,
    flush_every: usize,    // 1 means flush every write (safe default)
}

impl BufferedWriter {
    fn append(&mut self, record: &[u8]) -> io::Result<()> {
        self.file.write_all(record)?;
        self.pending += 1;
        if self.pending >= self.flush_every {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.get_ref().sync_data()?; // durable, not just out of the process
        self.pending = 0;
        Ok(())
    }
}

impl Drop for BufferedWriter {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            tracing::error!(error = %e, "flush on drop failed; recent writes may be lost");
        }
    }
}
```

- With batching, up to `flush_every - 1` writes are at risk on a hard
  crash. That is an operator decision: expose it as configuration, default
  to the safe setting, and document the risk.
- `BufWriter::flush` only moves bytes to the OS; durability needs
  `sync_data` or `sync_all`. Be explicit about which one a "flush" means.
- `Drop` cannot return an error; log it. Prefer an explicit `close()` that
  returns `Result` on normal shutdown and keep `Drop` as the safety net.
- Never fsync or write files while holding an engine lock that readers
  need; hand the record to a writer task over a bounded channel if the
  write is slow.

### Pattern: write-ahead log

When per-write durability is required and throughput must exceed what
per-write fsync allows:

1. A write appends to an in-memory structure and an append-only WAL file.
2. A background task folds the WAL into the main snapshot every N entries
   or every 10 to 100 ms.
3. Readers see the in-memory state (or merge WAL and snapshot).
4. On open, replay the WAL after the snapshot.

```text
write() --> memory + WAL append   (microseconds)
                  |
         background flush task
                  |
            main snapshot          (durable, up to one window stale)
```

Replay must re-run every integrity check the normal path runs (hash chains,
checksums, sequence numbers), and a torn final record is detected and
truncated, never half-applied. Reach for a WAL only when measurement shows
per-write fsync is the limit.

### Throughput reference

Order of magnitude, per entity, on a local SSD. Measure your own.

| Pattern | Writes/s | At risk on crash |
|---|---|---|
| fsync per write, reopening the file | 250 to 500 | nothing |
| fsync per write, file kept open | 500 to 800 | nothing |
| Batch flush every 16 | 2k to 5k | last 15 writes |
| WAL plus background flush | 10k to 50k | last flush window |
| In memory only | 1M and up | everything |

## Part 3: Async services

For services that keep shared state in memory, hand work between tasks, and
park callers until something happens (mailboxes, queues, buses, long polls).

### Pattern: share payloads with `Arc`, never copy bodies

A payload written once and read by many (recipients, history, views) is
stored as `Arc<T>`, with text as `Arc<str>`, so every reader clones a
pointer.

- Clone `Arc`s under the lock; build JSON, log and do I/O after releasing it.
- `Arc<str>` over `Arc<String>`: one indirection, no capacity slack.
- Never `.clone()` a payload struct or its body on a read path.

### Pattern: cursors for exactly-once delivery

Store data as append-only logs and give each reader a cursor. A read returns
the entries after the cursor and advances it in the same critical section.

- No `.await` between reading the entries and advancing the cursor. A
  cancelled future must leave either both done or neither.
- Two concurrent reads for one reader serialize on that section; the second
  sees the advanced cursor and gets nothing twice.
- If retention compacts a log from the front, cursors must survive it:
  store sequence numbers, not raw `Vec` indexes, or rebase every cursor in
  the same section that compacts.
- Retention takes locks in the same order as everyone else and does bounded
  work per pass, so a sweep never parks writers for long.

### Pattern: `Notify` without lost wakeups

The race: the condition is false, an event lands, the waiter parks, and
nobody wakes it. Register interest *before* checking.

```rust
loop {
    let notified = waiter.notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable(); // registered from here on
    if let Some(batch) = store.take_ready(&waiter)? {
        return Ok(batch);
    }
    if tokio::time::timeout_at(deadline, notified).await.is_err() {
        return Ok(Vec::new()); // timed out empty
    }
    // woken: loop and re-check; a wake is a hint, not a delivery
}
```

Sender side: update state under the lock, release it, then notify. Choose
one per design and say why in a comment:

- `notify_one()` wakes one waiter and stores a permit if none is parked.
- `notify_waiters()` wakes every enabled `Notified` and stores nothing.

Rules:

- Always loop and re-check after a wake; permits and spurious wakes mean
  "woken" is not "ready".
- One `Notify` per waiter identity (per agent, per queue), not one shared
  `Notify` that wakes everyone to find nothing.
- Bound parked waiters per identity and per process; a slow client must not
  pin unbounded futures.
- A wait dropped midway (client hung up) leaves no state behind. If you add
  bookkeeping, settle it in `Drop`.

### Pattern: maps of entities

`Arc<RwLock<HashMap<K, V>>>` held while working on a value serializes every
caller. Either:

- `DashMap<K, Arc<Entity>>`, cloning the `Arc` out of the `Ref` before doing
  anything else, or
- an outer `parking_lot::RwLock<HashMap<K, Arc<Entity>>>` held only long
  enough to look up or insert the `Arc`.

Measure before switching between them (load rust-criterion-benchmarking). At
tens of entities the outer map is rarely the bottleneck; the per-entity
section is.

### Pattern: durability in an in-memory service

If an in-memory service gains a persistent path, keep it off the read hot
path: hand records to a writer task over a bounded channel and use the
Part 2 buffering or WAL patterns there. Never write or fsync while holding
a state lock.

### Testing concurrency

- Run races on a multi-thread runtime:
  `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`.
- Exactly-once: N writers and M concurrent readers for one identity; assert
  the union of what readers got equals what was written, with no
  duplicates.
- Lost wakeup: park a waiter, send, assert it returns well before its
  timeout. Repeat in a loop to shake out interleavings.
- Cancellation: drop a waiter midway, then send; the next read gets the
  data.
- Crash safety (engines): kill or drop without `close()`, reopen, and assert
  integrity holds and only the documented window is lost.
- Assert with deterministic capture (channels, recorded order), never sleeps
  or wall-clock timing.
- For lock-order and memory-ordering bugs in small core types, `loom` can
  explore interleavings exhaustively.

## Common mistakes

- A `std::sync` or `parking_lot` guard, or a `DashMap` `Ref`, alive at an
  `.await`.
- Two locks taken in different orders on two paths, including a map `Ref`
  held while locking its value.
- A global lock on the hot path where per-entity locks fit.
- A write lock held during slow computation or I/O.
- Checking the condition, *then* creating the `Notified`: an event between
  the two is lost until the timeout.
- Treating a wake as a delivery instead of looping to re-check.
- Advancing a cursor in one section and marking delivery in another.
- Building JSON, logging bodies or doing I/O inside a lock section.
- Cloning payload bodies instead of `Arc`s on read paths.
- Buffered writers without a `Drop` flush, or calling `BufWriter::flush`
  "durable".
- WAL replay that skips integrity checks or half-applies a torn record.
- Timing-based assertions in concurrency tests.

## Version history

- **2026-11-07**: Shared general skill. Merged the MentisDB version
  (2026-07-09: DashMap, write buffering, per-entity locks, read/write
  asymmetry, WAL, throughput table) as "Storage engines" and the MentisChat
  version (2026-10-30: no guard across `.await`, lock order, Arc-shared
  payloads, cursors, `Notify` without lost wakeups, maps of entities,
  choosing a lock, testing) as "Async services", with shared core rules.
  Resolved the "always use tokio locks in async code" advice in favour of
  short sync sections plus tokio locks only when a guard must cross
  `.await`; added the re-check on apply, `sync_data` for durability, and
  logged `Drop` flush errors.
