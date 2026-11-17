//! Dialog „Map zusammenführen“.

use crate::app::App;
use crate::map::Map;
use crate::merge::{MergeOptions, MergeReport, Placement};
use eframe::egui::{self, RichText};
use glam::DVec3;
use std::path::{Path, PathBuf};

pub(crate) struct MergeDialog {
    pub path: PathBuf,
    pub map: Map,
    pub placement: usize,
    pub offset: [f64; 3],
    pub visgroup: bool,
    pub keep_groups: bool,
    pub diffs: Vec<(String, String, String)>,
    pub take: Vec<bool>,
    pub result: Option<MergeReport>,
}

/// VMF oder Quake/Valve-.map laden
pub fn load_any(path: &Path, tex_scale: f64) -> Result<Map, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("map")) {
        crate::importexport::import_map(&src, tex_scale)
    } else {
        Map::from_vmf(&src)
    }
}

impl App {
    pub(crate) fn start_merge(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Map", &["vmf", "vmx", "map"]).pick_file() else { return };
        match load_any(&p, self.ed.tex_scale) {
            Ok(map) => {
                let diffs = self.ed.world_diffs(&map);
                let take = vec![false; diffs.len()];
                self.merge_dlg = Some(MergeDialog { path: p, map, placement: 0, offset: [0.0; 3], visgroup: true, keep_groups: true, diffs, take, result: None });
            }
            Err(e) => {
                rfd::MessageDialog::new().set_title(crate::i18n::tr("Map konnte nicht geladen werden")).set_description(e).show();
            }
        }
    }

    pub(crate) fn merge_window(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.merge_dlg else { return };
        let mut open = true;
        let mut run = false;
        let mut close = false;
        let name = d.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let th = crate::theme::t();
        let has_result = d.result.is_some();
        let title = if has_result { crate::i18n::tr("Zusammengeführt") } else { crate::i18n::tr("Map zusammenführen") };
        let sub = d.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        crate::theme::dialog(ctx, &mut open, &crate::theme::Dlg::new(title, 560.0).subtitle(sub).id("merge").footer(), |ui, part| match part {
            crate::theme::Part::Body => {
                if let Some(r) = &d.result {
                    ui.horizontal(|ui| {
                        crate::theme::severity_mark(ui, 3);
                        ui.label(crate::theme::strong_text(&crate::i18n::tr_dyn(&r.summary())));
                    });
                    if !r.renamed.is_empty() {
                        crate::theme::section(ui, crate::i18n::tr("Umbenannt"));
                        for (o, n) in r.renamed.iter().take(30) {
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{o} → {n}"))).font(crate::theme::mono(12.0)));
                        }
                    }
                    if !r.warnings.is_empty() {
                        crate::theme::section(ui, crate::i18n::tr("Hinweise"));
                        for w in &r.warnings {
                            ui.horizontal(|ui| {
                                crate::theme::severity_mark(ui, 1);
                                ui.add(egui::Label::new(crate::i18n::tr_dyn(w)).wrap());
                            });
                        }
                    }
                    return;
                }
                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Brushes · {} Entities", d.map.all_solids().count(), d.map.entities.len()))).color(th.text2));
                crate::theme::section(ui, crate::i18n::tr("Position"));
                for (i, l, sub) in [(0usize, crate::i18n::tr("Koordinaten behalten"), crate::i18n::tr("Teile liegen an derselben Stelle wie in der anderen Map")), (1, crate::i18n::tr("Neben die offene Map setzen"), crate::i18n::tr("Verschiebt um die Breite der offenen Map (+X)")), (2, crate::i18n::tr("Verschieben um"), "")] {
                    let sel = d.placement == i;
                    let resp = egui::Frame::new()
                        .fill(if sel { th.accent_soft } else { egui::Color32::TRANSPARENT })
                        .stroke(egui::Stroke::new(1.0f32, if sel { th.accent } else { th.line }))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let _ = ui.radio(sel, "");
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.label(crate::theme::strong_text(l));
                                    if !sub.is_empty() {
                                        ui.label(RichText::new(sub).size(12.0).color(th.text3));
                                    }
                                });
                                if i == 2 {
                                    for v in &mut d.offset {
                                        ui.add_sized([70.0, 28.0], crate::theme::dv(egui::DragValue::new(v)).speed(16.0));
                                    }
                                }
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        d.placement = i;
                    }
                }
                crate::theme::section(ui, crate::i18n::tr("Optionen"));
                crate::theme::checkbox(ui, &mut d.visgroup, crate::i18n::tr_dyn(&format!("In Visgroup „Import: {name}“ legen")));
                crate::theme::checkbox(ui, &mut d.keep_groups, crate::i18n::tr("Gruppen und Visgroups der anderen Map übernehmen"));
                ui.label(RichText::new(crate::i18n::tr("Doppelte Entity-Namen werden automatisch umbenannt (name_2), Verweise ziehen mit.")).size(12.0).color(th.text3));
                if !d.diffs.is_empty() {
                    crate::theme::section(ui, crate::i18n::tr("Unterschiedliche Map-Eigenschaften · Haken = Wert der anderen Map"));
                    egui::Grid::new("mergediffs").striped(true).num_columns(4).show(ui, |ui| {
                        ui.label("");
                        ui.label(RichText::new(crate::i18n::tr("Schlüssel")).size(11.5).color(th.text3));
                        ui.label(RichText::new(crate::i18n::tr("offene Map")).size(11.5).color(th.text3));
                        ui.label(RichText::new(crate::i18n::tr("andere Map")).size(11.5).color(th.text3));
                        ui.end_row();
                        for (i, (k, a, b)) in d.diffs.iter().enumerate() {
                            crate::theme::checkbox(ui, &mut d.take[i], "");
                            ui.label(RichText::new(k).font(crate::theme::mono(12.0)));
                            ui.label(RichText::new(if a.is_empty() { "–" } else { a }).font(crate::theme::mono(12.0)));
                            ui.label(RichText::new(b).font(crate::theme::mono(12.0)));
                            ui.end_row();
                        }
                    });
                }
            }
            crate::theme::Part::Footer => {
                if has_result {
                    if crate::theme::primary_button(ui, crate::i18n::tr("Fertig")).clicked() {
                        close = true;
                    }
                    crate::theme::footer_hint(ui, crate::i18n::tr("Neue Objekte sind ausgewählt · Strg+Z macht alles rückgängig"));
                } else {
                    if crate::theme::primary_button(ui, crate::i18n::tr("Zusammenführen")).clicked() {
                        run = true;
                    }
                    if crate::theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                        close = true;
                    }
                }
            }
            _ => {}
        });
        if run {
            let o = MergeOptions {
                placement: match d.placement {
                    1 => Placement::Beside,
                    2 => Placement::Offset(DVec3::from_array(d.offset)),
                    _ => Placement::Keep,
                },
                visgroup: d.visgroup,
                keep_groups: d.keep_groups,
                take_world_keys: d.diffs.iter().zip(&d.take).filter(|(_, t)| **t).map(|(k, _)| k.0.clone()).collect(),
            };
            let dir = d.path.parent().map(|p| p.to_path_buf());
            match self.ed.merge_map(d.map.clone(), &name, dir.as_deref(), &o) {
                Ok(r) => {
                    let b = self.ed.selection_bounds();
                    for v in &mut self.views2d {
                        v.focus(&b, egui::Vec2::new(700.0, 450.0));
                    }
                    if let Some(d) = &mut self.merge_dlg {
                        d.result = Some(r);
                    }
                }
                Err(e) => self.ed.status = e,
            }
        }
        if close || !open {
            self.merge_dlg = None;
        }
    }
}

// ------------------------------------------------------------------ Map-Stände vergleichen / 3-Wege

pub(crate) struct VersionsDialog {
    pub title: String,
    pub diff: crate::diff::MapDiff,
    pub show: [bool; 3],
    pub merge: Option<crate::diff::Merge3>,
}

fn change_idx(c: crate::diff::Change) -> usize {
    match c {
        crate::diff::Change::Added => 0,
        crate::diff::Change::Removed => 1,
        crate::diff::Change::Modified => 2,
    }
}

impl App {
    fn set_diff_boxes(&mut self, d: &crate::diff::MapDiff) {
        self.ed.diff_boxes = d.objects.iter().filter(|o| !o.bounds.is_empty()).map(|o| (o.bounds, o.change.color())).collect();
        self.ed.touch();
    }

    /// Offene Map mit einer Datei (älterer/anderer Stand) vergleichen
    pub(crate) fn start_compare(&mut self) {
        let Some(p) = rfd::FileDialog::new().set_title(crate::i18n::tr("Älteren oder anderen Stand der Map wählen")).add_filter("Map", &["vmf", "vmx"]).pick_file() else { return };
        match load_any(&p, self.ed.tex_scale) {
            Ok(old) => {
                let d = crate::diff::diff(&old, &self.ed.map, &self.ed);
                self.ed.status = format!("Vergleich mit {}: {}", p.display(), d.summary());
                self.set_diff_boxes(&d);
                self.versions_dlg = Some(VersionsDialog { title: format!("Vergleich mit {}", p.file_name().unwrap_or_default().to_string_lossy()), diff: d, show: [true; 3], merge: None });
            }
            Err(e) => self.ed.status = e,
        }
    }

    /// 3-Wege: offene Map = unsere Version, dazu Basis und Version der anderen Person
    pub(crate) fn start_merge3(&mut self) {
        let Some(base_p) = rfd::FileDialog::new().set_title(crate::i18n::tr("1/2: Gemeinsamen Ausgangsstand (Basis) wählen")).add_filter("Map", &["vmf", "vmx"]).pick_file() else { return };
        let Some(their_p) = rfd::FileDialog::new().set_title(crate::i18n::tr("2/2: Version der anderen Person wählen")).add_filter("Map", &["vmf", "vmx"]).pick_file() else { return };
        let (base, theirs) = match (load_any(&base_p, self.ed.tex_scale), load_any(&their_p, self.ed.tex_scale)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                self.ed.status = e;
                return;
            }
        };
        let before = self.ed.map.clone();
        let mut m = self.ed.map.clone();
        let r = crate::diff::merge3(&base, &mut m, &theirs);
        self.ed.checkpoint();
        self.ed.map = m;
        self.ed.sel.clear();
        self.ed.faces.clear();
        self.ed.vsel.clear();
        let d = crate::diff::diff(&before, &self.ed.map, &self.ed);
        self.set_diff_boxes(&d);
        self.ed.status = format!("3-Wege-Zusammenführen: {}", r.summary());
        self.versions_dlg = Some(VersionsDialog { title: format!("Zusammengeführt mit {}", their_p.file_name().unwrap_or_default().to_string_lossy()), diff: d, show: [true; 3], merge: Some(r) });
    }

    pub(crate) fn versions_window(&mut self, ctx: &egui::Context) {
        let Some(dlg) = &mut self.versions_dlg else { return };
        let mut open = true;
        let mut goto: Option<(Option<crate::editor::Obj>, crate::geometry::Aabb)> = None;
        let mut clear = false;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(dlg.title.clone(), 560.0).id("versions"), |ui| {
            if let Some(m) = &dlg.merge {
                ui.label(RichText::new(crate::i18n::tr_dyn(&m.summary())).strong());
                if m.conflicts.is_empty() {
                    ui.colored_label(egui::Color32::from_rgb(110, 230, 120), crate::i18n::tr("Keine Konflikte."));
                } else {
                    ui.label(crate::i18n::tr("Konflikte (Klick wählt das Objekt):"));
                    egui::ScrollArea::vertical().id_salt("conf").max_height(160.0).show(ui, |ui| {
                        for c in &m.conflicts {
                            if ui.add(egui::Label::new(RichText::new(crate::i18n::tr_dyn(&format!("⚠ {}", c.text))).color(egui::Color32::from_rgb(255, 180, 80))).sense(egui::Sense::click())).clicked() {
                                let o = match c.kind {
                                    crate::diff::Kind::Solid => crate::editor::Obj::Solid(c.id),
                                    crate::diff::Kind::Entity => crate::editor::Obj::Entity(c.id),
                                };
                                goto = Some((Some(o), crate::geometry::Aabb::EMPTY));
                            }
                        }
                    });
                }
                ui.label(RichText::new(crate::i18n::tr("Strg+Z nimmt das Zusammenführen komplett zurück.")).small().weak());
                ui.separator();
            }
            ui.label(crate::i18n::tr_dyn(&dlg.diff.summary()));
            ui.horizontal(|ui| {
                for (i, c) in [crate::diff::Change::Added, crate::diff::Change::Removed, crate::diff::Change::Modified].into_iter().enumerate() {
                    let col = c.color();
                    crate::theme::checkbox(ui, &mut dlg.show[i], RichText::new(crate::i18n::tr_dyn(&format!("{} ({})", c.label(), dlg.diff.count(c)))).color(egui::Color32::from_rgb(col[0], col[1], col[2])));
                }
                if ui.button(crate::i18n::tr("Markierungen aus")).clicked() {
                    clear = true;
                }
            });
            egui::ScrollArea::vertical().id_salt("difflist").show(ui, |ui| {
                for (k, a, b) in &dlg.diff.world {
                    ui.label(crate::i18n::tr_dyn(&format!("Map-Eigenschaft {k}: {} -> {}", a.as_deref().unwrap_or("–"), b.as_deref().unwrap_or("–"))));
                }
                for o in dlg.diff.objects.iter().filter(|o| dlg.show[change_idx(o.change)]) {
                    let col = o.change.color();
                    let text = RichText::new(crate::i18n::tr_dyn(&format!("{:<9} {}", o.change.label(), o.label))).color(egui::Color32::from_rgb(col[0], col[1], col[2])).monospace();
                    if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(crate::i18n::tr("Klick: anwählen und hinzoomen")).clicked() {
                        let obj = (o.change != crate::diff::Change::Removed).then(|| match o.kind {
                            crate::diff::Kind::Solid => crate::editor::Obj::Solid(o.id),
                            crate::diff::Kind::Entity => crate::editor::Obj::Entity(o.id),
                        });
                        goto = Some((obj, o.bounds));
                    }
                }
            });
        });
        if let Some((obj, mut b)) = goto {
            if let Some(o) = obj {
                let exists = match o {
                    crate::editor::Obj::Solid(id) => self.ed.map.solid(id).is_some(),
                    crate::editor::Obj::Entity(id) => self.ed.map.entity(id).is_some(),
                };
                if exists {
                    self.ed.sel = [o].into_iter().collect();
                    b = self.ed.selection_bounds();
                }
            }
            if !b.is_empty() {
                for v in &mut self.views2d {
                    v.focus(&b, egui::Vec2::new(700.0, 450.0));
                }
                self.view3d.focus(&b);
            }
            self.ed.touch();
        }
        if clear || !open {
            self.ed.diff_boxes.clear();
            self.ed.touch();
        }
        if !open {
            self.versions_dlg = None;
        }
    }
}
