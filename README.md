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
slide and resize it, make text bold, italic, underlined or struck through, make
a paragraph a heading or a list item, undo any of that, and save. You cannot
apply other formatting, insert a table or a picture, edit a formula, or replace
text. What you did
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
edit mode a click puts the caret where you clicked and you type into the page
as it is drawn, formatting and all. Enter starts a new paragraph, Shift+Enter
breaks the line, Backspace at the start of a paragraph joins it to the one
before, and the arrows, Page Up and Page Down, Ctrl+Home and Ctrl+End, Ctrl+A,
Shift and the clipboard work across paragraphs the way they do in any word
processor. Ctrl+B, Ctrl+I and Ctrl+U, or the buttons over the page, make the
selection bold, italic or underlined, or what you type next when nothing is
selected; strikethrough is a button. Beside them are buttons that make the
paragraphs the selection runs over body text, a heading of the first, second or
third level, a bulleted list or a numbered one, and pressing a lit one takes it
off. Undo puts the caret back where the edit
was.

`xods` draws the sheet as a grid with the document's own column widths and cell
styles, one tab per sheet, and shows the formula behind whichever cell you pick.
Typing on the picked cell replaces it, Enter or F2 opens it with what it holds,
Enter commits and moves down, Tab commits and moves right, Escape puts it back,
and Delete clears it. A number is a number, `true` and `false` are booleans,
anything else is text. A cell that holds a formula is not edited, and once
anything in the sheet has changed every formula's result is drawn faint until a
spreadsheet application recalculates it. Dragging, or Shift with the arrows,
selects a range; Ctrl+C and Ctrl+X take it as tab separated text, as the cells
show it, Ctrl+V puts such text down from the picked cell, as the values typing
would make, and Delete empties it. A paste or a delete that reaches a cell it
cannot write, a formula or one under a merge, changes nothing.

`xodp` draws each slide at the size the document sets, with the speaker's notes
under it. In edit mode a click picks one of the slide's shapes, a drag moves it,
a drag on a corner resizes it, and a click on the text in one puts the caret
there to type into it as in `xodt`, within that shape. F5 starts the slideshow
at the first slide and Shift+F5 at the one in view: the slide fills the screen
on black, Right, Down, Space, Enter or a click go forward, Left, Up, Backspace
or a right click go back, and Escape ends it. The notes are not shown.

In `xodt`, a click on a link opens a web or mail address in the desktop's own
browser or mail program, or scrolls to a bookmark or heading in the document;
while editing it takes Ctrl and a click, a plain click being the caret. No other
kind of link is followed, whatever the document says.

Ctrl+F opens a search bar in any of the three, in reading or in edit mode:
Enter or F3 goes to the next match, Shift+Enter or Shift+F3 to the one before,
and Escape closes it. Every match is lit, `xodp` searches the notes as well as
the slides, and `xods` searches every sheet.

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
