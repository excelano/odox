# Checklist

Run against the **packaged** applications, never a developer build, one report
per item. Everything a machine can settle is in `.github/workflows/linux.yml`
and `packaging/preflight.sh`; what is here is what needs eyes and a session.

Install what the release built, rather than what is in `target/`:

    sudo apt install ./dist/xodt_*.deb ./dist/xods_*.deb ./dist/xodp_*.deb

## The desktop

1. Each application appears in the desktop's application list under its own
   name, with its own icon.
2. The icon reads at 16, 24, 32, 48 and 128 pixels, on a light ground and a dark
   one. Three drawings, one per application, and they are told apart at 16.
3. A `.odt`, a `.ods` and a `.odp` in a file manager each open the right
   application on a double-click, and each shows the desktop's own OpenDocument
   icon rather than a blank page.
4. The window's own icon in the task bar and the switcher is the application's
   and not egui's.

## Drawing

Open `corpus/libreoffice/text.odt`, `calc.ods` and `deck.odp`.

5. The text document draws its headings, its nested lists, its table and its
   character formatting; the outline lists its headings and clicking one moves
   the page.
6. The spreadsheet draws its columns at the document's own widths, its numbers
   against the right edge, its header band shaded, and the formula behind a
   picked cell in the bar.
7. The presentation draws each slide at the document's own shape, with its text
   where the document puts it.
8. The page is paper in a dark desktop as much as a light one, and the chrome
   around it follows the desktop.
9. Zoom in and out and back to actual size; the page keeps its proportions.
10. Drag across two paragraphs, press Ctrl+C, paste elsewhere: the selection
    highlights as it grows, survives scrolling the page, and the paste is the
    text in order. A double-click takes a word.

## Language

11. `POTEXT_LANG=en-x-pseudo` on a debug build: every sentence a person reads is
    bracketed and accented, and nothing but an application's own name is still
    in English. No label has its end cut off.

## Behaviour

12. Dropping a document on the window opens it; so does Ctrl+O; so does a path
    on the command line.
13. Handing an application the wrong kind of document names the sibling that
    reads it rather than failing silently.
14. Ctrl+R re-reads a document that changed on disk.
