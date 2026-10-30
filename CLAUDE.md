# CLAUDE.md

Three lightweight OpenDocument editors over one library: `xodt` opens text
documents, `xods` spreadsheets, `xodp` presentations; `odox` is a launcher that
hands a file to the right one. `DESIGN.md` is the authority on how they are
built; cite its sections.

    cargo build --release
    cargo test --workspace                                  # walks corpus/
    cargo clippy --workspace --all-targets -- -D warnings   # what CI runs
    cargo fmt --all
    cargo run -p xodt -- corpus/libreoffice/text.odt
    crates/odox-ui/po/update-po.sh          # after changing a user-visible string
    ./packaging/debian/build-deb.sh         # one .deb per application, into dist/

Releases: the apps in excelano/shipping, run from this directory. There is no release document.

`odox-core` takes bytes and returns bytes: no egui, no paths, no dialogs, and
`forbid(unsafe_code)`. A format question goes into the library; drawing goes into
`odox-ui`, which renders ODF's shared content model for all three windows. The
document tree stays faithful (DESIGN.md §3): a change that drops an element, an
attribute or whitespace has to be argued against the round-trip test.

Every sentence a person reads goes through `odox_ui::i18n::t`; a literal built
before a catalogue is in force is wrapped in `i18n::mark`. After changing one, run
`update-po.sh` and commit what it changes.

Clippy runs with `-D warnings`. `cargo test` and `cargo clippy` do not write
`target/debug/<app>`, so `cargo build` before looking at a window. A renderer is
verified by looking at it, on X11 here because the capture tool cannot see
Wayland: `env -u WAYLAND_DISPLAY DISPLAY=:0 cargo run -p xodt -- <file> &`, then
`import -window <id>`. A window driven with `xdotool` runs on a nested server,
since keystrokes sent to `:0` have landed in other windows: `Xephyr :7 -screen
1280x800 -ac &`, then `env -u WAYLAND_DISPLAY -u XDG_CURRENT_DESKTOP DISPLAY=:7
dbus-run-session -- <binary> <file> &` and `DISPLAY=:7 xdotool` (`windowfocus`,
find the window by `--pid`). The private bus with the environment set outside
it keeps rfd's file dialog inside Xephyr instead of on the real desktop; kill
the portal processes it started when done. Xephyr has no window manager, so
Fullscreen does nothing. A fake `BROWSER=<script>` records what a link click
opens without opening it. The binary is `~/.cache/cargo-target/debug/<app>`,
not `./target/debug/<app>`, because the target directory is shared across the
fleet. Stop an app with `kill <pid>` for the pid you started: `pkill -x xodt`
can reach David's own window, and `pkill -f` kills the shell too.

Linux is `packaging/linux` and `packaging/debian`. `packaging/windows` and
`packaging/macos` are edited from here too — `ssh winwork` reaches the Windows
VM (not always running) and `ssh macmini` reaches the Mac, for whatever
building, signing or verifying needs the real machine. What cannot be settled
even reaching those goes to David rather than into a guess.
