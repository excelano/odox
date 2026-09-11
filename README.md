# odox

Three small desktop applications that open an OpenDocument file and show it to
you. `xodt` reads text documents, `xods` reads spreadsheets, `xodp` reads
presentations. Rust and egui, one binary each, nothing sent anywhere.

    xodt report.odt
    xods accounts.ods
    xodp deck.odp

## Why not just open it in LibreOffice

LibreOffice is an office suite and these are viewers. The whole of `xodt` starts
in well under a second on a document a suite takes several to load, and reading
a document you were sent is most of what anyone does with one. The applications
are also built to one standard rather than to compatibility with another
program: what they implement is the OASIS OpenDocument specification, and a
document that is valid ODF is a document they are meant to read, whichever
application wrote it.

They read and do not write. A file you open is never modified, and there is no
network code in any of them.

## Install

From a clone, on Debian or Ubuntu:

    cargo build --release
    ./packaging/debian/build-deb.sh
    sudo apt install ./dist/xodt_*.deb ./dist/xods_*.deb ./dist/xodp_*.deb

Or without a package, which puts the binaries, the desktop entries and the icons
under `~/.local` so a file manager offers them:

    cargo build --release
    ./packaging/linux/install.sh          # --default to open ODF files with them

A Rust toolchain builds all of it and nothing else is needed: no C compiler, no
system libraries at build time, no code generator. `Cargo.toml` names the
toolchain version the build needs.

## Using them

Give a document on the command line, drop one on the window, or press Ctrl+O.
Ctrl+R re-reads the file from disk, Ctrl+W closes it, and Ctrl+plus, Ctrl+minus
and Ctrl+0 change the zoom.

`xodt` shows the document as one continuous page at the width its page layout
asks for, with the headings listed beside it; clicking one scrolls to it.
`xods` draws the sheet as a grid with the document's own column widths and cell
styles, one tab per sheet, and shows the formula behind whichever cell you pick.
`xodp` draws each slide at the size the document sets, with the speaker's notes
under it.

Each application opens one kind of file and says so when handed another, naming
the sibling that reads it.

## Where things are

`crates/odox-core` reads and writes the format and has no window in it.
`crates/odox-ui` is everything a person sees, shared by all three. `crates/xodt`,
`crates/xods` and `crates/xodp` are one screen each over that. `corpus/` holds
the documents the tests read. `packaging/` turns a build into something
installable, one directory per platform. `DESIGN.md` is where every decision and
its reasoning live; `CLAUDE.md` is the short guide for a session working here.
