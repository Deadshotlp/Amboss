//! OpenStreetMap-Import: Gebäude (Grundriss × Höhe → Brushes), Straßen und Wege (wie das
//! Spline-Werkzeug), auf Wunsch eine Bodenplatte. Quelle ist eine .osm-Datei (Export auf
//! openstreetmap.org) oder ein Ausschnitt, der über die Overpass-API geladen wird.
//! Gebäude und Straßen können auf vorhandenes Gelände (z. B. aus dem Heightmap-Import) gesetzt werden.

use crate::map::{Entity, Map, Solid};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

// ------------------------------------------------------------------ XML lesen

#[derive(Debug, Clone, Default)]
pub struct Way {
    pub nodes: Vec<i64>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Relation {
    /// (Typ, Ref, Rolle)
    pub members: Vec<(String, i64, String)>,
    pub tags: HashMap<String, String>,
}

/// Inhalt einer .osm-Datei
#[derive(Debug, Clone, Default)]
pub struct Osm {
    /// id → (Breite, Länge)
    pub nodes: HashMap<i64, (f64, f64)>,
    pub node_tags: Vec<(i64, HashMap<String, String>)>,
    pub ways: HashMap<i64, Way>,
    pub way_order: Vec<i64>,
    pub relations: Vec<Relation>,
    /// (minlat, minlon, maxlat, maxlon)
    pub bounds: Option<(f64, f64, f64, f64)>,
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let Some(end) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let ent = &after[1..end];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if ent.starts_with("#x") => out.extend(u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)),
            _ if ent.starts_with('#') => out.extend(ent[1..].parse::<u32>().ok().and_then(char::from_u32)),
            _ => out.push_str(&after[..=end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Ein XML-Tag: Name, Attribute, schließend (`</x>`), selbstschließend (`<x/>`)
fn tags(xml: &str) -> impl Iterator<Item = (String, Vec<(String, String)>, bool, bool)> + '_ {
    let b = xml.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || loop {
        let start = i + xml[i..].find('<')?;
        // Kommentare, Deklarationen, CDATA überspringen
        if xml[start..].starts_with("<!--") {
            i = start + xml[start..].find("-->").map(|e| e + 3)?;
            continue;
        }
        if xml[start..].starts_with("<?") || xml[start..].starts_with("<!") {
            i = start + xml[start..].find('>')? + 1;
            continue;
        }
        // Ende des Tags (Anführungszeichen beachten)
        let mut j = start + 1;
        let mut q = 0u8;
        while j < b.len() {
            let c = b[j];
            if q != 0 {
                if c == q {
                    q = 0;
                }
            } else if c == b'"' || c == b'\'' {
                q = c;
            } else if c == b'>' {
                break;
            }
            j += 1;
        }
        i = (j + 1).min(b.len());
        let inner = xml[start + 1..j.min(b.len())].trim();
        let closing = inner.starts_with('/');
        let self_closing = inner.ends_with('/');
        let inner = inner.trim_start_matches('/').trim_end_matches('/');
        let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
        let name = inner[..name_end].to_string();
        let mut attrs = Vec::new();
        let mut rest = &inner[name_end..];
        while let Some(eq) = rest.find('=') {
            let key = rest[..eq].trim().to_string();
            let after = rest[eq + 1..].trim_start();
            let Some(qc) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
            let Some(close) = after[1..].find(qc) else { break };
            attrs.push((key, unescape(&after[1..1 + close])));
            rest = &after[close + 2..];
        }
        return Some((name, attrs, closing, self_closing));
    })
}

/// .osm (XML) lesen
pub fn parse(xml: &str) -> Result<Osm, String> {
    if !xml.contains("<osm") {
        return Err("Keine OpenStreetMap-Datei (<osm> fehlt)".into());
    }
    let mut o = Osm::default();
    let attr = |a: &[(String, String)], k: &str| a.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    // aktuelles Element: 0 = keins, 1 = node, 2 = way, 3 = relation
    let (mut cur, mut cur_id) = (0, 0i64);
    let mut way = Way::default();
    let mut rel = Relation::default();
    let mut ntags: HashMap<String, String> = HashMap::new();
    for (name, a, closing, self_closing) in tags(xml) {
        match (name.as_str(), closing) {
            ("bounds", false) => {
                let f = |k| attr(&a, k).and_then(|v| v.parse::<f64>().ok());
                if let (Some(a1), Some(b1), Some(c1), Some(d1)) = (f("minlat"), f("minlon"), f("maxlat"), f("maxlon")) {
                    o.bounds = Some((a1, b1, c1, d1));
                }
            }
            ("node", false) => {
                let id = attr(&a, "id").and_then(|v| v.parse().ok()).unwrap_or(0);
                if let (Some(lat), Some(lon)) = (attr(&a, "lat").and_then(|v| v.parse().ok()), attr(&a, "lon").and_then(|v| v.parse().ok())) {
                    o.nodes.insert(id, (lat, lon));
                }
                if !self_closing {
                    cur = 1;
                    cur_id = id;
                    ntags.clear();
                }
            }
            ("node", true) => {
                if !ntags.is_empty() {
                    o.node_tags.push((cur_id, std::mem::take(&mut ntags)));
                }
                cur = 0;
            }
            ("way", false) => {
                cur = 2;
                cur_id = attr(&a, "id").and_then(|v| v.parse().ok()).unwrap_or(0);
                way = Way::default();
                if self_closing {
                    cur = 0;
                }
            }
            ("way", true) => {
                o.ways.insert(cur_id, std::mem::take(&mut way));
                o.way_order.push(cur_id);
                cur = 0;
            }
            ("relation", false) => {
                cur = 3;
                rel = Relation::default();
                if self_closing {
                    cur = 0;
                }
            }
            ("relation", true) => {
                o.relations.push(std::mem::take(&mut rel));
                cur = 0;
            }
            ("nd", false) if cur == 2 => {
                if let Some(r) = attr(&a, "ref").and_then(|v| v.parse().ok()) {
                    way.nodes.push(r);
                }
            }
            ("member", false) if cur == 3 => {
                if let (Some(t), Some(r)) = (attr(&a, "type"), attr(&a, "ref").and_then(|v| v.parse().ok())) {
                    rel.members.push((t, r, attr(&a, "role").unwrap_or_default()));
                }
            }
            ("tag", false) => {
                if let (Some(k), Some(v)) = (attr(&a, "k"), attr(&a, "v")) {
                    match cur {
                        1 => {
                            ntags.insert(k, v);
                        }
                        2 => {
                            way.tags.insert(k, v);
                        }
                        3 => {
                            rel.tags.insert(k, v);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if o.nodes.is_empty() {
        return Err("Die Datei enthält keine Punkte".into());
    }
    Ok(o)
}

/// Overpass-URL für einen Ausschnitt um (Breite, Länge) mit Kantenlänge in Metern
pub fn overpass_url(lat: f64, lon: f64, size_m: f64) -> String {
    let (dlat, dlon) = (size_m / 2.0 / 110_540.0, size_m / 2.0 / (111_320.0 * lat.to_radians().cos().max(0.01)));
    format!("https://overpass-api.de/api/map?bbox={:.6},{:.6},{:.6},{:.6}", lon - dlon, lat - dlat, lon + dlon, lat + dlat)
}

/// „52.52, 13.405“ oder „52.52 13.405“ → (Breite, Länge)
pub fn parse_latlon(s: &str) -> Option<(f64, f64)> {
    let v: Vec<f64> = s.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
    (v.len() == 2 && v[0].abs() <= 90.0 && v[1].abs() <= 180.0).then(|| (v[0], v[1]))
}

// ------------------------------------------------------------------ Geometrie

/// Fläche (positiv = gegen den Uhrzeigersinn)
fn area2(p: &[DVec2]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (u, v) = (p[i], p[(i + 1) % p.len()]);
        a += u.x * v.y - v.x * u.y;
    }
    a * 0.5
}

fn cross(o: DVec2, a: DVec2, b: DVec2) -> f64 {
    (a - o).perp_dot(b - o)
}

/// Polygon (gegen den Uhrzeigersinn, ohne doppelte Punkte) in konvexe Teile zerlegen:
/// Ohren abschneiden, dann benachbarte Dreiecke zusammenfassen, solange es konvex bleibt (Hertel-Mehlhorn)
pub fn convex_parts(poly: &[DVec2]) -> Vec<Vec<DVec2>> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    if is_convex(poly) {
        return vec![poly.to_vec()];
    }
    // Ohren abschneiden
    let mut idx: Vec<usize> = (0..n).collect();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut cut = false;
        for k in 0..m {
            let (a, b, c) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            if cross(poly[a], poly[b], poly[c]) <= 1e-9 {
                continue;
            }
            let inside = idx.iter().any(|&q| q != a && q != b && q != c && cross(poly[a], poly[b], poly[q]) >= -1e-9 && cross(poly[b], poly[c], poly[q]) >= -1e-9 && cross(poly[c], poly[a], poly[q]) >= -1e-9);
            if inside {
                continue;
            }
            tris.push([a, b, c]);
            idx.remove(k);
            cut = true;
            break;
        }
        if !cut {
            break; // sich selbst schneidendes Polygon
        }
    }
    if idx.len() == 3 {
        tris.push([idx[0], idx[1], idx[2]]);
    }
    // Zusammenfassen
    let mut parts: Vec<Vec<usize>> = tris.into_iter().map(|t| t.to_vec()).collect();
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..parts.len() {
            for j in i + 1..parts.len() {
                if let Some(m) = merge(&parts[i], &parts[j]) {
                    let pts: Vec<DVec2> = m.iter().map(|&k| poly[k]).collect();
                    if is_convex(&pts) {
                        parts[i] = m;
                        parts.remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
        }
    }
    parts.into_iter().map(|p| p.into_iter().map(|k| poly[k]).collect()).collect()
}

/// Zwei Polygone (Indexlisten, gleicher Umlaufsinn) an einer gemeinsamen Kante verbinden
fn merge(a: &[usize], b: &[usize]) -> Option<Vec<usize>> {
    for i in 0..a.len() {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        if let Some(j) = (0..b.len()).find(|&j| b[j] == q && b[(j + 1) % b.len()] == p) {
            // a bis p, dann b von p weiter bis vor q, dann a ab q
            let mut out = Vec::new();
            for k in 0..a.len() {
                out.push(a[(i + 1 + k) % a.len()]); // beginnt bei q … endet bei p
            }
            // out = q … p; jetzt b von p (j+1) ohne p und q
            let mut k = (j + 2) % b.len();
            while b[k] != q {
                out.push(b[k]);
                k = (k + 1) % b.len();
            }
            return Some(out);
        }
    }
    None
}

fn is_convex(p: &[DVec2]) -> bool {
    let n = p.len();
    n >= 3 && (0..n).all(|i| cross(p[i], p[(i + 1) % n], p[(i + 2) % n]) >= -1e-6)
}

/// Doppelte und auf einer Linie liegende Punkte entfernen, Umlaufsinn gegen den Uhrzeigersinn
fn clean_ring(mut p: Vec<DVec2>, min_len: f64) -> Vec<DVec2> {
    if p.len() > 1 && p[0].distance(*p.last().unwrap()) < min_len {
        p.pop();
    }
    let mut out: Vec<DVec2> = Vec::new();
    for q in p {
        if out.last().is_none_or(|l: &DVec2| l.distance(q) >= min_len) {
            out.push(q);
        }
    }
    let mut changed = true;
    while changed && out.len() > 3 {
        changed = false;
        for i in 0..out.len() {
            let n = out.len();
            let (a, b, c) = (out[(i + n - 1) % n], out[i], out[(i + 1) % n]);
            if cross(a, b, c).abs() < 1e-3 * a.distance(c).max(1.0) {
                out.remove(i);
                changed = true;
                break;
            }
        }
    }
    if area2(&out) < 0.0 {
        out.reverse();
    }
    out
}

// ------------------------------------------------------------------ Aufbau

/// Einstellungen des Imports
#[derive(Debug, Clone, PartialEq)]
pub struct OsmOpts {
    /// Einheiten je Meter (52,5 = übliches Source-Maß für Gebäude: 16 Einheiten = 1 Fuß)
    pub scale: f64,
    pub buildings: bool,
    pub roads: bool,
    pub paths: bool,
    pub ground: bool,
    /// auf vorhandenes Gelände setzen
    pub on_terrain: bool,
    pub ground_z: f64,
    pub level_m: f64,
    pub default_levels: f64,
    pub wall: String,
    pub roof: String,
    pub road: String,
    pub path: String,
    pub floor: String,
}

impl Default for OsmOpts {
    fn default() -> Self {
        OsmOpts {
            scale: 52.5,
            buildings: true,
            roads: true,
            paths: true,
            ground: true,
            on_terrain: false,
            ground_z: 0.0,
            level_m: 3.0,
            default_levels: 2.0,
            wall: "BRICK/BRICKWALL001A".into(),
            roof: "CONCRETE/CONCRETEFLOOR001A".into(),
            road: "CONCRETE/CONCRETEFLOOR001A".into(),
            path: "NATURE/DIRTFLOOR006A".into(),
            floor: "NATURE/DIRTFLOOR006A".into(),
        }
    }
}

/// Ergebnis
#[derive(Debug, Default)]
pub struct Built {
    pub world: Vec<Solid>,
    pub entities: Vec<Entity>,
    pub buildings: usize,
    pub pieces: usize,
    pub roads: usize,
    pub skipped: usize,
    /// Ausdehnung in Metern und Einheiten
    pub size_m: DVec2,
    pub size_u: DVec2,
    pub notes: Vec<String>,
}

/// Straßenbreite in Metern (None = keine Straße)
fn road_width(highway: &str, paths: bool) -> Option<(f64, bool)> {
    Some(match highway {
        "motorway" | "trunk" => (14.0, false),
        "primary" => (10.0, false),
        "secondary" => (8.0, false),
        "tertiary" => (7.0, false),
        "motorway_link" | "trunk_link" | "primary_link" | "secondary_link" | "tertiary_link" => (6.0, false),
        "residential" | "unclassified" | "living_street" | "road" => (6.0, false),
        "service" => (4.0, false),
        "pedestrian" => (5.0, true),
        "track" if paths => (3.0, true),
        "footway" | "path" | "cycleway" | "bridleway" | "steps" if paths => (2.0, true),
        _ => return None,
    })
}

/// Gebäudehöhe in Metern aus den Tags
fn building_height(t: &HashMap<String, String>, o: &OsmOpts) -> f64 {
    let num = |k: &str| t.get(k).and_then(|v| v.trim().trim_end_matches('m').trim().replace(',', ".").parse::<f64>().ok());
    num("height").or_else(|| num("building:height")).or_else(|| num("building:levels").map(|l| l * o.level_m + 1.0)).unwrap_or(o.default_levels * o.level_m + 1.0).clamp(2.0, 400.0)
}

/// Daten aufbauen. `ground(x, y)` liefert die Geländehöhe (nur bei `on_terrain`).
pub fn build(osm: &Osm, o: &OsmOpts, map_limit: f64, ground: &dyn Fn(f64, f64) -> Option<f64>) -> Built {
    let mut out = Built::default();
    let mut tmp = Map::new();
    // Bezugspunkt: Mitte der Grenzen bzw. der benutzten Punkte
    let (lat0, lon0) = match osm.bounds {
        Some((a, b, c, d)) => ((a + c) / 2.0, (b + d) / 2.0),
        None => {
            let n = osm.nodes.len().max(1) as f64;
            let s = osm.nodes.values().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
            (s.0 / n, s.1 / n)
        }
    };
    let kx = 111_320.0 * lat0.to_radians().cos();
    let ky = 110_540.0;
    let to_m = |id: &i64| osm.nodes.get(id).map(|&(lat, lon)| DVec2::new((lon - lon0) * kx, (lat - lat0) * ky));
    let s = o.scale;
    let gz = |x: f64, y: f64| if o.on_terrain { ground(x, y).unwrap_or(o.ground_z) } else { o.ground_z };
    let mut ext = crate::geometry::Aabb::EMPTY;

    // Gebäude: Wege mit building=* und äußere Ringe von Multipolygonen
    let mut rings: Vec<(Vec<DVec2>, &HashMap<String, String>)> = Vec::new();
    if o.buildings {
        for id in &osm.way_order {
            let w = &osm.ways[id];
            if w.tags.get("building").is_some_and(|v| v != "no") && w.nodes.len() >= 4 && w.nodes.first() == w.nodes.last() {
                rings.push((w.nodes.iter().filter_map(to_m).collect(), &w.tags));
            }
        }
        let mut holes = 0;
        for r in &osm.relations {
            if r.tags.get("type").map(|s| s.as_str()) == Some("multipolygon") && r.tags.get("building").is_some_and(|v| v != "no") {
                for (t, rf, role) in &r.members {
                    if t == "way" && role == "outer" {
                        if let Some(w) = osm.ways.get(rf).filter(|w| w.nodes.len() >= 4 && w.nodes.first() == w.nodes.last()) {
                            rings.push((w.nodes.iter().filter_map(to_m).collect(), &r.tags));
                        }
                    } else if role == "inner" {
                        holes += 1;
                    }
                }
            }
        }
        if holes > 0 {
            out.notes.push(format!("{holes} Innenhöfe (Löcher in Gebäuden) wurden gefüllt"));
        }
    }
    for (ring, t) in rings {
        let ring: Vec<DVec2> = ring.into_iter().map(|p| (p * s).round()).collect();
        let ring = clean_ring(ring, 2.0);
        if ring.len() < 3 || area2(&ring) < 64.0 {
            out.skipped += 1;
            continue;
        }
        let parts = convex_parts(&ring);
        if parts.is_empty() {
            out.skipped += 1;
            continue;
        }
        let base = ring.iter().map(|p| gz(p.x, p.y)).fold(f64::INFINITY, f64::min);
        let base = if base.is_finite() { base.floor() - if o.on_terrain { 8.0 } else { 0.0 } } else { o.ground_z };
        let top = (base + building_height(t, o) * s).round();
        let mut made = 0;
        for part in parts {
            let mut pts = Vec::new();
            for p in &part {
                pts.push(DVec3::new(p.x, p.y, base));
                pts.push(DVec3::new(p.x, p.y, top));
                ext.add_point(DVec3::new(p.x, p.y, base));
            }
            let Some(mut sol) = tmp.solid_from_points(&pts, &o.wall, 0.25) else { continue };
            for (k, side) in sol.sides.iter_mut().enumerate() {
                let nz = sol.planes.get(k).copied().flatten().map(|p| p.normal.z).unwrap_or(0.0);
                if nz > 0.7 {
                    side.material = o.roof.clone();
                } else if nz < -0.7 {
                    side.material = "TOOLS/TOOLSNODRAW".into();
                }
            }
            out.world.push(sol);
            made += 1;
        }
        if made > 0 {
            out.buildings += 1;
            out.pieces += made;
        } else {
            out.skipped += 1;
        }
    }

    // Straßen und Wege (wie das Spline-Werkzeug, als func_detail)
    if o.roads {
        let mut detail: Vec<Solid> = Vec::new();
        for id in &osm.way_order {
            let w = &osm.ways[id];
            let Some(hw) = w.tags.get("highway") else { continue };
            if w.tags.get("area").is_some_and(|v| v == "yes") {
                continue;
            }
            let Some((width_m, path)) = road_width(hw, o.paths) else { continue };
            let width = w.tags.get("width").and_then(|v| v.trim().trim_end_matches('m').trim().parse::<f64>().ok()).unwrap_or(width_m);
            let lift = if path { 1.0 } else { 2.0 };
            let pts: Vec<DVec3> = w
                .nodes
                .iter()
                .filter_map(to_m)
                .map(|p| {
                    let p = (p * s).round();
                    DVec3::new(p.x, p.y, gz(p.x, p.y).round() + lift)
                })
                .collect();
            if pts.len() < 2 {
                continue;
            }
            for p in &pts {
                ext.add_point(*p);
            }
            let mut so = crate::spline::SplineOpts { kind: crate::spline::Kind::Road, smooth: true, width: (width * s).round().max(16.0), thickness: 8.0, curb: false, ..Default::default() };
            so.step = (so.width * 1.5).clamp(64.0, 512.0);
            let mat = if path { &o.path } else { &o.road };
            let pl = crate::spline::plan(&pts, &so);
            for (i, (hull, _)) in pl.hulls.iter().enumerate() {
                let Some(mut sol) = tmp.solid_from_points(hull, mat, 0.25) else { continue };
                if let Some((_, dir, dist)) = pl.tex.iter().find(|t| t.0 == i) {
                    let right = crate::spline::right_of(*dir);
                    for (k, side) in sol.sides.iter_mut().enumerate() {
                        if sol.planes.get(k).copied().flatten().is_some_and(|p| p.normal.z > 0.5) {
                            side.uaxis.axis = *dir;
                            side.uaxis.scale = 0.25;
                            side.uaxis.shift = (dist - hull[0].dot(*dir)) / 0.25;
                            side.vaxis.axis = -right;
                            side.vaxis.scale = 0.25;
                            side.vaxis.shift = 0.0;
                        }
                    }
                }
                detail.push(sol);
            }
            out.roads += 1;
        }
        if !detail.is_empty() {
            let id = tmp.alloc_id();
            let mut e = Entity::new(id, "func_detail");
            e.solids = detail.into_iter().map(Into::into).collect();
            out.entities.push(e);
        }
    }

    if ext.is_empty() {
        return out;
    }
    let margin = 4.0 * s;
    let (lo, hi) = (ext.min.truncate() - DVec2::splat(margin), ext.max.truncate() + DVec2::splat(margin));
    out.size_u = hi - lo;
    out.size_m = out.size_u / s;
    // Bodenplatte unter allem (nicht bei vorhandenem Gelände)
    if o.ground && !o.on_terrain {
        let b = crate::geometry::Aabb::new(DVec3::new(lo.x.floor(), lo.y.floor(), o.ground_z - 16.0), DVec3::new(hi.x.ceil(), hi.y.ceil(), o.ground_z));
        let mut sol = tmp.make_box(b, &o.floor, 0.25);
        for (k, side) in sol.sides.iter_mut().enumerate() {
            if !sol.planes.get(k).copied().flatten().is_some_and(|p| p.normal.z > 0.7) {
                side.material = "TOOLS/TOOLSNODRAW".into();
            }
        }
        out.world.push(sol);
    }
    let lim = map_limit.max(1.0);
    if lo.x < -lim || lo.y < -lim || hi.x > lim || hi.y > lim {
        out.notes.push(format!(
            "Der Ausschnitt ist {:.0} × {:.0} m = {:.0} × {:.0} Einheiten und ragt über die Map-Grenze (±{:.0}). Kleineren Ausschnitt wählen oder den Maßstab verringern.",
            out.size_m.x, out.size_m.y, out.size_u.x, out.size_u.y, lim
        ));
    }
    out
}


// ------------------------------------------------------------------ Dialog

type Pending = std::sync::Arc<std::sync::Mutex<Option<Result<(Osm, Vec<u8>), String>>>>;

/// Zustand des Dialogs „OpenStreetMap importieren“
pub struct OsmDlg {
    pub open: bool,
    /// 0 = Datei, 1 = Herunterladen
    pub mode: usize,
    pub path: Option<std::path::PathBuf>,
    pub latlon: String,
    pub size_m: f64,
    pub opts: OsmOpts,
    pub data: Option<Osm>,
    /// Text der Datei (zum Speichern nach dem Herunterladen)
    raw: Vec<u8>,
    pub status: String,
    pending: Option<Pending>,
}

impl Default for OsmDlg {
    fn default() -> Self {
        OsmDlg { open: false, mode: 0, path: None, latlon: String::new(), size_m: 300.0, opts: OsmOpts::default(), data: None, raw: Vec::new(), status: String::new(), pending: None }
    }
}

/// Zählen, was importiert würde: (Gebäude, Straßen, Wege)
fn counts(o: &Osm) -> (usize, usize, usize) {
    let mut b = o.ways.values().filter(|w| w.tags.get("building").is_some_and(|v| v != "no") && w.nodes.first() == w.nodes.last()).count();
    b += o.relations.iter().filter(|r| r.tags.get("building").is_some()).count();
    let mut roads = 0;
    let mut paths = 0;
    for w in o.ways.values() {
        if let Some((_, p)) = w.tags.get("highway").and_then(|h| road_width(h, true)) {
            if p {
                paths += 1;
            } else {
                roads += 1;
            }
        }
    }
    (b, roads, paths)
}

impl crate::app::App {
    pub(crate) fn osm_load_file(&mut self, p: std::path::PathBuf) {
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| parse(&String::from_utf8_lossy(&b)).map(|o| (o, b))) {
            Ok((o, b)) => {
                self.osm.data = Some(o);
                self.osm.raw = b;
                self.osm.status.clear();
                self.osm.path = Some(p);
            }
            Err(e) => {
                self.osm.data = None;
                self.osm.status = format!("{}: {e}", p.display());
            }
        }
    }

    fn osm_download(&mut self, ctx: &eframe::egui::Context) {
        let Some((lat, lon)) = parse_latlon(&self.osm.latlon) else {
            self.osm.status = crate::i18n::tr("Koordinaten als „Breite, Länge“ eingeben, z. B. 52.5163, 13.3777").into();
            return;
        };
        let url = overpass_url(lat, lon, self.osm.size_m);
        let res: Pending = Default::default();
        let r2 = res.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = crate::update::http_get(&url, 90).and_then(|b| {
                let text = String::from_utf8_lossy(&b).to_string();
                parse(&text).map(|o| (o, b)).map_err(|e| if text.contains("runtime error") || text.contains("rate_limited") { "Overpass ist ausgelastet oder der Ausschnitt ist zu groß – später erneut versuchen".to_string() } else { e })
            });
            *r2.lock().unwrap() = Some(r);
            c.request_repaint();
        });
        self.osm.status = crate::i18n::tr("Lade von overpass-api.de…").into();
        self.osm.pending = Some(res);
    }

    fn osm_import(&mut self) {
        let Some(data) = self.osm.data.clone() else { return };
        let o = self.osm.opts.clone();
        let lim = self.ed.map_limit;
        let ed = &self.ed;
        let top = lim.max(4096.0);
        let ground = |x: f64, y: f64| ed.pick_3d(DVec3::new(x, y, top), -DVec3::Z).map(|h| top - h.2);
        let b = build(&data, &o, lim, &ground);
        if b.world.is_empty() && b.entities.is_empty() {
            self.osm.status = crate::i18n::tr("Nichts zu importieren – keine Gebäude oder Straßen im Ausschnitt").into();
            return;
        }
        self.win.start = false;
        self.ed.checkpoint();
        let gid = self.ed.map.alloc_id();
        let mut sel = Vec::new();
        for mut s in b.world {
            self.ed.map.reassign_ids(&mut s);
            s.editor.groupid = Some(gid);
            sel.push(crate::editor::Obj::Solid(s.id));
            self.ed.map.world.solids.push(s.into());
        }
        for mut e in b.entities {
            self.ed.map.reassign_entity_ids(&mut e);
            e.editor.groupid = Some(gid);
            e.editor.color = self.ed.fgd.resolve_color(e.classname()).unwrap_or([80, 200, 255]);
            sel.push(crate::editor::Obj::Entity(e.id));
            self.ed.map.entities.push(e.into());
        }
        self.ed.status = format!("OpenStreetMap: {} Gebäude ({} Brushes), {} Straßen und Wege, {:.0} × {:.0} m", b.buildings, b.pieces, b.roads, b.size_m.x, b.size_m.y);
        self.ed.touch();
        self.ed.dirty = true;
        let bounds = self.ed.map_bounds();
        for v in &mut self.views2d {
            v.focus(&bounds, eframe::egui::Vec2::new(700.0, 450.0));
        }
        self.view3d.focus(&bounds);
        let mut lines = vec![self.ed.status.clone()];
        if b.skipped > 0 {
            lines.push(format!("{} Gebäude übersprungen (zu klein oder ungültiger Grundriss)", b.skipped));
        }
        lines.extend(b.notes);
        lines.push("Daten © OpenStreetMap-Mitwirkende (ODbL) – bei Veröffentlichung der Map angeben.".into());
        self.decomp.report = Some(("OpenStreetMap".into(), self.osm.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| self.osm.latlon.clone()), lines));
        self.osm.open = false;
    }

    pub(crate) fn osm_window(&mut self, ctx: &eframe::egui::Context) {
        use crate::theme;
        use eframe::egui::{self, RichText, Vec2};
        if !self.osm.open {
            return;
        }
        if let Some(r) = self.osm.pending.as_ref().and_then(|p| p.lock().unwrap().take()) {
            self.osm.pending = None;
            match r {
                Ok((o, b)) => {
                    self.osm.data = Some(o);
                    self.osm.raw = b;
                    self.osm.path = None;
                    self.osm.status.clear();
                }
                Err(e) => self.osm.status = e,
            }
        } else if self.osm.pending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let th = theme::t();
        let known = self.known_materials();
        let terrain = self.ed.map.all_solids().any(|(_, s)| s.has_disp());
        let lim = self.ed.map_limit;
        let (mut open, mut pick, mut load, mut run, mut cancel, mut save) = (true, false, false, false, false, false);
        let busy = self.osm.pending.is_some();
        let d = &mut self.osm;
        let sub = d.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let dlg = theme::Dlg::new("OpenStreetMap importieren", 640.0).subtitle(sub).footer();
        theme::dialog(ctx, &mut open, &dlg, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 14)).show(ui, |ui| {
                    ui.label(RichText::new(crate::i18n::tr("Gebäude (Grundriss und Höhe aus den OSM-Daten) werden zu Brushes, Straßen und Wege zu Straßen-Brushes wie beim Spline-Werkzeug.")).color(th.text2));
                    ui.add_space(8.0);
                    d.mode = theme::segmented(ui, d.mode, &[crate::i18n::tr("Datei (.osm)"), crate::i18n::tr("Ausschnitt herunterladen")], true);
                    ui.add_space(6.0);
                    if d.mode == 0 {
                        ui.horizontal(|ui| {
                            if ui.button(crate::i18n::tr("Datei wählen…")).clicked() {
                                pick = true;
                            }
                            ui.label(RichText::new(crate::i18n::tr("Auf openstreetmap.org: Exportieren → Bereich wählen → Exportieren")).size(11.5).color(th.text3));
                        });
                    } else {
                        theme::field_row(ui, "Mitte (Breite, Länge)", |ui| ui.add(egui::TextEdit::singleline(&mut d.latlon).hint_text(theme::hint("52.5163, 13.3777")).desired_width(220.0)));
                        theme::field_row(ui, "Kantenlänge", |ui| ui.add(theme::dv(egui::DragValue::new(&mut d.size_m)).range(50.0..=2000.0).speed(10.0).suffix(" m")));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!busy, egui::Button::new(crate::i18n::tr("Laden"))).clicked() {
                                load = true;
                            }
                            if busy {
                                ui.spinner();
                            }
                            if d.data.is_some() && !d.raw.is_empty() && ui.small_button(crate::i18n::tr("Als .osm speichern…")).clicked() {
                                save = true;
                            }
                        });
                        ui.label(RichText::new(crate::i18n::tr("Über die Overpass-API (overpass-api.de). Koordinaten z. B. aus der Adresszeile von openstreetmap.org (…/#map=18/52.5163/13.3777).")).size(11.0).color(th.text3));
                    }
                    if !d.status.is_empty() {
                        ui.colored_label(th.warn, crate::i18n::tr_dyn(&d.status));
                    }
                    if let Some(o) = &d.data {
                        let (b, r, p) = counts(o);
                        ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Gefunden: {b} Gebäude, {r} Straßen, Fuß- und Radwege: {p}"))).color(th.ok));
                        if let Some((a, b2, c, e)) = o.bounds {
                            let w = (e - b2) * 111_320.0 * ((a + c) / 2.0).to_radians().cos();
                            let h = (c - a) * 110_540.0;
                            let (wu, hu) = (w * d.opts.scale, h * d.opts.scale);
                            let col = if wu > 2.0 * lim || hu > 2.0 * lim { th.err } else { th.text2 };
                            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("Ausschnitt {w:.0} × {h:.0} m = {wu:.0} × {hu:.0} Einheiten (Map darf {:.0} groß sein)", 2.0 * lim))).size(11.5).color(col));
                        }
                    }
                    ui.add_space(8.0);
                    theme::section(ui, "Umrechnung");
                    theme::field_row(ui, "Einheiten je Meter", |ui| {
                        ui.add(theme::dv(egui::DragValue::new(&mut d.opts.scale)).range(4.0..=100.0).speed(0.5)).on_hover_text(crate::i18n::tr("52,5 ist das übliche Source-Maß für Gebäude (16 Einheiten = 1 Fuß, eine 2,1 m hohe Tür ≈ 112 Einheiten). Kleiner = mehr Fläche passt in die Map."));
                        for (l, v) in [("1:1", 52.5), ("¾", 40.0), ("½", 26.0)] {
                            if ui.selectable_label((d.opts.scale - v).abs() < 0.01, l).clicked() {
                                d.opts.scale = v;
                            }
                        }
                    });
                    theme::field_row(ui, "Stockwerk", |ui| {
                        ui.add(theme::dv(egui::DragValue::new(&mut d.opts.level_m)).range(2.0..=6.0).speed(0.1).suffix(" m"));
                        ui.label(crate::i18n::tr("ohne Angabe:"));
                        ui.add(theme::dv(egui::DragValue::new(&mut d.opts.default_levels)).range(1.0..=20.0).speed(0.1));
                        ui.label(crate::i18n::tr("Stockwerke"));
                    });
                    ui.horizontal_wrapped(|ui| {
                        theme::checkbox(ui, &mut d.opts.buildings, crate::i18n::tr("Gebäude"));
                        theme::checkbox(ui, &mut d.opts.roads, crate::i18n::tr("Straßen"));
                        ui.add_enabled_ui(d.opts.roads, |ui| theme::checkbox(ui, &mut d.opts.paths, crate::i18n::tr("Fuß- und Radwege")));
                        ui.add_enabled_ui(!d.opts.on_terrain, |ui| theme::checkbox(ui, &mut d.opts.ground, crate::i18n::tr("Bodenplatte")));
                    });
                    ui.add_enabled_ui(terrain, |ui| theme::checkbox(ui, &mut d.opts.on_terrain, crate::i18n::tr("Auf vorhandenes Gelände setzen (z. B. aus dem Heightmap-Import)")))
                        .response
                        .on_disabled_hover_text(crate::i18n::tr("Die Map hat noch kein Gelände (Displacements) – erst Werkzeuge → Gelände aus Heightmap"));
                    if !d.opts.on_terrain {
                        theme::field_row(ui, "Bodenhöhe", |ui| ui.add(theme::dv(egui::DragValue::new(&mut d.opts.ground_z)).speed(8.0).suffix(" u")));
                    }
                    ui.add_space(6.0);
                    theme::section(ui, "Materialien");
                    for (l, m) in [("Wände", &mut d.opts.wall), ("Dächer", &mut d.opts.roof), ("Straßen", &mut d.opts.road), ("Wege", &mut d.opts.path), ("Boden", &mut d.opts.floor)] {
                        theme::field_row(ui, l, |ui| {
                            ui.add(egui::TextEdit::singleline(m).font(theme::mono(11.5)).desired_width(300.0));
                            if known.as_ref().is_some_and(|k| !k.contains(&m.to_ascii_lowercase()) && !k.contains(m.as_str())) {
                                ui.colored_label(th.warn, "?").on_hover_text(crate::i18n::tr("Material nicht im Spiel gefunden"));
                            }
                        });
                    }
                });
            }
            theme::Part::Footer => {
                let ok = d.data.is_some() && (d.opts.buildings || d.opts.roads);
                if ui.add_enabled(ok, egui::Button::new(RichText::new(crate::i18n::tr("Importieren")).family(theme::semibold()).color(th.accent_fg)).fill(th.accent).corner_radius(8).min_size(Vec2::new(0.0, 34.0))).clicked() {
                    run = true;
                }
                if theme::secondary_button(ui, "Abbrechen").clicked() {
                    cancel = true;
                }
                theme::footer_hint(ui, "Daten © OpenStreetMap-Mitwirkende · Ein Rückgängig-Schritt");
            }
            _ => {}
        });
        if pick {
            if let Some(p) = rfd::FileDialog::new().add_filter("OpenStreetMap", &["osm", "xml"]).pick_file() {
                self.osm_load_file(p);
            }
        }
        if load {
            self.osm_download(ctx);
        }
        if save {
            if let Some(p) = rfd::FileDialog::new().add_filter("OpenStreetMap", &["osm"]).set_file_name("ausschnitt.osm").save_file() {
                match std::fs::write(&p, &self.osm.raw) {
                    Ok(()) => self.osm.path = Some(p),
                    Err(e) => self.osm.status = e.to_string(),
                }
            }
        }
        if run {
            self.osm_import();
            return;
        }
        if cancel || !open {
            self.osm.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<osm version="0.6" generator="test">
 <bounds minlat="52.0000" minlon="13.0000" maxlat="52.0010" maxlon="13.0010"/>
 <!-- L-förmiges Haus (20 × 20 m mit Ausschnitt) -->
 <node id="1" lat="52.0004" lon="13.0004"/>
 <node id="2" lat="52.0004" lon="13.0007"/>
 <node id="3" lat="52.0005" lon="13.0007"/>
 <node id="4" lat="52.0005" lon="13.00055"/>
 <node id="5" lat="52.0006" lon="13.00055"/>
 <node id="6" lat="52.0006" lon="13.0004"/>
 <node id="10" lat="52.0002" lon="13.0001"/>
 <node id="11" lat="52.0002" lon="13.0009"/>
 <node id="12" lat="52.0008" lon="13.0005"><tag k="natural" v="tree"/></node>
 <way id="100">
  <nd ref="1"/><nd ref="2"/><nd ref="3"/><nd ref="4"/><nd ref="5"/><nd ref="6"/><nd ref="1"/>
  <tag k="building" v="house"/>
  <tag k="building:levels" v="3"/>
  <tag k="name" v="Haus &amp; Hof"/>
 </way>
 <way id="200">
  <nd ref="10"/><nd ref="11"/>
  <tag k="highway" v="residential"/>
 </way>
 <way id="201">
  <nd ref="10"/><nd ref="12"/>
  <tag k="highway" v="footway"/>
 </way>
</osm>"#;

    #[test]
    fn parse_and_build() {
        let osm = parse(SAMPLE).unwrap();
        assert_eq!(osm.nodes.len(), 9);
        assert_eq!(osm.ways.len(), 3);
        assert_eq!(osm.ways[&100].tags["name"], "Haus & Hof");
        assert_eq!(osm.node_tags.len(), 1);
        let o = OsmOpts::default();
        let b = build(&osm, &o, 16384.0, &|_, _| None);
        assert_eq!(b.buildings, 1);
        assert_eq!(b.pieces, 2, "L-Form → zwei konvexe Teile");
        // Höhe: 3 Stockwerke × 3 m + 1 m = 10 m → 525 Einheiten
        let house: Vec<&Solid> = b.world.iter().filter(|s| s.sides.iter().any(|sd| sd.material == o.wall)).collect();
        assert_eq!(house.len(), 2);
        assert!((house[0].bounds.max.z - 525.0).abs() < 1.0, "{:?}", house[0].bounds);
        assert!(house[0].sides.iter().any(|sd| sd.material == o.roof));
        // Straße + Fußweg als func_detail
        assert_eq!(b.roads, 2);
        assert_eq!(b.entities.len(), 1);
        assert!(b.entities[0].solids.iter().any(|s| s.sides.iter().any(|sd| sd.material == o.path)));
        // Bodenplatte
        assert!(b.world.iter().any(|s| s.sides.iter().any(|sd| sd.material == o.floor)));
        assert!(b.size_m.x > 50.0 && b.size_m.x < 80.0, "{:?}", b.size_m);
        assert!(b.notes.is_empty(), "{:?}", b.notes);
        // auf Gelände: Gebäude sinken 8 Einheiten ein, keine Bodenplatte
        let b2 = build(&osm, &OsmOpts { on_terrain: true, ..o.clone() }, 16384.0, &|_, _| Some(100.0));
        assert!(!b2.world.iter().any(|s| s.sides.iter().any(|sd| sd.material == o.floor)));
        assert_eq!(b2.world[0].bounds.min.z, 92.0);
        // zu groß für die Map
        let b3 = build(&osm, &OsmOpts { scale: 1000.0, ..o }, 16384.0, &|_, _| None);
        assert!(b3.notes.iter().any(|n| n.contains("Map-Grenze")));
        assert!(parse("<html/>").is_err());
    }

    #[test]
    fn convex_decomposition() {
        // U-Form
        let u: Vec<DVec2> = [(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (20.0, 30.0), (20.0, 10.0), (10.0, 10.0), (10.0, 30.0), (0.0, 30.0)].iter().map(|&(x, y)| DVec2::new(x, y)).collect();
        let parts = convex_parts(&u);
        assert!(parts.len() >= 3 && parts.len() <= 4, "{}", parts.len());
        assert!(parts.iter().all(|p| is_convex(p)));
        let total: f64 = parts.iter().map(|p| area2(p)).sum();
        assert!((total - area2(&u)).abs() < 1e-6);
        // Rechteck bleibt ein Teil
        assert_eq!(convex_parts(&u[..4].iter().copied().chain([DVec2::new(0.0, 30.0)]).collect::<Vec<_>>()).len(), 1);
        assert_eq!(parse_latlon("52.52, 13.405"), Some((52.52, 13.405)));
        assert!(overpass_url(52.52, 13.405, 400.0).starts_with("https://overpass-api.de/api/map?bbox=13.40"));
    }
}
