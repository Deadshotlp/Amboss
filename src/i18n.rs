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
    Fr,
    Es,
    Pl,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::De, Lang::En, Lang::Fr, Lang::Es, Lang::Pl, Lang::Ru];

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::Pl => "pl",
            Lang::Ru => "ru",
        }
    }

    /// Name in der eigenen Sprache (für die Auswahl)
    pub fn native(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::Pl => "Polski",
            Lang::Ru => "Русский",
        }
    }

    pub fn from_code(c: &str) -> Lang {
        Lang::ALL.into_iter().find(|l| l.code() == c).unwrap_or(Lang::De)
    }

    /// Sprache des Betriebssystems (für den ersten Start), sonst Englisch bzw. Deutsch
    pub fn from_system() -> Lang {
        let mut tags: Vec<String> = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"].iter().filter_map(|v| std::env::var(v).ok()).collect();
        #[cfg(windows)]
        tags.push(windows_ui_language());
        let tags: Vec<String> = tags.into_iter().filter(|t| t.len() >= 2 && t != "C" && !t.starts_with("C.") && t != "POSIX").collect();
        for t in &tags {
            let code: String = t.chars().take(2).collect::<String>().to_ascii_lowercase();
            if let Some(l) = Lang::ALL.into_iter().find(|l| l.code() == code) {
                return l;
            }
        }
        // andere Sprache: Englisch ist für die meisten verständlicher als Deutsch
        if tags.is_empty() { Lang::De } else { Lang::En }
    }
}

/// Windows: Sprache der Oberfläche (z. B. "fr-FR")
#[cfg(windows)]
fn windows_ui_language() -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(buf: *mut u16, len: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n - 1) as usize])
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::ALL.get(LANG.load(Ordering::Relaxed) as usize).copied().unwrap_or(Lang::De)
}

/// Komma als Dezimaltrennzeichen?
fn decimal_comma() -> bool {
    lang() != Lang::En
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
            // erster Eintrag gewinnt: die eigene Sprache steht vor der englischen Rückfall-Tabelle
            exact.entry(de).or_insert(en);
        }
    }
    // stabil sortieren: bei gleichen Mustern bleibt die eigene Sprache vorn
    patterns.sort_by_key(|(p, _)| std::cmp::Reverse(p.iter().map(|s| s.len()).sum::<usize>()));
    Table { exact, patterns }
}

const EN_ALL: &str = concat!(include_str!("../assets/lang/en.tsv"), "\n", include_str!("../assets/lang/en_fgd.tsv"));

/// Tabelle der Sprache; was dort fehlt, kommt aus der englischen (dann bleibt Deutsch)
fn table() -> Option<&'static Table> {
    static TABLES: [OnceLock<Table>; 5] = [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let (i, own): (usize, &'static str) = match lang() {
        Lang::De => return None,
        Lang::En => (0, ""),
        Lang::Fr => (1, include_str!("../assets/lang/fr.tsv")),
        Lang::Es => (2, include_str!("../assets/lang/es.tsv")),
        Lang::Pl => (3, include_str!("../assets/lang/pl.tsv")),
        Lang::Ru => (4, include_str!("../assets/lang/ru.tsv")),
    };
    Some(TABLES[i].get_or_init(|| load_layers(&[own, EN_ALL])))
}

fn load_layers(srcs: &[&'static str]) -> Table {
    let mut t = Table { exact: HashMap::new(), patterns: Vec::new() };
    for src in srcs {
        let l = load(src);
        for (k, v) in l.exact {
            t.exact.entry(k).or_insert(v);
        }
        t.patterns.extend(l.patterns);
    }
    t.patterns.sort_by_key(|(p, _)| std::cmp::Reverse(p.iter().map(|s| s.len()).sum::<usize>()));
    t
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
    fn languages() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), l);
        }
        // Rückfall: fehlt eine Zeile in der eigenen Sprache, gilt die englische
        let t = load_layers(&["Speichern\tEnregistrer\n{} Objekt(e) gelöscht\t{} objet(s) supprimé(s)", EN_ALL]);
        assert_eq!(tr_with(&t, "Speichern"), "Enregistrer");
        assert_eq!(tr_with(&t, "Öffnen…"), "Open…");
        assert_eq!(tr_with(&t, "3 Objekt(e) gelöscht"), "3 objet(s) supprimé(s)");
        // jede Sprachdatei: gleiche Platzhalter, kein Schlüssel doppelt, nur bekannte Schlüssel
        let en = load_layers(&[EN_ALL]);
        for (code, src) in [("fr", include_str!("../assets/lang/fr.tsv")), ("es", include_str!("../assets/lang/es.tsv")), ("pl", include_str!("../assets/lang/pl.tsv")), ("ru", include_str!("../assets/lang/ru.tsv"))] {
            let mut seen = std::collections::HashSet::new();
            for line in src.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
                let (de, tx) = line.split_once('\t').unwrap_or_else(|| panic!("{code}: kein Tab: {line}"));
                assert!(!tx.trim().is_empty(), "{code}: leer: {line}");
                assert_eq!(de.matches("{}").count(), tx.matches("{}").count(), "{code}: Platzhalter: {line}");
                assert!(seen.insert(de), "{code}: doppelt: {de}");
                assert!(en.exact.contains_key(unescape(de)) || en.patterns.iter().any(|(p, _)| p.join("{}") == unescape(de)), "{code}: unbekannter Schlüssel: {de}");
            }
        }
    }

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
        let all = load_layers(&[EN_ALL]);
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
    if decimal_comma() { s.replace('.', ",") } else { s }
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
    if lang() == Lang::De {
        return s.to_string();
    }
    s.replace("Strg", "Ctrl").replace("Umschalt", "Shift").replace("Entf", "Del").replace("Einfg", "Ins").replace("Pos1", "Home").replace("Bild↑", "PgUp").replace("Bild↓", "PgDn").replace("Rücktaste", "Backspace").replace("Leertaste", "Space")
}
