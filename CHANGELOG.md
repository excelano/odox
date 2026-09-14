# Changelog

All notable changes to odox are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- German. The window draws in the desktop's language, and the desktop entries
  carry it too.
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
