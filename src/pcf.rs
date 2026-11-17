//! Partikel-Dateien (.pcf, binäres DMX) lesen: Namen der Partikelsysteme und ihre Kinder.

/// Ein Partikelsystem aus einer .pcf-Datei
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystem {
    pub name: String,
    /// Namen der direkten Kinder-Systeme
    pub children: Vec<String>,
    /// Material des Systems (Attribut "material"), z. B. "particle/smoke1"
    pub material: Option<String>,
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.p)?;
        self.p += 1;
        Some(v)
    }
    fn i16(&mut self) -> Option<i16> {
        let v = i16::from_le_bytes(self.b.get(self.p..self.p + 2)?.try_into().ok()?);
        self.p += 2;
        Some(v)
    }
    fn i32(&mut self) -> Option<i32> {
        let v = i32::from_le_bytes(self.b.get(self.p..self.p + 4)?.try_into().ok()?);
        self.p += 4;
        Some(v)
    }
    fn cstr(&mut self) -> Option<String> {
        let end = self.b.get(self.p..)?.iter().position(|&c| c == 0)?;
        let s = String::from_utf8_lossy(&self.b[self.p..self.p + end]).to_string();
        self.p += end + 1;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        if self.p + n > self.b.len() {
            return None;
        }
        self.p += n;
        Some(())
    }
}

/// Partikelsysteme einer .pcf-Datei (binäre DMX-Versionen 2 bis 5)
pub fn parse(b: &[u8]) -> Option<Vec<ParticleSystem>> {
    let hdr_end = b.iter().position(|&c| c == 0)?;
    let hdr = String::from_utf8_lossy(&b[..hdr_end]).to_string();
    // "<!-- dmx encoding binary 2 format pcf 1 -->\n"
    let ver: u32 = hdr.split("binary").nth(1)?.split_whitespace().next()?.parse().ok()?;
    if !(2..=5).contains(&ver) {
        return None;
    }
    let mut r = R { b, p: hdr_end + 1 };
    let idx32 = ver >= 4;
    let count = if idx32 { r.i32()? } else if ver >= 3 { r.i32()? } else { r.i32()? };
    let mut strings = Vec::with_capacity(count.max(0) as usize);
    for _ in 0..count.max(0) {
        strings.push(r.cstr()?);
    }
    let read_idx = |r: &mut R| -> Option<usize> { if idx32 { r.i32().map(|v| v as usize) } else { r.i16().map(|v| v as u16 as usize) } };
    let n_el = r.i32()?.max(0) as usize;
    let mut types = Vec::with_capacity(n_el);
    let mut names = Vec::with_capacity(n_el);
    for _ in 0..n_el {
        let t = read_idx(&mut r)?;
        types.push(strings.get(t).cloned().unwrap_or_default());
        let name = if ver >= 4 { strings.get(read_idx(&mut r)?).cloned().unwrap_or_default() } else { r.cstr()? };
        names.push(name);
        r.skip(16)?; // GUID
    }
    // Attribute: "children" (Elemente) und bei DmeParticleChild "child"
    let mut children_refs: Vec<Vec<usize>> = vec![Vec::new(); n_el];
    let mut child_target: Vec<Option<usize>> = vec![None; n_el];
    let mut material: Vec<Option<String>> = vec![None; n_el];
    for el in 0..n_el {
        let n_attr = r.i32()?.max(0);
        for _ in 0..n_attr {
            let an = read_idx(&mut r)?;
            let aname = strings.get(an).map(|s| s.as_str()).unwrap_or("");
            let ty = r.u8()?;
            let read_str = |r: &mut R| -> Option<String> {
                if ver >= 4 {
                    read_idx(r).map(|i| strings.get(i).cloned().unwrap_or_default())
                } else {
                    r.cstr()
                }
            };
            let fixed = |t: u8| -> Option<usize> {
                Some(match t {
                    1 | 2 | 3 | 7 | 8 => 4,
                    4 => 1,
                    9 => 8,
                    10 | 12 => 12,
                    11 | 13 => 16,
                    14 => 64,
                    _ => return None,
                })
            };
            match ty {
                1 => {
                    let v = r.i32()?;
                    if aname == "child" && v >= 0 {
                        child_target[el] = Some(v as usize);
                    }
                }
                5 => {
                    let v = read_str(&mut r)?;
                    if aname == "material" && !v.is_empty() {
                        material[el] = Some(v);
                    }
                }
                6 => {
                    let n = r.i32()?.max(0) as usize;
                    r.skip(n)?;
                }
                t if t <= 14 => r.skip(fixed(t)?)?,
                15 => {
                    // Element-Array
                    let n = r.i32()?.max(0);
                    for _ in 0..n {
                        let v = r.i32()?;
                        if aname == "children" && v >= 0 {
                            children_refs[el].push(v as usize);
                        }
                    }
                }
                19 => {
                    let n = r.i32()?.max(0);
                    for _ in 0..n {
                        r.cstr()?; // String-Arrays stehen immer direkt in der Datei
                    }
                }
                20 => {
                    let n = r.i32()?.max(0) as usize;
                    for _ in 0..n {
                        let len = r.i32()?.max(0) as usize;
                        r.skip(len)?;
                    }
                }
                t if (16..=28).contains(&t) => {
                    let n = r.i32()?.max(0) as usize;
                    r.skip(n * fixed(t - 14)?)?;
                }
                _ => return None,
            }
        }
    }
    let mut out = Vec::new();
    for i in 0..n_el {
        if types[i] != "DmeParticleSystemDefinition" {
            continue;
        }
        let children = children_refs[i].iter().filter_map(|&c| child_target.get(c).copied().flatten()).filter_map(|t| names.get(t).cloned()).collect();
        out.push(ParticleSystem { name: names[i].clone(), children, material: material[i].take() });
    }
    Some(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Minimale PCF (binär v2): System "rauch" mit Kind "funken"
    pub fn sample_pcf() -> Vec<u8> {
        let mut b = b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0".to_vec();
        let strings = ["DmeElement", "DmeParticleSystemDefinition", "DmeParticleChild", "particleSystemDefinitions", "children", "child", "max_particles", "material"];
        b.extend_from_slice(&(strings.len() as i32).to_le_bytes());
        for s in strings {
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        // Elemente: 0 Wurzel, 1 rauch, 2 Kind-Verweis, 3 funken
        let els: [(i16, &str); 4] = [(0, "root"), (1, "rauch"), (2, "kind"), (1, "funken")];
        b.extend_from_slice(&(els.len() as i32).to_le_bytes());
        for (t, n) in els {
            b.extend_from_slice(&t.to_le_bytes());
            b.extend_from_slice(n.as_bytes());
            b.push(0);
            b.extend_from_slice(&[0u8; 16]);
        }
        // Wurzel: particleSystemDefinitions = [1, 3]
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&3i16.to_le_bytes());
        b.push(15);
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&3i32.to_le_bytes());
        // rauch: max_particles = 50 (int), children = [2]
        b.extend_from_slice(&2i32.to_le_bytes());
        b.extend_from_slice(&6i16.to_le_bytes());
        b.push(2);
        b.extend_from_slice(&50i32.to_le_bytes());
        b.extend_from_slice(&4i16.to_le_bytes());
        b.push(15);
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&2i32.to_le_bytes());
        // Kind-Verweis: child = 3
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&5i16.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&3i32.to_le_bytes());
        // funken: material = "particle/funke"
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&7i16.to_le_bytes());
        b.push(5);
        b.extend_from_slice(b"particle/funke\0");
        b
    }

    #[test]
    fn parse_v2() {
        let sys = parse(&sample_pcf()).unwrap();
        assert_eq!(sys.len(), 2);
        assert_eq!(sys[0], ParticleSystem { name: "rauch".into(), children: vec!["funken".into()], material: None });
        assert_eq!(sys[1].material.as_deref(), Some("particle/funke"));
        assert!(sys[1].children.is_empty());
        assert!(parse(b"kein pcf\0").is_none());
    }
}
