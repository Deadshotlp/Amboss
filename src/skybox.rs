//! 3D-Skybox: Raum um die sky_camera finden, anlegen, Teile der Map verkleinert hineinkopieren, prüfen.
//!
//! Abbildung wie in der Engine: Ein Weltpunkt `p` erscheint im Spiel dort, wo im Skybox-Raum der Punkt
//! `origin + p / scale` liegt (`origin` = Position der sky_camera, `scale` meist 16).

use crate::editor::{solid_tool_kind, ClipItem, Editor, Obj, ToolKind};
use crate::geometry::Aabb;
use crate::map::{fmt_num, fmt_vec, Entity, Map, Solid};
use crate::transform::Xform;
use glam::{DMat3, DVec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyRoom {
    /// Entity-ID der sky_camera
    pub camera: u32,
    pub origin: DVec3,
    pub scale: f64,
    /// Raum bis zu den Wänden (+32 Einheiten, damit die Wände dazugehören)
    pub bounds: Aabb,
}

impl SkyRoom {
    /// Weltpunkt → Punkt im Skybox-Raum
    pub fn to_sky(self, p: DVec3) -> DVec3 {
        self.origin + p / self.scale
    }

    /// Punkt im Skybox-Raum → Weltpunkt
    pub fn to_world(self, p: DVec3) -> DVec3 {
        (p - self.origin) * self.scale
    }

    /// Transformation Welt → Skybox (verkleinert)
    pub fn shrink_xform(&self) -> Xform {
        Xform { m: DMat3::from_diagonal(DVec3::splat(1.0 / self.scale)), t: self.origin }
    }
}

pub fn sky_cameras(map: &Map) -> Vec<&Entity> {
    map.entities.iter().map(|e| &**e).filter(|e| e.classname().eq_ignore_ascii_case("sky_camera")).collect()
}

fn camera_scale(e: &Entity) -> f64 {
    e.get("scale").and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| *s >= 1.0).unwrap_or(16.0)
}

fn has_sky_side(s: &Solid) -> bool {
    s.sides.iter().any(|x| x.material.to_ascii_lowercase().contains("toolsskybox"))
}

/// Raum um die (erste) sky_camera: Strahlen in sechs Richtungen bis zur nächsten Wand.
/// Zuerst zählen nur Brushes mit Skybox-Textur (Inhalte im Raum versperren sonst den Blick),
/// fehlt dort eine Wand, jeder Welt-Brush. Kein Treffer in einer Richtung → Raum offen → None.
pub fn sky_room(map: &Map) -> Option<SkyRoom> {
    let cam = sky_cameras(map).into_iter().next()?;
    let o = cam.origin()?;
    let mut b = Aabb::new(o, o);
    for d in [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::Z] {
        let hit = |only_sky: bool| map.world.solids.iter().filter(|s| !only_sky || has_sky_side(s)).filter_map(|s| s.ray_hit(o, d).map(|h| h.0)).fold(f64::MAX, f64::min);
        let mut t = hit(true);
        if t == f64::MAX {
            t = hit(false);
        }
        if t == f64::MAX {
            return None;
        }
        b.add_point(o + d * (t + 32.0));
    }
    Some(SkyRoom { camera: cam.id, origin: o, scale: camera_scale(cam), bounds: b })
}

/// Ausmaße der spielbaren Map (alles außerhalb des Skybox-Raums)
pub fn world_bounds(map: &Map, room: Option<&Aabb>) -> Aabb {
    let mut b = Aabb::EMPTY;
    let outside = |c: DVec3| room.is_none_or(|r| !r.contains_point(c));
    for (_, s) in map.solids_owned() {
        if outside(s.bounds.center()) {
            b = b.union(&s.bounds);
        }
    }
    for e in &map.entities {
        if e.is_brush_entity() || e.classname().eq_ignore_ascii_case("sky_camera") {
            continue;
        }
        if let Some(o) = e.origin() {
            if outside(o) {
                b.add_point(o);
            }
        }
    }
    b
}

/// Einstellungen für „3D-Skybox anlegen“
#[derive(Debug, Clone)]
pub struct CreateOpts {
    pub scale: f64,
    /// Innenmaß des Raums
    pub size: DVec3,
    /// Bodenplatte in Höhe des tiefsten Map-Punkts (verkleinert) mit diesem Material
    pub ground: Option<String>,
    /// Nebel der Map (env_fog_controller) auf die sky_camera übertragen
    pub copy_fog: bool,
}

/// Vorschlag für die Raumgröße: die verkleinerte Map dreifach in der Breite (mind. 1024), Höhe mind. 512
pub fn suggest_size(map: &Map, scale: f64) -> DVec3 {
    let w = world_bounds(map, None);
    if w.is_empty() {
        return DVec3::new(1024.0, 1024.0, 512.0);
    }
    let s = w.size() / scale;
    let r = |v: f64, min: f64| ((v * 3.0).max(min) / 64.0).ceil() * 64.0;
    DVec3::new(r(s.x.max(s.y), 1024.0), r(s.x.max(s.y), 1024.0), r(s.z, 512.0))
}

/// Wohin mit dem Raum? Über der Map, sonst darunter, sonst daneben – innerhalb der Map-Grenze.
/// Liefert (Innenraum, Position der sky_camera).
pub fn placement(map: &Map, opts: &CreateOpts, limit: f64) -> Result<(Aabb, DVec3), String> {
    let w = world_bounds(map, None);
    let w = if w.is_empty() { Aabb::new(DVec3::splat(-512.0), DVec3::splat(512.0)) } else { w };
    let half = opts.size / 2.0;
    let gap = 256.0 + 16.0;
    let wc = w.center();
    let mut candidates = vec![
        DVec3::new(wc.x, wc.y, w.max.z + gap + half.z),
        DVec3::new(wc.x, wc.y, w.min.z - gap - half.z),
        DVec3::new(w.max.x + gap + half.x, wc.y, wc.z),
        DVec3::new(w.min.x - gap - half.x, wc.y, wc.z),
        DVec3::new(wc.x, w.max.y + gap + half.y, wc.z),
        DVec3::new(wc.x, w.min.y - gap - half.y, wc.z),
    ];
    // Ecken der Map-Grenze als letzte Möglichkeit
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            candidates.push(DVec3::new(sx * (limit - half.x - 64.0), sy * (limit - half.y - 64.0), limit - half.z - 64.0));
        }
    }
    let lim = limit - 32.0;
    for c in candidates {
        let c = (c / 16.0).round() * 16.0;
        let inner = Aabb::new(c - half, c + half);
        let outer = Aabb::new(inner.min - DVec3::splat(16.0), inner.max + DVec3::splat(16.0));
        if outer.min.min_element() < -lim || outer.max.max_element() > lim {
            continue;
        }
        if map.solids_owned().any(|(_, s)| s.bounds.intersects(&outer)) {
            continue;
        }
        // sky_camera so, dass die verkleinerte Map in der Raummitte liegt (Boden etwas über dem Raumboden)
        let mut cam = c - wc / opts.scale;
        let floor = inner.min.z + 32.0;
        let map_floor = cam.z + w.min.z / opts.scale;
        if map_floor < floor {
            cam.z += floor - map_floor;
        }
        let cam = (cam / 8.0).round() * 8.0;
        if !inner.contains_point(cam) {
            continue;
        }
        return Ok((inner, cam));
    }
    Err("Kein freier Platz innerhalb der Map-Grenze für den Skybox-Raum".into())
}

/// Legt Skybox-Raum (sechs Wände mit tools/toolsskybox) und sky_camera an. Ein Rückgängig-Schritt.
pub fn create(ed: &mut Editor, opts: &CreateOpts) -> Result<SkyRoom, String> {
    if !sky_cameras(&ed.map).is_empty() {
        return Err("Die Map hat schon eine sky_camera".into());
    }
    let (inner, cam) = placement(&ed.map, opts, ed.map_limit)?;
    ed.checkpoint();
    let t = 16.0;
    let (a, b) = (inner.min, inner.max);
    let walls = [
        Aabb::new(DVec3::new(a.x - t, a.y - t, a.z - t), DVec3::new(b.x + t, b.y + t, a.z)),
        Aabb::new(DVec3::new(a.x - t, a.y - t, b.z), DVec3::new(b.x + t, b.y + t, b.z + t)),
        Aabb::new(DVec3::new(a.x - t, a.y - t, a.z), DVec3::new(a.x, b.y + t, b.z)),
        Aabb::new(DVec3::new(b.x, a.y - t, a.z), DVec3::new(b.x + t, b.y + t, b.z)),
        Aabb::new(DVec3::new(a.x, a.y - t, a.z), DVec3::new(b.x, a.y, b.z)),
        Aabb::new(DVec3::new(a.x, b.y, a.z), DVec3::new(b.x, b.y + t, b.z)),
    ];
    let mut new_sel = Vec::new();
    for w in walls {
        let s = ed.map.make_box(w, "TOOLS/TOOLSSKYBOX", ed.tex_scale);
        new_sel.push(Obj::Solid(s.id));
        ed.map.world.solids.push(s.into());
    }
    let w = world_bounds(&ed.map, Some(&inner));
    if let Some(mat) = &opts.ground {
        // Boden in Höhe des tiefsten Map-Punkts, über die ganze Raumfläche
        let z = if w.is_empty() { cam.z } else { cam.z + w.min.z / opts.scale };
        let z = z.max(a.z + 16.0);
        let g = ed.map.make_box(Aabb::new(DVec3::new(a.x, a.y, a.z), DVec3::new(b.x, b.y, z)), mat, ed.tex_scale);
        new_sel.push(Obj::Solid(g.id));
        ed.map.world.solids.push(g.into());
    }
    let id = ed.map.alloc_id();
    let mut e = Entity::new(id, "sky_camera");
    e.set("origin", fmt_vec(cam));
    e.set("angles", "0 0 0");
    e.set("scale", fmt_num(opts.scale));
    e.set("use_angles", "0");
    e.set("fogenable", "0");
    if opts.copy_fog {
        if let Some(f) = ed.map.entities.iter().find(|e| e.classname().eq_ignore_ascii_case("env_fog_controller")) {
            for k in ["fogenable", "fogblend", "fogcolor", "fogcolor2", "fogdir", "fogstart", "fogend", "fogmaxdensity"] {
                if let Some(v) = f.get(k) {
                    e.set(k, v.to_string());
                }
            }
        }
    }
    ed.map.entities.push(e.into());
    new_sel.push(Obj::Entity(id));
    ed.sel = new_sel.into_iter().collect();
    ed.touch();
    let room = sky_room(&ed.map).ok_or("Skybox-Raum nicht geschlossen")?;
    ed.status = format!("3D-Skybox angelegt (Maßstab 1:{})", fmt_num(opts.scale));
    Ok(room)
}

/// Kopiert die ausgewählten Brushes (Welt und Brush-Entities) verkleinert an die passende Stelle der Skybox.
/// Punkt-Entities werden übersprungen: Props lassen sich in Source nicht verkleinern.
/// Liefert (kopiert, übersprungen).
pub fn copy_selection_to_sky(ed: &mut Editor) -> Result<(usize, usize), String> {
    let room = sky_room(&ed.map).ok_or("Keine 3D-Skybox (sky_camera mit geschlossenem Raum) gefunden")?;
    let mut items = Vec::new();
    let mut skipped = 0;
    for o in ed.sel.clone() {
        match o {
            Obj::Solid(id) => {
                if let Some(s) = ed.map.solid(id) {
                    if room.bounds.contains_point(s.bounds.center()) {
                        skipped += 1;
                        continue;
                    }
                    items.push(ClipItem::Solid(s.clone()));
                }
            }
            Obj::Entity(id) => {
                if let Some(e) = ed.map.entity(id) {
                    if e.is_brush_entity() && !e.solids.iter().any(|s| room.bounds.contains_point(s.bounds.center())) {
                        items.push(ClipItem::Entity(e.clone()));
                    } else {
                        skipped += 1;
                    }
                }
            }
        }
    }
    if items.is_empty() {
        return Err("Nichts zum Kopieren ausgewählt (Brushes außerhalb der Skybox)".into());
    }
    ed.checkpoint();
    let n = items.len();
    // Texturen mitskalieren, damit sie im Spiel (16-fach vergrößert) wieder passen
    let (tl, sl) = (ed.tex_lock, ed.scale_lock);
    ed.tex_lock = true;
    ed.scale_lock = true;
    let objs = ed.insert_clip_items(items, &room.shrink_xform());
    ed.tex_lock = tl;
    ed.scale_lock = sl;
    ed.sel = objs.into_iter().collect();
    ed.touch();
    Ok((n, skipped))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
}

/// Prüfung der 3D-Skybox; leere Liste = alles in Ordnung
pub fn check(map: &Map, limit: f64) -> Vec<(Level, String)> {
    let mut out = Vec::new();
    let cams = sky_cameras(map);
    if cams.is_empty() {
        out.push((Level::Info, "Keine sky_camera – die Map hat keine 3D-Skybox.".to_string()));
        return out;
    }
    if cams.len() > 1 {
        out.push((Level::Warn, format!("{} sky_cameras – Source nutzt pro Bereich nur eine; meist ist das ein Versehen.", cams.len())));
    }
    if map.world.get("skyname").is_none_or(|s| s.trim().is_empty()) {
        out.push((Level::Warn, "Kein 2D-Himmel (skyname in den Map-Eigenschaften) gesetzt.".to_string()));
    }
    let cam = cams[0];
    let scale = cam.get("scale").and_then(|s| s.trim().parse::<f64>().ok());
    match scale {
        None => out.push((Level::Warn, "sky_camera ohne gültigen scale – die Engine nimmt 16.".to_string())),
        Some(s) if s < 1.0 => out.push((Level::Error, "scale der sky_camera muss mindestens 1 sein.".to_string())),
        Some(s) if s != 16.0 => out.push((Level::Info, format!("Maßstab 1:{} – üblich ist 1:16.", fmt_num(s)))),
        _ => {}
    }
    let Some(room) = sky_room(map) else {
        out.push((Level::Error, "Skybox-Raum nicht geschlossen: von der sky_camera aus führt mindestens eine Richtung ins Leere.".to_string()));
        return out;
    };
    // Wände mit Skybox-Textur
    let walls = map.world.solids.iter().filter(|s| room.bounds.intersects(&s.bounds) && has_sky_side(s)).count();
    if walls == 0 {
        out.push((Level::Warn, "Die Wände des Skybox-Raums haben keine tools/toolsskybox-Textur – dort ist kein Himmel zu sehen.".to_string()));
    }
    // Überschneidung mit der spielbaren Map
    let spawns = map.entities.iter().filter(|e| e.classname().to_ascii_lowercase().starts_with("info_player")).filter(|e| e.origin().is_some_and(|o| room.bounds.contains_point(o))).count();
    if spawns > 0 {
        out.push((Level::Error, "Spawnpunkte liegen im Skybox-Raum.".to_string()));
    }
    let world = world_bounds(map, Some(&room.bounds));
    if !world.is_empty() {
        let foot = Aabb::new(room.to_sky(world.min), room.to_sky(world.max));
        let inner = Aabb::new(room.bounds.min + DVec3::splat(32.0), room.bounds.max - DVec3::splat(32.0));
        if !(inner.contains_point(foot.min) && inner.contains_point(foot.max)) {
            out.push((Level::Warn, "Die verkleinerte Map ragt über den Skybox-Raum hinaus – Inhalte der Skybox stehen dann an falscher Stelle oder fehlen.".to_string()));
        }
        if world.intersects(&room.bounds) {
            out.push((Level::Warn, "Der Skybox-Raum überschneidet die spielbare Map.".to_string()));
        }
    }
    if room.bounds.min.min_element() < -limit || room.bounds.max.max_element() > limit {
        out.push((Level::Error, "Der Skybox-Raum liegt außerhalb der Map-Grenze.".to_string()));
    }
    // Nebel
    let fog_on = |e: &Entity| e.get("fogenable").is_some_and(|v| v.trim() == "1");
    if let Some(f) = map.entities.iter().find(|e| e.classname().eq_ignore_ascii_case("env_fog_controller")) {
        if fog_on(f) && !fog_on(cam) {
            out.push((Level::Warn, "Nebel ist in der Map an, in der Skybox (sky_camera) aus – der Horizont wirkt dann zu klar.".to_string()));
        } else if fog_on(f) && fog_on(cam) && f.get("fogcolor").map(str::trim) != cam.get("fogcolor").map(str::trim) {
            out.push((Level::Info, "Nebelfarbe von Map und Skybox unterscheiden sich.".to_string()));
        }
    }
    // Werkzeug-Brushes außer Skybox/nodraw haben im Skybox-Raum nichts zu suchen
    let odd = map
        .world
        .solids
        .iter()
        .filter(|s| room.bounds.contains_point(s.bounds.center()))
        .filter(|s| matches!(solid_tool_kind(s), Some(ToolKind::Trigger | ToolKind::Clip | ToolKind::Hint)))
        .count();
    if odd > 0 {
        out.push((Level::Info, format!("{odd} Trigger/Clip/Hint-Brushes im Skybox-Raum – dort wirkungslos.")));
    }
    out.sort_by_key(|x| x.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with_map() -> Editor {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.map_limit = 16384.0;
        // Boden 2048 × 2048 und ein Spawn
        let floor = ed.map.make_box(Aabb::new(DVec3::new(-1024.0, -1024.0, -16.0), DVec3::new(1024.0, 1024.0, 0.0)), "DEV/GRAYGRID", 0.25);
        ed.map.world.solids.push(floor.into());
        let id = ed.map.alloc_id();
        let mut sp = Entity::new(id, "info_player_start");
        sp.set("origin", "0 0 16");
        ed.map.entities.push(sp.into());
        ed
    }

    #[test]
    fn create_room_and_map_points() {
        let mut ed = editor_with_map();
        assert!(sky_room(&ed.map).is_none());
        let opts = CreateOpts { scale: 16.0, size: suggest_size(&ed.map, 16.0), ground: Some("DEV/GRAYGRID".into()), copy_fog: true };
        assert!(opts.size.x >= 1024.0 && opts.size.z >= 512.0);
        let room = create(&mut ed, &opts).unwrap();
        assert_eq!(room.scale, 16.0);
        // Raum liegt über der Map und überschneidet sie nicht
        assert!(room.bounds.min.z > 0.0);
        let foot = Aabb::new(room.to_sky(DVec3::new(-1024.0, -1024.0, -16.0)), room.to_sky(DVec3::new(1024.0, 1024.0, 0.0)));
        assert!(room.bounds.contains_point(foot.min) && room.bounds.contains_point(foot.max));
        // Hin- und Rückrechnung
        let p = DVec3::new(512.0, -256.0, 64.0);
        assert!((room.to_world(room.to_sky(p)) - p).length() < 1e-9);
        // geprüft: keine Fehler/Warnungen
        let issues = check(&ed.map, 16384.0);
        assert!(issues.iter().all(|i| i.0 == Level::Info), "{issues:?}");
        // zweites Anlegen wird abgelehnt
        assert!(create(&mut ed, &opts).is_err());
    }

    #[test]
    fn copy_shrinks_into_room() {
        let mut ed = editor_with_map();
        let opts = CreateOpts { scale: 16.0, size: DVec3::new(1024.0, 1024.0, 512.0), ground: None, copy_fog: false };
        let room = create(&mut ed, &opts).unwrap();
        let house = ed.map.make_box(Aabb::new(DVec3::new(256.0, 256.0, 0.0), DVec3::new(512.0, 512.0, 256.0)), "DEV/GRAYGRID", 0.25);
        let hid = house.id;
        ed.map.world.solids.push(house.into());
        ed.sel = [Obj::Solid(hid)].into_iter().collect();
        let (n, skipped) = copy_selection_to_sky(&mut ed).unwrap();
        assert_eq!((n, skipped), (1, 0));
        let copy = ed.sel.iter().next().copied().unwrap();
        let Obj::Solid(cid) = copy else { panic!() };
        let b = ed.map.solid(cid).unwrap().bounds;
        assert!((b.size() - DVec3::new(16.0, 16.0, 16.0)).length() < 1e-6, "{:?}", b.size());
        assert!((b.min - room.to_sky(DVec3::new(256.0, 256.0, 0.0))).length() < 1e-6);
        assert!(room.bounds.contains_point(b.center()));
        // Textur mitskaliert (1/16 der alten Skalierung)
        let side = &ed.map.solid(cid).unwrap().sides[0];
        assert!((side.uaxis.scale - 0.25 / 16.0).abs() < 1e-9, "{}", side.uaxis.scale);
    }

    #[test]
    fn open_room_and_fog_are_reported() {
        let mut ed = editor_with_map();
        let id = ed.map.alloc_id();
        let mut c = Entity::new(id, "sky_camera");
        c.set("origin", "0 0 4000");
        ed.map.entities.push(c.into());
        let issues = check(&ed.map, 16384.0);
        assert!(issues.iter().any(|i| i.0 == Level::Error), "{issues:?}");
    }

    #[test]
    fn view_filters() {
        use crate::editor::{tool_kind, AutoVis};
        assert_eq!(tool_kind("TOOLS/TOOLSNODRAW"), Some(ToolKind::Nodraw));
        assert_eq!(tool_kind("tools/toolsplayerclip"), Some(ToolKind::Clip));
        assert_eq!(tool_kind("TOOLS/TOOLSSKIP"), Some(ToolKind::Hint));
        assert_eq!(tool_kind("tools/toolsblocklight"), Some(ToolKind::Other));
        assert_eq!(tool_kind("DEV/GRAYGRID"), None);
        let mut ed = editor_with_map();
        // Trigger-Brush (alle Seiten toolstrigger), nodraw-Brush, Wand mit einer nodraw-Seite
        let trig = ed.map.make_box(Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 64.0)), "TOOLS/TOOLSTRIGGER", 0.25);
        let nod = ed.map.make_box(Aabb::new(DVec3::new(100.0, 0.0, 0.0), DVec3::new(164.0, 64.0, 64.0)), "TOOLS/TOOLSNODRAW", 0.25);
        let mut wall = ed.map.make_box(Aabb::new(DVec3::new(300.0, -64.0, 0.0), DVec3::new(316.0, 64.0, 128.0)), "DEV/GRAYGRID", 0.25);
        assert_eq!(solid_tool_kind(&trig), Some(ToolKind::Trigger));
        assert_eq!(solid_tool_kind(&nod), Some(ToolKind::Nodraw));
        assert_eq!(solid_tool_kind(&wall), None);
        // Seite, die zur Kamera (−X) zeigt, auf nodraw
        let k = wall.planes.iter().position(|p| p.is_some_and(|p| p.normal.x < -0.9)).unwrap();
        wall.sides[k].material = "TOOLS/TOOLSNODRAW".into();
        let (tid, nid, wid) = (trig.id, nod.id, wall.id);
        for s in [trig, nod, wall] {
            ed.map.world.solids.push(s.into());
        }
        let vis = |ed: &Editor, id: u32| ed.is_visible(Obj::Solid(id));
        assert!(vis(&ed, tid) && vis(&ed, nid) && vis(&ed, wid));
        ed.auto_vis.triggers = false;
        assert!(!vis(&ed, tid) && vis(&ed, nid));
        ed.auto_vis.nodraw = false;
        assert!(!vis(&ed, nid) && vis(&ed, wid), "Wand mit nodraw-Seite bleibt sichtbar");
        // Klick durch die ausgeblendete nodraw-Seite trifft die Rückseite der gegenüberliegenden Fläche
        let hit = ed.pick_3d(DVec3::new(200.0, 0.0, 64.0), DVec3::X).unwrap();
        assert_eq!(hit.0, Obj::Solid(wid));
        assert!((hit.2 - 116.0).abs() < 1e-6, "{}", hit.2);
        ed.auto_vis.nodraw = true;
        let hit = ed.pick_3d(DVec3::new(200.0, 0.0, 64.0), DVec3::X).unwrap();
        assert!((hit.2 - 100.0).abs() < 1e-6, "{}", hit.2);
        // Speichern in der VMF (viewsettings) und wieder laden
        ed.auto_vis = AutoVis::from_hidden_list("nodraw,triggers");
        assert_eq!(ed.auto_vis.hidden_list(), "triggers,nodraw");
        ed.map.view_filter = ed.auto_vis.hidden_list();
        let text = ed.map.to_vmf();
        assert!(text.contains("\"amboss_hidden\" \"triggers,nodraw\""));
        let back = Map::from_vmf(&text).unwrap();
        assert_eq!(AutoVis::from_hidden_list(&back.view_filter), ed.auto_vis);
        ed.map.view_filter.clear();
        assert!(!ed.map.to_vmf().contains("amboss_hidden"));
    }

    #[test]
    fn sky_mode_isolates_room() {
        let mut ed = editor_with_map();
        let opts = CreateOpts { scale: 16.0, size: DVec3::new(1024.0, 1024.0, 512.0), ground: None, copy_fog: false };
        let room = create(&mut ed, &opts).unwrap();
        let floor = ed.map.world.solids[0].id;
        let inside = ed.map.make_box(Aabb::new(room.origin, room.origin + DVec3::splat(16.0)), "DEV/GRAYGRID", 0.25);
        let iid = inside.id;
        ed.map.world.solids.push(inside.into());
        ed.touch();
        assert!(ed.is_visible(Obj::Solid(floor)) && ed.is_visible(Obj::Solid(iid)));
        // Filter „3D-Skybox-Raum“ blendet den Inhalt aus
        ed.auto_vis.sky_room = false;
        assert!(!ed.is_visible(Obj::Solid(iid)) && ed.is_visible(Obj::Solid(floor)));
        ed.auto_vis.sky_room = true;
        // Skybox-Modus: nur der Raum
        ed.sky_edit = Some(room.bounds);
        assert!(!ed.is_visible(Obj::Solid(floor)) && ed.is_visible(Obj::Solid(iid)));
        // fürs 3D-Netz (Spielersicht) bleibt alles sichtbar
        let fl = ed.map.solid(floor).unwrap();
        assert!(ed.is_visible_solid_all(None, fl));
        assert!(room.bounds.min.z > 100.0);
        assert!(ed.pick_3d(DVec3::new(0.0, 0.0, 100.0), -DVec3::Z).is_none(), "Map nicht anklickbar");
        ed.sky_edit = None;
        assert!(ed.pick_3d(DVec3::new(0.0, 0.0, 100.0), -DVec3::Z).is_some());
    }
}
