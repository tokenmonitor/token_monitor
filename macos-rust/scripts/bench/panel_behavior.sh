#!/bin/sh
# Checks the panel's focus-loss close and its behavior over a full-screen app.
# Prints the relevant log lines; screenshots go to OUT_DIR (full-screen case).
# Usage: panel_behavior.sh OUT_DIR
set -eu
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
APP="$ROOT/target/rust-beta/TokenMon (Rust beta).app/Contents/MacOS/tokenmon-macos"
OUT=${1:?output directory}
mkdir -p "$OUT"

echo "== focus loss: open at 2s, Finder activated at 4s =="
TOKENMON_LOG=debug "$APP" --mock-openrouter --open-panel-after 2 --quit-after 7 >"$OUT/focus.log" 2>&1 &
sleep 4
osascript -e 'tell application "Finder" to activate'
sleep 4
grep -E "panel (shown|hidden|focus)" "$OUT/focus.log" | sed 's/\x1b\[[0-9;]*m//g' | cut -c28-140

echo "== over a full-screen app: teal full-screen window, panel opened at 7s =="
swift "$ROOT/macos-rust/scripts/bench/fullscreen_host.swift" >/dev/null 2>&1 &
sleep 2
TOKENMON_LOG=debug "$APP" --mock-openrouter --open-panel-after 6 --quit-after 11 >"$OUT/fullscreen.log" 2>&1 &
sleep 9
screencapture -x -R"${PANEL_RECT:-2930,0,460,650}" "$OUT/fullscreen-over.png"
sleep 6
grep -E "panel (shown|hidden|placed)" "$OUT/fullscreen.log" | sed 's/\x1b\[[0-9;]*m//g' | cut -c28-220
