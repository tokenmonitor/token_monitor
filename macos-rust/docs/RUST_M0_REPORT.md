# Rust macOS client, M0 spike report (issue #89)

Branch `macos-rust-m0`. Code: `macos-rust/crates/tokenmon-macos` (bin + safe library),
`macos-rust/crates/tokenmon-signin-probe` (separate workspace, see 5), scripts in
`macos/scripts/` (`bundle-rust-beta.sh`, `bench/*`). Artifacts in `macos-rust/docs/m0/`.
Host: macOS 27.0 (26A428), Apple Silicon laptop with a notch display plus an
external display, rustc 1.99.1, Slint 1.18.1, winit 0.30.13, objc2 0.6.

## Recommendation: GO (conditional)

Everything the spike set out to prove works, with the shared `common/` crates
unchanged apart from two tiny additive edits. Rust idle numbers are far inside the
1.5x no-go line, but the Swift baseline could only be measured as a Debug build
(see 6), so the memory comparison is indicative, not final. Conditions for M1:
(a) AppKit status-item renderer behind the renderer trait, (b) `wry` for sign-in
with its own workspace/lock decision (3 risks below), (c) add a dir-name parameter
to `AppPaths`, (d) re-run the benchmark against a Release Swift build on a machine
where it may be launched.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Panel route | **winit window tuned through objc2** (no `NSPanel` needed) | Level, collection behavior, focus and focus-loss close all worked (section 2). |
| Status-item renderer | **AppKit/Core Text behind a renderer trait**; keep tiny-skia as the fallback/test renderer | Not indistinguishable from SF text (section 1). |
| Sign-in crate | **`wry`** | Passed all three probe checks (section 5); `objc2-web-kit` also passes but is more code to own. |
| Slint renderer | Software by default | Half the memory of FemtoVG with the same idle CPU (section 6). |

## What was built

1. **Skeleton and bundle.** `tokenmon-macos` is a root-workspace member. On non-macOS
   hosts it compiles to a bin that prints a message; the AppKit glue is under
   `#[cfg(target_os = "macos")]`. Pure logic (bundle identity, credential loader,
   placement math, status layout, tiny-skia renderer, option parsing, loopback mock)
   is in the library with tests in `tests/` (31 tests; the gate below passes).
   `macos-rust/scripts/bundle-rust-beta.sh [--features femtovg|skia]` builds
   `target/rust-beta/TokenMon (Rust beta).app`: bundle id `com.modelmonitor.app.rust`,
   display name "TokenMon (Rust beta)", `LSUIElement`, ad-hoc signed and verified
   with `codesign --verify --strict`. The Info.plist comes from the binary
   (`--print-info-plist`) so identity has one source. Data lives under
   `~/Library/Application Support/TokenMon-RustBeta`. Gap: `AppPaths::directory()`
   appends a fixed `TokenMon` folder, so files land in
   `TokenMon-RustBeta/TokenMon/`; M1 needs a dir-name parameter (or a macOS
   `AppPaths` constructor).
2. **Credentials, never logged.** Only `TOKENMON_RUSTBETA_OPENROUTER_API_KEY` or
   `TOKENMON_RUSTBETA_OPENROUTER_KEY_FILE` are read, never a developer's generic
   `OPENROUTER_API_KEY` (one was set in the shell I ran in; I did not use it). `Debug`
   on the key redacts, errors name the variable or file only. The vault blob is
   sealed with a random per-process key (`EphemeralEncryptor`) so nothing secret is
   written in the clear and no Keychain item is created; a stale blob from an earlier
   run is deleted at start. No Swift data or Keychain item was touched; the app is
   started with `home: None`, so it reads no local Claude/OpenCode data either.
3. **Polling end to end.** OpenRouter (selected tab) through the real shared
   `App::build`, registry, poller and `build_view`. Because I did not use a real
   key, I verified against a loopback mock (`--mock-openrouter`, serves `/key`).
   Result: 1 request, bearer header seen, the panel shows "50% Used / $12.50 of
   $25.00", and the status item shows 50%. To make that possible I added one
   additive hook in `tokenmon-app` (`openrouter_base_url`, loopback-HTTP only,
   env `TOKENMON_OPENROUTER_BASE_URL`, with tests in
   `common/crates/tokenmon-app/tests/test_openrouter_base_url.rs`). **Not
   verified with a real credential.**

## 1. Status item and renderer comparison

Screenshots (menu bar strip, cropped; the "light" and "dark" runs toggled the system
appearance and restored it):

- `macos-rust/docs/m0/screens/menubar-skia-{light,dark}.png`, `menubar-appkit-{light,dark}.png`
- Offscreen side-by-side (top tiny-skia, bottom AppKit, 1x and 2x, light and dark
  backdrops, enlarged): `macos-rust/docs/m0/status/compare-{light,dark}-{1x,2x}.png`
  (raw renders: `status-{skia,appkit}-*.png`).

Findings:

- Both renderers produce the Swift layout (22 pt tall, 16 pt logo, 48x8 pt bar,
  fixed `100%` text slot) with the shared tiny-skia logo glyphs and bars.
- **Text differs.** tiny-skia has no text; I rasterized Inter SemiBold with `ab_glyph`.
  AppKit draws the real system font (SF, monospaced digits, medium, 12.5 pt). SF digits
  are visibly larger and wider, so the slot and the whole image are wider and the
  glyph shapes differ. That is not "indistinguishable", so by the approved rule
  (decision 3) the answer is AppKit behind a renderer trait. Bars and rounded clips
  came out equivalent. Matching SF with tiny-skia would need Core Text anyway.
- AppKit also gives dynamic colors and the system's menu-bar appearance handling for
  free; the tiny-skia image is a static bitmap that must be re-rendered on appearance
  change (the Swift app does the same).
- **Menu bar appearance:** ink color must come from the *status bar window's*
  `effectiveAppearance` (what the Swift renderer walks to). The button's appearance
  and the system light/dark switch are not enough: with the system set to Light and a
  dark wallpaper the menu bar stayed dark and white ink was correct (the first
  version drew black ink on a black bar and the item was invisible). The panel is
  a normal window and follows the app appearance instead.
- **Not measured:** a genuinely light menu bar (needs a light wallpaper; I did not
  change the user's wallpaper), and a tinted menu bar other than this wallpaper tint.
  The "light" PNGs therefore show a dark bar. Pixel-level 1x vs 2x comparison on the
  live menu bar was done only offscreen.

## 2. Panel (route 1: winit window + objc2, chosen)

Screenshots: `macos-rust/docs/m0/screens/panel-{light,dark}.png`, `fullscreen-over.png`.
Script: `macos-rust/scripts/bench/panel_behavior.sh`.

Tuning applied after the window exists: `NSStatusWindowLevel` (25), collection
behavior `CanJoinAllSpaces | FullScreenAuxiliary | IgnoresCycle`,
`hidesOnDeactivate = false`, activate + `makeKeyAndOrderFront`. The app runs as an
accessory (LSUIElement; set at runtime when unbundled).

Works:
- Anchored under the status item, centred, 4 pt gap, clamped to the visible frame;
  the AppKit-to-winit y flip is tested. Placement was right on the notch display
  (39 pt menu bar) and on the primary display.
- The existing Overview/provider switcher and the OpenRouter body render with the
  real Slint components in light and dark.
- Closes on focus loss: activating Finder produced `Focused(false)` and the panel
  hid. Escape closes too.
- **Over a full-screen app:** with a full-screen window in its own Space the panel
  appeared on top of it (`fullscreen-over.png`).

Traps found (all fixed in the spike):
- The native window does not exist until after `show()`, so tuning must run again on
  the next event-loop turn (the first call returned "no NSWindow"); placement too.
- winit reports a spurious `Focused(false)` while the window opens. Closing on that
  hid the panel immediately; closing is armed only after the first `Focused(true)`.
- A status-item click while the panel is open first steals focus (closing it) and
  then fires the action, which would reopen it; a 400 ms guard after focus loss turns
  that click into "close".

Not measured / risks: switching **between Spaces** while open (needs Accessibility
permission to script), multi-display placement beyond the two displays here, and
Mission Control behavior. `CanJoinAllSpaces` should make the panel follow the user but
was not observed. The panel has square corners and no vibrancy (decision 1: add the
native background only if it works; not tried in M0). The `NSPanel` fallback was not
needed, so it was not built.

## 5. Sign-in probe (`macos-rust/scripts/bench/signin_probe.sh`)

Run: a local page sets `tm_session` (HttpOnly) in a non-persistent store; the cookie is
read back; a second process checks it did not persist; a `--persistent` control proves
the check can fail.

| Check | `wry` 0.58 | `objc2-web-kit` |
|---|---|---|
| Returns HttpOnly cookies | yes (`cookies()`: found, `http_only=true`, value matches; `document.cookie` saw nothing) | yes (`getAllCookies`, `isHTTPOnly=true`) |
| Non-persistent store | yes (`with_incognito(true)` -> `nonPersistentDataStore`; fresh process did not send the cookie; persistent control did) | yes (`nonPersistentDataStore`, same result) |
| Navigation events | yes: navigation handler (can veto) + page-load started/finished per URL | yes: `decidePolicy...`, didStart, didFinish |

**`wry` passes all three, so wry** (decision 4). Risks: (1) wry builds on `tao`, which
needs gtk 0.19 on Linux and **cannot share a lockfile with `tray-icon` 0.26 (gtk 0.18)**.
Adding it to the root workspace failed resolution, so the probe has its own
`[workspace]` (listed in `exclude`). M1/Linux need a tray-icon bump, a different tray
crate, or wry without tao. (2) I did not integrate wry into the Slint/winit window
(`build_as_child` from the winit raw handle, or a separate window); only the probe's own
tao window was tested. (3) Persistent-control cookie: the `--persistent` run stored a
throwaway 127.0.0.1 cookie in the probe binary's default WebKit store (1 h Max-Age).
The probe is ~100 lines per engine plus a 60-line server, not 50.

## 6. Benchmarks (`macos-rust/scripts/bench/bench_rust.sh`)

Rust numbers: release build, ad-hoc-signed bundle, mock server, 3 runs, `footprint`
(phys_footprint), CPU = cputime delta over 30 s as percent of one core (10 ms
resolution, so 0.07 means "a few ticks").

| Metric | Rust, software renderer | Rust, FemtoVG |
|---|---|---|
| Cold start to status item (spawn to "status item ready" log), ms | 228, 259, 122 (an earlier set: 526, 196, 310) | 187, 148, 131 |
| Idle footprint | 16 MB | 16 MB |
| Idle CPU | 0.07-0.10 % | 0.07 % |
| Panel open: footprint | 24 MB | 48 MB |
| Panel open: CPU (static panel, 30 s) | 0.07 % | 0.07 % |

Release binary 21 MB (femtovg feature build). Skia renderer: not built (the
`skia` feature exists; it downloads prebuilt Skia and I did not spend M0 time on it).
The software renderer is **not** a bottleneck here: idle CPU is ~0 and the panel cost
is 8 MB; FemtoVG doubles panel memory for no gain on this static panel. Not measured:
CPU while scrolling/animating, a 5K or 1x display, and many-provider panels.

Swift baseline: the only running Swift instance was a **Debug** build from the main
checkout (pid up 2.5 days): footprint 117 MB, 0.00 % CPU over 30 s. A Release Swift
build is installed in /Applications but launching it would read the user's real
Keychain and data, which was off limits, and relaunching the running Debug build
would do the same. So **Swift cold start, Release idle memory and panel-open numbers
are not measured.** Debug builds are not representative; treat 117 MB as an upper
bound. On this evidence Rust is at or below Swift idle memory and CPU, far from the
1.5x no-go line, but the final comparison needs a Release Swift run. "Cold start"
here is to an in-process marker because CoreGraphics does not list status-item
windows on this macOS version; it excludes the time until the item is painted.

## Gaps and risks for M1

- No single-instance lock, Settings window, sign-in UI, notifications, login item,
  update check, VoiceOver annotations, or the quit-the-Swift-app offer in the spike.
- `PanelWindow` (Slint wrapper) is a near copy of the Windows one; move it to
  `common/` in M1. Wiring of `Action::SignIn/OpenSettings/...` is a log line.
- Status-item hit regions per provider (Swift's `Region`s) not built; only whole-item
  click.
- Accessibility of the status item (title/label) not set.
- Panel lacks rounded corners/vibrancy; `NSPanel` fallback untested (not needed).
- `unsafe`: confined to `src/shell/` with SAFETY comments; the crate overrides the
  workspace `unsafe_code = "deny"` (warn).
- Coverage: the `shell/` glue is AppKit and is only exercised manually; per the skill
  I did not add it to `.config/coverage-excludes.txt`. M1 must either move logic out
  of it (the design already does) or agree an exclusion entry with a line budget.
  Coverage floors were not measured in M0.
- Windows/Linux CI: the new crate compiles to an empty bin elsewhere and its tests
  use only portable code; I verified only the macOS host (the Unix-only `/dev/urandom`
  test is `cfg(unix)`). Push the branch and watch the Windows and Linux jobs.
- Fonts: Inter SemiBold (OFL) is copied into the crate for the tiny-skia fallback;
  its license file lives next to the original under `tokenmon-ui/tests/fonts`.

## Reproduce

```sh
macos-rust/scripts/bundle-rust-beta.sh --features femtovg
macos-rust/scripts/bench/bench_rust.sh software 3     # or femtovg
macos-rust/scripts/bench/menubar_screens.sh macos-rust/docs/m0/screens   # toggles system appearance, restores it
macos-rust/scripts/bench/panel_behavior.sh macos-rust/docs/m0/screens
macos-rust/scripts/bench/signin_probe.sh
target/release/tokenmon-macos --render-status macos-rust/docs/m0/status
```
