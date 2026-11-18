//! Spline-Werkzeug: Straßen, Rohre, Zäune, Objekte und Kabel entlang einer Kurve durch gesetzte Punkte.

use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use crate::map::{fmt_num, fmt_vec, Entity, Solid};
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    #[default]
    Road,
    Pipe,
    Fence,
    Models,
    Cable,
    /// Kopien der gemerkten Auswahl entlang der Kurve
    Selection,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Road, Kind::Pipe, Kind::Fence, Kind::Models, Kind::Selection, Kind::Cable];
    pub fn label(self) -> &'static str {
        match self {
            Kind::Road => crate::i18n::tr("Straße / Weg"),
            Kind::Pipe => crate::i18n::tr("Rohr"),
            Kind::Fence => crate::i18n::tr("Zaun / Geländer"),
            Kind::Models => crate::i18n::tr("Modelle entlang"),
            Kind::Cable => crate::i18n::tr("Kabel / Seil"),
            Kind::Selection => crate::i18n::tr("Auswahl entlang"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SplineOpts {
    pub kind: Kind,
    pub closed: bool,
    /// Kurve (Catmull-Rom) statt gerader Strecken
    pub smooth: bool,
    /// Länge eines Segments
    pub step: f64,
    pub material: String,
    pub detail: bool,
    // Straße
    pub width: f64,
    pub thickness: f64,
    pub curb: bool,
    pub curb_width: f64,
    pub curb_height: f64,
    // Rohr
    pub radius: f64,
    pub sides: usize,
    // Zaun / Modelle
    pub spacing: f64,
    pub height: f64,
    pub post: f64,
    pub model: String,
    pub offset: f64,
    pub both_sides: bool,
    pub align: bool,
    // Kabel
    pub slack: f64,
    pub cable_width: f64,
}

impl Default for SplineOpts {
    fn default() -> Self {
        SplineOpts {
            kind: Kind::Road,
            closed: false,
            smooth: true,
            step: 64.0,
            material: String::new(),
            detail: true,
            width: 192.0,
            thickness: 16.0,
            curb: true,
            curb_width: 16.0,
            curb_height: 8.0,
            radius: 16.0,
            sides: 12,
            spacing: 128.0,
            height: 48.0,
            post: 6.0,
            model: "models/props_c17/lamppost03a_off.mdl".into(),
            offset: 0.0,
            both_sides: false,
            align: true,
            slack: 60.0,
            cable_width: 2.0,
        }
    }
}

/// Zustand im Editor
#[derive(Clone, Debug, Default)]
pub struct SplineState {
    pub open: bool,
    /// Klicks in 2D/3D setzen Punkte
    pub placing: bool,
    pub points: Vec<DVec3>,
    pub opts: SplineOpts,
    /// zuletzt erzeugte Objekte (für „Neu erzeugen“)
    pub last: Vec<Obj>,
    /// Höhe über der angeklickten Fläche
    pub lift: f64,
    /// Vorlage für „Auswahl entlang“: Objekte und ihr Bezugspunkt (Mitte unten)
    pub source: Vec<Obj>,
    pub source_pivot: DVec3,
}

// ------------------------------------------------------------------ Kurve

fn catmull(p0: DVec3, p1: DVec3, p2: DVec3, p3: DVec3, t: f64) -> DVec3 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * ((2.0 * p1) + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

/// Dichte Polylinie der Kurve (etwa alle `res` Einheiten)
pub fn polyline(points: &[DVec3], closed: bool, smooth: bool, res: f64) -> Vec<DVec3> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let spans = if closed { n } else { n - 1 };
    let get = |i: i64| -> DVec3 {
        if closed {
            points[i.rem_euclid(n as i64) as usize]
        } else {
            points[i.clamp(0, n as i64 - 1) as usize]
        }
    };
    let mut out = Vec::new();
    for s in 0..spans as i64 {
        let (p0, p1, p2, p3) = (get(s - 1), get(s), get(s + 1), get(s + 2));
        let len = p1.distance(p2);
        let k = ((len / res.max(1.0)).ceil() as usize).max(1);
        for j in 0..k {
            let t = j as f64 / k as f64;
            out.push(if smooth { catmull(p0, p1, p2, p3, t) } else { p1.lerp(p2, t) });
        }
    }
    out.push(if closed { points[0] } else { points[n - 1] });
    out
}

fn length(poly: &[DVec3]) -> f64 {
    poly.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Punkte im Abstand `step` entlang der Polylinie (Anfang und Ende immer dabei)
fn resample(poly: &[DVec3], step: f64) -> Vec<DVec3> {
    let total = length(poly);
    if poly.len() < 2 || total < 1e-6 {
        return poly.to_vec();
    }
    let n = ((total / step.max(1.0)).round() as usize).max(1);
    (0..=n).map(|k| at_distance(poly, total * k as f64 / n as f64).0).collect()
}

/// Position und Richtung bei Bogenlänge `d`
fn at_distance(poly: &[DVec3], d: f64) -> (DVec3, DVec3) {
    let mut acc = 0.0;
    for w in poly.windows(2) {
        let l = w[0].distance(w[1]);
        if l > 1e-9 && acc + l >= d {
            let t = ((d - acc) / l).clamp(0.0, 1.0);
            return (w[0].lerp(w[1], t), (w[1] - w[0]) / l);
        }
        acc += l;
    }
    let k = poly.len();
    (poly[k - 1], (poly[k - 1] - poly[k - 2]).normalize_or(DVec3::X))
}

/// Richtung an Stützstelle i (Mittel der Nachbarsegmente)
fn tangent(s: &[DVec3], i: usize, closed: bool) -> DVec3 {
    let n = s.len();
    let a = if i > 0 { s[i - 1] } else if closed && n > 2 { s[n - 2] } else { s[i] };
    let b = if i + 1 < n { s[i + 1] } else if closed && n > 2 { s[1] } else { s[i] };
    (b - a).normalize_or(DVec3::X)
}

fn round(p: DVec3) -> DVec3 {
    (p * 4.0).round() / 4.0
}

fn right_of(t: DVec3) -> DVec3 {
    let h = DVec3::new(t.x, t.y, 0.0);
    h.normalize_or(DVec3::X).cross(DVec3::Z).normalize_or(DVec3::NEG_Y)
}

// ------------------------------------------------------------------ Erzeugen

/// Was erzeugt würde: Brushes (als Punktwolken) und Punkt-Entities
pub struct Plan {
    pub hulls: Vec<(Vec<DVec3>, bool)>,
    /// (Klasse, Position, Gierwinkel, Schlüssel)
    pub ents: Vec<(String, DVec3, f64, Vec<(String, String)>)>,
    /// (Brush-Index, Richtung, Bogenlänge am Anfang) für die Texturausrichtung der Straße
    pub tex: Vec<(usize, DVec3, f64)>,
    pub length: f64,
}

pub fn plan(points: &[DVec3], o: &SplineOpts) -> Plan {
    let poly = polyline(points, o.closed, o.smooth, 8.0);
    let total = length(&poly);
    let mut p = Plan { hulls: Vec::new(), ents: Vec::new(), tex: Vec::new(), length: total };
    if points.len() < 2 {
        return p;
    }
    match o.kind {
        Kind::Road => {
            let s = resample(&poly, o.step);
            let mut dist = 0.0;
            let sec = |i: usize, off_a: f64, off_b: f64, top: f64, bottom: f64| -> [DVec3; 4] {
                let r = right_of(tangent(&s, i, o.closed));
                let c = s[i];
                [c + r * off_a + DVec3::Z * top, c + r * off_b + DVec3::Z * top, c + r * off_a - DVec3::Z * bottom, c + r * off_b - DVec3::Z * bottom].map(round)
            };
            let hw = o.width / 2.0;
            for i in 0..s.len() - 1 {
                let seg_len = s[i].distance(s[i + 1]);
                if seg_len < 1.0 {
                    continue;
                }
                let dir = (s[i + 1] - s[i]) / seg_len;
                let mut pts: Vec<DVec3> = sec(i, -hw, hw, 0.0, o.thickness).to_vec();
                pts.extend(sec(i + 1, -hw, hw, 0.0, o.thickness));
                p.tex.push((p.hulls.len(), dir, dist));
                p.hulls.push((pts, false));
                if o.curb {
                    for (a, b) in [(-hw - o.curb_width, -hw), (hw, hw + o.curb_width)] {
                        let mut c: Vec<DVec3> = sec(i, a, b, o.curb_height, o.thickness).to_vec();
                        c.extend(sec(i + 1, a, b, o.curb_height, o.thickness));
                        p.hulls.push((c, false));
                    }
                }
                dist += seg_len;
            }
        }
        Kind::Pipe => {
            let s = resample(&poly, o.step);
            let n = o.sides.clamp(4, 32);
            let ring = |i: usize| -> Vec<DVec3> {
                let t = tangent(&s, i, o.closed);
                let r = t.cross(DVec3::Z).try_normalize().unwrap_or(DVec3::X);
                let u = r.cross(t).normalize();
                (0..n).map(|k| {
                    let a = k as f64 / n as f64 * std::f64::consts::TAU;
                    round(s[i] + (r * a.cos() + u * a.sin()) * o.radius)
                }).collect()
            };
            for i in 0..s.len() - 1 {
                if s[i].distance(s[i + 1]) < 1.0 {
                    continue;
                }
                let mut pts = ring(i);
                pts.extend(ring(i + 1));
                p.hulls.push((pts, true));
            }
        }
        Kind::Fence => {
            let n = ((total / o.spacing.max(8.0)).round() as usize).max(1);
            let posts: Vec<(DVec3, DVec3)> = (0..=n).map(|k| at_distance(&poly, total * k as f64 / n as f64)).collect();
            let hp = o.post / 2.0;
            for (c, _) in &posts {
                let c = round(*c);
                let b = Aabb::new(c - DVec3::new(hp, hp, 0.0), c + DVec3::new(hp, hp, o.height));
                p.hulls.push((b.corners().to_vec(), true));
            }
            for w in posts.windows(2) {
                let (a, b) = (w[0].0, w[1].0);
                let dir = (b - a).normalize_or(DVec3::X);
                let r = right_of(dir) * (o.post * 0.3).max(1.0);
                for hgt in [o.height * 0.4, o.height * 0.85] {
                    let rh = 3.0;
                    let pts: Vec<DVec3> = [a + dir * hp, b - dir * hp]
                        .iter()
                        .flat_map(|q| [*q + r + DVec3::Z * (hgt - rh), *q - r + DVec3::Z * (hgt - rh), *q + r + DVec3::Z * (hgt + rh), *q - r + DVec3::Z * (hgt + rh)])
                        .map(round)
                        .collect();
                    p.hulls.push((pts, true));
                }
            }
        }
        Kind::Models | Kind::Selection => {
            let n = ((total / o.spacing.max(8.0)).floor() as usize).max(1);
            let sides: &[f64] = if o.both_sides { &[1.0, -1.0] } else { &[1.0] };
            for k in 0..=n {
                if o.closed && k == n {
                    break;
                }
                let (c, t) = at_distance(&poly, (total * k as f64 / n as f64).min(total));
                for sgn in sides {
                    let r = right_of(t);
                    let pos = round(c + r * o.offset * sgn);
                    let mut yaw = if o.align { t.y.atan2(t.x).to_degrees() } else { 0.0 };
                    if *sgn < 0.0 && o.align {
                        yaw += 180.0;
                    }
                    let class = if o.kind == Kind::Selection { String::new() } else { "prop_static".into() };
                    p.ents.push((class, pos, yaw, vec![("model".into(), o.model.clone())]));
                }
            }
        }
        Kind::Cable => {
            // Kabel hängen zwischen den gesetzten Punkten (nicht entlang der geglätteten Kurve)
            for (i, pt) in points.iter().enumerate() {
                let class = if i == 0 { "move_rope" } else { "keyframe_rope" };
                let keys = vec![("Slack".to_string(), fmt_num(o.slack)), ("Width".to_string(), fmt_num(o.cable_width)), ("RopeMaterial".to_string(), "cable/cable.vmt".to_string())];
                p.ents.push((class.into(), round(*pt), 0.0, keys));
            }
        }
    }
    p
}

/// Linien für die Vorschau in 2D/3D: Kurve, Kontrollpunkte und Umrisse der Teile
pub fn preview_lines(st: &SplineState) -> Vec<(DVec3, DVec3, [f32; 4])> {
    let mut out = Vec::new();
    if !st.open || st.points.is_empty() {
        return out;
    }
    let curve = [0.25, 0.85, 1.0, 1.0];
    let ctrl = [1.0, 0.75, 0.2, 0.9];
    for w in st.points.windows(2) {
        out.push((w[0], w[1], [ctrl[0], ctrl[1], ctrl[2], 0.35]));
    }
    for p in &st.points {
        let s = 6.0;
        out.push((*p - DVec3::X * s, *p + DVec3::X * s, ctrl));
        out.push((*p - DVec3::Y * s, *p + DVec3::Y * s, ctrl));
        out.push((*p - DVec3::Z * s, *p + DVec3::Z * s, ctrl));
    }
    let poly = polyline(&st.points, st.opts.closed, st.opts.smooth, 8.0);
    for w in poly.windows(2) {
        out.push((w[0], w[1], curve));
    }
    let pl = plan(&st.points, &st.opts);
    let ghost = [0.6, 0.9, 1.0, 0.45];
    for (pts, _) in pl.hulls.iter().take(600) {
        // Umriss: Kanten der Hülle grob als Verbindungslinien der Querschnitte
        let h = pts.len() / 2;
        for k in 0..h {
            out.push((pts[k], pts[k + h], ghost));
        }
    }
    for (_, pos, yaw, _) in pl.ents.iter().take(400) {
        let f = DVec3::new(yaw.to_radians().cos(), yaw.to_radians().sin(), 0.0);
        out.push((*pos, *pos + DVec3::Z * 32.0, ghost));
        out.push((*pos, *pos + f * 16.0, ghost));
    }
    out
}

/// Erzeugen (ein Rückgängig-Schritt). `replace` löscht die zuletzt erzeugten Objekte.
pub fn build(ed: &mut Editor, replace: bool) -> Result<usize, String> {
    let st = ed.spline.clone();
    if st.points.len() < 2 {
        return Err("Mindestens zwei Punkte setzen".into());
    }
    let mut o = st.opts.clone();
    if o.material.trim().is_empty() {
        o.material = ed.material.clone();
    }
    let pl = plan(&st.points, &o);
    if pl.hulls.is_empty() && pl.ents.is_empty() {
        return Err("Nichts zu erzeugen – Punkte liegen zu dicht".into());
    }
    if o.kind == Kind::Selection && st.source.iter().all(|x| !ed.obj_exists(*x)) {
        return Err("Erst eine Vorlage übernehmen: Objekte auswählen und „Auswahl als Vorlage“ klicken".into());
    }
    ed.checkpoint();
    if replace {
        for obj in &st.last {
            match obj {
                Obj::Solid(id) => {
                    ed.map.take_solid(*id);
                }
                Obj::Entity(id) => ed.map.entities.retain(|e| e.id != *id),
            }
        }
    }
    let gid = ed.map.alloc_id();
    let mut made: Vec<Obj> = Vec::new();
    let mut detail_solids: Vec<Solid> = Vec::new();
    for (i, (pts, det)) in pl.hulls.iter().enumerate() {
        let Some(mut s) = ed.map.solid_from_points(pts, &o.material, ed.tex_scale) else { continue };
        // Straße: Textur auf der Oberseite läuft entlang der Kurve, nahtlos fortgesetzt
        if let Some((_, dir, dist)) = pl.tex.iter().find(|t| t.0 == i) {
            let right = right_of(*dir);
            for (k, side) in s.sides.iter_mut().enumerate() {
                if s.planes.get(k).copied().flatten().is_some_and(|p| p.normal.z > 0.5) {
                    let sc = ed.tex_scale.max(0.01);
                    side.uaxis.axis = *dir;
                    side.uaxis.scale = sc;
                    side.uaxis.shift = (dist - pts[0].dot(*dir)) / sc;
                    side.vaxis.axis = -right;
                    side.vaxis.scale = sc;
                    side.vaxis.shift = 0.0;
                }
            }
        }
        s.editor.groupid = Some(gid);
        if *det && o.detail || (!det && o.detail && o.kind == Kind::Road) {
            detail_solids.push(s);
        } else {
            made.push(Obj::Solid(s.id));
            ed.map.world.solids.push(s.into());
        }
    }
    if !detail_solids.is_empty() {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "func_detail");
        e.editor.groupid = Some(gid);
        e.editor.color = ed.fgd.resolve_color("func_detail").unwrap_or([80, 200, 255]);
        e.solids = detail_solids.into_iter().map(Into::into).collect();
        ed.map.entities.push(e.into());
        made.push(Obj::Entity(id));
    }
    // Auswahl entlang: Kopien der Vorlage, um ihre Mitte unten gedreht
    if o.kind == Kind::Selection {
        let mut items = Vec::new();
        for obj in &st.source {
            match obj {
                Obj::Solid(id) => {
                    if let Some(s) = ed.map.solid(*id) {
                        items.push(crate::editor::ClipItem::Solid(s.clone()));
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = ed.map.entity(*id) {
                        items.push(crate::editor::ClipItem::Entity(e.clone()));
                    }
                }
            }
        }
        let c = st.source_pivot;
        for (_, pos, yaw, _) in &pl.ents {
            let x = crate::transform::Xform::about(c, crate::transform::Xform::rotation(2, *yaw)).then(&crate::transform::Xform::translate(*pos - c));
            for obj in ed.insert_clip_items(items.clone(), &x) {
                match obj {
                    Obj::Solid(id) => {
                        if let Some(s) = ed.map.solid_mut(id) {
                            s.editor.groupid = Some(gid);
                        }
                    }
                    Obj::Entity(id) => {
                        if let Some(e) = ed.map.entity_mut(id) {
                            e.editor.groupid = Some(gid);
                        }
                    }
                }
                made.push(obj);
            }
        }
    }
    // Punkt-Entities; Kabel als Kette über NextKey
    let base = format!("spline{gid}");
    let n = pl.ents.len();
    for (k, (class, pos, yaw, keys)) in pl.ents.iter().enumerate().filter(|(_, e)| !e.0.is_empty()) {
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, class);
        e.set("origin", fmt_vec(*pos));
        e.set("angles", format!("0 {} 0", fmt_num((yaw * 10.0).round() / 10.0)));
        for (kk, vv) in keys {
            e.set(kk, vv.clone());
        }
        if o.kind == Kind::Cable {
            e.set("targetname", format!("{base}_{k}"));
            if k + 1 < n {
                e.set("NextKey", format!("{base}_{}", k + 1));
            } else if o.closed && n > 2 {
                e.set("NextKey", format!("{base}_0"));
            }
            e.set("angles", "0 0 0");
        }
        e.editor.groupid = Some(gid);
        ed.map.entities.push(e.into());
        made.push(Obj::Entity(id));
    }
    if made.len() > 1 {
        ed.map.groups.push(crate::map::Group { id: gid, editor: Default::default() });
    } else {
        // eine einzelne Sache braucht keine Gruppe
        for obj in &made {
            match obj {
                Obj::Solid(id) => {
                    if let Some(s) = ed.map.solid_mut(*id) {
                        s.editor.groupid = None;
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = ed.map.entity_mut(*id) {
                        e.editor.groupid = None;
                    }
                }
            }
        }
    }
    ed.sel = made.iter().copied().collect();
    ed.spline.last = made.clone();
    ed.status = format!("Spline: {} Objekte erzeugt ({} Einheiten lang)", made.len(), pl.length.round());
    ed.touch();
    Ok(made.len())
}

// ------------------------------------------------------------------ Fenster

impl crate::app::App {
    pub(crate) fn spline_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::i18n::tr;
        use crate::theme;
        use eframe::egui::{self, RichText};
        if !self.ed.spline.open {
            self.ed.spline.placing = false;
            return;
        }
        let th = theme::t();
        let mut open = true;
        let (mut build_now, mut rebuild, mut from_sel, mut take_source) = (false, false, false, false);
        let st = &mut self.ed.spline;
        let pl = plan(&st.points, &st.opts);
        let cur_mat = self.ed.material.clone();
        let d = theme::Dlg::new(tr("Spline-Werkzeug"), 560.0).subtitle(tr("Straßen, Rohre, Zäune, Modelle und Kabel entlang einer Kurve")).footer();
        let before = (st.points.clone(), st.opts.kind, st.opts.closed, st.opts.smooth);
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                let o = &mut st.opts;
                ui.horizontal_wrapped(|ui| {
                    for k in Kind::ALL {
                        if ui.selectable_label(o.kind == k, k.label()).clicked() {
                            o.kind = k;
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let label = if st.placing { tr("Punkte setzen: an (Rechtsklick beendet)") } else { tr("Punkte setzen") };
                    if ui.add(egui::Button::new(RichText::new(label).color(if st.placing { th.accent_fg } else { th.text })).fill(if st.placing { th.accent } else { th.raised })).on_hover_text(tr("Klick in der 3D-Ansicht setzt einen Punkt auf die Fläche darunter, in 2D in der Höhe des letzten Punkts")).clicked() {
                        st.placing = !st.placing;
                    }
                    if ui.add_enabled(!st.points.is_empty(), egui::Button::new(tr("Letzten entfernen"))).clicked() {
                        st.points.pop();
                    }
                    if ui.add_enabled(!st.points.is_empty(), egui::Button::new(tr("Alle löschen"))).clicked() {
                        st.points.clear();
                    }
                    if ui.button(tr("Aus Auswahl")).on_hover_text(tr("Mittelpunkte der ausgewählten Objekte als Punkte übernehmen (nach ID)")).clicked() {
                        from_sel = true;
                    }
                });
                ui.horizontal(|ui| {
                    theme::checkbox(ui, &mut o.smooth, tr("Glatte Kurve"));
                    theme::checkbox(ui, &mut o.closed, tr("Geschlossen"));
                    ui.label(tr("Höhe über Fläche"));
                    ui.add(theme::dv(egui::DragValue::new(&mut st.lift).range(-256.0..=256.0).speed(1.0)));
                });
                egui::ScrollArea::vertical().max_height(120.0).id_salt("spline_pts").show(ui, |ui| {
                    let mut remove = None;
                    for (i, p) in st.points.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}", i + 1)).font(theme::mono(11.0)).color(th.text3));
                            for a in 0..3 {
                                ui.add(theme::dv(egui::DragValue::new(&mut p[a]).speed(1.0)).prefix(["X ", "Y ", "Z "][a]));
                            }
                            if ui.small_button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        st.points.remove(i);
                    }
                });
                ui.separator();
                egui::Grid::new("spline_opts").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let num = |ui: &mut egui::Ui, label: &'static str, v: &mut f64, range: std::ops::RangeInclusive<f64>| {
                        ui.label(tr(label));
                        ui.add(theme::dv(egui::DragValue::new(v).range(range).speed(1.0)));
                        ui.end_row();
                    };
                    match o.kind {
                        Kind::Road => {
                            num(ui, "Breite", &mut o.width, 16.0..=4096.0);
                            num(ui, "Dicke", &mut o.thickness, 1.0..=512.0);
                            num(ui, "Segmentlänge", &mut o.step, 8.0..=1024.0);
                            ui.label(tr("Bordstein"));
                            theme::checkbox(ui, &mut o.curb, tr("beidseitig"));
                            ui.end_row();
                            if o.curb {
                                num(ui, "Bordstein-Breite", &mut o.curb_width, 1.0..=256.0);
                                num(ui, "Bordstein-Höhe", &mut o.curb_height, 1.0..=256.0);
                            }
                        }
                        Kind::Pipe => {
                            num(ui, "Radius", &mut o.radius, 1.0..=1024.0);
                            ui.label(tr("Seiten"));
                            ui.add(egui::DragValue::new(&mut o.sides).range(4..=32));
                            ui.end_row();
                            num(ui, "Segmentlänge", &mut o.step, 8.0..=1024.0);
                        }
                        Kind::Fence => {
                            num(ui, "Pfostenabstand", &mut o.spacing, 16.0..=2048.0);
                            num(ui, "Höhe", &mut o.height, 8.0..=1024.0);
                            num(ui, "Pfostendicke", &mut o.post, 1.0..=128.0);
                        }
                        Kind::Models | Kind::Selection => {
                            if o.kind == Kind::Models {
                                ui.label(tr("Modell"));
                                ui.add(egui::TextEdit::singleline(&mut o.model).font(theme::mono(11.5)).desired_width(300.0));
                                ui.end_row();
                            } else {
                                ui.label(tr("Vorlage"));
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Objekte", st.source.len()))).color(th.text2));
                                    if ui.button(tr("Auswahl als Vorlage")).on_hover_text(tr("Die gerade ausgewählten Objekte werden entlang der Kurve kopiert; Bezugspunkt ist ihre Mitte unten")).clicked() {
                                        take_source = true;
                                    }
                                });
                                ui.end_row();
                            }
                            num(ui, "Abstand", &mut o.spacing, 8.0..=8192.0);
                            num(ui, "Seitlich versetzt", &mut o.offset, -4096.0..=4096.0);
                            ui.label("");
                            ui.horizontal(|ui| {
                                theme::checkbox(ui, &mut o.both_sides, tr("beidseitig"));
                                theme::checkbox(ui, &mut o.align, tr("an der Kurve ausrichten"));
                            });
                            ui.end_row();
                        }
                        Kind::Cable => {
                            num(ui, "Durchhang (Slack)", &mut o.slack, 0.0..=1000.0);
                            num(ui, "Dicke", &mut o.cable_width, 0.5..=32.0);
                        }
                    }
                    if matches!(o.kind, Kind::Road | Kind::Pipe | Kind::Fence) {
                        ui.label(tr("Material"));
                        ui.add(egui::TextEdit::singleline(&mut o.material).font(theme::mono(11.5)).desired_width(300.0).hint_text(theme::hint(cur_mat.clone())));
                        ui.end_row();
                        ui.label("");
                        theme::checkbox(ui, &mut o.detail, tr("als func_detail (schont VVIS)"));
                        ui.end_row();
                    }
                });
                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Punkte · Länge {} Einheiten · {} Brushes, {} Entities", st.points.len(), crate::i18n::dec(fmt_num(pl.length.round())), pl.hulls.len(), pl.ents.len()))).size(11.5).color(th.text3));
            }
            theme::Part::Footer => {
                let ok = st.points.len() >= 2;
                if ui.add_enabled(ok, egui::Button::new(RichText::new(tr("Erzeugen")).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8)).clicked() {
                    build_now = true;
                }
                if ui.add_enabled(ok && !st.last.is_empty(), egui::Button::new(tr("Neu erzeugen (ersetzt das Letzte)")).corner_radius(8)).clicked() {
                    rebuild = true;
                }
            }
            _ => {}
        });
        if before != (st.points.clone(), st.opts.kind, st.opts.closed, st.opts.smooth) {
            self.ed.touch();
        }
        if take_source {
            let sel: Vec<Obj> = self.ed.sel.iter().copied().collect();
            if sel.is_empty() {
                self.ed.status = tr("Erst Objekte auswählen").into();
            } else {
                let b = self.ed.selection_bounds();
                self.ed.spline.source = sel;
                self.ed.spline.source_pivot = DVec3::new(b.center().x, b.center().y, b.min.z);
            }
        }
        if from_sel {
            let pts: Vec<DVec3> = self.ed.sel.iter().map(|o| self.ed.obj_bounds(*o).center()).collect();
            if pts.len() >= 2 {
                self.ed.spline.points = pts;
                self.ed.touch();
            } else {
                self.ed.status = tr("Mindestens zwei Objekte auswählen").into();
            }
        }
        if build_now || rebuild {
            match build(&mut self.ed, rebuild) {
                Ok(_) => self.ed.spline.placing = false,
                Err(e) => self.ed.status = crate::i18n::tr_dyn(&e),
            }
        }
        if !open {
            self.ed.spline.open = false;
            self.ed.spline.placing = false;
            self.ed.touch();
        }
    }
}

/// Punkt aus einem 3D-Klick (Fläche unter der Maus, sonst Ebene des letzten Punkts)
pub fn point_from_ray(ed: &Editor, o: DVec3, d: DVec3) -> Option<DVec3> {
    if let Some(h) = ed.pick_3d(o, d) {
        return Some(crate::geometry::snap_vec_grid(o + d * h.2, ed.grid.max(1.0)) + DVec3::Z * ed.spline.lift);
    }
    let z = ed.spline.points.last().map(|p| p.z).unwrap_or(0.0);
    if d.z.abs() < 1e-6 {
        return None;
    }
    let t = (z - o.z) / d.z;
    (t > 0.0).then(|| crate::geometry::snap_vec_grid(o + d * t, ed.grid.max(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed_with(points: Vec<DVec3>, kind: Kind) -> Editor {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.material = "DEV/GRAYGRID".into();
        ed.spline.points = points;
        ed.spline.opts.kind = kind;
        ed
    }

    #[test]
    fn curve_passes_points() {
        let pts = vec![DVec3::ZERO, DVec3::new(512.0, 0.0, 0.0), DVec3::new(512.0, 512.0, 64.0)];
        let poly = polyline(&pts, false, true, 8.0);
        for p in &pts {
            assert!(poly.iter().any(|q| q.distance(*p) < 1e-6), "Kurve läuft durch {p}");
        }
        let straight = polyline(&pts, false, false, 8.0);
        assert!((length(&straight) - (512.0 + (512.0f64.powi(2) + 64.0f64.powi(2)).sqrt())).abs() < 1e-6);
        let r = resample(&straight, 64.0);
        assert!(r.windows(2).all(|w| (w[0].distance(w[1]) - 64.0).abs() < 8.0));
    }

    #[test]
    fn road_builds_connected_brushes() {
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(1024.0, 0.0, 0.0), DVec3::new(1024.0, 1024.0, 0.0)], Kind::Road);
        ed.spline.opts.detail = false;
        let n = build(&mut ed, false).unwrap();
        assert!(n > 10);
        let solids: Vec<&Solid> = ed.map.world.solids.iter().map(|s| &**s).collect();
        // Fahrbahn: oben auf z = 0, unten -16; Bordsteine 8 hoch
        let b = solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
        assert!((b.max.z - 8.0).abs() < 1e-6 && (b.min.z + 16.0).abs() < 1e-6, "{b:?}");
        // Textur der Oberseite läuft entlang der Fahrtrichtung
        let first = solids.iter().find(|s| s.bounds.max.z.abs() < 1e-6).unwrap();
        let top = first.sides.iter().zip(&first.planes).find(|(_, p)| p.is_some_and(|p| p.normal.z > 0.5)).unwrap().0;
        assert!(top.uaxis.axis.x.abs() > 0.99, "{:?}", top.uaxis.axis);
        // eine Gruppe, ein Rückgängig-Schritt
        assert_eq!(ed.map.groups.len(), 1);
        ed.undo();
        assert!(ed.map.world.solids.is_empty());
    }

    #[test]
    fn rebuild_replaces_and_kinds() {
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(512.0, 0.0, 128.0)], Kind::Pipe);
        build(&mut ed, false).unwrap();
        let first: usize = ed.map.entities.iter().map(|e| e.solids.len()).sum();
        assert!(first >= 8, "Rohr als func_detail");
        ed.spline.opts.radius = 32.0;
        build(&mut ed, true).unwrap();
        let after: usize = ed.map.entities.iter().map(|e| e.solids.len()).sum();
        assert_eq!(after, first, "ersetzt statt verdoppelt");
        // Kabel: Kette über NextKey, nur an den Punkten
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(256.0, 0.0, 0.0), DVec3::new(512.0, 0.0, 0.0)], Kind::Cable);
        build(&mut ed, false).unwrap();
        let ropes: Vec<&Entity> = ed.map.entities.iter().map(|e| &**e).collect();
        assert_eq!(ropes.len(), 3);
        assert_eq!(ropes[0].classname(), "move_rope");
        assert_eq!(ropes[0].get("NextKey"), ropes[1].get("targetname"));
        assert!(ropes[2].get("NextKey").is_none());
        // Modelle: alle 128 Einheiten, ausgerichtet
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(0.0, 512.0, 0.0)], Kind::Models);
        ed.spline.opts.spacing = 128.0;
        build(&mut ed, false).unwrap();
        assert_eq!(ed.map.entities.len(), 5);
        assert_eq!(ed.map.entities[0].get("angles"), Some("0 90 0"));
        // Zaun: Pfosten + je zwei Latten
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(256.0, 0.0, 0.0)], Kind::Fence);
        ed.spline.opts.spacing = 128.0;
        let p = plan(&ed.spline.points, &ed.spline.opts);
        assert_eq!(p.hulls.len(), 3 + 2 * 2);
        assert!(build(&mut ed_with(vec![DVec3::ZERO], Kind::Road), false).is_err());
        // Auswahl entlang: Vorlage (ein Pfosten) wird kopiert und an der Kurve ausgerichtet
        let mut ed = ed_with(vec![DVec3::ZERO, DVec3::new(0.0, 512.0, 0.0)], Kind::Selection);
        assert!(build(&mut ed, false).is_err(), "ohne Vorlage");
        let post = ed.map.make_box(Aabb::new(DVec3::new(-8.0, -2.0, 0.0), DVec3::new(8.0, 2.0, 64.0)), "DEV/GRAYGRID", 0.25);
        let pid = post.id;
        ed.map.world.solids.push(post.into());
        ed.spline.source = vec![Obj::Solid(pid)];
        ed.spline.source_pivot = DVec3::ZERO;
        ed.spline.opts.spacing = 256.0;
        let n = build(&mut ed, false).unwrap();
        assert_eq!(n, 3);
        // entlang +Y gedreht: die lange Seite (16) liegt jetzt in Y-Richtung
        let copy = ed.spline.last.iter().find_map(|o| if let Obj::Solid(id) = o { ed.map.solid(*id) } else { None }).unwrap();
        assert!((copy.bounds.size().y - 16.0).abs() < 1e-6 && (copy.bounds.size().x - 4.0).abs() < 1e-6, "{:?}", copy.bounds.size());
        assert!(ed.map.solid(pid).is_some(), "Vorlage bleibt")
    }
}
