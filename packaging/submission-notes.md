# Submission notes

What a store submission needs from a person and no file supplies. The listing
text, Apple's Notes for Review and Microsoft's Notes for certification are
`store-listing.toml`, and what a release changed is `release-notes.toml`; the
release apps push both. A field edited here reaches nobody.

## Capability justification

Odox Text, Odox Grid and Odox Deck are three Partner Center products, and each
declares `runFullTrust`. Partner Center asks for the justification once per
product, when the capability is first declared, and keeps the answer against
that product. 500 characters, newlines counted.

**Odox Text**

> Odox Text is a full-trust Win32 desktop application packaged as MSIX. It
> needs this capability to run at all. It opens the .odt file it was launched
> with, or one a save dialog names, and writes back only that file. It makes
> no network connection, needs no broad filesystem access, and uses no
> device.

**Odox Grid**

> Odox Grid is a full-trust Win32 desktop application packaged as MSIX. It
> needs this capability to run at all. It opens the .ods file it was launched
> with, or one a save dialog names, and writes back only that file. It makes
> no network connection, needs no broad filesystem access, and uses no
> device.

**Odox Deck**

> Odox Deck is a full-trust Win32 desktop application packaged as MSIX. It
> needs this capability to run at all. It opens the .odp file it was launched
> with, or one a save dialog names, and writes back only that file. It makes
> no network connection, needs no broad filesystem access, and uses no
> device.

## The answers a form asks

The same for all three products.

    Category                  Productivity
    Copyright and trademark   Excelano LLC
    Additional license terms  blank; the applications are MIT and the
                              repository carries the license
    Privacy policy            https://excelano.com/legal/#odox
    App Privacy (Apple)       Data Not Collected
    Price                     free

## Screenshots

Each product's frames are taken by `shots-winstore` and `shots-macstore` from
the keyboard recipes in `windows/shots.ps1` and `macos/shots.sh`, one
application per run: the document at rest, then edit mode with something
selected or half typed, and Find with its matches lit.
