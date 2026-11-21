//! Modell-Browser nach UI-Konzept: Raster mit Vorschaubildern, drehbare 3D-Vorschau, Skins, Infos.

use crate::app::App;
use crate::theme::{self, alpha, mono, sans, semibold, t};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use glam::{DVec3, Mat4, Vec3};
use std::collections::HashMap;

/// Zustand des Modell-Browsers
pub struct ModelBrowserState {
    pub filter: String,
    pub grid: bool,
    pub selected: Option<String>,
    pub skin: usize,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    collision: bool,
    thumbs: HashMap<String, egui::TextureHandle>,
    /// Texturen für die Vorschaubilder (Material → RGBA), None = nicht gefunden
    thumb_tex: HashMap<String, Option<std::sync::Arc<(u32, u32, Vec<u8>, bool)>>>,
    /// Animationen des ausgewählten Modells (wird im Hintergrund gelesen)
    anim_path: Option<String>,
    anim: Option<std::sync::Arc<crate::mdl_anim::Anim>>,
    anim_rx: Option<std::sync::mpsc::Receiver<(String, Option<std::sync::Arc<crate::mdl_anim::Anim>>)>>,
    /// gewählte Sequenz (None = Grundhaltung), Abspielen, aktuelles Bild
    seq: Option<usize>,
    playing: bool,
    frame: f32,
    last_t: Option<f64>,
}

impl Default for ModelBrowserState {
    fn default() -> Self {
        ModelBrowserState { filter: String::new(), grid: true, selected: None, skin: 0, yaw: 0.8, pitch: 0.35, zoom: 1.0, collision: false, thumbs: HashMap::new(), thumb_tex: HashMap::new(), anim_path: None, anim: None, anim_rx: None, seq: None, playing: false, frame: 0.0, last_t: None }
    }
}

/// Vorschaubild mit Textur (Software-Rasterizer, gleiche Ansicht wie `render_tris`).
/// `tex(material)` liefert die Grundtextur; fehlt sie, wird die Fläche grau schattiert.
fn model_thumb_textured(m: &crate::mdl::Model, size: usize, tex: &mut dyn FnMut(&str) -> Option<std::sync::Arc<(u32, u32, Vec<u8>, bool)>>) -> Vec<u8> {
    let bg = [26u8, 29, 35];
    let mut img: Vec<u8> = (0..size * size).flat_map(|_| [bg[0], bg[1], bg[2], 255]).collect();
    let mut zbuf = vec![f64::MIN; size * size];
    let (yaw, pitch) = (45f64.to_radians(), 30f64.to_radians());
    let right = DVec3::new(yaw.cos(), -yaw.sin(), 0.0);
    let fwd = DVec3::new(yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos(), -pitch.sin());
    let up = right.cross(fwd);
    let proj = |p: DVec3| (p.dot(right), p.dot(up), -p.dot(fwd));
    let v3 = |v: &[f32; 8]| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64);
    let (mut minx, mut maxx, mut miny, mut maxy) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for mesh in &m.meshes {
        for v in &mesh.verts {
            let (x, y, _) = proj(v3(v));
            minx = minx.min(x);
            maxx = maxx.max(x);
            miny = miny.min(y);
            maxy = maxy.max(y);
        }
    }
    if minx > maxx {
        return img;
    }
    let scale = (size as f64 * 0.88) / (maxx - minx).max(maxy - miny).max(1.0);
    let ox = size as f64 / 2.0 - (minx + maxx) / 2.0 * scale;
    let oy = size as f64 / 2.0 + (miny + maxy) / 2.0 * scale;
    let light = DVec3::new(-0.4, -0.6, 0.8).normalize();
    for mesh in &m.meshes {
        let t = m.material_for(0, mesh.skinref).and_then(|name| tex(name));
        for tri in mesh.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let vs = [&mesh.verts[tri[0] as usize], &mesh.verts[tri[1] as usize], &mesh.verts[tri[2] as usize]];
            let n = (v3(vs[1]) - v3(vs[0])).cross(v3(vs[2]) - v3(vs[0])).normalize_or_zero();
            let shade = 0.55 + 0.45 * n.dot(light).abs().max(n.dot(light));
            let p: Vec<(f64, f64, f64)> = vs.iter().map(|v| {
                let (x, y, z) = proj(v3(v));
                (ox + x * scale, oy - y * scale, z)
            }).collect();
            let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-9 {
                continue;
            }
            let x0 = p.iter().map(|q| q.0).fold(f64::MAX, f64::min).max(0.0) as usize;
            let x1 = (p.iter().map(|q| q.0).fold(f64::MIN, f64::max).ceil() as usize).min(size - 1);
            let y0 = p.iter().map(|q| q.1).fold(f64::MAX, f64::min).max(0.0) as usize;
            let y1 = (p.iter().map(|q| q.1).fold(f64::MIN, f64::max).ceil() as usize).min(size - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                    let w0 = ((p[1].0 - fx) * (p[2].1 - fy) - (p[2].0 - fx) * (p[1].1 - fy)) / area;
                    let w1 = ((p[2].0 - fx) * (p[0].1 - fy) - (p[0].0 - fx) * (p[2].1 - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let z = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                    let i = y * size + x;
                    if z <= zbuf[i] {
                        continue;
                    }
                    let mut c = [0.72, 0.74, 0.78];
                    if let Some(t) = &t {
                        let (tw, th, px, translucent) = (t.0 as usize, t.1 as usize, &t.2, t.3);
                        let u = w0 * vs[0][6] as f64 + w1 * vs[1][6] as f64 + w2 * vs[2][6] as f64;
                        let v = w0 * vs[0][7] as f64 + w1 * vs[1][7] as f64 + w2 * vs[2][7] as f64;
                        let tx = ((u.rem_euclid(1.0) * tw as f64) as usize).min(tw - 1);
                        let ty = ((v.rem_euclid(1.0) * th as f64) as usize).min(th - 1);
                        let k = (ty * tw + tx) * 4;
                        // Alphatest (Gitter, Blätter): durchsichtige Texel auslassen
                        if translucent && px[k + 3] < 100 {
                            continue;
                        }
                        c = [px[k] as f64 / 255.0, px[k + 1] as f64 / 255.0, px[k + 2] as f64 / 255.0];
                    }
                    zbuf[i] = z;
                    img[i * 4..i * 4 + 3].copy_from_slice(&[(c[0] * shade * 255.0) as u8, (c[1] * shade * 255.0) as u8, (c[2] * shade * 255.0) as u8]);
                }
            }
        }
    }
    img
}

/// Kleines Vorschaubild eines Modells ohne Textur (Software-Rasterizer, schattiert)
#[cfg(test)]
fn model_thumb(m: &crate::mdl::Model, size: usize) -> Vec<u8> {
    let mut tris = Vec::new();
    for mesh in &m.meshes {
        for t in mesh.indices.chunks(3) {
            if t.len() < 3 {
                continue;
            }
            let p = |i: u32| {
                let v = mesh.verts[i as usize];
                DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
            };
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let n = (b - a).cross(c - a).normalize_or_zero();
            tris.push(([a, b, c], [0.72, 0.74, 0.78, 1.0], n));
        }
    }
    crate::prefabs::render_tris(tris, size, [26, 29, 35])
}

impl App {
    pub(crate) fn model_browser(&mut self, ctx: &egui::Context) {
        if !self.win.models {
            return;
        }
        let th = t();
        let words: Vec<String> = self.mb.filter.to_ascii_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        let items: Vec<String> = self.models.list.iter().filter(|m| words.iter().all(|w| m.contains(w.as_str()))).cloned().collect();
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let sel = self.mb.selected.clone();
        let sel_model = sel.as_ref().and_then(|p| self.models.get(p));
        let body_h = if self.anim_ready() { 512.0 } else { 448.0 };
        let d = theme::Dlg::new(crate::i18n::tr("Modell-Browser"), 800.0).subtitle(crate::i18n::count(items.len(), "Modell", "Modelle")).height(body_h + 112.0).footer().header_extra().no_scroll();
        let renderer = self.view3d.renderer.clone();
        let fs = self.mats.fs();
        self.update_anim(ctx, sel.as_deref(), fs.clone());
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Header => {
                // Umschalter Liste/Raster und Filter (rechts im Kopf, Layout rechts-nach-links)
                for (icon, grid) in [("four_views", true), ("one_view", false)] {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(28.0, 26.0), Sense::click());
                    let on = self.mb.grid == grid;
                    ui.painter().rect_filled(r, 6.0, if on { th.accent_soft } else if resp.hovered() { th.hover } else { Color32::TRANSPARENT });
                    crate::icons::draw(ui.painter(), r.shrink(6.0), icon, if on { th.accent_text } else { th.text2 }, 1.4);
                    if resp.on_hover_text(if grid { crate::i18n::tr("Raster") } else { "Liste" }).clicked() {
                        self.mb.grid = grid;
                    }
                }
                ui.add_space(8.0);
                ui.add(egui::TextEdit::singleline(&mut self.mb.filter).hint_text(crate::theme::hint(crate::i18n::tr("Filtern, z. B. props_junk crate"))).desired_width(280.0));
            }
            theme::Part::Body => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    // ---- links: Raster oder Liste
                    let left_w = 800.0 - 312.0 - 1.0;
                    ui.allocate_ui_with_layout(Vec2::new(left_w, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(left_w);
                        if items.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| ui.label(RichText::new(if self.models.list.is_empty() { crate::i18n::tr("Keine Modelle gefunden – Spielordner in den Einstellungen prüfen.") } else { crate::i18n::tr("Kein Modell passt zum Filter.") }).color(th.text3)));
                            return;
                        }
                        egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 10, top: 12, bottom: 8 }).show(ui, |ui| {
                            if self.mb.grid {
                                let cols = 3;
                                let gap = 10.0;
                                let cw = (ui.available_width() - gap * (cols as f32 - 1.0) - 8.0) / cols as f32;
                                let ch = 92.0 + 40.0;
                                let rows = items.len().div_ceil(cols);
                                egui::ScrollArea::vertical().id_salt("mb_grid").auto_shrink([false, false]).show_rows(ui, ch + gap, rows, |ui, range| {
                                    for row in range {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = gap;
                                            for c in 0..cols {
                                                let Some(path) = items.get(row * cols + c) else { break };
                                                self.model_card(ui, path, cw, ch);
                                            }
                                        });
                                        ui.add_space(gap);
                                    }
                                });
                            } else {
                                egui::ScrollArea::vertical().id_salt("mb_list").auto_shrink([false, false]).show_rows(ui, 30.0, items.len(), |ui, range| {
                                    for i in range {
                                        let p = &items[i];
                                        let on = sel.as_deref() == Some(p.as_str());
                                        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
                                        if on {
                                            ui.painter().rect(r, 6.0, th.accent_soft, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                                        } else if resp.hovered() {
                                            ui.painter().rect_filled(r, 6.0, th.hover);
                                        }
                                        let mut job = egui::text::LayoutJob::simple_singleline(p.trim_start_matches("models/").to_string(), mono(12.0), th.text);
                                        job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 20.0);
                                        let g = ui.painter().layout_job(job);
                                        ui.painter().galley(egui::pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, th.text);
                                        if resp.clicked() {
                                            self.mb.selected = Some(p.clone());
                                            self.mb.skin = 0;
                                        }
                                        if resp.double_clicked() {
                                            apply = true;
                                        }
                                    }
                                });
                            }
                        });
                    });
                    let (sep, _) = ui.allocate_exact_size(Vec2::new(1.0, body_h), Sense::hover());
                    ui.painter().rect_filled(sep, 0.0, th.line);
                    // ---- rechts: Vorschau und Infos
                    ui.allocate_ui_with_layout(Vec2::new(312.0, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(312.0);
                        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 8 }).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 10.0;
                            self.model_preview(ui, sel.as_deref(), sel_model.as_deref(), renderer.clone());
                            match (&sel, &sel_model) {
                                (Some(_), Some(m)) => {
                                    let nskins = m.skins.len().max(1);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        ui.label(RichText::new(crate::i18n::tr("Skin")).color(th.text2));
                                        for k in 0..nskins.min(8) {
                                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                                            let mat = m.skins.get(k).and_then(|f| f.first()).and_then(|i| m.materials.get(*i)).cloned();
                                            let tex = mat.as_ref().and_then(|mm| self.mats.thumb(mm).map(|t| t.id()));
                                            if k == self.mb.skin {
                                                ui.painter().rect_stroke(r.expand(4.0), 9.0, Stroke::new(2.0f32, th.accent), StrokeKind::Inside);
                                            }
                                            match tex {
                                                Some(id) => {
                                                    egui::Image::new((id, r.size())).corner_radius(6).paint_at(ui, r);
                                                }
                                                None => {
                                                    let c = crate::view3d::material_color(mat.as_deref().unwrap_or("x"));
                                                    ui.painter().rect_filled(r, 6.0, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8));
                                                }
                                            }
                                            if resp.clicked() {
                                                self.mb.skin = k;
                                            }
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} / {nskins}", self.mb.skin + 1))).font(mono(11.5)).color(th.text3));
                                        });
                                    });
                                    self.anim_controls(ui);
                                    let s = m.bounds.size();
                                    let phy = sel.as_ref().map(|p| p.trim_end_matches(".mdl").to_string() + ".phy");
                                    let has_phy = phy.as_ref().zip(fs.as_ref()).is_some_and(|(p, fs)| fs.exists(p));
                                    let rows = [
                                        (crate::i18n::tr("Größe"), format!("{} × {} × {} u", s.x.round(), s.y.round(), s.z.round()), th.text),
                                        (crate::i18n::tr("Polygone"), crate::chrome::fmt_thousands(m.triangle_count()), th.text),
                                        (crate::i18n::tr("Kollisionsmodell"), if has_phy { crate::i18n::tr("vorhanden").to_string() } else { crate::i18n::tr("keins").to_string() }, if has_phy { th.ok } else { th.text3 }),
                                    ];
                                    for (k, v, c) in rows {
                                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
                                        ui.painter().text(egui::pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, k, sans(12.5), th.text2);
                                        ui.painter().text(egui::pos2(r.right(), r.center().y), egui::Align2::RIGHT_CENTER, v, mono(12.0), c);
                                        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0f32, th.line));
                                    }
                                }
                                (Some(_), None) => {
                                    ui.label(RichText::new(crate::i18n::tr("Modell wird geladen…")).color(th.text3));
                                }
                                _ => {
                                    ui.label(RichText::new(crate::i18n::tr("Links ein Modell anklicken.")).color(th.text3));
                                }
                            }
                        });
                    });
                });
            }
            theme::Part::Footer => {
                if ui.add_enabled(sel.is_some(), egui::Button::new(RichText::new(crate::i18n::tr("Übernehmen")).family(semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    apply = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    cancel = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let txt = sel.clone().unwrap_or_else(|| match &self.dlg.browse_target {
                        Some((id, key)) => format!("Ziel: Entity {id} / {key}"),
                        None if !self.ed.selected_entity_ids().is_empty() => crate::i18n::tr("Ziel: ausgewählte Entities").into(),
                        None => crate::i18n::tr("Ziel: Modell für neue Props").into(),
                    });
                    ui.add(egui::Label::new(RichText::new(txt).font(mono(11.5)).color(th.text3)).truncate());
                });
            }
        });
        if apply {
            if let Some(m) = self.mb.selected.clone() {
                let skin = self.mb.skin;
                match self.dlg.browse_target.clone() {
                    Some((id, key)) => {
                        self.ed.checkpoint();
                        if let Some(e) = self.ed.map.entity_mut(id) {
                            e.set(&key, m.clone());
                            if skin > 0 {
                                e.set("skin", skin.to_string());
                            }
                        }
                        self.ed.touch();
                    }
                    None => {
                        // ohne Ziel: auf ausgewählte Entities mit Modell anwenden (Props, Türen …),
                        // sonst als Modell für neue Props merken
                        let ids: Vec<u32> = self.ed.selected_entity_ids().into_iter().filter(|id| self.ed.map.entity(*id).is_some_and(|e| !e.is_brush_entity() && (e.get("model").is_some() || e.classname().starts_with("prop_")))).collect();
                        if ids.is_empty() {
                            self.ed.next_model = m.clone();
                        } else {
                            self.ed.checkpoint();
                            for id in ids {
                                if let Some(e) = self.ed.map.entity_mut(id) {
                                    e.set("model", m.clone());
                                    if skin > 0 {
                                        e.set("skin", skin.to_string());
                                    }
                                }
                            }
                            self.ed.touch();
                        }
                    }
                }
                self.ed.status = format!("Modell: {m}");
                open = false;
            }
        }
        if cancel {
            open = false;
        }
        self.win.models = open;
    }

    /// Karte im Raster: Vorschaubild (immer dunkel), Name, Ordner
    fn model_card(&mut self, ui: &mut egui::Ui, path: &str, w: f32, h: f32) {
        let th = t();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
        if !ui.is_rect_visible(r) {
            return;
        }
        let on = self.mb.selected.as_deref() == Some(path);
        if on {
            ui.painter().rect_stroke(r.expand(3.0), 11.0, Stroke::new(3.0f32, th.accent_soft), StrokeKind::Inside);
        }
        ui.painter().rect(r, 8.0, th.panel, Stroke::new(1.0f32, if on { th.accent } else if resp.hovered() { th.line_strong } else { th.line }), StrokeKind::Inside);
        let pr = Rect::from_min_size(r.min + Vec2::splat(1.0), Vec2::new(w - 2.0, 92.0));
        ui.painter().rect_filled(pr, egui::CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, Color32::from_rgb(0x1A, 0x1D, 0x23));
        // Vorschaubild: einmal rendern, sobald das Modell geladen ist
        if !self.mb.thumbs.contains_key(path) {
            if let Some(m) = self.models.get(path) {
                let fs = self.mats.fs();
                let cache = &mut self.mb.thumb_tex;
                let px = model_thumb_textured(&m, 88, &mut |name: &str| {
                    let key = name.to_ascii_lowercase();
                    cache.entry(key.clone()).or_insert_with(|| fs.as_ref().and_then(|fs| crate::materials::base_rgba(fs, &key, 128)).map(std::sync::Arc::new)).clone()
                });
                let img = egui::ColorImage::from_rgba_unmultiplied([88, 88], &px);
                let h = ui.ctx().load_texture(format!("mdlthumb:{path}"), img, egui::TextureOptions::LINEAR);
                self.mb.thumbs.insert(path.to_string(), h);
            }
        }
        match self.mb.thumbs.get(path) {
            Some(tex) => {
                let ir = Rect::from_center_size(pr.center(), Vec2::splat(88.0));
                egui::Image::new((tex.id(), ir.size())).paint_at(ui, ir);
            }
            None => {
                crate::icons::draw(ui.painter(), Rect::from_center_size(pr.center(), Vec2::splat(26.0)), "block", alpha(Color32::WHITE, 0.25), 1.4);
            }
        }
        let rel = path.trim_start_matches("models/");
        let (folder, name) = rel.rsplit_once('/').unwrap_or(("", rel));
        let tw = w - 16.0;
        let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), mono(11.5), th.text);
        job.wrap = egui::text::TextWrapping::truncate_at_width(tw);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(r.left() + 8.0, pr.bottom() + 6.0), g, th.text);
        let mut job = egui::text::LayoutJob::simple_singleline(folder.to_string(), sans(11.0), th.text3);
        job.wrap = egui::text::TextWrapping::truncate_at_width(tw);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(r.left() + 8.0, pr.bottom() + 22.0), g, th.text3);
        if resp.clicked() {
            self.mb.selected = Some(path.to_string());
            self.mb.skin = 0;
        }
        if resp.double_clicked() {
            self.mb.selected = Some(path.to_string());
        }
    }

    /// Drehbare 3D-Vorschau (280 × 280, immer dunkel)
    fn model_preview(&mut self, ui: &mut egui::Ui, path: Option<&str>, model: Option<&crate::mdl::Model>, renderer: Option<std::sync::Arc<std::sync::Mutex<crate::view3d::Renderer>>>) {
        let th = t();
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(280.0), Sense::click_and_drag());
        let bg = Color32::from_rgb(0x1A, 0x1D, 0x23);
        ui.painter().rect_filled(r, 10.0, bg);
        let p = ui.painter().with_clip_rect(r.shrink(1.0));
        let mut x = r.left() + 20.0;
        while x < r.right() {
            p.vline(x, r.y_range(), Stroke::new(1.0f32, alpha(Color32::WHITE, 0.04)));
            x += 20.0;
        }
        let mut y = r.top() + 20.0;
        while y < r.bottom() {
            p.hline(r.x_range(), y, Stroke::new(1.0f32, alpha(Color32::WHITE, 0.04)));
            y += 20.0;
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            self.mb.yaw -= d.x * 0.01;
            self.mb.pitch = (self.mb.pitch + d.y * 0.01).clamp(-1.4, 1.4);
        }
        if resp.hovered() {
            let s = ui.input(|i| i.raw_scroll_delta.y);
            if s != 0.0 {
                self.mb.zoom = (self.mb.zoom * (s * 0.002).exp()).clamp(0.3, 6.0);
            }
        }
        if let (Some(path), Some(m), Some(renderer)) = (path, model, renderer) {
            let skin = if self.mb.collision { usize::MAX } else { self.mb.skin };
            let key = format!("mdl:{path}#{skin}");
            // Animationsbild: Vertices auf der CPU verformen und jedes Bild neu hochladen
            let posed = match (&self.mb.anim, self.mb.seq) {
                (Some(a), Some(seq)) if !self.mb.collision && self.mb.anim_path.as_deref() == Some(path) => {
                    let mats = a.skin_matrices(seq, self.mb.frame);
                    Some(std::sync::Arc::new(crate::view3d::static_from_model_posed(m, skin, Some(&mats))))
                }
                _ => None,
            };
            if posed.is_none() {
                let mut rr = renderer.lock().unwrap();
                if !rr.has_static(&key) {
                    let (d, b) = crate::view3d::static_from_model(m, skin);
                    // Kollisionsansicht: einfarbig (ohne Texturen)
                    let b = if self.mb.collision { b.into_iter().map(|mut x| { x.tex = None; x }).collect() } else { b };
                    rr.add_static(key.clone(), d, b);
                }
            }
            if !self.mb.collision {
                for mesh in &m.meshes {
                    if let Some(mat) = m.material_for(self.mb.skin, mesh.skinref) {
                        self.mats.request_gl(mat);
                    }
                }
            }
            let c = m.bounds.center();
            let radius = (m.bounds.size().length() * 0.5).max(4.0) as f32;
            let center = Vec3::new(c.x as f32, c.y as f32, c.z as f32);
            let dist = radius * 2.4 / self.mb.zoom;
            let dir = Vec3::new(self.mb.yaw.cos() * self.mb.pitch.cos(), self.mb.yaw.sin() * self.mb.pitch.cos(), self.mb.pitch.sin());
            let eye = center + dir * dist;
            let proj = Mat4::perspective_rh_gl(40f32.to_radians(), 1.0, (dist - radius * 2.0).max(radius * 0.02), dist + radius * 3.0);
            let view = Mat4::look_at_rh(eye, center, Vec3::Z);
            let mvp = proj * view;
            let cb = eframe::egui_glow::CallbackFn::new(move |_info, painter| match &posed {
                Some(p) => renderer.lock().unwrap().paint_anim_preview(painter.gl(), &p.0, &p.1, mvp, eye),
                None => renderer.lock().unwrap().paint_static_preview(painter.gl(), &key, mvp, eye),
            });
            ui.painter().add(egui::PaintCallback { rect: r.shrink(1.0), callback: std::sync::Arc::new(cb) });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            // Achsenkreuz unten links
            let o = egui::pos2(r.left() + 22.0, r.bottom() - 22.0);
            let ax = th.axis_colors();
            let vrot = Mat4::look_at_rh(dir * 10.0, Vec3::ZERO, Vec3::Z);
            for (k, v) in [Vec3::X, Vec3::Y, Vec3::Z].iter().enumerate() {
                let q = vrot.transform_vector3(*v);
                let e = o + Vec2::new(q.x, -q.y) * 12.0;
                ui.painter().line_segment([o, e], Stroke::new(1.6f32, ax[k]));
                ui.painter().text(e + Vec2::new(q.x, -q.y) * 4.0, egui::Align2::CENTER_CENTER, ["X", "Y", "Z"][k], FontId::new(9.0, semibold()), ax[k]);
            }
        } else {
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, if path.is_some() { crate::i18n::tr("lädt…") } else { crate::i18n::tr("Keine Auswahl") }, sans(12.0), alpha(Color32::WHITE, 0.4));
        }
        // Schilder
        let tag = |txt: &str, at: egui::Pos2, align: egui::Align2| {
            let g = ui.painter().layout_no_wrap(txt.to_string(), sans(11.0), alpha(Color32::WHITE, 0.75));
            let br = align.anchor_size(at, g.size() + Vec2::new(12.0, 6.0));
            ui.painter().rect_filled(br, 6.0, alpha(Color32::BLACK, 0.45));
            ui.painter().galley(br.min + Vec2::new(6.0, 3.0), g, Color32::WHITE);
        };
        tag(crate::i18n::tr("Ziehen dreht · Rad zoomt"), r.left_top() + Vec2::new(8.0, 8.0), egui::Align2::LEFT_TOP);
        // Segment Textur/Kollision oben rechts
        let segw = 122.0;
        let sr = Rect::from_min_size(egui::pos2(r.right() - 8.0 - segw, r.top() + 8.0), Vec2::new(segw, 22.0));
        ui.painter().rect_filled(sr, 6.0, alpha(Color32::BLACK, 0.45));
        for (k, label) in [crate::i18n::tr("Textur"), crate::i18n::tr("Kollision")].iter().enumerate() {
            let cr = Rect::from_min_size(egui::pos2(sr.left() + 2.0 + k as f32 * (segw - 4.0) / 2.0, sr.top() + 2.0), Vec2::new((segw - 4.0) / 2.0, 18.0));
            let on = self.mb.collision == (k == 1);
            let cresp = ui.interact(cr, ui.id().with(("mbseg", k)), Sense::click());
            if on {
                ui.painter().rect_filled(cr, 5.0, alpha(Color32::WHITE, 0.16));
            }
            ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, *label, sans(10.5), if on { Color32::WHITE } else { alpha(Color32::WHITE, 0.6) });
            if cresp.clicked() {
                self.mb.collision = k == 1;
            }
        }
        // Zurücksetzen unten rechts
        let rr = Rect::from_min_size(egui::pos2(r.right() - 34.0, r.bottom() - 34.0), Vec2::splat(26.0));
        let rresp = ui.interact(rr, ui.id().with("mbreset"), Sense::click());
        ui.painter().rect_filled(rr, 6.0, alpha(Color32::BLACK, if rresp.hovered() { 0.6 } else { 0.45 }));
        crate::icons::draw(ui.painter(), rr.shrink(6.0), "restore", alpha(Color32::WHITE, 0.8), 1.4);
        if rresp.on_hover_text(crate::i18n::tr("Ansicht zurücksetzen")).clicked() {
            self.mb.yaw = 0.8;
            self.mb.pitch = 0.35;
            self.mb.zoom = 1.0;
        }
    }

    /// Hat das ausgewählte Modell abspielbare Animationen (mehr als ein Bild)?
    fn anim_ready(&self) -> bool {
        self.mb.anim.as_ref().is_some_and(|a| a.seqs.iter().any(|s| s.frames > 1)) && self.mb.anim_path == self.mb.selected
    }

    /// Animationen des ausgewählten Modells im Hintergrund laden und das Bild weiterzählen
    fn update_anim(&mut self, ctx: &egui::Context, sel: Option<&str>, fs: Option<std::sync::Arc<crate::gamefs::GameFs>>) {
        if self.mb.anim_path.as_deref() != sel {
            self.mb.anim_path = sel.map(|s| s.to_string());
            self.mb.anim = None;
            self.mb.anim_rx = None;
            self.mb.seq = None;
            self.mb.playing = false;
            self.mb.frame = 0.0;
            if let (Some(path), Some(fs)) = (sel, fs) {
                let (tx, rx) = std::sync::mpsc::channel();
                let path = path.to_string();
                let c = ctx.clone();
                std::thread::spawn(move || {
                    let a = fs.read(&path).and_then(|b| crate::mdl_anim::load(&fs, &b));
                    let _ = tx.send((path, a));
                    c.request_repaint();
                });
                self.mb.anim_rx = Some(rx);
            }
        }
        if let Some(rx) = &self.mb.anim_rx {
            if let Ok((p, a)) = rx.try_recv() {
                if Some(p.as_str()) == sel {
                    // Test/Bildschirmfotos: AMBOSS_DEBUG_OPEN=mbsel:…,mbplay spielt die erste Animation ab
                    if std::env::var("AMBOSS_DEBUG_OPEN").is_ok_and(|v| v.contains("mbplay")) {
                        self.mb.seq = a.as_ref().and_then(|a| a.seqs.iter().position(|s| s.frames > 1));
                        self.mb.playing = true;
                    }
                    self.mb.anim = a;
                }
                self.mb.anim_rx = None;
            }
        }
        let now = ctx.input(|i| i.time);
        let dt = self.mb.last_t.map(|t| (now - t) as f32).unwrap_or(0.0).min(0.25);
        self.mb.last_t = Some(now);
        if !self.mb.playing {
            return;
        }
        let Some(s) = self.mb.seq.and_then(|k| self.mb.anim.as_ref()?.seqs.get(k).cloned()) else { return };
        let n = s.frames.max(1) as f32;
        // Nicht schleifende Sequenzen bleiben am letzten Bild kurz stehen und beginnen dann neu
        let end = if s.looping { n } else { n - 1.0 + s.fps * 0.5 };
        self.mb.frame += dt * s.fps;
        if self.mb.frame >= end {
            self.mb.frame = self.mb.frame.rem_euclid(end.max(1.0));
        }
        ctx.request_repaint();
    }

    /// Bedienelemente unter der Vorschau: Sequenz, Abspielen/Anhalten, Bild-Regler
    fn anim_controls(&mut self, ui: &mut egui::Ui) {
        if !self.anim_ready() {
            return;
        }
        let Some(a) = self.mb.anim.clone() else { return };
        let th = t();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(crate::i18n::tr("Animation")).color(th.text2));
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
            ui.painter().rect_filled(r, 6.0, if resp.hovered() { th.hover } else { th.raised });
            if self.mb.playing {
                for dx in [-3.5f32, 3.5] {
                    ui.painter().rect_filled(Rect::from_center_size(r.center() + Vec2::new(dx, 0.0), Vec2::new(3.0, 11.0)), 1.0, th.text);
                }
            } else {
                crate::icons::draw(ui.painter(), r.shrink(7.0), "play", th.text, 1.4);
            }
            let tip = if self.mb.playing { crate::i18n::tr("Anhalten") } else { crate::i18n::tr("Abspielen") };
            if resp.on_hover_text(tip).clicked() {
                self.mb.playing = !self.mb.playing;
                if self.mb.playing && self.mb.seq.is_none() {
                    self.mb.seq = a.seqs.iter().position(|s| s.frames > 1);
                    self.mb.frame = 0.0;
                }
            }
            let cur = match self.mb.seq.and_then(|k| a.seqs.get(k)) {
                Some(s) => s.name.clone(),
                None => crate::i18n::tr("Grundhaltung").to_string(),
            };
            egui::ComboBox::from_id_salt("mb_seq").width(ui.available_width() - 4.0).height(320.0).selected_text(cur).show_ui(ui, |ui| {
                if ui.selectable_label(self.mb.seq.is_none(), crate::i18n::tr("Grundhaltung")).clicked() {
                    self.mb.seq = None;
                    self.mb.playing = false;
                }
                for (k, s) in a.seqs.iter().enumerate() {
                    let label = format!("{}  ({})", s.name, s.frames);
                    if ui.selectable_label(self.mb.seq == Some(k), label).clicked() {
                        self.mb.seq = Some(k);
                        self.mb.frame = 0.0;
                        self.mb.playing = s.frames > 1;
                    }
                }
            });
        });
        let Some(s) = self.mb.seq.and_then(|k| a.seqs.get(k)) else {
            ui.label(RichText::new(crate::i18n::count(a.seqs.len(), "Sequenz", "Sequenzen")).font(mono(11.5)).color(th.text3));
            return;
        };
        ui.horizontal(|ui| {
            let n = s.frames.max(1);
            let mut f = self.mb.frame.min((n - 1) as f32);
            ui.spacing_mut().slider_width = ui.available_width() - 96.0;
            if ui.add(egui::Slider::new(&mut f, 0.0..=(n - 1) as f32).show_value(false)).changed() {
                self.mb.frame = f;
                self.mb.playing = false;
            }
            ui.label(RichText::new(format!("{} / {n} · {} fps", f.floor() as usize + 1, s.fps.round())).font(mono(11.0)).color(th.text3));
        });
    }
}
