#!/bin/sh
# M0 benchmark of the Rust beta: cold start to status item, idle memory and CPU,
# and panel-open memory and CPU, for one renderer.
#
# Usage: bench_rust.sh [software|femtovg|skia] [RUNS]
# Needs a bundle built by macos-rust/scripts/bundle-rust-beta.sh (with --features
# femtovg or skia for those renderers). Uses the loopback mock, never real
# credentials. Memory is `footprint` (phys_footprint, what Activity Monitor
# shows); CPU is the cputime delta over a fixed window, in percent of one core.
set -eu
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
APP="$ROOT/target/rust-beta/TokenMon (Rust beta).app/Contents/MacOS/tokenmon-macos"
RENDERER=${1:-software}
RUNS=${2:-5}
IDLE_SECONDS=${IDLE_SECONDS:-30}
PROBE=${TMPDIR:-/tmp}/tm_launch_probe
[ -x "$PROBE" ] || swiftc -O "$ROOT/macos-rust/scripts/bench/launch_probe.swift" -o "$PROBE"

footprint_mb() { footprint -p "$1" 2>/dev/null | awk '/Footprint:/ {for (i=1;i<=NF;i++) if ($i=="Footprint:") {v=$(i+1); u=$(i+2)}; if (u ~ /^KB/) v/=1024; else if (u ~ /^GB/) v*=1024; printf "%.1f", v; exit}'; }
cpu_seconds() { ps -o cputime= -p "$1" | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else print $1*3600+$2*60+$3+$4/100 }'; }
cpu_percent() { # pid, window seconds
  a=$(cpu_seconds "$1"); sleep "$2"; b=$(cpu_seconds "$1")
  echo "$a $b $2" | awk '{printf "%.2f", ($2-$1)/$3*100}'
}

export SLINT_BACKEND="winit-$RENDERER"
echo "renderer=$RENDERER runs=$RUNS idle_window=${IDLE_SECONDS}s"
for i in $(seq 1 "$RUNS"); do
  out=$("$PROBE" "$APP" --mock-openrouter)
  pid=$(echo "$out" | sed 's/.*pid=\([0-9]*\).*/\1/')
  ms=$(echo "$out" | sed 's/.*status_item_ms=//')
  sleep 5   # settle: first poll and first status render are done
  idle_mb=$(footprint_mb "$pid")
  idle_cpu=$(cpu_percent "$pid" "$IDLE_SECONDS")
  kill "$pid" 2>/dev/null || true
  echo "run $i: cold_start_ms=$ms idle_footprint_mb=$idle_mb idle_cpu_pct=$idle_cpu"
done

# Panel open: opens at 3 s and stays (focus loss does not close it).
"$APP" --mock-openrouter --open-panel-after 3 --keep-open >/dev/null 2>&1 &
pid=$!
sleep 8
open_mb=$(footprint_mb "$pid")
open_cpu=$(cpu_percent "$pid" "$IDLE_SECONDS")
echo "panel_open: footprint_mb=$open_mb cpu_pct=$open_cpu"
kill "$pid" 2>/dev/null || true
