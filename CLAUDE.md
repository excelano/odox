# CLAUDE.md

Guidance for Claude Code working in `odox`. It is short because `DESIGN.md` is
where the reasoning lives; read that first and cite its sections rather than
restating them.

---

## What this is

Three OpenDocument viewers over one library. `xodt` reads text documents, `xods`
spreadsheets, `xodp` presentations. They read and do not write.

**The library has no window in it.** `odox-core` takes bytes and returns bytes,
links no egui, opens no files, and is the crate that keeps `forbid(unsafe_code)`.
`odox-ui` is `deny` with one `allow`, on `opened_document`, which is the whole of
the `unsafe` in the workspace: receiving a document from macOS needs one
Objective-C method. It is there rather than in the applications because all three
windows come through one `run`, and the three applications are `forbid`. Anything that needs a path, a dialog or a
`Ui` belongs in `odox-ui` or in an application. If a format question comes up
while working on a window, the answer goes into the library.

**The document tree is faithful and stays that way.** DESIGN.md §3. A change that
makes the reader drop an element, an attribute or a piece of whitespace it does
not understand is a change that has to be argued, because the round-trip test is
the only measure of compliance this repository has.

**Drawing belongs to `odox-ui`.** ODF's content model is shared across the three
formats, so a paragraph inside a spreadsheet cell and a paragraph inside a slide
draw through the same renderer as a paragraph in a document. Adding a case to
`flow.rs` fixes it in three places; adding it to an application fixes it in one
and hides the other two.

---

## Committing

**The trailer block is one line.** A `Co-Authored-By` naming the model, and
nothing under it. Some harnesses append a `Claude-Session:` line carrying a URL.
This repository is public, so that is a private identifier written into a
permanent public record for no reader's benefit; slipcase-desktop has stripped it
from pushed history twice, and this repository has 41 of them in its own pushed
history from before the rule was written down. Read what you are about to commit
rather than trusting what the harness composed.

The `Co-Authored-By` names the model and not the context window it ran in.

## Commands

    cargo build                          # debug
    cargo build --release
    cargo test --workspace               # the corpus is walked, not named
    cargo clippy --workspace --all-targets -- -D warnings   # what CI runs
    cargo fmt --all
    cargo check --target x86_64-pc-windows-msvc    # cross-check, from Linux
    cargo check --target aarch64-apple-darwin     # and the other one

    cargo run -p xodt -- corpus/libreoffice/text.odt
    cargo run -p xods -- corpus/libreoffice/calc.ods
    cargo run -p xodp -- corpus/libreoffice/deck.odp

    ./packaging/linux/install.sh         # desktop integration, after a release build
    ./packaging/debian/build-deb.sh      # one .deb per application
    ./packaging/preflight.sh --ci        # everything above, before a release

**If you are here for a release, read `RELEASE.md`.** It is the live document and
it holds the process rather than the history: what the next release costs, in the
order it is done. `ship odox` reports where the current one stands and
`ship odox <version>` runs it.

The build target directory is shared across the fleet and fills the disk. Check
`df -h /` before a long build; a debug tree of three eframe applications is
several gigabytes.

---

## The corpus

`corpus/libreoffice/` is authored by LibreOffice and rebuilt from the plain-text
sources beside it by `build.sh`, which needs `libreoffice-writer`,
`libreoffice-calc`, `libreoffice-impress` and `pandoc`. Those are the documents
to trust: a fixture written by hand agrees with whatever the person writing it
believed, which is the belief under test.

Adding a document to the corpus is enough to put it under test. Adding one that a
real producer wrote is worth more than adding three that were not.

---

## Looking at the window

**A renderer is verified by looking at it, and there is no substitute.** Every
defect worth finding here so far — a list label drawn over its own text, a merged
cell filled to one column of three, a slide title in dark ink on a dark ground —
was invisible to the tests and obvious in a screenshot. Run the application on a
corpus document and look before saying a drawing change works.

On this machine that means X11, because the capture tool cannot see a Wayland
window:

    env -u WAYLAND_DISPLAY DISPLAY=:0 cargo run -p xodt -- corpus/libreoffice/text.odt &
    import -window "$(xwininfo -root -tree | grep -i 'mutter-x11-frames' | grep -i xodt \
        | grep -o '0x[0-9a-f]*' | head -1)" /tmp/shot.png

`pkill -x xodt` to stop it; `pkill -f` matches the shell running the command and
kills that instead.

**A drawing can be checked against LibreOffice rather than against an opinion.**
It renders a slide to an image without a display, so a change to the slide
renderer has a reference to be measured against:

    soffice --headless --convert-to png --outdir /tmp corpus/libreoffice/focus.odp

Comparing the two by eye finds the gross errors. Comparing the proportion of
each colour, over the slide area of the window shot and the whole of
LibreOffice's, finds the rest and gives a number: the polygons of `focus.odp`
agree to within about one per cent, and a shape in the wrong place moves several.

**Run clippy with `-D warnings`, which is what CI runs.** Without it a pedantic
lint is a warning that scrolls past, and grepping the output for `^warning: [a-z]`
misses the ones whose message opens with a number — which is how
`many_single_char_names` reached `main` and turned CI red on a tree that looked
clean.

**`cargo test` and `cargo clippy` do not write `target/debug/<app>`.** Both were
run, both were green, and the window that was then looked at was Friday's
binary — twice now, once for the pseudolocale and once for German. `cargo build`
before you look, and if a window shows the old behaviour after a change to
strings or drawing, suspect the binary before the code.

---

## Strings

Every sentence a person reads goes through `odox_ui::i18n::t`, and a message that
has to be a literal where no catalogue is in force yet — a `const`, a `Product` —
is wrapped in `i18n::mark` and looked up through `t` where it is drawn. There is
one catalogue for the suite and it lives in `crates/odox-ui/po`, because
`potext::catalog!` gives the storage to the crate that invokes it and
`include_str!` cannot reach above a crate root.

After changing any such sentence, run `crates/odox-ui/po/update-po.sh` and commit
what it changes; `preflight.sh` refuses a release whose template is behind the
source. Then look at the result:

    crates/odox-ui/po/pseudo.sh
    POTEXT_LANG=en-x-pseudo cargo run -p xods -- corpus/libreoffice/calc.ods

The pseudolocale accents every message and pads it 40%, which is roughly what
German costs. A string still in English never went through `t`; a label with its
end cut off was built to the width of English. Neither is reachable by a test.

---

## Stay inside your own platform's arm

Linux is `packaging/linux` and `packaging/debian`, Windows is
`packaging/windows/README.md`, macOS is `packaging/macos/README.md`. Reviewing
another platform's arm is worth doing and is how the worst defects in the sibling
repositories were found; changing one you cannot run is not. What cannot be
settled from here goes to David rather than into a guess.
