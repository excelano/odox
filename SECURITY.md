# Security

## Supported versions

The latest release receives security fixes. Older releases are not supported.

## What these applications can reach

They open a file a person chose, write where a person chose, and reach nothing
else. There is no network code in any of the three: no update check, no
telemetry, no font or picture fetched from a URL — a picture referenced by a
document is read from inside the package or drawn as an empty frame.

They write only where asked. Save writes the document back over the file it was
opened from, and Save as and the PDF export write to the path chosen in the
dialog. A write goes to a `.part` file beside the target that is then renamed
over it, so the file is either what it was or what was written and not half of
each; on macOS, where the sandbox grant covers the file and not its directory,
the document is written in place.

## What they store

One settings file, `odox/settings.toml` under the platform's configuration
directory (`$XDG_CONFIG_HOME` or `~/.config` on Linux, `Library/Application
Support` on macOS, roaming `AppData` on Windows), written only when a person
changes a preference in the menu, so an installation nobody has configured has
no file. No cache, no recently-opened list, and no temporary copy of a document
beyond the `.part` file a save passes through. Anything else a window remembers,
it remembers until it closes.

## The shape of the risk

The input is an untrusted zip archive of untrusted XML. The reader is written in
safe Rust: `odox-core` and the three applications are `#![forbid(unsafe_code)]`,
and the shared `odox-ui` is `#![deny(unsafe_code)]` with one `allow` on the
module that receives a double-clicked document from macOS, compiled for that
platform alone (DESIGN.md §2), so a malformed document cannot corrupt memory. What it
can do is cost time or memory, and the reader is written against that — a repeat
count is never expanded into the cells it stands for, and a cycle in a style's
inheritance chain stops rather than hanging.

## Reporting

Report a vulnerability privately through GitHub's *Report a vulnerability* on
this repository, or by email to david.anderson@excelano.com. Please include the
document that triggers it where you can; a document that reproduces a crash is
worth more than a description of one, and it becomes a corpus fixture.
