//! Befehls-Registry: Menübefehle, frei belegbare Tastenkürzel, Befehlspalette (Strg+P) und MCP.

use crate::app::{App, Pending};
use crate::editor::ClipMode;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cmd {
    New,
    Open,
    Save,
    SaveAs,
    NewTab,
    CloseTab,
    NextTab,
    ImportMap,
    ImportModel,
    Workshop,
    Spline,
    UnifyTextures,
    MergeMap,
    CompareMap,
    Merge3,
    ExportObj,
    Undo,
    Redo,
    History,
    PasteSpecial,
    Delete,
    SelectAll,
    Duplicate,
    Carve,
    Hollow,
    TieToEntity,
    MoveToWorld,
    Group,
    Ungroup,
    Hide,
    HideUnselected,
    UnhideAll,
    Lock,
    UnlockAll,
    Transform,
    SnapToGrid,
    FlipH,
    FlipV,
    RotateCw,
    RotateMouse,
    RotateCcw,
    ApplyTexture,
    ReplaceTextures,
    NodrawHidden,
    TextureBrowser,
    ModelBrowser,
    SoundBrowser,
    ParticleBrowser,
    CheckProblems,
    ToggleSnap,
    Collab,
    CopyProps,
    PasteProps,
    SelectTouching,
    ToggleVertexSnap,
    Optimize,
    OptimizeCheck,
    EntityReport,
    Outliner,
    GotoNumber,
    MapInfo,
    LogicalView,
    Prefabs,
    Compile,
    CompileWindow,
    TestFromCamera,
    Walk,
    Settings,
    Help,
    Palette,
    GridDown,
    GridUp,
    CenterSel2D,
    CenterSel3D,
    NextCamera,
    Maximize,
    ToggleGizmo,
    LoadPointfile,
    ClearMeasure,
    ToggleNodraw,
    ToggleTriggers,
    ToggleToolBrushes,
    SkyboxMode,
    SkyboxCopy,
    SkyboxCheck,
    ToggleGameplay,
    AddNote,
    Notes,
    DecalFromImage,
    Screenshot,
    GamemodeChecklist,
    ExportGltf,
    Timeline,
    Tool(crate::editor::Tool),
}

impl Cmd {
    pub fn all() -> Vec<Cmd> {
        use Cmd::*;
        let mut v = vec![
            New, Open, Save, SaveAs, NewTab, CloseTab, NextTab, MergeMap, CompareMap, Merge3, ImportMap, ImportModel, Workshop, Spline, UnifyTextures, ExportObj, Undo, Redo, History, PasteSpecial, Delete, SelectAll, Duplicate, Carve, Hollow, TieToEntity, MoveToWorld, Group, Ungroup, Hide,
            HideUnselected, UnhideAll, Lock, UnlockAll, Transform, SnapToGrid, FlipH, FlipV, RotateMouse, RotateCw, RotateCcw, ApplyTexture, ReplaceTextures, NodrawHidden, TextureBrowser, ModelBrowser, SoundBrowser, ParticleBrowser, Collab, ToggleSnap, ToggleVertexSnap, SelectTouching, CopyProps, PasteProps, CheckProblems, Optimize, OptimizeCheck, EntityReport,
            Outliner, GotoNumber, MapInfo, LogicalView, Prefabs, Compile, CompileWindow, TestFromCamera, Walk, Settings, Help, Palette, GridDown, GridUp, CenterSel2D, CenterSel3D, NextCamera, Maximize, ToggleGizmo, LoadPointfile, ClearMeasure,
            ToggleNodraw, ToggleTriggers, ToggleToolBrushes, SkyboxMode, SkyboxCopy, SkyboxCheck, ToggleGameplay, AddNote, Notes, DecalFromImage, Screenshot, GamemodeChecklist, ExportGltf, Timeline,
        ];
        v.extend(crate::editor::Tool::ALL.iter().map(|t| Tool(*t)));
        v
    }

    pub fn id(self) -> String {
        match self {
            Cmd::Tool(t) => format!("tool_{}", format!("{t:?}").to_ascii_lowercase()),
            c => {
                let s = format!("{c:?}");
                // CamelCase → snake_case
                let mut out = String::new();
                for (i, ch) in s.chars().enumerate() {
                    if ch.is_uppercase() && i > 0 {
                        out.push('_');
                    }
                    out.push(ch.to_ascii_lowercase());
                }
                out
            }
        }
    }

    pub fn from_id(id: &str) -> Option<Cmd> {
        Cmd::all().into_iter().find(|c| c.id() == id)
    }

    pub fn label(self) -> String {
        use Cmd::*;
        match self {
            New => crate::i18n::tr("Neue Map (neuer Tab)"),
            Open => crate::i18n::tr("Map öffnen"),
            Save => crate::i18n::tr("Speichern"),
            SaveAs => crate::i18n::tr("Speichern unter"),
            NewTab => crate::i18n::tr("Neuer Tab"),
            CloseTab => crate::i18n::tr("Tab schließen"),
            NextTab => crate::i18n::tr("Nächster Tab"),
            ImportMap => crate::i18n::tr("Map anderer Editoren (.map, .rmf)…"),
            ImportModel => crate::i18n::tr("Modell importieren (glTF, OBJ)…"),
            Workshop => crate::i18n::tr("Im Workshop veröffentlichen…"),
            Spline => crate::i18n::tr("Spline: Straße, Rohr, Zaun, Kabel…"),
            UnifyTextures => crate::i18n::tr("Texturen nahtlos angleichen"),
            MergeMap => crate::i18n::tr("Map zusammenführen…"),
            CompareMap => crate::i18n::tr("Mit anderem Map-Stand vergleichen…"),
            Merge3 => crate::i18n::tr("3-Wege-Zusammenführen (Basis / andere Version)…"),
            ExportObj => crate::i18n::tr("Exportieren (.obj)"),
            ExportGltf => crate::i18n::tr("Exportieren (.glb, glTF mit Texturen)"),
            Undo => crate::i18n::tr("Rückgängig"),
            Redo => crate::i18n::tr("Wiederholen"),
            History => crate::i18n::tr("Rückgängig-Verlauf"),
            PasteSpecial => crate::i18n::tr("Inhalte einfügen"),
            Delete => crate::i18n::tr("Löschen"),
            SelectAll => crate::i18n::tr("Alles auswählen"),
            Duplicate => crate::i18n::tr("Duplizieren"),
            Carve => crate::i18n::tr("Carve"),
            Hollow => crate::i18n::tr("Aushöhlen"),
            TieToEntity => crate::i18n::tr("In Entity umwandeln"),
            MoveToWorld => crate::i18n::tr("In Welt verschieben"),
            Group => crate::i18n::tr("Gruppieren"),
            Ungroup => crate::i18n::tr("Gruppe auflösen"),
            Hide => crate::i18n::tr("Auswahl ausblenden"),
            HideUnselected => crate::i18n::tr("Unausgewähltes ausblenden"),
            UnhideAll => crate::i18n::tr("Alles einblenden"),
            Lock => crate::i18n::tr("Auswahl sperren"),
            UnlockAll => crate::i18n::tr("Alles entsperren"),
            Transform => crate::i18n::tr("Transformieren"),
            SnapToGrid => crate::i18n::tr("Am Raster ausrichten"),
            FlipH => crate::i18n::tr("Horizontal spiegeln"),
            FlipV => crate::i18n::tr("Vertikal spiegeln"),
            RotateCw => crate::i18n::tr("90° im Uhrzeigersinn drehen"),
            RotateMouse => crate::i18n::tr("Zur Maus drehen (mit Vorschau)"),
            RotateCcw => crate::i18n::tr("90° gegen den Uhrzeigersinn drehen"),
            ApplyTexture => crate::i18n::tr("Aktuelle Textur anwenden"),
            ReplaceTextures => crate::i18n::tr("Texturen ersetzen"),
            NodrawHidden => crate::i18n::tr("Verdeckte Flächen auf nodraw setzen"),
            TextureBrowser => crate::i18n::tr("Textur-Browser"),
            ModelBrowser => crate::i18n::tr("Modell-Browser"),
            SoundBrowser => crate::i18n::tr("Sound-Browser"),
            ParticleBrowser => crate::i18n::tr("Partikel-Browser"),
            CheckProblems => crate::i18n::tr("Auf Probleme prüfen"),
            ToggleSnap => crate::i18n::tr("Am Raster einrasten (an/aus)"),
            Collab => crate::i18n::tr("Live-Sitzung…"),
            CopyProps => crate::i18n::tr("Entity-Eigenschaften kopieren"),
            PasteProps => crate::i18n::tr("Entity-Eigenschaften auf Auswahl übertragen"),
            SelectTouching => crate::i18n::tr("Berührende Objekte zur Auswahl hinzufügen"),
            ToggleVertexSnap => crate::i18n::tr("An Eckpunkten einrasten (an/aus)"),
            Optimize => crate::i18n::tr("Automatische Optimierungen…"),
            OptimizeCheck => crate::i18n::tr("Optimierungen prüfen (KI-Netz, Leck, Vorschläge)"),
            EntityReport => crate::i18n::tr("Entity-Report"),
            Outliner => crate::i18n::tr("Objektliste (Outliner)"),
            GotoNumber => crate::i18n::tr("Gehe zu Nummer/Koordinate"),
            MapInfo => crate::i18n::tr("Map-Info"),
            LogicalView => crate::i18n::tr("Logische Ansicht"),
            Prefabs => crate::i18n::tr("Prefabs"),
            Compile => crate::i18n::tr("Sofort kompilieren & starten"),
            CompileWindow => crate::i18n::tr("Kompilieren & Exportieren…"),
            TestFromCamera => crate::i18n::tr("Im Spiel testen ab 3D-Kamera"),
            Walk => crate::i18n::tr("Begehen (im Editor spielen)"),
            Settings => crate::i18n::tr("Einstellungen"),
            Help => crate::i18n::tr("Hilfe"),
            Palette => crate::i18n::tr("Befehlspalette"),
            GridDown => crate::i18n::tr("Raster verkleinern"),
            GridUp => crate::i18n::tr("Raster vergrößern"),
            CenterSel2D => crate::i18n::tr("2D auf Auswahl zentrieren"),
            CenterSel3D => crate::i18n::tr("3D auf Auswahl zentrieren"),
            NextCamera => crate::i18n::tr("Nächste Kamera"),
            Maximize => crate::i18n::tr("Ansicht maximieren"),
            ToggleGizmo => crate::i18n::tr("3D-Gizmo ein/aus"),
            LoadPointfile => crate::i18n::tr("Pointfile laden"),
            ClearMeasure => crate::i18n::tr("Messung löschen"),
            ToggleNodraw => crate::i18n::tr("nodraw-Flächen ein-/ausblenden"),
            ToggleGameplay => crate::i18n::tr("Gameplay-Maße ein-/ausblenden"),
            AddNote => crate::i18n::tr("Notiz anheften"),
            Notes => crate::i18n::tr("Notizen…"),
            DecalFromImage => crate::i18n::tr("Decal/Overlay aus Bild…"),
            Screenshot => crate::i18n::tr("Screenshot in hoher Auflösung…"),
            GamemodeChecklist => crate::i18n::tr("Gamemode-Checkliste…"),
            Timeline => crate::i18n::tr("Zeitleiste der Speicherstände…"),
            ToggleTriggers => crate::i18n::tr("Trigger ein-/ausblenden"),
            ToggleToolBrushes => crate::i18n::tr("Alle Werkzeug-Brushes ein-/ausblenden"),
            SkyboxMode => crate::i18n::tr("3D-Skybox bearbeiten"),
            SkyboxCopy => crate::i18n::tr("Auswahl verkleinert in die 3D-Skybox kopieren"),
            SkyboxCheck => crate::i18n::tr("3D-Skybox prüfen"),
            Tool(t) => return format!("Werkzeug: {}", t.label()),
        }
        .to_string()
    }

    /// Standard-Tastenkürzel
    pub fn default_key(self) -> &'static str {
        use Cmd::*;
        match self {
            New => "Ctrl+N",
            Open => "Ctrl+O",
            Save => "Ctrl+S",
            SaveAs => "Ctrl+Shift+S",
            CloseTab => "Ctrl+F4",
            NextTab => "Ctrl+Tab",
            Undo => "Ctrl+Z",
            Redo => "Ctrl+Y",
            PasteSpecial => "Ctrl+Shift+V",
            Delete => "Delete",
            SelectAll => "Ctrl+A",
            Duplicate => "Ctrl+D",
            Carve => "Ctrl+Shift+C",
            Hollow => "Ctrl+H",
            TieToEntity => "Ctrl+T",
            MoveToWorld => "Ctrl+Shift+W",
            Group => "Ctrl+G",
            Ungroup => "Ctrl+U",
            Hide => "H",
            HideUnselected => "Shift+H",
            UnhideAll => "U",
            Lock => "L",
            UnlockAll => "Shift+L",
            Transform => "Ctrl+M",
            SnapToGrid => "Ctrl+B",
            FlipH => "Ctrl+L",
            FlipV => "Ctrl+I",
            RotateMouse => "Ctrl+R",
            RotateCcw => "Ctrl+Shift+R",
            ApplyTexture => "Shift+T",
            TextureBrowser => "Ctrl+Shift+T",
            CheckProblems => "Alt+P",
            ToggleSnap => "Alt+G",
            CopyProps => "Ctrl+Alt+C",
            PasteProps => "Ctrl+Alt+V",
            SelectTouching => "Ctrl+Alt+A",
            ToggleVertexSnap => "Alt+V",
            EntityReport => "Ctrl+Shift+F",
            Outliner => "Ctrl+Shift+O",
            GotoNumber => "Ctrl+Shift+G",
            Compile => "F9",
            Help => "F1",
            Palette => "Ctrl+K",
            GridDown => "[",
            GridUp => "]",
            CenterSel2D => "Ctrl+E",
            CenterSel3D => "Ctrl+Shift+E",
            NextCamera => "PageDown",
            Maximize => "Shift+Z",
            ToggleGizmo => "Shift+W",
            History => "Ctrl+Shift+H",
            Tool(t) => t.shortcut(),
            CompileWindow => "Ctrl+F9",
            TestFromCamera => "Shift+F9",
            Walk => "F6",
            UnifyTextures => "Ctrl+Shift+A",
            ToggleNodraw => "Alt+N",
            ToggleGameplay => "Alt+M",
            Screenshot => "Ctrl+F12",
            AddNote => "Ctrl+Shift+N",
            ToggleTriggers => "Alt+T",
            _ => "",
        }
    }

    /// Darf der Befehl auslösen, während die Maus über der 3D-Ansicht ist?
    /// (Shift+Buchstabe und einzelne Buchstaben kollidieren mit der Kamerasteuerung)
    pub fn allowed_in_3d(self, key: &str) -> bool {
        let k = key.to_ascii_lowercase();
        !(k.starts_with("shift+") && k.len() == 7 || k.len() == 1 && k.chars().all(|c| c.is_ascii_alphabetic()))
    }
}

/// "Ctrl+Shift+T" → KeyboardShortcut
pub fn parse_shortcut(s: &str) -> Option<KeyboardShortcut> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut mods = Modifiers::NONE;
    let parts: Vec<&str> = s.split('+').collect();
    // "Ctrl++" wäre die Plus-Taste
    let (mod_parts, key_part) = if s.ends_with("++") { (&parts[..parts.len() - 2], "Plus") } else { (&parts[..parts.len() - 1], *parts.last()?) };
    for m in mod_parts {
        match m.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "strg" | "cmd" => mods = mods | Modifiers::COMMAND,
            "shift" | "umschalt" => mods = mods | Modifiers::SHIFT,
            "alt" => mods = mods | Modifiers::ALT,
            _ => return None,
        }
    }
    let key = Key::from_name(key_part.trim()).or_else(|| Key::from_name(&key_part.trim().to_ascii_uppercase()))?;
    Some(KeyboardShortcut::new(mods, key))
}

pub fn format_shortcut(sc: &KeyboardShortcut) -> String {
    let mut s = String::new();
    if sc.modifiers.command || sc.modifiers.ctrl {
        s.push_str("Ctrl+");
    }
    if sc.modifiers.shift {
        s.push_str("Shift+");
    }
    if sc.modifiers.alt {
        s.push_str("Alt+");
    }
    s.push_str(sc.logical_key.name());
    s
}

/// Anzeige auf Deutsch (Strg statt Ctrl)
pub fn display_shortcut(s: &str) -> String {
    if crate::i18n::lang() != crate::i18n::Lang::De {
        return s.replace("PageDown", "PgDn");
    }
    s.replace("Ctrl", "Strg").replace("Delete", "Entf").replace("PageDown", "Bild↓")
}

impl App {
    /// Tatsächlich belegtes Kürzel (Benutzer-Belegung oder Standard)
    pub(crate) fn key_for(&self, cmd: Cmd) -> String {
        self.settings.keymap.get(&cmd.id()).cloned().unwrap_or_else(|| cmd.default_key().to_string())
    }

    pub(crate) fn run_cmd(&mut self, ctx: &egui::Context, cmd: Cmd) {
        use Cmd::*;
        let (h, v, depth) = self.view_axes();
        match cmd {
            New => self.new_tab(),
            Open => self.request(ctx, Pending::Open(None)),
            Save => {
                self.save();
            }
            SaveAs => {
                self.save_as();
            }
            NewTab => self.new_tab(),
            CloseTab => self.close_tab(ctx, self.active_tab),
            NextTab => {
                let n = self.tabs.len();
                if n > 1 {
                    self.switch_tab((self.active_tab + 1) % n);
                }
            }
            ImportMap => self.import_map(),
            MergeMap => self.start_merge(),
            CompareMap => self.start_compare(),
            Merge3 => self.start_merge3(),
            ExportObj => self.export_obj(),
            ExportGltf => self.export_gltf(),
            Undo => self.ed.undo(),
            Redo => self.ed.redo(),
            History => self.win.history = !self.win.history,
            PasteSpecial => self.win.paste_special = true,
            Delete => self.ed.delete_selection(),
            SelectAll => self.ed.select_all(),
            Duplicate => {
                self.ed.duplicate_in_place();
                let g = self.ed.grid;
                self.ed.translate_selection(glam::DVec3::new(g, g, 0.0));
            }
            Carve => self.ed.carve(),
            Hollow => self.ed.hollow(self.hollow_thickness),
            TieToEntity => self.open_convert_dialog(),
            MoveToWorld => self.ed.move_to_world(),
            Group => self.ed.group_selection(),
            Ungroup => self.ed.ungroup_selection(),
            Hide => self.ed.hide_selection(),
            HideUnselected => self.ed.hide_unselected(),
            UnhideAll => self.ed.unhide_all(),
            Lock => self.ed.lock_selection(true),
            UnlockAll => self.ed.unlock_all(),
            Transform => self.win.transform = true,
            SnapToGrid => self.ed.snap_selection_to_grid(),
            FlipH => self.ed.flip_selection(h),
            FlipV => self.ed.flip_selection(v),
            RotateCw => self.ed.rotate_selection(depth, -90.0),
            RotateMouse => self.ed.rot_live_begin(),
            RotateCcw => self.ed.rotate_selection(depth, 90.0),
            ApplyTexture => self.ed.apply_material(),
            ReplaceTextures => self.win.replace = true,
            NodrawHidden => {
                let only = !self.ed.sel.is_empty();
                let fs = self.mats.fs();
                self.ed.nodraw_covered_faces(only, fs.as_deref());
            }
            TextureBrowser => self.win.browser = !self.win.browser,
            ModelBrowser => {
                self.dlg.browse_target = None;
                self.win.models = true;
            }
            SoundBrowser => {
                self.dlg.browse_target = None;
                self.win.sounds = true;
            }
            ParticleBrowser => {
                self.pb.target = None;
                self.pb.open = true;
            }
            Optimize => self.win.optimize = !self.win.optimize,
            OptimizeCheck => {
                self.run_optimizer(false);
                self.win.optimize = true;
            }
            SelectTouching => self.ed.select_touching(),
            Collab => self.win.collab = !self.win.collab,
            CopyProps => self.ed.copy_entity_props(),
            PasteProps => self.ed.paste_entity_props(),
            ToggleSnap => {
                self.ed.snap = !self.ed.snap;
                self.ed.status = if self.ed.snap { format!("Am Raster einrasten: an ({})", self.ed.grid) } else { crate::i18n::tr("Am Raster einrasten: aus (1 Einheit)").into() };
            }
            ToggleVertexSnap => {
                self.ed.vertex_snap = !self.ed.vertex_snap;
                self.ed.status = format!("An Eckpunkten einrasten: {}", if self.ed.vertex_snap { "an" } else { crate::i18n::tr("aus") });
            }
            CheckProblems => {
                self.problems = crate::problems::check(&self.ed, self.known_materials().as_ref());
                self.win.problems = true;
            }
            EntityReport => self.win.entity_report = true,
            Outliner => self.win.outliner = !self.win.outliner,
            ImportModel => self.pick_model_file(),
            Workshop => self.open_workshop(),
            UnifyTextures => {
                self.ed.unify_textures();
            }
            Spline => {
                self.ed.spline.open = true;
                self.ed.spline.placing = true;
            }
            ToggleNodraw | ToggleTriggers | ToggleToolBrushes => {
                let av = &mut self.ed.auto_vis;
                let (on, what) = match cmd {
                    ToggleNodraw => {
                        av.nodraw = !av.nodraw;
                        (av.nodraw, crate::i18n::tr("nodraw-Flächen"))
                    }
                    ToggleTriggers => {
                        av.triggers = !av.triggers;
                        (av.triggers, crate::i18n::tr("Trigger"))
                    }
                    _ => {
                        let on = !(av.nodraw && av.triggers && av.clips && av.hints && av.portals && av.skybox && av.tools);
                        av.nodraw = on;
                        av.triggers = on;
                        av.clips = on;
                        av.hints = on;
                        av.portals = on;
                        av.skybox = on;
                        av.tools = on;
                        (on, crate::i18n::tr("Werkzeug-Brushes"))
                    }
                };
                let sel: Vec<_> = self.ed.sel.iter().copied().filter(|o| self.ed.is_visible(*o)).collect();
                self.ed.sel = sel.into_iter().collect();
                self.ed.touch();
                self.ed.status = crate::i18n::tr_dyn(&format!("{what}: {}", if on { crate::i18n::tr("eingeblendet") } else { crate::i18n::tr("ausgeblendet") }));
            }
            SkyboxMode => self.toggle_sky_mode(),
            SkyboxCopy => self.sky_copy_selection(),
            SkyboxCheck => {
                self.sky.issues = crate::skybox::check(&self.ed.map, self.ed.map_limit);
                self.sky.check_open = true;
            }
            GotoNumber => self.win.goto = true,
            MapInfo => self.win.map_info = true,
            LogicalView => self.win.logical = true,
            Prefabs => self.win.prefabs = true,
            Compile => self.start_compile(),
            CompileWindow => self.win.compile = true,
            TestFromCamera => self.test_from_camera(),
            Walk => self.toggle_walk(),
            Settings => self.win.settings = true,
            Help => self.win.help = !self.win.help,
            Palette => {
                self.win.palette = !self.win.palette;
                self.dlg.palette_filter.clear();
                self.dlg.palette_index = 0;
            }
            GridDown => self.ed.grid_down(),
            GridUp => self.ed.grid_up(),
            CenterSel2D => self.center_on_selection(false),
            CenterSel3D => self.center_on_selection(true),
            NextCamera => {
                let n = self.ed.map.cameras.len();
                if n > 1 {
                    self.ed.map.active_camera = (self.ed.map.active_camera + 1) % n;
                    self.ed.camera_changed = true;
                }
            }
            Maximize => {
                if let Some(v) = self.hovered_view {
                    self.maximized = if self.maximized.is_some() { None } else { Some(v) };
                }
            }
            AddNote => self.add_note(),
            Notes => self.win.notes = !self.win.notes,
            DecalFromImage => self.open_decal_gen(),
            Screenshot => self.shot.open = true,
            GamemodeChecklist => self.win.gamemode = !self.win.gamemode,
            Timeline => self.open_timeline(),
            ToggleGameplay => {
                self.settings.gameplay = !self.settings.gameplay;
                self.ed.gameplay = self.settings.gameplay;
                self.ed.status = if self.settings.gameplay { crate::i18n::tr("Gameplay-Maße an: Spielerfigur folgt der Maus (grün = passt, gelb = nur geduckt, rot = zu eng)") } else { crate::i18n::tr("Gameplay-Maße aus") }.into();
            }
            ToggleGizmo => {
                self.view3d.gizmo = !self.view3d.gizmo;
                self.ed.status = format!("3D-Gizmo {}", if self.view3d.gizmo { "an" } else { crate::i18n::tr("aus") });
            }
            LoadPointfile => self.load_pointfile(),
            ClearMeasure => {
                self.ed.measure = None;
                self.ed.touch();
            }
            Tool(t) => {
                if t == crate::editor::Tool::Clip && self.ed.tool == crate::editor::Tool::Clip {
                    self.ed.clip_mode = match self.ed.clip_mode {
                        ClipMode::Both => ClipMode::Front,
                        ClipMode::Front => ClipMode::Back,
                        ClipMode::Back => ClipMode::Both,
                    };
                    self.ed.touch();
                } else {
                    self.set_tool(t);
                }
            }
        }
    }

    /// Tastenkürzel aller Befehle prüfen (spezifischere Kombinationen zuerst)
    pub(crate) fn dispatch_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let mut list: Vec<(Cmd, String, KeyboardShortcut)> = Cmd::all()
            .into_iter()
            .filter_map(|c| {
                let k = self.key_for(c);
                parse_shortcut(&k).map(|sc| (c, k, sc))
            })
            .collect();
        let mods = |sc: &KeyboardShortcut| sc.modifiers.command as u8 + sc.modifiers.shift as u8 + sc.modifiers.alt as u8;
        list.sort_by_key(|(_, _, sc)| std::cmp::Reverse(mods(sc)));
        for (cmd, key, sc) in list {
            let file_cmd = matches!(cmd, Cmd::New | Cmd::Open | Cmd::Save | Cmd::SaveAs);
            if typing && !file_cmd {
                continue;
            }
            // über der 3D-Ansicht: beim Umsehen gehören Buchstaben der Kamera; sonst nur die
            // Bewegungstasten ohne Umschalt-/Alt-Taste (Shift+A usw. lösen ihren Befehl aus)
            if self.hover_3d && (self.view3d.flying || self.view3d.walker.on) && !cmd.allowed_in_3d(&key) {
                continue;
            }
            if self.hover_3d && !sc.modifiers.shift && !sc.modifiers.alt && !sc.modifiers.command && self.settings.cam.move_keys().iter().any(|(k, _)| *k == sc.logical_key) {
                continue;
            }
            // Buchstaben ohne Strg nicht auslösen, wenn Strg gedrückt ist (z. B. Strg+H ≠ H)
            if !sc.modifiers.command && ctx.input(|i| i.modifiers.command) {
                continue;
            }
            if !sc.modifiers.shift && sc.modifiers.command && ctx.input(|i| i.modifiers.shift) {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                self.run_cmd(ctx, cmd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_roundtrip() {
        for c in Cmd::all() {
            let k = c.default_key();
            if k.is_empty() {
                continue;
            }
            let sc = parse_shortcut(k).unwrap_or_else(|| panic!("{k}"));
            assert_eq!(parse_shortcut(&format_shortcut(&sc)), Some(sc), "{k}");
            assert_eq!(Cmd::from_id(&c.id()), Some(c));
        }
        // keine doppelten Standardbelegungen
        let mut seen = std::collections::HashSet::new();
        for c in Cmd::all() {
            let k = c.default_key();
            if !k.is_empty() {
                assert!(seen.insert(k.to_ascii_lowercase()), "doppelt: {k}");
            }
        }
    }
}
