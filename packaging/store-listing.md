# Store listing text

One draft for three applications on two stores, written to the shorter of each
pair of limits. What is shared sits at the top; what differs sits in a section
per application. `store-listing.de-de.md` is the German half, under the same
headings. Count the limits rather than estimating, in both languages, after any
edit:

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

The short description is written to 500 although Partner Center's form takes
1,000: the submission API refuses more, and it refuses while copying the
published listing into the next draft, so a longer one blocks every later upload.

---

# Shared

## URLs

All three listings carry the same values:

| Field | URL |
| --- | --- |
| Privacy policy | https://excelano.com/legal/#odox |
| Support | https://excelano.com/odox/#support |
| Marketing / website | https://excelano.com/odox/ |

## Release notes

*What's new in this version* on the Microsoft Store and *What's New* on the Mac
App Store, one version's text each, latest first.

### 0.4.0

Odox now edits. A document opens reading; Edit mode, from the Edit menu or Ctrl+E, lets you change what is there: text in a paragraph, a value in a cell, the place and size of a shape on a slide, and the text in it. Undo takes it back and Save writes the document back as it was opened, with your change in it. What you did not touch is written back as it was read, element for element, and a save that would not read back the same is refused rather than written. One preference, opening documents ready to edit, is kept in a settings file that is written only when you change it.

### 0.3.0

First release.

## App Review notes

The same notes for all three, with one sentence differing, marked below.

Odox is three lightweight editors for OpenDocument files: Odox Text for text
documents, Odox Grid for spreadsheets, Odox Deck for presentations. No account,
no sign-in, no test credentials, and no network connection of any kind are
needed to test any of them.

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

**Editing is changing what is there.** A window opens reading; Edit mode, from
the Edit menu or Ctrl+E, lets a reviewer type into a paragraph, a cell or a
slide's text, and drag or resize a shape. Save writes the document back over
the file that was opened and Save As writes it elsewhere; nothing else is
written, except one preference, in a settings file, when it is changed in the
menu. The bundle declares `CFBundleTypeRole` as `Editor`. There is no Export,
no formatting, no inserting and no formula editing, which is the scope of the
application rather than a limitation of the review build.

Each declares its one OpenDocument type at rank `Alternate` rather than `Default`
or `Owner`. OpenDocument is an OASIS standard that this project reads and does
not own, on a machine that may well have a full office suite already claiming
it, so the application adds itself to the list a person can choose from and does
not ask to be preferred.

The App Sandbox is on with two entitlements: the sandbox itself, and read-write
access to user-selected files, which is the grant a person gives by choosing a
document in the open panel or naming one in the save panel. There is no network
entitlement and no temporary exception. The application makes no network
request; it writes the document a person saves, in place, and its one
preference inside its own container.

The full privacy statement is at https://excelano.com/legal/#odox and the
complete source is at https://github.com/excelano/odox.

## Keywords, shared terms

Each application's own list is in its section. All three carry `opendocument`
and `odf`, and none carries the name of another office suite.

## Screenshots

Each lane takes its own with its platform's script (`packaging/windows/shots.ps1`,
`packaging/macos/screenshot.sh`), against the packaged application, light theme
first, with the pointer parked off the window and the window photographed by its
handle. The Windows lane names the executable rather than shelling out to the
document, because an install here adds each application to `OpenWithProgids`
and never writes `UserChoice`, so on a machine with a full office suite the
shell opens the office suite. Something is open in every shot: a screenshot of an empty window is what
guideline 2.3.3 sends back. Each lane records its set here with the version, the
commit and the resolution.

---

# Odox Text

## App name

    Microsoft Store   Odox Text
    Mac App Store     Odox Text

The binary is `xodt` and stays lowercase. Partner Center, App Store Connect,
`Package/Properties/DisplayName`, `CFBundleDisplayName` and the product page all
carry the name above.

## Subtitle (Mac App Store, 30)

Edit OpenDocument text

## Promotional text (Mac App Store, 170)

Open an OpenDocument text document, read it as it was written, fix what needs fixing, and save it back as the document it was.

## Short description (Microsoft Store, 500)

A small, fast editor for OpenDocument text documents, the `.odt` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws the page at the width the document asks for, with its headings, lists, tables and pictures where the document puts them, and an outline beside it. Click a paragraph to change it, take it back with Undo, and save: what you did not touch is written back as it was read, so the document you save is the one you opened.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.odt` text documents written by any application that follows the OpenDocument standard
- Draws the page at the width the document asks for, with its own fonts resolved against the ones you have installed
- Headings, nested lists, tables with merged cells, and the pictures inside the document, each where the document puts it
- An outline beside the page that moves it when you click a heading
- Select across paragraphs and copy, or copy the whole document as plain text
- Click a paragraph to change it, split it with Enter or join it with Backspace, and take any of it back with Undo
- Saves over the file you opened or somewhere else, and refuses a save that would not read back as the document in the window
- What you did not touch is written back as it was read, element for element, including what Odox has never heard of
- Writes nothing else: no temporary file left beside your document, no backup, no cache; one preference in a settings file, only when you change it
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Text opens an OpenDocument text document, lets you change what is in it, and saves it back as the document it was.

It is a lightweight editor and not a word processor. A document opens reading; Edit mode, from the Edit menu or Ctrl+E, lets you click a paragraph and change its text where it sits, start a new paragraph with Enter, join two with Backspace, take any of it back with Undo, and save. You cannot apply formatting, insert a table or a picture, or find and replace: for that you have an office suite. Nothing is written until you press Save, and then only the file you opened or the one you named; no temporary file is left beside it and no backup is kept. The one preference the application keeps, whether a document opens ready to edit, goes into a settings file only when you change it. There is no recent-documents list and no cache.

It draws the page at the width the document asks for. Headings, nested lists, tables with merged cells and borders, and the pictures inside the document appear where the document puts them, in the fonts it asks for, resolved against the ones you have installed. An outline sits beside the page and moves it when you click a heading. You can select across paragraphs and copy, or take the whole document as plain text in one command.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Grid reads spreadsheets and Odox Deck reads presentations, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

What an editor has to do and most do not is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and what you did not touch is written back as it was read, element for element. A test that every document in the corpus comes back the same, and that an edit to any paragraph of it changes that paragraph and nothing else, runs on every change. Before a save, what is about to be written is read back and compared with the document in the window, and a difference refuses the save rather than writing a document that would not come back the same.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odt,document,editor,edit,text,oasis,open,office

**Microsoft Store** (seven terms):

    opendocument, odf, odt, document editor, edit document, text document, oasis

---

# Odox Grid

## App name

    Microsoft Store   Odox Grid
    Mac App Store     Odox Grid

The binary is `xods`.

## Subtitle (Mac App Store, 30)

Edit OpenDocument sheets

## Promotional text (Mac App Store, 170)

Open an OpenDocument spreadsheet at its own column widths and cell styles, change the cells that need changing, and save it back as the document it was.

## Short description (Microsoft Store, 500)

A small, fast editor for OpenDocument spreadsheets, the `.ods` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws the sheet at the document's own column widths and cell styles, with one tab per sheet and the formula behind whichever cell you pick. Type into a cell to change it, take it back with Undo, and save: the formulas and everything you did not touch are written back as they were read, so the sheet you save is the one you opened.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.ods` spreadsheets written by any application that follows the OpenDocument standard
- Draws the sheet at the document's own column widths, row heights and cell styles
- One tab per sheet, and the formula behind whichever cell you pick shown above the grid
- Numbers, dates and booleans shown the way the document says to show them
- A sheet with a large empty gap in it opens as fast as one without, because the gap is never expanded
- Copy the whole sheet as tab-separated text, ready to paste anywhere
- Type into a cell to change it: a number is a number, TRUE and FALSE are booleans, anything else is text, and Undo takes it back
- Formulas are kept and not edited; once anything has changed their results are drawn faint until a spreadsheet application recalculates them
- Saves over the file you opened or somewhere else, and refuses a save that would not read back as the document in the window
- Writes nothing else: no temporary file left beside your document, no backup, no cache; one preference in a settings file, only when you change it
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Grid opens an OpenDocument spreadsheet, lets you change the cells that need changing, and saves it back as the document it was.

It is a lightweight editor and not a spreadsheet application. Typing on the cell you picked replaces it, Enter or F2 opens it with what it holds, Delete clears it, and Undo takes any of it back. A number is a number, TRUE and FALSE are booleans, anything else is text. A cell that holds a formula is kept as it is and not edited, and once anything in the sheet has changed every formula's result is drawn faint until a spreadsheet application recalculates it, which LibreOffice does on opening the file. Nothing is written until you press Save, and then only the file you opened or the one you named; no temporary file is left beside it and no backup is kept. The one preference the application keeps, whether a document opens ready to edit, goes into a settings file only when you change it. There is no recent-documents list and no cache.

It draws the sheet at the document's own column widths and row heights, with the cell styles the document carries, and shows numbers, dates and booleans the way the document says to show them. There is one tab per sheet, and picking a cell shows the formula behind it above the grid. Take the whole sheet as tab-separated text in one command and paste it wherever you need it.

A spreadsheet often has a gap in it, a run of ten thousand empty rows between one block of figures and the next, and a reader that turns that gap into ten thousand rows in memory is slow to open and slow to scroll. Odox Grid keeps the gap as the document writes it, which is why a sparse sheet opens as fast as a dense one.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Text reads text documents and Odox Deck reads presentations, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

What an editor has to do and most do not is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and what you did not touch is written back as it was read, element for element: a run of a thousand identical rows the document wrote once is split around the one cell you changed and the rest stay one run. A test that every document in the corpus comes back the same, and that an edit changes one cell and no other, runs on every change. Before a save, what is about to be written is read back and compared with the document in the window, and a difference refuses the save rather than writing a document that would not come back the same.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,ods,spreadsheet,editor,edit,sheet,oasis,calc,csv

**Microsoft Store** (seven terms):

    opendocument, odf, ods, spreadsheet editor, edit spreadsheet, sheet, oasis

---

# Odox Deck

## App name

    Microsoft Store   Odox Deck
    Mac App Store     Odox Deck

The binary is `xodp`.

## Subtitle (Mac App Store, 30)

Edit OpenDocument slides

## Promotional text (Mac App Store, 170)

Open an OpenDocument presentation and see each slide as it was designed; move a shape, resize it or change its text, and save the deck back as it was.

## Short description (Microsoft Store, 500)

A small, fast editor for OpenDocument presentations, the `.odp` files written by LibreOffice, OpenOffice and everything else that speaks the OASIS standard.

It draws each slide at the size the document sets, with the master page's background and decorations behind it and the shapes drawn from their own geometry. Pick a shape and drag it, resize it by a corner, or click its text to change it; take it back with Undo, and save: what you did not touch is written back as it was read.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Opens `.odp` presentations written by any application that follows the OpenDocument standard
- Draws each slide at the size the document sets, on the master page's own background
- The decorations a template puts behind every slide, drawn rather than skipped
- Shapes drawn from their own geometry: paths, custom shapes, connectors and the labels they carry
- Solid, linear and axial gradient fills, and the pictures a slide frames
- The speaker's notes underneath each slide, and a list of slides beside
- Pick a shape and drag it, resize it by a corner, or click its text to change it where it sits, and take any of it back with Undo
- Saves over the file you opened or somewhere else, and refuses a save that would not read back as the document in the window
- What you did not touch is written back as it was read, element for element, the master page and the template's decorations included
- Writes nothing else: no temporary file left beside your document, no backup, no cache; one preference in a settings file, only when you change it
- No network connection of any kind, no account, and no telemetry
- Follows your desktop's light or dark setting
- Speaks English and German
- Free and open source under the MIT licence

## Description (both, written to 4,000)

Odox Deck opens an OpenDocument presentation, lets you change what is on its slides, and saves it back as the deck it was.

It is a lightweight editor and not a presentation application. A deck opens reading; Edit mode, from the Edit menu or Ctrl+E, lets you pick a shape and drag it where it should be, resize it by a corner, click its text and change it where it sits, take any of it back with Undo, and save. You cannot add a slide or a shape, apply formatting, or change a master page: for that you have an office suite. Nothing is written until you press Save, and then only the file you opened or the one you named; no temporary file is left beside it and no backup is kept. The one preference the application keeps, whether a document opens ready to edit, goes into a settings file only when you change it. There is no recent-documents list and no cache.

It draws each slide at the size the document sets. Behind the slide's own text and pictures is the master page: the ground it fills and the decorations a template puts on every slide, which is most of what makes a deck look like the template it was built from. The shapes are drawn from the geometry the document states rather than approximated into rectangles, which covers paths, custom shapes with their own formulas, connectors routed between the shapes they join, and the labels a shape carries. The speaker's notes sit underneath the slide and a list of slides beside it.

How well it draws is measured rather than asserted. Every slide of the templates in its corpus is rendered by Odox Deck and by LibreOffice from the same file, and the two are compared colour by colour, because a renderer is the kind of thing that passes every test and still looks wrong.

It makes no network connection of any kind. There is no server behind it, no account to create, no analytics, no telemetry, no crash reporting and no third-party service. Its store privacy declaration is Data Not Collected, because the developer collects nothing and has nowhere to put it. The full statement is at https://excelano.com/legal/#odox and, because the source is open, every line of it is verifiable.

It is one of three. Odox Text reads text documents and Odox Grid reads spreadsheets, and all three are built on one library that reads the format and one renderer that draws it, so a paragraph looks the same in a document, a spreadsheet cell and a slide.

What an editor has to do and most do not is keep the whole document. Every element, attribute and piece of whitespace in your file survives being read, including the ones Odox has never heard of, and what you did not touch is written back as it was read, element for element: a moved shape is four attributes changed, in the unit the document wrote them in, and nothing else. A test that every document in the corpus comes back the same runs on every change. Before a save, what is about to be written is read back and compared with the document in the window, and a difference refuses the save rather than writing a document that would not come back the same.

Written in Rust, free, and open source under the MIT licence, at https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odp,presentation,slides,editor,edit,deck,oasis,impress

**Microsoft Store** (seven terms):

    opendocument, odf, odp, presentation editor, edit slides, slides, oasis
