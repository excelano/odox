# egui_richedit

Rich-text editing for [egui](https://github.com/emilk/egui), over paragraphs your
application lays out and draws itself.

egui's `TextEdit` edits one string in one format. A document editor draws
paragraphs of styled runs among lists, tables and pictures, and wants a caret that
moves through all of them as if the page were one surface: click anywhere and
type, Enter splits a paragraph, Backspace at its start joins it to the one before,
the arrows cross from one paragraph into the next, and a selection can run across
several. This crate is that caret. It owns no document and no layout. Your
application keeps both, and the crate asks it for two things:

- **A description of each paragraph's text** as it lays it out, built with
  `ParagraphJob`. Most text is appended with `text`. Anything drawn differently
  from what your document holds (a tab drawn as spaces, a footnote citation your
  document does not count as text, a field showing its value) is appended with
  `atom`, and the caret steps over it whole.
- **An implementation of `Model`** over your document: a paragraph's text, the
  paragraphs before and after it, and applying an `Edit` (replace a range, which
  may span a paragraph break, or split a paragraph). The editor tells the model
  when an edit begins a new undo step, so a run of typing is undone in one piece
  and your own undo keeps working.

In return, `RichEdit` handles keys, the pointer, the clipboard and input-method
commits, and paints the selection and the blinking caret over your galleys.

## Using it

Each frame, call `RichEdit::input` before anything is laid out, so the layout
that follows already shows what was typed. Then, for every editable paragraph in
document order, whether or not it is on screen:

1. Build its `ParagraphJob`.
2. Lay it out with egui.
3. Allocate a response that senses clicks and drags.
4. Hand the galley, the job's offset map and where you placed it to
   `RichEdit::paragraph`, which paints the paragraph.

The editor finds its way up and down through where each paragraph was drawn, so
paragraphs scrolled off screen are reported too.

`examples/notes.rs` is a complete window over a list of strings:

    cargo run -p egui_richedit --example notes

## Positions

A `Position` is a paragraph, named however your model likes, and a character
offset into your model's text of it. Offsets count characters, not bytes. They
count your document's text, not what is drawn; the offset map built by
`ParagraphJob` translates between the two.

## License

MIT.
