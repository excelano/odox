# Store listing text, German

The German half of `store-listing.md`, under that file's headings, which stay in
English so both files read the same way. Only the fields a store shows a reader
are here; the URLs, the review notes and the screenshot log live in the English
file alone. Terminology is the applications' own, out of `crates/odox-ui/po/de.po`
and the `.desktop` entries: *Gliederung*, *Folien*, *Notizen*, *Tabellenblatt*,
and *OpenDocument-Textdokument*, *OpenDocument-Tabellendokument* and
*OpenDocument-Präsentation* for the three kinds of file. Run the count in
`store-listing.md` against both files after any edit; the 30-character subtitle
is where German bites.

---

# Odox Text

## Subtitle (Mac App Store, 30)

Textdokumente lesen

## Promotional text (Mac App Store, 170)

Öffnen Sie ein OpenDocument-Textdokument und lesen Sie es wie geschrieben: Überschriften, Listen, Tabellen und Bilder an ihrem Platz, mit einer Gliederung daneben.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Betrachter für OpenDocument-Textdokumente, also die ODT-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Die Seite wird in der Breite gezeichnet, die das Dokument verlangt, mit Überschriften, Listen, Tabellen und Bildern an ihrem Platz und einer Gliederung daneben, die beim Klick auf eine Überschrift dorthin springt. Markieren und kopieren lässt sich alles.

Er liest und schreibt nicht: Ihre Datei bleibt unverändert.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODT-Textdokumente aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet die Seite in der Breite, die das Dokument verlangt, mit den Schriften, die auf Ihrem Rechner installiert sind
- Überschriften, verschachtelte Listen, Tabellen mit verbundenen Zellen und die Bilder im Dokument, jedes an seinem Platz
- Eine Gliederung neben der Seite, die beim Klick auf eine Überschrift dorthin springt
- Über Absätze hinweg markieren und kopieren, oder das ganze Dokument als reinen Text übernehmen
- Liest und schreibt nie: kein Speichern, keine temporäre Datei neben Ihrem Dokument, nichts wird angelegt
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Text öffnet ein OpenDocument-Textdokument und zeigt Ihnen, was darin steht.

Mehr nicht. Es gibt kein Speichern, kein Speichern unter und keinen Export; das Menü Datei bietet Öffnen, Neu einlesen, Schließen und Beenden. Die Datei, die Sie öffnen, wird zum Lesen geöffnet und bleibt genau so, wie sie war: keine temporäre Datei daneben, keine Sicherungskopie. Es wird nichts auf Ihre Festplatte geschrieben, auch nicht von Odox Text selbst: keine Einstellungsdatei, keine Liste zuletzt geöffneter Dokumente, kein Zwischenspeicher.

Die Seite wird in der Breite gezeichnet, die das Dokument verlangt. Überschriften, verschachtelte Listen, Tabellen mit verbundenen Zellen und Rahmen sowie die Bilder im Dokument erscheinen dort, wo das Dokument sie hinsetzt, in den Schriften, die es verlangt, aufgelöst gegen die auf Ihrem Rechner installierten. Neben der Seite steht eine Gliederung, die beim Klick auf eine Überschrift dorthin springt. Sie können über Absätze hinweg markieren und kopieren oder das ganze Dokument mit einem Befehl als reinen Text übernehmen.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Text ist eines von dreien. Odox Grid liest Tabellendokumente und Odox Deck liest Präsentationen. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Betrachter nicht tun müsste und dieser tut: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und bei jeder Änderung wird geprüft, dass eine wieder hinausgeschriebene Datei dasselbe ergibt. Einem Betrachter kostet das nichts. Es ist da, weil ein Betrachter, der wegwirft, was er nicht versteht, im Fenster genauso aussieht und zu einem Editor wird, der Dokumente still zerstört, sobald jemand zum ersten Mal speichert. Dorthin geht die Reise.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odt,textdokument,betrachter,lesen,dokument,oasis,büro

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODT, Textdokument, Betrachter, Dokument lesen, OASIS

---

# Odox Grid

## Subtitle (Mac App Store, 30)

Tabellendokumente lesen

## Promotional text (Mac App Store, 170)

Öffnen Sie ein OpenDocument-Tabellendokument mit seinen eigenen Spaltenbreiten und Zellformaten, ein Reiter je Tabellenblatt, mit der Formel hinter der gewählten Zelle.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Betrachter für OpenDocument-Tabellendokumente, also die ODS-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Das Blatt wird mit den Spaltenbreiten und Zellformaten des Dokuments gezeichnet, ein Reiter je Tabellenblatt, mit der Formel hinter der gewählten Zelle. Ein Blatt mit zehntausend leeren Zeilen dazwischen öffnet so schnell wie eines ohne.

Er liest und schreibt nicht: Ihre Datei bleibt unverändert.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODS-Tabellendokumente aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet das Blatt mit den Spaltenbreiten, Zeilenhöhen und Zellformaten des Dokuments
- Ein Reiter je Tabellenblatt, und über dem Gitter die Formel hinter der gewählten Zelle
- Zahlen, Datumsangaben und Wahrheitswerte so dargestellt, wie das Dokument es vorgibt
- Ein Blatt mit einer großen leeren Lücke öffnet so schnell wie ein volles, weil die Lücke nie ausgerollt wird
- Das ganze Tabellenblatt als tabulatorgetrennten Text kopieren und überall einfügen
- Liest und schreibt nie: kein Speichern, keine temporäre Datei daneben, nichts wird angelegt
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Grid öffnet ein OpenDocument-Tabellendokument und zeigt Ihnen, was darin steht.

Mehr nicht. Es gibt kein Speichern, kein Speichern unter und keinen Export; das Menü Datei bietet Öffnen, Neu einlesen, Schließen und Beenden. Die Datei, die Sie öffnen, wird zum Lesen geöffnet und bleibt genau so, wie sie war: keine temporäre Datei daneben, keine Sicherungskopie. Es wird nichts auf Ihre Festplatte geschrieben, auch nicht von Odox Grid selbst: keine Einstellungsdatei, keine Liste zuletzt geöffneter Dokumente, kein Zwischenspeicher.

Das Blatt wird mit den Spaltenbreiten und Zeilenhöhen des Dokuments und den Zellformaten gezeichnet, die es mitbringt, und Zahlen, Datumsangaben und Wahrheitswerte erscheinen so, wie das Dokument es vorgibt. Es gibt einen Reiter je Tabellenblatt, und wer eine Zelle wählt, sieht die Formel dahinter über dem Gitter. Mit einem Befehl übernehmen Sie das ganze Blatt als tabulatorgetrennten Text und fügen es ein, wo Sie es brauchen.

Ein Tabellendokument hat oft eine Lücke: einen Lauf von zehntausend leeren Zeilen zwischen zwei Zahlenblöcken. Ein Betrachter, der daraus zehntausend Zeilen im Speicher macht, öffnet langsam und rollt langsam. Odox Grid behält die Lücke so, wie das Dokument sie schreibt, und deshalb öffnet ein dünn besetztes Blatt so schnell wie ein dichtes.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Grid ist eines von dreien. Odox Text liest Textdokumente und Odox Deck liest Präsentationen. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Betrachter nicht tun müsste und dieser tut: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und bei jeder Änderung wird geprüft, dass eine wieder hinausgeschriebene Datei dasselbe ergibt. Einem Betrachter kostet das nichts. Es ist da, weil ein Betrachter, der wegwirft, was er nicht versteht, im Fenster genauso aussieht und zu einem Editor wird, der Dokumente still zerstört, sobald jemand zum ersten Mal speichert. Dorthin geht die Reise.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,ods,tabelle,tabellendokument,betrachter,lesen,oasis

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODS, Tabellendokument, Tabelle, Betrachter, OASIS

---

# Odox Deck

## Subtitle (Mac App Store, 30)

Präsentationen lesen

## Promotional text (Mac App Store, 170)

Öffnen Sie eine OpenDocument-Präsentation und sehen Sie jede Folie wie entworfen: die Masterfolie dahinter, die Formen aus ihrer eigenen Geometrie, die Notizen darunter.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Betrachter für OpenDocument-Präsentationen, also die ODP-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Jede Folie wird in der Größe gezeichnet, die das Dokument vorgibt, mit Hintergrund und Verzierungen der Masterfolie dahinter und den Formen aus ihrer eigenen Geometrie statt angenähert. Die Notizen stehen darunter, die Folienliste daneben.

Er liest und schreibt nicht: Ihre Datei bleibt unverändert.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODP-Präsentationen aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet jede Folie in der Größe, die das Dokument vorgibt, auf dem Hintergrund der Masterfolie
- Die Verzierungen, die eine Vorlage hinter jede Folie legt, werden gezeichnet statt übergangen
- Formen aus ihrer eigenen Geometrie: Pfade, benutzerdefinierte Formen, Verbinder und ihre Beschriftungen
- Einfarbige, lineare und axiale Verlaufsfüllungen sowie die Bilder, die eine Folie rahmt
- Die Notizen unter jeder Folie und eine Liste der Folien daneben
- Liest und schreibt nie: kein Speichern, keine temporäre Datei daneben, nichts wird angelegt
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Deck öffnet eine OpenDocument-Präsentation und zeigt Ihnen, was darin steht.

Mehr nicht. Es gibt kein Speichern, kein Speichern unter und keinen Export; das Menü Datei bietet Öffnen, Neu einlesen, Schließen und Beenden. Die Datei, die Sie öffnen, wird zum Lesen geöffnet und bleibt genau so, wie sie war: keine temporäre Datei daneben, keine Sicherungskopie. Es wird nichts auf Ihre Festplatte geschrieben, auch nicht von Odox Deck selbst: keine Einstellungsdatei, keine Liste zuletzt geöffneter Dokumente, kein Zwischenspeicher.

Jede Folie wird in der Größe gezeichnet, die das Dokument vorgibt. Hinter dem Text und den Bildern der Folie liegt die Masterfolie: der Grund, den sie füllt, und die Verzierungen, die eine Vorlage auf jede Folie legt, und das ist das meiste von dem, was einen Foliensatz nach seiner Vorlage aussehen lässt. Die Formen werden aus der Geometrie gezeichnet, die das Dokument angibt, statt zu Rechtecken angenähert zu werden: Pfade, benutzerdefinierte Formen mit ihren eigenen Formeln, Verbinder zwischen den Formen, die sie verknüpfen, und die Beschriftung, die eine Form trägt. Die Notizen stehen unter der Folie, eine Liste der Folien daneben.

Wie gut gezeichnet wird, ist gemessen und nicht behauptet. Jede Folie der Vorlagen im Prüfbestand wird von Odox Deck und von LibreOffice aus derselben Datei gezeichnet, und beide werden Farbe für Farbe verglichen, denn ein Zeichner ist genau die Art von Sache, die jeden Test besteht und trotzdem falsch aussieht.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Deck ist eines von dreien. Odox Text liest Textdokumente und Odox Grid liest Tabellendokumente. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Betrachter nicht tun müsste und dieser tut: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und bei jeder Änderung wird geprüft, dass eine wieder hinausgeschriebene Datei dasselbe ergibt. Einem Betrachter kostet das nichts. Es ist da, weil ein Betrachter, der wegwirft, was er nicht versteht, im Fenster genauso aussieht und zu einem Editor wird, der Dokumente still zerstört, sobald jemand zum ersten Mal speichert. Dorthin geht die Reise.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odp,präsentation,folien,betrachter,lesen,oasis

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODP, Präsentation, Folien, Betrachter, OASIS
