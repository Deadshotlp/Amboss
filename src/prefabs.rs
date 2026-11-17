//! Prefab-Bibliothek: mitgelieferte Bauteile, Ordner mit Kategorien, Hammer-.ol/.rmf-Import, Vorschaubilder.

use crate::editor::Editor;
use crate::fgd::Fgd;
use crate::geometry::{hull_planes, Aabb};
use crate::map::{default_tex_axes, fmt_vec, Entity, Map, Side, Solid, TexAxis};
use crate::primitives::Shape;
use glam::DVec3;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Builtin(&'static str),
    File(PathBuf),
    /// Eintrag in einer Hammer-Prefab-Bibliothek (.ol)
    Library(PathBuf, usize),
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: Source,
}

impl Entry {
    /// Stabile Kennung (für MCP und Vorschau-Cache)
    pub fn id(&self) -> String {
        match &self.source {
            Source::Builtin(id) => format!("builtin:{id}"),
            Source::File(p) => format!("file:{}", p.display()),
            Source::Library(p, i) => format!("ol:{}#{i}", p.display()),
        }
    }
}

/// Drag-&-Drop-Nutzlast aus dem Prefab-Fenster
#[derive(Clone, Debug)]
pub struct PrefabDrag(pub usize);

// ------------------------------------------------------------------ Mitgelieferte Prefabs

struct Builtin {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    description: &'static str,
    build: fn(&mut Editor),
}

const WALL: &str = "DEV/DEV_MEASUREWALL01A";
const DEV: &str = "DEV/DEV_MEASUREGENERIC01B";
const DEV2: &str = "DEV/DEV_MEASUREGENERIC01";
const GRAY: &str = "DEV/GRAYGRID";

pub(crate) fn b(ed: &mut Editor, mat: &str, a: [f64; 3], c: [f64; 3]) -> u32 {
    ed.material = mat.to_string();
    ed.shape.shape = Shape::Block;
    ed.create_block(Aabb::new(DVec3::from(a), DVec3::from(c)));
    *ed.selected_solid_ids().last().unwrap()
}

pub(crate) fn tie(ed: &mut Editor, ids: &[u32], class: &str, keys: &[(&str, &str)]) -> u32 {
    ed.sel = ids.iter().map(|i| crate::editor::Obj::Solid(*i)).collect();
    ed.tie_to_entity(class);
    let id = ed.selected_entity_ids()[0];
    let e = ed.map.entity_mut(id).unwrap();
    for (k, v) in keys {
        e.set(k, *v);
    }
    id
}

pub(crate) fn ent(ed: &mut Editor, class: &str, pos: [f64; 3], keys: &[(&str, &str)]) -> u32 {
    ed.entity_class = class.to_string();
    let id = ed.create_point_entity(DVec3::from(pos));
    let e = ed.map.entity_mut(id).unwrap();
    for (k, v) in keys {
        e.set(k, *v);
    }
    id
}

fn output(ed: &mut Editor, id: u32, out: &str, value: &str) {
    ed.map.entity_mut(id).unwrap().outputs.push((out.into(), value.into()));
}

/// Wand mit Öffnung (Carve)
fn wall_with_hole(ed: &mut Editor, hole_min: [f64; 3], hole_max: [f64; 3]) {
    b(ed, WALL, [-128.0, -8.0, 0.0], [128.0, 8.0, 128.0]);
    let cutter = b(ed, DEV, hole_min, hole_max);
    ed.sel = [crate::editor::Obj::Solid(cutter)].into_iter().collect();
    ed.carve();
    ed.sel = [crate::editor::Obj::Solid(cutter)].into_iter().collect();
    ed.delete_selection();
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "schiebetuer",
        name: "Schiebetür mit Knöpfen",
        category: "Türen & Fenster",
        description: "Wand mit func_door (fährt nach oben) und je einem Knopf auf beiden Seiten.",
        build: |ed| {
            wall_with_hole(ed, [-32.0, -16.0, 0.0], [32.0, 16.0, 112.0]);
            let d = b(ed, DEV2, [-32.0, -4.0, 0.0], [32.0, 4.0, 112.0]);
            tie(ed, &[d], "func_door", &[("targetname", "tuer"), ("movedir", "-90 0 0"), ("speed", "120"), ("wait", "4"), ("lip", "8"), ("spawnflags", "0")]);
            for y in [-12.0, 8.0] {
                let k = b(ed, "DEV/REFLECTIVITY_50", [48.0, y, 48.0], [60.0, y + 4.0, 64.0]);
                let id = tie(ed, &[k], "func_button", &[("spawnflags", "1025"), ("wait", "2")]);
                output(ed, id, "OnPressed", "tuer,Open,,0,-1");
            }
        },
    },
    Builtin {
        id: "drehtuer",
        name: "Drehtür",
        category: "Türen & Fenster",
        description: "Wand mit func_door_rotating (Scharnier über Origin-Brush), öffnet per Benutzen.",
        build: |ed| {
            wall_with_hole(ed, [-32.0, -16.0, 0.0], [32.0, 16.0, 112.0]);
            let d = b(ed, DEV2, [-32.0, -2.0, 0.0], [32.0, 2.0, 112.0]);
            let o = b(ed, "TOOLS/TOOLSORIGIN", [-32.0, -2.0, 0.0], [-28.0, 2.0, 112.0]);
            tie(ed, &[d, o], "func_door_rotating", &[("distance", "90"), ("speed", "120"), ("wait", "-1"), ("spawnflags", "256")]);
        },
    },
    Builtin {
        id: "fenster",
        name: "Fenster mit Glas",
        category: "Türen & Fenster",
        description: "Wand mit Fensteröffnung und zerbrechlicher Glasscheibe (func_breakable).",
        build: |ed| {
            wall_with_hole(ed, [-48.0, -16.0, 40.0], [48.0, 16.0, 104.0]);
            let g = b(ed, "GLASS/GLASSWINDOW001A", [-48.0, -1.0, 40.0], [48.0, 1.0, 104.0]);
            tie(ed, &[g], "func_breakable", &[("material", "0"), ("health", "1")]);
        },
    },
    Builtin {
        id: "leiter",
        name: "Leiter",
        category: "Bauteile",
        description: "Sprossenleiter (func_detail) mit unsichtbarem Kletter-Brush (toolsinvisibleladder).",
        build: |ed| {
            let mut ids = vec![b(ed, "METAL/METALFLOOR001A", [-28.0, -4.0, 0.0], [-24.0, 0.0, 192.0]), b(ed, "METAL/METALFLOOR001A", [24.0, -4.0, 0.0], [28.0, 0.0, 192.0])];
            for i in 0..9 {
                let z = 12.0 + i as f64 * 20.0;
                ids.push(b(ed, "METAL/METALFLOOR001A", [-24.0, -3.0, z], [24.0, -1.0, z + 3.0]));
            }
            tie(ed, &ids, "func_detail", &[]);
            b(ed, "TOOLS/TOOLSINVISIBLELADDER", [-28.0, -12.0, 0.0], [28.0, -4.0, 192.0]);
        },
    },
    Builtin {
        id: "deckenlampe",
        name: "Deckenlampe",
        category: "Bauteile",
        description: "Lampengehäuse mit Licht darunter. Auf eine Decke ziehen.",
        build: |ed| {
            let h = b(ed, "METAL/METALFLOOR001A", [-16.0, -16.0, 40.0], [16.0, 16.0, 48.0]);
            tie(ed, &[h], "func_detail", &[]);
            ent(ed, "light", [0.0, 0.0, 24.0], &[("_light", "255 235 200 250")]);
        },
    },
    Builtin {
        id: "lichtschalter",
        name: "Lichtschalter",
        category: "Technik & Logik",
        description: "Knopf, der eine benannte Lampe ein- und ausschaltet.",
        build: |ed| {
            let k = b(ed, "DEV/REFLECTIVITY_50", [-6.0, -2.0, 48.0], [6.0, 2.0, 64.0]);
            let btn = tie(ed, &[k], "func_button", &[("spawnflags", "1025"), ("wait", "0.5")]);
            ent(ed, "light", [0.0, -96.0, 120.0], &[("targetname", "licht_1"), ("_light", "255 255 255 300")]);
            output(ed, btn, "OnPressed", "licht_1,Toggle,,0,-1");
        },
    },
    Builtin {
        id: "teleporter",
        name: "Teleporter-Paar",
        category: "Technik & Logik",
        description: "trigger_teleport mit Ziel (info_teleport_destination) 256 Einheiten daneben.",
        build: |ed| {
            b(ed, DEV2, [-40.0, -40.0, -8.0], [40.0, 40.0, 0.0]);
            b(ed, DEV2, [216.0, -40.0, -8.0], [296.0, 40.0, 0.0]);
            let t = b(ed, "TOOLS/TOOLSTRIGGER", [-32.0, -32.0, 0.0], [32.0, 32.0, 96.0]);
            tie(ed, &[t], "trigger_teleport", &[("target", "tp_ziel"), ("spawnflags", "1")]);
            ent(ed, "info_teleport_destination", [256.0, 0.0, 0.0], &[("targetname", "tp_ziel"), ("angles", "0 0 0")]);
        },
    },
    Builtin {
        id: "schadenszone",
        name: "Schadenszone",
        category: "Technik & Logik",
        description: "trigger_hurt, der Spielern pro Sekunde Schaden zufügt.",
        build: |ed| {
            let t = b(ed, "TOOLS/TOOLSTRIGGER", [-64.0, -64.0, 0.0], [64.0, 64.0, 64.0]);
            tie(ed, &[t], "trigger_hurt", &[("damage", "10"), ("spawnflags", "1")]);
        },
    },
    Builtin {
        id: "spawnraum",
        name: "Spawn-Raum",
        category: "Räume",
        description: "Abgedichteter Raum (512 × 512 × 256) mit info_player_start, Licht und env_cubemap. Sofort kompilierbar.",
        build: |ed| {
            b(ed, DEV, [-256.0, -256.0, -16.0], [256.0, 256.0, 256.0]);
            ed.hollow(16.0);
            ent(ed, "info_player_start", [0.0, -128.0, 0.0], &[("angles", "0 90 0")]);
            ent(ed, "light", [0.0, 0.0, 200.0], &[("_light", "255 240 220 300")]);
            ent(ed, "env_cubemap", [0.0, 0.0, 64.0], &[]);
        },
    },
    Builtin {
        id: "aussenbereich",
        name: "Außenbereich mit Himmel",
        category: "Räume",
        description: "2048 × 2048 × 1024 große Skybox-Hülle mit Boden, light_environment und Spawn.",
        build: |ed| {
            b(ed, GRAY, [-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 1024.0]);
            ed.hollow(16.0);
            let floor = ed.map.world.solids.iter().min_by(|a, c| a.bounds.min.z.partial_cmp(&c.bounds.min.z).unwrap()).map(|s| s.id).unwrap();
            for s in &mut ed.map.world.solids {
                if s.id != floor {
                    s.set_material_all("TOOLS/TOOLSSKYBOX");
                }
            }
            ent(ed, "light_environment", [0.0, 0.0, 512.0], &[("angles", "0 45 0"), ("pitch", "-50")]);
            ent(ed, "info_player_start", [0.0, 0.0, 0.0], &[("angles", "0 0 0")]);
        },
    },
    Builtin {
        id: "wasserbecken",
        name: "Wasserbecken",
        category: "Räume",
        description: "Offenes Becken mit Wasser-Brush (Wasseroberfläche oben, sonst nodraw).",
        build: |ed| {
            b(ed, "CONCRETE/CONCRETEFLOOR001A", [-256.0, -256.0, -128.0], [256.0, 256.0, 16.0]);
            ed.hollow(16.0);
            let top = ed.map.world.solids.iter().max_by(|a, c| a.bounds.max.z.partial_cmp(&c.bounds.max.z).unwrap().then(a.bounds.min.z.partial_cmp(&c.bounds.min.z).unwrap())).map(|s| s.id).unwrap();
            ed.map.world.solids.retain(|s| s.id != top);
            let w = b(ed, "TOOLS/TOOLSNODRAW", [-240.0, -240.0, -112.0], [240.0, 240.0, -16.0]);
            if let Some(s) = ed.map.solid_mut(w) {
                for side in &mut s.sides {
                    if side.plane().is_some_and(|p| p.normal.z > 0.9) {
                        side.material = "NATURE/WATER_CANALS_WATER2".into();
                    }
                }
            }
        },
    },
    Builtin {
        id: "treppenhaus",
        name: "Treppe mit Podest",
        category: "Bauteile",
        description: "8 Stufen (16 hoch) auf 128 Einheiten Höhe mit Podest und Geländer.",
        build: |ed| {
            ed.material = GRAY.into();
            ed.shape.shape = Shape::Stairs;
            ed.shape.sides = 8;
            ed.shape.start_angle = 0.0;
            ed.create_block(Aabb::new(DVec3::new(-192.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 128.0)));
            b(ed, GRAY, [64.0, -64.0, 0.0], [192.0, 64.0, 128.0]);
            ed.material = "METAL/METALFLOOR001A".into();
            ed.shape.shape = Shape::Fence;
            ed.shape.sides = 3;
            ed.shape.wall = 4.0;
            ed.shape.cross_sides = 1;
            ed.create_block(Aabb::new(DVec3::new(64.0, 60.0, 128.0), DVec3::new(192.0, 64.0, 168.0)));
            ed.shape = Default::default();
        },
    },
    Builtin {
        id: "saeule",
        name: "Säule",
        category: "Bauteile",
        description: "Runde Säule mit Sockel und Kapitell (func_detail).",
        build: |ed| {
            let s1 = b(ed, "CONCRETE/CONCRETEFLOOR001A", [-40.0, -40.0, 0.0], [40.0, 40.0, 16.0]);
            ed.material = "CONCRETE/CONCRETEFLOOR001A".into();
            ed.shape.shape = Shape::Cylinder;
            ed.shape.sides = 12;
            ed.create_block(Aabb::new(DVec3::new(-28.0, -28.0, 16.0), DVec3::new(28.0, 28.0, 240.0)));
            let c = ed.selected_solid_ids()[0];
            ed.shape.shape = Shape::Block;
            let s2 = b(ed, "CONCRETE/CONCRETEFLOOR001A", [-40.0, -40.0, 240.0], [40.0, 40.0, 256.0]);
            tie(ed, &[s1, c, s2], "func_detail", &[]);
        },
    },
    Builtin {
        id: "physikkiste",
        name: "Physik-Kiste",
        category: "Bauteile",
        description: "prop_physics mit Holzkiste.",
        build: |ed| {
            ed.next_model = "models/props_junk/wood_crate001a.mdl".into();
            ent(ed, "prop_physics", [0.0, 0.0, 20.0], &[]);
        },
    },
];

pub fn builtin_entries() -> Vec<Entry> {
    BUILTINS
        .iter()
        .map(|b| Entry { name: b.name.into(), category: format!("Bibliothek / {}", b.category), description: b.description.into(), source: Source::Builtin(b.id) })
        .collect()
}

pub fn build_builtin(id: &str) -> Option<Map> {
    let def = BUILTINS.iter().find(|b| b.id == id)?;
    let mut ed = Editor::new(Fgd::builtin());
    (def.build)(&mut ed);
    Some(ed.map)
}

// ------------------------------------------------------------------ Ordner & Laden

/// Alle Prefabs: mitgelieferte + Ordner (Unterordner = Kategorie) + .ol-Bibliotheken
pub fn scan(dir: &Path) -> Vec<Entry> {
    let mut out = builtin_entries();
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    files.sort();
    for p in files {
        let rel_dir = p.parent().and_then(|d| d.strip_prefix(dir).ok()).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let category = if rel_dir.is_empty() { "Eigene".to_string() } else { format!("Eigene / {rel_dir}") };
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match ext.as_str() {
            "vmf" => out.push(Entry { name: stem, category, description: String::new(), source: Source::File(p) }),
            "rmf" => out.push(Entry { name: stem, category, description: "Hammer/Worldcraft RMF".into(), source: Source::File(p) }),
            "ol" => {
                if let Ok(lib) = read_ol(&p) {
                    for (i, e) in lib.iter().enumerate() {
                        out.push(Entry { name: e.name.clone(), category: format!("Hammer / {stem}"), description: e.notes.clone(), source: Source::Library(p.clone(), i) });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(base, &p, out);
        } else if p.extension().is_some_and(|x| ["vmf", "rmf", "ol"].iter().any(|y| x.eq_ignore_ascii_case(y))) {
            out.push(p);
        }
    }
}

/// Unterordner (Kategorien) für „Speichern unter“
pub fn categories(dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    fn walk(base: &Path, d: &Path, v: &mut Vec<String>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    if let Ok(r) = e.path().strip_prefix(base) {
                        v.push(r.to_string_lossy().replace('\\', "/"));
                    }
                    walk(base, &e.path(), v);
                }
            }
        }
    }
    walk(dir, dir, &mut v);
    v.sort();
    v
}

pub fn load(e: &Entry) -> Result<Map, String> {
    match &e.source {
        Source::Builtin(id) => build_builtin(id).ok_or_else(|| format!("Unbekanntes Prefab {id}")),
        Source::File(p) => {
            let b = std::fs::read(p).map_err(|err| format!("{}: {err}", p.display()))?;
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rmf")) {
                parse_rmf(&b)
            } else {
                Map::from_vmf(&String::from_utf8_lossy(&b))
            }
        }
        Source::Library(p, i) => {
            let lib = read_ol(p)?;
            let entry = lib.get(*i).ok_or("Eintrag nicht gefunden")?;
            parse_rmf(&entry.data)
        }
    }
}

/// Mitgelieferte Prefabs als VMF in einen Ordner schreiben (zum Anpassen)
pub fn export_builtins(dir: &Path) -> Result<usize, String> {
    let mut n = 0;
    for b in BUILTINS {
        let d = dir.join("Bibliothek").join(b.category.replace('&', "und"));
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        let m = build_builtin(b.id).unwrap();
        std::fs::write(d.join(format!("{}.vmf", b.id)), m.to_vmf()).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

// ------------------------------------------------------------------ Hammer .ol / .rmf

pub struct OlEntry {
    pub name: String,
    pub notes: String,
    pub data: Vec<u8>,
}

const OL_SIG: &[u8] = b"Worldcraft Prefab Library\r\n\x1a";

fn cstr_fixed(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).to_string()
}

/// Hammer-/Worldcraft-Prefab-Bibliothek lesen
pub fn read_ol(path: &Path) -> Result<Vec<OlEntry>, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_ol(&b)
}

pub fn parse_ol(b: &[u8]) -> Result<Vec<OlEntry>, String> {
    if !b.starts_with(OL_SIG) {
        return Err("Keine Hammer-Prefab-Bibliothek (.ol)".into());
    }
    let mut r = Rd { b, p: OL_SIG.len() };
    let _version = r.f32()?;
    let dir = r.u32()? as usize;
    let n = r.u32()? as usize;
    let mut out = Vec::new();
    for i in 0..n.min(10_000) {
        let mut e = Rd { b, p: dir + i * 544 };
        let off = e.u32()? as usize;
        let size = e.u32()? as usize;
        let name = cstr_fixed(e.bytes(31)?);
        let notes = cstr_fixed(e.bytes(501)?);
        let data = b.get(off..off + size).ok_or("Prefab-Daten außerhalb der Datei")?.to_vec();
        out.push(OlEntry { name, notes, data });
    }
    Ok(out)
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or("Datei zu kurz")?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn vec3(&mut self) -> Result<DVec3, String> {
        Ok(DVec3::new(self.f32()? as f64, self.f32()? as f64, self.f32()? as f64))
    }
    /// Zeichenkette mit Längen-Byte (inkl. Null)
    fn nstr(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        Ok(cstr_fixed(self.bytes(n)?))
    }
    fn count(&mut self) -> Result<usize, String> {
        let n = self.i32()?;
        if !(0..=1_000_000).contains(&n) {
            return Err("Ungültige Anzahl in RMF".into());
        }
        Ok(n as usize)
    }
}

struct RmfFace {
    tex: String,
    u: TexAxis,
    v: TexAxis,
    verts: Vec<DVec3>,
}

/// Hammer-/Worldcraft-RMF (Version 2.2, ältere Versionen mit 40-Zeichen-Texturnamen) einlesen
pub fn parse_rmf(b: &[u8]) -> Result<Map, String> {
    let mut r = Rd { b, p: 0 };
    let version = r.f32()?;
    if r.bytes(3)? != b"RMF" {
        return Err("Keine RMF-Daten".into());
    }
    let tex_len = if version >= 2.0 { 256 } else { 40 };
    let nvis = r.count()?;
    r.bytes(nvis * 144)?;
    let mut map = Map::new();
    let kind = r.nstr()?;
    if kind != "CMapWorld" {
        return Err(format!("CMapWorld erwartet, gefunden {kind}"));
    }
    r.i32()?;
    r.bytes(3)?;
    let n = r.count()?;
    for _ in 0..n {
        read_object(&mut r, &mut map, None, tex_len, version)?;
    }
    Ok(map)
}

fn read_keys(r: &mut Rd) -> Result<(String, Vec<(String, String)>), String> {
    let class = r.nstr()?;
    r.bytes(4)?;
    r.i32()?; // Flags
    let nk = r.count()?;
    let mut keys = Vec::new();
    for _ in 0..nk {
        let k = r.nstr()?;
        let v = r.nstr()?;
        keys.push((k, v));
    }
    Ok((class, keys))
}

fn read_object(r: &mut Rd, map: &mut Map, owner: Option<&mut Entity>, tex_len: usize, version: f32) -> Result<(), String> {
    let kind = r.nstr()?;
    r.i32()?; // Visgroup
    r.bytes(3)?; // Farbe
    match kind.as_str() {
        "CMapSolid" => {
            r.i32()?; // Kinder (immer 0)
            let nf = r.count()?;
            let mut faces = Vec::new();
            for _ in 0..nf {
                let tex = cstr_fixed(r.bytes(tex_len)?);
                if version >= 2.0 {
                    r.f32()?;
                }
                let ua = r.vec3()?;
                let us = r.f32()? as f64;
                let va = r.vec3()?;
                let vs = r.f32()? as f64;
                r.f32()?; // Rotation
                let su = r.f32()? as f64;
                let sv = r.f32()? as f64;
                r.bytes(16)?;
                let nv = r.count()?;
                let mut verts = Vec::new();
                for _ in 0..nv {
                    verts.push(r.vec3()?);
                }
                r.bytes(36)?; // Ebenenpunkte (wir nutzen die Eckpunkte)
                faces.push(RmfFace { tex, u: TexAxis { axis: ua, shift: us, scale: if su == 0.0 { 0.25 } else { su } }, v: TexAxis { axis: va, shift: vs, scale: if sv == 0.0 { 0.25 } else { sv } }, verts });
            }
            if let Some(s) = solid_from_faces(map, &faces) {
                match owner {
                    Some(e) => e.solids.push(s.into()),
                    None => map.world.solids.push(s.into()),
                }
            }
        }
        "CMapEntity" => {
            let ns = r.count()?;
            let mut e = Entity::new(0, "");
            e.props.clear();
            for _ in 0..ns {
                read_object(r, map, Some(&mut e), tex_len, version)?;
            }
            let (class, keys) = read_keys(r)?;
            r.bytes(14)?;
            let origin = r.vec3()?;
            r.i32()?;
            e.props.push(("classname".into(), class));
            e.props.extend(keys);
            if e.solids.is_empty() {
                e.set("origin", fmt_vec(origin));
            }
            e.id = map.alloc_id();
            map.entities.push(e.into());
        }
        "CMapGroup" => {
            let n = r.count()?;
            let mut owner = owner;
            for _ in 0..n {
                read_object(r, map, owner.as_deref_mut(), tex_len, version)?;
            }
        }
        other => return Err(format!("Unbekannter RMF-Objekttyp {other}")),
    }
    Ok(())
}

/// Konvexen Brush aus den Eckpunkten bauen, Texturen anhand der Flächennormale zuordnen
fn solid_from_faces(map: &mut Map, faces: &[RmfFace]) -> Option<Solid> {
    let pts: Vec<DVec3> = faces.iter().flat_map(|f| f.verts.iter().copied()).collect();
    let center = pts.iter().fold(DVec3::ZERO, |a, b| a + *b) / pts.len().max(1) as f64;
    let planes = hull_planes(&pts);
    if planes.len() < 4 {
        return None;
    }
    let face_normals: Vec<Option<DVec3>> = faces
        .iter()
        .map(|f| {
            if f.verts.len() < 3 {
                return None;
            }
            let c = f.verts.iter().fold(DVec3::ZERO, |a, b| a + *b) / f.verts.len() as f64;
            let n = (f.verts[1] - f.verts[0]).cross(f.verts[2] - f.verts[0]).normalize_or_zero();
            Some(if n.dot(c - center) < 0.0 { -n } else { n })
        })
        .collect();
    let mut sides: Vec<Side> = Vec::new();
    for pl in planes {
        let best = faces.iter().zip(&face_normals).filter_map(|(f, n)| n.map(|n| (f, n.dot(pl.normal)))).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut side = map.side_from_plane(pl, best.map(|b| b.0.tex.as_str()).unwrap_or("TOOLS/TOOLSNODRAW"), 0.25);
        match best {
            Some((f, d)) if d > 0.99 => {
                side.uaxis = f.u.clone();
                side.vaxis = f.v.clone();
            }
            _ => {
                let (u, v) = default_tex_axes(pl.normal, 0.25);
                side.uaxis = u;
                side.vaxis = v;
            }
        }
        sides.push(side);
    }
    let id = map.alloc_id();
    let mut s = Solid::new(id, sides);
    s.cleanup();
    s.is_valid().then_some(s)
}

// ------------------------------------------------------------------ Vorschaubilder (Software-Renderer)

/// Rendert eine isometrische Vorschau (RGBA) einer Prefab-Map
pub fn render_thumb(m: &Map, size: usize) -> Vec<u8> {
    // Dreiecke sammeln: (Punkte, Farbe)
    let mut tris: Vec<([DVec3; 3], [f32; 4], DVec3)> = Vec::new();
    for (_, s) in m.all_solids() {
        let has_disp = s.has_disp();
        for (i, poly) in s.polys.iter().enumerate() {
            if poly.len() < 3 || (has_disp && s.sides[i].disp.is_none()) {
                continue;
            }
            let c = crate::view3d::material_color(&s.sides[i].material);
            let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
            if let Some(Some(g)) = s.disps.get(i) {
                for t in g.triangles() {
                    tris.push(([g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]], c, n));
                }
            } else {
                for k in 1..poly.len() - 1 {
                    tris.push(([poly[0], poly[k], poly[k + 1]], c, n));
                }
            }
        }
    }
    for e in &m.entities {
        if e.is_brush_entity() {
            continue;
        }
        let Some(o) = e.origin() else { continue };
        let c = if e.classname().starts_with("light") { [1.0, 0.9, 0.3, 1.0] } else if e.classname().starts_with("info_player") { [0.2, 0.9, 0.2, 1.0] } else { [0.9, 0.3, 0.9, 1.0] };
        let half = if e.classname().starts_with("info_player") { DVec3::new(16.0, 16.0, 36.0) } else { DVec3::splat(8.0) };
        let center = if e.classname().starts_with("info_player") { o + DVec3::Z * 36.0 } else { o };
        let b = Aabb::new(center - half, center + half);
        let k = b.corners();
        for (q, n) in [([4, 5, 6, 7], DVec3::Z), ([1, 2, 6, 5], DVec3::X), ([0, 4, 7, 3], -DVec3::X), ([3, 7, 6, 2], DVec3::Y), ([0, 1, 5, 4], -DVec3::Y)] {
            tris.push(([k[q[0]], k[q[1]], k[q[2]]], c, n));
            tris.push(([k[q[0]], k[q[2]], k[q[3]]], c, n));
        }
    }
    render_tris(tris, size, [38, 40, 46])
}

/// Dreiecke isometrisch und schattiert rastern (RGBA)
pub fn render_tris(tris: Vec<([DVec3; 3], [f32; 4], DVec3)>, size: usize, bg: [u8; 3]) -> Vec<u8> {
    let mut img = vec![0u8; size * size * 4];
    for px in img.chunks_mut(4) {
        px.copy_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    let mut zbuf = vec![f64::MIN; size * size];
    if tris.is_empty() {
        return img;
    }
    // Isometrische Projektion: Blick von Süd-West, leicht von oben
    let (yaw, pitch) = (45f64.to_radians(), 30f64.to_radians());
    let right = DVec3::new(yaw.cos(), -yaw.sin(), 0.0);
    let fwd = DVec3::new(yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), -pitch.sin());
    let up = right.cross(fwd);
    let proj = |p: DVec3| (p.dot(right), p.dot(up), -p.dot(fwd));
    let (mut minx, mut maxx, mut miny, mut maxy) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (t, _, _) in &tris {
        for p in t {
            let (x, y, _) = proj(*p);
            minx = minx.min(x);
            maxx = maxx.max(x);
            miny = miny.min(y);
            maxy = maxy.max(y);
        }
    }
    let scale = (size as f64 * 0.88) / (maxx - minx).max(maxy - miny).max(1.0);
    let ox = size as f64 / 2.0 - (minx + maxx) / 2.0 * scale;
    let oy = size as f64 / 2.0 + (miny + maxy) / 2.0 * scale;
    let light = DVec3::new(-0.4, -0.6, 0.8).normalize();
    for (t, c, n) in tris {
        let shade = 0.55 + 0.45 * n.dot(light).abs().max(n.dot(light));
        let alpha = c[3] as f64;
        let p: Vec<(f64, f64, f64)> = t.iter().map(|q| {
            let (x, y, z) = proj(*q);
            (ox + x * scale, oy - y * scale, z)
        }).collect();
        let (x0, x1) = (p.iter().map(|q| q.0).fold(f64::MAX, f64::min).max(0.0) as usize, (p.iter().map(|q| q.0).fold(f64::MIN, f64::max).ceil() as usize).min(size - 1));
        let (y0, y1) = (p.iter().map(|q| q.1).fold(f64::MAX, f64::min).max(0.0) as usize, (p.iter().map(|q| q.1).fold(f64::MIN, f64::max).ceil() as usize).min(size - 1));
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-9 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((p[1].0 - fx) * (p[2].1 - fy) - (p[2].0 - fx) * (p[1].1 - fy)) / area;
                let w1 = ((p[2].0 - fx) * (p[0].1 - fy) - (p[0].0 - fx) * (p[2].1 - fy)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                let i = y * size + x;
                if alpha < 0.99 {
                    let px = &mut img[i * 4..i * 4 + 4];
                    for k in 0..3 {
                        px[k] = (px[k] as f64 * (1.0 - alpha * 0.6) + c[k] as f64 * shade * 255.0 * alpha * 0.6) as u8;
                    }
                    continue;
                }
                if z <= zbuf[i] {
                    continue;
                }
                zbuf[i] = z;
                img[i * 4..i * 4 + 3].copy_from_slice(&[(c[0] as f64 * shade * 255.0) as u8, (c[1] as f64 * shade * 255.0) as u8, (c[2] as f64 * shade * 255.0) as u8]);
            }
        }
    }
    img
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn all_builtins_build() {
        for e in builtin_entries() {
            let m = load(&e).unwrap();
            assert!(!m.world.solids.is_empty() || !m.entities.is_empty(), "{}", e.name);
            assert!(m.all_solids().all(|(_, s)| s.is_valid()), "{}", e.name);
            let back = Map::from_vmf(&m.to_vmf()).unwrap();
            assert_eq!(back.all_solids().count(), m.all_solids().count(), "{}", e.name);
            let img = render_thumb(&m, 64);
            assert!(img.chunks(4).any(|p| p != [38, 40, 46, 255]), "Vorschau leer: {}", e.name);
        }
        let door = load(&builtin_entries().into_iter().find(|e| e.name.starts_with("Schiebetür")).unwrap()).unwrap();
        assert!(door.entities.iter().any(|e| e.classname() == "func_door"));
        assert_eq!(door.entities.iter().filter(|e| e.classname() == "func_button").count(), 2);
    }

    /// Minimale RMF 2.2 mit einem Würfel und einer Entity
    pub fn make_rmf() -> Vec<u8> {
        let mut b = Vec::new();
        let nstr = |b: &mut Vec<u8>, s: &str| {
            b.push(s.len() as u8 + 1);
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        };
        let f = |b: &mut Vec<u8>, v: f32| b.extend_from_slice(&v.to_le_bytes());
        let i = |b: &mut Vec<u8>, v: i32| b.extend_from_slice(&v.to_le_bytes());
        f(&mut b, 2.2);
        b.extend_from_slice(b"RMF");
        i(&mut b, 0);
        nstr(&mut b, "CMapWorld");
        i(&mut b, 0);
        b.extend_from_slice(&[0, 0, 0]);
        i(&mut b, 2);
        // Würfel 0..64
        nstr(&mut b, "CMapSolid");
        i(&mut b, 0);
        b.extend_from_slice(&[0, 0, 0]);
        i(&mut b, 0);
        let c = Aabb::new(DVec3::ZERO, DVec3::splat(64.0)).corners();
        let quads = [[4, 5, 6, 7], [0, 3, 2, 1], [1, 2, 6, 5], [0, 4, 7, 3], [3, 7, 6, 2], [0, 1, 5, 4]];
        i(&mut b, 6);
        for q in quads {
            let mut name = b"CRATE01".to_vec();
            name.resize(256, 0);
            b.extend_from_slice(&name);
            f(&mut b, 0.0);
            for v in [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0] {
                f(&mut b, v);
            }
            b.extend_from_slice(&[0u8; 16]);
            i(&mut b, 4);
            for k in q {
                for a in 0..3 {
                    f(&mut b, c[k][a] as f32);
                }
            }
            b.extend_from_slice(&[0u8; 36]);
        }
        // Punkt-Entity
        nstr(&mut b, "CMapEntity");
        i(&mut b, 0);
        b.extend_from_slice(&[0, 0, 0]);
        i(&mut b, 0);
        nstr(&mut b, "light");
        b.extend_from_slice(&[0u8; 4]);
        i(&mut b, 0);
        i(&mut b, 1);
        nstr(&mut b, "_light");
        nstr(&mut b, "255 255 255 200");
        b.extend_from_slice(&[0u8; 14]);
        for v in [32.0, 32.0, 100.0] {
            f(&mut b, v);
        }
        i(&mut b, 0);
        b
    }

    pub fn make_ol(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut b = OL_SIG.to_vec();
        b.extend_from_slice(&0.1f32.to_le_bytes());
        let header_end = b.len() + 8 + 501;
        let mut data = Vec::new();
        let mut dir = Vec::new();
        for (name, rmf) in entries {
            let off = header_end + data.len();
            dir.extend_from_slice(&(off as u32).to_le_bytes());
            dir.extend_from_slice(&(rmf.len() as u32).to_le_bytes());
            let mut n = name.as_bytes().to_vec();
            n.resize(31, 0);
            dir.extend_from_slice(&n);
            dir.extend_from_slice(&[0u8; 501]);
            dir.extend_from_slice(&0i32.to_le_bytes());
            data.extend_from_slice(rmf);
        }
        let dir_off = header_end + data.len();
        b.extend_from_slice(&(dir_off as u32).to_le_bytes());
        b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0u8; 501]);
        b.extend_from_slice(&data);
        b.extend_from_slice(&dir);
        b
    }

    #[test]
    fn rmf_and_ol() {
        let m = parse_rmf(&make_rmf()).unwrap();
        assert_eq!(m.world.solids.len(), 1);
        let s = &m.world.solids[0];
        assert_eq!(s.bounds, Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        assert_eq!(s.sides.len(), 6);
        assert!(s.sides.iter().all(|x| x.material == "CRATE01"));
        assert_eq!(m.entities[0].classname(), "light");
        assert_eq!(m.entities[0].get("origin"), Some("32 32 100"));
        let ol = make_ol(&[("Kiste", make_rmf()), ("Kiste 2", make_rmf())]);
        let lib = parse_ol(&ol).unwrap();
        assert_eq!(lib.len(), 2);
        assert_eq!(lib[1].name, "Kiste 2");
        assert_eq!(parse_rmf(&lib[1].data).unwrap().world.solids.len(), 1);
    }

    #[test]
    fn scan_folder() {
        let dir = std::env::temp_dir().join(format!("amboss_pf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Möbel")).unwrap();
        let m = build_builtin("saeule").unwrap();
        std::fs::write(dir.join("Möbel/tisch.vmf"), m.to_vmf()).unwrap();
        std::fs::write(dir.join("alt.ol"), make_ol(&[("Kiste", make_rmf())])).unwrap();
        let list = scan(&dir);
        assert!(list.iter().any(|e| e.name == "tisch" && e.category == "Eigene / Möbel"));
        assert!(list.iter().any(|e| e.name == "Kiste" && e.category == "Hammer / alt"));
        assert_eq!(categories(&dir), vec!["Möbel".to_string()]);
        for e in &list {
            load(e).unwrap();
        }
        assert_eq!(export_builtins(&dir).unwrap(), BUILTINS.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
