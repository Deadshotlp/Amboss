//! Textur-Werkzeuge: nahtlos machen, Normalmap aus Höhen-/Farbbild, PBR-Set → Source-Material.

use crate::vtf_write::{resize, Rgba};

fn lum(p: &[u8]) -> f32 {
    (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0
}

/// Kachelbar machen: die Ränder werden mit der um die halbe Größe versetzten Kopie überblendet.
/// `border` = Anteil der halben Kantenlänge, der überblendet wird (0,1–1).
pub fn make_seamless(img: &Rgba, border: f32) -> Rgba {
    let (w, h) = (img.w as usize, img.h as usize);
    let bw = ((w as f32 / 2.0) * border.clamp(0.05, 1.0)).max(1.0);
    let bh = ((h as f32 / 2.0) * border.clamp(0.05, 1.0)).max(1.0);
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            // Gewicht der versetzten Kopie: 1 am Rand, 0 innen
            let ex = 1.0 - smooth((x.min(w - 1 - x) as f32 + 0.5) / bw);
            let ey = 1.0 - smooth((y.min(h - 1 - y) as f32 + 0.5) / bh);
            let m = ex.max(ey);
            let o = (y * w + x) * 4;
            let s = (((y + h / 2) % h) * w + (x + w / 2) % w) * 4;
            for c in 0..4 {
                px[o + c] = (img.px[o + c] as f32 * (1.0 - m) + img.px[s + c] as f32 * m).round() as u8;
            }
        }
    }
    Rgba { w: img.w, h: img.h, px }
}

/// Normalmap aus einem Höhenbild (Helligkeit = Höhe), kachelnd, im DirectX-Format wie Source
/// (grün = nach unten). `strength` ≈ 1–10.
pub fn normal_from_height(img: &Rgba, strength: f32) -> Rgba {
    let (w, h) = (img.w as i64, img.h as i64);
    let hgt = |x: i64, y: i64| -> f32 {
        let (x, y) = (x.rem_euclid(w), y.rem_euclid(h));
        lum(&img.px[((y * w + x) * 4) as usize..])
    };
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            // Sobel
            let gx = (hgt(x + 1, y - 1) + 2.0 * hgt(x + 1, y) + hgt(x + 1, y + 1)) - (hgt(x - 1, y - 1) + 2.0 * hgt(x - 1, y) + hgt(x - 1, y + 1));
            let gy = (hgt(x - 1, y + 1) + 2.0 * hgt(x, y + 1) + hgt(x + 1, y + 1)) - (hgt(x - 1, y - 1) + 2.0 * hgt(x, y - 1) + hgt(x + 1, y - 1));
            let n = glam::Vec3::new(-gx * strength, -gy * strength, 1.0).normalize();
            px.extend([n.x, n.y, n.z].map(|c| ((c * 0.5 + 0.5) * 255.0).round() as u8));
            px.push(255);
        }
    }
    Rgba { w: img.w, h: img.h, px }
}

/// Grünkanal umdrehen (OpenGL ↔ DirectX)
pub fn flip_green(img: &mut Rgba) {
    for p in img.px.chunks_mut(4) {
        p[1] = 255 - p[1];
    }
}

/// Rolle eines Bildes in einem PBR-Set (nach Dateinamen)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PbrMap {
    Albedo,
    NormalGl,
    NormalDx,
    Roughness,
    Metallic,
    Ao,
    Height,
}

pub fn classify(file: &str) -> Option<PbrMap> {
    let n = file.to_ascii_lowercase();
    let stem = n.rsplit_once('.').map(|x| x.0).unwrap_or(&n);
    let has = |k: &[&str]| k.iter().any(|w| stem.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == *w) || stem.ends_with(w));
    if has(&["normaldx", "nor_dx", "dx"]) && has(&["normal", "nor", "nrm", "normaldx"]) {
        return Some(PbrMap::NormalDx);
    }
    if has(&["normal", "normalgl", "nor", "nrm", "norm"]) {
        return Some(PbrMap::NormalGl);
    }
    if has(&["roughness", "rough", "rgh"]) {
        return Some(PbrMap::Roughness);
    }
    if has(&["metallic", "metalness", "metal", "mtl"]) {
        return Some(PbrMap::Metallic);
    }
    if has(&["ao", "ambientocclusion", "occlusion", "ambient_occlusion"]) {
        return Some(PbrMap::Ao);
    }
    if has(&["height", "displacement", "disp", "bump"]) {
        return Some(PbrMap::Height);
    }
    if has(&["albedo", "basecolor", "base_color", "diffuse", "diff", "color", "col", "colour"]) {
        return Some(PbrMap::Albedo);
    }
    None
}

/// Ergebnis der PBR-Umwandlung
pub struct PbrResult {
    /// $basetexture
    pub base: Rgba,
    /// $bumpmap mit Glanzmaske im Alpha-Kanal
    pub normal: Option<Rgba>,
    /// VMT-Parameter (Schlüssel, Wert)
    pub params: Vec<(String, String)>,
}

/// PBR-Bilder in ein Source-Material umrechnen. `model` = VertexLitGeneric (Phong) statt LightmappedGeneric.
/// Grundfarbe × AO → $basetexture; Normalmap (DirectX, sonst aus der Höhe) → $bumpmap;
/// Glanz aus Rauheit und Metall → Maske im Alpha der Normalmap (Spiegelung bzw. Phong).
pub fn convert_pbr(maps: &[(PbrMap, Rgba)], model: bool) -> Result<PbrResult, String> {
    let get = |k: PbrMap| maps.iter().find(|m| m.0 == k).map(|m| &m.1);
    let albedo = get(PbrMap::Albedo).ok_or("Keine Grundfarbe (albedo/basecolor/diffuse) im Set")?;
    let (w, h) = (albedo.w, albedo.h);
    let fit = |i: &Rgba| if i.w == w && i.h == h { i.clone() } else { resize(i, w, h) };
    let mut base = albedo.clone();
    if let Some(ao) = get(PbrMap::Ao).map(fit) {
        for (p, a) in base.px.chunks_mut(4).zip(ao.px.chunks(4)) {
            let f = lum(a);
            for c in &mut p[..3] {
                *c = (*c as f32 * f).round() as u8;
            }
        }
    }
    let mut normal = match (get(PbrMap::NormalDx), get(PbrMap::NormalGl), get(PbrMap::Height)) {
        (Some(n), _, _) => Some(fit(n)),
        (None, Some(n), _) => {
            let mut n = fit(n);
            flip_green(&mut n);
            Some(n)
        }
        (None, None, Some(hm)) => Some(normal_from_height(&fit(hm), 4.0)),
        _ => None,
    };
    let rough = get(PbrMap::Roughness).map(fit);
    let metal = get(PbrMap::Metallic).map(fit);
    let mut params: Vec<(String, String)> = Vec::new();
    let mut avg_gloss = 0.0f32;
    if rough.is_some() || metal.is_some() {
        let n = (w * h) as usize;
        let mask: Vec<u8> = (0..n)
            .map(|i| {
                let r = rough.as_ref().map(|r| lum(&r.px[i * 4..])).unwrap_or(0.5);
                let m = metal.as_ref().map(|m| lum(&m.px[i * 4..])).unwrap_or(0.0);
                let g = (1.0 - r) * (1.0 - r) * (0.3 + 0.7 * m);
                avg_gloss += g;
                (g.clamp(0.0, 1.0) * 255.0).round() as u8
            })
            .collect();
        avg_gloss /= n as f32;
        let nm = normal.get_or_insert_with(|| Rgba { w, h, px: [128u8, 128, 255, 255].repeat(n) });
        for (p, m) in nm.px.chunks_mut(4).zip(mask) {
            p[3] = m;
        }
        if model {
            let exp = 4.0 + avg_gloss * 120.0;
            params.push(("$phong".into(), "1".into()));
            params.push(("$phongboost".into(), "2".into()));
            params.push(("$phongexponent".into(), format!("{exp:.0}")));
            params.push(("$phongfresnelranges".into(), "[.3 .6 1]".into()));
        } else {
            params.push(("$envmap".into(), "env_cubemap".into()));
            params.push(("$normalmapalphaenvmapmask".into(), "1".into()));
            params.push(("$envmaptint".into(), "[.6 .6 .6]".into()));
        }
    }
    Ok(PbrResult { base, normal, params })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Rgba {
        let mut px = Vec::new();
        for _y in 0..h {
            for x in 0..w {
                let v = (x * 255 / (w - 1)) as u8;
                px.extend([v, v, v, 255]);
            }
        }
        Rgba { w, h, px }
    }

    #[test]
    fn seamless_wraps() {
        let img = gradient(64, 32);
        // vorher: harter Sprung zwischen rechter und linker Kante
        let jump = |i: &Rgba| (i.px[(10 * 64 + 63) * 4] as i32 - i.px[(10 * 64) * 4] as i32).abs();
        assert!(jump(&img) > 200);
        let s = make_seamless(&img, 0.5);
        assert!(jump(&s) < 12, "{}", jump(&s));
        // Mitte bleibt unverändert
        assert_eq!(s.px[(16 * 64 + 32) * 4], img.px[(16 * 64 + 32) * 4]);
    }

    #[test]
    fn normals_from_height() {
        let flat = Rgba { w: 8, h: 8, px: [100u8, 100, 100, 255].repeat(64) };
        let n = normal_from_height(&flat, 4.0);
        assert_eq!(&n.px[..4], &[128, 128, 255, 255]);
        // nach rechts ansteigend → Normale zeigt nach links (rot < 128)
        let g = normal_from_height(&gradient(16, 4), 4.0);
        let p = &g.px[(16 + 8) * 4..];
        assert!(p[0] < 120 && (p[1] as i32 - 128).abs() <= 1, "{:?}", &p[..4]);
    }

    #[test]
    fn pbr_set() {
        assert_eq!(classify("Bricks076_2K_Color.jpg"), Some(PbrMap::Albedo));
        assert_eq!(classify("Bricks076_2K_NormalGL.png"), Some(PbrMap::NormalGl));
        assert_eq!(classify("Bricks076_2K_NormalDX.png"), Some(PbrMap::NormalDx));
        assert_eq!(classify("rock_wall_rough_2k.jpg"), Some(PbrMap::Roughness));
        assert_eq!(classify("rock_wall_nor_gl_2k.jpg"), Some(PbrMap::NormalGl));
        assert_eq!(classify("rock_wall_diff_2k.jpg"), Some(PbrMap::Albedo));
        assert_eq!(classify("metal_plate_metallic.png"), Some(PbrMap::Metallic));
        assert_eq!(classify("x_AmbientOcclusion.png"), Some(PbrMap::Ao));
        assert_eq!(classify("x_Displacement.png"), Some(PbrMap::Height));
        assert_eq!(classify("readme.txt"), None);
        let solid = |v: u8| Rgba { w: 4, h: 4, px: [v, v, v, 255].repeat(16) };
        let maps = vec![(PbrMap::Albedo, solid(200)), (PbrMap::Ao, solid(128)), (PbrMap::Roughness, solid(0)), (PbrMap::NormalGl, Rgba { w: 4, h: 4, px: [128u8, 200, 255, 255].repeat(16) })];
        let r = convert_pbr(&maps, false).unwrap();
        assert_eq!(r.base.px[0], 100, "Grundfarbe × AO");
        let n = r.normal.unwrap();
        assert_eq!(n.px[1], 55, "Grün umgedreht (OpenGL → DirectX)");
        assert_eq!(n.px[3], 77, "glatt, nicht metallisch → Maske 0,3");
        assert!(r.params.iter().any(|p| p.0 == "$normalmapalphaenvmapmask"));
        let m = convert_pbr(&maps, true).unwrap();
        assert!(m.params.iter().any(|p| p.0 == "$phong"));
        assert!(convert_pbr(&[(PbrMap::Roughness, solid(1))], false).is_err());
    }
}
