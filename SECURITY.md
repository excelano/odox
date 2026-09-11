# Security

## Supported versions

odox is built from this repository and has had no release. Until it has, the
supported version is the current `main`, and a report is answered against that.

## What these applications can reach

They open a file a person chose, and nothing else. There is no network code in
any of the three: no update check, no telemetry, no font or picture fetched from
a URL — a picture referenced by a document is read from inside the package or
drawn as an empty frame.

They do not write. A document opened is never modified, and nothing is written
anywhere on the file system.

## What they store

Nothing. No configuration file, no cache, no recently-opened list, no temporary
copy of a document. What a window remembers, it remembers until it closes.

## The shape of the risk

The input is an untrusted zip archive of untrusted XML. The reader is written in
safe Rust: `odox-core` is `#![forbid(unsafe_code)]` and the three applications are
`#![deny(unsafe_code)]`, so a malformed document cannot corrupt memory. What it
can do is cost time or memory, and the reader is written against that — a repeat
count is never expanded into the cells it stands for, and a cycle in a style's
inheritance chain stops rather than hanging.

## Reporting

Report a vulnerability privately through GitHub's *Report a vulnerability* on
this repository, or by email to david.anderson@excelano.com. Please include the
document that triggers it where you can; a document that reproduces a crash is
worth more than a description of one, and it becomes a corpus fixture.
