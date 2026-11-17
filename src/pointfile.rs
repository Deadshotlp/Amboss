//! Pointfile (.lin, Leck-Pfad von VBSP) und Portalfile (.prt, Vis-Portale).

use glam::DVec3;
use std::path::Path;

pub fn parse_lin(src: &str) -> Vec<DVec3> {
    src.lines()
        .filter_map(|l| {
            let v: Vec<f64> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() >= 3).then(|| DVec3::new(v[0], v[1], v[2]))
        })
        .collect()
}

pub fn parse_prt(src: &str) -> Result<Vec<Vec<DVec3>>, String> {
    let mut lines = src.lines();
    let head = lines.next().unwrap_or("").trim();
    if head != "PRT1" && head != "PRT1-AM" {
        return Err("Keine Portaldatei (PRT1)".into());
    }
    let _leafs = lines.next();
    let _portals = lines.next();
    let mut out = Vec::new();
    for l in lines {
        let nums: Vec<f64> = l.replace(['(', ')'], " ").split_whitespace().filter_map(|x| x.parse().ok()).collect();
        // Format: Punktzahl Leaf1 Leaf2 (x y z) ...
        let Some(&count) = nums.first() else { continue };
        let count = count as usize;
        if nums.len() < 3 + count * 3 {
            continue;
        }
        let pts = (0..count).map(|i| DVec3::new(nums[3 + i * 3], nums[4 + i * 3], nums[5 + i * 3])).collect();
        out.push(pts);
    }
    Ok(out)
}

pub fn load_lin(path: &Path) -> Result<Vec<DVec3>, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pts = parse_lin(&s);
    if pts.len() < 2 {
        return Err("Pointfile enthält keinen Pfad".into());
    }
    Ok(pts)
}

pub fn load_prt(path: &Path) -> Result<Vec<Vec<DVec3>>, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_prt(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_files() {
        let lin = "0 0 0\n10 0 0\n10 20 0\n";
        assert_eq!(parse_lin(lin).len(), 3);
        let prt = "PRT1\n2\n1\n4 0 1 (0 0 0 ) (0 64 0 ) (0 64 64 ) (0 0 64 )\n";
        let p = parse_prt(prt).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0][2], DVec3::new(0.0, 64.0, 64.0));
    }
}
