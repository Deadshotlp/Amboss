#!/usr/bin/env python3
"""Wertet eine AMBOSS_I18N_AUDIT-Datei (gezeichnete Texte in Englisch) aus und listet deutsche Reste.
Aufruf: python3 tools/i18n_audit.py audit.txt"""
import re, sys, glob, os
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
de_keys = {}
for f in glob.glob(os.path.join(ROOT, "assets/lang/en*.tsv")):
    for line in open(f, encoding="utf-8"):
        if "\t" in line and not line.startswith("#"):
            d, e = line.rstrip("\n").split("\t", 1)
            if d != e:
                de_keys[d.replace("\\n", "\n")] = e
WORDS = r"und|oder|nicht|keine?n?|wird|werden|ist|sind|mit|aus|für|beim|zum|zur|der|die|das|den|dem|des|eine?n?r?|vor|nach|von|bei|auf|neu|neue[rsn]?|alle[sn]?|gefunden|geladen|gespeichert|ausgewählt|verschieden|wählen|anzeigen|einfügen|löschen|speichern|öffnen|schließen|abbrechen|zurück|weiter|fertig|leer|aktuell|kopieren|übertragen|einstellen|hinzufügen|entfernen|suchen|filtern|Datei|Dateien|Ordner|Fehler|Warnung(en)?|Auswahl|Spiel|Textur(en)?|Fläche[n]?|Gruppe[n]?|Kinder|Ansicht|Werkzeug|Einstellungen|Objekte?|Welt|Schritte?|Eigenschaften|Hilfe|Klasse[n]?|Ausdehnung|Zwischenablage|Bibliothek|Bauteile|Technik|Logik|Türen|Fenster|Abstand|Größe|Winkel|Drehen|Höhe|Breite|Tiefe|Zeile|Spalte|Hinweis|Beispiel|Seite[n]?|Punkt|Rechteck|Strg|Umschalt|Entf|Leertaste"
GERMAN = re.compile(r"[äöüÄÖÜß]|\b(" + WORDS + r")\b")
texts = sorted(set(l.rstrip("\n").replace("\\n", "\n") for l in open(sys.argv[1], encoding="utf-8")))
hits = []
for t in texts:
    if t in de_keys:
        hits.append(("TABELLE", t)); continue
    if GERMAN.search(t):
        hits.append(("WORT", t))
for k, t in hits:
    print(f"{k:8} {t[:160]!r}")
print(f"\n{len(hits)} Verdachtsfälle von {len(texts)} Texten", file=sys.stderr)
