//! Brushes als Modell (wie Propper): SMD + QC schreiben und mit studiomdl kompilieren.

use crate::editor::{Editor, Obj};
use crate::map::Solid;
use glam::DVec3;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct PropOpts {
    /// Name ohne Endung, z. B. "kiste_gross" → models/amboss/kiste_gross.mdl
    pub name: String,
    /// Unterordner unter models/
    pub folder: String,
    pub surfaceprop: String,
    /// true = prop_static ($staticprop), false = Physik-Prop mit Masse
    pub is_static: bool,
    pub mass: f64,
    /// Nach dem Kompilieren die Brushes durch ein Prop ersetzen
    pub replace: bool,
}

impl Default for PropOpts {
    fn default() -> Self {
        PropOpts { name: crate::i18n::tr("mein_prop").into(), folder: "amboss".into(), surfaceprop: "default".into(), is_static: true, mass: 50.0, replace: true }
    }
}

/// Werkzeug-Texturen werden nicht ins Modell übernommen
fn skip_material(m: &str) -> bool {
    let l = m.to_ascii_lowercase();
    l.starts_with("tools/") && !l.ends_with("toolsblack")
}

/// Materialname im Modell: ganzer Pfad mit "_" statt "/" (eindeutig, ein Ordner)
pub fn smd_name(m: &str) -> String {
    m.to_ascii_lowercase().replace(['\\', '/', ' '], "_")
}

/// Ordner der Modell-Materialien relativ zu materials/
pub fn mat_dir(o: &PropOpts) -> String {
    format!("models/{}/{}/", o.folder, o.name)
}

/// Brush-Materialien (LightmappedGeneric) leuchten auf Modellen nicht richtig –
/// wie Propper eine VertexLitGeneric-Kopie schreiben, die dieselben Texturen nutzt.
pub fn model_vmt(orig: Option<&str>, material: &str) -> String {
    let mut keys: Vec<(String, String)> = Vec::new();
    if let Some(root) = orig.and_then(|src| crate::keyvalues::parse(src).ok()).and_then(|b| b.into_iter().next()) {
        let mut take = |b: &crate::keyvalues::KvBlock| {
            for (k, v) in b.pairs() {
                let kl = k.to_ascii_lowercase();
                if ["$basetexture", "$bumpmap", "$normalmap", "$translucent", "$alphatest", "$alphatestreference", "$nocull", "$color", "$color2", "$envmap", "$envmapmask", "$envmaptint", "$surfaceprop", "$selfillum", "$detail", "$detailscale", "$detailblendfactor", "$basetexturetransform"].contains(&kl.as_str()) && !keys.iter().any(|(x, _)| *x == kl) {
                    keys.push((kl, v.to_string()));
                }
            }
        };
        take(&root);
        for b in root.blocks() {
            take(b);
        }
    }
    if !keys.iter().any(|(k, _)| k == "$basetexture") {
        keys.insert(0, ("$basetexture".into(), material.replace('\\', "/")));
    }
    let mut out = String::from("\"VertexLitGeneric\"\n{\n");
    for (k, v) in keys {
        let _ = writeln!(out, "\t\"{k}\" \"{v}\"");
    }
    let _ = writeln!(out, "\t\"$model\" \"1\"\n}}");
    out
}

/// Modell-Materialien unter `<game_dir>/materials/models/<ordner>/<name>/` schreiben
pub fn write_model_vmts(ed: &Editor, o: &PropOpts, game_dir: &Path, fs: Option<&crate::gamefs::GameFs>) -> Result<Vec<PathBuf>, String> {
    let mats: Vec<String> = selected_solids(ed).iter().flat_map(|s| s.sides.iter()).map(|sd| sd.material.clone()).collect();
    write_vmts(mats, &game_dir.join("materials").join(mat_dir(o)), fs)
}

/// VertexLitGeneric-Kopien der Materialien in `dir` schreiben (Werkzeug-Texturen ausgenommen)
pub fn write_vmts(mut mats: Vec<String>, dir: &Path, fs: Option<&crate::gamefs::GameFs>) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    mats.retain(|m| !skip_material(m));
    mats.sort_by_key(|m| m.to_ascii_lowercase());
    mats.dedup_by_key(|m| m.to_ascii_lowercase());
    let mut written = Vec::new();
    for m in mats {
        let src = fs.and_then(|f| f.read(&format!("materials/{}.vmt", m.to_ascii_lowercase().replace('\\', "/")))).map(|b| String::from_utf8_lossy(&b).to_string());
        let f = dir.join(format!("{}.vmt", smd_name(&m)));
        std::fs::write(&f, model_vmt(src.as_deref(), &m)).map_err(|e| format!("{}: {e}", f.display()))?;
        written.push(f);
    }
    Ok(written)
}

fn selected_solids(ed: &Editor) -> Vec<&Solid> {
    ed.sel.iter().filter_map(|x| if let Obj::Solid(id) = x { ed.map.solid(*id) } else { None }).filter(|s| !s.has_disp()).collect()
}

/// SMD-Dreiecke der Brushes. `tex_size` liefert die Texturgröße eines Materials (für UV).
pub fn smd_triangles(solids: &[&Solid], origin: DVec3, tex_size: &dyn Fn(&str) -> (f64, f64), with_tool_faces: bool) -> String {
    let mut out = String::new();
    for s in solids {
        for (i, poly) in s.polys.iter().enumerate() {
            let side = &s.sides[i];
            if poly.len() < 3 || (!with_tool_faces && skip_material(&side.material)) {
                continue;
            }
            let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
            let (tw, th) = tex_size(&side.material);
            let mat = smd_name(&side.material);
            let uv = |p: DVec3| {
                let u = (p.dot(side.uaxis.axis) / side.uaxis.scale.max(1e-4) + side.uaxis.shift) / tw;
                let v = (p.dot(side.vaxis.axis) / side.vaxis.scale.max(1e-4) + side.vaxis.shift) / th;
                (u, -v)
            };
            // Brush-Polygone sind von außen gesehen gegen den Uhrzeigersinn – so erwartet es auch
            // studiomdl (wie Blender-Exporte); es dreht beim Einlesen selbst um
            for k in 1..poly.len() - 1 {
                let _ = writeln!(out, "{mat}");
                for p in [poly[0], poly[k], poly[k + 1]] {
                    let q = p - origin;
                    let (u, v) = uv(p);
                    let _ = writeln!(out, "0 {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.5} {:.5} 0", q.x, q.y, q.z, n.x, n.y, n.z, u, v);
                }
            }
        }
    }
    out
}

pub(crate) fn smd(tris: &str) -> String {
    format!("version 1\nnodes\n0 \"root\" -1\nend\nskeleton\ntime 0\n0 0 0 0 0 0 0\nend\ntriangles\n{tris}end\n")
}

/// QC-Datei
pub fn qc(o: &PropOpts, cdmaterials: &[String]) -> String {
    let mut q = String::new();
    let _ = writeln!(q, "$modelname \"{}/{}.mdl\"", o.folder, o.name);
    let _ = writeln!(q, "$body \"body\" \"{}_ref.smd\"", o.name);
    for c in cdmaterials {
        let _ = writeln!(q, "$cdmaterials \"{c}\"");
    }
    if o.is_static {
        let _ = writeln!(q, "$staticprop");
    }
    let _ = writeln!(q, "$surfaceprop \"{}\"", o.surfaceprop);
    let _ = writeln!(q, "$sequence \"idle\" \"{}_ref.smd\" fps 30", o.name);
    let _ = writeln!(q, "$collisionmodel \"{}_phys.smd\" {{", o.name);
    let _ = writeln!(q, "\t$concave");
    let _ = writeln!(q, "\t$mass {}", if o.is_static { 1.0 } else { o.mass });
    let _ = writeln!(q, "}}");
    q
}

/// Ergebnis des Exports
pub struct Export {
    pub dir: PathBuf,
    pub qc: PathBuf,
    /// Mitte unten der Auswahl (dort wird das Prop gesetzt)
    pub origin: DVec3,
    pub solids: Vec<u32>,
}

/// Ausgewählte Brushes als SMD/QC in `dir` schreiben
pub fn export(ed: &Editor, o: &PropOpts, dir: &Path, tex_size: &dyn Fn(&str) -> (f64, f64)) -> Result<Export, String> {
    let ids: Vec<u32> = ed.sel.iter().filter_map(|x| if let Obj::Solid(id) = x { Some(*id) } else { None }).collect();
    let solids: Vec<&Solid> = ids.iter().filter_map(|id| ed.map.solid(*id)).filter(|s| !s.has_disp()).collect();
    if solids.is_empty() {
        return Err("Keine Brushes ausgewählt (Displacements werden nicht unterstützt)".into());
    }
    let b = solids.iter().fold(crate::geometry::Aabb::EMPTY, |b, s| b.union(&s.bounds));
    let origin = DVec3::new(b.center().x, b.center().y, b.min.z);
    let reference = smd_triangles(&solids, origin, tex_size, false);
    if reference.is_empty() {
        return Err("Alle Flächen haben Werkzeug-Texturen – nichts zu exportieren".into());
    }
    // Kollision: jeder Brush ist konvex; Werkzeugflächen zählen mit
    let phys = smd_triangles(&solids, origin, &|_| (512.0, 512.0), true);
    let cdm = vec![mat_dir(o)];
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let w = |name: String, text: String| std::fs::write(dir.join(&name), text).map_err(|e| format!("{name}: {e}"));
    w(format!("{}_ref.smd", o.name), smd(&reference))?;
    w(format!("{}_phys.smd", o.name), smd(&phys))?;
    let qcp = dir.join(format!("{}.qc", o.name));
    std::fs::write(&qcp, qc(o, &cdm)).map_err(|e| format!("QC: {e}"))?;
    Ok(Export { dir: dir.to_path_buf(), qc: qcp, origin, solids: ids })
}

/// studiomdl neben vbsp suchen
pub fn find_studiomdl(vbsp: &str) -> Option<PathBuf> {
    let p = Path::new(vbsp).parent()?;
    for name in ["studiomdl.exe", "studiomdl"] {
        for d in [p.to_path_buf(), p.join("win64"), p.parent().map(|x| x.to_path_buf()).unwrap_or_default()] {
            let f = d.join(name);
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// Nach erfolgreichem Kompilieren: Brushes durch ein Prop ersetzen (ein Rückgängig-Schritt)
pub fn replace_with_prop(ed: &mut Editor, ex: &Export, o: &PropOpts) {
    ed.checkpoint();
    for id in &ex.solids {
        ed.map.take_solid(*id);
    }
    let id = ed.map.alloc_id();
    let mut e = crate::map::Entity::new(id, if o.is_static { "prop_static" } else { "prop_physics" });
    e.set("model", format!("models/{}/{}.mdl", o.folder, o.name));
    e.set("origin", crate::map::fmt_vec(ex.origin));
    e.set("angles", "0 0 0");
    ed.map.entities.push(e.into());
    ed.sel = [Obj::Entity(id)].into_iter().collect();
    ed.status = format!("Brushes durch models/{}/{}.mdl ersetzt", o.folder, o.name);
    ed.touch();
}

/// Laufender studiomdl-Vorgang
pub struct PropJob {
    done: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
    export: Export,
    opts: PropOpts,
}

/// Zustand des Dialogs „Auswahl als Modell“
#[derive(Default)]
pub struct PropDlg {
    pub open: bool,
    pub opts: PropOpts,
    pub log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    pub job: Option<PropJob>,
}

/// studiomdl im Hintergrund starten; Ausgabe landet in `log`
pub(crate) fn spawn_studiomdl(exe: PathBuf, game_dir: String, qc: PathBuf, log: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> std::sync::Arc<std::sync::Mutex<Option<bool>>> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let done = std::sync::Arc::new(std::sync::Mutex::new(None));
    let d2 = done.clone();
    std::thread::spawn(move || {
        let say = |s: String| log.lock().unwrap().push(s);
        let mut cmd = Command::new(&exe);
        cmd.args(["-game", &game_dir, "-nop4"]).arg(&qc).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(dir) = qc.parent() {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        say(format!("> {} -game \"{game_dir}\" -nop4 \"{}\"", exe.display(), qc.display()));
        let mut c = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                say(format!("studiomdl konnte nicht gestartet werden: {e}"));
                *d2.lock().unwrap() = Some(false);
                return;
            }
        };
        let stderr = c.stderr.take();
        let log2 = log.clone();
        let et = std::thread::spawn(move || {
            if let Some(e) = stderr {
                for l in BufReader::new(e).lines().map_while(Result::ok) {
                    log2.lock().unwrap().push(l);
                }
            }
        });
        let mut error = false;
        if let Some(o) = c.stdout.take() {
            for l in BufReader::new(o).split(b'\n').map_while(Result::ok) {
                let l = String::from_utf8_lossy(&l).trim_end().to_string();
                if l.to_ascii_uppercase().starts_with("ERROR") {
                    error = true;
                }
                say(l);
            }
        }
        let _ = et.join();
        let ok = c.wait().map(|s| s.success()).unwrap_or(false) && !error;
        *d2.lock().unwrap() = Some(ok);
    });
    done
}

pub(crate) const SURFACEPROPS: &[&str] = &["default", "concrete", "metal", "metal_box", "metalpanel", "wood", "wood_crate", "wood_box", "plastic", "glass", "brick", "rock", "tile", "cardboard", "paper", "rubber", "dirt", "grass", "flesh", "computer"];

impl crate::app::App {
    /// Nach dem Kompilieren aufräumen (auch wenn der Dialog geschlossen ist)
    pub(crate) fn poll_prop(&mut self) {
        let Some(job) = &self.prop.job else { return };
        let Some(ok) = *job.done.lock().unwrap() else {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        let job = self.prop.job.take().unwrap();
        let model = format!("models/{}/{}.mdl", job.opts.folder, job.opts.name);
        if ok {
            self.models.forget(&model);
            self.prop.log.lock().unwrap().push(format!("Fertig: {model}"));
            if job.opts.replace {
                replace_with_prop(&mut self.ed, &job.export, &job.opts);
            } else {
                self.ed.status = format!("Modell kompiliert: {model}");
            }
        } else {
            self.prop.log.lock().unwrap().push("studiomdl ist fehlgeschlagen – Brushes bleiben unverändert.".into());
            self.ed.status = "Modell-Kompilierung fehlgeschlagen (siehe Log)".into();
        }
    }

    /// Ausgewählte Brushes exportieren und (optional) kompilieren
    pub(crate) fn prop_run(&mut self, compile: bool) {
        let o = self.prop.opts.clone();
        let log = self.prop.log.clone();
        log.lock().unwrap().clear();
        let say = |s: String| log.lock().unwrap().push(s);
        let base = self.ed.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(std::env::temp_dir);
        let dir = base.join("amboss_models").join(&o.name);
        let mats = &self.mats;
        let ex = match export(&self.ed, &o, &dir, &|m| mats.info(m).map(|i| (i.width.max(1) as f64, i.height.max(1) as f64)).unwrap_or((512.0, 512.0))) {
            Ok(e) => e,
            Err(e) => return say(e),
        };
        say(format!("SMD/QC geschrieben: {}", ex.dir.display()));
        if !compile {
            return;
        }
        let gd = self.settings.game_dir.trim().to_string();
        if gd.is_empty() {
            return say("Kein Spielverzeichnis eingestellt (Einstellungen → Spiel).".into());
        }
        match write_model_vmts(&self.ed, &o, Path::new(&gd), self.mats.fs().as_deref()) {
            Ok(v) => say(format!("{} Modell-Material(ien) in materials/{}", v.len(), mat_dir(&o))),
            Err(e) => return say(e),
        }
        let Some(exe) = find_studiomdl(&self.settings.vbsp) else {
            return say("studiomdl wurde nicht neben vbsp gefunden – QC liegt bereit und kann manuell kompiliert werden.".into());
        };
        let done = spawn_studiomdl(exe, gd, ex.qc.clone(), log.clone());
        self.prop.job = Some(PropJob { done, export: ex, opts: o });
    }

    pub(crate) fn prop_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme;
        use eframe::egui::{self, RichText, Vec2};
        self.poll_prop();
        if !self.prop.open {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let (mut run, mut only_export, mut close) = (false, false, false);
        let n = self.ed.sel.iter().filter(|x| matches!(x, Obj::Solid(_))).count();
        let busy = self.prop.job.is_some();
        let studiomdl = find_studiomdl(&self.settings.vbsp);
        let log: Vec<String> = self.prop.log.lock().unwrap().clone();
        let d = theme::Dlg::new("Auswahl als Modell (Prop)", 620.0).subtitle(crate::i18n::tr_dyn(&format!("{} ausgewählt", crate::i18n::count_de(n, "Brush", "Brushes")))).footer().no_scroll();
        let o = &mut self.prop.opts;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                ui.add_space(12.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 12 }).show(ui, |ui| {
                    theme::field_row(ui, "Name", |ui| ui.add(egui::TextEdit::singleline(&mut o.name).font(theme::mono(11.5)).desired_width(220.0)));
                    theme::field_row(ui, "Ordner unter models/", |ui| ui.add(egui::TextEdit::singleline(&mut o.folder).font(theme::mono(11.5)).desired_width(220.0)));
                    theme::field_row(ui, "Oberfläche ($surfaceprop)", |ui| {
                        egui::ComboBox::from_id_salt("prop_surf").selected_text(o.surfaceprop.clone()).width(220.0).show_ui(ui, |ui| {
                            for s in SURFACEPROPS {
                                ui.selectable_value(&mut o.surfaceprop, s.to_string(), *s);
                            }
                        })
                    });
                    theme::field_row(ui, "Typ", |ui| {
                        ui.radio_value(&mut o.is_static, true, crate::i18n::tr("prop_static"));
                        ui.radio_value(&mut o.is_static, false, crate::i18n::tr("prop_physics"));
                    });
                    if !o.is_static {
                        theme::field_row(ui, "Masse", |ui| ui.add(crate::theme::dv(egui::DragValue::new(&mut o.mass)).range(1.0..=50000.0).speed(1.0).suffix(" kg")));
                    }
                    theme::field_row(ui, "Ersetzen", |ui| crate::theme::checkbox(ui, &mut o.replace, crate::i18n::tr("Brushes nach dem Kompilieren durch das Prop ersetzen")));
                    o.name = o.name.trim().replace([' ', '\\', '/'], "_");
                    o.folder = o.folder.trim().trim_matches('/').replace('\\', "/");
                    ui.add_space(6.0);
                    let info = match &studiomdl {
                        Some(p) => crate::i18n::tr_dyn(&format!("studiomdl: {}", p.display())),
                        None => crate::i18n::tr("studiomdl nicht gefunden (wird neben vbsp gesucht) – nur Export möglich").to_string(),
                    };
                    ui.label(RichText::new(info).color(th.text3).size(11.5));
                    ui.label(RichText::new(crate::i18n::tr("Werkzeug-Texturen werden nicht ins Modell übernommen, zählen aber zur Kollision.")).color(th.text3).size(11.5));
                    ui.add_space(6.0);
                    egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(8).show(ui, |ui| {
                        ui.set_min_height(160.0);
                        egui::ScrollArea::vertical().max_height(160.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                            for l in &log {
                                let c = if l.to_ascii_uppercase().starts_with("ERROR") || l.contains("fehlgeschlagen") { th.err } else { th.text2 };
                                ui.label(RichText::new(crate::i18n::tr_dyn(l)).font(theme::mono(11.0)).color(c));
                            }
                        });
                    });
                });
            }
            theme::Part::Footer => {
                let ok = n > 0 && !busy && !o.name.is_empty();
                let label = if busy { crate::i18n::tr("Kompiliere…") } else { crate::i18n::tr("Exportieren & kompilieren") };
                if ui.add_enabled(ok && studiomdl.is_some(), egui::Button::new(RichText::new(label).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    run = true;
                }
                if ui.add_enabled(ok, egui::Button::new(crate::i18n::tr("Nur SMD/QC")).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    only_export = true;
                }
                if theme::secondary_button(ui, "Schließen").clicked() {
                    close = true;
                }
            }
            _ => {}
        });
        if run || only_export {
            self.prop_run(run);
        }
        if close {
            open = false;
        }
        self.prop.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Aabb;

    #[test]
    fn export_box() {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.material = "brick/brickwall001a".into();
        ed.create_block(Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 32.0, 16.0)));
        let id = ed.map.world.solids[0].id;
        ed.sel = [Obj::Solid(id)].into_iter().collect();
        let dir = std::env::temp_dir().join(format!("amboss_prop_{}", std::process::id()));
        let o = PropOpts { name: "kiste".into(), ..Default::default() };
        let ex = export(&ed, &o, &dir, &|_| (128.0, 128.0)).unwrap();
        assert_eq!(ex.origin, DVec3::new(32.0, 16.0, 0.0));
        let smd = std::fs::read_to_string(dir.join("kiste_ref.smd")).unwrap();
        // 6 Flächen × 2 Dreiecke
        assert_eq!(smd.matches("brickwall001a").count(), 12);
        let q = std::fs::read_to_string(&ex.qc).unwrap();
        assert!(q.contains("$modelname \"amboss/kiste.mdl\"") && q.contains("$cdmaterials \"models/amboss/kiste/\"") && q.contains("$staticprop"));
        let vmts = write_model_vmts(&ed, &o, &dir, None).unwrap();
        assert_eq!(vmts.len(), 1);
        let v = std::fs::read_to_string(&vmts[0]).unwrap();
        assert!(v.contains("VertexLitGeneric") && v.contains("\"$basetexture\" \"brick/brickwall001a\""), "{v}");
        let mv = model_vmt(Some("LightmappedGeneric { $basetexture \"a/b\" $translucent 1 $surfaceprop metal }"), "x/y");
        assert!(mv.contains("\"$basetexture\" \"a/b\"") && mv.contains("$translucent") && !mv.contains("x/y"), "{mv}");
        replace_with_prop(&mut ed, &ex, &o);
        assert!(ed.map.world.solids.is_empty());
        assert_eq!(ed.map.entities.last().unwrap().get("model"), Some("models/amboss/kiste.mdl"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
