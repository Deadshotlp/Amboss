//! Partikel-Vorschau im 3D-Fenster: `info_particle_system` spielt seinen Effekt ab (vereinfachte
//! Simulation aus dem Partikel-Editor, mit Kind-Systemen). Die .pcf-Dateien des Spiels werden
//! einmal im Hintergrund eingelesen.

use crate::dmx::{Dmx, Val};
use crate::editor::Editor;
use crate::particle_editor::Sim;
use glam::{DVec3, Vec3};
use std::collections::HashMap;
use std::sync::{mpsc, Arc};

/// Systemname (klein) → (Datei, Element-Index)
pub type Index = HashMap<String, (Arc<Dmx>, usize)>;

/// Sichtweite der Vorschau und Höchstzahl gleichzeitig laufender Effekte
const RANGE: f64 = 3000.0;
const MAX_EFFECTS: usize = 16;

struct Layer {
    dmx: Arc<Dmx>,
    sys: usize,
    delay: f32,
    sim: Sim,
    material: String,
}

struct Inst {
    name: String,
    origin: DVec3,
    time: f32,
    layers: Vec<Layer>,
}

#[derive(Default)]
pub struct Preview {
    pub on: bool,
    index: Option<Arc<Index>>,
    loading: Option<mpsc::Receiver<Index>>,
    insts: HashMap<u32, Inst>,
    /// Effekte, deren Name in keiner .pcf vorkommt (für die Statuszeile)
    pub missing: usize,
    /// Spielinhalte, aus denen der Index stammt (neu laden, wenn sie wechseln)
    fs_ptr: usize,
}

/// Alle .pcf unter particles/ einlesen
pub fn build_index(fs: &crate::gamefs::GameFs) -> Index {
    let mut idx = Index::new();
    for f in fs.list_files("particles/", &[".pcf"]) {
        let Some(d) = fs.read(&f).and_then(|b| crate::dmx::read(&b).ok()) else { continue };
        let d = Arc::new(d);
        for s in crate::particle_editor::systems(&d) {
            let name = d.elements[s].name.to_ascii_lowercase();
            idx.entry(name).or_insert_with(|| (d.clone(), s));
        }
    }
    idx
}

/// System und alle Kind-Systeme (mit Verzögerung), höchstens 12 Ebenen insgesamt
fn layers_of(idx: &Index, dmx: &Arc<Dmx>, sys: usize, delay: f32, out: &mut Vec<Layer>, depth: usize) {
    if out.len() >= 12 || depth > 4 {
        return;
    }
    let Some(e) = dmx.elements.get(sys) else { return };
    let material = e.str("material").unwrap_or("").replace('\\', "/").to_ascii_lowercase().trim_end_matches(".vmt").to_string();
    let mut sim = Sim::default();
    sim.reset();
    out.push(Layer { dmx: dmx.clone(), sys, delay, sim, material });
    for c in e.refs("children") {
        let Some(ch) = dmx.elements.get(c) else { continue };
        let d = delay + ch.f("delay", 0.0);
        match ch.get("child") {
            Some(Val::Element(i)) if *i >= 0 => layers_of(idx, dmx, *i as usize, d, out, depth + 1),
            _ => {
                // Verweis nur über den Namen (andere Datei)
                if let Some((d2, s2)) = idx.get(&ch.name.to_ascii_lowercase()) {
                    layers_of(idx, &d2.clone(), *s2, d, out, depth + 1);
                }
            }
        }
    }
}

impl Preview {
    pub fn new() -> Self {
        Preview { on: true, ..Default::default() }
    }

    /// Spielinhalte haben sich geändert → Index neu aufbauen
    pub fn invalidate(&mut self) {
        self.index = None;
        self.loading = None;
        self.insts.clear();
    }

    /// Effekte in der Nähe der Kamera weiterlaufen lassen. Liefert true, solange etwas läuft.
    pub fn update(&mut self, ed: &Editor, fs: Option<Arc<crate::gamefs::GameFs>>, cam: DVec3, dt: f32) -> bool {
        if !self.on {
            self.insts.clear();
            return false;
        }
        if let Some(f) = &fs {
            let ptr = Arc::as_ptr(f) as usize;
            if ptr != self.fs_ptr {
                self.invalidate();
                self.fs_ptr = ptr;
            }
        }
        if let Some(rx) = &self.loading {
            if let Ok(idx) = rx.try_recv() {
                self.index = Some(Arc::new(idx));
                self.loading = None;
            }
        }
        if self.index.is_none() {
            if self.loading.is_none() {
                if let Some(fs) = fs {
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(build_index(&fs));
                    });
                    self.loading = Some(rx);
                }
            }
            return self.loading.is_some();
        }
        let idx = self.index.clone().unwrap();
        // nächste Effekte in Sichtweite
        let mut near: Vec<(f64, u32, String, DVec3)> = ed
            .map
            .entities
            .iter()
            .filter(|e| e.classname().eq_ignore_ascii_case("info_particle_system") && ed.is_visible_entity(e))
            .filter_map(|e| {
                let o = e.origin()?;
                let name = e.get("effect_name")?.trim().to_ascii_lowercase();
                let d = o.distance(cam);
                (d < RANGE && !name.is_empty()).then_some((d, e.id, name, o))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_EFFECTS);
        self.missing = 0;
        let keep: std::collections::HashSet<u32> = near.iter().map(|n| n.1).collect();
        self.insts.retain(|id, _| keep.contains(id));
        let dt = dt.clamp(0.001, 0.05);
        for (_, id, name, o) in near {
            let fresh = self.insts.get(&id).is_none_or(|i| i.name != name);
            if fresh {
                let Some((d, s)) = idx.get(&name) else {
                    self.missing += 1;
                    self.insts.remove(&id);
                    continue;
                };
                let mut layers = Vec::new();
                layers_of(&idx, d, *s, 0.0, &mut layers, 0);
                self.insts.insert(id, Inst { name, origin: o, time: 0.0, layers });
            }
            let inst = self.insts.get_mut(&id).unwrap();
            inst.origin = o;
            inst.time += dt;
            for l in &mut inst.layers {
                if inst.time >= l.delay {
                    l.sim.step(&l.dmx, l.sys, dt);
                }
            }
            // einmalige Effekte (alle Partikel vergangen) nach einer Pause neu starten
            if inst.time > 1.0 && inst.layers.iter().all(|l| l.sim.count() == 0) {
                inst.time = -0.6;
                for l in &mut inst.layers {
                    l.sim.reset();
                }
            }
        }
        !self.insts.is_empty()
    }

    /// Partikel zum Zeichnen: (Material, Weltposition, Radius, Farbe RGBA, Drehung in Grad)
    pub fn for_each(&self, mut f: impl FnMut(&str, Vec3, f32, [f32; 4], f32)) {
        for inst in self.insts.values() {
            let o = inst.origin.as_vec3();
            for l in &inst.layers {
                for (p, r, c, a, rot) in l.sim.particles() {
                    f(&l.material, o + p, r, [c[0], c[1], c[2], a], rot);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn running(&self) -> usize {
        self.insts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_effect_with_child() {
        // Datei mit zwei Systemen; das zweite wird Kind des ersten
        let mut d = crate::particle_editor::new_file("feuer", 0);
        let funken = crate::particle_editor::add_system(&mut d, "funken", 1);
        let feuer = crate::particle_editor::systems(&d)[0];
        let child = d.elements.len();
        d.elements.push(crate::dmx::Element { ty: "DmeParticleChild".into(), name: "funken".into(), guid: [0; 16], attrs: vec![("child".into(), Val::Element(funken as i32)), ("delay".into(), Val::Float(0.0))] });
        d.elements[feuer].set("children", Val::Array(1, vec![Val::Element(child as i32)]));
        let d = Arc::new(d);
        let mut idx = Index::new();
        for s in crate::particle_editor::systems(&d) {
            idx.insert(d.elements[s].name.to_ascii_lowercase(), (d.clone(), s));
        }
        let mut layers = Vec::new();
        layers_of(&idx, &d, feuer, 0.0, &mut layers, 0);
        assert_eq!(layers.len(), 2, "System + Kind");

        let mut ed = Editor::new(crate::fgd::Fgd::default());
        let id = ed.map.alloc_id();
        let mut e = crate::map::Entity::new(id, "info_particle_system");
        e.set("origin", "100 0 0");
        e.set("effect_name", "Feuer");
        ed.map.entities.push(e.into());
        let mut p = Preview::new();
        p.index = Some(Arc::new(idx));
        for _ in 0..30 {
            p.update(&ed, None, DVec3::ZERO, 0.033);
        }
        assert_eq!(p.running(), 1);
        let mut n = 0;
        let mut near_origin = true;
        p.for_each(|_, pos, _, _, _| {
            n += 1;
            near_origin &= (pos.x - 100.0).abs() < 2000.0;
        });
        assert!(n > 0 && near_origin, "{n}");
        // außer Sichtweite: nichts läuft
        p.update(&ed, None, DVec3::new(10000.0, 0.0, 0.0), 0.033);
        assert_eq!(p.running(), 0);
    }

    /// Beispiel-PCF für Bildschirmtests: AMBOSS_PCF=pfad cargo test write_demo_pcf -- --ignored
    #[test]
    #[ignore]
    fn write_demo_pcf() {
        let mut d = crate::particle_editor::new_file("amboss_rauch", 0);
        crate::particle_editor::add_system(&mut d, "amboss_funken", 1);
        crate::particle_editor::add_system(&mut d, "amboss_feuer", 2);
        std::fs::write(std::env::var("AMBOSS_PCF").unwrap(), crate::dmx::write(&d, "pcf", 1)).unwrap();
    }
}
