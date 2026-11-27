#!/bin/sh
# Runs the sign-in probe for both engines: a non-persistent "set" run, then a
# fresh "check" run (the cookie must NOT come along), then the persistent
# control (the cookie MUST come along, proving the check can fail).
# Note: the persistent control stores a throwaway cookie for 127.0.0.1 in the
# default WebKit store of the probe binary; run it only on a dev machine.
set -eu
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
PROBE="$ROOT/macos/crates/tokenmon-signin-probe/target/release/tokenmon-signin-probe"
[ -x "$PROBE" ] || cargo build --release --manifest-path "$ROOT/macos/crates/tokenmon-signin-probe/Cargo.toml"
for engine in wry webkit; do
  echo "=== $engine: non-persistent set ==="
  "$PROBE" "$engine" set
  echo "=== $engine: non-persistent check (fresh process) ==="
  "$PROBE" "$engine" check | grep "server-saw-cookie"
  echo "=== $engine: persistent control set, then check ==="
  "$PROBE" "$engine" set --persistent | grep "server-saw-cookie"
  "$PROBE" "$engine" check --persistent | grep "server-saw-cookie"
done
