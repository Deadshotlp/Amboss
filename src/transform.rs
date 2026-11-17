//! Affine Transformationen (Drehen, Skalieren, Spiegeln, Scheren) und Source-Winkel.

use glam::{DMat3, DVec3};

/// p' = m · p + t
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Xform {
    pub m: DMat3,
    pub t: DVec3,
}

impl Xform {
    pub fn translate(d: DVec3) -> Xform {
        Xform { m: DMat3::IDENTITY, t: d }
    }

    /// Lineare Abbildung m um einen festen Punkt
    pub fn about(center: DVec3, m: DMat3) -> Xform {
        Xform { m, t: center - m * center }
    }

    /// Drehung um eine Weltachse (0 = X, 1 = Y, 2 = Z), rechtshändig, in Grad
    pub fn rotation(axis: usize, deg: f64) -> DMat3 {
        let r = deg.to_radians();
        match axis {
            0 => DMat3::from_rotation_x(r),
            1 => DMat3::from_rotation_y(r),
            _ => DMat3::from_rotation_z(r),
        }
    }

    pub fn mirror(axis: usize) -> DMat3 {
        let mut s = DVec3::ONE;
        s[axis] = -1.0;
        DMat3::from_diagonal(s)
    }

    /// Scheren: Achse `a` wird um k · (Koordinate auf Achse `b`) verschoben
    pub fn shear(a: usize, b: usize, k: f64) -> DMat3 {
        let mut m = DMat3::IDENTITY;
        // Spalte b, Zeile a
        let mut col = m.col(b);
        col[a] = k;
        match b {
            0 => m.x_axis = col,
            1 => m.y_axis = col,
            _ => m.z_axis = col,
        }
        m
    }

    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.m * p + self.t
    }

    pub fn inv_transpose(&self) -> DMat3 {
        self.m.inverse().transpose()
    }

    pub fn normal(&self, n: DVec3) -> DVec3 {
        (self.inv_transpose() * n).normalize_or_zero()
    }

    pub fn is_valid(&self) -> bool {
        self.m.determinant().abs() > 1e-9 && self.m.is_finite() && self.t.is_finite()
    }

    /// erst self, dann other
    pub fn then(&self, other: &Xform) -> Xform {
        Xform { m: other.m * self.m, t: other.m * self.t + other.t }
    }
}

/// Source-Winkel (Pitch Yaw Roll) → Basis (vorwärts, links, oben)
pub fn angles_to_basis(a: DVec3) -> (DVec3, DVec3, DVec3) {
    let (sp, cp) = a.x.to_radians().sin_cos();
    let (sy, cy) = a.y.to_radians().sin_cos();
    let (sr, cr) = a.z.to_radians().sin_cos();
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let right = DVec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = DVec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    (forward, -right, up)
}

pub fn angles_to_mat(a: DVec3) -> DMat3 {
    let (f, l, u) = angles_to_basis(a);
    DMat3::from_cols(f, l, u)
}

/// Basis → Source-Winkel (wie MatrixAngles im SDK)
pub fn basis_to_angles(f: DVec3, l: DVec3, u: DVec3) -> DVec3 {
    let xy = (f.x * f.x + f.y * f.y).sqrt();
    let clean = |v: f64| {
        let r = (v * 1000.0).round() / 1000.0;
        if r == 0.0 {
            0.0
        } else {
            r
        }
    };
    if xy > 0.001 {
        let yaw = f.y.atan2(f.x).to_degrees();
        let pitch = (-f.z).atan2(xy).to_degrees();
        let roll = l.z.atan2(u.z).to_degrees();
        DVec3::new(clean(pitch), clean(yaw.rem_euclid(360.0)), clean(roll))
    } else {
        let yaw = (-l.x).atan2(l.y).to_degrees();
        let pitch = (-f.z).atan2(xy).to_degrees();
        DVec3::new(clean(pitch), clean(yaw.rem_euclid(360.0)), 0.0)
    }
}

/// Entity-Winkel mit einer Transformation verändern (auch bei Spiegelung sinnvoll)
pub fn transform_angles(x: &Xform, a: DVec3) -> DVec3 {
    let (f, _l, u) = angles_to_basis(a);
    let f2 = (x.m * f).normalize_or_zero();
    let u2 = (x.m * u).normalize_or_zero();
    if f2 == DVec3::ZERO || u2 == DVec3::ZERO {
        return a;
    }
    let l2 = u2.cross(f2).normalize_or_zero();
    let u3 = f2.cross(l2).normalize_or_zero();
    basis_to_angles(f2, l2, u3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_roundtrip() {
        for a in [DVec3::new(0.0, 90.0, 0.0), DVec3::new(30.0, 45.0, 10.0), DVec3::new(-60.0, 200.0, -15.0)] {
            let (f, l, u) = angles_to_basis(a);
            let b = basis_to_angles(f, l, u);
            let (f2, _, u2) = angles_to_basis(b);
            assert!((f - f2).length() < 1e-6 && (u - u2).length() < 1e-6, "{a} -> {b}");
        }
        let (f, _, _) = angles_to_basis(DVec3::new(0.0, 90.0, 0.0));
        assert!((f - DVec3::Y).length() < 1e-9);
    }

    #[test]
    fn rotate_angles() {
        let x = Xform::about(DVec3::ZERO, Xform::rotation(2, 90.0));
        let a = transform_angles(&x, DVec3::new(0.0, 0.0, 0.0));
        assert!((a.y - 90.0).abs() < 1e-6);
        let s = Xform::shear(0, 1, 0.5);
        assert_eq!(s * DVec3::new(0.0, 2.0, 0.0), DVec3::new(1.0, 2.0, 0.0));
    }
}
