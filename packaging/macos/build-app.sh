#!/bin/sh
# Assemble an application bundle: the executable, the property list that imports
# the OpenDocument type it reads and claims it, and the icon. `Info.plist.in`
# beside this script says what is declared and why; nothing about that is
# repeated here.
#
# The bundle is the unit of everything on macOS. A bare executable can draw a
# window, but it has no bundle identifier, Launch Services files it as a nameless
# foreground process, and nothing can be registered or associated with it.
# `lsappinfo` reports `bundleID=[ NULL ]` for one, which is the whole reason this
# script exists.
#
# It signs the bundle when it is given an identity, because the Mac App Store is
# the chosen channel and an unsigned bundle is not a thing that can be tested:
# the App Sandbox is inert until the entitlement is inside a signature, so an
# unsigned bundle carrying `odox.entitlements` is not sandboxed and proves
# nothing. `README.md` beside this file says which certificate is which.
#
# THREE BUNDLES, ONE SCRIPT
#
# This is slipcase-desktop's script taking an application name. The three viewers
# differ in six strings and in nothing else, so the table below holds the six and
# the rest of the file is what those repositories measured; the comment beside
# each refusal says what it cost there. `--all` builds three bundles in a row,
# which is what a release wants and what `--store` cannot have: a provisioning
# profile covers one bundle identifier.
#
# The launcher gets no bundle. It hands off by replacing itself with the right
# viewer, which macOS has no use for, and it ships only on apt.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "${here}/../.." && pwd)
apps=""
binary_arg=""
outdir="${root}/dist"
# Not on PATH, and README.md says so.
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
universal=no
identity=""
store_profile=""

usage() {
    cat <<'USAGE'
usage: build-app.sh (xodt | xods | xodp | --all) [--binary PATH] [--outdir DIR]
                    [--universal] [--sign ID] [--store PROFILE]

  xodt|xods|xodp the application to bundle. One name, or --all for three.
  --all          build all three bundles, which is what a release wants
  --binary PATH  the executable to bundle (default: the release build)
  --outdir DIR   where to write the bundle (default: ./dist)
  --sign ID      sign the finished bundle with this identity and the sandbox
                 entitlements beside this script. `security find-identity -v
                 -p codesigning` lists what this machine holds. An Apple
                 Development identity is enough to test the sandbox; a Store
                 upload needs Apple Distribution.
  --universal    join the two per-architecture release builds with lipo, for
                 a Store build that has to run on Apple silicon and Intel:

                   MACOSX_DEPLOYMENT_TARGET=11.0 \
                     cargo build --release --target x86_64-apple-darwin
                   MACOSX_DEPLOYMENT_TARGET=11.0 \
                     cargo build --release --target aarch64-apple-darwin
                   ./packaging/macos/build-app.sh --all --universal
  --store PROFILE
                 build what the Mac App Store takes: a universal bundle
                 carrying PROFILE as embedded.provisionprofile, signed for
                 distribution, wrapped by productbuild into the .pkg that is
                 uploaded. Implies --universal, chooses its own identities,
                 and refuses rather than producing something subtly wrong.
                 PROFILE is the .provisionprofile downloaded from the
                 developer portal, and it covers one application:

                   ./packaging/macos/build-app.sh xodt \
                       --store ~/Downloads/Odox_Text_Mac_App_Store.provisionprofile
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        xodt|xods|xodp) apps="${apps} $1"; shift ;;
        --all) apps="xodt xods xodp"; shift ;;
        --binary) binary_arg="${2:?--binary needs a path}"; shift 2 ;;
        --outdir) outdir="${2:?--outdir needs a directory}"; shift 2 ;;
        --universal) universal=yes; shift ;;
        --sign) identity="${2:?--sign needs an identity}"; shift 2 ;;
        --store) store_profile="${2:?--store needs a .provisionprofile}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "build-app.sh: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

[ -n "$apps" ] || {
    echo "build-app.sh: name an application, or pass --all" >&2
    usage >&2
    exit 2
}

# The six strings each application differs in, and the corpus document to open
# afterwards. They are the same six the Windows package substitutes, and the
# reasoning behind the choice of identifiers is in `Info.plist.in`: OASIS
# registered them, macOS has declared them for years, and this suite imports
# them rather than minting its own.
#
# `extension` carries no dot, because a `public.filename-extension` tag does
# not. Windows wants the dot in the same place, which is one of the small ways
# the two manifests are not copies of one another.
table() {
    case "$1" in
        xodt)
            product="Odox Text"
            identifier="com.excelano.xodt"
            extension="odt"
            uti="org.oasis-open.opendocument.text"
            type_name="OpenDocument Text"
            content_type="application/vnd.oasis.opendocument.text"
            sample="corpus/libreoffice/text.odt"
            ;;
        xods)
            product="Odox Grid"
            identifier="com.excelano.xods"
            extension="ods"
            uti="org.oasis-open.opendocument.spreadsheet"
            type_name="OpenDocument Spreadsheet"
            content_type="application/vnd.oasis.opendocument.spreadsheet"
            sample="corpus/libreoffice/calc.ods"
            ;;
        xodp)
            product="Odox Deck"
            identifier="com.excelano.xodp"
            extension="odp"
            uti="org.oasis-open.opendocument.presentation"
            type_name="OpenDocument Presentation"
            content_type="application/vnd.oasis.opendocument.presentation"
            sample="corpus/libreoffice/growing-liberty.odp"
            ;;
        *)
            echo "build-app.sh: no such application: $1" >&2
            exit 2
            ;;
    esac
}

# One trap for everything this script makes, set before the first `mktemp` and
# never re-armed. A second `trap ... EXIT` replaces the first rather than adding
# to it, which is how slipcase-desktop left store temporaries behind.
store_plist=""
store_ents=""
cleanup() {
    [ -z "$store_plist" ] || rm -f "$store_plist"
    [ -z "$store_ents" ] || rm -f "$store_ents"
}
trap cleanup EXIT INT TERM

# Two combinations that cannot mean anything, refused before a build rather than
# after one. A profile is issued for a single bundle identifier, so `--all
# --store` would sign three bundles against one application's profile and be
# refused at upload for two of them; `--binary` names one file, so it cannot
# stand for three.
count=$(printf '%s\n' $apps | wc -l | tr -d ' ')
if [ "$count" -gt 1 ]; then
    [ -z "$store_profile" ] || {
        echo "build-app.sh: a provisioning profile covers one bundle identifier, so --store takes one application" >&2
        exit 2
    }
    [ -z "$binary_arg" ] || {
        echo "build-app.sh: --binary names one executable, so it cannot be used with --all" >&2
        exit 2
    }
fi

# Everything --store needs is checked before anything is built, because the
# failures here are cheap to see now and expensive to see after an upload: a
# profile for the wrong bundle identifier, an expired one, or a certificate this
# machine does not hold all produce a package that assembles perfectly and is
# refused by App Store Connect.
if [ -n "$store_profile" ]; then
    [ -z "$identity" ] || {
        echo "build-app.sh: --store chooses its own identities; drop --sign" >&2
        exit 2
    }
    [ -f "$store_profile" ] || {
        echo "build-app.sh: no provisioning profile at ${store_profile}" >&2
        exit 1
    }
    # A Store binary runs on both architectures or half the machines that bought
    # it cannot run it, so this is not a flag a person should have to remember.
    universal=yes

    # The profile is a CMS-signed property list. Decoding it is also the check
    # that it is one.
    store_plist=$(mktemp -t odox-profile)
    security cms -D -i "$store_profile" > "$store_plist" 2>/dev/null || {
        echo "build-app.sh: ${store_profile} is not a provisioning profile this can read" >&2
        exit 1
    }

    # ISO 8601 rather than PlistBuddy's rendering, which is locale-dependent and
    # would make this check pass or fail by what language the machine is in.
    store_expiry=$(plutil -extract ExpirationDate raw -o - "$store_plist" 2>/dev/null)
    store_expiry_at=$(date -j -u -f "%Y-%m-%dT%H:%M:%SZ" "$store_expiry" +%s 2>/dev/null || echo "")
    [ -n "$store_expiry_at" ] || {
        echo "build-app.sh: cannot read the profile's expiry date (${store_expiry:-none})" >&2
        exit 1
    }
    [ "$store_expiry_at" -gt "$(date +%s)" ] || {
        echo "build-app.sh: the profile expired on ${store_expiry}" >&2
        exit 1
    }

    # The team and the application identifier come out of the profile rather
    # than being written down here. The profile is the thing App Store Connect
    # validates against, so it is the only copy that cannot drift.
    store_app_id=$(/usr/libexec/PlistBuddy -c \
        'Print Entitlements:com.apple.application-identifier' "$store_plist" 2>/dev/null || echo "")
    store_team=$(/usr/libexec/PlistBuddy -c \
        'Print Entitlements:com.apple.developer.team-identifier' "$store_plist" 2>/dev/null || echo "")
    [ -n "$store_app_id" ] && [ -n "$store_team" ] || {
        echo "build-app.sh: the profile carries no application-identifier or team-identifier" >&2
        exit 1
    }
fi

# Cargo is asked where its target directory is. `[build] target-dir` in a Cargo
# configuration file moves it and no environment variable then says so.
target_dir=$(cd "$root" && cargo metadata --format-version 1 --no-deps |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')

# **A private symbol in the binary is a rejection.** Review scans the symbol
# table rather than the call graph, so *unreachable* is not *absent*: `winit`
# references `_CGSSetWindowBackgroundBlurRadius`, which none of these
# applications calls, and fat LTO with `-Wl,-dead_strip` still carries it. The
# workspace manifest's `[patch.crates-io]` is what removes it, and this is what
# notices if it or anything like it comes back.
#
# **The question it asks is a real one rather than a list of names.** A denylist
# of symbols Apple has already rejected somebody for would have caught nothing
# until after the rejection. So: for every undefined symbol the executable
# imports from a system *framework*, does that framework's own public headers
# declare it? That is exactly the line Apple draws, and it was measured against
# a refused binary: `CGShieldingWindowLevel` is in `CGDirectDisplay.h` and is
# fine, while the two `CGS` symbols appear in no header and only in
# `CoreGraphics.tbd`.
#
# Frameworks only. libSystem, libobjc and the rest are the compiler's own
# runtime, emitted rather than named by any source here and declared in no header
# by design; asking about them produced a dozen findings that were all noise. The
# whole `.framework` directory is searched and not just its `Headers`, because
# Carbon and CoreServices are umbrellas whose declarations live in sub-frameworks
# beneath them.
#
# **And the search follows symlinks.** An umbrella's sub-frameworks are not all
# real directories: `winit` links `CGDisplayCreateUUIDFromDisplayID` through
# ApplicationServices, the public header declaring it is ColorSync's, and in
# every SDK on the Mac that framework is a symlink up to the top-level one, which
# a plain `find` does not enter and `find -L` does.
private_symbols() {
    exe="$1"
    sdk=$(xcrun --sdk macosx --show-sdk-path 2>/dev/null) || sdk=""
    # Not being able to ask is not the same as a clean answer, and a check that
    # goes quiet on the machine that lacks a tool is the one that lets a build
    # through. Refuse instead.
    [ -n "$sdk" ] && [ -d "$sdk" ] || {
        echo "build-app.sh: no macOS SDK, so the private-symbol check cannot run" >&2
        echo "  install the Xcode command line tools: xcode-select --install" >&2
        exit 1
    }
    scratch=$(mktemp -d)
    # `nm -m` names the library each undefined symbol is expected to come from,
    # which is what makes the per-framework question askable at all. Both slices
    # of a universal binary are listed, and a symbol in either is a finding.
    nm -mu "$exe" 2>/dev/null |
        sed -n 's/.*(undefined) external _\{0,1\}\([A-Za-z0-9_]*\) (from \([A-Za-z0-9_+]*\)).*/\2 \1/p' |
        sort -u > "${scratch}/pairs"
    # An executable that imports nothing is not a clean answer, it is `nm` having
    # failed to read the file, and an empty list walks through every check below
    # it without a word. Refuse that rather than pass it.
    [ -s "${scratch}/pairs" ] || {
        echo "build-app.sh: nm read no imported symbols from ${exe}" >&2
        echo "  a Mach-O executable always imports some; this is not a pass" >&2
        rm -rf "$scratch"
        exit 1
    }
    : > "${scratch}/flagged"
    for framework in $(cut -d' ' -f1 "${scratch}/pairs" | sort -u); do
        dir="${sdk}/System/Library/Frameworks/${framework}.framework"
        [ -d "$dir" ] || continue
        awk -v f="$framework" '$1 == f { print $2 }' "${scratch}/pairs" |
            sort -u > "${scratch}/wanted"
        find -L "$dir" -name '*.h' -print0 2>/dev/null |
            xargs -0 grep -hoFw -f "${scratch}/wanted" 2>/dev/null |
            sort -u > "${scratch}/declared"
        comm -23 "${scratch}/wanted" "${scratch}/declared" |
            sed "s/^/${framework} /" >> "${scratch}/flagged"
    done
    if [ -s "${scratch}/flagged" ]; then
        echo "build-app.sh: the executable imports symbols no public header declares:" >&2
        sed 's/^/  /' "${scratch}/flagged" >&2
        echo "  App Store review refuses these as Guideline 2.5.1." >&2
        echo "  Find the crate with: grep -rn SYMBOL ~/.cargo/registry/src/*/" >&2
        rm -rf "$scratch"
        exit 1
    fi
    rm -rf "$scratch"
}

# Two numbers, not one, and that is the whole reason `version.sh` takes an
# argument. `CFBundleShortVersionString` is what a person sees in the About box
# and is the release version. `CFBundleVersion` is what App Store Connect
# deduplicates uploads by: it must increase on *every* upload, including a
# rejected one resubmitted with no change, so it cannot be the release version.
version=$("${here}/../version.sh" --short)
build=$("${here}/../version.sh" --build)

# Everything one application needs, from the executable to the finished package.
# Called once per name, so a `--all` release is three passes through this and not
# three scripts that might disagree.
build_bundle() {
    app="$1"
    table "$app"

    # A Store build has to run on both architectures, and Rosetta is not a plan
    # Apple is keeping. `cargo build --release` writes to `release/`; asking for
    # a target explicitly writes to `<triple>/release/`, so the two slices are
    # built separately and joined here. `lipo` is the only step: nothing is
    # compiled by this script.
    binary="$binary_arg"
    if [ "$universal" = yes ]; then
        [ -z "$binary" ] || {
            echo "build-app.sh: --universal builds its own binary; drop --binary" >&2
            exit 2
        }
        slices=""
        for triple in x86_64-apple-darwin aarch64-apple-darwin; do
            slice="${target_dir}/${triple}/release/${app}"
            [ -x "$slice" ] || {
                echo "build-app.sh: no executable at ${slice}; run 'cargo build --release -p ${app} --target ${triple}' first" >&2
                exit 1
            }
            slices="${slices} ${slice}"
        done
        binary="${target_dir}/release/${app}-universal"
        # shellcheck disable=SC2086
        lipo -create ${slices} -output "$binary"
        # A `lipo` that quietly produced one architecture would be a Store upload
        # rejected days later, or worse, accepted and unrunnable on half the
        # machines that bought it. Checked here instead.
        for arch in x86_64 arm64; do
            lipo -info "$binary" | grep -q "$arch" || {
                echo "build-app.sh: the joined executable has no ${arch} slice" >&2
                exit 1
            }
        done
    fi

    if [ -z "$binary" ]; then
        binary="${target_dir}/release/${app}"
    fi
    [ -x "$binary" ] || {
        echo "build-app.sh: no executable at ${binary}; run 'cargo build --release' first" >&2
        exit 1
    }

    private_symbols "$binary"

    bundle="${outdir}/${product}.app"
    rm -rf "$bundle"
    mkdir -p "${bundle}/Contents/MacOS" "${bundle}/Contents/Resources"

    # **The icon is copied and not rendered, and this is the departure from the
    # fleet.** slipcase-desktop, flyleaf, duckling and segler all build the
    # `.icns` here, rasterizing the SVG with `sips` and assembling it with
    # `iconutil`, and both of those tools exist only on a Mac. So in those
    # repositories the icon cannot be seen, checked or corrected until the work
    # reaches that machine, which is the point in a release where time is
    # shortest. `packaging/make-icons` renders all three `.icns` on Linux
    # instead, with the same `icns` library Apple's format is documented by, and
    # commits them beside this script. The whole sizing-and-resampling apparatus
    # those scripts carry, and the pixel-width check that guarded it, are gone
    # with it.
    #
    # What is left to go wrong is a missing or truncated file, so that is what is
    # checked: the four magic bytes every icon family begins with. A bundle whose
    # `.icns` is unreadable draws the generic application icon and says nothing.
    icns="${here}/${app}.icns"
    [ -f "$icns" ] || {
        echo "build-app.sh: no icon at ${icns}; run 'cargo run --manifest-path packaging/make-icons/Cargo.toml'" >&2
        exit 1
    }
    magic=$(dd if="$icns" bs=1 count=4 2>/dev/null)
    [ "$magic" = "icns" ] || {
        echo "build-app.sh: ${icns} does not begin 'icns', so it is not an icon family" >&2
        exit 1
    }
    install -m 0644 "$icns" "${bundle}/Contents/Resources/${app}.icns"

    # `|` as the delimiter because half these values carry a `/` and all of them
    # carry a `.`. None carries a `|`.
    sed -e "s|@EXECUTABLE@|${app}|g" \
        -e "s|@IDENTIFIER@|${identifier}|g" \
        -e "s|@PRODUCT@|${product}|g" \
        -e "s|@ICON@|${app}|g" \
        -e "s|@UTI@|${uti}|g" \
        -e "s|@EXTENSION@|${extension}|g" \
        -e "s|@CONTENT_TYPE@|${content_type}|g" \
        -e "s|@TYPE_NAME@|${type_name}|g" \
        -e "s|@VERSION@|${version}|g" \
        -e "s|@BUILD@|${build}|g" \
        "${here}/Info.plist.in" > "${bundle}/Contents/Info.plist"

    # A placeholder that survived substitution is a bundle that installs and is
    # wrong, so it is looked for rather than assumed away. This catches a
    # placeholder added to the template and not to the list above, which is the
    # realistic way the two part company; `build-msix.ps1` checks its manifest
    # the same way.
    left=$(grep -o '@[A-Z_]\{1,\}@' "${bundle}/Contents/Info.plist" | sort -u || true)
    [ -z "$left" ] || {
        echo "build-app.sh: Info.plist.in has placeholders this script does not substitute:" >&2
        printf '%s\n' "$left" | sed 's/^/  /' >&2
        exit 1
    }

    # A malformed property list is not an error Finder reports; it is a bundle
    # that quietly does not associate. Parsed here so the failure is loud.
    plutil -lint "${bundle}/Contents/Info.plist" >/dev/null

    install -m 0755 "$binary" "${bundle}/Contents/MacOS/${app}"

    # A released bundle's executable has to agree with the floor its property
    # list declares, and Cargo's default does not: measured on slipcase-desktop's
    # first universal build, the x86_64 slice said 10.12 while the bundle said
    # 12.0. Finder would refuse to launch it below the floor and the binary would
    # claim to run there, which is a promise to a person that the bundle then
    # breaks. `MACOSX_DEPLOYMENT_TARGET` is what moves it, and this is the check
    # that catches forgetting to set it.
    #
    # Only for `--universal`, which is the release path. A plain `cargo build
    # --release` for the local test loop is left alone, because failing the
    # everyday bundle over a floor that only matters on somebody else's machine
    # would be theatre.
    if [ "$universal" = yes ]; then
        floor=$(plutil -extract LSMinimumSystemVersion raw "${bundle}/Contents/Info.plist")
        for arch in x86_64 arm64; do
            # Two shapes: a modern build emits LC_BUILD_VERSION with `minos`, and
            # an old enough deployment target emits LC_VERSION_MIN_MACOSX with
            # `version`. Both are read, so this cannot pass by finding neither.
            got=$(otool -arch "$arch" -l "${bundle}/Contents/MacOS/${app}" |
                awk '/LC_BUILD_VERSION|LC_VERSION_MIN_MACOSX/ {want=1; next}
                     want && ($1 == "minos" || $1 == "version") {print $2; exit}')
            [ "$got" = "$floor" ] || {
                echo "build-app.sh: the ${arch} slice was built for ${got:-nothing} and Info.plist declares ${floor}; rebuild with MACOSX_DEPLOYMENT_TARGET=${floor}" >&2
                exit 1
            }
        done
    fi

    if [ -n "$store_profile" ]; then
        build_store
        return 0
    fi

    # Last, so that nothing this script writes lands inside the bundle after it
    # has been sealed. A signature covers what is there when it is made, and
    # adding a file afterwards is how a bundle becomes one macOS reports as
    # damaged.
    if [ -n "$identity" ]; then
        codesign --force --timestamp=none \
            --sign "$identity" \
            --entitlements "${here}/odox.entitlements" \
            "$bundle"
        # A signature that did not carry the entitlements is the failure that
        # costs a day: the bundle launches, behaves exactly as an unsigned one
        # does, and every sandbox measurement made against it is quietly
        # meaningless.
        #
        # The dots in the key are escaped because `plutil -extract` reads an
        # unescaped one as a key path separator, so the plain spelling looks for
        # five nested dictionaries, fails, and reports a correctly signed bundle
        # as unsigned.
        granted=$(codesign -d --entitlements - --xml "$bundle" 2>/dev/null |
            plutil -extract 'com\.apple\.security\.app-sandbox' raw - 2>/dev/null)
        [ "$granted" = true ] || {
            echo "build-app.sh: the signature carries no app-sandbox entitlement" >&2
            exit 1
        }
        echo "signed ${bundle} with ${identity}"
    fi

    echo "built ${bundle} from ${binary}"
    echo
    echo "register it and check that it took:"
    echo "  ${lsregister} -f ${bundle}"
    echo "  ${lsregister} -dump | grep -A8 ${identifier}"
    echo "  open ${sample}"
}

# The Store path. Everything it needs was validated before the build; what is
# left is to put the profile inside the bundle, sign what a submission is signed
# with, and wrap it. One application only, which the argument check above
# enforces, so the names below are that application's.
build_store() {
    # The profile has to match the bundle it goes into. `application-identifier`
    # is `TEAMID.bundle-identifier`, so the tail of it is what Info.plist must
    # say; a profile for a neighbouring identifier signs perfectly and is refused
    # at upload. With three bundle identifiers one letter apart, this is the
    # check most likely to earn its place here.
    bundle_id=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "${bundle}/Contents/Info.plist")
    [ "$store_app_id" = "${store_team}.${bundle_id}" ] || {
        echo "build-app.sh: the profile is for ${store_app_id} and this bundle is ${bundle_id}" >&2
        exit 1
    }

    # One identity or none, never a guess. Two certificates of the same kind in
    # one keychain is an ordinary state, an expiring one beside its replacement,
    # and picking whichever `grep` found first is how a package gets signed with
    # the wrong one.
    #
    # One certificate can be reported several times over. `find-identity`
    # searches every keychain in the search list, and the signing keychain
    # `mac-signing-keychain.sh` makes holds a copy of what the login keychain
    # already had, so a Mac set up to sign over ssh lists each identity at least
    # twice. That is one identity seen twice and not two identities, and the
    # SHA-1 each is listed under is what tells them apart. Counting lines
    # refused every machine that had a signing keychain at all.
    find_identity() {
        matches=$(security find-identity -v 2>/dev/null |
            grep "$1: .*(${store_team})" |
            sed 's/^ *[0-9]*) *\([0-9A-F]*\) *"\(.*\)"$/\1 \2/' |
            sort -u)
        count=$(printf '%s' "$matches" | grep -c . || true)
        [ "$count" = 1 ] || {
            echo "build-app.sh: expected one \"$1\" identity for team ${store_team}, found ${count}" >&2
            [ "$count" = 0 ] || echo "$matches" | sed 's/^/  /' >&2
            return 1
        }
        # The hash was for telling them apart; what signs is the name.
        printf '%s' "${matches#* }"
    }
    app_identity=$(find_identity "Apple Distribution") || exit 1
    # Apple's portal calls this Mac Installer Distribution; the certificate calls
    # itself something else, and the certificate is what `security` reports. It
    # also never appears under `-p codesigning`, because it signs a package
    # rather than code, which is why nothing here filters by that policy.
    pkg_identity=$(find_identity "3rd Party Mac Developer Installer") || exit 1

    # Before the signature, because a signature covers what is in the bundle when
    # it is made and this is part of what gets covered.
    cp "$store_profile" "${bundle}/Contents/embedded.provisionprofile"

    # **Then strip every extended attribute off the bundle, and this is not
    # tidying.** App Store Connect refuses a package containing any file marked
    # `com.apple.quarantine`, ITMS-91109, and a profile is downloaded from the
    # developer portal in a browser, so it arrives marked. macOS `cp` preserves
    # extended attributes, so the mark rides into the bundle, through the
    # signature, through `productbuild`, and past `altool --validate-app`. The
    # upload is then accepted, ingestion rejects it hours later by email, and
    # nothing appears in App Store Connect at all. The profile also carries
    # `kMDItemWhereFroms`, holding the portal
    # URL with the team and profile identifiers in it, which would otherwise ship
    # inside the application, so clearing all of them rather than the quarantine
    # one alone is the fix and not merely the convenient spelling of it.
    xattr -cr "$bundle"

    # The entitlements a Store build is signed with are not the ones a development
    # build is signed with, and this is generated rather than committed so the
    # team identifier has exactly one source: the profile. It is otherwise
    # `odox.entitlements` with two keys added, and that file says why the sandbox
    # grant is read-write.
    #
    # **The file grant is read-only, and it has to match `odox.entitlements`.**
    # This block came in from slipcase-desktop, which edits documents, and asked
    # for read-write; the development build beside it asks for read-only because
    # these applications write nothing. That would have shipped a write grant to
    # review while `packaging/store-listing.md` told the reviewer in writing that
    # there is "no write entitlement of any kind". Two files stating a capability
    # is two places for them to disagree, and this is the one that reaches Apple.
    #
    # `keychain-access-groups` is deliberately absent. The profile grants it and
    # these applications touch no keychain, and a capability asked for and unused
    # is a question at review with no good answer, the same rule
    # `AppxManifest.xml.in` follows about declaring only `runFullTrust`.
    store_ents=$(mktemp -t odox-entitlements)
    cat > "$store_ents" <<ENTITLEMENTS
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.security.app-sandbox</key>
	<true/>
	<key>com.apple.security.files.user-selected.read-only</key>
	<true/>
	<key>com.apple.application-identifier</key>
	<string>${store_app_id}</string>
	<key>com.apple.developer.team-identifier</key>
	<string>${store_team}</string>
</dict>
</plist>
ENTITLEMENTS

    codesign --force --timestamp --options runtime \
        --sign "$app_identity" \
        --entitlements "$store_ents" \
        "$bundle"

    # Read back rather than trusted, for both of them. The sandbox one is the
    # failure that costs a day; the identifier one is the failure that costs an
    # upload, and neither is visible by looking at the bundle.
    granted=$(codesign -d --entitlements - --xml "$bundle" 2>/dev/null |
        plutil -extract 'com\.apple\.security\.app-sandbox' raw - 2>/dev/null)
    [ "$granted" = true ] || {
        echo "build-app.sh: the Store signature carries no app-sandbox entitlement" >&2
        exit 1
    }
    # And refuse if anything is still marked, because the cost of finding out
    # later is an upload, a wait, and an email.
    #
    # `find -exec` rather than `xargs`: xargs answers 123 when the command it ran
    # was false, which is the *normal* case here, and under `set -eu` that kills
    # the substitution and the script with it, silently. The list is then printed
    # through `sed` rather than by word splitting, because these bundle names
    # carry a space and splitting would report each half as a path of its own.
    marked=$(find "$bundle" -type f -exec sh -c \
        'xattr -p com.apple.quarantine "$1" >/dev/null 2>&1 && echo "$1"' _ {} \;)
    [ -z "$marked" ] || {
        echo "build-app.sh: files in the bundle carry com.apple.quarantine, which" >&2
        echo "  App Store Connect refuses as ITMS-91109:" >&2
        printf '%s\n' "$marked" | sed 's/^/  /' >&2
        exit 1
    }

    granted=$(codesign -d --entitlements - --xml "$bundle" 2>/dev/null |
        plutil -extract 'com\.apple\.application-identifier' raw - 2>/dev/null)
    [ "$granted" = "$store_app_id" ] || {
        echo "build-app.sh: the Store signature says application-identifier ${granted:-nothing}, not ${store_app_id}" >&2
        exit 1
    }
    [ -f "${bundle}/Contents/embedded.provisionprofile" ] || {
        echo "build-app.sh: the signed bundle carries no embedded.provisionprofile" >&2
        exit 1
    }
    codesign --verify --deep --strict "$bundle" || {
        echo "build-app.sh: the signed bundle does not verify" >&2
        exit 1
    }
    echo "signed ${bundle} for the Store with ${app_identity}"

    # `--component ... /Applications` is where the Store installs it.
    # `productbuild` rather than `pkgbuild`: the first makes a distribution
    # package, which is what the upload takes, and the second makes a component
    # package, which it does not.
    pkg="${outdir}/${product}.pkg"
    productbuild --component "$bundle" /Applications --sign "$pkg_identity" "$pkg" >/dev/null
    pkgutil --check-signature "$pkg" | sed -n '1,3p'
    echo "built ${pkg} signed with ${pkg_identity}"

    # Launch Services must not know this bundle. It cannot run on this machine:
    # AMFI refuses its restricted entitlements without a profile covering the Mac,
    # and a Store profile covers none (README.md). Launch Services does not ask
    # whether a bundle can launch before choosing it, and among copies of one
    # identifier it prefers the newer version, so a submission build sitting here
    # is a handler candidate that the kernel kills on every double-click, one
    # crash report per attempt and no window. What registers a bundle is a hand-off,
    # `lsregister -f` on a development build at this same path being the usual
    # one, and the claim survives `rm -rf` and a rebuild. This withdraws it.
    #
    # `-u` exits 1 with -10814 when the bundle was never registered, which is the
    # usual state straight after a build, so its status is not the script's.
    "$lsregister" -u "$bundle" >/dev/null 2>&1 || true

    echo
    echo "validate without uploading, then upload; both need the App Store Connect"
    echo "API key in ~/.appstoreconnect/private_keys and its issuer id:"
    echo "  xcrun altool --validate-app -f \"${pkg}\" -t macos --apiKey KEY_ID --apiIssuer ISSUER_ID"
    echo "  xcrun altool --upload-app   -f \"${pkg}\" -t macos --apiKey KEY_ID --apiIssuer ISSUER_ID"
}

first=yes
for one in $apps; do
    [ "$first" = yes ] || echo
    first=no
    build_bundle "$one"
done
