#!/bin/sh
# Export text documents to PDF/UA and check each with veraPDF's ua1 profile.
#
#   packaging/verapdf.sh document.odt ...
#
# With no arguments it checks every .odt in corpus/, as the conformance test
# does. Figures with nothing to say are marked decorative first, which is
# what the export dialog's "mark all decorative" does. veraPDF has to be on
# the path: https://verapdf.org/software/.
#
# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

set -eu

cd "$(dirname "$0")/.."
command -v verapdf >/dev/null || { echo "verapdf is not on the path" >&2; exit 2; }
cargo build -q -p odox-pdf --example export
target=$(cargo metadata --format-version 1 --no-deps | sed 's/.*"target_directory":"\([^"]*\)".*/\1/')
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

if [ $# -eq 0 ]; then
	set -- $(find corpus -name '*.odt')
fi
status=0
for document in "$@"; do
	pdf="$out/$(basename "$document" .odt).pdf"
	if "$target/debug/examples/export" "$document" "$pdf"; then
		verapdf --flavour ua1 --format text "$pdf" | sed "s|$pdf|$document|" || status=1
	else
		status=1
	fi
done
exit $status
