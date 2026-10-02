# odox — design

The authority on how these applications are built. `README.md` is the front
door. Sections are numbered because the code cites them.

## §1 Three applications, one core

The three formats share one content model: `text:p`, `text:span`, `text:list`
and `table:table` appear in a text document, a spreadsheet cell and a slide's
text frame alike, and the style system, the package layout, the metadata and
`draw:frame` are shared outright. What differs is the one element inside
`office:body` and how the body is addressed: a flow, sheets indexed by row and
column, a sequence of pages. So the core is large and each application is thin.
`odox-core` reads and writes the format and knows nothing about a window;
`odox-ui` is every pixel a person sees, including the renderer for the shared
content model; each of `xodt`, `xods` and `xodp` is one view and a `main`. They
are three binaries rather than one with three modes because the desktop
registers a handler per media type.

`odox` is a launcher and not an application. It takes a file, works out which of
the three reads it, extension first and the package's declared media type where
the extension cannot say, and on Unix replaces itself with that one. It links
`odox-core` and not `odox-ui`, so a command that runs for milliseconds carries
no graphics toolkit. Its Debian package depends on all three, unversioned, which
makes `apt install odox` the way to install the suite.

The dependency runs one way. `odox-core` does not depend on `odox-ui`, does not
link egui, and does not open files: a document is made from a byte slice and
turned back into a `Vec<u8>`, so one library serves a window, a sandboxed macOS
application and a test that never reads a disk.

`odox-fonts` answers which of the machine's faces a family a document names
resolves to, and `odox-pdf` lays a text document out onto pages and writes it as
tagged PDF (§12). Both sit beside `odox-ui` rather than under it: the window and
an export ask `odox-fonts` the same question and so get the same face, and
`odox-pdf` links neither egui nor anything that draws, which is how only `xodt`,
the one application that exports, carries the PDF writer.

`egui_richedit` sits below `odox-ui` and knows nothing of ODF: it is the caret,
the selection and the keys over paragraphs an application lays out itself, and
it reaches a document only through the `Model` trait the application implements.
It depends on egui alone, its tests hold it to that, and it is published as a
crate for any egui application.

## §2 What the build depends on

Pure Rust: a build that needs a Rust toolchain and nothing else cross-compiles
from Linux with no MSVC toolchain, builds on a runner with nothing installed, and
packages the same way on three platforms. The check is on the artefact, never
the manifest, because `cargo tree -i cc` is not empty in any eframe tree
(`wayland-backend` declares `cc` under a feature nothing turns on):

    objdump -p target/release/xodt | grep NEEDED

answers `libgcc_s`, `libm` and `libc` and nothing else. Vulkan, X11, Wayland and
xkbcommon arrive through `dlopen` at run time, which is why §9's Debian
dependencies come from a running process rather than from the linker.

The renderer is wgpu, which is eframe's default and the rest of the fleet's:
Direct3D on Windows, Metal on macOS, Vulkan here. The alternative eframe offers
is `glow`, over OpenGL, and it is the wrong one on two platforms — on Windows
OpenGL arrives with the graphics vendor's driver and is Microsoft's 1.1 stand-in
without one, below what egui accepts, so the application exits rather than
opening a window; and Apple deprecated OpenGL in 2018 and runs it as a
translation layer over Metal. Switching cost nothing this section claims: the
`NEEDED` list above is the same either way, because wgpu reaches Vulkan through
the same `dlopen` glow used for OpenGL.

One `unsafe` module, on one platform. macOS delivers a double-clicked document as
an Apple Event, and receiving one needs a single Objective-C method: `odox-ui`'s
`opened_document`, compiled only for that target, which is why `odox-ui` is
`deny(unsafe_code)` with one `allow` where every other crate is `forbid`. It is
in the shared crate because all three windows come through one `run`.

## §3 The document is kept, not summarized

A document is held as the XML tree it was parsed from. An element nobody here has
heard of keeps its attributes, its children and its position, and is written back
where it was found, because a reader that discards what it has no opinion about
becomes an editor that destroys documents the first time anything saves one.
`crates/odox-core/tests/roundtrip.rs` measures it: every entry of the package
comes back byte for byte, and parsing what the writer produced gives back an
equal tree. Attribute order, `<a/>` against `<a></a>`, comments, processing
instructions and inter-element whitespace all survive. Text is held unescaped and
escaped again in the canonical form, so byte identity of an XML part is not
claimed; tree equality is. Typed reading is a view over the tree: `Styles`
resolves a name through its inheritance chain, the three document types index a
body, and both hand back the underlying `Element`.

## §4 Styles

Named and automatic styles are one table keyed by family and name, because ODF
scopes a style name within its family. Resolution walks the inheritance chain
from the family's `style:default-style` down, so the nearest style wins, and the
answer is cached. Resolved properties are one type across every family, because a
cell carries paragraph properties and a paragraph carries character properties.
`fo:margin`, `fo:border` and their per-edge siblings are read as the shorthand
ODF defines: the whole sets all four edges and a per-edge attribute overrides
one. An edge nobody set is absent rather than zero, which is the distinction
between inheriting a border and having none.

## §5 The three bodies

A **text document** is a flow of blocks in the order they are read, which is the
order they are drawn; the first master page's layout gives the line width.

A **spreadsheet** is indexed rather than flattened. ODF writes a run of identical
rows or cells once with a repeat count, and `table:number-columns-repeated="16384"`
on a last column is ordinary, so a sheet keeps the rows it was given, each with
the range of row numbers it stands for, and a lookup is a binary search through
those ranges. Rows inside `table:table-header-rows` or a `table:table-row-group`
belong to the sheet as if they were the table's own. The used extent is where
content stops, not where the repeat runs stop.

A **presentation** is a sequence of pages whose shapes carry their own position
and size in the page's coordinate space, so the page is scaled to the window and
each shape is put where the document says. A slide is drawn back to front: the
ground, the master page's contribution, the slide's own shapes. A child of a
master page carrying a `presentation:class` is a slot and not a decoration: the
slide's own frame of that class takes its place. The class is the test;
`presentation:placeholder` is not written reliably and is not consulted.

## §6 Drawing

**Fonts are the machine's.** The applications carry none; a document names a
family and the machine is asked for it once, when the document opens, because
egui rebuilds its glyph atlas when the font definitions change. `fontdb`'s
generic defaults resolve to nothing on Linux, so the generics are pointed at
faces the machine has first, and the families with a metrically compatible
substitute are named so that a document asking for Times New Roman keeps its
line breaks under Liberation Serif. Both are in `crates/odox-fonts`, which an
export asks too, and which embeds the face it answers (§12). egui's own fonts,
which every chain ends in, cover few scripts, so a character in the document
that no face loaded has is drawn from the first face on the machine that has
it, added at the end of every chain; the export draws it from the same face.
The characters are read when the document opens, as the families are.

**What a document leaves uncoloured follows the window's theme**: unset paper
and ink turn dark together with a dark window, matching the chrome around
them, so a document that colours nothing stays legible in either. A colour the
document does set is drawn as set regardless — including text coloured dark
against a page left unset, which the theme cannot then save from landing on
now-dark paper.

**A hyperlink is the one exception**: every link is drawn in the theme's link
colour even where the document's own character style resolves one, because
that style is almost always an ODF producer's boilerplate rather than
something an author chose, unlike a heading someone coloured by hand.

**A proportional line height is a proportion of each run's own size**, so a
span set larger than its paragraph takes a taller line; an absolute one is the
same for every run. Resolved against the paragraph's size alone, a title whose
size lives on its span sat in a line thirteen points tall.

**Text in the page can be selected and copied**, by dragging, double-click,
triple-click and Ctrl+C. Outside edit mode the paragraph hands its laid-out
galley and its anchor to egui's label-selection plugin, and in edit mode to the
page editor of §11; either paints the galley at that anchor and keeps the
selection across paragraphs and scrolling because every paragraph reports to it
whether or not it is on screen. Both begin a selection only on a response that
senses drag, which a bare allocation does not.

**What is not drawn.** The window does not paginate: a page layout gives a
width, and page boxes, widows, floats and multiple columns are typesetting rather
than reading. An export is where pages are made (§12).
Tab stops advance by a fixed amount. Right-to-left text is drawn left to right.
A `draw:measure` is undrawn. A radial, ellipsoidal, square or rectangular gradient
is filled with the flat average of its two colours, and a tiled picture with
nothing. A turned shape's label is undrawn, because an upright paragraph inside a
box that is not upright says something the document does not.

**Shapes are drawn from their own geometry**: rectangles, ellipses, polygons,
polylines, lines, paths, connectors and custom shapes, with a solid fill, a
linear or axial gradient, or a stretched picture, and an outline. A `draw:path`
states SVG path data, an elliptical arc becoming the straight line to its end. A
connector is positioned by the two ends it joins, takes the route the producer
wrote beside them or the straight line, and has no area whatever its style says,
because LibreOffice writes `draw:fill="solid"` on every one. A turned shape
states where it is as operations in `draw:transform` and no `svg:x`; they apply
to a point left to right, the opposite of SVG, with the angle in radians, and
`odox-core`'s `place` module records which fixture settles which.

**A shape's label is paragraphs of its own**, placed by
`draw:textarea-vertical-align` and laid out twice because its height is known
only after the wrapping is. **A frame may state its picture more than once**,
best first, and the first that decodes is the answer. **A custom shape states a
path in `draw:enhanced-geometry`**, in a space of its own, with numbers that may
be formulas over the space's edges and dragged adjustments; `odox-core`'s `draw`
module evaluates the formulas and flattens the path into polylines, because how
finely a curve is broken depends on that space and not on the window. **A fill is
cut into triangles by clipping ears**, because a fan from the first point is right
only for a convex outline. **`draw:fill` and `draw:fill-color` inherit
separately**: the colour names what a solid fill would use and does not turn the
fill on.

## §7 The window

One shell, `crates/odox-ui/src/shell.rs`, with a `View` for the part that
differs. The shell owns the menu, the keys, the file dialog, the error line, the
zoom and the side panel, and every read from disk, so a view is handed bytes and
never a path. The desktop's light and dark setting is read through the XDG portal
on Linux, because `winit` returns `None` from `system_theme()` there;
`src/system_theme.rs` is slipcase-desktop's module, unchanged but for the
thread's name.

A view can take the whole window. `View::presenting` is the slideshow's flag:
while it is true the shell draws no menu, no panel and no bar, takes none of
its keys, puts the window full screen and fills the central panel with black,
and the view draws one slide as large as the screen allows, keeping its shape.
It is the view's own mode, so the keys that move through a show are the view's.

## §8 Language

Every string a person reads lives in `odox-ui` and goes through `potext`'s `t`.
There is one catalogue for the suite, under `crates/odox-ui/po`, because
`potext::catalog!` gives the storage to the crate that invokes it and every
message is invoked from `odox-ui`; the extraction globs every crate. An
application contributes its own name, untranslated, and the name of the format
it opens, a `const` built before `run` puts a catalogue in force, so it is
wrapped in `i18n::mark` and looked up through `t` where it is drawn. The desktop
entries carry the same languages in `Comment[..]` and `GenericName[..]`, because
a file manager reads those and never the catalogue. The launcher links no
`odox-ui`, and its three sentences are English. German terminology follows the
fleet glossary in Comma's `de.po` and LibreOffice's own German for the rest.

`en-x-pseudo`, compiled into debug builds alone, returns every message accented,
bracketed and 40% longer, and shows a string that never went through `t`, a
sentence the catalogue never saw, and a label built to the width of English.
`po/update-po.sh` re-extracts and merges; CI refuses a push whose template is
behind the source. `msgmerge` marks a reworded message `#, fuzzy` and `potext`
refuses to load one, so a translation is current or visibly absent, never
silently wrong, which is the property a key-value catalogue cannot offer.

## §9 Size, speed and packaging

The release profile is fat LTO across one codegen unit with no symbol table.
Unwinding stays, because a panic in a document reader should reach a dialog
rather than kill the window with no message.

Each application's `build.rs` embeds the Windows application manifest, which
declares DPI awareness before any of the program's code runs, and stages the
window icon; it is two linker arguments and no resource compiler. The three
copies are byte identical and CI refuses a push where they have drifted. The
manifest is above each crate's directory, which `cargo package` does not carry,
so a build that cannot find it skips it with a warning. `+crt-static` is in
`.cargo/config.toml` for the MSVC targets, and `packaging/windows/check-imports.ps1`
refuses any import that does not ship with Windows.

The Debian dependencies are written by hand from `packaging/linux/check-libraries.sh`
run once on each display backend, because the binary links three libraries and
dlopens the rest; both backends are compiled in, so both sets are declared. No
package declares a media type, because shared-mime-info already does; the
desktop entries register that these applications can open one.

## §10 What "compliant" is measured as

Not that a document renders the way another application renders it; the
applications are written to the specification. What is measured is that a
document comes back unchanged, over a corpus, on every part of every package.
`corpus/libreoffice/` holds documents a real producer wrote, rebuilt from
committed plain-text sources by `build.sh`; `corpus/make-fixtures.py` writes a
spreadsheet and a presentation by hand to exercise a repeated row, a merged cell,
a formula with its cached value and a slide with a text frame. The tests walk the
corpus directory rather than naming its documents.

A formula cell carries `table:formula`, `office:value` and a `text:p` holding the
value as last displayed, so a viewer needs neither a number-format engine nor a
formula evaluator: it shows the string the producer formatted. Nothing gates on
`office:version`; the reader accepts what it is given.

## §11 Editing

A window opens reading and is put into edit mode by a person, with Ctrl+E or
the Edit menu, or by the one preference the suite keeps. Outside edit mode a
view draws and selects and never opens an editor; the spreadsheet is the
exception by convention, where typing on the selected cell edits. The
preference lives in one small file, `odox/settings.toml` under the platform's
configuration directory, written only when a preference is changed in the menu,
so an installation nobody has configured has no file.

**A cell is edited in place.** Typing on the picked cell replaces it, Enter or
F2 opens it with what it holds, Enter commits and moves down, Tab commits and
moves right, Escape puts it back, and Delete clears it; the text box sits in
the cell in the cell's own font. What is typed is read the way a spreadsheet
reads it: a number is a number, `true` and `false` are booleans, anything else
is text, and a formula is not recognised. A cell that holds a formula is not
opened, and once anything in the document has changed every formula's cached
result is drawn faint, because which of them went stale cannot be told without
evaluating them and a spreadsheet application recalculates on opening the
file. Leaving a cell as it was is not an edit, so stepping through a currency
does not retype it as a number.

**A selection is a range, and the clipboard carries it as text.** A press picks
the cell under it, Shift or a drag stretches the selection to another, and the
range is the rectangle between the two corners. Copy and cut hand over the cells
as tab separated rows, each as it is displayed, which is what every other
program can read; paste reads the same shape back, each cell as a typed value
would be read, from the top left of the selection. A paste, a cut or a delete is
one undo step and is all or nothing: if any cell it reaches holds a formula or
lies under a merge, none is written and the cell bar says which. Emptying a
range leaves cells the document never wrote unwritten.

**Nothing is written until Save, and then only the file that was opened or the
one Save As named.** The shell owns the write as it owns the read. Before the
bytes touch the disk they are read back and compared with the tree they were
written from, and a difference refuses the save and says so: the round-trip
tests make the same claim over the corpus, and a person's document is not in
the corpus. On Linux and Windows the bytes go into a `.part` file beside the
target and are renamed over it, carrying the target's permissions, so the file
is either what it was or what was written. macOS writes in place, because the
sandbox grant covers the file and not its directory.

**An edit changes one subtree and nothing beside it**, and the tests say so by
mutating a corpus document and comparing everything else. A name written into
the tree takes the prefix the document declares for its namespace on the
content root, so a document that spells `text:` as `t:` is written its own way.
A cell in a run the document wrote once with a repeat count is split into the
run before, the one, and the run after, with the counts fixed, so the cell
changes and its neighbours in the run keep what they had; a cell or row past
what the document wrote is created, with one repeated empty run filling the
gap. A cell that holds a formula is refused, and so is one under a neighbour's
span. A shape's `svg:x`, `svg:y`, `svg:width` and `svg:height` are written in
the unit each was read in.

**A slide's shape is moved by hand.** In edit mode a click picks one of the
slide's own shapes, a drag moves it, and a drag on a corner handle moves that
corner with the opposite one fixed, never thinner than a point. The hit is
tested where the button went down, because egui reports a drag only once the
pointer has travelled, and a press on a handle is a press on the handle. What
is not picked: a shape placed by `draw:transform`, which states no corner; a
line, placed by its ends; a connector; a group; and everything the master page
contributes. Each drag is one undo step, recorded when it begins.

**A text document is edited on the page as it is drawn.** In edit mode a
click puts a caret into the paragraph under it, and typing goes into the tree
before the next layout, so the paragraph is drawn with its own formatting as
it changes and what is typed into a bold word is bold. Enter splits the
paragraph, Shift+Enter is a line break inside it, and Backspace at its start
or Delete at its end joins it to its neighbour; the arrows, Home and End
cross from one paragraph into the next, Up, Down, Page Up and Page Down
keeping to the column they began in, and Ctrl+Home, Ctrl+End and Ctrl+A reach
the whole document. A selection made with Shift or a drag runs across paragraphs, and
Ctrl+C, Ctrl+X and Ctrl+V go through the clipboard, a pasted line break
beginning a paragraph. `egui_richedit` does the caret and the keys;
`odox-ui`'s `FlowModel` names each paragraph by its path under the body, in
the order the flow draws them through lists, table cells and text boxes, and
turns each edit into an `odox-core` edit. A paragraph inside a frame
anchored in a paragraph is drawn from a clone and is not edited in place.

**A range joins its ends across list structure and not out of a table.**
Replacing a selection, or Backspace at a paragraph's start, joins what is
left of the last paragraph onto the first and removes what lay between,
list items and lists emptied by that included; the first paragraph keeps its
style and its place. Enter in a list item begins a new item after it, taking
what followed in the item, and Enter in an empty one takes it out of the
list, which is split around the paragraph left in its place. A table or a frame the range wholly contains goes
with the rest. A range with an end inside a table, a cell or a frame it does
not wholly contain takes the selected text out of each paragraph it covers
and leaves every paragraph and cell standing, so Backspace at the start of a
cell does nothing.

The caret stands in the paragraph's flat text, and what is drawn is not that
text character for character: a tab is drawn as spaces, a note as its
citation, a field as its value. The renderer records each such piece as it
lays the paragraph out, so a click on one lands at its edge and the caret
steps over it, and a test holds the two lengths equal for every paragraph in
the corpus.

**A slide's label is edited the same way, on the slide.** Its paragraphs are
named by their path from the page, which begins with the shape's index, and
the caret keeps to the shape it is in: moving and joining never cross into
another shape, and a click in another moves it there. A drag on a slide moves
the shape, so a label's paragraphs take clicks alone and the caret goes down
on the click; Shift with the arrows selects. The arrows and the page keys step
through the slides only while nothing has the keyboard.

**Text is made bold, italic, underlined or struck through**, and nothing else
is formatting a person can apply. Ctrl+B, Ctrl+I and Ctrl+U, or the row of
buttons over the page in edit mode, give the selection the mark or take it off
where all of it already has it; strikethrough has a button and no key. With a
caret and nothing selected, the mark goes to what is typed next at the caret,
and moving the caret forgets it. A button is lit where the whole selection has
its mark, and for a caret where the text typed there would. Each is one undo
step, and a mark put on with typing is the same step as the typing. A
spreadsheet cell is not formatted.

**A mark changes spans and automatic styles and nothing else.** The range's
ends are cut into the nodes they fall inside. A span the range holds whole has
its style changed; any other run of what the range holds is wrapped in a new
span inside whatever holds it, a link included, so no link or span the
document had is taken apart. A property is written only where it changes what
is drawn: a span inside the range whose own style says otherwise stops saying
it, a span left saying nothing gives way to its contents, and taking a mark off
text it was put on gives back the paragraph it was. Bold and italic are written
for Asian and complex scripts too. A span's style is an automatic style in
`content.xml`, one the document already holds wherever one says the same, and
otherwise a new one named `T` and the first number no text style has; `Styles`
learns of each as it is written and forgets none, so an undo never frees a name
for a different style. A document with no automatic styles is given the
container before its body. `crates/odox-core/src/edit/format.rs` is the
mechanism; `tests/format.rs` formats a range of every paragraph in the corpus
and holds everything outside it equal.

**A paragraph is edited through its flat text**, built from the tree and not
from the renderer's layout: a `text:s` is its spaces, a `text:tab` a tab, a
`text:line-break` a newline, a span's or a link's contents the paragraph's own
characters, and everything else in the paragraph — a bookmark, a frame, a
field, a note — contributes nothing and stays where it was. Replacing a range
deletes the characters from the nodes that hold them, puts the new text into
the text node at the start of the range, then writes whitespace the way ODF
requires. A split carries a zero-length element at the split point to the
first half. `crates/odox-core/src/edit.rs` is the map; `tests/edit.rs`
measures it over every paragraph of the corpus.

**Undo is a stack of snapshots** of the content tree, bounded at a hundred.
Every edit records the tree as it stands and then mutates, except that a run
of typing, or of deleting, on the page is one step, ended by a move of the
caret or an edit of another kind. A snapshot also holds where the caret stood,
so undo puts the caret where the edit began and redo where it was when the
edit was undone. The document is modified when the stack is not at the depth
it had when the file was last read or written, so undoing back to that depth
is a document with nothing to save.
Close, Open, Reload, Quit and the window's own close button ask before a
modified document is thrown away.

**Find is a view of the text and not an edit.** Ctrl+F opens a bar under the
menu in any of the three windows, in reading or in edit mode; Enter and F3
move to the next match, Shift+Enter and Shift+F3 to the one before, and Escape
closes the bar. Case is not significant and the query is not a pattern. A
match is a range of characters in a paragraph's flat text, the string the
segment map reads and the page editor's offsets count in, so the characters
lit are the ones an edit there would change. The view answers how many there
are and draws them: a flow paints each behind its text, the current one in
orange and the rest in yellow, and scrolls to the current one when the search
has just moved to it. A sheet tints the cells that hold the query and picks
the current one; a deck searches every slide's labels and every slide's notes,
and moves to the slide, and opens the notes, that the current match is in.
The matches are looked for again when the query changes or the text does, and
at no other time.

**Replace is an edit over the matches Find holds.** Where the document can be
changed, the bar has a second row: Replace takes the current match and Replace
all every one, each as a single step to undo. A text document and a deck replace
through the page editor's own replacement, last match first so the offsets of
the ones before hold, so what replaces a match takes its formatting. A sheet
replaces only in cells whose value is text: a number, a date or a boolean would
be read again as something else or lose its format, and a formula is never
edited, so those matches are left and the bar says how many. A replacement that
still matches the query is passed over, so Replace moves on.

**A link is followed only to places a person means by one.** A paragraph's
links are the `text:a` elements in it, each covering a range of the flat text
the page editor counts in; the character under the pointer, found through the
same offset map the caret uses, says which. Reading, a click follows it;
editing, Ctrl and a click do, because a plain click puts the caret down. The
address is shown in a tooltip before anything is done. `http`, `https` and
`mailto` are handed to the desktop's own handler, and `#Name` scrolls to the
bookmark of that name, or to the heading whose text is `Name` where it is
written `Name|outline`. In a deck `#Name` goes to the slide whose `draw:name` is `Name`. Every other scheme, `file:` and the desktop's
registered handlers among them, is ignored, because a document is not trusted
with what the machine will run. The application makes no request itself.

**A paragraph's kind is changed, never its text.** `edit::set_heading` turns a
`text:p` into a `text:h` with an outline level, or back, and `edit::set_list`
wraps paragraphs in `text:list-item`s or takes them out; neither adds or
removes a paragraph, so a paragraph keeps its place in the order the flow draws
them in, and that ordinal is how the selection is found again afterwards. A
heading takes the paragraph style the document gives its level, by
`style:default-outline-level` and then by the name `Heading_20_N`; a document
with none is given an automatic style in `content.xml` that says size and
weight, shared by every heading of the level. A heading taken off becomes
a `text:p` in the style most of the body's paragraphs have. A list takes a list
style the document already has of the kind, bulleted or numbered, and failing
that one is written, six levels with the usual indents. Paragraphs side by side,
layout whitespace between them aside, become one list so that numbers run on;
taking a paragraph out of a list splits the list around it, the half after
continuing the numbering, and anything else its item held, a nested list,
stands beside it. One pass over the selection is one undo step, and the buttons
are lit where every paragraph in the selection is one. The Format menu is the same commands by name, with the marks' shortcuts beside them, so each has a route that needs no pointer; a menu is drawn before the page, so it draws from what the page last said was lit and hands its choice to the page to apply with the model. Enter at the end of a heading starts body text, in the style a heading
taken off takes; Enter inside one leaves two headings.

**A page is told to assistive technology a paragraph at a time.** Each paragraph
the flow draws is a node carrying its text as the rows the galley laid out, with
the role of a heading and its level where the paragraph is a `text:h`, and
paragraph otherwise; the page editor adds where the caret or the selection is in
the paragraph that holds it, counted in the galley's characters through the same
offset map the caret uses. A selection that began in another paragraph is
reported as a caret at its end here, because one node cannot name a position in
another. Slide labels go through the same flow and so are told the same way. A
sheet's grid is painted cell by cell and is not yet.

**A copy keeps its formatting inside the window it was made in.** The system
clipboard carries plain text, so the editor puts the selection there as text and
keeps beside it a `Fragment` its model made: the covered paragraphs cut to the
range, each with its own style and kind, and the spans and links in it, without
the ids, bookmarks, frames and notes that name one place in a document and would
then be there twice. A paste of exactly the text it put on the clipboard is a
paste of the fragment; any other text, a copy from another program or from
another window, is plain. The first pasted paragraph merges into the paragraph at
the caret and takes its style, as typed text would; the ones between keep their
own; the last merges with what followed the caret and keeps the paragraph's
style; in a list each is an item. A model that keeps no formatting leaves the two
`Model` methods as they are and every paste is plain. Across windows needs HTML
on the system clipboard and is not done.

**A deck's slides can be duplicated, deleted and moved, and not added.** A slide
is a `draw:page` with its notes, shapes and animations, named by `draw:name`, and
that name is what links, custom shows and the start page refer to it by. Moving
swaps it with the neighbouring page and so changes nothing that refers to it.
Deleting removes the page and takes its name out of the start page and the custom
shows that list it, a show left with none going; the last slide is refused.
Duplicating puts a copy after the original with a name no slide has (the
original's and a number) and a new `xml:id` and `draw:id` for everything in it,
the notes included, with the references that name those ids inside the copy
(`draw:start-shape`, `draw:end-shape`, `smil:targetElement`) pointing at the
copy's own. Styles are shared with the original, which is safe because a
format edit picks or writes a style and never changes one that another span
uses. A blank slide is not offered: it would hold only the master page's
decorations, and nothing here adds a shape to fill it, so a duplicate is how a
new slide is made. The Slide menu offers the four in edit mode, and each is one
undo step with the view following the slide.

**Rows and columns can be inserted and deleted, with the formulas that name
them following.** A row or a column is not always one element, since
`table:number-rows-repeated` and `table:number-columns-repeated` stand for many,
so an insert splits a run and a delete shortens one. An inserted row or column
is a copy of its neighbour with what it held cleared and its formats kept, and a
covered cell in the copy is an ordinary one, because a merged region does not
grow. The whole change is made to a copy of the content and replaces it only
when all of it can be made. It is refused when its line goes through a merged
region (one wholly inside a delete goes with it); when the document holds
something that names a range of cells and is not moved (a non-empty
conditional-formats, content-validations, database-ranges, data-pilot-tables or
consolidation, a chart or object, a print range, a shape anchored to a cell); or
when a formula cannot be moved. Formulas are text and every reference in one is
inside square brackets, so moving one is shifting what is in them and leaving the
rest alone: each end of a range on its own, so an insert inside a range grows it
and one just below it does not, an end of a range deleted becomes the nearest row
left, a reference to a cell that is deleted is refused, and so is a range with
nothing left. Named ranges and the base cells of named expressions move the same
way. A reference this reading cannot take, another document's or a range across
sheets, is refused, because a wrong formula looks right and nothing here
evaluates one. The rows or columns are changed before the formulas are shifted,
so a formula that is deleted with its row is not asked about what it pointed at.

## §12 Export

**A text document exports as PDF/UA-1**, the accessible profile of PDF, from
`xodt`'s File menu and nowhere else. The label says PDF/UA because the output is
checked: krilla, the writer, runs its PDF/UA-1 validation as it writes and fails
the export rather than producing a file that only claims to conform, and
`crates/odox-pdf/tests/conformance.rs` puts every `.odt` in the corpus through
veraPDF's ua1 profile. veraPDF is a Java program and not a build dependency, so
the test says it skipped the check where it is not installed;
`packaging/verapdf.sh` runs the same check over any documents by hand.

**The PDF holds what the window shows, in the same order.** `odox-pdf` reads the
body into blocks with the same decisions the window's flow makes, which live in
`odox-core` for both to call: which elements are blocks and which pass their
text through, how a list level writes its numbers, a tab as four spaces, a
picture anchored in a paragraph after it. An object with no picture of its own,
which the window shows as an empty box, is left out. A note is the one thing
the PDF holds that the window does not show: its citation stays in the text, and
its own text goes to the end of the document with the other notes, in the order
they are cited, below a short rule, each tagged as a Note with its citation as
its label.

**The window has no pages; an export does.** The body is laid out at the text
width of the first page style and filled into pages of its size and margins.
Text is shaped by rustybuzz in the faces `odox-fonts` resolves, a character the
face lacks in the first face on the machine that has it, and left to right as the
window draws it. Lines break where the Unicode line breaking algorithm allows, a
word wider than the line where it has to. A proportional line height is a share
of the face's own line height, which is what ODF means by it. A page is filled
with whole pieces: a line, a table row, a picture. A heading stays on the page
with the line after it, a page break the document asks for is taken, a table's
header rows are drawn again above the rest of it on a new page, and only a piece
taller than a page is cut. There are no headers, footers, widow control, floats
or columns. The window and the PDF measure text with different shapers, so a
paragraph can wrap a word differently in each; there are no page breaks in the
window for the PDF's to disagree with.

**Everything drawn is tagged as what it is, or marked as decoration.** The
structure tree is built during layout, in reading order. Headings are H1 to H6,
numbered by their depth among the headings above them, so that the first is a
level 1 and none skips a level as PDF/UA asks; a document numbered without gaps
keeps its own numbers, and what is drawn is not changed. Paragraphs are P;
lists are L, LI, Lbl and LBody with their numbering; tables are Table, TR, TH
for the header rows and TD, with column and row spans, and an empty cell is
still a cell. A picture is a Figure with its alternative text. A link to an
address outside the document is a Link with its annotation, whose text is the
address; a link to a place inside it is read as its text. Backgrounds, borders,
underlines, a repeated table header and a decorative picture are artifacts. The
title is `dc:title`, or the file's name; the language is the default paragraph
style's, then `dc:language`, then the language of the person exporting. The
outline is the headings.

**A picture has to say what it shows, or that it is decoration.** Its
alternative text is the frame's `svg:title`, or failing that its `svg:desc`; a
decorative one is marked as `LibreOffice` marks it, `loext:decorative` in the
frame's graphic style, so a document says the same in either application. An
export with a picture that says neither is refused, and `xodt` asks about each
one first, with the picture beside a field for its text and a box for
decoration; the answers are written into the document as one step to undo, and
a document that does not declare the SVG or `LibreOffice` namespace is given the
declaration. Nothing unanswered is written under the PDF/UA label.

**A font is embedded only where its licence allows it.** A face whose OS/2
flags restrict embedding or forbid subsetting is replaced by its metric
substitute or the generic family, and the export says so when it finishes. A
family with no embeddable face at all, and a character no face on the machine
has, refuse the export by name, since a PDF/UA file may hold no glyph that stands
for nothing.
