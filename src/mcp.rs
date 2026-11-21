//! Eingebauter MCP-Server (Model Context Protocol): KI-Assistenten können Amboss fernsteuern.
//!
//! * Im Editor läuft ein Streamable-HTTP-Endpunkt auf `http://127.0.0.1:<port>/mcp` (nur lokal).
//! * `amboss --mcp-stdio` spricht MCP über stdin/stdout: verbindet sich mit dem laufenden Editor
//!   oder arbeitet ohne Fenster direkt auf einer VMF-Datei (`--map datei.vmf`).

use crate::compile::{JobState, Settings};
use crate::editor::{ClipMode, Editor, Obj};
use crate::fgd::ClassKind;
use crate::geometry::{Aabb, Plane};
use crate::map::fmt_vec;
use crate::transform::Xform;
use glam::DVec3;
use serde_json::{json, Map as JsonMap, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 3917;
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// Schnittstelle zum Editor (GUI oder ohne Fenster)
pub trait Host {
    fn ed(&mut self) -> &mut Editor;
    fn materials(&self) -> Vec<String>;
    fn models(&self) -> Vec<String>;
    fn save(&mut self, path: Option<PathBuf>) -> Result<String, String>;
    fn open(&mut self, path: PathBuf) -> Result<String, String>;
    fn compile(&mut self) -> Result<String, String>;
    fn compile_status(&self) -> Value;
    fn run_command(&mut self, id: &str) -> Result<String, String>;
    fn commands(&self) -> Vec<(String, String)>;
    fn known_materials(&self) -> Option<std::collections::HashSet<String>>;
    fn prefab_dir(&self) -> PathBuf;
    fn opt_settings(&mut self) -> &mut crate::optimize::OptSettings;
    fn game_fs(&self) -> Option<std::sync::Arc<crate::gamefs::GameFs>>;
    /// Spielordner (…/garrysmod), leer wenn nicht eingestellt
    fn game_dir(&self) -> String {
        String::new()
    }
    /// Auswahl als Modell exportieren/kompilieren (nur im Editor)
    fn make_prop(&mut self, _o: crate::propper::PropOpts, _compile: bool) -> Result<Value, String> {
        Err("Nur im laufenden Editor verfügbar".into())
    }
    /// Modell (glTF/OBJ) importieren: Datei laden, Einstellungen überschreiben, kompilieren und platzieren
    fn import_model(&mut self, _path: Option<PathBuf>, _args: &Value) -> Result<Value, String> {
        Err("Nur im laufenden Editor verfügbar".into())
    }
    /// Ergebnis einer Optimierung anzeigen (Netz, Markierungen, Leck)
    fn show_report(&mut self, r: &crate::optimize::Report) {
        let ed = self.ed();
        ed.opt_links = r.links.clone();
        ed.opt_markers = r.markers.clone();
        if let Some(l) = &r.leak {
            ed.pointfile = l.clone();
        }
        ed.touch();
    }
}

// ------------------------------------------------------------------ Protokoll

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}

/// Verarbeitet eine JSON-RPC-Nachricht. None = Benachrichtigung ohne Antwort.
pub fn handle(msg: &Value, host: &mut dyn Host) -> Option<Value> {
    if let Some(arr) = msg.as_array() {
        let out: Vec<Value> = arr.iter().filter_map(|m| handle(m, host)).collect();
        return (!out.is_empty()).then(|| Value::Array(out));
    }
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOLS[0]);
            let version = if PROTOCOLS.contains(&requested) { requested } else { PROTOCOLS[0] };
            ok(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "amboss", "title": "Amboss Map-Editor", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => ok(&id, json!({})),
        "tools/list" => ok(&id, json!({"tools": tool_list()})),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args, host) {
                Ok(v) => ok(&id, json!({"content": [{"type": "text", "text": pretty(&v)}], "structuredContent": wrap(v), "isError": false})),
                Err(e) => ok(&id, json!({"content": [{"type": "text", "text": e}], "isError": true})),
            }
        }
        "resources/list" => ok(&id, json!({"resources": []})),
        "prompts/list" => ok(&id, json!({"prompts": []})),
        _ => err(&id, -32601, &format!("Unbekannte Methode: {method}")),
    })
}

fn wrap(v: Value) -> Value {
    if v.is_object() {
        v
    } else {
        json!({"result": v})
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

const INSTRUCTIONS: &str = "Amboss ist ein Map-Editor für Source-1-Spiele (Garry's Mod). Koordinaten sind Source-Einheiten \
(1 Einheit ≈ 2,54 cm, Z zeigt nach oben). Spieler: 32×32×72, Türen typ. 64×112, Treppenstufen max. 18 hoch. \
Eine Map muss vollständig dicht sein (keine Lecks), braucht info_player_start und Licht (light/light_environment). \
Materialien heißen z. B. DEV/DEV_MEASUREGENERIC01B, TOOLS/TOOLSNODRAW, TOOLS/TOOLSSKYBOX, TOOLS/TOOLSTRIGGER. \
Objekte werden über ihre numerische ID angesprochen. Alle Änderungen sind mit 'undo' rückgängig machbar.";

fn schema(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

fn vec3_schema(desc: &str) -> Value {
    json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": desc})
}

fn ids_schema() -> Value {
    json!({"type": "array", "items": {"type": "integer"}, "description": "IDs von Brushes oder Entities"})
}

pub fn tool_list() -> Vec<Value> {
    let t = |name: &str, desc: &str, input: Value| json!({"name": name, "description": desc, "inputSchema": input});
    vec![
        t("get_map_info", "Überblick: Datei, Anzahl Brushes/Entities, Ausdehnung, Auswahl, Raster.", schema(json!({}), &[])),
        t(
            "list_entities",
            "Entities auflisten (optional nach Klasse/Name filtern).",
            schema(json!({"classname": {"type": "string"}, "name": {"type": "string"}, "limit": {"type": "integer"}}), &[]),
        ),
        t("get_entity", "Alle Schlüssel, Outputs und Brushes einer Entity.", schema(json!({"id": {"type": "integer"}}), &["id"])),
        t(
            "create_entity",
            "Punkt-Entity erzeugen, z. B. info_player_start, light, prop_physics (keys.model = models/...mdl).",
            schema(
                json!({"classname": {"type": "string"}, "origin": vec3_schema("Position"), "angles": vec3_schema("Pitch Yaw Roll"), "keys": {"type": "object", "additionalProperties": {"type": "string"}}}),
                &["classname", "origin"],
            ),
        ),
        t(
            "set_entity_keys",
            "Schlüssel einer Entity setzen (leerer Wert entfernt den Schlüssel). id 1 = worldspawn.",
            schema(json!({"id": {"type": "integer"}, "keys": {"type": "object", "additionalProperties": {"type": "string"}}}), &["id", "keys"]),
        ),
        t(
            "add_output",
            "Entity-I/O-Verbindung hinzufügen (z. B. OnPressed → tür, Open).",
            schema(
                json!({"id": {"type": "integer"}, "output": {"type": "string"}, "target": {"type": "string"}, "input": {"type": "string"}, "parameter": {"type": "string"}, "delay": {"type": "number"}, "times": {"type": "integer"}}),
                &["id", "output", "target", "input"],
            ),
        ),
        t(
            "list_brushes",
            "Brushes auflisten (optional nur innerhalb einer Box).",
            schema(json!({"min": vec3_schema("Box-Minimum"), "max": vec3_schema("Box-Maximum"), "limit": {"type": "integer"}}), &[]),
        ),
        t(
            "create_brush",
            "Brush(es) erzeugen. shape: block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiralstairs, ramp, pipe, dome, fence.",
            schema(
                json!({"min": vec3_schema("Minimum"), "max": vec3_schema("Maximum"), "material": {"type": "string"}, "shape": {"type": "string"}, "sides": {"type": "integer", "description": "Seiten/Segmente/Stufen"}, "wall": {"type": "number"}, "arc": {"type": "number"}, "start_angle": {"type": "number", "description": "Bei Treppe/Rampe: Richtung 0=+X, 90=+Y, 180=-X, 270=-Y"}}),
                &["min", "max"],
            ),
        ),
        t(
            "create_room",
            "Geschlossenen Raum erzeugen (Block aushöhlen). Die Box ist die Außenkante.",
            schema(json!({"min": vec3_schema("Außen-Minimum"), "max": vec3_schema("Außen-Maximum"), "wall_thickness": {"type": "number"}, "material": {"type": "string"}}), &["min", "max"]),
        ),
        t("move_objects", "Objekte verschieben.", schema(json!({"ids": ids_schema(), "offset": vec3_schema("Versatz")}), &["ids", "offset"])),
        t(
            "rotate_objects",
            "Objekte um eine Achse drehen (um ihre gemeinsame Mitte oder center).",
            schema(json!({"ids": ids_schema(), "axis": {"type": "string", "enum": ["x", "y", "z"]}, "degrees": {"type": "number"}, "center": vec3_schema("Drehpunkt")}), &["ids", "axis", "degrees"]),
        ),
        t("resize_objects", "Auswahl auf eine neue Bounding Box skalieren.", schema(json!({"ids": ids_schema(), "min": vec3_schema("neues Minimum"), "max": vec3_schema("neues Maximum")}), &["ids", "min", "max"])),
        t("delete_objects", "Objekte löschen.", schema(json!({"ids": ids_schema()}), &["ids"])),
        t("set_material", "Material auf alle Flächen der Brushes legen.", schema(json!({"ids": ids_schema(), "material": {"type": "string"}}), &["ids", "material"])),
        t(
            "replace_material",
            "Material in der ganzen Map ersetzen.",
            schema(json!({"find": {"type": "string"}, "replace": {"type": "string"}, "partial": {"type": "boolean"}}), &["find", "replace"]),
        ),
        t(
            "tie_to_entity",
            "Brushes in eine Brush-Entity umwandeln (func_detail, trigger_multiple, func_door …).",
            schema(json!({"ids": ids_schema(), "classname": {"type": "string"}}), &["ids", "classname"]),
        ),
        t("carve", "Die angegebenen Brushes aus allen anderen herausschneiden (z. B. Türöffnung).", schema(json!({"ids": ids_schema()}), &["ids"])),
        t(
            "clip_brushes",
            "Brushes an einer Ebene schneiden. keep: both, front (Seite, in die normal zeigt) oder back.",
            schema(json!({"ids": ids_schema(), "point": vec3_schema("Punkt auf der Ebene"), "normal": vec3_schema("Normale"), "keep": {"type": "string", "enum": ["both", "front", "back"]}}), &["ids", "point", "normal"]),
        ),
        t("select", "Objekte im Editor auswählen.", schema(json!({"ids": ids_schema()}), &["ids"])),
        t("get_selection", "IDs der aktuellen Auswahl.", schema(json!({}), &[])),
        t("undo", "Letzte Änderungen rückgängig machen.", schema(json!({"steps": {"type": "integer"}}), &[])),
        t("redo", "Wiederholen.", schema(json!({"steps": {"type": "integer"}}), &[])),
        t("check_problems", "Map auf Probleme prüfen (optional automatisch beheben).", schema(json!({"fix": {"type": "boolean"}}), &[])),
        t("save_map", "Speichern (optional unter neuem Pfad).", schema(json!({"path": {"type": "string"}}), &[])),
        t("open_map", "VMF-Datei öffnen.", schema(json!({"path": {"type": "string"}}), &["path"])),
        t(
            "import_heightmap",
            "Gelände aus einer Heightmap (PNG-Graustufen oder RAW 16 Bit) als Displacement-Kacheln erzeugen. Weiß = hoch.",
            schema(
                json!({"path": {"type": "string"}, "size": {"type": "number", "description": "Kantenlänge (Standard 4096)"}, "height": {"type": "number", "description": "Höhe für Weiß (Standard 512)"}, "tiles": {"type": "integer", "description": "Kacheln pro Seite (Standard 8)"}, "power": {"type": "integer", "description": "2–4 (Standard 3)"}, "material": {"type": "string"}, "origin": vec3_schema("Ecke links unten (Standard: zentriert)")}),
                &["path"],
            ),
        ),
        t(
            "make_prop",
            "Ausgewählte Brushes als Modell (wie Propper): SMD/QC + VertexLitGeneric-Materialien schreiben und mit studiomdl kompilieren. Bei Erfolg werden die Brushes (replace=true) durch prop_static/prop_physics ersetzt. Läuft im Hintergrund – Status per get_prop_status.",
            schema(
                json!({"name": {"type": "string"}, "folder": {"type": "string", "description": "Unterordner unter models/ (Standard amboss)"}, "surfaceprop": {"type": "string"}, "static": {"type": "boolean", "description": "true = prop_static (Standard), false = prop_physics"}, "mass": {"type": "number"}, "replace": {"type": "boolean"}, "compile": {"type": "boolean", "description": "false = nur SMD/QC schreiben"}}),
                &["name"],
            ),
        ),
        t(
            "pack_bsp",
            "Eigene Inhalte der Map (Materialien, Modelle, Sounds, Partikel, Soundscapes, Nav) in eine fertige BSP packen. Ohne bsp: <vmf>.bsp. list_only=true zeigt nur die Kandidaten. Addon-Dateien nur mit addons=true.",
            schema(json!({"bsp": {"type": "string"}, "list_only": {"type": "boolean"}, "addons": {"type": "boolean"}}), &[]),
        ),
        t(
            "decompile_bsp",
            "Kompilierte Map (.bsp) wieder in eine VMF umwandeln (wie BSPSource): Brushes, func_detail, Brush-Entities, Displacements, Entities, statische Props, Overlays, Cubemaps. Auch Half-Life-1-BSPs (GoldSrc, Version 30): Brushes aus dem BSP-Baum, WAD-Texturen werden zu VTF unter materials/goldsrc. Ohne out: <bsp>_dekompiliert.vmf daneben. open=true öffnet sie danach.",
            schema(json!({"bsp": {"type": "string"}, "out": {"type": "string"}, "open": {"type": "boolean"}}), &["bsp"]),
        ),
        t(
            "run_lua",
            "Lua-5.4-Skript im Editor ausführen. Im Skript stehen alle Werkzeuge über amboss.call(name, {…}) bereit, dazu amboss.brush/entity/set/output/move/delete/select/entities/info. print() wird zurückgegeben. Ein Rückgängig-Schritt, Zeitlimit 10 s. Praktisch für Schleifen (Treppen, Raster, Umbenennen).",
            schema(json!({"code": {"type": "string"}}), &["code"]),
        ),
        t(
            "generate_navmesh",
            "Navmesh für NextBots erzeugen (eingebaut, ohne Spiel): begehbare Bereiche ab den Spawnpunkten (Stufen 18, Fallhöhe 200, Spieler 32×72, geduckt 36), gespeichert als <spiel>/maps/<map>.nav. save=false berechnet nur und zeigt die Vorschau.",
            schema(json!({"save": {"type": "boolean"}}), &[]),
        ),
        t("get_prop_status", "Log und Status der letzten Modell-Kompilierung (make_prop).", schema(json!({}), &[])),
        t(
            "import_model",
            "Modell-Datei (glTF/GLB oder OBJ mit MTL) als Prop importieren: Texturen werden zu VTF, Materialien zu VertexLitGeneric, Kollision als konvexe Hülle; studiomdl kompiliert im Hintergrund, danach steht ein prop_static/prop_physics an position (Standard: wohin die 3D-Kamera schaut). Status per get_import_status. Ohne Angaben: glTF in Metern Y-oben, OBJ je nach Größe.",
            schema(
                json!({"path": {"type": "string"}, "name": {"type": "string"}, "folder": {"type": "string"}, "unit": {"type": "string", "enum": ["m", "cm", "in"], "description": "Einheit in der Datei (in = Source-Einheiten)"}, "scale": {"type": "number", "description": "Einheiten pro Dateieinheit (statt unit)"}, "up": {"type": "string", "enum": ["y", "z"]}, "yaw": {"type": "number"}, "static": {"type": "boolean"}, "mass": {"type": "number"}, "surfaceprop": {"type": "string"}, "collision": {"type": "boolean"}, "texture_size": {"type": "integer"}, "position": vec3_schema("Ziel"), "compile": {"type": "boolean", "description": "false = nur SMD/QC/Texturen schreiben"}}),
                &["path"],
            ),
        ),
        t(
            "build_spline",
            "Objekte entlang einer Kurve durch die Punkte erzeugen (ein Rückgängig-Schritt, als Gruppe): kind road (Fahrbahn mit Bordstein, Textur läuft mit), pipe (Rohr), fence (Pfosten + Latten), models (Props im Abstand, ausgerichtet), cable (move_rope/keyframe_rope-Kette). replace_last=true ersetzt das zuletzt Erzeugte.",
            schema(
                json!({"kind": {"type": "string", "enum": ["road", "pipe", "fence", "models", "cable"]}, "points": {"type": "array", "items": vec3_schema("Punkt")}, "closed": {"type": "boolean"}, "smooth": {"type": "boolean", "description": "Kurve (Standard) oder gerade Strecken"}, "material": {"type": "string"}, "width": {"type": "number"}, "thickness": {"type": "number"}, "curb": {"type": "boolean"}, "radius": {"type": "number"}, "sides": {"type": "integer"}, "spacing": {"type": "number"}, "height": {"type": "number"}, "model": {"type": "string"}, "offset": {"type": "number"}, "both_sides": {"type": "boolean"}, "slack": {"type": "number"}, "step": {"type": "number", "description": "Segmentlänge"}, "detail": {"type": "boolean"}, "replace_last": {"type": "boolean"}}),
                &["kind", "points"],
            ),
        ),
        t("get_import_status", "Log und Status des letzten Modell-Imports (import_model).", schema(json!({}), &[])),
        t("new_map", "Neue leere Map.", schema(json!({}), &[])),
        t("compile", "Map speichern, kompilieren (VBSP/VVIS/VRAD) und laut Einstellungen starten.", schema(json!({}), &[])),
        t("get_compile_status", "Status und letzte Log-Zeilen des Kompilierens.", schema(json!({"lines": {"type": "integer"}}), &[])),
        t("list_materials", "Verfügbare Materialien (aus den Spielinhalten) durchsuchen.", schema(json!({"filter": {"type": "string"}, "limit": {"type": "integer"}}), &[])),
        t("list_models", "Verfügbare Modelle durchsuchen.", schema(json!({"filter": {"type": "string"}, "limit": {"type": "integer"}}), &[])),
        t("list_entity_classes", "Entity-Klassen aus der FGD (kind: point oder solid).", schema(json!({"filter": {"type": "string"}, "kind": {"type": "string"}}), &[])),
        t("get_entity_class", "Schlüssel, Inputs und Outputs einer Entity-Klasse.", schema(json!({"classname": {"type": "string"}}), &["classname"])),
        t(
            "list_prefabs",
            "Prefab-Bibliothek: fertige Bauteile (Türen mit Knöpfen, Fenster, Leiter, Lampen, Teleporter, Räume, Wasserbecken …) und eigene Prefabs.",
            schema(json!({"filter": {"type": "string"}, "category": {"type": "string"}}), &[]),
        ),
        t(
            "insert_prefab",
            "Prefab einfügen. position = Punkt auf Boden/Decke/Wand; surface_normal (optional) richtet aus: [0,0,1] Boden (Unterkante auf position), [0,0,-1] Decke, waagrecht = Wand. Doppelte Entity-Namen werden automatisch umbenannt.",
            schema(json!({"prefab": {"type": "string", "description": "ID oder Name aus list_prefabs"}, "position": vec3_schema("Zielpunkt"), "surface_normal": vec3_schema("Flächennormale")}), &["prefab", "position"]),
        ),
        t(
            "merge_map",
            "Eine andere Map-Datei (.vmf/.map) in die offene Map übernehmen. IDs werden neu vergeben, doppelte Entity-Namen umbenannt (Verweise ziehen mit), Gruppen/Visgroups übernommen. placement: keep (Koordinaten behalten), beside (neben die Map), offset (um offset verschieben). Liefert Unterschiede der Map-Eigenschaften und Warnungen (Überlappung, zwei Sonnen …). Ein Undo-Schritt.",
            schema(
                json!({"path": {"type": "string"}, "placement": {"type": "string", "enum": ["keep", "beside", "offset"]}, "offset": vec3_schema("Versatz bei placement=offset"), "visgroup": {"type": "boolean", "description": "in Visgroup 'Import: <name>' legen (Standard ja)"}, "keep_groups": {"type": "boolean"}, "take_world_keys": {"type": "array", "items": {"type": "string"}, "description": "worldspawn-Schlüssel, deren Wert aus der anderen Map übernommen wird"}}),
                &["path"],
            ),
        ),
        t(
            "compare_map",
            "Offene Map mit einer anderen Datei (älterer Stand, Version einer anderen Person) vergleichen: neue, gelöschte und geänderte Objekte sowie Map-Eigenschaften. Markiert die Unterschiede farbig in den Ansichten.",
            schema(json!({"path": {"type": "string"}, "limit": {"type": "integer"}}), &["path"]),
        ),
        t(
            "merge_versions",
            "3-Wege-Zusammenführen: die offene Map ist 'unsere' Version, base der gemeinsame Ausgangsstand, theirs die Version der anderen Person. Änderungen beider Seiten werden kombiniert; Konflikte behalten unsere Version, die andere liegt ausgeblendet in der Visgroup 'Konflikt: andere Version'. Ein Undo-Schritt.",
            schema(json!({"base": {"type": "string"}, "theirs": {"type": "string"}}), &["base", "theirs"]),
        ),
        t(
            "optimize_map",
            "Automatische Optimierungen: KI-Nodes (info_node/info_node_air, Tür-Controller, info_hint), Leckprüfung mit Pfad, func_detail, nodraw, Areaportals, Lightmaps, Cubemaps, Aufräumen, Props. apply=false prüft nur (Karte unverändert, Netz/Markierungen werden angezeigt), apply=true übernimmt (ein Undo-Schritt). options überschreibt einzelne Einstellungen nur für diesen Lauf.",
            schema(json!({"apply": {"type": "boolean"}, "options": {"type": "object", "description": "z. B. {\"ai_nodes\": true, \"node_spacing\": 192, \"areaportals\": true}"}}), &[]),
        ),
        t(
            "optimize_settings",
            "Gespeicherte Optimierungs-Einstellungen lesen und optional ändern (set). compile_auto=true wendet sie bei jedem Kompilieren auf einer Kopie an.",
            schema(json!({"set": {"type": "object"}}), &[]),
        ),
        t("list_commands", "Editor-Befehle (für run_command).", schema(json!({}), &[])),
        t("run_command", "Einen Editor-Befehl ausführen (IDs aus list_commands).", schema(json!({"command": {"type": "string"}}), &["command"])),
    ]
}

// ------------------------------------------------------------------ Werkzeuge

fn arg_vec(a: &Value, key: &str) -> Result<DVec3, String> {
    let v = a.get(key).and_then(|v| v.as_array()).ok_or(format!("'{key}' fehlt (Array mit 3 Zahlen)"))?;
    let n: Vec<f64> = v.iter().filter_map(|x| x.as_f64()).collect();
    if n.len() != 3 {
        return Err(format!("'{key}' braucht genau 3 Zahlen"));
    }
    Ok(DVec3::new(n[0], n[1], n[2]))
}

fn opt_vec(a: &Value, key: &str) -> Option<DVec3> {
    a.get(key).and_then(|_| arg_vec(a, key).ok())
}

fn arg_str<'a>(a: &'a Value, key: &str) -> Result<&'a str, String> {
    a.get(key).and_then(|v| v.as_str()).ok_or(format!("'{key}' fehlt"))
}

fn arg_ids(ed: &Editor, a: &Value) -> Result<Vec<Obj>, String> {
    let ids = a.get("ids").and_then(|v| v.as_array()).ok_or("'ids' fehlt")?;
    let mut out = Vec::new();
    for id in ids {
        let id = id.as_u64().ok_or("IDs müssen Zahlen sein")? as u32;
        if ed.map.entity(id).is_some() {
            out.push(Obj::Entity(id));
        } else if ed.map.solid(id).is_some() {
            out.push(Obj::Solid(id));
        } else {
            return Err(format!("Kein Objekt mit ID {id}"));
        }
    }
    if out.is_empty() {
        return Err("Keine IDs angegeben".into());
    }
    Ok(out)
}

fn bounds_json(b: &Aabb) -> Value {
    if b.is_empty() {
        return Value::Null;
    }
    json!({"min": [b.min.x, b.min.y, b.min.z], "max": [b.max.x, b.max.y, b.max.z]})
}

pub(crate) fn call_tool(name: &str, a: &Value, host: &mut dyn Host) -> Result<Value, String> {
    match name {
        "get_map_info" => {
            let ed = host.ed();
            Ok(json!({
                "file": ed.path.as_ref().map(|p| p.display().to_string()),
                "unsaved_changes": ed.dirty,
                "brushes": ed.map.all_solids().count(),
                "entities": ed.map.entities.len(),
                "bounds": bounds_json(&ed.map_bounds()),
                "selection": ed.sel.len(),
                "grid": ed.grid,
                "current_material": ed.material,
                "cordon_active": ed.map.cordon.is_some_and(|c| c.active),
                "worldspawn": ed.map.world.props.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<JsonMap<_, _>>(),
            }))
        }
        "list_entities" => {
            let ed = host.ed();
            let class = a.get("classname").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let limit = a.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
            let list: Vec<Value> = ed
                .map
                .entities
                .iter()
                .filter(|e| class.is_empty() || e.classname().to_ascii_lowercase().contains(&class))
                .filter(|e| name.is_empty() || e.targetname().is_some_and(|n| n.to_ascii_lowercase().contains(&name)))
                .take(limit)
                .map(|e| json!({"id": e.id, "classname": e.classname(), "targetname": e.targetname(), "origin": e.get("origin"), "angles": e.get("angles"), "brushes": e.solids.len()}))
                .collect();
            Ok(json!({"count": list.len(), "entities": list}))
        }
        "get_entity" => {
            let id = a.get("id").and_then(|v| v.as_u64()).ok_or("'id' fehlt")? as u32;
            let ed = host.ed();
            let e = if id == ed.map.world.id { Some(&ed.map.world) } else { ed.map.entity(id) }.ok_or(format!("Keine Entity mit ID {id}"))?;
            Ok(json!({
                "id": e.id,
                "classname": e.classname(),
                "keys": e.props.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<JsonMap<_, _>>(),
                "outputs": e.outputs.iter().map(|(o, v)| json!({"output": o, "value": v})).collect::<Vec<_>>(),
                "brush_ids": e.solids.iter().map(|s| s.id).collect::<Vec<_>>(),
                "bounds": bounds_json(&ed.entity_bounds(e)),
            }))
        }
        "create_entity" => {
            let class = arg_str(a, "classname")?.to_string();
            let origin = arg_vec(a, "origin")?;
            let ed = host.ed();
            let prev = std::mem::replace(&mut ed.entity_class, class.clone());
            let id = ed.create_point_entity(origin);
            ed.entity_class = prev;
            if let Some(e) = ed.map.entity_mut(id) {
                if let Some(ang) = opt_vec(a, "angles") {
                    e.set("angles", fmt_vec(ang));
                }
                if let Some(keys) = a.get("keys").and_then(|k| k.as_object()) {
                    for (k, v) in keys {
                        e.set(k, v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()));
                    }
                }
            }
            ed.status = format!("MCP: {class} erstellt");
            Ok(json!({"id": id}))
        }
        "set_entity_keys" => {
            let id = a.get("id").and_then(|v| v.as_u64()).ok_or("'id' fehlt")? as u32;
            let keys = a.get("keys").and_then(|k| k.as_object()).ok_or("'keys' fehlt")?.clone();
            let ed = host.ed();
            if id != ed.map.world.id && ed.map.entity(id).is_none() {
                return Err(format!("Keine Entity mit ID {id}"));
            }
            ed.checkpoint();
            let e = if id == ed.map.world.id { &mut ed.map.world } else { ed.map.entity_mut(id).unwrap() };
            for (k, v) in &keys {
                let v = v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string());
                if v.is_empty() {
                    e.remove(k);
                } else {
                    e.set(k, v);
                }
            }
            ed.status = format!("MCP: Entity {id} geändert");
            ed.touch();
            Ok(json!({"ok": true}))
        }
        "add_output" => {
            let id = a.get("id").and_then(|v| v.as_u64()).ok_or("'id' fehlt")? as u32;
            let value = format!(
                "{},{},{},{},{}",
                arg_str(a, "target")?,
                arg_str(a, "input")?,
                a.get("parameter").and_then(|v| v.as_str()).unwrap_or(""),
                a.get("delay").and_then(|v| v.as_f64()).unwrap_or(0.0),
                a.get("times").and_then(|v| v.as_i64()).unwrap_or(-1)
            );
            let output = arg_str(a, "output")?.to_string();
            let ed = host.ed();
            if ed.map.entity(id).is_none() {
                return Err(format!("Keine Entity mit ID {id}"));
            }
            ed.checkpoint();
            ed.map.entity_mut(id).unwrap().outputs.push((output, value));
            ed.status = "MCP: Output hinzugefügt".into();
            ed.touch();
            Ok(json!({"ok": true}))
        }
        "list_brushes" => {
            let ed = host.ed();
            let filter = match (opt_vec(a, "min"), opt_vec(a, "max")) {
                (Some(mn), Some(mx)) => Some(Aabb::new(mn, mx)),
                _ => None,
            };
            let limit = a.get("limit").and_then(|v| v.as_u64()).unwrap_or(300) as usize;
            let list: Vec<Value> = ed
                .map
                .all_solids()
                .filter(|(_, s)| filter.is_none_or(|f| f.intersects(&s.bounds)))
                .take(limit)
                .map(|(owner, s)| {
                    let mut mats: Vec<&str> = s.sides.iter().map(|x| x.material.as_str()).collect();
                    mats.sort();
                    mats.dedup();
                    json!({"id": s.id, "entity": owner, "bounds": bounds_json(&s.bounds), "materials": mats, "displacement": s.has_disp()})
                })
                .collect();
            Ok(json!({"count": list.len(), "brushes": list}))
        }
        "create_brush" => {
            let (mn, mx) = (arg_vec(a, "min")?, arg_vec(a, "max")?);
            let ed = host.ed();
            let before = ed.map.next_id;
            let prev_mat = ed.material.clone();
            let prev_shape = ed.shape;
            if let Some(m) = a.get("material").and_then(|v| v.as_str()) {
                ed.material = m.to_string();
            }
            if let Some(s) = a.get("shape").and_then(|v| v.as_str()) {
                ed.shape.shape = crate::primitives::Shape::ALL.iter().copied().find(|x| format!("{x:?}").eq_ignore_ascii_case(s)).ok_or(format!("Unbekannte Form '{s}'"))?;
            } else {
                ed.shape.shape = crate::primitives::Shape::Block;
            }
            if let Some(n) = a.get("sides").and_then(|v| v.as_u64()) {
                ed.shape.sides = n as u32;
            }
            if let Some(w) = a.get("wall").and_then(|v| v.as_f64()) {
                ed.shape.wall = w;
            }
            if let Some(w) = a.get("arc").and_then(|v| v.as_f64()) {
                ed.shape.arc = w;
            }
            if let Some(w) = a.get("start_angle").and_then(|v| v.as_f64()) {
                ed.shape.start_angle = w;
            }
            ed.create_block(Aabb::new(mn, mx));
            ed.material = prev_mat;
            ed.shape = prev_shape;
            let ids: Vec<u32> = ed.selected_solid_ids().into_iter().filter(|id| *id >= before).collect();
            if ids.is_empty() {
                return Err("Brush konnte nicht erzeugt werden (Box zu klein?)".into());
            }
            Ok(json!({"ids": ids}))
        }
        "create_room" => {
            let (mn, mx) = (arg_vec(a, "min")?, arg_vec(a, "max")?);
            let t = a.get("wall_thickness").and_then(|v| v.as_f64()).unwrap_or(16.0);
            let ed = host.ed();
            let prev = ed.material.clone();
            if let Some(m) = a.get("material").and_then(|v| v.as_str()) {
                ed.material = m.to_string();
            }
            let prev_shape = ed.shape.shape;
            ed.shape.shape = crate::primitives::Shape::Block;
            ed.create_block(Aabb::new(mn, mx));
            ed.shape.shape = prev_shape;
            ed.hollow(t);
            ed.material = prev;
            ed.status = "MCP: Raum erstellt".into();
            Ok(json!({"ids": ed.selected_solid_ids(), "inner_min": [mn.x + t, mn.y + t, mn.z + t], "inner_max": [mx.x - t, mx.y - t, mx.z - t]}))
        }
        "move_objects" => {
            let d = arg_vec(a, "offset")?;
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            ed.checkpoint();
            ed.translate_selection(d);
            ed.status = "MCP: verschoben".into();
            Ok(json!({"bounds": bounds_json(&ed.selection_bounds())}))
        }
        "rotate_objects" => {
            let axis = match arg_str(a, "axis")?.to_ascii_lowercase().as_str() {
                "x" => 0,
                "y" => 1,
                _ => 2,
            };
            let deg = a.get("degrees").and_then(|v| v.as_f64()).ok_or("'degrees' fehlt")?;
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            let c = opt_vec(a, "center").unwrap_or_else(|| ed.selection_center());
            ed.transform_selection(&Xform::about(c, Xform::rotation(axis, deg)), false);
            ed.status = "MCP: gedreht".into();
            Ok(json!({"bounds": bounds_json(&ed.selection_bounds())}))
        }
        "resize_objects" => {
            let to = Aabb::new(arg_vec(a, "min")?, arg_vec(a, "max")?);
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            let from = ed.selection_bounds();
            ed.transform_selection(&Editor::scale_xform(from, to), true);
            Ok(json!({"bounds": bounds_json(&ed.selection_bounds())}))
        }
        "delete_objects" => {
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            let n = ed.sel.len();
            ed.delete_selection();
            Ok(json!({"deleted": n}))
        }
        "set_material" => {
            let mat = arg_str(a, "material")?.to_string();
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            ed.faces.clear();
            let prev = std::mem::replace(&mut ed.material, mat);
            ed.apply_material();
            ed.material = prev;
            Ok(json!({"ok": true}))
        }
        "replace_material" => {
            let (f, r) = (arg_str(a, "find")?.to_string(), arg_str(a, "replace")?.to_string());
            let partial = a.get("partial").and_then(|v| v.as_bool()).unwrap_or(false);
            let n = host.ed().replace_materials(&f, &r, false, partial);
            Ok(json!({"faces_replaced": n}))
        }
        "tie_to_entity" => {
            let class = arg_str(a, "classname")?.to_string();
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            ed.tie_to_entity(&class);
            Ok(json!({"entity_id": ed.selected_entity_ids().first()}))
        }
        "carve" => {
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            ed.carve();
            Ok(json!({"status": ed.status}))
        }
        "clip_brushes" => {
            let p = arg_vec(a, "point")?;
            let n = arg_vec(a, "normal")?.normalize_or_zero();
            if n == DVec3::ZERO {
                return Err("Normale darf nicht 0 sein".into());
            }
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            let mode = match a.get("keep").and_then(|v| v.as_str()).unwrap_or("both") {
                "front" => ClipMode::Front,
                "back" => ClipMode::Back,
                _ => ClipMode::Both,
            };
            let ids = ed.clip_with_plane(Plane { normal: n, dist: n.dot(p) }, mode);
            Ok(json!({"ids": ids}))
        }
        "select" => {
            let ed = host.ed();
            ed.sel = arg_ids(ed, a)?.into_iter().collect();
            ed.touch();
            Ok(json!({"selected": ed.sel.len()}))
        }
        "get_selection" => {
            let ed = host.ed();
            let ids: Vec<u32> = ed.sel.iter().map(|o| match o {
                Obj::Solid(id) | Obj::Entity(id) => *id,
            }).collect();
            Ok(json!({"ids": ids, "bounds": bounds_json(&ed.selection_bounds())}))
        }
        "undo" | "redo" => {
            let steps = a.get("steps").and_then(|v| v.as_u64()).unwrap_or(1).min(200);
            let ed = host.ed();
            for _ in 0..steps {
                if name == "undo" {
                    ed.undo();
                } else {
                    ed.redo();
                }
            }
            let (u, r) = ed.history();
            Ok(json!({"undo_available": u.len(), "redo_available": r.len()}))
        }
        "check_problems" => {
            let known = host.known_materials();
            let fix = a.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);
            let ed = host.ed();
            let mut probs = crate::problems::check(ed, known.as_ref());
            if fix && probs.iter().any(|p| p.fix.is_some()) {
                ed.checkpoint();
                for p in &probs {
                    if let Some(f) = &p.fix {
                        crate::problems::apply(ed, f);
                    }
                }
                probs = crate::problems::check(ed, known.as_ref());
            }
            Ok(json!({"problems": probs.iter().map(|p| json!({"severity": format!("{:?}", p.severity), "text": p.text, "fixable": p.fix.is_some()})).collect::<Vec<_>>()}))
        }
        "save_map" => host.save(a.get("path").and_then(|v| v.as_str()).map(PathBuf::from)).map(|s| json!({"saved": s})),
        "open_map" => host.open(PathBuf::from(arg_str(a, "path")?)).map(|s| json!({"opened": s})),
        "import_heightmap" => {
            let hm = crate::heightmap::load(std::path::Path::new(arg_str(a, "path")?))?;
            let mut o = crate::heightmap::TerrainOpts::default();
            if let Some(v) = a.get("size").and_then(|v| v.as_f64()) {
                o.size = v.clamp(64.0, 32768.0);
            }
            o.origin = DVec3::new(-o.size / 2.0, -o.size / 2.0, 0.0);
            if let Some(v) = a.get("height").and_then(|v| v.as_f64()) {
                o.height = v;
            }
            if let Some(v) = a.get("tiles").and_then(|v| v.as_u64()) {
                o.tiles = v.clamp(1, 32) as usize;
            }
            if let Some(v) = a.get("power").and_then(|v| v.as_u64()) {
                o.power = v.clamp(2, 4) as u32;
            }
            if let Some(v) = a.get("material").and_then(|v| v.as_str()) {
                o.material = v.to_string();
            }
            if a.get("origin").is_some() {
                o.origin = arg_vec(a, "origin")?;
            }
            let n = crate::heightmap::build(host.ed(), &hm, &o);
            Ok(json!({"tiles": n, "image": [hm.w, hm.h], "size": o.size, "height": o.height}))
        }
        "make_prop" => {
            let d = crate::propper::PropOpts::default();
            let o = crate::propper::PropOpts {
                name: arg_str(a, "name")?.trim().replace([' ', '/', '\\'], "_"),
                folder: a.get("folder").and_then(|v| v.as_str()).map(|s| s.trim_matches('/').to_string()).unwrap_or(d.folder),
                surfaceprop: a.get("surfaceprop").and_then(|v| v.as_str()).map(String::from).unwrap_or(d.surfaceprop),
                is_static: a.get("static").and_then(|v| v.as_bool()).unwrap_or(true),
                mass: a.get("mass").and_then(|v| v.as_f64()).unwrap_or(d.mass),
                replace: a.get("replace").and_then(|v| v.as_bool()).unwrap_or(true),
            };
            host.make_prop(o, a.get("compile").and_then(|v| v.as_bool()).unwrap_or(true))
        }
        "pack_bsp" => {
            let fs = host.game_fs().ok_or("Spielinhalte sind nicht geladen (Spielverzeichnis einstellen)")?;
            let ed = host.ed();
            let vmf = ed.path.clone();
            let mut list = crate::assets::pack_candidates(ed, &fs, vmf.as_deref());
            if a.get("addons").and_then(|v| v.as_bool()).unwrap_or(false) {
                for e in list.iter_mut() {
                    e.include = true;
                }
            }
            let files: Vec<Value> = list.iter().map(|e| json!({"path": e.path, "size": e.size, "include": e.include, "source": match &e.source { crate::assets::PackSource::Loose(_) => "eigene Datei".to_string(), crate::assets::PackSource::Addon(n) => format!("Addon: {n}"), crate::assets::PackSource::Extra(_) => "hinzugefügt".to_string() }})).collect();
            if a.get("list_only").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Ok(json!({"files": files}));
            }
            let bsp = match a.get("bsp").and_then(|v| v.as_str()) {
                Some(b) => PathBuf::from(b),
                None => vmf.map(|v| v.with_extension("bsp")).ok_or("bsp angeben oder Map speichern")?,
            };
            let data: Vec<(String, Vec<u8>)> = list.iter().filter(|e| e.include).filter_map(|e| e.read(Some(&fs)).map(|b| (e.path.clone(), b))).collect();
            let r = crate::bsppack::pack(&bsp, &data)?;
            Ok(json!({"bsp": bsp.display().to_string(), "packed": data.len(), "added": r.added, "replaced": r.replaced, "kept": r.kept, "bytes": r.bytes}))
        }
        "decompile_bsp" => {
            let bsp = PathBuf::from(arg_str(a, "bsp")?);
            let out = match a.get("out").and_then(|v| v.as_str()) {
                Some(o) => PathBuf::from(o),
                None => bsp.with_file_name(format!("{}_dekompiliert.vmf", bsp.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())),
            };
            let r = crate::decompile::decompile_file(&bsp, &out, &host.game_dir(), &[])?;
            if a.get("open").and_then(|v| v.as_bool()).unwrap_or(false) {
                host.open(out.clone())?;
            }
            Ok(json!({"vmf": out.display().to_string(), "report": r.lines()}))
        }
        "run_lua" => {
            let code = arg_str(a, "code")?.to_string();
            let r = crate::lua::run(&code, host, std::time::Duration::from_secs(10));
            match r.error {
                Some(e) => Err(format!("{e}
Ausgabe:
{}", r.output.join("
"))),
                None => Ok(json!({"output": r.output, "calls": r.calls, "ms": r.millis as u64})),
            }
        }
        "generate_navmesh" => {
            let gd = host.game_dir();
            let ed = host.ed();
            let r = crate::navmesh::generate(&ed.map, ed.map_limit)?;
            let path = crate::navmesh::nav_path(&gd, ed.path.as_deref(), &ed.map_name());
            let saved = a.get("save").and_then(|v| v.as_bool()).unwrap_or(true);
            if saved {
                std::fs::write(&path, crate::navmesh::write_nav(&r.areas, 0)).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            let summary = r.summary(&path);
            let crouch = r.areas.iter().filter(|x| x.crouch).count();
            let n = r.areas.len();
            ed.nav_preview = Some(std::sync::Arc::new(r.areas));
            ed.nav_show = true;
            ed.status = summary;
            ed.touch();
            Ok(json!({"areas": n, "crouch_areas": crouch, "grid_points": r.nodes, "ms": r.millis as u64, "file": if saved { path.display().to_string() } else { String::new() }}))
        }
        "import_model" => host.import_model(Some(PathBuf::from(arg_str(a, "path")?)), a),
        "get_import_status" => host.import_model(None, a),
        "build_spline" => {
            use crate::spline::Kind;
            let ed = host.ed();
            let pts = a.get("points").and_then(|v| v.as_array()).ok_or("'points' fehlt")?;
            let mut points = Vec::new();
            for p in pts {
                let v = p.as_array().filter(|v| v.len() == 3).ok_or("Punkte als [x, y, z]")?;
                points.push(DVec3::new(v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0)));
            }
            let mut o = crate::spline::SplineOpts {
                kind: match arg_str(a, "kind")? {
                    "road" => Kind::Road,
                    "pipe" => Kind::Pipe,
                    "fence" => Kind::Fence,
                    "models" => Kind::Models,
                    "cable" => Kind::Cable,
                    k => return Err(format!("Unbekannte Art: {k}")),
                },
                ..Default::default()
            };
            let f = |k: &str| a.get(k).and_then(|v| v.as_f64());
            let b = |k: &str| a.get(k).and_then(|v| v.as_bool());
            if let Some(v) = b("closed") { o.closed = v; }
            if let Some(v) = b("smooth") { o.smooth = v; }
            if let Some(v) = b("curb") { o.curb = v; }
            if let Some(v) = b("both_sides") { o.both_sides = v; }
            if let Some(v) = b("detail") { o.detail = v; }
            if let Some(v) = a.get("material").and_then(|v| v.as_str()) { o.material = v.to_string(); }
            if let Some(v) = a.get("model").and_then(|v| v.as_str()) { o.model = v.to_string(); }
            if let Some(v) = a.get("sides").and_then(|v| v.as_u64()) { o.sides = v as usize; }
            for (k, slot) in [("width", &mut o.width), ("thickness", &mut o.thickness), ("radius", &mut o.radius), ("spacing", &mut o.spacing), ("height", &mut o.height), ("offset", &mut o.offset), ("slack", &mut o.slack), ("step", &mut o.step)] {
                if let Some(v) = f(k) {
                    *slot = v;
                }
            }
            ed.spline.points = points;
            ed.spline.opts = o;
            let n = crate::spline::build(ed, b("replace_last").unwrap_or(false))?;
            let ids: Vec<u32> = ed.spline.last.iter().map(|o| match o { Obj::Solid(id) | Obj::Entity(id) => *id }).collect();
            Ok(json!({"created": n, "ids": ids, "status": ed.status}))
        }
        "get_prop_status" => host.make_prop(crate::propper::PropOpts { name: String::new(), ..Default::default() }, false),
        "new_map" => {
            host.ed().new_map();
            Ok(json!({"ok": true}))
        }
        "compile" => host.compile().map(|s| json!({"status": s})),
        "get_compile_status" => {
            let mut v = host.compile_status();
            let lines = a.get("lines").and_then(|v| v.as_u64()).unwrap_or(40) as usize;
            if let Some(log) = v.get_mut("log").and_then(|l| l.as_array_mut()) {
                let skip = log.len().saturating_sub(lines);
                *log = log.split_off(skip);
            }
            Ok(v)
        }
        "list_materials" | "list_models" => {
            let f = a.get("filter").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let limit = a.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            let all = if name == "list_materials" { host.materials() } else { host.models() };
            let words: Vec<&str> = f.split_whitespace().collect();
            let hits: Vec<String> = all.into_iter().filter(|m| words.iter().all(|w| m.contains(w))).take(limit).collect();
            if hits.is_empty() && host.materials().is_empty() {
                return Err("Keine Spielinhalte geladen (Spielordner in den Einstellungen setzen).".into());
            }
            Ok(json!({"count": hits.len(), "items": hits}))
        }
        "list_entity_classes" => {
            let f = a.get("filter").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let ed = host.ed();
            let kinds: Vec<ClassKind> = match a.get("kind").and_then(|v| v.as_str()) {
                Some("point") => vec![ClassKind::Point],
                Some("solid") => vec![ClassKind::Solid],
                _ => vec![ClassKind::Point, ClassKind::Solid],
            };
            let mut list: Vec<Value> = Vec::new();
            for k in kinds {
                for n in ed.fgd.names(k) {
                    if f.is_empty() || n.contains(&f) {
                        let d = ed.fgd.class(&n).map(|c| c.description.clone()).unwrap_or_default();
                        list.push(json!({"classname": n, "kind": if k == ClassKind::Point { "point" } else { "solid" }, "description": d.chars().take(160).collect::<String>()}));
                    }
                }
            }
            Ok(json!({"count": list.len(), "classes": list}))
        }
        "get_entity_class" => {
            let c = arg_str(a, "classname")?;
            let ed = host.ed();
            let def = ed.fgd.class(c).ok_or(format!("Klasse '{c}' nicht in der FGD"))?;
            Ok(json!({
                "classname": def.name,
                "description": def.description,
                "keys": ed.fgd.all_props(c).iter().map(|p| json!({"key": p.name, "type": p.kind, "default": p.default, "description": p.description, "choices": p.choices.iter().map(|ch| json!({"value": ch.value, "name": ch.name})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "inputs": ed.fgd.all_io(c, false).iter().map(|i| json!({"name": i.name, "description": i.description})).collect::<Vec<_>>(),
                "outputs": ed.fgd.all_io(c, true).iter().map(|i| json!({"name": i.name, "description": i.description})).collect::<Vec<_>>(),
            }))
        }
        "list_prefabs" => {
            let f = a.get("filter").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let c = a.get("category").and_then(|v| v.as_str()).unwrap_or("").to_ascii_lowercase();
            let list: Vec<Value> = crate::prefabs::scan(&host.prefab_dir())
                .into_iter()
                .filter(|e| f.is_empty() || e.name.to_ascii_lowercase().contains(&f) || e.description.to_ascii_lowercase().contains(&f))
                .filter(|e| c.is_empty() || e.category.to_ascii_lowercase().contains(&c))
                .map(|e| {
                    let size = crate::prefabs::load(&e).ok().map(|m| {
                        let ed = Editor::new(crate::fgd::Fgd::builtin());
                        let mut b = m.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
                        for en in &m.entities {
                            b = b.union(&ed.entity_bounds(en));
                        }
                        b.size()
                    });
                    json!({"id": e.id(), "name": e.name, "category": e.category, "description": e.description, "size": size.map(|s| [s.x, s.y, s.z])})
                })
                .collect();
            Ok(json!({"count": list.len(), "prefabs": list}))
        }
        "insert_prefab" => {
            let key = arg_str(a, "prefab")?.to_string();
            let pos = arg_vec(a, "position")?;
            let normal = opt_vec(a, "surface_normal").map(|n| n.normalize_or_zero()).filter(|n| *n != DVec3::ZERO);
            let entries = crate::prefabs::scan(&host.prefab_dir());
            let e = entries
                .iter()
                .find(|e| e.id() == key)
                .or_else(|| entries.iter().find(|e| e.name.eq_ignore_ascii_case(&key)))
                .or_else(|| entries.iter().find(|e| e.name.to_ascii_lowercase().contains(&key.to_ascii_lowercase())))
                .ok_or(format!("Prefab '{key}' nicht gefunden (list_prefabs)"))?
                .clone();
            let m = crate::prefabs::load(&e)?;
            let ids = host.ed().insert_prefab_map(m, pos, normal)?;
            let ed = host.ed();
            let names: Vec<String> = ids.iter().filter_map(|id| ed.map.entity(*id)).filter_map(|en| en.targetname().map(|s| s.to_string())).collect();
            Ok(json!({"prefab": e.name, "ids": ids, "bounds": bounds_json(&ed.selection_bounds()), "entity_names": names}))
        }
        "merge_map" => {
            let path = PathBuf::from(arg_str(a, "path")?);
            let ed = host.ed();
            let map = crate::merge_ui::load_any(&path, ed.tex_scale)?;
            let placement = match a.get("placement").and_then(|v| v.as_str()).unwrap_or("keep") {
                "beside" => crate::merge::Placement::Beside,
                "offset" => crate::merge::Placement::Offset(opt_vec(a, "offset").unwrap_or(DVec3::ZERO)),
                "keep" => crate::merge::Placement::Keep,
                other => return Err(format!("Unbekanntes placement '{other}' (keep, beside, offset)")),
            };
            let o = crate::merge::MergeOptions {
                placement,
                visgroup: a.get("visgroup").and_then(|v| v.as_bool()).unwrap_or(true),
                keep_groups: a.get("keep_groups").and_then(|v| v.as_bool()).unwrap_or(true),
                take_world_keys: a.get("take_world_keys").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
            };
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let r = ed.merge_map(map, &name, path.parent(), &o)?;
            Ok(json!({
                "summary": r.summary(),
                "solids": r.solids,
                "entities": r.entities,
                "offset": [r.offset.x, r.offset.y, r.offset.z],
                "renamed": r.renamed.iter().map(|(o, n)| json!({"from": o, "to": n})).collect::<Vec<_>>(),
                "world_differences": r.world_diffs.iter().map(|(k, a, b)| json!({"key": k, "open_map": a, "other_map": b})).collect::<Vec<_>>(),
                "warnings": r.warnings,
                "bounds": bounds_json(&ed.selection_bounds()),
            }))
        }
        "compare_map" => {
            let path = PathBuf::from(arg_str(a, "path")?);
            let limit = a.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
            let ed = host.ed();
            let old = crate::merge_ui::load_any(&path, ed.tex_scale)?;
            let d = crate::diff::diff(&old, &ed.map, ed);
            ed.diff_boxes = d.objects.iter().filter(|o| !o.bounds.is_empty()).map(|o| (o.bounds, o.change.color())).collect();
            ed.touch();
            Ok(json!({
                "summary": d.summary(),
                "world": d.world.iter().map(|(k, x, y)| json!({"key": k, "other": x, "open_map": y})).collect::<Vec<_>>(),
                "objects": d.objects.iter().take(limit).map(|o| json!({"change": o.change.label(), "kind": if o.kind == crate::diff::Kind::Solid { "solid" } else { "entity" }, "id": o.id, "label": o.label, "bounds": bounds_json(&o.bounds)})).collect::<Vec<_>>(),
            }))
        }
        "merge_versions" => {
            let ed = host.ed();
            let base = crate::merge_ui::load_any(&PathBuf::from(arg_str(a, "base")?), ed.tex_scale)?;
            let theirs = crate::merge_ui::load_any(&PathBuf::from(arg_str(a, "theirs")?), ed.tex_scale)?;
            let mut m = ed.map.clone();
            let r = crate::diff::merge3(&base, &mut m, &theirs);
            ed.checkpoint();
            ed.map = m;
            ed.sel.clear();
            ed.faces.clear();
            ed.vsel.clear();
            ed.status = format!("3-Wege-Zusammenführen: {}", r.summary());
            ed.touch();
            Ok(json!({"summary": r.summary(), "conflicts": r.conflicts.iter().map(|c| json!({"id": c.id, "text": c.text})).collect::<Vec<_>>()}))
        }
        "optimize_map" => {
            let apply = a.get("apply").and_then(|v| v.as_bool()).unwrap_or(false);
            let o = match a.get("options") {
                Some(p) if !p.is_null() => crate::optimize::merge_settings(host.opt_settings(), p)?,
                _ => host.opt_settings().clone(),
            };
            let fs = host.game_fs();
            let ed = host.ed();
            let mut copy = crate::optimize::build_copy(ed, ed.map.clone());
            let r = crate::optimize::run(&mut copy, &o, fs.as_deref(), None);
            if apply && r.changes > 0 {
                let ed = host.ed();
                ed.checkpoint();
                ed.map = copy.map;
                ed.sel.clear();
                ed.faces.clear();
                ed.vsel.clear();
                ed.status = format!("Optimierungen übernommen (MCP): {} Änderung(en)", r.changes);
            }
            host.show_report(&r);
            let ents = &host.ed().map.entities;
            let count = |c: &str| ents.iter().filter(|e| e.classname() == c).count();
            let (nodes, air, hints) = (count("info_node"), count("info_node_air"), count("info_hint"));
            Ok(json!({
                "applied": apply && r.changes > 0,
                "changes": r.changes,
                "report": r.lines,
                "leak": r.leak.as_ref().map(|l| l.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>()),
                "node_links": r.links.len(),
                "map_counts": {"info_node": nodes, "info_node_air": air, "info_hint": hints},
                "markers": r.markers.iter().take(100).map(|m| json!({"position": [m.pos.x, m.pos.y, m.pos.z], "text": m.text})).collect::<Vec<_>>(),
            }))
        }
        "optimize_settings" => {
            if let Some(p) = a.get("set").filter(|v| !v.is_null()) {
                let o = crate::optimize::merge_settings(host.opt_settings(), p)?;
                *host.opt_settings() = o;
            }
            Ok(serde_json::to_value(host.opt_settings().clone()).map_err(|e| e.to_string())?)
        }
        "list_commands" => Ok(json!({"commands": host.commands().into_iter().map(|(id, l)| json!({"id": id, "label": l})).collect::<Vec<_>>()})),
        "run_command" => host.run_command(arg_str(a, "command")?).map(|s| json!({"status": s})),
        _ => Err(format!("Unbekanntes Werkzeug: {name}")),
    }
}

// ------------------------------------------------------------------ HTTP-Server (im Editor)

pub struct Request {
    pub body: Value,
    pub reply: Sender<Option<Value>>,
}

pub struct Server {
    pub port: u16,
    pub rx: Receiver<Request>,
}

/// Startet den HTTP-Endpunkt. `wake` wird aufgerufen, wenn eine Anfrage wartet (z. B. request_repaint).
pub fn start_server(port: u16, wake: impl Fn() + Send + Sync + 'static) -> Result<Server, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("MCP-Port {port} nicht verfügbar: {e}"))?;
    let (tx, rx) = channel::<Request>();
    let wake = std::sync::Arc::new(wake);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let wake = wake.clone();
            std::thread::spawn(move || {
                let _ = serve_connection(stream, &tx, &*wake);
            });
        }
    });
    Ok(Server { port, rx })
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let ct = if body.is_empty() { "" } else { "Content-Type: application/json\r\n" };
    write!(stream, "HTTP/1.1 {status}\r\n{ct}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    stream.flush()
}

fn serve_connection(mut stream: TcpStream, tx: &Sender<Request>, wake: &(dyn Fn() + Send + Sync)) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut len = 0usize;
    let mut origin: Option<String> = None;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap_or(0),
                "origin" => origin = Some(v.trim().to_ascii_lowercase()),
                _ => {}
            }
        }
    }
    // Schutz vor DNS-Rebinding: nur lokale Ursprünge
    if let Some(o) = &origin {
        if !(o.starts_with("http://localhost") || o.starts_with("http://127.0.0.1") || o == "null") {
            return respond(&mut stream, "403 Forbidden", "");
        }
    }
    if !(path == "/mcp" || path == "/" || path.starts_with("/mcp?")) {
        return respond(&mut stream, "404 Not Found", "");
    }
    if method != "POST" {
        return respond(&mut stream, "405 Method Not Allowed", "");
    }
    let mut body = vec![0u8; len.min(16 * 1024 * 1024)];
    reader.read_exact(&mut body)?;
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return respond(&mut stream, "400 Bad Request", &err(&Value::Null, -32700, &format!("Parse-Fehler: {e}")).to_string()),
    };
    let (rtx, rrx) = channel();
    if tx.send(Request { body: msg, reply: rtx }).is_err() {
        return respond(&mut stream, "503 Service Unavailable", "");
    }
    wake();
    match rrx.recv_timeout(Duration::from_secs(120)) {
        Ok(Some(v)) => respond(&mut stream, "200 OK", &v.to_string()),
        Ok(None) => respond(&mut stream, "202 Accepted", ""),
        Err(_) => respond(&mut stream, "504 Gateway Timeout", ""),
    }
}

// ------------------------------------------------------------------ stdio (für Claude Desktop & Co.)

fn post(port: u16, body: &str) -> std::io::Result<Option<String>> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    write!(s, "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    let mut resp = String::new();
    s.read_to_string(&mut resp)?;
    let (head, payload) = resp.split_once("\r\n\r\n").unwrap_or((&resp, ""));
    if head.starts_with("HTTP/1.1 202") || payload.is_empty() {
        return Ok(None);
    }
    Ok(Some(payload.to_string()))
}

/// Editor ohne Fenster (für `--mcp-stdio`, wenn kein Editor läuft)
pub struct Headless {
    pub prefab_dir: PathBuf,
    pub ed: Editor,
    pub settings: Settings,
    pub job: Option<crate::compile::CompileJob>,
    pub materials: Vec<String>,
    pub models: Vec<String>,
    pub fs: Option<std::sync::Arc<crate::gamefs::GameFs>>,
}

impl Headless {
    /// Leerer Host ohne Spielinhalte (für Tests)
    #[cfg(test)]
    pub fn for_tests() -> Headless {
        Headless { prefab_dir: std::env::temp_dir().join("amboss_mcp_prefabs_test"), ed: Editor::new(crate::fgd::Fgd::builtin()), settings: Settings::default(), job: None, materials: vec![], models: vec![], fs: None }
    }
}

impl Host for Headless {
    fn ed(&mut self) -> &mut Editor {
        &mut self.ed
    }
    fn materials(&self) -> Vec<String> {
        self.materials.clone()
    }
    fn models(&self) -> Vec<String> {
        self.models.clone()
    }
    fn save(&mut self, path: Option<PathBuf>) -> Result<String, String> {
        let p = path.or_else(|| self.ed.path.clone()).ok_or("Kein Pfad angegeben")?;
        self.ed.save(p.clone())?;
        Ok(p.display().to_string())
    }
    fn open(&mut self, path: PathBuf) -> Result<String, String> {
        self.ed.load(path.clone())?;
        Ok(path.display().to_string())
    }
    fn compile(&mut self) -> Result<String, String> {
        let p = self.ed.path.clone().ok_or("Map zuerst speichern (save_map mit path)")?;
        if self.ed.dirty {
            self.ed.save(p.clone())?;
        }
        let p0 = p.clone();
        let mut path = p;
        let mut prelude = Vec::new();
        if self.settings.opt.compile_auto {
            match crate::optimize::prepare_build(&self.ed, &self.settings.opt, self.fs.as_deref(), &path) {
                Ok((out, lines, r)) => {
                    path = out;
                    prelude = lines;
                    self.show_report(&r);
                }
                Err((msg, r)) => {
                    self.show_report(&r);
                    return Err(format!("{msg}\n{}", r.lines.join("\n")));
                }
            }
        }
        if let Some(fs) = self.fs.as_deref() {
            let mut lines = crate::assets::write_addon_list(&self.ed, fs, &p0);
            if self.settings.opt.pack_content {
                lines.extend(crate::assets::write_packlist(&self.ed, fs, &p0, &path));
            }
            lines.append(&mut prelude);
            prelude = lines;
        }
        self.job = Some(crate::compile::start_with(&self.settings, &path, prelude)?);
        Ok("Kompilieren gestartet".into())
    }
    fn compile_status(&self) -> Value {
        job_status(self.job.as_ref())
    }
    fn run_command(&mut self, _id: &str) -> Result<String, String> {
        Err("Befehle sind nur im laufenden Editor verfügbar".into())
    }
    fn commands(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn known_materials(&self) -> Option<std::collections::HashSet<String>> {
        (!self.materials.is_empty()).then(|| self.materials.iter().cloned().collect())
    }
    fn prefab_dir(&self) -> PathBuf {
        self.prefab_dir.clone()
    }
    fn opt_settings(&mut self) -> &mut crate::optimize::OptSettings {
        &mut self.settings.opt
    }
    fn game_dir(&self) -> String {
        self.settings.game_dir.clone()
    }
    fn game_fs(&self) -> Option<std::sync::Arc<crate::gamefs::GameFs>> {
        self.fs.clone()
    }
}

pub fn job_status(job: Option<&crate::compile::CompileJob>) -> Value {
    match job {
        None => json!({"state": "none", "log": []}),
        Some(j) => {
            let st = match j.state() {
                JobState::Running => "running",
                JobState::Done => "done",
                JobState::Failed => "failed",
                JobState::Cancelled => "cancelled",
            };
            json!({"state": st, "leaked": j.leaked.load(std::sync::atomic::Ordering::SeqCst), "log": j.log.lock().unwrap().clone()})
        }
    }
}

/// Einstiegspunkt für `amboss --mcp-stdio [--port N] [--map datei.vmf] [--game-dir pfad]`
pub fn run_stdio(args: &[String]) {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let port: u16 = arg("--port").and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let force_headless = args.iter().any(|a| a == "--headless");
    let gui = !force_headless && TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok();
    let mut headless = (!gui).then(|| {
        let mut settings = Settings::default();
        let mut fgd = crate::fgd::Fgd::builtin();
        let mut materials = Vec::new();
        let mut models = Vec::new();
        let mut gfs = None;
        if let Some(gd) = arg("--game-dir") {
            let gd = PathBuf::from(gd);
            if let Some(root) = gd.parent() {
                settings.apply_root(root);
            }
            settings.game_dir = gd.display().to_string();
            for f in &settings.fgd_files {
                let _ = fgd.load_file(std::path::Path::new(f));
            }
            let fs = crate::gamefs::GameFs::for_gmod(&gd);
            materials = fs.list_materials();
            models = fs.list_files("models/", &[".mdl"]);
            gfs = Some(std::sync::Arc::new(fs));
        }
        let mut ed = Editor::new(fgd);
        ed.game_dir = settings.game_dir.clone();
        if let Some(m) = arg("--map") {
            let p = PathBuf::from(m);
            if p.is_file() {
                if let Err(e) = ed.load(p.clone()) {
                    eprintln!("amboss: {e}");
                }
            } else {
                ed.path = Some(p);
            }
        }
        let prefab_dir = arg("--prefab-dir").map(PathBuf::from).unwrap_or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("prefabs"))).unwrap_or_default());
        Headless { prefab_dir, ed, settings, job: None, materials, models, fs: gfs }
    });
    eprintln!("amboss MCP (stdio): {}", if gui { format!("verbunden mit Editor auf Port {port}") } else { "ohne Fenster".into() });
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply: Option<String> = match &mut headless {
            Some(h) => match serde_json::from_str::<Value>(&line) {
                Ok(msg) => handle(&msg, h).map(|v| v.to_string()),
                Err(e) => Some(err(&Value::Null, -32700, &format!("Parse-Fehler: {e}")).to_string()),
            },
            None => match post(port, &line) {
                Ok(r) => r,
                Err(e) => {
                    let id = serde_json::from_str::<Value>(&line).ok().and_then(|v| v.get("id").cloned()).unwrap_or(Value::Null);
                    (!id.is_null()).then(|| err(&id, -32603, &format!("Editor nicht erreichbar: {e}")).to_string())
                }
            },
        };
        if let Some(r) = reply {
            let _ = writeln!(stdout, "{r}");
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;

    fn host() -> Headless {
        Headless { prefab_dir: std::env::temp_dir().join("amboss_mcp_prefabs_test"), ed: Editor::new(Fgd::builtin()), settings: Settings::default(), job: None, materials: vec![], models: vec![], fs: None }
    }

    fn call(h: &mut Headless, name: &str, args: Value) -> Value {
        let r = handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}), h).unwrap();
        assert_eq!(r["result"]["isError"], false, "{name}: {r}");
        r["result"]["structuredContent"].clone()
    }

    #[test]
    fn protocol_basics() {
        let mut h = host();
        let r = handle(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}), &mut h).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert!(handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), &mut h).is_none());
        let tools = handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}), &mut h).unwrap();
        assert!(tools["result"]["tools"].as_array().unwrap().len() > 30);
        let bad = handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "gibtsnicht"}), &mut h).unwrap();
        assert_eq!(bad["error"]["code"], -32601);
    }

    #[test]
    fn merge_via_mcp() {
        let dir = std::env::temp_dir().join(format!("amboss_merge_mcp_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut b = host();
        call(&mut b, "create_room", json!({"min": [-128, -128, 0], "max": [128, 128, 128]}));
        call(&mut b, "create_entity", json!({"classname": "light", "origin": [0, 0, 100], "keys": {"targetname": "lampe"}}));
        let bp = dir.join("b.vmf");
        call(&mut b, "save_map", json!({"path": bp.display().to_string()}));
        let mut h = host();
        call(&mut h, "create_room", json!({"min": [-256, -256, 0], "max": [256, 256, 192]}));
        call(&mut h, "create_entity", json!({"classname": "light", "origin": [0, 0, 150], "keys": {"targetname": "lampe"}}));
        let r = call(&mut h, "merge_map", json!({"path": bp.display().to_string(), "placement": "beside"}));
        assert_eq!(r["entities"], 1);
        assert_eq!(r["renamed"][0]["to"], "lampe_2");
        assert!(r["offset"][0].as_f64().unwrap() > 256.0);
        assert_eq!(h.ed.map.world.solids.len(), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn optimize_via_mcp() {
        let mut h = host();
        call(&mut h, "create_room", json!({"min": [-512, -256, 0], "max": [512, 256, 192], "wall_thickness": 16}));
        call(&mut h, "create_entity", json!({"classname": "info_player_start", "origin": [0, 0, 16]}));
        call(&mut h, "create_entity", json!({"classname": "light", "origin": [0, 0, 160]}));
        let before = h.ed.map.entities.len();
        let check = call(&mut h, "optimize_map", json!({"options": {"node_spacing": 192}}));
        assert_eq!(check["applied"], false);
        assert!(check["leak"].is_null(), "{check}");
        assert_eq!(h.ed.map.entities.len(), before, "Prüfen darf die Map nicht ändern");
        assert!(check["node_links"].as_u64().unwrap() > 0);
        assert!(!h.ed.opt_links.is_empty());
        let r = call(&mut h, "optimize_map", json!({"apply": true}));
        assert_eq!(r["applied"], true);
        assert!(r["map_counts"]["info_node"].as_u64().unwrap() >= 8, "{r}");
        call(&mut h, "undo", json!({}));
        assert_eq!(h.ed.map.entities.len(), before);
        let st = call(&mut h, "optimize_settings", json!({"set": {"compile_auto": true, "areaportals": true}}));
        assert_eq!(st["compile_auto"], true);
        assert!(h.settings.opt.areaportals);
        let bad = handle(&json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "optimize_settings", "arguments": {"set": {"gibtsnicht": 1}}}}), &mut h).unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }

    #[test]
    fn build_a_room_via_mcp() {
        let mut h = host();
        let room = call(&mut h, "create_room", json!({"min": [-256, -256, 0], "max": [256, 256, 192], "wall_thickness": 16}));
        assert_eq!(room["ids"].as_array().unwrap().len(), 6);
        let spawn = call(&mut h, "create_entity", json!({"classname": "info_player_start", "origin": [0, 0, 16], "angles": [0, 90, 0]}));
        let light = call(&mut h, "create_entity", json!({"classname": "light", "origin": [0, 0, 160], "keys": {"_light": "255 200 150 300"}}));
        let lid = light["id"].as_u64().unwrap();
        call(&mut h, "set_entity_keys", json!({"id": lid, "keys": {"targetname": "lampe"}}));
        let btn = call(&mut h, "create_brush", json!({"min": [200, -16, 48], "max": [208, 16, 80], "material": "DEV/DEV_MEASUREGENERIC01"}));
        let bid = btn["ids"][0].as_u64().unwrap();
        let ent = call(&mut h, "tie_to_entity", json!({"ids": [bid], "classname": "func_button"}));
        let eid = ent["entity_id"].as_u64().unwrap();
        call(&mut h, "add_output", json!({"id": eid, "output": "OnPressed", "target": "lampe", "input": "Toggle"}));
        let stairs = call(&mut h, "create_brush", json!({"min": [-200, -200, 0], "max": [-72, -72, 96], "shape": "stairs", "sides": 6}));
        assert_eq!(stairs["ids"].as_array().unwrap().len(), 6);
        call(&mut h, "rotate_objects", json!({"ids": [spawn["id"]], "axis": "z", "degrees": 90}));
        let pf = call(&mut h, "list_prefabs", json!({"filter": "tür"}));
        assert!(pf["count"].as_u64().unwrap() >= 2);
        let ins = call(&mut h, "insert_prefab", json!({"prefab": "Schiebetür mit Knöpfen", "position": [0, 240, 0], "surface_normal": [0, 0, 1]}));
        assert!(ins["ids"].as_array().unwrap().len() >= 4);
        assert_eq!(ins["entity_names"][0], "tuer");
        let info = call(&mut h, "get_map_info", json!({}));
        assert_eq!(info["entities"], 6);
        let probs = call(&mut h, "check_problems", json!({}));
        assert!(probs["problems"].as_array().unwrap().iter().all(|p| p["severity"] != "Error"), "{probs}");
        let e = call(&mut h, "get_entity", json!({"id": eid}));
        assert_eq!(e["outputs"][0]["output"], "OnPressed");
        call(&mut h, "undo", json!({"steps": 2}));
        call(&mut h, "redo", json!({"steps": 2}));
        // Fehlerfall
        let r = handle(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "delete_objects", "arguments": {"ids": [999999]}}}), &mut h).unwrap();
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn http_roundtrip() {
        let server = start_server(0, || {}).err();
        // Port 0 ist für bind erlaubt, aber wir brauchen den echten Port → eigener Listener-Test
        assert!(server.is_none());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let srv = start_server(port, || {}).unwrap();
        let t = std::thread::spawn(move || post(port, r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#).unwrap());
        let req = srv.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut h = host();
        req.reply.send(handle(&req.body, &mut h)).unwrap();
        let resp = t.join().unwrap().unwrap();
        assert!(resp.contains("\"id\":5"));
    }
}
