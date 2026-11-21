//! Updates: beim Start nachsehen, ob es auf GitHub eine neuere Version gibt (abschaltbar).
//! Auf Wunsch wird das Paket geladen und die Programmdatei ausgetauscht – aktiv ab dem
//! nächsten Start. Die laufende Datei lässt sich unter Windows nicht überschreiben, aber
//! umbenennen: amboss.exe → amboss.old.exe, neue Datei an ihre Stelle, alte beim nächsten
//! Start löschen.
//!
//! Für HTTPS wird das mitgelieferte `curl` des Systems genutzt (Windows 10/11, macOS, Linux),
//! damit Amboss keine eigene TLS-Bibliothek braucht. Es werden nur die öffentlichen
//! Release-Infos abgerufen; Amboss sendet dabei keine Daten über dich oder deine Maps.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const REPO: &str = "Deadshotlp/Amboss";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// Paket für dieses Betriebssystem (None: nur Seite öffnen)
    pub asset: Option<(String, String)>,
}

/// Versionsnummern vergleichen ("v0.10.2" > "0.9.10")
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect() };
    let (x, y) = (parse(a), parse(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

fn os_tag() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn curl() -> Command {
    let mut c = Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // kein Konsolenfenster
    }
    c.args(["-fsSL", "--retry", "2", "--connect-timeout", "10", "-A", "Amboss-Updater"]);
    c
}

/// Datei aus dem Netz laden (z. B. Textur-Bibliotheken); eigener Programmname als User-Agent
pub fn http_get(url: &str, max_secs: u32) -> Result<Vec<u8>, String> {
    let out = curl().args(["-A", "Amboss-Map-Editor", "--max-time", &max_secs.to_string(), url]).output().map_err(|e| format!("curl nicht gefunden ({e})"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "Keine Verbindung".into() } else { err });
    }
    Ok(out.stdout)
}

/// Namen aller Dateien in einem ZIP (ohne Ordner)
pub fn zip_names(zip: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(eocd) = (0..zip.len().saturating_sub(21)).rev().find(|&i| zip[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])) else { return out };
    let (Some(count), Some(mut p)) = (u16le(zip, eocd + 10), u32le(zip, eocd + 16)) else { return out };
    for _ in 0..count {
        if !zip.get(p..).is_some_and(|s| s.starts_with(&[0x50, 0x4b, 0x01, 0x02])) {
            break;
        }
        let (nlen, xlen, clen) = (u16le(zip, p + 28).unwrap_or(0), u16le(zip, p + 30).unwrap_or(0), u16le(zip, p + 32).unwrap_or(0));
        let Some(name) = zip.get(p + 46..p + 46 + nlen) else { break };
        let name = String::from_utf8_lossy(name).to_string();
        if let Some(n) = name.rsplit('/').next().filter(|n| !n.is_empty()) {
            out.push(n.to_string());
        }
        p += 46 + nlen + xlen + clen;
    }
    out
}

/// Seite laden (Text)
fn get(url: &str, max_secs: u32) -> Result<Vec<u8>, String> {
    let out = curl().args(["--max-time", &max_secs.to_string(), url]).output().map_err(|e| format!("curl nicht gefunden ({e})"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "Keine Verbindung zu GitHub".into() } else { err });
    }
    Ok(out.stdout)
}

/// Neueste Version aus der GitHub-Antwort lesen
pub fn parse_release(json: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let notes = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();
    let page = v.get("html_url").and_then(|b| b.as_str()).unwrap_or("").to_string();
    let os = os_tag();
    let asset = v.get("assets").and_then(|a| a.as_array()).and_then(|list| {
        list.iter().find_map(|a| {
            let name = a.get("name")?.as_str()?.to_ascii_lowercase();
            let url = a.get("browser_download_url")?.as_str()?;
            (name.contains(os) && name.ends_with(".zip") && !name.contains("quell") && !name.contains("source")).then(|| (name, url.to_string()))
        })
    });
    Some(Release { version: tag.trim_start_matches(['v', 'V']).to_string(), notes, page, asset })
}

/// Liste der Releases (Version, Text) – die eigene Überschrift „## Amboss x.y.z“ wird entfernt
pub fn parse_release_list(json: &str) -> Option<Vec<(String, String)>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Some(
        v.as_array()?
            .iter()
            .filter_map(|r| {
                let tag = r.get("tag_name")?.as_str()?.trim_start_matches(['v', 'V']).to_string();
                let body = r.get("body").and_then(|b| b.as_str()).unwrap_or("").replace("\r\n", "\n");
                let body = body.lines().skip_while(|l| l.trim().is_empty() || (l.starts_with("## ") && l.contains(&tag))).collect::<Vec<_>>().join("\n");
                Some((tag, body.trim().to_string()))
            })
            .collect(),
    )
}

/// Release-Texte von GitHub (für „Neu in Amboss“ in anderen Sprachen als Deutsch)
pub fn release_notes() -> Result<Vec<(String, String)>, String> {
    let body = get(&format!("https://api.github.com/repos/{REPO}/releases?per_page=50"), 15)?;
    parse_release_list(&String::from_utf8_lossy(&body)).ok_or_else(|| "Antwort von GitHub nicht lesbar".to_string())
}

/// Gibt es eine neuere Version? (blockiert einige Sekunden – im Hintergrund aufrufen)
pub fn check() -> Result<Option<Release>, String> {
    let body = get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), 20)?;
    let rel = parse_release(&String::from_utf8_lossy(&body)).ok_or("Antwort von GitHub nicht lesbar")?;
    Ok(newer(&rel.version, CURRENT).then_some(rel))
}

// ------------------------------------------------------------------ ZIP lesen (nur das Nötigste)

fn u16le(b: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]) as usize)
}

fn u32le(b: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]) as usize)
}

/// Eine Datei aus einem ZIP holen, deren Name (ohne Ordner) `want` ist
pub fn zip_extract(zip: &[u8], want: &str) -> Result<Vec<u8>, String> {
    // Ende des zentralen Verzeichnisses suchen
    let eocd = (0..zip.len().saturating_sub(21)).rev().find(|&i| zip[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).ok_or("Kein gültiges ZIP")?;
    let count = u16le(zip, eocd + 10).ok_or("ZIP beschädigt")?;
    let mut p = u32le(zip, eocd + 16).ok_or("ZIP beschädigt")?;
    for _ in 0..count {
        if !zip.get(p..).is_some_and(|s| s.starts_with(&[0x50, 0x4b, 0x01, 0x02])) {
            return Err("ZIP beschädigt (Verzeichnis)".into());
        }
        let method = u16le(zip, p + 10).ok_or("ZIP beschädigt")?;
        let csize = u32le(zip, p + 20).ok_or("ZIP beschädigt")?;
        let usize_ = u32le(zip, p + 24).ok_or("ZIP beschädigt")?;
        let (nlen, xlen, clen) = (u16le(zip, p + 28).unwrap_or(0), u16le(zip, p + 30).unwrap_or(0), u16le(zip, p + 32).unwrap_or(0));
        let local = u32le(zip, p + 42).ok_or("ZIP beschädigt")?;
        let name = String::from_utf8_lossy(zip.get(p + 46..p + 46 + nlen).ok_or("ZIP beschädigt")?).to_string();
        p += 46 + nlen + xlen + clen;
        if name.rsplit('/').next() != Some(want) {
            continue;
        }
        let lnlen = u16le(zip, local + 26).ok_or("ZIP beschädigt")?;
        let lxlen = u16le(zip, local + 28).ok_or("ZIP beschädigt")?;
        let start = local + 30 + lnlen + lxlen;
        let data = zip.get(start..start + csize).ok_or("ZIP unvollständig")?;
        let out = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, usize_.max(1) + 16).map_err(|e| format!("Entpacken fehlgeschlagen: {e:?}"))?,
            m => return Err(format!("ZIP-Verfahren {m} wird nicht unterstützt")),
        };
        if out.len() != usize_ {
            return Err("ZIP: Größe stimmt nicht".into());
        }
        return Ok(out);
    }
    Err(format!("{want} nicht im Paket gefunden"))
}

// ------------------------------------------------------------------ Austausch

fn old_path(exe: &Path) -> PathBuf {
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("amboss");
    let ext = exe.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    exe.with_file_name(format!("{stem}.old{ext}"))
}

/// Beim Start: Reste eines früheren Updates entfernen
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

/// Neue Programmdatei an die Stelle von `exe` setzen (alte wird umbenannt)
pub fn replace_exe(exe: &Path, new_bytes: &[u8]) -> Result<(), String> {
    if new_bytes.len() < 1_000_000 {
        return Err("Heruntergeladene Datei ist zu klein – Update abgebrochen".into());
    }
    let dir = exe.parent().ok_or("Programmordner unbekannt")?;
    let tmp = dir.join(".amboss_update.tmp");
    std::fs::write(&tmp, new_bytes).map_err(|e| format!("Kein Schreibrecht im Programmordner ({e}). Bitte das Paket von der Release-Seite laden und entpacken."))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Programmdatei lässt sich nicht ersetzen ({e})")
    })?;
    if let Err(e) = std::fs::rename(&tmp, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Programmdatei lässt sich nicht ersetzen ({e})"));
    }
    Ok(())
}

/// Paket laden, Programmdatei herausholen und austauschen (blockiert – im Hintergrund aufrufen)
pub fn install(rel: &Release) -> Result<(), String> {
    let (_, url) = rel.asset.as_ref().ok_or("Für dieses System gibt es kein Paket – bitte die Release-Seite öffnen")?;
    let zip = get(url, 600)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = exe.file_name().and_then(|n| n.to_str()).unwrap_or(if cfg!(windows) { "amboss.exe" } else { "amboss" }).to_string();
    let bytes = zip_extract(&zip, &name).or_else(|_| zip_extract(&zip, if cfg!(windows) { "amboss.exe" } else { "amboss" }))?;
    replace_exe(&exe, &bytes)
}

/// Neu starten: Programm erneut starten (danach beendet sich die laufende Instanz)
pub fn relaunch() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("0.10.0", "0.9.10"));
        assert!(newer("v1.0", "0.99.99"));
        assert!(!newer("0.9.10", "0.9.10"));
        assert!(!newer("v0.9.9", "0.9.10"));
        assert!(newer("0.9.10.1", "0.9.10"));
    }

    #[test]
    fn release_json() {
        let os = os_tag();
        let j = format!(
            r#"{{"tag_name":"v0.10.0","html_url":"https://github.com/x/y/releases/tag/v0.10.0","body":"Neu: alles","assets":[
            {{"name":"amboss-quellcode.zip","browser_download_url":"https://e/q.zip"}},
            {{"name":"Amboss-0.10.0-{os}.zip","browser_download_url":"https://e/a.zip"}}]}}"#
        );
        let r = parse_release(&j).unwrap();
        assert_eq!(r.version, "0.10.0");
        assert_eq!(r.asset.unwrap().1, "https://e/a.zip");
        assert_eq!(r.notes, "Neu: alles");
    }

    #[test]
    fn zip_and_replace() {
        let dir = std::env::temp_dir().join(format!("amboss_upd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in/amboss")).unwrap();
        let big: Vec<u8> = (0..1_500_000u32).map(|i| (i % 97) as u8).collect();
        std::fs::write(dir.join("in/amboss/amboss.exe"), &big).unwrap();
        std::fs::write(dir.join("in/amboss/README.md"), b"hallo").unwrap();
        let ok = Command::new("zip").current_dir(dir.join("in")).args(["-qr", "../p.zip", "amboss"]).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            eprintln!("zip fehlt – Test übersprungen");
            return;
        }
        let z = std::fs::read(dir.join("p.zip")).unwrap();
        assert_eq!(zip_extract(&z, "README.md").unwrap(), b"hallo");
        let got = zip_extract(&z, "amboss.exe").unwrap();
        assert!(got == big);
        // Austausch
        let exe = dir.join("amboss.exe");
        std::fs::write(&exe, b"alt").unwrap();
        replace_exe(&exe, &got).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap().len(), big.len());
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"alt");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
