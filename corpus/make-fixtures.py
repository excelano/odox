#!/usr/bin/env python3
"""Write the spreadsheet and presentation fixtures.

The text fixtures beside these were produced by a real producer and this pair
was not, because the machine they were written on has LibreOffice Writer and
neither Calc nor Impress. They are hand-written to exercise the parts of the
format a corpus is for — a repeated row, a merged cell, a formula with its
cached value, a slide with a text frame — and they are a stand-in. Replace them
with documents Calc and Impress wrote as soon as there is a machine that has
them, and delete this script when that happens.

Run:  python3 corpus/make-fixtures.py
"""

import pathlib
import zipfile

HERE = pathlib.Path(__file__).parent

SPREADSHEET = "application/vnd.oasis.opendocument.spreadsheet"
PRESENTATION = "application/vnd.oasis.opendocument.presentation"

NS = """xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
 xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
 xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
 xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
 xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
 xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
 xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0"
 xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0"
 xmlns:dc="http://purl.org/dc/elements/1.1/"
 office:version="1.3\""""

MANIFEST = """<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:media-type="{media}"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>
"""


def meta(title):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta {NS}>
 <office:meta>
  <dc:title>{title}</dc:title>
  <meta:generator>odox corpus fixture</meta:generator>
  <meta:creation-date>2026-09-11T12:00:00</meta:creation-date>
 </office:meta>
</office:document-meta>
"""


SHEET_STYLES = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}>
 <office:font-face-decls>
  <style:font-face style:name="Liberation Sans" svg:font-family="'Liberation Sans'"/>
 </office:font-face-decls>
 <office:styles>
  <style:default-style style:family="table-cell">
   <style:text-properties style:font-name="Liberation Sans" fo:font-size="10pt"/>
  </style:default-style>
  <style:style style:name="Default" style:family="table-cell"/>
 </office:styles>
</office:document-styles>
"""


def cell(value_type=None, value=None, text="", extra="", attrs=""):
    parts = []
    if value_type:
        parts.append(f'office:value-type="{value_type}"')
    if value is not None:
        parts.append(value)
    if attrs:
        parts.append(attrs)
    body = f"<text:p>{text}</text:p>" if text else ""
    if body:
        return f"<table:table-cell {' '.join(parts)}{extra}>{body}</table:table-cell>"
    return f"<table:table-cell {' '.join(parts)}{extra}/>"


def spreadsheet():
    rows = []
    rows.append(
        "<table:table-row table:style-name=\"ro1\">"
        + "".join(
            cell("string", None, name, attrs='table:style-name="ceHead"')
            for name in ("Item", "Quantity", "Unit price", "Total")
        )
        + "</table:table-row>"
    )
    data = [
        ("Bolt", 12, 0.45),
        ("Nut", 300, 0.07),
        ("Washer", 88, 1.25),
    ]
    for name, quantity, price in data:
        total = round(quantity * price, 2)
        rows.append(
            "<table:table-row>"
            + cell("string", None, name)
            + cell("float", f'office:value="{quantity}"', str(quantity))
            + cell(
                "currency",
                f'office:value="{price}" office:currency="EUR"',
                f"{price:.2f} €",
            )
            + cell(
                "currency",
                f'office:value="{total}" office:currency="EUR"',
                f"{total:.2f} €",
                attrs=f'table:formula="of:=[.B{len(rows)+1}]*[.C{len(rows)+1}]"',
            )
            + "</table:table-row>"
        )
    grand = round(sum(q * p for _, q, p in data), 2)
    rows.append(
        "<table:table-row>"
        + cell(
            "string",
            None,
            "Total, over two columns",
            attrs='table:style-name="ceHead" table:number-columns-spanned="3" table:number-rows-spanned="1"',
        )
        + "<table:covered-table-cell/><table:covered-table-cell/>"
        + cell(
            "currency",
            f'office:value="{grand}" office:currency="EUR"',
            f"{grand:.2f} €",
            attrs='table:formula="of:=SUM([.D2:.D4])" table:style-name="ceHead"',
        )
        + "</table:table-row>"
    )
    # A run of identical rows written once, which is what an index has to answer
    # for without expanding.
    rows.append(
        '<table:table-row table:number-rows-repeated="500">'
        + cell("string", None, "repeated")
        + "</table:table-row>"
    )

    second = (
        '<table:table table:name="Notes">'
        '<table:table-column table:style-name="co2"/>'
        "<table:table-row>"
        + cell("string", None, "A second sheet, to give the tabs something to do.")
        + "</table:table-row>"
        "<table:table-row>"
        + cell("percentage", 'office:value="0.15"', "15%")
        + "</table:table-row>"
        "<table:table-row>"
        + cell("date", 'office:date-value="2026-09-11"', "11/09/2026")
        + "</table:table-row>"
        "<table:table-row>"
        + cell("boolean", 'office:boolean-value="true"', "TRUE")
        + "</table:table-row>"
        "</table:table>"
    )

    return f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}>
 <office:automatic-styles>
  <style:style style:name="co1" style:family="table-column">
   <style:table-column-properties style:column-width="3.2cm"/>
  </style:style>
  <style:style style:name="co2" style:family="table-column">
   <style:table-column-properties style:column-width="7cm"/>
  </style:style>
  <style:style style:name="ro1" style:family="table-row">
   <style:table-row-properties style:row-height="0.6cm"/>
  </style:style>
  <style:style style:name="ceHead" style:family="table-cell" style:parent-style-name="Default">
   <style:table-cell-properties fo:background-color="#dbe5f1" fo:border="0.5pt solid #7f9db9"/>
   <style:text-properties fo:font-weight="bold"/>
  </style:style>
 </office:automatic-styles>
 <office:body><office:spreadsheet>
  <table:table table:name="Parts">
   <table:table-column table:style-name="co1" table:number-columns-repeated="4"/>
   {''.join(rows)}
  </table:table>
  {second}
 </office:spreadsheet></office:body>
</office:document-content>
"""


PRESENTATION_STYLES = f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles {NS}>
 <office:automatic-styles>
  <style:page-layout style:name="pm1">
   <style:page-layout-properties fo:page-width="28cm" fo:page-height="15.75cm"
    fo:margin-top="0cm" fo:margin-bottom="0cm" fo:margin-left="0cm" fo:margin-right="0cm"/>
  </style:page-layout>
 </office:automatic-styles>
 <office:master-styles>
  <style:master-page style:name="Default" style:page-layout-name="pm1"/>
 </office:master-styles>
</office:document-styles>
"""


def presentation():
    def slide(name, title, lines, notes):
        bullets = "".join(f"<text:p>{line}</text:p>" for line in lines)
        return f"""<draw:page draw:name="{name}" draw:master-page-name="Default">
   <draw:frame presentation:class="title" svg:width="24cm" svg:height="3cm" svg:x="2cm" svg:y="1.5cm">
    <draw:text-box><text:p>{title}</text:p></draw:text-box>
   </draw:frame>
   <draw:frame presentation:class="outline" svg:width="24cm" svg:height="8cm" svg:x="2cm" svg:y="5cm">
    <draw:text-box>{bullets}</draw:text-box>
   </draw:frame>
   <presentation:notes>
    <draw:frame svg:width="16cm" svg:height="8cm" svg:x="2cm" svg:y="12cm">
     <draw:text-box><text:p>{notes}</text:p></draw:text-box>
    </draw:frame>
   </presentation:notes>
  </draw:page>"""

    pages = "".join(
        [
            slide(
                "Opening",
                "A presentation fixture",
                ["A first point", "A second point", "A third point"],
                "What the speaker says about the opening.",
            ),
            slide(
                "Second",
                "The second slide",
                ["Text frames carry paragraphs", "Shapes carry their own geometry"],
                "And about the second.",
            ),
        ]
    )
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content {NS}>
 <office:body><office:presentation>
  {pages}
 </office:presentation></office:body>
</office:document-content>
"""


def package(path, media_type, content, styles, title):
    with zipfile.ZipFile(path, "w") as out:
        # The one ordering rule the format has: first, and stored rather than
        # deflated, so that the media type is readable from the first bytes.
        out.writestr(
            zipfile.ZipInfo("mimetype"), media_type, compress_type=zipfile.ZIP_STORED
        )
        for name, text in (
            ("META-INF/manifest.xml", MANIFEST.format(media=media_type)),
            ("content.xml", content),
            ("styles.xml", styles),
            ("meta.xml", meta(title)),
        ):
            out.writestr(name, text, compress_type=zipfile.ZIP_DEFLATED)
    print(f"wrote {path}")


package(
    HERE / "sample.ods",
    SPREADSHEET,
    spreadsheet(),
    SHEET_STYLES,
    "odox spreadsheet fixture",
)
package(
    HERE / "sample.odp",
    PRESENTATION,
    presentation(),
    PRESENTATION_STYLES,
    "odox presentation fixture",
)
