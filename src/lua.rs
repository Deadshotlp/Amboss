//! Lua-Skripte (Lua 5.4, eingebaut). Ein Skript kann alle Werkzeuge der KI-Fernsteuerung (MCP)
//! aufrufen – `amboss.call("create_brush", {...})` – und hat ein paar Abkürzungen dafür.
//! Das ganze Skript ist ein einziger Rückgängig-Schritt. Endlosschleifen brechen nach einem Zeitlimit ab.

use crate::mcp::Host;
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Abkürzungen, in Lua geschrieben (alles läuft über amboss.call)
const PRELUDE: &str = r#"
function amboss.brush(min, max, material)
  return amboss.call("create_brush", {min = min, max = max, material = material}).ids[1]
end
function amboss.room(min, max, material, wall)
  return amboss.call("create_room", {min = min, max = max, material = material, wall = wall}).ids
end
function amboss.entity(classname, origin, keys, angles)
  return amboss.call("create_entity", {classname = classname, origin = origin, keys = keys, angles = angles}).id
end
function amboss.set(id, keys)
  return amboss.call("set_entity_keys", {id = id, keys = keys})
end
function amboss.get(id)
  return amboss.call("get_entity", {id = id})
end
function amboss.output(id, output, target, input, parameter, delay)
  return amboss.call("add_output", {id = id, output = output, target = target, input = input, parameter = parameter, delay = delay})
end
function amboss.move(ids, offset)
  return amboss.call("move_objects", {ids = ids, offset = offset})
end
function amboss.delete(ids)
  return amboss.call("delete_objects", {ids = ids})
end
function amboss.select(ids)
  if #ids == 0 then return amboss.call("select", {ids = {0}}) end
  return amboss.call("select", {ids = ids})
end
function amboss.selection()
  return amboss.call("get_selection", {}).ids or {}
end
function amboss.entities(classname)
  local r = amboss.call("list_entities", {classname = classname, limit = 100000})
  return r.entities or {}
end
function amboss.info()
  return amboss.call("get_map_info", {})
end
"#;

/// Ergebnis eines Skriptlaufs
#[derive(Debug, Default, Clone)]
pub struct RunResult {
    pub output: Vec<String>,
    pub error: Option<String>,
    pub calls: usize,
    pub millis: u128,
}

fn fmt_value(v: &LuaValue) -> String {
    match v {
        LuaValue::Nil => "nil".into(),
        LuaValue::Boolean(b) => b.to_string(),
        LuaValue::Integer(i) => i.to_string(),
        LuaValue::Number(n) => crate::map::fmt_num(*n),
        LuaValue::String(s) => s.to_string_lossy().to_string(),
        LuaValue::Table(t) => {
            // kleine Tabellen lesbar ausgeben
            let mut parts = Vec::new();
            for pair in t.clone().pairs::<LuaValue, LuaValue>().take(32) {
                if let Ok((k, v)) = pair {
                    let vs = if matches!(v, LuaValue::Table(_)) { "{…}".to_string() } else { fmt_value(&v) };
                    match k {
                        LuaValue::Integer(_) => parts.push(vs),
                        _ => parts.push(format!("{}={vs}", fmt_value(&k))),
                    }
                }
            }
            format!("{{{}}}", parts.join(", "))
        }
        other => format!("<{}>", other.type_name()),
    }
}

/// Skript ausführen. `limit`: maximale Laufzeit.
pub fn run(src: &str, host: &mut dyn Host, limit: Duration) -> RunResult {
    let start = Instant::now();
    let mut res = RunResult::default();
    let lua = Lua::new();
    let out = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    // Zeitlimit gegen Endlosschleifen
    let deadline = start + limit;
    lua.set_hook(mlua::HookTriggers::new().every_nth_instruction(10_000), move |_, _| {
        if Instant::now() > deadline {
            Err(mlua::Error::runtime(format!("Zeitlimit von {} s überschritten – Skript abgebrochen", limit.as_secs())))
        } else {
            Ok(mlua::VmState::Continue)
        }
    });
    host.ed().checkpoint_hold = 1;
    let h: &mut dyn Host = &mut *host;
    let lua_r = &lua;
    let (out_c, calls_c) = (out.clone(), calls.clone());
    let r: mlua::Result<()> = lua.scope(move |scope| {
        let lua = lua_r;
        let g = lua.globals();
        let o2 = out_c;
        g.set(
            "print",
            scope.create_function(move |_, args: mlua::Variadic<LuaValue>| {
                o2.borrow_mut().push(args.iter().map(fmt_value).collect::<Vec<_>>().join("\t"));
                Ok(())
            })?,
        )?;
        let am = lua.create_table()?;
        let c2 = calls_c;
        let call = scope.create_function_mut(move |lua, (name, args): (String, Option<LuaValue>)| {
            c2.set(c2.get() + 1);
            if name == "run_lua" {
                return Err(mlua::Error::runtime("run_lua kann nicht aus einem Skript aufgerufen werden"));
            }
            let json: Value = match args {
                Some(LuaValue::Nil) | None => Value::Object(Default::default()),
                Some(v) => lua.from_value(v)?,
            };
            match crate::mcp::call_tool(&name, &json, &mut *h) {
                Ok(v) => lua.to_value(&v),
                Err(e) => Err(mlua::Error::runtime(format!("{name}: {e}"))),
            }
        })?;
        am.set("call", call)?;
        let tools: Vec<String> = crate::mcp::tool_list().iter().filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from)).collect();
        am.set("tools", tools)?;
        am.set("version", env!("CARGO_PKG_VERSION"))?;
        g.set("amboss", am)?;
        lua.load(PRELUDE).set_name("amboss").exec()?;
        lua.load(src).set_name("Skript").exec()
    });
    host.ed().checkpoint_hold = 0;
    if let Err(e) = r {
        // Lua-Fehler kürzen: nur die erste Zeile mit Ort, ohne Rust-Stacktrace
        let msg = e.to_string();
        let first = msg.lines().find(|l| !l.trim().is_empty()).unwrap_or("Fehler").trim().to_string();
        res.error = Some(first.trim_start_matches("runtime error: ").to_string());
    }
    let ed = host.ed();
    ed.status = match &res.error {
        None => format!("Skript ausgeführt ({} Aufrufe) – Rückgängig nimmt alles auf einmal zurück", calls.get()),
        Some(e) => format!("Skript-Fehler: {e}"),
    };
    ed.touch();
    res.output = out.borrow().clone();
    res.calls = calls.get();
    res.millis = start.elapsed().as_millis();
    res
}

/// Beispielskripte, die beim ersten Öffnen in den Skriptordner gelegt werden
const EXAMPLES: &[(&str, &str)] = &[
    ("treppe.lua", r#"-- Treppe aus 12 Stufen (18 hoch, 16 tief) vor dem Ursprung
local hoehe, tiefe, breite = 18, 16, 64
for i = 0, 11 do
  amboss.brush({i * tiefe, 0, 0}, {(i + 1) * tiefe, breite, (i + 1) * hoehe}, "DEV/DEV_MEASUREGENERIC01")
end
print("Treppe gebaut")
"#),
    ("lichter_raster.lua", r#"-- Lichter im Raster über der Auswahl (oder 1024×1024 um den Ursprung)
local abstand, z = 256, 160
local n = 0
for x = -512, 512, abstand do
  for y = -512, 512, abstand do
    amboss.entity("light", {x, y, z}, {_light = "255 236 200 200"})
    n = n + 1
  end
end
print(n .. " Lichter gesetzt")
"#),
    ("entities_zaehlen.lua", r#"-- Alle Entities nach Klasse zählen
local zahl = {}
for _, e in ipairs(amboss.entities()) do
  zahl[e.classname] = (zahl[e.classname] or 0) + 1
end
for k, v in pairs(zahl) do print(k, v) end
"#),
    ("tueren_benennen.lua", r#"-- Unbenannte Türen durchnummerieren: tuer_1, tuer_2,…
local n = 0
for _, e in ipairs(amboss.entities()) do
  if (e.classname == "func_door" or e.classname == "func_door_rotating" or e.classname == "prop_door_rotating") and not e.targetname then
    n = n + 1
    amboss.set(e.id, {targetname = "tuer_" .. n})
  end
end
print(n .. " Türen benannt")
"#),
];

/// Englische Fassung der Beispiele (bei englischer Oberfläche)
const EXAMPLES_EN: &[(&str, &str)] = &[
    ("stairs.lua", r#"-- Stairs with 12 steps (18 high, 16 deep) in front of the origin
local height, depth, width = 18, 16, 64
for i = 0, 11 do
  amboss.brush({i * depth, 0, 0}, {(i + 1) * depth, width, (i + 1) * height}, "DEV/DEV_MEASUREGENERIC01")
end
print("Stairs built")
"#),
    ("light_grid.lua", r#"-- Lights in a grid (1024×1024 around the origin)
local spacing, z = 256, 160
local n = 0
for x = -512, 512, spacing do
  for y = -512, 512, spacing do
    amboss.entity("light", {x, y, z}, {_light = "255 236 200 200"})
    n = n + 1
  end
end
print(n .. " lights placed")
"#),
    ("count_entities.lua", r#"-- Count all entities by class
local count = {}
for _, e in ipairs(amboss.entities()) do
  count[e.classname] = (count[e.classname] or 0) + 1
end
for k, v in pairs(count) do print(k, v) end
"#),
    ("name_doors.lua", r#"-- Number unnamed doors: door_1, door_2,…
local n = 0
for _, e in ipairs(amboss.entities()) do
  if (e.classname == "func_door" or e.classname == "func_door_rotating" or e.classname == "prop_door_rotating") and not e.targetname then
    n = n + 1
    amboss.set(e.id, {targetname = "door_" .. n})
  end
end
print(n .. " doors named")
"#),
];

/// Zustand des Skriptfensters
#[derive(Default)]
pub struct ScriptDlg {
    pub open: bool,
    pub code: String,
    pub file: Option<std::path::PathBuf>,
    pub name: String,
    pub files: Vec<std::path::PathBuf>,
    pub output: Vec<(String, bool)>,
    scanned: bool,
}

impl crate::app::App {
    pub(crate) fn scripts_dir(&self) -> std::path::PathBuf {
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("skripte"))).unwrap_or_else(|| "skripte".into())
    }

    fn scan_scripts(&mut self) {
        let dir = self.scripts_dir();
        if !dir.is_dir() && std::fs::create_dir_all(&dir).is_ok() {
            let ex = if crate::i18n::lang() == crate::i18n::Lang::En { EXAMPLES_EN } else { EXAMPLES };
            for (n, c) in ex {
                let _ = std::fs::write(dir.join(n), c);
            }
        }
        let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lua"))).collect()).unwrap_or_default();
        v.sort();
        self.scripts.files = v;
        self.scripts.scanned = true;
        if self.scripts.code.is_empty() {
            self.scripts.code = if crate::i18n::lang() == crate::i18n::Lang::En {
                "-- Lua script: every tool via amboss.call(name, {…}), see the help on the right\nprint(amboss.info().brushes .. \" brushes in the map\")\n".into()
            } else {
                "-- Lua-Skript: alle Werkzeuge über amboss.call(name, {…}), siehe Hilfe rechts\nprint(amboss.info().brushes .. \" Brushes in der Map\")\n".into()
            };
        }
    }

    fn run_script(&mut self) {
        let code = self.scripts.code.clone();
        let r = run(&code, self, Duration::from_secs(10));
        let mut out: Vec<(String, bool)> = r.output.into_iter().map(|l| (l, false)).collect();
        match r.error {
            Some(e) => out.push((e, true)),
            None => out.push((crate::i18n::tr_dyn(&format!("✓ fertig in {} ms, {}", r.millis, crate::i18n::count(r.calls, "Aufruf", "Aufrufe"))), false)),
        }
        self.scripts.output = out;
    }

    pub(crate) fn scripts_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme;
        use eframe::egui::{self, RichText, Vec2};
        if !self.scripts.open {
            return;
        }
        if !self.scripts.scanned {
            self.scan_scripts();
        }
        let th = theme::t();
        let mut open = true;
        let (mut run_now, mut save, mut load, mut newf, mut rescan) = (false, false, None, false, false);
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
            run_now = true;
        }
        let dir = self.scripts_dir();
        let sub = self.scripts.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| crate::i18n::tr("ungespeichert").into());
        let d = theme::Dlg::new(crate::i18n::tr("Lua-Skripte"), 1120.0).subtitle(sub).footer().no_scroll();
        let st = &mut self.scripts;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 10 }).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        // Skriptliste
                        ui.vertical(|ui| {
                            ui.set_width(180.0);
                            ui.label(RichText::new(crate::i18n::tr("Skripte")).strong());
                            egui::ScrollArea::vertical().id_salt("scrlist").max_height(330.0).show(ui, |ui| {
                                for f in &st.files {
                                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    if ui.selectable_label(st.file.as_ref() == Some(f), name).clicked() {
                                        load = Some(f.clone());
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button(crate::i18n::tr("Neu")).clicked() {
                                    newf = true;
                                }
                                if ui.button("⟳").on_hover_text(crate::i18n::tr("Ordner neu einlesen")).clicked() {
                                    rescan = true;
                                }
                            });
                            ui.add(egui::Label::new(RichText::new(theme::short_path(&dir.display().to_string(), 28)).small().weak())).on_hover_text(dir.display().to_string());
                        });
                        // Editor + Ausgabe
                        ui.vertical(|ui| {
                            ui.set_width(520.0);
                            egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(6).show(ui, |ui| {
                                egui::ScrollArea::vertical().id_salt("scrcode").max_height(300.0).show(ui, |ui| {
                                    ui.add(egui::TextEdit::multiline(&mut st.code).code_editor().desired_rows(18).desired_width(f32::INFINITY).lock_focus(true));
                                });
                            });
                            ui.add_space(6.0);
                            egui::Frame::new().fill(th.vp).corner_radius(6).inner_margin(6).show(ui, |ui| {
                                ui.set_min_height(90.0);
                                egui::ScrollArea::vertical().id_salt("scrout").max_height(110.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                                    if st.output.is_empty() {
                                        ui.label(RichText::new(crate::i18n::tr("Ausgabe von print() erscheint hier.")).color(th.text3).size(11.5));
                                    }
                                    for (l, err) in &st.output {
                                        ui.label(RichText::new(l).font(theme::mono(11.0)).color(if *err { th.err } else { th.text2 }));
                                    }
                                });
                            });
                        });
                        // Kurzhilfe: feste Restbreite, lange Zeilen brechen um
                        ui.vertical(|ui| {
                            let w = ui.available_width().max(160.0);
                            ui.set_width(w);
                            ui.label(RichText::new(crate::i18n::tr("Befehle")).strong());
                            egui::ScrollArea::vertical().id_salt("scrhelp").max_height(400.0).show(ui, |ui| {
                                for l in [
                                    "amboss.brush(min, max, mat) → id",
                                    "amboss.room(min, max, mat, wand) → ids",
                                    "amboss.entity(klasse, pos, keys, winkel) → id",
                                    "amboss.set(id, keys)",
                                    "amboss.get(id) → Tabelle",
                                    "amboss.output(id, out, ziel, input, param, verz.)",
                                    "amboss.move(ids, versatz)",
                                    "amboss.delete(ids)",
                                    "amboss.select(ids) / amboss.selection()",
                                    "amboss.entities([klasse])",
                                    "amboss.info()",
                                    "amboss.call(name, {…})",
                                ] {
                                    ui.add(egui::Label::new(RichText::new(crate::i18n::tr_dyn(l)).font(theme::mono(10.5)).color(th.text2)).wrap());
                                }
                                ui.add_space(6.0);
                                ui.add(egui::Label::new(RichText::new(crate::i18n::tr("Alle Werkzeuge für amboss.call:")).small().strong()).wrap());
                                for t in crate::mcp::tool_list() {
                                    let (Some(n), desc) = (t.get("name").and_then(|v| v.as_str()), t.get("description").and_then(|v| v.as_str()).unwrap_or("")) else { continue };
                                    if ui.add(egui::Label::new(RichText::new(n).font(theme::mono(10.5)).color(th.accent)).sense(egui::Sense::click())).on_hover_text(desc).clicked() {
                                        st.code.push_str(&format!("\namboss.call(\"{n}\", {{}})"));
                                    }
                                }
                            });
                        });
                    });
                });
            }
            theme::Part::Footer => {
                if ui.add(egui::Button::new(RichText::new(crate::i18n::tr("Ausführen")).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).on_hover_text(crate::i18n::keys("Strg+Enter")).clicked() {
                    run_now = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Speichern")).clicked() {
                    save = true;
                }
                ui.add(egui::TextEdit::singleline(&mut st.name).hint_text(crate::theme::hint(crate::i18n::tr("dateiname"))).desired_width(140.0));
                theme::footer_hint(ui, crate::i18n::tr("Ein Rückgängig-Schritt pro Skript · Zeitlimit 10 s"));
            }
            _ => {}
        });
        if rescan {
            self.scan_scripts();
        }
        if newf {
            self.scripts.file = None;
            self.scripts.name.clear();
            self.scripts.code = format!("-- {}\n", crate::i18n::tr("neues Skript"));
            self.scripts.output.clear();
        }
        if let Some(f) = load {
            match std::fs::read_to_string(&f) {
                Ok(c) => {
                    self.scripts.code = c;
                    self.scripts.name = f.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.scripts.file = Some(f);
                    self.scripts.output.clear();
                }
                Err(e) => self.scripts.output = vec![(e.to_string(), true)],
            }
        }
        if save {
            let name = self.scripts.name.trim().trim_end_matches(".lua").replace(['/', '\\'], "_");
            let path = if name.is_empty() { self.scripts.file.clone() } else { Some(dir.join(format!("{name}.lua"))) };
            match path {
                Some(p) => match std::fs::write(&p, &self.scripts.code) {
                    Ok(()) => {
                        self.scripts.output = vec![(format!("Gespeichert: {}", p.display()), false)];
                        self.scripts.file = Some(p);
                        self.scan_scripts();
                    }
                    Err(e) => self.scripts.output = vec![(e.to_string(), true)],
                },
                None => self.scripts.output = vec![(crate::i18n::tr("Bitte unten einen Dateinamen eingeben").into(), true)],
            }
        }
        if run_now {
            self.run_script();
        }
        self.scripts.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_builds_and_undoes_in_one_step() {
        let mut h = crate::mcp::Headless::for_tests();
        let src = r#"
            local ids = {}
            for i = 0, 4 do
              ids[#ids + 1] = amboss.brush({i * 64, 0, 0}, {i * 64 + 32, 32, 16 * (i + 1)}, "DEV/DEV_MEASUREGENERIC01")
            end
            local l = amboss.entity("light", {0, 0, 128}, {_light = "255 200 150 300"})
            print("Treppe:", #ids, "Stufen, Licht", l > 0)
            print(amboss.info().brushes)
        "#;
        let r = run(src, &mut h, Duration::from_secs(5));
        assert!(r.error.is_none(), "{:?}", r.error);
        assert_eq!(r.output[0], "Treppe:\t5\tStufen, Licht\ttrue");
        assert_eq!(r.output[1], "5");
        assert_eq!(h.ed().map.world.solids.len(), 5);
        // ein einziger Rückgängig-Schritt
        h.ed().undo();
        assert_eq!(h.ed().map.world.solids.len(), 0);
        assert!(h.ed().map.entities.is_empty());
    }

    #[test]
    fn errors_and_endless_loops_are_reported() {
        let mut h = crate::mcp::Headless::for_tests();
        let r = run("amboss.call('gibt_es_nicht', {})", &mut h, Duration::from_secs(5));
        assert!(r.error.as_deref().unwrap_or("").contains("gibt_es_nicht"), "{:?}", r.error);
        let r = run("while true do end", &mut h, Duration::from_millis(200));
        assert!(r.error.as_deref().unwrap_or("").contains("Zeitlimit"), "{:?}", r.error);
        let r = run("x = = 1", &mut h, Duration::from_secs(1));
        assert!(r.error.is_some());
        assert_eq!(h.ed().checkpoint_hold, 0);
        // nur lesendes Skript: kein Rückgängig-Schritt
        assert!(!h.ed().can_undo());
    }
}
