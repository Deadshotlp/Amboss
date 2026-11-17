//! Weitere Fenster: Befehlspalette, Objektliste (Outliner), Rückgängig-Verlauf, Tastenkürzel.

use crate::app::App;
use crate::commands::{display_shortcut, parse_shortcut, Cmd};
use crate::editor::Obj;
use eframe::egui::{self, Color32, RichText};

impl App {
    pub(crate) fn windows2(&mut self, ctx: &egui::Context) {
        self.palette_window(ctx);
        self.convert_dialog(ctx);
        self.particle_browser(ctx);
        self.heightmap_window(ctx);
        self.prop_window(ctx);
        self.decompile_window(ctx);
        self.scripts_window(ctx);
        self.material_editor_window(ctx);
        self.particle_editor_window(ctx);
        self.scatter_window(ctx);
        self.procgen_window(ctx);
        self.tiles_window(ctx);
        self.mesh_window(ctx);
        self.live_window(ctx);
        self.poll_mesh_job();
        self.poll_navmesh();
        self.setup_window(ctx);
        self.tutorial_window(ctx);
        self.class_picker_popover(ctx);
        self.outliner_window(ctx);
        self.history_window(ctx);
        self.merge_window(ctx);
        self.versions_window(ctx);
        self.collab_window(ctx);
        self.conflict_toasts(ctx);
    }

    fn outliner_window(&mut self, ctx: &egui::Context) {
        if !self.win.outliner {
            return;
        }
        let mut open = true;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Objektliste"), 380.0).height(560.0).no_scroll(), |ui| self.outliner_contents(ui, ctx));
        self.win.outliner = open;
    }

    /// Inhalt der Objektliste (Fenster und Panel-Tab „Objekte“)
    pub(crate) fn outliner_contents(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut select: Option<(Obj, bool)> = None;
        let mut toggle_hide: Option<Obj> = None;
        let mut toggle_lock: Option<Obj> = None;
        {
            ui.add(egui::TextEdit::singleline(&mut self.dlg.outliner_filter).hint_text(crate::theme::hint(crate::i18n::tr("Filtern (Klasse, Name, Material)"))).desired_width(f32::INFINITY));
            let f = self.dlg.outliner_filter.to_ascii_lowercase();
            ui.label(RichText::new(crate::i18n::tr("Klick wählt · Doppelklick zentriert · 👁 ausblenden · 🔒 sperren")).small());
            ui.separator();
            let ed = &self.ed;
            let row = |ui: &mut egui::Ui, o: Obj, text: String, hidden: bool, locked: bool, sel: &mut Option<(Obj, bool)>, th: &mut Option<Obj>, tl: &mut Option<Obj>| {
                ui.horizontal(|ui| {
                    if ui.small_button(if hidden { "◌" } else { "👁" }).on_hover_text(crate::i18n::tr("Ausblenden/Einblenden")).clicked() {
                        *th = Some(o);
                    }
                    if ui.small_button(if locked { "🔒" } else { "🔓" }).on_hover_text(crate::i18n::tr("Sperren/Entsperren")).clicked() {
                        *tl = Some(o);
                    }
                    let r = ui.selectable_label(ed.sel.contains(&o), RichText::new(text).color(if hidden { Color32::GRAY } else { Color32::from_gray(220) }));
                    if r.double_clicked() {
                        *sel = Some((o, true));
                    } else if r.clicked() {
                        *sel = Some((o, false));
                    }
                });
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let world: Vec<_> = ed
                    .map
                    .world
                    .solids
                    .iter()
                    .filter(|s| f.is_empty() || s.sides.iter().any(|x| x.material.to_ascii_lowercase().contains(&f)) || s.id.to_string() == f)
                    .collect();
                egui::CollapsingHeader::new(crate::i18n::tr_dyn(&format!("Welt-Brushes ({})", world.len()))).id_salt("ol_world").show(ui, |ui| {
                    for s in world.iter().take(2000) {
                        let mat = s.sides.first().map(|x| x.material.to_ascii_lowercase()).unwrap_or_default();
                        let text = format!("#{} {}{}", s.id, mat.rsplit('/').next().unwrap_or(""), if s.has_disp() { " (Terrain)" } else { "" });
                        row(ui, Obj::Solid(s.id), text, s.editor.hidden, s.editor.locked, &mut select, &mut toggle_hide, &mut toggle_lock);
                    }
                });
                let mut classes: Vec<String> = ed.map.entities.iter().map(|e| e.classname().to_string()).collect();
                classes.sort();
                classes.dedup();
                for c in classes {
                    let ents: Vec<_> = ed
                        .map
                        .entities
                        .iter()
                        .filter(|e| e.classname() == c)
                        .filter(|e| f.is_empty() || c.contains(&f) || e.targetname().is_some_and(|n| n.to_ascii_lowercase().contains(&f)))
                        .collect();
                    if ents.is_empty() {
                        continue;
                    }
                    egui::CollapsingHeader::new(format!("{c} ({})", ents.len())).id_salt(("ol", &c)).show(ui, |ui| {
                        for e in ents {
                            let text = format!("#{} {}", e.id, e.targetname().unwrap_or(""));
                            row(ui, Obj::Entity(e.id), text, e.editor.hidden, e.editor.locked, &mut select, &mut toggle_hide, &mut toggle_lock);
                        }
                    });
                }
                if !ed.map.groups.is_empty() {
                    ui.label(RichText::new(crate::i18n::count(ed.map.groups.len(), "Gruppe", "Gruppen")).weak());
                }
            });
        }
        if let Some(o) = toggle_hide.or(toggle_lock) {
            self.ed.checkpoint();
            let hide = toggle_hide.is_some();
            let flip = |e: &mut crate::map::EditorInfo| {
                if hide {
                    e.hidden = !e.hidden;
                } else {
                    e.locked = !e.locked;
                }
            };
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.ed.map.solid_mut(id) {
                        flip(&mut s.editor);
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.ed.map.entity_mut(id) {
                        flip(&mut e.editor);
                    }
                }
            }
            self.ed.sel.remove(&o);
            self.ed.touch();
        }
        if let Some((o, center)) = select {
            let add = ctx.input(|i| i.modifiers.command);
            if !add {
                self.ed.sel.clear();
            }
            self.ed.sel.insert(o);
            self.ed.touch();
            if center {
                self.center_on(self.ed.obj_bounds(o));
            }
        }
    }

    fn history_window(&mut self, ctx: &egui::Context) {
        if !self.win.history {
            return;
        }
        let mut open = true;
        let (undo, redo) = self.ed.history();
        let mut target: Option<(bool, usize)> = None;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Rückgängig-Verlauf"), 360.0), |ui| {
            ui.label(RichText::new(crate::i18n::tr("Klick springt zu diesem Stand")).small());
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (i, l) in undo.iter().enumerate() {
                    if ui.selectable_label(false, crate::i18n::tr_dyn(&format!("{}. {l}", i + 1))).on_hover_text(crate::i18n::tr("Bis vor diese Aktion zurückgehen")).clicked() {
                        target = Some((true, undo.len() - i));
                    }
                }
                ui.label(RichText::new(crate::i18n::tr("▶ Aktueller Stand")).strong().color(Color32::LIGHT_GREEN));
                for (i, l) in redo.iter().enumerate() {
                    if ui.selectable_label(false, RichText::new(crate::i18n::tr_dyn(&format!("↷ {l}"))).color(Color32::GRAY)).on_hover_text(crate::i18n::tr("Wiederholen bis hier")).clicked() {
                        target = Some((false, i + 1));
                    }
                }
            });
        });
        if let Some((is_undo, n)) = target {
            for _ in 0..n {
                if is_undo {
                    self.ed.undo();
                } else {
                    self.ed.redo();
                }
            }
        }
        self.win.history = open;
    }

    /// Steuerung der 3D-Kamera (in den Einstellungen)
    pub(crate) fn camera_controls_ui(&mut self, ui: &mut egui::Ui) {
        use crate::i18n::tr;
        ui.label(RichText::new(tr("3D-Kamera: Bewegen und Umsehen")).strong());
        if let Some(slot) = self.dlg.cam_capture {
            ui.colored_label(Color32::YELLOW, crate::i18n::tr_dyn(&format!("Neue Taste für „{}“ drücken… (Esc bricht ab)", tr(crate::compile::CamControls::SLOTS[slot]))));
        }
        let cam = &mut self.settings.cam;
        egui::Grid::new("camkeys").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
            ui.label(tr("Umsehen"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut cam.look, "middle".to_string(), tr("Mittlere Maustaste (Mausrad drücken)"));
                ui.radio_value(&mut cam.look, "right".to_string(), tr("Rechte Maustaste"));
            });
            ui.end_row();
            ui.label(tr("Empfindlichkeit"));
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut cam.sensitivity, 0.05..=1.0).fixed_decimals(2));
                crate::theme::checkbox(ui, &mut cam.invert_y, tr("Maus-Y umkehren"));
            });
            ui.end_row();
            ui.label(tr("Schneller fliegen"));
            ui.horizontal(|ui| {
                for (v, l) in [("Shift", tr("Umschalt")), ("Alt", tr("Alt")), ("", tr("aus"))] {
                    ui.radio_value(&mut cam.fast, v.to_string(), l);
                }
            });
            ui.end_row();
            for (i, label) in crate::compile::CamControls::SLOTS.iter().enumerate() {
                ui.label(tr(label));
                ui.horizontal(|ui| {
                    let k = cam.slot_mut(i).clone();
                    ui.label(RichText::new(if k.is_empty() { "—".to_string() } else { display_shortcut(&k) }).monospace());
                    if ui.small_button(tr("Ändern")).clicked() {
                        self.dlg.cam_capture = Some(i);
                    }
                    if ui.small_button("✖").on_hover_text(tr("Keine Taste")).clicked() {
                        cam.slot_mut(i).clear();
                    }
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            if ui.button(tr("Kamera-Steuerung zurücksetzen")).clicked() {
                self.settings.cam = crate::compile::CamControls::default();
            }
        });
        ui.label(RichText::new(tr("Ohne gedrückte Umsehen-Taste bewegen die Tasten die Kamera nur ohne Umschalt/Alt – Shift+Buchstabe wählt dann das Werkzeug (z. B. Shift+A = Textur). Beim Umsehen gehören alle Tasten der Kamera.")).small().color(crate::theme::t().text3));
    }

    /// Tastenkürzel-Editor (in den Einstellungen)
    pub(crate) fn keymap_ui(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new(crate::i18n::tr("Tastenkürzel")).strong()).id_salt("keymap").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.dlg.keymap_filter).hint_text(crate::theme::hint(crate::i18n::tr("Suchen…"))).desired_width(200.0));
                if ui.button(crate::i18n::tr("Alle zurücksetzen")).clicked() {
                    self.settings.keymap.clear();
                }
            });
            if let Some(c) = self.dlg.keymap_capture {
                ui.colored_label(Color32::YELLOW, crate::i18n::tr_dyn(&format!("Neue Taste für „{}“ drücken… (Esc bricht ab)", c.label())));
            }
            let f = self.dlg.keymap_filter.to_ascii_lowercase();
            // doppelte Belegungen markieren
            let mut used: std::collections::HashMap<String, usize> = Default::default();
            for c in Cmd::all() {
                let k = self.key_for(c).to_ascii_lowercase();
                if !k.is_empty() {
                    *used.entry(k).or_default() += 1;
                }
            }
            egui::ScrollArea::vertical().id_salt("keymap_scroll").max_height(260.0).show(ui, |ui| {
                egui::Grid::new("keys").num_columns(3).striped(true).show(ui, |ui| {
                    for c in Cmd::all() {
                        let label = c.label();
                        if !f.is_empty() && !label.to_ascii_lowercase().contains(&f) {
                            continue;
                        }
                        let key = self.key_for(c);
                        ui.label(&label);
                        let dup = used.get(&key.to_ascii_lowercase()).copied().unwrap_or(0) > 1;
                        let valid = key.is_empty() || parse_shortcut(&key).is_some();
                        let color = if !valid || dup { Color32::from_rgb(255, 130, 130) } else { Color32::from_gray(220) };
                        ui.label(RichText::new(if key.is_empty() { "—".into() } else { display_shortcut(&key) }).monospace().color(color));
                        ui.horizontal(|ui| {
                            if ui.small_button(crate::i18n::tr("Ändern")).clicked() {
                                self.dlg.keymap_capture = Some(c);
                            }
                            if ui.small_button("✖").on_hover_text(crate::i18n::tr("Kein Kürzel")).clicked() {
                                self.settings.keymap.insert(c.id(), String::new());
                            }
                            if self.settings.keymap.contains_key(&c.id()) && ui.small_button("↺").on_hover_text(crate::i18n::tr("Standard")).clicked() {
                                self.settings.keymap.remove(&c.id());
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        });
    }
}
