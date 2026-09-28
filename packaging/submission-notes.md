# Submission notes

What a store submission needs from a person and no file supplies: the answers
a form asks that no build can give.

The listing text itself is not here. It is `store-listing.toml` beside this,
which `ship` checks before the tag and pushes to both stores on every release,
and what a release tells them changed is `release-notes.toml`. A field edited
in this file would reach nobody. Apple's Notes for Review and Microsoft's
Notes for certification are there too, as `apple-review-notes` and
`microsoft-review-notes`, which `ship` pushes to `appStoreReviewDetail` and
`NotesForCertification` on every submission.

## Capability justification

Three separate Partner Center product records - Odox Text, Odox Grid, Odox
Deck - each declare `runFullTrust` and each carries its own capability
justification field, asked once when the capability is first declared on that
product rather than on every resubmission. It is not part of the submission
document `ship` reads and writes: none of the three's captured submissions or
`ship`'s own logs across many resubmissions of this repo carry a justification
string alongside `Capabilities`, and each has gone to certification since
without one being sent. 500-character limit, which counts newlines.

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
