//! Compile-Meldungen erklären: bekannte VBSP/VVIS/VRAD-Fehler und -Warnungen in verständlicher Sprache
//! mit Lösung und passender Aktion.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Leck-Pfad (Pointfile) laden
    Pointfile,
    /// zum Objekt aus der Zeile springen
    Goto(u32),
    /// Problemprüfung öffnen
    Problems,
    /// Einstellungen (Pfade) öffnen
    Settings,
    /// Texturen ersetzen öffnen
    Replace,
}

#[derive(Debug, Clone)]
pub struct Hint {
    pub key: &'static str,
    /// Reihenfolge der Regel (wichtigste zuerst)
    order: usize,
    /// 0 = Fehler, 1 = Warnung
    pub level: u8,
    pub title: &'static str,
    pub text: &'static str,
    pub fix: &'static str,
    pub count: usize,
    /// erste passende Zeile
    pub line: String,
    pub actions: Vec<Action>,
}

struct Rule {
    key: &'static str,
    level: u8,
    /// alle Teile müssen (kleingeschrieben) in der Zeile vorkommen; mehrere Muster = eines genügt
    any: &'static [&'static [&'static str]],
    title: &'static str,
    text: &'static str,
    fix: &'static str,
    actions: &'static [Action],
}

const RULES: &[Rule] = &[
    Rule {
        key: "leak",
        level: 0,
        any: &[&["leaked"]],
        title: "Die Map ist nicht dicht (Leck)",
        text: "Irgendwo führt ein Weg von einer Entity ins Leere außerhalb der Map. Dann kann VBSP innen und außen nicht trennen – VVIS bricht ab und VRAD beleuchtet alles hell.",
        fix: "Leck-Pfad laden: die rote Linie führt zum Loch. Dort eine Lücke mit einem Brush schließen. Außenwände dürfen kein func_detail und keine Entity sein; Entities müssen innen liegen.",
        actions: &[Action::Pointfile],
    },
    Rule {
        key: "limit",
        level: 0,
        any: &[&["max_map_"], &["exceeded"], &["too many"]],
        title: "Eine Grenze der Engine ist überschritten",
        text: "Die Map hat zu viele Brushes, Flächen, Ebenen, Entities, Overlays oder Lichtdaten für das BSP-Format.",
        fix: "Kleine Details zu func_detail machen oder als Modell (Auswahl als Modell), verdeckte Flächen auf nodraw setzen, gleiche Brushes zusammenfassen, bei Lichtdaten die Lightmap-Skalierung großer Flächen erhöhen.",
        actions: &[Action::Problems],
    },
    Rule {
        key: "outside",
        level: 0,
        any: &[&["outside world"], &["bad surface extents"], &["outside the map"]],
        title: "Etwas liegt außerhalb der Map-Grenze oder ist zu groß",
        text: "Ein Brush oder eine Entity liegt außerhalb von ±16384 Einheiten, oder eine Fläche ist für ihre Lightmap zu groß (Bad surface extents).",
        fix: "Objekt in die Map schieben. Bei „Bad surface extents“ die Lightmap-Skalierung der Fläche erhöhen (z. B. 32) oder die Fläche teilen.",
        actions: &[Action::Problems],
    },
    Rule {
        key: "material",
        level: 1,
        any: &[&["material", "not found"], &["couldn't find material"], &["unable to load material"], &["texture", "not found"]],
        title: "Textur fehlt",
        text: "Ein Material aus der Map gibt es im Spiel nicht. Im Spiel erscheint dann das lila-schwarze Schachbrett.",
        fix: "Schreibweise prüfen, fehlende Texturen ersetzen oder die eigenen Dateien ins Spiel kopieren und beim Kompilieren in die BSP packen lassen.",
        actions: &[Action::Replace],
    },
    Rule {
        key: "model",
        level: 1,
        any: &[&["error loading studio model"], &["model", "not found"], &["unable to load model"]],
        title: "Modell fehlt",
        text: "Ein Prop verweist auf ein Modell, das im Spiel nicht gefunden wurde.",
        fix: "Modellpfad prüfen (Modell-Browser) oder das Modell bzw. das Addon, aus dem es stammt, installieren und mitpacken.",
        actions: &[Action::Problems],
    },
    Rule {
        key: "dynamic",
        level: 1,
        any: &[&["must be used on a dynamic entity"], &["prop_static", "dynamic"]],
        title: "Dieses Modell geht nicht als prop_static",
        text: "Das Modell ist für bewegliche oder animierte Props gemacht (z. B. mit Physik oder Animation).",
        fix: "Die Entity auf prop_dynamic oder prop_physics umstellen.",
        actions: &[],
    },
    Rule {
        key: "mixed",
        level: 1,
        any: &[&["mixed face contents"]],
        title: "Brush mit gemischten Inhalten",
        text: "Ein Brush hat Flächen mit verschiedenen Inhaltsarten (z. B. Wasser und fester Stein, oder clip und normale Textur).",
        fix: "Alle Flächen des Brushes auf dieselbe Art bringen – z. B. Wasser nur oben, sonst nodraw; oder den Brush teilen.",
        actions: &[],
    },
    Rule {
        key: "broken",
        level: 0,
        any: &[&["plane with no normal"], &["invalid solid"], &["bad brush"], &["no visible sides"], &["has no sides"]],
        title: "Kaputter Brush",
        text: "Ein Brush ist nicht konvex, zu dünn oder hat doppelte Ebenen – oft nach freiem Vertex-Bearbeiten.",
        fix: "Brush auswählen, löschen und neu bauen, oder mit „Am Raster ausrichten“ (Strg+B) begradigen. Die Problemprüfung findet ihn.",
        actions: &[Action::Problems],
    },
    Rule {
        key: "areaportal",
        level: 0,
        any: &[&["areaportal", "doesn't touch two areas"], &["areaportal", "leak"], &["portal", "doesn't touch"]],
        title: "Areaportal trennt keine zwei Bereiche",
        text: "Ein func_areaportal muss eine Öffnung genau verschließen, sodass auf beiden Seiten ein eigener, dichter Bereich liegt.",
        fix: "Areaportal so anpassen, dass es die Tür- bzw. Fensteröffnung vollständig ausfüllt und die Bereiche dahinter dicht sind; sonst entfernen.",
        actions: &[Action::Problems],
    },
    Rule {
        key: "overlay",
        level: 1,
        any: &[&["overlay", "too many"], &["overlay", "faces"]],
        title: "Overlay liegt auf zu vielen Flächen",
        text: "Ein Overlay darf nur eine begrenzte Zahl von Flächen berühren.",
        fix: "Overlay verkleinern oder die Liste der Flächen (sides) kürzen.",
        actions: &[],
    },
    Rule {
        key: "start",
        level: 0,
        any: &[&["konnte nicht gestartet werden"], &["could not be started"], &["no such file or directory"], &["can't find", ".exe"]],
        title: "Ein Compile-Programm wurde nicht gefunden",
        text: "VBSP, VVIS, VRAD oder das Spiel liegen nicht dort, wo die Einstellungen sie suchen.",
        fix: "Einstellungen → Spiel & Pfade prüfen; der Einrichtungs-Assistent findet Garry's Mod meist automatisch.",
        actions: &[Action::Settings],
    },
    Rule {
        key: "memory",
        level: 0,
        any: &[&["out of memory"], &["memory allocation"]],
        title: "Nicht genug Speicher",
        text: "Ein Compile-Programm ist am Speicher gescheitert – meist bei sehr großen Maps oder sehr feinen Lightmaps.",
        fix: "Andere Programme schließen, Lightmap-Skalierung großer Flächen erhöhen, Details zu func_detail machen.",
        actions: &[],
    },
];

/// Protokoll durchsuchen; jede Regel höchstens einmal (mit Anzahl)
pub fn explain(log: &[String]) -> Vec<Hint> {
    let mut out: Vec<Hint> = Vec::new();
    for line in log {
        let l = line.to_ascii_lowercase();
        for (ri, r) in RULES.iter().enumerate() {
            if !r.any.iter().any(|parts| parts.iter().all(|p| l.contains(p))) {
                continue;
            }
            match out.iter_mut().find(|h| h.key == r.key) {
                Some(h) => h.count += 1,
                None => {
                    let mut actions: Vec<Action> = Vec::new();
                    if let Some(id) = crate::compile::find_id_in_line(line) {
                        actions.push(Action::Goto(id));
                    }
                    actions.extend_from_slice(r.actions);
                    out.push(Hint { key: r.key, order: ri, level: r.level, title: r.title, text: r.text, fix: r.fix, count: 1, line: line.clone(), actions });
                }
            }
            break;
        }
    }
    out.sort_by_key(|h| (h.level, h.order));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_messages() {
        let log: Vec<String> = [
            "Processing areas...done (0)",
            "**** leaked ****",
            "Entity info_player_start (-48.00 -32.00 -96.00) leaked!",
            "Material DEV/FOO not found!",
            "Material BAR/BAZ not found!",
            "Brush 1234: plane with no normal",
            "Error! prop_static using model \"models/props_c17/door01.mdl\", which must be used on a dynamic entity (i.e. prop_physics). Deleted.",
            "Wrote ../maps/test.bsp",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let h = explain(&log);
        let keys: Vec<&str> = h.iter().map(|x| x.key).collect();
        assert!(keys.contains(&"leak") && keys.contains(&"material") && keys.contains(&"broken") && keys.contains(&"dynamic"), "{keys:?}");
        assert_eq!(h.iter().find(|x| x.key == "leak").unwrap().count, 2);
        assert_eq!(h.iter().find(|x| x.key == "material").unwrap().count, 2);
        // Fehler vor Warnungen, das Leck zuerst, Brush-ID wird zur Aktion
        assert_eq!(h[0].key, "leak");
        let broken = h.iter().find(|x| x.key == "broken").unwrap();
        assert!(broken.actions.contains(&Action::Goto(1234)), "{:?}", broken.actions);
        assert!(explain(&["Wrote ../maps/test.bsp".to_string()]).is_empty());
    }
}
