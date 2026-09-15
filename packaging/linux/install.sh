#!/usr/bin/env bash
# Put the desktop entries and the icons where a file manager finds them, under
# ~/.local, for a build made from this repository.
#
# The media types themselves are not declared here and must not be: every desktop
# already knows what an OpenDocument file is, from shared-mime-info, and a second
# declaration would only be a second thing to keep right. What is registered is
# that these applications can open one.
#
# `--default` also makes each application the default for the types it opens.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/../.."
# Where cargo actually puts things, asked rather than assumed. `[build]
# target-dir` in a Cargo configuration file moves the target directory and
# `CARGO_TARGET_DIR` is not set when it does, so the fallback below is only
# right on a machine that has not moved it. The Windows and macOS scripts have
# always asked; the Linux ones guessed until 2026-09-14, when a release build
# and the check that reads it disagreed about where the binaries were.
target=$(cargo metadata --format-version 1 --no-deps 2>/dev/null |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
[ -n "$target" ] || target="${CARGO_TARGET_DIR:-$root/target}"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
bin="$HOME/.local/bin"
default=false
[ "${1:-}" = "--default" ] && default=true

mkdir -p "$apps" "$icons" "$bin"

for app in xodt xods xodp; do
    binary="$target/release/$app"
    if [ ! -x "$binary" ]; then
        echo "no $binary — run 'cargo build --release' first" >&2
        exit 1
    fi
    install -m 755 "$binary" "$bin/$app"
    install -m 644 "$here/$app.desktop" "$apps/$app.desktop"
    install -m 644 "$here/icons/$app.svg" "$icons/$app.svg"
    echo "installed $app"
done

update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

if $default; then
    # `xdg-mime` comes from `xdg-utils`, which not every machine has — it is in
    # one of GitHub's two runner images and not the other. Making the default
    # association is a convenience on top of installing, so a machine without the
    # tool is told and the install still stands.
    if ! command -v xdg-mime >/dev/null 2>&1; then
        echo "no xdg-mime — install xdg-utils to set the default association" >&2
        exit 1
    fi
    for app in xodt xods xodp; do
        # The entry already lists the types; this makes it the one that opens them.
        types=$(sed -n 's/^MimeType=//p' "$here/$app.desktop" | tr ';' ' ')
        for type in $types; do
            [ -n "$type" ] && xdg-mime default "$app.desktop" "$type"
        done
    done
    echo "set as the default for the OpenDocument types"
fi

case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "note: $bin is not on PATH" >&2 ;;
esac
