# Amboss – Testplan für Windows mit echten Daten

Dieser Plan prüft alles, was in der Entwicklungsumgebung (Linux, ohne Garry's Mod, ohne Grafikkarte) **nicht** getestet werden konnte.
Jeder Test hat eine ID, Schritte und das erwartete Ergebnis. Bitte das Ergebnis eintragen: **OK**, **Fehler** (mit Beschreibung) oder **übersprungen**.

**Bei einem Fehler bitte mitschicken:** Test-ID, was du getan hast, was passiert ist, einen Screenshot.
Falls vorhanden zusätzlich: das Compile-Log (`<map>.log` bzw. Kopieren-Knopf im Compile-Fenster), die Statuszeile und die betroffene VMF.
Bei einem Absturz zusätzlich den Inhalt von `%APPDATA%\amboss\` (Einstellungen, Autosave).

## Testumgebung

| Punkt | Bitte eintragen |
|---|---|
| Windows-Version (10/11, 64 Bit) | |
| Grafikkarte und Treiber | |
| Bildschirmauflösung und Skalierung (100 %, 125 %, 150 % …) | |
| Garry's-Mod-Pfad (Laufwerk, Steam-Bibliothek) | |
| Anzahl abonnierter Workshop-Addons (ungefähr) | |
| Amboss-Version (Datum der ZIP) | |
| Testmaps | `beispiel_raum.vmf`, `funktionen.vmf`, eigene große Map (z. B. `rp_omega_station_test.vmf`) |

Vor dem Start: eine **Sicherungskopie** der eigenen Maps anlegen.

---

## A. Start, Einstellungen, Spielinhalte

| ID | Schritte | Erwartet |
|---|---|---|
| A1 | `amboss.exe` aus dem entpackten Ordner starten (erster Start, keine alten Einstellungen). | Fenster erscheint in unter 2 s. Startbildschirm mit Vorlagen. Keine Meldung „reagiert nicht“. |
| A2 | Statuszeile/Startseite beobachten, ohne die Maus zu bewegen. | „Spielinhalte laden …“, danach „Spielinhalte: N Materialien aus X VPK(s) – Addons werden noch eingelesen …“, danach „… und Y Addon(s)“. Die Zahlen sind deutlich größer als 0. |
| A3 | Während des Ladens den Texturbrowser öffnen und scrollen. | Vorschaubilder erscheinen nach und nach, die sichtbaren zuerst. Die Oberfläche bleibt flüssig. |
| A4 | Bearbeiten → Einstellungen: Spielordner, VBSP/VVIS/VRAD, Spiel-EXE, FGD prüfen. | Automatisch gefunden: `…\GarrysMod\garrysmod`, `…\bin\win64\vbsp.exe` usw., FGD `…\bin\garrysmod.fgd` (falls vorhanden). |
| A5 | GMod in einer **zweiten Steam-Bibliothek** (anderes Laufwerk): Einstellungen leeren, „Automatisch suchen“. | Wird gefunden (Steam-Bibliotheken aus `libraryfolders.vdf`). |
| A6 | Spielordner absichtlich falsch eintragen (z. B. `…\GarrysMod`, ohne `garrysmod`, oder mit Anführungszeichen). | Pfad wird korrigiert oder es kommt eine verständliche Meldung. Danach „Spielinhalte neu laden“ → Materialien da. |
| A7 | Startseite: Zeile zur FGD. | „FGD geladen – N Entity-Klassen“ (mit echter FGD) bzw. „Eingebaute Entity-Liste (456 Klassen)“. |
| A8 | Design umschalten (Sonne/Mond oben rechts). | Hell/Dunkel wechselt sofort, alle Texte bleiben lesbar. Die 3D-Ansicht bleibt dunkel. |
| A9 | Windows-Skalierung 125 % und 150 % (Anzeigeeinstellungen), Amboss neu starten. | Schrift und Bedienelemente scharf und passend groß, nichts abgeschnitten. |
| A10 | Amboss schließen und neu öffnen. | Einstellungen, zuletzt geöffnete Maps und Fenstergröße sind erhalten. |
| A11 | Amboss zweimal gleichzeitig starten. | Beide laufen. Der zweite meldet, dass der MCP-Port belegt ist (kein Absturz). |
| A12 | Amboss mit leerem Profil starten (Ordner `%APPDATA%\amboss` vorher umbenennen). Assistent durchgehen: Sprache, Oberfläche, „Automatisch suchen“, Addons-Haken, Weiter. | Assistent erscheint, findet GMod, alle Prüfpunkte grün. Inhalte werden geladen. Danach ist der Assistent nicht mehr da (über Hilfe erneut aufrufbar). |
| A13 | Hilfe → Tutorial: alle fünf Kapitel einmal selbst und einmal nur mit „Zeig es mir“ durchspielen. | Schritte gehen von selbst weiter, sobald erledigt; hervorgehobene Werkzeuge stimmen; am Ende ist die Map kompilierbar und im Spiel öffnet die Tür über den Trigger. Unklare Texte bitte notieren. |
| A14 | Ansicht → Oberfläche: zwischen Einfach und Experte wechseln. | Einfach: 6 Werkzeuge, kurze Menüs, ohne Visgroups-Reiter und Spezialschalter. Experte: alles wie gewohnt. |
| A15 | Alle Menüs durchsehen. | Übersichtlich, keine doppelten Einträge, Punkte ohne Auswahl ausgegraut, nichts Wichtiges fehlt. |
| A16 | Datei → Öffnen: eine .bsp aus `garrysmod/maps` wählen; außerdem eine .bsp ins Fenster ziehen. | Dekompilier-Dialog erscheint, Ziel liegt nicht im maps-Ordner, danach öffnet sich die VMF. |

## B. Leistung mit großer Map

| ID | Schritte | Erwartet |
|---|---|---|
| B1 | Große eigene Map öffnen (z. B. 6.400 Brushes, 4.600 Entities). Ladezeit messen. | Unter ca. 5 s, kein Einfrieren. |
| B2 | In der 3D-Ansicht frei herumfliegen (WASD + Maus), FPS-Anzeige unten rechts ablesen. | Flüssig (mindestens 30 FPS, Wert notieren). |
| B3 | In den 2D-Ansichten zoomen und verschieben. | Flüssig, keine Ruckler. |
| B4 | Einen Brush auswählen und ziehen, mehrmals Strg+Z / Strg+Y. | Sofortige Reaktion. Rückgängig/Wiederholen in unter 0,5 s. |
| B5 | Ansicht → Texturiert / Einfarbig / Drahtgitter umschalten. | Wechsel ohne lange Pause. |
| B6 | MSAA (Einstellungen → Grafik) auf 4× und 8× stellen, neu starten. | Kanten geglättet, FPS notieren. |
| B7 | Amboss 30 Minuten offen lassen und dabei arbeiten. Arbeitsspeicher im Task-Manager beobachten. | Kein stetiges Wachsen, kein Absturz. |

## C. Darstellung

| ID | Schritte | Erwartet |
|---|---|---|
| C1 | Map mit vielen verschiedenen Texturen ansehen. | Texturen stimmen mit Hammer/Spiel überein (Ausrichtung, Skalierung, Farbe). |
| C2 | Glas, Fenster, Gitter, Zäune (`$translucent`, `$alphatest`, `$alpha`, Refract-Glas, Wasser). | Durchsichtig bzw. mit Löchern dargestellt. **Bitte konkrete Materialnamen notieren, die falsch aussehen.** |
| C3 | Blend-Texturen auf Displacements (WorldVertexTransition). | Zwei Texturen gemischt nach dem gemalten Alpha. |
| C4 | Props (`prop_static`, `prop_physics`, `prop_dynamic`) mit verschiedenen Skins. | Modelle mit richtigen Texturen, Größe und Drehung. |
| C5 | Modelle aus Workshop-Addons (.gma). | Werden gefunden und dargestellt. |
| C6 | `sky_camera` mit 3D-Skybox. | 3D-Skybox-Vorschau sichtbar. |
| C7 | Lichtvorschau (Darstellungsmenü der 3D-Ansicht). | Räume grob so hell wie im Spiel (ohne Schatten). |
| C8 | `info_overlay` und `infodecal` in der Map. | An der richtigen Stelle mit Textur sichtbar. |
| C9 | Editor-Sprites (Lichter, Sounds, Logik-Entities). | Symbole aus `materials/editor/` werden angezeigt. |
| C10 | Animierte Texturen (AnimatedTexture), Laufbänder/Wasserfälle (TextureScroll), Texturen mit `$basetexturetransform`. | Bewegen sich bzw. sind gedreht/skaliert wie im Spiel. |
| C11 | 3D-Menü → Kollision an, Map mit vielen Props (z. B. Omega Station). | Türkise Drahtgitter liegen deckungsgleich auf den Props (gedrehte/skalierte Props prüfen!). Ausgewählte Props gelb. Keine spürbaren Ruckler. |
| C12 | Lichtvorschau mit Schatten: Map mit light_environment und mehreren light/light_spot, durch die Map fliegen. | Sonne und nahe Lichter werfen Schatten, keine Streifen/Akne auf Flächen. Ruckeln nur kurz, wenn sich etwas ändert. FPS mit/ohne Schatten notieren. |
| C13 | Ansichten-Trenner und rechte Panel-Kante ziehen, Achse einer 2D-Ansicht ändern, Amboss beenden und neu starten; danach Ansicht → „Layout zurücksetzen“. | Aufteilung bleibt nach dem Neustart erhalten; Zurücksetzen stellt 50/50 und 344 px wieder her. |
| C14 | Einstellungen → Map-Grenze auf 32768, in 2D weit herauszoomen; Brush außerhalb von ±16384 setzen, Probleme prüfen. | Rote Grenze bei ±32768. Bei 16384 meldet die Prüfung den Brush, bei 32768 nicht. |

## D. Bearbeiten und Werkzeuge

| ID | Schritte | Erwartet |
|---|---|---|
| D1 | Jedes Werkzeug der linken Leiste einmal benutzen (Auswahl, Lupe, Kamera, Entity, Block, Textur, Decal, Overlay, Clip, Vertex, Terrain, Cordon). | Funktioniert wie beschrieben, keine Fehlermeldung. |
| D2 | Block-Werkzeug: alle Formen (Block, Keil, Zylinder, Spitze, Kugel, Bogen, Torus, Treppe, Rampe …). | Gültige Brushes, kompilieren ohne „Invalid solid“. |
| D3 | Aushöhlen, Carve, Clip, Vertex-Bearbeitung an schrägen Brushes. | Ergebnis ist gültig (siehe Kompilieren E1). |
| D4 | Textur-Werkzeug: Einpassen, Ausrichten, nahtlos umwickeln (Alt+Rechtsklick), Lightmap-Skalierung. | Werte stimmen im Spiel. |
| D5 | Displacement erstellen, malen (Anheben, Glätten, Alpha), unterteilen, nähen. | Im Spiel identisch zur Editor-Ansicht (Ausrichtung). |
| D6 | Kopieren in Amboss → Einfügen in Hammer/Hammer++ und umgekehrt (Strg+C/V über die Zwischenablage). | Objekte kommen vollständig an. |
| D7 | Tastenkürzel aus der Hilfe durchprobieren (siehe README). | Alle reagieren. Keine Doppelbelegung mit Windows-Kürzeln. |
| D8 | Mehrere Maps in Tabs öffnen und zwischen ihnen kopieren. | Funktioniert. Speichern pro Tab korrekt. |
| D9 | Datei → Speichern unter, danach VMF in Hammer öffnen. | Hammer öffnet die Datei ohne Fehler. Gruppen, Visgroups und Kameras sind erhalten. |
| D10 | Absturz simulieren: Amboss im Task-Manager beenden, neu starten. | Startseite bietet „Nicht gespeicherte Änderungen gefunden – Wiederherstellen“ an. |
| D11 | Terrain-Werkzeug: Rauschen, Terrassen (Stufenhöhe), Schärfen, Abfall weich/linear/hart; Strg beim Glätten. | Formen wie erwartet, Rückgängig nimmt einen ganzen Strich zurück. |
| D12 | Karte → „Gelände aus Heightmap…“ mit einem echten Heightmap-PNG (8 und 16 Bit), 8×8 Kacheln, danach kompilieren. | Nahtlose Kacheln, Höhen passen zum Bild (Norden oben), VBSP ohne Displacement-Fehler. |
| D13 | Ein paar Brushes (z. B. Kiste mit Holztextur) wählen → Werkzeuge → „Auswahl als Modell (Prop)…“ → prop_static → „Exportieren & kompilieren“. | studiomdl wird neben vbsp gefunden, Log endet mit „Fertig“, Brushes werden durch das Prop ersetzt. Im Modell-Browser und im Spiel sieht das Modell richtig texturiert aus, Kollision passt. |
| D14 | Wie D13 mit prop_physics und Masse 30. | Prop fällt im Spiel, lässt sich mit der Physgun aufheben. Neue Dateien liegen unter `garrysmod/models/amboss/` und `garrysmod/materials/models/amboss/<name>/`. |
| D15 | Map mit verschachtelten Instanzen (z. B. Wohnblock → Wohnungen → Türen, mit `replace`-Parametern und Ein-/Ausgängen zwischen den Ebenen) öffnen; eine Instanz wählen → Werkzeuge → „Instanz in Tab öffnen“, dort etwas ändern und speichern; zurück zur Map; zuletzt „Instanzen einklappen“ und kompilieren. | Alle Ebenen sind in 2D/3D sichtbar und richtig gedreht. Die Änderung erscheint sofort an allen Stellen. Nach dem Einklappen heißen Entities wie bei VBSP (z. B. `block-wohnung-tuer`), Outputs funktionieren im Spiel genauso wie ohne Einklappen. |

## E. Kompilieren

| ID | Schritte | Erwartet |
|---|---|---|
| E1 | `beispiel_raum.vmf` öffnen → Kompilieren & starten (Standardprofil). | VBSP, VVIS und VRAD laufen durch, GMod startet die Map. |
| E2 | Compile-Fenster: Fehlerzeile anklicken (Map mit absichtlichem Fehler, z. B. Brush mit falscher Form). | Springt zum betroffenen Brush. |
| E3 | Map mit absichtlichem Leck kompilieren. | Pointfile wird automatisch geladen, roter Pfad sichtbar. |
| E4 | Cordon setzen und kompilieren. | Nur der Bereich wird kompiliert, automatisch abgedichtet. |
| E5 | HDR an/aus, VVIS schnell/normal, VRAD schnell/normal/final. | Parameter erscheinen im Log, Ergebnis wie erwartet. |
| E6 | Expertenmodus: eigenen Schritt hinzufügen (z. B. `bspzip`). | Wird ausgeführt. |
| E7 | Kompilieren abbrechen (Knopf im Compile-Fenster). | Prozesse werden beendet, Amboss bleibt bedienbar. |
| E8 | Große eigene Map vollständig (VVIS normal) kompilieren. Zeiten notieren. | Läuft durch, Log ohne Fehler. |
| E9 | Pfade mit Leerzeichen und Umlauten (z. B. `C:\Users\…\Mein Gmod Zeug\Maps\Größe`). | Kompilieren funktioniert. |
| E10 | Kompilieren → „Kompilieren & Exportieren…“, Haken „Eigene Inhalte in die BSP packen“, Liste aufklappen, bei einer Map mit eigenen (losen) Texturen, Modellen, Sounds, einem eigenen Partikel und Soundscape. Einen Eintrag abwählen, „Auswahl speichern“, dann F9. | Liste vollständig (Bumpmaps, .vvd/.vtx/.phy, .pcf, Soundscape-Sounds, .nav). Nach dem Kompilieren sind die gewählten Dateien in der BSP (GCFScape/VIDE prüfen), der abgewählte nicht. Map läuft auf einem frischen GMod ohne die losen Dateien. |
| E11 | Inhalte in eine BSP packen, die VRAD schon Cubemaps eingepackt hat (nach buildcubemaps). | Cubemap-Einträge bleiben erhalten („waren schon drin“), Map lädt ohne Fehler. |
| E12 | Datei → „BSP dekompilieren…“ mit einer fremden GMod-Map (z. B. gm_construct, rp_-Map) und mit der eigenen Omega Station. Danach die dekompilierte VMF kompilieren. | Geometrie, Texturausrichtung, Türen/Trigger, Displacements, Props und Overlays sind am richtigen Platz. VBSP läuft durch. Bericht im Dialog notieren (übersprungene Brushes, Overlays ohne Fläche). |
| E13 | Karte → „Navmesh erzeugen“ in Omega Station und einer kleinen Testmap; danach F9 mit Kompilier-Option „Navmesh“, im Spiel `nav_edit 1` und einen NextBot (z. B. aus dem Workshop) spawnen. | Grüne Flächen decken die begehbaren Bereiche ab, keine in Wänden. GMod lädt die .nav (höchstens Hinweis „veraltet“ in der Konsole), `nav_edit` zeigt die Bereiche, NextBots laufen darauf. Bitte Bilder von Stellen schicken, an denen etwas fehlt oder falsch ist. |

## F. Automatische Optimierungen (wichtig: mit echten Daten prüfen)

Für jede Optimierung: zuerst **„Nur prüfen“**, Markierungen ansehen, dann auf einer **Kopie** der Map **„Auf Map anwenden“**, kompilieren und im Spiel ansehen.

| ID | Schritte | Erwartet |
|---|---|---|
| F1 | **Verdeckte Flächen → nodraw.** Danach im Spiel alle Böden, Wände, Decken, Treppen und Fenster ansehen. | Keine sichtbare Fläche fehlt oder zeigt „Spiegelflur“ (Hall of Mirrors). **Besonders prüfen:** Flächen unter Clip-, Trigger- und Glas-Brushes. |
| F2 | **Leckprüfung vor VBSP** mit absichtlichem Leck. | Roter Pfad, VBSP wird übersprungen (wenn eingestellt). Ohne Leck: keine Fehlmeldung. |
| F3 | **func_detail automatisch.** Danach Kompilieren: VVIS-Zeit vorher/nachher vergleichen. | Map bleibt dicht (kein Leck), VVIS schneller, keine Sichtfehler. |
| F4 | **Areaportals in Tür-Öffnungen.** Türen im Spiel öffnen und schließen. | Keine schwarzen/fehlenden Bereiche hinter Türen. Keine VBSP-Fehler „areaportal leaks“. |
| F5 | **Bodennodes / Luftnodes / Node-Link-Controller / Hints.** Map mit NPCs (z. B. Bürger, Combine) testen. | NPCs laufen durch Räume und Türen. Keine Fehler „node graph out of date“. Grüne Netzlinien im Editor passen zum Verhalten. |
| F6 | **Navmesh** (Häkchen) → Spielstart. | `maps/<map>.nav` wird erzeugt. NextBots laufen. |
| F7 | **Lightmap-Auflösung automatisch.** | Kompilieren ohne „too many lightmaps“, Schatten in Lichtnähe schärfer. Dateigröße notieren. |
| F8 | **env_cubemap verteilen** → `buildcubemaps` im Spiel. | Spiegelungen plausibel, keine doppelten Cubemaps. |
| F9 | **Lichtprüfung.** | Gemeldete Probleme stimmen (Licht in Brush, zu viele schaltbare Lichter). |
| F10 | **Eckpunkte runden, Quader zusammenführen, Splitter entfernen, flache Displacements vereinfachen.** | Optik unverändert, weniger Brushes, keine neuen Lecks oder „Invalid solid“. |
| F11 | **Prop-Ausblenddistanzen, Schatten kleiner Props.** | Props verschwinden nicht zu früh. Werte notieren, falls doch. |
| F12 | **Wirkungslose Logik entfernen.** | Nur wirklich unbenutzte Entities entfernt. Alle Funktionen der Map gehen noch. |
| F13 | **bspzip – eigene Inhalte packen.** BSP auf einem zweiten PC/Account ohne diese Inhalte starten. | Eigene Texturen und Modelle sind in der BSP, keine lila-schwarzen Texturen und keine ERROR-Modelle. |
| F14 | **Beim Kompilieren automatisch anwenden** (Häkchen). | Die eigene VMF bleibt unverändert, Kopie in `amboss_build\`. Bericht oben im Compile-Log. |

## G. Entities

| ID | Schritte | Erwartet |
|---|---|---|
| G1 | Entity-Werkzeug: Klassenliste durchsuchen (z. B. „door“, „npc_“, „Licht“). | Alle gängigen Klassen vorhanden (func_, prop_, npc_, trigger_, logic_, env_, info_, point_, filter_, game_, item_, weapon_, ai_ …). |
| G2 | Mit echter `garrysmod.fgd`: Eigenschaften von `prop_door_rotating`, `npc_citizen`, `env_projectedtexture`, `trigger_hurt`. | Felder, Auswahllisten und Spawnflags wie in Hammer. |
| G3 | **Ohne** FGD (Feld in den Einstellungen leeren): dieselben Klassen. | Eingebaute Liste liefert sinnvolle Felder. Map kompiliert, Entities funktionieren im Spiel. |
| G4 | Outputs anlegen (z. B. `func_button` → `OnPressed` → Tür `Open`), im Spiel testen. | Funktioniert. Rote/orange Prüfung zeigt falsche Ziele/Inputs. |
| G5 | Modell-Browser und Sound-Browser: suchen, übernehmen. | Pfade korrekt, im Spiel vorhanden. |
| G6 | Logische Ansicht (Entity-I/O-Diagramm) bei großer Map. | Bedienbar, Verbindungen stimmen. |
| G7 | `func_instance` mit eigener Instanz-VMF, einklappen. | Anzeige korrekt, VBSP fügt die Instanz richtig ein. |
| G8 | `lua_run` mit einfachem Code (`print("hallo")`, Spawnflag „Run code on spawn“). | Ausgabe in der GMod-Konsole. |
| G9 | func_door mit movedir (hoch, seitlich) und lip, func_door_rotating mit/ohne „One-way“ und „Reverse Dir“, prop_door_rotating mit opendir 0/1/2 auswählen. | Pfeil/Bogen und Umriss der offenen Position zeigen dieselbe Richtung und denselben Weg wie im Spiel. |
| G10 | Prop auswählen, beim Modell-Feld auf „…“ klicken, anderes Modell wählen, „Übernehmen“. | Browser öffnet mit dem aktuellen Modell markiert; danach steht das neue Modell im Feld und wird angezeigt. |
| G11 | Rechtes Panel schmal (300 px) und breit ziehen, verschiedene Entities (Tür mit Ausgängen, Prop, Licht) und Werkzeuge (Block, Clip, Textur) auswählen. | Nichts ragt rechts aus dem Fenster; die Kontextleiste zeigt bei Platzmangel nur noch Symbole. |
| G12 | Sprache auf English, ohne eigene garrysmod.fgd: Tür, Licht, Trigger, Prop auswählen; Klassen-Auswahl öffnen; „?“-Hilfe öffnen. | Beschreibungen, Schlüssel-Tooltips, Auswahlwerte, Flags und Ein-/Ausgänge sind englisch. Fundstellen mit Deutsch bitte notieren. |

## H. Zusammenführen, Vergleichen, Git

| ID | Schritte | Erwartet |
|---|---|---|
| H1 | Datei → Map zusammenführen: zwei eigene Maps mit gleichen Entity-Namen. | Namen werden umbenannt, Outputs ziehen mit, Import-Visgroup vorhanden. |
| H2 | Karte → Mit anderem Map-Stand vergleichen (alte Sicherung). | Grün/Rot/Gelb stimmt mit den echten Änderungen überein. |
| H3 | 3-Wege-Zusammenführen mit zwei Bearbeitungen derselben Ausgangsmap. | Beide Änderungen enthalten. Konflikte in der Visgroup „Konflikt: andere Version“. |
| H4 | Git-Merge-Treiber einrichten (siehe README) und in einem Test-Repository zusammenführen. | Läuft automatisch durch bzw. meldet Konflikte. |

## I. Live-Zusammenarbeit

| ID | Schritte | Erwartet |
|---|---|---|
| I1 | Heimnetz: PC 1 startet die Sitzung (Passwort ≥ 12 Zeichen), PC 2 tritt mit der angezeigten Heimnetz-Adresse bei. | PC 2 bekommt die Map. Beide sehen Kameras und Auswahl des anderen. |
| I2 | Beide bearbeiten gleichzeitig verschiedene Objekte, 10 Minuten lang. | Änderungen erscheinen sofort beim anderen. Keine Doppelungen, kein Auseinanderlaufen. Am Ende beide speichern und die VMFs vergleichen (H2): identisch. |
| I3 | Ein Objekt, das der andere ausgewählt hat, verschieben. | Geht nicht (gesperrt, in dessen Farbe umrandet). |
| I4 | Strg+Z auf beiden Seiten. | Nimmt nur die eigenen Schritte zurück. |
| I5 | Falsches Passwort. | „Abgelehnt: Falsches Passwort“. |
| I6 | Windows-Firewall: Beim ersten Start der Sitzung erscheint die Firewall-Abfrage. | Nach „Zulassen“ (privates Netz) können Gäste beitreten. |
| I7 | **Internet über IPv6:** Gastgeber gibt in der Router-Firewall TCP 3918 für seinen PC frei (FRITZ!Box: Internet → Freigaben → Gerät → IPv6). Der Gast nutzt die im Fenster angezeigte Adresse `[…]:3918` von einem anderen Anschluss (z. B. Handy-Hotspot). | Beitritt klappt. Bearbeiten wie bei I2. Verzögerung notieren. |
| I8 | Wie I7, aber ohne Freigabe im Router. | Verständliche Meldung („keine Antwort – Firewall/Router …“), kein Einfrieren über 10 s. |
| I9 | Gast verliert die Verbindung (WLAN aus). | Gast: „Verbindung getrennt – Map bleibt als lokale Kopie offen“. Gastgeber arbeitet weiter. |
| I10 | Gast mit älterer Amboss-Version. | Meldung „andere Amboss-Version“. |

## J. Prefabs, Vorlagen, Import/Export

| ID | Schritte | Erwartet |
|---|---|---|
| J1 | Alle 14 mitgelieferten Prefabs einfügen (2D und 3D, an Boden/Wand/Decke), kompilieren, im Spiel testen. | Funktionieren (Türen öffnen, Lichtschalter, Teleporter …). |
| J2 | Hammer-Prefab-Bibliothek (.ol) und .rmf öffnen. | Werden angezeigt bzw. umgewandelt. |
| J3 | Alle Vorlagen (Datei → Neu aus Vorlage) erzeugen und kompilieren. | Kompilieren ohne Leck. |
| J4 | .map-Import (TrenchBroom/J.A.C.K.) und OBJ-Export (in Blender öffnen). | Geometrie korrekt. |

## K. MCP (KI-Fernsteuerung)

| ID | Schritte | Erwartet |
|---|---|---|
| K1 | Claude Desktop mit `amboss.exe --mcp-stdio` einrichten (siehe README), Amboss läuft. | Claude kann z. B. „Baue einen Raum mit Licht und Spawn“ ausführen. Das Ergebnis erscheint live im Editor. |
| K2 | Wie K1, aber Amboss ist geschlossen. | Arbeitet ohne Fenster auf einer Datei (`--map`). |
| K3 | `compile` und `get_compile_status` per MCP. | Kompiliert, Status kommt zurück. |
| K4 | Werkzeuge → „Lua-Skripte…“: Beispiele „treppe.lua“, „lichter_raster.lua“, „tueren_benennen.lua“ ausführen, danach je einmal Strg+Z; ein eigenes Skript speichern und neu laden; `while true do end` ausführen. | Ergebnis erscheint sofort, Strg+Z nimmt das ganze Skript zurück. Der Ordner `skripte` liegt neben amboss.exe. Die Endlosschleife bricht nach 10 s mit Meldung ab, Amboss bleibt bedienbar. |
| K5 | Per KI: „Baue mit run_lua eine Wendeltreppe“. | Claude nutzt `run_lua`, die Treppe entsteht in einem Schritt. |

## M. UI-Prüfung (Deutsch und Englisch)

| ID | Schritt | Erwartung |
|----|---------|-----------|
| M1 | Einstellungen → Oberfläche → English, dann Deutsch | Panel bleibt gleich breit (360 px), nichts springt |
| M2 | In Englisch: Probleme prüfen, Kompilieren, Live-Sitzung, Map zusammenführen | Keine deutschen Texte in Meldungen und Protokoll |
| M3 | Licht/Prop/Trigger auswählen | Lange Schlüsselnamen in zwei Zeilen statt „…“, Wertfelder bis zum rechten Rand |
| M4 | Zwei Lichter auswählen | Gleiche Felder wie bei einem Licht, unterschiedliche Werte „(verschieden)“ |
| M5 | Brush wählen → Strg+T | Nur Brush-Klassen, func_detail vorgewählt |
| M6 | Menüs öffnen | Kürzel mit Abstand, „Shift“ einheitlich, Bearbeiten → Ausschneiden vorhanden |
| M7 | Hilfe → Über Amboss / Einstellungsordner öffnen | Dialog mit Version; Explorer öffnet den Ordner |
| M8 | Displacement-Werkzeug mit ausgewähltem Displacement | Modus-Raster 3×3, Kollisions-Häkchen zeigen den Zustand der Auswahl |
| M9 | Deutsch: Zahlen in Feldern und Zoom | Dezimalkomma; Eingabe mit Komma oder Punkt funktioniert |

## N. Neue Werkzeuge (Gestalten)

| ID | Schritt | Erwartung |
|----|---------|-----------|
| N1 | Overlay auf ein Displacement-Gelände legen | Overlay liegt im 3D-Fenster auf der Hügelform, nicht als flaches Rechteck |
| N2 | Werkzeuge → Material-Editor → Neu, PNG importieren, speichern | VMT/VTF unter `garrysmod/materials/…`; Textur erscheint im Browser und im Spiel |
| N3 | Partikel-Browser → System → „Im Partikel-Editor öffnen“, Wert ändern, speichern unter neuem Namen | Vorschau bewegt sich; neue .pcf lädt im Spiel (`info_particle_system`) |
| N4 | Lichtvorschau, Ansicht → Globale Beleuchtung, rote Wand neben weißem Boden | Boden vor der Wand rötlich; Fortschritt im FPS-Schild; FPS bleibt flüssig |
| N5 | Mehrere Props in die Luft heben → Objekt → Mit Physik fallen lassen | Props landen auf Boden/Gelände/aufeinander; Esc während des Fallens stellt alles zurück |
| N6 | Vegetation streuen mit echten Baum-Modellen über Gelände malen; Shift radiert | Bäume stehen auf der Oberfläche, Abstand wird eingehalten; Map kompiliert, Props im Spiel |
| N7 | Prozedurale Generatoren → Räume → Erzeugen → Kompilieren & starten | Map ohne Leck, Spieler spawnt in einem beleuchteten Raum |
| N8 | Tile-Set: Ecke, Gang, Tür, Treppe setzen, mit R drehen, Shift-Klick entfernt | Bausteine schließen dicht aneinander; Rückgängig je Baustein |
| N9 | Brushes wählen → Werkzeuge → Polygon-Mesh → „Aus ausgewählten Brushes erstellen“; Fläche ziehen, extrudieren; speichern, schließen, neu öffnen | Mesh unverändert (Datei `<map>.meshes.json` liegt neben der VMF) |
| N10 | Im Mesh-Fenster „Zum Modell kompilieren“ (Spielverzeichnis + VBSP-Pfad gesetzt) | Log endet mit „Fertig“; Modell `models/amboss/meshes/…` im Spiel sichtbar, **richtig herum** (nicht von innen) und mit Texturen; Kollision vorhanden |
| N11 | OBJ aus Blender (Meter, Y oben) importieren | Größe und Ausrichtung stimmen (1 m ≈ 39 Einheiten, steht aufrecht) |
| N12 | Werkzeuge → Auswahl als Modell (Prop) mit einer Kiste | Modell im Spiel **nicht** von innen nach außen gedreht (Dreiecksreihenfolge wurde geändert) |
| N13 | Brush wählen → Werkzeuge → Bezier-Patch → „Patch auf Auswahl beginnen“, mittlere Punkte hochziehen; Kompilieren | Glatte Kuppel im Editor und im Spiel; Rückgängig stellt die Fläche zurück |

## O. Partikel, Begehen, Live-Abgleich

| ID | Schritt | Erwartung |
|----|---------|-----------|
| O1 | `info_particle_system` mit einem HL2-Effekt (z. B. `fire_small_01`) setzen | Effekt läuft im 3D-Fenster; Ansicht → „Partikel-Effekte abspielen“ schaltet ab |
| O2 | F6 in einem Raum mit Treppe und Kisten | Fällt auf den Boden, Treppe (Stufen ≤ 18) geht, Kiste 40 nur mit Ducksprung, Wände halten auf; Esc beendet |
| O3 | Kompilieren → Live-Abgleich → „Brücke installieren“ (bzw. „neu installieren“), GMod neu starten, Map im Einzelspieler laden; GMod-Konsole öffnen | Konsole zeigt „[Amboss Live] Bruecke 2 aktiv“; Live-Fenster zeigt „Verbunden“, out.txt/alive.txt „gerade eben“, grüner Spieler in 2D/3D; bleibt verbunden, solange die Konsole offen ist |
| O4 | Bei verbundenem Spiel: Prop verschieben, Farbe einer Entity ändern, neue Kiste setzen, Entity löschen | Erscheint im Spiel innerhalb ~1 s |
| O5 | Neuen Brush (Wand) ziehen | Wand erscheint im Spiel (heller, ohne Licht) und ist fest |
| O6 | Input auslösen: Tür-Name, `Open` | Tür öffnet sich im Spiel |
| O7 | „Spieler zur 3D-Kamera“ / „3D-Kamera folgt dem Spieler“ | Spieler springt an die Kamera; Kamera folgt beim Laufen im Spiel |
| O8 | Shift+F9 bei laufendem Spiel | Kompiliert, Map lädt im laufenden Spiel neu (kein zweites GMod), Spieler steht an der Kamera |
| O9 | Shift+F9 ohne laufendes Spiel | GMod startet, Spieler steht an der Kamera |
| O10 | Brush wählen, Clip-Werkzeug, in 3D drei Punkte auf Flächen klicken, Enter | Schräger Schnitt wie in der Vorschau; Esc verwirft die Punkte |
| O11 | worldspawn `skyname` = `sky_day01_01` (bzw. eigener Himmel), Ansicht → Himmel-Vorschau | Himmel rundum richtig ausgerichtet (Horizont unten, Sonne wie im Spiel) – mit dem Spiel vergleichen |
| O12 | `env_fog_controller` mit fogenable 1, Farbe, Start/Ende | Entfernte Flächen gehen in die Nebelfarbe über; Ansicht → Nebel schaltet ab |
| O13 | `move_rope` mit NextKey auf `keyframe_rope`, Slack 50 | Seil hängt durch wie im Spiel, mit Seil-Textur; in 2D als Linie |
| O14 | Version prüfen | „0.13.0“ neben dem Logo, im Fenstertitel und unter Hilfe → Über Amboss |
| O15 | Hilfe → Tutorial…, Kapitel 6–9 mit echten Inhalten durchspielen (auch „Zeig es mir“) | Jeder Schritt wird erkannt; Gelände, Himmel, Nebel, Partikel, Overlay, Decal sehen richtig aus |
| O16 | Trigger mit Output: Ziel-Feld anklicken und einen Buchstaben tippen | Liste passender Namen mit Klasse; Pfeiltasten + Enter oder Klick übernimmt |
| O17 | Generatoren → Räume mit Höhenstufen 2, eigenen Texturen für Boden/Wände/Decke → Erzeugen → Kompilieren & starten | Kein Leck; Räume unterschiedlich hoch, Treppen in den Gängen begehbar; Texturen wie gewählt |
| O18 | Nach dem Erzeugen einen einzelnen Raum anklicken und verschieben/löschen; Strg+U | Nur dieser Raum ist ausgewählt; Gruppe lässt sich auflösen |
| O19 | Map → Gelände aus Heightmap → „Rauschen“, Regler ändern, Würfeln, Als PNG speichern, Gelände erstellen | Vorschau ändert sich sofort; PNG lässt sich wieder laden; Gelände entspricht der Vorschau |
| O20 | Werkzeuge → Prozedurale Generatoren → Räume mit Gängen, Höhenstufen 3, Windungen hoch, Generieren; in 3D durch die Gänge laufen (F6) | an Raumtüren kurze Treppen, danach ebene Gänge; Gänge mit Kurven und Kreuzungen; keine zwei Gänge nebeneinander zwischen denselben Räumen; jeder Raum begehbar erreichbar, kein Leck beim Kompilieren |
| O21 | Räume generieren (Standard), dann „als runde Kurve“ aus und erneut generieren; Wandstärke auf 8 stellen | Wände in 2D als schmale Doppellinien (nicht 64 dick); Gänge ohne Zickzack; mit Kurve: runde Außen- und Innenwand an Ecken, ohne: gerade Ecken; beides kompiliert ohne Leck |
| O22 | Einen Brush auswählen, dann einen in anderer Höhe; Ansicht → Raster → „2D: bei Auswahl andere Ebenen abdunkeln“ aus/an | Oben werden Objekte anderer Höhe blass, vorne die mit anderem X, seitlich die mit anderem Y; Auswahl aufheben → alles wieder normal; Einstellung bleibt nach Neustart |
| O23 | Maus über 3D: Shift+A drücken; mittlere Maustaste halten und ziehen; dabei W/A/S/D und Shift | Textur-Werkzeug wird gewählt (Kamera bewegt sich nicht); Umsehen mit Mausrad-Taste; beim Umsehen fliegt man mit WASD, mit Shift schneller |
| O24 | Einstellungen → Tastenkürzel → 3D-Kamera: Umsehen auf rechte Maustaste, Vorwärts auf Pfeil hoch, Empfindlichkeit ändern | Gilt sofort; nach Neustart noch da; „zurücksetzen“ stellt W/A/S/D und mittlere Maustaste wieder her |
| O25 | Textur-Werkzeug wählen, Flächen anklicken, rechts eine Textur anklicken; „Flächen-Einstellungen anzeigen“ | Rechtes Panel zeigt nur Texturen (alle, scrollbar); Klick wendet sie an; Einstellungen klappen darüber auf |
| O26 | prop_static auswählen, Ordner-Knopf neben „World Model“, Modell wählen, Übernehmen | Modell-Browser öffnet sich mit Vorschaubildern **mit Textur**; Modell wird ins Feld gesetzt und in 3D angezeigt |
| O27 | Entity bei Raster 16 neben das Raster legen (Raster 1), dann bei Raster 16 ziehen; Entity in 3D auf Boden/Wand setzen | Box-Ecken liegen nach dem Ziehen auf Rasterlinien; in 3D gesetzte Entities liegen auf dem 16er-Raster |
| O28 | Prop mit Loch/Gitter (z. B. Zaun, Regal) in 3D: daneben und durch die Lücke klicken | Keine gelbe Box mehr (nur rot bei Auswahl); Klick durch die Lücke wählt das Dahinterliegende |
| O29 | Objekt wählen, Maus in eine 2D-Ansicht, Strg+R, Maus bewegen, Linksklick; nochmal Strg+R, Rechtsklick | Vorschau dreht live (Winkel an der Maus); Linksklick behält, Rechtsklick stellt zurück; Strg+Z macht die Drehung rückgängig |
| O30 | Quader auswählen, Aushöhlen (Strg+H) | Sechs Wände ohne Überschneidung (Ecken nicht doppelt); kompiliert ohne Fehler |
| O31 | Entity-Werkzeug, Klasse light, in 3D setzen; Eigenschaften ansehen; Farbe über den Farbwähler ändern, Helligkeit auf 400 ziehen; kompilieren | „Farbe · Helligkeit“ zeigt Farbfeld + 200; nach Änderung steht z. B. „255 128 64 400“ im Feld; Licht in der Lichtvorschau und im Spiel in dieser Farbe/Helligkeit |
| O32 | Mit echter garrysmod.fgd: light setzen und auswählen | „Farbe · Helligkeit“, Appearance, Constant/Linear/Quadratic usw. sind da (wie bei light_environment) |
| O33 | Tür (prop_door_rotating) auswählen; Modell-Browser über das Menü öffnen, Modell wählen, Übernehmen; Panel schmal ziehen | Unten „Ziel: ausgewählte Entities“; Modell der Tür ändert sich; Ordner-Knopf neben „World model“ bleibt sichtbar |
| O34 | Entity-Werkzeug: ins Klassenfeld „info_p“ tippen, Pfeil runter + Enter; Entity setzen; Entity auswählen, Pfeil neben „Klasse“ → andere Klasse wählen | Vorschläge erscheinen unter dem Feld; gewählte Klasse wird gesetzt; über die Liste ändert sich die Klasse des ausgewählten Entities (Strg+Z macht es rückgängig) |
| O35 | Obere Leiste: etwas bauen, Pfeil zurück, Pfeil vor | Ohne Verlauf sind beide Pfeile grau; zurück macht die letzte Änderung rückgängig, vor stellt sie wieder her; Tooltip zeigt Strg+Z / Strg+Y |
| O36 | prop_static/prop_physics mit Modell setzen, info_player_start setzen | In Oben/Vorne/Seite ist die Form des Modells als Umriss zu sehen (kein Rechteck); ausgewählt in Akzentfarbe |
| O37 | Alte Map mit light ohne `_light` öffnen (z. B. unbenannt.vmf), kompilieren und starten | Protokoll: „Fehlende Standardwerte ergänzt: light: _light …“; Raum im Spiel beleuchtet |

## P. Version 0.10.0: Code-Verbindung, Updates, Installer, Sprachen

| Nr. | Schritt | Erwartung |
|---|---|---|
| P1 | Zwei PCs in **verschiedenen** Netzen (z. B. einer per Handy-Hotspot). A: Live-Sitzung → Gastgeber sein → Sitzung starten → „Einladungscode erstellen“, Code an B schicken | Code beginnt mit `AMB1-`; kein Hinweis „nur im selben Netz“ |
| P2 | B: Beitreten → „Mit Code“ → Code einfügen → „Antwortcode erzeugen und verbinden“, Antwortcode an A; A fügt ihn ein → Verbinden | Innerhalb weniger Sekunden verbunden, B hat die Map; Bearbeiten, Sperren, Chat wie bei der normalen Sitzung |
| P3 | Falsches Passwort bei B | „Falsches Passwort“, keine Verbindung |
| P4 | P1–P2 im selben Heimnetz | Verbindung klappt ebenfalls |
| P5 | Einstellungen → Oberfläche → Updates → „Jetzt suchen“ (mit dieser Version als neuestem Release) | Statuszeile „Amboss 0.10.0 ist aktuell.“ |
| P6 | Erst ab dem nächsten Release möglich (0.9.10 hat noch keine Update-Funktion): 0.10.0 aus einem Ordner starten, in dem man schreiben darf, wenn es ein neueres Release gibt | Hinweis „Amboss 0.10.0 ist verfügbar“ → „Jetzt aktualisieren“ → „Installiert“ → „Jetzt neu starten“ startet die neue Version; `amboss.old.exe` ist danach weg |
| P7 | Haken „Beim Start nach neuen Versionen suchen“ aus, neu starten | Kein Hinweis |
| P8 | `amboss-0.10.0-windows-setup.exe` ausführen (ohne Admin), mit Desktop- und .vmf-Verknüpfung | Amboss im Startmenü, Desktop-Symbol mit Amboss-Icon, Doppelklick auf .vmf öffnet Amboss; Deinstallation über „Apps“ entfernt alles außer eigenen Maps |
| P9 | Einstellungen → Sprache: Français, Español, Polski, Русский nacheinander; Menüs, Panel, Kompilieren-Dialog, Tutorial ansehen | Texte in der Sprache, keine deutschen Reste (Entity-Beschreibungen auf Englisch sind gewollt), kyrillische Schrift sauber |
| P10 | Windows-Skalierung 150 % und 200 %; zusätzlich „Größe der Oberfläche“ 125 % | Alles scharf, 3D/2D-Ansichten füllen ihre Fläche, Klicks treffen; große Dialoge passen ins Fenster und scrollen |

## Q. Version 0.11.0: Ansichtsfilter und 3D-Skybox

| Nr. | Schritt | Erwartung |
|---|---|---|
| Q1 | Echte Map mit Triggern und nodraw öffnen (z. B. eine dekompilierte GMod-Map). **Alt+N** in einer 2D-Ansicht | nodraw-Flächen verschwinden in 3D, auch einzelne nodraw-Seiten normaler Brushes; reine nodraw-Brushes verschwinden auch in 2D; Trichter-Knopf leuchtet, unten „Filter aktiv“ |
| Q2 | **Alt+T** | Alle trigger_* und Brushes mit toolstrigger verschwinden in 2D und 3D; ausgewählte Trigger werden abgewählt |
| Q3 | In 3D auf eine Stelle klicken, an der vorher eine ausgeblendete nodraw-Fläche war | Ausgewählt wird, was dahinter zu sehen ist (Rückseite der gegenüberliegenden Fläche bzw. der Brush dahinter) |
| Q4 | Trichter-Knopf einer Ansicht → einzelne Häkchen (Clips, Hint/Skip, Areaportals, Skybox-Flächen) | Wirkt sofort in allen Ansichten; Fenster bleibt beim Klicken offen |
| Q5 | Map speichern, schließen, wieder öffnen; dieselbe VMF in Hammer öffnen | Filter sind wieder so gesetzt; Hammer öffnet die Map ohne Meldung |
| Q6 | Map ohne 3D-Skybox: Ansicht → 3D-Skybox bearbeiten → Assistent mit Vorschlag → „Anlegen und bearbeiten“ | Raum (Wände mit toolsskybox) und sky_camera entstehen außerhalb der Map; Ansichten zeigen nur den Raum; unten rechts im 3D-Fenster die Spielersicht |
| Q7 | Vorher in der Map ein paar große Brushes als „Berge“ auswählen, dann Skybox-Modus → „Map-Auswahl verkleinert hineinkopieren“ | Kopien erscheinen 1:16 verkleinert im Raum; in der Spielersicht stehen sie hinter der Map an der richtigen Stelle |
| Q8 | In der Spielersicht ziehen und das Mausrad drehen | Sicht dreht/bewegt sich; das große 3D-Fenster bleibt unverändert |
| Q9 | Skybox-Modus → „Prüfen“; dann eine Wand des Raums löschen und erneut prüfen | Zuerst keine Fehler; danach „Skybox-Raum nicht geschlossen“ und der Modus beendet sich |
| Q10 | Map kompilieren und in GMod laden | Die kopierten Berge sind am Horizont zu sehen, an der Stelle wie in der Spielersicht; keine Lecks durch den Skybox-Raum |

## R. Version 0.12.0: Modell-Import, Texturen, Workshop, Splines

| Nr. | Schritt | Erwartung |
|---|---|---|
| R1 | Ein Modell aus Blender als .glb exportieren (mit Textur) und ins Amboss-Fenster ziehen | Dialog „Modell importieren“ mit sinnvoller Größe (Meter, Y oben), Vorschau, Material mit gefundener Textur |
| R2 | „Importieren und platzieren“ | studiomdl läuft durch, ein prop_static steht dort, wohin die 3D-Kamera schaut, mit Textur im 3D-Fenster und im Spiel; Spieler kollidiert mit der Hülle |
| R3 | Dasselbe mit einer .obj + .mtl + PNG/JPEG; einmal als prop_physics | Wie R2; prop_physics fällt im Spiel und lässt sich anfassen |
| R4 | Material-Editor → „PBR-Set importieren…“ mit einem Set von ambientCG oder Poly Haven (Color, NormalGL, Roughness, AO) → Speichern, auf einen Brush legen | Basistextur, Normalmap und Glanz im Spiel richtig (Relief nicht invertiert: Beleuchtung von oben hellt die Oberkanten auf) |
| R5 | „Nahtlos machen“ bei einer kantigen Textur, „Gekachelt“ ansehen, speichern | Keine sichtbaren Kanten mehr beim Kacheln, auch im Spiel |
| R6 | Map kompilieren → Kompilieren → „Im Workshop veröffentlichen…“ → „Aus 3D-Ansicht“, Tags wählen → „Nur .gma erstellen“ | `.gma` neben der Map in `amboss_workshop`, enthält maps/<map>.bsp und maps/thumb/<map>.png |
| R7 | Mit laufendem Steam „Veröffentlichen“ (Testaddon, danach auf der Workshop-Seite versteckt lassen oder löschen) | gmpublish lädt hoch, die Workshop-ID erscheint, „Seite öffnen“ zeigt das Addon mit Vorschaubild; ein zweiter Upload läuft als Update |
| R8 | Werkzeuge → Spline → in 3D vier Punkte auf den Boden klicken → Straße erzeugen | Fahrbahn mit Bordsteinen folgt der Kurve, Textur läuft entlang, keine Lücken zwischen den Stücken; kompiliert ohne Fehler |
| R10 | Boden aus mehreren verschieden großen Brushes bauen (einige mit „Texturen fixieren“ verschieben), Textur auflegen → alle auswählen → Strg+Shift+A | Keine Kanten mehr zwischen den Platten, in 3D und im Spiel; Strg+Z stellt den alten Zustand her |
| R9 | Dieselben Punkte → Rohr, Zaun, Modelle (Laterne, beidseitig, seitlich versetzt), Kabel; jeweils „Neu erzeugen“ | Jeweils sauber erzeugt; „Neu erzeugen“ ersetzt das vorherige Ergebnis; Kabel hängt im Spiel zwischen den Punkten |

## S. Version 0.13.0

| Nr. | Schritt | Erwartung |
|---|---|---|
| S1 | 0.12.1 starten, über den Update-Hinweis auf 0.13.0 aktualisieren und neu starten | Fenster „Neu in Amboss 0.13.0“ mit den Neuerungen von 0.13.0; beim nächsten Start nicht mehr; Hilfe → „Neuigkeiten (Patchnotes)…“ zeigt alle Versionen |
| S2 | Sprache Englisch, dann Hilfe → „What's new (patch notes)…“ | Nach kurzem Laden die englischen Release-Texte von GitHub |
| S3 | Eine Map mit absichtlichem Loch kompilieren | Unter „Was bedeutet das?“ steht „Die Map ist nicht dicht (Leck)“ mit „Leck-Pfad laden“; der Knopf zeigt die rote Linie |
| S4 | Map mit fehlender Textur und einem prop_static mit Physik-Modell kompilieren | Erklärungen „Textur fehlt“ und „Dieses Modell geht nicht als prop_static“ |
| S5 | Eine alte Half-Life-`.rmf` (Hammer 3.x) per Datei → Öffnen laden | Brushes und Entities da, Statuszeile nennt die GoldSrc-Texturen; nach „Texturen ersetzen“ kompilierbar |
| S6 | Spline → „Auswahl entlang“: einen kleinen Zaunabschnitt (Brushes) als Vorlage übernehmen, Kurve klicken, erzeugen | Kopien folgen der Kurve, ausgerichtet; „Neu erzeugen“ ersetzt sie |
| S7 | 3D-Darstellung „Lightmap-Dichte“, einer Fläche Lightmap-Skalierung 4 geben | Fläche wird rot, die geschätzte MB-Zahl unten rechts steigt |

## L. Nach dem Test

* Ergebnisliste (IDs mit OK/Fehler) und gesammelte Logs/Screenshots zurückschicken.
* Besonders wichtig für die nächste Runde: **C2** (Material-Namen), **F1/F3/F4** (Optimierungen im Spiel), **I7** (IPv6), **B2** (FPS-Wert), **N10/N12** (Modelle aus studiomdl richtig herum), **N2/N3** (eigene Materialien/Partikel im Spiel), **O3–O8** (Live-Abgleich: konnte ohne GMod nur gegen eine nachgebaute Lua-Umgebung getestet werden).
