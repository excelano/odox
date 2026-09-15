#!/bin/sh
# Ask an *installed* Odox what it actually is, on the machine it is on.
#
# `build-app.sh --store` checks what it built, on the machine that built it.
# That is a different question from this one, and the gap between them is where
# a submission goes wrong: the artefact is carried to another machine, macOS
# decides something about it there, and nothing in the build says what.
#
#     ./check-install.sh xodt               # /Applications/Odox Text.app
#     ./check-install.sh --all              # the three, in one pass
#     ./check-install.sh xods --in ./dist   # a bundle that is not installed yet
#
# It reports; it does not repair, and it does not stop at the first bad answer,
# since an hour on a borrowed machine is the wrong place to learn one thing per
# run. Its exit status is 0 whatever it finds, for the same reason: the findings
# are for a person to weigh against what the bundle was built for, and a script
# that gated on them would be one more thing to argue with at the end of a
# release. What no command can settle needs eyes.
#
# THREE BUNDLES, ONE SCRIPT
#
# The same table `build-app.sh` holds, for the reason that file gives: three
# applications differing in a handful of strings, and three copies of a check is
# three chances for one to drift. `--all` is what a release wants, because the
# three bundle identifiers are one letter apart and the cheapest way to sign one
# application's profile into another's bundle is to look at only one of them.
#
# WHAT A READER MUST NOT CARRY, AND MUST SAY
#
# These applications read and never write, and three of the checks below are
# here because of it rather than because the fleet's other scripts have them.
#
# The sandbox grant is `files.user-selected.read-only`. A write grant is a
# finding and not a harmless extra: a capability asked for and unused is a
# question at review with no good answer, and a reader asking permission to
# write is exactly that question. It is read back out of the signature rather
# than off `odox.entitlements`, which is the only way to notice a build path
# that signed with a different file.
#
# `CFBundleTypeRole` is `Viewer` and `LSHandlerRank` is `Alternate`.
# `Info.plist.in` says why both: the first is a true statement about what the
# application does, and the second is what claims a format this project reads
# and does not own without asking to be preferred over the office suite that
# may already be installed. Either one wrong is a claim to the platform that the
# store listing contradicts.
#
# `CFBundleLocalizations` lists `en` and `de`. macOS builds its per-application
# language picker from that key, so without it nobody can set one of these to
# German on an English machine however complete `de.po` is, and App Store
# Connect lists English alone. Nothing in the window shows the omission, which
# is how it went unnoticed across four repositories.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)
set -u

apps=""
prefix="/Applications"

usage() {
    cat <<'USAGE'
usage: check-install.sh (xodt | xods | xodp | --all) [--in DIR]

  xodt|xods|xodp the application to ask about. One name, or --all for three.
  --all          ask about all three, which is what a release wants
  --in DIR       where the bundles are (default: /Applications). The question
                 is about an installed bundle; this is for the borrowed machine
                 that keeps them somewhere else, and for looking at ./dist
                 before anything has been copied into place.
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        xodt|xods|xodp) apps="${apps} $1"; shift ;;
        --all) apps="xodt xods xodp"; shift ;;
        --in) prefix="${2:?--in needs a directory}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "check-install.sh: unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

[ -n "$apps" ] || {
    echo "check-install.sh: name an application, or pass --all" >&2
    usage >&2
    exit 2
}

# The strings each application differs in. `build-app.sh` holds the same table
# and puts them into the bundle; this one reads them back out, so a value that
# disagrees between the two is a bundle built from a different commit or an
# application's plist substituted with its neighbour's values.
table() {
    case "$1" in
        xodt)
            product="Odox Text"
            identifier="com.excelano.xodt"
            uti="org.oasis-open.opendocument.text"
            ;;
        xods)
            product="Odox Grid"
            identifier="com.excelano.xods"
            uti="org.oasis-open.opendocument.spreadsheet"
            ;;
        xodp)
            product="Odox Deck"
            identifier="com.excelano.xodp"
            uti="org.oasis-open.opendocument.presentation"
            ;;
        *)
            echo "check-install.sh: no such application: $1" >&2
            exit 2
            ;;
    esac
}

findings=0
say()  { printf '  %-46s %s\n' "$1" "$2"; }
ok()   { say "$1" "ok — $2"; }
bad()  { say "$1" "NO — $2"; findings=$((findings + 1)); }
note() { printf '  %-46s %s\n' "$1" "$2"; }

# Not on PATH, and `build-app.sh` and README.md both say so.
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

echo "check-install.sh on $(uname -m), macOS $(sw_vers -productVersion)"
echo "bundles in ${prefix}"

# Asked once rather than once per application. The dump is megabytes and `--all`
# would otherwise spend the cost of it three times over.
dump=$("$lsregister" -dump 2>/dev/null) || dump=""

check_one() {
    app="$1"
    table "$app"

    bundle="${prefix}/${product}.app"
    exe="${bundle}/Contents/MacOS/${app}"
    info="${bundle}/Contents/Info.plist"

    echo
    echo "${product} — ${bundle}"

    [ -d "$bundle" ] || {
        bad "it is there" "nothing at ${bundle}"
        return 0
    }
    [ -x "$exe" ] || {
        bad "the executable is Contents/MacOS/${app}" "absent, or not executable"
        return 0
    }

    plist_get() { /usr/libexec/PlistBuddy -c "Print :$1" "$info" 2>/dev/null; }

    # 1. The architectures. A release bundle is universal, because a Store build
    #    runs on both or half the machines that bought it cannot run it, and a
    #    `lipo` that quietly dropped a slice produces a bundle that installs and
    #    runs perfectly on the machine that built it. arm64 is the finding;
    #    x86_64 is reported rather than demanded, because a development bundle
    #    built for this machine alone is a legitimate thing to be looking at.
    arches=$(lipo -info "$exe" 2>/dev/null | sed 's/.*[Ii]s architecture: //; s/.*are: //')
    case "$arches" in
        *arm64*) ok "the binary has an arm64 slice" "$arches" ;;
        *) bad "the binary has an arm64 slice" "${arches:-lipo could not read it}" ;;
    esac
    case "$arches" in
        *x86_64*) note "and an x86_64 one" "universal, which is what the Store takes" ;;
        *) note "and an x86_64 one" "no — fine for a local build, not for the Store" ;;
    esac

    # 2. And whether the machine is actually running that slice, which is not
    #    the same claim. Asked of the running process rather than of the file:
    #    `vmmap` prints the code type of a process you own, and Rosetta is
    #    invisible in every other listing. Compared against *this machine*
    #    rather than against arm64, so it is right on an Intel Mac too.
    pid=$(pgrep -f "${product}.app/Contents/MacOS/${app}" | head -1)
    if [ -n "$pid" ]; then
        code_type=$(vmmap "$pid" 2>/dev/null | sed -n 's/^Code Type: *//p' | head -1)
        case "$(uname -m),$code_type" in
            arm64,ARM64*|x86_64,X86*) ok "the running process is native" "$code_type on $(uname -m)" ;;
            arm64,X86*) bad "the running process is native" "$code_type on arm64 — under Rosetta" ;;
            *,"") note "the running process is native" "vmmap said nothing usable" ;;
            *) bad "the running process is native" "$code_type on $(uname -m)" ;;
        esac
    else
        note "the running process is native" "not running — launch it and re-run"
    fi

    # 3. Which kind of build this is, decided from the certificate rather than
    #    from what the caller believed. The checks after it want *different*
    #    answers: a Store build must carry an application identifier and a
    #    profile, a Developer ID or development build must carry neither, and a
    #    TestFlight build carries the identifier and no profile, because Apple
    #    strips the profile and re-signs. The team is whatever the signature
    #    names; nothing here has to know it in advance.
    auth=$(codesign -dv --verbose=2 "$bundle" 2>&1)
    team=$(printf '%s' "$auth" | sed -n 's/^Authority=[^(]*(\([A-Z0-9]\{10\}\)).*/\1/p' | head -1)
    case "$auth" in
        *"TestFlight Beta Distribution"*)
            kind=testflight
            ok "signed" "TestFlight Beta Distribution — Apple re-signed this" ;;
        *"Apple Distribution: "*)
            kind=store
            ok "signed" "Apple Distribution, team ${team} — a Store build" ;;
        *"Developer ID Application: "*)
            kind=devid
            ok "signed" "Developer ID, team ${team} — the outside-the-Store hedge" ;;
        *"Apple Development"*)
            kind=dev
            ok "signed" "Apple Development — a local test build, not shippable" ;;
        *)
            kind=unknown
            bad "signed" "$(printf '%s' "$auth" | sed -n 's/^Authority=//p' | head -1 |
                grep . || echo 'not signed — and an unsigned bundle is not sandboxed')" ;;
    esac
    case "$auth" in
        *"Apple Root CA"*) ok "the chain reaches the Apple Root CA" "three authorities" ;;
        *) bad "the chain reaches the Apple Root CA" "it does not" ;;
    esac

    if codesign --verify --deep --strict "$bundle" 2>/dev/null; then
        ok "the signature verifies" "--deep --strict"
    else
        bad "the signature verifies" "$(codesign --verify --deep --strict "$bundle" 2>&1 | head -1)"
    fi

    # 4. The entitlements, read back out of the signature rather than off the
    #    file that was fed to it. `plutil -p` does not spell a boolean the same
    #    way on every macOS: 15.7 prints `=> 1` and 26 prints `=> true`. Match
    #    the key and accept either.
    ents=$(codesign -d --entitlements - --xml "$bundle" 2>/dev/null | plutil -p - 2>/dev/null)
    case "$ents" in
        *'"com.apple.security.app-sandbox" => 1'*|*'"com.apple.security.app-sandbox" => true'*)
            ok "the sandbox is in the signature" "app-sandbox" ;;
        *'com.apple.security.app-sandbox'*)
            bad "the sandbox is in the signature" "present but not true" ;;
        *) bad "the sandbox is in the signature" "absent — the build is not sandboxed" ;;
    esac
    # The one grant, and the header above says why a second one is a finding
    # rather than a spare. Both spellings of true again.
    case "$ents" in
        *'"com.apple.security.files.user-selected.read-only" => 1'*|*'"com.apple.security.files.user-selected.read-only" => true'*)
            ok "the file grant is read-only" "files.user-selected.read-only" ;;
        *) bad "the file grant is read-only" "absent — the open panel will hand over nothing" ;;
    esac
    case "$ents" in
        *'com.apple.security.files.user-selected.read-write'*)
            bad "and nothing more than read-only" "read-write is in the signature, and this suite writes nothing" ;;
        *) ok "and nothing more than read-only" "no write grant" ;;
    esac
    # The store listing says these applications make no network request of any
    # kind. This is the half of that claim a command can settle.
    case "$ents" in
        *com.apple.security.network*)
            bad "no network entitlement" "one is present, and the listing says there is none" ;;
        *) ok "no network entitlement" "absent, as the listing states" ;;
    esac
    # Restricted, and so the whole reason a Store build needs a profile and
    # cannot run without one. A Developer ID or development build must *not*
    # carry it: it would be refused at launch for exactly the reason the Store
    # build is.
    case "$kind" in
        store|testflight) wants_app_id=yes ;;
        *) wants_app_id=no ;;
    esac
    case "$wants_app_id,$ents" in
        yes,*"${team}.${identifier}"*)
            ok "the application identifier is there" "${team}.${identifier}" ;;
        yes,*)
            bad "the application identifier is there" "absent, or for another of the three — the upload is refused" ;;
        no,*".${identifier}"*)
            bad "no application identifier" "present on a ${kind} build — it will not launch" ;;
        *)  ok "no application identifier" "correct for a ${kind} build" ;;
    esac
    # Declined deliberately: the profile grants it, these applications touch no
    # keychain, and a capability asked for and unused is a question at review.
    case "$ents" in
        *keychain-access-groups*) bad "keychain-access-groups is declined" "it is present" ;;
        *) ok "keychain-access-groups is declined" "absent, as intended" ;;
    esac

    # 5. The profile has to be inside the bundle, and inside it *before* it was
    #    signed; added afterwards, macOS calls the bundle damaged, which the
    #    verify above already catches, so this one is about presence. A
    #    TestFlight build carries none and must not.
    profile="${bundle}/Contents/embedded.provisionprofile"
    if [ "$kind" != store ] && [ ! -f "$profile" ]; then
        ok "no provisioning profile" "correct for a ${kind} build"
    elif [ -f "$profile" ]; then
        decoded=$(mktemp)
        if security cms -D -i "$profile" -o "$decoded" 2>/dev/null; then
            pname=$(plutil -extract Name raw -o - "$decoded" 2>/dev/null)
            pexp=$(plutil -extract ExpirationDate raw -o - "$decoded" 2>/dev/null)
            ok "a provisioning profile is embedded" "${pname:-unnamed}, expires ${pexp:-unknown}"
        else
            bad "a provisioning profile is embedded" "present but would not decode"
        fi
        rm -f "$decoded"
    else
        bad "a provisioning profile is embedded" "no embedded.provisionprofile"
    fi

    # 6. What the bundle says it is. The identifier first, because three
    #    bundles one letter apart is the arrangement that makes copying one
    #    application's plist into another's bundle a thing that happens and
    #    nothing downstream notices: the wrong application opens, or two of them
    #    fight over one identifier in Launch Services.
    got=$(plist_get CFBundleIdentifier)
    if [ "$got" = "$identifier" ]; then
        ok "the bundle identifier" "$identifier"
    else
        bad "the bundle identifier" "${got:-nothing} — this bundle should be ${identifier}"
    fi

    short=$(plist_get CFBundleShortVersionString)
    build=$(plist_get CFBundleVersion)
    floor=$(plist_get LSMinimumSystemVersion)
    note "the version it declares" "${short:-?} (build ${build:-?}), macOS ${floor:-?} and later"

    # 7. The two values that say what this application is to the platform.
    #    Neither is visible anywhere a person would look, and both contradict
    #    the store listing if they are wrong.
    got=$(plist_get "CFBundleDocumentTypes:0:CFBundleTypeRole")
    if [ "$got" = Viewer ]; then
        ok "the document type's role is Viewer" "these read and do not write"
    else
        bad "the document type's role is Viewer" "${got:-nothing} — Editor is a claim the File menu does not keep"
    fi
    got=$(plist_get "CFBundleDocumentTypes:0:LSHandlerRank")
    if [ "$got" = Alternate ]; then
        ok "the handler rank is Alternate" "it offers rather than asks to be preferred"
    else
        bad "the handler rank is Alternate" "${got:-nothing} — Default or Owner takes the association from whatever owns it"
    fi

    # And that both halves of the declaration name the same type: one says this
    # application handles the format, the other says what the format is.
    got=$(plist_get "CFBundleDocumentTypes:0:LSItemContentTypes:0")
    if [ "$got" = "$uti" ]; then
        ok "the type it handles" "$uti"
    else
        bad "the type it handles" "${got:-nothing} — this bundle should handle ${uti}"
    fi
    got=$(plist_get "UTImportedTypeDeclarations:0:UTTypeIdentifier")
    if [ "$got" = "$uti" ]; then
        ok "and the type it imports" "the same one, as OASIS spells it"
    else
        bad "and the type it imports" "${got:-nothing} — the handled type is declared by nothing"
    fi

    # 8. The languages the bundle claims, which is a separate thing from the
    #    languages the window speaks; the header says what is lost when this is
    #    missing. PlistBuddy prints an array indented inside braces, so the
    #    lines are trimmed before they are compared.
    langs=$(plist_get CFBundleLocalizations | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
    missing=""
    for want in en de; do
        printf '%s\n' "$langs" | grep -Fxq "$want" || missing="${missing} ${want}"
    done
    if [ -z "$missing" ]; then
        ok "CFBundleLocalizations lists en and de" "$(printf '%s' "$langs" | tr '\n' ' ')"
    else
        bad "CFBundleLocalizations lists en and de" "missing${missing} — System Settings will offer no language picker"
    fi

    # 9. Gatekeeper's verdict, which is not the signature's, and which for a
    #    Store build is *rejection*, correctly: `spctl -a` assesses the Developer
    #    ID and notarization policy, and a Store build is not distributed under
    #    it. What is worth reporting is a verdict that does not match the
    #    certificate the bundle carries.
    gk=$(spctl -a -vvv "$bundle" 2>&1)
    case "$kind,$gk" in
        *,*accepted*)
            note "Gatekeeper" "accepted — $(printf '%s' "$gk" | sed -n 's/.*source=//p' | head -1)" ;;
        store,*rejected*)
            note "Gatekeeper" "rejected, as a Store build correctly is" ;;
        devid,*"Unnotarized Developer ID"*)
            note "Gatekeeper" "rejected — unnotarized, which is the hedge's own step" ;;
        dev,*rejected*)
            note "Gatekeeper" "rejected, as a development build correctly is" ;;
        testflight,*rejected*)
            bad "Gatekeeper" "rejected a TestFlight build, which it should accept" ;;
        *)  bad "Gatekeeper" "$(printf '%s' "$gk" | tr '\n' ' ')" ;;
    esac

    # And whether Gatekeeper gets to decide at all, which is the variable that
    # actually governs a first launch. An unquarantined copy, anything built
    # here or carried over by scp, is not assessed, so a bundle that would be
    # refused after a download starts without a murmur.
    if xattr -p com.apple.quarantine "$bundle" >/dev/null 2>&1; then
        note "it carries com.apple.quarantine" "$(xattr -p com.apple.quarantine "$bundle" 2>/dev/null)"
    else
        note "it carries com.apple.quarantine" "no — so Gatekeeper is not consulted"
    fi

    # 10. Whether the App Sandbox actually engaged, which is a fact about a
    #     *run* rather than about the bundle. The container directory is made on
    #     first launch and by nothing else, so its absence after a launch means
    #     the entitlement was carried and not honoured, the failure that looks
    #     like success in every static check above.
    container="${HOME}/Library/Containers/${identifier}"
    if [ -d "$container" ]; then
        ok "a sandbox container exists" "$(basename "$container")"
    else
        note "a sandbox container exists" "not yet — launch it once and re-run"
    fi

    # 11. Launch Services, which is what makes a double-click reach this
    #     application at all.
    #
    #     The type is not asked about on its own. `org.oasis-open.opendocument.*`
    #     is Apple's declaration as much as this bundle's, so finding it
    #     somewhere in the dump would say nothing about whether these
    #     applications are in the list a person chooses from. What is asked is
    #     whether the records naming this bundle name the type as well. The dump
    #     separates its records with a rule of dashes, which is what the record
    #     is cut on.
    if [ -z "$dump" ]; then
        note "Launch Services knows the bundle" "lsregister would not dump"
    else
        records=$(printf '%s\n' "$dump" | awk -v want="$identifier" '
            /^--------/ { if (index(buf, want)) printf "%s", buf; buf = ""; next }
            { buf = buf $0 "\n" }
            END { if (index(buf, want)) printf "%s", buf }')
        if [ -n "$records" ]; then
            ok "Launch Services knows the bundle" "$identifier"
            case "$records" in
                *"$uti"*) ok "and that it claims the type" "$uti" ;;
                *) bad "and that it claims the type" "${uti} appears in no record of this bundle" ;;
            esac
        else
            bad "Launch Services knows the bundle" "not registered — a double-click will not arrive"
            note "and that it claims the type" "unasked, since the bundle is not registered"
        fi
    fi
}

for one in $apps; do
    check_one "$one"
done

echo
if [ "$findings" -eq 0 ]; then
    echo "Nothing mechanical is wrong with this install."
else
    echo "${findings} thing(s) to write down in the commit."
fi
echo "The rest needs eyes: the icon in the Dock, which is"
echo "the one place the .icns can be seen to have won; the layout at 2x; a merged"
echo "cell and a slide's own shape; and the window in German."
exit 0
