//! Geometrie eines info_overlay wie im Spiel: das Rechteck wird auf die im Schlüssel `sides`
//! genannten Flächen projiziert und an ihnen zugeschnitten – bei Displacements folgt es der
//! verformten Oberfläche (VBSP macht das beim Kompilieren genauso).

use crate::map::{Entity, Map};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

/// Basis und Ecken eines Overlays
#[derive(Clone, Debug)]
pub struct Basis {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub n: DVec3,
    /// Ecken in Basis-Koordinaten (x entlang u, y entlang v)
    pub uv: [DVec2; 4],
    /// Texturbereich: StartU, EndU, StartV, EndV
    pub st: [f32; 4],
}

impl Basis {
    pub fn of(e: &Entity) -> Option<Basis> {
        let g = |k: &str| e.get(k).and_then(crate::map::parse_vec);
        let f = |k: &str, d: f32| e.get(k).and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(d);
        let c = |k: &str| g(k).map(|p| DVec2::new(p.x, p.y));
        Some(Basis {
            origin: g("BasisOrigin")?,
            u: g("BasisU")?,
            v: g("BasisV")?,
            n: g("BasisNormal")?.normalize_or_zero(),
            uv: [c("uv0")?, c("uv1")?, c("uv2")?, c("uv3")?],
            st: [f("StartU", 0.0), f("EndU", 1.0), f("StartV", 0.0), f("EndV", 1.0)],
        })
    }

    fn local(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    /// Texturkoordinate eines Punkts (in Basis-Koordinaten) über die umgekehrte bilineare Abbildung
    /// des Ecken-Vierecks: Ecke 0 → (StartU, StartV), 1 → (StartU, EndV), 2 → (EndU, EndV), 3 → (EndU, StartV)
    pub fn tex(&self, q: DVec2) -> [f32; 2] {
        let [p0, p1, p2, p3] = self.uv;
        let at = |a: f64, b: f64| p0 * ((1.0 - a) * (1.0 - b)) + p3 * (a * (1.0 - b)) + p2 * (a * b) + p1 * ((1.0 - a) * b);
        // Newton-Verfahren, für Rechtecke nach einem Schritt exakt
        let (mut a, mut b) = (0.5, 0.5);
        for _ in 0..8 {
            let r = at(a, b) - q;
            let da = (p3 - p0) * (1.0 - b) + (p2 - p1) * b;
            let db = (p1 - p0) * (1.0 - a) + (p2 - p3) * a;
            let det = da.x * db.y - da.y * db.x;
            if det.abs() < 1e-12 {
                break;
            }
            let ia = (r.x * db.y - r.y * db.x) / det;
            let ib = (da.x * r.y - da.y * r.x) / det;
            a -= ia;
            b -= ib;
            if ia.abs() + ib.abs() < 1e-9 {
                break;
            }
        }
        let [su, eu, sv, ev] = self.st;
        [su + (eu - su) * a as f32, sv + (ev - sv) * b as f32]
    }
}

/// Ein Eckpunkt des fertigen Overlay-Netzes
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OVert {
    pub p: DVec3,
    pub n: DVec3,
    pub uv: [f32; 2],
}

/// Fläche-ID → (Brush-ID, Seitenindex) für alle Brushes der Map
pub fn side_index(map: &Map) -> HashMap<u32, (u32, usize)> {
    let mut m = HashMap::new();
    for (_, s) in map.all_solids() {
        for (i, side) in s.sides.iter().enumerate() {
            m.insert(side.id, (s.id, i));
        }
    }
    m
}

/// Abstand über der Fläche gegen Z-Fighting
const LIFT: f64 = 0.6;

/// Dreiecke des Overlays auf seinen Flächen. `None`, wenn keine der genannten Flächen existiert –
/// dann zeichnet der Aufrufer das einfache Rechteck.
pub fn build(map: &Map, idx: &HashMap<u32, (u32, usize)>, e: &Entity) -> Option<Vec<[OVert; 3]>> {
    let b = Basis::of(e)?;
    let ids: Vec<u32> = e.get("sides").unwrap_or("").split_whitespace().filter_map(|s| s.parse().ok()).collect();
    let mut tris = Vec::new();
    let mut found = false;
    // Viereck im Uhrzeigersinn oder dagegen? Für das Zuschneiden müssen die Kanten nach innen zeigen
    let area: f64 = (0..4).map(|k| b.uv[k].perp_dot(b.uv[(k + 1) % 4])).sum();
    let quad: Vec<DVec2> = if area >= 0.0 { b.uv.to_vec() } else { b.uv.iter().rev().copied().collect() };
    for id in ids {
        let Some(&(sid, i)) = idx.get(&id) else { continue };
        let Some(s) = map.solid(sid) else { continue };
        found = true;
        let mut src: Vec<[DVec3; 3]> = Vec::new();
        if let Some(Some(g)) = s.disps.get(i) {
            for t in g.triangles() {
                src.push([g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]]);
            }
        } else if let Some(poly) = s.polys.get(i) {
            for k in 1..poly.len().saturating_sub(1) {
                src.push([poly[0], poly[k], poly[k + 1]]);
            }
        }
        for t in src {
            let tn = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            // nur Flächen, die zur Overlay-Normale zeigen (sonst landet es auf Rückseiten)
            let facing = tn.dot(b.n);
            if facing.abs() < 0.05 {
                continue;
            }
            let tn = if facing < 0.0 { -tn } else { tn };
            let mut poly: Vec<(DVec2, DVec3)> = t.iter().map(|p| (b.local(*p), *p)).collect();
            for k in 0..4 {
                poly = clip(&poly, quad[k], quad[(k + 1) % 4]);
                if poly.len() < 3 {
                    break;
                }
            }
            if poly.len() < 3 {
                continue;
            }
            let vs: Vec<OVert> = poly.iter().map(|(q, p)| OVert { p: *p + tn * LIFT, n: tn, uv: b.tex(*q) }).collect();
            for k in 1..vs.len() - 1 {
                tris.push([vs[0], vs[k], vs[k + 1]]);
            }
        }
    }
    found.then_some(tris)
}

/// Sutherland-Hodgman: behält den Teil links der Kante a→b (Viereck gegen den Uhrzeigersinn)
fn clip(poly: &[(DVec2, DVec3)], a: DVec2, b: DVec2) -> Vec<(DVec2, DVec3)> {
    let side = |q: DVec2| (b - a).perp_dot(q - a);
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
        let (sp, sq) = (side(p.0), side(q.0));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basis() -> Basis {
        Basis { origin: DVec3::ZERO, u: DVec3::X, v: DVec3::Y, n: DVec3::Z, uv: [DVec2::new(-10.0, -10.0), DVec2::new(-10.0, 10.0), DVec2::new(10.0, 10.0), DVec2::new(10.0, -10.0)], st: [0.0, 1.0, 0.0, 1.0] }
    }

    #[test]
    fn tex_mapping() {
        let b = basis();
        assert_eq!(b.tex(DVec2::new(-10.0, -10.0)), [0.0, 0.0]);
        assert_eq!(b.tex(DVec2::new(10.0, 10.0)), [1.0, 1.0]);
        let m = b.tex(DVec2::new(0.0, 5.0));
        assert!((m[0] - 0.5).abs() < 1e-5 && (m[1] - 0.75).abs() < 1e-5, "{m:?}");
        // schiefes Viereck: Ecken bleiben Ecken
        let mut s = b.clone();
        s.uv[2] = DVec2::new(14.0, 12.0);
        let t = s.tex(DVec2::new(14.0, 12.0));
        assert!((t[0] - 1.0).abs() < 1e-4 && (t[1] - 1.0).abs() < 1e-4, "{t:?}");
    }

    #[test]
    fn clips_to_quad() {
        // großes Dreieck über dem Overlay → nur der Teil innerhalb von ±10 bleibt
        let p: Vec<(DVec2, DVec3)> = [DVec3::new(-100.0, -100.0, 0.0), DVec3::new(100.0, -100.0, 0.0), DVec3::new(0.0, 100.0, 0.0)].iter().map(|p| (DVec2::new(p.x, p.y), *p)).collect();
        let q = basis().uv;
        let mut poly = p;
        // Viereck ist im Uhrzeigersinn angegeben → wie in build() umdrehen
        let quad: Vec<DVec2> = q.iter().rev().copied().collect();
        for k in 0..4 {
            poly = clip(&poly, quad[k], quad[(k + 1) % 4]);
        }
        assert!(poly.len() >= 4);
        assert!(poly.iter().all(|(q, _)| q.x.abs() <= 10.0 + 1e-9 && q.y.abs() <= 10.0 + 1e-9), "{poly:?}");
    }

    #[test]
    fn follows_displacement() {
        // Brush mit Displacement (Power 2) und Overlay in der Mitte: Höhe des Overlays folgt der Verformung
        // Würfel 0..128, Oberseite als Displacement mit Erhebung in der Mitte
        let mut m = Map::new();
        let mut s = m.make_box(crate::geometry::Aabb::new(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
        s.update_geometry();
        let top = (0..s.sides.len()).find(|&i| s.planes[i].is_some_and(|p| p.normal.z > 0.9)).unwrap();
        let top_id = s.sides[top].id;
        let n = s.planes[top].unwrap().normal;
        s.sides[top].disp = crate::disp::Disp::new_flat(2, &s.polys[top], n);
        if let Some(d) = s.sides[top].disp.as_mut() {
            let r = d.res();
            for a in 0..r {
                for c in 0..r {
                    // Pyramide: Mitte 32 hoch
                    let h = 32.0 - 16.0 * ((a as f64 - 2.0).abs().max((c as f64 - 2.0).abs()));
                    d.set_vector(a * r + c, DVec3::Z * h.max(0.0), n);
                }
            }
        }
        s.update_geometry();
        m.world.solids.push(crate::map::Shared::new(s));
        let mut e = Entity::new(2, "info_overlay");
        e.set("BasisOrigin", "64 64 64");
        e.set("BasisU", "1 0 0");
        e.set("BasisV", "0 1 0");
        e.set("BasisNormal", "0 0 1");
        for (k, c) in ["-16 -16 0", "-16 16 0", "16 16 0", "16 -16 0"].iter().enumerate() {
            e.set(&format!("uv{k}"), *c);
        }
        e.set("sides", &top_id.to_string());
        let idx = side_index(&m);
        let tris = build(&m, &idx, &e).expect("Fläche gefunden");
        assert!(!tris.is_empty());
        let max_z = tris.iter().flatten().map(|v| v.p.z).fold(f64::MIN, f64::max);
        let min_z = tris.iter().flatten().map(|v| v.p.z).fold(f64::MAX, f64::min);
        // flach wäre alles bei 64.6 – auf dem Displacement steigt es zur Mitte hin an
        // Mitte der Pyramide: Höhen zwischen 24 und 32 über der Fläche (+ Abstand gegen Z-Fighting)
        assert!(min_z > 64.0 + 23.0 && max_z > 64.0 + 31.0, "min {min_z} max {max_z}");
        assert!(max_z - min_z > 6.0, "min {min_z} max {max_z}");
        // alles innerhalb des Overlay-Rechtecks
        assert!(tris.iter().flatten().all(|v| (v.p.x - 64.0).abs() <= 16.5 && (v.p.y - 64.0).abs() <= 16.5));
        // ohne gültige Fläche: None (dann Rechteck)
        e.set("sides", "999");
        assert!(build(&m, &idx, &e).is_none());
    }
}
