//! Rechtes Eigenschaften-Panel: Material, Werkzeug-Optionen, Visgroups, Auswahl/Entity-Editor.

use crate::app::App;
use crate::disp::{PaintMode, FLAG_NO_HULL, FLAG_NO_PHYS, FLAG_NO_RAY};
use crate::editor::{ClipMode, Justify, Obj, Tool, XMode};
use crate::fgd::Fgd;
use crate::map::{default_tex_axes, fmt_vec, parse_vec, Entity, Map};
use crate::primitives::Shape;
use crate::theme;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use glam::{DQuat, DVec3};

pub const QUICK_MATERIALS: &[&str] = &[
    "DEV/DEV_MEASUREGENERIC01B",
    "DEV/DEV_MEASUREGENERIC01",
    "DEV/GRAYGRID",
    "DEV/DEV_MEASUREWALL01A",
    "DEV/DEV_MEASUREWALL01C",
    "DEV/REFLECTIVITY_50",
    "TOOLS/TOOLSNODRAW",
    "TOOLS/TOOLSSKYBOX",
    "TOOLS/TOOLSTRIGGER",
    "TOOLS/TOOLSCLIP",
    "TOOLS/TOOLSPLAYERCLIP",
    "TOOLS/TOOLSNPCCLIP",
    "TOOLS/TOOLSINVISIBLE",
    "TOOLS/TOOLSBLACK",
    "TOOLS/TOOLSHINT",
    "TOOLS/TOOLSSKIP",
    "TOOLS/TOOLSAREAPORTAL",
    "TOOLS/TOOLSBLOCKLIGHT",
    "CONCRETE/CONCRETEFLOOR001A",
    "BRICK/BRICKWALL001A",
    "NATURE/BLENDGRASSGRAVEL001A",
    "METAL/METALFLOOR001A",
];

/// Aktionen, die der Entity-Editor anfordert
pub(crate) enum EdAction {
    Pick(String),
    BrowseModel(String),
    BrowseSound(String),
    BrowseParticle(String),
    Help,
    /// Klassenliste öffnen (Ankerfläche des Knopfs)
    ClassList(egui::Rect),
    RotateYaw(f64),
    CopyProps,
    PasteProps,
}

impl App {
    pub(crate) fn properties_panel(&mut self, ctx: &egui::Context) {
        let th = theme::t();
        if !self.panel_width_init {
            // Breite gehört dem Nutzer (Einstellungen), nicht dem gespeicherten egui-Zustand – der konnte
            // früher durch zu breiten Inhalt wachsen und bliebe sonst für immer so
            self.panel_width_init = true;
            ctx.data_mut(|d| d.remove::<egui::containers::panel::PanelState>(egui::Id::new("props")));
            if !(300.0..=560.0).contains(&self.settings.layout.right_width) {
                self.settings.layout.right_width = 360.0;
            }
        }
        let resp = egui::SidePanel::right("props").resizable(true).default_width(self.settings.layout.right_width.clamp(300.0, 560.0)).width_range(300.0..=560.0).frame(egui::Frame::new().fill(th.panel)).show(ctx, |ui| {
            // Inhalt wird frei gezeichnet – ohne das schrumpft ein ziehbares Panel auf die Mindestbreite
            ui.set_min_width(ui.available_width());
            let full = ui.max_rect();
            // Segment-Tabs
            let tabs_rect = egui::Rect::from_min_size(full.min, Vec2::new(full.width(), 50.0));
            let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect.shrink2(Vec2::new(10.0, 8.0))));
            if self.settings.simple_ui {
                // einfache Oberfläche: keine Visgroups
                self.panel_tab = self.panel_tab.min(1);
                self.panel_tab = theme::segmented(&mut tabs, self.panel_tab, &[crate::i18n::tr("Eigenschaften"), crate::i18n::tr("Objekte")], true);
            } else {
                self.panel_tab = theme::segmented(&mut tabs, self.panel_tab, &[crate::i18n::tr("Eigenschaften"), crate::i18n::tr("Objekte"), crate::i18n::tr("Visgroups")], true);
            }
            ui.painter().hline(full.x_range(), tabs_rect.bottom(), Stroke::new(1.0f32, th.line));
            // Texturen unten, fest – beim Textur-Werkzeug fast das ganze Panel (Flächen-Einstellungen
            // nur aufgeklappt darüber)
            let face_mode = self.panel_tab == 0 && self.ed.tool == Tool::Face;
            let avail = full.bottom() - tabs_rect.bottom();
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("amboss_simple_ui"), self.settings.simple_ui));
            let tex_h = if face_mode {
                if self.face_opts_open { (avail * 0.55).max(260.0).min(avail - 120.0) } else { avail - 46.0 }
            } else if self.panel_tab == 0 {
                // höhenverstellbar (Trennlinie ziehen) und einklappbar
                if self.settings.layout.tex_collapsed { 44.0 } else { self.settings.layout.tex_h.clamp(120.0, (avail - 140.0).max(120.0)) }
            } else {
                0.0
            };
            let body = egui::Rect::from_min_max(egui::pos2(full.left(), tabs_rect.bottom() + 1.0), egui::pos2(full.right(), full.bottom() - tex_h));
            self.ui_rects.insert("panel", body);
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            if face_mode {
                // Kopfzeile: Flächen-Einstellungen auf-/zuklappen
                let n = self.ed.faces.len();
                let mut hui = bui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(body.min, Vec2::new(body.width(), 44.0)).shrink2(Vec2::new(14.0, 8.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let label = if self.face_opts_open { crate::i18n::tr("Flächen-Einstellungen ausblenden") } else { crate::i18n::tr("Flächen-Einstellungen anzeigen") };
                if hui.add(egui::Button::new(RichText::new(label).strong()).frame(false)).on_hover_text(crate::i18n::tr("Skalierung, Verschiebung, Drehung, Ausrichten, Lightmap …")).clicked() {
                    self.face_opts_open = !self.face_opts_open;
                }
                hui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(crate::i18n::count_de(n, crate::i18n::tr("Fläche gewählt"), crate::i18n::tr("Flächen gewählt"))).color(th.text3));
                });
            }
            let body_inner = if face_mode { egui::Rect::from_min_max(body.min + Vec2::new(0.0, 44.0), body.max) } else { body };
            let mut bui = ui.new_child(egui::UiBuilder::new().max_rect(body_inner));
            if face_mode && !self.face_opts_open {
                // nur Kopfzeile
            } else {
            egui::ScrollArea::vertical().id_salt(("panel", self.panel_tab)).auto_shrink([false, false]).show(&mut bui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 14, right: 14, top: 12, bottom: 10 }).show(ui, |ui| {
                    // Breite folgt dem (ziehbaren) Panel; 16 px für die Bildlaufleiste samt Abstand
                    ui.set_width((body.width() - 28.0 - 26.0).max(240.0));
                    match self.panel_tab {
                        0 if face_mode => self.face_section(ui),
                        0 => {
                            self.selection_header(ui);
                            self.tool_options(ui);
                            self.selection_section(ui);
                        }
                        1 => self.outliner_contents(ui, ctx),
                        _ => self.visgroup_section(ui),
                    }
                });
            });
            }
            if tex_h > 0.0 {
                let tr = egui::Rect::from_min_max(egui::pos2(full.left(), full.bottom() - tex_h), full.max);
                self.ui_rects.insert("textures", tr);
                ui.painter().hline(full.x_range(), tr.top(), Stroke::new(1.0f32, th.line));
                // Trennlinie ziehen = Höhe des Texturbereichs ändern
                if !face_mode && !self.settings.layout.tex_collapsed {
                    let grip = egui::Rect::from_min_max(egui::pos2(full.left(), tr.top() - 4.0), egui::pos2(full.right(), tr.top() + 4.0));
                    let gr = ui.interact(grip, ui.id().with("tex_split"), egui::Sense::drag());
                    if gr.hovered() || gr.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        ui.painter().hline(full.x_range(), tr.top(), Stroke::new(2.0f32, th.accent));
                    }
                    if gr.dragged() {
                        self.settings.layout.tex_h = (tex_h - gr.drag_delta().y).clamp(120.0, (avail - 140.0).max(120.0));
                    }
                    if gr.double_clicked() {
                        self.settings.layout.tex_h = 250.0;
                    }
                }
                let mut tui = ui.new_child(egui::UiBuilder::new().max_rect(tr.shrink2(Vec2::new(14.0, 12.0))));
                self.texture_grid(&mut tui, face_mode);
            }
        });
        self.settings.layout.right_width = resp.response.rect.width();
    }

    /// Kopf des Eigenschaften-Tabs: Symbol, Name, Art
    fn selection_header(&mut self, ui: &mut egui::Ui) {
        let th = theme::t();
        let ents = self.ed.selected_entity_ids();
        let solids: Vec<u32> = self.ed.selected_solid_ids();
        let (icon, name, sub) = if self.ed.sel.is_empty() {
            ("select", crate::i18n::tr("Nichts ausgewählt").to_string(), crate::i18n::tr("Objekt in einer Ansicht anklicken").to_string())
        } else if ents.len() == 1 && self.ed.sel.len() <= 1 + self.ed.map.entity(ents[0]).map(|e| e.solids.len()).unwrap_or(0) {
            let e = self.ed.map.entity(ents[0]).unwrap();
            let faces: usize = e.solids.iter().map(|s| s.sides.len()).sum();
            let kind = if e.is_brush_entity() { crate::i18n::tr_dyn(&format!("Brush-Entity · {faces} Flächen")) } else { crate::i18n::tr("Punkt-Entity").to_string() };
            (crate::icons::entity_icon(e.classname()), e.targetname().unwrap_or(e.classname()).to_string(), format!("{} · {kind}", e.classname()))
        } else if ents.is_empty() {
            let faces: usize = solids.iter().filter_map(|id| self.ed.map.solid(*id)).map(|s| s.sides.len()).sum();
            ("block", if solids.len() == 1 { format!("Brush {}", solids[0]) } else { crate::i18n::count(solids.len(), "Brush", "Brushes") }, crate::i18n::tr_dyn(&format!("Welt · {faces} Flächen")))
        } else {
            ("layers", crate::i18n::count(self.ed.sel.len(), "Objekt", "Objekte"), format!("{}, {}", crate::i18n::count(ents.len(), "Entity", "Entities"), crate::i18n::count(solids.len(), "Brush", "Brushes")))
        };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            ui.painter().rect_filled(r, 9.0, th.accent_soft);
            crate::icons::draw(ui.painter(), r.shrink(9.0), icon, th.accent_text, 1.9);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                // Mono nur für Entity-/Objektnamen, nicht für Zustände wie „Nichts ausgewählt“
                let font = if icon == "select" || icon == "layers" { egui::FontId::new(14.0, theme::semibold()) } else { egui::FontId::new(14.0, egui::FontFamily::Monospace) };
                ui.add(egui::Label::new(RichText::new(name).font(font).color(th.text)).truncate());
                ui.add(egui::Label::new(RichText::new(sub).size(12.0).color(th.text3)).truncate());
            });
        });
        ui.add_space(6.0);
        if self.ed.sel.is_empty() {
            if ui.button(crate::i18n::tr("Map-Eigenschaften (worldspawn)…")).clicked() {
                self.win.world_props = true;
            }
        }
    }

    /// Werkzeug-Optionen, die nicht in die Kontextleiste passen
    fn tool_options(&mut self, ui: &mut egui::Ui) {
        if matches!(self.ed.tool, Tool::Select) {
            return;
        }
        theme::section(ui, &crate::i18n::tr_dyn(&format!("Werkzeug · {}", self.ed.tool.label())));
        self.tool_section(ui);
        ui.add_space(8.0);
    }

    /// Textur-Raster unten im Panel (4 Spalten); `big` = Textur-Werkzeug: ganzes Panel, alle
    /// Materialien, so viele Spalten wie passen
    fn texture_grid(&mut self, ui: &mut egui::Ui, big: bool) {
        let th = theme::t();
        ui.horizontal(|ui| {
            if !big {
                // Texturbereich ein-/ausklappen
                let collapsed = self.settings.layout.tex_collapsed;
                let (r, resp) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
                let c = r.center();
                let pts = if collapsed { vec![c + Vec2::new(-4.0, 2.0), c + Vec2::new(0.0, -2.0), c + Vec2::new(4.0, 2.0)] } else { vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(0.0, 2.0), c + Vec2::new(4.0, -2.0)] };
                ui.painter().add(egui::Shape::line(pts, Stroke::new(1.5f32, if resp.hovered() { th.text } else { th.text3 })));
                if resp.on_hover_text(if collapsed { crate::i18n::tr("Texturen aufklappen") } else { crate::i18n::tr("Texturen einklappen (Trennlinie ziehen ändert die Höhe)") }).clicked() {
                    self.settings.layout.tex_collapsed = !collapsed;
                }
            }
            theme::section(ui, crate::i18n::tr("Texturen"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(crate::i18n::tr("Browser…")).clicked() {
                    self.win.browser = true;
                }
                ui.add(egui::TextEdit::singleline(&mut self.tex_filter).hint_text(crate::theme::hint(crate::i18n::tr("filtern"))).desired_width(130.0));
            });
        });
        if !big && self.settings.layout.tex_collapsed {
            return;
        }
        let f = self.tex_filter.to_ascii_lowercase();
        let mut list: Vec<String> = if f.is_empty() {
            let mut v: Vec<String> = self.settings.recent_materials.iter().map(|m| m.to_ascii_lowercase()).collect();
            v.extend(self.settings.favorite_materials.iter().map(|m| m.to_ascii_lowercase()));
            if big {
                v.extend(self.mats.list.iter().cloned());
            } else {
                v.extend(self.mats.list.iter().take(40).cloned());
            }
            if v.is_empty() {
                v.extend(QUICK_MATERIALS.iter().map(|m| m.to_ascii_lowercase()));
            }
            v
        } else {
            self.mats.list.iter().filter(|m| m.contains(&f)).take(if big { usize::MAX } else { 200 }).cloned().collect()
        };
        let mut seen = std::collections::HashSet::new();
        list.retain(|m| seen.insert(m.clone()));
        let cur = self.ed.material.to_ascii_lowercase();
        let cols = if big { ((ui.available_width() + 8.0) / 92.0).floor().clamp(3.0, 6.0) as usize } else { 4 };
        let cell = (ui.available_width() - (cols - 1) as f32 * 8.0) / cols as f32;
        if big {
            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Materialien · Klick: auswählen und auf gewählte Flächen anwenden", list.len()))).small().color(th.text3));
        }
        let rows = list.len().div_ceil(cols);
        let row_h = cell + 3.0 + 12.0;
        egui::ScrollArea::vertical().id_salt(("texgrid", big)).auto_shrink([false, false]).show_rows(ui, row_h, rows, |ui, range| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for row in range {
                ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for m in list.iter().skip(row * cols).take(cols) {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                        match self.mats.thumb(m) {
                            Some(tex) => {
                                egui::Image::new((tex.id(), r.size())).corner_radius(7).paint_at(ui, r);
                            }
                            None => {
                                let c = crate::view3d::material_color(m);
                                ui.painter().rect_filled(r, 7.0, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8));
                            }
                        }
                        if *m == cur {
                            ui.painter().rect_stroke(r.expand(1.0), 8.0, Stroke::new(2.0f32, th.accent), egui::StrokeKind::Outside);
                        } else if resp.hovered() {
                            ui.painter().rect_stroke(r, 7.0, Stroke::new(1.0f32, th.line_strong), egui::StrokeKind::Outside);
                        }
                        let short = m.rsplit('/').next().unwrap_or(m).to_string();
                        ui.add_sized([cell, 12.0], egui::Label::new(RichText::new(short).font(egui::FontId::new(10.0, egui::FontFamily::Monospace)).color(th.text2)).truncate());
                        let resp = resp.on_hover_text(crate::i18n::tr_dyn(&format!("{m}\nKlick: auswählen · Doppelklick: auf Auswahl anwenden")));
                        if resp.double_clicked() {
                            self.ed.material = m.to_ascii_uppercase();
                            self.ed.apply_material();
                        } else if resp.clicked() {
                            self.ed.material = m.to_ascii_uppercase();
                            // Textur-Werkzeug: Klick wendet direkt auf die gewählten Flächen an
                            if big && !self.ed.faces.is_empty() {
                                self.ed.apply_material();
                            }
                        }
                    });
                }
                });
            }
        });
    }

    fn tool_section(&mut self, ui: &mut egui::Ui) {
        match self.ed.tool {
            Tool::Select => {
                ui.label(crate::i18n::tr("Klick auf eine Auswahl wechselt die Griffe:"));
                ui.horizontal(|ui| {
                    for m in [XMode::Scale, XMode::Rotate, XMode::Skew] {
                        ui.radio_value(&mut self.ed.xmode, m, m.label());
                    }
                });
                ui.label(RichText::new(crate::i18n::tr("Drehen rastet in 15°-Schritten ein (Shift = frei).")).small());
            }
            Tool::Magnify => {
                ui.label(crate::i18n::tr("Linksklick vergrößert, Rechtsklick verkleinert (2D)."));
            }
            Tool::Camera => self.camera_section(ui),
            Tool::Block => self.block_section(ui),
            Tool::Entity => self.entity_tool_section(ui),
            Tool::Face => self.face_section(ui),
            Tool::Decal => {
                ui.label(crate::i18n::tr("Material rechts bei den Texturen wählen (z. B. decals/…), dann in 3D auf eine Fläche klicken."));
            }
            Tool::Overlay => {
                ui.label(crate::i18n::tr("Material rechts bei den Texturen wählen, dann in 3D auf eine Brush-Fläche klicken.\nGröße und Ausrichtung danach mit den Griffen in der Ansicht oder in den Eigenschaften (uv0–uv3)."));
            }
            Tool::Clip => {
                ui.label(crate::i18n::tr("In 2D eine Schnittlinie ziehen oder in 3D drei Punkte auf Flächen anklicken (schräge Schnitte). Beide/Vorne/Hinten bestimmt, welche Teile bleiben. Enter schneidet."));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.ed.clip_mode, ClipMode::Both, crate::i18n::tr("Beide"));
                    ui.radio_value(&mut self.ed.clip_mode, ClipMode::Front, crate::i18n::tr("Vorne"));
                    ui.radio_value(&mut self.ed.clip_mode, ClipMode::Back, crate::i18n::tr("Hinten"));
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.ed.clip_plane().is_some(), egui::Button::new(crate::i18n::tr("Schneiden (Enter)"))).clicked() {
                        self.ed.apply_clip();
                    }
                    if ui.button(crate::i18n::tr("Linie löschen")).clicked() {
                        self.ed.clip = None;
                        self.ed.clip3.clear();
                        self.ed.touch();
                    }
                });
            }
            Tool::Vertex => {
                ui.label(crate::i18n::tr("Brush auswählen, dann Eckpunkte (Quadrate) oder Kanten (Kreise) in 2D ziehen.\nStrg = mehrere, Rahmen ziehen = mehrere, Pfeiltasten = verschieben."));
            }
            Tool::Disp => self.disp_section(ui),
            Tool::Cordon => self.cordon_section(ui),
            Tool::Measure => {
                ui.label(crate::i18n::tr("2D: Linie ziehen · 3D: zwei Punkte auf Flächen anklicken. Esc löscht."));
                if let Some((a, b)) = self.ed.measure {
                    let d = b - a;
                    let text = crate::i18n::tr_dyn(&format!("Länge {:.2}\nΔX {:.1}  ΔY {:.1}  ΔZ {:.1}\n≈ {:.2} m", d.length(), d.x, d.y, d.z, d.length() * 0.0254));
                    theme::section(ui, crate::i18n::tr("Ergebnis"));
                    egui::Frame::new().fill(theme::t().raised).corner_radius(6).inner_margin(8).show(ui, |ui| {
                        ui.label(RichText::new(&text).monospace());
                    });
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::tr("Kopieren")).on_hover_text(crate::i18n::tr("Ergebnis in die Zwischenablage")).clicked() {
                            ui.ctx().copy_text(format!("{:.2}\t{:.1}\t{:.1}\t{:.1}", d.length(), d.x, d.y, d.z));
                            self.ed.status = crate::i18n::tr("Messung kopiert").into();
                        }
                        if ui.button(crate::i18n::tr("Messung löschen")).clicked() {
                            self.ed.measure = None;
                            self.ed.touch();
                        }
                    });
                }
            }
        }
    }

    fn camera_section(&mut self, ui: &mut egui::Ui) {
        ui.label(crate::i18n::tr("In 2D ziehen: neue Kamera (Start = Position, Ende = Blickziel). Punkte lassen sich verschieben. Bild↑/↓ wechselt."));
        let mut remove = None;
        for i in 0..self.ed.map.cameras.len() {
            ui.horizontal(|ui| {
                if ui.radio(self.ed.map.active_camera == i, "").on_hover_text(crate::i18n::tr("Diese Kamera benutzen")).clicked() {
                    self.ed.map.active_camera = i;
                    self.ed.camera_changed = true;
                }
                // Name direkt umbenennen (leer = „Kamera n“)
                let hint = crate::i18n::tr_dyn(&format!("Kamera {}", i + 1));
                let w = (ui.available_width() - 34.0).clamp(80.0, 220.0);
                let r = ui.add(egui::TextEdit::singleline(&mut self.ed.map.cameras[i].name).hint_text(crate::theme::hint(&hint)).desired_width(w));
                if r.changed() {
                    self.ed.dirty = true;
                }
                if ui.small_button("✖").on_hover_text(crate::i18n::tr("Kamera löschen")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.ed.checkpoint();
            self.ed.map.cameras.remove(i);
            self.ed.map.active_camera = self.ed.map.active_camera.min(self.ed.map.cameras.len().saturating_sub(1));
            self.ed.camera_changed = true;
            self.ed.touch();
        }
        if ui.button(crate::i18n::tr("Neue Kamera an aktueller Position")).clicked() {
            let c = self.view3d.camera_def();
            self.ed.add_camera(c.pos, c.look);
        }
    }

    fn block_section(&mut self, ui: &mut egui::Ui) {
        // die Form selbst wird oben in der Kontextleiste gewählt
        let p = &mut self.ed.shape;
        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Form: {} (oben in der Leiste wählen)", p.shape.label()))).color(theme::t().text2));
        egui::Grid::new("shapeparams").num_columns(2).show(ui, |ui| {
            if matches!(p.shape, Shape::Cylinder | Shape::Spike | Shape::Sphere | Shape::Arch | Shape::Torus) {
                ui.label(if matches!(p.shape, Shape::Arch | Shape::Torus) { "Segmente" } else { crate::i18n::tr("Seiten") });
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.sides)).range(3..=64));
                ui.end_row();
            }
            if matches!(p.shape, Shape::Arch | Shape::Torus) {
                ui.label(if p.shape == Shape::Arch { crate::i18n::tr("Wandstärke") } else { "Rohrdicke" });
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.wall)).range(1.0..=1024.0));
                ui.end_row();
                ui.label(crate::i18n::tr("Bogen (°)"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.arc)).range(1.0..=360.0));
                ui.end_row();
                ui.label(crate::i18n::tr("Startwinkel (°)"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.start_angle)).range(-360.0..=360.0));
                ui.end_row();
                ui.label(crate::i18n::tr("Höhe pro Segment"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.add_height)));
                ui.end_row();
            }
            if p.shape == Shape::Torus {
                ui.label(crate::i18n::tr("Querschnitt-Seiten"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut p.cross_sides)).range(3..=32));
                ui.end_row();
            }
        });
        ui.label(RichText::new(crate::i18n::tr("In einer 2D-Ansicht ein Rechteck ziehen. Die Höhe wird vom letzten Block übernommen.")).small());
    }

    fn entity_tool_section(&mut self, ui: &mut egui::Ui) {
        // Klasse für neue Entities: Feld mit Vorschlägen, Pfeil öffnet die Klassen-Auswahl
        let th = theme::t();
        ui.label(RichText::new(crate::i18n::tr("Klick in 2D oder auf eine Fläche in 3D platziert:")).color(th.text2));
        ui.horizontal(|ui| {
            let col = self.ed.fgd.resolve_color(&self.ed.entity_class).map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(Color32::from_rgb(120, 220, 120));
            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, col);
            let w = (ui.available_width() - BROWSE_W - 40.0).clamp(120.0, 260.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.ed.entity_class).font(egui::TextStyle::Monospace).desired_width(w));
            let typed = self.ed.entity_class.clone();
            if let Some(pick) = suggest_popup(ui, &r, ("class_sug_new", 0, 0), class_candidates(&self.ed.fgd, &typed), typed.trim().is_empty()) {
                self.ed.entity_class = pick;
            }
            let lb = list_button(ui, crate::i18n::tr("Alle Klassen anzeigen (Suche, Favoriten, Gruppen)"));
            if lb.clicked() {
                self.open_class_picker(lb.rect);
            }
            if ui.small_button("?").on_hover_text(crate::i18n::tr("Hilfe zu dieser Klasse")).clicked() {
                self.win.entity_help = Some(self.ed.entity_class.clone());
            }
        });
        if let Some(c) = self.ed.fgd.class(&self.ed.entity_class) {
            if !c.description.is_empty() {
                ui.label(RichText::new(crate::i18n::tr_dyn(&c.description)).color(th.text2));
            }
        }
        if self.ed.entity_class.starts_with("prop_") {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Modell:"));
                ui.add(egui::TextEdit::singleline(&mut self.ed.next_model).desired_width(180.0));
                if browse_button(ui, crate::i18n::tr("Modell-Browser")).clicked() {
                    self.dlg.browse_target = None;
                    self.win.models = true;
                }
            });
        }
    }

    fn face_section(&mut self, ui: &mut egui::Ui) {
        if self.ed.faces.is_empty() {
            ui.label(crate::i18n::tr("Klick in 3D wählt Flächen (Strg = mehrere).\nRechtsklick: Material anwenden · Alt+Rechtsklick: nahtlos ausrichten."));
            return;
        }
        ui.label(crate::i18n::tr_dyn(&format!("{} ausgewählt", crate::i18n::count_de(self.ed.faces.len(), "Fläche", "Flächen"))));
        let (sid, idx) = *self.ed.faces.iter().next().unwrap();
        let Some(side) = self.ed.map.solid(sid).and_then(|s| s.sides.get(idx)).cloned() else { return };
        let mut s = side.clone();
        let mut changed = false;
        let mut begin = false;
        egui::Grid::new("facegrid").num_columns(3).show(ui, |ui| {
            ui.label("");
            ui.label("U");
            ui.label("V");
            ui.end_row();
            ui.label(crate::i18n::tr("Skalierung"));
            for a in [&mut s.uaxis.scale, &mut s.vaxis.scale] {
                let r = ui.add(crate::theme::dv(egui::DragValue::new(a)).speed(0.01).range(0.01..=64.0));
                begin |= r.drag_started() || r.gained_focus();
                changed |= r.changed();
            }
            ui.end_row();
            ui.label(crate::i18n::tr("Verschiebung"));
            for a in [&mut s.uaxis.shift, &mut s.vaxis.shift] {
                let r = ui.add(crate::theme::dv(egui::DragValue::new(a)).speed(1.0));
                begin |= r.drag_started() || r.gained_focus();
                changed |= r.changed();
            }
            ui.end_row();
            ui.label(crate::i18n::tr("Rotation"));
            let r = ui.add(crate::theme::dv(egui::DragValue::new(&mut s.rotation)).speed(1.0).suffix("°"));
            begin |= r.drag_started() || r.gained_focus();
            changed |= r.changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Lightmap"));
            let r = ui.add(crate::theme::dv(egui::DragValue::new(&mut s.lightmapscale)).range(1..=128));
            begin |= r.drag_started() || r.gained_focus();
            changed |= r.changed();
            ui.end_row();
            ui.label(crate::i18n::tr("Smoothing"));
            let r = ui.add(egui::TextEdit::singleline(&mut s.smoothing_groups).desired_width(60.0));
            begin |= r.gained_focus();
            changed |= r.changed();
            ui.end_row();
        });
        ui.label(RichText::new(crate::i18n::tr("Ausrichten")).strong());
        let mats = &self.mats;
        let dims = |m: &str| mats.info(m).map(|i| (i.width as f64, i.height as f64)).unwrap_or((512.0, 512.0));
        let mut justify = None;
        ui.horizontal_wrapped(|ui| {
            for (j, l) in [(Justify::Fit, crate::i18n::tr("Einpassen")), (Justify::Left, "L"), (Justify::Right, "R"), (Justify::Top, "O"), (Justify::Bottom, "U"), (Justify::Center, "Mitte")] {
                if ui.button(l).clicked() {
                    justify = Some(j);
                }
            }
            crate::theme::checkbox(ui, &mut self.dlg.treat_as_one, crate::i18n::tr("Als Einheit"));
        });
        if let Some(j) = justify {
            self.ed.justify_faces(j, self.dlg.treat_as_one, &dims);
        }
        ui.horizontal(|ui| {
            if ui.button(crate::i18n::tr("An Welt")).clicked() {
                self.ed.align_faces(true);
            }
            if ui.button(crate::i18n::tr("An Fläche")).clicked() {
                self.ed.align_faces(false);
            }
            if ui.button(crate::i18n::tr("Material übernehmen")).on_hover_text(crate::i18n::tr("Material dieser Fläche als aktuelles setzen")).clicked() {
                self.ed.material = side.material.clone();
            }
        });
        ui.label(RichText::new(&side.material).monospace().small());
        ui.label(RichText::new(crate::i18n::tr("Displacement")).strong());
        ui.horizontal(|ui| {
            ui.label(crate::i18n::tr("Power"));
            ui.add(crate::theme::dv(egui::DragValue::new(&mut self.ed.dbrush.power)).range(2..=4));
            if ui.button(crate::i18n::tr("Erstellen")).clicked() {
                let p = self.ed.dbrush.power;
                self.ed.create_disp(p);
            }
            if ui.button(crate::i18n::tr("Entfernen")).clicked() {
                self.ed.destroy_disp();
            }
        });
        if begin {
            self.ed.checkpoint();
        }
        if changed {
            let rot_delta = s.rotation - side.rotation;
            for (sid, i) in self.ed.faces.clone() {
                if let Some(solid) = self.ed.map.solid_mut(sid) {
                    let n = solid.planes.get(i).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
                    if let Some(dst) = solid.sides.get_mut(i) {
                        dst.uaxis.scale = s.uaxis.scale;
                        dst.vaxis.scale = s.vaxis.scale;
                        dst.uaxis.shift = s.uaxis.shift;
                        dst.vaxis.shift = s.vaxis.shift;
                        dst.lightmapscale = s.lightmapscale;
                        dst.smoothing_groups = s.smoothing_groups.clone();
                        if rot_delta != 0.0 {
                            let q = DQuat::from_axis_angle(n, -rot_delta.to_radians());
                            dst.uaxis.axis = q * dst.uaxis.axis;
                            dst.vaxis.axis = q * dst.vaxis.axis;
                            dst.rotation = (dst.rotation + rot_delta).rem_euclid(360.0);
                        }
                    }
                }
            }
            self.ed.dirty = true;
            self.ed.touch();
        }
        let _ = default_tex_axes;
    }

    fn disp_section(&mut self, ui: &mut egui::Ui) {
        let th = theme::t();
        ui.label(crate::i18n::keys(crate::i18n::tr("In 3D mit gedrückter linker Maustaste malen. Strg kehrt Anheben/Absenken um.")));
        let (n_disp, power, flags_all, flags_any) = self.ed.disp_state();
        let b = &mut self.ed.dbrush;
        // Modi als 3×3-Raster gleich breiter Knöpfe (bricht in keiner Sprache unregelmäßig um)
        theme::section(ui, crate::i18n::tr("Modus"));
        let modes = [PaintMode::Raise, PaintMode::Lower, PaintMode::Smooth, PaintMode::Sharpen, PaintMode::Flatten, PaintMode::Terrace, PaintMode::Noise, PaintMode::AlphaAdd, PaintMode::AlphaRemove];
        let bw = ((ui.available_width() - 2.0 * 4.0) / 3.0).floor();
        for row in modes.chunks(3) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for m in row {
                    let sel = b.mode == *m;
                    let btn = egui::Button::new(RichText::new(crate::i18n::tr(m.label())).size(12.0).color(if sel { th.accent_text } else { th.text })).min_size(Vec2::new(bw, 26.0)).fill(if sel { th.accent_soft } else { th.raised }).stroke(Stroke::new(1.0f32, if sel { th.accent } else { th.line }));
                    if ui.add(btn).clicked() {
                        b.mode = *m;
                    }
                }
            });
        }
        theme::section(ui, crate::i18n::tr("Pinsel"));
        egui::Grid::new("dbrush").num_columns(2).show(ui, |ui| {
            ui.label(crate::i18n::tr("Radius"));
            ui.add(crate::theme::sl(egui::Slider::new(&mut b.radius, 8.0..=1024.0)));
            ui.end_row();
            ui.label(crate::i18n::tr("Stärke"));
            ui.add(crate::theme::sl(egui::Slider::new(&mut b.strength, 0.5..=64.0)));
            ui.end_row();
            ui.label(crate::i18n::tr("Abfall"));
            let fs = [crate::disp::Falloff::Soft, crate::disp::Falloff::Linear, crate::disp::Falloff::Hard];
            let cur = fs.iter().position(|f| *f == b.falloff).unwrap_or(0);
            let names: Vec<&str> = fs.iter().map(|f| crate::i18n::tr(f.label())).collect();
            b.falloff = fs[theme::segmented(ui, cur, &names, false)];
            ui.end_row();
            if b.mode == PaintMode::Terrace {
                ui.label(crate::i18n::tr("Stufenhöhe"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut b.step)).range(1.0..=512.0).speed(1.0).suffix(" u"));
                ui.end_row();
            }
            ui.label(crate::i18n::tr("Richtung"));
            let i = theme::segmented(ui, if b.along_z { 0 } else { 1 }, &[crate::i18n::tr("Z-Achse"), crate::i18n::tr("Flächennormale")], false);
            b.along_z = i == 0;
            ui.end_row();
        });
        crate::theme::checkbox(ui, &mut b.show_alpha, crate::i18n::tr("Alpha (Überblendung) anzeigen"));
        theme::section(ui, crate::i18n::tr("Auf ausgewählte Displacements anwenden"));
        if n_disp == 0 {
            ui.label(RichText::new(crate::i18n::tr("Keine Displacements ausgewählt.")).color(th.text3));
        }
        ui.add_enabled_ui(n_disp > 0, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::tr("Rauschen"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut self.ed.dbrush.noise_min)).prefix(crate::i18n::tr("Min ")).suffix(" u"));
                ui.add(crate::theme::dv(egui::DragValue::new(&mut self.ed.dbrush.noise_max)).prefix(crate::i18n::tr("Max ")).suffix(" u"));
                if ui.button(crate::i18n::tr("Anwenden")).clicked() {
                    self.ed.disp_noise();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button(crate::i18n::tr("Nähen")).on_hover_text(crate::i18n::tr("Gemeinsame Kanten benachbarter Displacements angleichen")).clicked() {
                    self.ed.disp_sew();
                }
                if ui.button(crate::i18n::tr("Alpha invertieren")).clicked() {
                    self.ed.disp_invert_alpha();
                }
                if ui.button(crate::i18n::tr("Unterteilen")).on_hover_text(crate::i18n::tr("Glättet alle ausgewählten Displacements gemeinsam (Catmull-Clark) – z. B. wird ein Würfel aus 6 Displacements rund")).clicked() {
                    self.ed.disp_subdivide();
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Power"));
                let cur = power.map(|p| p.clamp(2, 4) as usize - 2).unwrap_or(usize::MAX);
                let n = theme::segmented(ui, cur, &["2", "3", "4"], false);
                if n != cur && n < 3 {
                    self.ed.disp_set_power(n as u32 + 2);
                }
            })
            .response
            .on_hover_text(crate::i18n::tr("Auflösung ändern, Form und Alpha bleiben erhalten"));
            // Kollisions-Flags als echte Schalter (Zustand der Auswahl; gemischt = halb)
            for (flag, label) in [(FLAG_NO_PHYS, crate::i18n::tr("Keine Physik-Kollision")), (FLAG_NO_HULL, crate::i18n::tr("Keine Hull-Kollision")), (FLAG_NO_RAY, crate::i18n::tr("Keine Strahl-Kollision"))] {
                let mut on = flags_all & flag != 0;
                let mixed = !on && flags_any & flag != 0;
                let r = ui.scope(|ui| {
                    theme::box_style(ui);
                    ui.add(egui::Checkbox::new(&mut on, label).indeterminate(mixed))
                });
                if r.inner.changed() {
                    self.ed.disp_set_flag(flag, on);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Neue Displacements: im Textur-Werkzeug Flächen wählen →")).small().color(th.text3));
            if ui.small_button(crate::i18n::tr("Displacement erstellen")).on_hover_text(crate::i18n::tr("Aus den im Textur-Werkzeug ausgewählten Flächen")).clicked() {
                self.ed.create_disp(3);
            }
        });
    }

    fn cordon_section(&mut self, ui: &mut egui::Ui) {
        ui.label(crate::i18n::tr("In einer 2D-Ansicht einen Bereich ziehen. Beim Kompilieren wird nur dieser Teil gebaut (abgedichtet)."));
        let mut changed = false;
        let mut remove = false;
        if let Some(c) = self.ed.map.cordon.as_mut() {
            changed |= crate::theme::checkbox(ui, &mut c.active, crate::i18n::tr("Cordon aktiv")).changed();
            egui::Grid::new("cordon").num_columns(4).show(ui, |ui| {
                for (label, v) in [("Min", &mut c.bounds.min), ("Max", &mut c.bounds.max)] {
                    ui.label(label);
                    for a in 0..3 {
                        changed |= ui.add(crate::theme::dv(egui::DragValue::new(&mut v[a])).speed(8.0)).changed();
                    }
                    ui.end_row();
                }
            });
            remove = ui.button(crate::i18n::tr("Cordon entfernen")).clicked();
        } else {
            ui.label(crate::i18n::tr("Noch kein Cordon gesetzt."));
        }
        if remove {
            self.ed.checkpoint();
            self.ed.map.cordon = None;
        }
        if changed {
            self.ed.dirty = true;
            self.ed.touch();
        }
    }

    fn visgroup_section(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, crate::i18n::tr("Visgroups"));
        {
            let mut action: Option<(u32, u8)> = None;
            let th = theme::t();
            // Anzahl der Objekte je Visgroup
            let mut counts: std::collections::HashMap<u32, usize> = Default::default();
            for (e, s) in self.ed.map.solids_owned() {
                if e.is_none() {
                    for id in &s.editor.visgroups {
                        *counts.entry(*id).or_default() += 1;
                    }
                }
            }
            for e in &self.ed.map.entities {
                for id in &e.editor.visgroups {
                    *counts.entry(*id).or_default() += 1;
                }
            }
            let icon_btn = |ui: &mut egui::Ui, icon: &str, on: bool, tip: &str| -> bool {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, th.hover);
                }
                crate::icons::draw(ui.painter(), r.shrink(3.0), icon, if on { th.text } else { th.text3.gamma_multiply(0.5) }, 1.3);
                if !on {
                    ui.painter().line_segment([r.left_bottom() + Vec2::new(3.0, -3.0), r.right_top() + Vec2::new(-3.0, 3.0)], Stroke::new(1.2f32, th.text3));
                }
                resp.on_hover_text(tip).clicked()
            };
            // senkrechte Liste: Auge, Farbe, Name, Anzahl, Schloss; Aktionen rechts
            for v in self.ed.map.visgroups.clone() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if icon_btn(ui, "eye", v.visible, crate::i18n::tr("Ein-/Ausblenden")) {
                        action = Some((v.id, if v.visible { 0 } else { 1 }));
                    }
                    if icon_btn(ui, "lock", v.locked, crate::i18n::tr("Ebene sperren (nicht anklickbar)")) {
                        action = Some((v.id, if v.locked { 7 } else { 6 }));
                    }
                    let (r, p) = ui.allocate_painter(Vec2::splat(12.0), Sense::hover());
                    p.rect_filled(r.rect, 3.0, Color32::from_rgb(v.color[0], v.color[1], v.color[2]));
                    let n = counts.get(&v.id).copied().unwrap_or(0);
                    let l = ui.add(egui::Label::new(RichText::new(&v.name).color(if v.visible { th.text } else { th.text3 })).truncate().sense(Sense::click()));
                    if l.on_hover_text(crate::i18n::tr("Klick wählt alle Objekte dieser Visgroup")).clicked() {
                        action = Some((v.id, 2));
                    }
                    ui.label(RichText::new(n.to_string()).font(theme::mono(11.0)).color(th.text3));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖").on_hover_text(crate::i18n::tr("Visgroup löschen")).clicked() {
                            action = Some((v.id, 5));
                        }
                        if ui.small_button("−").on_hover_text(crate::i18n::tr("Auswahl entfernen")).clicked() {
                            action = Some((v.id, 4));
                        }
                        if ui.small_button("+").on_hover_text(crate::i18n::tr("Auswahl hinzufügen")).clicked() {
                            action = Some((v.id, 3));
                        }
                    });
                });
            }
            if self.ed.map.visgroups.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("Noch keine Visgroups – Objekte auswählen, Namen eingeben, „Aus Auswahl“.")).size(12.0).color(th.text3));
            }
            if let Some((id, a)) = action {
                match a {
                    0 | 1 => self.ed.set_visgroup_visible(id, a == 1),
                    2 => self.ed.select_visgroup(id),
                    3 => self.ed.add_selection_to_visgroup(id, true),
                    4 => self.ed.add_selection_to_visgroup(id, false),
                    6 | 7 => self.ed.set_visgroup_locked(id, a == 6),
                    _ => self.ed.delete_visgroup(id),
                }
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.dlg.visgroup_name).desired_width(150.0));
                if ui.add_enabled(!self.ed.sel.is_empty(), egui::Button::new(crate::i18n::tr("Aus Auswahl"))).clicked() {
                    let n = self.dlg.visgroup_name.clone();
                    self.ed.create_visgroup(&n);
                }
            });
            ui.separator();
            theme::section(ui, crate::i18n::tr("Automatische Filter"));
            let av = &mut self.ed.auto_vis;
            let before = *av;
            ui.horizontal_wrapped(|ui| {
                crate::theme::checkbox(ui, &mut av.world, crate::i18n::tr("Welt"));
                crate::theme::checkbox(ui, &mut av.detail, "func_detail");
                crate::theme::checkbox(ui, &mut av.triggers, crate::i18n::tr("Trigger"));
                crate::theme::checkbox(ui, &mut av.tools, crate::i18n::tr("Tool-Texturen"));
                crate::theme::checkbox(ui, &mut av.brush_ents, crate::i18n::tr("Brush-Entities"));
                crate::theme::checkbox(ui, &mut av.point_ents, crate::i18n::tr("Punkt-Entities"));
                crate::theme::checkbox(ui, &mut av.lights, crate::i18n::tr("Lichter"));
                crate::theme::checkbox(ui, &mut av.props, crate::i18n::tr("Props"));
                crate::theme::checkbox(ui, &mut av.disps, crate::i18n::tr("Displacements"));
            });
            if format!("{before:?}") != format!("{:?}", self.ed.auto_vis) {
                self.ed.touch();
            }
        }
    }

    fn selection_section(&mut self, ui: &mut egui::Ui) {
        let ents = self.ed.selected_entity_ids();
        let solids = self.ed.sel.iter().filter(|o| matches!(o, Obj::Solid(_))).count();
        if self.ed.sel.is_empty() {
            return;
        }
        if solids > 0 {
            theme::section(ui, crate::i18n::tr("Brushes"));
            self.brush_panel(ui, solids);
        }
        if ents.len() == 1 {
            let id = ents[0];
            if let Some(mut e) = self.ed.map.entity(id).cloned() {
                let (changed, begin, actions) = entity_editor(ui, &self.ed.fgd, &self.ed.map, &mut e, &mut self.new_kv, false);
                if begin {
                    self.ed.checkpoint();
                }
                if changed {
                    if let Some(dst) = self.ed.map.entity_mut(id) {
                        *dst = e.clone();
                    }
                    self.ed.dirty = true;
                    self.ed.touch();
                }
                self.handle_actions(id, &e, actions);
            }
        } else if ents.len() > 1 {
            self.multi_edit(ui, &ents);
        }
    }

    /// Brush-Auswahl: Größe und Position (genau eintippen), benutzte Materialien, Schnellknöpfe
    fn brush_panel(&mut self, ui: &mut egui::Ui, solids: usize) {
        let th = theme::t();
        let b = self.ed.selection_bounds();
        if !b.is_empty() {
            let size = b.size();
            let lw = set_label_w(ui);
            let mut new_size = size;
            let mut new_min = b.min;
            let mut begin = false;
            for (label, v, tip) in [(crate::i18n::tr("Größe"), &mut new_size, crate::i18n::tr("Breite × Tiefe × Höhe der Auswahl – ändern skaliert sie (Ecke unten links bleibt)")), (crate::i18n::tr("Position"), &mut new_min, crate::i18n::tr("Ecke unten links der Auswahl – ändern verschiebt sie"))] {
                ui.horizontal(|ui| {
                    grid_label_w(ui, label, tip, lw);
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let fw = ((ui.available_width() - 3.0 * 14.0 - 12.0) / 3.0).clamp(34.0, 80.0);
                    for (i, (axis, col)) in [("X", Color32::from_rgb(230, 90, 90)), ("Y", Color32::from_rgb(110, 200, 110)), ("Z", Color32::from_rgb(100, 150, 240))].into_iter().enumerate() {
                        ui.label(RichText::new(axis).color(col).strong().size(11.0));
                        let r = ui.add_sized([fw, 20.0], crate::theme::dv(egui::DragValue::new(&mut v[i]).speed(1.0).max_decimals(2)));
                        begin |= r.drag_started() || r.gained_focus();
                    }
                });
            }
            // ein Rückgängig-Schritt pro Ziehen bzw. Eingabe
            if begin {
                self.ed.checkpoint();
            }
            if (new_size - size).abs().max_element() > 1e-6 && new_size.min_element() > 0.5 {
                let to = crate::geometry::Aabb::new(b.min, b.min + new_size);
                self.ed.apply_xform(&crate::editor::Editor::scale_xform(b, to), true);
                self.ed.touch();
            } else if (new_min - b.min).abs().max_element() > 1e-6 {
                self.ed.translate_selection(new_min - b.min);
            }
        }
        // Materialien der ausgewählten Brushes
        let mut mats: Vec<(String, usize)> = Vec::new();
        for id in self.ed.selected_solid_ids() {
            if let Some(s) = self.ed.map.solid(id) {
                for side in &s.sides {
                    match mats.iter_mut().find(|m| m.0.eq_ignore_ascii_case(&side.material)) {
                        Some(m) => m.1 += 1,
                        None => mats.push((side.material.clone(), 1)),
                    }
                }
            }
        }
        mats.sort_by(|a, b| b.1.cmp(&a.1));
        if !mats.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Materialien ({})", mats.len()))).size(12.0).color(th.text3));
            for (m, n) in mats.iter().take(8) {
                ui.horizontal(|ui| {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    match self.mats.thumb(m) {
                        Some(t) => {
                            egui::Image::new((t.id(), r.size())).corner_radius(4).paint_at(ui, r);
                        }
                        None => {
                            let c = crate::view3d::material_color(m);
                            ui.painter().rect_filled(r, 4.0, Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8));
                        }
                    }
                    let short = m.rsplit('/').next().unwrap_or(m).to_ascii_lowercase();
                    let l = ui.add(egui::Label::new(RichText::new(format!("{short}  ×{n}")).font(theme::mono(11.5)).color(th.text2)).truncate().sense(Sense::click()));
                    if resp.clicked() || l.clicked() {
                        self.ed.material = m.clone();
                    }
                    if ui.small_button(crate::i18n::tr("ersetzen")).on_hover_text(crate::i18n::tr_dyn(&format!("Alle Flächen der Auswahl mit {short} auf die aktuelle Textur ({}) setzen", self.ed.material.to_ascii_lowercase()))).clicked() {
                        let (from, to) = (m.clone(), self.ed.material.clone());
                        self.ed.checkpoint();
                        for id in self.ed.selected_solid_ids() {
                            if let Some(s) = self.ed.map.solid_mut(id) {
                                for side in s.sides.iter_mut().filter(|sd| sd.material.eq_ignore_ascii_case(&from)) {
                                    side.material = to.clone();
                                }
                            }
                        }
                        self.ed.touch();
                    }
                });
            }
            if mats.len() > 8 {
                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("… und {} weitere", mats.len() - 8))).size(11.5).color(th.text3));
            }
        }
        // Schnellknöpfe
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            if ui.small_button(crate::i18n::tr("Aushöhlen")).on_hover_text(crate::i18n::keys("Strg+H")).clicked() {
                self.ed.hollow(self.hollow_thickness);
            }
            if ui.small_button(crate::i18n::tr("Carve")).on_hover_text(crate::i18n::keys("Strg+Shift+C")).clicked() {
                self.ed.carve();
            }
            if ui.small_button(crate::i18n::tr("Am Raster")).on_hover_text(crate::i18n::keys("Strg+B")).clicked() {
                self.ed.snap_selection_to_grid();
            }
            if ui.small_button(crate::i18n::tr("Spiegeln X")).clicked() {
                self.ed.flip_selection(0);
            }
            if ui.small_button(crate::i18n::tr("Spiegeln Y")).clicked() {
                self.ed.flip_selection(1);
            }
            if ui.small_button(crate::i18n::tr("90° drehen")).on_hover_text(crate::i18n::tr("Um die Hochachse")).clicked() {
                self.ed.rotate_selection(2, -90.0);
            }
            let label = if solids == 1 { crate::i18n::tr("In Entity umwandeln…").to_string() } else { crate::i18n::tr_dyn(&format!("{solids} Brushes in Entity umwandeln…")) };
            if ui.small_button(label).on_hover_text(crate::i18n::keys("Strg+T")).clicked() {
                self.open_convert_dialog();
            }
        });
    }

    pub(crate) fn handle_actions(&mut self, id: u32, e: &Entity, actions: Vec<EdAction>) {
        for a in actions {
            match a {
                EdAction::Pick(key) => {
                    self.ed.pick_target = Some((id, key));
                    self.ed.status = crate::i18n::tr("Pipette: Ziel-Entity in einer Ansicht anklicken").into();
                }
                EdAction::BrowseModel(key) => {
                    // aktuelles Modell im Browser vorwählen
                    if let Some(m) = e.get(&key).filter(|m| !m.is_empty()) {
                        self.mb.selected = Some(m.replace('\\', "/").to_ascii_lowercase());
                        self.mb.skin = e.get("skin").and_then(|s| s.parse().ok()).unwrap_or(0);
                    }
                    self.dlg.browse_target = Some((id, key));
                    self.win.models = true;
                }
                EdAction::BrowseSound(key) => {
                    self.dlg.browse_target = Some((id, key));
                    self.win.sounds = true;
                }
                EdAction::BrowseParticle(key) => {
                    self.pb.target = Some((id, key));
                    self.pb.open = true;
                }
                EdAction::Help => self.win.entity_help = Some(e.classname().to_string()),
                EdAction::ClassList(r) => self.open_class_picker_for(r, Some(id)),
                EdAction::RotateYaw(d) => self.ed.rotate_entities_yaw(d),
                EdAction::CopyProps => self.ed.copy_entity_props(),
                EdAction::PasteProps => self.ed.paste_entity_props(),
            }
        }
    }

    /// Mehrere Entities gleichzeitig bearbeiten
    fn multi_edit(&mut self, ui: &mut egui::Ui, ids: &[u32]) {
        ui.label(crate::i18n::tr_dyn(&format!("{} Entities ausgewählt – Änderungen gelten für alle", ids.len())));
        let ents: Vec<Entity> = ids.iter().filter_map(|id| self.ed.map.entity(*id).cloned()).collect();
        // gleiche Klasse: Felder wie bei der Einzelauswahl (FGD-Reihenfolge, Anzeigenamen, Auswahllisten)
        let class0 = ents.first().map(|e| e.classname().to_string()).unwrap_or_default();
        let same_class = ents.iter().all(|e| e.classname().eq_ignore_ascii_case(&class0));
        let defs = self.ed.fgd.all_props(&class0);
        let mut keys: Vec<String> = Vec::new();
        if same_class {
            for p in &defs {
                if p.kind != "flags" && !p.name.eq_ignore_ascii_case("origin") && !p.name.eq_ignore_ascii_case("angles") {
                    keys.push(p.name.clone());
                }
            }
        }
        for e in &ents {
            for (k, _) in &e.props {
                if !keys.iter().any(|x| x.eq_ignore_ascii_case(k)) && !k.eq_ignore_ascii_case("origin") && !(same_class && k.eq_ignore_ascii_case("classname")) {
                    keys.push(k.clone());
                }
            }
        }
        let mut set: Option<(String, String)> = None;
        let mut begin = false;
        let lw = set_label_w(ui);
        let multi_w = (ui.available_width() - lw - ui.spacing().item_spacing.x * 2.0 - 16.0).max(80.0);
        let mixed = crate::i18n::tr("(verschieden)");
        egui::Grid::new("multi").num_columns(2).striped(true).show(ui, |ui| {
            for k in &keys {
                let vals: Vec<Option<&str>> = ents.iter().map(|e| e.get(k)).collect();
                let same = vals.windows(2).all(|w| w[0] == w[1]);
                let def = defs.iter().find(|p| p.name.eq_ignore_ascii_case(k));
                match def {
                    Some(p) => {
                        let tip = if p.description.is_empty() { p.name.clone() } else { format!("{}\n{}", p.name, crate::i18n::tr_dyn(&p.description)) };
                        grid_label_w(ui, &crate::i18n::tr_dyn(&p.display), &tip, lw);
                    }
                    None => grid_label_w(ui, k, "", lw),
                }
                let cur = if same { vals[0].unwrap_or("").to_string() } else { String::new() };
                if let Some(p) = def.filter(|p| !p.choices.is_empty()) {
                    let shown = if same { p.choices.iter().find(|c| c.value == cur).map(|c| crate::i18n::tr_dyn(&c.name)).unwrap_or_else(|| cur.clone()) } else { mixed.to_string() };
                    egui::ComboBox::from_id_salt(("multi_choice", k)).selected_text(shown).width(multi_w).show_ui(ui, |ui| {
                        for c in &p.choices {
                            if ui.selectable_label(same && c.value == cur, crate::i18n::tr_dyn(&c.name)).clicked() {
                                begin = true;
                                set = Some((k.clone(), c.value.clone()));
                            }
                        }
                    });
                } else {
                    let mut v = cur;
                    let r = ui.add(egui::TextEdit::singleline(&mut v).hint_text(crate::theme::hint(if same { "" } else { mixed })).desired_width(multi_w));
                    begin |= r.gained_focus();
                    if r.changed() {
                        set = Some((k.clone(), v));
                    }
                }
                ui.end_row();
            }
        });
        if begin {
            self.ed.checkpoint();
        }
        if let Some((k, v)) = set {
            self.ed.set_key_selected(&k, &v);
        }
    }
}

fn is_target_kind(kind: &str) -> bool {
    matches!(kind, "target_destination" | "target_name_or_class" | "filterclass" | "npcclass")
}

/// Kleiner Kompass zum Einstellen des Yaw-Winkels
fn yaw_compass(ui: &mut egui::Ui, yaw: &mut f64) -> egui::Response {
    let (resp, p) = ui.allocate_painter(Vec2::splat(34.0), Sense::click_and_drag());
    let c = resp.rect.center();
    let r = 15.0;
    p.circle_stroke(c, r, Stroke::new(1.0f32, Color32::GRAY));
    let a = (*yaw as f32).to_radians();
    p.line_segment([c, c + Vec2::new(a.cos(), -a.sin()) * r], Stroke::new(2.0f32, Color32::YELLOW));
    let mut resp = resp;
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.clicked() || resp.dragged() {
            let d = pos - c;
            let ang = (-d.y).atan2(d.x).to_degrees() as f64;
            *yaw = ((ang / 15.0).round() * 15.0).rem_euclid(360.0);
            resp.mark_changed();
        }
    }
    resp.on_hover_text(crate::i18n::tr("Yaw (Blickrichtung) – klicken/ziehen"))
}

/// Eigenschaften-Editor für ein Entity (inkl. worldspawn). Liefert (geändert, Undo-Punkt nötig, Aktionen).
/// Beschriftung im Eigenschaften-Raster: 112 px breit, bei Überlänge abgeschnitten
fn grid_label(ui: &mut egui::Ui, text: &str, tip: &str) {
    grid_label_w(ui, text, tip, label_w(ui));
}

/// Breite der Beschriftungsspalte: 40 % der Panelbreite (112–170 px), einmal je Panel festgelegt
fn label_w(ui: &egui::Ui) -> f32 {
    ui.data(|d| d.get_temp::<f32>(egui::Id::new("amboss_label_w"))).unwrap_or(112.0)
}

/// Beschriftungsbreite für die folgenden Zeilen aus der verfügbaren Breite bestimmen und merken
fn set_label_w(ui: &mut egui::Ui) -> f32 {
    let w = (ui.available_width() * 0.4).clamp(112.0, 170.0);
    ui.data_mut(|d| d.insert_temp(egui::Id::new("amboss_label_w"), w));
    w
}

/// Beschriftung mit fester Breite (die erste Grid-Spalte bleibt so genau 112 px breit)
fn grid_label_w(ui: &mut egui::Ui, text: &str, tip: &str, width: f32) {
    let th = theme::t();
    // selbst gesetzt: egui würde in waagrechten Layouts den Umbruch abschalten und die Spalte verbreitern.
    // Bis zu zwei Zeilen, Rest mit „…“, voller Text im Tooltip.
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: theme::sans(12.5), color: th.text2, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: width, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
    let g = ui.painter().layout_job(job);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(width, g.size().y.max(24.0)), Sense::hover());
    ui.painter().galley(egui::pos2(r.left(), r.center().y - g.size().y / 2.0), g, th.text2);
    resp.on_hover_text(if tip.is_empty() { text } else { tip });
}

pub(crate) fn entity_editor(ui: &mut egui::Ui, fgd: &Fgd, map: &Map, e: &mut Entity, new_kv: &mut (String, String), is_world: bool) -> (bool, bool, Vec<EdAction>) {
    let mut changed = false;
    let mut begin = false;
    let mut actions = Vec::new();
    let class = e.classname().to_string();
    let def = fgd.class(&class);
    let names = map.targetnames();

    theme::section(ui, crate::i18n::tr("Allgemein"));
    ui.horizontal(|ui| {
        grid_label(ui, crate::i18n::tr("Klasse"), "");
        if is_world {
            ui.label(RichText::new(&class).monospace());
        } else {
            let mut c = class.clone();
            let w = (ui.available_width() - BROWSE_W - 26.0 - 30.0).clamp(80.0, 200.0);
            let r = ui.add(egui::TextEdit::singleline(&mut c).font(egui::TextStyle::Monospace).desired_width(w));
            begin |= r.gained_focus();
            if r.changed() {
                e.set("classname", c.clone());
                changed = true;
            }
            // Vorschläge beim Tippen (Klassennamen aus der FGD)
            if let Some(pick) = suggest_popup(ui, &r, ("class_sug", e.id, 0), class_candidates(fgd, &c), c.trim().is_empty()) {
                begin = true;
                e.set("classname", pick);
                changed = true;
            }
            let lb = list_button(ui, crate::i18n::tr("Alle Klassen anzeigen (Suche, Favoriten, Gruppen)"));
            if lb.clicked() {
                actions.push(EdAction::ClassList(lb.rect));
            }
        }
        if ui.small_button("?").on_hover_text(crate::i18n::tr("Hilfe zu dieser Klasse")).clicked() {
            actions.push(EdAction::Help);
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(120.0);
        if ui.small_button(crate::i18n::tr("Kopieren")).on_hover_text(crate::i18n::tr("Eigenschaften kopieren (Strg+Alt+C)")).clicked() {
            actions.push(EdAction::CopyProps);
        }
        if ui.small_button(crate::i18n::tr("Übertragen")).on_hover_text(crate::i18n::tr("Kopierte Eigenschaften auf alle ausgewählten Entities übertragen (Strg+Alt+V)")).clicked() {
            actions.push(EdAction::PasteProps);
        }
    });
    // Beschreibung steht oben bzw. in der Hilfe („?“) – hier nur der Hinweis bei unbekannter Klasse
    if def.is_none() {
        ui.colored_label(Color32::YELLOW, crate::i18n::tr("Unbekannte Klasse (nicht in der FGD)"));
    }

    // Winkel-Kompass
    if e.get("angles").is_some() && !is_world {
        let mut a = e.angles();
        let lw = set_label_w(ui);
        ui.horizontal(|ui| {
            grid_label_w(ui, crate::i18n::tr("Winkel"), "angles (Pitch Yaw Roll)", lw);
            let mut ch = false;
            ui.spacing_mut().interact_size.x = 44.0;
            ui.spacing_mut().item_spacing.x = 4.0;
            let r = yaw_compass(ui, &mut a.y);
            begin |= r.drag_started() || r.clicked();
            ch |= r.changed();
            // Zahlenfelder mit farbigem Buchstaben (Pitch rot, Yaw grün, Roll blau – wie die Drehachsen)
            for (v, l, col) in [(&mut a.x, "P", Color32::from_rgb(230, 90, 90)), (&mut a.y, "Y", Color32::from_rgb(110, 200, 110)), (&mut a.z, "R", Color32::from_rgb(100, 150, 240))] {
                ui.label(RichText::new(l).color(col).strong().size(11.0)).on_hover_text(match l {
                    "P" => crate::i18n::tr("Pitch (neigen)"),
                    "Y" => crate::i18n::tr("Yaw (drehen)"),
                    _ => crate::i18n::tr("Roll (rollen)"),
                });
                let r = ui.add(crate::theme::dv(egui::DragValue::new(v)).speed(1.0).max_decimals(2));
                begin |= r.drag_started() || r.gained_focus();
                ch |= r.changed();
            }
            if ch {
                e.set("angles", fmt_vec(a));
                changed = true;
            }
        });
        // Drehen: eine Reihe mit sechs gleich breiten Knöpfen (bricht in keiner Sprache um)
        ui.horizontal(|ui| {
            grid_label_w(ui, crate::i18n::tr("Drehen"), "", lw);
            ui.spacing_mut().item_spacing.x = 2.0;
            // Platz für sechs Knöpfe (mit Reserve, damit am rechten Rand nichts abgeschnitten wird)
            let bw = ((ui.available_width() - 5.0 * 2.0 - 20.0) / 6.0).floor().clamp(20.0, 56.0);
            ui.spacing_mut().button_padding.x = 1.0;
            let deg = if bw >= 34.0 { "°" } else { "" };
            for d in [-90.0f64, -45.0, -15.0, 15.0, 45.0, 90.0] {
                if ui.add_sized([bw, 22.0], egui::Button::new(RichText::new(format!("{}{}{deg}", if d > 0.0 { "+" } else { "−" }, d.abs())).size(10.5))).on_hover_text(crate::i18n::tr("Um die Hochachse drehen (Yaw)")).clicked() {
                    actions.push(EdAction::RotateYaw(d));
                }
            }
        });
    }

    let props = fgd.all_props(&class);
    // Schalter „Rohwerte“ (Schlüssel/Wert wie in der Datei) und im Einfach-Modus nur wichtige Schlüssel
    let raw_id = egui::Id::new("amboss_raw_kv");
    let all_id = egui::Id::new(("amboss_all_kv", e.id));
    let mut raw = ui.data_mut(|d| d.get_persisted::<bool>(raw_id)).unwrap_or(false);
    let simple = ui.data(|d| d.get_temp::<bool>(egui::Id::new("amboss_simple_ui"))).unwrap_or(false);
    let mut show_all = ui.data(|d| d.get_temp::<bool>(all_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        theme::section(ui, crate::i18n::tr("Schlüsselwerte"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if theme::checkbox(ui, &mut raw, crate::i18n::tr("Rohwerte")).on_hover_text(crate::i18n::tr("Schlüssel und Werte so zeigen, wie sie in der VMF stehen (ohne Anzeigenamen und Auswahllisten)")).changed() {
                ui.data_mut(|d| d.insert_persisted(raw_id, raw));
            }
        });
    });
    let important = |p: &crate::fgd::Property, cur: Option<&str>| -> bool {
        const KEYS: [&str; 24] = ["targetname", "parentname", "model", "skin", "_light", "_ambient", "pitch", "_cone", "_inner_cone", "target", "speed", "wait", "message", "health", "rendercolor", "renderamt", "effect_name", "start_active", "skyname", "texture", "fogcolor", "fogstart", "fogend", "filtername"];
        KEYS.contains(&p.name.to_ascii_lowercase().as_str()) || cur.is_some_and(|v| !v.trim().is_empty() && v.trim() != p.default.trim())
    };
    let hidden_count = if simple && !show_all && !raw { props.iter().filter(|p| p.kind != "flags" && !p.name.eq_ignore_ascii_case("angles") && !important(p, e.get(&p.name))).count() } else { 0 };
    // Breite der Wertespalte EINMAL außerhalb des Grids bestimmen: im Grid liefert available_width()
    // die Spaltenbreite des letzten Bildes – darauf aufbauende Felder würden die Spalte immer breiter machen
    let lw = set_label_w(ui);
    // 16 px: Innenabstand der Eingabefelder kommt zur gewünschten Breite hinzu
    let col_w = (ui.available_width() - lw - ui.spacing().item_spacing.x * 2.0 - 16.0).max(80.0);
    egui::Grid::new(("kv", e.id)).num_columns(2).striped(true).min_col_width(100.0).show(ui, |ui| {
        for p in props.iter().filter(|_| !raw) {
            if p.kind == "flags" || p.name.eq_ignore_ascii_case("angles") && !is_world {
                continue;
            }
            if hidden_count > 0 && !important(p, e.get(&p.name)) {
                continue;
            }
            let current = e.get(&p.name).map(|s| s.to_string());
            let tip = if p.description.is_empty() { format!("{} ({})", p.name, p.kind) } else { format!("{} ({})\n{}", p.name, p.kind, crate::i18n::tr_dyn(&p.description)) };
            let shown_label = match p.name.to_ascii_lowercase().as_str() {
                "_light" => crate::i18n::tr("Farbe · Helligkeit").to_string(),
                "_ambient" => crate::i18n::tr("Umgebung: Farbe · Helligkeit").to_string(),
                _ => crate::i18n::tr_dyn(&p.display),
            };
            grid_label(ui, &shown_label, &tip);
            if !p.choices.is_empty() {
                let cur = current.clone().unwrap_or_else(|| p.default.clone());
                let shown = p.choices.iter().find(|c| c.value == cur).map(|c| crate::i18n::tr_dyn(&c.name)).unwrap_or_else(|| cur.clone());
                egui::ComboBox::from_id_salt(("choice", e.id, &p.name)).selected_text(shown).width(col_w).show_ui(ui, |ui| {
                    for c in &p.choices {
                        if ui.selectable_label(c.value == cur, crate::i18n::tr_dyn(&c.name)).clicked() && c.value != cur {
                            begin = true;
                            e.set(&p.name, c.value.clone());
                            changed = true;
                        }
                    }
                });
            } else {
                let mut val = current.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    // Lichtfarbe (_light, _ambient …): Farbwähler + eigenes Feld für die Helligkeit
                    let ln = p.name.to_ascii_lowercase();
                    let light_key = matches!(ln.as_str(), "_light" | "_ambient" | "_lighthdr" | "_ambienthdr");
                    let is_color = p.kind == "color255" || light_key;
                    // HDR-Farbe „-1 -1 -1 1“ heißt: wie die normale Helligkeit – als Haken statt schwarzer Fläche
                    let hdr_key = light_key && ln.ends_with("hdr");
                    let src0 = if val.trim().is_empty() { p.default.clone() } else { val.clone() };
                    let same_as_ldr = hdr_key && src0.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()).is_some_and(|v| v < 0.0);
                    if hdr_key {
                        let mut same = same_as_ldr;
                        if theme::checkbox(ui, &mut same, crate::i18n::tr("wie Brightness")).on_hover_text(crate::i18n::tr("HDR-Licht übernimmt Farbe und Helligkeit des normalen Lichts (Wert „-1 -1 -1 1“)")).changed() {
                            begin |= !changed;
                            val = if same { "-1 -1 -1 1".to_string() } else { e.get(&ln.trim_end_matches("hdr").to_string()).filter(|v| !v.trim().is_empty()).unwrap_or("255 255 255 200").to_string() };
                            e.set(&p.name, val.clone());
                            changed = true;
                        }
                    }
                    if is_color && !(hdr_key && same_as_ldr) {
                        let src = if val.trim().is_empty() { p.default.clone() } else { val.clone() };
                        let mut rgb = parse_color(&src).unwrap_or([255, 255, 255]);
                        if ui.color_edit_button_srgb(&mut rgb).on_hover_text(crate::i18n::tr("Farbe wählen")).changed() {
                            begin |= !changed;
                            let rest: Vec<&str> = src.split_whitespace().skip(3).collect();
                            val = format!("{} {} {}{}", rgb[0], rgb[1], rgb[2], if rest.is_empty() { String::new() } else { format!(" {}", rest.join(" ")) });
                            e.set(&p.name, val.clone());
                            changed = true;
                        }
                        if light_key {
                            let nums: Vec<f64> = src.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                            let mut bright = nums.get(3).copied().unwrap_or(200.0);
                            let r = ui.add(egui::DragValue::new(&mut bright).range(-1.0..=100000.0).speed(2.0).max_decimals(1)).on_hover_text(crate::i18n::tr("Helligkeit (4. Wert)"));
                            begin |= r.drag_started() || r.gained_focus();
                            if r.changed() {
                                let c = parse_color(&src).unwrap_or([255, 255, 255]);
                                val = format!("{} {} {} {}", c[0], c[1], c[2], crate::map::fmt_num(bright));
                                e.set(&p.name, val.clone());
                                changed = true;
                            }
                        }
                    }
                    let target = is_target_kind(&p.kind);
                    // Modell-Feld: Typ studio, oder Schlüssel „model“ als einfacher Text
                    let is_model = p.kind == "studio" || p.kind == "studioprop" || (p.name.eq_ignore_ascii_case("model") && p.kind == "string");
                    let missing = target && !val.is_empty() && !val.starts_with('!') && !val.contains('*') && !names.iter().any(|n| n.eq_ignore_ascii_case(&val)) && fgd.class(&val).is_none();
                    // Breite nach verfügbarem Platz, Knöpfe rechts daneben bleiben sichtbar
                    let buttons = if target { 2 } else { 0 } + usize::from(is_model || is_color || matches!(p.kind.as_str(), "sound" | "particlesystem")) + usize::from(light_key);
                    // großzügig abziehen: schneidet das Panel rechts ab, verschwinden sonst zuerst die Knöpfe
                    let w = (col_w - buttons as f32 * (BROWSE_W + ui.spacing().item_spacing.x + 6.0) - 28.0).max(50.0);
                    let mut te = egui::TextEdit::singleline(&mut val).hint_text(crate::theme::hint(p.default.clone())).desired_width(w);
                    if missing {
                        te = te.text_color(Color32::from_rgb(255, 120, 120));
                    }
                    let r = ui.add(te);
                    begin |= r.gained_focus();
                    if r.changed() {
                        if val.is_empty() {
                            e.remove(&p.name);
                        } else {
                            e.set(&p.name, val.clone());
                        }
                        changed = true;
                    }
                    if target {
                        egui::ComboBox::from_id_salt(("names", e.id, &p.name)).selected_text("").width(18.0).show_ui(ui, |ui| {
                            for n in &names {
                                if ui.selectable_label(false, n).clicked() {
                                    begin = true;
                                    e.set(&p.name, n.clone());
                                    changed = true;
                                }
                            }
                        });
                        if ui.small_button("🖊").on_hover_text(crate::i18n::tr("Pipette: Ziel in einer Ansicht anklicken")).clicked() {
                            actions.push(EdAction::Pick(p.name.clone()));
                        }
                    }
                    if is_model && browse_button(ui, crate::i18n::tr("Modell-Browser öffnen – Modell wählen und „Übernehmen“ setzt es hier ein")).clicked() {
                        actions.push(EdAction::BrowseModel(p.name.clone()));
                    }
                    if p.kind == "sound" && browse_button(ui, crate::i18n::tr("Sound-Browser")).clicked() {
                        actions.push(EdAction::BrowseSound(p.name.clone()));
                    }
                    if p.kind == "particlesystem" && browse_button(ui, crate::i18n::tr("Partikel-Browser")).clicked() {
                        actions.push(EdAction::BrowseParticle(p.name.clone()));
                    }
                });
            }
            ui.end_row();
        }
        // Schlüssel ohne FGD-Definition (bei „Rohwerte“ alle Schlüssel)
        let known: Vec<String> = if raw { Vec::new() } else { props.iter().map(|p| p.name.to_ascii_lowercase()).collect() };
        let mut remove = None;
        for (i, (k, v)) in e.props.iter_mut().enumerate() {
            let lk = k.to_ascii_lowercase();
            if lk == "classname" || lk == "spawnflags" || (lk == "angles" && !is_world) || known.contains(&lk) {
                continue;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let b = ui.small_button("✖").on_hover_text(crate::i18n::tr("Schlüssel entfernen"));
                if b.clicked() {
                    remove = Some(i);
                }
                grid_label_w(ui, k.as_str(), "", (lw - b.rect.width() - 4.0).max(40.0));
            });
            if matches!(lk.as_str(), "_light" | "_ambient" | "_lighthdr" | "_ambienthdr") {
                // Lichtfarbe auch ohne FGD-Eintrag: Farbwähler + Helligkeit
                ui.horizontal(|ui| {
                    let mut rgb = parse_color(v).unwrap_or([255, 255, 255]);
                    if ui.color_edit_button_srgb(&mut rgb).on_hover_text(crate::i18n::tr("Farbe wählen")).changed() {
                        begin |= !changed;
                        let rest: Vec<String> = v.split_whitespace().skip(3).map(|s| s.to_string()).collect();
                        *v = format!("{} {} {}{}", rgb[0], rgb[1], rgb[2], if rest.is_empty() { String::new() } else { format!(" {}", rest.join(" ")) });
                        changed = true;
                    }
                    let mut bright = v.split_whitespace().nth(3).and_then(|s| s.parse::<f64>().ok()).unwrap_or(200.0);
                    let r = ui.add(egui::DragValue::new(&mut bright).range(-1.0..=100000.0).speed(2.0).max_decimals(1)).on_hover_text(crate::i18n::tr("Helligkeit (4. Wert)"));
                    begin |= r.drag_started() || r.gained_focus();
                    if r.changed() {
                        let c = parse_color(v).unwrap_or([255, 255, 255]);
                        *v = format!("{} {} {} {}", c[0], c[1], c[2], crate::map::fmt_num(bright));
                        changed = true;
                    }
                    let r = ui.add(egui::TextEdit::singleline(v).desired_width((col_w - 110.0).max(60.0)));
                    begin |= r.gained_focus();
                    changed |= r.changed();
                });
            } else {
                let r = ui.add(egui::TextEdit::singleline(v).desired_width(col_w));
                begin |= r.gained_focus();
                changed |= r.changed();
            }
            ui.end_row();
        }
        if let Some(i) = remove {
            begin = true;
            e.props.remove(i);
            changed = true;
        }
    });
    if hidden_count > 0 {
        if ui.button(crate::i18n::tr_dyn(&format!("Alle Eigenschaften ({})", hidden_count))).on_hover_text(crate::i18n::tr("Im Einfach-Modus stehen hier nur die wichtigen Schlüssel")).clicked() {
            show_all = true;
        }
    } else if simple && show_all && !raw && ui.small_button(crate::i18n::tr("Nur wichtige Eigenschaften")).clicked() {
        show_all = false;
    }
    ui.data_mut(|d| d.insert_temp(all_id, show_all));

    ui.horizontal(|ui| {
        let w = ((ui.available_width() - 96.0) / 2.0).max(50.0);
        ui.add(egui::TextEdit::singleline(&mut new_kv.0).hint_text(crate::theme::hint(crate::i18n::tr("Schlüssel"))).desired_width(w));
        ui.add(egui::TextEdit::singleline(&mut new_kv.1).hint_text(crate::theme::hint(crate::i18n::tr("Wert"))).desired_width(w));
        if ui.button("+").clicked() && !new_kv.0.trim().is_empty() {
            begin = true;
            e.set(new_kv.0.trim(), new_kv.1.clone());
            *new_kv = Default::default();
            changed = true;
        }
    });

    if let Some(flags) = props.iter().find(|p| p.kind == "flags") {
        ui.separator();
        theme::section(ui, crate::i18n::tr("Flags"));
        let mut v: u32 = e.get("spawnflags").and_then(|s| s.parse().ok()).unwrap_or(0);
        for c in &flags.choices {
            let bit: u32 = c.value.parse().unwrap_or(0);
            let mut on = v & bit != 0;
            if crate::theme::checkbox(ui, &mut on, crate::i18n::tr_dyn(&c.name)).changed() {
                begin = true;
                if on {
                    v |= bit;
                } else {
                    v &= !bit;
                }
                e.set("spawnflags", v.to_string());
                changed = true;
            }
        }
    }

    if !is_world {
        // Ausgänge (Outputs) mit Prüfung
        ui.separator();
        theme::section(ui, crate::i18n::tr("Ausgänge"));
        let outputs = fgd.all_io(&class, true);
        let mut remove = None;
        for (i, (name, val)) in e.outputs.iter_mut().enumerate() {
            let mut parts: Vec<String> = val.split(['\u{1b}', ',']).map(|s| s.to_string()).collect();
            parts.resize(5, String::new());
            let target_ent = map.entities.iter().find(|x| x.targetname().is_some_and(|n| n.eq_ignore_ascii_case(&parts[0])));
            let target_ok = parts[0].starts_with('!') || parts[0].contains('*') || target_ent.is_some() || map.entities.iter().any(|x| x.classname().eq_ignore_ascii_case(&parts[0]));
            let inputs = target_ent.map(|t| fgd.all_io(t.classname(), false)).unwrap_or_default();
            let input_ok = inputs.is_empty() || inputs.iter().any(|x| x.name.eq_ignore_ascii_case(&parts[1])) || ["kill", "killhierarchy", "addoutput", "fireuser1", "fireuser2", "fireuser3", "fireuser4", "use"].contains(&parts[1].to_ascii_lowercase().as_str());
            let output_ok = outputs.is_empty() || outputs.iter().any(|o| o.name.eq_ignore_ascii_case(name));
            // zwei Zeilen, damit es auch in einem schmalen Panel passt:
            // ✖ Output → Ziel   /   Input ▾  Parameter  Verzög.  Anz.
            let avail = ui.available_width();
            let mut part_changed = false;
            let colors = [if target_ok { None } else { Some(Color32::from_rgb(255, 120, 120)) }, if input_ok { None } else { Some(Color32::from_rgb(255, 180, 80)) }, None, None, None];
            let hints = [crate::i18n::tr("Ziel"), crate::i18n::tr("Input"), crate::i18n::tr("Parameter"), crate::i18n::tr("s"), crate::i18n::tr("×")];
            ui.horizontal(|ui| {
                if ui.small_button("✖").clicked() {
                    remove = Some(i);
                }
                let half = ((avail - 112.0) / 2.0).max(50.0);
                let mut te = egui::TextEdit::singleline(name).desired_width(half).hint_text(crate::theme::hint(crate::i18n::tr("Output")));
                if !output_ok {
                    te = te.text_color(Color32::from_rgb(255, 180, 80));
                }
                let r = ui.add(te);
                begin |= r.gained_focus();
                changed |= r.changed();
                ui.label("→");
                let mut te = egui::TextEdit::singleline(&mut parts[0]).desired_width(half - 16.0).hint_text(crate::theme::hint(hints[0]));
                if let Some(c) = colors[0] {
                    te = te.text_color(c);
                }
                let r = ui.add(te);
                begin |= r.gained_focus();
                part_changed |= r.changed();
                // Vorschläge unter dem Ziel-Feld: Namen aus der Map und !-Sonderziele
                if let Some(pick) = target_suggestions(ui, &r, &parts[0], map, (e.id, i)) {
                    parts[0] = pick;
                    part_changed = true;
                    begin = true;
                }
                // Prüfung nicht nur über Farbe: Warnsymbol mit Erklärung
                if !target_ok || !input_ok || !output_ok {
                    let why = if !target_ok { crate::i18n::tr("Kein Objekt mit diesem Namen in der Map") } else if !input_ok { crate::i18n::tr("Das Ziel kennt diesen Input nicht") } else { crate::i18n::tr("Diese Klasse hat keinen Output mit diesem Namen") };
                    ui.label(RichText::new("⚠").color(if !target_ok { Color32::from_rgb(255, 120, 120) } else { Color32::from_rgb(255, 180, 80) })).on_hover_text(why);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(26.0);
                // Verzögerung und Anzahl fest 44 px, Input/Parameter teilen sich den Rest
                let rest = (avail - 26.0 - 44.0 - 44.0 - 40.0 - 6.0 * 8.0 - 8.0).max(90.0);
                let widths = [0.0, rest * 0.55, rest * 0.45, 44.0, 44.0];
                let tips = ["", crate::i18n::tr("Input des Ziels"), crate::i18n::tr("Parameter (optional)"), crate::i18n::tr("Verzögerung in Sekunden"), crate::i18n::tr("Anzahl Auslösungen (−1 = immer)")];
                for k in 1..5 {
                    let mut te = egui::TextEdit::singleline(&mut parts[k]).desired_width(widths[k]).hint_text(crate::theme::hint(hints[k]));
                    if let Some(c) = colors[k] {
                        te = te.text_color(c);
                    }
                    let r = ui.add(te).on_hover_text(tips[k]);
                    begin |= r.gained_focus();
                    part_changed |= r.changed();
                    if k == 1 && !inputs.is_empty() {
                        egui::ComboBox::from_id_salt(("inp", i)).selected_text("").width(14.0).show_ui(ui, |ui| {
                            for inp in &inputs {
                                if ui.selectable_label(false, &inp.name).on_hover_text(crate::i18n::tr_dyn(&inp.description)).clicked() {
                                    parts[1] = inp.name.clone();
                                    part_changed = true;
                                    begin = true;
                                }
                            }
                        });
                    }
                }
            });
            ui.add_space(4.0);
            if part_changed {
                *val = parts.join(",");
                changed = true;
            }
        }
        if let Some(i) = remove {
            begin = true;
            e.outputs.remove(i);
            changed = true;
        }
        egui::ComboBox::from_id_salt(("addout", e.id)).selected_text(crate::i18n::tr("+ Output hinzufügen")).width(ui.available_width().min(220.0)).show_ui(ui, |ui| {
            for o in &outputs {
                if ui.selectable_label(false, &o.name).on_hover_text(format!("{} ({})", crate::i18n::tr_dyn(&o.description), o.kind)).clicked() {
                    begin = true;
                    e.outputs.push((o.name.clone(), ",,,0,-1".into()));
                    changed = true;
                }
            }
            if ui.selectable_label(false, crate::i18n::tr("(eigener Name)")).clicked() {
                begin = true;
                e.outputs.push(("OnTrigger".into(), ",,,0,-1".into()));
                changed = true;
            }
        });

        // Eingehende Verbindungen
        if let Some(name) = e.targetname().map(|s| s.to_string()) {
            let incoming: Vec<String> = map
                .entities
                .iter()
                .flat_map(|src| {
                    let name = name.clone();
                    src.outputs.iter().filter_map(move |(o, v)| {
                        let parts: Vec<&str> = v.split(['\u{1b}', ',']).collect();
                        (parts.first().is_some_and(|t| t.eq_ignore_ascii_case(&name))).then(|| format!("{} ({}) · {o} → {}", src.classname(), src.targetname().unwrap_or("–"), parts.get(1).unwrap_or(&"")))
                    })
                })
                .collect();
            ui.collapsing(crate::i18n::tr_dyn(&format!("Eingehende Verbindungen ({})", incoming.len())), |ui| {
                for l in incoming {
                    ui.label(RichText::new(l).small());
                }
            });
        }
        let inputs = fgd.all_io(&class, false);
        if !inputs.is_empty() {
            ui.collapsing(crate::i18n::tr_dyn(&format!("Inputs dieser Klasse ({})", inputs.len())), |ui| {
                for i in inputs {
                    ui.label(RichText::new(&i.name).monospace()).on_hover_text(crate::i18n::tr_dyn(&i.description));
                }
            });
        }
    }
    let _ = parse_vec;
    (changed, begin, actions)
}

/// Passende Ziele für ein Output: (Name, Klasse); Treffer am Anfang zuerst, höchstens 10
pub(crate) fn target_candidates(map: &crate::map::Map, typed: &str) -> Vec<(String, String)> {
    let q = typed.trim().to_ascii_lowercase();
    let mut all: Vec<(String, String)> = Vec::new();
    for e in map.entities.iter() {
        if let Some(n) = e.targetname().filter(|n| !n.is_empty()) {
            if !all.iter().any(|(x, _)| x.eq_ignore_ascii_case(n)) {
                all.push((n.to_string(), e.classname().to_string()));
            }
        }
    }
    all.sort_by_key(|a| a.0.to_ascii_lowercase());
    for (n, d) in [("!activator", "auslösende Entity"), ("!caller", "sendende Entity"), ("!self", "diese Entity"), ("!player", "Spieler"), ("!picker", "Spieler-Blickziel")] {
        all.push((n.to_string(), crate::i18n::tr(d).to_string()));
    }
    let mut hits: Vec<(u8, (String, String))> = all
        .into_iter()
        .filter_map(|c| {
            let l = c.0.to_ascii_lowercase();
            if q.is_empty() {
                Some((1, c))
            } else if l == q {
                None
            } else if l.starts_with(&q) {
                Some((0, c))
            } else if l.contains(&q) {
                Some((1, c))
            } else {
                None
            }
        })
        .collect();
    hits.sort_by_key(|h| h.0);
    hits.into_iter().map(|h| h.1).take(10).collect()
}

/// Vorschlagsliste unter dem Ziel-Feld (Pfeiltasten wählen, Enter/Tab übernimmt, Klick auch)
fn target_suggestions(ui: &mut egui::Ui, r: &egui::Response, typed: &str, map: &crate::map::Map, key: (u32, usize)) -> Option<String> {
    suggest_popup(ui, r, ("out_target_sug", key.0, key.1), target_candidates(map, typed), typed.trim().is_empty())
}

/// Klassennamen aus der FGD, die zum Getippten passen: Anfang zuerst, dann enthalten; höchstens 12.
/// Zweiter Wert: kurze Beschreibung
pub(crate) fn class_candidates(fgd: &Fgd, typed: &str) -> Vec<(String, String)> {
    let t = typed.trim().to_ascii_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    let mut pre = Vec::new();
    let mut mid = Vec::new();
    for c in fgd.classes.values().filter(|c| c.kind != crate::fgd::ClassKind::Base) {
        let n = c.name.to_ascii_lowercase();
        if n == t {
            continue;
        }
        let full = crate::i18n::tr_dyn(&c.description);
        let mut desc: String = full.chars().take(30).collect();
        if full.chars().count() > 30 {
            desc.push('…');
        }
        if n.starts_with(&t) {
            pre.push((c.name.clone(), desc));
        } else if n.contains(&t) {
            mid.push((c.name.clone(), desc));
        }
    }
    pre.sort();
    mid.sort();
    pre.extend(mid);
    pre.truncate(12);
    pre
}

/// Vorschlagsliste unter einem Textfeld (Pfeiltasten, Enter/Tab übernimmt, Klick)
fn suggest_popup(ui: &mut egui::Ui, r: &egui::Response, key: (&str, u32, usize), cands: Vec<(String, String)>, empty: bool) -> Option<String> {
    let id = egui::Id::new(key);
    let ctx = ui.ctx().clone();
    // (sichtbar, markierte Zeile, Fläche der Liste im letzten Bild)
    let (mut sel, last_rect): (usize, Option<egui::Rect>) = ctx.data(|d| d.get_temp(id)).unwrap_or((0, None));
    let over_list = last_rect.is_some_and(|lr| ctx.input(|i| i.pointer.interact_pos().is_some_and(|p| lr.contains(p))));
    if !r.has_focus() && !r.lost_focus() && !over_list {
        ctx.data_mut(|d| d.remove::<(usize, Option<egui::Rect>)>(id));
        return None;
    }
    if cands.is_empty() {
        ctx.data_mut(|d| d.remove::<(usize, Option<egui::Rect>)>(id));
        return None;
    }
    if r.changed() {
        sel = 0;
    }
    let mut pick = None;
    if r.has_focus() {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                sel = (sel + 1).min(cands.len() - 1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                sel = sel.saturating_sub(1);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                pick = Some(cands[sel.min(cands.len() - 1)].0.clone());
            }
        });
    }
    // Enter beendet das Einzeilenfeld → markierten Vorschlag übernehmen
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !empty {
        pick = Some(cands[sel.min(cands.len() - 1)].0.clone());
    }
    let th = crate::theme::t();
    let area = egui::Area::new(id.with("area")).order(egui::Order::Foreground).fixed_pos(r.rect.left_bottom() + egui::vec2(0.0, 2.0)).show(&ctx, |ui| {
        egui::Frame::new().fill(th.panel).stroke(egui::Stroke::new(1.0f32, th.line)).corner_radius(6).inner_margin(4).shadow(egui::epaint::Shadow { offset: [0, 4], blur: 12, spread: 0, color: th.shadow }).show(ui, |ui| {
            ui.set_min_width(r.rect.width().max(180.0));
            for (k, (name, class)) in cands.iter().enumerate() {
                let resp = ui.horizontal(|ui| {
                    let b = ui.selectable_label(k == sel, RichText::new(name).monospace());
                    ui.label(RichText::new(class).size(11.0).color(th.text3));
                    b
                });
                if resp.inner.clicked() {
                    pick = Some(name.clone());
                }
                if resp.inner.hovered() {
                    sel = k;
                }
            }
        });
    });
    if pick.is_some() {
        ctx.data_mut(|d| d.remove::<(usize, Option<egui::Rect>)>(id));
        r.surrender_focus();
    } else {
        ctx.data_mut(|d| d.insert_temp(id, (sel, Some(area.response.rect))));
    }
    pick
}

pub(crate) fn parse_color(s: &str) -> Option<[u8; 3]> {
    let v: Vec<u8> = s.split_whitespace().take(3).filter_map(|x| x.parse::<f32>().ok()).map(|f| f.clamp(0.0, 255.0) as u8).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

#[cfg(test)]
mod target_tests {
    #[test]
    fn suggestions_prefix_first() {
        let mut m = crate::map::Map::new();
        for (n, c) in [("tuer_links", "func_door"), ("licht_flur", "light"), ("haupttuer", "func_door_rotating"), ("tuer_links", "func_door")] {
            let id = m.alloc_id();
            let mut e = crate::map::Entity::new(id, c);
            e.set("targetname", n);
            m.entities.push(e.into());
        }
        let c = super::target_candidates(&m, "tue");
        let names: Vec<&str> = c.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(names, vec!["tuer_links", "haupttuer"], "Anfang zuerst, ohne Doppelte");
        assert_eq!(c[0].1, "func_door");
        assert!(super::target_candidates(&m, "!pl").iter().any(|x| x.0 == "!player"));
        // genau getroffen → keine Liste mehr
        assert!(super::target_candidates(&m, "licht_flur").is_empty());
        assert_eq!(super::target_candidates(&m, "").len(), 8, "3 Namen + 5 Sonderziele");
    }
}


/// Breite des Browser-Knopfs neben einem Wertefeld
const BROWSE_W: f32 = 26.0;

/// Gut sichtbarer Knopf mit Ordner-Symbol rechts neben einem Wertefeld (öffnet einen Browser)
fn browse_button(ui: &mut egui::Ui, tip: &str) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(BROWSE_W, 22.0), Sense::click());
    let fill = if resp.hovered() { th.accent_soft } else { th.hover };
    ui.painter().rect(r, 5.0, fill, Stroke::new(1.0f32, if resp.hovered() { th.accent } else { th.line_strong }), egui::StrokeKind::Inside);
    crate::icons::draw(ui.painter(), r.shrink2(Vec2::new(6.0, 4.0)), "folder", if resp.hovered() { th.accent_text } else { th.text }, 1.4);
    resp.on_hover_text(tip)
}

/// Knopf mit Pfeil nach unten rechts neben einem Feld (öffnet eine Liste)
fn list_button(ui: &mut egui::Ui, tip: &str) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(BROWSE_W, 22.0), Sense::click());
    let fill = if resp.hovered() { th.accent_soft } else { th.hover };
    ui.painter().rect(r, 5.0, fill, Stroke::new(1.0f32, if resp.hovered() { th.accent } else { th.line_strong }), egui::StrokeKind::Inside);
    let c = r.center();
    let col = if resp.hovered() { th.accent_text } else { th.text };
    ui.painter().add(egui::Shape::line(vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(0.0, 2.5), c + Vec2::new(4.0, -2.0)], Stroke::new(1.6f32, col)));
    resp.on_hover_text(tip)
}
