//! „Neu in Amboss“: nach einem Update einmal die Patchnotes aller übersprungenen Versionen zeigen.
//! Quelle: der eingebaute Changelog (Deutsch); in anderen Sprachen die englischen Release-Texte von GitHub.

use crate::app::App;
use crate::i18n::tr;
use crate::update::{newer, CURRENT};
use eframe::egui::{self, RichText};
use std::sync::mpsc::{channel, Receiver};

const CHANGELOG: &str = include_str!("../CHANGELOG.md");

/// Abschnitte „## x.y.z“ des Changelogs (neueste zuerst)
pub fn sections(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("## ") {
            out.push((v.trim().trim_start_matches(['v', 'V']).to_string(), String::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    for s in &mut out {
        s.1 = s.1.trim().to_string();
    }
    out
}

/// Versionen nach `from` bis einschließlich `to`
pub fn between(all: &[(String, String)], from: &str, to: &str) -> Vec<(String, String)> {
    all.iter().filter(|(v, _)| (from.is_empty() || newer(v, from)) && !newer(v, to)).cloned().collect()
}

#[derive(Default)]
pub(crate) struct WhatsNew {
    pub open: bool,
    /// (Version, Text) neueste zuerst
    pub notes: Vec<(String, String)>,
    /// Text kommt aus dem deutschen Changelog (in anderen Sprachen: Ersatz, bis GitHub antwortet)
    pub german: bool,
    rx: Option<Receiver<Result<Vec<(String, String)>, String>>>,
    pub title: String,
}

impl App {
    /// Beim Start: wurde Amboss aktualisiert? Dann einmal die Neuerungen zeigen.
    pub(crate) fn check_whats_new(&mut self) {
        let last = self.settings.last_version.clone();
        self.settings.last_version = CURRENT.to_string();
        if last.is_empty() {
            // erster Start überhaupt: nichts zeigen; nur ältere Installationen ohne Merker sehen die aktuelle Version
            if self.settings.setup_done {
                self.open_whats_new(Some(CURRENT));
            }
            return;
        }
        if newer(CURRENT, &last) {
            self.open_whats_new(Some(&last));
        }
    }

    /// `since` = zuletzt benutzte Version (None = alle Versionen)
    pub(crate) fn open_whats_new(&mut self, since: Option<&str>) {
        let all = sections(CHANGELOG);
        let notes = match since {
            Some(v) if v == CURRENT => all.into_iter().filter(|(x, _)| x == CURRENT).collect(),
            Some(v) => between(&all, v, CURRENT),
            None => all,
        };
        if notes.is_empty() {
            return;
        }
        let wn = &mut self.whatsnew;
        wn.title = match since {
            Some(_) => crate::i18n::tr_dyn(&format!("Neu in Amboss {CURRENT}")),
            None => tr("Neuigkeiten aller Versionen").to_string(),
        };
        wn.notes = notes;
        wn.german = true;
        wn.open = true;
        // andere Sprachen: englische Release-Texte nachladen
        if crate::i18n::lang() != crate::i18n::Lang::De {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::update::release_notes());
            });
            wn.rx = Some(rx);
        }
    }

    pub(crate) fn whats_new_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if let Some(rx) = &self.whatsnew.rx {
            match rx.try_recv() {
                Ok(Ok(list)) => {
                    self.whatsnew.rx = None;
                    // ersetzen, wenn es für die neueste gezeigte Version einen Text gibt; Versionen ohne eigenes
                    // Release (übersprungen, Text steht dann im nächsten) entfallen
                    let mapped: Vec<(String, String)> = self.whatsnew.notes.iter().filter_map(|(v, _)| list.iter().find(|(x, _)| x == v).cloned()).collect();
                    if mapped.first().map(|m| &m.0) == self.whatsnew.notes.first().map(|n| &n.0) {
                        self.whatsnew.notes = mapped;
                        self.whatsnew.german = false;
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.whatsnew.rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(300)),
            }
        }
        if !self.whatsnew.open {
            return;
        }
        let th = theme::t();
        let mut open = true;
        let mut close = false;
        let wn = &self.whatsnew;
        let d = theme::Dlg::new(wn.title.clone(), 680.0).height(620.0).footer();
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 8, bottom: 12 }).show(ui, |ui| {
                    if wn.german && crate::i18n::lang() != crate::i18n::Lang::De {
                        ui.label(RichText::new(if wn.rx.is_some() { "Loading the English release notes…" } else { "Release notes in German (the English ones could not be loaded)." }).size(11.5).color(th.text3));
                    }
                    for (v, text) in &wn.notes {
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("Amboss {v}")).font(egui::FontId::new(17.0, theme::semibold())).color(th.text));
                        ui.add_space(2.0);
                        markdown(ui, text, &th);
                    }
                });
            }
            theme::Part::Footer => {
                if theme::primary_button(ui, tr("Los geht's")).clicked() {
                    close = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if ui.link(tr("Alle Releases auf GitHub")).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://github.com/{}/releases", crate::update::REPO)));
                    }
                });
            }
            _ => {}
        });
        if !open || close {
            self.whatsnew.open = false;
        }
    }
}

/// Einfaches Markdown: Überschriften, fette Zeilen, Aufzählungen, **fett** und `Code` im Text
fn markdown(ui: &mut egui::Ui, text: &str, th: &crate::theme::Theme) {
    let inline = |ui: &mut egui::Ui, line: &str, size: f32, color: egui::Color32| {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = ui.available_width();
        let mut bold = false;
        let mut code = false;
        let mut buf = String::new();
        let mut chars = line.chars().peekable();
        let flush = |job: &mut egui::text::LayoutJob, buf: &mut String, bold: bool, code: bool| {
            if buf.is_empty() {
                return;
            }
            let font = if code { crate::theme::mono(size - 1.0) } else if bold { egui::FontId::new(size, crate::theme::semibold()) } else { egui::FontId::proportional(size) };
            job.append(buf, 0.0, egui::TextFormat::simple(font, if bold || code { th.text } else { color }));
            buf.clear();
        };
        while let Some(c) = chars.next() {
            if c == '*' && chars.peek() == Some(&'*') && !code {
                chars.next();
                flush(&mut job, &mut buf, bold, code);
                bold = !bold;
            } else if c == '`' {
                flush(&mut job, &mut buf, bold, code);
                code = !code;
            } else {
                buf.push(c);
            }
        }
        flush(&mut job, &mut buf, bold, code);
        ui.label(job);
    };
    for line in text.lines() {
        let t = line.trim_end();
        if t.trim().is_empty() {
            ui.add_space(4.0);
        } else if let Some(h) = t.strip_prefix("### ") {
            ui.add_space(4.0);
            ui.label(RichText::new(h.replace("**", "")).font(egui::FontId::new(14.5, crate::theme::semibold())).color(th.accent_text));
        } else if t.starts_with("**") && t.ends_with("**") && t.len() > 4 {
            ui.add_space(4.0);
            ui.label(RichText::new(&t[2..t.len() - 2]).font(egui::FontId::new(14.5, crate::theme::semibold())).color(th.accent_text));
        } else if let Some(item) = t.trim_start().strip_prefix("- ").or_else(|| t.trim_start().strip_prefix("* ")) {
            let indent = (t.len() - t.trim_start().len()) as f32 * 6.0;
            ui.horizontal_top(|ui| {
                ui.add_space(4.0 + indent);
                ui.label(RichText::new("•").color(th.text3));
                ui.vertical(|ui| inline(ui, item, 13.0, th.text2));
            });
        } else {
            inline(ui, t, 13.0, th.text2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_sections() {
        let all = sections(CHANGELOG);
        assert!(all.len() >= 5);
        assert_eq!(all[0].0, CURRENT, "Changelog beginnt mit der aktuellen Version");
        assert!(!all[0].1.is_empty());
        let s = sections("# Änderungen\n\n## 0.3.0\n- c\n\n## 0.2.1\n- b\n## 0.2.0\n- a\n## 0.1.0\n- x\n");
        let b = between(&s, "0.2.0", "0.3.0");
        assert_eq!(b.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), vec!["0.3.0", "0.2.1"]);
        assert_eq!(b[1].1, "- b");
        assert!(between(&s, "0.3.0", "0.3.0").is_empty());
        assert_eq!(between(&s, "", "0.2.1").len(), 3);
    }

    #[test]
    fn release_list() {
        let json = r###"[{"tag_name":"v0.12.1","body":"## Amboss 0.12.1\n- fix"},{"tag_name":"v0.12.0","body":"new"}]"###;
        let l = crate::update::parse_release_list(json).unwrap();
        assert_eq!(l[0], ("0.12.1".to_string(), "- fix".to_string()), "eigene Überschrift entfernt");
        assert_eq!(l[1].0, "0.12.0");
    }
}
