---
name: swift-engineer
description: >
  House style for writing, reviewing, and fixing native macOS Swift, SwiftUI,
  and AppKit code: value semantics and immutability, strongly typed domains,
  enum design, typed error handling, async/await and main-actor discipline,
  documentation, hermetic XCTest suites, and performance awareness while
  writing. Use whenever Swift code in any native macOS repository is being
  written, modified, or reviewed, including Codable models, enums, @MainActor
  services, SwiftUI views, AppKit drawing, parsers, API clients, and test
  suites. For hot-path audits, Instruments work, and micro-optimization, also
  load swift-performance-reviewer.
triggers:
  - swift
  - swiftui
  - appkit
  - implement swift
  - fix swift bug
  - refactor swift
  - review swift
  - typed enum
  - model a domain
  - codable struct
  - error model
  - add an endpoint handler
  - add a view or panel
  - make this more testable
  - xctest
tags:
  - swift
  - swiftui
  - appkit
  - macos
  - codable
  - concurrency
  - enum
  - error-handling
  - testing
  - code-review
---

# Swift Engineer

Senior native macOS Swift engineering: design types first, let the compiler be
the primary correctness tool, and treat every type and function as a contract.

## Companion skills (load when relevant)

| Skill | When |
|-------|------|
| `swift-performance-reviewer` | Hot-path audits, Instruments profiles, allocation/COW/cache footprint, main-actor hops, Codable/JSON and SQLite cost, laggy SwiftUI or AppKit rendering, "make it faster" |

Do not re-derive the performance review method from memory: open the companion
skill and follow it.

## Non-negotiables

1. **The repo's quality gate passes before you claim done.** Run the same
   checks CI runs (typically lint in strict mode, format check, secret scan,
   build, and every test target). Treat every SwiftLint warning as an error.
   Find the exact commands in the repo's `Makefile`, scripts, CI config, or
   `AGENTS.md`; do not hand off code that fails any step.
2. **Tests live in the test target, never in production source.** No
   `#if DEBUG` test-only code paths and no test helpers compiled into the app.
3. **Every bug fix needs a regression test.** Reproduce the failure first,
   then fix, then confirm the test fails without the fix and passes with it.
4. **The public surface is documented.** Every public or shared type, method,
   and non-obvious behavior gets a `///` doc comment (see Documentation style).
5. **Types are correct by construction.** Enums and small `RawRepresentable`
   types replace free-form strings; invalid states are unrepresentable.
6. **No crash on external input.** Network, file, and user data is untrusted;
   parse failures become typed errors, never `!`, `try!`, or `fatalError`.

## Core philosophy

**Value semantics and immutability are the default. Mutation is the last resort.**

- Prefer `struct` over `class`; model state as immutable values and derive new
  ones rather than mutating in place.
- Prefer `let` over `var`. When mutation is unavoidable, contain it to the
  smallest possible scope and document why.
- Prefer returning new values over modifying existing ones.
- Swift's copy-on-write value types make copy-based updates cheap; use them
  freely.

**Strongly typed domains eliminate entire classes of bugs.**

- Never use raw `String` constants as function parameters, control values,
  discriminators, or config keys.
- Model every domain concept with an `enum` (or small `RawRepresentable` type)
  that encodes valid states at compile time.
- Make enums convertible to and from strings for interoperability (JSON,
  `UserDefaults`, URLs, CLI ids) via `String` raw values and synthesized
  `Codable`.
- This removes stringly typed logic, rejects invalid input at compile time,
  enables exhaustive `switch` (the compiler flags new cases), and makes
  interfaces self-documenting.

**Simplicity and composability over cleverness.**

- Favor **composition over inheritance**: protocols, generics, and plain
  function composition before subclassing. Reserve `class` inheritance for
  genuinely shared behavior and state that composition cannot express cleanly.
- Write short, focused functions and views that do one thing well (typically
  under 30 lines), and compose them into larger behaviors.
- Prefer explicit over implicit: avoid protocol-extension magic or macro
  cleverness unless it genuinely reduces code and improves clarity.
- If a function needs a comment to explain what it does, break it into
  smaller, well-named parts.

## Domain modelling (enum design)

1. **Always conform to** `String, Codable, CaseIterable, Hashable, Sendable` at
   minimum. Most are synthesized with zero code.
2. Use `String` raw values for wire, `UserDefaults`, and network interop; avoid
   integer raw values unless the external protocol demands them.
3. Implement a computed `displayName` (or `CustomStringConvertible.description`)
   for user-facing output. Never store display strings as raw values.
4. For parsing, rely on `init?(rawValue:)` returning `nil` on unknown input;
   introduce a typed parse error only when the caller must distinguish failure
   reasons.
5. Never `switch` on raw strings. Switch on enum cases so the compiler flags
   unhandled cases as the enum grows.
6. Keep raw values stable once serialized anywhere (`UserDefaults` keys, JSON,
   files). Renaming a case is a migration decision.
7. Large, rarely used associated-value payloads go behind `indirect` or a
   reference so the common case stays small.

## Error handling

- Define a typed `enum XError: Error, LocalizedError, CustomStringConvertible`
  per domain. Never throw raw `String` or build `NSError` ad hoc.
- Prefer throwing functions (`func f() throws -> T`) over `Result` plumbing;
  use `Result` when failure is a stored outcome rather than control flow.
- Propagate with `try` / `try await`; do not catch and rethrow in a way that
  strips context.
- Reserve `!`, `try!`, and `fatalError` for provable invariants; use
  `precondition` with a message otherwise.
- Use `guard let` / `guard case` for early exits, `as?` over `as!`, and `try?`
  only when the error is deliberately discardable.
- In async and polling code, check `Task.isCancelled` and handle
  `CancellationError` as a normal exit, not a failure.
- Error text that is logged, persisted, or displayed never embeds raw response
  bodies or credentials; summarize untrusted bodies.

## Design bias (KISS + robust)

- **One mechanism. No speculative fallbacks.** A fallback is a second mechanism
  to test, document, and debug.
- **Design types first:** enums, structs, and protocols before logic, logic
  before tests.
- **Co-locate what is used together:** a type, its validation, and its
  persistence live near each other.
- **The main actor is for UI.** Parse, aggregate, and do I/O off it; publish one
  coalesced snapshot to the view layer. Keep `@MainActor` isolation on the
  types that truly own UI state.
- **Actor reentrancy is real.** State can change between `await`s; snapshot or
  re-check after each suspension.
- **`Sendable` value types cross concurrency domains.** Use
  `@unchecked Sendable` on a `final class` only when you can prove safety, and
  say why in a comment.
- **Comment only what is not obvious:** concurrency, compatibility, and
  surprising invariants.

## Performance quick rules (full skill: companion)

Summary only. Open `swift-performance-reviewer` for the audit method, lenses,
and report template.

1. Know which code is hot (per poll, per render, per row, per request);
   everything else is once-at-launch. Measure before claiming a win.
2. `let` plus value types makes copy-on-write sharing harmless; nobody mutates
   shared storage.
3. Expensive objects are `static let`: `DateFormatter`, `NumberFormatter`,
   `NSRegularExpression`, rank maps. Never construct one per call. Prefer
   `Date.FormatStyle` for display formatting.
4. Never allocate per row, render, or poll what can be hoisted: fonts, colors,
   attribute dictionaries, parse buffers. `reserveCapacity` when counts are
   known.
5. `Set` / `Dictionary` membership over `array.contains`; never call
   `firstIndex(of:)` inside a sort comparator (precompute a rank map).
6. Lean parsing: read only the fields you need. `Codable` for stable shapes and
   cold paths; `JSONSerialization` or a manual scan when the shape is unstable
   or the path is hot. Avoid `[UInt8](data)` copies in parse loops.
7. Bulk over per item: one connection, one prepared statement, one transaction,
   one `Dictionary(grouping:)`, one `reduce(into:)`.
8. `os.Logger` interpolation is evaluated eagerly; do not build expensive
   strings for a line that may be filtered.
9. A cache key encodes every input that changes the output, including
   appearance, locale, and time zone.
10. `Substring` retains its whole parent `String`; copy a slice that outlives
    its source.

## Documentation style

Every public or shared struct, enum, method, and non-obvious behavior has a
doc comment:

```swift
/// Represents the lifecycle state of an order in the fulfillment pipeline.
///
/// Each case encodes a valid state transition target. Invalid transitions
/// are prevented at compile time by the type system.
///
/// - Note: Wire values are stable; do not reorder or rename existing cases.
enum OrderStatus: String, Codable, CaseIterable, Hashable, Sendable {
    case pending
    case confirmed
    case shipped
    case delivered
    case cancelled

    /// Human-readable, user-visible name.
    var displayName: String {
        switch self {
        case .pending: return "Pending"
        case .confirmed: return "Confirmed"
        case .shipped: return "Shipped"
        case .delivered: return "Delivered"
        case .cancelled: return "Cancelled"
        }
    }
}
```

- Document the **why**, not just the **what**.
- Use the standard sections (`- Parameters:`, `- Returns:`, `- Throws:`,
  `- Precondition:`, `- Note:`, `- Warning:`) where they add meaning beyond
  the types; do not restate a type in prose.
- Comments and doc comments state what the code does now, in the present
  tense. Never narrate history ("used to", "previously", "no longer",
  "changed from", "fixed", "removed"); history belongs in commit messages and
  PRs. Remove such comments when you edit the code around them.
- Doc comments on public and shared items are API documentation; inline
  comments still cover only what is not obvious.
- For `async` APIs, document cancellation behavior.
- Document error conditions and edge cases.
- SwiftUI view bodies generally do not need docs; models, services, parsers,
  and shared utilities do.

## Implementation checklist

| Step | Action |
|------|--------|
| 1 | Understand the requirement; ask a focused question if the domain is ambiguous |
| 2 | Read neighboring files for naming, error, concurrency, and layout patterns |
| 3 | Design types first: enums, structs, protocols, error enums |
| 4 | Implement with value semantics and small functions and views; no unnecessary mutation |
| 5 | Prefer extending an existing type over adding a parallel abstraction |
| 6 | Never log secrets, cookies, tokens, or raw response bodies |
| 7 | Document public and shared items with `///` and the standard sections |
| 8 | Add or update hermetic tests in the test target, mirroring the source folder |
| 9 | If the project file is generated (XcodeGen, Tuist), regenerate it after adding, moving, or removing files |
| 10 | Sanity-check hot paths against the performance quick rules; for a real audit, load `swift-performance-reviewer` |
| 11 | Self-review: types documented, tests hermetic, mutation minimized, strings replaced by enums, errors typed |
| 12 | Run the repo's full quality gate; every step passes |

## Testing conventions

- One `XCTestCase` file per unit under test, in a test-target folder that
  mirrors the source folder.
- Use `@testable import <Module>` and keep internals `internal`; do not widen
  visibility to `public` for tests.
- Descriptive names that state the behavior: `testWorkspaceIDFromURL`,
  `testExportCSVNeutralizesFormulaInjection`,
  `testOrderStatusRejectsInvalidInput`. Never `test1`.
- Cover happy paths, error cases, edge cases, and boundary conditions.
- For enums, test `rawValue` round-tripping and invalid-input rejection
  (`XCTAssertNil(OrderStatus(rawValue: "nonexistent"))`).
- Use `XCTAssertEqual(_:_:accuracy:)` for floating point, never exact equality.
- Keep tests hermetic: fixture JSON, text, or hex captured from real payloads,
  never live endpoints. A test that must touch a live surface is gated behind
  an environment flag and skipped by default.
- Never touch real user state: inject a temporary directory instead of
  `~/Library/Application Support` or `~/Library/Caches`, a throwaway
  `UserDefaults(suiteName:)` instead of `.standard`, an in-memory credential
  store or fake Keychain backend instead of the Keychain, and a stubbed
  transport instead of the network.
- Inject `Calendar`, `TimeZone`, `Locale`, and a clock; never depend on the
  machine's current settings or wall-clock timing.
- Pure parsers and builders that need no app host can run in a fast,
  host-free test target or script if the repo provides one.
- Performance claims need numbers (XCTest `measure`, Instruments), not vibes.

## Review lens

1. **Type design first:** are domain concepts enums and value types, with
   invalid states unrepresentable?
2. **Failure modes:** does every external input path fail with a typed error
   instead of crashing? Are errors specific about what failed and why?
3. **Defaults:** what happens when optional parameters, settings, or fields are
   missing?
4. **Concurrency:** is UI state mutated only on the main actor? Is work done
   off it? Can reentrancy between `await`s break an invariant? Is cancellation
   honored?
5. **Regression surface:** is there a test that would have caught this, and is
   it hermetic?
6. **Rendering:** for UI and AppKit drawing changes, verify both light and dark
   appearances (and tinted menu bars where relevant).
7. **Simplicity:** is there a smaller fix that preserves the contract?
8. **Gate:** does the full quality gate pass?

Check failure modes and tests before micro-optimizations. Prefer a small
refactor that improves type safety over a large stylistic rewrite. For
performance questions, use `swift-performance-reviewer` (severity-graded
findings with `file:line` evidence) rather than ad hoc guesses.

## Hard-earned lessons

These override generic best practices when they conflict.

1. **A cache key must encode every input that changes the output.** If it
   affects the pixels (appearance, scale) or the text (time zone, locale), it
   is part of the key, or the cache is cleared when it changes.
2. **Never hardcode AppKit text colors, and never cache an appearance-derived
   value.** Resolve semantic colors such as `NSColor.labelColor` against the
   view's live `effectiveAppearance` at draw time; listen for appearance
   changes. A view's appearance can differ from the system theme (for example
   a wallpaper-tinted menu bar).
3. **Inspect live state before rewriting code.** Something that looks laggy or
   frozen is more often a stale cached value or a wrong predicate than slow
   code.
4. **Parse failures degrade, never crash.** Decode errors are logged and
   surfaced as bad responses. One rejected request is not proof a session
   expired; require a threshold before invalidating credentials.
5. **Credentials go in the Keychain, never in plain files or `UserDefaults`.**
   Only identifiers that cannot authenticate may live in files, with `0600`
   permissions. Migrate a secret out of a file only after a verified Keychain
   write.
6. **Use a dedicated, ephemeral `URLSession` for authenticated requests.**
   `URLSession.shared` has a cookie jar and disk cache that outlive sign-out;
   send stored credentials in explicit headers.
7. **Dates and day keys use the injected `Calendar`.** Bucket by
   `calendar.startOfDay(for:)`, never `Calendar.current` inside a function that
   takes a calendar; count recurring periods from their anchor, not from the
   previous result.
8. **A file on disk is not a file in the build.** With a generated project, a
   new Swift file compiles only after regeneration; commit the regenerated
   project if the repo tracks it.
9. **Published state must reach the observer that renders it.** When child
   services hold state that a parent view observes, forward their change
   notifications (and coalesce them) or the UI goes stale.
10. **After a surprising edit success, re-read the file.** An edit that
    "should not have matched" may have matched somewhere else.
11. **Verify claims before asserting them.** Assumptions about external APIs,
    OS behavior, and framework internals die on live verification.

## Anti-patterns

- Stringly typed states, keys, or discriminators where an enum fits
- `switch` on raw strings
- `class` and inheritance where a `struct` or protocol composition would do
- `!`, `try!`, `as!`, or `fatalError` on external input
- Throwing `String`s or ad hoc `NSError`s
- Test-only code in production source, or `#if DEBUG` test hooks
- Tests that hit live endpoints, the real Keychain, `UserDefaults.standard`,
  or real Application Support directories
- Widening visibility to `public` just for tests
- Parsing, I/O, or aggregation on the main actor
- `DateFormatter` / `NSRegularExpression` constructed per call on a hot path
- Hardcoded colors or cached appearance values in AppKit drawing
- Cache keys missing an input that changes the output
- Speculative fallbacks: a second mechanism nobody asked for
- Drive-by refactors unrelated to the task
- Claiming a performance win without a measurement

## Project notes

This skill is general. Before working in a specific repository:

1. Read the repo's `AGENTS.md`, `README`, and any project spec or docs folder.
2. Read any skill that lives in the current repo itself. It carries the project's architecture, build and quality-gate
   commands, test layout, hot paths, and its own lessons.
3. Where the repo's rules are stricter or more specific than this skill, the
   repo wins.

## Version history

- **2026-11-07**: Split from the token_monitor repo's own swift-engineer
  skill into a shared, general skill. Kept the house
  style (value semantics, typed domains, enum design, error handling,
  documentation, testing, review focus, quick performance rules, general
  lessons). Moved the deep performance review material to the companion
  `swift-performance-reviewer`. Left project specifics (architecture, build
  commands, providers, menu bar rendering, polling, project generation details,
  repo lessons and field notes) in the repo's own skill.
