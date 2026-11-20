//! Gameplay-Maße: Spielerfigur in Originalgröße mit Stufen-, Sprung-, Duck- und Augenhöhe,
//! dazu eine Prüfung, ob der Spieler an einer Stelle stehen oder nur geduckt durchpassen kann.
//! Werte wie in Garry's Mod (Sandbox): Sprungkraft 200, Schwerkraft 600.

use crate::editor::{Editor, ToolKind};
use crate::map::{Entity, Solid};
use glam::DVec3;

/// Breite der Spieler-Hülle (32 × 32)
pub const WIDTH: f64 = 32.0;
/// Höhe stehend
pub const STAND: f64 = 72.0;
/// Höhe geduckt
pub const DUCK: f64 = 36.0;
/// Augenhöhe stehend
pub const EYE: f64 = 64.0;
/// höchste Stufe, die man ohne Springen hochläuft
pub const STEP: f64 = 18.0;
/// Sprungkraft und Schwerkraft (GMod-Standard)
pub const JUMP_POWER: f64 = 200.0;
pub const GRAVITY: f64 = 600.0;

/// Sprunghöhe: v² / 2g
pub fn jump_height() -> f64 {
    JUMP_POWER * JUMP_POWER / (2.0 * GRAVITY)
}

/// Ducksprung: beim Ducken in der Luft zieht der Spieler die Beine um (stehend − geduckt) an
pub fn duck_jump_height() -> f64 {
    jump_height() + (STAND - DUCK)
}

/// Markierungen an der Figur: (Höhe, Beschriftung, Farbe RGB)
pub fn marks() -> Vec<(f64, String, [u8; 3])> {
    vec![
        (STEP, crate::i18n::tr_dyn(&format!("Stufe {}", STEP as i64)), [120, 220, 120]),
        (jump_height(), crate::i18n::tr_dyn(&format!("Sprung ca. {}", jump_height().round() as i64)), [120, 190, 255]),
        (DUCK, crate::i18n::tr_dyn(&format!("geduckt {}", DUCK as i64)), [255, 200, 90]),
        (EYE, crate::i18n::tr_dyn(&format!("Augen {}", EYE as i64)), [230, 230, 230]),
        (duck_jump_height(), crate::i18n::tr_dyn(&format!("Ducksprung ca. {}", duck_jump_height().round() as i64)), [180, 140, 255]),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// stehend passt
    Stand,
    /// nur geduckt
    Duck,
    /// gar nicht
    Blocked,
}

/// Blockiert dieser Brush den Spieler? Trigger, Hint/Skip, Portale und ähnliche Werkzeugflächen nicht.
pub(crate) fn blocks(owner: Option<&Entity>, s: &Solid) -> bool {
    if let Some(e) = owner {
        let c = e.classname().to_ascii_lowercase();
        if c.starts_with("trigger_") || matches!(c.as_str(), "func_illusionary" | "func_areaportal" | "func_areaportalwindow" | "func_occluder" | "func_viewportal" | "func_dustmotes" | "func_dustcloud" | "func_smokevolume" | "func_precipitation" | "func_buyzone" | "func_bomb_target" | "func_hostage_rescue" | "func_ladder" | "func_lod" | "func_clip_vphysics") {
            return false;
        }
        // nicht feste Brush-Entities (z. B. func_brush mit Solidity „Never“)
        if e.get("solidity") == Some("1") || e.get("solid") == Some("0") {
            return false;
        }
    }
    // ein Brush nur aus durchlässigen Werkzeugflächen blockiert nicht (npcclip hält Spieler nicht auf)
    !s.sides.iter().all(|sd| {
        let m = sd.material.to_ascii_lowercase();
        matches!(crate::editor::tool_kind(&sd.material), Some(ToolKind::Trigger | ToolKind::Hint | ToolKind::Portal)) || m.ends_with("toolsnpcclip") || m.ends_with("toolsoccluder") || m.ends_with("toolsblocklight") || m.ends_with("toolsfog")
    })
}

/// Schneidet die Box [min, max] den konvexen Brush? (Trennachsen: Ebenen des Brushes und Achsen der Box)
pub(crate) fn box_hits_solid(min: DVec3, max: DVec3, s: &Solid) -> bool {
    if s.bounds.max.cmplt(min).any() || s.bounds.min.cmpgt(max).any() {
        return false;
    }
    let c = (min + max) * 0.5;
    let h = (max - min) * 0.5;
    for pl in s.planes.iter().flatten() {
        // Abstand des nächsten Box-Punkts zur Ebene; liegt die ganze Box vor der Ebene, sind sie getrennt
        let r = h.x * pl.normal.x.abs() + h.y * pl.normal.y.abs() + h.z * pl.normal.z.abs();
        if pl.normal.dot(c) - pl.dist > r - 0.01 {
            return false;
        }
    }
    true
}

/// Passt eine Spieler-Hülle der Höhe `height` mit Fußpunkt `foot` (Mitte unten) hin?
pub fn hull_free(ed: &Editor, foot: DVec3, height: f64) -> bool {
    let min = foot + DVec3::new(-WIDTH / 2.0, -WIDTH / 2.0, 0.5);
    let max = foot + DVec3::new(WIDTH / 2.0, WIDTH / 2.0, height);
    !ed.map.solids_owned().any(|(owner, s)| ed.is_visible_solid_all(owner, s) && blocks(owner, s) && box_hits_solid(min, max, s))
}

pub fn fit(ed: &Editor, foot: DVec3) -> Fit {
    if hull_free(ed, foot, STAND) {
        Fit::Stand
    } else if hull_free(ed, foot, DUCK) {
        Fit::Duck
    } else {
        Fit::Blocked
    }
}

pub fn fit_color(f: Fit) -> [u8; 3] {
    match f {
        Fit::Stand => [90, 220, 120],
        Fit::Duck => [255, 200, 70],
        Fit::Blocked => [255, 90, 90],
    }
}

pub fn fit_label(f: Fit) -> &'static str {
    match f {
        Fit::Stand => crate::i18n::tr("Spieler passt hierhin"),
        Fit::Duck => crate::i18n::tr("Nur geduckt"),
        Fit::Blocked => crate::i18n::tr("Spieler passt nicht"),
    }
}

/// Kanten einer achsparallelen Box (12 Linien)
pub fn box_edges(min: DVec3, max: DVec3) -> Vec<(DVec3, DVec3)> {
    let c = |x: bool, y: bool, z: bool| DVec3::new(if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z });
    let mut v = Vec::with_capacity(12);
    for &(a, b) in &[(false, false), (true, false), (true, true), (false, true)] {
        v.push((c(a, b, false), c(a, b, true)));
    }
    for z in [false, true] {
        v.push((c(false, false, z), c(true, false, z)));
        v.push((c(true, false, z), c(true, true, z)));
        v.push((c(true, true, z), c(false, true, z)));
        v.push((c(false, true, z), c(false, false, z)));
    }
    v
}

/// Linien der Figur in 3D: Hülle (stehend oder geduckt) und waagrechte Ringe an den Markierungen
pub fn figure_lines(foot: DVec3, f: Fit) -> Vec<(DVec3, DVec3, [f32; 4])> {
    let col = fit_color(f);
    let c = [col[0] as f32 / 255.0, col[1] as f32 / 255.0, col[2] as f32 / 255.0, 1.0];
    let h = if f == Fit::Duck { DUCK } else { STAND };
    let half = DVec3::new(WIDTH / 2.0, WIDTH / 2.0, 0.0);
    let mut out: Vec<(DVec3, DVec3, [f32; 4])> = box_edges(foot - half, foot + half + DVec3::Z * h).into_iter().map(|(a, b)| (a, b, c)).collect();
    for (z, _, mc) in marks().into_iter().filter(|m| m.0 <= h + 0.1) {
        let m = [mc[0] as f32 / 255.0, mc[1] as f32 / 255.0, mc[2] as f32 / 255.0, 0.9];
        let o = foot + DVec3::Z * z;
        let k = [o + DVec3::new(-16.0, -16.0, 0.0), o + DVec3::new(16.0, -16.0, 0.0), o + DVec3::new(16.0, 16.0, 0.0), o + DVec3::new(-16.0, 16.0, 0.0)];
        for i in 0..4 {
            out.push((k[i], k[(i + 1) % 4], m));
        }
    }
    out
}

/// Beschriftungen übereinander ohne Überlappung: (Punkt, Beschriftungs-y, Text, Farbe); von unten nach oben,
/// jede mindestens `gap` Pixel über der vorigen
pub fn spread_labels(mut pts: Vec<(eframe::egui::Pos2, String, [u8; 3])>, gap: f32) -> Vec<(eframe::egui::Pos2, f32, String, [u8; 3])> {
    pts.sort_by(|a, b| b.0.y.total_cmp(&a.0.y));
    let mut last: Option<f32> = None;
    pts.into_iter()
        .map(|(p, l, c)| {
            let y = match last {
                Some(prev) => p.y.min(prev - gap),
                None => p.y,
            };
            last = Some(y);
            (p, y, l, c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights() {
        assert!((jump_height() - 33.33).abs() < 0.1);
        assert!((duck_jump_height() - 69.33).abs() < 0.1);
        assert_eq!(box_edges(DVec3::ZERO, DVec3::ONE).len(), 12);
        let p = |y: f32| eframe::egui::pos2(0.0, y);
        let l = spread_labels(vec![(p(100.0), "a".into(), [0; 3]), (p(95.0), "b".into(), [0; 3]), (p(50.0), "c".into(), [0; 3])], 12.0);
        assert_eq!(l.iter().map(|x| x.1).collect::<Vec<_>>(), vec![100.0, 88.0, 50.0]);
    }

    #[test]
    fn fit_under_ceiling() {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        // Boden 0, Decke auf 50 (ein Brush darüber): stehen geht nicht, ducken schon
        ed.create_block(crate::geometry::Aabb::new(DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0)));
        ed.create_block(crate::geometry::Aabb::new(DVec3::new(-256.0, -256.0, 50.0), DVec3::new(256.0, 256.0, 66.0)));
        assert_eq!(fit(&ed, DVec3::ZERO), Fit::Duck);
        assert_eq!(fit(&ed, DVec3::new(1000.0, 0.0, 0.0)), Fit::Stand);
        // Decke auf 20: gar nicht
        let mut ed2 = Editor::new(crate::fgd::Fgd::builtin());
        ed2.create_block(crate::geometry::Aabb::new(DVec3::new(-256.0, -256.0, 20.0), DVec3::new(256.0, 256.0, 40.0)));
        assert_eq!(fit(&ed2, DVec3::ZERO), Fit::Blocked);
        // Trigger blockiert nicht
        let mut ed3 = Editor::new(crate::fgd::Fgd::builtin());
        ed3.material = "TOOLS/TOOLSTRIGGER".into();
        ed3.create_block(crate::geometry::Aabb::new(DVec3::new(-256.0, -256.0, 20.0), DVec3::new(256.0, 256.0, 40.0)));
        assert_eq!(fit(&ed3, DVec3::ZERO), Fit::Stand);
    }
}
