//! Gamemode-Vorlagen und -Checkliste: was eine Map für TTT, Murder, Prop Hunt, DarkRP oder Deathrun braucht
//! (Spawnpunkte je Team, Waffen, Türen, Props …), mit Prüfung und Knöpfen zum Hinzufügen.
//! Die Werte sind übliche Empfehlungen der Gamemodes, keine festen Regeln.

use crate::editor::Editor;
use crate::geometry::Aabb;
use crate::prefabs::{b, ent, tie};
use glam::DVec3;

/// Eine Anforderung: mindestens `min` Entities einer der Klassen (`*` am Ende = Präfix)
pub struct Need {
    pub label: &'static str,
    pub classes: &'static [&'static str],
    pub min: usize,
    /// nur empfohlen (gelb statt rot)
    pub optional: bool,
    pub hint: &'static str,
    /// Klasse, die „Hinzufügen“ setzt
    pub add: &'static str,
}

pub struct Gamemode {
    pub id: &'static str,
    pub name: &'static str,
    /// übliche Präfixe des Map-Namens
    pub prefixes: &'static [&'static str],
    pub description: &'static str,
    pub needs: &'static [Need],
    pub tips: &'static [&'static str],
}

const SPAWNS: &[&str] = &["info_player_start", "info_player_deathmatch", "info_player_terrorist", "info_player_counterterrorist", "info_player_combine", "info_player_rebel"];

pub const GAMEMODES: &[Gamemode] = &[
    Gamemode {
        id: "ttt",
        name: "Trouble in Terrorist Town",
        prefixes: &["ttt_"],
        description: "Unschuldige gegen versteckte Verräter. Braucht viele Spawnpunkte und verteilte Waffen.",
        needs: &[
            Need { label: "Spawnpunkte", classes: SPAWNS, min: 16, optional: false, hint: "Mindestens so viele wie Spieler auf dem Server; TTT nimmt alle Spawn-Arten.", add: "info_player_deathmatch" },
            Need { label: "Waffen", classes: &["ttt_random_weapon", "weapon_zm_*", "weapon_ttt_*"], min: 8, optional: false, hint: "ttt_random_weapon oder feste TTT-Waffen, über die Map verteilt.", add: "ttt_random_weapon" },
            Need { label: "Munition", classes: &["ttt_random_ammo", "item_*_ttt"], min: 6, optional: false, hint: "ttt_random_ammo oder feste Munition neben den Waffen.", add: "ttt_random_ammo" },
            Need { label: "Map-Einstellungen", classes: &["ttt_map_settings"], min: 1, optional: true, hint: "ttt_map_settings: Brechstange an Türen und Knöpfen, Spielermodell.", add: "ttt_map_settings" },
            Need { label: "Verräter-Fallen", classes: &["ttt_traitor_button"], min: 1, optional: true, hint: "ttt_traitor_button: nur Verräter sehen und benutzen ihn (z. B. für Fallen).", add: "ttt_traitor_button" },
        ],
        tips: &["Den Map-Namen mit ttt_ beginnen, sonst taucht sie in der Map-Auswahl vieler Server nicht auf.", "Mehrere Wege und Verstecke; Lüftungsschächte und Fallen machen Verräter-Runden spannend."],
    },
    Gamemode {
        id: "murder",
        name: "Murder",
        prefixes: &["mu_", "ttt_", "cs_"],
        description: "Ein Mörder, ein Zuschauer mit Waffe, alle anderen sammeln Beweise. Kleine, verwinkelte Maps.",
        needs: &[Need { label: "Spawnpunkte", classes: SPAWNS, min: 12, optional: false, hint: "Verteilt, damit niemand neben dem Mörder startet.", add: "info_player_start" }],
        tips: &["Beute-Plätze (Loot) gehören nicht in die Map – sie werden im Spiel festgelegt und auf dem Server gespeichert.", "Dunkle Ecken, Türen und mehrere Wege; offene Flächen sind für den Mörder schwer."],
    },
    Gamemode {
        id: "prophunt",
        name: "Prop Hunt",
        prefixes: &["ph_"],
        description: "Props verstecken sich als Gegenstände, Jäger suchen sie. Braucht viele bewegliche Props.",
        needs: &[
            Need { label: "Spawnpunkte Jäger", classes: &["info_player_counterterrorist"], min: 6, optional: false, hint: "Jäger starten üblicherweise an info_player_counterterrorist.", add: "info_player_counterterrorist" },
            Need { label: "Spawnpunkte Props", classes: &["info_player_terrorist"], min: 6, optional: false, hint: "Props starten üblicherweise an info_player_terrorist.", add: "info_player_terrorist" },
            Need { label: "Physik-Props", classes: &["prop_physics", "prop_physics_multiplayer", "prop_physics_override"], min: 25, optional: false, hint: "Viele verschiedene Gegenstände (Fässer, Kisten, Stühle …) zum Verwandeln.", add: "prop_physics" },
        ],
        tips: &["Den Map-Namen mit ph_ beginnen.", "Props in passenden Größen mischen; zu große Props verraten sich."],
    },
    Gamemode {
        id: "darkrp",
        name: "DarkRP",
        prefixes: &["rp_"],
        description: "Rollenspiel in einer Stadt: Häuser mit Türen zum Kaufen, viel Platz.",
        needs: &[
            Need { label: "Spawnpunkte", classes: SPAWNS, min: 16, optional: false, hint: "Genug für volle Server; Spawns für einzelne Berufe legt man später auf dem Server fest.", add: "info_player_start" },
            Need { label: "Türen", classes: &["prop_door_rotating", "func_door", "func_door_rotating"], min: 10, optional: false, hint: "Türen lassen sich in DarkRP kaufen und abschließen – Häuser, Läden, Wohnungen.", add: "prop_door_rotating" },
        ],
        tips: &["Den Map-Namen mit rp_ beginnen.", "Gefängnis, Polizeiwache und Spawn-Bereich vorsehen; Positionen dafür werden im Spiel gesetzt."],
    },
    Gamemode {
        id: "deathrun",
        name: "Deathrun",
        prefixes: &["deathrun_", "dr_"],
        description: "Läufer rennen durch einen Fallen-Parcours, der Tod löst die Fallen mit Knöpfen aus.",
        needs: &[
            Need { label: "Spawnpunkte Läufer", classes: &["info_player_counterterrorist"], min: 8, optional: false, hint: "Läufer starten üblicherweise an info_player_counterterrorist.", add: "info_player_counterterrorist" },
            Need { label: "Spawnpunkte Tod", classes: &["info_player_terrorist"], min: 1, optional: false, hint: "Der Tod startet üblicherweise an info_player_terrorist.", add: "info_player_terrorist" },
            Need { label: "Fallen-Knöpfe", classes: &["func_button"], min: 3, optional: false, hint: "func_button für den Tod, verbunden mit den Fallen.", add: "" },
            Need { label: "Schadens-Bereiche", classes: &["trigger_hurt"], min: 1, optional: false, hint: "trigger_hurt in Gruben und Fallen.", add: "" },
        ],
        tips: &["Den Map-Namen mit deathrun_ beginnen.", "Am Ende ein Bereich, in dem die Läufer gegen den Tod kämpfen können."],
    },
];

/// Alltagsgegenstände aus HL2 (in GMod immer vorhanden) für Prop Hunt
pub const PROP_MODELS: &[&str] = &["models/props_c17/oildrum001.mdl", "models/props_junk/wood_crate001a.mdl", "models/props_c17/chair02a.mdl", "models/props_junk/trashbin01a.mdl", "models/props_interiors/furniture_chair03a.mdl", "models/props_c17/furnituretable002a.mdl", "models/props_junk/watermelon01.mdl", "models/props_lab/monitor01a.mdl"];

pub fn find(id: &str) -> Option<&'static Gamemode> {
    GAMEMODES.iter().find(|g| g.id == id)
}

fn class_matches(pattern: &str, class: &str) -> bool {
    // ein `*` steht für beliebig viele Zeichen (am Anfang, in der Mitte oder am Ende)
    match pattern.split_once('*') {
        Some((pre, suf)) => {
            let c = class.to_ascii_lowercase();
            c.len() >= pre.len() + suf.len() && c.starts_with(&pre.to_ascii_lowercase()) && c.ends_with(&suf.to_ascii_lowercase())
        }
        None => class.eq_ignore_ascii_case(pattern),
    }
}

/// Wie viele Entities passen zu einer Anforderung
pub fn count(ed: &Editor, need: &Need) -> usize {
    ed.map.entities.iter().filter(|e| need.classes.iter().any(|p| class_matches(p, e.classname()))).count()
}

/// Ergebnis je Anforderung: (Anforderung, Anzahl, erfüllt)
pub fn check(ed: &Editor, gm: &Gamemode) -> Vec<(&'static Need, usize, bool)> {
    gm.needs.iter().map(|n| (n, count(ed, n), count(ed, n) >= n.min)).collect()
}

/// Passt der Dateiname zu einem üblichen Präfix? (None = noch nicht gespeichert)
pub fn prefix_ok(ed: &Editor, gm: &Gamemode) -> Option<bool> {
    let name = ed.path.as_ref()?.file_stem()?.to_string_lossy().to_ascii_lowercase();
    Some(gm.prefixes.is_empty() || gm.prefixes.iter().any(|p| name.starts_with(p)))
}

/// Punkte in einem Raster um `center` (für Spawnpunkte, Waffen …), Abstand `step`
fn grid_points(center: DVec3, n: usize, step: f64) -> Vec<DVec3> {
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    (0..n).map(|i| center + DVec3::new((i % cols) as f64 * step - (cols - 1) as f64 * step / 2.0, (i / cols) as f64 * step - ((n - 1) / cols) as f64 * step / 2.0, 0.0)).collect()
}

/// Halle 2048 × 2048 × 384 mit Licht und Cubemap als Grundlage der Vorlagen
fn hall(ed: &mut Editor) {
    b(ed, "DEV/DEV_MEASUREGENERIC01B", [-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 384.0]);
    ed.hollow(16.0);
    for x in [-640.0, 0.0, 640.0] {
        for y in [-640.0, 0.0, 640.0] {
            ent(ed, "light", [x, y, 320.0], &[("_light", "255 240 220 350")]);
        }
    }
    ent(ed, "env_cubemap", [0.0, 0.0, 96.0], &[]);
}

fn place(ed: &mut Editor, class: &str, center: [f64; 2], n: usize, step: f64, keys: &[(&str, &str)]) {
    for p in grid_points(DVec3::new(center[0], center[1], 0.0), n, step) {
        ent(ed, class, [p.x, p.y, 0.0], keys);
    }
}

/// Kleines Haus mit Tür (func_door) – für DarkRP
fn house(ed: &mut Editor, x: f64, y: f64) {
    let min = [x - 160.0, y - 128.0, 0.0];
    let max = [x + 160.0, y + 128.0, 176.0];
    b(ed, "DEV/DEV_MEASUREGENERIC01", min, max);
    ed.hollow(8.0);
    // Türöffnung in der Vorderwand (−Y) ausschneiden
    let cut = b(ed, "DEV/DEV_MEASUREGENERIC01", [x - 32.0, y - 136.0, 0.0], [x + 32.0, y - 120.0, 112.0]);
    ed.sel = [crate::editor::Obj::Solid(cut)].into_iter().collect();
    ed.carve();
    ed.sel = [crate::editor::Obj::Solid(cut)].into_iter().collect();
    ed.delete_selection();
    let d = b(ed, "DEV/DEV_MEASUREGENERIC01B", [x - 32.0, y - 130.0, 0.0], [x + 32.0, y - 126.0, 112.0]);
    tie(ed, &[d], "func_door", &[("movedir", "0 0 0"), ("speed", "120"), ("wait", "-1"), ("lip", "4"), ("spawnflags", "256")]);
    ent(ed, "light", [x, y, 140.0], &[("_light", "255 230 200 120")]);
}

/// Vorlage für einen Gamemode bauen
pub fn build_template(ed: &mut Editor, id: &str) {
    hall(ed);
    match id {
        "ttt" => {
            place(ed, "info_player_deathmatch", [-600.0, -600.0], 16, 64.0, &[("angles", "0 45 0")]);
            for (i, p) in grid_points(DVec3::ZERO, 8, 360.0).into_iter().enumerate() {
                ent(ed, "ttt_random_weapon", [p.x, p.y, 8.0], &[("auto_ammo", "1")]);
                if i < 6 {
                    ent(ed, "ttt_random_ammo", [p.x + 32.0, p.y, 8.0], &[]);
                }
            }
            ent(ed, "ttt_map_settings", [0.0, 0.0, 64.0], &[]);
            ent(ed, "ttt_traitor_button", [-1000.0, 0.0, 64.0], &[("description", "Falle"), ("wait", "30")]);
        }
        "murder" => place(ed, "info_player_start", [0.0, 0.0], 12, 320.0, &[]),
        "prophunt" => {
            place(ed, "info_player_counterterrorist", [-700.0, -700.0], 6, 64.0, &[("angles", "0 45 0")]);
            place(ed, "info_player_terrorist", [700.0, 700.0], 6, 64.0, &[("angles", "0 225 0")]);
            for (i, p) in grid_points(DVec3::ZERO, 25, 150.0).into_iter().enumerate() {
                ent(ed, "prop_physics", [p.x, p.y, 0.0], &[("model", PROP_MODELS[i % PROP_MODELS.len()]), ("angles", &format!("0 {} 0", (i * 37) % 360))]);
            }
        }
        "darkrp" => {
            place(ed, "info_player_start", [0.0, 0.0], 16, 64.0, &[]);
            for x in [-640.0, 0.0, 640.0] {
                for y in [-640.0, 640.0] {
                    house(ed, x, y);
                }
            }
            // weitere Türen in einer Ladenzeile
            for i in 0..4 {
                house(ed, -720.0 + i as f64 * 480.0, 0.0 + if i % 2 == 0 { 280.0 } else { -280.0 });
            }
        }
        "deathrun" => {
            place(ed, "info_player_counterterrorist", [-800.0, -800.0], 8, 64.0, &[("angles", "0 45 0")]);
            ent(ed, "info_player_terrorist", [800.0, 800.0, 0.0], &[("angles", "0 225 0")]);
            // Grube mit trigger_hurt
            let h = b(ed, "TOOLS/TOOLSTRIGGER", [-256.0, -256.0, 0.0], [256.0, 256.0, 32.0]);
            tie(ed, &[h], "trigger_hurt", &[("damage", "1000"), ("damagetype", "0"), ("spawnflags", "1")]);
            for i in 0..3 {
                let k = b(ed, "DEV/REFLECTIVITY_50", [700.0 + i as f64 * 48.0, 700.0, 48.0], [716.0 + i as f64 * 48.0, 704.0, 64.0]);
                tie(ed, &[k], "func_button", &[("spawnflags", "1025"), ("wait", "10")]);
            }
        }
        _ => {}
    }
    ed.map.gamemode = id.to_string();
}

// ---------------------------------------------------------------- Checkliste (Fenster)
use crate::app::App;
use crate::i18n::tr;
use eframe::egui::{self, RichText};

impl App {
    /// Stelle für neue Entities: Boden dort, wohin die 3D-Kamera schaut
    fn gm_spot(&self, i: usize) -> DVec3 {
        let o = self.view3d.cam.pos.as_dvec3();
        let d = self.view3d.cam.forward().as_dvec3();
        let hit = match self.ed.pick_3d(o, d) {
            Some((_, _, t)) => o + d * (t - 24.0).max(0.0),
            None => o + d * 256.0,
        };
        // mehrere nebeneinander, dann auf den Boden darunter
        let p = hit + DVec3::new((i % 4) as f64 * 64.0, (i / 4) as f64 * 64.0, 0.0);
        let g = self.ed.snap_grid();
        let floor = self.ed.pick_3d(p + DVec3::Z * 8.0, -DVec3::Z).map(|(_, _, t)| p.z + 8.0 - t).unwrap_or(p.z);
        DVec3::new(crate::geometry::snap(p.x, g), crate::geometry::snap(p.y, g), floor.round())
    }

    pub(crate) fn gamemode_window(&mut self, ctx: &egui::Context) {
        if !self.win.gamemode {
            return;
        }
        let th = crate::theme::t();
        let mut open = true;
        let mut pick: Option<&'static str> = None;
        let mut add: Option<(&'static str, usize)> = None;
        let cur = find(&self.ed.map.gamemode);
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(tr("Gamemode-Checkliste"), 470.0).height(560.0), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Gamemode"));
                egui::ComboBox::from_id_salt("gm_pick").selected_text(cur.map(|g| g.name).unwrap_or(tr("(keiner)"))).width(260.0).show_ui(ui, |ui| {
                    for g in GAMEMODES {
                        if ui.selectable_label(cur.is_some_and(|c| c.id == g.id), g.name).clicked() {
                            pick = Some(g.id);
                        }
                    }
                });
            });
            let Some(gm) = cur else {
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Gamemode wählen: die Liste zeigt, welche Entities die Map dafür braucht. Fertige Vorlagen gibt es unter Datei → Neu aus Vorlage.")).color(th.text3));
                return;
            };
            ui.label(RichText::new(crate::i18n::tr_dyn(gm.description)).small().color(th.text3));
            ui.add_space(8.0);
            let green = egui::Color32::from_rgb(90, 200, 120);
            let red = egui::Color32::from_rgb(235, 95, 95);
            let yellow = egui::Color32::from_rgb(235, 190, 70);
            for (n, c, ok) in check(&self.ed, gm) {
                ui.horizontal(|ui| {
                    let (sym, col) = if ok { ("✔", green) } else if n.optional { ("•", yellow) } else { ("✖", red) };
                    ui.label(RichText::new(sym).color(col).strong());
                    ui.label(RichText::new(crate::i18n::tr_dyn(n.label)).color(th.text));
                    ui.label(RichText::new(format!("{c} / {}", n.min)).font(crate::theme::mono(11.5)).color(if ok { th.text2 } else { col }));
                    if n.optional && !ok {
                        ui.label(RichText::new(tr("empfohlen")).small().color(th.text3));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !n.add.is_empty() && !ok && ui.small_button(tr("Hinzufügen")).on_hover_text(crate::i18n::tr_dyn(&format!("{} × {} dort, wohin die 3D-Kamera schaut", n.min - c, n.add))).clicked() {
                            add = Some((n.add, n.min - c));
                        }
                    });
                });
                ui.label(RichText::new(crate::i18n::tr_dyn(n.hint)).small().color(th.text3));
                ui.add_space(4.0);
            }
            ui.separator();
            match prefix_ok(&self.ed, gm) {
                Some(true) => ui.label(RichText::new(tr("✔ Map-Name passt zum Gamemode")).color(green)),
                Some(false) => ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Map-Name sollte mit {} beginnen", gm.prefixes.join(" / ")))).color(yellow)),
                None => ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Beim Speichern: Name mit {} beginnen", gm.prefixes.join(" / ")))).color(th.text3)),
            };
            ui.add_space(6.0);
            crate::theme::section(ui, tr("Tipps"));
            for t in gm.tips {
                ui.label(RichText::new(format!("• {}", crate::i18n::tr_dyn(t))).color(th.text2));
            }
        });
        if let Some(id) = pick {
            self.ed.map.gamemode = id.to_string();
            self.ed.dirty = true;
        }
        if let Some((class, n)) = add {
            let spots: Vec<DVec3> = (0..n).map(|i| self.gm_spot(i)).collect();
            self.ed.checkpoint();
            let keep = self.ed.entity_class.clone();
            self.ed.entity_class = class.to_string();
            let hold = self.ed.checkpoint_hold;
            // ein gemeinsamer Rückgängig-Schritt
            self.ed.checkpoint_hold = 2;
            let mut ids = Vec::new();
            for (i, p) in spots.into_iter().enumerate() {
                let id = self.ed.create_point_entity(p);
                // Props ohne gewähltes Modell: abwechselnd Alltagsgegenstände
                if let Some(e) = self.ed.map.entity_mut(id) {
                    if e.classname().starts_with("prop_") && e.get("model").is_none_or(|m| m.is_empty()) {
                        e.set("model", PROP_MODELS[i % PROP_MODELS.len()]);
                    }
                }
                ids.push(id);
            }
            self.ed.checkpoint_hold = hold;
            self.ed.entity_class = keep;
            self.ed.sel = ids.into_iter().map(crate::editor::Obj::Entity).collect();
            self.ed.status = crate::i18n::tr_dyn(&format!("{n} × {class} hinzugefügt – verteilen und ausrichten"));
            self.ed.touch();
        }
        self.win.gamemode = open;
    }
}

/// Bounds der Halle (für Tests)
#[allow(dead_code)]
pub const HALL: Aabb = Aabb { min: DVec3::new(-1024.0, -1024.0, -16.0), max: DVec3::new(1024.0, 1024.0, 384.0) };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_pass_their_checklist() {
        for gm in GAMEMODES {
            let mut ed = Editor::new(crate::fgd::Fgd::builtin());
            build_template(&mut ed, gm.id);
            for (n, c, ok) in check(&ed, gm) {
                assert!(ok, "{}: {} hat {c}, braucht {}", gm.id, n.label, n.min);
            }
            assert_eq!(ed.map.gamemode, gm.id);
            assert!(ed.map.entities.iter().all(|e| ed.fgd.class(e.classname()).is_some()), "{}: unbekannte Klasse", gm.id);
        }
    }

    #[test]
    fn matching_and_prefix() {
        assert!(class_matches("weapon_zm_*", "weapon_zm_shotgun"));
        assert!(class_matches("item_*_ttt", "item_ammo_pistol_ttt"));
        assert!(!class_matches("weapon_zm_*", "weapon_ttt_m16"));
        assert!(class_matches("func_button", "FUNC_BUTTON"));
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let gm = find("ttt").unwrap();
        assert_eq!(prefix_ok(&ed, gm), None);
        ed.path = Some("maps/ttt_lager.vmf".into());
        assert_eq!(prefix_ok(&ed, gm), Some(true));
        ed.path = Some("maps/lager.vmf".into());
        assert_eq!(prefix_ok(&ed, gm), Some(false));
        // leere Map: nichts erfüllt
        assert!(check(&ed, gm).iter().filter(|c| !c.0.optional).all(|c| !c.2));
    }
}
