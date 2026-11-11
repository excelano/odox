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
resolves to, and `odox-pdf` lays a document out onto pages and writes it as
tagged PDF (§12). Both sit beside `odox-ui` rather than under it: the window and
an export ask `odox-fonts` the same question and so get the same face, and
`odox-pdf` links neither egui nor anything that draws.

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
Direct3D on Windows, Metal on macOS, Vulkan here. It reaches Vulkan through
`dlopen`, so it adds nothing to the `NEEDED` list above. On Windows it is
Direct3D 12 alone, unless `WGPU_BACKEND` names another backend: wgpu's
default set includes Vulkan and OpenGL, and creating a Vulkan instance loads
every installed Vulkan driver into the process, where a faulty one ends it
before a window opens. A machine with no usable graphics card still draws,
through the Microsoft Basic Render Driver.

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
`presentation:placeholder` is not written reliably and is not consulted. The
footer, date and slide-number slots are the exception, drawn from the master
with their field filled in: the footer and the date a slide names in
`presentation:use-footer-name` and `use-date-time-name`, unless its style turns
them off, and its number among the slides only where its style turns it on, as
`LibreOffice` draws them; a date that follows the clock shows today, in UTC. A
frame holding a table draws the table, and not the picture of it the producer
keeps beside it. A
shape's text is laid over the shape's own style: where a paragraph's style and
its spans do not say, its face, size, weight and alignment are its frame's, the
presentation style of a placeholder or the graphic style of a drawing, which is
where `LibreOffice` keeps a typed title's size. The window and the PDF lay it
the same way.

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
The characters are read when the document opens, as the families are, and
again as each character a person types, pastes or composes arrives.

**What a document leaves uncoloured follows the window's theme**: unset paper
and ink turn dark together with a dark window, matching the chrome around
them, so a document that colours nothing stays legible in either. A colour the
document does set is drawn as set regardless — including text coloured dark
against a page left unset, which the theme cannot then save from landing on
now-dark paper. Pure black is the exception and counts as no colour, in text and
in borders: spreadsheet producers write it into every cell, and word processors
onto the border of every table they insert, whether an author chose it or not,
so left as set it would put most real workbooks and tables black on a dark
window.

**Uncoloured text on a ground the document paints takes the ink that stands out
against it.** A cell's or a paragraph's fill, a slide's background and a
shape's fill are the document's own colours, so the theme's ink, chosen for the
theme's paper, can land light on light. Text there is drawn in whichever of the
light and dark palettes' inks has the higher contrast with the ground, and its
links in that palette's link colour, which is what ODF's automatic font colour
means. A gradient is judged by its middle colour; a picture is not judged, and
text on one keeps the theme's ink.

**A hyperlink is the one exception**: every link is drawn in the theme's link
colour even where the document's own character style resolves one, because
that style is almost always an ODF producer's boilerplate rather than
something an author chose, unlike a heading someone coloured by hand.

**A proportional line height is a proportion of each run's own size**, so a
span set larger than its paragraph takes a taller line; an absolute one is the
same for every run.

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
A tab goes to the paragraph's next stop (`flow.rs`'s `place_tabs`): the stops
its style sets, left, right, centre or on a decimal character, with a leader
of dots or a line where the stop has one, and past them the default stops the
default style spaces evenly, 1.25 cm apart where it says nothing; stops count
from the paragraph's left margin, as `LibreOffice` counts them, and
`corpus/libreoffice/tabs.odt` is the measure. The PDF export places a tab the
same way (`odox-pdf`'s `text.rs`), its leader written as text a reader hears.
Right-to-left text is drawn left to right. Small capitals are capitals at 80%
of the size. In them and in text a style puts in capitals, `ß` is `ẞ`, as
`LibreOffice` writes it, and a letter whose capital is two letters keeps its own
form, so what is drawn is as long as the text the caret moves through. Ruby is written after the text it
annotates, in parentheses, `漢字(かんじ)`, rather than set above it.
A `draw:measure` is undrawn. A turned shape's label is undrawn, because an upright paragraph inside a
box that is not upright says something the document does not.

**Shapes are drawn from their own geometry**: rectangles, ellipses, polygons,
polylines, lines, paths, connectors and custom shapes, with a solid fill, a
gradient, or a picture, and an outline. A gradient of any of ODF's six styles
runs as `LibreOffice` draws it (`Gradient::level` in `odox-core`, measured on
`corpus/libreoffice/gradients.odp`); the four that radiate from a point are
drawn as a picture of the gradient stretched over the shape, in the window and
in an exported PDF alike. A picture is stretched
over the shape, or laid at the style's size or its own, its pixels at the
resolution its header gives or 96 to the inch, against the centre unless the
style names a corner or an edge, and repeated across the shape or placed once,
as `LibreOffice` lays it (`Tiling` in `odox-core`, measured on
`corpus/libreoffice/fills.odp`); a fill that says nothing of repeating
repeats. A `draw:path`
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
on Linux (`src/system_theme.rs`), because `winit` returns `None` from
`system_theme()` there.

The side panel is an outline in a text document and a slide list in a deck. A
view says whether it has one for the document open, and the View menu offers
the panel only then, so a spreadsheet, whose sheets are tabs along the bottom,
or a text document with no headings, has no checkbox that does nothing.

**A text document pages with buttons under it.** Page up and Page down move the
view nine tenths of the window, so the line read last is still in sight, and
grey out at the top and the bottom. The Page Up and Page Down keys do the same
while no caret is taking them, and Home and End, with or without Ctrl, go to
the top and the bottom of the document; with a caret they move it, as §11
says.

A view can take the whole window. `View::presenting` is the slideshow's flag:
while it is true the shell draws no menu, no panel and no bar, takes none of
its keys, puts the window full screen and fills the central panel with black,
and the view draws one slide as large as the screen allows, keeping its shape.
It is the view's own mode, so the keys that move through a show are the view's.

**File > New, and Ctrl+N, starts a blank document** in a window that has none,
and otherwise starts another copy of the application with `--new`, which opens
one in a window of its own: a window holds one document, as it does for Open.
On macOS the copy is not a child process, because the sandbox kills a child
signed with the full sandbox entitlement before it reaches `main`: a bundle asks
`NSWorkspace` for a new instance of itself, and since Launch Services drops the
arguments a sandboxed application passes, the request travels as a marker file
in the temporary directory the instances share (`odox-ui/src/instance.rs`).
`odox_core::blank` writes each kind as a whole package that is then read like
any other: a text document with one empty paragraph, a spreadsheet with one
sheet named "Sheet1" in the reader's language, a deck with one 16:9 slide and
an empty title, on a master that places a title and a text frame in the common
paragraph styles "Title" and "Text". The default text style is in the reader's language, and the page
is Letter where the reader's country prints on it (the United States and
Canada) and A4 elsewhere. A blank document opens in edit mode, untitled, with
the caret in the first paragraph where the view has one; Save asks where to
write it, offering "Untitled". On a slide being edited, an empty title or text
placeholder says faintly what it is for, and a click anywhere on it puts the
caret in it.

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
`po/update-po.sh` re-extracts and merges. `msgmerge` marks a reworded message `#, fuzzy` and `potext`
refuses to load one, so a translation is current or visibly absent, never
silently wrong, which is the property a key-value catalogue cannot offer.

## §9 Size, speed and packaging

The release profile is fat LTO across one codegen unit with no symbol table.
Unwinding stays, because a panic in a document reader should reach a dialog
rather than kill the window with no message.

Each application's `build.rs` embeds the Windows application manifest, which
declares DPI awareness before any of the program's code runs, and stages the
window icon; it is two linker arguments and no resource compiler. The three
copies are byte identical. The
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
value as last displayed, so a document opens showing the string the producer
formatted, and nothing is worked out until a value changes. Nothing gates on
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
the cell in the cell's own font, taking every key from the first, however fast
they come. What is typed is read in the cell's number format, the data style its
cell style or its column's default cell style names: `1234.5` or `$1,234.50` in
a currency cell is the amount, with the cell's currency or the one its symbol
names, `15` or `15%` in a percentage cell is fifteen percent, `3/9/26` in a
date cell written month first is the ninth of March, `9:30 pm` in a time cell is
that time, and the separators are those of the format's language and country.
The cell then shows the value as the format writes it, grouping, decimals,
symbol, the section a `style:map` condition chooses, and month, day and quarter
names, so a typed
value looks as LibreOffice would show it; `corpus/libreoffice/formats.ods` is
the measure, and `codes.ods`, which `LibreOffice` builds from format codes
through `src/codes.py`, measures fractions, embedded text, engineering
notation, quarters and weeks, elapsed and fractional times and the General
format. General writes up to fifteen significant digits and switches to an
exponent at 2^53 and for small numbers as `LibreOffice` does, independent of
the column's width, where `LibreOffice` also shrinks a number to fit its
column. Separators, grouping, the names, `TRUE` and `FALSE` and the
currency of a new amount come from `odox-core`'s `locales.rs`, which
`locale.py` writes by asking `LibreOffice`, for 46 locales; a country outside
it takes the first locale of its language, and a language outside it is
written as United States English. What does not fit the format, a number with
its separators in another language's order among it, and anything typed into a
cell with none, is read
the way a spreadsheet reads it: a number is a number, a number
with `%` a percentage, `true` and `false` are booleans, and anything else is
text. A cell opens for editing with what reads back to its value: a percentage
with its `%`, a date or a time as its format shows it, a formula as it is typed.
The colour a format gives a section, red for a negative amount, is the colour
the value is drawn in, in the window and in the PDF.
Leaving a cell as it was is not an edit, so stepping through a currency does
not retype it as a number.

**What begins with `=` is a formula** (`formula/typed.rs`). It is typed as a
spreadsheet takes one, references bare, `A1`, `$A$1`, `A1:B3`, a sheet named
with a dot or an exclamation mark, `Data.B2`, `Data!B2`, `'My sheet'.A1`, and
arguments parted by `;` or `,`, and is stored as ODF writes it,
`of:=SUM([.A1:.A3])`, and worked out at once. It reads back, in the editor and
in the cell bar, as `LibreOffice` shows it, `=SUM(A1:A3)`, `=$Data.B2*3`. A
formula that cannot be read stays in the editor with the cell bar saying so,
until it is put right or Escape leaves the cell as it was; one that reads but
calls a function outside the core is kept for the application that opens the
file next, and drawn faint. A value typed or pasted over a formula takes its
place.

**Cells are filled down and across from the ones beside them**
(`sheet/fill.rs`). Sheet > Fill down, or Ctrl+D, fills the selection from its
first row, and Fill right from its first column; a selection one row deep or
one column wide is filled from the row above or the column to its left. Fill
right has no key, because Ctrl+R is Reload. The small square at the
selection's far corner is dragged down or right to fill past it, and a drag
also goes on with a series the selection starts: numbers a steady step apart,
dates a steady number of days apart, or of months where each falls on the same
day of the month (the last day where a month is shorter), text ending in a
number (`Item 1`, `Q03`), and day and month names in English and German, kept
in their case. One cell starts a series of
names or numbered text, and a lone number or date is copied, as in every
spreadsheet. A formula is copied with its relative references moved and its
absolute ones kept, one moved off the sheet's edge becoming `#REF!`, and every
cell takes its source's cell style. A fill over a merged cell is refused. Each
fill is one step to undo, and what reads the cells it wrote is worked out again.

**A block's rows are sorted by one of its columns** (`sheet/sort.rs`). Sheet >
Sort… asks which column, ascending or descending, and whether the first row is
a header, guessed where it is all text above a row holding a number or a date.
The block is the selection, or, where the selection is one cell, the filled
cells around it as far as no wholly empty row or column parts them. Numbers,
dates and times come before text, which goes by its letters whatever their
case, the order reversed for a descending sort, and empty cells go last either
way; rows that tie keep their order. A row's cells move together with their
cell styles, and a formula moves as a copy does, its relative references going
with it, as `LibreOffice` and Excel sort unless asked otherwise. A sort across a
merged cell is refused. Each sort is one step to undo.

**A cell's comment is shown and written** (`sheet/comment.rs`). A commented
cell has a small red corner; its comment, with who wrote it and on what day, is
shown beside it while the pointer is over the cell, and beside the picked cell
otherwise, and not while a cell is being typed in. Sheet > Comment… writes,
changes or deletes the picked cell's comment, as one step to undo. A comment
written here carries its text, its date in the computer's local time, which is how `LibreOffice` reads it, and as its author the full name
the system keeps for the user, or their login; it says nothing of the box it is
drawn in, which `LibreOffice` places itself, and a comment changed here keeps the
box and the display setting it had. A value typed into a commented cell keeps
the comment, and a sort moves each comment with its row.

**Cells are formatted from Sheet > Format cells…** (`sheet/format.rs`), or
Ctrl+1: a number format (general, a number with its decimals and thousands
grouped or not, a percentage, an amount in the currency of the document's
country with its symbol where that country puts it, a date in the order that country writes one, a time), bold, italic
and underline, a font colour, a fill, the text's alignment across and down the
cell and whether it wraps, and borders, none, around the selection or around
every cell in it. The question opens with what the picked cell shows and
changes only what is changed in it, so cells that differ keep what nobody
touched. Ctrl+B, Ctrl+I and Ctrl+U make the selection bold, italic or
underlined, or take it off where the picked cell has it. A cell's formatting is
its cell style: a formatted cell takes an automatic style in `content.xml`
holding what its old style held with the change written over it, a copy of its
own automatic style or one inheriting from its named style, and a style that
says the same thing is used again rather than written twice. A number format is
a data style written in the shape `LibreOffice` writes it, in the document's
language. A value takes the kind its new format shows, an amount given a
percentage format becoming a percentage and a number given a date format that
day, and its text is written again in the format; a formula keeps its formula.
A selection larger than a hundred thousand cells is cut to the cells the sheet
uses. Each is one step to undo.

**A change works out again every formula that reads it** (`odox-core`'s
`formula` module and `sheet/recalc.rs`). A document's own results are trusted
when it opens; when a value changes, every formula that reads the cell, on any
sheet and through any chain of other formulas, is worked out in the order they
depend on each other, in the same undo step as the change, and a row or column
put in or taken out works out every formula. The core is `OpenFormula` as ODF
stores it: numbers, text, booleans, errors, references and ranges, the
operators, and SUM, AVERAGE, MIN, MAX, COUNT, COUNTA, COUNTBLANK, PRODUCT, IF,
IFERROR, AND, OR, NOT, TRUE, FALSE, ROUND, ROUNDUP, ROUNDDOWN, ABS, INT, MOD,
POWER, SQRT, CONCATENATE, CONCAT, LEFT, RIGHT, MID, LEN, UPPER, LOWER, TRIM,
DATE, YEAR, MONTH, DAY, VLOOKUP, SUMIF, COUNTIF and AVERAGEIF. What they answer
is `LibreOffice`'s, measured on `corpus/libreoffice/formulas.ods`: a boolean is
one or zero, `=` compares text with its case while a criterion ignores it, a
date is days from 30 December 1899, a range where one value is wanted is its
cell in the formula's row or column, `SUMIF`'s sum range takes the shape of
its criteria range, a number joined to text is written as `LibreOffice` writes
it, `1E+020`, and `UPPER` makes ß the capital ẞ while `LOWER` leaves a dotted capital İ as it is. `DATE` refuses a day before
15 October 1582, where the Gregorian calendar begins, and a year below zero; a
day before it that arithmetic reaches is written in the proleptic Gregorian
calendar, with the years before the first counted without a year zero,
`-0061-04-04`, and shown in the Julian calendar, as `LibreOffice` does both.
Text that arithmetic or a function's argument wants as a number is read in the
conventions of United States English whatever the document's language, as
`LibreOffice` reads it (`formula/convert.rs`): `1,000`, `(5)`, `$5`, `5 %`,
`TRUE`, `1 1/2`, `5/1/2024`, `2024-05-01`, `May 1, 2024` and `9:30 pm` are
numbers, and a date written without its year falls in the year the window
read the clock when the file opened. `=` and `&` keep text as text. A
criterion of `SUMIF`, `COUNTIF`, `AVERAGEIF` and `VLOOKUP` is matched as the
document's calculation settings say, measured on `criteria.ods` and
`parts.ods`: with wildcards where they are allowed, and otherwise as a regular
expression, ODF's default, against the whole cell unless the settings say a
part will do. A number, a date or a boolean is matched by the text a number
joined to text would be, `45413` for a date and `1` for true. A reference past
column XFD or row 1048576 is a name `LibreOffice` does not know, `#NAME?`. A result
keeps the kind of value its cell held and is shown in the cell's number format; an error is written as
`LibreOffice` writes one. A formula outside the core keeps its result and is
drawn faint once something it reads changes, or once anything does where it
cannot even be read, as is every formula that reads one; a formula that reads
itself is `Err:522`.

**A selection is a range, and the clipboard carries it as text and as a
table.** A press picks the cell under it, Shift or a drag stretches the
selection to another, and the range is the rectangle between the two corners.
Copy and cut hand over the cells as tab separated rows, each as it is
displayed, which is what every other program can read, and as an HTML table of
the same cells, which a word processor pastes as a table; paste reads the same shape back, each cell as a value typed
into the cell it lands in would be read, from the top left of the selection. A paste, a cut or a delete is
one undo step and is all or nothing: if any cell it reaches lies under a merge,
none is written and the cell bar says which. A value written over a formula
takes its place, as it does when typed. Emptying a
range leaves cells the document never wrote unwritten.

**A cell's text is cut at the cell's edge.** It is drawn on one line, at the
top, middle or bottom of the cell as its style says and at the bottom where it
says nothing, as `LibreOffice` puts it, or, where the cell's style says to wrap (`fo:wrap-option="wrap"`), wrapped
inside the cell's width with each line aligned as the cell is. Either way what
does not fit the cell's height or width is not drawn: a row keeps the height the
document gives it, and does not grow while a person types. A merged cell is one
cell, and the grid's lines stop at its edge.

**A merged cell is picked, outlined and stepped over as one.** A press anywhere
on it picks its top left corner, the outline goes round all of it, and an arrow
leaves it from its far side. The Sheet menu merges the selection into its top
left cell, which takes `table:number-columns-spanned` and
`table:number-rows-spanned`, and writes each other cell as
`table:covered-table-cell` with what it held kept, as ODF allows and
`LibreOffice` does, so that unmerging gives each back. A merge wholly inside the
selection becomes part of the new one; one across its edge refuses the merge.
Unmerging takes apart the merge the picked cell is in. A copy leaves out what a
covered cell holds, since it is not shown. Each is one step to undo.

**A workbook's sheets are added, renamed, deleted and moved from the Sheet menu
and from a tab's own menu**, and a double-click on a tab renames it; each is one
step to undo. What refers to a sheet refers to it by name, so a move only
reorders the `table:table` elements. A new sheet is empty, goes after the one it
was asked from in that sheet's table style, and is named "Sheet" and the first
free number from the count of sheets on. A rename renames the sheet in every
reference that names it: the formulas and named expressions, whose references
are in brackets, and every attribute that holds addresses alone (a print range,
a named range, a database range, a conditional format's or a validation's
cells), quoting a name that is not letters, digits and underscores; the sheet's
view settings, its freeze among them, follow it. A rename is refused where a
reference cannot be read, or where a chart, an object, a validation's condition
or a conditional format's formula holds the name in a form this does not
rewrite. A delete is refused while anything outside the sheet refers into it,
for the same reason a row's is, and for the last sheet shown. A name may not be
empty, hold `[ ] * ? : / \`, start or end with a quote, or be another sheet's.

**A column's width and a row's height are dragged at the header's edge.** A
double-click on a column's edge fits it to the widest text in it. The size is
written into a style of the column's or the row's own, the style it had with
the size changed and named as an automatic style the document already holds
wherever one says the same, after the column or row is split out of a repeated
run; a row given a height is marked as no longer fitted to its text. Each
resize is one step to undo, and changes no value, so no formula is drawn faint
for it.

**Hidden rows and columns are drawn at no size.** A row or column whose
element says `table:visibility` other than "visible" takes no room in the
grid and has no header; a thicker stroke in the headers marks where it is. A
pointer finds the one after it, and the arrows and the other keys step over
it the way they were going, or back where nothing is shown past it. The Sheet
menu hides the selected rows or columns, writing "collapse" on their elements
once the ends are split out of their runs, and shows hidden ones by removing
the attribute: those in the selection and those next to it, so that a person
picks a cell beside what is hidden, since it cannot be picked itself. Each is
one step to undo. A selection is what it shows: a copy, the summary in the cell
bar and emptying the range all leave hidden cells out, and only a paste writes
through them, the way a block put down covers whatever is under it.

**Frozen rows and columns stay in view.** A freeze is a view setting, which ODF
has no element for and every producer writes as `LibreOffice` does, into
`settings.xml` under the first view, per sheet by name: split mode 2 and a
position, for the rows and for the columns. The grid is then drawn as four
panes, each inside its own edges: the part that scrolls both ways, the frozen
rows above it, the frozen columns beside it, and the corner where they meet,
with a line where the frozen part ends; a press is read against the pane it
lands in. View > Freeze rows and columns freezes those above and left of the
picked cell, and the same item unfreezes, each one step to undo; a package
with no `settings.xml` is given one and the manifest lists it. Ctrl+Home scrolls
the grid back to its start as well as picking the first cell, which a frozen
cell being picked does not do by itself.

**The keyboard walks a sheet.** The arrows move one cell, into the empty grid
past the data too; Ctrl and an arrow go to the end of the block of filled cells
the pick is in, or from an empty cell or a block's end to the next filled cell,
and to the grid's edge when there is none. Page Up and Page Down move by the
rows the grid last showed, Home and End to the first and the last written
column, Ctrl+Home and Ctrl+End to the first and the last used cell, and Ctrl+A
selects the used cells. Shift with any of them stretches the selection. Each
key is read with the modifiers it came with, because a press and the release of
Ctrl can arrive in one frame.
Ctrl+Page Down and Ctrl+Tab look at the next sheet, Ctrl+Page Up and
Ctrl+Shift+Tab the one before, round from the last to the first and passing
over hidden sheets. It is Ctrl on a Mac too, where Command+Tab is the system's.

**A selection says what it holds.** With more than one cell selected, the cell
bar gives the sum and the average of the numbers in it, percentages and
currencies counted as their numbers, and the count of cells that hold anything.
A formula counts as the result it shows. The figures are to six decimal places
at most, with the reader's decimal separator.

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

**A table is put in and reshaped from the Format menu's Table submenu, and the
toolbar's Table button.** The button and Insert a table ask how many rows and
columns, two by three to begin with, and the table goes after the caret's
paragraph, or before it where the paragraph is empty, with an empty paragraph
after it where nothing follows, and the caret in its first cell. It is written
as `LibreOffice` writes one (`odox-core/src/edit/table.rs`): an automatic table
style as wide as the text, a column style per width, thin-bordered cells with
the document's table-contents paragraph style where it has one. Rows and columns
go in above, below, left and right of the caret's cell and come out with it;
the last row or column out takes the table, and Delete table takes it at once.
The window draws columns at the widths their styles give, so a column put in
shares the width of the one it goes beside and a column taken out gives its
width to its neighbour, keeping the table as wide as it was. A table with merged
or repeated cells keeps its rows and columns, and the window says why. Tab in a
table selects what the next cell holds and Shift+Tab the one before, row by
row, and Tab in the last cell puts in a row and goes to its first. Each change
is one step to undo.

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
spreadsheet cell is formatted whole, as its own paragraph says below.

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
edit was undone, and the package entries the edit wrote outside the tree, as
a freeze or a sheet's rename writes `settings.xml` and the manifest, as they
were before it. The document is modified when the stack is not at the depth
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
another. Slide labels go through the same flow and so are told the same way.

**A sheet's grid is told as a grid of the cells in view.** The grid is painted
and has no widgets, so beside the painting each row in view and shown is a row
node and each cell in it a cell node, carrying its text, its row and column with
the names the headers give them, its bounds and whether it is selected, under a
grid node named for the sheet with the grid's size and the picked cell as its
active descendant; a cell under a merge is left out and the merge's own covers
it. A node is made under another by being a `Ui` inside the other's, with its
role set on the node egui gives every `Ui`. The grid does not take the
keyboard's focus, which would take the keys from it, so the picked cell is also
a polite live node saying its address and its text, which a screen reader reads
out as the pick moves. None of it is built unless a screen reader is
listening, which is when egui makes nodes at all. The cell editor is egui's
text box, named for its cell.

**A copy keeps its formatting, in the window and out of it.** The editor puts
the selection on the clipboard as text and as HTML, and keeps beside them a
`Fragment` its model made: the covered paragraphs cut to the range, each with its
own style and kind, and the spans and links in it, without the ids, bookmarks,
frames and notes that name one place in a document and would then be there
twice. A paste of exactly the text it put on the clipboard is a paste of the
fragment. Any other paste, from another window or another program, looks for
HTML beside the text and reads the part of it that every program writes
(`odox_core::edit::html`): paragraphs, headings of six levels, the items of
bulleted and numbered lists, line breaks, and bold, italic, underline and
strikethrough, by their tags or by a `style` attribute that says them, as a
browser writes them; a table gives its rows as paragraphs with a tab between
cells, and anything else gives its text. What is written is the same part, with
links. A read paragraph's marks become the automatic styles a mark put on by
hand would have, and its kind is made as the Format menu makes it, all in the
paste's one undo step. The first pasted paragraph merges into the paragraph at
the caret and takes its style, as typed text would; the ones between keep their
own; the last merges with what followed the caret and keeps the paragraph's
style; in a list each is an item. Outside a list, a paragraph pasted where the
caret has no text before it, or the last where it has none after, keeps its own
kind, so that a heading pasted on an empty line is a heading. The clipboard is
the system's through `arboard`, one kept for as long as the application runs;
on Linux that is Wayland's data-control protocol where the compositor offers it
and X11's otherwise, which under Wayland is Xwayland's and the desktop hands
on, and where none can be had a copy is egui's text alone. A model that keeps
no formatting leaves the four `Model` methods as they are and every paste is
plain.

**A deck's slides can be added, duplicated, deleted and moved.** A slide
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
uses. A new slide goes after the one in view, on its master, named `page` and
the first free number from the count of slides on, with an empty title frame
and an empty text frame where that master puts its title and outline
placeholders, or across the page where it has none. A placeholder's own layer
and any style only `styles.xml` can see are left behind; its paragraph style
is kept where it is a common one. Where the slide it follows has a frame of the
class, the new frame takes that frame's style and its first paragraph's, so a
new title looks like the one before it. The Slide menu offers the five in edit
mode, and each is one undo step with the view following the slide.

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

**Each of the three applications exports its document as PDF/UA-1**, the
accessible profile of PDF, from its File menu. The label says PDF/UA because the
output is checked: krilla, the writer, runs its PDF/UA-1 validation as it writes
and fails the export rather than producing a file that only claims to conform,
and `crates/odox-pdf/tests/conformance.rs` puts every `.odt`, `.ods` and `.odp`
in the corpus through veraPDF's ua1 profile. veraPDF is a Java program and not a
build dependency, so the test says it skipped the check where it is not
installed; `packaging/verapdf.sh` runs the same check over any documents by
hand. The export is one flow in `odox-ui` for all three windows, over one trait
in `odox-pdf` that each kind of document implements with a layout of its own.

**An export is made off the window's thread.** Once a file is chosen, the
document's parts are copied (`Document::snapshot`) and the PDF is laid out and
written on a thread of its own, from the copy, while a corner of the window says
which file is being exported; the menu item waits until it is done. The window
stays live and editable meanwhile, and what is typed then is not in that PDF.
The styles index caches what it resolves and cannot cross threads, so the copy
carries the parts and the thread indexes them again, as a read does.

**The PDF holds what the window shows, in the same order.** `odox-pdf` reads
text into blocks with the same decisions the window's flow makes, which live in
`odox-core` for both to call: which elements are blocks and which pass their
text through, how a list level writes its numbers, a tab to its stop, a
picture anchored in a paragraph after it. An object with no picture of its own,
which the window shows as an empty box, is left out. So is text the document
hides, which the window shows to the person editing it: a run or a paragraph
styled `text:display="none"`, a section displayed `none`, and a `hidden-text` or
`hidden-paragraph` field whose condition hides it now. A note in a text document
is the one thing the PDF holds that the window does not show: its citation stays
in the text, and its own text goes to the end of the document with the other
notes, in the order they are cited, below a short rule, each tagged as a Note
with its citation as its label. A sheet's name above its cells is the other.
Every page is white with black ink where the document names no colour, whatever
the window's theme.

**The windows have no pages; an export does.** Text is shaped by rustybuzz in
the faces `odox-fonts` resolves, a character the face lacks in the first face on
the machine that has it, and left to right as the window draws it. The window
and the PDF measure text with different shapers, so a line can wrap a word
differently in each; there are no page breaks in a window for the PDF's to
disagree with.

A text document's body is laid out at the text width of the first page style
and filled into pages of its size and margins. Lines break where the Unicode
line breaking algorithm allows, a word wider than the line where it has to. A
proportional line height is a share of the face's own line height, which is
what ODF means by it. A page is filled with whole pieces: a line, a table row, a
picture. A heading stays on the page with the line after it, a page break the
document asks for is taken, a table's header rows are drawn again above the rest
of it on a new page, and only a piece taller than a page is cut. There is no
widow control, and there are no floats or columns.

**A page prints its header and footer** (`odox-pdf`'s `bands.rs`, from
`odox-core`'s `doc/band.rs`): the ones the master page gives it, the first
page's own and an even page's left one where the master gives them, the
master's own otherwise, and none where the master turns them off. Their fields
are filled in for the page: its number and the count, the title, the sheet's
name, and today for a date that is not fixed. The body is shortened by the
room they take, their height and the spacing their page layout asks between
them and the body, and they are drawn inside the margins above and below it.
A text document's keep their paragraphs' own alignment; a spreadsheet's left,
centre and right regions each run across the page, and one with no regions is
centred, as Calc prints it. Page numbers run on across a spreadsheet's sheets.
`corpus/libreoffice/pages.odt` is the measure. The windows have no pages and
show neither.

A spreadsheet prints each sheet that is shown on the page its table style's
master page gives, beginning a new page with the sheet's name. What it prints is
its print range, as far as the cells in use, since a print area written as whole
columns reaches the last row a sheet can have, or every cell up to the last that
holds anything; a print range of several areas prints the rectangle around them.
Rows and columns the sheet hides are left out, and so are a hidden sheet, one
with nothing in it, and a page that none of its cells draws on. Columns that do not fit across the page continue on pages of
their own, down the sheet first and then across unless the page style's print
order says across first, and the rows the sheet marks as its header
(`table:table-header-rows`) are drawn again at the top of every page below the
first. The page style's scale, or the number of pages it asks the sheet to fit,
across, down or in all, decides how large the cells are drawn, down to a tenth.
A cell is drawn as the window draws it: its fill and borders, and its text
at the top, middle or bottom, on one line or, where the cell says to wrap
(`fo:wrap-option="wrap"`), wrapped inside its width, and cut at the cell's
edge; there are no grid lines. The
window draws none of the pictures or charts a sheet can hold, and neither does
the export.

A presentation is a page per slide at the slide's size, and a slide the show
skips (`presentation:visibility`) is left out. A slide is drawn as the window
draws it: its ground, the master page's decorations, then its own shapes back to
front, each filled, outlined and labelled, and a shape the window leaves undrawn
is left out too, as is a turned shape's label. The speaker's notes are not
exported.

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
underlines, a repeated table header, a page's header and footer and a
decorative picture are artifacts.

Each sheet is a section: its name as an H1, then one table however many pages
it takes, read row by row across all of them. The marked header rows are its TH
rows, and a sheet that marks none has its first row read as the header, since a
table without one tells a listener nothing about its columns. Each slide is a
section too: its title placeholder first, as an H1, and then its own shapes in
the order the document lists them, which is the order they are stacked in. A
shape with no text that says what it shows in `svg:title` is a Figure; any other
outline, and everything from the master page, is decoration.

The title is `dc:title`, or the file's name; the language is the document's
default style's for its kind (paragraph, cell or drawing), then `dc:language`,
then the language of the person exporting. The outline is the headings, the
sheets' names, or the slides' titles, an untitled slide named by its number.

**A picture has to say what it shows, or that it is decoration.** Its
alternative text is the frame's `svg:title`, or failing that its `svg:desc`; a
decorative one is marked as `LibreOffice` marks it, `loext:decorative` in the
frame's graphic style, or in the presentation style of a slide's placeholder,
which names that family rather than the other, so a document says the same in
either application. An export with a picture that says neither is refused, and
the window asks about each one first, with the picture beside a field for its
text and a box for decoration; the answers are written into the document as one
step to undo, and a document that does not declare the SVG or `LibreOffice`
namespace is given the declaration. Only a picture that would be drawn is asked
about: not one on a hidden slide or on the master page, whose pictures are
decoration. Nothing unanswered is written under the PDF/UA label.

**A font is embedded only where its licence allows it.** A face whose OS/2
flags restrict embedding or forbid subsetting is replaced by its metric
substitute or the generic family, and the export says so when it finishes. A
family with no embeddable face at all, and a character no face on the machine
has, refuse the export by name, since a PDF/UA file may hold no glyph that stands
for nothing. A document with nothing in it to draw, every slide hidden or every
sheet empty, is refused as well.

**The export runs in the window's own thread**, so a large sheet holds the
window while it is written.
