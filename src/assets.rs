//! Welche Dateien eine Map benutzt und woher sie stammen (Spiel, lose, Addon).
//! Grundlage für die Addon-Liste beim Kompilieren und für das Einpacken eigener Inhalte.

use crate::editor::Editor;
use crate::gamefs::{AssetSource, GameFs};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Texturen, die ein Material benutzt (inkl. `patch`-Include), als Pfade "materials/….vtf"
fn material_textures(fs: &GameFs, vmt: &str, depth: u32, out: &mut BTreeSet<String>) {
    let Some(src) = fs.read(vmt) else { return };
    let text = String::from_utf8_lossy(&src).to_ascii_lowercase();
    // Kommentare entfernen, dann Schlüssel/Wert-Paare über alle Zeilen hinweg lesen
    let clean: String = text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    let toks: Vec<&str> = clean.split(|c: char| c == '"' || c.is_whitespace() || c == '{' || c == '}').filter(|s| !s.is_empty()).collect();
    for w in toks.windows(2) {
        let (k, v) = (w[0], w[1].replace('\\', "/"));
        match k {
            "$basetexture" | "$basetexture2" | "$bumpmap" | "$bumpmap2" | "$normalmap" | "$envmapmask" | "$detail" | "$blendmodulatetexture" | "$selfillummask" | "$phongexponenttexture" | "$lightwarptexture" | "$refracttinttexture" | "$dudvmap" | "$texture2" => {
                out.insert(format!("materials/{}.vtf", v.trim_end_matches(".vtf")));
            }
            "$envmap" if v != "env_cubemap" => {
                out.insert(format!("materials/{}.vtf", v.trim_end_matches(".vtf")));
            }
            "include" if depth < 4 => {
                let inc = if v.starts_with("materials/") { v.clone() } else { format!("materials/{v}") };
                out.insert(inc.clone());
                material_textures(fs, &inc, depth + 1, out);
            }
            _ => {}
        }
    }
}

/// Alle Dateien (relativ zum Spielordner), die die Map braucht und die es im Dateisystem gibt
pub fn used_files(ed: &Editor, fs: &GameFs) -> BTreeSet<String> {
    collect(ed, fs, None)
}

/// Wörter einer KeyValues-Textdatei (ohne Kommentare), kleingeschrieben
fn kv_tokens(text: &str) -> Vec<String> {
    let clean: String = text.to_ascii_lowercase().lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in clean.chars() {
        match c {
            '"' => {
                if quoted || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if !quoted && (c.is_whitespace() || c == '{' || c == '}') => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Sound-Pfad aus einem Wert (Präfixe wie ")" oder "#" weg) → "sound/…"
fn sound_file(v: &str) -> String {
    let v = v.replace('\\', "/");
    let v = v.trim_start_matches(|c: char| "*#@<>^)}$!?".contains(c));
    if v.starts_with("sound/") { v.to_string() } else { format!("sound/{v}") }
}

/// Wie `used_files`, mit `map` zusätzlich alles, was zur Map gehört:
/// Nav-Mesh, Soundscapes (+ ihre Sounds), Partikel-Manifest, Übersichtskarte, Partikel-Dateien (+ Materialien)
pub fn collect(ed: &Editor, fs: &GameFs, map: Option<&str>) -> BTreeSet<String> {
    let mut mats: BTreeSet<String> = BTreeSet::new();
    for (_, s) in ed.map.all_solids() {
        for side in &s.sides {
            mats.insert(crate::materials::normalize(&side.material));
        }
    }
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut models: BTreeSet<String> = BTreeSet::new();
    let world = &ed.map.world;
    if let Some(sky) = world.get("skyname").filter(|s| !s.is_empty()) {
        for side in ["rt", "lf", "bk", "ft", "up", "dn"] {
            mats.insert(format!("skybox/{}{side}", sky.to_ascii_lowercase()));
            mats.insert(format!("skybox/{}_hdr{side}", sky.to_ascii_lowercase()));
        }
    }
    if let Some(d) = world.get("detailmaterial") {
        mats.insert(crate::materials::normalize(d));
    }
    for e in &ed.map.entities {
        for (k, v) in &e.props {
            let kl = k.to_ascii_lowercase();
            let vl = v.replace('\\', "/").to_ascii_lowercase();
            if matches!(kl.as_str(), "material" | "texture" | "ropematerial" | "texturename" | "overlaymaterial" | "spritename") {
                mats.insert(crate::materials::normalize(&vl));
            }
            if vl.ends_with(".vmt") || vl.ends_with(".spr") {
                mats.insert(crate::materials::normalize(vl.trim_end_matches(".spr")));
            }
            if vl.ends_with(".wav") || vl.ends_with(".mp3") || vl.ends_with(".ogg") {
                files.insert(sound_file(&vl));
            }
            if vl.ends_with(".mdl") {
                models.insert(vl.clone());
            }
        }
        if let Some(m) = ed.model_path(e) {
            models.insert(m);
        }
    }
    if let Some(name) = map.map(|m| m.to_ascii_lowercase()) {
        for f in [format!("maps/{name}.nav"), format!("maps/{name}_particles.txt"), format!("maps/{name}_level_sounds.txt"), format!("maps/{name}_cameras.txt"), format!("maps/{name}.txt"), format!("resource/overviews/{name}.txt")] {
            files.insert(f);
        }
        // Soundscapes der Map: alle "wave"-Einträge
        let ss = format!("scripts/soundscapes_{name}.txt");
        files.insert(ss.clone());
        if let Some(b) = fs.read(&ss) {
            let t = kv_tokens(&String::from_utf8_lossy(&b));
            for w in t.windows(2) {
                if w[0] == "wave" && (w[1].ends_with(".wav") || w[1].ends_with(".mp3") || w[1].ends_with(".ogg")) {
                    files.insert(sound_file(&w[1]));
                }
            }
        }
        // Übersichtskarte: "material" in resource/overviews/<map>.txt
        if let Some(b) = fs.read(&format!("resource/overviews/{name}.txt")) {
            let t = kv_tokens(&String::from_utf8_lossy(&b));
            for w in t.windows(2) {
                if w[0] == "material" {
                    mats.insert(crate::materials::normalize(&w[1]));
                }
            }
        }
        // Partikel: welche .pcf enthalten die benutzten Systeme?
        let mut wanted: BTreeSet<String> = ed.map.entities.iter().filter_map(|e| e.get("effect_name")).map(|s| s.to_ascii_lowercase()).filter(|s| !s.is_empty()).collect();
        if !wanted.is_empty() {
            let mut pcfs: BTreeSet<String> = BTreeSet::new();
            if let Some(b) = fs.read(&format!("maps/{name}_particles.txt")) {
                pcfs.extend(kv_tokens(&String::from_utf8_lossy(&b)).into_iter().filter(|t| t.ends_with(".pcf")).map(|t| t.trim_start_matches('!').replace('\\', "/")));
            }
            // nur eigene Partikel-Dateien durchsuchen (Spiel-Partikel sind ohnehin da)
            pcfs.extend(fs.list_files("particles/", &[".pcf"]).into_iter().filter(|f| !matches!(fs.source_of(f), Some(AssetSource::Game) | None)));
            let mut systems: Vec<(String, crate::pcf::ParticleSystem)> = Vec::new();
            for f in &pcfs {
                if let Some(list) = fs.read(f).and_then(|b| crate::pcf::parse(&b)) {
                    systems.extend(list.into_iter().map(|s| (f.clone(), s)));
                }
            }
            // Kinder mitnehmen
            loop {
                let more: Vec<String> = systems.iter().filter(|(_, s)| wanted.contains(&s.name.to_ascii_lowercase())).flat_map(|(_, s)| s.children.iter().map(|c| c.to_ascii_lowercase())).filter(|c| !wanted.contains(c)).collect();
                if more.is_empty() {
                    break;
                }
                wanted.extend(more);
            }
            for (f, s) in &systems {
                if wanted.contains(&s.name.to_ascii_lowercase()) {
                    files.insert(f.clone());
                    if let Some(m) = &s.material {
                        mats.insert(crate::materials::normalize(m));
                    }
                }
            }
        }
    }
    for m in &models {
        let base = m.trim_end_matches(".mdl");
        for ext in ["mdl", "vvd", "phy", "dx90.vtx", "dx80.vtx", "sw.vtx", "vtx"] {
            files.insert(format!("{base}.{ext}"));
        }
        if let Ok(model) = crate::mdl::load(fs, m) {
            mats.extend(model.materials.iter().cloned());
        }
    }
    for m in &mats {
        let vmt = format!("materials/{m}.vmt");
        files.insert(vmt.clone());
        let mut tex = BTreeSet::new();
        material_textures(fs, &vmt, 0, &mut tex);
        files.extend(tex);
    }
    files.into_iter().filter(|f| fs.exists(f)).collect()
}

/// Herkunft eines Pack-Kandidaten
#[derive(Debug, Clone, PartialEq)]
pub enum PackSource {
    /// lose im Spielordner – muss gepackt werden
    Loose(PathBuf),
    /// aus einem Addon (besser im Workshop verlinken)
    Addon(String),
    /// vom Nutzer hinzugefügt
    Extra(PathBuf),
}

/// Eine Datei, die in die BSP gepackt werden kann
#[derive(Debug, Clone)]
pub struct PackEntry {
    pub path: String,
    pub source: PackSource,
    pub size: u64,
    pub include: bool,
}

impl PackEntry {
    /// Inhalt der Datei
    pub fn read(&self, fs: Option<&GameFs>) -> Option<Vec<u8>> {
        match &self.source {
            PackSource::Loose(p) | PackSource::Extra(p) => std::fs::read(p).ok(),
            PackSource::Addon(_) => fs?.read(&self.path),
        }
    }
}

/// Datei mit Nutzer-Entscheidungen neben der VMF: `<map>_pack.txt`, Zeilen "+pfad=quelle" (hinzufügen/einschließen) bzw. "-pfad" (weglassen)
pub fn overrides_path(vmf: &Path) -> PathBuf {
    let stem = vmf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    vmf.with_file_name(format!("{stem}_pack.txt"))
}

/// Pfad im Spiel aus einem Dateipfad raten (alles ab materials/, models/, sound/ …)
pub fn game_path_of(file: &Path) -> Option<String> {
    let s = file.to_string_lossy().replace('\\', "/");
    let low = s.to_ascii_lowercase();
    ["materials/", "models/", "sound/", "particles/", "maps/", "scripts/", "resource/"]
        .iter()
        .filter_map(|r| low.rfind(&format!("/{r}")).map(|i| i + 1).or_else(|| low.starts_with(r).then_some(0)))
        .min()
        .map(|i| low[i..].to_string())
}

/// Kandidaten fürs Packen: eigene (lose) Dateien sind vorgewählt, Addon-Dateien nicht. Spiel-Dateien fehlen.
pub fn pack_candidates(ed: &Editor, fs: &GameFs, vmf: Option<&Path>) -> Vec<PackEntry> {
    let name = vmf.and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| ed.map_name());
    let mut out: Vec<PackEntry> = Vec::new();
    for f in collect(ed, fs, Some(&name)) {
        match fs.source_of(&f) {
            Some(AssetSource::Loose(p)) => {
                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                out.push(PackEntry { path: f, source: PackSource::Loose(p), size, include: true });
            }
            Some(AssetSource::Addon { name, file, .. }) => {
                let title = if name.is_empty() { file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { name };
                let size = fs.read(&f).map(|b| b.len() as u64).unwrap_or(0);
                out.push(PackEntry { path: f, source: PackSource::Addon(title), size, include: false });
            }
            _ => {}
        }
    }
    if let Some(text) = vmf.and_then(|v| std::fs::read_to_string(overrides_path(v)).ok()) {
        apply_overrides(&mut out, &text);
    }
    out
}

/// Nutzer-Entscheidungen anwenden
pub fn apply_overrides(list: &mut Vec<PackEntry>, text: &str) {
    for l in text.lines().map(str::trim) {
        if let Some(p) = l.strip_prefix('-') {
            let p = crate::bsppack::norm(p);
            if let Some(e) = list.iter_mut().find(|e| e.path == p) {
                e.include = false;
            }
        } else if let Some(rest) = l.strip_prefix('+') {
            let (p, src) = rest.split_once('=').map(|(a, b)| (a, Some(b))).unwrap_or((rest, None));
            let p = crate::bsppack::norm(p);
            match list.iter_mut().find(|e| e.path == p) {
                Some(e) => e.include = true,
                None => {
                    if let Some(src) = src.map(PathBuf::from).filter(|s| s.is_file()) {
                        let size = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
                        list.push(PackEntry { path: p, source: PackSource::Extra(src), size, include: true });
                    }
                }
            }
        }
    }
}

/// Entscheidungen speichern (nur Abweichungen vom Standard)
pub fn overrides_text(list: &[PackEntry]) -> String {
    let mut out = String::from("// Amboss: Auswahl fürs Packen (+ einpacken, - weglassen)\n");
    for e in list {
        match (&e.source, e.include) {
            (PackSource::Loose(_), false) => out.push_str(&format!("-{}\n", e.path)),
            (PackSource::Addon(_), true) => out.push_str(&format!("+{}\n", e.path)),
            (PackSource::Extra(p), true) => out.push_str(&format!("+{}={}\n", e.path, p.display())),
            _ => {}
        }
    }
    out
}

/// Packliste fürs Kompilieren schreiben (`<compile_vmf>.packlist.txt`, bspzip-Format).
/// Ausgewählte Addon-Dateien werden dafür nach `amboss_pack/` ausgepackt. Liefert Zeilen fürs Log.
pub fn write_packlist(ed: &Editor, fs: &GameFs, vmf: &Path, compile_vmf: &Path) -> Vec<String> {
    let list = compile_vmf.with_extension("packlist.txt");
    let _ = std::fs::remove_file(&list);
    let entries: Vec<PackEntry> = pack_candidates(ed, fs, Some(vmf)).into_iter().filter(|e| e.include).collect();
    if entries.is_empty() {
        return vec!["Inhalte packen: keine eigenen Dateien gefunden".into()];
    }
    let tmp = compile_vmf.parent().unwrap_or(Path::new(".")).join("amboss_pack");
    let mut text = String::new();
    let mut bytes = 0;
    for e in &entries {
        let abs = match &e.source {
            PackSource::Loose(p) | PackSource::Extra(p) => p.clone(),
            PackSource::Addon(_) => {
                let out = tmp.join(&e.path);
                let Some(b) = fs.read(&e.path) else { continue };
                let _ = std::fs::create_dir_all(out.parent().unwrap_or(&tmp));
                if std::fs::write(&out, b).is_err() {
                    continue;
                }
                out
            }
        };
        bytes += e.size;
        text.push_str(&format!("{}\n{}\n", e.path, abs.display()));
    }
    match std::fs::write(&list, text) {
        Ok(()) => vec![format!("Inhalte packen: {} Datei(en), {:.1} MB werden nach VRAD in die BSP gepackt (Auswahl: Kompilieren → Inhalte in BSP packen…)", entries.len(), bytes as f64 / 1048576.0)],
        Err(e) => vec![format!("Packliste konnte nicht geschrieben werden: {e}")],
    }
}

/// Ein Addon, aus dem die Map Inhalte benutzt
#[derive(Debug, Clone, PartialEq)]
pub struct AddonUse {
    pub name: String,
    pub workshop_id: Option<u64>,
    pub file: PathBuf,
    pub files: Vec<String>,
}

impl AddonUse {
    pub fn url(&self) -> Option<String> {
        self.workshop_id.map(|id| format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))
    }
}

/// Benutzte Dateien nach Herkunft: (Addons, Anzahl lose Dateien)
pub fn addon_report(ed: &Editor, fs: &GameFs) -> (Vec<AddonUse>, usize) {
    let mut by: BTreeMap<PathBuf, AddonUse> = BTreeMap::new();
    let mut loose = 0;
    for f in used_files(ed, fs) {
        match fs.source_of(&f) {
            Some(AssetSource::Addon { name, workshop_id, file }) => {
                by.entry(file.clone()).or_insert_with(|| AddonUse { name, workshop_id, file, files: Vec::new() }).files.push(f);
            }
            Some(AssetSource::Loose(_)) => loose += 1,
            _ => {}
        }
    }
    let mut v: Vec<AddonUse> = by.into_values().collect();
    v.sort_by(|a, b| b.files.len().cmp(&a.files.len()).then(a.name.cmp(&b.name)));
    (v, loose)
}

/// Text der Addon-Liste (für die Datei neben der VMF)
pub fn addon_text(map_name: &str, addons: &[AddonUse], loose: usize) -> String {
    let tr = |s: &str| crate::i18n::tr_dyn(s);
    let mut t = tr(&format!("Benötigte Addons für die Map \"{map_name}\" (erstellt von Amboss)\n"));
    t.push_str(&tr("Beim Hochladen in den Steam-Workshop unter \"Benötigte Objekte\" / \"Required Items\" verlinken,\n"));
    t.push_str(&tr("oder die Inhalte selbst in die BSP packen.\n\n"));
    if addons.is_empty() {
        t.push_str(&tr("Keine Inhalte aus Addons – die Map braucht nur das Spiel"));
        t.push_str(&tr(if loose > 0 { " und eigene, lose Dateien.\n" } else { ".\n" }));
    }
    for (i, a) in addons.iter().enumerate() {
        let title = if a.name.is_empty() { a.file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() } else { a.name.clone() };
        t.push_str(&format!("{}. {title}\n", i + 1));
        match a.url() {
            Some(u) => t.push_str(&format!("   {u}\n")),
            None => t.push_str(&tr(&format!("   (keine Workshop-ID gefunden – lokale Datei: {})\n", a.file.display()))),
        }
        t.push_str(&tr(&format!("   {} Datei(en), z. B.:\n", a.files.len())));
        for f in a.files.iter().take(8) {
            t.push_str(&format!("     {f}\n"));
        }
        if a.files.len() > 8 {
            t.push_str(&tr(&format!("     … und {} weitere\n", a.files.len() - 8)));
        }
        t.push('\n');
    }
    if loose > 0 {
        t.push_str(&tr(&format!("Außerdem {loose} eigene Datei(en), die lose im Spielordner liegen (nicht in einem Addon) – mit \"Eigene Inhalte in die BSP packen\" einpacken.\n")));
    }
    t
}

/// Addon-Liste erzeugen, als `<map>_addons.txt` neben `vmf` speichern und Zeilen fürs Compile-Log liefern
pub fn write_addon_list(ed: &Editor, fs: &GameFs, vmf: &Path) -> Vec<String> {
    let (addons, loose) = addon_report(ed, fs);
    let name = ed.map_name();
    let out = vmf.with_file_name(format!("{name}_addons.txt"));
    let mut lines = Vec::new();
    match std::fs::write(&out, addon_text(&name, &addons, loose)) {
        Ok(()) if addons.is_empty() => lines.push(format!("Addons: keine benötigt (Liste: {})", out.display())),
        Ok(()) => {
            lines.push(format!("Addons: die Map benutzt Inhalte aus {} Addon(s) – Liste mit Workshop-Links: {}", addons.len(), out.display()));
            for a in &addons {
                let title = if a.name.is_empty() { a.file.display().to_string() } else { a.name.clone() };
                lines.push(format!("  • {title} ({} Datei(en)){}", a.files.len(), a.url().map(|u| format!(" – {u}")).unwrap_or_default()));
            }
        }
        Err(e) => lines.push(format!("Addon-Liste konnte nicht gespeichert werden: {e}")),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;
    use crate::geometry::Aabb;
    use glam::DVec3;

    fn write_gma(path: &Path, name: &str, files: &[(&str, &[u8])]) {
        let mut b = b"GMAD".to_vec();
        b.push(3);
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.push(0); // keine required content
        for s in [name, "Beschreibung", "Autor"] {
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        b.extend_from_slice(&1i32.to_le_bytes());
        for (i, (n, d)) in files.iter().enumerate() {
            b.extend_from_slice(&(i as u32 + 1).to_le_bytes());
            b.extend_from_slice(n.as_bytes());
            b.push(0);
            b.extend_from_slice(&(d.len() as u64).to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        for (_, d) in files {
            b.extend_from_slice(d);
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn pack_candidates_full() {
        let dir = std::env::temp_dir().join(format!("amboss_pack_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gd = dir.join("garrysmod");
        let w = |p: &str, d: &[u8]| {
            let f = gd.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, d).unwrap();
        };
        w("materials/eigen/wand.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"eigen/wand\" \"$bumpmap\" \"eigen/wand_n\" }");
        w("materials/eigen/wand.vtf", b"t");
        w("materials/eigen/wand_n.vtf", b"n");
        w("maps/testmap.nav", b"nav");
        w("scripts/soundscapes_testmap.txt", b"\"testmap.wind\" { \"playlooping\" { \"wave\" \")ambient/eigen/wind.wav\" } }");
        w("sound/ambient/eigen/wind.wav", b"RIFF");
        w("particles/eigen.pcf", &crate::pcf::tests::sample_pcf());
        w("materials/particle/funke.vmt", b"\"SpriteCard\" { \"$basetexture\" \"particle/funke\" }");
        w("materials/particle/funke.vtf", b"f");
        let fs = GameFs::for_gmod(&gd);
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "eigen/wand".into();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let mut e = crate::map::Entity::new(ed.map.alloc_id(), "info_particle_system");
        e.set("effect_name", "rauch");
        ed.map.entities.push(e.into());
        let vmf = dir.join("testmap.vmf");
        let list = pack_candidates(&ed, &fs, Some(&vmf));
        let paths: Vec<&str> = list.iter().map(|e| e.path.as_str()).collect();
        for p in ["materials/eigen/wand.vmt", "materials/eigen/wand_n.vtf", "maps/testmap.nav", "scripts/soundscapes_testmap.txt", "sound/ambient/eigen/wind.wav", "particles/eigen.pcf", "materials/particle/funke.vtf"] {
            assert!(paths.contains(&p), "{p} fehlt in {paths:?}");
        }
        assert!(list.iter().all(|e| e.include));
        // Auswahl speichern und wieder anwenden
        let mut l2 = list.clone();
        l2.iter_mut().find(|e| e.path == "maps/testmap.nav").unwrap().include = false;
        let extra = dir.join("extra/materials/x/y.vmt");
        std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
        std::fs::write(&extra, "x").unwrap();
        assert_eq!(game_path_of(&extra).as_deref(), Some("materials/x/y.vmt"));
        l2.push(PackEntry { path: "materials/x/y.vmt".into(), source: PackSource::Extra(extra), size: 1, include: true });
        std::fs::write(overrides_path(&vmf), overrides_text(&l2)).unwrap();
        let l3 = pack_candidates(&ed, &fs, Some(&vmf));
        assert!(!l3.iter().find(|e| e.path == "maps/testmap.nav").unwrap().include);
        assert!(l3.iter().any(|e| e.path == "materials/x/y.vmt" && e.include));
        // Packliste fürs Kompilieren
        let lines = write_packlist(&ed, &fs, &vmf, &vmf);
        assert!(lines[0].contains("Datei(en)"), "{lines:?}");
        let pl = crate::bsppack::read_packlist(&std::fs::read_to_string(vmf.with_extension("packlist.txt")).unwrap());
        assert_eq!(pl.len(), l3.iter().filter(|e| e.include).count());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn addon_list_from_gma() {
        let dir = std::env::temp_dir().join(format!("amboss_addons_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gd = dir.join("steamapps/common/GarrysMod/garrysmod");
        std::fs::create_dir_all(gd.join("materials/eigen")).unwrap();
        std::fs::write(gd.join("materials/eigen/wand.vmt"), "\"LightmappedGeneric\" { \"$basetexture\" \"eigen/wand\" }").unwrap();
        let ws = dir.join("steamapps/workshop/content/4000/123456789/addon.gma");
        write_gma(&ws, "Tolle Texturen", &[("materials/ws/boden.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"ws/boden\" }"), ("materials/ws/boden.vtf", b"x"), ("models/ws/kiste.mdl", b"y")]);
        let fs = GameFs::for_gmod(&gd);
        assert_eq!(fs.gmas.len(), 1);
        assert_eq!(fs.gmas[0].name, "Tolle Texturen");
        assert_eq!(fs.gmas[0].workshop_id, Some(123456789));
        let mut ed = Editor::new(Fgd::builtin());
        ed.material = "WS/BODEN".into();
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        ed.material = "eigen/wand".into();
        ed.create_block(Aabb::new(DVec3::splat(100.0), DVec3::splat(164.0)));
        let mut e = crate::map::Entity::new(ed.map.alloc_id(), "prop_static");
        e.set("model", "models/ws/kiste.mdl");
        ed.map.entities.push(e.into());
        let (addons, loose) = addon_report(&ed, &fs);
        assert_eq!(addons.len(), 1);
        assert_eq!(addons[0].workshop_id, Some(123456789));
        assert!(addons[0].files.contains(&"materials/ws/boden.vtf".to_string()));
        assert!(addons[0].files.contains(&"models/ws/kiste.mdl".to_string()));
        assert_eq!(loose, 1, "eigene lose Datei");
        let txt = addon_text("test", &addons, loose);
        assert!(txt.contains("https://steamcommunity.com/sharedfiles/filedetails/?id=123456789") && txt.contains("Tolle Texturen"));
        // ohne Addons: GMA wird nicht geladen
        let fs2 = GameFs::for_gmod_opts(&gd, false);
        assert!(fs2.gmas.is_empty());
        assert!(addon_report(&ed, &fs2).0.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
