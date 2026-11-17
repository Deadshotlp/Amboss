//! Material-Editor: VMT-Dateien anlegen und bearbeiten, Bilder (PNG) als VTF importieren,
//! Vorschau der Textur. Gespeichert wird lose im Spielordner (materials/…), damit Amboss die Datei
//! sofort findet und „Eigene Inhalte packen“ sie mit in die BSP nimmt.

use crate::keyvalues::{self, KvBlock, KvItem};
use crate::vtf_write::{self, Rgba, VtfOpts};
use eframe::egui::{self, RichText, Vec2};
use std::collections::BTreeMap;

/// Inhalt einer VMT: Shader, Parameter in Reihenfolge, verschachtelte Blöcke (Proxies …) unverändert
#[derive(Clone, Debug, PartialEq)]
pub struct MatDoc {
    pub shader: String,
    pub params: Vec<(String, String)>,
    pub blocks: Vec<KvBlock>,
}

impl Default for MatDoc {
    fn default() -> Self {
        MatDoc { shader: "LightmappedGeneric".into(), params: vec![("$basetexture".into(), String::new()), ("$surfaceprop".into(), "concrete".into())], blocks: Vec::new() }
    }
}

impl MatDoc {
    pub fn parse(src: &str) -> Option<MatDoc> {
        let root = keyvalues::parse(src).ok()?.into_iter().next()?;
        let mut d = MatDoc { shader: root.name.clone(), params: Vec::new(), blocks: Vec::new() };
        for it in &root.items {
            match it {
                KvItem::Pair(k, v) => d.params.push((k.clone(), v.clone())),
                KvItem::Block(b) => d.blocks.push(b.clone()),
            }
        }
        Some(d)
    }

    pub fn to_vmt(&self) -> String {
        let mut root = KvBlock::new(self.shader.clone());
        for (k, v) in &self.params {
            if !k.trim().is_empty() && !v.trim().is_empty() {
                root.push(k.trim(), v.trim());
            }
        }
        for b in &self.blocks {
            root.push_block(b.clone());
        }
        keyvalues::write(&[root])
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(a, _)| a.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }

    /// Setzen; leerer Wert entfernt den Parameter
    pub fn set(&mut self, k: &str, v: &str) {
        match self.params.iter_mut().find(|(a, _)| a.eq_ignore_ascii_case(k)) {
            Some(p) if !v.is_empty() => p.1 = v.to_string(),
            Some(_) => self.params.retain(|(a, _)| !a.eq_ignore_ascii_case(k)),
            None if !v.is_empty() => self.params.push((k.to_string(), v.to_string())),
            None => {}
        }
    }

    pub fn flag(&self, k: &str) -> bool {
        self.get(k).and_then(|v| v.trim().parse::<f32>().ok()).is_some_and(|x| x > 0.0)
    }

    pub fn set_flag(&mut self, k: &str, on: bool) {
        self.set(k, if on { "1" } else { "" });
    }
}

/// Shader mit kurzer Erklärung
pub const SHADERS: [(&str, &str); 6] = [
    ("LightmappedGeneric", "Wände, Böden, alles auf Brushes"),
    ("VertexLitGeneric", "Modelle (Props)"),
    ("UnlitGeneric", "Ohne Licht (Leuchtschilder, Skybox-Teile)"),
    ("WorldVertexTransition", "Überblendung zweier Texturen auf Displacements"),
    ("UnlitTwoTexture", "Zwei Texturen ohne Licht"),
    ("Refract", "Glas mit Lichtbrechung"),
];

pub const SURFACEPROPS: [&str; 34] = [
    "default", "concrete", "brick", "plaster", "tile", "rock", "gravel", "dirt", "mud", "grass", "sand", "snow", "ice", "wood", "wood_crate", "wood_plank", "metal", "solidmetal", "metal_box", "metalgrate", "metalvent", "chainlink", "glass", "computer", "plastic", "rubber", "carpet", "paper", "cardboard", "porcelain", "water", "slime", "flesh", "ladder",
];

/// Zustand des Editor-Fensters
#[derive(Default)]
pub struct MatEd {
    pub open: bool,
    /// Materialname relativ zu materials/, ohne .vmt
    pub name: String,
    pub doc: MatDoc,
    /// importierte, noch nicht gespeicherte Bilder je Parameter ($basetexture, $bumpmap …)
    pub imports: BTreeMap<String, Rgba>,
    pub status: String,
    preview: Option<(String, egui::TextureHandle)>,
    pub tile: bool,
}

impl MatEd {
    /// Leeres neues Material
    pub fn new_material(&mut self, name: &str) {
        self.name = name.to_string();
        self.doc = MatDoc::default();
        self.doc.set("$basetexture", name);
        self.imports.clear();
        self.preview = None;
        self.status.clear();
        self.open = true;
    }
}

/// Vorschlag für den Dateinamen einer importierten Textur
fn tex_name_for(mat: &str, param: &str) -> String {
    match param.to_ascii_lowercase().as_str() {
        "$basetexture" => mat.to_string(),
        "$bumpmap" | "$normalmap" => format!("{mat}_normal"),
        "$basetexture2" => format!("{mat}_b"),
        "$bumpmap2" => format!("{mat}_b_normal"),
        "$detail" => format!("{mat}_detail"),
        other => format!("{mat}_{}", other.trim_start_matches('$')),
    }
}

impl crate::app::App {
    /// Material-Editor öffnen: vorhandenes Material laden (aus Spielinhalten) oder neues anlegen
    pub(crate) fn open_material_editor(&mut self, name: Option<&str>) {
        let st = &mut self.mated;
        st.imports.clear();
        st.preview = None;
        st.status.clear();
        st.open = true;
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            st.new_material("amboss/neues_material");
            return;
        };
        let name = crate::materials::normalize(name);
        st.name = name.clone();
        // Spielinhalte evtl. noch nicht geladen → lose Datei im Spielordner direkt lesen
        let src = self.mats.fs().and_then(|fs| fs.read(&format!("materials/{name}.vmt"))).or_else(|| std::fs::read(std::path::Path::new(&self.settings.game_dir).join(format!("materials/{name}.vmt"))).ok());
        match src.and_then(|b| MatDoc::parse(&String::from_utf8_lossy(&b))) {
            Some(d) => st.doc = d,
            None => {
                st.doc = MatDoc::default();
                st.doc.set("$basetexture", &name);
                st.status = crate::i18n::tr("Keine VMT gefunden – neues Material.").into();
            }
        }
    }

    /// Speichern: VTFs der importierten Bilder und die VMT in den Spielordner schreiben
    fn save_material(&mut self) -> Result<String, String> {
        let st = &mut self.mated;
        let name = st.name.trim().trim_matches('/').replace('\\', "/").to_ascii_lowercase();
        if name.is_empty() || name.contains("..") {
            return Err(crate::i18n::tr("Bitte einen Materialnamen eingeben, z. B. amboss/wand_rot").into());
        }
        if self.settings.game_dir.is_empty() {
            return Err(crate::i18n::tr("Kein Spielordner eingestellt (Einstellungen → Spiel & Pfade).").into());
        }
        let root = std::path::Path::new(&self.settings.game_dir).join("materials");
        let mut written = Vec::new();
        for (param, img) in std::mem::take(&mut st.imports) {
            let tex = st.doc.get(&param).map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| tex_name_for(&name, &param));
            let normal = param.to_ascii_lowercase().contains("bump") || param.to_ascii_lowercase().contains("normal");
            let data = vtf_write::encode(&img, VtfOpts { normal, ..Default::default() });
            let path = root.join(format!("{tex}.vtf"));
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
            st.doc.set(&param, &tex);
            written.push(format!("{tex}.vtf"));
        }
        let vmt = root.join(format!("{name}.vmt"));
        if let Some(dir) = vmt.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&vmt, st.doc.to_vmt()).map_err(|e| format!("{}: {e}", vmt.display()))?;
        st.name = name.clone();
        st.preview = None;
        // Material und seine Texturen neu laden, in der Liste anbieten
        self.mats.forget(&name);
        written.insert(0, format!("{name}.vmt"));
        Ok(crate::i18n::tr_dyn(&format!("Gespeichert: {}", written.join(", "))))
    }

    /// Vorschaubild der Basistextur (importiertes Bild oder VTF aus den Spielinhalten)
    fn material_preview(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let st = &mut self.mated;
        let key = if st.imports.contains_key("$basetexture") { "import:$basetexture".to_string() } else { st.doc.get("$basetexture").unwrap_or("").to_ascii_lowercase() };
        if let Some((k, t)) = &st.preview {
            if *k == key {
                return Some(t.clone());
            }
        }
        let img: Option<(u32, u32, Vec<u8>)> = if let Some(i) = st.imports.get("$basetexture") {
            let small = vtf_write::resize(i, i.w.min(256), i.h.min(256));
            Some((small.w, small.h, small.px))
        } else if !key.is_empty() {
            self.mats
                .fs()
                .and_then(|fs| fs.read(&format!("materials/{key}.vtf")))
                .or_else(|| std::fs::read(std::path::Path::new(&self.settings.game_dir).join(format!("materials/{key}.vtf"))).ok())
                .and_then(|b| crate::vtf::load(&b, 256).ok())
                .map(|i| (i.pw, i.ph, i.rgba))
        } else {
            None
        };
        let (w, h, px) = img?;
        let tex = ctx.load_texture(format!("mated:{key}"), egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px), egui::TextureOptions::LINEAR);
        st.preview = Some((key, tex.clone()));
        Some(tex)
    }

    pub(crate) fn material_editor_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.mated.open {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let preview = self.material_preview(ctx);
        let (mut save, mut import, mut apply) = (false, None::<String>, false);
        let d = theme::Dlg::new(crate::i18n::tr("Material-Editor"), 880.0).subtitle(format!("materials/{}.vmt", self.mated.name)).height(640.0).footer();
        let st = &mut self.mated;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.horizontal_top(|ui| {
                    // links: Vorschau
                    ui.vertical(|ui| {
                        ui.set_width(276.0);
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(272.0), egui::Sense::hover());
                        // Schachbrett, damit Transparenz sichtbar wird
                        let cells = 8;
                        let cs = r.width() / cells as f32;
                        for y in 0..cells {
                            for x in 0..cells {
                                let c = if (x + y) % 2 == 0 { egui::Color32::from_gray(70) } else { egui::Color32::from_gray(50) };
                                ui.painter().rect_filled(egui::Rect::from_min_size(r.min + Vec2::new(x as f32 * cs, y as f32 * cs), Vec2::splat(cs)), 0.0, c);
                            }
                        }
                        match &preview {
                            Some(t) => {
                                let uv = if st.tile { egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(2.0, 2.0)) } else { egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)) };
                                let tint = st.doc.get("$color").and_then(crate::materials::parse_color).map(|c| egui::Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(egui::Color32::WHITE);
                                let mut mesh = egui::Mesh::with_texture(t.id());
                                mesh.add_rect_with_uv(r, uv, tint);
                                ui.painter().add(mesh);
                            }
                            None => {
                                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Keine Basistextur"), theme::sans(12.5), th.text3);
                            }
                        }
                        theme::checkbox(ui, &mut st.tile, crate::i18n::tr("Gekachelt (2 × 2)"));
                        if let Some(t) = &preview {
                            ui.label(RichText::new(format!("{} × {} px", t.size()[0], t.size()[1])).size(12.0).color(th.text3));
                        }
                    });
                    ui.separator();
                    // rechts: Einstellungen
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical().id_salt("mated_scroll").auto_shrink([false, false]).show(ui, |ui| {
                            let lw = 150.0;
                            egui::Grid::new("mated").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                                ui.label(crate::i18n::tr("Name"));
                                ui.add(egui::TextEdit::singleline(&mut st.name).hint_text(theme::hint("amboss/wand_rot")).desired_width(320.0)).on_hover_text(crate::i18n::tr("Pfad unter materials/, ohne .vmt"));
                                ui.end_row();
                                ui.label(crate::i18n::tr("Shader"));
                                egui::ComboBox::from_id_salt("mated_shader").selected_text(st.doc.shader.clone()).width(320.0).show_ui(ui, |ui| {
                                    for (s, desc) in SHADERS {
                                        if ui.selectable_label(st.doc.shader.eq_ignore_ascii_case(s), s).on_hover_text(crate::i18n::tr(desc)).clicked() {
                                            st.doc.shader = s.to_string();
                                        }
                                    }
                                });
                                ui.end_row();
                                let wvt = st.doc.shader.eq_ignore_ascii_case("WorldVertexTransition");
                                let unlit = st.doc.shader.to_ascii_lowercase().starts_with("unlit");
                                let mut tex_row = |ui: &mut egui::Ui, label: &'static str, param: &str, tip: &'static str| {
                                    let _ = lw;
                                    ui.label(crate::i18n::tr(label)).on_hover_text(format!("{param}\n{}", crate::i18n::tr(tip)));
                                    ui.horizontal(|ui| {
                                        let mut v = st.doc.get(param).unwrap_or("").to_string();
                                        let pending = st.imports.contains_key(param);
                                        if ui.add(egui::TextEdit::singleline(&mut v).desired_width(220.0).hint_text(theme::hint(crate::i18n::tr("Pfad unter materials/")))).changed() {
                                            st.doc.set(param, &v);
                                        }
                                        if ui.button(crate::i18n::tr("Bild…")).on_hover_text(crate::i18n::tr("PNG importieren (wird beim Speichern zu VTF)")).clicked() {
                                            import = Some(param.to_string());
                                        }
                                        if pending {
                                            ui.label(RichText::new("●").color(th.accent)).on_hover_text(crate::i18n::tr("Importiert – wird beim Speichern geschrieben"));
                                        }
                                    });
                                    ui.end_row();
                                };
                                tex_row(ui, "Basistextur", "$basetexture", "Farbbild der Oberfläche");
                                if !unlit {
                                    tex_row(ui, "Normalmap", "$bumpmap", "Oberflächenrelief für Licht");
                                }
                                if wvt {
                                    tex_row(ui, "Zweite Textur", "$basetexture2", "Wird über die Alpha-Werte des Displacements eingeblendet");
                                    tex_row(ui, "Zweite Normalmap", "$bumpmap2", "Relief der zweiten Textur");
                                }
                                tex_row(ui, "Detailtextur", "$detail", "Feine Struktur, die über die Basistextur gelegt wird");
                                if st.doc.get("$detail").is_some_and(|v| !v.is_empty()) {
                                    ui.label(crate::i18n::tr("Detail-Skalierung"));
                                    let mut v = st.doc.get("$detailscale").unwrap_or("4").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut v).desired_width(80.0)).changed() {
                                        st.doc.set("$detailscale", &v);
                                    }
                                    ui.end_row();
                                }
                                ui.label(crate::i18n::tr("Oberflächenart"));
                                let cur = st.doc.get("$surfaceprop").unwrap_or("").to_string();
                                egui::ComboBox::from_id_salt("mated_sp").selected_text(if cur.is_empty() { crate::i18n::tr("(keine)").to_string() } else { cur.clone() }).width(220.0).show_ui(ui, |ui| {
                                    for sp in SURFACEPROPS {
                                        if ui.selectable_label(cur.eq_ignore_ascii_case(sp), sp).clicked() {
                                            st.doc.set("$surfaceprop", sp);
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(crate::i18n::tr("$surfaceprop: Schritt-Geräusche, Einschusslöcher, Physik"));
                                ui.end_row();
                                ui.label(crate::i18n::tr("Farbe"));
                                ui.horizontal(|ui| {
                                    let c = st.doc.get("$color").and_then(crate::materials::parse_color).unwrap_or([255, 255, 255]);
                                    let mut col = egui::Color32::from_rgb(c[0], c[1], c[2]);
                                    if ui.color_edit_button_srgba(&mut col).changed() {
                                        st.doc.set("$color", &format!("[{:.3} {:.3} {:.3}]", col.r() as f32 / 255.0, col.g() as f32 / 255.0, col.b() as f32 / 255.0));
                                    }
                                    if st.doc.get("$color").is_some() && ui.small_button(crate::i18n::tr("Zurücksetzen")).clicked() {
                                        st.doc.set("$color", "");
                                    }
                                });
                                ui.end_row();
                            });
                            theme::section(ui, crate::i18n::tr("Eigenschaften"));
                            egui::Grid::new("mated_flags").num_columns(3).spacing([16.0, 6.0]).show(ui, |ui| {
                                let mut n = 0;
                                for (k, label, tip) in [
                                    ("$translucent", "Durchscheinend", "Alpha-Kanal als Transparenz (weiche Kanten, sortiert)"),
                                    ("$alphatest", "Alpha-Test", "Harte Kanten (Zäune, Blätter) – schneller als durchscheinend"),
                                    ("$nocull", "Beidseitig", "Auch von hinten sichtbar"),
                                    ("$selfillum", "Selbstleuchtend", "Leuchtet unabhängig vom Licht (Maske im Alpha-Kanal)"),
                                    ("$additive", "Additiv", "Hellt den Hintergrund auf (Licht, Feuer)"),
                                ] {
                                    let mut on = st.doc.flag(k);
                                    if theme::checkbox(ui, &mut on, crate::i18n::tr(label)).on_hover_text(format!("{k}\n{}", crate::i18n::tr(tip))).changed() {
                                        st.doc.set_flag(k, on);
                                    }
                                    n += 1;
                                    if n % 3 == 0 {
                                        ui.end_row();
                                    }
                                }
                                let mut env = st.doc.get("$envmap").is_some();
                                if theme::checkbox(ui, &mut env, crate::i18n::tr("Spiegelung")).on_hover_text(crate::i18n::tr("$envmap env_cubemap – spiegelt die nächste env_cubemap")).changed() {
                                    st.doc.set("$envmap", if env { "env_cubemap" } else { "" });
                                    if env && st.doc.get("$envmaptint").is_none() {
                                        st.doc.set("$envmaptint", "[.3 .3 .3]");
                                    }
                                    if !env {
                                        st.doc.set("$envmaptint", "");
                                    }
                                }
                            });
                            theme::section(ui, crate::i18n::tr("Alle Parameter"));
                            let mut remove = None;
                            egui::Grid::new("mated_raw").num_columns(3).striped(true).show(ui, |ui| {
                                for (i, (k, v)) in st.doc.params.iter_mut().enumerate() {
                                    ui.add_sized([180.0, 22.0], egui::TextEdit::singleline(k).font(theme::mono(12.0)));
                                    ui.add_sized([250.0, 22.0], egui::TextEdit::singleline(v).font(theme::mono(12.0)));
                                    if ui.small_button("×").on_hover_text(crate::i18n::tr("Parameter entfernen")).clicked() {
                                        remove = Some(i);
                                    }
                                    ui.end_row();
                                }
                            });
                            if let Some(i) = remove {
                                st.doc.params.remove(i);
                            }
                            if ui.button(crate::i18n::tr("+ Parameter")).clicked() {
                                st.doc.params.push(("$".into(), String::new()));
                            }
                            if !st.doc.blocks.is_empty() {
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Unterblöcke (z. B. Proxies) bleiben unverändert erhalten.", st.doc.blocks.len()))).size(12.0).color(th.text3));
                            }
                        });
                    });
                });
            }
            theme::Part::Footer => {
                if theme::primary_button(ui, crate::i18n::tr("Speichern")).clicked() {
                    save = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Als aktuelles Material")).on_hover_text(crate::i18n::tr("Speichern und als aktuelle Textur für neue Brushes setzen")).clicked() {
                    save = true;
                    apply = true;
                }
                theme::footer_hint(ui, if st.status.is_empty() { crate::i18n::tr("Wird lose im Spielordner gespeichert und beim Kompilieren mit eingepackt") } else { &st.status });
            }
            _ => {}
        });
        self.mated.open = open;
        if let Some(param) = import {
            if let Some(p) = rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_file() {
                match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| vtf_write::load_png(&b)) {
                    Ok(img) => {
                        let st = &mut self.mated;
                        if st.doc.get(&param).is_none_or(|v| v.is_empty()) {
                            let n = tex_name_for(&st.name, &param);
                            st.doc.set(&param, &n);
                        }
                        st.status = crate::i18n::tr_dyn(&format!("Bild geladen: {} × {} px", img.w, img.h));
                        st.imports.insert(param, img);
                        st.preview = None;
                    }
                    Err(e) => self.mated.status = crate::i18n::tr_dyn(&format!("Bild konnte nicht geladen werden: {e}")),
                }
            }
        }
        if save {
            match self.save_material() {
                Ok(msg) => {
                    self.mated.status = msg.clone();
                    self.ed.status = msg;
                    if apply {
                        self.ed.material = self.mated.name.to_ascii_uppercase();
                    }
                }
                Err(e) => self.mated.status = e,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmt_roundtrip() {
        let src = "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"amboss/wand\"\n\t\"$surfaceprop\" \"brick\"\n\t\"Proxies\"\n\t{\n\t\t\"TextureScroll\"\n\t\t{\n\t\t\t\"texturescrollrate\" \"1\"\n\t\t}\n\t}\n}\n";
        let mut d = MatDoc::parse(src).unwrap();
        assert_eq!(d.shader, "LightmappedGeneric");
        assert_eq!(d.get("$surfaceprop"), Some("brick"));
        assert_eq!(d.blocks.len(), 1);
        d.set_flag("$translucent", true);
        d.set("$surfaceprop", "");
        let out = d.to_vmt();
        let back = MatDoc::parse(&out).unwrap();
        assert!(back.flag("$translucent"));
        assert_eq!(back.get("$surfaceprop"), None);
        assert_eq!(back.blocks[0].blocks().next().map(|b| b.name.as_str()), Some("TextureScroll"));
        assert_eq!(tex_name_for("amboss/wand", "$bumpmap"), "amboss/wand_normal");
    }
}
