//! Heightmap-Import: Graustufenbild (PNG oder RAW 16 Bit) → Gelände aus Displacement-Kacheln.

use crate::disp::Disp;
use crate::editor::{Editor, Obj};
use crate::geometry::Aabb;
use glam::DVec3;
use std::path::Path;

/// Höhenwerte 0..1, zeilenweise (y nach unten)
pub struct Heights {
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}

impl Heights {
    /// Bilinear abtasten, u/v in 0..1 (v = 0 oben im Bild = Norden/+Y)
    pub fn sample(&self, u: f64, v: f64) -> f64 {
        let x = (u.clamp(0.0, 1.0) * (self.w - 1) as f64).max(0.0);
        let y = (v.clamp(0.0, 1.0) * (self.h - 1) as f64).max(0.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let at = |x: usize, y: usize| self.v[y * self.w + x] as f64;
        let a = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let b = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        a * (1.0 - fy) + b * fy
    }
}

/// PNG (Graustufen oder Farbe, 8/16 Bit) oder RAW (16 Bit little endian, quadratisch) laden
pub fn load(path: &Path) -> Result<Heights, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if ext == "raw" || ext == "r16" {
        let n = bytes.len() / 2;
        let side = (n as f64).sqrt() as usize;
        if side < 2 || side * side != n {
            return Err("RAW-Datei muss quadratisch sein (16 Bit, z. B. 513 × 513)".into());
        }
        let v = bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as f32 / 65535.0).collect();
        return Ok(Heights { w: side, h: side, v });
    }
    let mut dec = png::Decoder::new(std::io::Cursor::new(&bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG zu groß")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = info.color_type.samples();
    let wide = info.bit_depth == png::BitDepth::Sixteen;
    let px = |i: usize, c: usize| -> f32 {
        if wide {
            let o = (i * ch + c) * 2;
            u16::from_be_bytes([buf[o], buf[o + 1]]) as f32 / 65535.0
        } else {
            buf[i * ch + c] as f32 / 255.0
        }
    };
    let v = (0..w * h)
        .map(|i| match ch {
            1 | 2 => px(i, 0),
            _ => 0.299 * px(i, 0) + 0.587 * px(i, 1) + 0.114 * px(i, 2),
        })
        .collect();
    Ok(Heights { w, h, v })
}

// ------------------------------------------------------------------ Rausch-Generator

/// Einstellungen des eingebauten Rausch-Generators (Perlin-Rauschen in mehreren Lagen)
#[derive(Clone, Debug, PartialEq)]
pub struct NoiseOpts {
    pub seed: u32,
    /// Größe der Hügel als Anteil der Geländebreite (0,05 = viele kleine, 1 = wenige große)
    pub scale: f64,
    /// Lagen feinerer Details
    pub octaves: usize,
    /// Rauheit: Anteil jeder feineren Lage (0,3 sanft … 0,7 zerklüftet)
    pub roughness: f64,
    /// Bergkämme statt runder Hügel
    pub ridged: bool,
    /// Ränder flach auslaufen lassen (Insel/Talkessel)
    pub island: bool,
    /// Talboden: alles unterhalb dieses Anteils wird flach
    pub flat: f64,
    /// Terrassen (0 = aus)
    pub terraces: usize,
}

impl Default for NoiseOpts {
    fn default() -> Self {
        NoiseOpts { seed: 1, scale: 0.35, octaves: 5, roughness: 0.5, ridged: false, island: false, flat: 0.0, terraces: 0 }
    }
}

/// Perlin-Rauschen 2D mit eigener Permutation (aus dem Startwert)
struct Perlin {
    p: [u8; 512],
}

impl Perlin {
    fn new(seed: u32) -> Perlin {
        let mut p = [0u8; 512];
        let mut v: Vec<u8> = (0..=255).collect();
        let mut s = seed as u64 ^ 0x2545_F491_4F6C_DD1D;
        for i in (1..256).rev() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            v.swap(i, (s % (i as u64 + 1)) as usize);
        }
        for i in 0..512 {
            p[i] = v[i & 255];
        }
        Perlin { p }
    }

    fn noise(&self, x: f64, y: f64) -> f64 {
        let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
        let grad = |h: u8, x: f64, y: f64| match h & 7 {
            0 => x + y,
            1 => -x + y,
            2 => x - y,
            3 => -x - y,
            4 => x,
            5 => -x,
            6 => y,
            _ => -y,
        };
        let (xi, yi) = (x.floor() as i64 & 255, y.floor() as i64 & 255);
        let (xf, yf) = (x - x.floor(), y - y.floor());
        let (u, v) = (fade(xf), fade(yf));
        let p = &self.p;
        let (xi, yi) = (xi as usize, yi as usize);
        let aa = p[p[xi] as usize + yi];
        let ab = p[p[xi] as usize + yi + 1];
        let ba = p[p[xi + 1] as usize + yi];
        let bb = p[p[xi + 1] as usize + yi + 1];
        let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
        l(l(grad(aa, xf, yf), grad(ba, xf - 1.0, yf), u), l(grad(ab, xf, yf - 1.0), grad(bb, xf - 1.0, yf - 1.0), u), v)
    }
}

/// Höhenfeld aus Rauschen (res × res Werte, auf 0..1 normiert)
pub fn noise_heights(o: &NoiseOpts, res: usize) -> Heights {
    let res = res.clamp(17, 2049);
    let pn = Perlin::new(o.seed);
    let freq0 = 1.0 / o.scale.clamp(0.02, 2.0);
    let mut v = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            let (x, y) = (i as f64 / (res - 1) as f64, j as f64 / (res - 1) as f64);
            let (mut sum, mut amp, mut f, mut norm) = (0.0, 1.0, freq0, 0.0);
            for k in 0..o.octaves.clamp(1, 10) {
                // jede Lage etwas versetzt, damit sich keine Muster wiederholen
                let n = pn.noise(x * f + k as f64 * 17.13, y * f + k as f64 * 31.7);
                sum += if o.ridged { (1.0 - n.abs() * 1.6).max(0.0).powi(2) } else { n } * amp;
                norm += amp;
                amp *= o.roughness.clamp(0.05, 0.95);
                f *= 2.0;
            }
            let mut h = sum / norm;
            if o.island {
                let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
                let fall = (1.0 - ((d - 0.55) / 0.45).clamp(0.0, 1.0)).powi(2);
                h = h * fall - (1.0 - fall) * 0.6;
            }
            v.push(h as f32);
        }
    }
    // auf 0..1 bringen, dann Talboden und Terrassen
    let (mn, mx) = v.iter().fold((f32::MAX, f32::MIN), |a, &x| (a.0.min(x), a.1.max(x)));
    let span = (mx - mn).max(1e-6);
    let flat = o.flat.clamp(0.0, 0.95) as f32;
    for x in &mut v {
        let mut t = (*x - mn) / span;
        if flat > 0.0 {
            t = ((t - flat) / (1.0 - flat)).max(0.0);
        }
        if o.terraces > 0 {
            let k = o.terraces as f32;
            let s = (t * k).floor();
            let fr = t * k - s;
            // weiche Stufenkanten
            t = (s + (fr * 3.0 - 2.0).clamp(0.0, 1.0)) / k;
        }
        *x = t.clamp(0.0, 1.0);
    }
    Heights { w: res, h: res, v }
}

/// Höhenfeld als 16-Bit-Graustufen-PNG speichern (z. B. um es weiterzubearbeiten)
pub fn save_png(h: &Heights, path: &Path) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), h.w as u32, h.h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Sixteen);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = h.v.iter().flat_map(|x| ((x.clamp(0.0, 1.0) * 65535.0) as u16).to_be_bytes()).collect();
    wr.write_image_data(&data).map_err(|e| e.to_string())
}

/// Einstellungen für das Gelände
#[derive(Clone, Debug)]
pub struct TerrainOpts {
    /// Kantenlänge des ganzen Geländes (Einheiten)
    pub size: f64,
    /// Höhe für Weiß
    pub height: f64,
    /// Kacheln pro Seite
    pub tiles: usize,
    /// Displacement-Power (2–4)
    pub power: u32,
    pub material: String,
    /// linke untere Ecke (Boden-Oberkante)
    pub origin: DVec3,
}

impl Default for TerrainOpts {
    fn default() -> Self {
        TerrainOpts { size: 4096.0, height: 512.0, tiles: 8, power: 3, material: "nature/blendrockgrass004a".into(), origin: DVec3::new(-2048.0, -2048.0, 0.0) }
    }
}

/// Gelände in die Map einfügen (ein Rückgängig-Schritt). Liefert die Anzahl Kacheln.
pub fn build(ed: &mut Editor, hm: &Heights, o: &TerrainOpts) -> usize {
    ed.checkpoint();
    let tile = o.size / o.tiles as f64;
    let mut ids = Vec::new();
    for ty in 0..o.tiles {
        for tx in 0..o.tiles {
            let min = o.origin + DVec3::new(tx as f64 * tile, ty as f64 * tile, -16.0);
            let max = min + DVec3::new(tile, tile, 16.0);
            let id = ed.map.alloc_id();
            let mut s = ed.map.make_box(Aabb::new(min, max), &o.material, 0.25);
            s.id = id;
            let Some(top) = s.sides.iter().position(|x| x.plane().is_some_and(|p| p.normal.z > 0.9)) else { continue };
            let Some(mut d) = Disp::new_flat(o.power, &s.polys[top], DVec3::Z) else { continue };
            if let Some(g) = d.geometry(&s.polys[top], DVec3::Z) {
                for (k, p) in g.pos.iter().enumerate() {
                    let u = (p.x - o.origin.x) / o.size;
                    let v = 1.0 - (p.y - o.origin.y) / o.size;
                    let z = hm.sample(u, v) * o.height;
                    d.set_vector(k, DVec3::Z * z, DVec3::Z);
                }
            }
            s.sides[top].disp = Some(d);
            s.update_geometry();
            ed.map.world.solids.push(s.into());
            ids.push(id);
        }
    }
    ed.sel = ids.iter().map(|id| Obj::Solid(*id)).collect();
    ed.status = format!("Gelände aus Heightmap: {} Kacheln ({} × {} Einheiten, Höhe {})", ids.len(), o.size, o.size, o.height);
    ed.touch();
    ids.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_from_heights() {
        // Rampe: links 0, rechts 1
        let w = 17;
        let hm = Heights { w, h: w, v: (0..w * w).map(|i| (i % w) as f32 / (w - 1) as f32).collect() };
        assert!((hm.sample(0.5, 0.3) - 0.5).abs() < 1e-6);
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let o = TerrainOpts { size: 1024.0, height: 256.0, tiles: 2, power: 2, material: "dev/graygrid".into(), origin: DVec3::new(0.0, 0.0, 0.0) };
        assert_eq!(build(&mut ed, &hm, &o), 4);
        let max = ed.map.all_solids().map(|(_, s)| s.bounds.max.z).fold(0.0, f64::max);
        assert!((max - 256.0).abs() < 1e-3, "höchster Punkt {max}");
        // PNG-Roundtrip
        let dir = std::env::temp_dir().join(format!("amboss_hm_{}.png", std::process::id()));
        {
            let f = std::fs::File::create(&dir).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), 4, 4);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&[0, 85, 170, 255].repeat(4)).unwrap();
        }
        let h = load(&dir).unwrap();
        assert_eq!((h.w, h.h), (4, 4));
        assert!((h.v[3] - 1.0).abs() < 1e-6);
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn noise_terrain() {
        let o = NoiseOpts::default();
        let a = noise_heights(&o, 65);
        assert_eq!(a.v.len(), 65 * 65);
        let (mn, mx) = a.v.iter().fold((1.0f32, 0.0f32), |m, &x| (m.0.min(x), m.1.max(x)));
        assert!(mn.abs() < 1e-6 && (mx - 1.0).abs() < 1e-6, "normiert {mn} {mx}");
        // gleicher Startwert = gleiches Gelände, anderer = anderes
        assert_eq!(a.v, noise_heights(&o, 65).v);
        assert_ne!(a.v, noise_heights(&NoiseOpts { seed: 2, ..o.clone() }, 65).v);
        // Insel: Ränder tief
        let isl = noise_heights(&NoiseOpts { island: true, ..o.clone() }, 65);
        let edge = (0..65).map(|i| isl.v[i]).fold(0.0f32, f32::max);
        assert!(edge < 0.3, "Rand {edge}");
        // Talboden: viel exakt 0
        let fl = noise_heights(&NoiseOpts { flat: 0.5, ..o.clone() }, 65);
        assert!(fl.v.iter().filter(|x| **x == 0.0).count() > 65 * 65 / 5);
        // PNG mit 16 Bit hin und zurück
        let p = std::env::temp_dir().join(format!("amboss_noise_{}.png", std::process::id()));
        save_png(&a, &p).unwrap();
        let b = load(&p).unwrap();
        assert_eq!((b.w, b.h), (65, 65));
        assert!(a.v.iter().zip(&b.v).all(|(x, y)| (x - y).abs() < 1e-3));
        let _ = std::fs::remove_file(&p);
    }
}

// ------------------------------------------------------------------ Dialog

/// Zustand des Dialogs „Gelände aus Heightmap“
#[derive(Default)]
pub struct HeightmapDlg {
    pub open: bool,
    pub path: Option<std::path::PathBuf>,
    pub heights: Option<Heights>,
    pub opts: Option<TerrainOpts>,
    pub error: String,
    preview: Option<eframe::egui::TextureHandle>,
    /// 0 = Bild, 1 = Rauschen
    pub mode: usize,
    pub noise: NoiseOpts,
    /// Stand, aus dem das Rausch-Gelände berechnet wurde
    noise_for: Option<(NoiseOpts, usize)>,
}

/// Vorschaubild 128 × 128 eines Höhenfelds
fn preview_texture(ctx: &eframe::egui::Context, h: &Heights) -> eframe::egui::TextureHandle {
    use eframe::egui;
    let n = 128;
    let px: Vec<u8> = (0..n * n)
        .flat_map(|i| {
            let g = (h.sample((i % n) as f64 / (n - 1) as f64, (i / n) as f64 / (n - 1) as f64) * 255.0) as u8;
            [g, g, g, 255]
        })
        .collect();
    ctx.load_texture("heightmap", egui::ColorImage::from_rgba_unmultiplied([n, n], &px), egui::TextureOptions::LINEAR)
}

impl crate::app::App {
    /// Rausch-Gelände neu berechnen, wenn sich Einstellungen oder Auflösung geändert haben
    pub(crate) fn heightmap_noise_update(&mut self, ctx: &eframe::egui::Context) {
        let o = self.hm.opts.clone().unwrap_or_default();
        let res = (o.tiles * ((1usize << o.power.clamp(2, 4)) + 0) + 1).clamp(65, 1025);
        let key = (self.hm.noise.clone(), res);
        if self.hm.noise_for.as_ref() == Some(&key) && self.hm.heights.is_some() {
            return;
        }
        let h = noise_heights(&self.hm.noise, res);
        self.hm.preview = Some(preview_texture(ctx, &h));
        self.hm.heights = Some(h);
        self.hm.path = None;
        self.hm.error.clear();
        self.hm.noise_for = Some(key);
    }
}

impl crate::app::App {
    pub(crate) fn heightmap_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme;
        use eframe::egui::{self, RichText, Vec2};
        if !self.hm.open {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut pick = false;
        let mut run = false;
        let mut cancel = false;
        let mut save = false;
        let mut mode_changed = false;
        if self.hm.mode == 1 {
            self.heightmap_noise_update(ctx);
        }
        let opts = self.hm.opts.get_or_insert_with(|| TerrainOpts { material: self.ed.material.clone(), ..Default::default() });
        let d = theme::Dlg::new("Gelände aus Heightmap", 620.0).subtitle(if self.hm.mode == 1 { crate::i18n::tr("Rausch-Generator").to_string() } else { self.hm.path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_default() }).footer().no_scroll();
        let mode = &mut self.hm.mode;
        let noise = &mut self.hm.noise;
        let preview = self.hm.preview.as_ref().map(|t| t.id());
        let has = self.hm.heights.is_some();
        let dims = self.hm.heights.as_ref().map(|h| format!("{} × {} Pixel", h.w, h.h));
        let err = self.hm.error.clone();
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.add_space(12.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(340.0);
                            let m = theme::segmented(ui, *mode, &[crate::i18n::tr("Bild"), crate::i18n::tr("Rauschen")], true);
                            if m != *mode {
                                *mode = m;
                                mode_changed = true;
                            }
                            ui.add_space(6.0);
                            if *mode == 0 {
                                if ui.button(crate::i18n::tr("Bild wählen… (PNG oder RAW 16 Bit)")).clicked() {
                                    pick = true;
                                }
                                if let Some(dm) = &dims {
                                    ui.label(RichText::new(dm).color(th.text3));
                                }
                            } else {
                                theme::field_row(ui, "Hügelgröße", |ui| ui.add(egui::Slider::new(&mut noise.scale, 0.05..=1.0).show_value(false))).on_hover_text(crate::i18n::tr("Klein = viele kleine Hügel, groß = wenige weite Berge"));
                                theme::field_row(ui, "Detail-Lagen", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut noise.octaves)).range(1..=8)));
                                theme::field_row(ui, "Rauheit", |ui| ui.add(egui::Slider::new(&mut noise.roughness, 0.2..=0.8).show_value(false)));
                                theme::field_row(ui, "Talboden", |ui| ui.add(egui::Slider::new(&mut noise.flat, 0.0..=0.8).show_value(false))).on_hover_text(crate::i18n::tr("Alles unterhalb dieser Höhe wird flach – gut für begehbare Täler"));
                                theme::field_row(ui, "Terrassen", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut noise.terraces)).range(0..=12)));
                                ui.horizontal(|ui| {
                                    theme::checkbox(ui, &mut noise.ridged, crate::i18n::tr("Bergkämme"));
                                    theme::checkbox(ui, &mut noise.island, crate::i18n::tr("Ränder flach"));
                                });
                                theme::field_row(ui, "Startwert", |ui| {
                                    ui.add(crate::theme::dv(egui::DragValue::new(&mut noise.seed)));
                                    if ui.small_button(crate::i18n::tr("Würfeln")).clicked() {
                                        noise.seed = noise.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) >> 8;
                                    }
                                });
                                if ui.small_button(crate::i18n::tr("Als PNG speichern…")).on_hover_text(crate::i18n::tr("16-Bit-Graustufenbild, z. B. zum Weiterbearbeiten")).clicked() {
                                    save = true;
                                }
                            }
                            if !err.is_empty() {
                                ui.colored_label(th.err, &err);
                            }
                            ui.add_space(8.0);
                            theme::field_row(ui, "Kantenlänge", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut opts.size)).range(256.0..=32768.0).speed(64.0).suffix(" u")));
                            theme::field_row(ui, "Höhe (Weiß)", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut opts.height)).range(0.0..=8192.0).speed(8.0).suffix(" u")));
                            theme::field_row(ui, "Kacheln pro Seite", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut opts.tiles)).range(1..=32)));
                            theme::field_row(ui, "Power", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut opts.power)).range(2..=4)));
                            theme::field_row(ui, "Material", |ui| ui.add(egui::TextEdit::singleline(&mut opts.material).font(theme::mono(11.5)).desired_width(170.0)));
                            theme::field_row(ui, "Ecke links unten", |ui| {
                                for k in 0..3 {
                                    ui.add(crate::theme::dv(egui::DragValue::new(&mut opts.origin[k])).speed(16.0));
                                }
                            });
                            let n = opts.tiles * opts.tiles;
                            let res = (1usize << opts.power) + 1;
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{n} Kacheln · Auflösung {} × {} Punkte", opts.tiles * (res - 1) + 1, opts.tiles * (res - 1) + 1))).color(th.text3).size(11.5));
                        });
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(200.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 8.0, th.vp);
                        match preview {
                            Some(id) => {
                                egui::Image::new((id, r.size())).corner_radius(8).paint_at(ui, r);
                            }
                            None => {
                                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Keine Vorschau"), theme::sans(12.0), th.text3);
                            }
                        }
                    });
                });
            }
            theme::Part::Footer => {
                if ui.add_enabled(has, egui::Button::new(RichText::new(crate::i18n::tr("Gelände erstellen")).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    run = true;
                }
                if theme::secondary_button(ui, "Abbrechen").clicked() {
                    cancel = true;
                }
                theme::footer_hint(ui, "Weiß = hoch, Schwarz = tief · Ein Rückgängig-Schritt");
            }
            _ => {}
        });
        if mode_changed {
            // Wechsel Bild ↔ Rauschen: das jeweils andere Höhenfeld verwerfen
            self.hm.heights = None;
            self.hm.preview = None;
            self.hm.noise_for = None;
            self.hm.path = None;
        }
        if save {
            if let (Some(h), Some(p)) = (&self.hm.heights, rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("heightmap.png").save_file()) {
                match save_png(h, &p) {
                    Ok(()) => self.ed.status = format!("Heightmap gespeichert: {}", p.display()),
                    Err(e) => self.hm.error = e,
                }
            }
        }
        if pick {
            if let Some(p) = rfd::FileDialog::new().add_filter("Heightmap", &["png", "raw", "r16"]).pick_file() {
                match load(&p) {
                    Ok(h) => {
                        self.hm.preview = Some(preview_texture(ctx, &h));
                        self.hm.heights = Some(h);
                        self.hm.error.clear();
                        self.hm.path = Some(p);
                    }
                    Err(e) => self.hm.error = e,
                }
            }
        }
        if run {
            if let (Some(h), Some(o)) = (&self.hm.heights, self.hm.opts.clone()) {
                build(&mut self.ed, h, &o);
                open = false;
            }
        }
        if cancel {
            open = false;
        }
        self.hm.open = open;
    }
}
