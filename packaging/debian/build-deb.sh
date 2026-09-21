#!/usr/bin/env bash
# Build one .deb per application from a release build.
#
# One script and one control template for four packages, because they differ in
# a handful of strings and in nothing else. Each of the three viewers carries one
# binary, its desktop entry and its icon; the media types are declared by
# shared-mime-info, which every desktop already has, so no package here declares
# one and there is no shared package for them to live in.
#
# The fourth is `odox`, the launcher, and it is the odd one: a command rather
# than an application, so it has no desktop entry, no icon and a manual page of
# its own. **It depends on the three viewers, which makes it the way to install
# the suite**: `apt install odox` brings the whole of odox. Depends rather than
# Recommends, because a recommendation is skipped with `APT::Install-Recommends`
# off or under `apt install --no-install-recommends`, and a launcher whose
# viewers are absent is a command that can only apologise.
#
# Unversioned, because there is no coupling to version: the launcher finds a
# viewer by name and hands the file over, so any version of one works with any
# version of the other.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/../.."
# Where cargo actually puts things, asked rather than assumed. `[build]
# target-dir` in a Cargo configuration file moves the target directory and
# `CARGO_TARGET_DIR` is not set when it does, so the fallback below is only
# right on a machine that has not moved it.
target=$(cargo metadata --format-version 1 --no-deps 2>/dev/null |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
[ -n "$target" ] || target="${CARGO_TARGET_DIR:-$root/target}"
version="$("$here/../version.sh")"
arch="$(dpkg --print-architecture)"
out="$root/dist"

# Written by hand from what `packaging/linux/check-libraries.sh` reports, run
# once on each display backend. The binary links three libraries at build time —
# `objdump -p` says libgcc_s, libm and libc, which is the pure-Rust property
# DESIGN.md §2 keeps — and every other library arrives through a dlopen that no
# manifest and no linker can see. Both display backends are compiled in, so both
# sets are declared: a machine has one or the other and the package cannot know
# which.
#
# `libvulkan1` and not `libgl1`, because the renderer is wgpu and Vulkan is the
# backend it takes on Linux. A running window opens `libvulkan.so.1` and no
# libGL under either display backend. `libwayland-egl1` went with `libgl1`,
# since EGL was there to serve GL.
#
# `libEGL.so.1` is open in a running window on both display backends and is
# still not declared, because it carries a fallback rather than the path this
# is built for. Measured by making each library unreadable in turn and starting
# the window again: without `libEGL.so.1` it draws through Vulkan and the frame
# is unchanged; without `libvulkan.so.1` it drops to GLES over EGL and draws;
# without either it refuses to start, saying
# `FailedToCreateSurfaceForAnyBackend`. A package naming both would claim a
# dependency the application does not have.
#
# Measured on Debian 13 with Mesa. Libraries the closure pulls in behind these
# are not named: `libvulkan1` recommends `mesa-vulkan-drivers | vulkan-icd`
# itself, and naming a driver here would be transcribing another package's
# dependencies and wrong on a machine whose driver is NVIDIA's.
# **Re-measure when eframe moves.**
depends="libc6, libgcc-s1, libvulkan1, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxext6, libxi6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0"

mkdir -p "$out"

describe() {
    case "$1" in
        xodt) echo "OpenDocument text document viewer|Open a .odt document and read it: headings, lists, tables, pictures and the document's own fonts, with an outline beside the page." ;;
        xods) echo "OpenDocument spreadsheet viewer|Open a .ods workbook and read it: every sheet, the values the document holds and the text it displays, with the formula behind the cell you pick." ;;
        xodp) echo "OpenDocument presentation viewer|Open a .odp deck and read it: every slide at the size the document sets, its text where the document puts it, and the speaker's notes." ;;
        odox) echo "open an OpenDocument file with the viewer that reads it|One command for any OpenDocument file. It works out from the file whether it is a text document, a spreadsheet or a presentation, and becomes xodt, xods or xodp accordingly." ;;
    esac
}

for app in xodt xods xodp odox; do
    binary="$target/release/$app"
    [ -x "$binary" ] || { echo "no $binary — run 'cargo build --release' first" >&2; exit 1; }

    staging="$(mktemp -d)"
    trap 'rm -rf "$staging"' EXIT
    # `mktemp` makes a 0700 directory and that mode becomes the package's root.
    chmod 755 "$staging"

    install -Dm755 "$binary" "$staging/usr/bin/$app"
    if [ "$app" != odox ]; then
        install -Dm644 "$here/../linux/$app.desktop" \
            "$staging/usr/share/applications/$app.desktop"
        install -Dm644 "$here/../artwork/$app-application.svg" \
            "$staging/usr/share/icons/hicolor/scalable/apps/$app.svg"
    fi
    install -Dm644 "$root/LICENSE" "$staging/usr/share/doc/$app/copyright"
    # A native package — the version carries no Debian revision — takes
    # `changelog.gz`, and lintian says so on every build until it does.
    install -Dm644 "$here/changelog" "$staging/usr/share/doc/$app/changelog"
    gzip -9n "$staging/usr/share/doc/$app/changelog"

    IFS='|' read -r summary description <<<"$(describe "$app")"
    # The launcher draws nothing, so it needs none of the window libraries, and
    # it suggests the viewers rather than requiring them.
    if [ "$app" = odox ]; then
        app_depends="libc6, libgcc-s1, xodt, xods, xodp"
        recommends=""
        closing="It draws nothing itself: it becomes the viewer that reads the file. Nothing is sent anywhere and nothing is written."
    else
        app_depends="$depends"
        recommends="fonts-liberation"
        closing="The document is drawn from the file and nothing is sent anywhere. Nothing is written either: this release reads OpenDocument and does not save it."
    fi
    # A control file's extended description is one space-prefixed line per line,
    # and lintian refuses one longer than eighty columns.
    wrapped=$(printf '%s\n' "$description" | fold -s -w 76 | sed 's/[[:space:]]*$//; s/^/ /')
    closing_wrapped=$(printf '%s\n' "$closing" | fold -s -w 76 | sed 's/[[:space:]]*$//; s/^/ /')

    install -d "$staging/usr/share/man/man1"
    manual="$here/manual.1.in"
    [ "$app" = odox ] && manual="$here/launcher.1.in"
    sed -e "s|@APP@|$app|g" \
        -e "s|@VERSION@|$version|g" \
        -e "s|@DATE@|$(date -u +%Y-%m-%d)|g" \
        -e "s|@SUMMARY@|$summary|g" \
        -e "s|@DESCRIPTION@|$description|g" \
        "$manual" > "$staging/usr/share/man/man1/$app.1"
    gzip -9n "$staging/usr/share/man/man1/$app.1"
    # A redirect takes the umask, which is 0002 on this machine and 0644 in the
    # package policy.
    chmod 644 "$staging/usr/share/man/man1/$app.1.gz"
    mkdir -p "$staging/DEBIAN"
    printf '%s\n' "$wrapped" > "$staging/DEBIAN/wrapped"
    printf '%s\n' "$closing_wrapped" > "$staging/DEBIAN/closing"
    sed -e "s|@APP@|$app|" \
        -e "s|@VERSION@|$version|" \
        -e "s|@ARCH@|$arch|" \
        -e "s|@DEPENDS@|$app_depends|" \
        -e "s|@SUMMARY@|$summary|" \
        "$here/control.in" > "$staging/DEBIAN/control.raw"
    # An empty `Recommends:` is malformed, so the line goes rather than emptying.
    if [ -n "$recommends" ]; then
        sed "s|@RECOMMENDS_LINE@|Recommends: $recommends|" \
            "$staging/DEBIAN/control.raw" > "$staging/DEBIAN/control.head"
    else
        sed '/@RECOMMENDS_LINE@/d' \
            "$staging/DEBIAN/control.raw" > "$staging/DEBIAN/control.head"
    fi
    rm -f "$staging/DEBIAN/control.raw"
    # The description is substituted separately because it is several lines and
    # sed's replacement is one.
    awk -v body="$staging/DEBIAN/wrapped" -v tail="$staging/DEBIAN/closing" '
        /@DESCRIPTION@/ { while ((getline line < body) > 0) print line; next }
        /@CLOSING@/ { while ((getline line < tail) > 0) print line; next }
        { print }
    ' "$staging/DEBIAN/control.head" > "$staging/DEBIAN/control"
    rm -f "$staging/DEBIAN/control.head" "$staging/DEBIAN/wrapped" \
        "$staging/DEBIAN/closing"

    # `dpkg -V` verifies an installed package against this and reports nothing
    # at all without it; lintian tags its absence. `dpkg-deb --build` does not
    # write one, so it is written here, over everything but the control files.
    ( cd "$staging" && find . -type f ! -path './DEBIAN/*' -printf '%P\0' |
        sort -z | xargs -0 md5sum > DEBIAN/md5sums )
    chmod 644 "$staging/DEBIAN/md5sums"

    dpkg-deb --root-owner-group --build "$staging" \
        "$out/${app}_${version}_${arch}.deb" >/dev/null
    echo "built $out/${app}_${version}_${arch}.deb"

    rm -rf "$staging"
    trap - EXIT
done

if command -v lintian >/dev/null; then
    # `--tag-display-limit 0` and not `--no-tag-display-limit`: lintian 2.122
    # prints a deprecation for the second and takes it anyway, so this would
    # start failing when the machine's lintian moves, for a reason that has
    # nothing to do with the package. The workflow's own call already says this.
    lintian --tag-display-limit 0 "$out"/*_"${version}"_"${arch}".deb || true
fi
