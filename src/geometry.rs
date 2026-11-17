//! Brush-Geometrie: Ebenen, Polygone, AABBs, Strahltests.

use glam::{DVec2, DVec3};

pub const EPS: f64 = 0.001;
const HUGE: f64 = 131072.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: DVec3,
    pub dist: f64,
}

impl Plane {
    /// Source-Konvention (wie VBSP): normal = (p0 - p1) x (p2 - p1), zeigt nach außen.
    pub fn from_points(p: &[DVec3; 3]) -> Option<Plane> {
        let n = (p[0] - p[1]).cross(p[2] - p[1]);
        let len = n.length();
        if len < 1e-9 {
            return None;
        }
        let normal = n / len;
        Some(Plane { normal, dist: normal.dot(p[0]) })
    }

    pub fn distance(&self, p: DVec3) -> f64 {
        self.normal.dot(p) - self.dist
    }
}

/// Wählt drei Punkte eines Polygons so, dass `Plane::from_points` die gewünschte Normale liefert.
pub fn plane_points_for(normal: DVec3, poly: &[DVec3]) -> Option<[DVec3; 3]> {
    if poly.len() < 3 {
        return None;
    }
    // Drei Punkte mit möglichst großem Dreieck wählen (numerisch stabil)
    let a = poly[0];
    let mut best = (0.0, 1, 2);
    for i in 1..poly.len() {
        for j in (i + 1)..poly.len() {
            let area = (poly[i] - a).cross(poly[j] - a).length();
            if area > best.0 {
                best = (area, i, j);
            }
        }
    }
    let (_, i, j) = best;
    let pts = [a, poly[i], poly[j]];
    let pl = Plane::from_points(&pts)?;
    if pl.normal.dot(normal) > 0.0 {
        Some(pts)
    } else {
        Some([pts[2], pts[1], pts[0]])
    }
}

/// Großes Quadrat auf der Ebene, Eckpunkte gegen den Uhrzeigersinn um die Normale.
pub fn base_winding(plane: &Plane) -> Vec<DVec3> {
    let n = plane.normal;
    let up = if n.z.abs() > 0.9 { DVec3::X } else { DVec3::Z };
    let u = up.cross(n).normalize();
    let v = n.cross(u);
    let c = n * plane.dist;
    vec![
        c + (-u - v) * HUGE,
        c + (u - v) * HUGE,
        c + (u + v) * HUGE,
        c + (-u + v) * HUGE,
    ]
}

/// Schneidet ein Polygon an einer Ebene und behält die Rückseite (Innenseite).
pub fn clip_polygon(poly: &[DVec3], plane: &Plane) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    if poly.is_empty() {
        return out;
    }
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let da = plane.distance(a);
        let db = plane.distance(b);
        if da <= EPS {
            out.push(a);
        }
        if (da > EPS && db < -EPS) || (da < -EPS && db > EPS) {
            let t = da / (da - db);
            out.push(a + (b - a) * t);
        }
    }
    out
}

fn snap_coord(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() < 0.01 {
        r
    } else {
        v
    }
}

pub fn snap_vec(v: DVec3) -> DVec3 {
    DVec3::new(snap_coord(v.x), snap_coord(v.y), snap_coord(v.z))
}

/// Berechnet für jede Ebene das zugehörige Polygon des konvexen Körpers.
pub fn polygons_from_planes(planes: &[Option<Plane>]) -> Vec<Vec<DVec3>> {
    planes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let Some(p) = p else { return Vec::new() };
            let mut poly = base_winding(p);
            for (j, other) in planes.iter().enumerate() {
                if i == j {
                    continue;
                }
                let Some(o) = other else { continue };
                // identische Ebenen überspringen
                if o.normal.dot(p.normal) > 0.9999 && (o.dist - p.dist).abs() < EPS {
                    if j < i {
                        return Vec::new();
                    }
                    continue;
                }
                poly = clip_polygon(&poly, o);
                if poly.len() < 3 {
                    return Vec::new();
                }
            }
            let mut out: Vec<DVec3> = Vec::with_capacity(poly.len());
            for v in poly.into_iter().map(snap_vec) {
                if out.last().map_or(true, |l: &DVec3| l.distance(v) > 0.01) {
                    out.push(v);
                }
            }
            if out.len() > 1 && out[0].distance(*out.last().unwrap()) <= 0.01 {
                out.pop();
            }
            if out.len() < 3 {
                Vec::new()
            } else {
                out
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb { min: DVec3::splat(f64::MAX), max: DVec3::splat(f64::MIN) };

    pub fn new(a: DVec3, b: DVec3) -> Self {
        Aabb { min: a.min(b), max: a.max(b) }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    pub fn add_point(&mut self, p: DVec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb { min: self.min.min(o.min), max: self.max.max(o.max) }
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    pub fn ray_hit(&self, origin: DVec3, dir: DVec3) -> Option<f64> {
        let mut tmin = 0.0f64;
        let mut tmax = f64::MAX;
        for a in 0..3 {
            let o = origin[a];
            let d = dir[a];
            if d.abs() < 1e-12 {
                if o < self.min[a] || o > self.max[a] {
                    return None;
                }
            } else {
                let t1 = (self.min[a] - o) / d;
                let t2 = (self.max[a] - o) / d;
                tmin = tmin.max(t1.min(t2));
                tmax = tmax.min(t1.max(t2));
                if tmin > tmax {
                    return None;
                }
            }
        }
        Some(tmin)
    }
}

/// Strahl gegen konvexen Körper (Ebenen zeigen nach außen).
/// Liefert (t, Index der getroffenen Ebene).
pub fn ray_convex(planes: &[Option<Plane>], origin: DVec3, dir: DVec3) -> Option<(f64, usize)> {
    let mut tmin = 0.0f64;
    let mut tmax = f64::MAX;
    let mut hit_plane = None;
    for (i, p) in planes.iter().enumerate() {
        let Some(p) = p else { continue };
        let denom = p.normal.dot(dir);
        let dist = p.distance(origin);
        if denom.abs() < 1e-12 {
            if dist > 0.0 {
                return None;
            }
            continue;
        }
        let t = -dist / denom;
        if denom < 0.0 {
            if t > tmin {
                tmin = t;
                hit_plane = Some(i);
            }
        } else if t < tmax {
            tmax = t;
        }
        if tmin > tmax {
            return None;
        }
    }
    hit_plane.map(|i| (tmin, i))
}

/// Ansichtsachsen der 2D-Fenster (wie Hammer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Axis2D {
    /// Top: X nach rechts, Y nach oben
    Top,
    /// Front: Y nach rechts, Z nach oben
    Front,
    /// Side: X nach rechts, Z nach oben
    Side,
}

impl Axis2D {
    /// (horizontale Achse, vertikale Achse, Tiefenachse)
    pub fn axes(self) -> (usize, usize, usize) {
        match self {
            Axis2D::Top => (0, 1, 2),
            Axis2D::Front => (1, 2, 0),
            Axis2D::Side => (0, 2, 1),
        }
    }

    pub fn project(self, p: DVec3) -> DVec2 {
        let (h, v, _) = self.axes();
        DVec2::new(p[h], p[v])
    }

    pub fn name(self) -> &'static str {
        match self {
            Axis2D::Top => crate::i18n::tr("Oben · X/Y"),
            Axis2D::Front => crate::i18n::tr("Vorn · Y/Z"),
            Axis2D::Side => crate::i18n::tr("Seite · X/Z"),
        }
    }
}

pub fn snap(v: f64, grid: f64) -> f64 {
    (v / grid).round() * grid
}

pub fn snap_vec_grid(v: DVec3, grid: f64) -> DVec3 {
    DVec3::new(snap(v.x, grid), snap(v.y, grid), snap(v.z, grid))
}

impl Aabb {
    pub fn intersects(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x && self.max.x >= o.min.x && self.min.y <= o.max.y && self.max.y >= o.min.y && self.min.z <= o.max.z && self.max.z >= o.min.z
    }

    pub fn contains_point(&self, p: DVec3) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y && p.z >= self.min.z && p.z <= self.max.z
    }

    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(b.x, b.y, b.z),
            DVec3::new(a.x, b.y, b.z),
        ]
    }

    pub fn from_points(pts: impl IntoIterator<Item = DVec3>) -> Aabb {
        let mut b = Aabb::EMPTY;
        for p in pts {
            b.add_point(p);
        }
        b
    }
}

/// Ebenen der konvexen Hülle einer Punktmenge (nach außen gerichtet).
/// Brute-Force mit frühem Abbruch – für Brushes (bis ein paar hundert Punkte) schnell genug.
pub fn hull_planes(points: &[DVec3]) -> Vec<Plane> {
    let mut pts: Vec<DVec3> = Vec::new();
    for p in points {
        if !pts.iter().any(|q| q.distance(*p) < 0.01) {
            pts.push(*p);
        }
    }
    let n = pts.len();
    let mut planes: Vec<Plane> = Vec::new();
    if n < 4 {
        return planes;
    }
    let eps = 0.01;
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                let nrm = (pts[j] - pts[i]).cross(pts[k] - pts[i]);
                let len = nrm.length();
                if len < 1e-6 {
                    continue;
                }
                let nrm = nrm / len;
                let d = nrm.dot(pts[i]);
                let mut pos = false;
                let mut neg = false;
                for p in &pts {
                    let dist = nrm.dot(*p) - d;
                    if dist > eps {
                        pos = true;
                    } else if dist < -eps {
                        neg = true;
                    }
                    if pos && neg {
                        break;
                    }
                }
                if pos && neg {
                    continue;
                }
                if !pos && !neg {
                    // alle Punkte koplanar → keine Hülle
                    return Vec::new();
                }
                let plane = if pos { Plane { normal: -nrm, dist: -d } } else { Plane { normal: nrm, dist: d } };
                if !planes.iter().any(|q| q.normal.dot(plane.normal) > 0.99999 && (q.dist - plane.dist).abs() < eps) {
                    planes.push(plane);
                }
            }
        }
    }
    planes
}

#[cfg(test)]
mod hull_tests {
    use super::*;

    #[test]
    fn hull_of_cube_and_pyramid() {
        let cube = Aabb::new(DVec3::ZERO, DVec3::splat(64.0)).corners();
        assert_eq!(hull_planes(&cube).len(), 6);
        let pyr = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0), DVec3::new(0.0, 64.0, 0.0), DVec3::new(32.0, 32.0, 64.0)];
        let planes = hull_planes(&pyr);
        assert_eq!(planes.len(), 5);
        // Mittelpunkt liegt innen
        let c = DVec3::new(32.0, 32.0, 10.0);
        assert!(planes.iter().all(|p| p.distance(c) < 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_orientation_matches_hammer() {
        // Deckfläche eines Hammer-Würfels
        let p = Plane::from_points(&[
            DVec3::new(-32.0, 32.0, 64.0),
            DVec3::new(32.0, 32.0, 64.0),
            DVec3::new(32.0, -32.0, 64.0),
        ])
        .unwrap();
        assert!((p.normal - DVec3::Z).length() < 1e-9);
        assert!((p.dist - 64.0).abs() < 1e-9);
    }

    #[test]
    fn cube_polygons() {
        let planes: Vec<Option<Plane>> = [
            (DVec3::X, 1.0),
            (-DVec3::X, 1.0),
            (DVec3::Y, 1.0),
            (-DVec3::Y, 1.0),
            (DVec3::Z, 1.0),
            (-DVec3::Z, 1.0),
        ]
        .iter()
        .map(|(n, d)| Some(Plane { normal: *n, dist: *d }))
        .collect();
        let polys = polygons_from_planes(&planes);
        for (poly, pl) in polys.iter().zip(&planes) {
            assert_eq!(poly.len(), 4);
            let pts = plane_points_for(pl.unwrap().normal, poly).unwrap();
            let rebuilt = Plane::from_points(&pts).unwrap();
            assert!(rebuilt.normal.dot(pl.unwrap().normal) > 0.999);
        }
        let hit = ray_convex(&planes, DVec3::new(-10.0, 0.0, 0.0), DVec3::X).unwrap();
        assert!((hit.0 - 9.0).abs() < 1e-9);
        assert_eq!(hit.1, 1);
    }
}
