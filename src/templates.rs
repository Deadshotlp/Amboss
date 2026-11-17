//! Vorlagen für neue Maps und Startbildschirm.

use crate::app::App;
use crate::editor::Editor;
use crate::geometry::Aabb;
use crate::prefabs::{b, ent, tie};
use crate::icons;
use crate::theme;
use eframe::egui::{self, Color32, RichText, Stroke};
use glam::DVec3;
use std::path::PathBuf;

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub build: fn(&mut Editor),
}

const DEV: &str = "DEV/DEV_MEASUREGENERIC01B";
const GRAY: &str = "DEV/GRAYGRID";

fn sky_hull(ed: &mut Editor, min: [f64; 3], max: [f64; 3], floor_mat: &str) {
    b(ed, floor_mat, min, max);
    ed.hollow(16.0);
    let floor = ed.map.world.solids.iter().min_by(|a, c| a.bounds.min.z.partial_cmp(&c.bounds.min.z).unwrap()).map(|s| s.id).unwrap();
    for s in &mut ed.map.world.solids {
        if s.id != floor {
            s.set_material_all("TOOLS/TOOLSSKYBOX");
        }
    }
}

pub const TEMPLATES: &[Template] = &[
    Template { id: "leer", name: "Leere Map", description: "Nichts außer den Map-Eigenschaften.", build: |_| {} },
    Template {
        id: "raum",
        name: "Raum mit Licht und Spawn",
        description: "Abgedichteter Raum 512 × 512 × 256 mit Spawn, Licht und Cubemap – sofort kompilierbar.",
        build: |ed| {
            if let Some(m) = crate::prefabs::build_builtin("spawnraum") {
                ed.map = m;
            }
        },
    },
    Template {
        id: "aussen",
        name: "Außenbereich",
        description: "Himmel, Sonne (light_environment) und Boden 2048 × 2048.",
        build: |ed| {
            if let Some(m) = crate::prefabs::build_builtin("aussenbereich") {
                ed.map = m;
            }
        },
    },
    Template {
        id: "bauwiese",
        name: "Große Bauwiese",
        description: "Sehr großer flacher Außenbereich (8192 × 8192) zum Bauen im Sandbox-Modus, mit Sonne und Spawnpunkten.",
        build: |ed| {
            sky_hull(ed, [-4096.0, -4096.0, -16.0], [4096.0, 4096.0, 2048.0], GRAY);
            ent(ed, "light_environment", [0.0, 0.0, 1024.0], &[("angles", "0 60 0"), ("pitch", "-45"), ("_light", "255 245 230 400"), ("_ambient", "150 170 200 80")]);
            for (i, (x, y)) in [(0.0, 0.0), (128.0, 0.0), (0.0, 128.0), (128.0, 128.0)].iter().enumerate() {
                ent(ed, "info_player_start", [*x, *y, 0.0], &[("angles", &format!("0 {} 0", i * 90))]);
            }
            ent(ed, "env_cubemap", [0.0, 0.0, 128.0], &[]);
        },
    },
    Template {
        id: "arena",
        name: "Arena (Deathmatch)",
        description: "Halle 2048 × 2048 × 512 mit Säulen, vier Spawnpunkten, Lichtern und Cubemap.",
        build: |ed| {
            b(ed, DEV, [-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 512.0]);
            ed.hollow(16.0);
            for (x, y) in [(-512.0, -512.0), (512.0, -512.0), (-512.0, 512.0), (512.0, 512.0)] {
                b(ed, "DEV/DEV_MEASUREGENERIC01", [x - 64.0, y - 64.0, 0.0], [x + 64.0, y + 64.0, 496.0]);
            }
            for (i, (x, y)) in [(-800.0, 0.0), (800.0, 0.0), (0.0, -800.0), (0.0, 800.0)].iter().enumerate() {
                let yaw = [0, 180, 90, 270][i];
                ent(ed, "info_player_start", [*x, *y, 0.0], &[("angles", &format!("0 {yaw} 0"))]);
            }
            for x in [-640.0, 0.0, 640.0] {
                for y in [-640.0, 0.0, 640.0] {
                    ent(ed, "light", [x, y, 448.0], &[("_light", "255 240 220 350")]);
                }
            }
            ent(ed, "env_cubemap", [0.0, 0.0, 96.0], &[]);
        },
    },
    Template {
        id: "zwei_raeume",
        name: "Zwei Räume mit Tür (NPC-Test)",
        description: "Zwei Räume, verbunden durch eine Schiebetür mit Knöpfen – gut zum Testen von KI-Nodes und Areaportals.",
        build: |ed| {
            b(ed, DEV, [-512.0, -256.0, -16.0], [512.0, 256.0, 208.0]);
            ed.hollow(16.0);
            let wall = b(ed, DEV, [-8.0, -240.0, 0.0], [8.0, 240.0, 192.0]);
            let cut = b(ed, DEV, [-16.0, -32.0, 0.0], [16.0, 32.0, 112.0]);
            ed.sel = [crate::editor::Obj::Solid(cut)].into_iter().collect();
            ed.carve();
            ed.sel = [crate::editor::Obj::Solid(cut)].into_iter().collect();
            ed.delete_selection();
            let _ = wall;
            let d = b(ed, "DEV/DEV_MEASUREGENERIC01", [-2.0, -32.0, 0.0], [2.0, 32.0, 112.0]);
            tie(ed, &[d], "func_door", &[("targetname", "tuer"), ("movedir", "-90 0 0"), ("speed", "120"), ("wait", "4"), ("lip", "8"), ("spawnflags", "0")]);
            for x in [-24.0, 20.0] {
                let k = b(ed, "DEV/REFLECTIVITY_50", [x, 40.0, 48.0], [x + 4.0, 56.0, 64.0]);
                let id = tie(ed, &[k], "func_button", &[("spawnflags", "1025"), ("wait", "2")]);
                if let Some(e) = ed.map.entity_mut(id) {
                    e.outputs.push(("OnPressed".into(), "tuer,Open,,0,-1".into()));
                }
            }
            ent(ed, "info_player_start", [-400.0, 0.0, 0.0], &[("angles", "0 0 0")]);
            ent(ed, "light", [-256.0, 0.0, 160.0], &[("_light", "255 240 220 300")]);
            ent(ed, "light", [256.0, 0.0, 160.0], &[("_light", "220 230 255 300")]);
        },
    },
];

/// Editor mit einer Vorlage füllen (ohne Rückgängig-Verlauf)
pub fn apply(ed: &mut Editor, id: &str) -> bool {
    let Some(t) = TEMPLATES.iter().find(|t| t.id == id) else { return false };
    let keep = (ed.material.clone(), ed.entity_class.clone(), ed.next_model.clone());
    (t.build)(ed);
    (ed.material, ed.entity_class, ed.next_model) = keep;
    ed.sel.clear();
    ed.faces.clear();
    ed.clear_history();
    ed.status = format!("Neue Map aus Vorlage „{}“", t.name);
    ed.touch();
    true
}

impl App {
    pub(crate) fn new_from_template(&mut self, id: &str) {
        if !self.is_pristine_pub() {
            self.new_tab();
        }
        if apply(&mut self.ed, id) {
            self.ed.dirty = id != "leer";
            let b = self.ed.map_bounds();
            if !b.is_empty() {
                for v in &mut self.views2d {
                    v.focus(&b, egui::Vec2::new(700.0, 450.0));
                }
                self.view3d.focus(&Aabb::new(b.min, b.max.min(b.min + DVec3::splat(1024.0))));
            }
        }
        self.win.start = false;
    }

    /// Startseite (füllt das Fenster, solange sie offen ist)
    pub(crate) fn start_page(&mut self, ctx: &egui::Context) {
        let th = theme::t();
        if self.template_maps.is_empty() {
            self.template_maps = TEMPLATES
                .iter()
                .map(|tp| {
                    let mut ed = Editor::new(crate::fgd::Fgd::builtin());
                    (tp.build)(&mut ed);
                    ed.map
                })
                .collect();
            self.recovery = find_recovery(&self.settings.recent_files);
        }
        let mut action: Option<StartAction> = None;
        egui::SidePanel::left("start_side").exact_width(288.0).resizable(false).frame(egui::Frame::new().fill(th.panel).inner_margin(egui::Margin { left: 22, right: 22, top: 28, bottom: 22 })).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 11.0, th.accent);
                icons::draw(ui.painter(), r.shrink(8.0), "anvil", th.accent_fg, 2.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(crate::i18n::tr("Amboss")).font(egui::FontId::new(18.0, theme::bold())));
                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Map-Editor für Source · Version {}", env!("CARGO_PKG_VERSION")))).size(12.0).color(th.text3));
                });
            });
            ui.add_space(16.0);
            if side_button(ui, "plus", crate::i18n::tr("Neue Map"), Some("Strg N"), true).clicked() {
                action = Some(StartAction::Template("leer"));
            }
            if side_button(ui, "folder", crate::i18n::tr("Map öffnen…"), Some("Strg O"), false).clicked() {
                action = Some(StartAction::Open(None));
            }
            if side_button(ui, "people", crate::i18n::tr("Live-Sitzung beitreten"), None, false).clicked() {
                action = Some(StartAction::Collab);
            }
            ui.add_space(16.0);
            theme::section(ui, crate::i18n::tr("Spielprofil"));
            let pname = self.settings.profiles.get(self.settings.active_profile).map(|p| p.name.clone()).unwrap_or_else(|| crate::i18n::tr("Kein Profil").into());
            let resp = side_button(ui, "settings", &pname, None, false);
            let pop = ui.make_persistent_id("start_profiles");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(pop));
            }
            egui::popup::popup_below_widget(ui, pop, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(220.0);
                let names: Vec<String> = self.settings.profiles.iter().map(|p| p.name.clone()).collect();
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == self.settings.active_profile, n).clicked() {
                        self.settings.load_profile(i);
                        self.reload_content();
                        self.reload_fgds();
                    }
                }
            });
            let fgd_ok = !self.settings.fgd_files.is_empty() && self.fgd_messages.iter().all(|m| !m.to_ascii_lowercase().contains("fehler"));
            let tools_ok = [&self.settings.vbsp, &self.settings.vvis, &self.settings.vrad].iter().all(|p| !p.is_empty() && std::path::Path::new(p.as_str()).is_file());
            let loading = self.content_rx.is_some();
            let content_ok = !self.mats.list.is_empty();
            let content_text = if loading { crate::i18n::tr("Spielinhalte werden geladen…").to_string() } else if content_ok { crate::i18n::tr_dyn(&format!("{} Materialien geladen", crate::chrome::fmt_thousands(self.mats.list.len()))) } else if self.settings.game_dir.is_empty() { crate::i18n::tr("Kein Spielordner – Einstellungen").to_string() } else { crate::i18n::tr("Keine Spielinhalte – Spielordner prüfen").to_string() };
            let nclasses = self.ed.fgd.names(crate::fgd::ClassKind::Point).len() + self.ed.fgd.names(crate::fgd::ClassKind::Solid).len();
            let fgd_good = crate::i18n::tr_dyn(&format!("FGD geladen – {nclasses} Entity-Klassen"));
            let fgd_bad = crate::i18n::tr_dyn(&format!("Eingebaute Entity-Liste ({nclasses} Klassen)"));
            for (ok, good, bad) in [
                (fgd_ok, fgd_good.as_str(), fgd_bad.as_str()),
                (tools_ok, crate::i18n::tr("VBSP · VVIS · VRAD gefunden"), crate::i18n::tr("Compiler nicht gefunden")),
            ] {
                // Klick auf eine Statuszeile öffnet den Einrichtungs-Assistenten
                let r = ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    theme::dot(ui, if ok { th.ok } else { th.warn });
                    ui.label(RichText::new(if ok { good } else { bad }).size(12.0).color(th.text2));
                    if !ok {
                        ui.label(RichText::new(crate::i18n::tr("Einrichten…")).size(12.0).color(th.accent_text));
                    }
                });
                let resp = ui.interact(r.response.rect, ui.id().with(("status", good)), egui::Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.on_hover_text(crate::i18n::tr("Einrichtungs-Assistent öffnen")).clicked() {
                    action = Some(StartAction::Setup);
                }
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if loading {
                    ui.spinner();
                } else {
                    theme::dot(ui, if content_ok { th.ok } else { th.warn });
                }
                let r = ui.add(egui::Label::new(RichText::new(content_text).size(12.0).color(th.text2)).sense(egui::Sense::click()));
                let tip = if self.content_status.is_empty() { self.settings.game_dir.clone() } else { self.content_status.clone() };
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.on_hover_text(tip).clicked() {
                    action = Some(if content_ok { StartAction::Settings } else { StartAction::Setup });
                }
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                for (icon, text, act) in [("settings", crate::i18n::tr("Einstellungen"), StartAction::Settings), ("keyboard", crate::i18n::tr("Tastenkürzel"), StartAction::Help), ("book", crate::i18n::tr("Bedienung & Hilfe"), StartAction::Help)] {
                    if link_row(ui, icon, text).clicked() {
                        action = Some(act);
                    }
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(th.bg).inner_margin(egui::Margin { left: 36, right: 36, top: 30, bottom: 24 })).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(if self.settings.recent_files.is_empty() { crate::i18n::tr("Willkommen") } else { crate::i18n::tr("Willkommen zurück") }).font(egui::FontId::new(24.0, theme::bold())));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("Zum Editor")).color(th.text2)).fill(th.panel)).on_hover_text(crate::i18n::tr("Startseite schließen")).clicked() {
                        action = Some(StartAction::Close);
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.dlg.start_filter).hint_text(crate::theme::hint(crate::i18n::tr("Maps und Vorlagen suchen"))).desired_width(280.0));
                });
            });
            ui.add_space(12.0);
            // Wiederherstellung
            if let Some((path, when)) = self.recovery.clone() {
                egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0f32, th.line)).corner_radius(11).inner_margin(egui::Margin::symmetric(14, 12)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 9.0, th.accent_soft);
                        icons::draw(ui.painter(), r.shrink(8.0), "restore", th.accent_text, 1.9);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(crate::i18n::tr("Nicht gespeicherte Änderungen gefunden")).family(theme::semibold()));
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.label(RichText::new(name).font(theme::mono(12.0)).color(th.text2));
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("· automatisch gesichert {when}"))).size(12.5).color(th.text2));
                            });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::accent_button(ui, crate::i18n::tr("Wiederherstellen")).clicked() {
                                action = Some(StartAction::Open(Some(path.clone())));
                            }
                            if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("Verwerfen")).color(th.text2)).fill(Color32::TRANSPARENT)).clicked() {
                                self.recovery = None;
                            }
                        });
                    });
                });
                ui.add_space(14.0);
            }
            // Vorlagen
            let f = self.dlg.start_filter.to_ascii_lowercase();
            ui.label(RichText::new(crate::i18n::tr("Neu aus Vorlage")).family(theme::semibold()).color(th.text2));
            let shown: Vec<usize> = (0..TEMPLATES.len()).filter(|i| f.is_empty() || TEMPLATES[*i].name.to_ascii_lowercase().contains(&f) || TEMPLATES[*i].description.to_ascii_lowercase().contains(&f)).collect();
            let cols = 3usize;
            let gap = 14.0;
            let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
            for row in shown.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for &i in row {
                        if template_card(ui, w, &TEMPLATES[i], &self.template_maps[i]).clicked() {
                            action = Some(StartAction::Template(TEMPLATES[i].id));
                        }
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(14.0);
            // Zuletzt geöffnet
            ui.label(RichText::new(crate::i18n::tr("Zuletzt geöffnet")).family(theme::semibold()).color(th.text2));
            let recent: Vec<String> = self.settings.recent_files.iter().filter(|r| f.is_empty() || r.to_ascii_lowercase().contains(&f)).take(10).cloned().collect();
            if recent.is_empty() {
                ui.label(RichText::new(crate::i18n::tr("Noch keine Maps geöffnet.")).color(th.text3));
            } else {
                egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0f32, th.line)).corner_radius(11).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(ui.available_height().max(120.0)).show(ui, |ui| {
                        // Dateien im Hintergrund lesen (große Maps auf langsamen Platten)
                        let missing: Vec<String> = {
                            let mut m = self.recent_info.lock().unwrap();
                            let miss: Vec<String> = recent.iter().filter(|r| !m.contains_key(*r)).cloned().collect();
                            for r in &miss {
                                m.insert(r.clone(), None);
                            }
                            miss
                        };
                        if !missing.is_empty() {
                            let shared = self.recent_info.clone();
                            let ctx2 = ui.ctx().clone();
                            std::thread::spawn(move || {
                                for r in missing {
                                    let info = file_info(std::path::Path::new(&r));
                                    shared.lock().unwrap().insert(r, Some(info));
                                    ctx2.request_repaint();
                                }
                            });
                        }
                        for (k, r) in recent.iter().enumerate() {
                            let info = self.recent_info.lock().unwrap().get(r).cloned().flatten();
                            if recent_row(ui, r, &info, k + 1 < recent.len()).clicked() {
                                action = Some(StartAction::Open(Some(PathBuf::from(r))));
                            }
                        }
                    });
                });
            }
        });
        match action {
            Some(StartAction::Template(id)) => self.new_from_template(id),
            Some(StartAction::Open(p)) => {
                self.win.start = false;
                let ctx2 = ctx.clone();
                self.request(&ctx2, crate::app::Pending::Open(p));
            }
            Some(StartAction::Collab) => {
                self.win.start = false;
                self.win.collab = true;
            }
            Some(StartAction::Settings) => self.win.settings = true,
            Some(StartAction::Setup) => {
                self.win.setup = true;
                self.setup.page = 0;
            }
            Some(StartAction::Help) => self.win.help = true,
            Some(StartAction::Close) => self.win.start = false,
            None => {}
        }
    }
}

enum StartAction {
    Template(&'static str),
    Open(Option<PathBuf>),
    Collab,
    Settings,
    Setup,
    Help,
    Close,
}

/// Infos zu einer zuletzt geöffneten Datei: (Brushes, Alter in Sekunden), None = fehlt
pub(crate) type RecentInfo = Option<(usize, u64, Vec<[f32; 4]>)>;

fn file_info(p: &std::path::Path) -> RecentInfo {
    let meta = std::fs::metadata(p).ok()?;
    let age = meta.modified().ok().and_then(|m| m.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
    let text = std::fs::read_to_string(p).ok()?;
    let brushes = text.lines().filter(|l| l.trim() == "solid").count();
    Some((brushes, age, mini_plan(&text)))
}

/// Kleine Draufsicht: Grundrisse der Brushes (x0, y0, x1, y1), auf 0..1 normiert (gleicher
/// Maßstab in x und y), höchstens 600 – die größten zuerst
fn mini_plan(vmf: &str) -> Vec<[f32; 4]> {
    let Ok(map) = crate::map::Map::from_vmf(vmf) else { return Vec::new() };
    let mut rects: Vec<crate::geometry::Aabb> = map.all_solids().map(|(_, s)| s.bounds).filter(|b| !b.is_empty()).collect();
    if rects.is_empty() {
        return Vec::new();
    }
    let all = rects.iter().fold(crate::geometry::Aabb::EMPTY, |a, b| a.union(b));
    let size = (all.max.x - all.min.x).max(all.max.y - all.min.y).max(1.0);
    let off = glam::DVec2::new((size - (all.max.x - all.min.x)) / 2.0, (size - (all.max.y - all.min.y)) / 2.0);
    rects.sort_by(|a, b| ((b.max.x - b.min.x) * (b.max.y - b.min.y)).total_cmp(&((a.max.x - a.min.x) * (a.max.y - a.min.y))));
    rects.truncate(600);
    let n = |v: f64, lo: f64, o: f64| ((v - lo + o) / size) as f32;
    // y nach oben (Draufsicht wie in der Ansicht „Oben“)
    rects.iter().map(|b| [n(b.min.x, all.min.x, off.x), 1.0 - n(b.max.y, all.min.y, off.y), n(b.max.x, all.min.x, off.x), 1.0 - n(b.min.y, all.min.y, off.y)]).collect()
}

fn age_text(secs: u64) -> String {
    match secs {
        0..=59 => crate::i18n::tr("gerade eben").into(),
        60..=3599 => crate::i18n::tr_dyn(&format!("vor {} Min.", secs / 60)),
        3600..=86399 => crate::i18n::tr_dyn(&format!("vor {} Std.", secs / 3600)),
        86400..=172799 => crate::i18n::tr("gestern").into(),
        _ => crate::i18n::tr_dyn(&format!("vor {} Tagen", secs / 86400)),
    }
}

/// Neueste Autosave-Datei, die jünger ist als ihre Map (Hinweis auf Absturz/ungespeicherte Arbeit)
fn find_recovery(recent: &[String]) -> Option<(PathBuf, String)> {
    let mut dirs: Vec<PathBuf> = recent.iter().filter_map(|r| std::path::Path::new(r).parent().map(|d| d.join("autosave"))).collect();
    dirs.push(std::env::temp_dir().join("amboss_autosave"));
    dirs.sort();
    dirs.dedup();
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let Some(stem) = name.split("_autosave").next().filter(|_| name.contains("_autosave")) else { continue };
            let Ok(m) = e.metadata().and_then(|m| m.modified()) else { continue };
            let original = d.parent().map(|pd| pd.join(format!("{stem}.vmf")));
            let newer = original.as_ref().and_then(|o| std::fs::metadata(o).ok()).and_then(|om| om.modified().ok()).is_none_or(|om| m > om);
            let recent_enough = m.elapsed().map(|e| e.as_secs() < 7 * 86400).unwrap_or(false);
            if newer && recent_enough && best.as_ref().is_none_or(|b| m > b.1) {
                best = Some((p, m));
            }
        }
    }
    best.map(|(p, m)| (p, age_text(m.elapsed().map(|e| e.as_secs()).unwrap_or(0))))
}

fn side_button(ui: &mut egui::Ui, icon: &str, text: &str, kbd: Option<&str>, primary: bool) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
    let (bg, fg, stroke) = if primary {
        (if resp.hovered() { theme::alpha(th.accent, 0.88) } else { th.accent }, th.accent_fg, Stroke::NONE)
    } else {
        (if resp.hovered() { th.hover } else { th.raised }, th.text, Stroke::new(1.0f32, th.line))
    };
    ui.painter().rect(r, 9.0, bg, stroke, egui::StrokeKind::Inside);
    icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(r.left() + 22.0, r.center().y), egui::vec2(17.0, 17.0)), icon, fg, 2.0);
    let font = if primary { egui::FontId::new(13.5, theme::semibold()) } else { theme::sans(13.5) };
    ui.painter().text(egui::pos2(r.left() + 40.0, r.center().y), egui::Align2::LEFT_CENTER, text, font, fg);
    if let Some(k) = kbd {
        ui.painter().text(egui::pos2(r.right() - 14.0, r.center().y), egui::Align2::RIGHT_CENTER, crate::i18n::keys(k), theme::mono(10.5), if primary { theme::alpha(fg, 0.75) } else { th.text3 });
    }
    resp
}

fn link_row(ui: &mut egui::Ui, icon: &str, text: &str) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 7.0, th.hover);
    }
    icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), egui::vec2(15.0, 15.0)), icon, th.text2, 1.9);
    ui.painter().text(egui::pos2(r.left() + 34.0, r.center().y), egui::Align2::LEFT_CENTER, text, theme::sans(13.0), th.text2);
    resp
}

/// Vorlagenkarte mit Draufsicht der Vorlage in Theme-Farben
fn template_card(ui: &mut egui::Ui, w: f32, tp: &Template, map: &crate::map::Map) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 170.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(r, 11.0, th.panel);
    let pic = egui::Rect::from_min_size(r.min, egui::vec2(w, 104.0));
    p.rect_filled(pic, egui::CornerRadius { nw: 11, ne: 11, sw: 0, se: 0 }, th.vp);
    // Raster
    let step = 13.0;
    let mut x = pic.left() + step;
    while x < pic.right() {
        p.vline(x, pic.y_range(), Stroke::new(1.0f32, th.grid_major));
        x += step;
    }
    let mut y = pic.top() + step;
    while y < pic.bottom() {
        p.hline(pic.x_range(), y, Stroke::new(1.0f32, th.grid_major));
        y += step;
    }
    // Draufsicht
    let mut b = map.all_solids().fold(crate::geometry::Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
    for e in &map.entities {
        if let Some(o) = e.origin() {
            b = b.union(&crate::geometry::Aabb::new(o, o));
        }
    }
    if !b.is_empty() {
        let size = b.size();
        let inner = pic.shrink(14.0);
        let scale = (inner.width() as f64 / size.x.max(1.0)).min(inner.height() as f64 / size.y.max(1.0));
        let c = b.center();
        let to = |v: glam::DVec3| egui::pos2(inner.center().x + ((v.x - c.x) * scale) as f32, inner.center().y - ((v.y - c.y) * scale) as f32);
        let painter = p.with_clip_rect(pic);
        for (_, s) in map.all_solids() {
            let r2 = egui::Rect::from_two_pos(to(s.bounds.min), to(s.bounds.max));
            let sky = s.sides.iter().all(|x| x.material.to_ascii_lowercase().contains("skybox"));
            if sky {
                continue;
            }
            painter.rect(r2, 0.0, theme::alpha(th.brush, 0.05), Stroke::new(1.2f32, th.brush), egui::StrokeKind::Middle);
        }
        for e in &map.entities {
            if let Some(o) = e.origin() {
                let col = if e.classname() == "light_environment" { th.warn } else { th.entity };
                painter.rect_filled(egui::Rect::from_center_size(to(o), egui::vec2(7.0, 7.0)), 2.0, col);
            }
        }
    } else {
        p.vline(pic.center().x, pic.y_range(), Stroke::new(1.0f32, th.axis));
        p.hline(pic.x_range(), pic.center().y, Stroke::new(1.0f32, th.axis));
    }
    let border = if resp.hovered() { Stroke::new(1.0f32, th.accent) } else { Stroke::new(1.0f32, th.line) };
    p.rect_stroke(r, 11.0, border, egui::StrokeKind::Inside);
    if resp.hovered() {
        p.rect_stroke(r.expand(2.0), 13.0, Stroke::new(3.0f32, th.accent_soft), egui::StrokeKind::Outside);
    }
    p.text(egui::pos2(r.left() + 12.0, pic.bottom() + 12.0), egui::Align2::LEFT_TOP, crate::i18n::tr(tp.name), egui::FontId::new(13.0, theme::semibold()), th.text);
    let g = p.layout(crate::i18n::tr(tp.description).to_string(), theme::sans(12.0), th.text3, w - 24.0);
    p.with_clip_rect(r.shrink(1.0)).galley(egui::pos2(r.left() + 12.0, pic.bottom() + 31.0), g, th.text3);
    resp
}

fn recent_row(ui: &mut egui::Ui, path: &str, info: &Option<RecentInfo>, divider: bool) -> egui::Response {
    let th = theme::t();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 0.0, th.hover);
    }
    if divider {
        p.hline(r.x_range(), r.bottom(), Stroke::new(1.0f32, th.line));
    }
    let tile = egui::Rect::from_min_size(egui::pos2(r.left() + 14.0, r.center().y - 18.0), egui::vec2(52.0, 36.0));
    p.rect(tile, 6.0, th.vp, Stroke::new(1.0f32, th.line), egui::StrokeKind::Inside);
    match info {
        // Mini-Draufsicht der Map
        Some(Some((_, _, plan))) if !plan.is_empty() => {
            let inner = tile.shrink(3.0);
            let side = inner.width().min(inner.height());
            let sq = egui::Rect::from_center_size(inner.center(), egui::vec2(side, side));
            let pc = p.with_clip_rect(tile.shrink(1.0));
            for q in plan.iter().rev() {
                let rr = egui::Rect::from_min_max(egui::pos2(sq.left() + q[0] * side, sq.top() + q[1] * side), egui::pos2(sq.left() + q[2] * side, sq.top() + q[3] * side));
                let rr = if rr.width() < 1.0 || rr.height() < 1.0 { rr.expand(0.5) } else { rr };
                pc.rect_stroke(rr, 0.0, Stroke::new(0.7f32, th.text2), egui::StrokeKind::Inside);
            }
        }
        _ => {
            let mut x = tile.left() + 9.0;
            while x < tile.right() {
                p.vline(x, tile.shrink(1.0).y_range(), Stroke::new(1.0f32, th.grid_major));
                x += 9.0;
            }
            let mut y = tile.top() + 9.0;
            while y < tile.bottom() {
                p.hline(tile.shrink(1.0).x_range(), y, Stroke::new(1.0f32, th.grid_major));
                y += 9.0;
            }
        }
    }
    let pp = std::path::Path::new(path);
    let name = pp.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let dir = pp.parent().map(|d| d.display().to_string()).unwrap_or_default();
    let exists = !matches!(info, Some(None));
    p.text(egui::pos2(tile.right() + 14.0, r.center().y - 9.0), egui::Align2::LEFT_CENTER, name, theme::mono(12.5), if exists { th.text } else { th.text3 });
    let right_w = 250.0;
    let dir_clip = egui::Rect::from_min_max(egui::pos2(tile.right() + 14.0, r.top()), egui::pos2(r.right() - right_w, r.bottom()));
    p.with_clip_rect(dir_clip).text(egui::pos2(tile.right() + 14.0, r.center().y + 10.0), egui::Align2::LEFT_CENTER, dir, theme::sans(12.0), th.text3);
    let (stats, when) = match info {
        Some(Some((b, age, _))) => (format!("{} Brushes", crate::chrome::fmt_thousands(*b)), age_text(*age)),
        Some(None) => ("fehlt".to_string(), String::new()),
        None => ("…".to_string(), String::new()),
    };
    p.text(egui::pos2(r.right() - 134.0, r.center().y), egui::Align2::RIGHT_CENTER, stats, theme::mono(11.5), th.text3);
    p.text(egui::pos2(r.right() - 14.0, r.center().y), egui::Align2::RIGHT_CENTER, when, theme::sans(12.0), th.text2);
    resp.on_hover_text(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;

    #[test]
    fn all_templates_build_and_save() {
        for t in TEMPLATES {
            let mut ed = Editor::new(Fgd::builtin());
            assert!(apply(&mut ed, t.id));
            assert!(!ed.can_undo(), "{}", t.id);
            assert!(ed.map.all_solids().all(|(_, s)| s.is_valid()), "{}", t.id);
            crate::map::Map::from_vmf(&ed.map.to_vmf()).unwrap();
            if t.id != "leer" {
                assert!(ed.map.entities.iter().any(|e| e.classname() == "info_player_start"), "{}", t.id);
                // abgedichtet?
                let r = crate::optimize::run(&mut crate::optimize::build_copy(&ed, ed.map.clone()), &crate::optimize::OptSettings { ai_nodes: false, cubemaps: false, ..Default::default() }, None, None);
                assert!(r.leak.is_none(), "{} hat ein Leck", t.id);
            }
        }
    }
}
