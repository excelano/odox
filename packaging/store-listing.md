# Store listing text

One draft, used six times: three applications on two stores. Both stores want
the same things at different lengths, so everything here is written to the
shorter limit. Nothing here is submitted yet; the Windows and Mac lanes copy
from this file into their forms and record in their `SUBMITTING.local.md` what
the form did with it.

This is written from the release notes, not beside it. Every claim below appears
there first, checked against the built applications. If the two disagree, the
changelog is right and this is stale.

**Three listings and not one.** A person opens a spreadsheet and a presentation
through different doors, which is why these are three applications, and it is
why each has its own store record. What is shared sits at the top of this file
and is written once; what differs sits in a section per application. A sentence
that appears in all three is written in the shared part and referred to, rather
than copied and left to drift.

Limits, so a later edit does not overrun them. **Count them rather than
estimating**, in both languages, after any edit to either: German ran four
fields over on its first draft and every one was a sentence that reads fine.

    python3 - <<'EOF'
    import re
    limits = {'Subtitle (Mac App Store, 30)': 30,
              'Promotional text (Mac App Store, 170)': 170,
              'Short description (Microsoft Store, 500)': 500,
              'Description (both, written to 4,000)': 4000}
    for path in ('packaging/store-listing.md', 'packaging/store-listing.de-de.md'):
        s = open(path).read()
        for app in ('Odox Text', 'Odox Grid', 'Odox Deck'):
            at = s.index(f'\n# {app}\n')
            nxt = s.find('\n# ', at + 1)
            block = s[at: nxt if nxt != -1 else len(s)]
            for head, cap in limits.items():
                body = re.search(rf'^## {re.escape(head)}[^\n]*\n\n(.*?)(?=\n## |\Z)',
                                 block, re.S | re.M).group(1).strip()
                if len(body) > cap:
                    print(f'{path} {app} {head}: {len(body)} over {cap}')
    EOF


| Field | Microsoft Store | Mac App Store |
| --- | --- | --- |
| App name | unmeasured | 30 |
| Description | 10,000 | 4,000 |
| Short description | 500 | — |
| Subtitle | — | 30 |
| Promotional text | — | 170 |
| Keywords | 7 terms | 100 characters |

**The API's limit is not the form's, and the API's is the one that binds.**
Partner Center's form takes 1,000 characters of short description. The
submission API refuses anything over 500, and it refuses it while copying the
*published* listing into the new draft, so a listing that went up through the
form over 500 blocks the next upload before the new text is ever sent. The fleet
measured that on Duckling. Nothing of Odox's has been submitted, so writing to
500 from the start is what keeps that wall from being built here.

---

# Shared

## URLs

Both forms ask for the same three, all three listings carry the same values, and
both lanes take them from here:

| Field | URL |
| --- | --- |
| Privacy policy | https://excelano.com/legal/#odox |
| Support | https://excelano.com/odox/#support |
| Marketing / website | https://excelano.com/odox/ |

The page at `excelano.com/odox/` is the support and marketing URL both, the way
every other application's is. It is a suite page rather than an application
page, because all three listings point at it and a visitor arrives knowing about
one of three. The store badges and the apt install block appear on it as each
lands, by switches at the top of its source.

## Release notes

*What's new in this version* on the Microsoft Store and *What's New* on the Mac
App Store, one version's text each, written from the release notes and kept latest
first. Only the version being submitted needs a subsection. Everything before
0.3.0 has none, because neither store has ever served Odox and there is nobody
upgrading from those to tell.

### 0.3.0

First release.

## App Review notes

The same notes for all three, with one sentence differing, marked below.

Odox is three readers for OpenDocument files: Odox Text for text documents, Odox
Grid for spreadsheets, Odox Deck for presentations. No account, no sign-in, no
test credentials, and no network connection of any kind are needed to test any
of them.

**Open a document first. With nothing open the window is empty and there is
nothing to try.** These read from the repository and need no release asset:

| Application | Document |
| --- | --- |
| Odox Text | https://github.com/excelano/odox/raw/main/corpus/libreoffice/text.odt |
| Odox Grid | https://github.com/excelano/odox/raw/main/corpus/libreoffice/calc.ods |
| Odox Deck | https://github.com/excelano/odox/raw/main/corpus/libreoffice/growing-liberty.odp |

Each was written by LibreOffice rather than by hand, so what you are opening is
what an office application actually emits. Nothing in any of them is anybody
else's copyright. Any OpenDocument file of your own will do as well.

**These applications read and do not write.** There is no Save, no Save As and
no Export; the File menu offers Open, Reload, Close and Quit. This is why the
bundle declares `CFBundleTypeRole` as `Viewer` rather than `Editor`, and it is
the honest description of what the application does rather than a limitation of
the review build.

Each declares its one OpenDocument type at rank `Alternate` rather than `Default`
or `Owner`. OpenDocument is an OASIS standard that this project reads and does
not own, on a machine that may well have a full office suite already claiming
it, so the application adds itself to the list a person can choose from and does
not ask to be preferred.

The App Sandbox is on with exactly two entitlements: the sandbox itself, and
read-only access to user-selected files, which is the grant a person gives by
choosing a document in the open panel. There is no network entitlement, no
temporary exception, and no write entitlement of any kind, because nothing is
written. The application makes no network request and creates no file anywhere.

The full privacy statement is at https://excelano.com/legal/#odox and the
complete source is at https://github.com/excelano/odox.

## Keywords, shared terms

Each application's own list is in its section. All three carry `opendocument`
and `odf`, because that is what a person searching for any of them types, and
none carries the name of another office suite.

## Screenshots

Each lane takes its own with its platform's script, against the packaged
application, light theme first because both platforms ship light by default,
with the pointer parked off the window and the window photographed by its
handle. **Something is open in every shot**, because a screenshot of an empty
window is what guideline 2.3.3 sends back, and because these applications show
nothing until a document is in them.

None taken yet. Each lane records its own set here when it does, with the
version, the commit and the resolution, the way the sibling repositories do.

---

# Odox Text

## App name

    Microsoft Store   Odox Text
    Mac App Store     Odox Text

Reserved on both, 2026-09-14. The binary is `xodt` and stays lowercase; the five
places carrying the product name agree: Partner Center, App Store Connect,
`Package/Properties/DisplayName`, `CFBundleDisplayName`, and the product page.

## Subtitle (Mac App Store, 30)

Read OpenDocument text

## Promotional text (Mac App Store, 170)

Open an OpenDocument text document and read it as it was written: headings, lists, tables and pictures where the document puts them, with an outline beside the page.

## Short description (Microsoft Store, 500)

A small, fast reader for OpenDocument text documents, the `.odt` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws the page at the width the document asks for, with its headings, lists, tables and pictures where the document puts them, and an outline beside it that moves the page when you click a heading. Select any of it and copy it out.

It reads and does not write. Your file is opened, drawn, and left exactly as it was.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.odt` text documents written by any application that follows the OpenDocument standard
- Draws the page at the width the document asks for, with its own fonts resolved against the ones you have installed
- Headings, nested lists, tables with merged cells, and the pictures inside the document, each where the document puts it
- An outline beside the page that moves it when you click a heading
- Select across paragraphs and copy, or copy the whole document as plain text
- Reads and never writes: no Save, no temporary file beside your document, nothing created anywhere
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Text opens an OpenDocument text document and shows you what is in it.

That is the whole of it. There is no Save, no Save As and no Export; the File menu offers Open, Reload, Close and Quit. The file you open is opened for reading and is left exactly as it was, with no temporary file beside it and no backup kept. Nothing is written anywhere on your disk, including by Odox Text itself: there is no settings file, no recent-documents list and no cache.

It draws the page at the width the document asks for. Headings, nested lists, tables with merged cells and borders, and the pictures inside the document appear where the document puts them, in the fonts it asks for, resolved against the ones you have installed. An outline sits beside the page and moves it when you click a heading. You can select across paragraphs and copy, or take the whole document as plain text in one command.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Grid reads spreadsheets and Odox Deck reads presentations, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

The thing it does that a reader does not have to do is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and a test that a file written back out parses to the same thing runs against every document in its corpus on every change. That costs a reader nothing today. It is there because a reader that throws away what it does not understand looks identical in a window and becomes an editor that quietly destroys documents the first time somebody saves one, and editing is where this is going.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odt,document,reader,viewer,text,oasis,open,office

**Microsoft Store** (seven terms):

    opendocument, odf, odt, document reader, document viewer, text document, oasis

---

# Odox Grid

## App name

    Microsoft Store   Odox Grid
    Mac App Store     Odox Grid

Reserved on both, 2026-09-14. The binary is `xods`.

## Subtitle (Mac App Store, 30)

Read OpenDocument sheets

## Promotional text (Mac App Store, 170)

Open an OpenDocument spreadsheet at its own column widths and cell styles, one tab per sheet, with the formula behind whichever cell you pick.

## Short description (Microsoft Store, 500)

A small, fast reader for OpenDocument spreadsheets, the `.ods` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws the sheet at the document's own column widths and cell styles, with one tab per sheet and the formula behind whichever cell you pick. A sheet with a gap of ten thousand empty rows in it opens as fast as one without.

It reads and does not write. Your file is opened, drawn, and left exactly as it was.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.ods` spreadsheets written by any application that follows the OpenDocument standard
- Draws the sheet at the document's own column widths, row heights and cell styles
- One tab per sheet, and the formula behind whichever cell you pick shown above the grid
- Numbers, dates and booleans shown the way the document says to show them
- A sheet with a large empty gap in it opens as fast as one without, because the gap is never expanded
- Copy the whole sheet as tab-separated text, ready to paste anywhere
- Reads and never writes: no Save, no temporary file beside your document, nothing created anywhere
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Grid opens an OpenDocument spreadsheet and shows you what is in it.

That is the whole of it. There is no Save, no Save As and no Export; the File menu offers Open, Reload, Close and Quit. The file you open is opened for reading and is left exactly as it was, with no temporary file beside it and no backup kept. Nothing is written anywhere on your disk, including by Odox Grid itself: there is no settings file, no recent-documents list and no cache.

It draws the sheet at the document's own column widths and row heights, with the cell styles the document carries, and shows numbers, dates and booleans the way the document says to show them. There is one tab per sheet, and picking a cell shows the formula behind it above the grid. Take the whole sheet as tab-separated text in one command and paste it wherever you need it.

A spreadsheet often has a gap in it, a run of ten thousand empty rows between one block of figures and the next, and a reader that turns that gap into ten thousand rows in memory is slow to open and slow to scroll. Odox Grid keeps the gap as the document writes it, which is why a sparse sheet opens as fast as a dense one.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Text reads text documents and Odox Deck reads presentations, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

The thing it does that a reader does not have to do is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and a test that a file written back out parses to the same thing runs against every document in its corpus on every change. That costs a reader nothing today. It is there because a reader that throws away what it does not understand looks identical in a window and becomes an editor that quietly destroys documents the first time somebody saves one, and editing is where this is going.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,ods,spreadsheet,reader,viewer,sheet,oasis,calc,csv

**Microsoft Store** (seven terms):

    opendocument, odf, ods, spreadsheet reader, spreadsheet viewer, sheet, oasis

---

# Odox Deck

## App name

    Microsoft Store   Odox Deck
    Mac App Store     Odox Deck

Reserved on both, 2026-09-14. The binary is `xodp`.

## Subtitle (Mac App Store, 30)

Read OpenDocument slides

## Promotional text (Mac App Store, 170)

Open an OpenDocument presentation and see each slide as it was designed: the master page behind it, the shapes drawn from their own geometry, the notes underneath.

## Short description (Microsoft Store, 500)

A small, fast reader for OpenDocument presentations, the `.odp` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws each slide at the size the document sets, with the master page's background and decorations behind it and the shapes drawn from their own geometry rather than approximated. The speaker's notes sit underneath, and a list of slides beside.

It reads and does not write. Your file is opened, drawn, and left exactly as it was.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.odp` presentations written by any application that follows the OpenDocument standard
- Draws each slide at the size the document sets, on the master page's own background
- The decorations a template puts behind every slide, drawn rather than skipped
- Shapes drawn from their own geometry: paths, custom shapes, connectors and the labels they carry
- Solid, linear and axial gradient fills, and the pictures a slide frames
- The speaker's notes underneath each slide, and a list of slides beside
- Reads and never writes: no Save, no temporary file beside your document, nothing created anywhere
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Deck opens an OpenDocument presentation and shows you what is in it.

That is the whole of it. There is no Save, no Save As and no Export; the File menu offers Open, Reload, Close and Quit. The file you open is opened for reading and is left exactly as it was, with no temporary file beside it and no backup kept. Nothing is written anywhere on your disk, including by Odox Deck itself: there is no settings file, no recent-documents list and no cache.

It draws each slide at the size the document sets. Behind the slide's own text and pictures is the master page: the ground it fills and the decorations a template puts on every slide, which is most of what makes a deck look like the template it was built from. The shapes are drawn from the geometry the document states rather than approximated into rectangles, which covers paths, custom shapes with their own formulas, connectors routed between the shapes they join, and the labels a shape carries. The speaker's notes sit underneath the slide and a list of slides beside it.

How well it draws is measured rather than asserted. Every slide of the templates in its corpus is rendered by Odox Deck and by LibreOffice from the same file, and the two are compared colour by colour, because a renderer is the kind of thing that passes every test and still looks wrong.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Text reads text documents and Odox Grid reads spreadsheets, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

The thing it does that a reader does not have to do is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and a test that a file written back out parses to the same thing runs against every document in its corpus on every change. That costs a reader nothing today. It is there because a reader that throws away what it does not understand looks identical in a window and becomes an editor that quietly destroys documents the first time somebody saves one, and editing is where this is going.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odp,presentation,slides,reader,viewer,deck,oasis,impress

**Microsoft Store** (seven terms):

    opendocument, odf, odp, presentation reader, slide viewer, slides, oasis
