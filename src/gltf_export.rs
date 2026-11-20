//! glTF-Export (.glb): Brushes und Displacements der Map oder der Auswahl mit eingebetteten
//! PNG-Texturen – in Metern und mit Y nach oben, wie Blender, Unity, Unreal und Godot es erwarten.
//! Werkzeug-Texturen (nodraw, Trigger, Clips …) und Entities ohne Brushes fehlen.

use crate::editor::Editor;
use crate::map::Solid;
use glam::DVec3;
use serde_json::{json, Value};

/// Source-Einheit → Meter
pub const METER: f64 = 0.0254;

/// Was zu einem Material bekannt ist
pub struct TexOut {
    /// Texturgröße in Pixeln (für die Texturkoordinaten)
    pub dims: (f64, f64),
    /// PNG der Grundtextur, falls lesbar
    pub png: Option<Vec<u8>>,
    pub translucent: bool,
    /// Ersatzfarbe ohne Textur (sRGB 0..1)
    pub color: [f32; 3],
}

#[derive(Default)]
struct Prim {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

/// Source (x, y, z) → glTF (x, z, −y), in Metern
fn conv(p: DVec3) -> [f32; 3] {
    [(p.x * METER) as f32, (p.z * METER) as f32, (-p.y * METER) as f32]
}

fn conv_n(n: DVec3) -> [f32; 3] {
    [n.x as f32, n.z as f32, -n.y as f32]
}

fn add_solid(prims: &mut Vec<(String, Prim)>, s: &Solid, tex: &mut dyn FnMut(&str) -> (f64, f64)) {
    for (i, poly) in s.polys.iter().enumerate() {
        let side = &s.sides[i];
        if poly.len() < 3 || (s.has_disp() && side.disp.is_none()) || crate::editor::is_tool_material(&side.material) {
            continue;
        }
        let mat = side.material.to_ascii_lowercase();
        let (tw, th) = tex(&mat);
        let k = match prims.iter().position(|(m, _)| *m == mat) {
            Some(k) => k,
            None => {
                prims.push((mat.clone(), Prim::default()));
                prims.len() - 1
            }
        };
        let p = &mut prims[k].1;
        let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
        let uv = |q: DVec3| [(side.uaxis.coord(q) / tw) as f32, (side.vaxis.coord(q) / th) as f32];
        let base = p.pos.len() as u32;
        let (pts, tris): (Vec<DVec3>, Vec<[usize; 3]>) = match s.disps.get(i).cloned().flatten() {
            Some(g) => {
                for (q, b) in g.pos.iter().zip(&g.base) {
                    p.pos.push(conv(*q));
                    p.nrm.push(conv_n(n));
                    p.uv.push(uv(*b));
                }
                let t = g.triangles();
                (g.pos, t)
            }
            None => {
                for q in poly {
                    p.pos.push(conv(*q));
                    p.nrm.push(conv_n(n));
                    p.uv.push(uv(*q));
                }
                (poly.clone(), (1..poly.len() - 1).map(|j| [0, j, j + 1]).collect())
            }
        };
        // glTF erwartet Dreiecke gegen den Uhrzeigersinn von außen: an der Flächennormale ausrichten
        for t in tris {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            let ccw = (b - a).cross(c - a).dot(n) >= 0.0;
            let t = if ccw { t } else { [t[0], t[2], t[1]] };
            p.idx.extend(t.map(|k| base + k as u32));
        }
    }
}

/// .glb bauen. `selection_only`: nur ausgewählte Brushes; `tex` liefert Texturgröße, PNG und Transparenz.
/// Ergebnis: (Datei, Anzahl Dreiecke, Anzahl Materialien)
pub fn build_glb(ed: &Editor, selection_only: bool, tex: &mut dyn FnMut(&str) -> TexOut) -> Result<(Vec<u8>, usize, usize), String> {
    let mut infos: Vec<(String, TexOut)> = Vec::new();
    let mut prims: Vec<(String, Prim)> = Vec::new();
    {
        let mut dims = |m: &str| -> (f64, f64) {
            if let Some((_, t)) = infos.iter().find(|(k, _)| k == m) {
                return t.dims;
            }
            let t = tex(m);
            let d = t.dims;
            infos.push((m.to_string(), t));
            d
        };
        for (e, s) in ed.map.solids_owned() {
            if !ed.is_visible_solid(e, s) || selection_only && !ed.is_selected_solid(e.map(|e| e.id), s.id) {
                continue;
            }
            add_solid(&mut prims, s, &mut dims);
        }
    }
    prims.retain(|(_, p)| !p.idx.is_empty());
    if prims.is_empty() {
        return Err(if selection_only { "In der Auswahl sind keine exportierbaren Flächen (nur Werkzeug-Texturen?)".into() } else { "Keine exportierbaren Flächen in der Map".into() });
    }
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut push_view = |bin: &mut Vec<u8>, data: &[u8], target: Option<u32>| -> usize {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let off = bin.len();
        bin.extend_from_slice(data);
        let mut v = json!({"buffer": 0, "byteOffset": off, "byteLength": data.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        views.push(v);
        views.len() - 1
    };
    let mut images: Vec<Value> = Vec::new();
    let mut textures: Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut primitives: Vec<Value> = Vec::new();
    let mut tris = 0usize;
    for (mat, p) in &prims {
        // Material
        let info = infos.iter().find(|(k, _)| k == mat).map(|(_, t)| t);
        let mut pbr = json!({"metallicFactor": 0.0, "roughnessFactor": 0.9});
        if let Some(png) = info.and_then(|t| t.png.as_ref()) {
            let v = push_view(&mut bin, png, None);
            images.push(json!({"bufferView": v, "mimeType": "image/png", "name": mat}));
            textures.push(json!({"source": images.len() - 1, "sampler": 0}));
            pbr["baseColorTexture"] = json!({"index": textures.len() - 1});
        } else {
            let c = info.map(|t| t.color).unwrap_or([0.7, 0.7, 0.7]);
            pbr["baseColorFactor"] = json!([c[0], c[1], c[2], 1.0]);
        }
        let mut m = json!({"name": mat, "pbrMetallicRoughness": pbr});
        if info.is_some_and(|t| t.translucent) {
            m["alphaMode"] = json!("BLEND");
            m["doubleSided"] = json!(true);
        }
        materials.push(m);
        // Daten
        let (mut mn, mut mx) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in &p.pos {
            for k in 0..3 {
                mn[k] = mn[k].min(q[k]);
                mx[k] = mx[k].max(q[k]);
            }
        }
        let v_pos = push_view(&mut bin, bytemuck::cast_slice(&p.pos), Some(34962));
        accessors.push(json!({"bufferView": v_pos, "componentType": 5126, "count": p.pos.len(), "type": "VEC3", "min": mn, "max": mx}));
        let a_pos = accessors.len() - 1;
        let v_n = push_view(&mut bin, bytemuck::cast_slice(&p.nrm), Some(34962));
        accessors.push(json!({"bufferView": v_n, "componentType": 5126, "count": p.nrm.len(), "type": "VEC3"}));
        let a_n = accessors.len() - 1;
        let v_uv = push_view(&mut bin, bytemuck::cast_slice(&p.uv), Some(34962));
        accessors.push(json!({"bufferView": v_uv, "componentType": 5126, "count": p.uv.len(), "type": "VEC2"}));
        let a_uv = accessors.len() - 1;
        let v_i = push_view(&mut bin, bytemuck::cast_slice(&p.idx), Some(34963));
        accessors.push(json!({"bufferView": v_i, "componentType": 5125, "count": p.idx.len(), "type": "SCALAR"}));
        let a_i = accessors.len() - 1;
        primitives.push(json!({"attributes": {"POSITION": a_pos, "NORMAL": a_n, "TEXCOORD_0": a_uv}, "indices": a_i, "material": materials.len() - 1}));
        tris += p.idx.len() / 3;
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": format!("Amboss {}", env!("CARGO_PKG_VERSION"))},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": ed.map_name()}],
        "meshes": [{"name": ed.map_name(), "primitives": primitives}],
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    if !images.is_empty() {
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
        // Wiederholen, linear mit Mipmaps
        doc["samplers"] = json!([{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]);
    }
    let mut js = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    Ok((out, tris, prims.len()))
}

// ---------------------------------------------------------------- Aufruf aus der Oberfläche
use crate::app::App;

impl App {
    pub(crate) fn export_gltf(&mut self) {
        let sel = !self.ed.sel.is_empty();
        let name = format!("{}{}.glb", self.ed.map_name(), if sel { "_auswahl" } else { "" });
        let Some(mut p) = rfd::FileDialog::new().add_filter("glTF (.glb)", &["glb"]).set_file_name(name).save_file() else { return };
        if p.extension().is_none() {
            p.set_extension("glb");
        }
        let fs = self.mats.fs();
        let mats = &self.mats;
        let mut tex = |m: &str| -> TexOut {
            let info = mats.info(m);
            let dims = info.as_ref().map(|i| (i.width as f64, i.height as f64)).unwrap_or((512.0, 512.0));
            let color = info.as_ref().map(|i| [i.avg[0] as f32 / 255.0, i.avg[1] as f32 / 255.0, i.avg[2] as f32 / 255.0]).unwrap_or([0.7, 0.7, 0.7]);
            let png = fs.as_ref().and_then(|fs| texture_png(fs, m));
            TexOut { dims, png, translucent: info.is_some_and(|i| i.translucent), color }
        };
        match build_glb(&self.ed, sel, &mut tex) {
            Ok((data, tris, n)) => {
                self.ed.status = match std::fs::write(&p, data) {
                    Ok(()) => crate::i18n::tr_dyn(&format!("Exportiert: {} ({tris} Dreiecke, {n} Materialien, Meter, Y nach oben)", p.display())),
                    Err(e) => crate::i18n::tr_dyn(&format!("Export fehlgeschlagen: {e}")),
                };
            }
            Err(e) => self.ed.status = crate::i18n::tr_dyn(&e),
        }
    }
}

/// Grundtextur eines Materials als PNG (höchstens 1024 Pixel)
fn texture_png(fs: &crate::gamefs::GameFs, mat: &str) -> Option<Vec<u8>> {
    let base = crate::materials::parse_vmt(fs, mat, 0).and_then(|v| v.basetexture).unwrap_or_else(|| mat.to_string());
    let base = base.replace('\\', "/").to_ascii_lowercase();
    let bytes = fs.read(&format!("materials/{}.vtf", base.trim_end_matches(".vtf")))?;
    let img = crate::vtf::load(&bytes, 1024).ok()?;
    crate::screenshot::png(&img.rgba, img.pw, img.ph).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_roundtrip() {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.material = "BRICK/BRICKWALL001A".into();
        ed.create_block(crate::geometry::Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 32.0)));
        ed.material = "TOOLS/TOOLSNODRAW".into();
        ed.create_block(crate::geometry::Aabb::new(DVec3::new(200.0, 0.0, 0.0), DVec3::new(264.0, 64.0, 64.0)));
        let png = crate::screenshot::png(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255], 2, 2).unwrap();
        let mut asked = Vec::new();
        let mut tex = |m: &str| {
            asked.push(m.to_string());
            TexOut { dims: (128.0, 128.0), png: Some(png.clone()), translucent: false, color: [1.0; 3] }
        };
        let (glb, tris, n) = build_glb(&ed, false, &mut tex).unwrap();
        assert_eq!((tris, n), (12, 1), "ein Quader (12 Dreiecke), nodraw fehlt");
        assert_eq!(asked, vec!["brick/brickwall001a"]);
        let g = gltf::Gltf::from_slice(&glb).expect("gültiges glTF");
        let blob = g.blob.clone().expect("BIN-Block");
        assert_eq!(g.document.images().count(), 1);
        assert!(matches!(g.document.images().next().unwrap().source(), gltf::image::Source::View { mime_type: "image/png", .. }));
        let mesh = g.document.meshes().next().unwrap();
        let prim = mesh.primitives().next().unwrap();
        let r = prim.reader(|_| Some(&blob[..]));
        let pos: Vec<[f32; 3]> = r.read_positions().unwrap().collect();
        // 128 Einheiten in X = 3,25 m; Höhe (Source Z) wird glTF Y
        let maxx = pos.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let maxy = pos.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let minz = pos.iter().map(|p| p[2]).fold(f32::MAX, f32::min);
        assert!((maxx - 3.2512).abs() < 1e-3 && (maxy - 0.8128).abs() < 1e-3 && (minz + 1.6256).abs() < 1e-3, "{maxx} {maxy} {minz}");
        // Normalen zeigen nach außen und passen zur Wicklung der Dreiecke
        let nrm: Vec<[f32; 3]> = r.read_normals().unwrap().collect();
        let idx: Vec<u32> = r.read_indices().unwrap().into_u32().collect();
        for t in idx.chunks(3) {
            let (a, b, c) = (glam::Vec3::from(pos[t[0] as usize]), glam::Vec3::from(pos[t[1] as usize]), glam::Vec3::from(pos[t[2] as usize]));
            let face = (b - a).cross(c - a).normalize();
            assert!(face.dot(glam::Vec3::from(nrm[t[0] as usize])) > 0.99, "Wicklung gegen den Uhrzeigersinn");
        }
        // nur Auswahl: nichts ausgewählt → Fehler
        assert!(build_glb(&ed, true, &mut |_| TexOut { dims: (1.0, 1.0), png: None, translucent: false, color: [1.0; 3] }).is_err());
    }
}
