//! Import von .map (Valve 220 und Quake-Standard, z. B. aus TrenchBroom/J.A.C.K.) und Export als OBJ.

use crate::editor::Editor;
use crate::map::{default_tex_axes, Entity, Map, Side, Solid, TexAxis};
use glam::DVec3;
use std::fmt::Write;

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Word(String),
    Sym(char),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '"' {
            i += 1;
            let mut s = String::new();
            while i < c.len() && c[i] != '"' {
                s.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if "{}()[]".contains(ch) {
            out.push(Tok::Sym(ch));
            i += 1;
        } else {
            let mut s = String::new();
            while i < c.len() && !c[i].is_whitespace() && !"{}()[]\"".contains(c[i]) {
                s.push(c[i]);
                i += 1;
            }
            out.push(Tok::Word(s));
        }
    }
    out
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn next(&mut self) -> Option<&Tok> {
        let t = self.t.get(self.i);
        self.i += 1;
        t
    }
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        match self.next() {
            Some(Tok::Sym(x)) if *x == c => Ok(()),
            other => Err(format!("'{c}' erwartet, gefunden {other:?}")),
        }
    }
    fn num(&mut self) -> Result<f64, String> {
        match self.next() {
            Some(Tok::Word(w)) => w.parse().map_err(|_| format!("Zahl erwartet: {w}")),
            other => Err(format!("Zahl erwartet, gefunden {other:?}")),
        }
    }
    fn point(&mut self) -> Result<DVec3, String> {
        self.expect('(')?;
        let p = DVec3::new(self.num()?, self.num()?, self.num()?);
        self.expect(')')?;
        Ok(p)
    }
    fn word(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Tok::Word(w)) | Some(Tok::Str(w)) => Ok(w.clone()),
            other => Err(format!("Name erwartet, gefunden {other:?}")),
        }
    }
}

/// Liest eine .map-Datei (Valve 220 oder Quake-Standardformat) und baut eine Map daraus.
pub fn import_map(src: &str, tex_scale_default: f64) -> Result<Map, String> {
    let mut p = P { t: tokenize(src), i: 0 };
    let mut map = Map::new();
    let mut first = true;
    let mut brushes = 0;
    while p.peek().is_some() {
        p.expect('{')?;
        let mut e = Entity::new(0, "");
        e.props.clear();
        let mut solids: Vec<Solid> = Vec::new();
        loop {
            match p.peek() {
                Some(Tok::Sym('}')) => {
                    p.i += 1;
                    break;
                }
                Some(Tok::Str(_)) => {
                    let k = p.word()?;
                    let v = p.word()?;
                    e.props.push((k, v));
                }
                Some(Tok::Sym('{')) => {
                    p.i += 1;
                    let mut sides = Vec::new();
                    // Patches/Brush-Definitionen anderer Formate überspringen
                    if matches!(p.peek(), Some(Tok::Word(w)) if w.starts_with("patch") || w == "brushDef" || w == "brushDef3") {
                        let mut depth = 1;
                        while depth > 0 {
                            match p.next() {
                                Some(Tok::Sym('{')) => depth += 1,
                                Some(Tok::Sym('}')) => depth -= 1,
                                None => return Err("Unerwartetes Dateiende".into()),
                                _ => {}
                            }
                        }
                        continue;
                    }
                    while !matches!(p.peek(), Some(Tok::Sym('}'))) {
                        let pts = [p.point()?, p.point()?, p.point()?];
                        let tex = p.word()?;
                        let normal = crate::geometry::Plane::from_points(&pts).map(|pl| pl.normal).unwrap_or(DVec3::Z);
                        let (uaxis, vaxis) = if matches!(p.peek(), Some(Tok::Sym('['))) {
                            // Valve 220: [ ux uy uz off ] [ vx vy vz off ] rot sx sy
                            p.expect('[')?;
                            let u = DVec3::new(p.num()?, p.num()?, p.num()?);
                            let uo = p.num()?;
                            p.expect(']')?;
                            p.expect('[')?;
                            let v = DVec3::new(p.num()?, p.num()?, p.num()?);
                            let vo = p.num()?;
                            p.expect(']')?;
                            let _rot = p.num()?;
                            let su = p.num()?;
                            let sv = p.num()?;
                            (TexAxis { axis: u, shift: uo, scale: su }, TexAxis { axis: v, shift: vo, scale: sv })
                        } else {
                            // Quake: offX offY rot sX sY
                            let ox = p.num()?;
                            let oy = p.num()?;
                            let _rot = p.num()?;
                            let sx = p.num()?;
                            let sy = p.num()?;
                            let (mut u, mut v) = default_tex_axes(normal, tex_scale_default);
                            u.shift = ox;
                            v.shift = oy;
                            u.scale = if sx == 0.0 { tex_scale_default } else { sx };
                            v.scale = if sy == 0.0 { tex_scale_default } else { sy };
                            (u, v)
                        };
                        // optionale Quake-2/3-Felder (Content, Flags, Value)
                        while matches!(p.peek(), Some(Tok::Word(w)) if w.parse::<f64>().is_ok()) {
                            p.i += 1;
                        }
                        sides.push(Side {
                            id: 0,
                            plane_pts: pts,
                            material: tex.to_uppercase(),
                            uaxis,
                            vaxis,
                            rotation: 0.0,
                            lightmapscale: 16,
                            smoothing_groups: "0".into(),
                            disp: None,
                            extra: Vec::new(),
                        });
                    }
                    p.expect('}')?;
                    let mut s = Solid::new(0, sides);
                    if s.is_valid() {
                        Editor::normalize_planes(&mut s);
                        solids.push(s);
                        brushes += 1;
                    }
                }
                Some(t) => return Err(format!("Unerwartetes Symbol {t:?}")),
                None => return Err("Unerwartetes Dateiende".into()),
            }
        }
        if first || e.classname() == "worldspawn" {
            first = false;
            for mut s in solids {
                map.reassign_ids(&mut s);
                map.world.solids.push(s.into());
            }
            for (k, v) in e.props {
                if !k.eq_ignore_ascii_case("classname") && !k.eq_ignore_ascii_case("mapversion") && !k.eq_ignore_ascii_case("wad") {
                    map.world.set(&k, v);
                }
            }
        } else {
            e.id = map.alloc_id();
            for mut s in solids {
                map.reassign_ids(&mut s);
                e.solids.push(s.into());
            }
            map.entities.push(e.into());
        }
    }
    if brushes == 0 && map.entities.is_empty() {
        return Err("Keine Brushes oder Entities gefunden".into());
    }
    Ok(map)
}

/// Exportiert sichtbare Geometrie als Wavefront OBJ (+ MTL). `dims` liefert Texturgrößen.
pub fn export_obj(ed: &Editor, scale: f64, dims: &dyn Fn(&str) -> (f64, f64), mtl_name: &str) -> (String, String) {
    let mut obj = String::new();
    let mut mtl = String::new();
    let _ = writeln!(obj, "# Exportiert mit Amboss\nmtllib {mtl_name}");
    let (mut vi, mut ti, mut ni) = (1usize, 1usize, 1usize);
    let mut materials: Vec<String> = Vec::new();
    for (e, s) in ed.map.solids_owned() {
        if !ed.is_visible_solid(e, s) {
            continue;
        }
        let _ = writeln!(obj, "o brush_{}", s.id);
        for (i, poly) in s.polys.iter().enumerate() {
            let side = &s.sides[i];
            if poly.len() < 3 || (s.has_disp() && side.disp.is_none()) || crate::editor::is_tool_material(&side.material) {
                continue;
            }
            let mat = side.material.to_ascii_lowercase();
            if !materials.contains(&mat) {
                materials.push(mat.clone());
            }
            let _ = writeln!(obj, "usemtl {mat}");
            let n = s.planes[i].map(|p| p.normal).unwrap_or(DVec3::Z);
            let (tw, th) = dims(&side.material);
            let uv = |p: DVec3| (side.uaxis.coord(p) / tw, -side.vaxis.coord(p) / th);
            let (pts, base, tris): (Vec<DVec3>, Vec<DVec3>, Vec<[usize; 3]>) = match s.disps.get(i).cloned().flatten() {
                Some(g) => {
                    let t = g.triangles();
                    (g.pos, g.base, t)
                }
                None => ((poly.clone()), poly.clone(), (1..poly.len() - 1).map(|k| [0, k, k + 1]).collect()),
            };
            for p in &pts {
                let _ = writeln!(obj, "v {} {} {}", p.x * scale, p.y * scale, p.z * scale);
            }
            for b in &base {
                let (u, v) = uv(*b);
                let _ = writeln!(obj, "vt {u} {v}");
            }
            let _ = writeln!(obj, "vn {} {} {}", n.x, n.y, n.z);
            for t in tris {
                let _ = writeln!(obj, "f {}/{}/{} {}/{}/{} {}/{}/{}", vi + t[0], ti + t[0], ni, vi + t[1], ti + t[1], ni, vi + t[2], ti + t[2], ni);
            }
            vi += pts.len();
            ti += base.len();
            ni += 1;
        }
    }
    for m in &materials {
        let _ = writeln!(mtl, "newmtl {m}\nKd 0.8 0.8 0.8\nmap_Kd {m}.png\n");
    }
    (obj, mtl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;
    use crate::geometry::Aabb;

    #[test]
    fn import_valve220_and_quake() {
        let v220 = r#"
// Game: Half-Life
// Format: Valve
{
"classname" "worldspawn"
"wad" "x.wad"
{
( -64 -64 -16 ) ( -64 -63 -16 ) ( -64 -64 -15 ) __TB_empty [ 0 -1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -64 -64 -16 ) ( -64 -64 -15 ) ( -63 -64 -16 ) __TB_empty [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -64 -64 -16 ) ( -63 -64 -16 ) ( -64 -63 -16 ) __TB_empty [ -1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 64 64 16 ) ( 64 65 16 ) ( 65 64 16 ) __TB_empty [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 64 64 16 ) ( 65 64 16 ) ( 64 64 17 ) __TB_empty [ -1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 64 64 16 ) ( 64 64 17 ) ( 64 65 16 ) __TB_empty [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
{
"classname" "info_player_start"
"origin" "0 0 40"
}
"#;
        let m = import_map(v220, 0.25).unwrap();
        assert_eq!(m.world.solids.len(), 1);
        let b = m.world.solids[0].bounds;
        assert_eq!(b.min, DVec3::new(-64.0, -64.0, -16.0));
        assert_eq!(b.max, DVec3::new(64.0, 64.0, 16.0));
        assert_eq!(m.entities.len(), 1);
        assert_eq!(m.entities[0].classname(), "info_player_start");
        // als VMF speicherbar und wieder ladbar
        let back = Map::from_vmf(&m.to_vmf()).unwrap();
        assert_eq!(back.world.solids[0].bounds, b);

        let quake = v220.replace("[ 0 -1 0 0 ] [ 0 0 -1 0 ] 0 1 1", "0 0 0 1 1").replace("[ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1", "0 0 0 1 1").replace("[ -1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1", "0 0 0 1 1").replace("[ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1", "0 0 0 1 1").replace("[ -1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1", "0 0 0 1 1").replace("[ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1", "0 0 0 1 1");
        assert_eq!(import_map(&quake, 0.25).unwrap().world.solids.len(), 1);
    }

    #[test]
    fn obj_export() {
        let mut ed = Editor::new(Fgd::builtin());
        ed.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let (obj, mtl) = export_obj(&ed, 1.0, &|_| (512.0, 512.0), "x.mtl");
        assert_eq!(obj.lines().filter(|l| l.starts_with("v ")).count(), 24);
        assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 12);
        assert!(mtl.contains("newmtl dev/dev_measuregeneric01b"));
    }
}
