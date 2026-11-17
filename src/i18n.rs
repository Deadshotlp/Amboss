//! Sprache der Oberfläche. Deutsch ist die Quellsprache; weitere Sprachen über Tabellen in assets/lang.
//!
//! `tr("Text")` übersetzt feste Texte, `tr_dyn(&text)` auch zusammengesetzte Meldungen
//! (Tabelleneinträge mit `{}` als Platzhalter, z. B. "{} Objekt(e) gelöscht").

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    /// Name in der eigenen Sprache (für die Auswahl)
    pub fn native(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
        }
    }

    pub fn from_code(c: &str) -> Lang {
        match c {
            "en" => Lang::En,
            _ => Lang::De,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::De,
    }
}

struct Table {
    exact: HashMap<&'static str, &'static str>,
    /// Muster mit Platzhaltern: (feste Teile auf Deutsch, englisches Muster)
    patterns: Vec<(Vec<&'static str>, &'static str)>,
}

fn unescape(s: &str) -> &'static str {
    if s.contains("\\n") {
        Box::leak(s.replace("\\n", "\n").into_boxed_str())
    } else {
        Box::leak(s.to_string().into_boxed_str())
    }
}

fn load(src: &'static str) -> Table {
    let mut exact = HashMap::new();
    let mut patterns = Vec::new();
    for line in src.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let Some((de, en)) = line.split_once('\t') else { continue };
        let (de, en) = (unescape(de), unescape(en));
        if de.contains("{}") {
            let parts: Vec<&'static str> = de.split("{}").collect();
            // längste Muster zuerst prüfen (spezifischer)
            patterns.push((parts, en));
        } else {
            exact.insert(de, en);
        }
    }
    patterns.sort_by_key(|(p, _)| std::cmp::Reverse(p.iter().map(|s| s.len()).sum::<usize>()));
    Table { exact, patterns }
}

fn table() -> Option<&'static Table> {
    static EN: OnceLock<Table> = OnceLock::new();
    match lang() {
        Lang::De => None,
        Lang::En => Some(EN.get_or_init(|| load(concat!(include_str!("../assets/lang/en_fgd.tsv"), "\n", include_str!("../assets/lang/en.tsv"))))),
    }
}

/// Fester Text (Literal) in der aktuellen Sprache
pub fn tr(s: &'static str) -> &'static str {
    match table() {
        None => s,
        Some(t) => t.exact.get(s).copied().unwrap_or(s),
    }
}

/// Beliebiger (auch zusammengesetzter) Text in der aktuellen Sprache
pub fn tr_dyn(s: &str) -> String {
    match table() {
        None => s.to_string(),
        Some(t) => tr_with(t, s),
    }
}

fn tr_with(t: &Table, s: &str) -> String {
    if let Some(e) = t.exact.get(s) {
        return e.to_string();
    }
    'pat: for (parts, en) in &t.patterns {
        if !s.starts_with(parts[0]) || !s.ends_with(parts[parts.len() - 1]) || s.len() < parts.iter().map(|p| p.len()).sum::<usize>() {
            continue;
        }
        // Platzhalter-Werte der Reihe nach herauslösen
        let mut vals = Vec::new();
        let mut rest = &s[parts[0].len()..];
        for (i, lit) in parts.iter().enumerate().skip(1) {
            let last = i == parts.len() - 1;
            let pos = if last {
                if lit.is_empty() {
                    Some(rest.len())
                } else {
                    rest.rfind(lit).filter(|p| p + lit.len() == rest.len())
                }
            } else if lit.is_empty() {
                continue 'pat;
            } else {
                rest.find(lit)
            };
            let Some(p) = pos else { continue 'pat };
            vals.push(&rest[..p]);
            rest = &rest[p + lit.len()..];
        }
        let mut out = String::new();
        let mut it = vals.into_iter();
        for (i, piece) in en.split("{}").enumerate() {
            if i > 0 {
                out.push_str(&tr_with(t, it.next().unwrap_or("")));
            }
            out.push_str(piece);
        }
        return out;
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate() {
        // ohne die globale Sprache umzustellen (Tests laufen parallel)
        let t = load(include_str!("../assets/lang/en.tsv"));
        assert_eq!(tr_with(&t, "Speichern"), "Save");
        assert_eq!(tr_with(&t, "unbekannt xyz"), "unbekannt xyz");
        assert_eq!(tr_with(&t, "12 Objekt(e) gelöscht"), "12 object(s) deleted");
        assert_eq!(tr_with(&t, "Geladen: a.vmf (9 Brushes, 4 Entities)"), "Loaded: a.vmf (9 brushes, 4 entities)");
        assert_eq!(tr_with(&t, "Gedreht um 90°"), "Rotated by 90°");
        assert_eq!(tr_with(&t, "ganz anders"), "ganz anders");
        assert_eq!(tr_with(&t, "Spielinhalte geladen: 16 Materialien aus 0 VPKs und 0 Addons"), "Game content loaded: 16 materials from 0 VPKs and 0 addons");
        // jede Zeile der Tabelle hat gleich viele Platzhalter
        for line in include_str!("../assets/lang/en.tsv").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let (de, en) = line.split_once('\t').unwrap_or_else(|| panic!("kein Tab: {line}"));
            assert_eq!(de.matches("{}").count(), en.matches("{}").count(), "Platzhalter: {line}");
        }
        assert_eq!(tr("Speichern"), "Speichern");
        // Entity-Texte (FGD) – die Oberflächen-Tabelle hat bei gleichen Schlüsseln Vorrang
        let all = load(concat!(include_str!("../assets/lang/en_fgd.tsv"), "\n", include_str!("../assets/lang/en.tsv")));
        assert_eq!(tr_with(&all, "Schiebetür (bewegt sich in eine Richtung)."), "Sliding door (moves in one direction).");
        assert_eq!(tr_with(&all, "Beim Öffnen blockiert"), "Blocked opening");
        assert_eq!(tr_with(&all, "Abstand"), "Spacing");
        for line in include_str!("../assets/lang/en_fgd.tsv").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            assert!(line.split_once('\t').is_some_and(|(d, e)| !d.is_empty() && !e.is_empty()), "kaputte Zeile: {line}");
        }
    }
}

/// Anzahl mit Einzahl/Mehrzahl in der aktuellen Sprache: `count(3, "Kind", "Kinder")` → „3 Kinder“ | „3 children“.
/// Die Tabelle braucht dafür „1 Kind“ und „{} Kinder“.
pub fn count(n: usize, one: &str, many: &str) -> String {
    let s = if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    tr_dyn(&s)
}

/// Wie `count`, aber immer deutsch – für Meldungen, die als Ganzes später mit `tr_dyn` übersetzt werden
pub fn count_de(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// Dezimalzahl als Text mit dem Trennzeichen der Sprache (DE: Komma, EN: Punkt)
pub fn dec(s: impl std::fmt::Display) -> String {
    let s = s.to_string();
    if lang() == Lang::De { s.replace('.', ",") } else { s }
}

/// Dateigröße: unter 1 MB in KB, sonst MB mit einer Nachkommastelle
pub fn size(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KB", ((bytes as f64 / 1024.0).ceil() as u64).max(1))
    } else {
        format!("{} MB", dec(format!("{:.1}", bytes as f64 / 1048576.0)))
    }
}

/// Tastenkürzel-Text in der eingestellten Sprache ("Strg+Umschalt+C" → "Ctrl+Shift+C")
pub fn keys(s: &str) -> String {
    if lang() != Lang::En {
        return s.to_string();
    }
    s.replace("Strg", "Ctrl").replace("Umschalt", "Shift").replace("Entf", "Del").replace("Einfg", "Ins").replace("Pos1", "Home").replace("Bild↑", "PgUp").replace("Bild↓", "PgDn").replace("Rücktaste", "Backspace").replace("Leertaste", "Space")
}
