//! Vorschau, wohin sich Türen, Knöpfe und bewegliche Brushes bewegen (für ausgewählte Entities).
//!
//! Lineare Bewegung (func_door, func_button, func_movelinear, func_water_analog): Pfeil in
//! `movedir` und Umriss der offenen Position. Drehtüren (func_door_rotating, prop_door_rotating):
//! Bogen um die Angel und Umriss der geöffneten Tür – bei Türen, die in beide Richtungen
//! aufgehen, beide Seiten (die zweite blasser).

use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use crate::map::{parse_vec, Entity};
use glam::{DQuat, DVec3};

pub type Seg = (DVec3, DVec3, [f32; 4]);

const MOVE: [f32; 4] = [1.0, 0.75, 0.15, 1.0];
const GHOST: [f32; 4] = [0.3, 0.9, 1.0, 0.95];
const GHOST2: [f32; 4] = [0.3, 0.9, 1.0, 0.4];

fn flags(e: &Entity) -> u32 {
    e.get("spawnflags").and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

fn num(e: &Entity, k: &str, d: f64) -> f64 {
    e.get(k).and_then(|v| v.trim().parse().ok()).unwrap_or(d)
}

/// Bewegungsrichtung aus `movedir` (Pitch Yaw Roll), mit den Sonderwerten für hoch/runter
pub fn move_dir(e: &Entity) -> DVec3 {
    let a = e.get("movedir").and_then(parse_vec).unwrap_or(DVec3::ZERO);
    // alte Schreibweise: Yaw -1 = hoch, -2 = runter
    if a.x == 0.0 && a.z == 0.0 {
        if a.y == -1.0 {
            return DVec3::Z;
        }
        if a.y == -2.0 {
            return -DVec3::Z;
        }
    }
    crate::transform::angles_to_basis(a).0.normalize_or_zero()
}

fn box_edges(c: &[DVec3; 8], col: [f32; 4], out: &mut Vec<Seg>) {
    // Ecken in der Reihenfolge von Aabb::corners: Bit 0 = x, Bit 1 = y, Bit 2 = z
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                out.push((c[i], c[i | bit], col));
            }
        }
    }
}

fn corners(b: &Aabb) -> [DVec3; 8] {
    let (lo, hi) = (b.min, b.max);
    std::array::from_fn(|i| DVec3::new(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z }))
}

fn arrow(a: DVec3, b: DVec3, col: [f32; 4], out: &mut Vec<Seg>) {
    let d = b - a;
    let len = d.length();
    if len < 1e-6 {
        return;
    }
    let dir = d / len;
    let side = if dir.z.abs() > 0.9 { DVec3::X } else { DVec3::Z };
    let h = (len * 0.2).clamp(4.0, 24.0);
    out.push((a, b, col));
    let s1 = dir.cross(side).normalize_or_zero();
    let s2 = dir.cross(s1).normalize_or_zero();
    for s in [s1, -s1, s2, -s2] {
        out.push((b, b - dir * h + s * h * 0.5, col));
    }
}

/// Drehung um `axis` durch `pivot`: Bogen + gedrehter Umriss
fn rotation(b: &Aabb, pivot: DVec3, axis: DVec3, deg: f64, col_arc: [f32; 4], col_box: [f32; 4], out: &mut Vec<Seg>) {
    let c = corners(b);
    // Bogen am Punkt, der am weitesten von der Achse entfernt ist
    let far = c.iter().copied().max_by(|p, q| {
        let dp = (*p - pivot) - axis * (*p - pivot).dot(axis);
        let dq = (*q - pivot) - axis * (*q - pivot).dot(axis);
        dp.length_squared().partial_cmp(&dq.length_squared()).unwrap()
    });
    let Some(far) = far else { return };
    let mid_h = axis * (b.center() - pivot).dot(axis);
    let r0 = (far - pivot) - axis * (far - pivot).dot(axis);
    let steps = ((deg.abs() / 7.5).ceil() as usize).max(2);
    let mut prev = pivot + mid_h + r0;
    for k in 1..=steps {
        let q = DQuat::from_axis_angle(axis, (deg * k as f64 / steps as f64).to_radians());
        let p = pivot + mid_h + q * r0;
        out.push((prev, p, col_arc));
        prev = p;
    }
    // Pfeilspitze am Ende des Bogens
    let q_end = DQuat::from_axis_angle(axis, deg.to_radians());
    let q_pre = DQuat::from_axis_angle(axis, (deg * 0.85).to_radians());
    arrow(pivot + mid_h + q_pre * r0, pivot + mid_h + q_end * r0, col_arc, out);
    let q = DQuat::from_axis_angle(axis, deg.to_radians());
    let rc: [DVec3; 8] = std::array::from_fn(|i| pivot + q * (c[i] - pivot));
    box_edges(&rc, col_box, out);
    out.push((pivot - axis * 8.0 + mid_h, pivot + axis * 8.0 + mid_h, col_arc));
}

/// Linien der Vorschau für ein Entity (leer, wenn es sich nicht bewegt)
pub fn preview(ed: &Editor, e: &Entity) -> Vec<Seg> {
    let class = e.classname().to_ascii_lowercase();
    let b = ed.entity_bounds(e);
    let mut out = Vec::new();
    if b.is_empty() {
        return out;
    }
    match class.as_str() {
        "func_door" | "func_button" | "func_movelinear" | "func_water_analog" => {
            let dir = move_dir(e);
            if dir == DVec3::ZERO {
                return out;
            }
            let dist = if class == "func_movelinear" || class == "func_water_analog" {
                num(e, "movedistance", 100.0)
            } else {
                // Weg = Ausdehnung in Bewegungsrichtung minus Lip
                let s = b.size();
                (s.x * dir.x.abs() + s.y * dir.y.abs() + s.z * dir.z.abs() - num(e, "lip", 0.0)).max(0.0)
            };
            let off = dir * dist;
            arrow(b.center(), b.center() + off, MOVE, &mut out);
            let moved = Aabb::new(b.min + off, b.max + off);
            box_edges(&corners(&moved), GHOST, &mut out);
        }
        "func_door_rotating" | "func_rot_button" | "momentary_rot_button" => {
            let f = flags(e);
            let axis = if f & 64 != 0 { DVec3::X } else if f & 128 != 0 { DVec3::Y } else { DVec3::Z };
            let mut deg = num(e, "distance", 90.0);
            if f & 2 != 0 {
                deg = -deg;
            }
            let pivot = e.origin().unwrap_or(b.center());
            rotation(&b, pivot, axis, deg, MOVE, GHOST, &mut out);
            // ohne "One-way" geht die Tür vom Spieler weg auf → auch die Gegenrichtung
            if class == "func_door_rotating" && f & 16 == 0 {
                rotation(&b, pivot, axis, -deg, [MOVE[0], MOVE[1], MOVE[2], 0.4], GHOST2, &mut out);
            }
        }
        "prop_door_rotating" => {
            let pivot = e.origin().unwrap_or(b.center());
            let deg = num(e, "distance", 90.0);
            let opendir = num(e, "opendir", 0.0) as i32;
            // Achse = "oben" der Tür
            let axis = crate::transform::angles_to_basis(e.angles()).2.normalize_or_zero();
            let axis = if axis == DVec3::ZERO { DVec3::Z } else { axis };
            match opendir {
                1 => rotation(&b, pivot, axis, deg, MOVE, GHOST, &mut out),
                2 => rotation(&b, pivot, axis, -deg, MOVE, GHOST, &mut out),
                _ => {
                    rotation(&b, pivot, axis, deg, MOVE, GHOST, &mut out);
                    rotation(&b, pivot, axis, -deg, [MOVE[0], MOVE[1], MOVE[2], 0.4], GHOST2, &mut out);
                }
            }
        }
        _ => {}
    }
    out
}

/// Vorschau für alle ausgewählten Entities (auch wenn nur einer ihrer Brushes gewählt ist)
pub fn selected(ed: &Editor) -> Vec<Seg> {
    let mut out = Vec::new();
    if ed.sel.is_empty() {
        return out;
    }
    for e in &ed.map.entities {
        let chosen = ed.sel.contains(&Obj::Entity(e.id)) || e.solids.iter().any(|s| ed.sel.contains(&Obj::Solid(s.id)));
        if chosen {
            out.extend(preview(ed, e));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door(class: &str, keys: &[(&str, &str)]) -> (Editor, u32) {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(8.0, 64.0, 112.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
        let mut e = Entity::new(ed.map.alloc_id(), class);
        for (k, v) in keys {
            e.set(k, *v);
        }
        e.solids.push(s.into());
        let id = e.id;
        ed.map.entities.push(e.into());
        (ed, id)
    }

    #[test]
    fn linear_door_moves_up_by_height_minus_lip() {
        let (mut ed, id) = door("func_door", &[("movedir", "-90 0 0"), ("lip", "12")]);
        ed.sel = [Obj::Entity(id)].into_iter().collect();
        let segs = selected(&ed);
        assert!(!segs.is_empty());
        // Pfeil von der Mitte 100 Einheiten nach oben (112 − 12)
        let (a, b, _) = segs[0];
        assert!((a - DVec3::new(4.0, 32.0, 56.0)).length() < 1e-9);
        assert!((b - DVec3::new(4.0, 32.0, 156.0)).length() < 1e-6, "{b:?}");
        assert_eq!(move_dir(&Entity::new(1, "x")), DVec3::X);
        let mut e = Entity::new(1, "x");
        e.set("movedir", "0 -2 0");
        assert_eq!(move_dir(&e), -DVec3::Z);
    }

    #[test]
    fn rotating_door_both_ways_unless_one_way() {
        let (ed, id) = door("func_door_rotating", &[("origin", "0 0 56"), ("distance", "90")]);
        let e = ed.map.entity(id).unwrap().clone();
        let both = preview(&ed, &e);
        let (mut ed2, id2) = door("func_door_rotating", &[("origin", "0 0 56"), ("distance", "90"), ("spawnflags", "16")]);
        let one = preview(&ed2, &ed2.map.entity(id2).unwrap().clone());
        assert!(both.len() > one.len());
        // geöffnet (90° um Z an der Angel bei x=0,y=0): die Tür zeigt jetzt in −X-Richtung
        assert!(one.iter().any(|(a, _, _)| (a.x + 64.0).abs() < 1e-6));
        ed2.sel.clear();
        assert!(selected(&ed2).is_empty());
    }
}
