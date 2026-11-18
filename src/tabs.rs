//! Mehrere Maps in Tabs, Import/Export, MCP-Anbindung des laufenden Editors.

use crate::app::{App, DocSlot, Pending};
use crate::commands::{display_shortcut, Cmd};
use crate::editor::Editor;
use crate::mcp::{self, Host};
use eframe::egui::{self, Color32, RichText};
use glam::DVec2;
use serde_json::Value;
use std::path::PathBuf;

impl App {
    fn current_slot_view(&self) -> ((glam::Vec3, f32, f32), [(DVec2, f64); 3]) {
        let c = &self.view3d.cam;
        ((c.pos, c.yaw, c.pitch), [0, 1, 2].map(|i| (self.views2d[i].center, self.views2d[i].zoom)))
    }

    pub(crate) fn switch_tab(&mut self, i: usize) {
        if i == self.active_tab || i >= self.tabs.len() {
            return;
        }
        let Some(target) = self.tabs[i].take() else { return };
        let (cam, views) = self.current_slot_view();
        let old = std::mem::replace(&mut self.ed, target.ed);
        self.tabs[self.active_tab] = Some(DocSlot { ed: old, cam, views });
        self.active_tab = i;
        self.view3d.cam.pos = target.cam.0;
        self.view3d.cam.yaw = target.cam.1;
        self.view3d.cam.pitch = target.cam.2;
        for (k, (c, z)) in target.views.iter().enumerate() {
            self.views2d[k].center = *c;
            self.views2d[k].zoom = *z;
        }
        self.view3d.invalidate();
        self.ed.touch();
        self.problems.clear();
    }

    pub(crate) fn new_tab(&mut self) {
        let mut ed = Editor::new(self.ed.fgd.clone());
        ed.tex_lock = self.settings.tex_lock;
        ed.grid = self.settings.grid;
        ed.model_bounds = self.ed.model_bounds.clone();
        ed.model_tris = self.ed.model_tris.clone();
        ed.model_edges = self.ed.model_edges.clone();
        let (cam, _) = self.current_slot_view();
        let views = [(DVec2::ZERO, 0.5), (DVec2::new(0.0, 128.0), 0.5), (DVec2::new(0.0, 128.0), 0.5)];
        self.tabs.push(Some(DocSlot { ed, cam, views }));
        let n = self.tabs.len() - 1;
        self.switch_tab(n);
    }

    /// Leere, unbenutzte Map? Dann kann sie ersetzt werden statt einen neuen Tab zu öffnen.
    pub(crate) fn is_pristine_pub(&self) -> bool {
        self.is_pristine()
    }

    fn is_pristine(&self) -> bool {
        self.ed.path.is_none() && !self.ed.dirty && self.ed.map.world.solids.is_empty() && self.ed.map.entities.is_empty()
    }

    pub(crate) fn open_in_tab(&mut self, path: PathBuf) {
        // kompilierte Map: erst dekompilieren
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")) {
            self.win.start = false;
            self.ask_decompile(path);
            return;
        }
        // schon offen? → dorthin wechseln
        if self.ed.path.as_ref() == Some(&path) {
            return;
        }
        if let Some(i) = self.tabs.iter().position(|t| t.as_ref().is_some_and(|t| t.ed.path.as_ref() == Some(&path))) {
            self.switch_tab(i);
            return;
        }
        if !self.is_pristine() {
            self.new_tab();
        }
        self.open_path(path);
    }

    pub(crate) fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        self.switch_tab(i);
        self.request(ctx, Pending::CloseTab(i));
    }

    pub(crate) fn do_close_tab(&mut self, i: usize) {
        if self.tabs.len() <= 1 {
            self.ed.new_map();
            return;
        }
        if i == self.active_tab {
            let j = if i > 0 { i - 1 } else { 1 };
            self.switch_tab(j);
        }
        self.tabs.remove(i);
        if self.active_tab > i {
            self.active_tab -= 1;
        }
    }

    /// Menüeintrag für einen Befehl (Beschriftung + aktuelles Kürzel)
    pub(crate) fn cmd_item(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, cmd: Cmd) {
        let key = display_shortcut(&self.key_for(cmd));
        ui.spacing_mut().item_spacing.x = 24.0;
        if ui.add(egui::Button::new(cmd.label()).shortcut_text(key)).clicked() {
            ui.close_menu();
            self.run_cmd(ctx, cmd);
        }
    }

    pub(crate) fn import_map(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter(crate::i18n::tr("Maps anderer Editoren (.map, .rmf)"), &["map", "rmf", "jmf"]).pick_file() else { return };
        self.import_foreign(p);
    }

    /// .map / .rmf als neue (ungespeicherte) Map übernehmen
    pub(crate) fn import_foreign(&mut self, p: std::path::PathBuf) {
        match crate::prefabs::load_foreign_map(&p, self.ed.tex_scale) {
            Ok(m) => {
                if !self.is_pristine() {
                    self.new_tab();
                }
                self.win.start = false;
                let n = (m.world.solids.len(), m.entities.len());
                let foreign = crate::prefabs::foreign_textures(&m);
                self.ed.map = m;
                self.ed.dirty = true;
                self.ed.status = format!("{} importiert: {} Brushes, {} Entities – bitte als VMF speichern", p.display(), n.0, n.1);
                if !foreign.is_empty() {
                    self.ed.status = format!("{} importiert: {} Brushes, {} Entities, {} Texturen ohne Ordner (GoldSrc?) – mit „Texturen ersetzen“ austauschen, dann als VMF speichern", p.display(), n.0, n.1, foreign.len());
                }
                let b = self.ed.map_bounds();
                for v in &mut self.views2d {
                    v.focus(&b, egui::Vec2::new(700.0, 450.0));
                }
                self.view3d.focus(&b);
                self.ed.touch();
            }
            Err(e) => {
                rfd::MessageDialog::new().set_title(crate::i18n::tr("Import fehlgeschlagen")).set_description(crate::i18n::tr_dyn(&e)).show();
            }
        }
    }

    pub(crate) fn export_obj(&mut self) {
        let Some(mut p) = rfd::FileDialog::new().add_filter("Wavefront OBJ", &["obj"]).set_file_name(format!("{}.obj", self.ed.map_name())).save_file() else { return };
        if p.extension().is_none() {
            p.set_extension("obj");
        }
        let mtl = p.with_extension("mtl");
        let mats = &self.mats;
        let dims = |m: &str| mats.info(m).map(|i| (i.width as f64, i.height as f64)).unwrap_or((512.0, 512.0));
        let scale = if self.settings.obj_meters { 0.0254 } else { 1.0 };
        let (obj, mtl_text) = crate::importexport::export_obj(&self.ed, scale, &dims, &mtl.file_name().unwrap_or_default().to_string_lossy());
        let r = std::fs::write(&p, obj).and_then(|_| std::fs::write(&mtl, mtl_text));
        self.ed.status = match r {
            Ok(()) => format!("Exportiert: {} (Z zeigt nach oben)", p.display()),
            Err(e) => format!("Export fehlgeschlagen: {e}"),
        };
    }

    // -------------------------------------------------------------- MCP

    pub(crate) fn restart_mcp(&mut self) {
        self.mcp = None;
        if !self.settings.mcp_enabled {
            self.mcp_status = crate::i18n::tr("aus").into();
            return;
        }
        let ctx = self.ctx.clone();
        match mcp::start_server(self.settings.mcp_port, move || ctx.request_repaint()) {
            Ok(s) => {
                self.mcp_status = format!("läuft auf http://127.0.0.1:{}/mcp", s.port);
                self.mcp = Some(s);
            }
            Err(e) => self.mcp_status = e,
        }
    }

    pub(crate) fn poll_mcp(&mut self) {
        let Some(srv) = &self.mcp else { return };
        let reqs: Vec<mcp::Request> = srv.rx.try_iter().collect();
        for r in reqs {
            let rev = self.ed.rev;
            let resp = mcp::handle(&r.body, self);
            let _ = r.reply.send(resp);
            // Ändert eine KI die Map, soll man das sehen: Startseite schließen
            let tool = r.body.get("params").and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            if self.ed.rev != rev || matches!(tool, "new_map" | "open_map" | "run_command") {
                self.win.start = false;
            }
        }
    }

    pub(crate) fn mcp_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(crate::i18n::tr("MCP-Server (KI-Fernsteuerung)")).strong());
        ui.horizontal(|ui| {
            let mut changed = crate::theme::checkbox(ui, &mut self.settings.mcp_enabled, "aktiv").changed();
            ui.label(crate::i18n::tr("Port"));
            changed |= ui.add(crate::theme::dv(egui::DragValue::new(&mut self.settings.mcp_port)).range(1024..=65535)).lost_focus();
            if changed {
                self.restart_mcp();
            }
        });
        let ok = self.mcp.is_some();
        ui.colored_label(if ok { Color32::LIGHT_GREEN } else { Color32::YELLOW }, crate::i18n::tr_dyn(&self.mcp_status));
        let exe = std::env::current_exe().map(|p| p.display().to_string().replace('\\', "\\\\")).unwrap_or_else(|_| "amboss.exe".into());
        let port = self.settings.mcp_port;
        let snippets = [
            ("Claude Code", format!("claude mcp add --transport http amboss http://127.0.0.1:{port}/mcp")),
            ("Claude Desktop (claude_desktop_config.json)", format!("{{\n  \"mcpServers\": {{\n    \"amboss\": {{ \"command\": \"{exe}\", \"args\": [\"--mcp-stdio\", \"--port\", \"{port}\"] }}\n  }}\n}}")),
        ];
        for (title, text) in snippets {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).small());
                if ui.small_button(crate::i18n::tr("Kopieren")).clicked() {
                    ui.ctx().copy_text(text.clone());
                }
            });
            ui.label(RichText::new(text).monospace().small());
        }
        ui.label(RichText::new(crate::i18n::tr("Ohne offenes Fenster: amboss --mcp-stdio --map pfad.vmf [--game-dir…/garrysmod]")).small());
    }
}

impl Host for App {
    fn ed(&mut self) -> &mut Editor {
        &mut self.ed
    }
    fn materials(&self) -> Vec<String> {
        self.mats.list.clone()
    }
    fn models(&self) -> Vec<String> {
        self.models.list.clone()
    }
    fn save(&mut self, path: Option<PathBuf>) -> Result<String, String> {
        let p = path.or_else(|| self.ed.path.clone()).ok_or("Map hat noch keinen Pfad – 'path' angeben")?;
        if self.save_to(p.clone()) {
            Ok(p.display().to_string())
        } else {
            Err("Speichern fehlgeschlagen".into())
        }
    }
    fn open(&mut self, path: PathBuf) -> Result<String, String> {
        if !path.is_file() {
            return Err(format!("{} nicht gefunden", path.display()));
        }
        self.open_in_tab(path.clone());
        Ok(path.display().to_string())
    }
    fn compile(&mut self) -> Result<String, String> {
        if self.ed.path.is_none() {
            return Err("Map zuerst mit save_map (path) speichern".into());
        }
        if self.settings.game_dir.is_empty() {
            return Err("Kein Spielordner eingestellt".into());
        }
        self.start_compile();
        Ok("Kompilieren gestartet – Status mit get_compile_status abfragen".into())
    }
    fn compile_status(&self) -> Value {
        mcp::job_status(self.job.as_ref())
    }
    fn run_command(&mut self, id: &str) -> Result<String, String> {
        let cmd = Cmd::from_id(id).ok_or(format!("Unbekannter Befehl '{id}'"))?;
        if matches!(cmd, Cmd::Open | Cmd::SaveAs | Cmd::ImportMap | Cmd::MergeMap | Cmd::CompareMap | Cmd::Merge3 | Cmd::ExportObj | Cmd::LoadPointfile) {
            return Err("Dieser Befehl öffnet einen Dateidialog und ist per MCP nicht verfügbar".into());
        }
        let ctx = self.ctx.clone();
        self.run_cmd(&ctx, cmd);
        Ok(self.ed.status.clone())
    }
    fn commands(&self) -> Vec<(String, String)> {
        Cmd::all()
            .into_iter()
            .filter(|c| !matches!(c, Cmd::Open | Cmd::SaveAs | Cmd::ImportMap | Cmd::MergeMap | Cmd::CompareMap | Cmd::Merge3 | Cmd::ExportObj | Cmd::LoadPointfile))
            .map(|c| (c.id(), c.label()))
            .collect()
    }
    fn known_materials(&self) -> Option<std::collections::HashSet<String>> {
        App::known_materials(self)
    }
    fn prefab_dir(&self) -> PathBuf {
        App::prefab_dir(self)
    }
    fn opt_settings(&mut self) -> &mut crate::optimize::OptSettings {
        &mut self.settings.opt
    }
    fn make_prop(&mut self, o: crate::propper::PropOpts, compile: bool) -> Result<Value, String> {
        // Leerer Name = nur Status abfragen
        if !o.name.is_empty() {
            if self.prop.job.is_some() {
                return Err("Es läuft bereits eine Modell-Kompilierung".into());
            }
            self.prop.opts = o;
            self.prop_run(compile);
        }
        self.poll_prop();
        let log = self.prop.log.lock().unwrap().clone();
        Ok(serde_json::json!({"running": self.prop.job.is_some(), "log": log}))
    }

    fn game_dir(&self) -> String {
        self.settings.game_dir.clone()
    }
    fn import_model(&mut self, path: Option<std::path::PathBuf>, a: &Value) -> Result<Value, String> {
        if let Some(p) = path {
            if self.imp.job.is_some() {
                return Err("Es läuft bereits ein Modell-Import".into());
            }
            self.open_model_import(p);
            if self.imp.model.is_none() {
                return Err(self.imp.log.lock().unwrap().join("\n"));
            }
            let o = &mut self.imp.opts;
            let s = |k: &str| a.get(k).and_then(|v| v.as_str());
            if let Some(n) = s("name") {
                o.name = n.trim().to_ascii_lowercase().replace([' ', '/', '\\'], "_");
            }
            if let Some(f) = s("folder") {
                o.folder = f.trim_matches('/').to_string();
            }
            match s("unit") {
                Some("m") => o.scale = crate::model_import::METER,
                Some("cm") => o.scale = crate::model_import::METER / 100.0,
                Some("in") => o.scale = 1.0,
                _ => {}
            }
            if let Some(x) = a.get("scale").and_then(|v| v.as_f64()) {
                o.scale = x;
            }
            if let Some(u) = s("up") {
                o.y_up = u.eq_ignore_ascii_case("y");
            }
            if let Some(x) = a.get("yaw").and_then(|v| v.as_f64()) {
                o.yaw = x;
            }
            if let Some(x) = a.get("static").and_then(|v| v.as_bool()) {
                o.is_static = x;
            }
            if let Some(x) = a.get("mass").and_then(|v| v.as_f64()) {
                o.mass = x;
            }
            if let Some(x) = s("surfaceprop") {
                o.surfaceprop = x.to_string();
            }
            if let Some(x) = a.get("collision").and_then(|v| v.as_bool()) {
                o.collision = x;
            }
            if let Some(x) = a.get("texture_size").and_then(|v| v.as_u64()) {
                o.tex_max = (x as u32).clamp(16, 4096);
            }
            if let Some(p) = a.get("position").and_then(|v| v.as_array()).filter(|v| v.len() == 3) {
                self.imp.pos = glam::DVec3::new(p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0), p[2].as_f64().unwrap_or(0.0));
            }
            let (tris, size) = self.import_run(a.get("compile").and_then(|v| v.as_bool()).unwrap_or(true))?;
            let o = &self.imp.opts;
            return Ok(serde_json::json!({"model": format!("models/{}/{}.mdl", o.folder, o.name), "triangles": tris, "size": [size.x, size.y, size.z], "running": self.imp.job.is_some(), "log": self.imp.log.lock().unwrap().clone()}));
        }
        self.poll_import();
        let log = self.imp.log.lock().unwrap().clone();
        Ok(serde_json::json!({"running": self.imp.job.is_some(), "log": log}))
    }
    fn game_fs(&self) -> Option<std::sync::Arc<crate::gamefs::GameFs>> {
        self.mats.fs()
    }
    fn show_report(&mut self, r: &crate::optimize::Report) {
        self.show_opt_report(r);
    }
}
