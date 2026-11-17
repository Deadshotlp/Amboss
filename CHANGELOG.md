# Änderungen

## 0.9.10

- **Schwarze Map behoben:** Entities, denen Schlüssel fehlten (z. B. ein `light` ohne `_light`, angelegt mit einer älteren Version), wurden so kompiliert – VRAD rechnet ein Licht ohne `_light` mit Helligkeit 0, die Map blieb schwarz. Vor dem Kompilieren ergänzt Amboss jetzt alle fehlenden Schlüssel mit ihrem FGD-Standardwert (wie Hammer beim Anlegen; vorhandene Werte bleiben, `model` wird nie erfunden) und speichert sie in die VMF. Im Protokoll steht, was ergänzt wurde
- Lichter bekommen immer Farbe und Helligkeit, auch wenn die FGD keinen Standard nennt

## 0.9.9

- **Modelle in 2D als Umriss:** Props und andere Modell-Entities werden in den 2D-Ansichten mit ihrem Umriss gezeichnet (Rand- und Knickkanten des Modells) statt als Rechteck – in allen drei Ansichten, ausgewählt in Akzentfarbe. Entities ohne Modell (z. B. Lichter mit Symbol) bleiben Kästchen
- Kompilieren: schlägt das Kopieren der BSP fehl, erklärt das Protokoll die häufigste Ursache (Map im Spiel geladen)

## 0.9.8

- **Klassenfeld mit Vorschlägen:** Beim Tippen ins Feld „Klasse“ erscheinen darunter passende Klassennamen mit kurzer Beschreibung (Pfeiltasten, Enter/Tab oder Klick übernimmt) – beim ausgewählten Entity und beim Entity-Werkzeug (Klasse für neue Entities)
- **Pfeil neben dem Klassenfeld** öffnet die vollständige Klassenliste (Suche, Favoriten, zuletzt benutzt, Gruppen); beim ausgewählten Entity ändert die Wahl dessen Klasse
- **Weniger Doppeltes im Panel:** Knöpfe „Klasse ändern…“ und „Hilfe zur Klasse“ entfernt (Hilfe über „?“), Beschreibung steht nur noch einmal (oben beim Entity-Werkzeug)
- **Obere Leiste:** „Klasse“ und „Textur“ entfernt (Klasse rechts im Panel, Textur rechts bei den Texturen); dafür Pfeile für **Rückgängig** und **Wiederholen** (Strg+Z / Strg+Y), ausgegraut, wenn es nichts gibt
- Eigenschaften-Panel: Inhalt bleibt auch bei schmalem Panel vollständig sichtbar (Klassenfeld passt sich an)

## 0.9.7

- **Lichter mit echter GMod-FGD:** Das normale `light` hatte keine Farbe, Helligkeit, Erscheinungsbild usw. – Valves FGD enthält die Basisklasse „Light“ und das Entity „light“, die sich beim Einlesen gegenseitig überschrieben haben. Basisklassen werden jetzt getrennt gespeichert (betrifft alle gleichnamigen Basis-/Entity-Paare)
- **Modell-Browser ohne Ziel** (übers Menü geöffnet): „Übernehmen“ setzt das Modell jetzt bei den ausgewählten Entities (Props, Türen …) statt nur für neue Props; unten steht „Ziel: ausgewählte Entities“
- **Eigenschaften-Panel:** Felder lassen rechts mehr Platz, damit der Ordner-Knopf neben dem Modell-Feld (und andere Knöpfe) bei schmalem Panel nicht abgeschnitten wird; die Drehen-Knöpfe passen in jede Panelbreite

## 0.9.6

- **Live-Abgleich robuster (Brücke 2):** Amboss zeigt jetzt „Brücke veraltet – neu installieren“; danach GMod einmal neu starten
  - zusätzliches Lebenszeichen vom Client (`alive.txt`, mit Echtzeit) – die Verbindung bleibt auch bestehen, wenn das Spiel pausiert ist (Konsole oder Menü offen)
  - Zahlen aus GMod (z. B. `5.0`) werden richtig gelesen – vorher wurde die Brücken-Version als 0 erkannt und bestätigte Befehle nicht abgehakt
  - Diagnose im Live-Fenster: beobachteter Ordner und wann `out.txt`/`alive.txt` zuletzt geschrieben wurden; GMod-Konsole: `amboss_live_status`, beim Laden eine Meldung „[Amboss Live] Bruecke 2 aktiv …“
- Knopf „Brücke installieren“ übersetzt

## 0.9.5

- **3D-Kamera einstellbar** (Einstellungen → Tastenkürzel → „3D-Kamera“): Tasten für vor/zurück/links/rechts/hoch/runter, Umsehen mit mittlerer (Standard) oder rechter Maustaste, Empfindlichkeit, Maus-Y umkehren, Taste für schneller fliegen
- **Umsehen jetzt standardmäßig mit der mittleren Maustaste** (Mausrad gedrückt halten und ziehen)
- **Shift+A und andere Werkzeug-Kürzel funktionieren auch über der 3D-Ansicht** – vorher blockiert, weil Shift+WASD dort schnelles Fliegen war. Schneller fliegen mit Shift jetzt beim Umsehen (Umsehen-Taste gedrückt)
- **Textur-Werkzeug:** das ganze rechte Panel zeigt Texturen (alle Materialien, mehr Spalten); Flächen-Einstellungen lassen sich darüber aufklappen; ein Klick auf eine Textur wendet sie direkt auf die gewählten Flächen an
- **Modell-Feld:** gut sichtbarer Ordner-Knopf rechts daneben öffnet den Modell-Browser, „Übernehmen“ setzt das Modell direkt ins Feld (auch bei Feldern „model“ ohne Typ studio)
- **Modell-Browser:** Vorschaubilder in der Liste mit Textur statt grau
- **Entities rasten richtig ein:** beim Verschieben (2D und 3D-Pfeile) rastet die Ecke der Box am Raster ein statt nur die Strecke; beim Setzen in 3D auf dem eingestellten Raster statt auf 8
- **3D:** keine gelbe Box mehr um Modell-Entities, wenn sie nicht ausgewählt sind; Klicks gehen durch Löcher und Lücken eines Modells hindurch (Treffer auf die Modell-Dreiecke statt die Box)
- **Strg+R dreht zur Maus:** mit Vorschau, 15°-Schritte (Shift = frei), Linksklick übernimmt, Rechtsklick oder Esc bricht ab; ein einzelnes Entity schaut von oben direkt zur Maus. „90° drehen“ bleibt im Menü (ohne Kürzel), Strg+Shift+R dreht 90° zurück
- **Lichter:** neue Lichter haben immer Farbe und Helligkeit (`_light` = 255 255 255 200, bei light_environment auch `_ambient`); in den Eigenschaften „Farbe · Helligkeit“ mit Farbwähler und eigenem Zahlenfeld für die Helligkeit – auch wenn die FGD den Schlüssel nicht kennt
- **Aushöhlen:** Wände überschneiden sich nicht mehr – jede Ecke gehört genau einer Wand (Boden/Decke über die volle Fläche, dann die Seitenwände)

## 0.9.4

- **Raum-Generator – dünne Wände:** Wände sind nicht mehr so dick wie eine ganze Zelle (64), sondern eine dünne Platte in Wandstärke (Standard 16) am Zellrand; Ecken werden sauber geschlossen
- **Gänge mit langen Geraden:** kein Treppchen-Zickzack mehr – zwischen zwei Abbiegungen liegt immer ein längeres gerades Stück. Nur wenn das einen großen Umweg bedeuten würde (z. B. einmal um einen Raum herum), ist ein kurzer Versatz erlaubt
- **Runde Kurven:** Abbiegungen werden, wo Platz ist, zu echten Kurven mit runder Außen- und Innenwand (Keil-Brushes); neue Einstellung „Abbiegungen: als runde Kurve“ mit Radius in Zellen (0 = nur außen rund). Ohne Häkchen bleiben es gerade Ecken
- **2D-Ansichten:** Ist etwas ausgewählt, werden alle Objekte blasser gezeichnet, die nicht auf derselben Tiefe liegen (±10 Einheiten) – oben: gleiche Höhe (Z), vorne: gleiches X, seitlich: gleiches Y. Abschaltbar unter Ansicht → Raster → „2D: bei Auswahl andere Ebenen abdunkeln“

## 0.9.3

- **Raum-Generator – Gänge neu:** statt langer Gänge mit vielen flachen Stufen gibt es jetzt an der Raumtür eine kurze, echte Treppe (16 Einheiten Höhe je Stufe, rund 24 tief), danach geht ein ebener Gang weiter. Alle Gänge liegen auf einer gemeinsamen Höhe und bilden ein Netz
- **Keine doppelten Gänge mehr:** zwei Räume werden höchstens einmal direkt verbunden; Gänge laufen nicht mehr nebeneinander her, sondern treffen sich an Kreuzungen und Abzweigungen
- **Längere, gewundene Gänge** mit Kurven, Kreuzungen und Rundwegen; neue Regler „Abstand der Räume (Zellen)“, „Windungen“ und „Zusätzliche Verbindungen“
- Das Raster wächst automatisch mit, wenn viele oder große Räume nicht hineinpassen würden
- Räume, die keinen Anschluss bekommen, werden weggelassen statt abgeschnitten erzeugt. Bereits erzeugte Ergebnisse bleiben, wie sie sind – für die neuen Gänge neu generieren

## 0.9.2

- **Raum-Generator:** Räume lagen scheinbar ineinander – Boden-, Decken- und Wandsäulen reichten über die volle Höhe aller Räume. Jetzt ist jeder Raum eine eigene Hülle in seiner eigenen Höhe (Bodenplatte, Deckenplatte, Wände), jeder Gang eine eigene Röhre; nur an Höhensprüngen reicht eine Platte bis zum Nachbarn. Weiterhin dicht und alle Räume erreichbar; neuer Test: keine zwei Brushes überschneiden sich

## 0.9.1

- **Raum-Generator in 3D:** jeder Raum mit eigener Höhe (min/max) und Bodenstufe, Gänge mit Treppen (höchstens 16 Einheiten je Stufe); alles bleibt dicht, jeder Raum ist erreichbar (geprüft mit Leck-Prüfung und Navmesh)
- **Räume einzeln bearbeitbar:** jeder Raum und jeder Gang ist eine eigene Gruppe statt eines einzigen großen Blocks
- **Eigene Texturen** für Boden, Wände und Decke im Generator (mit Vorschaubild und „← aktuelle“)
- **Rausch-Generator** im Dialog „Gelände aus Heightmap“: Hügelgröße, Detail-Lagen, Rauheit, Talboden, Terrassen, Bergkämme, flache Ränder, Startwert; Live-Vorschau, Export als 16-Bit-PNG
- **Tutorial Kapitel 6** um Heightmap und Rausch-Generator erweitert

## 0.9.0

Erste nummerierte Version. Noch nicht unter Windows mit echtem Garry's Mod getestet – siehe `TESTPLAN_WINDOWS.md`.

**Neu in dieser Version**
- Versionsnummer neben dem Logo, im Fenstertitel, auf der Startseite und unter Hilfe → Über Amboss
- Tutorial mit vier neuen Kapiteln: Gelände mit Displacements, Licht/Himmel/Nebel, Feinarbeit (Vertex, 3-Punkt-Clip, Overlays, Decals), schneller bauen und testen
- Outputs: Vorschläge beim Ziel (Namen der Map mit Klasse, Sonderziele wie `!activator`, Auswahl mit Pfeiltasten und Enter/Tab)

**Seit der letzten Lieferung ohne Nummer**
- Clip mit 3 Punkten in 3D, Nebel-Vorschau, Seile, 2D-Himmel
- Partikel-Vorschau im 3D-Fenster, Begehen-Modus (F6), Im Spiel testen ab 3D-Kamera (Shift+F9), Live-Abgleich mit dem laufenden Spiel
- Polygon-Meshes und Bezier-Patches, Material- und Partikel-Editor, globale Beleuchtung, Physik-Platzierung, Vegetation, prozedurale Generatoren, Tile-Sets, Overlays auf Displacements
- „Auswahl als Modell“: Dreiecksreihenfolge für studiomdl korrigiert
