//! Materialsystem: VMT auflösen, VTF im Hintergrund laden, Vorschaubilder & GL-Uploads verwalten.

use crate::gamefs::GameFs;
use crate::keyvalues;
use eframe::egui;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

/// Fertig geladene Textur
pub struct TexData {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pw: u32,
    pub ph: u32,
    pub rgba: Vec<u8>,
    pub translucent: bool,
    /// Deckkraft aus der VMT (1 = deckend)
    pub alpha: f32,
    /// `$additive` (wird aufaddiert, z. B. Partikel, Glühen)
    pub additive: bool,
    /// weitere Bilder (AnimatedTexture), Bilder pro Sekunde, Textur-Matrix, Scrollen
    pub frames: Vec<Vec<u8>>,
    pub anim_fps: Option<f32>,
    pub xform: Option<[f32; 6]>,
    pub scroll: Option<(f32, f32)>,
    /// Zweite Textur von Blend-Materialien (WorldVertexTransition): (Breite, Höhe, RGBA)
    pub rgba2: Option<(u32, u32, Vec<u8>)>,
    pub thumb: (u32, u32, Vec<u8>),
    pub decal_scale: f32,
    pub for_gl: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MatInfo {
    pub width: u32,
    pub height: u32,
    pub translucent: bool,
    /// Deckkraft aus der VMT (1 = deckend)
    pub alpha: f32,
    /// `$additive`
    pub additive: bool,
    /// Blend-Material mit zweiter Textur
    #[allow(dead_code)]
    pub blend: bool,
    pub in_gl: bool,
    /// `$decalscale` aus der VMT (1 = Texturgröße in Einheiten)
    pub decal_scale: f32,
    /// Durchschnittsfarbe der Textur (sRGB) – Oberflächenfarbe für die globale Beleuchtung
    pub avg: [u8; 3],
}

enum State {
    Loading { for_gl: bool },
    Ready(MatInfo),
    Missing,
}

struct Request {
    name: String,
    for_gl: bool,
}

/// Arbeitswarteschlange der Lade-Threads: Vorschaubilder (Browser) vor 3D-Texturen
struct Queue {
    inner: Mutex<(std::collections::VecDeque<Request>, std::collections::VecDeque<Request>, bool)>,
    cv: std::sync::Condvar,
}

impl Queue {
    fn new() -> Arc<Queue> {
        Arc::new(Queue { inner: Mutex::new((Default::default(), Default::default(), false)), cv: std::sync::Condvar::new() })
    }
    fn push(&self, r: Request) {
        let mut q = self.inner.lock().unwrap();
        if r.for_gl {
            q.1.push_back(r);
        } else {
            // zuletzt angefragte Vorschaubilder zuerst (der Nutzer schaut gerade hin)
            q.0.push_front(r);
        }
        self.cv.notify_one();
    }
    fn pop(&self) -> Option<Request> {
        let mut q = self.inner.lock().unwrap();
        loop {
            if q.2 {
                return None;
            }
            if let Some(r) = q.0.pop_front().or_else(|| q.1.pop_front()) {
                return Some(r);
            }
            q = self.cv.wait(q).unwrap();
        }
    }
    fn close(&self) {
        self.inner.lock().unwrap().2 = true;
        self.cv.notify_all();
    }
}

pub struct MaterialSystem {
    fs: Option<Arc<GameFs>>,
    queue: Option<Arc<Queue>>,
    /// Zum Neuzeichnen, sobald ein Lade-Thread fertig ist
    ctx: Option<egui::Context>,
    rx: Option<Receiver<Result<TexData, String>>>,
    states: HashMap<String, State>,
    pub thumbs: HashMap<String, egui::TextureHandle>,
    /// Texturen, die an den 3D-Renderer hochgeladen werden müssen
    pub uploads: Vec<TexData>,
    /// Liste aller Materialien (für den Browser)
    pub list: Vec<String>,
    /// Wird erhöht, sobald neue Texturen bereitstehen (→ 3D-Mesh neu bauen)
    pub generation: u64,
    pending: usize,
    pub messages: Vec<String>,
}

/// Entfernt Plattform-Bedingungen wie [$X360] / [!$PS3] aus VMT-Text
fn strip_conditionals(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            in_str = !in_str;
        }
        if !in_str && c == '[' {
            let rest: String = chars.clone().take(2).collect();
            if rest.starts_with('$') || rest.starts_with("!$") {
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

pub struct VmtInfo {
    pub basetexture: Option<String>,
    pub basetexture2: Option<String>,
    pub translucent: bool,
    /// Deckkraft (1 = deckend). Aus `$alpha`, bzw. geschätzt für Refract/Water/`$additive`.
    pub alpha: f32,
    pub additive: bool,
    /// Ersatzfarbe, wenn es keine `$basetexture` gibt (Refract-Glas, Wasser)
    pub tint: Option<[u8; 3]>,
    /// `$basetexturetransform` als 2×3-Matrix (Zeilen: u, v)
    pub xform: Option<[f32; 6]>,
    /// AnimatedTexture-Proxy: Bilder pro Sekunde
    pub anim_fps: Option<f32>,
    /// TextureScroll-Proxy: (Rate, Winkel in Grad)
    pub scroll: Option<(f32, f32)>,
    /// `$decalscale` (Weltgröße eines Decals/Overlays = Texturgröße × Faktor)
    pub decal_scale: f32,
}

impl VmtInfo {
    fn empty() -> Self {
        Self { basetexture: None, basetexture2: None, translucent: false, alpha: 1.0, additive: false, tint: None, xform: None, anim_fps: None, scroll: None, decal_scale: 1.0 }
    }
}

/// Schalter wie `$translucent` sind Zahlen: "1", "1.0", " 1" usw.
fn kv_num(b: &crate::keyvalues::KvBlock, k: &str) -> Option<f32> {
    b.get(k).and_then(|v| v.trim().trim_matches('"').parse::<f32>().ok())
}

/// Transparenz-Schlüssel eines Blocks übernehmen (Wurzel, Patch-Blöcke, Shader-Fallbacks)
fn apply_vmt_flags(b: &crate::keyvalues::KvBlock, v: &mut VmtInfo) {
    if let Some(t) = b.get("$basetexture") {
        v.basetexture = Some(t.to_string());
    }
    if let Some(t) = b.get("$basetexture2") {
        v.basetexture2 = Some(t.to_string());
    }
    let on = |k: &str| kv_num(b, k).map(|x| x > 0.0);
    for k in ["$translucent", "$alphatest"] {
        if let Some(x) = on(k) {
            v.translucent = x;
        }
    }
    if on("$additive") == Some(true) {
        v.additive = true;
        v.translucent = true;
        v.alpha = v.alpha.min(0.55);
    }
    if let Some(t) = b.get("$basetexturetransform") {
        v.xform = parse_texture_transform(t);
    }
    // Proxies: AnimatedTexture (Bildwechsel) und TextureScroll (laufende Textur)
    for px in b.blocks().filter(|x| x.name.eq_ignore_ascii_case("proxies")) {
        for p in px.blocks() {
            let n = p.name.to_ascii_lowercase();
            if n == "animatedtexture" && p.get("animatedtexturevar").is_none_or(|v| v.eq_ignore_ascii_case("$basetexture")) {
                v.anim_fps = Some(kv_num(p, "animatedtextureframerate").unwrap_or(15.0));
            }
            if n == "texturescroll" && p.get("texturescrollvar").is_none_or(|v| v.eq_ignore_ascii_case("$basetexturetransform")) {
                v.scroll = Some((kv_num(p, "texturescrollrate").unwrap_or(0.0), kv_num(p, "texturescrollangle").unwrap_or(0.0)));
            }
        }
    }
    if let Some(d) = kv_num(b, "$decalscale").filter(|d| *d > 0.0) {
        v.decal_scale = d;
    }
    if let Some(a) = kv_num(b, "$alpha") {
        if a < 0.999 {
            v.translucent = true;
            v.alpha = a.clamp(0.05, 1.0);
        }
    }
}

pub fn parse_vmt(fs: &GameFs, name: &str, depth: u32) -> Option<VmtInfo> {
    let bytes = fs.read(&format!("materials/{name}.vmt"))?;
    let src = strip_conditionals(&String::from_utf8_lossy(&bytes));
    let blocks = keyvalues::parse(&src).ok()?;
    let root = blocks.into_iter().next()?;
    if root.name.eq_ignore_ascii_case("patch") && depth < 4 {
        // patch { include "materials/x.vmt" replace { ... } insert { ... } }
        let include = root.get("include")?;
        let inc = include.trim_start_matches("materials/").trim_end_matches(".vmt").to_string();
        let mut base = parse_vmt(fs, &inc, depth + 1).unwrap_or_else(VmtInfo::empty);
        for b in root.blocks() {
            apply_vmt_flags(b, &mut base);
        }
        return Some(base);
    }
    let mut v = VmtInfo::empty();
    let shader = root.name.to_ascii_lowercase();
    // Shader-Fallbacks wie "LightmappedGeneric_DX9" { ... } enthalten oft die eigentlichen Schalter
    for b in root.blocks() {
        let n = b.name.to_ascii_lowercase();
        if n.contains("dx") || n.contains("hdr") || n.contains("ldr") || n.starts_with(&shader) {
            apply_vmt_flags(b, &mut v);
        }
    }
    apply_vmt_flags(&root, &mut v);
    if shader == "water" {
        v.translucent = true;
        v.alpha = v.alpha.min(0.7);
        v.tint = Some(root.get("$fogcolor").and_then(parse_color).unwrap_or([60, 90, 110]));
    } else if shader == "refract" || shader.starts_with("refract_") {
        // Refract-Glas: im Spiel durchsichtig, Textur ist meist nur eine Normalmap/Tönung
        v.translucent = true;
        v.alpha = v.alpha.min(0.35);
        v.tint = Some(root.get("$refracttint").and_then(parse_color).unwrap_or([200, 215, 225]));
    }
    Some(v)
}

/// Sieht man durch diese Textur hindurch (bzw. blockiert sie keine Sicht)?
/// Tool-Texturen außer nodraw/black/skybox, Wasser, Glas, Gitter usw.
pub fn see_through(fs: Option<&GameFs>, mat: &str) -> bool {
    let m = mat.replace('\\', "/").to_ascii_lowercase();
    if let Some(t) = m.strip_prefix("tools/") {
        return !matches!(t, "toolsnodraw" | "toolsblack" | "toolsskybox" | "toolsskybox2d" | "toolsblack_noportal");
    }
    if ["water", "glass", "grate", "fence", "chainlink"].iter().any(|k| m.contains(k)) {
        return true;
    }
    fs.and_then(|fs| parse_vmt(fs, &m, 0)).is_some_and(|v| v.translucent)
}

/// "center .5 .5 scale 1 1 rotate 0 translate 0 0" → Matrix T(translate)·T(center)·S·R·T(−center)
pub fn parse_texture_transform(s: &str) -> Option<[f32; 6]> {
    let t: Vec<&str> = s.split_whitespace().collect();
    let (mut c, mut sc, mut rot, mut tr) = ([0.5f32, 0.5], [1.0f32, 1.0], 0.0f32, [0.0f32, 0.0]);
    let num = |i: usize| t.get(i).and_then(|v| v.parse::<f32>().ok());
    let mut i = 0;
    while i < t.len() {
        match t[i].to_ascii_lowercase().as_str() {
            "center" => c = [num(i + 1)?, num(i + 2)?],
            "scale" => sc = [num(i + 1)?, num(i + 2)?],
            "rotate" => rot = num(i + 1)?,
            "translate" => tr = [num(i + 1)?, num(i + 2)?],
            _ => {}
        }
        i += 1;
    }
    let (sn, cs) = rot.to_radians().sin_cos();
    // M = S·R: [sx*cos, -sx*sin; sy*sin, sy*cos]
    let (a, b, d, e) = (sc[0] * cs, -sc[0] * sn, sc[1] * sn, sc[1] * cs);
    // uv' = M·(uv − c) + c + t
    let tx = -(a * c[0] + b * c[1]) + c[0] + tr[0];
    let ty = -(d * c[0] + e * c[1]) + c[1] + tr[1];
    let m = [a, b, tx, d, e, ty];
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    (m.iter().zip(identity.iter()).any(|(x, y)| (x - y).abs() > 1e-5)).then_some(m)
}

/// Farbe aus VMT: "[0.5 0.6 1]" (0..1) oder "{128 150 255}" (0..255)
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let t = s.trim();
    let int = t.starts_with('{');
    let n: Vec<f32> = t.trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}').split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if n.len() < 3 {
        return None;
    }
    let f = |x: f32| if int { x.clamp(0.0, 255.0) as u8 } else { (x.clamp(0.0, 1.0) * 255.0) as u8 };
    Some([f(n[0]), f(n[1]), f(n[2])])
}

/// Grundtextur eines Materials als RGBA (kleine Mip-Stufe, z. B. für Vorschaubilder):
/// (Breite, Höhe, Pixel, durchsichtig)
pub fn base_rgba(fs: &GameFs, name: &str, max: u32) -> Option<(u32, u32, Vec<u8>, bool)> {
    let vmt = parse_vmt(fs, name, 0)?;
    let translucent = vmt.translucent || vmt.alpha < 0.99;
    let Some(base) = vmt.basetexture else {
        let c = vmt.tint?;
        return Some((1, 1, vec![c[0], c[1], c[2], 255], true));
    };
    let base = base.replace('\\', "/").to_ascii_lowercase();
    let bytes = fs.read(&format!("materials/{}.vtf", base.trim_end_matches(".vtf")))?;
    let img = crate::vtf::load(&bytes, max).ok()?;
    Some((img.pw, img.ph, img.rgba, translucent))
}

fn load_texture(fs: &GameFs, name: &str, for_gl: bool) -> Result<TexData, String> {
    let vmt = parse_vmt(fs, name, 0).ok_or_else(|| format!("{name}: VMT nicht gefunden"))?;
    if let (None, Some(c)) = (&vmt.basetexture, vmt.tint) {
        // Glas/Wasser ohne Grundtextur: einfarbige, durchsichtige Ersatztextur
        let px: Vec<u8> = (0..16).flat_map(|_| [c[0], c[1], c[2], 255]).collect();
        return Ok(TexData {
            name: name.to_string(),
            width: 128,
            height: 128,
            pw: 4,
            ph: 4,
            rgba: if for_gl { px.clone() } else { Vec::new() },
            translucent: true,
            alpha: vmt.alpha,
            additive: vmt.additive,
            frames: Vec::new(),
            anim_fps: None,
            xform: None,
            scroll: None,
            rgba2: None,
            thumb: (4, 4, px),
            decal_scale: vmt.decal_scale,
            for_gl,
        });
    }
    let base = vmt.basetexture.unwrap_or_else(|| name.to_string());
    let base = base.replace('\\', "/").to_ascii_lowercase();
    let base = base.trim_end_matches(".vtf");
    let bytes = fs.read(&format!("materials/{base}.vtf")).ok_or_else(|| format!("{name}: VTF {base} nicht gefunden"))?;
    let img = crate::vtf::load_opts(&bytes, 256, if for_gl && vmt.anim_fps.is_some() { 32 } else { 1 }).map_err(|e| format!("{name}: {e}"))?;
    let thumb = downscale(&img.rgba, img.pw, img.ph, 96);
    let rgba2 = if for_gl {
        vmt.basetexture2.and_then(|b2| {
            let b2 = b2.replace('\\', "/").to_ascii_lowercase();
            let bytes = fs.read(&format!("materials/{}.vtf", b2.trim_end_matches(".vtf")))?;
            let i2 = crate::vtf::load(&bytes, 256).ok()?;
            Some((i2.pw, i2.ph, i2.rgba))
        })
    } else {
        None
    };
    Ok(TexData {
        name: name.to_string(),
        width: img.width,
        height: img.height,
        pw: img.pw,
        ph: img.ph,
        rgba: if for_gl { img.rgba } else { Vec::new() },
        translucent: vmt.translucent,
        alpha: vmt.alpha,
        additive: vmt.additive,
        frames: if for_gl { img.frames } else { Vec::new() },
        anim_fps: vmt.anim_fps,
        xform: vmt.xform,
        scroll: vmt.scroll,
        rgba2,
        thumb,
        decal_scale: vmt.decal_scale,
        for_gl,
    })
}

/// Einfaches Box-Downsampling für Vorschaubilder
fn downscale(rgba: &[u8], w: u32, h: u32, max: u32) -> (u32, u32, Vec<u8>) {
    let mut step = 1;
    while (w / step).max(h / step) > max {
        step *= 2;
    }
    let (nw, nh) = ((w / step).max(1), (h / step).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            let mut n = 0;
            for sy in 0..step {
                for sx in 0..step {
                    let px = (x * step + sx).min(w - 1);
                    let py = (y * step + sy).min(h - 1);
                    let o = ((py * w + px) * 4) as usize;
                    for k in 0..4 {
                        acc[k] += rgba[o + k] as u32;
                    }
                    n += 1;
                }
            }
            for a in acc {
                out.push((a / n) as u8);
            }
        }
    }
    (nw, nh, out)
}

pub fn normalize(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

impl Default for MaterialSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl MaterialSystem {
    pub fn new() -> Self {
        MaterialSystem {
            fs: None,
            queue: None,
            ctx: None,
            rx: None,
            states: HashMap::new(),
            thumbs: HashMap::new(),
            uploads: Vec::new(),
            list: Vec::new(),
            generation: 0,
            pending: 0,
            messages: Vec::new(),
        }
    }

    pub fn fs(&self) -> Option<Arc<GameFs>> {
        self.fs.clone()
    }

    pub fn has_fs(&self) -> bool {
        self.fs.is_some()
    }

    /// Wie `set_fs`, aber mit bereits erstellter Materialliste (aus einem Hintergrund-Thread)
    pub fn set_fs_prepared(&mut self, fs: GameFs, list: Vec<String>) {
        self.set_fs_inner(fs, Some(list));
    }

    /// Neues Dateisystem setzen (startet Lade-Threads neu)
    #[allow(dead_code)]
    pub fn set_fs(&mut self, fs: GameFs) {
        self.set_fs_inner(fs, None);
    }

    /// Fenster-Kontext setzen: Lade-Threads stoßen dann nach jeder Textur ein Neuzeichnen an
    pub fn set_ctx(&mut self, ctx: egui::Context) {
        self.ctx = Some(ctx);
    }

    fn set_fs_inner(&mut self, fs: GameFs, list: Option<Vec<String>>) {
        self.messages = fs.messages.clone();
        self.states.clear();
        self.thumbs.clear();
        self.uploads.clear();
        self.pending = 0;
        self.generation += 1;
        self.stop_workers();
        if fs.is_empty() {
            self.fs = None;
            self.list.clear();
            return;
        }
        self.list = list.unwrap_or_else(|| fs.list_materials());
        self.start_workers(Arc::new(fs));
    }

    /// Erweitertes Dateisystem übernehmen (z. B. nachdem die Addons eingelesen sind), ohne bereits
    /// geladene Texturen zu verwerfen. Nicht gefundene Materialien werden erneut versucht.
    pub fn update_fs(&mut self, fs: GameFs, list: Vec<String>) {
        if self.fs.is_none() {
            return self.set_fs_inner(fs, Some(list));
        }
        self.messages = fs.messages.clone();
        self.stop_workers();
        let requeue: Vec<(String, bool)> = self.states.iter().filter_map(|(k, v)| if let State::Loading { for_gl } = v { Some((k.clone(), *for_gl)) } else { None }).collect();
        self.states.retain(|_, v| matches!(v, State::Ready(_)));
        self.list = list;
        self.pending = 0;
        self.generation += 1;
        self.start_workers(Arc::new(fs));
        for (name, for_gl) in requeue {
            self.send(&name, for_gl);
        }
    }

    fn stop_workers(&mut self) {
        if let Some(q) = self.queue.take() {
            q.close();
        }
        self.rx = None;
    }

    fn start_workers(&mut self, fs: Arc<GameFs>) {
        let queue = Queue::new();
        let (res_tx, rx) = channel();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 6);
        for _ in 0..threads {
            let (fs, queue, res_tx, ctx) = (fs.clone(), queue.clone(), res_tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                while let Some(req) = queue.pop() {
                    let res = load_texture(&fs, &req.name, req.for_gl).map_err(|e| format!("{}\u{0}{e}", req.name));
                    if res_tx.send(res).is_err() {
                        return;
                    }
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                }
            });
        }
        self.fs = Some(fs);
        self.queue = Some(queue);
        self.rx = Some(rx);
    }

    fn send(&mut self, name: &str, for_gl: bool) {
        if let Some(q) = &self.queue {
            q.push(Request { name: name.to_string(), for_gl });
            self.pending += 1;
            self.states.insert(name.to_string(), State::Loading { for_gl });
            return;
        }
        self.states.insert(name.to_string(), State::Missing);
    }

    /// Für die 3D-Ansicht: Info abfragen und ggf. Laden (inkl. GL-Upload) anstoßen
    /// Steht fest, wie das Material in 3D aussieht (geladen oder endgültig fehlend)?
    pub fn gl_settled(&self, name: &str) -> bool {
        self.fs.is_none() || matches!(self.states.get(&normalize(name)), Some(State::Ready(MatInfo { in_gl: true, .. })) | Some(State::Missing))
    }

    pub fn request_gl(&mut self, name: &str) -> Option<MatInfo> {
        let key = normalize(name);
        match self.states.get_mut(&key) {
            Some(State::Ready(info)) if info.in_gl => Some(*info),
            Some(State::Ready(_)) => {
                self.send(&key, true);
                None
            }
            Some(State::Loading { for_gl }) => {
                *for_gl = true;
                None
            }
            Some(State::Missing) => None,
            None => {
                if self.fs.is_some() {
                    self.send(&key, true);
                }
                None
            }
        }
    }

    /// Für den Browser: nur Vorschaubild
    pub fn thumb(&mut self, name: &str) -> Option<&egui::TextureHandle> {
        let key = normalize(name);
        if !self.states.contains_key(&key) && self.fs.is_some() {
            self.send(&key, false);
        }
        self.thumbs.get(&key)
    }

    pub fn info(&self, name: &str) -> Option<MatInfo> {
        match self.states.get(&normalize(name)) {
            Some(State::Ready(i)) => Some(*i),
            _ => None,
        }
    }

    /// Material neu laden (nach dem Speichern im Material-Editor) und in der Liste anbieten
    pub fn forget(&mut self, name: &str) {
        let k = normalize(name);
        self.states.remove(&k);
        self.thumbs.remove(&k);
        if !self.list.contains(&k) {
            self.list.push(k);
            self.list.sort();
        }
        self.generation += 1;
    }

    pub fn is_missing(&self, name: &str) -> bool {
        matches!(self.states.get(&normalize(name)), Some(State::Missing))
    }

    pub fn is_loading(&self) -> bool {
        self.pending > 0
    }

    /// Ergebnisse der Lade-Threads übernehmen
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let mut results = Vec::new();
        while let Ok(r) = rx.try_recv() {
            results.push(r);
        }
        if results.is_empty() {
            return;
        }
        for r in results {
            self.pending = self.pending.saturating_sub(1);
            match r {
                Ok(mut t) => {
                    let want_gl = matches!(self.states.get(&t.name), Some(State::Loading { for_gl: true }));
                    let (tw, th, td) = std::mem::take(&mut t.thumb);
                    let img = egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &td);
                    let handle = ctx.load_texture(format!("mat:{}", t.name), img, egui::TextureOptions::LINEAR);
                    self.thumbs.insert(t.name.clone(), handle);
                    let in_gl = want_gl && t.for_gl;
                    self.states.insert(t.name.clone(), State::Ready(MatInfo { width: t.width, height: t.height, translucent: t.translucent, alpha: t.alpha, additive: t.additive, blend: t.rgba2.is_some(), in_gl, avg: avg_color(&t.thumb.2), decal_scale: t.decal_scale }));
                    if in_gl {
                        self.uploads.push(t);
                    } else if want_gl {
                        // nachträglich auch für die 3D-Ansicht laden
                        let n = t.name.clone();
                        self.send(&n, true);
                    }
                }
                Err(e) => {
                    let (name, msg) = e.split_once('\u{0}').unwrap_or(("", &e));
                    if !name.is_empty() {
                        self.states.insert(name.to_string(), State::Missing);
                    }
                    if self.messages.len() < 50 {
                        self.messages.push(msg.to_string());
                    }
                }
            }
        }
        self.generation += 1;
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamefs::tests::write_test_vpk;
    use crate::vtf::tests::make_vtf;

    #[test]
    fn vmt_and_vtf_from_vpk() {
        let dir = std::env::temp_dir().join(format!("amboss_mat_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vtf = make_vtf(4, 4, 13, &[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
        let vmt = b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"brick/red\"\n\t\"$envmap\" \"env_cubemap\" [$WIN32]\n\t\"%keywords\" \"x\"\n}\n";
        let patch = b"patch\n{\n\tinclude \"materials/brick/wall.vmt\"\n\treplace { \"$translucent\" \"1\" }\n}\n";
        let p = dir.join("t_dir.vpk");
        let blend = b"\"WorldVertexTransition\"\n{\n\t\"$basetexture\" \"brick/red\"\n\t\"$basetexture2\" \"brick/red\"\n}\n";
        write_test_vpk(&p, &[("materials/brick/wall.vmt", vmt), ("materials/brick/red.vtf", &vtf), ("materials/brick/glass.vmt", patch), ("materials/nature/blend.vmt", blend)]);
        let mut fs = GameFs::default();
        fs.add_vpk(&p);
        let t = load_texture(&fs, "brick/wall", true).unwrap();
        assert_eq!((t.width, t.height), (4, 4));
        assert_eq!(&t.rgba[0..4], &[255, 0, 0, 255]);
        assert!(!t.translucent);
        let g = load_texture(&fs, "brick/glass", false).unwrap();
        assert!(g.translucent);
        assert!(g.rgba.is_empty());
        assert!(load_texture(&fs, "missing/x", false).is_err());
        assert!(t.rgba2.is_none());
        let bl = load_texture(&fs, "nature/blend", true).unwrap();
        assert_eq!(bl.rgba2.as_ref().map(|x| (x.0, x.1)), Some((4, 4)));
        assert_eq!(fs.list_materials(), vec!["brick/glass".to_string(), "brick/wall".to_string(), "nature/blend".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transparency_variants() {
        let dir = std::env::temp_dir().join(format!("amboss_trans_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let vtf = make_vtf(4, 4, 13, &[0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0]);
        let alpha = b"LightmappedGeneric\n{\n$basetexture glass/a\n$alpha 0.4\n}\n";
        let tr = b"\"LightmappedGeneric\"\n{\n\"$basetexture\" \"glass/a\"\n\"$translucent\" \"1.0\"\n}\n";
        let fb = b"\"LightmappedGeneric\"\n{\n\"$basetexture\" \"glass/a\"\n\"LightmappedGeneric_DX9\"\n{\n\"$alphatest\" 1\n}\n}\n";
        let refract = b"\"Refract\"\n{\n\"$normalmap\" \"glass/n\"\n\"$refracttint\" \"[0.5 0.8 1]\"\n}\n";
        let opaque = b"\"LightmappedGeneric\"\n{\n\"$basetexture\" \"glass/a\"\n\"$translucent\" \"0\"\n}\n";
        let p = dir.join("t_dir.vpk");
        write_test_vpk(&p, &[("materials/glass/a.vtf", &vtf), ("materials/glass/alpha.vmt", alpha), ("materials/glass/tr.vmt", tr), ("materials/glass/fb.vmt", fb), ("materials/glass/refract.vmt", refract), ("materials/x/opaque.vmt", opaque)]);
        let mut fs = GameFs::default();
        fs.add_vpk(&p);
        let a = load_texture(&fs, "glass/alpha", true).unwrap();
        assert!(a.translucent && (a.alpha - 0.4).abs() < 1e-4);
        assert!(load_texture(&fs, "glass/tr", true).unwrap().translucent);
        assert!(load_texture(&fs, "glass/fb", true).unwrap().translucent);
        let r = load_texture(&fs, "glass/refract", true).unwrap();
        assert!(r.translucent && r.alpha < 0.5);
        assert_eq!(&r.rgba[0..3], &[127, 204, 255]);
        let o = load_texture(&fs, "x/opaque", true).unwrap();
        assert!(!o.translucent && o.alpha == 1.0);
        assert!(see_through(Some(&fs), "x/refract_dummy") == false);
        assert!(see_through(Some(&fs), "GLASS/REFRACT"));
        assert!(see_through(None, "TOOLS/TOOLSPLAYERCLIP"));
        assert!(!see_through(None, "TOOLS/TOOLSNODRAW"));
        assert!(!see_through(Some(&fs), "x/opaque"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn texture_transform_and_proxies() {
        let m = parse_texture_transform("center .5 .5 scale 2 2 rotate 0 translate 0 0").unwrap();
        assert!((m[0] - 2.0).abs() < 1e-5 && (m[2] + 0.5).abs() < 1e-5 && (m[4] - 2.0).abs() < 1e-5);
        assert!(parse_texture_transform("center .5 .5 scale 1 1 rotate 0 translate 0 0").is_none());
        let r = parse_texture_transform("center 0 0 scale 1 1 rotate 90 translate 0.25 0").unwrap();
        assert!(r[0].abs() < 1e-5 && (r[1] + 1.0).abs() < 1e-5 && (r[2] - 0.25).abs() < 1e-5);
        let src = "\"UnlitGeneric\" { \"$basetexture\" \"x/y\" \"Proxies\" { \"AnimatedTexture\" { \"animatedtexturevar\" \"$basetexture\" \"animatedtextureframenumvar\" \"$frame\" \"animatedtextureframerate\" \"8\" } \"TextureScroll\" { \"texturescrollvar\" \"$basetexturetransform\" \"texturescrollrate\" \"0.5\" \"texturescrollangle\" \"90\" } } }";
        let root = keyvalues::parse(src).unwrap().into_iter().next().unwrap();
        let mut v = VmtInfo::empty();
        apply_vmt_flags(&root, &mut v);
        assert_eq!(v.anim_fps, Some(8.0));
        assert_eq!(v.scroll, Some((0.5, 90.0)));
    }

    #[test]
    fn conditionals() {
        assert_eq!(strip_conditionals("\"a\" \"b\" [$X360]\n\"c\" \"[x]\""), "\"a\" \"b\" \n\"c\" \"[x]\"");
    }
}

/// Mittlere Farbe eines RGBA-Bildes
fn avg_color(px: &[u8]) -> [u8; 3] {
    let n = (px.len() / 4).max(1) as u64;
    let mut s = [0u64; 3];
    for p in px.chunks(4) {
        for c in 0..3 {
            s[c] += p[c] as u64;
        }
    }
    if px.is_empty() {
        return [160, 160, 160];
    }
    [0, 1, 2].map(|c| (s[c] / n) as u8)
}
