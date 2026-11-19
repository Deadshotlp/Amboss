//! Notizen an Stellen in der Map: gespeichert als `info_null` mit dem Schlüssel `amboss_note`.
//! VBSP entfernt info_null beim Kompilieren – im Spiel bleibt nichts übrig; Hammer zeigt sie als
//! normale Punkt-Entity, und in der Live-Sitzung werden sie wie jede Entity abgeglichen.

use crate::editor::{Editor, Obj};
use crate::map::{fmt_vec, Entity};
use glam::DVec3;

pub const CLASS: &str = "info_null";
pub const KEY: &str = "amboss_note";
/// Hammer begrenzt Werte auf 1024 Zeichen
pub const MAX_LEN: usize = 1000;

pub fn is_note(e: &Entity) -> bool {
    e.classname().eq_ignore_ascii_case(CLASS) && e.get(KEY).is_some()
}

/// VMF-Werte dürfen keine Anführungszeichen und Zeilenumbrüche enthalten
pub fn encode(text: &str) -> String {
    let mut s = text.replace('\\', "\\\\").replace("\r\n", "\n").replace('\n', "\\n").replace('"', "''");
    if s.chars().count() > MAX_LEN {
        s = s.chars().take(MAX_LEN).collect();
        // kein halbes Escape am Ende
        if s.ends_with('\\') && !s.ends_with("\\\\") {
            s.pop();
        }
    }
    s
}

pub fn decode(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.peek() {
                Some('n') => {
                    it.next();
                    out.push('\n');
                }
                Some('\\') => {
                    it.next();
                    out.push('\\');
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn text(e: &Entity) -> String {
    decode(e.get(KEY).unwrap_or(""))
}

/// Erste Zeile, gekürzt – für die Beschriftung in den Ansichten
pub fn headline(e: &Entity, max: usize) -> String {
    let t = text(e);
    let first = t.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let mut s: String = first.chars().take(max).collect();
    if first.chars().count() > max || t.lines().filter(|l| !l.trim().is_empty()).count() > 1 {
        s.push('…');
    }
    if s.is_empty() {
        s = crate::i18n::tr("(leere Notiz)").to_string();
    }
    s
}

/// Alle Notizen der Map: (Entity-ID, Position, Text)
pub fn all(ed: &Editor) -> Vec<(u32, DVec3, String)> {
    ed.map.entities.iter().filter(|e| is_note(e)).map(|e| (e.id, e.origin().unwrap_or_default(), text(e))).collect()
}

impl Editor {
    /// Notiz an `pos` anlegen und auswählen
    pub fn create_note(&mut self, pos: DVec3, text: &str) -> u32 {
        self.checkpoint();
        let id = self.map.alloc_id();
        let mut e = Entity::new(id, CLASS);
        e.set("origin", fmt_vec(pos));
        e.set(KEY, encode(text));
        self.map.entities.push(e.into());
        self.sel.clear();
        self.sel.insert(Obj::Entity(id));
        self.status = crate::i18n::tr("Notiz angeheftet – Text rechts im Panel eingeben").into();
        self.touch();
        id
    }
}


// ---------------------------------------------------------------- Oberfläche
use crate::app::App;

use crate::i18n::tr;
use eframe::egui::{self, RichText};

impl App {
    /// Notiz anheften: über der Auswahl, sonst dort, wohin die 3D-Kamera schaut
    pub(crate) fn add_note(&mut self) {
        let b = self.ed.selection_bounds();
        let pos = if !b.is_empty() {
            DVec3::new(b.center().x, b.center().y, b.max.z + 16.0)
        } else {
            let o = self.view3d.cam.pos.as_dvec3();
            let d = self.view3d.cam.forward().as_dvec3();
            match self.ed.pick_3d(o, d) {
                Some((_, _, t)) => o + d * (t - 8.0).max(0.0),
                None => o + d * 256.0,
            }
        };
        let g = self.ed.snap_grid();
        let pos = DVec3::new(crate::geometry::snap(pos.x, g), crate::geometry::snap(pos.y, g), pos.z.round());
        self.ed.create_note(pos, "");
        // Textfeld im Panel bekommt den Fokus
        self.dlg.focus_note = true;
    }

    pub(crate) fn notes_window(&mut self, ctx: &egui::Context) {
        if !self.win.notes {
            return;
        }
        let mut open = true;
        let notes = all(&self.ed);
        let mut go: Option<u32> = None;
        let mut add = false;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(tr("Notizen"), 380.0).height(460.0), |ui| {
            let th = crate::theme::t();
            ui.horizontal(|ui| {
                if crate::theme::primary_button(ui, tr("Notiz anheften")).on_hover_text(tr("Über der Auswahl oder dort, wohin die 3D-Kamera schaut (Strg+Shift+N)")).clicked() {
                    add = true;
                }
            });
            ui.label(RichText::new(tr("Notizen werden beim Kompilieren entfernt (info_null) und in der Live-Sitzung mit allen geteilt.")).small().color(th.text3));
            ui.separator();
            if notes.is_empty() {
                ui.label(RichText::new(tr("Noch keine Notizen in dieser Map.")).color(th.text3));
            }
            for (id, p, text) in &notes {
                let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                let shown = if first.is_empty() { tr("(leere Notiz)").to_string() } else { first };
                let sel = self.ed.sel.contains(&Obj::Entity(*id));
                let r = ui.selectable_label(sel, RichText::new(shown).color(th.text)).on_hover_text(if text.trim().is_empty() { tr("(leere Notiz)").to_string() } else { text.clone() });
                ui.label(RichText::new(format!("{:.0} {:.0} {:.0}", p.x, p.y, p.z)).font(crate::theme::mono(10.5)).color(th.text3));
                if r.clicked() {
                    go = Some(*id);
                }
                ui.add_space(2.0);
            }
        });
        if let Some(id) = go {
            self.ed.sel.clear();
            self.ed.sel.insert(Obj::Entity(id));
            self.ed.touch();
            self.center_on_selection(true);
        }
        if add {
            self.add_note();
        }
        self.win.notes = open;
    }
}

pub const COLOR: egui::Color32 = egui::Color32::from_rgb(255, 214, 90);

/// Notiz in einer Ansicht zeichnen: Stecknadel, erste Zeile; ausgewählt der ganze Text in einem Kasten
pub fn draw(painter: &egui::Painter, at: egui::Pos2, e: &Entity, selected: bool) {
    let th = crate::theme::t();
    painter.circle_filled(at, 6.0, COLOR);
    painter.circle_stroke(at, 6.0, egui::Stroke::new(1.5f32, egui::Color32::from_black_alpha(160)));
    painter.line_segment([at + egui::vec2(-2.5, 0.0), at + egui::vec2(2.5, 0.0)], egui::Stroke::new(1.5f32, egui::Color32::from_black_alpha(200)));
    let font = egui::FontId::proportional(12.0);
    if selected {
        let full = text(e);
        let full = if full.trim().is_empty() { tr("(leere Notiz)").to_string() } else { full };
        let g = painter.layout(full, font, th.text, 260.0);
        // über der Nadel, damit die Größenangabe der Auswahl darunter frei bleibt
        let size = g.size() + egui::vec2(12.0, 8.0);
        let r = egui::Rect::from_min_size(at + egui::vec2(10.0, -10.0 - size.y), size);
        painter.rect_filled(r, 6.0, th.raised);
        painter.rect_stroke(r, 6.0, egui::Stroke::new(1.5f32, COLOR), egui::StrokeKind::Inside);
        painter.galley(r.min + egui::vec2(6.0, 4.0), g, th.text);
    } else {
        let h = headline(e, 40);
        for d in [egui::vec2(1.0, 1.0), egui::vec2(-1.0, 1.0), egui::vec2(1.0, -1.0), egui::vec2(-1.0, -1.0)] {
            painter.text(at + egui::vec2(10.0, 0.0) + d, egui::Align2::LEFT_CENTER, &h, font.clone(), egui::Color32::from_black_alpha(190));
        }
        painter.text(at + egui::vec2(10.0, 0.0), egui::Align2::LEFT_CENTER, &h, font, COLOR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for t in ["Hallo", "Zeile 1\nZeile 2", "mit \"Zitat\"", "Pfad C:\\maps\\x", "a\\nb"] {
            let e = encode(t);
            assert!(!e.contains('"') && !e.contains('\n'), "{e}");
            let back = decode(&e);
            assert_eq!(back, t.replace('"', "''"), "{t}");
        }
        assert!(encode(&"x".repeat(5000)).chars().count() <= MAX_LEN);
    }

    #[test]
    fn create_and_list() {
        let mut ed = Editor::new(crate::fgd::Fgd::builtin());
        let id = ed.create_note(DVec3::new(10.0, 20.0, 30.0), "Hier fehlt noch Licht\nund ein Spawn");
        let n = all(&ed);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].0, id);
        assert_eq!(n[0].1, DVec3::new(10.0, 20.0, 30.0));
        let e = ed.map.entity(id).unwrap();
        assert!(is_note(e));
        assert_eq!(headline(e, 40), "Hier fehlt noch Licht…");
        // bleibt beim Speichern/Laden erhalten
        let vmf = ed.map.to_vmf();
        let m2 = crate::map::Map::from_vmf(&vmf).unwrap();
        assert_eq!(m2.entities.iter().filter(|e| is_note(e)).count(), 1);
        assert_eq!(text(m2.entities.iter().find(|e| is_note(e)).unwrap()), "Hier fehlt noch Licht\nund ein Spawn");
    }
}
