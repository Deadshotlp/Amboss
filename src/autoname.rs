//! Beim Kompilieren Entities ohne Namen automatisch benennen, damit man sie später z. B. über Lua
//! ansteuern kann (`ents.FindByName("light_12")[1]:Fire("TurnOff")`). Läuft auf einer Kopie
//! (`amboss_build/`), die eigene Map bleibt unverändert. Die Namen sind stabil: Klasse + ID.

use crate::map::Map;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// Einstellungen (im Kompilieren-Dialog)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AutoNames {
    pub enabled: bool,
    pub lights: bool,
    pub doors: bool,
    pub props: bool,
    pub sounds: bool,
    pub effects: bool,
    /// Lua-Liste `<map>_namen.lua` neben die VMF schreiben
    pub lua_list: bool,
}

impl Default for AutoNames {
    fn default() -> Self {
        AutoNames { enabled: false, lights: true, doors: true, props: true, sounds: true, effects: true, lua_list: true }
    }
}

/// Gruppen, nach denen benannt wird
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Lights,
    Doors,
    Props,
    Sounds,
    Effects,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Lights => "Lichter",
            Group::Doors => "Türen & Knöpfe",
            Group::Props => "Props",
            Group::Sounds => "Sounds",
            Group::Effects => "Effekte",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Group::Lights => "light, light_spot, light_dynamic (light_environment lässt sich nicht schalten)",
            Group::Doors => "Türen, Knöpfe, Züge, func_brush, func_breakable, func_physbox …",
            Group::Props => "prop_dynamic und prop_physics (prop_static gibt es im Spiel nicht als Entity)",
            Group::Sounds => "ambient_generic, env_soundscape",
            Group::Effects => "Partikel, Sprites, Rauch, Feuer, Funken, Scheinwerfer, projizierte Texturen, Nebel …",
        }
    }
}

/// Gruppe einer Klasse (nur Klassen, die im Spiel als Entity bestehen bleiben und sich steuern lassen)
pub fn group_of(class: &str) -> Option<Group> {
    Some(match class {
        "light" | "light_spot" | "light_dynamic" => Group::Lights,
        "func_door" | "func_door_rotating" | "prop_door_rotating" | "func_button" | "func_rot_button" | "func_movelinear" | "func_rotating" | "func_tracktrain" | "func_train" | "func_breakable" | "func_brush" | "func_wall_toggle" | "func_physbox" | "momentary_rot_button" | "func_tanktrain" => Group::Doors,
        "prop_dynamic" | "prop_dynamic_override" | "prop_physics" | "prop_physics_multiplayer" | "prop_physics_override" | "prop_ragdoll" => Group::Props,
        "ambient_generic" | "env_soundscape" | "env_soundscape_triggerable" => Group::Sounds,
        "info_particle_system" | "env_sprite" | "env_smokestack" | "env_fire" | "env_spark" | "point_spotlight" | "env_projectedtexture" | "env_steam" | "env_beam" | "env_laser" | "func_dustmotes" | "env_shake" | "env_fog_controller" | "env_lightglow" | "env_screenoverlay" => Group::Effects,
        _ => return None,
    })
}

fn wanted(o: &AutoNames, g: Group) -> bool {
    match g {
        Group::Lights => o.lights,
        Group::Doors => o.doors,
        Group::Props => o.props,
        Group::Sounds => o.sounds,
        Group::Effects => o.effects,
    }
}

/// Ein benanntes Entity (für Bericht und Lua-Liste)
#[derive(Debug, Clone, PartialEq)]
pub struct Named {
    pub name: String,
    pub class: String,
    pub pos: [f64; 3],
    /// von Amboss vergeben (sonst schon vorher benannt)
    pub auto: bool,
}

/// Unbenannte Entities der gewählten Gruppen benennen; liefert alle benannten Entities dieser Gruppen
pub fn apply(map: &mut Map, o: &AutoNames) -> Vec<Named> {
    let mut used: std::collections::HashSet<String> = map.entities.iter().filter_map(|e| e.get("targetname").map(|s| s.to_ascii_lowercase())).collect();
    let mut out = Vec::new();
    for e in map.entities.iter_mut() {
        let class = e.classname().to_string();
        let Some(g) = group_of(&class) else { continue };
        if !wanted(o, g) {
            continue;
        }
        let pos = e.origin().or_else(|| (!e.solids.is_empty()).then(|| e.solids.iter().fold(crate::geometry::Aabb::EMPTY, |b, s| b.union(&s.bounds)).center())).unwrap_or_default();
        let existing = e.get("targetname").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let (name, auto) = match existing {
            Some(n) => (n, false),
            None => {
                let base = format!("{class}_{}", e.id);
                let mut name = base.clone();
                let mut k = 2;
                while used.contains(&name.to_ascii_lowercase()) {
                    name = format!("{base}_{k}");
                    k += 1;
                }
                used.insert(name.to_ascii_lowercase());
                e.set("targetname", name.clone());
                (name, true)
            }
        };
        out.push(Named { name, class, pos: [pos.x, pos.y, pos.z], auto });
    }
    out
}

/// Lua-Datei mit allen Namen (Tabelle, die man in ein Addon kopieren oder per include laden kann)
pub fn lua_list(map_name: &str, named: &[Named]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "-- Benannte Entities der Map \"{map_name}\" – von Amboss beim Kompilieren geschrieben.");
    let _ = writeln!(s, "-- auto = true: Name wurde automatisch vergeben (Klasse_ID, bleibt bei jedem Kompilieren gleich).");
    let _ = writeln!(s, "-- Beispiel (Server):");
    let _ = writeln!(s, "--   for _, e in ipairs(ents.FindByName(\"light_12\")) do e:Fire(\"TurnOff\") end");
    let _ = writeln!(s, "--   for _, n in ipairs(AMBOSS_NAMES) do if n.class == \"light\" then ... end end");
    let _ = writeln!(s, "AMBOSS_NAMES = {{");
    for n in named {
        let _ = writeln!(s, "\t{{ name = \"{}\", class = \"{}\", pos = Vector({}, {}, {}), auto = {} }},", n.name.replace('"', "\\\""), n.class, crate::map::fmt_num(n.pos[0]), crate::map::fmt_num(n.pos[1]), crate::map::fmt_num(n.pos[2]), n.auto);
    }
    let _ = writeln!(s, "}}");
    let _ = writeln!(s, "return AMBOSS_NAMES");
    s
}

/// Vor dem Kompilieren: Kopie mit Namen schreiben. `vmf` ist die Datei, die kompiliert würde
/// (Original, Cordon- oder Optimierungs-Kopie); `src` die gespeicherte Map.
pub fn prepare(vmf: &Path, src: &Path, o: &AutoNames) -> Result<(PathBuf, Vec<String>), String> {
    let text = std::fs::read_to_string(vmf).map_err(|e| format!("{}: {e}", vmf.display()))?;
    let mut map = Map::from_vmf(&text)?;
    let named = apply(&mut map, o);
    let auto: Vec<&Named> = named.iter().filter(|n| n.auto).collect();
    let mut lines = Vec::new();
    let mut out = vmf.to_path_buf();
    if !auto.is_empty() {
        let in_copy = vmf.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "amboss_build" || n == "cordon");
        if !in_copy {
            let dir = vmf.parent().map(|p| p.join("amboss_build")).unwrap_or_else(|| "amboss_build".into());
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            out = dir.join(vmf.file_name().unwrap_or_default());
        }
        std::fs::write(&out, map.to_vmf()).map_err(|e| format!("{}: {e}", out.display()))?;
        let mut counts: std::collections::BTreeMap<Group, usize> = Default::default();
        for n in &auto {
            if let Some(g) = group_of(&n.class) {
                *counts.entry(g).or_default() += 1;
            }
        }
        let parts: Vec<String> = counts.iter().map(|(g, c)| format!("{c} {}", crate::i18n::tr(g.label()))).collect();
        lines.push(format!("Automatisch benannt: {}", parts.join(", ")));
        if let Some(l) = counts.get(&Group::Lights) {
            lines.push(format!(
                "Hinweis: {l} Lichter sind jetzt schaltbar. Jedes benannte Licht braucht eine eigene Lightmap-Ebene (höchstens 4 pro Fläche) – meldet VRAD „too many light styles“, weniger Lichter benennen."
            ));
        }
    }
    if o.lua_list && !named.is_empty() {
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let lp = src.with_file_name(format!("{stem}_namen.lua"));
        std::fs::write(&lp, lua_list(&stem, &named)).map_err(|e| format!("{}: {e}", lp.display()))?;
        lines.push(format!("{} Namen → {}", named.len(), lp.display()));
    }
    Ok((out, lines))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Entity;

    #[test]
    fn names_and_lua() {
        let mut m = Map::new();
        for (class, name) in [("light", None), ("light", Some("flur")), ("prop_static", None), ("func_door", None), ("light_environment", None), ("ambient_generic", None)] {
            let id = m.alloc_id();
            let mut e = Entity::new(id, class);
            e.set("origin", "1 2 3");
            if let Some(n) = name {
                e.set("targetname", n);
            }
            m.entities.push(e.into());
        }
        // Name schon belegt → Zusatz
        let taken = format!("light_{}", m.entities[0].id);
        let id = m.alloc_id();
        let mut e = Entity::new(id, "info_target");
        e.set("targetname", taken.clone());
        m.entities.push(e.into());
        let o = AutoNames { enabled: true, sounds: false, ..Default::default() };
        let named = apply(&mut m, &o);
        assert_eq!(named.len(), 3, "{named:?}");
        assert_eq!(m.entities[0].get("targetname"), Some(format!("{taken}_2").as_str()));
        assert_eq!(m.entities[1].get("targetname"), Some("flur"));
        assert!(m.entities[2].get("targetname").is_none(), "prop_static lässt sich nicht ansteuern");
        assert_eq!(m.entities[3].get("targetname"), Some(format!("func_door_{}", m.entities[3].id).as_str()));
        assert!(m.entities[4].get("targetname").is_none());
        assert!(m.entities[5].get("targetname").is_none(), "Sounds abgewählt");
        assert!(named.iter().any(|n| n.name == "flur" && !n.auto));
        let lua = lua_list("test", &named);
        assert!(lua.contains("{ name = \"flur\", class = \"light\", pos = Vector(1, 2, 3), auto = false },"), "{lua}");
        // Kopie im Build-Ordner, Lua-Liste neben der VMF
        let dir = std::env::temp_dir().join(format!("amboss_autoname_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("karte.vmf");
        let mut m2 = Map::new();
        let id = m2.alloc_id();
        let mut e = Entity::new(id, "light_spot");
        e.set("origin", "0 0 0");
        m2.entities.push(e.into());
        std::fs::write(&src, m2.to_vmf()).unwrap();
        let (out, lines) = prepare(&src, &src, &o).unwrap();
        assert!(out.ends_with("amboss_build/karte.vmf"));
        let back = Map::from_vmf(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert!(back.entities[0].get("targetname").unwrap().starts_with("light_spot_"));
        assert!(lines.iter().any(|l| l.contains("Lightmap-Ebene")));
        assert!(dir.join("karte_namen.lua").is_file());
        assert!(Map::from_vmf(&std::fs::read_to_string(&src).unwrap()).unwrap().entities[0].get("targetname").is_none(), "Original bleibt");
        std::fs::remove_dir_all(&dir).ok();
    }
}
