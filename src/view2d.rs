//! 2D-Ansichten (Top / Front / Side) wie in Hammer.

use crate::editor::{ClipLine, Editor, Obj, Tool, XMode};
use crate::geometry::{snap, Aabb, Axis2D};
use crate::theme;
use crate::map::{CameraDef, Map};
use crate::transform::Xform;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use glam::{DVec2, DVec3};

enum Drag {
    Pan,
    Move { start: DVec2, applied: DVec2, began: bool },
    Xform { hx: i32, hy: i32, base: Box<Map>, from: Aabb, start: DVec2, began: bool },
    Marquee { start: Pos2, vertex: bool },
    Block { start: DVec2 },
    Cordon { start: DVec2 },
    ClipNew { start: DVec2 },
    ClipEnd { first: bool },
    Vertex { start: DVec2, base: Box<Map>, base_sel: Vec<(u32, DVec3)>, began: bool },
    Camera { index: usize, look: bool },
    CameraNew { start: DVec2 },
    Measure { start: DVec2 },
}

pub struct View2D {
    pub axis: Axis2D,
    pub center: DVec2,
    pub zoom: f64,
    drag: Option<Drag>,
    /// Grafikkarten-Renderer: zeichnet die (nicht ausgewählten) Brush-Kanten aus einem Puffer
    pub gl: Option<std::sync::Arc<std::sync::Mutex<crate::view3d::Renderer>>>,
}

/// Farbe einer Brush-Kante in 2D (Theme, Visgroup, gesperrt, außerhalb Cordon)
fn solid_color(ed: &Editor, e: Option<&crate::map::Entity>, s: &crate::map::Solid) -> Color32 {
    let th = theme::t();
    let mut color = match e {
        None => th.brush,
        Some(e) if e.classname() == "func_detail" => theme::alpha(th.brush, 0.7),
        Some(_) => th.entity,
    };
    if ed.vis_colors {
        let vg = s.editor.visgroups.first().or_else(|| e.and_then(|e| e.editor.visgroups.first()));
        if let Some(v) = vg.and_then(|id| ed.map.visgroups.iter().find(|v| v.id == *id)) {
            color = c32(v.color);
        }
    }
    if ed.is_locked_solid(e, s) {
        color = color.gamma_multiply(0.45);
    }
    if !ed.in_cordon(&s.bounds) {
        color = color.gamma_multiply(0.3);
    }
    color
}

fn rgba(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0]
}

/// Ab so vielen ausgewählten Brushes zeichnet die Grafikkarte die Auswahl (statt einzelner Formen je Kante)
pub const SEL_GL_MIN: usize = 48;

/// Kanten der ausgewählten, sichtbaren Brushes in der Auswahlfarbe (jede Kante einmal)
pub fn build_sel_lines(ed: &Editor) -> Vec<f32> {
    let mut out = Vec::new();
    let c = rgba(theme::t().accent);
    let mut seen: std::collections::HashSet<([i64; 3], [i64; 3])> = std::collections::HashSet::new();
    let key = |p: DVec3| [(p.x * 8.0).round() as i64, (p.y * 8.0).round() as i64, (p.z * 8.0).round() as i64];
    for (owner, s) in ed.map.solids_owned() {
        if !ed.is_selected_solid(owner.map(|e| e.id), s.id) || !ed.is_visible_solid(owner, s) {
            continue;
        }
        if s.has_disp() {
            for g in s.disps.iter().flatten() {
                let r = g.res;
                for i in 0..r {
                    for j in 0..r {
                        let k = i * r + j;
                        if j + 1 < r {
                            crate::view3d::line_b(&mut out, g.pos[k], g.pos[k + 1], c, &s.bounds);
                        }
                        if i + 1 < r {
                            crate::view3d::line_b(&mut out, g.pos[k], g.pos[k + r], c, &s.bounds);
                        }
                    }
                }
            }
            continue;
        }
        // gemeinsame Kanten zweier Flächen nur einmal (beim Verschieben wird der Puffer nur versetzt, nicht neu gebaut)
        seen.clear();
        for poly in &s.polys {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (ka, kb) = (key(a), key(b));
                if seen.insert(if ka < kb { (ka, kb) } else { (kb, ka) }) {
                    crate::view3d::line_b(&mut out, a, b, c, &s.bounds);
                }
            }
        }
    }
    out
}

/// Linienpuffer für alle 2D-Ansichten: Kanten aller sichtbaren, nicht ausgewählten Brushes
/// (jede Kante einmal) und Modell-Boxen nicht ausgewählter Props, in Weltkoordinaten.
pub fn build_lines(ed: &Editor) -> Vec<f32> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<([i64; 3], [i64; 3])> = std::collections::HashSet::new();
    let key = |p: DVec3| [(p.x * 8.0).round() as i64, (p.y * 8.0).round() as i64, (p.z * 8.0).round() as i64];
    for (e, s) in ed.map.solids_owned() {
        let owner = e.map(|e| e.id);
        if !ed.is_visible_solid(e, s) || ed.is_selected_solid(owner, s.id) {
            continue;
        }
        let c = rgba(solid_color(ed, e, s));
        if s.has_disp() {
            for g in s.disps.iter().flatten() {
                let r = g.res;
                for i in 0..r {
                    for j in 0..r {
                        let k = i * r + j;
                        if j + 1 < r {
                            crate::view3d::line_b(&mut out, g.pos[k], g.pos[k + 1], c, &s.bounds);
                        }
                        if i + 1 < r {
                            crate::view3d::line_b(&mut out, g.pos[k], g.pos[k + r], c, &s.bounds);
                        }
                    }
                }
            }
            continue;
        }
        seen.clear();
        for poly in &s.polys {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                let (ka, kb) = (key(a), key(b));
                if seen.insert(if ka < kb { (ka, kb) } else { (kb, ka) }) {
                    crate::view3d::line_b(&mut out, a, b, c, &s.bounds);
                }
            }
        }
    }
    // Modell-Boxen (gedreht) bzw. Entity-Boxen der nicht ausgewählten Punkt-Entities
    let col = rgba(theme::t().entity);
    for e in &ed.map.entities {
        if e.is_brush_entity() || !ed.is_visible_entity(e) || ed.sel.contains(&Obj::Entity(e.id)) {
            continue;
        }
        let mut c = col;
        if !ed.in_cordon(&ed.entity_bounds(e)) {
            c[3] *= 0.3;
        }
        let eb = ed.entity_bounds(e);
        if ed.mesh_index(e.id).is_some() {
            for (a, b) in ed.mesh_edges(e.id) {
                crate::view3d::line_b(&mut out, a, b, c, &eb);
            }
            continue;
        }
        // Modell: Umriss (Rand- und Knickkanten) statt Box
        if let Some(edges) = ed.model_path(e).and_then(|m| ed.model_edges.get(&m)).filter(|v| !v.is_empty()) {
            let x = Editor::entity_xform(e);
            for [a, b] in edges.iter() {
                crate::view3d::line_b(&mut out, x.apply(*a), x.apply(*b), c, &eb);
            }
            continue;
        }
        let k = match ed.model_path(e).and_then(|m| ed.model_bounds.get(&m)).copied() {
            Some(mb) => {
                let x = Editor::entity_xform(e);
                mb.corners().map(|p| x.apply(p))
            }
            None => ed.entity_bounds(e).corners(),
        };
        for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
            crate::view3d::line_b(&mut out, k[i], k[j], c, &eb);
        }
    }
    out
}

const HANDLE: f32 = 4.0;
/// Toleranz (Einheiten), ab der ein Objekt beim Abdunkeln noch als „gleiche Ebene“ gilt
const DIM_TOL: f64 = 10.0;
/// Ab diesem Zoom zeichnet egui die Entities selbst (mit Füllung, Beschriftung, Richtungspfeil)
const GL_ENTITY_ZOOM: f64 = 1.0;

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn dashed_rect(p: &egui::Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(Shape::dashed_line(&pts, stroke, 6.0, 4.0));
}

impl View2D {
    pub fn new(axis: Axis2D) -> Self {
        View2D { axis, center: DVec2::new(0.0, if axis == Axis2D::Top { 0.0 } else { 128.0 }), zoom: 0.5, drag: None, gl: None }
    }

    fn to_screen(&self, rect: Rect, w: DVec2) -> Pos2 {
        let c = rect.center();
        Pos2::new(c.x + ((w.x - self.center.x) * self.zoom) as f32, c.y - ((w.y - self.center.y) * self.zoom) as f32)
    }

    fn to_world(&self, rect: Rect, p: Pos2) -> DVec2 {
        let c = rect.center();
        DVec2::new(self.center.x + (p.x - c.x) as f64 / self.zoom, self.center.y - (p.y - c.y) as f64 / self.zoom)
    }

    fn sp(&self, rect: Rect, p: DVec3) -> Pos2 {
        self.to_screen(rect, self.axis.project(p))
    }

    fn screen_rect(&self, rect: Rect, b: &Aabb) -> Rect {
        Rect::from_two_pos(self.sp(rect, b.min), self.sp(rect, b.max))
    }

    /// Zentriert die Ansicht auf eine Bounding Box
    pub fn focus(&mut self, b: &Aabb, rect_size: Vec2) {
        if b.is_empty() {
            return;
        }
        let mn = self.axis.project(b.min);
        let mx = self.axis.project(b.max);
        self.center = (mn + mx) * 0.5;
        let size = (mx - mn).max(DVec2::splat(64.0));
        self.zoom = ((rect_size.x as f64 / size.x).min(rect_size.y as f64 / size.y) * 0.8).clamp(0.01, 16.0);
    }

    pub fn center_on(&mut self, p: DVec3) {
        self.center = self.axis.project(p);
    }

    fn handle_positions(&self, sr: Rect, xmode: XMode) -> Vec<(i32, i32, Pos2)> {
        let mut out = Vec::new();
        for hx in -1..=1 {
            for hy in -1..=1 {
                if hx == 0 && hy == 0 {
                    continue;
                }
                let corner = hx != 0 && hy != 0;
                match xmode {
                    XMode::Rotate if !corner => continue,
                    XMode::Skew if corner => continue,
                    _ => {}
                }
                let x = match hx {
                    -1 => sr.left() - 6.0,
                    1 => sr.right() + 6.0,
                    _ => sr.center().x,
                };
                let y = match hy {
                    1 => sr.top() - 6.0,
                    -1 => sr.bottom() + 6.0,
                    _ => sr.center().y,
                };
                out.push((hx, hy, Pos2::new(x, y)));
            }
        }
        out
    }

    fn to3(&self, w: DVec2, depth_val: f64) -> DVec3 {
        let (h, v, d) = self.axis.axes();
        let mut p = DVec3::ZERO;
        p[h] = w.x;
        p[v] = w.y;
        p[d] = depth_val;
        p
    }

    /// Zeichnet und verarbeitet Eingaben. Liefert die Weltposition des Mauszeigers.
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, ed: &mut Editor, maximize_toggle: &mut bool) -> Option<DVec2> {
        let id = ui.id().with(("view2d", self.axis as u8));
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let (h, v, depth) = self.axis.axes();
        let grid = ed.snap_grid();
        let typing = ui.ctx().wants_keyboard_input();
        let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));

        // ---------------------------------------------------- Zoom / Pan
        let hover = resp.hover_pos();
        if let Some(mp) = hover {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom_at(rect, mp, (scroll as f64 * 0.004).exp());
            }
        }
        let space = ui.input(|i| i.key_down(Key::Space)) && !typing;
        if resp.drag_started_by(PointerButton::Middle) || (space && resp.drag_started_by(PointerButton::Primary)) {
            self.drag = Some(Drag::Pan);
        }
        if let Some(Drag::Pan) = self.drag {
            let d = resp.drag_delta();
            self.center.x -= d.x as f64 / self.zoom;
            self.center.y += d.y as f64 / self.zoom;
        }
        if resp.double_clicked_by(PointerButton::Middle) {
            *maximize_toggle = true;
        }

        let mouse_world = hover.map(|p| self.to_world(rect, p));
        // Strg+R: Auswahl dreht sich um die Blickachse dieser Ansicht zur Maus
        if let Some(c) = ed.rot_live.as_ref().map(|r| r.center) {
            let cc = self.axis.project(c);
            if let Some(w) = mouse_world {
                // Seitenansicht (X/Z) schaut entlang +Y: dort zählt der Winkel andersherum
                let sign = if self.axis == Axis2D::Side { -1.0 } else { 1.0 };
                ed.rot_live_update(depth, (w.y - cc.y).atan2(w.x - cc.x).to_degrees() * sign, shift);
            }
            if resp.clicked() {
                ed.rot_live_commit();
            } else if resp.clicked_by(PointerButton::Secondary) {
                ed.rot_live_cancel();
            }
            self.draw(&painter, rect, ed);
            if let (Some(p), Some(r)) = (hover, ed.rot_live.as_ref()) {
                let th = theme::t();
                let c0 = self.sp(rect, c);
                painter.line_segment([c0, p], Stroke::new(1.5f32, th.accent));
                painter.circle_stroke(c0, 5.0, Stroke::new(1.5f32, th.accent));
                painter.text(p + Vec2::new(12.0, -12.0), Align2::LEFT_BOTTOM, format!("{:+.0}°", r.deg), FontId::monospace(12.0), th.accent_text);
            }
            painter.text(rect.center_top() + Vec2::new(0.0, 44.0), Align2::CENTER_TOP, crate::i18n::tr("Drehen: Linksklick übernimmt · Rechtsklick/Esc bricht ab · Shift = frei"), FontId::proportional(13.0), theme::t().accent_text);
            ui.ctx().request_repaint();
            return mouse_world;
        }
        let snapped = |w: DVec2| DVec2::new(snap(w.x, grid), snap(w.y, grid));
        let tol = HANDLE as f64 / self.zoom;

        let sel_bounds = ed.selection_bounds();
        let sel_rect = (!sel_bounds.is_empty()).then(|| self.screen_rect(rect, &sel_bounds));

        // ---------------------------------------------------- Drag-Start
        if resp.drag_started_by(PointerButton::Primary) && !space {
            let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            let wpress = self.to_world(rect, press);
            match ed.tool {
                Tool::Select | Tool::Face | Tool::Disp | Tool::Decal | Tool::Overlay => {
                    let mut handled = false;
                    if let Some(sr) = sel_rect {
                        for (hx, hy, hp) in self.handle_positions(sr, ed.xmode) {
                            if hp.distance(press) <= HANDLE + 3.0 {
                                self.drag = Some(Drag::Xform { hx, hy, base: Box::new(ed.map.clone()), from: sel_bounds, start: wpress, began: false });
                                handled = true;
                                break;
                            }
                        }
                    }
                    if !handled {
                        let hits = ed.pick_2d(self.axis, wpress, tol);
                        let hit_sel = hits.iter().any(|o| ed.sel.contains(o));
                        if !hit_sel {
                            if let Some(o) = hits.first() {
                                ed.select(Some(*o), ctrl);
                            }
                        }
                        if !hits.is_empty() {
                            // Shift+Ziehen dupliziert die Auswahl (wie in Hammer/TrenchBroom)
                            let began = shift && ed.duplicate_in_place() > 0;
                            self.drag = Some(Drag::Move { start: wpress, applied: DVec2::ZERO, began });
                        } else {
                            self.drag = Some(Drag::Marquee { start: press, vertex: false });
                        }
                    }
                }
                Tool::Block => self.drag = Some(Drag::Block { start: snapped(wpress) }),
                Tool::Cordon => self.drag = Some(Drag::Cordon { start: snapped(wpress) }),
                Tool::Clip => {
                    let mut grabbed = false;
                    if let Some(c) = ed.clip.filter(|c| c.axis == self.axis) {
                        for (first, p) in [(true, c.a), (false, c.b)] {
                            if self.to_screen(rect, p).distance(press) < 8.0 {
                                self.drag = Some(Drag::ClipEnd { first });
                                grabbed = true;
                            }
                        }
                    }
                    if !grabbed {
                        self.drag = Some(Drag::ClipNew { start: snapped(wpress) });
                    }
                }
                Tool::Vertex => {
                    let hit = self.vertex_at(rect, ed, press);
                    if !hit.is_empty() {
                        if !hit.iter().any(|h| ed.vsel.iter().any(|s| s.0 == h.0 && s.1.distance(h.1) < 0.01)) {
                            if !ctrl {
                                ed.vsel.clear();
                            }
                            ed.vsel.extend(hit);
                        }
                        self.drag = Some(Drag::Vertex { start: wpress, base: Box::new(ed.map.clone()), base_sel: ed.vsel.clone(), began: false });
                    } else if let Some((sid, _, a, b)) = self.edge_at(rect, ed, press) {
                        ed.vsel = vec![(sid, a), (sid, b)];
                        self.drag = Some(Drag::Vertex { start: wpress, base: Box::new(ed.map.clone()), base_sel: ed.vsel.clone(), began: false });
                    } else {
                        self.drag = Some(Drag::Marquee { start: press, vertex: true });
                    }
                }
                Tool::Camera => {
                    let mut found = None;
                    for (i, c) in ed.map.cameras.iter().enumerate() {
                        if self.sp(rect, c.pos).distance(press) < 8.0 {
                            found = Some((i, false));
                        } else if self.sp(rect, c.look).distance(press) < 8.0 {
                            found = Some((i, true));
                        }
                    }
                    self.drag = Some(match found {
                        Some((index, look)) => {
                            ed.map.active_camera = index;
                            Drag::Camera { index, look }
                        }
                        None => Drag::CameraNew { start: wpress },
                    });
                }
                Tool::Measure => self.drag = Some(Drag::Measure { start: snapped(wpress) }),
                Tool::Entity | Tool::Magnify => {}
            }
        }

        // ---------------------------------------------------- Drag läuft
        let pointer_world = ui.input(|i| i.pointer.interact_pos()).map(|p| self.to_world(rect, p));
        if let (Some(drag), Some(mw)) = (&mut self.drag, pointer_world) {
            match drag {
                Drag::Move { start, applied, began } => {
                    // Die Ecke der Auswahl-Box rastet am Raster ein (wie in Hammer) – nicht nur die
                    // Strecke, sonst bleibt ein Objekt neben dem Raster um denselben Versatz daneben
                    let b = ed.selection_bounds();
                    let (o0, o1) = if b.min.x <= b.max.x { (b.min[h] - applied.x, b.min[v] - applied.y) } else { (0.0, 0.0) };
                    let mut total = DVec2::new(snap(o0 + mw.x - start.x, grid) - o0, snap(o1 + mw.y - start.y, grid) - o1);
                    if ed.vertex_snap {
                        let (pts, ids) = ed.selection_points();
                        let moving: Vec<DVec3> = pts
                            .iter()
                            .map(|p| {
                                let mut q = *p;
                                q[h] -= applied.x;
                                q[v] -= applied.y;
                                q
                            })
                            .collect();
                        if let Some(d) = ed.vertex_snap_delta((h, v), &moving, mw - *start, 10.0 / self.zoom, &ids) {
                            total = d;
                        }
                    }
                    let d = total - *applied;
                    if d != DVec2::ZERO {
                        if !*began {
                            ed.checkpoint();
                            *began = true;
                        }
                        let mut d3 = DVec3::ZERO;
                        d3[h] = d.x;
                        d3[v] = d.y;
                        ed.translate_selection(d3);
                        *applied = total;
                    }
                }
                Drag::Xform { hx, hy, base, from, start, began } => {
                    let x = match ed.xmode {
                        XMode::Scale => {
                            let mut to = *from;
                            let m = snapped(mw);
                            if *hx < 0 {
                                to.min[h] = m.x.min(from.max[h] - grid);
                            } else if *hx > 0 {
                                to.max[h] = m.x.max(from.min[h] + grid);
                            }
                            if *hy < 0 {
                                to.min[v] = m.y.min(from.max[v] - grid);
                            } else if *hy > 0 {
                                to.max[v] = m.y.max(from.min[v] + grid);
                            }
                            (to != *from).then(|| (Editor::scale_xform(*from, to), true))
                        }
                        XMode::Rotate => {
                            let c3 = crate::geometry::snap_vec_grid(from.center(), grid.min(8.0));
                            let c = self.axis.project(c3);
                            let a0 = (start.y - c.y).atan2(start.x - c.x);
                            let a1 = (mw.y - c.y).atan2(mw.x - c.x);
                            let mut deg = (a1 - a0).to_degrees();
                            if !shift {
                                deg = (deg / 15.0).round() * 15.0;
                            }
                            let sign = if self.axis == Axis2D::Side { -1.0 } else { 1.0 };
                            (deg.abs() > 1e-6).then(|| (Xform::about(c3, Xform::rotation(depth, deg * sign)), false))
                        }
                        XMode::Skew => {
                            let mut pivot = DVec3::ZERO;
                            if *hx == 0 {
                                // obere/untere Kante: entlang h verschieben
                                let edge = if *hy > 0 { from.max[v] } else { from.min[v] };
                                let piv = if *hy > 0 { from.min[v] } else { from.max[v] };
                                let d = snap(mw.x - start.x, grid);
                                pivot[v] = piv;
                                (d != 0.0).then(|| (Xform::about(pivot, Xform::shear(h, v, d / (edge - piv))), false))
                            } else {
                                let edge = if *hx > 0 { from.max[h] } else { from.min[h] };
                                let piv = if *hx > 0 { from.min[h] } else { from.max[h] };
                                let d = snap(mw.y - start.y, grid);
                                pivot[h] = piv;
                                (d != 0.0).then(|| (Xform::about(pivot, Xform::shear(v, h, d / (edge - piv))), false))
                            }
                        }
                    };
                    if let Some((x, scale)) = x {
                        if !*began {
                            ed.checkpoint();
                            *began = true;
                        }
                        ed.transform_selection_from(base, &x, scale);
                    }
                }
                Drag::ClipNew { start } => {
                    let e = snapped(mw);
                    if e != *start {
                        ed.clip = Some(ClipLine { axis: self.axis, a: *start, b: e });
                        ed.clip3.clear();
                    }
                }
                Drag::ClipEnd { first } => {
                    if let Some(c) = ed.clip.as_mut() {
                        if *first {
                            c.a = snapped(mw);
                        } else {
                            c.b = snapped(mw);
                        }
                        ed.touch();
                    }
                }
                Drag::Vertex { start, base, base_sel, began } => {
                    let mut d = DVec2::new(snap(mw.x - start.x, grid), snap(mw.y - start.y, grid));
                    if ed.vertex_snap {
                        let moving: Vec<DVec3> = base_sel.iter().map(|(_, p)| *p).collect();
                        let ids: std::collections::BTreeSet<u32> = base_sel.iter().map(|(id, _)| *id).collect();
                        if let Some(sd) = ed.vertex_snap_delta((h, v), &moving, mw - *start, 10.0 / self.zoom, &ids) {
                            d = sd;
                        }
                    }
                    let mut d3 = DVec3::ZERO;
                    d3[h] = d.x;
                    d3[v] = d.y;
                    if d != DVec2::ZERO || *began {
                        if !*began {
                            ed.checkpoint();
                            *began = true;
                        }
                        if !ed.move_vertices_from(base, base_sel, d3) {
                            ed.status = crate::i18n::tr("Ungültige Form – Eckpunkt hier nicht möglich").into();
                        } else {
                            ed.status = format!("Eckpunkte verschoben um {} {} {}", d3.x, d3.y, d3.z);
                        }
                    }
                }
                Drag::Measure { start } => {
                    let d = ed.work_point[depth];
                    let to3 = |w: DVec2| {
                        let mut p = DVec3::ZERO;
                        p[h] = w.x;
                        p[v] = w.y;
                        p[depth] = d;
                        p
                    };
                    let (a, b) = (to3(*start), to3(snapped(mw)));
                    ed.measure = Some((a, b));
                    ed.status = format!("Abstand: {:.1} Einheiten", a.distance(b));
                }
                Drag::Camera { index, look } => {
                    if let Some(c) = ed.map.cameras.get_mut(*index) {
                        let p = if *look { &mut c.look } else { &mut c.pos };
                        p[h] = mw.x;
                        p[v] = mw.y;
                        ed.camera_changed = true;
                    }
                }
                _ => {}
            }
        }

        // ---------------------------------------------------- Drag-Ende
        if resp.drag_stopped() {
            if let Some(drag) = self.drag.take() {
                let end = ui.input(|i| i.pointer.interact_pos()).unwrap_or(rect.center());
                let wend = self.to_world(rect, end);
                match drag {
                    Drag::Marquee { start, vertex } => {
                        let r = Rect::from_two_pos(start, end);
                        if vertex {
                            if !ctrl {
                                ed.vsel.clear();
                            }
                            for (sid, p) in ed.vertex_handles() {
                                if r.contains(self.sp(rect, p)) {
                                    ed.vsel.push((sid, p));
                                }
                            }
                        } else {
                            if !ctrl {
                                ed.sel.clear();
                            }
                            // Alt: auch nur berührte Objekte auswählen
                            let touching = ui.input(|i| i.modifiers.alt);
                            for (o, b) in ed.selectable() {
                                let sr = self.screen_rect(rect, &b);
                                if !b.is_empty() && (r.contains_rect(sr) || (touching && r.intersects(sr))) {
                                    for x in ed.expand(o) {
                                        ed.sel.insert(x);
                                    }
                                }
                            }
                        }
                        ed.touch();
                    }
                    Drag::Block { start } => {
                        let e = snapped(wend);
                        if (e.x - start.x).abs() >= grid && (e.y - start.y).abs() >= grid {
                            let (a, b) = self.box_from(start, e, ed.last_box.min[depth], ed.last_box.max[depth]);
                            ed.create_block(Aabb::new(a, b));
                        }
                    }
                    Drag::Cordon { start } => {
                        let e = snapped(wend);
                        if (e.x - start.x).abs() >= grid && (e.y - start.y).abs() >= grid {
                            let (dmin, dmax) = match ed.map.cordon {
                                Some(c) => (c.bounds.min[depth], c.bounds.max[depth]),
                                None => (-1024.0, 1024.0),
                            };
                            let (a, b) = self.box_from(start, e, dmin, dmax);
                            ed.set_cordon(Aabb::new(a, b));
                        }
                    }
                    Drag::CameraNew { start } => {
                        let d = ed.map.cameras.get(ed.map.active_camera).map(|c| c.pos[depth]).unwrap_or(128.0);
                        if start.distance(wend) > 4.0 {
                            ed.add_camera(self.to3(start, d), self.to3(wend, d));
                        }
                    }
                    Drag::Vertex { began: true, .. } => {
                        // ungültige Endstellung zurücksetzen erfolgt bereits beim Ziehen
                    }
                    _ => {}
                }
            }
        }

        // ---------------------------------------------------- Prefabs (Drag & Drop / Klick)
        let dropped = resp.dnd_release_payload::<crate::prefabs::PrefabDrag>().map(|p| p.0);
        let armed_click = ed.armed_prefab.filter(|_| resp.clicked_by(PointerButton::Primary));
        if let (Some(idx), Some(pp)) = (dropped.or(armed_click), ui.input(|i| i.pointer.interact_pos())) {
            let s = snapped(self.to_world(rect, pp));
            let mut pos = ed.work_point;
            pos[h] = s.x;
            pos[v] = s.y;
            let mut normal = None;
            if self.axis == Axis2D::Top {
                if let Some((_, _, t)) = ed.pick_3d(DVec3::new(pos.x, pos.y, 65536.0), -DVec3::Z) {
                    pos.z = 65536.0 - t;
                    normal = Some(DVec3::Z);
                }
            }
            ed.prefab_drop = Some((idx, pos, normal));
        }
        // ---------------------------------------------------- Spline-Punkte
        let spline_click = ed.spline.placing && ed.spline.open;
        if spline_click {
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(mw) = mouse_world {
                    let s = snapped(mw);
                    let mut p = ed.spline.points.last().copied().unwrap_or(ed.work_point);
                    p[h] = s.x;
                    p[v] = s.y;
                    ed.spline.points.push(p);
                    ed.touch();
                }
            } else if resp.clicked_by(PointerButton::Secondary) {
                ed.spline.placing = false;
            }
        }
        // ---------------------------------------------------- Klicks
        if !spline_click && resp.clicked_by(PointerButton::Primary) && armed_click.is_none() {
            if let Some(mw) = mouse_world {
                let s = snapped(mw);
                ed.work_point[h] = s.x;
                ed.work_point[v] = s.y;
                match ed.tool {
                    Tool::Magnify => self.zoom_at(rect, hover.unwrap(), 2.0),
                    Tool::Measure => {}
                    Tool::Select | Tool::Face | Tool::Block | Tool::Disp | Tool::Clip | Tool::Cordon | Tool::Camera | Tool::Overlay => {
                        let hits = ed.pick_2d(self.axis, mw, tol);
                        let first = hits.first().copied();
                        if ed.pick_target.is_some() {
                            ed.select(first, false);
                        } else if first.is_some_and(|o| ed.sel.contains(&o)) && !ctrl && ed.tool == Tool::Select {
                            ed.xmode = ed.xmode.next();
                            ed.status = format!("Griffe: {}", ed.xmode.label());
                        } else if first.is_some() || !ctrl {
                            ed.select(first, ctrl);
                            ed.xmode = XMode::Scale;
                        }
                    }
                    Tool::Vertex => {
                        let hit = self.vertex_at(rect, ed, hover.unwrap());
                        if hit.is_empty() {
                            // Brush wählen, dessen Eckpunkte bearbeitet werden sollen
                            let hits = ed.pick_2d(self.axis, mw, tol);
                            if !ctrl {
                                ed.vsel.clear();
                            }
                            if let Some(o) = hits.first() {
                                ed.select(Some(*o), ctrl);
                            }
                        } else {
                            if !ctrl {
                                ed.vsel.clear();
                            }
                            ed.vsel.extend(hit);
                        }
                        ed.touch();
                    }
                    Tool::Entity | Tool::Decal => {
                        let mut pos = ed.work_point;
                        pos[h] = s.x;
                        pos[v] = s.y;
                        if self.axis == Axis2D::Top {
                            let size = ed.point_size(&ed.entity_class);
                            if let Some((_, _, t)) = ed.pick_3d(DVec3::new(pos.x, pos.y, 65536.0), -DVec3::Z) {
                                pos.z = 65536.0 - t - if ed.tool == Tool::Entity { size.0.z } else { -1.0 };
                            }
                        }
                        if ed.tool == Tool::Entity {
                            ed.create_point_entity(pos);
                        } else {
                            ed.create_decal(pos);
                        }
                    }
                }
            }
        }
        if resp.clicked_by(PointerButton::Secondary) && ed.tool == Tool::Magnify {
            if let Some(mp) = hover {
                self.zoom_at(rect, mp, 0.5);
            }
        }

        // ---------------------------------------------------- Tastatur (nur wenn Maus über Ansicht)
        if hover.is_some() && !typing && (!ed.sel.is_empty() || !ed.vsel.is_empty()) {
            let mut d = DVec2::ZERO;
            ui.input(|i| {
                if i.key_pressed(Key::ArrowLeft) {
                    d.x -= grid;
                }
                if i.key_pressed(Key::ArrowRight) {
                    d.x += grid;
                }
                if i.key_pressed(Key::ArrowUp) {
                    d.y += grid;
                }
                if i.key_pressed(Key::ArrowDown) {
                    d.y -= grid;
                }
            });
            if d != DVec2::ZERO {
                let mut d3 = DVec3::ZERO;
                d3[h] = d.x;
                d3[v] = d.y;
                if ed.tool == Tool::Vertex && !ed.vsel.is_empty() {
                    let base = ed.map.clone();
                    let sel = ed.vsel.clone();
                    ed.checkpoint();
                    if !ed.move_vertices_from(&base, &sel, d3) {
                        ed.undo();
                    }
                } else {
                    ed.checkpoint();
                    ed.translate_selection(d3);
                }
            }
        }

        // ---------------------------------------------------- Zeichnen
        self.draw(&painter, rect, ed);

        // Vorschau der laufenden Aktion
        if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
            match &self.drag {
                Some(Drag::Marquee { start, .. }) => {
                    let r = Rect::from_two_pos(*start, pp);
                    let th = theme::t();
                    painter.rect_filled(r, CornerRadius::ZERO, theme::alpha(th.accent, 0.08));
                    painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                }
                Some(Drag::Block { start }) | Some(Drag::Cordon { start }) => {
                    let e = snapped(self.to_world(rect, pp));
                    let r = Rect::from_two_pos(self.to_screen(rect, *start), self.to_screen(rect, e));
                    let col = if matches!(self.drag, Some(Drag::Cordon { .. })) { theme::t().err } else { theme::t().accent };
                    painter.rect_filled(r, CornerRadius::ZERO, col.gamma_multiply(0.08));
                    painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0f32, col), StrokeKind::Inside);
                    painter.text(r.center_top() - Vec2::new(0.0, 4.0), Align2::CENTER_BOTTOM, format!("{} × {}", (e.x - start.x).abs(), (e.y - start.y).abs()), FontId::monospace(11.0), col);
                }
                Some(Drag::CameraNew { start }) => {
                    painter.arrow(self.to_screen(rect, *start), pp - self.to_screen(rect, *start), Stroke::new(1.5f32, Color32::LIGHT_BLUE));
                }
                _ => {}
            }
        }

        // Hinweise
        let th = theme::t();
        if ed.armed_prefab.is_some() {
            painter.text(rect.center_top() + Vec2::new(0.0, 44.0), Align2::CENTER_TOP, "Prefab: Klick setzt es hier ab (Esc bricht ab)", FontId::proportional(13.0), th.accent_text);
        }
        if ed.pick_target.is_some() {
            painter.text(rect.center_top() + Vec2::new(0.0, 44.0), Align2::CENTER_TOP, "Pipette: Ziel-Entity anklicken (Esc bricht ab)", FontId::proportional(13.0), th.accent_text);
        }
        let _ = resp.hovered();
        if self.drag.is_some() {
            ui.ctx().request_repaint();
        }
        mouse_world
    }

    fn zoom_at(&mut self, rect: Rect, mp: Pos2, factor: f64) {
        let before = self.to_world(rect, mp);
        self.zoom = (self.zoom * factor).clamp(0.005, 32.0);
        let after = self.to_world(rect, mp);
        self.center += before - after;
    }

    fn box_from(&self, a2: DVec2, b2: DVec2, dmin: f64, dmax: f64) -> (DVec3, DVec3) {
        let (h, v, d) = self.axis.axes();
        let mut a = DVec3::ZERO;
        let mut b = DVec3::ZERO;
        a[h] = a2.x.min(b2.x);
        b[h] = a2.x.max(b2.x);
        a[v] = a2.y.min(b2.y);
        b[v] = a2.y.max(b2.y);
        a[d] = dmin;
        b[d] = if dmax - dmin < 1.0 { dmin + 64.0 } else { dmax };
        (a, b)
    }

    /// Alle Eckpunkte, die an dieser Bildschirmstelle liegen
    fn vertex_at(&self, rect: Rect, ed: &Editor, p: Pos2) -> Vec<(u32, DVec3)> {
        ed.vertex_handles().into_iter().filter(|(_, v)| self.sp(rect, *v).distance(p) <= HANDLE + 2.0).collect()
    }

    fn edge_at(&self, rect: Rect, ed: &Editor, p: Pos2) -> Option<(u32, DVec3, DVec3, DVec3)> {
        ed.edge_handles().into_iter().find(|(_, m, _, _)| self.sp(rect, *m).distance(p) <= HANDLE + 1.0)
    }

    fn draw(&self, painter: &egui::Painter, rect: Rect, ed: &Editor) {
        let th = theme::t();
        painter.rect_filled(rect, CornerRadius::ZERO, th.vp);
        self.draw_grid(painter, rect, ed.grid, ed.map_limit);
        let (h, v, depth) = self.axis.axes();
        let use_gl = self.gl.is_some();
        // große Auswahl: Kanten kommen aus dem Auswahl-Puffer der Grafikkarte
        let gl_sel = use_gl && ed.sel_solid_count() >= SEL_GL_MIN;
        // Auswahl vorhanden: alles, was nicht auf ihrer Tiefe liegt (±10), blasser zeichnen
        let dim = (ed.dim_levels && !ed.sel.is_empty())
            .then(|| ed.selection_bounds())
            .filter(|b| b.min.x <= b.max.x)
            .map(|b| (depth, b.min[depth] - DIM_TOL, b.max[depth] + DIM_TOL));
        let off_level = |b: &Aabb| dim.is_some_and(|(d, lo, hi)| b.max[d] < lo || b.min[d] > hi);
        let dimmed = |c: Color32| c.gamma_multiply(crate::view3d::DIM_ALPHA);
        if let Some(r) = self.gl.clone() {
            // Orthografie: Welt (h, v) -> Clip-Koordinaten dieser Ansicht
            let (sx, sy) = ((self.zoom * 2.0 / rect.width() as f64) as f32, (self.zoom * 2.0 / rect.height() as f64) as f32);
            let mut m = [[0.0f32; 4]; 4];
            m[h][0] = sx;
            m[v][1] = sy;
            m[3][0] = -(self.center.x as f32) * sx;
            m[3][1] = -(self.center.y as f32) * sy;
            m[3][3] = 1.0;
            let mvp = glam::Mat4::from_cols_array_2d(&m);
            let sel_px = gl_sel.then(|| (2.0 / rect.width(), 2.0 / rect.height()));
            let cb = eframe::egui_glow::CallbackFn::new(move |_info, p| {
                r.lock().unwrap().paint_lines2d(p.gl(), mvp, dim, sel_px);
            });
            painter.add(egui::PaintCallback { rect, callback: std::sync::Arc::new(cb) });
        }

        let view_world = Rect::from_two_pos(self.to_world(rect, rect.left_bottom()).as_vec2().to_array().into(), self.to_world(rect, rect.right_top()).as_vec2().to_array().into());
        let visible = |b: &Aabb| {
            let mn = self.axis.project(b.min);
            let mx = self.axis.project(b.max);
            !(mx.x < view_world.min.x as f64 || mn.x > view_world.max.x as f64 || mx.y < view_world.min.y as f64 || mn.y > view_world.max.y as f64)
        };
        let red = th.accent;
        let mut selected_lines: Vec<[Pos2; 2]> = Vec::new();

        for (owner, s) in ed.map.all_solids() {
            let selected = ed.is_selected_solid(owner, s.id);
            if use_gl && (!selected || gl_sel) {
                continue; // kommt aus dem Grafikkarten-Puffer
            }
            let e = owner.and_then(|id| ed.map.entity(id));
            if !visible(&s.bounds) || !ed.is_visible_solid(e, s) {
                continue;
            }
            let mut color = solid_color(ed, e, s);
            if off_level(&s.bounds) {
                color = dimmed(color);
            }
            let mut push = |a: Pos2, b: Pos2| {
                if selected {
                    selected_lines.push([a, b]);
                } else {
                    painter.line_segment([a, b], Stroke::new(1.0f32, color));
                }
            };
            if s.has_disp() {
                for g in s.disps.iter().flatten() {
                    let r = g.res;
                    for i in 0..r {
                        for j in 0..r {
                            let k = i * r + j;
                            if j + 1 < r {
                                push(self.sp(rect, g.pos[k]), self.sp(rect, g.pos[k + 1]));
                            }
                            if i + 1 < r {
                                push(self.sp(rect, g.pos[k]), self.sp(rect, g.pos[k + r]));
                            }
                        }
                    }
                }
                continue;
            }
            for poly in &s.polys {
                for i in 0..poly.len() {
                    push(self.sp(rect, poly[i]), self.sp(rect, poly[(i + 1) % poly.len()]));
                }
            }
        }
        for [a, b] in selected_lines {
            painter.line_segment([a, b], Stroke::new(1.5f32, red));
        }

        // Punkt-Entities
        // weit herausgezoomt kommen nicht ausgewählte Entities aus dem Grafikkarten-Puffer
        let gl_entities = use_gl && self.zoom < GL_ENTITY_ZOOM;
        for e in &ed.map.entities {
            if e.is_brush_entity() || !ed.is_visible_entity(e) {
                continue;
            }
            let selected = ed.sel.contains(&Obj::Entity(e.id));
            if gl_entities && !selected {
                continue;
            }
            let b = ed.entity_bounds(e);
            if !visible(&b) {
                continue;
            }
            let mut col = if selected { red } else { th.entity };
            if !ed.in_cordon(&b) {
                col = col.gamma_multiply(0.3);
            }
            if !selected && off_level(&b) {
                col = dimmed(col);
            }
            let sr = self.screen_rect(rect, &b);
            let model_box = ed.model_path(e).and_then(|m| ed.model_bounds.get(&m)).copied();
            if ed.mesh_index(e.id).is_some() {
                // Polygon-Mesh: Kanten des Netzes
                if !(use_gl && !selected) {
                    for (a, b) in ed.mesh_edges(e.id) {
                        painter.line_segment([self.sp(rect, a), self.sp(rect, b)], Stroke::new(1.0f32, col));
                    }
                }
            } else if let Some(mb) = model_box {
                let edges = ed.model_path(e).and_then(|m| ed.model_edges.get(&m)).filter(|v| !v.is_empty());
                if use_gl && !selected {
                    // Umriss bzw. Box kommt aus dem Grafikkarten-Puffer
                } else if let Some(edges) = edges {
                    // Umriss des Modells (Rand- und Knickkanten)
                    let x = Editor::entity_xform(e);
                    let st = Stroke::new(if selected { 1.4f32 } else { 1.0 }, col);
                    for [a, b] in edges.iter() {
                        painter.line_segment([self.sp(rect, x.apply(*a)), self.sp(rect, x.apply(*b))], st);
                    }
                } else {
                // gedrehte Modell-Box
                let x = Editor::entity_xform(e);
                let c = mb.corners().map(|p| self.sp(rect, x.apply(p)));
                for (i, j) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
                    painter.line_segment([c[i], c[j]], Stroke::new(1.0f32, col));
                }
                }
            } else {
                painter.rect_filled(sr, CornerRadius::same(2), theme::alpha(col, if selected { 0.35 } else { 0.85 }));
                painter.rect_stroke(sr, CornerRadius::same(2), Stroke::new(1.0f32, col), StrokeKind::Inside);
            }
            if let Some(o) = e.origin() {
                let a = e.angles();
                if e.get("angles").is_some() {
                    let (f, _, _) = crate::transform::angles_to_basis(a);
                    let p0 = self.sp(rect, o);
                    let d = self.axis.project(f);
                    painter.line_segment([p0, p0 + Vec2::new(d.x as f32, -d.y as f32) * 14.0], Stroke::new(1.5f32, col));
                }
            }
            if sr.width() > 24.0 {
                painter.text(sr.right_bottom() + Vec2::new(4.0, 2.0), Align2::LEFT_TOP, e.classname(), FontId::monospace(9.5), th.text3);
            }
        }

        // Verbindungen ausgewählter Entities (Outputs → Ziele)
        for id in ed.selected_entity_ids() {
            let Some(e) = ed.map.entity(id) else { continue };
            let from = ed.entity_bounds(e).center();
            for (_, val) in &e.outputs {
                let target = val.split(['\u{1b}', ',']).next().unwrap_or("");
                for t in ed.map.entities.iter().filter(|x| x.targetname().is_some_and(|n| n.eq_ignore_ascii_case(target))) {
                    let to = ed.entity_bounds(t).center();
                    painter.line_segment([self.sp(rect, from), self.sp(rect, to)], Stroke::new(1.0f32, theme::alpha(th.ok, 0.8)));
                }
            }
        }

        // Seile (move_rope/keyframe_rope) mit Durchhang
        for rope in crate::ropes::ropes(ed) {
            let col = if ed.sel.contains(&Obj::Entity(rope.ent)) { red } else { th.entity };
            let pts: Vec<Pos2> = rope.points.iter().map(|p| self.sp(rect, *p)).collect();
            painter.add(Shape::line(pts, Stroke::new(1.5f32, col)));
        }

        // Spieler im laufenden Spiel (Live-Abgleich)
        if let Some((pos, ang)) = ed.live_player {
            let c = Color32::from_rgb(77, 255, 115);
            let r = self.screen_rect(rect, &crate::geometry::Aabb::new(pos + DVec3::new(-16.0, -16.0, 0.0), pos + DVec3::new(16.0, 16.0, 72.0)));
            painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5f32, c), StrokeKind::Inside);
            let (p, y) = (ang.x.to_radians(), ang.y.to_radians());
            let f = DVec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
            let eye = pos + DVec3::Z * 64.0;
            painter.line_segment([self.sp(rect, eye), self.sp(rect, eye + f * 64.0)], Stroke::new(1.5f32, c));
            painter.text(r.right_top() + Vec2::new(4.0, 0.0), Align2::LEFT_TOP, crate::i18n::tr("Spieler"), FontId::proportional(11.0), c);
        }

        // Navmesh-Vorschau (nur sichtbarer Ausschnitt)
        if ed.nav_show {
            if let Some(nav) = &ed.nav_preview {
                let (h, v, _) = self.axis.axes();
                let w0 = self.to_world(rect, rect.left_bottom());
                let w1 = self.to_world(rect, rect.right_top());
                let mut c = DVec3::ZERO;
                c[h] = (w0.x + w1.x) * 0.5;
                c[v] = (w0.y + w1.y) * 0.5;
                let r = ((w1.x - w0.x).abs().max((w1.y - w0.y).abs())) * 0.6;
                // in Seitenansichten liegen die Bereiche übereinander: dort nur grob begrenzen
                let near = if self.axis == crate::geometry::Axis2D::Top { r } else { r.max(4096.0) };
                for (a, b, col) in crate::navmesh::preview_lines(nav, c, near) {
                    let col = Color32::from_rgba_unmultiplied((col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8, 150);
                    painter.line_segment([self.sp(rect, a), self.sp(rect, b)], Stroke::new(1.0f32, col));
                }
            }
        }

        // Spline-Werkzeug
        for (a, b, c) in crate::spline::preview_lines(&ed.spline) {
            let col = Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8);
            painter.line_segment([self.sp(rect, a), self.sp(rect, b)], Stroke::new(1.5f32, col));
        }

        // Türen & Co.: Bewegungs-Vorschau der Auswahl
        for (a, b, c) in crate::door_preview::selected(ed) {
            let col = Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8);
            painter.line_segment([self.sp(rect, a), self.sp(rect, b)], Stroke::new(1.5f32, col));
        }

        // Notizen
        for e in ed.map.entities.iter().filter(|e| crate::notes::is_note(e)) {
            if let Some(o) = e.origin().filter(|_| ed.is_visible(Obj::Entity(e.id))) {
                let s = self.sp(rect, o);
                if rect.contains(s) {
                    crate::notes::draw(&painter, s, e, ed.sel.contains(&Obj::Entity(e.id)));
                }
            }
        }

        // Gameplay-Maße: Spielerfigur an der Maus (Seitenansichten mit Höhenmarken, von oben 32 × 32)
        if ed.gameplay {
            if let Some(hp) = painter.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) {
                let w = self.to_world(rect, hp);
                let (h, v, _) = self.axis.axes();
                let g = ed.snap_grid();
                let mut foot = DVec3::ZERO;
                foot[h] = w.x;
                foot[v] = if v == 2 { crate::geometry::snap(w.y, g) } else { w.y };
                let half = crate::gameplay::WIDTH / 2.0;
                let col = Color32::from_rgb(120, 220, 255);
                if v == 2 {
                    let r = self.screen_rect(rect, &crate::geometry::Aabb::new(foot - DVec3::splat(half).with_z(0.0), foot + DVec3::new(half, half, crate::gameplay::STAND)));
                    painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5f32, col), StrokeKind::Inside);
                    let pts: Vec<(Pos2, String, [u8; 3])> = crate::gameplay::marks().into_iter().map(|(z, l, c)| (egui::pos2(r.right() + 6.0, self.sp(rect, foot + DVec3::Z * z).y), l, c)).collect();
                    for (p, y, label, c) in crate::gameplay::spread_labels(pts, 12.0) {
                        let c = Color32::from_rgb(c[0], c[1], c[2]);
                        painter.line_segment([egui::pos2(r.left() - 6.0, p.y), p], Stroke::new(1.0f32, c));
                        if (y - p.y).abs() > 1.0 {
                            painter.line_segment([p, egui::pos2(p.x + 4.0, y)], Stroke::new(1.0f32, c));
                        }
                        painter.text(egui::pos2(p.x + 6.0, y), Align2::LEFT_CENTER, label, FontId::proportional(10.5), c);
                    }
                } else {
                    let r = self.screen_rect(rect, &crate::geometry::Aabb::new(foot - DVec3::splat(half), foot + DVec3::splat(half)));
                    painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.5f32, col), StrokeKind::Inside);
                    painter.text(r.right_top() + Vec2::new(4.0, 0.0), Align2::LEFT_TOP, crate::i18n::tr("Spieler 32 × 32 · Durchgang ab 33 breit, Türen meist 48–64"), FontId::proportional(10.5), col);
                }
            }
        }

        // Messlinie
        if let Some((a, b)) = ed.measure {
            let (pa, pb) = (self.sp(rect, a), self.sp(rect, b));
            painter.line_segment([pa, pb], Stroke::new(1.5f32, Color32::from_rgb(80, 255, 255)));
            painter.circle_filled(pa, 3.0, Color32::from_rgb(80, 255, 255));
            painter.circle_filled(pb, 3.0, Color32::from_rgb(80, 255, 255));
            let d = b - a;
            let (h, v, _) = self.axis.axes();
            painter.text(pa + (pb - pa) * 0.5 + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, format!("{:.1}  (Δ{} {:.0}, Δ{} {:.0})", d.length(), ["X", "Y", "Z"][h], d[h], ["X", "Y", "Z"][v], d[v]), FontId::monospace(11.0), Color32::from_rgb(80, 255, 255));
        }

        // Kameras
        for (i, c) in ed.map.cameras.iter().enumerate() {
            let active = i == ed.map.active_camera;
            let col = if active { Color32::from_rgb(120, 200, 255) } else { Color32::from_rgb(60, 100, 140) };
            let a = self.sp(rect, c.pos);
            let b = self.sp(rect, c.look);
            painter.line_segment([a, b], Stroke::new(1.0f32, col));
            painter.circle_stroke(a, 5.0, Stroke::new(1.5f32, col));
            painter.circle_filled(b, 2.5, col);
        }

        // Skybox-Modus: Umriss der verkleinerten Map und Spawnpunkte (wo die Map „steht“)
        if ed.sky_edit.is_some() {
            if let Some(room) = ed.sky_room() {
                let w = crate::skybox::world_bounds(&ed.map, Some(&room.bounds));
                if !w.is_empty() {
                    let col = theme::alpha(th.accent, 0.8);
                    let r = self.screen_rect(rect, &Aabb::new(room.to_sky(w.min), room.to_sky(w.max)));
                    dashed_rect(painter, r, Stroke::new(1.5f32, col));
                    painter.text(r.left_top() + Vec2::new(3.0, -2.0), Align2::LEFT_BOTTOM, crate::i18n::tr_dyn(&format!("Map (1:{})", crate::map::fmt_num(room.scale))), FontId::proportional(10.5), col);
                    for e in ed.map.entities.iter().filter(|e| e.classname().to_ascii_lowercase().starts_with("info_player")) {
                        if let Some(o) = e.origin() {
                            painter.circle_filled(self.sp(rect, room.to_sky(o)), 2.5, col);
                        }
                    }
                    let cam = self.sp(rect, room.origin);
                    painter.circle_stroke(cam, 5.0, Stroke::new(1.5f32, col));
                }
            }
        }

        // Cordon
        if let Some(c) = &ed.map.cordon {
            let r = self.screen_rect(rect, &c.bounds);
            let col = if c.active { Color32::from_rgb(255, 60, 60) } else { Color32::from_rgb(120, 60, 60) };
            dashed_rect(painter, r, Stroke::new(1.5f32, col));
            painter.text(r.left_top() + Vec2::new(3.0, 2.0), Align2::LEFT_TOP, if c.active { crate::i18n::tr("Cordon") } else { crate::i18n::tr("Cordon (aus)") }, FontId::proportional(10.0), col);
        }

        // Clip-Vorschau
        if ed.tool == Tool::Clip {
            let (keep, drop) = ed.clip_preview();
            for (list, col) in [(&drop, theme::alpha(th.err, 0.6)), (&keep, th.text)] {
                for s in list.iter() {
                    for poly in &s.polys {
                        let pts: Vec<Pos2> = poly.iter().chain(poly.first()).map(|p| self.sp(rect, *p)).collect();
                        painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0f32, col), 5.0, 3.0));
                    }
                }
            }
            // Punkte des 3-Punkt-Clips (aus der 3D-Ansicht)
            for (k, p) in ed.clip3.iter().enumerate() {
                let q = self.sp(rect, *p);
                painter.circle_filled(q, 4.0, th.accent);
                if let Some(n) = ed.clip3.get(k + 1).or(if ed.clip3.len() == 3 { ed.clip3.first() } else { None }) {
                    painter.line_segment([q, self.sp(rect, *n)], Stroke::new(1.0f32, th.accent));
                }
            }
            if let Some(c) = ed.clip.filter(|c| c.axis == self.axis) {
                let (a, b) = (self.to_screen(rect, c.a), self.to_screen(rect, c.b));
                painter.line_segment([a, b], Stroke::new(1.5f32, th.accent));
                painter.circle_filled(a, 4.0, th.accent);
                painter.circle_filled(b, 4.0, th.accent);
                let mode = match ed.clip_mode {
                    crate::editor::ClipMode::Both => crate::i18n::tr("Beide behalten"),
                    crate::editor::ClipMode::Front => crate::i18n::tr("Vorderseite behalten"),
                    crate::editor::ClipMode::Back => crate::i18n::tr("Rückseite behalten"),
                };
                painter.text(rect.left_bottom() + Vec2::new(6.0, -6.0), Align2::LEFT_BOTTOM, format!("Clip: {mode} – Enter schneidet, Shift+X wechselt"), FontId::proportional(12.0), th.text2);
            }
        }

        // Vertex-Griffe
        if ed.tool == Tool::Vertex {
            for (_, m, _, _) in ed.edge_handles() {
                painter.circle_stroke(self.sp(rect, m), 3.0, Stroke::new(1.0f32, th.text3));
            }
            for (sid, p) in ed.vertex_handles() {
                let selected = ed.vsel.iter().any(|(s, q)| *s == sid && q.distance(p) < 0.01);
                let r = Rect::from_center_size(self.sp(rect, p), Vec2::splat(HANDLE * 1.6));
                painter.rect_filled(r, CornerRadius::ZERO, if selected { th.accent } else { th.vp });
                painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.2f32, if selected { th.accent } else { th.brush }), StrokeKind::Middle);
            }
        }

        // Auswahl: Akzent-Umriss, 16 % Füllung, quadratische Griffe, Maßangabe
        if !ed.sel.is_empty() && ed.tool != Tool::Vertex && ed.tool != Tool::Clip {
            let b = ed.selection_bounds();
            let sr = self.screen_rect(rect, &b);
            painter.rect_filled(sr, CornerRadius::ZERO, theme::alpha(th.accent, 0.16));
            painter.rect_stroke(sr, CornerRadius::ZERO, Stroke::new(1.5f32, th.accent), StrokeKind::Middle);
            if matches!(ed.tool, Tool::Select | Tool::Face | Tool::Disp) {
                for (hx, hy, hp) in self.handle_positions(sr, ed.xmode) {
                    match ed.xmode {
                        XMode::Scale => {
                            let hr = Rect::from_center_size(hp, Vec2::splat(7.0));
                            painter.rect_filled(hr, CornerRadius::ZERO, th.vp);
                            painter.rect_stroke(hr, CornerRadius::ZERO, Stroke::new(1.5f32, th.accent), StrokeKind::Middle);
                        }
                        XMode::Rotate => {
                            painter.circle_filled(hp, 4.5, th.vp);
                            painter.circle_stroke(hp, 4.5, Stroke::new(1.5f32, th.accent));
                        }
                        XMode::Skew => {
                            let d = HANDLE + 1.0;
                            let pts = if hx == 0 {
                                vec![hp + Vec2::new(-d * 1.5, 0.0), hp + Vec2::new(0.0, -d * hy as f32), hp + Vec2::new(d * 1.5, 0.0)]
                            } else {
                                vec![hp + Vec2::new(0.0, -d * 1.5), hp + Vec2::new(d * hx as f32, 0.0), hp + Vec2::new(0.0, d * 1.5)]
                            };
                            painter.add(Shape::convex_polygon(pts, th.vp, Stroke::new(1.5f32, th.accent)));
                        }
                    }
                }
            }
            let size = b.size();
            // Maßlinie oben
            let y = sr.top() - 16.0;
            let ms = Stroke::new(1.0f32, th.accent);
            painter.line_segment([Pos2::new(sr.left(), y), Pos2::new(sr.right(), y)], ms);
            painter.line_segment([Pos2::new(sr.left(), y - 5.0), Pos2::new(sr.left(), y + 5.0)], ms);
            painter.line_segment([Pos2::new(sr.right(), y - 5.0), Pos2::new(sr.right(), y + 5.0)], ms);
            let wtxt = crate::map::fmt_num(size[h]);
            let g = painter.layout_no_wrap(wtxt, FontId::monospace(10.5), th.accent_text);
            let lr = Rect::from_center_size(Pos2::new(sr.center().x, y), g.size() + Vec2::new(10.0, 4.0));
            painter.rect_filled(lr, CornerRadius::same(4), th.vp);
            painter.galley(lr.center() - g.size() / 2.0, g, th.accent_text);
            painter.text(sr.right_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, crate::map::fmt_num(size[v]), FontId::monospace(10.5), th.accent_text);
            // Etikett: Klasse · B × T × H
            let class = ed.sel.iter().find_map(|o| if let Obj::Entity(id) = o { ed.map.entity(*id).map(|e| e.classname().to_string()) } else { None });
            let label = format!("{} · {} × {} × {}", class.unwrap_or_else(|| if ed.sel.len() == 1 { "Brush".into() } else { format!("{} Objekte", ed.sel.len()) }), crate::map::fmt_num(size.x), crate::map::fmt_num(size.y), crate::map::fmt_num(size.z));
            let g = painter.layout_no_wrap(label, FontId::proportional(11.0), th.tooltip_text);
            let pos = sr.right_bottom() + Vec2::new(10.0, 10.0);
            let lr = Rect::from_min_size(pos, g.size() + Vec2::new(14.0, 8.0));
            if rect.contains_rect(lr) || rect.contains(lr.min) {
                painter.rect_filled(lr, CornerRadius::same(6), th.tooltip);
                painter.galley(lr.min + Vec2::new(7.0, 4.0), g, th.tooltip_text);
            }
        }

        // Pointfile (Leck-Pfad)
        if ed.pointfile.len() > 1 {
            let pts: Vec<Pos2> = ed.pointfile.iter().map(|p| self.sp(rect, *p)).collect();
            painter.add(Shape::line(pts, Stroke::new(2.0f32, Color32::from_rgb(255, 0, 0))));
        }

        // Vergleich mit anderem Map-Stand
        for (b, c) in &ed.diff_boxes {
            let (p, q) = (self.sp(rect, b.min), self.sp(rect, b.max));
            let r = Rect::from_two_pos(p, q).expand(3.0);
            let col = Color32::from_rgb(c[0], c[1], c[2]);
            painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(2.0f32, col), StrokeKind::Outside);
        }
        // Andere Teilnehmer der Live-Sitzung: Auswahl und Kamera
        for u in &ed.remote_users {
            let col = Color32::from_rgb(u.color[0], u.color[1], u.color[2]);
            let b = ed.remote_selection_bounds(u);
            if !b.is_empty() {
                let r = Rect::from_two_pos(self.sp(rect, b.min), self.sp(rect, b.max)).expand(4.0);
                painter.rect_filled(r, CornerRadius::ZERO, theme::alpha(col, 0.10));
                dashed_rect(painter, r, Stroke::new(1.5f32, col));
                // Mauszeiger mit Namensschild
                let tip = r.right_bottom() + Vec2::new(-4.0, -4.0);
                let arrow = vec![tip, tip + Vec2::new(0.0, 13.0), tip + Vec2::new(3.5, 9.5), tip + Vec2::new(6.3, 15.1), tip + Vec2::new(8.3, 14.1), tip + Vec2::new(5.5, 8.6), tip + Vec2::new(10.5, 8.6)];
                painter.add(Shape::convex_polygon(arrow, col, Stroke::new(0.8f32, Color32::WHITE)));
                let g = painter.layout_no_wrap(u.name.clone(), FontId::new(10.5, theme::semibold()), Color32::WHITE);
                let nr = Rect::from_min_size(tip + Vec2::new(14.0, 12.0), g.size() + Vec2::new(14.0, 6.0));
                painter.rect_filled(nr, CornerRadius::same(4), col);
                painter.galley(nr.min + Vec2::new(7.0, 3.0), g, Color32::WHITE);
            }
            if let Some((p, d)) = u.cam {
                let pos = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                let dir = DVec3::new(d[0] as f64, d[1] as f64, d[2] as f64);
                let a = self.sp(rect, pos);
                let tip = self.sp(rect, pos + dir * (40.0 / self.zoom));
                if rect.expand(20.0).contains(a) {
                    painter.circle_filled(a, 5.0, col);
                    painter.line_segment([a, tip], Stroke::new(2.0f32, col));
                    painter.text(a + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, &u.name, FontId::proportional(11.0), col);
                }
            }
        }

        // Optimierer: KI-Netz und Markierungen
        for (a, b, ok) in &ed.opt_links {
            let c = if *ok { Color32::from_rgba_unmultiplied(80, 255, 120, 110) } else { Color32::from_rgb(255, 60, 60) };
            painter.line_segment([self.sp(rect, *a), self.sp(rect, *b)], Stroke::new(1.0f32, c));
        }
        let hover = painter.ctx().pointer_hover_pos().filter(|p| rect.contains(*p));
        for mk in &ed.opt_markers {
            let p = self.sp(rect, mk.pos);
            if !rect.expand(8.0).contains(p) {
                continue;
            }
            let c = Color32::from_rgb(mk.color[0], mk.color[1], mk.color[2]);
            painter.circle_stroke(p, 6.0, Stroke::new(2.0f32, c));
            painter.circle_filled(p, 2.0, c);
            if hover.is_some_and(|h| h.distance(p) < 9.0) {
                let g = painter.layout_no_wrap(mk.text.clone(), FontId::proportional(12.0), Color32::WHITE);
                let r = Rect::from_min_size(p + Vec2::new(10.0, -8.0), g.size()).expand(3.0);
                painter.rect_filled(r, 3.0, Color32::from_black_alpha(220));
                painter.galley(p + Vec2::new(10.0, -8.0), g, Color32::WHITE);
            }
        }

        // Arbeitspunkt (Kreuz)
        let wp = self.sp(rect, ed.work_point);
        let wc = theme::alpha(th.accent_text, 0.6);
        painter.line_segment([wp - Vec2::new(5.0, 0.0), wp + Vec2::new(5.0, 0.0)], Stroke::new(1.0f32, wc));
        painter.line_segment([wp - Vec2::new(0.0, 5.0), wp + Vec2::new(0.0, 5.0)], Stroke::new(1.0f32, wc));
        let _ = CameraDef { pos: DVec3::ZERO, look: DVec3::ZERO, name: String::new() };
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect, grid: f64, limit: f64) {
        let mut step = grid;
        while step * self.zoom < 6.0 {
            step *= 2.0;
        }
        let w0 = self.to_world(rect, rect.left_bottom());
        let w1 = self.to_world(rect, rect.right_top());
        let th = theme::t();
        let major = (step * 8.0).max(128.0);
        let line_color = |val: f64| {
            if val.abs() < 1e-6 {
                th.axis
            } else if (val / major).fract().abs() < 1e-6 || (val / major).fract().abs() > 1.0 - 1e-6 {
                th.grid_major
            } else {
                th.grid
            }
        };
        let mut x = (w0.x / step).floor() * step;
        let mut n = 0;
        while x <= w1.x && n < 2000 {
            let sx = self.to_screen(rect, DVec2::new(x, 0.0)).x;
            painter.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], Stroke::new(1.0f32, line_color(x)));
            x += step;
            n += 1;
        }
        let mut y = (w0.y / step).floor() * step;
        n = 0;
        while y <= w1.y && n < 2000 {
            let sy = self.to_screen(rect, DVec2::new(0.0, y)).y;
            painter.line_segment([Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)], Stroke::new(1.0f32, line_color(y)));
            y += step;
            n += 1;
        }
        // Map-Grenze: außerhalb abdunkeln, Rand rot
        let a = self.to_screen(rect, DVec2::new(-limit, -limit));
        let b = self.to_screen(rect, DVec2::new(limit, limit));
        let inner = Rect::from_two_pos(a, b);
        if !inner.contains_rect(rect) {
            let shade = egui::Color32::from_black_alpha(70);
            for r in [
                Rect::from_min_max(rect.min, Pos2::new(rect.max.x, inner.min.y.max(rect.min.y))),
                Rect::from_min_max(Pos2::new(rect.min.x, inner.max.y.min(rect.max.y)), rect.max),
                Rect::from_min_max(Pos2::new(rect.min.x, inner.min.y.max(rect.min.y)), Pos2::new(inner.min.x.max(rect.min.x), inner.max.y.min(rect.max.y))),
                Rect::from_min_max(Pos2::new(inner.max.x.min(rect.max.x), inner.min.y.max(rect.min.y)), Pos2::new(rect.max.x, inner.max.y.min(rect.max.y))),
            ] {
                if r.width() > 0.0 && r.height() > 0.0 {
                    painter.rect_filled(r, 0.0, shade);
                }
            }
            painter.rect_stroke(inner, 0.0, Stroke::new(1.5f32, th.err.gamma_multiply(0.8)), egui::StrokeKind::Middle);
        }
    }
}
