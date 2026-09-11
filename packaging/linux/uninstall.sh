#!/usr/bin/env bash
# Undo install.sh.
set -euo pipefail

apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor/scalable/apps"
bin="$HOME/.local/bin"

for app in xodt xods xodp; do
    rm -f "$bin/$app" "$apps/$app.desktop" "$icons/$app.svg"
    echo "removed $app"
done

update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
