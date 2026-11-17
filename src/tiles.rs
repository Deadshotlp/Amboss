//! Tile-Sets: Bausteine (Boden, Wand, Ecke, Tür, Gang, Säule, Treppe …) auf einem festen Raster.
//! In der 3D-Ansicht per Klick auf eine Ebene setzen, R dreht, Shift-Klick entfernt. Bausteine sind
//! gewöhnliche Brushes (je Feld gruppiert); entfernt wird alles, was im Feld liegt – das klappt auch
//! nach dem Speichern und erneuten Öffnen.

use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use glam::DVec3;

/// Bausteine des eingebauten Sets „Innenraum“ (Wände an der +Y-Kante bei Drehung 0)
pub const TILES: [(&str, &str); 9] = [
    ("Boden", "Nur Boden (und Decke)"),
    ("Wand", "Boden mit einer Wand"),
    ("Ecke", "Zwei Wände über Eck"),
    ("Sackgasse", "Drei Wände"),
    ("Gang", "Zwei gegenüberliegende Wände"),
    ("Tür", "Wand mit Durchgang"),
    ("Fenster", "Wand mit Fensteröffnung"),
    ("Säule", "Boden mit Säule in der Mitte"),
    ("Treppe", "Stufen bis zur nächsten Ebene"),
];

#[derive(Clone, Debug)]
pub struct TileState {
    pub active: bool,
    pub kind: usize,
    /// Drehung in 90°-Schritten
    pub rot: u8,
    pub size: f64,
    pub height: f64,
    pub wall: f64,
    /// Höhe der Bauebene
    pub level: f64,
    pub ceiling: bool,
    pub wall_material: String,
    pub floor_material: String,
}

impl Default for TileState {
    fn default() -> Self {
        TileState { active: false, kind: 1, rot: 0, size: 256.0, height: 128.0, wall: 16.0, level: 0.0, ceiling: true, wall_material: "DEV/DEV_MEASUREGENERIC01".into(), floor_material: "DEV/DEV_MEASUREGENERIC01B".into() }
    }
}

/// Quader eines Bausteins in Feldkoordinaten (0..size, Boden bei z = 0) für Drehung 0
pub fn tile_boxes(t: &TileState, kind: usize) -> Vec<(DVec3, DVec3, bool)> {
    let (s, h, w) = (t.size, t.height, t.wall);
    let v = DVec3::new;
    // (min, max, ist Boden/Decke)
    let mut b: Vec<(DVec3, DVec3, bool)> = vec![(v(0.0, 0.0, -w), v(s, s, 0.0), true)];
    if t.ceiling {
        b.push((v(0.0, 0.0, h), v(s, s, h + w), true));
    }
    let north = |b: &mut Vec<(DVec3, DVec3, bool)>| b.push((v(0.0, s - w, 0.0), v(s, s, h), false));
    let east = |b: &mut Vec<(DVec3, DVec3, bool)>| b.push((v(s - w, 0.0, 0.0), v(s, s, h), false));
    let west = |b: &mut Vec<(DVec3, DVec3, bool)>| b.push((v(0.0, 0.0, 0.0), v(w, s, h), false));
    let south = |b: &mut Vec<(DVec3, DVec3, bool)>| b.push((v(0.0, 0.0, 0.0), v(s, w, h), false));
    match kind {
        1 => north(&mut b),
        2 => {
            north(&mut b);
            b.push((v(s - w, 0.0, 0.0), v(s, s - w, h), false));
        }
        3 => {
            north(&mut b);
            b.push((v(s - w, 0.0, 0.0), v(s, s - w, h), false));
            b.push((v(0.0, 0.0, 0.0), v(w, s - w, h), false));
        }
        4 => {
            north(&mut b);
            south(&mut b);
        }
        5 | 6 => {
            // Öffnung in der Mitte der Nordwand: Tür 64 breit, 112 hoch; Fenster 96 breit, 48–96 hoch
            let (ow, z0, z1) = if kind == 5 { (64.0f64.min(s - 2.0 * w), 0.0, 112.0f64.min(h - 8.0)) } else { (96.0f64.min(s - 2.0 * w), 48.0f64.min(h * 0.4), 96.0f64.min(h - 8.0)) };
            let (x0, x1) = ((s - ow) / 2.0, (s + ow) / 2.0);
            b.push((v(0.0, s - w, 0.0), v(x0, s, h), false));
            b.push((v(x1, s - w, 0.0), v(s, s, h), false));
            b.push((v(x0, s - w, z1), v(x1, s, h), false));
            if z0 > 0.0 {
                b.push((v(x0, s - w, 0.0), v(x1, s, z0), false));
            }
        }
        7 => {
            let p = (s * 0.125).max(16.0);
            b.push((v(s / 2.0 - p, s / 2.0 - p, 0.0), v(s / 2.0 + p, s / 2.0 + p, h), false));
        }
        8 => {
            // Treppe entlang +Y bis zur Deckenhöhe (Decke über der Treppe entfällt)
            if t.ceiling {
                b.retain(|x| x.0.z < h);
            }
            let steps = ((h / 16.0).round() as usize).clamp(2, 24);
            let (rise, run) = (h / steps as f64, s / steps as f64);
            for k in 0..steps {
                b.push((v(w, k as f64 * run, 0.0), v(s - w, (k + 1) as f64 * run, (k + 1) as f64 * rise), false));
            }
            east(&mut b);
            west(&mut b);
        }
        _ => {}
    }
    b
}

/// Quader um die Feldmitte drehen (90°-Schritte)
fn rotate(min: DVec3, max: DVec3, s: f64, rot: u8) -> (DVec3, DVec3) {
    let c = DVec3::new(s / 2.0, s / 2.0, 0.0);
    let r = |p: DVec3| -> DVec3 {
        let d = p - c;
        let q = match rot % 4 {
            1 => DVec3::new(-d.y, d.x, d.z),
            2 => DVec3::new(-d.x, -d.y, d.z),
            3 => DVec3::new(d.y, -d.x, d.z),
            _ => d,
        };
        q + c
    };
    let (a, b) = (r(min), r(max));
    (a.min(b), a.max(b))
}

/// Feld unter einem Strahl (Schnitt mit der Bauebene)
pub fn cell_at(t: &TileState, o: DVec3, d: DVec3) -> Option<(i64, i64)> {
    if d.z.abs() < 1e-6 {
        return None;
    }
    let k = (t.level - o.z) / d.z;
    if k <= 0.0 {
        return None;
    }
    let p = o + d * k;
    Some(((p.x / t.size).floor() as i64, (p.y / t.size).floor() as i64))
}

/// Raum eines Feldes (für Anzeige und Entfernen)
pub fn cell_bounds(t: &TileState, c: (i64, i64)) -> Aabb {
    let min = DVec3::new(c.0 as f64 * t.size, c.1 as f64 * t.size, t.level - t.wall);
    Aabb::new(min, min + DVec3::new(t.size, t.size, t.height + 2.0 * t.wall))
}

impl Editor {
    /// Welt-Brushes, deren Mitte im Feld liegt
    fn tile_objects(&self, c: (i64, i64)) -> Vec<u32> {
        let b = cell_bounds(&self.tiles, c);
        self.map.world.solids.iter().filter(|s| b.contains_point(s.bounds.center())).map(|s| s.id).collect()
    }

    /// Feld leeren (ohne eigenen Rückgängig-Schritt, wenn `undo` falsch)
    pub fn tile_remove(&mut self, c: (i64, i64), undo: bool) -> usize {
        let ids = self.tile_objects(c);
        if ids.is_empty() {
            return 0;
        }
        if undo {
            self.checkpoint();
        }
        self.map.world.solids.retain(|s| !ids.contains(&s.id));
        self.sel.retain(|o| !matches!(o, Obj::Solid(id) if ids.contains(id)));
        self.touch();
        ids.len()
    }

    /// Baustein auf ein Feld setzen (ersetzt, was dort liegt)
    pub fn tile_place(&mut self, c: (i64, i64)) -> usize {
        self.checkpoint();
        let hold = self.checkpoint_hold;
        self.checkpoint_hold = 2;
        self.tile_remove(c, false);
        let t = self.tiles.clone();
        let origin = DVec3::new(c.0 as f64 * t.size, c.1 as f64 * t.size, t.level);
        let mut made = std::collections::BTreeSet::new();
        for (min, max, flat) in tile_boxes(&t, t.kind) {
            let (a, b) = rotate(min, max, t.size, t.rot);
            let mat = if flat { &t.floor_material } else { &t.wall_material };
            let s = self.map.make_box(Aabb::new(origin + a, origin + b), mat, self.tex_scale);
            made.insert(Obj::Solid(s.id));
            self.map.world.solids.push(s.into());
        }
        let n = made.len();
        self.sel = made;
        if n >= 2 {
            self.group_selection();
        }
        self.checkpoint_hold = hold;
        self.status = format!("{} gesetzt ({}°)", crate::i18n::tr(TILES[t.kind.min(TILES.len() - 1)].0), t.rot as u32 * 90);
        self.touch();
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_rotate_remove() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        ed.tiles.kind = 2; // Ecke
        let n = ed.tile_place((0, 0));
        assert_eq!(n, 4, "Boden, Decke, zwei Wände");
        // Wände liegen an +Y und +X
        assert!(ed.map.world.solids.iter().any(|s| s.bounds.min.y >= 256.0 - 16.0 - 1e-6 && s.bounds.max.z > 100.0));
        // gedreht um 180°: Wände an -Y und -X im Feld (1, 0)
        ed.tiles.rot = 2;
        ed.tile_place((1, 0));
        assert!(ed.map.world.solids.iter().any(|s| s.bounds.min.x >= 256.0 - 1e-6 && s.bounds.max.x <= 256.0 + 16.0 + 1e-6 && s.bounds.max.z > 100.0));
        // ersetzen statt stapeln
        let before = ed.map.world.solids.len();
        ed.tiles.kind = 0;
        ed.tile_place((1, 0));
        assert_eq!(ed.map.world.solids.len(), before - 2);
        // entfernen, auch ohne gemerkte Liste
        assert!(ed.tile_remove((0, 0), true) >= 4);
        // Treppe: Stufen steigen bis zur Deckenhöhe
        ed.tiles.kind = 8;
        ed.tile_place((5, 5));
        let top = ed.map.world.solids.iter().filter(|s| s.bounds.min.x >= 5.0 * 256.0).map(|s| s.bounds.max.z).fold(0.0, f64::max);
        assert!((top - 128.0).abs() < 1e-6, "{top}");
        // Feld unter einem Strahl
        assert_eq!(cell_at(&ed.tiles, DVec3::new(300.0, 10.0, 500.0), -DVec3::Z), Some((1, 0)));
    }
}
