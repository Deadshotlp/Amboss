//! Alle Fenster/Dialoge des Editors.

use crate::app::{App, Pending};
use crate::compile::{JobState, RadMode, Step, VisMode};
use crate::editor::Obj;
use crate::geometry::Aabb;
use crate::panels::entity_editor;
use crate::problems::Severity;
use crate::transform::Xform;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, Vec2};
use glam::{DMat3, DVec3};

impl App {
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.pending_window(ctx);
        self.world_props_window(ctx);
        self.compile_window(ctx);
        self.settings_window(ctx);
        self.help_window(ctx);
        self.about_window(ctx);
        self.texture_browser(ctx);
        self.transform_window(ctx);
        self.paste_special_window(ctx);
        self.replace_window(ctx);
        self.problems_window(ctx);
        self.entity_report_window(ctx);
        self.map_info_window(ctx);
        self.goto_window(ctx);
        self.prefab_window(ctx);
        self.optimize_window(ctx);
        self.model_browser(ctx);
        self.sound_browser(ctx);
        self.logical_view(ctx);
        self.entity_help_window(ctx);
        self.windows2(ctx);
    }

    fn pending_window(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.clone() else { return };
        let th = crate::theme::t();
        let mut open = true;
        let mut act = 0u8;
        let name = self.ed.map_name();
        let steps = self.ed.history().0.len();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Ungespeicherte Änderungen"), 440.0).footer().centered().no_scroll(), |ui, part| match part {
            crate::theme::Part::Body => {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 10.0, th.accent_soft);
                    crate::icons::draw(ui.painter(), r.shrink(9.0), "folder", th.accent_text, 1.9);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(RichText::new(crate::i18n::tr("Änderungen an")).font(egui::FontId::new(15.0, crate::theme::semibold())));
                            ui.label(RichText::new(&name).font(crate::theme::mono(14.0)));
                            ui.label(RichText::new(crate::i18n::tr("speichern?")).font(egui::FontId::new(15.0, crate::theme::semibold())));
                        });
                        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{steps} Schritt(e) im Verlauf. Ohne Speichern gehen die Änderungen verloren."))).color(th.text2));
                    });
                });
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, crate::i18n::tr("Speichern")).clicked() {
                    act = 1;
                }
                if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    act = 3;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if crate::theme::danger_button(ui, crate::i18n::tr("Nicht speichern")).clicked() {
                        act = 2;
                    }
                });
            }
            _ => {}
        });
        if !open {
            act = 3;
        }
        match act {
            1 => {
                self.pending = None;
                if self.save() {
                    self.run_pending(ctx, p.clone());
                }
            }
            2 => {
                self.pending = None;
                self.ed.dirty = false;
                self.run_pending(ctx, p.clone());
            }
            3 => self.pending = None,
            _ => {}
        }
        let _ = Pending::New;
    }

    fn world_props_window(&mut self, ctx: &egui::Context) {
        if !self.win.world_props {
            return;
        }
        let mut open = true;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Map-Eigenschaften (worldspawn)"), 460.0), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut w = self.ed.map.world.clone();
                let (changed, begin, _) = entity_editor(ui, &self.ed.fgd, &self.ed.map, &mut w, &mut self.new_kv, true);
                if begin {
                    self.ed.checkpoint();
                }
                if changed {
                    self.ed.map.world.props = w.props;
                    self.ed.dirty = true;
                }
            });
        });
        self.win.world_props = open;
    }

    fn compile_window(&mut self, ctx: &egui::Context) {
        if !self.win.compile {
            self.pack.shown = false;
            return;
        }
        if !self.pack.shown {
            // frisch geöffnet: Packliste zur aktuellen Map neu ermitteln
            self.pack.shown = true;
            self.pack.scanned = false;
            self.pack.log.clear();
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut goto: Option<u32> = None;
        let mut open_opt = false;
        let mut start = false;
        let mut cancel = false;
        let mut pack_only = false;
        let running = self.job.as_ref().is_some_and(|j| j.state() == JobState::Running);
        let subtitle = self.ed.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| crate::i18n::tr("nicht gespeichert").into());
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Kompilieren & Exportieren"), 760.0).subtitle(subtitle).footer().header_extra().height(760.0), |ui, part| match part {
            crate::theme::Part::Header => {
                // eigene Kompilierschritte nur in der Experten-Oberfläche
                if !self.settings.simple_ui {
                    ui.add_enabled_ui(!running, |ui| {
                        let n = crate::theme::segmented(ui, self.settings.expert as usize, &[crate::i18n::tr("Standard"), crate::i18n::tr("Eigene Schritte")], false);
                        self.settings.expert = n == 1;
                    });
                }
            }
            crate::theme::Part::Body => {
                ui.add_enabled_ui(!running, |ui| {
                    if !self.settings.expert || self.settings.simple_ui {
                        {
                            let s = &mut self.settings;
                            ui.columns(2, |c| {
                                crate::theme::section(&mut c[0], "Sichtbarkeit · VVIS");
                                let i = match s.vis { VisMode::Normal => 0, VisMode::Fast => 1, VisMode::Off => 2 };
                                let n = crate::theme::segmented(&mut c[0], i, &[crate::i18n::tr("Normal"), crate::i18n::tr("Schnell"), crate::i18n::tr("Aus")], true);
                                s.vis = [VisMode::Normal, VisMode::Fast, VisMode::Off][n];
                                c[1].horizontal(|ui| {
                                    crate::theme::section(ui, crate::i18n::tr("Licht · VRAD"));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        crate::theme::checkbox(ui, &mut s.hdr, crate::i18n::tr("HDR"));
                                    });
                                });
                                let i = match s.rad { RadMode::Normal => 0, RadMode::Fast => 1, RadMode::Final => 2, RadMode::Off => 3 };
                                let n = crate::theme::segmented(&mut c[1], i, &[crate::i18n::tr("Normal"), crate::i18n::tr("Schnell"), crate::i18n::tr("Final"), crate::i18n::tr("Aus")], true);
                                s.rad = [RadMode::Normal, RadMode::Fast, RadMode::Final, RadMode::Off][n];
                            });
                            ui.add_space(8.0);
                            crate::theme::section(ui, "Vorher");
                            ui.horizontal(|ui| {
                                crate::theme::checkbox(ui, &mut s.opt.compile_auto, crate::i18n::tr("Automatische Optimierungen")).on_hover_text(crate::i18n::tr("Läuft auf einer Kopie: KI-Nodes, Leckprüfung, func_detail, nodraw, Cubemaps…"));
                                if ui.link(crate::i18n::tr("Einstellen…")).clicked() {
                                    open_opt = true;
                                }
                            });
                            ui.add_space(6.0);
                            crate::theme::section(ui, "Exportieren");
                            crate::theme::checkbox(ui, &mut s.opt.navmesh, crate::i18n::tr("Navmesh für NextBots erzeugen")).on_hover_text(crate::i18n::tr("Eingebauter Generator: begehbare Bereiche ab den Spawnpunkten, gespeichert als maps/<map>.nav und mit „Inhalte packen“ in die BSP"));
                            crate::theme::checkbox(ui, &mut s.copy_to_maps, crate::i18n::tr("BSP in den maps-Ordner des Spiels kopieren"));
                            crate::theme::checkbox(ui, &mut s.opt.pack_content, crate::i18n::tr("Eigene Inhalte in die BSP packen")).on_hover_text(crate::i18n::tr("Materialien, Modelle, Sounds, Partikel, Soundscapes und Nav-Mesh, die nicht zum Spiel gehören, werden nach VRAD eingepackt"));
                        }
                        if self.settings.opt.pack_content {
                            ui.indent("packsec", |ui| self.pack_section(ui));
                        }
                        ui.label(RichText::new(crate::i18n::tr("Die Liste der benötigten Workshop-Addons wird immer als <map>_addons.txt neben die VMF geschrieben.")).size(11.0).color(th.text3));
                        let s = &mut self.settings;
                        ui.add_space(6.0);
                        crate::theme::section(ui, "Danach");
                        ui.horizontal_wrapped(|ui| {
                            crate::theme::checkbox(ui, &mut s.build_cubemaps, crate::i18n::tr("Cubemaps bauen"));
                            crate::theme::checkbox(ui, &mut s.launch_game, crate::i18n::tr("Spiel starten"));
                        });
                        ui.add_space(4.0);
                        if !s.simple_ui {
                        egui::CollapsingHeader::new(crate::i18n::tr("Zusatzparameter für VBSP/VVIS/VRAD")).id_salt("extraparams").show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let w = (ui.available_width() - 16.0) / 3.0;
                                for (l, v) in [("VBSP", &mut s.extra_vbsp), ("VVIS", &mut s.extra_vvis), ("VRAD", &mut s.extra_vrad)] {
                                    ui.add(egui::TextEdit::singleline(v).hint_text(crate::theme::hint(l)).font(crate::theme::mono(12.0)).desired_width(w));
                                }
                            });
                        });
                        }
                    } else {
                        if self.settings.steps.is_empty() {
                            self.settings.steps = self.settings.default_steps();
                        }
                        ui.label(RichText::new(crate::i18n::tr("Programme: Pfad oder $vbsp/$vvis/$vrad/$bspzip/$game/$gamewait/$copy · Variablen: $vmf $path $file $gamedir $bspdir")).size(12.0).color(th.text3));
                        let mut remove = None;
                        let mut up = None;
                        let n = self.settings.steps.len();
                        egui::ScrollArea::vertical().id_salt("steps").max_height(200.0).show(ui, |ui| {
                            for (i, st) in self.settings.steps.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    crate::theme::checkbox(ui, &mut st.enabled, "");
                                    ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(100.0));
                                    ui.add(egui::TextEdit::singleline(&mut st.exe).font(crate::theme::mono(12.0)).desired_width(110.0));
                                    ui.add(egui::TextEdit::singleline(&mut st.args).font(crate::theme::mono(12.0)).desired_width(300.0));
                                    if i > 0 && ui.small_button("▲").on_hover_text(crate::i18n::tr("nach oben")).clicked() {
                                        up = Some(i);
                                    }
                                    if ui.small_button("×").on_hover_text("entfernen").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                        });
                        if let Some(i) = remove {
                            self.settings.steps.remove(i);
                        }
                        if let Some(i) = up {
                            if i < n {
                                self.settings.steps.swap(i, i - 1);
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.button(crate::i18n::tr("+ Schritt")).clicked() {
                                self.settings.steps.push(Step { enabled: true, name: crate::i18n::tr("Neu").into(), exe: String::new(), args: String::new() });
                            }
                            if ui.button(crate::i18n::tr("Auf Standard zurücksetzen")).clicked() {
                                self.settings.steps = self.settings.default_steps();
                            }
                        });
                    }
                });
                // Hinweise
                if self.ed.map.cordon.is_some_and(|c| c.active) {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 1);
                        ui.label(crate::i18n::tr("Cordon aktiv: nur der Bereich wird kompiliert"));
                    });
                }
                if self.settings.game_dir.is_empty() {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 0);
                        ui.label(crate::i18n::tr("Spielordner fehlt (Bearbeiten → Einstellungen)"));
                    });
                }
                if !self.problems.is_empty() {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 1);
                        ui.label(crate::i18n::tr_dyn(&format!("{} mögliche Probleme", self.problems.len())));
                        if ui.link(crate::i18n::tr("Anzeigen")).clicked() {
                            self.win.problems = true;
                        }
                    });
                }
                ui.add_space(6.0);
                // Protokoll
                let (mut nerr, mut nwarn) = (0, 0);
                if let Some(j) = &self.job {
                    for l in j.log.lock().unwrap().iter() {
                        let lower = l.to_ascii_lowercase();
                        if lower.contains("error") || lower.contains("fehl") || lower.contains("leaked") {
                            nerr += 1;
                        } else if lower.contains("warning") || lower.contains("warnung") || lower.contains("skipped") || lower.contains("übersprungen") {
                            nwarn += 1;
                        }
                    }
                }
                egui::Frame::new().fill(th.vp).stroke(Stroke::new(1.0f32, th.line)).corner_radius(10).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 7)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(crate::i18n::tr("Protokoll")).size(12.0).color(th.text3));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Fehler: {nerr} · Warnungen: {nwarn}"))).size(12.0).color(th.text3));
                            });
                        });
                    });
                    let r = ui.min_rect();
                    ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0f32, th.line));
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                            match &self.job {
                                None => {
                                    ui.label(RichText::new(crate::i18n::tr("Noch nicht kompiliert.")).font(crate::theme::mono(11.5)).color(th.text3));
                                }
                                Some(j) => {
                                    let log = j.log.lock().unwrap();
                                    for line in log.iter() {
                                        let lower = line.to_ascii_lowercase();
                                        let color = if lower.contains("error") || lower.contains("fehl") || lower.contains("failed") || lower.contains("leak") {
                                            th.err
                                        } else if lower.contains("warning") || lower.contains("warnung") || lower.contains("skipped") || lower.contains("übersprungen") {
                                            th.warn
                                        } else {
                                            th.text2
                                        };
                                        let text = RichText::new(line).font(crate::theme::mono(11.5)).color(color);
                                        match crate::compile::find_id_in_line(line) {
                                            Some(id) => {
                                                if ui.add(egui::Label::new(text.underline()).sense(Sense::click())).on_hover_text(crate::i18n::tr("Klicken: zu diesem Objekt springen")).clicked() {
                                                    goto = Some(id);
                                                }
                                            }
                                            None => {
                                                ui.add(egui::Label::new(text).wrap());
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    });
                });
            }
            crate::theme::Part::Footer => {
                if running {
                    if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                        cancel = true;
                    }
                    ui.spinner();
                    crate::theme::footer_hint(ui, crate::i18n::tr("Läuft…"));
                } else {
                    if crate::theme::primary_button(ui, crate::i18n::tr("Kompilieren & Exportieren")).on_hover_text("F9").clicked() {
                        start = true;
                    }
                    if self.settings.opt.pack_content && (!self.settings.expert || self.settings.simple_ui) && crate::theme::secondary_button(ui, crate::i18n::tr("Nur packen")).on_hover_text(crate::i18n::tr("Eigene Inhalte sofort in die zuletzt kompilierte BSP packen, ohne neu zu kompilieren")).clicked() {
                        pack_only = true;
                    }
                    let hint = match self.job.as_ref().map(|j| j.state()) {
                        Some(JobState::Done) => crate::i18n::tr("Letzter Lauf erfolgreich"),
                        Some(JobState::Failed) => crate::i18n::tr("Letzter Lauf fehlgeschlagen"),
                        Some(JobState::Cancelled) => crate::i18n::tr("Letzter Lauf abgebrochen"),
                        _ => crate::i18n::tr("Speichert die Map vor dem Kompilieren"),
                    };
                    crate::theme::footer_hint(ui, hint);
                }
            }
        });
        if start {
            self.start_compile();
        }
        if pack_only {
            self.pack_now();
        }
        if cancel {
            if let Some(j) = &self.job {
                j.cancel();
            }
        }
        self.win.compile = open;
        if open_opt {
            self.win.optimize = true;
        }
        if let Some(id) = goto {
            self.goto_id(id);
        }
    }

    pub(crate) fn goto_id(&mut self, id: u32) {
        match self.ed.find_by_id(id) {
            Some(o) => {
                self.ed.sel.clear();
                self.ed.sel.insert(o);
                self.ed.touch();
                let b = self.ed.obj_bounds(o);
                self.center_on(b);
                self.ed.status = format!("Objekt {id} ausgewählt");
            }
            None => self.ed.status = format!("Keine Brush/Seite/Entity mit Nummer {id}"),
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.win.settings {
            return;
        }
        let mut open = true;
        let mut reload = false;
        let mut reload_fgd = false;
        let mut page = self.dlg.settings_page;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Einstellungen"), 860.0).height(720.0).no_scroll(), |ui| {
            let th = crate::theme::t();
            ui.horizontal_top(|ui| {
            // linke Navigation
            ui.vertical(|ui| {
                ui.set_width(180.0);
                for (i, l) in [crate::i18n::tr("Oberfläche"), crate::i18n::tr("Spiel & Pfade"), crate::i18n::tr("Spielinhalte"), crate::i18n::tr("KI-Steuerung (MCP)"), crate::i18n::tr("Tastenkürzel"), crate::i18n::tr("Autosave")].into_iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(176.0, 32.0), Sense::click());
                    if i == page {
                        ui.painter().rect_filled(r, 7.0, th.accent_soft);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 7.0, th.hover);
                    }
                    ui.painter().text(r.left_center() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_CENTER, l, crate::theme::sans(13.0), if i == page { th.accent_text } else { th.text });
                    if resp.clicked() {
                        page = i;
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
            egui::ScrollArea::vertical().id_salt(("settings_page", page)).auto_shrink([false, false]).show(ui, |ui| {
              if page == 0 {
                // Oberfläche
                ui.label(RichText::new(crate::i18n::tr("Oberfläche")).strong());
                ui.horizontal(|ui| {
                    let n = crate::theme::segmented(ui, (!self.settings.simple_ui) as usize, &[crate::i18n::tr("Einfach"), crate::i18n::tr("Experte")], false);
                    self.settings.simple_ui = n == 0;
                    ui.label(RichText::new(if self.settings.simple_ui {
                        crate::i18n::tr("Grundwerkzeuge, kurze Menüs – ideal zum Einstieg")
                    } else {
                        crate::i18n::tr("Alle Werkzeuge und Spezialfunktionen")
                    }).small().weak());
                });
              }
              if page == 1 {
                // Spielprofile
                ui.label(RichText::new(crate::i18n::tr("Spielprofil")).strong());
                ui.horizontal(|ui| {
                    let names: Vec<String> = self.settings.profiles.iter().map(|p| p.name.clone()).collect();
                    let cur = names.get(self.settings.active_profile).cloned().unwrap_or_default();
                    egui::ComboBox::from_id_salt("profile").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == self.settings.active_profile, n).clicked() && i != self.settings.active_profile {
                                self.settings.load_profile(i);
                                reload = true;
                                reload_fgd = true;
                            }
                        }
                    });
                    if ui.button(crate::i18n::tr("Neu")).clicked() {
                        self.settings.store_profile();
                        let mut p = self.settings.profile();
                        p.name = format!("Profil {}", self.settings.profiles.len() + 1);
                        self.settings.profiles.push(p);
                        self.settings.active_profile = self.settings.profiles.len() - 1;
                    }
                    if self.settings.profiles.len() > 1 && ui.button(crate::i18n::tr("Löschen")).clicked() {
                        let i = self.settings.active_profile;
                        self.settings.profiles.remove(i);
                        self.settings.active_profile = 0;
                        self.settings.load_profile(0);
                        reload = true;
                        reload_fgd = true;
                    }
                    let i = self.settings.active_profile;
                    if let Some(p) = self.settings.profiles.get_mut(i) {
                        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(140.0).hint_text(crate::theme::hint(crate::i18n::tr("Name"))));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("🔍 GMod automatisch finden")).clicked() {
                        match self.settings.autodetect() {
                            Ok(p) => self.ed.status = format!("Garry's Mod gefunden: {p}"),
                            Err(e) => self.ed.status = e,
                        }
                        reload = true;
                        reload_fgd = true;
                    }
                    if ui.button(crate::i18n::tr("📁 Spielordner wählen…")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title(crate::i18n::tr("GarrysMod-Hauptordner (enthält 'garrysmod' und 'bin')")).pick_folder() {
                            self.settings.apply_root(&dir);
                            reload = true;
                            reload_fgd = true;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Map-Grenze ±"));
                    let vals = [16384.0, 32768.0, 65536.0];
                    let cur = vals.iter().position(|v| *v == self.settings.map_limit).unwrap_or(usize::MAX);
                    let n = crate::theme::segmented(ui, cur, &["16384", "32768", "65536"], false);
                    if n < vals.len() {
                        self.settings.map_limit = vals[n];
                    }
                    ui.add(crate::theme::dv(egui::DragValue::new(&mut self.settings.map_limit)).range(4096.0..=131072.0).speed(256.0).suffix(" u")).on_hover_text(crate::i18n::tr("Eigener Wert"));
                });
                ui.label(RichText::new(crate::i18n::tr("Source/GMod: 16384 – größer nur für Spiele mit erweitertem Limit")).small().weak());
              }
              if page == 1 {
                let before_dir = self.settings.game_dir.clone();
                egui::Grid::new("paths").num_columns(3).striped(true).show(ui, |ui| {
                    let row = |ui: &mut egui::Ui, label: &str, val: &mut String, folder: bool, tip: &str| {
                        ui.label(crate::i18n::tr_dyn(label)).on_hover_text(crate::i18n::tr_dyn(tip));
                        ui.add_sized([420.0, 20.0], egui::TextEdit::singleline(val));
                        if ui.small_button("…").clicked() {
                            let r = if folder { rfd::FileDialog::new().pick_folder() } else { rfd::FileDialog::new().pick_file() };
                            if let Some(p) = r {
                                *val = p.display().to_string().replace('\\', "/");
                            }
                        }
                        ui.end_row();
                    };
                    let s = &mut self.settings;
                    row(ui, crate::i18n::tr("Spielordner"), &mut s.game_dir, true, ".../GarrysMod/garrysmod");
                    row(ui, "VBSP", &mut s.vbsp, false, ".../GarrysMod/bin/vbsp.exe");
                    row(ui, "VVIS", &mut s.vvis, false, ".../GarrysMod/bin/vvis.exe");
                    row(ui, "VRAD", &mut s.vrad, false, ".../GarrysMod/bin/vrad.exe");
                    row(ui, "Spiel (exe)", &mut s.game_exe, false, ".../GarrysMod/hl2.exe oder gmod.exe");
                    row(ui, "Prefab-Ordner", &mut s.prefab_dir, true, crate::i18n::tr("Ordner für gespeicherte Prefabs"));
                    ui.label(crate::i18n::tr("Startparameter")).on_hover_text("{map} wird durch den Map-Namen ersetzt");
                    ui.add_sized([420.0, 20.0], egui::TextEdit::singleline(&mut s.launch_args));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Wrapper")).on_hover_text(crate::i18n::tr("Unter Linux/macOS z. B. 'wine' für die Windows-Compiler. Unter Windows leer lassen."));
                    ui.add_sized([420.0, 20.0], egui::TextEdit::singleline(&mut s.wrapper));
                    ui.end_row();
                });
                // Spielordner geändert: kurz nach der letzten Eingabe neu laden
                if self.settings.game_dir != before_dir {
                    self.dlg.gamedir_changed = Some(std::time::Instant::now());
                }
                if let Some(t0) = self.dlg.gamedir_changed {
                    if t0.elapsed().as_secs_f32() > 1.0 {
                        self.dlg.gamedir_changed = None;
                        reload = true;
                    } else {
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
                    }
                }
                if ui
                    .checkbox(&mut self.settings.load_addons, crate::i18n::tr("Inhalte aus Addons laden (Workshop-.gma und addons-Ordner)"))
                    .on_hover_text(crate::i18n::tr("Aus: nur Inhalte aus dem Spielverzeichnis (VPKs, garrysmod-Ordner, eingebundene Spiele). So siehst du, was ohne Addons fehlt – fehlende Texturen erscheinen dann als Ersatzfarbe."))
                    .changed()
                {
                    reload = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Spielinhalte neu laden")).on_hover_text(crate::i18n::tr("VPKs, Addons und Materiallisten aus dem Spielordner neu einlesen")).clicked() {
                        reload = true;
                    }
                    let info = if self.content_rx.is_some() { crate::i18n::tr("lädt…").to_string() } else if self.content_status.is_empty() { format!("{} Materialien", self.mats.list.len()) } else { crate::i18n::tr_dyn(&self.content_status) };
                    ui.add(egui::Label::new(RichText::new(info).small()).wrap());
                });
              }
              if page == 1 {
                ui.label(RichText::new(crate::i18n::tr("FGD-Dateien (Entity-Definitionen)")).strong());
                ui.label(RichText::new(crate::i18n::tr("Für GMod: GarrysMod/bin/garrysmod.fgd (lädt base.fgd und halflife2.fgd automatisch mit).")).small());
                let mut remove = None;
                for (i, f) in self.settings.fgd_files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                        ui.label(f);
                    });
                }
                if let Some(i) = remove {
                    self.settings.fgd_files.remove(i);
                    reload_fgd = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("FGD hinzufügen…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("FGD", &["fgd"]).pick_file() {
                            self.settings.fgd_files.push(p.display().to_string().replace('\\', "/"));
                            reload_fgd = true;
                        }
                    }
                    if ui.button(crate::i18n::tr("Neu laden")).clicked() {
                        reload_fgd = true;
                    }
                    ui.label(crate::i18n::tr_dyn(&format!("{} Entity-Klassen geladen", self.ed.fgd.classes.len())));
                });
                for m in &self.fgd_messages {
                    ui.colored_label(Color32::YELLOW, m);
                }
              }
              if page == 2 {
                ui.label(RichText::new(crate::i18n::tr("Spielinhalte")).strong());
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Inhalte neu einlesen")).clicked() {
                        reload = true;
                    }
                    ui.label(crate::i18n::tr_dyn(&format!("{} Materialien · {} Modelle · {} Sounds", self.mats.list.len(), self.models.list.len(), self.dlg.sound_list.len())));
                });
                for m in self.mats.messages.iter().chain(self.models.messages.iter()).take(8) {
                    ui.colored_label(Color32::YELLOW, RichText::new(m).small());
                }
              }
              if page == 3 {
                self.mcp_settings_ui(ui);
              }
              if page == 4 {
                self.camera_controls_ui(ui);
                ui.add_space(8.0);
                self.keymap_ui(ui);
              }
              if page == 5 {
                ui.label(RichText::new(crate::i18n::tr("Autosave")).strong());
                egui::Grid::new("autosave").num_columns(2).show(ui, |ui| {
                    ui.label(crate::i18n::tr("Intervall (Minuten, 0 = aus)"));
                    ui.add(crate::theme::dv(egui::DragValue::new(&mut self.settings.autosave_minutes)).range(0..=60).suffix(" min"));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Anzahl Sicherungen"));
                    ui.add(crate::theme::dv(egui::DragValue::new(&mut self.settings.autosave_keep)).range(1..=50));
                    ui.end_row();
                });
              }
              if page == 0 {
                ui.add_space(10.0);
                ui.label(RichText::new(crate::i18n::tr("Sprache / Language")).strong());
                {
                    let cur = crate::i18n::Lang::ALL.iter().position(|l| *l == crate::i18n::lang()).unwrap_or(0);
                    let names: Vec<&str> = crate::i18n::Lang::ALL.iter().map(|l| l.native()).collect();
                    let n = crate::theme::segmented(ui, cur, &names, false);
                    if n != cur {
                        let l = crate::i18n::Lang::ALL[n];
                        crate::i18n::set_lang(l);
                        self.settings.language = l.code().into();
                    }
                }
                ui.add_space(10.0);
                ui.label(RichText::new(crate::i18n::tr("Darstellung")).strong());
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Design"));
                    let before = self.settings.theme_dark;
                    let n = crate::theme::segmented(ui, (!self.settings.theme_dark) as usize, &[crate::i18n::tr("Dunkel"), crate::i18n::tr("Hell")], false);
                    self.settings.theme_dark = n == 0;
                    if before != self.settings.theme_dark {
                        crate::theme::apply(ctx, self.settings.theme_dark);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Kantenglättung (MSAA)"));
                    let cur = self.dlg.msaa.get_or_insert_with(crate::gfx_msaa_pub);
                    let before = *cur;
                    let vals = [0u16, 2, 4, 8];
                    let i = vals.iter().position(|v| v == cur).unwrap_or(0);
                    let n = crate::theme::segmented(ui, i, &[crate::i18n::tr("aus"), "2×", "4×", "8×"], false);
                    *cur = vals[n];
                    if *cur != before {
                        let v = *cur;
                        self.ed.status = match crate::set_gfx_msaa(v) {
                            Ok(()) => crate::i18n::tr("Kantenglättung gilt ab dem nächsten Start. Startet Amboss danach nicht, die Datei grafik.txt im Einstellungsordner löschen.").into(),
                            Err(e) => format!("Konnte Grafik-Einstellung nicht speichern: {e}"),
                        };
                    }
                });
              }
            });
            });
            });
        });
        self.dlg.settings_page = page;
        self.win.settings = open;
        if reload_fgd {
            self.reload_fgds();
        }
        if reload {
            self.reload_content();
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.win.about {
            return;
        }
        let mut open = true;
        let th = crate::theme::t();
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Über Amboss"), 460.0), |ui| {
            ui.label(RichText::new(format!("Amboss {}", env!("CARGO_PKG_VERSION"))).font(egui::FontId::new(18.0, crate::theme::bold())));
            ui.label(RichText::new(crate::i18n::tr("Map-Editor für Source 1 und Garry's Mod")).color(th.text2));
            ui.label(RichText::new(crate::i18n::tr("© 2026 Deadshot · Open Source unter MIT oder Apache-2.0")).size(12.5).color(th.text2));
            ui.hyperlink_to("github.com/Deadshotlp/Amboss", env!("CARGO_PKG_REPOSITORY"));
            ui.add_space(8.0);
            crate::theme::section(ui, crate::i18n::tr("Verwendete Bibliotheken"));
            ui.label(RichText::new("egui / eframe (MIT/Apache-2.0) · glam (MIT/Apache-2.0) · serde (MIT/Apache-2.0) · mlua + Lua 5.4 (MIT) · rfd (MIT) · png (MIT/Apache-2.0) · rodio (MIT/Apache-2.0)").size(12.5).color(th.text2));
            ui.add_space(6.0);
            ui.label(RichText::new(crate::i18n::tr("Fehler gefunden? Beschreibe Schritte, Map und Fehlermeldung – die Einstellungen und Autosaves liegen im Einstellungsordner (Hilfe → Einstellungsordner öffnen).")).size(12.5).color(th.text3));
        });
        self.win.about = open;
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.win.help {
            return;
        }
        let mut open = true;
        let mut filter = std::mem::take(&mut self.help_filter);
        let mut settings = false;
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Bedienung & Tastenkürzel"), 760.0).height(620.0).footer(), |ui, part| match part {
            crate::theme::Part::Footer => {
                if crate::theme::secondary_button(ui, crate::i18n::tr("Tastenbelegung ändern…")).clicked() {
                    settings = true;
                }
            }
            crate::theme::Part::Header => {}
            crate::theme::Part::Body => {
                crate::theme::search_field(ui, &mut filter, crate::i18n::tr("Suchen…"), ui.available_width());
                ui.add_space(6.0);
                {
                    let rows = [
                        (crate::i18n::tr("Erste Schritte"), crate::i18n::tr("1. Block (Shift+B), in der Ansicht Oben ein Rechteck ziehen. 2. Strg+H höhlt zum Raum aus. 3. Entity (Shift+E): info_player_start und light setzen. 4. F9.")),
                        (crate::i18n::tr("Werkzeuge"), crate::i18n::tr("Shift+S Auswahl · G Lupe · C Kamera · E Entity · B Block · A Textur · D Decal · O Overlay · X Clip · V Vertex · P Displacement · K Cordon")),
                        (crate::i18n::tr("Auswahl"), crate::i18n::tr("Klick wählt, Strg = hinzufügen, Rahmen ziehen. Klick auf Auswahl: Skalieren → Drehen → Scheren")),
                        (crate::i18n::tr("Clip"), crate::i18n::tr("Linie ziehen, Enter schneidet, Shift+X wechselt Vorne/Hinten/Beide")),
                        (crate::i18n::tr("Vertex"), crate::i18n::tr("Eckpunkte/Kanten ziehen; ungültige Formen werden abgelehnt")),
                        (crate::i18n::tr("Displacement"), crate::i18n::tr("Fläche (Shift+A) wählen → Displacement erstellen, dann Shift+P und in 3D malen")),
                        ("2D", crate::i18n::tr("Mausrad zoomen, mittlere Taste/Leertaste verschieben, Pfeiltasten verschieben Auswahl")),
                        ("3D", crate::i18n::tr("Mittlere Maustaste (Mausrad gedrückt) + Ziehen umsehen, W A S D fliegen, Q/E runter/hoch, Shift beim Umsehen schneller – änderbar unter Einstellungen → Tastenkürzel")),
                        (crate::i18n::tr("Flächen"), crate::i18n::tr("Rechtsklick: Material anwenden · Alt+Rechtsklick: Ausrichtung nahtlos übernehmen")),
                        ("Strg+Z / Strg+Y", crate::i18n::tr("Rückgängig / Wiederholen")),
                        ("Strg+C / V / X", crate::i18n::tr("Kopieren / Einfügen / Ausschneiden · Strg+Shift+V Inhalte einfügen")),
                        ("Strg+G / U", crate::i18n::tr("Gruppieren / Gruppe lösen")),
                        ("H / Shift+H / U", crate::i18n::tr("Ausblenden / Unausgewähltes ausblenden / Alles einblenden")),
                        ("Strg+T / Strg+Shift+W", crate::i18n::tr("In Entity umwandeln / zurück in Welt")),
                        ("Strg+H / Strg+Shift+C", crate::i18n::tr("Aushöhlen / Carve")),
                        ("Strg+M / B", crate::i18n::tr("Transformieren-Dialog / Am Raster ausrichten")),
                        ("Strg+L / I / R", crate::i18n::tr("Spiegeln horizontal / vertikal, 90° drehen (Ansicht unter der Maus)")),
                        ("Shift+T / Strg+Shift+T", crate::i18n::tr("Aktuelle Textur anwenden / Textur-Browser")),
                        ("Alt+P", crate::i18n::tr("Auf Probleme prüfen (mit Auto-Fix)")),
                        ("Strg+Shift+F / G", crate::i18n::tr("Entity-Report / Gehe zu Nummer")),
                        ("Bild↑ / Bild↓", crate::i18n::tr("Kamera wechseln")),
                        ("[ ]", crate::i18n::tr("Raster verkleinern / vergrößern")),
                        ("F9", crate::i18n::tr("Kompilieren und in GMod starten")),
                        ("Strg+K", crate::i18n::tr("Befehlspalette: jeden Befehl per Suche ausführen")),
                        (crate::i18n::tr("Shift+Ziehen (2D) / Shift am Gizmo"), crate::i18n::tr("Auswahl duplizieren und verschieben · Strg+D dupliziert direkt")),
                        (crate::i18n::tr("3D-Gizmo"), crate::i18n::tr("Pfeile verschieben, Ringe drehen (15°-Raster, Shift frei) · Shift+W schaltet ihn ein/aus")),
                        (crate::i18n::tr("Flächen-Werkzeug in 3D"), crate::i18n::tr("Shift+Ziehen: Fläche verschieben (Brush wächst) · Alt+Ziehen: neuen Brush herausziehen")),
                        (crate::i18n::tr("Alt+Klick (3D)"), crate::i18n::tr("Pipette: Material einer Fläche übernehmen")),
                        ("L / Shift+L", crate::i18n::tr("Auswahl sperren / alles entsperren")),
                        ("Shift+M", crate::i18n::tr("Messwerkzeug (2D ziehen, 3D zwei Punkte)")),
                        ("Strg+Shift+1…9 / Strg+1…9", crate::i18n::tr("Kamera-Lesezeichen speichern / anspringen")),
                        ("Strg+Shift+O / Strg+Shift+H", crate::i18n::tr("Objektliste / Rückgängig-Verlauf")),
                        ("Strg+Tab / Strg+F4", crate::i18n::tr("Nächster Tab / Tab schließen")),
                        ("Alt+G / Alt+V", crate::i18n::tr("Am Raster einrasten an/aus · an Eckpunkten anderer Objekte einrasten an/aus")),
                        (crate::i18n::tr("3D: Ziehen mit links"), crate::i18n::tr("Rahmenauswahl (Strg = hinzufügen, Alt = auch berührte Objekte) · Strg+Alt+A: berührende auswählen")),
                        ("Strg+Alt+C / V", crate::i18n::tr("Entity-Eigenschaften kopieren / auf alle ausgewählten übertragen")),
                        (crate::i18n::tr("Werkzeuge → Versionen & Zusammenführen → Map zusammenführen"), crate::i18n::tr("Andere Map übernehmen: doppelte Namen werden umbenannt, Visgroup „Import“, Warnungen bei Überlappung")),
                        (crate::i18n::tr("Werkzeuge → Versionen & Zusammenführen → Vergleichen / 3-Wege"), crate::i18n::tr("Unterschiede zu einem anderen Stand farbig anzeigen · Änderungen zweier Personen zusammenführen")),
                        (crate::i18n::tr("Werkzeuge → Live-Sitzung"), crate::i18n::tr("Live-Sitzung: Gastgeber teilt die Map, Gäste treten mit Adresse und Passwort bei · Strg+Z nimmt nur eigene Schritte zurück")),
                        (crate::i18n::tr("Kompilieren → Automatische Optimierungen"), crate::i18n::tr("KI-Nodes, Leckprüfung, func_detail, nodraw, Areaportals, Cubemaps, Lightmaps… · „Nur prüfen“ zeigt das NPC-Netz (grün) und Probleme, „Auf Map anwenden“ übernimmt (Strg+Z), oder beim Kompilieren automatisch auf einer Kopie")),
                    ];
                    let th = crate::theme::t();
                    let f = filter.to_lowercase();
                    let key_w = 230.0;
                    for (i, (k, d)) in rows.iter().enumerate() {
                        let k = crate::i18n::keys(&crate::i18n::tr_dyn(k));
                        if !f.is_empty() && !k.to_lowercase().contains(&f) && !d.to_lowercase().contains(&f) {
                            continue;
                        }
                        // feste Tastenspalte, Beschreibung bricht um
                        let w = ui.available_width();
                        let desc = ui.painter().layout(d.to_string(), crate::theme::sans(13.0), th.text, w - key_w - 12.0);
                        let keys_g = ui.painter().layout(k.clone(), crate::theme::mono(12.0), th.text, key_w - 12.0);
                        let h = desc.size().y.max(keys_g.size().y) + 12.0;
                        let (r, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
                        if i % 2 == 1 {
                            ui.painter().rect_filled(r, 4.0, th.raised);
                        }
                        ui.painter().galley(r.min + Vec2::new(6.0, 6.0), keys_g, th.text);
                        ui.painter().galley(r.min + Vec2::new(key_w, 6.0), desc, th.text);
                    }
                }
            }
        });
        self.help_filter = filter;
        if settings {
            self.win.settings = true;
            open = false;
        }
        self.win.help = open;
    }

    fn texture_browser(&mut self, ctx: &egui::Context) {
        if !self.win.browser {
            return;
        }
        let mut open = true;
        let mut apply = false;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Textur-Browser"), 860.0).height(620.0).no_scroll(), |ui| {
            if !self.mats.has_fs() {
                ui.colored_label(Color32::YELLOW, crate::i18n::tr("Kein Spielordner gesetzt – unter Extras / Einstellungen „GMod automatisch finden“ wählen."));
                return;
            }
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Filter:"));
                ui.add(egui::TextEdit::singleline(&mut self.browser_filter).hint_text(crate::theme::hint(crate::i18n::tr("z. B. concrete, brick, dev/"))).desired_width(220.0));
                for quick in ["dev/", "tools/", "concrete", "brick", "wood", "metal", "nature", "decals/", "gm_construct"] {
                    if ui.small_button(quick).clicked() {
                        self.browser_filter = quick.to_string();
                    }
                }
                if ui.small_button(crate::i18n::tr("In Map")).on_hover_text(crate::i18n::tr("Nur in der Map verwendete Materialien")).clicked() {
                    self.browser_filter = "@map".into();
                }
                if ui.small_button(crate::i18n::tr("★ Favoriten")).clicked() {
                    self.browser_filter = "@fav".into();
                }
                if ui.small_button(crate::i18n::tr("Zuletzt")).clicked() {
                    self.browser_filter = "@recent".into();
                }
            });
            ui.label(RichText::new(crate::i18n::tr("Rechtsklick auf eine Textur: Favorit an/aus")).small());
            let filter = self.browser_filter.to_ascii_lowercase();
            let items: Vec<String> = if filter == "@fav" {
                self.settings.favorite_materials.iter().map(|m| crate::materials::normalize(m)).collect()
            } else if filter == "@recent" {
                self.settings.recent_materials.iter().map(|m| crate::materials::normalize(m)).collect()
            } else if filter == "@map" {
                let mut v: Vec<String> = self.ed.map.all_solids().flat_map(|(_, s)| s.sides.iter().map(|x| crate::materials::normalize(&x.material))).collect();
                v.sort();
                v.dedup();
                v
            } else {
                let words: Vec<&str> = filter.split_whitespace().collect();
                self.mats
                    .list
                    .iter()
                    .filter(|m| !m.starts_with("models/") && !m.starts_with("vgui/") && !m.starts_with("particle") && !m.starts_with("console/"))
                    .filter(|m| words.iter().all(|w| m.contains(w)))
                    .cloned()
                    .collect()
            };
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr_dyn(&format!("{} Materialien", items.len())));
                if self.mats.is_loading() {
                    ui.spinner();
                }
                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Aktuell: {}", self.ed.material.to_lowercase()))).monospace());
                if ui.button(crate::i18n::tr("Auf Auswahl anwenden")).clicked() {
                    apply = true;
                }
            });
            ui.separator();
            let cell = Vec2::new(112.0, 128.0);
            let cols = ((ui.available_width() / cell.x).floor() as usize).max(1);
            let rows = items.len().div_ceil(cols);
            let current = crate::materials::normalize(&self.ed.material);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, cell.y, rows, |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        for col in 0..cols {
                            let Some(name) = items.get(row * cols + col) else { break };
                            let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                            if *name == current {
                                ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_rgb(40, 70, 120));
                            } else if resp.hovered() {
                                ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_gray(45));
                            }
                            let img_rect = egui::Rect::from_min_size(rect.min + Vec2::new(8.0, 4.0), Vec2::splat(96.0));
                            match self.mats.thumb(name) {
                                Some(t) => {
                                    let s = t.size_vec2();
                                    let k = 96.0 / s.x.max(s.y);
                                    egui::Image::new((t.id(), s * k)).paint_at(ui, egui::Rect::from_center_size(img_rect.center(), s * k));
                                }
                                None => {
                                    let c = if self.mats.is_missing(name) { Color32::from_rgb(60, 20, 20) } else { Color32::from_gray(30) };
                                    ui.painter().rect_filled(img_rect, egui::CornerRadius::ZERO, c);
                                }
                            }
                            let short = name.rsplit('/').next().unwrap_or(name);
                            let short = if short.chars().count() > 16 { format!("{}…", short.chars().take(15).collect::<String>()) } else { short.to_string() };
                            ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 13.0), Align2::CENTER_CENTER, short, FontId::proportional(11.0), Color32::from_gray(210));
                            let fav = self.settings.favorite_materials.iter().any(|f| f.eq_ignore_ascii_case(name));
                            if fav {
                                ui.painter().text(rect.right_top() + Vec2::new(-6.0, 4.0), Align2::RIGHT_TOP, "★", FontId::proportional(14.0), Color32::GOLD);
                            }
                            let resp = resp.on_hover_text(name);
                            if resp.secondary_clicked() {
                                if fav {
                                    self.settings.favorite_materials.retain(|f| !f.eq_ignore_ascii_case(name));
                                } else {
                                    self.settings.favorite_materials.push(name.to_uppercase());
                                }
                            }
                            if resp.clicked() {
                                self.ed.material = name.to_uppercase();
                            }
                            if resp.double_clicked() {
                                self.ed.material = name.to_uppercase();
                                apply = true;
                            }
                        }
                    });
                }
            });
        });
        if apply {
            self.ed.apply_material();
        }
        self.win.browser = open;
    }

    fn transform_window(&mut self, ctx: &egui::Context) {
        if !self.win.transform {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let has_sel = !self.ed.sel.is_empty();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Transformieren"), 400.0).footer().no_scroll(), |ui, part| match part {
            crate::theme::Part::Body => {
                let n = crate::theme::segmented(ui, self.dlg.transform_mode, &[crate::i18n::tr("Verschieben"), crate::i18n::tr("Drehen"), crate::i18n::tr("Skalieren")], true);
                if n != self.dlg.transform_mode {
                    self.dlg.transform_mode = n;
                    self.dlg.transform_val = if n == 2 { [1.0; 3] } else { [0.0; 3] };
                }
                ui.add_space(6.0);
                crate::theme::section(ui, match self.dlg.transform_mode { 0 => crate::i18n::tr("Versatz (Einheiten)"), 1 => crate::i18n::tr("Winkel (Grad)"), _ => crate::i18n::tr("Faktor") });
                let ax = crate::theme::t().axis_colors();
                ui.horizontal(|ui| {
                    for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
                        ui.label(RichText::new(*l).font(crate::theme::mono(11.0)).color(ax[k]));
                        ui.add_sized([96.0, 30.0], crate::theme::dv(egui::DragValue::new(&mut self.dlg.transform_val[k])).speed(if self.dlg.transform_mode == 2 { 0.01 } else { 1.0 }));
                    }
                });
            }
            crate::theme::Part::Footer => {
                if ui.add_enabled_ui(has_sel, |ui| crate::theme::primary_button(ui, crate::i18n::tr("Anwenden"))).inner.clicked() {
                    apply = true;
                }
                crate::theme::footer_hint(ui, if has_sel { crate::i18n::tr("Drehen und Skalieren um die Mitte der Auswahl") } else { crate::i18n::tr("Zuerst etwas auswählen") });
            }
            _ => {}
        });
        if apply {
            let v = DVec3::from(self.dlg.transform_val);
            let c = self.ed.selection_bounds().center();
            let x = match self.dlg.transform_mode {
                0 => Xform::translate(v),
                1 => Xform::about(c, Xform::rotation(2, v.z) * Xform::rotation(1, v.y) * Xform::rotation(0, v.x)),
                _ => Xform::about(c, DMat3::from_diagonal(v)),
            };
            self.ed.transform_selection(&x, self.dlg.transform_mode == 2);
        }
        self.win.transform = open;
    }

    fn paste_special_window(&mut self, ctx: &egui::Context) {
        if !self.win.paste_special {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let cb = self.ed.clipboard_bounds();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Inhalte einfügen"), 640.0).footer().no_scroll(), |ui, part| match part {
            crate::theme::Part::Body => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    ui.vertical(|ui| {
                        ui.set_width(340.0);
                        crate::theme::field_row(ui, crate::i18n::tr("Kopien"), |ui| {
                            if ui.add_sized([30.0, 30.0], egui::Button::new("−")).clicked() && self.dlg.ps_copies > 1 {
                                self.dlg.ps_copies -= 1;
                            }
                            ui.add_sized([46.0, 30.0], crate::theme::dv(egui::DragValue::new(&mut self.dlg.ps_copies)).range(1..=256));
                            if ui.add_sized([30.0, 30.0], egui::Button::new("+")).clicked() && self.dlg.ps_copies < 256 {
                                self.dlg.ps_copies += 1;
                            }
                        });
                        let ax = crate::theme::t().axis_colors();
                        for (label, vals) in [(crate::i18n::tr("Versatz je Kopie"), &mut self.dlg.ps_offset), (crate::i18n::tr("Drehung je Kopie (°)"), &mut self.dlg.ps_rot)] {
                            crate::theme::field_row(ui, label, |ui| {
                                for k in 0..3 {
                                    ui.label(RichText::new(["X", "Y", "Z"][k]).font(egui::FontId::new(10.5, crate::theme::semibold())).color(ax[k]));
                                    ui.add_sized([44.0, 30.0], crate::theme::dv(egui::DragValue::new(&mut vals[k])).speed(1.0));
                                }
                            });
                        }
                        ui.add_space(4.0);
                        crate::theme::checkbox(ui, &mut self.dlg.ps_group, crate::i18n::tr("Jede Kopie als eigene Gruppe"));
                    });
                    ui.vertical(|ui| {
                        paste_preview(ui, cb, self.dlg.ps_copies, DVec3::from(self.dlg.ps_offset), DVec3::from(self.dlg.ps_rot));
                    });
                });
            }
            crate::theme::Part::Footer => {
                let n = self.dlg.ps_copies;
                if ui.add_enabled(cb.is_some(), egui::Button::new(RichText::new(crate::i18n::tr_dyn(&format!("{n} {} einfügen", if n == 1 { "Kopie" } else { crate::i18n::tr("Kopien") }))).family(crate::theme::semibold()).color(crate::theme::t().accent_fg)).fill(crate::theme::t().accent).corner_radius(8).min_size(egui::Vec2::new(0.0, 34.0))).clicked() {
                    apply = true;
                }
                if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    cancel = true;
                }
                crate::theme::footer_hint(ui, if cb.is_some() { crate::i18n::tr("Fügt die Zwischenablage (Strg+C) mehrfach ein") } else { crate::i18n::tr("Zwischenablage ist leer – erst etwas kopieren (Strg+C)") });
            }
            _ => {}
        });
        if apply {
            self.ed.paste_special(self.dlg.ps_copies, DVec3::from(self.dlg.ps_offset), DVec3::from(self.dlg.ps_rot), self.dlg.ps_group);
            open = false;
        }
        if cancel {
            open = false;
        }
        self.win.paste_special = open;
    }

    fn replace_window(&mut self, ctx: &egui::Context) {
        if !self.win.replace {
            return;
        }
        let mut open = true;
        let mut run = false;
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Texturen ersetzen"), 480.0).footer().no_scroll(), |ui, part| match part {
            crate::theme::Part::Body => {
                crate::theme::field_row(ui, crate::i18n::tr("Suchen"), |ui| ui.add(egui::TextEdit::singleline(&mut self.dlg.rep_find).font(crate::theme::mono(12.0)).desired_width(300.0)));
                crate::theme::field_row(ui, crate::i18n::tr("Ersetzen durch"), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.dlg.rep_with).font(crate::theme::mono(12.0)).desired_width(230.0));
                    if ui.button(crate::i18n::tr("Aktuell")).on_hover_text(crate::i18n::tr("Aktuelles Material einsetzen")).clicked() {
                        self.dlg.rep_with = self.ed.material.clone();
                    }
                });
                ui.add_space(4.0);
                crate::theme::checkbox(ui, &mut self.dlg.rep_sel, crate::i18n::tr("Nur in der Auswahl"));
                crate::theme::checkbox(ui, &mut self.dlg.rep_partial, crate::i18n::tr("Teiltreffer ersetzen (z. B. nur Ordnername)"));
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, crate::i18n::tr("Ersetzen")).clicked() {
                    run = true;
                }
                crate::theme::footer_hint(ui, crate::i18n::tr("Ein Rückgängig-Schritt"));
            }
            _ => {}
        });
        if run {
            let (f, w) = (self.dlg.rep_find.clone(), self.dlg.rep_with.clone());
            self.ed.replace_materials(&f, &w, self.dlg.rep_sel, self.dlg.rep_partial);
        }
        self.win.replace = open;
    }

    fn problems_window(&mut self, ctx: &egui::Context) {
        if !self.win.problems {
            return;
        }
        let mut open = true;
        let mut fix: Option<usize> = None;
        let mut fix_all = false;
        let mut select: Option<usize> = None;
        let mut recheck = false;
        let th = crate::theme::t();
        let count = |sev: Severity| self.problems.iter().filter(|p| p.severity == sev).count();
        let (ne, nw, ni) = (count(Severity::Error), count(Severity::Warning), count(Severity::Info));
        let fixable = self.problems.iter().filter(|p| p.fix.is_some()).count();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Probleme"), 600.0).footer().header_extra().height(480.0), |ui, part| match part {
            crate::theme::Part::Header => {
                for (kind, label) in [(2u8, crate::i18n::count(ni, "Hinweis", "Hinweise")), (1, crate::i18n::count(nw, "Warnung", "Warnungen")), (0, crate::i18n::count(ne, "Fehler", "Fehler"))] {
                    egui::Frame::new().fill(th.raised).corner_radius(11).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            crate::theme::severity_mark(ui, kind);
                            ui.label(RichText::new(label).size(11.5).color(th.text2));
                        });
                    });
                }
            }
            crate::theme::Part::Body => {
                if self.problems.is_empty() {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 3);
                        ui.label(crate::i18n::tr("Keine Probleme gefunden."));
                    });
                }
                for (i, p) in self.problems.iter().enumerate() {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, match p.severity {
                            Severity::Error => 0,
                            Severity::Warning => 1,
                            Severity::Info => 2,
                        });
                        ui.add(egui::Label::new(crate::i18n::tr_dyn(&p.text)).wrap());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(f) = &p.fix {
                                if ui.button(f.label()).clicked() {
                                    fix = Some(i);
                                }
                            }
                            if !p.objs.is_empty() && ui.add(egui::Button::new(RichText::new(crate::i18n::tr("Auswählen")).color(th.text2)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() {
                                select = Some(i);
                            }
                        });
                    });
                    let r = ui.max_rect();
                    ui.painter().hline(r.x_range(), ui.cursor().top() - 2.0, Stroke::new(1.0f32, th.line));
                }
            }
            crate::theme::Part::Footer => {
                if fixable > 0 && crate::theme::primary_button(ui, &format!("Alle beheben ({fixable})")).clicked() {
                    fix_all = true;
                }
                if crate::theme::secondary_button(ui, crate::i18n::tr("Erneut prüfen")).clicked() {
                    recheck = true;
                }
            }
        });
        if let Some(i) = select {
            let objs = self.problems[i].objs.clone();
            self.ed.sel = objs.into_iter().collect();
            self.ed.touch();
            self.center_on(self.ed.selection_bounds());
        }
        if fix_all || fix.is_some() {
            self.ed.checkpoint();
            let list: Vec<crate::problems::Fix> = if fix_all { self.problems.iter().filter_map(|p| p.fix.clone()).collect() } else { self.problems[fix.unwrap()].fix.clone().into_iter().collect() };
            for f in &list {
                crate::problems::apply(&mut self.ed, f);
            }
            recheck = true;
        }
        if recheck {
            self.problems = crate::problems::check(&self.ed, self.known_materials().as_ref());
        }
        self.win.problems = open;
    }

    fn entity_report_window(&mut self, ctx: &egui::Context) {
        if !self.win.entity_report {
            return;
        }
        let mut open = true;
        let mut action: Option<(u32, u8)> = None;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Entity-Report"), 640.0).height(520.0).no_scroll(), |ui| {
            crate::theme::search_field(ui, &mut self.dlg.report_filter, crate::i18n::tr("Klasse, Name oder Schlüsselwert"), ui.available_width());
            let f = self.dlg.report_filter.to_ascii_lowercase();
            let list: Vec<(u32, String, String, String)> = self
                .ed
                .map
                .entities
                .iter()
                .filter(|e| f.is_empty() || e.props.iter().any(|(_, v)| v.to_ascii_lowercase().contains(&f)))
                .map(|e| {
                    // Brush-Entities: Mittelpunkt der Box
                    let pos = e.origin().or_else(|| {
                        let mut b = crate::geometry::Aabb::EMPTY;
                        for s in &e.solids {
                            b = b.union(&s.bounds);
                        }
                        (!b.is_empty()).then(|| b.center())
                    });
                    (e.id, e.classname().to_string(), e.targetname().unwrap_or("").to_string(), pos.map(crate::map::fmt_vec).unwrap_or_default())
                })
                .collect();
            ui.label(crate::i18n::count(list.len(), "Entity", "Entities"));
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("report").num_columns(5).striped(true).min_col_width(60.0).show(ui, |ui| {
                    let th = crate::theme::t();
                    for h in [crate::i18n::tr("Klasse"), crate::i18n::tr("Name"), crate::i18n::tr("Position"), "", ""] {
                        ui.label(RichText::new(h.to_uppercase()).size(11.0).color(th.text3));
                    }
                    ui.end_row();
                    for (id, class, name, origin) in &list {
                        let selected = self.ed.sel.contains(&Obj::Entity(*id));
                        if ui.selectable_label(selected, class).clicked() {
                            action = Some((*id, 0));
                        }
                        ui.label(name);
                        ui.label(RichText::new(origin).small());
                        if ui.small_button(crate::i18n::tr("Gehe zu")).clicked() {
                            action = Some((*id, 1));
                        }
                        let (tr_rect, tresp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                        if tresp.hovered() {
                            ui.painter().rect_filled(tr_rect, 5.0, th.hover);
                        }
                        crate::icons::draw(ui.painter(), tr_rect.shrink(4.0), "trash", if tresp.hovered() { th.err } else { th.text3 }, 1.6);
                        if tresp.on_hover_text(crate::i18n::keys(crate::i18n::tr("Entity löschen (Strg+Z macht es rückgängig)"))).clicked() {
                            action = Some((*id, 2));
                        }
                        ui.end_row();
                    }
                });
            });
        });
        if let Some((id, a)) = action {
            self.ed.sel.clear();
            self.ed.sel.insert(Obj::Entity(id));
            self.ed.touch();
            match a {
                1 => self.center_on(self.ed.selection_bounds()),
                2 => self.ed.delete_selection(),
                _ => {}
            }
        }
        self.win.entity_report = open;
    }

    fn map_info_window(&mut self, ctx: &egui::Context) {
        if !self.win.map_info {
            return;
        }
        let mut open = true;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Map-Info"), 440.0), |ui| {
            let m = &self.ed.map;
            let solids = m.all_solids().count();
            let faces: usize = m.all_solids().map(|(_, s)| s.sides.len()).sum();
            let disps: usize = m.all_solids().map(|(_, s)| s.sides.iter().filter(|x| x.disp.is_some()).count()).sum();
            let mut mats: Vec<String> = m.all_solids().flat_map(|(_, s)| s.sides.iter().map(|x| x.material.to_ascii_lowercase())).collect();
            mats.sort();
            mats.dedup();
            let point = m.entities.iter().filter(|e| !e.is_brush_entity()).count();
            let bounds = m.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
            egui::Grid::new("info").num_columns(2).striped(true).show(ui, |ui| {
                for (k, v) in [
                    (crate::i18n::tr("Datei"), self.ed.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| crate::i18n::tr("(nicht gespeichert)").into())),
                    (crate::i18n::tr("Brushes"), solids.to_string()),
                    (crate::i18n::tr("Flächen"), faces.to_string()),
                    (crate::i18n::tr("Displacements"), disps.to_string()),
                    (crate::i18n::tr("Punkt-Entities"), point.to_string()),
                    (crate::i18n::tr("Brush-Entities"), (m.entities.len() - point).to_string()),
                    (crate::i18n::tr("Verschiedene Texturen"), mats.len().to_string()),
                    (crate::i18n::tr("Gruppen / Visgroups"), format!("{} / {}", m.groups.len(), m.visgroups.len())),
                    (crate::i18n::tr("Rückgängig-Speicher"), crate::i18n::tr_dyn(&format!("{} gespeicherte Brushes", self.ed.undo_stored_solids()))),
                    (crate::i18n::tr("Ausdehnung"), if bounds.is_empty() { "–".into() } else { format!("{} × {} × {}", bounds.size().x, bounds.size().y, bounds.size().z) }),
                ] {
                    ui.label(RichText::new(k).strong());
                    ui.label(v);
                    ui.end_row();
                }
            });
            ui.collapsing(crate::i18n::tr("Verwendete Texturen"), |ui| {
                for t in &mats {
                    ui.label(RichText::new(t).monospace().small());
                }
            });
            let mut classes: Vec<(String, usize)> = Vec::new();
            for e in &m.entities {
                match classes.iter_mut().find(|c| c.0 == e.classname()) {
                    Some(c) => c.1 += 1,
                    None => classes.push((e.classname().to_string(), 1)),
                }
            }
            classes.sort_by(|a, b| b.1.cmp(&a.1));
            ui.collapsing("Entities nach Klasse", |ui| {
                for (c, n) in classes {
                    ui.label(crate::i18n::tr_dyn(&format!("{n} × {c}")));
                }
            });
        });
        self.win.map_info = open;
    }

    fn goto_window(&mut self, ctx: &egui::Context) {
        if !self.win.goto {
            return;
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut go = false;
        let mut jump = false;
        let bad = !self.dlg.goto_text.trim().is_empty() && self.dlg.goto_text.trim().parse::<u32>().is_err();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Gehe zu Nummer"), 420.0).footer().no_scroll(), |ui, part| match part {
            crate::theme::Part::Body => {
                crate::theme::section(ui, crate::i18n::tr("Brush-, Seiten- oder Entity-ID"));
                let r = ui.add(egui::TextEdit::singleline(&mut self.dlg.goto_text).font(crate::theme::mono(13.0)).hint_text(crate::theme::hint(crate::i18n::tr("z. B. 2207 aus einer VBSP-Meldung"))).desired_width(f32::INFINITY));
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                if bad {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 0);
                        ui.label(RichText::new(crate::i18n::tr("Nur Ziffern erlaubt")).size(12.0).color(th.err));
                    });
                }
                ui.add_space(8.0);
                crate::theme::section(ui, crate::i18n::tr("Oder Koordinate (x y z)"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.dlg.goto_coord).font(crate::theme::mono(12.0)).desired_width(250.0).hint_text(crate::theme::hint("512 -256 64")));
                    if ui.button(crate::i18n::tr("Hinspringen")).clicked() {
                        jump = true;
                    }
                });
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, crate::i18n::tr("Gehe zu")).clicked() {
                    go = true;
                }
                if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    self.win.goto = false;
                }
            }
            _ => {}
        });
        if jump {
            if let Some(p) = crate::map::parse_vec(&self.dlg.goto_coord) {
                self.ed.work_point = p;
                for v in &mut self.views2d {
                    v.center_on(p);
                }
                let f = self.view3d.cam.forward();
                self.view3d.cam.pos = p.as_vec3() - f * 256.0;
                self.ed.status = format!("Koordinate {}", crate::map::fmt_vec(p));
                self.ed.touch();
            } else {
                self.ed.status = crate::i18n::tr("Ungültige Koordinate").into();
            }
        }
        if go {
            if let Ok(id) = self.dlg.goto_text.trim().parse() {
                self.goto_id(id);
                open = false;
            }
        }
        if !self.win.goto {
            open = false;
        }
        self.win.goto = open;
    }

    fn sound_browser(&mut self, ctx: &egui::Context) {
        if !self.win.sounds {
            self.player.stop();
            return;
        }
        use egui::{Rect, Sense, Stroke, StrokeKind, Vec2};
        let th = crate::theme::t();
        self.player.poll();
        if self.player.playing.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        let mut open = true;
        let mut chosen: Option<String> = None;
        let mut toggle: Option<String> = None;
        let f = self.dlg.sound_filter.to_ascii_lowercase();
        let items: Vec<String> = self.dlg.sound_list.iter().filter(|s| s.contains(&f)).cloned().collect();
        let n = items.len();
        self.dlg.sound_active = self.dlg.sound_active.min(n.saturating_sub(1));
        // Tastatur: ↑↓ wählen, Leertaste spielt/stoppt, Enter übernimmt
        // nicht innerhalb von input_mut abfragen (Kontext ist dort gesperrt)
        let typing = ctx.wants_keyboard_input();
        let (up, down, space, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                !typing && i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        let mut scroll = false;
        if up && self.dlg.sound_active > 0 {
            self.dlg.sound_active -= 1;
            scroll = true;
        }
        if down && self.dlg.sound_active + 1 < n {
            self.dlg.sound_active += 1;
            scroll = true;
        }
        if space {
            toggle = items.get(self.dlg.sound_active).cloned();
        }
        if enter {
            chosen = items.get(self.dlg.sound_active).cloned();
        }
        let fs = self.mats.fs();
        let d = crate::theme::Dlg::new(crate::i18n::tr("Sound-Browser"), 560.0).subtitle(crate::i18n::tr_dyn(&format!("{n} Sounds"))).height(500.0).footer().no_scroll();
        let mut apply = false;
        let mut cancel = false;
        let mut vol = self.player.volume;
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 0 }).show(ui, |ui| {
                    let te = ui.add(egui::TextEdit::singleline(&mut self.dlg.sound_filter).hint_text(crate::theme::hint(crate::i18n::tr("Sounds filtern (z. B. ambient/, door)"))).desired_width(f32::INFINITY));
                    if te.changed() {
                        self.dlg.sound_active = 0;
                    }
                    if !crate::audio::Player::available() {
                        ui.label(RichText::new(crate::i18n::tr("Abspielen ist in dieser Version nur unter Windows verfügbar.")).small().color(th.text3));
                    }
                    ui.add_space(6.0);
                    let row_h = 44.0;
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, n, |ui, range| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let w = ui.available_width();
                        for i in range {
                            let s = &items[i];
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
                            if i == self.dlg.sound_active && scroll {
                                ui.scroll_to_rect(r, None);
                            }
                            let playing = self.player.playing.as_deref() == Some(s.as_str());
                            let missing = self.dlg.sound_missing.contains(s);
                            let active = i == self.dlg.sound_active;
                            if playing {
                                ui.painter().rect(r, 7.0, th.accent_soft, if active { Stroke::new(1.0f32, th.accent) } else { Stroke::NONE }, StrokeKind::Inside);
                            } else if active {
                                ui.painter().rect(r, 7.0, th.hover, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, 7.0, th.hover);
                            }
                            // Rund-Knopf
                            let bc = egui::pos2(r.left() + 20.0, r.center().y);
                            let br = Rect::from_center_size(bc, Vec2::splat(28.0));
                            let bresp = ui.interact(br, ui.id().with(("play", i)), Sense::click());
                            let fade = |c: Color32| if missing { crate::theme::alpha(c, 0.5) } else { c };
                            if playing {
                                ui.painter().circle_filled(bc, 14.0, th.accent);
                                ui.painter().rect_filled(Rect::from_center_size(bc, Vec2::splat(10.0)), 1.5, th.accent_fg);
                            } else {
                                ui.painter().circle_filled(bc, 14.0, fade(if bresp.hovered() { th.hover } else { th.raised }));
                                ui.painter().add(egui::Shape::convex_polygon(vec![bc + Vec2::new(-3.0, -5.0), bc + Vec2::new(5.0, 0.0), bc + Vec2::new(-3.0, 5.0)], fade(th.text2), Stroke::NONE));
                            }
                            if bresp.clicked() && !missing {
                                toggle = Some(s.clone());
                            }
                            // Dauer (aus dem WAV-Kopf, einmal gelesen)
                            if !self.dlg.sound_durations.contains_key(s) && !missing {
                                let dur = fs.as_ref().and_then(|fs| fs.read(s)).map(|b| crate::audio::wav_duration(&b));
                                match dur {
                                    Some(d) => {
                                        self.dlg.sound_durations.insert(s.clone(), d);
                                    }
                                    None => {
                                        self.dlg.sound_missing.insert(s.clone());
                                    }
                                }
                            }
                            let dur = self.dlg.sound_durations.get(s).copied().flatten();
                            let fmt = |v: f32| crate::i18n::dec(format!("{:.1}", v));
                            let dtext = match (playing, self.player.progress()) {
                                (true, Some((t, total))) => format!("{} / {} s", fmt(t), total.or(dur).map(fmt).unwrap_or_else(|| "?".into())),
                                _ => dur.map(|d| format!("{} s", fmt(d))).unwrap_or_else(|| "–".into()),
                            };
                            let dg = ui.painter().layout_no_wrap(dtext, crate::theme::mono(11.5), if playing { th.accent_text } else { th.text3 });
                            let dw = dg.size().x;
                            ui.painter().galley(egui::pos2(r.right() - 10.0 - dw, r.center().y - dg.size().y / 2.0), dg, th.text3);
                            // Pfad
                            let tx = r.left() + 44.0;
                            let maxw = r.right() - 20.0 - dw - tx;
                            let name = s.trim_start_matches("sound/").to_string();
                            let mut job = egui::text::LayoutJob::simple_singleline(name, crate::theme::mono(12.0), if missing { th.text3 } else { th.text });
                            job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                            let g = ui.painter().layout_job(job);
                            let two = playing || missing;
                            ui.painter().galley(egui::pos2(tx, if two { r.center().y - 13.0 } else { r.center().y - g.size().y / 2.0 }), g, th.text);
                            if missing {
                                ui.painter().text(egui::pos2(tx, r.center().y + 8.0), egui::Align2::LEFT_CENTER, crate::i18n::tr("Datei fehlt im Spielordner"), crate::theme::sans(11.5), th.err);
                            } else if playing {
                                let bar = Rect::from_min_size(egui::pos2(tx, r.center().y + 7.0), Vec2::new(maxw, 4.0));
                                ui.painter().rect_filled(bar, 2.0, th.line);
                                if let Some((t, total)) = self.player.progress() {
                                    let frac = total.or(dur).map(|d| (t / d.max(0.01)).clamp(0.0, 1.0)).unwrap_or(0.0);
                                    ui.painter().rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac, 4.0)), 2.0, th.accent);
                                }
                            }
                            if resp.clicked() && !bresp.clicked() {
                                self.dlg.sound_active = i;
                            }
                            if resp.double_clicked() {
                                chosen = Some(s.clone());
                            }
                        }
                    });
                });
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, crate::i18n::tr("Übernehmen")).clicked() {
                    apply = true;
                }
                if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    cancel = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Lautsprecher + Regler
                    let (ir, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    let c = ir.center();
                    ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(-6.0, -2.5), c + Vec2::new(-3.0, -2.5), c + Vec2::new(1.0, -6.0), c + Vec2::new(1.0, 6.0), c + Vec2::new(-3.0, 2.5), c + Vec2::new(-6.0, 2.5)], th.text2, Stroke::NONE));
                    ui.painter().add(egui::Shape::line((0..=8).map(|k| { let a = -0.8 + 1.6 * k as f32 / 8.0; c + Vec2::new(3.0 + 3.5 * a.cos(), 3.5 * a.sin()) }).collect(), Stroke::new(1.2f32, th.text2)));
                    ui.add_space(6.0);
                    let (sr, sresp) = ui.allocate_exact_size(Vec2::new(120.0, 18.0), Sense::click_and_drag());
                    if let Some(p) = sresp.interact_pointer_pos() {
                        vol = ((p.x - sr.left()) / sr.width()).clamp(0.0, 1.0);
                    }
                    let track = Rect::from_center_size(sr.center(), Vec2::new(sr.width(), 4.0));
                    ui.painter().rect_filled(track, 2.0, th.line_strong);
                    ui.painter().rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * vol, 4.0)), 2.0, th.accent);
                    ui.painter().circle(egui::pos2(track.left() + track.width() * vol, track.center().y), 7.0, Color32::WHITE, Stroke::new(1.0f32, crate::theme::alpha(Color32::BLACK, 0.15)));
                    ui.add_space(8.0);
                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} %", (vol * 100.0).round()))).font(crate::theme::mono(11.5)).color(th.text2));
                });
            }
            _ => {}
        });
        if (vol - self.player.volume).abs() > 1e-4 {
            self.player.set_volume(vol);
        }
        if let Some(s) = toggle {
            if self.player.playing.as_deref() == Some(s.as_str()) {
                self.player.stop();
            } else if let Some(bytes) = fs.as_ref().and_then(|fs| fs.read(&s)) {
                if let Err(e) = self.player.play(&s, bytes) {
                    self.ed.status = e;
                }
            } else {
                self.dlg.sound_missing.insert(s);
            }
        }
        if apply {
            chosen = chosen.or_else(|| items.get(self.dlg.sound_active).cloned());
        }
        if let Some(s) = chosen {
            let s = s.trim_start_matches("sound/").to_string();
            if let Some((id, key)) = self.dlg.browse_target.clone() {
                self.ed.checkpoint();
                if let Some(e) = self.ed.map.entity_mut(id) {
                    e.set(&key, s.clone());
                }
                self.ed.touch();
            }
            self.ed.status = format!("Sound gewählt: {s}");
            open = false;
        }
        if cancel {
            open = false;
        }
        if !open {
            self.player.stop();
        }
        self.win.sounds = open;
    }

    /// Logische Ansicht: Entities als Knoten, Outputs als Pfeile
    fn logical_view(&mut self, ctx: &egui::Context) {
        if !self.win.logical {
            return;
        }
        let mut open = true;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Logische Ansicht (Entity-I/O)"), 900.0).height(620.0).no_scroll(), |ui| {
            ui.label(RichText::new(crate::i18n::tr("Knoten ziehen zum Anordnen · Klick wählt · Mausrad zoomt · Rechte Taste verschiebt")).small());
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = resp.rect;
            painter.rect_filled(rect, egui::CornerRadius::ZERO, Color32::from_gray(20));
            let zoom = self.dlg.logical_zoom;
            if resp.hovered() {
                let s = ui.input(|i| i.raw_scroll_delta.y);
                if s != 0.0 {
                    self.dlg.logical_zoom = (zoom * (s * 0.003).exp()).clamp(0.2, 4.0);
                }
            }
            if resp.dragged_by(egui::PointerButton::Secondary) {
                self.dlg.logical_pan += resp.drag_delta();
            }
            // Knoten: Entities mit Namen oder Outputs
            let ents: Vec<(u32, String, String, [f64; 2])> = self
                .ed
                .map
                .entities
                .iter()
                .filter(|e| e.targetname().is_some() || !e.outputs.is_empty())
                .enumerate()
                .map(|(i, e)| {
                    let pos = e.editor.logicalpos.unwrap_or([(i % 6) as f64 * 220.0, (i / 6) as f64 * 110.0]);
                    (e.id, e.classname().to_string(), e.targetname().unwrap_or("").to_string(), pos)
                })
                .collect();
            let to_screen = |p: [f64; 2]| rect.min + self.dlg.logical_pan + Vec2::new(p[0] as f32, p[1] as f32) * zoom + Vec2::splat(20.0);
            let node_size = Vec2::new(170.0, 44.0) * zoom;
            let node_rect = |p: [f64; 2]| egui::Rect::from_min_size(to_screen(p), node_size);
            // Kanten
            for e in &self.ed.map.entities {
                let Some(src) = ents.iter().find(|n| n.0 == e.id) else { continue };
                for (o, v) in &e.outputs {
                    let parts: Vec<&str> = v.split(['\u{1b}', ',']).collect();
                    let t = parts.first().copied().unwrap_or("");
                    for dst in ents.iter().filter(|n| n.2.eq_ignore_ascii_case(t)) {
                        let a = node_rect(src.3).right_center();
                        let b = node_rect(dst.3).left_center();
                        painter.arrow(a, b - a, Stroke::new(1.5f32, Color32::from_rgb(90, 220, 120)));
                        let mid = a + (b - a) * 0.5;
                        painter.text(mid, Align2::CENTER_BOTTOM, crate::i18n::tr_dyn(&format!("{o} → {}", parts.get(1).unwrap_or(&""))), FontId::proportional(10.0 * zoom.max(0.8)), Color32::from_rgb(160, 240, 170));
                    }
                }
            }
            // Knoten zeichnen + Interaktion
            let pointer = ui.input(|i| i.pointer.interact_pos());
            if resp.drag_started_by(egui::PointerButton::Primary) {
                self.dlg.logical_drag = pointer.and_then(|p| ents.iter().rev().find(|n| node_rect(n.3).contains(p)).map(|n| n.0));
            }
            if let Some(id) = self.dlg.logical_drag {
                let d = resp.drag_delta() / zoom;
                if d != Vec2::ZERO {
                    if let Some(e) = self.ed.map.entity_mut(id) {
                        let p = e.editor.logicalpos.unwrap_or_else(|| ents.iter().find(|n| n.0 == id).map(|n| n.3).unwrap_or([0.0, 0.0]));
                        e.editor.logicalpos = Some([p[0] + d.x as f64, p[1] + d.y as f64]);
                        self.ed.dirty = true;
                    }
                }
            }
            if resp.drag_stopped() {
                self.dlg.logical_drag = None;
            }
            if resp.clicked() {
                if let Some(p) = pointer {
                    if let Some(n) = ents.iter().rev().find(|n| node_rect(n.3).contains(p)) {
                        self.ed.sel.clear();
                        self.ed.sel.insert(Obj::Entity(n.0));
                        self.ed.touch();
                    }
                }
            }
            for (id, class, name, pos) in &ents {
                let r = node_rect(*pos);
                let sel = self.ed.sel.contains(&Obj::Entity(*id));
                painter.rect_filled(r, egui::CornerRadius::same(4), if sel { Color32::from_rgb(110, 40, 40) } else { Color32::from_rgb(45, 50, 65) });
                painter.rect_stroke(r, egui::CornerRadius::same(4), Stroke::new(1.0f32, Color32::from_gray(140)), egui::StrokeKind::Inside);
                painter.text(r.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, if name.is_empty() { crate::i18n::tr("(ohne Namen)") } else { name }, FontId::proportional(12.0 * zoom.max(0.7)), Color32::WHITE);
                painter.text(r.left_bottom() + Vec2::new(6.0, -4.0), Align2::LEFT_BOTTOM, class, FontId::monospace(10.0 * zoom.max(0.7)), Color32::from_gray(180));
            }
            if ents.is_empty() {
                painter.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr("Keine benannten Entities oder Outputs"), FontId::proportional(14.0), Color32::GRAY);
            }
        });
        self.win.logical = open;
    }

    fn entity_help_window(&mut self, ctx: &egui::Context) {
        let Some(class) = self.win.entity_help.clone() else { return };
        let mut open = true;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr_dyn(&format!("Hilfe: {class}")), 560.0), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.ed.fgd.class(&class) {
                None => {
                    ui.label(crate::i18n::tr("Diese Klasse ist in keiner geladenen FGD beschrieben."));
                }
                Some(c) => {
                    ui.label(crate::i18n::tr_dyn(&c.description));
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::tr("Schlüssel")).strong());
                    for p in self.ed.fgd.all_props(&class) {
                        ui.label(RichText::new(format!("{} ({}) – {}", p.name, p.kind, crate::i18n::tr_dyn(&p.display))).monospace().small());
                        if !p.description.is_empty() {
                            ui.label(RichText::new(crate::i18n::tr_dyn(&p.description)).small().italics());
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::tr("Inputs")).strong());
                    for i in self.ed.fgd.all_io(&class, false) {
                        ui.label(RichText::new(format!("{} – {}", i.name, crate::i18n::tr_dyn(&i.description))).small());
                    }
                    ui.label(RichText::new(crate::i18n::tr("Outputs")).strong());
                    for o in self.ed.fgd.all_io(&class, true) {
                        ui.label(RichText::new(format!("{} – {}", o.name, crate::i18n::tr_dyn(&o.description))).small());
                    }
                }
            });
        });
        if !open {
            self.win.entity_help = None;
        }
    }
}

/// Draufsicht (X/Y) der Kopien für „Inhalte einfügen“
fn paste_preview(ui: &mut egui::Ui, cb: Option<crate::geometry::Aabb>, copies: u32, offset: DVec3, rot: DVec3) {
    use crate::transform::Xform;
    use egui::{Rect, Sense, Stroke, StrokeKind, Vec2};
    let th = crate::theme::t();
    let (r, _) = ui.allocate_exact_size(Vec2::splat(236.0), Sense::hover());
    ui.painter().rect(r, 9.0, th.vp, Stroke::new(1.0f32, th.line), StrokeKind::Inside);
    let p = ui.painter().with_clip_rect(r.shrink(1.0));
    let mut x = r.left() + 8.0;
    while x < r.right() {
        p.vline(x, r.y_range(), Stroke::new(1.0f32, th.grid));
        x += 16.0;
    }
    let mut y = r.top() + 8.0;
    while y < r.bottom() {
        p.hline(r.x_range(), y, Stroke::new(1.0f32, th.grid));
        y += 16.0;
    }
    let total_label;
    if let Some(b) = cb {
        let c = b.center();
        let corners = b.corners();
        let mut shapes: Vec<Vec<DVec3>> = vec![corners.to_vec()];
        let mut total = b;
        for i in 1..=copies.min(256) {
            let k = i as f64;
            let rx = Xform::rotation(2, rot.z * k) * Xform::rotation(1, rot.y * k) * Xform::rotation(0, rot.x * k);
            let xf = Xform::about(c, rx).then(&Xform::translate(offset * k));
            let pts: Vec<DVec3> = corners.iter().map(|q| xf.apply(*q)).collect();
            for q in &pts {
                total = total.union(&crate::geometry::Aabb::new(*q, *q));
            }
            shapes.push(pts);
        }
        // automatisch einpassen
        let size = (total.max - total.min).truncate().max_element().max(1.0);
        let scale = (r.width() - 40.0) as f64 / size;
        let mid = (total.min + total.max) * 0.5;
        let to = |q: DVec3| egui::pos2(r.center().x + ((q.x - mid.x) * scale) as f32, r.center().y - ((q.y - mid.y) * scale) as f32);
        let edges = [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
        for (i, pts) in shapes.iter().enumerate().skip(1) {
            let a = (1.0 - 0.06 * (i as f32 - 1.0)).max(0.25);
            for (u, v) in edges {
                p.line_segment([to(pts[u]), to(pts[v])], Stroke::new(1.0f32, crate::theme::alpha(th.brush, a)));
            }
        }
        // Original
        let o = &shapes[0];
        let orect = Rect::from_two_pos(to(o[0]), to(o[6]));
        p.rect(orect, 2.0, th.accent_soft, Stroke::new(1.5f32, th.accent), StrokeKind::Inside);
        // Richtungslinie (gestrichelt) mit Pfeil
        if copies > 0 && offset.truncate().length() > 0.0 {
            let a = to(c);
            let e = to(c + offset * copies as f64);
            p.extend(egui::Shape::dashed_line(&[a, e], Stroke::new(1.0f32, th.text3), 4.0, 3.0));
            let d = (e - a).normalized();
            let n = Vec2::new(-d.y, d.x);
            p.add(egui::Shape::convex_polygon(vec![e, e - d * 7.0 + n * 3.5, e - d * 7.0 - n * 3.5], th.text3, Stroke::NONE));
        }
        let s = total.max - total.min;
        total_label = format!("Gesamt {} × {} × {}", crate::map::fmt_num(s.x), crate::map::fmt_num(s.y), crate::map::fmt_num(s.z));
    } else {
        p.text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Zwischenablage leer"), crate::theme::sans(12.0), th.text3);
        total_label = String::new();
    }
    // Schilder
    let tag = |txt: &str, at: egui::Pos2, align: egui::Align2| {
        let g = ui.painter().layout_no_wrap(txt.to_string(), crate::theme::sans(11.0), th.text2);
        let br = align.anchor_size(at, g.size() + Vec2::new(12.0, 6.0));
        ui.painter().rect_filled(br, 6.0, th.glass);
        ui.painter().galley(br.min + Vec2::new(6.0, 3.0), g, th.text2);
    };
    tag(crate::i18n::tr("Oben · X/Y"), r.left_top() + Vec2::new(8.0, 8.0), egui::Align2::LEFT_TOP);
    tag("■ Original", r.left_bottom() + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM);
    ui.add_space(6.0);
    ui.label(RichText::new(total_label).font(crate::theme::mono(11.5)).color(th.text2));
}
