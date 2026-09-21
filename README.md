# odox

Three small desktop applications that open an OpenDocument file, let you change
what is in it, and save it back as the document it was. `xodt` is for text
documents, `xods` for spreadsheets, `xodp` for presentations. Rust and egui, one
binary each, nothing sent anywhere.

    xodt report.odt
    xods accounts.ods
    xodp deck.odp

There is a launcher too, for a shell or a script that has a file and does not
want to know which kind it is:

    odox anything.ods

## Why not just open it in LibreOffice

LibreOffice is an office suite and these are lightweight editors. The whole of
`xodt` starts in well under a second on a document a suite takes several to
load, and reading a document you were sent, fixing a sentence in it and sending
it on is most of what anyone does with one. The applications are also built to
one standard rather than to compatibility with another program: what they
implement is the OASIS OpenDocument specification, and a document that is valid
ODF is a document they are meant to open, whichever application wrote it.

Editing means changing what is there, not authoring. You can type and delete
text, split a paragraph and join two, enter a value in a cell, move a shape on a
slide and resize it, undo any of that, and save. You cannot apply formatting,
insert a table or a picture, edit a formula, or find and replace. What you did
not touch is written back as it was read, element for element, including the
parts these applications have no opinion about: the document you save is the
document you opened, with your change in it.

## Install

On Debian or Ubuntu, from the Excelano apt repository, which is where updates
come from:

    curl -fsSL https://excelano.com/apt/setup.sh | sudo sh   # one-time
    sudo apt install odox

`odox` is the launcher and it depends on the three applications, so that
installs the whole suite. To take only the ones you want, name them instead —
`sudo apt install xods`, say, and no application depends on another. amd64 and
arm64 both.

From crates.io, which gives you the binary and nothing around it — no desktop
entry, no icon, so a file manager will not offer it:

    cargo install xodt

The launcher on its own is `cargo install odox`, and there it really is on its
own: a crate cannot depend on a Debian package, so the applications are a
separate `cargo install` each.

From a clone, which is the same package the apt repository serves:

    cargo build --release
    ./packaging/debian/build-deb.sh
    sudo apt install ./dist/*.deb

Or from a clone without a package, which puts the binaries, the desktop entries
and the icons under `~/.local` so a file manager offers them:

    cargo build --release
    ./packaging/linux/install.sh          # --default to open ODF files with them

A Rust toolchain builds all of it and nothing else is needed: no C compiler, no
system libraries at build time, no code generator. `Cargo.toml` names the
toolchain version the build needs.

Windows and macOS are built and tested from this repository; the applications
are on the Microsoft Store and the Mac App Store as Odox Text, Odox Grid and
Odox Deck. `packaging/windows/README.md` and `packaging/macos/README.md` say
how each lane is run.

## Using them

Give a document on the command line or press Ctrl+O. Ctrl+R re-reads the file from disk, Ctrl+W closes it, and Ctrl+plus, Ctrl+minus
and Ctrl+0 change the zoom.

A window opens reading. Ctrl+E, or Edit mode in the Edit menu, turns editing
on, and a preference in the same menu opens every document that way. Ctrl+S
saves over the file you opened and Ctrl+Shift+S saves somewhere else; Ctrl+Z
takes an edit back and Ctrl+Shift+Z puts it back. Closing, opening another
document or quitting with changes unsaved asks first. Before a document is
saved, what is about to be written is read back and compared with what is in
the window, and a difference refuses the save rather than writing a document
that would not come back the same.

`xodt` shows the document as one continuous page at the width its page layout
asks for, with the headings listed beside it; clicking one scrolls to it. In
edit mode a click on a paragraph opens it as a text box where it sits: Escape
puts it back, Ctrl+Enter or a click elsewhere keeps what you typed, Enter
starts a new paragraph, and Backspace at the very start joins the paragraph
onto the one before it.

`xods` draws the sheet as a grid with the document's own column widths and cell
styles, one tab per sheet, and shows the formula behind whichever cell you pick.
Typing on the picked cell replaces it, Enter or F2 opens it with what it holds,
Enter commits and moves down, Tab commits and moves right, Escape puts it back,
and Delete clears it. A number is a number, `true` and `false` are booleans,
anything else is text. A cell that holds a formula is not edited, and once
anything in the sheet has changed every formula's result is drawn faint until a
spreadsheet application recalculates it.

`xodp` draws each slide at the size the document sets, with the speaker's notes
under it. In edit mode a click picks one of the slide's shapes, a drag moves it,
a drag on a corner resizes it, and a click on the text in one opens it as a
text box the way a paragraph opens in `xodt`.

Each application opens one kind of file and says so when handed another, naming
the sibling that reads it.

The window draws in the desktop's language where it has one. German is there;
`crates/odox-ui/po` is where another goes.

## Where things are

`crates/odox-core` reads and writes the format and has no window in it.
`crates/odox-ui` is everything a person sees, shared by all three. `crates/xodt`,
`crates/xods` and `crates/xodp` are one screen each over that. `crates/odox` is
the launcher, which draws nothing and links no toolkit. `corpus/` holds
the documents the tests read. `packaging/` turns a build into something
installable, one directory per platform. `DESIGN.md` is where every decision and
its reasoning live; `CLAUDE.md` is the short guide for a session working here.
