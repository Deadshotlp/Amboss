//! Weitere Editor-Operationen (Komfort-Werkzeuge aus anderen Editoren).

use crate::editor::{ClipItem, Editor, Obj};
use crate::geometry::{plane_points_for, Aabb};
use crate::map::{Entity, Map, Shared, Solid};
use crate::transform::Xform;
use glam::DVec3;
use std::collections::BTreeSet;

/// Live-Drehen zur Maus (Strg+R)
pub struct RotLive {
    /// Zustand vor dem Drehen
    pub base: Box<Map>,
    /// Drehpunkt (Mitte der Auswahl)
    pub center: DVec3,
    /// Mauswinkel beim ersten Bild (Drehung relativ dazu)
    pub start: Option<f64>,
    /// Blickrichtung (Yaw) eines einzelnen Punkt-Entities: es schaut dann direkt zur Maus
    pub ent_yaw: Option<f64>,
    pub began: bool,
    /// aktuelle Drehung in Grad (für die Anzeige)
    pub deg: f64,
    /// Achse, um die zuletzt gedreht wurde (Wechsel der Ansicht = neu ansetzen)
    pub axis: Option<usize>,
}

impl Editor {
    /// Auswahl an Ort und Stelle duplizieren und die Kopien auswählen (Shift+Ziehen)
    pub fn duplicate_in_place(&mut self) -> usize {
        if self.sel.is_empty() {
            return 0;
        }
        self.checkpoint();
        let mut new_sel = BTreeSet::new();
        for o in self.sel.clone() {
            match o {
                Obj::Solid(id) => {
                    let Some(mut s) = self.map.solid(id).cloned() else { continue };
                    self.map.reassign_ids(&mut s);
                    new_sel.insert(Obj::Solid(s.id));
                    match self.map.solid_owner(id) {
                        Some(Some(eid)) => {
                            if let Some(e) = self.map.entity_mut(eid) {
                                e.solids.push(s.into());
                            }
                        }
                        _ => self.map.world.solids.push(s.into()),
                    }
                }
                Obj::Entity(id) => {
                    let Some(mut e) = self.map.entity(id).cloned() else { continue };
                    self.map.reassign_entity_ids(&mut e);
                    new_sel.insert(Obj::Entity(e.id));
                    self.map.entities.push(e.into());
                }
            }
        }
        let n = new_sel.len();
        self.sel = new_sel;
        self.status = format!("{} dupliziert", crate::i18n::count_de(n, "Objekt", "Objekte"));
        self.touch();
        n
    }

    pub fn lock_selection(&mut self, lock: bool) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        for o in std::mem::take(&mut self.sel) {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid_mut(id) {
                        s.editor.locked = lock;
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity_mut(id) {
                        e.editor.locked = lock;
                    }
                }
            }
        }
        self.status = "Gesperrt – nicht mehr anklickbar (Shift+L entsperrt alles)".into();
        self.touch();
    }

    pub fn unlock_all(&mut self) {
        self.checkpoint();
        for s in &mut self.map.world.solids {
            s.editor.locked = false;
        }
        for e in &mut self.map.entities {
            e.editor.locked = false;
            for s in &mut e.solids {
                s.editor.locked = false;
            }
        }
        for v in &mut self.map.visgroups {
            v.locked = false;
        }
        self.status = "Alles entsperrt".into();
        self.touch();
    }

    pub fn set_visgroup_locked(&mut self, vg: u32, locked: bool) {
        if let Some(v) = self.map.visgroups.iter_mut().find(|v| v.id == vg) {
            v.locked = locked;
            self.dirty = true;
            self.touch();
        }
    }

    /// Eine Fläche entlang ihrer Normale verschieben (Brush wird größer/kleiner). Ausgangspunkt ist `base`.
    pub fn move_face_from(&mut self, base: &Map, sid: u32, side: usize, dist: f64) -> bool {
        let Some(orig) = base.solid(sid) else { return false };
        if orig.has_disp() {
            return false;
        }
        let Some(n) = orig.planes.get(side).copied().flatten().map(|p| p.normal) else { return false };
        let mut s = orig.clone();
        for p in &mut s.sides[side].plane_pts {
            *p += n * dist;
        }
        s.cleanup();
        if !s.is_valid() || s.sides.len() < 4 {
            return false;
        }
        if let Some(dst) = self.map.solid_mut(sid) {
            *dst = s;
        }
        self.touch();
        true
    }

    /// Neuen Brush aus einer Fläche herausziehen. Die Map wird jedes Mal aus `base` wiederhergestellt.
    pub fn extrude_face_from(&mut self, base: &Map, sid: u32, side: usize, dist: f64) -> Option<u32> {
        let orig = base.solid(sid)?.clone();
        let poly = orig.polys.get(side)?.clone();
        let n = orig.planes.get(side).copied().flatten()?.normal;
        let mat = orig.sides[side].material.clone();
        self.map = base.clone();
        if dist.abs() < 0.5 || poly.len() < 3 {
            return None;
        }
        let mut pts = poly.clone();
        pts.extend(poly.iter().map(|p| *p + n * dist));
        let s = self.map.solid_from_points(&pts, &mat, self.tex_scale)?;
        let id = s.id;
        self.map.world.solids.push(s.into());
        self.sel.clear();
        self.sel.insert(Obj::Solid(id));
        self.touch();
        Some(id)
    }

    /// Flächen, die vollständig von anderen Brushes verdeckt sind, auf TOOLS/TOOLSNODRAW setzen
    pub fn nodraw_covered_faces(&mut self, only_selection: bool, fs: Option<&crate::gamefs::GameFs>) -> usize {
        let world: Vec<Shared<Solid>> = self.map.world.solids.clone();
        let targets: BTreeSet<u32> = if only_selection { self.selected_solid_ids().into_iter().collect() } else { world.iter().map(|s| s.id).collect() };
        // Nur Brushes, die wirklich die Sicht versperren, zählen als Abdeckung:
        // keine Clip-/Trigger-/Hint-Brushes, kein Wasser, kein Glas, keine Gitter
        let mut see: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
        let mut occluders: Vec<&Shared<Solid>> = Vec::new();
        for o in &world {
            if o.has_disp() {
                continue;
            }
            let opaque = o.sides.iter().all(|sd| !*see.entry(sd.material.to_ascii_lowercase()).or_insert_with(|| crate::materials::see_through(fs, &sd.material)));
            if opaque {
                occluders.push(o);
            }
        }
        let inside = |p: DVec3, skip: u32| occluders.iter().any(|o| o.id != skip && o.bounds.contains_point(p) && o.planes.iter().flatten().all(|pl| pl.distance(p) < -0.01));
        let mut changes: Vec<(u32, usize)> = Vec::new();
        for s in &world {
            if !targets.contains(&s.id) || s.has_disp() {
                continue;
            }
            for (i, poly) in s.polys.iter().enumerate() {
                let Some(n) = s.planes.get(i).copied().flatten().map(|p| p.normal) else { continue };
                if poly.len() < 3 || s.sides[i].material.eq_ignore_ascii_case("TOOLS/TOOLSNODRAW") || see.get(&s.sides[i].material.to_ascii_lowercase()).copied().unwrap_or(false) {
                    continue;
                }
                // Probepunkte dicht über die ganze Fläche (Dreiecksfächer, Raster ca. alle 8 Einheiten),
                // knapp vor der Fläche – nur wenn alle in deckenden Brushes liegen, ist die Fläche verdeckt
                let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / poly.len() as f64;
                let mut covered = true;
                'tri: for k in 0..poly.len() {
                    let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                    let len = (a - c).length().max((b - c).length()).max((a - b).length());
                    let steps = ((len / 8.0).ceil() as usize).clamp(2, 32);
                    for u in 0..=steps {
                        for v in 0..=(steps - u) {
                            let (fu, fv) = (u as f64 / steps as f64, v as f64 / steps as f64);
                            let p = c + (a - c) * fu + (b - c) * fv;
                            let p = p + (c - p) * 0.01; // Kanten minimal nach innen
                            if !inside(p + n * 0.5, s.id) {
                                covered = false;
                                break 'tri;
                            }
                        }
                    }
                }
                if covered {
                    changes.push((s.id, i));
                }
            }
        }
        if changes.is_empty() {
            self.status = "Keine verdeckten Flächen gefunden".into();
            return 0;
        }
        self.checkpoint();
        for (sid, i) in &changes {
            if let Some(side) = self.map.solid_mut(*sid).and_then(|s| s.sides.get_mut(*i)) {
                side.material = "TOOLS/TOOLSNODRAW".into();
            }
        }
        self.status = format!("{} auf nodraw gesetzt", crate::i18n::count_de(changes.len(), "verdeckte Fläche", "verdeckte Flächen"));
        self.touch();
        changes.len()
    }

    /// Auswahl als VMF-Text (für die System-Zwischenablage)
    pub fn selection_vmf(&self) -> String {
        let mut m = Map::new();
        for o in &self.sel {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid(*id) {
                        m.world.solids.push(s.clone().into());
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity(*id) {
                        m.entities.push(e.clone().into());
                    }
                }
            }
        }
        let mut text = String::from("// amboss-clipboard\n");
        text.push_str(&m.to_vmf());
        text
    }

    /// VMF-Text einfügen (aus Amboss oder anderen VMF-Ausschnitten). Liefert die Anzahl Objekte.
    pub fn paste_vmf(&mut self, text: &str, offset: DVec3) -> Result<usize, String> {
        if !text.contains("world") && !text.contains("entity") {
            return Err("Kein VMF-Text in der Zwischenablage".into());
        }
        let m = Map::from_vmf(text)?;
        let mut items: Vec<ClipItem> = m.world.solids.into_iter().map(|s| ClipItem::Solid(s.into_inner())).collect();
        items.extend(m.entities.into_iter().map(|e| ClipItem::Entity(e.into_inner())));
        if items.is_empty() {
            return Err("Zwischenablage enthält keine Objekte".into());
        }
        self.checkpoint();
        let n = items.len();
        let objs = self.insert_clip_items(items, &Xform::translate(offset));
        self.sel = objs.into_iter().collect();
        self.status = format!("{} aus der Zwischenablage eingefügt", crate::i18n::count_de(n, "Objekt", "Objekte"));
        self.touch();
        Ok(n)
    }

    /// Prefab-Map einfügen. `anchor` = Zielpunkt, `normal` = Flächennormale (aus 3D) zur Ausrichtung:
    /// Boden → Unterkante auf den Punkt, Decke → Oberkante an den Punkt, Wand → Rückseite an die Wand.
    /// Bereits vorhandene Entity-Namen werden umbenannt (name_2, name_3 …) inkl. Verweisen.
    pub fn insert_prefab_map(&mut self, pm: Map, anchor: DVec3, normal: Option<DVec3>) -> Result<Vec<u32>, String> {
        let mut items: Vec<ClipItem> = pm.world.solids.into_iter().map(|s| ClipItem::Solid(s.into_inner())).collect();
        let mut ents = pm.entities;
        // Namenskonflikte auflösen
        let rename = crate::merge::rename_conflicts(&self.fgd, &self.map.targetnames(), &mut ents);
        items.extend(ents.into_iter().map(|e| ClipItem::Entity(e.into_inner())));
        let b = items.iter().fold(Aabb::EMPTY, |b, it| match it {
            ClipItem::Solid(s) => b.union(&s.bounds),
            ClipItem::Entity(e) => b.union(&self.entity_bounds(e)),
        });
        if b.is_empty() {
            return Err("Prefab ist leer".into());
        }
        let c = b.center();
        let half = b.size() * 0.5;
        let g = self.grid.min(8.0);
        let snap = |v: f64| crate::geometry::snap(v, g);
        let off = match normal {
            Some(n) if n.z < -0.7 => DVec3::new(snap(anchor.x - c.x), snap(anchor.y - c.y), anchor.z - b.max.z),
            Some(n) if n.z.abs() <= 0.7 => {
                // Wand: Prefab-Mitte vor die Wand setzen, Unterkante bleibt auf Bodenhöhe des Ankers
                let push = (n.abs() * half).element_sum();
                let target = anchor + n * push;
                DVec3::new(target.x - c.x, target.y - c.y, snap(anchor.z - c.z))
            }
            _ => DVec3::new(snap(anchor.x - c.x), snap(anchor.y - c.y), anchor.z - b.min.z),
        };
        self.checkpoint();
        let objs = self.insert_clip_items(items, &Xform::translate(off));
        self.sel = objs.iter().copied().collect();
        if !rename.is_empty() {
            self.status = format!("Prefab eingefügt – {} Name(n) umbenannt", rename.len());
        } else {
            self.status = "Prefab eingefügt".into();
        }
        self.touch();
        Ok(objs.into_iter().map(|o| match o {
            Obj::Solid(id) | Obj::Entity(id) => id,
        }).collect())
    }

    /// Yaw ausgewählter Punkt-Entities um einen Winkel drehen (feste Schritte)
    pub fn rotate_entities_yaw(&mut self, delta: f64) {
        let ids = self.selected_entity_ids();
        if ids.is_empty() {
            return;
        }
        self.checkpoint();
        for id in ids {
            if let Some(e) = self.map.entity_mut(id) {
                if e.is_brush_entity() {
                    continue;
                }
                let mut a = e.angles();
                a.y = (a.y + delta).rem_euclid(360.0);
                e.set("angles", crate::map::fmt_vec(a));
            }
        }
        self.touch();
    }

    /// Schlüssel einer Entity merken (ohne Position, Name und Brush-Modell)
    pub fn copy_entity_props(&mut self) {
        let Some(e) = self.selected_entity_ids().first().and_then(|id| self.map.entity(*id)) else {
            self.status = "Erst eine Entity auswählen".into();
            return;
        };
        const SKIP: [&str; 5] = ["classname", "origin", "targetname", "hammerid", "model"];
        let keys: Vec<(String, String)> = e.props.iter().filter(|(k, v)| !SKIP.iter().any(|s| k.eq_ignore_ascii_case(s)) && !(k.eq_ignore_ascii_case("model") && v.starts_with('*'))).cloned().collect();
        self.status = format!("{} Eigenschaft(en) von {} kopiert", keys.len(), e.classname());
        self.prop_clipboard = Some((e.classname().to_string(), keys, e.outputs.clone()));
    }

    /// Gemerkte Eigenschaften auf alle ausgewählten Entities übertragen.
    /// Bei anderer Klasse nur Schlüssel, die die Zielklasse laut FGD kennt; Outputs nur bei gleicher Klasse.
    pub fn paste_entity_props(&mut self) {
        let Some((class, keys, outputs)) = self.prop_clipboard.clone() else {
            self.status = "Nichts kopiert (Eigenschaften kopieren zuerst)".into();
            return;
        };
        let ids = self.selected_entity_ids();
        if ids.is_empty() {
            self.status = "Keine Entities ausgewählt".into();
            return;
        }
        self.checkpoint();
        let mut n = 0;
        for id in ids {
            let target_class = self.map.entity(id).map(|e| e.classname().to_string()).unwrap_or_default();
            let same = target_class.eq_ignore_ascii_case(&class);
            let known: Vec<String> = self.fgd.all_props(&target_class).iter().map(|p| p.name.to_ascii_lowercase()).collect();
            let Some(e) = self.map.entity_mut(id) else { continue };
            for (k, v) in &keys {
                if same || known.contains(&k.to_ascii_lowercase()) {
                    e.set(k, v.clone());
                }
            }
            if same {
                for o in &outputs {
                    if !e.outputs.contains(o) {
                        e.outputs.push(o.clone());
                    }
                }
            }
            n += 1;
        }
        self.status = format!("Eigenschaften auf {n} Entity/Entities übertragen");
        self.touch();
    }

    /// Alle Objekte auswählen, die die Auswahl berühren oder überschneiden
    pub fn select_touching(&mut self) {
        let boxes: Vec<Aabb> = self.selectable().into_iter().filter(|(o, _)| self.sel.contains(o)).map(|(_, b)| b).collect();
        if boxes.is_empty() {
            self.status = "Erst etwas auswählen".into();
            return;
        }
        let grow = |b: &Aabb| Aabb::new(b.min - DVec3::splat(0.5), b.max + DVec3::splat(0.5));
        let mut n = 0;
        for (o, b) in self.selectable() {
            if !self.sel.contains(&o) && boxes.iter().any(|x| grow(x).intersects(&b)) {
                for x in self.expand(o) {
                    self.sel.insert(x);
                }
                n += 1;
            }
        }
        self.status = format!("{} hinzugefügt", crate::i18n::count_de(n, "berührendes Objekt", "berührende Objekte"));
        self.touch();
    }

    /// Grenzen der Objekte, die ein anderer Teilnehmer ausgewählt hat
    pub fn remote_selection_bounds(&self, u: &crate::collab::RemoteUser) -> Aabb {
        let mut b = Aabb::EMPTY;
        for id in &u.solids {
            if let Some(s) = self.map.solid(*id) {
                b = b.union(&s.bounds);
            }
        }
        for id in &u.entities {
            if let Some(e) = self.map.entity(*id) {
                b = b.union(&self.entity_bounds(e));
            }
        }
        b
    }

    /// Wirksames Raster zum Einrasten (Raster aus = 1 Einheit)
    pub fn snap_grid(&self) -> f64 {
        if self.snap {
            self.grid
        } else {
            1.0
        }
    }

    /// Einrasten an Eckpunkten: `moving` sind die bewegten Punkte (vor der Verschiebung), `raw` die Verschiebung in der Ansicht.
    /// Liefert eine Verschiebung, bei der ein bewegter Punkt genau auf einem fremden Eckpunkt landet (innerhalb `tol`).
    pub fn vertex_snap_delta(&self, axes: (usize, usize), moving: &[DVec3], raw: glam::DVec2, tol: f64, skip_solids: &BTreeSet<u32>) -> Option<glam::DVec2> {
        let (h, v) = axes;
        if moving.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = (glam::DVec2::splat(f64::MAX), glam::DVec2::splat(f64::MIN));
        for m in moving {
            let p = glam::DVec2::new(m[h], m[v]) + raw;
            lo = lo.min(p);
            hi = hi.max(p);
        }
        lo -= glam::DVec2::splat(tol);
        hi += glam::DVec2::splat(tol);
        let mut best: Option<(f64, glam::DVec2)> = None;
        let mut consider = |c: glam::DVec2| {
            if c.x < lo.x || c.y < lo.y || c.x > hi.x || c.y > hi.y {
                return;
            }
            for m in moving.iter().take(4000) {
                let p = glam::DVec2::new(m[h], m[v]) + raw;
                let d = c - p;
                let l = d.length();
                if l <= tol && best.is_none_or(|b| l < b.0) {
                    best = Some((l, raw + d));
                }
            }
        };
        for (e, s) in self.map.solids_owned() {
            if skip_solids.contains(&s.id) || !self.is_visible_solid(e, s) {
                continue;
            }
            let b = &s.bounds;
            if b.max[h] < lo.x || b.min[h] > hi.x || b.max[v] < lo.y || b.min[v] > hi.y {
                continue;
            }
            for p in s.vertices() {
                consider(glam::DVec2::new(p[h], p[v]));
            }
        }
        for e in self.map.entities.iter().filter(|e| !e.is_brush_entity() && !self.sel.contains(&Obj::Entity(e.id))) {
            if let Some(o) = e.origin() {
                consider(glam::DVec2::new(o[h], o[v]));
            }
        }
        best.map(|b| b.1)
    }

    /// Alle Eckpunkte der Auswahl (Brushes und Punkt-Entities) und die IDs der beteiligten Brushes
    pub fn selection_points(&self) -> (Vec<DVec3>, BTreeSet<u32>) {
        let mut pts = Vec::new();
        let mut ids = BTreeSet::new();
        for o in &self.sel {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid(*id) {
                        pts.extend(s.vertices());
                        ids.insert(*id);
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity(*id) {
                        if e.is_brush_entity() {
                            for s in &e.solids {
                                pts.extend(s.vertices());
                                ids.insert(s.id);
                            }
                        } else if let Some(o) = e.origin() {
                            pts.push(o);
                        }
                    }
                }
            }
        }
        (pts, ids)
    }

    /// Grenzen aller sichtbaren Objekte
    pub fn map_bounds(&self) -> Aabb {
        let mut b = self.map.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
        for e in &self.map.entities {
            if !e.is_brush_entity() {
                b = b.union(&self.entity_bounds(e));
            }
        }
        b
    }

    /// Plane-Punkte nach Änderungen auf echte Eckpunkte legen (Hilfe für Importe)
    pub fn normalize_planes(s: &mut Solid) {
        let polys = s.polys.clone();
        for (side, poly) in s.sides.iter_mut().zip(polys) {
            if let (Some(pl), true) = (side.plane(), poly.len() >= 3) {
                if let Some(pts) = plane_points_for(pl.normal, &poly) {
                    side.plane_pts = pts;
                }
            }
        }
        s.update_geometry();
    }

    #[allow(dead_code)]
    fn _entity(&self, _: &Entity) {}

    /// Strg+R: Live-Drehen zur Maus starten
    pub fn rot_live_begin(&mut self) {
        if self.sel.is_empty() {
            self.status = crate::i18n::tr("Erst etwas auswählen, dann Strg+R").into();
            return;
        }
        let ent_yaw = match (self.sel.len(), self.sel.iter().next()) {
            (1, Some(Obj::Entity(id))) => self.map.entity(*id).filter(|e| !e.is_brush_entity()).map(|e| e.angles().y),
            _ => None,
        };
        self.rot_live = Some(RotLive { base: Box::new(self.map.clone()), center: self.selection_bounds().center(), start: None, ent_yaw, began: false, deg: 0.0, axis: None });
        self.status = crate::i18n::tr("Drehen: Maus bewegen (15°-Schritte, Shift = frei) · Linksklick übernehmen · Rechtsklick oder Esc bricht ab").into();
    }

    /// Mauswinkel (Grad, um die Achse `axis` durch den Drehpunkt) → Vorschau
    pub fn rot_live_update(&mut self, axis: usize, angle: f64, free: bool) {
        let Some(mut r) = self.rot_live.take() else { return };
        let step = |d: f64| if free { d.round() } else { (d / 15.0).round() * 15.0 };
        if r.axis != Some(axis) {
            r.axis = Some(axis);
            r.start = None;
            r.deg = f64::NAN;
        }
        let d = match (r.ent_yaw, axis) {
            // einzelnes Entity von oben/3D: Blickrichtung zeigt zur Maus
            (Some(yaw), 2) => step(angle) - yaw,
            _ => {
                let start = *r.start.get_or_insert(angle);
                step(angle - start)
            }
        };
        let d = (d + 540.0).rem_euclid(360.0) - 180.0;
        if !((d - r.deg).abs() <= 1e-6) || !r.began {
            if !r.began && d.abs() > 1e-6 {
                self.checkpoint();
                r.began = true;
            }
            if r.began {
                self.transform_selection_from(&r.base, &Xform::about(r.center, Xform::rotation(axis, d)), false);
                self.touch();
            }
            r.deg = d;
        }
        self.rot_live = Some(r);
    }

    /// Linksklick: Drehung behalten
    pub fn rot_live_commit(&mut self) {
        if let Some(r) = self.rot_live.take() {
            self.status = crate::i18n::tr_dyn(&format!("Gedreht um {}°", r.deg.round()));
        }
    }

    /// Rechtsklick/Esc: zurück zum Zustand vor dem Drehen
    pub fn rot_live_cancel(&mut self) {
        if let Some(r) = self.rot_live.take() {
            if r.began {
                self.transform_selection_from(&r.base, &Xform::translate(DVec3::ZERO), false);
                self.touch();
            }
            self.status = crate::i18n::tr("Drehen abgebrochen").into();
        }
    }

}

#[cfg(test)]
mod tests {
    use crate::editor::{Editor, Obj};
    use crate::fgd::Fgd;
    use crate::geometry::Aabb;
    use glam::DVec3;

    #[test]
    fn rotate_live_to_mouse() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let id = ed.map.alloc_id();
        let mut e = crate::map::Entity::new(id, "info_player_start");
        e.set("origin", "64 0 0");
        e.set("angles", "0 0 0");
        ed.map.entities.push(e.into());
        ed.sel.insert(Obj::Entity(id));
        ed.rot_live_begin();
        // Maus „oben“ (90°): Entity schaut zur Maus
        ed.rot_live_update(2, 88.0, false);
        assert!((ed.map.entity(id).unwrap().angles().y - 90.0).abs() < 0.01);
        assert_eq!(ed.map.entity(id).unwrap().origin().unwrap(), DVec3::new(64.0, 0.0, 0.0), "dreht um die eigene Mitte");
        ed.rot_live_cancel();
        assert!(ed.map.entity(id).unwrap().angles().y.abs() < 0.01, "Abbrechen stellt zurück");
        // Brush: relativ zur Startposition der Maus, Linksklick behält
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::new(128.0, 32.0, 32.0)), "a", 0.25);
        let sid = s.id;
        ed.map.world.solids.push(s.into());
        ed.sel.clear();
        ed.sel.insert(Obj::Solid(sid));
        ed.rot_live_begin();
        ed.rot_live_update(2, 10.0, false);
        ed.rot_live_update(2, 100.0, false);
        ed.rot_live_commit();
        let b = ed.map.solid(sid).unwrap().bounds;
        assert!((b.size().x - 32.0).abs() < 0.01 && (b.size().y - 128.0).abs() < 0.01, "{:?}", b.size());
        assert!(ed.rot_live.is_none());
    }

    #[test]
    fn face_ops_and_duplicate() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let sid = ed.map.world.solids[0].id;
        let top = ed.map.world.solids[0].sides.iter().position(|s| s.plane().unwrap().normal.z > 0.9).unwrap();
        let base = ed.map.clone();
        assert!(ed.move_face_from(&base, sid, top, 32.0));
        assert_eq!(ed.map.world.solids[0].bounds.max.z, 96.0);
        assert!(!ed.move_face_from(&base, sid, top, -100.0));
        let base = ed.map.clone();
        let id = ed.extrude_face_from(&base, sid, top, 48.0).unwrap();
        assert_eq!(ed.map.world.solids.len(), 2);
        assert_eq!(ed.map.solid(id).unwrap().bounds.max.z, 144.0);
        assert_eq!(ed.duplicate_in_place(), 1);
        assert_eq!(ed.map.world.solids.len(), 3);
    }

    #[test]
    fn nodraw_lock_clipboard() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::new(128.0, 128.0, 16.0)));
        ed.create_block(Aabb::new(DVec3::new(32.0, 32.0, -64.0), DVec3::new(96.0, 96.0, 0.0)));
        // Unterseite des oberen Blocks ist nur teilweise verdeckt, Oberseite des unteren ganz
        assert_eq!(ed.nodraw_covered_faces(false, None), 1);
        {
            // Clip-Brush über einem Boden verdeckt nichts
            let mut e2 = Editor::new(Fgd::builtin());
            e2.create_block(Aabb::new(DVec3::new(0.0, 0.0, -16.0), DVec3::new(128.0, 128.0, 0.0)));
            e2.material = "TOOLS/TOOLSPLAYERCLIP".into();
            e2.create_block(Aabb::new(DVec3::new(-16.0, -16.0, 0.0), DVec3::new(144.0, 144.0, 64.0)));
            assert_eq!(e2.nodraw_covered_faces(false, None), 0);
            // Zwei Klötze an den Ecken, Lücke in der Mitte: Fläche bleibt sichtbar
            let mut e3 = Editor::new(Fgd::builtin());
            e3.create_block(Aabb::new(DVec3::new(0.0, 0.0, -16.0), DVec3::new(128.0, 128.0, 0.0)));
            e3.create_block(Aabb::new(DVec3::new(-8.0, -8.0, 0.0), DVec3::new(60.0, 136.0, 32.0)));
            e3.create_block(Aabb::new(DVec3::new(68.0, -8.0, 0.0), DVec3::new(136.0, 136.0, 32.0)));
            e3.nodraw_covered_faces(false, None);
            let floor = &e3.map.world.solids[0];
            assert!(floor.sides.iter().all(|s| !s.material.eq_ignore_ascii_case("TOOLS/TOOLSNODRAW")));
        }
        let id = ed.map.world.solids[0].id;
        ed.select(Some(Obj::Solid(id)), false);
        ed.lock_selection(true);
        assert!(ed.pick_2d(crate::geometry::Axis2D::Top, glam::DVec2::new(10.0, 10.0), 1.0).is_empty());
        ed.unlock_all();
        ed.select(Some(Obj::Solid(id)), false);
        let text = ed.selection_vmf();
        assert_eq!(ed.paste_vmf(&text, DVec3::new(0.0, 0.0, 200.0)).unwrap(), 1);
        assert_eq!(ed.map.world.solids.len(), 3);
        assert!(ed.paste_vmf("hallo", DVec3::ZERO).is_err());
        let (undo, _) = ed.history();
        assert!(undo.len() >= 3);
    }

    #[test]
    fn prefab_insert_alignment_and_names() {
        let mut ed = Editor::new(Fgd::builtin());
        let door = crate::prefabs::build_builtin("schiebetuer").unwrap();
        ed.insert_prefab_map(door.clone(), DVec3::new(0.0, 0.0, 64.0), None).unwrap();
        assert_eq!(ed.selection_bounds().min.z, 64.0);
        // zweites Mal: Namen werden umbenannt, Outputs zeigen auf den neuen Namen
        ed.insert_prefab_map(door, DVec3::new(512.0, 0.0, 0.0), Some(DVec3::Z)).unwrap();
        let names = ed.map.targetnames();
        assert!(names.contains(&"tuer".to_string()) && names.contains(&"tuer_2".to_string()));
        let new_btn = ed.map.entities.iter().filter(|e| e.classname() == "func_button").last().unwrap();
        assert!(new_btn.outputs[0].1.starts_with("tuer_2,"));
        // Decke: Oberkante an den Punkt
        let lamp = crate::prefabs::build_builtin("deckenlampe").unwrap();
        ed.insert_prefab_map(lamp, DVec3::new(0.0, 0.0, 256.0), Some(-DVec3::Z)).unwrap();
        assert_eq!(ed.selection_bounds().max.z, 256.0);
        // Wand mit Normale +X: Prefab liegt vollständig auf der +X-Seite
        let crate_ = crate::prefabs::build_builtin("saeule").unwrap();
        ed.insert_prefab_map(crate_, DVec3::new(100.0, 0.0, 0.0), Some(DVec3::X)).unwrap();
        assert!(ed.selection_bounds().min.x >= 99.9);
    }
}
