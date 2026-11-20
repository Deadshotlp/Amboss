//! Automatische Optimierungen: KI-Nodes, Leckprüfung, func_detail, Areaportals, Lightmaps,
//! Cubemaps, Aufräumen, Props, Inhalte packen. Läuft auf Wunsch beim Kompilieren auf einer Kopie.

use crate::editor::{is_tool_material, Editor, Obj};
use crate::gamefs::GameFs;
use crate::geometry::{ray_convex, Aabb, Plane};
use crate::map::{default_tex_axes, fmt_vec, Entity, Solid};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OptSettings {
    /// Beim Kompilieren automatisch (auf einer Kopie) anwenden
    pub compile_auto: bool,
    pub abort_on_leak: bool,
    // KI
    pub ai_nodes: bool,
    pub node_spacing: f64,
    pub ai_air_nodes: bool,
    pub ai_door_links: bool,
    pub ai_hints: bool,
    pub navmesh: bool,
    // Sichtbarkeit
    pub leak_check: bool,
    pub nodraw: bool,
    pub auto_detail: bool,
    pub areaportals: bool,
    pub hint_suggestions: bool,
    // Licht
    pub lightmaps: bool,
    pub cubemaps: bool,
    pub light_check: bool,
    // Geometrie
    pub snap_verts: bool,
    pub merge_brushes: bool,
    pub remove_slivers: bool,
    pub simplify_disps: bool,
    // Props & Entities
    pub prop_fades: bool,
    pub prop_shadows: bool,
    pub remove_unused_logic: bool,
    pub pack_content: bool,
}

impl Default for OptSettings {
    fn default() -> Self {
        OptSettings {
            compile_auto: false,
            abort_on_leak: true,
            ai_nodes: true,
            node_spacing: 128.0,
            ai_air_nodes: false,
            ai_door_links: true,
            ai_hints: false,
            navmesh: false,
            leak_check: true,
            nodraw: true,
            auto_detail: true,
            areaportals: false,
            hint_suggestions: true,
            lightmaps: true,
            cubemaps: true,
            light_check: true,
            snap_verts: true,
            merge_brushes: false,
            remove_slivers: true,
            simplify_disps: true,
            prop_fades: true,
            prop_shadows: true,
            remove_unused_logic: true,
            pack_content: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub pos: DVec3,
    pub text: String,
    pub color: [u8; 3],
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub lines: Vec<String>,
    pub leak: Option<Vec<DVec3>>,
    pub links: Vec<(DVec3, DVec3, bool)>,
    pub markers: Vec<Marker>,
    pub changes: usize,
}

impl Report {
    fn say(&mut self, s: impl Into<String>) {
        self.lines.push(s.into());
    }
    fn mark(&mut self, pos: DVec3, text: &str, color: [u8; 3]) {
        if self.markers.len() < 400 {
            self.markers.push(Marker { pos, text: text.into(), color });
        }
    }
}

// ------------------------------------------------------------------ Kollision

/// Räumlicher Index konvexer Brushes für schnelle Box-/Strahltests
pub struct Collider {
    items: Vec<(Aabb, Vec<Option<Plane>>)>,
    grid: HashMap<(i32, i32, i32), Vec<u32>>,
    cell: f64,
}

impl Collider {
    pub fn new<'a>(solids: impl Iterator<Item = &'a Solid>) -> Collider {
        let cell = 256.0;
        let mut c = Collider { items: Vec::new(), grid: HashMap::new(), cell };
        for s in solids {
            if s.bounds.is_empty() {
                continue;
            }
            let i = c.items.len() as u32;
            c.items.push((s.bounds, s.planes.clone()));
            let (a, b) = (c.key(s.bounds.min), c.key(s.bounds.max));
            for x in a.0..=b.0 {
                for y in a.1..=b.1 {
                    for z in a.2..=b.2 {
                        c.grid.entry((x, y, z)).or_default().push(i);
                    }
                }
            }
        }
        c
    }

    fn key(&self, p: DVec3) -> (i32, i32, i32) {
        ((p.x / self.cell).floor() as i32, (p.y / self.cell).floor() as i32, (p.z / self.cell).floor() as i32)
    }

    fn candidates(&self, b: &Aabb) -> Vec<u32> {
        let (a, c) = (self.key(b.min), self.key(b.max));
        let mut v = Vec::new();
        for x in a.0..=c.0 {
            for y in a.1..=c.1 {
                for z in a.2..=c.2 {
                    if let Some(l) = self.grid.get(&(x, y, z)) {
                        v.extend_from_slice(l);
                    }
                }
            }
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Überschneidet die Box irgendeinen Brush?
    pub fn box_blocked(&self, b: &Aabb) -> bool {
        let corners = b.corners();
        self.candidates(b).into_iter().any(|i| {
            let (bb, planes) = &self.items[i as usize];
            bb.intersects(b) && planes.iter().flatten().all(|p| corners.iter().any(|c| p.distance(*c) < -0.01))
        })
    }

    pub fn point_inside(&self, p: DVec3) -> bool {
        self.box_blocked(&Aabb::new(p, p))
    }

    /// Erster Treffer eines Strahls bis maxt
    pub fn ray(&self, o: DVec3, d: DVec3, maxt: f64) -> Option<f64> {
        let b = Aabb::new(o, o + d * maxt);
        self.candidates(&b)
            .into_iter()
            .filter_map(|i| {
                let (bb, planes) = &self.items[i as usize];
                bb.ray_hit(o, d)?;
                ray_convex(planes, o, d).map(|h| h.0).filter(|t| *t <= maxt)
            })
            .fold(None, |a: Option<f64>, t| Some(a.map_or(t, |x| x.min(t))))
    }
}

fn all_tool(s: &Solid, names: &[&str]) -> bool {
    s.sides.iter().all(|x| {
        let m = x.material.to_ascii_lowercase();
        names.iter().any(|n| m.contains(n))
    })
}

/// Brushes, die NPCs und Spieler blockieren
fn collision_solids(ed: &Editor) -> Vec<&Solid> {
    let passable = ["toolstrigger", "toolshint", "toolsskip", "toolsareaportal", "toolsplayerclip", "toolsinvisibleladder", "toolsorigin"];
    let mut v: Vec<&Solid> = ed.map.world.solids.iter().filter(|s| !all_tool(s, &passable)).map(|s| &**s).collect();
    for e in &ed.map.entities {
        let c = e.classname();
        if c.starts_with("trigger_") || c.starts_with("func_door") || matches!(c, "func_areaportal" | "func_illusionary" | "func_ladder" | "func_water_analog" | "func_button") {
            continue;
        }
        v.extend(e.solids.iter().filter(|s| !all_tool(s, &passable)).map(|s| &**s));
    }
    v
}

/// Brushes, die die Map gegen das Nichts abdichten (wie VBSP: nur Welt, ohne Wasser/Displacements/Clip)
fn sealing_solids(ed: &Editor) -> Vec<&Solid> {
    let nonsealing = ["toolsclip", "toolsplayerclip", "toolsnpcclip", "toolshint", "toolsskip", "toolsareaportal", "toolstrigger", "toolsinvisible", "toolsorigin"];
    ed.map
        .world
        .solids
        .iter()
        .filter(|s| !s.has_disp() && !all_tool(s, &nonsealing) && !s.sides.iter().any(|x| x.material.to_ascii_lowercase().contains("water")))
        .map(|s| &**s)
        .collect()
}

// ------------------------------------------------------------------ Flutfüllung (Lecks)

pub struct Flood {
    origin: DVec3,
    cell: f64,
    dims: [usize; 3],
    /// 0 = leer/außen, 1 = fest, 2 = innen (erreichbar)
    state: Vec<u8>,
    pub leak: Option<Vec<DVec3>>,
    pub leak_from: Option<String>,
}

impl Flood {
    fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    fn cell_of(&self, p: DVec3) -> Option<(usize, usize, usize)> {
        let r = (p - self.origin) / self.cell;
        if r.min_element() < 0.0 {
            return None;
        }
        let (x, y, z) = (r.x as usize, r.y as usize, r.z as usize);
        (x < self.dims[0] && y < self.dims[1] && z < self.dims[2]).then_some((x, y, z))
    }

    fn center(&self, x: usize, y: usize, z: usize) -> DVec3 {
        self.origin + DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5) * self.cell
    }

    /// Liegt der Punkt im erreichbaren Inneren der Map?
    pub fn inside(&self, p: DVec3) -> bool {
        self.cell_of(p).is_some_and(|(x, y, z)| self.state[self.idx(x, y, z)] == 2)
    }

    /// Liegt der Punkt im Nichts (weder fest noch erreichbar)?
    pub fn void(&self, p: DVec3) -> bool {
        match self.cell_of(p) {
            None => true,
            Some((x, y, z)) => self.state[self.idx(x, y, z)] == 0,
        }
    }

    pub fn run(ed: &Editor) -> Option<Flood> {
        let seal = sealing_solids(ed);
        let bounds = seal.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
        if bounds.is_empty() {
            return None;
        }
        let mut cell = 16.0;
        let size = bounds.size();
        while ((size.x / cell + 4.0) * (size.y / cell + 4.0) * (size.z / cell + 4.0)) > 6.0e6 {
            cell *= 2.0;
        }
        let origin = bounds.min - DVec3::splat(cell * 2.0);
        let dims = [((size.x / cell) as usize) + 5, ((size.y / cell) as usize) + 5, ((size.z / cell) as usize) + 5];
        let mut f = Flood { origin, cell, dims, state: vec![0; dims[0] * dims[1] * dims[2]], leak: None, leak_from: None };
        // feste Zellen: Zelle überschneidet einen dichtenden Brush
        for s in &seal {
            let (a, b) = (((s.bounds.min - origin) / cell).floor(), ((s.bounds.max - origin) / cell).floor());
            for z in (a.z.max(0.0) as usize)..=(b.z as usize).min(dims[2] - 1) {
                for y in (a.y.max(0.0) as usize)..=(b.y as usize).min(dims[1] - 1) {
                    for x in (a.x.max(0.0) as usize)..=(b.x as usize).min(dims[0] - 1) {
                        let i = f.idx(x, y, z);
                        if f.state[i] == 1 {
                            continue;
                        }
                        let c0 = origin + DVec3::new(x as f64, y as f64, z as f64) * cell;
                        let cb = Aabb::new(c0 + DVec3::splat(0.5), c0 + DVec3::splat(cell - 0.5));
                        let corners = cb.corners();
                        if s.planes.iter().flatten().all(|p| corners.iter().any(|c| p.distance(*c) < 0.0)) {
                            f.state[i] = 1;
                        }
                    }
                }
            }
        }
        // Flutung von allen Punkt-Entities
        let mut parent: Vec<u32> = vec![u32::MAX; f.state.len()];
        let mut q = VecDeque::new();
        let mut seeds: Vec<(usize, String)> = Vec::new();
        for e in &ed.map.entities {
            if e.is_brush_entity() {
                continue;
            }
            if let Some(o) = e.origin() {
                if let Some((x, y, z)) = f.cell_of(o + DVec3::Z * 4.0) {
                    let i = f.idx(x, y, z);
                    if f.state[i] == 0 {
                        f.state[i] = 2;
                        parent[i] = i as u32;
                        q.push_back(i);
                        seeds.push((i, format!("{} ({})", e.classname(), e.targetname().unwrap_or("ohne Namen"))));
                    }
                }
            }
        }
        let [dx, dy, dz] = dims;
        let mut leak_cell = None;
        while let Some(i) = q.pop_front() {
            let x = i % dx;
            let y = (i / dx) % dy;
            let z = i / (dx * dy);
            if x == 0 || y == 0 || z == 0 || x == dx - 1 || y == dy - 1 || z == dz - 1 {
                leak_cell = Some(i);
                break;
            }
            for n in [i - 1, i + 1, i - dx, i + dx, i - dx * dy, i + dx * dy] {
                if f.state[n] == 0 {
                    f.state[n] = 2;
                    parent[n] = i as u32;
                    q.push_back(n);
                }
            }
        }
        if let Some(mut i) = leak_cell {
            let mut path = Vec::new();
            loop {
                let (x, y, z) = (i % dx, (i / dx) % dy, i / (dx * dy));
                path.push(f.center(x, y, z));
                let p = parent[i] as usize;
                if p == i {
                    f.leak_from = seeds.iter().find(|s| s.0 == i).map(|s| s.1.clone());
                    break;
                }
                i = p;
            }
            path.reverse();
            // Pfad vereinfachen (nur Richtungswechsel behalten)
            let mut simple: Vec<DVec3> = Vec::new();
            for p in path {
                if simple.len() >= 2 {
                    let a = simple[simple.len() - 2];
                    let b = simple[simple.len() - 1];
                    if (b - a).normalize_or_zero().dot((p - b).normalize_or_zero()) > 0.999 {
                        simple.pop();
                    }
                }
                simple.push(p);
            }
            f.leak = Some(simple);
        }
        Some(f)
    }
}

// ------------------------------------------------------------------ Begehbare Flächen

#[derive(Clone, Copy)]
struct Sample {
    pos: DVec3,
    open: bool,
}

fn hull_free(col: &Collider, p: DVec3, half: f64) -> bool {
    !col.box_blocked(&Aabb::new(p + DVec3::new(-half, -half, 2.0), p + DVec3::new(half, half, 72.0)))
}

fn point_in_poly_2d(p: (f64, f64), poly: &[DVec3]) -> bool {
    let mut sign = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let c = (b.x - a.x) * (p.1 - a.y) - (b.y - a.y) * (p.0 - a.x);
        if c.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Abtastpunkte auf begehbaren Flächen (Raster `step`), mit Platz für einen NPC
fn walkable_samples(ed: &Editor, col: &Collider, flood: Option<&Flood>, step: f64) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let up_world = Collider::new(ed.map.world.solids.iter().map(|s| &**s));
    for s in collision_solids(ed) {
        let mut tris: Vec<[DVec3; 3]> = Vec::new();
        for (i, poly) in s.polys.iter().enumerate() {
            let m = s.sides[i].material.to_ascii_lowercase();
            if m.contains("toolsskybox") || m.contains("toolsplayerclip") || m.contains("toolsnpcclip") {
                continue;
            }
            if let Some(Some(g)) = s.disps.get(i) {
                for t in g.triangles() {
                    tris.push([g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]]);
                }
            } else if s.has_disp() {
                continue;
            } else if poly.len() >= 3 {
                for k in 1..poly.len() - 1 {
                    tris.push([poly[0], poly[k], poly[k + 1]]);
                }
            }
        }
        for t in tris {
            let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            let n = if n.z < 0.0 { -n } else { n };
            if n.z < 0.7 {
                continue;
            }
            let b = Aabb::from_points(t);
            let (x0, x1) = ((b.min.x / step).ceil() as i64, (b.max.x / step).floor() as i64);
            let (y0, y1) = ((b.min.y / step).ceil() as i64, (b.max.y / step).floor() as i64);
            for xi in x0..=x1 {
                for yi in y0..=y1 {
                    let (x, y) = (xi as f64 * step, yi as f64 * step);
                    if !point_in_poly_2d((x, y), &t) {
                        continue;
                    }
                    let z = t[0].z - (n.x * (x - t[0].x) + n.y * (y - t[0].y)) / n.z;
                    let key = (xi, yi, (z / 16.0).round() as i64);
                    if !seen.insert(key) {
                        continue;
                    }
                    let p = DVec3::new(x, y, z);
                    let inside = match flood.filter(|f| f.leak.is_none()) {
                        Some(f) => f.inside(p + DVec3::Z * 36.0),
                        None => up_world.ray(p + DVec3::Z * 8.0, DVec3::Z, 16384.0).is_some(),
                    };
                    if !inside || !hull_free(col, p, 14.0) {
                        continue;
                    }
                    let open = hull_free(col, p, 96.0);
                    out.push(Sample { pos: p, open });
                }
            }
        }
    }
    out
}

/// Freie Breite an einem Punkt entlang einer horizontalen Achse
fn free_extent(col: &Collider, p: DVec3, dir: DVec3, max: f64) -> f64 {
    let mut d = 0.0;
    while d < max {
        let q = p + dir * (d + 8.0);
        if col.box_blocked(&Aabb::new(q + DVec3::new(-2.0, -2.0, 4.0), q + DVec3::new(2.0, 2.0, 64.0))) {
            return d;
        }
        d += 8.0;
    }
    max
}

/// Durchgänge (Türen, Engstellen): (Mitte, Durchgangsrichtung)
fn find_doorways(col: &Collider, samples: &[Sample]) -> Vec<(DVec3, DVec3)> {
    let mut found: Vec<(DVec3, DVec3)> = Vec::new();
    for s in samples.iter().filter(|s| !s.open) {
        for (across, along) in [(DVec3::X, DVec3::Y), (DVec3::Y, DVec3::X)] {
            let w = free_extent(col, s.pos, across, 160.0) + free_extent(col, s.pos, -across, 160.0);
            if w > 112.0 || w < 24.0 {
                continue;
            }
            let wide = |q: DVec3| hull_free(col, q, 14.0) && free_extent(col, q, across, 160.0) + free_extent(col, q, -across, 160.0) >= 160.0;
            if wide(s.pos + along * 96.0) && wide(s.pos - along * 96.0) {
                // auf die Mitte der Öffnung zentrieren
                let r = free_extent(col, s.pos, across, 160.0);
                let l = free_extent(col, s.pos, -across, 160.0);
                let c = s.pos + across * ((r - l) / 2.0);
                if !found.iter().any(|f| f.0.distance(c) < 64.0) {
                    found.push((c, along));
                }
            }
        }
    }
    found
}

// ------------------------------------------------------------------ Hauptlauf

pub fn run(ed: &mut Editor, o: &OptSettings, fs: Option<&GameFs>, _packlist: Option<&Path>) -> Report {
    let mut r = Report::default();
    let before = (ed.map.all_solids().count(), ed.map.entities.len());

    // --- Geometrie aufräumen
    if o.remove_slivers {
        let slivers: Vec<u32> = ed.map.all_solids().filter(|(_, s)| !s.is_valid() || s.bounds.size().min_element() < 1.0).map(|(_, s)| s.id).collect();
        for id in &slivers {
            ed.map.take_solid(*id);
        }
        // exakte Duplikate
        let mut dup = Vec::new();
        let w = &ed.map.world.solids;
        for i in 0..w.len() {
            for j in (i + 1)..w.len() {
                if w[i].bounds == w[j].bounds && w[i].sides.len() == w[j].sides.len() && w[i].vertices().iter().all(|v| w[j].vertices().iter().any(|q| q.distance(*v) < 0.01)) {
                    dup.push(w[j].id);
                }
            }
        }
        dup.dedup();
        for id in &dup {
            ed.map.take_solid(*id);
        }
        if !slivers.is_empty() || !dup.is_empty() {
            r.say(format!("Aufräumen: {} Splitter/ungültige und {} doppelte Brushes entfernt", slivers.len(), dup.len()));
            r.changes += slivers.len() + dup.len();
        }
    }
    if o.snap_verts {
        let mut n = 0;
        let ids: Vec<u32> = ed.map.all_solids().filter(|(_, s)| !s.has_disp()).map(|(_, s)| s.id).collect();
        for id in ids {
            let Some(s) = ed.map.solid(id).cloned() else { continue };
            let v = s.vertices();
            if !v.iter().any(|p| (*p - p.round()).abs().max_element() > 0.001) {
                continue;
            }
            let snapped: Vec<DVec3> = v.iter().map(|p| p.round()).collect();
            if let Some(ns) = crate::csg::rebuild_from_vertices(&mut ed.map, &s, &snapped) {
                if ns.sides.len() == s.sides.len() {
                    *ed.map.solid_mut(id).unwrap() = ns;
                    n += 1;
                }
            }
        }
        if n > 0 {
            r.say(format!("Eckpunkte: {n} Brush(es) auf ganze Einheiten gerundet (verhindert Mini-Lecks)"));
            r.changes += n;
        }
    }
    if o.simplify_disps {
        let mut n = 0;
        let flat = |d: &crate::disp::Disp| d.distances.iter().all(|x| x.abs() < 0.01) && d.offsets.iter().all(|x| x.length() < 0.01) && d.elevation.abs() < 0.01 && d.alphas.iter().all(|a| *a < 0.5);
        // nur betroffene Brushes ändern (andere bleiben mit dem Rückgängig-Stand geteilt)
        ed.map.solids_mut_where(
            |s| s.sides.iter().any(|sd| sd.disp.as_ref().is_some_and(flat)),
            |s| {
                for side in &mut s.sides {
                    if side.disp.as_ref().is_some_and(flat) {
                        side.disp = None;
                        n += 1;
                    }
                }
                s.update_geometry();
            },
        );
        if n > 0 {
            r.say(format!("Displacements: {n} flache Displacement(s) in normale Flächen zurückverwandelt"));
            r.changes += n;
        }
    }
    if o.merge_brushes {
        let n = merge_boxes(ed);
        if n > 0 {
            r.say(format!("Brushes zusammengeführt: {n} Paar(e) gleicher Quader"));
            r.changes += n;
        }
    }
    if o.remove_unused_logic {
        let logic = ["logic_relay", "logic_timer", "logic_branch", "logic_compare", "logic_case", "math_counter", "logic_auto", "logic_multicompare"];
        let solid_class = |e: &Entity| ed.fgd.class(e.classname()).is_some_and(|c| c.kind == crate::fgd::ClassKind::Solid);
        let unused: Vec<u32> = ed.map.entities.iter().filter(|e| (logic.contains(&e.classname()) && e.outputs.is_empty()) || (e.solids.is_empty() && solid_class(e))).map(|e| e.id).collect();
        // doppelte Cubemaps
        let cubes: Vec<(u32, DVec3)> = ed.map.entities.iter().filter(|e| e.classname() == "env_cubemap").filter_map(|e| e.origin().map(|o| (e.id, o))).collect();
        let mut dup = Vec::new();
        for i in 0..cubes.len() {
            if cubes[..i].iter().any(|c| c.1.distance(cubes[i].1) < 32.0) {
                dup.push(cubes[i].0);
            }
        }
        let all: Vec<u32> = unused.into_iter().chain(dup).collect();
        if !all.is_empty() {
            ed.map.entities.retain(|e| !all.contains(&e.id));
            r.say(format!("Entities: {} wirkungslose Logik/leere Brush-Entities/doppelte Cubemaps entfernt", all.len()));
            r.changes += all.len();
        }
    }

    // --- Leckprüfung (auch Grundlage für func_detail und Innenraum-Erkennung)
    let flood = if o.leak_check || o.auto_detail || o.ai_nodes || o.cubemaps || o.ai_air_nodes { Flood::run(ed) } else { None };
    if o.leak_check {
        match &flood {
            Some(f) if f.leak.is_some() => {
                r.say(format!("LECK gefunden – ausgehend von {}. Der rote Pfad führt ins Nichts.", f.leak_from.clone().unwrap_or_else(|| "einer Entity".into())));
                r.leak = f.leak.clone();
            }
            Some(_) => r.say("Leckprüfung: Map ist dicht."),
            None => r.say("Leckprüfung: keine Welt-Brushes."),
        }
    }
    let sealed = flood.as_ref().filter(|f| f.leak.is_none());

    // --- func_detail
    if o.auto_detail {
        match sealed {
            Some(f) => {
                let n = auto_detail(ed, f);
                if n > 0 {
                    r.say(format!("func_detail: {n} Brush(es), die nicht abdichten, umgewandelt (VVIS wird schneller)"));
                    r.changes += n;
                }
            }
            None => r.say("func_detail übersprungen (Map nicht dicht)"),
        }
    }
    if o.nodraw {
        let prev = std::mem::take(&mut ed.status);
        let sel = std::mem::take(&mut ed.sel);
        let n = nodraw_quiet(ed, fs);
        ed.sel = sel;
        ed.status = prev;
        if n > 0 {
            r.say(format!("nodraw: {n} vollständig verdeckte Fläche(n)"));
            r.changes += n;
        }
    }

    // --- KI und Sichtbarkeit: begehbare Flächen
    let col = Collider::new(collision_solids(ed).into_iter());
    let need_samples = o.ai_nodes || o.cubemaps || o.light_check || o.hint_suggestions || o.ai_hints;
    let fine = (o.node_spacing / 2.0).max(16.0);
    let samples = if need_samples { walkable_samples(ed, &col, flood.as_ref(), fine) } else { Vec::new() };
    let doorways = if o.ai_nodes || o.hint_suggestions || o.ai_hints { find_doorways(&col, &samples) } else { Vec::new() };
    if o.hint_suggestions {
        for (c, along) in &doorways {
            r.mark(*c + DVec3::Z * 64.0, &format!("Hint-Brush hier sinnvoll (Ebene quer zu {})", if along.x.abs() > 0.5 { "X" } else { "Y" }), [255, 200, 60]);
        }
        if !doorways.is_empty() {
            r.say(format!("Hint-Vorschläge: {} Durchgang/Durchgänge markiert (gelb)", doorways.len()));
        }
    }

    let mut nodes: Vec<DVec3> = ed.map.entities.iter().filter(|e| e.classname() == "info_node").filter_map(|e| e.origin()).collect();
    if o.ai_nodes {
        let spacing = o.node_spacing.max(32.0);
        let on = |v: f64, s: f64| ((v / s).round() * s - v).abs() < 0.5;
        let mut new_nodes: Vec<DVec3> = Vec::new();
        let add = |p: DVec3, nodes: &mut Vec<DVec3>, new_nodes: &mut Vec<DVec3>, min: f64| {
            if nodes.len() < 4000 && !nodes.iter().any(|q| q.distance(p) < min) {
                nodes.push(p);
                new_nodes.push(p);
            }
        };
        for (c, along) in &doorways {
            for off in [0.0, 64.0, -64.0] {
                let p = *c + *along * off;
                if hull_free(&col, p, 14.0) {
                    add(p, &mut nodes, &mut new_nodes, 32.0);
                }
            }
        }
        // an Türen beidseitig
        for e in ed.map.entities.iter().filter(|e| e.classname().starts_with("func_door")) {
            let b = e.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
            if b.is_empty() {
                continue;
            }
            let s = b.size();
            let thin = if s.x < s.y { DVec3::X } else { DVec3::Y };
            for side in [1.0, -1.0] {
                let p = DVec3::new(b.center().x, b.center().y, b.min.z) + thin * side * (64.0 + (s.x.min(s.y)) / 2.0);
                if hull_free(&col, p, 14.0) {
                    add(p, &mut nodes, &mut new_nodes, 32.0);
                }
            }
        }
        // erst Rasterpunkte (gleichmäßiges Bild), dann Lücken auffüllen; im Freien weiter auseinander
        let mut ordered: Vec<&Sample> = samples.iter().collect();
        ordered.sort_by_key(|s| (!(on(s.pos.x, spacing) && on(s.pos.y, spacing)), (s.pos.y as i64), (s.pos.x as i64)));
        for s in ordered {
            let min = if s.open { spacing * 1.25 } else { spacing * 0.9 };
            add(s.pos, &mut nodes, &mut new_nodes, min);
        }
        for p in &new_nodes {
            let id = ed.map.alloc_id();
            let mut e = Entity::new(id, "info_node");
            e.set("origin", fmt_vec(*p + DVec3::Z * 8.0));
            e.set("nodeid", "0");
            ed.map.entities.push(e.into());
        }
        r.say(format!("KI-Nodes: {} Bodennode(s) gesetzt (Abstand {spacing}, dichter an Engstellen)", new_nodes.len()));
        r.changes += new_nodes.len();
    }
    if o.ai_nodes || !nodes.is_empty() {
        connectivity(&col, &nodes, o.node_spacing.max(32.0) * 2.3, &mut r);
    }
    if o.ai_air_nodes {
        let n = air_nodes(ed, &col, flood.as_ref());
        r.say(format!("Luftnodes: {n} info_node_air gesetzt"));
        r.changes += n;
    }
    if o.ai_door_links {
        let n = door_links(ed);
        if n > 0 {
            r.say(format!("Türen: {n} info_node_link_controller (NPC-Wege nur bei offener Tür)"));
            r.changes += n;
        }
    }
    if o.ai_hints {
        let n = cover_hints(ed, &col, &nodes, &doorways);
        r.say(format!("KI-Hinweise: {n} info_hint (Deckung, Engstellen)"));
        r.changes += n;
    }
    if o.areaportals {
        let (n, skipped) = areaportals(ed);
        if n + skipped > 0 {
            r.say(format!("Areaportals: {n} an Türen gesetzt{}", if skipped > 0 { format!(", {skipped} übersprungen (Öffnung nicht eindeutig)") } else { String::new() }));
            r.changes += n;
        }
    }

    // --- Licht
    if o.cubemaps {
        let mut cubes: Vec<DVec3> = ed.map.entities.iter().filter(|e| e.classname() == "env_cubemap").filter_map(|e| e.origin()).collect();
        let mut n = 0;
        for s in &samples {
            if cubes.len() >= 120 {
                break;
            }
            let p = s.pos + DVec3::Z * 64.0;
            if s.open && !cubes.iter().any(|c| c.distance(p) < 768.0) {
                cubes.push(p);
                let id = ed.map.alloc_id();
                let mut e = Entity::new(id, "env_cubemap");
                e.set("origin", fmt_vec(p));
                ed.map.entities.push(e.into());
                n += 1;
            }
        }
        if n > 0 {
            r.say(format!("Cubemaps: {n} env_cubemap gesetzt – nach dem Kompilieren „Cubemaps bauen“ ausführen"));
            r.changes += n;
        }
    }
    if o.lightmaps {
        let (fine_n, coarse_n) = lightmaps(ed);
        if fine_n + coarse_n > 0 {
            r.say(format!("Lightmaps: {coarse_n} Fläche(n) gröber (32), {fine_n} feiner (8) – weniger Compile-Zeit, schärferes Licht wo nötig"));
            r.changes += fine_n + coarse_n;
        }
    }
    if o.light_check {
        light_check(ed, &samples, &mut r);
    }

    // --- Props
    if o.prop_fades || o.prop_shadows {
        let (f, s) = props(ed, o.prop_fades, o.prop_shadows);
        if f + s > 0 {
            r.say(format!("Props: {f} mit Ausblend-Distanz, {s} kleine ohne Schatten"));
            r.changes += f + s;
        }
    }
    // Die Packliste schreibt assets::write_packlist (mit der Auswahl des Nutzers)
    let after = (ed.map.all_solids().count(), ed.map.entities.len());
    r.say(format!("Ergebnis: {} -> {} Brushes, {} -> {} Entities, {} Änderung(en)", before.0, after.0, before.1, after.1, r.changes));
    ed.touch();
    r
}

fn nodraw_quiet(ed: &mut Editor, fs: Option<&GameFs>) -> usize {
    let n = ed.nodraw_covered_faces(false, fs);
    // nodraw_covered_faces legt einen Undo-Punkt an; beim Optimieren ist das gewünscht nur im Editor
    n
}

/// Brushes ohne Kontakt zum Nichts werden func_detail
fn auto_detail(ed: &mut Editor, f: &Flood) -> usize {
    let mut ids = Vec::new();
    // Brushes um Türen bleiben Welt (Areaportals müssen von Welt-Brushes eingefasst sein)
    let doors: Vec<Aabb> = ed
        .map
        .entities
        .iter()
        .filter(|e| e.classname().starts_with("func_door") || e.classname() == "func_areaportal")
        .map(|e| e.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds)))
        .filter(|b| !b.is_empty())
        .map(|b| Aabb::new(b.min - DVec3::splat(16.0), b.max + DVec3::splat(16.0)))
        .collect();
    for s in &ed.map.world.solids {
        if s.has_disp() || s.sides.iter().any(|x| is_tool_material(&x.material) || x.material.to_ascii_lowercase().contains("water")) {
            continue;
        }
        let size = s.bounds.size();
        let axis_aligned = s.planes.iter().flatten().all(|p| p.normal.abs().max_element() > 0.999);
        let small = size.max_element() <= 256.0 || size.x * size.y * size.z <= 128.0 * 128.0 * 128.0;
        // Trennwände zwischen Räumen sind für VVIS wichtig
        let wall_like = axis_aligned && size.x.max(size.y) >= 128.0 && size.z >= 96.0;
        if (!small && axis_aligned) || wall_like || doors.iter().any(|d| d.intersects(&s.bounds)) {
            continue;
        }
        // Punkte knapp außerhalb jeder Fläche dürfen nicht im Nichts liegen
        let touches_void = s.polys.iter().zip(&s.planes).any(|(poly, pl)| {
            let Some(pl) = pl else { return false };
            if poly.is_empty() {
                return false;
            }
            let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
            std::iter::once(c).chain(poly.iter().map(|v| *v + (c - *v) * 0.1)).any(|p| f.void(p + pl.normal * (f.cell * 0.75)))
        });
        if !touches_void {
            ids.push(s.id);
        }
    }
    if ids.is_empty() {
        return 0;
    }
    let id = ed.map.alloc_id();
    let mut e = Entity::new(id, "func_detail");
    e.editor.color = [0, 180, 255];
    for sid in &ids {
        if let Some(s) = ed.map.take_solid(*sid) {
            e.solids.push(s.into());
        }
    }
    ed.map.entities.push(e.into());
    ids.len()
}

/// Gleich texturierte, achsenparallele Quader mit gemeinsamer Fläche zusammenfassen
fn merge_boxes(ed: &mut Editor) -> usize {
    let is_box = |s: &Solid| -> Option<String> {
        if s.has_disp() || s.sides.len() != 6 || !s.planes.iter().flatten().all(|p| p.normal.abs().max_element() > 0.9999) {
            return None;
        }
        let m = s.sides[0].material.clone();
        let world_aligned = s.sides.iter().zip(&s.planes).all(|(side, p)| {
            let Some(p) = p else { return false };
            let (u, v) = default_tex_axes(p.normal, side.uaxis.scale);
            side.material == m && side.uaxis.axis == u.axis && side.vaxis.axis == v.axis && side.uaxis.shift == 0.0 && side.vaxis.shift == 0.0 && side.uaxis.scale == side.vaxis.scale
        });
        (world_aligned && s.editor.groupid.is_none()).then_some(m)
    };
    let mut merged = 0;
    loop {
        let mut found = None;
        let w = &ed.map.world.solids;
        'outer: for i in 0..w.len() {
            let Some(mi) = is_box(&w[i]) else { continue };
            for j in (i + 1)..w.len() {
                let Some(mj) = is_box(&w[j]) else { continue };
                if mi != mj || w[i].sides[0].uaxis.scale != w[j].sides[0].uaxis.scale || w[i].editor.visgroups != w[j].editor.visgroups {
                    continue;
                }
                let (a, b) = (w[i].bounds, w[j].bounds);
                for k in 0..3 {
                    let others = [(k + 1) % 3, (k + 2) % 3];
                    let same = others.iter().all(|o| (a.min[*o] - b.min[*o]).abs() < 0.01 && (a.max[*o] - b.max[*o]).abs() < 0.01);
                    let touch = (a.max[k] - b.min[k]).abs() < 0.01 || (b.max[k] - a.min[k]).abs() < 0.01;
                    if same && touch {
                        found = Some((w[i].id, w[j].id, a.union(&b), mi.clone(), w[i].sides[0].uaxis.scale));
                        break 'outer;
                    }
                }
            }
        }
        let Some((a, b, bounds, mat, scale)) = found else { break };
        ed.map.take_solid(a);
        ed.map.take_solid(b);
        let s = ed.map.make_box(bounds, &mat, scale);
        ed.map.world.solids.push(s.into());
        merged += 1;
    }
    merged
}

/// Verbindungen zwischen Nodes simulieren und isolierte Bereiche melden
fn connectivity(col: &Collider, nodes: &[DVec3], max_dist: f64, r: &mut Report) {
    if nodes.is_empty() {
        return;
    }
    let cell = max_dist;
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in nodes.iter().enumerate() {
        grid.entry(((p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64)).or_default().push(i);
    }
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        let mut i = i;
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut degree = vec![0usize; nodes.len()];
    for (i, a) in nodes.iter().enumerate() {
        let k = ((a.x / cell).floor() as i64, (a.y / cell).floor() as i64, (a.z / cell).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(list) = grid.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) else { continue };
                    for &j in list {
                        if j <= i {
                            continue;
                        }
                        let b = nodes[j];
                        let d = a.distance(b);
                        if d > max_dist || (a.z - b.z).abs() > d * 0.8 + 24.0 {
                            continue;
                        }
                        if walkable_link(col, *a, b) {
                            degree[i] += 1;
                            degree[j] += 1;
                            let (ra, rb) = (find(&mut parent, i), find(&mut parent, j));
                            parent[ra] = rb;
                            if r.links.len() < 20000 {
                                r.links.push((*a, b, true));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut comps: HashMap<usize, usize> = HashMap::new();
    for i in 0..nodes.len() {
        let root = find(&mut parent, i);
        *comps.entry(root).or_default() += 1;
    }
    let isolated: Vec<usize> = (0..nodes.len()).filter(|i| degree[*i] == 0).collect();
    for i in &isolated {
        r.mark(nodes[*i] + DVec3::Z * 16.0, "Node ohne Verbindung", [255, 60, 60]);
    }
    r.say(format!("KI-Netz: {} Nodes, {} Verbindungen, {} zusammenhängende Bereiche, {} isolierte Nodes (rot)", nodes.len(), r.links.len(), comps.len(), isolated.len()));
    if comps.len() > 1 {
        // kleine Inseln markieren
        let biggest = comps.values().copied().max().unwrap_or(0);
        let mut small = 0;
        for i in 0..nodes.len() {
            let root = find(&mut parent, i);
            if comps[&root] < biggest && comps[&root] > 1 && small < 30 {
                r.mark(nodes[i] + DVec3::Z * 16.0, "Getrennter Node-Bereich", [255, 140, 40]);
                small += 1;
            }
        }
    }
}

/// Kann ein NPC von a nach b laufen? (Sichtlinie in Hüft- und Kopfhöhe, Boden durchgehend, Stufen ≤ 18)
fn walkable_link(col: &Collider, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 1.0 {
        return true;
    }
    let dir = d / len;
    for h in [20.0, 60.0] {
        if col.ray(a + DVec3::Z * h, dir, len).is_some() {
            return false;
        }
    }
    let steps = (len / 24.0).ceil() as usize;
    let mut last_z = a.z;
    for k in 1..steps {
        let p = a + dir * (len * k as f64 / steps as f64);
        let top = DVec3::new(p.x, p.y, p.z.max(last_z) + 40.0);
        let Some(t) = col.ray(top, -DVec3::Z, 120.0) else { return false };
        let gz = top.z - t;
        if (gz - last_z).abs() > 20.0 {
            return false;
        }
        last_z = gz;
    }
    (b.z - last_z).abs() <= 20.0
}

fn air_nodes(ed: &mut Editor, col: &Collider, flood: Option<&Flood>) -> usize {
    let b = ed.map.world.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
    if b.is_empty() {
        return 0;
    }
    let step = 256.0;
    let existing: Vec<DVec3> = ed.map.entities.iter().filter(|e| e.classname() == "info_node_air").filter_map(|e| e.origin()).collect();
    let mut placed = Vec::new();
    let mut z = (b.min.z / step).ceil() * step;
    while z <= b.max.z {
        let mut y = (b.min.y / step).ceil() * step;
        while y <= b.max.y {
            let mut x = (b.min.x / step).ceil() * step;
            while x <= b.max.x {
                let p = DVec3::new(x, y, z);
                let inside = flood.filter(|f| f.leak.is_none()).map_or_else(|| col.ray(p, DVec3::Z, 8192.0).is_some() && col.ray(p, -DVec3::Z, 8192.0).is_some(), |f| f.inside(p));
                let ground = col.ray(p, -DVec3::Z, 8192.0).unwrap_or(0.0);
                if inside && ground >= 96.0 && !col.box_blocked(&Aabb::new(p - DVec3::splat(32.0), p + DVec3::splat(32.0))) && !existing.iter().any(|q| q.distance(p) < 128.0) && placed.len() < 512 {
                    placed.push(p);
                }
                x += step;
            }
            y += step;
        }
        z += step;
    }
    for p in &placed {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "info_node_air");
        e.set("origin", fmt_vec(*p));
        e.set("nodeheight", "0");
        ed.map.entities.push(e.into());
    }
    placed.len()
}

fn door_links(ed: &mut Editor) -> usize {
    let doors: Vec<u32> = ed.map.entities.iter().filter(|e| e.classname().starts_with("func_door")).map(|e| e.id).collect();
    let mut n = 0;
    for (k, id) in doors.into_iter().enumerate() {
        let Some(door) = ed.map.entity(id) else { continue };
        let b = door.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
        if b.is_empty() || ed.map.entities.iter().any(|e| e.classname() == "info_node_link_controller" && e.origin().is_some_and(|o| b.contains_point(o))) {
            continue;
        }
        let name = match door.targetname() {
            Some(n) => n.to_string(),
            None => {
                let n = format!("amboss_tuer_{}", k + 1);
                ed.map.entity_mut(id).unwrap().set("targetname", n.clone());
                n
            }
        };
        let ctrl = format!("{name}_nodes");
        let c = b.center();
        let half = b.size() * 0.5 + DVec3::new(16.0, 16.0, 0.0);
        let cid = ed.map.alloc_id();
        let mut e = Entity::new(cid, "info_node_link_controller");
        e.set("targetname", ctrl.clone());
        e.set("origin", fmt_vec(c));
        e.set("mins", fmt_vec(-half));
        e.set("maxs", fmt_vec(half));
        e.set("initialstate", "0");
        ed.map.entities.push(e.into());
        let d = ed.map.entity_mut(id).unwrap();
        d.outputs.push(("OnOpen".into(), format!("{ctrl},TurnOn,,0,-1")));
        d.outputs.push(("OnClose".into(), format!("{ctrl},TurnOff,,0,-1")));
        n += 1;
    }
    n
}

fn cover_hints(ed: &mut Editor, col: &Collider, nodes: &[DVec3], doorways: &[(DVec3, DVec3)]) -> usize {
    let mut placed: Vec<(DVec3, &str, f64)> = Vec::new();
    for (c, along) in doorways {
        let yaw = along.y.atan2(along.x).to_degrees();
        placed.push((*c, "103", yaw));
    }
    for p in nodes {
        if placed.len() >= 96 {
            break;
        }
        for k in 0..8 {
            let a = std::f64::consts::TAU * k as f64 / 8.0;
            let dir = DVec3::new(a.cos(), a.sin(), 0.0);
            let q = *p + dir * 40.0;
            let low_blocked = col.box_blocked(&Aabb::new(q + DVec3::new(-6.0, -6.0, 4.0), q + DVec3::new(6.0, 6.0, 30.0)));
            let mid_blocked = col.box_blocked(&Aabb::new(q + DVec3::new(-6.0, -6.0, 36.0), q + DVec3::new(6.0, 6.0, 48.0)));
            let high_free = !col.box_blocked(&Aabb::new(q + DVec3::new(-6.0, -6.0, 60.0), q + DVec3::new(6.0, 6.0, 80.0)));
            let kind = if low_blocked && !mid_blocked && high_free {
                Some("101")
            } else if low_blocked && mid_blocked && high_free {
                Some("100")
            } else {
                None
            };
            if let Some(kind) = kind {
                if !placed.iter().any(|x| x.0.distance(*p) < 96.0) {
                    placed.push((*p, kind, a.to_degrees()));
                }
                break;
            }
        }
    }
    for (p, kind, yaw) in &placed {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "info_hint");
        e.set("origin", fmt_vec(*p + DVec3::Z * 8.0));
        e.set("hinttype", *kind);
        e.set("angles", format!("0 {} 0", crate::map::fmt_num(yaw.rem_euclid(360.0))));
        ed.map.entities.push(e.into());
    }
    placed.len()
}

/// Areaportal in jede Tür-Öffnung, die vollständig von Welt-Brushes eingefasst ist
fn areaportals(ed: &mut Editor) -> (usize, usize) {
    let world = Collider::new(sealing_solids(ed).into_iter());
    let doors: Vec<u32> = ed.map.entities.iter().filter(|e| e.classname().starts_with("func_door")).map(|e| e.id).collect();
    let (mut n, mut skipped) = (0, 0);
    for (k, id) in doors.into_iter().enumerate() {
        let Some(door) = ed.map.entity(id) else { continue };
        let b = door.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
        if b.is_empty() {
            continue;
        }
        let name = door.targetname().map(|s| s.to_string());
        let size = b.size();
        let thin = if size.x < size.y { 0 } else { 1 };
        let other = 1 - thin;
        // Öffnung bis zur Welt erweitern
        let mut pb = b;
        let c = b.center();
        pb.min[thin] = c[thin] - 1.0;
        pb.max[thin] = c[thin] + 1.0;
        let mut ok = true;
        for (axis, sign) in [(other, -1.0), (other, 1.0), (2usize, -1.0), (2, 1.0)] {
            let mut grown = 0.0;
            loop {
                let mut probe = pb;
                if sign < 0.0 {
                    probe.max[axis] = pb.min[axis];
                    probe.min[axis] = pb.min[axis] - 1.0;
                } else {
                    probe.min[axis] = pb.max[axis];
                    probe.max[axis] = pb.max[axis] + 1.0;
                }
                let mid = probe.center();
                if world.point_inside(mid) {
                    break;
                }
                if grown >= 32.0 {
                    ok = false;
                    break;
                }
                if sign < 0.0 {
                    pb.min[axis] -= 1.0;
                } else {
                    pb.max[axis] += 1.0;
                }
                grown += 1.0;
            }
        }
        if !ok {
            skipped += 1;
            continue;
        }
        let name = name.unwrap_or_else(|| {
            let n = format!("amboss_tuer_{}", k + 1);
            ed.map.entity_mut(id).unwrap().set("targetname", n.clone());
            n
        });
        let s = ed.map.make_box(pb, "TOOLS/TOOLSAREAPORTAL", 0.25);
        let pid = ed.map.alloc_id();
        let mut e = Entity::new(pid, "func_areaportal");
        e.set("target", name);
        e.set("StartOpen", "0");
        e.solids.push(s.into());
        ed.map.entities.push(e.into());
        n += 1;
    }
    (n, skipped)
}

fn light_positions(ed: &Editor) -> (Vec<DVec3>, bool) {
    let lights: Vec<DVec3> = ed.map.entities.iter().filter(|e| matches!(e.classname(), "light" | "light_spot" | "light_dynamic")).filter_map(|e| e.origin()).collect();
    let sun = ed.map.entities.iter().any(|e| e.classname() == "light_environment");
    (lights, sun)
}

fn lightmaps(ed: &mut Editor) -> (usize, usize) {
    let (lights, _) = light_positions(ed);
    let (mut fine, mut coarse) = (0, 0);
    let mut apply = |s: &mut Solid| {
        for (i, side) in s.sides.iter_mut().enumerate() {
            if side.lightmapscale != 16 || side.disp.is_some() || is_tool_material(&side.material) {
                continue;
            }
            let Some(poly) = s.polys.get(i).filter(|p| p.len() >= 3) else { continue };
            let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
            let area: f64 = (1..poly.len() - 1).map(|k| (poly[k] - poly[0]).cross(poly[k + 1] - poly[0]).length() * 0.5).sum();
            let nearest = lights.iter().map(|l| l.distance(c)).fold(f64::MAX, f64::min);
            if nearest < 160.0 && area < 256.0 * 256.0 {
                side.lightmapscale = 8;
                fine += 1;
            } else if nearest > 1024.0 || (area > 1024.0 * 1024.0 && nearest > 384.0) {
                side.lightmapscale = 32;
                coarse += 1;
            }
        }
    };
    for s in ed.map.world.solids.iter_mut() {
        apply(s);
    }
    for e in ed.map.entities.iter_mut() {
        for s in e.solids.iter_mut() {
            apply(s);
        }
    }
    (fine, coarse)
}

fn light_check(ed: &Editor, samples: &[Sample], r: &mut Report) {
    let world = Collider::new(sealing_solids(ed).into_iter());
    let mut stuck = 0;
    for e in ed.map.entities.iter().filter(|e| e.classname().starts_with("light") && e.classname() != "light_environment") {
        if let Some(o) = e.origin() {
            if world.point_inside(o) {
                r.mark(o, "Licht steckt in einem Brush", [255, 60, 60]);
                stuck += 1;
            }
        }
    }
    let named = ed.map.entities.iter().filter(|e| e.classname().starts_with("light") && e.targetname().is_some()).count();
    let (lights, sun) = light_positions(ed);
    let mut dark: Vec<DVec3> = Vec::new();
    if !sun {
        for s in samples {
            if dark.len() >= 30 {
                break;
            }
            if !lights.iter().any(|l| l.distance(s.pos) < 1024.0) && !dark.iter().any(|d| d.distance(s.pos) < 512.0) {
                dark.push(s.pos);
                r.mark(s.pos + DVec3::Z * 32.0, "Dunkel: kein Licht in der Nähe", [120, 120, 255]);
            }
        }
    }
    r.say(format!(
        "Licht: {} Lichtquelle(n){}, {stuck} in Brushes, {named} schaltbar{}, {} dunkle Bereiche",
        lights.len(),
        if sun { " + Sonne" } else { "" },
        if named > 32 { " (zu viele – Source erlaubt nur etwa 32 schaltbare Lichter)" } else { "" },
        dark.len()
    ));
}

fn props(ed: &mut Editor, fades: bool, shadows: bool) -> (usize, usize) {
    let (mut f, mut s) = (0, 0);
    let bounds: Vec<(u32, f64)> = ed
        .map
        .entities
        .iter()
        .filter(|e| e.classname().starts_with("prop_"))
        .filter_map(|e| ed.model_path(e).and_then(|m| ed.model_bounds.get(&m)).map(|b| (e.id, b.size().length() * 0.5)))
        .collect();
    for (id, radius) in bounds {
        let Some(e) = ed.map.entity_mut(id) else { continue };
        if fades && e.get("fademaxdist").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) <= 0.0 && radius < 128.0 {
            let (min, max) = if radius < 24.0 { (800.0, 1000.0) } else if radius < 64.0 { (1500.0, 1800.0) } else { (2500.0, 3000.0) };
            e.set("fademindist", crate::map::fmt_num(min));
            e.set("fademaxdist", crate::map::fmt_num(max));
            f += 1;
        }
        if shadows && radius < 24.0 && e.get("disableshadows").unwrap_or("0") == "0" {
            e.set("disableshadows", "1");
            s += 1;
        }
    }
    (f, s)
}

/// Vor dem Kompilieren: optimierte Kopie in `<ordner>/amboss_build/<name>.vmf` schreiben.
/// Ok(Pfad, Logzeilen, Bericht) oder Err(Meldung, Bericht) bei Leck/Schreibfehler.
pub fn prepare_build(ed: &Editor, o: &OptSettings, fs: Option<&GameFs>, src: &Path) -> Result<(std::path::PathBuf, Vec<String>, Report), (String, Report)> {
    let map = ed.cordoned_map().unwrap_or_else(|| ed.map.clone());
    let dir = src.parent().map(|p| p.join("amboss_build")).unwrap_or_else(|| "amboss_build".into());
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join(src.file_name().unwrap_or_default());
    let packlist = out.with_extension("packlist.txt");
    let _ = std::fs::remove_file(&packlist);
    let mut copy = build_copy(ed, map);
    let t = std::time::Instant::now();
    let r = run(&mut copy, o, fs, Some(&packlist));
    if r.leak.is_some() && o.abort_on_leak {
        return Err(("Kompilieren abgebrochen: Die Map hat ein Leck (roter Pfad).".into(), r));
    }
    if let Err(e) = std::fs::write(&out, copy.map.to_vmf()) {
        return Err((format!("Optimierte Kopie konnte nicht geschrieben werden: {e}"), r));
    }
    let mut lines = vec![format!("===== Automatische Optimierungen ({:.1} s) =====", t.elapsed().as_secs_f64()), format!("Kopie: {} (Original bleibt unverändert)", out.display())];
    lines.extend(r.lines.iter().cloned());
    Ok((out, lines, r))
}

/// Einstellungen teilweise per JSON überschreiben (unbekannte Schlüssel -> Fehler)
pub fn merge_settings(o: &OptSettings, patch: &serde_json::Value) -> Result<OptSettings, String> {
    let mut v = serde_json::to_value(o).map_err(|e| e.to_string())?;
    let (Some(obj), Some(p)) = (v.as_object_mut(), patch.as_object()) else { return Err("Optionen müssen ein Objekt sein".into()) };
    for (k, val) in p {
        if !obj.contains_key(k) {
            return Err(format!("Unbekannte Option '{k}' (gültig: {})", obj.keys().cloned().collect::<Vec<_>>().join(", ")));
        }
        obj.insert(k.clone(), val.clone());
    }
    serde_json::from_value(v).map_err(|e| format!("Ungültiger Wert: {e}"))
}

/// Kopie eines Editors für Analyse/Build (ohne Undo-Verlauf)
pub fn build_copy(ed: &Editor, map: crate::map::Map) -> Editor {
    let mut c = Editor::new(ed.fgd.clone());
    c.map = map;
    c.model_bounds = ed.model_bounds.clone();
    c.grid = ed.grid;
    c.tex_scale = ed.tex_scale;
    c
}

#[allow(dead_code)]
fn _obj(_: Obj) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;

    /// Zwei Räume, verbunden durch eine Tür-Öffnung, mit Spawn, Licht und Säule
    fn two_rooms() -> Editor {
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "DEV/DEV_MEASUREGENERIC01B".into();
        ed.create_block(Aabb::new(DVec3::new(-512.0, -256.0, -16.0), DVec3::new(512.0, 256.0, 192.0)));
        ed.hollow(16.0);
        // Trennwand mit Tür bei x=0
        ed.create_block(Aabb::new(DVec3::new(-8.0, -240.0, 0.0), DVec3::new(8.0, 240.0, 176.0)));
        let cutter_id = {
            ed.create_block(Aabb::new(DVec3::new(-16.0, -32.0, 0.0), DVec3::new(16.0, 32.0, 112.0)));
            ed.selected_solid_ids()[0]
        };
        ed.carve();
        ed.sel = [Obj::Solid(cutter_id)].into_iter().collect();
        ed.delete_selection();
        // Tür
        ed.create_block(Aabb::new(DVec3::new(-2.0, -32.0, 0.0), DVec3::new(2.0, 32.0, 112.0)));
        ed.tie_to_entity("func_door");
        // Säule im linken Raum
        ed.create_block(Aabb::new(DVec3::new(-300.0, -16.0, 0.0), DVec3::new(-268.0, 16.0, 176.0)));
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(-400.0, 0.0, 0.0));
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(-256.0, 0.0, 150.0));
        ed.create_point_entity(DVec3::new(256.0, 0.0, 150.0));
        ed
    }

    #[test]
    fn full_run_on_sealed_map() {
        let mut ed = two_rooms();
        let o = OptSettings { areaportals: true, ai_hints: true, ai_air_nodes: true, merge_brushes: true, ..Default::default() };
        let r = run(&mut ed, &o, None, None);
        let text = r.lines.join("\n");
        assert!(r.leak.is_none(), "{text}");
        let nodes = ed.map.entities.iter().filter(|e| e.classname() == "info_node").count();
        assert!(nodes >= 12, "{text}");
        assert!(text.contains("1 zusammenhängende Bereiche"), "{text}");
        assert!(ed.map.entities.iter().any(|e| e.classname() == "env_cubemap"), "{text}");
        assert!(ed.map.entities.iter().any(|e| e.classname() == "info_node_link_controller"), "{text}");
        assert!(ed.map.entities.iter().any(|e| e.classname() == "func_areaportal"), "{text}");
        // Säule wurde func_detail, Außenwände nicht
        let detail = ed.map.entities.iter().find(|e| e.classname() == "func_detail").expect(&text);
        assert!(detail.solids.iter().any(|s| s.bounds.size().x == 32.0));
        assert!(detail.solids.iter().all(|s| s.bounds.size().max_element() < 600.0));
        assert!(text.contains("Lightmaps"), "{text}");
        assert!(ed.map.all_solids().all(|(_, s)| s.is_valid()));
        // Ergebnis ist speicherbar
        Map::from_vmf(&ed.map.to_vmf()).unwrap();
    }

    #[test]
    fn leak_is_found_with_path() {
        let mut ed = two_rooms();
        // Decke entfernen -> Leck
        let top = ed.map.world.solids.iter().max_by(|a, b| a.bounds.max.z.partial_cmp(&b.bounds.max.z).unwrap().then(a.bounds.min.z.partial_cmp(&b.bounds.min.z).unwrap())).unwrap().id;
        ed.map.take_solid(top);
        let r = run(&mut ed, &OptSettings::default(), None, None);
        let leak = r.leak.expect("Leck erwartet");
        assert!(leak.len() >= 2);
        assert!(leak.last().unwrap().z > 192.0);
    }

    #[test]
    fn build_copy_leaves_original() {
        let dir = std::env::temp_dir().join(format!("amboss_optbuild_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("zwei.vmf");
        let ed = two_rooms();
        let before = ed.map.to_vmf();
        let (out, lines, r) = prepare_build(&ed, &OptSettings::default(), None, &src).unwrap();
        assert_eq!(ed.map.to_vmf(), before);
        assert!(out.ends_with("amboss_build/zwei.vmf"));
        let built = Map::from_vmf(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert!(built.entities.iter().any(|e| e.classname() == "info_node"));
        assert!(lines[0].contains("Optimierungen") && r.changes > 0);
        // mit Leck: Abbruch
        let mut leaky = two_rooms();
        let top = leaky.map.world.solids.iter().max_by(|a, b| a.bounds.max.z.partial_cmp(&b.bounds.max.z).unwrap().then(a.bounds.min.z.partial_cmp(&b.bounds.min.z).unwrap())).unwrap().id;
        leaky.map.take_solid(top);
        assert!(prepare_build(&leaky, &OptSettings::default(), None, &src).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_and_cleanup() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0)));
        ed.create_block(Aabb::new(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0)));
        ed.create_block(Aabb::new(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0)));
        let mut e = Entity::new(ed.map.alloc_id(), "logic_relay");
        e.set("origin", "0 0 0");
        ed.map.entities.push(e.into());
        let o = OptSettings { merge_brushes: true, leak_check: false, ai_nodes: false, cubemaps: false, auto_detail: false, ..Default::default() };
        let r = run(&mut ed, &o, None, None);
        assert_eq!(ed.map.world.solids.len(), 1, "{:?}", r.lines);
        assert_eq!(ed.map.world.solids[0].bounds.max.x, 128.0);
        assert!(ed.map.entities.is_empty());
    }

    use crate::map::Map;
}
