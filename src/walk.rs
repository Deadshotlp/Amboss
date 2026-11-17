//! Begehen-Modus („Im Editor spielen“): mit Spielergröße, Schwerkraft, Stufen und Sprung durch die
//! Map laufen – ohne das Spiel zu starten. Kollision wie in Source: eine Box (32×32×72, geduckt
//! 32×32×36) wird gegen die konvexen Brushes geschoben; Displacements tragen als Boden.

use crate::editor::Editor;
use crate::geometry::Aabb;
use glam::DVec3;

pub const HULL: (f64, f64) = (16.0, 72.0);
pub const DUCK_HEIGHT: f64 = 36.0;
pub const EYE: f64 = 64.0;
pub const DUCK_EYE: f64 = 28.0;
const STEP: f64 = 18.0;
const GRAVITY: f64 = 600.0;
const JUMP: f64 = 200.0;
const WALK: f64 = 200.0;
const RUN: f64 = 400.0;
const EPS: f64 = 0.03125;

/// Konvexer Körper (Ebenen nach außen) mit Hüllquader
#[derive(Clone)]
struct Hull {
    planes: Vec<(DVec3, f64)>,
    bounds: Aabb,
}

#[derive(Clone, Default)]
pub struct World {
    hulls: Vec<Hull>,
    /// Displacement-Dreiecke
    tris: Vec<[DVec3; 3]>,
}

/// Brushes, durch die man im Spiel läuft (Trigger, Hilfsflächen …)
fn passable<'a>(owner: Option<&str>, mut mats: impl Iterator<Item = &'a str>) -> bool {
    if let Some(c) = owner {
        let c = c.to_ascii_lowercase();
        if c.starts_with("trigger_") || matches!(c.as_str(), "func_illusionary" | "func_areaportal" | "func_areaportalwindow" | "func_viscluster" | "func_occluder" | "func_ladder" | "func_buyzone" | "func_bomb_target") {
            return true;
        }
    }
    mats.any(|m| {
        let m = m.to_ascii_lowercase();
        ["toolstrigger", "toolsskip", "toolshint", "toolsareaportal", "toolsoccluder", "toolsnpcclip", "toolsfog", "toolsskybox"].iter().any(|t| m.ends_with(t))
    })
}

impl World {
    pub fn build(ed: &Editor) -> World {
        let mut w = World::default();
        for (owner, s) in ed.map.solids_owned() {
            if !ed.is_visible_solid(owner, s) {
                continue;
            }
            let class = owner.map(|e| e.classname());
            if s.has_disp() {
                for g in s.disps.iter().flatten() {
                    for t in g.triangles() {
                        w.tris.push([g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]]);
                    }
                }
                continue;
            }
            if passable(class, s.sides.iter().map(|x| x.material.as_str())) {
                continue;
            }
            let planes: Vec<(DVec3, f64)> = s.planes.iter().flatten().map(|p| (p.normal, p.dist)).collect();
            if planes.len() >= 4 {
                w.hulls.push(Hull { planes, bounds: s.bounds });
            }
        }
        // Props als Quader (Modellgröße, sonst Entity-Box)
        for e in ed.map.entities.iter() {
            let c = e.classname().to_ascii_lowercase();
            if !(c.starts_with("prop_static") || c.starts_with("prop_physics") || c.starts_with("prop_dynamic")) || !ed.is_visible_entity(e) {
                continue;
            }
            let b = ed.entity_bounds(e);
            if b.is_empty() || b.size().min_element() < 2.0 {
                continue;
            }
            let planes = vec![(DVec3::X, b.max.x), (-DVec3::X, -b.min.x), (DVec3::Y, b.max.y), (-DVec3::Y, -b.min.y), (DVec3::Z, b.max.z), (-DVec3::Z, -b.min.z)];
            w.hulls.push(Hull { planes, bounds: b });
        }
        w
    }

    /// Box (relativ zu den Füßen: -r..r, 0..h) von `a` nach `b` schieben: (Anteil 0..1, Normale)
    pub fn trace(&self, a: DVec3, b: DVec3, r: f64, h: f64) -> (f64, Option<DVec3>) {
        let mn = DVec3::new(-r, -r, 0.0);
        let mx = DVec3::new(r, r, h);
        let sweep = Aabb::new(a.min(b) + mn - DVec3::ONE, a.max(b) + mx + DVec3::ONE);
        let mut best = (1.0, None);
        let d = b - a;
        for hull in &self.hulls {
            if !hull.bounds.intersects(&sweep) {
                continue;
            }
            let (mut enter, mut exit) = (-1.0f64, 1.0f64);
            let mut n_hit = None;
            let mut starts_out = false;
            let mut ok = true;
            // nächste Ebene, falls der Start (knapp) innen liegt
            let mut closest: (f64, f64, DVec3) = (f64::MIN, 0.0, DVec3::Z);
            for &(n, dist) in &hull.planes {
                // Ebene um die Box verschoben (nächste Ecke der Box)
                let off = DVec3::new(if n.x < 0.0 { mx.x } else { mn.x }, if n.y < 0.0 { mx.y } else { mn.y }, if n.z < 0.0 { mx.z } else { mn.z });
                let dd = dist - n.dot(off);
                let d1 = n.dot(a) - dd;
                let d2 = n.dot(b) - dd;
                if d1 > 0.0 {
                    starts_out = true;
                }
                if d1 > closest.0 {
                    closest = (d1, d2, n);
                }
                if d1 > 0.0 && d2 >= d1 {
                    ok = false; // ganz außerhalb dieser Ebene
                    break;
                }
                if d1 <= 0.0 && d2 <= 0.0 {
                    continue;
                }
                if d1 > d2 {
                    let f = (d1 - EPS) / (d1 - d2);
                    if f > enter {
                        enter = f;
                        n_hit = Some(n);
                    }
                } else {
                    let f = (d1 + EPS) / (d1 - d2);
                    exit = exit.min(f);
                }
            }
            if !ok {
                continue;
            }
            if !starts_out {
                // knapp innen (Rundung): wie Berührung, wenn die Bewegung weiter hineinführt;
                // tief drin: nicht festhalten (sonst kommt man nie heraus)
                if closest.0 > -1.0 && closest.1 < closest.0 && 0.0 < best.0 {
                    best = (0.0, Some(closest.2));
                }
                continue;
            }
            if enter < exit && enter > -1.0 && enter < best.0 {
                best = (enter.max(0.0), n_hit);
            }
        }
        let _ = d;
        best
    }

    /// Displacement-Boden unter (x, y) zwischen z0 (oben) und z1 (unten): (Höhe, Normale)
    pub fn disp_ground(&self, p: DVec3, z0: f64, z1: f64, r: f64) -> Option<(f64, DVec3)> {
        let mut best: Option<(f64, DVec3)> = None;
        // Mitte und vier Ecken der Box: höchster Punkt trägt
        for (dx, dy) in [(0.0, 0.0), (-r, -r), (r, -r), (-r, r), (r, r)] {
            let o = DVec3::new(p.x + dx * 0.7, p.y + dy * 0.7, z0);
            for t in &self.tris {
                let (mnx, mxx) = (t[0].x.min(t[1].x).min(t[2].x), t[0].x.max(t[1].x).max(t[2].x));
                let (mny, mxy) = (t[0].y.min(t[1].y).min(t[2].y), t[0].y.max(t[1].y).max(t[2].y));
                if o.x < mnx || o.x > mxx || o.y < mny || o.y > mxy {
                    continue;
                }
                let n = (t[1] - t[0]).cross(t[2] - t[0]);
                if n.z.abs() < 1e-9 {
                    continue;
                }
                // Höhe der Ebene an (x, y), dann Punkt-im-Dreieck-Test
                let z = t[0].z - (n.x * (o.x - t[0].x) + n.y * (o.y - t[0].y)) / n.z;
                if z > z0 || z < z1 {
                    continue;
                }
                let q = DVec3::new(o.x, o.y, z);
                let inside = |a: DVec3, b: DVec3| (b - a).cross(q - a).dot(n) >= -1e-6;
                if inside(t[0], t[1]) && inside(t[1], t[2]) && inside(t[2], t[0]) && best.is_none_or(|b| z > b.0) {
                    best = Some((z, (n * n.z.signum()).normalize()));
                }
            }
        }
        best
    }
}

/// Eingaben eines Bildes
#[derive(Clone, Copy, Default)]
pub struct Input {
    pub fwd: f64,
    pub side: f64,
    pub jump: bool,
    pub duck: bool,
    pub run: bool,
    /// Blickrichtung (Grad)
    pub yaw: f64,
}

#[derive(Clone, Default)]
pub struct Walker {
    pub on: bool,
    /// Füße
    pub pos: DVec3,
    pub vel: DVec3,
    pub on_ground: bool,
    pub ducked: bool,
    world: Option<(u64, World)>,
}

impl Walker {
    pub fn height(&self) -> f64 {
        if self.ducked { DUCK_HEIGHT } else { HULL.1 }
    }

    pub fn eye(&self) -> f64 {
        if self.ducked { DUCK_EYE } else { EYE }
    }

    /// Starten: Auge an der Kamera, dann auf den Boden fallen lassen
    pub fn start(&mut self, ed: &Editor, eye: DVec3) {
        self.on = true;
        self.pos = eye - DVec3::Z * EYE;
        self.vel = DVec3::ZERO;
        self.ducked = false;
        self.world = Some((ed.rev, World::build(ed)));
    }

    fn world(&mut self, ed: &Editor) -> &World {
        if self.world.as_ref().is_none_or(|w| w.0 != ed.rev) {
            self.world = Some((ed.rev, World::build(ed)));
        }
        &self.world.as_ref().unwrap().1
    }

    /// Bewegen mit Abgleiten an Wänden (bis zu 4 Ebenen)
    fn slide(w: &World, pos: &mut DVec3, vel: &mut DVec3, dt: f64, h: f64) {
        let mut left = dt;
        let mut planes: Vec<DVec3> = Vec::new();
        for _ in 0..4 {
            if left <= 0.0 || vel.length_squared() < 1e-6 {
                break;
            }
            let target = *pos + *vel * left;
            let (f, n) = w.trace(*pos, target, HULL.0, h);
            *pos += (target - *pos) * f;
            let Some(n) = n else { break };
            left *= 1.0 - f;
            planes.push(n);
            // Geschwindigkeit entlang aller berührten Ebenen
            for p in &planes {
                let into = vel.dot(*p);
                if into < 0.0 {
                    *vel -= *p * into * 1.001;
                }
            }
        }
    }

    /// Ein Schritt der Bewegung
    pub fn step(&mut self, ed: &Editor, inp: Input, dt: f64) {
        let dt = dt.clamp(0.001, 0.05);
        let w = self.world(ed).clone();
        // Ducken (Aufstehen nur, wenn Platz ist)
        if inp.duck && !self.ducked {
            self.ducked = true;
            if !self.on_ground {
                self.pos.z += HULL.1 - DUCK_HEIGHT; // in der Luft: Beine anziehen
            }
        } else if !inp.duck && self.ducked {
            let up = if self.on_ground { 0.0 } else { -(HULL.1 - DUCK_HEIGHT) };
            let test = self.pos + DVec3::Z * up;
            if w.trace(test, test + DVec3::Z * 0.1, HULL.0, HULL.1).0 >= 1.0 {
                self.pos = test;
                self.ducked = false;
            }
        }
        let h = self.height();
        // Wunschrichtung
        let (s, c) = inp.yaw.to_radians().sin_cos();
        let (f, r) = (DVec3::new(c, s, 0.0), DVec3::new(s, -c, 0.0));
        let mut wish = f * inp.fwd + r * inp.side;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }
        let speed = if self.ducked { WALK / 3.0 } else if inp.run { RUN } else { WALK };
        if self.on_ground {
            self.vel.x = wish.x * speed;
            self.vel.y = wish.y * speed;
            if inp.jump {
                self.vel.z = JUMP;
                self.on_ground = false;
            }
        } else {
            // wenig Steuerung in der Luft
            let add = wish * speed * 2.0 * dt;
            let hv = DVec3::new(self.vel.x + add.x, self.vel.y + add.y, 0.0);
            let m = hv.length().min(speed.max(DVec3::new(self.vel.x, self.vel.y, 0.0).length()));
            let hv = hv.normalize_or_zero() * m;
            self.vel.x = hv.x;
            self.vel.y = hv.y;
            self.vel.z -= GRAVITY * dt;
        }
        // Bewegung: normal und mit Stufe; das weitere Ergebnis gewinnt
        let start = self.pos;
        let mut p1 = start;
        let mut v1 = self.vel;
        Self::slide(&w, &mut p1, &mut v1, dt, h);
        if self.on_ground {
            let mut p2 = start;
            let up = w.trace(p2, p2 + DVec3::Z * STEP, HULL.0, h).0 * STEP;
            p2.z += up;
            let mut v2 = DVec3::new(self.vel.x, self.vel.y, 0.0);
            Self::slide(&w, &mut p2, &mut v2, dt, h);
            let (fd, n) = w.trace(p2, p2 - DVec3::Z * (up + STEP), HULL.0, h);
            p2.z -= (up + STEP) * fd;
            let flat = n.is_some_and(|n| n.z >= 0.7);
            let d1 = (p1 - start).truncate().length();
            let d2 = (p2 - start).truncate().length();
            if flat && d2 > d1 + 0.01 {
                p1 = p2;
                v1 = DVec3::new(v2.x, v2.y, 0.0);
            }
        }
        self.pos = p1;
        self.vel = v1;
        // Boden: Brushes (kurzer Test nach unten) und Displacements
        let (fd, n) = w.trace(self.pos, self.pos - DVec3::Z * 2.0, HULL.0, h);
        let mut ground = fd < 1.0 && n.is_some_and(|n| n.z >= 0.7) && self.vel.z <= 1.0;
        if ground {
            self.pos.z -= 2.0 * fd;
        }
        let snap = if self.on_ground { STEP } else { 0.0 };
        if let Some((z, gn)) = w.disp_ground(self.pos, self.pos.z + STEP, self.pos.z - snap - 2.0, HULL.0) {
            if self.vel.z <= 1.0 && gn.z >= 0.7 {
                // auf das Gelände stellen (auch bergauf bis Stufenhöhe)
                if z >= self.pos.z - snap - 2.0 {
                    self.pos.z = z;
                    ground = true;
                }
            } else if gn.z < 0.7 && z > self.pos.z + 2.0 && z <= self.pos.z + STEP {
                // zu steil: zurück
                self.pos.x = start.x;
                self.pos.y = start.y;
            }
        }
        if ground {
            self.vel.z = 0.0;
        }
        self.on_ground = ground;
        // nicht endlos fallen
        if self.pos.z < -65536.0 {
            self.pos = start;
            self.vel = DVec3::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> Editor {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let mut add = |a: DVec3, b: DVec3| {
            let s = ed.map.make_box(Aabb::new(a, b), "DEV/X", 0.25);
            ed.map.world.solids.push(s.into());
        };
        add(DVec3::new(-512.0, -512.0, -16.0), DVec3::new(512.0, 512.0, 0.0)); // Boden
        add(DVec3::new(256.0, -512.0, 0.0), DVec3::new(272.0, 512.0, 256.0)); // Wand bei x = 256
        add(DVec3::new(-200.0, -64.0, 0.0), DVec3::new(-100.0, 64.0, 16.0)); // Stufe 16
        add(DVec3::new(-200.0, 100.0, 0.0), DVec3::new(-100.0, 200.0, 40.0)); // Kiste 40
        ed
    }

    fn run(ed: &Editor, w: &mut Walker, inp: Input, secs: f64) {
        let n = (secs / 0.02) as usize;
        for _ in 0..n {
            w.step(ed, inp, 0.02);
        }
    }

    #[test]
    fn falls_walks_blocks_steps_jumps() {
        let ed = room();
        let mut w = Walker::default();
        w.start(&ed, DVec3::new(0.0, 0.0, 200.0));
        run(&ed, &mut w, Input::default(), 2.0);
        assert!(w.on_ground && w.pos.z.abs() < 0.1, "steht auf dem Boden: {:?}", w.pos);
        // gegen die Wand laufen: bleibt davor stehen (Box 16 breit)
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 0.0, ..Default::default() }, 3.0);
        assert!((w.pos.x - (256.0 - 16.0)).abs() < 0.5, "vor der Wand: {:?}", w.pos);
        // Stufe 16 wird hochgestiegen
        let mut w = Walker::default();
        w.start(&ed, DVec3::new(-20.0, 0.0, 64.0));
        run(&ed, &mut w, Input::default(), 0.5);
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 180.0, ..Default::default() }, 0.5);
        assert!((w.pos.z - 16.0).abs() < 0.5, "auf der Stufe: {:?}", w.pos);
        // Kiste 40: nicht ohne Sprung
        let mut w = Walker::default();
        w.start(&ed, DVec3::new(-20.0, 150.0, 64.0));
        run(&ed, &mut w, Input::default(), 0.5);
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 180.0, ..Default::default() }, 1.0);
        assert!(w.pos.z < 1.0 && w.pos.x > -100.0 + 16.0 - 1.0, "vor der Kiste: {:?}", w.pos);
        // Ducksprung wie im Spiel: springen, in der Luft ducken → hinauf
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 180.0, jump: true, ..Default::default() }, 0.04);
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 180.0, duck: true, ..Default::default() }, 1.0);
        assert!((w.pos.z - 40.0).abs() < 1.0, "auf der Kiste: {:?}", w.pos);
        // nur springen reicht nicht (33 Einheiten)
        let mut w2 = Walker::default();
        w2.start(&ed, DVec3::new(-20.0, 150.0, 64.0));
        run(&ed, &mut w2, Input::default(), 0.5);
        run(&ed, &mut w2, Input { fwd: 1.0, yaw: 180.0, jump: true, ..Default::default() }, 1.0);
        assert!(w2.pos.x > -100.0, "ohne Ducken nicht hinauf: {:?}", w2.pos);
    }

    #[test]
    fn walks_on_displacement_hill() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let mut s = ed.map.make_box(Aabb::new(DVec3::new(0.0, 0.0, -16.0), DVec3::new(512.0, 512.0, 0.0)), "DEV/X", 0.25);
        let top = (0..s.sides.len()).find(|&i| s.planes[i].is_some_and(|p| p.normal.z > 0.9)).unwrap();
        let n = s.planes[top].unwrap().normal;
        let mut d = crate::disp::Disp::new_flat(3, &s.polys[top], n).unwrap();
        s.update_geometry();
        // sanfte Rampe: Höhe wächst mit x (bis 64)
        s.sides[top].disp = Some(d.clone());
        s.update_geometry();
        let g = s.disps[top].clone().unwrap();
        for k in 0..g.pos.len() {
            d.set_vector(k, DVec3::Z * (g.base[k].x / 512.0 * 64.0), n);
        }
        s.sides[top].disp = Some(d);
        s.update_geometry();
        ed.map.world.solids.push(s.into());
        let mut w = Walker::default();
        w.start(&ed, DVec3::new(32.0, 256.0, 200.0));
        run(&ed, &mut w, Input::default(), 2.0);
        assert!(w.on_ground && (w.pos.z - 4.0).abs() < 3.0, "auf dem Gelände: {:?}", w.pos);
        run(&ed, &mut w, Input { fwd: 1.0, yaw: 0.0, ..Default::default() }, 1.5);
        assert!(w.pos.x > 300.0 && w.pos.z > 35.0, "die Rampe hinauf: {:?}", w.pos);
    }
}
