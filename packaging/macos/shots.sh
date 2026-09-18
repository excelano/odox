#!/bin/sh
# The Mac App Store screenshots, as recipes rather than as prose.
#
# `screenshot.sh` beside this is the driver and knows nothing about Odox. This
# file is the part that is Odox's — which application, which document, and what
# has to happen in the window before the shutter. It is the counterpart of
# `packaging/windows/shots.ps1`, and the two carry the same table in their own
# platform's spellings.
#
#     MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release
#     ./packaging/macos/build-app.sh --all --sign "Apple Development: ..."
#     ./packaging/macos/shots.sh                  # the English set
#     ./packaging/macos/shots.sh --lang de        # and the German one
#     ./packaging/macos/shots.sh --only xodt      # one application's frames
#
# Run it in Terminal at the console. Sizing another application's window goes
# through System Events, which is gated on Accessibility permission, and an ssh
# session cannot be granted it.
#
# WHAT IS PHOTOGRAPHED, WHICH IS NOT THE THING THAT SHIPS
#
# A signed development bundle built from the commit being released. The Store
# build cannot be launched on the machine that made it — the kernel refuses its
# entitlements without a profile covering this Mac, and a Store profile covers
# none — so no screenshot can ever be of the exact artefact that gets uploaded.
# Build both from one commit and say which in `packaging/store-listing.md`.
#
# Something is open in every shot: a screenshot of an empty window is what
# guideline 2.3.3 sends back.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "${here}/../.." && pwd)

only=""
lang=""
bundles="${root}/dist"
outdir=""

usage() {
    sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

refuse() { echo "shots.sh: $1" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --only) only="${2:?--only needs xodt, xods or xodp}"; shift 2 ;;
        --lang) lang="${2:?--lang needs a language tag}"; shift 2 ;;
        --bundles) bundles="${2:?--bundles needs a directory}"; shift 2 ;;
        --outdir) outdir="${2:?--outdir needs a directory}"; shift 2 ;;
        -h|--help) usage 0 ;;
        *) echo "shots.sh: unknown argument $1" >&2; usage 2 ;;
    esac
done

# The language is a tag and the directory it lands in is a locale, and they are
# not the same string. The Windows lane maps them the same way, in
# `take-shots.ps1`, because a frame's directory is half the name the Store files
# it under and the two lanes have to agree.
case "$lang" in
    "") locale="" ;;
    en*) locale="en-US" ;;
    de*) locale="de-DE" ;;
    *) refuse "no locale is known for $lang" ;;
esac

# Where the shots land. Not committed: dist is where every built artefact goes.
[ -n "$outdir" ] || outdir="${root}/dist/screenshots"
[ -z "$locale" ] || outdir="${outdir}/${locale}"

# The three strings each application needs here. `build-app.sh` holds the full
# table and `packaging/windows/shots.ps1` holds the same one in that platform's
# spellings; only these columns are wanted.
#
# A deck whose slides carry pictures is still decoding them when a document of a
# few pages has settled, so xodp waits longer than the driver's default. The
# Windows lane gives it the same eight seconds.
for app in xodt xods xodp; do
    [ -z "$only" ] || [ "$only" = "$app" ] || continue
    case "$app" in
        xodt) product="Odox Text"; document="corpus/libreoffice/text.odt"; settle=5 ;;
        xods) product="Odox Grid"; document="corpus/libreoffice/calc.ods"; settle=5 ;;
        xodp) product="Odox Deck"; document="corpus/libreoffice/growing-liberty.odp"; settle=8 ;;
    esac
    bundle="${bundles}/${product}.app"
    [ -d "$bundle" ] || refuse "no bundle at ${bundle} — build one with 'packaging/macos/build-app.sh ${app} --sign ...'"

    mkdir -p "$outdir"
    out="${outdir}/${app}-01-document.png"

    # No actions yet. These three read a document and do not edit one, so the
    # document on screen is what they do; a frame wanting a pane opened or a
    # slide selected takes a coordinate, and a coordinate is read off a frame
    # of the same size rather than guessed.
    echo "==> ${app}${lang:+ (${lang})}"
    "${here}/screenshot.sh" "$app" \
        --app "$bundle" \
        --document "${root}/${document}" \
        --settle "$settle" \
        ${lang:+--lang "$lang"} \
        --out "$out"
done

echo
find "$outdir" -name '*.png' | sort
