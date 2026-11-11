# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately, using the **Report a vulnerability** button on the Security tab of the repository concerned. If that button is not there, or you would rather not go through GitHub, email david.anderson@excelano.com instead. I aim to respond within seven days.

Please do not open public issues for security problems.

Include the document that triggers it where you can; a document that
reproduces a crash is worth more than a description of one, and it becomes a
corpus fixture.

## Supported versions

The latest release receives security fixes. Older versions are not supported.
A fix ships in a new release; update the way you installed.

## What Odox can access

The applications open a file a person chose, write where a person chose, and
reach nothing else. There is no network code in any of them: no update check,
no telemetry, no font or picture fetched from a URL — a picture referenced by a
document is read from inside the package or drawn as an empty frame.

They write only where asked. Save writes the document back over the file it was
opened from, and Save as and the PDF export write to the path chosen in the
dialog. A write goes to a `.part` file beside the target that is then renamed
over it, so the file is either what it was or what was written and not half of
each; on macOS, where the sandbox grant covers the file and not its directory,
the document is written in place.

## What Odox stores

One settings file, `odox/settings.toml` under the platform's configuration
directory (`$XDG_CONFIG_HOME` or `~/.config` on Linux, `Library/Application
Support` on macOS, roaming `AppData` on Windows), written only when a person
changes a preference in the menu, so an installation nobody has configured has
no file. No cache, no recently-opened list, and no temporary copy of a document
beyond the `.part` file a save passes through. On macOS, File > New in a window
that already holds a document leaves an empty marker file in the temporary
directory, which the new instance deletes as it starts. Anything else a window
remembers, it remembers until it closes.

## The shape of the risk

The input is an untrusted zip archive of untrusted XML. The reader is written in
safe Rust: `odox-core` and the applications are `#![forbid(unsafe_code)]`,
and the shared `odox-ui` is `#![deny(unsafe_code)]` with an `allow` only on the
modules that receive a double-clicked document from macOS and ask macOS for a
new instance, both compiled for that platform alone (DESIGN.md §2), so a
malformed document cannot corrupt memory. What it
can do is cost time or memory, and the reader is written against that — a repeat
count is never expanded into the cells it stands for, and a cycle in a style's
inheritance chain stops rather than hanging.
