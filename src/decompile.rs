//! Map-Decompiler (wie BSPSource): Source-BSP (Version 19–21) → VMF. GoldSrc-BSPs: `goldsrc.rs`.
//!
//! Zurückgewonnen werden Brushes (Welt, func_detail, Brush-Entities) mit Materialien und
//! Texturachsen, Displacements (als dünne Brushes mit Displacement oben), Entities, statische
//! Props, Overlays und Cubemaps. Nicht rekonstruierbar sind Visgroups, Gruppen, Instanzen,
//! Areaportal-/Occluder-Brushes und die originalen Brush-IDs (VBSP verwirft sie).

use crate::geometry::Plane;
use crate::map::{fmt_num, fmt_vec, Entity, Map, Side, Solid, TexAxis};
use glam::DVec3;
use std::collections::BTreeSet;

const LUMP_ENTITIES: usize = 0;
const LUMP_PLANES: usize = 1;
const LUMP_TEXDATA: usize = 2;
const LUMP_VERTEXES: usize = 3;
const LUMP_NODES: usize = 5;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LEAFS: usize = 10;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;
const LUMP_LEAFBRUSHES: usize = 17;
const LUMP_BRUSHES: usize = 18;
const LUMP_BRUSHSIDES: usize = 19;
const LUMP_DISPINFO: usize = 26;
const LUMP_DISP_VERTS: usize = 33;
const LUMP_GAME_LUMP: usize = 35;
const LUMP_CUBEMAPS: usize = 42;
const LUMP_TEXDATA_STRING_DATA: usize = 43;
const LUMP_TEXDATA_STRING_TABLE: usize = 44;
const LUMP_OVERLAYS: usize = 45;

const CONTENTS_PLAYERCLIP: u32 = 0x10000;
const CONTENTS_MONSTERCLIP: u32 = 0x20000;
const CONTENTS_DETAIL: u32 = 0x8000000;
const CONTENTS_AREAPORTAL: u32 = 0x8000;

/// Bericht des Decompilers
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub version: u32,
    pub brushes: usize,
    pub detail: usize,
    pub brush_entities: usize,
    pub entities: usize,
    pub displacements: usize,
    pub static_props: usize,
    pub overlays: usize,
    pub cubemaps: usize,
    /// umgewandelte Texturen (GoldSrc)
    pub textures: usize,
    pub skipped: usize,
    pub notes: Vec<String>,
}

impl Report {
    pub fn lines(&self) -> Vec<String> {
        if self.version == 46 || self.version == 47 {
            let mut v = vec![
                format!("Quake-3-BSP (Version {})", self.version),
                format!("{} Brushes (davon {} func_detail), {} Brush-Entities, {} Entities", self.brushes, self.detail, self.brush_entities, self.entities),
                format!("{} Displacements aus gebogenen Flächen, {} Texturen nach VTF umgewandelt", self.displacements, self.textures),
            ];
            v.extend(self.notes.iter().cloned());
            return v;
        }
        if self.version == 29 || self.version == 30 {
            let mut v = vec![
                format!("GoldSrc-BSP (Version {})", self.version),
                format!("{} Brushes, {} Brush-Entities, {} Entities", self.brushes, self.brush_entities, self.entities),
                format!("{} Texturen nach VTF umgewandelt", self.textures),
            ];
            if self.skipped > 0 {
                v.push(format!("{} Brushes ließen sich nicht rekonstruieren (ungültige Form)", self.skipped));
            }
            v.extend(self.notes.iter().cloned());
            return v;
        }
        let mut v = vec![
            format!("BSP-Version {}", self.version),
            format!("{} Brushes (davon {} func_detail), {} Brush-Entities", self.brushes, self.detail, self.brush_entities),
            format!("{} Entities, {} statische Props, {} Displacements", self.entities, self.static_props, self.displacements),
            format!("{} Overlays, {} Cubemaps", self.overlays, self.cubemaps),
        ];
        if self.skipped > 0 {
            v.push(format!("{} Brushes ließen sich nicht rekonstruieren (ungültige Form)", self.skipped));
        }
        v.extend(self.notes.iter().cloned());
        v
    }
}

struct Bsp<'a> {
    b: &'a [u8],
    version: u32,
    lumps: Vec<(usize, usize)>,
}

impl<'a> Bsp<'a> {
    fn new(b: &'a [u8]) -> Result<Self, String> {
        if b.get(0..4) != Some(b"VBSP") {
            return Err("Keine Source-BSP-Datei".into());
        }
        let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
        if !(19..=21).contains(&version) {
            return Err(format!("BSP-Version {version} wird nicht unterstützt (19–21)"));
        }
        let mut lumps = Vec::new();
        for i in 0..64 {
            let e = 8 + i * 16;
            let rd = |o: usize| u32::from_le_bytes(b[e + o..e + o + 4].try_into().unwrap()) as usize;
            // Version 21 (L4D2): Version, Offset, Länge
            let (ofs, len) = if version == 21 && rd(0) < 0x100 && rd(4) >= 8 + 64 * 16 { (rd(4), rd(8)) } else { (rd(0), rd(4)) };
            lumps.push(if ofs + len <= b.len() { (ofs, len) } else { (0, 0) });
        }
        Ok(Bsp { b, version, lumps })
    }
    fn lump(&self, i: usize) -> &'a [u8] {
        let (o, l) = self.lumps[i];
        &self.b[o..o + l]
    }
}

fn f32a(b: &[u8], o: usize) -> f64 {
    b.get(o..o + 4).map(|x| f32::from_le_bytes(x.try_into().unwrap()) as f64).unwrap_or(0.0)
}
fn i32a(b: &[u8], o: usize) -> i32 {
    b.get(o..o + 4).map(|x| i32::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn u16a(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|x| u16::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
fn i16a(b: &[u8], o: usize) -> i16 {
    u16a(b, o) as i16
}
fn vec3(b: &[u8], o: usize) -> DVec3 {
    DVec3::new(f32a(b, o), f32a(b, o + 4), f32a(b, o + 8))
}

/// Entity-Lump: { "schlüssel" "wert" … } Blöcke
pub fn parse_entities(text: &str) -> Vec<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut cur: Option<Vec<(String, String)>> = None;
    let mut key: Option<String> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => cur = Some(Vec::new()),
            '}' => {
                if let Some(e) = cur.take() {
                    out.push(e);
                }
                key = None;
            }
            '"' => {
                let mut s = String::new();
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                    s.push(d);
                }
                if let Some(e) = cur.as_mut() {
                    match key.take() {
                        Some(k) => e.push((k, s)),
                        None => key = Some(s),
                    }
                }
            }
            _ => {}
        }
    }
    out
}

struct Ctx<'a> {
    bsp: Bsp<'a>,
    planes: Vec<Plane>,
    tex_names: Vec<String>,
}

impl Ctx<'_> {
    fn plane(&self, i: usize) -> Option<Plane> {
        self.planes.get(i).copied()
    }

    /// (Material, u-Achse, v-Achse, Lightmap-Skalierung) einer Texinfo
    fn texinfo(&self, ti: i32) -> Option<(String, TexAxis, TexAxis, i32)> {
        if ti < 0 {
            return None;
        }
        let l = self.bsp.lump(LUMP_TEXINFO);
        let o = ti as usize * 72;
        if o + 72 > l.len() {
            return None;
        }
        let axis = |k: usize| {
            let v = vec3(l, o + k * 16);
            let len = v.length().max(1e-6);
            TexAxis { axis: v / len, shift: f32a(l, o + k * 16 + 12), scale: 1.0 / len }
        };
        let lm = vec3(l, o + 32).length();
        let lms = if lm > 1e-6 { (1.0 / lm).round().clamp(1.0, 128.0) as i32 } else { 16 };
        let td = i32a(l, o + 68);
        let name = self.texdata_name(td).unwrap_or_else(|| "tools/toolsnodraw".into());
        Some((name, axis(0), axis(1), lms))
    }

    fn texdata_name(&self, td: i32) -> Option<String> {
        let l = self.bsp.lump(LUMP_TEXDATA);
        let o = (td.max(0) as usize) * 32;
        let sid = i32a(l, o + 12).max(0) as usize;
        self.tex_names.get(sid).cloned()
    }

    fn face_poly(&self, face: usize) -> Option<(Vec<DVec3>, Plane, i32)> {
        let f = self.bsp.lump(LUMP_FACES);
        let o = face * 56;
        if o + 56 > f.len() {
            return None;
        }
        let mut plane = self.plane(u16a(f, o) as usize)?;
        if f[o + 2] != 0 {
            plane = Plane { normal: -plane.normal, dist: -plane.dist };
        }
        let first = i32a(f, o + 4).max(0) as usize;
        let n = i16a(f, o + 8).max(0) as usize;
        let ti = i16a(f, o + 10) as i32;
        let (se, ed, vx) = (self.bsp.lump(LUMP_SURFEDGES), self.bsp.lump(LUMP_EDGES), self.bsp.lump(LUMP_VERTEXES));
        let mut poly = Vec::with_capacity(n);
        for k in 0..n {
            let s = i32a(se, (first + k) * 4);
            let v = if s >= 0 { u16a(ed, s as usize * 4) } else { u16a(ed, (-s) as usize * 4 + 2) } as usize;
            poly.push(vec3(vx, v * 12));
        }
        Some((poly, plane, ti))
    }

    /// Alle Brush-Indizes im Baum eines Modells
    fn model_brushes(&self, model: usize) -> BTreeSet<usize> {
        let m = self.bsp.lump(LUMP_MODELS);
        let mut out = BTreeSet::new();
        if (model + 1) * 48 > m.len() {
            return out;
        }
        let head = i32a(m, model * 48 + 36);
        let (nodes, leafs, lb) = (self.bsp.lump(LUMP_NODES), self.bsp.lump(LUMP_LEAFS), self.bsp.lump(LUMP_LEAFBRUSHES));
        let leaf_size = if self.bsp.version == 19 { 56 } else { 32 };
        let mut stack = vec![head];
        let mut guard = 0;
        while let Some(n) = stack.pop() {
            guard += 1;
            if guard > 2_000_000 {
                break;
            }
            if n >= 0 {
                let o = n as usize * 32;
                if o + 32 > nodes.len() {
                    continue;
                }
                stack.push(i32a(nodes, o + 4));
                stack.push(i32a(nodes, o + 8));
            } else {
                let o = (-1 - n) as usize * leaf_size;
                if o + 28 > leafs.len() {
                    continue;
                }
                let first = u16a(leafs, o + 24) as usize;
                let count = u16a(leafs, o + 26) as usize;
                for k in first..first + count {
                    out.insert(u16a(lb, k * 2) as usize);
                }
            }
        }
        out
    }

    /// Brush rekonstruieren (Welt-Koordinaten, optional verschoben)
    fn brush(&self, map: &mut Map, idx: usize, offset: DVec3) -> Option<(Solid, u32)> {
        let bl = self.bsp.lump(LUMP_BRUSHES);
        let sl = self.bsp.lump(LUMP_BRUSHSIDES);
        let o = idx * 12;
        if o + 12 > bl.len() {
            return None;
        }
        let first = i32a(bl, o).max(0) as usize;
        let n = i32a(bl, o + 4).max(0) as usize;
        let contents = i32a(bl, o + 8) as u32;
        let fallback = if contents & CONTENTS_PLAYERCLIP != 0 && contents & CONTENTS_MONSTERCLIP != 0 {
            "tools/toolsclip"
        } else if contents & CONTENTS_PLAYERCLIP != 0 {
            "tools/toolsplayerclip"
        } else if contents & CONTENTS_MONSTERCLIP != 0 {
            "tools/toolsnpcclip"
        } else if contents & CONTENTS_AREAPORTAL != 0 {
            "tools/toolsareaportal"
        } else {
            "tools/toolsnodraw"
        };
        let mut sides = Vec::new();
        for k in first..first + n {
            let so = k * 8;
            if so + 8 > sl.len() {
                break;
            }
            if sl[so + 6] != 0 {
                continue; // Bevel-Ebene (nur für Kollision)
            }
            let Some(mut p) = self.plane(u16a(sl, so) as usize) else { continue };
            p.dist += p.normal.dot(offset);
            if sides.iter().any(|s: &Side| s.plane().is_some_and(|q| q.normal.dot(p.normal) > 0.9999 && (q.dist - p.dist).abs() < 0.01)) {
                continue;
            }
            let mut side = map.side_from_plane(p, fallback, 0.25);
            if let Some((mat, u, v, lms)) = self.texinfo(i16a(sl, so + 2) as i32) {
                side.material = mat.to_ascii_uppercase();
                let shift = |a: &TexAxis| TexAxis { axis: a.axis, scale: a.scale, shift: a.shift - offset.dot(a.axis) / a.scale };
                side.uaxis = shift(&u);
                side.vaxis = shift(&v);
                side.lightmapscale = lms;
            }
            sides.push(side);
        }
        let id = map.alloc_id();
        let mut s = Solid::new(id, sides);
        s.cleanup();
        s.is_valid().then_some((s, contents))
    }

    /// Displacement als dünner Brush (Fläche oben, 8 Einheiten dick)
    fn disp_brush(&self, map: &mut Map, di: usize) -> Option<Solid> {
        let dl = self.bsp.lump(LUMP_DISPINFO);
        let o = di * 176;
        if o + 176 > dl.len() {
            return None;
        }
        let start = vec3(dl, o);
        let vstart = i32a(dl, o + 12).max(0) as usize;
        let power = i32a(dl, o + 20).clamp(2, 4) as u32;
        let face = u16a(dl, o + 36) as usize;
        let (poly, plane, ti) = self.face_poly(face)?;
        if poly.len() != 4 {
            return None;
        }
        let n = plane.normal;
        let c = poly.iter().fold(DVec3::ZERO, |a, b| a + *b) / 4.0;
        let mut planes = vec![plane, Plane { normal: -n, dist: -(plane.dist - 8.0) }];
        for k in 0..4 {
            let (a, b) = (poly[k], poly[(k + 1) % 4]);
            let mut sn = (b - a).cross(n).normalize_or_zero();
            if sn.dot(a - c) < 0.0 {
                sn = -sn;
            }
            planes.push(Plane { normal: sn, dist: sn.dot(a) });
        }
        let mut sides: Vec<Side> = planes.iter().map(|p| map.side_from_plane(*p, "TOOLS/TOOLSNODRAW", 0.25)).collect();
        let (mat, u, v, lms) = self.texinfo(ti)?;
        sides[0].material = mat.to_ascii_uppercase();
        sides[0].uaxis = u;
        sides[0].vaxis = v;
        sides[0].lightmapscale = lms;
        let id = map.alloc_id();
        let mut s = Solid::new(id, sides);
        s.cleanup();
        let top = s.sides.iter().position(|sd| sd.plane().is_some_and(|p| p.normal.dot(n) > 0.9999))?;
        let mut d = crate::disp::Disp::new_flat(power, &s.polys[top], n)?;
        d.start = start;
        let res = (1usize << power) + 1;
        let vl = self.bsp.lump(LUMP_DISP_VERTS);
        for i in 0..res * res {
            let vo = (vstart + i) * 20;
            if vo + 20 > vl.len() {
                break;
            }
            d.normals[i] = vec3(vl, vo);
            d.distances[i] = f32a(vl, vo + 12);
            d.alphas[i] = f32a(vl, vo + 16);
        }
        s.sides[top].disp = Some(d);
        s.update_geometry();
        Some(s)
    }
}

/// BSP (Bytes) → Map
pub fn decompile(b: &[u8]) -> Result<(Map, Report), String> {
    let bsp = Bsp::new(b)?;
    let mut r = Report { version: bsp.version, ..Default::default() };
    let pl = bsp.lump(LUMP_PLANES);
    let planes: Vec<Plane> = (0..pl.len() / 20).map(|i| Plane { normal: vec3(pl, i * 20), dist: f32a(pl, i * 20 + 12) }).collect();
    let (sd, st) = (bsp.lump(LUMP_TEXDATA_STRING_DATA), bsp.lump(LUMP_TEXDATA_STRING_TABLE));
    let tex_names: Vec<String> = (0..st.len() / 4)
        .map(|i| {
            let o = i32a(st, i * 4).max(0) as usize;
            let end = sd.get(o..).and_then(|x| x.iter().position(|&c| c == 0)).map(|e| o + e).unwrap_or(sd.len());
            String::from_utf8_lossy(sd.get(o..end).unwrap_or(&[])).replace('\\', "/")
        })
        .collect();
    let cx = Ctx { bsp, planes, tex_names };
    let mut map = Map::new();
    let mut used = BTreeSet::new();

    // Entities
    let text = String::from_utf8_lossy(cx.bsp.lump(LUMP_ENTITIES)).to_string();
    for kv in parse_entities(&text) {
        let class = kv.iter().find(|(k, _)| k == "classname").map(|(_, v)| v.clone()).unwrap_or_default();
        if class == "worldspawn" {
            for (k, v) in &kv {
                if k != "model" && k != "world_mins" && k != "world_maxs" {
                    map.world.set(k, v.clone());
                }
            }
            continue;
        }
        let id = map.alloc_id();
        let mut e = Entity::new(id, &class);
        let mut model = None;
        for (k, v) in &kv {
            if k == "model" && v.starts_with('*') {
                model = v[1..].parse::<usize>().ok();
            } else if k != "classname" && k != "hammerid" {
                e.set(k, v.clone());
            }
        }
        if let Some(m) = model {
            // Brush-Entity: VBSP verschiebt Modelle mit "origin" in den Ursprung
            let off = e.get("origin").and_then(crate::map::parse_vec).unwrap_or(DVec3::ZERO);
            for bi in cx.model_brushes(m) {
                used.insert(bi);
                match cx.brush(&mut map, bi, off) {
                    Some((s, _)) => e.solids.push(s.into()),
                    None => r.skipped += 1,
                }
            }
            if e.solids.is_empty() {
                r.notes.push(format!("{class}: Brushes nicht rekonstruierbar (z. B. Areaportal/Occluder) – Entity bleibt als Punkt-Entity"));
            } else {
                r.brush_entities += 1;
                r.brushes += e.solids.len();
            }
        }
        r.entities += 1;
        map.entities.push(e.into());
    }

    // Welt-Brushes (Detail-Brushes als func_detail)
    for bi in cx.model_brushes(0) {
        if used.contains(&bi) {
            continue;
        }
        match cx.brush(&mut map, bi, DVec3::ZERO) {
            Some((s, contents)) => {
                r.brushes += 1;
                if contents & CONTENTS_DETAIL != 0 {
                    let id = map.alloc_id();
                    let mut e = Entity::new(id, "func_detail");
                    e.solids.push(s.into());
                    map.entities.push(e.into());
                    r.detail += 1;
                } else {
                    map.world.solids.push(s.into());
                }
            }
            None => r.skipped += 1,
        }
    }

    // Displacements
    let ndisp = cx.bsp.lump(LUMP_DISPINFO).len() / 176;
    for di in 0..ndisp {
        if let Some(s) = cx.disp_brush(&mut map, di) {
            map.world.solids.push(s.into());
            r.displacements += 1;
        }
    }

    // Statische Props (Game-Lump "sprp")
    let gl = cx.bsp.lump(LUMP_GAME_LUMP);
    let count = i32a(gl, 0).max(0) as usize;
    for k in 0..count.min(64) {
        let o = 4 + k * 16;
        if i32a(gl, o) as u32 != u32::from_be_bytes(*b"sprp") {
            continue;
        }
        let (flags, ver) = (u16a(gl, o + 4), u16a(gl, o + 6));
        let (ofs, len) = (i32a(gl, o + 8).max(0) as usize, i32a(gl, o + 12).max(0) as usize);
        if flags & 1 != 0 {
            r.notes.push("Statische Props sind komprimiert (LZMA) – nicht unterstützt".into());
            continue;
        }
        let Some(sp) = b.get(ofs..ofs + len) else { continue };
        let nd = i32a(sp, 0).max(0) as usize;
        let names: Vec<String> = (0..nd)
            .map(|i| {
                let s = sp.get(4 + i * 128..4 + (i + 1) * 128).unwrap_or(&[]);
                let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
                String::from_utf8_lossy(&s[..end]).replace('\\', "/")
            })
            .collect();
        let mut p = 4 + nd * 128;
        let nl = i32a(sp, p).max(0) as usize;
        p += 4 + nl * 2;
        let np = i32a(sp, p).max(0) as usize;
        p += 4;
        if np == 0 {
            continue;
        }
        let stride = (len.saturating_sub(p)) / np;
        if stride < 56 {
            r.notes.push(format!("Unbekanntes Format der statischen Props (Version {ver})"));
            continue;
        }
        for i in 0..np {
            let q = p + i * stride;
            let id = map.alloc_id();
            let mut e = Entity::new(id, "prop_static");
            e.set("origin", fmt_vec(vec3(sp, q)));
            e.set("angles", fmt_vec(vec3(sp, q + 12)));
            e.set("model", names.get(u16a(sp, q + 24) as usize).cloned().unwrap_or_default());
            e.set("solid", sp.get(q + 30).copied().unwrap_or(6).to_string());
            let fl = sp.get(q + 31).copied().unwrap_or(0);
            e.set("skin", i32a(sp, q + 32).to_string());
            e.set("fademindist", fmt_num(f32a(sp, q + 36)));
            e.set("fademaxdist", fmt_num(f32a(sp, q + 40)));
            if fl & 0x10 != 0 {
                e.set("disableshadows", "1");
            }
            if fl & 0x40 != 0 {
                e.set("disablevertexlighting", "1");
            }
            if stride >= 60 {
                e.set("fadescale", fmt_num(f32a(sp, q + 56)));
            }
            map.entities.push(e.into());
            r.static_props += 1;
        }
    }

    // Cubemaps
    let cl = cx.bsp.lump(LUMP_CUBEMAPS);
    for i in 0..cl.len() / 16 {
        let o = i * 16;
        let id = map.alloc_id();
        let mut e = Entity::new(id, "env_cubemap");
        e.set("origin", format!("{} {} {}", i32a(cl, o), i32a(cl, o + 4), i32a(cl, o + 8)));
        e.set("cubemapsize", i32a(cl, o + 12).to_string());
        map.entities.push(e.into());
        r.cubemaps += 1;
    }

    // Overlays (Seiten werden über die Lage der Brush-Flächen wiedergefunden)
    let ol = cx.bsp.lump(LUMP_OVERLAYS);
    let mut side_index: Vec<(u32, Plane, crate::geometry::Aabb)> = Vec::new();
    if ol.len() >= 352 {
        for s in map.world.solids.iter().map(|s| &**s).chain(map.entities.iter().flat_map(|e| e.solids.iter().map(|s| &**s))) {
            for (k, side) in s.sides.iter().enumerate() {
                if let (Some(Some(p)), Some(poly)) = (s.planes.get(k), s.polys.get(k)) {
                    side_index.push((side.id, *p, crate::geometry::Aabb::from_points(poly.iter().copied())));
                }
            }
        }
    }
    for i in 0..ol.len() / 352 {
        let o = i * 352;
        let ti = i16a(ol, o + 4) as i32;
        let Some((mat, ..)) = cx.texinfo(ti) else { continue };
        let us = (f32a(ol, o + 264), f32a(ol, o + 268));
        let vs = (f32a(ol, o + 272), f32a(ol, o + 276));
        let uv: Vec<DVec3> = (0..4).map(|k| vec3(ol, o + 280 + k * 12)).collect();
        let origin = vec3(ol, o + 328);
        let normal = vec3(ol, o + 340);
        let bu = DVec3::new(uv[0].z, uv[1].z, uv[2].z);
        let mut bv = normal.cross(bu);
        if uv[3].z == 1.0 {
            bv = -bv;
        }
        let sides: Vec<String> = side_index
            .iter()
            .filter(|(_, p, b)| p.distance(origin).abs() < 1.0 && p.normal.dot(normal) > 0.7 && crate::geometry::Aabb::new(b.min - DVec3::ONE, b.max + DVec3::ONE).contains_point(origin))
            .map(|(id, ..)| id.to_string())
            .collect();
        let id = map.alloc_id();
        let mut e = Entity::new(id, "info_overlay");
        e.set("material", mat);
        e.set("sides", sides.join(" "));
        e.set("RenderOrder", ((u16a(ol, o + 6) >> 14) as u32).to_string());
        e.set("StartU", fmt_num(us.0));
        e.set("EndU", fmt_num(us.1));
        e.set("StartV", fmt_num(vs.0));
        e.set("EndV", fmt_num(vs.1));
        e.set("BasisOrigin", fmt_vec(origin));
        e.set("BasisNormal", fmt_vec(normal));
        e.set("BasisU", fmt_vec(bu));
        e.set("BasisV", fmt_vec(bv));
        for (k, p) in uv.iter().enumerate() {
            e.set(&format!("uv{k}"), format!("{} {} 0", fmt_num(p.x), fmt_num(p.y)));
        }
        e.set("origin", fmt_vec(origin));
        if sides.is_empty() {
            r.notes.push(format!("Overlay {} ({}): keine passende Fläche gefunden", id, e.get("material").unwrap_or("")));
        }
        map.entities.push(e.into());
        r.overlays += 1;
    }
    Ok((map, r))
}

/// BSP-Datei dekompilieren und als VMF speichern
pub fn decompile_file(bsp: &std::path::Path, out: &std::path::Path, game_dir: &str, extra_wads: &[std::path::PathBuf]) -> Result<Report, String> {
    let b = std::fs::read(bsp).map_err(|e| format!("{}: {e}", bsp.display()))?;
    if crate::goldsrc::version(&b).is_some() {
        return decompile_goldsrc(&b, bsp, out, game_dir, extra_wads);
    }
    if crate::quake3::ibsp_version(&b).is_some() {
        let mut assets = crate::quake3::Assets::load(&crate::quake3::roots(bsp, extra_wads));
        let (map, mut r) = crate::quake3::decompile(&b, &mut assets)?;
        if !assets.files.is_empty() {
            let gd = std::path::Path::new(game_dir);
            let root = if !game_dir.is_empty() && gd.is_dir() { gd.join("materials") } else { out.parent().unwrap_or(std::path::Path::new(".")).join("materials") };
            crate::goldsrc::write_files(&assets.files, &root)?;
            r.notes.insert(0, format!("Materialien gespeichert in {}", root.join("quake3").display()));
        }
        if assets.has_data() {
            r.notes.insert(0, format!("Quake-3-Daten: {}", assets.summary()));
        }
        std::fs::write(out, map.to_vmf()).map_err(|e| format!("{}: {e}", out.display()))?;
        return Ok(r);
    }
    let (map, r) = decompile(&b)?;
    std::fs::write(out, map.to_vmf()).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(r)
}

/// GoldSrc-BSP: WADs suchen, dekompilieren, Materialien in den Spielordner (sonst neben die VMF) schreiben
fn decompile_goldsrc(b: &[u8], bsp: &std::path::Path, out: &std::path::Path, game_dir: &str, extra_wads: &[std::path::PathBuf]) -> Result<Report, String> {
    use crate::goldsrc;
    let wad_key = {
        let text = goldsrc_entities(b);
        parse_entities(&text).into_iter().find(|kv| kv.iter().any(|(k, v)| k == "classname" && v == "worldspawn")).and_then(|kv| kv.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("wad")).map(|(_, v)| v)).unwrap_or_default()
    };
    let wads = goldsrc::Wads::load(&goldsrc::find_wads(&goldsrc::wad_dirs(bsp, game_dir, extra_wads), &wad_key));
    let map_name = bsp.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let (map, mut r, files) = goldsrc::decompile(b, &goldsrc::Opts { wads: &wads, map_name })?;
    let gd = std::path::Path::new(game_dir);
    let root = if !game_dir.is_empty() && gd.is_dir() { gd.join("materials") } else { out.parent().unwrap_or(std::path::Path::new(".")).join("materials") };
    if !files.is_empty() {
        goldsrc::write_files(&files, &root)?;
        r.notes.insert(0, format!("Materialien gespeichert in {}", root.join("goldsrc").display()));
    }
    if wads.len() > 0 {
        r.notes.insert(0, format!("WAD-Dateien: {}", wads.names().join(", ")));
    }
    std::fs::write(out, map.to_vmf()).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(r)
}

/// Entity-Text einer GoldSrc-BSP (für den Schlüssel "wad")
pub fn goldsrc_entities(b: &[u8]) -> String {
    let rd = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize).unwrap_or(0);
    for k in [0usize, 1] {
        let (ofs, len) = (rd(4 + k * 8), rd(8 + k * 8));
        if let Some(t) = b.get(ofs..ofs + len) {
            if t.iter().find(|c| !c.is_ascii_whitespace()) == Some(&b'{') {
                return String::from_utf8_lossy(t).to_string();
            }
        }
    }
    String::new()
}

type DecompResult = std::sync::Arc<std::sync::Mutex<Option<Result<(std::path::PathBuf, Report), String>>>>;

/// Laufender Decompiler-Vorgang und Bericht
#[derive(Default)]
pub struct DecompState {
    job: Option<(std::path::PathBuf, DecompResult)>,
    /// (Titel, Datei, Zeilen)
    pub report: Option<(String, String, Vec<String>)>,
    pub dialog: Option<DecompDlg>,
}

/// Dialog vor dem Dekompilieren
pub struct DecompDlg {
    pub bsp: std::path::PathBuf,
    pub out: String,
    pub size: u64,
    pub version: Option<u32>,
    /// GoldSrc: zusätzlicher WAD-Ordner und gefundene WAD-Dateien
    pub wad_dir: String,
    pub wads: Vec<String>,
}

impl DecompDlg {
    fn goldsrc(&self) -> bool {
        matches!(self.version, Some(29 | 30))
    }
    fn quake3(&self) -> bool {
        matches!(self.version, Some(46 | 47))
    }
}

/// Gefundene Quake-3-Daten (pk3-Dateien bzw. Ordner) für den Dialog
fn q3_found(bsp: &std::path::Path, extra: &str) -> Vec<String> {
    let extra: Vec<std::path::PathBuf> = (!extra.trim().is_empty()).then(|| std::path::PathBuf::from(extra.trim())).into_iter().collect();
    let mut v = Vec::new();
    for r in crate::quake3::roots(bsp, &extra) {
        let mut paks: Vec<String> = std::fs::read_dir(&r).map(|rd| rd.flatten().filter_map(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("pk3")).then(|| e.file_name().to_string_lossy().to_string())).collect()).unwrap_or_default();
        paks.sort();
        if paks.is_empty() {
            v.push(r.display().to_string());
        } else {
            v.extend(paks);
        }
    }
    v
}

impl crate::app::App {
    /// Datei → BSP dekompilieren…
    /// Datei → BSP dekompilieren…: BSP wählen, dann den Dialog zeigen
    pub(crate) fn start_decompile(&mut self) {
        let maps = std::path::Path::new(&self.settings.game_dir).join("maps");
        let mut dlg = rfd::FileDialog::new().add_filter("BSP", &["bsp"]);
        if maps.is_dir() {
            dlg = dlg.set_directory(&maps);
        }
        if let Some(bsp) = dlg.pick_file() {
            self.ask_decompile(bsp);
        }
    }

    /// Dialog „BSP dekompilieren“ für eine BSP öffnen (z. B. wenn sie über Öffnen gewählt wurde)
    pub(crate) fn ask_decompile(&mut self, bsp: std::path::PathBuf) {
        let stem = bsp.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        // nicht in den maps-Ordner des Spiels schreiben, lieber zur zuletzt benutzten VMF
        let in_game_maps = bsp.parent().is_some_and(|d| d.ends_with("maps") && d.parent().is_some_and(|g| g.join("gameinfo.txt").is_file()));
        let dir = self.ed.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).filter(|_| in_game_maps).or_else(|| bsp.parent().map(|d| d.to_path_buf())).unwrap_or_default();
        let mut out = dir.join(format!("{stem}_dekompiliert.vmf"));
        let mut k = 2;
        while out.exists() {
            out = dir.join(format!("{stem}_dekompiliert_{k}.vmf"));
            k += 1;
        }
        let info = std::fs::metadata(&bsp).map(|m| m.len()).unwrap_or(0);
        let bytes = std::fs::read(&bsp).unwrap_or_default();
        let version = if bytes.get(0..4) == Some(b"VBSP") { Some(u32::from_le_bytes(bytes[4..8].try_into().unwrap())) } else { crate::goldsrc::version(&bytes).or_else(|| crate::quake3::ibsp_version(&bytes)) };
        let mut d = DecompDlg { bsp, out: out.display().to_string(), size: info, version, wad_dir: String::new(), wads: Vec::new() };
        if d.goldsrc() {
            d.wads = self.goldsrc_wads(&d.bsp, &bytes, "");
        } else if d.quake3() {
            d.wads = q3_found(&d.bsp, "");
        }
        self.decomp.dialog = Some(d);
    }

    /// Gefundene WAD-Dateien (Namen) für eine GoldSrc-BSP
    fn goldsrc_wads(&self, bsp: &std::path::Path, bytes: &[u8], extra: &str) -> Vec<String> {
        let wad_key = parse_entities(&goldsrc_entities(bytes)).into_iter().find(|kv| kv.iter().any(|(k, v)| k == "classname" && v == "worldspawn")).and_then(|kv| kv.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("wad")).map(|(_, v)| v)).unwrap_or_default();
        let extra: Vec<std::path::PathBuf> = (!extra.trim().is_empty()).then(|| std::path::PathBuf::from(extra.trim())).into_iter().collect();
        crate::goldsrc::find_wads(&crate::goldsrc::wad_dirs(bsp, &self.settings.game_dir, &extra), &wad_key).iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect()
    }

    pub(crate) fn run_decompile(&mut self, bsp: std::path::PathBuf, out: std::path::PathBuf, wad_dir: String) {
        let gd = self.settings.game_dir.clone();
        let res: DecompResult = Default::default();
        let r2 = res.clone();
        let ctx = self.ctx.clone();
        let o2 = out.clone();
        std::thread::spawn(move || {
            let extra: Vec<std::path::PathBuf> = (!wad_dir.trim().is_empty()).then(|| std::path::PathBuf::from(wad_dir.trim())).into_iter().collect();
            let r = decompile_file(&bsp, &o2, &gd, &extra).map(|rep| (o2, rep));
            *r2.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
        self.ed.status = format!("Dekompiliere… → {}", out.display());
        self.decomp.job = Some((out, res));
    }

    fn decompile_ask_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme;
        use eframe::egui::{self, RichText};
        let busy = self.decomp.job.is_some();
        let Some(d) = self.decomp.dialog.as_mut() else { return };
        let th = theme::t();
        let mut open = true;
        let (mut go, mut pick, mut cancel, mut pick_wad, mut rescan) = (false, false, false, false, false);
        let name = d.bsp.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let dlg = theme::Dlg::new("BSP dekompilieren", 620.0).subtitle(name).footer();
        theme::dialog(ctx, &mut open, &dlg, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 14)).show(ui, |ui| {
                    ui.label(RichText::new(crate::i18n::tr("Die kompilierte Map wird wieder in eine bearbeitbare VMF umgewandelt und danach geöffnet.")).color(th.text2));
                    ui.add_space(8.0);
                    theme::field_row(ui, "Quelle", |ui| ui.add(egui::Label::new(RichText::new(d.bsp.display().to_string()).font(theme::mono(11.0)).color(th.text2)).truncate()).on_hover_text(d.bsp.display().to_string()));
                    let ver = match d.version {
                        Some(v @ (29 | 30)) => format!("GoldSrc BSP v{v}"),
                        Some(v @ (46 | 47)) => format!("Quake 3 BSP v{v}"),
                        Some(v) => format!("BSP v{v}"),
                        None => crate::i18n::tr("keine gültige BSP").into(),
                    };
                    theme::field_row(ui, "Datei", |ui| ui.label(RichText::new(format!("{ver} · {}", crate::i18n::size(d.size as u64))).color(if d.version.is_some() { th.text2 } else { th.err })));
                    theme::field_row(ui, "Speichern als", |ui| {
                        ui.add(egui::TextEdit::singleline(&mut d.out).font(theme::mono(11.0)).desired_width(330.0));
                        if ui.button("…").clicked() {
                            pick = true;
                        }
                    });
                    if d.quake3() {
                        theme::field_row(ui, "Quake-3-Ordner", |ui| {
                            if ui.add(egui::TextEdit::singleline(&mut d.wad_dir).hint_text(crate::i18n::tr("optional, z. B. Quake 3/baseq3")).font(theme::mono(11.0)).desired_width(330.0)).lost_focus() {
                                rescan = true;
                            }
                            if ui.button("…").clicked() {
                                pick_wad = true;
                            }
                        });
                        let found = if d.wads.is_empty() { crate::i18n::tr("keine gefunden – Texturen fehlen dann").to_string() } else { d.wads.join(", ") };
                        theme::field_row(ui, "Spieldaten", |ui| ui.add(egui::Label::new(RichText::new(found).size(11.5).color(if d.wads.is_empty() { th.warn } else { th.text2 })).wrap()));
                        ui.add_space(8.0);
                        theme::section(ui, "Was zurückkommt");
                        ui.label(RichText::new(crate::i18n::tr("Quake-3-Map: alle Brushes mit Texturausrichtung, func_detail, Brush-Entities, Entities (Lichter umgerechnet), gebogene Flächen als Displacements, Texturen aus pk3 oder Ordnern (Shader-Skripte, TGA/JPG) als VTF/VMT unter materials/quake3, Wasser mit Source-Wassermaterial.")).size(12.0).color(th.text2));
                        ui.label(RichText::new(crate::i18n::tr("Nicht wiederherstellbar: Nebel-Volumen, Shader-Effekte (Animation, Leuchten), Modelle (misc_model), Waffen und Gegenstände (bleiben als Entities ohne Wirkung).")).size(11.5).color(th.text3));
                        return;
                    }
                    if d.goldsrc() {
                        theme::field_row(ui, "WAD-Ordner", |ui| {
                            if ui.add(egui::TextEdit::singleline(&mut d.wad_dir).hint_text(crate::i18n::tr("optional, z. B. Half-Life/valve")).font(theme::mono(11.0)).desired_width(330.0)).lost_focus() {
                                rescan = true;
                            }
                            if ui.button("…").clicked() {
                                pick_wad = true;
                            }
                        });
                        let found = if d.wads.is_empty() { crate::i18n::tr("keine gefunden – Texturen fehlen dann").to_string() } else { d.wads.join(", ") };
                        theme::field_row(ui, "WAD-Dateien", |ui| ui.add(egui::Label::new(RichText::new(found).size(11.5).color(if d.wads.is_empty() { th.warn } else { th.text2 })).wrap()));
                        ui.add_space(8.0);
                        theme::section(ui, "Was zurückkommt");
                        ui.label(RichText::new(crate::i18n::tr("Half-Life-1-Map: Brushes werden aus dem BSP-Baum zurückgewonnen (dicht, aber in mehr Teile zerlegt als im Original), Texturen aus BSP und WAD-Dateien als VTF/VMT unter materials/goldsrc, Entities mit Umwandlung (func_wall → func_brush, angle → angles/movedir, Lichtkegel), Wasser mit Source-Wassermaterial.")).size(12.0).color(th.text2));
                        ui.label(RichText::new(crate::i18n::tr("Nicht wiederherstellbar: Clip-Brushes, Monster und HL1-Waffen (bleiben als Entities ohne Wirkung), Himmel (wird sky_day01_01).")).size(11.5).color(th.text3));
                        return;
                    }
                    ui.add_space(8.0);
                    theme::section(ui, "Was zurückkommt");
                    ui.label(RichText::new(crate::i18n::tr("Brushes mit Materialien und Texturausrichtung · func_detail · Türen, Trigger und andere Brush-Entities · alle Entities · Displacements · statische Props · Overlays · Cubemaps")).size(12.0).color(th.text2));
                    ui.label(RichText::new(crate::i18n::tr("Nicht wiederherstellbar: Visgroups, Gruppen, Instanzen, Areaportal- und Occluder-Brushes, originale IDs. Displacements stehen als dünne Brushes mit Displacement oben in der Welt.")).size(11.5).color(th.text3));
                });
            }
            theme::Part::Footer => {
                let ok = d.version.is_some() && !d.out.trim().is_empty() && !busy;
                if ui.add_enabled(ok, egui::Button::new(RichText::new(crate::i18n::tr("Dekompilieren & öffnen")).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(egui::Vec2::new(0.0, 34.0))).clicked() {
                    go = true;
                }
                if theme::secondary_button(ui, "Abbrechen").clicked() {
                    cancel = true;
                }
                if busy {
                    ui.spinner();
                }
            }
            _ => {}
        });
        if pick {
            let cur = std::path::PathBuf::from(d.out.trim());
            let mut f = rfd::FileDialog::new().add_filter("VMF", &["vmf"]);
            if let Some(dir) = cur.parent().filter(|p| p.is_dir()) {
                f = f.set_directory(dir);
            }
            if let Some(n) = cur.file_name() {
                f = f.set_file_name(n.to_string_lossy());
            }
            if let Some(p) = f.save_file() {
                d.out = p.display().to_string();
            }
        }
        if pick_wad {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                d.wad_dir = p.display().to_string();
                rescan = true;
            }
        }
        if rescan {
            let (bsp, extra) = (d.bsp.clone(), d.wad_dir.clone());
            let bytes = std::fs::read(&bsp).unwrap_or_default();
            let w = if crate::quake3::ibsp_version(&bytes).is_some() { q3_found(&bsp, &extra) } else { self.goldsrc_wads(&bsp, &bytes, &extra) };
            if let Some(d) = self.decomp.dialog.as_mut() {
                d.wads = w;
            }
            return;
        }
        if go {
            let (bsp, mut out, wad_dir) = (d.bsp.clone(), std::path::PathBuf::from(d.out.trim()), d.wad_dir.clone());
            if out.extension().is_none() {
                out.set_extension("vmf");
            }
            self.decomp.dialog = None;
            self.run_decompile(bsp, out, wad_dir);
            return;
        }
        if cancel || !open {
            self.decomp.dialog = None;
        }
    }

    pub(crate) fn decompile_window(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui::{self, RichText};
        self.decompile_ask_window(ctx);
        let done = self.decomp.job.as_ref().and_then(|(_, r)| r.lock().unwrap().take());
        if let Some(r) = done {
            self.decomp.job = None;
            match r {
                Ok((out, rep)) => {
                    self.open_in_tab(out.clone());
                    self.ed.status = format!("Dekompiliert: {} Brushes, {} Entities → {}", rep.brushes, rep.entities + rep.static_props, out.display());
                    self.decomp.report = Some(("Map-Decompiler".into(), out.display().to_string(), rep.lines()));
                }
                Err(e) => {
                    self.ed.status = format!("Dekompilieren fehlgeschlagen: {e}");
                    self.decomp.report = Some(("Map-Decompiler".into(), String::new(), vec![e]));
                }
            }
        } else if self.decomp.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let Some((title, file, lines)) = self.decomp.report.clone() else { return };
        let mut open = true;
        let source_bsp = lines.first().is_some_and(|l| l.starts_with("BSP-Version"));
        crate::theme::dialog_simple(ctx, &mut open, &crate::theme::Dlg::new(&title, 560.0).subtitle(file), |ui| {
            for l in &lines {
                ui.add(egui::Label::new(RichText::new(crate::i18n::tr_dyn(l)).size(12.5)).wrap());
            }
            if source_bsp {
                ui.add_space(8.0);
                ui.label(RichText::new(crate::i18n::tr("Nicht wiederherstellbar: Visgroups, Gruppen, Instanzen, Areaportal- und Occluder-Brushes, originale IDs. Displacements stehen als dünne Brushes mit Displacement oben in der Welt.")).small().weak());
            }
            if ui.button(crate::i18n::tr("Schließen")).clicked() {
                self.decomp.report = None;
            }
        });
        if !open {
            self.decomp.report = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: &mut Vec<u8>, x: f32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn i(v: &mut Vec<u8>, x: i32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn h(v: &mut Vec<u8>, x: i16) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    /// Kleine BSP: Welt-Quader (mit Bevel), Detail-Brush, Tür mit origin, Displacement, statisches Prop, Cubemap
    fn test_bsp() -> Vec<u8> {
        let mut l: Vec<Vec<u8>> = vec![Vec::new(); 64];
        // Ebenen: drei Quader
        let boxes = [(DVec3::ZERO, DVec3::splat(64.0)), (DVec3::new(100.0, 0.0, 0.0), DVec3::new(132.0, 32.0, 32.0)), (DVec3::splat(-16.0), DVec3::splat(16.0))];
        let mut planes: Vec<(DVec3, f64)> = Vec::new();
        for (lo, hi) in boxes {
            for (n, d) in [(DVec3::X, hi.x), (-DVec3::X, -lo.x), (DVec3::Y, hi.y), (-DVec3::Y, -lo.y), (DVec3::Z, hi.z), (-DVec3::Z, -lo.z)] {
                planes.push((n, d));
            }
        }
        planes.push((DVec3::Z, 0.0)); // 18: Displacement-Fläche
        for (n, d) in &planes {
            for c in n.to_array() {
                f(&mut l[LUMP_PLANES], c as f32);
            }
            f(&mut l[LUMP_PLANES], *d as f32);
            i(&mut l[LUMP_PLANES], 0);
        }
        l[LUMP_TEXDATA_STRING_DATA] = b"BRICK/BRICKWALL001A\0".to_vec();
        i(&mut l[LUMP_TEXDATA_STRING_TABLE], 0);
        l[LUMP_TEXDATA] = vec![0u8; 12];
        for x in [0, 128, 128, 128, 128] {
            i(&mut l[LUMP_TEXDATA], x);
        }
        for x in [0.25, 0.0, 0.0, 8.0, 0.0, 0.0, -0.25, 0.0, 1.0 / 16.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0 / 16.0, 0.0] {
            f(&mut l[LUMP_TEXINFO], x);
        }
        i(&mut l[LUMP_TEXINFO], 0);
        i(&mut l[LUMP_TEXINFO], 0);
        // Brush-Seiten: Quader 0 mit zusätzlicher Bevel-Seite
        let side = |pl: u16, bevel: u8, out: &mut Vec<u8>| {
            out.extend_from_slice(&pl.to_le_bytes());
            out.extend_from_slice(&0i16.to_le_bytes());
            out.extend_from_slice(&(-1i16).to_le_bytes());
            out.push(bevel);
            out.push(0);
        };
        let mut sides = Vec::new();
        for k in 0..6 {
            side(k, 0, &mut sides);
        }
        side(4, 1, &mut sides);
        for k in 6..18 {
            side(k, 0, &mut sides);
        }
        l[LUMP_BRUSHSIDES] = sides;
        for (first, n, contents) in [(0, 7, 1u32), (7, 6, 1 | CONTENTS_DETAIL), (13, 6, 1)] {
            i(&mut l[LUMP_BRUSHES], first);
            i(&mut l[LUMP_BRUSHES], n);
            i(&mut l[LUMP_BRUSHES], contents as i32);
        }
        // Knoten: 0 → Blätter 0/1 (Welt), 1 → Blatt 2 (Tür)
        for ch in [[-1, -2], [-3, -3]] {
            let mut n = Vec::new();
            i(&mut n, 0);
            i(&mut n, ch[0]);
            i(&mut n, ch[1]);
            n.resize(32, 0);
            l[LUMP_NODES].extend(n);
        }
        for (first, count) in [(0u16, 2u16), (0, 0), (2, 1)] {
            let mut lf = vec![0u8; 24];
            lf.extend_from_slice(&first.to_le_bytes());
            lf.extend_from_slice(&count.to_le_bytes());
            lf.resize(32, 0);
            l[LUMP_LEAFS].extend(lf);
        }
        for b in [0u16, 1, 2] {
            l[LUMP_LEAFBRUSHES].extend_from_slice(&b.to_le_bytes());
        }
        for head in [0, 1] {
            let mut m = vec![0u8; 36];
            i(&mut m, head);
            m.resize(48, 0);
            l[LUMP_MODELS].extend(m);
        }
        l[LUMP_ENTITIES] = b"{\n\"classname\" \"worldspawn\"\n\"skyname\" \"sky_day01_01\"\n}\n{\n\"classname\" \"func_door\"\n\"model\" \"*1\"\n\"origin\" \"0 0 100\"\n\"targetname\" \"tuer\"\n}\n\0".to_vec();
        // Displacement auf einem Viereck bei x = 200
        for p in [[200.0, 0.0, 0.0], [200.0, 64.0, 0.0], [264.0, 64.0, 0.0], [264.0, 0.0, 0.0]] {
            for c in p {
                f(&mut l[LUMP_VERTEXES], c);
            }
        }
        for e in [[0u16, 1], [1, 2], [2, 3], [3, 0]] {
            for v in e {
                l[LUMP_EDGES].extend_from_slice(&v.to_le_bytes());
            }
        }
        for k in 0..4 {
            i(&mut l[LUMP_SURFEDGES], k);
        }
        let mut face = Vec::new();
        face.extend_from_slice(&18u16.to_le_bytes());
        face.extend_from_slice(&[0, 0]);
        i(&mut face, 0);
        h(&mut face, 4);
        h(&mut face, 0);
        h(&mut face, 0);
        face.resize(56, 0);
        l[LUMP_FACES] = face;
        let mut di = Vec::new();
        for c in [200.0, 0.0, 0.0] {
            f(&mut di, c);
        }
        i(&mut di, 0);
        i(&mut di, 0);
        i(&mut di, 2);
        di.resize(36, 0);
        di.extend_from_slice(&0u16.to_le_bytes());
        di.resize(176, 0);
        l[LUMP_DISPINFO] = di;
        for k in 0..25 {
            for c in [0.0, 0.0, 1.0, k as f32, 0.0] {
                f(&mut l[LUMP_DISP_VERTS], c);
            }
        }
        // Cubemap
        for x in [1, 2, 3, 0] {
            i(&mut l[LUMP_CUBEMAPS], x);
        }
        // Zusammensetzen (Game-Lump braucht absolute Offsets → zuletzt)
        let mut b = b"VBSP".to_vec();
        b.extend_from_slice(&20u32.to_le_bytes());
        b.resize(8 + 64 * 16 + 4, 0);
        for k in 0..64 {
            if k == LUMP_GAME_LUMP {
                continue;
            }
            let ofs = b.len();
            b.extend_from_slice(&l[k]);
            b[8 + k * 16..8 + k * 16 + 4].copy_from_slice(&(ofs as u32).to_le_bytes());
            b[8 + k * 16 + 4..8 + k * 16 + 8].copy_from_slice(&(l[k].len() as u32).to_le_bytes());
        }
        let mut sp = Vec::new();
        i(&mut sp, 1);
        let mut name = b"models/props/kiste.mdl".to_vec();
        name.resize(128, 0);
        sp.extend(name);
        i(&mut sp, 0);
        i(&mut sp, 1);
        let mut p = Vec::new();
        for c in [10.0, 20.0, 30.0, 0.0, 90.0, 0.0] {
            f(&mut p, c);
        }
        p.extend_from_slice(&[0, 0, 0, 0, 0, 0, 6, 0x10]);
        i(&mut p, 2);
        f(&mut p, 100.0);
        f(&mut p, 200.0);
        p.resize(76, 0);
        sp.extend(p);
        let gofs = b.len();
        let mut g = Vec::new();
        i(&mut g, 1);
        i(&mut g, u32::from_be_bytes(*b"sprp") as i32);
        g.extend_from_slice(&0u16.to_le_bytes());
        g.extend_from_slice(&10u16.to_le_bytes());
        i(&mut g, (gofs + 20) as i32);
        i(&mut g, sp.len() as i32);
        g.extend(sp);
        b.extend_from_slice(&g);
        let k = LUMP_GAME_LUMP;
        b[8 + k * 16..8 + k * 16 + 4].copy_from_slice(&(gofs as u32).to_le_bytes());
        b[8 + k * 16 + 4..8 + k * 16 + 8].copy_from_slice(&(g.len() as u32).to_le_bytes());
        b
    }

    #[test]
    fn decompile_small_bsp() {
        if let Ok(p) = std::env::var("AMBOSS_WRITE_TEST_BSP") {
            std::fs::write(p, test_bsp()).unwrap();
        }
        let (map, r) = decompile(&test_bsp()).unwrap();
        assert_eq!(r.version, 20);
        assert_eq!(map.world.get("skyname"), Some("sky_day01_01"));
        // Welt: Quader + Displacement-Brush
        assert_eq!(map.world.solids.len(), 2, "{r:?}");
        let wb = map.world.solids.iter().find(|s| !s.has_disp()).unwrap();
        assert_eq!(wb.sides.len(), 6, "Bevel-Seite muss fehlen");
        assert_eq!(wb.bounds.min, DVec3::ZERO);
        assert_eq!(wb.bounds.max, DVec3::splat(64.0));
        let s0 = &wb.sides[0];
        assert_eq!(s0.material, "BRICK/BRICKWALL001A");
        assert!((s0.uaxis.scale - 4.0).abs() < 1e-6 && (s0.uaxis.shift - 8.0).abs() < 1e-6 && s0.uaxis.axis == DVec3::X);
        assert_eq!(s0.lightmapscale, 16);
        // Displacement
        let db = map.world.solids.iter().find(|s| s.has_disp()).unwrap();
        let d = db.sides.iter().find_map(|s| s.disp.as_ref()).unwrap();
        assert_eq!(d.power, 2);
        assert_eq!(d.start, DVec3::new(200.0, 0.0, 0.0));
        assert_eq!(d.distances[24], 24.0);
        assert_eq!(db.bounds.max.z, 24.0, "Displacement-Punkte zählen zur Größe");
        assert_eq!(db.bounds.min.z, -8.0);
        // Detail, Tür (um origin verschoben), Prop, Cubemap
        let ents: Vec<&Entity> = map.entities.iter().map(|e| &**e).collect();
        assert_eq!(ents.iter().filter(|e| e.classname() == "func_detail").count(), 1);
        let door = ents.iter().find(|e| e.classname() == "func_door").unwrap();
        assert_eq!(door.solids.len(), 1);
        assert_eq!(door.solids[0].bounds.min, DVec3::new(-16.0, -16.0, 84.0));
        assert_eq!(door.get("targetname"), Some("tuer"));
        assert!(door.get("model").is_none());
        let prop = ents.iter().find(|e| e.classname() == "prop_static").unwrap();
        assert_eq!(prop.get("model"), Some("models/props/kiste.mdl"));
        assert_eq!(prop.get("origin"), Some("10 20 30"));
        assert_eq!(prop.get("angles"), Some("0 90 0"));
        assert_eq!(prop.get("skin"), Some("2"));
        assert_eq!(prop.get("disableshadows"), Some("1"));
        assert!(ents.iter().any(|e| e.classname() == "env_cubemap" && e.get("origin") == Some("1 2 3")));
        assert_eq!((r.brushes, r.detail, r.brush_entities, r.displacements, r.static_props, r.cubemaps), (3, 1, 1, 1, 1, 1));
        // VMF lässt sich wieder lesen
        let back = Map::from_vmf(&map.to_vmf()).unwrap();
        assert_eq!(back.world.solids.len(), 2);
        assert!(back.world.solids.iter().any(|s| s.has_disp()));
        assert!(decompile(b"kaputt").is_err());
    }
}
