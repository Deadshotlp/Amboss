//! Fenster „Prozedurale Generatoren“ und „Tile-Set“.

use eframe::egui::{self, RichText, Vec2};
use glam::DVec3;

impl crate::app::App {
    /// Einfügepunkt: dort, wo die 3D-Kamera auf die Ebene z = 0 blickt (auf 64 gerastert)
    fn gen_origin(&self) -> DVec3 {
        let c = &self.view3d.cam;
        let o = c.pos.as_dvec3();
        let d = c.forward().as_dvec3();
        let p = if d.z < -0.05 { o + d * (-o.z / d.z) } else { o + d * 512.0 };
        DVec3::new((p.x / 64.0).round() * 64.0, (p.y / 64.0).round() * 64.0, 0.0)
    }

    pub(crate) fn procgen_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.win.procgen {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut run = false;
        let has_sel = !self.ed.sel.is_empty();
        let cur_mat = self.ed.material.clone();
        // Vorschaubilder der drei Texturen (vor dem Dialog holen, wegen der Ausleihe)
        let thumbs: Vec<Option<egui::TextureId>> = [self.gen.floor_material.clone(), self.gen.material.clone(), self.gen.ceiling_material.clone()].iter().map(|m| self.mats.thumb(m).map(|t| t.id())).collect();
        let d = theme::Dlg::new(crate::i18n::tr("Prozedurale Generatoren"), 520.0).footer();
        let page = &mut self.gen_page;
        let o = &mut self.gen;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                *page = theme::segmented(ui, *page, &[crate::i18n::tr("Labyrinth"), crate::i18n::tr("Räume"), crate::i18n::tr("Stadt"), crate::i18n::tr("Kreis")], true);
                ui.add_space(8.0);
                let intro = match *page {
                    0 => crate::i18n::tr("Zufälliges Labyrinth mit Eingang und Ausgang."),
                    1 => crate::i18n::tr("Räume mit eigener Höhe und Bodenstufe, kurze Treppen an den Türen, Gänge mit langen Geraden, runden Kurven und Abzweigungen, dünne Wände – jedes Raumpaar höchstens einmal verbunden, geschlossen und sofort kompilierbar (mit Licht und Spawnpunkt)."),
                    2 => crate::i18n::tr("Straßenraster mit Häusern zufälliger Höhe."),
                    _ => crate::i18n::tr("Die Auswahl im Kreis um ihren Mittelpunkt vervielfältigen."),
                };
                ui.label(RichText::new(intro).color(th.text2));
                ui.add_space(4.0);
                egui::Grid::new("gen").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    let int = |ui: &mut egui::Ui, label: &'static str, v: &mut usize, r: std::ops::RangeInclusive<usize>| {
                        ui.label(crate::i18n::tr(label));
                        ui.add(theme::dv(egui::DragValue::new(v).range(r)));
                        ui.end_row();
                    };
                    match *page {
                        0 => {
                            int(ui, "Zellen (Breite)", &mut o.maze_w, 2..=64);
                            int(ui, "Zellen (Tiefe)", &mut o.maze_h, 2..=64);
                            ui.label(crate::i18n::tr("Gangbreite"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.maze_cell).range(32.0..=1024.0).speed(8.0).suffix(" u")));
                            ui.end_row();
                        }
                        1 => {
                            int(ui, "Räume", &mut o.rooms, 1..=40);
                            int(ui, "Raumgröße min. (Zellen)", &mut o.room_min, 2..=20);
                            int(ui, "Raumgröße max. (Zellen)", &mut o.room_max, 2..=30);
                            int(ui, "Gangbreite (Zellen)", &mut o.corridor, 1..=6);
                            int(ui, "Abstand der Räume (Zellen)", &mut o.room_gap, 2..=30);
                            ui.label(crate::i18n::tr("Windungen"));
                            ui.add(egui::Slider::new(&mut o.winding, 0.0..=1.0).show_value(false)).on_hover_text(crate::i18n::tr("0 = möglichst gerade Gänge, 1 = mehr Abbiegungen; zwischen zwei Abbiegungen liegt immer ein längeres gerades Stück"));
                            ui.end_row();
                            ui.label(crate::i18n::tr("Abbiegungen"));
                            ui.horizontal(|ui| {
                                theme::checkbox(ui, &mut o.curves, crate::i18n::tr("als runde Kurve")).on_hover_text(crate::i18n::tr("Ecken der Gänge werden zu Kurven mit runder Außen- und Innenwand, wo Platz ist; sonst bleibt eine gerade Ecke"));
                                ui.add_enabled_ui(o.curves, |ui| {
                                    ui.label(crate::i18n::tr("Radius"));
                                    ui.add(theme::dv(egui::DragValue::new(&mut o.curve_r).range(0..=4).custom_formatter(|v, _| crate::i18n::count_de(v as usize, crate::i18n::tr("Zelle"), crate::i18n::tr("Zellen"))).custom_parser(|t| t.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<f64>().ok()))).on_hover_text(crate::i18n::tr("Innenradius der Kurve; 0 = nur die Außenwand rund"));
                                });
                            });
                            ui.end_row();
                            ui.label(crate::i18n::tr("Zusätzliche Verbindungen"));
                            ui.add(egui::Slider::new(&mut o.extra_links, 0.0..=1.0).show_value(false)).on_hover_text(crate::i18n::tr("Mehr Verbindungen ergeben Schleifen und Kreuzungen; 0 = jeder Raum genau über einen Weg erreichbar"));
                            ui.end_row();
                            int(ui, "Rastergröße (Zellen)", &mut o.grid, 12..=128);
                            ui.label(crate::i18n::tr("Zellgröße"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.unit).range(16.0..=256.0).speed(4.0).suffix(" u")));
                            ui.end_row();
                            ui.label(crate::i18n::tr("Raumhöhe"));
                            ui.horizontal(|ui| {
                                ui.add(theme::dv(egui::DragValue::new(&mut o.room_h_min).range(64.0..=2048.0).speed(8.0).suffix(" u")));
                                ui.label("–");
                                ui.add(theme::dv(egui::DragValue::new(&mut o.room_h_max).range(64.0..=4096.0).speed(8.0).suffix(" u")));
                            });
                            ui.end_row();
                            ui.label(crate::i18n::tr("Ganghöhe"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.corridor_h).range(72.0..=1024.0).speed(8.0).suffix(" u")));
                            ui.end_row();
                            ui.label(crate::i18n::tr("Höhenstufen"));
                            ui.horizontal(|ui| {
                                ui.add(theme::dv(egui::DragValue::new(&mut o.levels).range(0..=8))).on_hover_text(crate::i18n::tr("Wie viele Stufen Böden nach oben und unten springen dürfen (0 = alle gleich hoch)"));
                                ui.label("×");
                                ui.add(theme::dv(egui::DragValue::new(&mut o.level_step).range(16.0..=512.0).speed(8.0).suffix(" u")));
                            });
                            ui.end_row();
                        }
                        2 => {
                            int(ui, "Blöcke (Breite)", &mut o.blocks_x, 1..=24);
                            int(ui, "Blöcke (Tiefe)", &mut o.blocks_y, 1..=24);
                            ui.label(crate::i18n::tr("Blockgröße"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.block).range(128.0..=4096.0).speed(16.0).suffix(" u")));
                            ui.end_row();
                            ui.label(crate::i18n::tr("Straßenbreite"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.street).range(64.0..=1024.0).speed(8.0).suffix(" u")));
                            ui.end_row();
                            ui.label(crate::i18n::tr("Haushöhe"));
                            ui.horizontal(|ui| {
                                ui.add(theme::dv(egui::DragValue::new(&mut o.bld_min).range(32.0..=4096.0).speed(16.0).suffix(" u")));
                                ui.label("–");
                                ui.add(theme::dv(egui::DragValue::new(&mut o.bld_max).range(32.0..=8192.0).speed(16.0).suffix(" u")));
                            });
                            ui.end_row();
                        }
                        _ => {
                            int(ui, "Anzahl (mit Original)", &mut o.ring_count, 2..=64);
                            ui.label(crate::i18n::tr("Winkelbereich"));
                            ui.add(theme::dv(egui::DragValue::new(&mut o.ring_angle).range(1.0..=360.0).speed(1.0).suffix("°")));
                            ui.end_row();
                        }
                    }
                    if *page == 0 {
                        ui.label(crate::i18n::tr("Raumhöhe"));
                        ui.add(theme::dv(egui::DragValue::new(&mut o.height).range(32.0..=2048.0).speed(8.0).suffix(" u")));
                        ui.end_row();
                    }
                    if *page < 3 {
                        ui.label(crate::i18n::tr("Wandstärke"));
                        let r = ui.add(theme::dv(egui::DragValue::new(&mut o.wall).range(1.0..=128.0).speed(1.0).suffix(" u")));
                        if *page == 1 {
                            r.on_hover_text(crate::i18n::tr("Wände sitzen als dünne Platte am Rand der Zelle, unabhängig von der Zellgröße (höchstens eine halbe Zelle)"));
                        }
                        ui.end_row();
                        ui.label(crate::i18n::tr("Zufallszahl"));
                        ui.horizontal(|ui| {
                            ui.add(theme::dv(egui::DragValue::new(&mut o.seed)));
                            if ui.small_button(crate::i18n::tr("Würfeln")).clicked() {
                                o.seed = o.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 16;
                            }
                        });
                        ui.end_row();
                    }
                });
                if *page == 1 {
                    theme::checkbox(ui, &mut o.lights, crate::i18n::tr("Licht in jedem Raum, Spawnpunkt im ersten"));
                }
                if *page < 3 {
                    theme::section(ui, crate::i18n::tr("Texturen"));
                    let rows: [(&str, &mut String); 3] = [("Boden", &mut o.floor_material), ("Wände", &mut o.material), ("Decke", &mut o.ceiling_material)];
                    for (k, (label, mat)) in rows.into_iter().enumerate() {
                        if *page != 1 && k == 2 {
                            continue; // Labyrinth und Stadt haben keine Decke
                        }
                        ui.horizontal(|ui| {
                            ui.add_sized([60.0, 20.0], egui::Label::new(crate::i18n::tr(label)));
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                            match thumbs[k] {
                                Some(id) => {
                                    egui::Image::new((id, r.size())).corner_radius(3).paint_at(ui, r);
                                }
                                None => {
                                    ui.painter().rect_filled(r, 3.0, th.raised);
                                }
                            }
                            ui.add(egui::TextEdit::singleline(mat).font(theme::mono(11.5)).desired_width(230.0));
                            if ui.small_button(crate::i18n::tr("← aktuelle")).on_hover_text(crate::i18n::tr_dyn(&format!("Aktuelle Textur übernehmen: {cur_mat}"))).clicked() {
                                *mat = cur_mat.clone();
                            }
                        });
                    }
                    let tip = if *page == 1 { crate::i18n::tr("Jeder Raum und jeder Gang wird eine eigene Gruppe – einzeln anklicken, verschieben oder mit Strg+U auflösen. Eingefügt wird dort, wohin die 3D-Kamera blickt.") } else { crate::i18n::tr("Eingefügt wird dort, wohin die 3D-Kamera blickt.") };
                    ui.label(RichText::new(tip).size(12.0).color(th.text3));
                }
            }
            theme::Part::Footer => {
                let ok = *page < 3 || has_sel;
                if ui.add_enabled(ok, egui::Button::new(RichText::new(crate::i18n::tr("Erzeugen")).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).on_disabled_hover_text(crate::i18n::tr("Erst Objekte auswählen")).clicked() {
                    run = true;
                }
                theme::footer_hint(ui, crate::i18n::tr("Ein Rückgängig-Schritt"));
            }
            _ => {}
        });
        self.win.procgen = open;
        if run {
            let origin = self.gen_origin();
            let g = self.gen.clone();
            match self.gen_page {
                0 => {
                    crate::procgen::maze(&mut self.ed, &g, origin);
                }
                1 => {
                    crate::procgen::dungeon(&mut self.ed, &g, origin);
                }
                2 => {
                    crate::procgen::city(&mut self.ed, &g, origin);
                }
                _ => {
                    let pivot = self.ed.selection_bounds().center();
                    crate::procgen::ring(&mut self.ed, &g, DVec3::new(pivot.x, pivot.y, pivot.z));
                }
            }
        }
    }

    pub(crate) fn tiles_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.win.tiles {
            self.ed.tiles.active = false;
            return;
        }
        let th = theme::t();
        let mut open = true;
        let cur_mat = self.ed.material.clone();
        let d = theme::Dlg::new(crate::i18n::tr("Tile-Set"), 470.0).footer();
        let t = &mut self.ed.tiles;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                theme::section(ui, crate::i18n::tr("Bausteine · Innenraum"));
                let bw = ((ui.available_width() - 2.0 * 6.0) / 3.0).floor();
                for row in crate::tiles::TILES.chunks(3).enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (k, (name, desc)) in row.1.iter().enumerate() {
                            let idx = row.0 * 3 + k;
                            let sel = t.kind == idx;
                            let b = egui::Button::new(RichText::new(crate::i18n::tr(name)).color(if sel { th.accent_text } else { th.text })).min_size(Vec2::new(bw, 30.0)).fill(if sel { th.accent_soft } else { th.raised }).stroke(egui::Stroke::new(1.0f32, if sel { th.accent } else { th.line }));
                            if ui.add(b).on_hover_text(crate::i18n::tr(desc)).clicked() {
                                t.kind = idx;
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::tr("Drehung"));
                    let r = theme::segmented(ui, t.rot as usize, &["0°", "90°", "180°", "270°"], false);
                    t.rot = r as u8;
                    ui.label(RichText::new(crate::i18n::tr("(R in der 3D-Ansicht)")).size(12.0).color(th.text3));
                });
                theme::section(ui, crate::i18n::tr("Raster"));
                egui::Grid::new("tiles").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label(crate::i18n::tr("Feldgröße"));
                    let cur = [128.0, 256.0, 512.0].iter().position(|v| *v == t.size).unwrap_or(1);
                    t.size = [128.0, 256.0, 512.0][theme::segmented(ui, cur, &["128", "256", "512"], false)];
                    ui.end_row();
                    ui.label(crate::i18n::tr("Raumhöhe"));
                    ui.add(theme::dv(egui::DragValue::new(&mut t.height).range(64.0..=1024.0).speed(8.0).suffix(" u")));
                    ui.end_row();
                    ui.label(crate::i18n::tr("Bauebene (Höhe)"));
                    ui.horizontal(|ui| {
                        ui.add(theme::dv(egui::DragValue::new(&mut t.level).speed(8.0).suffix(" u")));
                        if ui.small_button(crate::i18n::tr("Eine Etage höher")).clicked() {
                            t.level += t.height + t.wall;
                        }
                        if ui.small_button(crate::i18n::tr("tiefer")).clicked() {
                            t.level -= t.height + t.wall;
                        }
                    });
                    ui.end_row();
                });
                theme::checkbox(ui, &mut t.ceiling, crate::i18n::tr("Mit Decke"));
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::tr("Wände: aktuelle Textur")).on_hover_text(cur_mat.clone()).clicked() {
                        t.wall_material = cur_mat.clone();
                    }
                    if ui.button(crate::i18n::tr("Boden: aktuelle Textur")).on_hover_text(cur_mat.clone()).clicked() {
                        t.floor_material = cur_mat.clone();
                    }
                });
                ui.add_space(6.0);
                theme::checkbox(ui, &mut t.active, RichText::new(crate::i18n::tr("Setzen aktiv – in 3D klicken oder ziehen, Shift entfernt")).strong());
            }
            theme::Part::Footer => {
                theme::footer_hint(ui, crate::i18n::tr("Jeder Baustein ist eine Gruppe aus Brushes"));
            }
            _ => {}
        });
        self.win.tiles = open;
        if !open {
            self.ed.tiles.active = false;
        }
    }
}
