//! FGD-Parser (Forge Game Data) – Entity-Definitionen für Hammer-kompatible Editoren.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Base,
    Point,
    Solid,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct Choice {
    pub value: String,
    pub name: String,
    /// Für spawnflags: standardmäßig gesetzt?
    pub default_on: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Property {
    pub name: String,
    pub kind: String,
    pub display: String,
    pub default: String,
    pub description: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone)]
pub struct IoDef {
    pub name: String,
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct EntityClass {
    pub kind: ClassKind,
    pub name: String,
    pub description: String,
    pub bases: Vec<String>,
    pub size: Option<([f64; 3], [f64; 3])>,
    pub color: Option<[u8; 3]>,
    pub studio: Option<String>,
    pub sprite: Option<String>,
    pub props: Vec<Property>,
    pub inputs: Vec<IoDef>,
    pub outputs: Vec<IoDef>,
}

#[derive(Debug, Default, Clone)]
pub struct Fgd {
    pub classes: HashMap<String, EntityClass>,
    /// Basisklassen getrennt: Valves FGD hat z. B. die Basis „Light“ UND das Entity „light“ –
    /// in einer Tabelle (Groß-/Kleinschreibung egal) würde das eine das andere überschreiben
    pub base_classes: HashMap<String, EntityClass>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
enum T {
    Ident(String),
    Str(String),
    Sym(char),
}

fn tokenize(src: &str) -> Vec<T> {
    let mut out = Vec::new();
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < cs.len() && cs[i] != '"' {
                s.push(cs[i]);
                i += 1;
            }
            i += 1;
            out.push(T::Str(s));
        } else if "@()[]=:,+".contains(c) {
            out.push(T::Sym(c));
            i += 1;
        } else {
            let mut s = String::new();
            while i < cs.len() && !cs[i].is_whitespace() && !"@()[]=:,+\"".contains(cs[i]) {
                s.push(cs[i]);
                i += 1;
            }
            out.push(T::Ident(s));
        }
    }
    out
}

struct P {
    t: Vec<T>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&T> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<T> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn is_sym(&self, c: char) -> bool {
        self.peek() == Some(&T::Sym(c))
    }
    fn eat_sym(&mut self, c: char) -> bool {
        if self.is_sym(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// String mit "+"-Verkettung
    fn string(&mut self) -> String {
        let mut s = match self.peek() {
            Some(T::Str(s)) => {
                let s = s.clone();
                self.i += 1;
                s
            }
            _ => return String::new(),
        };
        while self.is_sym('+') {
            self.i += 1;
            if let Some(T::Str(more)) = self.peek() {
                s.push_str(more);
                self.i += 1;
            }
        }
        s
    }
    /// Wert: String, Ident oder Zahl
    fn value(&mut self) -> String {
        match self.peek() {
            Some(T::Str(_)) => self.string(),
            Some(T::Ident(s)) => {
                let s = s.clone();
                self.i += 1;
                s
            }
            _ => String::new(),
        }
    }
    /// Überspringt bis zur passenden schließenden Klammer
    fn skip_group(&mut self, open: char, close: char) {
        let mut depth = 0;
        while let Some(t) = self.next() {
            if t == T::Sym(open) {
                depth += 1;
            } else if t == T::Sym(close) {
                depth -= 1;
                if depth <= 0 {
                    return;
                }
            }
        }
    }
    /// Rohe Argumente innerhalb ( ... ) als Text
    fn args(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.eat_sym('(') {
            return out;
        }
        let mut cur = String::new();
        while let Some(t) = self.next() {
            match t {
                T::Sym(')') => break,
                T::Sym(',') => out.push(std::mem::take(&mut cur).trim().to_string()),
                T::Ident(s) | T::Str(s) => {
                    if !cur.is_empty() {
                        cur.push(' ');
                    }
                    cur.push_str(&s);
                }
                T::Sym(c) => cur.push(c),
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur.trim().to_string());
        }
        out
    }
}

fn nums(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|x| x.parse().ok()).collect()
}

impl Fgd {
    pub fn builtin() -> Fgd {
        let mut f = Fgd::default();
        for part in BUILTIN_FGD {
            f.parse_str(part, None);
        }
        f
    }

    pub fn load_file(&mut self, path: &Path) -> Result<(), String> {
        let canon = path.to_path_buf();
        if self.files.contains(&canon) {
            return Ok(());
        }
        let src = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = String::from_utf8_lossy(&src).to_string();
        self.files.push(canon);
        self.parse_str(&src, path.parent());
        Ok(())
    }

    pub fn parse_str(&mut self, src: &str, dir: Option<&Path>) {
        let mut p = P { t: tokenize(src), i: 0 };
        while let Some(t) = p.next() {
            if t != T::Sym('@') {
                continue;
            }
            let Some(T::Ident(kind)) = p.next() else { continue };
            let lk = kind.to_ascii_lowercase();
            match lk.as_str() {
                "include" => {
                    let file = p.string();
                    if let Some(dir) = dir {
                        let _ = self.load_file(&dir.join(file));
                    }
                }
                "mapsize" => {
                    p.args();
                }
                "materialexclusion" | "autovisgroup" | "visgroupfilter" => {
                    // bis zum nächsten '[' ... ']' überspringen
                    while let Some(t) = p.peek() {
                        if *t == T::Sym('[') {
                            p.skip_group('[', ']');
                            break;
                        }
                        if *t == T::Sym('@') {
                            break;
                        }
                        p.i += 1;
                    }
                }
                _ => {
                    let ckind = match lk.as_str() {
                        "baseclass" => ClassKind::Base,
                        "pointclass" | "npcclass" | "keyframeclass" | "moveclass" | "filterclass" => ClassKind::Point,
                        "solidclass" => ClassKind::Solid,
                        _ => ClassKind::Other,
                    };
                    if let Some(c) = parse_class(&mut p, ckind) {
                        let key = c.name.to_ascii_lowercase();
                        if ckind == ClassKind::Base {
                            // als Entity-Klasse nur, wenn es kein gleichnamiges echtes Entity gibt
                            if self.classes.get(&key).is_none_or(|e| e.kind == ClassKind::Base) {
                                self.classes.insert(key.clone(), c.clone());
                            }
                            self.base_classes.insert(key, c);
                        } else {
                            self.classes.insert(key, c);
                        }
                    }
                }
            }
        }
    }

    pub fn class(&self, name: &str) -> Option<&EntityClass> {
        self.classes.get(&name.to_ascii_lowercase())
    }

    /// Klasse, auf die sich `base(...)` bezieht: Basisklasse bevorzugt
    fn base_class(&self, name: &str) -> Option<&EntityClass> {
        let k = name.to_ascii_lowercase();
        self.base_classes.get(&k).or_else(|| self.classes.get(&k))
    }

    /// Alle Properties inklusive Basisklassen (Basis zuerst)
    pub fn all_props(&self, name: &str) -> Vec<Property> {
        let mut out: Vec<Property> = Vec::new();
        if let Some(c) = self.class(name) {
            self.collect_props(c, &mut out, 0);
        }
        out
    }

    fn collect_props(&self, c: &EntityClass, out: &mut Vec<Property>, depth: usize) {
        if depth > 16 {
            return;
        }
        for b in &c.bases {
            if let Some(bc) = self.base_class(b) {
                self.collect_props(bc, out, depth + 1);
            }
        }
        for p in &c.props {
            if let Some(existing) = out.iter_mut().find(|e| e.name.eq_ignore_ascii_case(&p.name)) {
                *existing = p.clone();
            } else {
                out.push(p.clone());
            }
        }
    }

    pub fn all_io(&self, name: &str, outputs: bool) -> Vec<IoDef> {
        let mut out = Vec::new();
        if let Some(c) = self.class(name) {
            self.collect_io(c, outputs, &mut out, 0);
        }
        out
    }

    fn collect_io(&self, c: &EntityClass, outputs: bool, out: &mut Vec<IoDef>, depth: usize) {
        if depth > 16 {
            return;
        }
        for b in &c.bases {
            if let Some(bc) = self.base_class(b) {
                self.collect_io(bc, outputs, out, depth + 1);
            }
        }
        for io in if outputs { &c.outputs } else { &c.inputs } {
            if !out.iter().any(|e: &IoDef| e.name == io.name) {
                out.push(io.clone());
            }
        }
    }

    /// Größe/Farbe auch über Basisklassen auflösen
    pub fn resolve_size(&self, name: &str) -> Option<([f64; 3], [f64; 3])> {
        self.resolve(name, 0, &|c| c.size)
    }

    pub fn resolve_color(&self, name: &str) -> Option<[u8; 3]> {
        self.resolve(name, 0, &|c| c.color)
    }

    fn resolve<R>(&self, name: &str, depth: usize, f: &dyn Fn(&EntityClass) -> Option<R>) -> Option<R> {
        if depth > 16 {
            return None;
        }
        let c = if depth == 0 { self.class(name) } else { self.base_class(name) }?;
        f(c).or_else(|| c.bases.iter().find_map(|b| self.resolve(b, depth + 1, f)))
    }

    pub fn names(&self, kind: ClassKind) -> Vec<String> {
        let mut v: Vec<String> = self.classes.values().filter(|c| c.kind == kind).map(|c| c.name.clone()).collect();
        v.sort();
        v
    }
}

fn parse_class(p: &mut P, kind: ClassKind) -> Option<EntityClass> {
    let mut c = EntityClass {
        kind,
        name: String::new(),
        description: String::new(),
        bases: vec![],
        size: None,
        color: None,
        studio: None,
        sprite: None,
        props: vec![],
        inputs: vec![],
        outputs: vec![],
    };
    // Helfer bis '='
    loop {
        match p.peek()? {
            T::Sym('=') => {
                p.i += 1;
                break;
            }
            T::Ident(h) => {
                let h = h.to_ascii_lowercase();
                p.i += 1;
                let args = if p.is_sym('(') { p.args() } else { vec![] };
                match h.as_str() {
                    "base" => c.bases.extend(args.iter().map(|a| a.trim().to_string())),
                    "size" => {
                        if args.len() == 2 {
                            let a = nums(&args[0]);
                            let b = nums(&args[1]);
                            if a.len() == 3 && b.len() == 3 {
                                c.size = Some(([a[0], a[1], a[2]], [b[0], b[1], b[2]]));
                            }
                        } else if args.len() == 1 {
                            let a = nums(&args[0]);
                            if a.len() == 3 {
                                c.size = Some(([-a[0] / 2.0, -a[1] / 2.0, -a[2] / 2.0], [a[0] / 2.0, a[1] / 2.0, a[2] / 2.0]));
                            }
                        }
                    }
                    "color" => {
                        if let Some(a) = args.first() {
                            let n = nums(a);
                            if n.len() == 3 {
                                c.color = Some([n[0] as u8, n[1] as u8, n[2] as u8]);
                            }
                        }
                    }
                    "studio" | "studioprop" => c.studio = args.first().cloned().filter(|s| !s.is_empty()),
                    "iconsprite" | "sprite" => c.sprite = args.first().cloned(),
                    _ => {}
                }
            }
            _ => {
                p.i += 1;
            }
        }
    }
    c.name = match p.next()? {
        T::Ident(s) | T::Str(s) => s,
        _ => return None,
    };
    if p.eat_sym(':') {
        c.description = p.string();
    }
    if !p.eat_sym('[') {
        return Some(c);
    }
    // Properties
    loop {
        match p.peek() {
            None => break,
            Some(T::Sym(']')) => {
                p.i += 1;
                break;
            }
            Some(T::Ident(name)) => {
                let name = name.clone();
                p.i += 1;
                let lname = name.to_ascii_lowercase();
                if (lname == "input" || lname == "output") && matches!(p.peek(), Some(T::Ident(_))) {
                    let Some(T::Ident(ioname)) = p.next() else { continue };
                    let kind = p.args().join(" ");
                    let desc = if p.eat_sym(':') { p.string() } else { String::new() };
                    let io = IoDef { name: ioname, kind, description: desc };
                    if lname == "input" {
                        c.inputs.push(io);
                    } else {
                        c.outputs.push(io);
                    }
                    continue;
                }
                let kind = p.args().join(" ").to_ascii_lowercase();
                // optionale Flags wie "readonly" / "report"
                while let Some(T::Ident(_)) = p.peek() {
                    p.i += 1;
                }
                let mut prop = Property { name, kind, ..Default::default() };
                if p.eat_sym(':') {
                    prop.display = p.string();
                    if p.eat_sym(':') {
                        if !p.is_sym(':') && !p.is_sym('=') && !p.is_sym(']') {
                            prop.default = p.value();
                        }
                        if p.eat_sym(':') {
                            prop.description = p.string();
                        }
                    }
                }
                if p.eat_sym('=') && p.eat_sym('[') {
                    loop {
                        match p.peek() {
                            None => break,
                            Some(T::Sym(']')) => {
                                p.i += 1;
                                break;
                            }
                            Some(T::Ident(_)) | Some(T::Str(_)) => {
                                let value = p.value();
                                let mut ch = Choice { value, ..Default::default() };
                                if p.eat_sym(':') {
                                    ch.name = p.string();
                                    if p.eat_sym(':') {
                                        ch.default_on = p.value() == "1";
                                    }
                                }
                                prop.choices.push(ch);
                            }
                            _ => {
                                p.i += 1;
                            }
                        }
                    }
                }
                if prop.display.is_empty() {
                    prop.display = prop.name.clone();
                }
                c.props.push(prop);
            }
            Some(_) => {
                p.i += 1;
            }
        }
    }
    Some(c)
}

/// Kleiner eingebauter Satz der wichtigsten Entities (falls keine FGD geladen ist).
/// Eingebaute Entity-Liste (HL2, Episoden, Garry's Mod) – eigene Beschreibungen, siehe assets/fgd
pub const BUILTIN_FGD: &[&str] = &[
    include_str!("../assets/fgd/amboss_base.fgd"),
    include_str!("../assets/fgd/amboss_point.fgd"),
    include_str!("../assets/fgd/amboss_actors.fgd"),
    include_str!("../assets/fgd/amboss_solid.fgd"),
    include_str!("../assets/fgd/amboss_gamemodes.fgd"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_complete() {
        let f = Fgd::builtin();
        let point = f.names(ClassKind::Point);
        let solid = f.names(ClassKind::Solid);
        assert!(point.len() > 250, "nur {} Punkt-Entities", point.len());
        assert!(solid.len() > 60, "nur {} Brush-Entities", solid.len());
        for c in ["prop_door_rotating", "npc_citizen", "npc_combine_s", "weapon_ar2", "item_healthkit", "info_particle_system", "env_projectedtexture", "point_template", "logic_case", "math_counter", "filter_activator_name", "game_text", "path_track", "keyframe_rope", "move_rope", "phys_hinge", "lua_run", "ai_relationship", "scripted_sequence", "env_skypaint", "func_instance", "info_overlay", "light_dynamic", "shadow_control"] {
            assert!(point.contains(&c.to_string()), "Punkt-Entity fehlt: {c}");
        }
        for c in ["func_door", "func_door_rotating", "func_movelinear", "func_rotating", "func_tracktrain", "func_breakable_surf", "func_areaportalwindow", "func_occluder", "func_precipitation", "func_monitor", "func_tank", "trigger_changelevel", "trigger_look", "trigger_vphysics_motion", "color_correction_volume", "momentary_rot_button"] {
            assert!(solid.contains(&c.to_string()), "Brush-Entity fehlt: {c}");
        }
        // Basisklassen werden aufgelöst, jede Klasse hat eine Beschreibung
        assert!(f.all_props("npc_combine_s").iter().any(|p| p.name == "squadname"));
        assert!(f.all_props("prop_physics").iter().any(|p| p.name == "model" && p.default.contains("wood_crate")));
        assert!(f.all_io("func_door", true).iter().any(|o| o.name == "OnFullyOpen"));
        assert!(f.all_io("trigger_once", false).iter().any(|o| o.name == "Enable"));
        for n in point.iter().chain(solid.iter()) {
            assert!(!f.class(n).unwrap().description.is_empty(), "keine Beschreibung: {n}");
        }
    }

    #[test]
    fn builtin_parses() {
        let f = Fgd::builtin();
        let c = f.class("light").unwrap();
        assert_eq!(c.kind, ClassKind::Point);
        assert_eq!(c.color, Some([255, 255, 0]));
        let props = f.all_props("light");
        assert!(props.iter().any(|p| p.name == "targetname"));
        assert!(props.iter().any(|p| p.name == "_light" && p.default == "255 255 255 200"));
        let flags = props.iter().find(|p| p.name == "spawnflags").unwrap();
        assert_eq!(flags.choices.len(), 1);
        assert_eq!(f.resolve_size("info_player_start"), Some(([-16.0, -16.0, 0.0], [16.0, 16.0, 72.0])));
        assert!(f.names(ClassKind::Solid).contains(&"func_detail".to_string()));
        assert!(f.all_io("trigger_multiple", true).iter().any(|o| o.name == "OnStartTouch"));
    }

    /// Valves FGD: Basisklasse „Light“ und Entity „light“ dürfen sich nicht überschreiben
    #[test]
    fn base_and_entity_with_same_name() {
        let mut f = Fgd::default();
        f.parse_str(r#"@BaseClass = Targetname
[
	targetname(target_source) : "Name"
]
@BaseClass = Light
[
	_light(color255) : "Brightness" : "255 255 255 200"
	_lightHDR(color255) : "BrightnessHDR" : "-1 -1 -1 1"
	style(Choices) : "Appearance" : 0 =
	[
		0 : "Normal"
		10: "Fluorescent flicker"
	]
	_constant_attn(string) : "Constant" : "0"
]
@PointClass base(Targetname, Light) iconsprite("editor/light.vmt") sphere(_fifty_percent_distance) sphere(_zero_percent_distance) light() = light :
	"An invisible omnidirectional lightsource."
[
	spawnflags(Flags) =
	[
		1 : "Initially dark" : 0
	]
	_distance(integer) : "Maximum Distance" : 0
]
@PointClass base(Angles) iconsprite("editor/light_env.vmt") = light_environment : 
	"Sets the color and angle of the light from the sun and sky."
[
	pitch(integer) : "Pitch" : 0
	_light(color255) : "Brightness" : "255 255 255 200"
	_ambient(color255) : "Ambient" : "255 255 255 20"
]
"#, None);
        let props = f.all_props("light");
        assert!(props.iter().any(|p| p.name == "_light" && p.kind == "color255" && p.default == "255 255 255 200"), "{:?}", props.iter().map(|p| &p.name).collect::<Vec<_>>());
        assert!(props.iter().any(|p| p.name == "style"));
        assert!(props.iter().any(|p| p.name == "_distance"));
        assert!(props.iter().any(|p| p.name == "targetname"));
        assert_eq!(f.class("light").unwrap().kind, ClassKind::Point);
        assert!(f.all_props("light_environment").iter().any(|p| p.name == "_ambient"));
    }

    #[test]
    fn valve_style() {
        let src = r#"
@PointClass base(Targetname) studio("models/x.mdl") = test_ent : "Desc " +
	"continued"
[
	health(integer) readonly : "Health" : 100 : "Some " + "help"
	empty(string) : "Empty" : : "No default"
	input Kill(void) : "Kill it"
]"#;
        let mut f = Fgd::default();
        f.parse_str(src, None);
        let c = f.class("test_ent").unwrap();
        assert_eq!(c.description, "Desc continued");
        assert_eq!(c.props[0].default, "100");
        assert_eq!(c.props[0].description, "Some help");
        assert_eq!(c.props[1].default, "");
        assert_eq!(c.props[1].description, "No default");
        assert_eq!(c.inputs[0].name, "Kill");
    }
}
