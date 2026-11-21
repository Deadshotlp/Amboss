# Amboss – offene Punkte

Aufwand: **S** = Stunden · **M** = Tage · **L** = Wochen. Erledigtes wird mit `[x]` abgehakt.

## A. Zusammenarbeit und Zusammenführen (neu)

- [x] **Map zusammenführen** (Datei → Map zusammenführen…): Position wählen, doppelte Namen umbenennen, eigene Visgroup „Import: …“, Unterschiede der Map-Eigenschaften, Warnungen (Spawn, Sonne, Überlappungen), per MCP `merge_map` – S–M
- [x] **Map-Stände vergleichen** (was hat sich geändert, farbig) und **3-Wege-Zusammenführen** (Basis / meins / deins) mit Konfliktliste, auch als Git-Merge-Treiber – M–L
- [x] **Live-Zusammenarbeit** (zwei oder mehr Personen gleichzeitig): Gastgeber/Gast mit Passwort, Abgleich pro Objekt, eigene ID-Bereiche, ausgewählte Objekte für andere gesperrt, Kameras und Auswahl der anderen sichtbar, Rückgängig pro Person – L

- [x] Live-Sitzung über das Internet ohne Server: IPv6 direkt, Verschlüsselung – S–M
- [x] Live-Sitzung: Verbindungsaufbau ohne Portfreigabe (Hole-Punching) – ohne eigenen Vermittler: Verbindungscodes zum Austauschen + öffentliche STUN-Server, zuverlässiger Datenstrom über UDP; nicht bei symmetrischem NAT – M

## B. Rest aus Liste 1 (Hammer)

- [x] Displacement **Unterteilen** (Subdivide, Catmull-Clark) + Power ändern ohne Formverlust – M
- [x] **Sculpt-Werkzeuge**: Rauschen, Terrassen, Schärfen, Abfall weich/linear/hart (Strg kehrt Glätten ↔ Schärfen um) – L
- [x] **Ziehbare Griffe für Overlays** (3D: Ecken, Drehen in 15°-Schritten, Verschieben, Etikett) – M
- [x] **3D-Vorschau im Modell-Browser** (drehbar, Skins, Größe, Polygone) – M
- [x] **Abspielen im Sound-Browser** (Windows; Dauer, Fortschritt, Lautstärke) – S–M
- [x] **Partikel-Browser** (.pcf lesen, Kinder-Systeme, Manifest) – M
- [x] **Verschachtelte Instanzen**: Instanzen in Instanzen werden (bis 8 Ebenen, mit Kreis-Erkennung) angezeigt und beim Einklappen mit Namen/Parametern wie VBSP aufgelöst; Werkzeuge → „Instanz in Tab öffnen“, Speichern aktualisiert alle offenen Maps – M
- [x] **Lichtvorschau mit Schatten** (Sonne + die 4 nächsten Lichter, Schattenkarten werden nur bei Änderungen neu berechnet) – L

## C. Hammer++

- [x] Echtzeit-Lichtvorschau mit Schatten (Sonne, Punkt- und Spotlichter; ohne Umgebungsverdeckung) – L
- [x] **Partikel-Vorschau** im 3D-Fenster: `info_particle_system` spielt seinen Effekt ab (vereinfachte Simulation aus dem Partikel-Editor, Kind-Systeme mit Verzögerung, `$additive` wird aufaddiert, bis 3000 Einheiten um die Kamera, höchstens 16 Effekte; Ansicht → „Partikel-Effekte abspielen“). Nicht nachgebildet: Sprite-Sheets (Animationsbilder), Kontrollpunkte, Kollision – L
- [x] `env_sprite` im 3D-Fenster (echtes Sprite mit Größe, Farbe, Deckkraft) – S
- [x] Fade-Distanz ausgewählter Props als Kugeln (gelb/orange) – S
- [x] Props beleuchtet und Kollisionsmodell (.phy) im 3D-Fenster anzeigen (3D-Menü → Kollision) – M
- [x] **Einrasten an Eckpunkten** anderer Brushes (Alt+V) – S–M
- [x] **Overlays auf Displacements** korrekt darstellen: das Overlay wird wie im Spiel auf die Flächen aus `sides` projiziert, am Overlay-Rechteck zugeschnitten und folgt den Displacement-Dreiecken – M
- [x] Größere Map-Grenzen je Spielprofil (Einstellungen; Grenze wird in 2D rot angezeigt, Problemprüfung nutzt sie) – S
- [x] **Clip mit 3 Punkten**: im Clip-Werkzeug in 3D drei Punkte auf Flächen anklicken (Raster bzw. Eckpunkte), schräge Schnittebene mit Vorschau in 2D und 3D, Enter schneidet – S
- [x] **Nebel-Vorschau** aus `env_fog_controller` (fogcolor, fogstart/fogend, fogmaxdensity; Ansicht → „Nebel“) – S
- [x] **Seile** `move_rope`/`keyframe_rope` über NextKey mit Durchhang aus Slack, Breite und RopeMaterial; in 2D als Linie – S
- [x] **2D-Himmel** aus worldspawn `skyname` (sechs Seiten um die Kamera, hinter allem; Ansicht → „Himmel-Vorschau“) – S

## D. Liste 3 (schwer zu übernehmen)

- [x] Heightmap-Import für Terrain (PNG 8/16 Bit, RAW 16 Bit → nahtlose Displacement-Kacheln; Karte → „Gelände aus Heightmap…“, MCP `import_heightmap`), dazu ein eingebauter Rausch-Generator (Perlin in Lagen, Talboden, Terrassen, Bergkämme, Insel; Export als 16-Bit-PNG) – M
- [x] Brush → Modell (wie Propper): Werkzeuge → „Auswahl als Modell (Prop)…“ schreibt SMD/QC und VertexLitGeneric-Materialien, ruft studiomdl auf und ersetzt die Brushes durch prop_static/prop_physics (MCP `make_prop`) – M–L
- [x] Lua-Skript-Schnittstelle (Lua 5.4 eingebaut): Werkzeuge → „Lua-Skripte…“ mit Editor, Ausgabe und Skriptordner (`skripte/` neben amboss.exe, mit Beispielen); alle MCP-Werkzeuge über `amboss.call`, dazu Abkürzungen; ein Rückgängig-Schritt pro Skript, Zeitlimit; MCP `run_lua` – L
- [x] Navmesh selbst erzeugen: Karte → „Navmesh erzeugen“ (ab den Spawnpunkten, Stufen 18, Fallhöhe 200, geduckt 36, Brushes + Displacements), Vorschau in 2D/3D, `.nav` Version 16 in `maps/`, beim Kompilieren automatisch und mit eingepackt; MCP `generate_navmesh`. Noch ohne: Prop-Kollision, Leitern, Verstecke/`nav_analyze` – L
- [x] **Material-Editor** (Werkzeuge → „Material-Editor…“): Shader, Basistextur, Normalmap, Durchsichtigkeit, Spiegelung, Oberflächenart, Flags; PNG wird als VTF (mit Mipmaps) importiert; Live-Vorschau; speichert VMT/VTF ins Spielverzeichnis – L
- [x] **Partikel-Editor** (Werkzeuge → „Partikel-Editor…“, auch aus dem Partikel-Browser): .pcf (DMX binär) lesen und schreiben, Systeme aus Vorlagen anlegen, Emitter/Initialisierer/Operatoren/Renderer mit allen Attributen bearbeiten, animierte Vorschau, Partikel-Manifest – L
- [x] **Globale Beleuchtung in Echtzeit** (Lichtvorschau → „Globale Beleuchtung“): indirektes Licht mit Farbübertrag über ein Sondenraster (Spherical Harmonics), im Hintergrund in 16 Durchgängen verfeinert, startet nach Änderungen neu; Meshes werfen mit – L
- [x] **Platzierung mit Physik** (Objekt → „Mit Physik fallen lassen“): ausgewählte Props/Entities fallen auf Brushes, Displacements und aufeinander, Esc bricht ab (ein Rückgängig-Schritt) – L
- [x] **Vegetation streuen** (Werkzeuge → „Vegetation streuen…“): Pinsel in 3D mit Radius, Dichte, Abstand, Größe/Drehung/Neigung zufällig, an Oberfläche ausrichten, Shift radiert; „Auswahl füllen“ – L
- [x] **Prozedurale Generatoren** (Werkzeuge → „Prozedurale Generatoren…“): Labyrinth, Räume mit Gängen (mit Licht und Spawnpunkt, sofort kompilierbar; in 3D mit Raumhöhen, Bodenstufen und kurzen Treppen an den Türen, Gangnetz mit langen Geraden, runden Kurven und Kreuzungen ohne Doppelgänge, dünne Wände, eigene Texturen für Boden/Wände/Decke, jeder Raum und Gang eine eigene Gruppe), Stadtblock, Kreis-Anordnung der Auswahl – L
- [x] **Tile-Sets** (Werkzeuge → „Tile-Set…“): 9 Bausteine (Boden, Wand, Ecke, Sackgasse, Gang, Tür, Fenster, Säule, Treppe) auf einem Raster per Klick/Ziehen setzen, R dreht, Shift entfernt, Etagen – L
- [x] **Freie Polygon-Meshes** (Werkzeuge → „Polygon-Mesh…“): aus Brushes (auch Displacements) erstellen oder OBJ importieren; Punkte/Flächen in 3D ziehen, Rahmenauswahl, Extrudieren, Glätten, Unterteilen, Umdrehen, Löschen, Textur zuweisen; jedes Mesh ist ein prop_static, die Netzdaten liegen in `<map>.meshes.json`; „Zum Modell kompilieren“ über studiomdl – L
- [x] **Bezier-Patches** (Werkzeuge → „Bezier-Patch…“): 16 Kontrollpunkte biegen eine Displacement-Fläche glatt (Bögen, Wellen, Kuppeln); legt bei Bedarf ein Displacement an – L
- [x] **Im Spiel testen ab 3D-Kamera** (Shift+F9): kompiliert und setzt den Spieler an die Stelle der 3D-Kamera; läuft GMod schon, wird die Map dort neu geladen statt ein zweites Spiel zu starten – M
- [x] **Live-Abgleich mit dem Spiel** (Kompilieren → „Live-Abgleich mit dem Spiel…“): eine Lua-Brücke (`addons/amboss_live`) und Dateien in `garrysmod/data/amboss_live/`. Entities verschieben/ändern/neu/löschen wirkt sofort (über die Hammer-ID), neue oder geänderte Brushes erscheinen als begehbare Vorschau, Inputs auslösen, Spieler ↔ 3D-Kamera, Kamera folgt dem Spieler, Spieler in 2D/3D sichtbar. Braucht weiter Kompilieren: Lichter, gelöschte Brushes, neue Brush-Entities – L
- [x] **Begehen-Modus** (F6, „im Editor spielen“): mit Spielergröße durch die Map laufen – Schwerkraft, Kollision mit Brushes/Props/Displacements, Stufen bis 18, Sprung (33) und Ducksprung, Ducken, Rennen – M

- [x] **Map-Decompiler** (wie BSPSource): Datei → „BSP dekompilieren…“ / MCP `decompile_bsp` – Brushes mit Materialien und Texturachsen, func_detail, Brush-Entities (inkl. origin), Displacements, Entities, statische Props, Overlays (Seiten werden wiedergefunden), Cubemaps. Nicht möglich: Visgroups, Gruppen, Instanzen, Areaportal-/Occluder-Brushes – L
- [x] **BSP-Packing-Werkzeug** (wie AutoBSPpackingTool, eingebaut ohne bspzip): Kompilieren → „Inhalte in BSP packen…“ findet Materialien (inkl. Bumpmap/Envmap/Detail), Modelle (.vvd/.vtx/.phy + Skins), Sounds, Partikel (.pcf nach benutzten Systemen + deren Materialien), Soundscapes (+ Sounds), Nav-Mesh, Übersichtskarte; Liste bearbeitbar, Auswahl gilt auch beim Kompilieren; MCP `pack_bsp` – M

## E. Liste 4 (allgemeine Lücken)

Darstellung
- [x] Blend-Texturen (WorldVertexTransition) gemischt anzeigen – M
- [x] `$basetexturetransform`, animierte Texturen (AnimatedTexture), TextureScroll – M
- [x] MSAA (Kantenglättung, unter Einstellungen → Grafik) – S
- [x] Frustum-Culling (Geometrie in 1024er-Blöcken) – M
- [x] 3D-Mesh bei Änderungen nur teilweise neu bauen (unveränderte Brushes aus dem Zwischenspeicher) – M

Auswahl und Bearbeitung
- [x] In 2D nach Kanten statt Bounding Box wählen (Kante/Mittelpunkt hat Vorrang) – S–M
- [x] Rahmenauswahl in 3D – S
- [x] „Alles Berührende auswählen“ (Strg+Alt+A, Alt beim Rahmen) – S
- [x] Raster-Einrasten abschaltbar (Alt+G) – S

Entities
- [x] Eigenschaften zwischen Entities kopieren (Strg+Alt+C / V) – S

Daten
- [x] Rückgängig speichert Änderungen statt ganzer Map – M
- [x] Absturz-Wiederherstellung (Startseite bietet automatisch gesicherte Stände an) – S

Oberfläche
- [x] Helles Design + neue Oberfläche nach UI-Konzept (Top-Bar, Werkzeugleiste, Kontextleiste, Panel, Startseite) – S
- [x] Panel-Layout speichern: Ansichten-Teilung ziehbar, rechtes Panel in der Breite ziehbar, Achsen und 3D-Optionen bleiben erhalten; Ansicht → „Layout zurücksetzen“ – S
- [x] Startbildschirm und Vorlagen für neue Maps – S
- [x] **Sprache des Editors wählbar** (Einstellungen → Grafik → Sprache / Language): Deutsch und Englisch, Umschalten ohne Neustart; Texte in `assets/lang/en.tsv` – M
- [x] Weitere Sprachen: Französisch, Spanisch, Polnisch, Russisch (Oberfläche; Entity-Beschreibungen fallen auf Englisch zurück), beim ersten Start Systemsprache – S je Sprache
- [ ] Entity-Beschreibungen (`en_fgd.tsv`) in FR/ES/PL/RU; Übersetzungen von Muttersprachlern prüfen lassen
- [x] Englische Beschreibungen für die eingebaute Entity-Liste: 456 Klassen, Schlüssel, Auswahlwerte, Flags, Ein-/Ausgänge (`assets/lang/en_fgd.tsv`, ~1000 Texte) – S

Inhalte
- [x] Assets nur aus dem Spielverzeichnis laden (Addons abschaltbar) – S
- [x] Beim Kompilieren Liste der benötigten Workshop-Addons erstellen (`<map>_addons.txt`) – S–M

Plattform
- [x] Installer (NSIS, ohne Adminrechte), Updates (GitHub-Releases beim Start, abschaltbar, Austausch der exe), macOS-/Linux-Build per GitHub Actions – M
- [ ] macOS-Build auf echtem Mac prüfen; Signatur/Notarisierung
- [x] Hohe DPI-Skalierung geprüft (150 %/200 %), Oberflächengröße einstellbar, Dialoge passen sich der Fenstergröße an – S

## F. Prüfung (wichtigste offene Aufgabe)

- [x] Unter Windows starten (läuft beim Nutzer; Start- und Ladezeit verbessert)
- [ ] Mit echtem GMod / Source SDK Base 2013, echter `garrysmod.fgd` und echtem VBSP/VVIS/VRAD testen
- [ ] Displacement-Ausrichtung, bspzip, `nav_generate`, automatische Areaportals und Grenzwerte der Optimierungen prüfen

## G. Rest aus dem UI-Konzept

- [x] Entity-Klassen-Auswahl (Popover mit Suche, Favoriten, Gruppen) und Dialog „In Entity umwandeln“
- [x] Alle HL2/GMod-Entities eingebaut (456 Klassen), echte garrysmod.fgd wird automatisch gefunden
- [x] Ziehbare Overlay-Griffe nach Entwurf

- [x] Konfliktauswahl „Meins / Andere“ bei Live-Zusammenarbeit – S–M
- [x] Vorschau im Dialog „Inhalte einfügen“ – S
- [x] Befehlspalette (Strg+K) im neuen Stil (Befehle, Entities, Texturen) – S
- [x] Innenleben der übrigen Dialoge ans neue Muster anpassen – S–M

## H. Bedienung für Einsteiger (neu)

- [x] Dialog „Kompilieren & Exportieren“ mit BSP-Packen (Dateiliste aufklappbar) und Navmesh als Haken; eigener Pack-Dialog entfällt
- [x] Öffnen zeigt auch .bsp und startet dafür den Dekompilier-Dialog
- [x] Menüs neu geordnet: Datei · Bearbeiten · Ansicht · Objekt · Werkzeuge · Map · Kompilieren · Hilfe (Untermenüs, ausgegraut ohne Auswahl)
- [x] Einfache / Experten-Oberfläche
- [x] Einrichtungs-Assistent beim ersten Start
- [x] Tutorial mit fünf Kapiteln (prüft Schritte selbst, „Zeig es mir“, Hervorhebung)

## I. UI-Prüfung (zweite Claude-Instanz, 81 Elemente in DE und EN)

Erledigt:
- [x] Panelbreite sprachunabhängig (Standard 360 px, ziehbar 300–560, gespeichert; altes egui-Gedächtnis wird beim Start ignoriert)
- [x] Deutsche Reste in der englischen Oberfläche: rund 100 Laufzeit-Meldungen übersetzt (Probleme, Compile-Log, Live-Sitzung, Zusammenführen, Addon-Liste …); Prüfwerkzeuge `tools/i18n_check.py` (Quelltext) und `tools/i18n_audit.py` (alle gezeichneten Texte, `AMBOSS_I18N_AUDIT=datei`)
- [x] Echte Einzahl/Mehrzahl statt „(e)/(s)“ in Panel, Statusleiste, Dialogen (`i18n::count`)
- [x] Dezimalkomma in DE (Zoom, Zahlenfelder, Schieberegler, Dateigrößen in KB/MB)
- [x] Tastennamen einheitlich („Shift“), Befehlspalette Strg+K, In Welt Strg+Shift+W, Kompilieren-Dialog Strg+F9, Ausschneiden im Menü, Abstand Text–Kürzel 24 px, „…“ ohne Leerzeichen
- [x] Begriffe: Map-Info, Live-Sitzung, Inhalte einfügen, Werkzeugnamen (Textur, Displacement)
- [x] Dialoge: feste Breite, nicht skalierbar, Trennlinien über volle Breite, erste Position mittig
- [x] Ein Checkbox-Stil (eckiges Kästchen), Platzhalter schwächer, Suchfelder mit Lupe
- [x] Panel: Beschriftungen 40 % breit mit bis zu zwei Zeilen, Wertfelder über die ganze Spalte, Drehen-Reihe, Ausgänge mit Tooltips und Warnsymbol
- [x] Entity-Werkzeug ohne alte Klassenliste; In Entity umwandeln nur mit Brush-Klassen, Vorauswahl func_detail
- [x] Mehrfachauswahl mit FGD-Feldern und „(verschieden)“
- [x] Assistent: Auswahlkarten, feste Größe, „Fast fertig – n Punkte fehlen“, Überspringen links
- [x] Hilfe-Dialog mit Suche und fester Tastenspalte; Einstellungen mit linker Navigation; Displacement-Panel neu; Lua-Dialog breiter, englische Beispiele; Entity-Report mit Spaltenköpfen; Über Amboss; Statusmeldung verschwindet nach 8 s

Ebenfalls erledigt (0.10.0):
- [x] Startbildschirm: Status-Punkte klickbar („Einrichten…“), Mini-Draufsicht in „Zuletzt geöffnet“
- [x] Kontextleiste: feste Reihenfolge beim Wegfallen von Beschriftungen; je Werkzeug nur relevante Elemente (Lupe/Kamera ohne Textur/Raster)
- [x] Panel: Schalter „Rohwerte“, Texturbereich einklappbar/höhenverstellbar, im Einfach-Modus nur wichtige Schlüssel + „Alle Eigenschaften (n)“
- [x] Brush-Panel mit Größe, Materialien und Schnellknöpfen; BrightnessHDR −1 als „wie Brightness“; P/Y/R als Zahlenfelder mit farbigem Präfix
- [x] Objekte: Auge/Schloss je Zeile, Gruppen als Knoten; Visgroups als senkrechte Liste mit Farbe und Anzahl
- [x] Kamera löschen/umbenennen, Block-Form nur in der Kontextleiste, Messergebnis im Panel mit Kopieren
- [x] Textur-Browser Kachelgröße, Texturen ersetzen mit Vorschau und Trefferzahl, Transformieren mit Drehpunktwahl, Gehe zu mit Brush/Seite/Entity
- [x] Modell-Browser echte Kachelvorschau; logische Ansicht mit Leerzustand und Suche; Live-Sitzung als Segment Gastgeber/Beitreten; Entity-Hilfe als Tabelle
- [x] Icons je Entity-Kategorie; Pfadfelder in den Einstellungen mit Status-Punkt


## J. Ideen für weitere Funktionen (noch nicht eingeplant)

Vorschläge vom 18.11.2026; die Reihenfolge innerhalb eines Abschnitts sagt nichts über die Priorität. ★ = Favoriten.

Ansicht und Übersicht
- [x] ★ **Werkzeugflächen einzeln ausblenden** (0.11.0) (wie Hammers Auto-Visgroups): eigene Schalter für nodraw, Trigger, Clips (player/npc/clip), Hint/Skip, Areaportals/Occluder, Skybox-Flächen, dazu Brush-Entities nach Klasse, Punkt-Entities, Props, Displacements; wirkt in 2D und 3D, ausgeblendete Objekte sind nicht anklickbar, Zustand bleibt pro Map gespeichert – S
- [x] ★ **Eigener Bereich für die 3D-Skybox** (0.11.0; Bearbeiten im Raum mit Umrechnung über Kopieren, Raster und Weltkoordinaten in der Statuszeile) (Ansicht → „3D-Skybox bearbeiten“): Skybox-Raum isoliert anzeigen und im Weltmaßstab bearbeiten (Objekte werden mit `scale` der sky_camera umgerechnet), Vorschau aus Spielersicht mit der Map im Vordergrund nebeneinander, Assistent legt Skybox-Raum + sky_camera an, Teile der Map verkleinert in die Skybox kopieren, Prüfung (sky_camera fehlt/doppelt, Nebel der sky_camera, skyname passt, Raum dicht). Die skalierte Darstellung im 3D-Fenster gibt es schon – M

Maps aus anderen Engines
- [x] Ganze Maps aus Hammer 3.x (`.rmf`) und `.map` öffnen (0.13.0; `.jmf` über Speichern als `.rmf` in J.A.C.K.) (`.rmf`-Leser gibt es für Prefabs schon) – S–M
- [ ] GoldSrc-BSP (Half-Life 1) dekompilieren, WAD-Texturen → VTF – M
- [ ] Quake-3-Maps inkl. gebogener Flächen (→ Bezier-Patches) – M
- [ ] Source-2-Maps (`.vmap`) – nur eingeschränkt: Meshes → Props, konvexe Teile → Brushes – L
- [ ] Szenen aus Unreal/Unity/Godot/Blender über glTF/FBX/OBJ (Geometrie → Props, Blockformen → Brushes per konvexer Zerlegung) – M–L
- [ ] OpenStreetMap-Gebäude und Straßen (zusammen mit dem Heightmap-Import) – M

Modelle
- [x] ★ glTF/GLB und OBJ per Ziehen in die Map (0.12.0; FBX/DAE noch offen): automatisch SMD/QC + VTF/VMT, studiomdl im Hintergrund, fertiges prop_static/prop_physics – M
- [ ] Kollisionsmodell automatisch (konvexe Hülle gibt es beim Modell-Import seit 0.12.0; Zerlegung in mehrere konvexe Teile noch offen) – M
- [ ] LODs automatisch erzeugen – M
- [x] Skins und Bodygroups im Import-Dialog (0.16.0) – S–M
- [x] Animationen im Modell-Browser abspielen (0.17.0) – M
- [ ] MDL dekompilieren und als glTF/OBJ exportieren (wie Crowbar) – M

Texturen und Materialien
- [ ] Textur-Editor mit Ebenen (Pinsel, Füllen, Stempel, Farbe/Helligkeit, Zuschneiden, Kachelvorschau) – L
- [x] ★ „Nahtlos machen“ mit einem Klick (0.12.0) – S
- [x] ★ Normalmap aus Höhen- oder Farbbild (0.12.0) – S–M
- [x] ★ PBR-Texturen (Roughness/Metallic) in Source-Materialien umwandeln (0.12.0) (Phong, Envmap-Maske) – M
- [x] Decal-/Overlay-Generator aus einem Bild (0.14.0; Werkzeuge → „Decal/Overlay aus Bild…“, Größe in Einheiten über `$decalscale`) – S
- [ ] Texturen in 3D direkt auf Flächen malen – L
- [x] Freie Textur-Bibliotheken (Poly Haven, ambientCG) durchsuchen und als VTF einbinden (0.17.0) – M

Bauwerkzeuge
- [x] ★ Spline-Werkzeug (0.12.0): Straßen, Schienen, Rohre, Zäune, Kabel entlang einer Kurve – M
- [ ] Saubere Boolesche Operationen (Vereinigen, Abziehen ohne Zerstückelung) – M
- [x] Array: entlang Linie (Inhalte einfügen), Kreis (Generatoren) und Kurve (Spline → „Auswahl entlang“, 0.13.0) – S
- [x] Wendeltreppen, Kuppeln, Röhren, Torbögen als eigene Formen (gibt es im Block-Werkzeug: Wendeltreppe, Kuppel, Torus/Rohr, Bogen) – S
- [x] Gameplay-Maße einblenden (0.14.0; Spielerfigur mit Stufe, Sprung, Ducken, Augenhöhe, Ducksprung; in 3D grün/gelb/rot, ob der Spieler passt; Durchgangs- und Türbreiten; Sichtlinien noch offen) – S
- [x] Weitere Generatoren: Höhlen, Innenausbau (Möbel streuen) (0.16.0) – M

Optimierung und Fehlersuche
- [x] Lightmap-Dichte als Farbkarte mit geschätzter Dateigröße (0.13.0) – S
- [ ] Sichtbarkeit ohne Kompilieren simulieren („Was sieht das Spiel von hier?“) – M–L
- [ ] Performance-Ampel (Draw-Calls, Props, Entities pro Bereich) – M
- [x] Compile-Fehler erklären: Meldungen klickbar, springt zum Brush, mit Lösungsvorschlag (0.13.0) – S–M

GMod
- [x] ★ Workshop-Upload aus Amboss (0.12.0) (gmad + gmpublish) mit Addon-Liste – M
- [x] ★ Workshop-Vorschaubild aus der 3D-Kamera rendern (0.12.0) – S
- [x] Gamemode-Vorlagen mit Checkliste (TTT, Murder, Prop Hunt, DarkRP, Deathrun) (0.15.0; Map → Gamemode-Checkliste, Vorlagen unter Neu aus Vorlage) – S–M
- [x] Lua-Entities aus Addons erkennen (Scripted Entities ohne FGD) (0.15.0; lua/entities, lua/weapons, gamemodes/*/entities, auch in .gma) – M
- [ ] Visueller Logik-Editor: Ein-/Ausgänge als Knoten verbinden – M

Zusammenarbeit und Arbeitsablauf
- [x] Patchnotes nach einem Update einmal anzeigen, Hilfe → Neuigkeiten (0.13.0)
- [x] Notizen/Kommentare an Stellen in der Map anheften (0.14.0; als info_null, beim Kompilieren entfernt, in der Live-Sitzung geteilt) – S
- [x] Textchat in der Live-Sitzung (gab es schon) – S
- [x] Zeitleiste der Speicherstände mit Vorschau und Vergleich (0.16.0) – S–M
- [ ] Plugin-Verwaltung für Lua-Skripte – M

Export und Präsentation
- [x] glTF-Export mit Texturen (0.15.0; .glb mit PNG-Texturen, Meter, Y oben; Props/Modelle noch nicht) – S–M
- [x] Hochauflösende Screenshots und 360°-Panoramen (0.14.0; bis 7680 × 4320 mit Kantenglättung, Panorama bis 8192 × 4096) – S
- [ ] Kamerafahrt als Video – M
