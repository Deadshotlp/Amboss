//! VTF-Decoder (Valve Texture Format) – liefert RGBA8 für eine passende Mip-Stufe.

pub struct Image {
    /// Originalgröße (Mip 0) – wichtig für die Textur-Koordinaten
    pub width: u32,
    pub height: u32,
    /// Größe der dekodierten Mip-Stufe
    pub pw: u32,
    pub ph: u32,
    pub rgba: Vec<u8>,
    /// weitere Bilder einer animierten Textur (gleiche Größe wie `rgba`)
    pub frames: Vec<Vec<u8>>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fmt {
    Rgba8888,
    Abgr8888,
    Rgb888,
    Bgr888,
    Rgb565,
    I8,
    Ia88,
    A8,
    Argb8888,
    Bgra8888,
    Dxt1,
    Dxt3,
    Dxt5,
    Bgrx8888,
    Bgr565,
    Bgrx5551,
    Bgra4444,
    Bgra5551,
    Uv88,
    Rgba16f,
}

fn fmt_from(id: u32) -> Option<Fmt> {
    Some(match id {
        0 => Fmt::Rgba8888,
        1 => Fmt::Abgr8888,
        2 | 9 => Fmt::Rgb888,
        3 | 10 => Fmt::Bgr888,
        4 => Fmt::Rgb565,
        5 => Fmt::I8,
        6 => Fmt::Ia88,
        8 => Fmt::A8,
        11 => Fmt::Argb8888,
        12 => Fmt::Bgra8888,
        13 | 20 => Fmt::Dxt1,
        14 => Fmt::Dxt3,
        15 => Fmt::Dxt5,
        16 => Fmt::Bgrx8888,
        17 => Fmt::Bgr565,
        18 => Fmt::Bgrx5551,
        19 => Fmt::Bgra4444,
        21 => Fmt::Bgra5551,
        22 => Fmt::Uv88,
        24 => Fmt::Rgba16f,
        _ => return None,
    })
}

fn data_size(f: Fmt, w: u32, h: u32) -> usize {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let blocks = w.div_ceil(4) * h.div_ceil(4);
    match f {
        Fmt::Dxt1 => blocks * 8,
        Fmt::Dxt3 | Fmt::Dxt5 => blocks * 16,
        Fmt::Rgba8888 | Fmt::Abgr8888 | Fmt::Argb8888 | Fmt::Bgra8888 | Fmt::Bgrx8888 => w * h * 4,
        Fmt::Rgb888 | Fmt::Bgr888 => w * h * 3,
        Fmt::Rgb565 | Fmt::Bgr565 | Fmt::Ia88 | Fmt::Bgrx5551 | Fmt::Bgra4444 | Fmt::Bgra5551 | Fmt::Uv88 => w * h * 2,
        Fmt::I8 | Fmt::A8 => w * h,
        Fmt::Rgba16f => w * h * 8,
    }
}

fn c565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 31) as u32;
    let g = ((c >> 5) & 63) as u32;
    let b = (c & 31) as u32;
    [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
}

fn decode_color_block(blk: &[u8], out: &mut [[u8; 4]; 16], allow_alpha: bool) {
    let c0 = u16::from_le_bytes([blk[0], blk[1]]);
    let c1 = u16::from_le_bytes([blk[2], blk[3]]);
    let a = c565(c0);
    let b = c565(c1);
    let mut pal = [[a[0], a[1], a[2], 255], [b[0], b[1], b[2], 255], [0; 4], [0; 4]];
    if c0 > c1 || !allow_alpha {
        for i in 0..3 {
            pal[2][i] = ((2 * a[i] as u32 + b[i] as u32) / 3) as u8;
            pal[3][i] = ((a[i] as u32 + 2 * b[i] as u32) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for i in 0..3 {
            pal[2][i] = ((a[i] as u32 + b[i] as u32) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([blk[4], blk[5], blk[6], blk[7]]);
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((idx >> (2 * i)) & 3) as usize];
    }
}

fn decode_dxt(f: Fmt, w: u32, h: u32, d: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (w as usize, h as usize);
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    let bsize = if f == Fmt::Dxt1 { 8 } else { 16 };
    let mut out = vec![0u8; w * h * 4];
    for by in 0..bh {
        for bx in 0..bw {
            let o = (by * bw + bx) * bsize;
            let blk = d.get(o..o + bsize)?;
            let mut px = [[0u8; 4]; 16];
            match f {
                Fmt::Dxt1 => decode_color_block(blk, &mut px, true),
                Fmt::Dxt3 => {
                    decode_color_block(&blk[8..], &mut px, false);
                    for (i, p) in px.iter_mut().enumerate() {
                        let nib = (blk[i / 2] >> ((i % 2) * 4)) & 0xF;
                        p[3] = nib * 17;
                    }
                }
                _ => {
                    decode_color_block(&blk[8..], &mut px, false);
                    let a0 = blk[0] as u32;
                    let a1 = blk[1] as u32;
                    let mut al = [0u32; 8];
                    al[0] = a0;
                    al[1] = a1;
                    if a0 > a1 {
                        for i in 1..7 {
                            al[i + 1] = ((7 - i as u32) * a0 + i as u32 * a1) / 7;
                        }
                    } else {
                        for i in 1..5 {
                            al[i + 1] = ((5 - i as u32) * a0 + i as u32 * a1) / 5;
                        }
                        al[6] = 0;
                        al[7] = 255;
                    }
                    let mut bits: u64 = 0;
                    for i in 0..6 {
                        bits |= (blk[2 + i] as u64) << (8 * i);
                    }
                    for (i, p) in px.iter_mut().enumerate() {
                        p[3] = al[((bits >> (3 * i)) & 7) as usize] as u8;
                    }
                }
            }
            for py in 0..4 {
                for pxx in 0..4 {
                    let x = bx * 4 + pxx;
                    let y = by * 4 + py;
                    if x < w && y < h {
                        let o = (y * w + x) * 4;
                        out[o..o + 4].copy_from_slice(&px[py * 4 + pxx]);
                    }
                }
            }
        }
    }
    Some(out)
}

fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1f) as i32;
    let m = (h & 0x3ff) as f32;
    if e == 0 {
        s * m * 2f32.powi(-24)
    } else if e == 31 {
        s * f32::INFINITY
    } else {
        s * (1.0 + m / 1024.0) * 2f32.powi(e - 15)
    }
}

fn decode(f: Fmt, w: u32, h: u32, d: &[u8]) -> Option<Vec<u8>> {
    if matches!(f, Fmt::Dxt1 | Fmt::Dxt3 | Fmt::Dxt5) {
        return decode_dxt(f, w, h, d);
    }
    let n = (w * h) as usize;
    let d = d.get(..data_size(f, w, h))?;
    let mut out = Vec::with_capacity(n * 4);
    let ex5 = |v: u16| ((v as u32 & 31) * 255 / 31) as u8;
    for i in 0..n {
        let px: [u8; 4] = match f {
            Fmt::Rgba8888 => [d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]],
            Fmt::Abgr8888 => [d[i * 4 + 3], d[i * 4 + 2], d[i * 4 + 1], d[i * 4]],
            Fmt::Argb8888 => [d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3], d[i * 4]],
            Fmt::Bgra8888 => [d[i * 4 + 2], d[i * 4 + 1], d[i * 4], d[i * 4 + 3]],
            Fmt::Bgrx8888 => [d[i * 4 + 2], d[i * 4 + 1], d[i * 4], 255],
            Fmt::Rgb888 => [d[i * 3], d[i * 3 + 1], d[i * 3 + 2], 255],
            Fmt::Bgr888 => [d[i * 3 + 2], d[i * 3 + 1], d[i * 3], 255],
            Fmt::I8 => [d[i], d[i], d[i], 255],
            Fmt::A8 => [255, 255, 255, d[i]],
            Fmt::Ia88 => [d[i * 2], d[i * 2], d[i * 2], d[i * 2 + 1]],
            Fmt::Uv88 => [d[i * 2], d[i * 2 + 1], 255, 255],
            Fmt::Rgb565 => {
                let c = c565(u16::from_le_bytes([d[i * 2], d[i * 2 + 1]]));
                [c[0], c[1], c[2], 255]
            }
            Fmt::Bgr565 => {
                let c = c565(u16::from_le_bytes([d[i * 2], d[i * 2 + 1]]));
                [c[2], c[1], c[0], 255]
            }
            Fmt::Bgrx5551 | Fmt::Bgra5551 => {
                let v = u16::from_le_bytes([d[i * 2], d[i * 2 + 1]]);
                let a = if f == Fmt::Bgra5551 && v & 0x8000 == 0 { 0 } else { 255 };
                [ex5(v >> 10), ex5(v >> 5), ex5(v), a]
            }
            Fmt::Bgra4444 => {
                let v = u16::from_le_bytes([d[i * 2], d[i * 2 + 1]]);
                [((v >> 8) & 15) as u8 * 17, ((v >> 4) & 15) as u8 * 17, (v & 15) as u8 * 17, ((v >> 12) & 15) as u8 * 17]
            }
            Fmt::Rgba16f => {
                let o = i * 8;
                let c = |k: usize| {
                    let v = half(u16::from_le_bytes([d[o + k * 2], d[o + k * 2 + 1]]));
                    (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8
                };
                [c(0), c(1), c(2), 255]
            }
            Fmt::Dxt1 | Fmt::Dxt3 | Fmt::Dxt5 => unreachable!(),
        };
        out.extend_from_slice(&px);
    }
    Some(out)
}

/// Lädt die größte Mip-Stufe, deren Kantenlänge `max_size` nicht überschreitet.
pub fn load(b: &[u8], max_size: u32) -> Result<Image, String> {
    load_opts(b, max_size, 1)
}

/// Wie `load`, liefert bei animierten Texturen bis zu `max_frames` Bilder
pub fn load_opts(b: &[u8], max_size: u32, max_frames: usize) -> Result<Image, String> {
    if b.get(0..4) != Some(b"VTF\0") {
        return Err("keine VTF-Datei".into());
    }
    let err = || "VTF beschädigt".to_string();
    let major = u32_at(b, 4).ok_or_else(err)?;
    let minor = u32_at(b, 8).ok_or_else(err)?;
    if major != 7 {
        return Err(format!("VTF-Version {major}.{minor} nicht unterstützt"));
    }
    let header_size = u32_at(b, 12).ok_or_else(err)? as usize;
    let width = u16_at(b, 16).ok_or_else(err)? as u32;
    let height = u16_at(b, 18).ok_or_else(err)? as u32;
    let flags = u32_at(b, 20).ok_or_else(err)?;
    let frames = u16_at(b, 24).ok_or_else(err)?.max(1) as usize;
    let first_frame = u16_at(b, 26).ok_or_else(err)?;
    let hi_fmt_id = u32_at(b, 52).ok_or_else(err)?;
    let mips = (*b.get(56).ok_or_else(err)?).max(1) as u32;
    let lo_fmt_id = u32_at(b, 57).ok_or_else(err)?;
    let lo_w = *b.get(61).ok_or_else(err)? as u32;
    let lo_h = *b.get(62).ok_or_else(err)? as u32;
    let depth = if minor >= 2 { u16_at(b, 63).ok_or_else(err)?.max(1) as u32 } else { 1 };
    let fmt = fmt_from(hi_fmt_id).ok_or_else(|| format!("VTF-Format {hi_fmt_id} nicht unterstützt"))?;

    let faces: usize = if flags & 0x4000 != 0 {
        if minor < 5 && first_frame == 0xffff {
            7
        } else {
            6
        }
    } else {
        1
    };

    let hi_start = if minor >= 3 {
        let n = u32_at(b, 68).ok_or_else(err)? as usize;
        let mut found = None;
        for i in 0..n {
            let o = 80 + i * 8;
            let tag = b.get(o..o + 3).ok_or_else(err)?;
            if tag == [0x30, 0, 0] {
                found = Some(u32_at(b, o + 4).ok_or_else(err)? as usize);
            }
        }
        found.ok_or("VTF ohne Bilddaten")?
    } else {
        let lo_size = if lo_fmt_id == 0xffff_ffff || lo_w == 0 { 0 } else { data_size(Fmt::Dxt1, lo_w, lo_h) };
        header_size + lo_size
    };

    // passende Mip-Stufe wählen
    let mut level = 0;
    while level + 1 < mips && (width >> level).max(height >> level) > max_size {
        level += 1;
    }
    let mut off = hi_start;
    let dim = |l: u32| ((width >> l).max(1), (height >> l).max(1), (depth >> l).max(1));
    for l in ((level + 1)..mips).rev() {
        let (w, h, d) = dim(l);
        off += data_size(fmt, w, h) * frames * faces * d as usize;
    }
    let (pw, ph, pd) = dim(level);
    let size = data_size(fmt, pw, ph);
    let data = b.get(off..off + size).ok_or("VTF-Daten zu kurz")?;
    let rgba = decode(fmt, pw, ph, data).ok_or_else(err)?;
    // Reihenfolge je Mip-Stufe: Bild → Seite → Schicht
    let stride = size * faces * pd as usize;
    let mut extra = Vec::new();
    for f in 1..frames.min(max_frames.max(1)) {
        let o = off + f * stride;
        match b.get(o..o + size).and_then(|d| decode(fmt, pw, ph, d)) {
            Some(px) => extra.push(px),
            None => break,
        }
    }
    Ok(Image { width, height, pw, ph, rgba, frames: extra })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Erzeugt eine VTF 7.2 mit BGR888-Daten (ohne Mips) bzw. DXT1
    pub fn make_vtf(w: u16, h: u16, fmt: u32, data: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 80];
        b[0..4].copy_from_slice(b"VTF\0");
        b[4..8].copy_from_slice(&7u32.to_le_bytes());
        b[8..12].copy_from_slice(&2u32.to_le_bytes());
        b[12..16].copy_from_slice(&80u32.to_le_bytes());
        b[16..18].copy_from_slice(&w.to_le_bytes());
        b[18..20].copy_from_slice(&h.to_le_bytes());
        b[24..26].copy_from_slice(&1u16.to_le_bytes());
        b[52..56].copy_from_slice(&fmt.to_le_bytes());
        b[56] = 1;
        b[57..61].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        b[63..65].copy_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn bgr888() {
        let data: Vec<u8> = (0..4).flat_map(|i| [10 * i as u8, 20, 30]).collect();
        let v = make_vtf(2, 2, 3, &data);
        let img = load(&v, 256).unwrap();
        assert_eq!((img.width, img.height, img.pw, img.ph), (2, 2, 2, 2));
        assert_eq!(&img.rgba[0..4], &[30, 20, 0, 255]);
        assert_eq!(&img.rgba[12..16], &[30, 20, 30, 255]);
    }

    #[test]
    fn dxt1_block() {
        // c0 = rot (0xF800), c1 = blau (0x001F), alle Pixel Index 0 -> rot
        let blk = [0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0];
        let v = make_vtf(4, 4, 13, &blk);
        let img = load(&v, 256).unwrap();
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(img.rgba.len(), 64);
    }

    #[test]
    fn mip_selection() {
        // 8x8 mit 4 Mips (1x1, 2x2, 4x4, 8x8), RGBA8888; kleinste zuerst gespeichert
        let mut data = Vec::new();
        for (s, val) in [(1, 1u8), (2, 2), (4, 4), (8, 8)] {
            data.extend(std::iter::repeat_n(val, s * s * 4));
        }
        let mut v = make_vtf(8, 8, 0, &data);
        v[56] = 4;
        let img = load(&v, 4).unwrap();
        assert_eq!((img.pw, img.ph), (4, 4));
        assert!(img.rgba.iter().all(|&x| x == 4));
        let img = load(&v, 256).unwrap();
        assert_eq!((img.pw, img.ph), (8, 8));
        assert!(img.rgba.iter().all(|&x| x == 8));
    }
}
