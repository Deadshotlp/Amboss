//! Fenster „Live-Abgleich mit dem Spiel“, „Im Spiel testen ab 3D-Kamera“ und der Abgleich in
//! jedem Bild.

use crate::live::{self, Live};
use eframe::egui::{self, RichText};
use glam::DVec3;

impl crate::app::App {
    /// In jedem Bild: Antwort des Spiels lesen, Änderungen übertragen, Kamera folgen lassen
    pub(crate) fn live_tick(&mut self, ctx: &egui::Context) {
        let gd = self.settings.game_dir.trim().to_string();
        if gd.is_empty() {
            return;
        }
        self.live.poll(&gd);
        let connected = self.live.connected();
        if connected {
            let mats = &self.mats;
            self.live.sync(&self.ed, &gd, &|m| mats.info(m).map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0)), false);
        }
        let player = self.live.status.as_ref().filter(|_| connected).map(|s| (s.pos, s.ang));
        // nur die Anzeige ändert sich (wird in jedem Bild neu gezeichnet), kein Rückgängig-Schritt
        self.ed.live_player = player;
        if let (true, Some((pos, ang))) = (self.live.follow, player) {
            let c = &mut self.view3d.cam;
            c.pos = (pos + DVec3::Z * 64.0).as_vec3();
            c.yaw = ang.y as f32;
            c.pitch = -ang.x as f32;
        }
        if connected || self.win.live {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    /// Begehen-Modus an/aus (Start an der 3D-Kamera)
    pub(crate) fn toggle_walk(&mut self) {
        if self.view3d.walker.on {
            self.view3d.walker.on = false;
        } else {
            let eye = self.view3d.cam.pos.as_dvec3();
            self.view3d.walker.start(&self.ed, eye);
            self.ed.status = crate::i18n::tr("Begehen: WASD laufen, Leertaste springen, Strg ducken, Esc beendet").into();
        }
    }

    /// Kamera als Spielerposition (Füße 64 Einheiten unter dem Auge) und Source-Winkel
    fn cam_as_player(&self) -> (DVec3, DVec3) {
        let c = &self.view3d.cam;
        (c.pos.as_dvec3() - DVec3::Z * 64.0, DVec3::new(-c.pitch as f64, c.yaw as f64, 0.0))
    }

    /// Kompilieren und das Spiel so starten, dass der Spieler an der 3D-Kamera steht
    pub(crate) fn test_from_camera(&mut self) {
        let gd = self.settings.game_dir.trim().to_string();
        if gd.is_empty() {
            self.ed.status = crate::i18n::tr("Kein Spielverzeichnis eingestellt (Einstellungen → Spiel).").into();
            return;
        }
        if live::installed(&gd).is_none_or(|v| v < live::LUA_VERSION) {
            match live::install(&gd) {
                Ok(_) => self.live.log.push(crate::i18n::tr("Live-Brücke installiert – wirkt nach dem nächsten Start von Garry's Mod").into()),
                Err(e) => {
                    self.ed.status = e;
                    return;
                }
            }
        }
        let (pos, ang) = self.cam_as_player();
        let name = self.ed.map_name();
        if let Err(e) = Live::write_start(&gd, &name, pos, ang) {
            self.ed.status = e.to_string();
            return;
        }
        // läuft das Spiel schon, wird die Map dort neu geladen (siehe start_compile)
        self.start_compile();
    }

    pub(crate) fn live_window(&mut self, ctx: &egui::Context) {
        use crate::i18n::{tr, tr_dyn};
        use crate::theme;
        if !self.win.live {
            return;
        }
        let th = theme::t();
        let gd = self.settings.game_dir.trim().to_string();
        let inst = if gd.is_empty() { None } else { live::installed(&gd) };
        let connected = self.live.connected();
        let status = self.live.status.clone();
        let sum = self.live.summary.clone();
        let log: Vec<String> = self.live.log.iter().rev().take(4).rev().cloned().collect();
        let has_baseline = self.live.has_baseline();
        let mut open = true;
        #[derive(PartialEq)]
        enum Act {
            Install,
            Test,
            PlayerToCam,
            CamToPlayer,
            Reload,
            Baseline,
            SyncNow,
            Fire,
            Noclip,
        }
        let mut acts: Vec<Act> = Vec::new();
        let d = theme::Dlg::new(tr("Live-Abgleich mit dem Spiel"), 500.0).footer();
        let live = &mut self.live;
        let fire = &mut self.live_fire;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.label(RichText::new(tr("Änderungen erscheinen sofort im laufenden Garry's Mod, ohne neu zu kompilieren: Entities verschieben, Werte ändern, neue Props und Entities, Löschen. Neue oder geänderte Brushes erscheinen als begehbare Vorschau.")).color(th.text2));
                ui.add_space(6.0);
                // Status
                ui.horizontal(|ui| {
                    let (c, txt) = match (&inst, connected, &status) {
                        (_, true, Some(s)) => (th.ok, tr_dyn(&format!("Verbunden: {} · Spieler bei {:.0} {:.0} {:.0}", s.map, s.pos.x, s.pos.y, s.pos.z))),
                        (None, _, _) => (th.warn, tr("Brücke nicht installiert").to_string()),
                        (Some(v), _, _) if *v < live::LUA_VERSION => (th.warn, tr("Brücke veraltet – neu installieren").to_string()),
                        _ => (th.text3, tr("Kein Spiel verbunden – Map in Garry's Mod starten (Einzelspieler)").to_string()),
                    };
                    theme::dot(ui, c);
                    ui.label(RichText::new(txt).strong());
                });
                // Diagnose: beobachteter Ordner und wann das Spiel zuletzt geschrieben hat
                {
                    let age = |n: &str| match live::file_age(&gd, n) {
                        Some(a) if a < 3.0 => tr("gerade eben").to_string(),
                        Some(a) => tr_dyn(&format!("vor {} s", a.round() as i64)),
                        None => tr("fehlt").to_string(),
                    };
                    let dir = live::data_dir(&gd);
                    ui.label(RichText::new(tr_dyn(&format!("Austausch: {} · out.txt (Server): {} · alive.txt (Client): {}", dir.display(), age("out.txt"), age("alive.txt")))).size(11.0).color(th.text3))
                        .on_hover_text(tr("Fehlen beide Dateien, obwohl die Map läuft: In der GMod-Konsole „amboss_live_status“ eingeben. Ist dort ein anderer Ordner zu sehen, ist in Amboss ein anderer Spielordner eingetragen als der, aus dem GMod startet."));
                }
                if let Some(s) = status.as_ref().filter(|s| connected && s.version < live::LUA_VERSION) {
                    ui.label(RichText::new(tr_dyn(&format!("Das Spiel nutzt noch Brücke {} – Garry's Mod neu starten, damit die neue geladen wird.", s.version))).size(12.0).color(th.warn));
                }
                if inst.is_none_or(|v| v < live::LUA_VERSION) {
                    if theme::accent_button(ui, tr("Brücke installieren")).clicked() {
                        acts.push(Act::Install);
                    }
                    ui.label(RichText::new(tr("Legt addons/amboss_live an; danach Garry's Mod einmal neu starten.")).size(12.0).color(th.text3));
                }
                theme::section(ui, tr("Testen"));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Im Spiel testen ab 3D-Kamera")).on_hover_text(tr("Kompilieren; der Spieler startet dort, wo die 3D-Kamera steht. Läuft das Spiel schon, wird die Map dort neu geladen.")).clicked() {
                        acts.push(Act::Test);
                    }
                    if ui.add_enabled(connected, egui::Button::new(tr("Spieler zur 3D-Kamera"))).clicked() {
                        acts.push(Act::PlayerToCam);
                    }
                    if ui.add_enabled(connected, egui::Button::new(tr("3D-Kamera zum Spieler"))).clicked() {
                        acts.push(Act::CamToPlayer);
                    }
                    if ui.add_enabled(connected, egui::Button::new(tr("Noclip an/aus"))).clicked() {
                        acts.push(Act::Noclip);
                    }
                });
                theme::checkbox(ui, &mut live.follow, tr("3D-Kamera folgt dem Spieler"));
                theme::section(ui, tr("Abgleich"));
                theme::checkbox(ui, &mut live.auto, tr("Änderungen automatisch übertragen"));
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(connected, egui::Button::new(tr("Jetzt übertragen"))).clicked() {
                        acts.push(Act::SyncNow);
                    }
                    if ui.add_enabled(connected, egui::Button::new(tr("Map neu laden"))).on_hover_text(tr("Lädt die zuletzt kompilierte Map im Spiel neu (live übertragene Änderungen gehen verloren)")).clicked() {
                        acts.push(Act::Reload);
                    }
                    if ui.button(tr("Spielstand = jetzige Map")).on_hover_text(tr("Wenn die Map im Spiel genau dem jetzigen Stand entspricht (z. B. außerhalb von Amboss kompiliert)")).clicked() {
                        acts.push(Act::Baseline);
                    }
                });
                if !has_baseline {
                    ui.label(RichText::new(tr("Noch kein Bezug: beim Verbinden gilt der jetzige Stand als das, was im Spiel geladen ist.")).size(12.0).color(th.text3));
                }
                if connected {
                    ui.label(tr_dyn(&format!("Übertragen: {} geändert · {} neu · {} entfernt · {} Brush-Vorschau", sum.updated, sum.created, sum.removed, sum.ghost_brushes)));
                    if sum.removed_brushes > 0 || !sum.needs_compile.is_empty() {
                        let mut parts = Vec::new();
                        if sum.removed_brushes > 0 {
                            parts.push(tr_dyn(&format!("{} gelöschte Brushes", sum.removed_brushes)));
                        }
                        if !sum.needs_compile.is_empty() {
                            parts.push(sum.needs_compile.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
                        }
                        ui.label(RichText::new(tr_dyn(&format!("Braucht Kompilieren: {}", parts.join(" · ")))).size(12.0).color(th.warn));
                    }
                }
                theme::section(ui, tr("Input auslösen"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut fire.0).hint_text(theme::hint(tr("Ziel (targetname)"))).desired_width(150.0));
                    ui.add(egui::TextEdit::singleline(&mut fire.1).hint_text(theme::hint(tr("Input"))).desired_width(100.0));
                    ui.add(egui::TextEdit::singleline(&mut fire.2).hint_text(theme::hint(tr("Parameter"))).desired_width(80.0));
                    if ui.add_enabled(connected && !fire.0.trim().is_empty(), egui::Button::new(tr("Auslösen"))).clicked() {
                        acts.push(Act::Fire);
                    }
                });
                if let Some(s) = status.as_ref().filter(|s| !s.error.is_empty()) {
                    ui.label(RichText::new(tr_dyn(&format!("Letzter Fehler im Spiel: {}", s.error))).size(12.0).color(th.err));
                }
                for l in &log {
                    ui.label(RichText::new(tr_dyn(l)).font(theme::mono(10.5)).color(th.text3));
                }
            }
            theme::Part::Footer => {
                theme::footer_hint(ui, tr("Nur im Einzelspieler bzw. auf dem eigenen Server"));
            }
            _ => {}
        });
        self.win.live = open;
        for a in acts {
            match a {
                Act::Install => match live::install(&gd) {
                    Ok(p) => self.live.log.push(format!("Installiert: {}", p.display())),
                    Err(e) => self.live.log.push(e),
                },
                Act::Test => self.test_from_camera(),
                Act::PlayerToCam => {
                    let (p, a) = self.cam_as_player();
                    self.live.teleport(&gd, p, a);
                }
                Act::CamToPlayer => {
                    if let Some(s) = &self.live.status {
                        let c = &mut self.view3d.cam;
                        c.pos = (s.pos + DVec3::Z * 64.0).as_vec3();
                        c.yaw = s.ang.y as f32;
                        c.pitch = -s.ang.x as f32;
                    }
                }
                Act::Noclip => self.live.send(&gd, serde_json::json!({ "op": "noclip" })),
                Act::Reload => {
                    let m = self.live.status.as_ref().map(|s| s.map.clone()).unwrap_or_else(|| self.ed.map_name());
                    self.live.reload_map(&gd, &m);
                }
                Act::Baseline => {
                    self.live.set_baseline(&self.ed.map);
                    self.live.log.push("Bezug gesetzt: jetziger Stand der Map".into());
                }
                Act::SyncNow => {
                    let mats = &self.mats;
                    self.live.sync(&self.ed, &gd, &|m| mats.info(m).map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0)), true);
                }
                Act::Fire => {
                    let (t, i, p) = self.live_fire.clone();
                    self.live.fire(&gd, t.trim(), i.trim(), p.trim());
                }
            }
        }
    }
}
