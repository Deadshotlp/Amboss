//! Freie Textur-Bibliotheken (Poly Haven, ambientCG; beide CC0) durchsuchen, PBR-Sets herunterladen
//! und als Source-Material (VMT + VTF) in den Spielordner schreiben.

use crate::app::App;
use crate::i18n::tr;
use crate::tex_tools::PbrMap;
use crate::vtf_write::{Rgba, VtfOpts};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Ordner unter materials/ für heruntergeladene Materialien
pub const FOLDER: &str = "amboss/lib";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    PolyHaven,
    AmbientCg,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::PolyHaven => "Poly Haven",
            Source::AmbientCg => "ambientCG",
        }
    }
    pub fn page(self, id: &str) -> String {
        match self {
            Source::PolyHaven => format!("https://polyhaven.com/a/{id}"),
            Source::AmbientCg => format!("https://ambientcg.com/a/{id}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub source: Source,
    pub id: String,
    pub name: String,
    /// Schlagwörter und Kategorien (klein geschrieben, für die Suche und $surfaceprop)
    pub tags: Vec<String>,
    pub thumb: String,
    pub downloads: u64,
}

impl Asset {
    fn matches(&self, words: &[String]) -> bool {
        words.iter().all(|w| self.id.to_ascii_lowercase().contains(w) || self.name.to_ascii_lowercase().contains(w) || self.tags.iter().any(|t| t.contains(w.as_str())))
    }
}

// ------------------------------------------------------------------ API-Antworten lesen

/// Poly Haven: GET https://api.polyhaven.com/assets?t=textures (Objekt id → Daten), beliebteste zuerst
pub fn parse_polyhaven_list(json: &str) -> Result<Vec<Asset>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Poly Haven: {e}"))?;
    let obj = v.as_object().ok_or("Poly Haven: unerwartete Antwort")?;
    let mut out: Vec<Asset> = obj
        .iter()
        .map(|(id, a)| {
            let strs = |k: &str| a.get(k).and_then(|x| x.as_array()).map(|l| l.iter().filter_map(|t| t.as_str().map(|s| s.to_ascii_lowercase())).collect::<Vec<_>>()).unwrap_or_default();
            let mut tags = strs("tags");
            tags.extend(strs("categories"));
            Asset {
                source: Source::PolyHaven,
                id: id.clone(),
                name: a.get("name").and_then(|x| x.as_str()).unwrap_or(id).to_string(),
                tags,
                // eigene Größe und ausdrücklich PNG (sonst liefert das CDN teils WebP)
                thumb: format!("https://cdn.polyhaven.com/asset_img/thumbs/{id}.png?width=128&height=128&format=png"),
                downloads: a.get("download_count").and_then(|x| x.as_u64()).unwrap_or(0),
            }
        })
        .collect();
    out.sort_by(|a, b| b.downloads.cmp(&a.downloads).then(a.id.cmp(&b.id)));
    Ok(out)
}

/// Poly Haven: GET https://api.polyhaven.com/files/<id> → Bild-URLs je Rolle in der gewünschten Auflösung
/// (sonst der nächstkleineren, notfalls der nächstgrößeren)
pub fn polyhaven_maps(json: &str, res: &str) -> Result<Vec<(PbrMap, String)>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Poly Haven: {e}"))?;
    let order = ["1k", "2k", "4k", "8k"];
    let want = order.iter().position(|r| *r == res).unwrap_or(0);
    let mut prefs: Vec<&str> = order[..=want].iter().rev().copied().collect();
    prefs.extend(&order[want + 1..]);
    let mut out = Vec::new();
    for (key, map) in [("Diffuse", PbrMap::Albedo), ("nor_dx", PbrMap::NormalDx), ("nor_gl", PbrMap::NormalGl), ("Rough", PbrMap::Roughness), ("Metal", PbrMap::Metallic), ("AO", PbrMap::Ao), ("Displacement", PbrMap::Height)] {
        let Some(m) = v.get(key) else { continue };
        let url = prefs.iter().find_map(|r| {
            let f = m.get(*r)?;
            ["jpg", "png"].iter().find_map(|fmt| f.get(*fmt)?.get("url")?.as_str().map(|s| s.to_string()))
        });
        if let Some(u) = url {
            out.push((map, u));
        }
    }
    // DirectX-Normalmap reicht; die OpenGL-Variante nur, wenn es keine gibt
    if out.iter().any(|m| m.0 == PbrMap::NormalDx) {
        out.retain(|m| m.0 != PbrMap::NormalGl);
    }
    if !out.iter().any(|m| m.0 == PbrMap::Albedo) {
        return Err("Poly Haven: keine Grundfarbe für dieses Material".into());
    }
    Ok(out)
}

/// ambientCG: GET https://ambientcg.com/api/v2/full_json?type=Material&q=…&include=imageData → (Assets, Gesamtzahl)
pub fn parse_ambientcg_list(json: &str) -> Result<(Vec<Asset>, usize), String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("ambientCG: {e}"))?;
    let list = v.get("foundAssets").and_then(|x| x.as_array()).ok_or("ambientCG: unerwartete Antwort")?;
    let total = v.get("numberOfResults").and_then(|x| x.as_u64()).unwrap_or(list.len() as u64) as usize;
    let out = list
        .iter()
        .filter_map(|a| {
            let id = a.get("assetId")?.as_str()?.to_string();
            let mut tags: Vec<String> = a.get("tags").and_then(|x| x.as_array()).map(|l| l.iter().filter_map(|t| t.as_str().map(|s| s.to_ascii_lowercase())).collect()).unwrap_or_default();
            if let Some(c) = a.get("displayCategory").and_then(|x| x.as_str()) {
                tags.push(c.to_ascii_lowercase());
            }
            let thumb = a.get("previewImage").and_then(|p| p.get("128-PNG").or_else(|| p.get("256-PNG"))).and_then(|x| x.as_str()).map(|s| s.to_string()).unwrap_or_default();
            Some(Asset {
                source: Source::AmbientCg,
                name: a.get("displayName").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).unwrap_or(&id).to_string(),
                id,
                tags,
                thumb,
                downloads: a.get("downloadCount").and_then(|x| x.as_u64()).unwrap_or(0),
            })
        })
        .collect();
    Ok((out, total))
}

pub fn ambientcg_url(query: &str, offset: usize) -> String {
    let q: String = query.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '+' }).collect();
    format!("https://ambientcg.com/api/v2/full_json?type=Material&sort=Popular&limit=60&offset={offset}&include=imageData,tagData,displayData&q={q}")
}

pub fn ambientcg_zip(id: &str, res: &str) -> String {
    format!("https://ambientcg.com/get?file={id}_{}-JPG.zip", res.to_ascii_uppercase())
}

/// Bilder aus einem ambientCG-ZIP ihren Rollen zuordnen (Color, NormalDX/GL, Roughness, Metalness, AO, Displacement)
pub fn ambientcg_maps(zip: &[u8]) -> Vec<(PbrMap, Vec<u8>)> {
    let mut out: Vec<(PbrMap, Vec<u8>)> = Vec::new();
    for name in crate::update::zip_names(zip) {
        let low = name.to_ascii_lowercase();
        if !(low.ends_with(".jpg") || low.ends_with(".jpeg") || low.ends_with(".png")) {
            continue;
        }
        // Vorschaubild (nur die ID) und Deckkraft gehören nicht zum Material
        if low.ends_with("_opacity.jpg") || !low.contains('_') {
            continue;
        }
        let Some(k) = crate::tex_tools::classify(&name) else { continue };
        if out.iter().any(|m| m.0 == k) {
            continue;
        }
        if let Ok(b) = crate::update::zip_extract(zip, &name) {
            out.push((k, b));
        }
    }
    if out.iter().any(|m| m.0 == PbrMap::NormalDx) {
        out.retain(|m| m.0 != PbrMap::NormalGl);
    }
    out
}

/// passender $surfaceprop aus Schlagwörtern
pub fn surfaceprop(tags: &[String]) -> &'static str {
    let has = |w: &[&str]| tags.iter().any(|t| w.iter().any(|x| t.contains(x)));
    if has(&["metal", "rust", "iron", "steel", "corrugated"]) {
        "metal"
    } else if has(&["wood", "plank", "bark"]) {
        "wood"
    } else if has(&["brick"]) {
        "brick"
    } else if has(&["tile"]) {
        "tile"
    } else if has(&["grass"]) {
        "grass"
    } else if has(&["gravel"]) {
        "gravel"
    } else if has(&["sand"]) {
        "sand"
    } else if has(&["snow"]) {
        "snow"
    } else if has(&["mud", "dirt", "soil", "ground", "forest"]) {
        "dirt"
    } else if has(&["rock", "stone", "cliff", "cobble"]) {
        "rock"
    } else if has(&["fabric", "carpet", "cloth", "leather"]) {
        "carpet"
    } else if has(&["plaster", "paint"]) {
        "plaster"
    } else {
        "concrete"
    }
}

pub fn material_name(a: &Asset) -> String {
    format!("{FOLDER}/{}", crate::decal_gen::clean_name(&a.id))
}

/// Fertiges Material: (Pfad unter materials/ ohne Endung, Dateiinhalt)
pub struct Built {
    pub material: String,
    pub files: Vec<(String, Vec<u8>)>,
    pub maps: Vec<PbrMap>,
}

/// PBR-Bilder → VMT + VTF (Grundfarbe, Normalmap mit Glanzmaske)
pub fn build(a: &Asset, images: Vec<(PbrMap, Rgba)>, max_size: u32) -> Result<Built, String> {
    let maps: Vec<PbrMap> = images.iter().map(|m| m.0).collect();
    let r = crate::tex_tools::convert_pbr(&images, false)?;
    let material = material_name(a);
    let mut files = vec![(format!("{material}.vtf"), crate::vtf_write::encode(&r.base, VtfOpts { max_size, ..Default::default() }))];
    let mut vmt = format!("\"LightmappedGeneric\"\n{{\n\t\"$basetexture\" \"{material}\"\n");
    if let Some(n) = &r.normal {
        files.push((format!("{material}_normal.vtf"), crate::vtf_write::encode(n, VtfOpts { normal: true, max_size, ..Default::default() })));
        vmt.push_str(&format!("\t\"$bumpmap\" \"{material}_normal\"\n"));
    }
    vmt.push_str(&format!("\t\"$surfaceprop\" \"{}\"\n", surfaceprop(&a.tags)));
    for (k, v) in &r.params {
        vmt.push_str(&format!("\t\"{k}\" \"{v}\"\n"));
    }
    vmt.push_str(&format!("\t// {} – {} (CC0) {}\n}}\n", a.name, a.source.label(), a.source.page(&a.id)));
    files.push((format!("{material}.vmt"), vmt.into_bytes()));
    Ok(Built { material, files, maps })
}

/// Herunterladen und umwandeln (läuft im Hintergrund)
fn fetch(a: &Asset, res: &str, max_size: u32, progress: &dyn Fn(String)) -> Result<Built, String> {
    let get = |url: &str| crate::update::http_get(url, 300);
    let raw: Vec<(PbrMap, Vec<u8>, String)> = match a.source {
        Source::PolyHaven => {
            let files = get(&format!("https://api.polyhaven.com/files/{}", a.id))?;
            let urls = polyhaven_maps(&String::from_utf8_lossy(&files), res)?;
            let n = urls.len();
            let mut out = Vec::new();
            for (k, (m, u)) in urls.into_iter().enumerate() {
                progress(crate::i18n::tr_dyn(&format!("Lade Bild {} von {}…", k + 1, n)));
                let label = u.rsplit('/').next().unwrap_or("bild").to_string();
                out.push((m, get(&u)?, label));
            }
            out
        }
        Source::AmbientCg => {
            progress(tr("Lade Paket…").to_string());
            let zip = get(&ambientcg_zip(&a.id, res))?;
            ambientcg_maps(&zip).into_iter().map(|(m, b)| (m, b, format!("{m:?}"))).collect()
        }
    };
    progress(tr("Wandle um…").to_string());
    let mut images = Vec::new();
    for (m, b, label) in raw {
        images.push((m, crate::model_import::load_image(&crate::model_import::Image::Bytes(b, label))?));
    }
    build(a, images, max_size)
}

// ------------------------------------------------------------------ Fenster

enum Msg {
    List(Source, String, Result<(Vec<Asset>, usize), String>),
    Thumb(String, Option<Rgba>),
    Progress(String),
    Done(Result<Built, String>),
}

pub struct TexLib {
    pub open: bool,
    pub(crate) source: Source,
    query: String,
    /// Suchbegriff der zuletzt angefragten ambientCG-Liste
    acg_query: Option<String>,
    res: &'static str,
    lists: HashMap<Source, Vec<Asset>>,
    acg_total: usize,
    loading: Option<Source>,
    selected: Option<Asset>,
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    thumb_queue: Arc<Mutex<VecDeque<String>>>,
    thumb_workers: usize,
    busy: bool,
    status: String,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Default for TexLib {
    fn default() -> Self {
        let (tx, rx) = channel();
        TexLib {
            open: false,
            source: Source::PolyHaven,
            query: String::new(),
            acg_query: None,
            res: "1k",
            lists: HashMap::new(),
            acg_total: 0,
            loading: None,
            selected: None,
            thumbs: HashMap::new(),
            thumb_queue: Arc::new(Mutex::new(VecDeque::new())),
            thumb_workers: 0,
            busy: false,
            status: String::new(),
            tx,
            rx,
        }
    }
}

impl TexLib {
    fn load_list(&mut self, ctx: &egui::Context, more: bool) {
        if self.loading.is_some() {
            return;
        }
        let src = self.source;
        let tx = self.tx.clone();
        let c = ctx.clone();
        let q = self.query.trim().to_string();
        let offset = if more { self.lists.get(&src).map(|l| l.len()).unwrap_or(0) } else { 0 };
        if src == Source::AmbientCg {
            self.acg_query = Some(q.clone());
        }
        self.loading = Some(src);
        std::thread::spawn(move || {
            let r = match src {
                Source::PolyHaven => crate::update::http_get("https://api.polyhaven.com/assets?t=textures", 60).and_then(|b| parse_polyhaven_list(&String::from_utf8_lossy(&b))).map(|l| {
                    let n = l.len();
                    (l, n)
                }),
                Source::AmbientCg => crate::update::http_get(&ambientcg_url(&q, offset), 60).and_then(|b| parse_ambientcg_list(&String::from_utf8_lossy(&b))),
            };
            let _ = tx.send(Msg::List(src, if offset > 0 { "+".into() } else { String::new() }, r));
            c.request_repaint();
        });
    }

    fn want_thumb(&mut self, ctx: &egui::Context, url: &str) {
        if url.is_empty() || self.thumbs.contains_key(url) {
            return;
        }
        self.thumbs.insert(url.to_string(), None);
        self.thumb_queue.lock().unwrap().push_back(url.to_string());
        // vier Arbeiter laden die Vorschaubilder (die sichtbaren zuerst angefragt)
        while self.thumb_workers < 4 {
            self.thumb_workers += 1;
            let q = self.thumb_queue.clone();
            let tx = self.tx.clone();
            let c = ctx.clone();
            std::thread::spawn(move || loop {
                let Some(u) = q.lock().unwrap().pop_front() else {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    continue;
                };
                let img = crate::update::http_get(&u, 30).ok().and_then(|b| crate::model_import::load_image(&crate::model_import::Image::Bytes(b, u.clone())).ok());
                if tx.send(Msg::Thumb(u, img)).is_err() {
                    return;
                }
                c.request_repaint();
            });
        }
    }
}

impl App {
    pub(crate) fn open_texlib(&mut self) {
        self.texlib.open = true;
    }

    fn texlib_poll(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.texlib.rx.try_recv() {
            let t = &mut self.texlib;
            match m {
                Msg::List(src, more, r) => {
                    t.loading = None;
                    match r {
                        Ok((list, total)) => {
                            if src == Source::AmbientCg {
                                t.acg_total = total;
                            }
                            let e = t.lists.entry(src).or_default();
                            if more.is_empty() {
                                *e = list;
                            } else {
                                e.extend(list);
                            }
                            t.status.clear();
                        }
                        Err(e) => t.status = crate::i18n::tr_dyn(&format!("Liste konnte nicht geladen werden: {e}")),
                    }
                }
                Msg::Thumb(u, img) => {
                    let h = img.map(|i| {
                        let i = if i.w > 160 { crate::vtf_write::resize(&i, 128, (128 * i.h / i.w.max(1)).max(1)) } else { i };
                        ctx.load_texture(format!("texlib:{u}"), egui::ColorImage::from_rgba_unmultiplied([i.w as usize, i.h as usize], &i.px), egui::TextureOptions::LINEAR)
                    });
                    t.thumbs.insert(u, h);
                }
                Msg::Progress(s) => t.status = s,
                Msg::Done(r) => {
                    t.busy = false;
                    match r {
                        Ok(b) => match self.texlib_write(&b) {
                            Ok(()) => {
                                let maps = b.maps.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>().join(", ");
                                let msg = crate::i18n::tr_dyn(&format!("Material {} gespeichert ({}) – ist jetzt die aktive Textur", b.material, maps));
                                self.ed.status = msg.clone();
                                self.texlib.status = msg;
                            }
                            Err(e) => self.texlib.status = e,
                        },
                        Err(e) => t.status = crate::i18n::tr_dyn(&format!("Fehlgeschlagen: {e}")),
                    }
                }
            }
        }
    }

    fn texlib_write(&mut self, b: &Built) -> Result<(), String> {
        if self.settings.game_dir.is_empty() {
            return Err(tr("Kein Spielordner eingestellt (Einstellungen → Spiel & Pfade).").into());
        }
        let root = std::path::Path::new(&self.settings.game_dir).join("materials");
        for (name, data) in &b.files {
            let p = root.join(name);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        self.mats.forget(&b.material);
        self.ed.material = b.material.clone();
        Ok(())
    }

    fn texlib_download(&mut self, ctx: &egui::Context) {
        let Some(a) = self.texlib.selected.clone() else { return };
        if self.settings.game_dir.is_empty() {
            self.texlib.status = tr("Kein Spielordner eingestellt (Einstellungen → Spiel & Pfade).").into();
            return;
        }
        let t = &mut self.texlib;
        t.busy = true;
        t.status = tr("Lade…").into();
        let res = t.res;
        let tx = t.tx.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let c2 = c.clone();
            let progress = move |s: String| {
                let _ = tx2.send(Msg::Progress(s));
                c2.request_repaint();
            };
            let r = fetch(&a, res, 2048, &progress);
            let _ = tx.send(Msg::Done(r));
            c.request_repaint();
        });
    }

    pub(crate) fn texlib_window(&mut self, ctx: &egui::Context) {
        if !self.texlib.open {
            return;
        }
        self.texlib_poll(ctx);
        let src = self.texlib.source;
        let need = match src {
            Source::PolyHaven => !self.texlib.lists.contains_key(&src),
            Source::AmbientCg => self.texlib.acg_query.is_none(),
        };
        if need && self.texlib.loading.is_none() {
            self.texlib.load_list(ctx, false);
        }
        let th = crate::theme::t();
        let words: Vec<String> = self.texlib.query.to_ascii_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        // Poly Haven: ganze Liste, hier gefiltert; ambientCG: Suche auf dem Server
        let items: Vec<Asset> = match src {
            Source::PolyHaven => self.texlib.lists.get(&src).map(|l| l.iter().filter(|a| a.matches(&words)).cloned().collect()).unwrap_or_default(),
            Source::AmbientCg => self.texlib.lists.get(&src).cloned().unwrap_or_default(),
        };
        let mut open = true;
        let mut download = false;
        let mut search = false;
        let mut more = false;
        let total = if src == Source::AmbientCg { self.texlib.acg_total } else { items.len() };
        let d = crate::theme::Dlg::new(tr("Textur-Bibliothek"), 820.0).subtitle(crate::i18n::count(total, "Material", "Materialien")).height(600.0).footer().header_extra().no_scroll();
        crate::theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            crate::theme::Part::Header => {
                let r = ui.add(egui::TextEdit::singleline(&mut self.texlib.query).hint_text(crate::theme::hint(tr("Suchen, z. B. brick, wood floor"))).desired_width(220.0));
                if src == Source::AmbientCg && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    search = true;
                }
                ui.add_space(8.0);
                for s in [Source::AmbientCg, Source::PolyHaven] {
                    if ui.selectable_label(self.texlib.source == s, s.label()).clicked() && self.texlib.source != s {
                        self.texlib.source = s;
                        self.texlib.selected = None;
                    }
                }
            }
            crate::theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    if items.is_empty() {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            let txt = if self.texlib.loading.is_some() { tr("Lade Liste…") } else if !self.texlib.status.is_empty() { self.texlib.status.as_str() } else { tr("Nichts gefunden.") };
                            ui.label(RichText::new(txt).color(th.text3));
                        });
                        return;
                    }
                    let cols = 6;
                    let gap = 8.0;
                    let cw = ((ui.available_width() - gap * (cols as f32 - 1.0) - 6.0) / cols as f32).floor();
                    let ch = cw + 34.0;
                    let rows = items.len().div_ceil(cols);
                    let show_more = src == Source::AmbientCg && items.len() < self.texlib.acg_total;
                    egui::ScrollArea::vertical().id_salt("texlib_grid").auto_shrink([false, false]).max_height(ui.available_height() - 4.0).show_rows(ui, ch + gap, rows + show_more as usize, |ui, range| {
                        for row in range {
                            if row == rows {
                                ui.vertical_centered(|ui| {
                                    if ui.add_enabled(self.texlib.loading.is_none(), egui::Button::new(tr("Mehr laden"))).clicked() {
                                        more = true;
                                    }
                                });
                                continue;
                            }
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for c in 0..cols {
                                    let Some(a) = items.get(row * cols + c) else { break };
                                    self.texlib.want_thumb(ui.ctx(), &a.thumb);
                                    let (r, resp) = ui.allocate_exact_size(Vec2::new(cw, ch), Sense::click());
                                    let sel = self.texlib.selected.as_ref().is_some_and(|s| s.id == a.id && s.source == a.source);
                                    ui.painter().rect_filled(r, 8.0, if sel { th.accent_soft } else if resp.hovered() { th.hover } else { th.raised });
                                    let img_r = egui::Rect::from_min_size(r.min + Vec2::splat(6.0), Vec2::splat(cw - 12.0));
                                    match self.texlib.thumbs.get(&a.thumb).and_then(|t| t.as_ref()) {
                                        Some(t) => {
                                            egui::Image::new((t.id(), img_r.size())).corner_radius(6).paint_at(ui, img_r);
                                        }
                                        None => {
                                            ui.painter().rect_filled(img_r, 6.0, crate::theme::alpha(Color32::GRAY, 0.15));
                                        }
                                    }
                                    let name = ui.painter().layout(a.name.clone(), crate::theme::sans(11.0), if sel { th.accent_text } else { th.text }, cw - 10.0);
                                    ui.painter().with_clip_rect(r).galley(egui::pos2(r.left() + 6.0, img_r.bottom() + 5.0), name, th.text);
                                    if sel {
                                        ui.painter().rect_stroke(r, 8.0, egui::Stroke::new(2.0f32, th.accent), egui::StrokeKind::Inside);
                                    }
                                    let resp = resp.on_hover_text(format!("{}\n{}", a.id, a.tags.join(", ")));
                                    if resp.clicked() {
                                        self.texlib.selected = Some(a.clone());
                                    }
                                    if resp.double_clicked() && !self.texlib.busy {
                                        download = true;
                                    }
                                }
                            });
                            ui.add_space(gap);
                        }
                    });
                });
            }
            crate::theme::Part::Footer => {
                let can = self.texlib.selected.is_some() && !self.texlib.busy;
                if ui.add_enabled(can, egui::Button::new(RichText::new(tr("Herunterladen und einbinden")).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(egui::vec2(0.0, 32.0))).clicked() {
                    download = true;
                }
                egui::ComboBox::from_id_salt("texlib_res").width(70.0).selected_text(self.texlib.res.to_ascii_uppercase()).show_ui(ui, |ui| {
                    for r in ["1k", "2k"] {
                        ui.selectable_value(&mut self.texlib.res, r, r.to_ascii_uppercase());
                    }
                });
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if self.texlib.busy {
                        ui.spinner();
                    }
                    let txt = if !self.texlib.status.is_empty() {
                        self.texlib.status.clone()
                    } else if let Some(a) = &self.texlib.selected {
                        format!("{} → materials/{}", a.name, material_name(a))
                    } else {
                        tr("Frei verwendbar (CC0). Material anklicken, dann herunterladen – es landet als VMT/VTF unter materials/amboss/lib.").to_string()
                    };
                    ui.add(egui::Label::new(RichText::new(txt).font(crate::theme::mono(11.0)).color(th.text3)).truncate());
                });
            }
        });
        if search {
            self.texlib.load_list(ctx, false);
        }
        if more {
            self.texlib.load_list(ctx, true);
        }
        if download {
            self.texlib_download(ctx);
        }
        if !open {
            self.texlib.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyhaven_parsing() {
        let list = r#"{"b_tex":{"name":"B Tex","tags":["Brick"],"categories":["wall"],"download_count":5},"a_tex":{"name":"A Tex","tags":[],"categories":["metal"],"download_count":50}}"#;
        let l = parse_polyhaven_list(list).unwrap();
        assert_eq!(l[0].id, "a_tex");
        assert_eq!(l[1].tags, vec!["brick", "wall"]);
        assert!(l[0].thumb.contains("a_tex.png"));
        assert!(l[1].matches(&["bri".into()]));
        assert!(!l[1].matches(&["wood".into()]));
        let files = r#"{"Diffuse":{"2k":{"jpg":{"url":"d2"}},"1k":{"jpg":{"url":"d1"},"png":{"url":"d1p"}}},
            "nor_gl":{"1k":{"jpg":{"url":"ngl"}}},"nor_dx":{"1k":{"jpg":{"url":"ndx"}}},
            "Rough":{"2k":{"png":{"url":"r2"}}},"arm":{"1k":{"jpg":{"url":"x"}}}}"#;
        let m = polyhaven_maps(files, "1k").unwrap();
        assert_eq!(m, vec![(PbrMap::Albedo, "d1".into()), (PbrMap::NormalDx, "ndx".into()), (PbrMap::Roughness, "r2".into())]);
        let m = polyhaven_maps(files, "2k").unwrap();
        assert_eq!(m[0].1, "d2");
        assert!(polyhaven_maps(r#"{"Rough":{}}"#, "1k").is_err());
    }

    #[test]
    fn ambientcg_parsing() {
        let j = r#"{"foundAssets":[{"assetId":"Bricks059","displayName":"Bricks 059","tags":["Bricks","red"],"displayCategory":"Bricks","downloadCount":7,
            "previewImage":{"64-PNG":"s","128-PNG":"https://x/128/Bricks059.png"}}],"numberOfResults":146}"#;
        let (l, total) = parse_ambientcg_list(j).unwrap();
        assert_eq!(total, 146);
        assert_eq!(l[0].name, "Bricks 059");
        assert_eq!(l[0].thumb, "https://x/128/Bricks059.png");
        assert_eq!(surfaceprop(&l[0].tags), "brick");
        assert_eq!(ambientcg_zip("Bricks059", "1k"), "https://ambientcg.com/get?file=Bricks059_1K-JPG.zip");
        assert!(ambientcg_url("wood floor", 60).contains("q=wood+floor") && ambientcg_url("", 60).contains("offset=60"));
    }

    /// Echte Downloads (Netz nötig): cargo test texlib::tests::live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live() {
        let ph = parse_polyhaven_list(&String::from_utf8_lossy(&crate::update::http_get("https://api.polyhaven.com/assets?t=textures", 60).unwrap())).unwrap();
        println!("Poly Haven: {} Texturen, beliebteste {}", ph.len(), ph[0].id);
        let (acg, total) = parse_ambientcg_list(&String::from_utf8_lossy(&crate::update::http_get(&ambientcg_url("bricks", 0), 60).unwrap())).unwrap();
        println!("ambientCG: {} von {total}, erstes {}", acg.len(), acg[0].id);
        assert!(!acg[0].thumb.is_empty());
        let t = crate::update::http_get(&acg[0].thumb, 30).unwrap();
        let img = crate::model_import::load_image(&crate::model_import::Image::Bytes(t, "thumb".into())).unwrap();
        println!("Vorschau {} × {}", img.w, img.h);
        let out = std::env::var("AMBOSS_TEXLIB_OUT").ok();
        for a in [ph.iter().find(|a| a.id == "red_brick").unwrap_or(&ph[0]), &acg[0]] {
            let b = fetch(a, "1k", 2048, &|s| println!("  {s}")).unwrap();
            println!("{} → {} {:?} {}", a.id, b.material, b.maps, String::from_utf8_lossy(&b.files.last().unwrap().1));
            if let Some(o) = &out {
                for (n, d) in &b.files {
                    let p = std::path::Path::new(o).join("materials").join(n);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, d).unwrap();
                }
            }
        }
    }

    #[test]
    fn builds_material() {
        let a = Asset { source: Source::PolyHaven, id: "Rusty Metal_02".into(), name: "Rusty Metal 02".into(), tags: vec!["metal".into()], thumb: String::new(), downloads: 0 };
        let solid = |v: u8| Rgba { w: 8, h: 8, px: [v, v, v, 255].repeat(64) };
        let b = build(&a, vec![(PbrMap::Albedo, solid(200)), (PbrMap::Roughness, solid(60)), (PbrMap::NormalDx, Rgba { w: 8, h: 8, px: [128, 128, 255, 255].repeat(64) })], 2048).unwrap();
        assert_eq!(b.material, "amboss/lib/rusty_metal_02");
        let names: Vec<&str> = b.files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, vec!["amboss/lib/rusty_metal_02.vtf", "amboss/lib/rusty_metal_02_normal.vtf", "amboss/lib/rusty_metal_02.vmt"]);
        let vmt = String::from_utf8(b.files[2].1.clone()).unwrap();
        assert!(vmt.contains("\"$bumpmap\" \"amboss/lib/rusty_metal_02_normal\""));
        assert!(vmt.contains("\"$surfaceprop\" \"metal\""));
        assert!(vmt.contains("$envmap"));
        assert!(b.files[0].1.starts_with(b"VTF\0"));
    }
}
