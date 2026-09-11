#!/usr/bin/env bash
# What a running window actually links, on both display backends.
#
# Every `Depends:` line in the Debian package is written by hand, so the list has
# to come from a process rather than from a manifest: a library pulled in by a
# feature nobody named is invisible in `Cargo.toml` and present in `/proc/PID/maps`.
# Run it once under Wayland and once under X11, because the backends load
# different libraries and only one of them is loaded on any given run.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/../.."
target="${CARGO_TARGET_DIR:-$root/target}"
app="${1:-xodt}"
document="${2:-$root/corpus/libreoffice/text.odt}"
binary="$target/release/$app"

[ -x "$binary" ] || { echo "no $binary — run 'cargo build --release' first" >&2; exit 1; }

"$binary" "$document" &
pid=$!
sleep 4

if [ ! -r "/proc/$pid/maps" ]; then
    echo "the window did not start" >&2
    kill "$pid" 2>/dev/null || true
    exit 1
fi

echo "# libraries $app has open (backend: ${WAYLAND_DISPLAY:+wayland}${WAYLAND_DISPLAY:-x11})"
awk '/\.so/ {print $NF}' "/proc/$pid/maps" | sort -u | while read -r library; do
    package=$(dpkg -S "$library" 2>/dev/null | cut -d: -f1 | head -1)
    printf '%-60s %s\n' "$library" "${package:-<not from a package>}"
done

kill "$pid" 2>/dev/null || true
