# Linux support plan (Ubuntu 24.04 LTS)

Target: Ubuntu 24.04 LTS desktop (GNOME, Wayland and X11), x86_64 and arm64. Scope: all seven providers (ChatGPT, Claude, Cursor, Grokbot, OpenCode, OpenRouter, SuperGrok) plus the Overview, alerts, history, and settings. No provider ships later than another.

Status: planned. Nothing in this document is implemented yet. Statements marked **verify** are reasoned from the code and the platform documentation and have not been run on Ubuntu.

## Starting point

| Layer | Size | Linux status |
|-------|------|--------------|
| Foundation-only logic: API clients, parsers, Daily Budget math, polling loop, models | 48 files, about 7.9k of 20k lines | Portable after `FoundationNetworking` imports and a logging shim (verify) |
| SwiftUI | 41 files | Not available; replaced by the Linux shell |
| AppKit, including the status-item renderer and panel | 17 files | Not available; replaced by the Linux shell |
| WebKit sign-in and cookie capture | 7 files | Replaced by WebKitGTK |
| Combine / `ObservableObject` | 22 files | Not available; OpenCombine or a refactor |

Already Linux-shaped: the Claude Code log reader (`~/.claude/projects`, `~/.config/claude`, `CLAUDE_CONFIG_DIR`), the OpenCode database path (`~/.local/share/opencode/opencode.db`), request handling through `ProviderURLSession` with explicit headers, and the `CredentialStore` / `KeychainBackend` protocols.

## Architecture

Two parts in one Swift package (`Package.swift`; `macos/project.yml` stays the macOS app definition):

- **`TokenMonCore`**: provider clients, parsers, pollers, Daily Budget, history, settings model, credential protocols. No AppKit, SwiftUI, WebKit, Security, SwiftData, or Combine imports. Builds on macOS and Linux.
- **Platform layers**: `TokenMonMac` (the existing app) and `TokenMonLinux` (GTK4 shell, tray, WebKitGTK sign-in, libsecret, desktop integration).

Core replacements:

| macOS | Core / Linux replacement |
|-------|--------------------------|
| `os.Logger` | `Logger(category:)` shim in `AppLog.swift` over `os` on macOS and stderr/journald on Linux |
| Combine `@Published` | OpenCombine, or an observation protocol the shells adapt (decided in Phase 1) |
| SwiftData `HistoryStore` | SQLite (system `libsqlite3`, already used by `OpenCodeLocalStats`) behind the same interface, with a one-time import on macOS |
| CryptoKit | swift-crypto |
| `ColorPalette` (`NSColor`/`Color`) | `SRGB` stays in core; conversions move to the shells |
| `Application Support` | `$XDG_DATA_HOME/tokenmon` (fallback `~/.local/share/tokenmon`), directory `0700`, files `0600` |
| OpenCode path | honours `$XDG_DATA_HOME`, falls back to `~/.local/share` |

## Linux platform layer

| Concern | Implementation |
|---------|----------------|
| Credentials | libsecret (Secret Service) store, one vault item matching the macOS `KeychainVault` shape; the existing `0600` file store is the fallback when no Secret Service is running |
| Sign-in | WebKitGTK window per provider, non-persistent data manager, cookies read through the WebKit cookie manager, then the same host match and essential-cookie allowlist as `WebKitCookieCapture` |
| Sign-in fallback | Paste-the-cookie screen per provider, driven by the allowlist table in `AUTH_AND_ENDPOINTS.md` |
| Tray | StatusNotifierItem / AppIndicator (Ubuntu ships the AppIndicator extension), custom icon pixmap for the used-percent bars |
| Panel | GTK4 window opened from the tray menu; placement under the icon is not assumed on Wayland (verify) |
| Notifications | libnotify over the D-Bus notifications interface |
| Launch at login | `~/.config/autostart/tokenmon.desktop` |
| Sleep / wake | logind `PrepareForSleep` over D-Bus, then a connectivity check before polling resumes |
| Updates | `.deb` through a GitHub release asset or apt repository; the in-place `.app` installer is macOS only |

## Phase 0: spikes (decides the plan)

Run on Ubuntu 24.04 with a desktop session and a signed-in account for each provider. Each spike has a pass condition and a recorded fallback.

| # | Spike | Pass | Fallback |
|---|-------|------|----------|
| 0.1 | WebKitGTK sign-in and cookie capture for Grok, Cursor (GitHub and Google identity hosts), OpenCode, Claude, ChatGPT | Preferred session cookie captured for every provider | Paste-the-cookie for the failing provider |
| 0.2 | Live usage call from Linux Foundation for all seven providers, including the ChatGPT session-to-token exchange and Grok gRPC-web billing | Each returns a parsed snapshot | Bot-protection challenges measured; if frequent, evaluate a different HTTP stack for that provider |
| 0.3 | Tray icon with a rendered percentage bitmap on stock Ubuntu 24.04 GNOME | Icon updates live | Static icon plus menu text |
| 0.4 | Panel window behaviour on Wayland and X11 | Usable, positioned predictably | Regular window opened from the tray |
| 0.5 | Swift GTK4 binding choice (maturity, WebKitGTK 6.0 access) | A window with a WebKit view built from Swift | Thin shell in another language over the Swift core |
| 0.6 | Core compile: `FoundationNetworking`, `URLSession.bytes`, `getpwuid_r`, SQLite on Linux | `swift build` of the Foundation-only files | Patch per file |

Gate: every provider has a working sign-in path (embedded or paste) and a working usage call before Phase 3 starts.

## Phases

| Phase | Work | Exit criterion |
|-------|------|----------------|
| 0 | Spikes above | Gate passed, decisions recorded |
| 1 | `Package.swift`, `TokenMonCore` target, shims, SQLite history, move `ColorPalette` conversions to the shells, Ubuntu 24.04 CI job | `swift build` and `swift test` green on Ubuntu 24.04 and macOS; macOS app unchanged in behaviour |
| 2 | libsecret store, XDG paths, autostart, libnotify, logind wake gate | Platform seams covered by tests against fakes, plus a manual run |
| 3 | GTK4 shell: tray, panel per provider, Overview, Daily Budget bars, preferences, sign-in windows | All seven providers show live usage |
| 4 | `.deb` and tarball, release workflow job, Linux README, ARCHITECTURE and AUTH_AND_ENDPOINTS sections | Clean install, sign-in, and polling on a fresh 24.04 VM |

Phases 1 and 2 run alongside Phase 0 once 0.6 passes.

## Tests and CI

- `ci.yml` gains an `ubuntu-24.04` job: `swift build`, `swift test` for `TokenMonCore`.
- `macos/Scripts/run_core_tests.sh` stops using `xcrun` and an Apple target; its source list contains no AppKit file.
- Existing XCTest files for Foundation-only code move to the package test target. UI, WebKit, and Keychain tests stay macOS-only.
- Provider clients get fixture-based tests that run on both platforms. Live-account checks stay manual.

## Packaging

- `.deb` (`tokenmon`, depends on `libgtk-4-1`, `libwebkitgtk-6.0-4`, `libsecret-1-0`, `libayatana-appindicator3-1`, `libsqlite3-0`; exact names verify) and a tarball.
- Snap and Flatpak are not targets: both confine access to hidden home directories, which the Claude Code and OpenCode readers need (verify).
- `release.yml` builds the Linux artifacts on `ubuntu-24.04` next to the macOS build.

## Security requirements

Same as macOS: provider requests carry credentials only in explicit headers, no shared cookie jar, no URL cache, cookies narrowed to the essential allowlist, three-strike session invalidation, secrets never logged. The WebKitGTK data manager is non-persistent and nothing is written to the default WebKit store. The file credential fallback is `0600` in a `0700` directory and is used only when no Secret Service is available.

## Risks

| Risk | Effect | Mitigation |
|------|--------|------------|
| OAuth providers reject the embedded view (Cursor via Google or GitHub) | Sign-in fails for that provider | User-agent and flow tuning in spike 0.1; paste-the-cookie fallback |
| Bot protection challenges Linux requests more often (different TLS fingerprint) | Transient errors on claude.ai or chatgpt.com | Measured in spike 0.2; backoff already treats challenges as transient |
| Wayland cannot place the panel under the tray icon | Different panel UX than macOS | Regular window from the tray menu |
| Immature Swift GTK4 bindings | Shell effort grows | Decided in spike 0.5 |
| No Secret Service in a session | No keyring | File fallback with the existing `0600` store |
| Provider endpoints change (none is a stable public API) | Usage breaks on both platforms | Unchanged: defensive decoding, fixtures |

## Open items

- Linux test machine: a 24.04 VM or box with a desktop session is needed for Phase 0; CI alone cannot exercise sign-in.
- Final product name for package, path, and bundle identifiers (`com.modelmonitor.app` and `ModelMonitor` remain in the macOS code).
