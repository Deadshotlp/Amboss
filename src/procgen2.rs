//! Weitere Generatoren: Höhlen (Zellautomat, Boden und Decke als Displacements, abgedichtet) und
//! Innenausbau (Möbel in einen ausgewählten Raum streuen – an Wände, in die Mitte, in Ecken).

use crate::disp::Disp;
use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use crate::map::{fmt_vec, Entity};
use glam::DVec3;

#[derive(Clone, Debug)]
pub struct CaveOpts {
    pub seed: u64,
    /// Zellen in X und Y
    pub w: usize,
    pub h: usize,
    pub cell: f64,
    /// Anteil Fels beim Start (0,4–0,55)
    pub fill: f64,
    pub height: f64,
    /// Höhe der Unebenheiten von Boden und Decke
    pub bumps: f64,
    pub wall_material: String,
    pub floor_material: String,
    pub lights: bool,
}

impl Default for CaveOpts {
    fn default() -> Self {
        CaveOpts { seed: 7, w: 40, h: 40, cell: 64.0, fill: 0.45, height: 192.0, bumps: 32.0, wall_material: "NATURE/ROCKWALL021A".into(), floor_material: "NATURE/DIRTFLOOR006A".into(), lights: true }
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
    fn f(&mut self) -> f64 {
        (self.u() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Glatte Wertrauschen-Funktion 0..1 (gleiche Welt-Koordinate → gleicher Wert, daher nahtlos über Kacheln)
pub fn noise(seed: u64, x: f64, y: f64) -> f64 {
    let h = |ix: i64, iy: i64| -> f64 {
        let mut v = (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
        v ^= v >> 33;
        v = v.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        v ^= v >> 33;
        (v >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut sum = 0.0;
    let mut amp = 0.6;
    let mut freq = 1.0 / 160.0;
    let mut norm = 0.0;
    for _ in 0..3 {
        let (fx, fy) = (x * freq, y * freq);
        let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
        let (tx, ty) = (fx - ix as f64, fy - iy as f64);
        let s = |t: f64| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (s(tx), s(ty));
        let a = h(ix, iy) + (h(ix + 1, iy) - h(ix, iy)) * sx;
        let b = h(ix, iy + 1) + (h(ix + 1, iy + 1) - h(ix, iy + 1)) * sx;
        sum += (a + (b - a) * sy) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Höhlen-Raster: true = Fels. Rand immer Fels, nur die größte zusammenhängende Höhle bleibt.
pub fn cave_grid(o: &CaveOpts) -> Vec<Vec<bool>> {
    let (w, h) = (o.w.clamp(8, 160), o.h.clamp(8, 160));
    let mut rng = Rng::new(o.seed);
    let mut g: Vec<Vec<bool>> = (0..h).map(|y| (0..w).map(|x| x == 0 || y == 0 || x == w - 1 || y == h - 1 || rng.f() < o.fill.clamp(0.3, 0.6)).collect()).collect();
    for _ in 0..5 {
        let prev = g.clone();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let n = (-1i32..=1).flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy))).filter(|&(dx, dy)| (dx, dy) != (0, 0) && prev[(y as i32 + dy) as usize][(x as i32 + dx) as usize]).count();
                g[y][x] = n > 4 || (n == 4 && prev[y][x]);
            }
        }
    }
    // größte offene Fläche behalten
    let mut label = vec![vec![usize::MAX; w]; h];
    let mut best = (0usize, 0usize);
    let mut id = 0;
    for sy in 0..h {
        for sx in 0..w {
            if g[sy][sx] || label[sy][sx] != usize::MAX {
                continue;
            }
            let mut stack = vec![(sx, sy)];
            label[sy][sx] = id;
            let mut n = 0;
            while let Some((x, y)) = stack.pop() {
                n += 1;
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = ((x as i32 + dx) as usize, (y as i32 + dy) as usize);
                    if nx < w && ny < h && !g[ny][nx] && label[ny][nx] == usize::MAX {
                        label[ny][nx] = id;
                        stack.push((nx, ny));
                    }
                }
            }
            if n > best.1 {
                best = (id, n);
            }
            id += 1;
        }
    }
    for y in 0..h {
        for x in 0..w {
            if !g[y][x] && label[y][x] != best.0 {
                g[y][x] = true;
            }
        }
    }
    g
}

/// Fläche des zuletzt angelegten Brushes mit Normale ≈ `n` zum Displacement machen; `height(p)` = Versatz entlang der Normale
fn displace(ed: &mut Editor, sid: u32, n: DVec3, height: &dyn Fn(DVec3) -> f64) {
    let Some(s) = ed.map.solid_mut(sid) else { return };
    let Some(i) = s.planes.iter().position(|p| p.is_some_and(|p| p.normal.dot(n) > 0.99)) else { return };
    let (Some(poly), Some(Some(pl))) = (s.polys.get(i).cloned(), s.planes.get(i).copied()) else { return };
    let Some(mut d) = Disp::new_flat(3, &poly, pl.normal) else { return };
    let Some(g) = d.geometry(&poly, pl.normal) else { return };
    for (k, p) in g.base.iter().enumerate() {
        d.distances[k] = height(*p);
    }
    s.sides[i].disp = Some(d);
    s.update_geometry();
}

/// Höhle bauen (Ursprung = linke untere Ecke); liefert die Anzahl der Objekte
pub fn cave(ed: &mut Editor, o: &CaveOpts, origin: DVec3) -> usize {
    let g = cave_grid(o);
    let (h, w) = (g.len(), g[0].len());
    let c = o.cell;
    let amp = o.bumps.max(0.0);
    ed.checkpoint();
    let hold = ed.checkpoint_hold;
    ed.checkpoint_hold = 2;
    let mut made: Vec<Obj> = Vec::new();
    let mut block = |ed: &mut Editor, min: DVec3, max: DVec3, mat: &str| -> u32 {
        let s = ed.map.make_box(Aabb::new(min, max), mat, ed.tex_scale);
        let id = s.id;
        ed.map.world.solids.push(s.into());
        made.push(Obj::Solid(id));
        id
    };
    let (sx, sy) = (w as f64 * c, h as f64 * c);
    let bottom = -amp - 48.0;
    let top = o.height + 48.0;
    // Dichtung unten und oben (Displacements dichten nicht ab)
    block(ed, origin + DVec3::new(0.0, 0.0, bottom), origin + DVec3::new(sx, sy, bottom + 16.0), "TOOLS/TOOLSNODRAW");
    block(ed, origin + DVec3::new(0.0, 0.0, top - 16.0), origin + DVec3::new(sx, sy, top), "TOOLS/TOOLSNODRAW");
    // Felswände: Zellen zu Rechtecken zusammengefasst, vom Boden bis zur Decke der Dichtung
    for (x0, y0, x1, y1) in crate::procgen::rects(&g) {
        block(ed, origin + DVec3::new(x0 as f64 * c, y0 as f64 * c, bottom + 16.0), origin + DVec3::new((x1 + 1) as f64 * c, (y1 + 1) as f64 * c, top - 16.0), &o.wall_material);
    }
    // Boden- und Deckenkacheln (4 × 4 Zellen) mit Unebenheiten, nur wo Höhle ist
    let tile = 4;
    let seed = o.seed;
    for ty in (0..h).step_by(tile) {
        for tx in (0..w).step_by(tile) {
            let (tx1, ty1) = ((tx + tile).min(w), (ty + tile).min(h));
            if !(ty..ty1).any(|y| (tx..tx1).any(|x| !g[y][x])) {
                continue;
            }
            let (x0, y0, x1, y1) = (tx as f64 * c, ty as f64 * c, tx1 as f64 * c, ty1 as f64 * c);
            let f = block(ed, origin + DVec3::new(x0, y0, -16.0), origin + DVec3::new(x1, y1, 0.0), &o.floor_material);
            displace(ed, f, DVec3::Z, &|p| -noise(seed, p.x, p.y) * amp);
            let k = block(ed, origin + DVec3::new(x0, y0, o.height), origin + DVec3::new(x1, y1, o.height + 16.0), &o.wall_material);
            displace(ed, k, -DVec3::Z, &|p| noise(seed ^ 0xABCD, p.x, p.y) * amp);
        }
    }
    // Licht, Spawn, Cubemap in offenen Zellen
    let open: Vec<(usize, usize)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| !g[y][x]).collect();
    let center = open.iter().fold((0.0, 0.0), |a, &(x, y)| (a.0 + x as f64, a.1 + y as f64));
    let (cx, cy) = (center.0 / open.len().max(1) as f64, center.1 / open.len().max(1) as f64);
    // Zelle mit freien Nachbarn in der Nähe der Mitte (Spieler passt hin)
    let free = |x: usize, y: usize| (y.saturating_sub(1)..=(y + 1).min(h - 1)).all(|yy| (x.saturating_sub(1)..=(x + 1).min(w - 1)).all(|xx| !g[yy][xx]));
    let spawn = open.iter().filter(|&&(x, y)| free(x, y)).min_by(|a, b| ((a.0 as f64 - cx).powi(2) + (a.1 as f64 - cy).powi(2)).total_cmp(&((b.0 as f64 - cx).powi(2) + (b.1 as f64 - cy).powi(2)))).or(open.first()).copied();
    let mut ent = |ed: &mut Editor, class: &str, p: DVec3, kv: &[(&str, &str)]| {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, class);
        e.set("origin", fmt_vec(p));
        for (k, v) in kv {
            e.set(k, *v);
        }
        ed.map.entities.push(e.into());
        made.push(Obj::Entity(id));
    };
    let at = |x: usize, y: usize, z: f64| origin + DVec3::new((x as f64 + 0.5) * c, (y as f64 + 0.5) * c, z);
    if let Some((x, y)) = spawn {
        ent(ed, "info_player_start", at(x, y, 8.0), &[("angles", "0 0 0")]);
        ent(ed, "env_cubemap", at(x, y, 64.0), &[]);
    }
    if o.lights {
        let step = (384.0 / c).max(2.0) as usize;
        for &(x, y) in open.iter().filter(|&&(x, y)| x % step == step / 2 && y % step == step / 2 && free(x, y)) {
            ent(ed, "light", at(x, y, (o.height - amp - 24.0).max(48.0)), &[("_light", "255 210 160 120"), ("_quadratic_attn", "1")]);
        }
    }
    ed.checkpoint_hold = hold;
    let n = made.len();
    ed.sel = made.into_iter().collect();
    ed.status = format!("Höhle: {}", crate::i18n::count_de(n, "Objekt", "Objekte"));
    ed.touch();
    n
}

// ---------------------------------------------------------------- Innenausbau

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    Wall,
    Center,
    Corner,
}

/// Ein Möbelstück: Modell, Größe (Breite entlang der Wand × Tiefe × Höhe), Platz, Häufigkeit
pub struct Item {
    pub model: &'static str,
    pub size: [f64; 3],
    pub place: Where,
    pub weight: u32,
}

pub const STYLES: [&str; 3] = ["Wohnung", "Büro", "Lager"];

pub fn items(style: usize) -> &'static [Item] {
    match style {
        0 => &[
            Item { model: "models/props_c17/furniturecouch001a.mdl", size: [80.0, 36.0, 36.0], place: Where::Wall, weight: 2 },
            Item { model: "models/props_c17/furnitureshelf001a.mdl", size: [64.0, 20.0, 80.0], place: Where::Wall, weight: 2 },
            Item { model: "models/props_c17/furnituredrawer001a.mdl", size: [40.0, 22.0, 44.0], place: Where::Wall, weight: 2 },
            Item { model: "models/props_c17/furniturecupboard001a.mdl", size: [36.0, 22.0, 50.0], place: Where::Wall, weight: 1 },
            Item { model: "models/props_c17/furnituretable001a.mdl", size: [48.0, 48.0, 30.0], place: Where::Center, weight: 2 },
            Item { model: "models/props_c17/furniturechair001a.mdl", size: [24.0, 24.0, 40.0], place: Where::Center, weight: 0 },
            Item { model: "models/props_c17/lamp001a.mdl", size: [20.0, 20.0, 64.0], place: Where::Corner, weight: 1 },
        ],
        1 => &[
            Item { model: "models/props_wasteland/controlroom_filecabinet002a.mdl", size: [24.0, 24.0, 56.0], place: Where::Wall, weight: 3 },
            Item { model: "models/props_c17/furnitureshelf001b.mdl", size: [64.0, 20.0, 80.0], place: Where::Wall, weight: 1 },
            Item { model: "models/props_interiors/furniture_desk01a.mdl", size: [64.0, 32.0, 30.0], place: Where::Center, weight: 2 },
            Item { model: "models/props_interiors/furniture_chair01a.mdl", size: [24.0, 24.0, 40.0], place: Where::Center, weight: 0 },
            Item { model: "models/props_lab/monitor01a.mdl", size: [16.0, 16.0, 20.0], place: Where::Center, weight: 0 },
            Item { model: "models/props_c17/lamp001a.mdl", size: [20.0, 20.0, 64.0], place: Where::Corner, weight: 1 },
        ],
        _ => &[
            Item { model: "models/props_wasteland/kitchen_shelf001a.mdl", size: [64.0, 24.0, 80.0], place: Where::Wall, weight: 3 },
            Item { model: "models/props_junk/wood_crate002a.mdl", size: [80.0, 40.0, 40.0], place: Where::Wall, weight: 2 },
            Item { model: "models/props_junk/wood_crate001a.mdl", size: [40.0, 40.0, 40.0], place: Where::Center, weight: 3 },
            Item { model: "models/props_c17/oildrum001.mdl", size: [32.0, 32.0, 48.0], place: Where::Corner, weight: 2 },
            Item { model: "models/props_junk/wood_pallet001a.mdl", size: [48.0, 48.0, 8.0], place: Where::Center, weight: 1 },
        ],
    }
}

/// Belegt die Box (Grundfläche min..max, Höhe z0..z1) einen festen Brush?
fn blocked(ed: &Editor, min: DVec3, max: DVec3) -> bool {
    ed.map.solids_owned().any(|(owner, s)| crate::gameplay::blocks(owner, s) && crate::gameplay::box_hits_solid(min, max, s))
}

/// Abstand zur nächsten Wand in Richtung `d` (Strahl in Hüfthöhe), höchstens `max`
fn wall_dist(ed: &Editor, p: DVec3, d: DVec3, max: f64) -> f64 {
    ed.pick_3d(p, d).map(|(_, _, t)| t).filter(|t| *t < max).unwrap_or(max)
}

/// Möbel in den Bereich `area` (Auswahl eines Raums) streuen. `exists(model)` filtert fehlende Modelle,
/// `bounds(model)` liefert die echte Größe, wenn bekannt. Liefert die Anzahl.
pub fn furnish(ed: &mut Editor, area: Aabb, style: usize, density: f64, seed: u64, exists: &dyn Fn(&str) -> bool, bounds: &dyn Fn(&str) -> Option<Aabb>) -> usize {
    let list: Vec<&Item> = items(style).iter().filter(|i| exists(i.model)).collect();
    if list.is_empty() || area.is_empty() {
        return 0;
    }
    let mut rng = Rng::new(seed);
    // echte Größe aus dem Modell, sonst die Tabelle (Breite, Tiefe, Höhe)
    let size_of = |it: &Item| -> [f64; 3] {
        match bounds(it.model) {
            Some(b) if !b.is_empty() => [b.size().x.max(b.size().y), b.size().x.min(b.size().y), b.size().z],
            _ => it.size,
        }
    };
    let mut taken: Vec<Aabb> = Vec::new();
    let mut placed: Vec<(String, DVec3, f64)> = Vec::new();
    let free = |ed: &Editor, taken: &[Aabb], min: DVec3, max: DVec3| -> bool { !blocked(ed, min, max) && !taken.iter().any(|t| t.min.x < max.x && t.max.x > min.x && t.min.y < max.y && t.max.y > min.y && t.min.z < max.z && t.max.z > min.z) };
    // Bodenhöhe: Strahl von oben in den Raum
    let floor_at = |ed: &Editor, x: f64, y: f64| -> Option<f64> {
        let o = DVec3::new(x, y, area.max.z - 2.0);
        ed.pick_3d(o, -DVec3::Z).map(|(_, _, t)| o.z - t).filter(|z| *z > area.min.z - 1.0)
    };
    let pick_item = |rng: &mut Rng, place: Where| -> Option<&Item> {
        let c: Vec<&&Item> = list.iter().filter(|i| i.place == place && i.weight > 0).collect();
        let total: u32 = c.iter().map(|i| i.weight).sum();
        if total == 0 {
            return None;
        }
        let mut r = (rng.u() % total as u64) as u32;
        for i in c {
            if r < i.weight {
                return Some(i);
            }
            r -= i.weight;
        }
        None
    };
    let target = ((area.size().x * area.size().y) / (110.0 * 110.0) * density.clamp(0.05, 1.0)).ceil() as usize;
    let step = 24.0;
    let mut tries = 0;
    while placed.len() < target.max(1) && tries < 4000 {
        tries += 1;
        let x = area.min.x + 24.0 + rng.f() * (area.size().x - 48.0).max(1.0);
        let y = area.min.y + 24.0 + rng.f() * (area.size().y - 48.0).max(1.0);
        let Some(z) = floor_at(ed, x, y) else { continue };
        let probe = DVec3::new(x, y, z + 24.0);
        let dirs = [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y];
        let dist: Vec<f64> = dirs.iter().map(|d| wall_dist(ed, probe, *d, 400.0)).collect();
        let near: Vec<usize> = (0..4).filter(|i| dist[*i] < 48.0).collect();
        let place = match near.len() {
            0 if dist.iter().all(|d| *d > 72.0) => Where::Center,
            1 => Where::Wall,
            2 if near[0] / 2 != near[1] / 2 => Where::Corner,
            _ => continue,
        };
        let Some(it) = pick_item(&mut rng, place) else { continue };
        let [w, d, hgt] = size_of(it);
        // an die Wand schieben, Rückseite zur Wand; Blickrichtung (Yaw) von der Wand weg
        let (mut c, yaw, along_x) = match place {
            Where::Wall | Where::Corner => {
                let i = near[0];
                let n = dirs[i];
                let c = probe + n * (dist[i] - d / 2.0 - 1.0);
                let yaw = [180.0, 0.0, 270.0, 90.0][i];
                (c, yaw, i >= 2)
            }
            Where::Center => (probe, (rng.u() % 4) as f64 * 90.0, rng.u() % 2 == 0),
        };
        if place == Where::Corner && near.len() == 2 {
            let j = near[1];
            c += dirs[j] * (dist[j] - w.min(d) / 2.0 - 1.0);
        }
        c.x = (c.x / 4.0).round() * 4.0;
        c.y = (c.y / 4.0).round() * 4.0;
        let (hx, hy) = if along_x { (w / 2.0, d / 2.0) } else { (d / 2.0, w / 2.0) };
        let min = DVec3::new(c.x - hx, c.y - hy, z + 1.0);
        let max = DVec3::new(c.x + hx, c.y + hy, z + hgt);
        // Platz zum Laufen davor freihalten
        let pad = DVec3::new(8.0, 8.0, 0.0);
        if !free(ed, &taken, min - pad, max + pad) {
            continue;
        }
        taken.push(Aabb::new(min - pad, max + pad));
        placed.push((it.model.to_string(), DVec3::new(c.x, c.y, z), yaw));
        // Tisch/Schreibtisch: Stühle drumherum (und ein Bildschirm auf dem Schreibtisch)
        if place == Where::Center {
            if let Some(chair) = list.iter().find(|i| i.place == Where::Center && i.weight == 0 && i.model.contains("chair")) {
                let cs = size_of(chair);
                for (k, (dx, dy)) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)].into_iter().enumerate() {
                    if style == 1 && k > 0 {
                        break; // Büro: ein Stuhl am Schreibtisch
                    }
                    let off = DVec3::new(dx * (hx + cs[1] / 2.0 + 4.0), dy * (hy + cs[1] / 2.0 + 4.0), 0.0);
                    let p = DVec3::new(c.x, c.y, z) + off;
                    let (cmin, cmax) = (p - DVec3::new(cs[1] / 2.0, cs[1] / 2.0, -1.0), p + DVec3::new(cs[1] / 2.0, cs[1] / 2.0, cs[2]));
                    if free(ed, &taken, cmin, cmax) && floor_at(ed, p.x, p.y).is_some_and(|fz| (fz - z).abs() < 4.0) {
                        taken.push(Aabb::new(cmin, cmax));
                        // Stuhl schaut zum Tisch
                        let face = (-off.y).atan2(-off.x).to_degrees().rem_euclid(360.0);
                        placed.push((chair.model.to_string(), p, face));
                    }
                }
            }
            if let Some(mon) = list.iter().find(|i| i.model.contains("monitor")).filter(|_| it.model.contains("desk")) {
                placed.push((mon.model.to_string(), DVec3::new(c.x, c.y, z + hgt), yaw));
            }
        }
        let _ = step;
    }
    if placed.is_empty() {
        return 0;
    }
    ed.checkpoint();
    let mut ids = Vec::new();
    for (m, p, yaw) in &placed {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "prop_static");
        e.set("origin", fmt_vec(*p));
        e.set("angles", format!("0 {} 0", crate::map::fmt_num(*yaw)));
        e.set("model", m.clone());
        e.set("solid", "6");
        ed.map.entities.push(e.into());
        ids.push(Obj::Entity(id));
    }
    let n = ids.len();
    ed.sel = ids.into_iter().collect();
    ed.status = format!("Innenausbau: {}", crate::i18n::count_de(n, "Möbelstück", "Möbelstücke"));
    ed.touch();
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cave_is_connected_and_sealed() {
        let o = CaveOpts { w: 30, h: 24, ..Default::default() };
        let g = cave_grid(&o);
        assert!(g[0].iter().all(|c| *c) && g.iter().all(|r| r[0] && r[r.len() - 1]), "Rand ist Fels");
        let open = g.iter().flatten().filter(|c| !**c).count();
        assert!(open > 100, "genug Höhle: {open}");
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let n = cave(&mut ed, &o, DVec3::ZERO);
        assert!(n > 10);
        assert!(ed.map.all_solids().any(|(_, s)| s.has_disp()), "Boden/Decke als Displacement");
        assert!(ed.map.entities.iter().any(|e| e.classname() == "info_player_start"));
        assert!(ed.map.entities.iter().any(|e| e.classname() == "light"));
        // dicht: Leckprüfung auf einer Kopie
        let r = crate::optimize::run(&mut crate::optimize::build_copy(&ed, ed.map.clone()), &crate::optimize::OptSettings { ai_nodes: false, cubemaps: false, ..Default::default() }, None, None);
        assert!(r.leak.is_none(), "Höhle hat ein Leck");
        // nahtlos: Rauschen hängt nur von der Weltposition ab
        assert_eq!(noise(3, 100.0, 200.0), noise(3, 100.0, 200.0));
        assert!((0.0..=1.0).contains(&noise(3, -555.0, 12.5)));
    }

    #[test]
    fn furnish_room() {
        let room = || {
            let mut ed = Editor::new(crate::fgd::Fgd::builtin());
            // Raum 512 × 384 × 128, Wände 16
            ed.create_block(Aabb::new(DVec3::new(-256.0, -192.0, -16.0), DVec3::new(256.0, 192.0, 144.0)));
            ed.sel = ed.map.world.solids.iter().map(|s| Obj::Solid(s.id)).collect();
            ed.hollow(16.0);
            ed
        };
        for style in 0..3 {
            let mut e2 = room();
            let area = e2.selection_bounds();
            let n = furnish(&mut e2, area, style, 0.8, 5, &|_| true, &|_| None);
            assert!(n >= 3, "Stil {style}: nur {n}");
            // alles innerhalb der Wände, auf dem Boden, nichts überlappt die Wände
            for e in e2.map.entities.iter().filter(|e| e.classname() == "prop_static") {
                let o = e.origin().unwrap();
                assert!(o.x > -240.0 && o.x < 240.0 && o.y > -176.0 && o.y < 176.0, "{o}");
                assert!(o.z.abs() < 1.0 || e.get("model").is_some_and(|m| m.contains("monitor")), "auf dem Boden: {o}");
            }
        }
    }
}
