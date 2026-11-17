//! Displacements (Terrain auf Brush-Flächen), wie in VMF/VBSP.

use crate::keyvalues::{KvBlock, KvItem};
use crate::transform::Xform;
use glam::DVec3;
use std::collections::HashMap;

pub const FLAG_NO_PHYS: u32 = 1 << 1;
pub const FLAG_NO_HULL: u32 = 1 << 2;
pub const FLAG_NO_RAY: u32 = 1 << 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Disp {
    pub power: u32,
    pub start: DVec3,
    pub elevation: f64,
    pub subdiv: bool,
    pub flags: u32,
    pub normals: Vec<DVec3>,
    pub distances: Vec<f64>,
    pub offsets: Vec<DVec3>,
    pub offset_normals: Vec<DVec3>,
    pub alphas: Vec<f64>,
    pub tags: Option<KvBlock>,
    pub allowed: Option<KvBlock>,
    pub extra: Vec<KvItem>,
}

/// Berechnete Punkte einer Displacement-Fläche
#[derive(Debug, Clone, Default)]
pub struct DispGeom {
    pub res: usize,
    /// Punkte der unverformten Fläche (für Texturkoordinaten)
    pub base: Vec<DVec3>,
    /// Endgültige Punkte
    pub pos: Vec<DVec3>,
}

impl DispGeom {
    /// Dreiecke (Indizes) im Source-typischen Wechselmuster
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        let r = self.res;
        let mut out = Vec::with_capacity((r - 1) * (r - 1) * 2);
        for i in 0..r - 1 {
            for j in 0..r - 1 {
                let a = i * r + j;
                let b = (i + 1) * r + j;
                let c = (i + 1) * r + j + 1;
                let d = i * r + j + 1;
                if (i + j) % 2 == 0 {
                    out.push([a, b, c]);
                    out.push([a, c, d]);
                } else {
                    out.push([a, b, d]);
                    out.push([b, c, d]);
                }
            }
        }
        out
    }

    /// Vertex-Normalen (gemittelt aus angrenzenden Dreiecken)
    pub fn vertex_normals(&self, face_normal: DVec3) -> Vec<DVec3> {
        let mut n = vec![DVec3::ZERO; self.pos.len()];
        for t in self.triangles() {
            let fnrm = (self.pos[t[1]] - self.pos[t[0]]).cross(self.pos[t[2]] - self.pos[t[0]]);
            for k in t {
                n[k] += fnrm;
            }
        }
        n.into_iter()
            .map(|v| {
                let v = v.normalize_or_zero();
                // Ausrichtung an der Flächennormale sicherstellen
                if v.dot(face_normal) < 0.0 {
                    -v
                } else if v == DVec3::ZERO {
                    face_normal
                } else {
                    v
                }
            })
            .collect()
    }

    pub fn ray_hit(&self, o: DVec3, d: DVec3) -> Option<(f64, usize)> {
        let mut best: Option<(f64, usize)> = None;
        for t in self.triangles() {
            if let Some(dist) = ray_triangle(o, d, self.pos[t[0]], self.pos[t[1]], self.pos[t[2]]) {
                if best.map_or(true, |b| dist < b.0) {
                    // nächster Gitterpunkt
                    let hit = o + d * dist;
                    let k = *t.iter().min_by(|a, b| self.pos[**a].distance(hit).partial_cmp(&self.pos[**b].distance(hit)).unwrap()).unwrap();
                    best = Some((dist, k));
                }
            }
        }
        best
    }
}

pub fn ray_triangle(o: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-6).then_some(t)
}

fn parse_floats(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|x| x.parse().ok()).collect()
}

fn parse_vec_bracket(s: &str) -> Option<DVec3> {
    let v = parse_floats(&s.replace(['[', ']'], " "));
    (v.len() >= 3).then(|| DVec3::new(v[0], v[1], v[2]))
}

fn fmt(v: f64) -> String {
    crate::map::fmt_num(v)
}

/// Einfacher Zufallsgenerator (xorshift) für Rauschen
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Disp {
    pub fn res(&self) -> usize {
        (1 << self.power) + 1
    }

    pub fn count(&self) -> usize {
        self.res() * self.res()
    }

    /// Neue flache Displacement-Fläche auf einem Viereck
    pub fn new_flat(power: u32, poly: &[DVec3], normal: DVec3) -> Option<Disp> {
        if poly.len() != 4 {
            return None;
        }
        let power = power.clamp(2, 4);
        let n = ((1usize << power) + 1).pow(2);
        // Start an der Ecke mit den kleinsten Koordinaten (wie Hammer meist)
        let start = *poly.iter().min_by(|a, b| (a.x + a.y + a.z).partial_cmp(&(b.x + b.y + b.z)).unwrap()).unwrap();
        let mut allowed = KvBlock::new("allowed_verts");
        allowed.push("10", vec!["-1"; 10].join(" "));
        Some(Disp {
            power,
            start,
            elevation: 0.0,
            subdiv: false,
            flags: 0,
            normals: vec![normal; n],
            distances: vec![0.0; n],
            offsets: vec![DVec3::ZERO; n],
            offset_normals: vec![normal; n],
            alphas: vec![0.0; n],
            tags: None,
            allowed: Some(allowed),
            extra: Vec::new(),
        })
    }

    pub fn parse(b: &KvBlock) -> Option<Disp> {
        let power: u32 = b.get("power")?.trim().parse().ok()?;
        if !(1..=4).contains(&power) {
            return None;
        }
        let res = (1usize << power) + 1;
        let n = res * res;
        let start = parse_vec_bracket(b.get("startposition")?)?;
        let rows = |name: &str, per: usize| -> Option<Vec<f64>> {
            let blk = b.blocks().find(|c| c.name.eq_ignore_ascii_case(name))?;
            let mut out = Vec::with_capacity(n * per);
            for i in 0..res {
                let row = blk.get(&format!("row{i}"))?;
                let v = parse_floats(row);
                if v.len() < res * per {
                    return None;
                }
                out.extend_from_slice(&v[..res * per]);
            }
            Some(out)
        };
        let vecs = |name: &str, def: DVec3| -> Vec<DVec3> {
            rows(name, 3).map(|v| v.chunks(3).map(|c| DVec3::new(c[0], c[1], c[2])).collect()).unwrap_or_else(|| vec![def; n])
        };
        let mut d = Disp {
            power,
            start,
            elevation: b.get("elevation").and_then(|v| v.trim().parse().ok()).unwrap_or(0.0),
            subdiv: b.get("subdiv").is_some_and(|v| v.trim() == "1"),
            flags: b.get("flags").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
            normals: vecs("normals", DVec3::Z),
            distances: rows("distances", 1).unwrap_or_else(|| vec![0.0; n]),
            offsets: vecs("offsets", DVec3::ZERO),
            offset_normals: vecs("offset_normals", DVec3::Z),
            alphas: rows("alphas", 1).unwrap_or_else(|| vec![0.0; n]),
            tags: b.blocks().find(|c| c.name.eq_ignore_ascii_case("triangle_tags")).cloned(),
            allowed: b.blocks().find(|c| c.name.eq_ignore_ascii_case("allowed_verts")).cloned(),
            extra: Vec::new(),
        };
        const KNOWN: [&str; 13] = [
            "power", "startposition", "elevation", "subdiv", "flags", "normals", "distances", "offsets", "offset_normals", "alphas", "triangle_tags", "allowed_verts", "id",
        ];
        for item in &b.items {
            let name = match item {
                KvItem::Pair(k, _) => k,
                KvItem::Block(c) => &c.name,
            };
            if !KNOWN.iter().any(|k| name.eq_ignore_ascii_case(k)) {
                d.extra.push(item.clone());
            }
        }
        Some(d)
    }

    pub fn to_kv(&self) -> KvBlock {
        let res = self.res();
        let mut b = KvBlock::new("dispinfo");
        b.push("power", self.power.to_string());
        b.push("startposition", format!("[{} {} {}]", fmt(self.start.x), fmt(self.start.y), fmt(self.start.z)));
        b.push("flags", self.flags.to_string());
        b.push("elevation", fmt(self.elevation));
        b.push("subdiv", if self.subdiv { "1" } else { "0" });
        let vec_rows = |name: &str, v: &[DVec3]| {
            let mut blk = KvBlock::new(name);
            for i in 0..res {
                let row: Vec<String> = v[i * res..(i + 1) * res].iter().map(|p| format!("{} {} {}", fmt(p.x), fmt(p.y), fmt(p.z))).collect();
                blk.push(format!("row{i}"), row.join(" "));
            }
            blk
        };
        let f_rows = |name: &str, v: &[f64]| {
            let mut blk = KvBlock::new(name);
            for i in 0..res {
                let row: Vec<String> = v[i * res..(i + 1) * res].iter().map(|x| fmt(*x)).collect();
                blk.push(format!("row{i}"), row.join(" "));
            }
            blk
        };
        b.push_block(vec_rows("normals", &self.normals));
        b.push_block(f_rows("distances", &self.distances));
        b.push_block(vec_rows("offsets", &self.offsets));
        b.push_block(vec_rows("offset_normals", &self.offset_normals));
        b.push_block(f_rows("alphas", &self.alphas));
        if let Some(t) = &self.tags {
            b.push_block(t.clone());
        }
        if let Some(a) = &self.allowed {
            b.push_block(a.clone());
        }
        b.items.extend(self.extra.iter().cloned());
        b
    }

    /// Eckpunkte in VBSP-Reihenfolge (Uhrzeigersinn, beginnend bei startposition)
    pub fn corners(&self, poly: &[DVec3]) -> Option<[DVec3; 4]> {
        if poly.len() != 4 {
            return None;
        }
        let rev: Vec<DVec3> = poly.iter().rev().copied().collect();
        let idx = (0..4).min_by(|a, b| rev[*a].distance(self.start).partial_cmp(&rev[*b].distance(self.start)).unwrap())?;
        Some([rev[idx], rev[(idx + 1) % 4], rev[(idx + 2) % 4], rev[(idx + 3) % 4]])
    }

    pub fn geometry(&self, poly: &[DVec3], face_normal: DVec3) -> Option<DispGeom> {
        let c = self.corners(poly)?;
        let res = self.res();
        let mut base = Vec::with_capacity(res * res);
        let mut pos = Vec::with_capacity(res * res);
        for i in 0..res {
            let tr = i as f64 / (res - 1) as f64;
            let a = c[0] + (c[1] - c[0]) * tr;
            let b = c[3] + (c[2] - c[3]) * tr;
            for j in 0..res {
                let tc = j as f64 / (res - 1) as f64;
                let p = a + (b - a) * tc;
                let k = i * res + j;
                base.push(p);
                pos.push(p + self.offsets[k] + self.normals[k] * self.distances[k] + face_normal * self.elevation);
            }
        }
        Some(DispGeom { res, base, pos })
    }

    pub fn vector(&self, k: usize) -> DVec3 {
        self.normals[k] * self.distances[k]
    }

    pub fn set_vector(&mut self, k: usize, v: DVec3, face_normal: DVec3) {
        let len = v.length();
        if len < 1e-6 {
            self.normals[k] = face_normal;
            self.distances[k] = 0.0;
        } else {
            self.normals[k] = v / len;
            self.distances[k] = len;
        }
    }

    pub fn translate(&mut self, d: DVec3) {
        self.start += d;
    }

    pub fn transform(&mut self, x: &Xform) {
        self.start = x.apply(self.start);
        for n in self.normals.iter_mut().chain(self.offset_normals.iter_mut()) {
            *n = (x.m * *n).normalize_or_zero();
        }
        for o in &mut self.offsets {
            *o = x.m * *o;
        }
        // Verzerrungen ändern die Länge der Verschiebungen
        let s = x.m.determinant().abs().cbrt();
        for d in &mut self.distances {
            *d *= s;
        }
    }

    pub fn invert_alpha(&mut self) {
        for a in &mut self.alphas {
            *a = 255.0 - *a;
        }
    }

    pub fn add_noise(&mut self, min: f64, max: f64, face_normal: DVec3, rng: &mut Rng) {
        for k in 0..self.count() {
            let v = self.vector(k) + face_normal * (min + (max - min) * rng.next_f64());
            self.set_vector(k, v, face_normal);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintMode {
    Raise,
    Lower,
    Smooth,
    Flatten,
    AlphaAdd,
    AlphaRemove,
    /// zufällige Unebenheiten
    Noise,
    /// Höhen auf Stufen ziehen (Terrassen)
    Terrace,
    /// Kanten schärfen (Gegenteil von Glätten)
    Sharpen,
}

/// Abfall des Pinsels zum Rand
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Falloff {
    Soft,
    Linear,
    Hard,
}

impl Falloff {
    pub fn label(self) -> &'static str {
        match self {
            Falloff::Soft => crate::i18n::tr("Weich"),
            Falloff::Linear => crate::i18n::tr("Linear"),
            Falloff::Hard => crate::i18n::tr("Hart"),
        }
    }
    fn weight(self, d: f64, r: f64) -> f64 {
        match self {
            Falloff::Soft => 0.5 + 0.5 * (std::f64::consts::PI * d / r).cos(),
            Falloff::Linear => 1.0 - d / r,
            Falloff::Hard => 1.0,
        }
    }
}

/// Pseudozufall aus einer Position (stabil pro Punkt und Strich)
fn hash01(p: DVec3, seed: u32) -> f64 {
    let mut h = (p.x.round() as i64).wrapping_mul(73856093) ^ (p.y.round() as i64).wrapping_mul(19349663) ^ (p.z.round() as i64).wrapping_mul(83492791) ^ (seed as i64).wrapping_mul(2654435761);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h & 0xffff) as f64 / 65535.0
}

impl PaintMode {
    pub fn label(self) -> &'static str {
        match self {
            PaintMode::Raise => crate::i18n::tr("Anheben"),
            PaintMode::Lower => crate::i18n::tr("Absenken"),
            PaintMode::Smooth => crate::i18n::tr("Glätten"),
            PaintMode::Flatten => crate::i18n::tr("Auf Höhe setzen"),
            PaintMode::AlphaAdd => crate::i18n::tr("Alpha malen"),
            PaintMode::AlphaRemove => crate::i18n::tr("Alpha entfernen"),
            PaintMode::Noise => crate::i18n::tr("Rauschen"),
            PaintMode::Terrace => crate::i18n::tr("Terrassen"),
            PaintMode::Sharpen => crate::i18n::tr("Schärfen"),
        }
    }
}

/// Wendet einen Pinselstrich auf ein Displacement an. Liefert true bei Änderung.
#[allow(clippy::too_many_arguments, dead_code)]
pub fn paint(disp: &mut Disp, geom: &DispGeom, face_normal: DVec3, center: DVec3, radius: f64, amount: f64, mode: PaintMode, axis: DVec3, flatten_height: f64) -> bool {
    paint_ex(disp, geom, face_normal, center, radius, amount, mode, axis, flatten_height, Falloff::Soft, 16.0, 0)
}

/// Wie `paint`, mit Abfall-Kurve, Stufenhöhe (Terrassen) und Zufallswert (Rauschen)
#[allow(clippy::too_many_arguments)]
pub fn paint_ex(disp: &mut Disp, geom: &DispGeom, face_normal: DVec3, center: DVec3, radius: f64, amount: f64, mode: PaintMode, axis: DVec3, flatten_height: f64, falloff_kind: Falloff, step: f64, seed: u32) -> bool {
    let res = geom.res;
    let mut changed = false;
    let old: Vec<DVec3> = (0..disp.count()).map(|k| disp.vector(k)).collect();
    for k in 0..geom.pos.len() {
        let d = geom.pos[k].distance(center);
        if d > radius {
            continue;
        }
        // Abfall zum Rand
        let falloff = falloff_kind.weight(d, radius);
        match mode {
            PaintMode::Raise | PaintMode::Lower => {
                let sign = if mode == PaintMode::Raise { 1.0 } else { -1.0 };
                let v = old[k] + axis * (amount * falloff * sign);
                disp.set_vector(k, v, face_normal);
            }
            PaintMode::Smooth => {
                let (i, j) = (k / res, k % res);
                let mut sum = DVec3::ZERO;
                let mut cnt = 0.0;
                for (di, dj) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (ni, nj) = (i as i32 + di, j as i32 + dj);
                    if ni >= 0 && nj >= 0 && (ni as usize) < res && (nj as usize) < res {
                        sum += old[ni as usize * res + nj as usize];
                        cnt += 1.0;
                    }
                }
                let avg = sum / cnt;
                let t = (falloff * amount / 20.0).clamp(0.0, 1.0);
                disp.set_vector(k, old[k] + (avg - old[k]) * t, face_normal);
            }
            PaintMode::Flatten => {
                // Höhe entlang der Achse auf flatten_height ziehen
                let cur = geom.pos[k].dot(axis);
                let t = (falloff * amount / 20.0).clamp(0.0, 1.0);
                let v = old[k] + axis * ((flatten_height - cur) * t);
                disp.set_vector(k, v, face_normal);
            }
            PaintMode::Noise => {
                let r = hash01(geom.pos[k], seed) * 2.0 - 1.0;
                disp.set_vector(k, old[k] + axis * (r * amount * falloff), face_normal);
            }
            PaintMode::Terrace => {
                let cur = geom.pos[k].dot(axis);
                let target = (cur / step.max(1.0)).round() * step.max(1.0);
                let t = (falloff * amount / 20.0).clamp(0.0, 1.0);
                disp.set_vector(k, old[k] + axis * ((target - cur) * t), face_normal);
            }
            PaintMode::Sharpen => {
                let (i, j) = (k / res, k % res);
                let mut sum = DVec3::ZERO;
                let mut cnt = 0.0;
                for (di, dj) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (ni, nj) = (i as i32 + di, j as i32 + dj);
                    if ni >= 0 && nj >= 0 && (ni as usize) < res && (nj as usize) < res {
                        sum += old[ni as usize * res + nj as usize];
                        cnt += 1.0;
                    }
                }
                let avg = sum / cnt;
                let t = (falloff * amount / 40.0).clamp(0.0, 0.5);
                disp.set_vector(k, old[k] + (old[k] - avg) * t, face_normal);
            }
            PaintMode::AlphaAdd | PaintMode::AlphaRemove => {
                let sign = if mode == PaintMode::AlphaAdd { 1.0 } else { -1.0 };
                disp.alphas[k] = (disp.alphas[k] + sign * amount * 4.0 * falloff).clamp(0.0, 255.0);
            }
        }
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_roundtrip_and_geometry() {
        let poly = vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(256.0, 0.0, 0.0), DVec3::new(256.0, 256.0, 0.0), DVec3::new(0.0, 256.0, 0.0)];
        let mut d = Disp::new_flat(3, &poly, DVec3::Z).unwrap();
        assert_eq!(d.res(), 9);
        d.distances[40] = 32.0; // Mitte anheben
        let kv = d.to_kv();
        let back = Disp::parse(&kv).unwrap();
        assert_eq!(back.distances[40], 32.0);
        let g = back.geometry(&poly, DVec3::Z).unwrap();
        assert_eq!(g.pos.len(), 81);
        assert_eq!(g.pos[0], DVec3::ZERO);
        assert_eq!(g.pos[40], DVec3::new(128.0, 128.0, 32.0));
        assert_eq!(g.triangles().len(), 128);
        let hit = g.ray_hit(DVec3::new(128.0, 128.0, 100.0), -DVec3::Z).unwrap();
        assert!((hit.0 - 68.0).abs() < 1e-6);
        assert_eq!(hit.1, 40);
    }

    #[test]
    fn painting() {
        let poly = vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(256.0, 0.0, 0.0), DVec3::new(256.0, 256.0, 0.0), DVec3::new(0.0, 256.0, 0.0)];
        let mut d = Disp::new_flat(2, &poly, DVec3::Z).unwrap();
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        assert!(paint(&mut d, &g, DVec3::Z, DVec3::new(128.0, 128.0, 0.0), 70.0, 10.0, PaintMode::Raise, DVec3::Z, 0.0));
        let g2 = d.geometry(&poly, DVec3::Z).unwrap();
        assert!((g2.pos[12].z - 10.0).abs() < 1e-6);
        assert_eq!(g2.pos[0].z, 0.0);
        paint(&mut d, &g2, DVec3::Z, DVec3::new(128.0, 128.0, 0.0), 70.0, 20.0, PaintMode::AlphaAdd, DVec3::Z, 0.0);
        assert!(d.alphas[12] > 0.0);
        d.invert_alpha();
        assert!(d.alphas[0] == 255.0);
    }

    #[test]
    fn sculpt_modes() {
        let poly = vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(256.0, 0.0, 0.0), DVec3::new(256.0, 256.0, 0.0), DVec3::new(0.0, 256.0, 0.0)];
        let c = DVec3::new(128.0, 128.0, 0.0);
        // Rauschen: Punkte bewegen sich unterschiedlich
        let mut d = Disp::new_flat(3, &poly, DVec3::Z).unwrap();
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        assert!(paint_ex(&mut d, &g, DVec3::Z, c, 200.0, 10.0, PaintMode::Noise, DVec3::Z, 0.0, Falloff::Hard, 16.0, 1));
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        let zs: Vec<f64> = g.pos.iter().map(|p| p.z).collect();
        assert!(zs.iter().any(|z| *z > 0.5) && zs.iter().any(|z| *z < -0.5));
        // Terrassen: angehobene Fläche wird auf Stufen gezogen
        let mut d = Disp::new_flat(2, &poly, DVec3::Z).unwrap();
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        paint_ex(&mut d, &g, DVec3::Z, c, 400.0, 21.0, PaintMode::Raise, DVec3::Z, 0.0, Falloff::Hard, 16.0, 0);
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        paint_ex(&mut d, &g, DVec3::Z, c, 400.0, 20.0, PaintMode::Terrace, DVec3::Z, 0.0, Falloff::Hard, 16.0, 0);
        let g = d.geometry(&poly, DVec3::Z).unwrap();
        assert!(g.pos.iter().all(|p| (p.z - 16.0).abs() < 1e-6), "auf 16 gezogen: {}", g.pos[0].z);
        // Abfall-Kurven
        assert_eq!(Falloff::Hard.weight(50.0, 100.0), 1.0);
        assert!((Falloff::Linear.weight(50.0, 100.0) - 0.5).abs() < 1e-9);
    }
}

// ------------------------------------------------------------------ Feinheit ändern / Unterteilen

impl Disp {
    /// Power ändern, Form und Alpha bleiben erhalten (bilineare Interpolation)
    pub fn resample(&mut self, power: u32, face_normal: DVec3) {
        let power = power.clamp(2, 4);
        if power == self.power {
            return;
        }
        let (r0, r1) = (self.res(), (1usize << power) + 1);
        let sample = |f: &dyn Fn(usize) -> DVec3, u: f64, v: f64| -> DVec3 {
            let (x, y) = (u * (r0 - 1) as f64, v * (r0 - 1) as f64);
            let (i, j) = ((x.floor() as usize).min(r0 - 2), (y.floor() as usize).min(r0 - 2));
            let (fx, fy) = (x - i as f64, y - j as f64);
            let a = f(i * r0 + j) * (1.0 - fy) + f(i * r0 + j + 1) * fy;
            let b = f((i + 1) * r0 + j) * (1.0 - fy) + f((i + 1) * r0 + j + 1) * fy;
            a * (1.0 - fx) + b * fx
        };
        let vec_of = |k: usize| self.normals[k] * self.distances[k];
        let off_of = |k: usize| self.offsets[k];
        let alpha_of = |k: usize| DVec3::splat(self.alphas[k]);
        let n = r1 * r1;
        let (mut vecs, mut offs, mut alphas) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..r1 {
            for j in 0..r1 {
                let (u, v) = (i as f64 / (r1 - 1) as f64, j as f64 / (r1 - 1) as f64);
                vecs.push(sample(&vec_of, u, v));
                offs.push(sample(&off_of, u, v));
                alphas.push(sample(&alpha_of, u, v).x);
            }
        }
        self.power = power;
        self.normals = vec![face_normal; n];
        self.distances = vec![0.0; n];
        for (k, v) in vecs.into_iter().enumerate() {
            self.set_vector(k, v, face_normal);
        }
        self.offsets = offs;
        self.offset_normals = vec![face_normal; n];
        self.alphas = alphas;
        self.tags = None;
    }
}

/// Eine Displacement-Fläche für die Unterteilung: Grundviereck (in Displacement-Reihenfolge)
pub struct SubdivFace {
    pub corners: [DVec3; 4],
    pub power: u32,
}

/// Catmull-Clark-Unterteilung über mehrere zusammenhängende Displacement-Flächen (wie Hammers „Subdivide“).
/// Liefert je Fläche die Zielpunkte im Raster (res × res, Index i * res + j).
pub fn subdivide_faces(faces: &[SubdivFace]) -> Vec<Vec<DVec3>> {
    let levels = faces.iter().map(|f| f.power).max().unwrap_or(2);
    // gemeinsame Eckpunkte verschweißen
    let mut verts: Vec<DVec3> = Vec::new();
    let weld = |p: DVec3, verts: &mut Vec<DVec3>| -> usize {
        match verts.iter().position(|q| q.distance(p) < 0.1) {
            Some(i) => i,
            None => {
                verts.push(p);
                verts.len() - 1
            }
        }
    };
    // Raster je Fläche: (m+1)×(m+1) Vertex-IDs; Start m = 1
    let mut grids: Vec<(usize, Vec<usize>)> = faces
        .iter()
        .map(|f| {
            let ids: Vec<usize> = f.corners.iter().map(|c| weld(*c, &mut verts)).collect();
            // G[i][j]: i entlang c0->c1, j entlang c0->c3
            (1usize, vec![ids[0], ids[3], ids[1], ids[2]])
        })
        .collect();
    for _ in 0..levels {
        // Quads der aktuellen Stufe
        let mut quads: Vec<[usize; 4]> = Vec::new();
        for (m, g) in &grids {
            let w = m + 1;
            for i in 0..*m {
                for j in 0..*m {
                    quads.push([g[i * w + j], g[(i + 1) * w + j], g[(i + 1) * w + j + 1], g[i * w + j + 1]]);
                }
            }
        }
        let face_pts: Vec<DVec3> = quads.iter().map(|q| q.iter().map(|v| verts[*v]).sum::<DVec3>() / 4.0).collect();
        // Kanten -> angrenzende Quads
        let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (qi, q) in quads.iter().enumerate() {
            for k in 0..4 {
                let (a, b) = (q[k], q[(k + 1) % 4]);
                edges.entry((a.min(b), a.max(b))).or_default().push(qi);
            }
        }
        let mut new_verts: Vec<DVec3> = verts.clone();
        let mut edge_pt: HashMap<(usize, usize), usize> = HashMap::new();
        for (e, adj) in &edges {
            let mid = (verts[e.0] + verts[e.1]) * 0.5;
            let p = if adj.len() == 2 { (verts[e.0] + verts[e.1] + face_pts[adj[0]] + face_pts[adj[1]]) / 4.0 } else { mid };
            new_verts.push(p);
            edge_pt.insert(*e, new_verts.len() - 1);
        }
        // Eckpunkte verschieben
        let mut v_faces: Vec<Vec<usize>> = vec![Vec::new(); verts.len()];
        let mut v_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); verts.len()];
        for (qi, q) in quads.iter().enumerate() {
            for v in q {
                v_faces[*v].push(qi);
            }
        }
        for e in edges.keys() {
            v_edges[e.0].push(*e);
            v_edges[e.1].push(*e);
        }
        for v in 0..verts.len() {
            let es = &v_edges[v];
            if es.is_empty() {
                continue;
            }
            let boundary: Vec<&(usize, usize)> = es.iter().filter(|e| edges[e].len() < 2).collect();
            new_verts[v] = if boundary.is_empty() {
                let n = v_faces[v].len() as f64;
                let f = v_faces[v].iter().map(|q| face_pts[*q]).sum::<DVec3>() / n;
                let r = es.iter().map(|e| (verts[e.0] + verts[e.1]) * 0.5).sum::<DVec3>() / es.len() as f64;
                (f + r * 2.0 + verts[v] * (n - 3.0)) / n
            } else if boundary.len() == 2 && v_faces[v].len() > 1 {
                let other = |e: &(usize, usize)| if e.0 == v { e.1 } else { e.0 };
                verts[v] * 0.75 + (verts[other(boundary[0])] + verts[other(boundary[1])]) * 0.125
            } else {
                verts[v] // Ecke am Rand bleibt fest
            };
        }
        let mut q_index = 0;
        let mut new_grids = Vec::with_capacity(grids.len());
        for (m, g) in &grids {
            let (w, m2) = (m + 1, m * 2);
            let w2 = m2 + 1;
            let mut ng = vec![0usize; w2 * w2];
            for i in 0..=*m {
                for j in 0..=*m {
                    ng[2 * i * w2 + 2 * j] = g[i * w + j];
                }
            }
            for i in 0..*m {
                for j in 0..*m {
                    new_verts.push(face_pts[q_index]);
                    q_index += 1;
                    ng[(2 * i + 1) * w2 + 2 * j + 1] = new_verts.len() - 1;
                }
            }
            let ek = |a: usize, b: usize| edge_pt[&(a.min(b), a.max(b))];
            for i in 0..=*m {
                for j in 0..*m {
                    ng[2 * i * w2 + 2 * j + 1] = ek(g[i * w + j], g[i * w + j + 1]);
                }
            }
            for i in 0..*m {
                for j in 0..=*m {
                    ng[(2 * i + 1) * w2 + 2 * j] = ek(g[i * w + j], g[(i + 1) * w + j]);
                }
            }
            new_grids.push((m2, ng));
        }
        verts = new_verts;
        grids = new_grids;
    }
    // auf die Auflösung jeder Fläche abtasten
    let full = 1usize << levels;
    faces
        .iter()
        .zip(&grids)
        .map(|(f, (_, g))| {
            let res = (1usize << f.power) + 1;
            let step = full >> f.power;
            let mut out = Vec::with_capacity(res * res);
            for i in 0..res {
                for j in 0..res {
                    out.push(verts[g[(i * step) * (full + 1) + j * step]]);
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod subdiv_tests {
    use super::*;

    #[test]
    fn resample_keeps_shape() {
        let poly = [DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 256.0, 0.0), DVec3::new(256.0, 256.0, 0.0), DVec3::new(256.0, 0.0, 0.0)];
        let mut d = Disp::new_flat(2, &poly, DVec3::Z).unwrap();
        let res = d.res();
        for k in 0..d.count() {
            let (i, j) = (k / res, k % res);
            d.set_vector(k, DVec3::Z * (i as f64 * 8.0 + j as f64), DVec3::Z);
            d.alphas[k] = 255.0 * i as f64 / 4.0;
        }
        let before = d.geometry(&poly, DVec3::Z).unwrap();
        d.resample(4, DVec3::Z);
        assert_eq!(d.res(), 17);
        let after = d.geometry(&poly, DVec3::Z).unwrap();
        // Ecken und Mitte gleich, lineare Funktion bleibt exakt
        assert!(after.pos[0].distance(before.pos[0]) < 1e-6);
        assert!(after.pos[17 * 17 - 1].distance(before.pos[24]) < 1e-6);
        assert!(after.pos[8 * 17 + 8].distance(before.pos[12]) < 1e-6);
        assert!((d.alphas[16 * 17] - 255.0).abs() < 1e-6);
        d.resample(2, DVec3::Z);
        let back = d.geometry(&poly, DVec3::Z).unwrap();
        for k in 0..25 {
            assert!(back.pos[k].distance(before.pos[k]) < 1e-6);
        }
    }

    #[test]
    fn cube_becomes_round() {
        // 6 Flächen eines Würfels (Kantenlänge 256, Mitte 0)
        let h = 128.0;
        let c = |x: f64, y: f64, z: f64| DVec3::new(x * h, y * h, z * h);
        let quads = [
            [c(-1., -1., 1.), c(1., -1., 1.), c(1., 1., 1.), c(-1., 1., 1.)],
            [c(-1., -1., -1.), c(-1., 1., -1.), c(1., 1., -1.), c(1., -1., -1.)],
            [c(-1., -1., -1.), c(1., -1., -1.), c(1., -1., 1.), c(-1., -1., 1.)],
            [c(-1., 1., -1.), c(-1., 1., 1.), c(1., 1., 1.), c(1., 1., -1.)],
            [c(-1., -1., -1.), c(-1., -1., 1.), c(-1., 1., 1.), c(-1., 1., -1.)],
            [c(1., -1., -1.), c(1., 1., -1.), c(1., 1., 1.), c(1., -1., 1.)],
        ];
        let faces: Vec<SubdivFace> = quads.iter().map(|q| SubdivFace { corners: *q, power: 3 }).collect();
        let out = subdivide_faces(&faces);
        assert_eq!(out.len(), 6);
        assert_eq!(out[0].len(), 81);
        // alle Punkte ungefähr gleich weit von der Mitte (Kugel), deutlich näher als die Würfelecken
        let d: Vec<f64> = out.iter().flatten().map(|p| p.length()).collect();
        let (mn, mx) = d.iter().fold((f64::MAX, 0.0f64), |a, b| (a.0.min(*b), a.1.max(*b)));
        assert!(mx / mn < 1.25, "{mn} {mx}");
        assert!(mx < h * 3f64.sqrt() * 0.8);
        // gemeinsame Kanten passen exakt zusammen (keine Risse)
        let shared = out[0].iter().filter(|p| out[2].iter().any(|q| q.distance(**p) < 1e-6)).count();
        assert_eq!(shared, 9);
    }
}
