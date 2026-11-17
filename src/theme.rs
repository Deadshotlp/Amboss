//! Farbschemata (Dunkel/Hell), Schriften und kleine UI-Bausteine im Amboss-Design.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, Vec2};
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub accent: Color32,
    pub accent_fg: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub vp: Color32,
    pub grid: Color32,
    pub grid_major: Color32,
    pub axis: Color32,
    pub brush: Color32,
    pub entity: Color32,
    pub collab1: Color32,
    pub collab2: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
    pub glass: Color32,
    pub tooltip: Color32,
    pub tooltip_text: Color32,
    pub tab_active: Color32,
    pub shadow: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Farbe mit Deckkraft (0–1), vormultipliziert für egui
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0) as u8)
}

impl Theme {
    pub const DARK: Theme = Theme {
        dark: true,
        bg: hex(0x0B0C0F),
        panel: hex(0x15171C),
        raised: hex(0x1D2027),
        hover: hex(0x262A33),
        line: hex(0x262A33),
        line_strong: hex(0x353A46),
        text: hex(0xECEEF2),
        text2: hex(0xA7ADBA),
        text3: hex(0x8A91A0),
        accent: hex(0xFF8A3D),
        accent_fg: hex(0x1B0D03),
        accent_text: hex(0xFFA266),
        accent_soft: Color32::from_rgba_premultiplied(36, 19, 9, 36),
        vp: hex(0x0F1115),
        grid: hex(0x171A20),
        grid_major: hex(0x252932),
        axis: hex(0x3A4252),
        brush: hex(0xC9CED8),
        entity: hex(0xC58CFF),
        collab1: hex(0x4C9AFF),
        collab2: hex(0x2FBF7F),
        ok: hex(0x3DD68C),
        warn: hex(0xF2C14E),
        err: hex(0xFF6B6B),
        glass: Color32::from_rgba_premultiplied(17, 19, 23, 209),
        tooltip: hex(0x2A2E38),
        tooltip_text: hex(0xECEEF2),
        tab_active: hex(0x2A2E38),
        shadow: Color32::from_rgba_premultiplied(0, 0, 0, 128),
    };

    pub const LIGHT: Theme = Theme {
        dark: false,
        bg: hex(0xE6E8EC),
        panel: hex(0xFFFFFF),
        raised: hex(0xF3F4F7),
        hover: hex(0xEAECF0),
        line: hex(0xE1E4E9),
        line_strong: hex(0xCDD2DA),
        text: hex(0x14161B),
        text2: hex(0x4F5562),
        text3: hex(0x666D7A),
        accent: hex(0xC9520A),
        accent_fg: hex(0xFFFFFF),
        accent_text: hex(0xAE4504),
        accent_soft: Color32::from_rgba_premultiplied(20, 8, 1, 26),
        vp: hex(0xFAFBFC),
        grid: hex(0xEEF0F3),
        grid_major: hex(0xDDE1E7),
        axis: hex(0xB7BFCB),
        brush: hex(0x3B414D),
        entity: hex(0x8B3FD9),
        collab1: hex(0x1F6FE0),
        collab2: hex(0x0E8A57),
        ok: hex(0x12804F),
        warn: hex(0x8F5E00),
        err: hex(0xC42F2F),
        glass: Color32::from_rgba_premultiplied(224, 224, 224, 224),
        tooltip: hex(0x14161B),
        tooltip_text: hex(0xFFFFFF),
        tab_active: hex(0xFFFFFF),
        shadow: Color32::from_rgba_premultiplied(4, 4, 5, 46),
    };

    /// Farben der Achsen X/Y/Z
    pub fn axis_colors(&self) -> [Color32; 3] {
        if self.dark {
            [hex(0xFF6B6B), hex(0x4FC46E), hex(0x5C9BFF)]
        } else {
            [hex(0xD93036), hex(0x1E8A44), hex(0x2563D9)]
        }
    }

}

static CURRENT: RwLock<Theme> = RwLock::new(Theme::DARK);

/// Aktuelles Farbschema
pub fn t() -> Theme {
    *CURRENT.read().unwrap()
}

// ------------------------------------------------------------------ Schriften

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let add = |fonts: &mut egui::FontDefinitions, name: &str, data: &'static [u8]| {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(data)));
    };
    add(&mut fonts, "inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "inter-semibold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "inter-bold", include_bytes!("../assets/fonts/Inter-Bold.ttf"));
    add(&mut fonts, "jbmono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    // Inter zuerst, egui-Schriften als Ersatz für Symbole/Emoji
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, with("inter"));
    fonts.families.insert(semibold(), with("inter-semibold"));
    fonts.families.insert(bold(), with("inter-bold"));
    let mono_fallback: Vec<String> = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    fonts.families.insert(FontFamily::Monospace, std::iter::once("jbmono".to_string()).chain(mono_fallback).chain(["inter".to_string()]).collect());
    ctx.set_fonts(fonts);
}

// ------------------------------------------------------------------ Stil

pub fn apply(ctx: &egui::Context, dark: bool) {
    let th = if dark { Theme::DARK } else { Theme::LIGHT };
    *CURRENT.write().unwrap() = th;
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = th.panel;
    v.window_fill = th.panel;
    v.extreme_bg_color = th.raised;
    v.faint_bg_color = th.raised;
    v.code_bg_color = th.raised;
    v.override_text_color = Some(th.text);
    v.hyperlink_color = th.accent_text;
    v.warn_fg_color = th.warn;
    v.error_fg_color = th.err;
    v.window_stroke = Stroke::new(1.0f32, th.line);
    v.window_corner_radius = CornerRadius::same(11);
    v.menu_corner_radius = CornerRadius::same(9);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 24, spread: 0, color: th.shadow };
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 20, spread: 0, color: th.shadow };
    v.selection.bg_fill = alpha(th.accent, if dark { 0.35 } else { 0.25 });
    v.selection.stroke = Stroke::new(1.0f32, th.accent_text);
    v.text_cursor.stroke = Stroke::new(2.0f32, th.accent);
    let r = CornerRadius::same(7);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = th.panel;
    w.noninteractive.weak_bg_fill = th.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0f32, th.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0f32, th.text2);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = th.raised;
    w.inactive.weak_bg_fill = th.raised;
    w.inactive.bg_stroke = Stroke::new(1.0f32, th.line);
    w.inactive.fg_stroke = Stroke::new(1.0f32, th.text);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = th.hover;
    w.hovered.weak_bg_fill = th.hover;
    w.hovered.bg_stroke = Stroke::new(1.0f32, th.line_strong);
    w.hovered.fg_stroke = Stroke::new(1.0f32, th.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;
    w.active.bg_fill = th.accent_soft;
    w.active.weak_bg_fill = th.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0f32, th.accent);
    w.active.fg_stroke = Stroke::new(1.0f32, th.accent_text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;
    w.open.bg_fill = th.hover;
    w.open.weak_bg_fill = th.hover;
    w.open.bg_stroke = Stroke::new(1.0f32, th.line_strong);
    w.open.fg_stroke = Stroke::new(1.0f32, th.text);
    w.open.corner_radius = r;
    v.slider_trailing_fill = true;
    v.striped = true;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
        s.spacing.interact_size = Vec2::new(28.0, 26.0);
        s.spacing.window_margin = egui::Margin::same(14);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.combo_height = 320.0;
        s.text_styles.insert(egui::TextStyle::Body, sans(13.0));
        s.text_styles.insert(egui::TextStyle::Button, sans(13.0));
        s.text_styles.insert(egui::TextStyle::Small, sans(11.5));
        s.text_styles.insert(egui::TextStyle::Monospace, mono(12.0));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::new(17.0, bold()));
    });
}

// ------------------------------------------------------------------ Bausteine

/// Abschnittsüberschrift: 11 px, Großbuchstaben, Text 3
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(crate::i18n::tr_dyn(text).to_uppercase()).font(FontId::new(11.0, semibold())).color(t().text3).extra_letter_spacing(0.6));
}

/// Akzent-Knopf (gefüllt)
pub fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let th = t();
    ui.add(egui::Button::new(RichText::new(crate::i18n::tr_dyn(text)).family(semibold()).color(th.accent_fg)).fill(th.accent).stroke(Stroke::NONE).corner_radius(8).min_size(Vec2::new(0.0, 30.0)))
}

/// Umschalt-Chip (an = Akzent-weich)
pub fn toggle_chip(ui: &mut egui::Ui, on: bool, icon: Option<&str>, text: &str, tip: &str) -> bool {
    let th = t();
    let (fg, bg, stroke) = if on { (th.accent_text, th.accent_soft, Stroke::NONE) } else { (th.text2, Color32::TRANSPARENT, Stroke::new(1.0f32, th.line)) };
    let galley = ui.painter().layout_no_wrap(text.to_string(), sans(12.5), fg);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    // nur Symbol (kompakte Leiste): quadratisch, Symbol mittig
    let icon_only = text.is_empty() && icon.is_some();
    let size = if icon_only { Vec2::new(30.0, 28.0) } else { Vec2::new(galley.size().x + 16.0 + icon_w, 28.0) };
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let bg = if resp.hovered() && !on { th.hover } else { bg };
    ui.painter().rect(rect, 7.0, bg, stroke, egui::StrokeKind::Inside);
    let mut x = if icon_only { rect.center().x - 7.0 } else { rect.left() + 8.0 };
    if let Some(i) = icon {
        crate::icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), Vec2::splat(14.0)), i, fg, 1.7);
        x += icon_w;
    }
    ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    resp.on_hover_text(tip).clicked()
}

/// Kleiner Status-Punkt
pub fn dot(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 3.5, c);
}

/// Segment-Umschalter (wie Tabs), liefert den neuen Index
pub fn segmented(ui: &mut egui::Ui, current: usize, labels: &[&str], full_width: bool) -> usize {
    let th = t();
    let mut out = current;
    let total_w = if full_width { ui.available_width() } else { labels.iter().map(|l| ui.painter().layout_no_wrap(l.to_string(), sans(12.5), th.text).size().x + 24.0).sum::<f32>() + 6.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total_w, 34.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 9.0, th.raised);
    let inner = rect.shrink(3.0);
    let n = labels.len() as f32;
    let mut x = inner.left();
    for (i, l) in labels.iter().enumerate() {
        let w = if full_width { (inner.width() - (n - 1.0) * 2.0) / n } else { ui.painter().layout_no_wrap(l.to_string(), sans(12.5), th.text).size().x + 24.0 };
        let r = egui::Rect::from_min_size(egui::pos2(x, inner.top()), Vec2::new(w, inner.height()));
        let resp = ui.interact(r, ui.id().with(("seg", l, rect.min.x as i32, rect.min.y as i32)), egui::Sense::click());
        let active = i == current;
        if active {
            if !th.dark {
                ui.painter().rect_filled(r.translate(Vec2::new(0.0, 1.0)), 7.0, alpha(Color32::BLACK, 0.08));
            }
            ui.painter().rect_filled(r, 7.0, th.tab_active);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 7.0, th.hover);
        }
        let font = if active { FontId::new(12.5, semibold()) } else { sans(12.5) };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *l, font, if active { th.text } else { th.text2 });
        if resp.clicked() {
            out = i;
        }
        x += w + 2.0;
    }
    out
}

// ------------------------------------------------------------------ Dialoge (gemeinsames Muster)

/// Teil eines Dialogs, für den der Inhalts-Closure aufgerufen wird
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// rechts im Kopf, vor dem Schließen-Knopf (z. B. Segment-Umschalter)
    Header,
    Body,
    /// Fußleiste: links Hinweis/Neben-Aktion, rechts (Layout rechts-nach-links) Haupt- und Nebenknopf
    Footer,
}

/// Beschreibung eines Dialogfensters
pub struct Dlg {
    pub title: String,
    pub subtitle: Option<String>,
    pub width: f32,
    pub height: Option<f32>,
    pub footer: bool,
    pub header_extra: bool,
    pub scroll: bool,
    pub modal_center: bool,
    pub id: Option<String>,
    /// Hintergrund abdunkeln und Klicks daneben abfangen
    pub modal: bool,
}

impl Dlg {
    pub fn new(title: impl Into<String>, width: f32) -> Dlg {
        Dlg { title: crate::i18n::tr_dyn(&title.into()), subtitle: None, width, height: None, footer: false, header_extra: false, scroll: true, modal_center: false, id: None, modal: false }
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Dlg {
        let s = crate::i18n::tr_dyn(&s.into());
        self.subtitle = (!s.is_empty()).then_some(s);
        self
    }
    pub fn height(mut self, h: f32) -> Dlg {
        self.height = Some(h);
        self
    }
    pub fn footer(mut self) -> Dlg {
        self.footer = true;
        self
    }
    pub fn header_extra(mut self) -> Dlg {
        self.header_extra = true;
        self
    }
    pub fn no_scroll(mut self) -> Dlg {
        self.scroll = false;
        self
    }
    pub fn centered(mut self) -> Dlg {
        self.modal_center = true;
        self
    }
    /// Fester Bezeichner (Fensterlage bleibt, auch wenn sich der Titel ändert)
    pub fn with_id(mut self, id: &str) -> Dlg {
        self.id = Some(id.to_string());
        self
    }
    /// Modal: verdeckt den Rest, bis der Dialog geschlossen ist
    pub fn modal(mut self) -> Dlg {
        self.modal = true;
        self.modal_center = true;
        self
    }
    pub fn id(mut self, id: impl Into<String>) -> Dlg {
        self.id = Some(id.into());
        self
    }
}

/// Dialog im Amboss-Stil zeichnen. `f` wird für Kopf-Zusatz, Inhalt und Fuß aufgerufen (je nach `Dlg`).
pub fn dialog(ctx: &egui::Context, open: &mut bool, d: &Dlg, mut f: impl FnMut(&mut egui::Ui, Part)) {
    if !*open {
        return;
    }
    let th = t();
    // nie größer als das Fenster (hohe Skalierung, kleine Bildschirme) – dann wird gescrollt
    let scr = ctx.screen_rect();
    let width = d.width.min(scr.width() - 24.0).max(280.0);
    let height = d.height.map(|h| h.min(scr.height() - 24.0).max(200.0));
    let squeezed = d.height.is_some_and(|h| h > scr.height() - 24.0);
    let frame = egui::Frame::new()
        .fill(th.panel)
        .stroke(Stroke::new(1.0f32, th.line_strong))
        .corner_radius(12)
        .shadow(egui::epaint::Shadow { offset: [0, 18], blur: 48, spread: 0, color: th.shadow })
        .inner_margin(0);
    let id = egui::Id::new(("amboss_dlg", d.id.clone().unwrap_or_else(|| d.title.clone())));
    // feste Breite, nicht skalierbar: Inhalte brechen um statt den Dialog zu verbreitern (sonst enden
    // Kopf-Trennlinie und Fuß vor dem rechten Rand); Höhe folgt dem Inhalt bzw. ist fest
    let mut w = egui::Window::new(d.title.clone()).id(id).title_bar(false).frame(frame).resizable(false).collapsible(false).fixed_size([width, height.unwrap_or(0.0)]);
    if height.is_none() {
        w = egui::Window::new(d.title.clone()).id(id).title_bar(false).frame(frame).resizable(false).collapsible(false).default_width(width).min_width(width).max_width(width);
    }
    if d.modal_center {
        w = w.anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    } else {
        // erstes Öffnen mittig über dem Fenster (unabhängig von Größe und Sprache), danach merkt egui die Position
        w = w.pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center() + Vec2::new(26.0, 0.0));
    }
    if d.modal {
        // abgedunkelter Hintergrund fängt alle Klicks ab
        let screen = ctx.screen_rect();
        egui::Area::new(id.with("modal_backdrop")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
            ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(140));
        });
        w = w.order(egui::Order::Foreground);
    }
    let mut close = false;
    w.show(ctx, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 0.0;
        // Kopf
        let (hr, _) = ui.allocate_exact_size(Vec2::new(width, 52.0), egui::Sense::hover());
        let mut header = ui.new_child(egui::UiBuilder::new().max_rect(hr.shrink2(Vec2::new(0.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        header.add_space(20.0);
        header.label(RichText::new(&d.title).font(FontId::new(15.0, semibold())));
        if let Some(s) = &d.subtitle {
            header.add_space(4.0);
            header.add(egui::Label::new(RichText::new(s).font(sans(12.5)).color(th.text3)).truncate());
        }
        header.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            let (cr, cresp) = ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::click());
            if cresp.hovered() {
                ui.painter().rect_filled(cr, 7.0, th.hover);
            }
            crate::icons::draw(ui.painter(), cr.shrink(7.0), "close", th.text3, 2.0);
            if cresp.on_hover_text(crate::i18n::tr("Schließen (Esc)")).clicked() {
                close = true;
            }
            if d.header_extra {
                ui.add_space(8.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                f(ui, Part::Header);
            }
        });
        ui.painter().hline(hr.x_range(), hr.bottom(), Stroke::new(1.0f32, th.line));
        // Inhalt
        let footer_h = if d.footer { 60.0 } else { 0.0 };
        let body_max = height.map(|h| h - 52.0 - footer_h).unwrap_or_else(|| (ctx.screen_rect().height() * 0.8 - 52.0 - footer_h).max(120.0));
        egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 16, bottom: 18 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.set_width(width - 40.0);
            if let Some(h) = height {
                ui.set_height(h - 52.0 - footer_h - 34.0);
            }
            if d.scroll || squeezed {
                egui::ScrollArea::vertical().max_height(body_max - 34.0).auto_shrink([false, true]).show(ui, |ui| f(ui, Part::Body));
            } else {
                f(ui, Part::Body);
            }
        });
        // Fuß
        if d.footer {
            let (fr, _) = ui.allocate_exact_size(Vec2::new(width, 60.0), egui::Sense::hover());
            ui.painter().rect_filled(fr, CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 }, th.raised);
            ui.painter().hline(fr.x_range(), fr.top(), Stroke::new(1.0f32, th.line));
            let mut fui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(fr.min + Vec2::new(20.0, 0.0), fr.max - Vec2::new(16.0, 0.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
            fui.spacing_mut().item_spacing.x = 8.0;
            f(&mut fui, Part::Footer);
        }
    });
    if d.modal {
        // Klick auf den Hintergrund darf den Dialog nicht verdecken
        ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
    }
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) && is_top(ctx, id) {
        *open = false;
    }
}

fn is_top(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.memory(|m| m.layer_ids().last().is_some_and(|l| l.id == id))
}

/// Dialog ohne Fuß und Kopf-Zusatz
pub fn dialog_simple(ctx: &egui::Context, open: &mut bool, d: &Dlg, body: impl FnOnce(&mut egui::Ui)) {
    let mut body = Some(body);
    dialog(ctx, open, d, |ui, p| {
        if p == Part::Body {
            if let Some(b) = body.take() {
                b(ui);
            }
        }
    });
}

/// Zahlenfeld mit dem Dezimaltrennzeichen der Sprache (Eingabe mit Komma oder Punkt)
pub fn dv(d: egui::DragValue<'_>) -> egui::DragValue<'_> {
    d.custom_formatter(|v, range| crate::i18n::dec(egui::emath::format_with_decimals_in_range(v, range)))
        .custom_parser(|s| s.trim().replace(',', ".").parse::<f64>().ok())
}

/// Schieberegler mit dem Dezimaltrennzeichen der Sprache
pub fn sl(d: egui::Slider<'_>) -> egui::Slider<'_> {
    d.custom_formatter(|v, range| crate::i18n::dec(egui::emath::format_with_decimals_in_range(v, range)))
        .custom_parser(|s| s.trim().replace(',', ".").parse::<f64>().ok())
}

/// Einheitliche Checkbox: eckiges Kästchen mit sichtbarem Rand (die allgemeine Rundung von 7 px
/// machte das 14-px-Kästchen sonst zum Kreis – sah aus wie ein Radio-Knopf)
pub fn checkbox(ui: &mut egui::Ui, value: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.scope(|ui| {
        box_style(ui);
        ui.checkbox(value, text)
    })
    .inner
}

/// Kästchen-Stil im aktuellen Bereich (für eigene Checkbox-Aufrufe)
pub fn box_style(ui: &mut egui::Ui) {
    let th = t();
    let w = &mut ui.visuals_mut().widgets;
    for st in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active] {
        st.corner_radius = CornerRadius::same(4);
    }
    w.inactive.bg_fill = th.vp;
    w.inactive.bg_stroke = Stroke::new(1.0f32, th.line_strong);
    w.hovered.bg_fill = th.vp;
    w.active.bg_fill = th.vp;
    // Häkchen in Akzentfarbe
    w.inactive.fg_stroke = Stroke::new(2.0f32, th.accent_text);
    w.hovered.fg_stroke = Stroke::new(2.0f32, th.accent_text);
    w.active.fg_stroke = Stroke::new(2.0f32, th.accent_text);
    ui.visuals_mut().override_text_color = Some(th.text);
}

/// Langen Pfad in der Mitte kürzen ("/home/…/garrysmod"), damit das wichtige Ende sichtbar bleibt
pub fn short_path(p: &str, max_chars: usize) -> String {
    let chars: Vec<char> = p.chars().collect();
    if chars.len() <= max_chars || max_chars < 8 {
        return p.to_string();
    }
    let tail = (max_chars * 2 / 3).max(4);
    let head = max_chars.saturating_sub(tail + 1).max(3);
    format!("{}…{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}

/// Auswahlkarte (ganze Breite): Titel + umbrechende Beschreibung, gewählt = Akzentrand und -fläche
pub fn choice_card(ui: &mut egui::Ui, selected: bool, title: &str, text: &str) -> egui::Response {
    let th = t();
    let w = ui.available_width();
    let title_g = ui.painter().layout_no_wrap(title.to_string(), FontId::new(13.5, semibold()), th.text);
    let text_g = ui.painter().layout(text.to_string(), sans(12.5), th.text2, w - 46.0);
    let h = 14.0 + title_g.size().y + 4.0 + text_g.size().y + 14.0;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());
    let (fill, stroke) = if selected { (th.accent_soft, Stroke::new(1.5f32, th.accent)) } else if resp.hovered() { (th.hover, Stroke::new(1.0f32, th.line_strong)) } else { (th.panel, Stroke::new(1.0f32, th.line)) };
    ui.painter().rect(r, 10.0, fill, stroke, egui::StrokeKind::Inside);
    // Auswahlpunkt
    let c = egui::pos2(r.left() + 18.0, r.top() + 14.0 + title_g.size().y / 2.0);
    ui.painter().circle_stroke(c, 7.0, Stroke::new(1.5f32, if selected { th.accent } else { th.line_strong }));
    if selected {
        ui.painter().circle_filled(c, 3.5, th.accent);
    }
    ui.painter().galley(egui::pos2(r.left() + 34.0, r.top() + 14.0), title_g, th.text);
    ui.painter().galley(egui::pos2(r.left() + 34.0, r.top() + 14.0 + 18.0 + 4.0), text_g, th.text2);
    resp
}

/// Platzhalter in Eingabefeldern: deutlich schwächer als eingegebener Text
pub fn hint(s: impl Into<String>) -> RichText {
    RichText::new(s.into()).color(t().text3)
}

/// Suchfeld mit Lupe und Platzhalter (32 px hoch)
pub fn search_field(ui: &mut egui::Ui, text: &mut String, placeholder: &str, width: f32) -> egui::Response {
    let th = t();
    let (r, _) = ui.allocate_exact_size(Vec2::new(width, 32.0), egui::Sense::hover());
    ui.painter().rect(r, 8.0, th.vp, Stroke::new(1.0f32, th.line), egui::StrokeKind::Inside);
    crate::icons::draw(ui.painter(), egui::Rect::from_center_size(egui::pos2(r.left() + 16.0, r.center().y), Vec2::splat(14.0)), "search", th.text3, 1.6);
    let inner = egui::Rect::from_min_max(egui::pos2(r.left() + 30.0, r.top() + 1.0), egui::pos2(r.right() - 8.0, r.bottom() - 1.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add(egui::TextEdit::singleline(text).hint_text(hint(placeholder)).frame(false).desired_width(inner.width()).margin(egui::Margin::symmetric(2, 6)))
}

/// Hauptknopf im Fuß (Akzent)
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let th = t();
    ui.add(egui::Button::new(RichText::new(crate::i18n::tr_dyn(text)).family(semibold()).color(th.accent_fg)).fill(th.accent).stroke(Stroke::NONE).corner_radius(8).min_size(Vec2::new(0.0, 34.0)))
}

/// Nebenknopf im Fuß (Rand, Panel-Hintergrund)
pub fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let th = t();
    ui.add(egui::Button::new(RichText::new(crate::i18n::tr_dyn(text)).color(th.text)).fill(th.panel).stroke(Stroke::new(1.0f32, th.line)).corner_radius(8).min_size(Vec2::new(0.0, 34.0)))
}

/// Gefährliche Aktion als Text-Knopf in Fehlerfarbe
pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let th = t();
    ui.add(egui::Button::new(RichText::new(crate::i18n::tr_dyn(text)).color(th.err)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE).corner_radius(8).min_size(Vec2::new(0.0, 34.0)))
}

/// Hinweistext links im Fuß (in einem rechts-nach-links-Layout: zuletzt aufrufen)
pub fn footer_hint(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add(egui::Label::new(RichText::new(crate::i18n::tr_dyn(text)).size(12.0).color(t().text3)).truncate());
    });
}

/// Schweregrad-Marke: Fehler = Quadrat, Warnung = Raute, Hinweis = Kreis-Umriss, neu = Kreis
pub fn severity_mark(ui: &mut egui::Ui, kind: u8) {
    let th = t();
    let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
    let c = r.center();
    match kind {
        0 => {
            ui.painter().rect_filled(egui::Rect::from_center_size(c, Vec2::splat(10.0)), 2.0, th.err);
        }
        1 => {
            let d = 5.5;
            ui.painter().add(egui::Shape::convex_polygon(vec![c + Vec2::new(0.0, -d), c + Vec2::new(d, 0.0), c + Vec2::new(0.0, d), c + Vec2::new(-d, 0.0)], th.warn, Stroke::NONE));
        }
        2 => {
            ui.painter().circle_stroke(c, 4.5, Stroke::new(1.5f32, th.text3));
        }
        _ => {
            ui.painter().circle_filled(c, 5.0, th.ok);
        }
    }
}

/// Zeile Beschriftung | Inhalt (für Dialoge und Panel)
pub fn field_row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(140.0, 30.0), egui::Sense::hover());
        let g = ui.painter().layout(crate::i18n::tr_dyn(label), sans(12.5), t().text2, 136.0);
        ui.painter().galley(egui::pos2(r.left(), r.center().y - g.size().y / 2.0), g, t().text2);
        add(ui)
    })
    .inner
}

/// Text in Inter SemiBold
pub fn strong_text(text: &str) -> RichText {
    RichText::new(text).family(semibold())
}
