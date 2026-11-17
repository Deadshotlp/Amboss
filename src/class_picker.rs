//! Entity-Klassen-Auswahl nach UI-Konzept: Popover am Klassen-Chip und Dialog „In Entity umwandeln“.

use crate::app::App;
use crate::fgd::{ClassKind, Fgd};
use crate::theme::{self, alpha, mono, sans, semibold, t};
use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::collections::BTreeSet;

#[derive(Clone)]
struct Entry {
    name: String,
    desc: String,
    color: Option<Color32>,
    solid: bool,
}

enum Row {
    Header { label: String, count: usize, group: Option<String>, open: bool },
    Item(usize),
}

/// Zustand einer Klassenliste (Suche, Filter, aktive Zeile, aufgeklappte Gruppen)
#[derive(Default)]
pub struct ClassList {
    pub query: String,
    /// 0 = alle, 1 = Punkt, 2 = Brush
    pub kind: usize,
    active: usize,
    expanded: BTreeSet<String>,
    expanded_init: bool,
    focus: bool,
    scroll_to_active: bool,
    /// nach dem Öffnen: aktive Zeile auf die aktuelle Klasse setzen
    want_current: bool,
}

fn entries(fgd: &Fgd) -> Vec<Entry> {
    let mut v = Vec::new();
    for (kind, solid) in [(ClassKind::Point, false), (ClassKind::Solid, true)] {
        for n in fgd.names(kind) {
            if n.eq_ignore_ascii_case("worldspawn") {
                continue;
            }
            let c = fgd.class(&n);
            v.push(Entry {
                desc: c.map(|c| crate::i18n::tr_dyn(&c.description)).unwrap_or_default(),
                color: fgd.resolve_color(&n).map(|c| Color32::from_rgb(c[0], c[1], c[2])),
                solid,
                name: n,
            });
        }
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn prefix(name: &str) -> String {
    match name.find('_') {
        Some(i) => name[..=i].to_string(),
        None => "sonstige".into(),
    }
}

/// Ähnlichster Klassenname (für „Meintest du …?“)
fn suggest(entries: &[Entry], q: &str) -> Option<String> {
    fn dist(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut cur = vec![i + 1];
            for (j, cb) in b.iter().enumerate() {
                cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
            }
            prev = cur;
        }
        prev[b.len()]
    }
    let q = q.to_ascii_lowercase();
    entries.iter().map(|e| (dist(&q, &e.name), &e.name)).filter(|(d, _)| *d <= q.len().max(3) / 2 + 1).min().map(|(_, n)| n.clone())
}

/// Ergebnis einer Interaktion mit der Liste
pub enum Pick {
    Chosen(String),
    ToggleFav(String),
}

impl ClassList {
    pub fn reset(&mut self, current: &str) {
        self.query.clear();
        self.active = 0;
        self.expanded.clear();
        self.expanded.insert(prefix(current));
        self.expanded_init = true;
        self.focus = true;
        self.scroll_to_active = true;
        self.want_current = true;
    }

    fn rows(&mut self, es: &[Entry], favs: &[String], recents: &[String], only_solid: bool) -> Vec<Row> {
        let kind_ok = |e: &Entry| match self.kind {
            1 => !e.solid,
            2 => e.solid,
            _ => true,
        };
        let mut rows = Vec::new();
        let q = self.query.trim().to_ascii_lowercase();
        if !q.is_empty() {
            let name_hits: Vec<usize> = (0..es.len()).filter(|&i| kind_ok(&es[i]) && es[i].name.contains(&q)).collect();
            let desc_hits: Vec<usize> = (0..es.len()).filter(|&i| kind_ok(&es[i]) && !es[i].name.contains(&q) && es[i].desc.to_ascii_lowercase().contains(&q)).collect();
            for (label, hits) in [(crate::i18n::tr("Im Namen"), name_hits), (crate::i18n::tr("In der Beschreibung"), desc_hits)] {
                if !hits.is_empty() {
                    rows.push(Row::Header { label: label.into(), count: hits.len(), group: None, open: true });
                    rows.extend(hits.into_iter().map(Row::Item));
                }
            }
            return rows;
        }
        let idx = |n: &String| es.iter().position(|e| e.name == *n);
        let fav: Vec<usize> = favs.iter().filter_map(idx).filter(|&i| kind_ok(&es[i])).collect();
        if !fav.is_empty() {
            rows.push(Row::Header { label: crate::i18n::tr("Favoriten").into(), count: fav.len(), group: None, open: true });
            rows.extend(fav.into_iter().map(Row::Item));
        }
        let rec: Vec<usize> = recents.iter().filter_map(idx).filter(|&i| kind_ok(&es[i]) && !favs.contains(&es[i].name)).take(5).collect();
        if !rec.is_empty() {
            rows.push(Row::Header { label: crate::i18n::tr("Zuletzt benutzt").into(), count: rec.len(), group: None, open: true });
            rows.extend(rec.into_iter().map(Row::Item));
        }
        let mut groups: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
        for (i, e) in es.iter().enumerate() {
            if kind_ok(e) && (!only_solid || e.solid || true) {
                groups.entry(prefix(&e.name)).or_default().push(i);
            }
        }
        for (g, items) in groups {
            let open = self.expanded.contains(&g);
            rows.push(Row::Header { label: g.clone(), count: items.len(), group: Some(g), open });
            if open {
                rows.extend(items.into_iter().map(Row::Item));
            }
        }
        rows
    }

    /// Suchfeld + Segment; liefert true, wenn Esc gedrückt wurde
    fn search_ui(&mut self, ui: &mut egui::Ui, with_kind: bool, hits: Option<usize>) {
        let th = t();
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::hover());
        ui.painter().rect(r, 8.0, th.raised, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
        crate::icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), Vec2::splat(14.0)), "search", th.text3, 1.5);
        let right = if hits.is_some() { 96.0 } else { 10.0 };
        let er = Rect::from_min_max(egui::pos2(r.left() + 30.0, r.top() + 2.0), egui::pos2(r.right() - right, r.bottom() - 2.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(er).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let te = child.add(egui::TextEdit::singleline(&mut self.query).frame(false).hint_text(crate::theme::hint(crate::i18n::tr("Klasse suchen…"))).font(sans(13.0)).desired_width(er.width()));
        if self.focus {
            te.request_focus();
            self.focus = false;
        }
        if te.changed() {
            self.active = 0;
            self.scroll_to_active = true;
        }
        if let Some(n) = hits {
            let cr = Rect::from_center_size(egui::pos2(r.right() - 17.0, r.center().y), Vec2::splat(22.0));
            let c = ui.interact(cr, ui.id().with("clear_q"), Sense::click());
            if c.hovered() {
                ui.painter().rect_filled(cr, 5.0, th.hover);
            }
            crate::icons::draw(ui.painter(), cr.shrink(6.0), "close", th.text2, 1.4);
            if c.clicked() {
                self.query.clear();
                self.focus = true;
            }
            ui.painter().text(egui::pos2(cr.left() - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, format!("{n} Treffer"), sans(11.5), th.text3);
        }
        if with_kind {
            ui.add_space(8.0);
            let k = small_segment(ui, self.kind, &[crate::i18n::tr("Alle"), crate::i18n::tr("Punkt"), "Brush"]);
            if k != self.kind {
                self.kind = k;
                self.active = 0;
            }
        }
    }

    /// Liste zeichnen und Tastatur behandeln
    #[allow(clippy::too_many_arguments)]
    fn list_ui(&mut self, ui: &mut egui::Ui, es: &[Entry], favs: &[String], recents: &[String], current: &str, row_h: f32, height: f32, solid_only: bool, current_q: &str) -> Option<Pick> {
        let th = t();
        let rows = self.rows(es, favs, recents, solid_only);
        let items: Vec<(usize, usize)> = rows.iter().enumerate().filter_map(|(ri, r)| if let Row::Item(i) = r { Some((ri, *i)) } else { None }).collect();
        let mut out = None;
        if self.want_current {
            self.want_current = false;
            if let Some(k) = items.iter().rposition(|(_, i)| es[*i].name == current) {
                self.active = k;
            }
        }
        // Tastatur
        let (up, down, enter, left, right) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) && self.query.is_empty(),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) && self.query.is_empty(),
            )
        });
        if !items.is_empty() {
            self.active = self.active.min(items.len() - 1);
            if up && self.active > 0 {
                self.active -= 1;
                self.scroll_to_active = true;
            }
            if down && self.active + 1 < items.len() {
                self.active += 1;
                self.scroll_to_active = true;
            }
            let ae = &es[items[self.active].1];
            if enter && (!solid_only || ae.solid) {
                out = Some(Pick::Chosen(ae.name.clone()));
            }
            if left {
                self.expanded.remove(&prefix(&ae.name));
            }
            if right {
                self.expanded.insert(prefix(&ae.name));
            }
        }
        if items.is_empty() && rows.is_empty() {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
            let c = r.center();
            crate::icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(c.x, c.y - 30.0), Vec2::splat(28.0)), "search", th.text3, 1.8);
            ui.painter().text(egui::pos2(c.x, c.y + 4.0), egui::Align2::CENTER_CENTER, crate::i18n::tr("Keine Klasse gefunden"), FontId::new(13.0, semibold()), th.text);
            if let Some(s) = suggest(es, current_q) {
                let g = ui.painter().layout_no_wrap(crate::i18n::tr_dyn(&format!("Meintest du {s}?")), sans(12.0), th.text2);
                let lr = Rect::from_center_size(egui::pos2(c.x, c.y + 26.0), g.size());
                let resp = ui.interact(lr, ui.id().with("suggest"), Sense::click());
                let pre = ui.painter().layout_no_wrap("Meintest du ".into(), sans(12.0), th.text2);
                ui.painter().galley(lr.min, pre.clone(), th.text2);
                let sg = ui.painter().layout_no_wrap(s.clone(), mono(12.0), th.accent_text);
                let sx = lr.left() + pre.size().x;
                ui.painter().galley(egui::pos2(sx, lr.top()), sg.clone(), th.accent_text);
                ui.painter().line_segment([egui::pos2(sx, lr.bottom()), egui::pos2(sx + sg.size().x, lr.bottom())], Stroke::new(1.0f32, th.accent_text));
                ui.painter().text(egui::pos2(sx + sg.size().x, lr.top()), egui::Align2::LEFT_TOP, "?", sans(12.0), th.text2);
                if resp.clicked() {
                    self.query = s;
                }
            }
            return out;
        }
        let active_row = items.get(self.active).map(|x| x.0);
        let q = self.query.trim().to_ascii_lowercase();
        egui::ScrollArea::vertical().id_salt("classlist_rows").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let w = ui.available_width();
            for (ri, row) in rows.iter().enumerate() {
                match row {
                    Row::Header { label, count, group, open } => {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), if group.is_some() { Sense::click() } else { Sense::hover() });
                        let y = r.bottom() - 8.0;
                        match group {
                            Some(g) => {
                                let cr = Rect::from_center_size(egui::pos2(r.left() + 12.0, y), Vec2::splat(11.0));
                                let pts = if *open { [cr.left_top() + Vec2::new(1.0, 3.0), cr.right_top() + Vec2::new(-1.0, 3.0), cr.center_bottom() + Vec2::new(0.0, -2.0)] } else { [cr.left_top() + Vec2::new(3.0, 1.0), cr.left_bottom() + Vec2::new(3.0, -1.0), cr.right_center() + Vec2::new(-2.0, 0.0)] };
                                ui.painter().add(egui::Shape::convex_polygon(pts.to_vec(), th.text3, Stroke::NONE));
                                ui.painter().text(egui::pos2(r.left() + 24.0, y), egui::Align2::LEFT_CENTER, g, FontId::new(12.0, egui::FontFamily::Monospace), th.text2);
                                if resp.clicked() {
                                    if *open {
                                        self.expanded.remove(g);
                                    } else {
                                        self.expanded.insert(g.clone());
                                    }
                                }
                            }
                            None => {
                                ui.painter().text(egui::pos2(r.left() + 8.0, y), egui::Align2::LEFT_CENTER, label.to_uppercase(), FontId::new(11.0, semibold()), th.text3);
                            }
                        }
                        ui.painter().text(egui::pos2(r.right() - 8.0, y), egui::Align2::RIGHT_CENTER, count.to_string(), sans(11.0), th.text3);
                    }
                    Row::Item(i) => {
                        let e = &es[*i];
                        let disabled = solid_only && !e.solid;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
                        let is_active = Some(ri) == active_row;
                        if is_active && self.scroll_to_active {
                            ui.scroll_to_rect(r, None);
                            self.scroll_to_active = false;
                        }
                        let fade = |c: Color32| if disabled { alpha(c, 0.45) } else { c };
                        if is_active || e.name == current {
                            ui.painter().rect(r, 6.0, th.accent_soft, if is_active { Stroke::new(1.0f32, th.accent) } else { Stroke::NONE }, StrokeKind::Inside);
                        } else if resp.hovered() && !disabled {
                            ui.painter().rect_filled(r, 6.0, th.hover);
                        }
                        // Farbpunkt: Kreis = Punkt-Entity, Quadrat = Brush
                        let c = fade(e.color.unwrap_or(th.text3));
                        let pc = egui::pos2(r.left() + 12.0, r.center().y);
                        if e.solid {
                            ui.painter().rect_filled(Rect::from_center_size(pc, Vec2::splat(8.0)), 2.0, c);
                        } else {
                            ui.painter().circle_filled(pc, 4.0, c);
                        }
                        // Kennzeichen rechts
                        let badge = if e.solid { "Brush" } else { crate::i18n::tr("Punkt") };
                        let bg = ui.painter().layout_no_wrap(badge.into(), FontId::new(10.5, semibold()), th.text3);
                        let br = Rect::from_min_size(egui::pos2(r.right() - 8.0 - bg.size().x - 12.0, r.center().y - 9.0), Vec2::new(bg.size().x + 12.0, 18.0));
                        if e.solid {
                            ui.painter().rect(br, 4.0, Color32::TRANSPARENT, Stroke::new(1.0f32, fade(th.line_strong)), StrokeKind::Inside);
                            ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, badge, FontId::new(10.5, semibold()), fade(th.text2));
                        } else {
                            ui.painter().rect_filled(br, 4.0, fade(th.raised));
                            ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, badge, FontId::new(10.5, semibold()), fade(th.text3));
                        }
                        // Stern (Favorit) – Klick schaltet um
                        let fav = favs.contains(&e.name);
                        let sr = Rect::from_center_size(egui::pos2(br.left() - 14.0, r.center().y), Vec2::splat(18.0));
                        let sresp = ui.interact(sr, ui.id().with(("fav", &e.name)), Sense::click());
                        if fav || sresp.hovered() || resp.hovered() {
                            star(ui.painter(), sr.center(), 6.0, if fav { th.warn } else { alpha(th.text3, 0.6) }, fav);
                        }
                        if sresp.clicked() {
                            out = Some(Pick::ToggleFav(e.name.clone()));
                        }
                        // Name (mit Treffer-Markierung) und Beschreibung
                        let tx = r.left() + 26.0;
                        let maxw = sr.left() - tx - 6.0;
                        let two = row_h >= 36.0 && !e.desc.is_empty();
                        let ny = if two { r.center().y - 8.0 } else { r.center().y };
                        let mut job = egui::text::LayoutJob::default();
                        let nf = egui::TextFormat { font_id: mono(12.5), color: fade(th.text), ..Default::default() };
                        match (!q.is_empty()).then(|| e.name.find(&q)).flatten() {
                            Some(p) => {
                                job.append(&e.name[..p], 0.0, nf.clone());
                                job.append(&e.name[p..p + q.len()], 0.0, egui::TextFormat { color: th.accent_text, background: th.accent_soft, ..nf.clone() });
                                job.append(&e.name[p + q.len()..], 0.0, nf);
                            }
                            None => job.append(&e.name, 0.0, nf),
                        }
                        job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                        let g = ui.painter().layout_job(job);
                        ui.painter().galley(egui::pos2(tx, ny - g.size().y / 2.0), g, th.text);
                        if two {
                            let mut dj = egui::text::LayoutJob::simple_singleline(e.desc.clone(), sans(11.5), fade(th.text3));
                            dj.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                            let dg = ui.painter().layout_job(dj);
                            ui.painter().galley(egui::pos2(tx, r.center().y + 9.0 - dg.size().y / 2.0), dg, th.text3);
                        }
                        let resp = if !two && !e.desc.is_empty() { resp.on_hover_text(&e.desc) } else { resp };
                        if resp.clicked() && !sresp.clicked() && !disabled {
                            if let Some(k) = items.iter().position(|x| x.0 == ri) {
                                self.active = k;
                            }
                            out = Some(Pick::Chosen(e.name.clone()));
                        }
                    }
                }
            }
        });
        out
    }
}

fn star(p: &egui::Painter, c: egui::Pos2, r: f32, col: Color32, filled: bool) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            c + Vec2::new(a.cos(), a.sin()) * rr
        })
        .collect();
    if filled {
        // konkav: als Fächer aus Dreiecken zeichnen
        for i in 0..10 {
            p.add(egui::Shape::convex_polygon(vec![c, pts[i], pts[(i + 1) % 10]], col, Stroke::NONE));
        }
    } else {
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.2f32, col)));
    }
}

/// Kleiner Segment-Umschalter (Höhe 24)
fn small_segment(ui: &mut egui::Ui, current: usize, labels: &[&str]) -> usize {
    let th = t();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, 7.0, th.raised);
    let inner = rect.shrink(2.0);
    let n = labels.len() as f32;
    let sw = inner.width() / n;
    let mut out = current;
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(egui::pos2(inner.left() + sw * i as f32, inner.top()), Vec2::new(sw, inner.height()));
        let resp = ui.interact(r, ui.id().with(("sseg", i, rect.min.x as i32)), Sense::click());
        if i == current {
            if !th.dark {
                ui.painter().rect_filled(r.translate(Vec2::new(0.0, 1.0)), 6.0, alpha(Color32::BLACK, 0.08));
            }
            ui.painter().rect_filled(r, 6.0, th.tab_active);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 6.0, th.hover);
        }
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *l, if i == current { FontId::new(11.5, semibold()) } else { sans(11.5) }, if i == current { th.text } else { th.text2 });
        if resp.clicked() {
            out = i;
        }
    }
    out
}

/// Zustand des Popovers und des Umwandeln-Dialogs
#[derive(Default)]
pub struct ClassPicker {
    pub open: bool,
    pub anchor: Option<Rect>,
    /// Klasse dieses Entities ändern (None = Klasse für neue Entities)
    pub target: Option<u32>,
    list: ClassList,
    pub convert_open: bool,
    convert: ClassList,
    convert_choice: Option<String>,
    convert_props: bool,
    convert_each: bool,
}

impl App {
    fn fgd_source(&self) -> (Option<String>, usize) {
        let n = self.ed.fgd.names(ClassKind::Point).len() + self.ed.fgd.names(ClassKind::Solid).len();
        let file = self.settings.fgd_files.first().map(|f| std::path::Path::new(f).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        (file, n)
    }

    fn remember_class(&mut self, c: &str) {
        self.settings.recent_classes.retain(|x| x != c);
        self.settings.recent_classes.insert(0, c.to_string());
        self.settings.recent_classes.truncate(10);
    }

    fn toggle_fav_class(&mut self, c: String) {
        if let Some(i) = self.settings.fav_classes.iter().position(|x| *x == c) {
            self.settings.fav_classes.remove(i);
        } else {
            self.settings.fav_classes.push(c);
        }
    }

    /// Popover öffnen (unter dem Klassen-Chip)
    pub(crate) fn open_class_picker(&mut self, anchor: Rect) {
        self.open_class_picker_for(anchor, None);
    }

    /// Klassen-Auswahl öffnen; mit `target` ändert die Wahl die Klasse dieses Entities
    pub(crate) fn open_class_picker_for(&mut self, anchor: Rect, target: Option<u32>) {
        self.class_picker.open = true;
        self.class_picker.anchor = Some(anchor);
        self.class_picker.target = target;
        let cur = target.and_then(|id| self.ed.map.entity(id)).map(|e| e.classname().to_string()).unwrap_or_else(|| self.ed.entity_class.clone());
        self.class_picker.list.reset(&cur);
    }

    pub(crate) fn class_picker_popover(&mut self, ctx: &egui::Context) {
        if !self.class_picker.open {
            return;
        }
        let th = t();
        let anchor = self.class_picker.anchor.unwrap_or(Rect::from_min_size(egui::pos2(200.0, 80.0), Vec2::ZERO));
        let es = entries(&self.ed.fgd);
        let favs = self.settings.fav_classes.clone();
        let recents = self.settings.recent_classes.clone();
        let current = self.ed.entity_class.clone();
        let (file, n) = self.fgd_source();
        let mut pick = None;
        let mut close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let resp = egui::Area::new(egui::Id::new("class_picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(anchor.left(), anchor.bottom() + 8.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0f32, th.line_strong))
                    .corner_radius(10)
                    .shadow(egui::epaint::Shadow { offset: [0, 16], blur: 48, spread: 0, color: th.shadow })
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 8 }).show(ui, |ui| {
                            let q = self.class_picker.list.query.trim().to_ascii_lowercase();
                            let hits = (!q.is_empty()).then(|| es.iter().filter(|e| e.name.contains(&q) || e.desc.to_ascii_lowercase().contains(&q)).count());
                            self.class_picker.list.search_ui(ui, true, hits);
                        });
                        ui.painter().hline(ui.min_rect().x_range(), ui.cursor().top(), Stroke::new(1.0f32, th.line));
                        egui::Frame::new().inner_margin(4).show(ui, |ui| {
                            let q = self.class_picker.list.query.clone();
                            pick = self.class_picker.list.list_ui(ui, &es, &favs, &recents, &current, 40.0, 480.0 - 34.0 - 24.0 - 26.0 - 32.0 - 12.0, false, &q);
                        });
                        footer(ui, file.as_deref(), n, 360.0);
                    });
            });
        // Klick außerhalb schließt
        if ctx.input(|i| i.pointer.any_click()) {
            if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                if !resp.response.rect.contains(p) && !anchor.contains(p) {
                    close = true;
                }
            }
        }
        match pick {
            Some(Pick::Chosen(c)) if self.class_picker.target.is_some() => {
                // Klasse des ausgewählten Entities ändern
                let id = self.class_picker.target.unwrap();
                self.remember_class(&c);
                self.ed.checkpoint();
                if let Some(e) = self.ed.map.entity_mut(id) {
                    e.set("classname", c.clone());
                }
                self.ed.entity_class = c;
                self.ed.touch();
                close = true;
            }
            Some(Pick::Chosen(c)) => {
                let solid = self.ed.fgd.class(&c).is_some_and(|k| k.kind == ClassKind::Solid);
                self.remember_class(&c);
                if solid {
                    if self.ed.sel.iter().any(|o| matches!(o, crate::editor::Obj::Solid(_))) {
                        self.ed.tie_to_entity(&c);
                    } else {
                        self.ed.status = format!("{c} ist eine Brush-Entity: erst Brushes auswählen, dann umwandeln (Strg+T)");
                    }
                } else {
                    self.ed.entity_class = c;
                }
                close = true;
            }
            Some(Pick::ToggleFav(c)) => self.toggle_fav_class(c),
            None => {}
        }
        if close {
            self.class_picker.open = false;
        }
    }

    pub(crate) fn open_convert_dialog(&mut self) {
        self.class_picker.convert_open = true;
        self.class_picker.convert.reset("func_");
        self.class_picker.convert.kind = 0;
        // Vorauswahl: zuletzt benutzte Brush-Klasse, sonst func_detail
        let last = self.settings.recent_classes.iter().find(|c| self.ed.fgd.class(c).is_some_and(|k| k.kind == crate::fgd::ClassKind::Solid)).cloned();
        self.class_picker.convert_choice = Some(last.unwrap_or_else(|| "func_detail".into()));
        self.class_picker.convert_props = true;
    }

    pub(crate) fn convert_dialog(&mut self, ctx: &egui::Context) {
        if !self.class_picker.convert_open {
            return;
        }
        let th = t();
        let n_sel = self.ed.sel.iter().filter(|o| matches!(o, crate::editor::Obj::Solid(_))).count();
        // nur Brush-Klassen: Punkt-Klassen sind hier nie wählbar (und Gruppen ohne Brush-Klassen entfallen)
        let es: Vec<Entry> = entries(&self.ed.fgd).into_iter().filter(|e| e.solid).collect();
        let favs = self.settings.fav_classes.clone();
        let recents = self.settings.recent_classes.clone();
        let nsolid = es.iter().filter(|e| e.solid).count();
        let (file, _) = self.fgd_source();
        let mut open = true;
        let mut action = None;
        let mut pick = None;
        let d = theme::Dlg::new(crate::i18n::tr("In Entity umwandeln"), 560.0).subtitle(crate::i18n::tr_dyn(&format!("{} ausgewählt", crate::i18n::count(n_sel, "Brush", "Brushes")))).height(600.0).footer().no_scroll();
        let choice = self.class_picker.convert_choice.clone();
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Header => {}
            theme::Part::Body => {
                ui.add_space(14.0);
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 0 }).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let w = ui.available_width() - 150.0;
                        ui.allocate_ui(Vec2::new(w, 34.0), |ui| self.class_picker.convert.search_ui(ui, false, None));
                        ui.add_space(8.0);
                        let (cr, _) = ui.allocate_exact_size(Vec2::new(140.0, 28.0), Sense::hover());
                        ui.painter().rect(cr, 14.0, th.accent_soft, Stroke::new(1.0f32, th.accent), StrokeKind::Inside);
                        ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr("Nur Brush-Entities"), sans(12.0), th.accent_text);
                    });
                    ui.add_space(10.0);
                    egui::Frame::new().stroke(Stroke::new(1.0f32, th.line)).corner_radius(10).inner_margin(4).show(ui, |ui| {
                        let q = self.class_picker.convert.query.clone();
                        pick = self.class_picker.convert.list_ui(ui, &es, &favs, &recents, choice.as_deref().unwrap_or(""), 44.0, 360.0, true, &q);
                    });
                    ui.add_space(10.0);
                    crate::theme::checkbox(ui, &mut self.class_picker.convert_props, crate::i18n::tr("Danach Eigenschaften öffnen"));
                    crate::theme::checkbox(ui, &mut self.class_picker.convert_each, crate::i18n::tr("Jeden Brush einzeln umwandeln"));
                });
            }
            theme::Part::Footer => {
                let can = choice.is_some() && n_sel > 0;
                let label = match &choice {
                    Some(c) => format!("In {c} umwandeln"),
                    None => crate::i18n::tr("Umwandeln").into(),
                };
                if ui.add_enabled(can, egui::Button::new(RichText::new(label).family(semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    action = Some(1);
                }
                if theme::secondary_button(ui, crate::i18n::tr("Abbrechen")).clicked() {
                    action = Some(0);
                }
                let src = match &file {
                    Some(f) => format!("aus {f} · {nsolid} Brush-Klassen"),
                    None => format!("eingebaute Liste · {nsolid} Brush-Klassen"),
                };
                theme::footer_hint(ui, &src);
            }
        });
        match pick {
            Some(Pick::Chosen(c)) => self.class_picker.convert_choice = Some(c),
            Some(Pick::ToggleFav(c)) => self.toggle_fav_class(c),
            None => {}
        }
        match action {
            Some(1) => {
                if let Some(c) = self.class_picker.convert_choice.clone() {
                    self.remember_class(&c);
                    if self.class_picker.convert_each {
                        let ids: Vec<u32> = self.ed.selected_solid_ids();
                        for id in ids {
                            self.ed.sel = [crate::editor::Obj::Solid(id)].into_iter().collect();
                            self.ed.tie_to_entity(&c);
                        }
                    } else {
                        self.ed.tie_to_entity(&c);
                    }
                    if self.class_picker.convert_props {
                        self.panel_tab = 0;
                    }
                }
                open = false;
            }
            Some(0) => open = false,
            _ => {}
        }
        self.class_picker.convert_open = open;
    }
}

/// Fußzeile mit Herkunft der Klassen
fn footer(ui: &mut egui::Ui, file: Option<&str>, n: usize, w: f32) {
    let th = t();
    match file {
        Some(f) => {
            let (r, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
            ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, th.raised);
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0f32, th.line));
            let mut job = egui::text::LayoutJob::default();
            job.append(crate::i18n::tr("aus "), 0.0, egui::TextFormat { font_id: sans(11.5), color: th.text3, ..Default::default() });
            job.append(f, 0.0, egui::TextFormat { font_id: mono(11.5), color: th.text2, ..Default::default() });
            job.append(&crate::i18n::tr_dyn(&format!(" · {n} Klassen")), 0.0, egui::TextFormat { font_id: sans(11.5), color: th.text3, ..Default::default() });
            let g = ui.painter().layout_job(job);
            ui.painter().galley(egui::pos2(r.left() + 12.0, r.center().y - g.size().y / 2.0), g, th.text3);
            ui.painter().text(egui::pos2(r.right() - 12.0, r.center().y), egui::Align2::RIGHT_CENTER, "↑↓ Enter", mono(10.5), th.text3);
        }
        None => {
            let (r, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
            ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, th.raised);
            ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0f32, th.line));
            let c = egui::pos2(r.left() + 16.0, r.top() + 16.0);
            ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(0.0, -4.5), c + Vec2::new(4.5, 0.0), c + Vec2::new(0.0, 4.5), c + Vec2::new(-4.5, 0.0)], th.warn, Stroke::NONE));
            let mut job = egui::text::LayoutJob::simple(crate::i18n::tr_dyn(&format!("garrysmod.fgd nicht gefunden – eingebaute Liste mit {n} Klassen.")), sans(12.0), th.text2, w - 40.0);
            job.wrap.max_rows = 2;
            let g = ui.painter().layout_job(job);
            ui.painter().galley(egui::pos2(r.left() + 28.0, r.top() + 8.0), g, th.text2);
        }
    }
}
