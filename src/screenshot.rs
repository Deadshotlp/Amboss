//! Screenshots in hoher Auflösung und 360°-Panoramen aus der 3D-Ansicht.
//! Der Renderer zeichnet außerhalb des Bildschirms (ohne Hilfslinien), hier wird das Ergebnis
//! verkleinert (Supersampling) bzw. aus sechs Würfelseiten ein Panorama (equirektangulär) gemacht.

use glam::{Mat4, Vec3};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotKind {
    /// Bild in Breite × Höhe, `ss` = doppelt rendern und verkleinern (glattere Kanten)
    Image { w: u32, h: u32, ss: bool },
    /// 360°-Panorama aus sechs Würfelseiten mit Kantenlänge `face`; Ergebnis 4·face × 2·face
    Panorama { face: u32 },
}

#[derive(Debug, Clone)]
pub struct ShotReq {
    pub path: PathBuf,
    pub kind: ShotKind,
    /// laufende Nummer: der Renderer arbeitet jeden Auftrag nur einmal ab
    pub id: u64,
}

/// Was der Renderer liefert: Bilder (RGBA, oberste Zeile zuerst) und die benutzten Matrizen
pub struct ShotRaw {
    pub req: ShotReq,
    pub images: Result<Vec<Vec<u8>>, String>,
    pub mvps: Vec<Mat4>,
    pub cam: Vec3,
    pub yaw: f32,
}

/// Blickrichtung und Oben-Vektor der sechs Würfelseiten
pub const FACES: [(Vec3, Vec3); 6] = [(Vec3::X, Vec3::Z), (Vec3::NEG_X, Vec3::Z), (Vec3::Y, Vec3::Z), (Vec3::NEG_Y, Vec3::Z), (Vec3::Z, Vec3::Y), (Vec3::NEG_Z, Vec3::Y)];

/// Matrizen der sechs Würfelseiten (90° Blickwinkel, quadratisch)
pub fn cube_mvps(pos: Vec3) -> Vec<Mat4> {
    let proj = Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1.0, 4.0, 65536.0);
    FACES.iter().map(|(d, up)| proj * Mat4::look_at_rh(pos, pos + *d, *up)).collect()
}

/// Größe, in der gerendert wird: (Breite, Höhe, Anzahl Bilder)
pub fn render_size(k: ShotKind) -> (u32, u32, usize) {
    match k {
        ShotKind::Image { w, h, ss } => {
            let f = if ss { 2 } else { 1 };
            (w * f, h * f, 1)
        }
        ShotKind::Panorama { face } => (face, face, 6),
    }
}

/// Auf die halbe Größe verkleinern (Mittel aus 2 × 2)
pub fn half(px: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (nw, nh) = ((w / 2) as usize, (h / 2) as usize);
    let w = w as usize;
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let s = px[((2 * y) * w + 2 * x) * 4 + c] as u32 + px[((2 * y) * w + 2 * x + 1) * 4 + c] as u32 + px[((2 * y + 1) * w + 2 * x) * 4 + c] as u32 + px[((2 * y + 1) * w + 2 * x + 1) * 4 + c] as u32;
                out[(y * nw + x) * 4 + c] = ((s + 2) / 4) as u8;
            }
        }
    }
    out
}

/// Bilinear aus einer Würfelseite lesen (u, v in Pixeln)
fn sample(face: &[u8], n: usize, u: f32, v: f32) -> [f32; 3] {
    let u = (u - 0.5).clamp(0.0, (n - 1) as f32);
    let v = (v - 0.5).clamp(0.0, (n - 1) as f32);
    let (x0, y0) = (u.floor() as usize, v.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
    let (fx, fy) = (u - x0 as f32, v - y0 as f32);
    let p = |x: usize, y: usize, c: usize| face[(y * n + x) * 4 + c] as f32;
    let mut out = [0.0; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let a = p(x0, y0, c) * (1.0 - fx) + p(x1, y0, c) * fx;
        let b = p(x0, y1, c) * (1.0 - fx) + p(x1, y1, c) * fx;
        *o = a * (1.0 - fy) + b * fy;
    }
    out
}

/// Sechs Würfelseiten → equirektanguläres Panorama (Breite 4n, Höhe 2n); die Bildmitte zeigt in Richtung `yaw` (Grad)
pub fn equirect(faces: &[Vec<u8>], n: u32, mvps: &[Mat4], pos: Vec3, yaw: f32) -> Vec<u8> {
    let (w, h) = ((4 * n) as usize, (2 * n) as usize);
    let n = n as usize;
    let yaw0 = yaw.to_radians();
    let mut out = vec![255u8; w * h * 4];
    for y in 0..h {
        let lat = std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / h as f32 * std::f32::consts::PI;
        let (sl, cl) = lat.sin_cos();
        for x in 0..w {
            // nach rechts im Bild = nach rechts drehen (Yaw nimmt ab)
            let lon = ((x as f32 + 0.5) / w as f32 - 0.5) * std::f32::consts::TAU;
            let a = yaw0 - lon;
            let d = Vec3::new(cl * a.cos(), cl * a.sin(), sl);
            let fi = (0..6).max_by(|&i, &j| FACES[i].0.dot(d).total_cmp(&FACES[j].0.dot(d))).unwrap();
            let ndc = mvps[fi].project_point3(pos + d * 64.0);
            let u = (ndc.x + 1.0) * 0.5 * n as f32;
            let v = (1.0 - ndc.y) * 0.5 * n as f32;
            let c = sample(&faces[fi], n, u, v);
            let o = (y * w + x) * 4;
            out[o] = c[0].round() as u8;
            out[o + 1] = c[1].round() as u8;
            out[o + 2] = c[2].round() as u8;
        }
    }
    out
}

pub fn png(px: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().and_then(|mut wr| wr.write_image_data(px)).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Rohbilder des Renderers fertig machen und speichern; liefert (Pfad, Breite, Höhe)
pub fn finish(raw: ShotRaw) -> Result<(PathBuf, u32, u32), String> {
    let imgs = raw.images?;
    let (px, w, h) = match raw.req.kind {
        ShotKind::Image { w, h, ss } => {
            let img = imgs.into_iter().next().ok_or("kein Bild")?;
            if ss {
                (half(&img, w * 2, h * 2), w, h)
            } else {
                (img, w, h)
            }
        }
        ShotKind::Panorama { face } => (equirect(&imgs, face, &raw.mvps, raw.cam, raw.yaw), face * 4, face * 2),
    };
    let data = png(&px, w, h)?;
    std::fs::write(&raw.req.path, data).map_err(|e| format!("{}: {e}", raw.req.path.display()))?;
    Ok((raw.req.path, w, h))
}


// ---------------------------------------------------------------- Dialog
use crate::app::App;
use crate::i18n::tr;
use eframe::egui::{self, RichText};

pub const SIZES: [(u32, u32); 5] = [(1920, 1080), (2560, 1440), (3840, 2160), (5120, 2880), (7680, 4320)];
pub const PANO: [u32; 3] = [512, 1024, 2048];

pub struct ShotUi {
    pub open: bool,
    pub pano: bool,
    pub size: usize,
    pub ss: bool,
    pub face: usize,
    pub rx: Option<std::sync::mpsc::Receiver<Result<(PathBuf, u32, u32), String>>>,
    pub status: String,
    pub next_id: u64,
}

impl Default for ShotUi {
    fn default() -> Self {
        ShotUi { open: false, pano: false, size: 2, ss: true, face: 1, rx: None, status: String::new(), next_id: 0 }
    }
}

impl App {
    pub(crate) fn shot_window(&mut self, ctx: &egui::Context) {
        // Ergebnis des Renderers abholen und im Hintergrund fertig machen
        let done = {
            let mut o = self.view3d.shot_out.lock().unwrap();
            let id = self.view3d.shot.as_ref().map(|r| r.id);
            if o.as_ref().is_some_and(|x| Some(x.req.id) == id) {
                o.take()
            } else {
                None
            }
        };
        if let Some(raw) = done {
            self.view3d.shot = None;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(finish(raw));
            });
            self.shot.rx = Some(rx);
            self.shot.status = tr("Bild wird gespeichert…").into();
        }
        if let Some(rx) = &self.shot.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.shot.rx = None;
                    self.shot.status = match r {
                        Ok((p, w, h)) => crate::i18n::tr_dyn(&format!("Gespeichert: {} ({w} × {h})", p.display())),
                        Err(e) => e,
                    };
                    self.ed.status = self.shot.status.clone();
                    if std::env::var_os("AMBOSS_DEBUG_OPEN").is_some() {
                        eprintln!("shot: {}", self.shot.status);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(_) => self.shot.rx = None,
            }
        }
        if !self.shot.open {
            return;
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut go = false;
        let busy = self.shot.rx.is_some() || self.view3d.shot.is_some();
        let d = crate::theme::Dlg::new(tr("Screenshot in hoher Auflösung"), 420.0).footer();
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 10)).show(ui, |ui| {
                    let s = &mut self.shot;
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut s.pano, false, tr("Bild"));
                        ui.radio_value(&mut s.pano, true, tr("360°-Panorama"));
                    });
                    ui.add_space(6.0);
                    if !s.pano {
                        egui::ComboBox::from_id_salt("shot_size").selected_text(format!("{} × {}", SIZES[s.size].0, SIZES[s.size].1)).show_ui(ui, |ui| {
                            for (i, (w, h)) in SIZES.iter().enumerate() {
                                ui.selectable_value(&mut s.size, i, format!("{w} × {h}"));
                            }
                        });
                        crate::theme::checkbox(ui, &mut s.ss, tr("Kantenglättung (doppelt so groß rendern, dann verkleinern)"));
                        ui.label(RichText::new(tr("Ausschnitt und Blickwinkel wie in der 3D-Ansicht; das Seitenverhältnis kommt aus der gewählten Größe.")).small().color(th.text3));
                    } else {
                        egui::ComboBox::from_id_salt("shot_pano").selected_text(format!("{} × {}", PANO[s.face] * 4, PANO[s.face] * 2)).show_ui(ui, |ui| {
                            for (i, f) in PANO.iter().enumerate() {
                                ui.selectable_value(&mut s.face, i, format!("{} × {}", f * 4, f * 2));
                            }
                        });
                        ui.label(RichText::new(tr("Rundumsicht vom Standort der 3D-Kamera (equirektangulär) – für Panorama-Betrachter, VR oder als Vorschau im Workshop.")).small().color(th.text3));
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(tr("Ohne Hilfslinien und Auswahlrahmen; Darstellungsart (texturiert, Lichtvorschau …) wie in der 3D-Ansicht.")).small().color(th.text3));
                    if !s.status.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&s.status).color(th.text2));
                    }
                });
            }
            crate::theme::Part::Footer => {
                if ui.add_enabled(!busy, egui::Button::new(RichText::new(tr("Speichern unter…")).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(egui::vec2(0.0, 32.0))).clicked() {
                    go = true;
                }
                if crate::theme::secondary_button(ui, tr("Schließen")).clicked() {
                    self.shot.open = false;
                }
            }
            _ => {}
        });
        if go {
            let name = format!("{}{}.png", self.ed.display_name().trim_end_matches(".vmf"), if self.shot.pano { "_360" } else { "" });
            if let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(name).save_file() {
                self.request_shot(path);
            }
        }
        if !open {
            self.shot.open = false;
        }
    }

    /// Screenshot anfordern; der Renderer liefert ihn beim nächsten Zeichnen der 3D-Ansicht
    pub(crate) fn request_shot(&mut self, path: PathBuf) {
        let kind = if self.shot.pano { ShotKind::Panorama { face: PANO[self.shot.face] } } else { ShotKind::Image { w: SIZES[self.shot.size].0, h: SIZES[self.shot.size].1, ss: self.shot.ss } };
        self.shot.next_id += 1;
        self.view3d.shot = Some(ShotReq { path, kind, id: self.shot.next_id });
        // altes Ergebnis verwerfen
        *self.view3d.shot_out.lock().unwrap() = None;
        self.shot.status = tr("Wird gerendert… (die 3D-Ansicht muss sichtbar sein)").into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves() {
        let px = vec![10, 20, 30, 255, 30, 40, 50, 255, 10, 20, 30, 255, 30, 40, 50, 255];
        assert_eq!(half(&px, 2, 2), vec![20, 30, 40, 255]);
        assert_eq!(render_size(ShotKind::Image { w: 1920, h: 1080, ss: true }), (3840, 2160, 1));
    }

    #[test]
    fn panorama_faces_land_in_the_right_place() {
        // jede Würfelseite einfarbig: +X rot, −X grün, +Y blau, −Y gelb, +Z weiß, −Z schwarz
        let cols = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [255, 255, 255], [0, 0, 0]];
        let n = 16u32;
        let faces: Vec<Vec<u8>> = cols.iter().map(|c| (0..n * n).flat_map(|_| [c[0], c[1], c[2], 255]).collect()).collect();
        let pos = Vec3::new(100.0, 200.0, 50.0);
        let mvps = cube_mvps(pos);
        // Blick nach +X (Yaw 0): Bildmitte rot, ganz oben weiß, ganz unten schwarz,
        // ein Viertel nach rechts (Yaw −90° = −Y) gelb, ein Viertel nach links (+Y) blau
        let p = equirect(&faces, n, &mvps, pos, 0.0);
        let (w, h) = ((4 * n) as usize, (2 * n) as usize);
        let at = |x: usize, y: usize| [p[(y * w + x) * 4], p[(y * w + x) * 4 + 1], p[(y * w + x) * 4 + 2]];
        assert_eq!(at(w / 2, h / 2), [255, 0, 0]);
        assert_eq!(at(w / 2, 0), [255, 255, 255]);
        assert_eq!(at(w / 2, h - 1), [0, 0, 0]);
        assert_eq!(at(w * 3 / 4, h / 2), [255, 255, 0]);
        assert_eq!(at(w / 4, h / 2), [0, 0, 255]);
        assert_eq!(at(0, h / 2), [0, 255, 0]);
    }
}
