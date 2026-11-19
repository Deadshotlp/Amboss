//! 3D-Ansicht: OpenGL-Renderer (glow), Kamera, Picking und 3D-Werkzeuge.

use crate::editor::{Editor, Obj, Tool};
use crate::geometry::Aabb;
use crate::map::{parse_vec, Entity, Map, Shared, Solid};
use crate::materials::{normalize, MaterialSystem};
use crate::models::ModelSystem;
use crate::transform::{angles_to_basis, Xform};
use eframe::egui::{self, Align2, Color32, FontId, Key, PointerButton, Rect, Sense, Ui, Vec2};
use eframe::egui_glow;
use eframe::glow::{self, HasContext};
use glam::{DVec2, DVec3, Mat4, Vec3};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ------------------------------------------------------------------ Kamera

#[derive(Clone, Debug)]
pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Z).normalize_or_zero()
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = Mat4::perspective_rh_gl(self.fov.to_radians(), aspect.max(0.01), 4.0, 65536.0);
        let view = Mat4::look_at_rh(self.pos, self.pos + self.forward(), Vec3::Z);
        proj * view
    }

    pub fn ray(&self, rect: Rect, p: egui::Pos2) -> (DVec3, DVec3) {
        let ndc_x = (p.x - rect.left()) / rect.width() * 2.0 - 1.0;
        let ndc_y = 1.0 - (p.y - rect.top()) / rect.height() * 2.0;
        let inv = self.view_proj(rect.width() / rect.height()).inverse();
        let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, -1.0));
        let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (self.pos.as_dvec3(), (far - near).normalize().as_dvec3())
    }

    pub fn look_at(&mut self, target: Vec3) {
        let d = (target - self.pos).normalize_or_zero();
        self.yaw = d.y.atan2(d.x).to_degrees();
        self.pitch = d.z.asin().to_degrees();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Textured,
    Flat,
    Wireframe,
    LightmapGrid,
    Smoothing,
    Lighting,
}

impl RenderMode {
    pub const ALL: [RenderMode; 6] = [RenderMode::Textured, RenderMode::Flat, RenderMode::Wireframe, RenderMode::LightmapGrid, RenderMode::Smoothing, RenderMode::Lighting];
    pub fn label(self) -> &'static str {
        match self {
            RenderMode::Textured => crate::i18n::tr("Texturiert"),
            RenderMode::Flat => crate::i18n::tr("Einfarbig"),
            RenderMode::Wireframe => crate::i18n::tr("Drahtgitter"),
            RenderMode::LightmapGrid => crate::i18n::tr("Lightmap-Dichte"),
            RenderMode::Smoothing => crate::i18n::tr("Smoothing-Gruppen"),
            RenderMode::Lighting => crate::i18n::tr("Lichtvorschau"),
        }
    }
    fn shader_id(self) -> i32 {
        match self {
            RenderMode::Textured | RenderMode::Wireframe => 0,
            RenderMode::Flat => 1,
            RenderMode::LightmapGrid => 2,
            RenderMode::Smoothing => 3,
            RenderMode::Lighting => 4,
        }
    }
}

// ------------------------------------------------------------------ Mesh-Daten

/// pos(3) normal(3) farbe(4) uv(2) lightmap(2) aux(2: smoothing, alpha)
const STRIDE: usize = 16;

#[derive(Clone, Debug)]
pub struct Batch {
    pub tex: Option<String>,
    pub first: i32,
    pub count: i32,
    /// Grenzen (min xyz, max xyz) für Frustum-Culling; None = immer zeichnen
    pub bounds: Option<[f32; 6]>,
}

/// Sichtpyramide aus der Kameramatrix (6 Ebenen, nach innen zeigend)
pub fn frustum_planes(mvp: &Mat4) -> [glam::Vec4; 6] {
    let (r0, r1, r2, r3) = (mvp.row(0), mvp.row(1), mvp.row(2), mvp.row(3));
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2]
}

/// Liegt die Box (teilweise) in der Sichtpyramide?
pub fn box_visible(planes: &[glam::Vec4; 6], b: &[f32; 6]) -> bool {
    planes.iter().all(|p| {
        let x = if p.x >= 0.0 { b[3] } else { b[0] };
        let y = if p.y >= 0.0 { b[4] } else { b[1] };
        let z = if p.z >= 0.0 { b[5] } else { b[2] };
        p.x * x + p.y * y + p.z * z + p.w >= 0.0
    })
}

#[derive(Clone, Copy, Default)]
pub struct LightInfo {
    pub pos: Vec3,
    pub color: Vec3,
    pub spot: Option<(Vec3, f32, f32)>,
}

#[derive(Default)]
pub struct MeshData {
    pub opaque: Vec<f32>,
    pub opaque_batches: Vec<Batch>,
    pub trans: Vec<f32>,
    pub trans_batches: Vec<Batch>,
    pub sky: Vec<f32>,
    pub sky_batches: Vec<Batch>,
    pub lines: Vec<f32>,
    pub overlay_lines: Vec<f32>,
    pub wire: Vec<f32>,
    pub statics: Vec<(String, Vec<f32>, Vec<Batch>)>,
}

#[derive(Clone, Copy, Default)]
struct V {
    p: [f32; 3],
    n: [f32; 3],
    c: [f32; 4],
    uv: [f32; 2],
    lm: [f32; 2],
    aux: [f32; 2],
}

fn push(buf: &mut Vec<f32>, v: V) {
    buf.extend_from_slice(&v.p);
    buf.extend_from_slice(&v.n);
    buf.extend_from_slice(&v.c);
    buf.extend_from_slice(&v.uv);
    buf.extend_from_slice(&v.lm);
    buf.extend_from_slice(&v.aux);
}

fn f3(p: DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// Deckkraft abgedunkelter 2D-Objekte (nicht auf der Ebene der Auswahl)
pub const DIM_ALPHA: f32 = 0.22;

/// 2D-Linie, die die Grenzen ihres Objekts mitträgt (für das Abdunkeln anderer Ebenen)
pub(crate) fn line_b(buf: &mut Vec<f32>, a: DVec3, b: DVec3, c: [f32; 4], bb: &crate::geometry::Aabb) {
    let (mn, mx) = (f3(bb.min), f3(bb.max));
    for p in [a, b] {
        push(buf, V { p: f3(p), n: mn, c, uv: [mx[0], mx[1]], lm: [mx[2], 1.0], ..Default::default() });
    }
}

pub(crate) fn line(buf: &mut Vec<f32>, a: DVec3, b: DVec3, c: [f32; 4]) {
    push(buf, V { p: f3(a), c, ..Default::default() });
    push(buf, V { p: f3(b), c, ..Default::default() });
}

/// Sammelt Dreiecke nach Material und fügt sie zu einem Puffer + Batches zusammen
/// Kantenlänge der Blöcke, in die die Geometrie für das Frustum-Culling aufgeteilt wird
const CHUNK: f64 = 1024.0;

type ChunkKey = Option<(i32, i32, i32)>;

#[derive(Default)]
struct Bucketed {
    groups: Vec<(Option<String>, ChunkKey, Vec<f32>, Option<[f32; 6]>)>,
    index: HashMap<(Option<String>, ChunkKey), usize>,
}

impl Bucketed {
    fn group(&mut self, key: Option<String>, chunk: ChunkKey) -> usize {
        match self.index.get(&(key.clone(), chunk)) {
            Some(i) => *i,
            None => {
                self.groups.push((key.clone(), chunk, Vec::new(), None));
                self.index.insert((key, chunk), self.groups.len() - 1);
                self.groups.len() - 1
            }
        }
    }

    /// Puffer ohne Culling (immer gezeichnet)
    fn buf(&mut self, key: Option<String>) -> &mut Vec<f32> {
        let i = self.group(key, None);
        &mut self.groups[i].2
    }

    /// Puffer für Geometrie innerhalb von `b` (wird je Block gesammelt und kann ausgeblendet werden)
    fn buf_at(&mut self, key: Option<String>, b: &Aabb) -> &mut Vec<f32> {
        let c = b.center() / CHUNK;
        let i = self.group(key, Some((c.x.floor() as i32, c.y.floor() as i32, c.z.floor() as i32)));
        let g = &mut self.groups[i];
        let nb = [b.min.x as f32, b.min.y as f32, b.min.z as f32, b.max.x as f32, b.max.y as f32, b.max.z as f32];
        g.3 = Some(match g.3 {
            None => nb,
            Some(o) => [o[0].min(nb[0]), o[1].min(nb[1]), o[2].min(nb[2]), o[3].max(nb[3]), o[4].max(nb[4]), o[5].max(nb[5])],
        });
        &mut g.2
    }

    fn finish(mut self) -> (Vec<f32>, Vec<Batch>) {
        // nach Textur sortieren, damit Texturwechsel selten bleiben
        self.groups.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut data = Vec::with_capacity(self.groups.iter().map(|g| g.2.len()).sum());
        let mut batches = Vec::new();
        for (tex, chunk, d, bounds) in self.groups {
            let first = (data.len() / STRIDE) as i32;
            let count = (d.len() / STRIDE) as i32;
            data.extend_from_slice(&d);
            batches.push(Batch { tex, first, count, bounds: chunk.and(bounds) });
        }
        (data, batches)
    }
}

/// Darstellungsfarbe eines Materials (solange keine echten Texturen geladen sind)
pub fn material_color(mat: &str) -> [f32; 4] {
    let m = mat.to_ascii_lowercase();
    if m.contains("toolsnodraw") {
        [0.85, 0.75, 0.25, 1.0]
    } else if m.contains("toolsskybox") || m.contains("toolsskycamera") {
        [0.45, 0.72, 0.95, 1.0]
    } else if m.contains("toolstrigger") {
        [1.0, 0.55, 0.1, 0.45]
    } else if m.contains("clip") && m.starts_with("tools") {
        [0.75, 0.3, 0.9, 0.45]
    } else if m.contains("toolsinvisible") || m.contains("toolshint") || m.contains("toolsskip") || m.contains("toolsareaportal") || m.contains("toolsblock") {
        [0.6, 0.6, 0.7, 0.35]
    } else if m.contains("toolsblack") {
        [0.05, 0.05, 0.05, 1.0]
    } else if m.contains("dev_measuregeneric") {
        [0.95, 0.55, 0.2, 1.0]
    } else if m.contains("graygrid") || m.contains("dev_measurewall") || m.contains("dev_measurecrate") {
        [0.62, 0.62, 0.62, 1.0]
    } else if m.contains("water") {
        [0.25, 0.45, 0.65, 0.7]
    } else {
        let mut h: u32 = 2166136261;
        for b in m.bytes() {
            h = (h ^ b as u32).wrapping_mul(16777619);
        }
        let (r, g, b) = crate::map::hsv((h % 360) as f32, 0.3, 0.8);
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }
}

fn tint(c: [f32; 4], t: [f32; 3], a: f32) -> [f32; 4] {
    [c[0] * (1.0 - a) + t[0] * a, c[1] * (1.0 - a) + t[1] * a, c[2] * (1.0 - a) + t[2] * a, c[3].max(0.6)]
}

fn box_faces(buf: &mut Vec<f32>, b: &Aabb, c: [f32; 4]) {
    let k = b.corners();
    let quads: [([usize; 4], DVec3); 6] = [
        ([4, 5, 6, 7], DVec3::Z),
        ([0, 3, 2, 1], -DVec3::Z),
        ([1, 2, 6, 5], DVec3::X),
        ([0, 4, 7, 3], -DVec3::X),
        ([3, 7, 6, 2], DVec3::Y),
        ([0, 1, 5, 4], -DVec3::Y),
    ];
    for (q, n) in quads {
        for i in [0, 1, 2, 0, 2, 3] {
            push(buf, V { p: f3(k[q[i]]), n: f3(n), c, ..Default::default() });
        }
    }
}

fn box_lines(buf: &mut Vec<f32>, corners: &[DVec3; 8], c: [f32; 4]) {
    for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
        line(buf, corners[i], corners[j], c);
    }
}

fn smoothing_value(s: &str) -> f32 {
    s.trim().parse::<u32>().unwrap_or(0) as f32
}

/// Dreiecke einer Brush-Fläche (inkl. Displacement) in einen Puffer schreiben
#[allow(clippy::too_many_arguments)]
fn face_triangles(buf: &mut Vec<f32>, wire: Option<&mut Vec<f32>>, s: &Solid, i: usize, c: [f32; 4], tw: f64, th: f64, smooth: f32) {
    let side = &s.sides[i];
    let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
    let lms = side.lightmapscale.max(1) as f64;
    let (ua, va) = (side.uaxis.axis.normalize_or_zero(), side.vaxis.axis.normalize_or_zero());
    let uv = |p: DVec3| -> [f32; 2] { [((p.dot(side.uaxis.axis) / side.uaxis.scale.max(1e-4) + side.uaxis.shift) / tw) as f32, ((p.dot(side.vaxis.axis) / side.vaxis.scale.max(1e-4) + side.vaxis.shift) / th) as f32] };
    let lm = |p: DVec3| -> [f32; 2] { [(p.dot(ua) / lms) as f32, (p.dot(va) / lms) as f32] };
    if let Some(Some(g)) = s.disps.get(i) {
        let normals = g.vertex_normals(n);
        let alphas: Vec<f32> = side.disp.as_ref().map(|d| d.alphas.iter().map(|a| (*a / 255.0) as f32).collect()).unwrap_or_default();
        for t in g.triangles() {
            for k in t {
                push(buf, V { p: f3(g.pos[k]), n: f3(normals[k]), c, uv: uv(g.base[k]), lm: lm(g.base[k]), aux: [smooth, *alphas.get(k).unwrap_or(&0.0)] });
            }
        }
        if let Some(w) = wire {
            let r = g.res;
            for a in 0..r {
                for b in 0..r {
                    let k = a * r + b;
                    if b + 1 < r {
                        line(w, g.pos[k], g.pos[k + 1], [0.8, 0.8, 0.8, 1.0]);
                    }
                    if a + 1 < r {
                        line(w, g.pos[k], g.pos[k + r], [0.8, 0.8, 0.8, 1.0]);
                    }
                }
            }
        }
        return;
    }
    let poly = &s.polys[i];
    for k in 1..poly.len() - 1 {
        for p in [poly[0], poly[k], poly[k + 1]] {
            push(buf, V { p: f3(p), n: f3(n), c, uv: uv(p), lm: lm(p), aux: [smooth, 0.0] });
        }
    }
    if let Some(w) = wire {
        for k in 0..poly.len() {
            line(w, poly[k], poly[(k + 1) % poly.len()], [0.85, 0.85, 0.85, 1.0]);
        }
    }
}

/// Ebenen-Quad (Decals, Overlays, Sprites) als texturierte Dreiecke
/// Nebel aus dem ersten aktiven env_fog_controller: (Farbe linear, Beginn, Ende, Dichte)
pub fn fog_settings(ed: &Editor) -> Option<([f32; 3], f32, f32, f32)> {
    let e = ed.map.entities.iter().find(|e| e.classname().eq_ignore_ascii_case("env_fog_controller") && e.get("fogenable").is_some_and(|v| v.trim() == "1"))?;
    let num = |k: &str, d: f32| e.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
    let col = e.get("fogcolor").and_then(parse_vec).unwrap_or(DVec3::splat(255.0)) / 255.0;
    let (start, end) = (num("fogstart", 500.0), num("fogend", 2000.0));
    let dens = num("fogmaxdensity", 1.0).clamp(0.0, 1.0);
    (end > start && dens > 0.0).then_some(([col.x as f32, col.y as f32, col.z as f32], start, end, dens))
}

/// 2D-Himmel aus worldspawn „skyname“: sechs Flächen eines Würfels um die Kamera
/// (Seitenzuordnung wie in der Engine: rt = +X, lf = −X, bk = +Y, ft = −Y, up, dn)
fn sky2d_geometry(ed: &Editor, mats: &mut MaterialSystem, cam: DVec3) -> (Vec<f32>, Vec<Batch>) {
    let name = ed.map.world.get("skyname").map(str::trim).filter(|s| !s.is_empty()).unwrap_or("sky_day01_01").to_ascii_lowercase();
    let mut b = Bucketed::default();
    let r = 1000.0;
    // Richtung aus (s, t) ∈ [−1, 1]² je Seite
    type F = fn(f64, f64) -> DVec3;
    let faces: [(&str, F); 6] = [
        ("rt", |s, t| DVec3::new(1.0, -s, t)),
        ("lf", |s, t| DVec3::new(-1.0, s, t)),
        ("bk", |s, t| DVec3::new(s, 1.0, t)),
        ("ft", |s, t| DVec3::new(-s, -1.0, t)),
        ("up", |s, t| DVec3::new(-t, -s, 1.0)),
        ("dn", |s, t| DVec3::new(t, -s, -1.0)),
    ];
    for (suf, f) in faces {
        let mat = format!("skybox/{name}{suf}");
        if mats.request_gl(&mat).is_none() {
            continue;
        }
        let st = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)];
        let c = st.map(|(s, t)| cam + f(s, t) * r);
        // u = (s + 1) / 2, v = (1 − t) / 2; leicht nach innen, damit keine Kanten aufblitzen
        let uv = st.map(|(s, t)| [((s * 0.998 + 1.0) / 2.0) as f32, ((1.0 - t * 0.998) / 2.0) as f32]);
        quad(b.buf(Some(normalize(&mat))), c, uv, -f(0.0, 0.0), [1.0; 4]);
    }
    b.finish()
}

/// Ersatztextur für Partikel ohne geladenes Material: weiche Scheibe
const SOFT_DISC: &str = "__amboss_soft_disc";

unsafe fn soft_disc_texture(gl: &glow::Context) -> Result<glow::Texture, String> {
    let n = 64usize;
    let mut px = vec![255u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 + 0.5 - n as f32 / 2.0).hypot(y as f32 + 0.5 - n as f32 / 2.0) / (n as f32 / 2.0)).min(1.0);
            px[(y * n + x) * 4 + 3] = ((1.0 - d).powf(1.5) * 255.0) as u8;
        }
    }
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, n as i32, n as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&px)));
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.bind_texture(glow::TEXTURE_2D, None);
    Ok(t)
}

fn quad(buf: &mut Vec<f32>, c: [DVec3; 4], uv: [[f32; 2]; 4], n: DVec3, col: [f32; 4]) {
    for i in [0, 1, 2, 0, 2, 3] {
        push(buf, V { p: f3(c[i]), n: f3(n), c: col, uv: uv[i], ..Default::default() });
    }
}

pub struct BuildOpts {
    pub textured: bool,
    pub mode: RenderMode,
    pub sky_preview: bool,
    pub helpers: bool,
    /// Ansichtsfilter (nodraw- und Skybox-Flächen einzeln auslassen)
    pub vis: crate::editor::AutoVis,
}

/// Skybox-Raum um die sky_camera: (Position, Maßstab, Grenzen)
fn sky_room(ed: &Editor) -> Option<(DVec3, f64, Aabb)> {
    crate::skybox::sky_room(&ed.map).map(|r| (r.origin, r.scale, r.bounds))
}

pub fn sky_matrix(ed: &Editor) -> Option<Mat4> {
    let (o, scale, _) = sky_room(ed)?;
    Some(Mat4::from_scale(Vec3::splat(scale as f32)) * Mat4::from_translation(-o.as_vec3()))
}

/// Zwischenspeicher für die Dreiecke einzelner Brushes (Teil-Neubau des 3D-Netzes)
#[derive(Default)]
pub struct SolidCache {
    solids: HashMap<u32, CachedSolid>,
}

#[derive(Default)]
struct CachedSolid {
    /// gemeinsam genutzte Kopie: ändert sich der Brush, zeigt die Map auf ein neues Objekt
    solid: Option<Shared<Solid>>,
    state: u64,
    face_mask: u64,
    /// Material-Stand beim Bauen (nur relevant, solange nicht alle Texturen geladen waren)
    gen: u64,
    complete: bool,
    /// (Ziel 0 = deckend, 1 = transparent, 2 = Skybox-Vorschau, 3 = Drahtgitter; Material; Vertizes)
    parts: Vec<(u8, Option<String>, Vec<f32>)>,
    overlay: Vec<f32>,
}

/// Dreiecke und Umrisslinien eines Brushes
#[allow(clippy::too_many_arguments)]
fn build_solid(s: &Solid, mats: &mut MaterialSystem, opts: &BuildOpts, selected: bool, in_sky: bool, sky_preview: bool, face_mask: u64, faces: &std::collections::BTreeSet<(u32, usize)>) -> CachedSolid {
    let sel_tint = [1.0, 0.1, 0.1];
    let wireframe = opts.mode == RenderMode::Wireframe;
    let flat = opts.mode == RenderMode::Flat;
    let mut out = CachedSolid { complete: true, ..Default::default() };
    let part = |out: &mut CachedSolid, target: u8, key: Option<String>| -> usize {
        match out.parts.iter().position(|(t, k, _)| *t == target && *k == key) {
            Some(i) => i,
            None => {
                out.parts.push((target, key, Vec::new()));
                out.parts.len() - 1
            }
        }
    };
    let has_disp = s.has_disp();
    for (i, poly) in s.polys.iter().enumerate() {
        if poly.len() < 3 || (has_disp && s.sides[i].disp.is_none()) {
            continue;
        }
        let side = &s.sides[i];
        if opts.vis.hides_face(&side.material) {
            continue;
        }
        let face_sel = if i < 64 { face_mask & (1 << i) != 0 } else { faces.contains(&(s.id, i)) };
        let base = material_color(&side.material);
        let is_sky_mat = side.material.to_ascii_lowercase().contains("toolsskybox");
        let info = if opts.textured && !flat { mats.request_gl(&side.material) } else { None };
        if opts.textured && !flat && info.is_none() && !mats.gl_settled(&side.material) {
            out.complete = false;
        }
        let (mut c, key) = match info {
            Some(i) => ([1.0, 1.0, 1.0, base[3] * i.alpha], Some(normalize(&side.material))),
            None => (base, None),
        };
        let translucent = c[3] < 0.99 || info.is_some_and(|i| i.translucent);
        let (tw, th) = info.map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0));
        let smooth = smoothing_value(&side.smoothing_groups);
        // in der Skybox zeigt das Spiel keine Werkzeug-Texturen (nodraw, clip, trigger …)
        if in_sky && !translucent && !is_sky_mat && crate::editor::tool_kind(&side.material).is_none() {
            let p = part(&mut out, 2, key.clone());
            face_triangles(&mut out.parts[p].2, None, s, i, c, tw, th, smooth);
        }
        if is_sky_mat && sky_preview {
            continue; // Skybox-Flächen ausblenden, damit die 3D-Skybox sichtbar ist
        }
        if face_sel {
            c = tint(c, sel_tint, 0.55);
        } else if selected {
            c = tint(c, sel_tint, 0.4);
        }
        if wireframe {
            let p = part(&mut out, 3, None);
            face_triangles(&mut Vec::new(), Some(&mut out.parts[p].2), s, i, c, tw, th, smooth);
        } else {
            let p = part(&mut out, if translucent { 1 } else { 0 }, key);
            face_triangles(&mut out.parts[p].2, None, s, i, c, tw, th, smooth);
        }
        if selected || face_sel {
            let lc = if face_sel { [1.0, 1.0, 0.0, 1.0] } else { [1.0, 0.2, 0.2, 1.0] };
            if let Some(Some(g)) = s.disps.get(i) {
                let r = g.res;
                for a in [0, r - 1] {
                    for b in 0..r - 1 {
                        line(&mut out.overlay, g.pos[a * r + b], g.pos[a * r + b + 1], lc);
                        line(&mut out.overlay, g.pos[b * r + a], g.pos[(b + 1) * r + a], lc);
                    }
                }
            } else {
                for k in 0..poly.len() {
                    line(&mut out.overlay, poly[k], poly[(k + 1) % poly.len()], lc);
                }
            }
        }
    }
    out
}

pub fn build_mesh(ed: &Editor, mats: &mut MaterialSystem, opts: &BuildOpts, cache: &mut SolidCache) -> MeshData {
    let mut m = MeshData::default();
    let mut opaque = Bucketed::default();
    let mut trans = Bucketed::default();
    let mut sky = Bucketed::default();
    let sel_tint = [1.0, 0.1, 0.1];
    let wireframe = opts.mode == RenderMode::Wireframe;
    let flat = opts.mode == RenderMode::Flat;
    let sky_box = if opts.sky_preview { sky_room(ed).map(|s| s.2) } else { None };

    // Brushes: unveränderte Brushes kommen aus dem Zwischenspeicher (nur geänderte werden neu berechnet)
    let gen = mats.generation;
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::with_capacity(cache.solids.len());
    let mode_key = (opts.textured as u64) | (flat as u64) << 1 | (wireframe as u64) << 2 | (opts.sky_preview as u64) << 3 | opts.vis.face_bits() << 6;
    let world = ed.map.world.solids.iter().map(|s| (None, s));
    let in_ents = ed.map.entities.iter().flat_map(|e| e.solids.iter().map(move |s| (Some(&**e), s)));
    for (e, shared) in world.chain(in_ents) {
        let s: &Solid = shared;
        let owner = e.map(|e| e.id);
        if !ed.is_visible_solid_all(e, s) || !ed.in_cordon(&s.bounds) {
            continue;
        }
        let selected = ed.is_selected_solid(owner, s.id);
        let in_sky = sky_box.is_some_and(|b| b.contains_point(s.bounds.center()));
        let mut face_mask = 0u64;
        if !ed.faces.is_empty() {
            for i in 0..s.polys.len().min(64) {
                if ed.faces.contains(&(s.id, i)) {
                    face_mask |= 1 << i;
                }
            }
        }
        let state = mode_key | (selected as u64) << 4 | (in_sky as u64) << 5;
        seen.insert(s.id);
        let valid = cache.solids.get(&s.id).is_some_and(|c| c.solid.as_ref().is_some_and(|x| Shared::ptr_eq(x, shared)) && c.state == state && c.face_mask == face_mask && (c.complete || c.gen == gen));
        if !valid {
            let built = build_solid(s, mats, opts, selected, in_sky, opts.sky_preview, face_mask, &ed.faces);
            cache.solids.insert(s.id, CachedSolid { solid: Some(shared.clone()), state, face_mask, gen, ..built });
        }
        let c = &cache.solids[&s.id];
        for (target, key, data) in &c.parts {
            let buf = match target {
                0 => opaque.buf_at(key.clone(), &s.bounds),
                1 => trans.buf_at(key.clone(), &s.bounds),
                2 => sky.buf_at(key.clone(), &s.bounds),
                _ => &mut m.wire,
            };
            buf.extend_from_slice(data);
        }
        m.overlay_lines.extend_from_slice(&c.overlay);
    }
    cache.solids.retain(|id, _| seen.contains(id));

    // Punkt-Entities
    let mut ent_buf = Vec::new();
    // Flächen-Index nur bauen, wenn es Overlays gibt
    let mut side_idx: Option<std::collections::HashMap<u32, (u32, usize)>> = None;
    for e in &ed.map.entities {
        if e.is_brush_entity() || !ed.is_visible_entity_all(e) {
            continue;
        }
        let b = ed.entity_bounds(e);
        if !ed.in_cordon(&b) {
            continue;
        }
        let col = ed.entity_color(e);
        let mut c = [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0, 1.0];
        let selected = ed.sel.contains(&Obj::Entity(e.id));
        let class = e.classname();
        let has_model = ed.model_path(e).is_some_and(|p| ed.model_bounds.contains_key(&p)) || ed.map.meshes.iter().any(|m| m.ent == e.id);
        let is_instance = class.eq_ignore_ascii_case("func_instance") && e.get("file").is_some_and(|f| matches!(ed.instances.get(f), Some(Some(_))));
        if selected {
            c = tint(c, sel_tint, 0.5);
            box_lines(&mut m.overlay_lines, &b.corners(), [1.0, 0.2, 0.2, 1.0]);
        }
        let sprite = ed.fgd.class(class).and_then(|d| d.sprite.clone());
        let small = class == "infodecal" || class == "info_overlay";
        if has_model || is_instance {
            // Modelle zeigen sich selbst – die Box nur bei Auswahl (rot, oben)
        } else if sprite.is_some() || small {
            box_lines(&mut m.lines, &b.corners(), [c[0], c[1], c[2], 0.8]);
        } else {
            box_faces(&mut ent_buf, &b, c);
        }
        if let Some(o) = e.origin() {
            if e.get("angles").is_some() && !has_model {
                let (f, _, _) = angles_to_basis(e.angles());
                line(&mut m.lines, o, o + f * 32.0, [1.0, 1.0, 0.0, 1.0]);
            }
            if opts.helpers && class == "light_spot" {
                spot_helper(&mut m.lines, e, o);
            }
        }
        if class == "infodecal" {
            if let (Some(o), Some(tex)) = (e.origin(), e.get("texture")) {
                decal_quad(ed, mats, &mut trans, o, tex, selected);
            }
        }
        if class == "info_overlay" {
            let idx = side_idx.get_or_insert_with(|| crate::overlay_mesh::side_index(&ed.map));
            overlay_mesh(ed, idx, e, mats, &mut trans, selected);
        }
        if is_instance {
            let file = e.get("file").unwrap().to_string();
            let key = format!("inst:{file}");
            if !m.statics.iter().any(|s| s.0 == key) {
                if let Some(Some(inst)) = ed.instances.get(&file) {
                    let (d, bt) = static_from_map(inst, mats, opts.textured && !flat);
                    m.statics.push((key, d, bt));
                }
            }
        }
    }
    opaque.buf(None).extend_from_slice(&ent_buf);

    // Polygon-Meshes (Netz direkt aus der Map, auch vor dem Kompilieren zum Modell)
    for mo in &ed.map.meshes {
        let Some(e) = ed.map.entity(mo.ent) else { continue };
        if !ed.is_visible_entity_all(e) {
            continue;
        }
        let bounds = ed.entity_bounds(e);
        if !ed.in_cordon(&bounds) {
            continue;
        }
        let x = Editor::entity_xform(e);
        let selected = ed.sel.contains(&Obj::Entity(e.id));
        let fsel = ed.mesh_edit.as_ref().filter(|m| m.ent == e.id && m.faces_mode).map(|m| &m.sel);
        for (fi, f) in mo.mesh.faces.iter().enumerate() {
            if f.idx.len() < 3 || f.idx.iter().any(|&i| i >= mo.mesh.verts.len()) {
                continue;
            }
            let base = material_color(&f.material);
            let info = if opts.textured && !flat { mats.request_gl(&f.material) } else { None };
            let (mut c, key) = match info {
                Some(i) => ([1.0, 1.0, 1.0, base[3] * i.alpha], Some(normalize(&f.material))),
                None => (base, None),
            };
            let translucent = c[3] < 0.99 || info.is_some_and(|i| i.translucent);
            let (tw, th) = info.map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0));
            if fsel.is_some_and(|s| s.contains(&fi)) {
                c = tint(c, [1.0, 0.85, 0.1], 0.55);
            } else if selected {
                c = tint(c, sel_tint, 0.3);
            }
            let ln = mo.mesh.normal(fi);
            let n = x.m * ln;
            let an = ln.abs();
            // planar nach Hauptachse, Maßstab 0,25 (wie beim Kompilieren)
            let uv = |p: DVec3| -> [f32; 2] {
                let (a, b) = if an.z >= an.x && an.z >= an.y { (p.x, -p.y) } else if an.x >= an.y { (p.y, -p.z) } else { (p.x, -p.z) };
                [(a / 0.25 / tw) as f32, (b / 0.25 / th) as f32]
            };
            if wireframe {
                for k in 0..f.idx.len() {
                    line(&mut m.wire, x.apply(mo.mesh.vert(f.idx[k])), x.apply(mo.mesh.vert(f.idx[(k + 1) % f.idx.len()])), [0.85, 0.85, 0.85, 1.0]);
                }
                continue;
            }
            let buf = if translucent { trans.buf_at(key, &bounds) } else { opaque.buf_at(key, &bounds) };
            for k in 1..f.idx.len() - 1 {
                for i in [f.idx[0], f.idx[k], f.idx[k + 1]] {
                    let p = mo.mesh.vert(i);
                    let w = x.apply(p);
                    push(buf, V { p: f3(w), n: f3(n), c, uv: uv(p), lm: [(w.x / 16.0) as f32, (w.y / 16.0) as f32], aux: [0.0, 0.0] });
                }
            }
        }
    }

    // Verbindungen ausgewählter Entities
    if opts.helpers {
        for id in ed.selected_entity_ids() {
            let Some(e) = ed.map.entity(id) else { continue };
            let from = ed.entity_bounds(e).center();
            for (_, val) in &e.outputs {
                let target = val.split(['\u{1b}', ',']).next().unwrap_or("");
                for t in ed.map.entities.iter().filter(|x| x.targetname().is_some_and(|n| n.eq_ignore_ascii_case(target))) {
                    line(&mut m.overlay_lines, from, ed.entity_bounds(t).center(), [0.3, 1.0, 0.45, 1.0]);
                }
            }
        }
    }

    // Pointfile & Portale
    for w in ed.pointfile.windows(2) {
        line(&mut m.overlay_lines, w[0], w[1], [1.0, 0.0, 0.0, 1.0]);
    }
    for (b, c) in &ed.diff_boxes {
        let col = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0];
        let k = b.corners();
        for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
            line(&mut m.overlay_lines, k[i], k[j], col);
        }
    }
    for (a, b, ok) in &ed.opt_links {
        let c = if *ok { [0.3, 1.0, 0.45, 0.8] } else { [1.0, 0.25, 0.25, 1.0] };
        line(&mut m.overlay_lines, *a + DVec3::Z * 8.0, *b + DVec3::Z * 8.0, c);
    }
    for mk in &ed.opt_markers {
        let c = [mk.color[0] as f32 / 255.0, mk.color[1] as f32 / 255.0, mk.color[2] as f32 / 255.0, 1.0];
        for d in [DVec3::X, DVec3::Y, DVec3::Z] {
            line(&mut m.overlay_lines, mk.pos - d * 12.0, mk.pos + d * 12.0, c);
        }
    }
    for p in &ed.portals {
        for k in 0..p.len() {
            line(&mut m.lines, p[k], p[(k + 1) % p.len()], [0.2, 0.9, 1.0, 0.5]);
        }
    }

    // Bodenraster (folgt dem Raster ab 64 Einheiten)
    let step = ed.grid.max(64.0);
    let ext = (step * 32.0).min(8192.0);
    let mut x = -ext;
    while x <= ext {
        let c = if x == 0.0 { [0.2, 0.6, 0.6, 0.7] } else { [0.35, 0.35, 0.4, 0.3] };
        line(&mut m.lines, DVec3::new(x, -ext, 0.0), DVec3::new(x, ext, 0.0), c);
        line(&mut m.lines, DVec3::new(-ext, x, 0.0), DVec3::new(ext, x, 0.0), c);
        x += step;
    }
    let (d, b) = opaque.finish();
    m.opaque = d;
    m.opaque_batches = b;
    let (d, b) = trans.finish();
    m.trans = d;
    m.trans_batches = b;
    let (d, b) = sky.finish();
    m.sky = d;
    m.sky_batches = b;
    m
}

fn spot_helper(lines: &mut Vec<f32>, e: &Entity, o: DVec3) {
    let mut a = e.angles();
    if let Some(p) = e.get("pitch").and_then(|p| p.parse::<f64>().ok()) {
        a.x = -p;
    }
    let (f, l, u) = angles_to_basis(a);
    let cone = e.get("_cone").and_then(|c| c.parse::<f64>().ok()).unwrap_or(45.0).to_radians();
    let len = 96.0;
    let r = len * cone.tan();
    let c = [1.0, 1.0, 0.4, 0.8];
    let mut prev = None;
    for i in 0..=16 {
        let ang = std::f64::consts::TAU * i as f64 / 16.0;
        let p = o + f * len + (l * ang.cos() + u * ang.sin()) * r;
        if i % 4 == 0 {
            line(lines, o, p, c);
        }
        if let Some(q) = prev {
            line(lines, q, p, c);
        }
        prev = Some(p);
    }
}

fn plane_basis(n: DVec3) -> (DVec3, DVec3) {
    let mut u = n.cross(DVec3::Z);
    if u.length() < 0.1 {
        u = DVec3::X;
    }
    let u = u.normalize();
    (u, n.cross(u).normalize())
}

fn decal_quad(ed: &Editor, mats: &mut MaterialSystem, trans: &mut Bucketed, o: DVec3, tex: &str, selected: bool) {
    let mut best: Option<(f64, DVec3, DVec3)> = None;
    for d in [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::Z] {
        for s in &ed.map.world.solids {
            if let Some((t, i)) = s.ray_hit(o - d * 0.5, d) {
                if t < 32.0 && best.map_or(true, |b| t < b.0) {
                    let n = s.planes[i].map(|p| p.normal).unwrap_or(-d);
                    best = Some((t, o - d * 0.5 + d * t, n));
                }
            }
        }
    }
    let Some((_, p, n)) = best else { return };
    let info = mats.request_gl(tex);
    let (w, h) = info.map(|i| (i.width as f64, i.height as f64)).unwrap_or((64.0, 64.0));
    let (u, v) = plane_basis(n);
    let c = p + n * 0.6;
    let corners = [c - u * w / 2.0 - v * h / 2.0, c - u * w / 2.0 + v * h / 2.0, c + u * w / 2.0 + v * h / 2.0, c + u * w / 2.0 - v * h / 2.0];
    let col = if selected { [1.0, 0.6, 0.6, 1.0] } else { [1.0; 4] };
    quad(trans.buf(Some(normalize(tex))), corners, [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], n, col);
}

/// Overlay wie im Spiel: auf die Flächen aus `sides` projiziert (folgt Displacements), sonst Rechteck
fn overlay_mesh(ed: &Editor, idx: &std::collections::HashMap<u32, (u32, usize)>, e: &Entity, mats: &mut MaterialSystem, trans: &mut Bucketed, selected: bool) {
    match crate::overlay_mesh::build(&ed.map, idx, e) {
        Some(tris) if !tris.is_empty() => {
            let mat = e.get("material").unwrap_or("");
            mats.request_gl(mat);
            let col = if selected { [1.0, 0.6, 0.6, 1.0] } else { [1.0; 4] };
            let buf = trans.buf(Some(normalize(mat)));
            for t in tris {
                for v in t {
                    push(buf, V { p: f3(v.p), n: f3(v.n), c: col, uv: v.uv, ..Default::default() });
                }
            }
        }
        // Flächen vorhanden, Overlay liegt aber ganz daneben: nichts zeichnen (wie im Spiel)
        Some(_) => {}
        None => overlay_quad(e, mats, trans, selected),
    }
}

fn overlay_quad(e: &Entity, mats: &mut MaterialSystem, trans: &mut Bucketed, selected: bool) {
    let g = |k: &str| e.get(k).and_then(parse_vec);
    let (Some(o), Some(u), Some(v), Some(n)) = (g("BasisOrigin"), g("BasisU"), g("BasisV"), g("BasisNormal")) else { return };
    let uvp: Vec<DVec3> = (0..4).filter_map(|i| g(&format!("uv{i}"))).collect();
    if uvp.len() != 4 {
        return;
    }
    let f = |k: &str, d: f32| e.get(k).and_then(|s| s.parse::<f32>().ok()).unwrap_or(d);
    let (su, eu, sv, ev) = (f("StartU", 0.0), f("EndU", 1.0), f("StartV", 0.0), f("EndV", 1.0));
    let corners = [0, 1, 2, 3].map(|i| o + u * uvp[i].x + v * uvp[i].y + n * 0.6);
    let mat = e.get("material").unwrap_or("");
    mats.request_gl(mat);
    let col = if selected { [1.0, 0.6, 0.6, 1.0] } else { [1.0; 4] };
    quad(trans.buf(Some(normalize(mat))), corners, [[su, sv], [su, ev], [eu, ev], [eu, sv]], n, col);
}

/// Statisches Mesh einer Instanz-Map
fn static_from_map(m: &Map, mats: &mut MaterialSystem, textured: bool) -> (Vec<f32>, Vec<Batch>) {
    let mut b = Bucketed::default();
    for (_, s) in m.all_solids() {
        let has_disp = s.has_disp();
        for (i, poly) in s.polys.iter().enumerate() {
            if poly.len() < 3 || (has_disp && s.sides[i].disp.is_none()) {
                continue;
            }
            let side = &s.sides[i];
            let base = material_color(&side.material);
            if base[3] < 0.99 {
                continue;
            }
            let info = if textured { mats.request_gl(&side.material) } else { None };
            let (c, key) = match info {
                Some(_) => ([1.0, 1.0, 1.0, 1.0], Some(normalize(&side.material))),
                None => (base, None),
            };
            let (tw, th) = info.map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0));
            face_triangles(b.buf(key), None, s, i, c, tw, th, 0.0);
        }
    }
    b.finish()
}

/// Statisches Mesh eines Modells (eine Skin-Familie)
pub fn static_from_model(model: &crate::mdl::Model, skin: usize) -> (Vec<f32>, Vec<Batch>) {
    let mut b = Bucketed::default();
    for mesh in &model.meshes {
        let mat = model.material_for(skin, mesh.skinref).map(normalize);
        let buf = b.buf(mat);
        for idx in &mesh.indices {
            let v = mesh.verts[*idx as usize];
            push(buf, V { p: [v[0], v[1], v[2]], n: [v[3], v[4], v[5]], c: [0.8, 0.8, 0.8, 1.0], uv: [v[6], v[7]], ..Default::default() });
        }
    }
    b.finish()
}

// ------------------------------------------------------------------ GL-Renderer

struct GpuBuffer {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    count: i32,
}

struct StaticMesh {
    buf: GpuBuffer,
    batches: Vec<Batch>,
}

#[derive(Clone)]
pub struct StaticDraw {
    pub key: String,
    pub model: Mat4,
    pub tint: [f32; 4],
    pub sky: bool,
}

#[derive(Default)]
pub struct FrameData {
    pub mvp: Mat4,
    pub cam: Vec3,
    pub view_mode: i32,
    pub show_alpha: bool,
    pub lights: Vec<LightInfo>,
    pub ambient: Vec3,
    pub sun_dir: Vec3,
    pub sun_col: Vec3,
    pub sky: Option<Mat4>,
    pub draws: Vec<StaticDraw>,
    pub dyn_lines: Vec<f32>,
    pub dyn_quads: Vec<f32>,
    pub dyn_batches: Vec<Batch>,
    /// aufaddierte Flächen (Partikel mit $additive)
    pub add_quads: Vec<f32>,
    pub add_batches: Vec<Batch>,
    /// 2D-Himmel (skyname) als Würfel um die Kamera
    pub sky2d: Vec<f32>,
    pub sky2d_batches: Vec<Batch>,
    /// Nebel: Farbe (linear 0..1), Beginn, Ende, Dichte
    pub fog: Option<([f32; 3], f32, f32, f32)>,
    pub wireframe: bool,
    pub clear: [f32; 3],
    /// Lichtvorschau mit Schatten
    pub shadows: bool,
    /// Sondenraster der globalen Beleuchtung (nur Lichtvorschau)
    pub gi: Option<Arc<crate::gi::Grid>>,
    /// Skybox-Modus: Geometrie nur innerhalb dieser Box
    pub clip: Option<Aabb>,
}

struct Uniforms {
    mvp: Option<glow::UniformLocation>,
    model: Option<glow::UniformLocation>,
    mode: Option<glow::UniformLocation>,
    pass: Option<glow::UniformLocation>,
    cam: Option<glow::UniformLocation>,
    tex: Option<glow::UniformLocation>,
    tex2: Option<glow::UniformLocation>,
    blend: Option<glow::UniformLocation>,
    view: Option<glow::UniformLocation>,
    tint: Option<glow::UniformLocation>,
    dim: Option<glow::UniformLocation>,
    show_alpha: Option<glow::UniformLocation>,
    nlights: Option<glow::UniformLocation>,
    lpos: Option<glow::UniformLocation>,
    lcol: Option<glow::UniformLocation>,
    ldir: Option<glow::UniformLocation>,
    ambient: Option<glow::UniformLocation>,
    sun_dir: Option<glow::UniformLocation>,
    sun_col: Option<glow::UniformLocation>,
    tm0: Option<glow::UniformLocation>,
    tm1: Option<glow::UniformLocation>,
    shadows: Option<glow::UniformLocation>,
    sun_vp: Option<glow::UniformLocation>,
    sun_bias: Option<glow::UniformLocation>,
    cube_far: Option<glow::UniformLocation>,
    gi: Option<glow::UniformLocation>,
    gi_min: Option<glow::UniformLocation>,
    gi_size: Option<glow::UniformLocation>,
    amb_scale: Option<glow::UniformLocation>,
    fog_on: Option<glow::UniformLocation>,
    fog: Option<glow::UniformLocation>,
    fog_range: Option<glow::UniformLocation>,
    unlit: Option<glow::UniformLocation>,
    clip: Option<glow::UniformLocation>,
    clip_min: Option<glow::UniformLocation>,
    clip_max: Option<glow::UniformLocation>,
}

/// Zusatzdaten einer Textur: Animation, $basetexturetransform, Scrollen
#[derive(Clone, Copy)]
struct TexMeta {
    frames: usize,
    fps: f32,
    xform: [f32; 6],
    scroll: (f32, f32),
}

pub struct Renderer {
    program: glow::Program,
    u: Uniforms,
    /// 0 opaque, 1 trans, 2 lines, 3 overlay, 4 wire, 5 sky, 6 dyn lines, 7 dyn quads
    bufs: Vec<GpuBuffer>,
    batches: [Vec<Batch>; 3],
    pending: Option<MeshData>,
    statics: HashMap<String, StaticMesh>,
    pending_statics: Vec<(String, Vec<f32>, Vec<Batch>)>,
    textures: HashMap<String, glow::Texture>,
    tex_meta: HashMap<String, TexMeta>,
    pending_tex: Vec<crate::materials::TexData>,
    start: std::time::Instant,
    /// im letzten Bild wurde eine animierte/laufende Textur gezeichnet → weiter neu zeichnen
    animated: std::cell::Cell<bool>,
    /// 2D-Linien (alle nicht ausgewählten Brushes/Props in Weltkoordinaten), für alle drei 2D-Ansichten
    lines2d: Option<GpuBuffer>,
    pending_lines2d: Option<Vec<f32>>,
    /// zählt hoch, wenn sich Geometrie ändert (Schattenkarten neu)
    mesh_gen: u64,
    shadow: Option<Shadows>,
    /// Sonden-Texturen der globalen Beleuchtung und hochgeladene Version
    gi_tex: Option<[glow::Texture; 3]>,
    gi_version: (u64, [usize; 3]),
}

const MAX_LIGHTS: usize = 32;

const VS: &str = r#"
in vec3 a_pos;
in vec3 a_normal;
in vec4 a_color;
in vec2 a_uv;
in vec2 a_lm;
in vec2 a_aux;
uniform mat4 u_mvp;
uniform mat4 u_model;
out vec3 v_pos;
out vec3 v_normal;
out vec4 v_color;
out vec2 v_uv;
out vec2 v_lm;
out vec2 v_aux;
void main() {
    vec4 wp = u_model * vec4(a_pos, 1.0);
    v_pos = wp.xyz;
    v_normal = mat3(u_model) * a_normal;
    v_color = a_color;
    v_uv = a_uv;
    v_lm = a_lm;
    v_aux = a_aux;
    gl_Position = u_mvp * wp;
}
"#;

const FS: &str = r#"
in vec3 v_pos;
in vec3 v_normal;
in vec4 v_color;
in vec2 v_uv;
in vec2 v_lm;
in vec2 v_aux;
uniform int u_mode;
uniform int u_pass;
uniform int u_view;
uniform int u_show_alpha;
uniform vec3 u_cam;
uniform vec4 u_tint;
uniform sampler2D u_tex;
uniform vec3 u_tm0;
uniform vec3 u_tm1;
uniform sampler2D u_tex2;
uniform int u_blend;
// 2D-Linien: (Achse, untere Grenze, obere Grenze, Deckkraft außerhalb); Deckkraft 0 = aus
uniform vec4 u_dim;
uniform int u_nlights;
uniform vec4 u_lpos[32];
uniform vec4 u_lcol[32];
uniform vec4 u_ldir[32];
uniform vec3 u_ambient;
uniform vec3 u_sun_dir;
uniform vec3 u_sun_col;
// Skybox-Modus: nur innerhalb dieser Box zeichnen
uniform int u_clip;
uniform vec3 u_clip_min;
uniform vec3 u_clip_max;
uniform int u_shadows;
uniform mat4 u_sun_vp;
uniform vec2 u_sun_bias;
uniform sampler2D u_sun_map;
uniform samplerCube u_cube0;
uniform samplerCube u_cube1;
uniform samplerCube u_cube2;
uniform samplerCube u_cube3;
uniform vec4 u_cube_far;
uniform int u_gi;
uniform vec3 u_gi_min;
uniform vec3 u_gi_size;
uniform float u_amb_scale;
uniform sampler3D u_gi_r;
uniform sampler3D u_gi_g;
uniform sampler3D u_gi_b;
uniform int u_fog_on;
uniform vec4 u_fog;
uniform vec2 u_fog_range;
uniform int u_unlit;
out vec4 frag;

// Sonnenschatten: Tiefenkarte von oben entlang der Sonnenrichtung, 3×3 weich gefiltert
float sun_shadow(vec3 p, vec3 n) {
    vec4 c = u_sun_vp * vec4(p + n * u_sun_bias.y, 1.0);
    vec3 s = c.xyz / c.w * 0.5 + 0.5;
    if (s.x <= 0.0 || s.x >= 1.0 || s.y <= 0.0 || s.y >= 1.0 || s.z >= 1.0) return 1.0;
    vec2 texel = 1.0 / vec2(textureSize(u_sun_map, 0));
    float lit = 0.0;
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            float d = texture(u_sun_map, s.xy + vec2(float(x), float(y)) * texel).r;
            lit += (s.z - u_sun_bias.x > d) ? 0.0 : 1.0;
        }
    }
    return lit / 9.0;
}

// Punkt-/Spotlicht: Würfel-Tiefenkarte mit Abstand / Reichweite
float cube_shadow(samplerCube m, vec3 lp, float far, vec3 p, vec3 n) {
    if (far <= 0.0) return 1.0;
    vec3 d = p + n * 1.5 - lp;
    float cur = length(d) / far;
    if (cur >= 1.0) return 1.0;
    float closest = texture(m, d).r;
    return (cur - 2.0 / far > closest) ? 0.0 : 1.0;
}

float gridline(vec2 uv) {
    vec2 g = abs(fract(uv - 0.5) - 0.5) / max(fwidth(uv), vec2(1e-4));
    return 1.0 - clamp(min(g.x, g.y), 0.0, 1.0);
}

void main() {
    if (u_clip == 1 && (any(lessThan(v_pos, u_clip_min)) || any(greaterThan(v_pos, u_clip_max)))) discard;
    if (u_mode == 1) {
        vec4 c = v_color;
        // 2D-Linien tragen die Grenzen ihres Objekts (min in a_normal, max in a_uv/a_lm.x, a_lm.y = 1)
        if (u_dim.w > 0.0 && v_lm.y > 0.5) {
            int ax = int(u_dim.x + 0.5);
            float mn = ax == 0 ? v_normal.x : (ax == 1 ? v_normal.y : v_normal.z);
            float mx = ax == 0 ? v_uv.x : (ax == 1 ? v_uv.y : v_lm.x);
            if (mx < u_dim.y || mn > u_dim.z) c.a *= u_dim.w;
        }
        frag = c;
        return;
    }
    vec3 n = normalize(v_normal);
    vec3 base;
    float a = 1.0;
    if (u_mode == 2) {
        vec2 tuv = vec2(dot(u_tm0, vec3(v_uv, 1.0)), dot(u_tm1, vec3(v_uv, 1.0)));
        vec4 t = texture(u_tex, tuv);
        if (u_blend == 1) {
            // WorldVertexTransition: Displacement-Alpha mischt die zweite Textur ein
            t = mix(t, texture(u_tex2, v_uv), clamp(v_aux.y, 0.0, 1.0));
        }
        base = t.rgb * v_color.rgb;
        a = (u_pass == 1) ? t.a * v_color.a : 1.0;
        if (u_pass == 1 && a < 0.02) discard;
    } else {
        base = v_color.rgb;
        a = (u_pass == 1) ? v_color.a : 1.0;
        if (u_mode == 0) {
            vec3 an = abs(n);
            vec2 uv = (an.z >= an.x && an.z >= an.y) ? v_pos.xy : ((an.x >= an.y) ? v_pos.yz : v_pos.xz);
            base *= 1.0 - 0.22 * gridline(uv / 64.0) - 0.07 * gridline(uv / 16.0);
        }
    }
    if (u_view == 3) {
        float sg = v_aux.x;
        base = sg < 0.5 ? vec3(0.55) : 0.5 + 0.5 * cos(6.2831 * (vec3(0.0, 0.33, 0.67) + fract(sg * 0.618)));
    }
    if (u_view == 2) {
        // Lightmap-Dichte: Weltgröße eines Luxels aus den Ableitungen (rot = fein 4, grün = 16, blau = grob 64)
        vec2 dl = vec2(length(dFdx(v_lm)), length(dFdy(v_lm)));
        vec2 dp = vec2(length(dFdx(v_pos)), length(dFdy(v_pos)));
        float lms = max(dp.x, dp.y) / max(max(dl.x, dl.y), 1e-6);
        float t = clamp((log2(max(lms, 0.01)) - 2.0) / 4.0, 0.0, 1.0);
        vec3 heat = t < 0.5 ? mix(vec3(1.0, 0.25, 0.15), vec3(0.25, 0.85, 0.3), t * 2.0) : mix(vec3(0.25, 0.85, 0.3), vec3(0.25, 0.5, 1.0), (t - 0.5) * 2.0);
        base = mix(base, heat, 0.5);
        base = mix(base, vec3(0.0), 0.6 * gridline(v_lm));
    }
    if (u_show_alpha == 1) {
        base = mix(base, vec3(0.2, 0.45, 1.0), v_aux.y * 0.75);
    }
    base = mix(base, u_tint.rgb, u_tint.a);
    vec3 light;
    if (u_unlit == 1) {
        light = vec3(1.0);
    } else if (u_view == 4) {
        float ns = max(dot(n, -u_sun_dir), 0.0);
        float ss = (u_shadows == 1 && ns > 0.0) ? sun_shadow(v_pos, n) : 1.0;
        light = u_ambient * u_amb_scale + u_sun_col * ns * ss;
        if (u_gi == 1) {
            // indirektes Licht: E(n) = a + b·n je Farbkanal, durch π für diffuse Flächen
            // etwa eine halbe Zelle vor der Fläche abtasten (Sonden in der Wand nicht durchschlagen lassen)
            vec3 uvw = (v_pos + n * 24.0 - u_gi_min) / u_gi_size;
            vec4 cr = texture(u_gi_r, uvw);
            vec4 cg = texture(u_gi_g, uvw);
            vec4 cb = texture(u_gi_b, uvw);
            vec3 e = vec3(cr.x + dot(cr.yzw, n), cg.x + dot(cg.yzw, n), cb.x + dot(cb.yzw, n));
            light += max(e, vec3(0.0)) / 3.14159;
        }
        float sh[4];
        sh[0] = cube_shadow(u_cube0, u_lpos[0].xyz, u_cube_far.x, v_pos, n);
        sh[1] = cube_shadow(u_cube1, u_lpos[1].xyz, u_cube_far.y, v_pos, n);
        sh[2] = cube_shadow(u_cube2, u_lpos[2].xyz, u_cube_far.z, v_pos, n);
        sh[3] = cube_shadow(u_cube3, u_lpos[3].xyz, u_cube_far.w, v_pos, n);
        for (int i = 0; i < 32; i++) {
            if (i >= u_nlights) break;
            vec3 d = u_lpos[i].xyz - v_pos;
            float dist = max(length(d), 1.0);
            vec3 L = d / dist;
            float att = min(10000.0 / (dist * dist), 4.0);
            if (u_lpos[i].w > 0.5) {
                float c = dot(-L, u_ldir[i].xyz);
                float outer = u_ldir[i].w;
                float inner = u_lcol[i].w;
                att *= clamp((c - outer) / max(inner - outer, 0.001), 0.0, 1.0);
            }
            if (i < 4) att *= sh[i];
            light += u_lcol[i].rgb * max(dot(n, L), 0.0) * att;
        }
        // Source speichert Licht linear; Anzeige mit Gamma 2.2
        light = pow(min(light, vec3(4.0)), vec3(1.0 / 2.2));
    } else {
        light = vec3(0.62 + 0.38 * (0.5 + 0.5 * dot(n, normalize(vec3(0.35, 0.55, 0.85)))));
    }
    vec3 c = base * light;
    float d = length(v_pos - u_cam);
    if (u_fog_on == 1) {
        // Nebel wie env_fog_controller: linear von fogstart bis fogend, höchstens fogmaxdensity
        float f = clamp((d - u_fog_range.x) / max(u_fog_range.y - u_fog_range.x, 1.0), 0.0, 1.0) * u_fog.a;
        c = mix(c, u_fog.rgb, f);
    } else if (u_unlit == 0) {
        c = mix(c, vec3(0.12, 0.12, 0.14), clamp(d / 30000.0, 0.0, 0.6));
    }
    frag = vec4(c, a);
}
"#;

impl Renderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let header = if gl.version().is_embedded { "#version 300 es\nprecision highp float;\n" } else { "#version 330 core\n" };
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &format!("{header}{src}"));
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    return Err(format!("Shader-Fehler: {}", gl.get_shader_info_log(sh)));
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            for (i, n) in ["a_pos", "a_normal", "a_color", "a_uv", "a_lm", "a_aux"].iter().enumerate() {
                gl.bind_attrib_location(program, i as u32, n);
            }
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(format!("Link-Fehler: {}", gl.get_program_info_log(program)));
            }
            for sh in shaders {
                gl.detach_shader(program, sh);
                gl.delete_shader(sh);
            }
            let loc = |n: &str| gl.get_uniform_location(program, n);
            let u = Uniforms {
                mvp: loc("u_mvp"),
                model: loc("u_model"),
                mode: loc("u_mode"),
                pass: loc("u_pass"),
                cam: loc("u_cam"),
                tex: loc("u_tex"),
                tex2: loc("u_tex2"),
                blend: loc("u_blend"),
                view: loc("u_view"),
                tint: loc("u_tint"),
                dim: loc("u_dim"),
                show_alpha: loc("u_show_alpha"),
                nlights: loc("u_nlights"),
                lpos: loc("u_lpos[0]").or_else(|| loc("u_lpos")),
                lcol: loc("u_lcol[0]").or_else(|| loc("u_lcol")),
                ldir: loc("u_ldir[0]").or_else(|| loc("u_ldir")),
                ambient: loc("u_ambient"),
                sun_dir: loc("u_sun_dir"),
                sun_col: loc("u_sun_col"),
                tm0: loc("u_tm0"),
                tm1: loc("u_tm1"),
                shadows: loc("u_shadows"),
                sun_vp: loc("u_sun_vp"),
                sun_bias: loc("u_sun_bias"),
                cube_far: loc("u_cube_far"),
                gi: loc("u_gi"),
                gi_min: loc("u_gi_min"),
                gi_size: loc("u_gi_size"),
                amb_scale: loc("u_amb_scale"),
                fog_on: loc("u_fog_on"),
                fog: loc("u_fog"),
                fog_range: loc("u_fog_range"),
                unlit: loc("u_unlit"),
                clip: loc("u_clip"),
                clip_min: loc("u_clip_min"),
                clip_max: loc("u_clip_max"),
            };
            // Texture-Einheiten fest vergeben (verschiedene Sampler-Typen dürfen sich keine Einheit teilen)
            gl.use_program(Some(program));
            gl.uniform_1_i32(loc("u_tex").as_ref(), 0);
            gl.uniform_1_i32(loc("u_tex2").as_ref(), 1);
            gl.uniform_1_i32(loc("u_sun_map").as_ref(), 2);
            for k in 0..4 {
                gl.uniform_1_i32(loc(&format!("u_cube{k}")).as_ref(), 3 + k);
            }
            // globale Beleuchtung: drei 3D-Texturen (R, G, B) auf den Einheiten 7–9
            for (k, n) in ["u_gi_r", "u_gi_g", "u_gi_b"].iter().enumerate() {
                gl.uniform_1_i32(loc(n).as_ref(), 7 + k as i32);
            }
            gl.use_program(None);
            let mut bufs = Vec::new();
            for _ in 0..10 {
                bufs.push(Self::make_buffer(gl)?);
            }
            Ok(Renderer {
                program,
                u,
                bufs,
                batches: [Vec::new(), Vec::new(), Vec::new()],
                pending: None,
                statics: HashMap::new(),
                pending_statics: Vec::new(),
                textures: HashMap::new(),
                pending_tex: Vec::new(),
                tex_meta: HashMap::new(),
                start: std::time::Instant::now(),
                animated: std::cell::Cell::new(false),
                lines2d: None,
                pending_lines2d: None,
                mesh_gen: 0,
                shadow: None,
                gi_tex: None,
                gi_version: (u64::MAX, [0; 3]),
            })
        }
    }

    unsafe fn make_buffer(gl: &glow::Context) -> Result<GpuBuffer, String> {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = (STRIDE * 4) as i32;
        for (i, (size, off)) in [(3, 0), (3, 12), (4, 24), (2, 40), (2, 48), (2, 56)].iter().enumerate() {
            gl.enable_vertex_attrib_array(i as u32);
            gl.vertex_attrib_pointer_f32(i as u32, *size, glow::FLOAT, false, stride, *off);
        }
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        Ok(GpuBuffer { vao, vbo, count: 0 })
    }

    unsafe fn fill(gl: &glow::Context, buf: &mut GpuBuffer, data: &[f32]) {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buf.vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck::cast_slice(data), glow::DYNAMIC_DRAW);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        buf.count = (data.len() / STRIDE) as i32;
    }

    /// Neue 2D-Linien (werden beim nächsten Zeichnen hochgeladen)
    pub fn set_lines2d(&mut self, data: Vec<f32>) {
        self.pending_lines2d = Some(data);
    }

    /// 2D-Linien mit einer Orthografie-Matrix zeichnen (Aufruf aus dem Zeichen-Callback einer 2D-Ansicht)
    pub fn paint_lines2d(&mut self, gl: &glow::Context, mvp: Mat4, dim: Option<(usize, f64, f64)>) {
        unsafe {
            if let Some(data) = self.pending_lines2d.take() {
                if self.lines2d.is_none() {
                    self.lines2d = Self::make_buffer(gl).ok();
                }
                if let Some(b) = self.lines2d.as_mut() {
                    Self::fill(gl, b, &data);
                }
            }
            let Some(b) = &self.lines2d else { return };
            if b.count == 0 {
                return;
            }
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(self.u.mvp.as_ref(), false, &mvp.to_cols_array());
            gl.uniform_matrix_4_f32_slice(self.u.model.as_ref(), false, &Mat4::IDENTITY.to_cols_array());
            gl.uniform_1_i32(self.u.mode.as_ref(), 1);
            match dim {
                Some((ax, lo, hi)) => gl.uniform_4_f32(self.u.dim.as_ref(), ax as f32, lo as f32, hi as f32, DIM_ALPHA),
                None => gl.uniform_4_f32(self.u.dim.as_ref(), 0.0, 0.0, 0.0, 0.0),
            }
            gl.bind_vertex_array(Some(b.vao));
            gl.draw_arrays(glow::LINES, 0, b.count);
            gl.bind_vertex_array(None);
            gl.uniform_4_f32(self.u.dim.as_ref(), 0.0, 0.0, 0.0, 0.0);
            gl.use_program(None);
        }
    }

    /// Einzelnes statisches Mesh (Modell-Vorschau im Browser) in das aktuelle Viewport-Rechteck zeichnen
    pub fn paint_static_preview(&mut self, gl: &glow::Context, key: &str, mvp: Mat4, cam: Vec3) {
        unsafe {
            self.upload(gl);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.program));
            let u = &self.u;
            gl.uniform_matrix_4_f32_slice(u.mvp.as_ref(), false, &mvp.to_cols_array());
            gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &Mat4::IDENTITY.to_cols_array());
            gl.uniform_3_f32(u.cam.as_ref(), cam.x, cam.y, cam.z);
            gl.uniform_1_i32(u.view.as_ref(), 0);
            gl.uniform_1_i32(u.show_alpha.as_ref(), 0);
            gl.uniform_1_i32(u.tex.as_ref(), 0);
            gl.uniform_1_i32(u.tex2.as_ref(), 1);
            gl.uniform_4_f32(u.tint.as_ref(), 0.0, 0.0, 0.0, 0.0);
            gl.active_texture(glow::TEXTURE0);
            if let Some(sm) = self.statics.get(key) {
                self.draw_batches(gl, &sm.buf, &sm.batches, 0, 3, None);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_vertex_array(None);
            gl.disable(glow::DEPTH_TEST);
            gl.use_program(None);
        }
    }

    pub fn set_mesh(&mut self, m: MeshData) {
        self.pending = Some(m);
    }

    pub fn queue_textures(&mut self, t: Vec<crate::materials::TexData>) {
        self.pending_tex.extend(t);
    }

    pub fn add_static(&mut self, key: String, data: Vec<f32>, batches: Vec<Batch>) {
        self.pending_statics.push((key, data, batches));
    }

    pub fn has_static(&self, key: &str) -> bool {
        self.statics.contains_key(key) || self.pending_statics.iter().any(|s| s.0 == key)
    }

    /// Alle Texturen/Modelle verwerfen (z. B. nach Wechsel des Spielordners)
    pub fn clear_content(&mut self) {
        self.pending_tex.clear();
        self.tex_meta.clear();
        self.textures.clear();
        self.statics.clear();
        self.pending_statics.clear();
    }

    unsafe fn upload(&mut self, gl: &glow::Context) {
        for t in std::mem::take(&mut self.pending_tex) {
            let mut list = vec![(t.name.clone(), t.pw, t.ph, t.rgba)];
            if let Some((w, h, d)) = t.rgba2 {
                list.push((format!("{}#2", t.name), w, h, d));
            }
            let nframes = 1 + t.frames.len();
            for (k, f) in t.frames.into_iter().enumerate() {
                list.push((format!("{}#f{}", t.name, k + 1), t.pw, t.ph, f));
            }
            if nframes > 1 || t.xform.is_some() || t.scroll.is_some() {
                self.tex_meta.insert(t.name.clone(), TexMeta { frames: nframes, fps: t.anim_fps.unwrap_or(15.0), xform: t.xform.unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), scroll: t.scroll.unwrap_or((0.0, 0.0)) });
            } else {
                self.tex_meta.remove(&t.name);
            }
            for (name, w, h, rgba) in list {
                if rgba.len() != (w * h * 4) as usize {
                    continue;
                }
                let Ok(tex) = gl.create_texture() else { continue };
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&rgba)));
                gl.generate_mipmap(glow::TEXTURE_2D);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.bind_texture(glow::TEXTURE_2D, None);
                if let Some(old) = self.textures.insert(name, tex) {
                    gl.delete_texture(old);
                }
            }
        }
        if let Some(m) = self.pending.take() {
            self.batches = [m.opaque_batches.clone(), m.trans_batches.clone(), m.sky_batches.clone()];
            for (i, data) in [(0, &m.opaque), (1, &m.trans), (2, &m.lines), (3, &m.overlay_lines), (4, &m.wire), (5, &m.sky)] {
                Self::fill(gl, &mut self.bufs[i], data);
            }
            self.pending_statics.extend(m.statics);
            self.mesh_gen += 1;
        }
        if !self.pending_statics.is_empty() {
            self.mesh_gen += 1;
        }
        for (key, data, batches) in std::mem::take(&mut self.pending_statics) {
            let Ok(mut buf) = Self::make_buffer(gl) else { continue };
            Self::fill(gl, &mut buf, &data);
            if let Some(old) = self.statics.insert(key, StaticMesh { buf, batches }) {
                gl.delete_vertex_array(old.buf.vao);
                gl.delete_buffer(old.buf.vbo);
            }
        }
    }

    unsafe fn draw_batches(&self, gl: &glow::Context, buf: &GpuBuffer, batches: &[Batch], pass: i32, untextured_mode: i32, frustum: Option<&[glam::Vec4; 6]>) {
        gl.uniform_1_i32(self.u.pass.as_ref(), pass);
        gl.bind_vertex_array(Some(buf.vao));
        for b in batches {
            if b.count == 0 {
                continue;
            }
            if let (Some(f), Some(bb)) = (frustum, &b.bounds) {
                if !box_visible(f, bb) {
                    continue;
                }
            }
            match b.tex.as_ref().and_then(|t| self.textures.get(t)) {
                Some(t) => {
                    let mut tex = *t;
                    let mut m = [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0];
                    if let Some(meta) = b.tex.as_ref().and_then(|n| self.tex_meta.get(n)) {
                        let time = self.start.elapsed().as_secs_f32();
                        m = meta.xform;
                        if meta.frames > 1 {
                            let k = (time * meta.fps.max(0.1)) as usize % meta.frames;
                            if k > 0 {
                                if let Some(f) = b.tex.as_ref().and_then(|n| self.textures.get(&format!("{n}#f{k}"))) {
                                    tex = *f;
                                }
                            }
                            self.animated.set(true);
                        }
                        if meta.scroll.0 != 0.0 {
                            let (sn, cs) = meta.scroll.1.to_radians().sin_cos();
                            m[2] += meta.scroll.0 * time * cs;
                            m[5] += meta.scroll.0 * time * sn;
                            self.animated.set(true);
                        }
                    }
                    gl.uniform_3_f32(self.u.tm0.as_ref(), m[0], m[1], m[2]);
                    gl.uniform_3_f32(self.u.tm1.as_ref(), m[3], m[4], m[5]);
                    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                    gl.uniform_1_i32(self.u.mode.as_ref(), 2);
                    let t2 = b.tex.as_ref().and_then(|n| self.textures.get(&format!("{n}#2")));
                    gl.uniform_1_i32(self.u.blend.as_ref(), t2.is_some() as i32);
                    if let Some(t2) = t2 {
                        gl.active_texture(glow::TEXTURE1);
                        gl.bind_texture(glow::TEXTURE_2D, Some(*t2));
                        gl.active_texture(glow::TEXTURE0);
                    }
                }
                None => {
                    gl.uniform_1_i32(self.u.mode.as_ref(), untextured_mode);
                    gl.uniform_1_i32(self.u.blend.as_ref(), 0);
                }
            }
            gl.draw_arrays(glow::TRIANGLES, b.first, b.count);
        }
        gl.bind_texture(glow::TEXTURE_2D, None);
    }

    pub fn paint(&mut self, gl: &glow::Context, f: &FrameData) {
        unsafe {
            self.upload(gl);
            Self::fill(gl, &mut self.bufs[6], &f.dyn_lines);
            Self::fill(gl, &mut self.bufs[7], &f.dyn_quads);
            Self::fill(gl, &mut self.bufs[8], &f.add_quads);
            Self::fill(gl, &mut self.bufs[9], &f.sky2d);
            if !self.textures.contains_key(SOFT_DISC) {
                if let Ok(t) = soft_disc_texture(gl) {
                    self.textures.insert(SOFT_DISC.to_string(), t);
                }
            }
            let shadow_state = if f.view_mode == 4 && f.shadows { self.update_shadows(gl, f) } else { None };

            gl.clear_color(f.clear[0], f.clear[1], f.clear[2], 1.0);
            gl.clear_depth_f32(1.0);
            gl.depth_mask(true);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.program));
            let u = &self.u;
            let ident = Mat4::IDENTITY.to_cols_array();
            gl.uniform_matrix_4_f32_slice(u.mvp.as_ref(), false, &f.mvp.to_cols_array());
            let frustum = frustum_planes(&f.mvp);
            gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &ident);
            gl.uniform_3_f32(u.cam.as_ref(), f.cam.x, f.cam.y, f.cam.z);
            gl.uniform_1_i32(u.tex.as_ref(), 0);
            gl.uniform_1_i32(u.tex2.as_ref(), 1);
            gl.uniform_1_i32(u.blend.as_ref(), 0);
            gl.uniform_1_i32(u.view.as_ref(), f.view_mode);
            gl.uniform_1_i32(u.show_alpha.as_ref(), f.show_alpha as i32);
            gl.uniform_4_f32(u.tint.as_ref(), 0.0, 0.0, 0.0, 0.0);
            let n = f.lights.len().min(MAX_LIGHTS);
            let mut lp = vec![0.0f32; MAX_LIGHTS * 4];
            let mut lc = vec![0.0f32; MAX_LIGHTS * 4];
            let mut ld = vec![0.0f32; MAX_LIGHTS * 4];
            for (i, l) in f.lights.iter().take(n).enumerate() {
                lp[i * 4..i * 4 + 4].copy_from_slice(&[l.pos.x, l.pos.y, l.pos.z, if l.spot.is_some() { 1.0 } else { 0.0 }]);
                let (dir, inner, outer) = l.spot.unwrap_or((Vec3::Z, 1.0, 0.0));
                lc[i * 4..i * 4 + 4].copy_from_slice(&[l.color.x, l.color.y, l.color.z, inner]);
                ld[i * 4..i * 4 + 4].copy_from_slice(&[dir.x, dir.y, dir.z, outer]);
            }
            gl.uniform_1_i32(u.nlights.as_ref(), n as i32);
            gl.uniform_4_f32_slice(u.lpos.as_ref(), &lp);
            gl.uniform_4_f32_slice(u.lcol.as_ref(), &lc);
            gl.uniform_4_f32_slice(u.ldir.as_ref(), &ld);
            gl.uniform_3_f32(u.ambient.as_ref(), f.ambient.x, f.ambient.y, f.ambient.z);
            gl.uniform_3_f32(u.sun_dir.as_ref(), f.sun_dir.x, f.sun_dir.y, f.sun_dir.z);
            gl.uniform_3_f32(u.sun_col.as_ref(), f.sun_col.x, f.sun_col.y, f.sun_col.z);
            match (&shadow_state, &self.shadow) {
                (Some(st), Some(sh)) => {
                    gl.uniform_1_i32(u.shadows.as_ref(), st.sun as i32);
                    gl.uniform_matrix_4_f32_slice(u.sun_vp.as_ref(), false, &sh.sun_vp.to_cols_array());
                    gl.uniform_2_f32(u.sun_bias.as_ref(), sh.sun_bias[0], sh.sun_bias[1]);
                    gl.uniform_4_f32(u.cube_far.as_ref(), st.far[0], st.far[1], st.far[2], st.far[3]);
                    gl.active_texture(glow::TEXTURE2);
                    gl.bind_texture(glow::TEXTURE_2D, Some(sh.sun_tex));
                    for (k, c) in sh.cubes.iter().enumerate() {
                        gl.active_texture(glow::TEXTURE3 + k as u32);
                        gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(*c));
                    }
                }
                _ => {
                    gl.uniform_1_i32(u.shadows.as_ref(), 0);
                    gl.uniform_4_f32(u.cube_far.as_ref(), 0.0, 0.0, 0.0, 0.0);
                }
            }
            match f.gi.as_ref().filter(|_| f.view_mode == 4) {
                Some(g) if !g.sh.is_empty() => {
                    if self.gi_version != (g.version, g.dims) || self.gi_tex.is_none() {
                        self.upload_gi(gl, g);
                    }
                    if let Some(t) = self.gi_tex {
                        for k in 0..3 {
                            gl.active_texture(glow::TEXTURE7 + k as u32);
                            gl.bind_texture(glow::TEXTURE_3D, Some(t[k]));
                        }
                        let size = g.size();
                        gl.uniform_1_i32(self.u.gi.as_ref(), 1);
                        gl.uniform_3_f32(self.u.gi_min.as_ref(), g.origin.x, g.origin.y, g.origin.z);
                        gl.uniform_3_f32(self.u.gi_size.as_ref(), size.x, size.y, size.z);
                        gl.uniform_1_f32(self.u.amb_scale.as_ref(), 0.3);
                    }
                }
                _ => {
                    gl.uniform_1_i32(self.u.gi.as_ref(), 0);
                    gl.uniform_1_f32(self.u.amb_scale.as_ref(), 1.0);
                }
            }
            let u = &self.u;
            gl.active_texture(glow::TEXTURE0);

            let draw_statics = |sky: bool| {
                for d in f.draws.iter().filter(|d| d.sky == sky) {
                    if let Some(sm) = self.statics.get(&d.key) {
                        gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &d.model.to_cols_array());
                        gl.uniform_4_f32(u.tint.as_ref(), d.tint[0], d.tint[1], d.tint[2], d.tint[3]);
                        self.draw_batches(gl, &sm.buf, &sm.batches, 0, 3, None);
                    }
                }
                gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &ident);
                gl.uniform_4_f32(u.tint.as_ref(), 0.0, 0.0, 0.0, 0.0);
            };

            gl.uniform_1_i32(u.fog_on.as_ref(), 0);
            gl.uniform_1_i32(u.unlit.as_ref(), 0);
            gl.uniform_1_i32(u.clip.as_ref(), 0);
            if let Some(c) = &f.clip {
                gl.uniform_3_f32(u.clip_min.as_ref(), c.min.x as f32, c.min.y as f32, c.min.z as f32);
                gl.uniform_3_f32(u.clip_max.as_ref(), c.max.x as f32, c.max.y as f32, c.max.z as f32);
            }
            // -1. 2D-Himmel: hinter allem, ohne Licht und Tiefe
            if !f.sky2d.is_empty() && !f.wireframe {
                gl.disable(glow::DEPTH_TEST);
                gl.depth_mask(false);
                gl.uniform_1_i32(u.unlit.as_ref(), 1);
                self.draw_batches(gl, &self.bufs[9], &f.sky2d_batches, 0, 1, None);
                gl.uniform_1_i32(u.unlit.as_ref(), 0);
                gl.depth_mask(true);
                gl.enable(glow::DEPTH_TEST);
            }
            gl.uniform_1_i32(u.clip.as_ref(), f.clip.is_some() as i32);
            // 0. 3D-Skybox (skaliert), danach Tiefe löschen
            if let Some(sky) = f.sky {
                gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &sky.to_cols_array());
                self.draw_batches(gl, &self.bufs[5], &self.batches[2], 0, 0, None);
                gl.uniform_matrix_4_f32_slice(u.model.as_ref(), false, &ident);
                draw_statics(true);
                gl.clear(glow::DEPTH_BUFFER_BIT);
            }
            if let Some((c, start, end, dens)) = f.fog {
                gl.uniform_1_i32(u.fog_on.as_ref(), 1);
                gl.uniform_4_f32(u.fog.as_ref(), c[0], c[1], c[2], dens);
                gl.uniform_2_f32(u.fog_range.as_ref(), start, end);
            }
            // 1. Undurchsichtige Flächen + Modelle
            if !f.wireframe {
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.0, 1.0);
                self.draw_batches(gl, &self.bufs[0], &self.batches[0], 0, 0, Some(&frustum));
                gl.disable(glow::POLYGON_OFFSET_FILL);
            }
            draw_statics(false);
            // 2. Linien
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.uniform_1_i32(u.mode.as_ref(), 1);
            for i in [2usize, 4] {
                if i == 4 && !f.wireframe {
                    continue;
                }
                gl.bind_vertex_array(Some(self.bufs[i].vao));
                gl.draw_arrays(glow::LINES, 0, self.bufs[i].count);
            }
            // 3. Transparente Flächen (Tool-Texturen, Glas, Decals, Overlays, Sprites)
            gl.depth_mask(false);
            if !f.wireframe {
                self.draw_batches(gl, &self.bufs[1], &self.batches[1], 1, 0, Some(&frustum));
            }
            self.draw_batches(gl, &self.bufs[7], &f.dyn_batches, 1, 1, None);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE);
            self.draw_batches(gl, &self.bufs[8], &f.add_batches, 1, 1, None);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.depth_mask(true);
            gl.uniform_1_i32(u.fog_on.as_ref(), 0);
            // 4. Auswahl-Umrisse und Werkzeug-Linien immer sichtbar
            gl.uniform_1_i32(u.clip.as_ref(), 0);
            gl.uniform_1_i32(u.mode.as_ref(), 1);
            gl.depth_func(glow::ALWAYS);
            for i in [3usize, 6] {
                gl.bind_vertex_array(Some(self.bufs[i].vao));
                gl.draw_arrays(glow::LINES, 0, self.bufs[i].count);
            }
            gl.depth_func(glow::LESS);
            gl.disable(glow::DEPTH_TEST);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }
}

// ------------------------------------------------------------------ Schatten

const SHADOW_VS: &str = r#"
in vec3 a_pos;
uniform mat4 u_vp;
uniform mat4 u_model;
out vec3 v_w;
void main() {
    vec4 w = u_model * vec4(a_pos, 1.0);
    v_w = w.xyz;
    gl_Position = u_vp * w;
}
"#;

const SHADOW_FS: &str = r#"
in vec3 v_w;
uniform vec3 u_lpos;
uniform float u_far;
uniform int u_point;
out vec4 frag;
void main() {
    gl_FragDepth = (u_point == 1) ? length(v_w - u_lpos) / u_far : gl_FragCoord.z;
    frag = vec4(1.0);
}
"#;

/// Zahl der Lichter mit eigenen Schatten (die nächsten zur Kamera)
const SHADOW_LIGHTS: usize = 4;
const CUBE_SIZE: i32 = 512;

struct Shadows {
    prog: glow::Program,
    u_vp: Option<glow::UniformLocation>,
    u_model: Option<glow::UniformLocation>,
    u_lpos: Option<glow::UniformLocation>,
    u_far: Option<glow::UniformLocation>,
    u_point: Option<glow::UniformLocation>,
    fbo: glow::Framebuffer,
    sun_tex: glow::Texture,
    sun_size: i32,
    sun_vp: Mat4,
    /// (Tiefen-Bias, Normalen-Versatz in Einheiten)
    sun_bias: [f32; 2],
    cubes: Vec<glow::Texture>,
    key: u64,
    state: ShadowState,
}

#[derive(Clone, Copy, Default)]
struct ShadowState {
    sun: bool,
    far: [f32; 4],
}

/// Reichweite eines Lichts: dort fällt die Helligkeit unter ~0,5 %
fn light_range(c: Vec3) -> f32 {
    (10000.0 * c.max_element().max(0.01) / 0.005).sqrt().clamp(256.0, 4096.0)
}

impl Renderer {
    /// Sondenraster als drei RGBA32F-3D-Texturen hochladen
    unsafe fn upload_gi(&mut self, gl: &glow::Context, g: &crate::gi::Grid) {
        if self.gi_tex.is_none() {
            let mut t = Vec::new();
            for _ in 0..3 {
                match gl.create_texture() {
                    Ok(x) => t.push(x),
                    Err(_) => return,
                }
            }
            self.gi_tex = Some([t[0], t[1], t[2]]);
        }
        let Some(tex) = self.gi_tex else { return };
        let [w, h, d] = g.dims;
        for (c, t) in tex.iter().enumerate() {
            let data: Vec<f32> = g.sh.iter().flat_map(|s| s[c]).collect();
            let bytes: &[u8] = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4);
            gl.active_texture(glow::TEXTURE7 + c as u32);
            gl.bind_texture(glow::TEXTURE_3D, Some(*t));
            gl.tex_image_3d(glow::TEXTURE_3D, 0, glow::RGBA32F as i32, w as i32, h as i32, d as i32, 0, glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(Some(bytes)));
            gl.tex_parameter_i32(glow::TEXTURE_3D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_3D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            for p in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T, glow::TEXTURE_WRAP_R] {
                gl.tex_parameter_i32(glow::TEXTURE_3D, p, glow::CLAMP_TO_EDGE as i32);
            }
        }
        gl.active_texture(glow::TEXTURE0);
        self.gi_version = (g.version, g.dims);
    }

    unsafe fn create_shadows(gl: &glow::Context) -> Result<Shadows, String> {
        let header = if gl.version().is_embedded { "#version 300 es\nprecision highp float;\n" } else { "#version 330 core\n" };
        let prog = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, SHADOW_VS), (glow::FRAGMENT_SHADER, SHADOW_FS)] {
            let sh = gl.create_shader(kind)?;
            gl.shader_source(sh, &format!("{header}{src}"));
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                return Err(format!("Schatten-Shader: {}", gl.get_shader_info_log(sh)));
            }
            gl.attach_shader(prog, sh);
            shaders.push(sh);
        }
        gl.bind_attrib_location(prog, 0, "a_pos");
        gl.link_program(prog);
        if !gl.get_program_link_status(prog) {
            return Err(format!("Schatten-Shader: {}", gl.get_program_info_log(prog)));
        }
        for sh in shaders {
            gl.detach_shader(prog, sh);
            gl.delete_shader(sh);
        }
        let depth_params = |target: u32| {
            gl.tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_R, glow::CLAMP_TO_EDGE as i32);
        };
        let sun_size = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE).clamp(1024, 4096);
        let sun_tex = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(sun_tex));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, sun_size, sun_size, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
        depth_params(glow::TEXTURE_2D);
        gl.bind_texture(glow::TEXTURE_2D, None);
        let mut cubes = Vec::new();
        for _ in 0..SHADOW_LIGHTS {
            let t = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(t));
            for face in 0..6 {
                gl.tex_image_2d(glow::TEXTURE_CUBE_MAP_POSITIVE_X + face, 0, glow::DEPTH_COMPONENT24 as i32, CUBE_SIZE, CUBE_SIZE, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
            }
            depth_params(glow::TEXTURE_CUBE_MAP);
            cubes.push(t);
        }
        gl.bind_texture(glow::TEXTURE_CUBE_MAP, None);
        let fbo = gl.create_framebuffer()?;
        let loc = |n: &str| gl.get_uniform_location(prog, n);
        Ok(Shadows { u_vp: loc("u_vp"), u_model: loc("u_model"), u_lpos: loc("u_lpos"), u_far: loc("u_far"), u_point: loc("u_point"), prog, fbo, sun_tex, sun_size, sun_vp: Mat4::IDENTITY, sun_bias: [0.0, 0.0], cubes, key: 0, state: ShadowState::default() })
    }

    /// Alle schattenwerfenden Flächen (undurchsichtige Brushes ohne Skybox + Modelle) zeichnen
    unsafe fn draw_casters(&self, gl: &glow::Context, sh: &Shadows, f: &FrameData, vp: &Mat4) {
        let frustum = frustum_planes(vp);
        gl.uniform_matrix_4_f32_slice(sh.u_vp.as_ref(), false, &vp.to_cols_array());
        gl.uniform_matrix_4_f32_slice(sh.u_model.as_ref(), false, &Mat4::IDENTITY.to_cols_array());
        gl.bind_vertex_array(Some(self.bufs[0].vao));
        for b in &self.batches[0] {
            if b.count == 0 || b.tex.as_deref().is_some_and(|t| t.contains("toolsskybox")) {
                continue;
            }
            if let Some(bb) = &b.bounds {
                if !box_visible(&frustum, bb) {
                    continue;
                }
            }
            gl.draw_arrays(glow::TRIANGLES, b.first, b.count);
        }
        for d in f.draws.iter().filter(|d| !d.sky) {
            if let Some(sm) = self.statics.get(&d.key) {
                gl.uniform_matrix_4_f32_slice(sh.u_model.as_ref(), false, &d.model.to_cols_array());
                gl.bind_vertex_array(Some(sm.buf.vao));
                for b in &sm.batches {
                    gl.draw_arrays(glow::TRIANGLES, b.first, b.count);
                }
            }
        }
    }

    /// Schattenkarten bei Bedarf neu zeichnen. Gibt zurück, welche Schatten gültig sind.
    unsafe fn update_shadows(&mut self, gl: &glow::Context, f: &FrameData) -> Option<ShadowState> {
        if self.shadow.is_none() {
            match Self::create_shadows(gl) {
                Ok(s) => self.shadow = Some(s),
                Err(e) => {
                    eprintln!("{e}");
                    return None;
                }
            }
        }
        // Szenengrenzen aus den Blöcken
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for b in self.batches[0].iter().filter_map(|b| b.bounds) {
            lo = lo.min(Vec3::new(b[0], b[1], b[2]));
            hi = hi.max(Vec3::new(b[3], b[4], b[5]));
        }
        let has_scene = lo.x <= hi.x;
        // Große Maps: Sonnenschatten nur im Umkreis der Kamera (in 1024er-Schritten nachgeführt)
        let full_r = if has_scene { (hi - lo).length() * 0.5 } else { 0.0 };
        let (center, radius) = if full_r > 6000.0 { ((f.cam / 1024.0).round() * 1024.0, 6000.0) } else { ((lo + hi) * 0.5, full_r.max(64.0)) };
        let sun = has_scene && f.sun_col.max_element() > 0.0 && f.sun_dir.length_squared() > 0.5;
        let n = f.lights.len().min(SHADOW_LIGHTS);
        // Schlüssel: Geometrie, Sonne, Lichter, Modelle
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.mesh_gen.hash(&mut h);
        sun.hash(&mut h);
        for v in [f.sun_dir, center] {
            v.to_array().map(f32::to_bits).hash(&mut h);
        }
        for l in &f.lights[..n] {
            l.pos.to_array().map(f32::to_bits).hash(&mut h);
            light_range(l.color).to_bits().hash(&mut h);
        }
        for d in f.draws.iter().filter(|d| !d.sky) {
            d.key.hash(&mut h);
            d.model.to_cols_array().map(f32::to_bits).hash(&mut h);
        }
        let key = h.finish();
        let sh = self.shadow.as_ref().unwrap();
        if sh.key == key {
            return Some(sh.state);
        }

        // GL-Zustand von egui sichern
        let mut vp_old = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut vp_old);
        let fb_old = gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_func(glow::LESS);
        gl.depth_mask(true);
        gl.color_mask(false, false, false, false);
        gl.enable(glow::POLYGON_OFFSET_FILL);
        gl.polygon_offset(2.0, 4.0);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(sh.fbo));
        gl.draw_buffers(&[glow::NONE]);
        gl.read_buffer(glow::NONE);
        gl.use_program(Some(sh.prog));

        let mut state = ShadowState { sun: false, far: [0.0; 4] };
        let mut sun_vp = sh.sun_vp;
        let mut sun_bias = sh.sun_bias;
        if sun {
            let dir = f.sun_dir.normalize();
            let up = if dir.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
            let view = Mat4::look_at_rh(center - dir * radius * 2.0, center, up);
            let proj = Mat4::orthographic_rh_gl(-radius, radius, -radius, radius, radius * 0.5, radius * 4.0);
            sun_vp = proj * view;
            let texel = 2.0 * radius / sh.sun_size as f32;
            // Tiefe läuft über 3,5 × Radius (orthographisch, linear) → 2 Einheiten Bias
            sun_bias = [2.0 / (radius * 3.5) * 2.0, texel * 1.5];
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(sh.sun_tex), 0);
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE {
                gl.viewport(0, 0, sh.sun_size, sh.sun_size);
                gl.clear_depth_f32(1.0);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.uniform_1_i32(sh.u_point.as_ref(), 0);
                self.draw_casters(gl, sh, f, &sun_vp);
                state.sun = true;
            }
        }
        // Würfelkarten der nächsten Lichter
        let faces = [(Vec3::X, -Vec3::Y), (-Vec3::X, -Vec3::Y), (Vec3::Y, Vec3::Z), (-Vec3::Y, -Vec3::Z), (Vec3::Z, -Vec3::Y), (-Vec3::Z, -Vec3::Y)];
        gl.viewport(0, 0, CUBE_SIZE, CUBE_SIZE);
        gl.uniform_1_i32(sh.u_point.as_ref(), 1);
        for (i, l) in f.lights[..n].iter().enumerate() {
            let far = light_range(l.color);
            gl.uniform_3_f32(sh.u_lpos.as_ref(), l.pos.x, l.pos.y, l.pos.z);
            gl.uniform_1_f32(sh.u_far.as_ref(), far);
            let proj = Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1.0, 1.0, far);
            let mut ok = true;
            for (k, (dir, up)) in faces.iter().enumerate() {
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_CUBE_MAP_POSITIVE_X + k as u32, Some(sh.cubes[i]), 0);
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                    ok = false;
                    break;
                }
                gl.clear_depth_f32(1.0);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                let vp = proj * Mat4::look_at_rh(l.pos, l.pos + *dir, *up);
                self.draw_casters(gl, sh, f, &vp);
            }
            if ok {
                state.far[i] = far;
            }
        }

        // egui-Zustand wiederherstellen
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, None, 0);
        gl.bind_framebuffer(glow::FRAMEBUFFER, fb_old);
        gl.viewport(vp_old[0], vp_old[1], vp_old[2], vp_old[3]);
        if scissor {
            gl.enable(glow::SCISSOR_TEST);
        }
        gl.disable(glow::POLYGON_OFFSET_FILL);
        gl.color_mask(true, true, true, true);
        gl.bind_vertex_array(None);
        let sh = self.shadow.as_mut().unwrap();
        sh.key = key;
        sh.state = state;
        sh.sun_vp = sun_vp;
        sh.sun_bias = sun_bias;
        Some(state)
    }
}

// ------------------------------------------------------------------ Lichter

fn parse_light(s: Option<&str>, default: [f32; 4]) -> Vec3 {
    let v: Vec<f32> = s.unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let c = if v.len() >= 4 { [v[0], v[1], v[2], v[3]] } else if v.len() == 3 { [v[0], v[1], v[2], 200.0] } else { default };
    Vec3::new(c[0], c[1], c[2]) / 255.0 * (c[3] / 255.0)
}

pub fn collect_lights(ed: &Editor) -> (Vec<LightInfo>, Vec3, Vec3, Vec3) {
    let mut lights = Vec::new();
    let mut ambient = Vec3::splat(0.04);
    let mut sun_dir = Vec3::new(0.0, 0.0, -1.0);
    let mut sun_col = Vec3::ZERO;
    for e in &ed.map.entities {
        if !ed.is_visible_entity_all(e) {
            continue;
        }
        let Some(o) = e.origin() else { continue };
        let mut a = e.angles();
        if let Some(p) = e.get("pitch").and_then(|p| p.parse::<f64>().ok()) {
            a.x = -p;
        }
        match e.classname() {
            "light" => lights.push(LightInfo { pos: o.as_vec3(), color: parse_light(e.get("_light"), [255.0, 255.0, 255.0, 200.0]), spot: None }),
            "light_spot" => {
                let (f, _, _) = angles_to_basis(a);
                let inner = e.get("_inner_cone").and_then(|c| c.parse::<f32>().ok()).unwrap_or(30.0).to_radians().cos();
                let outer = e.get("_cone").and_then(|c| c.parse::<f32>().ok()).unwrap_or(45.0).to_radians().cos();
                lights.push(LightInfo { pos: o.as_vec3(), color: parse_light(e.get("_light"), [255.0, 255.0, 255.0, 400.0]), spot: Some((f.as_vec3(), inner, outer)) });
            }
            "light_environment" => {
                let (f, _, _) = angles_to_basis(a);
                sun_dir = f.as_vec3().normalize_or_zero();
                sun_col = parse_light(e.get("_light"), [255.0, 255.0, 255.0, 300.0]) * 1.2;
                ambient = parse_light(e.get("_ambient"), [140.0, 160.0, 200.0, 80.0]) * 1.5;
            }
            _ => {}
        }
    }
    (lights, ambient, sun_dir, sun_col)
}

/// Weltpunkt → Bildschirm (None, wenn hinter der Kamera)
fn project_with(cam: &Camera, rect: Rect, p: DVec3) -> Option<egui::Pos2> {
    let clip = cam.view_proj(rect.width() / rect.height()) * p.as_vec3().extend(1.0);
    if clip.w <= 0.01 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(egui::pos2(rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(), rect.top() + (1.0 - ndc.y) * 0.5 * rect.height()))
}

pub fn xform_mat4(x: &Xform) -> Mat4 {
    let m = x.m;
    Mat4::from_cols(m.x_axis.as_vec3().extend(0.0), m.y_axis.as_vec3().extend(0.0), m.z_axis.as_vec3().extend(0.0), x.t.as_vec3().extend(1.0))
}

// ------------------------------------------------------------------ View

type BuiltRev = (u64, u64, u64, bool, RenderMode, bool, bool);

#[derive(Clone, Copy, PartialEq)]
enum GizmoKind {
    Axis(usize),
    Ring(usize),
}

/// Geometrie eines info_overlay (Basis und vier Ecken in Basis-Koordinaten)
#[derive(Clone, Debug)]
struct OverlayGeom {
    origin: DVec3,
    u: DVec3,
    v: DVec3,
    n: DVec3,
    uv: [DVec3; 4],
}

impl OverlayGeom {
    fn of(e: &Entity) -> Option<OverlayGeom> {
        let g = |k: &str| e.get(k).and_then(parse_vec);
        let uv = [g("uv0")?, g("uv1")?, g("uv2")?, g("uv3")?];
        Some(OverlayGeom { origin: g("BasisOrigin")?, u: g("BasisU")?, v: g("BasisV")?, n: g("BasisNormal")?, uv })
    }
    fn corners(&self) -> [DVec3; 4] {
        self.uv.map(|p| self.origin + self.u * p.x + self.v * p.y + self.n * 0.6)
    }
    fn plane_hit(&self, o: DVec3, d: DVec3) -> Option<DVec3> {
        let den = d.dot(self.n);
        if den.abs() < 1e-6 {
            return None;
        }
        let t = (self.origin - o).dot(self.n) / den;
        (t > 0.0).then(|| o + d * t)
    }
    fn size(&self) -> (f64, f64) {
        let xs = self.uv.iter().map(|p| p.x);
        let ys = self.uv.iter().map(|p| p.y);
        (xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min), ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min))
    }
    fn write(&self, e: &mut Entity) {
        use crate::map::fmt_vec;
        e.set("BasisOrigin", fmt_vec(self.origin));
        e.set("BasisU", fmt_vec(self.u));
        e.set("BasisV", fmt_vec(self.v));
        for (k, p) in self.uv.iter().enumerate() {
            e.set(&format!("uv{k}"), fmt_vec(*p));
        }
        e.set("origin", fmt_vec(self.origin));
    }
}

#[derive(Clone)]
struct OverlayDrag {
    id: u32,
    /// 0–3 Ecke, 4 Drehen, 5 Verschieben
    kind: usize,
    start: DVec3,
    base: OverlayGeom,
    began: bool,
}

/// Genau ein ausgewähltes info_overlay?
fn selected_overlay(ed: &Editor) -> Option<u32> {
    if ed.sel.len() != 1 {
        return None;
    }
    match ed.sel.iter().next()? {
        Obj::Entity(id) => ed.map.entity(*id).filter(|e| e.classname().eq_ignore_ascii_case("info_overlay")).map(|_| *id),
        _ => None,
    }
}

struct GizmoDrag {
    kind: GizmoKind,
    base: Box<Map>,
    start: egui::Pos2,
    center: DVec3,
    len: f64,
    began: bool,
    /// Ecke der Auswahl-Box beim Start (sie rastet am Raster ein, nicht nur die Strecke)
    bmin: DVec3,
}

struct FaceDrag {
    sid: u32,
    side: usize,
    base: Box<Map>,
    start: egui::Pos2,
    dir: Vec2,
    extrude: bool,
    began: bool,
}

const AXIS_COLORS: [[f32; 4]; 3] = [[1.0, 0.25, 0.25, 1.0], [0.3, 1.0, 0.3, 1.0], [0.35, 0.55, 1.0, 1.0]];

/// Griff-Ziehen: Ausgangszustand und Startpunkt in der Bildebene
struct GripDrag {
    base: crate::mesh_edit::GripBase,
    start: DVec3,
    began: bool,
}

pub struct View3D {
    pub gizmo: bool,
    gizmo_drag: Option<GizmoDrag>,
    overlay_drag: Option<OverlayDrag>,
    face_drag: Option<FaceDrag>,
    measure_start: Option<DVec3>,
    /// Rahmenauswahl: Startpunkt auf dem Bildschirm
    box_sel: Option<egui::Pos2>,
    pub cam: Camera,
    pub speed: f32,
    /// Tasten und Maustaste der Kamerasteuerung (aus den Einstellungen)
    pub controls: crate::compile::CamControls,
    /// Umsehen-Taste gerade gedrückt (dann gehören alle Tasten der Kamera)
    pub flying: bool,
    /// Tastenkürzel mit Umschalt-/Alt-Taste, die Befehle auslösen – ohne gedrückte Umsehen-Taste
    /// bewegen sie die Kamera nicht (z. B. Shift+A = Textur-Werkzeug statt schnell nach links)
    pub taken: Vec<egui::KeyboardShortcut>,
    pub textured: bool,
    pub mode: RenderMode,
    pub sky_preview: bool,
    pub helpers: bool,
    pub models: bool,
    /// Kollisionsmodelle (.phy) der Props als Drahtgitter
    pub collision: bool,
    /// Lichtvorschau mit Schatten
    pub shadows: bool,
    pub renderer: Option<Arc<Mutex<Renderer>>>,
    built_rev: BuiltRev,
    last_build: std::time::Instant,
    lights: (Vec<LightInfo>, Vec3, Vec3, Vec3),
    sky: Option<Mat4>,
    sky_box: Option<Aabb>,
    /// Skybox-Modus: nur innerhalb dieser Box zeichnen (die Map liegt außerhalb)
    pub clip_box: Option<Aabb>,
    /// Skybox-Modus: Spielersicht als Bild im Bild (Kamera)
    pub pip: Option<Camera>,
    painting: bool,
    pub error: Option<String>,
    solid_cache: SolidCache,
    /// globale Beleuchtung in der Lichtvorschau
    pub gi_on: bool,
    /// Partikel-Vorschau (info_particle_system)
    pub particles: crate::particle_preview::Preview,
    /// Begehen-Modus (mit Spielergröße durch die Map laufen)
    pub walker: crate::walk::Walker,
    /// Nebel aus env_fog_controller anzeigen
    pub fog_preview: bool,
    pub gi_grid: Option<Arc<crate::gi::Grid>>,
    gi_job: Option<crate::gi::Job>,
    /// (Durchgang, Durchgänge)
    pub gi_progress: (usize, usize),
    gi_rev: Option<(u64, u64)>,
    gi_wait: Option<std::time::Instant>,
    /// Tile-Set: zuletzt gesetztes Feld während des Ziehens
    tile_last: Option<(i64, i64)>,
    /// Mesh-/Patch-Griff wird gezogen
    grip_drag: Option<GripDrag>,
    /// Rahmenauswahl von Griffen (Startpunkt)
    grip_box: Option<egui::Pos2>,
}

impl View3D {
    pub fn new(gl: Option<&glow::Context>) -> Self {
        let (renderer, error) = match gl.map(Renderer::new) {
            Some(Ok(r)) => (Some(Arc::new(Mutex::new(r))), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, Some(crate::i18n::tr("Kein OpenGL-Kontext verfügbar").into())),
        };
        let mut cam = Camera { pos: Vec3::new(-420.0, -520.0, 360.0), yaw: 0.0, pitch: 0.0, fov: 75.0 };
        cam.look_at(Vec3::new(0.0, 0.0, 64.0));
        View3D {
            gizmo: true,
            gizmo_drag: None,
            overlay_drag: None,
            face_drag: None,
            measure_start: None,
            box_sel: None,
            cam,
            speed: 700.0,
            controls: crate::compile::CamControls::default(),
            flying: false,
            taken: Vec::new(),
            textured: true,
            mode: RenderMode::Textured,
            sky_preview: true,
            helpers: true,
            models: true,
            collision: false,
            shadows: true,
            gi_on: false,
            particles: crate::particle_preview::Preview::new(),
            walker: Default::default(),
            fog_preview: true,
            gi_grid: None,
            gi_job: None,
            gi_progress: (0, 0),
            gi_rev: None,
            gi_wait: None,
            tile_last: None,
            grip_drag: None,
            grip_box: None,
            renderer,
            built_rev: (0, 0, 0, true, RenderMode::Textured, true, true),
            last_build: std::time::Instant::now() - std::time::Duration::from_secs(10),
            lights: (Vec::new(), Vec3::ZERO, Vec3::Z, Vec3::ZERO),
            sky: None,
            sky_box: None,
            clip_box: None,
            pip: None,
            painting: false,
            error,
            solid_cache: SolidCache::default(),
        }
    }

    pub fn focus(&mut self, b: &Aabb) {
        if b.is_empty() {
            return;
        }
        let c = b.center().as_vec3();
        let r = (b.size().length() as f32).max(64.0);
        self.cam.pos = c - self.cam.forward() * r * 1.2;
    }

    /// Mesh beim nächsten Bild neu aufbauen (z. B. nach Tab-Wechsel)
    pub fn invalidate(&mut self) {
        self.built_rev.0 = 0;
    }

    pub fn clear_content(&mut self) {
        if let Some(r) = &self.renderer {
            r.lock().unwrap().clear_content();
        }
        // Zwischengespeicherte Brush-Netze verweisen auf die verworfenen Texturen → neu bauen
        // (sonst bleiben Brushes nach „Inhalte neu einlesen“ weiß)
        self.solid_cache = SolidCache::default();
        self.built_rev.0 = 0;
    }

    pub fn set_from_camera(&mut self, c: &crate::map::CameraDef) {
        self.cam.pos = c.pos.as_vec3();
        self.cam.look_at(c.look.as_vec3());
    }

    pub fn camera_def(&self) -> crate::map::CameraDef {
        crate::map::CameraDef { pos: self.cam.pos.as_dvec3(), look: (self.cam.pos + self.cam.forward() * 128.0).as_dvec3(), name: String::new() }
    }

    /// Bereich der Spielersicht (Bild im Bild) unten rechts
    pub fn pip_rect(rect: Rect) -> Rect {
        let w = (rect.width() * 0.38).clamp(200.0, 520.0).min(rect.width() - 20.0);
        let h = (w * 0.62).min(rect.height() * 0.5);
        Rect::from_min_size(rect.right_bottom() - Vec2::new(w + 10.0, h + 40.0), Vec2::new(w, h))
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, ed: &mut Editor, mats: &mut MaterialSystem, models: &mut ModelSystem, maximize_toggle: &mut bool) {
        let resp = ui.interact(rect, ui.id().with("view3d"), Sense::click_and_drag());
        // Spielersicht: liegt über der Ansicht und fängt Maus und Mausrad ab
        if let Some(pc) = self.pip.as_mut() {
            let pr = Self::pip_rect(rect);
            let presp = ui.interact(pr, ui.id().with("view3d_pip"), Sense::click_and_drag());
            if presp.dragged() {
                let d = presp.drag_delta();
                let sens = self.controls.sensitivity.clamp(0.02, 2.0);
                pc.yaw -= d.x * sens;
                pc.pitch = (pc.pitch - d.y * sens).clamp(-89.0, 89.0);
            }
            if presp.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    let f = pc.forward();
                    pc.pos += f * scroll * 4.0;
                }
            }
        }
        let typing = ui.ctx().wants_keyboard_input();
        let (ctrl, alt) = ui.input(|i| (i.modifiers.command, i.modifiers.alt));

        // ---- Kamera
        let look_button = if self.controls.look == "right" { PointerButton::Secondary } else { PointerButton::Middle };
        let looking = resp.dragged_by(look_button);
        self.flying = looking;
        if looking {
            let d = resp.drag_delta();
            let sens = self.controls.sensitivity.clamp(0.02, 2.0);
            let inv = if self.controls.invert_y { -1.0 } else { 1.0 };
            self.cam.yaw -= d.x * sens;
            self.cam.pitch = (self.cam.pitch - d.y * sens * inv).clamp(-89.0, 89.0);
        }
        if self.walker.on && !typing {
            // Begehen: WASD laufen, Leertaste springen, Strg ducken, Shift rennen
            let (dt, inp, esc) = ui.input(|i| {
                let k = |a: Key, b: Key| (i.key_down(a) as i32 - i.key_down(b) as i32) as f64;
                (
                    i.stable_dt.min(0.05) as f64,
                    crate::walk::Input { fwd: k(Key::W, Key::S), side: k(Key::D, Key::A), jump: i.key_down(Key::Space), duck: i.modifiers.ctrl || i.key_down(Key::C), run: i.modifiers.shift, yaw: self.cam.yaw as f64 },
                    i.key_pressed(Key::Escape),
                )
            });
            if esc {
                self.walker.on = false;
            } else {
                self.walker.step(ed, inp, dt);
                self.cam.pos = (self.walker.pos + DVec3::Z * self.walker.eye()).as_vec3();
                ui.ctx().request_repaint();
            }
        } else if (resp.hovered() || looking) && !typing {
            let move_keys = self.controls.move_keys();
            let fast_mod = self.controls.fast.to_ascii_lowercase();
            let taken = &self.taken;
            let (dt, keys, fast, scroll) = ui.input(|i| {
                let m = i.modifiers;
                let mut keys = [false; 6];
                for (k, d) in &move_keys {
                    if !i.key_down(*k) {
                        continue;
                    }
                    // ohne Umsehen-Taste: Kombination gehört einem Befehl (z. B. Shift+A) → nicht bewegen
                    let is_cmd = !looking && taken.iter().any(|sc| sc.logical_key == *k && sc.modifiers.shift == m.shift && sc.modifiers.alt == m.alt && (sc.modifiers.command || sc.modifiers.ctrl) == m.command);
                    if !is_cmd {
                        keys[*d] = true;
                    }
                }
                let fast = match fast_mod.as_str() {
                    "shift" => m.shift,
                    "alt" => m.alt,
                    _ => false,
                };
                (i.stable_dt.min(0.1), keys, fast, i.raw_scroll_delta.y)
            });
            let f = self.cam.forward();
            let r = self.cam.right();
            let mut mv = Vec3::ZERO;
            if !ctrl {
                for (k, d) in keys.iter().zip([f, -f, -r, r, Vec3::Z, -Vec3::Z]) {
                    if *k {
                        mv += d;
                    }
                }
            }
            if mv != Vec3::ZERO {
                self.cam.pos += mv.normalize() * self.speed * if fast { 3.0 } else { 1.0 } * dt;
                ui.ctx().request_repaint();
            }
            if scroll != 0.0 {
                self.cam.pos += f * scroll * 2.0;
            }
        }
        if resp.double_clicked_by(PointerButton::Middle) {
            *maximize_toggle = true;
        }

        // ---- Terrain-Pinsel
        let hover_ray = resp.hover_pos().map(|p| self.cam.ray(rect, p));
        let mut brush_cursor: Option<DVec3> = None;
        if ed.tool == Tool::Disp {
            let pressed = ui.input(|i| i.pointer.primary_down());
            let mode = ed.dbrush.mode;
            if ctrl {
                use crate::disp::PaintMode::*;
                ed.dbrush.mode = match mode {
                    Raise => Lower,
                    Lower => Raise,
                    AlphaAdd => AlphaRemove,
                    AlphaRemove => AlphaAdd,
                    Smooth => Sharpen,
                    Sharpen => Smooth,
                    m => m,
                };
            }
            if pressed && (resp.hovered() || self.painting) {
                if let Some((o, d)) = hover_ray {
                    if !self.painting {
                        ed.begin_paint();
                        self.painting = true;
                    }
                    let dt = ui.input(|i| i.stable_dt).clamp(0.005, 0.1) as f64;
                    brush_cursor = ed.paint_disp(o, d, dt);
                    ui.ctx().request_repaint();
                }
            } else {
                self.painting = false;
                if let Some((o, d)) = hover_ray {
                    brush_cursor = ed.pick_3d(o, d).filter(|h| h.1.is_some_and(|(sid, _)| ed.map.solid(sid).is_some_and(|s| s.has_disp()))).map(|h| o + d * h.2);
                }
            }
            ed.dbrush.mode = mode;
        }

        // ---- Streu-Pinsel (Vegetation): malen, Shift radiert
        let mut scatter_cursor: Option<(DVec3, DVec3)> = None;
        if ed.scatter.active {
            let pressed = ui.input(|i| i.pointer.primary_down() || i.pointer.primary_pressed());
            let erase = ui.input(|i| i.modifiers.shift);
            if let Some((o, d)) = hover_ray {
                scatter_cursor = ed.surface_at(o, d);
                if pressed && (resp.hovered() || self.painting) {
                    let first = !self.painting;
                    self.painting = true;
                    ed.scatter_paint(o, d, first, erase);
                    ui.ctx().request_repaint();
                }
            }
            if !pressed {
                self.painting = false;
            }
        }

        // ---- Tile-Set: Feld unter der Maus, Klick/Ziehen setzt, Shift entfernt, R dreht
        let mut tile_cell: Option<(i64, i64)> = None;
        if ed.tiles.active {
            if resp.hovered() && ui.input(|i| i.key_pressed(Key::R) && !i.modifiers.command) {
                ed.tiles.rot = (ed.tiles.rot + 1) % 4;
            }
            tile_cell = hover_ray.and_then(|(o, d)| crate::tiles::cell_at(&ed.tiles, o, d));
            // ein kurzer Klick kann Drücken und Loslassen im selben Bild enthalten
            let pressed = ui.input(|i| i.pointer.primary_down() || i.pointer.primary_pressed());
            if pressed && resp.hovered() {
                if let Some(c) = tile_cell {
                    if self.tile_last != Some(c) {
                        if ui.input(|i| i.modifiers.shift) {
                            ed.tile_remove(c, true);
                        } else {
                            ed.tile_place(c);
                        }
                        self.tile_last = Some(c);
                    }
                }
            } else {
                self.tile_last = None;
            }
        }

        // ---- Mesh-/Patch-Griffe (ersetzen im Bearbeitungsmodus Auswahl und Gizmo)
        if ed.grip_mode() {
            ed.patch_sync();
            self.grip_interact(ui, rect, &resp, ed);
        } else {
            self.grip_drag = None;
            self.grip_box = None;
        }

        // ---- Overlay-Griffe (vor Gizmo und Rahmenauswahl)
        let shift = ui.input(|i| i.modifiers.shift);
        self.overlay_interact(ui, rect, &resp, ed, shift);

        // ---- Gizmo und Flächen ziehen
        // Strg+R: Auswahl dreht sich um die Hochachse zur Maus (Punkt auf der Höhe des Drehpunkts)
        let rotating = ed.rot_live.is_some();
        if rotating {
            if let (Some(p), Some(c)) = (resp.hover_pos(), ed.rot_live.as_ref().map(|r| r.center)) {
                let (o, d) = self.cam.ray(rect, p);
                if d.z.abs() > 1e-6 {
                    let t = (c.z - o.z) / d.z;
                    if t > 0.0 {
                        let q = o + d * t;
                        let free = ui.input(|i| i.modifiers.shift);
                        ed.rot_live_update(2, (q.y - c.y).atan2(q.x - c.x).to_degrees(), free);
                    }
                }
            }
            if resp.clicked_by(PointerButton::Primary) {
                ed.rot_live_commit();
            } else if resp.clicked_by(PointerButton::Secondary) {
                ed.rot_live_cancel();
            }
            ui.ctx().request_repaint();
        }
        if !rotating && resp.drag_started_by(PointerButton::Primary) && self.overlay_drag.is_none() && !ed.scatter.active && !ed.tiles.active && !ed.grip_mode() {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            if ed.tool == Tool::Select && self.gizmo && !ed.sel.is_empty() && selected_overlay(ed).is_none() {
                if let Some(kind) = self.gizmo_hit(rect, ed, press) {
                    let sb = ed.selection_bounds();
                    let center = sb.center();
                    let len = self.gizmo_len(center);
                    let mut began = false;
                    if shift && matches!(kind, GizmoKind::Axis(_)) {
                        ed.duplicate_in_place();
                        began = true;
                    }
                    self.gizmo_drag = Some(GizmoDrag { kind, base: Box::new(ed.map.clone()), start: press, center, len, began, bmin: sb.min });
                }
            }
            if ed.tool == Tool::Select && self.gizmo_drag.is_none() && ed.armed_prefab.is_none() {
                self.box_sel = Some(press);
            } else if ed.tool == Tool::Face && (shift || alt) {
                let (o, d) = self.cam.ray(rect, press);
                if let Some((_, Some((sid, side)), _)) = ed.pick_3d(o, d) {
                    if let Some(s) = ed.map.solid(sid) {
                        if let (Some(poly), Some(Some(pl))) = (s.polys.get(side), s.planes.get(side)) {
                            let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len().max(1) as f64;
                            if let (Some(p0), Some(p1)) = (self.project(rect, c), self.project(rect, c + pl.normal * 64.0)) {
                                self.face_drag = Some(FaceDrag { sid, side, base: Box::new(ed.map.clone()), start: press, dir: p1 - p0, extrude: alt, began: false });
                            }
                        }
                    }
                }
            }
        }
        if let Some(cur) = ui.input(|i| i.pointer.interact_pos()) {
            if let Some(g) = &mut self.gizmo_drag {
                let p0 = project_with(&self.cam, rect, g.center);
                let x = match g.kind {
                    GizmoKind::Axis(a) => {
                        let mut e = DVec3::ZERO;
                        e[a] = 1.0;
                        let p1 = project_with(&self.cam, rect, g.center + e * g.len);
                        match (p0, p1) {
                            (Some(p0), Some(p1)) => {
                                let dir = p1 - p0;
                                let t = (cur - g.start).dot(dir) / dir.length_sq().max(1e-3) * g.len as f32;
                                // Box-Ecke landet auf dem Raster (wie in Hammer)
                                let t = crate::geometry::snap(g.bmin[a] + t as f64, ed.snap_grid()) - g.bmin[a];
                                (t != 0.0).then(|| Xform::translate(e * t))
                            }
                            _ => None,
                        }
                    }
                    GizmoKind::Ring(a) => p0.and_then(|p0| {
                        let ang = |p: egui::Pos2| (-(p.y - p0.y)).atan2(p.x - p0.x) as f64;
                        let mut deg = (ang(cur) - ang(g.start)).to_degrees();
                        let mut e = DVec3::ZERO;
                        e[a] = 1.0;
                        if e.dot(self.cam.pos.as_dvec3() - g.center) < 0.0 {
                            deg = -deg;
                        }
                        if !shift {
                            deg = (deg / 15.0).round() * 15.0;
                        }
                        (deg.abs() > 1e-6).then(|| Xform::about(g.center, Xform::rotation(a, deg)))
                    }),
                };
                if let Some(x) = x {
                    if !g.began {
                        ed.checkpoint();
                        g.began = true;
                    }
                    ed.transform_selection_from(&g.base, &x, false);
                } else if g.began {
                    ed.transform_selection_from(&g.base, &Xform::translate(DVec3::ZERO), false);
                }
                ui.ctx().request_repaint();
            }
            if let Some(f) = &mut self.face_drag {
                let units = (cur - f.start).dot(f.dir) / f.dir.length_sq().max(1e-3) * 64.0;
                let units = crate::geometry::snap(units as f64, ed.snap_grid());
                if units != 0.0 && !f.began {
                    ed.checkpoint();
                    f.began = true;
                }
                if f.began {
                    if f.extrude {
                        ed.extrude_face_from(&f.base, f.sid, f.side, units);
                    } else if !ed.move_face_from(&f.base, f.sid, f.side, units) {
                        ed.status = crate::i18n::tr("Fläche lässt sich nicht so weit verschieben").into();
                    }
                    ed.status = format!("{} {units}", if f.extrude { "Extrudiert:" } else { crate::i18n::tr("Fläche verschoben:") });
                }
                ui.ctx().request_repaint();
            }
        }
        if let (Some(a), Some(b)) = (self.box_sel, ui.input(|i| i.pointer.interact_pos())) {
            let r = egui::Rect::from_two_pos(a, b);
            let painter = ui.painter_at(rect);
            painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(255, 220, 60, 25));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0f32, egui::Color32::from_rgb(255, 220, 60)), egui::StrokeKind::Inside);
            if resp.drag_stopped() {
                if r.width() > 4.0 && r.height() > 4.0 {
                    let ctrl = ui.input(|i| i.modifiers.command);
                    let touching = ui.input(|i| i.modifiers.alt);
                    if !ctrl {
                        ed.sel.clear();
                    }
                    let mut n = 0;
                    for (o, bb) in ed.selectable() {
                        if bb.is_empty() {
                            continue;
                        }
                        // Objekt wählen, wenn seine Mitte (Alt: irgendeine Ecke) im Rahmen liegt
                        let inside = if touching {
                            bb.corners().iter().chain(std::iter::once(&bb.center())).any(|c| self.project(rect, *c).is_some_and(|p| r.contains(p)))
                        } else {
                            self.project(rect, bb.center()).is_some_and(|p| r.contains(p))
                        };
                        if inside {
                            for x in ed.expand(o) {
                                ed.sel.insert(x);
                            }
                            n += 1;
                        }
                    }
                    ed.status = format!("{} im Rahmen ausgewählt", crate::i18n::count_de(n, "Objekt", "Objekte"));
                    ed.touch();
                }
                self.box_sel = None;
            }
            ui.ctx().request_repaint();
        }
        if resp.drag_stopped() {
            self.gizmo_drag = None;
            self.overlay_drag = None;
            self.face_drag = None;
            self.box_sel = None;
        }

        // ---- Prefabs: Drag & Drop aus der Bibliothek oder scharf geschaltetes Prefab per Klick
        let dropped = resp.dnd_release_payload::<crate::prefabs::PrefabDrag>().map(|p| p.0);
        let armed_click = ed.armed_prefab.filter(|_| resp.clicked_by(PointerButton::Primary) && !rotating);
        if let Some(idx) = dropped.or(armed_click) {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                let (o, d) = self.cam.ray(rect, p);
                match ed.pick_3d(o, d) {
                    Some((_, face, t)) => {
                        let n = face.and_then(|(sid, i)| ed.map.solid(sid).and_then(|s| s.planes.get(i).copied().flatten())).map(|p| p.normal);
                        ed.prefab_drop = Some((idx, o + d * t, n.or(Some(DVec3::Z))));
                    }
                    None => ed.prefab_drop = Some((idx, o + d * 256.0, None)),
                }
            }
        }
        if ed.armed_prefab.is_some() || egui::DragAndDrop::has_payload_of_type::<crate::prefabs::PrefabDrag>(ui.ctx()) {
            ui.painter_at(rect).text(rect.center_top() + Vec2::new(0.0, 22.0), Align2::CENTER_TOP, crate::i18n::tr("Prefab: auf eine Fläche klicken bzw. loslassen (Esc bricht ab)"), FontId::proportional(13.0), Color32::LIGHT_GREEN);
        }

        // ---- Spline: Klick setzt einen Punkt, Rechtsklick beendet
        if ed.spline.placing && ed.spline.open {
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (o, d) = self.cam.ray(rect, p);
                    if let Some(q) = crate::spline::point_from_ray(ed, o, d) {
                        ed.spline.points.push(q);
                        ed.touch();
                    }
                }
            } else if resp.clicked_by(PointerButton::Secondary) {
                ed.spline.placing = false;
            }
        }
        let spline_click = ed.spline.placing && ed.spline.open;
        // ---- Klicks
        if !rotating && !spline_click && resp.clicked_by(PointerButton::Primary) && ed.tool != Tool::Disp && armed_click.is_none() && !ed.scatter.active && !ed.tiles.active && !ed.grip_mode() {
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(rect, p);
                let hit = ed.pick_3d(o, d);
                let face_normal = |ed: &Editor, face: Option<(u32, usize)>| face.and_then(|(sid, i)| ed.map.solid(sid).and_then(|s| s.planes.get(i).copied().flatten())).map(|p| p.normal);
                if alt && ed.tool != Tool::Face {
                    // Pipette: Material der Fläche übernehmen
                    if let Some((sid, i)) = hit.and_then(|h| h.1) {
                        if let Some(side) = ed.map.solid(sid).and_then(|s| s.sides.get(i)) {
                            ed.material = side.material.clone();
                            ed.status = format!("Material übernommen: {}", ed.material);
                        }
                    }
                } else if ed.tool == Tool::Clip {
                    // 3-Punkt-Clip: Punkte auf Flächen setzen
                    if let Some((_, _, t)) = hit {
                        ed.clip3_add(o + d * t);
                    }
                } else if ed.tool == Tool::Measure {
                    if let Some((_, _, t)) = hit {
                        let p = o + d * t;
                        match self.measure_start.take() {
                            None => {
                                self.measure_start = Some(p);
                                ed.measure = Some((p, p));
                            }
                            Some(a) => {
                                ed.measure = Some((a, p));
                                ed.status = format!("Abstand: {:.1} Einheiten", a.distance(p));
                            }
                        }
                        ed.touch();
                    }
                } else if ed.pick_target.is_some() {
                    ed.select(hit.map(|h| h.0), false);
                } else {
                    match ed.tool {
                        Tool::Face => {
                            // Linksklick nimmt die Textur der Fläche auf (Pipette), Strg+Klick wählt Flächen aus;
                            // Klick ins Leere hebt die Flächenauswahl auf
                            let face = hit.and_then(|h| h.1);
                            if ctrl {
                                ed.select_face(face, true);
                            } else if let Some((sid, i)) = face {
                                if let Some(side) = ed.map.solid(sid).and_then(|s| s.sides.get(i)) {
                                    ed.material = side.material.clone();
                                    ed.status = crate::i18n::tr_dyn(&format!("Textur aufgenommen: {}", ed.material));
                                    ed.touch();
                                }
                            } else {
                                ed.select_face(None, false);
                            }
                        }
                        Tool::Entity => {
                            if let Some((_, face, t)) = hit {
                                let mut pos = o + d * t;
                                let size = ed.point_size(&ed.entity_class);
                                let n = face_normal(ed, face).unwrap_or(DVec3::Z);
                                if n.z > 0.7 {
                                    pos.z -= size.0.z;
                                } else {
                                    pos += n * size.1.abs().max_element();
                                }
                                // auf dem Raster einrasten – in den Richtungen entlang der Fläche
                                // (senkrecht dazu bleibt das Entity an der Fläche anliegen)
                                let g = ed.snap_grid();
                                let k = if n.z.abs() >= n.x.abs().max(n.y.abs()) { 2 } else if n.x.abs() >= n.y.abs() { 0 } else { 1 };
                                for i in (0..3).filter(|i| *i != k) {
                                    pos[i] = crate::geometry::snap(pos[i], g);
                                }
                                ed.create_point_entity(pos);
                            }
                        }
                        Tool::Decal => {
                            if let Some((_, face, t)) = hit {
                                let n = face_normal(ed, face).unwrap_or(DVec3::Z);
                                ed.create_decal(o + d * t + n);
                            }
                        }
                        Tool::Overlay => {
                            if let Some((_, Some((sid, i)), t)) = hit {
                                let n = face_normal(ed, Some((sid, i))).unwrap_or(DVec3::Z);
                                let side_id = ed.map.solid(sid).map(|s| s.sides[i].id).unwrap_or(0);
                                let (w, h) = mats.info(&ed.material).map(|i| (i.width as f64 * 0.25, i.height as f64 * 0.25)).unwrap_or((128.0, 128.0));
                                ed.create_overlay(o + d * t, n, side_id, DVec2::new(w / 2.0, h / 2.0));
                            } else {
                                ed.status = crate::i18n::tr("Overlay: auf eine Brush-Fläche klicken").into();
                            }
                        }
                        Tool::Magnify | Tool::Camera | Tool::Cordon | Tool::Clip | Tool::Measure => {}
                        _ => ed.select(hit.map(|h| h.0), ctrl),
                    }
                }
            }
        }
        // Flächen-Werkzeug: Rechtsklick wendet Material an, Alt+Rechtsklick richtet nahtlos aus
        if ed.tool == Tool::Face && resp.clicked_by(PointerButton::Secondary) {
            if let Some(p) = resp.interact_pointer_pos() {
                let (o, d) = self.cam.ray(rect, p);
                if let Some((_, Some(face), _)) = ed.pick_3d(o, d) {
                    if alt {
                        if !ed.wrap_texture(face) {
                            ed.status = crate::i18n::tr("Zuerst eine Quell-Fläche auswählen").into();
                        }
                    } else {
                        ed.checkpoint();
                        let mat = ed.material.clone();
                        if let Some(side) = ed.map.solid_mut(face.0).and_then(|s| s.sides.get_mut(face.1)) {
                            side.material = mat;
                        }
                    }
                }
            }
        }

        // ---- Rendern
        let painter = ui.painter_at(rect);
        if let Some(renderer) = self.renderer.clone() {
            let inst_count = ed.instances.len() as u64;
            let rev = (ed.rev, mats.generation + models.generation, inst_count, self.textured, self.mode, self.sky_preview, self.helpers);
            // Nachladende Texturen/Modelle lösen nur gedrosselt einen Neubau aus (große Maps!),
            // während des Ziehens höchstens alle 150 ms
            let only_content = self.built_rev.0 == rev.0 && (self.built_rev.3, self.built_rev.4, self.built_rev.5, self.built_rev.6) == (rev.3, rev.4, rev.5, rev.6);
            let dragging = ui.input(|i| i.pointer.any_down());
            let wait = if only_content { 0.5 } else if dragging { 0.15 } else { 0.0 };
            let due = self.last_build.elapsed().as_secs_f32() >= wait;
            if self.built_rev != rev && !due {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(wait));
            }
            if self.built_rev != rev && due {
                self.last_build = std::time::Instant::now();
                let opts = BuildOpts { textured: self.textured, mode: self.mode, sky_preview: self.sky_preview, helpers: self.helpers, vis: ed.auto_vis };
                let mesh = build_mesh(ed, mats, &opts, &mut self.solid_cache);
                self.lights = collect_lights(ed);
                self.sky = if self.sky_preview { sky_matrix(ed) } else { None };
                self.sky_box = if self.sky_preview { sky_room(ed).map(|s| s.2) } else { None };
                self.built_rev = rev;
                renderer.lock().unwrap().set_mesh(mesh);
            }
            self.gi_update(ui.ctx(), ed, mats);
            {
                let dt = ui.input(|i| i.stable_dt);
                if self.particles.update(ed, mats.fs(), self.cam.pos.as_dvec3(), dt) {
                    ui.ctx().request_repaint();
                }
            }
            let mut r = renderer.lock().unwrap();
            if r.animated.replace(false) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
            }
            if !mats.uploads.is_empty() {
                r.queue_textures(std::mem::take(&mut mats.uploads));
            }
            // Modelle & Instanzen
            let mut draws = Vec::new();
            let mut coll_lines: Vec<f32> = Vec::new();
            for e in &ed.map.entities {
                if e.is_brush_entity() || !ed.is_visible_entity_all(e) || !ed.in_cordon(&ed.entity_bounds(e)) {
                    continue;
                }
                let selected = ed.sel.contains(&Obj::Entity(e.id));
                let tint = if selected { [1.0, 0.15, 0.15, 0.45] } else { [0.0; 4] };
                let mat4 = xform_mat4(&Editor::entity_xform(e));
                if e.classname().eq_ignore_ascii_case("func_instance") {
                    if let Some(f) = e.get("file") {
                        let key = format!("inst:{f}");
                        if r.has_static(&key) {
                            draws.push(StaticDraw { key, model: mat4, tint, sky: false });
                        }
                    }
                    continue;
                }
                // Kollision: ausgewählte immer, sonst bis 4096 Einheiten (Ragdolls liegen im Knochenraum → weglassen)
                let want_coll = self.collision
                    && !e.classname().eq_ignore_ascii_case("prop_ragdoll")
                    && (selected || e.origin().is_some_and(|o| o.as_vec3().distance_squared(self.cam.pos) < 4096.0 * 4096.0));
                if !self.models && !want_coll {
                    continue;
                }
                let Some(path) = ed.model_path(e) else { continue };
                if ed.map.meshes.iter().any(|m| m.ent == e.id) {
                    continue; // Polygon-Mesh: das Netz selbst wird gezeigt
                }
                let Some(model) = models.get(&path) else { continue };
                if want_coll {
                    let c = if selected { [1.0, 0.85, 0.2, 1.0] } else { [0.25, 0.95, 1.0, 1.0] };
                    for x in &model.phys_edges {
                        let a = mat4.transform_point3(Vec3::new(x[0], x[1], x[2])).as_dvec3();
                        let b = mat4.transform_point3(Vec3::new(x[3], x[4], x[5])).as_dvec3();
                        line(&mut coll_lines, a, b, c);
                    }
                }
                if !self.models {
                    continue;
                }
                let skin = e.get("skin").and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let key = format!("mdl:{path}#{skin}");
                if !r.has_static(&key) {
                    let (d, b) = static_from_model(&model, skin);
                    r.add_static(key.clone(), d, b);
                }
                if self.textured && self.mode != RenderMode::Flat {
                    for mesh in &model.meshes {
                        if let Some(m) = model.material_for(skin, mesh.skinref) {
                            mats.request_gl(m);
                        }
                    }
                }
                if let (Some(sky), Some(b)) = (self.sky, self.sky_box) {
                    if e.origin().is_some_and(|o| b.contains_point(o)) {
                        draws.push(StaticDraw { key: key.clone(), model: sky * mat4, tint, sky: true });
                    }
                }
                draws.push(StaticDraw { key, model: mat4, tint, sky: false });
            }
            let mut lights = self.lights.0.clone();
            let cp = self.cam.pos;
            lights.sort_by(|a, b| a.pos.distance_squared(cp).partial_cmp(&b.pos.distance_squared(cp)).unwrap());
            let (mut dyn_lines, dyn_quads, dyn_batches, add_quads, add_batches) = self.dynamic_geometry(ed, mats, brush_cursor);
            let (sky2d, sky2d_batches) = if self.sky_preview { sky2d_geometry(ed, mats, self.cam.pos.as_dvec3()) } else { (Vec::new(), Vec::new()) };
            let fog = if self.fog_preview && self.mode != RenderMode::Wireframe { fog_settings(ed) } else { None };
            dyn_lines.extend_from_slice(&coll_lines);
            let frame = Arc::new(FrameData {
                mvp: self.cam.view_proj(rect.width() / rect.height()),
                cam: cp,
                view_mode: self.mode.shader_id(),
                show_alpha: ed.dbrush.show_alpha || ed.tool == Tool::Disp && matches!(ed.dbrush.mode, crate::disp::PaintMode::AlphaAdd | crate::disp::PaintMode::AlphaRemove),
                lights,
                ambient: self.lights.1,
                sun_dir: self.lights.2,
                sun_col: self.lights.3,
                sky: if self.clip_box.is_some() { None } else { self.sky },
                draws: draws.clone(),
                dyn_lines,
                dyn_quads,
                dyn_batches,
                add_quads,
                add_batches,
                sky2d,
                sky2d_batches,
                fog,
                wireframe: self.mode == RenderMode::Wireframe,
                clear: if self.sky.is_some() { [0.45, 0.6, 0.78] } else { [0.12, 0.12, 0.14] },
                shadows: self.shadows,
                gi: if self.mode == RenderMode::Lighting && self.gi_on { self.gi_grid.clone() } else { None },
                clip: self.clip_box,
            });
            // Spielersicht (Skybox-Modus): ganze Map mit 3D-Skybox aus der gemerkten Kamera
            let pip_frame = self.pip.as_ref().map(|pc| {
                let pr = Self::pip_rect(rect);
                let (sky2d, sky2d_batches) = if self.sky_preview { sky2d_geometry(ed, mats, pc.pos.as_dvec3()) } else { (Vec::new(), Vec::new()) };
                let mut pl = self.lights.0.clone();
                pl.sort_by(|a, b| a.pos.distance_squared(pc.pos).partial_cmp(&b.pos.distance_squared(pc.pos)).unwrap());
                (
                    pr,
                    Arc::new(FrameData {
                        mvp: pc.view_proj(pr.width() / pr.height()),
                        cam: pc.pos,
                        view_mode: self.mode.shader_id(),
                        lights: pl,
                        ambient: self.lights.1,
                        sun_dir: self.lights.2,
                        sun_col: self.lights.3,
                        sky: self.sky,
                        draws: draws.clone(),
                        sky2d,
                        sky2d_batches,
                        fog,
                        wireframe: self.mode == RenderMode::Wireframe,
                        clear: if self.sky.is_some() { [0.45, 0.6, 0.78] } else { [0.12, 0.12, 0.14] },
                        ..Default::default()
                    }),
                )
            });
            drop(r);
            let rr = renderer.clone();
            let cb = egui_glow::CallbackFn::new(move |_info, painter| {
                rr.lock().unwrap().paint(painter.gl(), &frame);
            });
            painter.add(egui::PaintCallback { rect, callback: Arc::new(cb) });
            if let Some((pr, pf)) = pip_frame {
                let rr = renderer.clone();
                let cb = egui_glow::CallbackFn::new(move |_info, painter| {
                    rr.lock().unwrap().paint(painter.gl(), &pf);
                });
                // eigener Zuschnitt: sonst löscht die Spielersicht die ganze 3D-Ansicht
                painter.with_clip_rect(pr).add(egui::PaintCallback { rect: pr, callback: Arc::new(cb) });
                painter.rect_stroke(pr, 6.0, egui::Stroke::new(1.5f32, Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Outside);
                let label = crate::i18n::tr("Spielersicht (ziehen: umsehen)");
                let g = painter.layout_no_wrap(label.to_string(), FontId::proportional(11.0), Color32::WHITE);
                let lr = Rect::from_min_size(pr.left_top() + Vec2::new(6.0, 6.0), g.size() + Vec2::new(12.0, 6.0));
                painter.rect_filled(lr, 5.0, Color32::from_black_alpha(160));
                painter.galley(lr.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
            }
        } else {
            painter.rect_filled(rect, egui::CornerRadius::ZERO, Color32::from_rgb(30, 10, 10));
            painter.text(rect.center(), Align2::CENTER_CENTER, self.error.as_deref().unwrap_or(crate::i18n::tr("3D nicht verfügbar")), FontId::proportional(14.0), Color32::LIGHT_RED);
        }
        // Achsenkreuz unten links
        let o = rect.left_bottom() + Vec2::new(30.0, -30.0);
        let ax = [(glam::DVec3::X, Color32::from_rgb(255, 107, 107), "X"), (glam::DVec3::Y, Color32::from_rgb(92, 214, 122), "Y"), (glam::DVec3::Z, Color32::from_rgb(92, 155, 255), "Z")];
        let f = self.cam.forward().as_dvec3();
        let right = f.cross(glam::DVec3::Z).normalize_or(glam::DVec3::X);
        let up = right.cross(f);
        for (d, c, n) in ax {
            let v = Vec2::new(d.dot(right) as f32, -d.dot(up) as f32) * 20.0;
            painter.line_segment([o, o + v], egui::Stroke::new(2.0f32, c));
            painter.text(o + v * 1.3, Align2::CENTER_CENTER, n, FontId::monospace(9.5), c);
        }
        // Andere Teilnehmer: Kamera als Punkt mit Namen, Auswahl als Kasten
        for u in &ed.remote_users {
            let col = Color32::from_rgb(u.color[0], u.color[1], u.color[2]);
            let b = ed.remote_selection_bounds(u);
            if !b.is_empty() {
                let k = b.corners();
                let pr: Vec<Option<egui::Pos2>> = k.iter().map(|c| self.project(rect, *c)).collect();
                for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
                    if let (Some(a), Some(c)) = (pr[i], pr[j]) {
                        painter.line_segment([a, c], egui::Stroke::new(2.0f32, col));
                    }
                }
                if let Some(t) = pr[6].or(self.project(rect, b.center())) {
                    painter.text(t, Align2::LEFT_BOTTOM, &u.name, FontId::proportional(12.0), col);
                }
            }
            if let Some((cp, _)) = u.cam {
                if let Some(s) = self.project(rect, DVec3::new(cp[0] as f64, cp[1] as f64, cp[2] as f64)) {
                    if rect.contains(s) {
                        painter.circle_filled(s, 6.0, col);
                        painter.text(s + Vec2::new(9.0, 0.0), Align2::LEFT_CENTER, &u.name, FontId::proportional(12.0), col);
                    }
                }
            }
        }
        if let Some((a, b)) = ed.measure {
            if let Some(m) = self.project(rect, (a + b) * 0.5) {
                let d = b - a;
                painter.text(m, Align2::CENTER_BOTTOM, crate::i18n::tr_dyn(&format!("{:.1}  (Δ {:.0} {:.0} {:.0})", d.length(), d.x, d.y, d.z)), FontId::monospace(12.0), Color32::from_rgb(80, 255, 255));
            }
        }
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        self.overlay_draw(&painter, rect, ed, hover);
        if ed.grip_mode() {
            self.grip_draw(&painter, rect, ed, ui.input(|i| i.pointer.hover_pos()));
        }
        if let Some(c) = tile_cell {
            // Feldumriss auf der Bauebene, Pfeil zeigt die Nordseite (Wand bei Drehung 0)
            let t = &ed.tiles;
            let o = DVec3::new(c.0 as f64 * t.size, c.1 as f64 * t.size, t.level + 1.0);
            let k = [o, o + DVec3::X * t.size, o + DVec3::new(t.size, t.size, 0.0), o + DVec3::Y * t.size];
            let col = if ui.input(|i| i.modifiers.shift) { Color32::from_rgb(255, 110, 110) } else { Color32::from_rgb(120, 200, 255) };
            for i in 0..4 {
                if let (Some(a), Some(b)) = (self.project(rect, k[i]), self.project(rect, k[(i + 1) % 4])) {
                    painter.line_segment([a, b], egui::Stroke::new(2.0f32, col));
                }
            }
            let mid = o + DVec3::new(t.size / 2.0, t.size / 2.0, 0.0);
            let dir = match t.rot % 4 {
                1 => -DVec3::X,
                2 => -DVec3::Y,
                3 => DVec3::X,
                _ => DVec3::Y,
            };
            if let (Some(a), Some(b)) = (self.project(rect, mid), self.project(rect, mid + dir * t.size * 0.4)) {
                painter.arrow(a, b - a, egui::Stroke::new(2.0f32, col));
            }
        }
        if let Some((c, n)) = scatter_cursor {
            let t1 = n.any_orthonormal_vector();
            let t2 = n.cross(t1);
            let r = ed.scatter.radius;
            let pts: Vec<Option<egui::Pos2>> = (0..=40).map(|k| {
                let a = k as f64 / 40.0 * std::f64::consts::TAU;
                self.project(rect, c + (t1 * a.cos() + t2 * a.sin()) * r + n * 2.0)
            }).collect();
            let col = if ui.input(|i| i.modifiers.shift) { Color32::from_rgb(255, 110, 110) } else { Color32::from_rgb(120, 230, 140) };
            for w in pts.windows(2) {
                if let (Some(a), Some(b)) = (w[0], w[1]) {
                    painter.line_segment([a, b], egui::Stroke::new(2.0f32, col));
                }
            }
        }
        if self.walker.on {
            let txt = crate::i18n::tr("Begehen · WASD · Leertaste springt · Strg duckt · Esc beendet");
            painter.text(rect.center_bottom() - Vec2::new(0.0, 14.0), Align2::CENTER_BOTTOM, txt, FontId::proportional(12.5), Color32::from_rgb(120, 230, 140));
            // Fadenkreuz
            let c = rect.center();
            painter.line_segment([c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)], egui::Stroke::new(1.5f32, Color32::WHITE));
            painter.line_segment([c - Vec2::new(0.0, 6.0), c + Vec2::new(0.0, 6.0)], egui::Stroke::new(1.5f32, Color32::WHITE));
        }
        if ed.tool == Tool::Disp {
            let b = ed.dbrush;
            painter.text(rect.right_bottom() + Vec2::new(-64.0, -8.0), Align2::RIGHT_BOTTOM, crate::i18n::tr_dyn(&format!("{} · R {} · S {} · Strg = umkehren", crate::i18n::tr(b.mode.label()), b.radius, b.strength)), FontId::proportional(12.0), Color32::WHITE);
        }
    }

    /// GI-Berechnung starten/neu starten (0,8 s nach der letzten Änderung) und Ergebnisse abholen
    fn gi_update(&mut self, ctx: &egui::Context, ed: &Editor, mats: &MaterialSystem) {
        if self.mode != RenderMode::Lighting || !self.gi_on {
            self.gi_job = None;
            self.gi_wait = None;
            return;
        }
        let rev = (ed.rev, mats.generation);
        if self.gi_rev != Some(rev) {
            let w = *self.gi_wait.get_or_insert_with(std::time::Instant::now);
            if w.elapsed().as_secs_f32() >= 0.8 || self.gi_grid.is_none() {
                self.gi_job = Some(crate::gi::start(gi_scene(ed, mats)));
                self.gi_rev = Some(rev);
                self.gi_wait = None;
                self.gi_progress = (0, crate::gi::PASSES);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if let Some(job) = &self.gi_job {
            let mut got = false;
            while let Ok((g, p, t)) = job.rx.try_recv() {
                self.gi_grid = Some(g);
                self.gi_progress = (p, t);
                got = true;
            }
            if got {
                ctx.request_repaint();
            }
            if self.gi_progress.0 < self.gi_progress.1 {
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
    }

    /// Fortschritt der GI-Berechnung für die Anzeige (None = aus oder fertig)
    pub fn gi_status(&self) -> Option<String> {
        (self.mode == RenderMode::Lighting && self.gi_on && self.gi_progress.0 < self.gi_progress.1)
            .then(|| crate::i18n::tr_dyn(&format!("Indirektes Licht: {} %", self.gi_progress.0 * 100 / self.gi_progress.1.max(1))))
    }

    fn project(&self, rect: Rect, p: DVec3) -> Option<egui::Pos2> {
        project_with(&self.cam, rect, p)
    }

    /// Bildschirmpositionen der Overlay-Griffe: (Ecken, Mitte, Drehgriff, Mitte der oberen Kante)
    fn overlay_screen(&self, rect: Rect, g: &OverlayGeom) -> Option<([egui::Pos2; 4], egui::Pos2, egui::Pos2, egui::Pos2)> {
        let mut c = [egui::Pos2::ZERO; 4];
        for (k, w) in g.corners().iter().enumerate() {
            c[k] = self.project(rect, *w)?;
        }
        let center = egui::pos2(c.iter().map(|p| p.x).sum::<f32>() / 4.0, c.iter().map(|p| p.y).sum::<f32>() / 4.0);
        // obere Kante = Kante mit der kleinsten mittleren y-Koordinate
        let (mut best, mut by) = (0, f32::MAX);
        for k in 0..4 {
            let m = (c[k].y + c[(k + 1) % 4].y) / 2.0;
            if m < by {
                by = m;
                best = k;
            }
        }
        let mid = c[best] + (c[(best + 1) % 4] - c[best]) * 0.5;
        let dir = (mid - center).normalized();
        let dir = if dir.is_finite() && dir.length() > 0.1 { dir } else { Vec2::new(0.0, -1.0) };
        Some((c, center, mid + dir * 32.0, mid))
    }

    /// Griff unter dem Mauszeiger (Index, Weltposition)
    fn grip_at(&self, rect: Rect, grips: &[(usize, DVec3)], p: egui::Pos2) -> Option<(usize, DVec3)> {
        grips.iter().filter_map(|(i, w)| self.project(rect, *w).map(|s| (*i, *w, s.distance(p)))).filter(|x| x.2 < 9.0).min_by(|a, b| a.2.total_cmp(&b.2)).map(|x| (x.0, x.1))
    }

    /// Griffe anklicken, ziehen (in der Bildebene, auf das Raster) und per Rahmen auswählen
    fn grip_interact(&mut self, ui: &egui::Ui, rect: Rect, resp: &egui::Response, ed: &mut Editor) {
        let shift = ui.input(|i| i.modifiers.shift);
        let grips = ed.grips();
        let fwd = self.cam.forward().as_dvec3();
        let plane_hit = |o: DVec3, d: DVec3, p: DVec3| -> Option<DVec3> {
            let den = d.dot(fwd);
            (den.abs() > 1e-9).then(|| o + d * ((p - o).dot(fwd) / den))
        };
        if resp.drag_started_by(PointerButton::Primary) {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            if let Some((i, w)) = self.grip_at(rect, &grips, press) {
                if !ed.grip_sel().contains(&i) {
                    ed.grip_select(Some(i), shift);
                }
                let (o, d) = self.cam.ray(rect, press);
                if let (Some(base), Some(start)) = (ed.grip_base(), plane_hit(o, d, w)) {
                    self.grip_drag = Some(GripDrag { base, start, began: false });
                }
            } else {
                self.grip_box = Some(press);
            }
        }
        if resp.dragged() {
            if let (Some(g), Some(cur)) = (&mut self.grip_drag, ui.input(|i| i.pointer.interact_pos())) {
                let (o, d) = self.cam.ray(rect, cur);
                if let Some(hit) = plane_hit(o, d, g.start) {
                    let grid = if ed.snap { ed.snap_grid() } else { 1.0 };
                    let dl = (hit - g.start).to_array().map(|v| crate::geometry::snap(v, grid));
                    let dl = DVec3::from_array(dl);
                    if !g.began {
                        if dl.length_squared() < 1e-9 {
                            return;
                        }
                        ed.checkpoint();
                        g.began = true;
                    }
                    ed.grip_drag(&g.base, dl);
                    ui.ctx().request_repaint();
                }
            }
        }
        if resp.drag_stopped() {
            if let (Some(a), Some(b)) = (self.grip_box.take(), ui.input(|i| i.pointer.interact_pos())) {
                let r = Rect::from_two_pos(a, b);
                if !shift {
                    ed.grip_select(None, false);
                }
                for (i, w) in &grips {
                    if self.project(rect, *w).is_some_and(|p| r.contains(p)) && !ed.grip_sel().contains(i) {
                        ed.grip_select(Some(*i), true);
                    }
                }
            }
            self.grip_drag = None;
        }
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                ed.grip_select(self.grip_at(rect, &grips, p).map(|g| g.0), shift);
            }
        }
    }

    /// Kanten bzw. Kontrollnetz und Griffe zeichnen
    fn grip_draw(&self, painter: &egui::Painter, rect: Rect, ed: &Editor, hover: Option<egui::Pos2>) {
        let th = crate::theme::t();
        let net = if ed.patch_edit.is_some() { Color32::from_rgb(255, 200, 80) } else { Color32::from_rgba_unmultiplied(120, 220, 255, 200) };
        for (a, b) in ed.grip_lines() {
            if let (Some(a), Some(b)) = (self.project(rect, a), self.project(rect, b)) {
                painter.line_segment([a, b], egui::Stroke::new(1.2f32, net));
            }
        }
        let sel = ed.grip_sel();
        let grips = ed.grips();
        let hot = hover.and_then(|h| self.grip_at(rect, &grips, h)).map(|g| g.0);
        let faces = ed.mesh_edit.as_ref().is_some_and(|m| m.faces_mode);
        for (i, w) in &grips {
            let Some(p) = self.project(rect, *w) else { continue };
            let on = sel.contains(i);
            let r = if hot == Some(*i) { 5.5 } else { 4.0 };
            let fill = if on { th.accent } else { Color32::from_rgb(0x1A, 0x1D, 0x23) };
            let stroke = egui::Stroke::new(1.5f32, if on { Color32::WHITE } else { net });
            if faces {
                painter.circle(p, r, fill, stroke);
            } else {
                painter.rect(Rect::from_center_size(p, Vec2::splat(r * 2.0)), 1.0, fill, stroke, egui::StrokeKind::Middle);
            }
        }
        if let (Some(a), Some(b)) = (self.grip_box, hover) {
            let r = Rect::from_two_pos(a, b);
            painter.rect(r, 0.0, crate::theme::alpha(th.accent, 0.1), egui::Stroke::new(1.0f32, th.accent), egui::StrokeKind::Middle);
        }
        let txt = if ed.patch_edit.is_some() {
            crate::i18n::tr("Bezier-Patch · Punkte ziehen, Shift fügt hinzu, Rahmen wählt mehrere")
        } else {
            crate::i18n::tr("Mesh bearbeiten · Punkte ziehen, Shift fügt hinzu, Rahmen wählt mehrere")
        };
        painter.text(rect.center_bottom() - Vec2::new(0.0, 14.0), Align2::CENTER_BOTTOM, txt, FontId::proportional(13.0), net);
    }

    fn overlay_interact(&mut self, ui: &egui::Ui, rect: Rect, resp: &egui::Response, ed: &mut Editor, shift: bool) {
        let Some(id) = selected_overlay(ed) else {
            self.overlay_drag = None;
            return;
        };
        let Some(g) = ed.map.entity(id).and_then(OverlayGeom::of) else { return };
        if resp.drag_started_by(PointerButton::Primary) {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            if let Some((c, center, rot, _)) = self.overlay_screen(rect, &g) {
                let kind = (0..4).find(|k| c[*k].distance(press) < 9.0).or_else(|| (rot.distance(press) < 9.0).then_some(4)).or_else(|| (center.distance(press) < 10.0).then_some(5));
                if let Some(kind) = kind {
                    let (o, d) = self.cam.ray(rect, press);
                    let start = g.plane_hit(o, d).unwrap_or(g.origin);
                    self.overlay_drag = Some(OverlayDrag { id, kind, start, base: g, began: false });
                }
            }
        }
        let Some(cur) = ui.input(|i| i.pointer.interact_pos()) else { return };
        let Some(dr) = self.overlay_drag.clone() else { return };
        if !resp.dragged() {
            return;
        }
        let (o, d) = self.cam.ray(rect, cur);
        let Some(hit) = dr.base.plane_hit(o, d) else { return };
        let grid = if ed.snap { ed.snap_grid() } else { 1.0 };
        let snap = |v: f64| crate::geometry::snap(v, grid);
        let mut g = dr.base.clone();
        match dr.kind {
            k @ 0..=3 => {
                let l = hit - g.origin;
                g.uv[k] = DVec3::new(snap(l.dot(g.u)), snap(l.dot(g.v)), 0.0);
            }
            4 => {
                let a = (dr.start - g.origin).normalize_or_zero();
                let b = (hit - g.origin).normalize_or_zero();
                let mut deg = g.n.dot(a.cross(b)).atan2(a.dot(b)).to_degrees();
                if !shift {
                    deg = (deg / 15.0).round() * 15.0;
                }
                let r = glam::DQuat::from_axis_angle(g.n, deg.to_radians());
                g.u = r * g.u;
                g.v = r * g.v;
            }
            _ => {
                let dl = hit - dr.start;
                g.origin += g.u * snap(dl.dot(g.u)) + g.v * snap(dl.dot(g.v));
            }
        }
        if !dr.began {
            ed.checkpoint();
            if let Some(x) = &mut self.overlay_drag {
                x.began = true;
            }
        }
        if let Some(e) = ed.map.entity_mut(dr.id) {
            g.write(e);
        }
        ed.touch();
        ui.ctx().request_repaint();
    }

    /// Griffe eines ausgewählten Overlays zeichnen (Pixelgröße unabhängig vom Zoom)
    fn overlay_draw(&self, painter: &egui::Painter, rect: Rect, ed: &Editor, hover: Option<egui::Pos2>) {
        let Some(id) = selected_overlay(ed) else { return };
        let Some(e) = ed.map.entity(id) else { return };
        let Some(g) = OverlayGeom::of(e) else { return };
        let Some((c, center, rot, mid)) = self.overlay_screen(rect, &g) else { return };
        let th = crate::theme::t();
        let acc = th.accent;
        let vp = Color32::from_rgb(0x1A, 0x1D, 0x23);
        painter.add(egui::Shape::convex_polygon(c.to_vec(), crate::theme::alpha(acc, 0.12), egui::Stroke::new(1.5f32, acc)));
        let dragging = self.overlay_drag.as_ref().map(|d| d.kind);
        let label_box = |at: egui::Pos2, text: String| {
            let gl = painter.layout_no_wrap(text, crate::theme::mono(10.5), Color32::WHITE);
            let r = Rect::from_min_size(at, gl.size() + Vec2::new(10.0, 6.0));
            painter.rect_filled(r, 4.0, Color32::from_rgb(0x2A, 0x2E, 0x38));
            painter.galley(r.min + Vec2::new(5.0, 3.0), gl, Color32::WHITE);
        };
        // Drehgriff mit Stiel
        painter.line_segment([mid, rot], egui::Stroke::new(1.2f32, acc));
        let rh = hover.is_some_and(|h| h.distance(rot) < 9.0) || dragging == Some(4);
        painter.circle(rot, if rh { 6.0 } else { 5.0 }, if rh { acc } else { vp }, egui::Stroke::new(1.5f32, acc));
        // Verschieben: Kreuz in der Mitte
        let mh = hover.is_some_and(|h| h.distance(center) < 10.0) || dragging == Some(5);
        let w: f32 = if mh { 2.2 } else { 1.5 };
        painter.line_segment([center - Vec2::new(7.0, 0.0), center + Vec2::new(7.0, 0.0)], egui::Stroke::new(w, acc));
        painter.line_segment([center - Vec2::new(0.0, 7.0), center + Vec2::new(0.0, 7.0)], egui::Stroke::new(w, acc));
        // Eckgriffe (rund)
        for (k, p) in c.iter().enumerate() {
            let act = dragging == Some(k);
            let hov = hover.is_some_and(|h| h.distance(*p) < 9.0);
            if act {
                painter.circle_filled(*p, 11.0, crate::theme::alpha(acc, 0.25));
                painter.circle_filled(*p, 6.0, acc);
                label_box(*p + Vec2::new(12.0, -22.0), format!("U {} · V {}", crate::map::fmt_num(g.uv[k].x), crate::map::fmt_num(g.uv[k].y)));
            } else if hov {
                painter.circle_filled(*p, 6.0, acc);
            } else {
                painter.circle(*p, 4.5, vp, egui::Stroke::new(1.5f32, acc));
            }
        }
        // Etikett unten rechts: Material und Größe
        let br = c.iter().copied().fold(c[0], |a, b| if b.x + b.y > a.x + a.y { b } else { a });
        let (w, h) = g.size();
        let mat = e.get("material").unwrap_or("?").to_ascii_lowercase();
        let short = mat.rsplit('/').next().unwrap_or(&mat).to_string();
        label_box(br + Vec2::new(8.0, 6.0), format!("{short} · {}×{}", crate::map::fmt_num(w), crate::map::fmt_num(h)));
    }

    fn gizmo_len(&self, c: DVec3) -> f64 {
        (self.cam.pos.as_dvec3().distance(c) * 0.15).max(8.0)
    }

    fn ring_points(c: DVec3, a: usize, r: f64) -> Vec<DVec3> {
        (0..=48)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / 48.0;
                let (s, co) = t.sin_cos();
                match a {
                    0 => c + DVec3::new(0.0, co, s) * r,
                    1 => c + DVec3::new(co, 0.0, s) * r,
                    _ => c + DVec3::new(co, s, 0.0) * r,
                }
            })
            .collect()
    }

    fn gizmo_hit(&self, rect: Rect, ed: &Editor, press: egui::Pos2) -> Option<GizmoKind> {
        let c = ed.selection_bounds().center();
        let len = self.gizmo_len(c);
        let p0 = self.project(rect, c)?;
        let seg_dist = |a: egui::Pos2, b: egui::Pos2| {
            let ab = b - a;
            let t = ((press - a).dot(ab) / ab.length_sq().max(1e-3)).clamp(0.0, 1.0);
            (a + ab * t).distance(press)
        };
        for a in 0..3 {
            let mut e = DVec3::ZERO;
            e[a] = 1.0;
            if let Some(p1) = self.project(rect, c + e * len) {
                if seg_dist(p0, p1) < 7.0 {
                    return Some(GizmoKind::Axis(a));
                }
            }
        }
        for a in 0..3 {
            let pts: Vec<egui::Pos2> = Self::ring_points(c, a, len * 0.75).into_iter().filter_map(|p| self.project(rect, p)).collect();
            if pts.windows(2).any(|w| seg_dist(w[0], w[1]) < 6.0) {
                return Some(GizmoKind::Ring(a));
            }
        }
        None
    }

    /// Pro Bild neu: Pinselkreis, Vertex-Marker, Gizmo, Messlinie, Editor-Sprites
    fn dynamic_geometry(&self, ed: &Editor, mats: &mut MaterialSystem, brush: Option<DVec3>) -> (Vec<f32>, Vec<f32>, Vec<Batch>, Vec<f32>, Vec<Batch>) {
        let mut lines = Vec::new();
        if self.gizmo && ed.tool == Tool::Select && !ed.sel.is_empty() && selected_overlay(ed).is_none() && !ed.grip_mode() {
            let c = ed.selection_bounds().center();
            let len = self.gizmo_len(c);
            let active = self.gizmo_drag.as_ref().map(|g| g.kind);
            for a in 0..3 {
                let mut e = DVec3::ZERO;
                e[a] = 1.0;
                let col = if active == Some(GizmoKind::Axis(a)) { [1.0, 1.0, 0.2, 1.0] } else { AXIS_COLORS[a] };
                line(&mut lines, c, c + e * len, col);
                // Pfeilspitze
                let side = if a == 2 { DVec3::X } else { DVec3::Z };
                line(&mut lines, c + e * len, c + e * len * 0.85 + side * len * 0.06, col);
                line(&mut lines, c + e * len, c + e * len * 0.85 - side * len * 0.06, col);
                let rc = if active == Some(GizmoKind::Ring(a)) { [1.0, 1.0, 0.2, 1.0] } else { [AXIS_COLORS[a][0], AXIS_COLORS[a][1], AXIS_COLORS[a][2], 0.6] };
                for w in Self::ring_points(c, a, len * 0.75).windows(2) {
                    line(&mut lines, w[0], w[1], rc);
                }
            }
        }
        // Navmesh-Vorschau um die Kamera
        if ed.nav_show {
            if let Some(nav) = &ed.nav_preview {
                for (a, b, c) in crate::navmesh::preview_lines(nav, self.cam.pos.as_dvec3(), 3000.0) {
                    line(&mut lines, a, b, c);
                }
            }
        }
        // Türen & Co.: wohin bewegen sie sich?
        for (a, b, c) in crate::door_preview::selected(ed) {
            line(&mut lines, a, b, c);
        }
        for (a, b, c) in crate::spline::preview_lines(&ed.spline) {
            line(&mut lines, a, b, c);
        }
        if let Some((a, b)) = ed.measure {
            line(&mut lines, a, b, [0.2, 1.0, 1.0, 1.0]);
            for p in [a, b] {
                for d in [DVec3::X, DVec3::Y, DVec3::Z] {
                    line(&mut lines, p - d * 4.0, p + d * 4.0, [0.2, 1.0, 1.0, 1.0]);
                }
            }
        }
        if let Some(c) = brush {
            let r = ed.dbrush.radius;
            let mut prev = None;
            for i in 0..=32 {
                let a = std::f64::consts::TAU * i as f64 / 32.0;
                let p = c + DVec3::new(a.cos() * r, a.sin() * r, 0.0);
                if let Some(q) = prev {
                    line(&mut lines, q, p, [1.0, 1.0, 0.2, 1.0]);
                }
                prev = Some(p);
            }
            line(&mut lines, c, c + DVec3::Z * 16.0, [1.0, 1.0, 0.2, 1.0]);
        }
        if ed.tool == Tool::Vertex {
            for (sid, v) in ed.vertex_handles() {
                let sel = ed.vsel.iter().any(|(s, q)| *s == sid && q.distance(v) < 0.01);
                let c = if sel { [1.0, 0.2, 0.2, 1.0] } else { [1.0, 1.0, 1.0, 1.0] };
                for d in [DVec3::X, DVec3::Y, DVec3::Z] {
                    line(&mut lines, v - d * 3.0, v + d * 3.0, c);
                }
            }
        }
        // Ausblend-Distanzen ausgewählter Props (gelb = Beginn, orange = ganz ausgeblendet)
        for id in ed.selected_entity_ids() {
            let Some(e) = ed.map.entity(id) else { continue };
            if !e.classname().starts_with("prop_") {
                continue;
            }
            let Some(o) = e.origin() else { continue };
            let f = |k: &str| e.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            for (r, col) in [(f("fademindist"), [1.0, 0.9, 0.2, 0.8]), (f("fademaxdist"), [1.0, 0.5, 0.1, 0.8])] {
                if r <= 0.0 || r > 20000.0 {
                    continue;
                }
                for a in 0..3 {
                    let pts = Self::ring_points(o, a, r);
                    for w in pts.windows(2) {
                        line(&mut lines, w[0], w[1], col);
                    }
                }
            }
        }
        // Clip-Vorschau: behaltene Teile weiß, verworfene rot; Punkte des 3-Punkt-Clips
        if ed.tool == Tool::Clip {
            let (keep, drop) = ed.clip_preview();
            for (list, c) in [(&drop, [1.0, 0.3, 0.3, 0.9]), (&keep, [1.0, 1.0, 1.0, 0.9])] {
                for s in list.iter() {
                    for poly in &s.polys {
                        for k in 0..poly.len() {
                            line(&mut lines, poly[k], poly[(k + 1) % poly.len()], c);
                        }
                    }
                }
            }
            let acc = [1.0, 0.55, 0.15, 1.0];
            for (k, p) in ed.clip3.iter().enumerate() {
                for a in [DVec3::X, DVec3::Y, DVec3::Z] {
                    line(&mut lines, *p - a * 6.0, *p + a * 6.0, acc);
                }
                if let Some(n) = ed.clip3.get(k + 1).or(if ed.clip3.len() == 3 { ed.clip3.first() } else { None }) {
                    line(&mut lines, *p, *n, acc);
                }
            }
        }
        // Spieler im laufenden Spiel (Live-Abgleich): Box 32×32×72 und Blickrichtung
        if let Some((pos, ang)) = ed.live_player {
            let c = [0.3, 1.0, 0.45, 1.0];
            box_lines(&mut lines, &Aabb::new(pos + DVec3::new(-16.0, -16.0, 0.0), pos + DVec3::new(16.0, 16.0, 72.0)).corners(), c);
            let (p, y) = (ang.x.to_radians(), ang.y.to_radians());
            let f = DVec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
            let eye = pos + DVec3::Z * 64.0;
            line(&mut lines, eye, eye + f * 48.0, c);
        }
        // Editor-Sprites (zur Kamera ausgerichtet)
        let mut quads = Bucketed::default();
        let right = self.cam.right().as_dvec3();
        let up = right.cross(self.cam.forward().as_dvec3()).normalize_or_zero();
        let cam = self.cam.pos.as_dvec3();
        let mut count = 0;
        for e in &ed.map.entities {
            if count > 600 || e.is_brush_entity() || !ed.is_visible_entity_all(e) {
                continue;
            }
            let class = e.classname();
            let Some(o) = e.origin() else { continue };
            if o.distance(cam) > 4096.0 || !ed.in_cordon(&Aabb::new(o, o)) {
                continue;
            }
            let selected = ed.sel.contains(&Obj::Entity(e.id));
            // env_sprite/env_glow: das echte Sprite zeigen (Material aus "model", Größe × scale, Farbe/Deckkraft)
            let real = if matches!(class, "env_sprite" | "env_glow" | "env_sprite_oriented") {
                e.get("model").map(|m| m.replace('\\', "/").to_ascii_lowercase()).map(|m| m.trim_end_matches(".spr").trim_end_matches(".vmt").to_string()).and_then(|m| mats.request_gl(&m).map(|i| (m, i)))
            } else {
                None
            };
            if let Some((mat, info)) = real {
                let scale = e.get("scale").and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0).unwrap_or(0.25);
                let half = (info.width.max(info.height) as f64 * scale * 0.5).clamp(1.0, 2048.0);
                let rc = e.get("rendercolor").and_then(parse_vec).unwrap_or(DVec3::splat(255.0)) / 255.0;
                let amt = e.get("renderamt").and_then(|v| v.parse::<f32>().ok()).unwrap_or(255.0) / 255.0;
                let corners = [o - right * half - up * half, o - right * half + up * half, o + right * half + up * half, o + right * half - up * half];
                let col = if selected { [1.0, 0.5, 0.5, amt.max(0.3)] } else { [rc.x as f32, rc.y as f32, rc.z as f32, amt] };
                quad(quads.buf(Some(normalize(&mat))), corners, [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], (cam - o).normalize_or_zero(), col);
                count += 1;
                continue;
            }
            let Some(sprite) = ed.fgd.class(class).and_then(|c| c.sprite.clone()) else { continue };
            let mat = sprite.trim_end_matches(".vmt").to_string();
            if mats.request_gl(&mat).is_none() {
                continue;
            }
            let s = 12.0;
            let corners = [o - right * s - up * s, o - right * s + up * s, o + right * s + up * s, o + right * s - up * s];
            let col = if selected { [1.0, 0.5, 0.5, 1.0] } else { [1.0; 4] };
            quad(quads.buf(Some(normalize(&mat))), corners, [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], (cam - o).normalize_or_zero(), col);
            count += 1;
        }
        // Seile (move_rope/keyframe_rope) als Band zur Kamera
        for rope in crate::ropes::ropes(ed) {
            let selected = ed.sel.contains(&Obj::Entity(rope.ent));
            let col = if selected { [1.0, 0.5, 0.5, 1.0] } else { [1.0; 4] };
            let key = mats.request_gl(&rope.material).map(|_| normalize(&rope.material));
            let mut along = 0.0;
            for w in rope.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let dir = (b - a).normalize_or_zero();
                let side = dir.cross(cam - (a + b) * 0.5).normalize_or_zero() * (rope.width * 0.5);
                let len = a.distance(b);
                let (u0, u1) = ((along / (rope.width * 4.0)) as f32, ((along + len) / (rope.width * 4.0)) as f32);
                along += len;
                let n = (cam - (a + b) * 0.5).normalize_or_zero();
                quad(quads.buf(key.clone()), [a - side, a + side, b + side, b - side], [[u0, 0.0], [u0, 1.0], [u1, 1.0], [u1, 0.0]], n, if key.is_some() { col } else { [0.15, 0.13, 0.1, 1.0] });
            }
            if selected {
                for w in rope.points.windows(2) {
                    line(&mut lines, w[0], w[1], [1.0, 0.3, 0.3, 1.0]);
                }
            }
        }
        // Partikel-Vorschau (zur Kamera ausgerichtet, $additive wird aufaddiert)
        let mut add = Bucketed::default();
        let fwd = self.cam.forward();
        let (rf, uf) = (right.as_vec3(), up.as_vec3());
        let mut seen: HashMap<String, Option<(String, bool)>> = HashMap::new();
        self.particles.for_each(|mat, p, r, c, rot| {
            let key = seen
                .entry(mat.to_string())
                .or_insert_with(|| if mat.is_empty() { None } else { mats.request_gl(mat).map(|i| (normalize(mat), i.additive || i.translucent && i.alpha < 0.6)) })
                .clone();
            let (key, additive) = key.unwrap_or_else(|| (SOFT_DISC.to_string(), true));
            let (sn, cs) = rot.to_radians().sin_cos();
            let (a, b) = ((rf * cs + uf * sn) * r, (uf * cs - rf * sn) * r);
            let k = [p - a - b, p - a + b, p + a + b, p + a - b].map(|v| v.as_dvec3());
            let buf = if additive { add.buf(Some(key)) } else { quads.buf(Some(key)) };
            quad(buf, k, [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], (-fwd).as_dvec3(), c);
        });
        let (q, b) = quads.finish();
        let (aq, ab) = add.finish();
        (lines, q, b, aq, ab)
    }
}

#[cfg(test)]
mod culling_tests {
    use super::*;

    #[test]
    fn frustum_culls_boxes_behind_camera() {
        let proj = Mat4::perspective_rh_gl(1.2, 1.5, 4.0, 20000.0);
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::X, Vec3::Z);
        let f = frustum_planes(&(proj * view));
        assert!(box_visible(&f, &[500.0, -50.0, -50.0, 600.0, 50.0, 50.0]));
        assert!(!box_visible(&f, &[-600.0, -50.0, -50.0, -500.0, 50.0, 50.0]));
        assert!(!box_visible(&f, &[500.0, 5000.0, -50.0, 600.0, 5100.0, 50.0]));
        assert!(box_visible(&f, &[-100.0, -100.0, -100.0, 100.0, 100.0, 100.0]));
    }
}

#[cfg(test)]
mod perf_tests {
    use super::*;

    /// Zeitmessung mit einer großen Map: AMBOSS_BIG=datei.vmf cargo test --release perf_big -- --ignored --nocapture
    #[test]
    #[ignore]
    fn perf_big() {
        let path = std::env::var("AMBOSS_BIG").unwrap();
        let t = std::time::Instant::now();
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.load(std::path::PathBuf::from(&path)).unwrap();
        println!("Laden: {:?} ({} Brushes, {} Entities)", t.elapsed(), ed.map.all_solids().count(), ed.map.entities.len());
        let mut mats = MaterialSystem::new();
        let opts = BuildOpts { textured: true, mode: RenderMode::Textured, sky_preview: true, helpers: true, vis: ed.auto_vis };
        let mut cache = SolidCache::default();
        for k in 0..3 {
            if k == 2 {
                // ein Brush geändert
                let id = ed.map.world.solids[10].id;
                ed.map.solid_mut(id).unwrap().editor.color = [1, 2, 3];
            }
            let t = std::time::Instant::now();
            let m = build_mesh(&ed, &mut mats, &opts, &mut cache);
            println!("build_mesh: {:?} (opaque {} Vertizes, {} Batches, lines {}, overlay {})", t.elapsed(), m.opaque.len() / STRIDE, m.opaque_batches.len(), m.lines.len() / STRIDE, m.overlay_lines.len() / STRIDE);
        }
        let t = std::time::Instant::now();
        for _ in 0..10 {
            ed.checkpoint();
        }
        println!("10× checkpoint: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let s = ed.map.to_vmf();
        println!("to_vmf: {:?} ({} MB)", t.elapsed(), s.len() / 1_000_000);
        let t = std::time::Instant::now();
        for _ in 0..10 {
            let _ = ed.selectable();
        }
        println!("10× selectable: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        for _ in 0..10 {
            let _ = ed.pick_2d(crate::geometry::Axis2D::Top, glam::DVec2::ZERO, 2.0);
        }
        println!("10× pick_2d: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let n: usize = ed.map.entities.iter().map(|e| ed.entity_bounds(e).size().length() as usize).sum();
        println!("entity_bounds alle: {:?} {n}", t.elapsed());
    }
}

/// Szene für die globale Beleuchtung: Welt-Brushes und func_detail (wie VRAD), Oberflächenfarbe aus
/// der mittleren Texturfarbe, Himmel aus toolsskybox
fn gi_scene(ed: &Editor, mats: &MaterialSystem) -> crate::gi::Scene {
    let (lights, ambient, sun_dir, sun_col) = collect_lights(ed);
    let mut tris = Vec::new();
    for (owner, s) in ed.map.solids_owned() {
        if let Some(e) = owner {
            let c = e.classname();
            if !(c == "func_detail" || c == "func_wall" || c == "func_brush") {
                continue;
            }
        }
        if !ed.is_visible_solid_all(owner, s) {
            continue;
        }
        let has_disp = s.has_disp();
        for (i, side) in s.sides.iter().enumerate() {
            let m = side.material.to_ascii_lowercase();
            let sky = m.contains("toolsskybox");
            let base = material_color(&side.material);
            if !sky && base[3] < 0.99 {
                continue;
            }
            let albedo = if m.contains("toolsnodraw") || m.contains("toolsblack") {
                Vec3::ZERO
            } else {
                match mats.info(&side.material) {
                    Some(info) => Vec3::new(info.avg[0] as f32, info.avg[1] as f32, info.avg[2] as f32) / 255.0,
                    None => Vec3::new(base[0], base[1], base[2]) * 0.8,
                }
                .powf(2.2)
                    * 0.9
            };
            let mut push = |a: DVec3, b: DVec3, c: DVec3| tris.push(crate::gi::Tri { a: a.as_vec3(), b: b.as_vec3(), c: c.as_vec3(), albedo, sky });
            if let Some(Some(g)) = s.disps.get(i) {
                for t in g.triangles() {
                    push(g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]);
                }
            } else if !has_disp {
                if let Some(poly) = s.polys.get(i) {
                    for k in 1..poly.len().saturating_sub(1) {
                        push(poly[0], poly[k], poly[k + 1]);
                    }
                }
            }
        }
    }
    // Polygon-Meshes werfen wie Props Schatten und färben das Licht
    for mo in &ed.map.meshes {
        let Some(e) = ed.map.entity(mo.ent) else { continue };
        if !ed.is_visible_entity_all(e) {
            continue;
        }
        let x = Editor::entity_xform(e);
        for (t, fi) in mo.mesh.triangles() {
            let mat = &mo.mesh.faces[fi].material;
            let base = material_color(mat);
            let albedo = match mats.info(mat) {
                Some(info) => Vec3::new(info.avg[0] as f32, info.avg[1] as f32, info.avg[2] as f32) / 255.0,
                None => Vec3::new(base[0], base[1], base[2]) * 0.8,
            }
            .powf(2.2)
                * 0.9;
            tris.push(crate::gi::Tri { a: x.apply(t[0]).as_vec3(), b: x.apply(t[1]).as_vec3(), c: x.apply(t[2]).as_vec3(), albedo, sky: false });
        }
    }
    let lights = lights.iter().map(|l| crate::gi::Light { pos: l.pos, color: l.color, spot: l.spot }).collect();
    crate::gi::Scene { tris, lights, ambient, sun_dir, sun_col, ambient_scale: 0.3 }
}

#[cfg(test)]
mod gi_tests {
    /// Laufzeit der GI auf einer großen Map: AMBOSS_GI_MAP=pfad cargo test gi_perf -- --ignored --nocapture
    #[test]
    #[ignore]
    fn gi_perf() {
        let Ok(p) = std::env::var("AMBOSS_GI_MAP") else { return };
        let mut ed = crate::editor::Editor::new(crate::fgd::Fgd::default());
        ed.map = crate::map::Map::from_vmf(&std::fs::read_to_string(p).unwrap()).unwrap();
        let mats = crate::materials::MaterialSystem::new();
        let t = std::time::Instant::now();
        let scene = super::gi_scene(&ed, &mats);
        println!("Szene: {} Dreiecke, {} Lichter, {} ms", scene.tris.len(), scene.lights.len(), t.elapsed().as_millis());
        let job = crate::gi::start(scene);
        while let Ok((g, p, n)) = job.rx.recv() {
            println!("Durchgang {p}/{n} nach {:.1} s – {:?} Sonden", t.elapsed().as_secs_f32(), g.dims);
        }
    }
}
