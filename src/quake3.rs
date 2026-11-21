//! Quake 3: Bezier-Patches → Displacements, Texturen aus pk3/Ordnern (Shader-Skripte, TGA/JPG) → VTF,
//! Entities umwandeln und kompilierte Q3-BSPs (IBSP Version 46/47) dekompilieren.
//!
//! Der Leser für .map-Dateien selbst steht in `importexport.rs`; er ruft für Patches und
//! Texturen die Funktionen hier auf.

use crate::decompile::{parse_entities, Report};
use crate::disp::Disp;
use crate::geometry::{clip_polygon, polygons_from_planes, Plane};
use crate::map::{default_tex_axes, Entity, Map, Side, Solid, TexAxis};
use crate::vtf_write::VtfOpts;
use glam::{DVec2, DVec3};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ Texturen

/// Art eines Materials (bestimmt, was mit dem Brush passiert)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Normal,
    /// Wasser/Schleim/Lava: Oberseite Wassermaterial, Rest nodraw
    Water,
    /// Nebel-Volumen: Brush entfällt
    Fog,
}

/// Aufgelöste Textur
#[derive(Clone, Debug)]
pub struct TexRes {
    pub material: String,
    /// Größe des Originalbilds und der VTF (für die Umrechnung der Achsen)
    pub w: f64,
    pub h: f64,
    pub vw: f64,
    pub vh: f64,
    pub kind: Kind,
}

impl TexRes {
    pub fn plain(material: &str) -> TexRes {
        TexRes { material: material.to_string(), w: 128.0, h: 128.0, vw: 128.0, vh: 128.0, kind: Kind::Normal }
    }
}

/// Werkzeug- und Sondertexturen aus Quake 3 (und Quake 1/2, GoldSrc) ohne Spieldaten erkennen
pub fn special(name: &str) -> Option<TexRes> {
    let n = name.trim_start_matches("textures/").to_ascii_lowercase();
    let tool = |m: &str| Some(TexRes::plain(m));
    match n.as_str() {
        "common/caulk" | "common/nodraw" | "common/nodrawnonsolid" | "common/caulkshadow" | "common/nodrawwater" => return tool("TOOLS/TOOLSNODRAW"),
        "common/clip" | "common/full_clip" | "common/clipfull" => return tool("TOOLS/TOOLSPLAYERCLIP"),
        "common/weapclip" | "common/metalclip" => return tool("TOOLS/TOOLSCLIP"),
        "common/botclip" | "common/monsterclip" => return tool("TOOLS/TOOLSNPCCLIP"),
        "common/trigger" => return tool("TOOLS/TOOLSTRIGGER"),
        "common/hint" | "common/hintskip" => return tool("TOOLS/TOOLSHINT"),
        "common/skip" => return tool("TOOLS/TOOLSSKIP"),
        "common/areaportal" => return tool("TOOLS/TOOLSAREAPORTAL"),
        "common/origin" => return tool("TOOLS/TOOLSORIGIN"),
        "common/donotenter" => return tool("TOOLS/TOOLSNODRAW"),
        _ => {}
    }
    if n.starts_with("skies/") || n.starts_with("sky/") || n == "sky1" || n == "sky" {
        return tool("TOOLS/TOOLSSKYBOX");
    }
    if n.starts_with("liquids/") || n.starts_with("*") {
        return Some(TexRes { kind: Kind::Water, ..TexRes::plain(crate::goldsrc::WATER_MATERIAL) });
    }
    if n.starts_with("sfx/fog") || n.contains("/fog") || n.starts_with("fogs/") {
        return Some(TexRes { kind: Kind::Fog, ..TexRes::plain("TOOLS/TOOLSNODRAW") });
    }
    if !n.contains('/') {
        if let Some(t) = crate::goldsrc::tool_material(&n) {
            return tool(t);
        }
    }
    None
}

/// Ein Eintrag in einem pk3 (ZIP), Daten werden erst beim Bedarf gelesen
#[derive(Clone)]
struct PakEntry {
    pak: usize,
    method: u16,
    csize: usize,
    usize: usize,
    local: usize,
}

/// Shader aus scripts/*.shader (nur, was für die Umwandlung zählt)
#[derive(Clone, Debug, Default)]
struct Shader {
    image: Option<String>,
    editor: Option<String>,
    parms: Vec<String>,
    alphafunc: bool,
    blend: bool,
}

/// Spieldaten von Quake 3 (pk3-Pakete und entpackte Ordner) und erzeugte Materialien
#[derive(Default)]
pub struct Assets {
    paks: Vec<PathBuf>,
    /// Name (klein, `/`) → Fundstelle; Ordner-Dateien haben Vorrang vor Paketen
    pak_files: HashMap<String, PakEntry>,
    dirs: Vec<PathBuf>,
    shaders: HashMap<String, Shader>,
    cache: HashMap<String, Option<TexRes>>,
    /// erzeugte Dateien (Pfad relativ zu materials/)
    pub files: Vec<(String, Vec<u8>)>,
    pub converted: usize,
    pub missing: Vec<String>,
}

fn u16le(b: &[u8], o: usize) -> usize {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize).unwrap_or(0)
}
fn u32le(b: &[u8], o: usize) -> usize {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize).unwrap_or(0)
}

/// Nur das Zentralverzeichnis eines ZIPs lesen (pk3 können Hunderte MB groß sein)
fn zip_index(path: &Path) -> Result<Vec<(String, u16, usize, usize, usize)>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len() as usize;
    let tail = len.min(22 + 65535);
    f.seek(SeekFrom::Start((len - tail) as u64)).map_err(|e| e.to_string())?;
    let mut t = vec![0u8; tail];
    f.read_exact(&mut t).map_err(|e| e.to_string())?;
    let eocd = (0..t.len().saturating_sub(21)).rev().find(|&i| t[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).ok_or("kein ZIP")?;
    let (n, size, ofs) = (u16le(&t, eocd + 10), u32le(&t, eocd + 12), u32le(&t, eocd + 16));
    f.seek(SeekFrom::Start(ofs as u64)).map_err(|e| e.to_string())?;
    let mut d = vec![0u8; size];
    f.read_exact(&mut d).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(n);
    let mut p = 0;
    for _ in 0..n {
        if !d.get(p..).is_some_and(|s| s.starts_with(&[0x50, 0x4b, 0x01, 0x02])) {
            break;
        }
        let method = u16le(&d, p + 10) as u16;
        let (csize, usize_) = (u32le(&d, p + 20), u32le(&d, p + 24));
        let (nl, el, cl) = (u16le(&d, p + 28), u16le(&d, p + 30), u16le(&d, p + 32));
        let local = u32le(&d, p + 42);
        let name = String::from_utf8_lossy(d.get(p + 46..p + 46 + nl).unwrap_or(&[])).replace('\\', "/").to_ascii_lowercase();
        p += 46 + nl + el + cl;
        out.push((name, method, csize, usize_, local));
    }
    Ok(out)
}

/// Ordner, in denen Q3-Daten liegen könnten: Ordner der Datei und darüber (mit pk3s, textures/
/// oder scripts/) und jeweils `baseq3` darunter, dazu ein eigener Ordner
pub fn roots(file: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut cand: Vec<PathBuf> = extra.to_vec();
    for a in file.ancestors().skip(1).take(5) {
        cand.push(a.to_path_buf());
        cand.push(a.join("baseq3"));
    }
    let looks = |d: &Path| {
        d.join("textures").is_dir()
            || d.join("scripts").is_dir()
            || std::fs::read_dir(d).is_ok_and(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("pk3"))))
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for d in cand {
        if d.is_dir() && looks(&d) && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

impl Assets {
    /// Pakete und Ordner einlesen (nur Verzeichnisse und Shader-Skripte)
    pub fn load(roots: &[PathBuf]) -> Assets {
        let mut a = Assets { dirs: roots.to_vec(), ..Default::default() };
        for r in roots {
            let mut paks: Vec<PathBuf> = std::fs::read_dir(r).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pk3"))).collect()).unwrap_or_default();
            // pak0 < pak1 < … : spätere überschreiben frühere
            paks.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
            for p in paks {
                let Ok(idx) = zip_index(&p) else { continue };
                let pi = a.paks.len();
                a.paks.push(p);
                for (name, method, csize, usize_, local) in idx {
                    a.pak_files.insert(name, PakEntry { pak: pi, method, csize, usize: usize_, local });
                }
            }
        }
        // Shader-Skripte: zuerst aus Paketen, dann aus Ordnern (überschreiben)
        let mut scripts: Vec<String> = a.pak_files.keys().filter(|k| k.starts_with("scripts/") && k.ends_with(".shader")).cloned().collect();
        scripts.sort();
        for s in scripts {
            if let Some(b) = a.read_pak(&s) {
                a.parse_shaders(&String::from_utf8_lossy(&b));
            }
        }
        for r in roots {
            if let Ok(rd) = std::fs::read_dir(r.join("scripts")) {
                let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("shader"))).collect();
                v.sort();
                for p in v {
                    if let Ok(t) = std::fs::read_to_string(&p) {
                        a.parse_shaders(&t);
                    }
                }
            }
        }
        a
    }

    pub fn has_data(&self) -> bool {
        !self.pak_files.is_empty() || self.dirs.iter().any(|d| d.join("textures").is_dir())
    }

    pub fn summary(&self) -> String {
        let paks: Vec<String> = self.paks.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect();
        format!("{} pk3 ({}), {} Shader", paks.len(), paks.join(", "), self.shaders.len())
    }

    fn read_pak(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.pak_files.get(name)?;
        let mut f = std::fs::File::open(&self.paks[e.pak]).ok()?;
        let mut hd = [0u8; 30];
        f.seek(SeekFrom::Start(e.local as u64)).ok()?;
        f.read_exact(&mut hd).ok()?;
        let skip = 30 + u16le(&hd, 26) + u16le(&hd, 28);
        f.seek(SeekFrom::Start((e.local + skip) as u64)).ok()?;
        let mut data = vec![0u8; e.csize];
        f.read_exact(&mut data).ok()?;
        match e.method {
            0 => Some(data),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&data, e.usize.max(1) + 16).ok(),
            _ => None,
        }
    }

    /// Datei aus Ordnern oder Paketen (Name relativ zum Spielordner)
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        for d in &self.dirs {
            if let Ok(b) = std::fs::read(d.join(name)) {
                return Some(b);
            }
        }
        self.read_pak(&name.to_ascii_lowercase())
    }

    /// Bild suchen: wie die Engine erst .tga, dann .jpg (und .png)
    fn image(&self, base: &str) -> Option<(Vec<u8>, String)> {
        let stem = base.trim_end_matches(".tga").trim_end_matches(".jpg").trim_end_matches(".png");
        for ext in ["tga", "jpg", "png", "jpeg"] {
            let n = format!("{stem}.{ext}");
            if let Some(b) = self.read(&n) {
                return Some((b, n));
            }
        }
        None
    }

    fn parse_shaders(&mut self, text: &str) {
        let toks = shader_tokens(text);
        let mut i = 0;
        while i < toks.len() {
            let name = toks[i].to_ascii_lowercase();
            i += 1;
            if toks.get(i).map(|s| s.as_str()) != Some("{") {
                continue;
            }
            i += 1;
            let mut sh = Shader::default();
            let mut depth = 1;
            while i < toks.len() && depth > 0 {
                let t = toks[i].to_ascii_lowercase();
                match t.as_str() {
                    "{" => depth += 1,
                    "}" => depth -= 1,
                    "map" | "clampmap" if depth == 2 => {
                        if let Some(m) = toks.get(i + 1) {
                            if !m.starts_with('$') && sh.image.is_none() {
                                sh.image = Some(m.to_ascii_lowercase());
                            }
                        }
                    }
                    "animmap" if depth == 2 => {
                        if let Some(m) = toks.get(i + 2) {
                            if sh.image.is_none() {
                                sh.image = Some(m.to_ascii_lowercase());
                            }
                        }
                    }
                    "qer_editorimage" => sh.editor = toks.get(i + 1).map(|s| s.to_ascii_lowercase()),
                    "surfaceparm" => {
                        if let Some(p) = toks.get(i + 1) {
                            sh.parms.push(p.to_ascii_lowercase());
                        }
                    }
                    "alphafunc" => sh.alphafunc = true,
                    "blendfunc" => sh.blend = true,
                    _ => {}
                }
                i += 1;
            }
            self.shaders.entry(name).or_insert(sh);
        }
    }

    /// Texturname aus der Map → Source-Material (wandelt beim ersten Mal um)
    pub fn resolve(&mut self, name: &str) -> TexRes {
        let key = name.trim_start_matches("textures/").to_ascii_lowercase();
        if let Some(c) = self.cache.get(&key) {
            return c.clone().unwrap_or_else(|| TexRes::plain(&format!("QUAKE3/FEHLT/{}", key.to_ascii_uppercase())));
        }
        let r = self.convert(&key);
        if r.is_none() && !self.missing.contains(&key) {
            self.missing.push(key.clone());
        }
        self.cache.insert(key.clone(), r.clone());
        r.unwrap_or_else(|| TexRes::plain(&format!("QUAKE3/FEHLT/{}", key.to_ascii_uppercase())))
    }

    fn convert(&mut self, key: &str) -> Option<TexRes> {
        let full = format!("textures/{key}");
        let sh = self.shaders.get(&full).cloned();
        if let Some(sh) = &sh {
            let p = |x: &str| sh.parms.iter().any(|s| s == x);
            if p("fog") {
                return Some(TexRes { kind: Kind::Fog, ..TexRes::plain("TOOLS/TOOLSNODRAW") });
            }
            if p("water") || p("slime") || p("lava") {
                return Some(TexRes { kind: Kind::Water, ..TexRes::plain(crate::goldsrc::WATER_MATERIAL) });
            }
            if p("sky") {
                return Some(TexRes::plain("TOOLS/TOOLSSKYBOX"));
            }
            if p("nodraw") && sh.image.is_none() {
                return Some(TexRes::plain("TOOLS/TOOLSNODRAW"));
            }
        }
        if let Some(t) = special(key) {
            return Some(t);
        }
        if !self.has_data() {
            return None;
        }
        let img_name = sh.as_ref().and_then(|s| s.image.clone().or_else(|| s.editor.clone())).unwrap_or_else(|| full.clone());
        let (bytes, label) = self.image(&img_name).or_else(|| sh.as_ref().and_then(|s| s.editor.clone()).and_then(|e| self.image(&e))).or_else(|| self.image(&full))?;
        let img = crate::vtf_write::load_any(&bytes, &label).ok()?;
        let material = format!("quake3/{key}").replace(|c: char| !(c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-'), "_");
        let (vw, vh) = (crate::vtf_write::pow2(img.w, 2048), crate::vtf_write::pow2(img.h, 2048));
        self.files.push((format!("{material}.vtf"), crate::vtf_write::encode(&img, VtfOpts { max_size: 2048, ..Default::default() })));
        let mut vmt = format!("\"LightmappedGeneric\"\n{{\n\t\"$basetexture\" \"{material}\"\n");
        let alpha = img.has_alpha();
        if let Some(sh) = &sh {
            if alpha && sh.alphafunc {
                vmt.push_str("\t\"$alphatest\" \"1\"\n");
            } else if alpha && (sh.blend || sh.parms.iter().any(|s| s == "trans")) {
                vmt.push_str("\t\"$translucent\" \"1\"\n");
            }
            if sh.parms.iter().any(|s| s == "metalsteps") {
                vmt.push_str("\t\"$surfaceprop\" \"metal\"\n");
            }
        }
        vmt.push_str(&format!("\t// Quake-3-Textur \"{key}\" ({label}) – von Amboss umgewandelt\n}}\n"));
        self.files.push((format!("{material}.vmt"), vmt.into_bytes()));
        self.converted += 1;
        Some(TexRes { material: material.to_ascii_uppercase(), w: img.w as f64, h: img.h as f64, vw: vw as f64, vh: vh as f64, kind: Kind::Normal })
    }

    /// Hinweise für den Bericht
    pub fn notes(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.has_data() {
            v.push("Keine Quake-3-Daten (pk3/textures) gefunden – Texturen behalten ihre Namen; Ordner im Dialog angeben".into());
        } else if !self.missing.is_empty() {
            let mut m = self.missing.clone();
            m.sort();
            let shown: Vec<&str> = m.iter().take(12).map(|s| s.as_str()).collect();
            let more = if m.len() > 12 { format!(" … (+{})", m.len() - 12) } else { String::new() };
            v.push(format!("{} Texturen nicht gefunden: {}{more}", m.len(), shown.join(", ")));
        }
        v
    }
}

fn shader_tokens(text: &str) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if ch == '{' || ch == '}' {
            out.push(ch.to_string());
            i += 1;
        } else if ch == '"' {
            i += 1;
            let mut s = String::new();
            while i < c.len() && c[i] != '"' {
                s.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(s);
        } else {
            let mut s = String::new();
            while i < c.len() && !c[i].is_whitespace() && c[i] != '{' && c[i] != '}' {
                s.push(c[i]);
                i += 1;
            }
            out.push(s);
        }
    }
    out
}

// ------------------------------------------------------------------ Texturachsen

/// Achsen aus drei Punkten mit Texturkoordinaten (normiert, 1 = einmal die Textur) bestimmen
pub fn fit_axes(p: [DVec3; 3], st: [DVec2; 3], normal: DVec3, w: f64, h: f64) -> (TexAxis, TexAxis) {
    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
    let (a, b, c) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = a * c - b * b;
    let fallback = default_tex_axes(normal, 0.25);
    if det.abs() < 1e-9 {
        return fallback;
    }
    let solve = |d1: f64, d2: f64| {
        let al = (d1 * c - d2 * b) / det;
        let be = (d2 * a - d1 * b) / det;
        e1 * al + e2 * be
    };
    let axis = |grad: DVec3, origin_val: f64, size: f64, fb: TexAxis| {
        let u = grad * size;
        let len = u.length();
        if len < 1e-9 {
            return fb;
        }
        let c0 = origin_val * size - u.dot(p[0]);
        TexAxis { axis: u / len, scale: 1.0 / len, shift: c0 }
    };
    let gs = solve(st[1].x - st[0].x, st[2].x - st[0].x);
    let gt = solve(st[1].y - st[0].y, st[2].y - st[0].y);
    (axis(gs, st[0].x, w, fallback.0), axis(gt, st[0].y, h, fallback.1))
}

/// Achsen-Basis der „Brush Primitives“ (Q3Radiant ComputeAxisBase)
fn bp_base(n: DVec3) -> (DVec3, DVec3) {
    let n = DVec3::new(if n.x.abs() < 1e-6 { 0.0 } else { n.x }, if n.y.abs() < 1e-6 { 0.0 } else { n.y }, if n.z.abs() < 1e-6 { 0.0 } else { n.z });
    let rot_y = -n.z.atan2((n.y * n.y + n.x * n.x).sqrt());
    let rot_z = n.y.atan2(n.x);
    let s = DVec3::new(-rot_z.sin(), rot_z.cos(), 0.0);
    let t = DVec3::new(-rot_y.sin() * rot_z.cos(), -rot_y.sin() * rot_z.sin(), -rot_y.cos());
    (s, t)
}

/// brushDef-Texturmatrix ( ( a b c ) ( d e f ) ) → Source-Achsen
pub fn bp_axes(normal: DVec3, m: [[f64; 3]; 2], w: f64, h: f64) -> (TexAxis, TexAxis) {
    let (bs, bt) = bp_base(normal);
    let one = |r: [f64; 3], size: f64, fb: TexAxis| {
        let u = (bs * r[0] + bt * r[1]) * size;
        let len = u.length();
        if len < 1e-9 {
            return fb;
        }
        TexAxis { axis: u / len, scale: 1.0 / len, shift: r[2] * size }
    };
    let fb = default_tex_axes(normal, 0.25);
    (one(m[0], w, fb.0), one(m[1], h, fb.1))
}

// ------------------------------------------------------------------ Patches

/// Bezier-Patch aus Quake 3: Raster w×h Kontrollpunkte (ungerade), quadratische Teilstücke
#[derive(Clone, Debug)]
pub struct Patch {
    pub texture: String,
    pub w: usize,
    pub h: usize,
    /// `pts[y * w + x]`: Punkt und Texturkoordinate
    pub pts: Vec<(DVec3, DVec2)>,
}

fn bern2(t: f64) -> [f64; 3] {
    let s = 1.0 - t;
    [s * s, 2.0 * s * t, t * t]
}

impl Patch {
    fn at(&self, x: usize, y: usize) -> (DVec3, DVec2) {
        self.pts[y.min(self.h - 1) * self.w + x.min(self.w - 1)]
    }

    /// Punkt und Texturkoordinate bei (u, v) ∈ [0, 1]²
    pub fn eval(&self, u: f64, v: f64) -> (DVec3, DVec2) {
        let (nu, nv) = ((self.w - 1) / 2, (self.h - 1) / 2);
        let (fu, fv) = (u.clamp(0.0, 1.0) * nu as f64, v.clamp(0.0, 1.0) * nv as f64);
        let (iu, iv) = ((fu.floor() as usize).min(nu - 1), (fv.floor() as usize).min(nv - 1));
        let (bu, bv) = (bern2(fu - iu as f64), bern2(fv - iv as f64));
        let (mut p, mut t) = (DVec3::ZERO, DVec2::ZERO);
        for (r, wr) in bv.iter().enumerate() {
            for (k, wk) in bu.iter().enumerate() {
                let (cp, ct) = self.at(iu * 2 + k, iv * 2 + r);
                p += cp * (wr * wk);
                t += ct * (wr * wk);
            }
        }
        (p, t)
    }

    fn valid(&self) -> bool {
        self.w >= 3 && self.h >= 3 && self.w % 2 == 1 && self.h % 2 == 1 && self.pts.len() == self.w * self.h
    }

    /// Teil-Patch (Ausschnitt des Kontrollrasters)
    fn sub(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Patch {
        let mut pts = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                pts.push(self.at(x, y));
            }
        }
        Patch { texture: self.texture.clone(), w: x1 - x0 + 1, h: y1 - y0 + 1, pts }
    }
}

/// Patch → Displacement-Brushes. Ist die Grundfläche (vier Ecken) entartet, wird in Teilstücke
/// zerlegt. `None` für Teile, die sich gar nicht abbilden lassen.
pub fn patch_solids(map: &mut Map, p: &Patch, tex: &TexRes) -> (Vec<Solid>, usize) {
    if !p.valid() {
        return (Vec::new(), 1);
    }
    if let Some(s) = patch_solid(map, p, tex) {
        return (vec![s], 0);
    }
    let (mut out, mut bad) = (Vec::new(), 0);
    if p.w == 3 && p.h == 3 {
        return (out, 1);
    }
    // in der Mitte teilen (an Teilstück-Grenzen)
    let parts = if p.w >= p.h {
        let m = ((p.w - 1) / 4) * 2;
        let m = if m == 0 { 2 } else { m };
        [p.sub(0, 0, m, p.h - 1), p.sub(m, 0, p.w - 1, p.h - 1)]
    } else {
        let m = ((p.h - 1) / 4) * 2;
        let m = if m == 0 { 2 } else { m };
        [p.sub(0, 0, p.w - 1, m), p.sub(0, m, p.w - 1, p.h - 1)]
    };
    for q in parts {
        let (s, b) = patch_solids(map, &q, tex);
        out.extend(s);
        bad += b;
    }
    (out, bad)
}

fn patch_solid(map: &mut Map, p: &Patch, tex: &TexRes) -> Option<Solid> {
    let corners = [p.at(0, 0).0, p.at(p.w - 1, 0).0, p.at(p.w - 1, p.h - 1).0, p.at(0, p.h - 1).0];
    let params = [DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(0.0, 1.0)];
    // Newell-Normale: Vorderseite wie in Quake 3 (Kreuzprodukt Breite × Höhe)
    let mut n = DVec3::ZERO;
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    let area = n.length() * 0.5;
    if area < 4.0 {
        return None;
    }
    let n = n.normalize();
    let c = corners.iter().fold(DVec3::ZERO, |a, b| a + *b) / 4.0;
    let d = n.dot(c);
    let quad: Vec<DVec3> = corners.iter().map(|q| *q - n * (n.dot(*q) - d)).collect();
    // Grundfläche muss ein konvexes Viereck sein
    for k in 0..4 {
        let (a, b, e) = (quad[k], quad[(k + 1) % 4], quad[(k + 2) % 4]);
        if (b - a).cross(e - b).dot(n) <= 1e-6 {
            return None;
        }
    }
    let mut planes = vec![Plane { normal: n, dist: d }, Plane { normal: -n, dist: -(d - 4.0) }];
    for k in 0..4 {
        let (a, b) = (quad[k], quad[(k + 1) % 4]);
        let mut sn = (b - a).cross(n).normalize_or_zero();
        if sn.dot(a - c) < 0.0 {
            sn = -sn;
        }
        planes.push(Plane { normal: sn, dist: sn.dot(a) });
    }
    let mut sides: Vec<Side> = planes.iter().map(|pl| map.side_from_plane(*pl, "TOOLS/TOOLSNODRAW", 0.25)).collect();
    // Texturachsen aus drei Ecken und ihren st-Werten
    let st = [p.at(0, 0).1, p.at(p.w - 1, 0).1, p.at(0, p.h - 1).1];
    let (u, v) = fit_axes([quad[0], quad[1], quad[3]], st, n, tex.vw, tex.vh);
    sides[0].material = tex.material.clone();
    sides[0].uaxis = u;
    sides[0].vaxis = v;
    let id = map.alloc_id();
    let mut s = Solid::new(id, sides);
    s.cleanup();
    let top = s.sides.iter().position(|sd| sd.plane().is_some_and(|q| q.normal.dot(n) > 0.9999))?;
    if s.polys[top].len() != 4 {
        return None;
    }
    let nsub = ((p.w - 1) / 2).max((p.h - 1) / 2);
    let power = if nsub >= 4 { 4 } else { 3 };
    let mut disp = Disp::new_flat(power, &s.polys[top], n)?;
    let g = disp.geometry(&s.polys[top], n)?;
    let r = g.res;
    // Ecken des Rasters den Patch-Ecken zuordnen
    let near = |q: DVec3| (0..4).min_by(|&a, &b| quad[a].distance(q).partial_cmp(&quad[b].distance(q)).unwrap()).unwrap();
    let (c00, c0r, cr0) = (params[near(g.base[0])], params[near(g.base[r - 1])], params[near(g.base[(r - 1) * r])]);
    for i in 0..r {
        for j in 0..r {
            let k = i * r + j;
            let (ti, tj) = (i as f64 / (r - 1) as f64, j as f64 / (r - 1) as f64);
            let uv = c00 + (c0r - c00) * tj + (cr0 - c00) * ti;
            let (pos, _) = p.eval(uv.x, uv.y);
            let fixed = disp.offsets.get(k).copied().unwrap_or(DVec3::ZERO) + n * disp.elevation;
            disp.set_vector(k, pos - g.base[k] - fixed, n);
        }
    }
    s.sides[top].disp = Some(disp);
    s.update_geometry();
    Some(s)
}

// ------------------------------------------------------------------ Entities

/// Quake-3-Entity → Source (Klasse und Schlüssel); `brush` = hat Brushes
pub fn convert_entity(kv: &[(String, String)], brush: bool) -> (String, Vec<(String, String)>) {
    let get = |k: &str| kv.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
    let class = get("classname").unwrap_or_default();
    let mut new_class = match class.as_str() {
        "misc_teleporter_dest" | "target_position" if !brush => "info_teleport_destination".to_string(),
        "info_player_intermission" => "info_target".to_string(),
        "func_static" if brush => "func_brush".to_string(),
        c => c.to_string(),
    };
    let mut out = Vec::new();
    let is_light = class == "light";
    for (k, v) in kv {
        let kl = k.to_ascii_lowercase();
        match kl.as_str() {
            "classname" | "model" if kl == "classname" || v.starts_with('*') => {}
            "angle" if brush => out.push(("movedir".into(), format!("0 {} 0", v.trim()))),
            "angle" => out.push(("angles".into(), match v.trim() {
                "-1" => "-90 0 0".into(),
                "-2" => "90 0 0".into(),
                y => format!("0 {y} 0"),
            })),
            "light" | "_light" if is_light => {}
            "_color" | "color" if is_light => {}
            "_style" if is_light => out.push(("style".into(), v.clone())),
            _ => out.push((k.clone(), v.clone())),
        }
    }
    if is_light {
        let i: f64 = get("light").or_else(|| get("_light")).and_then(|s| s.trim().parse().ok()).unwrap_or(300.0);
        let col: Vec<f64> = get("_color").or_else(|| get("color")).map(|s| s.split_whitespace().filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
        let (r, g, b) = if col.len() == 3 {
            let m = col.iter().cloned().fold(0.0, f64::max).max(1e-6);
            let f = if m > 1.0 { 255.0 / m } else { 255.0 };
            (col[0] * f, col[1] * f, col[2] * f)
        } else {
            (255.0, 255.0, 255.0)
        };
        out.push(("_light".into(), format!("{} {} {} {}", r.round(), g.round(), b.round(), i.round())));
        if get("target").is_some() {
            new_class = "light_spot".into();
        }
    }
    (new_class, out)
}

/// Klassen aus Quake 3 ohne Gegenstück in Source
pub fn foreign_class(c: &str) -> bool {
    c.starts_with("weapon_") || c.starts_with("item_") || c.starts_with("ammo_") || c.starts_with("holdable_") || c.starts_with("team_") || c == "misc_model" || c.starts_with("target_") || c == "misc_portal_surface"
}

// ------------------------------------------------------------------ IBSP (kompilierte Maps)

const Q_ENTITIES: usize = 0;
const Q_SHADERS: usize = 1;
const Q_PLANES: usize = 2;
const Q_MODELS: usize = 7;
const Q_BRUSHES: usize = 8;
const Q_BRUSHSIDES: usize = 9;
const Q_DRAWVERTS: usize = 10;
const Q_DRAWINDEXES: usize = 11;
const Q_SURFACES: usize = 13;

const CONTENTS_LAVA: u32 = 8;
const CONTENTS_SLIME: u32 = 16;
const CONTENTS_WATER: u32 = 32;
const CONTENTS_FOG: u32 = 64;
const CONTENTS_AREAPORTAL: u32 = 0x8000;
const CONTENTS_PLAYERCLIP: u32 = 0x10000;
const CONTENTS_MONSTERCLIP: u32 = 0x20000;
const CONTENTS_ORIGIN: u32 = 0x100_0000;
const CONTENTS_DETAIL: u32 = 0x800_0000;
const CONTENTS_TRANSLUCENT: u32 = 0x2000_0000;
const CONTENTS_TRIGGER: u32 = 0x4000_0000;
const SURF_SKY: u32 = 0x4;
const SURF_NODRAW: u32 = 0x80;
const SURF_HINT: u32 = 0x100;
const SURF_SKIP: u32 = 0x200;

/// Ist das eine Quake-3-BSP? (IBSP 46, Quake Live 47)
pub fn ibsp_version(b: &[u8]) -> Option<u32> {
    if b.get(0..4) != Some(b"IBSP") || b.len() < 8 + 17 * 8 {
        return None;
    }
    let v = u32le(b, 4) as u32;
    matches!(v, 46 | 47).then_some(v)
}

fn f32a(b: &[u8], o: usize) -> f64 {
    b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()) as f64).unwrap_or(0.0)
}
fn i32a(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn vec3(b: &[u8], o: usize) -> DVec3 {
    DVec3::new(f32a(b, o), f32a(b, o + 4), f32a(b, o + 8))
}

struct Q<'a> {
    b: &'a [u8],
    lumps: Vec<(usize, usize)>,
    planes: Vec<Plane>,
    /// (Name, Oberflächen-Flags, Inhalt)
    shaders: Vec<(String, u32, u32)>,
    /// sichtbare ebene Flächen je Shader: (Normale, Abstand, Dreiecke mit st)
    planar: HashMap<usize, Vec<(DVec3, f64, Vec<[(DVec3, DVec2); 3]>)>>,
}

impl Q<'_> {
    fn lump(&self, i: usize) -> &[u8] {
        let (o, l) = self.lumps[i];
        &self.b[o..o + l]
    }

    fn vert(&self, i: usize) -> (DVec3, DVec2) {
        let v = self.lump(Q_DRAWVERTS);
        let o = i * 44;
        (vec3(v, o), DVec2::new(f32a(v, o + 12), f32a(v, o + 16)))
    }

    fn surfaces(&self) -> usize {
        self.lump(Q_SURFACES).len() / 104
    }

    /// Patch-Fläche → Patch
    fn patch(&self, si: usize) -> Option<(usize, Patch)> {
        let s = self.lump(Q_SURFACES);
        let o = si * 104;
        if i32a(s, o + 8) != 2 {
            return None;
        }
        let shader = i32a(s, o).max(0) as usize;
        let (fv, nv) = (i32a(s, o + 12).max(0) as usize, i32a(s, o + 16).max(0) as usize);
        let (w, h) = (i32a(s, o + 96).max(0) as usize, i32a(s, o + 100).max(0) as usize);
        if w * h != nv || nv == 0 {
            return None;
        }
        let pts = (0..nv).map(|k| self.vert(fv + k)).collect();
        Some((shader, Patch { texture: self.shaders.get(shader).map(|s| s.0.clone()).unwrap_or_default(), w, h, pts }))
    }

    /// Texturachsen einer Brush-Seite aus der passenden sichtbaren Fläche
    fn side_axes(&self, shader: usize, plane: &Plane, poly: &[DVec3], tex: &TexRes) -> Option<(TexAxis, TexAxis)> {
        let list = self.planar.get(&shader)?;
        let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
        let edges: Vec<Plane> = (0..poly.len())
            .filter_map(|k| {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let mut n = (b - a).cross(plane.normal).normalize_or_zero();
                if n == DVec3::ZERO {
                    return None;
                }
                if n.dot(a - c) < 0.0 {
                    n = -n;
                }
                Some(Plane { normal: n, dist: n.dot(a) + 0.1 })
            })
            .collect();
        for (n, d, tris) in list {
            if n.dot(plane.normal) < 0.999 || (d - plane.dist).abs() > 0.5 {
                continue;
            }
            let hit = tris.iter().find(|t| {
                let mut poly: Vec<DVec3> = t.iter().map(|x| x.0).collect();
                for e in &edges {
                    poly = clip_polygon(&poly, e);
                }
                poly.len() >= 3
            });
            if let Some(t) = hit {
                return Some(fit_axes([t[0].0, t[1].0, t[2].0], [t[0].1, t[1].1, t[2].1], plane.normal, tex.vw, tex.vh));
            }
        }
        None
    }
}

/// Kompilierte Quake-3-Map → Map und Bericht (Materialien landen in `assets.files`)
pub fn decompile(b: &[u8], assets: &mut Assets) -> Result<(Map, Report), String> {
    let ver = ibsp_version(b).ok_or("Keine Quake-3-BSP (IBSP 46/47)")?;
    let lumps: Vec<(usize, usize)> = (0..17)
        .map(|i| {
            let (o, l) = (u32le(b, 8 + i * 8), u32le(b, 12 + i * 8));
            if o + l <= b.len() {
                (o, l)
            } else {
                (0, 0)
            }
        })
        .collect();
    let mut q = Q { b, lumps, planes: Vec::new(), shaders: Vec::new(), planar: HashMap::new() };
    let pl = q.lump(Q_PLANES);
    q.planes = (0..pl.len() / 16).map(|i| Plane { normal: vec3(pl, i * 16), dist: f32a(pl, i * 16 + 12) }).collect();
    let sl = q.lump(Q_SHADERS);
    q.shaders = (0..sl.len() / 72)
        .map(|i| {
            let o = i * 72;
            let raw = &sl[o..o + 64];
            let end = raw.iter().position(|&c| c == 0).unwrap_or(64);
            (String::from_utf8_lossy(&raw[..end]).to_string(), i32a(sl, o + 64) as u32, i32a(sl, o + 68) as u32)
        })
        .collect();
    // ebene Flächen sammeln (für die Texturachsen der Brush-Seiten)
    let idx = q.lump(Q_DRAWINDEXES);
    let mut planar: HashMap<usize, Vec<(DVec3, f64, Vec<[(DVec3, DVec2); 3]>)>> = HashMap::new();
    for si in 0..q.surfaces() {
        let s = q.lump(Q_SURFACES);
        let o = si * 104;
        if i32a(s, o + 8) != 1 {
            continue;
        }
        let shader = i32a(s, o).max(0) as usize;
        let (fv, fi, ni) = (i32a(s, o + 12).max(0) as usize, i32a(s, o + 20).max(0) as usize, i32a(s, o + 24).max(0) as usize);
        let n = vec3(s, o + 84);
        let mut tris = Vec::new();
        for t in 0..ni / 3 {
            let k = |m: usize| i32a(idx, (fi + t * 3 + m) * 4).max(0) as usize;
            tris.push([q.vert(fv + k(0)), q.vert(fv + k(1)), q.vert(fv + k(2))]);
        }
        if let Some(t) = tris.first() {
            let d = n.dot(t[0].0);
            planar.entry(shader).or_default().push((n, d, tris));
        }
    }
    q.planar = planar;

    let mut r = Report { version: ver, ..Default::default() };
    let mut map = Map::new();
    let models = q.lump(Q_MODELS).to_vec();
    let model = |i: usize| -> Option<(usize, usize, usize, usize)> {
        let o = i * 40;
        (o + 40 <= models.len()).then(|| (i32a(&models, o + 24).max(0) as usize, i32a(&models, o + 28).max(0) as usize, i32a(&models, o + 32).max(0) as usize, i32a(&models, o + 36).max(0) as usize))
    };
    let mut fog = 0;
    let mut bad_patches = 0;
    let mut patches = 0;

    // Brushes eines Modells; Ergebnis: (Solid, Inhalt)
    let brushes_of = |map: &mut Map, assets: &mut Assets, first: usize, count: usize, offset: DVec3, fog: &mut usize| -> Vec<(Solid, u32)> {
        let (bl, sl) = (q.lump(Q_BRUSHES), q.lump(Q_BRUSHSIDES));
        let mut out = Vec::new();
        for bi in first..first + count {
            let o = bi * 12;
            if o + 12 > bl.len() {
                break;
            }
            let (fs, ns, sh) = (i32a(bl, o).max(0) as usize, i32a(bl, o + 4).max(0) as usize, i32a(bl, o + 8).max(0) as usize);
            let contents = q.shaders.get(sh).map(|s| s.2).unwrap_or(1);
            if contents & CONTENTS_FOG != 0 {
                *fog += 1;
                continue;
            }
            if contents & CONTENTS_ORIGIN != 0 {
                continue;
            }
            let liquid = contents & (CONTENTS_WATER | CONTENTS_SLIME | CONTENTS_LAVA) != 0;
            let mut planes = Vec::new();
            let mut shs = Vec::new();
            for k in fs..fs + ns {
                let so = k * 8;
                let Some(p) = q.planes.get(i32a(sl, so).max(0) as usize).copied() else { continue };
                planes.push(Some(p));
                shs.push(i32a(sl, so + 4).max(0) as usize);
            }
            let polys = polygons_from_planes(&planes);
            let mut sides = Vec::new();
            for ((p, shi), poly) in planes.iter().zip(&shs).zip(&polys) {
                let Some(p) = p else { continue };
                if poly.len() < 3 {
                    continue;
                }
                let mut pp = *p;
                pp.dist += pp.normal.dot(offset);
                let mut side = map.side_from_plane(pp, "TOOLS/TOOLSNODRAW", 0.25);
                let (name, sflags, _) = q.shaders.get(*shi).cloned().unwrap_or_default();
                let mat = if liquid {
                    if p.normal.z > 0.7 {
                        TexRes::plain(crate::goldsrc::WATER_MATERIAL)
                    } else {
                        TexRes::plain("TOOLS/TOOLSNODRAW")
                    }
                } else if contents & CONTENTS_TRIGGER != 0 {
                    TexRes::plain("TOOLS/TOOLSTRIGGER")
                } else if contents & CONTENTS_AREAPORTAL != 0 {
                    TexRes::plain("TOOLS/TOOLSAREAPORTAL")
                } else if contents & CONTENTS_PLAYERCLIP != 0 && contents & 1 == 0 {
                    TexRes::plain(if contents & CONTENTS_MONSTERCLIP != 0 { "TOOLS/TOOLSCLIP" } else { "TOOLS/TOOLSPLAYERCLIP" })
                } else if sflags & SURF_SKY != 0 {
                    TexRes::plain("TOOLS/TOOLSSKYBOX")
                } else if sflags & SURF_HINT != 0 {
                    TexRes::plain("TOOLS/TOOLSHINT")
                } else if sflags & SURF_SKIP != 0 {
                    TexRes::plain("TOOLS/TOOLSSKIP")
                } else if sflags & SURF_NODRAW != 0 {
                    TexRes::plain("TOOLS/TOOLSNODRAW")
                } else {
                    assets.resolve(&name)
                };
                side.material = mat.material.clone();
                if mat.kind == Kind::Normal && !mat.material.starts_with("TOOLS/") {
                    if let Some((u, v)) = q.side_axes(*shi, p, poly, &mat) {
                        let sh = |a: &TexAxis| TexAxis { axis: a.axis, scale: a.scale, shift: a.shift - offset.dot(a.axis) / a.scale };
                        side.uaxis = sh(&u);
                        side.vaxis = sh(&v);
                    }
                }
                sides.push(side);
            }
            let id = map.alloc_id();
            let mut s = Solid::new(id, sides);
            s.cleanup();
            if s.is_valid() {
                out.push((s, contents));
            }
        }
        out
    };

    // Entities
    let text = String::from_utf8_lossy(q.lump(Q_ENTITIES)).to_string();
    let mut foreign: Vec<String> = Vec::new();
    for kv in parse_entities(&text) {
        let class = kv.iter().find(|(k, _)| k == "classname").map(|(_, v)| v.clone()).unwrap_or_default();
        if class == "worldspawn" {
            for (k, v) in &kv {
                if !matches!(k.to_ascii_lowercase().as_str(), "classname" | "_blocksize" | "gridsize" | "_ambient" | "ambient" | "_color" | "_minlight") {
                    map.world.set(k, v.clone());
                }
            }
            continue;
        }
        let mi = kv.iter().find(|(k, v)| k == "model" && v.starts_with('*')).and_then(|(_, v)| v[1..].parse::<usize>().ok());
        let (nc, props) = convert_entity(&kv, mi.is_some());
        if foreign_class(&class) && !foreign.contains(&class) {
            foreign.push(class.clone());
        }
        let id = map.alloc_id();
        let mut e = Entity::new(id, &nc);
        for (k, v) in props {
            e.set(&k, v);
        }
        if let Some((_, _, fb, nb)) = mi.and_then(model) {
            let off = e.get("origin").and_then(crate::map::parse_vec).unwrap_or(DVec3::ZERO);
            for (s, _) in brushes_of(&mut map, assets, fb, nb, off, &mut fog) {
                e.solids.push(s.into());
            }
            if !e.solids.is_empty() {
                r.brush_entities += 1;
                r.brushes += e.solids.len();
            }
        }
        r.entities += 1;
        map.entities.push(e.into());
    }
    if !foreign.is_empty() {
        foreign.sort();
        r.notes.push(format!("Klassen ohne Source-Gegenstück (bleiben erhalten, wirken im Spiel nicht): {}", foreign.join(", ")));
    }
    // Welt
    let (fsurf, nsurf, fb, nb) = model(0).ok_or("BSP ohne Welt-Modell")?;
    for (s, contents) in brushes_of(&mut map, assets, fb, nb, DVec3::ZERO, &mut fog) {
        r.brushes += 1;
        if contents & (CONTENTS_DETAIL | CONTENTS_TRANSLUCENT) != 0 && contents & (CONTENTS_WATER | CONTENTS_SLIME | CONTENTS_LAVA) == 0 {
            let id = map.alloc_id();
            let mut e = Entity::new(id, "func_detail");
            e.solids.push(s.into());
            map.entities.push(e.into());
            r.detail += 1;
        } else {
            map.world.solids.push(s.into());
        }
    }
    // Patches der Welt
    for si in fsurf..fsurf + nsurf {
        let Some((_, p)) = q.patch(si) else { continue };
        let tex = assets.resolve(&p.texture);
        let (solids, bad) = patch_solids(&mut map, &p, &tex);
        bad_patches += bad;
        for s in solids {
            map.world.solids.push(s.into());
            r.displacements += 1;
        }
        patches += 1;
    }
    if patches > 0 {
        r.notes.push(format!("{patches} gebogene Flächen (Patches) → {} Displacements", r.displacements));
    }
    if bad_patches > 0 {
        r.notes.push(format!("{bad_patches} Patch-Teile ließen sich nicht als Displacement abbilden (z. B. Dreiecksform)"));
    }
    if fog > 0 {
        r.notes.push(format!("{fog} Nebel-Volumen ausgelassen (in Source: env_fog_controller)"));
    }
    r.textures = assets.converted;
    r.notes.extend(assets.notes());
    r.notes.push("Maßstab 1:1 übernommen – Quake-3-Spieler sind kleiner (56 statt 72 Einheiten); bei Bedarf mit Transformieren auf 125 % skalieren".into());
    Ok((map, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_patch(w: usize, h: usize, bend: f64) -> Patch {
        let mut pts = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f64 / (w - 1) as f64, y as f64 / (h - 1) as f64);
                // Bogen: mittlere Kontrollzeile angehoben
                let z = if y == h / 2 { bend } else { 0.0 };
                pts.push((DVec3::new(fx * 128.0, fy * 128.0, z), DVec2::new(fx, fy)));
            }
        }
        Patch { texture: "base_wall/test".into(), w, h, pts }
    }

    #[test]
    fn patch_to_displacement() {
        let mut map = Map::new();
        let p = flat_patch(3, 3, 64.0);
        // Mitte der Fläche: quadratisch → halbe Höhe des Kontrollpunkts
        assert!((p.eval(0.5, 0.5).0.z - 32.0).abs() < 1e-9);
        let (solids, bad) = patch_solids(&mut map, &p, &TexRes::plain("QUAKE3/BASE_WALL/TEST"));
        assert_eq!((solids.len(), bad), (1, 0));
        let s = &solids[0];
        let top = s.sides.iter().position(|sd| sd.disp.is_some()).unwrap();
        assert!(s.sides[top].plane().unwrap().normal.z > 0.99, "Vorderseite zeigt nach oben");
        assert_eq!(s.sides[top].material, "QUAKE3/BASE_WALL/TEST");
        let g = s.disps[top].as_ref().unwrap();
        let r = g.res;
        let mid = g.pos[(r / 2) * r + r / 2];
        assert!((mid.z - 32.0).abs() < 1e-6 && (mid.x - 64.0).abs() < 1e-6, "{mid:?}");
        // Ränder bleiben auf den Kontrollpunkten
        assert!(g.pos[0].z.abs() < 1e-6);
        // Textur: Breite der Fläche = 1 Wiederholung bei 128er Textur → Maßstab 1
        let u = &s.sides[top].uaxis;
        assert!((u.scale.abs() - 1.0).abs() < 1e-6, "{u:?}");
        // Breiter Patch (5×3) passt ebenfalls in ein Displacement
        let (s2, _) = patch_solids(&mut map, &flat_patch(5, 3, 10.0), &TexRes::plain("X"));
        assert_eq!(s2.len(), 1);
        // entartet (alle Punkte auf einer Linie) → nichts
        let mut line = flat_patch(3, 3, 0.0);
        for q in &mut line.pts {
            q.0.y = 0.0;
        }
        assert_eq!(patch_solids(&mut map, &line, &TexRes::plain("X")).0.len(), 0);
        // Wieder lesbar als VMF
        map.world.solids.push(solids[0].clone().into());
        let back = Map::from_vmf(&map.to_vmf()).unwrap();
        assert!(back.world.solids[0].has_disp());
    }

    #[test]
    fn axes_and_specials() {
        // st = x/128, y/128 auf einer Bodenfläche, Textur 256 → Maßstab 0.5
        let p = [DVec3::ZERO, DVec3::new(128.0, 0.0, 0.0), DVec3::new(0.0, 128.0, 0.0)];
        let st = [DVec2::ZERO, DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0)];
        let (u, v) = fit_axes(p, st, DVec3::Z, 256.0, 256.0);
        assert!((u.axis - DVec3::X).length() < 1e-9 && (u.scale - 0.5).abs() < 1e-9 && u.shift.abs() < 1e-9);
        assert!((v.axis - DVec3::Y).length() < 1e-9);
        // brushDef: Boden, Einheitsmatrix → 1 Textur je Einheit … mal Größe
        let (bu, _) = bp_axes(DVec3::Z, [[1.0 / 128.0, 0.0, 0.5], [0.0, 1.0 / 128.0, 0.0]], 128.0, 128.0);
        assert!((bu.scale - 1.0).abs() < 1e-9 && (bu.shift - 64.0).abs() < 1e-9);
        assert_eq!(special("common/caulk").unwrap().material, "TOOLS/TOOLSNODRAW");
        assert_eq!(special("textures/skies/hellsky").unwrap().material, "TOOLS/TOOLSSKYBOX");
        assert_eq!(special("liquids/clear_calm1").unwrap().kind, Kind::Water);
        assert_eq!(special("sfx/fog_timdm1").unwrap().kind, Kind::Fog);
        assert!(special("base_wall/concrete").is_none());
        // Licht
        let (c, kv) = convert_entity(&[("classname".into(), "light".into()), ("light".into(), "400".into()), ("_color".into(), "1 0.5 0".into())], false);
        assert_eq!(c, "light");
        assert!(kv.contains(&("_light".into(), "255 128 0 400".into())), "{kv:?}");
    }

    /// Kleine IBSP: Boden-Brush (Fläche oben mit st), Wasser-Brush, ein Patch, ein Spawn
    fn test_ibsp() -> Vec<u8> {
        let mut l: Vec<Vec<u8>> = vec![Vec::new(); 17];
        let f = |v: &mut Vec<u8>, x: f32| v.extend_from_slice(&x.to_le_bytes());
        let i = |v: &mut Vec<u8>, x: i32| v.extend_from_slice(&x.to_le_bytes());
        l[Q_ENTITIES] = b"{\n\"classname\" \"worldspawn\"\n\"message\" \"Test\"\n}\n{\n\"classname\" \"info_player_deathmatch\"\n\"origin\" \"64 64 24\"\n\"angle\" \"90\"\n}\n{\n\"classname\" \"weapon_rocketlauncher\"\n\"origin\" \"0 0 16\"\n}\n\0".to_vec();
        for (name, sf, cf) in [("textures/base_floor/test", 0u32, 1u32), ("textures/liquids/water", 0, CONTENTS_WATER), ("textures/common/caulk", SURF_NODRAW, 1)] {
            let mut n = name.as_bytes().to_vec();
            n.resize(64, 0);
            l[Q_SHADERS].extend(n);
            i(&mut l[Q_SHADERS], sf as i32);
            i(&mut l[Q_SHADERS], cf as i32);
        }
        // Ebenen: Quader 0..128 × 0..128 × −16..0 und Wasser 0..128 × 0..128 × 0..32
        let mut planes = Vec::new();
        for (lo, hi) in [(DVec3::new(0.0, 0.0, -16.0), DVec3::new(128.0, 128.0, 0.0)), (DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 128.0, 32.0))] {
            for (n, d) in [(DVec3::Z, hi.z), (-DVec3::Z, -lo.z), (DVec3::X, hi.x), (-DVec3::X, -lo.x), (DVec3::Y, hi.y), (-DVec3::Y, -lo.y)] {
                planes.push((n, d));
            }
        }
        for (n, d) in &planes {
            for c in n.to_array() {
                f(&mut l[Q_PLANES], c as f32);
            }
            f(&mut l[Q_PLANES], *d as f32);
        }
        // Brushes: 0 = Boden (Oberseite base_floor, Rest caulk), 1 = Wasser
        for (first, sh) in [(0, 0), (6, 1)] {
            i(&mut l[Q_BRUSHES], first);
            i(&mut l[Q_BRUSHES], 6);
            i(&mut l[Q_BRUSHES], sh);
        }
        for k in 0..12 {
            i(&mut l[Q_BRUSHSIDES], k);
            i(&mut l[Q_BRUSHSIDES], if k == 0 { 0 } else if k < 6 { 2 } else { 1 });
        }
        // Zeichenpunkte: Bodenfläche (4), dann Patch 3×3
        let mut verts: Vec<(DVec3, DVec2)> = vec![(DVec3::new(0.0, 0.0, 0.0), DVec2::new(0.0, 0.0)), (DVec3::new(128.0, 0.0, 0.0), DVec2::new(0.5, 0.0)), (DVec3::new(128.0, 128.0, 0.0), DVec2::new(0.5, 0.5)), (DVec3::new(0.0, 128.0, 0.0), DVec2::new(0.0, 0.5))];
        for y in 0..3 {
            for x in 0..3 {
                verts.push((DVec3::new(200.0 + x as f64 * 32.0, y as f64 * 32.0, if x == 1 { 48.0 } else { 0.0 }), DVec2::new(x as f64 * 0.5, y as f64 * 0.5)));
            }
        }
        for (p, st) in &verts {
            for c in p.to_array() {
                f(&mut l[Q_DRAWVERTS], c as f32);
            }
            f(&mut l[Q_DRAWVERTS], st.x as f32);
            f(&mut l[Q_DRAWVERTS], st.y as f32);
            let n = l[Q_DRAWVERTS].len() + 24;
            l[Q_DRAWVERTS].resize(n, 0);
        }
        for k in [0, 1, 2, 0, 2, 3] {
            i(&mut l[Q_DRAWINDEXES], k);
        }
        // Flächen: 0 = eben (Boden oben), 1 = Patch
        let surf = |sh: i32, typ: i32, fv: i32, nv: i32, fi: i32, ni: i32, n: DVec3, pw: i32, ph: i32, out: &mut Vec<u8>| {
            let mut s = Vec::new();
            for x in [sh, -1, typ, fv, nv, fi, ni] {
                s.extend_from_slice(&x.to_le_bytes());
            }
            s.resize(84, 0);
            for c in n.to_array() {
                s.extend_from_slice(&(c as f32).to_le_bytes());
            }
            s.extend_from_slice(&pw.to_le_bytes());
            s.extend_from_slice(&ph.to_le_bytes());
            out.extend(s);
        };
        surf(0, 1, 0, 4, 0, 6, DVec3::Z, 0, 0, &mut l[Q_SURFACES]);
        surf(0, 2, 4, 9, 0, 0, DVec3::ZERO, 3, 3, &mut l[Q_SURFACES]);
        // Modell 0: Flächen 0..2, Brushes 0..2
        for x in [0.0f32; 6] {
            f(&mut l[Q_MODELS], x);
        }
        for x in [0, 2, 0, 2] {
            i(&mut l[Q_MODELS], x);
        }
        let mut b = b"IBSP".to_vec();
        b.extend_from_slice(&46u32.to_le_bytes());
        b.resize(8 + 17 * 8, 0);
        for k in 0..17 {
            let ofs = b.len();
            b.extend_from_slice(&l[k]);
            b[8 + k * 8..12 + k * 8].copy_from_slice(&(ofs as u32).to_le_bytes());
            b[12 + k * 8..16 + k * 8].copy_from_slice(&(l[k].len() as u32).to_le_bytes());
        }
        b
    }

    #[test]
    fn decompile_ibsp() {
        let mut assets = Assets::default();
        let (map, r) = decompile(&test_ibsp(), &mut assets).unwrap();
        assert_eq!(r.version, 46);
        assert_eq!(map.world.get("message"), Some("Test"));
        let floor = map.world.solids.iter().find(|s| s.bounds.max.z == 0.0 && !s.has_disp()).expect("Boden");
        let top = floor.sides.iter().find(|s| s.plane().is_some_and(|p| p.normal.z > 0.99)).unwrap();
        // ohne Spieldaten: Name bleibt erkennbar, Achsen aus den st-Werten (0.5 je 128 → Maßstab 2 bei 128er Textur)
        assert_eq!(top.material, "QUAKE3/FEHLT/BASE_FLOOR/TEST");
        assert!((top.uaxis.scale - 2.0).abs() < 1e-6, "{:?}", top.uaxis);
        assert!(floor.sides.iter().filter(|s| s.material == "TOOLS/TOOLSNODRAW").count() == 5);
        let water = map.world.solids.iter().find(|s| s.sides.iter().any(|sd| sd.material == crate::goldsrc::WATER_MATERIAL)).expect("Wasser");
        assert_eq!(water.bounds.max.z, 32.0);
        assert_eq!(r.displacements, 1);
        assert!(map.world.solids.iter().any(|s| s.has_disp() && s.bounds.max.z > 20.0));
        let sp = map.entities.iter().find(|e| e.classname() == "info_player_deathmatch").unwrap();
        assert_eq!(sp.get("angles"), Some("0 90 0"));
        assert!(r.notes.iter().any(|n| n.contains("weapon_rocketlauncher")));
        assert!(decompile(b"IBSPkaputt", &mut assets).is_err());
    }

    #[test]
    fn shader_and_tga() {
        let mut a = Assets::default();
        a.parse_shaders("// Kommentar\ntextures/base_light/lamp\n{\n qer_editorimage textures/base_light/lamp_ed.tga\n surfaceparm nomarks\n {\n  map $lightmap\n }\n {\n  map textures/base_light/lamp.tga\n  blendFunc GL_DST_COLOR GL_ZERO\n }\n}\ntextures/liquids/slime\n{\n surfaceparm slime\n}\n");
        let s = &a.shaders["textures/base_light/lamp"];
        assert_eq!(s.image.as_deref(), Some("textures/base_light/lamp.tga"));
        assert_eq!(s.editor.as_deref(), Some("textures/base_light/lamp_ed.tga"));
        assert_eq!(a.resolve("liquids/slime").kind, Kind::Water);
        // TGA 2×2 unten-links, BGR
        let mut t = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 24, 0];
        t.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        let img = crate::vtf_write::load_tga(&t).unwrap();
        assert_eq!((img.w, img.h), (2, 2));
        assert_eq!(&img.px[0..4], &[255, 0, 0, 255], "obere Zeile = zweite Datenzeile: rot");
        assert_eq!(&img.px[8..12], &[0, 0, 255, 255], "untere Zeile: blau");
    }
}
