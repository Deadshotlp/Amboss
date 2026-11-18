//! Hauptfenster: Menüs, Werkzeugleiste, Tastenkürzel, Ansichten.

use crate::commands::Cmd;
use crate::compile::{self, CompileJob, JobState, Settings};
use crate::editor::{Editor, Tool};
use crate::fgd::Fgd;
use crate::gamefs::GameFs;
use crate::geometry::{Aabb, Axis2D};
use crate::materials::MaterialSystem;
use crate::models::ModelSystem;
use crate::problems::Problem;
use crate::view2d::View2D;
use crate::view3d::{RenderMode, View3D};
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText, Vec2};
use glam::{DVec2, DVec3};
use std::path::PathBuf;
use std::time::Instant;

const SETTINGS_KEY: &str = "amboss_settings";

#[derive(Clone)]
pub(crate) enum Pending {
    New,
    Open(Option<PathBuf>),
    Quit,
    CloseTab(usize),
}

/// Nicht aktive Map (Tab)
pub(crate) struct DocSlot {
    pub ed: Editor,
    pub cam: (glam::Vec3, f32, f32),
    pub views: [(DVec2, f64); 3],
}

#[derive(Default)]
pub(crate) struct Windows {
    /// Einrichtungs-Assistent
    pub setup: bool,
    /// Tutorial-Kapitelauswahl
    pub tutorial: bool,
    /// Über Amboss
    pub about: bool,
    /// Vegetation streuen
    pub scatter: bool,
    /// Prozedurale Generatoren
    pub procgen: bool,
    /// Tile-Set-Werkzeug
    pub tiles: bool,
    /// Polygon-Mesh / Bezier-Patch
    pub mesh: bool,
    /// Live-Abgleich mit dem Spiel
    pub live: bool,
    pub settings: bool,
    pub compile: bool,
    pub help: bool,
    pub world_props: bool,
    pub browser: bool,
    pub transform: bool,
    pub paste_special: bool,
    pub replace: bool,
    pub problems: bool,
    pub entity_report: bool,
    pub map_info: bool,
    pub goto: bool,
    pub prefabs: bool,
    pub models: bool,
    pub sounds: bool,
    pub logical: bool,
    pub entity_help: Option<String>,
    pub outliner: bool,
    pub history: bool,
    pub palette: bool,
    pub optimize: bool,
    pub start: bool,
    pub collab: bool,
}

/// Zustände der Dialoge
pub(crate) struct DialogState {
    /// MSAA-Stufe (aus grafik.txt, None = noch nicht gelesen)
    pub msaa: Option<u16>,
    /// neue Oberflächengröße, im nächsten Bild anwenden
    pub pending_zoom: Option<f32>,
    pub start_filter: String,
    /// Zeitpunkt der letzten Änderung am Spielordner (für automatisches Neuladen)
    pub gamedir_changed: Option<Instant>,
    pub transform_mode: usize,
    pub transform_val: [f64; 3],
    pub ps_copies: u32,
    pub ps_offset: [f64; 3],
    pub ps_rot: [f64; 3],
    pub ps_group: bool,
    pub rep_find: String,
    /// Kachelgröße im Textur-Browser (px)
    pub browser_tile: f32,
    /// Drehpunkt beim Transformieren: 0 Mitte der Auswahl, 1 Weltursprung, 2 eigener Punkt
    pub transform_pivot: usize,
    pub transform_pivot_pt: [f64; 3],
    /// „Gehe zu“: 0 automatisch, 1 Brush, 2 Seite, 3 Entity, 4 Name
    pub goto_kind: usize,
    pub rep_with: String,
    pub rep_sel: bool,
    pub rep_partial: bool,
    pub report_filter: String,
    pub settings_page: usize,
    pub goto_text: String,
    pub sound_filter: String,
    /// Browser-Ziel: (Entity-ID, Schlüssel); None = Standard für neue Props
    pub browse_target: Option<(u32, String)>,
    pub sound_active: usize,
    pub sound_durations: std::collections::HashMap<String, Option<f32>>,
    pub sound_missing: std::collections::HashSet<String>,
    pub sound_list: Vec<String>,
    pub prefab_name: String,
    pub prefab_category: String,
    pub prefab_filter: String,
    pub visgroup_name: String,
    pub treat_as_one: bool,
    pub logical_pan: egui::Vec2,
    pub logical_zoom: f32,
    /// Suche in der logischen Ansicht
    pub logical_filter: String,
    pub logical_drag: Option<u32>,
    pub palette_filter: String,
    pub palette_index: usize,
    pub palette_expand: Option<usize>,
    pub palette_face_counts: Option<std::collections::HashMap<String, usize>>,
    pub goto_coord: String,
    pub outliner_filter: String,
    pub keymap_capture: Option<Cmd>,
    pub keymap_filter: String,
    /// Aufnahme einer Kamera-Taste (Index der Bewegungsrichtung)
    pub cam_capture: Option<usize>,
}

impl Default for DialogState {
    fn default() -> Self {
        DialogState {
            pending_zoom: None,
            msaa: None,
            start_filter: String::new(),
            gamedir_changed: None,
            transform_mode: 0,
            transform_val: [0.0; 3],
            ps_copies: 1,
            ps_offset: [0.0, 0.0, 64.0],
            ps_rot: [0.0; 3],
            ps_group: false,
            rep_find: String::new(),
            browser_tile: 96.0,
            transform_pivot: 0,
            transform_pivot_pt: [0.0; 3],
            goto_kind: 0,
            rep_with: String::new(),
            rep_sel: false,
            rep_partial: true,
            report_filter: String::new(),
            settings_page: 0,
            goto_text: String::new(),
            sound_filter: String::new(),
            browse_target: None,
            sound_active: 0,
            sound_durations: Default::default(),
            sound_missing: Default::default(),
            sound_list: Vec::new(),
            prefab_name: crate::i18n::tr("mein_prefab").into(),
            prefab_category: String::new(),
            prefab_filter: String::new(),
            visgroup_name: crate::i18n::tr("Neue Visgroup").into(),
            treat_as_one: false,
            logical_pan: egui::Vec2::ZERO,
            logical_zoom: 1.0,
            logical_filter: String::new(),
            logical_drag: None,
            palette_filter: String::new(),
            palette_index: 0,
            palette_expand: None,
            palette_face_counts: None,
            goto_coord: String::new(),
            outliner_filter: String::new(),
            keymap_capture: None,
            cam_capture: None,
            keymap_filter: String::new(),
        }
    }
}

pub struct App {
    pub(crate) ed: Editor,
    pub(crate) views2d: [View2D; 3],
    pub(crate) view3d: View3D,
    pub(crate) settings: Settings,
    /// None = 4 Ansichten, Some(0..4) = eine Ansicht maximiert (0 = 3D)
    pub(crate) maximized: Option<usize>,
    pub(crate) win: Windows,
    pub(crate) dlg: DialogState,
    pub(crate) job: Option<CompileJob>,
    pub(crate) job_seen: Option<JobState>,
    pub(crate) pending: Option<Pending>,
    pub(crate) allow_close: bool,
    pub(crate) cursor: Option<(Axis2D, DVec2)>,
    pub(crate) tie_class: String,
    pub(crate) hollow_thickness: f64,
    pub(crate) new_kv: (String, String),
    pub(crate) hover_3d: bool,
    pub(crate) hovered_view: Option<usize>,
    pub(crate) last_title: String,
    pub(crate) problems: Vec<Problem>,
    pub(crate) fgd_messages: Vec<String>,
    pub(crate) mats: MaterialSystem,
    pub(crate) models: ModelSystem,
    pub(crate) browser_filter: String,
    pub(crate) last_autosave: Instant,
    pub(crate) autosave_index: u32,
    /// Tabs: an der aktiven Position steht None (die Map liegt in `ed`)
    pub(crate) tabs: Vec<Option<DocSlot>>,
    pub(crate) active_tab: usize,
    pub(crate) mcp: Option<crate::mcp::Server>,
    pub(crate) mcp_status: String,
    pub(crate) ctx: egui::Context,
    pub(crate) last_material: String,
    pub(crate) prefab_entries: Vec<crate::prefabs::Entry>,
    pub(crate) prefab_thumbs: std::collections::HashMap<String, egui::TextureHandle>,
    pub(crate) opt_report: Option<crate::optimize::Report>,
    pub(crate) merge_dlg: Option<crate::merge_ui::MergeDialog>,
    pub(crate) class_picker: crate::class_picker::ClassPicker,
    pub(crate) player: crate::audio::Player,
    pub(crate) mb: crate::model_browser::ModelBrowserState,
    pub(crate) pb: crate::particle_browser::ParticleBrowserState,
    pub(crate) hm: crate::heightmap::HeightmapDlg,
    pub(crate) prop: crate::propper::PropDlg,
    pub(crate) pack: crate::pack_ui::PackDlg,
    pub(crate) decomp: crate::decompile::DecompState,
    /// Kontextleiste: benötigte Breite je Kompakt-Stufe (vom letzten Zeichnen)
    pub(crate) ctx_widths: [f32; 3],
    pub(crate) ctx_sig: String,
    pub(crate) scripts: crate::lua::ScriptDlg,
    pub(crate) mated: crate::material_editor::MatEd,
    pub(crate) parted: crate::particle_editor::PartEd,
    pub(crate) phys: Option<crate::physics::Sim>,
    pub(crate) gen: crate::procgen::GenOpts,
    pub(crate) gen_page: usize,
    pub(crate) mesh_ui: crate::mesh_ui::MeshUi,
    pub(crate) live: crate::live::Live,
    /// Nach dem Kompilieren die Map im laufenden Spiel neu laden
    pub(crate) live_reload: Option<String>,
    /// Feld „Input auslösen“ im Live-Fenster: Ziel, Input, Parameter
    pub(crate) live_fire: (String, String, String),
    pub(crate) mesh_page: usize,
    pub(crate) nav: crate::navmesh::NavState,
    pub(crate) setup: crate::setup::SetupState,
    pub(crate) tut: crate::tutorial::TutorialState,
    /// Lage wichtiger Bedienelemente (für die Hervorhebung im Tutorial)
    pub(crate) ui_rects: std::collections::HashMap<&'static str, egui::Rect>,
    /// Panelbreite beim ersten Bild aus den Einstellungen statt aus egui-Gedächtnis
    pub(crate) panel_width_init: bool,
    pub(crate) help_filter: String,
    pub(crate) status_seen: (String, std::time::Instant),
    pub(crate) versions_dlg: Option<crate::merge_ui::VersionsDialog>,
    pub(crate) collab: Option<crate::collab::Collab>,
    pub(crate) collab_form: crate::collab_ui::CollabForm,
    pub(crate) upd: crate::update_ui::UpdateUi,
    pub(crate) sky: crate::sky_ui::SkyUi,
    pub(crate) imp: crate::model_import_ui::ImportUi,
    pub(crate) ws: crate::workshop::WorkshopUi,
    /// Zeitpunkt des letzten Speicherns
    pub(crate) last_saved: Option<Instant>,
    /// Rechtes Panel: 0 Eigenschaften, 1 Objekte, 2 Visgroups
    pub(crate) panel_tab: usize,
    /// Zuletzt angeklickte Ansicht (0 = 3D)
    pub(crate) active_view: Option<usize>,
    pub(crate) tex_filter: String,
    /// Textur-Werkzeug: Flächen-Einstellungen über den Texturen aufgeklappt
    pub(crate) face_opts_open: bool,
    pub(crate) template_maps: Vec<crate::map::Map>,
    pub(crate) recovery: Option<(PathBuf, String)>,
    /// Infos zu zuletzt geöffneten Dateien (im Hintergrund ermittelt); None = wird noch gelesen
    pub(crate) recent_info: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Option<crate::templates::RecentInfo>>>>,
    /// Hintergrund-Laden der Spielinhalte: (Dateisystem, Materialliste, Soundliste)
    pub(crate) content_rx: Option<std::sync::mpsc::Receiver<Result<ContentMsg, String>>>,
    /// 0 = noch nichts empfangen, 1 = Spiel-VPKs da, 2 = mit Addons
    pub(crate) content_stage: u8,
    /// Ergebnis des letzten Ladens der Spielinhalte (für Startseite/Statusleiste)
    pub(crate) content_status: String,
    model_req_rev: (u64, usize),
    lines_key: (u64, bool, bool, u64),
    lines_built: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, SETTINGS_KEY)).unwrap_or_default();
        if settings.profiles.is_empty() {
            settings.store_profile();
        }
        // Sprache zuerst – Vorgabetexte (Visgroup-, Prefab-Namen …) entstehen schon beim Aufbau
        crate::i18n::set_lang(crate::i18n::Lang::from_code(&settings.language));
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx, settings.theme_dark);
        settings.ui_scale = if settings.ui_scale.is_finite() { settings.ui_scale.clamp(0.75, 2.5) } else { 1.0 };
        cc.egui_ctx.set_zoom_factor(settings.ui_scale);

        let (fgd, fgd_messages) = load_fgds(&settings.fgd_files);
        let mut ed = Editor::new(fgd);
        ed.tex_lock = settings.tex_lock;
        ed.grid = settings.grid.clamp(1.0, 1024.0);
        let mut app = App {
            ed,
            views2d: [View2D::new(Axis2D::Top), View2D::new(Axis2D::Front), View2D::new(Axis2D::Side)],
            view3d: View3D::new(cc.gl.as_deref()),
            settings,
            maximized: None,
            win: Windows::default(),
            dlg: DialogState::default(),
            job: None,
            job_seen: None,
            pending: None,
            allow_close: false,
            cursor: None,
            tie_class: "func_detail".into(),
            hollow_thickness: 16.0,
            new_kv: Default::default(),
            hover_3d: false,
            hovered_view: None,
            last_title: String::new(),
            problems: Vec::new(),
            fgd_messages,
            mats: MaterialSystem::new(),
            models: ModelSystem::new(),
            browser_filter: String::new(),
            last_autosave: Instant::now(),
            autosave_index: 0,
            tabs: vec![None],
            active_tab: 0,
            mcp: None,
            mcp_status: String::new(),
            ctx: cc.egui_ctx.clone(),
            last_material: String::new(),
            prefab_entries: Vec::new(),
            prefab_thumbs: Default::default(),
            opt_report: None,
            merge_dlg: None,
            class_picker: Default::default(),
            player: Default::default(),
            mb: Default::default(),
            pb: Default::default(),
            hm: Default::default(),
            prop: Default::default(),
            pack: Default::default(),
            decomp: Default::default(),
            ctx_widths: [0.0; 3],
            ctx_sig: String::new(),
            scripts: Default::default(),
            mated: Default::default(),
            parted: Default::default(),
            phys: None,
            gen: Default::default(),
            gen_page: 0,
            mesh_ui: Default::default(),
            live: Default::default(),
            live_reload: None,
            live_fire: (String::new(), "Use".into(), String::new()),
            mesh_page: 0,
            nav: Default::default(),
            setup: Default::default(),
            tut: Default::default(),
            ui_rects: Default::default(),
            panel_width_init: false,
            help_filter: String::new(),
            status_seen: (String::new(), std::time::Instant::now()),
            versions_dlg: None,
            collab: None,
            collab_form: Default::default(),
            upd: Default::default(),
            sky: Default::default(),
            imp: Default::default(),
            ws: Default::default(),
            last_saved: None,
            panel_tab: 0,
            active_view: Some(1),
            tex_filter: String::new(),
            face_opts_open: false,
            template_maps: Vec::new(),
            recovery: None,
            recent_info: Default::default(),
            content_rx: None,
            content_stage: 0,
            content_status: String::new(),
            model_req_rev: (u64::MAX, 0),
            lines_key: (u64::MAX, false, false, 0),
            lines_built: Instant::now(),
        };
        app.apply_layout();
        app.rescan_prefabs();
        app.restart_mcp();
        for v in &mut app.views2d {
            v.gl = app.view3d.renderer.clone();
        }
        app.mats.set_ctx(cc.egui_ctx.clone());
        app.models.ctx = Some(cc.egui_ctx.clone());
        app.reload_content();
        if let Some(path) = std::env::args().skip(1).find(|a| !a.starts_with("--")) {
            app.open_path(PathBuf::from(path));
        } else {
            app.win.start = app.settings.show_start;
        }
        crate::i18n::set_lang(crate::i18n::Lang::from_code(&app.settings.language));
        app.maybe_start_setup();
        app.apply_debug_open();
        crate::update::cleanup();
        if app.settings.check_updates && std::env::var_os("AMBOSS_DEBUG_OPEN").is_none() && std::env::var_os("AMBOSS_NO_UPDATE").is_none() {
            app.start_update_check(false);
        }
        app
    }

    // -------------------------------------------------------------- Datei

    pub(crate) fn open_path(&mut self, path: PathBuf) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")) {
            self.win.start = false;
            self.ask_decompile(path);
            return;
        }
        match self.ed.load(path.clone()) {
            Ok(()) => {
                self.win.start = false;
                self.add_recent(&path);
                let b = self.ed.map.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
                for v in &mut self.views2d {
                    v.focus(&b, Vec2::new(700.0, 450.0));
                }
                if let Some(c) = self.ed.map.cameras.get(self.ed.map.active_camera) {
                    self.view3d.set_from_camera(c);
                } else if !b.is_empty() {
                    self.view3d.focus(&b);
                }
                self.ed.camera_changed = false;
            }
            Err(e) => {
                self.ed.status = format!("Fehler beim Laden: {e}");
                rfd::MessageDialog::new().set_title("Fehler").set_description(format!("Konnte {} nicht laden:\n{e}", path.display())).show();
            }
        }
    }

    fn add_recent(&mut self, path: &std::path::Path) {
        let s = path.display().to_string();
        self.settings.recent_files.retain(|r| r != &s);
        self.settings.recent_files.insert(0, s);
        self.settings.recent_files.truncate(10);
    }

    pub(crate) fn save_as(&mut self) -> bool {
        let mut dlg = rfd::FileDialog::new().add_filter("Valve Map File", &["vmf"]).set_file_name(format!("{}.vmf", self.ed.map_name()));
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        } else if !self.settings.game_dir.is_empty() {
            dlg = dlg.set_directory(PathBuf::from(&self.settings.game_dir).join("maps"));
        }
        if let Some(mut path) = dlg.save_file() {
            if path.extension().is_none() {
                path.set_extension("vmf");
            }
            return self.save_to(path);
        }
        false
    }

    pub(crate) fn save(&mut self) -> bool {
        match self.ed.path.clone() {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    pub(crate) fn save_to(&mut self, path: PathBuf) -> bool {
        self.sync_camera_to_map();
        match self.ed.save(path.clone()) {
            Ok(()) => {
                self.add_recent(&path);
                self.last_saved = Some(Instant::now());
                // Die gespeicherte Datei könnte eine Instanz sein: andere offene Maps aktualisieren
                for t in self.tabs.iter_mut().flatten() {
                    t.ed.reload_instances();
                }
                true
            }
            Err(e) => {
                rfd::MessageDialog::new().set_title("Fehler").set_description(format!("Speichern fehlgeschlagen:\n{e}")).show();
                false
            }
        }
    }

    pub(crate) fn request(&mut self, ctx: &egui::Context, p: Pending) {
        let any_dirty = self.ed.dirty || self.tabs.iter().flatten().any(|t| t.ed.dirty);
        let needs = match p {
            Pending::Quit => any_dirty,
            Pending::CloseTab(_) => self.ed.dirty,
            _ => false,
        };
        if needs {
            self.pending = Some(p);
        } else {
            self.run_pending(ctx, p);
        }
    }

    pub(crate) fn run_pending(&mut self, ctx: &egui::Context, p: Pending) {
        match p {
            Pending::New => self.new_tab(),
            Pending::Open(path) => {
                let path = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter(crate::i18n::tr("Maps (VMF, BSP)"), &["vmf", "vmx", "bsp"])
                        .add_filter("Valve Map File (VMF)", &["vmf", "vmx"])
                        .add_filter(crate::i18n::tr("Kompilierte Map (BSP) – wird dekompiliert"), &["bsp"])
                        .pick_file()
                });
                if let Some(path) = path {
                    self.open_in_tab(path);
                }
            }
            Pending::CloseTab(i) => self.do_close_tab(i),
            Pending::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub(crate) fn autosave_dir(&self) -> PathBuf {
        match self.ed.path.as_ref().and_then(|p| p.parent()) {
            Some(d) => d.join("autosave"),
            None => std::env::temp_dir().join("amboss_autosave"),
        }
    }

    fn autosave(&mut self) {
        let mins = self.settings.autosave_minutes;
        if mins == 0 || !self.ed.dirty || self.last_autosave.elapsed().as_secs() < mins as u64 * 60 {
            return;
        }
        self.last_autosave = Instant::now();
        let dir = self.autosave_dir();
        let _ = std::fs::create_dir_all(&dir);
        self.autosave_index = self.autosave_index % self.settings.autosave_keep.max(1) + 1;
        let path = dir.join(format!("{}_autosave{}.vmf", self.ed.map_name(), self.autosave_index));
        self.ed.map.grid = self.ed.grid;
        match std::fs::write(&path, self.ed.map.to_vmf()) {
            Ok(()) => self.ed.status = format!("Autosave: {}", path.display()),
            Err(e) => self.ed.status = format!("Autosave fehlgeschlagen: {e}"),
        }
    }

    pub(crate) fn start_compile(&mut self) {
        if self.job.as_ref().is_some_and(|j| j.state() == JobState::Running) {
            return;
        }
        // Fehlende Standardwerte aus der FGD ergänzen (wie Hammer beim Anlegen) – sonst ist z. B. ein
        // light ohne „_light“ für VRAD schwarz
        let filled = self.ed.fill_fgd_defaults();
        if (self.ed.dirty || self.ed.path.is_none()) && !self.save() {
            return;
        }
        let Some(mut path) = self.ed.path.clone() else { return };
        self.problems = crate::problems::check(&self.ed, self.known_materials().as_ref());
        let mut prelude = Vec::new();
        if !filled.is_empty() {
            prelude.push(crate::i18n::tr_dyn(&format!("Fehlende Standardwerte ergänzt: {}", filled.join(", "))));
        }
        if self.settings.opt.compile_auto {
            match self.optimized_build(&path) {
                Ok((p, lines)) => {
                    path = p;
                    prelude = lines;
                }
                Err(()) => return,
            }
        } else if let Some(m) = self.ed.cordoned_map() {
            // Cordon: nur den Bereich exportieren (gleicher Map-Name, eigener Ordner)
            let dir = path.parent().map(|p| p.join("cordon")).unwrap_or_else(|| PathBuf::from("cordon"));
            let _ = std::fs::create_dir_all(&dir);
            let cp = dir.join(path.file_name().unwrap_or_default());
            if let Err(e) = std::fs::write(&cp, m.to_vmf()) {
                self.ed.status = format!("Cordon-Export fehlgeschlagen: {e}");
                return;
            }
            path = cp;
        }
        // Liste der benötigten Workshop-Addons (neben der VMF) – Hinweis oben im Compile-Log
        if let (Some(fs), Some(src)) = (self.mats.fs(), self.ed.path.clone()) {
            let mut lines = crate::assets::write_addon_list(&self.ed, &fs, &src);
            // Navmesh vor dem Packen erzeugen, damit maps/<map>.nav mit eingepackt wird
            if self.settings.opt.navmesh {
                let np = crate::navmesh::nav_path(&self.settings.game_dir, Some(&src), &self.ed.map_name());
                match crate::navmesh::generate_and_save(&self.ed.map, self.ed.map_limit, &np, 0) {
                    Ok(r) => {
                        lines.push(r.summary(&np));
                        self.ed.nav_preview = Some(std::sync::Arc::new(r.areas));
                    }
                    Err(e) => lines.push(format!("Navmesh: {e}")),
                }
            }
            if self.settings.opt.pack_content {
                lines.extend(crate::assets::write_packlist(&self.ed, &fs, &src, &path));
            }
            lines.append(&mut prelude);
            prelude = lines;
        }
        // Polygon-Meshes brauchen ihr kompiliertes Modell, sonst fehlt das Prop im Spiel
        let gd = self.settings.game_dir.trim().to_string();
        let missing: Vec<String> = self.ed.map.meshes.iter().filter(|m| self.ed.map.entity(m.ent).is_some()).filter(|m| gd.is_empty() || !std::path::Path::new(&gd).join(format!("models/amboss/meshes/{}.mdl", m.name)).is_file()).map(|m| m.name.clone()).collect();
        if !missing.is_empty() {
            prelude.insert(0, crate::i18n::tr_dyn(&format!("Warnung: {} Polygon-Mesh(es) noch nicht zum Modell kompiliert (Werkzeuge → Polygon-Mesh…): {}", missing.len(), missing.join(", "))));
        }
        // Live-Abgleich: dieser Stand ist der neue Bezug; läuft das Spiel schon, wird die Map dort
        // neu geladen statt ein zweites Spiel zu starten
        self.live.set_baseline(&self.ed.map);
        let mut st = self.settings.clone();
        if self.live.connected() {
            st.launch_game = false;
            st.steps.retain(|s| s.exe.trim() != "$game");
            self.live_reload = Some(path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        }
        match compile::start_with(&st, &path, prelude) {
            Ok(job) => {
                self.job = Some(job);
                self.job_seen = Some(JobState::Running);
            }
            Err(e) => self.ed.status = e,
        }
        self.win.compile = true;
    }

    pub(crate) fn known_materials(&self) -> Option<std::collections::HashSet<String>> {
        (!self.mats.list.is_empty()).then(|| self.mats.list.iter().cloned().collect())
    }

    fn poll_job(&mut self) {
        let Some(job) = &self.job else { return };
        let st = job.state();
        if self.job_seen == Some(st) {
            return;
        }
        self.job_seen = Some(st);
        if let Some(map) = self.live_reload.take() {
            if st == JobState::Done {
                self.live.reload_map(self.settings.game_dir.trim(), &map);
            } else if st == JobState::Running {
                self.live_reload = Some(map);
            }
        }
        if st != JobState::Running && job.leaked.load(std::sync::atomic::Ordering::SeqCst) {
            let lin = job.vmf_base.with_extension("lin");
            match crate::pointfile::load_lin(&lin) {
                Ok(p) => {
                    self.ed.pointfile = p;
                    self.ed.status = crate::i18n::tr("Leck gefunden – roter Pfad zeigt den Weg ins Freie (Kompilieren → Leck-Suche)").into();
                    self.ed.touch();
                }
                Err(e) => self.ed.status = format!("Leck, aber kein Pointfile: {e}"),
            }
        }
    }

    pub(crate) fn center_on_selection(&mut self, also_3d: bool) {
        let b = self.ed.selection_bounds();
        if b.is_empty() {
            return;
        }
        for v in &mut self.views2d {
            v.center_on(b.center());
        }
        if also_3d {
            self.view3d.focus(&b);
        }
    }

    pub(crate) fn center_on(&mut self, b: Aabb) {
        if b.is_empty() {
            return;
        }
        for v in &mut self.views2d {
            v.center_on(b.center());
        }
        self.view3d.focus(&b);
    }

    /// Spielinhalte (VPKs, Addons, Material- und Soundlisten) im Hintergrund einlesen,
    /// damit das Fenster sofort bedienbar ist (bei vielen Workshop-Addons dauert das lange)
    pub(crate) fn reload_content(&mut self) {
        // Pfad tolerant machen (Hauptordner oder Unterordner gewählt)
        let fixed = crate::compile::normalize_game_dir(&self.settings.game_dir);
        if fixed != self.settings.game_dir {
            self.settings.game_dir = fixed;
        }
        let dir = self.settings.game_dir.clone();
        let with_addons = self.settings.load_addons;
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // Fehler im Lade-Thread dürfen nicht stumm bleiben
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // kein Spielordner: Garry's Mod in allen Steam-Bibliotheken suchen (kann bei Netzlaufwerken dauern)
                let found = if dir.is_empty() { crate::compile::find_gmod_root() } else { None };
                let dir = match &found {
                    Some(root) => root.join("garrysmod").display().to_string(),
                    None => dir,
                };
                let scan = |addons: bool| {
                    let fs = if dir.is_empty() { GameFs::default() } else { GameFs::for_gmod_opts(std::path::Path::new(&dir), addons) };
                    let materials = fs.list_materials();
                    let sounds = fs.list_files("sound/", &[".wav", ".mp3", ".ogg"]);
                    let models = fs.list_files("models/", &[".mdl"]);
                    ContentMsg { final_stage: addons || !with_addons, found: found.clone(), fs, materials, sounds, models }
                };
                // Stufe 1: Spiel-VPKs und lose Ordner – schnell, sofort nutzbar
                let first = scan(false);
                if !with_addons {
                    // nur Spielverzeichnis: fertig
                    return first;
                }
                let _ = tx.send(Ok(first));
                ctx.request_repaint();
                // Stufe 2: zusätzlich alle Workshop-/Addon-Archive
                scan(true)
            }));
            let r = r.map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "unbekannter Fehler".into()));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.content_rx = Some(rx);
        self.content_stage = 0;
        self.ed.status = if self.settings.game_dir.is_empty() { crate::i18n::tr("Suche Garry's Mod in den Steam-Bibliotheken…").into() } else { crate::i18n::tr("Spielinhalte werden im Hintergrund geladen…").into() };
    }

    pub(crate) fn reload_fgds(&mut self) {
        let (fgd, msgs) = load_fgds(&self.settings.fgd_files);
        self.ed.fgd = fgd;
        self.fgd_messages = msgs;
        self.ed.touch();
    }

    fn sync_camera_to_map(&mut self) {
        let c = self.view3d.camera_def();
        if self.ed.map.cameras.is_empty() {
            self.ed.map.cameras.push(c);
            self.ed.map.active_camera = 0;
        } else {
            let i = self.ed.map.active_camera.min(self.ed.map.cameras.len() - 1);
            self.ed.map.cameras[i] = c;
        }
    }

    /// Horizontale/vertikale Achse der Ansicht unter der Maus (für Spiegeln/Drehen)
    pub(crate) fn view_axes(&self) -> (usize, usize, usize) {
        match self.hovered_view {
            Some(i @ 1..=3) => self.views2d[i - 1].axis.axes(),
            _ => Axis2D::Top.axes(),
        }
    }

    // -------------------------------------------------------------- Tastatur

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // Kamera-Taste aufnehmen (einzelne Taste ohne Umschalt/Strg/Alt)
        if let Some(slot) = self.dlg.cam_capture {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, .. } => Some(*key),
                    _ => None,
                })
            });
            if let Some(k) = pressed {
                if k != Key::Escape {
                    *self.settings.cam.slot_mut(slot) = k.name().to_string();
                }
                self.dlg.cam_capture = None;
            }
            return;
        }
        // Tastenkürzel-Aufnahme in den Einstellungen
        if let Some(cmd) = self.dlg.keymap_capture {
            let pressed = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } => Some(KeyboardShortcut::new(*modifiers, *key)),
                    _ => None,
                })
            });
            if let Some(sc) = pressed {
                if sc.logical_key == Key::Escape {
                    self.dlg.keymap_capture = None;
                } else if !matches!(sc.logical_key, Key::Enter) {
                    let mut sc = sc;
                    sc.modifiers.ctrl = sc.modifiers.ctrl || sc.modifiers.command;
                    self.settings.keymap.insert(cmd.id(), crate::commands::format_shortcut(&sc));
                    self.dlg.keymap_capture = None;
                }
            }
            return;
        }
        // Begehen: Tasten gehören der Spielfigur (Strg = ducken); nur F6/Esc beenden
        if self.view3d.walker.on {
            if ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::F6)) {
                self.view3d.walker.on = false;
            }
            return;
        }
        // Befehlspalette: Strg+K (angezeigt), Strg+P weiterhin still
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::P))) {
            self.dlg.palette_filter.clear();
            self.win.palette = true;
        }
        if self.win.palette {
            return;
        }
        let typing = ctx.wants_keyboard_input();
        // Kopieren/Einfügen kommen von egui als eigene Events (inkl. Text der System-Zwischenablage)
        let (mut copy, mut cut, mut paste_text): (bool, bool, Option<String>) = (false, false, None);
        if !typing {
            ctx.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::Copy => copy = true,
                        egui::Event::Cut => cut = true,
                        egui::Event::Paste(t) => paste_text = Some(t.clone()),
                        _ => {}
                    }
                }
            });
            let sc = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
            copy |= sc(Modifiers::COMMAND, Key::C) && !ctx.input(|i| i.modifiers.shift);
            cut |= sc(Modifiers::COMMAND, Key::X);
            if sc(Modifiers::COMMAND, Key::V) && paste_text.is_none() && !ctx.input(|i| i.modifiers.shift) {
                paste_text = Some(String::new());
            }
            if self.ed.grip_mode() {
                // Mesh-/Patch-Bearbeitung: Entf löscht Flächen statt des Objekts, Esc beendet
                if self.ed.mesh_edit.is_some() && (sc(Modifiers::NONE, Key::Delete) || sc(Modifiers::NONE, Key::Backspace)) {
                    self.ed.mesh_op(crate::mesh_edit::MeshOp::Delete);
                }
                if sc(Modifiers::NONE, Key::Escape) {
                    self.ed.grip_end();
                    self.ed.touch();
                }
                if sc(Modifiers::COMMAND, Key::A) {
                    if self.ed.mesh_edit.is_some() {
                        self.ed.mesh_op(crate::mesh_edit::MeshOp::SelectAll);
                    } else if let Some(p) = &mut self.ed.patch_edit {
                        p.sel = (0..16).collect();
                    }
                }
            }
            if self.ed.rot_live.is_some() && sc(Modifiers::NONE, Key::Escape) {
                self.ed.rot_live_cancel();
            }
            if sc(Modifiers::NONE, Key::Escape) {
                if self.ed.armed_prefab.take().is_none() && self.ed.pick_target.take().is_none() && self.ed.clip.take().is_none() && std::mem::take(&mut self.ed.clip3).is_empty() && self.ed.measure.take().is_none() {
                    self.ed.clear_selection();
                }
                self.ed.touch();
            }
            if self.ed.tool == Tool::Clip && sc(Modifiers::NONE, Key::Enter) {
                self.ed.apply_clip();
            }
            if sc(Modifiers::NONE, Key::Backspace) {
                self.ed.delete_selection();
            }
            // Kamera-Lesezeichen: Strg+Shift+1…9 speichern, Strg+1…9 anspringen
            let nums = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            for (n, k) in nums.iter().enumerate() {
                if sc(Modifiers::COMMAND | Modifiers::SHIFT, *k) {
                    self.save_bookmark(n);
                } else if sc(Modifiers::COMMAND, *k) {
                    self.load_bookmark(n);
                }
            }
        }
        if copy || cut {
            self.ed.copy();
            if !self.ed.sel.is_empty() {
                ctx.copy_text(self.ed.selection_vmf());
            }
        }
        if cut {
            self.ed.delete_selection();
        }
        if let Some(text) = paste_text {
            let g = self.ed.grid;
            match (!text.is_empty()).then(|| self.ed.paste_vmf(&text, DVec3::new(g, g, 0.0))) {
                Some(Ok(_)) => {}
                _ => self.ed.paste(),
            }
        }
        self.dispatch_shortcuts(ctx);
    }

    fn bookmark_key(&self) -> String {
        self.ed.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unbenannt".into())
    }

    fn save_bookmark(&mut self, n: usize) {
        let c = &self.view3d.cam;
        let v = [c.pos.x, c.pos.y, c.pos.z, c.yaw, c.pitch];
        let key = self.bookmark_key();
        let list = self.settings.bookmarks.entry(key).or_default();
        if list.len() <= n {
            list.resize(n + 1, None);
        }
        list[n] = Some(v);
        self.ed.status = format!("Kamera-Lesezeichen {} gespeichert (Strg+{} springt hin)", n + 1, n + 1);
    }

    fn load_bookmark(&mut self, n: usize) {
        let key = self.bookmark_key();
        match self.settings.bookmarks.get(&key).and_then(|l| l.get(n).copied().flatten()) {
            Some(v) => {
                self.view3d.cam.pos = glam::Vec3::new(v[0], v[1], v[2]);
                self.view3d.cam.yaw = v[3];
                self.view3d.cam.pitch = v[4];
                self.ed.status = format!("Kamera-Lesezeichen {}", n + 1);
            }
            None => self.ed.status = format!("Lesezeichen {} ist leer (Strg+Shift+{} speichert)", n + 1, n + 1),
        }
    }

    /// Für Bildschirmfotos und Tests: `AMBOSS_DEBUG_OPEN=compile,select:12,tool:block` öffnet Fenster direkt
    fn apply_debug_open(&mut self) {
        let Ok(list) = std::env::var("AMBOSS_DEBUG_OPEN") else { return };
        for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (k, v) = item.split_once(':').unwrap_or((item, ""));
            match k {
                "simple" => self.settings.simple_ui = true,
                "expert" => self.settings.simple_ui = false,
                "nostart" => self.win.start = false,
                "nosetup" => self.win.setup = false,
                "start" => self.win.start = true,
                "settings" => self.win.settings = true,
                "compile" => self.win.compile = true,
                "help" => self.win.help = true,
                "world_props" => self.win.world_props = true,
                "browser" => self.win.browser = true,
                "transform" => self.win.transform = true,
                "paste_special" => self.win.paste_special = true,
                "replace" => self.win.replace = true,
                "problems" => {
                    self.problems = crate::problems::check(&self.ed, self.known_materials().as_ref());
                    self.win.problems = true;
                }
                "entity_report" => self.win.entity_report = true,
                "map_info" => self.win.map_info = true,
                "goto" => self.win.goto = true,
                "prefabs" => self.win.prefabs = true,
                "models" => self.win.models = true,
                "sounds" => self.win.sounds = true,
                "logical" => self.win.logical = true,
                "outliner" => self.win.outliner = true,
                "history" => self.win.history = true,
                "palette" => self.win.palette = true,
                "optimize" => self.win.optimize = true,
                "collab" => self.win.collab = true,
                "lang" => {
                    let l = crate::i18n::Lang::from_code(v);
                    crate::i18n::set_lang(l);
                    self.settings.language = l.code().into();
                }
                "zoom" => {
                    if let Ok(z) = v.parse::<f32>() {
                        self.settings.ui_scale = z;
                        self.dlg.pending_zoom = Some(z);
                    }
                }
                "update_demo" => {
                    self.upd.available = Some(crate::update::Release { version: "0.10.1".into(), notes: "- Beispiel-Neuerung\n- Fehler behoben".into(), page: "https://github.com/Deadshotlp/Amboss/releases".into(), asset: Some(("amboss-windows.zip".into(), String::new())) });
                    self.upd.show_notes = true;
                }
                "collab_code" => {
                    self.win.collab = true;
                    self.collab_form.mode = 1;
                    self.collab_form.join_mode = 1;
                }
                "tutorial" => self.win.tutorial = true,
                "particles" => self.pb.open = true,
                "heightmap" => {
                    self.hm.open = true;
                    if v == "noise" {
                        self.hm.mode = 1;
                    }
                }
                "prop" => self.prop.open = true,
                "scripts" => self.scripts.open = true,
                "mated" => self.open_material_editor(Some(v)),
                "parted" => self.open_particle_file(if v.is_empty() { None } else { Some(PathBuf::from(v)) }),
                "light" => {
                    // Lichtvorschau, optional mit GI: light:gi
                    self.view3d.mode = crate::view3d::RenderMode::Lighting;
                    self.view3d.gi_on = v == "gi";
                }
                "scatter" => {
                    self.win.scatter = true;
                    if !v.is_empty() {
                        self.ed.scatter.models = vec![v.to_string()];
                    }
                }
                "scatterfill" => {
                    self.ed.scatter.models = vec![v.to_string()];
                    self.ed.scatter.density = 2.0;
                    self.ed.scatter_fill_selection();
                }
                "phys" => self.physics_start(),
                "procgen" => {
                    self.win.procgen = true;
                    self.gen_page = v.parse().unwrap_or(0);
                }
                "genseed" => self.gen.seed = v.parse().unwrap_or(1),
                "render" => {
                    if let Some(m) = crate::view3d::RenderMode::ALL.get(v.parse::<usize>().unwrap_or(0)) {
                        self.view3d.mode = *m;
                    }
                }
                "genmats" => {
                    // genmats:Boden;Wände;Decke
                    let m: Vec<&str> = v.split(';').collect();
                    if m.len() == 3 {
                        self.gen.floor_material = m[0].into();
                        self.gen.material = m[1].into();
                        self.gen.ceiling_material = m[2].into();
                    }
                }
                "gen" => {
                    // gen:N erzeugt sofort (0 Labyrinth, 1 Räume, 2 Stadt)
                    let g = self.gen.clone();
                    match v {
                        "0" => {
                            crate::procgen::maze(&mut self.ed, &g, glam::DVec3::ZERO);
                        }
                        "1" => {
                            crate::procgen::dungeon(&mut self.ed, &g, glam::DVec3::ZERO);
                        }
                        _ => {
                            crate::procgen::city(&mut self.ed, &g, glam::DVec3::ZERO);
                        }
                    }
                }
                "live" => self.win.live = true,
                "walk" => self.toggle_walk(),
                "clipdemo" => {
                    // Würfel, Clip-Werkzeug, drei Punkte gesetzt
                    let b = self.ed.map.make_box(crate::geometry::Aabb::new(glam::DVec3::ZERO, glam::DVec3::splat(256.0)), &self.ed.material.clone(), 0.25);
                    let id = b.id;
                    self.ed.map.world.solids.push(b.into());
                    self.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                    self.ed.tool = Tool::Clip;
                    for p in [glam::DVec3::new(0.0, 0.0, 192.0), glam::DVec3::new(0.0, 256.0, 192.0), glam::DVec3::new(256.0, 0.0, 64.0)] {
                        self.ed.clip3_add(p);
                    }
                }
                "skydemo" => {
                    // Test-Himmel, Boden, ein Seil, Nebel (fog:1)
                    self.ed.map.world.set("skyname", "amboss_test");
                    let b = self.ed.map.make_box(crate::geometry::Aabb::new(glam::DVec3::new(-1024.0, -1024.0, -16.0), glam::DVec3::new(1024.0, 1024.0, 0.0)), "DEV/DEV_MEASUREGENERIC01B", 0.25);
                    self.ed.map.world.solids.push(b.into());
                    for (k, (o, extra)) in [("-200 300 200", Some("seil2")), ("200 300 160", None)].iter().enumerate() {
                        let id = self.ed.map.alloc_id();
                        let mut e = crate::map::Entity::new(id, if k == 0 { "move_rope" } else { "keyframe_rope" });
                        e.set("origin", *o);
                        e.set("Slack", "60");
                        e.set("Width", "3");
                        match extra {
                            Some(n) => e.set("NextKey", *n),
                            None => e.set("targetname", "seil2"),
                        }
                        self.ed.map.entities.push(e.into());
                    }
                    if v == "fog" {
                        let id = self.ed.map.alloc_id();
                        let mut e = crate::map::Entity::new(id, "env_fog_controller");
                        e.set("origin", "0 0 64");
                        e.set("fogenable", "1");
                        e.set("fogcolor", "120 200 120");
                        e.set("fogstart", "100");
                        e.set("fogend", "700");
                        e.set("fogmaxdensity", "0.9");
                        self.ed.map.entities.push(e.into());
                    }
                    self.ed.touch();
                }
                "partdemo" => {
                    // drei Partikel-Effekte nebeneinander
                    for (k, name) in ["amboss_rauch", "amboss_funken", "amboss_feuer"].iter().enumerate() {
                        let id = self.ed.map.alloc_id();
                        let mut e = crate::map::Entity::new(id, "info_particle_system");
                        e.set("origin", format!("{} 0 0", k as i32 * 160 - 160));
                        e.set("effect_name", *name);
                        e.set("start_active", "1");
                        self.ed.map.entities.push(e.into());
                    }
                    self.ed.touch();
                }
                "meshdemo" => {
                    // Würfel → Mesh, Oberseite extrudiert und verschoben, Fenster offen
                    let b = self.ed.map.make_box(crate::geometry::Aabb::new(glam::DVec3::ZERO, glam::DVec3::new(256.0, 256.0, 128.0)), &self.ed.material.clone(), 0.25);
                    let id = b.id;
                    self.ed.map.world.solids.push(b.into());
                    self.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                    let _ = self.ed.mesh_from_selection();
                    if let Some(m) = &mut self.ed.mesh_edit {
                        m.faces_mode = true;
                    }
                    if let Some(top) = self.ed.grips().into_iter().max_by(|a, b| a.1.z.total_cmp(&b.1.z)) {
                        self.ed.grip_select(Some(top.0), false);
                        self.ed.mesh_op(crate::mesh_edit::MeshOp::Extrude(96.0));
                        if let Some(base) = self.ed.grip_base() {
                            self.ed.grip_drag(&base, glam::DVec3::new(64.0, 0.0, 0.0));
                        }
                    }
                    self.win.mesh = true;
                    self.mesh_page = 0;
                }
                "patchdemo" => {
                    let b = self.ed.map.make_box(crate::geometry::Aabb::new(glam::DVec3::ZERO, glam::DVec3::new(512.0, 512.0, 32.0)), &self.ed.material.clone(), 0.25);
                    let id = b.id;
                    self.ed.map.world.solids.push(b.into());
                    self.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                    if self.ed.patch_begin().is_ok() {
                        for i in [5, 6, 9, 10] {
                            self.ed.grip_select(Some(i), true);
                        }
                        if let Some(base) = self.ed.grip_base() {
                            self.ed.grip_drag(&base, glam::DVec3::Z * 220.0);
                        }
                        self.ed.grip_select(None, false);
                        self.ed.grip_select(Some(5), false);
                    }
                    self.win.mesh = true;
                    self.mesh_page = 1;
                }
                "tiles" => {
                    self.win.tiles = true;
                    self.ed.tiles.active = true;
                }
                "cam" => {
                    // cam:x;y;z;zielx;ziely;zielz
                    let f: Vec<f32> = v.split(';').filter_map(|x| x.trim().parse().ok()).collect();
                    if f.len() == 6 {
                        self.view3d.cam.pos = glam::Vec3::new(f[0], f[1], f[2]);
                        self.view3d.cam.look_at(glam::Vec3::new(f[3], f[4], f[5]));
                    }
                }
                "entity_help" => self.win.entity_help = Some(v.to_string()),
                "chapter" => self.start_chapter(v.parse().unwrap_or(0)),
                "chapter_step" => {
                    self.tut.step = v.parse().unwrap_or(0);
                }
                "setup" => {
                    self.win.setup = true;
                    self.setup.page = v.parse().unwrap_or(0);
                }
                "decompile" => self.ask_decompile(PathBuf::from(v)),
                "classpicker" => self.open_class_picker(egui::Rect::from_min_size(egui::pos2(380.0, 88.0), egui::vec2(220.0, 28.0))),
                "convert" => self.open_convert_dialog(),
                "panel_tab" => self.panel_tab = v.parse().unwrap_or(0),
                "import" => self.open_model_import(std::path::PathBuf::from(v)),
                "workshop" => self.open_workshop(),
                "spline" => {
                    let st = &mut self.ed.spline;
                    st.open = true;
                    st.points = vec![glam::DVec3::new(-900.0, -700.0, 1.0), glam::DVec3::new(-200.0, -500.0, 1.0), glam::DVec3::new(200.0, 100.0, 1.0), glam::DVec3::new(900.0, 400.0, 1.0)];
                    st.opts.kind = match v {
                        "pipe" => crate::spline::Kind::Pipe,
                        "fence" => crate::spline::Kind::Fence,
                        "models" => crate::spline::Kind::Models,
                        "cable" => crate::spline::Kind::Cable,
                        _ => crate::spline::Kind::Road,
                    };
                    if v == "pipe" || v == "cable" {
                        for p in &mut st.points {
                            p.z += 160.0;
                        }
                    }
                }
                "spline_go" => {
                    let _ = crate::spline::build(&mut self.ed, false);
                }
                "ws_shot" => self.ws.capture = 1,
                "ws_img" => self.ws.image = crate::model_import::load_image(&crate::model_import::Image::File(std::path::PathBuf::from(v))).ok(),
                "ws_go" => {
                    if let Err(e) = self.workshop_run_pub(v == "gma") {
                        self.ws.log.lock().unwrap().push(e);
                    }
                }
                "import_go" => {
                    let _ = self.import_run(true);
                }
                "template" => {
                    if let Some(t) = crate::templates::TEMPLATES.iter().find(|t| t.id == v) {
                        (t.build)(&mut self.ed);
                        self.ed.touch();
                    }
                }
                "hide" => {
                    self.ed.auto_vis = crate::editor::AutoVis::from_hidden_list(&v.replace('+', ","));
                    self.ed.touch();
                }
                "skycreate" => {
                    self.sky.opts.size = crate::skybox::suggest_size(&self.ed.map, 16.0);
                    self.sky.create_open = true;
                }
                "skycheck" => {
                    self.sky.issues = crate::skybox::check(&self.ed.map, self.ed.map_limit);
                    self.sky.check_open = true;
                }
                "sky3d" => {
                    // Beispiel-Skybox: Raum anlegen, ferne Berge und Türme verkleinert hineinkopieren, Modus an
                    let opts = crate::skybox::CreateOpts { scale: 16.0, size: crate::skybox::suggest_size(&self.ed.map, 16.0), ground: Some("DEV/DEV_MEASUREGENERIC01B".into()), copy_fog: true };
                    if crate::skybox::create(&mut self.ed, &opts).is_ok() {
                        let mut ids = Vec::new();
                        let w = crate::skybox::world_bounds(&self.ed.map, self.ed.sky_room_bounds().as_ref());
                        let c = w.center();
                        for (k, (dx, dy, sx, sy, h, mat)) in [(1.6, 0.2, 1500.0, 2200.0, 2400.0, "DEV/GRAYGRID"), (1.4, -1.1, 2600.0, 1400.0, 1700.0, "DEV/GRAYGRID"), (-1.5, 1.2, 1800.0, 1800.0, 3000.0, "DEV/GRAYGRID"), (0.2, 1.7, 500.0, 500.0, 4200.0, "DEV/DEV_MEASUREGENERIC01B"), (0.6, 1.7, 400.0, 400.0, 3200.0, "DEV/DEV_MEASUREGENERIC01B")].into_iter().enumerate() {
                            let ext = w.size().x.max(w.size().y).max(2048.0);
                            let o = glam::DVec3::new(c.x + dx * ext, c.y + dy * ext, w.min.z);
                            let b = crate::geometry::Aabb::new(o - glam::DVec3::new(sx / 2.0, sy / 2.0, 0.0), o + glam::DVec3::new(sx / 2.0, sy / 2.0, h));
                            let s = self.ed.map.make_box(b, mat, 0.25);
                            let _ = k;
                            ids.push(crate::editor::Obj::Solid(s.id));
                            self.ed.map.world.solids.push(s.into());
                        }
                        self.ed.sel = ids.iter().copied().collect();
                        let _ = crate::skybox::copy_selection_to_sky(&mut self.ed);
                        // Vorlagen wieder entfernen
                        for o in ids {
                            if let crate::editor::Obj::Solid(id) = o {
                                self.ed.map.world.solids.retain(|s| s.id != id);
                            }
                        }
                        self.ed.sel.clear();
                        self.ed.touch();
                        // Spielerkamera: am Rand der Map, Blick über die Map
                        self.view3d.cam.pos = glam::Vec3::new((w.min.x + 64.0) as f32, (w.min.y + 64.0) as f32, (w.min.z + 180.0) as f32);
                        self.view3d.cam.yaw = 35.0;
                        self.view3d.cam.pitch = 4.0;
                        if v != "nomode" {
                            self.toggle_sky_mode();
                        }
                    }
                }
                "fit" => {
                    // alles Ausgewählte in allen Ansichten einpassen
                    let b = self.ed.selection_bounds();
                    self.center_on(b);
                }
                "pick" => {
                    // pick:x;y;z wählt den kleinsten Brush, der diesen Punkt enthält
                    let f: Vec<f64> = v.split(';').filter_map(|x| x.trim().parse().ok()).collect();
                    if f.len() == 3 {
                        let p = glam::DVec3::new(f[0], f[1], f[2]);
                        let best = self.ed.map.world.solids.iter().filter(|s| (s.bounds.min - glam::DVec3::ONE).cmple(p).all() && p.cmple(s.bounds.max + glam::DVec3::ONE).all()).min_by(|a, b| a.bounds.size().length().total_cmp(&b.bounds.size().length())).map(|s| s.id);
                        self.ed.sel = best.map(crate::editor::Obj::Solid).into_iter().collect();
                    }
                }
                "select" => {
                    self.ed.sel = v.split(';').filter_map(|x| x.parse::<u32>().ok()).filter_map(|id| self.ed.find_by_id(id)).collect();
                }
                "tool" => {
                    let t = match v {
                        "select" => Tool::Select,
                        "magnify" => Tool::Magnify,
                        "camera" => Tool::Camera,
                        "entity" => Tool::Entity,
                        "block" => Tool::Block,
                        "face" => Tool::Face,
                        "decal" => Tool::Decal,
                        "overlay" => Tool::Overlay,
                        "clip" => Tool::Clip,
                        "vertex" => Tool::Vertex,
                        "disp" => Tool::Disp,
                        "cordon" => Tool::Cordon,
                        _ => Tool::Measure,
                    };
                    self.set_tool(t);
                }
                _ => eprintln!("AMBOSS_DEBUG_OPEN: unbekannt: {k}"),
            }
        }
    }

    pub(crate) fn set_tool(&mut self, t: Tool) {
        self.ed.tool = t;
        if t != Tool::Face && t != Tool::Disp {
            self.ed.faces.clear();
        }
        if t != Tool::Vertex {
            self.ed.vsel.clear();
        }
        self.ed.touch();
    }

    // -------------------------------------------------------------- Menüs

    /// Hauptmenüs (in der oberen Leiste)
    pub(crate) fn menus(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::i18n::tr;
        // Experten-Punkte erscheinen nur in der Experten-Oberfläche
        let ex = !self.settings.simple_ui;
        // Abstand Text ↔ Tastenkürzel mindestens 24 px
        let item = |ui: &mut egui::Ui, text: &str, sc: &str| {
            ui.spacing_mut().item_spacing.x = 24.0;
            ui.add(egui::Button::new(text).shortcut_text(crate::i18n::keys(sc))).clicked()
        };
        let item_if = |ui: &mut egui::Ui, on: bool, text: &str, sc: &str| {
            ui.spacing_mut().item_spacing.x = 24.0;
            ui.add_enabled(on, egui::Button::new(text).shortcut_text(crate::i18n::keys(sc))).clicked()
        };
        let has_sel = !self.ed.sel.is_empty();

        // ---------------------------------------------------------------- Datei
        ui.menu_button(tr("Datei"), |ui| {
            if item(ui, tr("Neu"), "Strg+N") {
                ui.close_menu();
                self.request(ctx, Pending::New);
            }
            if ui.button(tr("Neu aus Vorlage…")).clicked() {
                ui.close_menu();
                self.win.start = true;
            }
            ui.separator();
            if item(ui, tr("Öffnen… (VMF oder BSP)"), "Strg+O") {
                ui.close_menu();
                self.request(ctx, Pending::Open(None));
            }
            ui.menu_button(tr("Zuletzt geöffnet"), |ui| {
                if self.settings.recent_files.is_empty() {
                    ui.label(tr("(leer)"));
                }
                for r in self.settings.recent_files.clone() {
                    if ui.button(&r).clicked() {
                        ui.close_menu();
                        self.request(ctx, Pending::Open(Some(PathBuf::from(r))));
                    }
                }
            });
            if ui.button(tr("Autosave öffnen…")).clicked() {
                ui.close_menu();
                let p = rfd::FileDialog::new().set_directory(self.autosave_dir()).add_filter("VMF", &["vmf"]).pick_file();
                if p.is_some() {
                    self.request(ctx, Pending::Open(p));
                }
            }
            ui.separator();
            if item(ui, tr("Speichern"), "Strg+S") {
                ui.close_menu();
                self.save();
            }
            if item(ui, tr("Speichern unter…"), "Strg+Shift+S") {
                ui.close_menu();
                self.save_as();
            }
            ui.separator();
            ui.menu_button(tr("Importieren"), |ui| {
                self.cmd_item(ui, ctx, Cmd::ImportModel);
                if ui.button(tr("BSP dekompilieren…")).clicked() {
                    ui.close_menu();
                    self.start_decompile();
                }
                if ex {
                    self.cmd_item(ui, ctx, Cmd::ImportMap);
                }
            });
            if ex {
                ui.menu_button(tr("Exportieren"), |ui| {
                    self.cmd_item(ui, ctx, Cmd::ExportObj);
                    crate::theme::checkbox(ui, &mut self.settings.obj_meters, tr("OBJ in Metern (statt Source-Einheiten)"));
                });
            }
            ui.separator();
            self.cmd_item(ui, ctx, Cmd::NewTab);
            self.cmd_item(ui, ctx, Cmd::CloseTab);
            ui.separator();
            if ui.button(tr("Beenden")).clicked() {
                ui.close_menu();
                self.request(ctx, Pending::Quit);
            }
        });

        // ---------------------------------------------------------------- Bearbeiten
        ui.menu_button(tr("Bearbeiten"), |ui| {
            if item_if(ui, self.ed.can_undo(), tr("Rückgängig"), "Strg+Z") {
                self.ed.undo();
                ui.close_menu();
            }
            if item_if(ui, self.ed.can_redo(), tr("Wiederholen"), "Strg+Y") {
                self.ed.redo();
                ui.close_menu();
            }
            if ex {
                self.cmd_item(ui, ctx, Cmd::History);
            }
            ui.separator();
            if item_if(ui, has_sel, tr("Ausschneiden"), "Strg+X") {
                self.ed.copy();
                ctx.copy_text(self.ed.selection_vmf());
                self.ed.delete_selection();
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("Kopieren"), "Strg+C") {
                self.ed.copy();
                ctx.copy_text(self.ed.selection_vmf());
                ui.close_menu();
            }
            if item(ui, tr("Einfügen"), "Strg+V") {
                self.ed.paste();
                ui.close_menu();
            }
            if ex && item(ui, tr("Inhalte einfügen…"), "Strg+Shift+V") {
                self.win.paste_special = true;
                ui.close_menu();
            }
            self.cmd_item(ui, ctx, Cmd::Duplicate);
            if item_if(ui, has_sel, tr("Löschen"), "Entf") {
                self.ed.delete_selection();
                ui.close_menu();
            }
            ui.separator();
            if item(ui, tr("Alles auswählen"), "Strg+A") {
                self.ed.select_all();
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("Auswahl aufheben"), "Esc") {
                self.ed.clear_selection();
                ui.close_menu();
            }
            if ex && item(ui, tr("Gehe zu Brush/Entity-Nummer…"), "Strg+Shift+G") {
                self.win.goto = true;
                ui.close_menu();
            }
            ui.separator();
            if item(ui, tr("Aktuelle Textur anwenden"), "Shift+T") {
                self.ed.apply_material();
                ui.close_menu();
            }
            if ex && ui.button(tr("Texturen ersetzen…")).clicked() {
                self.win.replace = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Einstellungen…")).clicked() {
                self.win.settings = true;
                ui.close_menu();
            }
        });

        // ---------------------------------------------------------------- Ansicht
        ui.menu_button(tr("Ansicht"), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Oberfläche:")).weak());
                if ui.radio(self.settings.simple_ui, tr("Einfach")).clicked() {
                    self.settings.simple_ui = true;
                }
                if ui.radio(!self.settings.simple_ui, tr("Experte")).clicked() {
                    self.settings.simple_ui = false;
                }
            });
            ui.separator();
            ui.menu_button(tr("Fensteraufteilung"), |ui| {
                let names = [tr("Nur 3D"), tr("Nur Oben"), tr("Nur Vorn"), tr("Nur Seite")];
                if ui.radio(self.maximized.is_none(), tr("4 Ansichten")).clicked() {
                    self.maximized = None;
                    ui.close_menu();
                }
                for (i, n) in names.iter().enumerate() {
                    if ui.radio(self.maximized == Some(i), *n).clicked() {
                        self.maximized = Some(i);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button(tr("Layout zurücksetzen")).clicked() {
                    self.settings.layout = crate::compile::Layout::default();
                    self.apply_layout();
                    // gespeicherte Panelbreite in egui verwerfen
                    ctx.data_mut(|d| d.remove::<egui::containers::panel::PanelState>(egui::Id::new("props")));
                    ui.close_menu();
                }
            });
            ui.menu_button(tr("3D-Darstellung"), |ui| {
                for m in RenderMode::ALL {
                    if !ex && matches!(m, RenderMode::LightmapGrid | RenderMode::Smoothing) {
                        continue;
                    }
                    if ui.radio(self.view3d.mode == m, m.label()).clicked() {
                        self.view3d.mode = m;
                    }
                }
                ui.separator();
                crate::theme::checkbox(ui, &mut self.view3d.textured, tr("Texturen"));
                crate::theme::checkbox(ui, &mut self.view3d.models, tr("Modelle anzeigen"));
                crate::theme::checkbox(ui, &mut self.view3d.sky_preview, tr("Himmel-Vorschau (2D-Himmel und 3D-Skybox)"));
                crate::theme::checkbox(ui, &mut self.view3d.fog_preview, tr("Nebel (env_fog_controller)"));
                crate::theme::checkbox(ui, &mut self.view3d.helpers, tr("Helfer (Lichtkegel, Verbindungen)"));
                crate::theme::checkbox(ui, &mut self.view3d.particles.on, tr("Partikel-Effekte abspielen")).on_hover_text(tr("info_particle_system zeigt seinen Effekt (bis 3000 Einheiten um die Kamera, vereinfachte Simulation)"));
                crate::theme::checkbox(ui, &mut self.view3d.shadows, tr("Schatten in der Lichtvorschau"));
                crate::theme::checkbox(ui, &mut self.view3d.gi_on, tr("Indirektes Licht in der Lichtvorschau (GI)")).on_hover_text(tr("Von Wänden zurückgeworfenes Licht, wird im Hintergrund berechnet und laufend verfeinert"));
                if ex {
                    crate::theme::checkbox(ui, &mut self.view3d.collision, tr("Kollisionsmodelle der Props"));
                    crate::theme::checkbox(ui, &mut self.ed.dbrush.show_alpha, tr("Displacement-Alpha anzeigen"));
                }
                ui.separator();
                ui.add(crate::theme::sl(egui::Slider::new(&mut self.view3d.speed, 100.0..=3000.0)).text(tr("Kamera-Geschwindigkeit")));
                ui.add(crate::theme::sl(egui::Slider::new(&mut self.view3d.cam.fov, 50.0..=110.0)).text(tr("Sichtfeld")));
            });
            ui.menu_button(tr("Raster"), |ui| {
                if item(ui, tr("Raster verkleinern"), "[") {
                    self.ed.grid_down();
                }
                if item(ui, tr("Raster vergrößern"), "]") {
                    self.ed.grid_up();
                }
                if ex && crate::theme::checkbox(ui, &mut self.ed.vis_colors, tr("2D: Farben nach Visgroup")).changed() {
                    self.ed.touch();
                }
                crate::theme::checkbox(ui, &mut self.ed.dim_levels, tr("2D: bei Auswahl andere Ebenen abdunkeln")).on_hover_text(tr("Objekte, die nicht auf derselben Tiefe wie die Auswahl liegen (±10 Einheiten), werden blasser gezeichnet. Oben: gleiche Höhe (Z), Vorne: gleiches X, Seite: gleiches Y."));
            });
            ui.separator();
            if item_if(ui, has_sel, tr("2D auf Auswahl zentrieren"), "Strg+E") {
                self.center_on_selection(false);
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("3D auf Auswahl zentrieren"), "Strg+Shift+E") {
                self.center_on_selection(true);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(tr("Ausblenden & Sperren"), |ui| {
                if item_if(ui, has_sel, tr("Auswahl ausblenden"), "H") {
                    self.ed.hide_selection();
                    ui.close_menu();
                }
                if item_if(ui, has_sel, tr("Unausgewähltes ausblenden"), "Shift+H") {
                    self.ed.hide_unselected();
                    ui.close_menu();
                }
                if item(ui, tr("Alles einblenden"), "U") {
                    self.ed.unhide_all();
                    ui.close_menu();
                }
                ui.separator();
                self.cmd_item(ui, ctx, Cmd::Lock);
                self.cmd_item(ui, ctx, Cmd::UnlockAll);
            });
            ui.menu_button(tr("Einblenden (Filter)"), |ui| {
                ui.set_min_width(260.0);
                crate::sky_ui::filter_ui(&mut self.ed, ui, 1);
            });
            if crate::theme::checkbox(ui, &mut self.sky.active.clone(), tr("3D-Skybox bearbeiten")).on_hover_text(tr("Nur den Raum um die sky_camera zeigen, mit Spielersicht; ohne Skybox startet der Assistent")).clicked() {
                self.toggle_sky_mode();
                ui.close_menu();
            }
            self.cmd_item(ui, ctx, Cmd::Outliner);
            if ex && item(ui, tr("Logische Ansicht (Entity-I/O)"), "") {
                self.win.logical = true;
                ui.close_menu();
            }
            self.cmd_item(ui, ctx, Cmd::Palette);
            crate::theme::checkbox(ui, &mut self.view3d.gizmo, tr("3D-Gizmo (Verschieben/Drehen)"));
        });

        // ---------------------------------------------------------------- Objekt
        ui.menu_button(tr("Objekt"), |ui| {
            if item_if(ui, has_sel, tr("Gruppieren"), "Strg+G") {
                self.ed.group_selection();
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("Gruppe auflösen"), "Strg+U") {
                self.ed.ungroup_selection();
                ui.close_menu();
            }
            ui.separator();
            if item_if(ui, has_sel, tr("In Entity umwandeln…"), "Strg+T") {
                self.open_convert_dialog();
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("In Welt verschieben"), "Strg+Shift+W") {
                self.ed.move_to_world();
                ui.close_menu();
            }
            if item_if(ui, has_sel, tr("Mit Physik fallen lassen"), "") {
                self.physics_start();
                ui.close_menu();
            }
            ui.separator();
            ui.add_enabled_ui(has_sel, |ui| {
                ui.menu_button(tr("Transformieren"), |ui| {
                    if item(ui, tr("Transformieren…"), "Strg+M") {
                        self.win.transform = true;
                        ui.close_menu();
                    }
                    if item(ui, tr("Am Raster ausrichten"), "Strg+B") {
                        self.ed.snap_selection_to_grid();
                        ui.close_menu();
                    }
                    let (h, v, d) = self.view_axes();
                    if item(ui, tr("Horizontal spiegeln"), "Strg+L") {
                        self.ed.flip_selection(h);
                        ui.close_menu();
                    }
                    if item(ui, tr("Vertikal spiegeln"), "Strg+I") {
                        self.ed.flip_selection(v);
                        ui.close_menu();
                    }
                    if item(ui, tr("90° drehen"), "Strg+R") {
                        self.ed.rotate_selection(d, -90.0);
                        ui.close_menu();
                    }
                    if ex {
                        ui.menu_button(tr("Objekte ausrichten"), |ui| {
                            for (axis, name) in [(0, "X"), (1, "Y"), (2, "Z")] {
                                if ui.button(crate::i18n::tr_dyn(&format!("{name}: an Minimum"))).clicked() {
                                    self.ed.align_selection(axis, false);
                                    ui.close_menu();
                                }
                                if ui.button(crate::i18n::tr_dyn(&format!("{name}: an Maximum"))).clicked() {
                                    self.ed.align_selection(axis, true);
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
                ui.menu_button(tr("Brush-Operationen"), |ui| {
                    if item(ui, tr("Aushöhlen"), "Strg+H") {
                        self.ed.hollow(self.hollow_thickness);
                        ui.close_menu();
                    }
                    if item(ui, tr("Carve"), "Strg+Shift+C") {
                        self.ed.carve();
                        ui.close_menu();
                    }
                    self.cmd_item(ui, ctx, Cmd::UnifyTextures);
                    if ex {
                        self.cmd_item(ui, ctx, Cmd::NodrawHidden);
                    }
                });
            });
            if ex {
                ui.menu_button(tr("Instanzen"), |ui| {
                    if item(ui, tr("Instanzen einklappen"), "") {
                        self.ed.collapse_instances();
                        ui.close_menu();
                    }
                    if item(ui, tr("Instanz in Tab öffnen"), "") {
                        ui.close_menu();
                        let files = self.ed.selected_instance_files();
                        let chosen = !self.ed.selected_entity_ids().is_empty();
                        match files.len() {
                            0 => self.ed.status = "Keine func_instance mit gefundener Datei ausgewählt".into(),
                            n if n > 1 && !chosen => self.ed.status = format!("{n} Instanzen in der Map – bitte eine func_instance auswählen"),
                            _ => self.open_in_tab(files[0].clone()),
                        }
                    }
                    if item(ui, tr("Instanzen neu laden"), "") {
                        ui.close_menu();
                        self.ed.reload_instances();
                    }
                });
            }
            ui.separator();
            if item_if(ui, has_sel, tr("Als Prefab speichern…"), "") {
                self.win.prefabs = true;
                ui.close_menu();
            }
            if ex && item_if(ui, has_sel, tr("Auswahl als Modell (Prop)…"), "") {
                self.prop.open = true;
                ui.close_menu();
            }
        });

        // ---------------------------------------------------------------- Werkzeuge
        ui.menu_button(tr("Werkzeuge"), |ui| {
            ui.menu_button(tr("Browser"), |ui| {
                if ui.button(tr("Prefabs…")).clicked() {
                    self.win.prefabs = true;
                    ui.close_menu();
                }
                if ui.button(tr("Modell-Browser…")).clicked() {
                    self.dlg.browse_target = None;
                    self.win.models = true;
                    ui.close_menu();
                }
                if ui.button(tr("Sound-Browser…")).clicked() {
                    self.dlg.browse_target = None;
                    self.win.sounds = true;
                    ui.close_menu();
                }
                if ex && ui.button(tr("Partikel-Browser…")).clicked() {
                    self.pb.target = None;
                    self.pb.open = true;
                    ui.close_menu();
                }
            });
            ui.menu_button(tr("Navmesh"), |ui| {
                if ui.button(tr("Navmesh erzeugen")).on_hover_text(tr("Begehbare Bereiche ab den Spawnpunkten berechnen und als maps/<map>.nav speichern (für NextBots)")).clicked() {
                    self.start_navmesh();
                    ui.close_menu();
                }
                if ui.button(tr("Navmesh laden (.nav)…")).clicked() {
                    ui.close_menu();
                    self.load_navmesh();
                }
                if ui.scope(|ui| { crate::theme::box_style(ui); ui.add_enabled(self.ed.nav_preview.is_some(), egui::Checkbox::new(&mut self.ed.nav_show, tr("Navmesh anzeigen"))) }).inner.changed() {
                    self.ed.touch();
                }
            });
            ui.separator();
            if ui.button(tr("Material-Editor…")).on_hover_text(tr("Eigene Materialien anlegen, Bilder als Textur importieren")).clicked() {
                let cur = self.ed.material.clone();
                self.open_material_editor(Some(&cur));
                ui.close_menu();
            }
            if ui.button(tr("Prozedurale Generatoren…")).on_hover_text(tr("Labyrinth, Räume mit Gängen, Stadtblock, Kreis-Anordnung")).clicked() {
                self.win.procgen = true;
                ui.close_menu();
            }
            if ui.button(tr("Tile-Set…")).on_hover_text(tr("Bausteine auf einem Raster setzen: Boden, Wand, Ecke, Tür, Treppe …")).clicked() {
                self.win.tiles = true;
                ui.close_menu();
            }
            self.cmd_item(ui, ctx, Cmd::Spline);
            if ui.button(tr("Polygon-Mesh…")).on_hover_text(tr("Freie Netze: aus Brushes erstellen oder OBJ importieren, Punkte und Flächen ziehen, extrudieren")).clicked() {
                self.win.mesh = true;
                self.mesh_page = 0;
                if let Some(id) = self.ed.selected_mesh() {
                    self.ed.mesh_begin(id);
                }
                ui.close_menu();
            }
            if ui.button(tr("Bezier-Patch…")).on_hover_text(tr("Eine Displacement-Fläche mit 16 Kontrollpunkten glatt biegen")).clicked() {
                self.win.mesh = true;
                self.mesh_page = 1;
                ui.close_menu();
            }
            if ui.button(tr("Vegetation streuen…")).on_hover_text(tr("Bäume, Büsche, Steine mit einem Pinsel in 3D verteilen")).clicked() {
                self.win.scatter = true;
                ui.close_menu();
            }
            if ui.button(tr("Partikel-Editor…")).on_hover_text(tr("Partikeleffekte (.pcf) anlegen und bearbeiten, mit Vorschau")).clicked() {
                if self.parted.dmx.elements.is_empty() {
                    self.open_particle_file(None);
                }
                self.parted.open = true;
                ui.close_menu();
            }
            if ex && ui.button(tr("Gelände aus Heightmap…")).clicked() {
                self.hm.open = true;
                ui.close_menu();
            }
            if ex && ui.button(tr("Lua-Skripte…")).clicked() {
                self.scripts.open = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Live-Sitzung…")).clicked() {
                self.win.collab = true;
                ui.close_menu();
            }
            if ex {
                ui.menu_button(tr("Versionen & Zusammenführen"), |ui| {
                    self.cmd_item(ui, ctx, Cmd::MergeMap);
                    self.cmd_item(ui, ctx, Cmd::CompareMap);
                    self.cmd_item(ui, ctx, Cmd::Merge3);
                });
            }
        });

        // ---------------------------------------------------------------- Map
        ui.menu_button(tr("Map"), |ui| {
            if ui.button(tr("Map-Eigenschaften (worldspawn)…")).clicked() {
                self.win.world_props = true;
                ui.close_menu();
            }
            if ui.button(tr("Map-Info…")).clicked() {
                self.win.map_info = true;
                ui.close_menu();
            }
            ui.separator();
            if item(ui, tr("Auf Probleme prüfen…"), "Alt+P") {
                self.problems = crate::problems::check(&self.ed, self.known_materials().as_ref());
                self.win.problems = true;
                ui.close_menu();
            }
            if ex && item(ui, tr("Entity-Report…"), "Strg+Shift+F") {
                self.win.entity_report = true;
                ui.close_menu();
            }
            ui.menu_button(tr("3D-Skybox"), |ui| {
                let has = !crate::skybox::sky_cameras(&self.ed.map).is_empty();
                if ui.button(if self.sky.active { tr("Skybox-Modus beenden") } else if has { tr("Bearbeiten") } else { tr("Anlegen…") }).clicked() {
                    self.toggle_sky_mode();
                    ui.close_menu();
                }
                let has_sel = !self.ed.sel.is_empty();
                if ui.add_enabled(has && has_sel, egui::Button::new(tr("Auswahl verkleinert hineinkopieren"))).on_hover_text(tr("Brushes der Auswahl im Maßstab der sky_camera an die passende Stelle der Skybox kopieren")).clicked() {
                    self.sky_copy_selection();
                    ui.close_menu();
                }
                if ui.button(tr("Prüfen…")).clicked() {
                    self.sky.issues = crate::skybox::check(&self.ed.map, self.ed.map_limit);
                    self.sky.check_open = true;
                    ui.close_menu();
                }
            });
            if ex {
                ui.separator();
                let cordon_active = self.ed.map.cordon.is_some_and(|c| c.active);
                if ui.add_enabled(self.ed.map.cordon.is_some(), egui::Button::new(if cordon_active { tr("Cordon deaktivieren") } else { tr("Cordon aktivieren") })).on_disabled_hover_text(crate::i18n::keys(tr("Lege zuerst mit dem Cordon-Werkzeug (Shift+K) einen Bereich fest."))).clicked() {
                    if let Some(c) = self.ed.map.cordon.as_mut() {
                        c.active = !c.active;
                    }
                    self.ed.dirty = true;
                    self.ed.touch();
                    ui.close_menu();
                }
            }
        });

        // ---------------------------------------------------------------- Kompilieren
        ui.menu_button(tr("Kompilieren"), |ui| {
            self.cmd_item(ui, ctx, crate::commands::Cmd::CompileWindow);
            if item(ui, tr("Sofort kompilieren & starten"), "F9") {
                ui.close_menu();
                self.start_compile();
            }
            self.cmd_item(ui, ctx, crate::commands::Cmd::TestFromCamera);
            self.cmd_item(ui, ctx, crate::commands::Cmd::Walk);
            self.cmd_item(ui, ctx, crate::commands::Cmd::Workshop);
            if ui.button(tr("Live-Abgleich mit dem Spiel…")).on_hover_text(tr("Änderungen sofort im laufenden Garry's Mod sehen, ohne neu zu kompilieren")).clicked() {
                self.win.live = true;
                ui.close_menu();
            }
            if ex && ui.button(tr("Automatische Optimierungen…")).clicked() {
                self.win.optimize = true;
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(tr("Leck-Suche"), |ui| {
                if ui.button(tr("Pointfile laden (.lin)…")).clicked() {
                    ui.close_menu();
                    self.load_pointfile();
                }
                if ui.add_enabled(!self.ed.pointfile.is_empty(), egui::Button::new(tr("Kamera zum Leck"))).clicked() {
                    let p = self.ed.pointfile[0].as_vec3();
                    let q = self.ed.pointfile.get(1).copied().unwrap_or_default().as_vec3();
                    self.view3d.cam.pos = p;
                    self.view3d.cam.look_at(q);
                    ui.close_menu();
                }
                if ui.add_enabled(!self.ed.pointfile.is_empty(), egui::Button::new(tr("Pointfile entladen"))).clicked() {
                    self.ed.pointfile.clear();
                    self.ed.touch();
                    ui.close_menu();
                }
                if ex {
                    ui.separator();
                    if ui.button(tr("Portalfile laden (.prt)…")).clicked() {
                        ui.close_menu();
                        self.load_portalfile();
                    }
                    if ui.add_enabled(!self.ed.portals.is_empty(), egui::Button::new(tr("Portalfile entladen"))).clicked() {
                        self.ed.portals.clear();
                        self.ed.touch();
                        ui.close_menu();
                    }
                }
            });
        });

        // ---------------------------------------------------------------- Hilfe
        ui.menu_button(tr("Hilfe"), |ui| {
            if ui.button(tr("Tutorial…")).clicked() {
                self.win.tutorial = true;
                ui.close_menu();
            }
            if item(ui, tr("Tastenkürzel & Bedienung"), "F1") {
                self.win.help = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Einrichtungs-Assistent…")).clicked() {
                self.win.setup = true;
                ui.close_menu();
            }
            if ui.button(tr("Startbildschirm")).clicked() {
                self.win.start = true;
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Einstellungsordner öffnen")).on_hover_text(tr("Ordner mit Einstellungen, Autosaves und grafik.txt")).clicked() {
                if let Some(d) = eframe::storage_dir("Amboss") {
                    open_folder(&d);
                }
                ui.close_menu();
            }
            if ui.button(tr("Nach Updates suchen")).clicked() {
                self.start_update_check(true);
                ui.close_menu();
            }
            if ui.button(tr("Über Amboss…")).clicked() {
                self.win.about = true;
                ui.close_menu();
            }
        });
    }

    pub(crate) fn load_pointfile(&mut self) {
        let mut d = rfd::FileDialog::new().add_filter("Pointfile", &["lin"]);
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        if let Some(p) = d.pick_file() {
            match crate::pointfile::load_lin(&p) {
                Ok(pts) => {
                    self.ed.pointfile = pts;
                    self.ed.touch();
                }
                Err(e) => self.ed.status = e,
            }
        }
    }

    fn load_portalfile(&mut self) {
        let mut d = rfd::FileDialog::new().add_filter("Portalfile", &["prt"]);
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        if let Some(p) = d.pick_file() {
            match crate::pointfile::load_prt(&p) {
                Ok(pts) => {
                    self.ed.status = format!("{} Portale geladen", pts.len());
                    self.ed.portals = pts;
                    self.ed.touch();
                }
                Err(e) => self.ed.status = e,
            }
        }
    }

    // -------------------------------------------------------------- Ansichten

    fn viewports(&mut self, ctx: &egui::Context) {
        let th = crate::theme::t();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(th.bg).inner_margin(3)).show(ctx, |ui| {
            let full = ui.max_rect();
            self.ui_rects.insert("viewports", full);
            let gap = 3.0;
            let [sx, sy] = self.settings.layout.split.map(|v| v.clamp(0.15, 0.85));
            let wl = ((full.width() - gap) * sx).round();
            let ht = ((full.height() - gap) * sy).round();
            let quad = |i: usize| {
                let (x, w) = if i % 2 == 0 { (full.left(), wl) } else { (full.left() + wl + gap, full.width() - gap - wl) };
                let (y, h) = if i < 2 { (full.top(), ht) } else { (full.top() + ht + gap, full.height() - gap - ht) };
                egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(w, h))
            };
            let mut toggle = [false; 4];
            self.cursor = None;
            self.hover_3d = false;
            self.hovered_view = None;
            let pointer = ctx.input(|i| i.pointer.hover_pos());
            let pressed = ctx.input(|i| i.pointer.any_pressed());
            let mut rects: Vec<(usize, egui::Rect)> = Vec::new();
            for i in 0..4 {
                let rect = match self.maximized {
                    Some(m) if m == i => full,
                    Some(_) => continue,
                    None => quad(i),
                };
                rects.push((i, rect));
                match i {
                    0 => {
                        self.ui_rects.insert("view3d", rect);
                    }
                    1 => {
                        self.ui_rects.insert("view_top", rect);
                    }
                    _ => {}
                }
                if pointer.is_some_and(|p| rect.contains(p)) {
                    self.hovered_view = Some(i);
                    if pressed {
                        self.active_view = Some(i);
                    }
                }
                let tv = Instant::now();
                if i == 0 {
                    if self.view3d.controls != self.settings.cam {
                        self.view3d.controls = self.settings.cam.clone();
                    }
                    self.view3d.taken = crate::commands::Cmd::all().into_iter().filter_map(|c| crate::commands::parse_shortcut(&self.key_for(c))).filter(|sc| sc.modifiers.shift || sc.modifiers.alt).collect();
                    self.view3d.show(ui, rect, &mut self.ed, &mut self.mats, &mut self.models, &mut toggle[0]);
                    self.hover_3d = self.hovered_view == Some(0);
                } else {
                    let v = &mut self.views2d[i - 1];
                    if let Some(p) = v.show(ui, rect, &mut self.ed, &mut toggle[i]) {
                        self.cursor = Some((v.axis, p));
                    }
                }
                if std::env::var_os("AMBOSS_PROFILE").is_some() {
                    eprintln!("view{i}_ms {:.1}", tv.elapsed().as_secs_f64() * 1000.0);
                }
            }
            // Rahmen, Rundung und Schilder über den Ansichten
            let painter = ui.painter().clone();
            for (i, rect) in rects {
                round_corners(&painter, rect, 7.0, th.bg);
                if self.active_view == Some(i) && self.maximized.is_none() {
                    painter.rect_stroke(rect.shrink(0.75), 7.0, egui::Stroke::new(1.5f32, th.accent), egui::StrokeKind::Middle);
                }
                if !self.capturing() {
                    self.viewport_overlay(ui, i, rect, &mut toggle[i]);
                }
            }
            for (i, t) in toggle.iter().enumerate() {
                if *t {
                    self.maximized = if self.maximized.is_some() { None } else { Some(i) };
                }
            }
            // Trenner zwischen den Ansichten ziehen (Doppelklick = Mitte)
            if self.maximized.is_none() {
                let vx = full.left() + wl + gap / 2.0;
                let hy = full.top() + ht + gap / 2.0;
                let bars = [
                    (egui::Rect::from_center_size(egui::pos2(vx, hy), Vec2::splat(14.0)), true, true, egui::CursorIcon::ResizeNwSe),
                    (egui::Rect::from_x_y_ranges(vx - 4.0..=vx + 4.0, full.y_range()), true, false, egui::CursorIcon::ResizeHorizontal),
                    (egui::Rect::from_x_y_ranges(full.x_range(), hy - 4.0..=hy + 4.0), false, true, egui::CursorIcon::ResizeVertical),
                ];
                let mut taken = false;
                for (k, (r, mx, my, cursor)) in bars.into_iter().enumerate() {
                    let resp = ui.interact(r, ui.id().with(("vsplit", k)), egui::Sense::click_and_drag());
                    if taken {
                        continue;
                    }
                    if resp.hovered() || resp.dragged() {
                        ctx.set_cursor_icon(cursor);
                        taken = true;
                    }
                    if resp.double_clicked() {
                        self.settings.layout.split = [0.5, 0.5];
                    } else if resp.dragged() {
                        if let Some(p) = resp.interact_pointer_pos() {
                            if mx {
                                self.settings.layout.split[0] = ((p.x - full.left()) / full.width()).clamp(0.15, 0.85);
                            }
                            if my {
                                self.settings.layout.split[1] = ((p.y - full.top()) / full.height()).clamp(0.15, 0.85);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Glas-Schild oben links (Ansicht/Darstellung), Maximieren oben rechts, Zoom bzw. FPS unten rechts
    fn viewport_overlay(&mut self, ui: &mut egui::Ui, i: usize, rect: egui::Rect, toggle: &mut bool) {
        use crate::icons;
        use crate::theme::{mono, sans};
        let th = crate::theme::t();
        // 3D bleibt immer dunkel: dort dunkles Glas
        let (glass, text, text2, line) = if i == 0 { (crate::theme::Theme::DARK.glass, crate::theme::Theme::DARK.text, crate::theme::Theme::DARK.text2, crate::theme::Theme::DARK.line) } else { (th.glass, th.text, th.text2, th.line) };
        let label = if i == 0 { format!("3D · {}", self.view3d.mode.label()) } else { self.views2d[i - 1].axis.name().to_string() };
        let g = ui.painter().layout_no_wrap(label, sans(12.0), text);
        let sr = egui::Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), Vec2::new(g.size().x + 34.0, 26.0));
        let resp = ui.interact(sr, ui.id().with(("vpshield", i)), egui::Sense::click());
        ui.painter().rect(sr, 7.0, if resp.hovered() { crate::theme::alpha(glass, 1.0) } else { glass }, egui::Stroke::new(1.0f32, line), egui::StrokeKind::Inside);
        ui.painter().galley(egui::pos2(sr.left() + 10.0, sr.center().y - g.size().y / 2.0), g, text);
        icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(sr.right() - 13.0, sr.center().y), Vec2::splat(12.0)), "chevron", text2, 2.2);
        let pop = ui.make_persistent_id(("vppop", i));
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(pop));
        }
        egui::popup::popup_below_widget(ui, pop, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(190.0);
            if i == 0 {
                crate::theme::section(ui, crate::i18n::tr("Darstellung"));
                for m in RenderMode::ALL {
                    if ui.selectable_label(self.view3d.mode == m, m.label()).clicked() {
                        self.view3d.mode = m;
                    }
                }
                ui.separator();
                crate::theme::checkbox(ui, &mut self.view3d.textured, crate::i18n::tr("Texturen"));
                crate::theme::checkbox(ui, &mut self.view3d.models, crate::i18n::tr("Modelle"));
                crate::theme::checkbox(ui, &mut self.view3d.collision, crate::i18n::tr("Kollision"));
                crate::theme::checkbox(ui, &mut self.view3d.shadows, crate::i18n::tr("Schatten (Lichtvorschau)"));
                crate::theme::checkbox(ui, &mut self.view3d.gi_on, crate::i18n::tr("Indirektes Licht (GI)")).on_hover_text(crate::i18n::tr("Von Wänden zurückgeworfenes Licht, wird im Hintergrund berechnet und laufend verfeinert"));
                crate::theme::checkbox(ui, &mut self.view3d.sky_preview, crate::i18n::tr("Himmel"));
                crate::theme::checkbox(ui, &mut self.view3d.fog_preview, crate::i18n::tr("Nebel"));
                crate::theme::checkbox(ui, &mut self.view3d.helpers, crate::i18n::tr("Helfer"));
                crate::theme::checkbox(ui, &mut self.view3d.particles.on, crate::i18n::tr("Partikel"));
            } else {
                crate::theme::section(ui, crate::i18n::tr("Ansicht"));
                for a in [Axis2D::Top, Axis2D::Front, Axis2D::Side] {
                    if ui.selectable_label(self.views2d[i - 1].axis == a, a.name()).clicked() {
                        self.views2d[i - 1].axis = a;
                    }
                }
            }
        });
        // Ansichtsfilter (Trichter): hervorgehoben, wenn etwas ausgeblendet ist
        let fr = egui::Rect::from_min_size(egui::pos2(sr.right() + 6.0, sr.top()), Vec2::splat(26.0));
        let fresp = ui.interact(fr, ui.id().with(("vpfilter", i)), egui::Sense::click());
        let filtered = !self.ed.auto_vis.all_visible();
        let (accent, accent_fg) = (crate::theme::t().accent, crate::theme::t().accent_fg);
        if filtered || fresp.hovered() || self.hovered_view == Some(i) {
            ui.painter().rect(fr, 7.0, if filtered { accent } else { glass }, egui::Stroke::new(1.0f32, if filtered { accent } else { line }), egui::StrokeKind::Inside);
            crate::icons::draw(ui.painter(), fr.shrink(6.0), "filter", if filtered { accent_fg } else { text2 }, 2.0);
        }
        let fpop = ui.make_persistent_id(("vpfilterpop", i));
        if fresp.on_hover_text(if filtered { crate::i18n::tr("Einblenden – es ist etwas ausgeblendet") } else { crate::i18n::tr("Einblenden (Filter)") }).clicked() {
            ui.memory_mut(|m| m.toggle_popup(fpop));
        }
        let fanchor = ui.interact(fr, ui.id().with(("vpfilteranchor", i)), egui::Sense::hover());
        egui::popup::popup_below_widget(ui, fpop, &fanchor, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(250.0);
            crate::theme::section(ui, crate::i18n::tr("Einblenden"));
            crate::sky_ui::filter_ui(&mut self.ed, ui, 1);
        });
        // Maximieren
        let mr = egui::Rect::from_min_size(egui::pos2(rect.right() - 34.0, rect.top() + 8.0), Vec2::splat(26.0));
        let mresp = ui.interact(mr, ui.id().with(("vpmax", i)), egui::Sense::click());
        if mresp.hovered() || self.hovered_view == Some(i) {
            ui.painter().rect(mr, 7.0, glass, egui::Stroke::new(1.0f32, line), egui::StrokeKind::Inside);
            icons::draw(ui.painter(), mr.shrink(6.0), "maximize", text2, 2.0);
        }
        if mresp.on_hover_text(if self.maximized.is_some() { crate::i18n::tr("Vier Ansichten (Shift+Z)") } else { crate::i18n::tr("Ansicht maximieren (Shift+Z)") }).clicked() {
            *toggle = true;
        }
        // Zoom bzw. FPS
        let info = if i == 0 {
            let dt = ui.input(|inp| inp.stable_dt).max(1e-3);
            match self.view3d.gi_status() {
                Some(g) => format!("{g} · {:.0} FPS", (1.0 / dt).min(999.0)),
                None => format!("{:.0} FPS", (1.0 / dt).min(999.0)),
            }
        } else {
            let z = self.views2d[i - 1].zoom;
            if z >= 1.0 { format!("{} : 1", crate::i18n::dec(crate::map::fmt_num((z * 10.0).round() / 10.0))) } else { format!("1 : {}", crate::i18n::dec(crate::map::fmt_num((10.0 / z).round() / 10.0))) }
        };
        let g = ui.painter().layout_no_wrap(info, mono(10.5), text2);
        let ir = egui::Rect::from_min_size(rect.right_bottom() - Vec2::new(g.size().x + 24.0, 30.0), Vec2::new(g.size().x + 16.0, 22.0));
        ui.painter().rect_filled(ir, 6.0, glass);
        ui.painter().galley(ir.center() - g.size() / 2.0, g, text2);
    }

    /// Hintergrund-Ergebnisse übernehmen (Texturen, Modelle, Instanzen)
    fn background(&mut self, ctx: &egui::Context) {
        self.mats.poll(ctx);
        // Spielinhalte, die im Hintergrund eingelesen wurden, übernehmen
        if let Some(rx) = &self.content_rx {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    let ContentMsg { final_stage, found, fs, materials: list, sounds, models } = msg;
                    if final_stage {
                        self.content_rx = None;
                    }
                    if let Some(root) = found.filter(|_| self.content_stage == 0) {
                        // automatisch gefunden: alle Pfade (Compiler, Spiel, FGD) übernehmen
                        self.settings.apply_root(&root);
                        self.settings.store_profile();
                        self.reload_fgds();
                    }
                    if self.content_stage == 0 && self.settings.detect_fgd() {
                        // echte FGD gefunden: hat Vorrang vor der eingebauten Liste
                        self.settings.store_profile();
                        self.reload_fgds();
                    }
                    let msgs = fs.messages.clone();
                    let (vpks, gmas) = (fs.vpks.len(), fs.gmas.len());
                    if self.content_stage == 0 {
                        // erste Stufe: neu aufsetzen
                        self.mats.set_fs_prepared(fs, list);
                        self.models.set_fs_prepared(self.mats.fs(), models);
                        self.ed.model_bounds.clear();
                        self.ed.model_tris.clear();
                        self.ed.model_edges.clear();
                        self.view3d.clear_content();
                    } else {
                        // zweite Stufe: Addons dazu, bereits Geladenes bleibt erhalten
                        self.mats.update_fs(fs, list);
                        self.models.update_fs(self.mats.fs(), models);
                    }
                    self.content_stage += 1;
                    self.dlg.sound_list = sounds;
                    self.ed.touch();
                    let n = self.mats.list.len();
                    self.ed.status = if self.settings.game_dir.is_empty() {
                        crate::i18n::tr("Kein Spielordner eingestellt – Bearbeiten → Einstellungen → „Automatisch suchen“ (oder…/GarrysMod/garrysmod eintragen)").into()
                    } else if n == 0 && final_stage {
                        let why = msgs.first().cloned().unwrap_or_else(|| format!("keine VPK-Dateien und kein materials-Ordner in {}", self.settings.game_dir));
                        format!("Keine Materialien gefunden: {why}. Der Spielordner muss…/GarrysMod/garrysmod sein.")
                    } else {
                        // echte Mehrzahl statt „(s)“ – die Teile werden einzeln übersetzt
                        let vp = if vpks == 1 { "1 VPK".to_string() } else { format!("{vpks} VPKs") };
                        let ad = if gmas == 1 { "1 Addon".to_string() } else { format!("{gmas} Addons") };
                        let mats = crate::chrome::fmt_thousands(n);
                        if !final_stage {
                            format!("Spielinhalte: {mats} Materialien aus {vp} – Addons werden noch eingelesen…")
                        } else if !self.settings.load_addons {
                            format!("Spielinhalte geladen: {mats} Materialien aus {vp} – Addons ausgeschaltet (Einstellungen)")
                        } else {
                            format!("Spielinhalte geladen: {mats} Materialien aus {vp} und {ad}")
                        }
                    };
                    self.content_status = self.ed.status.clone();
                    ctx.request_repaint();
                }
                Ok(Err(e)) => {
                    self.content_rx = None;
                    self.ed.status = format!("Spielinhalte konnten nicht geladen werden: {e}");
                    self.content_status = self.ed.status.clone();
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.content_rx = None;
                    self.ed.status = crate::i18n::tr("Spielinhalte konnten nicht geladen werden (Lade-Thread beendet)").into();
                    self.content_status = self.ed.status.clone();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        // Neu geladene Modelle: nur Grenzen übernehmen. Kein touch(), sonst würde bei vielen Props
        // ständig alles neu gebaut; 2D/3D bauen gedrosselt über models.generation neu.
        if self.models.poll() {
            for m in std::mem::take(&mut self.models.fresh) {
                self.ed.model_bounds.insert(m.name.clone(), m.bounds);
                let mut tris = Vec::new();
                for mesh in &m.meshes {
                    let v = |i: u32| mesh.verts.get(i as usize).map(|v| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
                    for t in mesh.indices.chunks(3) {
                        if let (Some(a), Some(b), Some(c)) = (t.first().and_then(|i| v(*i)), t.get(1).and_then(|i| v(*i)), t.get(2).and_then(|i| v(*i))) {
                            tris.push([a, b, c]);
                        }
                    }
                }
                self.ed.model_edges.insert(m.name.clone(), std::sync::Arc::new(Editor::feature_edges(&tris)));
                self.ed.model_tris.insert(m.name.clone(), std::sync::Arc::new(tris));
            }
            ctx.request_repaint();
        }
        // Modelle für alle Props anfordern (für 2D-Boxen) – nur wenn sich die Map geändert hat
        if !self.models.list.is_empty() && self.model_req_rev != (self.ed.rev, self.models.list.len()) {
            self.model_req_rev = (self.ed.rev, self.models.list.len());
            let paths: Vec<String> = self.ed.map.entities.iter().filter_map(|e| self.ed.model_path(e)).collect();
            for p in paths {
                self.models.get(&p);
            }
        }
        // 2D-Linienpuffer (Grafikkarte) bei Änderungen neu bauen; während des Ziehens gedrosselt
        if self.view3d.renderer.is_some() {
            let key = (self.ed.rev, crate::theme::t().dark, self.ed.vis_colors, self.models.generation);
            if key != self.lines_key {
                let dragging = ctx.input(|i| i.pointer.any_down());
                let content_only = key.0 == self.lines_key.0 && key.1 == self.lines_key.1 && key.2 == self.lines_key.2;
                let wait = if content_only { 0.5 } else if dragging { 0.25 } else { 0.0 };
                if self.lines_built.elapsed().as_secs_f32() >= wait {
                    let data = crate::view2d::build_lines(&self.ed);
                    if let Some(r) = &self.view3d.renderer {
                        r.lock().unwrap().set_lines2d(data);
                    }
                    self.lines_key = key;
                    self.lines_built = Instant::now();
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f32(wait));
                }
            }
        }
        self.ed.game_dir = self.settings.game_dir.clone();
        self.ed.load_instances();
    }
}

/// Ergebnis einer Stufe beim Einlesen der Spielinhalte
pub(crate) struct ContentMsg {
    final_stage: bool,
    found: Option<PathBuf>,
    fs: GameFs,
    materials: Vec<String>,
    sounds: Vec<String>,
    models: Vec<String>,
}

pub(crate) fn load_fgds(files: &[String]) -> (Fgd, Vec<String>) {
    let mut fgd = Fgd::builtin();
    let mut msgs = Vec::new();
    for f in files {
        if let Err(e) = fgd.load_file(std::path::Path::new(f)) {
            msgs.push(format!("FGD konnte nicht geladen werden: {e}"));
        }
    }
    (fgd, msgs)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Zeitmessung: AMBOSS_PROFILE=1 gibt die Rechenzeit pro Bild aus (gesamt / nur update)
        let profile = std::env::var_os("AMBOSS_PROFILE").is_some();
        let t_update = Instant::now();
        if profile {
            if let Some(cpu) = _frame.info().cpu_usage {
                eprintln!("frame_cpu_ms {:.1}", cpu * 1000.0);
            }
        }
        self.ed.map_limit = self.settings.limit();
        self.update_inner(ctx);
        // Sprach-Prüfung: AMBOSS_I18N_AUDIT=<datei> schreibt jeden gezeichneten Text (einmalig) in die Datei
        if let Some(out) = std::env::var_os("AMBOSS_I18N_AUDIT") {
            i18n_audit(ctx, std::path::Path::new(&out));
        }
        if profile {
            eprintln!("update_ms {:.1}", t_update.elapsed().as_secs_f64() * 1000.0);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.upd.relaunch && self.upd.installed {
            crate::update::relaunch();
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.store_profile();
        self.store_layout();
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}

impl App {
    /// Gespeicherte Aufteilung übernehmen (beim Start)
    pub(crate) fn apply_layout(&mut self) {
        let l = self.settings.layout.clone();
        self.maximized = l.maximized.filter(|m| *m < 4);
        for (v, a) in self.views2d.iter_mut().zip(l.axes) {
            v.axis = a;
        }
        self.view3d.mode = RenderMode::ALL.get(l.render_mode).copied().unwrap_or(RenderMode::Textured);
        self.view3d.textured = l.textured;
        self.view3d.models = l.models;
        self.view3d.collision = l.collision;
        self.view3d.shadows = l.shadows;
        self.view3d.gi_on = l.gi;
        self.view3d.particles.on = l.particles;
        self.view3d.fog_preview = l.fog;
        self.ed.dim_levels = l.dim_levels;
        self.view3d.sky_preview = l.sky_preview;
        self.view3d.helpers = l.helpers;
        self.panel_tab = l.panel_tab.min(2);
    }

    /// Aktuelle Aufteilung in die Einstellungen schreiben
    pub(crate) fn store_layout(&mut self) {
        let l = &mut self.settings.layout;
        l.maximized = self.maximized;
        for (a, v) in l.axes.iter_mut().zip(&self.views2d) {
            *a = v.axis;
        }
        l.render_mode = RenderMode::ALL.iter().position(|m| *m == self.view3d.mode).unwrap_or(0);
        l.textured = self.view3d.textured;
        l.models = self.view3d.models;
        l.collision = self.view3d.collision;
        l.shadows = self.view3d.shadows;
        l.gi = self.view3d.gi_on;
        l.particles = self.view3d.particles.on;
        l.fog = self.view3d.fog_preview;
        l.dim_levels = self.ed.dim_levels;
        l.sky_preview = self.view3d.sky_preview;
        l.helpers = self.view3d.helpers;
        l.panel_tab = self.panel_tab;
    }

    fn update_inner(&mut self, ctx: &egui::Context) {
        self.physics_tick(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && (self.ed.dirty || self.tabs.iter().flatten().any(|t| t.ed.dirty)) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.iter().find(|p| crate::model_import::is_model_file(p)) {
            self.open_model_import(p.clone());
        } else if let Some(p) = dropped.into_iter().find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf") || e.eq_ignore_ascii_case("bsp"))) {
            self.request(ctx, Pending::Open(Some(p)));
        }

        self.background(ctx);
        self.poll_mcp();
        self.poll_collab(ctx);
        self.update_notice(ctx);
        if let Some(z) = self.dlg.pending_zoom.take() {
            ctx.set_zoom_factor(z.clamp(0.75, 2.5));
        }
        // Strg+Plus/Minus/0 (egui) ändert die Größe – merken
        else if (ctx.zoom_factor() - self.settings.ui_scale).abs() > 0.001 {
            self.settings.ui_scale = ctx.zoom_factor();
        }
        self.shortcuts(ctx);
        if self.win.start {
            self.start_page(ctx);
        } else {
            self.top_bar(ctx);
            self.tab_strip(ctx);
            self.status_line(ctx);
            self.tool_rail(ctx);
            self.properties_panel(ctx);
            self.context_bar(ctx);
            self.viewports(ctx);
            self.sky_frame(ctx);
        }
        self.process_prefab_drop();
        self.windows(ctx);
        self.poll_job();
        self.live_tick(ctx);
        self.autosave();
        self.ed.finish_label();
        if self.ed.material != self.last_material {
            self.last_material = self.ed.material.clone();
            let m = self.ed.material.clone();
            self.settings.recent_materials.retain(|x| !x.eq_ignore_ascii_case(&m));
            self.settings.recent_materials.insert(0, m);
            self.settings.recent_materials.truncate(24);
        }

        // Kamera: Änderungen aus dem Kamera-Werkzeug übernehmen, sonst 3D-Kamera speichern
        if self.ed.camera_changed {
            if let Some(c) = self.ed.map.cameras.get(self.ed.map.active_camera).cloned() {
                self.view3d.set_from_camera(&c);
            }
            self.ed.camera_changed = false;
        } else if !self.ed.map.cameras.is_empty() {
            let i = self.ed.map.active_camera.min(self.ed.map.cameras.len() - 1);
            self.ed.map.cameras[i] = self.view3d.camera_def();
        }

        self.settings.tex_lock = self.ed.tex_lock;
        self.settings.grid = self.ed.grid;
        let title = format!("Amboss {} – {}{}", env!("CARGO_PKG_VERSION"), self.ed.display_name(), if self.ed.dirty { " *" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        if self.mats.is_loading() || self.models.is_loading() || self.job.as_ref().is_some_and(|j| j.state() == JobState::Running) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.settings.autosave_minutes > 0 {
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
        }
    }

}

#[allow(dead_code)]
fn _unused(_: DVec3) {}

/// Ecken eines Rechtecks optisch abrunden (Viertelkreise in Hintergrundfarbe darüber zeichnen)
fn round_corners(p: &egui::Painter, r: egui::Rect, rad: f32, bg: Color32) {
    let corners = [(r.left_top(), egui::vec2(1.0, 1.0)), (r.right_top(), egui::vec2(-1.0, 1.0)), (r.right_bottom(), egui::vec2(-1.0, -1.0)), (r.left_bottom(), egui::vec2(1.0, -1.0))];
    for (c, d) in corners {
        let center = c + d * rad;
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(c, bg);
        let n = 8;
        for k in 0..=n {
            let a = std::f32::consts::FRAC_PI_2 * k as f32 / n as f32;
            // Bogen von der Kante zur Kante, der Ecke zugewandt
            let v = egui::vec2(-d.x * a.cos(), -d.y * a.sin());
            mesh.colored_vertex(center + v * rad, bg);
        }
        for k in 1..=n as u32 {
            mesh.add_triangle(0, k, k + 1);
        }
        p.add(egui::Shape::mesh(mesh));
    }
}

/// Alle in diesem Bild gezeichneten Texte sammeln und neue an `out` anhängen (Werkzeug für die Sprachprüfung)
fn i18n_audit(ctx: &egui::Context, out: &std::path::Path) {
    use std::io::Write;
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::OnceLock::new();
    fn walk(shape: &egui::Shape, acc: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => acc.push(t.galley.job.text.clone()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            _ => {}
        }
    }
    let layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let mut texts = Vec::new();
    ctx.graphics(|g| {
        for l in layers.iter().copied().chain([egui::LayerId::background()]) {
            if let Some(list) = g.get(l) {
                for cs in list.all_entries() {
                    walk(&cs.shape, &mut texts);
                }
            }
        }
    });
    let seen = SEEN.get_or_init(Default::default);
    let mut seen = seen.lock().unwrap();
    let mut new = String::new();
    for t in texts {
        let t = t.trim().to_string();
        if !t.is_empty() && seen.insert(t.clone()) {
            new.push_str(&t.replace('\n', "\\n"));
            new.push('\n');
        }
    }
    if !new.is_empty() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(out) {
            let _ = f.write_all(new.as_bytes());
        }
    }
}

/// Ordner im Dateimanager des Systems öffnen
pub(crate) fn open_folder(p: &std::path::Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(p).spawn();
}
