//! Kompilieren (VBSP / VVIS / VRAD …), Spielstart, Spielprofile und Einstellungen.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisMode {
    Normal,
    Fast,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadMode {
    Normal,
    Fast,
    Final,
    Off,
}

/// Ein Schritt im Expertenmodus.
/// Programme: Pfad oder $vbsp/$vvis/$vrad/$game, Spezialbefehle: $copy (Quelle Ziel), $gamewait (Spiel starten und warten)
/// Variablen in Argumenten: $vmf (Pfad ohne Endung), $path (Ordner), $file (Map-Name), $gamedir, $bspdir
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub enabled: bool,
    pub name: String,
    pub exe: String,
    pub args: String,
}

/// Gespeicherte Fensteraufteilung
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Layout {
    /// Teilung der vier Ansichten (Anteil links bzw. oben)
    pub split: [f32; 2],
    /// Breite des rechten Panels
    pub right_width: f32,
    pub maximized: Option<usize>,
    /// Achsen der drei 2D-Ansichten
    pub axes: [crate::geometry::Axis2D; 3],
    /// 3D: Darstellungsart (Index in RenderMode::ALL)
    pub render_mode: usize,
    pub textured: bool,
    pub models: bool,
    pub collision: bool,
    pub shadows: bool,
    /// globale Beleuchtung in der Lichtvorschau
    pub gi: bool,
    /// Partikel-Vorschau (info_particle_system)
    pub particles: bool,
    /// Nebel aus env_fog_controller
    pub fog: bool,
    /// 2D: bei Auswahl andere Ebenen abdunkeln
    pub dim_levels: bool,
    /// Höhe des Texturbereichs unten im rechten Panel, eingeklappt
    pub tex_h: f32,
    pub tex_collapsed: bool,
    pub sky_preview: bool,
    pub helpers: bool,
    /// Reiter im rechten Panel
    pub panel_tab: usize,
}

impl Default for Layout {
    fn default() -> Self {
        use crate::geometry::Axis2D;
        Layout { split: [0.5, 0.5], right_width: 360.0, maximized: None, axes: [Axis2D::Top, Axis2D::Front, Axis2D::Side], render_mode: 0, textured: true, models: true, collision: false, shadows: true, gi: false, particles: true, fog: true, dim_levels: true, tex_h: 250.0, tex_collapsed: false, sky_preview: true, helpers: true, panel_tab: 0 }
    }
}

/// Pfade eines Spiels (z. B. Garry's Mod, CS:S, HL2)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct GameProfile {
    pub name: String,
    pub game_dir: String,
    pub vbsp: String,
    pub vvis: String,
    pub vrad: String,
    pub game_exe: String,
    pub launch_args: String,
    pub wrapper: String,
    pub fgd_files: Vec<String>,
    /// Map-Grenze (±Einheiten); 0 = Standard 16384
    pub map_limit: f64,
}

/// Steuerung der 3D-Kamera. Tasten als egui-Namen ("W", "Space", "ArrowUp" …, "" = keine)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CamControls {
    pub forward: String,
    pub back: String,
    pub left: String,
    pub right: String,
    pub up: String,
    /// zweite Taste für „hoch“
    pub up2: String,
    pub down: String,
    /// Maustaste zum Umsehen: "middle" (Mausrad gedrückt) oder "right"
    pub look: String,
    /// Taste für schneller fliegen: "Shift", "Alt" oder "" (keine)
    pub fast: String,
    /// Grad je Pixel beim Umsehen
    pub sensitivity: f32,
    pub invert_y: bool,
}

impl Default for CamControls {
    fn default() -> Self {
        CamControls {
            forward: "W".into(),
            back: "S".into(),
            left: "A".into(),
            right: "D".into(),
            up: "E".into(),
            up2: "Space".into(),
            down: "Q".into(),
            look: "middle".into(),
            fast: "Shift".into(),
            sensitivity: 0.25,
            invert_y: false,
        }
    }
}

impl CamControls {
    /// Beschriftung und Feld der Bewegungstasten (für die Einstellungen)
    pub const SLOTS: [&'static str; 7] = ["Vorwärts", "Rückwärts", "Links", "Rechts", "Hoch", "Hoch (2. Taste)", "Runter"];
    pub fn slot_mut(&mut self, i: usize) -> &mut String {
        match i {
            0 => &mut self.forward,
            1 => &mut self.back,
            2 => &mut self.left,
            3 => &mut self.right,
            4 => &mut self.up,
            5 => &mut self.up2,
            _ => &mut self.down,
        }
    }
    /// Bewegungstasten mit Richtung: (Taste, Index 0 vor, 1 zurück, 2 links, 3 rechts, 4 hoch, 5 runter)
    pub fn move_keys(&self) -> Vec<(eframe::egui::Key, usize)> {
        [(&self.forward, 0), (&self.back, 1), (&self.left, 2), (&self.right, 3), (&self.up, 4), (&self.up2, 4), (&self.down, 5)]
            .into_iter()
            .filter_map(|(k, d)| eframe::egui::Key::from_name(k.trim()).map(|k| (k, d)))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// .../GarrysMod/garrysmod
    pub game_dir: String,
    pub vbsp: String,
    pub vvis: String,
    pub vrad: String,
    /// .../GarrysMod/hl2.exe (oder gmod.exe)
    pub game_exe: String,
    pub launch_args: String,
    /// z. B. "wine" unter Linux (Compile-Tools sind Windows-Programme)
    pub wrapper: String,
    pub fgd_files: Vec<String>,
    pub vis: VisMode,
    pub rad: RadMode,
    pub hdr: bool,
    pub copy_to_maps: bool,
    pub launch_game: bool,
    pub build_cubemaps: bool,
    pub extra_vbsp: String,
    pub extra_vvis: String,
    pub extra_vrad: String,
    pub expert: bool,
    pub steps: Vec<Step>,
    pub profiles: Vec<GameProfile>,
    pub active_profile: usize,
    /// Map-Grenze (±Einheiten) des aktiven Profils
    pub map_limit: f64,
    /// Fenster-Aufteilung, Panelbreite, Ansichten (wird beim Beenden gespeichert)
    pub layout: Layout,
    pub autosave_minutes: u32,
    pub autosave_keep: u32,
    pub prefab_dir: String,
    pub tex_lock: bool,
    pub grid: f64,
    pub recent_files: Vec<String>,
    /// Eigene Tastenbelegung: Befehls-ID → Kürzel ("" = keins)
    pub keymap: std::collections::HashMap<String, String>,
    /// Steuerung der 3D-Kamera (Bewegungstasten, Umsehen-Taste)
    pub cam: CamControls,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub recent_materials: Vec<String>,
    pub favorite_materials: Vec<String>,
    /// Kamera-Lesezeichen je Map: [x, y, z, yaw, pitch]
    pub bookmarks: std::collections::HashMap<String, Vec<Option<[f32; 5]>>>,
    pub obj_meters: bool,
    /// Automatische Optimierungen (KI-Nodes, func_detail, nodraw …)
    pub opt: crate::optimize::OptSettings,
    /// Startbildschirm mit Vorlagen anzeigen
    pub show_start: bool,
    /// Einfache Oberfläche (weniger Werkzeuge und Menüpunkte)
    pub simple_ui: bool,
    /// Einrichtungs-Assistent wurde abgeschlossen
    pub setup_done: bool,
    /// Tutorial: abgeschlossene Kapitel
    pub tutorial_done: Vec<usize>,
    /// Dunkles (true) oder helles Design
    pub theme_dark: bool,
    /// Workshop-/Addon-Inhalte (.gma, addons/<name>) als Inhaltsquelle verwenden
    pub load_addons: bool,
    /// Favorisierte und zuletzt benutzte Entity-Klassen (Klassen-Auswahl)
    pub fav_classes: Vec<String>,
    pub recent_classes: Vec<String>,
    /// Sprache der Oberfläche ("de", "en", "fr", "es", "pl", "ru")
    pub language: String,
    /// beim Start auf GitHub nach neuen Versionen suchen
    pub check_updates: bool,
    /// diese Version nicht mehr anbieten
    pub skip_version: String,
    /// Größe der Oberfläche (1.0 = Systemvorgabe)
    pub ui_scale: f32,
    /// zuletzt gestartete Version (für „Neu in Amboss“ nach einem Update)
    pub last_version: String,
    /// Gameplay-Maße eingeblendet
    pub gameplay: bool,
    /// Workshop-Angaben je Map (Name ohne Endung)
    pub workshop: std::collections::HashMap<String, crate::workshop::WorkshopInfo>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            game_dir: String::new(),
            vbsp: String::new(),
            vvis: String::new(),
            vrad: String::new(),
            game_exe: String::new(),
            launch_args: "-steam -game garrysmod -novid +map {map}".into(),
            wrapper: String::new(),
            fgd_files: Vec::new(),
            vis: VisMode::Normal,
            rad: RadMode::Normal,
            hdr: true,
            copy_to_maps: true,
            launch_game: true,
            build_cubemaps: false,
            extra_vbsp: String::new(),
            extra_vvis: String::new(),
            extra_vrad: String::new(),
            expert: false,
            steps: Vec::new(),
            layout: Layout::default(),
            map_limit: 16384.0,
            profiles: Vec::new(),
            active_profile: 0,
            autosave_minutes: 5,
            autosave_keep: 5,
            prefab_dir: String::new(),
            tex_lock: true,
            grid: 16.0,
            recent_files: Vec::new(),
            keymap: Default::default(),
            cam: CamControls::default(),
            mcp_enabled: true,
            mcp_port: crate::mcp::DEFAULT_PORT,
            recent_materials: Vec::new(),
            favorite_materials: Vec::new(),
            bookmarks: Default::default(),
            obj_meters: false,
            opt: Default::default(),
            show_start: true,
            simple_ui: false,
            setup_done: false,
            tutorial_done: Vec::new(),
            load_addons: true,
            fav_classes: Vec::new(),
            recent_classes: Vec::new(),
            language: crate::i18n::Lang::from_system().code().into(),
            theme_dark: true,
            check_updates: true,
            skip_version: String::new(),
            ui_scale: 1.0,
            workshop: Default::default(),
            last_version: String::new(),
            gameplay: false,
        }
    }
}

/// Steam-Ordner und alle Steam-Bibliotheken (aus steamapps/libraryfolders.vdf)
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["PROGRAMFILES(X86)", "PROGRAMFILES", "ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(p) = std::env::var_os(var) {
            roots.push(PathBuf::from(p).join("Steam"));
        }
    }
    for drive in "CDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
        roots.push(PathBuf::from(format!("{drive}:/Program Files (x86)/Steam")));
        roots.push(PathBuf::from(format!("{drive}:/Program Files/Steam")));
        roots.push(PathBuf::from(format!("{drive}:/Steam")));
        roots.push(PathBuf::from(format!("{drive}:/SteamLibrary")));
        roots.push(PathBuf::from(format!("{drive}:/Games/Steam")));
        roots.push(PathBuf::from(format!("{drive}:/Games/SteamLibrary")));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join("Library/Application Support/Steam"));
    }
    let mut libs: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !r.join("steamapps").is_dir() {
            continue;
        }
        libs.push(r.clone());
        if let Ok(vdf) = std::fs::read_to_string(r.join("steamapps").join("libraryfolders.vdf")) {
            libs.extend(parse_libraryfolders(&vdf));
        }
    }
    let mut seen = std::collections::HashSet::new();
    libs.retain(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase().replace('\\', "/")));
    libs
}

/// Pfade aus Steams libraryfolders.vdf (Zeilen der Form  "path"  "D:\\SteamLibrary")
pub fn parse_libraryfolders(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('"').collect();
            (parts.len() >= 4 && parts[1].eq_ignore_ascii_case("path")).then(|| PathBuf::from(parts[3].replace("\\\\", "\\")))
        })
        .collect()
}

/// GarrysMod-Hauptordner in allen Steam-Bibliotheken suchen
pub fn find_gmod_root() -> Option<PathBuf> {
    steam_libraries().into_iter().map(|l| l.join("steamapps").join("common").join("GarrysMod")).find(|p| p.join("garrysmod").is_dir())
}

/// Spielordner-Eingabe tolerant machen: GarrysMod-Hauptordner oder Unterordner →…/garrysmod
pub fn normalize_game_dir(dir: &str) -> String {
    let p = PathBuf::from(dir.trim().trim_matches('"'));
    if p.as_os_str().is_empty() {
        return String::new();
    }
    // Hauptordner gewählt (enthält garrysmod/)
    for sub in ["garrysmod", "GarrysMod"] {
        let g = p.join(sub);
        if g.join("gameinfo.txt").is_file() || g.join("garrysmod_dir.vpk").is_file() || (g.join("materials").is_dir() && !p.join("materials").is_dir()) {
            return p.join(sub).display().to_string().replace('\\', "/");
        }
    }
    // Unterordner gewählt (z. B. garrysmod/materials oder garrysmod/addons)
    for a in p.ancestors() {
        if a.join("gameinfo.txt").is_file() {
            return a.display().to_string().replace('\\', "/");
        }
    }
    dir.trim().to_string()
}

/// Argumente mit Anführungszeichen aufteilen
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

impl Settings {
    pub fn profile(&self) -> GameProfile {
        GameProfile {
            name: self.profiles.get(self.active_profile).map(|p| p.name.clone()).unwrap_or_else(|| "Garry's Mod".into()),
            game_dir: self.game_dir.clone(),
            vbsp: self.vbsp.clone(),
            vvis: self.vvis.clone(),
            vrad: self.vrad.clone(),
            game_exe: self.game_exe.clone(),
            launch_args: self.launch_args.clone(),
            wrapper: self.wrapper.clone(),
            fgd_files: self.fgd_files.clone(),
            map_limit: self.map_limit,
        }
    }

    /// Aktuelle Pfade im aktiven Profil speichern
    pub fn store_profile(&mut self) {
        let p = self.profile();
        if self.profiles.is_empty() {
            self.profiles.push(p);
            self.active_profile = 0;
        } else {
            let i = self.active_profile.min(self.profiles.len() - 1);
            self.profiles[i] = p;
        }
    }

    pub fn load_profile(&mut self, i: usize) {
        self.store_profile();
        let Some(p) = self.profiles.get(i).cloned() else { return };
        self.active_profile = i;
        self.game_dir = p.game_dir;
        self.vbsp = p.vbsp;
        self.vvis = p.vvis;
        self.vrad = p.vrad;
        self.game_exe = p.game_exe;
        self.launch_args = p.launch_args;
        self.wrapper = p.wrapper;
        self.fgd_files = p.fgd_files;
        self.map_limit = if p.map_limit > 0.0 { p.map_limit } else { 16384.0 };
    }

    /// Map-Grenze des aktiven Profils (±Einheiten)
    pub fn limit(&self) -> f64 {
        if self.map_limit >= 1024.0 { self.map_limit } else { 16384.0 }
    }

    /// Sucht eine Garry's-Mod-Installation an den üblichen Steam-Orten.
    pub fn autodetect(&mut self) -> Result<String, String> {
        let root = find_gmod_root().ok_or("Garry's Mod wurde nicht gefunden (auch nicht in weiteren Steam-Bibliotheken). Bitte den Ordner …/GarrysMod/garrysmod manuell eintragen.")?;
        self.apply_root(&root);
        Ok(root.display().to_string())
    }

    /// Setzt alle Pfade relativ zum GarrysMod-Hauptordner.
    /// FGD neben dem Spiel suchen (…/GarrysMod/bin/garrysmod.fgd), wenn noch keine eingetragen ist
    pub fn detect_fgd(&mut self) -> bool {
        if !self.fgd_files.is_empty() || self.game_dir.is_empty() {
            return false;
        }
        let gd = Path::new(&self.game_dir);
        let Some(root) = gd.parent() else { return false };
        for name in ["garrysmod.fgd", "halflife2.fgd", "base.fgd"] {
            for dir in [root.join("bin"), root.join("bin").join("win64"), gd.to_path_buf()] {
                let p = dir.join(name);
                if p.is_file() {
                    self.fgd_files.push(p.display().to_string().replace('\\', "/"));
                    return true;
                }
            }
        }
        false
    }

    pub fn apply_root(&mut self, root: &Path) {
        let s = |p: PathBuf| p.display().to_string().replace('\\', "/");
        self.game_dir = s(root.join("garrysmod"));
        let bin = root.join("bin");
        let tool = |name: &str| -> String {
            for dir in [bin.join("win64"), bin.clone()] {
                for file in [format!("{name}.exe"), format!("{name}_linux"), name.to_string()] {
                    let p = dir.join(&file);
                    if p.is_file() {
                        return s(p);
                    }
                }
            }
            s(bin.join(format!("{name}.exe")))
        };
        self.vbsp = tool("vbsp");
        self.vvis = tool("vvis");
        self.vrad = tool("vrad");
        self.game_exe.clear();
        for exe in ["gmod.exe", "hl2.exe", "bin/win64/gmod.exe", "hl2.sh", "hl2_linux"] {
            if root.join(exe).is_file() {
                self.game_exe = s(root.join(exe));
                break;
            }
        }
        if self.game_exe.is_empty() {
            self.game_exe = s(root.join("hl2.exe"));
        }
        if self.fgd_files.is_empty() {
            let p = bin.join("garrysmod.fgd");
            if p.is_file() {
                self.fgd_files.push(s(p));
            }
        }
    }

    /// Schritte für den einfachen Modus (oder als Vorlage für den Expertenmodus)
    pub fn default_steps(&self) -> Vec<Step> {
        let step = |enabled: bool, name: &str, exe: &str, args: String| Step { enabled, name: name.into(), exe: exe.into(), args };
        let vis_fast = if self.vis == VisMode::Fast { "-fast " } else { "" };
        let rad_q = match self.rad {
            RadMode::Fast => "-fast ",
            RadMode::Final => "-final ",
            _ => "",
        };
        let hdr = if self.hdr { "-both " } else { "-ldr " };
        // Navmesh erzeugt Amboss selbst vor dem Kompilieren (navmesh.rs), nicht mehr per nav_generate im Spiel
        let launch = self.launch_args.replace("{map}", "$file");
        vec![
            step(true, "VBSP", "$vbsp", format!("{}-game \"$gamedir\" \"$vmf\"", with_space(&self.extra_vbsp))),
            step(self.vis != VisMode::Off, "VVIS", "$vvis", format!("{vis_fast}{}-game \"$gamedir\" \"$vmf\"", with_space(&self.extra_vvis))),
            step(self.rad != RadMode::Off, "VRAD", "$vrad", format!("{hdr}{rad_q}{}-game \"$gamedir\" \"$vmf\"", with_space(&self.extra_vrad))),
            step(self.opt.pack_content, "Inhalte packen", "$bspzip", "-game \"$gamedir\" -addlist \"$vmf.bsp\" \"$vmf.packlist.txt\" \"$vmf.bsp\"".into()),
            step(self.copy_to_maps, "BSP kopieren", "$copy", "\"$vmf.bsp\" \"$gamedir/maps/$file.bsp\"".into()),
            step(self.build_cubemaps, "Cubemaps bauen", "$gamewait", "-game garrysmod -novid -windowed -buildcubemaps +map $file".into()),
            step(self.launch_game, "Spiel starten", "$game", launch),
        ]
    }

    pub fn active_steps(&self) -> Vec<Step> {
        if self.expert && !self.simple_ui && !self.steps.is_empty() {
            self.steps.clone()
        } else {
            self.default_steps()
        }
    }
}

fn with_space(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        String::new()
    } else {
        format!("{t} ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

pub struct CompileJob {
    pub log: Arc<Mutex<Vec<String>>>,
    pub state: Arc<Mutex<JobState>>,
    pub leaked: Arc<AtomicBool>,
    /// VMF-Pfad (ohne Endung) – für Pointfile
    pub vmf_base: PathBuf,
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl CompileJob {
    /// Fertiger Lauf mit vorgegebenem Protokoll (Vorschau/Debug)
    pub fn finished(log: Vec<String>, ok: bool) -> CompileJob {
        CompileJob {
            log: Arc::new(Mutex::new(log)),
            state: Arc::new(Mutex::new(if ok { JobState::Done } else { JobState::Failed })),
            leaked: Arc::new(AtomicBool::new(false)),
            vmf_base: PathBuf::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }

    pub fn state(&self) -> JobState {
        *self.state.lock().unwrap()
    }
}

struct Resolved {
    name: String,
    kind: Kind,
}

enum Kind {
    Run { exe: String, args: Vec<String>, wait: bool },
    Copy { from: PathBuf, to: PathBuf },
    /// Eigene Inhalte aus der Packliste in die BSP packen (eingebaut, ohne bspzip)
    Pack { bsp: PathBuf, list: PathBuf },
}

#[allow(dead_code)]
pub fn start(settings: &Settings, vmf: &Path) -> Result<CompileJob, String> {
    start_with(settings, vmf, Vec::new())
}

/// Wie `start`, mit vorangestellten Log-Zeilen (z. B. Optimierungsbericht)
pub fn start_with(settings: &Settings, vmf: &Path, prelude: Vec<String>) -> Result<CompileJob, String> {
    if settings.game_dir.is_empty() {
        return Err("Spielverzeichnis ist nicht gesetzt – siehe Einstellungen.".into());
    }
    let map_base = vmf.with_extension("");
    let vars: Vec<(&str, String)> = vec![
        ("$vmf", map_base.display().to_string()),
        ("$path", vmf.parent().map(|p| p.display().to_string()).unwrap_or_default()),
        ("$file", vmf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
        ("$gamedir", settings.game_dir.clone()),
        ("$bspdir", Path::new(&settings.game_dir).join("maps").display().to_string()),
    ];
    let subst = |s: &str| {
        let mut out = s.to_string();
        for (k, v) in &vars {
            out = out.replace(k, v);
        }
        out
    };
    let mut steps = Vec::new();
    for st in settings.active_steps().into_iter().filter(|s| s.enabled) {
        let args: Vec<String> = split_args(&st.args).into_iter().map(|a| subst(&a)).collect();
        let kind = match st.exe.trim() {
            "$copy" => {
                if args.len() < 2 {
                    return Err(format!("{}: $copy braucht Quelle und Ziel", st.name));
                }
                Kind::Copy { from: PathBuf::from(&args[0]), to: PathBuf::from(&args[1]) }
            }
            "$game" | "$gamewait" => Kind::Run { exe: settings.game_exe.clone(), args, wait: st.exe.trim() == "$gamewait" },
            "$vbsp" => Kind::Run { exe: settings.vbsp.clone(), args, wait: true },
            "$vvis" => Kind::Run { exe: settings.vvis.clone(), args, wait: true },
            "$vrad" => Kind::Run { exe: settings.vrad.clone(), args, wait: true },
            "$bspzip" => {
                // nur wenn es etwas zu packen gibt
                let list = PathBuf::from(format!("{}.packlist.txt", map_base.display()));
                if !list.is_file() {
                    continue;
                }
                let _ = args;
                Kind::Pack { bsp: PathBuf::from(format!("{}.bsp", map_base.display())), list }
            }
            other => Kind::Run { exe: subst(other), args, wait: true },
        };
        steps.push(Resolved { name: st.name.clone(), kind });
    }

    let log = Arc::new(Mutex::new(prelude.iter().map(|l| crate::i18n::tr_dyn(l)).collect::<Vec<_>>()));
    let state = Arc::new(Mutex::new(JobState::Running));
    let cancel = Arc::new(AtomicBool::new(false));
    let leaked = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let job = CompileJob { log: log.clone(), state: state.clone(), leaked: leaked.clone(), vmf_base: map_base, cancel: cancel.clone(), child: child.clone() };
    let wrapper = settings.wrapper.trim().to_string();
    let game_exe = settings.game_exe.clone();

    std::thread::spawn(move || {
        // eigene Meldungen in der Oberflächensprache; Compiler-Ausgabe bleibt wie sie ist
        let say = |s: String| log.lock().unwrap().push(crate::i18n::tr_dyn(&s));
        let raw = |s: String| log.lock().unwrap().push(s);
        let finish = |st: JobState| *state.lock().unwrap() = st;
        for step in steps {
            if cancel.load(Ordering::SeqCst) {
                say("** Abgebrochen **".into());
                return finish(JobState::Cancelled);
            }
            say(format!("\n===== {} =====", step.name));
            match step.kind {
                Kind::Pack { bsp, list } => {
                    let text = std::fs::read_to_string(&list).unwrap_or_default();
                    let mut files = Vec::new();
                    for (rel, abs) in crate::bsppack::read_packlist(&text) {
                        match std::fs::read(&abs) {
                            Ok(b) => files.push((rel, b)),
                            Err(e) => say(format!("Übersprungen: {} ({e})", abs.display())),
                        }
                    }
                    match crate::bsppack::pack(&bsp, &files) {
                        Ok(r) => say(format!("{} Datei(en) in {} gepackt ({} neu, {} ersetzt, {} vorhanden), {:.1} MB", files.len(), bsp.display(), r.added, r.replaced, r.kept, r.bytes as f64 / 1048576.0)),
                        Err(e) => {
                            say(format!("Packen fehlgeschlagen: {e}"));
                            return finish(JobState::Failed);
                        }
                    }
                }
                Kind::Copy { from, to } => {
                    if let Some(dir) = to.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    match std::fs::copy(&from, &to) {
                        Ok(_) => say(format!("{} → {}", from.display(), to.display())),
                        Err(e) => {
                            say(format!("Kopieren fehlgeschlagen ({} → {}): {e}", from.display(), to.display()));
                            say("Hinweis: Ist diese Map gerade im Spiel geladen? Windows sperrt die Datei dann. Im Spiel eine andere Map laden oder das Spiel schließen – mit aktivem Live-Abgleich macht Amboss das automatisch.".into());
                            return finish(JobState::Failed);
                        }
                    }
                }
                Kind::Run { exe, args, wait } => {
                    let is_game = exe == game_exe;
                    let mut cmd = if wrapper.is_empty() || is_game {
                        Command::new(&exe)
                    } else {
                        let mut c = Command::new(&wrapper);
                        c.arg(&exe);
                        c
                    };
                    cmd.args(&args).stdin(Stdio::null());
                    if wait && !is_game {
                        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
                    }
                    if let Some(dir) = Path::new(&exe).parent() {
                        cmd.current_dir(dir);
                    }
                    #[cfg(windows)]
                    if !is_game {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                    }
                    say(format!("> {} {}", exe, args.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ")));
                    let mut c = match cmd.spawn() {
                        Ok(c) => c,
                        Err(e) => {
                            say(format!("Fehler: {exe} konnte nicht gestartet werden: {e}"));
                            return finish(JobState::Failed);
                        }
                    };
                    if !wait {
                        continue;
                    }
                    let stdout = c.stdout.take();
                    let stderr = c.stderr.take();
                    *child.lock().unwrap() = Some(c);
                    let log2 = log.clone();
                    let err_thread = std::thread::spawn(move || {
                        if let Some(mut e) = stderr {
                            let mut s = String::new();
                            let _ = e.read_to_string(&mut s);
                            for line in s.lines() {
                                log2.lock().unwrap().push(line.to_string());
                            }
                        }
                    });
                    if let Some(out) = stdout {
                        let mut reader = BufReader::new(out);
                        let mut buf = Vec::new();
                        loop {
                            buf.clear();
                            match reader.read_until(b'\n', &mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {
                                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                                    if line.contains("LEAKED") {
                                        leaked.store(true, Ordering::SeqCst);
                                    }
                                    raw(line);
                                }
                            }
                        }
                    }
                    let _ = err_thread.join();
                    let status = child.lock().unwrap().take().map(|mut c| c.wait());
                    if cancel.load(Ordering::SeqCst) {
                        say("** Abgebrochen **".into());
                        return finish(JobState::Cancelled);
                    }
                    match status {
                        Some(Ok(s)) if s.success() || is_game => {}
                        Some(Ok(s)) => {
                            say(format!("{} ist mit Code {:?} fehlgeschlagen.", step.name, s.code()));
                            if leaked.load(Ordering::SeqCst) {
                                say("Die Map hat ein LECK (Loch zur Leere). Das Pointfile wird geladen und zeigt den Weg.".into());
                            }
                            return finish(JobState::Failed);
                        }
                        _ => return finish(JobState::Failed),
                    }
                    if leaked.load(Ordering::SeqCst) && step.name == "VBSP" {
                        say("*** WARNUNG: Die Map hat ein LECK! ***".into());
                    }
                }
            }
        }
        say("\n===== Fertig =====".into());
        finish(JobState::Done)
    });
    Ok(job)
}

/// Sucht in einer Log-Zeile nach "brush N" / "entity N" (für Klick → Gehe zu)
pub fn find_id_in_line(line: &str) -> Option<u32> {
    let l = line.to_ascii_lowercase();
    for key in ["brush ", "brush:", "side ", "entity "] {
        if let Some(i) = l.find(key) {
            let rest = &l[i + key.len()..];
            let digits: String = rest.trim_start_matches(['#', ' ', '(']).chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn game_dir_detection() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"apps\" { \"4000\" \"1\" }\n\t}\n}\n";
        let libs = parse_libraryfolders(vdf);
        assert_eq!(libs, vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("D:\\SteamLibrary")]);
        let dir = std::env::temp_dir().join(format!("amboss_gd_{}", std::process::id()));
        let gm = dir.join("GarrysMod").join("garrysmod");
        std::fs::create_dir_all(gm.join("materials").join("dev")).unwrap();
        std::fs::write(gm.join("gameinfo.txt"), "\"GameInfo\" {}").unwrap();
        let want = gm.display().to_string().replace('\\', "/");
        assert_eq!(normalize_game_dir(&dir.join("GarrysMod").display().to_string()), want, "Hauptordner gewählt");
        assert_eq!(normalize_game_dir(&gm.join("materials").join("dev").display().to_string()), want, "Unterordner gewählt");
        assert_eq!(normalize_game_dir(&format!("  \"{}\"  ", gm.display())), want, "mit Anführungszeichen");
        assert_eq!(normalize_game_dir(""), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn helpers() {
        assert_eq!(split_args("-game \"C:/a b\" x"), vec!["-game", "C:/a b", "x"]);
        assert_eq!(find_id_in_line("Brush 123: plane with no normal"), Some(123));
        assert_eq!(find_id_in_line("Entity 7 (light) has no origin"), Some(7));
        assert_eq!(find_id_in_line("nothing here"), None);
    }

    #[test]
    fn pipeline_with_fake_tools() {
        let dir = std::env::temp_dir().join(format!("amboss_test_{}", std::process::id()));
        let game = dir.join("garrysmod");
        std::fs::create_dir_all(game.join("maps")).unwrap();
        let script = |name: &str, body: &str| {
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho \"{name} $@\"\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.display().to_string()
        };
        let mut s = Settings::default();
        s.game_dir = game.display().to_string();
        s.vbsp = script("vbsp", "for a; do last=$a; done; echo BSP > \"$last.bsp\"");
        s.vvis = script("vvis", "echo 0…1…2");
        s.vrad = script("vrad", "true");
        s.launch_game = false;
        let vmf = dir.join("testmap.vmf");
        std::fs::write(&vmf, "world { }").unwrap();
        let wait = |job: &CompileJob| {
            for _ in 0..300 {
                if job.state() != JobState::Running {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        let job = start(&s, &vmf).unwrap();
        wait(&job);
        let log = job.log.lock().unwrap().join("\n");
        assert_eq!(job.state(), JobState::Done, "{log}");
        assert!(log.contains("vvis -game"));
        assert!(log.contains("-both"));
        assert!(game.join("maps/testmap.bsp").is_file());

        // Leck erkennen
        s.vbsp = script("vbsp_leak", "echo '**** leaked ****'; echo 'LEAKED'; exit 1");
        let job = start(&s, &vmf).unwrap();
        wait(&job);
        assert_eq!(job.state(), JobState::Failed);
        assert!(job.leaked.load(Ordering::SeqCst));

        // Expertenmodus mit eigenem Schritt
        s.expert = true;
        s.steps = vec![Step { enabled: true, name: "Eigenes".into(), exe: script("custom", "echo custom-ok"), args: "\"$file\"".into() }];
        let job = start(&s, &vmf).unwrap();
        wait(&job);
        let log = job.log.lock().unwrap().join("\n");
        assert!(log.contains("custom testmap"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
