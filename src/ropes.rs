//! Seile (`move_rope` / `keyframe_rope`): jedes Seil-Entity verbindet sich über `NextKey` mit dem
//! nächsten. Der Durchhang folgt aus `Slack` (Seillänge = Abstand + Slack) als Parabel – nah am
//! Ergebnis der Seil-Simulation im Spiel.

use crate::editor::Editor;
use glam::DVec3;

pub struct Rope {
    /// Entity, an dem das Seilstück beginnt
    pub ent: u32,
    pub points: Vec<DVec3>,
    pub width: f64,
    pub material: String,
}

fn is_rope(class: &str) -> bool {
    class.eq_ignore_ascii_case("move_rope") || class.eq_ignore_ascii_case("keyframe_rope")
}

/// Punkte eines durchhängenden Seils zwischen a und b
pub fn sag_points(a: DVec3, b: DVec3, slack: f64, segments: usize) -> Vec<DVec3> {
    let d = a.distance(b).max(1.0);
    // Bogenlänge einer flachen Parabel: L ≈ D + 8h²/(3D) → h aus L = D + slack
    let h = (3.0 * d * slack.max(0.0) / 8.0).sqrt();
    (0..=segments)
        .map(|k| {
            let t = k as f64 / segments as f64;
            a.lerp(b, t) - DVec3::Z * (4.0 * h * t * (1.0 - t))
        })
        .collect()
}

/// Alle Seilstücke der Map
pub fn ropes(ed: &Editor) -> Vec<Rope> {
    let mut out = Vec::new();
    for e in ed.map.entities.iter() {
        if !is_rope(e.classname()) || !ed.is_visible_entity(e) {
            continue;
        }
        let Some(next) = e.get("NextKey").or_else(|| e.get("nextkey")).map(str::trim).filter(|s| !s.is_empty()) else { continue };
        let Some(a) = e.origin() else { continue };
        let Some(t) = ed.map.entities.iter().find(|x| is_rope(x.classname()) && x.targetname().is_some_and(|n| n.eq_ignore_ascii_case(next))) else { continue };
        let Some(b) = t.origin() else { continue };
        let num = |k: &str, def: f64| e.get(k).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(def);
        let slack = num("Slack", 25.0);
        let subdiv = num("Subdiv", 2.0).clamp(0.0, 8.0) as usize;
        let width = num("Width", 2.0).clamp(0.5, 64.0);
        let material = e.get("RopeMaterial").unwrap_or("cable/cable.vmt").replace('\\', "/").to_ascii_lowercase();
        let material = material.trim_end_matches(".vmt").trim_start_matches("materials/").to_string();
        out.push(Rope { ent: e.id, points: sag_points(a, b, slack, (subdiv + 1) * 6), width, material });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rope_sags_by_slack() {
        let p = sag_points(DVec3::ZERO, DVec3::new(200.0, 0.0, 0.0), 25.0, 10);
        assert_eq!(p.len(), 11);
        let mid = p[5];
        // Länge des Polygonzugs ≈ Abstand + Slack
        let len: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((len - 225.0).abs() < 6.0, "Länge {len}");
        assert!(mid.z < -40.0, "hängt durch: {mid:?}");
        assert!(p[0].z == 0.0 && p[10].z == 0.0);

        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let a = ed.map.alloc_id();
        let mut e = crate::map::Entity::new(a, "move_rope");
        e.set("origin", "0 0 100");
        e.set("NextKey", "r2");
        ed.map.entities.push(e.into());
        let b = ed.map.alloc_id();
        let mut e = crate::map::Entity::new(b, "keyframe_rope");
        e.set("origin", "256 0 100");
        e.set("targetname", "r2");
        ed.map.entities.push(e.into());
        let r = ropes(&ed);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].material, "cable/cable");
        assert!(r[0].points.len() > 4);
    }

    /// Test-Himmel für Bildschirmtests: AMBOSS_SKY=…/materials/skybox cargo test write_test_sky -- --ignored
    /// (jede Seite eigene Farbe, weißes Quadrat oben links)
    #[test]
    #[ignore]
    fn write_test_sky() {
        let dir = std::path::PathBuf::from(std::env::var("AMBOSS_SKY").unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let cols = [("rt", [220, 60, 60]), ("lf", [60, 200, 60]), ("bk", [60, 90, 230]), ("ft", [230, 210, 60]), ("up", [90, 220, 230]), ("dn", [200, 70, 200])];
        for (suf, c) in cols {
            let (w, h) = (64u32, 64u32);
            let mut px = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    let mark = x < 16 && y < 16;
                    let k = 0.55 + 0.45 * (1.0 - y as f32 / h as f32);
                    let rgb = if mark { [255, 255, 255] } else { c.map(|v| (v as f32 * k) as u8) };
                    px.extend([rgb[0], rgb[1], rgb[2], 255]);
                }
            }
            let img = crate::vtf_write::Rgba { w, h, px };
            let vtf = crate::vtf_write::encode(&img, crate::vtf_write::VtfOpts { normal: false, clamp: true, max_size: 64 });
            std::fs::write(dir.join(format!("amboss_test{suf}.vtf")), vtf).unwrap();
            std::fs::write(dir.join(format!("amboss_test{suf}.vmt")), format!("\"UnlitGeneric\"\n{{\n\t\"$basetexture\" \"skybox/amboss_test{suf}\"\n\t\"$nofog\" \"1\"\n}}\n")).unwrap();
        }
    }
}

#[cfg(test)]
mod fog_tests {
    #[test]
    fn fog_from_controller() {
        let mut ed = crate::editor::Editor::new(crate::fgd::Fgd::default());
        let id = ed.map.alloc_id();
        let mut e = crate::map::Entity::new(id, "env_fog_controller");
        e.set("fogenable", "1");
        e.set("fogcolor", "200 170 150");
        e.set("fogstart", "100");
        e.set("fogend", "900");
        e.set("fogmaxdensity", "0.9");
        ed.map.entities.push(e.into());
        let f = crate::view3d::fog_settings(&ed).expect("Nebel erkannt");
        assert!((f.0[0] - 200.0 / 255.0).abs() < 1e-3 && f.1 == 100.0 && f.2 == 900.0 && (f.3 - 0.9).abs() < 1e-6);
    }
}
