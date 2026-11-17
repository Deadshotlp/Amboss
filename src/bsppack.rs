//! Eigene Inhalte in eine fertige BSP packen (wie bspzip / AutoBSPpackingTool), ohne externes Programm.
//! Der Pakfile-Lump (Nr. 40) ist ein unkomprimiertes ZIP. Vorhandene Einträge (z. B. Cubemaps von VBSP)
//! bleiben erhalten, gleichnamige werden ersetzt. Der neue Lump wird ans Dateiende gehängt
//! (bzw. an seiner alten Stelle neu geschrieben, wenn er schon am Ende lag) – andere Lumps bewegen sich nicht.

use std::path::Path;

const PAK_LUMP: usize = 40;
const HEADER_LUMPS: usize = 64;

/// Ein Eintrag im ZIP (Daten roh, ggf. komprimiert – wird beim Umpacken unverändert übernommen)
#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub crc: u32,
    pub csize: u32,
    pub usize: u32,
    pub data: Vec<u8>,
}

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *v = c;
    }
    t
}

pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]])).ok_or_else(|| "ZIP zu kurz".into())
}
fn u32_at(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])).ok_or_else(|| "ZIP zu kurz".into())
}

/// Pfad im Paket: Kleinbuchstaben, Schrägstriche
pub fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

/// ZIP aus dem Speicher lesen (über das zentrale Verzeichnis)
pub fn read_zip(b: &[u8]) -> Result<Vec<ZipEntry>, String> {
    if b.is_empty() {
        return Ok(Vec::new());
    }
    // Ende des zentralen Verzeichnisses rückwärts suchen (Kommentar max. 64 KB)
    let start = b.len().saturating_sub(22 + 65535);
    let eocd = (start..=b.len().saturating_sub(22)).rev().find(|&i| b[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or("Kein ZIP-Verzeichnis im Pakfile")?;
    let n = u16_at(b, eocd + 10)? as usize;
    let mut p = u32_at(b, eocd + 16)? as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if u32_at(b, p)? != 0x0201_4b50 {
            return Err("Defektes ZIP-Verzeichnis".into());
        }
        let method = u16_at(b, p + 10)?;
        let crc = u32_at(b, p + 16)?;
        let csize = u32_at(b, p + 20)?;
        let usize = u32_at(b, p + 24)?;
        let nl = u16_at(b, p + 28)? as usize;
        let el = u16_at(b, p + 30)? as usize;
        let cl = u16_at(b, p + 32)? as usize;
        let local = u32_at(b, p + 42)? as usize;
        let name = String::from_utf8_lossy(b.get(p + 46..p + 46 + nl).ok_or("ZIP zu kurz")?).to_string();
        p += 46 + nl + el + cl;
        if u32_at(b, local)? != 0x0403_4b50 {
            return Err(format!("Defekter Eintrag {name}"));
        }
        let lnl = u16_at(b, local + 26)? as usize;
        let lel = u16_at(b, local + 28)? as usize;
        let d0 = local + 30 + lnl + lel;
        let data = b.get(d0..d0 + csize as usize).ok_or_else(|| format!("Daten von {name} fehlen"))?.to_vec();
        out.push(ZipEntry { name, method, crc, csize, usize, data });
    }
    Ok(out)
}

/// ZIP schreiben (Einträge werden so übernommen, wie sie sind)
pub fn write_zip(entries: &[ZipEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let off = out.len() as u32;
        let name = e.name.as_bytes();
        // lokaler Kopf
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&10u16.to_le_bytes()); // benötigte Version
        out.extend_from_slice(&0u16.to_le_bytes()); // Flags
        out.extend_from_slice(&e.method.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // Zeit/Datum
        out.extend_from_slice(&e.crc.to_le_bytes());
        out.extend_from_slice(&e.csize.to_le_bytes());
        out.extend_from_slice(&e.usize.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&e.data);
        // zentraler Eintrag
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&10u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&e.method.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&e.crc.to_le_bytes());
        central.extend_from_slice(&e.csize.to_le_bytes());
        central.extend_from_slice(&e.usize.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 8]); // Extra, Kommentar, Disk, interne Attribute
        central.extend_from_slice(&0u32.to_le_bytes()); // externe Attribute
        central.extend_from_slice(&off.to_le_bytes());
        central.extend_from_slice(name);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Lage des Pakfile-Lumps: (Offset, Länge, Position des Offset-Felds, Position des Längen-Felds)
fn pak_lump(b: &[u8]) -> Result<(usize, usize, usize, usize), String> {
    if b.get(0..4) != Some(b"VBSP") {
        return Err("Keine Source-BSP-Datei (VBSP fehlt)".into());
    }
    let version = u32_at(b, 4)?;
    if !(17..=21).contains(&version) {
        return Err(format!("BSP-Version {version} wird nicht unterstützt"));
    }
    let entry = 8 + PAK_LUMP * 16;
    // Version 21 (L4D2) speichert (Version, Offset, Länge) statt (Offset, Länge, Version)
    let (op, lp) = if version == 21 && u32_at(b, entry)? < 0x100 && u32_at(b, entry + 4)? as usize >= 8 + HEADER_LUMPS * 16 { (entry + 4, entry + 8) } else { (entry, entry + 4) };
    let (ofs, len) = (u32_at(b, op)? as usize, u32_at(b, lp)? as usize);
    if ofs + len > b.len() {
        return Err("Pakfile-Lump zeigt hinter das Dateiende".into());
    }
    Ok((ofs, len, op, lp))
}

/// Dateien im Pakfile einer BSP
#[allow(dead_code)]
pub fn list(bsp: &Path) -> Result<Vec<ZipEntry>, String> {
    let b = std::fs::read(bsp).map_err(|e| format!("{}: {e}", bsp.display()))?;
    let (ofs, len, ..) = pak_lump(&b)?;
    read_zip(&b[ofs..ofs + len])
}

/// Ergebnis des Packens
#[derive(Debug, Default, Clone)]
pub struct PackResult {
    pub added: usize,
    pub replaced: usize,
    pub kept: usize,
    pub bytes: usize,
}

/// Dateien (Pfad im Spiel, Inhalt) in die BSP packen (im Speicher)
pub fn pack_bytes(b: &mut Vec<u8>, files: &[(String, Vec<u8>)]) -> Result<PackResult, String> {
    let (ofs, len, o_pos, l_pos) = pak_lump(b)?;
    let mut entries = read_zip(&b[ofs..ofs + len])?;
    let mut r = PackResult::default();
    for (path, data) in files {
        let name = norm(path);
        let e = ZipEntry { name: name.clone(), method: 0, crc: crc32(data), csize: data.len() as u32, usize: data.len() as u32, data: data.clone() };
        r.bytes += data.len();
        match entries.iter().position(|x| norm(&x.name) == name) {
            Some(i) => {
                entries[i] = e;
                r.replaced += 1;
            }
            None => {
                entries.push(e);
                r.added += 1;
            }
        }
    }
    r.kept = entries.len() - r.added - r.replaced;
    let zip = write_zip(&entries);
    // Lag der alte Lump am Ende? Dann dort überschreiben, sonst anhängen (4-Byte-ausgerichtet)
    let new_ofs = if len > 0 && ofs + len == b.len() {
        b.truncate(ofs);
        ofs
    } else {
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b.len()
    };
    b.extend_from_slice(&zip);
    b[o_pos..o_pos + 4].copy_from_slice(&(new_ofs as u32).to_le_bytes());
    b[l_pos..l_pos + 4].copy_from_slice(&(zip.len() as u32).to_le_bytes());
    Ok(r)
}

/// Dateien in eine BSP-Datei packen (sicher: erst temporäre Datei, dann umbenennen)
pub fn pack(bsp: &Path, files: &[(String, Vec<u8>)]) -> Result<PackResult, String> {
    let mut b = std::fs::read(bsp).map_err(|e| format!("{}: {e}", bsp.display()))?;
    let r = pack_bytes(&mut b, files)?;
    let tmp = bsp.with_extension("bsp.packtmp");
    std::fs::write(&tmp, &b).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, bsp).map_err(|e| format!("{}: {e}", bsp.display()))?;
    Ok(r)
}

/// bspzip-Packliste lesen (abwechselnd Pfad im Spiel / Pfad auf der Platte)
pub fn read_packlist(text: &str) -> Vec<(String, std::path::PathBuf)> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].to_string(), std::path::PathBuf::from(c[1]))).collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Minimale BSP v20: Header, ein Daten-Lump, Pakfile mit einem Eintrag am Ende
    pub fn tiny_bsp() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"VBSP");
        b.extend_from_slice(&20u32.to_le_bytes());
        b.resize(8 + 64 * 16 + 4, 0);
        // Lump 0: 16 Bytes Daten
        let l0 = b.len();
        b.extend_from_slice(&[7u8; 16]);
        b[8..12].copy_from_slice(&(l0 as u32).to_le_bytes());
        b[12..16].copy_from_slice(&16u32.to_le_bytes());
        let zip = write_zip(&[ZipEntry { name: "materials/maps/test/c0.vtf".into(), method: 0, crc: crc32(b"cube"), csize: 4, usize: 4, data: b"cube".to_vec() }]);
        let po = b.len();
        b.extend_from_slice(&zip);
        let e = 8 + 40 * 16;
        b[e..e + 4].copy_from_slice(&(po as u32).to_le_bytes());
        b[e + 4..e + 8].copy_from_slice(&(zip.len() as u32).to_le_bytes());
        b
    }

    #[test]
    fn crc() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn pack_merge() {
        let mut b = tiny_bsp();
        let r = pack_bytes(&mut b, &[("Materials\\Eigene\\Holz.vmt".into(), b"LightmappedGeneric {}".to_vec()), ("materials/maps/test/c0.vtf".into(), b"neu".to_vec())]).unwrap();
        assert_eq!((r.added, r.replaced, r.kept), (1, 1, 0));
        let (ofs, len, ..) = pak_lump(&b).unwrap();
        let e = read_zip(&b[ofs..ofs + len]).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].data, b"neu");
        assert_eq!(e[1].name, "materials/eigene/holz.vmt");
        assert_eq!(e[1].crc, crc32(b"LightmappedGeneric {}"));
        // Lump 0 unverändert
        assert_eq!(&b[8 + 64 * 16 + 4..8 + 64 * 16 + 20], &[7u8; 16]);
        // zweites Packen: Lump lag am Ende → wird ersetzt, Datei wächst nicht unnötig
        let size = b.len();
        pack_bytes(&mut b, &[]).unwrap();
        assert_eq!(b.len(), size);
        assert!(pack_bytes(&mut b"kaputt".to_vec(), &[]).is_err());
    }

    #[test]
    fn packlist() {
        let l = read_packlist("materials/a.vmt\nC:\\x\\a.vmt\n\nmodels/b.mdl\n/y/b.mdl\n");
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].0, "models/b.mdl");
    }
}
