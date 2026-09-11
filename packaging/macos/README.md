# macOS

Nothing here has been built yet. Two things this platform needs are code rather
than files, so they cannot arrive with a packaging script, and both are recorded
in `~/notes/desktop_app_from_the_start.md` from the repositories that learned
them.

**Opening a document.** macOS delivers a double-clicked document as an Apple
Event and never as `argv[1]`. Without a handler AppKit refuses it and Finder
blames the application. The handler is one Objective-C method and therefore the
one module in an application crate that will need `#[allow(unsafe_code)]`, which
is why those crates are `deny` and not `forbid`; `odox-core` and `odox-ui` stay
`forbid` either way. slipcase-desktop's `opened_document.rs` is the module to
copy, installed at `applicationWillFinishLaunching:` through a notification
observer, which it measured as the only one of three moments that catches both a
cold launch and a document dropped on a running window.

**Saving.** Not yet: these applications do not write. When one does, the sandbox
grant a person gives by choosing a file covers the file and not its directory, so
a temporary file beside the target fails with *Operation not permitted*. The
rewrite waits in `NSItemReplacementDirectory` asked for with the target's URL and
lands with `replaceItemAtURL:`.

**The winit patch** in the workspace manifest is here for this platform: review
reads the symbol table rather than the call graph, and winit 0.30 declares a
private CoreGraphics symbol whether or not anything calls it. Delete the patch
when a winit release carries the upstream gate.

## What is left to do here

An `.icns` per application rendered from `packaging/linux/icons/`, an
`Info.plist.in` per application with `LSApplicationCategoryType`,
`ITSAppUsesNonExemptEncryption` false, `LSMinimumSystemVersion` agreeing with
`MACOSX_DEPLOYMENT_TARGET`, `CFBundleVersion` from the commit count, and the
OpenDocument types declared as imported rather than exported, since this suite
does not own the format. Then `build-app.sh`, which signs with an Apple
Development identity from the first bundle — the sandbox is inert until the
entitlement is inside a signature — and refuses any binary importing a symbol no
public framework header declares.

Hand the viewport an **empty** `IconData` on this platform: eframe substitutes
its own egui logo for a viewport that names no icon and passes it to
`setApplicationIconImage:`, which outranks the bundle's `.icns`. Nothing short of
a person looking at the Dock finds that.
