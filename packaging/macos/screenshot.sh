#!/bin/sh
# Photograph one application's own window at a size App Store Connect accepts.
#
# The counterpart of `packaging/windows/screenshot.ps1`, and written for the
# same reason: screenshots were once filed under *by hand, because no script
# can*, which was an assumption rather than a measurement. What a script cannot
# do is decide which document to open or whether the result is a good
# advertisement. What it can do is every mechanical part — size the window,
# front it, drive it into the state the shot is of, move the pointer out of the
# frame, capture, and refuse if what came back is the wrong size.
#
#   ./packaging/macos/screenshot.sh xodt --out shots/xodt-01-page.png
#   ./packaging/macos/screenshot.sh xods --click 90,220 \
#       --out shots/xods-02-sheet.png
#   ./packaging/macos/screenshot.sh xodp --app dist/'Odox Deck'.app \
#       --document corpus/libreoffice/focus.odp --out shots/xodp-01.png
#
# The bundle defaults to `dist/<Product>.app`, which is where `build-app.sh`
# puts it, and the document to the corpus file `packaging/store-listing.toml`
# names for that application. Only `--out` has no sensible default.
#
# FOUR ACTIONS, IN THE ORDER GIVEN
#
# `--click X,Y` presses a control. `--double X,Y` presses it twice inside the
# double-click interval, which is how a word in the page is taken as a
# selection. `--type TEXT` types. `--key NAME` sends one key, optionally with
# modifiers: `--key cmd+a`, `--key right`. X and Y are measured from the frame's
# top-left corner on a shot of the same size, so a coordinate read off an
# earlier shot is the coordinate to give.
#
# **They exist because a listing wants more than a document at rest.** Apple
# rejected segler's first Mac screenshots under guideline 2.3.3 for exactly
# that: four frames of a document sitting still, nothing selected, and the same
# panel reading *Select an element* in every one. A shot has to show the
# application being used, and driving it is the only way to get one.
#
# These three cannot be *used* the way an editor can — there is nothing to type
# into and nothing to save — so what a shot shows instead is a person having got
# somewhere: a heading picked in the outline and the page moved to it, a
# different sheet open with its own columns, a slide chosen out of the deck, a
# paragraph selected, a page scrolled off its first line. Each application's
# side panel is the cheapest of those to drive, because it is a list of
# clickable rows down the left edge: `--click` a row and the central panel
# follows it. `--key right` and `--key down` move a slide and a cell cursor
# respectively, which is the other half of what these windows respond to.
#
# THREE THINGS MEASURED RATHER THAN ASSUMED
#
# **It captures the window by its id, not by its rectangle.** `screencapture -R`
# photographs whatever is on screen in that region, so anything overlapping the
# window lands in the picture — which happened to segler on the first attempt
# and came back as a screenful of terminal. `-l` takes the window's own buffer
# and is indifferent to what is in front of it.
#
# **The pointer is moved off the window first.** Windows found this the
# expensive way: a shot came back 2292 pixels different from its predecessor and
# none of them were the change being photographed, because the pointer was
# resting on a control and egui drew it hovered with the scroll bar showing.
# Neither is wrong, and both read as an interface caught mid-use.
#
# **It photographs a bundle, never the bare executable.** A bare Unix executable
# has no bundle identifier and no icon, so it is not the thing anybody installs,
# and on this suite it is also not the thing that draws the right icon in the
# Dock. The closest this platform can get is a *signed bundle built from the
# same commit*: the Store package cannot be launched at all off the Store — the
# kernel refuses its entitlements without a profile covering this Mac — so no
# screenshot can ever be of the exact artefact that gets uploaded. Build the
# bundle from the commit being released and say so in
# `packaging/store-listing.toml`.
#
# THE WINDOW IS FOUND BY PROCESS ID AND NOT BY NAME
#
# This is where the script departs from the sibling it came from. segler's asks
# the window server for a window whose owner is called *Segler* and asks System
# Events for a process whose name contains *segler*, and both work there because
# the product name and the executable name are the same word. Here they are not:
# the bundle is `Odox Text.app`, AppKit names the process after `CFBundleName`,
# and the executable is `xodt`. Rather than guess which of the two each API
# reports — and be wrong on one machine — the process is found once by the path
# of its executable and everything after that is asked by its id.
#
# Needs Accessibility permission for whatever runs it, because sizing another
# application's window goes through System Events. System Settings → Privacy &
# Security → Accessibility.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "${here}/../.." && pwd)

name=""
bundle=""
document=""
out=""
# 1440x900 is one of the four sizes App Store Connect accepts for macOS, and the
# largest reachable without a Retina display. The other two — 2560x1600 and
# 2880x1800 — need a backing scale of 2, which is why they are not the default.
width=1440
height=900
# Anywhere the window fits entirely on screen; the capture does not depend on
# this, but a window hanging off the edge is clipped by the window server.
x=100
y=80

usage() {
    sed -n '2,29p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

refuse() { echo "screenshot.sh: $1" >&2; exit 1; }

# The two strings this needs per application, and the document
# `packaging/store-listing.toml` tells a reviewer to open, which is the document
# the listing's own pictures should therefore be of. `build-app.sh` holds the
# full table; only these three columns are wanted here.
table() {
    case "$1" in
        xodt) product="Odox Text"; sample="corpus/libreoffice/text.odt" ;;
        xods) product="Odox Grid"; sample="corpus/libreoffice/calc.ods" ;;
        xodp) product="Odox Deck"; sample="corpus/libreoffice/growing-liberty.odp" ;;
        *) refuse "no such application: $1" ;;
    esac
}

# The actions, one to a line, in the order they were given. A file rather than
# a variable because `--type` takes text with spaces in it, and a
# space-separated list would split a sentence into words.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM
actions="$work/actions"
: > "$actions"

while [ $# -gt 0 ]; do
    case "$1" in
        xodt|xods|xodp) name="$1"; shift ;;
        --app) bundle="${2:?--app needs a bundle}"; shift 2 ;;
        --document) document="${2:?--document needs a file}"; shift 2 ;;
        --out) out="${2:?--out needs a path}"; shift 2 ;;
        --click) echo "click ${2:?--click needs X,Y}" >> "$actions"; shift 2 ;;
        --double) echo "double ${2:?--double needs X,Y}" >> "$actions"; shift 2 ;;
        --type) echo "type ${2?--type needs text}" >> "$actions"; shift 2 ;;
        --key) echo "key ${2:?--key needs a name}" >> "$actions"; shift 2 ;;
        --width) width="${2:?}"; shift 2 ;;
        --height) height="${2:?}"; shift 2 ;;
        --x) x="${2:?}"; shift 2 ;;
        --y) y="${2:?}"; shift 2 ;;
        -h|--help) usage 0 ;;
        *) echo "screenshot.sh: unknown argument $1" >&2; usage 2 ;;
    esac
done

[ -n "$name" ] || refuse "name an application: xodt, xods or xodp"
table "$name"

[ -n "$out" ] || refuse "no --out given"
[ -n "$bundle" ] || bundle="${root}/dist/${product}.app"
[ -n "$document" ] || document="${root}/${sample}"

[ -d "$bundle" ] || refuse "no bundle at ${bundle}; build one with 'packaging/macos/build-app.sh ${name}'"
[ -f "$document" ] || refuse "no document at $document"

case "$bundle" in
    *.app) ;;
    *) refuse "--app wants a .app bundle; a bare executable has no icon and is not what anybody installs" ;;
esac

# `open -a` reads a relative path as an application *name* to look up, and
# answers "Unable to find application named 'dist/Odox Text.app'" — which reads
# like the bundle is missing when it is sitting right there.
bundle=$(cd "$(dirname "$bundle")" && pwd)/$(basename "$bundle")
document=$(cd "$(dirname "$document")" && pwd)/$(basename "$document")

mkdir -p "$(dirname "$out")"

# The helper does the things no shell command on this platform will: it reads
# the window server for an ordinary window belonging to a given process, it puts
# the pointer somewhere harmless, and it posts the pointer and keyboard events
# that drive the window. Compiled once rather than interpreted at each call,
# because a shot that drives the window calls it six or seven times and `swift`
# pays its compile every time; the temporary directory goes with the trap either
# way.
source="$work/helper.swift"
helper="$work/helper"
cat > "$source" <<'SWIFT'
import CoreGraphics
import Foundation

// Park the pointer in the far corner. The corner rather than a constant: a
// fixed coordinate is off-screen on a smaller display, and the window server
// clamps to an edge, which could be the edge the window is on.
//
// Press a control: move there, then a press and a release a moment apart, which
// is what egui reads as a click. A System Events `click at` at the same point
// toggled nothing in the repository this came from, measured twice; this did,
// and why was not chased.
//
// A double press is the same two events twice with `mouseEventClickState`
// counting them, which is the field egui reads to tell a second click from a
// first. Without it, two presses a moment apart are two clicks in the page and
// never a word taken as a selection.
//
// Typing goes in as a Unicode string on a keyboard event rather than as a key
// code, so a line with punctuation in it needs no layout table. Forty
// milliseconds a character, because the window redraws between them and faster
// than that drops characters the way `xdotool` does on the Linux lane.

let args = CommandLine.arguments

func post(_ type: CGEventType, at p: CGPoint, clicks: Int64) {
    let event = CGEvent(
        mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)!
    event.setIntegerValueField(.mouseEventClickState, value: clicks)
    event.post(tap: .cghidEventTap)
}

func press(at p: CGPoint, times: Int64) {
    post(.mouseMoved, at: p, clicks: 1)
    usleep(150_000)
    for n in 1...times {
        post(.leftMouseDown, at: p, clicks: n)
        usleep(40_000)
        post(.leftMouseUp, at: p, clicks: n)
        // Under the double-click interval, which is what makes a second press a
        // second click rather than another first one.
        usleep(60_000)
    }
}

if args.contains("--click") || args.contains("--double") {
    let p = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
    press(at: p, times: args.contains("--double") ? 2 : 1)
    usleep(100_000)
    exit(0)
}

if args.contains("--type") {
    for character in Array(args[2]) {
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: down)!
            var utf16 = Array(String(character).utf16)
            event.keyboardSetUnicodeString(stringLength: utf16.count, unicodeString: &utf16)
            // Emptied rather than left alone. An event made from a nil source
            // takes the session's current modifier state, and a `--key cmd+a`
            // just before this leaves command in it, so every character after
            // one arrives as a command shortcut and types nothing at all.
            event.flags = []
            event.post(tap: .cghidEventTap)
            usleep(20_000)
        }
        usleep(40_000)
    }
    exit(0)
}

// Only the keys a screenshot run has wanted; add to the table rather than
// reaching for a layout API. The arrows, Home, End and the page keys are here
// because they are what these three windows answer to: a slide steps on Right
// and PageDown, a cell cursor moves on any arrow, and Home and End go to the
// ends of a deck or a sheet.
if args.contains("--key") {
    let codes: [String: CGKeyCode] = [
        "a": 0, "c": 8, "o": 31, "r": 15, "return": 36, "escape": 53, "tab": 48,
        "space": 49, "delete": 51, "left": 123, "right": 124, "down": 125,
        "up": 126, "home": 115, "end": 119, "pagedown": 121, "pageup": 116,
    ]
    var flags: CGEventFlags = []
    var name = ""
    for part in args[2].lowercased().split(separator: "+") {
        switch part {
        case "cmd", "command": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "alt", "option": flags.insert(.maskAlternate)
        case "ctrl", "control": flags.insert(.maskControl)
        default: name = String(part)
        }
    }
    guard let code = codes[name] else {
        FileHandle.standardError.write("no key named \(name)\n".data(using: .utf8)!)
        exit(2)
    }
    for down in [true, false] {
        let event = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)!
        event.flags = flags
        event.post(tap: .cghidEventTap)
        usleep(40_000)
    }
    // The modifier is let go here as its own event, so that the session state
    // the next event inherits has nothing held down in it.
    if !flags.isEmpty {
        let cleared = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)!
        cleared.type = .flagsChanged
        cleared.flags = []
        cleared.post(tap: .cghidEventTap)
        usleep(40_000)
    }
    exit(0)
}

if args.contains("--park") {
    let screen = CGDisplayBounds(CGMainDisplayID())
    CGWarpMouseCursorPosition(CGPoint(x: screen.maxX - 1, y: screen.maxY - 1))
    exit(0)
}

// The window belonging to a process id. Layer 0 is an ordinary window: a menu,
// a tooltip and the Dock's own surfaces are all above it, and one of those
// would otherwise be photographed as though it were the application.
let wanted = args.count > 1 ? (Int(args[1]) ?? -1) : -1
guard
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)
        as? [[String: Any]]
else {
    FileHandle.standardError.write("the window server returned nothing\n".data(using: .utf8)!)
    exit(2)
}
for w in windows {
    guard (w[kCGWindowOwnerPID as String] as? Int) == wanted,
          (w[kCGWindowLayer as String] as? Int ?? -1) == 0,
          let number = w[kCGWindowNumber as String] as? Int
    else { continue }
    print(number)
    exit(0)
}
FileHandle.standardError.write("no ordinary window belonging to process \(wanted)\n".data(using: .utf8)!)
exit(1)
SWIFT

swiftc -O -o "$helper" "$source" || refuse "the helper did not compile"

# Anything already running is stopped, so the window photographed is the one
# holding the document this run was given rather than one left over. Matched on
# the bundle's own executable path, which is what makes this safe to run while
# the other two applications have documents open.
pkill -f "${bundle}/Contents/MacOS/" 2>/dev/null || true
sleep 1

open -a "$bundle" "$document"
sleep 5

# Found once, by the path of the executable inside this bundle, and used by id
# everywhere below. The header says why not by name.
pid=$(pgrep -f "${bundle}/Contents/MacOS/${name}" | head -1)
[ -n "$pid" ] || refuse "nothing is running from ${bundle} — did it open ${document}?"

osascript >/dev/null <<OSA || refuse "could not size the window — is Accessibility granted?"
tell application "System Events"
    set p to first process whose unix id is ${pid}
    set frontmost of p to true
    tell p
        set position of window 1 to {$x, $y}
        set size of window 1 to {$width, $height}
    end tell
end tell
OSA
sleep 1

# Pointer coordinates are given in the frame and posted on the screen, so the
# window's own origin is added here and nowhere else.
#
# The list is read on a descriptor of its own and the helper is given no input
# at all. Both matter: with the loop reading the file as stdin, the helper
# inherits it, reads what is left of it, and the actions it swallowed never run
# — which looked exactly like typing that did not reach the window, and cost an
# afternoon.
while IFS= read -r action <&3; do
    verb=${action%% *}
    rest=${action#* }
    case "$verb" in
        click|double)
            "$helper" "--$verb" \
                "$(( x + ${rest%,*} ))" "$(( y + ${rest#*,} ))" </dev/null
            ;;
        type) "$helper" --type "$rest" </dev/null ;;
        key) "$helper" --key "$rest" </dev/null ;;
        *) refuse "unknown action $verb" ;;
    esac
    sleep 1
done 3< "$actions"

"$helper" --park
sleep 1

id=$("$helper" "$pid") || refuse "could not find the window"
screencapture -x -o -l "$id" "$out"

got_w=$(sips -g pixelWidth "$out" | sed -n 's/.*pixelWidth: *//p')
got_h=$(sips -g pixelHeight "$out" | sed -n 's/.*pixelHeight: *//p')
if [ "$got_w" != "$width" ] || [ "$got_h" != "$height" ]; then
    refuse "asked for ${width}x${height} and got ${got_w}x${got_h} — App Store Connect refuses anything but its own sizes"
fi

echo "${out}: ${got_w}x${got_h}, window ${id} of ${product}"
echo "look at it before it goes anywhere: a correct size is not a good screenshot,"
echo "and a document sitting still is what guideline 2.3.3 sends back"
