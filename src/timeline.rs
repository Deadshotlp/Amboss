//! Zeitleiste der Speicherstände: jedes Speichern legt zusätzlich eine Kopie mit Zeitstempel ab
//! (`<Map-Ordner>/autosave/verlauf/<map>/<Zeit>.vmf`). Zusammen mit den Autosaves erscheinen sie auf
//! einer Zeitleiste – mit Draufsicht, Unterschied zum jetzigen Stand, Öffnen und Wiederherstellen.

use crate::app::App;
use crate::geometry::Aabb;
use crate::i18n::tr;
use crate::map::Map;
use eframe::egui::{self, Color32, RichText, Stroke};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// beim Speichern abgelegt
    Saved,
    Autosave,
    /// die Datei selbst (zuletzt gespeichert)
    File,
}

#[derive(Debug, Clone)]
pub struct Snap {
    pub path: PathBuf,
    pub time: SystemTime,
    pub kind: SnapKind,
    pub size: u64,
}

/// Ordner der Speicherstände einer Map
pub fn history_dir(map_path: &Path) -> PathBuf {
    let name = map_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "map".into());
    map_path.parent().unwrap_or(Path::new(".")).join("autosave").join("verlauf").join(name)
}

/// Kopie des eben gespeicherten Stands ablegen und alte Stände über `keep` hinaus löschen
pub fn record(map_path: &Path, vmf: &str, keep: usize) -> std::io::Result<PathBuf> {
    let dir = history_dir(map_path);
    std::fs::create_dir_all(&dir)?;
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let p = dir.join(format!("{secs}.vmf"));
    std::fs::write(&p, vmf)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "vmf")).collect();
    files.sort();
    while files.len() > keep.max(1) {
        let _ = std::fs::remove_file(files.remove(0));
    }
    Ok(p)
}

fn snap_of(path: PathBuf, kind: SnapKind) -> Option<Snap> {
    let md = std::fs::metadata(&path).ok()?;
    Some(Snap { time: md.modified().unwrap_or(UNIX_EPOCH), size: md.len(), path, kind })
}

/// Alle Stände einer Map: Verlauf, Autosaves und die Datei selbst – älteste zuerst
pub fn list(map_path: Option<&Path>, autosave_dir: &Path, map_name: &str) -> Vec<Snap> {
    let mut out = Vec::new();
    if let Some(mp) = map_path {
        if let Ok(rd) = std::fs::read_dir(history_dir(mp)) {
            out.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "vmf")).filter_map(|p| snap_of(p, SnapKind::Saved)));
        }
        out.extend(snap_of(mp.to_path_buf(), SnapKind::File));
    }
    if let Ok(rd) = std::fs::read_dir(autosave_dir) {
        let prefix = format!("{map_name}_autosave");
        out.extend(rd.flatten().map(|e| e.path()).filter(|p| p.file_stem().is_some_and(|s| s.to_string_lossy().starts_with(&prefix))).filter_map(|p| snap_of(p, SnapKind::Autosave)));
    }
    out.sort_by_key(|s| s.time);
    out
}

/// Ausgewerteter Stand (zwischengespeichert): Zahlen, Umrisse von oben, Unterschied zum jetzigen Stand
pub struct SnapInfo {
    pub brushes: usize,
    pub entities: usize,
    /// Kanten von oben (XY)
    pub lines: Vec<[[f32; 2]; 2]>,
    pub bounds: Aabb,
    pub diff: crate::diff::MapDiff,
    pub map: Map,
}

pub fn analyze(path: &Path, current: &crate::editor::Editor) -> Result<SnapInfo, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let map = Map::from_vmf(&text)?;
    let mut lines = Vec::new();
    let mut b = Aabb::EMPTY;
    for (_, s) in map.all_solids() {
        b = b.union(&s.bounds);
        // nur die Oberseiten: weniger Linien, gleiche Draufsicht
        for (i, poly) in s.polys.iter().enumerate() {
            if s.planes.get(i).copied().flatten().is_some_and(|p| p.normal.z > 0.5) {
                for k in 0..poly.len() {
                    let (a, c) = (poly[k], poly[(k + 1) % poly.len()]);
                    lines.push([[a.x as f32, a.y as f32], [c.x as f32, c.y as f32]]);
                }
            }
        }
    }
    let diff = crate::diff::diff(&map, &current.map, current);
    Ok(SnapInfo { brushes: map.all_solids().count(), entities: map.entities.len(), lines, bounds: b, diff, map })
}

#[derive(Default)]
pub struct TimelineUi {
    pub open: bool,
    pub snaps: Vec<Snap>,
    pub sel: Option<usize>,
    pub cache: HashMap<PathBuf, Result<SnapInfo, String>>,
    /// Stand der Map, für den die Unterschiede gerechnet wurden
    pub cache_rev: u64,
}

fn when(t: SystemTime) -> String {
    let secs = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=59 => tr("gerade eben").into(),
        60..=3599 => crate::i18n::tr_dyn(&format!("vor {} Min.", secs / 60)),
        3600..=86399 => crate::i18n::tr_dyn(&format!("vor {} Std.", secs / 3600)),
        86400..=172799 => tr("gestern").into(),
        _ => crate::i18n::tr_dyn(&format!("vor {} Tagen", secs / 86400)),
    }
}

fn kind_color(k: SnapKind) -> Color32 {
    match k {
        SnapKind::Saved => Color32::from_rgb(110, 180, 255),
        SnapKind::Autosave => Color32::from_rgb(160, 160, 170),
        SnapKind::File => Color32::from_rgb(90, 210, 120),
    }
}

fn kind_label(k: SnapKind) -> &'static str {
    match k {
        SnapKind::Saved => tr("Gespeichert"),
        SnapKind::Autosave => tr("Autosave"),
        SnapKind::File => tr("Datei (zuletzt gespeichert)"),
    }
}

impl App {
    /// Nach erfolgreichem Speichern: Stand in den Verlauf legen
    pub(crate) fn record_history(&mut self, path: &Path) {
        if self.settings.history_keep == 0 {
            return;
        }
        let _ = record(path, &self.ed.map.to_vmf(), self.settings.history_keep as usize);
        if self.timeline.open {
            self.timeline_refresh();
        }
    }

    pub(crate) fn timeline_refresh(&mut self) {
        let snaps = list(self.ed.path.as_deref(), &self.autosave_dir(), &self.ed.map_name());
        let t = &mut self.timeline;
        t.snaps = snaps;
        if t.sel.is_none_or(|i| i >= t.snaps.len()) {
            t.sel = t.snaps.len().checked_sub(1);
        }
    }

    pub(crate) fn open_timeline(&mut self) {
        self.timeline.open = true;
        self.timeline.cache.clear();
        self.timeline.sel = None;
        self.timeline_refresh();
    }

    pub(crate) fn timeline_window(&mut self, ctx: &egui::Context) {
        if !self.timeline.open {
            return;
        }
        // Unterschiede neu rechnen, wenn sich die Map geändert hat
        if self.timeline.cache_rev != self.ed.rev {
            self.timeline.cache.clear();
            self.timeline.cache_rev = self.ed.rev;
        }
        if let Some(i) = self.timeline.sel {
            if let Some(s) = self.timeline.snaps.get(i) {
                if !self.timeline.cache.contains_key(&s.path) {
                    let r = analyze(&s.path, &self.ed);
                    self.timeline.cache.insert(s.path.clone(), r);
                }
            }
        }
        let th = crate::theme::t();
        let mut open = true;
        let (mut compare, mut restore, mut tab) = (false, false, false);
        let tl = &mut self.timeline;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(tr("Zeitleiste der Speicherstände"), 820.0).height(600.0).no_scroll(), |ui| {
            if tl.snaps.is_empty() {
                ui.label(RichText::new(tr("Noch keine Speicherstände. Beim Speichern legt Amboss jedes Mal eine Kopie im Verlauf ab; dazu kommen die Autosaves.")).color(th.text3));
                return;
            }
            // Zeitleiste: Punkte nach Zeit, Klick wählt
            let (t0, t1) = (tl.snaps.first().unwrap().time, tl.snaps.last().unwrap().time);
            let span = t1.duration_since(t0).map(|d| d.as_secs_f64()).unwrap_or(0.0).max(1.0);
            let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::click());
            let p = ui.painter_at(r);
            let x0 = r.left() + 14.0;
            let w = r.width() - 28.0;
            p.line_segment([egui::pos2(x0, r.center().y), egui::pos2(x0 + w, r.center().y)], Stroke::new(2.0f32, th.line_strong));
            let xs: Vec<f32> = tl.snaps.iter().map(|s| x0 + (s.time.duration_since(t0).map(|d| d.as_secs_f64()).unwrap_or(0.0) / span) as f32 * w).collect();
            for (i, s) in tl.snaps.iter().enumerate() {
                let c = egui::pos2(xs[i], r.center().y);
                let sel = tl.sel == Some(i);
                p.circle_filled(c, if sel { 8.0 } else { 5.5 }, kind_color(s.kind));
                if sel {
                    p.circle_stroke(c, 10.5, Stroke::new(2.0f32, th.accent));
                }
            }
            p.text(egui::pos2(x0, r.bottom()), egui::Align2::LEFT_BOTTOM, when(t0), egui::FontId::proportional(10.5), th.text3);
            p.text(egui::pos2(x0 + w, r.bottom()), egui::Align2::RIGHT_BOTTOM, when(t1), egui::FontId::proportional(10.5), th.text3);
            if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    tl.sel = xs.iter().enumerate().min_by(|a, b| (a.1 - pos.x).abs().total_cmp(&(b.1 - pos.x).abs())).map(|(i, _)| i);
                }
            }
            ui.add_space(4.0);
            ui.horizontal_top(|ui| {
                // links: Liste (neueste oben)
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    egui::ScrollArea::vertical().id_salt("tl_list").max_height(440.0).show(ui, |ui| {
                        for i in (0..tl.snaps.len()).rev() {
                            let s = &tl.snaps[i];
                            let text = format!("{}  ·  {}  ·  {} KB", when(s.time), kind_label(s.kind), s.size / 1024);
                            let sel = tl.sel == Some(i);
                            let r = ui.selectable_label(sel, RichText::new(text).color(if sel { th.accent_fg } else { kind_color(s.kind) }));
                            if r.clicked() {
                                tl.sel = Some(i);
                            }
                        }
                    });
                });
                // rechts: Draufsicht und Unterschied
                ui.vertical(|ui| {
                    let Some(s) = tl.sel.and_then(|i| tl.snaps.get(i)) else { return };
                    ui.label(RichText::new(s.path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()).font(crate::theme::mono(11.0)).color(th.text3));
                    match tl.cache.get(&s.path) {
                        Some(Ok(info)) => {
                            ui.label(RichText::new(format!("{} · {}", crate::i18n::count(info.brushes, "Brush", "Brushes"), crate::i18n::count(info.entities, "Entity", "Entities"))).color(th.text));
                            let d = &info.diff;
                            let (a, rm, m) = (d.count(crate::diff::Change::Added), d.count(crate::diff::Change::Removed), d.count(crate::diff::Change::Modified));
                            ui.label(RichText::new(if a + rm + m == 0 && d.world.is_empty() { tr("Gleich wie jetzt").to_string() } else { crate::i18n::tr_dyn(&format!("Seitdem: {a} neu, {rm} gelöscht, {m} geändert")) }).color(th.text2));
                            draw_top(ui, info, &th);
                            ui.label(RichText::new(tr("Grau: Stand damals · Grün: seitdem neu · Rot: seitdem gelöscht · Gelb: seitdem geändert")).size(10.5).color(th.text3));
                            ui.horizontal_wrapped(|ui| {
                                if ui.button(tr("Unterschiede in der Map zeigen")).clicked() {
                                    compare = true;
                                }
                                if ui.button(tr("In neuem Tab öffnen")).clicked() {
                                    tab = true;
                                }
                                if crate::theme::primary_button(ui, tr("Wiederherstellen")).on_hover_text(tr("Diesen Stand in die offene Map laden (Strg+Z nimmt es zurück)")).clicked() {
                                    restore = true;
                                }
                            });
                        }
                        Some(Err(e)) => {
                            ui.label(RichText::new(crate::i18n::tr_dyn(e)).color(th.err));
                        }
                        None => {
                            ui.label(tr("lädt…"));
                        }
                    }
                });
            });
        });
        let sel = self.timeline.sel.and_then(|i| self.timeline.snaps.get(i)).cloned();
        if let Some(s) = sel {
            if compare {
                if let Some(Ok(info)) = self.timeline.cache.get(&s.path) {
                    let d = info.diff.clone();
                    self.ed.diff_boxes = d.objects.iter().filter(|o| !o.bounds.is_empty()).map(|o| (o.bounds, o.change.color())).collect();
                    self.ed.touch();
                    self.timeline.cache_rev = self.ed.rev;
                    self.versions_dlg = Some(crate::merge_ui::VersionsDialog { title: crate::i18n::tr_dyn(&format!("Vergleich mit dem Stand {}", when(s.time))), diff: d, show: [true; 3], merge: None });
                }
            }
            if tab {
                self.new_tab();
                self.open_path(s.path.clone());
                // nicht die Verlaufsdatei überschreiben: als unbenannt behandeln
                self.ed.path = None;
                self.ed.dirty = true;
            }
            if restore {
                if let Some(Ok(info)) = self.timeline.cache.remove(&s.path) {
                    self.ed.checkpoint();
                    self.ed.map = info.map;
                    self.ed.sel.clear();
                    self.ed.faces.clear();
                    self.ed.vsel.clear();
                    self.ed.dirty = true;
                    self.ed.status = crate::i18n::tr_dyn(&format!("Stand {} wiederhergestellt – Strg+Z nimmt es zurück", when(s.time)));
                    self.ed.touch();
                }
            }
        }
        self.timeline.open = open;
    }
}

/// Draufsicht des Stands mit farbigen Unterschieden
fn draw_top(ui: &mut egui::Ui, info: &SnapInfo, th: &crate::theme::Theme) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(480.0) - 4.0, 320.0), egui::Sense::hover());
    let p = ui.painter_at(r);
    p.rect_filled(r, 6.0, th.vp);
    let mut b = info.bounds;
    for o in &info.diff.objects {
        if !o.bounds.is_empty() {
            b = b.union(&o.bounds);
        }
    }
    if b.is_empty() {
        return;
    }
    let size = b.size();
    let k = ((r.width() - 20.0) as f64 / size.x.max(1.0)).min((r.height() - 20.0) as f64 / size.y.max(1.0)) as f32;
    let c = r.center();
    let mid = b.center();
    let sp = |x: f32, y: f32| egui::pos2(c.x + (x - mid.x as f32) * k, c.y - (y - mid.y as f32) * k);
    for [a, bb] in info.lines.iter().take(60000) {
        p.line_segment([sp(a[0], a[1]), sp(bb[0], bb[1])], Stroke::new(1.0f32, th.text3.gamma_multiply(0.6)));
    }
    for o in &info.diff.objects {
        if o.bounds.is_empty() {
            continue;
        }
        let col = o.change.color();
        let rr = egui::Rect::from_two_pos(sp(o.bounds.min.x as f32, o.bounds.min.y as f32), sp(o.bounds.max.x as f32, o.bounds.max.y as f32)).expand(1.0);
        p.rect_stroke(rr, 1.0, Stroke::new(1.5f32, Color32::from_rgb(col[0], col[1], col[2])), egui::StrokeKind::Outside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_lists_and_prunes() {
        let d = std::env::temp_dir().join(format!("amboss_tl_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mp = d.join("lager.vmf");
        let mut ed = crate::editor::Editor::new(crate::fgd::Fgd::builtin());
        ed.create_block(Aabb::new(glam::DVec3::ZERO, glam::DVec3::splat(64.0)));
        let v1 = ed.map.to_vmf();
        std::fs::write(&mp, &v1).unwrap();
        for _ in 0..4 {
            record(&mp, &v1, 3).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // nur die drei neuesten bleiben
        assert_eq!(std::fs::read_dir(history_dir(&mp)).unwrap().count(), 3);
        // Autosave dazu
        let asd = d.join("autosave");
        std::fs::write(asd.join("lager_autosave1.vmf"), &v1).unwrap();
        std::fs::write(asd.join("anders_autosave1.vmf"), &v1).unwrap();
        let l = list(Some(&mp), &asd, "lager");
        assert_eq!(l.iter().filter(|s| s.kind == SnapKind::Saved).count(), 3);
        assert_eq!(l.iter().filter(|s| s.kind == SnapKind::Autosave).count(), 1, "nur Autosaves dieser Map");
        assert_eq!(l.iter().filter(|s| s.kind == SnapKind::File).count(), 1);
        assert!(l.windows(2).all(|w| w[0].time <= w[1].time));
        // Unterschied: im jetzigen Stand ein Brush mehr
        ed.create_block(Aabb::new(glam::DVec3::splat(100.0), glam::DVec3::splat(164.0)));
        let info = analyze(&l[0].path, &ed).unwrap();
        assert_eq!(info.brushes, 1);
        assert_eq!(info.diff.count(crate::diff::Change::Added), 1);
        assert!(!info.lines.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
