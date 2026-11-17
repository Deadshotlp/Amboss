//! Source-Modelle laden: .mdl (Header, Texturen, Skins), .vvd (Vertices), .vtx (Dreiecke).

use crate::gamefs::GameFs;
use crate::geometry::Aabb;
use glam::DVec3;

#[derive(Debug, Clone)]
pub struct ModelMesh {
    /// Index in die Skin-Tabelle (skinref)
    pub skinref: usize,
    /// pos(3) normal(3) uv(2)
    pub verts: Vec<[f32; 8]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub meshes: Vec<ModelMesh>,
    /// Materialpfade (ohne "materials/" und ".vmt") je Textur-Index
    pub materials: Vec<String>,
    /// Skin-Familien: Familie → (skinref → Textur-Index)
    pub skins: Vec<Vec<usize>>,
    pub bounds: Aabb,
    /// Kanten des Kollisionsmodells (.phy) in Modell-Koordinaten: [ax, ay, az, bx, by, bz]
    pub phys_edges: Vec<[f32; 6]>,
}

impl Model {
    pub fn material_for(&self, skin: usize, skinref: usize) -> Option<&str> {
        let fam = self.skins.get(skin).or_else(|| self.skins.first())?;
        let tex = *fam.get(skinref).unwrap_or(&skinref);
        self.materials.get(tex).map(|s| s.as_str())
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len() / 3).sum()
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn i32(&self, o: usize) -> Result<i32, String> {
        self.0.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| format!("Datei zu kurz (Offset {o})"))
    }
    fn u16(&self, o: usize) -> Result<u16, String> {
        self.0.get(o..o + 2).map(|b| u16::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| format!("Datei zu kurz (Offset {o})"))
    }
    fn i16(&self, o: usize) -> Result<i16, String> {
        Ok(self.u16(o)? as i16)
    }
    fn f32(&self, o: usize) -> Result<f32, String> {
        self.0.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| format!("Datei zu kurz (Offset {o})"))
    }
    fn cstr(&self, o: usize) -> Result<String, String> {
        let b = self.0.get(o..).ok_or("ungültiger String-Offset")?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8_lossy(&b[..end]).to_string())
    }
    fn off(&self, base: usize, rel: i32) -> Result<usize, String> {
        let v = base as i64 + rel as i64;
        if v < 0 || v as usize > self.0.len() {
            return Err("ungültiger Offset".into());
        }
        Ok(v as usize)
    }
}

pub fn load(fs: &GameFs, path: &str) -> Result<Model, String> {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    let mdl_b = fs.read(&path).ok_or_else(|| format!("{path} nicht gefunden"))?;
    let base = path.trim_end_matches(".mdl");
    let vvd_b = fs.read(&format!("{base}.vvd")).ok_or_else(|| format!("{base}.vvd nicht gefunden"))?;
    let vtx_b = ["dx90.vtx", "vtx", "dx80.vtx", "sw.vtx"].iter().find_map(|e| fs.read(&format!("{base}.{e}"))).ok_or_else(|| format!("{base}.vtx nicht gefunden"))?;
    let mut m = parse(&path, &mdl_b, &vvd_b, &vtx_b, &|p| fs.exists(p))?;
    if let Some(phy) = fs.read(&format!("{base}.phy")) {
        m.phys_edges = parse_phy(&phy).unwrap_or_default();
    }
    Ok(m)
}

/// Kollisionsmodell (.phy) lesen: alle konvexen Teile (IVP-Ledges) als Kantenliste.
/// IVP rechnet in Metern mit vertauschten Achsen: Source (x, y, z) = (x, z, −y) / 0,0254.
pub fn parse_phy(b: &[u8]) -> Result<Vec<[f32; 6]>, String> {
    let r = R(b);
    let header = r.i32(0)?.max(16) as usize;
    let solids = r.i32(8)?.clamp(0, 128) as usize;
    let k = 1.0 / 0.0254;
    let conv = |o: usize| -> Result<[f32; 3], String> { Ok([r.f32(o)? * k, r.f32(o + 8)? * k, -r.f32(o + 4)? * k]) };
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut pos = header;
    for _ in 0..solids {
        let size = r.i32(pos)?.max(0) as usize;
        let start = pos + 4;
        let end = (start + size).min(b.len());
        pos = start + size;
        // Neues Format: "VPHY"-Kopf (28 Bytes), danach die IVP-Oberfläche
        let surf = if b.get(start..start + 4) == Some(b"VPHY") {
            if r.i16(start + 6)? != 0 {
                continue; // nur konvexe Oberflächen (kein MOPP)
            }
            start + 28
        } else {
            start
        };
        let tree = surf + r.i32(surf + 32)?.max(0) as usize;
        let mut ledge = surf + 48;
        let mut verts_start = end;
        let mut guard = 0;
        while ledge + 16 <= end && ledge < verts_start && ledge < tree.max(surf + 48 + 16) && guard < 100_000 {
            guard += 1;
            let pts = r.off(ledge, r.i32(ledge)?)?;
            verts_start = verts_start.min(pts);
            let n_tri = r.i16(ledge + 12)?.max(0) as usize;
            if n_tri == 0 {
                break;
            }
            for t in 0..n_tri {
                let tri = ledge + 16 + t * 16;
                let idx: Vec<usize> = (0..3).map(|e| r.u16(tri + 4 + e * 4).map(|v| v as usize)).collect::<Result<_, _>>()?;
                for e in 0..3 {
                    let (a, c) = (idx[e], idx[(e + 1) % 3]);
                    let key = (pts + a.min(c) * 16, pts + a.max(c) * 16);
                    if seen.insert(key) {
                        let (pa, pc) = (conv(pts + a * 16)?, conv(pts + c * 16)?);
                        edges.push([pa[0], pa[1], pa[2], pc[0], pc[1], pc[2]]);
                    }
                }
            }
            ledge += 16 + n_tri * 16;
        }
    }
    Ok(edges)
}

pub fn parse(name: &str, mdl_b: &[u8], vvd_b: &[u8], vtx_b: &[u8], exists: &dyn Fn(&str) -> bool) -> Result<Model, String> {
    let mdl = R(mdl_b);
    if mdl_b.get(0..4) != Some(b"IDST") {
        return Err("keine MDL-Datei".into());
    }
    let version = mdl.i32(4)?;
    if !(44..=49).contains(&version) {
        return Err(format!("MDL-Version {version} nicht unterstützt"));
    }

    // Texturen + Suchpfade
    let num_tex = mdl.i32(204)?.max(0) as usize;
    let tex_index = mdl.i32(208)? as usize;
    let mut tex_names = Vec::new();
    for i in 0..num_tex.min(1024) {
        let b = tex_index + i * 64;
        let name_off = mdl.off(b, mdl.i32(b)?)?;
        tex_names.push(mdl.cstr(name_off)?.replace('\\', "/").to_ascii_lowercase());
    }
    let num_cd = mdl.i32(212)?.max(0) as usize;
    let cd_index = mdl.i32(216)? as usize;
    let mut cds = Vec::new();
    for i in 0..num_cd.min(64) {
        let off = mdl.i32(cd_index + i * 4)? as usize;
        let mut cd = mdl.cstr(off)?.replace('\\', "/").to_ascii_lowercase();
        if !cd.is_empty() && !cd.ends_with('/') {
            cd.push('/');
        }
        cds.push(cd.trim_start_matches('/').to_string());
    }
    if cds.is_empty() {
        cds.push(String::new());
    }
    let materials: Vec<String> = tex_names
        .iter()
        .map(|t| {
            cds.iter()
                .map(|cd| format!("{cd}{t}"))
                .find(|m| exists(&format!("materials/{m}.vmt")))
                .unwrap_or_else(|| format!("{}{t}", cds[0]))
        })
        .collect();

    // Skins
    let num_skinref = mdl.i32(220)?.max(0) as usize;
    let num_fam = mdl.i32(224)?.max(0) as usize;
    let skin_index = mdl.i32(228)? as usize;
    let mut skins = Vec::new();
    for f in 0..num_fam.min(64) {
        let mut fam = Vec::new();
        for r in 0..num_skinref.min(1024) {
            fam.push(mdl.i16(skin_index + (f * num_skinref + r) * 2)?.max(0) as usize);
        }
        skins.push(fam);
    }

    // VVD
    let vvd = R(vvd_b);
    if vvd_b.get(0..4) != Some(b"IDSV") {
        return Err("keine VVD-Datei".into());
    }
    let lod0_count = vvd.i32(16)?.max(0) as usize;
    let num_fixups = vvd.i32(48)?.max(0) as usize;
    let fixup_start = vvd.i32(52)? as usize;
    let vert_start = vvd.i32(56)? as usize;
    let read_vert = |i: usize| -> Result<[f32; 8], String> {
        let o = vert_start + i * 48;
        Ok([
            vvd.f32(o + 16)?,
            vvd.f32(o + 20)?,
            vvd.f32(o + 24)?,
            vvd.f32(o + 28)?,
            vvd.f32(o + 32)?,
            vvd.f32(o + 36)?,
            vvd.f32(o + 40)?,
            vvd.f32(o + 44)?,
        ])
    };
    let mut verts: Vec<[f32; 8]> = Vec::with_capacity(lod0_count);
    if num_fixups == 0 {
        for i in 0..lod0_count {
            verts.push(read_vert(i)?);
        }
    } else {
        for f in 0..num_fixups {
            let o = fixup_start + f * 12;
            let lod = vvd.i32(o)?;
            let src = vvd.i32(o + 4)?.max(0) as usize;
            let n = vvd.i32(o + 8)?.max(0) as usize;
            if lod >= 0 {
                for i in 0..n {
                    verts.push(read_vert(src + i)?);
                }
            }
        }
    }

    // VTX + MDL-Bodyparts parallel durchlaufen
    let vtx = R(vtx_b);
    let vtx_version = vtx.i32(0)?;
    if vtx_version != 7 {
        return Err(format!("VTX-Version {vtx_version} nicht unterstützt"));
    }
    let sg_size = if version >= 49 { 33 } else { 25 };
    let strip_extra = if version >= 49 { 8 } else { 0 };
    let _ = strip_extra;
    let vtx_num_bp = vtx.i32(28)?.max(0) as usize;
    let vtx_bp_off = vtx.i32(32)? as usize;
    let mdl_num_bp = mdl.i32(232)?.max(0) as usize;
    let mdl_bp_index = mdl.i32(236)? as usize;

    let mut meshes = Vec::new();
    for bp in 0..mdl_num_bp.min(vtx_num_bp).min(64) {
        let mbp = mdl_bp_index + bp * 16;
        let nmodels = mdl.i32(mbp + 4)?.max(0) as usize;
        if nmodels == 0 {
            continue;
        }
        let mmodel = mdl.off(mbp, mdl.i32(mbp + 12)?)?; // Modell 0 (Standard-Bodygroup)
        let model_vert_index = (mdl.i32(mmodel + 84)?.max(0) as usize) / 48;
        let nmeshes = mdl.i32(mmodel + 72)?.max(0) as usize;
        let mesh_index = mdl.i32(mmodel + 76)?;

        let vbp = vtx_bp_off + bp * 8;
        let vmodel = vtx.off(vbp, vtx.i32(vbp + 4)?)?;
        let vlod = vtx.off(vmodel, vtx.i32(vmodel + 4)?)?; // LOD 0
        let vnmeshes = vtx.i32(vlod)?.max(0) as usize;
        let vmesh_base = vtx.off(vlod, vtx.i32(vlod + 4)?)?;

        for m in 0..nmeshes.min(vnmeshes).min(256) {
            let mmesh = mdl.off(mmodel, mesh_index)? + m * 116;
            let skinref = mdl.i32(mmesh)?.max(0) as usize;
            let mesh_vert_off = mdl.i32(mmesh + 12)?.max(0) as usize;
            let vmesh = vmesh_base + m * 9;
            let nsg = vtx.i32(vmesh)?.max(0) as usize;
            let sg_base = vtx.off(vmesh, vtx.i32(vmesh + 4)?)?;
            let mut out_verts: Vec<[f32; 8]> = Vec::new();
            let mut out_idx: Vec<u32> = Vec::new();
            for g in 0..nsg.min(64) {
                let sg = sg_base + g * sg_size;
                let nv = vtx.i32(sg)?.max(0) as usize;
                let v_off = vtx.off(sg, vtx.i32(sg + 4)?)?;
                let ni = vtx.i32(sg + 8)?.max(0) as usize;
                let i_off = vtx.off(sg, vtx.i32(sg + 12)?)?;
                let first = out_verts.len() as u32;
                for v in 0..nv {
                    let orig = vtx.u16(v_off + v * 9 + 4)? as usize;
                    let gi = model_vert_index + mesh_vert_off + orig;
                    out_verts.push(*verts.get(gi).ok_or("Vertex-Index außerhalb")?);
                }
                for i in 0..ni {
                    let idx = vtx.u16(i_off + i * 2)? as u32;
                    if idx as usize >= nv {
                        return Err("Index außerhalb der Strip-Gruppe".into());
                    }
                    out_idx.push(first + idx);
                }
            }
            out_idx.truncate(out_idx.len() / 3 * 3);
            if !out_idx.is_empty() {
                meshes.push(ModelMesh { skinref, verts: out_verts, indices: out_idx });
            }
        }
    }
    if meshes.is_empty() {
        return Err("Modell enthält keine Dreiecke".into());
    }
    let bounds = Aabb::from_points(meshes.iter().flat_map(|m| m.verts.iter().map(|v| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64))));
    Ok(Model { name: name.to_string(), meshes, materials, skins, bounds, phys_edges: Vec::new() })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn put_i32(b: &mut Vec<u8>, o: usize, v: i32) {
        if b.len() < o + 4 {
            b.resize(o + 4, 0);
        }
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Minimales Modell: ein Dreieck, eine Textur "kiste" in "models/test/"
    pub fn make_test_model() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        // ---- MDL
        let mut mdl = vec![0u8; 400];
        mdl[0..4].copy_from_slice(b"IDST");
        put_i32(&mut mdl, 4, 48);
        // Textur bei 400 (64 Bytes), Name bei 470
        put_i32(&mut mdl, 204, 1);
        put_i32(&mut mdl, 208, 400);
        mdl.resize(464, 0);
        put_i32(&mut mdl, 400, 70); // relativ → 470
        mdl.resize(470, 0);
        mdl.extend_from_slice(b"kiste\0");
        // cdtextures: Tabelle bei 480 → String bei 490
        put_i32(&mut mdl, 212, 1);
        put_i32(&mut mdl, 216, 480);
        put_i32(&mut mdl, 480, 490);
        mdl.resize(490, 0);
        mdl.extend_from_slice(b"models\\test\\\0");
        // Skins bei 510: 1 Familie, 1 skinref → 0
        put_i32(&mut mdl, 220, 1);
        put_i32(&mut mdl, 224, 1);
        put_i32(&mut mdl, 228, 510);
        mdl.resize(512, 0);
        // Bodypart bei 520, Modell bei 540, Mesh bei 700
        put_i32(&mut mdl, 232, 1);
        put_i32(&mut mdl, 236, 520);
        put_i32(&mut mdl, 524, 1); // nummodels
        put_i32(&mut mdl, 532, 20); // modelindex relativ → 540
        put_i32(&mut mdl, 540 + 72, 1); // nummeshes
        put_i32(&mut mdl, 540 + 76, 160); // meshindex relativ → 700
        put_i32(&mut mdl, 540 + 84, 0); // vertexindex
        put_i32(&mut mdl, 700, 0); // material/skinref
        put_i32(&mut mdl, 700 + 12, 0); // vertexoffset
        mdl.resize(820, 0);

        // ---- VVD: 3 Vertices
        let mut vvd = vec![0u8; 64];
        vvd[0..4].copy_from_slice(b"IDSV");
        put_i32(&mut vvd, 4, 4);
        put_i32(&mut vvd, 12, 1);
        put_i32(&mut vvd, 16, 3);
        put_i32(&mut vvd, 56, 64);
        for p in [[0.0f32, 0.0, 0.0], [32.0, 0.0, 0.0], [0.0, 32.0, 16.0]] {
            let mut v = vec![0u8; 48];
            for (k, f) in p.iter().chain([0.0f32, 0.0, 1.0, 0.5, 0.5].iter()).enumerate() {
                v[16 + k * 4..20 + k * 4].copy_from_slice(&f.to_le_bytes());
            }
            vvd.extend_from_slice(&v);
        }

        // ---- VTX
        let mut vtx = vec![0u8; 36];
        put_i32(&mut vtx, 0, 7);
        put_i32(&mut vtx, 28, 1);
        put_i32(&mut vtx, 32, 36); // Bodypart bei 36
        put_i32(&mut vtx, 36, 1);
        put_i32(&mut vtx, 40, 8); // Modell bei 44
        put_i32(&mut vtx, 44, 1);
        put_i32(&mut vtx, 48, 8); // LOD bei 52
        put_i32(&mut vtx, 52, 1);
        put_i32(&mut vtx, 56, 12); // Mesh bei 64
        put_i32(&mut vtx, 64, 1);
        put_i32(&mut vtx, 68, 9); // Strip-Gruppe bei 73
        vtx.resize(73 + 25, 0);
        put_i32(&mut vtx, 73, 3); // numVerts
        put_i32(&mut vtx, 77, 25); // vertOffset → 98
        put_i32(&mut vtx, 81, 3); // numIndices
        put_i32(&mut vtx, 85, 25 + 27); // indexOffset → 125
        vtx.resize(98, 0);
        for i in 0..3u16 {
            let mut v = vec![0u8; 9];
            v[4..6].copy_from_slice(&i.to_le_bytes());
            vtx.extend_from_slice(&v);
        }
        for i in [0u16, 1, 2] {
            vtx.extend_from_slice(&i.to_le_bytes());
        }
        (mdl, vvd, vtx)
    }

    #[test]
    fn parse_minimal_model() {
        let (mdl, vvd, vtx) = make_test_model();
        let m = parse("models/test/kiste.mdl", &mdl, &vvd, &vtx, &|p| p == "materials/models/test/kiste.vmt").unwrap();
        assert_eq!(m.meshes.len(), 1);
        assert_eq!(m.triangle_count(), 1);
        assert_eq!(m.material_for(0, 0), Some("models/test/kiste"));
        assert_eq!(m.bounds.max, DVec3::new(32.0, 32.0, 16.0));
        assert_eq!(m.meshes[0].verts[1][0], 32.0);
    }
}

#[cfg(test)]
mod writer {
    /// .phy mit einem Würfel (8 Punkte, 12 Dreiecke) im neuen VPHY-Format schreiben
    fn phy_cube(half: f32) -> Vec<u8> {
        let k = 0.0254 * half;
        let pts: Vec<[f32; 3]> = (0..8).map(|i| [if i & 1 == 0 { -k } else { k }, if i & 2 == 0 { -k } else { k }, if i & 4 == 0 { -k } else { k }]).collect();
        let tris: [[u16; 3]; 12] = [[0, 1, 3], [0, 3, 2], [4, 6, 7], [4, 7, 5], [0, 4, 5], [0, 5, 1], [2, 3, 7], [2, 7, 6], [0, 2, 6], [0, 6, 4], [1, 5, 7], [1, 7, 3]];
        let mut ledge = Vec::new();
        let ledge_size = 16 + 12 * 16;
        ledge.extend_from_slice(&(ledge_size as i32).to_le_bytes()); // Punkte direkt hinter der Ledge
        ledge.extend_from_slice(&0i32.to_le_bytes());
        ledge.extend_from_slice(&(((ledge_size / 16) as u32) << 8).to_le_bytes());
        ledge.extend_from_slice(&12i16.to_le_bytes());
        ledge.extend_from_slice(&0i16.to_le_bytes());
        for t in tris {
            ledge.extend_from_slice(&0u32.to_le_bytes());
            for v in t {
                ledge.extend_from_slice(&(v as u32).to_le_bytes());
            }
        }
        for p in &pts {
            for c in p {
                ledge.extend_from_slice(&c.to_le_bytes());
            }
            ledge.extend_from_slice(&0f32.to_le_bytes());
        }
        let mut surf = vec![0u8; 48];
        surf[32..36].copy_from_slice(&((48 + ledge.len()) as i32).to_le_bytes());
        surf.extend_from_slice(&ledge);
        let mut solid = b"VPHY".to_vec();
        solid.extend_from_slice(&0x100i16.to_le_bytes());
        solid.extend_from_slice(&0i16.to_le_bytes());
        solid.extend_from_slice(&[0u8; 20]);
        solid.extend_from_slice(&surf);
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&(solid.len() as i32).to_le_bytes());
        out.extend_from_slice(&solid);
        out.extend_from_slice(b"solid {}");
        out
    }

    #[test]
    fn phy_cube_edges() {
        let e = super::parse_phy(&phy_cube(16.0)).unwrap();
        // 12 Würfelkanten + 6 Flächendiagonalen
        assert_eq!(e.len(), 18);
        for x in &e {
            for c in x {
                assert!((c.abs() - 16.0).abs() < 0.01, "{x:?}");
            }
        }
    }

    /// Schreibt das Testmodell nach $AMBOSS_MODEL_DIR (cargo test write_test_model -- --ignored)
    #[test]
    #[ignore]
    fn write_test_model() {
        let dir = std::path::PathBuf::from(std::env::var("AMBOSS_MODEL_DIR").expect("AMBOSS_MODEL_DIR"));
        std::fs::create_dir_all(&dir).unwrap();
        let (mdl, vvd, vtx) = super::tests::make_test_model();
        std::fs::write(dir.join("kiste.mdl"), mdl).unwrap();
        std::fs::write(dir.join("kiste.vvd"), vvd).unwrap();
        std::fs::write(dir.join("kiste.dx90.vtx"), vtx).unwrap();
    }
}
