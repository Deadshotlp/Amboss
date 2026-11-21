//! Modelle importieren: glTF/GLB und OBJ (+MTL) lesen, in Source-Koordinaten umrechnen,
//! als SMD/QC mit VertexLitGeneric-Materialien und VTF-Texturen schreiben (studiomdl macht daraus ein MDL).

use crate::geometry::Aabb;
use crate::vtf_write::Rgba;
use glam::{DMat3, DMat4, DVec2, DVec3};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Bildquelle einer Textur
#[derive(Clone, Debug)]
pub enum Image {
    File(PathBuf),
    /// eingebettet (glTF), mit Namen für Meldungen
    Bytes(Vec<u8>, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alpha {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug)]
pub struct Material {
    pub name: String,
    pub image: Option<Image>,
    /// Grundfarbe (multipliziert mit der Textur bzw. allein)
    pub color: [f32; 4],
    /// Farbe ist linear (glTF) statt sRGB (OBJ)
    pub linear: bool,
    pub alpha: Alpha,
    pub double_sided: bool,
}

impl Material {
    fn plain(name: &str) -> Material {
        Material { name: name.to_string(), image: None, color: [0.8, 0.8, 0.8, 1.0], linear: false, alpha: Alpha::Opaque, double_sided: false }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vert {
    pub p: DVec3,
    pub n: Option<DVec3>,
    /// UV mit Ursprung unten links (wie SMD/OBJ)
    pub uv: DVec2,
}

/// Eingelesenes Modell (Koordinaten wie in der Datei)
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub tris: Vec<([Vert; 3], usize)>,
    pub materials: Vec<Material>,
    /// Datei ist sicher Y-oben in Metern (glTF)
    pub gltf: bool,
    /// Teile (OBJ-Objekte/-Gruppen, glTF-Knoten) – für Bodygroups
    pub parts: Vec<String>,
    /// Teil je Dreieck (gleiche Länge wie `tris`; leer = alles Teil 0)
    pub tri_part: Vec<usize>,
}

impl Model {
    /// Index eines Teils (neu anlegen, wenn unbekannt)
    fn part_index(&mut self, name: &str) -> usize {
        match self.parts.iter().position(|p| p == name) {
            Some(i) => i,
            None => {
                self.parts.push(name.to_string());
                self.parts.len() - 1
            }
        }
    }

    pub fn part_of(&self, tri: usize) -> usize {
        self.tri_part.get(tri).copied().unwrap_or(0)
    }

    /// Dreiecke je Teil
    pub fn part_counts(&self) -> Vec<usize> {
        let mut c = vec![0; self.parts.len().max(1)];
        for i in 0..self.tris.len() {
            if let Some(x) = c.get_mut(self.part_of(i)) {
                *x += 1;
            }
        }
        c
    }
}

// ------------------------------------------------------------------ Laden

pub fn load(path: &Path) -> Result<Model, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let m = match ext.as_str() {
        "obj" => parse_obj(&String::from_utf8_lossy(&bytes), dir)?,
        "gltf" | "glb" => parse_gltf(&bytes, dir)?,
        "fbx" | "dae" => return Err("FBX und DAE werden noch nicht unterstützt – bitte als glTF (.glb) oder OBJ exportieren (Blender: Datei → Exportieren)".into()),
        _ => return Err(format!("Unbekanntes Format: .{ext}")),
    };
    if m.tris.is_empty() {
        return Err("Das Modell enthält keine Dreiecke".into());
    }
    Ok(m)
}

/// Dateiendungen, die importiert werden können
pub fn is_model_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| ["obj", "gltf", "glb", "fbx", "dae"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
}

fn parse_obj(src: &str, dir: &Path) -> Result<Model, String> {
    let mut m = Model::default();
    let (mut v, mut vt, mut vn): (Vec<DVec3>, Vec<DVec2>, Vec<DVec3>) = (Vec::new(), Vec::new(), Vec::new());
    let mut mats: Vec<Material> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut cur_part: Option<usize> = None;
    let num = |s: Option<&str>| s.and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    let mut used: Vec<Material> = Vec::new();
    let mat_index = |name: &str, used: &mut Vec<Material>, defs: &[Material]| -> usize {
        if let Some(i) = used.iter().position(|x| x.name == name) {
            return i;
        }
        used.push(defs.iter().find(|x| x.name == name).cloned().unwrap_or_else(|| Material::plain(name)));
        used.len() - 1
    };
    for line in src.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => v.push(DVec3::new(num(it.next()), num(it.next()), num(it.next()))),
            Some("vt") => vt.push(DVec2::new(num(it.next()), num(it.next()))),
            Some("vn") => vn.push(DVec3::new(num(it.next()), num(it.next()), num(it.next()))),
            Some("mtllib") => {
                let file = line["mtllib".len()..].trim();
                if let Ok(t) = std::fs::read_to_string(dir.join(file.replace('\\', "/"))) {
                    mats.extend(parse_mtl(&t, dir));
                }
            }
            Some("usemtl") => {
                let name = line["usemtl".len()..].trim();
                cur = Some(mat_index(name, &mut used, &mats));
            }
            Some("o") | Some("g") => {
                let name = line[1..].trim();
                if !name.is_empty() {
                    cur_part = Some(m.part_index(name));
                }
            }
            Some("f") => {
                let idx = |s: &str, len: usize| -> Option<usize> {
                    let i: i64 = s.parse().ok()?;
                    let r = if i < 0 { len as i64 + i } else { i - 1 };
                    (r >= 0 && (r as usize) < len).then_some(r as usize)
                };
                let mut poly: Vec<Vert> = Vec::new();
                for tok in it {
                    let mut parts = tok.split('/');
                    let Some(pi) = parts.next().and_then(|s| idx(s, v.len())) else { return Err(format!("Ungültige Fläche: {line}")) };
                    let ti = parts.next().filter(|s| !s.is_empty()).and_then(|s| idx(s, vt.len()));
                    let ni = parts.next().filter(|s| !s.is_empty()).and_then(|s| idx(s, vn.len()));
                    poly.push(Vert { p: v[pi], n: ni.map(|i| vn[i]), uv: ti.map(|i| vt[i]).unwrap_or_default() });
                }
                let mi = match cur {
                    Some(i) => i,
                    None => {
                        let i = mat_index("default", &mut used, &mats);
                        cur = Some(i);
                        i
                    }
                };
                let part = match cur_part {
                    Some(p) => p,
                    None => {
                        let p = m.part_index("model");
                        cur_part = Some(p);
                        p
                    }
                };
                for k in 1..poly.len().saturating_sub(1) {
                    m.tris.push(([poly[0], poly[k], poly[k + 1]], mi));
                    m.tri_part.push(part);
                }
            }
            _ => {}
        }
    }
    m.materials = used;
    Ok(m)
}

fn parse_mtl(src: &str, dir: &Path) -> Vec<Material> {
    let mut out: Vec<Material> = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        let mut it = line.split_whitespace();
        let Some(k) = it.next() else { continue };
        if k == "newmtl" {
            out.push(Material::plain(line[6..].trim()));
            continue;
        }
        let Some(m) = out.last_mut() else { continue };
        let nums: Vec<f32> = it.clone().filter_map(|x| x.parse().ok()).collect();
        match k {
            "Kd" if nums.len() >= 3 => {
                m.color[0] = nums[0];
                m.color[1] = nums[1];
                m.color[2] = nums[2];
            }
            "d" if !nums.is_empty() => m.color[3] = nums[0],
            "Tr" if !nums.is_empty() => m.color[3] = 1.0 - nums[0],
            "map_Kd" => {
                // Optionen (-s 1 1 1 …) überspringen: Dateiname = Rest ab dem letzten Token mit Bild-Endung
                let rest = line[6..].trim();
                let toks: Vec<&str> = rest.split_whitespace().collect();
                let file = match toks.iter().rposition(|t| has_image_ext(t)) {
                    Some(i) => {
                        // Leerzeichen im Namen: ab dem ersten Token nach den Optionen
                        let start = toks.iter().position(|t| !t.starts_with('-') && t.parse::<f64>().is_err()).unwrap_or(i).min(i);
                        toks[start..=i].join(" ")
                    }
                    None => toks.last().map(|s| s.to_string()).unwrap_or_default(),
                };
                if !file.is_empty() {
                    m.image = Some(Image::File(dir.join(file.replace('\\', "/"))));
                }
            }
            _ => {}
        }
    }
    for m in &mut out {
        if m.color[3] < 0.99 {
            m.alpha = Alpha::Blend;
        }
    }
    out
}

fn has_image_ext(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".tga", ".bmp", ".dds", ".tif", ".tiff"].iter().any(|e| l.ends_with(e))
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Daten einer URI (data:…;base64 oder Datei relativ zum glTF)
fn uri_bytes(uri: &str, dir: &Path) -> Result<Vec<u8>, String> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let data = rest.split_once(";base64,").map(|x| x.1).ok_or("data-URI ohne base64")?;
        return base64_decode(data).ok_or_else(|| "base64-Daten ungültig".to_string());
    }
    let p = dir.join(percent_decode(uri));
    std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
}

fn parse_gltf(bytes: &[u8], dir: &Path) -> Result<Model, String> {
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| format!("glTF: {e}"))?;
    let doc = &g.document;
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for b in doc.buffers() {
        buffers.push(match b.source() {
            gltf::buffer::Source::Bin => g.blob.clone().ok_or("GLB ohne Binärdaten")?,
            gltf::buffer::Source::Uri(u) => uri_bytes(u, dir)?,
        });
    }
    let mut m = Model { gltf: true, ..Default::default() };
    // Materialien (Index im glTF → Index hier; dazu ein Standardmaterial am Ende)
    for (i, mat) in doc.materials().enumerate() {
        let pbr = mat.pbr_metallic_roughness();
        let image = pbr.base_color_texture().map(|t| t.texture().source()).and_then(|img| match img.source() {
            gltf::image::Source::Uri { uri, .. } => {
                if uri.starts_with("data:") {
                    uri_bytes(uri, dir).ok().map(|b| Image::Bytes(b, format!("Bild {}", img.index())))
                } else {
                    Some(Image::File(dir.join(percent_decode(uri))))
                }
            }
            gltf::image::Source::View { view, .. } => {
                let buf = buffers.get(view.buffer().index())?;
                let s = view.offset();
                buf.get(s..s + view.length()).map(|b| Image::Bytes(b.to_vec(), img.name().unwrap_or("eingebettet").to_string()))
            }
        });
        m.materials.push(Material {
            name: mat.name().map(str::to_string).unwrap_or_else(|| format!("material{i}")),
            image,
            color: pbr.base_color_factor(),
            linear: true,
            alpha: match mat.alpha_mode() {
                gltf::material::AlphaMode::Opaque => Alpha::Opaque,
                gltf::material::AlphaMode::Mask => Alpha::Mask,
                gltf::material::AlphaMode::Blend => Alpha::Blend,
            },
            double_sided: mat.double_sided(),
        });
    }
    let default_mat = m.materials.len();
    m.materials.push(Material { linear: true, ..Material::plain("default") });
    let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) else { return Err("glTF ohne Szene".into()) };
    let mut stack: Vec<(gltf::Node, DMat4)> = scene.nodes().map(|n| (n, DMat4::IDENTITY)).collect();
    while let Some((node, parent)) = stack.pop() {
        let local = DMat4::from_cols_array_2d(&node.transform().matrix().map(|c| c.map(|x| x as f64)));
        let world = parent * local;
        for c in node.children() {
            stack.push((c, world));
        }
        let Some(mesh) = node.mesh() else { continue };
        let part = m.part_index(&node.name().or(mesh.name()).map(str::to_string).unwrap_or_else(|| format!("teil{}", node.index())));
        let nmat = DMat3::from_mat4(world).inverse().transpose();
        let flip = DMat3::from_mat4(world).determinant() < 0.0;
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|v| v.as_slice()));
            let Some(pos) = reader.read_positions() else { continue };
            let pos: Vec<DVec3> = pos.map(|p| world.transform_point3(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))).collect();
            let nrm: Option<Vec<DVec3>> = reader.read_normals().map(|it| it.map(|n| (nmat * DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64)).normalize_or_zero()).collect());
            // glTF-UV: Ursprung oben links → für SMD umdrehen
            let uv: Option<Vec<DVec2>> = reader.read_tex_coords(0).map(|t| t.into_f32().map(|u| DVec2::new(u[0] as f64, 1.0 - u[1] as f64)).collect());
            let idx: Vec<usize> = match reader.read_indices() {
                Some(i) => i.into_u32().map(|x| x as usize).collect(),
                None => (0..pos.len()).collect(),
            };
            let tri_idx: Vec<[usize; 3]> = match prim.mode() {
                gltf::mesh::Mode::Triangles => idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
                gltf::mesh::Mode::TriangleStrip => (2..idx.len()).map(|k| if k % 2 == 0 { [idx[k - 2], idx[k - 1], idx[k]] } else { [idx[k - 1], idx[k - 2], idx[k]] }).collect(),
                gltf::mesh::Mode::TriangleFan => (2..idx.len()).map(|k| [idx[0], idx[k - 1], idx[k]]).collect(),
                _ => continue, // Punkte und Linien
            };
            let mi = prim.material().index().unwrap_or(default_mat);
            let vert = |i: usize| -> Option<Vert> { Some(Vert { p: *pos.get(i)?, n: nrm.as_ref().and_then(|n| n.get(i).copied()), uv: uv.as_ref().and_then(|u| u.get(i).copied()).unwrap_or_default() }) };
            for t in tri_idx {
                let (Some(a), Some(b), Some(c)) = (vert(t[0]), vert(t[1]), vert(t[2])) else { continue };
                m.tris.push((if flip { [a, c, b] } else { [a, b, c] }, mi));
                m.tri_part.push(part);
            }
        }
    }
    // unbenutzte Materialien bleiben drin (Index-treu); export schreibt nur benutzte
    Ok(m)
}

// ------------------------------------------------------------------ Umrechnen

#[derive(Clone, Debug)]
pub struct ImportOpts {
    pub name: String,
    pub folder: String,
    /// Einheiten pro Dateieinheit (Meter → 39,37)
    pub scale: f64,
    /// Datei ist Y-oben (glTF, Blender-OBJ)
    pub y_up: bool,
    /// Drehung um die Hochachse in Grad
    pub yaw: f64,
    pub is_static: bool,
    pub mass: f64,
    pub surfaceprop: String,
    /// größte Texturkante
    pub tex_max: u32,
    pub collision: bool,
    /// Bodygroups: Teile darin sind wählbar, alle anderen bilden den festen Körper
    pub bodygroups: Vec<Bodygroup>,
    /// weitere Skins (Skin 0 ist das Original): je Material ein anderes Bild oder `None` = wie Skin 0
    pub skins: Vec<Vec<Option<PathBuf>>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bodygroup {
    pub name: String,
    /// Teile, je eines eine Variante (in dieser Reihenfolge)
    pub parts: Vec<usize>,
    /// zusätzliche Variante „nichts“
    pub blank: bool,
}

impl Default for ImportOpts {
    fn default() -> Self {
        ImportOpts { name: "modell".into(), folder: "amboss".into(), scale: 39.37, y_up: true, yaw: 0.0, is_static: true, mass: 50.0, surfaceprop: "default".into(), tex_max: 1024, collision: true, bodygroups: Vec::new(), skins: Vec::new() }
    }
}

/// 1 Meter in Source-Einheiten (1 Einheit = 1 Zoll)
pub const METER: f64 = 39.37;

/// Vorschlag für die Einstellungen: glTF ist Y-oben in Metern; OBJ in Metern, wenn es sehr klein ist
pub fn suggest(m: &Model, path: &Path) -> ImportOpts {
    let raw = m.tris.iter().flat_map(|t| t.0.iter().map(|v| v.p)).fold(Aabb::EMPTY, |mut b, p| {
        b.add_point(p);
        b
    });
    let ext = raw.size().max_element();
    let name: String = path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    ImportOpts {
        name: if name.is_empty() { "modell".into() } else { name },
        scale: if m.gltf || ext < 50.0 { METER } else { 1.0 },
        y_up: m.gltf || raw.size().y > raw.size().z * 1.2,
        ..Default::default()
    }
}

fn convert(p: DVec3, o: &ImportOpts) -> DVec3 {
    // Y-oben → Z-oben (Drehung um X, Händigkeit bleibt)
    let q = if o.y_up { DVec3::new(p.x, -p.z, p.y) } else { p };
    let (s, c) = o.yaw.to_radians().sin_cos();
    DVec3::new(q.x * c - q.y * s, q.x * s + q.y * c, q.z) * o.scale
}

fn convert_n(n: DVec3, o: &ImportOpts) -> DVec3 {
    let q = if o.y_up { DVec3::new(n.x, -n.z, n.y) } else { n };
    let (s, c) = o.yaw.to_radians().sin_cos();
    DVec3::new(q.x * c - q.y * s, q.x * s + q.y * c, q.z).normalize_or_zero()
}

/// Dreiecke in Source-Koordinaten, Ursprung unten in der Mitte
pub fn converted(m: &Model, o: &ImportOpts) -> (Vec<([Vert; 3], usize)>, Aabb) {
    let mut b = Aabb::EMPTY;
    let mut out: Vec<([Vert; 3], usize)> = m
        .tris
        .iter()
        .map(|(t, mi)| {
            let t = t.map(|v| {
                let p = convert(v.p, o);
                b.add_point(p);
                Vert { p, n: v.n.map(|n| convert_n(n, o)), uv: v.uv }
            });
            (t, *mi)
        })
        .collect();
    let origin = DVec3::new(b.center().x, b.center().y, b.min.z);
    for (t, _) in &mut out {
        for v in t.iter_mut() {
            v.p -= origin;
        }
    }
    (out, Aabb::new(b.min - origin, b.max - origin))
}

/// Materialname im Modell (Kleinbuchstaben, nur a–z, 0–9, _), eindeutig je Index
pub fn smd_material_names(m: &Model) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (i, mat) in m.materials.iter().enumerate() {
        let mut n: String = mat.name.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        n = n.trim_matches('_').to_string();
        if n.is_empty() {
            n = format!("mat{i}");
        }
        if names.contains(&n) {
            n = format!("{n}_{i}");
        }
        names.push(n);
    }
    names
}

fn smd_tris(tris: &[([Vert; 3], usize)], names: &[String]) -> String {
    let mut out = String::new();
    for (t, mi) in tris {
        let face_n = (t[1].p - t[0].p).cross(t[2].p - t[0].p).normalize_or(DVec3::Z);
        let _ = writeln!(out, "{}", names[*mi]);
        for v in t {
            let n = v.n.filter(|n| n.length_squared() > 0.5).unwrap_or(face_n);
            let _ = writeln!(out, "0 {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.5} {:.5} 0", v.p.x, v.p.y, v.p.z, n.x, n.y, n.z, v.uv.x, v.uv.y);
        }
    }
    out
}

/// Konvexe Hülle als Kollisionsmodell: äußerste Punkte in 62 Richtungen, daraus die Hülle
pub fn hull_smd(tris: &[([Vert; 3], usize)]) -> Option<String> {
    let pts: Vec<DVec3> = tris.iter().flat_map(|t| t.0.iter().map(|v| v.p)).collect();
    if pts.len() < 4 {
        return None;
    }
    let n = 62;
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let mut extreme: Vec<DVec3> = Vec::new();
    for i in 0..n {
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
        let r = (1.0 - y * y).sqrt();
        let d = DVec3::new((golden * i as f64).cos() * r, (golden * i as f64).sin() * r, y);
        let best = pts.iter().copied().fold((f64::MIN, DVec3::ZERO), |acc, p| if p.dot(d) > acc.0 { (p.dot(d), p) } else { acc });
        if !extreme.iter().any(|q| q.distance(best.1) < 0.5) {
            extreme.push(best.1);
        }
    }
    let planes: Vec<Option<crate::geometry::Plane>> = crate::geometry::hull_planes(&extreme).into_iter().map(Some).collect();
    if planes.len() < 4 {
        return None;
    }
    let mut out = String::new();
    for poly in crate::geometry::polygons_from_planes(&planes) {
        if poly.len() < 3 {
            continue;
        }
        let nrm = (poly[1] - poly[0]).cross(poly[2] - poly[0]).normalize_or(DVec3::Z);
        for k in 1..poly.len() - 1 {
            let _ = writeln!(out, "phys");
            for p in [poly[0], poly[k], poly[k + 1]] {
                let _ = writeln!(out, "0 {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} 0 0 0", p.x, p.y, p.z, nrm.x, nrm.y, nrm.z);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Bild einer Textur einlesen (PNG oder JPEG)
pub fn load_image(img: &Image) -> Result<Rgba, String> {
    let (bytes, label) = match img {
        Image::File(p) => (std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?, p.display().to_string()),
        Image::Bytes(b, n) => (b.clone(), n.clone()),
    };
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return crate::vtf_write::load_png(&bytes).map_err(|e| format!("{label}: {e}"));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(&bytes));
        let px = dec.decode().map_err(|e| format!("{label}: {e}"))?;
        let info = dec.info().ok_or("JPEG ohne Bildinfo")?;
        let rgba: Vec<u8> = match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            jpeg_decoder::PixelFormat::L16 => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], 255]).collect(),
            jpeg_decoder::PixelFormat::CMYK32 => px.chunks(4).flat_map(|c| {
                let k = 255 - c[3] as u32;
                [(255 - c[0] as u32) * k / 255, (255 - c[1] as u32) * k / 255, (255 - c[2] as u32) * k / 255].map(|x| x as u8).into_iter().chain([255])
            }).collect(),
        };
        return Ok(Rgba { w: info.width as u32, h: info.height as u32, px: rgba });
    }
    if label.to_ascii_lowercase().ends_with(".tga") || matches!(bytes.get(2), Some(2 | 3 | 10 | 11)) {
        return crate::vtf_write::load_tga(&bytes).map_err(|e| format!("{label}: {e}"));
    }
    Err(format!("{label}: nur PNG, JPEG und TGA werden unterstützt"))
}

fn srgb8(c: f32, linear: bool) -> u8 {
    let c = c.clamp(0.0, 1.0);
    ((if linear { c.powf(1.0 / 2.2) } else { c }) * 255.0).round() as u8
}

/// Ergebnis des Exports
pub struct Export {
    pub dir: PathBuf,
    pub qc: PathBuf,
    pub tris: usize,
    pub size: DVec3,
    /// Meldungen (fehlende Texturen u. a.)
    pub notes: Vec<String>,
}

/// Modellordner unter materials/
pub fn mat_dir(o: &ImportOpts) -> String {
    format!("models/{}/{}", o.folder, o.name)
}

/// SMD/QC nach `dir` schreiben, Materialien (VMT + VTF) nach `<game_dir>/materials/…`
pub fn export(m: &Model, o: &ImportOpts, dir: &Path, game_dir: Option<&Path>) -> Result<Export, String> {
    let (tris, b) = converted(m, o);
    let names = smd_material_names(m);
    let mut notes = Vec::new();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let w = |name: String, text: String| std::fs::write(dir.join(&name), text).map_err(|e| format!("{name}: {e}"));
    // Teile in Bodygroups gehören nicht zum festen Körper
    let in_group = |part: usize| o.bodygroups.iter().any(|g| g.parts.contains(&part));
    let pick = |f: &dyn Fn(usize) -> bool| -> Vec<([Vert; 3], usize)> { tris.iter().enumerate().filter(|(i, _)| f(m.part_of(*i))).map(|(_, t)| *t).collect() };
    let main = pick(&|p| !in_group(p));
    let mut smds: Vec<String> = Vec::new();
    if !main.is_empty() {
        w(format!("{}_ref.smd", o.name), crate::propper::smd(&smd_tris(&main, &names)))?;
        smds.push(format!("{}_ref.smd", o.name));
    }
    // je Bodygroup und Variante eine eigene SMD
    let mut groups: Vec<(String, Vec<String>, bool)> = Vec::new();
    for (gi, g) in o.bodygroups.iter().enumerate() {
        let mut files = Vec::new();
        for (k, part) in g.parts.iter().enumerate() {
            let part_tris = pick(&|p| p == *part);
            if part_tris.is_empty() {
                continue;
            }
            let f = format!("{}_bg{gi}_{k}.smd", o.name);
            w(f.clone(), crate::propper::smd(&smd_tris(&part_tris, &names)))?;
            smds.push(f.clone());
            files.push(f);
        }
        if !files.is_empty() || g.blank {
            let gname: String = g.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
            groups.push((if gname.is_empty() { format!("gruppe{gi}") } else { gname }, files, g.blank));
        }
    }
    if smds.is_empty() {
        return Err("Keine Dreiecke für das Modell (alle Teile leer?)".into());
    }
    let phys = if o.collision { hull_smd(&tris) } else { None };
    if o.collision && phys.is_none() {
        notes.push("Keine Kollision: das Modell ist flach".to_string());
    }
    if let Some(p) = &phys {
        w(format!("{}_phys.smd", o.name), crate::propper::smd(p))?;
    }
    let mut q = String::new();
    let _ = writeln!(q, "$modelname \"{}/{}.mdl\"", o.folder, o.name);
    if !main.is_empty() {
        let _ = writeln!(q, "$body \"body\" \"{}_ref.smd\"", o.name);
    }
    for (gname, files, blank) in &groups {
        let _ = writeln!(q, "$bodygroup \"{gname}\"\n{{");
        for f in files {
            let _ = writeln!(q, "\tstudio \"{f}\"");
        }
        if *blank {
            let _ = writeln!(q, "\tblank");
        }
        let _ = writeln!(q, "}}");
    }
    let _ = writeln!(q, "$cdmaterials \"{}/\"", mat_dir(o));
    if o.is_static {
        let _ = writeln!(q, "$staticprop");
    }
    let _ = writeln!(q, "$surfaceprop \"{}\"", o.surfaceprop);
    // Skins: je Skin die Materialnamen in derselben Reihenfolge
    let mut used = vec![false; m.materials.len()];
    for (_, mi) in &tris {
        used[*mi] = true;
    }
    let skins: Vec<&Vec<Option<PathBuf>>> = o.skins.iter().filter(|sk| sk.iter().enumerate().any(|(i, x)| x.is_some() && used.get(i).copied().unwrap_or(false))).collect();
    if !skins.is_empty() {
        let used_idx: Vec<usize> = (0..m.materials.len()).filter(|i| used[*i]).collect();
        let _ = writeln!(q, "$texturegroup \"skinfamilies\"\n{{");
        let row = |f: &dyn Fn(usize) -> String| used_idx.iter().map(|i| format!("\"{}\"", f(*i))).collect::<Vec<_>>().join(" ");
        let _ = writeln!(q, "\t{{ {} }}", row(&|i| names[i].clone()));
        for (k, sk) in skins.iter().enumerate() {
            let _ = writeln!(q, "\t{{ {} }}", row(&|i| if sk.get(i).is_some_and(|x| x.is_some()) { format!("{}_skin{}", names[i], k + 1) } else { names[i].clone() }));
        }
        let _ = writeln!(q, "}}");
    }
    let _ = writeln!(q, "$sequence \"idle\" \"{}\" fps 30", smds[0]);
    if phys.is_some() {
        let _ = writeln!(q, "$collisionmodel \"{}_phys.smd\" {{\n\t$mass {}\n}}", o.name, if o.is_static { 1.0 } else { o.mass });
    }
    let qc = dir.join(format!("{}.qc", o.name));
    std::fs::write(&qc, q).map_err(|e| format!("QC: {e}"))?;
    if tris.len() > 60000 {
        notes.push(format!("{} Dreiecke – studiomdl kann bei sehr großen Modellen abbrechen; besser vereinfachen", tris.len()));
    }
    // Materialien: nur benutzte
    if let Some(gd) = game_dir {
        let md = gd.join("materials").join(mat_dir(o));
        std::fs::create_dir_all(&md).map_err(|e| format!("{}: {e}", md.display()))?;
        for (i, mat) in m.materials.iter().enumerate().filter(|(i, _)| used[*i]) {
            let img = match &mat.image {
                Some(im) => match load_image(im) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        notes.push(format!("Textur nicht geladen ({e}) – {} bekommt seine Grundfarbe", mat.name));
                        None
                    }
                },
                None => None,
            };
            let tinted = img.is_some() && mat.color[..3].iter().any(|c| (c - 1.0).abs() > 0.01);
            let rgba = img.unwrap_or_else(|| {
                let c = [srgb8(mat.color[0], mat.linear), srgb8(mat.color[1], mat.linear), srgb8(mat.color[2], mat.linear), (mat.color[3].clamp(0.0, 1.0) * 255.0) as u8];
                Rgba { w: 4, h: 4, px: c.repeat(16) }
            });
            let vtf = crate::vtf_write::encode(&rgba, crate::vtf_write::VtfOpts { max_size: o.tex_max, ..Default::default() });
            std::fs::write(md.join(format!("{}.vtf", names[i])), vtf).map_err(|e| format!("VTF: {e}"))?;
            let mut vmt = format!("\"VertexLitGeneric\"\n{{\n\t\"$basetexture\" \"{}/{}\"\n", mat_dir(o), names[i]);
            if tinted && mat.image.is_some() {
                let _ = writeln!(vmt, "\t\"$color2\" \"[{:.3} {:.3} {:.3}]\"", mat.color[0], mat.color[1], mat.color[2]);
            }
            match mat.alpha {
                Alpha::Mask => vmt.push_str("\t\"$alphatest\" \"1\"\n"),
                Alpha::Blend if rgba.has_alpha() => vmt.push_str("\t\"$translucent\" \"1\"\n"),
                _ => {}
            }
            if mat.double_sided {
                vmt.push_str("\t\"$nocull\" \"1\"\n");
            }
            vmt.push_str("\t\"$model\" \"1\"\n}\n");
            std::fs::write(md.join(format!("{}.vmt", names[i])), &vmt).map_err(|e| format!("VMT: {e}"))?;
            // weitere Skins: gleiche Einstellungen, anderes Bild
            for (k, sk) in skins.iter().enumerate() {
                let Some(Some(path)) = sk.get(i) else { continue };
                let alt = format!("{}_skin{}", names[i], k + 1);
                match load_image(&Image::File(path.clone())) {
                    Ok(img) => {
                        let vtf = crate::vtf_write::encode(&img, crate::vtf_write::VtfOpts { max_size: o.tex_max, ..Default::default() });
                        std::fs::write(md.join(format!("{alt}.vtf")), vtf).map_err(|e| format!("VTF: {e}"))?;
                    }
                    Err(e) => notes.push(format!("Skin {}: Bild für {} nicht geladen ({e})", k + 1, mat.name)),
                }
                let v2 = vmt.replace(&format!("{}/{}\"", mat_dir(o), names[i]), &format!("{}/{alt}\"", mat_dir(o)));
                std::fs::write(md.join(format!("{alt}.vmt")), v2).map_err(|e| format!("VMT: {e}"))?;
            }
        }
    }
    Ok(Export { dir: dir.to_path_buf(), qc, tris: tris.len(), size: b.size(), notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amboss_imp_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn obj_with_mtl() {
        let d = tmp("obj");
        // 1×1-PNG als Textur
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 2, 2);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]).unwrap();
        }
        std::fs::write(d.join("holz bunt.png"), &png).unwrap();
        std::fs::write(d.join("kiste.mtl"), "newmtl Holz\nKd 1 1 1\nmap_Kd -s 1 1 1 holz bunt.png\nnewmtl Glas\nKd 0.2 0.4 0.8\nd 0.5\n").unwrap();
        let obj = "mtllib kiste.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nusemtl Holz\nf 1/1 2/2 3/3 4/4\nusemtl Glas\nf -4 -2 -1\n";
        std::fs::write(d.join("kiste.obj"), obj).unwrap();
        let m = load(&d.join("kiste.obj")).unwrap();
        assert_eq!(m.tris.len(), 3);
        assert_eq!(m.materials.len(), 2);
        assert!(matches!(&m.materials[0].image, Some(Image::File(p)) if p.ends_with("holz bunt.png")));
        assert_eq!(m.materials[1].alpha, Alpha::Blend);
        assert_eq!(m.tris[2].0[1].p, DVec3::new(1.0, 1.0, 0.0));
        // Y-oben in Metern: 1 m → 39,37 Einheiten, Fläche steht dann aufrecht
        let o = ImportOpts { name: "kiste".into(), ..Default::default() };
        let (tris, b) = converted(&m, &o);
        assert!((b.size().z - METER).abs() < 1e-9 && b.size().y.abs() < 1e-9, "{:?}", b.size());
        assert!(b.min.z.abs() < 1e-9);
        assert_eq!(tris.len(), 3);
        let game = d.join("garrysmod");
        let ex = export(&m, &o, &d.join("src"), Some(&game)).unwrap();
        assert!(ex.notes.is_empty() || ex.notes.iter().all(|n| n.contains("flach")), "{:?}", ex.notes);
        let smd = std::fs::read_to_string(d.join("src/kiste_ref.smd")).unwrap();
        assert_eq!(smd.lines().filter(|l| *l == "holz" || *l == "glas").count(), 3, "{smd}");
        let q = std::fs::read_to_string(&ex.qc).unwrap();
        assert!(q.contains("$cdmaterials \"models/amboss/kiste/\"") && q.contains("$staticprop"));
        let md = game.join("materials/models/amboss/kiste");
        assert!(md.join("holz.vtf").is_file() && md.join("glas.vtf").is_file());
        let glas = std::fs::read_to_string(md.join("glas.vmt")).unwrap();
        assert!(glas.contains("$translucent") && glas.contains("models/amboss/kiste/glas"), "{glas}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bodygroups_and_skins() {
        let d = tmp("bg");
        let mut png = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png, 1, 1);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[0, 0, 255]).unwrap();
        }
        std::fs::write(d.join("blau.png"), &png).unwrap();
        // Körper (Quader-Fläche), zwei Helme als eigene Objekte
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\n\
o koerper\nusemtl stoff\nf 1 2 3 4\n\
o helm_a\nusemtl metall\nf 5 6 7\n\
o helm_b\nusemtl metall\nf 5 7 6\n";
        std::fs::write(d.join("figur.obj"), obj).unwrap();
        let m = load(&d.join("figur.obj")).unwrap();
        assert_eq!(m.parts, vec!["koerper", "helm_a", "helm_b"]);
        assert_eq!(m.part_counts(), vec![2, 1, 1]);
        let mut o = ImportOpts { name: "figur".into(), ..Default::default() };
        o.bodygroups.push(Bodygroup { name: "Helm".into(), parts: vec![1, 2], blank: true });
        // Skin 1: „metall“ (Material 1) bekommt ein blaues Bild, „stoff“ bleibt
        o.skins.push(vec![None, Some(d.join("blau.png"))]);
        let game = d.join("garrysmod");
        let ex = export(&m, &o, &d.join("src"), Some(&game)).unwrap();
        let q = std::fs::read_to_string(&ex.qc).unwrap();
        assert!(q.contains("$body \"body\" \"figur_ref.smd\""), "{q}");
        assert!(q.contains("$bodygroup \"Helm\"\n{\n\tstudio \"figur_bg0_0.smd\"\n\tstudio \"figur_bg0_1.smd\"\n\tblank\n}"), "{q}");
        assert!(q.contains("$texturegroup \"skinfamilies\"\n{\n\t{ \"stoff\" \"metall\" }\n\t{ \"stoff\" \"metall_skin1\" }\n}"), "{q}");
        let body = std::fs::read_to_string(d.join("src/figur_ref.smd")).unwrap();
        assert!(body.contains("stoff") && !body.contains("metall"), "Helme nicht im festen Körper");
        assert!(d.join("src/figur_bg0_1.smd").is_file());
        let md = game.join("materials/models/amboss/figur");
        assert!(md.join("metall_skin1.vtf").is_file());
        let v = std::fs::read_to_string(md.join("metall_skin1.vmt")).unwrap();
        assert!(v.contains("models/amboss/figur/metall_skin1\""), "{v}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Minimales glTF mit einem Dreieck (eingebetteter Puffer), Knoten verschoben
    fn tiny_gltf() -> String {
        let mut buf: Vec<u8> = Vec::new();
        for f in [0.0f32, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            buf.extend_from_slice(&f.to_le_bytes());
        }
        for i in [0u16, 1, 2, 0] {
            buf.extend_from_slice(&i.to_le_bytes());
        }
        let b64 = {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in buf.chunks(3) {
                let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                for k in 0..4 {
                    if k <= c.len() {
                        s.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
                    } else {
                        s.push('=');
                    }
                }
            }
            s
        };
        format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[0,0,5]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],
            "materials":[{{"name":"Rot Metall","pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}}}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[2,1,0]}},
                         {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            len = buf.len()
        )
    }

    #[test]
    fn gltf_triangle() {
        let d = tmp("gltf");
        std::fs::write(d.join("dreieck.gltf"), tiny_gltf()).unwrap();
        let m = load(&d.join("dreieck.gltf")).unwrap();
        assert!(m.gltf);
        assert_eq!(m.tris.len(), 1);
        assert_eq!(m.tris[0].0[1].p, DVec3::new(2.0, 0.0, 5.0), "Knoten-Verschiebung angewendet");
        assert_eq!(m.materials[m.tris[0].1].name, "Rot Metall");
        let o = suggest(&m, &d.join("dreieck.gltf"));
        assert_eq!(o.name, "dreieck");
        assert_eq!(o.scale, METER);
        assert!(o.y_up);
        assert_eq!(smd_material_names(&m)[0], "rot_metall");
        // flaches Modell: keine Kollision, Grundfarbe als 4×4-VTF
        let ex = export(&m, &o, &d.join("src"), Some(&d)).unwrap();
        assert!(ex.notes.iter().any(|n| n.contains("flach")));
        assert!(d.join("materials/models/amboss/dreieck/rot_metall.vtf").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hull_of_box() {
        let mut m = Model::default();
        m.materials.push(Material::plain("x"));
        let c: Vec<DVec3> = (0..8).map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 10.0).collect();
        for f in [[0, 1, 3], [0, 3, 2], [4, 6, 7], [4, 7, 5], [0, 4, 5], [0, 5, 1], [2, 3, 7], [2, 7, 6], [0, 2, 6], [0, 6, 4], [1, 5, 7], [1, 7, 3]] {
            m.tris.push((f.map(|i| Vert { p: c[i], n: None, uv: DVec2::ZERO }), 0));
        }
        let o = ImportOpts { y_up: false, scale: 1.0, ..Default::default() };
        let (tris, b) = converted(&m, &o);
        assert_eq!(b.size(), DVec3::splat(10.0));
        let h = hull_smd(&tris).unwrap();
        // Würfel: 6 Flächen × 2 Dreiecke
        assert_eq!(h.matches("phys").count(), 12, "{h}");
        assert_eq!(base64_decode("aGFsbG8=").unwrap(), b"hallo");
        assert_eq!(percent_decode("a%20b.png"), "a b.png");
    }
}
