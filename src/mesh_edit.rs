//! Bearbeiten von Polygon-Meshes und Bezier-Patches in der 3D-Ansicht. Beide zeigen Griffe
//! (Punkte, Flächenmitten bzw. 16 Kontrollpunkte), die angeklickt und gezogen werden.

use crate::editor::{Editor, Obj};
use crate::map::{Entity, Shared, Solid};
use crate::mesh::{Mesh, MeshObj};
use glam::DVec3;
use std::collections::BTreeSet;

/// Ein Mesh wird bearbeitet
#[derive(Clone, Debug)]
pub struct MeshEdit {
    pub ent: u32,
    /// true = Flächen auswählen, false = Punkte
    pub faces_mode: bool,
    pub sel: BTreeSet<usize>,
}

/// Ein Displacement wird als Bezier-Patch geformt
#[derive(Clone, Debug)]
pub struct PatchEdit {
    pub solid: u32,
    pub side: usize,
    /// Kontrollnetz in Weltkoordinaten (`row * 4 + col`)
    pub ctrl: [DVec3; 16],
    pub sel: BTreeSet<usize>,
    /// Stand des Brushes nach unserer letzten Änderung (sonst Netz neu bestimmen: Rückgängig o. Ä.)
    applied: Option<Shared<Solid>>,
}

/// Ausgangszustand beim Ziehen
#[derive(Clone)]
pub enum GripBase {
    Mesh(Mesh),
    Patch([DVec3; 16]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshOp {
    Extrude(f64),
    Smooth,
    Subdivide,
    Delete,
    Flip,
    SelectAll,
}

fn shared_solid(map: &crate::map::Map, id: u32) -> Option<Shared<Solid>> {
    map.world.solids.iter().chain(map.entities.iter().flat_map(|e| e.solids.iter())).find(|s| s.id == id).cloned()
}

impl Editor {
    pub fn mesh_index(&self, ent: u32) -> Option<usize> {
        self.map.meshes.iter().position(|m| m.ent == ent && self.map.entity(ent).is_some())
    }

    pub fn mesh_of(&self, ent: u32) -> Option<&MeshObj> {
        self.mesh_index(ent).map(|i| &*self.map.meshes[i])
    }

    /// Einzeln ausgewähltes Mesh-Prop
    pub fn selected_mesh(&self) -> Option<u32> {
        match self.sel.iter().collect::<Vec<_>>()[..] {
            [Obj::Entity(id)] if self.mesh_index(*id).is_some() => Some(*id),
            _ => None,
        }
    }

    /// Griffe aktiv (Mesh- oder Patch-Bearbeitung)
    pub fn grip_mode(&self) -> bool {
        self.mesh_edit.is_some() || self.patch_edit.is_some()
    }

    pub fn mesh_begin(&mut self, ent: u32) {
        if self.mesh_index(ent).is_some() {
            self.patch_edit = None;
            self.mesh_edit = Some(MeshEdit { ent, faces_mode: false, sel: BTreeSet::new() });
            self.sel = [Obj::Entity(ent)].into_iter().collect();
        }
    }

    pub fn grip_end(&mut self) {
        self.mesh_edit = None;
        self.patch_edit = None;
    }

    /// Eindeutiger Modellname: <mapname>_mesh<id>
    fn mesh_name(&self, id: u32) -> String {
        let stem = self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "map".into());
        let clean: String = stem.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
        format!("{clean}_mesh{id}")
    }

    /// Neues prop_static mit Netz anlegen (ohne eigenen Rückgängig-Schritt)
    fn mesh_new_entity(&mut self, mesh: Mesh, origin: DVec3) -> u32 {
        let id = self.map.alloc_id();
        let name = self.mesh_name(id);
        let mut e = Entity::new(id, "prop_static");
        e.set("model", format!("models/amboss/meshes/{name}.mdl"));
        e.set("origin", crate::map::fmt_vec(origin));
        e.set("angles", "0 0 0");
        e.set("solid", "6");
        self.map.entities.push(e.into());
        self.map.meshes.push(Shared::new(MeshObj { ent: id, mesh, name }));
        id
    }

    /// Ausgewählte Brushes in ein frei bearbeitbares Mesh umwandeln (Displacements als Raster)
    pub fn mesh_from_selection(&mut self) -> Result<u32, String> {
        let ids: Vec<u32> = self.sel.iter().filter_map(|o| if let Obj::Solid(id) = o { Some(*id) } else { None }).collect();
        let solids: Vec<&Solid> = ids.iter().filter_map(|id| self.map.solid(*id)).collect();
        if solids.is_empty() {
            return Err(crate::i18n::tr("Erst Brushes auswählen").into());
        }
        let mut polys: Vec<(Vec<DVec3>, String)> = Vec::new();
        for s in &solids {
            for (i, poly) in s.polys.iter().enumerate() {
                let mat = s.sides[i].material.clone();
                if let Some(Some(g)) = s.disps.get(i) {
                    let r = g.res;
                    for y in 0..r - 1 {
                        for x in 0..r - 1 {
                            let p = |x: usize, y: usize| g.pos[y * r + x];
                            let mut q = vec![p(x, y), p(x + 1, y), p(x + 1, y + 1), p(x, y + 1)];
                            // Ausrichtung wie die Fläche (Normale nach außen)
                            let n = (q[1] - q[0]).cross(q[2] - q[0]);
                            if s.planes[i].is_some_and(|pl| n.dot(pl.normal) < 0.0) {
                                q.reverse();
                            }
                            polys.push((q, mat.clone()));
                        }
                    }
                } else if s.sides.len() > 1 && s.sides.iter().any(|sd| sd.disp.is_some()) {
                    // Brush mit Displacement: nur die Displacement-Fläche zählt (wie im Spiel)
                    continue;
                } else if poly.len() >= 3 {
                    polys.push((poly.clone(), mat));
                }
            }
        }
        let b = solids.iter().fold(crate::geometry::Aabb::EMPTY, |b, s| b.union(&s.bounds));
        let origin = DVec3::new(b.center().x, b.center().y, b.min.z);
        let mesh = Mesh::from_polys(&polys, origin);
        self.checkpoint();
        for id in &ids {
            self.map.take_solid(*id);
        }
        let ent = self.mesh_new_entity(mesh, origin);
        self.mesh_begin(ent);
        self.status = crate::i18n::tr_dyn(&format!("Mesh aus {} Brush(es) erstellt", ids.len())).to_string();
        self.touch();
        Ok(ent)
    }

    /// OBJ-Datei als Mesh einfügen
    pub fn mesh_import(&mut self, src: &str, scale: f64, y_up: bool, at: DVec3) -> Result<u32, String> {
        let mut mesh = Mesh::from_obj(src, scale, y_up)?;
        // Unterkante auf den Einfügepunkt, horizontal zentriert
        let b = mesh.bounds();
        let shift = DVec3::new(b.center().x, b.center().y, b.min.z);
        for v in &mut mesh.verts {
            *v = (DVec3::from_array(*v) - shift).to_array();
        }
        mesh.weld(0.001);
        let mat = self.material.clone();
        for f in &mut mesh.faces {
            if f.material == "DEV/DEV_MEASUREGENERIC01" || f.material.is_empty() {
                f.material = mat.clone();
            }
        }
        self.checkpoint();
        let ent = self.mesh_new_entity(mesh, at);
        self.mesh_begin(ent);
        self.touch();
        Ok(ent)
    }

    /// Griffe in Weltkoordinaten (Index, Position)
    pub fn grips(&self) -> Vec<(usize, DVec3)> {
        if let Some(pe) = &self.patch_edit {
            return pe.ctrl.iter().copied().enumerate().collect();
        }
        let Some(me) = &self.mesh_edit else { return Vec::new() };
        let (Some(m), Some(e)) = (self.mesh_of(me.ent), self.map.entity(me.ent)) else { return Vec::new() };
        let x = Self::entity_xform(e);
        if me.faces_mode {
            (0..m.mesh.faces.len()).map(|f| (f, x.apply(m.mesh.center(f)))).collect()
        } else {
            (0..m.mesh.verts.len()).map(|i| (i, x.apply(m.mesh.vert(i)))).collect()
        }
    }

    /// Linien zur Anzeige (Kanten des Meshes bzw. Kontrollnetz)
    pub fn grip_lines(&self) -> Vec<(DVec3, DVec3)> {
        let mut out = Vec::new();
        if let Some(pe) = &self.patch_edit {
            for r in 0..4 {
                for c in 0..3 {
                    out.push((pe.ctrl[r * 4 + c], pe.ctrl[r * 4 + c + 1]));
                    out.push((pe.ctrl[c * 4 + r], pe.ctrl[(c + 1) * 4 + r]));
                }
            }
            return out;
        }
        let Some(me) = &self.mesh_edit else { return out };
        self.mesh_edges(me.ent)
    }

    /// Kanten eines Meshes in Weltkoordinaten (jede Kante einmal)
    pub fn mesh_edges(&self, ent: u32) -> Vec<(DVec3, DVec3)> {
        let mut out = Vec::new();
        let (Some(m), Some(e)) = (self.mesh_of(ent), self.map.entity(ent)) else { return out };
        let x = Self::entity_xform(e);
        let mut seen = std::collections::HashSet::new();
        for f in &m.mesh.faces {
            for k in 0..f.idx.len() {
                let (a, b) = (f.idx[k], f.idx[(k + 1) % f.idx.len()]);
                if seen.insert((a.min(b), a.max(b))) {
                    out.push((x.apply(m.mesh.vert(a)), x.apply(m.mesh.vert(b))));
                }
            }
        }
        out
    }

    pub fn grip_sel(&self) -> BTreeSet<usize> {
        self.patch_edit.as_ref().map(|p| p.sel.clone()).or_else(|| self.mesh_edit.as_ref().map(|m| m.sel.clone())).unwrap_or_default()
    }

    /// Griff anklicken: auswählen (mit `add` umschalten); None leert die Auswahl
    pub fn grip_select(&mut self, i: Option<usize>, add: bool) {
        let sel = match (&mut self.patch_edit, &mut self.mesh_edit) {
            (Some(p), _) => &mut p.sel,
            (None, Some(m)) => &mut m.sel,
            _ => return,
        };
        match i {
            Some(i) if add => {
                if !sel.remove(&i) {
                    sel.insert(i);
                }
            }
            Some(i) => {
                if !sel.contains(&i) {
                    *sel = [i].into_iter().collect();
                }
            }
            None if !add => sel.clear(),
            None => {}
        }
    }

    pub fn grip_base(&self) -> Option<GripBase> {
        if let Some(p) = &self.patch_edit {
            return Some(GripBase::Patch(p.ctrl));
        }
        let me = self.mesh_edit.as_ref()?;
        Some(GripBase::Mesh(self.mesh_of(me.ent)?.mesh.clone()))
    }

    /// Ausgewählte Griffe um `d` (Welt) gegenüber dem Ausgangszustand verschieben
    pub fn grip_drag(&mut self, base: &GripBase, d: DVec3) {
        match base {
            GripBase::Patch(c) => {
                let Some(pe) = &mut self.patch_edit else { return };
                let mut ctrl = *c;
                for &i in &pe.sel {
                    if i < 16 {
                        ctrl[i] += d;
                    }
                }
                pe.ctrl = ctrl;
                self.patch_apply();
            }
            GripBase::Mesh(m) => {
                let Some(me) = self.mesh_edit.clone() else { return };
                let Some(i) = self.mesh_index(me.ent) else { return };
                let Some(e) = self.map.entity(me.ent) else { return };
                let local = Self::entity_xform(e).m.transpose() * d;
                let verts: BTreeSet<usize> = if me.faces_mode {
                    me.sel.iter().filter_map(|&f| m.faces.get(f)).flat_map(|f| f.idx.iter().copied()).collect()
                } else {
                    me.sel.iter().copied().filter(|&v| v < m.verts.len()).collect()
                };
                let mut mesh = m.clone();
                mesh.move_verts(&verts, local);
                self.map.meshes[i].mesh = mesh;
                self.touch();
            }
        }
    }

    /// Werkzeuge im Mesh-Modus (je ein Rückgängig-Schritt)
    pub fn mesh_op(&mut self, op: MeshOp) {
        let Some(me) = self.mesh_edit.clone() else { return };
        let Some(i) = self.mesh_index(me.ent) else { return };
        if op == MeshOp::SelectAll {
            let n = if me.faces_mode { self.map.meshes[i].mesh.faces.len() } else { self.map.meshes[i].mesh.verts.len() };
            if let Some(m) = &mut self.mesh_edit {
                m.sel = (0..n).collect();
            }
            return;
        }
        let faces: BTreeSet<usize> = if me.faces_mode {
            me.sel.clone()
        } else {
            // Punkte gewählt: Flächen, deren Punkte alle gewählt sind
            self.map.meshes[i].mesh.faces.iter().enumerate().filter(|(_, f)| !me.sel.is_empty() && f.idx.iter().all(|v| me.sel.contains(v))).map(|(k, _)| k).collect()
        };
        let needs_faces = matches!(op, MeshOp::Extrude(_) | MeshOp::Delete);
        if needs_faces && faces.is_empty() {
            self.status = crate::i18n::tr("Erst Flächen auswählen").into();
            return;
        }
        self.checkpoint();
        let m = &mut self.map.meshes[i].mesh;
        let mut new_sel = me.sel.clone();
        match op {
            MeshOp::Extrude(dist) => {
                let made = m.extrude(&faces, dist);
                new_sel = if me.faces_mode { made } else { made.iter().flat_map(|&f| m.faces[f].idx.clone()).collect() };
            }
            MeshOp::Smooth => {
                let sel: BTreeSet<usize> = if me.sel.is_empty() {
                    (0..m.verts.len()).collect()
                } else if me.faces_mode {
                    me.sel.iter().filter_map(|&f| m.faces.get(f)).flat_map(|f| f.idx.iter().copied()).collect()
                } else {
                    me.sel.clone()
                };
                m.smooth(&sel, 2, 0.5);
            }
            MeshOp::Subdivide => {
                m.subdivide();
                new_sel.clear();
            }
            MeshOp::Delete => {
                m.delete_faces(&faces);
                new_sel.clear();
            }
            MeshOp::Flip => m.flip(&faces),
            MeshOp::SelectAll => {}
        }
        if let Some(x) = &mut self.mesh_edit {
            x.sel = new_sel;
        }
        self.touch();
    }

    /// Aktuelle Textur auf die gewählten Flächen (bzw. das ganze Mesh)
    pub fn mesh_paint(&mut self) {
        let Some(me) = self.mesh_edit.clone() else { return };
        let Some(i) = self.mesh_index(me.ent) else { return };
        self.checkpoint();
        let mat = self.material.clone();
        let all = !me.faces_mode || me.sel.is_empty();
        for (k, f) in self.map.meshes[i].mesh.faces.iter_mut().enumerate() {
            if all || me.sel.contains(&k) {
                f.material = mat.clone();
            }
        }
        self.touch();
    }

    /// Bezier-Patch auf der gewählten Fläche beginnen (legt bei Bedarf ein Displacement an)
    pub fn patch_begin(&mut self) -> Result<(), String> {
        let target = self.faces.iter().next().copied().or_else(|| {
            // ein Brush gewählt: vorhandenes Displacement, sonst die Oberseite
            let id = self.sel.iter().find_map(|o| if let Obj::Solid(id) = o { Some(*id) } else { None })?;
            let s = self.map.solid(id)?;
            let side = (0..s.sides.len()).find(|&i| s.sides[i].disp.is_some()).or_else(|| (0..s.sides.len()).filter(|&i| s.planes[i].is_some()).max_by(|&a, &b| s.planes[a].unwrap().normal.z.total_cmp(&s.planes[b].unwrap().normal.z)))?;
            Some((id, side))
        });
        let Some((sid, side)) = target else { return Err(crate::i18n::tr("Erst eine Fläche oder einen Brush auswählen").into()) };
        let s = self.map.solid(sid).ok_or("?")?;
        if s.sides[side].disp.is_none() {
            let poly = s.polys.get(side).cloned().unwrap_or_default();
            let n = s.planes[side].map(|p| p.normal).unwrap_or(DVec3::Z);
            let Some(d) = crate::disp::Disp::new_flat(3, &poly, n) else { return Err(crate::i18n::tr("Die Fläche braucht genau vier Ecken").into()) };
            self.checkpoint();
            let s = self.map.solid_mut(sid).ok_or("?")?;
            s.sides[side].disp = Some(d);
            s.update_geometry();
            self.touch();
        }
        let g = self.map.solid(sid).and_then(|s| s.disps.get(side).cloned().flatten()).ok_or("?")?;
        self.mesh_edit = None;
        self.patch_edit = Some(PatchEdit { solid: sid, side, ctrl: crate::patch::from_disp(&g), sel: BTreeSet::new(), applied: shared_solid(&self.map, sid) });
        self.status = crate::i18n::tr("Bezier-Patch: Kontrollpunkte in der 3D-Ansicht ziehen").into();
        Ok(())
    }

    /// Netz neu bestimmen, wenn der Brush von außen geändert wurde (Rückgängig, Pinsel …)
    pub fn patch_sync(&mut self) {
        let Some(pe) = &self.patch_edit else { return };
        let cur = shared_solid(&self.map, pe.solid);
        let Some(cur) = cur else {
            self.patch_edit = None;
            return;
        };
        if pe.applied.as_ref().is_some_and(|a| Shared::ptr_eq(a, &cur)) {
            return;
        }
        let side = pe.side;
        match cur.disps.get(side).cloned().flatten() {
            Some(g) => {
                let pe = self.patch_edit.as_mut().unwrap();
                pe.ctrl = crate::patch::from_disp(&g);
                pe.applied = Some(cur);
            }
            None => self.patch_edit = None,
        }
    }

    fn patch_apply(&mut self) {
        let Some(pe) = self.patch_edit.clone() else { return };
        let Some(s) = self.map.solid_mut(pe.solid) else { return };
        let (Some(Some(g)), Some(mut d)) = (s.disps.get(pe.side).cloned(), s.sides.get(pe.side).and_then(|x| x.disp.clone())) else { return };
        let n = s.planes[pe.side].map(|p| p.normal).unwrap_or(DVec3::Z);
        crate::patch::apply(&mut d, &g, &pe.ctrl, n);
        s.sides[pe.side].disp = Some(d);
        s.update_geometry();
        let now = shared_solid(&self.map, pe.solid);
        if let Some(p) = &mut self.patch_edit {
            p.applied = now;
        }
        self.touch();
    }

    /// Kontrollnetz glätten: innere Punkte auf die Ebene der Ecken (flacher Patch)
    pub fn patch_reset(&mut self) {
        let Some(pe) = self.patch_edit.clone() else { return };
        let c = pe.ctrl;
        let mut n = c;
        for r in 0..4 {
            for k in 0..4 {
                let (u, v) = (k as f64 / 3.0, r as f64 / 3.0);
                n[r * 4 + k] = (c[0] * (1.0 - u) + c[3] * u) * (1.0 - v) + (c[12] * (1.0 - u) + c[15] * u) * v;
            }
        }
        self.checkpoint();
        if let Some(p) = &mut self.patch_edit {
            p.ctrl = n;
        }
        self.patch_apply();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Aabb;

    #[test]
    fn brushes_to_mesh_and_edit() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0)), "DEV/X", 0.25);
        let sid = s.id;
        ed.map.world.solids.push(s.into());
        ed.sel = [Obj::Solid(sid)].into_iter().collect();
        let ent = ed.mesh_from_selection().unwrap();
        assert!(ed.map.solid(sid).is_none(), "Brush entfernt");
        assert_eq!(ed.mesh_of(ent).unwrap().mesh.verts.len(), 8);
        // Prop steht unten in der Mitte, Box passt zum Netz
        let b = ed.entity_bounds(ed.map.entity(ent).unwrap());
        assert!((b.min - DVec3::ZERO).length() < 1e-6 && (b.max - DVec3::new(128.0, 128.0, 64.0)).length() < 1e-6, "{b:?}");
        // obere Fläche wählen und hochziehen
        ed.mesh_edit.as_mut().unwrap().faces_mode = true;
        let top = ed.grips().into_iter().max_by(|a, b| a.1.z.total_cmp(&b.1.z)).unwrap().0;
        ed.grip_select(Some(top), false);
        let base = ed.grip_base().unwrap();
        ed.checkpoint();
        ed.grip_drag(&base, DVec3::Z * 32.0);
        let b = ed.entity_bounds(ed.map.entity(ent).unwrap());
        assert!((b.max.z - 96.0).abs() < 1e-6);
        // extrudieren: 4 neue Seitenflächen
        ed.mesh_op(MeshOp::Extrude(16.0));
        assert_eq!(ed.mesh_of(ent).unwrap().mesh.faces.len(), 10);
        // Rückgängig stellt das Netz wieder her
        ed.undo();
        assert_eq!(ed.mesh_of(ent).unwrap().mesh.faces.len(), 6);
    }

    #[test]
    fn save_and_load_sidecar() {
        let dir = std::env::temp_dir().join(format!("amboss_mesh_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vmf = dir.join("t.vmf");
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)), "DEV/X", 0.25);
        let sid = s.id;
        ed.map.world.solids.push(s.into());
        ed.sel = [Obj::Solid(sid)].into_iter().collect();
        let ent = ed.mesh_from_selection().unwrap();
        ed.save(vmf.clone()).unwrap();
        assert!(Editor::mesh_file(&vmf).is_file());
        let mut ed2 = Editor::new(crate::fgd::Fgd::default());
        ed2.load(vmf.clone()).unwrap();
        assert_eq!(ed2.mesh_of(ent).map(|m| m.mesh.faces.len()), Some(6));
        // ohne Meshes verschwindet die Begleitdatei
        let mut ed3 = Editor::new(crate::fgd::Fgd::default());
        ed3.save(vmf.clone()).unwrap();
        assert!(!Editor::mesh_file(&vmf).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_on_top_face() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(256.0, 256.0, 32.0)), "DEV/X", 0.25);
        let sid = s.id;
        ed.map.world.solids.push(s.into());
        ed.sel = [Obj::Solid(sid)].into_iter().collect();
        ed.patch_begin().unwrap();
        assert!(ed.map.solid(sid).unwrap().has_disp());
        // mittlere Punkte hochziehen
        for i in [5, 6, 9, 10] {
            ed.grip_select(Some(i), true);
        }
        let base = ed.grip_base().unwrap();
        ed.grip_drag(&base, DVec3::Z * 100.0);
        let s = ed.map.solid(sid).unwrap();
        let g = s.disps.iter().flatten().next().unwrap();
        let mid = g.pos[(g.res / 2) * g.res + g.res / 2];
        assert!(mid.z > 32.0 + 50.0, "{mid:?}");
        // eigene Änderung ändert das Netz nicht; Rückgängig schon
        let before = ed.patch_edit.as_ref().unwrap().ctrl;
        ed.patch_sync();
        assert_eq!(before, ed.patch_edit.as_ref().unwrap().ctrl);
    }
}
