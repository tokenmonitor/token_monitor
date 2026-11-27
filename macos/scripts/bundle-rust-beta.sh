#!/bin/sh
# Builds "TokenMon (Rust beta).app" (bundle id com.modelmonitor.app.rust) from the
# tokenmon-macos crate and signs it ad hoc (M0 spike of issue #89).
#
# Usage: macos/scripts/bundle-rust-beta.sh [--features femtovg|skia] [OUT_DIR]
# Output: OUT_DIR/TokenMon (Rust beta).app   (default OUT_DIR: target/rust-beta)
# Signing identity: set CODESIGN_IDENTITY (default "-" = ad hoc).
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FEATURES=""
if [ "${1:-}" = "--features" ]; then FEATURES="--features $2"; shift 2; fi
OUT=${1:-$ROOT/target/rust-beta}
APP="$OUT/TokenMon (Rust beta).app"

cd "$ROOT"
# shellcheck disable=SC2086
cargo build --release --locked -p tokenmon-macos $FEATURES
BIN="$ROOT/target/release/tokenmon-macos"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/tokenmon-macos"
"$BIN" --print-info-plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
codesign --force --sign "${CODESIGN_IDENTITY:--}" --identifier com.modelmonitor.app.rust "$APP"
codesign --verify --strict "$APP"
echo "$APP"
