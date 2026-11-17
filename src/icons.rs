//! Icons als Linienzeichnungen (24×24-SVG-Pfade aus dem UI-Konzept), mit egui gezeichnet.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

/// Ein Element: Pfad (d), Kreis (cx, cy, r) oder Rechteck (x, y, w, h, rx)
enum El {
    Path(&'static str),
    Circle(f32, f32, f32),
    Rect(f32, f32, f32, f32, f32),
}
use El::*;

fn icon(name: &str) -> &'static [El] {
    match name {
        "select" => &[Path("M5 3l14 7-6 2-2 6z")],
        "camera" => &[Rect(3.0, 7.0, 13.0, 10.0, 2.0), Path("M16 11l5-3v8l-5-3")],
        "entity" => &[Path("M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.5c.7.7 1 1.5 1 2.5h6c0-1 .3-1.8 1-2.5A6 6 0 0 0 12 3z")],
        "block" => &[Path("M12 3l8 4.5v9L12 21l-8-4.5v-9z"), Path("M12 12l8-4.5M12 12v9M12 12L4 7.5")],
        "texture" => &[Rect(4.0, 3.0, 14.0, 6.0, 1.5), Path("M18 6h2v5h-8v3"), Rect(10.5, 14.0, 3.0, 7.0, 1.0)],
        "overlay" => &[Path("M4 4h16v10l-6 6H4z"), Path("M14 20v-6h6")],
        "decal" => &[Circle(12.0, 12.0, 8.0), Path("M8 12h8M12 8v8")],
        "clip" => &[Circle(6.0, 6.0, 3.0), Circle(6.0, 18.0, 3.0), Path("M8.5 7.5L20 18M8.5 16.5L20 6")],
        "vertex" => &[Path("M6 18L12 6l6 12z"), Circle(12.0, 6.0, 2.0), Circle(6.0, 18.0, 2.0), Circle(18.0, 18.0, 2.0)],
        "disp" => &[Path("M3 19l5-8 4 5 3-4 6 7z")],
        "magnify" => &[Circle(11.0, 11.0, 7.0), Path("M20 20l-4-4M11 8v6M8 11h6")],
        "search" => &[Circle(11.0, 11.0, 7.0), Path("M20 20l-4-4")],
        "trash" => &[Path("M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7")],
        "cordon" => &[Path("M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4"), Rect(8.0, 8.0, 8.0, 8.0, 1.0)],
        "measure" => &[Path("M3 17L17 3l4 4L7 21z"), Path("M7 13l2 2M10 10l2 2M13 7l2 2")],
        "settings" => &[Path("M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12"), Circle(16.0, 6.0, 2.0), Circle(10.0, 12.0, 2.0), Circle(18.0, 18.0, 2.0)],
        "sun" => &[Circle(12.0, 12.0, 4.0), Path("M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4")],
        "moon" => &[Path("M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z")],
        "chevron" => &[Path("M6 9l6 6 6-6")],
        "plus" => &[Path("M12 5v14M5 12h14")],
        "minus" => &[Path("M5 12h14")],
        "close" => &[Path("M6 6l12 12M18 6L6 18")],
        "play" => &[Path("M7 4l13 8-13 8z")],
        "one_view" => &[Rect(4.0, 4.0, 16.0, 16.0, 2.0)],
        "four_views" => &[Rect(4.0, 4.0, 16.0, 16.0, 2.0), Path("M12 4v16M4 12h16")],
        "maximize" => &[Path("M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5")],
        "eye" => &[Path("M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"), Circle(12.0, 12.0, 3.0)],
        "anvil" => &[Path("M3 7h12c0 2.5 2 4 6 4v1.5c-3 0-5 1-6 3H9c-1-2-3-3-6-3.5z"), Path("M9 15.5L8 20h8l-1-4.5")],
        "folder" => &[Path("M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z")],
        "people" => &[Circle(9.0, 8.0, 3.5), Path("M2.5 20a6.5 6.5 0 0 1 13 0"), Path("M16 4.5a3.5 3.5 0 0 1 0 7M18.5 20a6.5 6.5 0 0 0-3-5.5")],
        "book" => &[Path("M4 5a2 2 0 0 1 2-2h12v18H6a2 2 0 0 1-2-2z"), Path("M8 7h6")],
        "keyboard" => &[Rect(2.0, 6.0, 20.0, 12.0, 2.0), Path("M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10")],
        "restore" => &[Path("M3 12a9 9 0 1 0 3-6.7L3 8"), Path("M3 3v5h5M12 7v5l3 2")],
        "magnet" => &[Path("M6 4v7a6 6 0 0 0 12 0V4"), Path("M6 8h4M14 8h4")],
        "target" => &[Circle(12.0, 12.0, 7.0), Circle(12.0, 12.0, 2.0)],
        "lock" => &[Rect(5.0, 11.0, 14.0, 10.0, 2.0), Path("M8 11V7a4 4 0 0 1 8 0v4")],
        "layers" => &[Path("M12 3l9 5-9 5-9-5z"), Path("M3 13l9 5 9-5")],
        "tool" => &[Path("M14 7l3-3 3 3-3 3M14 7L4 17v3h3L17 10")],
        _ => &[Path("M5 5h14v14H5z")],
    }
}

/// Icon in `rect` zeichnen (Koordinaten 0–24 werden auf das Rechteck abgebildet)
pub fn draw(p: &Painter, rect: Rect, name: &str, color: Color32, width: f32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - eframe::egui::vec2(12.0 * s, 12.0 * s);
    let tf = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let stroke = Stroke::new(width * s.max(0.6), color);
    for el in icon(name) {
        match el {
            Circle(cx, cy, r) => {
                p.circle_stroke(tf(*cx, *cy), r * s, stroke);
            }
            Rect(x, y, w, h, rx) => {
                let r = eframe::egui::Rect::from_min_max(tf(*x, *y), tf(x + w, y + h));
                p.rect_stroke(r, rx * s, stroke, eframe::egui::StrokeKind::Middle);
            }
            Path(d) => {
                for (pts, closed) in parse_path(d) {
                    let pts: Vec<Pos2> = pts.iter().map(|(x, y)| tf(*x, *y)).collect();
                    if pts.len() == 2 && pts[0].distance(pts[1]) < 0.01 {
                        p.circle_filled(pts[0], stroke.width * 0.5, color);
                    } else if closed {
                        p.add(Shape::closed_line(pts, stroke));
                    } else if pts.len() >= 2 {
                        p.add(Shape::line(pts, stroke));
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ SVG-Pfade

fn tokens(d: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() {
            out.push(Tok::Cmd(c));
            i += 1;
        } else if c == '-' || c == '.' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            let mut dot = c == '.';
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !dot {
                    dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Tok::Num(d[start..i].parse().unwrap_or(0.0)));
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Tok {
    Cmd(char),
    Num(f32),
}

/// Pfad in Polylinien zerlegen: (Punkte, geschlossen)
fn parse_path(d: &str) -> Vec<(Vec<(f32, f32)>, bool)> {
    let toks = tokens(d);
    let mut out: Vec<(Vec<(f32, f32)>, bool)> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut last_ctrl: Option<(f32, f32)> = None;
    let mut cmd = 'M';
    let mut i = 0;
    let nums = |i: &mut usize, n: usize| -> Option<Vec<f32>> {
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            match toks.get(*i) {
                Some(Tok::Num(f)) => {
                    v.push(*f);
                    *i += 1;
                }
                _ => return None,
            }
        }
        Some(v)
    };
    while i < toks.len() {
        if let Tok::Cmd(c) = toks[i] {
            cmd = c;
            i += 1;
            if c == 'z' || c == 'Z' {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), true));
                }
                x = sx;
                y = sy;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (bx, by) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                let Some(v) = nums(&mut i, 2) else { break };
                if cur.len() > 1 {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.clear();
                x = bx + v[0];
                y = by + v[1];
                sx = x;
                sy = y;
                cur.push((x, y));
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let Some(v) = nums(&mut i, 2) else { break };
                x = bx + v[0];
                y = by + v[1];
                cur.push((x, y));
            }
            'H' => {
                let Some(v) = nums(&mut i, 1) else { break };
                x = bx + v[0];
                cur.push((x, y));
            }
            'V' => {
                let Some(v) = nums(&mut i, 1) else { break };
                y = by + v[0];
                cur.push((x, y));
            }
            'C' | 'S' => {
                let (c1, c2, e) = if cmd.to_ascii_uppercase() == 'C' {
                    let Some(v) = nums(&mut i, 6) else { break };
                    ((bx + v[0], by + v[1]), (bx + v[2], by + v[3]), (bx + v[4], by + v[5]))
                } else {
                    let Some(v) = nums(&mut i, 4) else { break };
                    let c1 = last_ctrl.map(|(cx, cy)| (2.0 * x - cx, 2.0 * y - cy)).unwrap_or((x, y));
                    (c1, (bx + v[0], by + v[1]), (bx + v[2], by + v[3]))
                };
                for k in 1..=12 {
                    let t = k as f32 / 12.0;
                    let u = 1.0 - t;
                    let px = u * u * u * x + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * e.0;
                    let py = u * u * u * y + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * e.1;
                    cur.push((px, py));
                }
                last_ctrl = Some(c2);
                x = e.0;
                y = e.1;
                continue;
            }
            'A' => {
                let Some(v) = nums(&mut i, 7) else { break };
                let (ex, ey) = (bx + v[5], by + v[6]);
                arc(&mut cur, (x, y), v[0], v[1], v[2], v[3] != 0.0, v[4] != 0.0, (ex, ey));
                x = ex;
                y = ey;
            }
            _ => {
                i += 1;
            }
        }
        last_ctrl = None;
    }
    if cur.len() > 1 {
        out.push((cur, false));
    }
    out
}

/// SVG-Bogen (Endpunkt-Parameter) als Polylinie anhängen
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<(f32, f32)>, p0: (f32, f32), rx: f32, ry: f32, rot: f32, large: bool, sweep: bool, p1: (f32, f32)) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-4 || ry < 1e-4 {
        out.push(p1);
        return;
    }
    let phi = rot.to_radians();
    let (cs, sn) = (phi.cos(), phi.sin());
    let dx = (p0.0 - p1.0) / 2.0;
    let dy = (p0.1 - p1.1) / 2.0;
    let x1 = cs * dx + sn * dy;
    let y1 = -sn * dx + cs * dy;
    let l = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = (num / den).max(0.0).sqrt();
    if large == sweep {
        co = -co;
    }
    let cx1 = co * rx * y1 / ry;
    let cy1 = -co * ry * x1 / rx;
    let cx = cs * cx1 - sn * cy1 + (p0.0 + p1.0) / 2.0;
    let cy = sn * cx1 + cs * cy1 + (p0.1 + p1.1) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let t1 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = ang((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && dt > 0.0 {
        dt -= std::f32::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f32::consts::TAU;
    }
    let n = ((dt.abs() / 0.25).ceil() as usize).max(2);
    for k in 1..=n {
        let t = t1 + dt * k as f32 / n as f32;
        let (x, y) = (rx * t.cos(), ry * t.sin());
        out.push((cs * x - sn * y + cx, sn * x + cs * y + cy));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_parse() {
        let p = parse_path("M12 3a6 6 0 0 0-4 10.5c.7.7 1 1.5 1 2.5h6c0-1 .3-1.8 1-2.5A6 6 0 0 0 12 3z");
        assert_eq!(p.len(), 1);
        assert!(p[0].1);
        let last = *p[0].0.last().unwrap();
        assert!((last.0 - 12.0).abs() < 0.01 && (last.1 - 3.0).abs() < 0.01, "{last:?}");
        // alle Punkte im 24er-Feld
        assert!(p[0].0.iter().all(|(x, y)| (-1.0..=25.0).contains(x) && (-1.0..=25.0).contains(y)));
        let s = parse_path("M12 2v2M12 20v2M4.9 4.9l1.4 1.4");
        assert_eq!(s.len(), 3);
        assert_eq!(s[2].0[1], (6.3, 6.3));
    }
}
