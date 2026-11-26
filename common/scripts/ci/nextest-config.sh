#!/usr/bin/env bash
# Writes the nextest config CI runs with and prints its path.
#
#   nextest-config.sh <workspace-dir> <out-file>
#
# The result is .config/nextest.toml (when it exists) followed by
# nextest-ci.toml, which adds the `ws-ci` profile with JUnit output. Pass it
# to nextest with `--config-file <out-file> --profile ws-ci`.
set -euo pipefail

workspace_dir=${1:?usage: nextest-config.sh <workspace-dir> <out-file>}
out=${2:?usage: nextest-config.sh <workspace-dir> <out-file>}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

{
  if [ -f "$workspace_dir/.config/nextest.toml" ]; then
    cat "$workspace_dir/.config/nextest.toml"
    echo
  fi
  cat "$here/nextest-ci.toml"
} > "$out"

echo "$out"
