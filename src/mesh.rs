//! Freie Polygon-Meshes: Netze mit beliebigen (auch nicht konvexen) Formen. In der Map gehört jedes
//! Mesh zu einem prop_static; die Netzdaten liegen in einer Begleitdatei neben der VMF
//! (`<map>.meshes.json`, VBSP sieht sie nie). Zum Kompilieren wird das Netz über studiomdl zum
//! Modell (wie „Auswahl als Modell“).

use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub idx: Vec<usize>,
    pub material: String,
}

/// Netz in Koordinaten relativ zum Ursprung seines Props
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    pub verts: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
}

/// Mesh einer Map: gehört zum Prop mit dieser Entity-ID
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshObj {
    pub ent: u32,
    pub mesh: Mesh,
    /// Modellname ohne Endung (models/amboss/meshes/<name>.mdl)
    pub name: String,
}

fn v(a: [f64; 3]) -> DVec3 {
    DVec3::from_array(a)
}

impl Mesh {
    pub fn vert(&self, i: usize) -> DVec3 {
        v(self.verts[i])
    }

    pub fn bounds(&self) -> crate::geometry::Aabb {
        self.verts.iter().fold(crate::geometry::Aabb::EMPTY, |b, p| b.union(&crate::geometry::Aabb::new(v(*p), v(*p))))
    }

    /// Flächennormale (Newell, robust auch für leicht nicht-ebene Vielecke)
    pub fn normal(&self, f: usize) -> DVec3 {
        let idx = &self.faces[f].idx;
        let mut n = DVec3::ZERO;
        for k in 0..idx.len() {
            let (a, b) = (self.vert(idx[k]), self.vert(idx[(k + 1) % idx.len()]));
            n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
        }
        n.normalize_or_zero()
    }

    pub fn center(&self, f: usize) -> DVec3 {
        let idx = &self.faces[f].idx;
        idx.iter().map(|&i| self.vert(i)).sum::<DVec3>() / idx.len().max(1) as f64
    }

    /// Dreiecke (Fächer) mit Flächenindex
    pub fn triangles(&self) -> Vec<([DVec3; 3], usize)> {
        let mut out = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for k in 1..f.idx.len().saturating_sub(1) {
                out.push(([self.vert(f.idx[0]), self.vert(f.idx[k]), self.vert(f.idx[k + 1])], fi));
            }
        }
        out
    }

    /// Punkte zusammenführen, die näher als `eps` beieinander liegen
    pub fn weld(&mut self, eps: f64) {
        let mut map = vec![0usize; self.verts.len()];
        let mut out: Vec<[f64; 3]> = Vec::new();
        for (i, p) in self.verts.iter().enumerate() {
            match out.iter().position(|q| (v(*q) - v(*p)).length() < eps) {
                Some(j) => map[i] = j,
                None => {
                    map[i] = out.len();
                    out.push(*p);
                }
            }
        }
        self.verts = out;
        for f in &mut self.faces {
            for i in &mut f.idx {
                *i = map[*i];
            }
            f.idx.dedup();
            if f.idx.len() > 1 && f.idx.first() == f.idx.last() {
                f.idx.pop();
            }
        }
        self.faces.retain(|f| f.idx.len() >= 3);
    }

    /// Aus Brush-Polygonen (Weltkoordinaten) relativ zu `origin`
    pub fn from_polys(polys: &[(Vec<DVec3>, String)], origin: DVec3) -> Mesh {
        let mut m = Mesh::default();
        for (poly, mat) in polys {
            let start = m.verts.len();
            // Brush-Polygone sind von außen gesehen gegen den Uhrzeigersinn (Normale nach außen)
            for p in poly.iter() {
                m.verts.push((*p - origin).to_array());
            }
            m.faces.push(Face { idx: (start..start + poly.len()).collect(), material: mat.clone() });
        }
        m.weld(0.01);
        m
    }

    /// Wavefront OBJ einlesen (v, f, usemtl); Y-oben wird zu Z-oben, Maßstab in Einheiten
    pub fn from_obj(src: &str, scale: f64, y_up: bool) -> Result<Mesh, String> {
        let mut m = Mesh::default();
        let mut mat = String::from("DEV/DEV_MEASUREGENERIC01");
        for line in src.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let c: Vec<f64> = it.take(3).filter_map(|x| x.parse().ok()).collect();
                    if c.len() == 3 {
                        let p = if y_up { [c[0], -c[2], c[1]] } else { [c[0], c[1], c[2]] };
                        m.verts.push(p.map(|x| x * scale));
                    }
                }
                Some("usemtl") => mat = it.collect::<Vec<_>>().join(" "),
                Some("f") => {
                    let n = m.verts.len() as i64;
                    let idx: Vec<usize> = it
                        .filter_map(|t| t.split('/').next()?.parse::<i64>().ok())
                        .map(|i| if i < 0 { (n + i) as usize } else { (i - 1) as usize })
                        .collect();
                    if idx.len() >= 3 && idx.iter().all(|&i| i < m.verts.len()) {
                        m.faces.push(Face { idx, material: mat.clone() });
                    }
                }
                _ => {}
            }
        }
        if m.faces.is_empty() {
            return Err("OBJ enthält keine Flächen".into());
        }
        Ok(m)
    }

    /// Ausgewählte Punkte verschieben
    pub fn move_verts(&mut self, sel: &BTreeSet<usize>, d: DVec3) {
        for &i in sel {
            if let Some(p) = self.verts.get_mut(i) {
                *p = (v(*p) + d).to_array();
            }
        }
    }

    /// Flächen herausziehen: neue Kappe um `dist` entlang der mittleren Normale, Seitenwände dazwischen.
    /// Liefert die Indizes der neuen Kappenflächen.
    pub fn extrude(&mut self, faces: &BTreeSet<usize>, dist: f64) -> BTreeSet<usize> {
        if faces.is_empty() {
            return BTreeSet::new();
        }
        let dir = faces.iter().map(|&f| self.normal(f)).sum::<DVec3>().normalize_or(DVec3::Z);
        // Randkanten der Auswahl (Kanten, die nur von einer ausgewählten Fläche benutzt werden)
        let mut count: std::collections::HashMap<(usize, usize), (i32, usize, usize, String)> = std::collections::HashMap::new();
        for &f in faces {
            let idx = &self.faces[f].idx;
            for k in 0..idx.len() {
                let (a, b) = (idx[k], idx[(k + 1) % idx.len()]);
                let key = (a.min(b), a.max(b));
                let e = count.entry(key).or_insert((0, a, b, self.faces[f].material.clone()));
                e.0 += 1;
            }
        }
        // neue Punkte für alle Punkte der Auswahl
        let mut newv = std::collections::HashMap::new();
        for &f in faces {
            for &i in &self.faces[f].idx.clone() {
                newv.entry(i).or_insert_with(|| {
                    self.verts.push((self.vert(i) + dir * dist).to_array());
                    self.verts.len() - 1
                });
            }
        }
        // Kappen: ausgewählte Flächen benutzen die neuen Punkte
        for &f in faces {
            for i in &mut self.faces[f].idx {
                *i = newv[i];
            }
        }
        // Seitenwände entlang der Randkanten (Richtung wie die Kante in der Fläche)
        for (_, (n, a, b, mat)) in count {
            if n == 1 {
                self.faces.push(Face { idx: vec![a, b, newv[&b], newv[&a]], material: mat });
            }
        }
        faces.clone()
    }

    /// Laplace-Glätten der ausgewählten (bzw. aller) Punkte
    pub fn smooth(&mut self, sel: &BTreeSet<usize>, iterations: usize, k: f64) {
        let n = self.verts.len();
        let mut nb: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
        for f in &self.faces {
            for j in 0..f.idx.len() {
                let (a, b) = (f.idx[j], f.idx[(j + 1) % f.idx.len()]);
                nb[a].insert(b);
                nb[b].insert(a);
            }
        }
        for _ in 0..iterations {
            let old = self.verts.clone();
            for i in 0..n {
                if (!sel.is_empty() && !sel.contains(&i)) || nb[i].is_empty() {
                    continue;
                }
                let avg = nb[i].iter().map(|&j| v(old[j])).sum::<DVec3>() / nb[i].len() as f64;
                self.verts[i] = (v(old[i]) + (avg - v(old[i])) * k).to_array();
            }
        }
    }

    /// Jede Fläche in Vierecke unterteilen (Mittelpunkt + Kantenmitten), danach leicht glätten
    pub fn subdivide(&mut self) {
        let old_faces = std::mem::take(&mut self.faces);
        let mut edge_mid: std::collections::HashMap<(usize, usize), usize> = std::collections::HashMap::new();
        for f in &old_faces {
            let c = f.idx.iter().map(|&i| self.vert(i)).sum::<DVec3>() / f.idx.len() as f64;
            self.verts.push(c.to_array());
            let ci = self.verts.len() - 1;
            let mut mids = Vec::new();
            for k in 0..f.idx.len() {
                let (a, b) = (f.idx[k], f.idx[(k + 1) % f.idx.len()]);
                let key = (a.min(b), a.max(b));
                let m = *edge_mid.entry(key).or_insert_with(|| {
                    self.verts.push(((self.vert(a) + self.vert(b)) * 0.5).to_array());
                    self.verts.len() - 1
                });
                mids.push(m);
            }
            for k in 0..f.idx.len() {
                let prev = mids[(k + f.idx.len() - 1) % f.idx.len()];
                self.faces.push(Face { idx: vec![f.idx[k], mids[k], ci, prev], material: f.material.clone() });
            }
        }
        self.smooth(&BTreeSet::new(), 1, 0.3);
    }

    pub fn delete_faces(&mut self, faces: &BTreeSet<usize>) {
        let mut k = 0;
        self.faces.retain(|_| {
            let keep = !faces.contains(&k);
            k += 1;
            keep
        });
    }

    pub fn flip(&mut self, faces: &BTreeSet<usize>) {
        for (i, f) in self.faces.iter_mut().enumerate() {
            if faces.is_empty() || faces.contains(&i) {
                f.idx.reverse();
            }
        }
    }

    /// SMD-Dreiecke (Texturkoordinaten planar nach Hauptachse, Maßstab 0,25 wie Brushes)
    pub fn smd_triangles(&self, tex_size: &dyn Fn(&str) -> (f64, f64)) -> String {
        let mut out = String::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let n = self.normal(fi);
            let (tw, th) = tex_size(&f.material);
            let an = n.abs();
            let uv = |p: DVec3| -> (f64, f64) {
                let (a, b) = if an.z >= an.x && an.z >= an.y { (p.x, -p.y) } else if an.x >= an.y { (p.y, -p.z) } else { (p.x, -p.z) };
                (a / 0.25 / tw, -(b / 0.25 / th))
            };
            let mat = crate::propper::smd_name(&f.material);
            for k in 1..f.idx.len().saturating_sub(1) {
                let _ = writeln!(out, "{mat}");
                for &i in &[f.idx[0], f.idx[k], f.idx[k + 1]] {
                    let p = self.vert(i);
                    let (u, w) = uv(p);
                    let _ = writeln!(out, "0 {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.5} {:.5} 0", p.x, p.y, p.z, n.x, n.y, n.z, u, w);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Mesh {
        // Würfel 0..64 aus einem Brush
        let mut m = crate::map::Map::new();
        let s = m.make_box(crate::geometry::Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "DEV/X", 0.25);
        let polys: Vec<(Vec<DVec3>, String)> = s.polys.iter().zip(&s.sides).map(|(p, side)| (p.clone(), side.material.clone())).collect();
        Mesh::from_polys(&polys, DVec3::new(32.0, 32.0, 0.0))
    }

    #[test]
    fn from_brush_closed_and_outward() {
        let m = cube();
        assert_eq!(m.verts.len(), 8, "Punkte zusammengeführt");
        assert_eq!(m.faces.len(), 6);
        // Normalen zeigen nach außen
        for f in 0..6 {
            let c = m.center(f);
            assert!(m.normal(f).dot(c - DVec3::new(0.0, 0.0, 32.0)) > 0.0, "Fläche {f}");
        }
        let b = m.bounds();
        assert_eq!(b.min, DVec3::new(-32.0, -32.0, 0.0));
    }

    #[test]
    fn extrude_top_face() {
        let mut m = cube();
        let top = (0..6).find(|&f| m.normal(f).z > 0.9).unwrap();
        let sel: BTreeSet<usize> = [top].into();
        m.extrude(&sel, 32.0);
        assert_eq!(m.faces.len(), 10, "4 neue Seitenwände");
        assert_eq!(m.verts.len(), 12);
        assert!((m.bounds().max.z - 96.0).abs() < 1e-9);
        // Seitenwände zeigen nach außen
        for f in 6..10 {
            let c = m.center(f);
            assert!(m.normal(f).dot(DVec3::new(c.x, c.y, 0.0)) > 0.0, "Wand {f}");
        }
        // SMD: 10 Vierecke = 20 Dreiecke
        let smd = m.smd_triangles(&|_| (512.0, 512.0));
        assert_eq!(smd.lines().filter(|l| l.starts_with("dev_x")).count(), 20);
    }

    #[test]
    fn obj_and_subdivide() {
        let src = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nusemtl brick/wall\nf 1 2 3 4\n";
        let mut m = Mesh::from_obj(src, 64.0, false).unwrap();
        assert_eq!(m.faces[0].material, "brick/wall");
        assert!((m.vert(2) - DVec3::new(64.0, 64.0, 0.0)).length() < 1e-9);
        m.subdivide();
        assert_eq!(m.faces.len(), 4);
        assert_eq!(m.verts.len(), 9);
        assert!(Mesh::from_obj("v 0 0 0\n", 1.0, false).is_err());
    }
}
