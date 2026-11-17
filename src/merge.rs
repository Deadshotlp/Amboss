//! Zwei Maps zusammenführen: Objekte aus Map B in die offene Map übernehmen.
//! IDs werden neu vergeben, doppelte Namen umbenannt, Gruppen/Visgroups übernommen,
//! Unterschiede der Map-Eigenschaften und mögliche Probleme gemeldet.

use crate::editor::{Editor, Obj};
use crate::fgd::Fgd;
use crate::geometry::Aabb;
use crate::map::{Entity, Map, Visgroup};
use crate::transform::Xform;
use glam::DVec3;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Koordinaten unverändert übernehmen
    Keep,
    /// Neben die bestehende Map setzen (+X, mit Abstand)
    Beside,
    /// Um einen festen Versatz verschieben
    Offset(DVec3),
}

#[derive(Debug, Clone)]
pub struct MergeOptions {
    pub placement: Placement,
    /// Alles aus B in eine Visgroup „Import: <name>“ legen
    pub visgroup: bool,
    /// Gruppen und Visgroups aus B übernehmen
    pub keep_groups: bool,
    /// Diese worldspawn-Schlüssel aus B übernehmen (sonst gelten die Werte von A)
    pub take_world_keys: Vec<String>,
}

impl Default for MergeOptions {
    fn default() -> Self {
        MergeOptions { placement: Placement::Keep, visgroup: true, keep_groups: true, take_world_keys: Vec::new() }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MergeReport {
    pub solids: usize,
    pub entities: usize,
    pub renamed: Vec<(String, String)>,
    /// (Schlüssel, Wert in A, Wert in B)
    pub world_diffs: Vec<(String, String, String)>,
    pub warnings: Vec<String>,
    pub offset: DVec3,
    pub objects: Vec<Obj>,
}

impl MergeReport {
    pub fn summary(&self) -> String {
        let mut s = format!("Zusammengeführt: {} Brushes, {} Entities", self.solids, self.entities);
        if self.offset != DVec3::ZERO {
            s += &format!(", verschoben um {} {} {}", self.offset.x, self.offset.y, self.offset.z);
        }
        if !self.renamed.is_empty() {
            s += &format!(", {} Name(n) umbenannt", self.renamed.len());
        }
        if !self.warnings.is_empty() {
            s += &format!(", {} Hinweis(e)", self.warnings.len());
        }
        s
    }
}

/// Schlüssel, deren Werte Entity-Namen sind
pub fn is_name_key(fgd: &Fgd, class: &str, key: &str) -> bool {
    const KEYS: [&str; 12] = ["targetname", "target", "parentname", "filtername", "damagefilter", "lightingorigin", "landmark", "master", "measurereference", "measuretarget", "attach1", "attach2"];
    if KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)) {
        return true;
    }
    fgd.all_props(class).iter().any(|p| p.name.eq_ignore_ascii_case(key) && (p.kind.starts_with("target_") || p.kind == "filterclass"))
}

/// Namen der Entities in `ents`, die in `existing` schon vorkommen, eindeutig machen (`name_2`),
/// inklusive aller Verweise (Schlüssel und Outputs) innerhalb von `ents`.
pub fn rename_conflicts(fgd: &Fgd, existing: &[String], ents: &mut [crate::map::Shared<Entity>]) -> Vec<(String, String)> {
    let existing: BTreeSet<String> = existing.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut taken = existing.clone();
    for e in ents.iter() {
        if let Some(n) = e.targetname() {
            taken.insert(n.to_ascii_lowercase());
        }
    }
    let mut rename: Vec<(String, String)> = Vec::new();
    for e in ents.iter() {
        if let Some(n) = e.targetname() {
            if existing.contains(&n.to_ascii_lowercase()) && !rename.iter().any(|(o, _)| o.eq_ignore_ascii_case(n)) {
                let mut k = 2;
                while taken.contains(&format!("{}_{k}", n.to_ascii_lowercase())) {
                    k += 1;
                }
                let new = format!("{n}_{k}");
                taken.insert(new.to_ascii_lowercase());
                rename.push((n.to_string(), new));
            }
        }
    }
    if rename.is_empty() {
        return rename;
    }
    let fix = |v: &str| rename.iter().find(|(o, _)| o.eq_ignore_ascii_case(v)).map(|(_, n)| n.clone());
    for e in ents.iter_mut() {
        let class = e.classname().to_string();
        for (k, v) in &mut e.props {
            if is_name_key(fgd, &class, k) {
                if let Some(n) = fix(v) {
                    *v = n;
                }
            }
        }
        for (_, v) in &mut e.outputs {
            let sep = if v.contains('\u{1b}') { '\u{1b}' } else { ',' };
            let mut parts: Vec<String> = v.split(sep).map(|s| s.to_string()).collect();
            if let Some(n) = parts.first().and_then(|t| fix(t)) {
                parts[0] = n;
                *v = parts.join(&sep.to_string());
            }
        }
    }
    rename
}

/// Entities, von denen es nur eine (aktive) geben sollte
const SINGLETONS: [(&str, &str); 6] = [
    ("light_environment", "zwei Sonnen – Source benutzt nur eine"),
    ("sky_camera", "zwei 3D-Skybox-Kameras – nur eine ist erlaubt"),
    ("env_fog_controller", "mehrere Nebel-Controller – prüfen, welcher aktiv ist"),
    ("env_tonemap_controller", "mehrere Tonemap-Controller"),
    ("shadow_control", "mehrere shadow_control – nur einer wirkt"),
    ("water_lod_control", "mehrere water_lod_control"),
];

/// Ignorierte worldspawn-Schlüssel beim Vergleich
const WORLD_IGNORE: [&str; 4] = ["id", "classname", "mapversion", "spawnflags"];

/// Weg von `from_dir` zu `to` als relativer Pfad (für func_instance)
fn relative_path(to: &Path, from_dir: &Path) -> Option<PathBuf> {
    let to: Vec<_> = to.components().collect();
    let from: Vec<_> = from_dir.components().collect();
    let common = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut p = PathBuf::new();
    for _ in common..from.len() {
        p.push("..");
    }
    for c in &to[common..] {
        p.push(c.as_os_str());
    }
    Some(p)
}

impl Editor {
    /// Unterschiede der worldspawn-Eigenschaften (Schlüssel, A, B)
    pub fn world_diffs(&self, other: &Map) -> Vec<(String, String, String)> {
        let mut keys: Vec<String> = self.map.world.props.iter().chain(other.world.props.iter()).map(|(k, _)| k.to_ascii_lowercase()).collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .filter(|k| !WORLD_IGNORE.contains(&k.as_str()))
            .filter_map(|k| {
                let a = self.map.world.get(&k).unwrap_or("").to_string();
                let b = other.world.get(&k).unwrap_or("").to_string();
                (a != b && !b.is_empty()).then_some((k, a, b))
            })
            .collect()
    }

    /// Map B in die offene Map übernehmen (ein Rückgängig-Schritt).
    /// `name` dient für die Visgroup, `src_dir` (Ordner von B) zum Anpassen von Instanz-Pfaden.
    pub fn merge_map(&mut self, mut other: Map, name: &str, src_dir: Option<&Path>, o: &MergeOptions) -> Result<MergeReport, String> {
        let mut r = MergeReport::default();
        if other.world.solids.is_empty() && other.entities.is_empty() {
            return Err("Die andere Map ist leer".into());
        }
        r.world_diffs = self.world_diffs(&other);

        // Grenzen und Versatz
        let bounds_of = |ed: &Editor, m: &Map| {
            let mut b = m.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
            for e in &m.entities {
                if !e.is_brush_entity() {
                    b = b.union(&ed.entity_bounds(e));
                }
            }
            b
        };
        let a_bounds = bounds_of(self, &self.map);
        let b_bounds = bounds_of(self, &other);
        r.offset = match o.placement {
            Placement::Keep => DVec3::ZERO,
            Placement::Offset(v) => v,
            Placement::Beside => {
                if a_bounds.is_empty() {
                    DVec3::ZERO
                } else {
                    let snap = |v: f64| (v / 64.0).round() * 64.0;
                    DVec3::new(snap(a_bounds.max.x + 256.0 - b_bounds.min.x), snap(a_bounds.min.y - b_bounds.min.y), snap(a_bounds.min.z - b_bounds.min.z))
                }
            }
        };

        // Warnungen
        let moved = Aabb::new(b_bounds.min + r.offset, b_bounds.max + r.offset);
        if !a_bounds.is_empty() && a_bounds.intersects(&moved) {
            let n = self.map.all_solids().filter(|(_, s)| s.bounds.intersects(&moved)).count();
            r.warnings.push(format!("Die Bereiche überlappen sich ({n} Brushes der offenen Map liegen im Bereich der neuen) – evtl. „Daneben setzen“ wählen"));
        }
        for (class, why) in SINGLETONS {
            let in_a = self.map.entities.iter().any(|e| e.classname() == class);
            let in_b = other.entities.iter().any(|e| e.classname() == class);
            if in_a && in_b {
                r.warnings.push(format!("{class}: {why}"));
            }
        }
        let spawns = other.entities.iter().filter(|e| e.classname().starts_with("info_player_")).count();
        if spawns > 0 && self.map.entities.iter().any(|e| e.classname().starts_with("info_player_")) {
            r.warnings.push(format!("{spawns} Spawnpunkt(e) aus der anderen Map kommen hinzu"));
        }
        let half = self.map_limit;
        if moved.min.min_element() < -half || moved.max.max_element() > half {
            r.warnings.push(format!("Teile liegen außerhalb der Map-Grenzen (±{half})"));
        }

        // Instanz-Pfade relativ zur offenen Map umschreiben
        if let (Some(src), Some(dst)) = (src_dir, self.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()))) {
            if src != dst.as_path() {
                for e in other.entities.iter_mut().filter(|e| e.classname() == "func_instance") {
                    if let Some(f) = e.get("file").map(|s| s.to_string()) {
                        let abs = src.join(&f);
                        match relative_path(&abs, &dst) {
                            Some(rel) if abs.is_file() => e.set("file", rel.display().to_string().replace('\\', "/")),
                            _ => r.warnings.push(format!("Instanz-Datei {f} nicht gefunden – Pfad prüfen")),
                        }
                    }
                }
            }
        }

        // Namen
        let existing = self.map.targetnames();
        r.renamed = rename_conflicts(&self.fgd, &existing, &mut other.entities);

        self.checkpoint();

        // Gruppen und Visgroups neu nummerieren
        let mut group_map: HashMap<u32, u32> = HashMap::new();
        let mut vis_map: HashMap<u32, u32> = HashMap::new();
        let import_vis = if o.visgroup {
            let id = self.map.alloc_id();
            let vname = format!("Import: {name}");
            self.map.visgroups.push(Visgroup { id, name: vname, color: [255, 160, 40], parent: None, visible: true, locked: false });
            Some(id)
        } else {
            None
        };
        if o.keep_groups {
            for g in &other.groups {
                let id = self.map.alloc_id();
                group_map.insert(g.id, id);
                let mut ng = g.clone();
                ng.id = id;
                self.map.groups.push(ng);
            }
            // Eltern zuerst, damit parent umgeschrieben werden kann
            for v in &other.visgroups {
                vis_map.insert(v.id, self.map.alloc_id());
            }
            for v in &other.visgroups {
                let mut nv = v.clone();
                nv.id = vis_map[&v.id];
                nv.parent = v.parent.and_then(|p| vis_map.get(&p).copied()).or(import_vis);
                self.map.visgroups.push(nv);
            }
        }
        let fix_editor = |ed: &mut crate::map::EditorInfo| {
            ed.groupid = ed.groupid.and_then(|g| group_map.get(&g).copied());
            ed.visgroups = ed.visgroups.iter().filter_map(|v| vis_map.get(v).copied()).collect();
            if let Some(iv) = import_vis {
                if ed.visgroups.is_empty() {
                    ed.visgroups.push(iv);
                }
            }
            ed.hidden = false;
        };

        // Map-Eigenschaften übernehmen
        for k in &o.take_world_keys {
            if let Some(v) = other.world.get(k).map(|s| s.to_string()) {
                self.map.world.set(k, v);
            }
        }

        let x = Xform::translate(r.offset);
        let tl = self.tex_lock;
        for mut s in std::mem::take(&mut other.world.solids) {
            self.map.reassign_ids(&mut s);
            fix_editor(&mut s.editor);
            s.transform(&x, tl, true);
            r.objects.push(Obj::Solid(s.id));
            self.map.world.solids.push(s);
            r.solids += 1;
        }
        for mut e in other.entities {
            self.map.reassign_entity_ids(&mut e);
            fix_editor(&mut e.editor);
            for s in &mut e.solids {
                fix_editor(&mut s.editor);
            }
            e.transform(&x, tl, true);
            r.objects.push(Obj::Entity(e.id));
            r.solids += e.solids.len();
            self.map.entities.push(e);
            r.entities += 1;
        }
        self.sel = r.objects.iter().copied().collect();
        self.faces.clear();
        self.vsel.clear();
        self.status = r.summary();
        self.touch();
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;

    fn room(ed: &mut Editor, name: &str) {
        ed.create_block(Aabb::new(DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 192.0)));
        ed.hollow(16.0);
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 16.0));
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 160.0));
        let id = ed.map.entities.last().unwrap().id;
        ed.map.entity_mut(id).unwrap().set("targetname", name);
        ed.entity_class = "func_button".into();
        ed.create_block(Aabb::new(DVec3::new(200.0, -16.0, 48.0), DVec3::new(208.0, 16.0, 80.0)));
        ed.tie_to_entity("func_button");
        let bid = ed.map.entities.last().unwrap().id;
        ed.map.entity_mut(bid).unwrap().outputs.push(("OnPressed".into(), format!("{name},Toggle,,0,-1")));
    }

    #[test]
    fn merge_two_rooms() {
        let mut a = Editor::new(Fgd::builtin());
        room(&mut a, "lampe");
        a.map.world.set("skyname", "sky_day01_01");
        let mut b = Editor::new(Fgd::builtin());
        room(&mut b, "lampe");
        b.map.world.set("skyname", "sky_night_01");
        let gid = b.map.alloc_id();
        b.map.visgroups.push(Visgroup { id: gid, name: "Deko".into(), color: [1, 2, 3], parent: None, visible: true, locked: false });
        b.map.world.solids[0].editor.visgroups.push(gid);
        let (sa, ea) = (a.map.all_solids().count(), a.map.entities.len());
        let (sb, eb) = (b.map.all_solids().count(), b.map.entities.len());
        let o = MergeOptions { placement: Placement::Beside, ..Default::default() };
        let r = a.merge_map(b.map.clone(), "b", None, &o).unwrap();
        assert_eq!(a.map.all_solids().count(), sa + sb);
        assert_eq!(a.map.entities.len(), ea + eb);
        assert_eq!(r.renamed, vec![("lampe".to_string(), "lampe_2".to_string())]);
        // Output des neuen Knopfs zeigt auf die umbenannte Lampe, der alte nicht
        let outs: Vec<&str> = a.map.entities.iter().filter(|e| e.classname() == "func_button").map(|e| e.outputs[0].1.as_str()).collect();
        assert!(outs.iter().any(|o| o.starts_with("lampe,")) && outs.iter().any(|o| o.starts_with("lampe_2,")), "{outs:?}");
        assert_eq!(r.world_diffs.iter().find(|d| d.0 == "skyname").unwrap().2, "sky_night_01");
        assert_eq!(a.map.world.get("skyname"), Some("sky_day01_01"));
        assert!(r.offset.x >= 512.0 && r.warnings.iter().all(|w| !w.contains("überlappen")), "{r:?}");
        // IDs eindeutig
        let mut ids: Vec<u32> = a.map.all_solids().map(|(_, s)| s.id).chain(a.map.entities.iter().map(|e| e.id)).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
        // Visgroups: Import + Deko (unter Import)
        let imp = a.map.visgroups.iter().find(|v| v.name == "Import: b").unwrap().id;
        let deko = a.map.visgroups.iter().find(|v| v.name == "Deko").unwrap();
        assert_eq!(deko.parent, Some(imp));
        // ein Rückgängig-Schritt
        a.undo();
        assert_eq!(a.map.entities.len(), ea);
        // Speicherbar
        a.redo();
        Map::from_vmf(&a.map.to_vmf()).unwrap();
    }

    #[test]
    fn merge_in_place_warns_and_takes_keys() {
        let mut a = Editor::new(Fgd::builtin());
        room(&mut a, "x");
        let mut sun = Entity::new(a.map.alloc_id(), "light_environment");
        sun.set("origin", "0 0 100");
        a.map.entities.push(sun.clone().into());
        let mut b = Editor::new(Fgd::builtin());
        room(&mut b, "y");
        b.map.entities.push(sun.into());
        b.map.world.set("detailmaterial", "detail/detailsprites_custom");
        let o = MergeOptions { take_world_keys: vec!["detailmaterial".into()], visgroup: false, ..Default::default() };
        let r = a.merge_map(b.map.clone(), "b", None, &o).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("überlappen")));
        assert!(r.warnings.iter().any(|w| w.contains("light_environment")));
        assert!(r.renamed.is_empty());
        assert_eq!(a.map.world.get("detailmaterial"), Some("detail/detailsprites_custom"));
        assert!(a.map.visgroups.is_empty());
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative_path(Path::new("/a/b/inst/x.vmf"), Path::new("/a/c")).unwrap(), PathBuf::from("../b/inst/x.vmf"));
    }
}
