//! Abschnitt „Inhalte in die BSP packen“ im Dialog „Kompilieren & Exportieren“:
//! Liste der eigenen Dateien prüfen, an-/abwählen, ergänzen. Die Auswahl gilt beim Kompilieren.

use crate::assets::{PackEntry, PackSource};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

#[derive(Default)]
pub struct PackDlg {
    pub list: Vec<PackEntry>,
    pub filter: String,
    pub scanned: bool,
    pub log: Vec<String>,
    /// Kompilieren-Dialog war im letzten Bild offen (beim Öffnen neu ermitteln)
    pub shown: bool,
}

fn mb(b: u64) -> String {
    crate::i18n::size(b)
}

impl crate::app::App {
    /// BSP, die zuletzt kompiliert wurde: neben der VMF, sonst im maps-Ordner des Spiels
    pub(crate) fn default_bsp(&self) -> Option<PathBuf> {
        let vmf = self.ed.path.as_ref()?;
        let local = vmf.with_extension("bsp");
        if local.is_file() {
            return Some(local);
        }
        let build = vmf.parent()?.join("amboss_build").join(local.file_name()?);
        if build.is_file() {
            return Some(build);
        }
        let name = vmf.file_stem()?.to_string_lossy().to_string();
        let game = PathBuf::from(&self.settings.game_dir).join("maps").join(format!("{name}.bsp"));
        game.is_file().then_some(game)
    }

    pub(crate) fn pack_scan(&mut self) {
        let Some(fs) = self.mats.fs() else {
            self.pack.log = vec![crate::i18n::tr("Spielinhalte sind noch nicht geladen (Einstellungen → Spielverzeichnis).").into()];
            return;
        };
        self.pack.list = crate::assets::pack_candidates(&self.ed, &fs, self.ed.path.as_deref());
        self.pack.scanned = true;
    }

    fn pack_save_overrides(&mut self) {
        if let Some(vmf) = &self.ed.path {
            let _ = std::fs::write(crate::assets::overrides_path(vmf), crate::assets::overrides_text(&self.pack.list));
        }
    }

    /// Eigene Inhalte sofort in die zuletzt kompilierte BSP packen (ohne neu zu kompilieren)
    pub(crate) fn pack_now(&mut self) {
        let Some(bsp) = self.default_bsp() else {
            self.pack.log = vec![crate::i18n::tr("Keine kompilierte BSP gefunden – erst kompilieren").into()];
            return;
        };
        if !self.pack.scanned {
            self.pack_scan();
        }
        let fs = self.mats.fs();
        let mut files = Vec::new();
        let mut log = Vec::new();
        for e in self.pack.list.iter().filter(|e| e.include) {
            match e.read(fs.as_deref()) {
                Some(b) => files.push((e.path.clone(), b)),
                None => log.push(format!("Nicht lesbar: {}", e.path)),
            }
        }
        match crate::bsppack::pack(&bsp, &files) {
            Ok(r) => log.push(format!("{} Datei(en) in {} gepackt: {} neu, {} ersetzt, {} waren schon drin · {}", files.len(), bsp.display(), r.added, r.replaced, r.kept, mb(r.bytes as u64))),
            Err(e) => log.push(format!("Packen fehlgeschlagen: {e}")),
        }
        self.pack.log = log;
    }

    /// Dateiliste fürs Packen (aufklappbar im Kompilieren-Dialog)
    pub(crate) fn pack_section(&mut self, ui: &mut egui::Ui) {
        use crate::theme;
        if !self.pack.scanned {
            self.pack_scan();
        }
        let th = theme::t();
        let sel: Vec<&PackEntry> = self.pack.list.iter().filter(|e| e.include).collect();
        let head = crate::i18n::tr_dyn(&format!("{} von {} Dateien ausgewählt · {}", sel.len(), self.pack.list.len(), mb(sel.iter().map(|e| e.size).sum())));
        let (mut rescan, mut add_file, mut changed) = (false, false, false);
        egui::CollapsingHeader::new(head).id_salt("packlist").show(ui, |ui| {
            let p = &mut self.pack;
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut p.filter).hint_text(crate::theme::hint(crate::i18n::tr("filtern"))).desired_width(160.0));
                if ui.small_button(crate::i18n::tr("Neu ermitteln")).clicked() {
                    rescan = true;
                }
                if ui.small_button(crate::i18n::tr("Datei hinzufügen…")).clicked() {
                    add_file = true;
                }
                if ui.small_button(crate::i18n::tr("Alle eigenen")).clicked() {
                    for e in p.list.iter_mut() {
                        e.include = !matches!(e.source, PackSource::Addon(_));
                    }
                    changed = true;
                }
                if ui.small_button(crate::i18n::tr("Addons dazu")).clicked() {
                    for e in p.list.iter_mut() {
                        e.include = true;
                    }
                    changed = true;
                }
            });
            let f = p.filter.to_ascii_lowercase();
            egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(6).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("packrows").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                    if p.list.is_empty() {
                        ui.label(RichText::new(crate::i18n::tr("Keine eigenen Inhalte gefunden – alles, was die Map benutzt, gehört zum Spiel.")).color(th.text3).size(11.5));
                    }
                    egui::Grid::new("packgrid").num_columns(4).spacing([10.0, 2.0]).striped(true).show(ui, |ui| {
                        for e in p.list.iter_mut().filter(|e| f.is_empty() || e.path.contains(&f)) {
                            changed |= crate::theme::checkbox(ui, &mut e.include, "").changed();
                            ui.label(RichText::new(&e.path).font(theme::mono(11.0)).color(if e.include { th.text } else { th.text3 }));
                            let (src, c) = match &e.source {
                                PackSource::Loose(_) => (crate::i18n::tr("eigene Datei").to_string(), th.text2),
                                PackSource::Addon(n) => (crate::i18n::tr_dyn(&format!("Addon: {n}")), th.warn),
                                PackSource::Extra(_) => (crate::i18n::tr("hinzugefügt").to_string(), th.accent),
                            };
                            ui.label(RichText::new(src).size(11.0).color(c));
                            ui.label(RichText::new(mb(e.size)).font(theme::mono(10.5)).color(th.text3));
                            ui.end_row();
                        }
                    });
                });
            });
            ui.label(RichText::new(crate::i18n::tr("Eigene Dateien sind vorgewählt, Addon-Inhalte besser im Workshop verlinken")).size(11.0).color(th.text3));
        });
        if rescan {
            self.pack_scan();
        }
        if add_file {
            let start = PathBuf::from(&self.settings.game_dir);
            if let Some(files) = rfd::FileDialog::new().set_directory(start).pick_files() {
                for f in files {
                    match crate::assets::game_path_of(&f) {
                        Some(gp) if !self.pack.list.iter().any(|e| e.path == gp) => {
                            let size = std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
                            self.pack.list.push(PackEntry { path: gp, source: PackSource::Extra(f), size, include: true });
                            changed = true;
                        }
                        Some(_) => {}
                        None => self.pack.log.push(format!("{}: liegt nicht unter materials/, models/, sound/ … – Pfad im Spiel unklar", f.display())),
                    }
                }
            }
        }
        // Auswahl sofort merken – sie gilt beim nächsten Kompilieren
        if changed {
            self.pack_save_overrides();
        }
        for l in &self.pack.log {
            let c = if l.contains("fehlgeschlagen") || l.contains("Nicht lesbar") || l.contains("Keine") || l.contains("failed") { th.err } else { th.text2 };
            ui.label(RichText::new(crate::i18n::tr_dyn(l)).size(11.5).color(c));
        }
    }
}
