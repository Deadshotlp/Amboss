//! Im Steam-Workshop veröffentlichen: Addon-Ordner anlegen (BSP, Navmesh, Spawnmenü-Bild, addon.json),
//! mit gmad zu .gma packen und mit gmpublish hochladen; Vorschaubild aus der 3D-Ansicht.

use crate::app::App;
use crate::i18n::tr;
use crate::vtf_write::{resize, Rgba};
use eframe::egui::{self, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Erlaubte Workshop-Tags für Garry's Mod (höchstens zwei)
pub const TAGS: [&str; 9] = ["fun", "roleplay", "scenic", "movie", "realism", "cartoon", "water", "comic", "build"];
/// Addon-Typen, die für Maps sinnvoll sind
pub const KINDS: [&str; 2] = ["map", "servercontent"];

/// Je Map gemerkt (Einstellungen): Workshop-ID, Titel, Tags
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkshopInfo {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub tags: Vec<String>,
}

/// gmad/gmpublish suchen (…/GarrysMod/bin)
pub fn find_tool(game_dir: &str, name: &str) -> Option<PathBuf> {
    let gd = Path::new(game_dir);
    let root = gd.parent()?;
    for d in [root.join("bin"), root.join("bin").join("win64"), root.to_path_buf()] {
        for f in [format!("{name}.exe"), name.to_string(), format!("{name}_linux")] {
            let p = d.join(&f);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub fn addon_json(title: &str, kind: &str, tags: &[String]) -> String {
    let tags: Vec<&String> = tags.iter().filter(|t| TAGS.contains(&t.as_str())).take(2).collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "title": title,
        "type": if KINDS.contains(&kind) { kind } else { "map" },
        "tags": tags,
        "ignore": ["*.vmf", "*.vmx", "*.log", "*.prt", "*.lin", "thumbs.db", "desktop.ini"],
    }))
    .unwrap_or_default()
}

/// Addon-Ordner neu anlegen: maps/<map>.bsp, optional .nav und maps/thumb/<map>.png, addon.json
pub fn stage(dir: &Path, map: &str, bsp: &Path, nav: Option<&Path>, thumb_png: Option<&[u8]>, json: &str) -> Result<Vec<String>, String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let maps = dir.join("maps");
    std::fs::create_dir_all(&maps).map_err(|e| format!("{}: {e}", maps.display()))?;
    let mut files = Vec::new();
    std::fs::copy(bsp, maps.join(format!("{map}.bsp"))).map_err(|e| format!("{}: {e}", bsp.display()))?;
    files.push(format!("maps/{map}.bsp"));
    if let Some(n) = nav {
        std::fs::copy(n, maps.join(format!("{map}.nav"))).map_err(|e| format!("{}: {e}", n.display()))?;
        files.push(format!("maps/{map}.nav"));
    }
    if let Some(png) = thumb_png {
        std::fs::create_dir_all(maps.join("thumb")).map_err(|e| e.to_string())?;
        std::fs::write(maps.join("thumb").join(format!("{map}.png")), png).map_err(|e| e.to_string())?;
        files.push(format!("maps/thumb/{map}.png"));
    }
    std::fs::write(dir.join("addon.json"), json).map_err(|e| e.to_string())?;
    Ok(files)
}

pub fn gmad_args(folder: &Path, out: &Path) -> Vec<String> {
    vec!["create".into(), "-folder".into(), folder.display().to_string(), "-out".into(), out.display().to_string()]
}

/// Neu anlegen (ohne ID) oder aktualisieren
pub fn gmpublish_args(gma: &Path, icon: Option<&Path>, id: &str, changes: &str) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    if id.trim().is_empty() {
        a.extend(["create".into(), "-addon".into(), gma.display().to_string()]);
        if let Some(i) = icon {
            a.extend(["-icon".into(), i.display().to_string()]);
        }
    } else {
        a.extend(["update".into(), "-addon".into(), gma.display().to_string(), "-id".into(), id.trim().to_string()]);
        if let Some(i) = icon {
            a.extend(["-icon".into(), i.display().to_string()]);
        }
        if !changes.trim().is_empty() {
            a.extend(["-changes".into(), changes.trim().to_string()]);
        }
    }
    a
}

/// Workshop-ID aus der Ausgabe von gmpublish („UID: 123456789“)
pub fn parse_uid(log: &[String]) -> Option<String> {
    for l in log.iter().rev() {
        if let Some(i) = l.find("UID:").or_else(|| l.find("UID :")) {
            let digits: String = l[i..].chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 5 {
                return Some(digits);
            }
        }
    }
    None
}

/// Quadratischer Ausschnitt aus der Mitte, auf `size` skaliert
pub fn square(img: &Rgba, size: u32) -> Rgba {
    let s = img.w.min(img.h);
    let (x0, y0) = ((img.w - s) / 2, (img.h - s) / 2);
    let mut px = Vec::with_capacity((s * s * 4) as usize);
    for y in 0..s {
        let row = (((y0 + y) * img.w + x0) * 4) as usize;
        px.extend_from_slice(&img.px[row..row + (s * 4) as usize]);
    }
    resize(&Rgba { w: s, h: s, px }, size, size)
}

/// Workshop-Bild: 512 × 512 JPEG (verlangt gmpublish)
pub fn icon_jpeg(img: &Rgba) -> Result<Vec<u8>, String> {
    let sq = square(img, 512);
    let rgb: Vec<u8> = sq.px.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90).encode(&rgb, 512, 512, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Bild fürs Spawnmenü (maps/thumb), 256 × 256 PNG
pub fn thumb_png(img: &Rgba) -> Result<Vec<u8>, String> {
    let sq = square(img, 256);
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, 256, 256);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().and_then(|mut w| w.write_image_data(&sq.px)).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Programme nacheinander ausführen, Ausgabe ins Log; Ende mit Erfolg/Fehler
fn spawn_chain(steps: Vec<(PathBuf, Vec<String>, PathBuf)>, log: Arc<Mutex<Vec<String>>>) -> Arc<Mutex<Option<bool>>> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let done = Arc::new(Mutex::new(None));
    let d2 = done.clone();
    std::thread::spawn(move || {
        let say = |s: String| log.lock().unwrap().push(s);
        for (exe, args, cwd) in steps {
            say(format!("> {} {}", exe.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(), args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ")));
            let mut cmd = Command::new(&exe);
            cmd.args(&args).current_dir(&cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let mut c = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    say(format!("{}: {e}", exe.display()));
                    *d2.lock().unwrap() = Some(false);
                    return;
                }
            };
            let err = c.stderr.take();
            let log2 = log.clone();
            let et = std::thread::spawn(move || {
                if let Some(e) = err {
                    for l in BufReader::new(e).lines().map_while(Result::ok) {
                        log2.lock().unwrap().push(l);
                    }
                }
            });
            if let Some(o) = c.stdout.take() {
                for l in BufReader::new(o).split(b'\n').map_while(Result::ok) {
                    say(String::from_utf8_lossy(&l).trim_end().to_string());
                }
            }
            let _ = et.join();
            if !c.wait().map(|s| s.success()).unwrap_or(false) {
                *d2.lock().unwrap() = Some(false);
                return;
            }
        }
        *d2.lock().unwrap() = Some(true);
    });
    done
}

/// Zustand des Dialogs
#[derive(Default)]
pub(crate) struct WorkshopUi {
    pub open: bool,
    pub info: WorkshopInfo,
    pub changes: String,
    pub nav: bool,
    pub spawn_thumb: bool,
    /// Vorschaubild (RGBA), Textur zur Anzeige
    pub image: Option<Rgba>,
    tex: Option<egui::TextureHandle>,
    /// 1 = beim nächsten Bild ohne Dialog/Beschriftungen aufnehmen, 2 = warte auf Bild
    pub capture: u8,
    pub log: Arc<Mutex<Vec<String>>>,
    job: Option<(Arc<Mutex<Option<bool>>>, bool)>,
}

impl App {
    pub(crate) fn open_workshop(&mut self) {
        let map = self.ed.map_name();
        let ws = &mut self.ws;
        ws.info = self.settings.workshop.get(&map).cloned().unwrap_or_default();
        if ws.info.title.is_empty() {
            ws.info.title = map.clone();
        }
        if ws.info.kind.is_empty() {
            ws.info.kind = "map".into();
        }
        ws.nav = true;
        ws.spawn_thumb = true;
        ws.open = true;
    }

    fn workshop_bsp(&self) -> Option<PathBuf> {
        let map = self.ed.map_name();
        let g = Path::new(&self.settings.game_dir).join("maps").join(format!("{map}.bsp"));
        if g.is_file() {
            return Some(g);
        }
        self.ed.path.as_ref().map(|p| p.with_extension("bsp")).filter(|p| p.is_file())
    }

    fn workshop_nav(&self) -> Option<PathBuf> {
        let map = self.ed.map_name();
        let g = Path::new(&self.settings.game_dir).join("maps").join(format!("{map}.nav"));
        g.is_file().then_some(g)
    }

    /// Bildschirmfoto abholen und auf die 3D-Ansicht zuschneiden
    pub(crate) fn workshop_capture(&mut self, ctx: &egui::Context) {
        match self.ws.capture {
            1 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.ws.capture = 2;
                ctx.request_repaint();
            }
            2 => {
                let shot = ctx.input(|i| i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
                let Some(img) = shot else {
                    ctx.request_repaint();
                    return;
                };
                self.ws.capture = 0;
                let Some(r) = self.ui_rects.get("view3d").copied() else { return };
                let ppp = ctx.pixels_per_point();
                let (iw, ih) = (img.size[0] as i64, img.size[1] as i64);
                let x0 = ((r.min.x * ppp) as i64).clamp(0, iw - 1);
                let y0 = ((r.min.y * ppp) as i64).clamp(0, ih - 1);
                let x1 = ((r.max.x * ppp) as i64).clamp(x0 + 1, iw);
                let y1 = ((r.max.y * ppp) as i64).clamp(y0 + 1, ih);
                let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
                let mut px = Vec::with_capacity((w * h * 4) as usize);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let c = img.pixels[(y * iw + x) as usize];
                        px.extend([c.r(), c.g(), c.b(), 255]);
                    }
                }
                self.ws.image = Some(Rgba { w, h, px });
                self.ws.tex = None;
            }
            _ => {}
        }
    }

    /// Während der Aufnahme: Beschriftungen der Ansichten weglassen
    pub(crate) fn capturing(&self) -> bool {
        self.ws.capture > 0
    }

    fn workshop_run(&mut self, upload: bool) -> Result<(), String> {
        let map = self.ed.map_name();
        let log = self.ws.log.clone();
        log.lock().unwrap().clear();
        let bsp = self.workshop_bsp().ok_or_else(|| tr("Keine kompilierte BSP gefunden – erst kompilieren").to_string())?;
        let gmad = find_tool(&self.settings.game_dir, "gmad").ok_or_else(|| tr("gmad nicht gefunden (…/GarrysMod/bin)").to_string())?;
        let gmpublish = if upload { Some(find_tool(&self.settings.game_dir, "gmpublish").ok_or_else(|| tr("gmpublish nicht gefunden (…/GarrysMod/bin)").to_string())?) } else { None };
        if upload && self.ws.image.is_none() && self.ws.info.id.trim().is_empty() {
            return Err(tr("Für eine neue Veröffentlichung braucht es ein Vorschaubild").into());
        }
        let base = self.ed.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(std::env::temp_dir).join("amboss_workshop");
        let folder = base.join(&map);
        let thumb = if self.ws.spawn_thumb { self.ws.image.as_ref().map(thumb_png).transpose()? } else { None };
        let nav = if self.ws.nav { self.workshop_nav() } else { None };
        let files = stage(&folder, &map, &bsp, nav.as_deref(), thumb.as_deref(), &addon_json(&self.ws.info.title, &self.ws.info.kind, &self.ws.info.tags))?;
        log.lock().unwrap().push(crate::i18n::tr_dyn(&format!("Addon-Ordner: {} ({})", folder.display(), files.join(", "))));
        let gma = base.join(format!("{map}.gma"));
        let mut steps = vec![(gmad, gmad_args(&folder, &gma), base.clone())];
        if let Some(gp) = gmpublish {
            let icon = match &self.ws.image {
                Some(img) => {
                    let p = base.join(format!("{map}_icon.jpg"));
                    std::fs::write(&p, icon_jpeg(img)?).map_err(|e| e.to_string())?;
                    Some(p)
                }
                None => None,
            };
            steps.push((gp, gmpublish_args(&gma, icon.as_deref(), &self.ws.info.id, &self.ws.changes), base.clone()));
        }
        self.settings.workshop.insert(map, self.ws.info.clone());
        self.ws.job = Some((spawn_chain(steps, log), upload));
        Ok(())
    }

    /// Für Debug/Tests: wie die Knöpfe „Nur .gma“ (true) bzw. „Veröffentlichen“
    pub(crate) fn workshop_run_pub(&mut self, gma_only: bool) -> Result<(), String> {
        self.workshop_run(!gma_only)
    }

    fn poll_workshop(&mut self) {
        let Some((done, upload)) = &self.ws.job else { return };
        let upload = *upload;
        let Some(ok) = *done.lock().unwrap() else {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        };
        self.ws.job = None;
        let log = self.ws.log.lock().unwrap().clone();
        if ok && upload {
            if let Some(id) = parse_uid(&log) {
                if self.ws.info.id != id {
                    self.ws.info.id = id;
                    self.settings.workshop.insert(self.ed.map_name(), self.ws.info.clone());
                }
            }
            self.ed.status = tr("Im Workshop veröffentlicht").into();
        } else if ok {
            self.ed.status = tr(".gma erstellt").into();
        } else {
            self.ed.status = tr("Workshop: fehlgeschlagen (siehe Log)").into();
        }
        self.ws.log.lock().unwrap().push(if ok { tr("Fertig.").to_string() } else { tr("Fehlgeschlagen.").to_string() });
    }

    pub(crate) fn workshop_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        self.workshop_capture(ctx);
        self.poll_workshop();
        if !self.ws.open || self.ws.capture > 0 {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let (mut gma_only, mut upload, mut shot, mut pick) = (false, false, false, false);
        let bsp = self.workshop_bsp();
        let nav = self.workshop_nav();
        let gmad = find_tool(&self.settings.game_dir, "gmad");
        let gmpub = find_tool(&self.settings.game_dir, "gmpublish");
        let busy = self.ws.job.is_some();
        let log: Vec<String> = self.ws.log.lock().unwrap().clone();
        if let (Some(img), None) = (&self.ws.image, &self.ws.tex) {
            let sq = square(img, 256);
            self.ws.tex = Some(ctx.load_texture("ws_icon", egui::ColorImage::from_rgba_unmultiplied([256, 256], &sq.px), egui::TextureOptions::LINEAR));
        }
        let d = theme::Dlg::new(tr("Im Workshop veröffentlichen"), 760.0).subtitle(self.ed.map_name()).footer();
        let ws = &mut self.ws;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.add_space(10.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        // links: Bild
                        ui.vertical(|ui| {
                            ui.set_width(220.0);
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(212.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, 8.0, th.vp);
                            match &ws.tex {
                                Some(t) => {
                                    ui.painter().image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                                }
                                None => {
                                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, tr("Kein Vorschaubild"), theme::sans(12.0), th.text3);
                                }
                            }
                            if ui.button(tr("Aus 3D-Ansicht")).on_hover_text(tr("Nimmt das Bild der 3D-Ansicht (quadratischer Ausschnitt aus der Mitte)")).clicked() {
                                shot = true;
                            }
                            if ui.button(tr("Bild wählen…")).clicked() {
                                pick = true;
                            }
                            ui.label(RichText::new(tr("Wird als 512 × 512 JPEG hochgeladen")).size(11.0).color(th.text3));
                        });
                        // rechts: Angaben
                        ui.vertical(|ui| {
                            theme::field_row(ui, "Titel", |ui| ui.add(egui::TextEdit::singleline(&mut ws.info.title).desired_width(300.0)));
                            theme::field_row(ui, "Typ", |ui| {
                                for k in KINDS {
                                    ui.radio_value(&mut ws.info.kind, k.to_string(), k);
                                }
                            });
                            theme::field_row(ui, "Tags (höchstens 2)", |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.set_max_width(320.0);
                                    for t in TAGS {
                                        let mut on = ws.info.tags.iter().any(|x| x == t);
                                        let full = ws.info.tags.len() >= 2 && !on;
                                        if ui.add_enabled(!full, egui::Checkbox::new(&mut on, t)).changed() {
                                            if on {
                                                ws.info.tags.push(t.to_string());
                                            } else {
                                                ws.info.tags.retain(|x| x != t);
                                            }
                                        }
                                    }
                                });
                            });
                            theme::field_row(ui, "Workshop-ID", |ui| {
                                ui.add(egui::TextEdit::singleline(&mut ws.info.id).desired_width(160.0).hint_text(theme::hint(tr("leer = neu anlegen"))));
                                if !ws.info.id.trim().is_empty() && ui.link(tr("Seite öffnen")).clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", ws.info.id.trim())));
                                }
                            });
                            if !ws.info.id.trim().is_empty() {
                                theme::field_row(ui, "Änderungen", |ui| ui.add(egui::TextEdit::singleline(&mut ws.changes).desired_width(300.0).hint_text(theme::hint(tr("kurze Notiz zum Update")))));
                            }
                            theme::field_row(ui, "Beilegen", |ui| {
                                ui.add_enabled(nav.is_some(), egui::Checkbox::new(&mut ws.nav, tr("Navmesh (.nav)")));
                                theme::checkbox(ui, &mut ws.spawn_thumb, tr("Bild fürs Spawnmenü"));
                            });
                            ui.add_space(4.0);
                            let line = |ui: &mut egui::Ui, ok: bool, text: String| {
                                ui.label(RichText::new(format!("{} {text}", if ok { "✓" } else { "✗" })).size(11.5).color(if ok { th.ok } else { th.warn }));
                            };
                            match &bsp {
                                Some(p) => line(ui, true, crate::i18n::tr_dyn(&format!("BSP: {}", p.display()))),
                                None => line(ui, false, tr("Keine kompilierte BSP gefunden – erst kompilieren").into()),
                            }
                            line(ui, gmad.is_some(), if gmad.is_some() { "gmad".into() } else { tr("gmad nicht gefunden (…/GarrysMod/bin)").into() });
                            line(ui, gmpub.is_some(), if gmpub.is_some() { "gmpublish".into() } else { tr("gmpublish nicht gefunden (…/GarrysMod/bin)").into() });
                            ui.label(RichText::new(tr("Steam muss laufen und angemeldet sein. Beschreibung und Sichtbarkeit stellst du danach auf der Workshop-Seite ein (neue Addons sind zunächst versteckt). Eigene Inhalte vorher in die BSP packen (Kompilieren & Exportieren).")).size(11.5).color(th.text3));
                        });
                    });
                });
                if !log.is_empty() {
                    egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(8).outer_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(150.0).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
                            for l in &log {
                                let c = if l.to_ascii_lowercase().contains("error") || l.contains("fehlgeschlagen") || l.contains("Fehlgeschlagen") { th.err } else { th.text2 };
                                ui.label(RichText::new(crate::i18n::tr_dyn(l)).font(theme::mono(11.0)).color(c));
                            }
                        });
                    });
                }
            }
            theme::Part::Footer => {
                let ready = bsp.is_some() && gmad.is_some() && !busy && !ws.info.title.trim().is_empty();
                let label = if busy { tr("Läuft…") } else if ws.info.id.trim().is_empty() { tr("Veröffentlichen") } else { tr("Update hochladen") };
                if ui.add_enabled(ready && gmpub.is_some(), egui::Button::new(RichText::new(label).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    upload = true;
                }
                if ui.add_enabled(ready, egui::Button::new(tr("Nur .gma erstellen")).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    gma_only = true;
                }
            }
            _ => {}
        });
        if !open {
            self.ws.open = false;
        }
        if shot {
            self.ws.capture = 1;
            ctx.request_repaint();
        }
        if pick {
            if let Some(p) = rfd::FileDialog::new().add_filter(tr("Bilder (PNG, JPEG)"), &["png", "jpg", "jpeg"]).pick_file() {
                match crate::model_import::load_image(&crate::model_import::Image::File(p)) {
                    Ok(img) => {
                        self.ws.image = Some(img);
                        self.ws.tex = None;
                    }
                    Err(e) => self.ed.status = crate::i18n::tr_dyn(&e),
                }
            }
        }
        if upload || gma_only {
            if let Err(e) = self.workshop_run(upload) {
                self.ws.log.lock().unwrap().push(e.clone());
                self.ed.status = crate::i18n::tr_dyn(&e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addon_files_and_args() {
        let j = addon_json("Meine Map", "map", &["build".into(), "fun".into(), "water".into(), "quatsch".into()]);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["type"], "map");
        assert_eq!(v["tags"].as_array().unwrap().len(), 2);
        let d = std::env::temp_dir().join(format!("amboss_ws_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let bsp = d.join("x.bsp");
        std::fs::write(&bsp, b"VBSP").unwrap();
        let img = Rgba { w: 40, h: 20, px: [10u8, 200, 30, 255].repeat(800) };
        let png = thumb_png(&img).unwrap();
        let files = stage(&d.join("addon"), "gm_test", &bsp, None, Some(&png), &j).unwrap();
        assert_eq!(files, vec!["maps/gm_test.bsp", "maps/thumb/gm_test.png"]);
        assert!(d.join("addon/addon.json").is_file() && d.join("addon/maps/gm_test.bsp").is_file());
        let jpg = icon_jpeg(&img).unwrap();
        assert!(jpg.starts_with(&[0xFF, 0xD8]));
        let back = crate::model_import::load_image(&crate::model_import::Image::Bytes(jpg, "t".into())).unwrap();
        assert_eq!((back.w, back.h), (512, 512));
        assert!(back.px[1] > 150 && back.px[0] < 60, "{:?}", &back.px[..4]);
        assert_eq!(gmad_args(Path::new("/a"), Path::new("/b.gma"))[0], "create");
        let new = gmpublish_args(Path::new("/b.gma"), Some(Path::new("/i.jpg")), "", "x");
        assert_eq!(new[0], "create");
        assert!(new.contains(&"-icon".to_string()) && !new.contains(&"-changes".to_string()));
        let upd = gmpublish_args(Path::new("/b.gma"), None, "123456", "Neue Brücke");
        assert_eq!(upd[0], "update");
        assert!(upd.windows(2).any(|w| w[0] == "-id" && w[1] == "123456"));
        assert_eq!(parse_uid(&["Uploading…".into(), "Success! UID: 2876543210".into()]), Some("2876543210".into()));
        assert_eq!(parse_uid(&["nichts".into()]), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
