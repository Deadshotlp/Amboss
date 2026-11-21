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
    import_map_ex(src, tex_scale_default, &mut crate::quake3::Assets::default()).map(|r| r.0)
}

/// Patch (patchDef2/patchDef3) lesen; der Name ist schon gelesen
fn parse_patch(p: &mut P) -> Result<crate::quake3::Patch, String> {
    p.expect('{')?;
    let texture = p.word()?;
    p.expect('(')?;
    let w = p.num()? as usize;
    let h = p.num()? as usize;
    while !matches!(p.peek(), Some(Tok::Sym(')'))) {
        p.num()?;
    }
    p.expect(')')?;
    if w == 0 || h == 0 || w * h > 65536 {
        return Err(format!("Patch mit ungültiger Größe {w}×{h}"));
    }
    let mut pts = vec![(DVec3::ZERO, glam::DVec2::ZERO); w * h];
    p.expect('(')?;
    for x in 0..w {
        p.expect('(')?;
        for y in 0..h {
            p.expect('(')?;
            let pos = DVec3::new(p.num()?, p.num()?, p.num()?);
            let st = glam::DVec2::new(p.num()?, p.num()?);
            p.expect(')')?;
            pts[y * w + x] = (pos, st);
        }
        p.expect(')')?;
    }
    p.expect(')')?;
    p.expect('}')?;
    Ok(crate::quake3::Patch { texture, w, h, pts })
}

fn mk_side(plane_pts: [DVec3; 3], material: String, uaxis: TexAxis, vaxis: TexAxis) -> Side {
    Side { id: 0, plane_pts, material, uaxis, vaxis, rotation: 0.0, lightmapscale: 16, smoothing_groups: "0".into(), disp: None, extra: Vec::new() }
}

/// Wie `import_map`, löst aber Quake-3-Texturen über `assets` auf (wandelt sie um), liest
/// Patches und brushDef und gibt Hinweise zurück
pub fn import_map_ex(src: &str, tex_scale_default: f64, assets: &mut crate::quake3::Assets) -> Result<(Map, Vec<String>), String> {
    use crate::quake3::{Kind, TexRes};
    let q3 = src.contains("patchDef") || src.contains("brushDef") || src.contains("common/caulk") || src.contains("common/clip") || src.contains("common/trigger");
    let resolve = |name: &str, assets: &mut crate::quake3::Assets| -> TexRes {
        if q3 {
            assets.resolve(name)
        } else {
            crate::quake3::special(name).unwrap_or_else(|| TexRes::plain(&name.to_uppercase()))
        }
    };
    let mut notes = Vec::new();
    let (mut patches, mut bad_patches, mut fog) = (0usize, 0usize, 0usize);
    let mut p = P { t: tokenize(src), i: 0 };
    let mut map = Map::new();
    let mut first = true;
    let mut brushes = 0;
    let mut foreign: Vec<String> = Vec::new();
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
                    let mut kinds = Vec::new();
                    // Quake-3-/Doom-3-Patches → Displacements
                    if matches!(p.peek(), Some(Tok::Word(w)) if w == "patchDef2" || w == "patchDef3") {
                        p.i += 1;
                        let pt = parse_patch(&mut p)?;
                        p.expect('}')?;
                        let tex = resolve(&pt.texture, assets);
                        let (ss, bad) = crate::quake3::patch_solids(&mut map, &pt, &tex);
                        patches += 1;
                        bad_patches += bad;
                        solids.extend(ss);
                        continue;
                    }
                    // brushDef (Q3 „Brush Primitives“) und brushDef3 (Doom 3)
                    if matches!(p.peek(), Some(Tok::Word(w)) if w == "brushDef" || w == "brushDef3") {
                        let d3 = matches!(p.peek(), Some(Tok::Word(w)) if w == "brushDef3");
                        p.i += 1;
                        p.expect('{')?;
                        while !matches!(p.peek(), Some(Tok::Sym('}'))) {
                            let pts = if d3 {
                                p.expect('(')?;
                                let n = DVec3::new(p.num()?, p.num()?, p.num()?);
                                let d = p.num()?;
                                p.expect(')')?;
                                let pl = crate::geometry::Plane { normal: n.normalize_or(DVec3::Z), dist: -d / n.length().max(1e-9) };
                                let w = crate::geometry::base_winding(&pl);
                                crate::geometry::plane_points_for(pl.normal, &w).unwrap_or([w[0], w[1], w[2]])
                            } else {
                                [p.point()?, p.point()?, p.point()?]
                            };
                            p.expect('(')?;
                            let mut m = [[0.0; 3]; 2];
                            for row in &mut m {
                                p.expect('(')?;
                                for v in row.iter_mut() {
                                    *v = p.num()?;
                                }
                                p.expect(')')?;
                            }
                            p.expect(')')?;
                            let tex = resolve(&p.word()?, assets);
                            while matches!(p.peek(), Some(Tok::Word(w)) if w.parse::<f64>().is_ok()) {
                                p.i += 1;
                            }
                            let normal = crate::geometry::Plane::from_points(&pts).map(|pl| pl.normal).unwrap_or(DVec3::Z);
                            let (u, v) = crate::quake3::bp_axes(normal, m, tex.vw, tex.vh);
                            kinds.push(tex.kind);
                            sides.push(mk_side(pts, tex.material, u, v));
                        }
                        p.expect('}')?;
                        p.expect('}')?;
                    } else if matches!(p.peek(), Some(Tok::Word(w)) if w.starts_with("patch")) {
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
                    } else {
                    while !matches!(p.peek(), Some(Tok::Sym('}'))) {
                        let pts = [p.point()?, p.point()?, p.point()?];
                        let tex = resolve(&p.word()?, assets);
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
                            // Bild in eine andere Größe umgewandelt (2ⁿ): Texel-Angaben anpassen
                            let (fu, fv) = (tex.vw / tex.w.max(1.0), tex.vh / tex.h.max(1.0));
                            u.scale /= fu;
                            u.shift *= fu;
                            v.scale /= fv;
                            v.shift *= fv;
                            (u, v)
                        };
                        // optionale Quake-2/3-Felder (Content, Flags, Value)
                        while matches!(p.peek(), Some(Tok::Word(w)) if w.parse::<f64>().is_ok()) {
                            p.i += 1;
                        }
                        kinds.push(tex.kind);
                        sides.push(mk_side(pts, tex.material, uaxis, vaxis));
                    }
                    p.expect('}')?;
                    }
                    if kinds.contains(&Kind::Fog) {
                        fog += 1;
                        continue;
                    }
                    let water = kinds.contains(&Kind::Water);
                    let mut s = Solid::new(0, sides);
                    if !s.is_valid() && src.contains("brushDef3") {
                        // Doom 3: Ebenen zeigen evtl. nach innen
                        for sd in &mut s.sides {
                            sd.plane_pts.swap(0, 2);
                        }
                        s.update_geometry();
                    }
                    if water {
                        for (k, sd) in s.sides.iter_mut().enumerate() {
                            let up = s.planes.get(k).copied().flatten().is_some_and(|pl| pl.normal.z > 0.7);
                            sd.material = if up { crate::goldsrc::WATER_MATERIAL.into() } else { "TOOLS/TOOLSNODRAW".into() };
                        }
                    }
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
        // Quake 3: func_group gehört zur Welt, Entities umwandeln
        let is_group = q3 && e.classname() == "func_group";
        if q3 && !first && e.classname() != "worldspawn" && !is_group {
            let class = e.classname().to_string();
            if crate::quake3::foreign_class(&class) && !foreign.contains(&class) {
                foreign.push(class);
            }
            let (nc, props) = crate::quake3::convert_entity(&e.props, !solids.is_empty());
            e.props = vec![("classname".into(), nc)];
            e.props.extend(props);
        }
        if is_group {
            for mut s in solids {
                map.reassign_ids(&mut s);
                map.world.solids.push(s.into());
            }
            continue;
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
    if q3 {
        if patches > 0 {
            notes.push(format!("{patches} gebogene Flächen (Patches) → Displacements"));
        }
        if bad_patches > 0 {
            notes.push(format!("{bad_patches} Patch-Teile ließen sich nicht als Displacement abbilden (z. B. Dreiecksform)"));
        }
        if fog > 0 {
            notes.push(format!("{fog} Nebel-Volumen ausgelassen (in Source: env_fog_controller)"));
        }
        if !foreign.is_empty() {
            foreign.sort();
            notes.push(format!("Klassen ohne Source-Gegenstück (bleiben erhalten, wirken im Spiel nicht): {}", foreign.join(", ")));
        }
        if assets.converted > 0 {
            notes.push(format!("{} Texturen nach VTF umgewandelt", assets.converted));
        }
        notes.extend(assets.notes());
        notes.push("Maßstab 1:1 übernommen – Quake-3-Spieler sind kleiner (56 statt 72 Einheiten); bei Bedarf mit Transformieren auf 125 % skalieren".into());
    }
    if brushes == 0 && map.entities.is_empty() && patches == 0 {
        return Err("Keine Brushes oder Entities gefunden".into());
    }
    Ok((map, notes))
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

    /// Quake-3-Map: Brush (Standard), brushDef, Patch, Wasser in func_group, Licht; Textur aus pk3
    #[test]
    fn import_quake3_map() {
        let pts = [
            "( -64 -64 -16 ) ( -64 -63 -16 ) ( -64 -64 -15 )",
            "( -64 -64 -16 ) ( -64 -64 -15 ) ( -63 -64 -16 )",
            "( -64 -64 -16 ) ( -63 -64 -16 ) ( -64 -63 -16 )",
            "( 64 64 16 ) ( 64 65 16 ) ( 65 64 16 )",
            "( 64 64 16 ) ( 65 64 16 ) ( 64 64 17 )",
            "( 64 64 16 ) ( 64 64 17 ) ( 64 65 16 )",
        ];
        let std_brush = |tex: &str, dz: f64| {
            let mut b = String::from("{\n");
            for p in pts {
                let p = if dz != 0.0 { p.replace(" -16 )", &format!(" {} )", -16.0 + dz)).replace(" -15 )", &format!(" {} )", -15.0 + dz)).replace(" 16 )", &format!(" {} )", 16.0 + dz)).replace(" 17 )", &format!(" {} )", 17.0 + dz)) } else { p.to_string() };
                b.push_str(&format!("{p} {tex} 0 0 0 0.5 0.5 0 0 0\n"));
            }
            b.push_str("}\n");
            b
        };
        let mut bd = String::from("{\nbrushDef\n{\n");
        for p in pts {
            // x um 200 verschieben
            let p: String = p
                .split(')')
                .filter(|q| q.contains('('))
                .map(|q| {
                    let n: Vec<f64> = q.replace('(', " ").split_whitespace().map(|x| x.parse().unwrap()).collect();
                    format!("( {} {} {} )", n[0] + 200.0, n[1], n[2])
                })
                .collect::<Vec<_>>()
                .join(" ");
            bd.push_str(&format!("{p} ( ( 0.0078125 0 0 ) ( 0 0.0078125 0 ) ) base_wall/test 0 0 0\n"));
        }
        bd.push_str("}\n}\n");
        let map = format!(
            "// Game: Quake 3\n{{\n\"classname\" \"worldspawn\"\n\"message\" \"q3test\"\n{}{bd}{{\npatchDef2\n{{\nbase_wall/test\n( 3 3 0 0 0 )\n(\n( ( 0 300 0 0 0 ) ( 0 364 0 0 0.5 ) ( 0 428 0 0 1 ) )\n( ( 64 300 64 0.5 0 ) ( 64 364 64 0.5 0.5 ) ( 64 428 64 0.5 1 ) )\n( ( 128 300 0 1 0 ) ( 128 364 0 1 0.5 ) ( 128 428 0 1 1 ) )\n)\n}}\n}}\n}}\n{{\n\"classname\" \"func_group\"\n{}}}\n{{\n\"classname\" \"light\"\n\"origin\" \"0 0 64\"\n\"light\" \"250\"\n\"_color\" \"1 1 1\"\n}}\n{{\n\"classname\" \"weapon_shotgun\"\n\"origin\" \"0 0 0\"\n}}\n{{\n\"classname\" \"func_door\"\n\"angle\" \"90\"\n{}}}\n",
            std_brush("base_wall/test", 0.0),
            std_brush("liquids/clear_calm1", 100.0),
            std_brush("common/caulk", 300.0)
        );
        // Spielordner mit pk3 (TGA 4×4 + Shader)
        let dir = std::env::temp_dir().join(format!("amboss_q3_{}", std::process::id()));
        let base = dir.join("baseq3");
        std::fs::create_dir_all(base.join("maps")).unwrap();
        let mut tga = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 4, 0, 24, 0x20];
        tga.extend(std::iter::repeat([10u8, 20, 30]).take(16).flatten());
        let shader = b"textures/base_wall/test\n{\n {\n  map $lightmap\n }\n {\n  map textures/base_wall/test_img.tga\n }\n}\n".to_vec();
        let entry = |name: &str, data: Vec<u8>| crate::bsppack::ZipEntry { name: name.into(), method: 0, crc: 0, csize: data.len() as u32, usize: data.len() as u32, data };
        std::fs::write(base.join("pak0.pk3"), crate::bsppack::write_zip(&[entry("textures/base_wall/test_img.tga", tga), entry("scripts/test.shader", shader)])).unwrap();
        let mp = base.join("maps").join("q3test.map");
        std::fs::write(&mp, &map).unwrap();
        let (m, notes, files) = crate::prefabs::load_foreign_map_ex(&mp, 0.25, &[]).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(m.world.get("message"), Some("q3test"));
        // Welt: Standard-Brush, brushDef, Patch und Wasser (aus func_group)
        assert_eq!(m.world.solids.len(), 4, "{notes:?}");
        let box0 = m.world.solids.iter().find(|s| s.bounds.min.x == -64.0 && s.bounds.max.z == 16.0).unwrap();
        assert_eq!(box0.sides[0].material, "QUAKE3/BASE_WALL/TEST");
        // 4×4-TGA → VTF 4×4: Maßstab bleibt 0.5
        assert!((box0.sides[0].uaxis.scale - 0.5).abs() < 1e-9);
        let bd = m.world.solids.iter().find(|s| s.bounds.min.x == 136.0).expect("brushDef");
        let top = bd.sides.iter().find(|s| s.plane().is_some_and(|p| p.normal.z > 0.99)).unwrap();
        assert_eq!(top.material, "QUAKE3/BASE_WALL/TEST");
        // 1/128 je Einheit bei 4 px → 32 Einheiten je Texel
        assert!((top.uaxis.scale - 32.0).abs() < 1e-6, "{:?}", top.uaxis);
        let patch = m.world.solids.iter().find(|s| s.has_disp()).expect("Patch");
        assert!(patch.bounds.max.z > 30.0);
        let water = m.world.solids.iter().find(|s| s.sides.iter().any(|sd| sd.material == crate::goldsrc::WATER_MATERIAL)).expect("Wasser");
        assert_eq!(water.sides.iter().filter(|sd| sd.material == crate::goldsrc::WATER_MATERIAL).count(), 1);
        // Entities
        let light = m.entities.iter().find(|e| e.classname() == "light").unwrap();
        assert_eq!(light.get("_light"), Some("255 255 255 250"));
        let door = m.entities.iter().find(|e| e.classname() == "func_door").unwrap();
        assert_eq!(door.get("movedir"), Some("0 90 0"));
        assert_eq!(door.solids[0].sides[0].material, "TOOLS/TOOLSNODRAW");
        assert!(notes.iter().any(|n| n.contains("weapon_shotgun")), "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("1 Texturen")), "{notes:?}");
        let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        assert_eq!(names, vec!["quake3/base_wall/test.vtf", "quake3/base_wall/test.vmt"]);
    }
}
