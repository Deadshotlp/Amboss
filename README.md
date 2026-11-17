# Amboss – Map-Editor für Source 1 / Garry's Mod

**Version 0.9.10** – steht oben neben dem Logo, im Fenstertitel und unter Hilfe → Über Amboss. Was neu ist, steht in `CHANGELOG.md`.

Amboss ist ein eigener Level-Editor in **Rust**, angelehnt an Hammer/Hammer++.
Er bearbeitet **VMF-Dateien**, zeigt echte **Texturen aus den GMod-/HL2-VPKs** an und
kompiliert die Map mit den originalen Valve-Tools (**VBSP / VVIS / VRAD**). Anschließend
startet er Garry's Mod direkt mit der neuen Map.

![Aufbau](https://img.shields.io/badge/Rust-egui%20%2B%20OpenGL-orange) ![Lizenz](https://img.shields.io/badge/Lizenz-MIT%20oder%20Apache--2.0-blue)

## Funktionen

### Einstieg
* **Einrichtungs-Assistent** beim ersten Start (Hilfe → Einrichtungs-Assistent…): Sprache, Oberfläche, Garry's Mod automatisch finden oder wählen (mit Prüfung von gameinfo.txt, Compilern, Spiel, FGD), Inhalte aus Addons/Workshop und eingebundenen Spielen (mount.cfg).
* **Tutorial** (Hilfe → Tutorial…) in neun Kapiteln – erste Map, Texturen und Formen, Entities, Tür und Trigger, Kompilieren und Testen, dazu Gelände mit Displacements, Licht/Himmel/Nebel, Feinarbeit (Vertex, 3-Punkt-Clip, Overlays, Decals) und schneller bauen (Prefabs, Generatoren, Begehen, im Spiel testen). Jeder Schritt erklärt die Aufgabe, hebt das passende Werkzeug hervor, prüft selbst, ob sie erledigt ist, und kann sie mit „Zeig es mir“ vormachen.
* **Einfache und Experten-Oberfläche** (Ansicht → Oberfläche oder Einstellungen): Einfach zeigt nur die Grundwerkzeuge (Auswahl, Block, Entity, Textur, Decal, Clip), kurze Menüs und weniger Schalter; Experte alles.

### Oberfläche
**Sprache wählbar:** Deutsch oder Englisch (Einstellungen → Grafik → Sprache / Language, sofort ohne Neustart) – auf Englisch auch die Beschreibungen der eingebauten Entity-Liste (Klassen, Schlüssel, Auswahlwerte, Flags, Ein-/Ausgänge). Weitere Sprachen: Tabelle `assets/lang/<code>.tsv` anlegen.
Neues Design nach UI-Konzept in **Dunkel und Hell** (Umschalter Sonne/Mond oben rechts oder Einstellungen → Grafik → Design):
obere Leiste mit Menüs, Befehlssuche (Strg+K), Live-Sitzung und „Kompilieren & starten“ mit Profilwahl · Tabs mit Speicherstand und Spielprofil ·
schmale Werkzeugleiste mit Symbolen · Kontextleiste je Werkzeug (Form, Rückgängig/Wiederholen-Pfeile, Raster, Einrasten, An Eckpunkten, Texturen fixieren, Visgroups, 1/4 Ansichten) ·
Ansichten mit Glas-Schildern (Ansicht/Darstellung umschalten), Zoom bzw. FPS, Akzent-Auswahl mit Griffen, Maßangabe und Etikett · bei Auswahl werden in 2D Objekte auf anderer Ebene (andere Tiefe, ±10) blasser gezeichnet · Modelle erscheinen in 2D als Umriss statt als Rechteck ·
rechtes Panel mit Eigenschaften / Objekte / Visgroups und Textur-Raster (Breite ziehbar) · **Trenner zwischen den vier Ansichten ziehbar** (Doppelklick = Mitte); Aufteilung, Achsen und 3D-Optionen werden gespeichert, Ansicht → „Layout zurücksetzen“ · **Map-Grenze je Spielprofil** (Einstellungen, Standard ±16384, in 2D rot markiert) · Startseite mit Vorlagen, Wiederherstellung nach Absturz und zuletzt geöffneten Maps.
Schriften: Inter und JetBrains Mono (SIL Open Font License, liegen unter `assets/fonts`). Die 3D-Ansicht bleibt in beiden Designs dunkel.

### Werkzeuge (wie in Hammer)

| Werkzeug | Taste | Was es kann |
|---|---|---|
| Auswahl | Shift+S | Klick, Rahmen, Strg = hinzufügen. Griffe: Skalieren → Drehen (15°-Raster, Shift = frei) → Scheren |
| Lupe | Shift+G | Klick zoomt rein, Rechtsklick raus |
| Kamera | Shift+C | Mehrere 3D-Kameras, in 2D setzen/verschieben, Bild↑/↓ wechselt (werden in der VMF gespeichert) |
| Entity | Shift+E | Punkt-Entities in 2D oder direkt auf Oberflächen in 3D platzieren |
| Block | Shift+B | Block, Keil, Zylinder, Spitze, Kugel, Bogen, Torus (mit Seiten, Bogenwinkel, Wandstärke, Wendel-Höhe) |
| Flächen/Textur | Shift+A | Flächen wählen, Skalierung/Verschiebung/Rotation/Lightmap/Smoothing, Einpassen/Links/Rechts/Oben/Unten/Mitte, „als Einheit“, an Welt/Fläche ausrichten, Alt+Rechtsklick = nahtlos umwickeln. Das rechte Panel zeigt dabei nur Texturen (Klick = auf gewählte Flächen anwenden), Flächen-Einstellungen aufklappbar |
| Decal | Shift+D | `infodecal` auf Flächen setzen, wird in 3D mit Textur angezeigt |
| Overlay | Shift+O | `info_overlay` auf eine Fläche legen; ausgewählte Overlays haben in 3D runde Eckgriffe, einen Drehgriff und ein Verschiebe-Kreuz |
| Clip | Shift+X | Brushes entlang einer Linie (2D) oder einer Ebene durch **drei Punkte in 3D** schneiden, Vorschau in 2D und 3D, Vorne/Hinten/Beide (Shift+X wechselt), Enter |
| Vertex | Shift+V | Eckpunkte und Kanten ziehen, Mehrfachauswahl, ungültige (nicht-konvexe) Formen werden abgelehnt |
| Terrain | Shift+P | Displacements formen: Anheben, Absenken, Glätten, Schärfen, Auf Höhe setzen, Rauschen, Terrassen, Alpha malen/entfernen; Abfall weich/linear/hart, Strg kehrt Glätten ↔ Schärfen um |
| Cordon | Shift+K | Teilbereich kompilieren (automatisch mit Skybox-Wänden abgedichtet) |

### Bearbeiten
Undo/Redo · Kopieren/Einfügen · **Paste Special** (Kopien mit Versatz + Drehung) · **Carve** · **echtes Aushöhlen** (auch schräge Brushes, Wände innen oder außen) · **Gruppen** · **Ausblenden** (H / Shift+H / U) · **Visgroups** + automatische Filter (Welt, Detail, Trigger, Tool-Texturen, Lichter, Props, Displacements …) · **Transformieren-Dialog** (exakt verschieben/drehen/skalieren) · **Spiegeln** · **Objekte ausrichten** · **Am Raster ausrichten** · **Texturen ersetzen** · **Textur-Lock und Skalier-Lock** · **Auswahlmodus** Gruppen/Objekte/einzelne Brushes · **Prefabs** speichern & einfügen · **Instanzen** (`func_instance`) anzeigen und einklappen – auch **verschachtelt** (Instanzen in Instanzen, Namen und `$`-Parameter wie VBSP), „Instanz in Tab öffnen“, Speichern aktualisiert alle Maps · **Raster-Einrasten abschaltbar** (Alt+G) · **Einrasten an Eckpunkten** anderer Objekte (Alt+V) · **Rahmenauswahl auch in 3D** · **berührende Objekte auswählen** (Strg+Alt+A, Alt beim Rahmen) · 2D-Auswahl über Kanten und Mittelpunkt · Rückgängig speichert nur Änderungen (wenig Speicher) · **Auswahl als Modell** (wie Propper, Werkzeuge → „Auswahl als Modell (Prop)…“): Brushes werden als SMD/QC mit VertexLitGeneric-Materialien exportiert, mit studiomdl kompiliert und durch ein prop_static/prop_physics ersetzt

### Displacements
Erstellen (Power 2–4), Entfernen, **Unterteilen** (Catmull-Clark-Glättung über mehrere Displacements – aus einem Würfel wird eine Kugel), **Feinheit ändern ohne Formverlust**, Rauschen, Nähen (Sew), Alpha invertieren, **Gelände aus Heightmap** (PNG 8/16 Bit oder RAW 16 Bit → nahtlose Kacheln, Karte → „Gelände aus Heightmap…“) mit eingebautem **Rausch-Generator** (Hügelgröße, Detail-Lagen, Rauheit, Talboden, Terrassen, Bergkämme, flache Ränder, Startwert; Vorschau, Export als 16-Bit-PNG), Kollisions-Flags (No Phys/Hull/Ray), Pinsel mit Radius/Stärke/Richtung, Alpha-Anzeige

### Darstellung (3D)
Texturiert (inkl. **Blend-Texturen** WorldVertexTransition, gemischt über Displacement-Alpha, `$basetexturetransform`, **animierte Texturen** und **TextureScroll**) · Einfarbig · Drahtgitter · Lightmap-Raster · Smoothing-Gruppen · **Lichtvorschau mit Schatten** (light, light_spot, light_environment; Sonne und die vier nächsten Lichter werfen Schatten) · **Kollisionsmodelle** der Props (.phy) als Drahtgitter · **Modelle (MDL)** mit Texturen und Skins · Editor-Sprites · Lichtkegel-Helfer · Verbindungslinien (Outputs) · **3D-Skybox-Vorschau** (sky_camera) · **2D-Himmel** (`skyname` aus worldspawn) · **Nebel** aus `env_fog_controller` · **Seile** (`move_rope`/`keyframe_rope` mit Durchhang, auch in 2D) · Pointfile (Leck-Pfad) · Portalfile · **Frustum-Culling** für große Maps · **Kantenglättung (MSAA)** unter Einstellungen → Grafik

### Gestalten (Werkzeuge-Menü)
* **Material-Editor** – eigene Materialien (VMT) anlegen, PNG-Bilder als VTF importieren (mit Mipmaps, Normalmaps), Shader, Durchsichtigkeit, Spiegelung, Oberflächenart, Live-Vorschau.
* **Partikel-Editor** – .pcf-Dateien öffnen, Systeme aus Vorlagen anlegen, Emitter/Initialisierer/Operatoren/Renderer bearbeiten, animierte Vorschau, Partikel-Manifest schreiben.
* **Globale Beleuchtung** – in der Lichtvorschau zuschaltbar: indirektes Licht mit Farbübertrag, berechnet im Hintergrund.
* **Mit Physik fallen lassen** (Objekt-Menü) – Props fallen auf Boden, Displacements und aufeinander; Esc bricht ab.
* **Vegetation streuen** – Pinsel in 3D für Bäume, Büsche, Steine (Dichte, Abstand, zufällige Größe/Drehung/Neigung, Shift radiert) oder Auswahl füllen.
* **Prozedurale Generatoren** – Labyrinth, Räume mit Gängen, Stadtblock, Kreis-Anordnung. Räume in 3D: eigene Höhe je Raum, ein Gangnetz mit langen Geraden, runden Kurven (Radius einstellbar), Kreuzungen und Rundwegen (je zwei Räume höchstens einmal verbunden), dünne Wände in Wandstärke, kurze Treppen an den Raumtüren (16 je Stufe), Regler für Abstand, Windungen und zusätzliche Verbindungen, eigene Texturen für Boden, Wände und Decke; jeder Raum und jeder Gang ist eine eigene Gruppe und lässt sich einzeln bearbeiten.
* **Tile-Set** – Boden, Wand, Ecke, Tür, Fenster, Säule, Treppe … auf einem Raster setzen; R dreht, Shift entfernt.
* **Polygon-Mesh** – freie Netze aus Brushes oder OBJ: Punkte und Flächen in 3D ziehen, Extrudieren, Glätten, Unterteilen; wird als prop_static über studiomdl zum Modell (Netzdaten in `<map>.meshes.json` neben der VMF – mitkopieren!).
* **Bezier-Patch** – eine Displacement-Fläche mit 16 Kontrollpunkten glatt biegen.
* Overlays auf Displacements folgen im 3D-Fenster der Fläche wie im Spiel.
* **Partikel-Vorschau** – `info_particle_system` spielt seinen Effekt im 3D-Fenster ab (Ansicht → „Partikel-Effekte abspielen“).

### Testen und Live-Abgleich
* **Begehen** (F6) – mit Spielergröße, Schwerkraft und Kollision durch die Map laufen: WASD, Leertaste springt, Strg duckt, Shift rennt, Esc beendet.
* **Im Spiel testen ab 3D-Kamera** (Shift+F9) – kompiliert und startet Garry's Mod mit dem Spieler an der Kamera; läuft das Spiel schon, wird die Map dort neu geladen.
* **Live-Abgleich** (Kompilieren → „Live-Abgleich mit dem Spiel…“) – Änderungen erscheinen sofort im laufenden Spiel: Entities verschieben und ändern, neue Props/Entities, Löschen, neue Brushes als begehbare Vorschau, Inputs auslösen, Spieler zur Kamera holen oder Kamera dem Spieler folgen lassen. Einmalig „Brücke installieren“ (legt `addons/amboss_live` an) und GMod neu starten; Austausch über `garrysmod/data/amboss_live/`, nur im Einzelspieler bzw. auf dem eigenen Server.

### Entities
**456 Entity-Klassen eingebaut** (func_, prop_, npc_, trigger_, logic_, env_, info_, point_, filter_, game_, item_, weapon_, ai_, path_, phys_ … mit Feldern, Ein-/Ausgängen und Beschreibungen); eine echte `garrysmod.fgd` wird automatisch gefunden und hat Vorrang · **Klassen-Feld mit Vorschlägen beim Tippen** und Pfeil zur **Klassen-Auswahl** (Suche, Favoriten, zuletzt benutzt, Gruppen nach Präfix, Tastatur) · **In Entity umwandeln** (Strg+T) als Dialog · **Sound-Browser mit Abspielen** (Windows), **Modell-Browser mit drehbarer 3D-Vorschau**, **Partikel-Browser** (.pcf) · FGD-Parser (`garrysmod.fgd` inkl. `@include`) · Auswahllisten, Farbwähler, Spawnflags · **Winkel-Kompass** · **Pipette** und Namensliste für Ziele · **Outputs mit Prüfung** (rot = Ziel fehlt, orange = Input unbekannt), **Vorschläge beim Ziel** (Namen aus der Map mit Klasse und `!activator`/`!player` …, Pfeiltasten + Enter/Tab) und Input-Auswahl · eingehende Verbindungen · **Mehrfachbearbeitung** · **Eigenschaften kopieren/übertragen** (Strg+Alt+C / V) · Modell- und Sound-Browser · Hilfe pro Klasse · **Logische Ansicht** (Entity-I/O als Diagramm) · **Bewegungs-Vorschau** für ausgewählte Türen/Knöpfe (func_door, func_button, func_movelinear: Pfeil + Endposition; func_door_rotating, prop_door_rotating: Drehbogen + geöffnete Tür, bei beidseitig öffnenden Türen beide Richtungen) · Modell-Browser direkt am Modell-Feld (Ordner-Knopf, aktuelles Modell vorgewählt, „Übernehmen“ setzt es ein), Vorschaubilder mit Textur

### Karte
**Auf Probleme prüfen** mit Auto-Fix (Alt+P) · **Entity-Report** · **Gehe zu Nummer** (z. B. aus VBSP-Fehlern) · Karten-Info · Map-Eigenschaften

### Navmesh (Karte → Navmesh erzeugen)
Eigener Generator für das NextBot-Navmesh – ohne Spiel und ohne `nav_generate`: Ab den Spawnpunkten wird ein 25er-Raster abgelaufen (Stufen bis 18, Herunterfallen bis 200, Spieler 32×72, sonst geduckt 36, Boden bis 45° Neigung, Brushes und Displacements; Türen gelten als offen). Die Zellen werden zu Rechtecken zusammengefasst und als `garrysmod/maps/<map>.nav` gespeichert. Grün/blau (geduckt) in 2D und 3D sichtbar, „Navmesh laden (.nav)…“ zeigt auch vorhandene Dateien. Mit der Kompilier-Option „Navmesh“ entsteht es automatisch vor VBSP und wird mit „Inhalte packen“ in die BSP gelegt. Noch nicht berücksichtigt: Kollision von Props, Leitern, Verstecke/Sichtbarkeit (`nav_analyze`).

### Kompilieren & Exportieren
Ein Dialog für alles (Kompilieren → Kompilieren & Exportieren…): Qualität von VVIS/VRAD, automatische Optimierungen, **Navmesh erzeugen**, BSP in den maps-Ordner kopieren, **eigene Inhalte in die BSP packen** (aufklappbare, bearbeitbare Dateiliste; „Nur packen“ packt ohne neu zu kompilieren), Cubemaps, Spiel starten. Eigene Kompilierschritte gibt es in der Experten-Oberfläche.
**BSP-Dateien öffnen:** Datei → Öffnen zeigt VMF und BSP; eine BSP öffnet den Dialog „BSP dekompilieren“ und danach die VMF (auch per Ziehen ins Fenster).

### Kompilieren
Einfacher Modus (VBSP/VVIS/VRAD-Stufen, HDR, Zusatzparameter, Cubemaps, Kopieren, Spielstart) und **Expertenmodus** mit frei definierbaren Schritten · anklickbare Fehlerzeilen (springt zum Brush) · **Pointfile wird bei Lecks automatisch geladen** · Cordon-Kompilieren · optional **automatische Optimierungen** vor VBSP · **Inhalte in BSP packen** (eingebaut, ohne bspzip; Kompilieren → „Inhalte in BSP packen…“): findet Materialien inkl. Bumpmaps/Envmaps, Modelle mit allen Dateien und Skins, Sounds, Partikel (.pcf nach benutzten Systemen + Materialien), Soundscapes, Nav-Mesh und Übersichtskarte; Liste bearbeitbar, Auswahl gilt auch beim Kompilieren, Addon-Inhalte bleiben standardmäßig draußen · **BSP dekompilieren** (Datei → „BSP dekompilieren…“, wie BSPSource): Brushes, func_detail, Brush-Entities, Displacements, Entities, statische Props, Overlays, Cubemaps → VMF

### Automatische Optimierungen (Karte → Automatische Optimierungen…)
Laufen auf Knopfdruck („Nur prüfen“ / „Auf Map anwenden“ mit einem Rückgängig-Schritt) oder **beim Kompilieren automatisch auf einer Kopie**
(`amboss_build/<map>.vmf` neben der VMF – die eigene Map bleibt unverändert, der Bericht steht oben im Compile-Log). Jede Optimierung ist einzeln abschaltbar.

| Bereich | Was passiert |
|---|---|
| **KI (NPCs)** | `info_node` auf allen begehbaren Flächen (Abstand einstellbar, dichter an Türen/Engstellen) · **Netz-Prüfung**: Verbindungen werden simuliert und grün angezeigt, isolierte Nodes und getrennte Bereiche rot/orange markiert · `info_node_air` für fliegende NPCs · `info_node_link_controller` an Türen (OnOpen/OnClose werden ergänzt) · `info_hint` für Deckung und Engstellen · Navmesh für NextBots (`nav_generate` beim Spielstart) |
| **Sichtbarkeit** | **Leckprüfung vor VBSP** per Flutfüllung (mit rotem Pfad, Compile wird abgebrochen) · verdeckte Flächen → nodraw · kleine/schräge Brushes ohne Kontakt zur Leere → `func_detail` (Trennwände und Türrahmen bleiben Welt) · `func_areaportal` in Tür-Öffnungen · Hint-Brush-Vorschläge (gelb) |
| **Licht** | Lightmap-Auflösung nach Nähe zu Lichtern (8/16/32) · `env_cubemap` automatisch verteilen · Lichtprüfung (Licht in Brush, > 32 schaltbare Lichter, dunkle Bereiche) |
| **Geometrie** | Eckpunkte auf ganze Einheiten runden · gleiche Quader zusammenführen · Splitter und doppelte Brushes entfernen · flache Displacements vereinfachen |
| **Props & Entities** | Ausblend-Distanzen nach Modellgröße · Schatten kleiner Props aus · wirkungslose Logik und doppelte Cubemaps entfernen · **eigene Inhalte mit bspzip in die BSP packen** (Materialien/Modelle, die lose im Spielordner liegen) |

Per MCP: `optimize_map` (prüfen oder anwenden, Optionen pro Lauf) und `optimize_settings`.

### Zusammenführen, Vergleichen, gemeinsam arbeiten
* **Map zusammenführen** (Datei → Map zusammenführen…): eine andere VMF/.map übernehmen – Koordinaten behalten, daneben setzen oder verschieben; doppelte Entity-Namen werden umbenannt (Verweise ziehen mit), alles landet in einer Visgroup „Import: …“, unterschiedliche Map-Eigenschaften (Himmel, Detail-Material …) einzeln wählbar, Warnungen bei Überlappung, zwei Sonnen, zwei Skybox-Kameras.
* **Mit anderem Map-Stand vergleichen** (Karte → …): neue (grün), gelöschte (rot) und geänderte (gelb) Objekte farbig in allen Ansichten, Liste zum Anklicken.
* **3-Wege-Zusammenführen** (Karte → …): offene Map + gemeinsamer Ausgangsstand + Version einer anderen Person. Änderungen beider Seiten werden kombiniert; echte Konflikte behalten deine Version, die andere liegt ausgeblendet in der Visgroup „Konflikt: andere Version“.
* **Als Git-Merge-Treiber** (zwei Leute bearbeiten dieselbe VMF in Git):

```bash
git config merge.amboss.driver "amboss --merge3 %O %A %B"
```

  dazu in `.gitattributes`: `*.vmf merge=amboss`. Ohne Konflikte läuft das Zusammenführen automatisch durch.
  `amboss --diff ALT.vmf NEU.vmf` listet die Unterschiede auf.
* **Live-Zusammenarbeit** (Extras → Zusammenarbeit): eine Person ist Gastgeber und teilt die offene Map, andere treten mit Adresse und Passwort bei.
  Änderungen erscheinen sofort bei allen; was jemand ausgewählt hat, ist für die anderen gesperrt und in seiner Farbe umrandet;
  Kameras der anderen sind in 2D/3D zu sehen; Chat; **Strg+Z nimmt nur die eigenen Schritte zurück**. Jede Person bekommt einen eigenen ID-Bereich.
  Die Verbindung ist verschlüsselt (ChaCha20 + HMAC-SHA256, Schlüssel aus Passwort und Zufallswerten; das Passwort selbst wird nie gesendet). Standard-Port 3918.
  **Über das Internet ohne Server:** Der Gastgeber lauscht auf IPv4 und IPv6. Haben beide IPv6, verbindet sich der Gast direkt mit der IPv6-Adresse, die im Sitzungsfenster angezeigt wird (z. B. `[2001:db8::1]:3918`). Dafür muss die Router-Firewall des Gastgebers eingehende Verbindungen auf TCP-Port 3918 zu diesem PC erlauben (FRITZ!Box: Internet → Freigaben → Gerät → IPv6-Freigabe), ggf. auch die Windows-Firewall. Ohne IPv6 geht es per IPv4-Portweiterleitung (nicht bei DS-Lite/CGNAT) oder über ein VPN wie Tailscale/ZeroTier. Für das Internet ein langes Passwort wählen.
* Per MCP: `merge_map`, `compare_map`, `merge_versions`.

### Neue Map aus Vorlage
Startbildschirm (abschaltbar) und Datei → Neu aus Vorlage…: leere Map, Raum mit Licht und Spawn, Außenbereich, große Bauwiese, Arena, zwei Räume mit Tür (NPC-Test) – alle abgedichtet und sofort kompilierbar.

### Komfort (aus anderen Editoren übernommen)
**3D-Gizmo** (verschieben/drehen) · **Shift+Ziehen dupliziert** · **Flächen in 3D ziehen und extrudieren** · **Treppen, Wendeltreppen, Rampen, Rohre, Kuppeln, Zäune** · **Ebenen sperren** und Objekte sperren · **Objektliste (Outliner)** · **Kopieren als VMF-Text** (zwischen Tabs und Programmfenstern) · **Messwerkzeug** · **Kamera-Lesezeichen** · **Gehe zu Koordinate** · **Befehlspalette** (Strg+K) · **frei belegbare Tastenkürzel** · **zuletzt benutzte und favorisierte Texturen** · **Textur-Pipette** (Alt+Klick) · **Drehen in festen Schritten** · **verdeckte Flächen automatisch auf nodraw** · **.map-Import** (TrenchBroom/J.A.C.K./Quake) · **OBJ-Export** (für Blender) · **mehrere Maps in Tabs** · **Rückgängig-Verlauf** zum Anklicken · **Workshop-Addons (.gma)** als Inhaltsquelle

### Prefab-Bibliothek (Datei → Prefabs…)
* **14 mitgelieferte Prefabs** in vier Kategorien:
  * *Türen & Fenster:* Schiebetür mit Knöpfen, Drehtür, Fenster mit Glas
  * *Bauteile:* Leiter, Deckenlampe, Treppe mit Podest, Säule, Physik-Kiste
  * *Technik & Logik:* Lichtschalter, Teleporter-Paar, Schadenszone
  * *Räume:* Spawn-Raum, Außenbereich mit Himmel, Wasserbecken
* **Eigene Prefabs:** Auswahl speichern, Unterordner im Prefab-Ordner werden zu Kategorien.
* **Vorschaubilder** werden automatisch gerendert (Kachelansicht mit Suche).
* **Einfügen:** Kachel in eine 2D- oder 3D-Ansicht ziehen oder anklicken und dann in die Ansicht klicken (Shift = mehrfach).
  In 3D richtet sich das Prefab an der Fläche aus: Boden → steht darauf, Decke → hängt daran, Wand → liegt davor.
* **Doppelte Entity-Namen** werden beim Einfügen automatisch umbenannt (`tuer` → `tuer_2`), Outputs ziehen mit.
* **Hammer-Bibliotheken (.ol) und .rmf**: werden direkt angezeigt (Kategorie „Hammer / …“) oder per Knopf in VMF-Dateien umgewandelt.
* „Mitgelieferte als VMF kopieren“ legt die Bibliothek als bearbeitbare Dateien im Prefab-Ordner ab.

### MCP – KI-Fernsteuerung
Amboss enthält einen **MCP-Server** (Model Context Protocol). Damit kann ein KI-Assistent wie Claude Maps bauen und bearbeiten:
Räume, Formen, Prefabs, Entities, Outputs, Materialien, Verschieben/Drehen/Schneiden, Problemprüfung, automatische Optimierungen, Maps zusammenführen und vergleichen, Gelände aus Heightmaps, Brushes als Modell kompilieren, Inhalte in BSP packen, BSP dekompilieren, Lua-Skripte ausführen, Navmesh erzeugen, Speichern und Kompilieren (48 Werkzeuge).

* Läuft automatisch, solange Amboss offen ist: `http://127.0.0.1:3917/mcp` (nur lokal erreichbar; Port in den Einstellungen).
* **Claude Code:**

```bash
claude mcp add --transport http amboss http://127.0.0.1:3917/mcp
```

* **Claude Desktop** (`claude_desktop_config.json`):

```json
{ "mcpServers": { "amboss": { "command": "C:\\Pfad\\amboss.exe", "args": ["--mcp-stdio"] } } }
```

  Läuft der Editor, steuert die KI ihn live; sonst arbeitet `--mcp-stdio` ohne Fenster.
* Ohne Fenster direkt auf einer Datei: `amboss --mcp-stdio --headless --map pfad.vmf [--game-dir …/garrysmod] [--prefab-dir …]`

### Lua-Skripte (Werkzeuge → Lua-Skripte…)
Eingebautes **Lua 5.4** für eigene Werkzeuge und Wiederholarbeiten (Treppen, Lichtraster, Umbenennen …). Editor mit Ausgabe, **Strg+Enter** führt aus, Skripte liegen in `skripte/` neben `amboss.exe` (Beispiele werden beim ersten Öffnen angelegt).
Im Skript steht **jedes Werkzeug der KI-Fernsteuerung** bereit: `amboss.call("create_brush", {min = {0,0,0}, max = {64,64,64}, material = "DEV/DEV_MEASUREGENERIC01"})`, dazu Abkürzungen: `amboss.brush`, `amboss.room`, `amboss.entity`, `amboss.set`, `amboss.get`, `amboss.output`, `amboss.move`, `amboss.delete`, `amboss.select`, `amboss.selection`, `amboss.entities`, `amboss.info`. `print()` schreibt in die Ausgabe. Ein ganzes Skript ist **ein** Rückgängig-Schritt; Endlosschleifen brechen nach 10 s ab.

```lua
for i = 0, 11 do  -- Treppe
  amboss.brush({i * 16, 0, 0}, {(i + 1) * 16, 64, (i + 1) * 18}, "DEV/DEV_MEASUREGENERIC01")
end
```

### Testen ohne Garry's Mod
Der kostenlose Steam-Titel **Source SDK Base 2013 Multiplayer** enthält HL2-Texturen sowie VBSP/VVIS/VRAD.
Unter *Einstellungen* als Spielordner `…/Source SDK Base 2013 Multiplayer/hl2` (oder `…/hl2mp`) eintragen und die Tools aus `…/bin` wählen.
Amboss liest dann alle `*_dir.vpk` des Spielordners und den Basisordner `hl2`.

### Inhalte und Workshop
Einstellungen → „Inhalte aus Addons laden“ abschaltbar (dann nur Spielverzeichnis) · beim Kompilieren entsteht `<map>_addons.txt` mit allen benötigten Workshop-Addons und Links (Liste auch oben im Compile-Log)

### Sonstiges
**Spielprofile** (mehrere Spiele/Installationen) · **Autosave** · Textur-Browser (Filter, „in Map verwendet“) · VPK/VTF/VMT/MDL direkt aus den Spielinhalten

## Installation

### Fertige Version (Windows)
`amboss.exe` irgendwo hinlegen und starten. Es wird nichts installiert.

### Selbst bauen
1. Rust installieren: <https://rustup.rs>
2. Im Projektordner ausführen:

```bash
cargo build --release
```

3. Das Programm liegt danach unter `target/release/amboss(.exe)`.

Unter Linux werden zusätzlich die üblichen X11/Wayland-Bibliotheken benötigt
(`libxkbcommon-x11-0`, OpenGL). Die Compile-Tools von Valve sind Windows-Programme.
Unter Linux trägst du in den Einstellungen als *Wrapper* `wine` ein.

## Erste Schritte

1. **Extras → Einstellungen → „GMod automatisch finden“.** Das setzt Spielordner, VBSP/VVIS/VRAD,
   die Spiel-EXE und `garrysmod.fgd`. Danach werden die Texturen eingelesen.
2. **Raum bauen:** Block-Werkzeug (`Shift+B`), in der Top-Ansicht ein Rechteck ziehen und
   danach `Strg+H` (Aushöhlen) drücken.
3. **Texturen:** `Shift+T` öffnet den Browser. Ein Doppelklick wendet die Textur auf die Auswahl an.
4. **Entities:** Entity-Werkzeug (`Shift+E`), z. B. `info_player_start` und `light` wählen und in
   2D oder 3D klicken.
5. **Testen:** `F9` speichert, kompiliert und startet GMod mit der Map.

> Tipp: Jede Map muss **dicht** sein. Es darf kein Loch ins „Nichts“ geben, sonst meldet VBSP ein *LEAK*.
> Ein Himmel ist ein Brush mit `TOOLS/TOOLSSKYBOX`. Für Sonnenlicht gibt es `light_environment`.

## Tastenkürzel

| Taste | Funktion |
|---|---|
| `Shift+S G C E B A D O X V P K` | Werkzeuge (siehe oben) |
| `Shift+T` / `Strg+Shift+T` | Aktuelle Textur anwenden / Textur-Browser |
| `[` / `]` | Raster kleiner / größer |
| Pfeiltasten | Auswahl (oder Eckpunkte) um ein Rasterfeld verschieben |
| `Strg+Z` / `Strg+Y` | Rückgängig / Wiederholen |
| `Strg+C / V / X`, `Strg+Shift+V` | Kopieren / Einfügen / Ausschneiden, Paste Special |
| `Entf` | Löschen |
| `Strg+G` / `Strg+U` | Gruppieren / Gruppe lösen |
| `H` / `Shift+H` / `U` | Auswahl ausblenden / Rest ausblenden / alles einblenden |
| `Strg+T` / `Strg+Shift+W` | In Entity umwandeln / zurück in Welt |
| `Strg+H` / `Strg+Shift+C` | Aushöhlen / Carve |
| `Strg+M` / `Strg+B` | Transformieren / am Raster ausrichten |
| `Strg+L` / `Strg+I` | Spiegeln horizontal / vertikal (bezogen auf die Ansicht unter der Maus) |
| `Strg+R` | Zur Maus drehen mit Vorschau (um die Blickachse der Ansicht, 15°-Schritte, Shift = frei); Linksklick übernimmt, Rechtsklick/Esc bricht ab · `Strg+Shift+R` = 90° zurück |
| `Enter` / `Esc` | Clip ausführen / abbrechen bzw. Auswahl aufheben |
| `Alt+P` | Auf Probleme prüfen |
| `Strg+Shift+F` / `Strg+Shift+G` | Entity-Report / Gehe zu Nummer |
| `Strg+E` / `Strg+Shift+E` | 2D- bzw. 3D-Ansicht auf Auswahl zentrieren |
| `Bild↑` / `Bild↓` | Kamera wechseln |
| `Shift+Z`, Doppelklick mittlere Maustaste | Ansicht maximieren |
| 3D: mittlere Maustaste (Mausrad gedrückt) + Ziehen, `W A S D`, `Q`/`E` | Umsehen, fliegen, runter/hoch (`Shift` beim Umsehen = schneller); alles änderbar unter Einstellungen → Tastenkürzel → „3D-Kamera“ (auch rechte Maustaste, Empfindlichkeit, Y umkehren) |
| 2D: Mausrad, mittlere Maustaste / Leertaste + Ziehen | Zoomen, Verschieben |
| `F9` | Kompilieren & starten |
| `Shift+F9` | Im Spiel testen ab 3D-Kamera |
| `F6` | Begehen (im Editor spielen) |
| `Strg+K` | Befehlspalette |
| `Strg+F9` | Dialog Kompilieren & Exportieren |
| `Strg+D`, Shift+Ziehen | Duplizieren |
| `L` / `Shift+L` | Sperren / alles entsperren |
| `Shift+M` | Messwerkzeug |
| `Shift+W` | 3D-Gizmo ein/aus |
| Alt+Klick (3D) | Textur-Pipette |
| `Strg+Shift+1…9` / `Strg+1…9` | Kamera-Lesezeichen speichern / anspringen |
| `Strg+Shift+O` / `Strg+Shift+H` | Objektliste / Rückgängig-Verlauf |
| `Strg+Tab` / `Strg+F4` | Nächster Tab / Tab schließen |
| `Alt+G` / `Alt+V` | Am Raster einrasten an/aus / an Eckpunkten einrasten an/aus |
| `Strg+Alt+A` | Berührende Objekte zur Auswahl hinzufügen |
| `Strg+Alt+C` / `Strg+Alt+V` | Entity-Eigenschaften kopieren / auf Auswahl übertragen |
| 3D: linke Maustaste ziehen (Auswahl-Werkzeug) | Rahmenauswahl (`Strg` = hinzufügen, `Alt` = auch berührte) |

Alle Kürzel lassen sich unter *Extras → Einstellungen → Tastenkürzel* ändern.

## Aufbau des Codes

```
src/
  main.rs        Programmstart (eframe/OpenGL)
  app.rs         Hauptfenster: Menüs, Werkzeugleiste, Tastenkürzel, Ansichten, Autosave
  panels.rs      Eigenschaften-Panel, Werkzeug-Optionen, Visgroups, Entity-Editor
  dialogs.rs     Alle Fenster (Compile, Einstellungen, Browser, Probleme, Report, logische Ansicht …)
  editor.rs      Editor-Zustand & alle Operationen (GUI-unabhängig, getestet)
  map.rs         Datenmodell + VMF lesen/schreiben (Gruppen, Visgroups, Kameras, Cordon, hidden)
  geometry.rs    Ebenen → Polygone, konvexe Hülle, AABB, Strahltests
  transform.rs   Affine Transformationen, Source-Winkel
  csg.rs         Schneiden an Ebenen, Carve, Aushöhlen, Vertex-Umbau
  primitives.rs  Grundformen des Block-Werkzeugs
  disp.rs        Displacements (VMF, Geometrie, Pinsel)
  problems.rs    Problemprüfung mit Korrekturen
  pointfile.rs   .lin / .prt
  view2d.rs      Top/Front/Side mit allen Werkzeugen
  view3d.rs      OpenGL-Renderer, Render-Modi, Lichtvorschau, Modelle, Skybox
  keyvalues.rs   KeyValues-Parser/-Writer
  fgd.rs         FGD-Parser + eingebaute GMod-Entities
  gamefs.rs      VPK-Leser + Spiel-Dateisystem
  vtf.rs         VTF-Decoder
  materials.rs   VMT-Auflösung, Textur-Ladethreads
  mdl.rs         MDL/VVD/VTX-Lader
  models.rs      Modell-Cache (Hintergrund-Laden)
  compile.rs     Compile-Pipeline, Expertenmodus, Spielprofile, Einstellungen
  commands.rs    Befehls-Registry, Tastenkürzel, Befehlspalette
  mcp.rs         MCP-Server (HTTP + stdio + ohne Fenster)
  tabs.rs        Tabs, Import/Export, MCP-Anbindung
  editor2.rs     Duplizieren, Flächen ziehen/extrudieren, Sperren, nodraw, Zwischenablage
  importexport.rs  .map-Import, OBJ-Export
  prefabs.rs     Prefab-Bibliothek: mitgelieferte Prefabs, Ordner, .ol/.rmf, Vorschau-Renderer
  prefab_ui.rs   Prefab-Fenster, Drag & Drop, Hammer-Import
  dialogs2.rs    Befehlspalette, Objektliste, Verlauf, Tastenkürzel-Editor
  merge.rs       Map zusammenführen (IDs, Namen, Visgroups, Warnungen)
  merge_ui.rs    Dialoge Zusammenführen / Vergleichen / 3-Wege
  diff.rs        Map-Stände vergleichen, 3-Wege-Zusammenführen (auch für Git)
  collab.rs      Live-Zusammenarbeit: Protokoll, Anmeldung, Abgleich pro Objekt, IPv4/IPv6
  netcrypt.rs    Verschlüsselung der Live-Sitzung (ChaCha20, HMAC-SHA256)
  collab_ui.rs   Fenster „Zusammenarbeit“, Anzeige der anderen Teilnehmer
  templates.rs   Vorlagen für neue Maps, Startseite
  theme.rs       Farbschemata Dunkel/Hell, Schriften, UI-Bausteine
  icons.rs       Symbole (SVG-Pfade aus dem UI-Konzept) mit egui gezeichnet
  chrome.rs      Obere Leiste, Tabs, Werkzeugleiste, Kontextleiste, Statusleiste
  optimize.rs    Automatische Optimierungen: Flutfüllung/Lecks, KI-Nodes, Netz-Simulation, func_detail, Areaportals …
  opt_ui.rs      Optimierungs-Fenster, Einbindung in den Compile-Ablauf
  overlay_mesh.rs  Overlays auf Flächen/Displacements projizieren
  material_editor.rs, vtf_write.rs   Material-Editor, PNG → VTF
  dmx.rs, particle_editor.rs         DMX/PCF lesen und schreiben, Partikel-Editor mit Vorschau
  gi.rs          Globale Beleuchtung (BVH, Sondenraster, Hintergrund-Berechnung)
  physics.rs, place_ui.rs            Physik-Platzierung
  scatter.rs     Vegetation streuen
  procgen.rs, procgen_ui.rs, tiles.rs  Generatoren und Tile-Sets
  mesh.rs, mesh_edit.rs, mesh_ui.rs   Polygon-Meshes (Daten, Bearbeitung, Fenster, studiomdl)
  patch.rs       Bezier-Patches auf Displacements
  particle_preview.rs  Partikel-Effekte im 3D-Fenster
  live.rs, live_ui.rs, assets/live/amboss_live.lua   Live-Abgleich mit dem Spiel (Lua-Brücke)
  walk.rs        Begehen-Modus (Spieler-Kollision wie in Source)
```

Tests: `cargo test` (VMF-Roundtrip, Geometrie, FGD, VPK, VTF, Materialien, Compile-Pipeline).
Die Beispiel-Map `examples/beispiel_raum.vmf` wird mit `cargo test write_example -- --ignored` neu erzeugt.

## Sprachen prüfen

Die Oberfläche gibt es auf Deutsch und Englisch (`assets/lang/en.tsv`). Zwei Hilfsskripte finden fehlende Übersetzungen:

* `python3 tools/i18n_check.py` – sucht im Quelltext deutsche Texte ohne Eintrag in der Tabelle.
* `AMBOSS_I18N_AUDIT=texte.txt amboss.exe` schreibt jeden gezeichneten Text in die Datei; `python3 tools/i18n_audit.py texte.txt` listet danach deutsche Reste der englischen Oberfläche.

## Noch nicht enthalten

Die vollständige, abgehakte Liste steht in `ROADMAP.md`. Die wichtigsten offenen Punkte:

- Editor-Sprites erscheinen nur, wenn die Spielinhalte `materials/editor/*.vmt` enthalten (sonst Kästchen)

## Lizenz

Copyright (c) 2026 Deadshot

Amboss steht wahlweise unter einer dieser beiden Lizenzen:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) bzw. <https://www.apache.org/licenses/LICENSE-2.0>)
* MIT-Lizenz ([LICENSE-MIT](LICENSE-MIT) bzw. <https://opensource.org/licenses/MIT>)

Die mitgelieferten Schriften stehen unter der SIL Open Font License 1.1:
Inter ([assets/fonts/Inter-OFL.txt](assets/fonts/Inter-OFL.txt)) und
JetBrains Mono ([assets/fonts/JetBrainsMono-OFL.txt](assets/fonts/JetBrainsMono-OFL.txt)).

### Beiträge

Sofern du nichts anderes angibst, steht jeder Beitrag, den du zur Aufnahme in Amboss
einreichst, wie oben unter MIT oder Apache-2.0 – ohne weitere Bedingungen (wie in
Abschnitt 5 der Apache-2.0-Lizenz beschrieben).

### Hinweis zu Valve

Amboss ist ein unabhängiges Projekt und nicht mit Valve Corporation oder Facepunch Studios
verbunden. „Source“, „Hammer“, „Half-Life“ und „Garry's Mod“ sind Marken ihrer jeweiligen
Inhaber. Amboss enthält keine Dateien von Valve; die Compile-Tools (VBSP, VVIS, VRAD) und
Spielinhalte kommen aus deiner eigenen Installation.

