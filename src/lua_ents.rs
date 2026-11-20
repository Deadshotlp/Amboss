//! Lua-Entities aus Garry's-Mod-Addons und -Gamemodes erkennen (Scripted Entities und Waffen ohne FGD).
//! Gelesen wird aus `lua/entities`, `lua/weapons` und `gamemodes/<gm>/entities/{entities,weapons}` –
//! auch in Workshop-Archiven. Daraus werden Klassen für die Entity-Liste: Name, Kategorie, Modell,
//! Schlüssel aus `ENT:KeyValue`, Eingänge aus `ENT:AcceptInput`, Ausgänge aus `TriggerOutput`.
//! Eine echte FGD hat immer Vorrang.

use crate::fgd::{ClassKind, EntityClass, Fgd, IoDef, Property};
use crate::gamefs::{AssetSource, GameFs};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LuaEnt {
    pub class: String,
    /// "entity" oder "weapon"
    pub what: &'static str,
    /// ENT.Type ("anim", "point", "brush", "ai", "nextbot", "filter")
    pub ty: String,
    pub print_name: String,
    pub category: String,
    pub base: String,
    pub model: String,
    pub keys: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    /// Addon, Gamemode oder „Spiel“
    pub source: String,
}

/// `NAME.Feld = "Wert"` (auch mit einfachen Anführungszeichen)
fn assign(text: &str, lhs: &[&str]) -> Option<String> {
    for line in text.lines() {
        let t = line.trim_start();
        for l in lhs {
            if let Some(rest) = t.strip_prefix(l) {
                let rest = rest.trim_start();
                let Some(rest) = rest.strip_prefix('=') else { continue };
                let rest = rest.trim_start();
                let q = rest.chars().next()?;
                if q != '"' && q != '\'' {
                    continue;
                }
                let body = &rest[1..];
                if let Some(end) = body.find(q) {
                    return Some(body[..end].to_string());
                }
            }
        }
    }
    None
}

/// Alle `== "x"`-Vergleiche im Körper der Funktion `header` (bis zur nächsten Funktion auf oberster Ebene)
fn compared_strings(text: &str, header: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(i) = text[pos..].find(header) {
        let start = pos + i + header.len();
        let end = text[start..].find("\nfunction ").map(|e| start + e).unwrap_or(text.len());
        let body = &text[start..end];
        let mut p = 0;
        while let Some(j) = body[p..].find("==") {
            let rest = body[p + j + 2..].trim_start();
            if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
                if let Some(e) = rest[1..].find(q) {
                    let s = &rest[1..1 + e];
                    if !s.is_empty() && s.len() < 64 && !s.contains(' ') && !out.iter().any(|x: &String| x.eq_ignore_ascii_case(s)) {
                        out.push(s.to_string());
                    }
                }
            }
            p += j + 2;
        }
        pos = end;
    }
    out
}

/// `TriggerOutput("Name"` – Ausgänge
fn outputs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in text.match_indices("TriggerOutput(") {
        let rest = text[i + "TriggerOutput(".len()..].trim_start();
        if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
            if let Some(e) = rest[1..].find(q) {
                let s = rest[1..1 + e].to_string();
                if !s.is_empty() && !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// Text einer Lua-Datei auswerten
pub fn parse(class: &str, what: &'static str, text: &str) -> LuaEnt {
    let (t, pfx) = if what == "weapon" { ("SWEP", "SWEP.") } else { ("ENT", "ENT.") };
    let f = |field: &str| assign(text, &[&format!("{pfx}{field}")]);
    let mut model = if what == "weapon" { f("WorldModel") } else { f("Model") }.unwrap_or_default();
    if model.is_empty() {
        // self:SetModel("…") im Spawn/Initialize
        if let Some(i) = text.find("SetModel(") {
            let rest = text[i + 9..].trim_start();
            if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
                if let Some(e) = rest[1..].find(q) {
                    model = rest[1..1 + e].to_string();
                }
            }
        }
    }
    LuaEnt {
        class: class.to_ascii_lowercase(),
        what,
        ty: f("Type").unwrap_or_default().to_ascii_lowercase(),
        print_name: f("PrintName").unwrap_or_default(),
        category: f("Category").unwrap_or_default(),
        base: f("Base").unwrap_or_default(),
        model: model.replace('\\', "/").to_ascii_lowercase(),
        keys: compared_strings(text, &format!("function {t}:KeyValue")),
        inputs: compared_strings(text, &format!("function {t}:AcceptInput")),
        outputs: outputs(text),
        source: String::new(),
    }
}

/// Zwei Auswertungen derselben Klasse zusammenführen (shared.lua, init.lua, cl_init.lua)
fn merge(a: &mut LuaEnt, b: LuaEnt) {
    for (x, y) in [(&mut a.ty, b.ty), (&mut a.print_name, b.print_name), (&mut a.category, b.category), (&mut a.base, b.base), (&mut a.model, b.model)] {
        if x.is_empty() {
            *x = y;
        }
    }
    for (x, y) in [(&mut a.keys, b.keys), (&mut a.inputs, b.inputs), (&mut a.outputs, b.outputs)] {
        for v in y {
            if !x.iter().any(|e| e.eq_ignore_ascii_case(&v)) {
                x.push(v);
            }
        }
    }
}

/// Pfad → (Klasse, entity/weapon, Gamemode)
pub fn class_of(path: &str) -> Option<(String, &'static str, Option<String>)> {
    let p = path.to_ascii_lowercase();
    let (rest, gm) = if let Some(r) = p.strip_prefix("lua/") {
        (r.to_string(), None)
    } else if let Some(r) = p.strip_prefix("gamemodes/") {
        let (gm, r) = r.split_once('/')?;
        (r.strip_prefix("entities/")?.to_string(), Some(gm.to_string()))
    } else {
        return None;
    };
    let (what, rest) = if let Some(r) = rest.strip_prefix("entities/") {
        ("entity", r)
    } else if let Some(r) = rest.strip_prefix("weapons/") {
        ("weapon", r)
    } else {
        return None;
    };
    let class = match rest.split_once('/') {
        Some((dir, file)) => {
            // Ordner-Entity: nur die üblichen Dateien
            if !matches!(file, "shared.lua" | "init.lua" | "cl_init.lua") {
                return None;
            }
            dir.to_string()
        }
        None => rest.strip_suffix(".lua")?.to_string(),
    };
    if class.is_empty() || class.contains('.') {
        return None;
    }
    Some((class, what, gm))
}

/// Alle Lua-Entities und -Waffen im Dateisystem
pub fn scan(fs: &GameFs) -> Vec<LuaEnt> {
    let mut files = fs.list_files("lua/entities/", &[".lua"]);
    files.extend(fs.list_files("lua/weapons/", &[".lua"]));
    files.extend(fs.list_files("gamemodes/", &[".lua"]).into_iter().filter(|f| f.contains("/entities/entities/") || f.contains("/entities/weapons/")));
    let mut map: BTreeMap<String, LuaEnt> = BTreeMap::new();
    for f in files {
        let Some((class, what, gm)) = class_of(&f) else { continue };
        let Some(bytes) = fs.read(&f) else { continue };
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(256 * 1024)]);
        let mut e = parse(&class, what, &text);
        e.source = match (gm, fs.source_of(&f)) {
            (Some(g), _) => format!("Gamemode {g}"),
            (None, Some(AssetSource::Addon { name, .. })) => name,
            _ => String::new(),
        };
        match map.get_mut(&e.class) {
            Some(a) => merge(a, e),
            None => {
                map.insert(e.class.clone(), e);
            }
        }
    }
    map.into_values().collect()
}

/// In die Entity-Liste übernehmen (nur Klassen, die keine FGD kennt). Liefert die Anzahl neuer Klassen.
pub fn apply(fgd: &mut Fgd, ents: &[LuaEnt]) -> usize {
    let mut n = 0;
    for e in ents {
        if fgd.class(&e.class).is_some() {
            continue;
        }
        // Basisklassen der Engine (base_anim …) sind keine platzierbaren Entities
        if e.class.starts_with("base_") || e.ty == "filter" && e.class.starts_with("base") {
            continue;
        }
        // in der aktuellen Sprache zusammensetzen (Namen und Kategorien stammen aus dem Addon)
        use crate::i18n::{tr, tr_dyn};
        let mut desc = (if e.what == "weapon" { tr("Lua-Waffe") } else { tr("Lua-Entity") }).to_string();
        if !e.print_name.is_empty() && !e.print_name.starts_with('#') {
            desc.push_str(&format!(": {}", e.print_name));
        }
        if !e.category.is_empty() {
            desc.push_str(&format!(" ({})", e.category));
        }
        if !e.source.is_empty() {
            desc.push_str(&format!(" – {}", tr_dyn(&format!("aus {}", e.source))));
        }
        desc.push_str(". ");
        desc.push_str(tr("Ohne FGD erkannt; Schlüssel aus dem Lua-Code."));
        let mut props = vec![Property { name: "targetname".into(), kind: "target_source".into(), display: "Name".into(), ..Default::default() }];
        if e.ty != "brush" {
            props.push(Property { name: "angles".into(), kind: "angle".into(), display: "Pitch Yaw Roll (Y Z X)".into(), default: "0 0 0".into(), ..Default::default() });
        }
        for k in &e.keys {
            if !props.iter().any(|p| p.name.eq_ignore_ascii_case(k)) {
                props.push(Property { name: k.clone(), kind: "string".into(), display: k.clone(), ..Default::default() });
            }
        }
        let io = |v: &Vec<String>| v.iter().map(|s| IoDef { name: s.clone(), kind: "void".into(), description: String::new() }).collect::<Vec<_>>();
        let class = EntityClass {
            kind: if e.ty == "brush" { ClassKind::Solid } else { ClassKind::Point },
            name: e.class.clone(),
            description: desc,
            bases: Vec::new(),
            size: if e.model.is_empty() { Some(([-8.0, -8.0, -8.0], [8.0, 8.0, 8.0])) } else { None },
            color: Some(if e.what == "weapon" { [255, 200, 0] } else { [120, 200, 255] }),
            studio: (!e.model.is_empty()).then(|| e.model.clone()),
            sprite: None,
            props,
            inputs: io(&e.inputs),
            outputs: io(&e.outputs),
        };
        fgd.classes.insert(e.class.to_ascii_lowercase(), class);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARED: &str = r#"
ENT.Type = "anim"
ENT.Base = "base_gmodentity"
ENT.PrintName = "Geldautomat"
ENT.Category = "DarkRP Extras"
ENT.Spawnable = true

function ENT:KeyValue(key, value)
    if key == "startmoney" then self.Money = tonumber(value)
    elseif key == 'owner_team' then self.Team = value end
    if key == "startmoney" then end
end

function ENT:AcceptInput(name, activator, caller, data)
    if name == "Lock" then self.Locked = true return true end
    if name == "Unlock" then self.Locked = false return true end
end

function ENT:Use(ply)
    self:TriggerOutput("OnUsed", ply)
    if ply:Team() == "x" then end
end
"#;

    #[test]
    fn parses_entity_file() {
        let e = parse("atm_machine", "entity", SHARED);
        assert_eq!(e.ty, "anim");
        assert_eq!(e.print_name, "Geldautomat");
        assert_eq!(e.category, "DarkRP Extras");
        assert_eq!(e.keys, vec!["startmoney", "owner_team"]);
        assert_eq!(e.inputs, vec!["Lock", "Unlock"]);
        assert_eq!(e.outputs, vec!["OnUsed"]);
        let init = "function ENT:Initialize()\n self:SetModel( \"models/props_lab/reciever_cart.mdl\" )\nend";
        assert_eq!(parse("x", "entity", init).model, "models/props_lab/reciever_cart.mdl");
        let swep = "SWEP.PrintName = \"Taser\"\nSWEP.WorldModel = \"models/weapons/w_pistol.mdl\"\nSWEP.Category = 'Polizei'";
        let w = parse("weapon_taser", "weapon", swep);
        assert_eq!((w.print_name.as_str(), w.model.as_str(), w.category.as_str()), ("Taser", "models/weapons/w_pistol.mdl", "Polizei"));
    }

    #[test]
    fn paths_to_classes() {
        assert_eq!(class_of("lua/entities/atm_machine/shared.lua"), Some(("atm_machine".into(), "entity", None)));
        assert_eq!(class_of("lua/entities/my_ent.lua"), Some(("my_ent".into(), "entity", None)));
        assert_eq!(class_of("lua/weapons/weapon_taser/shared.lua"), Some(("weapon_taser".into(), "weapon", None)));
        assert_eq!(class_of("gamemodes/terrortown/entities/entities/ttt_health_station.lua"), Some(("ttt_health_station".into(), "entity", Some("terrortown".into()))));
        assert_eq!(class_of("lua/entities/atm_machine/sh_config.lua"), None);
        assert_eq!(class_of("lua/autorun/foo.lua"), None);
    }

    #[test]
    fn scans_vpk_and_applies() {
        let dir = std::env::temp_dir().join(format!("amboss_luaents_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lua/entities/atm_machine")).unwrap();
        std::fs::write(dir.join("lua/entities/atm_machine/shared.lua"), SHARED).unwrap();
        std::fs::write(dir.join("lua/entities/atm_machine/init.lua"), "function ENT:Initialize() self:SetModel(\"models/props_lab/reciever_cart.mdl\") end").unwrap();
        std::fs::create_dir_all(dir.join("lua/entities")).unwrap();
        // prop_physics gibt es schon in der FGD – darf nicht überschrieben werden
        std::fs::write(dir.join("lua/entities/prop_physics.lua"), "ENT.Type = \"anim\"").unwrap();
        let mut fs = GameFs::default();
        fs.add_dir(&dir);
        let ents = scan(&fs);
        let atm = ents.iter().find(|e| e.class == "atm_machine").unwrap();
        assert_eq!(atm.model, "models/props_lab/reciever_cart.mdl", "init.lua ergänzt shared.lua");
        let mut fgd = Fgd::builtin();
        let before = fgd.class("prop_physics").unwrap().description.clone();
        assert_eq!(apply(&mut fgd, &ents), 1);
        let c = fgd.class("atm_machine").unwrap();
        assert!(c.description.contains("Geldautomat") && c.description.contains("DarkRP Extras"));
        assert_eq!(c.studio.as_deref(), Some("models/props_lab/reciever_cart.mdl"));
        assert!(fgd.all_props("atm_machine").iter().any(|p| p.name == "startmoney"));
        assert!(c.inputs.iter().any(|i| i.name == "Lock") && c.outputs.iter().any(|o| o.name == "OnUsed"));
        assert_eq!(fgd.class("prop_physics").unwrap().description, before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
