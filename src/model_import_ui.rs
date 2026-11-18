//! Dialog „Modell importieren“ (glTF/GLB, OBJ) mit Vorschau, Kompilieren über studiomdl und Platzieren.

use crate::app::App;
use crate::i18n::tr;
use crate::model_import::{self, ImportOpts, Model, METER};
use eframe::egui::{self, RichText, Vec2};
use glam::DVec3;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) struct ImportJob {
    done: Arc<Mutex<Option<bool>>>,
    opts: ImportOpts,
    pos: DVec3,
}

#[derive(Default)]
pub(crate) struct ImportUi {
    pub open: bool,
    pub path: Option<PathBuf>,
    pub model: Option<Arc<Model>>,
    pub opts: ImportOpts,
    /// 0 = Meter, 1 = Zentimeter, 2 = Zoll/Source-Einheiten, 3 = frei
    pub unit: u8,
    pub log: Arc<Mutex<Vec<String>>>,
    pub job: Option<ImportJob>,
    /// Ziel des neuen Props
    pub pos: DVec3,
}

const UNITS: [(f64, &str); 3] = [(METER, "Meter"), (METER / 100.0, "Zentimeter"), (1.0, "Zoll / Source-Einheiten")];

impl App {
    /// Datei laden und den Dialog öffnen (Menü, Drag & Drop, MCP)
    pub(crate) fn open_model_import(&mut self, path: PathBuf) {
        self.imp.log.lock().unwrap().clear();
        match model_import::load(&path) {
            Ok(m) => {
                let mut o = model_import::suggest(&m, &path);
                if !self.imp.opts.folder.is_empty() {
                    o.folder = self.imp.opts.folder.clone();
                }
                o.tex_max = if self.imp.opts.tex_max == 0 { 1024 } else { self.imp.opts.tex_max };
                self.imp.unit = UNITS.iter().position(|u| (u.0 - o.scale).abs() < 1e-9).unwrap_or(3) as u8;
                self.imp.opts = o;
                self.imp.model = Some(Arc::new(m));
            }
            Err(e) => {
                self.imp.model = None;
                self.imp.log.lock().unwrap().push(e);
            }
        }
        // Ziel: wo die 3D-Kamera hinschaut (sonst 256 Einheiten vor ihr)
        let o = self.view3d.cam.pos.as_dvec3();
        let d = self.view3d.cam.forward().as_dvec3();
        let hit = self.ed.pick_3d(o, d).map(|h| o + d * h.2).filter(|p| p.distance(o) < 4096.0);
        self.imp.pos = crate::geometry::snap_vec_grid(hit.unwrap_or(o + d * 256.0), self.ed.grid.max(1.0));
        self.imp.path = Some(path);
        self.imp.open = true;
    }

    pub(crate) fn pick_model_file(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("glTF, OBJ", &["glb", "gltf", "obj"]).pick_file() {
            self.open_model_import(p);
        }
    }

    /// SMD/QC, Materialien schreiben und (optional) studiomdl starten
    pub(crate) fn import_run(&mut self, compile: bool) -> Result<(usize, DVec3), String> {
        let Some(m) = self.imp.model.clone() else { return Err("Kein Modell geladen".into()) };
        let o = self.imp.opts.clone();
        let log = self.imp.log.clone();
        log.lock().unwrap().clear();
        let say = |s: String| log.lock().unwrap().push(s);
        let gd = self.settings.game_dir.trim().to_string();
        if compile && gd.is_empty() {
            let e = tr("Kein Spielverzeichnis eingestellt (Einstellungen → Spiel).").to_string();
            say(e.clone());
            return Err(e);
        }
        let base = self.ed.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(std::env::temp_dir);
        let dir = base.join("amboss_models").join(&o.name);
        let ex = model_import::export(&m, &o, &dir, (!gd.is_empty()).then(|| Path::new(&gd)))?;
        say(crate::i18n::tr_dyn(&format!("SMD/QC geschrieben: {}", ex.dir.display())));
        if !gd.is_empty() {
            say(crate::i18n::tr_dyn(&format!("Materialien in materials/{}", model_import::mat_dir(&o))));
        }
        for n in &ex.notes {
            say(crate::i18n::tr_dyn(n));
        }
        if !compile {
            return Ok((ex.tris, ex.size));
        }
        let Some(exe) = crate::propper::find_studiomdl(&self.settings.vbsp) else {
            let e = tr("studiomdl wurde nicht neben vbsp gefunden – die QC liegt bereit.").to_string();
            say(e.clone());
            return Err(e);
        };
        let done = crate::propper::spawn_studiomdl(exe, gd, ex.qc.clone(), log.clone());
        self.imp.job = Some(ImportJob { done, opts: o, pos: self.imp.pos });
        Ok((ex.tris, ex.size))
    }

    /// Fertige Kompilierung übernehmen: Prop setzen
    pub(crate) fn poll_import(&mut self) {
        let Some(job) = &self.imp.job else { return };
        let Some(ok) = *job.done.lock().unwrap() else {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        let job = self.imp.job.take().unwrap();
        let model = format!("models/{}/{}.mdl", job.opts.folder, job.opts.name);
        if ok {
            self.models.forget(&model);
            self.ed.checkpoint();
            let id = self.ed.map.alloc_id();
            let mut e = crate::map::Entity::new(id, if job.opts.is_static { "prop_static" } else { "prop_physics" });
            e.set("model", model.clone());
            e.set("origin", crate::map::fmt_vec(job.pos));
            e.set("angles", "0 0 0");
            self.ed.map.entities.push(e.into());
            self.ed.sel = [crate::editor::Obj::Entity(id)].into_iter().collect();
            self.ed.touch();
            self.imp.log.lock().unwrap().push(crate::i18n::tr_dyn(&format!("Fertig: {model}")));
            self.ed.status = crate::i18n::tr_dyn(&format!("Modell importiert: {model}"));
            self.imp.open = false;
        } else {
            self.imp.log.lock().unwrap().push(tr("studiomdl ist fehlgeschlagen.").into());
            self.ed.status = tr("Modell-Import fehlgeschlagen (siehe Log)").into();
        }
    }

    pub(crate) fn import_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        self.poll_import();
        if !self.imp.open {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let (mut run, mut only_export, mut other) = (false, false, false);
        let busy = self.imp.job.is_some();
        let studiomdl = crate::propper::find_studiomdl(&self.settings.vbsp);
        let log: Vec<String> = self.imp.log.lock().unwrap().clone();
        let file = self.imp.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let model = self.imp.model.clone();
        let d = theme::Dlg::new(tr("Modell importieren"), 720.0).subtitle(file).footer();
        let imp = &mut self.imp;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.add_space(10.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                    let Some(m) = &model else {
                        ui.label(RichText::new(tr("Datei konnte nicht gelesen werden.")).color(th.err));
                        return;
                    };
                    let o = &mut imp.opts;
                    let (tris, b) = model_import::converted(m, o);
                    ui.horizontal_top(|ui| {
                        // links: Einstellungen
                        ui.vertical(|ui| {
                            ui.set_width(400.0);
                            theme::field_row(ui, "Name", |ui| ui.add(egui::TextEdit::singleline(&mut o.name).font(theme::mono(11.5)).desired_width(220.0)));
                            theme::field_row(ui, "Ordner unter models/", |ui| ui.add(egui::TextEdit::singleline(&mut o.folder).font(theme::mono(11.5)).desired_width(220.0)));
                            theme::field_row(ui, "Einheit in der Datei", |ui| {
                                let label = if imp.unit < 3 { tr(UNITS[imp.unit as usize].1).to_string() } else { tr("frei").to_string() };
                                egui::ComboBox::from_id_salt("imp_unit").selected_text(label).width(220.0).show_ui(ui, |ui| {
                                    for (i, (f, n)) in UNITS.iter().enumerate() {
                                        if ui.selectable_label(imp.unit == i as u8, tr(n)).clicked() {
                                            imp.unit = i as u8;
                                            o.scale = *f;
                                        }
                                    }
                                    if ui.selectable_label(imp.unit == 3, tr("frei")).clicked() {
                                        imp.unit = 3;
                                    }
                                })
                            });
                            if imp.unit == 3 {
                                theme::field_row(ui, "Faktor", |ui| ui.add(theme::dv(egui::DragValue::new(&mut o.scale).range(0.001..=10000.0).speed(0.05))));
                            }
                            theme::field_row(ui, "Oben ist", |ui| {
                                ui.radio_value(&mut o.y_up, true, tr("Y (Blender, glTF)"));
                                ui.radio_value(&mut o.y_up, false, "Z");
                            });
                            theme::field_row(ui, "Drehen", |ui| {
                                for deg in [0.0, 90.0, 180.0, 270.0] {
                                    if ui.selectable_label(o.yaw == deg, format!("{deg}°")).clicked() {
                                        o.yaw = deg;
                                    }
                                }
                            });
                            theme::field_row(ui, "Typ", |ui| {
                                ui.radio_value(&mut o.is_static, true, "prop_static");
                                ui.radio_value(&mut o.is_static, false, "prop_physics");
                            });
                            if !o.is_static {
                                theme::field_row(ui, "Masse", |ui| ui.add(theme::dv(egui::DragValue::new(&mut o.mass)).range(1.0..=50000.0).speed(1.0).suffix(" kg")));
                            }
                            theme::field_row(ui, "Oberfläche ($surfaceprop)", |ui| {
                                egui::ComboBox::from_id_salt("imp_surf").selected_text(o.surfaceprop.clone()).width(220.0).show_ui(ui, |ui| {
                                    for s in crate::propper::SURFACEPROPS {
                                        ui.selectable_value(&mut o.surfaceprop, s.to_string(), *s);
                                    }
                                })
                            });
                            theme::field_row(ui, "Texturen höchstens", |ui| {
                                for px in [512u32, 1024, 2048] {
                                    if ui.selectable_label(o.tex_max == px, format!("{px} px")).clicked() {
                                        o.tex_max = px;
                                    }
                                }
                            });
                            theme::field_row(ui, "Kollision", |ui| crate::theme::checkbox(ui, &mut o.collision, tr("Konvexe Hülle (für Spieler und Physik)")));
                            o.name = o.name.trim().to_ascii_lowercase().replace([' ', '\\', '/'], "_");
                            o.folder = o.folder.trim().trim_matches('/').replace('\\', "/");
                        });
                        // rechts: Vorschau und Daten
                        ui.vertical(|ui| {
                            let s = b.size();
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Dreiecke · {} Materialien", tris.len(), m.materials.len()))).color(th.text2));
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Größe {} × {} × {} Einheiten", crate::i18n::dec(crate::map::fmt_num(s.x.round())), crate::i18n::dec(crate::map::fmt_num(s.y.round())), crate::i18n::dec(crate::map::fmt_num(s.z.round()))))).color(th.text));
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("≈ {} m hoch (Spieler: 72 Einheiten ≈ 1,83 m)", crate::i18n::dec(format!("{:.2}", s.z / METER))))).size(11.5).color(th.text3));
                            preview(ui, &tris, b, &th);
                            ui.add_space(4.0);
                            for mat in &m.materials {
                                let tex = match &mat.image {
                                    Some(model_import::Image::File(p)) if p.is_file() => p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
                                    Some(model_import::Image::File(p)) => crate::i18n::tr_dyn(&format!("fehlt: {}", p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default())),
                                    Some(model_import::Image::Bytes(_, n)) => crate::i18n::tr_dyn(&format!("eingebettet ({n})")),
                                    None => tr("nur Farbe").to_string(),
                                };
                                let missing = matches!(&mat.image, Some(model_import::Image::File(p)) if !p.is_file());
                                ui.label(RichText::new(format!("{} – {tex}", mat.name)).size(11.5).color(if missing { th.warn } else { th.text3 }));
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let info = match &studiomdl {
                        Some(p) => crate::i18n::tr_dyn(&format!("studiomdl: {}", p.display())),
                        None => tr("studiomdl nicht gefunden (wird neben vbsp gesucht) – nur Export möglich").to_string(),
                    };
                    ui.label(RichText::new(info).color(th.text3).size(11.5));
                });
                if !log.is_empty() {
                    egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(8).outer_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(140.0).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
                            for l in &log {
                                let c = if l.to_ascii_uppercase().starts_with("ERROR") || l.contains("fehlgeschlagen") || l.contains("failed") { th.err } else { th.text2 };
                                ui.label(RichText::new(crate::i18n::tr_dyn(l)).font(theme::mono(11.0)).color(c));
                            }
                        });
                    });
                }
            }
            theme::Part::Footer => {
                let ok = model.is_some() && !busy && !imp.opts.name.is_empty();
                let label = if busy { tr("Kompiliere…") } else { tr("Importieren und platzieren") };
                if ui.add_enabled(ok && studiomdl.is_some(), egui::Button::new(RichText::new(label).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    run = true;
                }
                if ui.add_enabled(ok, egui::Button::new(tr("Nur SMD/QC/Texturen")).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    only_export = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if theme::secondary_button(ui, tr("Andere Datei…")).clicked() {
                        other = true;
                    }
                });
            }
            _ => {}
        });
        if run || only_export {
            if let Err(e) = self.import_run(run) {
                self.ed.status = crate::i18n::tr_dyn(&e);
            }
        }
        if other {
            self.pick_model_file();
        }
        if !open {
            self.imp.open = false;
        }
    }
}

/// Seitenansicht (vorn) und Draufsicht als Drahtgitter, höchstens 6000 Dreiecke
fn preview(ui: &mut egui::Ui, tris: &[([model_import::Vert; 3], usize)], b: crate::geometry::Aabb, th: &crate::theme::Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(250.0, 150.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 6.0, th.vp);
    let halves = [egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y)), egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max)];
    let s = b.size();
    let step = (tris.len() / 6000).max(1);
    for (k, r) in halves.iter().enumerate() {
        let r = r.shrink(10.0);
        // vorn: X/Z, oben: X/Y
        let (w, h) = if k == 0 { (s.x, s.z) } else { (s.x, s.y) };
        let sc = (r.width() as f64 / w.max(1e-6)).min(r.height() as f64 / h.max(1e-6)) as f32;
        let map = |v: DVec3| -> egui::Pos2 {
            let (x, y) = if k == 0 { (v.x - b.center().x, v.z - b.center().z) } else { (v.x - b.center().x, v.y - b.center().y) };
            r.center() + Vec2::new(x as f32 * sc, -(y as f32) * sc)
        };
        let col = crate::theme::alpha(th.accent, 0.55);
        for (t, _) in tris.iter().step_by(step) {
            let q = t.map(|v| map(v.p));
            p.line_segment([q[0], q[1]], egui::Stroke::new(0.6f32, col));
            p.line_segment([q[1], q[2]], egui::Stroke::new(0.6f32, col));
        }
        p.text(r.left_top() - Vec2::new(4.0, 6.0), egui::Align2::LEFT_TOP, if k == 0 { tr("vorn") } else { tr("oben") }, egui::FontId::proportional(10.0), th.text3);
    }
}
