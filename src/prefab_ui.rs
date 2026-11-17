//! Prefab-Bibliothek: Fenster mit Kategorien, Vorschaubildern, Drag & Drop und Hammer-Import.

use crate::app::App;
use crate::prefabs::{self, PrefabDrag, Source};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Vec2};
use std::path::PathBuf;

impl App {
    pub(crate) fn prefab_dir(&self) -> PathBuf {
        if !self.settings.prefab_dir.is_empty() {
            return PathBuf::from(&self.settings.prefab_dir);
        }
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("prefabs"))).unwrap_or_else(|| PathBuf::from("prefabs"))
    }

    pub(crate) fn rescan_prefabs(&mut self) {
        self.prefab_entries = prefabs::scan(&self.prefab_dir());
        self.prefab_thumbs.clear();
        self.ed.armed_prefab = None;
    }

    /// Platzierungswunsch aus den Ansichten ausführen
    pub(crate) fn process_prefab_drop(&mut self) {
        let Some((idx, anchor, normal)) = self.ed.prefab_drop.take() else { return };
        let keep_armed = self.ctx.input(|i| i.modifiers.shift);
        if !keep_armed {
            self.ed.armed_prefab = None;
        }
        let Some(entry) = self.prefab_entries.get(idx).cloned() else { return };
        match prefabs::load(&entry).and_then(|m| self.ed.insert_prefab_map(m, anchor, normal)) {
            Ok(_) => self.ed.status = format!("Prefab „{}“ eingefügt{}", entry.name, if keep_armed { " (Shift: weiter platzieren)" } else { "" }),
            Err(e) => self.ed.status = format!("Prefab „{}“: {e}", entry.name),
        }
    }

    fn prefab_thumb(&mut self, ctx: &egui::Context, idx: usize) -> Option<egui::TextureHandle> {
        let e = self.prefab_entries.get(idx)?;
        let stamp = match &e.source {
            Source::File(p) => std::fs::metadata(p).and_then(|m| m.modified()).map(|t| format!("{t:?}")).unwrap_or_default(),
            _ => String::new(),
        };
        let key = format!("{}|{stamp}", e.id());
        if let Some(t) = self.prefab_thumbs.get(&key) {
            return Some(t.clone());
        }
        // höchstens ein neues Vorschaubild pro Bild rendern (flüssige Oberfläche)
        if self.prefab_thumbs.len() > 400 {
            self.prefab_thumbs.clear();
        }
        let m = prefabs::load(e).ok()?;
        let size = 112;
        let img = prefabs::render_thumb(&m, size);
        let t = ctx.load_texture(format!("prefab:{key}"), egui::ColorImage::from_rgba_unmultiplied([size, size], &img), egui::TextureOptions::LINEAR);
        self.prefab_thumbs.insert(key, t.clone());
        Some(t)
    }

    pub(crate) fn prefab_window(&mut self, ctx: &egui::Context) {
        if !self.win.prefabs {
            return;
        }
        let mut open = true;
        let dir = self.prefab_dir();
        let mut rescan = false;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Prefab-Bibliothek"), 760.0).height(640.0).no_scroll(), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(dir.display().to_string()).small());
                if ui.small_button("⟳").on_hover_text(crate::i18n::tr("Neu einlesen")).clicked() {
                    rescan = true;
                }
            });
            // Speichern
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Speichern als"));
                ui.add(egui::TextEdit::singleline(&mut self.dlg.prefab_name).desired_width(140.0));
                ui.label("in");
                let cats = prefabs::categories(&dir);
                egui::ComboBox::from_id_salt("pfcat").selected_text(if self.dlg.prefab_category.is_empty() { crate::i18n::tr("(Hauptordner)").to_string() } else { self.dlg.prefab_category.clone() }).width(140.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.dlg.prefab_category, String::new(), crate::i18n::tr("(Hauptordner)"));
                    for c in cats {
                        ui.selectable_value(&mut self.dlg.prefab_category, c.clone(), c);
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut self.dlg.prefab_category).hint_text(crate::theme::hint(crate::i18n::tr("neue Kategorie"))).desired_width(100.0));
                if ui.add_enabled(!self.ed.sel.is_empty(), egui::Button::new(crate::i18n::tr("Auswahl speichern"))).clicked() {
                    let d = if self.dlg.prefab_category.trim().is_empty() { dir.clone() } else { dir.join(self.dlg.prefab_category.trim()) };
                    let _ = std::fs::create_dir_all(&d);
                    let p = d.join(format!("{}.vmf", self.dlg.prefab_name.trim()));
                    match self.ed.save_prefab(&p) {
                        Ok(n) => {
                            self.ed.status = format!("Prefab gespeichert ({n} Objekte): {}", p.display());
                            rescan = true;
                        }
                        Err(e) => self.ed.status = e,
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(crate::i18n::tr("Hammer-Bibliothek (.ol/.rmf) importieren…")).on_hover_text(crate::i18n::tr("Wandelt alle Prefabs in VMF-Dateien in einem eigenen Unterordner um")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Hammer-Prefabs", &["ol", "rmf"]).pick_file() {
                        match import_hammer(&p, &dir) {
                            Ok(n) => {
                                self.ed.status = format!("{n} Prefab(s) aus {} importiert", p.display());
                                rescan = true;
                            }
                            Err(e) => self.ed.status = e,
                        }
                    }
                }
                if ui.button(crate::i18n::tr("Mitgelieferte als VMF kopieren")).on_hover_text(crate::i18n::tr("Zum Anpassen in den Prefab-Ordner schreiben")).clicked() {
                    match prefabs::export_builtins(&dir) {
                        Ok(n) => {
                            self.ed.status = format!("{n} Prefabs nach {} kopiert", dir.join("Bibliothek").display());
                            rescan = true;
                        }
                        Err(e) => self.ed.status = e,
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tr("Suchen:"));
                ui.add(egui::TextEdit::singleline(&mut self.dlg.prefab_filter).desired_width(200.0));
                ui.label(RichText::new(crate::i18n::tr("Ziehen in eine Ansicht oder anklicken und dann in eine Ansicht klicken (Shift = mehrfach)")).small());
            });
            ui.separator();
            let f = self.dlg.prefab_filter.to_ascii_lowercase();
            let mut cats: Vec<String> = self.prefab_entries.iter().map(|e| e.category.clone()).collect();
            cats.dedup();
            let mut seen = Vec::new();
            cats.retain(|c| {
                let new = !seen.contains(c);
                seen.push(c.clone());
                new
            });
            let mut arm: Option<usize> = None;
            let mut delete: Option<PathBuf> = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for cat in cats {
                    let idxs: Vec<usize> = (0..self.prefab_entries.len())
                        .filter(|i| self.prefab_entries[*i].category == cat)
                        .filter(|i| f.is_empty() || self.prefab_entries[*i].name.to_ascii_lowercase().contains(&f) || self.prefab_entries[*i].description.to_ascii_lowercase().contains(&f))
                        .collect();
                    if idxs.is_empty() {
                        continue;
                    }
                    egui::CollapsingHeader::new(RichText::new(format!("{} ({})", crate::i18n::tr_dyn(&cat), idxs.len())).strong()).id_salt(("pf", &cat)).default_open(true).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for i in idxs {
                                let thumb = self.prefab_thumb(ctx, i);
                                let mut e = self.prefab_entries[i].clone();
                                e.name = crate::i18n::tr_dyn(&e.name);
                                e.description = crate::i18n::tr_dyn(&e.description);
                                let armed = self.ed.armed_prefab == Some(i);
                                let id = egui::Id::new(("prefab_tile", i));
                                let inner = ui.dnd_drag_source(id, PrefabDrag(i), |ui| {
                                    let (rect, _) = ui.allocate_exact_size(Vec2::new(124.0, 142.0), Sense::hover());
                                    let bg = if armed { Color32::from_rgb(40, 90, 50) } else { Color32::from_gray(40) };
                                    ui.painter().rect_filled(rect, egui::CornerRadius::same(4), bg);
                                    if let Some(t) = &thumb {
                                        egui::Image::new((t.id(), Vec2::splat(112.0))).paint_at(ui, egui::Rect::from_min_size(rect.min + Vec2::new(6.0, 4.0), Vec2::splat(112.0)));
                                    }
                                    let name: String = if e.name.chars().count() > 18 { format!("{}…", e.name.chars().take(17).collect::<String>()) } else { e.name.clone() };
                                    ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 12.0), Align2::CENTER_CENTER, name, FontId::proportional(11.0), Color32::from_gray(220));
                                });
                                let r = ui.interact(inner.response.rect, id.with("click"), Sense::click());
                                let tip = if e.description.is_empty() { e.name.clone() } else { format!("{}\n{}", e.name, e.description) };
                                let r = r.on_hover_text(tip);
                                if r.clicked() {
                                    arm = Some(i);
                                }
                                if let Source::File(p) = &e.source {
                                    r.context_menu(|ui| {
                                        if ui.button(crate::i18n::tr("Datei löschen")).clicked() {
                                            delete = Some(p.clone());
                                            ui.close_menu();
                                        }
                                        if ui.button(crate::i18n::tr("Als Map öffnen")).clicked() {
                                            self.open_in_tab(p.clone());
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                        });
                    });
                }
            });
            if let Some(i) = arm {
                self.ed.armed_prefab = if self.ed.armed_prefab == Some(i) { None } else { Some(i) };
                if self.ed.armed_prefab.is_some() {
                    self.ed.status = format!("Prefab „{}“: in eine Ansicht klicken", self.prefab_entries[i].name);
                }
            }
            if let Some(p) = delete {
                if std::fs::remove_file(&p).is_ok() {
                    rescan = true;
                }
            }
        });
        if rescan {
            self.rescan_prefabs();
        }
        self.win.prefabs = open;
    }
}

/// Hammer-.ol oder -.rmf in VMF-Dateien umwandeln
pub fn import_hammer(path: &std::path::Path, prefab_dir: &std::path::Path) -> Result<usize, String> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "import".into());
    let out = prefab_dir.join("Hammer").join(&stem);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let is_ol = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("ol"));
    let list: Vec<(String, Vec<u8>)> = if is_ol {
        prefabs::read_ol(path)?.into_iter().map(|e| (e.name, e.data)).collect()
    } else {
        vec![(stem.clone(), std::fs::read(path).map_err(|e| e.to_string())?)]
    };
    let mut n = 0;
    for (name, data) in list {
        let m = prefabs::parse_rmf(&data).map_err(|e| format!("{name}: {e}"))?;
        let file: String = name.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' { c } else { '_' }).collect();
        std::fs::write(out.join(format!("{}.vmf", file.trim())), m.to_vmf()).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    #[test]
    fn import_ol_to_folder() {
        let dir = std::env::temp_dir().join(format!("amboss_olimp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ol = crate::prefabs::tests::make_ol(&[("Kiste A", crate::prefabs::tests::make_rmf()), ("Kiste/B", crate::prefabs::tests::make_rmf())]);
        let p = dir.join("alt.ol");
        std::fs::write(&p, ol).unwrap();
        assert_eq!(super::import_hammer(&p, &dir).unwrap(), 2);
        assert!(dir.join("Hammer/alt/Kiste A.vmf").is_file());
        assert!(dir.join("Hammer/alt/Kiste_B.vmf").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
