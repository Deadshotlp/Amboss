//! GoldSrc-Decompiler: Half-Life-1-BSP (Version 30, Quake 1 Version 29 ohne Texturen) → VMF,
//! Texturen aus der BSP und aus WAD-Dateien → VTF/VMT.
//!
//! GoldSrc-BSPs enthalten keine Brushes mehr, nur den BSP-Baum. Jeder solide Bereich des Baums
//! ist konvex und wird zu einem Brush; die Seiten bekommen Material und Ausrichtung der sichtbaren
//! Fläche, die auf ihnen liegt, alle anderen Seiten nodraw. Das Ergebnis ist dicht und kompiliert,
//! besteht aber aus mehr (kleineren) Brushes als das Original.

use crate::decompile::{parse_entities, Report};
use crate::geometry::{clip_polygon, polygons_from_planes, Aabb, Plane};
use crate::map::{Entity, Map, Solid, TexAxis};
use crate::vtf_write::{Rgba, VtfOpts};
use glam::DVec3;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const L_ENTITIES: usize = 0;
const L_PLANES: usize = 1;
const L_TEXTURES: usize = 2;
const L_VERTICES: usize = 3;
const L_NODES: usize = 5;
const L_TEXINFO: usize = 6;
const L_FACES: usize = 7;
const L_LEAVES: usize = 10;
const L_EDGES: usize = 12;
const L_SURFEDGES: usize = 13;
const L_MODELS: usize = 14;

const C_SOLID: i32 = -2;
const C_WATER: i32 = -3;
const C_LAVA: i32 = -5;
const C_SKY: i32 = -6;
const C_TRANSLUCENT: i32 = -15;

/// Material für Wasserflächen (gibt es in jedem Source-Spiel mit HL2-Inhalten, also auch in GMod)
pub const WATER_MATERIAL: &str = "NATURE/WATER_CANALS_WATER2";

fn f32a(b: &[u8], o: usize) -> f64 {
    b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()) as f64).unwrap_or(0.0)
}
fn i32a(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn u32a(b: &[u8], o: usize) -> u32 {
    i32a(b, o) as u32
}
fn u16a(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|x| u16::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn i16a(b: &[u8], o: usize) -> i16 {
    u16a(b, o) as i16
}
fn vec3(b: &[u8], o: usize) -> DVec3 {
    DVec3::new(f32a(b, o), f32a(b, o + 4), f32a(b, o + 8))
}
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

/// Ist das eine GoldSrc-/Quake-BSP? (Version 30 bzw. 29)
pub fn version(b: &[u8]) -> Option<u32> {
    let v = u32a(b, 0);
    (b.len() >= 4 + 15 * 8 && (v == 29 || v == 30)).then_some(v)
}

// ------------------------------------------------------------------ Texturen

/// WAD-Dateien (WAD2/WAD3) mit Verzeichnis: Name (klein) → (Datei, Offset, Länge)
#[derive(Default)]
pub struct Wads {
    files: Vec<(String, Vec<u8>)>,
    index: HashMap<String, (usize, usize, usize)>,
}

impl Wads {
    /// WADs laden; bei doppelten Namen gewinnt die zuerst genannte Datei
    pub fn load(paths: &[PathBuf]) -> Wads {
        let mut w = Wads::default();
        for p in paths {
            if let Ok(b) = std::fs::read(p) {
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                w.add(&stem, b);
            }
        }
        w
    }

    pub fn add(&mut self, stem: &str, b: Vec<u8>) {
        if b.get(0..4) != Some(b"WAD3") && b.get(0..4) != Some(b"WAD2") {
            return;
        }
        let fi = self.files.len();
        let (n, dir) = (i32a(&b, 4).max(0) as usize, i32a(&b, 8).max(0) as usize);
        for k in 0..n.min(65536) {
            let o = dir + k * 32;
            if o + 32 > b.len() {
                break;
            }
            let (pos, size, typ, comp) = (i32a(&b, o).max(0) as usize, i32a(&b, o + 4).max(0) as usize, b[o + 12], b[o + 13]);
            if (typ == 0x43 || typ == 0x44) && comp == 0 && pos + size <= b.len() {
                let name = cstr(&b[o + 16..o + 32]).to_ascii_lowercase();
                self.index.entry(name).or_insert((fi, pos, size));
            }
        }
        self.files.push((stem.to_string(), b));
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn names(&self) -> Vec<String> {
        self.files.iter().map(|(s, _)| format!("{s}.wad")).collect()
    }

    fn find(&self, name: &str) -> Option<(&str, &[u8])> {
        let &(f, o, l) = self.index.get(&name.to_ascii_lowercase())?;
        Some((&self.files[f].0, &self.files[f].1[o..o + l]))
    }
}

/// Wo nach WAD-Dateien gesucht wird: Ordner der BSP und darüber, eigene Ordner und – aus dem
/// Spielordner abgeleitet – die Half-Life-Installation von Steam
pub fn wad_dirs(bsp: &Path, game_dir: &str, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = extra.to_vec();
    if let Some(d) = bsp.parent() {
        dirs.push(d.to_path_buf());
        if let Some(up) = d.parent() {
            dirs.push(up.to_path_buf());
            dirs.push(up.join("valve"));
            if let Some(up2) = up.parent() {
                dirs.push(up2.join("valve"));
            }
        }
    }
    let gd = Path::new(game_dir);
    if let Some(common) = gd.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("common"))) {
        for m in ["valve", "valve_hd", "cstrike", "gearbox", "bshift", "czero", "tfc", "dod"] {
            dirs.push(common.join("Half-Life").join(m));
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for d in dirs {
        if d.is_dir() && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// Alle WADs in den Ordnern; die im Schlüssel "wad" der Map genannten zuerst
pub fn find_wads(dirs: &[PathBuf], wad_key: &str) -> Vec<PathBuf> {
    let wanted: Vec<String> = wad_key
        .split(';')
        .filter_map(|s| s.trim().replace('\\', "/").rsplit('/').next().map(|n| n.to_ascii_lowercase()))
        .filter(|n| n.ends_with(".wad"))
        .collect();
    let mut all: Vec<PathBuf> = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wad"))).collect();
            v.sort();
            all.extend(v);
        }
    }
    let lname = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut out: Vec<PathBuf> = Vec::new();
    for w in &wanted {
        if let Some(p) = all.iter().find(|p| &lname(p) == w) {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
    }
    for p in all {
        if !out.contains(&p) && !out.iter().any(|q| lname(q) == lname(&p)) {
            out.push(p);
        }
    }
    out
}

/// Miptex (aus BSP oder WAD) → Name und RGBA-Bild (Palette am Ende, `{`-Texturen: Index 255 durchsichtig)
pub fn miptex_rgba(m: &[u8]) -> Option<(String, Rgba)> {
    if m.len() < 40 {
        return None;
    }
    let name = cstr(&m[0..16]);
    let (w, h) = (u32a(m, 16), u32a(m, 20));
    let ofs0 = u32a(m, 24) as usize;
    let ofs3 = u32a(m, 36) as usize;
    if ofs0 == 0 || w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let n = (w * h) as usize;
    let pal_ofs = ofs3 + ((w / 8) * (h / 8)) as usize + 2;
    let idx = m.get(ofs0..ofs0 + n)?;
    let pal = m.get(pal_ofs..pal_ofs + 768)?;
    let alpha = name.starts_with('{');
    let mut px = Vec::with_capacity(n * 4);
    for &i in idx {
        let c = &pal[i as usize * 3..i as usize * 3 + 3];
        if alpha && i == 255 {
            px.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Some((name, Rgba { w, h, px }))
}

/// Werkzeug-Textur von GoldSrc → Source-Werkzeugmaterial
pub fn tool_material(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "sky" | "env_sky" => "TOOLS/TOOLSSKYBOX",
        "aaatrigger" | "trigger" => "TOOLS/TOOLSTRIGGER",
        "clip" | "clipbevel" => "TOOLS/TOOLSCLIP",
        "origin" => "TOOLS/TOOLSORIGIN",
        "null" | "bevel" | "nullnoclip" => "TOOLS/TOOLSNODRAW",
        "hint" | "solidhint" => "TOOLS/TOOLSHINT",
        "skip" => "TOOLS/TOOLSSKIP",
        _ => return None,
    })
}

/// Dateiname aus einem GoldSrc-Texturnamen (Präfixe bleiben unterscheidbar)
fn clean_name(name: &str) -> String {
    let mut s = String::new();
    for (i, c) in name.to_ascii_lowercase().chars().enumerate() {
        match c {
            'a'..='z' | '0'..='9' | '_' => s.push(c),
            '{' if i == 0 => s.push_str("a_"),
            '!' if i == 0 => s.push_str("w_"),
            '+' if i == 0 => s.push_str("p_"),
            '~' if i == 0 => s.push_str("t_"),
            '-' if i == 0 => s.push_str("r_"),
            _ => s.push('_'),
        }
    }
    if s.is_empty() {
        "unbenannt".into()
    } else {
        s
    }
}

fn surfaceprop(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |k: &[&str]| k.iter().any(|x| n.contains(x));
    if has(&["glass", "gls", "window", "wndw"]) {
        "glass"
    } else if has(&["metal", "met", "grate", "pipe", "steel", "vent"]) {
        "metal"
    } else if has(&["wood", "wd", "crate", "plank"]) {
        "wood"
    } else if has(&["grass"]) {
        "grass"
    } else if has(&["dirt", "sand", "mud"]) {
        "dirt"
    } else if has(&["tile", "tle"]) {
        "tile"
    } else if has(&["brick", "brk"]) {
        "brick"
    } else {
        "concrete"
    }
}

/// Eine Textur der Map nach der Umwandlung
#[derive(Clone, Debug)]
pub struct Tex {
    /// Source-Materialname
    pub material: String,
    /// Originalgröße (GoldSrc) und Größe der VTF – die Achsen werden darauf umgerechnet
    pub w: f64,
    pub h: f64,
    pub vw: f64,
    pub vh: f64,
}

fn convert_textures(lump: &[u8], wads: &Wads, map_name: &str, with_palette: bool, files: &mut Vec<(String, Vec<u8>)>, r: &mut Report) -> Vec<Tex> {
    let n = i32a(lump, 0).max(0) as usize;
    let mut out = Vec::with_capacity(n);
    let mut missing = Vec::new();
    for k in 0..n.min(4096) {
        let o = i32a(lump, 4 + k * 4);
        let m = if o >= 0 { lump.get(o as usize..).unwrap_or(&[]) } else { &[][..] };
        let name = if m.len() >= 16 { cstr(&m[0..16]) } else { format!("textur{k}") };
        let (w, h) = if m.len() >= 24 { (u32a(m, 16).max(1) as f64, u32a(m, 20).max(1) as f64) } else { (64.0, 64.0) };
        if let Some(t) = tool_material(&name) {
            out.push(Tex { material: t.into(), w, h, vw: w, vh: h });
            continue;
        }
        let clean = clean_name(&name);
        // eingebettet, sonst aus einer WAD
        let (img, src) = match (with_palette.then(|| miptex_rgba(m)).flatten(), wads.find(&name)) {
            (Some((_, img)), _) => (Some(img), map_name.to_string()),
            (None, Some((stem, data))) if with_palette => (miptex_rgba(data).map(|x| x.1), stem.to_string()),
            _ => (None, String::new()),
        };
        let Some(img) = img else {
            missing.push(name.clone());
            out.push(Tex { material: format!("goldsrc/fehlt/{clean}").to_ascii_uppercase(), w, h, vw: w, vh: h });
            continue;
        };
        let material = format!("goldsrc/{}/{clean}", clean_name(&src));
        if !files.iter().any(|(p, _)| p == &format!("{material}.vmt")) {
            let (vw, vh) = (crate::vtf_write::pow2(img.w, 2048), crate::vtf_write::pow2(img.h, 2048));
            files.push((format!("{material}.vtf"), crate::vtf_write::encode(&img, VtfOpts { max_size: 2048, ..Default::default() })));
            let mut vmt = format!("\"LightmappedGeneric\"\n{{\n\t\"$basetexture\" \"{material}\"\n\t\"$surfaceprop\" \"{}\"\n", surfaceprop(&name));
            if name.starts_with('{') {
                vmt.push_str("\t\"$alphatest\" \"1\"\n");
            }
            if name.starts_with('!') {
                vmt.push_str("\t\"$translucent\" \"1\"\n\t\"$alpha\" \"0.6\"\n");
            }
            vmt.push_str(&format!("\t// GoldSrc-Textur \"{name}\" aus {src} – von Amboss umgewandelt\n}}\n"));
            files.push((format!("{material}.vmt"), vmt.into_bytes()));
            r.textures += 1;
            out.push(Tex { material: material.to_ascii_uppercase(), w: img.w as f64, h: img.h as f64, vw: vw as f64, vh: vh as f64 });
        } else {
            let prev = out.iter().find(|t| t.material.eq_ignore_ascii_case(&material)).cloned();
            out.push(prev.unwrap_or(Tex { material: material.to_ascii_uppercase(), w, h, vw: w, vh: h }));
        }
    }
    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        let shown: Vec<&str> = missing.iter().take(12).map(|s| s.as_str()).collect();
        let more = if missing.len() > 12 { format!(" … (+{})", missing.len() - 12) } else { String::new() };
        r.notes.push(format!("{} Texturen nicht gefunden (WAD fehlt?): {}{more}", missing.len(), shown.join(", ")));
    }
    out
}

// ------------------------------------------------------------------ Baum → Brushes

struct G<'a> {
    b: &'a [u8],
    lumps: [(usize, usize); 15],
    planes: Vec<Plane>,
    texs: Vec<Tex>,
}

/// Ein konvexer Bereich des Baums: Ebenen (nach außen zeigend) mit dem Knoten, aus dem sie stammen
struct Region {
    contents: i32,
    sides: Vec<(Plane, i32)>,
}

impl G<'_> {
    fn lump(&self, i: usize) -> &[u8] {
        let (o, l) = self.lumps[i];
        &self.b[o..o + l]
    }

    /// Alle Bereiche unter `head`, deren Inhalt `keep` erfüllt
    fn regions(&self, head: i32, keep: &dyn Fn(i32) -> bool) -> Vec<Region> {
        let (nodes, leaves) = (self.lump(L_NODES), self.lump(L_LEAVES));
        let mut out = Vec::new();
        let mut stack: Vec<(i32, Vec<(Plane, i32)>)> = vec![(head, Vec::new())];
        let mut guard = 0usize;
        while let Some((n, path)) = stack.pop() {
            guard += 1;
            if guard > 4_000_000 {
                break;
            }
            if n >= 0 {
                let o = n as usize * 24;
                if o + 24 > nodes.len() || path.len() > 256 {
                    continue;
                }
                let Some(p) = self.planes.get(i32a(nodes, o).max(0) as usize).copied() else { continue };
                // Vorderseite: n·x ≥ d → nach außen zeigt −n; Rückseite: nach außen zeigt n
                let mut front = path.clone();
                front.push((Plane { normal: -p.normal, dist: -p.dist }, n));
                let mut back = path;
                back.push((p, n));
                stack.push((i16a(nodes, o + 4) as i32, front));
                stack.push((i16a(nodes, o + 6) as i32, back));
            } else {
                let leaf = (-1 - n) as usize;
                let contents = i32a(leaves, leaf * 28);
                if keep(contents) {
                    out.push(Region { contents, sides: path });
                }
            }
        }
        out
    }

    fn face_poly(&self, face: usize) -> Option<(Vec<DVec3>, DVec3, usize)> {
        let f = self.lump(L_FACES);
        let o = face * 20;
        if o + 20 > f.len() {
            return None;
        }
        let p = self.planes.get(u16a(f, o) as usize)?;
        let normal = if u16a(f, o + 2) != 0 { -p.normal } else { p.normal };
        let first = i32a(f, o + 4).max(0) as usize;
        let n = u16a(f, o + 8) as usize;
        let ti = u16a(f, o + 10) as usize;
        let (se, ed, vx) = (self.lump(L_SURFEDGES), self.lump(L_EDGES), self.lump(L_VERTICES));
        let poly = (0..n)
            .map(|k| {
                let s = i32a(se, (first + k) * 4);
                let v = if s >= 0 { u16a(ed, s as usize * 4) } else { u16a(ed, (-s) as usize * 4 + 2) } as usize;
                vec3(vx, v * 12)
            })
            .collect();
        Some((poly, normal, ti))
    }

    /// Material und Achsen einer Texinfo (auf die VTF-Größe umgerechnet)
    fn texinfo(&self, ti: usize) -> Option<(String, TexAxis, TexAxis)> {
        let l = self.lump(L_TEXINFO);
        let o = ti * 40;
        if o + 40 > l.len() {
            return None;
        }
        let t = self.texs.get(i32a(l, o + 32).max(0) as usize)?;
        let axis = |k: usize, orig: f64, vtf: f64| {
            let v = vec3(l, o + k * 16);
            let len = v.length().max(1e-6);
            let f = vtf / orig;
            TexAxis { axis: v / len, shift: f32a(l, o + k * 16 + 12) * f, scale: 1.0 / len / f }
        };
        Some((t.material.clone(), axis(0, t.w, t.vw), axis(1, t.h, t.vh)))
    }

    /// Sichtbare Fläche, die auf einer Bereichsseite liegt (größte Überdeckung)
    fn side_face(&self, node: i32, outward: &Plane, poly: &[DVec3]) -> Option<usize> {
        if node < 0 || poly.len() < 3 {
            return None;
        }
        let nodes = self.lump(L_NODES);
        let o = node as usize * 24;
        let (first, count) = (u16a(nodes, o + 20) as usize, u16a(nodes, o + 22) as usize);
        let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
        // Kantenebenen der Seite (nach außen), um Flächen darauf zuzuschneiden
        let edges: Vec<Plane> = (0..poly.len())
            .filter_map(|k| {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let mut n = (b - a).cross(outward.normal).normalize_or_zero();
                if n == DVec3::ZERO {
                    return None;
                }
                if n.dot(a - c) < 0.0 {
                    n = -n;
                }
                Some(Plane { normal: n, dist: n.dot(a) })
            })
            .collect();
        let mut best: Option<(f64, usize)> = None;
        for f in first..first + count {
            let Some((fp, fnorm, ti)) = self.face_poly(f) else { continue };
            if fnorm.dot(outward.normal) < 0.999 {
                continue;
            }
            let mut clipped = fp;
            for e in &edges {
                clipped = clip_polygon(&clipped, e);
            }
            let area = poly_area(&clipped);
            if area > 0.25 && best.is_none_or(|(a, _)| area > a) {
                best = Some((area, ti));
            }
        }
        best.map(|b| b.1)
    }

    /// Bereich → Brush; `water` färbt Oberseiten mit dem Wassermaterial
    fn brush(&self, map: &mut Map, reg: &Region, bounds: &Aabb, offset: DVec3, water: bool) -> Option<Solid> {
        let mut planes: Vec<(Plane, i32)> = reg.sides.clone();
        for (n, d) in [(DVec3::X, bounds.max.x), (-DVec3::X, -bounds.min.x), (DVec3::Y, bounds.max.y), (-DVec3::Y, -bounds.min.y), (DVec3::Z, bounds.max.z), (-DVec3::Z, -bounds.min.z)] {
            planes.push((Plane { normal: n, dist: d }, -1));
        }
        let opt: Vec<Option<Plane>> = planes.iter().map(|p| Some(p.0)).collect();
        let polys = polygons_from_planes(&opt);
        let mut sides = Vec::new();
        for ((p, node), poly) in planes.iter().zip(&polys) {
            if poly.len() < 3 {
                continue;
            }
            let mut q = *p;
            q.dist += q.normal.dot(offset);
            let mut side = map.side_from_plane(q, "TOOLS/TOOLSNODRAW", 0.25);
            if water {
                if p.normal.z > 0.7 {
                    side.material = WATER_MATERIAL.into();
                }
            } else if let Some((mat, u, v)) = self.side_face(*node, p, poly).and_then(|ti| self.texinfo(ti)) {
                let shift = |a: &TexAxis| TexAxis { axis: a.axis, scale: a.scale, shift: a.shift - offset.dot(a.axis) / a.scale };
                side.material = mat;
                side.uaxis = shift(&u);
                side.vaxis = shift(&v);
            }
            sides.push(side);
        }
        if sides.len() < 4 {
            return None;
        }
        let id = map.alloc_id();
        let mut s = Solid::new(id, sides);
        s.cleanup();
        s.is_valid().then_some(s)
    }

    fn model(&self, i: usize) -> Option<(Aabb, i32)> {
        let m = self.lump(L_MODELS);
        let o = i * 64;
        if o + 64 > m.len() {
            return None;
        }
        Some((Aabb::new(vec3(m, o), vec3(m, o + 12)), i32a(m, o + 36)))
    }
}

fn poly_area(p: &[DVec3]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    let mut a = DVec3::ZERO;
    for k in 1..p.len() - 1 {
        a += (p[k] - p[0]).cross(p[k + 1] - p[0]);
    }
    a.length() * 0.5
}

// ------------------------------------------------------------------ Entities

fn angle_to_angles(a: &str) -> String {
    match a.trim() {
        "-1" => "-90 0 0".into(),
        "-2" => "90 0 0".into(),
        y => format!("0 {y} 0"),
    }
}

/// GoldSrc-Entity → Source-Klasse und Schlüssel; `brush` = hat Brushes
fn convert_entity(kv: &[(String, String)], brush: bool, notes: &mut Vec<String>) -> (String, Vec<(String, String)>) {
    let get = |k: &str| kv.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
    let class = get("classname").unwrap_or_default().to_ascii_lowercase();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut new_class = class.clone();
    let mut extra: Vec<(&str, String)> = Vec::new();
    match class.as_str() {
        "func_wall" | "func_wall_toggle" => new_class = "func_brush".into(),
        "func_illusionary" => {
            new_class = "func_brush".into();
            extra.push(("Solidity", "1".into()));
        }
        "func_water" => {
            new_class = "func_brush".into();
            extra.push(("Solidity", "1".into()));
            notes.push("func_water → nicht solider func_brush (Wasser bitte von Hand nachbauen)".into());
        }
        "func_pushable" => new_class = "func_physbox".into(),
        "func_ladder" => new_class = "func_detail".into(),
        _ => {}
    }
    for (k, v) in kv {
        let kl = k.to_ascii_lowercase();
        match kl.as_str() {
            "classname" | "model" | "wad" | "mapversion" | "_generator" => {}
            "angle" if brush => out.push(("movedir".into(), angle_to_angles(v))),
            "angles" if brush => out.push(("movedir".into(), v.clone())),
            "angle" => out.push(("angles".into(), angle_to_angles(v))),
            "_cone" if class == "light_spot" => out.push(("_inner_cone".into(), v.clone())),
            "_cone2" if class == "light_spot" => out.push(("_cone".into(), v.clone())),
            _ => out.push((k.clone(), v.clone())),
        }
    }
    for (k, v) in extra {
        out.push((k.into(), v));
    }
    if new_class == "func_detail" {
        out.retain(|(k, _)| k == "origin");
    }
    (new_class, out)
}

/// Bekannte Source-Klassen ohne GoldSrc-Gegenstück werden als Hinweis gesammelt
fn foreign_class(c: &str) -> bool {
    c.starts_with("monster_") || c.starts_with("weapon_") || c.starts_with("ammo_") || c.starts_with("item_") || c == "info_node" || c == "info_node_air" || c.starts_with("scripted_") || c == "env_sound" || c == "multi_manager"
}

// ------------------------------------------------------------------ Hauptfunktion

/// Optionen des GoldSrc-Decompilers
pub struct Opts<'a> {
    pub wads: &'a Wads,
    /// Name der Map (Unterordner für eingebettete Texturen)
    pub map_name: String,
}

/// GoldSrc-BSP → Map, Bericht und die erzeugten Materialdateien (Pfad relativ zu `materials/`)
pub fn decompile(b: &[u8], o: &Opts) -> Result<(Map, Report, Vec<(String, Vec<u8>)>), String> {
    let ver = version(b).ok_or("Keine GoldSrc-BSP (Version 29/30)")?;
    let mut lumps = [(0usize, 0usize); 15];
    for (i, l) in lumps.iter_mut().enumerate() {
        let (ofs, len) = (u32a(b, 4 + i * 8) as usize, u32a(b, 8 + i * 8) as usize);
        *l = if ofs + len <= b.len() { (ofs, len) } else { (0, 0) };
    }
    // Blue Shift vertauscht Entities und Ebenen
    let starts_text = |l: (usize, usize)| b[l.0..l.0 + l.1].iter().find(|c| !c.is_ascii_whitespace()) == Some(&b'{');
    if !starts_text(lumps[L_ENTITIES]) && starts_text(lumps[L_PLANES]) {
        lumps.swap(L_ENTITIES, L_PLANES);
    }
    let mut r = Report { version: ver, ..Default::default() };
    let pl = &b[lumps[L_PLANES].0..lumps[L_PLANES].0 + lumps[L_PLANES].1];
    let planes: Vec<Plane> = (0..pl.len() / 20).map(|i| Plane { normal: vec3(pl, i * 20), dist: f32a(pl, i * 20 + 12) }).collect();
    let mut files = Vec::new();
    let tl = &b[lumps[L_TEXTURES].0..lumps[L_TEXTURES].0 + lumps[L_TEXTURES].1];
    let texs = convert_textures(tl, o.wads, &o.map_name, ver == 30, &mut files, &mut r);
    if ver == 29 {
        r.notes.push("Quake-1-BSP: Texturen haben keine eigene Farbpalette und werden nicht umgewandelt".into());
    }
    let g = G { b, lumps, planes, texs };
    let mut map = Map::new();

    // Entities
    let text = String::from_utf8_lossy(g.lump(L_ENTITIES)).to_string();
    let mut foreign: Vec<String> = Vec::new();
    for kv in parse_entities(&text) {
        let class = kv.iter().find(|(k, _)| k == "classname").map(|(_, v)| v.to_ascii_lowercase()).unwrap_or_default();
        if class == "worldspawn" {
            for (k, v) in &kv {
                match k.to_ascii_lowercase().as_str() {
                    "classname" | "wad" | "mapversion" | "_generator" | "sounds" | "maxrange" => {}
                    "skyname" => {
                        r.notes.push(format!("Himmel \"{v}\" (GoldSrc) durch sky_day01_01 ersetzt"));
                        map.world.set("skyname", "sky_day01_01");
                    }
                    _ => map.world.set(k, v.clone()),
                }
            }
            continue;
        }
        let model = kv.iter().find(|(k, v)| k == "model" && v.starts_with('*')).and_then(|(_, v)| v[1..].parse::<usize>().ok());
        let (new_class, props) = convert_entity(&kv, model.is_some(), &mut r.notes);
        if foreign_class(&class) && !foreign.contains(&class) {
            foreign.push(class.clone());
        }
        let id = map.alloc_id();
        let mut e = Entity::new(id, &new_class);
        for (k, v) in props {
            e.set(&k, v);
        }
        if let Some(m) = model {
            if let Some((bounds, head)) = g.model(m) {
                let off = e.get("origin").and_then(crate::map::parse_vec).unwrap_or(DVec3::ZERO);
                let pad = Aabb::new(bounds.min - DVec3::ONE, bounds.max + DVec3::ONE);
                let regs = g.regions(head, &|c| c != -1 && c >= C_TRANSLUCENT);
                for reg in &regs {
                    match g.brush(&mut map, reg, &pad, off, false) {
                        Some(mut s) => {
                            if class == "func_ladder" {
                                s.set_material_all("TOOLS/TOOLSINVISIBLELADDER");
                            }
                            e.solids.push(s.into());
                        }
                        None => r.skipped += 1,
                    }
                }
            }
            if new_class == "func_detail" {
                e.props.retain(|(k, _)| k == "classname");
            }
            if e.solids.is_empty() {
                r.notes.push(format!("{class}: keine Brushes rekonstruierbar – bleibt als Punkt-Entity"));
            } else {
                r.brush_entities += 1;
                r.brushes += e.solids.len();
            }
        } else if class.starts_with("func_") || class.starts_with("trigger_") {
            r.notes.push(format!("{class}: ohne Brush-Modell"));
        }
        r.entities += 1;
        map.entities.push(e.into());
    }
    if !foreign.is_empty() {
        foreign.sort();
        r.notes.push(format!("Klassen ohne Source-Gegenstück (bleiben erhalten, wirken im Spiel nicht): {}", foreign.join(", ")));
    }

    // Welt: solide Bereiche und Himmel, Rand 16 Einheiten um die sichtbaren Flächen
    let (wb, head) = g.model(0).ok_or("BSP ohne Welt-Modell")?;
    let pad = Aabb::new(wb.min - DVec3::splat(16.0), wb.max + DVec3::splat(16.0));
    for reg in g.regions(head, &|c| c == C_SOLID || c == C_SKY) {
        match g.brush(&mut map, &reg, &pad, DVec3::ZERO, false) {
            Some(s) => {
                map.world.solids.push(s.into());
                r.brushes += 1;
            }
            None => r.skipped += 1,
        }
    }
    let mut water = 0;
    for reg in g.regions(head, &|c| (C_LAVA..=C_WATER).contains(&c)) {
        if let Some(s) = g.brush(&mut map, &reg, &pad, DVec3::ZERO, true) {
            debug_assert!(reg.contents <= C_WATER);
            map.world.solids.push(s.into());
            water += 1;
        }
    }
    if water > 0 {
        r.brushes += water;
        r.notes.push(format!("{water} Wasser-Brushes mit {WATER_MATERIAL} (Schleim und Lava ebenfalls als Wasser)"));
    }
    r.notes.push("Clip-Brushes stehen nur in den Kollisions-Hüllen und kommen nicht zurück".into());
    Ok((map, r, files))
}

/// Erzeugte Materialien speichern: in den Spielordner (materials/…) oder neben die VMF
pub fn write_files(files: &[(String, Vec<u8>)], materials_root: &Path) -> Result<(), String> {
    for (name, data) in files {
        let p = materials_root.join(name);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn f(v: &mut Vec<u8>, x: f32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn i(v: &mut Vec<u8>, x: i32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn h(v: &mut Vec<u8>, x: i16) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    /// Miptex mit Palette (w×h, eine Farbe je Pixelindex)
    pub fn miptex(name: &str, w: u32, h: u32, embed: bool) -> Vec<u8> {
        let mut m = Vec::new();
        let mut n = name.as_bytes().to_vec();
        n.resize(16, 0);
        m.extend(n);
        m.extend_from_slice(&w.to_le_bytes());
        m.extend_from_slice(&h.to_le_bytes());
        if !embed {
            m.extend_from_slice(&[0u8; 16]);
            return m;
        }
        let mut ofs = 40u32;
        for k in 0..4 {
            m.extend_from_slice(&ofs.to_le_bytes());
            ofs += (w >> k) * (h >> k);
        }
        for k in 0..4 {
            for p in 0..(w >> k) * (h >> k) {
                m.push(if p % 2 == 0 { 1 } else { 255 });
            }
        }
        m.extend_from_slice(&256u16.to_le_bytes());
        for c in 0..256 {
            m.extend_from_slice(&[c as u8, 100, 200]);
        }
        m
    }

    /// Kleine GoldSrc-BSP: Raum 0..128 (innen), Boden-Wasser, eine Tür (*1) mit origin
    ///
    /// Baum der Welt: sechs Ebenen schneiden den Innenraum aus; alles außerhalb ist solide.
    /// Knoten k teilt an Ebene k: Vorderseite = draußen (solid), Rückseite = weiter innen.
    pub fn test_bsp() -> Vec<u8> {
        let mut l: Vec<Vec<u8>> = vec![Vec::new(); 15];
        l[L_ENTITIES] = b"{\n\"classname\" \"worldspawn\"\n\"wad\" \"\\half-life\\valve\\test.wad\"\n\"skyname\" \"desert\"\n}\n{\n\"classname\" \"func_door\"\n\"model\" \"*1\"\n\"origin\" \"0 0 200\"\n\"angle\" \"90\"\n\"targetname\" \"tuer\"\n}\n{\n\"classname\" \"info_player_start\"\n\"origin\" \"64 64 36\"\n\"angle\" \"180\"\n}\n{\n\"classname\" \"monster_scientist\"\n\"origin\" \"32 32 36\"\n}\n\0".to_vec();
        // Ebenen 0–5: Raumwände (Normalen nach außen = Vorderseite draußen); 6: Wasseroberfläche z=16
        // 7–12: Tür-Quader −8..8
        let mut planes: Vec<(DVec3, f64)> = vec![(DVec3::X, 128.0), (-DVec3::X, 0.0), (DVec3::Y, 128.0), (-DVec3::Y, 0.0), (DVec3::Z, 128.0), (-DVec3::Z, 0.0), (DVec3::Z, 16.0)];
        for (n, d) in [(DVec3::X, 8.0), (-DVec3::X, 8.0), (DVec3::Y, 8.0), (-DVec3::Y, 8.0), (DVec3::Z, 8.0), (-DVec3::Z, 8.0)] {
            planes.push((n, d));
        }
        for (n, d) in &planes {
            for c in n.to_array() {
                f(&mut l[L_PLANES], c as f32);
            }
            f(&mut l[L_PLANES], *d as f32);
            i(&mut l[L_PLANES], 0);
        }
        // Texturen: 0 = eingebettet "{GITTER" (24×16, kein 2ⁿ), 1 = aus WAD "CRETE1", 2 = "sky", 3 = fehlt
        let texs = [miptex("{GITTER", 24, 16, true), miptex("CRETE1", 16, 16, false), miptex("sky", 16, 16, false), miptex("FEHLT", 16, 16, false)];
        i(&mut l[L_TEXTURES], texs.len() as i32);
        let mut ofs = 4 + 4 * texs.len() as i32;
        for t in &texs {
            i(&mut l[L_TEXTURES], ofs);
            ofs += t.len() as i32;
        }
        for t in &texs {
            l[L_TEXTURES].extend_from_slice(t);
        }
        // Texinfo 0: Achsen X/−Z, Textur 1 (CRETE1); 1: Achsen X/Y Textur 0 ({GITTER)
        for (s, t, tex) in [([1.0, 0.0, 0.0, 4.0], [0.0, 0.0, -1.0, 0.0], 1), ([1.0, 0.0, 0.0, 0.0], [0.0, -1.0, 0.0, 0.0], 0)] {
            for x in s.iter().chain(t.iter()) {
                f(&mut l[L_TEXINFO], *x as f32);
            }
            i(&mut l[L_TEXINFO], tex);
            i(&mut l[L_TEXINFO], 0);
        }
        // Eine sichtbare Fläche: Wand x=128 von innen gesehen (Normale −X, liegt auf Ebene 0, side=1)
        for p in [[128.0, 0.0, 0.0], [128.0, 0.0, 128.0], [128.0, 128.0, 128.0], [128.0, 128.0, 0.0]] {
            for c in p {
                f(&mut l[L_VERTICES], c);
            }
        }
        // Türfläche oben (z=8, Normale +Z), Ebene 11
        for p in [[-8.0, -8.0, 8.0], [-8.0, 8.0, 8.0], [8.0, 8.0, 8.0], [8.0, -8.0, 8.0]] {
            for c in p {
                f(&mut l[L_VERTICES], c);
            }
        }
        for e in [[0u16, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4]] {
            for v in e {
                l[L_EDGES].extend_from_slice(&v.to_le_bytes());
            }
        }
        for k in 0..8 {
            i(&mut l[L_SURFEDGES], k);
        }
        let face = |plane: u16, side: u16, first: i32, ti: u16, out: &mut Vec<u8>| {
            out.extend_from_slice(&plane.to_le_bytes());
            out.extend_from_slice(&side.to_le_bytes());
            out.extend_from_slice(&first.to_le_bytes());
            out.extend_from_slice(&4u16.to_le_bytes());
            out.extend_from_slice(&ti.to_le_bytes());
            out.extend_from_slice(&[0u8; 8]);
        };
        face(0, 1, 0, 0, &mut l[L_FACES]);
        face(11, 0, 4, 1, &mut l[L_FACES]);
        // Blätter: 0 = solid (gemeinsam), 1 = leer, 2 = Wasser
        for c in [C_SOLID, -1, C_WATER] {
            let mut lf = Vec::new();
            i(&mut lf, c);
            lf.resize(28, 0);
            l[L_LEAVES].extend(lf);
        }
        // Welt-Knoten 0..5: Ebene k, Vorderseite → Blatt 0 (solid), Rückseite → Knoten k+1;
        // Knoten 6: Ebene 6 (Wasser z=16): vorne leer, hinten Wasser.
        // Fläche 0 hängt an Knoten 0.
        let node = |plane: i32, front: i16, back: i16, ff: u16, nf: u16, out: &mut Vec<u8>| {
            i(out, plane);
            h(out, front);
            h(out, back);
            out.extend_from_slice(&[0u8; 12]);
            out.extend_from_slice(&ff.to_le_bytes());
            out.extend_from_slice(&nf.to_le_bytes());
        };
        for k in 0..6 {
            node(k, -1, (k + 1) as i16, 0, if k == 0 { 1 } else { 0 }, &mut l[L_NODES]);
        }
        node(6, -2, -3, 0, 0, &mut l[L_NODES]);
        // Tür-Knoten 7..12: Vorderseite leer (Blatt 1), Rückseite weiter; zuletzt solid
        for k in 0..6 {
            let back = if k == 5 { -1 } else { (8 + k) as i16 };
            node(7 + k, -2, back, 1, if k == 4 { 1 } else { 0 }, &mut l[L_NODES]);
        }
        // Modelle: Welt (Kopf 0), Tür (Kopf 7)
        for (mins, maxs, head) in [([0.0f32, 0.0, 0.0], [128.0f32, 128.0, 128.0], 0), ([-8.0, -8.0, -8.0], [8.0, 8.0, 8.0], 7)] {
            for c in mins.iter().chain(maxs.iter()) {
                f(&mut l[L_MODELS], *c);
            }
            for _ in 0..3 {
                f(&mut l[L_MODELS], 0.0);
            }
            i(&mut l[L_MODELS], head);
            let n = l[L_MODELS].len() + 24;
            l[L_MODELS].resize(n, 0);
        }
        let mut b = 30u32.to_le_bytes().to_vec();
        b.resize(4 + 15 * 8, 0);
        for k in 0..15 {
            let ofs = b.len();
            b.extend_from_slice(&l[k]);
            b[4 + k * 8..8 + k * 8].copy_from_slice(&(ofs as u32).to_le_bytes());
            b[8 + k * 8..12 + k * 8].copy_from_slice(&(l[k].len() as u32).to_le_bytes());
        }
        b
    }

    /// WAD3 mit einer Textur
    pub fn test_wad() -> Vec<u8> {
        let tex = miptex("CRETE1", 16, 16, true);
        let mut w = b"WAD3".to_vec();
        i(&mut w, 1);
        i(&mut w, (12 + tex.len()) as i32);
        w.extend_from_slice(&tex);
        i(&mut w, 12);
        i(&mut w, tex.len() as i32);
        i(&mut w, tex.len() as i32);
        w.extend_from_slice(&[0x43, 0, 0, 0]);
        let mut n = b"crete1".to_vec();
        n.resize(16, 0);
        w.extend(n);
        w
    }

    #[test]
    fn miptex_and_wad() {
        let (name, img) = miptex_rgba(&miptex("{GITTER", 24, 16, true)).unwrap();
        assert_eq!(name, "{GITTER");
        assert_eq!((img.w, img.h), (24, 16));
        assert_eq!(&img.px[0..4], &[1, 100, 200, 255]);
        assert_eq!(img.px[7], 0, "Index 255 ist bei {{-Texturen durchsichtig");
        let mut w = Wads::default();
        w.add("test", test_wad());
        assert_eq!(w.len(), 1);
        assert!(w.find("CRETE1").is_some());
        assert_eq!(clean_name("{GITTER"), "a_gitter");
        assert_eq!(clean_name("!water-1"), "w_water_1");
        assert_eq!(tool_material("AAATRIGGER"), Some("TOOLS/TOOLSTRIGGER"));
        let dirs = find_wads(&[], "\\half-life\\valve\\test.wad;x.wad");
        assert!(dirs.is_empty());
    }

    #[test]
    fn decompile_goldsrc() {
        if let Ok(p) = std::env::var("AMBOSS_WRITE_TEST_GOLDSRC") {
            std::fs::write(&p, test_bsp()).unwrap();
            std::fs::write(std::path::Path::new(&p).with_file_name("test.wad"), test_wad()).unwrap();
        }
        let mut wads = Wads::default();
        wads.add("test", test_wad());
        let (map, r, files) = decompile(&test_bsp(), &Opts { wads: &wads, map_name: "raum".into() }).unwrap();
        assert_eq!(r.version, 30);
        assert_eq!(map.world.get("skyname"), Some("sky_day01_01"));
        assert!(map.world.get("wad").is_none());
        // Welt: sechs Außenwände (16 dick) + Wasser
        let water: Vec<_> = map.world.solids.iter().filter(|s| s.sides.iter().any(|sd| sd.material == WATER_MATERIAL)).collect();
        assert_eq!(water.len(), 1, "{r:?}");
        assert_eq!(water[0].bounds.max.z, 16.0);
        assert_eq!(water[0].bounds.min, DVec3::ZERO);
        let walls: Vec<_> = map.world.solids.iter().filter(|s| !s.sides.iter().any(|sd| sd.material == WATER_MATERIAL)).collect();
        assert_eq!(walls.len(), 6, "{r:?}");
        let east = walls.iter().find(|s| s.bounds.min.x == 128.0).unwrap();
        assert_eq!(east.bounds.max.x, 144.0);
        // Innenseite der Ostwand: Textur aus der WAD, Achsen übernommen
        let inner = east.sides.iter().find(|s| s.plane().is_some_and(|p| p.normal.x < -0.99)).unwrap();
        assert_eq!(inner.material, "GOLDSRC/TEST/CRETE1");
        assert_eq!(inner.uaxis.axis, DVec3::X);
        assert!((inner.uaxis.shift - 4.0).abs() < 1e-6 && (inner.uaxis.scale - 1.0).abs() < 1e-6);
        assert!(east.sides.iter().filter(|s| s.material == "TOOLS/TOOLSNODRAW").count() == 5);
        // Tür: um origin verschoben, Oberseite {GITTER (24 → 32 breit: Achse umgerechnet)
        let door = map.entities.iter().find(|e| e.classname() == "func_door").unwrap();
        assert_eq!(door.solids.len(), 1);
        assert_eq!(door.solids[0].bounds.min, DVec3::new(-8.0, -8.0, 192.0));
        assert_eq!(door.get("movedir"), Some("0 90 0"));
        assert!(door.get("angles").is_none());
        let top = door.solids[0].sides.iter().find(|s| s.plane().is_some_and(|p| p.normal.z > 0.99)).unwrap();
        assert_eq!(top.material, "GOLDSRC/RAUM/A_GITTER");
        assert!((top.uaxis.scale - 0.75).abs() < 1e-6, "{}", top.uaxis.scale);
        // Punkt-Entities
        let ps = map.entities.iter().find(|e| e.classname() == "info_player_start").unwrap();
        assert_eq!(ps.get("angles"), Some("0 180 0"));
        assert!(r.notes.iter().any(|n| n.contains("monster_scientist")));
        assert!(r.notes.iter().any(|n| n.contains("FEHLT")));
        // Dateien: eingebettete und WAD-Textur, je VTF + VMT
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert!(names.contains(&"goldsrc/raum/a_gitter.vmt") && names.contains(&"goldsrc/test/crete1.vtf"), "{names:?}");
        let vmt = String::from_utf8(files.iter().find(|f| f.0 == "goldsrc/raum/a_gitter.vmt").unwrap().1.clone()).unwrap();
        assert!(vmt.contains("$alphatest"));
        assert_eq!(r.textures, 2);
        // VMF wieder lesbar
        let back = Map::from_vmf(&map.to_vmf()).unwrap();
        assert_eq!(back.world.solids.len(), 7);
        assert!(decompile(b"kaputt", &Opts { wads: &wads, map_name: String::new() }).is_err());
    }
}
