//! Eingebautes Tutorial: führt Einsteiger in Kapiteln durch die erste Map.
//!
//! Jeder Schritt erklärt eine Aufgabe, prüft den Editor-Zustand selbst (geht automatisch weiter,
//! sobald die Aufgabe erledigt ist) und kann sie mit „Zeig es mir“ auch vormachen. Werkzeuge und
//! Bereiche, um die es gerade geht, werden in der Oberfläche hervorgehoben.

use crate::app::App;
use crate::editor::Tool;
use eframe::egui::{self, RichText, Vec2};
use glam::Vec3;
use serde_json::json;

/// Stand zu Beginn eines Schritts (zum Vergleichen)
#[derive(Clone, Default)]
pub struct Base {
    solids: usize,
    cam: Vec3,
    material: String,
    textured_faces: usize,
    /// Brushes mit schrägen Flächen
    slanted: usize,
    entities: usize,
    sky: String,
    disps: usize,
}

type Check = fn(&App, &Base) -> bool;
type Auto = fn(&mut App);

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    /// Bereich der Oberfläche, der hervorgehoben wird (Schlüssel in App::ui_rects)
    pub focus: Option<&'static str>,
    /// Aufgabe erledigt? (None = reiner Erklärschritt, „Weiter“ klicken)
    pub check: Option<Check>,
    /// „Zeig es mir“: Aufgabe automatisch ausführen
    pub auto: Option<Auto>,
}

pub struct Chapter {
    pub title: &'static str,
    pub summary: &'static str,
    pub steps: Vec<Step>,
}

fn solids(a: &App) -> usize {
    a.ed.map.all_solids().count()
}
fn has_class(a: &App, c: &str) -> bool {
    a.ed.map.entities.iter().any(|e| e.classname().eq_ignore_ascii_case(c))
}
fn mcp(a: &mut App, name: &str, args: serde_json::Value) {
    if let Err(e) = crate::mcp::call_tool(name, &args, a) {
        a.ed.status = format!("Tutorial: {e}");
    }
}
/// Mittelpunkt des Bodens (oberste Fläche unter dem Ursprung) – für automatisch gesetzte Entities
fn floor_z(a: &App) -> f64 {
    a.ed.map.world.solids.iter().filter(|s| s.bounds.min.x <= 0.0 && s.bounds.max.x >= 0.0 && s.bounds.min.y <= 0.0 && s.bounds.max.y >= 0.0 && s.bounds.max.z <= 64.0).map(|s| s.bounds.max.z).fold(0.0, f64::max)
}
fn textured(a: &App) -> usize {
    let m = a.ed.material.to_ascii_lowercase();
    a.ed.map.all_solids().flat_map(|(_, s)| s.sides.iter()).filter(|sd| sd.material.to_ascii_lowercase() == m).count()
}
/// Brushes mit mindestens einer schrägen Fläche
fn slanted(a: &App) -> usize {
    a.ed.map.all_solids().filter(|(_, s)| s.planes.iter().flatten().any(|p| {
        let n = p.normal.abs();
        n.x > 0.05 && n.x < 0.95 || n.y > 0.05 && n.y < 0.95 || n.z > 0.05 && n.z < 0.95
    })).count()
}
fn disp_count(a: &App) -> usize {
    a.ed.map.all_solids().filter(|(_, s)| s.has_disp()).count()
}
/// Größte Höhe eines Displacement-Punkts über seiner Grundfläche
fn disp_height(a: &App) -> f64 {
    a.ed.map.all_solids().flat_map(|(_, s)| s.disps.iter().flatten()).flat_map(|g| g.pos.iter().zip(&g.base).map(|(p, b)| p.z - b.z)).fold(0.0, f64::max)
}
fn disp_alpha(a: &App) -> bool {
    a.ed.map.all_solids().flat_map(|(_, s)| s.sides.iter()).filter_map(|sd| sd.disp.as_ref()).any(|d| d.alphas.iter().any(|x| *x > 10.0))
}
/// Größte flache Platte (Boden fürs Gelände) und ihre Oberseite
fn ground_face(a: &App) -> Option<(u32, usize)> {
    let s = a.ed.map.world.solids.iter().filter(|s| s.bounds.size().x >= 256.0 && s.bounds.size().y >= 256.0).max_by(|x, y| (x.bounds.size().x * x.bounds.size().y).total_cmp(&(y.bounds.size().x * y.bounds.size().y)))?;
    let i = (0..s.sides.len()).find(|&i| s.planes[i].is_some_and(|p| p.normal.z > 0.9))?;
    Some((s.id, i))
}
/// Pinselstriche über das Gelände (für „Zeig es mir“)
fn paint_strokes(a: &mut App, mode: crate::disp::PaintMode) {
    let Some((sid, _)) = ground_face(a) else { return };
    let Some(c) = a.ed.map.solid(sid).map(|s| s.bounds.center()) else { return };
    a.set_tool(Tool::Disp);
    let old = (a.ed.dbrush.mode, a.ed.dbrush.radius, a.ed.dbrush.strength);
    a.ed.dbrush.mode = mode;
    a.ed.dbrush.radius = 256.0;
    a.ed.dbrush.strength = if mode == crate::disp::PaintMode::Raise { 40.0 } else { 60.0 };
    a.ed.begin_paint();
    // breiter, flacher Hügel: Striche auf einem Kreis um die Mitte
    for k in 0..12 {
        let t = k as f64 / 12.0 * std::f64::consts::TAU;
        let o = glam::DVec3::new(c.x + t.cos() * 140.0, c.y + t.sin() * 140.0, c.z + 2000.0);
        a.ed.paint_disp(o, -glam::DVec3::Z, 0.1);
    }
    (a.ed.dbrush.mode, a.ed.dbrush.radius, a.ed.dbrush.strength) = old;
    a.ed.touch();
}
fn door(a: &App) -> Option<&crate::map::Entity> {
    a.ed.map.entities.iter().map(|e| &**e).find(|e| e.classname().starts_with("func_door"))
}
fn trigger(a: &App) -> Option<&crate::map::Entity> {
    a.ed.map.entities.iter().map(|e| &**e).find(|e| e.classname().starts_with("trigger_"))
}

pub fn chapters() -> Vec<Chapter> {
    vec![
        Chapter {
            title: "1 · Die erste Map",
            summary: "Oberfläche kennenlernen, einen Raum bauen, Kamera bewegen, speichern.",
            steps: vec![
                Step {
                    title: "Willkommen!",
                    text: "Links oben siehst du die 3D-Ansicht, daneben und darunter drei 2D-Ansichten von oben, vorn und von der Seite. In 2D baust du genau auf dem Raster, in 3D schaust du dir das Ergebnis an. Links ist die Werkzeugleiste, rechts stehen die Eigenschaften und unten rechts die Texturen.",
                    focus: Some("view3d"),
                    check: None,
                    auto: None,
                },
                Step {
                    title: "Eine neue Map",
                    text: "Lege eine leere Map an: Datei → Neu (Strg+N). Sie öffnet sich in einem eigenen Tab.",
                    focus: None,
                    check: Some(|a, _| a.ed.path.is_none() && a.ed.map.world.solids.is_empty() && a.ed.map.entities.is_empty()),
                    auto: Some(|a| a.new_tab()),
                },
                Step {
                    title: "Das Block-Werkzeug",
                    text: "Mit dem Block-Werkzeug baust du Wände, Böden und alles Feste. Klicke es in der Werkzeugleiste an oder drücke Shift+B.",
                    focus: Some("tool:block"),
                    check: Some(|a, _| a.ed.tool == Tool::Block),
                    auto: Some(|a| a.set_tool(Tool::Block)),
                },
                Step {
                    title: "Einen großen Block aufziehen",
                    text: "Ziehe in der Ansicht „Oben“ mit gedrückter linker Maustaste ein großes Rechteck auf (etwa 512 × 512). In „Vorn“ oder „Seite“ kannst du die Höhe ziehen (etwa 256). Drücke dann Enter.",
                    focus: Some("view_top"),
                    check: Some(|a, b| solids(a) > b.solids && a.ed.map.all_solids().any(|(_, s)| s.bounds.size().x >= 128.0 && s.bounds.size().y >= 128.0)),
                    auto: Some(|a| {
                        a.set_tool(Tool::Block);
                        mcp(a, "create_brush", json!({"min": [-256, -256, 0], "max": [256, 256, 256], "material": "DEV/DEV_MEASUREGENERIC01"}));
                        let ids: Vec<u32> = a.ed.map.world.solids.iter().map(|s| s.id).collect();
                        if let Some(id) = ids.last() {
                            a.ed.sel = [crate::editor::Obj::Solid(*id)].into_iter().collect();
                        }
                    }),
                },
                Step {
                    title: "Aushöhlen – ein Raum entsteht",
                    text: "Ein Raum ist ein hohler Block. Wähle den Block mit dem Auswahl-Werkzeug (Shift+S) und wähle Objekt → Brush-Operationen → Aushöhlen (Strg+H). Aus dem Block werden sechs Wände.",
                    focus: None,
                    check: Some(|a, b| solids(a) >= b.solids + 5),
                    auto: Some(|a| {
                        if a.ed.sel.is_empty() {
                            if let Some(id) = a.ed.map.world.solids.last().map(|s| s.id) {
                                a.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                            }
                        }
                        a.ed.hollow(16.0);
                    }),
                },
                Step {
                    title: "Umsehen in 3D",
                    text: "Halte in der 3D-Ansicht die mittlere Maustaste (Mausrad drücken) gedrückt und bewege die Maus, um dich umzusehen. Mit W, A, S, D fliegst du dabei herum. Fliege in deinen Raum hinein. (Die Tasten lassen sich unter Einstellungen → Tastenkürzel ändern.)",
                    focus: Some("view3d"),
                    check: Some(|a, b| a.view3d.cam.pos.distance(b.cam) > 64.0),
                    auto: Some(|a| {
                        a.view3d.cam.pos = Vec3::new(-200.0, -200.0, 128.0);
                        a.view3d.cam.look_at(Vec3::new(0.0, 0.0, 64.0));
                    }),
                },
                Step {
                    title: "Speichern",
                    text: "Speichere die Map: Datei → Speichern (Strg+S). Leg sie am besten in einen eigenen Ordner, z. B. „Dokumente/Maps“.",
                    focus: None,
                    check: Some(|a, _| a.ed.path.is_some() && !a.ed.dirty),
                    auto: Some(|a| {
                        a.save();
                    }),
                },
            ],
        },
        Chapter {
            title: "2 · Texturen und Formen",
            summary: "Texturen wählen und auftragen, Kisten bauen, kopieren und mit dem Clip-Werkzeug schneiden.",
            steps: vec![
                Step {
                    title: "Eine Textur wählen",
                    text: "Unten rechts siehst du die Texturen. Klicke eine an, zum Beispiel eine Ziegelwand. Mit „Browser…“ kannst du alle Texturen durchsuchen.",
                    focus: Some("textures"),
                    check: Some(|a, b| a.ed.material != b.material),
                    auto: Some(|a| a.ed.material = "BRICK/BRICKWALL001A".into()),
                },
                Step {
                    title: "Textur auftragen",
                    text: "Wähle eine Wand mit dem Auswahl-Werkzeug und drücke Shift+T (Bearbeiten → Aktuelle Textur anwenden). Für einzelne Flächen gibt es das Textur-Werkzeug (Shift+A): Textur wählen und Flächen in 3D mit Rechtsklick bemalen; ein Linksklick nimmt die Textur einer Fläche auf.",
                    focus: Some("tool:face"),
                    check: Some(|a, b| textured(a) > b.textured_faces),
                    auto: Some(|a| {
                        if a.ed.sel.is_empty() {
                            if let Some(id) = a.ed.map.world.solids.first().map(|s| s.id) {
                                a.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                            }
                        }
                        a.ed.apply_material();
                    }),
                },
                Step {
                    title: "Eine Kiste bauen",
                    text: "Nimm wieder das Block-Werkzeug (Shift+B) und ziehe im Raum einen kleinen Block auf, etwa 64 × 64 × 64. Enter bestätigt.",
                    focus: Some("tool:block"),
                    check: Some(|a, b| solids(a) > b.solids),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_brush", json!({"min": [64, 64, z], "max": [128, 128, z + 64.0], "material": "DEV/DEV_MEASUREGENERIC01"}));
                    }),
                },
                Step {
                    title: "Kopieren",
                    text: "Wähle die Kiste (Shift+S, dann anklicken) und drücke Strg+D, um sie zu duplizieren. Die Kopie kannst du in 2D an einen anderen Platz ziehen.",
                    focus: None,
                    check: Some(|a, b| solids(a) > b.solids),
                    auto: Some(|a| {
                        if let Some(id) = a.ed.map.world.solids.last().map(|s| s.id) {
                            a.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                        }
                        a.ed.duplicate_in_place();
                        let g = a.ed.grid.max(16.0) * 4.0;
                        a.ed.translate_selection(glam::DVec3::new(g, 0.0, 0.0));
                    }),
                },
                Step {
                    title: "Schneiden mit dem Clip-Werkzeug",
                    text: "Das Clip-Werkzeug (Shift+X) schneidet Blöcke entlang einer Linie – so entstehen Schrägen und Rampen. Wähle eine Kiste, nimm das Clip-Werkzeug und ziehe in einer 2D-Ansicht schräg über die Kiste. Enter schneidet.",
                    focus: Some("tool:clip"),
                    check: Some(|a, _| a.ed.map.all_solids().any(|(_, s)| s.planes.iter().flatten().any(|p| {
                        let n = p.normal.abs();
                        n.x > 0.05 && n.x < 0.95 || n.y > 0.05 && n.y < 0.95 || n.z > 0.05 && n.z < 0.95
                    }))),
                    auto: None,
                },
            ],
        },
        Chapter {
            title: "3 · Entities",
            summary: "Spawnpunkt, Licht und ein Prop setzen und ihre Eigenschaften ändern.",
            steps: vec![
                Step {
                    title: "Was sind Entities?",
                    text: "Entities sind alles, was nicht einfach feste Geometrie ist: Spawnpunkte, Lichter, Props, Türen, Trigger, Sounds… Gesetzt werden sie mit dem Entity-Werkzeug (Shift+E). Die Klasse wählst du rechts im Panel: ins Feld tippen (mit Vorschlägen) oder über den Pfeil daneben aus der Liste.",
                    focus: Some("tool:entity"),
                    check: None,
                    auto: None,
                },
                Step {
                    title: "Ein Spawnpunkt",
                    text: "Ohne Spawnpunkt kann man die Map nicht betreten. Nimm das Entity-Werkzeug, wähle die Klasse „info_player_start“ und klicke in 3D auf den Boden des Raums.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| has_class(a, "info_player_start")),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_entity", json!({"classname": "info_player_start", "origin": [-128, -128, z]}));
                    }),
                },
                Step {
                    title: "Licht",
                    text: "Ohne Licht ist die Map im Spiel stockdunkel. Setze ein Entity der Klasse „light“ in die Mitte des Raums, etwas unter die Decke.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| a.ed.map.entities.iter().any(|e| e.classname().starts_with("light"))),
                    auto: Some(|a| {
                        let z = floor_z(a) + 192.0;
                        mcp(a, "create_entity", json!({"classname": "light", "origin": [0, 0, z]}));
                    }),
                },
                Step {
                    title: "Eigenschaften ändern",
                    text: "Wähle das Licht aus. Rechts im Panel stehen jetzt seine Eigenschaften. Ändere die Helligkeit „Brightness“ – zum Beispiel über das Farbfeld – auf ein warmes Gelb.",
                    focus: Some("panel"),
                    check: Some(|a, _| a.ed.map.entities.iter().any(|e| e.classname() == "light" && e.get("_light").is_some_and(|v| !v.starts_with("255 255 255")))),
                    auto: Some(|a| {
                        if let Some(id) = a.ed.map.entities.iter().find(|e| e.classname() == "light").map(|e| e.id) {
                            mcp(a, "set_entity_keys", json!({"id": id, "keys": {"_light": "255 214 160 300"}}));
                        }
                    }),
                },
                Step {
                    title: "Ein Prop",
                    text: "Props sind Modelle, zum Beispiel Kisten, Fässer oder Stühle. Setze ein „prop_physics“ und wähle beim Feld „World Model“ über „…“ im Modell-Browser ein Modell aus.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| a.ed.map.entities.iter().any(|e| e.classname().starts_with("prop_") && e.get("model").is_some_and(|m| !m.is_empty()))),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_entity", json!({"classname": "prop_physics", "origin": [-64, 96, z + 16.0], "keys": {"model": "models/props_junk/wood_crate001a.mdl"}}));
                    }),
                },
            ],
        },
        Chapter {
            title: "4 · Tür und Trigger",
            summary: "Eine Schiebetür bauen, einen Trigger setzen und beide mit einem Ausgang verknüpfen.",
            steps: vec![
                Step {
                    title: "Eine Tür aus einem Block",
                    text: "Baue einen schmalen Block als Türblatt (etwa 8 × 64 × 112) und wandle ihn um: Objekt → In Entity umwandeln (Strg+T) und als Klasse „func_door“ wählen.",
                    focus: None,
                    check: Some(|a, _| door(a).is_some()),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_brush", json!({"min": [200, -32, z], "max": [208, 32, z + 112.0], "material": "DEV/DEV_MEASUREGENERIC01"}));
                        if let Some(id) = a.ed.map.world.solids.last().map(|s| s.id) {
                            mcp(a, "tie_to_entity", json!({"ids": [id], "classname": "func_door"}));
                        }
                    }),
                },
                Step {
                    title: "Name und Richtung",
                    text: "Gib der Tür rechts im Panel den Namen „tuer“. Bei „Move Direction“ legst du fest, wohin sie fährt – zum Beispiel „-90 0 0“ für nach oben. Der gelbe Pfeil zeigt die Bewegung.",
                    focus: Some("panel"),
                    check: Some(|a, _| door(a).is_some_and(|d| d.targetname().is_some_and(|n| !n.is_empty()))),
                    auto: Some(|a| {
                        if let Some(id) = door(a).map(|d| d.id) {
                            mcp(a, "set_entity_keys", json!({"id": id, "keys": {"targetname": crate::i18n::tr("tuer"), "movedir": "-90 0 0"}}));
                            a.ed.sel = [crate::editor::Obj::Entity(id)].into_iter().collect();
                        }
                    }),
                },
                Step {
                    title: "Ein Trigger",
                    text: "Ein Trigger ist ein unsichtbarer Bereich, der etwas auslöst. Wähle die Textur „TOOLS/TOOLSTRIGGER“, baue einen Block vor die Tür und wandle ihn mit Strg+T in „trigger_multiple“ um.",
                    focus: None,
                    check: Some(|a, _| trigger(a).is_some()),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_brush", json!({"min": [96, -64, z], "max": [192, 64, z + 96.0], "material": "TOOLS/TOOLSTRIGGER"}));
                        if let Some(id) = a.ed.map.world.solids.last().map(|s| s.id) {
                            mcp(a, "tie_to_entity", json!({"ids": [id], "classname": "trigger_multiple"}));
                        }
                    }),
                },
                Step {
                    title: "Verknüpfen: der Ausgang",
                    text: "Wähle den Trigger. Rechts unter „Ausgänge“: „+ Output hinzufügen“ → „OnStartTouch“, als Ziel „tuer“ und als Input „Open“. Jetzt öffnet sich die Tür, sobald jemand den Trigger betritt.",
                    focus: Some("panel"),
                    check: Some(|a, _| {
                        let name = door(a).and_then(|d| d.targetname()).unwrap_or("").to_ascii_lowercase();
                        trigger(a).is_some_and(|t| t.outputs.iter().any(|(_, v)| {
                            let p: Vec<String> = v.split(['\u{1b}', ',']).map(|s| s.trim().to_ascii_lowercase()).collect();
                            !name.is_empty() && p.first().is_some_and(|x| *x == name) && p.get(1).is_some_and(|x| x == "open")
                        }))
                    }),
                    auto: Some(|a| {
                        let name = door(a).and_then(|d| d.targetname()).unwrap_or(crate::i18n::tr("tuer")).to_string();
                        if let Some(id) = trigger(a).map(|t| t.id) {
                            mcp(a, "add_output", json!({"id": id, "output": "OnStartTouch", "target": name, "input": "Open"}));
                            a.ed.sel = [crate::editor::Obj::Entity(id)].into_iter().collect();
                        }
                    }),
                },
            ],
        },
        Chapter {
            title: "5 · Kompilieren und testen",
            summary: "Map prüfen, kompilieren und in Garry's Mod ausprobieren.",
            steps: vec![
                Step {
                    title: "Auf Probleme prüfen",
                    text: "Bevor du kompilierst: Map → Auf Probleme prüfen (Alt+P). Amboss findet Lecks, fehlende Spawnpunkte, kaputte Brushes und vieles mehr – oft mit „Beheben“-Knopf.",
                    focus: None,
                    check: Some(|a, _| a.win.problems),
                    auto: Some(|a| {
                        a.problems = crate::problems::check(&a.ed, a.known_materials().as_ref());
                        a.win.problems = true;
                    }),
                },
                Step {
                    title: "Kompilieren & Exportieren",
                    text: "Öffne Kompilieren → Kompilieren & Exportieren… Hier stellst du die Qualität ein und kannst eigene Inhalte in die BSP packen und ein Navmesh erzeugen lassen.",
                    focus: Some("compile"),
                    check: Some(|a, _| a.win.compile),
                    auto: Some(|a| a.win.compile = true),
                },
                Step {
                    title: "Los!",
                    text: "Klicke im Dialog auf „Kompilieren & Exportieren“ (oder drücke F9). Das Protokoll zeigt den Fortschritt. Ist „Spiel starten“ angehakt, startet Garry's Mod danach direkt mit deiner Map.",
                    focus: Some("compile"),
                    check: Some(|a, _| a.job.is_some()),
                    auto: Some(|a| a.start_compile()),
                },
                Step {
                    title: "Geschafft!",
                    text: "Deine erste Map ist fertig. In Garry's Mod lädst du sie auch über die Konsole: „map <name>“. Weiter geht es mit den nächsten Kapiteln: Gelände mit Displacements, Licht und Himmel, Feinarbeit und schneller bauen.",
                    focus: None,
                    check: None,
                    auto: None,
                },
            ],
        },
        Chapter {
            title: "6 · Gelände mit Displacements",
            summary: "Eine Fläche in ein Displacement verwandeln, Hügel malen, Texturen überblenden – und große Landschaften aus einer Heightmap oder per Rausch-Generator.",
            steps: vec![
                Step {
                    title: "Was sind Displacements?",
                    text: "Ein Displacement verwandelt eine flache Brush-Fläche in ein feines Raster, dessen Punkte du verschieben kannst – so entstehen Hügel, Täler, Höhlenwände und Wege. Displacements dichten die Map nicht ab: darunter muss immer noch ein Brush oder die Skybox liegen.",
                    focus: Some("tool:disp"),
                    check: None,
                    auto: None,
                },
                Step {
                    title: "Eine Bodenplatte",
                    text: "Baue mit dem Block-Werkzeug (Shift+B) eine große, flache Platte, etwa 1024 × 1024 Einheiten und 16 hoch. Tipp: Die Textur „NATURE/BLENDGRASSGRAVEL001A“ mischt später Gras und Kies.",
                    focus: Some("tool:block"),
                    check: Some(|a, _| ground_face(a).is_some()),
                    auto: Some(|a| {
                        mcp(a, "create_brush", json!({"min": [-512, -512, -16], "max": [512, 512, 0], "material": "NATURE/BLENDGRASSGRAVEL001A"}));
                    }),
                },
                Step {
                    title: "Die Oberseite auswählen",
                    text: "Nimm das Textur-Werkzeug (Shift+A) und klicke mit gedrückter Strg-Taste in der 3D-Ansicht auf die Oberseite der Platte – Strg+Klick wählt Flächen aus. Nur ausgewählte Flächen werden zu Displacements.",
                    focus: Some("tool:face"),
                    check: Some(|a, _| a.ed.faces.iter().any(|(sid, i)| a.ed.map.solid(*sid).and_then(|s| s.planes.get(*i).copied().flatten()).is_some_and(|p| p.normal.z > 0.9))),
                    auto: Some(|a| {
                        a.set_tool(Tool::Face);
                        if let Some(f) = ground_face(a) {
                            a.ed.faces = [f].into_iter().collect();
                            a.ed.touch();
                        }
                    }),
                },
                Step {
                    title: "Displacement erstellen",
                    text: "Rechts im Panel unter „Displacement“: Power 3 lassen und „Erstellen“ klicken. Power bestimmt die Feinheit – 2 ist grob, 4 sehr fein (und teuer).",
                    focus: Some("panel"),
                    check: Some(|a, _| disp_count(a) > 0),
                    auto: Some(|a| {
                        if a.ed.faces.is_empty() {
                            if let Some(f) = ground_face(a) {
                                a.ed.faces = [f].into_iter().collect();
                            }
                        }
                        a.ed.create_disp(3);
                    }),
                },
                Step {
                    title: "Einen Hügel malen",
                    text: "Wähle das Terrain-Werkzeug (Shift+P), Modus „Anheben“. Male mit gedrückter linker Maustaste in der 3D-Ansicht über das Gelände. Radius und Stärke stehen im Panel; Strg kehrt Anheben ↔ Absenken um.",
                    focus: Some("tool:disp"),
                    check: Some(|a, _| disp_height(a) >= 24.0),
                    auto: Some(|a| paint_strokes(a, crate::disp::PaintMode::Raise)),
                },
                Step {
                    title: "Glätten",
                    text: "Stelle im Panel den Modus „Glätten“ ein und male über steile Kanten – sie werden weicher. Mit gedrückter Strg-Taste wird geschärft. Weitere Modi: Auf Höhe setzen, Rauschen, Terrassen.",
                    focus: Some("panel"),
                    check: None,
                    auto: Some(|a| paint_strokes(a, crate::disp::PaintMode::Smooth)),
                },
                Step {
                    title: "Texturen überblenden",
                    text: "Mit einer Blend-Textur (zwei Texturen in einer, z. B. Gras und Kies) malst du im Modus „Alpha malen“ die zweite Textur ins Gelände – etwa einen Weg. „Alpha anzeigen“ im Panel macht die Bemalung blau sichtbar.",
                    focus: Some("panel"),
                    check: Some(|a, _| disp_alpha(a)),
                    auto: Some(|a| paint_strokes(a, crate::disp::PaintMode::AlphaAdd)),
                },
                Step {
                    title: "Große Landschaften: Heightmap",
                    text: "Für große Gelände malst du nicht jede Kachel einzeln: Map → „Gelände aus Heightmap…“ macht aus einem Graustufenbild (Weiß = hoch, Schwarz = tief) ein Gelände aus vielen nahtlosen Displacement-Kacheln. Kantenlänge, Höhe, Kacheln und Power stellst du im Dialog ein.",
                    focus: None,
                    check: Some(|a, _| a.hm.open),
                    auto: Some(|a| a.hm.open = true),
                },
                Step {
                    title: "Gelände per Rauschen",
                    text: "Kein Bild zur Hand? Wähle oben im Dialog „Rauschen“: Amboss erzeugt das Höhenbild selbst. Hügelgröße, Rauheit, Talboden, Bergkämme und Terrassen formen die Landschaft, „Würfeln“ liefert eine neue Form. Mit „Gelände erstellen“ landet es in der Map – „Als PNG speichern…“ hebt das Bild für später auf.",
                    focus: None,
                    check: Some(|a, b| disp_count(a) >= b.disps + 4),
                    auto: Some(|a| {
                        a.hm.open = true;
                        a.hm.mode = 1;
                        let o = crate::heightmap::TerrainOpts { size: 2048.0, height: 384.0, tiles: 4, power: 3, origin: glam::DVec3::new(1024.0, -1024.0, 0.0), material: a.ed.material.clone() };
                        a.hm.opts = Some(o.clone());
                        let h = crate::heightmap::noise_heights(&a.hm.noise, 4 * 8 + 1);
                        crate::heightmap::build(&mut a.ed, &h, &o);
                        a.hm.open = false;
                    }),
                },
                Step {
                    title: "Noch mehr Gelände-Werkzeuge",
                    text: "Im Panel findest du außerdem: Rauschen (zufällige Unebenheit), Nähen (Kanten benachbarter Displacements angleichen), Unterteilen (macht z. B. aus einem Würfel eine Kugel) und das Ändern der Power ohne Formverlust.",
                    focus: Some("panel"),
                    check: None,
                    auto: None,
                },
            ],
        },
        Chapter {
            title: "7 · Licht, Himmel und Nebel",
            summary: "Lichtvorschau nutzen, Sonne und Himmel einrichten, indirektes Licht, Nebel und einen Partikel-Effekt.",
            steps: vec![
                Step {
                    title: "Die Lichtvorschau",
                    text: "Klicke oben links in der 3D-Ansicht auf „3D · Texturiert“ und wähle „Lichtvorschau“. Jetzt siehst du die Lichter der Map mit Schatten – ohne zu kompilieren.",
                    focus: Some("view3d"),
                    check: Some(|a, _| a.view3d.mode == crate::view3d::RenderMode::Lighting),
                    auto: Some(|a| a.view3d.mode = crate::view3d::RenderMode::Lighting),
                },
                Step {
                    title: "Die Sonne: light_environment",
                    text: "Für Außenbereiche setzt du genau ein „light_environment“ (Entity-Werkzeug, Shift+E). Sein Winkel „Pitch“ bestimmt, wie hoch die Sonne steht; „Ambient“ hellt die Schatten auf.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| has_class(a, "light_environment")),
                    auto: Some(|a| {
                        let z = floor_z(a) + 128.0;
                        mcp(a, "create_entity", json!({"classname": "light_environment", "origin": [0, -96, z], "keys": {"angles": "0 45 0", "pitch": "-50", "_light": "255 240 215 450", "_ambient": "130 150 180 80"}}));
                    }),
                },
                Step {
                    title: "Der Himmel",
                    text: "Öffne die Map-Eigenschaften (nichts auswählen, dann „Map-Eigenschaften“ im Panel) und trage bei „skyname“ einen Himmel ein, z. B. „sky_day01_05“. Flächen mit TOOLS/TOOLSSKYBOX zeigen im Spiel diesen Himmel – mit Ansicht → „Himmel-Vorschau“ auch im Editor.",
                    focus: Some("panel"),
                    check: Some(|a, b| a.ed.map.world.get("skyname").is_some_and(|s| !s.is_empty() && s != b.sky)),
                    auto: Some(|a| {
                        a.ed.checkpoint();
                        let next = if a.ed.map.world.get("skyname") == Some("sky_day01_05") { "sky_day01_01" } else { "sky_day01_05" };
                        a.ed.map.world.set("skyname", next);
                        a.view3d.sky_preview = true;
                        a.ed.touch();
                    }),
                },
                Step {
                    title: "Indirektes Licht",
                    text: "Licht prallt im Spiel von Wänden ab und färbt die Umgebung. Schalte es in der Lichtvorschau ein: Ansicht → „Indirektes Licht in der Lichtvorschau (GI)“. Die Berechnung läuft im Hintergrund und wird immer feiner.",
                    focus: Some("view3d"),
                    check: Some(|a, _| a.view3d.gi_on),
                    auto: Some(|a| {
                        a.view3d.mode = crate::view3d::RenderMode::Lighting;
                        a.view3d.gi_on = true;
                    }),
                },
                Step {
                    title: "Nebel",
                    text: "Setze ein „env_fog_controller“, stelle „Fog Enable“ auf Ja und wähle Farbe, Start und Ende. Entfernte Flächen verschwinden im Nebel – das sieht stimmungsvoll aus und spart Leistung.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| crate::view3d::fog_settings(&a.ed).is_some()),
                    auto: Some(|a| {
                        let z = floor_z(a) + 64.0;
                        mcp(a, "create_entity", json!({"classname": "env_fog_controller", "origin": [96, -96, z], "keys": {"fogenable": "1", "fogcolor": "190 200 210", "fogstart": "300", "fogend": "2500", "fogmaxdensity": "0.8"}}));
                    }),
                },
                Step {
                    title: "Ein Partikel-Effekt",
                    text: "Setze ein „info_particle_system“ und wähle bei „Particle System Name“ einen Effekt, z. B. „fire_small_01“ (Partikel-Browser über „…“). Der Effekt läuft direkt im 3D-Fenster; „Start Active“ lässt ihn im Spiel von Anfang an laufen.",
                    focus: Some("tool:entity"),
                    check: Some(|a, _| a.ed.map.entities.iter().any(|e| e.classname() == "info_particle_system" && e.get("effect_name").is_some_and(|v| !v.is_empty()))),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_entity", json!({"classname": "info_particle_system", "origin": [-96, 96, z + 8.0], "keys": {"effect_name": "fire_small_01", "start_active": "1"}}));
                    }),
                },
            ],
        },
        Chapter {
            title: "8 · Feinarbeit: Vertex, Clip, Overlays",
            summary: "Eckpunkte verschieben, schräg schneiden mit drei Punkten, Overlays und Decals auftragen.",
            steps: vec![
                Step {
                    title: "Das Vertex-Werkzeug",
                    text: "Wähle einen Block und drücke Shift+V. An den Ecken erscheinen Griffe: Ziehe einen oberen Eckpunkt in einer 2D-Ansicht zur Seite – so entstehen Schrägen und Dächer. Ungültige Formen (nicht konvex) lehnt Amboss ab.",
                    focus: Some("tool:vertex"),
                    check: Some(|a, b| slanted(a) > b.slanted),
                    auto: None,
                },
                Step {
                    title: "Schräg schneiden mit drei Punkten",
                    text: "Wähle einen Block, nimm das Clip-Werkzeug (Shift+X) und klicke in der 3D-Ansicht nacheinander drei Punkte auf seine Flächen. Die Ebene durch die drei Punkte wird als Vorschau gezeigt; Enter schneidet, Shift+X wechselt, welche Seite bleibt.",
                    focus: Some("tool:clip"),
                    check: Some(|a, b| slanted(a) > b.slanted),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "create_brush", json!({"min": [-160, 64, z], "max": [-64, 160, z + 96.0], "material": "DEV/DEV_MEASUREGENERIC01"}));
                        if let Some(id) = a.ed.map.world.solids.last().map(|s| s.id) {
                            a.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                        }
                        a.set_tool(Tool::Clip);
                        a.ed.clip3.clear();
                        for p in [[-160.0, 64.0, z + 96.0], [-160.0, 160.0, z + 96.0], [-64.0, 64.0, z + 32.0]] {
                            a.ed.clip3_add(glam::DVec3::from_array(p));
                        }
                        a.ed.apply_clip();
                    }),
                },
                Step {
                    title: "Ein Overlay",
                    text: "Overlays sind Bilder, die auf Flächen liegen – Schilder, Risse, Wegmarkierungen. Wähle rechts bei den Texturen eine Overlay-Textur, nimm das Overlay-Werkzeug (Shift+O) und klicke auf eine Fläche. Mit den runden Griffen änderst du Größe und Drehung.",
                    focus: Some("tool:overlay"),
                    check: Some(|a, _| has_class(a, "info_overlay")),
                    auto: Some(|a| {
                        let Some((sid, i)) = ground_face(a).or_else(|| {
                            let s = a.ed.map.world.solids.iter().find(|s| s.planes.iter().flatten().any(|p| p.normal.z > 0.9))?;
                            Some((s.id, (0..s.sides.len()).find(|&i| s.planes[i].is_some_and(|p| p.normal.z > 0.9))?))
                        }) else { return };
                        let Some(s) = a.ed.map.solid(sid) else { return };
                        let side = s.sides[i].id;
                        let p = glam::DVec3::new(s.bounds.center().x, s.bounds.center().y, s.bounds.max.z);
                        a.set_tool(Tool::Overlay);
                        a.ed.material = "DECALS/LAMBDASPRAY_2A".into();
                        a.ed.create_overlay(p, glam::DVec3::Z, side, glam::DVec2::new(64.0, 64.0));
                    }),
                },
                Step {
                    title: "Ein Decal",
                    text: "Decals sind kleine Bilder wie Einschusslöcher oder Flecken. Nimm das Decal-Werkzeug (Shift+D), wähle eine Textur aus „decals/…“ und klicke auf eine Wand. Anders als Overlays haben Decals eine feste Größe.",
                    focus: Some("tool:decal"),
                    check: Some(|a, _| has_class(a, "infodecal")),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        a.set_tool(Tool::Decal);
                        a.ed.material = "DECALS/LAMBDASPRAY_2A".into();
                        a.ed.create_decal(glam::DVec3::new(32.0, 32.0, z + 1.0));
                    }),
                },
                Step {
                    title: "Sauber arbeiten",
                    text: "Hilfreich für die Feinarbeit: Alt+V rastet an Eckpunkten anderer Objekte ein, Alt+G schaltet das Raster-Einrasten ab, H blendet die Auswahl aus, und mit Strg+G gruppierst du Teile, die zusammengehören.",
                    focus: None,
                    check: None,
                    auto: None,
                },
            ],
        },
        Chapter {
            title: "9 · Schneller bauen und testen",
            summary: "Fertige Bauteile aus der Prefab-Bibliothek, Räume per Generator, durch die Map laufen und im Spiel testen.",
            steps: vec![
                Step {
                    title: "Die Prefab-Bibliothek",
                    text: "Öffne Datei → Prefabs… Hier liegen fertige Bauteile: Türen mit Knöpfen, Leitern, Lampen, Treppen, ganze Räume. Eigene Prefabs speicherst du aus der Auswahl.",
                    focus: None,
                    check: Some(|a, _| a.win.prefabs),
                    auto: Some(|a| a.win.prefabs = true),
                },
                Step {
                    title: "Ein Prefab einsetzen",
                    text: "Ziehe eine Kachel in die 3D-Ansicht auf den Boden – das Prefab richtet sich an der Fläche aus. Alternativ: Kachel anklicken, dann in eine Ansicht klicken (Shift = mehrmals).",
                    focus: Some("view3d"),
                    check: Some(|a, b| solids(a) > b.solids || a.ed.map.entities.len() > b.entities),
                    auto: Some(|a| {
                        let z = floor_z(a);
                        mcp(a, "insert_prefab", json!({"prefab": "Säule", "position": [160, 160, z], "surface_normal": [0, 0, 1]}));
                    }),
                },
                Step {
                    title: "Räume per Generator",
                    text: "Werkzeuge → Prozedurale Generatoren… erzeugt Labyrinthe, Räume mit Gängen oder ganze Stadtblöcke – abgedichtet und sofort kompilierbar. Probiere „Räume“ mit „Erzeugen“; ein Rückgängig-Schritt nimmt alles wieder weg.",
                    focus: None,
                    check: Some(|a, b| solids(a) >= b.solids + 10),
                    auto: Some(|a| {
                        let g = a.gen.clone();
                        crate::procgen::dungeon(&mut a.ed, &g, glam::DVec3::new(2048.0, 0.0, 0.0));
                    }),
                },
                Step {
                    title: "Durch die Map laufen",
                    text: "Drücke F6: Jetzt läufst du mit Spielergröße durch die Map – WASD, Leertaste springt, Strg duckt. So prüfst du schnell, ob Türen breit genug und Stufen nicht zu hoch sind. Esc beendet.",
                    focus: Some("view3d"),
                    check: Some(|a, _| a.view3d.walker.on),
                    auto: Some(|a| a.toggle_walk()),
                },
                Step {
                    title: "Im Spiel testen",
                    text: "Shift+F9 kompiliert und startet Garry's Mod so, dass du dort stehst, wo die 3D-Kamera ist. Mit Kompilieren → „Live-Abgleich mit dem Spiel…“ erscheinen Änderungen sogar sofort im laufenden Spiel.",
                    focus: Some("compile"),
                    check: None,
                    auto: None,
                },
            ],
        },
    ]
}

/// Laufendes Tutorial
#[derive(Default)]
pub struct TutorialState {
    pub chapter: Option<usize>,
    pub step: usize,
    pub base: Base,
    /// Aufgabe erledigt (seit wann) – kurz grün zeigen, dann weiter
    done_at: Option<std::time::Instant>,
}

impl App {
    fn tut_base(&self) -> Base {
        Base { solids: solids(self), cam: self.view3d.cam.pos, material: self.ed.material.clone(), textured_faces: textured(self), slanted: slanted(self), entities: self.ed.map.entities.len(), sky: self.ed.map.world.get("skyname").unwrap_or("").to_string(), disps: disp_count(self) }
    }

    pub(crate) fn start_chapter(&mut self, c: usize) {
        self.tut.chapter = Some(c);
        self.tut.step = 0;
        self.tut.done_at = None;
        self.tut.base = self.tut_base();
        self.win.tutorial = false;
        self.win.start = false;
    }

    fn tut_goto(&mut self, step: usize) {
        self.tut.step = step;
        self.tut.done_at = None;
        self.tut.base = self.tut_base();
    }

    /// Kapitelauswahl (Hilfe → Tutorial)
    pub(crate) fn tutorial_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        self.tutorial_overlay(ctx);
        if !self.win.tutorial {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut start = None;
        let chapters = chapters();
        let done = self.settings.tutorial_done.clone();
        let d = theme::Dlg::new(crate::i18n::tr("Tutorial"), 600.0).height(640.0).subtitle(crate::i18n::tr_dyn(&format!("{} von {} Kapiteln erledigt", done.len(), chapters.len()))).with_id("tutorial_chapters").centered();
        theme::dialog_simple(ctx, &mut open, &d, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 14)).show(ui, |ui| {
                ui.label(RichText::new(crate::i18n::tr("Kapitel 1–5 führen Schritt für Schritt durch die erste Map, Kapitel 6–9 zeigen weitere Werkzeuge. Jeder Schritt prüft selbst, ob er erledigt ist – und mit „Zeig es mir“ macht Amboss ihn vor.")).color(th.text2));
                ui.add_space(10.0);
                for (i, c) in chapters.iter().enumerate() {
                    egui::Frame::new().fill(th.raised).corner_radius(8).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            theme::severity_mark(ui, if done.contains(&i) { 3 } else { 2 });
                            let w = ui.available_width() - 110.0;
                            ui.vertical(|ui| {
                                ui.set_max_width(w);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(crate::i18n::tr(c.title)).strong());
                                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Schritte", c.steps.len()))).size(11.5).color(th.text3));
                                });
                                ui.label(RichText::new(crate::i18n::tr(c.summary)).size(12.0).color(th.text3));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if done.contains(&i) { crate::i18n::tr("Wiederholen") } else { crate::i18n::tr("Starten") };
                                if ui.button(label).clicked() {
                                    start = Some(i);
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
        });
        if let Some(i) = start {
            self.start_chapter(i);
        }
        self.win.tutorial = open && start.is_none();
    }

    /// Schritt-Karte und Hervorhebung während eines Kapitels
    fn tutorial_overlay(&mut self, ctx: &egui::Context) {
        use crate::theme;
        let Some(ci) = self.tut.chapter else { return };
        let chapters = chapters();
        let Some(ch) = chapters.get(ci) else {
            self.tut.chapter = None;
            return;
        };
        let si = self.tut.step.min(ch.steps.len() - 1);
        let step = &ch.steps[si];
        let th = theme::t();
        // erledigt? → kurz grün, dann automatisch weiter
        let ok = step.check.is_some_and(|c| c(self, &self.tut.base));
        if ok && self.tut.done_at.is_none() {
            self.tut.done_at = Some(std::time::Instant::now());
        }
        let advance_auto = self.tut.done_at.is_some_and(|t| t.elapsed().as_secs_f32() > 1.2);
        if self.tut.done_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // Hervorhebung
        if let Some(r) = step.focus.and_then(|f| self.ui_rects.get(f)).copied() {
            let t = ctx.input(|i| i.time) as f32;
            let a = 0.55 + 0.45 * (t * 4.0).sin();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("tut_focus")));
            p.rect_stroke(r.expand(3.0), 8.0, egui::Stroke::new(3.0f32, theme::alpha(th.accent, a)), egui::StrokeKind::Outside);
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
        let (mut next, mut back, mut quit, mut show) = (false, false, false, false);
        let last = si + 1 == ch.steps.len();
        egui::Window::new("tutorial_step")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::LEFT_BOTTOM, [66.0, -40.0])
            .fixed_size([400.0, 0.0])
            .frame(egui::Frame::new().fill(th.panel).stroke(egui::Stroke::new(1.5f32, th.accent)).corner_radius(12).inner_margin(16).shadow(egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: th.shadow }))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(crate::i18n::tr(ch.title)).size(11.5).color(th.accent));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text(crate::i18n::tr("Tutorial beenden")).clicked() {
                            quit = true;
                        }
                        ui.label(RichText::new(format!("{} / {}", si + 1, ch.steps.len())).size(11.5).color(th.text3));
                    });
                });
                // Fortschritt
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, th.line);
                let mut done_r = r;
                done_r.set_width(r.width() * (si as f32 + if ok { 1.0 } else { 0.0 }) / ch.steps.len() as f32);
                ui.painter().rect_filled(done_r, 2.0, th.accent);
                ui.add_space(8.0);
                ui.label(RichText::new(crate::i18n::tr(step.title)).size(16.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new(crate::i18n::tr(step.text)).size(13.0).color(th.text2));
                ui.add_space(10.0);
                if ok {
                    ui.horizontal(|ui| {
                        theme::severity_mark(ui, 3);
                        ui.label(RichText::new(crate::i18n::tr("Erledigt!")).color(th.ok).strong());
                    });
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    if si > 0 && ui.button(crate::i18n::tr("Zurück")).clicked() {
                        back = true;
                    }
                    if step.auto.is_some() && !ok && ui.button(crate::i18n::tr("Zeig es mir")).on_hover_text(crate::i18n::tr("Amboss erledigt diesen Schritt für dich")).clicked() {
                        show = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if last { crate::i18n::tr("Kapitel abschließen") } else if step.check.is_some() && !ok { crate::i18n::tr("Überspringen") } else { crate::i18n::tr("Weiter") };
                        let primary = step.check.is_none() || ok;
                        let b = if primary { egui::Button::new(RichText::new(label).color(th.accent_fg).strong()).fill(th.accent) } else { egui::Button::new(label) };
                        if ui.add(b).clicked() {
                            next = true;
                        }
                    });
                });
            });
        if show {
            if let Some(f) = step.auto {
                f(self);
            }
        }
        if quit {
            self.tut.chapter = None;
            return;
        }
        if back {
            self.tut_goto(si.saturating_sub(1));
        } else if next || (advance_auto && !last) {
            if last {
                if !self.settings.tutorial_done.contains(&ci) {
                    self.settings.tutorial_done.push(ci);
                }
                self.tut.chapter = None;
                // nächstes Kapitel anbieten
                self.win.tutorial = true;
            } else {
                self.tut_goto(si + 1);
            }
        }
    }
}
