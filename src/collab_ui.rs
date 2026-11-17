//! Fenster „Zusammenarbeit“ und Einbindung der Live-Sitzung in den Editor.

use crate::app::App;
use crate::collab::{Collab, Notice, Role, DEFAULT_PORT};
use crate::p2p::Invite;
use eframe::egui::{self, Color32, RichText};
use std::sync::mpsc::Receiver;

pub(crate) struct CollabForm {
    pub name: String,
    pub port: u16,
    pub password: String,
    pub address: String,
    /// Zwischengespeicherte eigene IPv6-Adresse (None = noch nicht gesucht)
    pub v6: Option<Option<std::net::Ipv6Addr>>,
    pub chat: String,
    pub error: String,
    /// 0 = Gastgeber sein, 1 = Beitreten
    pub mode: usize,
    /// Beitreten: 0 = Adresse, 1 = Verbindungscode (ohne Portfreigabe)
    pub join_mode: usize,
    /// Gastgeber: Einladung wird erstellt / ist fertig
    pub invite_rx: Option<Receiver<Result<Invite, String>>>,
    pub invite: Option<Invite>,
    pub invite_code: String,
    pub reply_in: String,
    pub host_info: String,
    /// Gast: Code des Gastgebers, eigener Antwortcode, laufender Versuch
    pub code_in: String,
    pub reply_code: String,
    pub guest_rx: Option<Receiver<P2pEvent>>,
}

/// Fortschritt beim Beitritt über Code (läuft im Hintergrund)
pub(crate) enum P2pEvent {
    /// eigener Antwortcode steht fest – jetzt dem Gastgeber schicken
    Code(String),
    Done(Result<Collab, String>),
}

impl Default for CollabForm {
    fn default() -> Self {
        let name = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "Mapper".into());
        CollabForm {
            name,
            port: DEFAULT_PORT,
            password: String::new(),
            address: String::new(),
            v6: None,
            chat: String::new(),
            error: String::new(),
            mode: 0,
            join_mode: 0,
            invite_rx: None,
            invite: None,
            invite_code: String::new(),
            reply_in: String::new(),
            host_info: String::new(),
            code_in: String::new(),
            reply_code: String::new(),
            guest_rx: None,
        }
    }
}

/// Lokale Netzwerkadresse (ohne Daten zu senden: UDP-„connect“ wählt nur die Route)
fn local_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

/// Eigene öffentliche IPv6-Adresse (ebenfalls ohne Daten zu senden). None, wenn es keine gibt.
fn global_ipv6() -> Option<std::net::Ipv6Addr> {
    let s = std::net::UdpSocket::bind("[::]:0").ok()?;
    s.connect("[2001:4860:4860::8888]:53").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V6(ip) if (ip.segments()[0] & 0xe000) == 0x2000 => Some(ip),
        _ => None,
    }
}

/// Adresszeile mit Kopieren-Knopf
fn addr_row(ui: &mut egui::Ui, label: &str, addr: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().weak());
        ui.label(RichText::new(addr).monospace());
        if ui.small_button(crate::i18n::tr("Kopieren")).clicked() {
            ui.ctx().copy_text(addr.to_string());
        }
    });
}

/// Code in einer Box mit Kopieren-Knopf
fn code_box(ui: &mut egui::Ui, code: &str) {
    let th = crate::theme::t();
    egui::Frame::new().fill(th.raised).stroke(egui::Stroke::new(1.0f32, th.line)).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(code).monospace()).selectable(true));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(crate::i18n::tr("Kopieren")).clicked() {
                    ui.ctx().copy_text(code.to_string());
                }
            });
        });
    });
}

/// Gastgeber: Gäste über Verbindungscodes einladen (ohne Portfreigabe)
fn host_code_ui(ui: &mut egui::Ui, c: &mut Collab, f: &mut CollabForm) {
    ui.label(RichText::new(crate::i18n::tr("Über das Internet ohne Portfreigabe")).strong());
    if f.invite_rx.is_some() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(crate::i18n::tr("Ermittle deine Internet-Adresse…"));
        });
    } else if f.invite.is_some() {
        ui.label(crate::i18n::tr("1. Schick diesen Einladungscode an deinen Gast:"));
        code_box(ui, &f.invite_code);
        ui.label(crate::i18n::tr("2. Antwortcode des Gasts hier einfügen:"));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.reply_in).hint_text(crate::theme::hint("AMB1-…")).font(egui::TextStyle::Monospace).desired_width(300.0));
            if ui.add_enabled(!f.reply_in.trim().is_empty(), egui::Button::new(crate::i18n::tr("Verbinden"))).clicked() {
                match crate::p2p::parse_code(&f.reply_in) {
                    Ok(peer) => {
                        if let Some(inv) = f.invite.take() {
                            match c.accept_punched(inv, peer) {
                                Ok(()) => f.host_info = crate::i18n::tr("Verbinde… (bis 40 Sekunden). Für weitere Gäste einen neuen Code erstellen.").to_string(),
                                Err(e) => f.host_info = e,
                            }
                        }
                        f.reply_in.clear();
                    }
                    Err(e) => f.host_info = crate::i18n::tr_dyn(&e).to_string(),
                }
            }
        });
        if ui.small_button(crate::i18n::tr("Abbrechen")).clicked() {
            f.invite = None;
            f.host_info.clear();
        }
    } else if ui.button(crate::i18n::tr("Einladungscode erstellen")).clicked() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Invite::new());
        });
        f.invite_rx = Some(rx);
        f.host_info.clear();
    }
    if !f.host_info.is_empty() {
        ui.label(RichText::new(crate::i18n::tr_dyn(&f.host_info)).small().color(crate::theme::t().warn));
    }
    ui.label(RichText::new(crate::i18n::tr("Jeder Gast bekommt einen eigenen Code. Beide Seiten müssen den Code des anderen innerhalb weniger Minuten eingeben.")).small().weak());
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

impl App {
    /// Einmal pro Bild: Sitzung abgleichen (nur im Tab, der zur Sitzung gehört)
    pub(crate) fn poll_collab(&mut self, ctx: &egui::Context) {
        if self.collab.as_ref().is_some_and(|c| c.connected) && !self.ed.collab_session && !self.tabs.iter().flatten().any(|t| t.ed.collab_session) {
            // Tab der Sitzung wurde geschlossen
            self.end_session();
            self.ed.status = crate::i18n::tr("Live-Sitzung beendet (Tab geschlossen)").into();
            return;
        }
        let Some(c) = &mut self.collab else { return };
        if !self.ed.collab_session {
            return;
        }
        let cam = (self.view3d.cam.pos.to_array(), self.view3d.cam.forward().to_array());
        let dragging = ctx.input(|i| i.pointer.any_down());
        for n in c.poll(&mut self.ed, cam, dragging) {
            match n {
                Notice::Joined => {
                    self.ed.status = format!("Sitzung beigetreten – {}", c.status);
                    let b = self.ed.map_bounds();
                    if !b.is_empty() {
                        for v in &mut self.views2d {
                            v.focus(&b, egui::Vec2::new(700.0, 450.0));
                        }
                    }
                }
                Notice::Text(t) => self.ed.status = t,
                Notice::Denied(r) => {
                    self.collab_form.error = r.clone();
                    self.ed.status = format!("Beitritt abgelehnt: {r}");
                    self.ed.collab_session = false;
                }
                Notice::Disconnected => {
                    self.ed.status = c.status.clone();
                    self.ed.collab_session = false;
                }
                Notice::Conflict => {
                    ctx.request_repaint();
                }
            }
        }
        c.expire_conflicts();
        if !c.conflicts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if !c.connected {
            self.ed.remote_users.clear();
            self.ed.remote_locked_solids.clear();
            self.ed.remote_locked_entities.clear();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// Hintergrund-Arbeit für Verbindungscodes abholen (auch wenn das Fenster zu ist)
    fn poll_p2p(&mut self, ctx: &egui::Context) {
        let f = &mut self.collab_form;
        if let Some(rx) = &f.invite_rx {
            match rx.try_recv() {
                Ok(Ok(inv)) => {
                    f.invite_code = inv.code();
                    f.host_info = if inv.public.is_none() { crate::i18n::tr("Internet-Adresse nicht ermittelbar – dieser Code klappt nur im selben Netz.").to_string() } else { String::new() };
                    f.invite = Some(inv);
                    f.invite_rx = None;
                }
                Ok(Err(e)) => {
                    f.host_info = e;
                    f.invite_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
                Err(_) => f.invite_rx = None,
            }
        }
        let mut joined = None;
        if let Some(rx) = &f.guest_rx {
            loop {
                match rx.try_recv() {
                    Ok(P2pEvent::Code(c)) => f.reply_code = c,
                    Ok(P2pEvent::Done(r)) => {
                        f.guest_rx = None;
                        f.reply_code.clear();
                        match r {
                            Ok(c) => joined = Some(c),
                            Err(e) => f.error = e,
                        }
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(std::time::Duration::from_millis(150));
                        break;
                    }
                    Err(_) => {
                        f.guest_rx = None;
                        break;
                    }
                }
            }
        }
        if let Some(c) = joined {
            self.end_session();
            self.collab_form.error.clear();
            self.collab_form.code_in.clear();
            if !self.is_pristine_pub() {
                self.new_tab();
            }
            self.ed.collab_session = true;
            self.ed.status = c.status.clone();
            self.collab = Some(c);
        }
    }

    fn end_session(&mut self) {
        if let Some(mut c) = self.collab.take() {
            if self.ed.collab_session {
                c.flush(&self.ed);
                c.stop(&mut self.ed);
            }
        }
        self.ed.collab_session = false;
        for t in self.tabs.iter_mut().flatten() {
            t.ed.collab_session = false;
            t.ed.remote_users.clear();
            t.ed.remote_locked_solids.clear();
            t.ed.remote_locked_entities.clear();
        }
        self.ed.remote_users.clear();
        self.ed.remote_locked_solids.clear();
        self.ed.remote_locked_entities.clear();
        self.ed.touch();
    }

    /// Konflikt-Toasts unten rechts über den Ansichten (max. 3 gestapelt)
    pub(crate) fn conflict_toasts(&mut self, ctx: &egui::Context) {
        use egui::{Rect, Sense, Stroke, StrokeKind, Vec2};
        let Some(c) = &self.collab else { return };
        if c.conflicts.is_empty() || !self.ed.collab_session {
            return;
        }
        let th = crate::theme::t();
        let screen = ctx.screen_rect();
        let mut action: Option<(usize, bool)> = None;
        let n = c.conflicts.len();
        let shown: Vec<(usize, crate::collab::Conflict)> = c.conflicts.iter().cloned().enumerate().rev().take(3).collect();
        let mut bottom = screen.bottom() - 30.0 - 16.0;
        let right = screen.right() - 344.0 - 16.0;
        if n > 3 {
            let r = Rect::from_min_max(egui::pos2(right - 380.0, bottom - 28.0), egui::pos2(right, bottom));
            egui::Area::new(egui::Id::new("conflict_more")).order(egui::Order::Foreground).fixed_pos(r.min).show(ctx, |ui| {
                let (rr, _) = ui.allocate_exact_size(r.size(), Sense::hover());
                ui.painter().rect(rr, 8.0, th.panel, Stroke::new(1.0f32, th.line_strong), StrokeKind::Inside);
                ui.painter().text(rr.center(), egui::Align2::CENTER_CENTER, crate::i18n::tr_dyn(&format!("+{} weitere Konflikte", n - 3)), crate::theme::sans(12.0), th.text2);
            });
            bottom -= 36.0;
        }
        for (i, cf) in shown {
            let col = Color32::from_rgb(cf.from_color[0], cf.from_color[1], cf.from_color[2]);
            let rows = cf.rows.len() as f32 + if cf.more > 0 { 1.0 } else { 0.0 };
            let h = 14.0 + 40.0 + 12.0 + (24.0 + rows * 20.0) + 12.0 + 32.0 + 12.0 + 16.0 + 14.0;
            let pos = egui::pos2(right - 380.0, bottom - h);
            bottom -= h + 8.0;
            egui::Area::new(egui::Id::new(("conflict", cf.key))).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
                egui::Frame::new()
                    .fill(th.panel)
                    .stroke(Stroke::new(1.0f32, th.line_strong))
                    .corner_radius(12)
                    .shadow(egui::epaint::Shadow { offset: [0, 12], blur: 32, spread: 0, color: th.shadow })
                    .inner_margin(egui::Margin { left: 16, right: 14, top: 14, bottom: 14 })
                    .show(ui, |ui| {
                        ui.set_width(380.0 - 30.0);
                        ui.spacing_mut().item_spacing.y = 12.0;
                        // Kopf: Avatar, Text, X
                        ui.horizontal(|ui| {
                            let (ar, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                            let c0 = ar.center();
                            ui.painter().circle_filled(c0, 19.0, th.warn);
                            ui.painter().circle_filled(c0, 17.0, th.panel);
                            ui.painter().circle_filled(c0, 16.0, col);
                            let ini = cf.from_name.chars().next().unwrap_or('?').to_uppercase().to_string();
                            ui.painter().text(c0, egui::Align2::CENTER_CENTER, ini, egui::FontId::new(14.0, crate::theme::semibold()), Color32::WHITE);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let mut job = egui::text::LayoutJob::default();
                                job.append(&cf.from_name, 0.0, egui::TextFormat { font_id: egui::FontId::new(13.0, crate::theme::semibold()), color: col, ..Default::default() });
                                job.append(crate::i18n::tr(" hat dasselbe Objekt geändert"), 0.0, egui::TextFormat { font_id: egui::FontId::new(13.0, crate::theme::semibold()), color: th.text, ..Default::default() });
                                ui.label(job);
                                ui.label(RichText::new(&cf.label).font(crate::theme::mono(12.0)).color(th.text2));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                let (xr, xresp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                                if xresp.hovered() {
                                    ui.painter().rect_filled(xr, 6.0, th.hover);
                                }
                                crate::icons::draw(ui.painter(), xr.shrink(7.0), "close", th.text2, 1.4);
                                if xresp.on_hover_text(crate::i18n::tr("Später entscheiden (deine Version bleibt vorerst)")).clicked() {
                                    action = Some((i, false));
                                }
                            });
                        });
                        // Vergleich
                        egui::Frame::new().fill(th.raised).corner_radius(8).inner_margin(egui::Margin { left: 10, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            let w = ui.available_width();
                            let col_w = (w - 70.0) / 2.0;
                            let row = |ui: &mut egui::Ui, a: &str, b: &str, c: &str, head: bool| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(w, if head { 16.0 } else { 16.0 }), Sense::hover());
                                let f = if head { crate::theme::sans(11.0) } else { crate::theme::mono(11.5) };
                                ui.painter().text(egui::pos2(r.left(), r.center().y), egui::Align2::LEFT_CENTER, a, if head { crate::theme::sans(11.0) } else { crate::theme::sans(11.5) }, th.text3);
                                for (k, (txt, color)) in [(b, if head { th.text3 } else { th.text }), (c, if head { col } else { th.text })].into_iter().enumerate() {
                                    let x = r.left() + 70.0 + col_w * k as f32;
                                    let mut job = egui::text::LayoutJob::simple_singleline(txt.to_string(), f.clone(), color);
                                    job.wrap = egui::text::TextWrapping::truncate_at_width(col_w - 6.0);
                                    let g = ui.painter().layout_job(job);
                                    ui.painter().galley(egui::pos2(x, r.center().y - g.size().y / 2.0), g, color);
                                }
                            };
                            row(ui, "", crate::i18n::tr("Deins"), &cf.from_name, true);
                            for (k, a, b) in &cf.rows {
                                row(ui, k, a, b, false);
                            }
                            if cf.more > 0 {
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("+{} weitere", cf.more))).size(11.0).color(th.text3));
                            }
                        });
                        // Knöpfe
                        ui.horizontal(|ui| {
                            let bw = (ui.available_width() - 8.0) / 2.0;
                            if ui.add_sized([bw, 32.0], egui::Button::new(RichText::new(crate::i18n::tr("Meins behalten")).color(th.text)).fill(th.panel).stroke(Stroke::new(1.0f32, th.line)).corner_radius(8)).clicked() {
                                action = Some((i, false));
                            }
                            if ui.add_sized([bw, 32.0], egui::Button::new(RichText::new(crate::i18n::tr_dyn(&format!("{} übernehmen", cf.from_name))).family(crate::theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8)).clicked() {
                                action = Some((i, true));
                            }
                        });
                        // Zeitbalken
                        let left = (crate::collab::CONFLICT_SECS - cf.created.elapsed().as_secs_f32()).max(0.0);
                        let (br, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 3.0), Sense::hover());
                        ui.painter().rect_filled(br, 1.5, th.line);
                        ui.painter().rect_filled(Rect::from_min_size(br.min, Vec2::new(br.width() * left / crate::collab::CONFLICT_SECS, 3.0)), 1.5, th.text3);
                        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Ohne Wahl bleibt deins · 0:{:02}", left.ceil() as u32))).size(11.5).color(th.text3));
                    });
            });
        }
        if let Some((i, theirs)) = action {
            if let Some(c) = &mut self.collab {
                c.resolve_conflict(&mut self.ed, i, theirs);
            }
        }
    }

    pub(crate) fn collab_window(&mut self, ctx: &egui::Context) {
        self.poll_p2p(ctx);
        if !self.win.collab {
            return;
        }
        let mut open = true;
        let mut action: Option<u8> = None;
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(crate::i18n::tr("Live-Sitzung"), 460.0), |ui| {
            let f = &mut self.collab_form;
            match &mut self.collab {
                Some(c) if c.connected => {
                    ui.label(RichText::new(crate::i18n::tr_dyn(&c.status)).strong());
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(c32(c.me.color), format!("{} (du{})", c.me.name, if c.role == Role::Host { ", Gastgeber" } else { "" }));
                        for u in &c.users {
                            ui.colored_label(c32(u.color), &u.name);
                        }
                    });
                    if c.role == Role::Host {
                        if f.v6.is_none() {
                            f.v6 = Some(global_ipv6());
                        }
                        ui.label(RichText::new(crate::i18n::tr("Gäste verbinden sich mit")).small());
                        let ip = local_ip().unwrap_or_else(|| "?".into());
                        addr_row(ui, crate::i18n::tr("Heimnetz"), &format!("{ip}:{}", c.port));
                        match f.v6.flatten() {
                            Some(v6) => {
                                addr_row(ui, crate::i18n::tr("Internet (IPv6)"), &format!("[{v6}]:{}", c.port));
                                ui.label(
                                    RichText::new(crate::i18n::tr_dyn(&format!(
                                        "Für das Internet muss die Router-Firewall eingehende IPv6-Verbindungen auf TCP-Port {} zu diesem PC erlauben (z. B. FRITZ!Box: Internet → Freigaben → Gerät → IPv6). Die Adresse kann sich täglich ändern.",
                                        c.port
                                    )))
                                    .small()
                                    .weak(),
                                );
                            }
                            None => {
                                ui.label(RichText::new(crate::i18n::tr("Keine öffentliche IPv6-Adresse gefunden – über das Internet dann nur per Portweiterleitung (IPv4) oder VPN.")).small().weak());
                            }
                        }
                        ui.separator();
                        host_code_ui(ui, c, f);
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(180.0).stick_to_bottom(true).show(ui, |ui| {
                        for (n, col, t) in &c.chat {
                            if n.is_empty() {
                                ui.label(RichText::new(crate::i18n::tr_dyn(t)).italics().color(c32(*col)));
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.colored_label(c32(*col), format!("{n}:"));
                                    ui.label(t);
                                });
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut f.chat).hint_text(crate::theme::hint(crate::i18n::tr("Nachricht…"))).desired_width(300.0));
                        if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button(crate::i18n::tr("Senden")).clicked() {
                            c.say(&f.chat.clone());
                            f.chat.clear();
                            r.request_focus();
                        }
                    });
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::tr("Von anderen ausgewählte Objekte sind für dich gesperrt (in ihrer Farbe umrandet). Strg+Z nimmt nur deine eigenen Schritte zurück.")).small().weak());
                    if ui.button(if c.role == Role::Host { crate::i18n::tr("Sitzung beenden") } else { crate::i18n::tr("Trennen") }).clicked() {
                        action = Some(9);
                    }
                }
                _ => {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr("Dein Name"));
                        ui.text_edit_singleline(&mut f.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr("Passwort"));
                        ui.add(egui::TextEdit::singleline(&mut f.password).password(true));
                    });
                    ui.add_space(4.0);
                    f.mode = crate::theme::segmented(ui, f.mode, &[crate::i18n::tr("Gastgeber sein"), crate::i18n::tr("Beitreten")], true);
                    ui.add_space(4.0);
                    if f.mode == 0 {
                    ui.label(RichText::new(crate::i18n::tr("Die aktuell offene Map wird geteilt.")).small());
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::tr("Port"));
                        ui.add(crate::theme::dv(egui::DragValue::new(&mut f.port)).range(1024..=65535));
                        if ui.button(crate::i18n::tr("Sitzung starten")).clicked() {
                            action = Some(1);
                        }
                    });
                    ui.label(RichText::new(crate::i18n::tr("Nach dem Start erscheinen hier deine Adressen und ein Einladungscode für Gäste ohne Portfreigabe.")).small().weak());
                    } else {
                    f.join_mode = crate::theme::segmented(ui, f.join_mode, &[crate::i18n::tr("Mit Adresse"), crate::i18n::tr("Mit Code (ohne Portfreigabe)")], true);
                    if f.join_mode == 0 {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::tr("Adresse"));
                            ui.add(egui::TextEdit::singleline(&mut f.address).hint_text(crate::theme::hint(crate::i18n::tr("192.168.0.10 oder [2001:db8::1]:3918"))).desired_width(230.0));
                            if ui.button(crate::i18n::tr("Verbinden")).clicked() {
                                action = Some(2);
                            }
                        });
                    } else if f.guest_rx.is_some() {
                        if f.reply_code.is_empty() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(crate::i18n::tr("Ermittle deine Internet-Adresse…"));
                            });
                        } else {
                            ui.label(crate::i18n::tr("2. Schick diesen Antwortcode an den Gastgeber:"));
                            code_box(ui, &f.reply_code);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(crate::i18n::tr("Warte auf den Gastgeber (bis 3 Minuten)…")).small());
                            });
                        }
                        if ui.button(crate::i18n::tr("Abbrechen")).clicked() {
                            f.guest_rx = None;
                            f.reply_code.clear();
                        }
                    } else {
                        ui.label(crate::i18n::tr("1. Einladungscode des Gastgebers einfügen:"));
                        ui.add(egui::TextEdit::singleline(&mut f.code_in).hint_text(crate::theme::hint("AMB1-…")).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                        if ui.add_enabled(!f.code_in.trim().is_empty(), egui::Button::new(crate::i18n::tr("Antwortcode erzeugen und verbinden"))).clicked() {
                            action = Some(3);
                        }
                        ui.label(RichText::new(crate::i18n::tr("Klappt mit den meisten Heim-Routern, ohne Portfreigabe. Nicht in manchen Mobilfunk- und Firmennetzen – dann IPv6, Portweiterleitung oder ein VPN nutzen.")).small().weak());
                    }
                    }
                    if !f.error.is_empty() {
                        ui.colored_label(Color32::LIGHT_RED, crate::i18n::tr_dyn(&f.error));
                    }
                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::tr("Die Verbindung ist verschlüsselt; Schlüssel kommen aus dem Passwort – für das Internet ein langes Passwort wählen (mind. 12 Zeichen). Über das Internet geht es direkt per IPv6, wenn beide IPv6 haben und die Router-Firewall des Gastgebers den Port freigibt. Beide brauchen dieselbe Amboss-Version."))
                            .small()
                            .weak(),
                    );
                }
            }
        });
        match action {
            Some(1) => {
                self.end_session();
                self.collab_form.v6 = None;
                match Collab::host(&self.collab_form.name, self.collab_form.port, &self.collab_form.password, &self.ed) {
                    Ok(c) => {
                        self.collab_form.error.clear();
                        self.ed.status = c.status.clone();
                        self.ed.collab_session = true;
                        self.collab = Some(c);
                    }
                    Err(e) => self.collab_form.error = e,
                }
            }
            Some(2) => {
                self.end_session();
                match Collab::join(&self.collab_form.name, self.collab_form.address.trim(), &self.collab_form.password) {
                    Ok(c) => {
                        self.collab_form.error.clear();
                        if !self.is_pristine_pub() {
                            self.new_tab();
                        }
                        self.ed.collab_session = true;
                        self.ed.status = c.status.clone();
                        self.collab = Some(c);
                    }
                    Err(e) => self.collab_form.error = e,
                }
            }
            Some(3) => match crate::p2p::parse_code(&self.collab_form.code_in) {
                Ok(peer) => {
                    self.collab_form.error.clear();
                    self.collab_form.reply_code.clear();
                    let (tx, rx) = std::sync::mpsc::channel();
                    let (name, pw) = (self.collab_form.name.clone(), self.collab_form.password.clone());
                    std::thread::spawn(move || {
                        let inv = match Invite::new() {
                            Ok(i) => i,
                            Err(e) => {
                                let _ = tx.send(P2pEvent::Done(Err(e)));
                                return;
                            }
                        };
                        if inv.public.is_none() && !peer.public.ip().is_unspecified() {
                            let _ = tx.send(P2pEvent::Done(Err(crate::i18n::tr("Deine Internet-Adresse ließ sich nicht ermitteln (UDP ins Internet gesperrt?). Über das Internet geht es so nicht.").to_string())));
                            return;
                        }
                        if tx.send(P2pEvent::Code(inv.code())).is_err() {
                            return;
                        }
                        let _ = tx.send(P2pEvent::Done(Collab::join_code(&name, inv, &peer, &pw, std::time::Duration::from_secs(180))));
                    });
                    self.collab_form.guest_rx = Some(rx);
                }
                Err(e) => self.collab_form.error = crate::i18n::tr_dyn(&e).to_string(),
            },
            Some(9) => {
                self.end_session();
                self.ed.status = crate::i18n::tr("Sitzung beendet").into();
            }
            _ => {}
        }
        self.win.collab = open;
    }
}
