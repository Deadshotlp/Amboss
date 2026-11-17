//! "Auf Probleme prüfen" (wie Hammer, Alt+P) mit automatischen Korrekturen.

use crate::editor::{Editor, Obj};
use crate::fgd::ClassKind;
use crate::geometry::Aabb;
use crate::map::fmt_vec;
use glam::DVec3;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub enum Fix {
    Delete(Vec<Obj>),
    RemoveKeys(u32, Vec<String>),
    RemoveOutputs(u32, Vec<usize>),
    ReplaceMaterial(String, String),
    CleanupSolid(u32),
    DestroyDisp(u32, usize),
    SolidsToWorld(u32),
    CreatePlayerStart,
}

impl Fix {
    pub fn label(&self) -> &'static str {
        match self {
            Fix::Delete(_) => "Löschen",
            Fix::RemoveKeys(..) => "Schlüssel entfernen",
            Fix::RemoveOutputs(..) => "Outputs entfernen",
            Fix::ReplaceMaterial(..) => "Material ersetzen",
            Fix::CleanupSolid(_) => "Leere Seiten entfernen",
            Fix::DestroyDisp(..) => "Displacement entfernen",
            Fix::SolidsToWorld(_) => "Brushes in Welt verschieben",
            Fix::CreatePlayerStart => "info_player_start erstellen",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Problem {
    pub severity: Severity,
    pub text: String,
    pub objs: Vec<Obj>,
    pub fix: Option<Fix>,
}

const SPECIAL_TARGETS: [&str; 7] = ["!activator", "!caller", "!self", "!player", "!pvsplayer", "!speechtarget", "!picker"];
/// Schlüssel, die nie in FGDs stehen, aber gültig sind
const STANDARD_KEYS: [&str; 8] = ["classname", "origin", "angles", "spawnflags", "targetname", "model", "hammerid", "id"];

pub fn check(ed: &Editor, known_materials: Option<&HashSet<String>>) -> Vec<Problem> {
    let mut out = Vec::new();
    let map = &ed.map;
    let fgd_loaded = !ed.fgd.files.is_empty();

    if !map.entities.iter().any(|e| e.classname().starts_with("info_player")) {
        out.push(Problem { severity: Severity::Error, text: "Kein info_player_start vorhanden.".into(), objs: vec![], fix: Some(Fix::CreatePlayerStart) });
    }
    if !map.entities.iter().any(|e| e.classname().starts_with("light")) {
        out.push(Problem { severity: Severity::Warning, text: "Keine Lichtquelle – die Map wird nach VRAD komplett dunkel.".into(), objs: vec![], fix: None });
    }

    let names: HashSet<String> = map.entities.iter().filter_map(|e| e.targetname().map(|s| s.to_ascii_lowercase())).collect();
    let classes: HashSet<String> = map.entities.iter().map(|e| e.classname().to_ascii_lowercase()).collect();

    for (owner, s) in map.all_solids() {
        let obj = ed.obj_for(owner, s.id);
        if !s.is_valid() {
            out.push(Problem { severity: Severity::Error, text: format!("Ungültiger Brush (ID {}).", s.id), objs: vec![Obj::Solid(s.id)], fix: Some(Fix::Delete(vec![Obj::Solid(s.id)])) });
            continue;
        }
        if s.polys.iter().any(|p| p.len() < 3) {
            out.push(Problem { severity: Severity::Warning, text: format!("Brush {} hat Seiten ohne Fläche.", s.id), objs: vec![obj], fix: Some(Fix::CleanupSolid(s.id)) });
        }
        for (i, side) in s.sides.iter().enumerate() {
            if side.disp.is_some() && s.polys.get(i).map_or(0, |p| p.len()) != 4 {
                out.push(Problem { severity: Severity::Error, text: format!("Displacement auf nicht-viereckiger Fläche (Brush {}).", s.id), objs: vec![obj], fix: Some(Fix::DestroyDisp(s.id, i)) });
            }
        }
        if s.bounds.min.abs().max_element() > ed.map_limit || s.bounds.max.abs().max_element() > ed.map_limit {
            out.push(Problem { severity: Severity::Error, text: format!("Brush {} liegt außerhalb der Map-Grenze (±{}).", s.id, ed.map_limit), objs: vec![obj], fix: Some(Fix::Delete(vec![obj])) });
        }
    }

    // doppelte Brushes
    let world: Vec<_> = map.world.solids.iter().collect();
    for i in 0..world.len() {
        for j in (i + 1)..world.len() {
            let (a, b) = (world[i], world[j]);
            if a.bounds == b.bounds && a.planes.len() == b.planes.len() && a.planes.iter().all(|p| b.planes.iter().any(|q| matches!((p, q), (Some(p), Some(q)) if p.normal.dot(q.normal) > 0.9999 && (p.dist - q.dist).abs() < 0.01))) {
                out.push(Problem { severity: Severity::Warning, text: format!("Brush {} ist ein Duplikat von {}.", b.id, a.id), objs: vec![Obj::Solid(b.id)], fix: Some(Fix::Delete(vec![Obj::Solid(b.id)])) });
            }
        }
    }

    // fehlende Materialien
    if let Some(known) = known_materials {
        let mut missing: Vec<String> = Vec::new();
        for (_, s) in map.all_solids() {
            for side in &s.sides {
                let m = side.material.to_ascii_lowercase().replace('\\', "/");
                if !known.contains(&m) && !missing.contains(&m) {
                    missing.push(m);
                }
            }
        }
        for m in missing {
            let objs: Vec<Obj> = map.all_solids().filter(|(_, s)| s.sides.iter().any(|x| x.material.eq_ignore_ascii_case(&m))).map(|(o, s)| ed.obj_for(o, s.id)).collect();
            out.push(Problem {
                severity: Severity::Warning,
                text: format!("Material \"{m}\" wurde in den Spielinhalten nicht gefunden ({} Brush(es)).", objs.len()),
                objs,
                fix: Some(Fix::ReplaceMaterial(m, "DEV/DEV_MEASUREGENERIC01B".into())),
            });
        }
    }

    for e in &map.entities {
        let obj = Obj::Entity(e.id);
        let class = e.classname();
        let def = ed.fgd.class(class);
        match def {
            None if fgd_loaded || !class.is_empty() && ed.fgd.classes.len() > 100 => {
                out.push(Problem { severity: Severity::Info, text: format!("Unbekannte Entity-Klasse \"{class}\" (ID {}).", e.id), objs: vec![obj], fix: None });
            }
            Some(d) if d.kind == ClassKind::Solid && e.solids.is_empty() => {
                out.push(Problem { severity: Severity::Error, text: format!("Brush-Entity {class} (ID {}) hat keine Brushes.", e.id), objs: vec![obj], fix: Some(Fix::Delete(vec![obj])) });
            }
            Some(d) if d.kind == ClassKind::Point && !e.solids.is_empty() => {
                out.push(Problem { severity: Severity::Error, text: format!("Punkt-Entity {class} (ID {}) enthält Brushes.", e.id), objs: vec![obj], fix: Some(Fix::SolidsToWorld(e.id)) });
            }
            _ => {}
        }
        if class.is_empty() {
            out.push(Problem { severity: Severity::Error, text: format!("Entity {} hat keine Klasse.", e.id), objs: vec![obj], fix: Some(Fix::Delete(vec![obj])) });
        }
        if fgd_loaded && def.is_some() {
            let props = ed.fgd.all_props(class);
            let unused: Vec<String> = e
                .props
                .iter()
                .map(|(k, _)| k.clone())
                .filter(|k| !STANDARD_KEYS.iter().any(|s| s.eq_ignore_ascii_case(k)) && !props.iter().any(|p| p.name.eq_ignore_ascii_case(k)))
                .filter(|k| !k.starts_with("BasisO") && !k.starts_with("uv") && k != "sides")
                .collect();
            if !unused.is_empty() {
                out.push(Problem {
                    severity: Severity::Info,
                    text: format!("{class} (ID {}): Schlüssel nicht in der FGD: {}", e.id, unused.join(", ")),
                    objs: vec![obj],
                    fix: Some(Fix::RemoveKeys(e.id, unused)),
                });
            }
        }
        // Outputs prüfen
        let mut bad = Vec::new();
        for (i, (_, v)) in e.outputs.iter().enumerate() {
            let target = v.split(['\u{1b}', ',']).next().unwrap_or("").trim().to_ascii_lowercase();
            if target.is_empty() || SPECIAL_TARGETS.contains(&target.as_str()) || target.contains('*') {
                if target.is_empty() {
                    bad.push(i);
                }
                continue;
            }
            if !names.contains(&target) && !classes.contains(&target) {
                bad.push(i);
            }
        }
        if !bad.is_empty() {
            out.push(Problem {
                severity: Severity::Warning,
                text: format!("{class} (ID {}): {} Output(s) zielen auf nicht existierende Namen.", e.id, bad.len()),
                objs: vec![obj],
                fix: Some(Fix::RemoveOutputs(e.id, bad)),
            });
        }
        if !e.is_brush_entity() {
            if let Some(o) = e.origin() {
                if o.abs().max_element() > ed.map_limit {
                    out.push(Problem { severity: Severity::Error, text: format!("{class} (ID {}) liegt außerhalb der Map-Grenze.", e.id), objs: vec![obj], fix: Some(Fix::Delete(vec![obj])) });
                }
                if class.starts_with("info_player") {
                    let b = ed.entity_bounds(e);
                    let shrink = Aabb::new(b.min + DVec3::splat(1.0), b.max - DVec3::splat(1.0));
                    if map.world.solids.iter().any(|s| s.bounds.intersects(&shrink) && s.planes.iter().flatten().all(|p| shrink.corners().iter().any(|c| p.distance(*c) < 0.0))) {
                        out.push(Problem { severity: Severity::Warning, text: format!("Spawnpunkt {} steckt in einem Brush.", e.id), objs: vec![obj], fix: None });
                    }
                }
            } else if class != "worldspawn" {
                out.push(Problem { severity: Severity::Warning, text: format!("{class} (ID {}) hat keinen Ursprung.", e.id), objs: vec![obj], fix: None });
            }
        }
    }
    if let Some(c) = map.cordon.filter(|c| c.active) {
        if !map.entities.iter().any(|e| e.classname().starts_with("info_player") && e.origin().is_some_and(|o| c.bounds.contains_point(o))) {
            out.push(Problem { severity: Severity::Warning, text: "Cordon aktiv, aber kein Spawnpunkt im Cordon-Bereich.".into(), objs: vec![], fix: None });
        }
    }
    out.sort_by_key(|p| p.severity);
    out
}

/// Wendet eine Korrektur an (ohne eigenen Undo-Punkt)
pub fn apply(ed: &mut Editor, fix: &Fix) {
    match fix {
        Fix::Delete(objs) => {
            let prev = std::mem::replace(&mut ed.sel, objs.iter().copied().collect());
            ed.delete_selection();
            ed.sel = prev.into_iter().filter(|o| !objs.contains(o)).collect();
        }
        Fix::RemoveKeys(id, keys) => {
            if let Some(e) = ed.map.entity_mut(*id) {
                for k in keys {
                    e.remove(k);
                }
            }
        }
        Fix::RemoveOutputs(id, idx) => {
            if let Some(e) = ed.map.entity_mut(*id) {
                let mut i = 0;
                e.outputs.retain(|_| {
                    let keep = !idx.contains(&i);
                    i += 1;
                    keep
                });
            }
        }
        Fix::ReplaceMaterial(from, to) => {
            for s in ed.map.world.solids.iter_mut().chain(ed.map.entities.iter_mut().flat_map(|e| e.solids.iter_mut())) {
                for side in &mut s.sides {
                    if side.material.eq_ignore_ascii_case(from) {
                        side.material = to.clone();
                    }
                }
            }
        }
        Fix::CleanupSolid(id) => {
            if let Some(s) = ed.map.solid_mut(*id) {
                s.cleanup();
            }
        }
        Fix::DestroyDisp(id, i) => {
            if let Some(s) = ed.map.solid_mut(*id) {
                if let Some(side) = s.sides.get_mut(*i) {
                    side.disp = None;
                }
                s.update_geometry();
            }
        }
        Fix::SolidsToWorld(id) => {
            if let Some(e) = ed.map.entity_mut(*id) {
                let solids = std::mem::take(&mut e.solids);
                ed.map.world.solids.extend(solids);
            }
        }
        Fix::CreatePlayerStart => {
            let class = std::mem::replace(&mut ed.entity_class, "info_player_start".into());
            let p = ed.work_point;
            ed.create_point_entity(p);
            ed.entity_class = class;
            ed.status = format!("info_player_start bei {} erstellt", fmt_vec(p));
        }
    }
    ed.dirty = true;
    ed.touch();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;
    use crate::map::Entity;

    #[test]
    fn finds_and_fixes() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let dup = ed.map.world.solids[0].clone();
        let mut dup = dup;
        ed.map.reassign_ids(&mut dup);
        ed.map.world.solids.push(dup);
        let mut e = Entity::new(ed.map.alloc_id(), "logic_relay");
        e.set("origin", "0 0 0");
        e.outputs.push(("OnTrigger".into(), "gibtsnicht,Kill,,0,-1".into()));
        ed.map.entities.push(e.into());
        let p = check(&ed, None);
        assert!(p.iter().any(|x| x.text.contains("info_player_start")));
        assert!(p.iter().any(|x| x.text.contains("Duplikat")));
        assert!(p.iter().any(|x| x.text.contains("Output")));
        for prob in &p {
            if let Some(f) = &prob.fix {
                apply(&mut ed, f);
            }
        }
        let p2 = check(&ed, None);
        assert!(!p2.iter().any(|x| x.text.contains("Duplikat") || x.text.contains("Output") || x.text.contains("info_player_start")));
    }
}
