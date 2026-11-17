//! Einrichtungs-Assistent beim ersten Start: Sprache und Oberfläche, Spielordner, Inhalte aus
//! Addons und eingebundenen Spielen, danach Tutorial oder erste Map. Über Hilfe erneut aufrufbar.

use eframe::egui::{self, RichText, Vec2};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct SetupState {
    pub page: usize,
    pub status: String,
    /// gewählter Einstieg auf der letzten Seite (1 Tutorial, 2 Vorlage, 3 Startbildschirm)
    pub choice: Option<u8>,
}

const PAGES: usize = 4;

/// Eingebundene Spiele aus garrysmod/cfg/mount.cfg: (Name, Pfad, vorhanden)
pub fn mounts(game_dir: &str) -> Vec<(String, PathBuf, bool)> {
    let Ok(src) = std::fs::read_to_string(Path::new(game_dir).join("cfg/mount.cfg")) else { return Vec::new() };
    let Ok(blocks) = crate::keyvalues::parse(&src) else { return Vec::new() };
    blocks.iter().flat_map(|b| b.pairs().map(|(k, v)| (k.to_string(), PathBuf::from(v), Path::new(v).is_dir())).collect::<Vec<_>>()).collect()
}

/// Prüfpunkt: (Text, ok)
fn checks(s: &crate::compile::Settings) -> Vec<(String, bool)> {
    let gd = Path::new(&s.game_dir);
    let exe_ok = |p: &str| !p.is_empty() && Path::new(p).is_file();
    vec![
        (crate::i18n::tr("Spielordner mit gameinfo.txt").into(), gd.join("gameinfo.txt").is_file()),
        (crate::i18n::tr("Compiler VBSP / VVIS / VRAD").into(), exe_ok(&s.vbsp) && exe_ok(&s.vvis) && exe_ok(&s.vrad)),
        (crate::i18n::tr("Spiel (zum Testen der Map)").into(), exe_ok(&s.game_exe)),
        (if s.fgd_files.is_empty() { crate::i18n::tr("Entity-Liste: eingebaut (456 Klassen)") } else { crate::i18n::tr("Entity-Liste: garrysmod.fgd gefunden") }.into(), true),
    ]
}

impl crate::app::App {
    /// Beim Start: Assistent zeigen, wenn noch nie eingerichtet (bestehende Installationen gelten als eingerichtet)
    pub(crate) fn maybe_start_setup(&mut self) {
        if self.settings.setup_done {
            return;
        }
        if !self.settings.game_dir.is_empty() && !self.settings.recent_files.is_empty() {
            self.settings.setup_done = true;
            return;
        }
        self.setup = SetupState::default();
        // neue Installation: mit der einfachen Oberfläche beginnen
        self.settings.simple_ui = true;
        self.win.setup = true;
    }

    pub(crate) fn setup_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.win.setup {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let page = self.setup.page.min(PAGES - 1);
        let (mut next, mut back, mut skip, mut finish) = (false, false, false, None::<u8>);
        let (mut find, mut pick) = (false, false);
        let mut back_to = None::<usize>;
        let mut choice = self.setup.choice;
        let titles = [crate::i18n::tr("Willkommen bei Amboss"), crate::i18n::tr("Garry's Mod finden"), crate::i18n::tr("Inhalte"), crate::i18n::tr("Fertig!")];
        let sub = crate::i18n::tr_dyn(&format!("Schritt {} von {}", page + 1, PAGES));
        // feste Größe: der Assistent springt zwischen den Schritten weder in Breite noch Höhe
        let d = theme::Dlg::new(titles[page], 640.0).subtitle(sub).footer().height(640.0).with_id("setup").modal();
        let mounts = if page == 2 { mounts(&self.settings.game_dir) } else { Vec::new() };
        let checks = if page == 1 || page == 3 { checks(&self.settings) } else { Vec::new() };
        let status = self.setup.status.clone();
        let content_status = self.content_status.clone();
        let s = &mut self.settings;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 16)).show(ui, |ui| {
                    ui.set_width(640.0 - 40.0 - 48.0);
                    // Fortschritt
                    ui.horizontal(|ui| {
                        for i in 0..PAGES {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 4.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, 2.0, if i <= page { th.accent } else { th.line });
                        }
                    });
                    ui.add_space(12.0);
                    match page {
                        0 => {
                            ui.label(RichText::new(crate::i18n::tr("Amboss ist ein Map-Editor für Garry's Mod und andere Source-Spiele. In vier kurzen Schritten ist alles bereit.")).size(14.0));
                            ui.add_space(14.0);
                            theme::section(ui, "Sprache / Language");
                            let cur = crate::i18n::Lang::ALL.iter().position(|l| *l == crate::i18n::lang()).unwrap_or(0);
                            let names: Vec<&str> = crate::i18n::Lang::ALL.iter().map(|l| l.native()).collect();
                            let n = theme::segmented(ui, cur, &names, false);
                            if n != cur {
                                let l = crate::i18n::Lang::ALL[n];
                                crate::i18n::set_lang(l);
                                s.language = l.code().into();
                            }
                            ui.add_space(10.0);
                            theme::section(ui, crate::i18n::tr("Oberfläche"));
                            for (simple, title, text) in [
                                (true, crate::i18n::tr("Einfach"), crate::i18n::tr("Die wichtigsten Werkzeuge und kurze Menüs. Ideal, wenn du neu mit dem Mappen beginnst.")),
                                (false, crate::i18n::tr("Experte"), crate::i18n::tr("Alle Werkzeuge: Vertex-Bearbeitung, Displacements, Overlays, Instanzen, Lua, Versionen…")),
                            ] {
                                if theme::choice_card(ui, s.simple_ui == simple, title, text).clicked() {
                                    s.simple_ui = simple;
                                }
                                ui.add_space(6.0);
                            }
                            ui.label(RichText::new(crate::i18n::tr("Beides lässt sich jederzeit unter Ansicht → Oberfläche umschalten.")).color(th.text3).size(11.5));
                        }
                        1 => {
                            ui.label(crate::i18n::tr("Amboss braucht den Ordner von Garry's Mod: für Texturen und Modelle, die Compiler (VBSP, VVIS, VRAD) und um die Map zu testen."));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if theme::primary_button(ui, crate::i18n::tr("Automatisch suchen")).clicked() {
                                    find = true;
                                }
                                if theme::secondary_button(ui, crate::i18n::tr("Ordner wählen…")).clicked() {
                                    pick = true;
                                }
                            });
                            ui.add_space(8.0);
                            theme::field_row(ui, "Spielordner", |ui| {
                                let full = if s.game_dir.is_empty() { crate::i18n::tr("(noch nicht gesetzt)").to_string() } else { s.game_dir.clone() };
                                let max = ((ui.available_width() / 6.8) as usize).max(12);
                                ui.add(egui::Label::new(RichText::new(theme::short_path(&full, max)).font(theme::mono(11.0)))).on_hover_text(full)
                            });
                            if !status.is_empty() {
                                ui.label(RichText::new(&status).size(12.0).color(th.text2));
                            }
                            ui.add_space(8.0);
                            for (t, ok) in &checks {
                                ui.horizontal(|ui| {
                                    theme::severity_mark(ui, if *ok { 3 } else { 0 });
                                    ui.label(t);
                                });
                            }
                            ui.add_space(6.0);
                            ui.label(RichText::new(crate::i18n::tr("Gesucht wird in allen Steam-Bibliotheken. Gewählt werden kann der Ordner „GarrysMod“ oder „GarrysMod/garrysmod“.")).color(th.text3).size(11.5));
                        }
                        2 => {
                            ui.label(crate::i18n::tr("Welche Texturen und Modelle soll Amboss anbieten?"));
                            ui.add_space(10.0);
                            crate::theme::checkbox(ui, &mut s.load_addons, RichText::new(crate::i18n::tr("Inhalte aus Addons und dem Workshop verwenden")).strong());
                            ui.label(RichText::new(crate::i18n::tr("Abonnierte Workshop-Addons und Ordner unter garrysmod/addons. Benutzt deine Map solche Inhalte, erstellt Amboss beim Kompilieren eine Liste zum Verlinken im Workshop.")).color(th.text3).size(12.0));
                            ui.add_space(12.0);
                            theme::section(ui, "Eingebundene Spiele (mount.cfg)");
                            if mounts.is_empty() {
                                ui.label(RichText::new(crate::i18n::tr("Keine Spiele eingebunden. Inhalte anderer Source-Spiele (z. B. Counter-Strike: Source) bindest du in Garry's Mod ein: Hauptmenü → Spiele-Symbol.")).color(th.text3).size(12.0));
                            }
                            for (name, path, ok) in &mounts {
                                ui.horizontal(|ui| {
                                    theme::severity_mark(ui, if *ok { 3 } else { 1 });
                                    ui.label(RichText::new(name).strong());
                                    ui.label(RichText::new(path.display().to_string()).font(theme::mono(10.5)).color(th.text3));
                                });
                            }
                            ui.add_space(10.0);
                            ui.label(RichText::new(crate::i18n::tr("Mit „Weiter“ werden die Inhalte eingelesen – das läuft im Hintergrund weiter.")).color(th.text3).size(11.5));
                        }
                        _ => {
                            let missing = checks.iter().filter(|(_, ok)| !*ok).count();
                            if missing == 0 {
                                ui.label(RichText::new(crate::i18n::tr("Alles bereit. Wie möchtest du anfangen?")).size(14.0));
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Fast fertig – {} fehlen noch.", crate::i18n::count_de(missing, "Punkt", "Punkte")))).size(14.0));
                                    if ui.link(crate::i18n::tr("Zu Schritt 2")).clicked() {
                                        back_to = Some(1);
                                    }
                                });
                            }
                            ui.add_space(6.0);
                            for (t, ok) in &checks {
                                ui.horizontal(|ui| {
                                    theme::severity_mark(ui, if *ok { 3 } else { 1 });
                                    ui.label(RichText::new(t).size(12.0));
                                });
                            }
                            if !content_status.is_empty() {
                                ui.label(RichText::new(crate::i18n::tr_dyn(&content_status)).size(11.5).color(th.text3));
                            }
                            ui.add_space(12.0);
                            // empfohlener Einstieg vorgewählt: Einfach → Tutorial, Experte → Vorlage
                            if choice.is_none() {
                                choice = Some(if s.simple_ui { 1 } else { 2 });
                            }
                            for (k, title, text) in [
                                (1u8, crate::i18n::tr("Tutorial starten"), crate::i18n::tr("Schritt für Schritt zur ersten eigenen Map – empfohlen für Einsteiger.")),
                                (2, crate::i18n::tr("Neue Map aus Vorlage"), crate::i18n::tr("Fertigen Raum mit Licht und Spawnpunkt öffnen und loslegen.")),
                                (3, crate::i18n::tr("Zum Startbildschirm"), crate::i18n::tr("Vorlagen, zuletzt geöffnete Maps, Map öffnen.")),
                            ] {
                                let r = theme::choice_card(ui, choice == Some(k), title, text);
                                if r.clicked() {
                                    choice = Some(k);
                                }
                                if r.double_clicked() {
                                    finish = Some(k);
                                }
                                ui.add_space(6.0);
                            }
                        }
                    }
                });
            }
            theme::Part::Footer => {
                let failed = page == 1 && checks.iter().any(|(_, ok)| !*ok);
                if page + 1 < PAGES && theme::primary_button(ui, if failed { crate::i18n::tr("Trotzdem weiter") } else { crate::i18n::tr("Weiter") }).clicked() {
                    next = true;
                }
                if page + 1 == PAGES && theme::primary_button(ui, crate::i18n::tr("Los geht's")).clicked() {
                    finish = choice.or(Some(3));
                }
                if page > 0 && theme::secondary_button(ui, crate::i18n::tr("Zurück")).clicked() {
                    back = true;
                }
                // „Überspringen“ links, weit weg vom Hauptknopf
                if page + 1 < PAGES {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if ui.link(crate::i18n::tr("Überspringen")).clicked() {
                            skip = true;
                        }
                    });
                }
            }
            _ => {}
        });
        if find {
            self.setup.status = match self.settings.autodetect() {
                Ok(p) => {
                    self.settings.detect_fgd();
                    crate::i18n::tr_dyn(&format!("Garry's Mod gefunden: {p}"))
                }
                Err(e) => e,
            };
        }
        if pick {
            if let Some(dir) = rfd::FileDialog::new().set_title(crate::i18n::tr("GarrysMod-Hauptordner (enthält 'garrysmod' und 'bin')")).pick_folder() {
                // GarrysMod/garrysmod gewählt? Dann eine Ebene höher
                let root = if dir.join("gameinfo.txt").is_file() { dir.parent().map(Path::to_path_buf).unwrap_or(dir) } else { dir };
                self.settings.apply_root(&root);
                self.settings.detect_fgd();
                self.setup.status = crate::i18n::tr_dyn(&format!("Ordner gesetzt: {}", root.display()));
            }
        }
        if next {
            // nach „Inhalte“: Spielinhalte einlesen (mit der gewählten Addon-Einstellung)
            if page == 2 || (page == 1 && !self.settings.game_dir.is_empty()) {
                self.reload_fgds();
                self.reload_content();
            }
            self.setup.page = (page + 1).min(PAGES - 1);
        }
        if back {
            self.setup.page = page.saturating_sub(1);
        }
        if let Some(p) = back_to {
            self.setup.page = p;
        }
        self.setup.choice = choice;
        if skip {
            open = false;
        }
        if let Some(k) = finish {
            open = false;
            match k {
                1 => self.win.tutorial = true,
                2 => {
                    self.win.start = false;
                    self.new_from_template("raum");
                }
                _ => self.win.start = true,
            }
        }
        if !open {
            self.settings.setup_done = true;
            self.win.setup = false;
        }
    }
}
