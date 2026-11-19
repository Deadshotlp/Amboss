//! Decal-/Overlay-Generator: aus einem Bild (PNG mit Transparenz oder JPEG) ein fertiges
//! Decal-Material (VTF + VMT mit `$decal` und `$decalscale`) machen und gleich platzieren.

use crate::app::App;
use crate::editor::Tool;
use crate::i18n::tr;
use crate::vtf_write::{Rgba, VtfOpts};
use eframe::egui::{self, RichText};
use std::path::PathBuf;

/// Ordner unterhalb von materials/
pub const FOLDER: &str = "amboss/decals";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// infodecal: wird beim Laden der Map auf die Fläche gedruckt
    Decal,
    /// info_overlay: frei skalier- und drehbar, auch über mehrere Flächen
    Overlay,
}

pub struct DecalGen {
    pub open: bool,
    pub path: Option<PathBuf>,
    pub img: Option<Rgba>,
    pub name: String,
    pub kind: Kind,
    /// Breite in der Welt (Einheiten)
    pub width: f64,
    pub trim: bool,
    pub max_size: u32,
    pub preview: Option<egui::TextureHandle>,
    pub status: String,
}

impl Default for DecalGen {
    fn default() -> Self {
        DecalGen { open: false, path: None, img: None, name: String::new(), kind: Kind::Decal, width: 64.0, trim: true, max_size: 512, preview: None, status: String::new() }
    }
}

/// Durchsichtige Ränder abschneiden (Alpha < 8); ganz durchsichtige Bilder bleiben, wie sie sind
pub fn trim_alpha(img: &Rgba) -> Rgba {
    let (w, h) = (img.w as usize, img.h as usize);
    let a = |x: usize, y: usize| img.px[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if a(x, y) >= 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || (x0 == 0 && y0 == 0 && x1 == w - 1 && y1 == h - 1) {
        return img.clone();
    }
    // 1 Pixel Rand lassen, damit die Kanten beim Filtern nicht abgeschnitten wirken
    let (x0, y0) = (x0.saturating_sub(1), y0.saturating_sub(1));
    let (x1, y1) = ((x1 + 1).min(w - 1), (y1 + 1).min(h - 1));
    let (nw, nh) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut px = Vec::with_capacity(nw * nh * 4);
    for y in y0..=y1 {
        px.extend_from_slice(&img.px[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
    }
    Rgba { w: nw as u32, h: nh as u32, px }
}

/// Dateiname → Materialname (klein, nur a–z 0–9 _ -)
pub fn clean_name(s: &str) -> String {
    let n: String = s.trim().to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let n = n.trim_matches('_').to_string();
    if n.is_empty() {
        "decal".into()
    } else {
        n
    }
}

/// VMT für Decal und Overlay (LightmappedGeneric mit $decal; Transparenz nur bei Alpha)
pub fn vmt(material: &str, translucent: bool, decal_scale: f64) -> String {
    let mut s = format!("\"LightmappedGeneric\"\n{{\n\t\"$basetexture\" \"{material}\"\n\t\"$decal\" \"1\"\n\t\"$decalscale\" \"{:.4}\"\n", decal_scale);
    if translucent {
        s.push_str("\t\"$translucent\" \"1\"\n");
    }
    s.push_str("\t\"$surfaceprop\" \"default\"\n}\n");
    s
}

/// Ergebnis: (Materialname, Texturgröße, $decalscale, Höhe in Einheiten)
pub struct Built {
    pub material: String,
    pub tex: (u32, u32),
    pub decal_scale: f64,
    pub height: f64,
    pub vtf: Vec<u8>,
    pub vmt: String,
}

/// Bild → VTF + VMT (im Speicher)
pub fn build(img: &Rgba, name: &str, width_units: f64, trim: bool, max_size: u32) -> Built {
    let img = if trim && img.has_alpha() { trim_alpha(img) } else { img.clone() };
    let max = if max_size == 0 { 2048 } else { max_size };
    let tex = (crate::vtf_write::pow2(img.w.max(1), max), crate::vtf_write::pow2(img.h.max(1), max));
    let vtf = crate::vtf_write::encode(&img, VtfOpts { clamp: true, max_size: max, ..Default::default() });
    // Breite in der Welt = Texturbreite × $decalscale; Höhe folgt dem Seitenverhältnis des Bildes
    let decal_scale = width_units.max(1.0) / tex.0 as f64;
    let height = width_units.max(1.0) * img.h as f64 / img.w.max(1) as f64;
    // Das Bild wird auf Zweierpotenzen gestreckt: die Textur hat das Seitenverhältnis tex.0 : tex.1,
    // in der Welt soll es aber wie das Bild aussehen – Source kennt nur einen Faktor, daher
    // so wählen, dass die Breite stimmt (die Höhe weicht höchstens um die Rundung ab)
    let material = format!("{FOLDER}/{}", clean_name(name));
    let vmt = vmt(&material, img.has_alpha(), decal_scale);
    Built { material, tex, decal_scale, height, vtf, vmt }
}

impl App {
    pub(crate) fn open_decal_gen(&mut self) {
        self.dgen.open = true;
        if self.dgen.img.is_none() {
            self.decal_pick_file();
        }
    }

    fn decal_pick_file(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter(tr("Bilder (PNG, JPEG)"), &["png", "jpg", "jpeg"]).pick_file() else { return };
        self.decal_load(p);
    }

    pub(crate) fn decal_load(&mut self, p: PathBuf) {
        let g = &mut self.dgen;
        match crate::model_import::load_image(&crate::model_import::Image::File(p.clone())) {
            Ok(img) => {
                g.name = clean_name(&p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                // Vorschlag: ein Pixel = ¼ Einheit, zwischen 16 und 512 Einheiten
                g.width = ((img.w as f64) * 0.25).clamp(16.0, 512.0).round();
                g.img = Some(img);
                g.path = Some(p);
                g.preview = None;
                g.status.clear();
            }
            Err(e) => g.status = e,
        }
    }

    /// für AMBOSS_DEBUG_OPEN=decalgo
    pub(crate) fn decal_create_dbg(&mut self) -> Result<String, String> {
        self.decal_create()
    }

    fn decal_create(&mut self) -> Result<String, String> {
        let g = &self.dgen;
        let img = g.img.as_ref().ok_or_else(|| tr("Zuerst ein Bild wählen").to_string())?;
        if self.settings.game_dir.is_empty() {
            return Err(tr("Kein Spielordner eingestellt (Einstellungen → Spiel & Pfade).").into());
        }
        let b = build(img, &g.name, g.width, g.trim, g.max_size);
        let root = std::path::Path::new(&self.settings.game_dir).join("materials");
        let vtf = root.join(format!("{}.vtf", b.material));
        if let Some(d) = vtf.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&vtf, &b.vtf).map_err(|e| format!("{}: {e}", vtf.display()))?;
        let vmt = root.join(format!("{}.vmt", b.material));
        std::fs::write(&vmt, &b.vmt).map_err(|e| format!("{}: {e}", vmt.display()))?;
        self.mats.forget(&b.material);
        self.ed.material = b.material.clone();
        let tool = if g.kind == Kind::Decal { Tool::Decal } else { Tool::Overlay };
        let msg = crate::i18n::tr_dyn(&format!("Material {} gespeichert ({:.0} × {:.0} Einheiten, Textur {} × {}, $decalscale {:.3}) – jetzt in 3D auf eine Fläche klicken", b.material, g.width, b.height, b.tex.0, b.tex.1, b.decal_scale));
        self.set_tool(tool);
        Ok(msg)
    }

    pub(crate) fn decal_gen_window(&mut self, ctx: &egui::Context) {
        if !self.dgen.open {
            return;
        }
        let th = crate::theme::t();
        if self.dgen.preview.is_none() {
            if let Some(img) = &self.dgen.img {
                let small = if img.w.max(img.h) > 256 {
                    let k = 256.0 / img.w.max(img.h) as f64;
                    crate::vtf_write::resize(img, ((img.w as f64 * k) as u32).max(1), ((img.h as f64 * k) as u32).max(1))
                } else {
                    img.clone()
                };
                self.dgen.preview = Some(ctx.load_texture("decal_gen", egui::ColorImage::from_rgba_unmultiplied([small.w as usize, small.h as usize], &small.px), egui::TextureOptions::LINEAR));
            }
        }
        let mut open = true;
        let mut pick = false;
        let mut create = false;
        let d = crate::theme::Dlg::new(tr("Decal / Overlay aus Bild"), 460.0).footer();
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 10)).show(ui, |ui| {
                    let g = &mut self.dgen;
                    ui.horizontal(|ui| {
                        if ui.button(tr("Bild wählen…")).clicked() {
                            pick = true;
                        }
                        let f = g.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| tr("kein Bild").to_string());
                        ui.label(RichText::new(f).color(th.text2));
                    });
                    if let (Some(img), Some(tex)) = (&g.img, &g.preview) {
                        ui.add_space(6.0);
                        // Schachbrett hinter der Vorschau zeigt die Transparenz
                        let sz = tex.size_vec2();
                        let k = (200.0 / sz.x.max(sz.y)).min(1.0) * 1.0;
                        let (r, _) = ui.allocate_exact_size(sz * k.max(0.5), egui::Sense::hover());
                        let cell = 8.0;
                        let p = ui.painter_at(r);
                        let mut y = r.top();
                        let mut row = 0;
                        while y < r.bottom() {
                            let mut x = r.left() + if row % 2 == 0 { 0.0 } else { cell };
                            while x < r.right() {
                                p.rect_filled(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(cell, cell)), 0.0, egui::Color32::from_gray(70));
                                x += cell * 2.0;
                            }
                            y += cell;
                            row += 1;
                        }
                        p.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} × {} Pixel{}", img.w, img.h, if img.has_alpha() { tr(", mit Transparenz") } else { "" }))).small().color(th.text3));
                    }
                    ui.add_space(8.0);
                    egui::Grid::new("dgen").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label(tr("Name"));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{FOLDER}/")).font(crate::theme::mono(12.0)).color(th.text3));
                            ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(160.0));
                        });
                        ui.end_row();
                        ui.label(tr("Art"));
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut g.kind, Kind::Decal, tr("Decal")).on_hover_text(tr("infodecal: einfach, feste Größe, wird auf die Fläche gedruckt"));
                            ui.radio_value(&mut g.kind, Kind::Overlay, tr("Overlay")).on_hover_text(tr("info_overlay: frei skalier- und drehbar, auch über mehrere Flächen"));
                        });
                        ui.end_row();
                        ui.label(tr("Breite"));
                        ui.horizontal(|ui| {
                            ui.add(crate::theme::dv(egui::DragValue::new(&mut g.width).range(1.0..=4096.0).speed(1.0).suffix(tr(" Einheiten"))));
                            if let Some(img) = &g.img {
                                let h = g.width * img.h as f64 / img.w.max(1) as f64;
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Höhe {:.0} · {:.2} m", h, g.width * 0.0254))).color(th.text3));
                            }
                        });
                        ui.end_row();
                        ui.label(tr("Texturgröße"));
                        egui::ComboBox::from_id_salt("dgen_max").selected_text(crate::i18n::tr_dyn(&format!("bis {} px", g.max_size))).show_ui(ui, |ui| {
                            for m in [256u32, 512, 1024, 2048] {
                                ui.selectable_value(&mut g.max_size, m, crate::i18n::tr_dyn(&format!("bis {m} px")));
                            }
                        });
                        ui.end_row();
                    });
                    crate::theme::checkbox(ui, &mut g.trim, tr("Durchsichtige Ränder abschneiden"));
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Wird unter materials/ im Spielordner gespeichert; „Eigene Inhalte in die BSP packen“ nimmt es beim Kompilieren mit.")).small().color(th.text3));
                    if !g.status.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(&g.status).color(th.text2));
                    }
                });
            }
            crate::theme::Part::Footer => {
                if ui.add_enabled(self.dgen.img.is_some() && !self.dgen.name.trim().is_empty(), egui::Button::new(RichText::new(tr("Erstellen und platzieren")).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(egui::vec2(0.0, 32.0))).clicked() {
                    create = true;
                }
                if crate::theme::secondary_button(ui, tr("Schließen")).clicked() {
                    self.dgen.open = false;
                }
            }
            _ => {}
        });
        if pick {
            self.decal_pick_file();
        }
        if create {
            match self.decal_create() {
                Ok(m) => {
                    self.ed.status = m.clone();
                    self.dgen.status = m;
                    self.dgen.open = false;
                }
                Err(e) => self.dgen.status = e,
            }
        }
        if !open {
            self.dgen.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Rgba {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        Rgba { w, h, px }
    }

    #[test]
    fn trims_transparent_border() {
        // 100×50, sichtbar nur 20..40 × 10..30
        let i = img(100, 50, |x, y| if (20..40).contains(&x) && (10..30).contains(&y) { [255, 0, 0, 255] } else { [0, 0, 0, 0] });
        let t = trim_alpha(&i);
        assert_eq!((t.w, t.h), (22, 22));
        // deckendes Bild bleibt
        let o = img(10, 10, |_, _| [1, 2, 3, 255]);
        assert_eq!(trim_alpha(&o).w, 10);
    }

    #[test]
    fn builds_material() {
        let i = img(200, 100, |x, _| if x < 10 { [0, 0, 0, 0] } else { [200, 50, 50, 255] });
        let b = build(&i, "Mein Logo!", 128.0, false, 512);
        assert_eq!(b.material, "amboss/decals/mein_logo");
        assert_eq!(b.tex, (256, 128));
        assert!((b.decal_scale - 0.5).abs() < 1e-9);
        assert!((b.height - 64.0).abs() < 1e-9);
        assert!(b.vmt.contains("\"$decal\" \"1\"") && b.vmt.contains("\"$translucent\" \"1\"") && b.vmt.contains("\"$decalscale\" \"0.5000\""));
        assert!(b.vtf.starts_with(b"VTF\0"));
        // ohne Alpha kein $translucent
        let o = img(64, 64, |_, _| [9, 9, 9, 255]);
        assert!(!build(&o, "x", 32.0, true, 512).vmt.contains("$translucent"));
        assert_eq!(clean_name("  "), "decal");
    }
}
