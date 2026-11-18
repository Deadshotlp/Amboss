//! Datenmodell einer Map und Umwandlung von/nach VMF.

use crate::disp::{Disp, DispGeom};
use crate::geometry::{base_winding, plane_points_for, polygons_from_planes, ray_convex, Aabb, Plane};
use crate::keyvalues::{self, KvBlock, KvItem};
use crate::transform::{transform_angles, Xform};
use glam::DVec3;

/// Gemeinsam genutztes Objekt mit Copy-on-Write: Kopien der Map (Rückgängig-Verlauf) teilen sich
/// unveränderte Brushes/Entities; erst beim Ändern wird das einzelne Objekt kopiert.
#[derive(Debug, Default)]
pub struct Shared<T: Clone>(std::sync::Arc<T>);

impl<T: Clone> Shared<T> {
    pub fn new(v: T) -> Self {
        Shared(std::sync::Arc::new(v))
    }
    /// Inhalt herausnehmen (kopiert nur, wenn noch geteilt)
    pub fn into_inner(self) -> T {
        std::sync::Arc::try_unwrap(self.0).unwrap_or_else(|a| (*a).clone())
    }
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        std::sync::Arc::ptr_eq(&a.0, &b.0)
    }
    /// Adresse des geteilten Werts (zum Erkennen von Änderungen)
    pub fn as_ptr(&self) -> usize {
        std::sync::Arc::as_ptr(&self.0) as *const u8 as usize
    }
}

impl<T: Clone> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T: Clone> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> std::ops::DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        std::sync::Arc::make_mut(&mut self.0)
    }
}

impl<T: Clone + PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other) || *self.0 == *other.0
    }
}

impl<T: Clone> From<T> for Shared<T> {
    fn from(v: T) -> Self {
        Shared::new(v)
    }
}

pub fn fmt_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        "0".into()
    } else {
        format!("{r}")
    }
}

pub fn fmt_vec(v: DVec3) -> String {
    format!("{} {} {}", fmt_num(v.x), fmt_num(v.y), fmt_num(v.z))
}

pub fn parse_vec(s: &str) -> Option<DVec3> {
    let cleaned = s.replace(['[', ']', '(', ')'], " ");
    let mut it = cleaned.split_whitespace().map(|x| x.parse::<f64>().ok());
    Some(DVec3::new(it.next()??, it.next()??, it.next()??))
}

#[derive(Debug, Clone, PartialEq)]
pub struct TexAxis {
    pub axis: DVec3,
    pub shift: f64,
    pub scale: f64,
}

impl TexAxis {
    /// Format: "[1 0 0 0] 0.25"
    pub fn parse(s: &str) -> Option<TexAxis> {
        let s = s.trim();
        let open = s.find('[')?;
        let close = s.find(']')?;
        let inner: Vec<f64> = s[open + 1..close].split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if inner.len() < 4 {
            return None;
        }
        let scale = s[close + 1..].trim().parse().unwrap_or(0.25);
        Some(TexAxis { axis: DVec3::new(inner[0], inner[1], inner[2]), shift: inner[3], scale })
    }

    pub fn to_vmf(&self) -> String {
        format!(
            "[{} {} {} {}] {}",
            fmt_num(self.axis.x),
            fmt_num(self.axis.y),
            fmt_num(self.axis.z),
            fmt_num(self.shift),
            fmt_num(self.scale)
        )
    }

    /// Texturkoordinate (in Texeln) eines Punktes
    pub fn coord(&self, p: DVec3) -> f64 {
        p.dot(self.axis) / self.scale.max(1e-6) + self.shift
    }
}

/// Weltausgerichtete Texturachsen wie in Hammer.
pub fn default_tex_axes(normal: DVec3, scale: f64) -> (TexAxis, TexAxis) {
    let a = normal.abs();
    let (u, v) = if a.z >= a.x && a.z >= a.y {
        (DVec3::X, -DVec3::Y)
    } else if a.x >= a.y {
        (DVec3::Y, -DVec3::Z)
    } else {
        (DVec3::X, -DVec3::Z)
    };
    (TexAxis { axis: u, shift: 0.0, scale }, TexAxis { axis: v, shift: 0.0, scale })
}

/// Flächenausgerichtete Texturachsen (Achsen liegen in der Fläche)
pub fn face_tex_axes(normal: DVec3, scale: f64) -> (TexAxis, TexAxis) {
    let (wu, wv) = default_tex_axes(normal, scale);
    let u = (wu.axis - normal * wu.axis.dot(normal)).normalize_or_zero();
    let mut v = normal.cross(u).normalize_or_zero();
    if v.dot(wv.axis) < 0.0 {
        v = -v;
    }
    (TexAxis { axis: u, shift: 0.0, scale }, TexAxis { axis: v, shift: 0.0, scale })
}

pub fn transform_tex_axis_pub(ax: &mut TexAxis, x: &Xform, reference: DVec3, lock_scale: bool) {
    transform_tex_axis(ax, x, reference, lock_scale)
}

/// Texturachse bei einer Transformation anpassen.
/// `lock_scale`: Textur skaliert mit (exakt), sonst bleibt die Skalierung gleich.
fn transform_tex_axis(ax: &mut TexAxis, x: &Xform, reference: DVec3, lock_scale: bool) {
    let b = x.inv_transpose() * ax.axis;
    let len = b.length();
    if len < 1e-9 {
        return;
    }
    if lock_scale {
        ax.shift -= x.t.dot(b) / ax.scale;
        ax.scale /= len;
        ax.axis = b / len;
    } else {
        // Referenzpunkt behält seine Texturkoordinate
        let u_ref = ax.coord(reference);
        ax.axis = b / len;
        let p = x.apply(reference);
        ax.shift = u_ref - p.dot(ax.axis) / ax.scale;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub id: u32,
    pub plane_pts: [DVec3; 3],
    pub material: String,
    pub uaxis: TexAxis,
    pub vaxis: TexAxis,
    pub rotation: f64,
    pub lightmapscale: i32,
    pub smoothing_groups: String,
    pub disp: Option<Disp>,
    /// Unbekannte Schlüssel/Blöcke werden unverändert erhalten
    pub extra: Vec<KvItem>,
}

impl Side {
    pub fn plane(&self) -> Option<Plane> {
        Plane::from_points(&self.plane_pts)
    }
}

/// Editor-Metadaten (editor-Block)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorInfo {
    pub color: [u8; 3],
    pub groupid: Option<u32>,
    pub visgroups: Vec<u32>,
    /// Mit "H" ausgeblendet (wird als hidden-Block gespeichert)
    pub hidden: bool,
    /// Gesperrt: nicht anklickbar (Amboss-Erweiterung)
    pub locked: bool,
    pub logicalpos: Option<[f64; 2]>,
    pub other: Vec<(String, String)>,
}

impl EditorInfo {
    fn parse(b: &KvBlock) -> EditorInfo {
        let mut e = EditorInfo::default();
        for (k, v) in b.pairs() {
            match k.to_ascii_lowercase().as_str() {
                "color" => {
                    if let Some(c) = parse_vec(v) {
                        e.color = [c.x as u8, c.y as u8, c.z as u8];
                    }
                }
                "groupid" => e.groupid = v.trim().parse().ok(),
                "visgroupid" => {
                    if let Ok(id) = v.trim().parse() {
                        e.visgroups.push(id);
                    }
                }
                "logicalpos" => {
                    let n: Vec<f64> = v.replace(['[', ']'], " ").split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    if n.len() >= 2 {
                        e.logicalpos = Some([n[0], n[1]]);
                    }
                }
                "visgroupshown" | "visgroupautoshown" => {}
                "amboss_locked" => e.locked = v.trim() == "1",
                _ => e.other.push((k.to_string(), v.to_string())),
            }
        }
        e
    }

    fn write(&self, map: &Map, entity: bool) -> KvBlock {
        let mut b = KvBlock::new("editor");
        b.push("color", format!("{} {} {}", self.color[0], self.color[1], self.color[2]));
        if let Some(g) = self.groupid {
            b.push("groupid", g.to_string());
        }
        for v in &self.visgroups {
            b.push("visgroupid", v.to_string());
        }
        b.push("visgroupshown", if map.visgroups_visible(&self.visgroups) { "1" } else { "0" });
        b.push("visgroupautoshown", "1");
        if self.locked {
            b.push("amboss_locked", "1");
        }
        for (k, v) in &self.other {
            b.push(k.clone(), v.clone());
        }
        if entity {
            let lp = self.logicalpos.unwrap_or([0.0, 0.0]);
            b.push("logicalpos", format!("[{} {}]", fmt_num(lp[0]), fmt_num(lp[1])));
        }
        b
    }
}

#[derive(Debug, Clone)]
pub struct Solid {
    pub id: u32,
    pub sides: Vec<Side>,
    pub editor: EditorInfo,
    pub extra: Vec<KvItem>,
    // --- berechnet ---
    pub planes: Vec<Option<Plane>>,
    pub polys: Vec<Vec<DVec3>>,
    pub disps: Vec<Option<DispGeom>>,
    pub bounds: Aabb,
}

/// Gleichheit ohne die berechneten Felder (Ebenen, Polygone, Grenzen)
impl PartialEq for Solid {
    fn eq(&self, o: &Solid) -> bool {
        self.id == o.id && self.sides == o.sides && self.editor == o.editor && self.extra == o.extra
    }
}

impl Solid {
    pub fn new(id: u32, sides: Vec<Side>) -> Solid {
        let mut s = Solid {
            id,
            sides,
            editor: EditorInfo { color: random_color(id), ..Default::default() },
            extra: Vec::new(),
            planes: vec![],
            polys: vec![],
            disps: vec![],
            bounds: Aabb::EMPTY,
        };
        s.update_geometry();
        s
    }

    pub fn update_geometry(&mut self) {
        self.planes = self.sides.iter().map(|s| s.plane()).collect();
        self.polys = polygons_from_planes(&self.planes);
        self.disps = self
            .sides
            .iter()
            .zip(&self.polys)
            .zip(&self.planes)
            .map(|((s, poly), pl)| s.disp.as_ref().and_then(|d| d.geometry(poly, pl.map(|p| p.normal).unwrap_or(DVec3::Z))))
            .collect();
        let mut b = Aabb::EMPTY;
        for p in self.polys.iter().flatten() {
            b.add_point(*p);
        }
        for g in self.disps.iter().flatten() {
            for p in &g.pos {
                b.add_point(*p);
            }
        }
        self.bounds = b;
    }

    pub fn has_disp(&self) -> bool {
        self.sides.iter().any(|s| s.disp.is_some())
    }

    pub fn is_valid(&self) -> bool {
        self.polys.iter().filter(|p| p.len() >= 3).count() >= 4 && !self.bounds.is_empty()
    }

    /// Eindeutige Eckpunkte
    pub fn vertices(&self) -> Vec<DVec3> {
        let mut out: Vec<DVec3> = Vec::new();
        for p in self.polys.iter().flatten() {
            if !out.iter().any(|q| q.distance(*p) < 0.01) {
                out.push(*p);
            }
        }
        out
    }

    /// Eindeutige Kanten (als Punktpaare)
    pub fn edges(&self) -> Vec<(DVec3, DVec3)> {
        let mut out: Vec<(DVec3, DVec3)> = Vec::new();
        for poly in &self.polys {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if !out.iter().any(|(x, y)| (x.distance(a) < 0.01 && y.distance(b) < 0.01) || (x.distance(b) < 0.01 && y.distance(a) < 0.01)) {
                    out.push((a, b));
                }
            }
        }
        out
    }

    /// Strahltest, bei dem ausgeblendete Seiten durchlässig sind: trifft dann die Rückseite der
    /// gegenüberliegenden Fläche (so wie sie in 3D zu sehen ist)
    pub fn ray_hit_visible(&self, origin: DVec3, dir: DVec3, hidden: impl Fn(&str) -> bool) -> Option<(f64, usize)> {
        if self.has_disp() || !self.sides.iter().any(|s| hidden(&s.material)) {
            return self.ray_hit(origin, dir);
        }
        self.bounds.ray_hit(origin, dir)?;
        let ((t_in, i_in), (t_out, i_out)) = crate::geometry::ray_convex_through(&self.planes, origin, dir)?;
        if !hidden(&self.sides[i_in].material) {
            return Some((t_in, i_in));
        }
        (!hidden(&self.sides[i_out].material) && t_out > 0.0).then_some((t_out, i_out))
    }

    pub fn ray_hit(&self, origin: DVec3, dir: DVec3) -> Option<(f64, usize)> {
        self.bounds.ray_hit(origin, dir)?;
        if self.has_disp() {
            // Bei Displacements zählt nur die Terrain-Oberfläche
            let mut best: Option<(f64, usize)> = None;
            for (i, g) in self.disps.iter().enumerate() {
                if let Some(g) = g {
                    if let Some((t, _)) = g.ray_hit(origin, dir) {
                        if best.map_or(true, |b| t < b.0) {
                            best = Some((t, i));
                        }
                    }
                }
            }
            return best;
        }
        ray_convex(&self.planes, origin, dir)
    }

    /// Verschiebt den Brush. Mit `tex_lock` bleiben Texturen an der Fläche "kleben".
    pub fn translate(&mut self, d: DVec3, tex_lock: bool) {
        for s in &mut self.sides {
            for p in &mut s.plane_pts {
                *p += d;
            }
            if tex_lock {
                s.uaxis.shift -= d.dot(s.uaxis.axis) / s.uaxis.scale;
                s.vaxis.shift -= d.dot(s.vaxis.axis) / s.vaxis.scale;
            }
            if let Some(disp) = &mut s.disp {
                disp.translate(d);
            }
        }
        self.update_geometry();
    }

    /// Beliebige affine Transformation (Drehen, Skalieren, Spiegeln, Scheren).
    pub fn transform(&mut self, x: &Xform, tex_lock: bool, scale_lock: bool) {
        let polys = self.polys.clone();
        let mirrored = x.m.determinant() < 0.0;
        for (side, poly) in self.sides.iter_mut().zip(polys.iter()) {
            let old_n = Plane::from_points(&side.plane_pts).map(|p| p.normal).unwrap_or(DVec3::Z);
            let n = x.normal(old_n);
            let reference = poly.first().copied().unwrap_or(side.plane_pts[0]);
            if poly.len() >= 3 {
                let moved: Vec<DVec3> = poly.iter().map(|p| x.apply(*p)).collect();
                if let Some(pts) = plane_points_for(n, &moved) {
                    side.plane_pts = pts;
                }
            } else {
                let mut pts = side.plane_pts.map(|p| x.apply(p));
                if mirrored {
                    pts.reverse();
                }
                side.plane_pts = pts;
            }
            if tex_lock {
                transform_tex_axis(&mut side.uaxis, x, reference, scale_lock);
                transform_tex_axis(&mut side.vaxis, x, reference, scale_lock);
            }
            if let Some(d) = &mut side.disp {
                d.transform(x);
            }
        }
        self.update_geometry();
    }

    pub fn set_material_all(&mut self, mat: &str) {
        for s in &mut self.sides {
            s.material = mat.to_string();
        }
    }

    /// Entfernt Seiten ohne Fläche und setzt Ebenenpunkte auf echte Eckpunkte
    pub fn cleanup(&mut self) {
        self.update_geometry();
        let polys = self.polys.clone();
        let mut keep = Vec::new();
        for (side, poly) in std::mem::take(&mut self.sides).into_iter().zip(polys) {
            if poly.len() >= 3 {
                let mut side = side;
                if let Some(n) = side.plane().map(|p| p.normal) {
                    if let Some(pts) = plane_points_for(n, &poly) {
                        side.plane_pts = pts;
                    }
                }
                keep.push(side);
            }
        }
        self.sides = keep;
        self.update_geometry();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: u32,
    /// Alle Schlüssel außer "id" (inkl. classname, origin)
    pub props: Vec<(String, String)>,
    pub solids: Vec<Shared<Solid>>,
    /// Ausgänge (connections-Block)
    pub outputs: Vec<(String, String)>,
    pub editor: EditorInfo,
    pub extra: Vec<KvItem>,
}

impl Entity {
    pub fn new(id: u32, classname: &str) -> Self {
        Entity {
            id,
            props: vec![("classname".into(), classname.into())],
            solids: Vec::new(),
            outputs: Vec::new(),
            editor: EditorInfo { color: [220, 30, 220], ..Default::default() },
            extra: Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: &str, val: impl Into<String>) {
        let val = val.into();
        if let Some(p) = self.props.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            p.1 = val;
        } else {
            self.props.push((key.to_string(), val));
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.props.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
    }

    pub fn classname(&self) -> &str {
        self.get("classname").unwrap_or("")
    }

    pub fn targetname(&self) -> Option<&str> {
        self.get("targetname").filter(|s| !s.is_empty())
    }

    pub fn origin(&self) -> Option<DVec3> {
        self.get("origin").and_then(parse_vec)
    }

    pub fn angles(&self) -> DVec3 {
        self.get("angles").and_then(parse_vec).unwrap_or(DVec3::ZERO)
    }

    pub fn is_brush_entity(&self) -> bool {
        !self.solids.is_empty()
    }

    pub fn translate(&mut self, d: DVec3, tex_lock: bool) {
        for s in &mut self.solids {
            s.translate(d, tex_lock);
        }
        if let Some(o) = self.origin() {
            self.set("origin", fmt_vec(o + d));
        }
        // Overlays: Basis mitverschieben
        if let Some(o) = self.get("BasisOrigin").and_then(parse_vec) {
            self.set("BasisOrigin", fmt_vec(o + d));
        }
    }

    pub fn transform(&mut self, x: &Xform, tex_lock: bool, scale_lock: bool) {
        for s in &mut self.solids {
            s.transform(x, tex_lock, scale_lock);
        }
        if let Some(o) = self.origin() {
            self.set("origin", fmt_vec(x.apply(o)));
        }
        if !self.is_brush_entity() && (self.get("angles").is_some() || self.get("model").is_some()) {
            let a = transform_angles(x, self.angles());
            self.set("angles", fmt_vec(a));
        }
        if let Some(o) = self.get("BasisOrigin").and_then(parse_vec) {
            self.set("BasisOrigin", fmt_vec(x.apply(o)));
            for k in ["BasisU", "BasisV"] {
                if let Some(v) = self.get(k).and_then(parse_vec) {
                    self.set(k, fmt_vec((x.m * v).normalize_or_zero()));
                }
            }
            if let Some(v) = self.get("BasisNormal").and_then(parse_vec) {
                self.set("BasisNormal", fmt_vec(x.normal(v)));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Visgroup {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    pub parent: Option<u32>,
    pub visible: bool,
    /// Ebene gesperrt (Objekte nicht anklickbar)
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: u32,
    pub editor: EditorInfo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraDef {
    pub pos: DVec3,
    pub look: DVec3,
    /// Name (nur Amboss; Hammer liest den Schlüssel nicht und lässt ihn stehen)
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cordon {
    pub bounds: Aabb,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct Map {
    pub world: Entity,
    pub entities: Vec<Shared<Entity>>,
    pub groups: Vec<Group>,
    pub visgroups: Vec<Visgroup>,
    pub cameras: Vec<CameraDef>,
    pub active_camera: usize,
    pub cordon: Option<Cordon>,
    /// versioninfo, viewsettings u. a. unbekannte Blöcke
    pub extra: Vec<KvBlock>,
    pub next_id: u32,
    pub grid: f64,
    /// ausgeblendete Objektarten der Ansichtsfilter („nodraw,triggers“; viewsettings → amboss_hidden)
    pub view_filter: String,
    /// Freie Polygon-Meshes (Begleitdatei <map>.meshes.json, nicht in der VMF)
    pub meshes: Vec<Shared<crate::mesh::MeshObj>>,
}

impl Default for Map {
    fn default() -> Self {
        Self::new()
    }
}

impl Map {
    pub fn new() -> Self {
        let mut world = Entity::new(1, "worldspawn");
        world.set("mapversion", "1");
        world.set("skyname", "sky_day01_01");
        world.set("maxpropscreenwidth", "-1");
        world.set("detailvbsp", "detail.vbsp");
        world.set("detailmaterial", "detail/detailsprites");
        Map {
            world,
            entities: Vec::new(),
            groups: Vec::new(),
            visgroups: Vec::new(),
            cameras: Vec::new(),
            active_camera: 0,
            cordon: None,
            extra: Vec::new(),
            next_id: 2,
            grid: 16.0,
            view_filter: String::new(),
            meshes: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Alle Solids (Welt + Brush-Entities) mit Besitzer (None = Welt)
    pub fn all_solids(&self) -> impl Iterator<Item = (Option<u32>, &Solid)> {
        self.world
            .solids
            .iter()
            .map(|s| (None, &**s))
            .chain(self.entities.iter().flat_map(|e| e.solids.iter().map(move |s| (Some(e.id), &**s))))
    }

    /// Alle Brushes mit ihrer Entity (ohne Suche nach der Entity-ID)
    pub fn solids_owned(&self) -> impl Iterator<Item = (Option<&Entity>, &Solid)> {
        self.world.solids.iter().map(|s| (None, &**s)).chain(self.entities.iter().flat_map(|e| e.solids.iter().map(move |s| (Some(&**e), &**s))))
    }

    pub fn entity(&self, id: u32) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id).map(|e| &**e)
    }

    pub fn entity_mut(&mut self, id: u32) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id).map(|e| &mut **e)
    }

    pub fn solid_mut(&mut self, id: u32) -> Option<&mut Solid> {
        if let Some(i) = self.world.solids.iter().position(|s| s.id == id) {
            return Some(&mut *self.world.solids[i]);
        }
        self.entities.iter_mut().flat_map(|e| e.solids.iter_mut()).find(|s| s.id == id).map(|s| &mut **s)
    }

    pub fn solid(&self, id: u32) -> Option<&Solid> {
        self.all_solids().map(|(_, s)| s).find(|s| s.id == id)
    }

    pub fn solid_owner(&self, id: u32) -> Option<Option<u32>> {
        self.all_solids().find(|(_, s)| s.id == id).map(|(o, _)| o)
    }

    /// Entfernt einen Solid (egal wo) und liefert ihn zurück
    pub fn take_solid(&mut self, id: u32) -> Option<Solid> {
        if let Some(i) = self.world.solids.iter().position(|s| s.id == id) {
            return Some(self.world.solids.remove(i).into_inner());
        }
        for e in &mut self.entities {
            if let Some(i) = e.solids.iter().position(|s| s.id == id) {
                return Some(e.solids.remove(i).into_inner());
            }
        }
        None
    }

    pub fn visgroups_visible(&self, ids: &[u32]) -> bool {
        ids.iter().all(|id| self.visgroups.iter().find(|v| v.id == *id).map_or(true, |v| v.visible && v.parent.map_or(true, |p| self.visgroups_visible(&[p]))))
    }

    pub fn visgroups_locked(&self, ids: &[u32]) -> bool {
        ids.iter().any(|id| self.visgroups.iter().any(|v| v.id == *id && v.locked))
    }

    pub fn targetnames(&self) -> Vec<String> {
        let mut v: Vec<String> = self.entities.iter().filter_map(|e| e.targetname().map(|s| s.to_string())).collect();
        v.sort();
        v.dedup();
        v
    }

    // ---------------------------------------------------------------- Erzeugen

    pub fn make_box(&mut self, bounds: Aabb, material: &str, tex_scale: f64) -> Solid {
        self.solid_from_points(&bounds.corners(), material, tex_scale).expect("Quader ist immer gültig")
    }

    /// Neue Seite aus einer Ebene (Ebenenpunkte werden später auf Eckpunkte gesetzt)
    pub fn side_from_plane(&mut self, plane: Plane, material: &str, tex_scale: f64) -> Side {
        let w = base_winding(&plane);
        let pts = plane_points_for(plane.normal, &w).unwrap_or([w[0], w[1], w[2]]);
        let (u, v) = default_tex_axes(plane.normal, tex_scale);
        Side {
            id: self.alloc_id(),
            plane_pts: pts,
            material: material.to_string(),
            uaxis: u,
            vaxis: v,
            rotation: 0.0,
            lightmapscale: 16,
            smoothing_groups: "0".into(),
            disp: None,
            extra: Vec::new(),
        }
    }

    pub fn solid_from_planes(&mut self, planes: &[Plane], material: &str, tex_scale: f64) -> Option<Solid> {
        let sides: Vec<Side> = planes.iter().map(|p| self.side_from_plane(*p, material, tex_scale)).collect();
        let id = self.alloc_id();
        let mut s = Solid::new(id, sides);
        s.cleanup();
        s.is_valid().then_some(s)
    }

    /// Konvexe Hülle einer Punktmenge als Brush
    pub fn solid_from_points(&mut self, pts: &[DVec3], material: &str, tex_scale: f64) -> Option<Solid> {
        let planes = crate::geometry::hull_planes(pts);
        if planes.len() < 4 {
            return None;
        }
        self.solid_from_planes(&planes, material, tex_scale)
    }

    /// Neue IDs für einen kopierten Solid vergeben
    pub fn reassign_ids(&mut self, s: &mut Solid) {
        s.id = self.alloc_id();
        for side in &mut s.sides {
            side.id = self.alloc_id();
        }
    }

    pub fn reassign_entity_ids(&mut self, e: &mut Entity) {
        e.id = self.alloc_id();
        for s in &mut e.solids {
            self.reassign_ids(s);
        }
    }

    // ---------------------------------------------------------------- VMF I/O

    pub fn from_vmf(src: &str) -> Result<Map, String> {
        let blocks = keyvalues::parse(src).map_err(|e| e.to_string())?;
        let mut map = Map::new();
        map.extra.clear();
        let mut max_id = 1u32;
        let mut have_world = false;
        for b in blocks {
            match b.name.to_ascii_lowercase().as_str() {
                "world" => {
                    let (w, groups) = parse_world(&b, &mut max_id);
                    map.world = w;
                    map.groups = groups;
                    have_world = true;
                }
                "entity" => map.entities.push(parse_entity(&b, &mut max_id).into()),
                "hidden" => {
                    for c in b.blocks() {
                        if c.name.eq_ignore_ascii_case("entity") {
                            let mut e = parse_entity(c, &mut max_id);
                            e.editor.hidden = true;
                            map.entities.push(e.into());
                        }
                    }
                }
                "visgroups" => parse_visgroups(&b, None, &mut map.visgroups),
                "cameras" => {
                    map.active_camera = b.get("activecamera").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0).max(0) as usize;
                    for c in b.blocks() {
                        if let (Some(p), Some(l)) = (c.get("position").and_then(parse_vec), c.get("look").and_then(parse_vec)) {
                            map.cameras.push(CameraDef { pos: p, look: l, name: c.get("name").unwrap_or("").to_string() });
                        }
                    }
                }
                "cordon" => {
                    if let (Some(a), Some(c)) = (b.get("mins").and_then(parse_vec), b.get("maxs").and_then(parse_vec)) {
                        map.cordon = Some(Cordon { bounds: Aabb::new(a, c), active: b.get("active") == Some("1") });
                    }
                }
                "cordons" => {
                    let active = b.get("active") == Some("1");
                    for c in b.blocks() {
                        if let Some(bx) = c.blocks().find(|x| x.name.eq_ignore_ascii_case("box")) {
                            if let (Some(a), Some(m)) = (bx.get("mins").and_then(parse_vec), bx.get("maxs").and_then(parse_vec)) {
                                map.cordon = Some(Cordon { bounds: Aabb::new(a, m), active: active && c.get("active") != Some("0") });
                                break;
                            }
                        }
                    }
                }
                _ => map.extra.push(b),
            }
        }
        if !have_world {
            return Err("Keine 'world' in der VMF gefunden".into());
        }
        // Sichtbarkeit der Visgroups aus den Objekten ableiten
        let mut hidden_vg: Vec<u32> = Vec::new();
        let check = |ed: &EditorInfo, shown: bool, out: &mut Vec<u32>| {
            if !shown {
                out.extend(ed.visgroups.iter().copied());
            }
        };
        for (_, s) in map.all_solids() {
            check(&s.editor, !s.editor.other.iter().any(|(k, v)| k == "__hidden_vg" && v == "1"), &mut hidden_vg);
        }
        for e in &map.entities {
            check(&e.editor, !e.editor.other.iter().any(|(k, v)| k == "__hidden_vg" && v == "1"), &mut hidden_vg);
        }
        for v in &mut map.visgroups {
            if hidden_vg.contains(&v.id) {
                v.visible = false;
            }
        }
        strip_marker(&mut map);
        if let Some(n) = map.extra.iter().find(|b| b.name.eq_ignore_ascii_case("viewsettings")).and_then(|b| b.get("nGridSpacing")).and_then(|v| v.parse().ok()) {
            map.grid = n;
        }
        if let Some(f) = map.extra.iter().find(|b| b.name.eq_ignore_ascii_case("viewsettings")).and_then(|b| b.get("amboss_hidden")) {
            map.view_filter = f.to_string();
        }
        map.next_id = max_id.max(map.visgroups.iter().map(|v| v.id).max().unwrap_or(0)) + 1;
        Ok(map)
    }

    pub fn to_vmf(&self) -> String {
        let mut blocks: Vec<KvBlock> = Vec::new();
        let find_extra = |name: &str| self.extra.iter().find(|b| b.name.eq_ignore_ascii_case(name)).cloned();

        blocks.push(find_extra("versioninfo").unwrap_or_else(|| {
            let mut b = KvBlock::new("versioninfo");
            b.push("editorversion", "400");
            b.push("editorbuild", "8864");
            b.push("mapversion", "1");
            b.push("formatversion", "100");
            b.push("prefab", "0");
            b
        }));
        // Visgroups (verschachtelt)
        let mut vg = KvBlock::new("visgroups");
        for v in self.visgroups.iter().filter(|v| v.parent.is_none()) {
            vg.push_block(self.write_visgroup(v));
        }
        blocks.push(vg);
        let mut vs = find_extra("viewsettings").unwrap_or_else(|| {
            let mut b = KvBlock::new("viewsettings");
            b.push("bSnapToGrid", "1");
            b.push("bShowGrid", "1");
            b.push("bShowLogicalGrid", "0");
            b.push("nGridSpacing", "16");
            b.push("bShow3DGrid", "0");
            b
        });
        for it in &mut vs.items {
            if let KvItem::Pair(k, v) = it {
                if k.eq_ignore_ascii_case("nGridSpacing") {
                    *v = fmt_num(self.grid);
                }
            }
        }
        // Ansichtsfilter (Hammer ignoriert unbekannte Schlüssel); leer = Schlüssel entfernen
        vs.items.retain(|it| !matches!(it, KvItem::Pair(k, _) if k.eq_ignore_ascii_case("amboss_hidden")));
        if !self.view_filter.is_empty() {
            vs.push("amboss_hidden", self.view_filter.clone());
        }
        blocks.push(vs);
        let mut world = write_entity(&self.world, "world", self);
        for g in &self.groups {
            let mut gb = KvBlock::new("group");
            gb.push("id", g.id.to_string());
            gb.push_block(g.editor.write(self, false));
            world.push_block(gb);
        }
        // versteckte Welt-Brushes
        let hidden: Vec<&Solid> = self.world.solids.iter().filter(|s| s.editor.hidden).map(|s| &**s).collect();
        if !hidden.is_empty() {
            let mut h = KvBlock::new("hidden");
            for s in hidden {
                h.push_block(write_solid(s, self));
            }
            world.push_block(h);
        }
        blocks.push(world);
        for e in &self.entities {
            let b = write_entity(e, "entity", self);
            if e.editor.hidden {
                let mut h = KvBlock::new("hidden");
                h.push_block(b);
                blocks.push(h);
            } else {
                blocks.push(b);
            }
        }
        for b in &self.extra {
            let n = b.name.to_ascii_lowercase();
            if n != "versioninfo" && n != "visgroups" && n != "viewsettings" {
                blocks.push(b.clone());
            }
        }
        let mut cams = KvBlock::new("cameras");
        cams.push("activecamera", if self.cameras.is_empty() { "-1".to_string() } else { self.active_camera.min(self.cameras.len() - 1).to_string() });
        for c in &self.cameras {
            let mut cb = KvBlock::new("camera");
            cb.push("position", format!("[{}]", fmt_vec(c.pos)));
            cb.push("look", format!("[{}]", fmt_vec(c.look)));
            if !c.name.trim().is_empty() {
                cb.push("name", c.name.clone());
            }
            cams.push_block(cb);
        }
        blocks.push(cams);
        if let Some(c) = &self.cordon {
            let mut cb = KvBlock::new("cordon");
            cb.push("mins", format!("({})", fmt_vec(c.bounds.min)));
            cb.push("maxs", format!("({})", fmt_vec(c.bounds.max)));
            cb.push("active", if c.active { "1" } else { "0" });
            blocks.push(cb);
        }
        keyvalues::write(&blocks)
    }

    fn write_visgroup(&self, v: &Visgroup) -> KvBlock {
        let mut b = KvBlock::new("visgroup");
        b.push("name", v.name.clone());
        b.push("visgroupid", v.id.to_string());
        b.push("color", format!("{} {} {}", v.color[0], v.color[1], v.color[2]));
        if v.locked {
            b.push("amboss_locked", "1");
        }
        for c in self.visgroups.iter().filter(|c| c.parent == Some(v.id)) {
            b.push_block(self.write_visgroup(c));
        }
        b
    }
}

/// Entfernt die temporäre Markierung, die beim Laden für visgroupshown genutzt wird
fn strip_marker(map: &mut Map) {
    let strip = |e: &mut EditorInfo| e.other.retain(|(k, _)| k != "__hidden_vg");
    for s in &mut map.world.solids {
        strip(&mut s.editor);
    }
    for e in &mut map.entities {
        strip(&mut e.editor);
        for s in &mut e.solids {
            strip(&mut s.editor);
        }
    }
}

fn parse_visgroups(b: &KvBlock, parent: Option<u32>, out: &mut Vec<Visgroup>) {
    for c in b.blocks().filter(|c| c.name.eq_ignore_ascii_case("visgroup")) {
        let Some(id) = c.get("visgroupid").and_then(|v| v.trim().parse().ok()) else { continue };
        let color = c.get("color").and_then(parse_vec).map(|v| [v.x as u8, v.y as u8, v.z as u8]).unwrap_or([200, 200, 200]);
        let locked = c.get("amboss_locked") == Some("1");
        out.push(Visgroup { id, name: c.get("name").unwrap_or("Visgroup").to_string(), color, parent, visible: true, locked });
        parse_visgroups(c, Some(id), out);
    }
}

pub fn random_color(seed: u32) -> [u8; 3] {
    let mut x = seed.wrapping_mul(2654435761);
    x ^= x >> 13;
    let h = (x % 360) as f32;
    let (r, g, b) = hsv(h, 0.5, 0.95);
    [r, g, b]
}

pub fn hsv(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}

fn parse_points(s: &str) -> Option<[DVec3; 3]> {
    let nums: Vec<f64> = s.replace(['(', ')'], " ").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if nums.len() < 9 {
        return None;
    }
    Some([DVec3::new(nums[0], nums[1], nums[2]), DVec3::new(nums[3], nums[4], nums[5]), DVec3::new(nums[6], nums[7], nums[8])])
}

fn parse_id(b: &KvBlock, max_id: &mut u32) -> u32 {
    let id = b.get("id").and_then(|v| v.parse().ok()).unwrap_or(0);
    *max_id = (*max_id).max(id);
    id
}

fn parse_editor(b: &KvBlock) -> EditorInfo {
    let mut e = EditorInfo::parse(b);
    if b.get("visgroupshown") == Some("0") {
        e.other.push(("__hidden_vg".into(), "1".into()));
    }
    e
}

fn parse_side(b: &KvBlock, max_id: &mut u32) -> Option<Side> {
    let id = parse_id(b, max_id);
    let plane_pts = parse_points(b.get("plane")?)?;
    let mut side = Side {
        id,
        plane_pts,
        material: b.get("material").unwrap_or("TOOLS/TOOLSNODRAW").to_string(),
        uaxis: b.get("uaxis").and_then(TexAxis::parse).unwrap_or(TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 }),
        vaxis: b.get("vaxis").and_then(TexAxis::parse).unwrap_or(TexAxis { axis: -DVec3::Y, shift: 0.0, scale: 0.25 }),
        rotation: b.get("rotation").and_then(|v| v.parse().ok()).unwrap_or(0.0),
        lightmapscale: b.get("lightmapscale").and_then(|v| v.parse().ok()).unwrap_or(16),
        smoothing_groups: b.get("smoothing_groups").unwrap_or("0").to_string(),
        disp: None,
        extra: Vec::new(),
    };
    const KNOWN: [&str; 8] = ["id", "plane", "material", "uaxis", "vaxis", "rotation", "lightmapscale", "smoothing_groups"];
    for item in &b.items {
        match item {
            KvItem::Pair(k, _) if KNOWN.iter().any(|n| k.eq_ignore_ascii_case(n)) => {}
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("dispinfo") => match Disp::parse(c) {
                Some(d) => side.disp = Some(d),
                None => side.extra.push(item.clone()),
            },
            _ => side.extra.push(item.clone()),
        }
    }
    Some(side)
}

fn parse_solid(b: &KvBlock, max_id: &mut u32) -> Solid {
    let id = parse_id(b, max_id);
    let mut sides = Vec::new();
    let mut extra = Vec::new();
    let mut editor = EditorInfo { color: random_color(id), ..Default::default() };
    for item in &b.items {
        match item {
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("side") => {
                if let Some(s) = parse_side(c, max_id) {
                    sides.push(s);
                }
            }
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("editor") => editor = parse_editor(c),
            KvItem::Pair(k, _) if k.eq_ignore_ascii_case("id") => {}
            _ => extra.push(item.clone()),
        }
    }
    let mut s = Solid::new(id, sides);
    s.editor = editor;
    s.extra = extra;
    s
}

fn parse_entity(b: &KvBlock, max_id: &mut u32) -> Entity {
    let id = parse_id(b, max_id);
    let mut e = Entity::new(id, "");
    e.props.clear();
    for item in &b.items {
        match item {
            KvItem::Pair(k, _) if k.eq_ignore_ascii_case("id") => {}
            KvItem::Pair(k, v) => e.props.push((k.clone(), v.clone())),
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("solid") => e.solids.push(parse_solid(c, max_id).into()),
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("hidden") => {
                for s in c.blocks().filter(|s| s.name.eq_ignore_ascii_case("solid")) {
                    let mut s = parse_solid(s, max_id);
                    s.editor.hidden = true;
                    e.solids.push(s.into());
                }
            }
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("connections") => {
                for (k, v) in c.pairs() {
                    e.outputs.push((k.to_string(), v.to_string()));
                }
            }
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("editor") => e.editor = parse_editor(c),
            KvItem::Block(_) => e.extra.push(item.clone()),
        }
    }
    e
}

fn parse_world(b: &KvBlock, max_id: &mut u32) -> (Entity, Vec<Group>) {
    let mut groups = Vec::new();
    let mut filtered = KvBlock::new(b.name.clone());
    for item in &b.items {
        match item {
            KvItem::Block(c) if c.name.eq_ignore_ascii_case("group") => {
                let id = parse_id(c, max_id);
                let editor = c.blocks().find(|x| x.name.eq_ignore_ascii_case("editor")).map(parse_editor).unwrap_or_default();
                groups.push(Group { id, editor });
            }
            _ => filtered.items.push(item.clone()),
        }
    }
    (parse_entity(&filtered, max_id), groups)
}

fn write_side(s: &Side) -> KvBlock {
    let mut b = KvBlock::new("side");
    b.push("id", s.id.to_string());
    let p = &s.plane_pts;
    b.push("plane", format!("({}) ({}) ({})", fmt_vec(p[0]), fmt_vec(p[1]), fmt_vec(p[2])));
    b.push("material", s.material.clone());
    b.push("uaxis", s.uaxis.to_vmf());
    b.push("vaxis", s.vaxis.to_vmf());
    b.push("rotation", fmt_num(s.rotation));
    b.push("lightmapscale", s.lightmapscale.to_string());
    b.push("smoothing_groups", s.smoothing_groups.clone());
    if let Some(d) = &s.disp {
        b.push_block(d.to_kv());
    }
    b.items.extend(s.extra.iter().cloned());
    b
}

fn write_solid(s: &Solid, map: &Map) -> KvBlock {
    let mut b = KvBlock::new("solid");
    b.push("id", s.id.to_string());
    for side in &s.sides {
        b.push_block(write_side(side));
    }
    b.items.extend(s.extra.iter().cloned());
    b.push_block(s.editor.write(map, false));
    b
}

fn write_entity(e: &Entity, block_name: &str, map: &Map) -> KvBlock {
    let mut b = KvBlock::new(block_name);
    b.push("id", e.id.to_string());
    if let Some(c) = e.get("classname") {
        b.push("classname", c);
    }
    for (k, v) in &e.props {
        if !k.eq_ignore_ascii_case("classname") {
            b.push(k.clone(), v.clone());
        }
    }
    if !e.outputs.is_empty() {
        let mut c = KvBlock::new("connections");
        for (k, v) in &e.outputs {
            c.push(k.clone(), v.clone());
        }
        b.push_block(c);
    }
    let world = block_name == "world";
    for s in e.solids.iter().filter(|s| !(world && s.editor.hidden)) {
        if s.editor.hidden && !world {
            let mut h = KvBlock::new("hidden");
            h.push_block(write_solid(s, map));
            b.push_block(h);
        } else {
            b.push_block(write_solid(s, map));
        }
    }
    b.items.extend(e.extra.iter().cloned());
    if !world {
        b.push_block(e.editor.write(map, true));
    }
    b
}

fn strip_ids(b: &mut KvBlock) {
    b.items.retain(|it| !matches!(it, KvItem::Pair(k, _) if k == "id"));
    for it in &mut b.items {
        if let KvItem::Block(c) = it {
            strip_ids(c);
        }
    }
}

impl Map {
    /// Inhalt eines Brushes ohne IDs (für Vergleiche zwischen Map-Ständen)
    pub fn solid_fingerprint(&self, s: &Solid) -> String {
        let mut b = write_solid(s, self);
        strip_ids(&mut b);
        keyvalues::write(&[b])
    }

    /// Inhalt einer Entity (inkl. Brushes) ohne IDs
    pub fn entity_fingerprint(&self, e: &Entity) -> String {
        let mut b = write_entity(e, "entity", self);
        strip_ids(&mut b);
        keyvalues::write(&[b])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_roundtrip() {
        let mut map = Map::new();
        let s = map.make_box(Aabb::new(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 128.0)), "DEV/DEV_MEASUREGENERIC01B", 0.25);
        assert!(s.is_valid());
        assert_eq!(s.sides.len(), 6);
        assert_eq!(s.bounds.min, DVec3::new(-64.0, -64.0, 0.0));
        assert_eq!(s.bounds.max, DVec3::new(64.0, 64.0, 128.0));
        for (side, poly) in s.sides.iter().zip(&s.polys) {
            let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
            assert!(side.plane().unwrap().normal.dot(c - s.bounds.center()) > 0.0);
        }
        map.world.solids.push(s.into());
        let mut e = Entity::new(map.alloc_id(), "info_player_start");
        e.set("origin", "0 0 1");
        e.editor.visgroups.push(7);
        map.entities.push(e.into());
        map.visgroups.push(Visgroup { id: 7, name: "Spawns".into(), color: [1, 2, 3], parent: None, visible: false, locked: true });
        map.groups.push(Group { id: 50, editor: EditorInfo::default() });
        map.world.solids[0].editor.groupid = Some(50);
        map.cameras.push(CameraDef { pos: DVec3::new(1.0, 2.0, 3.0), look: DVec3::new(4.0, 5.0, 6.0), name: "Eingang".into() });
        map.cordon = Some(Cordon { bounds: Aabb::new(DVec3::splat(-10.0), DVec3::splat(10.0)), active: true });

        let text = map.to_vmf();
        let back = Map::from_vmf(&text).unwrap();
        assert_eq!(back.world.solids.len(), 1);
        assert_eq!(back.world.solids[0].bounds, map.world.solids[0].bounds);
        assert_eq!(back.world.solids[0].editor.groupid, Some(50));
        assert_eq!(back.groups.len(), 1);
        assert_eq!(back.entities[0].classname(), "info_player_start");
        assert_eq!(back.visgroups.len(), 1);
        assert!(!back.visgroups[0].visible);
        assert!(back.visgroups[0].locked);
        assert_eq!(back.cameras, map.cameras);
        assert_eq!(back.cordon, map.cordon);
        assert!(back.next_id > map.world.solids[0].sides.last().unwrap().id);
    }

    #[test]
    fn hidden_roundtrip() {
        let mut map = Map::new();
        let mut s = map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(32.0)), "a", 0.25);
        s.editor.hidden = true;
        map.world.solids.push(s.into());
        let mut e = Entity::new(map.alloc_id(), "light");
        e.editor.hidden = true;
        map.entities.push(e.into());
        let back = Map::from_vmf(&map.to_vmf()).unwrap();
        assert!(back.world.solids[0].editor.hidden);
        assert!(back.entities[0].editor.hidden);
    }

    #[test]
    fn transforms_with_texture_lock() {
        let mut map = Map::new();
        let mut s = map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "a", 0.25);
        let top = s.sides.iter().position(|x| x.plane().unwrap().normal.z > 0.9).unwrap();
        let p = DVec3::new(10.0, 20.0, 64.0);
        let (u0, v0) = (s.sides[top].uaxis.coord(p), s.sides[top].vaxis.coord(p));
        let x = Xform::about(DVec3::new(32.0, 32.0, 0.0), Xform::rotation(2, 30.0));
        s.transform(&x, true, true);
        assert!(s.is_valid());
        let top2 = s.sides.iter().position(|x| x.plane().unwrap().normal.z > 0.9).unwrap();
        let p2 = x.apply(p);
        assert!((s.sides[top2].uaxis.coord(p2) - u0).abs() < 1e-6);
        assert!((s.sides[top2].vaxis.coord(p2) - v0).abs() < 1e-6);
        // Spiegeln erhält gültige, nach außen zeigende Ebenen
        let m = Xform::about(DVec3::ZERO, Xform::mirror(0));
        s.transform(&m, true, true);
        assert!(s.is_valid());
        for (side, poly) in s.sides.iter().zip(&s.polys) {
            let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
            assert!(side.plane().unwrap().normal.dot(c - s.bounds.center()) > 0.0);
        }
    }

    #[test]
    fn disp_in_vmf() {
        let mut map = Map::new();
        let mut s = map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(256.0, 256.0, 16.0)), "a", 0.25);
        let top = s.sides.iter().position(|x| x.plane().unwrap().normal.z > 0.9).unwrap();
        let mut d = Disp::new_flat(2, &s.polys[top], DVec3::Z).unwrap();
        d.distances[12] = 50.0;
        s.sides[top].disp = Some(d);
        s.update_geometry();
        assert_eq!(s.bounds.max.z, 66.0);
        map.world.solids.push(s.into());
        let back = Map::from_vmf(&map.to_vmf()).unwrap();
        assert!(back.world.solids[0].has_disp());
        assert_eq!(back.world.solids[0].bounds.max.z, 66.0);
    }
}
