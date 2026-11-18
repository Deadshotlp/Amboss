//! Fensterrahmen im Amboss-Design: obere Leiste, Tabs, Werkzeugleiste, Kontextleiste, Statusleiste.

use crate::app::App;
use crate::compile::JobState;
use crate::editor::{ClipMode, SelMode, Tool, XMode};
use crate::icons;
use crate::theme::{self, alpha, mono, sans, semibold, t};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, RichText, Sense, Stroke, StrokeKind, Vec2};

/// Icon-Name und Gruppe je Werkzeug
fn tool_icon(t: Tool) -> &'static str {
    match t {
        Tool::Select => "select",
        Tool::Magnify => "magnify",
        Tool::Camera => "camera",
        Tool::Entity => "entity",
        Tool::Block => "block",
        Tool::Face => "texture",
        Tool::Decal => "decal",
        Tool::Overlay => "overlay",
        Tool::Clip => "clip",
        Tool::Vertex => "vertex",
        Tool::Disp => "disp",
        Tool::Cordon => "cordon",
        Tool::Measure => "measure",
    }
}

/// Einfache Oberfläche: nur die Grundwerkzeuge
const TOOL_GROUPS_SIMPLE: [&[Tool]; 3] = [&[Tool::Select], &[Tool::Block, Tool::Entity, Tool::Face, Tool::Decal], &[Tool::Clip]];

/// Werkzeuge der aktuellen Oberfläche
pub(crate) fn tool_groups(simple: bool) -> &'static [&'static [Tool]] {
    if simple { &TOOL_GROUPS_SIMPLE } else { &TOOL_GROUPS }
}

const TOOL_GROUPS: [&[Tool]; 4] = [
    &[Tool::Select, Tool::Magnify, Tool::Camera],
    &[Tool::Entity, Tool::Block, Tool::Face, Tool::Decal, Tool::Overlay],
    &[Tool::Clip, Tool::Vertex, Tool::Disp],
    &[Tool::Cordon, Tool::Measure],
];

fn tool_title(t: Tool) -> &'static str {
    match t {
        Tool::Select => crate::i18n::tr("Auswahl"),
        Tool::Magnify => crate::i18n::tr("Lupe"),
        Tool::Camera => crate::i18n::tr("Kamera"),
        Tool::Entity => crate::i18n::tr("Entity platzieren"),
        Tool::Block => crate::i18n::tr("Block-Werkzeug"),
        Tool::Face => crate::i18n::tr("Textur anwenden"),
        Tool::Decal => crate::i18n::tr("Decal"),
        Tool::Overlay => crate::i18n::tr("Overlay"),
        Tool::Clip => crate::i18n::tr("Clip"),
        Tool::Vertex => crate::i18n::tr("Vertex bearbeiten"),
        Tool::Disp => crate::i18n::tr("Displacement & Sculpt"),
        Tool::Cordon => crate::i18n::tr("Cordon"),
        Tool::Measure => crate::i18n::tr("Messen"),
    }
}

/// Tooltip im Konzept-Stil: Text + Tastenkürzel
fn tip(resp: egui::Response, text: &str, key: &str) -> egui::Response {
    let (text, key) = (text.to_string(), key.replace('+', " "));
    resp.on_hover_ui(move |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&text).size(12.0));
            if !key.is_empty() {
                ui.label(RichText::new(&key).font(mono(10.5)).color(t().text3));
            }
        });
    })
}

/// Quadratischer Icon-Knopf
fn icon_button(ui: &mut egui::Ui, icon: &str, size: f32, active: bool, radius: f32) -> egui::Response {
    let th = t();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let (bg, fg) = if active {
        (th.accent_soft, th.accent_text)
    } else if resp.hovered() {
        (th.hover, th.text)
    } else {
        (Color32::TRANSPARENT, th.text2)
    };
    ui.painter().rect_filled(rect, radius, bg);
    icons::draw(ui.painter(), rect.shrink(size * 0.25), icon, fg, 1.75);
    resp
}

/// Kasten-Knopf mit Rahmen (z. B. Theme-Umschalter)
fn boxed_icon_button(ui: &mut egui::Ui, icon: &str, size: f32) -> egui::Response {
    let th = t();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    ui.painter().rect(rect, 8.0, if resp.hovered() { th.hover } else { th.raised }, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
    icons::draw(ui.painter(), rect.shrink(size * 0.25), icon, th.text2, 1.9);
    resp
}

/// Knopf mit Text und Pfeil nach unten (für Auswahlmenüs)
pub fn dropdown_button(ui: &mut egui::Ui, text: &str, height: f32, size: f32, color: Color32, fill: Color32) -> egui::Response {
    let th = t();
    let g = ui.painter().layout_no_wrap(text.to_string(), sans(size), color);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 34.0, height), Sense::click());
    ui.painter().rect(r, 7.0, if resp.hovered() { th.hover } else { fill }, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
    ui.painter().galley(egui::pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, color);
    icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(r.right() - 13.0, r.center().y), Vec2::splat(12.0)), "chevron", th.text3, 2.2);
    resp
}

fn initials(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => crate::i18n::tr("gerade eben").into(),
        60..=3599 => crate::i18n::tr_dyn(&format!("vor {} Min.", secs / 60)),
        _ => crate::i18n::tr_dyn(&format!("vor {} Std.", secs / 3600)),
    }
}

impl App {
    // ------------------------------------------------------------------ obere Leiste

    pub(crate) fn top_bar(&mut self, ctx: &egui::Context) {
        let th = t();
        egui::TopBottomPanel::top("topbar").exact_height(44.0).frame(egui::Frame::new().fill(th.panel).inner_margin(Margin::symmetric(12, 0))).show(ctx, |ui| {
            let full = ui.max_rect();
            // rechte Gruppe zuerst: Kompilieren, Theme, Live
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(full).layout(egui::Layout::right_to_left(egui::Align::Center)));
            right.spacing_mut().item_spacing.x = 10.0;
            self.compile_split_button(&mut right);
            let icon = if th.dark { "sun" } else { "moon" };
            let label = if th.dark { "Zu hellem Design wechseln" } else { "Zu dunklem Design wechseln" };
            if boxed_icon_button(&mut right, icon, 32.0).on_hover_text(label).clicked() {
                self.settings.theme_dark = !self.settings.theme_dark;
                theme::apply(ctx, self.settings.theme_dark);
                self.ed.touch();
            }
            self.live_chip(&mut right);
            let right_left = right.min_rect().left();
            // linke Gruppe: Logo und Menüs
            let left_rect = egui::Rect::from_min_max(full.min, egui::pos2(right_left - 12.0, full.max.y));
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
            left.spacing_mut().item_spacing.x = 10.0;
            let (r, _) = left.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            left.painter().rect_filled(r, 8.0, th.accent);
            icons::draw(left.painter(), r.shrink(5.0), "anvil", th.accent_fg, 2.0);
            left.label(RichText::new(crate::i18n::tr("Amboss")).font(FontId::new(14.0, theme::bold())));
            left.add_space(-6.0);
            left.label(RichText::new(env!("CARGO_PKG_VERSION")).font(theme::mono(10.5)).color(th.text3)).on_hover_text(crate::i18n::tr("Version – Hilfe → Über Amboss"));
            left.add_space(4.0);
            left.scope(|ui| {
                let v = &mut ui.style_mut().visuals.widgets;
                v.inactive.fg_stroke.color = th.text2;
                v.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.inactive.bg_stroke = Stroke::NONE;
                v.hovered.bg_stroke = Stroke::NONE;
                v.open.bg_stroke = Stroke::NONE;
                ui.spacing_mut().button_padding = Vec2::new(10.0, 6.0);
                ui.spacing_mut().item_spacing.x = 18.0;
                ui.visuals_mut().button_frame = false;
                self.menus(ui, ctx);
            });
            let left_right = left.cursor().left();
            // Befehlssuche dazwischen, möglichst mittig im Fenster
            let (lo, hi) = (left_right + 16.0, right_left - 16.0);
            if hi - lo >= 140.0 {
                let w = 400.0f32.min(hi - lo);
                let x = (ui.ctx().screen_rect().center().x - w / 2.0).clamp(lo, hi - w);
                let sr = egui::Rect::from_min_size(egui::pos2(x, full.center().y - 15.0), Vec2::new(w, 30.0));
                let sresp = ui.interact(sr, ui.id().with("cmdsearch"), Sense::click());
                ui.painter().rect(sr, 8.0, if sresp.hovered() { th.hover } else { th.raised }, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
                icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(sr.left() + 18.0, sr.center().y), Vec2::splat(15.0)), "search", th.text3, 2.0);
                let placeholder = if w > 300.0 { crate::i18n::tr("Befehle, Entities, Texturen suchen…") } else { crate::i18n::tr("Suchen…") };
                ui.painter().with_clip_rect(sr.shrink(2.0)).text(egui::pos2(sr.left() + 34.0, sr.center().y), Align2::LEFT_CENTER, placeholder, sans(12.5), th.text3);
                let kbd = egui::Rect::from_min_size(egui::pos2(sr.right() - 52.0, sr.center().y - 9.0), Vec2::new(46.0, 18.0));
                ui.painter().rect(kbd, 5.0, th.panel, Stroke::new(1.0f32, th.line_strong), StrokeKind::Inside);
                ui.painter().text(kbd.center(), Align2::CENTER_CENTER, crate::i18n::keys("Strg K"), mono(10.5), th.text3);
                if sresp.on_hover_text(crate::i18n::tr("Befehlspalette (Strg+K / Strg+P)")).clicked() {
                    self.dlg.palette_filter.clear();
                    self.win.palette = true;
                }
            }
        });
    }

    fn live_chip(&mut self, ui: &mut egui::Ui) {
        let th = t();
        let live = self.collab.as_ref().filter(|c| c.connected);
        let text = match live {
            Some(c) => format!("Live · {}", c.participants()),
            None => crate::i18n::tr("Live-Sitzung").into(),
        };
        let g = ui.painter().layout_no_wrap(text, sans(11.5), th.text2);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 26.0, 24.0), Sense::click());
        ui.painter().rect(r, 12.0, if resp.hovered() { th.hover } else { th.raised }, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
        ui.painter().circle_filled(egui::pos2(r.left() + 11.0, r.center().y), 3.5, if live.is_some() { th.ok } else { th.text3 });
        ui.painter().galley(egui::pos2(r.left() + 19.0, r.center().y - g.size().y / 2.0), g, th.text2);
        if resp.on_hover_text(crate::i18n::tr("Live-Sitzung starten oder beitreten")).clicked() {
            self.win.collab = true;
        }
        if let Some(c) = live {
            // Avatare (rechts nach links gezeichnet)
            let people: Vec<(String, Color32)> = c.users.iter().rev().map(|u| (u.name.clone(), Color32::from_rgb(u.color[0], u.color[1], u.color[2]))).collect();
            let n = people.len();
            let w = if n == 0 { 0.0 } else { 26.0 + (n as f32 - 1.0) * 18.0 };
            let (ar, _) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::hover());
            for (i, (name, col)) in people.iter().enumerate() {
                let cx = ar.right() - 13.0 - i as f32 * 18.0;
                let p = egui::pos2(cx, ar.center().y);
                ui.painter().circle(p, 13.0, *col, Stroke::new(2.0f32, th.panel));
                ui.painter().text(p, Align2::CENTER_CENTER, initials(name), FontId::new(11.0, semibold()), Color32::WHITE);
            }
        }
    }

    fn compile_split_button(&mut self, ui: &mut egui::Ui) {
        let th = t();
        let running = self.job.as_ref().is_some_and(|j| j.state() == JobState::Running);
        // Dropdown (rechts)
        let (dr, dresp) = ui.allocate_exact_size(Vec2::new(28.0, 32.0), Sense::click());
        ui.painter().rect_filled(dr, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, if dresp.hovered() { theme::alpha(th.accent, 0.85) } else { th.accent });
        ui.painter().line_segment([dr.left_top() + Vec2::new(0.0, 6.0), dr.left_bottom() - Vec2::new(0.0, 6.0)], Stroke::new(1.0f32, alpha(th.accent_fg, 0.25)));
        icons::draw(ui.painter(), dr.shrink(7.0), "chevron", th.accent_fg, 2.2);
        let popup = ui.make_persistent_id("compile_profiles");
        if dresp.on_hover_text(crate::i18n::tr("Kompilier-Profil wählen")).clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        // Hauptknopf
        let label = if running { "Kompiliert…" } else { crate::i18n::tr("Kompilieren & starten") };
        let g = ui.painter().layout_no_wrap(label.to_string(), FontId::new(13.0, semibold()), th.accent_fg);
        ui.spacing_mut().item_spacing.x = 0.0;
        let (mr, mresp) = ui.allocate_exact_size(Vec2::new(g.size().x + 40.0, 32.0), Sense::click());
        self.ui_rects.insert("compile", mr);
        ui.painter().rect_filled(mr, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, if mresp.hovered() { theme::alpha(th.accent, 0.85) } else { th.accent });
        let ic = egui::Rect::from_center_size(egui::pos2(mr.left() + 17.0, mr.center().y), Vec2::splat(13.0));
        if running {
            let a = ui.input(|i| i.time) as f32 * 5.0;
            ui.painter().add(egui::Shape::circle_stroke(ic.center(), 5.0, Stroke::new(2.0f32, alpha(th.accent_fg, 0.3))));
            ui.painter().line_segment([ic.center(), ic.center() + Vec2::angled(a) * 5.0], Stroke::new(2.0f32, th.accent_fg));
            ui.ctx().request_repaint();
        } else {
            let pts = vec![egui::pos2(ic.left() + 2.0, ic.top()), egui::pos2(ic.right(), ic.center().y), egui::pos2(ic.left() + 2.0, ic.bottom())];
            ui.painter().add(egui::Shape::convex_polygon(pts, th.accent_fg, Stroke::NONE));
        }
        ui.painter().galley(egui::pos2(mr.left() + 30.0, mr.center().y - g.size().y / 2.0), g, th.accent_fg);
        ui.spacing_mut().item_spacing.x = 10.0;
        if mresp.on_hover_text(crate::i18n::tr("F9 – Map speichern, kompilieren und im Spiel starten")).clicked() {
            if running {
                self.win.compile = true;
            } else {
                self.start_compile();
            }
        }
        egui::popup::popup_below_widget(ui, popup, &dresp_dummy(ui, dr), egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(240.0);
            theme::section(ui, crate::i18n::tr("Spielprofil"));
            let names: Vec<String> = self.settings.profiles.iter().map(|p| p.name.clone()).collect();
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_label(i == self.settings.active_profile, n).clicked() {
                    self.settings.load_profile(i);
                    self.reload_content();
                    self.reload_fgds();
                }
            }
            ui.separator();
            if ui.button(crate::i18n::tr("Kompilieren-Fenster…")).clicked() {
                self.win.compile = true;
            }
            if ui.button(crate::i18n::tr("Automatische Optimierungen…")).clicked() {
                self.win.optimize = true;
            }
            crate::theme::checkbox(ui, &mut self.settings.launch_game, crate::i18n::tr("Danach Spiel starten"));
        });
    }

    // ------------------------------------------------------------------ Tabs

    pub(crate) fn tab_strip(&mut self, ctx: &egui::Context) {
        let th = t();
        let mut switch = None;
        let mut close = None;
        egui::TopBottomPanel::top("tabs").exact_height(36.0).show_separator_line(true).frame(egui::Frame::new().fill(th.bg).inner_margin(Margin { left: 8, right: 12, top: 0, bottom: 0 })).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let bottom = ui.max_rect().bottom();
                for i in 0..self.tabs.len() {
                    let (name, dirty) = match &self.tabs[i] {
                        Some(tb) => (tb.ed.display_name(), tb.ed.dirty),
                        None => (self.ed.display_name(), self.ed.dirty),
                    };
                    let active = i == self.active_tab;
                    let g = ui.painter().layout_no_wrap(name.clone(), mono(12.0), if active { th.text } else { th.text3 });
                    let w = g.size().x + 22.0 + 18.0;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
                    let tab = egui::Rect::from_min_max(egui::pos2(r.left(), bottom - 30.0), egui::pos2(r.right(), bottom + 1.0));
                    if active {
                        ui.painter().rect(tab, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, th.panel, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
                        ui.painter().line_segment([tab.left_bottom() + Vec2::new(1.0, -1.0), tab.right_bottom() + Vec2::new(-1.0, -1.0)], Stroke::new(2.0f32, th.panel));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(tab, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, alpha(th.hover, 0.6));
                    }
                    let ty = tab.center().y;
                    ui.painter().galley(egui::pos2(tab.left() + 12.0, ty - g.size().y / 2.0), g, if active { th.text } else { th.text3 });
                    // Punkt = ungespeichert, × beim Darüberfahren
                    let xr = egui::Rect::from_center_size(egui::pos2(tab.right() - 14.0, ty), Vec2::splat(16.0));
                    let xresp = ui.interact(xr, ui.id().with(("tabx", i)), Sense::click());
                    if resp.hovered() || xresp.hovered() {
                        if xresp.hovered() {
                            ui.painter().rect_filled(xr, 4.0, th.hover);
                        }
                        icons::draw(ui.painter(), xr.shrink(3.0), "close", th.text2, 2.0);
                    } else if dirty {
                        ui.painter().circle_filled(xr.center(), 3.5, th.accent);
                    }
                    if xresp.clicked() {
                        close = Some(i);
                    } else if resp.clicked() {
                        switch = Some(i);
                    }
                    resp.on_hover_text(match &self.tabs[i] {
                        Some(tb) => tb.ed.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| crate::i18n::tr("nicht gespeichert").into()),
                        None => self.ed.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| crate::i18n::tr("nicht gespeichert").into()),
                    });
                }
                if icon_button(ui, "plus", 28.0, false, 6.0).on_hover_text(crate::i18n::tr("Neuer Tab (Strg+N)")).clicked() {
                    switch = Some(usize::MAX);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let pname = self.settings.profiles.get(self.settings.active_profile).map(|p| p.name.clone()).unwrap_or_else(|| crate::i18n::tr("Kein Profil").into());
                    let resp = dropdown_button(ui, &format!("{}: {pname}", crate::i18n::tr("Spielprofil")), 24.0, 11.5, th.text2, th.panel);
                    let pop = ui.make_persistent_id("profile_pop");
                    if resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(pop));
                    }
                    egui::popup::popup_below_widget(ui, pop, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(200.0);
                        let names: Vec<String> = self.settings.profiles.iter().map(|p| p.name.clone()).collect();
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == self.settings.active_profile, n).clicked() {
                                self.settings.load_profile(i);
                                self.reload_content();
                                self.reload_fgds();
                            }
                        }
                        ui.separator();
                        if ui.button(crate::i18n::tr("Einstellungen…")).clicked() {
                            self.win.settings = true;
                        }
                    });
                    let saved = match (&self.ed.path, self.last_saved) {
                        (None, _) => crate::i18n::tr("Noch nicht gespeichert").to_string(),
                        (Some(_), Some(t0)) if !self.ed.dirty => format!("Gespeichert {}", ago(t0.elapsed().as_secs())),
                        (Some(_), Some(t0)) => format!("Letzte Speicherung {}", ago(t0.elapsed().as_secs())),
                        (Some(_), None) => if self.ed.dirty { crate::i18n::tr("Ungespeicherte Änderungen").into() } else { crate::i18n::tr("Gespeichert").into() },
                    };
                    ui.label(RichText::new(saved).size(11.5).color(th.text3));
                });
            });
        });
        match switch {
            Some(usize::MAX) => self.new_tab(),
            Some(i) => self.switch_tab(i),
            None => {}
        }
        if let Some(i) = close {
            self.close_tab(ctx, i);
        }
    }

    // ------------------------------------------------------------------ Werkzeugleiste

    pub(crate) fn tool_rail(&mut self, ctx: &egui::Context) {
        let th = t();
        egui::SidePanel::left("tools").resizable(false).exact_width(52.0).frame(egui::Frame::new().fill(th.panel).inner_margin(Margin::symmetric(7, 8))).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let groups = tool_groups(self.settings.simple_ui);
                // Werkzeug der Experten-Oberfläche aktiv, aber hier ausgeblendet → zurück zur Auswahl
                if !groups.iter().any(|g| g.contains(&self.ed.tool)) {
                    self.set_tool(Tool::Select);
                }
                for (gi, group) in groups.iter().enumerate() {
                    if gi > 0 {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(24.0, 9.0), Sense::hover());
                        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0f32, th.line));
                    }
                    for tool in group.iter() {
                        let resp = icon_button(ui, tool_icon(*tool), 38.0, self.ed.tool == *tool, 9.0);
                        let key = match tool {
                            Tool::Block => Some("tool:block"),
                            Tool::Entity => Some("tool:entity"),
                            Tool::Face => Some("tool:face"),
                            Tool::Clip => Some("tool:clip"),
                            Tool::Select => Some("tool:select"),
                            Tool::Disp => Some("tool:disp"),
                            Tool::Vertex => Some("tool:vertex"),
                            Tool::Overlay => Some("tool:overlay"),
                            Tool::Decal => Some("tool:decal"),
                            _ => None,
                        };
                        if let Some(k) = key {
                            self.ui_rects.insert(k, resp.rect);
                        }
                        if tip(resp, tool_title(*tool), tool.shortcut()).clicked() {
                            self.set_tool(*tool);
                        }
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if tip(icon_button(ui, "settings", 38.0, self.win.settings, 9.0), crate::i18n::tr("Einstellungen"), "").clicked() {
                        self.win.settings = !self.win.settings;
                    }
                });
            });
        });
    }

    // ------------------------------------------------------------------ Kontextleiste

    pub(crate) fn context_bar(&mut self, ctx: &egui::Context) {
        let th = t();
        egui::TopBottomPanel::top("context").exact_height(44.0).frame(egui::Frame::new().fill(th.panel).inner_margin(Margin::symmetric(12, 0))).show(ctx, |ui| {
            let full = ui.max_rect();
            // rechts: Visgroups und Layout
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(full).layout(egui::Layout::right_to_left(egui::Align::Center)));
            {
                let ui = &mut right;
                ui.spacing_mut().item_spacing.x = 8.0;
                // Layout 1 / 2×2
                let (lr, _) = ui.allocate_exact_size(Vec2::new(60.0, 28.0), Sense::hover());
                ui.painter().rect(lr, 7.0, th.raised, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
                let one = egui::Rect::from_min_size(lr.min + Vec2::new(3.0, 3.0), Vec2::new(26.0, 22.0));
                let four = egui::Rect::from_min_size(lr.min + Vec2::new(31.0, 3.0), Vec2::new(26.0, 22.0));
                let single = self.maximized.is_some();
                for (r, ic, on, id, text) in [(one, "one_view", single, "lay1", crate::i18n::tr("Eine Ansicht (Shift+Z)")), (four, "four_views", !single, "lay4", crate::i18n::tr("Vier Ansichten"))] {
                    let resp = ui.interact(r, ui.id().with(id), Sense::click());
                    if on {
                        ui.painter().rect_filled(r, 5.0, th.tab_active);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 5.0, th.hover);
                    }
                    icons::draw(ui.painter(), r.shrink(4.0), ic, if on { th.text } else { th.text3 }, 1.8);
                    if resp.on_hover_text(text).clicked() {
                        self.maximized = if id == "lay1" { Some(self.hovered_view.or(self.active_view).unwrap_or(0)) } else { None };
                    }
                }
                if !self.settings.simple_ui {
                    self.visgroup_chip(ui);
                }
            }
            let left_rect = egui::Rect::from_min_max(full.min, egui::pos2(right.min_rect().left() - 10.0, full.max.y));
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(left_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
            left.set_clip_rect(left_rect);
            // Passt nicht alles? Stufe 1: Schalter nur als Symbol, ohne "Textur"/"Raster"; Stufe 2: auch ohne Werkzeugname.
            // Werkzeug oder Textur gewechselt → Breiten neu messen
            let sig = format!("{:?}|{}|{}|{}", self.ed.tool, self.ed.material, crate::i18n::lang().code(), self.settings.simple_ui);
            if sig != self.ctx_sig {
                self.ctx_sig = sig;
                self.ctx_widths = [0.0; 3];
            }
            let avail = left_rect.width();
            let level = (0..3).find(|&l| self.ctx_widths[l] <= avail).unwrap_or(2);
            let start_x = left_rect.left();
            {
                let ui = &mut left;
                ui.spacing_mut().item_spacing.x = 6.0;
                if level < 2 {
                    ui.label(RichText::new(tool_title(self.ed.tool)).family(semibold()));
                }
                self.tool_controls(ui);
                vsep(ui);
                // Rückgängig / Wiederholen
                for redo in [false, true] {
                    let enabled = if redo { self.ed.can_redo() } else { self.ed.can_undo() };
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::click());
                    if resp.hovered() && enabled {
                        ui.painter().rect_filled(r, 7.0, th.hover);
                    }
                    undo_icon(ui.painter(), r, if enabled { th.text } else { th.text3.gamma_multiply(0.6) }, redo);
                    let (label, cmd) = if redo { (crate::i18n::tr("Wiederholen"), crate::commands::Cmd::Redo) } else { (crate::i18n::tr("Rückgängig"), crate::commands::Cmd::Undo) };
                    let key = crate::commands::display_shortcut(&self.key_for(cmd));
                    if tip(resp, label, &key).clicked() && enabled {
                        if redo {
                            self.ed.redo();
                        } else {
                            self.ed.undo();
                        }
                    }
                }
                vsep(ui);
                // nur, was für das Werkzeug zählt: Lupe/Kamera/Textur brauchen weder Raster noch Einrasten
                let tool = self.ed.tool;
                let grid_rel = !matches!(tool, Tool::Magnify | Tool::Camera | Tool::Face);
                let vsnap_rel = matches!(tool, Tool::Select | Tool::Block | Tool::Vertex | Tool::Clip | Tool::Entity | Tool::Measure | Tool::Cordon);
                let tlock_rel = matches!(tool, Tool::Select | Tool::Vertex);
                if grid_rel {
                // Raster
                if level == 0 {
                    ui.label(RichText::new(crate::i18n::tr("Raster")).color(th.text3));
                }
                let (gr, _) = ui.allocate_exact_size(Vec2::new(84.0, 28.0), Sense::hover());
                ui.painter().rect(gr, 7.0, th.raised, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
                let minus = egui::Rect::from_min_size(gr.min, Vec2::new(26.0, 28.0));
                let plus = egui::Rect::from_min_size(egui::pos2(gr.right() - 26.0, gr.top()), Vec2::new(26.0, 28.0));
                let mr = ui.interact(minus, ui.id().with("gridm"), Sense::click());
                let pr = ui.interact(plus, ui.id().with("gridp"), Sense::click());
                for (r, resp, ic) in [(minus, &mr, "minus"), (plus, &pr, "plus")] {
                    if resp.hovered() {
                        ui.painter().rect_filled(r.shrink(2.0), 5.0, th.hover);
                    }
                    icons::draw(ui.painter(), r.shrink(7.0), ic, th.text2, 2.0);
                }
                ui.painter().text(gr.center(), Align2::CENTER_CENTER, crate::map::fmt_num(self.ed.grid), FontId::new(12.0, egui::FontFamily::Monospace), th.text);
                if tip(mr, crate::i18n::tr("Raster verkleinern"), "[").clicked() {
                    self.ed.grid_down();
                }
                if tip(pr, crate::i18n::tr("Raster vergrößern"), "]").clicked() {
                    self.ed.grid_up();
                }
                }
                let chip = |ui: &mut egui::Ui, on: bool, icon: &str, label: &str, tip: &str| -> bool {
                    if level == 0 {
                        theme::toggle_chip(ui, on, Some(icon), label, tip)
                    } else {
                        theme::toggle_chip(ui, on, Some(icon), "", &format!("{label} – {tip}"))
                    }
                };
                if grid_rel && chip(ui, self.ed.snap, "magnet", crate::i18n::tr("Einrasten"), crate::i18n::tr("Am Raster einrasten (Alt+G)")) {
                    self.ed.snap = !self.ed.snap;
                }
                if !self.settings.simple_ui {
                    if vsnap_rel && chip(ui, self.ed.vertex_snap, "target", crate::i18n::tr("An Eckpunkten"), crate::i18n::tr("An Eckpunkten anderer Objekte einrasten (Alt+V)")) {
                        self.ed.vertex_snap = !self.ed.vertex_snap;
                    }
                    if tlock_rel && chip(ui, self.ed.tex_lock, "lock", crate::i18n::tr("Texturen fixieren"), crate::i18n::tr("Textur-Lock: Texturen beim Verschieben/Drehen mitbewegen")) {
                        self.ed.tex_lock = !self.ed.tex_lock;
                    }
                }
            }
            // Breite dieser Stufe merken; schmalere Stufen gelten danach neu (Werkzeugwechsel ändert die Breite)
            self.ctx_widths[level] = left.min_rect().right() - start_x;
            // passt diese Stufe nicht, gleich noch ein Bild mit der nächsten zeichnen (egui zeichnet sonst erst beim nächsten Ereignis)
            if self.ctx_widths[level] > avail && level < 2 {
                ctx.request_repaint();
            }
        });
    }

    /// Werkzeugabhängige Bedienelemente in der Kontextleiste
    fn tool_controls(&mut self, ui: &mut egui::Ui) {
        let th = t();
        match self.ed.tool {
            Tool::Select => {
                let idx = match self.ed.sel_mode {
                    SelMode::Groups => 0,
                    SelMode::Objects => 1,
                    SelMode::Solids => 2,
                };
                let names = [crate::i18n::tr("Gruppen"), crate::i18n::tr("Objekte"), "Einzelne Brushes"];
                let resp = dropdown_button(ui, names[idx], 28.0, 12.5, th.text, th.raised).on_hover_text(crate::i18n::tr("Auswahlmodus: was ein Klick auswählt"));
                let pop = ui.make_persistent_id("selmode_pop");
                if resp.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(pop));
                }
                egui::popup::popup_below_widget(ui, pop, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(160.0);
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(i == idx, *n).clicked() {
                            self.ed.sel_mode = [SelMode::Groups, SelMode::Objects, SelMode::Solids][i];
                        }
                    }
                });
                let has_sel = !self.ed.sel.is_empty();
                ui.add_enabled_ui(has_sel, |ui| {
                    ui.menu_button(crate::i18n::tr("Aktionen"), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::tr("Griffe:"));
                            for m in [XMode::Scale, XMode::Rotate, XMode::Skew] {
                                ui.radio_value(&mut self.ed.xmode, m, m.label());
                            }
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(crate::i18n::tr("Aushöhlen")).on_hover_text(crate::i18n::keys("Strg+H")).clicked() {
                                self.ed.hollow(self.hollow_thickness);
                                ui.close_menu();
                            }
                            ui.add(crate::theme::dv(egui::DragValue::new(&mut self.hollow_thickness)).range(-256.0..=256.0).prefix("Wand ")).on_hover_text(crate::i18n::tr("Negativ = Wände außen"));
                        });
                        let items: [(&str, &str, u8); 9] = [
                            (crate::i18n::tr("Carve"), "Strg+Shift+C", 1),
                            (crate::i18n::tr("Gruppieren"), "Strg+G", 2),
                            (crate::i18n::tr("Gruppe lösen"), "Strg+U", 3),
                            (crate::i18n::tr("In Entity umwandeln"), "Strg+T", 4),
                            (crate::i18n::tr("Zurück in Welt"), "Strg+Shift+W", 5),
                            (crate::i18n::tr("Transformieren…"), "Strg+M", 6),
                            (crate::i18n::tr("Ausblenden"), "H", 7),
                            (crate::i18n::tr("Sperren"), "L", 8),
                            (crate::i18n::tr("Berührende hinzufügen"), "Strg+Alt+A", 9),
                        ];
                        for (label, key, id) in items {
                            if ui.add(egui::Button::new(label).shortcut_text(crate::i18n::keys(key))).clicked() {
                                match id {
                                    1 => self.ed.carve(),
                                    2 => self.ed.group_selection(),
                                    3 => self.ed.ungroup_selection(),
                                    4 => {
                                        let c = self.tie_class.clone();
                                        self.ed.tie_to_entity(&c);
                                    }
                                    5 => self.ed.move_to_world(),
                                    6 => self.win.transform = true,
                                    7 => self.ed.hide_selection(),
                                    8 => self.ed.lock_selection(true),
                                    _ => self.ed.select_touching(),
                                }
                                ui.close_menu();
                            }
                        }
                    });
                });
            }
            Tool::Block => {
                ui.label(RichText::new(crate::i18n::tr("Form")).color(th.text3));
                let p = &mut self.ed.shape;
                egui::ComboBox::from_id_salt("shape_ctx").selected_text(p.shape.label()).width(110.0).show_ui(ui, |ui| {
                    for s in crate::primitives::Shape::ALL {
                        ui.selectable_value(&mut p.shape, s, s.label());
                    }
                });
            }
            Tool::Clip => {
                let idx = match self.ed.clip_mode {
                    ClipMode::Both => 0,
                    ClipMode::Front => 1,
                    ClipMode::Back => 2,
                };
                let n = theme::segmented(ui, idx, &[crate::i18n::tr("Beide"), crate::i18n::tr("Vorne"), crate::i18n::tr("Hinten")], false);
                self.ed.clip_mode = [ClipMode::Both, ClipMode::Front, ClipMode::Back][n];
                if ui.add_enabled(self.ed.clip_plane().is_some(), egui::Button::new(crate::i18n::tr("Schneiden"))).on_hover_text(crate::i18n::tr("Enter")).clicked() {
                    self.ed.apply_clip();
                }
            }
            // Entity: die Klasse wird rechts im Panel gewählt (Feld mit Vorschlägen und Liste)
            Tool::Vertex => {
                let n = self.ed.vsel.len();
                let t = if n == 0 { crate::i18n::tr("Keine Eckpunkte ausgewählt").to_string() } else { crate::i18n::tr_dyn(&format!("{} ausgewählt", crate::i18n::count(n, "Eckpunkt", "Eckpunkte"))) };
                ui.label(RichText::new(t).color(th.text2));
            }
            Tool::Disp => {
                ui.label(RichText::new(self.ed.dbrush.mode.label()).color(th.text2));
            }
            _ => {}
        }
    }


    fn visgroup_chip(&mut self, ui: &mut egui::Ui) {
        let th = t();
        let total = self.ed.map.visgroups.len();
        let hidden = self.ed.map.visgroups.iter().filter(|v| !v.visible).count();
        let text = if total == 0 {
            crate::i18n::tr("Visgroups").to_string()
        } else if hidden == 0 {
            format!("Visgroups ({total})")
        } else {
            format!("Visgroups {}/{total}", total - hidden)
        };
        let resp = dropdown_button(ui, &text, 28.0, 12.5, th.text, th.raised);
        let pop = ui.make_persistent_id("vis_pop");
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(pop));
        }
        egui::popup::popup_below_widget(ui, pop, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(220.0);
            if self.ed.map.visgroups.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("Keine Visgroups – im Panel unter „Visgroups“ anlegen")).color(th.text3));
            }
            let mut changed = false;
            for v in &mut self.ed.map.visgroups {
                let col = Color32::from_rgb(v.color[0], v.color[1], v.color[2]);
                ui.horizontal(|ui| {
                    theme::dot(ui, col);
                    changed |= crate::theme::checkbox(ui, &mut v.visible, &v.name).changed();
                });
            }
            if changed {
                self.ed.sel.retain(|_| true);
                self.ed.touch();
            }
            ui.separator();
            if ui.button(crate::i18n::tr("Alles einblenden")).clicked() {
                self.ed.unhide_all();
                for v in &mut self.ed.map.visgroups {
                    v.visible = true;
                }
                self.ed.touch();
            }
            if ui.button(crate::i18n::tr("Visgroups verwalten…")).clicked() {
                self.panel_tab = 2;
                ui.memory_mut(|m| m.close_popup());
            }
        });
    }

    // ------------------------------------------------------------------ Statusleiste

    pub(crate) fn status_line(&mut self, ctx: &egui::Context) {
        let th = t();
        egui::TopBottomPanel::bottom("status").exact_height(28.0).frame(egui::Frame::new().fill(th.panel).inner_margin(Margin::symmetric(12, 0))).show(ctx, |ui| {
            let full = ui.max_rect();
            let split = full.left() + (full.width() * 0.32).max(260.0);
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(full.min, egui::pos2(split - 14.0, full.max.y))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            left.set_clip_rect(egui::Rect::from_min_max(full.min, egui::pos2(split - 14.0, full.max.y)));
            {
                let ui = &mut left;
                ui.spacing_mut().item_spacing.x = 18.0;
                let small = |s: String| RichText::new(s).size(11.5).color(th.text2);
                let running = self.job.as_ref().is_some_and(|j| j.state() == JobState::Running);
                let low = self.ed.status.to_ascii_lowercase();
                let col = if running {
                    th.accent
                } else if low.contains("fehler") || low.contains("leck") || low.contains("abgelehnt") || low.contains(crate::i18n::tr("ungültig")) {
                    th.warn
                } else {
                    th.ok
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    theme::dot(ui, col);
                    // Meldungen verschwinden nach 8 s (die letzte bleibt als Tooltip lesbar)
                    if self.status_seen.0 != self.ed.status {
                        self.status_seen = (self.ed.status.clone(), std::time::Instant::now());
                    }
                    let age = self.status_seen.1.elapsed().as_secs_f32();
                    let last = if self.ed.status.is_empty() { String::new() } else { crate::i18n::tr_dyn(&self.ed.status) };
                    let status = if self.ed.status.is_empty() || age > 8.0 { crate::i18n::tr("Bereit").to_string() } else { last.clone() };
                    if age <= 8.0 {
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f32(8.1 - age));
                    }
                    let r = ui.add(egui::Label::new(small(status.clone())).truncate());
                    if !last.is_empty() {
                        r.on_hover_text(last);
                    }
                });
            }
            // Mitte und rechte Seite in eigenem Bereich, damit lange Meldungen sie nicht verdrängen
            let mid = egui::Rect::from_min_max(egui::pos2(split, full.top()), full.max);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(mid).layout(egui::Layout::left_to_right(egui::Align::Center)));
            let ui = &mut child;
            ui.spacing_mut().item_spacing.x = 18.0;
            let ax = th.axis_colors();
            if let Some((axis, p)) = self.cursor {
                let (h, v, _) = axis.axes();
                let n = ["X", "Y", "Z"];
                let mut job = egui::text::LayoutJob::default();
                for (i, (k, val)) in [(h, p.x), (v, p.y)].into_iter().enumerate() {
                    if i > 0 {
                        job.append("  ", 0.0, egui::TextFormat::simple(mono(11.0), th.text2));
                    }
                    job.append(n[k], 0.0, egui::TextFormat::simple(mono(11.0), ax[k]));
                    job.append(&format!(" {}", crate::map::fmt_num(val.round())), 0.0, egui::TextFormat::simple(mono(11.0), th.text2));
                }
                ui.label(job);
                if let Some(h) = self.sky_cursor_hint(axis, p) {
                    ui.label(RichText::new(h).font(mono(11.0)).color(th.accent_text));
                }
            }
            let sel = self.ed.sel.len();
            if sel > 0 {
                let class = self.ed.selected_entity_ids().first().and_then(|id| self.ed.map.entity(*id)).map(|e| e.classname().to_string());
                let text = match (sel, class) {
                    (1, Some(c)) => crate::i18n::tr_dyn(&format!("Auswahl: 1 Objekt ({c})")),
                    (1, None) => crate::i18n::tr("Auswahl: 1 Objekt").into(),
                    (n, _) => crate::i18n::tr_dyn(&format!("Auswahl: {n} Objekte")),
                };
                ui.label(RichText::new(text).size(11.5).color(th.text2));
            }
            let grid = crate::i18n::tr_dyn(&if self.ed.snap { format!("Raster {}", crate::map::fmt_num(self.ed.grid)) } else { format!("Raster {} (frei)", crate::map::fmt_num(self.ed.grid)) });
            if ui.add(egui::Label::new(RichText::new(grid).size(11.5).color(th.text2)).sense(Sense::click())).on_hover_text(crate::i18n::tr("Klick: Einrasten an/aus (Alt+G)")).clicked() {
                self.ed.snap = !self.ed.snap;
            }
            if self.ed.map.cordon.is_some_and(|c| c.active) {
                ui.label(RichText::new(crate::i18n::tr("Cordon aktiv")).size(11.5).color(th.warn));
            }
            if !self.ed.auto_vis.all_visible() {
                let hidden = crate::sky_ui::hidden_labels(&self.ed.auto_vis);
                if ui.add(egui::Label::new(RichText::new(crate::i18n::tr("Filter aktiv")).size(11.5).color(th.warn)).sense(Sense::click())).on_hover_text(crate::i18n::tr_dyn(&format!("Ausgeblendet: {hidden} – Klick: alles einblenden"))).clicked() {
                    self.ed.auto_vis = crate::editor::AutoVis::default();
                    self.ed.touch();
                }
            }
            if self.sky.active {
                ui.label(RichText::new(crate::i18n::tr("3D-Skybox-Modus")).size(11.5).color(th.accent_text));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                // Live-Personen
                if let Some(c) = self.collab.as_ref().filter(|c| c.connected) {
                    let users: Vec<(String, [u8; 3])> = c.users.iter().map(|u| (u.name.clone(), u.color)).collect();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (n, col) in users.iter().rev() {
                            ui.label(RichText::new(n).size(11.5).color(th.text2));
                            theme::dot(ui, Color32::from_rgb(col[0], col[1], col[2]));
                        }
                    });
                }
                // Spielinhalte
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let (text, col) = if self.content_rx.is_some() {
                        (crate::i18n::tr("Spielinhalte laden…").to_string(), th.accent)
                    } else if self.mats.list.is_empty() {
                        (crate::i18n::tr("Keine Spielinhalte").to_string(), th.warn)
                    } else {
                        (crate::i18n::tr_dyn(&format!("{} Materialien", fmt_thousands(self.mats.list.len()))), th.ok)
                    };
                    let tip = if self.content_status.is_empty() { format!("Spielordner: {}", if self.settings.game_dir.is_empty() { crate::i18n::tr("nicht eingestellt") } else { &self.settings.game_dir }) } else { crate::i18n::tr_dyn(&self.content_status) };
                    if ui.add(egui::Label::new(RichText::new(text).size(11.5).color(th.text2)).sense(Sense::click())).on_hover_text(crate::i18n::tr_dyn(&format!("{tip}\nKlick: Einstellungen"))).clicked() {
                        self.win.settings = true;
                    }
                    if self.content_rx.is_some() {
                        ui.spinner();
                    } else {
                        theme::dot(ui, col);
                    }
                });
                let mcp_on = self.mcp.is_some();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(if mcp_on { crate::i18n::tr("MCP verbunden") } else { crate::i18n::tr("MCP aus") }).size(11.5).color(th.text2)).on_hover_text(crate::i18n::tr_dyn(&self.mcp_status));
                    theme::dot(ui, if mcp_on { th.ok } else { th.text3 });
                });
                if self.mats.is_loading() || self.models.is_loading() {
                    ui.spinner();
                }
                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Brushes · {} Entities", fmt_thousands(self.ed.map.all_solids().count()), fmt_thousands(self.ed.map.entities.len())))).font(mono(11.0)).color(th.text3));
            });
        });
    }
}

fn vsep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0f32, t().line));
}

/// 1284 -> "1.284"
pub fn fmt_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Antwort-Objekt für Popups, die unter einem selbst gezeichneten Bereich erscheinen
fn dresp_dummy(ui: &egui::Ui, rect: egui::Rect) -> egui::Response {
    ui.interact(rect, ui.id().with(("popup_anchor", rect.min.x as i32, rect.min.y as i32)), Sense::hover())
}

#[allow(dead_code)]
fn _unused(_: StrokeKind) {}

/// Pfeil für Rückgängig (gebogen nach links) bzw. Wiederholen (gespiegelt)
fn undo_icon(p: &egui::Painter, r: egui::Rect, col: Color32, mirror: bool) {
    let c = r.center() + Vec2::new(0.0, 1.5);
    let sx = if mirror { -1.0 } else { 1.0 };
    let rad = 6.0f32;
    // Bogen von links über oben nach rechts unten
    let pts: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0 * 1.25);
            egui::pos2(c.x + sx * rad * a.cos(), c.y + rad * a.sin())
        })
        .collect();
    let start = pts[0];
    p.add(egui::Shape::line(pts, Stroke::new(1.7f32, col)));
    // Pfeilspitze am linken Ende, zeigt nach unten
    p.add(egui::Shape::line(vec![start + Vec2::new(-sx * 3.5, -2.5), start + Vec2::new(0.0, 2.0), start + Vec2::new(sx * 3.5, -2.5)], Stroke::new(1.7f32, col)));
}
