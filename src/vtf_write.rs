//! VTF schreiben (Version 7.2, unkomprimiert BGR888/BGRA8888 mit allen Mip-Stufen) und
//! Bilder (PNG) einlesen – für eigene Texturen aus dem Material-Editor.

/// Ein RGBA-Bild
#[derive(Clone, Debug)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    pub fn has_alpha(&self) -> bool {
        self.px.chunks(4).any(|p| p[3] < 250)
    }
}

/// PNG lesen (alle Farbtypen → RGBA8)
pub fn load_png(bytes: &[u8]) -> Result<Rgba, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; r.output_buffer_size().ok_or("PNG zu groß")?];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let src = &buf[..info.buffer_size()];
    let px: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => src.to_vec(),
        png::ColorType::Rgb => src.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => src.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("PNG mit Farbpalette wird nicht unterstützt".into()),
    };
    Ok(Rgba { w, h, px })
}

/// Nächste Zweierpotenz (Source verlangt Texturgrößen 2ⁿ), höchstens `max`
pub fn pow2(v: u32, max: u32) -> u32 {
    let mut p = 1;
    while p < v && p < max {
        p *= 2;
    }
    // näher an der kleineren Potenz? dann die nehmen (weniger Speicher, kaum Verlust)
    if p > 1 && p > v && (p - v) > (v - p / 2) {
        p / 2
    } else {
        p
    }
}

/// Bilinear auf neue Größe skalieren
pub fn resize(img: &Rgba, w: u32, h: u32) -> Rgba {
    if img.w == w && img.h == h {
        return img.clone();
    }
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * img.h as f32 / h as f32 - 0.5).clamp(0.0, img.h as f32 - 1.0);
        let (y0, ty) = (fy.floor() as u32, fy.fract());
        let y1 = (y0 + 1).min(img.h - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * img.w as f32 / w as f32 - 0.5).clamp(0.0, img.w as f32 - 1.0);
            let (x0, tx) = (fx.floor() as u32, fx.fract());
            let x1 = (x0 + 1).min(img.w - 1);
            for c in 0..4 {
                let g = |xx: u32, yy: u32| img.px[((yy * img.w + xx) * 4 + c) as usize] as f32;
                let v = (g(x0, y0) * (1.0 - tx) + g(x1, y0) * tx) * (1.0 - ty) + (g(x0, y1) * (1.0 - tx) + g(x1, y1) * tx) * ty;
                px[((y * w + x) * 4 + c) as usize] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Rgba { w, h, px }
}

/// Nächstkleinere Mip-Stufe (2×2-Mittelwert)
fn half(img: &Rgba) -> Rgba {
    let (w, h) = ((img.w / 2).max(1), (img.h / 2).max(1));
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            for c in 0..4 {
                let mut s = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (xx, yy) = ((x * 2 + dx).min(img.w - 1), (y * 2 + dy).min(img.h - 1));
                    s += img.px[((yy * img.w + xx) * 4 + c) as usize] as u32;
                }
                px[((y * w + x) * 4 + c) as usize] = ((s + 2) / 4) as u8;
            }
        }
    }
    Rgba { w, h, px }
}

/// Optionen für das Schreiben
#[derive(Clone, Copy, Debug, Default)]
pub struct VtfOpts {
    /// Normalmap (Flag NORMAL)
    pub normal: bool,
    /// Kanten nicht kacheln (CLAMPS/CLAMPT)
    pub clamp: bool,
    /// größte Kantenlänge
    pub max_size: u32,
}

/// VTF 7.2 erzeugen. Größe wird auf Zweierpotenzen gebracht; Alpha nur, wenn das Bild welches hat.
pub fn encode(img: &Rgba, o: VtfOpts) -> Vec<u8> {
    let max = if o.max_size == 0 { 2048 } else { o.max_size };
    let (w, h) = (pow2(img.w.max(1), max), pow2(img.h.max(1), max));
    let base = resize(img, w, h);
    let alpha = base.has_alpha();
    // Mip-Kette (größte zuerst)
    let mut mips = vec![base];
    while mips.last().is_some_and(|m| m.w > 1 || m.h > 1) {
        let next = half(mips.last().unwrap());
        mips.push(next);
    }
    // Durchschnittsfarbe → reflectivity (für VRAD-Lichtabstrahlung)
    let last = mips.last().unwrap();
    let refl = [0, 1, 2].map(|c| (last.px[c] as f32 / 255.0).powf(2.2));
    let mut flags: u32 = 0;
    if alpha {
        flags |= 0x2000; // EIGHTBITALPHA
    }
    if o.normal {
        flags |= 0x80; // NORMAL
    }
    if o.clamp {
        flags |= 0x4 | 0x8;
    }
    let fmt: u32 = if alpha { 12 } else { 3 }; // BGRA8888 / BGR888
    let mut b = vec![0u8; 80];
    b[0..4].copy_from_slice(b"VTF\0");
    b[4..8].copy_from_slice(&7u32.to_le_bytes());
    b[8..12].copy_from_slice(&2u32.to_le_bytes());
    b[12..16].copy_from_slice(&80u32.to_le_bytes());
    b[16..18].copy_from_slice(&(w as u16).to_le_bytes());
    b[18..20].copy_from_slice(&(h as u16).to_le_bytes());
    b[20..24].copy_from_slice(&flags.to_le_bytes());
    b[24..26].copy_from_slice(&1u16.to_le_bytes()); // Bilder
    b[26..28].copy_from_slice(&0u16.to_le_bytes()); // erstes Bild
    for (i, r) in refl.iter().enumerate() {
        b[32 + i * 4..36 + i * 4].copy_from_slice(&r.to_le_bytes());
    }
    b[48..52].copy_from_slice(&1.0f32.to_le_bytes()); // bumpmap scale
    b[52..56].copy_from_slice(&fmt.to_le_bytes());
    b[56] = mips.len() as u8;
    b[57..61].copy_from_slice(&0xffff_ffffu32.to_le_bytes()); // kein Vorschaubild
    b[61] = 0;
    b[62] = 0;
    b[63..65].copy_from_slice(&1u16.to_le_bytes()); // Tiefe
    // Daten: kleinste Mip-Stufe zuerst
    for m in mips.iter().rev() {
        for p in m.px.chunks(4) {
            if alpha {
                b.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            } else {
                b.extend_from_slice(&[p[2], p[1], p[0]]);
            }
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32, alpha: u8) -> Rgba {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let on = (x / 4 + y / 4) % 2 == 0;
                px.extend_from_slice(&if on { [250, 120, 20, alpha] } else { [20, 20, 30, alpha] });
            }
        }
        Rgba { w, h, px }
    }

    #[test]
    fn roundtrip_with_reader() {
        // 48×20 → 64×16 (nächste Zweierpotenzen), ohne Alpha → BGR888
        let v = encode(&checker(48, 20, 255), VtfOpts::default());
        let img = crate::vtf::load(&v, 4096).unwrap();
        assert_eq!((img.width, img.height), (64, 16));
        assert_eq!((img.pw, img.ph), (64, 16));
        assert_eq!(&img.rgba[0..3], &[250, 120, 20]);
        // kleine Mip-Stufe wird gewählt und ist gemittelt
        let small = crate::vtf::load(&v, 8).unwrap();
        assert!(small.pw <= 8 && small.ph <= 8);
        // mit Alpha → BGRA8888, Flag gesetzt
        let v = encode(&checker(16, 16, 100), VtfOpts { normal: true, ..Default::default() });
        let flags = u32::from_le_bytes(v[20..24].try_into().unwrap());
        assert!(flags & 0x2000 != 0 && flags & 0x80 != 0);
        let img = crate::vtf::load(&v, 64).unwrap();
        assert_eq!(img.rgba[3], 100);
    }

    #[test]
    fn png_import() {
        // PNG im Speicher erzeugen und wieder lesen
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 3, 2);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9, 8, 8, 8, 7, 7, 7]).unwrap();
        }
        let img = load_png(&out).unwrap();
        assert_eq!((img.w, img.h), (3, 2));
        assert_eq!(&img.px[0..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert!(!img.has_alpha());
    }
}
