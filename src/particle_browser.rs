//! Partikel-Browser nach UI-Konzept: Systeme aus allen .pcf-Dateien, gruppiert nach Datei.

use crate::app::App;
use crate::pcf::ParticleSystem;
use crate::theme::{self, mono, sans, semibold, t};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct ParticleBrowserState {
    pub open: bool,
    pub filter: String,
    /// (Datei, System)
    selected: Option<(usize, usize)>,
    collapsed: BTreeSet<usize>,
    data: Option<Vec<(String, Vec<ParticleSystem>)>>,
    manifest: BTreeSet<String>,
    /// Ziel: (Entity-ID, Schlüssel)
    pub target: Option<(u32, String)>,
}

fn spark(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    for k in 0..4 {
        let a = std::f32::consts::FRAC_PI_4 * k as f32;
        let d = Vec2::new(a.cos(), a.sin()) * if k % 2 == 0 { 6.0 } else { 4.0 };
        p.line_segment([c - d, c + d], Stroke::new(1.3f32, col));
    }
}

impl App {
    fn load_particles(&mut self) {
        let Some(fs) = self.mats.fs() else {
            self.pb.data = Some(Vec::new());
            return;
        };
        let mut v = Vec::new();
        for f in fs.list_files("particles/", &[".pcf"]) {
            if let Some(sys) = fs.read(&f).and_then(|b| crate::pcf::parse(&b)) {
                if !sys.is_empty() {
                    v.push((f, sys));
                }
            }
        }
        let mut man = BTreeSet::new();
        for mf in ["particles/particles_manifest.txt", "particles/particles_manifest_base.txt"] {
            if let Some(b) = fs.read(mf) {
                for line in String::from_utf8_lossy(&b).lines() {
                    if let Some(p) = line.split('"').filter(|s| s.to_ascii_lowercase().ends_with(".pcf")).next_back() {
                        man.insert(p.trim_start_matches('!').to_ascii_lowercase().replace('\\', "/"));
                    }
                }
            }
        }
        self.pb.manifest = man;
        self.pb.data = Some(v);
    }

    pub(crate) fn particle_browser(&mut self, ctx: &egui::Context) {
        if !self.pb.open {
            return;
        }
        if self.pb.data.is_none() {
            self.load_particles();
        }
        let th = t();
        let data = self.pb.data.take().unwrap_or_default();
        let q = self.pb.filter.trim().to_ascii_lowercase();
        let nfiles = data.len();
        let nsys: usize = data.iter().map(|d| d.1.len()).sum();
        let hits: usize = if q.is_empty() { nsys } else { data.iter().map(|d| d.1.iter().filter(|s| s.name.to_ascii_lowercase().contains(&q)).count()).sum() };
        let mut open = true;
        let mut apply = false;
        let mut edit = false;
        let mut cancel = false;
        let d = theme::Dlg::new(crate::i18n::tr("Partikel-Browser"), 760.0).height(520.0).footer().header_extra().no_scroll();
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Header => {
                ui.add(egui::TextEdit::singleline(&mut self.pb.filter).hint_text(crate::theme::hint(crate::i18n::tr("Systeme filtern…"))).desired_width(260.0));
            }
            theme::Part::Body => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let lw = 760.0 - 260.0 - 1.0;
                    ui.allocate_ui_with_layout(Vec2::new(lw, 408.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(lw);
                        if data.is_empty() {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| ui.label(RichText::new(crate::i18n::tr("Keine .pcf-Dateien im Spielordner gefunden (particles/).")).color(th.text3)));
                            return;
                        }
                        egui::ScrollArea::vertical().id_salt("pb_list").auto_shrink([false, false]).show(ui, |ui| {
                            egui::Frame::new().inner_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 8 }).show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let w = ui.available_width();
                                for (fi, (file, systems)) in data.iter().enumerate() {
                                    let shown: Vec<usize> = (0..systems.len()).filter(|&i| q.is_empty() || systems[i].name.to_ascii_lowercase().contains(&q)).collect();
                                    if shown.is_empty() {
                                        continue;
                                    }
                                    let collapsed = q.is_empty() && self.pb.collapsed.contains(&fi);
                                    let (hr, hresp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
                                    if hresp.hovered() {
                                        ui.painter().rect_filled(hr, 6.0, th.hover);
                                    }
                                    let cc = egui::pos2(hr.left() + 12.0, hr.center().y);
                                    let pts = if collapsed { vec![cc + Vec2::new(-2.0, -4.0), cc + Vec2::new(3.0, 0.0), cc + Vec2::new(-2.0, 4.0)] } else { vec![cc + Vec2::new(-4.0, -2.0), cc + Vec2::new(4.0, -2.0), cc + Vec2::new(0.0, 3.0)] };
                                    ui.painter().add(egui::Shape::convex_polygon(pts, th.text3, Stroke::NONE));
                                    // Datei-Symbol
                                    let fr = Rect::from_center_size(egui::pos2(hr.left() + 30.0, hr.center().y), Vec2::new(10.0, 13.0));
                                    ui.painter().rect_stroke(fr, 2.0, Stroke::new(1.2f32, th.text3), StrokeKind::Inside);
                                    let fname = file.trim_start_matches("particles/");
                                    ui.painter().text(egui::pos2(hr.left() + 44.0, hr.center().y), egui::Align2::LEFT_CENTER, fname, FontId::new(12.0, egui::FontFamily::Monospace), th.text);
                                    ui.painter().text(egui::pos2(hr.right() - 8.0, hr.center().y), egui::Align2::RIGHT_CENTER, shown.len().to_string(), sans(11.5), th.text3);
                                    if hresp.clicked() {
                                        if collapsed {
                                            self.pb.collapsed.remove(&fi);
                                        } else {
                                            self.pb.collapsed.insert(fi);
                                        }
                                    }
                                    if collapsed {
                                        continue;
                                    }
                                    for si in shown {
                                        let s = &systems[si];
                                        let on = self.pb.selected == Some((fi, si));
                                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
                                        if on {
                                            ui.painter().rect(r, 6.0, th.accent_soft, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                                        } else if resp.hovered() {
                                            ui.painter().rect_filled(r, 6.0, th.hover);
                                        }
                                        spark(ui.painter(), egui::pos2(r.left() + 40.0, r.center().y), th.warn);
                                        let mut job = egui::text::LayoutJob::default();
                                        let nf = egui::TextFormat { font_id: mono(12.0), color: th.text, ..Default::default() };
                                        let lower = s.name.to_ascii_lowercase();
                                        match (!q.is_empty()).then(|| lower.find(&q)).flatten() {
                                            Some(p) => {
                                                job.append(&s.name[..p], 0.0, nf.clone());
                                                job.append(&s.name[p..p + q.len()], 0.0, egui::TextFormat { color: th.accent_text, background: th.accent_soft, ..nf.clone() });
                                                job.append(&s.name[p + q.len()..], 0.0, nf);
                                            }
                                            None => job.append(&s.name, 0.0, nf),
                                        }
                                        job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 140.0);
                                        let g = ui.painter().layout_job(job);
                                        ui.painter().galley(egui::pos2(r.left() + 54.0, r.center().y - g.size().y / 2.0), g, th.text);
                                        if !s.children.is_empty() {
                                            ui.painter().text(egui::pos2(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::count(s.children.len(), "Kind", "Kinder"), sans(11.5), th.text3);
                                        }
                                        if resp.clicked() {
                                            self.pb.selected = Some((fi, si));
                                        }
                                        if resp.double_clicked() {
                                            apply = true;
                                        }
                                    }
                                }
                            });
                        });
                    });
                    let (sep, _) = ui.allocate_exact_size(Vec2::new(1.0, 408.0), Sense::hover());
                    ui.painter().rect_filled(sep, 0.0, th.line);
                    ui.allocate_ui_with_layout(Vec2::new(260.0, 408.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(260.0);
                        egui::Frame::new().inner_margin(16).show(ui, |ui| {
                            let Some((fi, si)) = self.pb.selected.filter(|(f, s)| data.get(*f).is_some_and(|d| *s < d.1.len())) else {
                                ui.label(RichText::new(crate::i18n::tr("Links ein Partikelsystem wählen.")).color(th.text3));
                                return;
                            };
                            let (file, systems) = &data[fi];
                            let s = &systems[si];
                            ui.label(RichText::new(&s.name).font(FontId::new(13.5, egui::FontFamily::Monospace)).strong());
                            ui.label(RichText::new(crate::i18n::tr("Partikelsystem")).color(th.text3).size(11.5));
                            ui.add_space(10.0);
                            let in_man = self.pb.manifest.contains(&file.to_ascii_lowercase());
                            for (k, v, c) in [
                                ("Datei", file.trim_start_matches("particles/").to_string(), th.text),
                                ("Kinder-Systeme", s.children.len().to_string(), th.text),
                                ("Im Manifest", if in_man { "ja".into() } else { "nein".into() }, if in_man { th.ok } else { th.warn }),
                            ] {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
                                ui.painter().text(egui::pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, k, sans(12.5), th.text2);
                                ui.painter().text(egui::pos2(r.right(), r.center().y), egui::Align2::RIGHT_CENTER, v, mono(11.5), c);
                                ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0f32, th.line));
                            }
                            ui.add_space(12.0);
                            ui.label(RichText::new(crate::i18n::tr("AUFBAU")).font(FontId::new(11.0, semibold())).color(th.text3));
                            ui.add_space(4.0);
                            let mut lines = vec![s.name.clone()];
                            fn tree(name: &str, prefix: &str, systems: &[ParticleSystem], out: &mut Vec<String>, depth: usize) {
                                let Some(s) = systems.iter().find(|x| x.name == name) else { return };
                                if depth > 6 {
                                    return;
                                }
                                for (i, c) in s.children.iter().enumerate() {
                                    let last = i + 1 == s.children.len();
                                    out.push(format!("{prefix}{}{c}", if last { "└ " } else { "├ " }));
                                    tree(c, &format!("{prefix}{}", if last { "  " } else { "│ " }), systems, out, depth + 1);
                                }
                            }
                            tree(&s.name, "", systems, &mut lines, 0);
                            egui::ScrollArea::vertical().id_salt("pb_tree").max_height(150.0).show(ui, |ui| {
                                for l in lines.iter().take(60) {
                                    ui.label(RichText::new(l).font(mono(11.5)).color(th.text2));
                                }
                            });
                            ui.add_space(8.0);
                            ui.label(RichText::new(crate::i18n::tr("Wird als effect_name in info_particle_system eingetragen.")).size(11.5).color(th.text3));
                        });
                    });
                });
            }
            theme::Part::Footer => {
                if ui.add_enabled(self.pb.selected.is_some(), egui::Button::new(RichText::new(crate::i18n::tr("Übernehmen")).family(semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    apply = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    cancel = true;
                }
                if ui.add_enabled(self.pb.selected.is_some(), egui::Button::new(crate::i18n::tr("Im Partikel-Editor öffnen"))).clicked() {
                    edit = true;
                }
                theme::footer_hint(ui, &format!("{nfiles} .pcf-Dateien · {nsys} Systeme{}", if q.is_empty() { String::new() } else { format!(" · {hits} Treffer") }));
            }
        });
        if edit {
            if let Some((fi, si)) = self.pb.selected {
                if let Some((file, _)) = data.get(fi) {
                    let loose = std::path::Path::new(&self.settings.game_dir).join(file);
                    if loose.is_file() {
                        self.open_particle_file(Some(loose));
                    } else if let Some(b) = self.mats.fs().and_then(|fs| fs.read(file)) {
                        // aus VPK/Addon: bearbeiten und unter neuem Namen speichern
                        match crate::dmx::read(&b) {
                            Ok(d) => {
                                self.parted.dmx = d;
                                self.parted.path = None;
                                self.parted.open = true;
                                self.parted.dirty = true;
                                self.parted.status = crate::i18n::tr("Aus den Spielinhalten geladen – bitte unter neuem Namen speichern").into();
                            }
                            Err(e) => self.ed.status = e,
                        }
                    }
                    self.parted.selected = si;
                    self.parted.sim.reset();
                }
            }
        }
        if apply {
            if let Some((fi, si)) = self.pb.selected {
                if let Some(s) = data.get(fi).and_then(|d| d.1.get(si)) {
                    let name = s.name.clone();
                    match self.pb.target.clone() {
                        Some((id, key)) => {
                            self.ed.checkpoint();
                            if let Some(e) = self.ed.map.entity_mut(id) {
                                e.set(&key, name.clone());
                            }
                            self.ed.touch();
                        }
                        None => {
                            // ausgewählte info_particle_system bekommen das System, sonst in die Zwischenablage
                            let ids: Vec<u32> = self.ed.selected_entity_ids().into_iter().filter(|id| self.ed.map.entity(*id).is_some_and(|e| e.classname() == "info_particle_system")).collect();
                            if ids.is_empty() {
                                ctx.copy_text(name.clone());
                                self.ed.status = format!("Partikelsystem {name} in die Zwischenablage kopiert");
                            } else {
                                self.ed.checkpoint();
                                for id in ids {
                                    if let Some(e) = self.ed.map.entity_mut(id) {
                                        e.set("effect_name", name.clone());
                                    }
                                }
                                self.ed.touch();
                                self.ed.status = format!("Partikelsystem: {name}");
                            }
                        }
                    }
                    open = false;
                }
            }
        }
        if cancel {
            open = false;
        }
        self.pb.data = Some(data);
        self.pb.open = open;
    }
}
