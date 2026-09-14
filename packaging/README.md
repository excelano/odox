# packaging

One directory per platform, and each platform's decisions live in its own
directory. What is here is shared by all three: `version.sh`, which is the only
thing that reads the version, and `preflight.sh`, which runs everything that has
to be true before a release.

`linux/` holds the desktop entries, the icons and the scripts that install them
under `~/.local`, plus `check-libraries.sh`, which is where `debian/`'s `Depends`
line comes from. `debian/` builds one `.deb` per application from a release
build. `windows/` holds the application manifest every binary embeds and the
import check that keeps a build off the Visual C++ runtime. `macos/` holds what
that platform needs and has not been built yet.

The icons are one SVG each, drawn on a 64-unit grid with `width` and `height` on
the root element, because the macOS build script rewrites those to render each
size natively and refuses when it cannot find them. Check a change to one at 16,
24, 32, 48 and 128 pixels on light and dark grounds before committing it.

`make-icons/` is the one generator: it reads the three SVGs in `linux/icons`
and writes the `.ico` and package assets Windows wants, the `.icns` macOS wants,
and the squares every store's listing form asks for. It is a standalone package
so that nothing it depends on reaches a shipped binary, and it is at this level
rather than inside a platform's arm because it writes into three of them.
Rasterized artefacts are committed, because neither of those platforms has a
step that rasterizes at install time the way a Linux desktop does.
