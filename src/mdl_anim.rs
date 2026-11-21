//! Animationen aus Source-Modellen: Skelett, Sequenzen und Animationsdaten lesen
//! (auch aus .ani-Dateien und per $includemodel eingebundenen Modellen) und die Vertices
//! eines Bilds auf der CPU verformen (Skinning).

use crate::gamefs::GameFs;
use glam::{Mat4, Quat, Vec3, Vec4};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub pos: Vec3,
    pub quat: Quat,
    /// Euler-Winkel der Ruhelage (Basis für komprimierte Drehungen)
    pub rot: Vec3,
    pub posscale: Vec3,
    pub rotscale: Vec3,
    /// Modellraum → Knochenraum (Ruhelage)
    pub pose_to_bone: Mat4,
}

/// Eine Datei mit Animationen: das Modell selbst oder ein eingebundenes
#[derive(Debug)]
struct Source {
    mdl: Arc<Vec<u8>>,
    ani: Option<Arc<Vec<u8>>>,
    bones: Vec<Bone>,
    /// Knochen dieser Datei → Knochen des Modells
    map: Vec<Option<usize>>,
}

#[derive(Debug, Clone)]
pub struct Seq {
    pub name: String,
    pub frames: usize,
    pub fps: f32,
    pub looping: bool,
    src: usize,
    /// absoluter Offset der Animationsbeschreibung (mstudioanimdesc_t) in der Quelldatei
    animdesc: usize,
    delta: bool,
}

#[derive(Debug)]
pub struct Anim {
    pub bones: Vec<Bone>,
    pub seqs: Vec<Seq>,
    sources: Vec<Source>,
}

// ------------------------------------------------------------------ Lesehilfen

fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    b.get(o..o + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap()))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    b.get(o..o + 2).map(|x| i16::from_le_bytes(x.try_into().unwrap()))
}
fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes(x.try_into().unwrap()))
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()))
}
fn vec3_at(b: &[u8], o: usize) -> Option<Vec3> {
    Some(Vec3::new(f32_at(b, o)?, f32_at(b, o + 4)?, f32_at(b, o + 8)?))
}
fn cstr_at(b: &[u8], o: usize) -> String {
    let Some(s) = b.get(o..) else { return String::new() };
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).to_string()
}
fn rel(base: usize, r: i32) -> usize {
    (base as i64 + r as i64).max(0) as usize
}

/// 16-Bit-Gleitkommazahl (Vector48)
fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as f32;
    s * match e {
        0 => m / 1024.0 * 2f32.powi(-14),
        31 => 65504.0,
        _ => (1.0 + m / 1024.0) * 2f32.powi(e - 15),
    }
}

/// Euler-Winkel (Source RadianEuler: x = Rollen, y = Neigen, z = Gieren) → Quaternion
pub fn euler_quat(a: Vec3) -> Quat {
    let (sr, cr) = (a.x * 0.5).sin_cos();
    let (sp, cp) = (a.y * 0.5).sin_cos();
    let (sy, cy) = (a.z * 0.5).sin_cos();
    Quat::from_xyzw(sr * cp * cy - cr * sp * sy, cr * sp * cy + sr * cp * sy, cr * cp * sy - sr * sp * cy, cr * cp * cy + sr * sp * sy)
}

fn quat48(b: &[u8], o: usize) -> Option<Quat> {
    let x = u16_at(b, o)? as f32;
    let y = u16_at(b, o + 2)? as f32;
    let zw = u16_at(b, o + 4)?;
    let (x, y, z) = ((x - 32768.0) / 32768.0, (y - 32768.0) / 32768.0, ((zw & 0x7FFF) as f32 - 16384.0) / 16384.0);
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    Some(Quat::from_xyzw(x, y, z, if zw & 0x8000 != 0 { -w } else { w }))
}

fn quat64(b: &[u8], o: usize) -> Option<Quat> {
    let v = u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?);
    let f = |sh: u32| ((v >> sh) & 0x1F_FFFF) as f64;
    let (x, y, z) = ((f(0) - 1048576.0) / 1048576.5, (f(21) - 1048576.0) / 1048576.5, (f(42) - 1048576.0) / 1048576.5);
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    Some(Quat::from_xyzw(x as f32, y as f32, z as f32, if v >> 63 != 0 { -w as f32 } else { w as f32 }))
}

/// Ein Wert aus einer lauflängenkodierten Kurve (mstudioanimvalue_t) für Bild `frame`
fn anim_value(b: &[u8], mut o: usize, frame: usize) -> f32 {
    let mut k = frame;
    for _ in 0..10_000 {
        let (Some(&valid), Some(&total)) = (b.get(o), b.get(o + 1)) else { return 0.0 };
        if total == 0 {
            return 0.0;
        }
        if (total as usize) <= k {
            k -= total as usize;
            o += (valid as usize + 1) * 2;
            continue;
        }
        let idx = if (valid as usize) > k { k + 1 } else { valid as usize };
        return i16_at(b, o + idx * 2).unwrap_or(0) as f32;
    }
    0.0
}

/// Drei Kurven (mstudioanim_valueptr_t: drei Offsets relativ zum Anfang)
fn anim_vec(b: &[u8], o: usize, frame: usize) -> Vec3 {
    let mut v = [0.0f32; 3];
    for (k, x) in v.iter_mut().enumerate() {
        let off = i16_at(b, o + k * 2).unwrap_or(0);
        if off > 0 {
            *x = anim_value(b, o + off as usize, frame);
        }
    }
    Vec3::from(v)
}

// ------------------------------------------------------------------ Laden

fn parse_bones(b: &[u8]) -> Vec<Bone> {
    let n = i32_at(b, 156).unwrap_or(0).clamp(0, 512) as usize;
    let idx = i32_at(b, 160).unwrap_or(0).max(0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let o = idx + i * 216;
        let Some(m) = (0..12).map(|k| f32_at(b, o + 96 + k * 4)).collect::<Option<Vec<f32>>>() else { break };
        let (Some(pos), Some(rot), Some(posscale), Some(rotscale)) = (vec3_at(b, o + 32), vec3_at(b, o + 60), vec3_at(b, o + 72), vec3_at(b, o + 84)) else { break };
        let q = Quat::from_xyzw(f32_at(b, o + 44).unwrap_or(0.0), f32_at(b, o + 48).unwrap_or(0.0), f32_at(b, o + 52).unwrap_or(0.0), f32_at(b, o + 56).unwrap_or(1.0));
        out.push(Bone {
            name: cstr_at(b, rel(o, i32_at(b, o).unwrap_or(0))),
            parent: i32_at(b, o + 4).unwrap_or(-1),
            pos,
            quat: if q.length_squared() > 1e-6 { q.normalize() } else { Quat::IDENTITY },
            rot,
            posscale,
            rotscale,
            pose_to_bone: Mat4::from_cols(Vec4::new(m[0], m[4], m[8], 0.0), Vec4::new(m[1], m[5], m[9], 0.0), Vec4::new(m[2], m[6], m[10], 0.0), Vec4::new(m[3], m[7], m[11], 1.0)),
        });
    }
    out
}

fn parse_seqs(b: &[u8], src: usize, out: &mut Vec<Seq>, seen: &mut std::collections::HashSet<String>) {
    let n = i32_at(b, 188).unwrap_or(0).clamp(0, 4096) as usize;
    let idx = i32_at(b, 192).unwrap_or(0).max(0) as usize;
    let nanim = i32_at(b, 180).unwrap_or(0).max(0) as usize;
    let animidx = i32_at(b, 184).unwrap_or(0).max(0) as usize;
    for i in 0..n {
        let o = idx + i * 212;
        if o + 212 > b.len() {
            break;
        }
        let name = cstr_at(b, rel(o, i32_at(b, o + 4).unwrap_or(0)));
        // gleichnamige Sequenzen: die des Modells gewinnt (wie in der Engine)
        if !seen.insert(name.to_ascii_lowercase()) {
            continue;
        }
        let flags = i32_at(b, o + 12).unwrap_or(0);
        let numblends = i32_at(b, o + 56).unwrap_or(1).max(1);
        let gs0 = i32_at(b, o + 68).unwrap_or(1).max(1);
        let gs1 = i32_at(b, o + 72).unwrap_or(1).max(1);
        // Mischsequenzen (z. B. Zielen): die mittlere Animation
        let blend = if numblends > 1 { (gs0 / 2 + (gs1 / 2) * gs0).min(numblends - 1) } else { 0 };
        let aii = rel(o, i32_at(b, o + 60).unwrap_or(0));
        let Some(a) = i16_at(b, aii + blend as usize * 2) else { continue };
        let a = a.max(0) as usize;
        if a >= nanim {
            continue;
        }
        let ad = animidx + a * 100;
        let (Some(fps), Some(aflags), Some(frames)) = (f32_at(b, ad + 8), i32_at(b, ad + 12), i32_at(b, ad + 16)) else { continue };
        out.push(Seq { name, frames: frames.max(1) as usize, fps: if fps > 0.0 && fps.is_finite() { fps } else { 30.0 }, looping: flags & 1 != 0, src, animdesc: ad, delta: aflags & 4 != 0 });
    }
}

fn read_ani(fs: &GameFs, b: &[u8]) -> Option<Arc<Vec<u8>>> {
    let name = cstr_at(b, i32_at(b, 348)?.max(0) as usize);
    if name.is_empty() || i32_at(b, 352).unwrap_or(0) <= 0 {
        return None;
    }
    fs.read(&name.replace('\\', "/").to_ascii_lowercase()).map(Arc::new)
}

fn make_source(mdl: Arc<Vec<u8>>, ani: Option<Arc<Vec<u8>>>, main: &[Bone]) -> Source {
    let bones = parse_bones(&mdl);
    let map = bones.iter().map(|sb| main.iter().position(|mb| mb.name.eq_ignore_ascii_case(&sb.name))).collect();
    Source { mdl, ani, bones, map }
}

/// Skelett und Sequenzen aus den Dateibytes; `include` liefert eingebundene Modelle (Bytes, .ani)
pub fn parse(mdl: Arc<Vec<u8>>, ani: Option<Arc<Vec<u8>>>, include: &mut dyn FnMut(&str) -> Option<(Arc<Vec<u8>>, Option<Arc<Vec<u8>>>)>) -> Option<Arc<Anim>> {
    let bones = parse_bones(&mdl);
    if bones.is_empty() {
        return None;
    }
    let mut seqs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut sources = vec![make_source(mdl.clone(), ani, &bones)];
    parse_seqs(&mdl, 0, &mut seqs, &mut seen);
    let ninc = i32_at(&mdl, 336).unwrap_or(0).clamp(0, 64) as usize;
    let inc_idx = i32_at(&mdl, 340).unwrap_or(0).max(0) as usize;
    for i in 0..ninc {
        let o = inc_idx + i * 8;
        let name = cstr_at(&mdl, rel(o, i32_at(&mdl, o + 4).unwrap_or(0))).replace('\\', "/").to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        let name = if name.starts_with("models/") { name } else { format!("models/{name}") };
        if let Some((b, a)) = include(&name) {
            if b.get(0..4) != Some(b"IDST") {
                continue;
            }
            let k = sources.len();
            parse_seqs(&b, k, &mut seqs, &mut seen);
            sources.push(make_source(b, a, &bones));
        }
    }
    // Sequenzen mit nur einem Bild und ohne Bewegung (z. B. „ref“) bleiben in der Liste: sie zeigen die Grundhaltung
    if seqs.is_empty() {
        return None;
    }
    Some(Arc::new(Anim { bones, seqs, sources }))
}

pub fn load(fs: &GameFs, mdl_b: &[u8]) -> Option<Arc<Anim>> {
    let mdl = Arc::new(mdl_b.to_vec());
    let ani = read_ani(fs, mdl_b);
    parse(mdl, ani, &mut |name| {
        let b = fs.read(name)?;
        let a = read_ani(fs, &b);
        Some((Arc::new(b), a))
    })
}

// ------------------------------------------------------------------ Pose berechnen

impl Anim {
    /// Daten einer Animation für Bild `frame`: (Bytes, Offset des ersten mstudioanim_t, Bild im Abschnitt)
    fn anim_data(&self, seq: &Seq, frame: usize) -> Option<(&[u8], usize, usize)> {
        let src = self.sources.get(seq.src)?;
        let b: &[u8] = &src.mdl;
        let ad = seq.animdesc;
        let numframes = i32_at(b, ad + 16)?.max(1) as usize;
        let sectionframes = i32_at(b, ad + 84).unwrap_or(0).max(0) as usize;
        let mut f = frame.min(numframes - 1);
        let (block, index) = if sectionframes > 0 {
            let section = if numframes > sectionframes && f == numframes - 1 {
                f = 0;
                numframes / sectionframes + 1
            } else {
                let s = f / sectionframes;
                f -= s * sectionframes;
                s
            };
            let so = rel(ad, i32_at(b, ad + 80)?) + section * 8;
            (i32_at(b, so)?, i32_at(b, so + 4)?)
        } else {
            (i32_at(b, ad + 52)?, i32_at(b, ad + 56)?)
        };
        if block < 0 {
            return None;
        }
        if block == 0 {
            return Some((b, rel(ad, index), f));
        }
        let ani: &[u8] = src.ani.as_deref()?;
        let nblocks = i32_at(b, 352)?;
        if block >= nblocks {
            return None;
        }
        let start = i32_at(b, i32_at(b, 356)?.max(0) as usize + block as usize * 8)?;
        Some((ani, rel(start.max(0) as usize, index), f))
    }

    /// Lokale Lage (Position, Drehung) aller Knochen in einem ganzen Bild
    fn local_pose(&self, seq: &Seq, frame: usize) -> Vec<(Vec3, Quat)> {
        let mut pose: Vec<(Vec3, Quat)> = self.bones.iter().map(|b| (b.pos, b.quat)).collect();
        let Some(src) = self.sources.get(seq.src) else { return pose };
        let Some((b, mut o, f)) = self.anim_data(seq, frame) else { return pose };
        for _ in 0..1024 {
            let (Some(&bone), Some(&flags), Some(next)) = (b.get(o), b.get(o + 1), i16_at(b, o + 2)) else { break };
            let bone = bone as usize;
            if let (Some(sb), Some(Some(target))) = (src.bones.get(bone), src.map.get(bone)) {
                let delta = seq.delta || flags & 0x10 != 0;
                let mut d = o + 4;
                let mut q = None;
                if flags & 0x02 != 0 {
                    q = quat48(b, d);
                    d += 6;
                }
                if flags & 0x20 != 0 {
                    q = quat64(b, d);
                    d += 8;
                }
                if flags & 0x08 != 0 {
                    let a = anim_vec(b, d, f) * sb.rotscale;
                    q = Some(euler_quat(if delta { a } else { sb.rot + a }));
                    d += 6;
                }
                let mut p = None;
                if flags & 0x01 != 0 {
                    p = Some(Vec3::new(half(u16_at(b, d).unwrap_or(0)), half(u16_at(b, d + 2).unwrap_or(0)), half(u16_at(b, d + 4).unwrap_or(0))));
                } else if flags & 0x04 != 0 {
                    let a = anim_vec(b, d, f) * sb.posscale;
                    p = Some(if delta { a } else { sb.pos + a });
                }
                let entry = &mut pose[*target];
                if delta {
                    // Ebene auf die Grundhaltung addieren
                    if let Some(q) = q {
                        entry.1 = (entry.1 * q).normalize();
                    }
                    if let Some(p) = p {
                        entry.0 += p;
                    }
                } else {
                    if let Some(q) = q {
                        entry.1 = q.normalize();
                    }
                    if let Some(p) = p {
                        entry.0 = p;
                    }
                }
            }
            if next <= 0 {
                break;
            }
            o += next as usize;
        }
        pose
    }

    /// Skinning-Matrizen (Modellraum Ruhelage → Modellraum Bild) zum Zeitpunkt `frame` (mit Bruchteil)
    pub fn skin_matrices(&self, seq: usize, frame: f32) -> Vec<Mat4> {
        let Some(s) = self.seqs.get(seq) else { return vec![Mat4::IDENTITY; self.bones.len()] };
        let n = s.frames.max(1);
        let f = frame.max(0.0);
        let f0 = (f.floor() as usize).min(n - 1);
        let f1 = if s.looping { (f0 + 1) % n } else { (f0 + 1).min(n - 1) };
        let t = (f - f.floor()).clamp(0.0, 1.0);
        let a = self.local_pose(s, f0);
        let b = if f1 != f0 && t > 0.001 { self.local_pose(s, f1) } else { a.clone() };
        let mut world: Vec<Mat4> = Vec::with_capacity(self.bones.len());
        for (i, bone) in self.bones.iter().enumerate() {
            let q = if a[i].1.dot(b[i].1) < 0.0 { a[i].1.slerp(-b[i].1, t) } else { a[i].1.slerp(b[i].1, t) };
            let local = Mat4::from_rotation_translation(q.normalize(), a[i].0.lerp(b[i].0, t));
            let w = match usize::try_from(bone.parent).ok().filter(|&p| p < i) {
                Some(p) => world[p] * local,
                None => local,
            };
            world.push(w);
        }
        world.iter().zip(&self.bones).map(|(w, b)| *w * b.pose_to_bone).collect()
    }
}

/// Vertices verformen: pos(3) normal(3) uv(2) mit Gewichten
pub fn skin(verts: &[[f32; 8]], weights: &[[(u8, f32); 3]], mats: &[Mat4]) -> Vec<[f32; 8]> {
    verts
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let p = Vec3::new(v[0], v[1], v[2]);
            let n = Vec3::new(v[3], v[4], v[5]);
            let (mut po, mut no) = (Vec3::ZERO, Vec3::ZERO);
            let mut sum = 0.0;
            for &(bone, w) in weights.get(i).map(|w| &w[..]).unwrap_or(&[]) {
                if w <= 0.0 {
                    continue;
                }
                let Some(m) = mats.get(bone as usize) else { continue };
                po += m.transform_point3(p) * w;
                no += m.transform_vector3(n) * w;
                sum += w;
            }
            if sum <= 1e-6 {
                return *v;
            }
            po /= sum;
            let no = no.normalize_or_zero();
            [po.x, po.y, po.z, no.x, no.y, no.z, v[6], v[7]]
        })
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn put(b: &mut Vec<u8>, o: usize, d: &[u8]) {
        if b.len() < o + d.len() {
            b.resize(o + d.len(), 0);
        }
        b[o..o + d.len()].copy_from_slice(d);
    }
    fn pi(b: &mut Vec<u8>, o: usize, v: i32) {
        put(b, o, &v.to_le_bytes());
    }
    fn pf(b: &mut Vec<u8>, o: usize, v: f32) {
        put(b, o, &v.to_le_bytes());
    }

    /// Skelett mit zwei Knochen („root“ im Ursprung, „arm“ bei x = 10) und einer Sequenz „wave“
    /// mit 3 Bildern: der Arm dreht sich um z von 0 auf 90 Grad (komprimierte Kurve)
    pub fn make_anim_mdl() -> Vec<u8> {
        let mut b = vec![0u8; 400];
        b[0..4].copy_from_slice(b"IDST");
        pi(&mut b, 4, 48);
        // Knochen bei 1000
        pi(&mut b, 156, 2);
        pi(&mut b, 160, 1000);
        for (i, (name_off, parent, x)) in [(2000usize, -1i32, 0.0f32), (2010, 0, 10.0)].iter().enumerate() {
            let o = 1000 + i * 216;
            pi(&mut b, o, *name_off as i32 - o as i32);
            pi(&mut b, o + 4, *parent);
            pf(&mut b, o + 32, *x);
            pf(&mut b, o + 56, 1.0); // quat w
            for k in 0..3 {
                pf(&mut b, o + 72 + k * 4, 1.0);
                pf(&mut b, o + 84 + k * 4, std::f32::consts::FRAC_PI_2 / 100.0);
            }
            // poseToBone = Inverse der Weltlage (nur Verschiebung)
            let wx = if i == 0 { 0.0 } else { 10.0 };
            for (k, v) in [1.0, 0.0, 0.0, -wx, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().enumerate() {
                pf(&mut b, o + 96 + k * 4, *v);
            }
        }
        put(&mut b, 2000, b"root\0");
        put(&mut b, 2010, b"arm\0");
        // Animation bei 2100
        pi(&mut b, 180, 1);
        pi(&mut b, 184, 2100);
        pf(&mut b, 2100 + 8, 10.0);
        pi(&mut b, 2100 + 16, 3);
        pi(&mut b, 2100 + 56, 100); // Daten bei 2200
        // mstudioanim_t: Knochen 1, ANIMROT, letzter Eintrag
        put(&mut b, 2200, &[1, 0x08]);
        put(&mut b, 2202, &0i16.to_le_bytes());
        // valueptr: x = 0, y = 0, z → +6 (relativ zum valueptr bei 2204 → 2210)
        put(&mut b, 2204, &0i16.to_le_bytes());
        put(&mut b, 2206, &0i16.to_le_bytes());
        put(&mut b, 2208, &6i16.to_le_bytes());
        // Kurve: valid 3, total 3, Werte 0, 50, 100 (× rotscale = 0 … 90°)
        put(&mut b, 2210, &[3, 3]);
        for (k, v) in [0i16, 50, 100].iter().enumerate() {
            put(&mut b, 2212 + k * 2, &v.to_le_bytes());
        }
        // Sequenz bei 2300
        pi(&mut b, 188, 1);
        pi(&mut b, 192, 2300);
        pi(&mut b, 2300 + 4, 2600 - 2300);
        pi(&mut b, 2300 + 12, 1); // looping
        pi(&mut b, 2300 + 56, 1);
        pi(&mut b, 2300 + 60, 2550 - 2300);
        put(&mut b, 2550, &0i16.to_le_bytes());
        put(&mut b, 2600, b"wave\0");
        b.resize(2700, 0);
        b
    }

    #[test]
    fn decode_and_skin() {
        let a = parse(Arc::new(make_anim_mdl()), None, &mut |_| None).expect("Animation");
        assert_eq!(a.bones.len(), 2);
        assert_eq!(a.seqs.len(), 1);
        assert_eq!(a.seqs[0].name, "wave");
        assert_eq!(a.seqs[0].frames, 3);
        assert!(a.seqs[0].looping);
        // Ein Vertex bei x = 20 hängt am Arm: Bild 0 unverändert, Bild 2 um 90° um (10, 0, 0) gedreht
        let v = [[20.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]];
        let w = [[(1u8, 1.0f32), (0, 0.0), (0, 0.0)]];
        let s0 = skin(&v, &w, &a.skin_matrices(0, 0.0));
        assert!((s0[0][0] - 20.0).abs() < 1e-3 && s0[0][1].abs() < 1e-3, "{:?}", s0[0]);
        let s2 = skin(&v, &w, &a.skin_matrices(0, 2.0));
        assert!((s2[0][0] - 10.0).abs() < 1e-3 && (s2[0][1] - 10.0).abs() < 1e-3, "{:?}", s2[0]);
        // Normale dreht mit
        assert!((s2[0][4] - 1.0).abs() < 1e-3);
        // Zwischenbild 1,5: 67,5°
        let s15 = skin(&v, &w, &a.skin_matrices(0, 1.5));
        let ang = (s15[0][1]).atan2(s15[0][0] - 10.0).to_degrees();
        assert!((ang - 67.5).abs() < 0.1, "{ang}");
        // Ein Vertex am Wurzelknochen bleibt
        let r = skin(&v, &[[(0, 1.0), (0, 0.0), (0, 0.0)]], &a.skin_matrices(0, 2.0));
        assert!((r[0][0] - 20.0).abs() < 1e-3);
    }

    /// Testmodell (ein Dreieck) mit Skelett und Animation: Vertex 1 (32, 0, 0) hängt am Arm
    pub fn make_animated_model() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (mut mdl, mut vvd, vtx) = crate::mdl::tests::make_test_model();
        let anim = make_anim_mdl();
        mdl.resize(anim.len(), 0);
        mdl[1000..].copy_from_slice(&anim[1000..]);
        for o in [156, 160, 180, 184, 188, 192] {
            mdl[o..o + 4].copy_from_slice(&anim[o..o + 4]);
        }
        // boneweight_t von Vertex 1: Gewicht 1, Knochen 1, ein Knochen
        let v1 = 64 + 48;
        vvd[v1..v1 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        vvd[v1 + 12] = 1;
        vvd[v1 + 15] = 1;
        (mdl, vvd, vtx)
    }

    #[test]
    fn animated_model_pipeline() {
        let (mdl, vvd, vtx) = make_animated_model();
        let m = crate::mdl::parse("models/test/winker.mdl", &mdl, &vvd, &vtx, &|_| false).unwrap();
        assert_eq!(m.meshes[0].weights[1][0], (1, 1.0));
        assert_eq!(m.meshes[0].weights[0][0], (0, 1.0));
        let a = parse(Arc::new(mdl), None, &mut |_| None).unwrap();
        let mesh = &m.meshes[0];
        let p = skin(&mesh.verts, &mesh.weights, &a.skin_matrices(0, 2.0));
        let i = mesh.verts.iter().position(|v| v[0] == 32.0).unwrap();
        assert!((p[i][0] - 10.0).abs() < 1e-3 && (p[i][1] - 22.0).abs() < 1e-3, "{:?}", p[i]);
        let (data, _) = crate::view3d::static_from_model_posed(&m, 0, Some(&a.skin_matrices(0, 2.0)));
        assert!(!data.is_empty());
    }

    #[test]
    fn compressed_types() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0xC000), -2.0);
        assert!((half(0x3555) - 0.333).abs() < 0.001);
        // Quaternion48 der Identität: x = y = 32768, z = 16384, w positiv
        let mut b = Vec::new();
        b.extend_from_slice(&32768u16.to_le_bytes());
        b.extend_from_slice(&32768u16.to_le_bytes());
        b.extend_from_slice(&16384u16.to_le_bytes());
        let q = quat48(&b, 0).unwrap();
        assert!((q.w - 1.0).abs() < 1e-4);
        let v: u64 = 1048576 | (1048576 << 21) | (1048576 << 42);
        let q = quat64(&v.to_le_bytes(), 0).unwrap();
        assert!((q.w - 1.0).abs() < 1e-4);
        // Euler: 90° Gieren = Drehung um z
        let q = euler_quat(Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2));
        let r = q * Vec3::X;
        assert!((r.y - 1.0).abs() < 1e-5);
        // Lauflänge: valid 2, total 4 → Werte 7, 9, dann 9 wiederholt; danach neuer Lauf mit 5
        let mut c = vec![2u8, 4];
        for v in [7i16, 9] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c.extend_from_slice(&[1, 2]);
        c.extend_from_slice(&5i16.to_le_bytes());
        assert_eq!((0..6).map(|f| anim_value(&c, 0, f)).collect::<Vec<_>>(), vec![7.0, 9.0, 9.0, 9.0, 5.0, 5.0]);
    }
}
