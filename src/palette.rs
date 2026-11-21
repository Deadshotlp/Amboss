//! Befehlspalette (Strg+K / Strg+P) nach UI-Konzept: Befehle, Entities in der Map, Texturen.

use crate::app::App;
use crate::commands::{display_shortcut, Cmd};
use crate::editor::Obj;
use crate::theme::{alpha, mono, sans, t};
use eframe::egui::{self, Color32, FontId, Key, Rect, Sense, Stroke, StrokeKind, Vec2};

#[derive(Clone)]
enum Item {
    Cmd(Cmd, String, &'static str, String),
    Ent(u32, String, String, Option<Color32>),
    Tex(String, usize),
    More(usize, usize),
}

/// Kategorie eines Befehls (für die Zusatzspalte)
fn category(c: Cmd) -> &'static str {
    use Cmd::*;
    match c {
        New | Open | Save | SaveAs | NewTab | CloseTab | NextTab | MergeMap | ImportMap | ImportOsm | ImportModel | ExportObj | ExportGltf | Screenshot | Timeline | Prefabs | Settings => crate::i18n::tr("Datei"),
        Undo | Redo | History | PasteSpecial | Delete | SelectAll | Duplicate | Carve | Hollow | TieToEntity | MoveToWorld | Group | Ungroup | Transform | SnapToGrid | FlipH | FlipV | RotateMouse | RotateCw | RotateCcw | SelectTouching | CopyProps | PasteProps | Lock | UnlockAll => crate::i18n::tr("Bearbeiten"),
        Hide | HideUnselected | UnhideAll | Outliner | LogicalView | GridDown | GridUp | CenterSel2D | CenterSel3D | NextCamera | Maximize | ToggleGizmo | ToggleSnap | ToggleVertexSnap | Palette | ToggleNodraw | ToggleTriggers | ToggleToolBrushes | SkyboxMode | ToggleGameplay => crate::i18n::tr("Ansicht"),
        Spline | UnifyTextures | DecalFromImage | TextureLibrary | ApplyTexture | ReplaceTextures | NodrawHidden | TextureBrowser | ModelBrowser | SoundBrowser | ParticleBrowser => crate::i18n::tr("Texturen"),
        CompareMap | Merge3 | CheckProblems | Optimize | OptimizeCheck | EntityReport | GotoNumber | MapInfo | LoadPointfile | ClearMeasure | SkyboxCopy | SkyboxCheck | AddNote | Notes | GamemodeChecklist => "Map",
        Compile | CompileWindow | TestFromCamera | Walk | Workshop => crate::i18n::tr("Kompilieren"),
        Collab => crate::i18n::tr("Live-Sitzung"),
        Help => crate::i18n::tr("Hilfe"),
        Tool(_) => crate::i18n::tr("Werkzeug"),
    }
}

impl App {
    pub(crate) fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.win.palette {
            return;
        }
        let th = t();
        let raw = self.dlg.palette_filter.clone();
        let (only, query) = match raw.chars().next() {
            Some('@') => (Some(1), raw[1..].to_string()),
            Some('#') => (Some(2), raw[1..].to_string()),
            Some('>') => (Some(0), raw[1..].to_string()),
            _ => (None, raw.clone()),
        };
        let words: Vec<String> = query.to_ascii_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        let hit = |s: &str| words.iter().all(|w| s.to_ascii_lowercase().contains(w.as_str()));
        let expand = self.dlg.palette_expand;
        let limit = |g: usize, n: usize| if expand == Some(g) || only == Some(g) { n } else { 5.min(n) };
        // Gruppen sammeln
        let mut groups: Vec<(&str, Vec<Item>, usize)> = Vec::new();
        if only.is_none_or(|o| o == 0) {
            let all: Vec<Item> = Cmd::all().into_iter().map(|c| (c, c.label())).filter(|(c, l)| hit(l) || hit(&c.id())).map(|(c, l)| Item::Cmd(c, l, category(c), display_shortcut(&self.key_for(c)))).collect();
            let n = all.len();
            groups.push((crate::i18n::tr("Befehle"), all.into_iter().take(limit(0, n)).collect(), n));
        }
        if only.is_none_or(|o| o == 1) && (!words.is_empty() || only == Some(1)) {
            let all: Vec<Item> = self
                .ed
                .map
                .entities
                .iter()
                .filter(|e| hit(e.classname()) || e.get("targetname").is_some_and(hit))
                .map(|e| {
                    let name = e.get("targetname").filter(|n| !n.is_empty()).map(|n| n.to_string()).unwrap_or_else(|| format!("{} #{}", e.classname(), e.id));
                    let col = self.ed.fgd.resolve_color(e.classname()).map(|c| Color32::from_rgb(c[0], c[1], c[2]));
                    Item::Ent(e.id, name, e.classname().to_string(), col)
                })
                .collect();
            let n = all.len();
            groups.push((crate::i18n::tr("Entities in der Map"), all.into_iter().take(limit(1, n)).collect(), n));
        }
        if only.is_none_or(|o| o == 2) && (!words.is_empty() || only == Some(2)) {
            if self.dlg.palette_face_counts.is_none() {
                let mut m = std::collections::HashMap::new();
                for (_, s) in self.ed.map.all_solids() {
                    for side in &s.sides {
                        *m.entry(crate::materials::normalize(&side.material)).or_insert(0usize) += 1;
                    }
                }
                self.dlg.palette_face_counts = Some(m);
            }
            let counts = self.dlg.palette_face_counts.as_ref().unwrap();
            let mut all: Vec<Item> = self.mats.list.iter().filter(|m| hit(m)).take(2000).map(|m| Item::Tex(m.clone(), counts.get(m).copied().unwrap_or(0))).collect();
            // in der Map benutzte zuerst
            all.sort_by_key(|i| if let Item::Tex(_, n) = i { std::cmp::Reverse(*n) } else { std::cmp::Reverse(0) });
            let n = all.len();
            groups.push((crate::i18n::tr("Texturen"), all.into_iter().take(limit(2, n)).collect(), n));
        }
        // „Alle N anzeigen“
        for (gi, g) in groups.iter_mut().enumerate() {
            if g.1.len() < g.2 {
                let n = g.2;
                g.1.push(Item::More(gi_index(gi, &only), n));
            }
        }
        let flat: Vec<(usize, Item)> = groups.iter().enumerate().flat_map(|(gi, g)| g.1.iter().cloned().map(move |i| (gi, i))).collect();
        // Tastatur
        let (up, down, enter, esc, tab) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
                i.consume_key(egui::Modifiers::NONE, Key::Tab),
            )
        });
        let n = flat.len();
        let mut idx = self.dlg.palette_index.min(n.saturating_sub(1));
        let mut scroll = false;
        if down && idx + 1 < n {
            idx += 1;
            scroll = true;
        }
        if up && idx > 0 {
            idx -= 1;
            scroll = true;
        }
        if tab && n > 0 {
            let g = flat[idx].0;
            idx = flat.iter().position(|(gi, _)| *gi > g).unwrap_or(0);
            scroll = true;
        }
        let mut chosen: Option<Item> = None;
        let screen = ctx.screen_rect();
        let w = 620.0f32.min(screen.width() - 32.0);
        let top = screen.top() + screen.height() * 0.15;
        egui::Area::new(egui::Id::new("palette2")).order(egui::Order::Foreground).fixed_pos(egui::pos2(screen.center().x - w / 2.0, top)).show(ctx, |ui| {
            egui::Frame::new().fill(th.panel).stroke(Stroke::new(1.0f32, th.line_strong)).corner_radius(14).shadow(egui::epaint::Shadow { offset: [0, 24], blur: 64, spread: 0, color: th.shadow }).show(ui, |ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing.y = 0.0;
                // Suchfeld
                let (sr, _) = ui.allocate_exact_size(Vec2::new(w, 56.0), Sense::hover());
                crate::icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(sr.left() + 26.0, sr.center().y), Vec2::splat(18.0)), "search", th.text3, 1.6);
                let er = Rect::from_min_max(egui::pos2(sr.left() + 48.0, sr.top() + 10.0), egui::pos2(sr.right() - 60.0, sr.bottom() - 10.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(er).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let te = child.add(egui::TextEdit::singleline(&mut self.dlg.palette_filter).frame(false).font(sans(16.0)).hint_text(crate::theme::hint(crate::i18n::tr("Suchen…   @ Entities · # Texturen · > Befehle"))).desired_width(er.width()));
                te.request_focus();
                if te.changed() {
                    idx = 0;
                    self.dlg.palette_expand = None;
                }
                keycap(ui, egui::pos2(sr.right() - 20.0, sr.center().y), "Esc", true);
                ui.painter().hline(sr.x_range(), sr.bottom(), Stroke::new(1.0f32, th.line));
                // Ergebnisse
                egui::ScrollArea::vertical().max_height(480.0 - 56.0 - 38.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 2, bottom: 6 }).show(ui, |ui| {
                        let rw = ui.available_width();
                        if flat.is_empty() {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(rw, 80.0), Sense::hover());
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Nichts gefunden"), sans(13.0), th.text3);
                        }
                        let mut k = 0usize;
                        for (gi, g) in groups.iter().enumerate() {
                            if g.1.is_empty() {
                                continue;
                            }
                            let (hr, _) = ui.allocate_exact_size(Vec2::new(rw, 30.0), Sense::hover());
                            ui.painter().text(egui::pos2(hr.left() + 12.0, hr.bottom() - 6.0), egui::Align2::LEFT_BOTTOM, g.0.to_uppercase(), FontId::new(11.0, crate::theme::semibold()), th.text3);
                            ui.painter().text(egui::pos2(hr.right() - 12.0, hr.bottom() - 6.0), egui::Align2::RIGHT_BOTTOM, g.2.to_string(), sans(11.0), th.text3);
                            let _ = gi;
                            for it in &g.1 {
                                let active = k == idx;
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(rw, 36.0), Sense::click());
                                if active && scroll {
                                    ui.scroll_to_rect(r, None);
                                }
                                if active {
                                    ui.painter().rect(r, 8.0, th.accent_soft, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(r, 8.0, th.hover);
                                }
                                let ic = egui::pos2(r.left() + 20.0, r.center().y);
                                let lx = r.left() + 38.0;
                                let (label, label_font, extra, keys): (String, FontId, String, String) = match it {
                                    Item::Cmd(_, l, cat, key) => {
                                        ui.painter().rect_stroke(Rect::from_center_size(ic, Vec2::splat(14.0)), 3.0, Stroke::new(1.2f32, th.text3), StrokeKind::Inside);
                                        (l.clone(), sans(13.5), cat.to_string(), key.clone())
                                    }
                                    Item::Ent(_, name, class, col) => {
                                        ui.painter().circle_filled(ic, 4.5, col.unwrap_or(th.text3));
                                        (name.clone(), mono(12.5), class.clone(), String::new())
                                    }
                                    Item::Tex(name, faces) => {
                                        let pr = Rect::from_center_size(ic, Vec2::splat(16.0));
                                        match self.mats.thumb(name) {
                                            Some(tex) => {
                                                egui::Image::new((tex.id(), pr.size())).corner_radius(3).paint_at(ui, pr);
                                            }
                                            None => {
                                                ui.painter().rect_filled(pr, 3.0, th.raised);
                                            }
                                        }
                                        let size = self.mats.info(name).map(|i| format!("{}×{}", i.width, i.height)).unwrap_or_default();
                                        let extra = match (*faces, size.is_empty()) {
                                            (0, _) => size,
                                            (n, true) => format!("{n} Flächen"),
                                            (n, false) => format!("{size} · {n} Flächen"),
                                        };
                                        (name.clone(), mono(12.5), extra, String::new())
                                    }
                                    Item::More(_, n) => (crate::i18n::tr_dyn(&format!("Alle {n} anzeigen")), sans(12.5), String::new(), String::new()),
                                };
                                let lc = if matches!(it, Item::More(..)) { th.accent_text } else { th.text };
                                let lg = ui.painter().layout_no_wrap(label, label_font, lc);
                                let lw = lg.size().x.min(rw * 0.6);
                                ui.painter().with_clip_rect(Rect::from_min_max(egui::pos2(lx, r.top()), egui::pos2(lx + lw, r.bottom()))).galley(egui::pos2(lx, r.center().y - lg.size().y / 2.0), lg, lc);
                                if !extra.is_empty() {
                                    ui.painter().text(egui::pos2(lx + lw + 10.0, r.center().y), egui::Align2::LEFT_CENTER, extra, sans(12.0), th.text3);
                                }
                                if !keys.is_empty() {
                                    let mut x = r.right() - 12.0;
                                    for part in keys.split('+').rev() {
                                        x -= keycap(ui, egui::pos2(x, r.center().y), part, true) + 4.0;
                                    }
                                }
                                if resp.clicked() {
                                    chosen = Some(it.clone());
                                }
                                k += 1;
                            }
                        }
                    });
                });
                // Fuß
                let (fr, _) = ui.allocate_exact_size(Vec2::new(w, 38.0), Sense::hover());
                ui.painter().rect_filled(fr, egui::CornerRadius { nw: 0, ne: 0, sw: 14, se: 14 }, th.raised);
                ui.painter().hline(fr.x_range(), fr.top(), Stroke::new(1.0f32, th.line));
                let mut x = fr.left() + 14.0;
                for (k, txt) in [("↑ ↓", crate::i18n::tr("wählen")), ("Enter", crate::i18n::tr("ausführen / auswählen")), ("Tab", crate::i18n::tr("nächste Gruppe"))] {
                    let kw = keycap(ui, egui::pos2(x, fr.center().y), k, false);
                    x += kw + 6.0;
                    let g = ui.painter().layout_no_wrap(txt.into(), sans(11.5), th.text3);
                    let gw = g.size().x;
                    ui.painter().galley(egui::pos2(x, fr.center().y - g.size().y / 2.0), g, th.text3);
                    x += gw + 16.0;
                }
            });
        });
        if enter {
            chosen = chosen.or_else(|| flat.get(idx).map(|x| x.1.clone()));
        }
        self.dlg.palette_index = idx;
        // Klick außerhalb schließt
        let outside = ctx.input(|i| i.pointer.any_click()) && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.y < top || p.y > top + 480.0 || (p.x - screen.center().x).abs() > w / 2.0);
        if esc || outside {
            self.win.palette = false;
        }
        match chosen {
            Some(Item::Cmd(c, ..)) => {
                self.win.palette = false;
                self.run_cmd(ctx, c);
            }
            Some(Item::Ent(id, ..)) => {
                self.win.palette = false;
                self.ed.sel = [Obj::Entity(id)].into_iter().collect();
                self.ed.touch();
                self.center_on_selection(true);
            }
            Some(Item::Tex(name, _)) => {
                self.win.palette = false;
                self.ed.material = name.clone();
                self.ed.status = format!("Aktuelle Textur: {name} (Shift+T wendet sie auf die Auswahl an)");
            }
            Some(Item::More(g, _)) => {
                self.dlg.palette_expand = Some(g);
            }
            None => {}
        }
        if !self.win.palette {
            self.dlg.palette_face_counts = None;
            self.dlg.palette_expand = None;
        }
    }
}

/// Gruppen-Nummer (0 Befehle, 1 Entities, 2 Texturen) aus der Position in der Liste
fn gi_index(pos: usize, only: &Option<usize>) -> usize {
    match only {
        Some(o) => *o,
        None => pos,
    }
}

/// Tastenkappe; `right` = Position ist die rechte Kante, sonst die linke. Liefert die Breite.
fn keycap(ui: &egui::Ui, at: egui::Pos2, text: &str, right: bool) -> f32 {
    let th = t();
    let g = ui.painter().layout_no_wrap(text.to_string(), mono(10.5), th.text2);
    let w = (g.size().x + 10.0).max(20.0);
    let r = if right { Rect::from_min_size(egui::pos2(at.x - w, at.y - 10.0), Vec2::new(w, 20.0)) } else { Rect::from_min_size(egui::pos2(at.x, at.y - 10.0), Vec2::new(w, 20.0)) };
    ui.painter().rect(r, 4.0, alpha(th.panel, 1.0), Stroke::new(1.0f32, th.line_strong), StrokeKind::Inside);
    ui.painter().galley(egui::pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, th.text2);
    w
}
