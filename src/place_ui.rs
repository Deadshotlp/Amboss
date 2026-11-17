//! Fenster „Vegetation streuen“ und die Live-Simulation „Mit Physik fallen lassen“.

use crate::editor::Obj;
use crate::map::fmt_vec;
use eframe::egui::{self, RichText};

impl crate::app::App {
    /// Ausgewählte Punkt-Entities mit Physik fallen lassen (Live-Simulation, Esc bricht ab)
    pub(crate) fn physics_start(&mut self) {
        let mut props = Vec::new();
        for o in self.ed.sel.iter() {
            let Obj::Entity(id) = o else { continue };
            let Some(e) = self.ed.map.entity(*id) else { continue };
            if e.is_brush_entity() {
                continue;
            }
            let Some(origin) = e.origin() else { continue };
            // Modellmaße, sonst die Größe aus der FGD/Ersatzbox
            let (mn, mx) = match self.ed.model_path(e).and_then(|p| self.ed.model_bounds.get(&p).copied()) {
                Some(b) => (b.min, b.max),
                None => {
                    let b = self.ed.entity_bounds(e);
                    (b.min - origin, b.max - origin)
                }
            };
            props.push((*id, origin, e.angles(), mn, mx));
        }
        if props.is_empty() {
            self.ed.status = crate::i18n::tr("Zuerst Props (Punkt-Entities) auswählen").into();
            return;
        }
        self.ed.checkpoint();
        self.phys = Some(crate::physics::Sim::new(&self.ed.map, &props));
        self.ed.status = crate::i18n::tr_dyn(&format!("Physik: {} fallen – Esc bricht ab", crate::i18n::count_de(props.len(), "Objekt", "Objekte")));
    }

    /// Ein Bild der Physik-Simulation: Positionen in die Map schreiben
    pub(crate) fn physics_tick(&mut self, ctx: &egui::Context) {
        let Some(sim) = self.phys.as_mut() else { return };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.phys = None;
            self.ed.undo();
            self.ed.status = crate::i18n::tr("Physik abgebrochen").into();
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 1.0 / 30.0) as f64;
        sim.step(dt);
        for (id, o, a) in sim.result() {
            if let Some(e) = self.ed.map.entity_mut(id) {
                e.set("origin", fmt_vec(o));
                e.set("angles", fmt_vec(a));
            }
        }
        self.ed.touch();
        if sim.finished() {
            let n = sim.bodies.len();
            let secs = sim.time;
            self.phys = None;
            self.ed.status = crate::i18n::tr_dyn(&format!("Physik: {} liegen nach {} s", crate::i18n::count_de(n, "Objekt", "Objekte"), crate::i18n::dec(format!("{secs:.1}"))));
        } else {
            ctx.request_repaint();
        }
    }

    pub(crate) fn scatter_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.win.scatter {
            self.ed.scatter.active = false;
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut fill = false;
        let next_model = self.ed.next_model.clone();
        let d = theme::Dlg::new(crate::i18n::tr("Vegetation streuen"), 460.0).footer();
        let o = &mut self.ed.scatter;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                theme::section(ui, crate::i18n::tr("Modelle"));
                let mut remove = None;
                for (i, m) in o.models.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(m).font(theme::mono(12.0)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text(crate::i18n::tr("Entfernen")).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    o.models.remove(i);
                }
                if o.models.is_empty() {
                    ui.label(RichText::new(crate::i18n::tr("Noch keine Modelle – im Modell-Browser eines wählen und hier hinzufügen.")).color(th.text3));
                }
                ui.horizontal(|ui| {
                    let ok = !next_model.is_empty() && !o.models.contains(&next_model);
                    if ui.add_enabled(ok, egui::Button::new(crate::i18n::tr("+ Aktuelles Modell"))).on_hover_text(next_model.clone()).clicked() {
                        o.models.push(next_model.clone());
                    }
                });
                theme::section(ui, crate::i18n::tr("Verteilung"));
                egui::Grid::new("scatter").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(crate::i18n::tr("Pinselradius"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.radius, 32.0..=2048.0).suffix(" u").logarithmic(true)));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Dichte"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.density, 0.25..=40.0).logarithmic(true))).on_hover_text(crate::i18n::tr("Objekte je 256 × 256 Einheiten"));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Mindestabstand"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.min_dist, 0.0..=512.0).suffix(" u")));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Größte Neigung"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.max_slope, 0.0..=90.0).suffix("°"))).on_hover_text(crate::i18n::tr("Steilere Hänge bleiben frei"));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Zufällige Neigung"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.tilt, 0.0..=30.0).suffix("°")));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Einsinken"));
                    ui.add(theme::sl(egui::Slider::new(&mut o.sink, -32.0..=64.0).suffix(" u"))).on_hover_text(crate::i18n::tr("Wie tief die Modelle in den Boden ragen (Wurzeln, Steine)"));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Klasse"));
                    egui::ComboBox::from_id_salt("scatter_class").selected_text(o.class.clone()).show_ui(ui, |ui| {
                        for c in ["prop_static", "prop_dynamic", "prop_physics"] {
                            ui.selectable_value(&mut o.class, c.to_string(), c);
                        }
                    });
                    ui.end_row();
                });
                theme::checkbox(ui, &mut o.random_yaw, crate::i18n::tr("Zufällig drehen"));
                theme::checkbox(ui, &mut o.align, crate::i18n::tr("An der Oberfläche ausrichten"));
                ui.add_space(6.0);
                let can = !o.models.is_empty();
                ui.add_enabled_ui(can, |ui| {
                    theme::checkbox(ui, &mut o.active, RichText::new(crate::i18n::tr("Pinsel aktiv – in 3D malen, Shift radiert")).strong());
                });
                if !can {
                    o.active = false;
                }
            }
            theme::Part::Footer => {
                if ui.add_enabled(!o.models.is_empty(), egui::Button::new(crate::i18n::tr("Auswahl füllen"))).on_hover_text(crate::i18n::tr("Oberseiten der ausgewählten Brushes und Displacements bestreuen")).clicked() {
                    fill = true;
                }
                theme::footer_hint(ui, crate::i18n::tr("Ein Pinselstrich ist ein Rückgängig-Schritt"));
            }
            _ => {}
        });
        if fill {
            self.ed.scatter_fill_selection();
        }
        self.win.scatter = open;
        if !open {
            self.ed.scatter.active = false;
        }
    }
}
