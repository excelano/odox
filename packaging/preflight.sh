#!/usr/bin/env bash
# Everything that has to be true before a release, in the order that finds a
# problem soonest.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/.."
cd "$root"

step() { printf '\n== %s\n' "$1"; }

step "version"
"$here/version.sh"

step "format"
cargo fmt --all --check

step "lint"
cargo clippy --workspace --all-targets -- -D warnings

step "tests"
cargo test --workspace

step "the three build scripts still agree"
# They are byte identical by design and read the binary's name from the
# environment, so there is nothing in them to diverge over and no reason for a
# change to reach one and not the others. DESIGN.md §9.
if [ "$(md5sum crates/*/build.rs | awk '{print $1}' | sort -u | wc -l)" != 1 ]; then
    echo "crates/*/build.rs have drifted apart" >&2
    exit 1
fi

step "the catalogue template is current"
# Catches a string added to the source and never extracted, which is invisible
# otherwise: the window shows English, which is what a working English window
# shows. Only the message set is compared, because the template's creation date
# changes on every run.
if command -v xgettext >/dev/null; then
    before=$(mktemp)
    grep '^msgid ' crates/odox-ui/po/odox.pot | sort > "$before"
    crates/odox-ui/po/update-po.sh >/dev/null 2>&1
    after=$(mktemp)
    grep '^msgid ' crates/odox-ui/po/odox.pot | sort > "$after"
    if ! diff -q "$before" "$after" >/dev/null; then
        echo "the template is behind the source — run crates/odox-ui/po/update-po.sh and commit" >&2
        diff "$before" "$after" >&2 || true
        rm -f "$before" "$after"
        exit 1
    fi
    rm -f "$before" "$after"
else
    echo "skipped: gettext is not installed"
fi

step "the Windows cross-check"
if rustup target list --installed | grep -q x86_64-pc-windows-msvc; then
    cargo check --workspace --target x86_64-pc-windows-msvc
else
    echo "skipped: the msvc target is not installed"
fi

step "release build"
cargo build --release

step "nothing compiled C"
# The check is the artefact and never the manifest: `cargo tree -i cc` is not
# empty in any eframe tree. DESIGN.md §2.
for app in xodt xods xodp; do
    binary="${CARGO_TARGET_DIR:-$root/target}/release/$app"
    needed=$(objdump -p "$binary" | awk '/NEEDED/ {print $2}' | sort | tr '\n' ' ')
    printf '%-6s %s\n' "$app" "$needed"
    case "$needed" in
        "libc.so.6 libgcc_s.so.1 libm.so.6 ") ;;
        *) echo "$app links something new — DESIGN.md §2 and the Debian Depends both want re-measuring" >&2; exit 1 ;;
    esac
done

step "size"
for app in xodt xods xodp; do
    binary="${CARGO_TARGET_DIR:-$root/target}/release/$app"
    printf '%-6s %s\n' "$app" "$(du -h "$binary" | cut -f1)"
done

step "packages"
"$here/debian/build-deb.sh"

printf '\nready\n'
