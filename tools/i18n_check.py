#!/usr/bin/env python3
"""Sucht deutsche UI-Texte ohne englische Übersetzung.

1. Literale in tr("…") / tr_dyn(&format!("…")) / tr_dyn("…"), die weder exakt noch als
   Muster ({} = Platzhalter) in assets/lang/en*.tsv stehen.
2. Verdächtige Literale mit deutschen Wörtern/Umlauten in UI-Aufrufen (label, button,
   RichText::new, on_hover_text, format! für Statusmeldungen …) ohne tr-Hülle.

Aufruf:  python3 tools/i18n_check.py [--all]
"""
import re, sys, os, glob

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
exact, patterns = set(), []
for f in glob.glob(os.path.join(ROOT, "assets/lang/en*.tsv")):
    for line in open(f, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line or line.startswith("#") or "\t" not in line:
            continue
        de = line.split("\t", 1)[0].replace("\\n", "\n")
        if "{}" in de:
            patterns.append(re.compile("^" + ".*?".join(re.escape(p) for p in de.split("{}")) + "$", re.S))
        exact.add(de)

def known(s):
    if s in exact:
        return True
    return any(p.match(s) for p in patterns)

def norm_fmt(s):
    # {name}, {:.1}, {x:>3} → {} ; {{ }} → { }
    s = re.sub(r"\{\{", "\x00", s); s = re.sub(r"\}\}", "\x01", s)
    s = re.sub(r"\{[^{}]*\}", "{}", s)
    return s.replace("\x00", "{").replace("\x01", "}")

def unescape(s):
    return s.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\")

GERMAN = re.compile(r"[äöüÄÖÜß]|\b(und|oder|nicht|keine?|wird|werden|ist|sind|mit|aus|für|beim|zum|zur|der|die|das|den|dem|eine?n?|Datei|Ordner|Fehler|Warnung|Auswahl|ausgewählt|gefunden|geladen|Spiel|Textur|Fläche|wählen|Einstellungen|Speichern|Öffnen|Schließen|Abbrechen|Zurück|Weiter|Neu|Löschen|Kopieren|Einfügen|Ansicht|Werkzeug)\b")
LIT = r'"((?:[^"\\]|\\.)*)"'
missing, raw = [], []
# mcp.rs: Texte für KI-Clients (Werkzeugbeschreibungen) – bewusst nur Deutsch
SKIP = {"src/mcp.rs"}
for path in sorted(glob.glob(os.path.join(ROOT, "src/**/*.rs"), recursive=True)):
    rel = os.path.relpath(path, ROOT)
    if rel in SKIP:
        continue
    src = open(path, encoding="utf-8").read()
    # Tests und Kommentare ausblenden
    src_nc = re.sub(r"#\[cfg\(test\)\][\s\S]*$", "", src)
    lines = src_nc.split("\n")
    for i, line in enumerate(lines, 1):
        code = re.sub(r"//.*$", "", line) if '"' not in line.split("//")[0][-1:] else line
        for m in re.finditer(r"\btr(?:_dyn)?\(\s*&?(?:format!\()?" + LIT, code):
            s = unescape(m.group(1))
            if "format!(" in m.group(0):
                s = norm_fmt(s)
            if not known(s) and GERMAN.search(s):
                missing.append((rel, i, s))
        # rohe UI-Literale ohne tr
        if re.search(r"\btr(_dyn)?\(|tr_keys|i18n::keys|eprintln!|println!|log::|debug!|panic!|expect\(|assert|Err\(|#\[|serde|\.join\(|key\s*==|==\s*\"|starts_with|ends_with|contains\(|strip_prefix|replace\(", code):
            continue
        if True:
            for m in re.finditer(LIT, code):
                s = unescape(m.group(1))
                if len(s) > 3 and (" " in s or re.search("[äöüÄÖÜß]", s)) and GERMAN.search(s) and not known(norm_fmt(s)) and not known(s):
                    raw.append((rel, i, s))

print(f"== tr()-Texte ohne Übersetzung: {len(missing)}")
for r, i, s in missing:
    print(f"{r}:{i}: {s!r}")
print(f"\n== Verdächtige Texte ohne tr(): {len(raw)}")
for r, i, s in raw[: (None if '--all' in sys.argv else 200)]:
    print(f"{r}:{i}: {s!r}")
