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
