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

**Fonts are the machine's.** Nothing is embedded; a document names a family and
the machine is asked for it once, when the document opens, because egui rebuilds
its glyph atlas when the font definitions change. `fontdb`'s generic defaults
resolve to nothing on Linux, so the generics are pointed at faces the machine
has first, and the families with a metrically compatible substitute are named so
that a document asking for Times New Roman keeps its line breaks under Liberation
Serif. Both are in `crates/odox-ui/src/fonts.rs`.

**A page is paper**, in a dark window as much as a light one: the page keeps its
own ground and the window's chrome follows the desktop.

**Text in the page can be selected and copied**, by dragging, double-click,
triple-click and Ctrl+C. The paragraph hands its laid-out galley and its anchor
to egui's label-selection plugin, which paints the galley at that anchor and keeps
the selection across paragraphs and scrolling because every paragraph reports to
it whether or not it is on screen. The plugin begins a selection only on a
response that senses drag, which a bare allocation does not.

**What is not drawn.** Nothing paginates: a page layout gives a width, and page
boxes, widows, floats and multiple columns are typesetting rather than reading.
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

**Nothing is written until Save, and then only the file that was opened or the
one Save As named.** The shell owns the write as it owns the read. Before the
bytes touch the disk they are read back and compared with the tree they were
written from, and a difference refuses the save and says so: the round-trip
tests make the same claim over the corpus, and a person's document is not in
the corpus. On Linux and Windows the bytes go into a `.part` file beside the
target and are renamed over it, carrying the target's permissions, so the file
is either what it was or what was written. macOS writes in place, because the
sandbox grant covers the file and not its directory.

**Undo is a stack of snapshots** of the content tree, bounded at a hundred.
Every edit records the tree as it stands and then mutates; the document is
modified when the stack is not at the depth it had when the file was last read
or written, so undoing back to that depth is a document with nothing to save.
Close, Open, Reload, Quit and the window's own close button ask before a
modified document is thrown away.
