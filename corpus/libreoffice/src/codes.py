"""Have LibreOffice write codes.ods: one value per row in a number format given
by its format code and language, with the text Calc shows for it, beside the
code that made it. Run by build.sh as `python3 src/codes.py <out.ods>`.
"""

# Author: David M. Anderson
# Built with AI assistance (Claude, Anthropic)

import os
import subprocess
import sys
import tempfile
import time

import uno
from com.sun.star.beans import PropertyValue
from com.sun.star.lang import Locale

DATE = 45413.0
TIME = 0.5000057870370
CASES = [
    ("en-US", "# ?/16", 0.5),
    ("en-US", "# ?/16", 0.3),
    ("en-US", "# ?/16", 1.5),
    ("en-US", "# ?/16", 0),
    ("en-US", "# ?/16", 2),
    ("en-US", "# ?/?", 0.5),
    ("en-US", "# ?/?", 1.25),
    ("en-US", "# ?/?", 2),
    ("en-US", "# ??/??", 0.3),
    ("en-US", "# ??/??", 0),
    ("en-US", "# ??/??", 0.5),
    ("en-US", "000-00-0000", 123456789),
    ("en-US", "000-00-0000", 12345),
    ("en-US", "##0.00E+00", 12345),
    ("en-US", "##0.00E+00", 0.00012345),
    ("en-US", "##0.00E+00", 1),
    ("en-US", "##0.00E+00", 999999),
    ("en-US", "##0.00E+00", -4567),
    ("en-US", "Q YYYY", DATE),
    ("en-US", "QQ YYYY", DATE),
    ("en-US", "WW", DATE),
    ("en-US", "HH:MM:SS.00", TIME),
    ("en-US", "[MM]:SS", 0.1),
    ("en-US", "[SS]", 0.01),
    ("en-US", "[MM]:SS.0", 0.0123456),
    ("en-US", "MM:SS.000", 0.0123456),
    ("en-US", "[HH]:MM:SS", 0.999999),
    ("en-US", "[HH]:MM", 1.75),
    ("en-US", '0.00;[RED]-0.00;"zero";"text: "@', 5),
    ("en-US", '0.00;[RED]-0.00;"zero";"text: "@', -5),
    ("en-US", '0.00;[RED]-0.00;"zero";"text: "@', 0),
    ("en-US", '0.00;[RED]-0.00;"zero";"text: "@', "abc"),
    ("en-US", "General", 1e15),
    ("en-US", "General", 1e16),
    ("en-US", "General", 1.2345678901234567e19),
    ("en-US", "General", -1e20),
    ("en-US", "General", 1e100),
    ("en-US", "General", 0.000001),
    ("en-US", "General", 1.5e-7),
    ("en-US", "General", 1e-20),
    ("en-US", "General", 0.123456789012345),
    ("en-US", "General", 9.99e15),
    ("en-US", "General", 1.5e16),
    ("en-US", "General", 1e17),
    ("en-US", "General", 1e-10),
    ("en-US", "General", 1e-15),
    ("en-US", "General", 1e-16),
    ("en-US", "General", 1.5e-17),
    ("en-US", "General", 1e-17),
    ("en-US", "General", 1e-100),
    ("en-US", "General", 1.0 / 3.0),
    ("en-US", "General", 2.0 / 3.0),
    ("en-US", "General", 123456.7891234567),
    ("de-DE", "DDD, D. MMMM YYYY", DATE),
    ("de-DE", "DDDD, D. MMM YYYY", DATE),
    ("fr-FR", "DDDD D MMMM YYYY", DATE),
    ("fr-FR", "DDD D MMM YYYY", DATE),
    ("sv-SE", "DDDD D MMMM", DATE),
    ("pl-PL", "D MMM YYYY", DATE),
    ("hu-HU", "DDD", DATE),
    ("de-DE", "QQ YYYY", DATE),
    ("de-DE", "Q", DATE),
    ("de-DE", "BOOLEAN", 1),
    ("fr-FR", "BOOLEAN", 0),
    ("de-CH", "#,##0.00", 1234567.5),
    ("en-IN", "#,##0.00", 1234567.5),
    ("fr-FR", "#,##0.00", 1234567.5),
    ("sv-SE", "#,##0.00", -1234567.5),
    ("de-DE", "0.00", 3.25),
]


def connect(profile):
    port = 2083
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
            context = resolver.resolve(
                f"uno:socket,host=localhost,port={port};urp;StarOffice.ComponentContext"
            )
            return office, context
        except Exception:
            time.sleep(0.5)
    office.kill()
    sys.exit("LibreOffice did not answer")


def locale(tag):
    language, country = tag.split("-")
    return Locale(language, country, "")


def main(target):
    with tempfile.TemporaryDirectory() as profile:
        office, context = connect(profile)
        try:
            desktop = context.ServiceManager.createInstanceWithContext(
                "com.sun.star.frame.Desktop", context
            )
            hidden = PropertyValue(Name="Hidden", Value=True)
            book = desktop.loadComponentFromURL("private:factory/scalc", "_blank", 0, (hidden,))
            sheet = book.Sheets.getByIndex(0)
            formats = book.NumberFormats
            for row, (tag, code, value) in enumerate(CASES):
                key = formats.addNewConverted(code, locale("en-US"), locale(tag))
                sheet.getCellByPosition(0, row).setString(f"{tag} {code}")
                cell = sheet.getCellByPosition(1, row)
                cell.NumberFormat = key
                if isinstance(value, str):
                    cell.setString(value)
                else:
                    cell.setValue(float(value))
            url = uno.systemPathToFileUrl(os.path.abspath(target))
            book.storeToURL(url, (PropertyValue(Name="FilterName", Value="calc8"),))
            book.close(True)
        finally:
            try:
                desktop.terminate()
            except Exception:
                pass
            office.wait(timeout=30)


if __name__ == "__main__":
    main(sys.argv[1])
