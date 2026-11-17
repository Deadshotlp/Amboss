//! Live-Abgleich mit dem laufenden Spiel (Garry's Mod). Amboss legt eine kleine Lua-Brücke als
//! Addon ab; beide Seiten tauschen Dateien in `garrysmod/data/amboss_live/` aus (kein Netzwerk,
//! keine Cheats nötig):
//!
//! * `in.txt`      Amboss → Spiel: Befehle mit fortlaufender Nummer (Entities ändern/erzeugen/
//!                  entfernen, Inputs auslösen, Spieler versetzen, Map neu laden)
//! * `brushes.txt` Amboss → Spiel: neue/geänderte Brushes als begehbare Vorschau
//! * `start.txt`   Startposition für „Im Spiel testen ab 3D-Kamera“
//! * `out.txt`     Spiel → Amboss: Map, Spielerposition, zuletzt ausgeführter Befehl
//!
//! Entities der kompilierten Map werden über ihre Hammer-ID gefunden (VBSP schreibt die VMF-ID
//! als `hammerid`). Was sich ohne Kompilieren nicht ändern lässt (Lichter, Weltgeometrie
//! entfernen, Brush-Entities neu anlegen), wird gezählt und angezeigt.

use crate::editor::Editor;
use crate::map::{Entity, Map, Shared, Solid};
use glam::DVec3;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Lua-Brücke (wird nach `garrysmod/addons/amboss_live/lua/autorun/` geschrieben)
pub const LUA: &str = include_str!("../assets/live/amboss_live.lua");
/// Version der Brücke (steht in der ersten Zeile der Lua-Datei)
pub const LUA_VERSION: u32 = 2;

/// Zustand des Spiels laut `out.txt`
#[derive(Clone, Debug, Default)]
pub struct GameStatus {
    pub map: String,
    pub seq: u64,
    pub pos: DVec3,
    /// Source-Winkel (Pitch nach unten positiv, Yaw, Roll)
    pub ang: DVec3,
    /// Zeitpunkt des Map-Starts (ändert sich beim Neuladen)
    pub loaded: f64,
    pub error: String,
    pub version: u32,
}

/// Was beim letzten Abgleich übertragen wurde
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub updated: usize,
    pub created: usize,
    pub removed: usize,
    pub ghost_brushes: usize,
    pub removed_brushes: usize,
    /// Änderungen, die Kompilieren brauchen (Lichter, worldspawn, neue Brush-Entities …)
    pub needs_compile: Vec<String>,
}

#[derive(Clone, PartialEq)]
struct EntState {
    class: String,
    props: Vec<(String, String)>,
    outputs: Vec<(String, String)>,
}

impl EntState {
    fn of(e: &Entity) -> Self {
        EntState { class: e.classname().to_string(), props: e.props.clone(), outputs: e.outputs.clone() }
    }
}

pub struct Live {
    /// Änderungen automatisch übertragen
    pub auto: bool,
    /// 3D-Kamera folgt dem Spieler
    pub follow: bool,
    /// Stand der kompilierten Map (Bezug für alle Änderungen)
    baseline: Option<Map>,
    /// Was das Spiel nach unseren Befehlen hat (Entity-ID → Zustand)
    shadow: HashMap<u32, EntState>,
    /// Im Spiel live erzeugte Entities
    created: HashSet<u32>,
    seq: u64,
    queue: Vec<Value>,
    ghost_sig: u64,
    last_rev: u64,
    last_poll: Option<Instant>,
    last_sync: Option<Instant>,
    pub status: Option<GameStatus>,
    /// `out.txt` zuletzt geändert
    status_time: Option<SystemTime>,
    /// Änderungszeit von alive.txt (Lebenszeichen des Clients, läuft auch bei pausiertem Spiel)
    pub alive_time: Option<SystemTime>,
    pub summary: Summary,
    pub log: Vec<String>,
}

impl Default for Live {
    fn default() -> Self {
        Live { auto: true, follow: false, baseline: None, shadow: HashMap::new(), created: HashSet::new(), seq: 0, queue: Vec::new(), ghost_sig: 0, last_rev: u64::MAX, last_poll: None, last_sync: None, status: None, status_time: None, alive_time: None, summary: Summary::default(), log: Vec::new() }
    }
}

pub fn data_dir(game_dir: &str) -> PathBuf {
    Path::new(game_dir).join("data").join("amboss_live")
}

pub fn lua_path(game_dir: &str) -> PathBuf {
    Path::new(game_dir).join("addons").join("amboss_live").join("lua").join("autorun").join("amboss_live.lua")
}

/// Für die Anzeige: Alter einer Austauschdatei in Sekunden (None = fehlt)
pub fn file_age(game_dir: &str, name: &str) -> Option<f64> {
    let m = std::fs::metadata(data_dir(game_dir).join(name)).ok()?.modified().ok()?;
    Some(m.elapsed().map(|d| d.as_secs_f64()).unwrap_or(0.0))
}

/// Brücke installiert und aktuell?
pub fn installed(game_dir: &str) -> Option<u32> {
    let s = std::fs::read_to_string(lua_path(game_dir)).ok()?;
    s.lines().next()?.rsplit(' ').next()?.trim().parse().ok()
}

/// Lua-Brücke ins Spiel schreiben (ein Neustart von GMod lädt das Addon)
pub fn install(game_dir: &str) -> Result<PathBuf, String> {
    if game_dir.trim().is_empty() {
        return Err("Kein Spielverzeichnis eingestellt".into());
    }
    let p = lua_path(game_dir);
    std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {e}", p.display()))?;
    std::fs::write(&p, LUA).map_err(|e| format!("{}: {e}", p.display()))?;
    let info = Path::new(game_dir).join("addons").join("amboss_live").join("addon.json");
    let _ = std::fs::write(info, "{\n\t\"title\": \"Amboss Live\",\n\t\"type\": \"tool\",\n\t\"tags\": [\"build\"],\n\t\"ignore\": []\n}\n");
    std::fs::create_dir_all(data_dir(game_dir)).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Datei ersetzen, ohne dass das Spiel eine halbe Datei liest
fn write_atomic(p: &Path, text: &str) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, p)
}

fn arr(v: DVec3) -> Value {
    json!([v.x, v.y, v.z])
}

fn parse_v(v: &Value) -> DVec3 {
    let f = |i: usize| v.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    DVec3::new(f(0), f(1), f(2))
}

/// Klassen, die zur Laufzeit nichts bewirken oder vom Compiler verarbeitet werden
fn compile_only(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    matches!(c.as_str(), "light" | "light_spot" | "light_environment" | "env_cubemap" | "info_lighting" | "worldspawn" | "func_detail" | "info_node" | "info_node_air" | "info_overlay" | "infodecal" | "func_instance" | "env_fog_controller_dummy")
        || c.starts_with("info_node")
}

/// Brushes, die im Spiel nicht fest sind (keine Kollision in der Vorschau)
fn nonsolid_owner(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("trigger_") || c == "func_illusionary" || c == "func_areaportal" || c == "func_viscluster" || c == "func_occluder" || c == "func_ladder"
}

fn same_solid(a: &Shared<Solid>, b: &Shared<Solid>) -> bool {
    Shared::ptr_eq(a, b) || (a.polys == b.polys && a.sides.iter().zip(&b.sides).all(|(x, y)| x.material == y.material && x.uaxis == y.uaxis && x.vaxis == y.vaxis) && a.sides.len() == b.sides.len())
}

impl Live {
    pub fn has_baseline(&self) -> bool {
        self.baseline.is_some()
    }

    /// Stand der kompilierten Map merken (beim Kompilieren oder per Knopf)
    pub fn set_baseline(&mut self, map: &Map) {
        self.baseline = Some(map.clone());
        self.reset_shadow();
    }

    fn reset_shadow(&mut self) {
        self.shadow = self.baseline.as_ref().map(|m| m.entities.iter().map(|e| (e.id, EntState::of(e))).collect()).unwrap_or_default();
        self.created.clear();
        self.queue.clear();
        self.ghost_sig = 0;
        self.last_rev = u64::MAX;
    }

    /// Spiel antwortet (out.txt jünger als 3 s)
    pub fn connected(&self) -> bool {
        let fresh = |t: Option<SystemTime>| t.and_then(|t| t.elapsed().ok()).is_some_and(|d| d < Duration::from_secs(3));
        fresh(self.status_time) || (self.status.is_some() && fresh(self.alive_time))
    }

    fn push(&mut self, mut cmd: Value) {
        self.seq += 1;
        cmd["seq"] = json!(self.seq);
        self.queue.push(cmd);
        if self.queue.len() > 400 {
            self.queue.drain(..self.queue.len() - 400);
        }
    }

    fn flush(&mut self, game_dir: &str) {
        let acked = self.status.as_ref().map(|s| s.seq).unwrap_or(0);
        self.queue.retain(|c| c["seq"].as_u64().unwrap_or(0) > acked);
        let dir = data_dir(game_dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = write_atomic(&dir.join("in.txt"), &json!({ "cmds": self.queue }).to_string());
    }

    /// Befehl sofort senden (Spieler versetzen, Input auslösen …)
    pub fn send(&mut self, game_dir: &str, cmd: Value) {
        self.push(cmd);
        self.flush(game_dir);
    }

    pub fn teleport(&mut self, game_dir: &str, pos: DVec3, ang: DVec3) {
        self.send(game_dir, json!({ "op": "teleport", "pos": arr(pos), "ang": arr(ang) }));
    }

    pub fn fire(&mut self, game_dir: &str, target: &str, input: &str, param: &str) {
        self.send(game_dir, json!({ "op": "fire", "target": target, "input": input, "param": param }));
    }

    pub fn reload_map(&mut self, game_dir: &str, map: &str) {
        self.send(game_dir, json!({ "op": "map", "map": map }));
    }

    /// Startposition für den nächsten Map-Start
    pub fn write_start(game_dir: &str, map: &str, pos: DVec3, ang: DVec3) -> std::io::Result<()> {
        let dir = data_dir(game_dir);
        std::fs::create_dir_all(&dir)?;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        write_atomic(&dir.join("start.txt"), &json!({ "map": map.to_ascii_lowercase(), "pos": arr(pos), "ang": arr(ang), "time": now }).to_string())
    }

    /// `out.txt` lesen (höchstens alle 0,3 s)
    pub fn poll(&mut self, game_dir: &str) {
        if game_dir.is_empty() || self.last_poll.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return;
        }
        self.last_poll = Some(Instant::now());
        self.alive_time = std::fs::metadata(data_dir(game_dir).join("alive.txt")).ok().and_then(|m| m.modified().ok());
        let p = data_dir(game_dir).join("out.txt");
        let Ok(meta) = std::fs::metadata(&p) else { return };
        let mtime = meta.modified().ok();
        if mtime == self.status_time {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { return };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { return };
        let st = GameStatus {
            map: v["map"].as_str().unwrap_or("").to_string(),
            // GMod schreibt Zahlen auch als Kommazahl (5.0) – deshalb über f64 lesen
            seq: v["seq"].as_f64().unwrap_or(0.0).max(0.0) as u64,
            pos: parse_v(&v["pos"]),
            ang: parse_v(&v["ang"]),
            loaded: v["loaded"].as_f64().unwrap_or(0.0),
            error: v["err"].as_str().unwrap_or("").to_string(),
            version: v["version"].as_f64().unwrap_or(0.0).max(0.0) as u32,
        };
        // Map neu geladen: alles, was live geändert wurde, ist weg → neu übertragen
        if self.status.as_ref().is_none_or(|o| o.loaded != st.loaded || o.map != st.map) {
            if self.baseline.is_none() {
                self.log.push(format!("Verbunden mit {} – Bezug ist der jetzige Stand der Map", st.map));
            } else {
                self.log.push(format!("Map geladen: {}", st.map));
            }
            self.reset_shadow();
        }
        if !st.error.is_empty() && self.status.as_ref().is_none_or(|o| o.error != st.error) {
            self.log.push(format!("Fehler im Spiel: {}", st.error));
        }
        self.status = Some(st);
        self.status_time = mtime;
        if self.log.len() > 50 {
            self.log.drain(..self.log.len() - 50);
        }
    }

    /// Änderungen seit dem Bezug ins Spiel übertragen (höchstens alle 0,4 s, nur wenn verbunden)
    pub fn sync(&mut self, ed: &Editor, game_dir: &str, tex_size: &dyn Fn(&str) -> (f64, f64), force: bool) {
        if !self.connected() || (!self.auto && !force) {
            return;
        }
        if self.baseline.is_none() {
            self.set_baseline(&ed.map);
        }
        if !force && (ed.rev == self.last_rev || self.last_sync.is_some_and(|t| t.elapsed() < Duration::from_millis(400))) {
            return;
        }
        self.last_rev = ed.rev;
        self.last_sync = Some(Instant::now());
        let base = self.baseline.clone().unwrap();
        let mut sum = Summary::default();
        let mut cmds = Vec::new();
        // ---- Entities
        let cur: HashMap<u32, &Shared<Entity>> = ed.map.entities.iter().map(|e| (e.id, e)).collect();
        for e in &ed.map.entities {
            let now = EntState::of(e);
            match self.shadow.get(&e.id) {
                Some(old) if *old == now => {}
                Some(old) => {
                    if compile_only(&now.class) {
                        sum.needs_compile.push(format!("{} #{}", now.class, e.id));
                        continue;
                    }
                    let kv: serde_json::Map<String, Value> = now.props.iter().filter(|(k, v)| !old.props.iter().any(|(k2, v2)| k2 == k && v2 == v)).map(|(k, v)| (k.clone(), json!(v))).collect();
                    let outputs: Vec<Value> = if old.outputs != now.outputs { now.outputs.iter().map(|(k, v)| json!([k, v])).collect() } else { Vec::new() };
                    let live = self.created.contains(&e.id);
                    cmds.push(json!({ "op": "update", "id": e.id, "live": live, "class": now.class, "kv": kv, "outputs": outputs }));
                    sum.updated += 1;
                    self.shadow.insert(e.id, now);
                }
                None => {
                    if compile_only(&now.class) || e.is_brush_entity() {
                        sum.needs_compile.push(format!("{} #{}", now.class, e.id));
                        continue;
                    }
                    let kv: serde_json::Map<String, Value> = now.props.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
                    let outputs: Vec<Value> = now.outputs.iter().map(|(k, v)| json!([k, v])).collect();
                    cmds.push(json!({ "op": "create", "id": e.id, "class": now.class, "kv": kv, "outputs": outputs }));
                    self.created.insert(e.id);
                    sum.created += 1;
                    self.shadow.insert(e.id, now);
                }
            }
        }
        let gone: Vec<u32> = self.shadow.keys().copied().filter(|id| !cur.contains_key(id)).collect();
        for id in gone {
            let st = self.shadow.remove(&id).unwrap();
            if compile_only(&st.class) {
                sum.needs_compile.push(format!("{} #{id}", st.class));
                continue;
            }
            cmds.push(json!({ "op": "remove", "id": id, "live": self.created.remove(&id) }));
            sum.removed += 1;
        }
        for c in cmds {
            self.push(c);
        }
        // ---- Brushes: neue/geänderte als Vorschau
        let base_solids: HashMap<u32, &Shared<Solid>> = base.world.solids.iter().chain(base.entities.iter().flat_map(|e| e.solids.iter())).map(|s| (s.id, s)).collect();
        let mut ghosts: Vec<(Option<&Entity>, &Shared<Solid>)> = Vec::new();
        let mut seen = HashSet::new();
        let world = ed.map.world.solids.iter().map(|s| (None, s));
        let ents = ed.map.entities.iter().flat_map(|e| e.solids.iter().map(move |s| (Some(&**e), s)));
        for (owner, s) in world.chain(ents) {
            seen.insert(s.id);
            if base_solids.get(&s.id).is_some_and(|b| same_solid(b, s)) {
                continue;
            }
            ghosts.push((owner, s));
        }
        sum.removed_brushes = base_solids.keys().filter(|id| !seen.contains(id)).count();
        sum.ghost_brushes = ghosts.len();
        let sig = ghosts.iter().fold(ghosts.len() as u64 + 1, |h, (_, s)| h.wrapping_mul(1_000_003).wrapping_add(s.as_ptr() as u64 ^ s.id as u64));
        if sig != self.ghost_sig {
            self.ghost_sig = sig;
            let list: Vec<Value> = ghosts.iter().map(|(owner, s)| ghost_json(s, owner.is_some_and(|e| nonsolid_owner(e.classname())), tex_size)).collect();
            let dir = data_dir(game_dir);
            let _ = std::fs::create_dir_all(&dir);
            let _ = write_atomic(&dir.join("brushes.txt"), &json!({ "rev": sig, "brushes": list }).to_string());
        }
        if !sum.needs_compile.is_empty() || sum != self.summary {
            sum.needs_compile.sort();
            sum.needs_compile.dedup();
        }
        self.summary = sum;
        self.flush(game_dir);
    }
}

/// Brush für die Vorschau: Hülle (Punkte) für die Kollision, Dreiecke mit Texturkoordinaten
fn ghost_json(s: &Solid, nonsolid: bool, tex_size: &dyn Fn(&str) -> (f64, f64)) -> Value {
    let mut hull: Vec<f64> = Vec::new();
    let mut faces: Vec<Value> = Vec::new();
    for (i, poly) in s.polys.iter().enumerate() {
        if poly.len() < 3 {
            continue;
        }
        let side = &s.sides[i];
        for p in poly {
            hull.extend([p.x, p.y, p.z]);
        }
        let m = side.material.to_ascii_lowercase();
        if m.starts_with("tools/") {
            continue; // Werkzeug-Texturen sind im Spiel unsichtbar
        }
        let (tw, th) = tex_size(&side.material);
        let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
        let uv = |p: DVec3| ((p.dot(side.uaxis.axis) / side.uaxis.scale.max(1e-4) + side.uaxis.shift) / tw, (p.dot(side.vaxis.axis) / side.vaxis.scale.max(1e-4) + side.vaxis.shift) / th);
        let mut v: Vec<f64> = Vec::new();
        let pts: Vec<DVec3> = match s.disps.get(i) {
            Some(Some(g)) => g.triangles().into_iter().flat_map(|t| [g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]]).collect(),
            _ => (1..poly.len() - 1).flat_map(|k| [poly[0], poly[k], poly[k + 1]]).collect(),
        };
        for t in pts.chunks(3) {
            // Source zeichnet im Uhrzeigersinn → Reihenfolge umdrehen
            for p in [t[0], t[2], t[1]] {
                let (u, w) = uv(p);
                v.extend([p.x, p.y, p.z, u, w, n.x, n.y, n.z]);
            }
        }
        faces.push(json!({ "mat": m, "v": v }));
    }
    json!({ "id": s.id, "solid": !nonsolid && !s.has_disp(), "hull": hull, "faces": faces })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Aabb;

    #[test]
    fn diff_entities_and_brushes() {
        let dir = std::env::temp_dir().join(format!("amboss_live_{}", std::process::id()));
        let gd = dir.to_string_lossy().to_string();
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "prop_physics");
        e.set("origin", "0 0 0");
        e.set("model", "models/props_c17/oildrum001.mdl");
        ed.map.entities.push(e.into());
        let mut live = Live::default();
        live.set_baseline(&ed.map);
        // Spiel „verbunden“
        std::fs::create_dir_all(data_dir(&gd)).unwrap();
        std::fs::write(data_dir(&gd).join("out.txt"), r#"{"map":"test","seq":0,"pos":[1,2,3],"ang":[0,90,0],"loaded":5}"#).unwrap();
        live.poll(&gd);
        assert!(live.connected());
        assert_eq!(live.status.as_ref().unwrap().pos, DVec3::new(1.0, 2.0, 3.0));
        // verschieben, neues Licht, neue Kiste, neuer Brush
        ed.map.entity_mut(id).unwrap().set("origin", "64 0 0");
        let l = ed.map.alloc_id();
        let mut le = Entity::new(l, "light");
        le.set("origin", "0 0 64");
        ed.map.entities.push(le.into());
        let k = ed.map.alloc_id();
        let mut ke = Entity::new(k, "prop_dynamic");
        ke.set("origin", "0 64 0");
        ed.map.entities.push(ke.into());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "BRICK/BRICKWALL001A", 0.25);
        ed.map.world.solids.push(s.into());
        ed.rev += 1;
        live.sync(&ed, &gd, &|_| (512.0, 512.0), true);
        assert_eq!((live.summary.updated, live.summary.created, live.summary.removed, live.summary.ghost_brushes), (1, 1, 0, 1), "{:?}", live.summary);
        assert_eq!(live.summary.needs_compile.len(), 1, "Licht braucht Kompilieren");
        let inp: Value = serde_json::from_str(&std::fs::read_to_string(data_dir(&gd).join("in.txt")).unwrap()).unwrap();
        let cmds = inp["cmds"].as_array().unwrap();
        assert_eq!(cmds.len(), 2);
        let upd = cmds.iter().find(|c| c["op"] == "update").unwrap();
        assert_eq!(upd["kv"]["origin"], "64 0 0");
        assert!(upd["kv"].get("model").is_none(), "nur geänderte Schlüssel");
        let br: Value = serde_json::from_str(&std::fs::read_to_string(data_dir(&gd).join("brushes.txt")).unwrap()).unwrap();
        assert_eq!(br["brushes"][0]["faces"].as_array().unwrap().len(), 6);
        // erneuter Abgleich ohne Änderung sendet nichts Neues; Entfernen der Kiste schon
        let before = live.seq;
        live.sync(&ed, &gd, &|_| (512.0, 512.0), true);
        assert_eq!(live.seq, before);
        ed.map.entities.retain(|e| e.id != k);
        live.sync(&ed, &gd, &|_| (512.0, 512.0), true);
        assert_eq!(live.summary.removed, 1);
        // Spiel bestätigt → Warteschlange wird kürzer
        std::fs::write(data_dir(&gd).join("out.txt"), format!(r#"{{"map":"test","seq":{},"loaded":5}}"#, live.seq)).unwrap();
        live.last_poll = None;
        live.status_time = None;
        live.poll(&gd);
        live.flush(&gd);
        assert!(live.queue.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Die Lua-Brücke mit nachgebauten GMod-Funktionen ausführen: Befehle aus in.txt wirken auf
    /// Entities, out.txt wird geschrieben, Vorschau-Brushes erzeugen ein Kollisionsobjekt
    #[test]
    fn lua_bridge_runs_with_gmod_mock() {
        let lua = mlua::Lua::new();
        let mock = r#"
SERVER, CLIENT = true, false
FILES = {}
function AddCSLuaFile() end
game = { IsDedicated = function() return false end, GetMap = function() return "gm_test" end, ConsoleCommand = function(s) LASTCMD = s end }
file = {
  Read = function(p) return FILES[p] end,
  Write = function(p, s) FILES[p] = s end,
  CreateDir = function() end,
  Delete = function(p) FILES[p] = nil end,
}
util = { JSONToTable = function(s) return json_decode(s) end, TableToJSON = function(t) return json_encode(t) end, CRC = function(s) return "1" end }
TIMERS, HOOKS = {}, {}
timer = { Create = function(n, d, r, f) TIMERS[n] = f end, Simple = function(d, f) f() end }
hook = { Add = function(ev, n, f) HOOKS[ev .. "/" .. n] = f end }
concommand = { Add = function(n, f) CMDS = CMDS or {} CMDS[n] = f end }
scripted_ents = { Register = function(t, n) REG, REGT = n, t end }
SOLID_VPHYSICS, MOVETYPE_NONE, RENDERGROUP_OPAQUE, RENDERMODE_TRANSCOLOR = 6, 0, 7, 1
function SysTime() return 1.5 end
function IsValid(e) return e ~= nil and not e.removed end
local VMT = {}
VMT.__index = VMT
function Vector(x, y, z)
  if type(x) == "string" then local a, b, c = x:match("(%S+)%s+(%S+)%s+(%S+)") return setmetatable({ x = tonumber(a), y = tonumber(b), z = tonumber(c) }, VMT) end
  return setmetatable({ x = x or 0, y = y or 0, z = z or 0 }, VMT)
end
function Angle(p, y, r)
  if type(p) == "string" then local a, b, c = p:match("(%S+)%s+(%S+)%s+(%S+)") return { p = tonumber(a), y = tonumber(b), r = tonumber(c) } end
  return { p = p or 0, y = y or 0, r = r or 0 }
end
function Color(r, g, b, a) return { r = r, g = g, b = b, a = a or 255 } end
string.Explode = function(sep, s) local t = {} for w in s:gmatch("[^" .. sep .. "]+") do t[#t + 1] = w end return t end
string.Replace = function(s, a, b) return (s:gsub(a, b)) end
local function ent(class, kv)
  local e = { class = class, kv = kv or {}, removed = false, fired = {} }
  function e:GetKeyValues() return self.kv end
  function e:SetKeyValue(k, v) self.kv[k] = v end
  function e:SetPos(p) self.pos = p end
  function e:SetAngles(a) self.ang = a end
  function e:SetModel(m) self.model = m end
  function e:SetSkin(s) self.skin = s end
  function e:GetColor() return self.col or Color(255, 255, 255, 255) end
  function e:SetColor(c) self.col = c end
  function e:SetRenderMode() end
  function e:SetModelScale(s) self.scale = s end
  function e:SetName(n) self.kv.targetname = n end
  function e:Spawn() self.spawned = true if self.Initialize then self:Initialize() end end
  function e:Activate() end
  function e:Remove() self.removed = true end
  function e:Fire(i, p) self.fired[#self.fired + 1] = i end
  function e:GetPhysicsObject() return { EnableMotion = function() end } end
  function e:PhysicsInitMultiConvex(h) self.hulls = h end
  function e:SetSolid() end function e:SetMoveType() end function e:EnableCustomCollisions() end
  function e:SetRenderBounds() end
  return e
end
WORLD = { ent("prop_physics", { hammerid = 7, targetname = "tor" }) }
ents = {
  GetAll = function() return WORLD end,
  Create = function(c) local e = ent(c) WORLD[#WORLD + 1] = e if c == REG then BRUSHENT = e for k, v in pairs(REGT) do e[k] = v end end return e end,
  FindByName = function(n) local r = {} for _, e in ipairs(WORLD) do if e.kv.targetname == n then r[#r + 1] = e end end return r end,
}
PLAYER = ent("player")
function PLAYER:EyeAngles() return Angle(10, 90, 0) end
function PLAYER:GetPos() return Vector(1, 2, 3) end
function PLAYER:SetEyeAngles(a) self.eye = a end
function PLAYER:SetLocalVelocity() end
player = { GetHumans = function() return { PLAYER } end }
"#;
        let g = lua.globals();
        g.set("json_decode", lua.create_function(|l, s: String| { let v: serde_json::Value = serde_json::from_str(&s).map_err(mlua::Error::external)?; mlua::LuaSerdeExt::to_value(l, &v) }).unwrap()).unwrap();
        g.set("json_encode", lua.create_function(|l, t: mlua::Value| { let v: serde_json::Value = mlua::LuaSerdeExt::from_value(l, t)?; Ok(v.to_string()) }).unwrap()).unwrap();
        lua.load(mock).exec().unwrap();
        // Client-Seite: Lebenszeichen über Echtzeit (läuft auch bei pausiertem Spiel)
        {
            let cl = mlua::Lua::new();
            let cg = cl.globals();
            cg.set("json_decode", cl.create_function(|l, s: String| { let v: serde_json::Value = serde_json::from_str(&s).map_err(mlua::Error::external)?; mlua::LuaSerdeExt::to_value(l, &v) }).unwrap()).unwrap();
            cg.set("json_encode", cl.create_function(|l, t: mlua::Value| { let v: serde_json::Value = mlua::LuaSerdeExt::from_value(l, t)?; Ok(v.to_string()) }).unwrap()).unwrap();
            cl.load(mock).exec().unwrap();
            cl.load("SERVER, CLIENT = false, true function RealTime() return 10 end function LocalPlayer() return PLAYER end").exec().unwrap();
            cl.load(LUA).set_name("amboss_live.lua").exec().expect("Lua-Brücke lädt auf dem Client");
            let alive: String = cl.load(r#"HOOKS["Think/AmbossLiveAlive"]() return json_decode(FILES["amboss_live/alive.txt"]).map"#).eval().expect("alive.txt");
            assert_eq!(alive, "gm_test");
            assert!(cl.load(r#"return FILES["amboss_live/out.txt"] == nil"#).eval::<bool>().unwrap(), "out.txt schreibt nur der Server");
        }
        lua.load(LUA).set_name("amboss_live.lua").exec().expect("Lua-Brücke lädt");
        // Map-Start, dann Befehle
        lua.load(r#"
HOOKS["InitPostEntity/AmbossLive"]()
FILES["amboss_live/in.txt"] = json_encode({ cmds = {
  { seq = 1, op = "update", id = 7, kv = { origin = "64 0 0", rendercolor = "255 0 0" }, outputs = { { "OnBreak", "tor\27Open\27\0270\27-1" } } },
  { seq = 2, op = "create", id = 50, class = "prop_static", kv = { model = "models/a.mdl", origin = "1 2 3" }, outputs = {} },
  { seq = 3, op = "fire", target = "tor", input = "Open", param = "" },
  { seq = 4, op = "teleport", pos = { 5, 6, 7 }, ang = { 0, 45, 0 } },
  { seq = 5, op = "remove", id = 50, live = true },
} })
TIMERS["AmbossLiveIn"]()
TIMERS["AmbossLiveOut"]()
FILES["amboss_live/brushes.txt"] = json_encode({ rev = 9, brushes = { { id = 1, solid = true, hull = { 0,0,0, 64,0,0, 0,64,0, 0,0,64, 64,64,64 }, faces = {} } } })
TIMERS["AmbossLiveBrushes"]()
"#).exec().expect("Befehle laufen");
        let check: (f64, String, String, bool, f64, bool, usize, String) = lua.load(r#"
local t = WORLD[1]
local made = WORLD[2]
local out = json_decode(FILES["amboss_live/out.txt"])
return t.pos.x, t.col.r .. "," .. t.col.g, made.class, made.removed, PLAYER.pos.z, t.fired[1] == "Open", #BRUSHENT.hulls == 0 and 0 or #BRUSHENT.hulls[1], out.map .. "/" .. out.seq .. "/" .. out.err
"#).eval().unwrap();
        assert_eq!(check.0, 64.0, "verschoben");
        assert_eq!(check.1, "255,0");
        assert_eq!(check.2, "prop_dynamic", "prop_static wird zu prop_dynamic");
        assert!(check.3, "wieder entfernt");
        assert_eq!(check.4, 7.0, "Spieler versetzt");
        assert!(check.5, "Input ausgelöst");
        assert_eq!(check.6, 5, "Kollisionshülle mit 5 Punkten");
        assert_eq!(check.7, "gm_test/5/", "Antwort: Map, letzter Befehl, kein Fehler");
    }
}
