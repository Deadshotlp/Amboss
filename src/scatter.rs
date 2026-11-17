//! Vegetation (oder beliebige Modelle) streuen: Pinsel in der 3D-Ansicht oder Auffüllen der
//! ausgewählten Flächen. Dichte, Mindestabstand, Hangneigung, zufällige Drehung und Ausrichtung an
//! der Oberfläche einstellbar; Shift radiert.

use crate::editor::{Editor, Obj};
use crate::map::{fmt_vec, Entity};
use crate::transform::basis_to_angles;
use glam::DVec3;

#[derive(Clone, Debug)]
pub struct ScatterOpts {
    /// Pinsel in der 3D-Ansicht aktiv
    pub active: bool,
    /// Modelle, aus denen zufällig gewählt wird
    pub models: Vec<String>,
    pub radius: f64,
    /// Objekte je 256 × 256 Einheiten Fläche
    pub density: f64,
    pub min_dist: f64,
    /// größte Hangneigung in Grad
    pub max_slope: f64,
    /// an der Oberfläche ausrichten (sonst aufrecht)
    pub align: bool,
    pub random_yaw: bool,
    /// zusätzliche zufällige Neigung in Grad
    pub tilt: f64,
    /// prop_static (Standard) oder prop_dynamic usw.
    pub class: String,
    /// Höhe über/unter der Oberfläche
    pub sink: f64,
    seed: u64,
    painted: bool,
}

impl Default for ScatterOpts {
    fn default() -> Self {
        ScatterOpts {
            active: false,
            models: Vec::new(),
            radius: 192.0,
            density: 3.0,
            min_dist: 48.0,
            max_slope: 35.0,
            align: false,
            random_yaw: true,
            tilt: 4.0,
            class: "prop_static".into(),
            sink: 0.0,
            seed: 0x51ED_2701,
            painted: false,
        }
    }
}

impl ScatterOpts {
    fn rnd(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Editor {
    /// Oberfläche unter einem Strahl: Punkt und Normale (bei Displacements die des getroffenen Dreiecks)
    pub fn surface_at(&self, o: DVec3, d: DVec3) -> Option<(DVec3, DVec3)> {
        let (_, face, t) = self.pick_3d(o, d)?;
        let (sid, i) = face?;
        let s = self.map.solid(sid)?;
        let p = o + d * t;
        if let Some(Some(g)) = s.disps.get(i) {
            let face_n = s.planes.get(i).copied().flatten().map(|pl| pl.normal).unwrap_or(DVec3::Z);
            let mut best: Option<(f64, DVec3)> = None;
            for tri in g.triangles() {
                let (a, b, c) = (g.pos[tri[0]], g.pos[tri[1]], g.pos[tri[2]]);
                if let Some(h) = crate::disp::ray_triangle(o, d, a, b, c) {
                    if best.is_none_or(|x| h < x.0) {
                        let mut n = (b - a).cross(c - a).normalize_or_zero();
                        if n.dot(face_n) < 0.0 {
                            n = -n;
                        }
                        best = Some((h, n));
                    }
                }
            }
            return best.map(|(h, n)| (o + d * h, n)).or(Some((p, face_n)));
        }
        let n = s.planes.get(i).copied().flatten().map(|pl| pl.normal).unwrap_or(DVec3::Z);
        Some((p, n))
    }

    /// Bereits gestreute Objekte (Props mit einem der Modelle) in der Nähe
    fn scatter_near(&self, p: DVec3, r: f64) -> Vec<(u32, DVec3)> {
        let models: Vec<String> = self.scatter.models.iter().map(|m| m.to_ascii_lowercase()).collect();
        self.map
            .entities
            .iter()
            .filter(|e| e.classname().starts_with("prop_") && e.get("model").is_some_and(|m| models.contains(&m.to_ascii_lowercase())))
            .filter_map(|e| e.origin().map(|o| (e.id, o)))
            .filter(|(_, o)| (*o - p).length() < r)
            .collect()
    }

    /// Ein Objekt setzen (ohne Rückgängig-Schritt)
    fn scatter_place(&mut self, p: DVec3, n: DVec3) -> Option<u32> {
        let o = &mut self.scatter;
        if o.models.is_empty() {
            return None;
        }
        let k = ((o.rnd() * o.models.len() as f64) as usize).min(o.models.len() - 1);
        let model = o.models[k].clone();
        let yaw = if o.random_yaw { o.rnd() * 360.0 } else { 0.0 };
        let tilt_a = o.rnd() * std::f64::consts::TAU;
        let tilt = o.tilt * o.rnd();
        let up = if o.align { n } else { DVec3::Z };
        // leichte zufällige Neigung um eine zufällige Achse
        let axis = DVec3::new(tilt_a.cos(), tilt_a.sin(), 0.0);
        let up = glam::DQuat::from_axis_angle(axis, tilt.to_radians()) * up;
        let fwd0 = DVec3::new(yaw.to_radians().cos(), yaw.to_radians().sin(), 0.0);
        let fwd = (fwd0 - up * fwd0.dot(up)).normalize_or(DVec3::X);
        let left = up.cross(fwd);
        let ang = basis_to_angles(fwd, left, up);
        let class = o.class.clone();
        let sink = o.sink;
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, &class);
        e.set("model", model);
        e.set("origin", fmt_vec(p - up * sink));
        e.set("angles", fmt_vec(ang));
        if class == "prop_static" {
            e.set("solid", "6");
        }
        self.map.entities.push(e.into());
        Some(id)
    }

    /// Pinselstrich an der Stelle unter dem Strahl: füllt bis zur Dichte auf bzw. radiert (`erase`).
    /// Liefert die Pinselmitte für die Anzeige.
    pub fn scatter_paint(&mut self, o: DVec3, d: DVec3, first: bool, erase: bool) -> Option<DVec3> {
        let (c, n) = self.surface_at(o, d)?;
        if first {
            self.checkpoint();
            self.scatter.painted = false;
        }
        let r = self.scatter.radius;
        if erase {
            let gone: Vec<u32> = self.scatter_near(c, r).into_iter().map(|(id, _)| id).collect();
            if !gone.is_empty() {
                self.map.entities.retain(|e| !gone.contains(&e.id));
                self.sel.retain(|x| !matches!(x, Obj::Entity(id) if gone.contains(id)));
                self.touch();
            }
            return Some(c);
        }
        let area = std::f64::consts::PI * r * r;
        let target = (self.scatter.density * area / 65536.0).round() as usize;
        let mut have = self.scatter_near(c, r);
        let mut placed = 0;
        // je Schritt einige Versuche – volle Dichte entsteht beim Malen über mehrere Bilder
        for _ in 0..12 {
            if have.len() >= target.max(1) || placed >= 4 {
                break;
            }
            let (a, rr) = (self.scatter.rnd() * std::f64::consts::TAU, r * self.scatter.rnd().sqrt());
            // in der Ebene der Oberfläche versetzen, dann von oben auf die Geometrie projizieren
            let t1 = n.any_orthonormal_vector();
            let t2 = n.cross(t1);
            let guess = c + (t1 * a.cos() + t2 * a.sin()) * rr;
            let start = guess + n * 256.0;
            let Some((p, pn)) = self.surface_at(start, -n) else { continue };
            if pn.z < self.scatter.max_slope.to_radians().cos() {
                continue;
            }
            if have.iter().any(|(_, q)| (*q - p).length() < self.scatter.min_dist) {
                continue;
            }
            if let Some(id) = self.scatter_place(p, pn) {
                have.push((id, p));
                placed += 1;
            }
        }
        if placed > 0 {
            self.scatter.painted = true;
            self.touch();
        }
        Some(c)
    }

    /// Ausgewählte Brushes (Oberseiten, auch Displacements) mit Objekten füllen; liefert die Anzahl
    pub fn scatter_fill_selection(&mut self) -> usize {
        let ids = self.selected_solid_ids();
        if ids.is_empty() || self.scatter.models.is_empty() {
            return 0;
        }
        // Dreiecke der nach oben zeigenden Flächen mit Fläche
        let max_slope = self.scatter.max_slope.to_radians().cos();
        let mut tris: Vec<([DVec3; 3], f64)> = Vec::new();
        for id in &ids {
            let Some(s) = self.map.solid(*id) else { continue };
            let has_disp = s.has_disp();
            for (i, g) in s.disps.iter().enumerate() {
                if let Some(g) = g {
                    for t in g.triangles() {
                        let tri = [g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]];
                        let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                        if n.normalize_or_zero().z.abs() >= max_slope {
                            tris.push((tri, n.length() * 0.5));
                        }
                    }
                } else if !has_disp {
                    let Some(Some(pl)) = s.planes.get(i) else { continue };
                    if pl.normal.z < max_slope {
                        continue;
                    }
                    let poly = &s.polys[i];
                    for k in 1..poly.len().saturating_sub(1) {
                        let tri = [poly[0], poly[k], poly[k + 1]];
                        tris.push((tri, (tri[1] - tri[0]).cross(tri[2] - tri[0]).length() * 0.5));
                    }
                }
            }
        }
        let total: f64 = tris.iter().map(|t| t.1).sum();
        if total <= 0.0 {
            return 0;
        }
        self.checkpoint();
        let target = (self.scatter.density * total / 65536.0).round() as usize;
        let mut have: Vec<DVec3> = self.scatter_near(DVec3::ZERO, f64::MAX).into_iter().map(|(_, p)| p).collect();
        let mut placed = 0;
        for _ in 0..target * 6 {
            if placed >= target {
                break;
            }
            // Dreieck nach Fläche wählen, Punkt gleichverteilt darin
            let mut pick = self.scatter.rnd() * total;
            let mut tri = tris[0].0;
            for (t, a) in &tris {
                if pick <= *a {
                    tri = *t;
                    break;
                }
                pick -= a;
            }
            let (mut u, mut v) = (self.scatter.rnd(), self.scatter.rnd());
            if u + v > 1.0 {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            let p0 = tri[0] + (tri[1] - tri[0]) * u + (tri[2] - tri[0]) * v;
            // auf die sichtbare Oberfläche (von oben) projizieren
            let Some((p, n)) = self.surface_at(p0 + DVec3::Z * 64.0, -DVec3::Z) else { continue };
            if n.z < max_slope || have.iter().any(|q| (*q - p).length() < self.scatter.min_dist) {
                continue;
            }
            if self.scatter_place(p, n).is_some() {
                have.push(p);
                placed += 1;
            }
        }
        self.status = format!("{} gestreut", crate::i18n::count_de(placed, "Objekt", "Objekte"));
        self.touch();
        placed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Aabb;

    #[test]
    fn fill_and_erase() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let floor = ed.map.make_box(Aabb::new(DVec3::new(0.0, 0.0, -32.0), DVec3::new(1024.0, 1024.0, 0.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
        let fid = floor.id;
        ed.map.world.solids.push(crate::map::Shared::new(floor));
        ed.sel.insert(Obj::Solid(fid));
        ed.scatter.models = vec!["models/props_foliage/tree_pine04.mdl".into()];
        ed.scatter.density = 4.0;
        ed.scatter.min_dist = 32.0;
        let n = ed.scatter_fill_selection();
        // 1024² = 16 Kacheln à 256² → etwa 64 Objekte
        assert!((48..=64).contains(&n), "{n}");
        assert!(ed.map.entities.iter().all(|e| e.origin().is_some_and(|o| o.z.abs() < 0.5 && (0.0..=1024.0).contains(&o.x))));
        // Mindestabstand eingehalten
        let pts: Vec<DVec3> = ed.map.entities.iter().filter_map(|e| e.origin()).collect();
        for (i, a) in pts.iter().enumerate() {
            for b in &pts[i + 1..] {
                assert!((*a - *b).length() >= 32.0 - 1e-6);
            }
        }
        // Radieren in der Mitte
        let before = ed.map.entities.len();
        ed.scatter.radius = 200.0;
        ed.scatter_paint(DVec3::new(512.0, 512.0, 500.0), -DVec3::Z, true, true);
        assert!(ed.map.entities.len() < before);
        // Malen füllt wieder auf
        let mid = ed.map.entities.len();
        for k in 0..10 {
            ed.scatter_paint(DVec3::new(512.0, 512.0, 500.0), -DVec3::Z, k == 0, false);
        }
        assert!(ed.map.entities.len() > mid);
    }
}
