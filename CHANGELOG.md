# Änderungen

## 0.16.0

**Modell-Import: Bodygroups und Skins**
- Im Import-Dialog unter „Teile und Bodygroups“: Teile des Modells (Objekte/Gruppen einer OBJ-Datei, Knoten eines glTF) lassen sich zu Bodygroups zusammenfassen – jedes Teil wird eine wählbare Variante, auf Wunsch auch „nichts“; alle anderen Teile bilden den festen Körper
- Unter „Skins“: weitere Skins tauschen die Bilder einzelner Materialien; Amboss schreibt dafür eigene VTF/VMT und `$texturegroup` in die QC (in GMod über den Entity-Schlüssel `skin`, Bodygroups über `body` bzw. SetBodygroup)
- Behoben: Bei langem Inhalt verschwanden die Knöpfe unten im Import-Dialog

**Zeitleiste der Speicherstände**
- Jedes Speichern legt zusätzlich eine Kopie mit Zeitstempel unter `autosave/verlauf/<map>/` ab (Anzahl unter Einstellungen → Autosave, Standard 30)
- Datei → „Zeitleiste der Speicherstände…“: Verlauf, Autosaves und die gespeicherte Datei auf einer Zeitleiste; je Stand eine Draufsicht, in der markiert ist, was seitdem neu (grün), gelöscht (rot) oder geändert (gelb) ist
- „Unterschiede in der Map zeigen“, „In neuem Tab öffnen“ und „Wiederherstellen“ (Strg+Z nimmt es zurück)

**Generatoren: Höhle und Innenausbau**
- Werkzeuge → Prozedurale Generatoren → „Höhle“: natürliche Höhle aus einem Zellautomaten – Felswände, unebener Boden und unebene Decke als Displacements (nahtlos über die Kacheln), Lichter, Spawnpunkt und Cubemap; abgedichtet mit nodraw-Platten, weil Displacements nicht abdichten
- „Innenausbau“: Möbel in den ausgewählten Raum streuen – Stil Wohnung, Büro oder Lager; Schränke und Regale mit dem Rücken an die Wand, Tische mit Stühlen in die Mitte (im Büro Schreibtisch mit Stuhl und Bildschirm), Lampen und Fässer in die Ecken; keine Überschneidungen mit Wänden oder untereinander, Platz zum Laufen bleibt frei; mit Spielinhalten werden nur vorhandene Modelle und ihre echte Größe benutzt

## 0.15.0

**Gamemode-Vorlagen und -Checkliste**
- Map → „Gamemode-Checkliste…“ für TTT, Murder, Prop Hunt, DarkRP und Deathrun: zeigt, was die Map noch braucht – Spawnpunkte (je Team), Waffen und Munition, Physik-Props, Türen, Fallen-Knöpfe, Schadens-Bereiche – mit grünem Haken, rotem Kreuz oder „empfohlen“ und der Anzahl
- „Hinzufügen“ setzt die fehlenden Entities dort ab, wohin die 3D-Kamera schaut (auf den Boden, als ein Rückgängig-Schritt); Props bekommen gleich Alltags-Modelle aus HL2
- Hinweis auf das übliche Map-Präfix (ttt_, ph_, rp_, deathrun_ …) und Tipps je Gamemode; der gewählte Gamemode wird mit der Map gespeichert
- Neue Vorlagen unter „Neu aus Vorlage“: TTT, Murder, Prop Hunt, DarkRP (Häuser mit Türen) und Deathrun (Grube mit trigger_hurt, Fallen-Knöpfe) – erfüllen ihre Checkliste
- Die eingebaute Entity-Liste kennt jetzt die TTT-Entities (ttt_random_weapon, ttt_random_ammo, ttt_map_settings, ttt_traitor_button, ttt_logic_role, ttt_win, ttt_traitor_check, TTT-Waffen und -Munition) – 476 Klassen

**Lua-Entities aus Addons erkennen**
- Scripted Entities und Waffen ohne FGD erscheinen in der Entity-Liste: aus `lua/entities`, `lua/weapons` und `gamemodes/*/entities` – im Spielordner, in addons-Ordnern und in Workshop-Archiven (.gma)
- Mit Anzeigename, Kategorie und Herkunft (Addon/Gamemode), Modell (`ENT.Model`, `SWEP.WorldModel` oder `SetModel`), Schlüsseln aus `ENT:KeyValue`, Eingängen aus `ENT:AcceptInput` und Ausgängen aus `TriggerOutput`
- Eine echte FGD hat immer Vorrang; abschaltbar unter Einstellungen → Spiel & Pfade

**glTF-Export mit Texturen**
- Datei → Exportieren → „Exportieren (.glb, glTF mit Texturen)“: die ganze Map oder nur die Auswahl als eine .glb-Datei mit eingebetteten PNG-Texturen – in Metern und mit Y nach oben, für Blender, Unity, Unreal und Godot
- Brushes und Displacements; Werkzeug-Texturen werden weggelassen, durchsichtige Materialien als transparent markiert (Props/Modelle noch nicht)

## 0.14.1

**Schneller bei großen Auswahlen**
- Viele ausgewählte Objekte bremsen nicht mehr: Mit 1600 ausgewählten Brushes brauchte ein Bild in der Testmessung 142 ms (unter 10 FPS), jetzt etwa 28 ms. Die Rechenzeit von Amboss selbst sank von 18 auf 3 ms pro Bild
- Die 2D-Ansichten zeichnen große Auswahlen jetzt über die Grafikkarte statt jede Kante einzeln; beim Verschieben wird nur noch versetzt, nicht neu aufgebaut
- Auswahlgrenzen, Zählen, Verschieben, Drehen und Skalieren gehen bei großen Auswahlen in einem Durchgang über die Map statt jedes Objekt einzeln zu suchen; reines Verschieben rechnet die Brush-Geometrie nicht mehr neu
- Behoben: Das Ändern eines Brushes in einer Brush-Entity (und „Texturen ersetzen“ sowie das Vereinfachen flacher Displacements) hat alle übrigen Entities unnötig kopiert und als geändert markiert – mehr Speicher für Rückgängig und mehr Daten in der Live-Sitzung

## 0.14.0

**Gameplay-Maße**
- Ansicht → „Gameplay-Maße“ (Alt+M): Eine Spielerfigur in Originalgröße folgt der Maus – mit Stufenhöhe 18, Sprung ca. 33, geduckt 36, Augenhöhe 64 und Ducksprung ca. 69 (Werte wie in Garry's Mod: Sprungkraft 200, Schwerkraft 600)
- In 3D steht die Figur auf dem Boden unter der Maus (an Wänden davor) und zeigt farbig, ob der Spieler dort hinpasst: grün = stehend, gelb = nur geduckt, rot = zu eng. Trigger, Hint/Skip und andere durchlässige Brushes zählen nicht
- In den Seitenansichten die Höhenmarken mit Beschriftung, von oben 32 × 32 mit Hinweis auf Durchgangs- und Türbreiten

**Notizen in der Map**
- Map → „Notiz anheften“ (Strg+Shift+N): eine Notiz über der Auswahl oder dort, wohin die 3D-Kamera schaut; Text rechts im Panel. In 2D und 3D eine gelbe Nadel mit der ersten Zeile, ausgewählt der ganze Text
- Map → „Notizen…“ listet alle Notizen; ein Klick springt hin
- Gespeichert als `info_null` mit dem Schlüssel `amboss_note`: VBSP entfernt sie beim Kompilieren, Hammer öffnet die Map weiterhin, und in der Live-Sitzung sehen alle die Notizen

**Decal/Overlay aus einem Bild**
- Werkzeuge → „Decal/Overlay aus Bild…“ (auch unter Datei → Importieren und als Knopf im Decal- und Overlay-Werkzeug): PNG mit Transparenz oder JPEG wird zum fertigen Decal-Material (VTF + VMT mit `$decal`, `$decalscale` und bei Bedarf `$translucent`) unter `materials/amboss/decals/`
- Größe in Einheiten einstellbar (Höhe folgt dem Seitenverhältnis), durchsichtige Ränder werden auf Wunsch abgeschnitten, Texturgröße bis 2048
- Danach ist das Decal- bzw. Overlay-Werkzeug mit dem neuen Material aktiv; Decals und neue Overlays haben in 3D die Größe aus `$decalscale`

**Screenshots in hoher Auflösung und 360°-Panoramen**
- Datei → „Screenshot in hoher Auflösung…“ (Strg+F12): die 3D-Ansicht als PNG bis 7680 × 4320, auf Wunsch doppelt gerendert und verkleinert (glatte Kanten)
- 360°-Panorama vom Standort der Kamera (equirektangulär, bis 8192 × 4096) – für Panorama-Betrachter, VR oder als Workshop-Vorschau
- Ohne Raster, Hilfslinien und Auswahlrahmen; Darstellungsart wie in der 3D-Ansicht

**Kleinigkeiten**
- Die Beschriftungen „Segmente“ und „Rohrdicke“ im Formen-Panel sind jetzt übersetzt

## 0.13.1

- **Textur-Werkzeug: Linksklick nimmt die Textur auf**: Ein Linksklick auf eine Fläche übernimmt jetzt nur ihre Textur als aktuelle Textur (Pipette) und wählt nichts aus – damit lässt sich sofort per Rechtsklick weitermalen. Flächen auswählen (zum Ausrichten, Skalieren, für Displacements) geht mit **Strg+Klick**, auch mehrere; ein Klick ins Leere hebt die Flächenauswahl auf
- **Kompilieren & Exportieren**: Die Leisten „Sichtbarkeit · VVIS“ und „Licht · VRAD“ liegen jetzt auf einer Höhe (die HDR-Checkbox neben „Licht“ hatte die rechte Leiste nach unten geschoben)
- **Mehrere Entities bearbeiten**: Beim Feld „World Model“ (und anderen Modellfeldern) gibt es jetzt auch bei Mehrfachauswahl den Knopf für den Modell-Browser – „Übernehmen“ setzt das Modell bei allen ausgewählten Props
- Auswahllisten bei Mehrfachauswahl zeigen den Standardwert statt eines leeren Felds, wenn der Schlüssel bei allen nicht gesetzt ist

## 0.13.0

**Neu in Amboss nach einem Update**
- Beim ersten Start nach einem Update zeigt Amboss einmal die Patchnotes aller Versionen seit der zuletzt benutzten – zum Durchlesen der neuen Funktionen. Jederzeit wieder über Hilfe → „Neuigkeiten (Patchnotes)…“
- Auf Deutsch aus dem eingebauten Changelog, in den anderen Sprachen die englischen Release-Texte von GitHub (ohne Internet die deutschen)

**Kompilieren: Fehlermeldungen verstehen**
- Der Dialog „Kompilieren & Exportieren“ erkennt bekannte Meldungen von VBSP, VVIS und VRAD und erklärt sie unter „Was bedeutet das?“ in einfachen Worten mit Lösung: Leck, überschrittene Engine-Grenzen, Objekte außerhalb der Map, fehlende Texturen und Modelle, Modelle, die nicht als prop_static gehen, gemischte Brush-Inhalte, kaputte Brushes, Areaportals, Overlays, fehlende Compile-Programme, Speichermangel
- Dazu passende Knöpfe: Leck-Pfad laden, zum Objekt springen, Problemprüfung, Texturen ersetzen, Einstellungen

**Maps aus anderen Editoren**
- Ganze Maps aus Hammer 3.x / Worldcraft (`.rmf`) und Quake/Valve-`.map` lassen sich direkt öffnen (Datei → Öffnen, Drag & Drop, Datei → Importieren); `skyname` und andere Map-Eigenschaften werden übernommen. Texturen aus GoldSrc-WADs gibt es in Source nicht – die Statuszeile weist darauf hin, „Texturen ersetzen“ tauscht sie aus
- J.A.C.K.-Maps (`.jmf`): in J.A.C.K. als `.rmf` oder `.map` speichern, dann öffnen (Amboss sagt das beim Öffnen einer `.jmf`)

**Spline-Werkzeug: Auswahl entlang der Kurve**
- Neue Art „Auswahl entlang“: beliebige Objekte (Brushes, Entities, Prefabs) als Vorlage übernehmen und im Abstand entlang der Kurve kopieren, an der Kurve ausgerichtet, seitlich versetzt oder beidseitig

**Lightmap-Dichte**
- Die 3D-Ansicht „Lightmap-Raster“ heißt jetzt „Lightmap-Dichte“ und färbt die Flächen nach ihrer Lightmap-Auflösung (rot = fein 4, grün = Standard 16, blau = grob 64); unten rechts steht die geschätzte Größe der Lichtdaten (Luxel und MB, mit HDR)

## 0.12.1

- **Texturen nahtlos angleichen**: Mehrere Brushes, die zusammen eine Fläche bilden (z. B. ein Boden aus verschieden großen Platten), zeigen beim Texturieren keine Kanten mehr. Alle gleich ausgerichteten Flächen der Auswahl (z. B. alle Oberseiten) bekommen Achsen, Verschiebung, Skalierung und Drehung der größten von ihnen; weil Source die Textur über die Weltkoordinaten legt, läuft sie dann durch. Im Brush-Panel („Texturen nahtlos“), im Textur-Werkzeug („Nahtlos angleichen“, nur ausgewählte Flächen), unter Objekt → Brush-Operationen und mit **Strg+Shift+A**; ein Rückgängig-Schritt. Werkzeug-Texturen bleiben unverändert

## 0.12.0

**Modelle importieren (glTF, OBJ)**
- Datei → Importieren → „Modell importieren (glTF, OBJ)…“ oder die Datei einfach ins Fenster ziehen: `.glb`/`.gltf` (auch mit eingebetteten Daten und Knoten-Transformationen) und `.obj` mit `.mtl` werden zum fertigen prop_static oder prop_physics
- Einheit (Meter, Zentimeter, Zoll/Source-Einheiten oder frei), Hochachse (Y wie Blender/glTF oder Z) und Drehung einstellbar; der Dialog zeigt Größe in Einheiten, Höhe in Metern, eine Vorschau von vorn und oben und je Material, ob die Textur gefunden wurde
- Texturen (PNG, JPEG) werden zu VTF, Materialien zu VertexLitGeneric (Transparenz, Alpha-Test, beidseitig, Grundfarbe aus glTF/MTL); Kollision als konvexe Hülle; studiomdl kompiliert im Hintergrund, danach steht das Prop dort, wohin die 3D-Kamera schaut
- MCP: `import_model`, `get_import_status`. FBX und DAE noch nicht (Hinweis: aus Blender als glTF exportieren)

**Texturen im Material-Editor**
- „PBR-Set importieren…“: Bilder eines PBR-Materials (Poly Haven, ambientCG …) werden am Dateinamen erkannt – Grundfarbe × AO wird zur Basistextur, die Normalmap wird ins Source-Format (DirectX) gedreht oder aus der Höhe erzeugt, Rauheit und Metall werden zur Glanzmaske (Spiegelung bei Brush-Materialien, Phong bei Modellen)
- „Nahtlos machen“: Ränder der Basistextur und Normalmap werden überblendet, damit beim Kacheln keine Kanten sichtbar sind
- „Normalmap aus Basistextur“ mit einstellbarer Stärke
- Bilder können jetzt auch JPEG sein

**Im Workshop veröffentlichen**
- Kompilieren → „Im Workshop veröffentlichen…“: legt den Addon-Ordner an (BSP, auf Wunsch Navmesh und Bild fürs Spawnmenü, `addon.json` mit Typ und bis zu zwei Tags), packt ihn mit gmad und lädt ihn mit gmpublish hoch – neu oder als Update mit Änderungsnotiz
- Vorschaubild mit einem Klick aus der 3D-Ansicht (ohne Beschriftungen, 512 × 512 JPEG) oder aus einer Bilddatei
- Die Workshop-ID wird aus der Ausgabe übernommen und je Map gemerkt; „Seite öffnen“ führt zur Workshop-Seite

**Spline-Werkzeug**
- Werkzeuge → „Spline: Straße, Rohr, Zaun, Kabel…“: Punkte per Klick in der 3D-Ansicht (auf die Fläche darunter) oder in 2D setzen, als glatte Kurve oder gerade Strecken, offen oder geschlossen
- Straße/Weg mit Bordsteinen (die Textur der Fahrbahn läuft entlang der Kurve), Rohr (4–32 Seiten), Zaun/Geländer aus Pfosten und Latten, Modelle im Abstand (ausgerichtet, seitlich versetzt, beidseitig – z. B. Laternen), Kabel als move_rope/keyframe_rope-Kette
- Vorschau in allen Ansichten, ein Rückgängig-Schritt, Ergebnis als Gruppe und auf Wunsch func_detail; „Neu erzeugen“ ersetzt das zuletzt Erzeugte
- MCP: `build_spline`

## 0.11.0

**Ansichtsfilter („Einblenden“)**
- Objektarten einzeln aus- und einblenden, in 2D und 3D: Welt-Brushes, func_detail, Brush-Entities, **Trigger**, **nodraw-Flächen**, Clips (clip/playerclip/npcclip, func_clip_vphysics), Hint/Skip, Areaportals & Occluder, Skybox-Flächen, sonstige Werkzeug-Brushes, Displacements, Punkt-Entities, Lichter, Props und der 3D-Skybox-Raum
- nodraw- und Skybox-Flächen verschwinden in 3D **einzeln** – auch auf normalen Brushes; ein Klick geht durch die ausgeblendete Fläche hindurch auf das, was dahinter zu sehen ist. In 2D werden Brushes ausgeblendet, die nur aus nodraw bestehen
- Erkennung nach Textur (tools/toolstrigger, toolsclip, toolsskip …) und nach Entity-Klasse (trigger_*, func_areaportal, func_occluder …)
- Erreichbar über den Trichter-Knopf in jeder Ansicht, Ansicht → Einblenden (Filter), den Reiter Visgroups und die Befehlspalette; Tastenkürzel **Alt+N** (nodraw) und **Alt+T** (Trigger), frei belegbar; „Alle Werkzeug-Brushes ein-/ausblenden“ als Befehl
- Ist etwas ausgeblendet, leuchtet der Trichter, und unten steht „Filter aktiv“ (Klick blendet alles ein). Ausgeblendete Objekte sind nicht anklickbar und werden abgewählt
- Der Zustand wird pro Map in der VMF gespeichert (`viewsettings`, Schlüssel `amboss_hidden`; Hammer ignoriert ihn)

**3D-Skybox bearbeiten**
- Ansicht → „3D-Skybox bearbeiten“ bzw. Map → 3D-Skybox: Alle Ansichten zeigen nur noch den Raum um die sky_camera; das 3D-Fenster blickt hinein, der 2D-Himmel ist dahinter zu sehen
- **Spielersicht** als Bild im Bild unten rechts im 3D-Fenster: die ganze Map aus der vorherigen Kamera mit der vergrößerten 3D-Skybox dahinter – Änderungen im Raum sind sofort zu sehen. Ziehen dreht die Sicht, das Mausrad bewegt sie
- In 2D zeigt ein gestrichelter Rahmen, wo die Map im Skybox-Maßstab liegt (mit Spawnpunkten); die Statuszeile zeigt zur Mausposition die passende Weltkoordinate; das Raster wird passend zum Maßstab verkleinert
- **Assistent** „3D-Skybox anlegen“, wenn es noch keine gibt: Maßstab, Raumgröße (Vorschlag: dreifache Breite der verkleinerten Map), Bodenplatte, Nebel aus env_fog_controller übernehmen. Raum (tools/toolsskybox) und sky_camera werden dort angelegt, wo innerhalb der Map-Grenze Platz ist
- **Verkleinert hineinkopieren**: ausgewählte Brushes (auch Brush-Entities) werden im Maßstab der sky_camera an die passende Stelle der Skybox kopiert, Texturen mitskaliert. Props und andere Punkt-Entities werden übersprungen (Source kann sie nicht verkleinern)
- **Prüfen**: fehlende oder doppelte sky_camera, kein skyname, ungültiger Maßstab, Raum nicht geschlossen, Wände ohne Skybox-Textur, Spawnpunkte im Raum, Raum überschneidet die Map oder liegt außerhalb der Grenze, verkleinerte Map ragt über den Raum, Nebel nur in Map oder Skybox, Trigger/Clips im Raum
- In der 3D-Skybox-Vorschau werden Werkzeug-Texturen (nodraw, clip, trigger …) nicht mehr gezeichnet – wie im Spiel; der Skybox-Raum wird jetzt auch dann richtig erkannt, wenn Inhalte vor der sky_camera stehen

**Sonstiges**
- Neue, noch ungespeicherte Maps heißen in Tab und Fenstertitel in der gewählten Sprache („untitled“ usw.)
- Build: Zeitlimit und Wiederholungsversuche für apt-get, Zeitlimits je Job

## 0.10.0

**Live-Sitzung ohne Portfreigabe**
- Neuer Weg „Mit Code (ohne Portfreigabe)“: Der Gastgeber erstellt einen Einladungscode (`AMB1-…`), der Gast fügt ihn ein und schickt seinen Antwortcode zurück – danach verbinden sich beide direkt (UDP-Hole-Punching). Die eigene Internet-Adresse wird über öffentliche STUN-Server erfragt; Map-Daten gehen nie über fremde Server. Darüber läuft ein eigener zuverlässiger Datenstrom mit der bisherigen Verschlüsselung. Klappt mit den meisten Heim-Routern, nicht bei symmetrischem NAT (manche Mobilfunk- und Firmennetze)
- Fenster „Live-Sitzung“ mit Segment Gastgeber sein / Beitreten, Beitreten mit Adresse oder Code

**Updates, Installer, macOS**
- Beim Start fragt Amboss bei GitHub nach einer neueren Version (abschaltbar unter Einstellungen → Oberfläche → Updates, von Hand über Hilfe → Nach Updates suchen). Hinweis unten rechts mit „Was ist neu?“, „Jetzt aktualisieren“ (lädt das Paket und tauscht die Programmdatei aus, aktiv nach dem Neustart), „Später“ und „Diese Version überspringen“. Es wird nur die öffentliche Versionsliste abgerufen
- Windows-Installer (`amboss-…-windows-setup.exe`): installiert ohne Adminrechte nach `%LOCALAPPDATA%\Programs\Amboss`, Startmenü, optional Desktop-Verknüpfung und `.vmf`-Verknüpfung, Deinstallation über die Systemsteuerung
- Programm-Icon (Fenster, Taskleiste, exe) und Versionsangaben in der exe
- GitHub Actions: testet jeden Push und baut bei einem veröffentlichten Release Windows (ZIP + Installer), macOS (Universal-App für Apple Silicon und Intel) und Linux und hängt sie ans Release

**Sprachen und Darstellung**
- Neue Sprachen: Französisch, Spanisch, Polnisch, Russisch (Oberfläche, Meldungen, Tutorial). Fehlende Texte fallen auf Englisch zurück; die Beschreibungen der eingebauten Entity-Liste gibt es weiterhin auf Deutsch und Englisch. Beim ersten Start wird die Systemsprache gewählt
- Oberflächengröße einstellbar (90–200 %, zusätzlich zur Windows-Skalierung; auch Strg+Plus/Minus, Strg+0), wird gespeichert
- Hohe DPI geprüft: Dialoge werden nie größer als das Fenster und scrollen dann
- Rund 40 neuere Texte, die bisher auch im Englischen deutsch blieben, übersetzt; `tools/i18n_check.py` prüft jetzt alle `tr(…)`-Texte

**UI-Prüfung abgeschlossen**
- Startseite: Status-Zeilen klickbar („Einrichten…“), Mini-Draufsicht bei zuletzt geöffneten Maps
- Kontextleiste zeigt je Werkzeug nur passende Elemente
- Panel: „Rohwerte“, Texturbereich einklappbar und in der Höhe ziehbar, Einfach-Modus mit „Alle Eigenschaften (n)“, Brush-Panel (Größe/Position, Materialien, Schnellknöpfe), HDR „wie Brightness“, farbige P/Y/R
- Objektliste mit Auge/Schloss und Gruppen als Knoten; Visgroups als Liste mit Farbe und Anzahl
- Kameras umbenennen/löschen, Messergebnis mit Kopieren, Block-Form nur noch in der Kontextleiste
- Textur-Browser mit Kachelgröße; Ersetzen mit Vorschau und Trefferzahl; Transformieren mit Drehpunkt; Gehe zu mit Brush/Seite/Entity/Name und Vorschau; logische Ansicht mit Suche und Leerzustand; Entity-Hilfe als Tabelle; Icons je Entity-Art; Pfade in den Einstellungen mit Status-Punkt

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
