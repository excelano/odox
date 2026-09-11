#!/usr/bin/env bash
# The version, read from the one place that has it.
#
# Every script that needs the version calls this, so that a release changes one
# line in one file. The workspace manifest is that file; the crates inherit it.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
grep -m1 '^version' "$here/../Cargo.toml" | sed 's/.*"\(.*\)".*/\1/'
