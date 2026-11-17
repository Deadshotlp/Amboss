//! Globale Beleuchtung für die Lichtvorschau: indirektes Licht (Licht, das von Wänden zurückgeworfen
//! wird) in einem Raster aus Licht-Sonden. Ein Hintergrund-Thread schießt von jeder Sonde Strahlen in
//! alle Richtungen, berechnet am Trefferpunkt das direkte Licht (gleiche Formel wie der Shader, mit
//! Schattenstrahlen) mal Oberflächenfarbe und speichert das Ergebnis als sphärische Harmonische
//! 1. Ordnung (je Farbkanal 4 Zahlen). Ab dem dritten Durchgang kommt ein weiterer Abprall dazu.
//! Die Ergebnisse werden laufend verfeinert und an die 3D-Ansicht geschickt.

use glam::Vec3;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// Lichtquelle (wie im Shader)
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: Vec3,
    pub color: Vec3,
    /// Spot: Richtung, cos(innen), cos(außen)
    pub spot: Option<(Vec3, f32, f32)>,
}

/// Dreieck der Szene
#[derive(Clone, Copy, Debug)]
pub struct Tri {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    /// lineare Oberflächenfarbe (0–1)
    pub albedo: Vec3,
    /// Himmel (toolsskybox): Strahl sieht den Himmel
    pub sky: bool,
}

/// Eingabe für die Berechnung
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub tris: Vec<Tri>,
    pub lights: Vec<Light>,
    pub ambient: Vec3,
    pub sun_dir: Vec3,
    pub sun_col: Vec3,
    /// Anteil der pauschalen Umgebungshelligkeit, der zusätzlich zur GI bleibt
    pub ambient_scale: f32,
}

/// Ergebnis: Sondenraster
#[derive(Clone, Debug, Default)]
pub struct Grid {
    pub origin: Vec3,
    pub cell: f32,
    pub dims: [usize; 3],
    /// je Sonde und Kanal (R, G, B): Bestrahlung E(n) = a + b·n als [a, bx, by, bz]
    pub sh: Vec<[[f32; 4]; 3]>,
    /// fortlaufende Nummer (für das Hochladen)
    pub version: u64,
}

impl Grid {
    pub fn size(&self) -> Vec3 {
        Vec3::new(self.dims[0] as f32, self.dims[1] as f32, self.dims[2] as f32) * self.cell
    }
    fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }
    /// Bestrahlung an einem Punkt für eine Normale (trilinear zwischen den Sonden)
    pub fn eval(&self, p: Vec3, n: Vec3) -> Vec3 {
        if self.sh.is_empty() {
            return Vec3::ZERO;
        }
        let g = (p - self.origin) / self.cell - Vec3::splat(0.5);
        let max = |d: usize| (d as f32 - 1.0).max(0.0);
        let g = Vec3::new(g.x.clamp(0.0, max(self.dims[0])), g.y.clamp(0.0, max(self.dims[1])), g.z.clamp(0.0, max(self.dims[2])));
        let (x0, y0, z0) = (g.x.floor() as usize, g.y.floor() as usize, g.z.floor() as usize);
        let f = g - Vec3::new(x0 as f32, y0 as f32, z0 as f32);
        let mut out = Vec3::ZERO;
        for (dx, wx) in [(0, 1.0 - f.x), (1, f.x)] {
            for (dy, wy) in [(0, 1.0 - f.y), (1, f.y)] {
                for (dz, wz) in [(0, 1.0 - f.z), (1, f.z)] {
                    let (x, y, z) = ((x0 + dx).min(self.dims[0] - 1), (y0 + dy).min(self.dims[1] - 1), (z0 + dz).min(self.dims[2] - 1));
                    let s = &self.sh[self.idx(x, y, z)];
                    let e = |c: &[f32; 4]| c[0] + c[1] * n.x + c[2] * n.y + c[3] * n.z;
                    out += Vec3::new(e(&s[0]), e(&s[1]), e(&s[2])) * (wx * wy * wz);
                }
            }
        }
        out.max(Vec3::ZERO)
    }
}

// ------------------------------------------------------------------ Strahlen gegen Dreiecke (BVH)

#[derive(Clone, Copy)]
struct Node {
    min: Vec3,
    max: Vec3,
    /// Blatt: erstes Dreieck und Anzahl; sonst linkes Kind (rechts = links + 1 … über `right`)
    start: u32,
    count: u32,
    right: u32,
}

pub struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<Tri>,
}

impl Bvh {
    pub fn build(mut tris: Vec<Tri>) -> Bvh {
        let mut nodes = Vec::with_capacity(tris.len() * 2 / 3 + 1);
        if !tris.is_empty() {
            let n = tris.len();
            build_rec(&mut tris, 0, n, &mut nodes);
        }
        Bvh { nodes, tris }
    }

    /// Nächster Treffer: (Abstand, Dreieck)
    pub fn hit(&self, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, usize)> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best: Option<(f32, usize)> = None;
        let mut limit = tmax;
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = self.nodes[stack[sp] as usize];
            if !slab(o, inv, n.min, n.max, limit) {
                continue;
            }
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    let t = &self.tris[i as usize];
                    if let Some(h) = tri_hit(o, d, t) {
                        if h < limit {
                            limit = h;
                            best = Some((h, i as usize));
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = stack_idx(&n, false);
                stack[sp + 1] = stack_idx(&n, true);
                sp += 2;
            }
        }
        best
    }

    /// Gibt es irgendeinen Treffer vor `tmax`? (Schattenstrahl)
    pub fn occluded(&self, o: Vec3, d: Vec3, tmax: f32) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut stack = [0u32; 64];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let n = self.nodes[stack[sp] as usize];
            if !slab(o, inv, n.min, n.max, tmax) {
                continue;
            }
            if n.count > 0 {
                for i in n.start..n.start + n.count {
                    let t = &self.tris[i as usize];
                    if !t.sky && tri_hit(o, d, t).is_some_and(|h| h < tmax) {
                        return true;
                    }
                }
            } else if sp + 2 <= stack.len() {
                stack[sp] = stack_idx(&n, false);
                stack[sp + 1] = stack_idx(&n, true);
                sp += 2;
            }
        }
        false
    }

    pub fn tri(&self, i: usize) -> &Tri {
        &self.tris[i]
    }
}

fn stack_idx(n: &Node, right: bool) -> u32 {
    if right {
        n.right
    } else {
        n.start
    }
}

fn build_rec(tris: &mut [Tri], start: usize, end: usize, nodes: &mut Vec<Node>) -> u32 {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut cmin = Vec3::splat(f32::MAX);
    let mut cmax = Vec3::splat(f32::MIN);
    for t in &tris[start..end] {
        min = min.min(t.a).min(t.b).min(t.c);
        max = max.max(t.a).max(t.b).max(t.c);
        let c = (t.a + t.b + t.c) / 3.0;
        cmin = cmin.min(c);
        cmax = cmax.max(c);
    }
    let me = nodes.len();
    nodes.push(Node { min, max, start: start as u32, count: (end - start) as u32, right: 0 });
    if end - start <= 4 {
        return me as u32;
    }
    let ext = cmax - cmin;
    let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
    let key = |t: &Tri| (t.a[axis] + t.b[axis] + t.c[axis]) / 3.0;
    let mid = (start + end) / 2;
    tris[start..end].select_nth_unstable_by(mid - start, |a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
    let l = build_rec(tris, start, mid, nodes);
    let r = build_rec(tris, mid, end, nodes);
    nodes[me] = Node { min, max, start: l, count: 0, right: r };
    me as u32
}

fn slab(o: Vec3, inv: Vec3, min: Vec3, max: Vec3, tmax: f32) -> bool {
    let t1 = (min - o) * inv;
    let t2 = (max - o) * inv;
    let tn = t1.min(t2).max_element();
    let tf = t1.max(t2).min_element();
    tf >= tn.max(0.0) && tn < tmax
}

/// Möller-Trumbore, beidseitig
fn tri_hit(o: Vec3, d: Vec3, t: &Tri) -> Option<f32> {
    let e1 = t.b - t.a;
    let e2 = t.c - t.a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t.a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let h = e2.dot(q) * inv;
    (h > 1e-3).then_some(h)
}

// ------------------------------------------------------------------ Licht

/// Direktes Licht an einem Punkt (wie der Shader, vor der Gamma-Korrektur), mit Schatten
pub fn direct(bvh: &Bvh, s: &Scene, p: Vec3, n: Vec3) -> Vec3 {
    let p = p + n * 1.0;
    let mut l = s.ambient * s.ambient_scale;
    let ns = n.dot(-s.sun_dir);
    if s.sun_col.max_element() > 0.0 && ns > 0.0 && !bvh.occluded(p, -s.sun_dir, 32768.0) {
        l += s.sun_col * ns;
    }
    for li in &s.lights {
        let d = li.pos - p;
        let dist = d.length().max(1.0);
        let dir = d / dist;
        let nl = n.dot(dir);
        if nl <= 0.0 {
            continue;
        }
        let mut att = (10000.0 / (dist * dist)).min(4.0);
        if let Some((sd, inner, outer)) = li.spot {
            let c = (-dir).dot(sd);
            att *= ((c - outer) / (inner - outer).max(0.001)).clamp(0.0, 1.0);
        }
        let contrib = li.color * nl * att;
        if contrib.max_element() < 0.002 || bvh.occluded(p, dir, dist - 2.0) {
            continue;
        }
        l += contrib;
    }
    l
}

// ------------------------------------------------------------------ Berechnung

pub struct Job {
    pub rx: Receiver<(Arc<Grid>, usize, usize)>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Anzahl der Durchgänge und Strahlen je Durchgang
pub const PASSES: usize = 16;
const RAYS: usize = 32;

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn dir(&mut self) -> Vec3 {
        let z = self.f() * 2.0 - 1.0;
        let a = self.f() * std::f32::consts::TAU;
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), r * a.sin(), z)
    }
}

/// Raster über die Szene legen: Zellgröße so, dass höchstens ~48 Sonden je Achse entstehen
pub fn layout(scene: &Scene) -> Grid {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for t in &scene.tris {
        min = min.min(t.a).min(t.b).min(t.c);
        max = max.max(t.a).max(t.b).max(t.c);
    }
    if scene.tris.is_empty() {
        return Grid::default();
    }
    let ext = (max - min).max(Vec3::splat(1.0));
    let cell = (ext.max_element() / 48.0).max(48.0);
    let dims = [0, 1, 2].map(|k| ((ext[k] / cell).ceil() as usize + 1).clamp(1, 64));
    let size = Vec3::new(dims[0] as f32, dims[1] as f32, dims[2] as f32) * cell;
    let origin = (min + max) * 0.5 - size * 0.5;
    Grid { origin, cell, dims, sh: vec![[[0.0; 4]; 3]; dims[0] * dims[1] * dims[2]], version: 0 }
}

/// Berechnung starten; liefert nach jedem Durchgang ein neues Raster
pub fn start(scene: Scene) -> Job {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    std::thread::spawn(move || {
        let mut grid = layout(&scene);
        let bvh = Bvh::build(scene.tris.clone());
        let n = grid.sh.len();
        if n == 0 {
            return;
        }
        let pos: Vec<Vec3> = (0..n)
            .map(|i| {
                let (x, y, z) = (i % grid.dims[0], (i / grid.dims[0]) % grid.dims[1], i / (grid.dims[0] * grid.dims[1]));
                grid.origin + (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5)) * grid.cell
            })
            .collect();
        // Summen je Sonde: SH-Koeffizienten (3 Kanäle × 4), Strahlen, Rückseiten-Treffer
        let mut acc: Vec<([[f32; 4]; 3], u32, u32)> = vec![([[0.0; 4]; 3], 0, 0); n];
        // ein Kern bleibt für die Oberfläche frei
        let threads = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4).saturating_sub(1).clamp(1, 16);
        let mut prev: Option<Arc<Grid>> = None;
        for pass in 0..PASSES {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let bounce = prev.clone().filter(|_| pass >= 2);
            let chunk = n.div_ceil(threads);
            std::thread::scope(|sc| {
                for (ci, part) in acc.chunks_mut(chunk).enumerate() {
                    let (bvh, scene, pos, stop, bounce) = (&bvh, &scene, &pos, &stop, bounce.clone());
                    sc.spawn(move || {
                        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((pass as u64) << 32) ^ (ci as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
                        for (k, a) in part.iter_mut().enumerate() {
                            if k % 64 == 0 && stop.load(Ordering::Relaxed) {
                                return;
                            }
                            let p = pos[ci * chunk + k];
                            for _ in 0..RAYS {
                                let d = rng.dir();
                                let radiance = match bvh.hit(p, d, 65536.0) {
                                    Some((t, ti)) => {
                                        let tri = bvh.tri(ti);
                                        if tri.sky {
                                            scene.ambient
                                        } else {
                                            let mut nn = (tri.b - tri.a).cross(tri.c - tri.a).normalize_or_zero();
                                            if nn.dot(d) > 0.0 {
                                                // Rückseite: Sonde steckt vermutlich in einem Brush
                                                a.2 += 1;
                                                nn = -nn;
                                            }
                                            let hp = p + d * t;
                                            let mut l = direct(bvh, scene, hp, nn);
                                            if let Some(g) = &bounce {
                                                l += g.eval(hp + nn * 2.0, nn) / std::f32::consts::PI;
                                            }
                                            tri.albedo * l
                                        }
                                    }
                                    None => scene.ambient * 0.5,
                                };
                                // SH-Projektion (uniforme Kugel-Abtastung, Gewicht 4π/N wird beim Auswerten angewandt)
                                for c in 0..3 {
                                    a.0[c][0] += radiance[c] * 0.282095;
                                    a.0[c][1] += radiance[c] * 0.488603 * d.x;
                                    a.0[c][2] += radiance[c] * 0.488603 * d.y;
                                    a.0[c][3] += radiance[c] * 0.488603 * d.z;
                                }
                                a.1 += 1;
                            }
                        }
                    });
                }
            });
            if stop.load(Ordering::SeqCst) {
                return;
            }
            // Raster aus den Summen: E(n) = π·L00·Y00 + 2π/3·Σ L1m·Y1m(n)
            let mut valid = vec![false; n];
            for i in 0..n {
                let (c, rays, back) = acc[i];
                if rays == 0 {
                    continue;
                }
                valid[i] = (back as f32) < rays as f32 * 0.25;
                let w = 4.0 * std::f32::consts::PI / rays as f32;
                for ch in 0..3 {
                    let l00 = c[ch][0] * w;
                    let l1 = [c[ch][1] * w, c[ch][2] * w, c[ch][3] * w];
                    let k0 = std::f32::consts::PI * 0.282095;
                    let k1 = 2.0 * std::f32::consts::PI / 3.0 * 0.488603;
                    grid.sh[i][ch] = [k0 * l00, k1 * l1[0], k1 * l1[1], k1 * l1[2]];
                }
            }
            dilate(&mut grid, &valid);
            smooth(&mut grid);
            grid.version = pass as u64 + 1;
            let g = Arc::new(grid.clone());
            prev = Some(g.clone());
            if tx.send((g, pass + 1, PASSES)).is_err() {
                return;
            }
        }
    });
    Job { rx, cancel }
}

/// Leichte räumliche Glättung (1-2-1 je Achse) gegen Rauschen der Zufallsstrahlen
fn smooth(g: &mut Grid) {
    for axis in 0..3 {
        let src = g.sh.clone();
        let d = g.dims;
        for z in 0..d[2] {
            for y in 0..d[1] {
                for x in 0..d[0] {
                    let mut p = [x, y, z];
                    let i = g.idx(x, y, z);
                    let mut sum = src[i].map(|c| c.map(|v| v * 2.0));
                    let mut w = 2.0;
                    for off in [-1i32, 1] {
                        let q = p[axis] as i32 + off;
                        if q < 0 || q >= d[axis] as i32 {
                            continue;
                        }
                        p[axis] = q as usize;
                        let j = g.idx(p[0], p[1], p[2]);
                        for c in 0..3 {
                            for k in 0..4 {
                                sum[c][k] += src[j][c][k];
                            }
                        }
                        w += 1.0;
                        p = [x, y, z];
                    }
                    g.sh[i] = sum.map(|c| c.map(|v| v / w));
                }
            }
        }
    }
}

/// Sonden in Wänden übernehmen den Mittelwert gültiger Nachbarn (sonst dunkle Flecken an Wänden)
fn dilate(g: &mut Grid, valid: &[bool]) {
    let mut ok = valid.to_vec();
    for _ in 0..3 {
        let mut next = ok.clone();
        let src = g.sh.clone();
        for z in 0..g.dims[2] {
            for y in 0..g.dims[1] {
                for x in 0..g.dims[0] {
                    let i = g.idx(x, y, z);
                    if ok[i] {
                        continue;
                    }
                    let mut sum = [[0.0f32; 4]; 3];
                    let mut cnt = 0;
                    for (dx, dy, dz) in [(-1i32, 0i32, 0i32), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)] {
                        let (nx, ny, nz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                        if nx < 0 || ny < 0 || nz < 0 || nx >= g.dims[0] as i32 || ny >= g.dims[1] as i32 || nz >= g.dims[2] as i32 {
                            continue;
                        }
                        let j = g.idx(nx as usize, ny as usize, nz as usize);
                        if ok[j] {
                            for c in 0..3 {
                                for k in 0..4 {
                                    sum[c][k] += src[j][c][k];
                                }
                            }
                            cnt += 1;
                        }
                    }
                    if cnt > 0 {
                        for c in 0..3 {
                            for k in 0..4 {
                                g.sh[i][c][k] = sum[c][k] / cnt as f32;
                            }
                        }
                        next[i] = true;
                    }
                }
            }
        }
        ok = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, albedo: Vec3) -> [Tri; 2] {
        [Tri { a, b, c, albedo, sky: false }, Tri { a, b: c, c: d, albedo, sky: false }]
    }

    /// Geschlossene Kiste 0..512, rote Wand bei x = 0, Licht oben in der Mitte
    fn room() -> Scene {
        let (l, h) = (0.0f32, 512.0f32);
        let w = Vec3::splat(0.7);
        let red = Vec3::new(0.9, 0.05, 0.05);
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let mut tris = Vec::new();
        tris.extend(quad(p(l, l, l), p(h, l, l), p(h, h, l), p(l, h, l), w)); // Boden
        tris.extend(quad(p(l, l, h), p(l, h, h), p(h, h, h), p(h, l, h), w)); // Decke
        tris.extend(quad(p(l, l, l), p(l, h, l), p(l, h, h), p(l, l, h), red)); // x = 0
        tris.extend(quad(p(h, l, l), p(h, l, h), p(h, h, h), p(h, h, l), w));
        tris.extend(quad(p(l, l, l), p(l, l, h), p(h, l, h), p(h, l, l), w));
        tris.extend(quad(p(l, h, l), p(h, h, l), p(h, h, h), p(l, h, h), w));
        Scene { tris, lights: vec![Light { pos: p(256.0, 256.0, 450.0), color: Vec3::splat(3.0), spot: None }], ambient: Vec3::ZERO, sun_dir: -Vec3::Z, sun_col: Vec3::ZERO, ambient_scale: 1.0 }
    }

    #[test]
    fn bvh_hits() {
        let s = room();
        let b = Bvh::build(s.tris.clone());
        let (t, i) = b.hit(Vec3::new(256.0, 256.0, 256.0), -Vec3::Z, 1e5).unwrap();
        assert!((t - 256.0).abs() < 1e-2);
        assert_eq!(b.tri(i).a.z, 0.0);
        assert!(b.occluded(Vec3::new(256.0, 256.0, 256.0), Vec3::X, 1e5));
        assert!(!b.occluded(Vec3::new(256.0, 256.0, 256.0), Vec3::X, 100.0));
    }

    #[test]
    fn color_bleeding() {
        let job = start(room());
        let mut last = None;
        while let Ok(m) = job.rx.recv() {
            last = Some(m);
        }
        let (g, pass, total) = last.unwrap();
        assert_eq!(pass, total);
        // nahe der roten Wand, Blick zur Wand (-x): deutlich mehr Rot als Grün
        let near_red = g.eval(Vec3::new(40.0, 256.0, 200.0), -Vec3::X);
        assert!(near_red.x > near_red.y * 2.0, "{near_red:?}");
        // gegenüber (weiße Wand) kaum Farbstich
        let near_white = g.eval(Vec3::new(470.0, 256.0, 200.0), Vec3::X);
        assert!(near_white.x < near_white.y * 1.5, "{near_white:?}");
        // der Boden wird von der hellen Decke indirekt beleuchtet
        let floor = g.eval(Vec3::new(256.0, 256.0, 20.0), Vec3::Z);
        assert!(floor.length() > 0.01, "{floor:?}");
    }
}
