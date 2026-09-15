# macOS

The lane needs a Mac with Xcode's command line tools and a Rust toolchain.
`build-app.sh` assembles a bundle per application from the release build: the
executable, `Info.plist.in` with the version and the per-application strings
substituted, the `.icns`, and the entitlements. From the repository root:

    MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release
    ./packaging/macos/build-app.sh --all
    ./packaging/macos/build-app.sh xodt --sign "Apple Development: ..."
    ./packaging/macos/build-app.sh --all --universal     # after both --target builds
    ./packaging/macos/build-app.sh xodt \
        --store ~/Downloads/Odox_Text_Mac_App_Store.provisionprofile
    ./packaging/macos/check-install.sh --all             # what an installed bundle is
    ./packaging/macos/screenshot.sh xodt --out shots/xodt-01-page.png

The sandbox entitlement does nothing until it is inside a signature, so a
bundle to test is signed with an Apple Development identity. `--store` signs
with Apple Distribution, wraps the bundle with `productbuild` under a 3rd Party
Mac Developer Installer identity, and produces the `.pkg` that `altool`
validates and uploads; `security find-identity -v -p codesigning` lists what
the machine holds. A provisioning profile covers one bundle identifier, so
`--store` takes one application at a time. A Store build cannot run on the
machine that made it, because its entitlements need a profile covering the Mac
and a Store profile covers none, which is why the script unregisters the bundle
from Launch Services after building it.

`CFBundleTypeRole` is `Viewer` and `LSHandlerRank` is `Alternate`, because the
applications read a format they do not own; the OpenDocument types are declared
as imported. `LSMinimumSystemVersion` is 11.0 and the build's
`MACOSX_DEPLOYMENT_TARGET` has to agree with it, which `build-app.sh` checks.
The sandbox grant is `files.user-selected.read-only` and nothing else.
`CFBundleVersion` is the first-parent commit count. `lsregister` is under
`/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/`
and not on `PATH`.

`build-app.sh` refuses a binary that imports a symbol no public framework header
declares, which is what App Store review rejects. The workspace manifest patches
`winit` to drop one such symbol; delete the patch when a winit release carries
the upstream gate.

`odox_ui::run` hands the viewport an empty `IconData` on this platform, because
eframe otherwise substitutes its own logo and that outranks the bundle's
`.icns`. Only the Dock shows whether it works.
