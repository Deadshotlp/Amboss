//! Fenster „Polygon-Mesh / Bezier-Patch“ und Kompilieren der Meshes zum Modell (studiomdl).

use crate::editor::Obj;
use crate::mesh_edit::MeshOp;
use eframe::egui::{self, RichText, Vec2};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct MeshUi {
    pub extrude: f64,
    /// OBJ-Maßstab: 0 = Source-Einheiten (Zoll), 1 = Meter, 2 = Zentimeter
    pub obj_unit: usize,
    pub y_up: bool,
    pub log: Arc<Mutex<Vec<String>>>,
    pub job: Option<(Arc<Mutex<Option<bool>>>, String)>,
}

impl Default for MeshUi {
    fn default() -> Self {
        MeshUi { extrude: 32.0, obj_unit: 1, y_up: true, log: Default::default(), job: None }
    }
}

/// Einheiten je OBJ-Einheit (Source: 1 Zoll = 1 Einheit, 1 m ≈ 39,37)
const OBJ_SCALE: [f64; 3] = [1.0, 39.37, 0.3937];

enum Act {
    FromBrushes,
    Import,
    Edit(u32),
    Op(MeshOp),
    Paint,
    FacesMode(bool),
    Done,
    PatchBegin,
    PatchFlat,
    Compile(u32),
}

impl crate::app::App {
    pub(crate) fn mesh_window(&mut self, ctx: &egui::Context) {
        use crate::i18n::{tr, tr_dyn};
        use crate::theme;
        if !self.win.mesh {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut acts: Vec<Act> = Vec::new();
        let has_brush = self.ed.sel.iter().any(|o| matches!(o, Obj::Solid(_)));
        let has_face = !self.ed.faces.is_empty();
        let sel_mesh = self.ed.selected_mesh();
        let editing = self.ed.mesh_edit.clone();
        let patching = self.ed.patch_edit.as_ref().map(|p| p.sel.len());
        let stats = editing.as_ref().and_then(|m| self.ed.mesh_of(m.ent)).map(|m| (m.mesh.verts.len(), m.mesh.faces.len(), m.name.clone()));
        let target = editing.as_ref().map(|m| m.ent).or(sel_mesh);
        let target_name = target.and_then(|t| self.ed.mesh_of(t)).map(|m| m.name.clone());
        let compiled = target_name.as_ref().is_some_and(|n| {
            let gd = self.settings.game_dir.trim();
            !gd.is_empty() && Path::new(gd).join(format!("models/amboss/meshes/{n}.mdl")).is_file()
        });
        let busy = self.mesh_ui.job.is_some();
        let log: Vec<String> = self.mesh_ui.log.lock().unwrap().iter().rev().take(5).rev().cloned().collect();
        let mut page = self.mesh_page;
        let d = theme::Dlg::new(tr("Polygon-Mesh / Bezier-Patch"), 480.0).footer();
        let o = &mut self.mesh_ui;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                page = theme::segmented(ui, page, &[tr("Polygon-Mesh"), tr("Bezier-Patch")], true);
                ui.add_space(6.0);
                let btn = |ui: &mut egui::Ui, on: bool, label: &str, tip: &str| ui.add_enabled(on, egui::Button::new(tr_dyn(label)).min_size(Vec2::new(0.0, 28.0))).on_hover_text(tr_dyn(tip)).on_disabled_hover_text(tr_dyn(tip)).clicked();
                if page == 0 {
                    match (&editing, &stats) {
                        (Some(me), Some((nv, nf, name))) => {
                            ui.label(RichText::new(tr_dyn(&format!("Bearbeite {name}: {} · {} · {} gewählt", crate::i18n::count_de(*nv, "Eckpunkt", "Eckpunkte"), crate::i18n::count_de(*nf, "Fläche", "Flächen"), me.sel.len()))).color(th.text2));
                            ui.add_space(4.0);
                            let cur = me.faces_mode as usize;
                            let m = theme::segmented(ui, cur, &[tr("Punkte"), tr("Flächen")], false);
                            if m != cur {
                                acts.push(Act::FacesMode(m == 1));
                            }
                            theme::section(ui, tr("Werkzeuge"));
                            let any = !me.sel.is_empty();
                            ui.horizontal(|ui| {
                                if btn(ui, any, "Extrudieren", "Gewählte Flächen um den Abstand herausziehen (neue Seitenflächen)") {
                                    acts.push(Act::Op(MeshOp::Extrude(o.extrude)));
                                }
                                ui.add(theme::dv(egui::DragValue::new(&mut o.extrude).range(-4096.0..=4096.0).speed(1.0).suffix(" u")));
                            });
                            ui.horizontal_wrapped(|ui| {
                                if btn(ui, true, "Glätten", "Punkte zum Mittel ihrer Nachbarn ziehen (ohne Auswahl: alle)") {
                                    acts.push(Act::Op(MeshOp::Smooth));
                                }
                                if btn(ui, true, "Unterteilen", "Jede Fläche in Vierecke teilen (für feinere Formen)") {
                                    acts.push(Act::Op(MeshOp::Subdivide));
                                }
                                if btn(ui, any, "Umdrehen", "Vorder- und Rückseite der Flächen tauschen") {
                                    acts.push(Act::Op(MeshOp::Flip));
                                }
                                if btn(ui, any, "Löschen", "Gewählte Flächen entfernen (Entf)") {
                                    acts.push(Act::Op(MeshOp::Delete));
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                if btn(ui, true, "Alles auswählen", "Alle Punkte bzw. Flächen (Strg+A)") {
                                    acts.push(Act::Op(MeshOp::SelectAll));
                                }
                                if btn(ui, true, "Aktuelle Textur zuweisen", "Auf die gewählten Flächen, ohne Auswahl auf das ganze Mesh") {
                                    acts.push(Act::Paint);
                                }
                            });
                            ui.add_space(4.0);
                            if theme::accent_button(ui, "Fertig").clicked() {
                                acts.push(Act::Done);
                            }
                        }
                        _ => {
                            ui.label(RichText::new(tr("Freie Netze mit beliebiger Form. Sie gehören zu einem prop_static und werden beim Kompilieren zum Modell; die Netzdaten liegen neben der VMF in <map>.meshes.json.")).color(th.text2));
                            ui.add_space(6.0);
                            if let Some(id) = sel_mesh {
                                if theme::accent_button(ui, "Ausgewähltes Mesh bearbeiten").clicked() {
                                    acts.push(Act::Edit(id));
                                }
                            }
                            if btn(ui, has_brush, "Aus ausgewählten Brushes erstellen", "Die Brushes werden durch ein Mesh ersetzt (Displacements als Raster)") {
                                acts.push(Act::FromBrushes);
                            }
                            theme::section(ui, tr("OBJ importieren"));
                            ui.horizontal(|ui| {
                                ui.label(tr("Einheit der Datei"));
                                o.obj_unit = theme::segmented(ui, o.obj_unit, &[tr("Source-Einheiten"), tr("Meter"), tr("Zentimeter")], false);
                            });
                            theme::checkbox(ui, &mut o.y_up, tr("Y zeigt nach oben (Blender-Export)"));
                            if btn(ui, true, "OBJ-Datei wählen…", "Wavefront OBJ mit Flächen (v, f, usemtl); eingefügt dort, wohin die 3D-Kamera blickt") {
                                acts.push(Act::Import);
                            }
                        }
                    }
                } else {
                    ui.label(RichText::new(tr("Ein Bezier-Patch biegt eine Displacement-Fläche mit 16 Kontrollpunkten glatt – für Bögen, Wellen, Röhrenstücke und Kuppeln. Das Ergebnis ist ein normales Displacement.")).color(th.text2));
                    ui.add_space(6.0);
                    match patching {
                        Some(n) => {
                            ui.label(tr_dyn(&format!("{n} von 16 Kontrollpunkten gewählt")));
                            ui.horizontal(|ui| {
                                if btn(ui, true, "Flach zurücksetzen", "Alle Punkte auf die Ebene der vier Ecken") {
                                    acts.push(Act::PatchFlat);
                                }
                                if theme::accent_button(ui, "Fertig").clicked() {
                                    acts.push(Act::Done);
                                }
                            });
                        }
                        None => {
                            if btn(ui, has_face || has_brush, "Patch auf Auswahl beginnen", "Gewählte Fläche (Flächen-Werkzeug) oder Oberseite des gewählten Brushes; ohne Displacement wird eines angelegt") {
                                acts.push(Act::PatchBegin);
                            }
                        }
                    }
                }
                if page == 0 {
                    theme::section(ui, tr("Modell"));
                    match &target_name {
                        Some(n) => {
                            ui.label(RichText::new(format!("models/amboss/meshes/{n}.mdl")).font(theme::mono(11.5)).color(th.text2));
                            ui.label(RichText::new(if compiled { tr("Modell vorhanden – nach Änderungen neu kompilieren.") } else { tr("Noch nicht kompiliert: vor dem Kompilieren der Map nötig.") }).size(12.0).color(if compiled { th.text3 } else { th.warn }));
                            if btn(ui, !busy && target.is_some(), if busy { "Kompiliert …" } else { "Zum Modell kompilieren" }, "SMD/QC schreiben und studiomdl starten (Spielverzeichnis aus den Einstellungen)") {
                                acts.push(Act::Compile(target.unwrap()));
                            }
                        }
                        None => {
                            ui.label(RichText::new(tr("Kein Mesh gewählt")).size(12.0).color(th.text3));
                        }
                    }
                    for l in &log {
                        ui.label(RichText::new(l).font(theme::mono(10.5)).color(th.text3));
                    }
                }
            }
            theme::Part::Footer => {
                theme::footer_hint(ui, tr("Griffe in der 3D-Ansicht ziehen · Shift fügt hinzu · Esc beendet"));
            }
            _ => {}
        });
        self.mesh_page = page;
        self.win.mesh = open;
        if !open {
            self.ed.grip_end();
        }
        for a in acts {
            match a {
                Act::FromBrushes => {
                    if let Err(e) = self.ed.mesh_from_selection() {
                        self.ed.status = e;
                    }
                }
                Act::Import => self.mesh_import_dialog(),
                Act::Edit(id) => self.ed.mesh_begin(id),
                Act::Op(op) => self.ed.mesh_op(op),
                Act::Paint => self.ed.mesh_paint(),
                Act::FacesMode(f) => {
                    if let Some(m) = &mut self.ed.mesh_edit {
                        m.faces_mode = f;
                        m.sel.clear();
                    }
                }
                Act::Done => self.ed.grip_end(),
                Act::PatchBegin => {
                    if let Err(e) = self.ed.patch_begin() {
                        self.ed.status = e;
                    }
                }
                Act::PatchFlat => self.ed.patch_reset(),
                Act::Compile(id) => self.mesh_compile(id),
            }
            self.ed.touch();
        }
    }

    fn mesh_import_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Wavefront OBJ", &["obj"]).pick_file() else { return };
        let src = match std::fs::read(&p) {
            Ok(b) => String::from_utf8_lossy(&b).to_string(),
            Err(e) => {
                self.ed.status = format!("{}: {e}", p.display());
                return;
            }
        };
        let c = &self.view3d.cam;
        let (o, d) = (c.pos.as_dvec3(), c.forward().as_dvec3());
        let at = self.ed.surface_at(o, d).map(|h| h.0).unwrap_or(o + d * 256.0);
        let g = self.ed.grid.max(1.0);
        let at = glam::DVec3::new((at.x / g).round() * g, (at.y / g).round() * g, at.z.round());
        let scale = OBJ_SCALE[self.mesh_ui.obj_unit.min(2)];
        match self.ed.mesh_import(&src, scale, self.mesh_ui.y_up, at) {
            Ok(id) => {
                let n = self.ed.mesh_of(id).map(|m| m.mesh.faces.len()).unwrap_or(0);
                self.ed.status = crate::i18n::tr_dyn(&format!("OBJ importiert: {}", crate::i18n::count_de(n, "Fläche", "Flächen")));
            }
            Err(e) => self.ed.status = e,
        }
    }

    /// Mesh als SMD/QC schreiben und mit studiomdl kompilieren
    pub(crate) fn mesh_compile(&mut self, ent: u32) {
        use crate::propper::{find_studiomdl, mat_dir, qc, smd, spawn_studiomdl, write_vmts, PropOpts};
        let Some(mo) = self.ed.mesh_of(ent).cloned() else { return };
        let log = self.mesh_ui.log.clone();
        log.lock().unwrap().clear();
        let say = |s: String| log.lock().unwrap().push(s);
        let o = PropOpts { name: mo.name.clone(), folder: "amboss/meshes".into(), surfaceprop: "default".into(), is_static: true, mass: 1.0, replace: false };
        let base = self.ed.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(std::env::temp_dir);
        let dir = base.join("amboss_models").join("meshes").join(&mo.name);
        let mats = &self.mats;
        let tris = mo.mesh.smd_triangles(&|m| mats.info(m).map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0)));
        if tris.is_empty() {
            return say("Das Mesh hat keine Flächen.".into());
        }
        let write = || -> Result<std::path::PathBuf, String> {
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(dir.join(format!("{}_ref.smd", o.name)), smd(&tris)).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{}_phys.smd", o.name)), smd(&tris)).map_err(|e| e.to_string())?;
            let q = dir.join(format!("{}.qc", o.name));
            std::fs::write(&q, qc(&o, &[mat_dir(&o)])).map_err(|e| e.to_string())?;
            Ok(q)
        };
        let qcp = match write() {
            Ok(q) => q,
            Err(e) => return say(e),
        };
        say(format!("SMD/QC: {}", dir.display()));
        let gd = self.settings.game_dir.trim().to_string();
        if gd.is_empty() {
            return say("Kein Spielverzeichnis eingestellt (Einstellungen → Spiel).".into());
        }
        let list: Vec<String> = mo.mesh.faces.iter().map(|f| f.material.clone()).collect();
        if let Err(e) = write_vmts(list, &Path::new(&gd).join("materials").join(mat_dir(&o)), self.mats.fs().as_deref()) {
            return say(e);
        }
        let Some(exe) = find_studiomdl(&self.settings.vbsp) else {
            return say("studiomdl wurde nicht neben vbsp gefunden – die QC liegt bereit.".into());
        };
        let done = spawn_studiomdl(exe, gd, qcp, log.clone());
        self.mesh_ui.job = Some((done, format!("models/amboss/meshes/{}.mdl", mo.name)));
    }

    pub(crate) fn poll_mesh_job(&mut self) {
        let Some((done, model)) = &self.mesh_ui.job else { return };
        let Some(ok) = *done.lock().unwrap() else {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        let model = model.clone();
        self.mesh_ui.job = None;
        if ok {
            self.models.forget(&model);
            self.mesh_ui.log.lock().unwrap().push(format!("Fertig: {model}"));
            self.ed.status = format!("Modell kompiliert: {model}");
        } else {
            self.mesh_ui.log.lock().unwrap().push("studiomdl ist fehlgeschlagen.".into());
            self.ed.status = "Modell-Kompilierung fehlgeschlagen".into();
        }
    }
}
