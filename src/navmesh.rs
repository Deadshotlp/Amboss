//! Eigener Navmesh-Generator (wie `nav_generate`, aber im Editor und ohne Spiel).
//!
//! Ablauf wie bei Source: Von den Spawnpunkten aus wird ein 25er-Raster abgelaufen. Ein Punkt ist
//! begehbar, wenn darunter Boden ist (Brush oder Displacement, höchstens 45° steil), der Spieler
//! hineinpasst (72 hoch, sonst geduckt 36) und der Weg dorthin nicht durch eine Wand führt
//! (Stufen bis 18 hoch, herunterfallen bis 200). Die Rasterzellen werden zu Rechtecken
//! zusammengefasst und als `.nav` (Version 16, wie Garry's Mod) gespeichert.
//!
//! Nicht berücksichtigt: Kollision von Props (nur Brushes und Displacements), Leitern, Verstecke,
//! Sichtbarkeit zwischen Bereichen (`nav_analyze`).

use crate::map::Map;
use crate::geometry::{ray_convex, Aabb, Plane};
use crate::map::Solid;
use glam::DVec3;
use std::collections::{HashMap, VecDeque};

pub const STEP: f64 = 25.0;
pub const STEP_HEIGHT: f64 = 18.0;
pub const DEATH_DROP: f64 = 200.0;
pub const HULL_HALF: f64 = 16.0;
pub const STAND: f64 = 72.0;
pub const CROUCH: f64 = 36.0;
/// cos(45°): steilere Flächen sind nicht begehbar
pub const MAX_SLOPE_Z: f64 = 0.7;
const MAX_NODES: usize = 400_000;

const NAV_CROUCH: u32 = 0x1;

/// Zähler, warum das Ablaufen an einer Stelle stoppt (zur Fehlersuche)
pub static STOP: [std::sync::atomic::AtomicUsize; 4] = [const { std::sync::atomic::AtomicUsize::new(0) }; 4];
fn stop(i: usize) {
    STOP[i].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

// ------------------------------------------------------------------ Kollision

struct World {
    brushes: Vec<(Aabb, Vec<Option<Plane>>)>,
    tris: Vec<[DVec3; 3]>,
    grid: HashMap<(i32, i32), (Vec<u32>, Vec<u32>)>,
}

const CELL: f64 = 128.0;

fn key(x: f64, y: f64) -> (i32, i32) {
    ((x / CELL).floor() as i32, (y / CELL).floor() as i32)
}

impl World {
    fn new(solids: &[&Solid]) -> World {
        let mut w = World { brushes: Vec::new(), tris: Vec::new(), grid: HashMap::new() };
        for s in solids {
            if s.bounds.is_empty() {
                continue;
            }
            if s.has_disp() {
                // Displacement-Brushes kollidieren nur mit ihrer Oberfläche
                for g in s.disps.iter().flatten() {
                    for t in g.triangles() {
                        let tri = [g.pos[t[0]], g.pos[t[1]], g.pos[t[2]]];
                        let i = w.tris.len() as u32;
                        w.tris.push(tri);
                        let b = Aabb::from_points(tri);
                        w.cells(&b, |e| e.1.push(i));
                    }
                }
                continue;
            }
            let i = w.brushes.len() as u32;
            w.brushes.push((s.bounds, s.planes.clone()));
            w.cells(&s.bounds, |e| e.0.push(i));
        }
        w
    }

    fn cells(&mut self, b: &Aabb, mut f: impl FnMut(&mut (Vec<u32>, Vec<u32>))) {
        let (a, c) = (key(b.min.x, b.min.y), key(b.max.x, b.max.y));
        for x in a.0..=c.0 {
            for y in a.1..=c.1 {
                f(self.grid.entry((x, y)).or_default());
            }
        }
    }

    fn near(&self, b: &Aabb) -> (Vec<u32>, Vec<u32>) {
        let (a, c) = (key(b.min.x, b.min.y), key(b.max.x, b.max.y));
        let (mut br, mut tr) = (Vec::new(), Vec::new());
        for x in a.0..=c.0 {
            for y in a.1..=c.1 {
                if let Some((bv, tv)) = self.grid.get(&(x, y)) {
                    br.extend_from_slice(bv);
                    tr.extend_from_slice(tv);
                }
            }
        }
        br.sort_unstable();
        br.dedup();
        tr.sort_unstable();
        tr.dedup();
        (br, tr)
    }

    /// Erster Treffer eines Strahls: (Abstand, Flächennormale)
    fn ray(&self, o: DVec3, d: DVec3, maxt: f64) -> Option<(f64, DVec3)> {
        let b = Aabb::new(o.min(o + d * maxt), o.max(o + d * maxt));
        let (br, tr) = self.near(&b);
        let mut best: Option<(f64, DVec3)> = None;
        for i in br {
            let (bb, planes) = &self.brushes[i as usize];
            if !bb.intersects(&b) {
                continue;
            }
            if let Some((t, pi)) = ray_convex(planes, o, d) {
                if t > 1e-6 && t <= maxt && best.is_none_or(|x| t < x.0) {
                    let n = planes.get(pi).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
                    best = Some((t, n));
                }
            }
        }
        for i in tr {
            let [a, b2, c] = self.tris[i as usize];
            // Möller–Trumbore, beidseitig
            let (e1, e2) = (b2 - a, c - a);
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = o - a;
            let u = s.dot(p) * inv;
            if !(-1e-6..=1.0 + 1e-6).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = d.dot(q) * inv;
            if v < -1e-6 || u + v > 1.0 + 1e-6 {
                continue;
            }
            let t = e2.dot(q) * inv;
            if t > 1e-6 && t <= maxt && best.is_none_or(|x| t < x.0) {
                let mut n = e1.cross(e2).normalize_or_zero();
                if n.dot(d) > 0.0 {
                    n = -n;
                }
                best = Some((t, n));
            }
        }
        best
    }

    /// Überschneidet die Box einen Brush (Displacements bremsen hier nicht – sie sind Boden)?
    fn box_blocked(&self, b: &Aabb) -> bool {
        let corners = b.corners();
        let (br, _) = self.near(b);
        br.into_iter().any(|i| {
            let (bb, planes) = &self.brushes[i as usize];
            bb.intersects(b) && planes.iter().flatten().all(|p| corners.iter().any(|c| p.distance(*c) < -0.01))
        })
    }
}

/// Brushes, die Spieler blockieren (Welt, func_detail und feste Brush-Entities; Türen gelten als offen)
fn player_solids(map: &Map) -> Vec<&Solid> {
    let passable = ["toolstrigger", "toolshint", "toolsskip", "toolsareaportal", "toolsnpcclip", "toolsinvisibleladder", "toolsorigin", "toolsfog", "toolsblock_los", "toolsoccluder"];
    let all_pass = |s: &Solid| {
        s.sides.iter().all(|x| {
            let m = x.material.to_ascii_lowercase();
            passable.iter().any(|n| m.contains(n)) || m.contains("water")
        })
    };
    let mut v: Vec<&Solid> = map.world.solids.iter().map(|s| &**s).filter(|s| !all_pass(s)).collect();
    for e in &map.entities {
        let c = e.classname().to_ascii_lowercase();
        let solid = matches!(c.as_str(), "func_detail" | "func_brush" | "func_wall" | "func_wall_toggle" | "func_lod" | "func_breakable" | "func_monitor" | "func_reflective_glass" | "func_clip_vphysics");
        if solid {
            v.extend(e.solids.iter().map(|s| &**s).filter(|s| !all_pass(s)));
        }
    }
    v
}

// ------------------------------------------------------------------ Raster ablaufen

#[derive(Clone, Copy)]
struct Node {
    ix: i32,
    iy: i32,
    z: f64,
    normal: DVec3,
    crouch: bool,
    /// erreichbare Nachbarn in Richtung N(−Y), O(+X), S(+Y), W(−X)
    links: [Option<u32>; 4],
}

const DIRS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Boden unter einem Punkt (Höhe, Normale); `from_z` = Starthöhe des Strahls
fn ground(w: &World, x: f64, y: f64, from_z: f64, depth: f64) -> Option<(f64, DVec3)> {
    let (t, n) = w.ray(DVec3::new(x, y, from_z), -DVec3::Z, depth)?;
    (n.z >= MAX_SLOPE_Z).then_some((from_z - t, n))
}

/// Passt der Spieler hinein? Some(geduckt?)
fn clearance(w: &World, x: f64, y: f64, z: f64) -> Option<bool> {
    // wie Source: die Box beginnt erst in Stufenhöhe – alles darunter steigt der Spieler hinauf
    let hull = |h: f64| Aabb::new(DVec3::new(x - HULL_HALF, y - HULL_HALF, z + STEP_HEIGHT), DVec3::new(x + HULL_HALF, y + HULL_HALF, z + h));
    if !w.box_blocked(&hull(STAND)) {
        Some(false)
    } else if !w.box_blocked(&hull(CROUCH)) {
        Some(true)
    } else {
        None
    }
}

pub struct Graph {
    nodes: Vec<Node>,
}

fn walk(limit: f64, w: &World, seeds: &[DVec3]) -> Result<Graph, String> {
    let mut nodes: Vec<Node> = Vec::new();
    // (ix, iy) → Knoten auf verschiedenen Höhen (Etagen)
    let mut at: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    let mut queue = VecDeque::new();
    let find = |at: &HashMap<(i32, i32), Vec<u32>>, nodes: &[Node], ix: i32, iy: i32, z: f64| -> Option<u32> {
        at.get(&(ix, iy))?.iter().copied().find(|&i| (nodes[i as usize].z - z).abs() < 4.0)
    };
    for s in seeds {
        let (ix, iy) = ((s.x / STEP).round() as i32, (s.y / STEP).round() as i32);
        let (x, y) = (ix as f64 * STEP, iy as f64 * STEP);
        let Some((z, n)) = ground(w, x, y, s.z + 24.0, 4096.0) else { continue };
        if find(&at, &nodes, ix, iy, z).is_some() {
            continue;
        }
        let Some(crouch) = clearance(w, x, y, z) else { continue };
        let i = nodes.len() as u32;
        nodes.push(Node { ix, iy, z, normal: n, crouch, links: [None; 4] });
        at.entry((ix, iy)).or_default().push(i);
        queue.push_back(i);
    }
    if nodes.is_empty() {
        return Err("Kein Startpunkt mit Boden gefunden – die Map braucht einen info_player_start (o. ä.) über begehbarem Boden".into());
    }
    while let Some(i) = queue.pop_front() {
        let cur = nodes[i as usize];
        let (cx, cy) = (cur.ix as f64 * STEP, cur.iy as f64 * STEP);
        for (d, (dx, dy)) in DIRS.iter().enumerate() {
            let (nx, ny) = (cur.ix + dx, cur.iy + dy);
            let (x, y) = (nx as f64 * STEP, ny as f64 * STEP);
            if x.abs() > limit || y.abs() > limit {
                continue;
            }
            // Boden beim Nachbarn: höchstens eine Stufe höher, bis DEATH_DROP tiefer
            let Some((z, n)) = ground(w, x, y, cur.z + STEP_HEIGHT + 1.0, STEP_HEIGHT + 1.0 + DEATH_DROP) else {
                stop(0);
                continue;
            };
            // Weg dorthin frei? (in Hüfthöhe und knapp über der Stufe)
            let blocked = [STEP_HEIGHT + 2.0, CROUCH - 2.0].iter().any(|h| {
                let o = DVec3::new(cx, cy, cur.z + h);
                let t = DVec3::new(x, y, cur.z.max(z) + h);
                let dir = t - o;
                w.ray(o, dir.normalize(), dir.length()).is_some()
            });
            if blocked {
                stop(1);
                continue;
            }
            let j = match find(&at, &nodes, nx, ny, z) {
                Some(j) => j,
                None => {
                    let Some(crouch) = clearance(w, x, y, z) else {
                        stop(2);
                        continue;
                    };
                    if nodes.len() >= MAX_NODES {
                        return Err(format!("Mehr als {MAX_NODES} Rasterpunkte – Map zu groß oder nicht abgedichtet?"));
                    }
                    let j = nodes.len() as u32;
                    nodes.push(Node { ix: nx, iy: ny, z, normal: n, crouch, links: [None; 4] });
                    at.entry((nx, ny)).or_default().push(j);
                    queue.push_back(j);
                    j
                }
            };
            nodes[i as usize].links[d] = Some(j);
        }
    }
    Ok(Graph { nodes })
}

// ------------------------------------------------------------------ Bereiche

/// Ein Nav-Bereich (Rechteck aus Rasterzellen)
#[derive(Debug, Clone)]
pub struct Area {
    pub id: u32,
    pub nw: DVec3,
    pub se: DVec3,
    pub ne_z: f64,
    pub sw_z: f64,
    pub crouch: bool,
    /// verbundene Bereiche je Richtung N, O, S, W
    pub connect: [Vec<u32>; 4],
}

impl Area {
    /// Ecken für die Anzeige: NW, NO, SO, SW
    pub fn corners(&self) -> [DVec3; 4] {
        [self.nw, DVec3::new(self.se.x, self.nw.y, self.ne_z), self.se, DVec3::new(self.nw.x, self.se.y, self.sw_z)]
    }
}

/// Zellen zu Rechtecken zusammenfassen (gierig: erst nach rechts, dann Zeilen nach unten)
fn build_areas(g: &Graph) -> Vec<Area> {
    let n = g.nodes.len();
    let mut owner: Vec<u32> = vec![u32::MAX; n];
    // Ebene des Startknotens: neue Zellen müssen darauf liegen
    let fits = |a: &Node, b: &Node| -> bool {
        if a.crouch != b.crouch || a.normal.dot(b.normal) < 0.985 {
            return false;
        }
        let dx = (b.ix - a.ix) as f64 * STEP;
        let dy = (b.iy - a.iy) as f64 * STEP;
        let pz = a.z - (a.normal.x * dx + a.normal.y * dy) / a.normal.z.max(0.1);
        (pz - b.z).abs() < 2.0
    };
    // Nachbar in Richtung d, der in beide Richtungen verbunden ist (keine Stufe dazwischen)
    let mutual = |i: u32, d: usize| -> Option<u32> {
        let j = g.nodes[i as usize].links[d]?;
        (g.nodes[j as usize].links[(d + 2) % 4] == Some(i)).then_some(j)
    };
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by_key(|&i| (g.nodes[i as usize].iy, g.nodes[i as usize].ix));
    let mut areas: Vec<Area> = Vec::new();
    for &start in &order {
        if owner[start as usize] != u32::MAX {
            continue;
        }
        let s = g.nodes[start as usize];
        let id = areas.len() as u32;
        // Zeile nach Osten verlängern
        let mut row = vec![start];
        while row.len() < 32 {
            let last = *row.last().unwrap();
            match mutual(last, 1) {
                Some(j) if owner[j as usize] == u32::MAX && fits(&s, &g.nodes[j as usize]) => row.push(j),
                _ => break,
            }
        }
        let mut rows = vec![row.clone()];
        // weitere Zeilen nach Süden, solange die ganze Zeile passt
        'grow: while rows.len() < 32 {
            let prev = rows.last().unwrap().clone();
            let mut next = Vec::with_capacity(prev.len());
            for (k, &p) in prev.iter().enumerate() {
                match mutual(p, 2) {
                    Some(j) if owner[j as usize] == u32::MAX && fits(&s, &g.nodes[j as usize]) => {
                        // auch seitlich mit dem Vorgänger in der neuen Zeile verbunden
                        if k > 0 && mutual(next[k - 1], 1) != Some(j) {
                            break 'grow;
                        }
                        next.push(j);
                    }
                    _ => break 'grow,
                }
            }
            rows.push(next);
        }
        for r in &rows {
            for &c in r {
                owner[c as usize] = id;
            }
        }
        let (w, h) = (rows[0].len() as f64, rows.len() as f64);
        let half = STEP / 2.0;
        let x0 = s.ix as f64 * STEP - half;
        let y0 = s.iy as f64 * STEP - half;
        let (x1, y1) = (x0 + w * STEP, y0 + h * STEP);
        let zat = |x: f64, y: f64| s.z - (s.normal.x * (x - s.ix as f64 * STEP) + s.normal.y * (y - s.iy as f64 * STEP)) / s.normal.z.max(0.1);
        areas.push(Area {
            id: id + 1,
            nw: DVec3::new(x0, y0, zat(x0, y0)),
            se: DVec3::new(x1, y1, zat(x1, y1)),
            ne_z: zat(x1, y0),
            sw_z: zat(x0, y1),
            crouch: s.crouch,
            connect: Default::default(),
        });
    }
    // Verbindungen: über die Randzellen (Richtung wie im Raster, Stufen nur abwärts = einseitig)
    for (i, node) in g.nodes.iter().enumerate() {
        let a = owner[i];
        for d in 0..4 {
            if let Some(j) = node.links[d] {
                let b = owner[j as usize];
                if b != a && !areas[a as usize].connect[d].contains(&(b + 1)) {
                    areas[a as usize].connect[d].push(b + 1);
                }
            }
        }
    }
    areas
}

// ------------------------------------------------------------------ Ergebnis

#[derive(Debug, Default, Clone)]
pub struct NavResult {
    pub areas: Vec<Area>,
    pub nodes: usize,
    pub seeds: usize,
    pub millis: u128,
}

/// Startpunkte: Spawnpunkte der Map
pub fn seeds(map: &Map) -> Vec<DVec3> {
    map.entities
        .iter()
        .filter(|e| {
            let c = e.classname().to_ascii_lowercase();
            c.starts_with("info_player_") || c == "gmod_player_start" || c == "info_node"
        })
        .filter_map(|e| e.origin())
        .collect()
}

/// Navmesh für die Map erzeugen
pub fn generate(map: &Map, limit: f64) -> Result<NavResult, String> {
    let t = std::time::Instant::now();
    let solids = player_solids(map);
    let w = World::new(&solids);
    let s = seeds(map);
    if s.is_empty() {
        return Err("Keine Spawnpunkte (info_player_start, info_player_deathmatch…) – von dort aus wird das Navmesh erzeugt".into());
    }
    let g = walk(limit, &w, &s)?;
    let areas = build_areas(&g);
    Ok(NavResult { nodes: g.nodes.len(), seeds: s.len(), areas, millis: t.elapsed().as_millis() })
}

/// `.nav`-Datei (Version 16, wie Garry's Mod). `bsp_size` = Größe der kompilierten BSP (0 = unbekannt;
/// GMod meldet dann nur „veraltet“ in der Konsole und lädt das Navmesh trotzdem).
pub fn write_nav(areas: &[Area], bsp_size: u32) -> Vec<u8> {
    let mut b = Vec::new();
    let u32_ = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
    let f32_ = |b: &mut Vec<u8>, v: f64| b.extend_from_slice(&(v as f32).to_le_bytes());
    u32_(&mut b, 0xFEED_FACE);
    u32_(&mut b, 16);
    u32_(&mut b, 0); // Untersion
    u32_(&mut b, bsp_size);
    b.push(0); // nicht analysiert
    b.extend_from_slice(&0u16.to_le_bytes()); // keine Orte (Places)
    b.push(1); // Bereiche ohne Ort vorhanden
    u32_(&mut b, areas.len() as u32);
    for a in areas {
        u32_(&mut b, a.id);
        u32_(&mut b, if a.crouch { NAV_CROUCH } else { 0 });
        for v in [a.nw, a.se] {
            f32_(&mut b, v.x);
            f32_(&mut b, v.y);
            f32_(&mut b, v.z);
        }
        f32_(&mut b, a.ne_z);
        f32_(&mut b, a.sw_z);
        for d in 0..4 {
            u32_(&mut b, a.connect[d].len() as u32);
            for id in &a.connect[d] {
                u32_(&mut b, *id);
            }
        }
        b.push(0); // Verstecke
        u32_(&mut b, 0); // Begegnungswege
        b.extend_from_slice(&0u16.to_le_bytes()); // Ort
        for _ in 0..2 {
            u32_(&mut b, 0); // Leitern hoch/runter
        }
        for _ in 0..2 {
            f32_(&mut b, 0.0); // früheste Besetzung je Team
        }
        for _ in 0..4 {
            f32_(&mut b, 1.0); // Licht an den Ecken
        }
        u32_(&mut b, 0); // sichtbare Bereiche
        u32_(&mut b, 0); // Sichtbarkeit erben von
    }
    u32_(&mut b, 0); // Leitern
    b
}

/// `.nav` lesen (nur Version 16 aus `write_nav`; für Test und Anzeige)
pub fn read_nav(b: &[u8]) -> Result<Vec<Area>, String> {
    let mut p = 0usize;
    let u32_ = |p: &mut usize| -> Result<u32, String> {
        let v = b.get(*p..*p + 4).ok_or("Datei zu kurz")?;
        *p += 4;
        Ok(u32::from_le_bytes(v.try_into().unwrap()))
    };
    if u32_(&mut p)? != 0xFEED_FACE {
        return Err("keine .nav-Datei".into());
    }
    let ver = u32_(&mut p)?;
    if ver != 16 {
        return Err(format!(".nav-Version {ver} wird nicht unterstützt (16)"));
    }
    let _sub = u32_(&mut p)?;
    let _bsp = u32_(&mut p)?;
    p += 1;
    let places = u16::from_le_bytes(b.get(p..p + 2).ok_or("Datei zu kurz")?.try_into().unwrap());
    p += 2;
    for _ in 0..places {
        let l = u16::from_le_bytes(b.get(p..p + 2).ok_or("Datei zu kurz")?.try_into().unwrap()) as usize;
        p += 2 + l;
    }
    p += 1;
    let n = u32_(&mut p)?;
    let mut out = Vec::new();
    let f = |p: &mut usize| -> Result<f64, String> {
        let v = b.get(*p..*p + 4).ok_or("Datei zu kurz")?;
        *p += 4;
        Ok(f32::from_le_bytes(v.try_into().unwrap()) as f64)
    };
    for _ in 0..n.min(1_000_000) {
        let id = u32_(&mut p)?;
        let flags = u32_(&mut p)?;
        let nw = DVec3::new(f(&mut p)?, f(&mut p)?, f(&mut p)?);
        let se = DVec3::new(f(&mut p)?, f(&mut p)?, f(&mut p)?);
        let (ne_z, sw_z) = (f(&mut p)?, f(&mut p)?);
        let mut connect: [Vec<u32>; 4] = Default::default();
        for c in connect.iter_mut() {
            let k = u32_(&mut p)?;
            for _ in 0..k {
                c.push(u32_(&mut p)?);
            }
        }
        let hs = *b.get(p).ok_or("Datei zu kurz")? as usize;
        p += 1 + hs * 17;
        let enc = u32_(&mut p)?;
        for _ in 0..enc {
            p += 4 + 1 + 4 + 1;
            let spots = *b.get(p).ok_or("Datei zu kurz")? as usize;
            p += 1 + spots * 5;
        }
        p += 2;
        for _ in 0..2 {
            let k = u32_(&mut p)?;
            p += k as usize * 4;
        }
        p += 8 + 16;
        let vis = u32_(&mut p)?;
        p += vis as usize * 5 + 4;
        out.push(Area { id, nw, se, ne_z, sw_z, crouch: flags & NAV_CROUCH != 0, connect });
    }
    Ok(out)
}

/// Wohin die .nav gehört: `<spiel>/maps/<map>.nav`, ohne Spielordner neben die VMF
pub fn nav_path(game_dir: &str, vmf: Option<&std::path::Path>, map_name: &str) -> std::path::PathBuf {
    let maps = std::path::Path::new(game_dir).join("maps");
    if !game_dir.is_empty() && maps.is_dir() {
        return maps.join(format!("{map_name}.nav"));
    }
    match vmf {
        Some(v) => v.with_extension("nav"),
        None => std::path::PathBuf::from(format!("{map_name}.nav")),
    }
}

/// Erzeugen und speichern; liefert (Ergebnis, Pfad)
pub fn generate_and_save(map: &Map, limit: f64, path: &std::path::Path, bsp_size: u32) -> Result<NavResult, String> {
    let r = generate(map, limit)?;
    std::fs::write(path, write_nav(&r.areas, bsp_size)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(r)
}

impl NavResult {
    pub fn summary(&self, path: &std::path::Path) -> String {
        let crouch = self.areas.iter().filter(|a| a.crouch).count();
        format!("Navmesh: {} Bereiche ({} geduckt) aus {} Rasterpunkten ab {} Spawnpunkt(en) in {} ms → {}", self.areas.len(), crouch, self.nodes, self.seeds, self.millis, path.display())
    }
}

type NavJob = std::sync::Arc<std::sync::Mutex<Option<Result<(NavResult, std::path::PathBuf), String>>>>;

#[derive(Default)]
pub struct NavState {
    job: Option<NavJob>,
}

impl crate::app::App {
    /// Karte → Navmesh erzeugen (im Hintergrund)
    pub(crate) fn start_navmesh(&mut self) {
        if self.nav.job.is_some() {
            return;
        }
        let map = self.ed.map.clone();
        let limit = self.ed.map_limit;
        let path = nav_path(&self.settings.game_dir, self.ed.path.as_deref(), &self.ed.map_name());
        let res: NavJob = Default::default();
        let r2 = res.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let r = generate_and_save(&map, limit, &path, 0).map(|r| (r, path));
            *r2.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
        self.ed.status = "Navmesh wird erzeugt…".into();
        self.nav.job = Some(res);
    }

    /// Fertigen Hintergrund-Job übernehmen (jedes Bild aufrufen)
    pub(crate) fn poll_navmesh(&mut self) {
        let Some(job) = &self.nav.job else { return };
        let Some(r) = job.lock().unwrap().take() else {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        self.nav.job = None;
        match r {
            Ok((r, path)) => {
                self.ed.status = r.summary(&path);
                self.ed.nav_preview = Some(std::sync::Arc::new(r.areas));
                self.ed.nav_show = true;
            }
            Err(e) => self.ed.status = format!("Navmesh: {e}"),
        }
        self.ed.touch();
    }

    /// Vorhandene .nav anzeigen
    pub(crate) fn load_navmesh(&mut self) {
        let dir = std::path::Path::new(&self.settings.game_dir).join("maps");
        let mut d = rfd::FileDialog::new().add_filter("Navmesh", &["nav"]);
        if dir.is_dir() {
            d = d.set_directory(dir);
        }
        let Some(p) = d.pick_file() else { return };
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| read_nav(&b)) {
            Ok(a) => {
                self.ed.status = format!("Navmesh geladen: {} Bereiche ({})", a.len(), p.display());
                self.ed.nav_preview = Some(std::sync::Arc::new(a));
                self.ed.nav_show = true;
            }
            Err(e) => self.ed.status = format!("Navmesh konnte nicht gelesen werden: {e}"),
        }
        self.ed.touch();
    }
}

/// Linien der Navmesh-Vorschau in der Nähe eines Punkts (grün, geduckt blau, Verbindungen gelb)
pub fn preview_lines(areas: &[Area], near: DVec3, radius: f64) -> Vec<(DVec3, DVec3, [f32; 4])> {
    let mut out = Vec::new();
    let lift = DVec3::Z * 1.0;
    for a in areas {
        let c = (a.nw + a.se) * 0.5;
        if (c.x - near.x).abs() > radius || (c.y - near.y).abs() > radius {
            continue;
        }
        let col = if a.crouch { [0.3, 0.55, 1.0, 0.9] } else { [0.25, 0.95, 0.35, 0.9] };
        let k = a.corners();
        for i in 0..4 {
            out.push((k[i] + lift, k[(i + 1) % 4] + lift, col));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::map::Entity;

    /// Raum 512×512 mit einer 64 hohen Stufe in der Mitte (nicht erklimmbar) und einer 16er-Stufe
    fn room() -> Editor {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let m = "DEV/DEV_MEASUREGENERIC01";
        for b in [
            Aabb::new(DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0)),
            Aabb::new(DVec3::new(-256.0, -256.0, 256.0), DVec3::new(256.0, 256.0, 272.0)),
            Aabb::new(DVec3::new(-272.0, -256.0, 0.0), DVec3::new(-256.0, 256.0, 256.0)),
            Aabb::new(DVec3::new(256.0, -256.0, 0.0), DVec3::new(272.0, 256.0, 256.0)),
            Aabb::new(DVec3::new(-256.0, -272.0, 0.0), DVec3::new(256.0, -256.0, 256.0)),
            Aabb::new(DVec3::new(-256.0, 256.0, 0.0), DVec3::new(256.0, 272.0, 256.0)),
            // hohe Kiste: oben drauf kommt man nicht, aber man fällt herunter
            Aabb::new(DVec3::new(100.0, 100.0, 0.0), DVec3::new(200.0, 200.0, 64.0)),
            // Podest 16 hoch: begehbar
            Aabb::new(DVec3::new(-200.0, -200.0, 0.0), DVec3::new(-100.0, -100.0, 16.0)),
        ] {
            let s = ed.map.make_box(b, m, 0.25);
            ed.map.world.solids.push(s.into());
        }
        let mut e = Entity::new(ed.map.alloc_id(), "info_player_start");
        e.set("origin", "0 0 8");
        ed.map.entities.push(e.into());
        ed
    }

    #[test]
    fn generates_connected_areas() {
        let ed = room();
        let r = generate(&ed.map, ed.map_limit).unwrap();
        assert!(r.nodes > 200, "{} Rasterpunkte", r.nodes);
        assert!(!r.areas.is_empty() && r.areas.len() < r.nodes, "{} Bereiche aus {} Punkten", r.areas.len(), r.nodes);
        // nichts auf der hohen Kiste, aber auf dem Podest
        let on = |x: f64, y: f64, z: f64| r.areas.iter().any(|a| a.nw.x <= x && x <= a.se.x && a.nw.y <= y && y <= a.se.y && (a.nw.z - z).abs() < 1.0);
        assert!(!on(150.0, 150.0, 64.0), "hohe Kiste ist nicht erreichbar");
        assert!(on(-150.0, -150.0, 16.0), "Podest (Stufe 16) ist erreichbar");
        assert!(on(0.0, 0.0, 0.0));
        // keine Bereiche in den Wänden
        assert!(r.areas.iter().all(|a| a.nw.x >= -260.0 && a.se.x <= 260.0));
        // jede Verbindung zeigt auf einen existierenden Bereich, alle Bereiche hängen zusammen
        let ids: std::collections::HashSet<u32> = r.areas.iter().map(|a| a.id).collect();
        assert!(r.areas.iter().all(|a| a.connect.iter().flatten().all(|c| ids.contains(c))));
        let mut seen = std::collections::HashSet::from([r.areas[0].id]);
        let mut st = vec![r.areas[0].id];
        while let Some(i) = st.pop() {
            let a = &r.areas[(i - 1) as usize];
            for c in a.connect.iter().flatten() {
                if seen.insert(*c) {
                    st.push(*c);
                }
            }
        }
        assert_eq!(seen.len(), r.areas.len(), "alle Bereiche erreichbar");
        // Datei schreiben und wieder lesen
        let bytes = write_nav(&r.areas, 1234);
        let back = read_nav(&bytes).unwrap();
        assert_eq!(back.len(), r.areas.len());
        assert_eq!(back[0].connect, r.areas[0].connect);
        assert!((back[3].se - r.areas[3].se).length() < 1e-3);
    }

    /// AMBOSS_NAV_MAP=pfad.vmf cargo test --release nav_debug -- --ignored --nocapture
    #[test]
    #[ignore]
    fn nav_debug() {
        let Ok(p) = std::env::var("AMBOSS_NAV_MAP") else { return };
        let m = Map::from_vmf(&std::fs::read_to_string(p).unwrap()).unwrap();
        let r = generate(&m, 16384.0).unwrap();
        let st: Vec<usize> = STOP.iter().map(|a| a.load(std::sync::atomic::Ordering::Relaxed)).collect();
        println!("{} Punkte, {} Bereiche, {} ms; Stopp: kein Boden/zu steil {}, Wand {}, kein Platz {}", r.nodes, r.areas.len(), r.millis, st[0], st[1], st[2]);
        let solids = player_solids(&m);
        let w = World::new(&solids);
        // Randpunkte: wo ist der nächste Boden, wie steil?
        let s = seeds(&m)[0];
        println!("Spawn {s:?}, Boden darunter: {:?}", w.ray(s + DVec3::Z * 24.0, -DVec3::Z, 4096.0));
    }

    #[test]
    fn no_spawn_is_an_error() {
        let mut ed = room();
        ed.map.entities.clear();
        assert!(generate(&ed.map, ed.map_limit).is_err());
    }
}
