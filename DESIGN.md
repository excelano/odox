# odox — design

The authority on how these applications are built and why. `README.md` is the
front door; this is the reasoning behind what it describes. Sections are
numbered because the code cites them.

---

## §1 Three applications, one core

The three formats are not three formats. `text:p`, `text:span`, `text:list` and
`table:table` appear in a text document, in a spreadsheet cell and in a slide's
text frame alike; the style system is one system, with the same families, the
same inheritance through `style:parent-style-name` and the same split between
named styles in `styles.xml` and generated ones in `content.xml`; the package
layout, the metadata and `draw:frame` are shared outright. What differs between
the formats is the one element inside `office:body`, and after that only how the
body is addressed: a flow, a set of sheets indexed by row and column, a sequence
of pages.

So the core is large and each application is thin. `odox-core` reads and writes
the format and knows nothing about a window. `odox-ui` is every pixel a person
sees, including the renderer that draws ODF's shared content model, which is why
a spreadsheet cell holding a formatted paragraph and a slide's text frame draw
through the same code as a document's body. Each of `xodt`, `xods` and `xodp` is
one view and a `main`.

They are three binaries rather than one with three modes because a person opens
a spreadsheet and a presentation through different doors: the desktop registers a
handler per media type, and a file manager offers an application, not a mode.
They share no bytes on disk and every line of source.

There is a fourth binary and it is not an application. `odox` is a launcher: it
takes a file, works out which of the three reads it, and on Unix replaces itself
with that one. The extension answers first, because it is free and it is the same
answer the desktop's association gives; the package's declared media type answers
where the extension cannot, which is the file named `download` or named nothing.
It links `odox-core` and deliberately not `odox-ui` — reaching the shared window
crate would put a graphics toolkit inside a command that runs for a few
milliseconds. **The applications are the product and the launcher is a
convenience over them**: a desktop offers the three directly, one per media type,
and nothing in them depends on it. Its Debian package depends on all three the
other way round, which makes `apt install odox` the way to install the suite —
unversioned, because the launcher finds a viewer by name and hands the file over,
so there is no coupling to a version.

The dependency between them runs one way and never back. `odox-core` does not
depend on `odox-ui`, does not link egui, and does not open files: a document is
made from a byte slice and turned back into a `Vec<u8>`. That is what lets the
same code serve a window, a sandboxed macOS application that may only replace the
file it was handed, a browser, and a test that never touches a disk.

## §2 What the build depends on

Pure Rust, as a rule this repository keeps for itself. The fleet stance is a
preference rather than a rule and is argued once in `~/notes/pure_rust_preference.md`;
nothing in a viewer for a zipped XML format needs C, so nothing here takes it.

The property is worth more than tidiness. A build that needs a Rust toolchain and
nothing else cross-compiles from a Linux box with no MSVC toolchain, builds on a
runner with nothing installed, and packages the same way on three platforms. And
a dependency on the toolchain is invisible from inside the toolchain, which is
how slipcase-desktop 0.1.1 passed every local check and failed Microsoft Store
certification for linking `VCRUNTIME140.dll`.

The check is the artefact and never the manifest. `cargo tree -i cc` is not empty
in any eframe tree and never will be, because `wayland-backend` declares `cc` and
uses it only under a feature nothing turns on. What is measured instead:

    objdump -p target/release/xodt | grep NEEDED

which answers `libgcc_s`, `libm` and `libc` and nothing else. Every other library
these applications use — OpenGL, X11, Wayland, xkbcommon — arrives through a
`dlopen` at run time, which is why §9's Debian dependencies are written from a
running process rather than from the linker.

## §3 The document is kept, not summarized

A document is held as the XML tree it was parsed from. An element nobody here has
heard of keeps its attributes, its children and its position, and is written back
where it was found.

A viewer does not need that, and it is the most consequential decision in the
repository. A reader that discards the part of ODF it has no opinion about looks
identical in a window and becomes an editor that destroys documents the first
time anything saves one — and by then the model is load-bearing in every file. So
the tree is faithful from the first release, and `crates/odox-core/tests/roundtrip.rs`
measures it before anything can save at all: every entry of the package comes back
byte for byte, and parsing what the writer produced gives back an equal tree.

Attribute order, the choice between `<a/>` and `<a></a>`, comments, processing
instructions and the whitespace between elements all survive. Text is held
unescaped and escaped again on the way out in the canonical form, so byte
identity of an XML part is not claimed; tree equality is, and that is what the
test asserts.

Typed reading is a view over the tree rather than a replacement for it. `Styles`
resolves a name through its inheritance chain, the three document types index a
body for the application that draws it, and both hand back the underlying
`Element` so that a property this crate does not model is still reachable.

## §4 Styles

Named and automatic styles are collected into one table keyed by family and name,
because ODF scopes a style name within its family and nothing downstream needs to
know which kind of style it is holding. Resolution walks the inheritance chain
from its root down, beginning at the family's `style:default-style`, so the
nearest style wins. The answer is cached: a spreadsheet asks for the same handful
of cell styles once per visible cell per frame.

Resolved properties are one type across every family rather than one type per
family, because a cell carries paragraph properties and a paragraph carries
character properties, and a renderer asking a cell for its font would otherwise
resolve three styles and merge them itself.

`fo:margin` and its per-edge siblings, and `fo:border` and its, are read as the
shorthand ODF defines: the whole sets all four edges and a per-edge attribute
overrides one. An edge nobody set is absent rather than zero, which is the
distinction between inheriting a border and having none.

## §5 The three bodies

A **text document** is a flow of blocks in the order they are read, which is the
order they are drawn. The page layout of the first master page says how wide a
line may be, and that width is what the renderer is given.

A **spreadsheet** is indexed rather than flattened. ODF writes a run of identical
rows or cells once with a repeat count, and a sheet whose last column says
`table:number-columns-repeated="16384"` is ordinary — every office application
writes one, and expanding it would turn a small file into a large allocation. So a
sheet keeps the rows it was given, each with the range of row numbers it stands
for, and a lookup is a binary search through those ranges. Rows nested inside
`table:table-header-rows` or a `table:table-row-group` belong to the sheet as if
they were the table's own, and are reached again by the path recorded when they
were indexed.

The used extent of a sheet is where content stops, not where the repeat runs stop,
so a grid draws the rows a document has rather than the million a sheet may
declare.

A **presentation** is a sequence of pages whose shapes carry their own position
and size in the page's coordinate space. That is what makes a slide drawable
without laying anything out: the page is scaled to the space the window has and
each shape is put where the document says.

A slide is drawn in three passes, back to front: the ground, then what its
master page contributes, then the slide's own shapes. The master is where a
template keeps its identity — a gradient band, six polygons — and a slide names
one rather than copying it.

**A child of a master page carrying a `presentation:class` is a slot and not a
decoration.** The slide's own frame of that class takes its place, and what the
master holds is a prompt or a field: drawn as it stands, every slide gains the
words *Click to edit Master title style* and a literal `<number>`. The class is
the test and `presentation:placeholder` is not, which would be the obvious one
and is not written reliably — the title frame on the corpus deck's master carries
the prompt and no such attribute. Measured, not read.

## §6 Drawing

**Fonts are the machine's.** Nothing is embedded. A document names a family and
the machine is asked for it, because every platform this ships on carries a
metrically compatible face for the families office documents use, and three
applications carrying a megabyte of fonts each would spend three megabytes on a
question the operating system has answered. Faces are resolved once when a
document opens, because egui rebuilds its glyph atlas when the font definitions
change.

Two things make that work and both were found by looking rather than by a test.
`fontdb` names Times New Roman, Arial and Courier New as its own generic
defaults, which is right on Windows and resolves to nothing at all on a Linux box,
so the generics are pointed at faces the machine has before anything is asked for.
And a document naming Times New Roman on a machine with Liberation Serif keeps its
line breaks only if it is given Liberation Serif rather than whatever the generic
serif happens to be, so the handful of families with a metrically compatible
substitute are named. Both are in `crates/odox-ui/src/fonts.rs`.

**A page is paper.** In a dark window as much as a light one. The colours in a
document are the document's: a heading its author made near-black is drawn
near-black, and putting it on a dark ground because the desktop asked for a dark
desktop makes it invisible — which is what happened to the title of the first
Impress deck opened here. So the page keeps its own ground and the window's
chrome follows the desktop, which is what every office application and every PDF
viewer does with a page.

**Text in the page can be selected and copied**, by dragging, double-clicking
a word or triple-clicking a line, and Ctrl+C. The paragraph hands its laid-out
galley and the anchor it was laid out around to egui's own label-selection
plugin, which paints the galley at that anchor — so alignment is untouched — and
keeps the selection across paragraphs and across scrolling, because every
paragraph reports to it whether or not it is on screen. The one thing that had
to be found by measuring: the plugin begins a selection only on a response that
senses drag, which `Label` adds to its own and a bare allocation does not.

**What this release does not draw.** Nothing paginates: a page layout gives a
width, and page boxes, widows, floats and multiple columns are typesetting rather
than reading. Tab stops advance by a fixed amount rather than to the paragraph's
stops, so a document whose layout depends on tabs is laid out approximately.
Right-to-left text is drawn left to right: `fo:text-align` distinguishes `start`
from `left`, and honouring the difference needs a writing direction this release
does not read.

**Of ODF's shapes, what is drawn is what a fixture proves.** Rectangles,
ellipses, polygons, polylines, lines and custom shapes are drawn from their own
geometry, with a solid fill, a linear or axial gradient, or a stretched picture,
and an outline. A `draw:path` states its outline as SVG path data rather than in
ODF's own commands and flattens through the same pen, save for an elliptical arc,
which is drawn as the straight line to where it ends. Connectors and measures are
left undrawn rather than approximated into something the document does not say. A radial,
ellipsoidal, square or rectangular gradient is filled with the flat average of
its two colours, and a tiled picture with nothing: both are visibly
approximations rather than wrong directions, and no fixture uses either.

**A custom shape states a path and not points**, in `draw:enhanced-geometry`, in
a coordinate space of its own, with numbers that may be references to named
formulas over the space's edges and over adjustments a person dragged. `odox-core`'s
`draw` module evaluates the formulas and flattens the path — curves and arcs
included — into polylines, because how finely a curve must be broken depends on
the size of that space and not on the size of the window. The whole command
language is implemented; what is *measured* is the part that occurs across the
twenty-three presentation templates LibreOffice ships, which is `M`, `L`, `C`,
`Z`, `N`, `U`, `X`, `Y` and `V`, and formulas over the edge constants, `pi`,
`if`, `sin`, `cos` and `abs`.

**A fill is cut into triangles before it is painted**, by clipping ears. A
graphics toolkit fills a closed path by triangulating it, and the cheap way — a
fan from the first point, which is what `Shape::convex_polygon` does — is right
only for a convex outline. An arrow, a callout and a puzzle piece are none of
them convex, and a fan across one paints outside it.

**The kind of fill and the value it uses are separate properties and inherit
separately.** A style may set `draw:fill-color` and say nothing about
`draw:fill`: that names the colour a solid fill *would* use and does not turn the
fill on, so a shape whose parent style says `draw:fill="none"` stays empty.
Reading the two as one put a white box over a template's photograph, which is
what split them.

## §7 The window

One shell, in `crates/odox-ui/src/shell.rs`, with a `Viewer` for the part that
differs. The shell owns the menu, the keys, the file dialog, the error line, the
zoom and the side panel; it also owns every read from disk, so a view is handed
bytes and never a path. A view answers whether it has anything to put beside the
document, and a format that has not gets no empty panel.

The desktop's light and dark setting is followed through the XDG portal on Linux,
because `winit` answers `system_theme()` on the other two platforms and returns
`None` here. `src/system_theme.rs` is slipcase-desktop's module taken unchanged
but for the thread's name; the measurements, the reasoning and the tests are in
that repository, which is the same arrangement duckling's copy records.

## §8 Language

Every string a person reads lives in `odox-ui` and goes through `potext`'s `t`.
There is one catalogue for the suite rather than one per application, because
`potext::catalog!` gives the storage to the crate that invokes it and every
message is invoked from `odox-ui`: a string written in `xodt` is looked up in
`odox-ui`'s catalogue. So the extraction globs every crate and the catalogues
live under `crates/odox-ui/po`, which is also what `include_str!` requires —
reaching above a crate root compiles locally and fails in `cargo package`, and
flyleaf lost a release tag to exactly that.

An application contributes its own name, which is not translated, and the name of
the format it opens, which is. The desktop entries carry the same languages
in `Comment[..]` and `GenericName[..]`, because a file manager reads those and
never the catalogue. That one is a literal in a `const` built before
`run` puts a catalogue in force, so it is wrapped in `i18n::mark` — gettext's
`N_` — and looked up through `t` where it is drawn.

The launcher is the exception and says so here rather than in a comment nobody
reads: it draws no window, links no `odox-ui`, and its three sentences are
English. Reaching the catalogue would mean linking the toolkit that holds it,
which is twelve megabytes to translate a usage message.

German was written against Comma's `de.po`, which is the fleet's glossary: one
set of words for *Datei*, *Öffnen …*, *Neu einlesen* and
*OpenDocument-Tabellendokument* across every window that says them, and
LibreOffice's own German for what Comma never needed — *Folien*, *Gliederung*,
*Referentennotizen*.

**The pseudolocale is a debugging tool and not a translation.** `en-x-pseudo`
returns every message accented, bracketed and 40% longer, and running a window in
it shows three things at a glance that no test reaches: a string that never went
through `t`, a sentence the catalogue never saw, and a label built to the width of
English. The third is what German hits, German running about a third longer, and
this finds it without anybody reading German. It is compiled into debug builds
alone. Its first run here found the whole mechanism inert — a catalogue was
shipped and never chosen, which an English window is indistinguishable from — and
that is now a unit test rather than something a person has to remember to look
for.

`po/update-po.sh` re-extracts and merges; `preflight.sh` refuses a release whose
template is behind the source. `msgmerge` marks a reworded message `#, fuzzy` and
`potext` refuses to load one, so the window falls back to English until somebody
has read the new sentence: a translation is never silently wrong, it is current or
visibly absent. That is the property a key-value catalogue cannot offer and the
reason this speaks `.po`.

## §9 Size, speed and packaging

The release profile trades compile time for size: fat LTO across one codegen unit
so that egui's unused arms are dropped between crates, and no symbol table.
Unwinding stays, because a panic in a document reader should reach a dialog rather
than kill the window with no message. A stripped viewer is about 12 MB, and the
overwhelming majority of that is egui and its dependencies rather than anything
here — three viewers of a zipped XML format are a small program inside a graphics
toolkit. The launcher, which reads the same packages through the same library and
links no toolkit at all, is 564 KB, which is the size of the argument.

Each application carries a `build.rs` that embeds the Windows application manifest
and does nothing else ever. The manifest declares DPI awareness before any of the
program's code runs, which is what the Windows App Certification Kit reads;
slipcase-desktop failed `DPIAwarenessValidation` until the file existed. It is two
linker arguments and no resource compiler. The three copies are byte identical and
read the binary's name from the environment, so there is nothing in them to
diverge over, and `packaging/preflight.sh` refuses a release where they have
drifted. The manifest is above each crate's own directory, which `cargo package`
would not carry, so a build that cannot find it skips it with a warning rather
than failing — a Store build is made from this repository, where it is there.

`+crt-static` is in `.cargo/config.toml` for the MSVC targets, and
`packaging/windows/check-imports.ps1` refuses any import that does not ship with
Windows, because that is the pair that answers the 0.1.1 certification failure.

The Debian dependencies are written by hand from `packaging/linux/check-libraries.sh`
run once on each display backend, because the binary links three libraries and
dlopens the rest. Both backends are compiled in, so both sets are declared.
No package here declares a media type: every desktop already knows what an
OpenDocument file is, from shared-mime-info, and a second declaration would be a
second thing to keep right. What the desktop entries register is that these
applications can open one.

## §10 What "compliant" is measured as

Not that a document renders the way another application renders it — nothing
automated measures that, and the applications are written to the specification
rather than to another program's behaviour. What is measured is that a document
comes back unchanged, over a corpus, on every part of every package.

`corpus/libreoffice/` holds documents a real producer wrote, rebuilt from
committed plain-text sources by `build.sh`. `corpus/make-fixtures.py` writes a
spreadsheet and a presentation by hand; they exist to exercise a repeated row, a
merged cell, a formula with its cached value and a slide with a text frame, and
they are worth less than the real ones. The tests walk the corpus directory rather
than naming its documents, so adding one is a change to the directory.

Two things a corpus of real output settled. A formula cell carries
`table:formula`, `office:value` and a `text:p` holding the text of the value as it
was last displayed, so a viewer needs neither a number-format engine nor a formula
evaluator: it shows the string the producer formatted. Where a cell has no number
format, that string is the full-precision one Calc stored, which is what the
document says and not what Calc's column width shows. And LibreOffice 25.2 writes
`office:version="1.4"`. Nothing here gates on the version — the parts a viewer
reads have been stable since 1.0 — and the reader accepts what it is given.
