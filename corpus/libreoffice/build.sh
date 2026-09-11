#!/bin/sh
# Regenerate the fixtures in this directory from the sources in `src/`.
#
# These are the corpus's *real producer* half: documents LibreOffice wrote, so
# that what the readers are tested against is what an office application
# actually emits rather than what this project believes it emits. The hand-built
# fixtures beside them exercise edge cases a producer will not write on request;
# these exercise the ordinary case, and they are where a wrong assumption about
# the format shows up.
#
# Each fixture's source is plain text and is committed, so a fixture can be
# rebuilt after a LibreOffice upgrade and the diff read.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

set -eu

cd "$(dirname "$0")"
out=$(pwd)

# Every call gets its own profile directory. Two concurrent invocations share
# the default profile and both fail with "source file could not be loaded",
# and a soffice left running from an earlier call breaks the next one the same
# way. A profile per call is what makes this script safe to run twice.
convert() {
	format=$1
	target=$2
	infilter=$3
	source=$4
	profile=$(mktemp -d)
	soffice -env:UserInstallation="file://$profile" \
		--headless \
		--infilter="$infilter" \
		--convert-to "$format" \
		--outdir "$profile" \
		"src/$source" >/dev/null 2>&1
	produced=$profile/$(basename "$source" | sed 's/\.[^.]*$//').${format%%:*}
	mv "$produced" "$out/$target"
	rm -rf "$profile"
	printf '%s\n' "$target"
}

# A text document. The HTML import has two filters and they produce different
# documents: the default for a `.html` input is Writer/Web, whose styles carry a
# thinner page layout than a Writer document's. Naming the Writer filter is what
# makes this a document of the kind a person writes.
convert odt:writer8 text.odt "HTML (StarWriter)" rich.html

# A spreadsheet of the ordinary kind: typed cells, a column span and the covered
# cell beneath it, column styles. HTML into Calc also needs its filter named,
# because the extension alone sends the file to Writer/Web, which has no
# spreadsheet to export.
convert ods sheet.ods "calc_HTML_WebQuery" sheet.html

# A spreadsheet of formulas, dates and booleans. The twelfth filter token is
# Calc's *evaluate formulas* switch; without it a leading `=` imports as text
# and the fixture carries no `table:formula` at all.
csv_options='44,34,76,1,,1033,false,true,false,false,false,true'
convert ods calc.ods "Text - txt - csv (StarCalc):$csv_options" calc.csv

# A spreadsheet with a gap in it, which is the fixture the row index exists for:
# Calc writes the empty span between the two occupied rows as one row element
# with a repeat count, and a reader that expands it allocates the whole sheet.
convert ods gaps.ods "Text - txt - csv (StarCalc):$csv_options" gaps.csv

# A presentation. Impress has no text format to import from, so the deck is
# built as PowerPoint first and converted; the result is an Impress document
# whose master pages and placeholders are Impress's own.
profile=$(mktemp -d)
pandoc src/deck.md -o "$profile/deck.pptx"
soffice -env:UserInstallation="file://$profile" --headless \
	--convert-to odp --outdir "$profile" "$profile/deck.pptx" >/dev/null 2>&1
mv "$profile/deck.odp" "$out/deck.odp"
rm -rf "$profile"
printf '%s\n' deck.odp
