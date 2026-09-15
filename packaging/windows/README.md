# Windows

The lane needs a Windows machine with a Rust toolchain, the Windows SDK for
`makeappx` and `signtool`, and `identity.psd1` copied from
`identity.psd1.example` with the values Partner Center assigned; that copy is
not committed. From the repository root:

    cargo build --release
    powershell -ExecutionPolicy Bypass -File packaging\windows\check-imports.ps1
    powershell -ExecutionPolicy Bypass -File packaging\windows\build-msix.ps1 -All
    ...\build-msix.ps1 xodt -SelfSign             # installable here, to look at
    ...\build-msix.ps1 xodt -SelfSign -Certify    # and the certification kit, elevated
    ...\install.ps1                               # per-user integration, under HKCU
    ...\uninstall.ps1
    ...\check-install.ps1                         # install, uninstall, nothing left
    ...\screenshot.ps1 xodt -Out C:\shots\xodt-01-page.png

`build-msix.ps1` produces one package per application from the release
executable, the manifest with the identity and version substituted, and the
assets under `assets\`. The Store signs what it distributes; `-SelfSign` makes a
throwaway certificate so a package can be installed and looked at here, and
`-Certify` runs the Windows App Certification Kit against it. `windows.yml`
runs on every push the checks that need no identity; the package build, the kit
and the screenshots need this machine.

`odox.manifest` is embedded in all three viewers by each crate's `build.rs`,
through two linker arguments and no resource compiler; it declares per-monitor
DPI awareness and UTF-8 as the active code page. The window icon travels the
same way: `build.rs` stages the `.ico` into `OUT_DIR` and the application
includes it. `check-imports.ps1` refuses any import that does not ship with
Windows, which together with `+crt-static` in `.cargo/config.toml` keeps the
Visual C++ runtime out of a shipped binary.

## Claiming a file type

An install adds each application to `OpenWithProgids` for its extension and
never writes `UserChoice` or the extension's default value, so it appears in
Open With and a double-click keeps going wherever it went before. OpenDocument
is a format this suite reads and does not own, on a machine that may have an
office suite claiming it; the macOS bundle takes the same posture with
`LSHandlerRank` set to `Alternate`. A key nobody writes is a key no uninstaller
can strand.
