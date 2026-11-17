//! Hinweis „Neue Version verfügbar“ und Ablauf des Updates in der Oberfläche.

use crate::app::App;
use crate::update::{self, Release};
use eframe::egui::{self, RichText};
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub(crate) struct UpdateUi {
    check_rx: Option<Receiver<Result<Option<Release>, String>>>,
    install_rx: Option<Receiver<Result<(), String>>>,
    pub available: Option<Release>,
    /// Hinweis ausgeblendet („Später“)
    dismissed: bool,
    /// von Hand gesucht: auch „aktuell“ und Fehler melden
    manual: bool,
    pub installed: bool,
    pub error: String,
    /// nach dem Beenden die neue Version starten
    pub relaunch: bool,
    pub show_notes: bool,
}

impl App {
    /// Im Hintergrund nach einer neueren Version suchen
    pub(crate) fn start_update_check(&mut self, manual: bool) {
        if self.upd.check_rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
        });
        self.upd.check_rx = Some(rx);
        self.upd.manual = manual;
        self.upd.dismissed = false;
        self.upd.error.clear();
        if manual {
            self.ed.status = crate::i18n::tr("Suche nach Updates…").into();
        }
    }

    pub(crate) fn update_notice(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.upd.check_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.upd.check_rx = None;
                    match r {
                        Ok(Some(rel)) => {
                            if self.upd.manual || rel.version != self.settings.skip_version {
                                self.upd.available = Some(rel);
                            }
                        }
                        Ok(None) => {
                            if self.upd.manual {
                                self.ed.status = crate::i18n::tr_dyn(&format!("Amboss {} ist aktuell.", update::CURRENT)).into();
                            }
                        }
                        Err(e) => {
                            if self.upd.manual {
                                self.ed.status = crate::i18n::tr_dyn(&format!("Update-Suche fehlgeschlagen: {e}")).into();
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
                Err(_) => self.upd.check_rx = None,
            }
        }
        if let Some(rx) = &self.upd.install_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.upd.install_rx = None;
                    match r {
                        Ok(()) => self.upd.installed = true,
                        Err(e) => self.upd.error = e,
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => self.upd.install_rx = None,
            }
        }
        let Some(rel) = self.upd.available.clone() else { return };
        if self.upd.dismissed {
            return;
        }
        let th = crate::theme::t();
        let tr = crate::i18n::tr;
        let screen = ctx.screen_rect();
        let w = 360.0;
        let mut later = false;
        let mut skip = false;
        let mut install = false;
        let mut restart = false;
        egui::Area::new(egui::Id::new("update_notice")).order(egui::Order::Foreground).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0)).show(ctx, |ui| {
            let _ = screen;
            egui::Frame::new()
                .fill(th.panel)
                .stroke(egui::Stroke::new(1.0f32, th.line_strong))
                .corner_radius(12)
                .shadow(egui::epaint::Shadow { offset: [0, 12], blur: 32, spread: 0, color: th.shadow })
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        crate::icons::draw(ui.painter(), r, "download", th.accent, 1.6);
                        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Amboss {} ist verfügbar", rel.version))).family(crate::theme::semibold()).size(14.0).color(th.text));
                    });
                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Du hast {}.", update::CURRENT))).size(12.0).color(th.text2));
                    if !rel.notes.trim().is_empty() {
                        let label = if self.upd.show_notes { tr("Neuerungen ausblenden") } else { tr("Was ist neu?") };
                        if ui.link(label).clicked() {
                            self.upd.show_notes = !self.upd.show_notes;
                        }
                        if self.upd.show_notes {
                            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                                ui.label(RichText::new(rel.notes.trim()).size(12.0).color(th.text2));
                            });
                        }
                    }
                    if self.upd.install_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Lade und installiere…"));
                        });
                    } else if self.upd.installed {
                        ui.label(RichText::new(tr("Installiert – aktiv nach dem Neustart.")).color(th.ok));
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::new(RichText::new(tr("Jetzt neu starten")).color(th.accent_fg)).fill(th.accent)).clicked() {
                                restart = true;
                            }
                            if ui.button(tr("Später")).clicked() {
                                later = true;
                            }
                        });
                    } else {
                        if !self.upd.error.is_empty() {
                            ui.label(RichText::new(crate::i18n::tr_dyn(&self.upd.error)).size(12.0).color(th.warn));
                        }
                        ui.horizontal(|ui| {
                            if rel.asset.is_some() && ui.add(egui::Button::new(RichText::new(tr("Jetzt aktualisieren")).color(th.accent_fg)).fill(th.accent)).clicked() {
                                install = true;
                            }
                            if !rel.page.is_empty() && ui.button(tr("Release-Seite")).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(&rel.page));
                            }
                            if ui.button(tr("Später")).clicked() {
                                later = true;
                            }
                        });
                        if ui.small_button(tr("Diese Version überspringen")).clicked() {
                            skip = true;
                        }
                    }
                    ui.label(RichText::new(tr("Updates abschalten: Einstellungen → Oberfläche.")).size(11.0).color(th.text3));
                });
        });
        if later {
            self.upd.dismissed = true;
        }
        if skip {
            self.settings.skip_version = rel.version.clone();
            self.upd.dismissed = true;
            self.upd.available = None;
        }
        if install {
            let (tx, rx) = channel();
            let r2 = rel.clone();
            std::thread::spawn(move || {
                let _ = tx.send(update::install(&r2));
            });
            self.upd.install_rx = Some(rx);
            self.upd.error.clear();
        }
        if restart {
            // normaler Beenden-Weg: fragt bei ungespeicherten Änderungen nach
            self.upd.relaunch = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Einstellungen → Allgemein: Abschnitt „Updates“
    pub(crate) fn update_settings_ui(&mut self, ui: &mut egui::Ui) {
        let tr = crate::i18n::tr;
        ui.label(RichText::new(tr("Updates")).strong());
        ui.checkbox(&mut self.settings.check_updates, tr("Beim Start nach neuen Versionen suchen (GitHub)"));
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Installiert: {}", update::CURRENT))).color(crate::theme::t().text2));
            if self.upd.check_rx.is_some() {
                ui.spinner();
            } else if ui.button(tr("Jetzt suchen")).clicked() {
                self.start_update_check(true);
            }
        });
        ui.label(RichText::new(tr("Es wird nur die öffentliche Versionsliste abgerufen; Amboss sendet dabei keine Daten über dich oder deine Maps.")).small().weak());
    }
}
