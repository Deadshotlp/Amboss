//! Spiel-Dateisystem: VPK-Archive + lose Ordner (wie die Source-Engine sie einbindet).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const VPK_SIG: u32 = 0x55aa1234;
const IN_DIR: u16 = 0x7fff;

#[derive(Debug, Clone)]
struct VpkEntry {
    archive: u16,
    offset: u32,
    length: u32,
    preload: Vec<u8>,
}

#[derive(Debug)]
pub struct Vpk {
    dir_path: PathBuf,
    data_start: u64,
    entries: HashMap<String, VpkEntry>,
}

fn rd_u16(b: &[u8], p: &mut usize) -> Option<u16> {
    let v = u16::from_le_bytes(b.get(*p..*p + 2)?.try_into().ok()?);
    *p += 2;
    Some(v)
}

fn rd_u32(b: &[u8], p: &mut usize) -> Option<u32> {
    let v = u32::from_le_bytes(b.get(*p..*p + 4)?.try_into().ok()?);
    *p += 4;
    Some(v)
}

fn rd_cstr(b: &[u8], p: &mut usize) -> Option<String> {
    let end = b[*p..].iter().position(|&c| c == 0)? + *p;
    let s = String::from_utf8_lossy(&b[*p..end]).to_string();
    *p = end + 1;
    Some(s)
}

impl Vpk {
    pub fn open(dir_path: &Path) -> Result<Vpk, String> {
        let data = std::fs::read(dir_path).map_err(|e| format!("{}: {e}", dir_path.display()))?;
        Self::parse(dir_path.to_path_buf(), &data).ok_or_else(|| format!("{}: ungültige VPK", dir_path.display()))
    }

    fn parse(dir_path: PathBuf, b: &[u8]) -> Option<Vpk> {
        let mut p = 0;
        if rd_u32(b, &mut p)? != VPK_SIG {
            return None;
        }
        let version = rd_u32(b, &mut p)?;
        let tree_size = rd_u32(b, &mut p)?;
        if version == 2 {
            p += 16;
        } else if version != 1 {
            return None;
        }
        let header = p;
        let mut entries = HashMap::new();
        loop {
            let ext = rd_cstr(b, &mut p)?;
            if ext.is_empty() {
                break;
            }
            loop {
                let path = rd_cstr(b, &mut p)?;
                if path.is_empty() {
                    break;
                }
                loop {
                    let name = rd_cstr(b, &mut p)?;
                    if name.is_empty() {
                        break;
                    }
                    let _crc = rd_u32(b, &mut p)?;
                    let preload_len = rd_u16(b, &mut p)? as usize;
                    let archive = rd_u16(b, &mut p)?;
                    let offset = rd_u32(b, &mut p)?;
                    let length = rd_u32(b, &mut p)?;
                    let _term = rd_u16(b, &mut p)?;
                    let preload = b.get(p..p + preload_len)?.to_vec();
                    p += preload_len;
                    let mut full = String::new();
                    if path != " " {
                        full.push_str(&path);
                        full.push('/');
                    }
                    full.push_str(&name);
                    if ext != " " {
                        full.push('.');
                        full.push_str(&ext);
                    }
                    entries.insert(full.to_ascii_lowercase(), VpkEntry { archive, offset, length, preload });
                }
            }
        }
        Some(Vpk { dir_path, data_start: (header as u64) + tree_size as u64, entries })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let e = self.entries.get(path)?;
        let mut out = e.preload.clone();
        if e.length > 0 {
            let (file, offset) = if e.archive == IN_DIR {
                (self.dir_path.clone(), self.data_start + e.offset as u64)
            } else {
                let s = self.dir_path.to_string_lossy();
                let base = s.strip_suffix("_dir.vpk").unwrap_or(&s);
                (PathBuf::from(format!("{base}_{:03}.vpk", e.archive)), e.offset as u64)
            };
            let mut f = File::open(file).ok()?;
            f.seek(SeekFrom::Start(offset)).ok()?;
            let start = out.len();
            out.resize(start + e.length as usize, 0);
            f.read_exact(&mut out[start..]).ok()?;
        }
        Some(out)
    }

    pub fn files(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

#[derive(Default)]
pub struct GameFs {
    pub dirs: Vec<PathBuf>,
    pub vpks: Vec<Vpk>,
    pub gmas: Vec<Gma>,
    pub messages: Vec<String>,
    /// lose Addon-Ordner (garrysmod/addons/<name>) – gehören nicht zum Spiel selbst
    pub addon_dirs: Vec<PathBuf>,
}

/// Woher eine Datei stammt
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetSource {
    /// Spiel selbst (VPKs, garrysmod-Ordner, eingebundene Spiele)
    Game,
    /// lose im Spielordner (eigene Inhalte, die gepackt werden müssen)
    Loose(PathBuf),
    /// Addon (Workshop-.gma oder addons/<name>-Ordner)
    Addon { name: String, workshop_id: Option<u64>, file: PathBuf },
}

/// Garry's-Mod-Addon-Archiv (.gma, z. B. Workshop-Inhalte)
#[derive(Debug)]
pub struct Gma {
    path: PathBuf,
    entries: HashMap<String, (u64, u64)>,
    /// Titel aus dem GMA-Kopf
    pub name: String,
    /// Workshop-ID aus dem Pfad (…/workshop/content/4000/<id>/…, cache/workshop/<id>.gma, name_<id>.gma)
    pub workshop_id: Option<u64>,
}

impl Gma {
    /// Liest nur den Kopf mit dem Inhaltsverzeichnis (nicht die ganze, oft riesige Datei)
    pub fn open(path: &Path) -> Result<Gma, String> {
        let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let total = f.metadata().map(|m| m.len()).unwrap_or(0);
        let mut buf = Vec::new();
        let mut want: u64 = 256 * 1024;
        loop {
            let target = want.min(total) as usize;
            if buf.len() < target {
                let start = buf.len();
                buf.resize(target, 0);
                f.read_exact(&mut buf[start..]).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            if let Some(mut g) = Self::parse(path.to_path_buf(), &buf) {
                g.workshop_id = workshop_id_of(path);
                return Ok(g);
            }
            if target as u64 >= total {
                return Err(format!("{}: ungültige GMA", path.display()));
            }
            want *= 4;
        }
    }

    fn parse(path: PathBuf, b: &[u8]) -> Option<Gma> {
        if b.get(0..4)? != b"GMAD" {
            return None;
        }
        let version = *b.get(4)?;
        let mut p = 5 + 8 + 8; // Version, SteamID, Zeitstempel
        if version > 1 {
            while rd_cstr(b, &mut p)? != "" {}
        }
        let name = rd_cstr(b, &mut p)?.to_string();
        for _ in 0..2 {
            rd_cstr(b, &mut p)?; // Beschreibung, Autor
        }
        p += 4; // Addon-Version
        let mut files: Vec<(String, u64)> = Vec::new();
        loop {
            let num = rd_u32(b, &mut p)?;
            if num == 0 {
                break;
            }
            let name = rd_cstr(b, &mut p)?.replace('\\', "/").to_ascii_lowercase();
            let size = u64::from_le_bytes(b.get(p..p + 8)?.try_into().ok()?);
            p += 8 + 4; // Größe + CRC
            files.push((name, size));
        }
        let mut offset = p as u64;
        let mut entries = HashMap::new();
        for (name, size) in files {
            entries.insert(name, (offset, size));
            offset += size;
        }
        Some(Gma { path, entries, name, workshop_id: None })
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let (off, size) = *self.entries.get(path)?;
        let mut f = File::open(&self.path).ok()?;
        f.seek(SeekFrom::Start(off)).ok()?;
        let mut out = vec![0u8; size as usize];
        f.read_exact(&mut out).ok()?;
        Some(out)
    }
}

/// Workshop-ID aus dem Dateipfad einer .gma
pub fn workshop_id_of(path: &Path) -> Option<u64> {
    let comps: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    // …/workshop/content/4000/<id>/datei.gma
    if let Some(i) = comps.iter().position(|c| c == "4000") {
        if let Some(id) = comps.get(i + 1).and_then(|c| c.parse::<u64>().ok()) {
            return Some(id);
        }
    }
    // cache/workshop/<id>.gma oder name_<id>.gma
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let digits: String = stem.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    if digits.len() >= 6 {
        return digits.parse().ok();
    }
    None
}

impl GameFs {
    /// Woher stammt die Datei (gleiche Reihenfolge wie `read`)?
    pub fn source_of(&self, path: &str) -> Option<AssetSource> {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        for d in &self.dirs {
            let f = d.join(&p);
            if f.is_file() || read_case_insensitive_exists(d, &p) {
                if let Some(ad) = self.addon_dirs.iter().find(|a| *a == d) {
                    let name = ad.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    return Some(AssetSource::Addon { workshop_id: workshop_id_of(ad), name, file: ad.clone() });
                }
                return Some(if self.vpks.iter().any(|v| v.entries.contains_key(&p)) { AssetSource::Game } else { AssetSource::Loose(f) });
            }
        }
        if self.vpks.iter().any(|v| v.entries.contains_key(&p)) {
            return Some(AssetSource::Game);
        }
        self.gmas.iter().find(|g| g.entries.contains_key(&p)).map(|g| AssetSource::Addon { name: g.name.clone(), workshop_id: g.workshop_id, file: g.path.clone() })
    }

    /// Baut das Dateisystem für Garry's Mod auf (garrysmod-Ordner, HL2-VPKs, Addons, mount.cfg).
    pub fn for_gmod(game_dir: &Path) -> GameFs {
        Self::for_gmod_opts(game_dir, true)
    }

    /// `addons = false`: nur Spiel-VPKs und lose Ordner (schnell), ohne Workshop-/Addon-Archive
    pub fn for_gmod_opts(game_dir: &Path, addons: bool) -> GameFs {
        let mut fs = GameFs::default();
        if !game_dir.is_dir() {
            fs.messages.push(format!("Spielordner nicht gefunden: {}", game_dir.display()));
            return fs;
        }
        let root = game_dir.parent().map(Path::to_path_buf).unwrap_or_default();
        fs.add_dir(game_dir);
        // alle VPKs im Spielordner (garrysmod_dir.vpk, hl2_textures_dir.vpk, … – auch für andere Source-Spiele)
        if let Ok(rd) = std::fs::read_dir(game_dir) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().to_ascii_lowercase().ends_with("_dir.vpk")).collect();
            v.sort();
            for p in v {
                fs.add_vpk(&p);
            }
        }
        // Workshop-/Addon-Archive (.gma)
        let mut gma_dirs = vec![game_dir.join("addons"), game_dir.join("cache/workshop")];
        if let Some(steamapps) = game_dir.ancestors().find(|a| a.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steamapps"))) {
            gma_dirs.push(steamapps.join("workshop/content/4000"));
        }
        for d in gma_dirs.into_iter().filter(|_| addons) {
            walk(&d, &mut |p| {
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gma")) {
                    match Gma::open(p) {
                        Ok(g) => fs.gmas.push(g),
                        Err(e) => fs.messages.push(e),
                    }
                }
            });
        }
        // Legacy-Addons (garrysmod/addons/<name>/materials)
        if let Ok(rd) = std::fs::read_dir(game_dir.join("addons")).map_err(|_| ()).and_then(|rd| if addons { Ok(rd) } else { Err(()) }) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    fs.add_dir(&e.path());
                    fs.addon_dirs.push(e.path());
                }
            }
        }
        fs.add_dir(&game_dir.join("download"));
        let se = root.join("sourceengine");
        for v in ["hl2_textures_dir.vpk", "hl2_misc_dir.vpk"] {
            fs.add_vpk(&se.join(v));
        }
        // Andere Source-Spiele (z. B. Source SDK Base 2013): Basis-Ordner "hl2" mit einbinden
        let hl2 = root.join("hl2");
        if hl2.is_dir() && hl2 != game_dir {
            fs.add_dir(&hl2);
            if let Ok(rd) = std::fs::read_dir(&hl2) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().to_ascii_lowercase().ends_with("_dir.vpk") {
                        fs.add_vpk(&e.path());
                    }
                }
            }
        }
        // Eingebundene Spiele (cfg/mount.cfg), z. B. Counter-Strike: Source
        if let Ok(src) = std::fs::read_to_string(game_dir.join("cfg/mount.cfg")) {
            if let Ok(blocks) = crate::keyvalues::parse(&src) {
                for b in blocks {
                    for (_, path) in b.pairs() {
                        let p = PathBuf::from(path);
                        if p.is_dir() {
                            fs.add_dir(&p);
                            if let Ok(rd) = std::fs::read_dir(&p) {
                                for e in rd.flatten() {
                                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                                    if n.ends_with("_dir.vpk") {
                                        fs.add_vpk(&e.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        fs
    }

    pub fn add_dir(&mut self, p: &Path) {
        if p.is_dir() {
            self.dirs.push(p.to_path_buf());
        }
    }

    pub fn add_vpk(&mut self, p: &Path) {
        if !p.is_file() {
            return;
        }
        match Vpk::open(p) {
            Ok(v) => self.vpks.push(v),
            Err(e) => self.messages.push(e),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && self.vpks.is_empty() && self.gmas.is_empty()
    }

    /// Datei, die nur lose im Spielordner liegt (nicht in VPK/GMA) → eigener Inhalt, der gepackt werden muss
    #[allow(dead_code)]
    pub fn loose_only(&self, path: &str) -> Option<PathBuf> {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        if self.vpks.iter().any(|v| v.entries.contains_key(&p)) || self.gmas.iter().any(|g| g.entries.contains_key(&p)) {
            return None;
        }
        self.dirs.iter().map(|d| d.join(&p)).find(|f| f.is_file())
    }

    /// Alle Dateien in Archiven (VPK + GMA)
    pub fn archive_files(&self) -> impl Iterator<Item = &String> {
        self.vpks.iter().flat_map(|v| v.files()).chain(self.gmas.iter().flat_map(|g| g.entries.keys()))
    }

    /// Datei lesen (Pfad relativ, z. B. "materials/dev/graygrid.vmt")
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        for d in &self.dirs {
            let f = d.join(&p);
            if let Ok(b) = std::fs::read(&f) {
                return Some(b);
            }
            // Groß-/Kleinschreibung auf Linux tolerieren
            if let Some(b) = read_case_insensitive(d, &p) {
                return Some(b);
            }
        }
        self.vpks.iter().find_map(|v| v.read(&p)).or_else(|| self.gmas.iter().find_map(|g| g.read(&p)))
    }

    pub fn exists(&self, path: &str) -> bool {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        self.vpks.iter().any(|v| v.entries.contains_key(&p)) || self.gmas.iter().any(|g| g.entries.contains_key(&p)) || self.dirs.iter().any(|d| d.join(&p).is_file())
    }

    /// Alle Dateien unter einem Ordner mit einer der Endungen (relativ, kleingeschrieben)
    pub fn list_files(&self, prefix: &str, exts: &[&str]) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        let ok = |f: &str| exts.iter().any(|e| f.ends_with(e));
        for f in self.archive_files() {
            if f.starts_with(prefix) && ok(f) {
                set.insert(f.clone());
            }
        }
        for d in &self.dirs {
            let base = d.join(prefix);
            walk(&base, &mut |p| {
                if let Ok(rel) = p.strip_prefix(d) {
                    let s = rel.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
                    if ok(&s) {
                        set.insert(s);
                    }
                }
            });
        }
        set.into_iter().collect()
    }

    /// Alle Materialnamen (ohne "materials/" und ".vmt"), sortiert
    pub fn list_materials(&self) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for f in self.archive_files() {
            if let Some(m) = f.strip_prefix("materials/").and_then(|s| s.strip_suffix(".vmt")) {
                set.insert(m.to_string());
            }
        }
        for d in &self.dirs {
            let base = d.join("materials");
            walk(&base, &mut |p| {
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmt")) {
                    if let Ok(rel) = p.strip_prefix(&base) {
                        let s = rel.with_extension("").to_string_lossy().replace('\\', "/").to_ascii_lowercase();
                        set.insert(s);
                    }
                }
            });
        }
        set.into_iter().collect()
    }
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else {
            f(&p);
        }
    }
}

#[cfg(windows)]
fn read_case_insensitive(_d: &Path, _p: &str) -> Option<Vec<u8>> {
    None
}

#[cfg(not(windows))]
fn read_case_insensitive(d: &Path, p: &str) -> Option<Vec<u8>> {
    let mut cur = d.to_path_buf();
    for part in p.split('/') {
        let rd = std::fs::read_dir(&cur).ok()?;
        let found = rd.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = found.path();
    }
    std::fs::read(cur).ok()
}

fn read_case_insensitive_exists(d: &Path, p: &str) -> bool {
    if cfg!(windows) {
        return false;
    }
    let mut cur = d.to_path_buf();
    for part in p.split('/') {
        let Ok(rd) = std::fs::read_dir(&cur) else { return false };
        match rd.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part)) {
            Some(f) => cur = f.path(),
            None => return false,
        }
    }
    cur.is_file()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Schreibt eine minimale VPK v2 (alle Daten in der _dir-Datei)
    pub fn write_test_vpk(path: &Path, files: &[(&str, &[u8])]) {
        let mut tree = Vec::new();
        let mut data = Vec::new();
        // nach Endung / Pfad gruppieren (hier: jede Datei einzeln)
        for (full, content) in files {
            let (dir, file) = full.rsplit_once('/').unwrap_or((" ", full));
            let (name, ext) = file.rsplit_once('.').unwrap();
            for s in [ext, dir, name] {
                tree.extend_from_slice(s.as_bytes());
                tree.push(0);
            }
            tree.extend_from_slice(&0u32.to_le_bytes()); // crc
            tree.extend_from_slice(&2u16.to_le_bytes()); // preload = 2 Bytes
            tree.extend_from_slice(&IN_DIR.to_le_bytes());
            tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
            tree.extend_from_slice(&((content.len() - 2) as u32).to_le_bytes());
            tree.extend_from_slice(&0xffffu16.to_le_bytes());
            tree.extend_from_slice(&content[..2]);
            data.extend_from_slice(&content[2..]);
            tree.extend_from_slice(&[0, 0]); // Ende der Namen- und Pfadliste
        }
        tree.push(0);
        let mut out = Vec::new();
        out.extend_from_slice(&VPK_SIG.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&tree);
        out.extend_from_slice(&data);
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn vpk_roundtrip() {
        let dir = std::env::temp_dir().join(format!("amboss_vpk_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("test_dir.vpk");
        write_test_vpk(&p, &[("materials/dev/a.vmt", b"hello world"), ("materials/b.vtf", b"xyz123")]);
        let v = Vpk::open(&p).unwrap();
        assert_eq!(v.read("materials/dev/a.vmt").unwrap(), b"hello world");
        assert_eq!(v.read("materials/b.vtf").unwrap(), b"xyz123");
        let mut fs = GameFs::default();
        fs.vpks.push(v);
        assert_eq!(fs.list_materials(), vec!["dev/a".to_string()]);
        assert!(fs.read("MATERIALS\\DEV\\A.VMT").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gma_parse() {
        let mut b = Vec::new();
        b.extend_from_slice(b"GMAD");
        b.push(3);
        b.extend_from_slice(&[0u8; 16]);
        b.push(0); // keine required content
        for s in ["Mein Addon", "{}", "Autor"] {
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        b.extend_from_slice(&1i32.to_le_bytes());
        let content = b"VMT-INHALT";
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(b"materials/test/a.vmt\0");
        b.extend_from_slice(&(content.len() as u64).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(content);
        let dir = std::env::temp_dir().join(format!("amboss_gma_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.gma");
        std::fs::write(&p, &b).unwrap();
        let g = Gma::open(&p).unwrap();
        assert_eq!(g.read("materials/test/a.vmt").unwrap(), content);
        let mut fs = GameFs::default();
        fs.gmas.push(g);
        assert_eq!(fs.list_materials(), vec!["test/a".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
