//! Binäres DMX (Valves Datenformat für .pcf-Partikeldateien) vollständig lesen (Versionen 2–5)
//! und schreiben (Version 2, die alle Source-Spiele laden).

/// Wert eines Attributs
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    /// Verweis auf ein Element (Index), -1 = leer
    Element(i32),
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Binary(Vec<u8>),
    /// Zeit in Zehntausendstel Sekunden
    Time(i32),
    Color([u8; 4]),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    QAngle([f32; 3]),
    Quat([f32; 4]),
    Matrix([f32; 16]),
    /// Liste mit dem Grundtyp (1–14) und den Werten
    Array(u8, Vec<Val>),
}

impl Val {
    /// Typnummer im Dateiformat
    pub fn type_id(&self) -> u8 {
        match self {
            Val::Element(_) => 1,
            Val::Int(_) => 2,
            Val::Float(_) => 3,
            Val::Bool(_) => 4,
            Val::Str(_) => 5,
            Val::Binary(_) => 6,
            Val::Time(_) => 7,
            Val::Color(_) => 8,
            Val::Vec2(_) => 9,
            Val::Vec3(_) => 10,
            Val::Vec4(_) => 11,
            Val::QAngle(_) => 12,
            Val::Quat(_) => 13,
            Val::Matrix(_) => 14,
            Val::Array(t, _) => t + 14,
        }
    }
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Val::Float(f) => Some(*f),
            Val::Int(i) => Some(*i as f32),
            Val::Time(t) => Some(*t as f32 / 10000.0),
            Val::Bool(b) => Some(*b as i32 as f32),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub ty: String,
    pub name: String,
    pub guid: [u8; 16],
    pub attrs: Vec<(String, Val)>,
}

impl Element {
    pub fn new(ty: &str, name: &str) -> Element {
        Element { ty: ty.into(), name: name.into(), guid: new_guid(), attrs: Vec::new() }
    }
    pub fn get(&self, k: &str) -> Option<&Val> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut Val> {
        self.attrs.iter_mut().find(|(a, _)| a == k).map(|(_, v)| v)
    }
    pub fn set(&mut self, k: &str, v: Val) {
        match self.get_mut(k) {
            Some(x) => *x = v,
            None => self.attrs.push((k.to_string(), v)),
        }
    }
    pub fn f(&self, k: &str, def: f32) -> f32 {
        self.get(k).and_then(Val::as_f32).unwrap_or(def)
    }
    pub fn str(&self, k: &str) -> Option<&str> {
        match self.get(k) {
            Some(Val::Str(s)) => Some(s),
            _ => None,
        }
    }
    /// Verweise einer Element-Liste
    pub fn refs(&self, k: &str) -> Vec<usize> {
        match self.get(k) {
            Some(Val::Array(1, v)) => v.iter().filter_map(|x| if let Val::Element(i) = x { (*i >= 0).then_some(*i as usize) } else { None }).collect(),
            _ => Vec::new(),
        }
    }
}

/// Zufällige GUID (Zeit + Zähler – eindeutig genug für neue Elemente)
pub fn new_guid() -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut g = [0u8; 16];
    g[..8].copy_from_slice(&t.to_le_bytes());
    g[8..].copy_from_slice(&(n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ t.rotate_left(17)).to_le_bytes());
    g
}

/// Eine DMX-Datei: Elemente, das erste ist die Wurzel
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Dmx {
    pub elements: Vec<Element>,
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or("Datei zu kurz")?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn fs<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut a = [0f32; N];
        for x in &mut a {
            *x = self.f32()?;
        }
        Ok(a)
    }
    fn cstr(&mut self) -> Result<String, String> {
        let end = self.b.get(self.p..).ok_or("Datei zu kurz")?.iter().position(|&c| c == 0).ok_or("Text ohne Ende")?;
        let s = String::from_utf8_lossy(&self.b[self.p..self.p + end]).to_string();
        self.p += end + 1;
        Ok(s)
    }
}

/// Binäres DMX lesen
pub fn read(b: &[u8]) -> Result<Dmx, String> {
    let hdr_end = b.iter().position(|&c| c == 0).ok_or("kein DMX")?;
    let hdr = String::from_utf8_lossy(&b[..hdr_end]).to_string();
    let ver: u32 = hdr.split("binary").nth(1).and_then(|s| s.split_whitespace().next()).and_then(|s| s.parse().ok()).ok_or("kein binäres DMX")?;
    if !(2..=5).contains(&ver) {
        return Err(format!("DMX-Version {ver} wird nicht unterstützt"));
    }
    let idx32 = ver >= 4;
    let mut r = R { b, p: hdr_end + 1 };
    let count = r.i32()?.max(0);
    let mut strings = Vec::with_capacity(count as usize);
    for _ in 0..count {
        strings.push(r.cstr()?);
    }
    let idx = |r: &mut R| -> Result<String, String> {
        let i = if idx32 { r.i32()? as usize } else { r.i16()? as u16 as usize };
        strings.get(i).cloned().ok_or_else(|| "String-Index außerhalb".to_string())
    };
    let n_el = r.i32()?.max(0) as usize;
    let mut elements = Vec::with_capacity(n_el);
    for _ in 0..n_el {
        let ty = idx(&mut r)?;
        let name = if ver >= 4 { idx(&mut r)? } else { r.cstr()? };
        let guid: [u8; 16] = r.take(16)?.try_into().unwrap();
        elements.push(Element { ty, name, guid, attrs: Vec::new() });
    }
    for el in 0..n_el {
        let n_attr = r.i32()?.max(0);
        for _ in 0..n_attr {
            let name = idx(&mut r)?;
            let ty = r.u8()?;
            let v = read_val(&mut r, ty, ver, &idx)?;
            elements[el].attrs.push((name, v));
        }
    }
    Ok(Dmx { elements })
}

fn read_val(r: &mut R, ty: u8, ver: u32, idx: &dyn Fn(&mut R) -> Result<String, String>) -> Result<Val, String> {
    let one = |r: &mut R, t: u8, in_array: bool| -> Result<Val, String> {
        Ok(match t {
            1 => {
                let i = r.i32()?;
                if i == -2 {
                    // Verweis auf ein externes Element (GUID als Text) – nicht auflösbar
                    r.cstr()?;
                    Val::Element(-1)
                } else {
                    Val::Element(i)
                }
            }
            2 => Val::Int(r.i32()?),
            3 => Val::Float(r.f32()?),
            4 => Val::Bool(r.u8()? != 0),
            5 => Val::Str(if ver >= 4 && !in_array { idx(r)? } else { r.cstr()? }),
            6 => {
                let n = r.i32()?.max(0) as usize;
                Val::Binary(r.take(n)?.to_vec())
            }
            7 => Val::Time(r.i32()?),
            8 => {
                let c = r.take(4)?;
                Val::Color([c[0], c[1], c[2], c[3]])
            }
            9 => Val::Vec2(r.fs::<2>()?),
            10 => Val::Vec3(r.fs::<3>()?),
            11 => Val::Vec4(r.fs::<4>()?),
            12 => Val::QAngle(r.fs::<3>()?),
            13 => Val::Quat(r.fs::<4>()?),
            14 => Val::Matrix(r.fs::<16>()?),
            _ => return Err(format!("unbekannter Attributtyp {t}")),
        })
    };
    if (15..=28).contains(&ty) {
        let base = ty - 14;
        let n = r.i32()?.max(0) as usize;
        let mut v = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            v.push(one(r, base, true)?);
        }
        return Ok(Val::Array(base, v));
    }
    one(r, ty, false)
}

/// Als binäres DMX Version 2 schreiben (`format` z. B. "pcf", `format_ver` meist 1)
pub fn write(d: &Dmx, format: &str, format_ver: u32) -> Vec<u8> {
    let mut b = format!("<!-- dmx encoding binary 2 format {format} {format_ver} -->\n").into_bytes();
    b.push(0);
    // Stringtabelle: Elementtypen und Attributnamen (Namen und Textwerte stehen in v2 direkt im Element)
    let mut table: Vec<String> = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut intern = |s: &str, table: &mut Vec<String>| -> u16 {
        *index.entry(s.to_string()).or_insert_with(|| {
            table.push(s.to_string());
            (table.len() - 1) as u16
        })
    };
    let mut ty_idx = Vec::new();
    let mut attr_idx: Vec<Vec<u16>> = Vec::new();
    for e in &d.elements {
        ty_idx.push(intern(&e.ty, &mut table));
        attr_idx.push(e.attrs.iter().map(|(k, _)| intern(k, &mut table)).collect());
    }
    b.extend_from_slice(&(table.len() as i32).to_le_bytes());
    for s in &table {
        b.extend_from_slice(s.as_bytes());
        b.push(0);
    }
    b.extend_from_slice(&(d.elements.len() as i32).to_le_bytes());
    for (e, t) in d.elements.iter().zip(&ty_idx) {
        b.extend_from_slice(&(*t as i16).to_le_bytes());
        b.extend_from_slice(e.name.as_bytes());
        b.push(0);
        b.extend_from_slice(&e.guid);
    }
    for (e, ai) in d.elements.iter().zip(&attr_idx) {
        b.extend_from_slice(&(e.attrs.len() as i32).to_le_bytes());
        for ((_, v), k) in e.attrs.iter().zip(ai) {
            b.extend_from_slice(&(*k as i16).to_le_bytes());
            b.push(v.type_id());
            write_val(&mut b, v);
        }
    }
    b
}

fn write_val(b: &mut Vec<u8>, v: &Val) {
    let fs = |b: &mut Vec<u8>, a: &[f32]| a.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
    match v {
        Val::Element(i) | Val::Int(i) | Val::Time(i) => b.extend_from_slice(&i.to_le_bytes()),
        Val::Float(f) => b.extend_from_slice(&f.to_le_bytes()),
        Val::Bool(x) => b.push(*x as u8),
        Val::Str(s) => {
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        Val::Binary(d) => {
            b.extend_from_slice(&(d.len() as i32).to_le_bytes());
            b.extend_from_slice(d);
        }
        Val::Color(c) => b.extend_from_slice(c),
        Val::Vec2(a) => fs(b, a),
        Val::Vec3(a) | Val::QAngle(a) => fs(b, a),
        Val::Vec4(a) | Val::Quat(a) => fs(b, a),
        Val::Matrix(a) => fs(b, a),
        Val::Array(_, items) => {
            b.extend_from_slice(&(items.len() as i32).to_le_bytes());
            for x in items {
                write_val(b, x);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_sample() {
        let src = crate::pcf::tests::sample_pcf();
        let d = read(&src).unwrap();
        assert_eq!(d.elements.len(), 4);
        assert_eq!(d.elements[1].name, "rauch");
        assert_eq!(d.elements[1].get("max_particles"), Some(&Val::Int(50)));
        let out = write(&d, "pcf", 1);
        assert_eq!(read(&out).unwrap(), d);
        // der einfache Leser für den Browser versteht das Ergebnis ebenfalls
        let sys = crate::pcf::parse(&out).unwrap();
        assert_eq!(sys[0].children, vec!["funken".to_string()]);
    }

    #[test]
    fn all_types() {
        let mut e = Element::new("DmeParticleOperator", "Movement Basic");
        e.set("functionName", Val::Str("Movement Basic".into()));
        e.set("gravity", Val::Vec3([0.0, 0.0, -400.0]));
        e.set("drag", Val::Float(0.1));
        e.set("on", Val::Bool(true));
        e.set("col", Val::Color([1, 2, 3, 4]));
        e.set("t", Val::Time(15000));
        e.set("q", Val::Quat([0.0, 0.0, 0.0, 1.0]));
        e.set("m", Val::Matrix([1.0; 16]));
        e.set("names", Val::Array(5, vec![Val::Str("a".into()), Val::Str("b".into())]));
        e.set("fl", Val::Array(3, vec![Val::Float(1.0), Val::Float(2.5)]));
        e.set("bin", Val::Binary(vec![9, 8, 7]));
        let d = Dmx { elements: vec![Element::new("DmElement", "root"), e] };
        let back = read(&write(&d, "pcf", 1)).unwrap();
        assert_eq!(back, d);
        assert!((back.elements[1].f("t", 0.0) - 1.5).abs() < 1e-6);
    }
}
