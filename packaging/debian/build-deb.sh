#!/usr/bin/env bash
# Build one .deb per application from a release build.
#
# One script and one control template for three packages, because the three
# differ in four strings and in nothing else. Each package carries one binary,
# its desktop entry and its icon; the media types are declared by
# shared-mime-info, which every desktop already has, so no package here declares
# one and there is no shared package for them to live in.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/../.."
target="${CARGO_TARGET_DIR:-$root/target}"
version="$("$here/../version.sh")"
arch="$(dpkg --print-architecture)"
out="$root/dist"

# Written by hand from what `packaging/linux/check-libraries.sh` reports, run
# once on each display backend. The binary links three libraries at build time —
# `objdump -p` says libgcc_s, libm and libc, which is the pure-Rust property
# DESIGN.md §2 keeps — and every other library arrives through a dlopen that no
# manifest and no linker can see. Both backends are compiled in, so both sets are
# declared: a machine has one or the other and the package cannot know which.
#
# Measured on Debian 13 with Mesa. Libraries the closure pulls in behind these
# are not named: libgl1 depends on its own drivers and naming them here would be
# transcribing another package'"'"'s dependencies. **Re-measure when eframe moves.**
depends="libc6, libgcc-s1, libgl1, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxext6, libxi6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-egl1"

mkdir -p "$out"

describe() {
    case "$1" in
        xodt) echo "OpenDocument text document viewer|Open a .odt document and read it: headings, lists, tables, pictures and the document's own fonts, with an outline beside the page." ;;
        xods) echo "OpenDocument spreadsheet viewer|Open a .ods workbook and read it: every sheet, the values the document holds and the text it displays, with the formula behind the cell you pick." ;;
        xodp) echo "OpenDocument presentation viewer|Open a .odp deck and read it: every slide at the size the document sets, its text where the document puts it, and the speaker's notes." ;;
    esac
}

for app in xodt xods xodp; do
    binary="$target/release/$app"
    [ -x "$binary" ] || { echo "no $binary — run 'cargo build --release' first" >&2; exit 1; }

    staging="$(mktemp -d)"
    trap 'rm -rf "$staging"' EXIT
    # `mktemp` makes a 0700 directory and that mode becomes the package's root.
    chmod 755 "$staging"

    install -Dm755 "$binary" "$staging/usr/bin/$app"
    install -Dm644 "$here/../linux/$app.desktop" \
        "$staging/usr/share/applications/$app.desktop"
    install -Dm644 "$here/../linux/icons/$app.svg" \
        "$staging/usr/share/icons/hicolor/scalable/apps/$app.svg"
    install -Dm644 "$root/LICENSE" "$staging/usr/share/doc/$app/copyright"
    # A native package — the version carries no Debian revision — takes
    # `changelog.gz`, and lintian says so on every build until it does.
    install -Dm644 "$here/changelog" "$staging/usr/share/doc/$app/changelog"
    gzip -9n "$staging/usr/share/doc/$app/changelog"

    IFS='|' read -r summary description <<<"$(describe "$app")"
    # A control file's extended description is one space-prefixed line per line,
    # and lintian refuses one longer than eighty columns.
    wrapped=$(printf '%s\n' "$description" | fold -s -w 76 | sed 's/[[:space:]]*$//; s/^/ /')

    install -d "$staging/usr/share/man/man1"
    sed -e "s|@APP@|$app|g" \
        -e "s|@VERSION@|$version|g" \
        -e "s|@DATE@|$(date -u +%Y-%m-%d)|g" \
        -e "s|@SUMMARY@|$summary|g" \
        -e "s|@DESCRIPTION@|$description|g" \
        "$here/manual.1.in" > "$staging/usr/share/man/man1/$app.1"
    gzip -9n "$staging/usr/share/man/man1/$app.1"
    # A redirect takes the umask, which is 0002 on this machine and 0644 in the
    # package policy.
    chmod 644 "$staging/usr/share/man/man1/$app.1.gz"
    mkdir -p "$staging/DEBIAN"
    printf '%s\n' "$wrapped" > "$staging/DEBIAN/wrapped"
    sed -e "s|@APP@|$app|" \
        -e "s|@VERSION@|$version|" \
        -e "s|@ARCH@|$arch|" \
        -e "s|@DEPENDS@|$depends|" \
        -e "s|@SUMMARY@|$summary|" \
        "$here/control.in" > "$staging/DEBIAN/control.head"
    # The description is substituted separately because it is several lines and
    # sed's replacement is one.
    awk -v file="$staging/DEBIAN/wrapped" '
        /@DESCRIPTION@/ { while ((getline line < file) > 0) print line; next }
        { print }
    ' "$staging/DEBIAN/control.head" > "$staging/DEBIAN/control"
    rm -f "$staging/DEBIAN/control.head" "$staging/DEBIAN/wrapped"

    dpkg-deb --root-owner-group --build "$staging" \
        "$out/${app}_${version}_${arch}.deb" >/dev/null
    echo "built $out/${app}_${version}_${arch}.deb"

    rm -rf "$staging"
    trap - EXIT
done

if command -v lintian >/dev/null; then
    lintian --no-tag-display-limit "$out"/*_"${version}"_"${arch}".deb || true
fi
