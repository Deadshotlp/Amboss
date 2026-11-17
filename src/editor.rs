//! Editor-Zustand und alle Bearbeitungs-Operationen (unabhängig von der GUI).

use crate::csg;
use crate::disp::{self, Disp, PaintMode, Rng};
use crate::fgd::{ClassKind, Fgd};
use crate::geometry::{snap_vec_grid, Aabb, Axis2D, Plane};
use crate::map::{default_tex_axes, face_tex_axes, fmt_vec, Entity, Group, Map, Shared, Solid, Visgroup};
use crate::primitives::{self, ShapeParams};
use crate::transform::{angles_to_mat, Xform};
use glam::{DMat3, DQuat, DVec2, DVec3};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Obj {
    /// Brush (Welt oder – im Solid-Modus – innerhalb einer Entity)
    Solid(u32),
    /// Entity (Punkt- oder Brush-Entity)
    Entity(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Magnify,
    Camera,
    Entity,
    Block,
    Face,
    Decal,
    Overlay,
    Clip,
    Vertex,
    Disp,
    Cordon,
    Measure,
}

impl Tool {
    pub const ALL: [Tool; 13] = [
        Tool::Select,
        Tool::Magnify,
        Tool::Camera,
        Tool::Entity,
        Tool::Block,
        Tool::Face,
        Tool::Decal,
        Tool::Overlay,
        Tool::Clip,
        Tool::Vertex,
        Tool::Disp,
        Tool::Cordon,
        Tool::Measure,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => crate::i18n::tr("Auswahl"),
            Tool::Magnify => crate::i18n::tr("Lupe"),
            Tool::Camera => crate::i18n::tr("Kamera"),
            Tool::Entity => crate::i18n::tr("Entity"),
            Tool::Block => crate::i18n::tr("Block"),
            Tool::Face => crate::i18n::tr("Textur"),
            Tool::Decal => crate::i18n::tr("Decal"),
            Tool::Overlay => crate::i18n::tr("Overlay"),
            Tool::Clip => crate::i18n::tr("Clip"),
            Tool::Vertex => crate::i18n::tr("Vertex"),
            Tool::Disp => crate::i18n::tr("Displacement"),
            Tool::Cordon => crate::i18n::tr("Cordon"),
            Tool::Measure => crate::i18n::tr("Messen"),
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Tool::Select => "Shift+S",
            Tool::Magnify => "Shift+G",
            Tool::Camera => "Shift+C",
            Tool::Entity => "Shift+E",
            Tool::Block => "Shift+B",
            Tool::Face => "Shift+A",
            Tool::Decal => "Shift+D",
            Tool::Overlay => "Shift+O",
            Tool::Clip => "Shift+X",
            Tool::Vertex => "Shift+V",
            Tool::Disp => "Shift+P",
            Tool::Cordon => "Shift+K",
            Tool::Measure => "Shift+M",
        }
    }
}

/// Modus der Auswahlgriffe (Klick auf die Auswahl wechselt wie in Hammer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XMode {
    Scale,
    Rotate,
    Skew,
}

impl XMode {
    pub fn next(self) -> XMode {
        match self {
            XMode::Scale => XMode::Rotate,
            XMode::Rotate => XMode::Skew,
            XMode::Skew => XMode::Scale,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            XMode::Scale => crate::i18n::tr("Skalieren"),
            XMode::Rotate => crate::i18n::tr("Drehen"),
            XMode::Skew => crate::i18n::tr("Scheren"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelMode {
    Groups,
    Objects,
    Solids,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Both,
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipLine {
    pub axis: Axis2D,
    pub a: DVec2,
    pub b: DVec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
    Fit,
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

#[derive(Debug, Clone, Copy)]
pub struct DispBrush {
    pub mode: PaintMode,
    pub radius: f64,
    pub strength: f64,
    pub along_z: bool,
    pub power: u32,
    pub noise_min: f64,
    pub noise_max: f64,
    pub show_alpha: bool,
    /// Abfall des Pinsels und Stufenhöhe für Terrassen
    pub falloff: crate::disp::Falloff,
    pub step: f64,
    /// zählt Striche hoch (Rauschen sieht bei jedem Strich anders aus)
    pub stroke: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AutoVis {
    pub world: bool,
    pub detail: bool,
    pub triggers: bool,
    pub tools: bool,
    pub lights: bool,
    pub point_ents: bool,
    pub brush_ents: bool,
    pub disps: bool,
    pub props: bool,
}

impl Default for AutoVis {
    fn default() -> Self {
        AutoVis { world: true, detail: true, triggers: true, tools: true, lights: true, point_ents: true, brush_ents: true, disps: true, props: true }
    }
}

/// Map-Stand im Rückgängig-Verlauf: vollständig oder als Unterschied zum nächsten Stand
#[derive(Clone)]
enum SnapData {
    Full(Map),
    Delta(Box<MapDelta>),
}

/// Ein Map-Stand, gespeichert als Abweichung von einem Bezugsstand (dem nächsten im Verlauf)
#[derive(Clone)]
struct MapDelta {
    /// Map ohne Brushes und Entities (Eigenschaften, Gruppen, Visgroups, Kameras …)
    shell: Map,
    world_order: Vec<u32>,
    ent_order: Vec<u32>,
    /// Brushes/Entities, die im Bezugsstand fehlen oder anders sind
    solids: HashMap<u32, Shared<Solid>>,
    entities: HashMap<u32, Shared<Entity>>,
}

fn unique_ids(m: &Map) -> bool {
    let mut a = std::collections::HashSet::new();
    let mut b = std::collections::HashSet::new();
    m.world.solids.iter().all(|s| a.insert(s.id)) && m.entities.iter().all(|e| b.insert(e.id))
}

impl SnapData {
    /// `old` relativ zu `reference` speichern
    fn delta(mut old: Map, reference: &Map) -> SnapData {
        if !unique_ids(&old) || !unique_ids(reference) {
            return SnapData::Full(old);
        }
        let rs: HashMap<u32, &Shared<Solid>> = reference.world.solids.iter().map(|s| (s.id, s)).collect();
        let re: HashMap<u32, &Shared<Entity>> = reference.entities.iter().map(|e| (e.id, e)).collect();
        let solids = std::mem::take(&mut old.world.solids);
        let ents = std::mem::take(&mut old.entities);
        let world_order = solids.iter().map(|s| s.id).collect();
        let ent_order = ents.iter().map(|e| e.id).collect();
        let solids = solids.into_iter().filter(|s| rs.get(&s.id).is_none_or(|r| *r != s)).map(|s| (s.id, s)).collect();
        let entities = ents.into_iter().filter(|e| re.get(&e.id).is_none_or(|r| *r != e)).map(|e| (e.id, e)).collect();
        SnapData::Delta(Box::new(MapDelta { shell: old, world_order, ent_order, solids, entities }))
    }

    /// Vollständige Map herstellen (`reference` = Stand, auf den sich ein Delta bezieht)
    fn into_map(self, reference: &Map) -> Map {
        match self {
            SnapData::Full(m) => m,
            SnapData::Delta(d) => {
                let d = *d;
                let (mut solids, mut entities) = (d.solids, d.entities);
                let rs: HashMap<u32, &Shared<Solid>> = reference.world.solids.iter().map(|s| (s.id, s)).collect();
                let re: HashMap<u32, &Shared<Entity>> = reference.entities.iter().map(|e| (e.id, e)).collect();
                let mut m = d.shell;
                m.world.solids = d.world_order.iter().filter_map(|id| solids.remove(id).or_else(|| rs.get(id).map(|s| (*s).clone()))).collect();
                m.entities = d.ent_order.iter().filter_map(|id| entities.remove(id).or_else(|| re.get(id).map(|e| (*e).clone()))).collect();
                m
            }
        }
    }

    fn is_full(&self) -> bool {
        matches!(self, SnapData::Full(_))
    }

    /// Ungefähre Größe (Anzahl gespeicherter Brushes) – für Tests/Anzeige
    fn stored_solids(&self) -> usize {
        match self {
            SnapData::Full(m) => m.all_solids().count(),
            SnapData::Delta(d) => d.solids.len() + d.entities.values().map(|e| e.solids.len()).sum::<usize>(),
        }
    }
}

#[derive(Clone)]
struct Snapshot {
    data: SnapData,
    sel: BTreeSet<Obj>,
    faces: BTreeSet<(u32, usize)>,
    /// Beschreibung der Aktion, die auf diesen Zustand folgte
    label: String,
}

#[derive(Clone)]
pub enum ClipItem {
    Solid(Solid),
    Entity(Entity),
}

pub struct Editor {
    pub map: Map,
    pub sel: BTreeSet<Obj>,
    /// Ausgewählte Flächen (Solid-ID, Seiten-Index)
    pub faces: BTreeSet<(u32, usize)>,
    /// Ausgewählte Eckpunkte im Vertex-Werkzeug (Solid-ID, Position)
    pub vsel: Vec<(u32, DVec3)>,
    pub tool: Tool,
    pub xmode: XMode,
    pub sel_mode: SelMode,
    pub grid: f64,
    pub material: String,
    pub entity_class: String,
    /// Modell für neue prop_*-Entities
    pub next_model: String,
    pub tex_lock: bool,
    pub scale_lock: bool,
    pub tex_scale: f64,
    pub shape: ShapeParams,
    pub clip: Option<ClipLine>,
    pub clip_mode: ClipMode,
    pub dbrush: DispBrush,
    /// Streu-Pinsel (Vegetation)
    pub scatter: crate::scatter::ScatterOpts,
    /// Tile-Set-Werkzeug
    pub tiles: crate::tiles::TileState,
    /// Polygon-Mesh in Bearbeitung
    pub mesh_edit: Option<crate::mesh_edit::MeshEdit>,
    /// Bezier-Patch in Bearbeitung
    pub patch_edit: Option<crate::mesh_edit::PatchEdit>,
    /// Spieler im laufenden Spiel (Live-Abgleich): Füße, Blickwinkel
    pub live_player: Option<(DVec3, DVec3)>,
    flatten_ref: Option<f64>,
    pub fgd: Fgd,
    pub path: Option<PathBuf>,
    pub game_dir: String,
    pub dirty: bool,
    /// Wird bei jeder Änderung (Map oder Auswahl) erhöht → 3D-Neuaufbau
    pub rev: u64,
    /// Arbeitspunkt: wird durch Klicks in den 2D-Ansichten gesetzt
    pub work_point: DVec3,
    /// Maße des zuletzt erstellten Blocks (Tiefe für neue Blöcke)
    pub last_box: Aabb,
    pub status: String,
    pub auto_vis: AutoVis,
    /// Bounding Boxes geladener Modelle (Pfad → Box)
    pub model_bounds: HashMap<String, Aabb>,
    /// Dreiecke geladener Modelle (Pfad → Eckpunkte in Modell-Koordinaten) – für genaues
    /// Anklicken in 3D (durch Löcher und Lücken eines Modells hindurch)
    pub model_tris: HashMap<String, Arc<Vec<[DVec3; 3]>>>,
    /// Umriss-Kanten geladener Modelle (Rand- und Knickkanten) für die 2D-Ansichten
    pub model_edges: HashMap<String, Arc<Vec<[DVec3; 2]>>>,
    /// Geladene Instanz-Maps (Dateiname → Map)
    pub instances: HashMap<String, Option<Arc<Map>>>,
    /// Pipette: (Entity-ID, Schlüssel) wartet auf Klick
    pub pick_target: Option<(u32, String)>,
    pub camera_changed: bool,
    pub pointfile: Vec<DVec3>,
    pub portals: Vec<Vec<DVec3>>,
    /// Optimierer-Anzeige: Node-Verbindungen (a, b, ok) und Markierungen
    pub opt_links: Vec<(DVec3, DVec3, bool)>,
    pub opt_markers: Vec<crate::optimize::Marker>,
    /// Vergleich mit anderem Map-Stand: farbige Kästen
    pub diff_boxes: Vec<(crate::geometry::Aabb, [u8; 3])>,
    /// Am Raster einrasten (aus = auf ganze Einheiten)
    pub snap: bool,
    /// Beim Verschieben an Eckpunkten anderer Objekte einrasten
    pub vertex_snap: bool,
    /// Kopierte Entity-Eigenschaften (Klasse, Schlüssel, Outputs)
    pub prop_clipboard: Option<(String, Vec<(String, String)>, Vec<(String, String)>)>,
    /// Zusammenarbeit: von anderen ausgewählte (gesperrte) Objekte und Teilnehmer zur Anzeige
    pub remote_locked_solids: std::collections::HashSet<u32>,
    pub remote_locked_entities: std::collections::HashSet<u32>,
    pub remote_users: Vec<crate::collab::RemoteUser>,
    /// Diese Map gehört zur laufenden Live-Sitzung
    pub collab_session: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    clipboard: Vec<ClipItem>,
    paste_count: u32,
    rng: Rng,
    label_pending: bool,
    /// Messwerkzeug: Start- und Endpunkt
    pub measure: Option<(DVec3, DVec3)>,
    /// Prefab zum Platzieren (Index in die Prefab-Liste) – nächster Klick setzt es
    pub armed_prefab: Option<usize>,
    /// Platzierungswunsch aus einer Ansicht: (Prefab-Index, Ankerpunkt, Flächennormale)
    pub prefab_drop: Option<(usize, DVec3, Option<DVec3>)>,
    /// 2D-Ansicht färbt Objekte nach ihrer Visgroup
    pub vis_colors: bool,
    /// Strg+R: Auswahl dreht sich live zur Maus (Linksklick übernimmt, Rechtsklick/Esc bricht ab)
    pub rot_live: Option<crate::editor2::RotLive>,
    /// 2D: bei Auswahl alles abdunkeln, was nicht auf derselben Tiefe liegt (±10)
    pub dim_levels: bool,
    /// Während ein Lua-Skript läuft, legen einzelne Befehle keine eigenen Rückgängig-Schritte an
    pub checkpoint_hold: u8,
    /// Navmesh zum Anzeigen (erzeugt oder geladen)
    pub nav_preview: Option<std::sync::Arc<Vec<crate::navmesh::Area>>>,
    pub nav_show: bool,
    /// Map-Grenze (±Einheiten), je Spielprofil – Garry's Mod/Source: 16384
    pub map_limit: f64,
    clip_cache: std::cell::RefCell<Option<([u64; 4], ClipMode, u64, Vec<Solid>, Vec<Solid>)>>,
    /// Clip mit drei Punkten (in der 3D-Ansicht gesetzt)
    pub clip3: Vec<DVec3>,
}

const UNDO_LIMIT: usize = 200;
const TOOL_MATS: [&str; 12] = [
    "toolsnodraw", "toolsclip", "toolsplayerclip", "toolsnpcclip", "toolstrigger", "toolsskybox", "toolsinvisible", "toolshint", "toolsskip", "toolsareaportal", "toolsblocklight", "toolsblack",
];

pub fn is_tool_material(m: &str) -> bool {
    let m = m.to_ascii_lowercase();
    m.starts_with("tools/") && TOOL_MATS.iter().any(|t| m.contains(t))
}

impl Editor {
    pub fn new(fgd: Fgd) -> Self {
        Editor {
            map: Map::new(),
            sel: BTreeSet::new(),
            faces: BTreeSet::new(),
            vsel: Vec::new(),
            tool: Tool::Select,
            xmode: XMode::Scale,
            sel_mode: SelMode::Groups,
            grid: 16.0,
            material: "DEV/DEV_MEASUREGENERIC01B".into(),
            entity_class: "info_player_start".into(),
            next_model: "models/props_junk/wood_crate001a.mdl".into(),
            tex_lock: true,
            scale_lock: false,
            tex_scale: 0.25,
            shape: ShapeParams::default(),
            clip: None,
            clip_mode: ClipMode::Both,
            scatter: Default::default(),
            tiles: Default::default(),
            mesh_edit: None,
            patch_edit: None,
            live_player: None,
            dbrush: DispBrush { mode: PaintMode::Raise, radius: 64.0, strength: 8.0, along_z: true, power: 3, noise_min: -8.0, noise_max: 8.0, show_alpha: false, falloff: crate::disp::Falloff::Soft, step: 32.0, stroke: 0 },
            flatten_ref: None,
            fgd,
            path: None,
            game_dir: String::new(),
            dirty: false,
            rev: 1,
            work_point: DVec3::ZERO,
            last_box: Aabb::new(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 128.0)),
            status: "Bereit".into(),
            auto_vis: AutoVis::default(),
            model_bounds: HashMap::new(),
            model_tris: HashMap::new(),
            model_edges: HashMap::new(),
            instances: HashMap::new(),
            pick_target: None,
            camera_changed: false,
            pointfile: Vec::new(),
            portals: Vec::new(),
            opt_links: Vec::new(),
            opt_markers: Vec::new(),
            diff_boxes: Vec::new(),
            snap: true,
            vertex_snap: false,
            prop_clipboard: None,
            remote_locked_solids: Default::default(),
            remote_locked_entities: Default::default(),
            remote_users: Vec::new(),
            collab_session: false,
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: Vec::new(),
            paste_count: 0,
            rng: Rng::new(0x2545F4914F6CDD1D),
            label_pending: false,
            measure: None,
            armed_prefab: None,
            prefab_drop: None,
            vis_colors: false,
            rot_live: None,
            dim_levels: true,
            checkpoint_hold: 0,
            nav_preview: None,
            nav_show: true,
            map_limit: 16384.0,
            clip_cache: std::cell::RefCell::new(None),
            clip3: Vec::new(),
        }
    }

    // ------------------------------------------------------------ Datei

    fn reset_state(&mut self) {
        self.sel.clear();
        self.faces.clear();
        self.vsel.clear();
        self.undo.clear();
        self.redo.clear();
        self.clip = None;
        self.pointfile.clear();
        self.portals.clear();
        self.opt_links.clear();
        self.opt_markers.clear();
        self.diff_boxes.clear();
        self.instances.clear();
        self.camera_changed = true;
        self.touch();
    }

    pub fn new_map(&mut self) {
        self.map = Map::new();
        self.path = None;
        self.dirty = false;
        self.reset_state();
    }

    pub fn load(&mut self, path: PathBuf) -> Result<(), String> {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let src = String::from_utf8_lossy(&bytes);
        let mut map = Map::from_vmf(&src)?;
        // Polygon-Meshes aus der Begleitdatei
        if let Ok(t) = std::fs::read_to_string(Self::mesh_file(&path)) {
            if let Ok(v) = serde_json::from_str::<Vec<crate::mesh::MeshObj>>(&t) {
                map.meshes = v.into_iter().map(crate::map::Shared::new).collect();
            }
        }
        self.map = map;
        self.grid = self.map.grid.clamp(1.0, 1024.0);
        self.status = format!("Geladen: {} ({} Brushes, {} Entities)", path.display(), self.map.all_solids().count(), self.map.entities.len());
        self.path = Some(path);
        self.dirty = false;
        self.reset_state();
        Ok(())
    }

    pub fn save(&mut self, path: PathBuf) -> Result<(), String> {
        self.map.grid = self.grid;
        let text = self.map.to_vmf();
        if path.exists() {
            let _ = std::fs::copy(&path, path.with_extension("vmx"));
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        // Polygon-Meshes: nur die mit vorhandenem Prop speichern
        let live: Vec<crate::mesh::MeshObj> = self.map.meshes.iter().filter(|m| self.map.entity(m.ent).is_some()).map(|m| (**m).clone()).collect();
        let mf = Self::mesh_file(&path);
        if live.is_empty() {
            let _ = std::fs::remove_file(&mf);
        } else {
            std::fs::write(&mf, serde_json::to_string(&live).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        self.status = format!("Gespeichert: {}", path.display());
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    pub fn map_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "unbenannt".into())
    }

    // ------------------------------------------------------------ Undo

    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    /// Vor jeder Änderung aufrufen.
    pub fn checkpoint(&mut self) {
        // Skript läuft: ein gemeinsamer Rückgängig-Schritt für das ganze Skript.
        // 1 = noch kein Schritt angelegt (der erste ändernde Befehl legt ihn an), 2 = schon angelegt
        match self.checkpoint_hold {
            2 => {
                self.dirty = true;
                self.touch();
                return;
            }
            1 => self.checkpoint_hold = 2,
            _ => {}
        }
        // noch unbeschrifteten vorherigen Schritt (gleiches Bild) mit dem aktuellen Status benennen
        if self.label_pending {
            if let Some(s) = self.undo.last_mut() {
                if s.label.is_empty() {
                    s.label = self.status.clone();
                }
            }
        }
        // bisherige Vollkopie oben im Stapel in einen Unterschied zum jetzigen Stand umwandeln
        if let Some(top) = self.undo.last_mut() {
            if top.data.is_full() {
                let SnapData::Full(m) = std::mem::replace(&mut top.data, SnapData::Full(Map::default())) else { unreachable!() };
                top.data = SnapData::delta(m, &self.map);
            }
        }
        self.undo.push(Snapshot { data: SnapData::Full(self.map.clone()), sel: self.sel.clone(), faces: self.faces.clone(), label: String::new() });
        self.label_pending = true;
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.touch();
    }

    /// Einen Schritt von einem Stapel holen und den jetzigen Stand auf den anderen legen.
    /// Oben auf beiden Stapeln liegt immer eine Vollkopie, darunter Unterschiede zum jeweils nächsten Stand.
    fn step_history(&mut self, redo: bool) -> bool {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(s) = from.pop() else { return false };
        let new_map = s.data.into_map(&self.map);
        if let Some(top) = from.last_mut() {
            if !top.data.is_full() {
                let d = std::mem::replace(&mut top.data, SnapData::Full(Map::default()));
                top.data = SnapData::Full(d.into_map(&new_map));
            }
        }
        let old_map = std::mem::replace(&mut self.map, new_map);
        if let Some(top) = to.last_mut() {
            if top.data.is_full() {
                let SnapData::Full(m) = std::mem::replace(&mut top.data, SnapData::Full(Map::default())) else { unreachable!() };
                top.data = SnapData::delta(m, &old_map);
            }
        }
        to.push(Snapshot { data: SnapData::Full(old_map), sel: std::mem::replace(&mut self.sel, s.sel), faces: std::mem::replace(&mut self.faces, s.faces), label: s.label });
        true
    }

    /// Änderungen anderer Teilnehmer übernehmen: aktuelle Map und der ganze Rückgängig-Verlauf werden angepasst,
    /// damit Strg+Z nur eigene Schritte zurücknimmt.
    pub fn apply_remote_ops(&mut self, ops: &[crate::collab::Parsed]) {
        use crate::collab::{apply_to_map, Parsed};
        if ops.is_empty() {
            return;
        }
        for p in ops {
            apply_to_map(&mut self.map, p);
        }
        for snap in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            match &mut snap.data {
                SnapData::Full(m) => {
                    for p in ops {
                        apply_to_map(m, p);
                    }
                }
                SnapData::Delta(d) => {
                    for p in ops {
                        match p {
                            Parsed::PutSolid(s) => {
                                if let Some(o) = d.solids.get_mut(&s.id) {
                                    *o = s.clone().into();
                                }
                                if !d.world_order.contains(&s.id) {
                                    d.world_order.push(s.id);
                                }
                            }
                            Parsed::RemoveSolid(id) => {
                                d.solids.remove(id);
                                d.world_order.retain(|x| x != id);
                            }
                            Parsed::PutEntity(e) => {
                                if let Some(o) = d.entities.get_mut(&e.id) {
                                    *o = e.clone().into();
                                }
                                if !d.ent_order.contains(&e.id) {
                                    d.ent_order.push(e.id);
                                }
                            }
                            Parsed::RemoveEntity(id) => {
                                d.entities.remove(id);
                                d.ent_order.retain(|x| x != id);
                            }
                            Parsed::Shell(_) => apply_to_map(&mut d.shell, p),
                        }
                    }
                }
            }
        }
        // Auswahl bereinigen (gelöschte Objekte)
        let map = &self.map;
        self.sel.retain(|o| match o {
            Obj::Solid(id) => map.solid(*id).is_some(),
            Obj::Entity(id) => map.entity(*id).is_some(),
        });
        self.faces.retain(|(sid, _)| map.solid(*sid).is_some());
        self.vsel.clear();
        self.touch();
    }

    /// Rückgängig-Verlauf leeren (z. B. nach dem Anlegen aus einer Vorlage)
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.label_pending = false;
    }

    /// Anzahl der im Verlauf gespeicherten Brushes (Speicherbedarf)
    pub fn undo_stored_solids(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(|s| s.data.stored_solids()).sum()
    }

    /// Beschriftet den letzten Undo-Schritt mit der aktuellen Statusmeldung (einmal pro Bild aufrufen)
    pub fn finish_label(&mut self) {
        if self.label_pending {
            self.label_pending = false;
            if let Some(s) = self.undo.last_mut() {
                s.label = if self.status.is_empty() { "Änderung".into() } else { self.status.clone() };
            }
        }
    }

    /// (Undo-Beschriftungen ältestes zuerst, Redo-Beschriftungen nächstes zuerst)
    pub fn history(&self) -> (Vec<String>, Vec<String>) {
        (self.undo.iter().map(|s| s.label.clone()).collect(), self.redo.iter().rev().map(|s| s.label.clone()).collect())
    }

    pub fn undo(&mut self) {
        if self.step_history(false) {
            self.dirty = true;
            self.vsel.clear();
            self.status = "Rückgängig".into();
            self.touch();
        }
    }

    pub fn redo(&mut self) {
        if self.step_history(true) {
            self.dirty = true;
            self.vsel.clear();
            self.status = "Wiederholen".into();
            self.touch();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ------------------------------------------------------------ Abfragen

    pub fn point_size(&self, classname: &str) -> (DVec3, DVec3) {
        match self.fgd.resolve_size(classname) {
            Some((a, b)) => (DVec3::from(a), DVec3::from(b)),
            None => (DVec3::splat(-8.0), DVec3::splat(8.0)),
        }
    }

    /// Modellpfad einer Entity (aus "model" oder der FGD-Klasse)
    pub fn model_path(&self, e: &Entity) -> Option<String> {
        if let Some(m) = e.get("model") {
            if m.to_ascii_lowercase().ends_with(".mdl") {
                return Some(m.replace('\\', "/").to_ascii_lowercase());
            }
        }
        self.fgd.class(e.classname()).and_then(|c| c.studio.clone()).filter(|s| s.ends_with(".mdl")).map(|s| s.to_ascii_lowercase())
    }

    /// Umriss eines Modells: Randkanten und Kanten, an denen die Flächen deutlich abknicken
    /// (> 35°) – so sieht man in 2D die Form statt nur einer Box, ohne jedes Dreieck zu zeichnen
    pub fn feature_edges(tris: &[[DVec3; 3]]) -> Vec<[DVec3; 2]> {
        let key = |p: DVec3| [(p.x * 16.0).round() as i64, (p.y * 16.0).round() as i64, (p.z * 16.0).round() as i64];
        // Kante → (Endpunkte, Normale der ersten Fläche, Anzahl Flächen, Knick)
        let mut map: HashMap<([i64; 3], [i64; 3]), ([DVec3; 2], DVec3, u32, bool)> = HashMap::new();
        let cos = 35f64.to_radians().cos();
        for t in tris {
            let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
            if n == DVec3::ZERO {
                continue;
            }
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(a), key(b));
                let k = if ka < kb { (ka, kb) } else { (kb, ka) };
                map.entry(k).and_modify(|e| {
                    e.2 += 1;
                    if e.1.dot(n).abs() < cos {
                        e.3 = true;
                    }
                }).or_insert(([a, b], n, 1, false));
            }
        }
        let mut out: Vec<[DVec3; 2]> = map.into_values().filter(|e| e.2 == 1 || e.3).map(|e| e.0).collect();
        out.truncate(30000);
        out
    }

    pub fn entity_xform(e: &Entity) -> Xform {
        Xform { m: angles_to_mat(e.angles()), t: e.origin().unwrap_or(DVec3::ZERO) }
    }

    pub fn entity_bounds(&self, e: &Entity) -> Aabb {
        if e.is_brush_entity() {
            return e.solids.iter().fold(Aabb::EMPTY, |b, s| b.union(&s.bounds));
        }
        let x = Self::entity_xform(e);
        if let Some(m) = self.map.meshes.iter().find(|m| m.ent == e.id) {
            let b = m.mesh.bounds();
            if !b.is_empty() {
                return Aabb::from_points(b.corners().map(|c| x.apply(c)));
            }
        }
        if let Some(b) = self.model_path(e).and_then(|m| self.model_bounds.get(&m)) {
            return Aabb::from_points(b.corners().map(|c| x.apply(c)));
        }
        if e.classname().eq_ignore_ascii_case("func_instance") {
            if let Some(Some(m)) = e.get("file").and_then(|f| self.instances.get(f)) {
                let b = m.all_solids().fold(Aabb::EMPTY, |b, (_, s)| b.union(&s.bounds));
                if !b.is_empty() {
                    return Aabb::from_points(b.corners().map(|c| x.apply(c)));
                }
            }
        }
        let o = e.origin().unwrap_or(DVec3::ZERO);
        let size = self.point_size(e.classname());
        Aabb::new(o + size.0, o + size.1)
    }

    pub fn entity_color(&self, e: &Entity) -> [u8; 3] {
        self.fgd.resolve_color(e.classname()).unwrap_or(if e.is_brush_entity() { [80, 200, 255] } else { [220, 30, 220] })
    }

    pub fn obj_bounds(&self, o: Obj) -> Aabb {
        match o {
            Obj::Solid(id) => self.map.solid(id).map(|s| s.bounds).unwrap_or(Aabb::EMPTY),
            Obj::Entity(id) => self.map.entity(id).map(|e| self.entity_bounds(e)).unwrap_or(Aabb::EMPTY),
        }
    }

    pub fn selection_bounds(&self) -> Aabb {
        self.sel.iter().fold(Aabb::EMPTY, |b, o| b.union(&self.obj_bounds(*o)))
    }

    // ------------------------------------------------------------ Sichtbarkeit

    fn auto_visible_solid(&self, owner: Option<&Entity>, s: &Solid) -> bool {
        let av = &self.auto_vis;
        if s.has_disp() && !av.disps {
            return false;
        }
        if !av.tools && s.sides.iter().all(|x| is_tool_material(&x.material)) {
            return false;
        }
        match owner {
            None => av.world,
            Some(e) => {
                let c = e.classname();
                if c == "func_detail" {
                    av.detail
                } else if c.starts_with("trigger_") {
                    av.triggers && av.brush_ents
                } else {
                    av.brush_ents
                }
            }
        }
    }

    pub fn is_visible_solid(&self, owner: Option<&Entity>, s: &Solid) -> bool {
        if s.editor.hidden || !self.map.visgroups_visible(&s.editor.visgroups) {
            return false;
        }
        if let Some(e) = owner {
            if e.editor.hidden || !self.map.visgroups_visible(&e.editor.visgroups) {
                return false;
            }
        }
        self.auto_visible_solid(owner, s)
    }

    pub fn is_visible_entity(&self, e: &Entity) -> bool {
        if e.editor.hidden || !self.map.visgroups_visible(&e.editor.visgroups) {
            return false;
        }
        if e.is_brush_entity() {
            return e.solids.iter().any(|s| self.is_visible_solid(Some(e), s));
        }
        let av = &self.auto_vis;
        let c = e.classname();
        if c.starts_with("light") {
            return av.lights && av.point_ents;
        }
        if c.starts_with("prop_") {
            return av.props && av.point_ents;
        }
        av.point_ents
    }

    /// Begleitdatei der Polygon-Meshes: <map>.meshes.json
pub fn mesh_file(vmf: &std::path::Path) -> std::path::PathBuf {
    vmf.with_extension("meshes.json")
}

#[cfg(test)]
    pub fn is_visible(&self, o: Obj) -> bool {
        match o {
            Obj::Solid(id) => match self.map.solid_owner(id) {
                Some(owner) => {
                    let e = owner.and_then(|eid| self.map.entity(eid));
                    self.map.solid(id).is_some_and(|s| self.is_visible_solid(e, s))
                }
                None => false,
            },
            Obj::Entity(id) => self.map.entity(id).is_some_and(|e| self.is_visible_entity(e)),
        }
    }

    /// Liegt eine Box im aktiven Cordon (oder ist kein Cordon aktiv)?
    pub fn in_cordon(&self, b: &Aabb) -> bool {
        match &self.map.cordon {
            Some(c) if c.active => c.bounds.intersects(b),
            _ => true,
        }
    }

    // ------------------------------------------------------------ Auswahl

    /// Welches Objekt wird gewählt, wenn man diesen Solid anklickt?
    pub fn obj_for(&self, owner: Option<u32>, solid_id: u32) -> Obj {
        match (self.sel_mode, owner) {
            (SelMode::Solids, _) | (_, None) => Obj::Solid(solid_id),
            (_, Some(e)) => Obj::Entity(e),
        }
    }

    fn groupid_of(&self, o: Obj) -> Option<u32> {
        match o {
            Obj::Solid(id) => self.map.solid(id).and_then(|s| s.editor.groupid),
            Obj::Entity(id) => self.map.entity(id).and_then(|e| e.editor.groupid),
        }
    }

    /// Gruppen-Modus: alle Mitglieder derselben Gruppe
    pub fn expand(&self, o: Obj) -> Vec<Obj> {
        if self.sel_mode != SelMode::Groups {
            return vec![o];
        }
        let Some(g) = self.groupid_of(o) else { return vec![o] };
        let mut out: Vec<Obj> = self.map.world.solids.iter().filter(|s| s.editor.groupid == Some(g)).map(|s| Obj::Solid(s.id)).collect();
        out.extend(self.map.entities.iter().filter(|e| e.editor.groupid == Some(g)).map(|e| Obj::Entity(e.id)));
        if out.is_empty() {
            vec![o]
        } else {
            out
        }
    }

    pub fn is_locked_solid(&self, owner: Option<&Entity>, s: &Solid) -> bool {
        s.editor.locked || self.remote_locked_solids.contains(&s.id) || self.map.visgroups_locked(&s.editor.visgroups) || owner.is_some_and(|e| self.is_locked_entity(e))
    }

    pub fn is_locked_entity(&self, e: &Entity) -> bool {
        e.editor.locked || self.remote_locked_entities.contains(&e.id) || self.map.visgroups_locked(&e.editor.visgroups)
    }

    /// Auswählbare Objekte mit ihren Boxen (abhängig vom Auswahlmodus)
    pub fn selectable(&self) -> Vec<(Obj, Aabb)> {
        let mut out = Vec::new();
        for s in &self.map.world.solids {
            if self.is_visible_solid(None, s) && !self.is_locked_solid(None, s) {
                out.push((Obj::Solid(s.id), s.bounds));
            }
        }
        for e in &self.map.entities {
            if !self.is_visible_entity(e) || self.is_locked_entity(e) {
                continue;
            }
            if e.is_brush_entity() && self.sel_mode == SelMode::Solids {
                for s in &e.solids {
                    if self.is_visible_solid(Some(e), s) {
                        out.push((Obj::Solid(s.id), s.bounds));
                    }
                }
            } else {
                out.push((Obj::Entity(e.id), self.entity_bounds(e)));
            }
        }
        out
    }

    /// Objekte unter einem Punkt einer 2D-Ansicht (kleinstes zuerst)
    pub fn pick_2d(&self, axis: Axis2D, p: DVec2, tol: f64) -> Vec<Obj> {
        // Treffer auf einer Kante oder am Mittelpunkt eines Brushes haben Vorrang (wie in Hammer),
        // sonst entscheidet die kleinste umgebende Box
        let edge_hit = |s: &Solid| -> bool {
            let c = axis.project(s.bounds.center());
            if (c - p).abs().max_element() <= tol * 1.5 {
                return true;
            }
            s.polys.iter().any(|poly| {
                (0..poly.len()).any(|k| {
                    let a = axis.project(poly[k]);
                    let b = axis.project(poly[(k + 1) % poly.len()]);
                    let ab = b - a;
                    let t = if ab.length_squared() < 1e-9 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
                    (a + ab * t).distance(p) <= tol
                })
            })
        };
        let mut hits: Vec<(f64, Obj)> = Vec::new();
        let mut consider = |o: Obj, b: &Aabb, prio: f64| {
            let mn = axis.project(b.min);
            let mx = axis.project(b.max);
            if p.x >= mn.x - tol && p.x <= mx.x + tol && p.y >= mn.y - tol && p.y <= mx.y + tol {
                let area = (mx.x - mn.x).max(1.0) * (mx.y - mn.y).max(1.0) * prio;
                match hits.iter_mut().find(|h| h.1 == o) {
                    Some(h) => h.0 = h.0.min(area),
                    None => hits.push((area, o)),
                }
            }
        };
        for s in &self.map.world.solids {
            if self.is_visible_solid(None, s) && !self.is_locked_solid(None, s) {
                consider(Obj::Solid(s.id), &s.bounds, if edge_hit(s) { 1e-6 } else { 1.0 });
            }
        }
        for e in &self.map.entities {
            if !self.is_visible_entity(e) || self.is_locked_entity(e) {
                continue;
            }
            if e.is_brush_entity() {
                for s in &e.solids {
                    if self.is_visible_solid(Some(e), s) {
                        consider(self.obj_for(Some(e.id), s.id), &s.bounds, if edge_hit(s) { 1e-6 } else { 1.0 });
                    }
                }
            } else {
                consider(Obj::Entity(e.id), &self.entity_bounds(e), 1e-7);
            }
        }
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        hits.into_iter().map(|h| h.1).collect()
    }

    /// Strahltest in 3D: (Objekt, Solid-ID + Seite falls Brush, Abstand)
    pub fn pick_3d(&self, origin: DVec3, dir: DVec3) -> Option<(Obj, Option<(u32, usize)>, f64)> {
        let mut best: Option<(Obj, Option<(u32, usize)>, f64)> = None;
        let mut consider = |o: Obj, face: Option<(u32, usize)>, t: f64| {
            if best.as_ref().map_or(true, |b| t < b.2) {
                best = Some((o, face, t));
            }
        };
        for (e, s) in self.map.solids_owned() {
            let owner = e.map(|e| e.id);
            if !self.is_visible_solid(e, s) || !self.in_cordon(&s.bounds) || self.is_locked_solid(e, s) {
                continue;
            }
            if let Some((t, i)) = s.ray_hit(origin, dir) {
                consider(self.obj_for(owner, s.id), Some((s.id, i)), t);
            }
        }
        for e in &self.map.entities {
            if !e.is_brush_entity() && self.is_visible_entity(e) && !self.is_locked_entity(e) {
                let b = self.entity_bounds(e);
                if self.in_cordon(&b) {
                    if let Some(t) = b.ray_hit(origin, dir) {
                        // Modell: nur treffen, wo wirklich ein Dreieck ist (nicht die ganze Box)
                        match self.model_path(e).and_then(|m| self.model_tris.get(&m)) {
                            Some(tris) if !tris.is_empty() => {
                                let x = Self::entity_xform(e);
                                let hit = tris.iter().filter_map(|t| crate::disp::ray_triangle(origin, dir, x.apply(t[0]), x.apply(t[1]), x.apply(t[2]))).fold(f64::MAX, f64::min);
                                if hit < f64::MAX {
                                    consider(Obj::Entity(e.id), None, hit);
                                }
                            }
                            _ => consider(Obj::Entity(e.id), None, t),
                        }
                    }
                }
            }
        }
        best
    }

    pub fn is_selected_solid(&self, owner: Option<u32>, solid_id: u32) -> bool {
        self.sel.contains(&Obj::Solid(solid_id)) || owner.is_some_and(|e| self.sel.contains(&Obj::Entity(e)))
    }

    /// Pipette für Zielnamen: true, wenn der Klick verbraucht wurde
    pub fn handle_pick_target(&mut self, o: Option<Obj>) -> bool {
        let Some((eid, key)) = self.pick_target.clone() else { return false };
        self.pick_target = None;
        let ent = match o {
            Some(Obj::Entity(id)) => Some(id),
            Some(Obj::Solid(sid)) => self.map.solid_owner(sid).flatten(),
            None => None,
        };
        let name = ent.and_then(|id| self.map.entity(id)).and_then(|e| e.targetname().map(|s| s.to_string()));
        match name {
            Some(n) => {
                self.checkpoint();
                if let Some(e) = self.map.entity_mut(eid) {
                    e.set(&key, n.clone());
                }
                self.status = format!("{key} = {n}");
            }
            None => self.status = "Das angeklickte Objekt hat keinen Namen (targetname)".into(),
        }
        true
    }

    pub fn select(&mut self, o: Option<Obj>, additive: bool) {
        if self.handle_pick_target(o) {
            return;
        }
        if !additive {
            self.sel.clear();
        }
        if let Some(o) = o {
            let objs = self.expand(o);
            if additive && self.sel.contains(&o) {
                for x in objs {
                    self.sel.remove(&x);
                }
            } else {
                self.sel.extend(objs);
            }
        }
        self.touch();
    }

    pub fn select_face(&mut self, face: Option<(u32, usize)>, additive: bool) {
        if !additive {
            self.faces.clear();
        }
        if let Some(f) = face {
            if additive && self.faces.contains(&f) {
                self.faces.remove(&f);
            } else {
                self.faces.insert(f);
                if let Some(side) = self.map.solid(f.0).and_then(|s| s.sides.get(f.1)) {
                    self.material = side.material.clone();
                }
            }
        }
        self.touch();
    }

    pub fn select_all(&mut self) {
        self.sel = self.selectable().into_iter().map(|(o, _)| o).collect();
        self.touch();
    }

    pub fn clear_selection(&mut self) {
        self.sel.clear();
        self.faces.clear();
        self.vsel.clear();
        self.touch();
    }

    /// Alle ausgewählten Solid-IDs (inkl. Brushes ausgewählter Entities)
    pub fn selected_solid_ids(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for o in &self.sel {
            match o {
                Obj::Solid(id) => out.push(*id),
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity(*id) {
                        out.extend(e.solids.iter().map(|s| s.id));
                    }
                }
            }
        }
        out
    }

    pub fn selected_entity_ids(&self) -> Vec<u32> {
        self.sel.iter().filter_map(|o| if let Obj::Entity(id) = o { Some(*id) } else { None }).collect()
    }

    // ------------------------------------------------------------ Transformieren

    pub fn translate_selection(&mut self, d: DVec3) {
        let tex_lock = self.tex_lock;
        for o in self.sel.clone() {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid_mut(id) {
                        s.translate(d, tex_lock);
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity_mut(id) {
                        e.translate(d, tex_lock);
                    }
                }
            }
        }
        self.touch();
    }

    fn apply_xform(&mut self, x: &Xform, scale_op: bool) {
        if !x.is_valid() {
            return;
        }
        let (tl, sl) = (self.tex_lock, !scale_op || self.scale_lock);
        for o in self.sel.clone() {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid_mut(id) {
                        s.transform(x, tl, sl);
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity_mut(id) {
                        e.transform(x, tl, sl);
                    }
                }
            }
        }
        self.touch();
    }

    /// Transformation auf die Auswahl anwenden (mit Undo-Punkt)
    pub fn transform_selection(&mut self, x: &Xform, scale_op: bool) {
        if self.sel.is_empty() || !x.is_valid() {
            return;
        }
        self.checkpoint();
        self.apply_xform(x, scale_op);
    }

    /// Setzt die Auswahl auf den Stand in `base` zurück und transformiert sie (für Maus-Ziehen)
    pub fn transform_selection_from(&mut self, base: &Map, x: &Xform, scale_op: bool) {
        for o in self.sel.clone() {
            match o {
                Obj::Solid(id) => {
                    if let (Some(src), Some(dst)) = (base.solid(id).cloned(), self.map.solid_mut(id)) {
                        *dst = src;
                    }
                }
                Obj::Entity(id) => {
                    if let (Some(src), Some(dst)) = (base.entity(id).cloned(), self.map.entity_mut(id)) {
                        *dst = src;
                    }
                }
            }
        }
        self.apply_xform(x, scale_op);
    }

    /// Skalieren von einer Box auf eine andere
    pub fn scale_xform(from: Aabb, to: Aabb) -> Xform {
        let fs = from.size();
        let ts = to.size();
        let s = DVec3::new(
            if fs.x > 0.0 { ts.x / fs.x } else { 1.0 },
            if fs.y > 0.0 { ts.y / fs.y } else { 1.0 },
            if fs.z > 0.0 { ts.z / fs.z } else { 1.0 },
        );
        Xform { m: DMat3::from_diagonal(s), t: to.min - from.min * s }
    }

    pub fn selection_center(&self) -> DVec3 {
        let c = self.selection_bounds().center();
        snap_vec_grid(c, self.grid.min(8.0))
    }

    pub fn rotate_selection(&mut self, axis: usize, deg: f64) {
        let c = self.selection_center();
        self.transform_selection(&Xform::about(c, Xform::rotation(axis, deg)), false);
        self.status = format!("Gedreht um {deg}°");
    }

    pub fn flip_selection(&mut self, axis: usize) {
        let c = self.selection_bounds().center();
        self.transform_selection(&Xform::about(c, Xform::mirror(axis)), false);
        self.status = format!("Gespiegelt ({})", ["X", "Y", "Z"][axis]);
    }

    /// Alle Objekte an einer Kante der Gesamtauswahl ausrichten
    pub fn align_selection(&mut self, axis: usize, to_max: bool) {
        if self.sel.len() < 2 {
            return;
        }
        self.checkpoint();
        let all = self.selection_bounds();
        let tl = self.tex_lock;
        for o in self.sel.clone() {
            let b = self.obj_bounds(o);
            let mut d = DVec3::ZERO;
            d[axis] = if to_max { all.max[axis] - b.max[axis] } else { all.min[axis] - b.min[axis] };
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid_mut(id) {
                        s.translate(d, tl);
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity_mut(id) {
                        e.translate(d, tl);
                    }
                }
            }
        }
        self.touch();
    }

    /// Strg+B: Auswahl so verschieben, dass ihre Box am Raster liegt
    pub fn snap_selection_to_grid(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        let b = self.selection_bounds();
        let d = snap_vec_grid(b.min, self.grid) - b.min;
        if d == DVec3::ZERO {
            return;
        }
        self.checkpoint();
        self.translate_selection(d);
        self.status = "Am Raster ausgerichtet".into();
    }

    // ------------------------------------------------------------ Erzeugen / Löschen

    pub fn create_block(&mut self, bounds: Aabb) {
        let s = bounds.size();
        if s.x < 1.0 || s.y < 1.0 || s.z < 1.0 {
            return;
        }
        self.checkpoint();
        let mat = self.material.clone();
        let solids = primitives::create(&mut self.map, bounds, &self.shape, &mat, self.tex_scale);
        if solids.is_empty() {
            self.status = "Form konnte nicht erzeugt werden".into();
            return;
        }
        self.sel.clear();
        // mehrteilige Formen (Bogen, Torus) als Gruppe
        let gid = (solids.len() > 1).then(|| {
            let id = self.map.alloc_id();
            self.map.groups.push(Group { id, editor: Default::default() });
            id
        });
        for mut solid in solids {
            solid.editor.groupid = gid;
            self.sel.insert(Obj::Solid(solid.id));
            self.map.world.solids.push(solid.into());
        }
        self.last_box = bounds;
        self.status = format!("{} erstellt: {} × {} × {}", self.shape.shape.label(), s.x, s.y, s.z);
        self.touch();
    }

    fn apply_fgd_defaults(&self, e: &mut Entity) {
        let class = e.classname().to_string();
        for p in self.fgd.all_props(&class) {
            if p.kind == "flags" {
                let v: u32 = p.choices.iter().filter(|c| c.default_on).filter_map(|c| c.value.parse::<u32>().ok()).sum();
                e.set("spawnflags", v.to_string());
            } else if !p.default.is_empty() && !p.name.eq_ignore_ascii_case("origin") {
                e.set(&p.name, p.default.clone());
            }
        }
    }

    /// Allen Entities (außer worldspawn) fehlende Schlüssel mit ihrem FGD-Standardwert geben – so,
    /// wie Hammer sie beim Anlegen schreibt. Liefert eine Beschreibung je ergänztem Schlüssel
    /// („light: _light“). Vorhandene Werte bleiben unangetastet; „model“ wird nie erfunden.
    pub fn fill_fgd_defaults(&mut self) -> Vec<String> {
        let mut todo: Vec<(u32, String, String)> = Vec::new();
        for e in self.map.entities.iter() {
            let class = e.classname().to_string();
            for p in self.fgd.all_props(&class) {
                let k = p.name.to_ascii_lowercase();
                if matches!(k.as_str(), "origin" | "model" | "classname") || e.get(&p.name).is_some() {
                    continue;
                }
                let v = if p.kind == "flags" {
                    if e.get("spawnflags").is_some() {
                        continue;
                    }
                    p.choices.iter().filter(|c| c.default_on).filter_map(|c| c.value.parse::<u32>().ok()).sum::<u32>().to_string()
                } else if p.default.trim().is_empty() {
                    continue;
                } else {
                    p.default.clone()
                };
                todo.push((e.id, if p.kind == "flags" { "spawnflags".into() } else { p.name.clone() }, v));
            }
            // Lichter brauchen Farbe und Helligkeit, auch wenn die FGD nichts vorgibt
            if matches!(class.to_ascii_lowercase().as_str(), "light" | "light_spot" | "light_environment" | "light_dynamic") && e.get("_light").is_none_or(|v| v.trim().is_empty()) && !todo.iter().any(|t| t.0 == e.id && t.1 == "_light") {
                todo.push((e.id, "_light".into(), "255 255 255 200".into()));
            }
        }
        if todo.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (id, k, v) in todo {
            if let Some(e) = self.map.entity_mut(id) {
                let line = format!("{}: {k}", e.classname());
                e.set(&k, v);
                if !out.contains(&line) {
                    out.push(line);
                }
            }
        }
        self.dirty = true;
        self.touch();
        out
    }

    pub fn create_point_entity(&mut self, pos: DVec3) -> u32 {
        let class = self.entity_class.clone();
        self.checkpoint();
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, &class);
        self.apply_fgd_defaults(&mut e);
        if e.get("angles").is_none() && self.fgd.all_props(&class).iter().any(|p| p.name.eq_ignore_ascii_case("angles")) {
            e.set("angles", "0 0 0");
        }
        if class.starts_with("prop_") && !self.next_model.is_empty() {
            e.set("model", self.next_model.clone());
        }
        // Lichter immer mit Farbe und Helligkeit anlegen (auch wenn die FGD keinen Standard nennt)
        let lc = class.to_ascii_lowercase();
        if matches!(lc.as_str(), "light" | "light_spot" | "light_environment" | "light_dynamic") && e.get("_light").is_none_or(|v| v.trim().is_empty()) {
            e.set("_light", "255 255 255 200");
        }
        if lc == "light_environment" && e.get("_ambient").is_none_or(|v| v.trim().is_empty()) {
            e.set("_ambient", "255 255 255 20");
        }
        e.set("origin", fmt_vec(pos));
        self.map.entities.push(e.into());
        self.sel.clear();
        self.sel.insert(Obj::Entity(id));
        self.status = format!("{class} platziert bei {}", fmt_vec(pos));
        self.touch();
        id
    }

    pub fn create_decal(&mut self, pos: DVec3) {
        self.checkpoint();
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, "infodecal");
        e.set("texture", self.material.to_ascii_lowercase());
        e.set("origin", fmt_vec(pos));
        self.map.entities.push(e.into());
        self.sel.clear();
        self.sel.insert(Obj::Entity(id));
        self.status = format!("Decal {} platziert", self.material);
        self.touch();
    }

    /// Overlay auf eine Fläche legen (size = halbe Breite/Höhe)
    pub fn create_overlay(&mut self, pos: DVec3, normal: DVec3, side_id: u32, half: DVec2) {
        self.checkpoint();
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, "info_overlay");
        let mut u = normal.cross(DVec3::Z);
        if u.length() < 0.1 {
            u = DVec3::X;
        }
        let u = u.normalize();
        let v = normal.cross(u).normalize();
        e.set("material", self.material.to_ascii_lowercase());
        e.set("sides", side_id.to_string());
        e.set("RenderOrder", "0");
        e.set("StartU", "0");
        e.set("EndU", "1");
        e.set("StartV", "0");
        e.set("EndV", "1");
        e.set("BasisOrigin", fmt_vec(pos));
        e.set("BasisU", fmt_vec(u));
        e.set("BasisV", fmt_vec(v));
        e.set("BasisNormal", fmt_vec(normal));
        e.set("uv0", format!("{} {} 0", -half.x, -half.y));
        e.set("uv1", format!("{} {} 0", -half.x, half.y));
        e.set("uv2", format!("{} {} 0", half.x, half.y));
        e.set("uv3", format!("{} {} 0", half.x, -half.y));
        e.set("origin", fmt_vec(pos));
        self.map.entities.push(e.into());
        self.sel.clear();
        self.sel.insert(Obj::Entity(id));
        self.status = format!("Overlay {} platziert", self.material);
        self.touch();
    }

    pub fn delete_selection(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        let sel = std::mem::take(&mut self.sel);
        let solid_ids: BTreeSet<u32> = sel.iter().filter_map(|o| if let Obj::Solid(id) = o { Some(*id) } else { None }).collect();
        self.map.world.solids.retain(|s| !solid_ids.contains(&s.id));
        for e in &mut self.map.entities {
            e.solids.retain(|s| !solid_ids.contains(&s.id));
        }
        // Brush-Entities ohne Brushes entfernen
        let solid_class = |e: &Entity| self.fgd.class(e.classname()).is_some_and(|c| c.kind == ClassKind::Solid);
        let empty: Vec<u32> = self.map.entities.iter().filter(|e| e.solids.is_empty() && solid_class(e)).map(|e| e.id).collect();
        self.map.entities.retain(|e| !sel.contains(&Obj::Entity(e.id)) && !empty.contains(&e.id));
        self.faces.clear();
        self.vsel.clear();
        self.status = format!("{} gelöscht", crate::i18n::count_de(sel.len(), "Objekt", "Objekte"));
        self.touch();
    }

    pub fn copy(&mut self) {
        self.clipboard.clear();
        for o in &self.sel {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid(*id) {
                        self.clipboard.push(ClipItem::Solid(s.clone()));
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity(*id) {
                        self.clipboard.push(ClipItem::Entity(e.clone()));
                    }
                }
            }
        }
        self.paste_count = 0;
        self.status = format!("{} kopiert", crate::i18n::count_de(self.clipboard.len(), "Objekt", "Objekte"));
    }

    fn insert_items(&mut self, items: Vec<ClipItem>, x: &Xform) -> Vec<Obj> {
        let mut out = Vec::new();
        let tl = self.tex_lock;
        for item in items {
            match item {
                ClipItem::Solid(mut s) => {
                    self.map.reassign_ids(&mut s);
                    s.editor.hidden = false;
                    s.transform(x, tl, true);
                    out.push(Obj::Solid(s.id));
                    self.map.world.solids.push(s.into());
                }
                ClipItem::Entity(mut e) => {
                    self.map.reassign_entity_ids(&mut e);
                    e.editor.hidden = false;
                    e.transform(x, tl, true);
                    out.push(Obj::Entity(e.id));
                    self.map.entities.push(e.into());
                }
            }
        }
        out
    }

    pub(crate) fn insert_clip_items(&mut self, items: Vec<ClipItem>, x: &Xform) -> Vec<Obj> {
        self.insert_items(items, x)
    }

    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.checkpoint();
        self.paste_count += 1;
        let off = DVec3::new(self.grid, self.grid, 0.0) * self.paste_count as f64;
        let items = self.clipboard.clone();
        self.sel = self.insert_items(items, &Xform::translate(off)).into_iter().collect();
        self.status = "Eingefügt".into();
        self.touch();
    }

    /// Paste Special: mehrere Kopien mit Versatz und Drehung (um die Kopiermitte)
    /// Ausmaße der Zwischenablage (für die Vorschau in „Inhalte einfügen“)
    pub fn clipboard_bounds(&self) -> Option<Aabb> {
        let b = self.clipboard.iter().fold(Aabb::EMPTY, |b, it| match it {
            ClipItem::Solid(s) => b.union(&s.bounds),
            ClipItem::Entity(e) => b.union(&self.entity_bounds(e)),
        });
        (!b.is_empty()).then_some(b)
    }

    pub fn paste_special(&mut self, copies: u32, offset: DVec3, rotation: DVec3, make_group: bool) {
        if self.clipboard.is_empty() || copies == 0 {
            return;
        }
        self.checkpoint();
        let center = self.clipboard.iter().fold(Aabb::EMPTY, |b, it| match it {
            ClipItem::Solid(s) => b.union(&s.bounds),
            ClipItem::Entity(e) => b.union(&self.entity_bounds(e)),
        });
        let c = center.center();
        self.sel.clear();
        for i in 1..=copies {
            let k = i as f64;
            let rot = Xform::rotation(2, rotation.z * k) * Xform::rotation(1, rotation.y * k) * Xform::rotation(0, rotation.x * k);
            let x = Xform::about(c, rot).then(&Xform::translate(offset * k));
            let items = self.clipboard.clone();
            let objs = self.insert_items(items, &x);
            if make_group {
                let gid = self.map.alloc_id();
                self.map.groups.push(Group { id: gid, editor: Default::default() });
                for o in &objs {
                    self.set_group(*o, Some(gid));
                }
            }
            self.sel.extend(objs);
        }
        self.status = format!("{copies} Kopie(n) eingefügt");
        self.touch();
    }

    /// Ausgewählte Brushes zu einer Brush-Entity machen (Strg+T)
    pub fn tie_to_entity(&mut self, class: &str) {
        let solids = self.selected_solid_ids();
        if solids.is_empty() {
            self.status = "Keine Brushes ausgewählt".into();
            return;
        }
        self.checkpoint();
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, class);
        self.apply_fgd_defaults(&mut e);
        e.editor.color = self.fgd.resolve_color(class).unwrap_or([80, 200, 255]);
        for sid in solids {
            if let Some(s) = self.map.take_solid(sid) {
                e.solids.push(s.into());
            }
        }
        // leer gewordene Entities löschen
        let gone: Vec<u32> = self.sel.iter().filter_map(|o| if let Obj::Entity(id) = o { Some(*id) } else { None }).collect();
        self.map.entities.retain(|x| !(gone.contains(&x.id) && x.solids.is_empty()));
        self.map.entities.push(e.into());
        self.sel.clear();
        self.sel.insert(Obj::Entity(id));
        self.status = format!("In {class} umgewandelt");
        self.touch();
    }

    /// Brush-Entities zurück in die Welt (Strg+W)
    pub fn move_to_world(&mut self) {
        let ents: Vec<u32> = self.selected_entity_ids().into_iter().filter(|id| self.map.entity(*id).is_some_and(|e| e.is_brush_entity())).collect();
        let loose: Vec<u32> = self.sel.iter().filter_map(|o| if let Obj::Solid(id) = o { Some(*id) } else { None }).filter(|id| self.map.solid_owner(*id) != Some(None)).collect();
        if ents.is_empty() && loose.is_empty() {
            return;
        }
        self.checkpoint();
        self.sel.clear();
        for sid in loose {
            if let Some(s) = self.map.take_solid(sid) {
                self.sel.insert(Obj::Solid(s.id));
                self.map.world.solids.push(s.into());
            }
        }
        let mut i = 0;
        while i < self.map.entities.len() {
            if ents.contains(&self.map.entities[i].id) || self.map.entities[i].solids.is_empty() && self.fgd.class(self.map.entities[i].classname()).is_some_and(|c| c.kind == ClassKind::Solid) {
                let e = self.map.entities.remove(i).into_inner();
                for s in e.solids {
                    self.sel.insert(Obj::Solid(s.id));
                    self.map.world.solids.push(s);
                }
            } else {
                i += 1;
            }
        }
        self.status = "In die Welt verschoben".into();
        self.touch();
    }

    /// Ersetzt einen Solid (egal wo) durch mehrere neue an derselben Stelle
    fn replace_solid(&mut self, id: u32, new: Vec<Solid>) {
        let owner = self.map.solid_owner(id);
        let _ = self.map.take_solid(id);
        match owner {
            Some(Some(eid)) => {
                if let Some(e) = self.map.entity_mut(eid) {
                    e.solids.extend(new.into_iter().map(Shared::new));
                }
            }
            _ => self.map.world.solids.extend(new.into_iter().map(Shared::new)),
        }
    }

    /// Echtes Aushöhlen (Strg+H)
    pub fn hollow(&mut self, thickness: f64) {
        let ids: Vec<u32> = self.selected_solid_ids().into_iter().filter(|id| self.map.solid(*id).is_some_and(|s| !s.has_disp())).collect();
        if ids.is_empty() {
            self.status = "Keine Brushes zum Aushöhlen ausgewählt".into();
            return;
        }
        self.checkpoint();
        let mut new_sel = BTreeSet::new();
        for id in ids {
            let Some(s) = self.map.solid(id).cloned() else { continue };
            let walls = csg::hollow(&mut self.map, &s, thickness, self.tex_scale);
            if walls.is_empty() {
                continue;
            }
            for w in &walls {
                new_sel.insert(Obj::Solid(w.id));
            }
            self.replace_solid(id, walls);
        }
        self.sel = new_sel;
        self.status = format!("Ausgehöhlt (Wandstärke {thickness})");
        self.touch();
    }

    /// Carve: ausgewählte Brushes aus allen anderen herausschneiden
    pub fn carve(&mut self) {
        let cutters: Vec<Solid> = self.selected_solid_ids().into_iter().filter_map(|id| self.map.solid(id).cloned()).collect();
        if cutters.is_empty() {
            return;
        }
        self.checkpoint();
        let cutter_ids: Vec<u32> = cutters.iter().map(|c| c.id).collect();
        let mut count = 0;
        for c in &cutters {
            let targets: Vec<Solid> = self
                .map
                .all_solids()
                .filter(|(_, s)| !cutter_ids.contains(&s.id) && !s.has_disp() && s.bounds.intersects(&c.bounds))
                .map(|(_, s)| s.clone())
                .collect();
            for t in targets {
                if let Some(pieces) = csg::carve(&mut self.map, &t, c, self.tex_scale) {
                    self.replace_solid(t.id, pieces);
                    count += 1;
                }
            }
        }
        self.status = format!("Carve: {} geschnitten", crate::i18n::count_de(count, "Brush", "Brushes"));
        self.touch();
    }

    // ------------------------------------------------------------ Clip-Werkzeug

    pub fn clip_plane(&self) -> Option<Plane> {
        if self.clip3.len() == 3 {
            return Plane::from_points(&[self.clip3[0], self.clip3[1], self.clip3[2]]);
        }
        let c = self.clip?;
        if c.a.distance(c.b) < 0.5 {
            return None;
        }
        let (h, v, d) = c.axis.axes();
        let mut a = DVec3::ZERO;
        a[h] = c.a.x;
        a[v] = c.a.y;
        let mut b = DVec3::ZERO;
        b[h] = c.b.x;
        b[v] = c.b.y;
        let mut depth = DVec3::ZERO;
        depth[d] = 1.0;
        let n = (b - a).cross(depth).normalize_or_zero();
        (n != DVec3::ZERO).then(|| Plane { normal: n, dist: n.dot(a) })
    }

    /// Vorschau: (behaltene Teile, verworfene Teile)
    pub fn clip_preview(&self) -> (Vec<Solid>, Vec<Solid>) {
        let Some(pl) = self.clip_plane() else { return (vec![], vec![]) };
        let c = [pl.normal.x.to_bits(), pl.normal.y.to_bits(), pl.normal.z.to_bits(), pl.dist.to_bits()];
        if let Some((cl, cm, rev, k, d)) = self.clip_cache.borrow().as_ref() {
            if *cl == c && *cm == self.clip_mode && *rev == self.rev {
                return (k.clone(), d.clone());
            }
        }
        let mut tmp = Map::new();
        tmp.next_id = self.map.next_id;
        let mut keep = Vec::new();
        let mut drop = Vec::new();
        for id in self.selected_solid_ids() {
            let Some(s) = self.map.solid(id).cloned() else { continue };
            if s.has_disp() {
                continue;
            }
            let (f, b) = csg::split(&mut tmp, &s, pl, "x", 0.25);
            for (part, front) in [(f, true), (b, false)] {
                if let Some(p) = part {
                    let kept = match self.clip_mode {
                        ClipMode::Both => true,
                        ClipMode::Front => front,
                        ClipMode::Back => !front,
                    };
                    if kept {
                        keep.push(p);
                    } else {
                        drop.push(p);
                    }
                }
            }
        }
        *self.clip_cache.borrow_mut() = Some((c, self.clip_mode, self.rev, keep.clone(), drop.clone()));
        (keep, drop)
    }

    pub fn apply_clip(&mut self) {
        let Some(pl) = self.clip_plane() else { return };
        let mode = self.clip_mode;
        self.clip_with_plane(pl, mode);
        self.clip = None;
        self.clip3.clear();
    }

    /// Punkt für den 3-Punkt-Clip setzen (Raster bzw. nächster Eckpunkt); der vierte beginnt neu
    pub fn clip3_add(&mut self, p: DVec3) {
        let mut q = p;
        if self.vertex_snap {
            let near = self.map.all_solids().filter(|(_, s)| s.bounds.intersects(&crate::geometry::Aabb::new(p - DVec3::splat(12.0), p + DVec3::splat(12.0)))).flat_map(|(_, s)| s.vertices()).min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)));
            if let Some(v) = near.filter(|v| v.distance(p) < 12.0) {
                q = v;
            }
        } else if self.snap {
            let g = self.snap_grid();
            q = DVec3::new(crate::geometry::snap(p.x, g), crate::geometry::snap(p.y, g), crate::geometry::snap(p.z, g));
        }
        if self.clip3.len() >= 3 {
            self.clip3.clear();
        }
        self.clip = None;
        self.clip3.push(q);
        self.status = if self.clip3.len() < 3 {
            crate::i18n::tr_dyn(&format!("Clip: Punkt {} von 3 – auf Flächen klicken", self.clip3.len()))
        } else if self.clip_plane().is_some() {
            crate::i18n::tr("Clip-Ebene steht – Enter schneidet, Shift+X wechselt die Seite").into()
        } else {
            crate::i18n::tr("Die drei Punkte liegen auf einer Linie – neu setzen").into()
        };
        self.touch();
    }

    /// Ausgewählte Brushes an einer Ebene schneiden. Liefert die IDs der entstandenen Teile.
    pub fn clip_with_plane(&mut self, pl: Plane, mode: ClipMode) -> Vec<u32> {
        let ids: Vec<u32> = self.selected_solid_ids().into_iter().filter(|id| self.map.solid(*id).is_some_and(|s| !s.has_disp())).collect();
        if ids.is_empty() {
            self.status = "Zum Schneiden zuerst Brushes auswählen".into();
            return Vec::new();
        }
        self.checkpoint();
        let mut new_sel = BTreeSet::new();
        for id in ids {
            let Some(s) = self.map.solid(id).cloned() else { continue };
            let mat = csg::main_material(&s);
            let (f, b) = csg::split(&mut self.map, &s, pl, &mat, self.tex_scale);
            let mut parts = Vec::new();
            match mode {
                ClipMode::Both => parts.extend(f.into_iter().chain(b)),
                ClipMode::Front => parts.extend(f),
                ClipMode::Back => parts.extend(b),
            }
            for p in &parts {
                new_sel.insert(Obj::Solid(p.id));
            }
            self.replace_solid(id, parts);
        }
        // Entities ohne Brushes aufräumen
        self.map.entities.retain(|e| !(e.solids.is_empty() && self.fgd.class(e.classname()).is_some_and(|c| c.kind == ClassKind::Solid)));
        let out: Vec<u32> = new_sel.iter().map(|o| match o {
            Obj::Solid(id) | Obj::Entity(id) => *id,
        }).collect();
        self.sel = new_sel;
        self.status = "Geschnitten".into();
        self.touch();
        out
    }

    // ------------------------------------------------------------ Vertex-Werkzeug

    /// Eckpunkte aller ausgewählten Brushes
    pub fn vertex_handles(&self) -> Vec<(u32, DVec3)> {
        let mut out = Vec::new();
        for id in self.selected_solid_ids() {
            if let Some(s) = self.map.solid(id) {
                if !s.has_disp() {
                    out.extend(s.vertices().into_iter().map(|v| (id, v)));
                }
            }
        }
        out
    }

    /// Kanten-Mittelpunkte: (Solid, Mitte, Endpunkt a, Endpunkt b)
    pub fn edge_handles(&self) -> Vec<(u32, DVec3, DVec3, DVec3)> {
        let mut out = Vec::new();
        for id in self.selected_solid_ids() {
            if let Some(s) = self.map.solid(id) {
                if !s.has_disp() {
                    out.extend(s.edges().into_iter().map(|(a, b)| (id, (a + b) * 0.5, a, b)));
                }
            }
        }
        out
    }

    /// Verschiebt ausgewählte Eckpunkte ausgehend vom Stand in `base`. false = ungültig.
    pub fn move_vertices_from(&mut self, base: &Map, base_sel: &[(u32, DVec3)], delta: DVec3) -> bool {
        let mut ok = true;
        let ids: BTreeSet<u32> = base_sel.iter().map(|v| v.0).collect();
        let mut new_solids = Vec::new();
        for id in &ids {
            let Some(old) = base.solid(*id) else { continue };
            let verts: Vec<DVec3> = old.vertices().into_iter().map(|v| if base_sel.iter().any(|(sid, p)| sid == id && p.distance(v) < 0.01) { v + delta } else { v }).collect();
            let mut tmp_map = self.map.clone();
            match csg::rebuild_from_vertices(&mut tmp_map, old, &verts) {
                Some(s) => {
                    self.map.next_id = self.map.next_id.max(tmp_map.next_id);
                    new_solids.push(s);
                }
                None => ok = false,
            }
        }
        if !ok {
            return false;
        }
        for s in new_solids {
            if let Some(dst) = self.map.solid_mut(s.id) {
                *dst = s;
            }
        }
        self.vsel = base_sel.iter().map(|(id, p)| (*id, *p + delta)).collect();
        self.touch();
        true
    }

    // ------------------------------------------------------------ Texturen

    /// Material auf ausgewählte Flächen bzw. komplette Brushes anwenden
    pub fn apply_material(&mut self) {
        let mat = self.material.clone();
        if !self.faces.is_empty() {
            self.checkpoint();
            for (sid, i) in self.faces.clone() {
                if let Some(side) = self.map.solid_mut(sid).and_then(|s| s.sides.get_mut(i)) {
                    side.material = mat.clone();
                }
            }
        } else if !self.sel.is_empty() {
            self.checkpoint();
            for id in self.selected_solid_ids() {
                if let Some(s) = self.map.solid_mut(id) {
                    s.set_material_all(&mat);
                }
            }
        } else {
            return;
        }
        self.status = format!("Material {mat} angewendet");
        self.touch();
    }

    /// Texturen suchen & ersetzen. Liefert die Anzahl ersetzter Flächen.
    pub fn replace_materials(&mut self, find: &str, replace: &str, only_selection: bool, partial: bool) -> usize {
        let f = find.to_ascii_lowercase();
        if f.is_empty() {
            return 0;
        }
        let ids: Vec<u32> = if only_selection { self.selected_solid_ids() } else { self.map.all_solids().map(|(_, s)| s.id).collect() };
        self.checkpoint();
        let mut n = 0;
        for id in ids {
            if let Some(s) = self.map.solid_mut(id) {
                for side in &mut s.sides {
                    let m = side.material.to_ascii_lowercase();
                    if partial && m.contains(&f) {
                        side.material = m.replace(&f, &replace.to_ascii_lowercase()).to_uppercase();
                        n += 1;
                    } else if !partial && m == f {
                        side.material = replace.to_uppercase();
                        n += 1;
                    }
                }
            }
        }
        self.status = format!("{} ersetzt", crate::i18n::count_de(n, "Fläche", "Flächen"));
        self.touch();
        n
    }

    /// Texturen ausrichten. `dims` liefert die Texturgröße eines Materials.
    pub fn justify_faces(&mut self, mode: Justify, treat_as_one: bool, dims: &dyn Fn(&str) -> (f64, f64)) {
        if self.faces.is_empty() {
            return;
        }
        self.checkpoint();
        let faces: Vec<(u32, usize)> = self.faces.iter().copied().collect();
        // Bereich in Weltkoordinaten entlang der Achsen (gemeinsam oder pro Fläche)
        let range = |map: &Map, list: &[(u32, usize)]| -> Option<(f64, f64, f64, f64)> {
            let (sid, i) = list[0];
            let side = map.solid(sid)?.sides.get(i)?.clone();
            let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for (sid, i) in list {
                let s = map.solid(*sid)?;
                for p in s.polys.get(*i)? {
                    let u = p.dot(side.uaxis.axis);
                    let v = p.dot(side.vaxis.axis);
                    u0 = u0.min(u);
                    u1 = u1.max(u);
                    v0 = v0.min(v);
                    v1 = v1.max(v);
                }
            }
            Some((u0, u1, v0, v1))
        };
        let groups: Vec<Vec<(u32, usize)>> = if treat_as_one { vec![faces.clone()] } else { faces.iter().map(|f| vec![*f]).collect() };
        for g in groups {
            let Some((u0, u1, v0, v1)) = range(&self.map, &g) else { continue };
            for (sid, i) in &g {
                let Some(side) = self.map.solid_mut(*sid).and_then(|s| s.sides.get_mut(*i)) else { continue };
                let (w, h) = dims(&side.material);
                let (ua, va) = (&mut side.uaxis, &mut side.vaxis);
                match mode {
                    Justify::Fit => {
                        ua.scale = ((u1 - u0) / w).max(0.001);
                        va.scale = ((v1 - v0) / h).max(0.001);
                        ua.shift = -u0 / ua.scale;
                        va.shift = -v0 / va.scale;
                    }
                    Justify::Left => ua.shift = -u0 / ua.scale,
                    Justify::Right => ua.shift = w - u1 / ua.scale,
                    Justify::Top => va.shift = -v0 / va.scale,
                    Justify::Bottom => va.shift = h - v1 / va.scale,
                    Justify::Center => {
                        ua.shift = w / 2.0 - (u0 + u1) / 2.0 / ua.scale;
                        va.shift = h / 2.0 - (v0 + v1) / 2.0 / va.scale;
                    }
                }
                ua.shift = ua.shift.rem_euclid(w);
                va.shift = va.shift.rem_euclid(h);
            }
        }
        self.touch();
    }

    /// Texturachsen an Welt (true) oder Fläche (false) ausrichten
    pub fn align_faces(&mut self, world: bool) {
        if self.faces.is_empty() {
            return;
        }
        self.checkpoint();
        for (sid, i) in self.faces.clone() {
            if let Some(s) = self.map.solid_mut(sid) {
                let n = s.planes.get(i).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
                if let Some(side) = s.sides.get_mut(i) {
                    let (u, v) = if world { default_tex_axes(n, side.uaxis.scale) } else { face_tex_axes(n, side.uaxis.scale) };
                    side.uaxis = TexAxisKeep::keep(&side.uaxis, u);
                    side.vaxis = TexAxisKeep::keep(&side.vaxis, v);
                    side.rotation = 0.0;
                }
            }
        }
        self.touch();
    }

    /// Alt+Rechtsklick: Material und Ausrichtung der ersten ausgewählten Fläche nahtlos auf eine Nachbarfläche übertragen
    pub fn wrap_texture(&mut self, target: (u32, usize)) -> bool {
        let Some(&src) = self.faces.iter().next() else { return false };
        let (Some(s_solid), Some(t_solid)) = (self.map.solid(src.0), self.map.solid(target.0)) else { return false };
        let (Some(sp), Some(tp)) = (s_solid.planes.get(src.1).copied().flatten(), t_solid.planes.get(target.1).copied().flatten()) else { return false };
        let src_side = s_solid.sides[src.1].clone();
        let sp_poly = s_solid.polys[src.1].clone();
        let tp_poly = t_solid.polys[target.1].clone();
        // gemeinsamer Punkt (Kante) als Drehpunkt
        let shared = sp_poly.iter().find(|a| tp_poly.iter().any(|b| b.distance(**a) < 0.1)).copied();
        let pivot = shared.unwrap_or_else(|| sp_poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / sp_poly.len().max(1) as f64);
        let q = DQuat::from_rotation_arc(sp.normal, tp.normal);
        let x = Xform::about(pivot, DMat3::from_quat(q));
        let mut u = src_side.uaxis.clone();
        let mut v = src_side.vaxis.clone();
        crate::map::transform_tex_axis_pub(&mut u, &x, pivot, true);
        crate::map::transform_tex_axis_pub(&mut v, &x, pivot, true);
        self.checkpoint();
        if let Some(side) = self.map.solid_mut(target.0).and_then(|s| s.sides.get_mut(target.1)) {
            side.material = src_side.material.clone();
            side.uaxis = u;
            side.vaxis = v;
            side.rotation = src_side.rotation;
        }
        self.touch();
        true
    }

    // ------------------------------------------------------------ Gruppen & Ausblenden

    fn set_group(&mut self, o: Obj, g: Option<u32>) {
        match o {
            Obj::Solid(id) => {
                if let Some(s) = self.map.solid_mut(id) {
                    s.editor.groupid = g;
                }
            }
            Obj::Entity(id) => {
                if let Some(e) = self.map.entity_mut(id) {
                    e.editor.groupid = g;
                }
            }
        }
    }

    pub fn group_selection(&mut self) {
        if self.sel.len() < 2 {
            self.status = "Zum Gruppieren mindestens zwei Objekte auswählen".into();
            return;
        }
        self.checkpoint();
        let gid = self.map.alloc_id();
        self.map.groups.push(Group { id: gid, editor: Default::default() });
        for o in self.sel.clone() {
            self.set_group(o, Some(gid));
        }
        self.status = format!("Gruppe {gid} erstellt ({} Objekte)", self.sel.len());
        self.touch();
    }

    pub fn ungroup_selection(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        for o in self.sel.clone() {
            self.set_group(o, None);
        }
        let used: BTreeSet<u32> = self.map.all_solids().filter_map(|(_, s)| s.editor.groupid).chain(self.map.entities.iter().filter_map(|e| e.editor.groupid)).collect();
        self.map.groups.retain(|g| used.contains(&g.id));
        self.status = "Gruppe aufgelöst".into();
        self.touch();
    }

    fn set_hidden(&mut self, o: Obj, h: bool) {
        match o {
            Obj::Solid(id) => {
                if let Some(s) = self.map.solid_mut(id) {
                    s.editor.hidden = h;
                }
            }
            Obj::Entity(id) => {
                if let Some(e) = self.map.entity_mut(id) {
                    e.editor.hidden = h;
                }
            }
        }
    }

    pub fn hide_selection(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        for o in std::mem::take(&mut self.sel) {
            self.set_hidden(o, true);
        }
        self.status = "Ausgeblendet (U = alles einblenden)".into();
        self.touch();
    }

    pub fn hide_unselected(&mut self) {
        self.checkpoint();
        for (o, _) in self.selectable() {
            if !self.sel.contains(&o) {
                self.set_hidden(o, true);
            }
        }
        self.touch();
    }

    pub fn unhide_all(&mut self) {
        self.checkpoint();
        for s in &mut self.map.world.solids {
            s.editor.hidden = false;
        }
        for e in &mut self.map.entities {
            e.editor.hidden = false;
            for s in &mut e.solids {
                s.editor.hidden = false;
            }
        }
        self.status = "Alles eingeblendet".into();
        self.touch();
    }

    // ------------------------------------------------------------ Visgroups

    fn obj_visgroups_mut(&mut self, o: Obj) -> Option<&mut Vec<u32>> {
        match o {
            Obj::Solid(id) => self.map.solid_mut(id).map(|s| &mut s.editor.visgroups),
            Obj::Entity(id) => self.map.entity_mut(id).map(|e| &mut e.editor.visgroups),
        }
    }

    pub fn create_visgroup(&mut self, name: &str) -> u32 {
        self.checkpoint();
        let id = self.map.alloc_id();
        let color = crate::map::random_color(id);
        self.map.visgroups.push(Visgroup { id, name: name.to_string(), color, parent: None, visible: true, locked: false });
        for o in self.sel.clone() {
            if let Some(v) = self.obj_visgroups_mut(o) {
                v.push(id);
            }
        }
        self.touch();
        id
    }

    pub fn add_selection_to_visgroup(&mut self, vg: u32, add: bool) {
        self.checkpoint();
        for o in self.sel.clone() {
            if let Some(v) = self.obj_visgroups_mut(o) {
                v.retain(|x| *x != vg);
                if add {
                    v.push(vg);
                }
            }
        }
        self.touch();
    }

    pub fn select_visgroup(&mut self, vg: u32) {
        self.sel.clear();
        for s in &self.map.world.solids {
            if s.editor.visgroups.contains(&vg) {
                self.sel.insert(Obj::Solid(s.id));
            }
        }
        for e in &self.map.entities {
            if e.editor.visgroups.contains(&vg) {
                self.sel.insert(Obj::Entity(e.id));
            }
        }
        self.touch();
    }

    pub fn delete_visgroup(&mut self, vg: u32) {
        self.checkpoint();
        let parent = self.map.visgroups.iter().find(|v| v.id == vg).and_then(|v| v.parent);
        for v in &mut self.map.visgroups {
            if v.parent == Some(vg) {
                v.parent = parent;
            }
        }
        self.map.visgroups.retain(|v| v.id != vg);
        let strip = |l: &mut Vec<u32>| l.retain(|x| *x != vg);
        for s in &mut self.map.world.solids {
            strip(&mut s.editor.visgroups);
        }
        for e in &mut self.map.entities {
            strip(&mut e.editor.visgroups);
            for s in &mut e.solids {
                strip(&mut s.editor.visgroups);
            }
        }
        self.touch();
    }

    pub fn set_visgroup_visible(&mut self, vg: u32, visible: bool) {
        if let Some(v) = self.map.visgroups.iter_mut().find(|v| v.id == vg) {
            v.visible = visible;
            self.dirty = true;
            self.touch();
        }
    }

    // ------------------------------------------------------------ Prefabs

    pub fn save_prefab(&self, path: &Path) -> Result<usize, String> {
        let mut m = Map::new();
        m.next_id = self.map.next_id;
        for o in &self.sel {
            match o {
                Obj::Solid(id) => {
                    if let Some(s) = self.map.solid(*id) {
                        m.world.solids.push(s.clone().into());
                    }
                }
                Obj::Entity(id) => {
                    if let Some(e) = self.map.entity(*id) {
                        m.entities.push(e.clone().into());
                    }
                }
            }
        }
        if m.world.solids.is_empty() && m.entities.is_empty() {
            return Err("Nichts ausgewählt".into());
        }
        let n = m.world.solids.len() + m.entities.len();
        std::fs::write(path, m.to_vmf()).map_err(|e| e.to_string())?;
        Ok(n)
    }

    /// Prefab am Arbeitspunkt einfügen (Unterkante auf Arbeitshöhe)
    #[allow(dead_code)]
    pub fn insert_prefab(&mut self, path: &Path) -> Result<(), String> {
        let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let pm = Map::from_vmf(&src)?;
        let mut items: Vec<ClipItem> = pm.world.solids.iter().map(|s| ClipItem::Solid((**s).clone())).collect();
        items.extend(pm.entities.iter().map(|e| ClipItem::Entity((**e).clone())));
        let b = items.iter().fold(Aabb::EMPTY, |b, it| match it {
            ClipItem::Solid(s) => b.union(&s.bounds),
            ClipItem::Entity(e) => b.union(&self.entity_bounds(e)),
        });
        if b.is_empty() {
            return Err("Prefab ist leer".into());
        }
        let _ = items;
        let wp = self.work_point;
        self.insert_prefab_map(pm, wp, None)?;
        self.status = format!("Prefab {} eingefügt", path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        Ok(())
    }

    // ------------------------------------------------------------ Displacements

    pub fn create_disp(&mut self, power: u32) {
        let faces: Vec<(u32, usize)> = self.faces.iter().copied().collect();
        if faces.is_empty() {
            self.status = "Zuerst Flächen im Flächen-Werkzeug auswählen".into();
            return;
        }
        self.checkpoint();
        let mut made = 0;
        let mut skipped = 0;
        for (sid, i) in faces {
            let Some(s) = self.map.solid_mut(sid) else { continue };
            let (Some(poly), Some(Some(pl))) = (s.polys.get(i).cloned(), s.planes.get(i).copied()) else { continue };
            match Disp::new_flat(power, &poly, pl.normal) {
                Some(d) => {
                    s.sides[i].disp = Some(d);
                    s.update_geometry();
                    made += 1;
                }
                None => skipped += 1,
            }
        }
        self.status = if skipped > 0 { format!("{made} Displacement(s) erstellt, {skipped} übersprungen (nur 4-eckige Flächen)") } else { format!("{made} Displacement(s) erstellt") };
        self.touch();
    }

    /// Displacements der ausgewählten Flächen (oder aller ausgewählten Brushes)
    fn disp_targets(&self) -> Vec<(u32, usize)> {
        let mut out: Vec<(u32, usize)> = self.faces.iter().copied().filter(|(s, i)| self.map.solid(*s).is_some_and(|x| x.sides.get(*i).is_some_and(|y| y.disp.is_some()))).collect();
        if out.is_empty() {
            for id in self.selected_solid_ids() {
                if let Some(s) = self.map.solid(id) {
                    out.extend(s.sides.iter().enumerate().filter(|(_, x)| x.disp.is_some()).map(|(i, _)| (id, i)));
                }
            }
        }
        out
    }

    /// Zustand der ausgewählten Displacements: (Anzahl, gemeinsame Power, Flags bei allen, Flags bei irgendeinem)
    pub fn disp_state(&self) -> (usize, Option<u32>, u32, u32) {
        let t = self.disp_targets();
        let ds: Vec<&Disp> = t.iter().filter_map(|(sid, i)| self.map.solid(*sid).and_then(|s| s.sides.get(*i)).and_then(|x| x.disp.as_ref())).collect();
        if ds.is_empty() {
            return (0, None, 0, 0);
        }
        let p = ds[0].power;
        let power = ds.iter().all(|d| d.power == p).then_some(p);
        let all = ds.iter().fold(u32::MAX, |a, d| a & d.flags);
        let any = ds.iter().fold(0, |a, d| a | d.flags);
        (ds.len(), power, all, any)
    }

    fn for_each_disp(&mut self, f: &mut dyn FnMut(&mut Disp, DVec3, &mut Rng)) -> usize {
        let targets = self.disp_targets();
        if targets.is_empty() {
            self.status = "Keine Displacements ausgewählt".into();
            return 0;
        }
        self.checkpoint();
        let mut rng = Rng::new(self.rng.next_f64().to_bits() | 1);
        for (sid, i) in &targets {
            if let Some(s) = self.map.solid_mut(*sid) {
                let n = s.planes.get(*i).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
                if let Some(d) = s.sides[*i].disp.as_mut() {
                    f(d, n, &mut rng);
                }
                s.update_geometry();
            }
        }
        self.touch();
        targets.len()
    }

    pub fn destroy_disp(&mut self) {
        let n = self.for_each_disp(&mut |_, _, _| {});
        if n > 0 {
            for (sid, i) in self.disp_targets() {
                if let Some(s) = self.map.solid_mut(sid) {
                    s.sides[i].disp = None;
                    s.update_geometry();
                }
            }
            self.status = format!("{n} Displacement(s) entfernt");
        }
    }

    pub fn disp_noise(&mut self) {
        let (mn, mx) = (self.dbrush.noise_min, self.dbrush.noise_max);
        let n = self.for_each_disp(&mut |d, nrm, rng| d.add_noise(mn, mx, nrm, rng));
        self.status = format!("Rauschen auf {n} Displacement(s)");
    }

    /// Feinheit (Power) der ausgewählten Displacements ändern, Form bleibt
    pub fn disp_set_power(&mut self, power: u32) {
        let n = self.for_each_disp(&mut |d, nrm, _| d.resample(power, nrm));
        if n > 0 {
            self.status = format!("{n} Displacement(s) auf Power {} umgerechnet", power.clamp(2, 4));
        }
    }

    /// Unterteilen (Catmull-Clark über alle ausgewählten Displacements, wie Hammers „Subdivide“)
    pub fn disp_subdivide(&mut self) {
        let targets = self.disp_targets();
        let mut faces = Vec::new();
        let mut used = Vec::new();
        for (sid, i) in &targets {
            let Some(s) = self.map.solid(*sid) else { continue };
            let (Some(d), Some(poly)) = (s.sides[*i].disp.as_ref(), s.polys.get(*i)) else { continue };
            if let Some(c) = d.corners(poly) {
                faces.push(disp::SubdivFace { corners: c, power: d.power });
                used.push((*sid, *i));
            }
        }
        if faces.is_empty() {
            self.status = "Keine Displacements ausgewählt".into();
            return;
        }
        let targets_pos = disp::subdivide_faces(&faces);
        self.checkpoint();
        for ((sid, i), pts) in used.iter().zip(targets_pos) {
            let Some(s) = self.map.solid_mut(*sid) else { continue };
            let n = s.planes.get(*i).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
            let Some(poly) = s.polys.get(*i).cloned() else { continue };
            if let Some(d) = s.sides[*i].disp.as_mut() {
                let Some(c) = d.corners(&poly) else { continue };
                let res = d.res();
                for a in 0..res {
                    let tr = a as f64 / (res - 1) as f64;
                    let p0 = c[0] + (c[1] - c[0]) * tr;
                    let p1 = c[3] + (c[2] - c[3]) * tr;
                    for b in 0..res {
                        let k = a * res + b;
                        let base = p0 + (p1 - p0) * (b as f64 / (res - 1) as f64);
                        d.offsets[k] = DVec3::ZERO;
                        d.set_vector(k, pts[k] - base - n * d.elevation, n);
                    }
                }
            }
            s.update_geometry();
        }
        self.status = format!("{} Displacement(s) unterteilt (geglättet)", used.len());
        self.touch();
    }

    pub fn disp_invert_alpha(&mut self) {
        self.for_each_disp(&mut |d, _, _| d.invert_alpha());
    }

    pub fn disp_set_flag(&mut self, flag: u32, on: bool) {
        self.for_each_disp(&mut |d, _, _| if on { d.flags |= flag } else { d.flags &= !flag });
    }

    /// Nähen: gemeinsame Randpunkte benachbarter Displacements auf gleiche Höhe bringen
    pub fn disp_sew(&mut self) {
        let targets = self.disp_targets();
        let all: Vec<(u32, usize)> = if targets.len() >= 2 {
            targets
        } else {
            self.map.all_solids().flat_map(|(_, s)| s.sides.iter().enumerate().filter(|(_, x)| x.disp.is_some()).map(move |(i, _)| (s.id, i))).collect()
        };
        // Punkte sammeln: (Index in all, Punktindex, Basis, Position)
        let mut pts: Vec<(usize, usize, DVec3, DVec3)> = Vec::new();
        for (n, (sid, i)) in all.iter().enumerate() {
            if let Some(g) = self.map.solid(*sid).and_then(|s| s.disps.get(*i).cloned().flatten()) {
                let r = g.res;
                for k in 0..g.base.len() {
                    let (a, b) = (k / r, k % r);
                    if a == 0 || b == 0 || a == r - 1 || b == r - 1 {
                        pts.push((n, k, g.base[k], g.pos[k]));
                    }
                }
            }
        }
        let mut moves: Vec<(usize, usize, DVec3)> = Vec::new();
        for p in &pts {
            let same: Vec<&(usize, usize, DVec3, DVec3)> = pts.iter().filter(|q| q.0 != p.0 && q.2.distance(p.2) < 0.5).collect();
            if same.is_empty() {
                continue;
            }
            let avg = (same.iter().fold(p.3, |a, q| a + q.3)) / (same.len() + 1) as f64;
            moves.push((p.0, p.1, avg - p.3));
        }
        if moves.is_empty() {
            self.status = "Keine gemeinsamen Kanten gefunden".into();
            return;
        }
        self.checkpoint();
        for (n, k, delta) in &moves {
            let (sid, i) = all[*n];
            if let Some(d) = self.map.solid_mut(sid).and_then(|s| s.sides[i].disp.as_mut()) {
                d.offsets[*k] += *delta;
            }
        }
        for (sid, _) in &all {
            if let Some(s) = self.map.solid_mut(*sid) {
                s.update_geometry();
            }
        }
        self.status = format!("{} Randpunkte vernäht", moves.len());
        self.touch();
    }

    /// Pinselstrich beginnen (Undo-Punkt, Referenzhöhe merken)
    pub fn begin_paint(&mut self) {
        self.checkpoint();
        self.flatten_ref = None;
        self.dbrush.stroke = self.dbrush.stroke.wrapping_add(1);
    }

    /// Displacement-Pinsel an der Stelle unter dem Strahl anwenden. Liefert den Trefferpunkt.
    pub fn paint_disp(&mut self, origin: DVec3, dir: DVec3, dt: f64) -> Option<DVec3> {
        let mut best: Option<(f64, DVec3, DVec3)> = None;
        for (e, s) in self.map.solids_owned() {
            if !self.is_visible_solid(e, s) {
                continue;
            }
            for (i, g) in s.disps.iter().enumerate() {
                let Some(g) = g else { continue };
                if let Some((t, _)) = g.ray_hit(origin, dir) {
                    if best.map_or(true, |b| t < b.0) {
                        let n = s.planes.get(i).copied().flatten().map(|p| p.normal).unwrap_or(DVec3::Z);
                        best = Some((t, origin + dir * t, n));
                    }
                }
            }
        }
        let (_, hit, hit_n) = best?;
        let b = self.dbrush;
        let axis = if b.along_z { DVec3::Z } else { hit_n };
        let flatten = *self.flatten_ref.get_or_insert(hit.dot(axis));
        let amount = b.strength * dt * 10.0;
        let ids: Vec<u32> = self.map.all_solids().filter(|(_, s)| s.has_disp() && s.bounds.min.distance(hit) < 1e9).map(|(_, s)| s.id).collect();
        for id in ids {
            let Some(s) = self.map.solid_mut(id) else { continue };
            if s.bounds.ray_hit(hit, DVec3::Z).is_none() && !s.bounds.intersects(&Aabb::new(hit - DVec3::splat(b.radius), hit + DVec3::splat(b.radius))) {
                continue;
            }
            let mut changed = false;
            for i in 0..s.sides.len() {
                let (Some(Some(g)), Some(Some(pl))) = (s.disps.get(i).cloned(), s.planes.get(i).copied()) else { continue };
                if let Some(d) = s.sides[i].disp.as_mut() {
                    let ax = if b.along_z { DVec3::Z } else { pl.normal };
                    changed |= disp::paint_ex(d, &g, pl.normal, hit, b.radius, amount, b.mode, ax, flatten, b.falloff, b.step, b.stroke);
                }
            }
            if changed {
                s.update_geometry();
            }
        }
        let _ = axis;
        self.dirty = true;
        self.touch();
        Some(hit)
    }

    // ------------------------------------------------------------ Cordon

    pub fn set_cordon(&mut self, b: Aabb) {
        self.checkpoint();
        self.map.cordon = Some(crate::map::Cordon { bounds: b, active: true });
        self.status = format!("Cordon: {} × {} × {}", b.size().x, b.size().y, b.size().z);
        self.touch();
    }

    /// Kopie der Map, die nur den Cordon-Bereich enthält und mit Skybox-Wänden abgedichtet ist
    pub fn cordoned_map(&self) -> Option<Map> {
        let c = self.map.cordon.filter(|c| c.active)?;
        let mut m = self.map.clone();
        m.world.solids.retain(|s| s.bounds.intersects(&c.bounds));
        m.entities.retain(|e| if e.is_brush_entity() { e.solids.iter().any(|s| s.bounds.intersects(&c.bounds)) } else { e.origin().map_or(true, |o| c.bounds.contains_point(o)) });
        let (mn, mx) = (c.bounds.min - DVec3::splat(16.0), c.bounds.max + DVec3::splat(16.0));
        let (a, b) = (c.bounds.min, c.bounds.max);
        let walls = [
            Aabb::new(mn, DVec3::new(mx.x, mx.y, a.z)),
            Aabb::new(DVec3::new(mn.x, mn.y, b.z), mx),
            Aabb::new(DVec3::new(mn.x, mn.y, a.z), DVec3::new(a.x, mx.y, b.z)),
            Aabb::new(DVec3::new(b.x, mn.y, a.z), DVec3::new(mx.x, mx.y, b.z)),
            Aabb::new(DVec3::new(a.x, mn.y, a.z), DVec3::new(b.x, a.y, b.z)),
            Aabb::new(DVec3::new(a.x, b.y, a.z), DVec3::new(b.x, mx.y, b.z)),
        ];
        for w in walls {
            let s = m.make_box(w, "TOOLS/TOOLSSKYBOX", 0.25);
            m.world.solids.push(s.into());
        }
        m.cordon = None;
        Some(m)
    }

    // ------------------------------------------------------------ Instanzen

    pub fn resolve_instance(&self, file: &str) -> Option<PathBuf> {
        resolve_instance_file(self.path.as_ref().and_then(|p| p.parent()), &self.game_dir, file)
    }

    /// Lädt noch nicht geladene Instanz-Dateien
    pub fn load_instances(&mut self) {
        let files: Vec<String> = self
            .map
            .entities
            .iter()
            .filter(|e| e.classname().eq_ignore_ascii_case("func_instance"))
            .filter_map(|e| e.get("file").map(|s| s.to_string()))
            .filter(|f| !self.instances.contains_key(f))
            .collect();
        if files.is_empty() {
            return;
        }
        let mut notes = Vec::new();
        for f in files {
            let m = self.resolve_instance(&f).and_then(|p| load_instance_expanded(&p, &self.game_dir, &mut Vec::new(), &mut notes)).map(Arc::new);
            self.instances.insert(f, m);
        }
        if let Some(n) = notes.first() {
            self.status = n.clone();
        }
        self.touch();
    }

    /// Ausgewählte func_instance in echte Objekte umwandeln (Namens-Fixup wie VBSP)
    pub fn collapse_instances(&mut self) {
        let ids: Vec<u32> = self.selected_entity_ids().into_iter().filter(|id| self.map.entity(*id).is_some_and(|e| e.classname().eq_ignore_ascii_case("func_instance"))).collect();
        if ids.is_empty() {
            self.status = "Keine func_instance ausgewählt".into();
            return;
        }
        self.load_instances();
        self.checkpoint();
        self.sel.clear();
        for id in ids {
            let Some(e) = self.map.entity(id).cloned() else { continue };
            let Some(Some(inst)) = e.get("file").and_then(|f| self.instances.get(f)).cloned() else {
                self.status = format!("Instanz-Datei {} nicht gefunden", e.get("file").unwrap_or(""));
                continue;
            };
            for o in merge_instance(&mut self.map, &inst, &e, self.tex_lock) {
                self.sel.insert(o);
            }
            self.map.entities.retain(|x| x.id != id);
        }
        self.status = "Instanz(en) eingeklappt".into();
        self.touch();
    }

    /// Pfade aller func_instance-Dateien der Auswahl (bzw. aller, wenn nichts gewählt ist), aufgelöst
    pub fn selected_instance_files(&self) -> Vec<PathBuf> {
        let sel = self.selected_entity_ids();
        self.map
            .entities
            .iter()
            .filter(|e| e.classname().eq_ignore_ascii_case("func_instance") && (sel.is_empty() || sel.contains(&e.id)))
            .filter_map(|e| e.get("file").and_then(|f| self.resolve_instance(f)))
            .collect()
    }

    /// Instanz-Dateien neu laden (z. B. nachdem eine davon gespeichert wurde)
    pub fn reload_instances(&mut self) {
        if !self.instances.is_empty() {
            self.instances.clear();
            self.load_instances();
        }
    }

    // ------------------------------------------------------------ Sonstiges

    /// Gehe zu Brush-/Seiten-/Entity-ID
    pub fn find_by_id(&self, id: u32) -> Option<Obj> {
        for (owner, s) in self.map.all_solids() {
            if s.id == id || s.sides.iter().any(|x| x.id == id) {
                return Some(self.obj_for(owner, s.id));
            }
        }
        self.map.entities.iter().find(|e| e.id == id).map(|e| Obj::Entity(e.id))
    }

    /// Schlüssel auf allen ausgewählten Entities setzen (Mehrfachbearbeitung)
    pub fn set_key_selected(&mut self, key: &str, val: &str) {
        for id in self.selected_entity_ids() {
            if let Some(e) = self.map.entity_mut(id) {
                if val.is_empty() {
                    e.remove(key);
                } else {
                    e.set(key, val);
                }
            }
        }
        self.dirty = true;
        self.touch();
    }

    #[allow(dead_code)]
    pub fn solid_entity_classes(&self) -> Vec<String> {
        self.fgd.names(ClassKind::Solid).into_iter().filter(|n| n != "worldspawn").collect()
    }

    pub fn grid_up(&mut self) {
        self.grid = (self.grid * 2.0).min(1024.0);
        self.status = format!("Raster: {}", self.grid);
    }

    pub fn grid_down(&mut self) {
        self.grid = (self.grid / 2.0).max(1.0);
        self.status = format!("Raster: {}", self.grid);
    }

    /// Neue Kamera in der Map anlegen
    pub fn add_camera(&mut self, pos: DVec3, look: DVec3) {
        self.map.cameras.push(crate::map::CameraDef { pos, look });
        self.map.active_camera = self.map.cameras.len() - 1;
        self.camera_changed = true;
        self.dirty = true;
    }

}

/// Hilfsfunktion: neue Achse übernehmen, Verschiebung/Skalierung behalten
struct TexAxisKeep;

impl TexAxisKeep {
    fn keep(old: &crate::map::TexAxis, new: crate::map::TexAxis) -> crate::map::TexAxis {
        crate::map::TexAxis { axis: new.axis, shift: old.shift, scale: old.scale }
    }
}


/// Instanz-Datei finden: neben der einbindenden VMF, dann unter <spiel>/maps und <spiel> (wie VBSP)
pub fn resolve_instance_file(base_dir: Option<&Path>, game_dir: &str, file: &str) -> Option<PathBuf> {
    let file = file.replace('\\', "/");
    let mut cands = Vec::new();
    if let Some(dir) = base_dir {
        cands.push(dir.join(&file));
    }
    if !game_dir.is_empty() {
        cands.push(Path::new(game_dir).join("maps").join(&file));
        cands.push(Path::new(game_dir).join(&file));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// Höchste Verschachtelungstiefe von Instanzen
pub const MAX_INSTANCE_DEPTH: usize = 8;

/// Instanz-Datei laden und darin enthaltene Instanzen rekursiv einsetzen (wie VBSP).
/// `stack` erkennt Kreise (A bindet B ein, B bindet A ein).
pub fn load_instance_expanded(path: &Path, game_dir: &str, stack: &mut Vec<PathBuf>, notes: &mut Vec<String>) -> Option<Map> {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if stack.contains(&canon) {
        notes.push(format!("Instanz-Kreis: {} bindet sich selbst ein – übersprungen", path.display()));
        return None;
    }
    if stack.len() >= MAX_INSTANCE_DEPTH {
        notes.push(format!("Instanzen tiefer als {MAX_INSTANCE_DEPTH} Ebenen – {} übersprungen", path.display()));
        return None;
    }
    let mut map = Map::from_vmf(&std::fs::read_to_string(path).ok()?).ok()?;
    let inner: Vec<Entity> = map.entities.iter().filter(|e| e.classname().eq_ignore_ascii_case("func_instance")).map(|e| (**e).clone()).collect();
    if inner.is_empty() {
        return Some(map);
    }
    stack.push(canon);
    for e in inner {
        let Some(child_path) = e.get("file").and_then(|f| resolve_instance_file(path.parent(), game_dir, f)) else {
            notes.push(format!("Instanz-Datei {} (in {}) nicht gefunden", e.get("file").unwrap_or(""), path.display()));
            continue;
        };
        if let Some(child) = load_instance_expanded(&child_path, game_dir, stack, notes) {
            merge_instance(&mut map, &child, &e, true);
            map.entities.retain(|x| x.id != e.id);
        }
    }
    stack.pop();
    Some(map)
}

/// Inhalt einer Instanz (`inst`) an der Stelle der func_instance `e` in `target` einsetzen:
/// verschieben/drehen, IDs neu vergeben, Namen nach fixup_style ergänzen, $Parameter ersetzen.
/// Liefert die neuen Objekte.
pub fn merge_instance(target: &mut Map, inst: &Map, e: &Entity, tex_lock: bool) -> Vec<Obj> {
    let mut out = Vec::new();
    let x = Editor::entity_xform(e);
    let name = e.targetname().unwrap_or("").to_string();
    let style = e.get("fixup_style").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let mut replacements: Vec<(String, String)> = Vec::new();
    for (k, v) in &e.props {
        if k.to_ascii_lowercase().starts_with("replace") {
            if let Some((var, val)) = v.split_once(' ') {
                replacements.push((var.to_string(), val.to_string()));
            }
        }
    }
    // längere Variablen zuerst ($farbe2 vor $farbe)
    replacements.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    let fix = |n: &str| -> String {
        if n.is_empty() || n.starts_with('@') || n.starts_with('!') || name.is_empty() {
            return n.to_string();
        }
        match style {
            0 => format!("{name}-{n}"),
            1 => format!("{n}-{name}"),
            _ => n.to_string(),
        }
    };
    for s in &inst.world.solids {
        let mut s = s.clone();
        target.reassign_ids(&mut s);
        s.transform(&x, tex_lock, true);
        out.push(Obj::Solid(s.id));
        target.world.solids.push(s);
    }
    for ie in &inst.entities {
        if ie.classname().eq_ignore_ascii_case("func_instance_parms") {
            continue;
        }
        let mut ne = ie.clone();
        target.reassign_entity_ids(&mut ne);
        ne.transform(&x, tex_lock, true);
        for (k, v) in &mut ne.props {
            for (var, val) in &replacements {
                *v = v.replace(var.as_str(), val);
            }
            let lk = k.to_ascii_lowercase();
            if ["targetname", "parentname", "target", "filtername", "damagefilter"].contains(&lk.as_str()) {
                *v = fix(v);
            }
        }
        for (_, v) in &mut ne.outputs {
            for (var, val) in &replacements {
                *v = v.replace(var.as_str(), val);
            }
            let mut parts: Vec<String> = v.split(['\u{1b}', ',']).map(|s| s.to_string()).collect();
            if let Some(t) = parts.first_mut() {
                *t = fix(t);
            }
            *v = parts.join(",");
        }
        out.push(Obj::Entity(ne.id));
        target.entities.push(ne);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed() -> Editor {
        Editor::new(Fgd::builtin())
    }

    /// Testkarte mit Blend-Displacement: AMBOSS_OUT=datei.vmf cargo test write_blend_map -- --ignored
    /// Beim Kompilieren: ein light ohne „_light“ (wie in einer älteren Map) bekommt Farbe und
    /// Helligkeit, vorhandene Werte bleiben
    #[test]
    fn compile_fills_missing_light_values() {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "light");
        e.set("origin", "-57 2 65");
        e.set("spawnflags", "0");
        ed.map.entities.push(e.into());
        let id2 = ed.map.alloc_id();
        let mut e = Entity::new(id2, "light");
        e.set("_light", "255 0 0 50");
        ed.map.entities.push(e.into());
        let filled = ed.fill_fgd_defaults();
        assert!(filled.iter().any(|f| f == "light: _light"), "{filled:?}");
        assert_eq!(ed.map.entity(id).unwrap().get("_light"), Some("255 255 255 200"));
        assert_eq!(ed.map.entity(id2).unwrap().get("_light"), Some("255 0 0 50"));
        assert_eq!(ed.map.entity(id).unwrap().get("spawnflags"), Some("0"));
        assert!(ed.fill_fgd_defaults().is_empty(), "zweites Mal nichts mehr");
    }

    /// Neue Lichter haben immer Farbe und Helligkeit – auch ohne FGD-Standard
    #[test]
    fn new_light_has_color_and_brightness() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        ed.entity_class = "light".into();
        let id = ed.create_point_entity(DVec3::ZERO);
        assert_eq!(ed.map.entity(id).unwrap().get("_light"), Some("255 255 255 200"));
        ed.entity_class = "light_environment".into();
        let id = ed.create_point_entity(DVec3::ZERO);
        assert!(ed.map.entity(id).unwrap().get("_ambient").is_some());
    }

    /// Umriss eines Würfels aus 12 Dreiecken: genau seine 12 Kanten (Diagonalen der Seiten fallen weg)
    #[test]
    fn model_outline_edges() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let s = ed.map.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(32.0)), "a", 0.25);
        let mut tris = Vec::new();
        for p in &s.polys {
            for k in 1..p.len() - 1 {
                tris.push([p[0], p[k], p[k + 1]]);
            }
        }
        assert_eq!(tris.len(), 12);
        assert_eq!(Editor::feature_edges(&tris).len(), 12);
    }

    /// Modell-Entities werden nur dort getroffen, wo ein Dreieck ist – durch Löcher klickt man hindurch
    #[test]
    fn pick_through_model_holes() {
        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let id = ed.map.alloc_id();
        let mut e = Entity::new(id, "prop_static");
        e.set("origin", "0 0 0");
        e.set("model", "models/test/rahmen.mdl");
        ed.map.entities.push(e.into());
        // zwei senkrechte Platten links und rechts (x = ±40), dazwischen ein Loch
        let q = |x0: f64, x1: f64| [[DVec3::new(x0, 0.0, 0.0), DVec3::new(x1, 0.0, 0.0), DVec3::new(x1, 0.0, 64.0)], [DVec3::new(x0, 0.0, 0.0), DVec3::new(x1, 0.0, 64.0), DVec3::new(x0, 0.0, 64.0)]];
        let mut tris = q(-64.0, -32.0).to_vec();
        tris.extend(q(32.0, 64.0));
        ed.model_bounds.insert("models/test/rahmen.mdl".into(), Aabb::new(DVec3::new(-64.0, -1.0, 0.0), DVec3::new(64.0, 1.0, 64.0)));
        ed.model_tris.insert("models/test/rahmen.mdl".into(), Arc::new(tris));
        let from = DVec3::new(0.0, -200.0, 32.0);
        assert!(ed.pick_3d(from, DVec3::Y).is_none(), "durch das Loch");
        let hit = ed.pick_3d(DVec3::new(-48.0, -200.0, 32.0), DVec3::Y).expect("Platte getroffen");
        assert!((hit.2 - 200.0).abs() < 0.01);
    }

    #[test]
    #[ignore]
    fn write_blend_map() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "NATURE/BLEND_TEST".into();
        ed.create_block(Aabb::new(DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0)));
        let id = ed.map.world.solids[0].id;
        let top = ed.map.solid(id).unwrap().planes.iter().position(|p| p.is_some_and(|p| p.normal.z > 0.9)).unwrap();
        ed.faces = [(id, top)].into_iter().collect();
        ed.create_disp(3);
        let s = ed.map.solid_mut(id).unwrap();
        let d = s.sides[top].disp.as_mut().unwrap();
        let res = d.res();
        for k in 0..d.count() {
            d.alphas[k] = 255.0 * (k / res) as f64 / (res - 1) as f64;
        }
        s.update_geometry();
        ed.save(std::path::PathBuf::from(std::env::var("AMBOSS_OUT").unwrap())).unwrap();
    }

    #[test]
    fn copy_paste_entity_props() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 64.0));
        ed.create_point_entity(DVec3::new(128.0, 0.0, 64.0));
        let (a, b) = (ed.map.entities[0].id, ed.map.entities[1].id);
        ed.map.entity_mut(a).unwrap().set("_light", "255 0 0 400");
        ed.map.entity_mut(a).unwrap().set("targetname", "rot");
        ed.sel = [Obj::Entity(a)].into_iter().collect();
        ed.copy_entity_props();
        ed.sel = [Obj::Entity(b)].into_iter().collect();
        ed.paste_entity_props();
        let e = ed.map.entity(b).unwrap();
        assert_eq!(e.get("_light"), Some("255 0 0 400"));
        assert_eq!(e.get("targetname"), None);
        assert_eq!(e.get("origin"), Some("128 0 64"));
    }

    #[test]
    fn picking_snapping_touching() {
        let mut ed = Editor::new(Fgd::builtin());
        // großer Raum-Boden und kleiner Block darauf
        ed.create_block(Aabb::new(DVec3::new(-512.0, -512.0, -16.0), DVec3::new(512.0, 512.0, 0.0)));
        let floor = ed.map.world.solids[0].id;
        ed.create_block(Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 64.0)));
        let small = ed.map.world.solids[1].id;
        // Klick auf die Kante des Bodens (Top-Ansicht) trifft den Boden, obwohl der Block kleiner ist
        let hits = ed.pick_2d(Axis2D::Top, DVec2::new(-512.0, 100.0), 2.0);
        assert_eq!(hits[0], Obj::Solid(floor));
        let hits = ed.pick_2d(Axis2D::Top, DVec2::new(30.0, 30.0), 2.0);
        assert_eq!(hits[0], Obj::Solid(small));
        // Einrasten an Eckpunkten: Blockecke (0,0) in die Nähe der Bodenecke (-512,-512) ziehen
        ed.sel = [Obj::Solid(small)].into_iter().collect();
        let (pts, ids) = ed.selection_points();
        let d = ed.vertex_snap_delta((0, 1), &pts, DVec2::new(-509.0, -510.0), 6.0, &ids).unwrap();
        assert_eq!(d, DVec2::new(-512.0, -512.0));
        assert!(ed.vertex_snap_delta((0, 1), &pts, DVec2::new(-300.0, 2.0), 6.0, &ids).is_none());
        // Berührende auswählen
        ed.select_touching();
        assert!(ed.sel.contains(&Obj::Solid(floor)));
        // Raster aus
        ed.snap = false;
        assert_eq!(ed.snap_grid(), 1.0);
    }

    #[test]
    fn subdivide_cube_of_displacements() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::splat(-128.0), DVec3::splat(128.0)));
        let id = ed.map.world.solids[0].id;
        ed.faces = (0..6).map(|i| (id, i)).collect();
        ed.create_disp(3);
        ed.disp_subdivide();
        let s = ed.map.solid(id).unwrap();
        let pts: Vec<DVec3> = s.disps.iter().flatten().flat_map(|g| g.pos.clone()).collect();
        assert_eq!(pts.len(), 6 * 81);
        let (mn, mx) = pts.iter().map(|p| p.length()).fold((f64::MAX, 0.0f64), |a, b| (a.0.min(b), a.1.max(b)));
        assert!(mx / mn < 1.25 && mx < 200.0, "{mn} {mx}");
        let text = ed.map.to_vmf();
        Map::from_vmf(&text).unwrap();
        ed.undo();
        assert!(ed.map.solid(id).unwrap().disps.iter().flatten().all(|g| g.pos.iter().all(|p| p.abs().max_element() > 127.9)));
    }

    #[test]
    fn undo_stores_only_changes() {
        let mut ed = Editor::new(Fgd::builtin());
        for i in 0..40 {
            let x = i as f64 * 64.0;
            ed.create_block(Aabb::new(DVec3::new(x, 0.0, 0.0), DVec3::new(x + 32.0, 32.0, 32.0)));
        }
        let states: Vec<String> = {
            let mut v = vec![ed.map.to_vmf()];
            for i in 0..10 {
                let id = ed.map.world.solids[i].id;
                ed.sel = [Obj::Solid(id)].into_iter().collect();
                ed.checkpoint();
                ed.translate_selection(DVec3::new(0.0, 0.0, 16.0));
                v.push(ed.map.to_vmf());
            }
            v
        };
        // 50 Schritte, aber nur wenige Brushes gespeichert (statt 50 × ~40)
        assert!(ed.undo_stored_solids() < 200, "{}", ed.undo_stored_solids());
        for k in (0..10).rev() {
            ed.undo();
            assert_eq!(ed.map.to_vmf(), states[k], "undo {k}");
        }
        for k in 1..=10 {
            ed.redo();
            assert_eq!(ed.map.to_vmf(), states[k], "redo {k}");
        }
        for _ in 0..5 {
            ed.undo();
        }
        assert_eq!(ed.map.to_vmf(), states[5]);
        // neue Änderung mitten im Verlauf
        ed.sel = [Obj::Solid(ed.map.world.solids[39].id)].into_iter().collect();
        ed.delete_selection();
        let after = ed.map.to_vmf();
        ed.undo();
        assert_eq!(ed.map.to_vmf(), states[5]);
        ed.redo();
        assert_eq!(ed.map.to_vmf(), after);
        for _ in 0..5 {
            ed.undo();
        }
        assert_eq!(ed.map.to_vmf(), states[1]);
    }

    #[test]
    fn workflow() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 256.0)));
        assert_eq!(ed.map.world.solids.len(), 1);
        ed.hollow(16.0);
        assert_eq!(ed.map.world.solids.len(), 6);
        assert!(ed.map.world.solids.iter().all(|s| s.is_valid()));
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 16.0));
        assert_eq!(ed.map.entities.len(), 1);
        ed.undo();
        assert_eq!(ed.map.entities.len(), 0);
        ed.redo();
        assert_eq!(ed.map.entities.len(), 1);

        let first = ed.map.world.solids[0].id;
        ed.sel_mode = SelMode::Objects;
        ed.select(Some(Obj::Solid(first)), false);
        ed.tie_to_entity("func_detail");
        assert_eq!(ed.map.world.solids.len(), 5);
        ed.move_to_world();
        assert_eq!(ed.map.world.solids.len(), 6);

        // Skalieren per Transformation
        ed.select(Some(Obj::Solid(first)), false);
        let base = ed.map.clone();
        let from = ed.selection_bounds();
        let mut to = from;
        to.max.z += 64.0;
        ed.transform_selection_from(&base, &Editor::scale_xform(from, to), true);
        assert_eq!(ed.selection_bounds(), to);

        ed.copy();
        ed.paste();
        assert_eq!(ed.map.world.solids.len(), 7);
        ed.paste_special(3, DVec3::new(0.0, 0.0, 64.0), DVec3::new(0.0, 0.0, 15.0), true);
        assert_eq!(ed.map.world.solids.len(), 10);
        assert_eq!(ed.map.groups.len(), 3);

        let back = Map::from_vmf(&ed.map.to_vmf()).unwrap();
        assert_eq!(back.world.solids.len(), 10);
    }

    #[test]
    fn groups_hide_visgroups() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        ed.create_block(Aabb::new(DVec3::new(128.0, 0.0, 0.0), DVec3::new(192.0, 64.0, 64.0)));
        let (a, b) = (ed.map.world.solids[0].id, ed.map.world.solids[1].id);
        ed.sel = [Obj::Solid(a), Obj::Solid(b)].into_iter().collect();
        ed.group_selection();
        ed.clear_selection();
        ed.sel_mode = SelMode::Groups;
        ed.select(Some(Obj::Solid(a)), false);
        assert_eq!(ed.sel.len(), 2);
        ed.ungroup_selection();
        ed.select(Some(Obj::Solid(a)), false);
        assert_eq!(ed.sel.len(), 1);

        ed.hide_selection();
        assert!(!ed.is_visible(Obj::Solid(a)));
        assert_eq!(ed.pick_2d(Axis2D::Top, DVec2::new(32.0, 32.0), 1.0).len(), 0);
        ed.unhide_all();
        assert!(ed.is_visible(Obj::Solid(a)));

        ed.select(Some(Obj::Solid(b)), false);
        let vg = ed.create_visgroup("Test");
        ed.set_visgroup_visible(vg, false);
        assert!(!ed.is_visible(Obj::Solid(b)));
        let back = Map::from_vmf(&ed.map.to_vmf()).unwrap();
        assert!(!back.visgroups[0].visible);
        ed.delete_visgroup(vg);
        assert!(ed.is_visible(Obj::Solid(b)));
    }

    #[test]
    fn clip_three_points() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(128.0)));
        // schräge Ebene durch drei Punkte auf der Oberfläche (leicht neben dem Raster)
        ed.clip3_add(DVec3::new(0.0, 0.0, 129.0));
        ed.clip3_add(DVec3::new(0.0, 127.0, 128.0));
        assert!(ed.clip_plane().is_none(), "erst mit drei Punkten");
        ed.clip3_add(DVec3::new(127.0, 0.0, 0.0));
        let pl = ed.clip_plane().unwrap();
        assert!((pl.normal.x.abs() - pl.normal.z.abs()).abs() < 1e-9 && pl.normal.y.abs() < 1e-9, "auf das Raster gerastet: {pl:?}");
        let (keep, _) = ed.clip_preview();
        assert_eq!(keep.len(), 2, "beide Hälften");
        ed.apply_clip();
        assert_eq!(ed.map.world.solids.len(), 2);
        assert!(ed.clip3.is_empty());
        // vierter Punkt beginnt neu
        for p in [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z] {
            ed.clip3_add(p * 64.0);
        }
        assert_eq!(ed.clip3.len(), 1);
    }

    #[test]
    fn transforms_flip_align_snap() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::new(128.0, 64.0, 64.0)));
        ed.rotate_selection(2, 90.0);
        let b = ed.selection_bounds();
        assert!((b.size().x - 64.0).abs() < 1e-6 && (b.size().y - 128.0).abs() < 1e-6);
        ed.flip_selection(0);
        assert!(ed.map.world.solids[0].is_valid());
        ed.translate_selection(DVec3::new(3.0, 0.0, 0.0));
        ed.snap_selection_to_grid();
        assert_eq!(ed.selection_bounds().min.x % 16.0, 0.0);
        ed.create_block(Aabb::new(DVec3::new(0.0, 0.0, 100.0), DVec3::new(32.0, 32.0, 132.0)));
        ed.select_all();
        ed.align_selection(2, false);
        assert!(ed.map.world.solids.iter().all(|s| s.bounds.min.z == 0.0));
    }

    #[test]
    fn clip_carve_vertex() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(128.0)));
        ed.clip = Some(ClipLine { axis: Axis2D::Top, a: DVec2::new(64.0, -10.0), b: DVec2::new(64.0, 200.0) });
        let (keep, _) = ed.clip_preview();
        assert_eq!(keep.len(), 2);
        ed.apply_clip();
        assert_eq!(ed.map.world.solids.len(), 2);
        ed.clip_mode = ClipMode::Front;
        ed.clip = Some(ClipLine { axis: Axis2D::Front, a: DVec2::new(-10.0, 64.0), b: DVec2::new(200.0, 64.0) });
        ed.select_all();
        ed.apply_clip();
        assert_eq!(ed.map.world.solids.len(), 2);

        // Carve
        let mut ed = self::ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::new(256.0, 16.0, 128.0)));
        ed.create_block(Aabb::new(DVec3::new(96.0, -8.0, 0.0), DVec3::new(160.0, 24.0, 96.0)));
        ed.carve();
        assert_eq!(ed.map.world.solids.len(), 4); // 3 Teile + Cutter

        // Vertex
        let mut ed = self::ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let base = ed.map.clone();
        let sid = ed.map.world.solids[0].id;
        let vs = vec![(sid, DVec3::new(64.0, 64.0, 64.0))];
        assert!(ed.move_vertices_from(&base, &vs, DVec3::new(0.0, 0.0, 32.0)));
        assert_eq!(ed.map.world.solids[0].bounds.max.z, 96.0);
        // ungültig: alle Punkte flach machen
        let all: Vec<(u32, DVec3)> = base.world.solids[0].vertices().into_iter().filter(|v| v.z > 1.0).map(|v| (sid, v)).collect();
        assert!(!ed.move_vertices_from(&base, &all, DVec3::new(0.0, 0.0, -64.0)));
    }

    #[test]
    fn textures_and_disp() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::new(512.0, 512.0, 16.0)));
        let sid = ed.map.world.solids[0].id;
        let top = ed.map.world.solids[0].sides.iter().position(|s| s.plane().unwrap().normal.z > 0.9).unwrap();
        ed.select_face(Some((sid, top)), false);
        ed.justify_faces(Justify::Fit, false, &|_| (512.0, 512.0));
        let side = &ed.map.world.solids[0].sides[top];
        assert!((side.uaxis.scale - 1.0).abs() < 1e-9);
        assert_eq!(ed.replace_materials("dev_measure", "brick/x", false, true), 6);

        ed.create_disp(3);
        assert!(ed.map.world.solids[0].has_disp());
        ed.dbrush.radius = 200.0;
        ed.begin_paint();
        let hit = ed.paint_disp(DVec3::new(256.0, 256.0, 500.0), -DVec3::Z, 0.1).unwrap();
        assert!((hit.z - 16.0).abs() < 1e-6);
        assert!(ed.map.world.solids[0].bounds.max.z > 16.0);
        ed.disp_noise();
        ed.disp_set_flag(disp::FLAG_NO_PHYS, true);
        let back = Map::from_vmf(&ed.map.to_vmf()).unwrap();
        assert_eq!(back.world.solids[0].sides[top].disp.as_ref().unwrap().flags, disp::FLAG_NO_PHYS);
        ed.destroy_disp();
        assert!(!ed.map.world.solids[0].has_disp());
    }

    #[test]
    fn cordon_and_prefab() {
        let mut ed = ed();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        ed.create_block(Aabb::new(DVec3::splat(1000.0), DVec3::splat(1064.0)));
        ed.set_cordon(Aabb::new(DVec3::splat(-128.0), DVec3::splat(128.0)));
        let m = ed.cordoned_map().unwrap();
        assert_eq!(m.world.solids.len(), 1 + 6);

        let dir = std::env::temp_dir().join(format!("amboss_prefab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("kiste.vmf");
        ed.select_all();
        assert_eq!(ed.save_prefab(&p).unwrap(), 2);
        ed.work_point = DVec3::new(512.0, 512.0, 0.0);
        ed.insert_prefab(&p).unwrap();
        assert_eq!(ed.map.world.solids.len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod example {
    use super::*;

    /// Erzeugt examples/beispiel_raum.vmf (cargo test write_example -- --ignored)
    #[test]
    #[ignore]
    fn write_example() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "DEV/DEV_MEASUREGENERIC01B".into();
        ed.create_block(Aabb::new(DVec3::new(-512.0, -384.0, -16.0), DVec3::new(512.0, 384.0, 256.0)));
        ed.hollow(16.0);
        ed.material = "DEV/GRAYGRID".into();
        ed.create_block(Aabb::new(DVec3::new(128.0, 64.0, 0.0), DVec3::new(192.0, 128.0, 240.0)));
        ed.create_block(Aabb::new(DVec3::new(-320.0, 128.0, 0.0), DVec3::new(-192.0, 256.0, 64.0)));
        ed.material = "TOOLS/TOOLSTRIGGER".into();
        ed.create_block(Aabb::new(DVec3::new(256.0, -256.0, 0.0), DVec3::new(384.0, -128.0, 128.0)));
        ed.tie_to_entity("trigger_multiple");
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(-384.0, -256.0, 0.0));
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(0.0, 0.0, 192.0));
        ed.entity_class = "prop_physics".into();
        ed.create_point_entity(DVec3::new(0.0, -200.0, 16.0));
        ed.clear_selection();
        std::fs::create_dir_all("examples").unwrap();
        ed.save("examples/beispiel_raum.vmf".into()).unwrap();
    }
}

#[cfg(test)]
mod showcase {
    use super::*;

    /// Erzeugt examples/funktionen.vmf mit Terrain, Formen, Instanz-Datei, Modell-Prop
    #[test]
    #[ignore]
    fn write_showcase() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "DEV/GRAYGRID".into();
        ed.create_block(Aabb::new(DVec3::new(-1024.0, -1024.0, -16.0), DVec3::new(1024.0, 1024.0, 512.0)));
        ed.hollow(16.0);
        // Terrain auf dem Boden
        ed.material = "CONCRETE/CONCRETEFLOOR001A".into();
        ed.create_block(Aabb::new(DVec3::new(-512.0, 0.0, 0.0), DVec3::new(0.0, 512.0, 16.0)));
        let sid = ed.map.world.solids.last().unwrap().id;
        let top = ed.map.world.solids.last().unwrap().sides.iter().position(|s| s.plane().unwrap().normal.z > 0.9).unwrap();
        ed.faces.insert((sid, top));
        ed.create_disp(3);
        ed.dbrush.radius = 220.0;
        ed.dbrush.strength = 40.0;
        ed.begin_paint();
        ed.paint_disp(DVec3::new(-256.0, 256.0, 400.0), -DVec3::Z, 0.1);
        ed.faces.clear();
        // Formen
        ed.material = "BRICK/BRICKWALL001A".into();
        ed.shape.shape = crate::primitives::Shape::Arch;
        ed.shape.sides = 8;
        ed.shape.arc = 180.0;
        ed.shape.wall = 32.0;
        ed.create_block(Aabb::new(DVec3::new(128.0, 128.0, 0.0), DVec3::new(512.0, 512.0, 64.0)));
        ed.shape.shape = crate::primitives::Shape::Cylinder;
        ed.shape.sides = 12;
        ed.create_block(Aabb::new(DVec3::new(-400.0, -400.0, 0.0), DVec3::new(-272.0, -272.0, 256.0)));
        ed.shape.shape = crate::primitives::Shape::Block;
        ed.entity_class = "info_player_start".into();
        ed.create_point_entity(DVec3::new(0.0, -600.0, 0.0));
        ed.entity_class = "light_spot".into();
        let id = ed.create_point_entity(DVec3::new(256.0, -256.0, 400.0));
        ed.map.entity_mut(id).unwrap().set("pitch", "-60");
        ed.entity_class = "light".into();
        ed.create_point_entity(DVec3::new(-256.0, 256.0, 300.0));
        ed.entity_class = "prop_physics".into();
        ed.next_model = "models/test/kiste.mdl".into();
        ed.create_point_entity(DVec3::new(300.0, -500.0, 0.0));
        ed.add_camera(DVec3::new(-900.0, -900.0, 400.0), DVec3::new(0.0, 0.0, 0.0));
        ed.clear_selection();
        std::fs::create_dir_all("examples").unwrap();
        ed.save("examples/funktionen.vmf".into()).unwrap();
    }
}

#[cfg(test)]
mod nested_instance_tests {
    use super::*;

    fn write(dir: &Path, name: &str, f: impl FnOnce(&mut Map)) {
        let mut m = Map::new();
        f(&mut m);
        std::fs::write(dir.join(name), m.to_vmf()).unwrap();
    }

    fn inst(m: &mut Map, file: &str, name: &str, origin: &str, angles: &str, replace: Option<&str>) {
        let mut e = Entity::new(m.alloc_id(), "func_instance");
        e.set("file", file);
        e.set("targetname", name);
        e.set("origin", origin);
        e.set("angles", angles);
        if let Some(r) = replace {
            e.set("replace01", r);
        }
        m.entities.push(e.into());
    }

    #[test]
    fn nested_three_levels() {
        let dir = std::env::temp_dir().join(format!("amboss_nested_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inst")).unwrap();
        // Ebene 3: Tür mit Knopf
        write(&dir.join("inst"), "tuer.vmf", |m| {
            let s = m.make_box(Aabb::new(DVec3::ZERO, DVec3::splat(8.0)), "DEV/DEV_MEASUREGENERIC01", 0.25);
            m.world.solids.push(s.into());
            let mut b = Entity::new(m.alloc_id(), "func_button");
            b.set("targetname", "knopf");
            b.set("rendercolor", "$farbe");
            b.outputs.push(("OnPressed".into(), "tuer\u{1b}Open\u{1b}\u{1b}0\u{1b}-1".into()));
            m.entities.push(b.into());
        });
        // Ebene 2: Wohnung bindet die Tür ein (Pfad relativ zur Wohnung!) und reicht $wfarbe weiter
        write(&dir.join("inst"), "wohnung.vmf", |m| inst(m, "tuer.vmf", "t1", "100 0 0", "0 0 0", Some("$farbe $wfarbe")));
        // Ebene 1: Block bindet die Wohnung gedreht ein
        write(&dir.join("inst"), "block.vmf", |m| inst(m, "wohnung.vmf", "w1", "0 1000 0", "0 90 0", Some("$wfarbe 255 0 0")));
        // Kreis
        write(&dir.join("inst"), "kreis.vmf", |m| inst(m, "kreis.vmf", "k", "0 0 0", "0 0 0", None));

        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        ed.path = Some(dir.join("haupt.vmf"));
        inst(&mut ed.map, "inst/block.vmf", "b", "0 0 500", "0 0 0", None);
        inst(&mut ed.map, "inst/kreis.vmf", "k0", "0 0 0", "0 0 0", None);
        ed.load_instances();
        let block = ed.instances.get("inst/block.vmf").cloned().flatten().expect("Block geladen");
        assert_eq!(block.world.solids.len(), 1, "Brush der Tür steckt im Block");
        assert!(!block.entities.iter().any(|e| e.classname() == "func_instance"), "innere Instanzen sind aufgelöst");
        // Kreis wird erkannt und übersprungen (kein Absturz, keine Endlosschleife)
        assert!(ed.status.contains("Kreis") || ed.instances.contains_key("inst/kreis.vmf"));
        // Bounds der Instanz in der Haupt-Map: gedreht + verschoben
        let be = ed.map.entities.iter().find(|e| e.targetname() == Some("b")).unwrap().clone();
        let bb = ed.entity_bounds(&be);
        assert!((bb.min - DVec3::new(-8.0, 1100.0, 500.0)).length() < 1e-6, "{bb:?}");
        // Einklappen: Namen über alle Ebenen, Parameter durchgereicht, Outputs angepasst
        ed.sel = [Obj::Entity(be.id)].into_iter().collect();
        ed.collapse_instances();
        let btn = ed.map.entities.iter().find(|e| e.classname() == "func_button").expect("Knopf eingeklappt");
        assert_eq!(btn.targetname(), Some("b-w1-t1-knopf"));
        assert_eq!(btn.get("rendercolor"), Some("255 0 0"));
        assert!(btn.outputs[0].1.starts_with("b-w1-t1-tuer"), "{:?}", btn.outputs);
        let s = ed.map.world.solids.last().unwrap();
        assert!((s.bounds.min - DVec3::new(-8.0, 1100.0, 500.0)).length() < 1e-6, "{:?}", s.bounds);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
