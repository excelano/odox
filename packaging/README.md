# packaging

One directory per platform. `linux/` holds the desktop entries and the scripts
that install them under `~/.local`, plus `check-libraries.sh`, which is where
`debian/`'s `Depends` line comes from. `debian/` builds one `.deb` per
application from a release build. `windows/` and `macos/` each carry
their platform's manifest, build script, install check and screenshot script,
with a README saying how to run that lane. `store-listing.md` and its German
half are the text both stores take. Releases are `ship odox`.

`version.sh` is the only thing that reads the version, from the workspace
`Cargo.toml`, in whichever spelling a caller needs: plain, `--appx` for the
four-part Store form, `--short` for `CFBundleShortVersionString`, and `--build`
for `CFBundleVersion`, which is the first-parent commit count.

`artwork/` holds the drawings, two for each application and both on a 64-unit
grid. `<app>-document.svg` is the page a file of that format is drawn as, and
`<app>-application.svg` is the glyph off that page standing alone, which is what
a window, a launcher, a Dock and a store listing show. Check a change at 16, 24,
32, 48 and 128 pixels on light and dark grounds, then rerun the generator:

    cargo run --manifest-path packaging/make-icons/Cargo.toml

`make-icons/` reads them and writes the `.ico` files and package assets under
`windows/`, the `.icns` files under `macos/`, and the squares under `icons/`
that the store listing forms ask for. It is a standalone package so that nothing
it depends on reaches a shipped binary. Its output is committed, because neither
Windows nor macOS rasterizes at install time, and CI refuses a push where the
committed files differ from what the generator writes.

Linux takes no rendered icon. `install.sh` and `build-deb.sh` put the
application drawing straight into the hicolor theme and the desktop rasterizes
it, and a document there is drawn by shared-mime-info rather than by anything in
this repository.
