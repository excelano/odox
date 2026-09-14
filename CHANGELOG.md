# Changelog

All notable changes to odox are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `draw:path`, which states a shape's outline as SVG path data rather than in
  ODF's own commands. It flattens through the same pen the custom shapes use,
  so a path fills, strokes and holds text the way every other shape does. An
  elliptical arc inside one is drawn as the straight line to where it ends.

## [0.2.1] — 2026-09-14

### Changed

- The `odox` package depends on the three viewers rather than recommending
  them, so `apt install odox` installs the suite. Recommending them installed
  them on a default Debian and not on one with `APT::Install-Recommends` off,
  and a launcher whose viewers are absent can only apologise.

## [0.2.0] — 2026-09-14

The presentation viewer draws slides rather than listing their text, the page
can be selected and copied from, and the window speaks German.

### Added

- A slide is drawn with its master page: the ground it fills, and the shapes a
  template decorates every slide with. Rectangles, ellipses, polygons, polylines,
  lines and custom shapes are drawn from their own geometry, with solid,
  gradient and stretched-picture fills.
- `draw:enhanced-geometry`, which is how a custom shape states its outline: a
  path in a coordinate space of its own, with numbers that may be formulas.
- Text in the page can be selected and copied: drag, double-click a word,
  triple-click a line, Ctrl+C. A selection survives scrolling and spans
  paragraphs.
- German. The window draws in the desktop's language, and the desktop entries
  carry it too.
- A Debian package for the `odox` launcher, which until now was on crates.io
  and in no package. It depends on neither the window libraries nor the viewers:
  it draws nothing, and it recommends the three rather than requiring them.
- `odox`, a launcher: it takes a file, works out which of the three viewers
  reads it, and becomes that one. The extension answers first and the package's
  own media type answers where the extension cannot. `--which` names the viewer
  and launches nothing. On crates.io; in no Debian package yet.

## [0.1.0] — 2026-09-11

First release. Three viewers, no editing.

### Added

- `xodt`, `xods` and `xodp`: viewers for OpenDocument text documents,
  spreadsheets and presentations, over one shared library and one shared window.
  `xodt` draws a document at the width its page layout asks for, with an outline
  beside it; `xods` draws a sheet as a grid with the document's own widths and
  cell styles, one tab per sheet, and the formula behind the cell you pick;
  `xodp` draws each slide at the size the document sets, with the speaker's
  notes. Presentation shapes that are not text or a picture are outlined rather
  than drawn, and a master page contributes neither its background nor its
  placeholder geometry — both are the next release's.
- `odox-core`, which reads and writes an OpenDocument package and keeps every
  part of a document it was given, including the parts it has no opinion about.
  That is what makes the round trip measurable, and it is measured over a corpus
  of documents LibreOffice wrote.
- `odox-ui`, which maps ODF styles onto what egui draws with and renders the
  content model the three formats share.
- A Debian package per application, desktop entries, icons and man pages.
- The release machinery: the fleet's Rust CI, a workflow that builds the six
  packages on a runner of each architecture and attaches them to the release,
  and a workflow that publishes the five crates in dependency order.
- The translation mechanism: one `.po` catalogue for the suite, the extraction
  and pseudolocale scripts, and a pseudolocale compiled into debug builds alone.
  No translation ships yet.
