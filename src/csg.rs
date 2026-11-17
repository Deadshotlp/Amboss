//! Brush-CSG: Schneiden an Ebenen, Carve, Aushöhlen, Vertex-Bearbeitung.

use crate::geometry::{hull_planes, Plane, EPS};
use crate::map::{default_tex_axes, Map, Side, Solid};
use glam::DVec3;

/// Fügt dem Solid eine Schnittebene hinzu (behält die Rückseite). None, wenn nichts übrig bleibt.
fn with_cut(map: &mut Map, s: &Solid, plane: Plane, material: &str, tex_scale: f64, new_ids: bool) -> Option<Solid> {
    let mut c = s.clone();
    if new_ids {
        map.reassign_ids(&mut c);
    }
    let side = map.side_from_plane(plane, material, tex_scale);
    c.sides.push(side);
    c.cleanup();
    c.is_valid().then_some(c)
}

/// Teilt einen Solid an einer Ebene: (Vorderseite, Rückseite).
/// Liegt der Solid komplett auf einer Seite, ist die andere None und der Solid unverändert.
pub fn split(map: &mut Map, s: &Solid, plane: Plane, material: &str, tex_scale: f64) -> (Option<Solid>, Option<Solid>) {
    let mut front = false;
    let mut back = false;
    for p in s.polys.iter().flatten() {
        let d = plane.distance(*p);
        if d > EPS * 10.0 {
            front = true;
        } else if d < -EPS * 10.0 {
            back = true;
        }
    }
    match (front, back) {
        (true, false) | (false, false) => (Some(s.clone()), None),
        (false, true) => (None, Some(s.clone())),
        (true, true) => {
            let b = with_cut(map, s, plane, material, tex_scale, false);
            let f = with_cut(map, s, Plane { normal: -plane.normal, dist: -plane.dist }, material, tex_scale, true);
            (f, b)
        }
    }
}

/// Häufigstes Material eines Solids (für neue Schnittflächen)
pub fn main_material(s: &Solid) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for side in &s.sides {
        match counts.iter_mut().find(|c| c.0 == side.material) {
            Some(c) => c.1 += 1,
            None => counts.push((side.material.clone(), 1)),
        }
    }
    counts.into_iter().max_by_key(|c| c.1).map(|c| c.0).unwrap_or_else(|| "TOOLS/TOOLSNODRAW".into())
}

/// Schneidet `target` mit dem konvexen `cutter`. Liefert die Teile außerhalb des Cutters,
/// oder None, wenn sich die beiden nicht überschneiden.
pub fn carve(map: &mut Map, target: &Solid, cutter: &Solid, tex_scale: f64) -> Option<Vec<Solid>> {
    if !target.bounds.intersects(&cutter.bounds) {
        return None;
    }
    let mut pieces = Vec::new();
    let mut rest = target.clone();
    for (i, pl) in cutter.planes.iter().enumerate() {
        let Some(pl) = pl else { continue };
        let mat = cutter.sides[i].material.clone();
        let (f, b) = split(map, &rest, *pl, &mat, tex_scale);
        match (f, b) {
            (Some(f), None) => {
                // komplett außerhalb dieser Ebene → keine Überschneidung
                if pieces.is_empty() {
                    return None;
                }
                pieces.push(f);
                return Some(pieces);
            }
            (f, Some(b)) => {
                if let Some(f) = f {
                    pieces.push(f);
                }
                rest = b;
            }
            (None, None) => break,
        }
    }
    // was übrig bleibt, liegt im Cutter und fällt weg
    Some(pieces)
}

/// Echtes Aushöhlen: eine Wand pro Fläche (beliebige konvexe Brushes).
/// Positive Dicke = Wände innen, negative = außen.
///
/// Die Wände überschneiden sich nicht: Jede Wand wird an den Innenebenen der vorher gebauten
/// Wände abgeschnitten (Reihenfolge: Boden/Decke zuerst über die volle Fläche, dann die Wände
/// entlang X, zuletzt die dazwischen) – so bleibt jede Wand konvex und die Ecken gehören genau
/// einer Wand.
pub fn hollow(map: &mut Map, s: &Solid, thickness: f64, tex_scale: f64) -> Vec<Solid> {
    let mut out = Vec::new();
    let t = thickness.abs();
    let mut order: Vec<(usize, Plane)> = s.planes.iter().enumerate().filter_map(|(i, p)| p.map(|p| (i, p))).collect();
    order.sort_by(|a, b| {
        let k = |p: &Plane| (-(p.normal.z.abs() * 1000.0).round() as i64, -(p.normal.x.abs() * 1000.0).round() as i64);
        k(&a.1).cmp(&k(&b.1))
    });
    for (oi, &(i, pl)) in order.iter().enumerate() {
        let pl = &pl;
        let mut sides: Vec<Side> = Vec::new();
        for (j, other) in s.planes.iter().enumerate() {
            let Some(other) = other else { continue };
            let mut side = s.sides[j].clone();
            side.id = map.alloc_id();
            side.disp = None;
            if thickness < 0.0 {
                // nach außen: alle Ebenen (inkl. der eigenen) um die Dicke verschieben
                side.plane_pts = shift_points(&side.plane_pts, other.normal * t);
            }
            sides.push(side);
        }
        // Gegenebene: innen (bzw. die ursprüngliche Fläche bei Außenwänden)
        let inner = if thickness >= 0.0 {
            Plane { normal: -pl.normal, dist: -(pl.dist - t) }
        } else {
            Plane { normal: -pl.normal, dist: -pl.dist }
        };
        let mut inner_side = map.side_from_plane(inner, &s.sides[i].material, tex_scale);
        let (u, v) = default_tex_axes(-pl.normal, s.sides[i].uaxis.scale);
        inner_side.uaxis = u;
        inner_side.vaxis = v;
        sides.push(inner_side);
        // nicht in die schon gebauten Wände hineinragen: innerhalb ihrer Innenebene bleiben
        for &(_, prev) in &order[..oi] {
            let d = if thickness >= 0.0 { prev.dist - t } else { prev.dist };
            let mut seam = map.side_from_plane(Plane { normal: prev.normal, dist: d }, &s.sides[i].material, tex_scale);
            let (u, v) = default_tex_axes(prev.normal, s.sides[i].uaxis.scale);
            seam.uaxis = u;
            seam.vaxis = v;
            sides.push(seam);
        }
        let id = map.alloc_id();
        let mut w = Solid::new(id, sides);
        w.editor = s.editor.clone();
        w.cleanup();
        if w.is_valid() {
            out.push(w);
        }
    }
    out
}

fn shift_points(p: &[DVec3; 3], d: DVec3) -> [DVec3; 3] {
    [p[0] + d, p[1] + d, p[2] + d]
}

/// Baut einen Solid aus neuen Eckpunkten neu auf (Vertex-Werkzeug).
/// Materialien/Texturachsen werden von der alten Fläche mit der ähnlichsten Normale übernommen.
pub fn rebuild_from_vertices(map: &mut Map, old: &Solid, verts: &[DVec3]) -> Option<Solid> {
    let planes = hull_planes(verts);
    if planes.len() < 4 {
        return None;
    }
    let mut sides = Vec::new();
    for pl in planes {
        let best = old
            .sides
            .iter()
            .zip(&old.planes)
            .filter_map(|(s, p)| p.map(|p| (s, p.normal.dot(pl.normal))))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut side = map.side_from_plane(pl, &best.map(|b| b.0.material.clone()).unwrap_or_default(), 0.25);
        if let Some((src, dot)) = best {
            if dot > 0.7 {
                side.uaxis = src.uaxis.clone();
                side.vaxis = src.vaxis.clone();
                side.lightmapscale = src.lightmapscale;
                side.smoothing_groups = src.smoothing_groups.clone();
                if dot > 0.9999 {
                    side.id = src.id;
                }
            } else {
                let (u, v) = default_tex_axes(pl.normal, src.uaxis.scale);
                side.uaxis = u;
                side.vaxis = v;
            }
        }
        sides.push(side);
    }
    let mut s = Solid::new(old.id, sides);
    s.editor = old.editor.clone();
    s.extra = old.extra.clone();
    s.cleanup();
    s.is_valid().then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Aabb;

    #[test]
    fn split_and_carve() {
        let mut map = Map::new();
        let s = map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "a", 0.25);
        let (f, b) = split(&mut map, &s, Plane { normal: DVec3::X, dist: 16.0 }, "cut", 0.25);
        let (f, b) = (f.unwrap(), b.unwrap());
        assert_eq!(b.bounds.max.x, 16.0);
        assert_eq!(f.bounds.min.x, 16.0);
        assert_eq!(f.sides.len(), 6);
        assert_ne!(f.id, b.id);

        // Loch in eine Wand schneiden
        let wall = map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(256.0, 16.0, 128.0)), "wall", 0.25);
        let door = map.make_box(Aabb::new(DVec3::new(96.0, -8.0, 0.0), DVec3::new(160.0, 24.0, 96.0)), "door", 0.25);
        let pieces = carve(&mut map, &wall, &door, 0.25).unwrap();
        assert_eq!(pieces.len(), 3);
        let vol: f64 = pieces.iter().map(|p| p.bounds.size().x * p.bounds.size().y * p.bounds.size().z).sum();
        assert!((vol - (256.0 * 16.0 * 128.0 - 64.0 * 16.0 * 96.0)).abs() < 1.0);
        // keine Überschneidung → None
        let far = map.make_box(Aabb::new(DVec3::splat(1000.0), DVec3::splat(1100.0)), "x", 0.25);
        assert!(carve(&mut map, &wall, &far, 0.25).is_none());
    }

    #[test]
    fn hollow_any_brush() {
        let mut map = Map::new();
        // Keil (nicht quaderförmig)
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(256.0, 0.0, 0.0),
            DVec3::new(256.0, 256.0, 0.0),
            DVec3::new(0.0, 256.0, 0.0),
            DVec3::new(0.0, 256.0, 256.0),
            DVec3::new(256.0, 256.0, 256.0),
        ];
        let wedge = map.solid_from_points(&pts, "a", 0.25).unwrap();
        assert_eq!(wedge.sides.len(), 5);
        let walls = hollow(&mut map, &wedge, 16.0, 0.25);
        assert_eq!(walls.len(), 5);
        assert!(walls.iter().all(|w| w.is_valid()));
        let out = hollow(&mut map, &wedge, -16.0, 0.25);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|w| w.bounds.min.x <= -15.0 || w.bounds.max.x >= 256.0 || w.bounds.min.z < 0.0 || w.bounds.max.y > 256.0 || w.bounds.max.z > 0.0));
    }

    /// Wände eines ausgehöhlten Quaders überschneiden sich nicht und füllen genau die Hülle
    #[test]
    fn hollow_walls_do_not_overlap() {
        for t in [16.0, -16.0] {
            let mut map = Map::new();
            let b = map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(256.0, 192.0, 128.0)), "a", 0.25);
            let walls = hollow(&mut map, &b, t, 0.25);
            assert_eq!(walls.len(), 6);
            for i in 0..walls.len() {
                for j in i + 1..walls.len() {
                    let lo = walls[i].bounds.min.max(walls[j].bounds.min);
                    let hi = walls[i].bounds.max.min(walls[j].bounds.max);
                    assert!((hi - lo).min_element() <= 0.01, "Dicke {t}: Wände {i} und {j} überschneiden sich");
                }
            }
            let vol: f64 = walls.iter().map(|w| { let s = w.bounds.size(); s.x * s.y * s.z }).sum();
            let expect = if t > 0.0 { 256.0 * 192.0 * 128.0 - 224.0 * 160.0 * 96.0 } else { 288.0 * 224.0 * 160.0 - 256.0 * 192.0 * 128.0 };
            assert!((vol - expect).abs() < 1.0, "Dicke {t}: Volumen {vol} statt {expect}");
        }
    }

    #[test]
    fn vertex_rebuild() {
        let mut map = Map::new();
        let s = map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "a", 0.25);
        let mut v = s.vertices();
        // eine obere Ecke nach oben ziehen → Fläche wird geteilt
        let i = v.iter().position(|p| *p == DVec3::new(64.0, 64.0, 64.0)).unwrap();
        v[i].z = 96.0;
        let r = rebuild_from_vertices(&mut map, &s, &v).unwrap();
        assert!(r.is_valid());
        assert_eq!(r.bounds.max.z, 96.0);
        assert_eq!(r.id, s.id);
        assert!(r.sides.len() >= 7);
    }
}
