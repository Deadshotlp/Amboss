//! Oberfläche für Ansichtsfilter („Einblenden“) und den 3D-Skybox-Modus.

use crate::app::App;
use crate::editor::{AutoVis, Editor};
use crate::i18n::tr;
use crate::skybox::{self, CreateOpts, Level};
use eframe::egui::{self, RichText, Vec2};
use glam::{DVec2, DVec3};

/// Zustand des Skybox-Modus und seiner Fenster
pub(crate) struct SkyUi {
    pub active: bool,
    /// gemerkte Ansichten (werden beim Verlassen wiederhergestellt)
    saved_cam: Option<crate::view3d::Camera>,
    saved_views: Vec<(DVec2, f64)>,
    saved_grid: f64,
    saved_sky_preview: bool,
    /// Tab und Datei beim Betreten (Wechsel → Modus beenden)
    tab: usize,
    path: Option<std::path::PathBuf>,
    pub pip: bool,
    pub create_open: bool,
    pub opts: CreateOpts,
    pub ground: bool,
    pub ground_mat: String,
    pub check_open: bool,
    pub issues: Vec<(Level, String)>,
    /// Auswahl in der Map beim Betreten (zum verkleinerten Hineinkopieren)
    carry: Vec<crate::editor::Obj>,
}

impl Default for SkyUi {
    fn default() -> Self {
        SkyUi {
            active: false,
            saved_cam: None,
            saved_views: Vec::new(),
            saved_grid: 16.0,
            saved_sky_preview: true,
            tab: 0,
            path: None,
            pip: true,
            create_open: false,
            opts: CreateOpts { scale: 16.0, size: DVec3::new(1024.0, 1024.0, 512.0), ground: None, copy_fog: true },
            ground: true,
            ground_mat: "DEV/GRAYGRID".into(),
            check_open: false,
            issues: Vec::new(),
            carry: Vec::new(),
        }
    }
}

/// Einträge der Filterliste: (Schlüssel, Beschriftung, Tooltip)
fn filter_rows() -> [(&'static str, &'static str, &'static str); 15] {
    [
        ("world", "Welt-Brushes", "Alle Brushes, die zu keinem Entity gehören"),
        ("detail", "func_detail", "Detail-Brushes"),
        ("brush_ents", "Brush-Entities", "Türen, func_brush, func_wall …"),
        ("triggers", "Trigger", "trigger_* und Brushes mit tools/toolstrigger"),
        ("nodraw", "nodraw-Flächen", "In 3D jede nodraw-Fläche einzeln, in 2D reine nodraw-Brushes"),
        ("clips", "Clips", "clip, playerclip, npcclip, func_clip_vphysics"),
        ("hints", "Hint/Skip", "hint- und skip-Brushes"),
        ("portals", "Areaportals & Occluder", "func_areaportal(window), func_occluder, func_viscluster"),
        ("skybox", "Skybox-Flächen", "tools/toolsskybox – in 3D jede Fläche einzeln"),
        ("tools", "Sonstige Werkzeug-Brushes", "invisible, blocklight, black, fog …"),
        ("disps", "Displacements", ""),
        ("point_ents", "Punkt-Entities", ""),
        ("lights", "Lichter", ""),
        ("props", "Props", ""),
        ("sky_room", "3D-Skybox-Raum", "Alles im Raum um die sky_camera"),
    ]
}

fn flag<'a>(av: &'a mut AutoVis, key: &str) -> &'a mut bool {
    match key {
        "world" => &mut av.world,
        "detail" => &mut av.detail,
        "brush_ents" => &mut av.brush_ents,
        "triggers" => &mut av.triggers,
        "nodraw" => &mut av.nodraw,
        "clips" => &mut av.clips,
        "hints" => &mut av.hints,
        "portals" => &mut av.portals,
        "skybox" => &mut av.skybox,
        "tools" => &mut av.tools,
        "disps" => &mut av.disps,
        "point_ents" => &mut av.point_ents,
        "lights" => &mut av.lights,
        "props" => &mut av.props,
        _ => &mut av.sky_room,
    }
}

/// Ausgeblendete Arten als lesbare Liste
pub(crate) fn hidden_labels(av: &AutoVis) -> String {
    let mut av = *av;
    filter_rows().iter().filter(|(k, _, _)| !*flag(&mut av, k)).map(|(_, l, _)| tr(l)).collect::<Vec<_>>().join(", ")
}

/// Liste der Ansichtsfilter (Häkchen = sichtbar). `cols` = Spalten.
pub(crate) fn filter_ui(ed: &mut Editor, ui: &mut egui::Ui, cols: usize) {
    let before = ed.auto_vis;
    let rows = filter_rows();
    egui::Grid::new(ui.id().with("viewfilter")).num_columns(cols).spacing([14.0, 4.0]).show(ui, |ui| {
        for (i, (key, label, tip)) in rows.iter().enumerate() {
            let r = crate::theme::checkbox(ui, flag(&mut ed.auto_vis, key), tr(label));
            if !tip.is_empty() {
                r.on_hover_text(tr(tip));
            }
            if (i + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!ed.auto_vis.all_visible(), egui::Button::new(tr("Alles einblenden"))).clicked() {
            ed.auto_vis = AutoVis::default();
        }
        if ui.button(tr("Werkzeug-Brushes ausblenden")).on_hover_text(tr("nodraw, Trigger, Clips, Hint/Skip, Areaportals, Skybox und sonstige")).clicked() {
            let av = &mut ed.auto_vis;
            av.nodraw = false;
            av.triggers = false;
            av.clips = false;
            av.hints = false;
            av.portals = false;
            av.skybox = false;
            av.tools = false;
        }
    });
    if ed.auto_vis != before {
        // ausgeblendete Objekte nicht mehr ausgewählt lassen
        let sel: Vec<_> = ed.sel.iter().copied().filter(|o| ed.is_visible(*o)).collect();
        ed.sel = sel.into_iter().collect();
        ed.touch();
    }
}

impl App {
    /// Ansicht → 3D-Skybox bearbeiten
    pub(crate) fn toggle_sky_mode(&mut self) {
        if self.sky.active {
            self.exit_sky_mode();
            return;
        }
        let Some(room) = skybox::sky_room(&self.ed.map) else {
            if skybox::sky_cameras(&self.ed.map).is_empty() {
                // noch keine Skybox: Assistent
                self.sky.opts.size = skybox::suggest_size(&self.ed.map, self.sky.opts.scale);
                self.sky.ground_mat = if self.ed.material.is_empty() { "DEV/GRAYGRID".into() } else { self.ed.material.clone() };
                self.sky.create_open = true;
            } else {
                self.ed.status = tr("Der Raum um die sky_camera ist nicht geschlossen – siehe Skybox prüfen").into();
                self.sky.issues = skybox::check(&self.ed.map, self.ed.map_limit);
                self.sky.check_open = true;
            }
            return;
        };
        self.sky.active = true;
        self.sky.carry = self.ed.sel.iter().copied().filter(|o| !room.bounds.contains_point(self.ed.obj_bounds(*o).center())).collect();
        self.sky.tab = self.active_tab;
        self.sky.path = self.ed.path.clone();
        self.sky.saved_cam = Some(self.view3d.cam.clone());
        self.sky.saved_views = self.views2d.iter().map(|v| (v.center, v.zoom)).collect();
        self.sky.saved_grid = self.ed.grid;
        self.sky.saved_sky_preview = self.view3d.sky_preview;
        self.view3d.sky_preview = true;
        // Raster passend zum Maßstab (16er-Raster der Welt → 1 im Raum), mindestens 1
        let g = (self.ed.grid / room.scale).max(1.0);
        self.ed.grid = 2f64.powf(g.log2().round()).clamp(1.0, 64.0);
        // Ansichten auf den Raum
        let b = room.bounds;
        for v in &mut self.views2d {
            let (h, vv, _) = v.axis.axes();
            v.center = DVec2::new(b.center()[h], b.center()[vv]);
            let ext = b.size()[h].max(b.size()[vv]).max(64.0);
            v.zoom = (400.0 / ext).clamp(0.02, 8.0);
        }
        // 3D-Kamera in den Raum, Blick auf die verkleinerte Map
        let target = room.to_sky(skybox::world_bounds(&self.ed.map, Some(&b)).center());
        let target = if b.contains_point(target) { target } else { b.center() };
        let from = DVec3::new(b.min.x + b.size().x * 0.15, b.min.y + b.size().y * 0.15, target.z + b.size().z * 0.25);
        let d = (target - from).normalize_or(DVec3::X);
        self.view3d.cam.pos = from.as_vec3();
        self.view3d.cam.yaw = d.y.atan2(d.x).to_degrees() as f32;
        self.view3d.cam.pitch = (d.z.asin().to_degrees() as f32).clamp(-89.0, 89.0);
        self.ed.status = tr_scale("3D-Skybox-Modus – Maßstab 1:{}", room.scale);
        self.sky_sync();
    }

    pub(crate) fn exit_sky_mode(&mut self) {
        if !self.sky.active {
            return;
        }
        self.sky.active = false;
        self.sky.carry.clear();
        self.ed.sky_edit = None;
        self.view3d.clip_box = None;
        self.view3d.pip = None;
        if let Some(c) = self.sky.saved_cam.take() {
            self.view3d.cam = c;
        }
        for (v, (c, z)) in self.views2d.iter_mut().zip(self.sky.saved_views.drain(..)) {
            v.center = c;
            v.zoom = z;
        }
        self.ed.grid = self.sky.saved_grid;
        self.view3d.sky_preview = self.sky.saved_sky_preview;
        self.ed.touch();
    }

    /// Jedes Bild: Raum neu bestimmen (Wände können sich ändern), Eingrenzung setzen
    fn sky_sync(&mut self) {
        if !self.sky.active {
            return;
        }
        if self.active_tab != self.sky.tab || self.ed.path != self.sky.path {
            self.exit_sky_mode();
            return;
        }
        let Some(room) = self.ed.sky_room_bounds() else {
            self.exit_sky_mode();
            self.ed.status = tr("3D-Skybox-Modus beendet: kein geschlossener Raum um die sky_camera").into();
            return;
        };
        let iso = crate::geometry::Aabb::new(room.min - DVec3::splat(32.0), room.max + DVec3::splat(32.0));
        if self.ed.sky_edit != Some(iso) {
            self.ed.sky_edit = Some(iso);
            // ausgegrenzte Objekte abwählen
            let sel: Vec<_> = self.ed.sel.iter().copied().filter(|o| self.ed.is_visible(*o)).collect();
            self.ed.sel = sel.into_iter().collect();
            self.ed.touch();
        }
        self.view3d.clip_box = Some(iso);
        self.view3d.sky_preview = true;
        match (self.sky.pip, self.view3d.pip.is_some()) {
            (true, false) => self.view3d.pip = self.sky.saved_cam.clone(),
            (false, true) => {
                // Blickrichtung der Spielersicht merken
                self.sky.saved_cam = self.view3d.pip.take();
            }
            _ => {}
        }
        if let Some(p) = &self.view3d.pip {
            self.sky.saved_cam = Some(p.clone());
        }
    }

    /// Leiste oben über den Ansichten, Dialoge
    pub(crate) fn sky_frame(&mut self, ctx: &egui::Context) {
        self.sky_sync();
        if self.sky.active {
            self.sky_bar(ctx);
        }
        self.sky_create_window(ctx);
        self.sky_check_window(ctx);
    }

    fn sky_bar(&mut self, ctx: &egui::Context) {
        let th = crate::theme::t();
        let Some(rect) = self.ui_rects.get("viewports").copied() else {
            return;
        };
        let scale = self.ed.sky_room_bounds().and(skybox::sky_room(&self.ed.map)).map(|r| r.scale).unwrap_or(16.0);
        let mut act = 0u8;
        egui::Area::new(egui::Id::new("sky_bar")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(rect.center().x - ctx.screen_rect().center().x, rect.bottom() - ctx.screen_rect().bottom() - 12.0)).show(ctx, |ui| {
            egui::Frame::new()
                .fill(th.panel)
                .stroke(egui::Stroke::new(1.0f32, th.accent))
                .corner_radius(9)
                .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: th.shadow })
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        crate::icons::draw(ui.painter(), r, "sky", th.accent, 1.7);
                        ui.label(RichText::new(tr_scale("3D-Skybox · 1:{}", scale)).strong().color(th.text));
                        ui.separator();
                        crate::theme::checkbox(ui, &mut self.sky.pip, tr("Spielersicht"));
                        let n = self.sky.carry.len();
                        let label = if n > 0 { crate::i18n::tr_dyn(&format!("Map-Auswahl verkleinert hineinkopieren ({n})")) } else { tr("Map-Auswahl verkleinert hineinkopieren").to_string() };
                        if ui
                            .add_enabled(n > 0, egui::Button::new(label))
                            .on_hover_text(tr("Kopiert die Brushes, die vor dem Skybox-Modus in der Map ausgewählt waren, im Maßstab der sky_camera an die passende Stelle"))
                            .on_disabled_hover_text(tr("Vor dem Skybox-Modus Brushes in der Map auswählen (z. B. Berge, Gebäude), dann hier verkleinert hineinkopieren"))
                            .clicked()
                        {
                            act = 1;
                        }
                        if ui.button(tr("Prüfen")).clicked() {
                            act = 2;
                        }
                        if crate::theme::primary_button(ui, tr("Fertig")).clicked() {
                            act = 3;
                        }
                    });
                });
        });
        match act {
            1 => {
                self.ed.sel = std::mem::take(&mut self.sky.carry).into_iter().collect();
                self.sky_copy_selection();
            }
            2 => {
                self.sky.issues = skybox::check(&self.ed.map, self.ed.map_limit);
                self.sky.check_open = true;
            }
            3 => self.exit_sky_mode(),
            _ => {}
        }
    }

    /// Auswahl in der Map verkleinert in die Skybox kopieren (Menübefehl, auch außerhalb des Modus)
    pub(crate) fn sky_copy_selection(&mut self) {
        // im Modus ist die Map ausgegrenzt: die Auswahl stammt dann von vorher
        let iso = self.ed.sky_edit.take();
        let r = skybox::copy_selection_to_sky(&mut self.ed);
        self.ed.sky_edit = iso;
        match r {
            Ok((n, 0)) => self.ed.status = crate::i18n::tr_dyn(&format!("{n} Objekte verkleinert in die Skybox kopiert")),
            Ok((n, skipped)) => self.ed.status = crate::i18n::tr_dyn(&format!("{n} Objekte verkleinert in die Skybox kopiert, {skipped} übersprungen (Punkt-Entities oder schon in der Skybox)")),
            Err(e) => {
                self.ed.sel.clear();
                self.ed.status = crate::i18n::tr_dyn(&e)
            }
        }
    }

    fn sky_create_window(&mut self, ctx: &egui::Context) {
        if !self.sky.create_open {
            return;
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut create = false;
        let suggested = skybox::suggest_size(&self.ed.map, self.sky.opts.scale);
        let d = crate::theme::Dlg::new(tr("3D-Skybox anlegen"), 520.0).subtitle(tr("Raum mit tools/toolsskybox und sky_camera")).footer();
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Body => {
                ui.label(RichText::new(tr("Was im Skybox-Raum steht, zeigt das Spiel vergrößert hinter der Map – Berge, Städte, ferne Gebäude. Der Raum wird automatisch über, unter oder neben die Map gelegt, wo Platz ist.")).color(th.text2));
                ui.add_space(6.0);
                egui::Grid::new("sky_create").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label(tr("Maßstab 1:"));
                    let mut sc = self.sky.opts.scale;
                    if ui.add(crate::theme::dv(egui::DragValue::new(&mut sc).range(2.0..=64.0).speed(1.0))).on_hover_text(tr("Üblich ist 16: 1 Einheit in der Skybox = 16 Einheiten im Spiel")).changed() {
                        self.sky.opts.scale = sc.round();
                    }
                    ui.end_row();
                    ui.label(tr("Innenmaß (B × T × H)"));
                    ui.horizontal(|ui| {
                        for k in 0..3 {
                            let mut v = self.sky.opts.size[k];
                            if ui.add(crate::theme::dv(egui::DragValue::new(&mut v).range(256.0..=8192.0).speed(16.0))).changed() {
                                self.sky.opts.size[k] = (v / 16.0).round() * 16.0;
                            }
                        }
                        if ui.small_button(tr("Vorschlag")).on_hover_text(tr("Dreifache Breite der verkleinerten Map")).clicked() {
                            self.sky.opts.size = suggested;
                        }
                    });
                    ui.end_row();
                    ui.label(tr("Boden"));
                    ui.horizontal(|ui| {
                        crate::theme::checkbox(ui, &mut self.sky.ground, tr("Bodenplatte in Höhe der Map"));
                        ui.add_enabled(self.sky.ground, egui::TextEdit::singleline(&mut self.sky.ground_mat).desired_width(170.0).font(crate::theme::mono(12.0)));
                    });
                    ui.end_row();
                    ui.label(tr("Nebel"));
                    crate::theme::checkbox(ui, &mut self.sky.opts.copy_fog, tr("Nebel der Map übernehmen (env_fog_controller)"));
                    ui.end_row();
                });
                let world = skybox::world_bounds(&self.ed.map, None);
                if !world.is_empty() {
                    let s = world.size() / self.sky.opts.scale;
                    ui.add_space(4.0);
                    ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Die Map ist in der Skybox {} × {} × {} Einheiten groß.", crate::i18n::dec(crate::map::fmt_num(s.x.round())), crate::i18n::dec(crate::map::fmt_num(s.y.round())), crate::i18n::dec(crate::map::fmt_num(s.z.round()))))).size(12.0).color(th.text3));
                }
                let opts = self.current_create_opts();
                if let Err(e) = skybox::placement(&self.ed.map, &opts, self.ed.map_limit) {
                    ui.label(RichText::new(crate::i18n::tr_dyn(&e)).color(th.warn));
                }
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, tr("Anlegen und bearbeiten")).clicked() {
                    create = true;
                }
                if crate::theme::secondary_button(ui, tr("Abbrechen")).clicked() {
                    self.sky.create_open = false;
                }
            }
            _ => {}
        });
        if !open {
            self.sky.create_open = false;
        }
        if create {
            let opts = self.current_create_opts();
            match skybox::create(&mut self.ed, &opts) {
                Ok(_) => {
                    self.sky.create_open = false;
                    self.toggle_sky_mode();
                }
                Err(e) => self.ed.status = crate::i18n::tr_dyn(&e),
            }
        }
    }

    fn current_create_opts(&self) -> CreateOpts {
        let mut o = self.sky.opts.clone();
        o.ground = (self.sky.ground && !self.sky.ground_mat.trim().is_empty()).then(|| self.sky.ground_mat.trim().to_string());
        o
    }

    fn sky_check_window(&mut self, ctx: &egui::Context) {
        if !self.sky.check_open {
            return;
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut again = false;
        let d = crate::theme::Dlg::new(tr("3D-Skybox prüfen"), 520.0).footer();
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Body => {
                if self.sky.issues.is_empty() {
                    ui.label(RichText::new(tr("Keine Probleme gefunden.")).color(th.ok));
                }
                for (lvl, msg) in &self.sky.issues {
                    let (c, t) = match lvl {
                        Level::Error => (th.err, tr("Fehler")),
                        Level::Warn => (th.warn, tr("Warnung")),
                        Level::Info => (th.text3, tr("Hinweis")),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(t).strong().color(c));
                        ui.label(crate::i18n::tr_dyn(msg));
                    });
                }
            }
            crate::theme::Part::Footer => {
                if crate::theme::primary_button(ui, tr("Schließen")).clicked() {
                    self.sky.check_open = false;
                }
                if crate::theme::secondary_button(ui, tr("Erneut prüfen")).clicked() {
                    again = true;
                }
            }
            _ => {}
        });
        if !open {
            self.sky.check_open = false;
        }
        if again {
            self.sky.issues = skybox::check(&self.ed.map, self.ed.map_limit);
        }
    }

    /// Statuszeile im Skybox-Modus: Cursor als Weltkoordinate
    pub(crate) fn sky_cursor_hint(&self, axis: crate::geometry::Axis2D, p: DVec2) -> Option<String> {
        if !self.sky.active {
            return None;
        }
        let room = skybox::sky_room(&self.ed.map)?;
        let (h, v, _) = axis.axes();
        let mut q = room.origin;
        q[h] = p.x;
        q[v] = p.y;
        let w = room.to_world(q);
        let n = ["X", "Y", "Z"];
        Some(crate::i18n::tr_dyn(&format!("≙ Welt {} {} {} {}", n[h], crate::map::fmt_num(w[h].round()), n[v], crate::map::fmt_num(w[v].round()))))
    }
}

fn tr_scale(s: &str, scale: f64) -> String {
    crate::i18n::tr_dyn(&s.replace("{}", &crate::map::fmt_num(scale)))
}

