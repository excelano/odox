# Changelog

All notable changes to odox are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `xodt`, `xods` and `xodp`: viewers for OpenDocument text documents,
  spreadsheets and presentations, over one shared library and one shared window.
- `odox-core`, which reads and writes an OpenDocument package and keeps every
  part of a document it was given, including the parts it has no opinion about.
- `odox-ui`, which maps ODF styles onto what egui draws with and renders the
  content model the three formats share.
- A Debian package per application, desktop entries and icons.
