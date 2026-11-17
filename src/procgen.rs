//! Prozedurale Werkzeuge: Labyrinth, Räume mit Gängen, Stadtblock und Kreis-Anordnung der Auswahl.
//! Ergebnisse sind gewöhnliche Brushes (zu großen Rechtecken zusammengefasst, damit es wenige
//! bleiben) und werden ausgewählt – ein Rückgängig-Schritt. Bei „Räume“ ist jeder Raum und jeder
//! Gang eine eigene Gruppe, damit man sie einzeln anklicken und bearbeiten kann.

use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use crate::map::{fmt_vec, Entity};
use crate::transform::Xform;
use glam::DVec3;

/// Einstellungen der Generatoren (im Dialog bearbeitet)
#[derive(Clone, Debug)]
pub struct GenOpts {
    pub seed: u64,
    /// Wände
    pub material: String,
    pub floor_material: String,
    pub ceiling_material: String,
    pub height: f64,
    pub wall: f64,
    // Labyrinth
    pub maze_w: usize,
    pub maze_h: usize,
    pub maze_cell: f64,
    // Räume
    pub rooms: usize,
    pub room_min: usize,
    pub room_max: usize,
    pub unit: f64,
    pub grid: usize,
    pub corridor: usize,
    pub lights: bool,
    /// Raumhöhe je Raum zufällig zwischen min und max
    pub room_h_min: f64,
    pub room_h_max: f64,
    /// Höhenstufen der Böden (0 = alle gleich hoch) und Höhe einer Stufe
    pub levels: usize,
    pub level_step: f64,
    /// Höhe der Gänge
    pub corridor_h: f64,
    /// Mindestabstand zwischen Räumen (Zellen) – größer = längere Gänge
    pub room_gap: usize,
    /// Zusätzliche Verbindungen über den Verbindungsbaum hinaus (0..1) – Schleifen und Kreuzungen
    pub extra_links: f64,
    /// Windungen der Gänge (0 = möglichst gerade, 1 = stark gewunden)
    pub winding: f64,
    /// Abbiegungen als echte Kurven (runde Wand außen und innen) statt Ecken
    pub curves: bool,
    /// Innenradius der Kurven in Zellen (0 = nur außen gerundet)
    pub curve_r: usize,
    // Stadt
    pub blocks_x: usize,
    pub blocks_y: usize,
    pub block: f64,
    pub street: f64,
    pub bld_min: f64,
    pub bld_max: f64,
    // Kreis
    pub ring_count: usize,
    pub ring_angle: f64,
}

impl Default for GenOpts {
    fn default() -> Self {
        GenOpts {
            seed: 1,
            material: "DEV/DEV_MEASUREGENERIC01".into(),
            floor_material: "DEV/DEV_MEASUREGENERIC01B".into(),
            ceiling_material: "DEV/DEV_MEASUREGENERIC01".into(),
            height: 128.0,
            wall: 16.0,
            maze_w: 8,
            maze_h: 8,
            maze_cell: 128.0,
            rooms: 6,
            room_min: 3,
            room_max: 7,
            unit: 64.0,
            grid: 40,
            corridor: 2,
            lights: true,
            room_h_min: 128.0,
            room_h_max: 256.0,
            levels: 2,
            level_step: 64.0,
            corridor_h: 128.0,
            room_gap: 5,
            extra_links: 0.3,
            winding: 0.5,
            curves: true,
            curve_r: 1,
            blocks_x: 4,
            blocks_y: 3,
            block: 512.0,
            street: 192.0,
            bld_min: 128.0,
            bld_max: 768.0,
            ring_count: 8,
            ring_angle: 360.0,
        }
    }
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn u(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, a: usize, b: usize) -> usize {
        if b <= a {
            return a;
        }
        a + (self.u() % (b - a + 1) as u64) as usize
    }
    fn f(&mut self) -> f64 {
        (self.u() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Belegte Zellen zu möglichst großen Rechtecken zusammenfassen: (x0, y0, x1, y1) inklusiv
pub fn rects(occ: &[Vec<bool>]) -> Vec<(usize, usize, usize, usize)> {
    let h = occ.len();
    let w = occ.first().map_or(0, |r| r.len());
    let mut used = vec![vec![false; w]; h];
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !occ[y][x] || used[y][x] {
                continue;
            }
            let mut x1 = x;
            while x1 + 1 < w && occ[y][x1 + 1] && !used[y][x1 + 1] {
                x1 += 1;
            }
            let mut y1 = y;
            'grow: while y1 + 1 < h {
                for xx in x..=x1 {
                    if !occ[y1 + 1][xx] || used[y1 + 1][xx] {
                        break 'grow;
                    }
                }
                y1 += 1;
            }
            for row in used.iter_mut().take(y1 + 1).skip(y) {
                for c in row.iter_mut().take(x1 + 1).skip(x) {
                    *c = true;
                }
            }
            out.push((x, y, x1, y1));
        }
    }
    out
}

/// Sammelt neue Objekte; jedes gehört optional zu einer Gruppe (z. B. ein Raum oder Gang)
struct Builder<'a> {
    ed: &'a mut Editor,
    made: Vec<(Obj, Option<usize>)>,
    /// Gruppe für die nächsten Objekte
    owner: Option<usize>,
}

impl<'a> Builder<'a> {
    fn new(ed: &'a mut Editor) -> Self {
        ed.checkpoint();
        Builder { ed, made: Vec::new(), owner: None }
    }
    fn block(&mut self, min: DVec3, max: DVec3, mat: &str) {
        if (max - min).min_element() < 0.5 {
            return;
        }
        let s = self.ed.map.make_box(Aabb::new(min, max), mat, self.ed.tex_scale);
        self.made.push((Obj::Solid(s.id), self.owner));
        self.ed.map.world.solids.push(s.into());
    }
    /// Senkrechtes Prisma über einem Dreieck (x/y relativ zu `origin`) von z0 bis z1
    fn prism(&mut self, origin: DVec3, tri: [DVec3; 3], z0: f64, z1: f64, mat: &str) {
        let area = (tri[1] - tri[0]).truncate().perp_dot((tri[2] - tri[0]).truncate()).abs() / 2.0;
        if area < 4.0 || z1 - z0 < 0.5 {
            return;
        }
        let pts: Vec<DVec3> = tri.iter().flat_map(|p| [origin + DVec3::new(p.x, p.y, z0), origin + DVec3::new(p.x, p.y, z1)]).collect();
        if let Some(s) = self.ed.map.solid_from_points(&pts, mat, self.ed.tex_scale) {
            self.made.push((Obj::Solid(s.id), self.owner));
            self.ed.map.world.solids.push(s.into());
        }
    }
    fn entity(&mut self, class: &str, origin: DVec3, kv: &[(&str, &str)]) {
        let id = self.ed.map.alloc_id();
        let mut e = Entity::new(id, class);
        e.set("origin", fmt_vec(origin));
        for (k, v) in kv {
            e.set(k, *v);
        }
        self.made.push((Obj::Entity(id), self.owner));
        self.ed.map.entities.push(e.into());
    }
    /// Gruppen bilden (je Besitzer eine, `all` = alles in eine), alles auswählen
    fn finish(self, what: &str, all_in_one: bool) -> usize {
        let n = self.made.len();
        let hold = self.ed.checkpoint_hold;
        self.ed.checkpoint_hold = 2;
        let mut groups: std::collections::BTreeMap<usize, Vec<Obj>> = std::collections::BTreeMap::new();
        for (o, ow) in &self.made {
            match (all_in_one, ow) {
                (true, _) => groups.entry(0).or_default().push(*o),
                (false, Some(k)) => groups.entry(*k).or_default().push(*o),
                _ => {}
            }
        }
        let mut ngroups = 0;
        for objs in groups.values().filter(|v| v.len() >= 2) {
            self.ed.sel = objs.iter().copied().collect();
            self.ed.group_selection();
            ngroups += 1;
        }
        self.ed.checkpoint_hold = hold;
        self.ed.sel = self.made.iter().map(|m| m.0).collect();
        self.ed.status = if !all_in_one && ngroups > 1 {
            format!("{what}: {} in {} Gruppen", crate::i18n::count_de(n, "Objekt", "Objekte"), ngroups)
        } else {
            format!("{what}: {}", crate::i18n::count_de(n, "Objekt", "Objekte"))
        };
        self.ed.touch();
        n
    }
}

/// Labyrinth (Tiefensuche) mit Boden, Ursprung unten links
pub fn maze(ed: &mut Editor, o: &GenOpts, origin: DVec3) -> usize {
    let (cw, ch) = (o.maze_w.clamp(2, 64), o.maze_h.clamp(2, 64));
    let (gw, gh) = (cw * 2 + 1, ch * 2 + 1);
    // true = Wand
    let mut occ = vec![vec![true; gw]; gh];
    let mut rng = Rng::new(o.seed);
    let mut stack = vec![(0usize, 0usize)];
    let mut seen = vec![vec![false; cw]; ch];
    seen[0][0] = true;
    occ[1][1] = false;
    while let Some(&(x, y)) = stack.last() {
        let mut next = Vec::new();
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx >= 0 && ny >= 0 && (nx as usize) < cw && (ny as usize) < ch && !seen[ny as usize][nx as usize] {
                next.push((nx as usize, ny as usize));
            }
        }
        if next.is_empty() {
            stack.pop();
            continue;
        }
        let (nx, ny) = next[rng.range(0, next.len() - 1)];
        seen[ny][nx] = true;
        occ[ny * 2 + 1][nx * 2 + 1] = false;
        occ[y + ny + 1][x + nx + 1] = false;
        stack.push((nx, ny));
    }
    // Eingang und Ausgang
    occ[1][0] = false;
    occ[gh - 2][gw - 1] = false;
    // Spalten/Zeilen: Wände schmal, Gänge breit
    let size = |i: usize| if i % 2 == 0 { o.wall } else { o.maze_cell };
    let pre = |n: usize| -> Vec<f64> {
        let mut v = vec![0.0];
        for i in 0..n {
            v.push(v[i] + size(i));
        }
        v
    };
    let (px, py) = (pre(gw), pre(gh));
    let mut b = Builder::new(ed);
    let (w_tot, h_tot) = (px[gw], py[gh]);
    b.block(origin - DVec3::Z * o.wall, origin + DVec3::new(w_tot, h_tot, 0.0), &o.floor_material);
    for (x0, y0, x1, y1) in rects(&occ) {
        b.block(origin + DVec3::new(px[x0], py[y0], 0.0), origin + DVec3::new(px[x1 + 1], py[y1 + 1], o.height), &o.material);
    }
    b.finish(crate::i18n::tr("Labyrinth"), true)
}

/// Wer eine offene Zelle besitzt
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Owner {
    Room(usize),
    Corridor(usize),
}

/// Treppe vor einer Tür: Rechteck in Zellen (inklusiv), Laufrichtung von der Tür weg,
/// Höhe an der Tür und am Gang
struct Stair {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    dir: (i64, i64),
    z_door: f64,
    z_out: f64,
    owner: usize,
}

/// Raster-Zustand beim Bauen der Räume und Gänge
struct Grid {
    n: usize,
    owner: Vec<Vec<Option<Owner>>>,
    /// Ring um einen Raum (Wandzellen) – hier dürfen Gänge nur als Tür hindurch
    ring: Vec<Vec<Option<usize>>>,
    /// Tür- und Treppenzellen: Wegsuche darf nicht hindurch
    locked: Vec<Vec<bool>>,
    /// Mittellinie vorhandener Gänge (Wiederverwendung = Abzweigung statt Parallelgang)
    center: Vec<Vec<bool>>,
    fz: Vec<Vec<f64>>,
    cz: Vec<Vec<f64>>,
    /// Welcher Gang eine Zelle benutzt: -1 keiner, -2 mehrere (Kreuzung)
    link_of: Vec<Vec<i64>>,
    /// Zelle gehört zu einer Kurve (dort keine zweite Kurve)
    curve: Vec<Vec<bool>>,
}

impl Grid {
    fn mark(&mut self, x: usize, y: usize, link: usize) {
        let v = &mut self.link_of[y][x];
        *v = if *v == -1 || *v == link as i64 { link as i64 } else { -2 };
    }
    fn inside(&self, x: i64, y: i64) -> bool {
        x >= 1 && y >= 1 && (x as usize) < self.n - 1 && (y as usize) < self.n - 1
    }
    fn free(&self, x: usize, y: usize) -> bool {
        self.owner[y][x].is_none() && self.ring[y][x].is_none()
    }
    fn set(&mut self, x: usize, y: usize, ow: Owner, f: f64, c: f64, undo: &mut Vec<(usize, usize)>) {
        if self.owner[y][x].is_none() {
            self.owner[y][x] = Some(ow);
            self.fz[y][x] = f;
            self.cz[y][x] = c;
            undo.push((x, y));
        }
    }
}

/// Glattes Zufallsfeld 0..1 (Wert-Rauschen auf einem 4er-Gitter) für gewundene Gänge
fn value_noise(seed: u64, x: usize, y: usize) -> f64 {
    let h = |i: i64, j: i64| -> f64 {
        let mut v = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
        v ^= v >> 29;
        v = v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        v ^= v >> 32;
        (v >> 11) as f64 / (1u64 << 53) as f64
    };
    let (fx, fy) = (x as f64 / 4.0, y as f64 / 4.0);
    let (i, j) = (fx.floor() as i64, fy.floor() as i64);
    let (tx, ty) = (fx - i as f64, fy - j as f64);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let a = h(i, j) + (h(i + 1, j) - h(i, j)) * s(tx);
    let b = h(i, j + 1) + (h(i + 1, j + 1) - h(i, j + 1)) * s(tx);
    a + (b - a) * s(ty)
}

/// Wegsuche (Dijkstra mit Richtung und Lauflänge) für die Mittellinie eines Gangs der Breite `cw`.
/// Zwischen zwei Abbiegungen liegen mindestens `lmin` Zellen gerader Gang – so entstehen lange
/// Geraden statt Treppchen-Zickzack, und an jeder Abbiegung ist Platz für eine Kurve.
fn route(g: &Grid, cw: usize, seed: u64, winding: f64, lmin: usize, start: (usize, usize), goal: (usize, usize)) -> Option<(Vec<(usize, usize)>, f64)> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let n = g.n;
    // Kosten einer Mittellinienzelle (None = gesperrt)
    let cost = |x: usize, y: usize| -> Option<f64> {
        if (x, y) == goal || (x, y) == start {
            return Some(0.1);
        }
        let mut overlaps = false;
        for yy in y..y + cw {
            for xx in x..x + cw {
                if !g.inside(xx as i64, yy as i64) || g.locked[yy][xx] || g.ring[yy][xx].is_some() || matches!(g.owner[yy][xx], Some(Owner::Room(_))) {
                    return None;
                }
                overlaps |= g.owner[yy][xx].is_some();
            }
        }
        if g.center[y][x] {
            return Some(0.3); // vorhandenen Gang mitbenutzen → Abzweigung
        }
        let mut c = 1.0 + winding * 4.0 * value_noise(seed, x, y);
        if overlaps {
            c += 6.0; // halb über einem anderen Gang: würde breite Doppelgänge ergeben
        } else {
            // direkt daneben ein anderer Gang (nur eine Wand dazwischen) → vermeiden
            let near = (y.saturating_sub(2)..(y + cw + 2).min(n)).any(|yy| (x.saturating_sub(2)..(x + cw + 2).min(n)).any(|xx| matches!(g.owner[yy][xx], Some(Owner::Corridor(_)))));
            if near {
                c += 3.0;
            }
        }
        // direkt an einem Raum entlang (nur dessen Wand dazwischen) → lieber mit Abstand
        let hug = (y.saturating_sub(1)..(y + cw + 1).min(n)).any(|yy| (x.saturating_sub(1)..(x + cw + 1).min(n)).any(|xx| g.ring[yy][xx].is_some()));
        if hug {
            c += 1.5;
        }
        Some(c)
    };
    let dirs = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)];
    // Zustand: Zelle, Richtung, Zellen seit der letzten Abbiegung (gedeckelt bei lmin)
    let lmin = lmin.max(1);
    let ns = lmin + 1;
    let idx = |x: usize, y: usize, d: usize, r: usize| ((y * n + x) * 4 + d) * ns + r;
    let cell_of = |i: usize| (i / ns / 4 % n, i / ns / 4 / n);
    let mut dist = vec![f64::MAX; n * n * 4 * ns];
    let mut prev = vec![usize::MAX; n * n * 4 * ns];
    let mut heap = BinaryHeap::new();
    for d in 0..4 {
        let i = idx(start.0, start.1, d, lmin);
        dist[i] = 0.0;
        heap.push(Reverse((0u64, i)));
    }
    // Abbiegen kostet deutlich mehr als ein gerader Schritt; mit mehr Windungen etwas weniger
    let turn_cost = 5.0 - 3.0 * winding;
    let mut found = None;
    while let Some(Reverse((dk, i))) = heap.pop() {
        let dcur = dk as f64 / 1000.0;
        if dcur > dist[i] + 1e-9 {
            continue;
        }
        let (x, y) = cell_of(i);
        let (d, run) = (i / ns % 4, i % ns);
        if (x, y) == goal {
            found = Some(i);
            break;
        }
        for (nd, (dx, dy)) in dirs.iter().enumerate() {
            let turn = nd != d;
            // nicht umkehren; abbiegen erst nach lmin geraden Zellen
            if turn && (nd ^ 1 == d || run < lmin) {
                continue;
            }
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx < 0 || ny < 0 || nx as usize >= n || ny as usize >= n {
                continue;
            }
            let Some(c) = cost(nx as usize, ny as usize) else { continue };
            let nr = if turn { 1 } else { (run + 1).min(lmin) };
            let ni = idx(nx as usize, ny as usize, nd, nr);
            let nv = dcur + c + if turn { turn_cost } else { 0.0 };
            if nv < dist[ni] {
                dist[ni] = nv;
                prev[ni] = i;
                heap.push(Reverse(((nv * 1000.0) as u64, ni)));
            }
        }
    }
    let mut i = found?;
    let total = dist[i];
    let mut path = vec![cell_of(i)];
    while prev[i] != usize::MAX {
        i = prev[i];
        let c = cell_of(i);
        if path.last() != Some(&c) {
            path.push(c);
        }
    }
    path.reverse();
    Some((path, total))
}

/// Kurve an einer Abbiegung, in Zellen-Koordinaten: Mittelpunkt `c`, äußere Ecke `q`, Richtung vor
/// (`d`) und nach (`e`) der Kurve, Innen- und Außenradius
struct Curve {
    c: (f64, f64),
    q: (f64, f64),
    d: (f64, f64),
    e: (f64, f64),
    r_in: f64,
    r_out: f64,
    link: usize,
}

impl Curve {
    /// Punkt auf dem Bogen mit Radius r, t = 0 (Ende, Richtung d) … 1 (Anfang, Richtung −e)
    fn arc(&self, r: f64, t: f64) -> (f64, f64) {
        let a = t * std::f64::consts::FRAC_PI_2;
        let (cs, sn) = (a.cos(), a.sin());
        (self.c.0 + r * (cs * self.d.0 - sn * self.e.0), self.c.1 + r * (cs * self.d.1 - sn * self.e.1))
    }
}

/// Abbiegungen der Gänge zu Kurven machen, wo Platz ist: Das Quadrat zwischen Kurvenmittelpunkt und
/// äußerer Ecke wird zum Gang (Boden und Decke), die Bereiche außerhalb des Kreisrings werden später
/// mit Wandkeilen gefüllt. Nur wo vorher und nachher gerader Gang liegt, keine Kreuzung, Tür oder
/// Treppe berührt wird und der Platz an der Innenseite frei ist – sonst bleibt die Ecke.
fn make_curves(g: &mut Grid, paths: &[(usize, Vec<(usize, usize)>)], cw: usize, k: usize, corr_h: f64) -> Vec<Curve> {
    let mut out = Vec::new();
    let n = g.n as i64;
    let dirv = |a: (usize, usize), b: (usize, usize)| (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64);
    for (link, path) in paths {
        let link = *link;
        let mut i = 1;
        while i + 1 < path.len() {
            let d = dirv(path[i - 1], path[i]);
            let e = dirv(path[i], path[i + 1]);
            if d == e || i < k || i + k >= path.len() || (1.max(i.saturating_sub(k))..=i).any(|j| dirv(path[j - 1], path[j]) != d) || (i + 1..=i + k).any(|j| dirv(path[j - 1], path[j]) != e) {
                i += 1;
                continue;
            }
            let (bx, by) = (path[i].0 as f64, path[i].1 as f64);
            let h = cw as f64 / 2.0;
            let (mx, my) = (bx + h, by + h);
            let (df, ef) = ((d.0 as f64, d.1 as f64), (e.0 as f64, e.1 as f64));
            let q = (mx + h * (df.0 - ef.0), my + h * (df.1 - ef.1));
            let p = (mx + h * (ef.0 - df.0), my + h * (ef.1 - df.1));
            let c = (p.0 + k as f64 * (ef.0 - df.0), p.1 + k as f64 * (ef.1 - df.1));
            let (sx0, sx1) = (c.0.min(q.0).round() as i64, c.0.max(q.0).round() as i64);
            let (sy0, sy1) = (c.1.min(q.1).round() as i64, c.1.max(q.1).round() as i64);
            if sx0 < 1 || sy0 < 1 || sx1 > n - 1 || sy1 > n - 1 {
                i += 1;
                continue;
            }
            let in_s = |x: i64, y: i64| x >= sx0 && x < sx1 && y >= sy0 && y < sy1;
            // Zellen des L (gerader Gang vor und nach der Abbiegung)
            let mut l_cells = std::collections::HashSet::new();
            for &(px, py) in &path[i - k..=i + k] {
                for y in py..py + cw {
                    for x in px..px + cw {
                        l_cells.insert((x as i64, y as i64));
                    }
                }
            }
            let mut ok = true;
            'check: for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let (ux, uy) = (x as usize, y as usize);
                    let own = l_cells.contains(&(x, y));
                    let fine = if own {
                        g.owner[uy][ux] == Some(Owner::Corridor(link)) && g.link_of[uy][ux] == link as i64 && !g.locked[uy][ux] && !g.curve[uy][ux]
                    } else {
                        g.owner[uy][ux].is_none() && g.ring[uy][ux].is_none() && !g.curve[uy][ux]
                    };
                    if !fine {
                        ok = false;
                        break 'check;
                    }
                    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, ny) = (x + dx, y + dy);
                        if in_s(nx, ny) || g.owner[ny as usize][nx as usize].is_none() {
                            continue;
                        }
                        // offen nach außen nur an den beiden Enden des Gangs
                        let end = own && ((dx, dy) == (-d.0, -d.1) || (dx, dy) == e) && g.link_of[ny as usize][nx as usize] == link as i64;
                        if !end {
                            ok = false;
                            break 'check;
                        }
                    }
                }
            }
            if !ok {
                i += 1;
                continue;
            }
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let (ux, uy) = (x as usize, y as usize);
                    if g.owner[uy][ux].is_none() {
                        g.owner[uy][ux] = Some(Owner::Corridor(link));
                        g.fz[uy][ux] = 0.0;
                        g.cz[uy][ux] = corr_h;
                        g.link_of[uy][ux] = link as i64;
                    }
                    g.curve[uy][ux] = true;
                }
            }
            out.push(Curve { c, q, d: df, e: ef, r_in: k as f64, r_out: (k + cw) as f64, link });
            i += k.max(1) + 1;
        }
    }
    out
}

/// Räume mit Gängen in 3D. Jeder Raum hat seine eigene Höhe und Bodenstufe; die Gänge liegen auf
/// einer gemeinsamen Höhe, und jede Tür hat eine kurze, gerade Treppe dorthin. Räume werden über
/// einen Verbindungsbaum (jedes Paar höchstens einmal) plus wahlweise zusätzliche Verbindungen
/// verknüpft; die Wegsuche bevorzugt Abzweigungen statt paralleler Gänge und windet sich nach
/// Wunsch. Jeder Raum und jeder Gang wird eine eigene Gruppe; alles bleibt dicht.
pub fn dungeon(ed: &mut Editor, o: &GenOpts, origin: DVec3) -> usize {
    let mut rng = Rng::new(o.seed);
    let snap16 = |v: f64| (v / 16.0).round() * 16.0;
    let u = o.unit.max(16.0);
    let cw = o.corridor.clamp(1, 6);
    let step = snap16(o.level_step.max(16.0));
    // Treppe: Stufen 16 hoch und etwa 24 tief (wie übliche Source-Treppen); die Stufen werden
    // gleichmäßig über die nötigen Zellen verteilt
    let stair_cells = |dz: f64| -> usize { (((dz.abs() / 16.0).round() * 24.0) / u).ceil() as usize };
    let m_max = stair_cells(o.levels as f64 * step);
    let gap = o.room_gap.max(m_max + 3);
    // Raster groß genug für alle Räume samt Abstand (wächst bei Bedarf über die Einstellung hinaus)
    let per_row = (o.rooms.max(1) as f64).sqrt().ceil() as usize;
    let n = o.grid.max(per_row * (o.room_max.max(o.room_min) + gap) + gap + 2).clamp(16, 160);
    // Räume platzieren
    let mut rooms: Vec<(usize, usize, usize, usize)> = Vec::new();
    for _ in 0..o.rooms.clamp(1, 40) * 40 {
        if rooms.len() >= o.rooms {
            break;
        }
        let rmin = o.room_min.max(cw + 1);
        let (w, h) = (rng.range(rmin, o.room_max.max(rmin)), rng.range(rmin, o.room_max.max(rmin)));
        if w + 2 * gap >= n || h + 2 * gap >= n {
            continue;
        }
        let (x, y) = (rng.range(gap, n - w - gap), rng.range(gap, n - h - gap));
        if rooms.iter().any(|&(rx, ry, rw, rh)| x < rx + rw + gap && rx < x + w + gap && y < ry + rh + gap && ry < y + h + gap) {
            continue;
        }
        rooms.push((x, y, w, h));
    }
    if rooms.is_empty() {
        return 0;
    }
    let nr = rooms.len();
    let (hmin, hmax) = (o.room_h_min.max(64.0), o.room_h_max.max(o.room_h_min.max(64.0)));
    let room_h: Vec<f64> = (0..nr).map(|_| snap16(hmin + (hmax - hmin) * rng.f())).collect();
    let corr_h = snap16(o.corridor_h.clamp(72.0, hmin));
    let mut level: Vec<f64> = (0..nr).map(|_| if o.levels == 0 { 0.0 } else { (rng.range(0, o.levels * 2) as f64 - o.levels as f64) * step }).collect();
    let mut g = Grid {
        n,
        owner: vec![vec![None; n]; n],
        ring: vec![vec![None; n]; n],
        locked: vec![vec![false; n]; n],
        center: vec![vec![false; n]; n],
        fz: vec![vec![0.0; n]; n],
        cz: vec![vec![0.0; n]; n],
        link_of: vec![vec![-1; n]; n],
        curve: vec![vec![false; n]; n],
    };
    // Kurven: Innenradius k Zellen; zwischen zwei Abbiegungen Platz für beide Kurven
    let kc = if o.curves { o.curve_r.min(4) } else { 0 };
    let lmin = if o.curves { cw + 2 * kc + 1 } else { (cw + 2).max(4) };
    for (i, &(x, y, w, h)) in rooms.iter().enumerate() {
        for yy in y..y + h {
            for xx in x..x + w {
                g.owner[yy][xx] = Some(Owner::Room(i));
                g.fz[yy][xx] = level[i];
                g.cz[yy][xx] = level[i] + room_h[i];
            }
        }
        for yy in y - 1..=y + h {
            for xx in x - 1..=x + w {
                if g.owner[yy][xx].is_none() {
                    g.ring[yy][xx] = Some(i);
                }
            }
        }
    }
    let center = |r: (usize, usize, usize, usize)| (r.0 as i64 + r.2 as i64 / 2, r.1 as i64 + r.3 as i64 / 2);
    let mut stairs: Vec<Stair> = Vec::new();
    // vorhandene Türen: (Raum, Seite) → Startzelle der Wegsuche
    let mut doors: Vec<(usize, usize, (usize, usize))> = Vec::new();
    // Tür mit Treppe an einer Seite anlegen (oder vorhandene nehmen); liefert die Startzelle
    let make_door = |g: &mut Grid, stairs: &mut Vec<Stair>, doors: &mut Vec<(usize, usize, (usize, usize))>, r: usize, toward: (i64, i64), link: usize, undo: &mut Vec<(usize, usize)>, nst: &mut usize| -> Option<(usize, usize)> {
        let (rx, ry, rw, rh) = rooms[r];
        let c = center(rooms[r]);
        let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
        // Seiten in Vorzugsreihenfolge: 0 = +x, 1 = −x, 2 = +y, 3 = −y
        let sx = if dx >= 0 { 0 } else { 1 };
        let sy = if dy >= 0 { 2 } else { 3 };
        let order = if dx.abs() >= dy.abs() { [sx, sy, 5 - sy, 1 - sx] } else { [sy, sx, 1 - sx, 5 - sy] };
        let dz = level[r];
        let m = stair_cells(dz);
        for side in order {
            if let Some(d) = doors.iter().find(|d| d.0 == r && d.1 == side) {
                return Some(d.2);
            }
            let horizontal = side < 2;
            let (len, pos0) = if horizontal { (rh, ry) } else { (rw, rx) };
            if len < cw {
                continue;
            }
            let t = if horizontal { toward.1 } else { toward.0 } - cw as i64 / 2;
            let p = t.clamp(pos0 as i64, (pos0 + len - cw) as i64) as usize;
            let sgn: i64 = if side % 2 == 0 { 1 } else { -1 };
            let door = match side {
                0 => (rx + rw) as i64,
                1 => rx as i64 - 1,
                2 => (ry + rh) as i64,
                _ => ry as i64 - 1,
            };
            // Spalten/Zeilen entlang der Laufrichtung: Tür, m Treppenzellen, Absatz
            let line: Vec<i64> = (0..=(m as i64 + 1)).map(|k| door + sgn * k).collect();
            let start_main = if sgn > 0 { *line.last().unwrap() } else { *line.last().unwrap() - cw as i64 + 1 };
            let cell = |a: i64, b: usize| if horizontal { (a, b as i64) } else { (b as i64, a) };
            let mut ok = true;
            for (k, a) in line.iter().enumerate() {
                for b in p..p + cw {
                    let (x, y) = cell(*a, b);
                    if !g.inside(x, y) {
                        ok = false;
                        continue;
                    }
                    let (x, y) = (x as usize, y as usize);
                    let fine = if k == 0 { g.owner[y][x].is_none() && g.ring[y][x] == Some(r) } else { g.free(x, y) };
                    ok &= fine;
                }
            }
            // Platz für den Gang hinter dem Absatz
            for a in start_main..start_main + cw as i64 {
                for b in p..p + cw {
                    let (x, y) = cell(a, b);
                    ok &= g.inside(x, y) && (g.free(x as usize, y as usize) || matches!(g.owner[y as usize][x as usize], Some(Owner::Corridor(_))) && !g.locked[y as usize][x as usize]);
                }
            }
            if !ok {
                continue;
            }
            let ow = Owner::Corridor(link);
            let (lo, hi) = (dz.min(0.0), dz.max(0.0));
            for (k, a) in line.iter().enumerate() {
                for b in p..p + cw {
                    let (x, y) = cell(*a, b);
                    let (x, y) = (x as usize, y as usize);
                    if k == 0 {
                        g.set(x, y, ow, dz, dz + corr_h, undo);
                        g.locked[y][x] = true;
                    } else if k <= m {
                        g.set(x, y, ow, lo, hi + corr_h, undo);
                        g.locked[y][x] = true;
                    } else {
                        g.set(x, y, ow, 0.0, corr_h, undo);
                    }
                    g.mark(x, y, link);
                }
            }
            if m > 0 {
                let (a0, a1) = (line[1].min(line[m]), line[1].max(line[m]));
                let (x0, y0) = cell(a0, p);
                let (x1, y1) = cell(a1, p + cw - 1);
                let dir = if horizontal { (sgn, 0) } else { (0, sgn) };
                stairs.push(Stair { x0: x0 as usize, y0: y0 as usize, x1: x1 as usize, y1: y1 as usize, dir, z_door: dz, z_out: 0.0, owner: link });
                *nst += 1;
            }
            let start = if horizontal { (start_main as usize, p) } else { (p, start_main as usize) };
            doors.push((r, side, start));
            return Some(start);
        }
        None
    };
    // Verbindungsbaum (Prim): immer den nächsten noch nicht verbundenen Raum anschließen
    let dist = |a: usize, b: usize| {
        let (p, q) = (center(rooms[a]), center(rooms[b]));
        (p.0 - q.0).abs() + (p.1 - q.1).abs()
    };
    let start_room = (0..nr).min_by_key(|&i| dist(i, i) + center(rooms[i]).0 + center(rooms[i]).1).unwrap_or(0);
    let mut connected = vec![false; nr];
    connected[start_room] = true;
    let mut links: Vec<(usize, usize)> = Vec::new();
    // Mittellinie jedes Gangs (für die Kurven)
    let mut paths: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
    let try_link = |g: &mut Grid, stairs: &mut Vec<Stair>, doors: &mut Vec<(usize, usize, (usize, usize))>, paths: &mut Vec<(usize, Vec<(usize, usize)>)>, a: usize, b: usize, link: usize| -> bool {
        let mut undo = Vec::new();
        let (ns0, nd0) = (stairs.len(), doors.len());
        let mut nst = 0;
        let sa = make_door(g, stairs, doors, a, center(rooms[b]), link, &mut undo, &mut nst);
        let sb = sa.and_then(|_| make_door(g, stairs, doors, b, center(rooms[a]), link, &mut undo, &mut nst));
        let path = match (sa, sb) {
            // bevorzugt mit langen Geraden; wäre das ein großer Umweg (z. B. einmal um einen Raum
            // herum), lieber mit kurzem Versatz
            (Some(p), Some(q)) => {
                let wind = o.winding.clamp(0.0, 1.0);
                let strict = route(g, cw, o.seed, wind, lmin, p, q);
                let loose = route(g, cw, o.seed, wind, 1, p, q);
                match (strict, loose) {
                    (Some(a), Some(b)) => Some(if a.1 > b.1 * 1.3 + 8.0 { b.0 } else { a.0 }),
                    (a, b) => a.or(b).map(|r| r.0),
                }
            }
            _ => None,
        };
        let Some(path) = path else {
            for (x, y) in undo {
                g.owner[y][x] = None;
                g.locked[y][x] = false;
                g.link_of[y][x] = -1;
            }
            stairs.truncate(ns0);
            doors.truncate(nd0);
            return false;
        };
        for &(cx, cy) in &path {
            g.center[cy][cx] = true;
            for y in cy..cy + cw {
                for x in cx..cx + cw {
                    g.set(x, y, Owner::Corridor(link), 0.0, corr_h, &mut undo);
                    g.mark(x, y, link);
                }
            }
        }
        paths.push((link, path));
        true
    };
    loop {
        let mut cands: Vec<(i64, usize, usize)> = Vec::new();
        for a in (0..nr).filter(|&i| connected[i]) {
            for b in (0..nr).filter(|&i| !connected[i]) {
                cands.push((dist(a, b), a, b));
            }
        }
        if cands.is_empty() {
            break;
        }
        cands.sort();
        let mut done = false;
        for (_, a, b) in cands {
            if try_link(&mut g, &mut stairs, &mut doors, &mut paths, a, b, links.len()) {
                links.push((a, b));
                connected[b] = true;
                done = true;
                break;
            }
        }
        if !done {
            break; // übrige Räume lassen sich nicht anschließen → werden weggelassen
        }
    }
    // zusätzliche Verbindungen (Schleifen, Kreuzungen)
    let extra = ((nr as f64) * o.extra_links.clamp(0.0, 1.0)).round() as usize;
    if extra > 0 {
        let mut cands: Vec<(i64, usize, usize)> = Vec::new();
        for a in 0..nr {
            for b in a + 1..nr {
                if connected[a] && connected[b] && !links.iter().any(|&(p, q)| (p, q) == (a, b) || (p, q) == (b, a)) {
                    cands.push((dist(a, b), a, b));
                }
            }
        }
        cands.sort();
        let mut added = 0;
        for (_, a, b) in cands {
            if added >= extra {
                break;
            }
            if try_link(&mut g, &mut stairs, &mut doors, &mut paths, a, b, links.len()) {
                links.push((a, b));
                added += 1;
            }
        }
    }
    // nicht angeschlossene Räume entfernen
    for (i, &(x, y, w, h)) in rooms.iter().enumerate() {
        if !connected[i] {
            for yy in y..y + h {
                for xx in x..x + w {
                    g.owner[yy][xx] = None;
                }
            }
            level[i] = 0.0;
        }
    }
    let curves = if o.curves { make_curves(&mut g, &paths, cw, kc, corr_h) } else { Vec::new() };
    if std::env::var("AMBOSS_GEN_DEBUG").is_ok() {
        eprintln!("Räume {nr}, Abstand {gap}, verbunden {}, Verbindungen {}, Treppen {}", connected.iter().filter(|c| **c).count(), links.len(), stairs.len());
        for cv in &curves {
            eprintln!("Kurve Mitte {:?} Ecke {:?} d {:?} e {:?} Gang {}", (cv.c.0 * u, cv.c.1 * u), (cv.q.0 * u, cv.q.1 * u), cv.d, cv.e, cv.link);
        }
        for st in &stairs {
            eprintln!("Treppe x {}..{} y {}..{} Richtung {:?} Tür {} Gang {}", st.x0 as f64 * u, (st.x1 + 1) as f64 * u, st.y0 as f64 * u, (st.y1 + 1) as f64 * u, st.dir, st.z_door, st.z_out);
        }
    }
    if let Ok(path) = std::env::var("AMBOSS_GEN_PLAN") {
        // Plan als Bild: Räume grau, Gänge orange, Tür/Treppe blau (nur für Tests)
        let sc = 6usize;
        let mut px = vec![20u8; n * n * sc * sc * 3];
        for y in 0..n {
            for x in 0..n {
                let c: [u8; 3] = match g.owner[y][x] {
                    Some(Owner::Room(_)) => [170, 170, 170],
                    Some(Owner::Corridor(_)) if g.locked[y][x] => [80, 140, 255],
                    Some(Owner::Corridor(_)) if g.curve[y][x] => [120, 210, 90],
                    Some(Owner::Corridor(_)) => [240, 140, 50],
                    None if g.ring[y][x].is_some() => [60, 60, 60],
                    None => [20, 20, 20],
                };
                for yy in 0..sc {
                    for xx in 0..sc {
                        let i = (((n - 1 - y) * sc + yy) * n * sc + x * sc + xx) * 3;
                        px[i..i + 3].copy_from_slice(&c);
                    }
                }
            }
        }
        if let Ok(f) = std::fs::File::create(&path) {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), (n * sc) as u32, (n * sc) as u32);
            enc.set_color(png::ColorType::Rgb);
            if let Ok(mut w) = enc.write_header() {
                let _ = w.write_image_data(&px);
            }
        }
    }
    let Grid { owner, fz: floor_z, cz, .. } = g;
    // Jede offene Zelle bekommt eine Bodenplatte unter und eine Deckenplatte über sich. Stoßen zwei
    // offene Zellen mit verschiedener Höhe aneinander, reicht die Platte der niedrigeren bis zur
    // höheren (Treppen, Türstürze) – so bleibt alles dicht, ohne riesige Säulen über alle Räume.
    let ceil_of = |x: usize, y: usize| cz[y][x];
    let t = o.wall.max(16.0);
    let open = |x: i64, y: i64| -> Option<(usize, usize)> {
        (x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && owner[y as usize][x as usize].is_some()).then_some((x as usize, y as usize))
    };
    let mut slab_lo = vec![vec![0.0f64; n]; n]; // Unterkante der Bodenplatte
    let mut slab_hi = vec![vec![0.0f64; n]; n]; // Oberkante der Deckenplatte
    for y in 0..n {
        for x in 0..n {
            if owner[y][x].is_none() {
                continue;
            }
            let (mut lo, mut hi) = (floor_z[y][x], ceil_of(x, y));
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                if let Some((nx, ny)) = open(x as i64 + dx, y as i64 + dy) {
                    lo = lo.min(floor_z[ny][nx]);
                    hi = hi.max(ceil_of(nx, ny));
                }
            }
            slab_lo[y][x] = lo - t;
            slab_hi[y][x] = hi + t;
        }
    }
    // Wände: nur ein dünner Streifen (Wandstärke) am Rand geschlossener Zellen, die an offene
    // grenzen. Dazu ist jede Zelle in 3×3 Teile geteilt: Ecken (Wandstärke im Quadrat), Seiten
    // (Wandstärke × Rest) und Mitte (nie Wand). Eine Seite wird Wand, wenn die Nachbarzelle dort
    // offen ist; eine Ecke, wenn eine der drei Zellen an dieser Ecke offen ist. Höhe = Platten der
    // angrenzenden offenen Zellen; Wände gehören bevorzugt dem angrenzenden Raum.
    let u = o.unit.max(16.0);
    let wt = o.wall.clamp(1.0, u / 2.0);
    let nf = n * 3;
    let mut wall_owner: Vec<Vec<Option<Owner>>> = vec![vec![None; nf]; nf];
    let mut wall_range = vec![vec![(0.0f64, 0.0f64); nf]; nf];
    for y in 0..n {
        for x in 0..n {
            if owner[y][x].is_some() {
                continue;
            }
            for j in 0..3usize {
                for i in 0..3usize {
                    let (dx, dy) = (i as i64 - 1, j as i64 - 1);
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let cand: &[(i64, i64)] = if dx != 0 && dy != 0 { &[(dx, 0), (0, dy), (dx, dy)] } else { &[(dx, dy)] };
                    let mut range: Option<(f64, f64)> = None;
                    let mut ow_room = None;
                    let mut ow_any = None;
                    for &(cx, cy) in cand {
                        if let Some((nx, ny)) = open(x as i64 + cx, y as i64 + cy) {
                            let r = (slab_lo[ny][nx], slab_hi[ny][nx]);
                            range = Some(range.map_or(r, |q| (q.0.min(r.0), q.1.max(r.1))));
                            match owner[ny][nx] {
                                Some(Owner::Room(k)) if ow_room.is_none() => ow_room = Some(Owner::Room(k)),
                                Some(other) if ow_any.is_none() => ow_any = Some(other),
                                _ => {}
                            }
                        }
                    }
                    if let Some(r) = range {
                        wall_owner[y * 3 + j][x * 3 + i] = ow_room.or(ow_any);
                        wall_range[y * 3 + j][x * 3 + i] = r;
                    }
                }
            }
        }
    }
    // Teilraster-Index → Weltkoordinate (relativ)
    let fine = |k: usize| (k / 3) as f64 * u + [0.0, wt, u - wt][k % 3];
    let mut owners: Vec<Owner> = (0..rooms.len()).map(Owner::Room).collect();
    owners.extend((0..links.len()).map(Owner::Corridor));
    let mut bld = Builder::new(ed);
    // Zellen mit gleichem Schlüssel zu Rechtecken zusammenfassen und als Blöcke setzen
    // (`m` Zellen je Seite, `pos` Index → Weltkoordinate relativ zum Ursprung)
    let emit = |bld: &mut Builder, m: usize, pos: &dyn Fn(usize) -> f64, sel: &dyn Fn(usize, usize) -> Option<(i64, i64)>, mat: &str| {
        let mut keys: Vec<(i64, i64)> = Vec::new();
        for y in 0..m {
            for x in 0..m {
                if let Some(k) = sel(x, y) {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
            }
        }
        for k in keys {
            let occ: Vec<Vec<bool>> = (0..m).map(|y| (0..m).map(|x| sel(x, y) == Some(k)).collect()).collect();
            for (x0, y0, x1, y1) in rects(&occ) {
                let a = origin + DVec3::new(pos(x0), pos(y0), k.0 as f64 / 8.0);
                let b = origin + DVec3::new(pos(x1 + 1), pos(y1 + 1), k.1 as f64 / 8.0);
                bld.block(a, b, mat);
            }
        }
    };
    let cell = |k: usize| k as f64 * u;
    let key = |a: f64, b: f64| ((a * 8.0).round() as i64, (b * 8.0).round() as i64);
    for (gi, ow) in owners.iter().enumerate() {
        bld.owner = Some(gi);
        let mine = |x: usize, y: usize| owner[y][x] == Some(*ow);
        emit(&mut bld, n, &cell, &|x, y| mine(x, y).then(|| key(slab_lo[y][x], floor_z[y][x])), &o.floor_material);
        emit(&mut bld, n, &cell, &|x, y| mine(x, y).then(|| key(ceil_of(x, y), slab_hi[y][x])), &o.ceiling_material);
        emit(&mut bld, nf, &fine, &|x, y| (wall_owner[y][x] == Some(*ow)).then(|| key(wall_range[y][x].0, wall_range[y][x].1)), &o.material);
        // Kurven dieses Gangs: Wandkeile außerhalb des Kreisrings (außen von der Ecke aus, innen
        // vom Mittelpunkt aus aufgefächert), Ecken auf ganze Einheiten gerundet
        if let Owner::Corridor(li) = ow {
            for cv in curves.iter().filter(|c| c.link == *li) {
                let w = |p: (f64, f64)| DVec3::new((p.0 * u).round(), (p.1 * u).round(), 0.0);
                let segs = ((cv.r_out * u * std::f64::consts::FRAC_PI_2) / 40.0).ceil().clamp(4.0, 16.0) as usize;
                for s in 0..segs {
                    let (t0, t1) = (s as f64 / segs as f64, (s + 1) as f64 / segs as f64);
                    bld.prism(origin, [w(cv.q), w(cv.arc(cv.r_out, t0)), w(cv.arc(cv.r_out, t1))], 0.0, corr_h, &o.material);
                    if cv.r_in > 0.0 {
                        bld.prism(origin, [w(cv.c), w(cv.arc(cv.r_in, t0)), w(cv.arc(cv.r_in, t1))], 0.0, corr_h, &o.material);
                    }
                }
            }
        }
        // Treppen dieses Gangs: Stufen 16 hoch, gleichmäßig über die Treppenzellen verteilt
        if let Owner::Corridor(li) = ow {
            for st in stairs.iter().filter(|st| st.owner == *li) {
                let (lo, hi) = (st.z_door.min(st.z_out), st.z_door.max(st.z_out));
                let steps = ((hi - lo) / 16.0).round() as usize;
                if steps == 0 {
                    continue;
                }
                let horizontal = st.dir.0 != 0;
                let (a0, a1) = if horizontal { (st.x0 as f64 * u, (st.x1 + 1) as f64 * u) } else { (st.y0 as f64 * u, (st.y1 + 1) as f64 * u) };
                let run = (a1 - a0) / steps as f64;
                // Ende auf der niedrigen Seite: Tür liegt bei Laufrichtung + am kleinen Ende
                let door_at_min = if horizontal { st.dir.0 > 0 } else { st.dir.1 > 0 };
                let low_at_door = st.z_door < st.z_out;
                let low_at_min = door_at_min == low_at_door;
                for i in 1..=steps {
                    let (s0, s1) = if low_at_min { (a0 + (i - 1) as f64 * run, a0 + i as f64 * run) } else { (a1 - i as f64 * run, a1 - (i - 1) as f64 * run) };
                    let top = lo + i as f64 * 16.0;
                    let (min, max) = if horizontal {
                        (origin + DVec3::new(s0, st.y0 as f64 * u, lo), origin + DVec3::new(s1, (st.y1 + 1) as f64 * u, top))
                    } else {
                        (origin + DVec3::new(st.x0 as f64 * u, s0, lo), origin + DVec3::new((st.x1 + 1) as f64 * u, s1, top))
                    };
                    bld.block(min, max, &o.floor_material);
                }
            }
        }
        if let (true, Owner::Room(i)) = (o.lights && connected[match ow { Owner::Room(i) => *i, _ => 0 }], ow) {
            let r = rooms[*i];
            let (cx, cy) = (r.0 as f64 + r.2 as f64 / 2.0, r.1 as f64 + r.3 as f64 / 2.0);
            let c = origin + DVec3::new(cx * u, cy * u, level[*i]);
            bld.entity("light", c + DVec3::Z * (room_h[*i] - 24.0), &[("_light", "255 240 210 300")]);
            if *i == start_room {
                bld.entity("info_player_start", c + DVec3::Z, &[("angles", "0 0 0")]);
            }
        }
    }
    bld.owner = None;
    bld.finish(crate::i18n::tr("Räume"), false)
}

/// Stadtblock: Straßenraster mit Häusern zufälliger Höhe
pub fn city(ed: &mut Editor, o: &GenOpts, origin: DVec3) -> usize {
    let mut rng = Rng::new(o.seed);
    let (bx, by) = (o.blocks_x.clamp(1, 24), o.blocks_y.clamp(1, 24));
    let pitch = o.block + o.street;
    let mut b = Builder::new(ed);
    let total = DVec3::new(bx as f64 * pitch + o.street, by as f64 * pitch + o.street, 0.0);
    b.block(origin - DVec3::Z * o.wall, origin + total, &o.floor_material);
    for i in 0..bx {
        for j in 0..by {
            let base = origin + DVec3::new(o.street + i as f64 * pitch, o.street + j as f64 * pitch, 0.0);
            // 1–4 Häuser je Block (geteilt längs oder quer)
            let split = rng.range(1, 2);
            let parts = rng.range(1, 2);
            for a in 0..split {
                for c in 0..parts {
                    let (w, d) = (o.block / split as f64, o.block / parts as f64);
                    let min = base + DVec3::new(a as f64 * w + 8.0, c as f64 * d + 8.0, 0.0);
                    let h = o.bld_min + (o.bld_max - o.bld_min).max(0.0) * rng.f();
                    let h = (h / 16.0).round() * 16.0;
                    b.block(min, min + DVec3::new(w - 16.0, d - 16.0, h.max(32.0)), &o.material);
                }
            }
        }
    }
    b.finish(crate::i18n::tr("Stadtblock"), false)
}

/// Auswahl kreisförmig um ihren Mittelpunkt (bzw. den Weltursprung der Auswahl) vervielfältigen
pub fn ring(ed: &mut Editor, o: &GenOpts, pivot: DVec3) -> usize {
    if ed.sel.is_empty() || o.ring_count < 2 {
        return 0;
    }
    let originals = ed.sel.clone();
    let hold = ed.checkpoint_hold;
    ed.checkpoint_hold = 1;
    let step = o.ring_angle / if (o.ring_angle - 360.0).abs() < 1e-6 { o.ring_count as f64 } else { (o.ring_count - 1).max(1) as f64 };
    let mut all = originals.clone();
    for k in 1..o.ring_count {
        ed.sel = originals.clone();
        ed.duplicate_in_place();
        let x = Xform::about(pivot, Xform::rotation(2, step * k as f64));
        ed.transform_selection(&x, false);
        all.extend(ed.sel.iter().copied());
    }
    ed.sel = all;
    ed.checkpoint_hold = hold;
    let n = ed.sel.len();
    ed.status = format!("{} im Kreis angeordnet", crate::i18n::count_de(n, "Objekt", "Objekte"));
    ed.touch();
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_rects_cover_exactly() {
        let occ: Vec<Vec<bool>> = ["##..#", "##..#", "....#", "###.."].iter().map(|r| r.chars().map(|c| c == '#').collect()).collect();
        let r = rects(&occ);
        let mut cover = vec![vec![0; 5]; 4];
        for (x0, y0, x1, y1) in &r {
            for row in cover.iter_mut().take(y1 + 1).skip(*y0) {
                for c in row.iter_mut().take(x1 + 1).skip(*x0) {
                    *c += 1;
                }
            }
        }
        for y in 0..4 {
            for x in 0..5 {
                assert_eq!(cover[y][x], occ[y][x] as i32, "{x},{y}");
            }
        }
        assert!(r.len() <= 4, "{r:?}");
    }

    #[test]
    fn rooms_in_3d_are_sealed_and_separate() {
        let mut multi = 0;
        for seed in 1..16 {
            let mut ed = Editor::new(crate::fgd::Fgd::default());
            let o = GenOpts { seed, levels: 3, level_step: 64.0, room_h_min: 128.0, room_h_max: 320.0, floor_material: "F".into(), material: "W".into(), ceiling_material: "C".into(), ..Default::default() };
            dungeon(&mut ed, &o, DVec3::ZERO);
            // dicht: Flutfüllung ab den Lichtern/dem Spawn findet kein Leck
            let f = crate::optimize::Flood::run(&ed).expect("Welt-Brushes");
            assert!(f.leak.is_none(), "Seed {seed}: Leck von {:?}", f.leak_from);
            // eigene Texturen für Boden, Wände, Decke
            for m in ["F", "W", "C"] {
                assert!(ed.map.world.solids.iter().any(|s| s.sides.iter().any(|sd| sd.material == m)), "Material {m}");
            }
            // Böden auf unterschiedlichen Höhen (Raumböden sind die Oberseiten der Boden-Säulen)
            let tops: std::collections::BTreeSet<i64> = ed.map.world.solids.iter().filter(|s| s.sides.iter().all(|sd| sd.material == "F")).map(|s| s.bounds.max.z as i64).collect();
            if tops.len() >= 2 {
                multi += 1;
            }
            // Treppen: benachbarte Bodenhöhen unterscheiden sich um höchstens 16 innerhalb eines Gangs
            let groups: std::collections::BTreeSet<u32> = ed.map.world.solids.iter().filter_map(|s| s.editor.groupid).collect();
            assert!(groups.len() >= 3);
            // begehbar: das Navmesh ab dem Spawn (Stufen ≤ 18) erreicht jeden Raum (unter jedem Licht)
            let nav = crate::navmesh::generate(&ed.map, 16384.0).expect("Navmesh");
            for l in ed.map.entities.iter().filter(|e| e.classname() == "light") {
                let p = l.origin().unwrap();
                let hit = nav.areas.iter().any(|a| p.x >= a.nw.x.min(a.se.x) - 32.0 && p.x <= a.nw.x.max(a.se.x) + 32.0 && p.y >= a.nw.y.min(a.se.y) - 32.0 && p.y <= a.nw.y.max(a.se.y) + 32.0 && (a.nw.z - p.z).abs() < 400.0);
                assert!(hit, "Seed {seed}: Raum unter Licht {p:?} nicht erreichbar");
            }
        }
        // meistens mehrere Bodenhöhen (flach nur, wenn sich Gänge ungünstig kreuzen)
        assert!(multi >= 11, "nur {multi} von 15 mit Höhenstufen");
    }

    /// Keine zwei Brushes überschneiden sich (Räume liegen nicht ineinander), auch mit dem Startwert
    /// aus der Meldung des Nutzers
    /// Überlappen sich zwei konvexe Grundrisse (mehr als 0,01)? Trennachsen-Test über alle Kanten
    fn footprints_overlap(a: &[glam::DVec2], b: &[glam::DVec2]) -> bool {
        let hull = |p: &[glam::DVec2]| {
            // Punkte um den Schwerpunkt sortieren → Umriss
            let c = p.iter().fold(glam::DVec2::ZERO, |s, q| s + *q) / p.len() as f64;
            let mut v = p.to_vec();
            v.sort_by(|x, y| (x.y - c.y).atan2(x.x - c.x).total_cmp(&(y.y - c.y).atan2(y.x - c.x)));
            v
        };
        let (a, b) = (hull(a), hull(b));
        for poly in [&a, &b] {
            for i in 0..poly.len() {
                let e = poly[(i + 1) % poly.len()] - poly[i];
                let ax = glam::DVec2::new(-e.y, e.x).normalize_or_zero();
                let pa = a.iter().map(|p| p.dot(ax));
                let pb = b.iter().map(|p| p.dot(ax));
                let (a0, a1) = pa.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
                let (b0, b1) = pb.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
                if a1.min(b1) - a0.max(b0) <= 0.01 {
                    return false;
                }
            }
        }
        true
    }

    /// Wände sind dünn (Wandstärke statt ganzer Zelle), Abbiegungen werden zu Kurven, und Gänge
    /// biegen nicht im Zickzack ab
    #[test]
    fn thin_walls_and_curves() {
        let mut total_curves = 0;
        for seed in [119122791515133u64, 1, 2, 3, 7] {
            let mut ed = Editor::new(crate::fgd::Fgd::default());
            let o = GenOpts { seed, material: "W".into(), floor_material: "F".into(), ceiling_material: "C".into(), ..Default::default() };
            dungeon(&mut ed, &o, DVec3::ZERO);
            for s in ed.map.world.solids.iter().filter(|s| s.sides.len() == 6 && s.sides.iter().all(|x| x.material == o.material)) {
                let sz = s.bounds.size();
                assert!(sz.x.min(sz.y) <= o.wall + 0.01, "Seed {seed}: Wand {sz:?} dicker als {}", o.wall);
            }
            total_curves += ed.map.world.solids.iter().filter(|s| s.sides.len() == 5).count();
            let f = crate::optimize::Flood::run(&ed).unwrap();
            assert!(f.leak.is_none(), "Seed {seed}: Leck");
        }
        assert!(total_curves > 20, "Kurven-Keile: {total_curves}");
        // ohne Kurven: keine Keile
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        dungeon(&mut ed, &GenOpts { seed: 3, curves: false, ..Default::default() }, DVec3::ZERO);
        assert!(ed.map.world.solids.iter().all(|s| s.sides.len() == 6));
    }

    #[test]
    fn route_keeps_straights() {
        // freies Feld: Weg von links unten nach rechts oben mit höchstens wenigen Abbiegungen,
        // jede gerade Strecke mindestens lmin lang (außer der letzten)
        let n = 40;
        let g = Grid {
            n,
            owner: vec![vec![None; n]; n],
            ring: vec![vec![None; n]; n],
            locked: vec![vec![false; n]; n],
            center: vec![vec![false; n]; n],
            fz: vec![vec![0.0; n]; n],
            cz: vec![vec![0.0; n]; n],
            link_of: vec![vec![-1; n]; n],
            curve: vec![vec![false; n]; n],
        };
        for seed in 1..6 {
            let p = route(&g, 2, seed, 1.0, 7, (3, 3), (33, 30)).unwrap().0;
            let dirs: Vec<(i64, i64)> = p.windows(2).map(|w| (w[1].0 as i64 - w[0].0 as i64, w[1].1 as i64 - w[0].1 as i64)).collect();
            let mut runs = vec![1usize];
            for w in dirs.windows(2) {
                if w[0] == w[1] {
                    *runs.last_mut().unwrap() += 1;
                } else {
                    runs.push(1);
                }
            }
            assert!(runs[1..runs.len() - 1].iter().all(|r| *r >= 7), "Seed {seed}: {runs:?}");
        }
    }

    #[test]
    fn rooms_do_not_overlap() {
        for seed in [119122791515133u64, 1, 2, 3, 7, 42] {
            let mut ed = Editor::new(crate::fgd::Fgd::default());
            let o = GenOpts { seed, ..Default::default() };
            dungeon(&mut ed, &o, DVec3::ZERO);
            let b: Vec<Aabb> = ed.map.world.solids.iter().map(|s| s.bounds).collect();
            // alle Brushes sind senkrechte Prismen: Grundriss (konvex) + Höhenbereich
            let foot: Vec<Vec<glam::DVec2>> = ed.map.world.solids.iter().map(|s| {
                let mut v: Vec<glam::DVec2> = Vec::new();
                for p in s.vertices() {
                    let q = p.truncate();
                    if !v.iter().any(|w| w.distance(q) < 0.01) {
                        v.push(q);
                    }
                }
                v
            }).collect();
            for i in 0..b.len() {
                for j in i + 1..b.len() {
                    let lo = b[i].min.max(b[j].min);
                    let hi = b[i].max.min(b[j].max);
                    let d = hi - lo;
                    if d.min_element() <= 0.01 {
                        continue;
                    }
                    assert!(!footprints_overlap(&foot[i], &foot[j]), "Seed {seed}: Brushes {i} und {j} überschneiden sich: {:?} / {:?}", b[i], b[j]);
                }
            }
            let f = crate::optimize::Flood::run(&ed).unwrap();
            assert!(f.leak.is_none(), "Seed {seed}: Leck");
            // kein Raum ragt über die Höhe aller anderen hinaus: Raumhöhe = eigene Höhe + Platten
            let tallest = ed.map.world.solids.iter().map(|s| s.bounds.size().z).fold(0.0, f64::max);
            assert!(tallest <= o.room_h_max + o.levels as f64 * o.level_step * 2.0 + 64.0, "Seed {seed}: Brush {tallest} hoch");
        }
    }

    #[test]
    fn generators_build() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let o = GenOpts::default();
        let n = maze(&mut ed, &o, DVec3::ZERO);
        assert!(n > 10 && ed.map.world.solids.len() == n, "{n}");
        // ein Rückgängig-Schritt
        ed.undo();
        assert!(ed.map.world.solids.is_empty());
        let n = dungeon(&mut ed, &o, DVec3::ZERO);
        assert!(n > 10);
        // jeder Raum und Gang eine eigene Gruppe
        let groups: std::collections::BTreeSet<u32> = ed.map.world.solids.iter().filter_map(|s| s.editor.groupid).collect();
        assert!(groups.len() >= o.rooms.min(3), "Gruppen: {}", groups.len());
        assert!(ed.map.entities.iter().any(|e| e.classname() == "info_player_start"));
        assert!(ed.map.entities.iter().filter(|e| e.classname() == "light").count() >= 1);
        // Brushes sind gültig (6 Seiten, Volumen)
        assert!(ed.map.world.solids.iter().all(|s| (s.sides.len() == 6 || s.sides.len() == 5) && s.bounds.size().min_element() > 0.4));
        let before = ed.map.world.solids.len();
        city(&mut ed, &o, DVec3::new(5000.0, 0.0, 0.0));
        assert!(ed.map.world.solids.len() > before + 4);
    }

    #[test]
    fn ring_copies() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let s = ed.map.make_box(Aabb::new(DVec3::new(200.0, -16.0, 0.0), DVec3::new(232.0, 16.0, 32.0)), "X", 0.25);
        let id = s.id;
        ed.map.world.solids.push(s.into());
        ed.sel.insert(Obj::Solid(id));
        let o = GenOpts { ring_count: 4, ..Default::default() };
        ring(&mut ed, &o, DVec3::ZERO);
        assert_eq!(ed.map.world.solids.len(), 4);
        // Kopie bei 90° liegt auf der +Y-Achse
        assert!(ed.map.world.solids.iter().any(|s| s.bounds.center().y > 190.0 && s.bounds.center().x.abs() < 20.0));
        // ein Rückgängig-Schritt für alles
        ed.undo();
        assert_eq!(ed.map.world.solids.len(), 1);
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;
    #[test]
    #[ignore]
    fn debug_seed() {
        let seed: u64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(11);
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let o = GenOpts { seed, ..Default::default() };
        dungeon(&mut ed, &o, DVec3::ZERO);
        let nav = crate::navmesh::generate(&ed.map, 16384.0).unwrap();
        println!("areas {}", nav.areas.len());
        for l in ed.map.entities.iter().filter(|e| e.classname() == "light" || e.classname() == "info_player_start") {
            let p = l.origin().unwrap();
            let near = nav.areas.iter().filter(|a| (a.nw.x.min(a.se.x) - 32.0..=a.nw.x.max(a.se.x) + 32.0).contains(&p.x) && (a.nw.y.min(a.se.y) - 32.0..=a.nw.y.max(a.se.y) + 32.0).contains(&p.y)).map(|a| a.nw.z).collect::<Vec<_>>();
            println!("{} {:?} nav-z {:?}", l.classname(), p, near);
        }
        std::fs::write(std::env::temp_dir().join("amboss_seed.vmf"), ed.map.to_vmf()).unwrap();
    }
}
