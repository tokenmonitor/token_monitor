#!/bin/sh
# Captures the Rust beta's status item (both renderers) and panel with the
# system appearance light and dark, then restores the original appearance.
# Needs Screen Recording permission for the terminal. The capture rectangles are
# for the machine used in the M0 report: pass X,Y,W,H of the menu bar strip and
# of the panel as BAR_RECT and PANEL_RECT to adapt (points, global coordinates).
# Usage: menubar_screens.sh OUT_DIR
set -eu
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
APP="$ROOT/target/rust-beta/TokenMon (Rust beta).app/Contents/MacOS/tokenmon-macos"
OUT=${1:?output directory}
BAR_RECT=${BAR_RECT:-2900,0,500,36}
PANEL_RECT=${PANEL_RECT:-2930,0,460,650}
mkdir -p "$OUT"

original=$(osascript -e 'tell application "System Events" to tell appearance preferences to get dark mode')
setdark() { osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $1"; }
trap 'setdark "$original"' EXIT

for mode in false true; do
  setdark "$mode"; sleep 2
  name=light; [ "$mode" = true ] && name=dark
  for r in skia appkit; do
    "$APP" --mock-openrouter --renderer "$r" --quit-after 5 >/dev/null 2>&1 &
    sleep 3
    screencapture -x -R"$BAR_RECT" "$OUT/menubar-$r-$name.png"
    sleep 3
  done
  "$APP" --mock-openrouter --open-panel-after 2 --keep-open --quit-after 6 >/dev/null 2>&1 &
  sleep 4
  screencapture -x -R"$PANEL_RECT" "$OUT/panel-$name.png"
  sleep 3
done
