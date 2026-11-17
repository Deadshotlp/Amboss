//! Modell-Cache: lädt MDL-Modelle im Hintergrund.

use crate::gamefs::GameFs;
use crate::mdl::{self, Model};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

enum State {
    Loading,
    Ready(Arc<Model>),
    Failed,
}

pub struct ModelSystem {
    tx: Option<Sender<String>>,
    /// Zum Neuzeichnen, sobald ein Modell geladen ist
    pub ctx: Option<eframe::egui::Context>,
    rx: Option<Receiver<(String, Result<Model, String>)>>,
    states: HashMap<String, State>,
    /// Alle Modellpfade (für den Modell-Browser)
    pub list: Vec<String>,
    /// Neu geladene Modelle (für Bounding Boxes / GPU-Upload)
    pub fresh: Vec<Arc<Model>>,
    pub generation: u64,
    pub messages: Vec<String>,
    pending: usize,
}

impl Default for ModelSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelSystem {
    pub fn new() -> Self {
        ModelSystem { tx: None, ctx: None, rx: None, states: HashMap::new(), list: Vec::new(), fresh: Vec::new(), generation: 0, messages: Vec::new(), pending: 0 }
    }

    #[allow(dead_code)]
    pub fn set_fs(&mut self, fs: Option<Arc<GameFs>>) {
        let list = fs.as_ref().map(|f| f.list_files("models/", &[".mdl"])).unwrap_or_default();
        self.set_fs_prepared(fs, list);
    }

    /// Erweitertes Dateisystem (z. B. mit Addons): geladene Modelle bleiben, fehlgeschlagene werden erneut versucht
    pub fn update_fs(&mut self, fs: Option<Arc<GameFs>>, list: Vec<String>) {
        let requeue: Vec<String> = self.states.iter().filter_map(|(k, v)| matches!(v, State::Loading).then(|| k.clone())).collect();
        let keep: HashMap<String, State> = self.states.drain().filter(|(_, v)| matches!(v, State::Ready(_))).collect();
        let gen = self.generation;
        self.set_fs_prepared(fs, list);
        self.states = keep;
        self.generation = gen + 1;
        for p in requeue {
            self.get(&p);
        }
    }

    /// Wie `set_fs`, mit bereits ermittelter Modellliste (aus einem Hintergrund-Thread)
    pub fn set_fs_prepared(&mut self, fs: Option<Arc<GameFs>>, list: Vec<String>) {
        self.states.clear();
        self.fresh.clear();
        self.messages.clear();
        self.pending = 0;
        self.generation += 1;
        let Some(fs) = fs else {
            self.tx = None;
            self.rx = None;
            self.list.clear();
            return;
        };
        self.list = list;
        let (tx, req_rx) = channel::<String>();
        let (res_tx, rx) = channel();
        let req_rx = Arc::new(Mutex::new(req_rx));
        for _ in 0..2 {
            let fs = fs.clone();
            let req_rx = req_rx.clone();
            let res_tx = res_tx.clone();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || loop {
                let path = match req_rx.lock().unwrap().recv() {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let r = mdl::load(&fs, &path);
                if res_tx.send((path, r)).is_err() {
                    return;
                }
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            });
        }
        self.tx = Some(tx);
        self.rx = Some(rx);
    }

    /// Modell abfragen (startet ggf. das Laden)
    pub fn get(&mut self, path: &str) -> Option<Arc<Model>> {
        let key = path.replace('\\', "/").to_ascii_lowercase();
        match self.states.get(&key) {
            Some(State::Ready(m)) => Some(m.clone()),
            Some(_) => None,
            None => {
                if let Some(tx) = &self.tx {
                    if tx.send(key.clone()).is_ok() {
                        self.pending += 1;
                        self.states.insert(key, State::Loading);
                        return None;
                    }
                }
                self.states.insert(key, State::Failed);
                None
            }
        }
    }

    /// Zwischenspeicher für ein Modell verwerfen (z. B. nach dem Neukompilieren)
    pub fn forget(&mut self, path: &str) {
        self.states.remove(&path.replace('\\', "/").to_ascii_lowercase());
    }

    #[allow(dead_code)]
    pub fn peek(&self, path: &str) -> Option<Arc<Model>> {
        match self.states.get(&path.to_ascii_lowercase()) {
            Some(State::Ready(m)) => Some(m.clone()),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        self.pending > 0
    }

    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut got = false;
        while let Ok((path, r)) = rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            got = true;
            match r {
                Ok(m) => {
                    let m = Arc::new(m);
                    self.fresh.push(m.clone());
                    self.states.insert(path, State::Ready(m));
                }
                Err(e) => {
                    if self.messages.len() < 30 {
                        self.messages.push(format!("{path}: {e}"));
                    }
                    self.states.insert(path, State::Failed);
                }
            }
        }
        if got {
            self.generation += 1;
        }
        got
    }
}
