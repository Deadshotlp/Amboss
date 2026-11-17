//! Map-Stände vergleichen und 3-Wege-Zusammenführen (z. B. für Git).
//! Objekte werden über ihre VMF-ID zugeordnet, der Inhalt ohne IDs verglichen.

use crate::editor::Editor;
use crate::geometry::Aabb;
use crate::map::{Entity, Map, Solid, Visgroup};
use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Solid,
    Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Removed,
    Modified,
}

impl Change {
    pub fn label(self) -> &'static str {
        match self {
            Change::Added => "neu",
            Change::Removed => "gelöscht",
            Change::Modified => "geändert",
        }
    }
    pub fn color(self) -> [u8; 3] {
        match self {
            Change::Added => [80, 230, 110],
            Change::Removed => [255, 70, 70],
            Change::Modified => [255, 200, 50],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjDiff {
    pub kind: Kind,
    pub id: u32,
    pub change: Change,
    pub label: String,
    pub bounds: Aabb,
}

#[derive(Debug, Clone, Default)]
pub struct MapDiff {
    pub objects: Vec<ObjDiff>,
    /// worldspawn: (Schlüssel, alt, neu)
    pub world: Vec<(String, Option<String>, Option<String>)>,
}

impl MapDiff {
    pub fn count(&self, c: Change) -> usize {
        self.objects.iter().filter(|o| o.change == c).count()
    }
    pub fn summary(&self) -> String {
        format!(
            "{} neu, {} gelöscht, {} geändert, {} Map-Eigenschaft(en) anders",
            self.count(Change::Added),
            self.count(Change::Removed),
            self.count(Change::Modified),
            self.world.len()
        )
    }
}

fn solid_label(s: &Solid) -> String {
    format!("Brush {} ({})", s.id, s.sides.first().map(|x| x.material.as_str()).unwrap_or(""))
}

fn entity_label(e: &Entity) -> String {
    match e.targetname() {
        Some(n) => format!("{} „{n}“ ({})", e.classname(), e.id),
        None => format!("{} ({})", e.classname(), e.id),
    }
}

struct Index<'a> {
    solids: HashMap<u32, (&'a Solid, String)>,
    entities: HashMap<u32, (&'a Entity, String)>,
}

fn index(m: &Map) -> Index<'_> {
    Index {
        solids: m.world.solids.iter().map(|s| (s.id, (&**s, m.solid_fingerprint(s)))).collect(),
        entities: m.entities.iter().map(|e| (e.id, (&**e, m.entity_fingerprint(e)))).collect(),
    }
}

fn world_props(m: &Map) -> HashMap<String, String> {
    m.world.props.iter().filter(|(k, _)| !matches!(k.to_ascii_lowercase().as_str(), "id" | "mapversion" | "classname")).map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect()
}

/// Unterschiede von `old` nach `new`
pub fn diff(old: &Map, new: &Map, ed: &Editor) -> MapDiff {
    let (a, b) = (index(old), index(new));
    let mut out = MapDiff::default();
    let ids: BTreeSet<u32> = a.solids.keys().chain(b.solids.keys()).copied().collect();
    for id in ids {
        let (x, y) = (a.solids.get(&id), b.solids.get(&id));
        let change = match (x, y) {
            (None, Some(_)) => Change::Added,
            (Some(_), None) => Change::Removed,
            (Some(p), Some(q)) if p.1 != q.1 => Change::Modified,
            _ => continue,
        };
        let s = y.or(x).unwrap().0;
        out.objects.push(ObjDiff { kind: Kind::Solid, id, change, label: solid_label(s), bounds: s.bounds });
    }
    let ids: BTreeSet<u32> = a.entities.keys().chain(b.entities.keys()).copied().collect();
    for id in ids {
        let (x, y) = (a.entities.get(&id), b.entities.get(&id));
        let change = match (x, y) {
            (None, Some(_)) => Change::Added,
            (Some(_), None) => Change::Removed,
            (Some(p), Some(q)) if p.1 != q.1 => Change::Modified,
            _ => continue,
        };
        let e = y.or(x).unwrap().0;
        let mut bounds = ed.entity_bounds(e);
        if let (Some(p), Some(_)) = (x, y) {
            bounds = bounds.union(&ed.entity_bounds(p.0));
        }
        out.objects.push(ObjDiff { kind: Kind::Entity, id, change, label: entity_label(e), bounds });
    }
    let (wa, wb) = (world_props(old), world_props(new));
    let mut keys: Vec<&String> = wa.keys().chain(wb.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let (x, y) = (wa.get(k), wb.get(k));
        if x != y {
            out.world.push((k.clone(), x.cloned(), y.cloned()));
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub kind: Kind,
    pub id: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Merge3 {
    pub conflicts: Vec<Conflict>,
    /// Änderungen der anderen Seite, die übernommen wurden
    pub taken: usize,
    /// Neue Objekte der anderen Seite, die wegen ID-Kollision neu nummeriert wurden
    pub renumbered: usize,
}

impl Merge3 {
    pub fn summary(&self) -> String {
        format!("{} Änderung(en) der anderen Seite übernommen, {} Konflikt(e), {} Objekt(e) neu nummeriert", self.taken, self.conflicts.len(), self.renumbered)
    }
}

fn all_ids(m: &Map) -> (HashSet<u32>, HashSet<u32>) {
    let mut objs = HashSet::new();
    let mut sides = HashSet::new();
    for (_, s) in m.all_solids() {
        objs.insert(s.id);
        sides.extend(s.sides.iter().map(|x| x.id));
    }
    objs.extend(m.entities.iter().map(|e| e.id));
    (objs, sides)
}

/// 3-Wege-Zusammenführen: `ours` wird zum Ergebnis.
/// Nicht auflösbare Konflikte behalten unsere Version; die andere Version wird
/// zusätzlich (ausgeblendet) in die Visgroup „Konflikt: andere Version“ gelegt.
pub fn merge3(base: &Map, ours: &mut Map, theirs: &Map) -> Merge3 {
    let mut r = Merge3::default();
    let (ib, it) = (index(base), index(theirs));
    let io_solids: HashMap<u32, String> = ours.world.solids.iter().map(|s| (s.id, ours.solid_fingerprint(s))).collect();
    let io_ents: HashMap<u32, String> = ours.entities.iter().map(|e| (e.id, ours.entity_fingerprint(e))).collect();
    ours.next_id = ours.next_id.max(theirs.next_id);
    let (used, used_sides) = all_ids(ours);

    enum Act {
        Put(Kind, u32),
        Remove(Kind, u32),
        AddCopy(Kind, u32, bool),
    }
    let mut acts = Vec::new();
    for kind in [Kind::Solid, Kind::Entity] {
        let (b, o, t): (HashMap<u32, &String>, HashMap<u32, &String>, HashMap<u32, &String>) = match kind {
            Kind::Solid => (ib.solids.iter().map(|(k, v)| (*k, &v.1)).collect(), io_solids.iter().map(|(k, v)| (*k, v)).collect(), it.solids.iter().map(|(k, v)| (*k, &v.1)).collect()),
            Kind::Entity => (ib.entities.iter().map(|(k, v)| (*k, &v.1)).collect(), io_ents.iter().map(|(k, v)| (*k, v)).collect(), it.entities.iter().map(|(k, v)| (*k, &v.1)).collect()),
        };
        let ids: BTreeSet<u32> = b.keys().chain(o.keys()).chain(t.keys()).copied().collect();
        let what = |id: u32| match kind {
            Kind::Solid => format!("Brush {id}"),
            Kind::Entity => it.entities.get(&id).map(|e| entity_label(e.0)).or_else(|| ib.entities.get(&id).map(|e| entity_label(e.0))).unwrap_or_else(|| format!("Entity {id}")),
        };
        for id in ids {
            let (bv, ov, tv) = (b.get(&id), o.get(&id), t.get(&id));
            if ov == tv || tv == bv {
                continue; // gleich oder nur bei uns geändert
            }
            if ov == bv {
                // nur bei der anderen Seite geändert
                acts.push(if tv.is_some() { Act::Put(kind, id) } else { Act::Remove(kind, id) });
                r.taken += 1;
                continue;
            }
            match (bv, ov, tv) {
                (None, Some(_), Some(_)) => {
                    // beide haben unabhängig ein Objekt mit derselben ID angelegt -> beide behalten
                    acts.push(Act::AddCopy(kind, id, false));
                    r.taken += 1;
                }
                (Some(_), None, Some(_)) => {
                    acts.push(Act::Put(kind, id));
                    r.conflicts.push(Conflict { kind, id, text: format!("{}: bei uns gelöscht, bei der anderen Seite geändert – geänderte Version wiederhergestellt", what(id)) });
                }
                (Some(_), Some(_), None) => {
                    r.conflicts.push(Conflict { kind, id, text: format!("{}: bei uns geändert, bei der anderen Seite gelöscht – unsere Version behalten", what(id)) });
                }
                _ => {
                    acts.push(Act::AddCopy(kind, id, true));
                    r.conflicts.push(Conflict { kind, id, text: format!("{}: auf beiden Seiten verschieden geändert – unsere Version behalten, die andere liegt ausgeblendet in „Konflikt: andere Version“", what(id)) });
                }
            }
        }
    }

    let mut conflict_vis: Option<u32> = None;
    for act in acts {
        match act {
            Act::Put(Kind::Solid, id) => {
                let s = it.solids[&id].0.clone();
                match ours.world.solids.iter_mut().find(|x| x.id == id) {
                    Some(x) => *x = s.into(),
                    None => ours.world.solids.push(s.into()),
                }
            }
            Act::Put(Kind::Entity, id) => {
                let e = it.entities[&id].0.clone();
                match ours.entities.iter_mut().find(|x| x.id == id) {
                    Some(x) => *x = e.into(),
                    None => ours.entities.push(e.into()),
                }
            }
            Act::Remove(Kind::Solid, id) => ours.world.solids.retain(|x| x.id != id),
            Act::Remove(Kind::Entity, id) => ours.entities.retain(|x| x.id != id),
            Act::AddCopy(kind, id, hidden) => {
                let vis = if hidden {
                    Some(*conflict_vis.get_or_insert_with(|| {
                        let v = ours.alloc_id();
                        ours.visgroups.push(Visgroup { id: v, name: "Konflikt: andere Version".into(), color: [255, 60, 200], parent: None, visible: false, locked: false });
                        v
                    }))
                } else {
                    None
                };
                match kind {
                    Kind::Solid => {
                        let mut s = it.solids[&id].0.clone();
                        ours.reassign_ids(&mut s);
                        if let Some(v) = vis {
                            s.editor.visgroups = vec![v];
                        }
                        ours.world.solids.push(s.into());
                    }
                    Kind::Entity => {
                        let mut e = it.entities[&id].0.clone();
                        ours.reassign_entity_ids(&mut e);
                        if let Some(v) = vis {
                            e.editor.visgroups = vec![v];
                        }
                        ours.entities.push(e.into());
                    }
                }
                r.renumbered += 1;
            }
        }
    }
    // Übernommene neue Objekte, deren IDs mit unseren kollidieren, neu nummerieren
    let theirs_new: Vec<(Kind, u32)> = it.solids.keys().filter(|id| !ib.solids.contains_key(id) && !io_solids.contains_key(id)).map(|id| (Kind::Solid, *id)).chain(it.entities.keys().filter(|id| !ib.entities.contains_key(id) && !io_ents.contains_key(id)).map(|id| (Kind::Entity, *id))).collect();
    for (kind, id) in theirs_new {
        let taken = |x: u32| used.contains(&x) || used_sides.contains(&x);
        let clash = match kind {
            Kind::Solid => taken(id) || ours.world.solids.iter().find(|s| s.id == id).is_some_and(|s| s.sides.iter().any(|x| taken(x.id))),
            Kind::Entity => taken(id) || ours.entities.iter().find(|e| e.id == id).is_some_and(|e| e.solids.iter().any(|s| taken(s.id) || s.sides.iter().any(|x| taken(x.id)))),
        };
        if clash {
            let mut m = std::mem::take(ours);
            match kind {
                Kind::Solid => {
                    if let Some(pos) = m.world.solids.iter().position(|s| s.id == id) {
                        let mut s = m.world.solids.remove(pos);
                        m.reassign_ids(&mut s);
                        m.world.solids.push(s);
                    }
                }
                Kind::Entity => {
                    if let Some(pos) = m.entities.iter().position(|e| e.id == id) {
                        let mut e = m.entities.remove(pos);
                        m.reassign_entity_ids(&mut e);
                        m.entities.push(e);
                    }
                }
            }
            *ours = m;
            r.renumbered += 1;
        }
    }

    // worldspawn
    let (wb, wo, wt) = (world_props(base), world_props(ours), world_props(theirs));
    let mut keys: Vec<String> = wb.keys().chain(wo.keys()).chain(wt.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let (bv, ov, tv) = (wb.get(&k), wo.get(&k), wt.get(&k));
        if ov == tv || tv == bv {
            continue;
        }
        if ov == bv {
            match tv {
                Some(v) => ours.world.set(&k, v.clone()),
                None => ours.world.props.retain(|(x, _)| !x.eq_ignore_ascii_case(&k)),
            }
            r.taken += 1;
        } else {
            r.conflicts.push(Conflict { kind: Kind::Entity, id: ours.world.id, text: format!("Map-Eigenschaft {k}: beide geändert ({} / {}) – unser Wert bleibt", ov.map(|s| s.as_str()).unwrap_or("–"), tv.map(|s| s.as_str()).unwrap_or("–")) });
        }
    }
    // Visgroups und Gruppen der anderen Seite, die bei uns fehlen
    let have_vis: HashSet<u32> = ours.visgroups.iter().map(|v| v.id).collect();
    let base_vis: HashSet<u32> = base.visgroups.iter().map(|v| v.id).collect();
    for v in &theirs.visgroups {
        if !have_vis.contains(&v.id) && !base_vis.contains(&v.id) && !used.contains(&v.id) {
            ours.visgroups.push(v.clone());
        }
    }
    let have_groups: HashSet<u32> = ours.groups.iter().map(|g| g.id).collect();
    let base_groups: HashSet<u32> = base.groups.iter().map(|g| g.id).collect();
    for g in &theirs.groups {
        if !have_groups.contains(&g.id) && !base_groups.contains(&g.id) {
            ours.groups.push(g.clone());
        }
    }
    let max = all_ids(ours).0.into_iter().chain(all_ids(ours).1).max().unwrap_or(0);
    ours.next_id = ours.next_id.max(max + 1);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;
    use glam::DVec3;

    fn base() -> Editor {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 192.0)));
        ed.hollow(16.0);
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 160.0));
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 16.0));
        ed
    }

    fn reload(m: &Map) -> Map {
        Map::from_vmf(&m.to_vmf()).unwrap()
    }

    #[test]
    fn diff_finds_changes() {
        let ed = base();
        let old = reload(&ed.map);
        let mut new = reload(&ed.map);
        let light = new.entities.iter().find(|e| e.classname() == "light").unwrap().id;
        new.entity_mut(light).unwrap().set("_light", "255 0 0 200");
        let wall = new.world.solids[0].id;
        new.take_solid(wall);
        let b = new.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(32.0)), "DEV/DEV_MEASUREGENERIC01B", 0.25);
        new.world.solids.push(b.into());
        new.world.set("skyname", "sky_night_01");
        let d = diff(&old, &new, &ed);
        assert_eq!((d.count(Change::Added), d.count(Change::Removed), d.count(Change::Modified)), (1, 1, 1), "{d:?}");
        assert_eq!(d.world.len(), 1);
        assert!(diff(&old, &reload(&old), &ed).objects.is_empty());
    }

    #[test]
    fn three_way_merge() {
        let ed = base();
        let b = reload(&ed.map);
        let light = b.entities.iter().find(|e| e.classname() == "light").unwrap().id;
        let spawn = b.entities.iter().find(|e| e.classname() == "info_player_start").unwrap().id;
        let wall = b.world.solids[0].id;
        let wall2 = b.world.solids[1].id;
        // wir: Licht geändert, neuen Block, Wand 2 geändert
        let mut ours = reload(&b);
        ours.entity_mut(light).unwrap().set("_light", "255 0 0 200");
        let s = ours.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(32.0)), "OURS", 0.25);
        ours.world.solids.push(s.into());
        ours.solid_mut(wall2).unwrap().sides[0].material = "OURS".into();
        // die anderen: Spawn verschoben, Wand gelöscht, neuen Block (gleiche ID!), Wand 2 anders geändert, Himmel
        let mut theirs = reload(&b);
        theirs.entity_mut(spawn).unwrap().set("origin", "64 0 16");
        theirs.take_solid(wall);
        let s = theirs.make_box(Aabb::new(DVec3::splat(64.0), DVec3::splat(96.0)), "THEIRS", 0.25);
        theirs.world.solids.push(s.into());
        theirs.solid_mut(wall2).unwrap().sides[0].material = "THEIRS".into();
        theirs.world.set("skyname", "sky_night_01");
        let mut m = reload(&ours);
        let r = merge3(&b, &mut m, &theirs);
        assert_eq!(m.entity(light).unwrap().get("_light"), Some("255 0 0 200"));
        assert_eq!(m.entity(spawn).unwrap().get("origin"), Some("64 0 16"));
        assert!(m.solid(wall).is_none());
        assert!(m.world.solids.iter().any(|s| s.sides[0].material == "OURS" && s.bounds.size().x == 32.0));
        assert!(m.world.solids.iter().any(|s| s.sides[0].material == "THEIRS" && s.bounds.size().x == 32.0));
        assert_eq!(m.solid(wall2).unwrap().sides[0].material, "OURS");
        assert_eq!(r.conflicts.len(), 1, "{r:?}");
        assert!(m.visgroups.iter().any(|v| v.name.starts_with("Konflikt")));
        assert_eq!(m.world.get("skyname"), Some("sky_night_01"));
        // IDs eindeutig
        let (objs, sides) = all_ids(&m);
        let n_objs = m.all_solids().count() + m.entities.len();
        assert_eq!(objs.len(), n_objs);
        let n_sides: usize = m.all_solids().map(|(_, s)| s.sides.len()).sum();
        assert_eq!(sides.len(), n_sides);
        reload(&m);
    }
}
