//! Platzierung mit Physik: ausgewählte Props fallen als Starrkörper (Box aus den Modellmaßen) auf
//! Brushes, Displacements und einander, rutschen, kippen und kommen zur Ruhe. Einfache impulsbasierte
//! Simulation (Kontaktpunkte an Ecken, Kanten und Flächen der Box), Einheiten wie in Source (Zoll,
//! Schwerkraft 600 u/s²).

use crate::geometry::{Aabb, Plane};
use crate::map::Map;
use crate::transform::{angles_to_basis, basis_to_angles};
use glam::{DMat3, DQuat, DVec3};

const GRAVITY: f64 = 600.0;
const DT: f64 = 1.0 / 120.0;
const RESTITUTION: f64 = 0.15;
const FRICTION: f64 = 0.6;
const MAX_TIME: f64 = 10.0;

/// Ein fallender Körper
#[derive(Clone, Debug)]
pub struct Body {
    pub ent: u32,
    pos: DVec3,
    rot: DQuat,
    vel: DVec3,
    ang: DVec3,
    half: DVec3,
    /// Mittelpunkt der Box im Modellraum (Ursprung des Props ≠ Mitte)
    center: DVec3,
    inv_m: f64,
    inv_i: DVec3,
    calm: u32,
    pub asleep: bool,
}

struct Convex {
    bounds: Aabb,
    planes: Vec<Plane>,
}

struct STri {
    a: DVec3,
    b: DVec3,
    c: DVec3,
    n: DVec3,
    bounds: Aabb,
}

pub struct Sim {
    pub bodies: Vec<Body>,
    convex: Vec<Convex>,
    tris: Vec<STri>,
    pub time: f64,
    floor_z: f64,
}

/// Brush blockiert Props? (Trigger, Hint, Areaportals usw. nicht)
fn blocks(mats: &[&str]) -> bool {
    !mats.iter().all(|m| {
        let m = m.to_ascii_lowercase();
        ["toolstrigger", "toolshint", "toolsskip", "toolsareaportal", "toolsplayerclip", "toolsnpcclip", "toolsoccluder", "toolsfog", "toolsskybox"].iter().any(|t| m.contains(t))
    })
}

/// 26 Kontaktpunkte einer Box (Ecken, Kantenmitten, Flächenmitten) in Körperkoordinaten
fn sample_points(h: DVec3) -> Vec<DVec3> {
    let mut v = Vec::with_capacity(26);
    for x in [-1.0, 0.0, 1.0] {
        for y in [-1.0, 0.0, 1.0] {
            for z in [-1.0, 0.0, 1.0] {
                if x == 0.0 && y == 0.0 && z == 0.0 {
                    continue;
                }
                v.push(DVec3::new(x * h.x, y * h.y, z * h.z));
            }
        }
    }
    v
}

struct Contact {
    a: usize,
    /// zweiter Körper oder None = Welt
    b: Option<usize>,
    p: DVec3,
    /// zeigt von B (bzw. der Welt) zu A
    n: DVec3,
    depth: f64,
}

impl Sim {
    /// `props`: (Entity, Ursprung, Winkel, Modell-Min, Modell-Max); `ignore`: diese Entities sind keine Hindernisse
    pub fn new(map: &Map, props: &[(u32, DVec3, DVec3, DVec3, DVec3)]) -> Sim {
        let mut convex = Vec::new();
        let mut tris = Vec::new();
        let mut floor_z = f64::MAX;
        for (owner, s) in map.solids_owned() {
            if let Some(e) = owner {
                let c = e.classname();
                if c.starts_with("trigger_") || c.starts_with("func_areaportal") || c == "func_illusionary" || c == "func_occluder" || c == "func_ladder" || c == "func_viscluster" {
                    continue;
                }
            }
            let mats: Vec<&str> = s.sides.iter().map(|x| x.material.as_str()).collect();
            if !blocks(&mats) {
                continue;
            }
            floor_z = floor_z.min(s.bounds.min.z);
            if s.has_disp() {
                for (i, g) in s.disps.iter().enumerate() {
                    let (Some(g), Some(Some(pl))) = (g, s.planes.get(i)) else { continue };
                    for t in g.triangles() {
                        let (a, b, c) = (g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]);
                        let mut n = (b - a).cross(c - a).normalize_or_zero();
                        if n.dot(pl.normal) < 0.0 {
                            n = -n;
                        }
                        let mut bounds = Aabb::new(a, b);
                        bounds = bounds.union(&Aabb::new(c, c));
                        tris.push(STri { a, b, c, n, bounds });
                    }
                }
            } else {
                convex.push(Convex { bounds: s.bounds, planes: s.planes.iter().flatten().copied().collect() });
            }
        }
        let bodies = props
            .iter()
            .map(|&(ent, origin, angles, mn, mx)| {
                let half = ((mx - mn) * 0.5).max(DVec3::splat(2.0));
                let center = (mn + mx) * 0.5;
                let (f, l, u) = angles_to_basis(angles);
                let rot = DQuat::from_mat3(&DMat3::from_cols(f, l, u)).normalize();
                let m = (half.x * half.y * half.z * 8.0 / 1000.0).clamp(1.0, 5000.0);
                let ix = m / 3.0 * (half.y * half.y + half.z * half.z);
                let iy = m / 3.0 * (half.x * half.x + half.z * half.z);
                let iz = m / 3.0 * (half.x * half.x + half.y * half.y);
                Body { ent, pos: origin + rot * center, rot, vel: DVec3::ZERO, ang: DVec3::ZERO, half, center, inv_m: 1.0 / m, inv_i: DVec3::new(1.0 / ix, 1.0 / iy, 1.0 / iz), calm: 0, asleep: false }
            })
            .collect();
        Sim { bodies, convex, tris, time: 0.0, floor_z: if floor_z == f64::MAX { -16384.0 } else { floor_z } }
    }

    pub fn finished(&self) -> bool {
        self.time >= MAX_TIME || self.bodies.iter().all(|b| b.asleep)
    }

    /// Ursprung und Winkel je Entity (für die Map)
    pub fn result(&self) -> Vec<(u32, DVec3, DVec3)> {
        self.bodies
            .iter()
            .map(|b| {
                let m = DMat3::from_quat(b.rot);
                let ang = basis_to_angles(m.x_axis, m.y_axis, m.z_axis);
                (b.ent, b.pos - b.rot * b.center, ang)
            })
            .collect()
    }

    fn inv_inertia_world(b: &Body) -> DMat3 {
        let r = DMat3::from_quat(b.rot);
        r * DMat3::from_diagonal(b.inv_i) * r.transpose()
    }

    fn contacts(&self) -> Vec<Contact> {
        let mut out = Vec::new();
        for (ai, a) in self.bodies.iter().enumerate() {
            if a.asleep {
                continue;
            }
            let pts: Vec<DVec3> = sample_points(a.half).into_iter().map(|q| a.pos + a.rot * q).collect();
            let reach = a.half.length() + 2.0;
            let abox = Aabb::new(a.pos - DVec3::splat(reach), a.pos + DVec3::splat(reach));
            // Welt: konvexe Brushes
            for c in self.convex.iter().filter(|c| c.bounds.intersects(&abox)) {
                for &p in &pts {
                    let mut best: Option<(f64, DVec3)> = None;
                    let mut inside = true;
                    for pl in &c.planes {
                        let d = pl.distance(p);
                        if d > 0.05 {
                            inside = false;
                            break;
                        }
                        if best.is_none_or(|(bd, _)| d > bd) {
                            best = Some((d, pl.normal));
                        }
                    }
                    if inside {
                        if let Some((d, n)) = best {
                            out.push(Contact { a: ai, b: None, p, n, depth: -d });
                        }
                    }
                }
            }
            // Welt: Displacement-Oberflächen
            for t in self.tris.iter().filter(|t| t.bounds.intersects(&abox)) {
                for &p in &pts {
                    let sd = t.n.dot(p - t.a);
                    if !(-24.0..0.05).contains(&sd) {
                        continue;
                    }
                    let q = p - t.n * sd;
                    if in_tri(q, t.a, t.b, t.c, t.n) {
                        out.push(Contact { a: ai, b: None, p, n: t.n, depth: -sd });
                    }
                }
            }
}
        // Körper untereinander: Normale über Trennachsen (Flächenachsen beider Boxen), Punkte beider Seiten
        for i in 0..self.bodies.len() {
            for j in i + 1..self.bodies.len() {
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                if (a.asleep && b.asleep) || (a.pos - b.pos).length() > a.half.length() + b.half.length() + 2.0 {
                    continue;
                }
                let (ma, mb) = (DMat3::from_quat(a.rot), DMat3::from_quat(b.rot));
                let axes = [ma.x_axis, ma.y_axis, ma.z_axis, mb.x_axis, mb.y_axis, mb.z_axis];
                let d = a.pos - b.pos;
                let mut best: Option<(f64, DVec3)> = None;
                for l in axes {
                    let ra = a.half.x * ma.x_axis.dot(l).abs() + a.half.y * ma.y_axis.dot(l).abs() + a.half.z * ma.z_axis.dot(l).abs();
                    let rb = b.half.x * mb.x_axis.dot(l).abs() + b.half.y * mb.y_axis.dot(l).abs() + b.half.z * mb.z_axis.dot(l).abs();
                    let overlap = ra + rb - d.dot(l).abs();
                    if overlap <= 0.0 {
                        best = None;
                        break;
                    }
                    if best.is_none_or(|(o, _)| overlap < o) {
                        best = Some((overlap, if d.dot(l) >= 0.0 { l } else { -l }));
                    }
                }
                let Some((depth, n)) = best else { continue };
                // der wache Körper ist immer „a“ (schlafende gelten als fest)
                let (i, j, n) = if self.bodies[i].asleep { (j, i, -n) } else { (i, j, n) };
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                let inside = |p: DVec3, o: &Body| {
                    let q = o.rot.inverse() * (p - o.pos);
                    (o.half - q.abs()).min_element() > 0.0
                };
                for q in sample_points(a.half) {
                    let p = a.pos + a.rot * q;
                    if inside(p, b) {
                        out.push(Contact { a: i, b: Some(j), p, n, depth });
                    }
                }
                for q in sample_points(b.half) {
                    let p = b.pos + b.rot * q;
                    if inside(p, a) {
                        out.push(Contact { a: i, b: Some(j), p, n, depth });
                    }
                }
            }
        }
        out
    }

    /// Ein Zeitschritt (intern in festen Unterschritten)
    pub fn step(&mut self, frame_dt: f64) {
        let steps = ((frame_dt / DT).round() as usize).clamp(1, 8);
        for _ in 0..steps {
            if self.finished() {
                return;
            }
            self.substep();
        }
    }

    fn substep(&mut self) {
        self.time += DT;
        // schnell fallende Körper wecken schlafende in ihrer Nähe
        let movers: Vec<(DVec3, f64)> = self.bodies.iter().filter(|b| !b.asleep && b.vel.length() > 60.0).map(|b| (b.pos, b.half.length())).collect();
        for b in self.bodies.iter_mut().filter(|b| b.asleep) {
            if movers.iter().any(|(p, r)| (*p - b.pos).length() < r + b.half.length() + 8.0) {
                b.asleep = false;
                b.calm = 0;
            }
        }
        for b in self.bodies.iter_mut().filter(|b| !b.asleep) {
            b.vel.z -= GRAVITY * DT;
        }
        let contacts = self.contacts();
        for _ in 0..6 {
            for c in &contacts {
                self.resolve(c);
            }
        }
        // Eindringen auflösen: je Körper der Mittelwert der Korrekturen seiner Kontakte
        let mut push = vec![(DVec3::ZERO, 0usize); self.bodies.len()];
        for c in &contacts {
            let (ma, mb) = (self.bodies[c.a].inv_m, c.b.map(|b| if self.bodies[b].asleep { 0.0 } else { self.bodies[b].inv_m }).unwrap_or(0.0));
            let share = ma / (ma + mb).max(1e-9);
            push[c.a].0 += c.n * c.depth * share;
            push[c.a].1 += 1;
            if let Some(b) = c.b {
                if mb > 0.0 {
                    push[b].0 -= c.n * c.depth * (1.0 - share);
                    push[b].1 += 1;
                }
            }
        }
        for (b, (p, n)) in self.bodies.iter_mut().zip(push) {
            if n > 0 && !b.asleep {
                let d = p / n as f64 * 0.8;
                b.pos += d.clamp_length_max(4.0);
            }
        }
        for b in self.bodies.iter_mut().filter(|b| !b.asleep) {
            b.pos += b.vel * DT;
            let w = b.ang;
            if w.length_squared() > 1e-12 {
                let dq = DQuat::from_xyzw(w.x, w.y, w.z, 0.0) * b.rot;
                b.rot = DQuat::from_xyzw(b.rot.x + 0.5 * DT * dq.x, b.rot.y + 0.5 * DT * dq.y, b.rot.z + 0.5 * DT * dq.z, b.rot.w + 0.5 * DT * dq.w).normalize();
            }
            b.vel *= 0.999;
            b.ang *= 0.985;
            // zur Ruhe gekommen?
            if b.vel.length() < 6.0 && b.ang.length() < 0.15 {
                b.calm += 1;
                if b.calm > 40 {
                    b.asleep = true;
                    b.vel = DVec3::ZERO;
                    b.ang = DVec3::ZERO;
                }
            } else {
                b.calm = 0;
            }
            // ins Leere gefallen: anhalten
            if b.pos.z < self.floor_z - 2048.0 {
                b.asleep = true;
            }
        }
    }

    fn resolve(&mut self, c: &Contact) {
        let a = &self.bodies[c.a];
        if a.asleep {
            return;
        }
        let b = c.b.map(|i| &self.bodies[i]).filter(|b| !b.asleep);
        let ra = c.p - a.pos;
        let ia = Self::inv_inertia_world(a);
        let (vb, rb, ib, mb) = match b {
            Some(b) => (b.vel + b.ang.cross(c.p - b.pos), c.p - b.pos, Self::inv_inertia_world(b), b.inv_m),
            None => (DVec3::ZERO, DVec3::ZERO, DMat3::ZERO, 0.0),
        };
        let vrel = a.vel + a.ang.cross(ra) - vb;
        let vn = vrel.dot(c.n);
        if vn >= 0.0 {
            return;
        }
        let k = |d: DVec3| a.inv_m + mb + d.dot((ia * ra.cross(d)).cross(ra)) + d.dot((ib * rb.cross(d)).cross(rb));
        let e = if vn < -60.0 { RESTITUTION } else { 0.0 };
        let jn = -(1.0 + e) * vn / k(c.n).max(1e-9);
        let mut imp = c.n * jn;
        let vt = vrel - c.n * vn;
        if vt.length() > 1e-6 {
            let t = vt.normalize();
            let jt = (-vt.length() / k(t).max(1e-9)).max(-FRICTION * jn);
            imp += t * jt;
        }
        let (am, ai_) = (a.inv_m, ia);
        let ba = &mut self.bodies[c.a];
        ba.vel += imp * am;
        ba.ang += ai_ * ra.cross(imp);
        if let Some(bi) = c.b {
            if !self.bodies[bi].asleep {
                let bb = &mut self.bodies[bi];
                bb.vel -= imp * mb;
                bb.ang -= ib * rb.cross(imp);
            }
        }
    }
}

fn in_tri(p: DVec3, a: DVec3, b: DVec3, c: DVec3, n: DVec3) -> bool {
    let e = |u: DVec3, v: DVec3| (v - u).cross(p - u).dot(n) >= -1e-6;
    e(a, b) && e(b, c) && e(c, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sim: &mut Sim) {
        let mut n = 0;
        while !sim.finished() && n < 5000 {
            sim.step(1.0 / 60.0);
            n += 1;
        }
    }

    #[test]
    fn box_lands_on_floor() {
        let mut m = Map::new();
        let floor = m.make_box(Aabb::new(DVec3::new(-512.0, -512.0, -64.0), DVec3::new(512.0, 512.0, 0.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
        m.world.solids.push(crate::map::Shared::new(floor));
        // Kiste 32³ (Modell-Ursprung unten in der Mitte), schräg in 200 Höhe
        let mut sim = Sim::new(&m, &[(7, DVec3::new(0.0, 0.0, 200.0), DVec3::new(30.0, 20.0, 40.0), DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 32.0))]);
        run(&mut sim);
        assert!(sim.finished());
        let (id, o, a) = sim.result()[0];
        assert_eq!(id, 7);
        // liegt auf dem Boden: tiefster Punkt der Box bei z ≈ 0
        let b = &sim.bodies[0];
        let low = sample_points(b.half).into_iter().map(|q| (b.pos + b.rot * q).z).fold(f64::MAX, f64::min);
        assert!(low > -3.0 && low < 3.0, "tiefster Punkt {low}, Ursprung {o:?}");
        // flach liegend: eine Achse der Kiste steht senkrecht
        let m = DMat3::from_quat(b.rot);
        let up = [m.x_axis, m.y_axis, m.z_axis].iter().map(|v| v.z.abs()).fold(0.0, f64::max);
        assert!(up > 0.99, "Winkel {a:?}, senkrechteste Achse {up}");
        assert!(o.x.abs() < 64.0 && o.y.abs() < 64.0, "nicht weggerutscht: {o:?}");
    }

    #[test]
    fn boxes_stack() {
        let mut m = Map::new();
        let floor = m.make_box(Aabb::new(DVec3::new(-512.0, -512.0, -64.0), DVec3::new(512.0, 512.0, 0.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
        m.world.solids.push(crate::map::Shared::new(floor));
        let (mn, mx) = (DVec3::new(-16.0, -16.0, -16.0), DVec3::new(16.0, 16.0, 16.0));
        let mut sim = Sim::new(&m, &[(1, DVec3::new(0.0, 0.0, 40.0), DVec3::ZERO, mn, mx), (2, DVec3::new(2.0, 1.0, 120.0), DVec3::ZERO, mn, mx)]);
        run(&mut sim);
        let r = sim.result();
        // untere Kiste liegt auf dem Boden (Mitte bei 16), obere auf ihr (Mitte bei ~48)
        assert!((r[0].1.z - 16.0).abs() < 4.0, "{:?}", r[0]);
        assert!((r[1].1.z - 48.0).abs() < 6.0, "{:?}", r[1]);
    }
}
