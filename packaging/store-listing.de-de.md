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

Textdokumente bearbeiten

## Promotional text (Mac App Store, 170)

Öffnen Sie ein OpenDocument-Textdokument, lesen Sie es wie geschrieben, ändern Sie, was zu ändern ist, und speichern Sie es als das Dokument, das es war.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Editor für OpenDocument-Textdokumente, also die ODT-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Die Seite wird in der Breite gezeichnet, die das Dokument verlangt, mit Überschriften, Listen, Tabellen und Bildern an ihrem Platz und einer Gliederung daneben. Klicken Sie auf einen Absatz, um ihn zu ändern, nehmen Sie es zurück und speichern Sie: Was Sie nicht angefasst haben, bleibt, wie es gelesen wurde.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODT-Textdokumente aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet die Seite in der Breite, die das Dokument verlangt, mit den Schriften, die auf Ihrem Rechner installiert sind
- Überschriften, verschachtelte Listen, Tabellen mit verbundenen Zellen und die Bilder im Dokument, jedes an seinem Platz
- Eine Gliederung neben der Seite, die beim Klick auf eine Überschrift dorthin springt
- Über Absätze hinweg markieren und kopieren, oder das ganze Dokument als reinen Text übernehmen
- Auf einen Absatz klicken und ihn ändern, mit Enter teilen oder mit der Rücktaste zusammenfügen, und alles mit Rückgängig zurücknehmen
- Speichert über die geöffnete Datei oder anderswohin und verweigert ein Speichern, das nicht als das Dokument im Fenster zurückgelesen würde
- Was Sie nicht angefasst haben, wird so zurückgeschrieben, wie es gelesen wurde, Element für Element, auch das, wovon Odox nie gehört hat
- Schreibt sonst nichts: keine temporäre Datei neben Ihrem Dokument, keine Sicherungskopie, kein Zwischenspeicher; eine Einstellung in einer Einstellungsdatei, nur wenn Sie sie ändern
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Text öffnet ein OpenDocument-Textdokument, lässt Sie ändern, was darin steht, und speichert es als das Dokument zurück, das es war.

Es ist ein leichter Editor und keine Textverarbeitung. Ein Dokument öffnet sich zum Lesen; der Bearbeitungsmodus, im Menü Bearbeiten oder mit Strg+E, lässt Sie auf einen Absatz klicken und seinen Text an Ort und Stelle ändern, mit Enter einen neuen Absatz beginnen, mit der Rücktaste zwei zusammenfügen, alles mit Rückgängig zurücknehmen und speichern. Formatieren, eine Tabelle oder ein Bild einfügen, suchen und ersetzen: dafür haben Sie eine Bürosuite. Es wird nichts geschrieben, bis Sie speichern, und dann nur die Datei, die Sie geöffnet haben, oder die, die Sie benannt haben; keine temporäre Datei bleibt daneben, keine Sicherungskopie. Die eine Einstellung, die die Anwendung sich merkt, ob ein Dokument zum Bearbeiten geöffnet wird, landet nur dann in einer Einstellungsdatei, wenn Sie sie ändern. Es gibt keine Liste zuletzt geöffneter Dokumente und keinen Zwischenspeicher.

Die Seite wird in der Breite gezeichnet, die das Dokument verlangt. Überschriften, verschachtelte Listen, Tabellen mit verbundenen Zellen und Rahmen sowie die Bilder im Dokument erscheinen dort, wo das Dokument sie hinsetzt, in den Schriften, die es verlangt, aufgelöst gegen die auf Ihrem Rechner installierten. Neben der Seite steht eine Gliederung, die beim Klick auf eine Überschrift dorthin springt. Sie können über Absätze hinweg markieren und kopieren oder das ganze Dokument mit einem Befehl als reinen Text übernehmen.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Text ist eines von dreien. Odox Grid bearbeitet Tabellendokumente und Odox Deck Präsentationen. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Editor tun muss und die meisten nicht tun: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und was Sie nicht angefasst haben, wird so zurückgeschrieben, wie es gelesen wurde, Element für Element. Dass jedes Dokument im Prüfbestand unverändert zurückkommt und dass eine Änderung nur das ändert, was geändert wurde, wird bei jeder Änderung am Programm geprüft. Vor dem Speichern wird das, was geschrieben werden soll, zurückgelesen und mit dem Dokument im Fenster verglichen, und ein Unterschied verweigert das Speichern, statt ein Dokument zu schreiben, das nicht so zurückkäme.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odt,textdokument,editor,bearbeiten,dokument,oasis,büro

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODT, Textdokument, Editor, Dokument bearbeiten, OASIS

---

# Odox Grid

## Subtitle (Mac App Store, 30)

Tabellendokumente bearbeiten

## Promotional text (Mac App Store, 170)

Öffnen Sie ein OpenDocument-Tabellendokument mit seinen eigenen Spaltenbreiten und Zellformaten, ändern Sie Zellen und speichern Sie es als das Dokument, das es war.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Editor für OpenDocument-Tabellendokumente, also die ODS-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Das Blatt wird mit den Spaltenbreiten und Zellformaten des Dokuments gezeichnet, ein Reiter je Tabellenblatt, mit der Formel hinter der gewählten Zelle. Tippen Sie in eine Zelle, um sie zu ändern, nehmen Sie es zurück und speichern Sie: Formeln und alles Unberührte bleiben, wie sie gelesen wurden.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODS-Tabellendokumente aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet das Blatt mit den Spaltenbreiten, Zeilenhöhen und Zellformaten des Dokuments
- Ein Reiter je Tabellenblatt, und über dem Gitter die Formel hinter der gewählten Zelle
- Zahlen, Datumsangaben und Wahrheitswerte so dargestellt, wie das Dokument es vorgibt
- Ein Blatt mit einer großen leeren Lücke öffnet so schnell wie ein volles, weil die Lücke nie ausgerollt wird
- Das ganze Tabellenblatt als tabulatorgetrennten Text kopieren und überall einfügen
- In eine Zelle tippen, um sie zu ändern: Eine Zahl ist eine Zahl, WAHR und FALSCH sind Wahrheitswerte, alles andere ist Text, und Rückgängig nimmt es zurück
- Formeln bleiben erhalten und werden nicht bearbeitet; sobald sich etwas geändert hat, werden ihre Ergebnisse blass gezeichnet, bis eine Tabellenkalkulation sie neu berechnet
- Speichert über die geöffnete Datei oder anderswohin und verweigert ein Speichern, das nicht als das Dokument im Fenster zurückgelesen würde
- Schreibt sonst nichts: keine temporäre Datei neben Ihrem Dokument, keine Sicherungskopie, kein Zwischenspeicher; eine Einstellung in einer Einstellungsdatei, nur wenn Sie sie ändern
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Grid öffnet ein OpenDocument-Tabellendokument, lässt Sie die Zellen ändern, die es brauchen, und speichert es als das Dokument zurück, das es war.

Es ist ein leichter Editor und keine Tabellenkalkulation. Tippen auf der gewählten Zelle ersetzt sie, Enter oder F2 öffnet sie mit ihrem Inhalt, Entf leert sie, und Rückgängig nimmt alles zurück. Eine Zahl ist eine Zahl, WAHR und FALSCH sind Wahrheitswerte, alles andere ist Text. Eine Zelle mit einer Formel bleibt, wie sie ist, und wird nicht bearbeitet; sobald sich im Blatt etwas geändert hat, wird das Ergebnis jeder Formel blass gezeichnet, bis eine Tabellenkalkulation es neu berechnet, was LibreOffice beim Öffnen der Datei tut. Es wird nichts geschrieben, bis Sie speichern, und dann nur die Datei, die Sie geöffnet haben, oder die, die Sie benannt haben; keine temporäre Datei bleibt daneben, keine Sicherungskopie. Die eine Einstellung, die die Anwendung sich merkt, ob ein Dokument zum Bearbeiten geöffnet wird, landet nur dann in einer Einstellungsdatei, wenn Sie sie ändern. Es gibt keine Liste zuletzt geöffneter Dokumente und keinen Zwischenspeicher.

Das Blatt wird mit den Spaltenbreiten und Zeilenhöhen des Dokuments und den Zellformaten gezeichnet, die es mitbringt, und Zahlen, Datumsangaben und Wahrheitswerte erscheinen so, wie das Dokument es vorgibt. Es gibt einen Reiter je Tabellenblatt, und wer eine Zelle wählt, sieht die Formel dahinter über dem Gitter. Mit einem Befehl übernehmen Sie das ganze Blatt als tabulatorgetrennten Text und fügen es ein, wo Sie es brauchen.

Ein Tabellendokument hat oft eine Lücke: einen Lauf von zehntausend leeren Zeilen zwischen zwei Zahlenblöcken. Ein Programm, das daraus zehntausend Zeilen im Speicher macht, öffnet langsam und rollt langsam. Odox Grid behält die Lücke so, wie das Dokument sie schreibt, und deshalb öffnet ein dünn besetztes Blatt so schnell wie ein dichtes.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Grid ist eines von dreien. Odox Text bearbeitet Textdokumente und Odox Deck Präsentationen. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Editor tun muss und die meisten nicht tun: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und was Sie nicht angefasst haben, wird so zurückgeschrieben, wie es gelesen wurde, Element für Element. Dass jedes Dokument im Prüfbestand unverändert zurückkommt und dass eine Änderung nur das ändert, was geändert wurde, wird bei jeder Änderung am Programm geprüft. Vor dem Speichern wird das, was geschrieben werden soll, zurückgelesen und mit dem Dokument im Fenster verglichen, und ein Unterschied verweigert das Speichern, statt ein Dokument zu schreiben, das nicht so zurückkäme.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,ods,tabelle,tabellendokument,editor,bearbeiten,oasis

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODS, Tabellendokument, Tabelle, Editor, OASIS

---

# Odox Deck

## Subtitle (Mac App Store, 30)

Präsentationen bearbeiten

## Promotional text (Mac App Store, 170)

Öffnen Sie eine OpenDocument-Präsentation, sehen Sie jede Folie wie entworfen, verschieben Sie eine Form oder ändern Sie ihren Text, und speichern Sie sie, wie sie war.

## Short description (Microsoft Store, 500)

Ein kleiner, schneller Editor für OpenDocument-Präsentationen, also die ODP-Dateien, die LibreOffice, OpenOffice und alles andere schreiben, was dem OASIS-Standard folgt.

Jede Folie wird in der Größe gezeichnet, die das Dokument vorgibt, mit der Masterfolie dahinter und den Formen aus ihrer eigenen Geometrie. Wählen Sie eine Form und ziehen Sie sie, ändern Sie ihre Größe oder ihren Text; Rückgängig nimmt es zurück, und alles andere bleibt, wie es gelesen wurde.

## App features (Microsoft Store, up to 20 bullets of 200 characters)

- Öffnet ODP-Präsentationen aus jeder Anwendung, die dem OpenDocument-Standard folgt
- Zeichnet jede Folie in der Größe, die das Dokument vorgibt, auf dem Hintergrund der Masterfolie
- Die Verzierungen, die eine Vorlage hinter jede Folie legt, werden gezeichnet statt übergangen
- Formen aus ihrer eigenen Geometrie: Pfade, benutzerdefinierte Formen, Verbinder und ihre Beschriftungen
- Einfarbige, lineare und axiale Verlaufsfüllungen sowie die Bilder, die eine Folie rahmt
- Die Notizen unter jeder Folie und eine Liste der Folien daneben
- Eine Form wählen und ziehen, an einer Ecke in der Größe ändern oder auf ihren Text klicken und ihn an Ort und Stelle ändern, und alles mit Rückgängig zurücknehmen
- Speichert über die geöffnete Datei oder anderswohin und verweigert ein Speichern, das nicht als das Dokument im Fenster zurückgelesen würde
- Was Sie nicht angefasst haben, wird so zurückgeschrieben, wie es gelesen wurde, Element für Element, Masterfolie und Verzierungen der Vorlage eingeschlossen
- Schreibt sonst nichts: keine temporäre Datei neben Ihrem Dokument, keine Sicherungskopie, kein Zwischenspeicher; eine Einstellung in einer Einstellungsdatei, nur wenn Sie sie ändern
- Keinerlei Netzwerkverbindung, kein Konto, keine Telemetrie
- Folgt der hellen oder dunklen Einstellung Ihrer Arbeitsumgebung
- Spricht Deutsch und Englisch
- Frei und quelloffen unter der MIT-Lizenz

## Description (both, written to 4,000)

Odox Deck öffnet eine OpenDocument-Präsentation, lässt Sie ändern, was auf den Folien steht, und speichert sie als den Foliensatz zurück, der sie war.

Es ist ein leichter Editor und kein Präsentationsprogramm. Ein Foliensatz öffnet sich zum Lesen; der Bearbeitungsmodus, im Menü Bearbeiten oder mit Strg+E, lässt Sie eine Form wählen und dorthin ziehen, wo sie hingehört, ihre Größe an einer Ecke ändern, auf ihren Text klicken und ihn an Ort und Stelle ändern, alles mit Rückgängig zurücknehmen und speichern. Eine Folie oder eine Form hinzufügen, formatieren oder eine Masterfolie ändern: dafür haben Sie eine Bürosuite. Es wird nichts geschrieben, bis Sie speichern, und dann nur die Datei, die Sie geöffnet haben, oder die, die Sie benannt haben; keine temporäre Datei bleibt daneben, keine Sicherungskopie. Die eine Einstellung, die die Anwendung sich merkt, ob ein Dokument zum Bearbeiten geöffnet wird, landet nur dann in einer Einstellungsdatei, wenn Sie sie ändern. Es gibt keine Liste zuletzt geöffneter Dokumente und keinen Zwischenspeicher.

Jede Folie wird in der Größe gezeichnet, die das Dokument vorgibt. Hinter dem Text und den Bildern der Folie liegt die Masterfolie: der Grund, den sie füllt, und die Verzierungen, die eine Vorlage auf jede Folie legt, und das ist das meiste von dem, was einen Foliensatz nach seiner Vorlage aussehen lässt. Die Formen werden aus der Geometrie gezeichnet, die das Dokument angibt, statt zu Rechtecken angenähert zu werden: Pfade, benutzerdefinierte Formen mit ihren eigenen Formeln, Verbinder zwischen den Formen, die sie verknüpfen, und die Beschriftung, die eine Form trägt. Die Notizen stehen unter der Folie, eine Liste der Folien daneben.

Wie gut gezeichnet wird, ist gemessen und nicht behauptet. Jede Folie der Vorlagen im Prüfbestand wird von Odox Deck und von LibreOffice aus derselben Datei gezeichnet, und beide werden Farbe für Farbe verglichen, denn ein Zeichner ist genau die Art von Sache, die jeden Test besteht und trotzdem falsch aussieht.

Es besteht keinerlei Netzwerkverbindung. Kein Server dahinter, kein Konto, keine Analyse, keine Telemetrie, keine Absturzberichte, kein Dienst Dritter. Die Datenschutzangabe im Store lautet daher „Keine Daten erfasst“, weil der Entwickler nichts erfasst und nichts hätte, wohin er es legen könnte. Die vollständige Erklärung steht unter https://excelano.com/legal/#odox, und weil der Quelltext offen ist, lässt sich jede Zeile davon nachprüfen.

Odox Deck ist eines von dreien. Odox Text bearbeitet Textdokumente und Odox Grid Tabellendokumente. Alle drei stehen auf einer Bibliothek, die das Format liest, und einem Zeichner, der es darstellt, sodass ein Absatz im Dokument, in einer Tabellenzelle und auf einer Folie gleich aussieht.

Was ein Editor tun muss und die meisten nicht tun: das ganze Dokument behalten. Jedes Element, jedes Attribut und jeder Leerraum Ihrer Datei übersteht das Einlesen, auch das, wovon Odox noch nie gehört hat, und was Sie nicht angefasst haben, wird so zurückgeschrieben, wie es gelesen wurde, Element für Element. Dass jedes Dokument im Prüfbestand unverändert zurückkommt und dass eine Änderung nur das ändert, was geändert wurde, wird bei jeder Änderung am Programm geprüft. Vor dem Speichern wird das, was geschrieben werden soll, zurückgelesen und mit dem Dokument im Fenster verglichen, und ein Unterschied verweigert das Speichern, statt ein Dokument zu schreiben, das nicht so zurückkäme.

In Rust geschrieben, frei und quelloffen unter der MIT-Lizenz, unter https://github.com/excelano/odox.

## Keywords

**Mac App Store** (100 characters, comma-separated, no spaces after commas):

    opendocument,odf,odp,präsentation,folien,editor,bearbeiten,oasis

**Microsoft Store** (seven terms):

    OpenDocument, ODF, ODP, Präsentation, Folien, Editor, OASIS
