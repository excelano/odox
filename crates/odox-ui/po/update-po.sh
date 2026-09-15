#!/bin/sh
# Re-read every translatable string out of the workspace, and bring each
# catalogue up to date with what it found.
#
# Run it after changing any sentence a person reads, and commit what it changes.
# It is a command rather than a build step because it needs `gettext`, which is
# on the Linux machine and on neither of the other two, and a build that quietly
# skips itself where a tool is missing is a build that silently ships last
# month's German.
#
# **The catalogue lives here, in the crate that reads it, and the sources it is
# extracted from do not.** `odox_ui::i18n` declares the catalogue and the `t`
# that looks a message up, so a string in `xodt` is looked up in *this* crate's
# catalogue — the storage belongs to the crate that invoked `potext::catalog!`.
# So the extraction globs every crate and the output stays under this one, which
# is also what `include_str!` requires: reaching above a crate root compiles
# locally and fails in `cargo package`, which flyleaf lost a release tag to.
#
# **The file list is a glob and never a `POTFILES.in`.** A list kept by hand goes
# stale the first time somebody adds a file, and the symptom is a catalogue that
# looks complete.
#
# **`xgettext` has no Rust.** Its `--language` list ends at Vala, so the files
# are handed to it as C. That reads `t("…")` correctly and keeps `//` comments
# out, and it joins adjacent string literals the way C does, which Rust does not
# — so write every message as one literal. A raw string inside `t(…)` extracts
# wrongly for the same reason; neither shape appears in this tree.
#
# **One literal also means one line.** A string continued with a backslash at
# the end of a line is the same trap wearing a different hat: C keeps the
# indentation that follows the continuation and Rust drops it, so the extracted
# msgid carries a run of spaces the runtime string will never have, and the
# translation silently never loads. `msgfmt` sees nothing wrong, because both
# files are valid. Keep the literal on one line however long it gets; `rustfmt`
# will not break a string anyway. The Duckling session hit this on a tooltip on
# 2026-09-14 and found it only by reading the generated `.pot`; no message in
# this tree is wrapped, which was checked rather than assumed.
#
# **It prints a screen of warnings and they are noise.** `unterminated character
# constant` is a Rust lifetime read as the start of a C character literal. They
# cost nothing and they are also where a genuine miss would hide, so the way a
# dropped string is found is not by reading them — it is `pseudo.sh`, where
# anything still in English stands out on sight.
#
# `--check` extracts to a temporary file and compares the message set with the
# committed template, writing nothing. CI runs that: a check that
# rewrote the file it was checking would leave the tree dirty after every run,
# which is what it did until 2026-09-14 — the template carries the moment it was
# extracted, so re-running it always changes a line.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$here/../../.."

check=no
[ "${1:-}" = --check ] && check=yes

domain=odox
committed="crates/odox-ui/po/$domain.pot"
pot="$committed"
if [ "$check" = yes ]; then
    pot=$(mktemp)
    trap 'rm -f "$pot"' EXIT
fi

# Sorted so that two runs on two machines produce the same file.
sources=$(find crates -name '*.rs' -not -path '*/target/*' | sort)

# `--keyword` with no argument first, which drops xgettext's built-in C
# keywords: `gettext` and its family are not what this calls, and leaving them
# in would extract from any function that happened to share a name.
#
# `tc:1c,2` says the first argument is the context and the second the message.
# `tn:1,2` says the first two are the singular and the plural. `mark` is
# gettext's `N_`: a message that must be a literal where no catalogue is in force
# yet, looked up through `t` where it is shown.
#
# No version in `Project-Id-Version`: it would be one more thing a release has
# to remember, and every catalogue would show a diff for every release that
# changed nothing anybody translates.
xgettext \
    --language=C \
    --from-code=UTF-8 \
    --keyword \
    --keyword=t \
    --keyword=mark \
    --keyword=tc:1c,2 \
    --keyword=tn:1,2 \
    --add-comments=Translators: \
    --sort-by-file \
    --package-name="$domain" \
    --msgid-bugs-address=https://github.com/excelano/odox/issues \
    --output="$pot" \
    $sources

sed -i "s/^\"Project-Id-Version: $domain VERSION/\"Project-Id-Version: $domain/" "$pot"

# Only the message set is compared. The template records the moment it was
# extracted, so the file differs on every run and the messages do not.
if [ "$check" = yes ]; then
    # Temporary files and not process substitution: this script is `sh`, and
    # `<(...)` is bash.
    before=$(mktemp)
    after=$(mktemp)
    grep '^msgid ' "$committed" | sort > "$before"
    grep '^msgid ' "$pot" | sort > "$after"
    if diff -u "$before" "$after"; then
        rm -f "$before" "$after"
        echo "the catalogue template is current"
        exit 0
    fi
    rm -f "$before" "$after"
    echo "the template is behind the source — run $0 and commit what it changes" >&2
    exit 1
fi

# **`charset=CHARSET` is a trap when every message is ASCII.** `msginit` reads
# the placeholder, sees nothing but ASCII, and writes `charset=ASCII` into the
# new catalogue — after which the first German word makes `msgfmt` refuse the
# file with *invalid multibyte sequence*. Declared here so no catalogue starts
# life wrong.
sed -i 's/charset=CHARSET/charset=UTF-8/' "$pot"

# `msgmerge` is the whole reason this speaks `.po`: where a message's English has
# changed it finds the entry the new text descended from, carries the old German
# over, and marks it `#, fuzzy` — and `potext` refuses a fuzzy entry, so the
# window falls back to English until a person has read the new sentence. A
# translation is never silently wrong; it is current or visibly absent.
for catalogue in crates/odox-ui/po/*.po; do
    [ -e "$catalogue" ] || continue
    msgmerge --update --backup=none --previous "$catalogue" "$pot"
    # Syntax is caught here, before a commit, because `potext` cannot report it
    # at run time: a catalogue is compiled into the binary, and an application
    # that refused to start over a stray quote in a translation would be worse
    # than one that showed English.
    msgfmt --check --output-file=/dev/null "$catalogue"
    printf '%s: ' "$catalogue"
    msgfmt --statistics --output-file=/dev/null "$catalogue" 2>&1
done
