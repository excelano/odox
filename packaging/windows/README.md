# Windows

Nothing here has been built yet. What is in place is the part that is expensive
to add later and cheap to add now, and the reasoning for each is in `DESIGN.md`
§9.

`odox.manifest` is embedded in all three binaries by each crate's `build.rs`,
through two linker arguments and no resource compiler. It declares per-monitor
DPI awareness, which the Windows App Certification Kit reads out of the manifest
rather than out of the running process, and UTF-8 as the active code page.

`check-imports.ps1` refuses any import that does not ship with Windows. Run it on
each release binary. It pairs with `+crt-static` in `.cargo/config.toml`, and the
two together are what answers the certification failure recorded in the script's
own header.

## Claiming a file type, and how far

**These applications never write `UserChoice`.** An install adds each one to
`OpenWithProgids` for its extension, so it appears in Open With and a person can
choose it, and it stops there. It does not make itself the default for `.odt`.

That is the same posture the macOS bundle takes with `LSHandlerRank` set to
`Alternate`, and for the same reason: OpenDocument is a format this suite reads
and does not own, on a machine that may well have a full office suite already
claiming it. Taking the default without being asked is a thing a person then has
to undo.

It also removes a failure the fleet has already had. A script that writes
`UserChoice` has to delete it on the way out, slipcase-desktop's did not, and the
extension was left pointing at a program that was no longer there; flyleaf's
uninstaller deletes the key by name from its parent because `DeleteSubKeyTree`
was not enough. None of that applies to a key nobody writes.

## What is left to do here

A window icon: Windows takes one from a resource compiled into the executable,
and there is no resource compiler in this build, so the `.ico` travels as
`include_bytes!` and is decoded into `IconData` at startup — the arrangement
slipcase-desktop, segler and duckling all use, each with an `ico` dependency
gated to this target. An `.ico` per application, rendered from
`packaging/linux/icons/`. Then the installer or the MSIX package, one
`uap:FileTypeAssociation` per application with its own icon assets, the
`BackgroundColor` transparent with `altform-unplated` assets present, and a
certification baseline left empty until a kit run fills it.
