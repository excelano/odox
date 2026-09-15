# packaging

One directory per platform. `linux/` holds the desktop entries, the icon SVGs
and the scripts that install them under `~/.local`, plus `check-libraries.sh`,
which is where `debian/`'s `Depends` line comes from. `debian/` builds one
`.deb` per application from a release build. `windows/` and `macos/` each carry
their platform's manifest, build script, install check and screenshot script,
with a README saying how to run that lane. `store-listing.md` and its German
half are the text both stores take. Releases are `ship odox`.

`version.sh` is the only thing that reads the version, from the workspace
`Cargo.toml`, in whichever spelling a caller needs: plain, `--appx` for the
four-part Store form, `--short` for `CFBundleShortVersionString`, and `--build`
for `CFBundleVersion`, which is the first-parent commit count.

The icons are one SVG each in `linux/icons`, drawn on a 64-unit grid. Check a
change at 16, 24, 32, 48 and 128 pixels on light and dark grounds, then rerun
the generator:

    cargo run --manifest-path packaging/make-icons/Cargo.toml

`make-icons/` reads the three SVGs and writes the `.ico` files and package
assets under `windows/`, the `.icns` files under `macos/`, and the squares
under `icons/` that the store listing forms ask for. It is a standalone package
so that nothing it depends on reaches a shipped binary. Its output is committed,
because neither Windows nor macOS rasterizes at install time, and CI refuses a
push where the committed files differ from what the generator writes.
