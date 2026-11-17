//! Fenster „Automatische Optimierungen“ und Einbindung in den Compile-Ablauf.

use crate::app::App;
use crate::optimize::{self, OptSettings, Report};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

impl App {
    /// Optimierer auf einer Kopie laufen lassen; `apply` übernimmt das Ergebnis (ein Undo-Schritt)
    pub(crate) fn run_optimizer(&mut self, apply: bool) -> Report {
        let fs = self.mats.fs();
        let mut copy = optimize::build_copy(&self.ed, self.ed.map.clone());
        let r = optimize::run(&mut copy, &self.settings.opt, fs.as_deref(), None);
        if apply && r.changes > 0 {
            self.ed.checkpoint();
            self.ed.map = copy.map;
            self.ed.sel.clear();
            self.ed.faces.clear();
            self.ed.vsel.clear();
            self.ed.status = format!("Optimierungen übernommen: {} Änderung(en) – Strg+Z macht sie rückgängig", r.changes);
        } else if apply {
            self.ed.status = crate::i18n::tr("Optimierung: nichts zu ändern").into();
        } else {
            self.ed.status = format!("Prüfung: {} mögliche Änderung(en), Map unverändert", r.changes);
        }
        self.show_opt_report(&r);
        r
    }

    /// Netz, Markierungen und Leck-Pfad in den Ansichten anzeigen
    pub(crate) fn show_opt_report(&mut self, r: &Report) {
        self.ed.opt_links = r.links.clone();
        self.ed.opt_markers = r.markers.clone();
        if let Some(l) = &r.leak {
            self.ed.pointfile = l.clone();
        }
        self.ed.touch();
        self.opt_report = Some(r.clone());
    }

    /// Vor dem Kompilieren: optimierte Kopie schreiben. Ok(Pfad, Logzeilen) oder Err(Abbruch)
    pub(crate) fn optimized_build(&mut self, src: &std::path::Path) -> Result<(PathBuf, Vec<String>), ()> {
        let fs = self.mats.fs();
        match optimize::prepare_build(&self.ed, &self.settings.opt, fs.as_deref(), src) {
            Ok((p, lines, r)) => {
                self.show_opt_report(&r);
                Ok((p, lines))
            }
            Err((msg, r)) => {
                self.show_opt_report(&r);
                self.ed.status = format!("{msg} Siehe Kompilieren → Automatische Optimierungen.");
                self.win.optimize = true;
                Err(())
            }
        }
    }

    pub(crate) fn optimize_window(&mut self, ctx: &egui::Context) {
        if !self.win.optimize {
            return;
        }
        let mut open = true;
        let mut action = None;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Automatische Optimierungen"), 560.0), |ui| {
            let o = &mut self.settings.opt;
            crate::theme::checkbox(ui, &mut o.compile_auto, RichText::new(crate::i18n::tr("Beim Kompilieren automatisch anwenden")).strong())
                .on_hover_text(crate::i18n::tr("Läuft auf einer Kopie (Ordner amboss_build neben der VMF) vor VBSP. Deine Map bleibt unverändert."));
            ui.scope(|ui| { crate::theme::box_style(ui); ui.add_enabled(o.compile_auto, egui::Checkbox::new(&mut o.abort_on_leak, crate::i18n::tr("Bei Leck abbrechen (spart den VBSP-Lauf)"))).on_disabled_hover_text(crate::i18n::tr("Nur aktiv, wenn automatisch angewendet wird")); });
            ui.separator();
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                opt_checkboxes(ui, o);
            });
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("Standard")).clicked() {
                    let keep = (o.compile_auto, o.abort_on_leak);
                    *o = OptSettings::default();
                    (o.compile_auto, o.abort_on_leak) = keep;
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("🔍 Nur prüfen")).on_hover_text(crate::i18n::tr("Läuft auf einer Kopie und zeigt Netz, Markierungen und Bericht – die Map bleibt unverändert")).clicked() {
                    action = Some(0);
                }
                if ui.button(crate::i18n::tr("✔ Auf Map anwenden")).on_hover_text(crate::i18n::tr("Ergebnis in die Map übernehmen (ein Rückgängig-Schritt)")).clicked() {
                    action = Some(1);
                }
                if ui.add_enabled(!self.ed.opt_links.is_empty() || !self.ed.opt_markers.is_empty(), egui::Button::new(crate::i18n::tr("Markierungen löschen"))).clicked() {
                    action = Some(2);
                }
            });
            if let Some(r) = &self.opt_report {
                ui.separator();
                ui.label(RichText::new(crate::i18n::tr("Bericht")).strong());
                egui::ScrollArea::vertical().id_salt("optrep").max_height(200.0).show(ui, |ui| {
                    for l in &r.lines {
                        let c = if l.contains("LECK") {
                            egui::Color32::from_rgb(255, 90, 90)
                        } else if l.contains("isolierte") && !l.contains(" 0 isolierte") {
                            egui::Color32::from_rgb(255, 180, 80)
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(RichText::new(crate::i18n::tr_dyn(l)).color(c));
                    }
                });
                ui.label(RichText::new(crate::i18n::tr("Grün = NPC-Verbindungen, Rot = Probleme, Gelb = Hint-Vorschläge, Blau = dunkle Bereiche. Maus über eine Markierung in 2D zeigt den Text.")).small().weak());
            }
        });
        match action {
            Some(0) => {
                self.run_optimizer(false);
            }
            Some(1) => {
                self.run_optimizer(true);
            }
            Some(2) => {
                self.ed.opt_links.clear();
                self.ed.opt_markers.clear();
                self.opt_report = None;
                self.ed.touch();
            }
            _ => {}
        }
        self.win.optimize = open;
    }
}

fn opt_checkboxes(ui: &mut egui::Ui, o: &mut OptSettings) {
    let c = |ui: &mut egui::Ui, v: &mut bool, label: &str, tip: &str| {
        crate::theme::checkbox(ui, v, label).on_hover_text(tip);
    };
    ui.label(RichText::new(crate::i18n::tr("KI (NPCs)")).strong());
    c(ui, &mut o.ai_nodes, crate::i18n::tr("Bodennodes (info_node) automatisch setzen"), crate::i18n::tr("Raster auf begehbaren Flächen, dichter an Türen und Engstellen; vorhandene Nodes bleiben"));
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        ui.label(crate::i18n::tr("Abstand"));
        ui.add(crate::theme::dv(egui::DragValue::new(&mut o.node_spacing)).range(64.0..=512.0).speed(8.0));
    });
    c(ui, &mut o.ai_air_nodes, crate::i18n::tr("Luftnodes (info_node_air) für fliegende NPCs"), crate::i18n::tr("Gitter alle 256 Einheiten in hohen, freien Räumen"));
    c(ui, &mut o.ai_door_links, crate::i18n::tr("Türen: info_node_link_controller"), crate::i18n::tr("NPC-Wege durch Türen nur bei offener Tür (Ausgaben OnOpen/OnClose werden ergänzt)"));
    c(ui, &mut o.ai_hints, crate::i18n::tr("info_hint für Deckung und Engstellen"), crate::i18n::tr("Hinweise an halbhohen Deckungen und Durchgängen"));
    c(ui, &mut o.navmesh, crate::i18n::tr("Navmesh (NextBots) beim Kompilieren erzeugen"), crate::i18n::tr("Eingebauter Generator: begehbare Bereiche ab den Spawnpunkten, gespeichert als maps/<map>.nav und mit „Inhalte packen“ in die BSP"));
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::tr("Sichtbarkeit (VVIS)")).strong());
    c(ui, &mut o.leak_check, crate::i18n::tr("Leckprüfung vor VBSP (mit Pfad)"), crate::i18n::tr("Flutfüllung von allen Punkt-Entities – findet Löcher zur Leere ohne VBSP-Lauf"));
    c(ui, &mut o.nodraw, crate::i18n::tr("Verdeckte Flächen -> nodraw"), crate::i18n::tr("Flächen, die vollständig an anderen Brushes anliegen"));
    c(ui, &mut o.auto_detail, crate::i18n::tr("Kleine/schräge Brushes -> func_detail"), crate::i18n::tr("Brushes ohne Kontakt zur Leere; Trennwände und Türrahmen bleiben Welt"));
    c(ui, &mut o.areaportals, crate::i18n::tr("Areaportals in Tür-Öffnungen"), crate::i18n::tr("func_areaportal an func_door* (nur wenn die Öffnung vollständig von Welt eingefasst ist)"));
    c(ui, &mut o.hint_suggestions, crate::i18n::tr("Hint-Brush-Vorschläge markieren"), crate::i18n::tr("Gelbe Markierungen an Durchgängen – wird nicht automatisch gebaut"));
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::tr("Licht")).strong());
    c(ui, &mut o.lightmaps, crate::i18n::tr("Lightmap-Auflösung automatisch"), crate::i18n::tr("Nahe an Lichtern 8, große/entfernte Flächen 32 (nur Flächen mit Standardwert 16)"));
    c(ui, &mut o.cubemaps, crate::i18n::tr("env_cubemap automatisch verteilen"), crate::i18n::tr("In offenen Bereichen ca. alle 768 Einheiten; danach „Cubemaps bauen“"));
    c(ui, &mut o.light_check, crate::i18n::tr("Lichtprüfung"), crate::i18n::tr("Lichter in Brushes, zu viele schaltbare Lichter, dunkle Bereiche"));
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::tr("Geometrie")).strong());
    c(ui, &mut o.snap_verts, crate::i18n::tr("Eckpunkte auf ganze Einheiten runden"), crate::i18n::tr("Verhindert Mini-Lecks und „Invalid solid“"));
    c(ui, &mut o.merge_brushes, crate::i18n::tr("Gleiche Quader zusammenführen"), crate::i18n::tr("Nebeneinander liegende, gleich texturierte Quader werden ein Brush"));
    c(ui, &mut o.remove_slivers, crate::i18n::tr("Splitter und doppelte Brushes entfernen"), crate::i18n::tr("Ungültige, hauchdünne (< 1 Einheit) und exakt doppelte Brushes"));
    c(ui, &mut o.simplify_disps, crate::i18n::tr("Flache Displacements vereinfachen"), crate::i18n::tr("Displacements ohne Höhen/Alpha werden normale Flächen"));
    ui.add_space(4.0);
    ui.label(RichText::new(crate::i18n::tr("Props und Entities")).strong());
    c(ui, &mut o.prop_fades, crate::i18n::tr("Ausblend-Distanz für Props"), crate::i18n::tr("Nach Modellgröße, nur wenn keine gesetzt ist"));
    c(ui, &mut o.prop_shadows, crate::i18n::tr("Schatten kleiner Props aus"), crate::i18n::tr("disableshadows bei sehr kleinen Modellen"));
    c(ui, &mut o.remove_unused_logic, crate::i18n::tr("Wirkungslose Logik entfernen"), crate::i18n::tr("logic_* ohne Ausgaben, leere Brush-Entities, doppelte Cubemaps"));
    c(ui, &mut o.pack_content, crate::i18n::tr("Eigene Inhalte in die BSP packen"), crate::i18n::tr("Materialien, Modelle, Sounds, Partikel, Soundscapes und Nav-Mesh, die nicht zum Spiel gehören, werden nach VRAD eingepackt (Auswahl: Kompilieren → Inhalte in BSP packen…)"));
}
