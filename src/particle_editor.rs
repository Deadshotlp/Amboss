//! Partikel-Editor: .pcf öffnen, Partikelsysteme und ihre Operatoren bearbeiten, neue Systeme aus
//! Vorlagen anlegen, Live-Vorschau (vereinfachte Simulation der gängigen Source-Operatoren) und
//! als binäres DMX speichern.

use crate::dmx::{Dmx, Element, Val};
use eframe::egui::{self, Color32, RichText, Vec2};
use glam::Vec3;
use std::path::PathBuf;

/// Bereiche eines Partikelsystems (Attributname, Anzeigename)
pub const SECTIONS: [(&str, &str); 6] = [("emitters", "Emitter"), ("initializers", "Initialisierer"), ("operators", "Operatoren"), ("renderers", "Renderer"), ("forces", "Kräfte"), ("constraints", "Einschränkungen")];

/// Bekannte Operatoren mit Standardwerten (Auswahl der gebräuchlichsten)
pub fn catalog(section: &str) -> Vec<(&'static str, Vec<(&'static str, Val)>)> {
    use Val::*;
    match section {
        "emitters" => vec![
            ("emit_continuously", vec![("emission_rate", Float(50.0)), ("emission_duration", Float(0.0)), ("emission_start_time", Float(0.0))]),
            ("emit_instantaneously", vec![("num_to_emit", Int(50)), ("emission_start_time", Float(0.0))]),
        ],
        "initializers" => vec![
            ("Position Within Sphere Random", vec![("distance_min", Float(0.0)), ("distance_max", Float(8.0)), ("speed_min", Float(20.0)), ("speed_max", Float(60.0)), ("speed_in_local_coordinate_system_min", Vec3([0.0, 0.0, 40.0])), ("speed_in_local_coordinate_system_max", Vec3([0.0, 0.0, 80.0]))]),
            ("Lifetime Random", vec![("lifetime_min", Float(1.0)), ("lifetime_max", Float(2.0))]),
            ("Radius Random", vec![("radius_min", Float(4.0)), ("radius_max", Float(8.0))]),
            ("Color Random", vec![("color1", Color([255, 255, 255, 255])), ("color2", Color([200, 200, 200, 255]))]),
            ("Alpha Random", vec![("alpha_min", Int(200)), ("alpha_max", Int(255))]),
            ("Rotation Random", vec![("rotation_initial", Float(0.0)), ("rotation_offset_min", Float(0.0)), ("rotation_offset_max", Float(360.0))]),
            ("Rotation Speed Random", vec![("rotation_rate_min", Float(-30.0)), ("rotation_rate_max", Float(30.0))]),
        ],
        "operators" => vec![
            ("Movement Basic", vec![("gravity", Vec3([0.0, 0.0, 0.0])), ("drag", Float(0.05))]),
            ("Lifespan Decay", vec![]),
            ("Alpha Fade In Random", vec![("fade_in_time_min", Float(0.1)), ("fade_in_time_max", Float(0.2)), ("proportional 0/1", Bool(true))]),
            ("Alpha Fade Out Random", vec![("fade_out_time_min", Float(0.3)), ("fade_out_time_max", Float(0.5)), ("proportional 0/1", Bool(true))]),
            ("Alpha Fade and Decay", vec![("start_alpha", Float(1.0)), ("end_alpha", Float(0.0)), ("start_fade_in_time", Float(0.0)), ("end_fade_in_time", Float(0.2)), ("start_fade_out_time", Float(0.6)), ("end_fade_out_time", Float(1.0))]),
            ("Radius Scale", vec![("radius_start_scale", Float(1.0)), ("radius_end_scale", Float(2.0)), ("start_time", Float(0.0)), ("end_time", Float(1.0))]),
            ("Color Fade", vec![("color_fade", Color([255, 120, 40, 255])), ("fade_start_time", Float(0.0)), ("fade_end_time", Float(1.0))]),
            ("Rotation Basic", vec![]),
        ],
        "renderers" => vec![("render_animated_sprites", vec![("animation_rate", Float(0.1))]), ("render_sprite_trail", vec![("min_length", Float(4.0)), ("max_length", Float(40.0)), ("length_scale", Float(0.1))])],
        "forces" => vec![("Pull towards control point", vec![("amount of force", Float(100.0)), ("falloff power", Float(2.0))])],
        "constraints" => vec![("Constrain distance to control point", vec![("minimum distance", Float(0.0)), ("maximum distance", Float(100.0))])],
        _ => vec![],
    }
}

fn op(name: &str, attrs: Vec<(&str, Val)>) -> Element {
    let mut e = Element::new("DmeParticleOperator", name);
    e.set("functionName", Val::Str(name.to_string()));
    for (k, v) in attrs {
        e.set(k, v);
    }
    e
}

fn push_ref(d: &mut Dmx, owner: usize, list: &str, child: usize) {
    let e = &mut d.elements[owner];
    match e.get_mut(list) {
        Some(Val::Array(1, v)) => v.push(Val::Element(child as i32)),
        _ => e.set(list, Val::Array(1, vec![Val::Element(child as i32)])),
    }
}

/// Neue leere PCF mit einem System aus der Vorlage
pub fn new_file(name: &str, template: usize) -> Dmx {
    let mut d = Dmx { elements: vec![Element::new("DmElement", "untitled")] };
    d.elements[0].set("particleSystemDefinitions", Val::Array(1, vec![]));
    add_system(&mut d, name, template);
    d
}

/// Vorlagen für neue Systeme
pub const TEMPLATES: [&str; 4] = ["Rauch", "Funken", "Feuer", "Staub"];

/// System aus Vorlage anlegen und in die Wurzelliste eintragen; liefert den Elementindex
pub fn add_system(d: &mut Dmx, name: &str, template: usize) -> usize {
    use Val::{Array, Color, Float, Int, Str, Vec3};
    let mut sys = Element::new("DmeParticleSystemDefinition", name);
    let (max, mat, radius, color) = match template {
        1 => (200, "effects/spark", 2.0, [255, 220, 150, 255]),
        2 => (120, "particle/fire", 12.0, [255, 160, 60, 255]),
        3 => (80, "particle/particle_smokegrenade", 20.0, [160, 140, 110, 255]),
        _ => (60, "particle/particle_smokegrenade", 16.0, [180, 180, 180, 255]),
    };
    sys.set("max_particles", Int(max));
    sys.set("initial_particles", Int(0));
    sys.set("material", Str(mat.into()));
    sys.set("radius", Float(radius));
    sys.set("color", Color(color));
    for (k, _) in SECTIONS {
        sys.set(k, Array(1, vec![]));
    }
    sys.set("children", Array(1, vec![]));
    d.elements.push(sys);
    let si = d.elements.len() - 1;
    let ops: Vec<(&str, Element)> = match template {
        1 => vec![
            ("emitters", op("emit_instantaneously", vec![("num_to_emit", Int(60))])),
            ("initializers", op("Position Within Sphere Random", vec![("distance_max", Float(2.0)), ("speed_min", Float(150.0)), ("speed_max", Float(400.0))])),
            ("initializers", op("Lifetime Random", vec![("lifetime_min", Float(0.4)), ("lifetime_max", Float(1.0))])),
            ("operators", op("Movement Basic", vec![("gravity", Vec3([0.0, 0.0, -600.0])), ("drag", Float(0.02))])),
            ("operators", op("Lifespan Decay", vec![])),
            ("operators", op("Color Fade", vec![("color_fade", Color([255, 80, 20, 255]))])),
            ("renderers", op("render_sprite_trail", vec![("min_length", Float(2.0)), ("max_length", Float(24.0)), ("length_scale", Float(0.05))])),
        ],
        2 => vec![
            ("emitters", op("emit_continuously", vec![("emission_rate", Float(60.0))])),
            ("initializers", op("Position Within Sphere Random", vec![("distance_max", Float(10.0)), ("speed_in_local_coordinate_system_min", Vec3([0.0, 0.0, 40.0])), ("speed_in_local_coordinate_system_max", Vec3([0.0, 0.0, 90.0]))])),
            ("initializers", op("Lifetime Random", vec![("lifetime_min", Float(0.5)), ("lifetime_max", Float(1.0))])),
            ("initializers", op("Radius Random", vec![("radius_min", Float(8.0)), ("radius_max", Float(14.0))])),
            ("operators", op("Movement Basic", vec![("gravity", Vec3([0.0, 0.0, 60.0])), ("drag", Float(0.1))])),
            ("operators", op("Lifespan Decay", vec![])),
            ("operators", op("Radius Scale", vec![("radius_start_scale", Float(1.0)), ("radius_end_scale", Float(0.2))])),
            ("operators", op("Color Fade", vec![("color_fade", Color([200, 40, 10, 255]))])),
            ("operators", op("Alpha Fade and Decay", vec![])),
            ("renderers", op("render_animated_sprites", vec![])),
        ],
        3 => vec![
            ("emitters", op("emit_continuously", vec![("emission_rate", Float(15.0))])),
            ("initializers", op("Position Within Sphere Random", vec![("distance_max", Float(60.0)), ("speed_min", Float(2.0)), ("speed_max", Float(10.0))])),
            ("initializers", op("Lifetime Random", vec![("lifetime_min", Float(2.0)), ("lifetime_max", Float(4.0))])),
            ("initializers", op("Alpha Random", vec![("alpha_min", Int(40)), ("alpha_max", Int(90))])),
            ("operators", op("Movement Basic", vec![("drag", Float(0.2))])),
            ("operators", op("Lifespan Decay", vec![])),
            ("operators", op("Alpha Fade In Random", vec![])),
            ("operators", op("Alpha Fade Out Random", vec![])),
            ("renderers", op("render_animated_sprites", vec![])),
        ],
        _ => vec![
            ("emitters", op("emit_continuously", vec![("emission_rate", Float(20.0))])),
            ("initializers", op("Position Within Sphere Random", vec![("distance_max", Float(6.0)), ("speed_in_local_coordinate_system_min", Vec3([-10.0, -10.0, 50.0])), ("speed_in_local_coordinate_system_max", Vec3([10.0, 10.0, 90.0]))])),
            ("initializers", op("Lifetime Random", vec![("lifetime_min", Float(2.0)), ("lifetime_max", Float(3.0))])),
            ("initializers", op("Rotation Random", vec![])),
            ("initializers", op("Alpha Random", vec![("alpha_min", Int(120)), ("alpha_max", Int(180))])),
            ("operators", op("Movement Basic", vec![("drag", Float(0.05))])),
            ("operators", op("Lifespan Decay", vec![])),
            ("operators", op("Radius Scale", vec![("radius_start_scale", Float(0.6)), ("radius_end_scale", Float(2.5))])),
            ("operators", op("Alpha Fade In Random", vec![])),
            ("operators", op("Alpha Fade Out Random", vec![])),
            ("renderers", op("render_animated_sprites", vec![])),
        ],
    };
    for (sec, e) in ops {
        d.elements.push(e);
        let i = d.elements.len() - 1;
        push_ref(d, si, sec, i);
    }
    push_ref(d, 0, "particleSystemDefinitions", si);
    si
}

/// Indizes aller Partikelsysteme (in der Reihenfolge der Wurzelliste, sonst alle Definitionen)
pub fn systems(d: &Dmx) -> Vec<usize> {
    let mut v = d.elements.first().map(|r| r.refs("particleSystemDefinitions")).unwrap_or_default();
    if v.is_empty() {
        v = (0..d.elements.len()).filter(|&i| d.elements[i].ty == "DmeParticleSystemDefinition").collect();
    }
    v.retain(|&i| i < d.elements.len());
    v
}

/// Nicht mehr erreichbare Elemente entfernen und Verweise neu nummerieren (vor dem Speichern)
pub fn compact(d: &mut Dmx) {
    let n = d.elements.len();
    if n == 0 {
        return;
    }
    let mut keep = vec![false; n];
    let mut stack = vec![0usize];
    // alle Systemdefinitionen behalten, auch wenn sie nicht in der Wurzelliste stehen
    stack.extend((0..n).filter(|&i| d.elements[i].ty == "DmeParticleSystemDefinition"));
    while let Some(i) = stack.pop() {
        if i >= n || keep[i] {
            continue;
        }
        keep[i] = true;
        for (_, v) in &d.elements[i].attrs {
            let mut visit = |x: &Val| {
                if let Val::Element(j) = x {
                    if *j >= 0 {
                        stack.push(*j as usize);
                    }
                }
            };
            match v {
                Val::Array(1, items) => items.iter().for_each(&mut visit),
                x => visit(x),
            }
        }
    }
    let mut map = vec![-1i32; n];
    let mut next = 0;
    for i in 0..n {
        if keep[i] {
            map[i] = next;
            next += 1;
        }
    }
    let old = std::mem::take(&mut d.elements);
    for (i, mut e) in old.into_iter().enumerate() {
        if !keep[i] {
            continue;
        }
        for (_, v) in &mut e.attrs {
            let fix = |x: &mut Val| {
                if let Val::Element(j) = x {
                    if *j >= 0 {
                        *j = map.get(*j as usize).copied().unwrap_or(-1);
                    }
                }
            };
            match v {
                Val::Array(1, items) => {
                    items.iter_mut().for_each(fix);
                    items.retain(|x| !matches!(x, Val::Element(-1)));
                }
                x => fix(x),
            }
        }
        d.elements.push(e);
    }
}

// ------------------------------------------------------------------ Vorschau-Simulation

#[derive(Clone, Copy)]
struct P {
    pos: Vec3,
    prev: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    radius: f32,
    radius0: f32,
    color: [f32; 3],
    color0: [f32; 3],
    alpha: f32,
    alpha0: f32,
    rot: f32,
    rot_rate: f32,
    fade_in: f32,
    fade_out: f32,
}

/// Vereinfachte Simulation eines Systems
#[derive(Default)]
pub struct Sim {
    parts: Vec<P>,
    pub time: f32,
    acc: f32,
    burst_done: bool,
    seed: u64,
}

impl Sim {
    fn rnd(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.seed >> 33) as u32) as f32 / u32::MAX as f32 * 2.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rnd().min(1.0)
    }
    pub fn reset(&mut self) {
        *self = Sim { seed: 0x1234_5678 ^ self.seed.rotate_left(7), ..Default::default() };
    }
    pub fn count(&self) -> usize {
        self.parts.len()
    }

    /// Lebende Partikel: (Position, Radius, Farbe, Deckkraft, Drehung in Grad)
    pub fn particles(&self) -> impl Iterator<Item = (Vec3, f32, [f32; 3], f32, f32)> + '_ {
        self.parts.iter().map(|p| (p.pos, p.radius, p.color, p.alpha.clamp(0.0, 1.0), p.rot))
    }

    pub fn step(&mut self, d: &Dmx, sys: usize, dt: f32) {
        let Some(s) = d.elements.get(sys) else { return };
        let ops = |sec: &str| -> Vec<&Element> { s.refs(sec).into_iter().filter_map(|i| d.elements.get(i)).collect() };
        let fname = |e: &Element| e.str("functionName").unwrap_or(&e.name).to_string();
        let max = (s.f("max_particles", 1000.0) as usize).clamp(1, 5000);
        let t0 = self.time;
        self.time += dt;
        // Emitter
        let mut spawn = 0usize;
        for e in ops("emitters") {
            let start = e.f("emission_start_time", 0.0);
            match fname(e).as_str() {
                "emit_continuously" => {
                    let dur = e.f("emission_duration", 0.0);
                    if self.time >= start && (dur <= 0.0 || t0 < start + dur) {
                        self.acc += e.f("emission_rate", 100.0) * dt;
                        let n = self.acc.floor();
                        self.acc -= n;
                        spawn += n as usize;
                    }
                }
                "emit_instantaneously" if !self.burst_done && self.time >= start => {
                    self.burst_done = true;
                    spawn += e.f("num_to_emit", 100.0) as usize;
                }
                _ => {}
            }
        }
        let base_r = s.f("radius", 5.0);
        let base_c = match s.get("color") {
            Some(Val::Color(c)) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0],
            _ => [1.0; 3],
        };
        let base_a = match s.get("color") {
            Some(Val::Color(c)) => c[3] as f32 / 255.0,
            _ => 1.0,
        };
        let inits = ops("initializers");
        for _ in 0..spawn.min(max.saturating_sub(self.parts.len())) {
            let mut p = P { pos: Vec3::ZERO, prev: Vec3::ZERO, vel: Vec3::ZERO, age: 0.0, life: f32::INFINITY, radius: base_r, radius0: base_r, color: base_c, color0: base_c, alpha: base_a, alpha0: base_a, rot: 0.0, rot_rate: 0.0, fade_in: 0.0, fade_out: 0.0 };
            for e in &inits {
                match fname(e).as_str() {
                    "Position Within Sphere Random" => {
                        let dir = loop {
                            let v = Vec3::new(self.rnd() - 1.0, self.rnd() - 1.0, self.rnd() - 1.0);
                            if v.length_squared() > 1e-4 && v.length_squared() <= 1.0 {
                                break v.normalize();
                            }
                        };
                        let dist = self.range(e.f("distance_min", 0.0), e.f("distance_max", 0.0));
                        p.pos = dir * dist;
                        p.vel = dir * self.range(e.f("speed_min", 0.0), e.f("speed_max", 0.0));
                        if let (Some(Val::Vec3(a)), Some(Val::Vec3(b))) = (e.get("speed_in_local_coordinate_system_min"), e.get("speed_in_local_coordinate_system_max")) {
                            p.vel += Vec3::new(self.range(a[0], b[0]), self.range(a[1], b[1]), self.range(a[2], b[2]));
                        }
                    }
                    "Lifetime Random" => p.life = self.range(e.f("lifetime_min", 1.0), e.f("lifetime_max", 1.0)).max(0.01),
                    "Radius Random" => {
                        p.radius = self.range(e.f("radius_min", 1.0), e.f("radius_max", 1.0));
                        p.radius0 = p.radius;
                    }
                    "Color Random" => {
                        let (c1, c2) = match (e.get("color1"), e.get("color2")) {
                            (Some(Val::Color(a)), Some(Val::Color(b))) => (*a, *b),
                            _ => ([255; 4], [255; 4]),
                        };
                        let t = self.rnd().min(1.0);
                        p.color = [0, 1, 2].map(|k| (c1[k] as f32 + (c2[k] as f32 - c1[k] as f32) * t) / 255.0);
                        p.color0 = p.color;
                    }
                    "Alpha Random" => {
                        p.alpha = self.range(e.f("alpha_min", 255.0), e.f("alpha_max", 255.0)) / 255.0;
                        p.alpha0 = p.alpha;
                    }
                    "Rotation Random" => p.rot = e.f("rotation_initial", 0.0) + self.range(e.f("rotation_offset_min", 0.0), e.f("rotation_offset_max", 360.0)),
                    "Rotation Speed Random" => p.rot_rate = self.range(e.f("rotation_rate_min", 0.0), e.f("rotation_rate_max", 0.0)),
                    _ => {}
                }
            }
            p.prev = p.pos;
            self.parts.push(p);
        }
        // Operatoren
        let opers = ops("operators");
        let mut decay = false;
        let mut seeds: Vec<(f32, f32)> = Vec::new();
        for _ in 0..self.parts.len() {
            seeds.push((self.rnd().min(1.0), self.rnd().min(1.0)));
        }
        for (p, (r1, r2)) in self.parts.iter_mut().zip(seeds) {
            p.prev = p.pos;
            p.age += dt;
            let life_t = if p.life.is_finite() { (p.age / p.life).clamp(0.0, 1.0) } else { 0.0 };
            for e in &opers {
                match fname(e).as_str() {
                    "Movement Basic" => {
                        let g = match e.get("gravity") {
                            Some(Val::Vec3(v)) => Vec3::from(*v),
                            _ => Vec3::ZERO,
                        };
                        let drag = e.f("drag", 0.0).clamp(0.0, 1.0);
                        p.vel += g * dt;
                        // Luftwiderstand als Anteil pro Sekunde
                        p.vel *= (1.0 - drag).powf(dt);
                        p.pos += p.vel * dt;
                    }
                    "Lifespan Decay" => decay = true,
                    "Alpha Fade In Random" => {
                        if p.fade_in == 0.0 {
                            p.fade_in = e.f("fade_in_time_min", 0.25) + (e.f("fade_in_time_max", 0.25) - e.f("fade_in_time_min", 0.25)) * r1;
                        }
                        let prop = !matches!(e.get("proportional 0/1"), Some(Val::Bool(false)));
                        let t = if prop { life_t } else { p.age };
                        p.alpha = p.alpha0 * (t / p.fade_in.max(1e-3)).min(1.0);
                    }
                    "Alpha Fade Out Random" => {
                        if p.fade_out == 0.0 {
                            p.fade_out = e.f("fade_out_time_min", 0.25) + (e.f("fade_out_time_max", 0.25) - e.f("fade_out_time_min", 0.25)) * r2;
                        }
                        let prop = !matches!(e.get("proportional 0/1"), Some(Val::Bool(false)));
                        let left = if prop { 1.0 - life_t } else { p.life - p.age };
                        p.alpha = p.alpha.min(p.alpha0 * (left / p.fade_out.max(1e-3)).clamp(0.0, 1.0));
                    }
                    "Alpha Fade and Decay" => {
                        let (sa, ea) = (e.f("start_alpha", 1.0), e.f("end_alpha", 0.0));
                        let (fi0, fi1, fo0, fo1) = (e.f("start_fade_in_time", 0.0), e.f("end_fade_in_time", 0.5), e.f("start_fade_out_time", 0.5), e.f("end_fade_out_time", 1.0));
                        let a = if life_t < fi1 { ((life_t - fi0) / (fi1 - fi0).max(1e-3)).clamp(0.0, 1.0) } else if life_t > fo0 { sa + (ea - sa) * ((life_t - fo0) / (fo1 - fo0).max(1e-3)).clamp(0.0, 1.0) } else { sa };
                        p.alpha = p.alpha0 * a;
                    }
                    "Radius Scale" => {
                        let (st, et) = (e.f("start_time", 0.0), e.f("end_time", 1.0));
                        let k = ((life_t - st) / (et - st).max(1e-3)).clamp(0.0, 1.0);
                        p.radius = p.radius0 * (e.f("radius_start_scale", 1.0) + (e.f("radius_end_scale", 1.0) - e.f("radius_start_scale", 1.0)) * k);
                    }
                    "Color Fade" => {
                        let c = match e.get("color_fade") {
                            Some(Val::Color(c)) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0],
                            _ => [1.0; 3],
                        };
                        let (st, et) = (e.f("fade_start_time", 0.0), e.f("fade_end_time", 1.0));
                        let k = ((life_t - st) / (et - st).max(1e-3)).clamp(0.0, 1.0);
                        p.color = [0, 1, 2].map(|i| p.color0[i] + (c[i] - p.color0[i]) * k);
                    }
                    "Rotation Basic" => p.rot += p.rot_rate * dt,
                    _ => {}
                }
            }
        }
        // ohne „Lifespan Decay“ sterben Partikel in Source nicht – hier trotzdem nach Ablauf entfernen,
        // wenn eine Lebensdauer gesetzt ist, damit die Vorschau nicht volläuft
        let _ = decay;
        self.parts.retain(|p| p.age < p.life);
    }
}

// ------------------------------------------------------------------ Fenster

#[derive(Default)]
pub struct PartEd {
    pub open: bool,
    pub dmx: Dmx,
    pub path: Option<PathBuf>,
    pub selected: usize,
    pub sim: Sim,
    pub playing: bool,
    pub dirty: bool,
    pub status: String,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    tex: std::collections::HashMap<String, Option<egui::TextureHandle>>,
    new_name: String,
}

impl PartEd {
    pub fn new_doc(&mut self) {
        self.dmx = new_file("mein_effekt", 0);
        self.path = None;
        self.selected = 0;
        self.sim.reset();
        self.playing = true;
        self.dirty = true;
        self.status.clear();
        self.open = true;
    }
}

/// Wert-Widget je Attributtyp; true bei Änderung
fn val_ui(ui: &mut egui::Ui, v: &mut Val) -> bool {
    use crate::theme::dv;
    let mut ch = false;
    match v {
        Val::Int(i) => ch |= ui.add(dv(egui::DragValue::new(i))).changed(),
        Val::Float(f) => ch |= ui.add(dv(egui::DragValue::new(f).speed(0.05))).changed(),
        Val::Time(t) => {
            let mut s = *t as f32 / 10000.0;
            if ui.add(dv(egui::DragValue::new(&mut s).speed(0.01).suffix(" s"))).changed() {
                *t = (s * 10000.0).round() as i32;
                ch = true;
            }
        }
        Val::Bool(b) => ch |= crate::theme::checkbox(ui, b, "").changed(),
        Val::Str(s) => ch |= ui.add(egui::TextEdit::singleline(s).desired_width(220.0)).changed(),
        Val::Color(c) => {
            let mut col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
            if ui.color_edit_button_srgba(&mut col).changed() {
                *c = col.to_srgba_unmultiplied();
                ch = true;
            }
        }
        Val::Vec2(a) => {
            ui.horizontal(|ui| a.iter_mut().for_each(|x| ch |= ui.add(dv(egui::DragValue::new(x).speed(0.5))).changed()));
        }
        Val::Vec3(a) | Val::QAngle(a) => {
            ui.horizontal(|ui| a.iter_mut().for_each(|x| ch |= ui.add(dv(egui::DragValue::new(x).speed(0.5))).changed()));
        }
        Val::Vec4(a) | Val::Quat(a) => {
            ui.horizontal(|ui| a.iter_mut().for_each(|x| ch |= ui.add(dv(egui::DragValue::new(x).speed(0.05))).changed()));
        }
        Val::Element(i) => {
            ui.label(RichText::new(if *i < 0 { "–".into() } else { format!("→ #{i}") }).weak());
        }
        Val::Array(_, items) => {
            ui.label(RichText::new(crate::i18n::tr_dyn(&format!("({} Einträge)", items.len()))).weak());
        }
        Val::Binary(b) => {
            ui.label(RichText::new(format!("{} B", b.len())).weak());
        }
        Val::Matrix(_) => {
            ui.label(RichText::new("4×4").weak());
        }
    };
    ch
}

impl crate::app::App {
    pub(crate) fn open_particle_file(&mut self, path: Option<PathBuf>) {
        let pe = &mut self.parted;
        pe.open = true;
        let Some(p) = path else {
            pe.new_doc();
            return;
        };
        match std::fs::read(&p).map_err(|e| e.to_string()).and_then(|b| crate::dmx::read(&b)) {
            Ok(d) => {
                pe.dmx = d;
                pe.path = Some(p);
                pe.selected = 0;
                pe.sim.reset();
                pe.playing = true;
                pe.dirty = false;
                pe.status.clear();
            }
            Err(e) => pe.status = crate::i18n::tr_dyn(&format!("Konnte nicht geöffnet werden: {e}")),
        }
    }

    /// Textur eines Partikelmaterials für die Vorschau
    fn particle_tex(&mut self, ctx: &egui::Context, mat: &str) -> Option<egui::TextureHandle> {
        let key = crate::materials::normalize(mat);
        if let Some(t) = self.parted.tex.get(&key) {
            return t.clone();
        }
        let read = |p: &str| -> Option<Vec<u8>> { self.mats.fs().and_then(|fs| fs.read(p)).or_else(|| std::fs::read(std::path::Path::new(&self.settings.game_dir).join(p)).ok()) };
        let base = read(&format!("materials/{key}.vmt"))
            .and_then(|b| crate::material_editor::MatDoc::parse(&String::from_utf8_lossy(&b)))
            .and_then(|d| d.get("$basetexture").map(|s| crate::materials::normalize(s)))
            .unwrap_or(key.clone());
        let t = read(&format!("materials/{base}.vtf")).and_then(|b| crate::vtf::load(&b, 128).ok()).map(|i| ctx.load_texture(format!("pe:{key}"), egui::ColorImage::from_rgba_unmultiplied([i.pw as usize, i.ph as usize], &i.rgba), egui::TextureOptions::LINEAR));
        self.parted.tex.insert(key, t.clone());
        t
    }

    pub(crate) fn particle_editor_window(&mut self, ctx: &egui::Context) {
        use crate::theme;
        if !self.parted.open {
            return;
        }
        let th = theme::t();
        let sys_list = systems(&self.parted.dmx);
        if self.parted.selected >= sys_list.len() {
            self.parted.selected = 0;
        }
        let cur = sys_list.get(self.parted.selected).copied();
        let mat = cur.and_then(|s| self.parted.dmx.elements[s].str("material").map(|s| s.to_string())).unwrap_or_default();
        let tex = if mat.is_empty() { None } else { self.particle_tex(ctx, &mat) };
        // Simulation weiterlaufen lassen
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        if let (Some(s), true) = (cur, self.parted.playing) {
            let pe = &mut self.parted;
            pe.sim.step(&pe.dmx, s, dt);
            ctx.request_repaint();
        }
        let mut open = true;
        let (mut save, mut save_as, mut open_file, mut manifest) = (false, false, false, false);
        let title = self.parted.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| crate::i18n::tr("neue Datei").into());
        let d = theme::Dlg::new(crate::i18n::tr("Partikel-Editor"), 1120.0).subtitle(format!("{title}{}", if self.parted.dirty { " •" } else { "" })).height(720.0).footer().no_scroll();
        let pe = &mut self.parted;
        theme::dialog(ctx, &mut open, &d, |ui, part| match part {
            theme::Part::Body => {
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 8 }).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        // Systemliste
                        ui.vertical(|ui| {
                            ui.set_width(220.0);
                            theme::section(ui, crate::i18n::tr("Partikelsysteme"));
                            egui::ScrollArea::vertical().id_salt("pe_sys").max_height(380.0).show(ui, |ui| {
                                for (k, &si) in sys_list.iter().enumerate() {
                                    let name = pe.dmx.elements[si].name.clone();
                                    if ui.selectable_label(k == pe.selected, name).clicked() {
                                        pe.selected = k;
                                        pe.sim.reset();
                                    }
                                }
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut pe.new_name).hint_text(theme::hint(crate::i18n::tr("Name"))).desired_width(110.0));
                                ui.menu_button(crate::i18n::tr("+ Neu"), |ui| {
                                    for (t, tn) in TEMPLATES.iter().enumerate() {
                                        if ui.button(crate::i18n::tr(tn)).clicked() {
                                            let n = if pe.new_name.trim().is_empty() { format!("{}_{}", tn.to_lowercase(), sys_list.len() + 1) } else { pe.new_name.trim().to_string() };
                                            add_system(&mut pe.dmx, &n, t);
                                            pe.selected = systems(&pe.dmx).len() - 1;
                                            pe.new_name.clear();
                                            pe.sim.reset();
                                            pe.dirty = true;
                                            ui.close_menu();
                                        }
                                    }
                                });
                            });
                            if let Some(s) = cur {
                                if ui.button(crate::i18n::tr("System entfernen")).clicked() {
                                    if let Some(Val::Array(1, v)) = pe.dmx.elements[0].get_mut("particleSystemDefinitions") {
                                        v.retain(|x| *x != Val::Element(s as i32));
                                    }
                                    pe.dmx.elements[s].ty = "DmElement".into();
                                    pe.selected = 0;
                                    pe.dirty = true;
                                }
                            }
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width());
                            // Vorschau
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 300.0), egui::Sense::drag());
                            if resp.dragged() {
                                let d = resp.drag_delta();
                                pe.yaw += d.x * 0.01;
                                pe.pitch = (pe.pitch + d.y * 0.01).clamp(-1.2, 1.2);
                            }
                            if resp.hovered() {
                                let s = ui.input(|i| i.smooth_scroll_delta.y);
                                pe.zoom = (pe.zoom - s * 0.002).clamp(-2.0, 2.0);
                            }
                            draw_preview(ui, r, pe, tex.as_ref());
                            ui.horizontal(|ui| {
                                if ui.button(if pe.playing { crate::i18n::tr("Pause") } else { crate::i18n::tr("Abspielen") }).clicked() {
                                    pe.playing = !pe.playing;
                                }
                                if ui.button(crate::i18n::tr("Neu starten")).clicked() {
                                    pe.sim.reset();
                                    pe.playing = true;
                                }
                                ui.label(RichText::new(crate::i18n::tr_dyn(&format!("{} Partikel · {:.1} s", pe.sim.count(), pe.sim.time))).size(12.0).color(th.text3));
                                ui.label(RichText::new(crate::i18n::tr("Ziehen dreht, Mausrad zoomt · vereinfachte Vorschau")).size(12.0).color(th.text3));
                            });
                            // Eigenschaften
                            let Some(s) = cur else {
                                ui.label(crate::i18n::tr("Kein Partikelsystem in der Datei."));
                                return;
                            };
                            egui::ScrollArea::vertical().id_salt("pe_props").auto_shrink([false, false]).show(ui, |ui| {
                                let mut changed = false;
                                theme::section(ui, crate::i18n::tr("System"));
                                egui::Grid::new("pe_sysattrs").num_columns(2).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                                    ui.label(crate::i18n::tr("Name"));
                                    changed |= ui.add(egui::TextEdit::singleline(&mut pe.dmx.elements[s].name).desired_width(220.0)).changed();
                                    ui.end_row();
                                    let e = &mut pe.dmx.elements[s];
                                    for (k, v) in e.attrs.iter_mut() {
                                        if matches!(v, Val::Array(..)) {
                                            continue;
                                        }
                                        ui.label(RichText::new(k.as_str()).font(theme::mono(12.0)));
                                        changed |= val_ui(ui, v);
                                        ui.end_row();
                                    }
                                });
                                for (sec, label) in SECTIONS {
                                    let refs = pe.dmx.elements[s].refs(sec);
                                    egui::CollapsingHeader::new(RichText::new(format!("{} ({})", crate::i18n::tr(label), refs.len())).strong()).id_salt(("pe_sec", sec)).default_open(!refs.is_empty() && sec != "constraints").show(ui, |ui| {
                                        let mut remove = None;
                                        for (k, &oi) in refs.iter().enumerate() {
                                            let fname = pe.dmx.elements[oi].str("functionName").unwrap_or(&pe.dmx.elements[oi].name).to_string();
                                            egui::Frame::new().fill(th.raised).corner_radius(6).inner_margin(6).show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(&fname).strong());
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        if ui.small_button("×").on_hover_text(crate::i18n::tr("Entfernen")).clicked() {
                                                            remove = Some(k);
                                                        }
                                                    });
                                                });
                                                egui::Grid::new(("pe_op", oi)).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                                                    for (an, v) in pe.dmx.elements[oi].attrs.iter_mut() {
                                                        if an == "functionName" {
                                                            continue;
                                                        }
                                                        ui.label(RichText::new(an.as_str()).font(theme::mono(11.5)).color(th.text2));
                                                        changed |= val_ui(ui, v);
                                                        ui.end_row();
                                                    }
                                                });
                                            });
                                            ui.add_space(3.0);
                                        }
                                        if let Some(k) = remove {
                                            if let Some(Val::Array(1, v)) = pe.dmx.elements[s].get_mut(sec) {
                                                v.remove(k);
                                            }
                                            changed = true;
                                        }
                                        ui.menu_button(crate::i18n::tr("+ Hinzufügen"), |ui| {
                                            for (name, attrs) in catalog(sec) {
                                                if ui.button(name).clicked() {
                                                    pe.dmx.elements.push(op(name, attrs));
                                                    let i = pe.dmx.elements.len() - 1;
                                                    push_ref(&mut pe.dmx, s, sec, i);
                                                    changed = true;
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                    });
                                }
                                if changed {
                                    pe.dirty = true;
                                    pe.sim.reset();
                                }
                            });
                        });
                    });
                });
            }
            theme::Part::Footer => {
                if theme::primary_button(ui, crate::i18n::tr("Speichern")).clicked() {
                    save = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Speichern unter…")).clicked() {
                    save_as = true;
                }
                if theme::secondary_button(ui, crate::i18n::tr("Öffnen…")).clicked() {
                    open_file = true;
                }
                if ui.button(crate::i18n::tr("In Map-Partikelliste eintragen")).on_hover_text(crate::i18n::tr("Schreibt maps/<map>_particles.txt, damit die Map die Datei lädt")).clicked() {
                    manifest = true;
                }
                theme::footer_hint(ui, &pe.status);
            }
            _ => {}
        });
        self.parted.open = open;
        let particles_dir = std::path::Path::new(&self.settings.game_dir).join("particles");
        if open_file {
            let mut fd = rfd::FileDialog::new().add_filter("PCF", &["pcf"]);
            if particles_dir.is_dir() {
                fd = fd.set_directory(&particles_dir);
            }
            if let Some(p) = fd.pick_file() {
                self.open_particle_file(Some(p));
            }
        }
        if save && self.parted.path.is_none() {
            save_as = true;
            save = false;
        }
        if save_as {
            let _ = std::fs::create_dir_all(&particles_dir);
            let mut fd = rfd::FileDialog::new().add_filter("PCF", &["pcf"]).set_file_name("mein_effekt.pcf");
            if particles_dir.is_dir() {
                fd = fd.set_directory(&particles_dir);
            }
            if let Some(p) = fd.save_file() {
                self.parted.path = Some(p);
                save = true;
            }
        }
        if save {
            if let Some(p) = self.parted.path.clone() {
                compact(&mut self.parted.dmx);
                match std::fs::write(&p, crate::dmx::write(&self.parted.dmx, "pcf", 1)) {
                    Ok(()) => {
                        self.parted.dirty = false;
                        self.parted.status = crate::i18n::tr_dyn(&format!("Gespeichert: {}", p.display()));
                    }
                    Err(e) => self.parted.status = e.to_string(),
                }
            }
        }
        if manifest {
            self.parted.status = match self.write_particle_manifest() {
                Ok(m) => m,
                Err(e) => e,
            };
        }
    }

    /// maps/<map>_particles.txt um die aktuelle Datei ergänzen
    fn write_particle_manifest(&mut self) -> Result<String, String> {
        let p = self.parted.path.clone().ok_or_else(|| crate::i18n::tr("Erst speichern").to_string())?;
        let game = std::path::Path::new(&self.settings.game_dir);
        let rel = p.strip_prefix(game).map(|r| r.to_string_lossy().replace('\\', "/")).map_err(|_| crate::i18n::tr("Die Datei muss im Spielordner liegen (particles/…)").to_string())?;
        let man = game.join("maps").join(format!("{}_particles.txt", self.ed.map_name()));
        let mut entries: Vec<String> = std::fs::read_to_string(&man).ok().map(|s| s.lines().filter_map(|l| l.split('"').nth(3).map(|x| x.to_string())).collect()).unwrap_or_default();
        let entry = format!("!{rel}");
        if !entries.contains(&entry) {
            entries.push(entry);
        }
        let mut txt = String::from("particles_manifest\n{\n");
        for e in &entries {
            txt.push_str(&format!("\t\"file\" \"{e}\"\n"));
        }
        txt.push_str("}\n");
        let _ = std::fs::create_dir_all(man.parent().unwrap_or(game));
        std::fs::write(&man, txt).map_err(|e| e.to_string())?;
        Ok(crate::i18n::tr_dyn(&format!("Eingetragen in {}", man.display())))
    }
}

/// Partikel perspektivisch zeichnen (Kamera kreist um den Ursprung)
fn draw_preview(ui: &egui::Ui, r: egui::Rect, pe: &PartEd, tex: Option<&egui::TextureHandle>) {
    let p = ui.painter_at(r);
    p.rect_filled(r, 6.0, Color32::from_rgb(18, 20, 26));
    let dist = 340.0 * 2f32.powf(pe.zoom);
    let (yaw, pitch) = (pe.yaw - 0.6, pe.pitch + 0.35);
    let eye = Vec3::new(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin()) * dist + Vec3::Z * 90.0;
    let target = Vec3::Z * 90.0;
    let fwd = (target - eye).normalize();
    let right = fwd.cross(Vec3::Z).normalize();
    let up = right.cross(fwd);
    let f = r.height() * 1.2;
    let proj = |w: Vec3| -> Option<(egui::Pos2, f32)> {
        let v = w - eye;
        let z = v.dot(fwd);
        (z > 1.0).then(|| (r.center() + Vec2::new(v.dot(right) / z * f, -v.dot(up) / z * f), z))
    };
    // Bodenraster
    for i in -4..=4 {
        let a = i as f32 * 32.0;
        for (s, e) in [(Vec3::new(a, -128.0, 0.0), Vec3::new(a, 128.0, 0.0)), (Vec3::new(-128.0, a, 0.0), Vec3::new(128.0, a, 0.0))] {
            if let (Some((a2, _)), Some((b2, _))) = (proj(s), proj(e)) {
                p.line_segment([a2, b2], egui::Stroke::new(1.0f32, Color32::from_gray(if i == 0 { 70 } else { 40 })));
            }
        }
    }
    let trail = pe.dmx.elements.get(systems(&pe.dmx).get(pe.selected).copied().unwrap_or(usize::MAX)).is_some_and(|s| s.refs("renderers").iter().any(|&i| pe.dmx.elements.get(i).and_then(|e| e.str("functionName")) == Some("render_sprite_trail")));
    let mut list: Vec<(f32, egui::Pos2, f32, Color32, f32, egui::Pos2)> = Vec::new();
    for q in &pe.sim.parts {
        let Some((sp, z)) = proj(q.pos) else { continue };
        let prev = proj(q.prev - (q.pos - q.prev) * 3.0).map(|x| x.0).unwrap_or(sp);
        let rad = (q.radius / z * f).clamp(0.5, 200.0);
        let c = Color32::from_rgba_unmultiplied((q.color[0] * 255.0) as u8, (q.color[1] * 255.0) as u8, (q.color[2] * 255.0) as u8, (q.alpha.clamp(0.0, 1.0) * 255.0) as u8);
        list.push((z, sp, rad, c, q.rot, prev));
    }
    list.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, sp, rad, c, rot, prev) in list {
        if trail {
            p.line_segment([prev, sp], egui::Stroke::new((rad * 0.6).max(1.0), c));
            continue;
        }
        match tex {
            Some(t) => {
                let mut m = egui::Mesh::with_texture(t.id());
                let (s, co) = rot.to_radians().sin_cos();
                let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y): (f32, f32)| sp + Vec2::new(x * co - y * s, x * s + y * co) * rad);
                let uvs = [egui::pos2(0.0, 0.0), egui::pos2(1.0, 0.0), egui::pos2(1.0, 1.0), egui::pos2(0.0, 1.0)];
                for k in 0..4 {
                    m.vertices.push(egui::epaint::Vertex { pos: corners[k], uv: uvs[k], color: c });
                }
                m.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                p.add(m);
            }
            None => {
                // weiche Scheibe: Mitte deckend, Rand durchsichtig
                let mut m = egui::Mesh::default();
                m.colored_vertex(sp, c);
                let n = 20;
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    m.colored_vertex(sp + Vec2::new(a.cos(), a.sin()) * rad, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 0));
                }
                for k in 0..n as u32 {
                    m.add_triangle(0, 1 + k, 1 + (k + 1) % n as u32);
                }
                p.add(m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_save() {
        let mut d = new_file("rauch", 0);
        add_system(&mut d, "funken", 1);
        assert_eq!(systems(&d).len(), 2);
        // Operator entfernen → nach compact verschwindet das Element, Verweise bleiben gültig
        let s = systems(&d)[0];
        let before = d.elements.len();
        if let Some(Val::Array(1, v)) = d.elements[s].get_mut("operators") {
            v.remove(0);
        }
        compact(&mut d);
        assert_eq!(d.elements.len(), before - 1);
        for e in &d.elements {
            for (_, v) in &e.attrs {
                if let Val::Array(1, items) = v {
                    for x in items {
                        if let Val::Element(i) = x {
                            assert!((*i as usize) < d.elements.len());
                        }
                    }
                }
            }
        }
        // speichern und wieder lesen, der Browser-Leser sieht beide Systeme
        let bytes = crate::dmx::write(&d, "pcf", 1);
        let back = crate::dmx::read(&bytes).unwrap();
        assert_eq!(back, d);
        let names: Vec<String> = crate::pcf::parse(&bytes).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["rauch".to_string(), "funken".to_string()]);
    }

    #[test]
    fn simulation_runs() {
        let d = new_file("funken", 1);
        let s = systems(&d)[0];
        let mut sim = Sim::default();
        sim.step(&d, s, 0.016);
        assert_eq!(sim.count(), 60, "Ausstoß auf einmal");
        let z0: f32 = sim.parts.iter().map(|p| p.pos.z).sum::<f32>();
        for _ in 0..30 {
            sim.step(&d, s, 0.016);
        }
        // Schwerkraft zieht nach unten
        let z1: f32 = sim.parts.iter().map(|p| p.pos.z).sum::<f32>();
        assert!(z1 < z0, "{z0} → {z1}");
        for _ in 0..120 {
            sim.step(&d, s, 0.016);
        }
        assert_eq!(sim.count(), 0, "Lebensdauer abgelaufen");
        // Dauerausstoß: ungefähr Rate × Zeit
        let d = new_file("rauch", 0);
        let s = systems(&d)[0];
        let mut sim = Sim::default();
        for _ in 0..60 {
            sim.step(&d, s, 1.0 / 60.0);
        }
        assert!((15..=25).contains(&sim.count()), "{}", sim.count());
    }
}
