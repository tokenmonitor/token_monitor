# macOS development guide

How the macOS app in `macos/` is built, tested and reviewed. The module map and data flow are in [ARCHITECTURE.md](ARCHITECTURE.md); the auth model and endpoints are in [../../docs/AUTH_AND_ENDPOINTS.md](../../docs/AUTH_AND_ENDPOINTS.md). Run every command from the repository root as `make -C macos <target>`, or from `macos/` as `make <target>`.

## Stack

- macOS 14+, Swift 5.10, SwiftUI plus AppKit. The app is `LSUIElement` (an agent app, no Dock icon).
- The menu bar is a custom `NSStatusItem` owned by `MenuBarController`. Its label is one AppKit-rendered bitmap (`MenuBarStatusRenderer`), and its dropdown is a borderless `MenuBarPanel` (`NSPanel`), not `MenuBarExtra` or `NSPopover`. The status item keeps `variableLength` and sizes to the bitmap.
- The bitmap is a single `NSImage` composite of per-provider segments plus the hit regions for the same layout, so a status-item click maps back to its provider (`MenuBarController`).
- `macos/project.yml` (XcodeGen) is the only project definition. After adding or moving source files, run `make -C macos project`; a new file is not compiled otherwise.
- App-wide services live on `AppModel` (`@MainActor`). It re-reads observable child state through `ObservationLoop`, which coalesces a burst of changes into one handler call (`AppModel.changeCoalescing`, 100 ms), so a poll that sets many fields triggers one re-render.
- Polling uses `PollingLoop`, one loop per provider:
  - It refreshes at start, then sleeps the full remaining interval: the active interval while the panel is open, the idle interval otherwise (`AppSettings.pollInterval(for:menuIsOpen:)`). There is no periodic tick.
  - `interval()` returning nil parks the loop with no timer. `wake()` re-evaluates it and re-arms from the last refresh, refreshing at once when overdue. `refreshNow()` restarts the wait from the manual refresh.
  - Each refresh returns a `PollOutcome`. Failures back off exponentially (`BackoffTimer`, 30 s to 10 min), a 429 `Retry-After` is honoured, and every wait carries ±10% jitter.
  - `SystemWakeGate` calls `pause()` on sleep and `resume()` once the network is back.
- Auth: `ProviderAuthSession` subclasses with per-provider `WebKitCookieCapture` policies (isolated non-persistent WebKit store, essential-cookie allowlist, three-strike invalidation). Secrets (session cookie headers, the OpenRouter key) live in the login Keychain through `SecretRoutingCredentialStore` and `KeychainCredentialStore`. Email, account identity and workspace id are mode-`0600` files under Application Support. Tests use `InMemoryCredentialStore` or a fake `KeychainBackend`.
- Percent semantics: the menu bar shows used %; the dropdown shows used and remaining; the Grok daily chart shows the 7 days of the active billing period (plus the reset day before its reset instant), deriving day-over-day deltas from local history.

## Layout and conventions

- Tests live in `macos/TokenMonTests/`, never in `macos/TokenMon/` source.
- Types first: define enums, structs and protocols before logic. Prefer value semantics and `let` plus derived values, which makes copy-on-write a non-issue because nothing mutates shared storage. Default to `Sendable` value types and confined mutable state.
- In async and polling code, honour `Task.isCancelled` and handle `CancellationError` (see `PollingLoop`).
- Code under `macos/TokenMon/Core` imports no UI framework (`core_imports_no_ui` in `macos/.swiftlint.yml`). The core test script compiles every file under `macos/TokenMon/Core`; a provider file it needs goes in `PROVIDER_SOURCES` in `macos/scripts/run_core_tests.sh`.

## Testing

- One `XCTestCase` file per unit under test, in the folder that mirrors its source: `macos/TokenMonTests/Core/Polling/`, `macos/TokenMonTests/Providers/Grok/`, `macos/TokenMonTests/Features/Settings/`, and so on.
- Import with `@testable import TokenMon`. Keep internals `internal`; `@testable` reaches them from the test bundle, so nothing is widened to `public` for tests.
- Never add `#if DEBUG` test-only code or inline test modules to source files; production modules stay free of test-only imports and behaviour.
- Use descriptive names (`testWorkspaceIDFromURL`, `testExportCSVNeutralizesFormulaInjection`, `testOrderStatusRejectsInvalidInput`), never `test1`.
- Cover happy paths, error cases, edge cases and boundaries. For enums, test `rawValue` round-tripping and invalid-input rejection (`XCTAssertNil(OrderStatus(rawValue: "nonexistent"))`). Use `XCTAssertEqual(_:_:accuracy:)` for floating point, never exact equality.
- Pure parsers and builders that need no app host go in `macos/Tests/Manual/CoreTestsMain.swift` and run through `macos/scripts/run_core_tests.sh` (`make -C macos test-core`). The ledger keys of `windows/docs/PARITY.md` are these paths, so they do not move.
- Keep tests hermetic: fixture JSON, text or hex (see the gRPC hex fixture in `macos/Tests/Manual/CoreTestsMain.swift`), never live endpoints. A test that must touch a live surface is gated behind an environment flag and skipped by default.
- Never touch real user state. Inject a temporary directory instead of `~/Library/Application Support/TokenMon` or `~/Library/Caches`, a throwaway `UserDefaults(suiteName:)` instead of `.standard`, `InMemoryCredentialStore` or a fake `KeychainBackend` instead of the Keychain, and a stubbed transport instead of the network.

## Quality gate

Run before claiming done or asking to commit. These are the checks CI runs:

1. `make -C macos project` when files were added, moved or removed, and commit the regenerated `macos/TokenMon.xcodeproj`. CI fails when `make project` leaves `git diff` non-empty. It needs the XcodeGen version CI pins (see [CONTRIBUTING.md](../../CONTRIBUTING.md)).
2. `make -C macos lint`: SwiftLint strict; every warning is an error.
3. `make -C macos format`: SwiftFormat lint; no drift.
4. `make -C macos secrets`: gitleaks over the working tree and history.
5. `make -C macos build`, then `make -C macos test` and `make -C macos test-core`.

Every step must pass.

## Review focus

- Check type design before line-by-line logic.
- Check failure modes, boundary cases and test coverage before micro-optimizations.
- Prefer a small refactor that improves type safety over a large stylistic rewrite.
- For menu bar and rendering changes, verify both light and dark appearances, including wallpaper-tinted menu bars.
- For performance questions, classify hot and cold paths, give `file:line` evidence and grade severity instead of guessing.
- Typical work: adding a provider or endpoint handler, making a module more testable, introducing a typed enum or error model, adding a menu bar segment or panel, modelling a new domain concept (usage, billing period, product) as a `Codable` struct or enum, and chasing a slow, laggy or CPU-spinning view.

## Performance

Know which code is hot: per poll, per render, per row and per request. Everything else runs once at launch.

### Hot paths

- **Menu bar render**: `MenuBarStatusRenderer` (`NSCache`-bounded; keep the key complete, keep colours resolved against the live appearance).
- **Polling**: `PollingLoop`, one per provider (see Stack).
- **OpenCode SQLite reads**: `OpenCodeLocalStats` (one read-only connection per scan, `json_extract` in SQL so rows arrive as plain columns, `busy_timeout`, re-read only when the database changes).
- **Grok payload parsing**: `GRPCWebParser` (manual protobuf scan); do not replace it with a framework.

Before claiming done, check these paths for the usual smells: a formatter built per call, full JSON parsing per row, `[UInt8]` copies, eagerly built log strings.

### Patterns in this codebase

- Parsing: when the server shape is unstable, a manual scan beats `JSONDecoder` on hot paths. `GRPCWebParser` is the pattern. Stable shapes and cold paths may use `Codable`.
- Reused expensive objects live in `static let` or a lock-guarded cache: `ISO8601DateFormatter.flexible`, `productEnumMap`, `colorCache`, `Format.cachedFormatter`, `GrokDailyUsageBuilder.makeDateFormatters`.
- Main actor: pollers fetch and parse off the view, and `AppModel` coalesces changes through `ObservationLoop`, so the UI receives one snapshot rather than a notification per field.
- `URLSession` reuse: provider requests share `ProviderURLSession.shared` (ephemeral, no cookie jar, no cache), so one shared connection pool is correct. Do not create a `URLSession` per poll, and do not send provider requests through `URLSession.shared`, whose cookie jar and disk cache outlive sign-out.
- Cache keys: `MenuBarStatusRenderer._cache` is keyed by every input that changes the pixels, including the appearance. A key must encode the time zone for date formatters too.
- `MenuBarStatusRenderer._render` builds `usedAttrs` and `labelAttrs` on every render. They are hoistable, though the `NSCache` already bounds the cost.
- `GrokUsageClient` gates its `debugFieldDump` log behind `#if DEBUG`.

### SQLite (`OpenCodeLocalStats`)

The app reads the OpenCode SQLite database (`~/.local/share/opencode/opencode.db`) read-only through the C API (`SQLITE_OPEN_READONLY`). It re-reads when the file changes, so a scan runs at most once per change rather than once per view.

- [ ] `=`, not `LIKE`, for exact keys; `LIKE` without wildcards still forces a different plan.
- [ ] `EXPLAIN QUERY PLAN` on every new or changed query against a realistic database.
- [ ] Composite indexes match the real predicates; single-column indexes on `year`, `month` and `day` are often dead weight.
- [ ] Prepare statements once, then `reset` and re-bind in the loop.
- [ ] One transaction per batch where you write. This repo only reads, but readers still set `PRAGMA query_only=ON`.
- [ ] `PRAGMA busy_timeout` on every connection (`OpenCodeLocalStats` sets 2000 ms; the default of 0 fails with `database is locked` while a writer holds the lock).
- [ ] A read-only open is correct for readers concurrent with a writer; keep it.
- [ ] No DISTINCT or GROUP BY when a bare filter returns one row per key.
- [ ] N+1 point queries become a bulk `IN (...)` or a single ranged scan.
- [ ] One connection per fetch, not one per query: open once and prepare each statement on it.
- [ ] Do not parse full JSON per row; select the fields with `json_extract` in SQL, as `OpenCodeLocalStats` does, instead of `String(cString:)` plus `JSONSerialization` per row.
- [ ] Back up before schema changes; `ANALYZE` after them.

### Polling loops and network work ordering

The app is poll-driven, so cadence multiplies waste.

- [ ] Order work cheapest to dearest: settings, throttle and `isSignedIn` gates before network I/O. `PollingLoop` checks `Task.isCancelled`; keep that.
- [ ] Fail fast on auth: `401` or `403` surfaces re-auth immediately and does not fall through to other endpoints.
- [ ] Do not pay N times the external I/O over the whole queue when one poll advances one item.
- [ ] Never poll hot against a throttling server. A refresh returns `PollOutcome`; `.failure` grows the wait through `BackoffTimer` (30 s to 10 min), and a 429's `Retry-After` (`UsageError.rateLimited`) sets the earliest next refresh. Report `.failure` for real fetch errors and `.skipped` for cancellation or nothing to do, or the backoff never engages.
- [ ] Do not wake parked loops for nothing. A provider that is not needed returns a nil interval (`AppSettings.pollInterval(for:menuIsOpen:)`) and its loop holds no timer; a change to an input of that interval (menu opened, polling settings edited) calls `wake()`.
- [ ] Cache discovery results: the OpenCode workspace id is stored after sign-in and tried first. Do not re-resolve per poll what rarely changes.
- [ ] Logging: per-item `logger.info` times N polls is a disk and instruction-cache tax. Log a summary once and use `debug` for detail.
- [ ] Sleep and wake: `SystemWakeGate` pauses every loop on system sleep and resumes them once `NWPathMonitor` reports a satisfied path (or after a fallback delay), so a wake does not fail every provider at once.

### Field notes

1. Inspect live state first. A label that looks laggy or frozen is more often a stale cached value or a wrong predicate than slow code.
2. `[UInt8](data)` copies in the protobuf parse loop. `GRPCWebParser` converts `Data` to `[UInt8]` in `dataFrames`, `trailerFields` and `scanProtobuf`. For the small gRPC-web billing payload this is negligible, but the pattern is a trap on a larger payload. Prefer `data.withUnsafeBytes` plus pointer and index math when a parser graduates from toy size to hot size.
3. `firstIndex(of:)` inside sort comparators. `ProductCatalog.sortForDisplay` and `GrokDailyUsageBuilder.finalize` sort with a comparator calling `firstIndex(of:)`, which is O(k² log k) for k products. It is tiny here (at most 6 products), but precompute a `[String: Int]` rank once for any larger list.
4. Eager string building in logging. `GrokUsageClient` builds `productLog` (`products.map { … }.joined(separator:)`) to pass to `logger.info`. `os.Logger` interpolation is evaluated eagerly even when the level discards it, so gate the build behind a level check or `#if DEBUG`.
5. What not to optimize. The menu bar render is `NSCache`-bounded by a full input key, which is the win. The gRPC-web payload is a few KB; a protobuf framework would cost size and complexity, and the hand scan is right. Per-session work (OpenCode workspace discovery, sign-in capture) is not per-poll: cache the result, do not speed the path.

## Rules of this codebase

These override generic best practice when they conflict.

1. **The menu bar is not SwiftUI.** It is an `NSStatusItem` whose label is one AppKit bitmap (`MenuBarStatusRenderer`) and whose dropdown is a borderless `MenuBarPanel`. A status-item click is hit-tested from the status image's x to a provider region, and the event's own location is unreliable for that (`MenuBarController` reads `NSEvent.mouseLocation`). Do not use `MenuBarExtra` or `NSPopover`.
2. **Never hardcode menu bar text colour, and never cache an appearance value.** Resolve `NSColor.labelColor` against the status bar's live `effectiveAppearance` on every render; key cached images by the resolved appearance and clear the cache on `AppleInterfaceThemeChangedNotification`. Account for wallpaper-tinted menu bars, where the status bar appearance differs from the system theme.
3. **A cache key must encode every input that changes the output.** If an input affects the pixels (appearance) or the text (time zone, locale), it is part of the key, or the cache is cleared when it changes.
4. **Credentials go in the Keychain through `SecretRoutingCredentialStore`, never in a file.** Only identifiers that cannot authenticate (email, account identity, workspace id) stay in `0600` files. A refused Keychain write throws `CredentialStoreError.secureStoreRefused` and the sign-in fails; there is no file fallback. A secret found in a legacy `.dat` file moves into the Keychain after a verified write, and the file is deleted either way (a refused move signs the provider out).
5. **Provider requests use `ProviderURLSession.shared`.** It has no cookie jar and no cache; send the stored credential in an explicit header.
6. **`macos/project.yml` is the only source of truth.** A Swift file added without `make -C macos project` never compiles, and CI rejects an uncommitted project diff.
7. **The menu bar label re-renders from observed state.** `MenuBarController` re-reads what the label shows through an `ObservationLoop`, so every input of the label must be an `@Observable` property that the `track` closure reads, or the label goes stale.
8. **Parse failures degrade, never crash; one rejection is not an expiry.** Decode errors are logged and surfaced as bad responses. A 401 or 403 counts toward `ProviderAuthSession.authFailureThreshold` (three in a row) before the session is invalidated, and a bot-protection challenge never counts.
9. **Day keys and dates.** Bucket by `calendar.startOfDay(for:)` with the injected `Calendar`, never `Calendar.current` inside a function that takes one. Month arithmetic counts each cycle from its anchor, not from the previous result.
10. **After a surprising edit success, re-read the file.** An edit that should not have matched may have matched somewhere else.
