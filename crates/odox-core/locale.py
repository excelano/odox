"""Write src/locales.rs from what LibreOffice shows in each locale: month,
day and quarter names, TRUE and FALSE, the decimal and group separators and
how digits are grouped, and the currency a new amount is shown in.
Run as `python3 locale.py > src/locales.rs && cargo fmt` from this directory.
"""

# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

import subprocess
import sys
import tempfile
import time

import uno
from com.sun.star.beans import PropertyValue
from com.sun.star.lang import Locale

LOCALES = [
    "en-US", "en-GB", "en-AU", "en-CA", "en-IN", "en-IE", "en-NZ",
    "de-DE", "de-AT", "de-CH", "fr-FR", "fr-BE", "fr-CA", "fr-CH",
    "es-ES", "es-MX", "it-IT", "it-CH", "nl-NL", "nl-BE", "pt-PT", "pt-BR",
    "da-DK", "sv-SE", "nb-NO", "nn-NO", "fi-FI", "pl-PL", "cs-CZ", "sk-SK",
    "hu-HU", "ro-RO", "hr-HR", "sl-SI", "bg-BG", "el-GR", "ru-RU", "uk-UA",
    "et-EE", "lv-LV", "lt-LT", "tr-TR", "id-ID", "ja-JP", "ko-KR", "zh-CN",
]
CURRENCY = 8
MONTH_DAYS = [45306.0 + 30.4 * m for m in range(12)]
SUNDAY = 45417.0


def connect(profile):
    port = 2084
    office = subprocess.Popen(
        [
            "soffice",
            f"-env:UserInstallation=file://{profile}",
            "--headless",
            "--invisible",
            f"--accept=socket,host=localhost,port={port};urp;",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    local = uno.getComponentContext()
    resolver = local.ServiceManager.createInstanceWithContext(
        "com.sun.star.bridge.UnoUrlResolver", local
    )
    for _ in range(60):
        try:
            return office, resolver.resolve(
                f"uno:socket,host=localhost,port={port};urp;StarOffice.ComponentContext"
            )
        except Exception:
            time.sleep(0.5)
    office.kill()
    sys.exit("LibreOffice did not answer")


def rust(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def array(items):
    return "[" + ", ".join(rust(item) for item in items) + "]"


def describe(tag, formats, formatter):
    language, country = tag.split("-")
    where = Locale(language, country, "")
    keys = {}

    def shown(code, value):
        if code not in keys:
            keys[code] = formats.addNewConverted(code, Locale("en", "US", ""), where)
        return formatter.convertNumberToString(keys[code], value)

    grouped = shown("#,##0.00", 1234567.891)
    decimal = grouped[-3]
    integer = grouped[:-3]
    group = next(c for c in integer if not c.isdigit())
    indian = integer.split(group)[1:2] == ["34"]
    key = formats.getStandardFormat(CURRENCY, where)
    properties = formats.getByKey(key)
    amount = formatter.convertNumberToString(key, 1234.5)
    symbol = properties.CurrencySymbol
    at = amount.find(symbol)
    digits = [i for i, c in enumerate(amount) if c.isdigit()]
    before = at < digits[0]
    between = amount[at + len(symbol) : digits[0]] if before else amount[digits[-1] + 1 : at]
    return "\n".join(
        [
            "    Locale {",
            f"        language: {rust(language)},",
            f"        country: {rust(country)},",
            f"        decimal: {rust(decimal)},",
            f"        group: {rust(group)},",
            f"        indian: {'true' if indian else 'false'},",
            f"        months: {array(shown('MMMM', d) for d in MONTH_DAYS)},",
            f"        months_short: {array(shown('MMM', d) for d in MONTH_DAYS)},",
            f"        days: {array(shown('DDDD', SUNDAY + n) for n in range(7))},",
            f"        days_short: {array(shown('DDD', SUNDAY + n) for n in range(7))},",
            f"        quarters: {array(shown('Q', d) for d in MONTH_DAYS[::3])},",
            f"        quarters_long: {array(shown('QQ', d) for d in MONTH_DAYS[::3])},",
            f"        truth: {array([shown('BOOLEAN', 1.0), shown('BOOLEAN', 0.0)])},",
            "        currency: Currency {",
            f"            symbol: {rust(symbol)},",
            f"            code: {rust(properties.CurrencyAbbreviation)},",
            f"            before: {'true' if before else 'false'},",
            f"            between: {rust(between)},",
            "        },",
            "    },",
        ]
    )


def main():
    with tempfile.TemporaryDirectory() as profile:
        office, context = connect(profile)
        try:
            desktop = context.ServiceManager.createInstanceWithContext(
                "com.sun.star.frame.Desktop", context
            )
            hidden = PropertyValue(Name="Hidden", Value=True)
            book = desktop.loadComponentFromURL("private:factory/scalc", "_blank", 0, (hidden,))
            formatter = context.ServiceManager.createInstanceWithContext(
                "com.sun.star.util.NumberFormatter", context
            )
            formatter.attachNumberFormatsSupplier(book)
            entries = [describe(tag, book.NumberFormats, formatter) for tag in LOCALES]
            book.close(True)
        finally:
            try:
                desktop.terminate()
            except Exception:
                pass
            office.wait(timeout=30)
    print(
        "//! What `LibreOffice` shows in each locale it is asked about: month, day\n"
        "//! and quarter names, TRUE and FALSE, separators, grouping and the\n"
        "//! currency of a new amount. Written by `locale.py`, which asks\n"
        "//! `LibreOffice`; run it again rather than editing this file.\n"
        "\n"
        "// Author: David M. Anderson\n"
        "// Built with AI assistance (Claude, Anthropic)\n"
        "\n"
        "use crate::locale::{Currency, Locale};\n"
        "\n"
        "/// Every locale asked about, the first of each language standing for it.\n"
        "pub(crate) const LOCALES: &[Locale] = &[\n" + "\n".join(entries) + "\n];"
    )


if __name__ == "__main__":
    main()
