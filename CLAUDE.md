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

Releases: run `ship odox`. There is no release document.

`odox-core` takes bytes and returns bytes: no egui, no paths, no dialogs, and
`forbid(unsafe_code)`. A format question goes into the library; drawing goes into
`odox-ui`, which renders ODF's shared content model for all three windows. The
document tree stays faithful (DESIGN.md §3): a change that drops an element, an
attribute or whitespace has to be argued against the round-trip test.

Every sentence a person reads goes through `odox_ui::i18n::t`; a literal built
before a catalogue is in force is wrapped in `i18n::mark`. After changing one, run
`update-po.sh` and commit what it changes; CI refuses a stale template.

Clippy runs with `-D warnings`. `cargo test` and `cargo clippy` do not write
`target/debug/<app>`, so `cargo build` before looking at a window. A renderer is
verified by looking at it, on X11 here because the capture tool cannot see
Wayland: `env -u WAYLAND_DISPLAY DISPLAY=:0 cargo run -p xodt -- <file> &`, then
`import -window <id>`. `pkill -x xodt` stops it; `pkill -f` kills the shell too.
The target directory is shared across the fleet: `df -h /` before a long build.

The commit trailer is one line, a `Co-Authored-By` naming the model, and nothing
under it: no `Claude-Session:` line, because this repository is public.

Stay inside your own platform's arm: Linux is `packaging/linux` and
`packaging/debian`; `packaging/windows` and `packaging/macos` are reviewed from
here and changed only on their own machine. What cannot be settled here goes to
David rather than into a guess.
