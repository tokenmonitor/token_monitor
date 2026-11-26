# Onboarding tour and Settings consolidation

Status: planned, not started. Needs maintainer approval before any code. Applies to the macOS app (`macos/`) and the Windows port (`windows/`) together; every requirement ships on both in the same PR, with `windows/docs/PARITY.md` rows for every new test.

## Goals

- A first launch shows a short tour: the user picks the providers they use, signs in to them, and sets up the menu bar (tray) and alerts. Nothing else is on until they choose it.
- Settings shrink from four tabs (General, Providers, Accounts, Data) and about ten sections to two tabs, with everything about one provider in one place.

## Requirements

### First run and defaults

- **R1** A fresh install starts with no providers enabled. `enabledProviderIDs` may be empty; the menu then shows only a "Set up TokenMon" entry that reopens the tour.
- **R2** An install that already has saved settings (any existing `enabledProviderIDs` key) never sees the tour and keeps its providers. A new `onboardingCompleted` flag is set for it on first launch of this version.
- **R3** No provider polls, and no network request is made, until the user enables it and signs in.

### Tour

- **R4** The tour is one window with four steps and a step indicator: Welcome, Choose providers, Sign in, Menu bar and alerts. Back and Next work on every step; closing the window at any step keeps what was chosen so far and leaves the tour reachable from the menu.
- **R5** Choose providers lists the seven usage providers with logo, name and a one-line description, each with a toggle, all off. Next is disabled until at least one is on. Grokbot is described as using the Cursor sign-in.
- **R6** Sign in lists only the chosen providers, each with its status (Not signed in, Signed in as <email>) and a Sign in button that opens the existing sign-in window, driven by the existing sign-in model (`b397d8c`). OpenRouter shows the API key field instead. Grokbot shows "Uses your Cursor sign-in". Skipping a provider is allowed; it stays enabled and shows "Sign in" in the menu.
- **R7** Menu bar and alerts offers: show the selected provider only or the composite graph, a graph toggle per chosen provider that supports one, and the usage alert toggle and threshold. Defaults: composite graph on for the first chosen provider, alert off.
- **R8** Finish sets `onboardingCompleted`, closes the window, starts polling the enabled and signed-in providers, and opens the menu once so the user sees the result.
- **R9** The tour can be replayed from Settings (General > "Show welcome tour"); a replay starts from the current choices, not from blank.

### Settings (two tabs)

- **R10** Tab **Providers**: one row per usage provider in the user's order (drag to reorder, as today). Each row: enable toggle, sign-in status with Sign in / Sign out / Re-authenticate, and a disclosure with that provider's options: menu bar graph, alert watch, and provider-specific extras (Grok categories in the menu bar and SuperGrok categories, SuperGrok history export and clear, OpenRouter key).
- **R11** Tab **General**: Startup (launch at login), Updates, Refresh intervals, Usage alert (on/off and threshold; which providers it watches lives on each provider row), Menu bar style (selected provider only vs composite), Show welcome tour, Quit.
- **R12** Every setting reachable today is reachable after the change; none are removed. A test enumerates the `AppSettings` keys (and the Rust settings keys) and asserts each is bound to a control in the new layout. The existing per-tab tests in `macos/TokenMonTests/Features/Settings/PreferencesViewTests.swift` are the inventory of today's controls: they are moved to the new tabs, not deleted, and every control they cover keeps a test.
- **R13** Settings opens on Providers when nothing is signed in, otherwise on General (replaces `SettingsTab.initial`).
- **R14** The privacy note (`AccountsPrivacyNote`) appears on the Providers tab and in the tour's Sign in step, unchanged in wording.

### Quality

- **R15** All tour and Settings text follows the public copy rules: plain, no em dashes.
- **R16** Keyboard: every control in the tour and Settings is reachable by Tab and operable by Space/Return; Escape closes the tour (same as closing the window, R4). VoiceOver (macOS) and UI Automation names (Windows) are set on every control.
- **R17** Unit tests cover R1 to R3, R8, R9, R12 and R13 in both apps; existing settings tests keep passing.

## Recording and verification (no runs on the maintainer's Mac)

- **R18** A UI test drives the full tour on a GitHub-hosted runner: XCUITest on `macos-26` for macOS; for Windows, the Slint screens rendered headlessly on ubuntu (as the existing settings snapshot tests do) plus a `TokenMon.exe --tour-smoke` run on `windows-latest`.
- **R19** Sign-in in these runs uses a local stub page served by the test (no real provider, no real account), and an in-memory credential store. The stub path is compiled only into Debug/test builds; Release builds cannot load it (a test asserts the Release binary has no stub entry point).
- **R20** Each run uploads a screen recording (macOS) and per-step screenshots (both) as workflow artifacts. The results are published as one Artifact page that maps each requirement R1 to R16 to the recording timestamp or screenshot that shows it.

## Delivery

1. This plan, committed to `plans/` and listed in `plans/OPEN_WORK.md`.
2. PR 1: model changes (R1 to R3, R8, R9, R13) with tests, both platforms.
3. PR 2: Settings consolidation (R10 to R12, R14), both platforms.
4. PR 3: the tour UI (R4 to R7, R15, R16), both platforms.
5. PR 4: the recording workflow and stub sign-in (R18 to R20), then the Artifact.

Each PR names the requirements it covers; review checks the code against them. When all are merged and the Artifact is approved, this plan moves to `plans/archive/`.

## Open questions

- Should the first Windows release (`windows-v*`, still open in `windows/docs/IMPLEMENTATION_PLAN.md`) ship with this work or before it?
