//! Bezier-Patches: ein bikubisches 4×4-Kontrollnetz formt eine Displacement-Fläche. Die
//! Kontrollpunkte werden in der 3D-Ansicht gezogen, das Displacement folgt der glatten Fläche –
//! damit lassen sich Bögen, Wellen und Kuppeln formen, die ganz normal kompilieren.

use crate::disp::{Disp, DispGeom};
use glam::DVec3;

/// Bernstein-Polynome 3. Grades
fn bern(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t]
}

/// Punkt auf dem Patch (u, v ∈ [0, 1]); `c[row * 4 + col]`, Zeile = v, Spalte = u
pub fn eval(c: &[DVec3; 16], u: f64, v: f64) -> DVec3 {
    let (bu, bv) = (bern(u), bern(v));
    let mut p = DVec3::ZERO;
    for (r, wr) in bv.iter().enumerate() {
        for (k, wk) in bu.iter().enumerate() {
            p += c[r * 4 + k] * (wr * wk);
        }
    }
    p
}

/// Kontrollnetz aus dem jetzigen Displacement: die Fläche wird an 4×4 Stellen abgetastet und die
/// inneren Punkte so gewählt, dass der Patch durch diese Stellen läuft (Interpolation)
pub fn from_disp(g: &DispGeom) -> [DVec3; 16] {
    let r = g.res;
    let sample = |u: f64, v: f64| -> DVec3 {
        // bilinear im Punktraster
        let (fx, fy) = (u * (r - 1) as f64, v * (r - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(r - 1), (y0 + 1).min(r - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let p = |x: usize, y: usize| g.pos[y * r + x];
        (p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx) * (1.0 - ty) + (p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx) * ty
    };
    let ts = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];
    let mut q = [DVec3::ZERO; 16];
    for (row, &v) in ts.iter().enumerate() {
        for (col, &u) in ts.iter().enumerate() {
            q[row * 4 + col] = sample(u, v);
        }
    }
    // Interpolation: B · C · Bᵀ = Q  →  C = B⁻¹ · Q · B⁻ᵀ (B = Bernstein-Werte an den Stellen ts)
    let b: [[f64; 4]; 4] = ts.map(bern);
    let inv = invert4(&b);
    let mut tmp = [DVec3::ZERO; 16];
    for i in 0..4 {
        for j in 0..4 {
            tmp[i * 4 + j] = (0..4).map(|k| q[k * 4 + j] * inv[i][k]).sum();
        }
    }
    let mut c = [DVec3::ZERO; 16];
    for i in 0..4 {
        for j in 0..4 {
            c[i * 4 + j] = (0..4).map(|k| tmp[i * 4 + k] * inv[j][k]).sum();
        }
    }
    c
}

fn invert4(m: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut a = *m;
    let mut inv = [[0.0; 4]; 4];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for col in 0..4 {
        let piv = (col..4).max_by(|&x, &y| a[x][col].abs().partial_cmp(&a[y][col].abs()).unwrap()).unwrap();
        a.swap(col, piv);
        inv.swap(col, piv);
        let d = a[col][col];
        for k in 0..4 {
            a[col][k] /= d;
            inv[col][k] /= d;
        }
        for r in 0..4 {
            if r != col {
                let f = a[r][col];
                for k in 0..4 {
                    a[r][k] -= f * a[col][k];
                    inv[r][k] -= f * inv[col][k];
                }
            }
        }
    }
    inv
}

/// Displacement auf den Patch setzen: jeder Punkt des Rasters liegt danach auf der Fläche
pub fn apply(d: &mut Disp, g: &DispGeom, c: &[DVec3; 16], face_normal: DVec3) {
    let r = g.res;
    for y in 0..r {
        for x in 0..r {
            let k = y * r + x;
            let p = eval(c, x as f64 / (r - 1) as f64, y as f64 / (r - 1) as f64);
            let fixed = d.offsets.get(k).copied().unwrap_or(DVec3::ZERO) + face_normal * d.elevation;
            d.set_vector(k, p - g.base[k] - fixed, face_normal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_and_bends() {
        // flaches Displacement (Power 3) auf einem 256er Würfel oben
        let mut m = crate::map::Map::new();
        let mut s = m.make_box(crate::geometry::Aabb::new(DVec3::ZERO, DVec3::new(256.0, 256.0, 64.0)), "X", 0.25);
        let top = (0..s.sides.len()).find(|&i| s.planes[i].is_some_and(|p| p.normal.z > 0.9)).unwrap();
        let n = s.planes[top].unwrap().normal;
        s.sides[top].disp = Disp::new_flat(3, &s.polys[top], n);
        s.update_geometry();
        let g = s.disps[top].clone().unwrap();
        // Kontrollnetz einer flachen Fläche ist flach und läuft durch die Ecken
        let c = from_disp(&g);
        assert!(c.iter().all(|p| (p.z - 64.0).abs() < 1e-6), "{c:?}");
        for (corner, (u, v)) in [(0, (0.0, 0.0)), (3, (1.0, 0.0)), (12, (0.0, 1.0)), (15, (1.0, 1.0))] {
            assert!((eval(&c, u, v) - c[corner]).length() < 1e-9);
        }
        // mittlere Kontrollpunkte anheben → Kuppel
        let mut c2 = c;
        for i in [5, 6, 9, 10] {
            c2[i].z += 120.0;
        }
        let mut d = s.sides[top].disp.clone().unwrap();
        apply(&mut d, &g, &c2, n);
        s.sides[top].disp = Some(d);
        s.update_geometry();
        let g2 = s.disps[top].as_ref().unwrap();
        let r = g2.res;
        let mid = g2.pos[(r / 2) * r + r / 2];
        let edge = g2.pos[r / 2];
        assert!(mid.z > 64.0 + 60.0, "Mitte {mid:?}");
        assert!((edge.z - 64.0).abs() < 1e-6, "Rand bleibt: {edge:?}");
        // Rückweg: Netz aus der gebogenen Fläche ergibt wieder dieselbe Form
        let c3 = from_disp(g2);
        let p = eval(&c3, 0.5, 0.5);
        // Abtaststellen bei 1/3, 2/3 liegen zwischen den Rasterpunkten → kleine Abweichung
        assert!((p - eval(&c2, 0.5, 0.5)).length() < 4.0, "{p:?}");
    }
}
