//! Grundformen des Block-Werkzeugs (wie in Hammer): Block, Keil, Zylinder, Spitze, Kugel, Bogen, Torus.

use crate::geometry::Aabb;
use crate::map::{Map, Solid};
use glam::DVec3;
use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Block,
    Wedge,
    Cylinder,
    Spike,
    Sphere,
    Arch,
    Torus,
    Stairs,
    SpiralStairs,
    Ramp,
    Pipe,
    Dome,
    Fence,
}

impl Shape {
    pub const ALL: [Shape; 13] = [
        Shape::Block,
        Shape::Wedge,
        Shape::Cylinder,
        Shape::Spike,
        Shape::Sphere,
        Shape::Arch,
        Shape::Torus,
        Shape::Stairs,
        Shape::SpiralStairs,
        Shape::Ramp,
        Shape::Pipe,
        Shape::Dome,
        Shape::Fence,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Block => crate::i18n::tr("Block"),
            Shape::Wedge => crate::i18n::tr("Keil"),
            Shape::Cylinder => crate::i18n::tr("Zylinder"),
            Shape::Spike => crate::i18n::tr("Spitze"),
            Shape::Sphere => crate::i18n::tr("Kugel"),
            Shape::Arch => crate::i18n::tr("Bogen"),
            Shape::Torus => crate::i18n::tr("Torus"),
            Shape::Stairs => crate::i18n::tr("Treppe"),
            Shape::SpiralStairs => crate::i18n::tr("Wendeltreppe"),
            Shape::Ramp => crate::i18n::tr("Rampe"),
            Shape::Pipe => crate::i18n::tr("Rohr"),
            Shape::Dome => crate::i18n::tr("Kuppel"),
            Shape::Fence => crate::i18n::tr("Zaun/Geländer"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShapeParams {
    pub shape: Shape,
    /// Seitenzahl (Zylinder, Spitze, Kugel, Bogen-Segmente, Torus-Ring)
    pub sides: u32,
    /// Bogen/Torus: Wandstärke bzw. Rohrdicke
    pub wall: f64,
    /// Bogen/Torus: Öffnungswinkel in Grad
    pub arc: f64,
    /// Bogen/Torus: Startwinkel in Grad
    pub start_angle: f64,
    /// Bogen/Torus: zusätzliche Höhe pro Segment (Wendel)
    pub add_height: f64,
    /// Torus: Seiten des Rohrquerschnitts
    pub cross_sides: u32,
}

impl Default for ShapeParams {
    fn default() -> Self {
        ShapeParams { shape: Shape::Block, sides: 8, wall: 32.0, arc: 360.0, start_angle: 0.0, add_height: 0.0, cross_sides: 8 }
    }
}

fn ellipse(c: DVec3, rx: f64, ry: f64, a: f64) -> DVec3 {
    DVec3::new(c.x + rx * a.cos(), c.y + ry * a.sin(), c.z)
}

/// Punktwolken der einzelnen Brushes einer Form
pub fn point_sets(b: Aabb, p: &ShapeParams) -> Vec<Vec<DVec3>> {
    let (mn, mx) = (b.min, b.max);
    let c = DVec3::new((mn.x + mx.x) / 2.0, (mn.y + mx.y) / 2.0, mn.z);
    let (rx, ry) = ((mx.x - mn.x) / 2.0, (mx.y - mn.y) / 2.0);
    let h = mx.z - mn.z;
    let n = p.sides.clamp(3, 64) as usize;
    // Halber Schritt Versatz, damit bei 4/8 Seiten flache Seiten an den Achsen liegen
    let ang = |i: usize, count: usize| TAU * i as f64 / count as f64 + std::f64::consts::PI / count as f64;
    match p.shape {
        Shape::Block => vec![b.corners().to_vec()],
        Shape::Wedge => vec![vec![
            DVec3::new(mn.x, mn.y, mn.z),
            DVec3::new(mx.x, mn.y, mn.z),
            DVec3::new(mx.x, mx.y, mn.z),
            DVec3::new(mn.x, mx.y, mn.z),
            DVec3::new(mn.x, mx.y, mx.z),
            DVec3::new(mx.x, mx.y, mx.z),
        ]],
        Shape::Cylinder => {
            let mut pts = Vec::new();
            for i in 0..n {
                let e = ellipse(c, rx, ry, ang(i, n));
                pts.push(e);
                pts.push(e + DVec3::Z * h);
            }
            vec![pts]
        }
        Shape::Spike => {
            let mut pts: Vec<DVec3> = (0..n).map(|i| ellipse(c, rx, ry, ang(i, n))).collect();
            pts.push(c + DVec3::Z * h);
            vec![pts]
        }
        Shape::Sphere => {
            let n = n.min(24);
            let rings = (n / 2).max(2);
            let center = DVec3::new(c.x, c.y, mn.z + h / 2.0);
            let rz = h / 2.0;
            let mut pts = vec![center + DVec3::Z * rz, center - DVec3::Z * rz];
            for r in 1..rings {
                let phi = std::f64::consts::PI * r as f64 / rings as f64;
                let (s, co) = phi.sin_cos();
                for i in 0..n {
                    let a = ang(i, n);
                    pts.push(center + DVec3::new(rx * s * a.cos(), ry * s * a.sin(), rz * co));
                }
            }
            vec![pts]
        }
        Shape::Arch => {
            let wall = p.wall.max(1.0);
            let arc = p.arc.clamp(1.0, 360.0).to_radians();
            let start = p.start_angle.to_radians();
            (0..n)
                .map(|i| {
                    let a0 = start + arc * i as f64 / n as f64;
                    let a1 = start + arc * (i + 1) as f64 / n as f64;
                    let z = mn.z + p.add_height * i as f64;
                    let mut pts = Vec::new();
                    for a in [a0, a1] {
                        for (rxx, ryy) in [(rx, ry), ((rx - wall).max(0.5), (ry - wall).max(0.5))] {
                            let e = DVec3::new(c.x + rxx * a.cos(), c.y + ryy * a.sin(), z);
                            pts.push(e);
                            pts.push(e + DVec3::Z * h);
                        }
                    }
                    pts
                })
                .collect()
        }
        Shape::Stairs | Shape::Ramp => {
            // Richtung über den Startwinkel: 0 = +X, 90 = +Y, 180 = -X, 270 = -Y
            let dir = ((p.start_angle.rem_euclid(360.0) + 45.0) / 90.0) as usize % 4;
            let along_x = dir % 2 == 0;
            let positive = dir < 2;
            let (a0, a1) = if along_x { (mn.x, mx.x) } else { (mn.y, mx.y) };
            let len = a1 - a0;
            let make = |s0: f64, s1: f64, z: f64| -> Vec<DVec3> {
                let (lo, hi) = if positive { (a0 + s0, a0 + s1) } else { (a1 - s1, a1 - s0) };
                let b = if along_x { Aabb::new(DVec3::new(lo, mn.y, mn.z), DVec3::new(hi, mx.y, mn.z + z)) } else { Aabb::new(DVec3::new(mn.x, lo, mn.z), DVec3::new(mx.x, hi, mn.z + z)) };
                b.corners().to_vec()
            };
            if p.shape == Shape::Ramp {
                // Keil, der in Richtung `dir` ansteigt
                let mut pts = Vec::new();
                for c in Aabb::new(mn, DVec3::new(mx.x, mx.y, mn.z)).corners().iter().take(4) {
                    pts.push(*c);
                }
                let high = |x: f64, y: f64| DVec3::new(x, y, mx.z);
                match (along_x, positive) {
                    (true, true) => pts.extend([high(mx.x, mn.y), high(mx.x, mx.y)]),
                    (true, false) => pts.extend([high(mn.x, mn.y), high(mn.x, mx.y)]),
                    (false, true) => pts.extend([high(mn.x, mx.y), high(mx.x, mx.y)]),
                    (false, false) => pts.extend([high(mn.x, mn.y), high(mx.x, mn.y)]),
                }
                return vec![pts];
            }
            let steps = n;
            (0..steps).map(|i| make(len * i as f64 / steps as f64, len * (i + 1) as f64 / steps as f64, h * (i + 1) as f64 / steps as f64)).collect()
        }
        Shape::SpiralStairs => {
            let wall = p.wall.max(8.0);
            let arc = p.arc.clamp(10.0, 1440.0).to_radians();
            let start = p.start_angle.to_radians();
            let step_h = h / n as f64;
            (0..n)
                .map(|i| {
                    let a0 = start + arc * i as f64 / n as f64;
                    let a1 = start + arc * (i + 1) as f64 / n as f64;
                    let z0 = mn.z + step_h * i as f64;
                    let mut pts = Vec::new();
                    for a in [a0, a1] {
                        for (rxx, ryy) in [(rx, ry), ((rx - wall).max(1.0), (ry - wall).max(1.0))] {
                            let e = DVec3::new(c.x + rxx * a.cos(), c.y + ryy * a.sin(), z0);
                            pts.push(e);
                            pts.push(e + DVec3::Z * step_h);
                        }
                    }
                    pts
                })
                .collect()
        }
        Shape::Pipe => {
            let q = ShapeParams { shape: Shape::Arch, arc: 360.0, start_angle: 0.0, add_height: 0.0, ..*p };
            point_sets(b, &q)
        }
        Shape::Dome => {
            let n = n.min(24);
            let rings = (n / 4).max(2);
            let mut pts: Vec<DVec3> = (0..n).map(|i| ellipse(c, rx, ry, ang(i, n))).collect();
            pts.push(c + DVec3::Z * h);
            for r in 1..rings {
                let phi = std::f64::consts::FRAC_PI_2 * r as f64 / rings as f64;
                let (s, co) = phi.sin_cos();
                for i in 0..n {
                    let a = ang(i, n);
                    pts.push(DVec3::new(c.x + rx * co * a.cos(), c.y + ry * co * a.sin(), mn.z + h * s));
                }
            }
            vec![pts]
        }
        Shape::Fence => {
            let posts = p.sides.clamp(2, 64) as usize;
            let t = p.wall.clamp(1.0, 256.0);
            let along_x = (mx.x - mn.x) >= (mx.y - mn.y);
            let (a0, a1) = if along_x { (mn.x, mx.x) } else { (mn.y, mx.y) };
            let mid = if along_x { (mn.y + mx.y) / 2.0 } else { (mn.x + mx.x) / 2.0 };
            let boxed = |s0: f64, s1: f64, w: f64, z0: f64, z1: f64| -> Vec<DVec3> {
                let b = if along_x { Aabb::new(DVec3::new(s0, mid - w / 2.0, z0), DVec3::new(s1, mid + w / 2.0, z1)) } else { Aabb::new(DVec3::new(mid - w / 2.0, s0, z0), DVec3::new(mid + w / 2.0, s1, z1)) };
                b.corners().to_vec()
            };
            let mut out = Vec::new();
            for i in 0..posts {
                let pos = a0 + (a1 - a0 - t) * i as f64 / (posts - 1) as f64;
                out.push(boxed(pos, pos + t, t, mn.z, mx.z));
            }
            let rails = p.cross_sides.clamp(1, 8) as usize;
            for r in 0..rails {
                let z = mn.z + h * (r + 1) as f64 / (rails + 1) as f64;
                out.push(boxed(a0 + t, a1 - t, t / 2.0, z - t / 4.0, z + t / 4.0));
            }
            if rails > 0 {
                // Handlauf oben
                out.push(boxed(a0, a1, t, mx.z, mx.z + t / 2.0));
            }
            out
        }
        Shape::Torus => {
            let arc = p.arc.clamp(1.0, 360.0).to_radians();
            let start = p.start_angle.to_radians();
            let tube = (p.wall / 2.0).min(rx.min(ry) / 2.0).max(1.0);
            let m = p.cross_sides.clamp(3, 32) as usize;
            let zc = mn.z + h / 2.0;
            let tube_z = (h / 2.0).max(1.0);
            (0..n)
                .map(|i| {
                    let mut pts = Vec::new();
                    for a in [start + arc * i as f64 / n as f64, start + arc * (i + 1) as f64 / n as f64] {
                        let ring = DVec3::new(c.x + (rx - tube) * a.cos(), c.y + (ry - tube) * a.sin(), zc + p.add_height * i as f64);
                        let radial = DVec3::new(a.cos(), a.sin(), 0.0);
                        for j in 0..m {
                            let b = ang(j, m);
                            pts.push(ring + radial * (tube * b.cos()) + DVec3::Z * (tube_z * b.sin()));
                        }
                    }
                    pts
                })
                .collect()
        }
    }
}

/// Erzeugt die Brushes einer Form
pub fn create(map: &mut Map, b: Aabb, p: &ShapeParams, material: &str, tex_scale: f64) -> Vec<Solid> {
    point_sets(b, p).into_iter().filter_map(|pts| map.solid_from_points(&pts, material, tex_scale)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_shapes_valid() {
        let mut map = Map::new();
        let b = Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, 128.0));
        for s in Shape::ALL {
            let p = ShapeParams { shape: s, ..Default::default() };
            let solids = create(&mut map, b, &p, "a", 0.25);
            assert!(!solids.is_empty(), "{s:?}");
            assert!(solids.iter().all(|x| x.is_valid()), "{s:?}");
            let expected = match s {
                Shape::Arch | Shape::Torus | Shape::Stairs | Shape::SpiralStairs | Shape::Pipe => 8,
                Shape::Fence => 8 + 8 + 1,
                _ => 1,
            };
            assert_eq!(solids.len(), expected, "{s:?}");
        }
        let cyl = create(&mut map, b, &ShapeParams { shape: Shape::Cylinder, sides: 12, ..Default::default() }, "a", 0.25);
        assert_eq!(cyl[0].sides.len(), 14);
    }
}
