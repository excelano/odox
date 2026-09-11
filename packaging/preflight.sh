#!/usr/bin/env bash
# Everything that has to be true before a release, in the order that finds a
# problem soonest.
#
#     ./packaging/preflight.sh          # everything it can settle locally
#     ./packaging/preflight.sh --ci     # and whether GitHub is green on HEAD
#
# It refuses; it does not repair. A check that quietly fixed what it found would
# be a release nobody looked at. `ship` runs it with `--ci`, which is the fleet's
# spelling for *also ask the things that need the network*.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$here/.."
cd "$root"

ask_ci=no
while [ $# -gt 0 ]; do
    case "$1" in
        --ci) ask_ci=yes; shift ;;
        -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "preflight.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done

step() { printf '\n== %s\n' "$1"; }

step "version"
"$here/version.sh"

# Asked before the long build, because a red CI is a reason not to spend four
# minutes linking. Three states and not two: a run still going is neither a pass
# nor a failure, and a release cut while CI is mid-flight is one nobody checked.
step "CI is green on HEAD"
if [ "$ask_ci" = no ]; then
    echo "skipped — pass --ci"
elif ! command -v gh >/dev/null 2>&1; then
    echo "no gh to ask with" >&2
    exit 1
else
    sha=$(git rev-parse HEAD)
    counts=$(gh run list --limit 20 --json headSha,status,conclusion \
        --jq "[.[] | select(.headSha == \"${sha}\")]
              | \"\(length) \(map(select(.status != \"completed\")) | length) \(map(select(.status == \"completed\" and .conclusion != \"success\")) | length)\"" \
        2>/dev/null) || counts=""
    read -r total running red <<<"${counts:-0 0 0}"
    if [ "$total" -eq 0 ]; then
        echo "no runs for ${sha}" >&2; exit 1
    elif [ "$red" -gt 0 ]; then
        echo "${red} of ${total} runs failed" >&2; exit 1
    elif [ "$running" -gt 0 ]; then
        echo "${running} of ${total} runs still going" >&2; exit 1
    fi
    echo "green (${total} runs)"
fi

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
