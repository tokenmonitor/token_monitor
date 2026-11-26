# Composable installer actions

Each action does one job and can be used on its own from any workflow
(`uses: ./.github/actions/<name>`, after `actions/checkout`). Signing and
notarization actions do nothing when their secrets are empty, so unsigned
builds work until the certificates exist.

## macOS (run on a macOS runner)

| Action | Does | Needs |
|---|---|---|
| `macos-import-signing` | Imports the Developer ID `.p12` files into a temporary keychain; `make` auto-detects them | optional certificate inputs |
| `macos-build-app` | Release build, staged as `macos/dist/TokenMon.app` | |
| `macos-notarize` | Notarizes and staples an `.app` or `.pkg` (`macos/Scripts/notarize.sh`) | optional Apple notary inputs |
| `macos-pkg` | `macos/dist/TokenMon-<version>.pkg` from `macos/dist/TokenMon.app` | |

Order: `macos-import-signing`, `macos-build-app`, `macos-notarize` (app),
`macos-pkg`, `macos-notarize` (pkg). The zip and dSYM zip come from
`make -C macos _zip _dsym`. `.github/workflows/macos-installers.yml` wires them together
and `release.yml` calls it.

## Windows (run on a Windows runner, bash shell)

| Action | Does | Needs |
|---|---|---|
| `windows-msi` | Per-user MSI (and an optional upgrade-probe MSI) with pinned, checksummed WiX 3 and cargo-wix | a prior `cargo build --release -p tokenmon-app` |
| `windows-sign` | Authenticode-signs files with `windows/installer/sign.sh` and verifies them | optional PFX inputs |

`windows-release.yml` signs the exe, builds the MSI, then signs the MSI.

## Secrets that turn signing on

Add them as repository secrets; nothing else changes.

| Secret | Used by |
|---|---|
| `MACOS_APP_CERTIFICATE_BASE64`, `MACOS_INSTALLER_CERTIFICATE_BASE64`, `MACOS_CERTIFICATE_PASSWORD` | `macos-import-signing` |
| `MACOS_NOTARY_APPLE_ID`, `MACOS_NOTARY_TEAM_ID`, `MACOS_NOTARY_PASSWORD` | `macos-notarize` |
| `WINDOWS_SIGNING_PFX_BASE64`, `WINDOWS_SIGNING_PFX_PASSWORD` | `windows-sign` |
