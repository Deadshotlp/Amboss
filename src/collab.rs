//! Live-Zusammenarbeit: mehrere Personen bearbeiten gleichzeitig eine Map.
//!
//! Ein Gastgeber hält die Map, Gäste verbinden sich per TCP (Zeilen mit JSON).
//! Anmeldung per Passwort (Challenge-Response, das Passwort selbst wird nie gesendet).
//! Abgleich pro Objekt (Brush/Entity über die VMF-ID), jede Person bekommt einen eigenen ID-Bereich.
//! Was andere ausgewählt haben, ist gesperrt. Fremde Änderungen werden in den eigenen
//! Rückgängig-Verlauf eingearbeitet, Strg+Z nimmt also nur eigene Schritte zurück.
//! Nach der Anmeldung ist alles verschlüsselt (siehe `netcrypt`), Schlüssel aus Passwort + Zufallswerten.
//! Der Gastgeber lauscht auf IPv4 und IPv6 – über das Internet geht es damit direkt per IPv6,
//! ohne Vermittlungsserver (nur die Firewall des Routers muss den Port durchlassen).

use crate::editor::{Editor, Obj};
use crate::map::{Entity, Map, Solid};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use crate::netcrypt::{self, Opener, Sealer};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 3918;
const PROTOCOL: u32 = 2;
/// Abstand der ID-Bereiche je Teilnehmer
const ID_RANGE: u32 = 10_000_000;

// ------------------------------------------------------------------ SHA-256 (für die Anmeldung)

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn proof(nonce: &str, cnonce: &str, pwkey: &[u8; 32]) -> String {
    hex(&netcrypt::hmac(pwkey, format!("auth|{nonce}|{cnonce}").as_bytes()))
}

fn random_nonce() -> String {
    hex(&netcrypt::random_bytes::<32>())
}

// ------------------------------------------------------------------ Protokoll

/// Eine Änderung an der Map (Objekte als VMF-Text)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Op {
    PutSolid(String),
    RemoveSolid(u32),
    PutEntity(String),
    RemoveEntity(u32),
    /// Map ohne Brushes/Entities: Eigenschaften, Gruppen, Visgroups
    Shell(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerInfo {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Msg {
    Challenge { nonce: String },
    Auth {
        protocol: u32,
        name: String,
        proof: String,
        #[serde(default)]
        cnonce: String,
    },
    Denied { reason: String },
    Welcome { you: u32, color: [u8; 3], id_base: u32, map: String, peers: Vec<PeerInfo> },
    Peers { peers: Vec<PeerInfo> },
    Ops { from: u32, ops: Vec<Op> },
    Presence { from: u32, cam: [f32; 6], solids: Vec<u32>, entities: Vec<u32> },
    Chat { from: u32, text: String },
}

/// Anzeige-Daten eines anderen Teilnehmers
#[derive(Debug, Clone, Default)]
pub struct RemoteUser {
    pub id: u32,
    pub name: String,
    pub color: [u8; 3],
    /// Kamera: Position und Blickrichtung
    pub cam: Option<([f32; 3], [f32; 3])>,
    pub solids: Vec<u32>,
    pub entities: Vec<u32>,
}

fn solid_text(m: &Map, s: &Solid) -> String {
    let mut w = Map::new();
    w.world.solids.push(s.clone().into());
    w.visgroups = m.visgroups.clone();
    w.to_vmf()
}

fn entity_text(m: &Map, e: &Entity) -> String {
    let mut w = Map::new();
    w.entities.push(e.clone().into());
    w.visgroups = m.visgroups.clone();
    w.to_vmf()
}

fn shell_text(m: &Map) -> String {
    let mut s = m.clone();
    s.world.solids.clear();
    s.entities.clear();
    s.cameras.clear();
    s.cordon = None;
    s.to_vmf()
}

/// Unterschiede `old` → `new` als Operationen
pub fn diff_ops(old: &Map, new: &Map) -> Vec<Op> {
    let mut ops = Vec::new();
    let os: HashMap<u32, &Solid> = old.world.solids.iter().map(|s| (s.id, &**s)).collect();
    let ns: HashMap<u32, &Solid> = new.world.solids.iter().map(|s| (s.id, &**s)).collect();
    for s in &new.world.solids {
        if os.get(&s.id).is_none_or(|o| **o != **s) {
            ops.push(Op::PutSolid(solid_text(new, s)));
        }
    }
    for s in &old.world.solids {
        if !ns.contains_key(&s.id) {
            ops.push(Op::RemoveSolid(s.id));
        }
    }
    let oe: HashMap<u32, &Entity> = old.entities.iter().map(|e| (e.id, &**e)).collect();
    let ne: HashMap<u32, &Entity> = new.entities.iter().map(|e| (e.id, &**e)).collect();
    for e in &new.entities {
        if oe.get(&e.id).is_none_or(|o| **o != **e) {
            ops.push(Op::PutEntity(entity_text(new, e)));
        }
    }
    for e in &old.entities {
        if !ne.contains_key(&e.id) {
            ops.push(Op::RemoveEntity(e.id));
        }
    }
    if old.world.props != new.world.props || old.visgroups != new.visgroups || old.groups != new.groups {
        ops.push(Op::Shell(shell_text(new)));
    }
    ops
}

/// Ausgelesene Operation
pub enum Parsed {
    PutSolid(Solid),
    RemoveSolid(u32),
    PutEntity(Entity),
    RemoveEntity(u32),
    Shell(Map),
}

pub fn parse_op(op: &Op) -> Option<Parsed> {
    Some(match op {
        Op::PutSolid(t) => Parsed::PutSolid(Map::from_vmf(t).ok()?.world.solids.into_iter().next()?.into_inner()),
        Op::RemoveSolid(id) => Parsed::RemoveSolid(*id),
        Op::PutEntity(t) => Parsed::PutEntity(Map::from_vmf(t).ok()?.entities.into_iter().next()?.into_inner()),
        Op::RemoveEntity(id) => Parsed::RemoveEntity(*id),
        Op::Shell(t) => Parsed::Shell(Map::from_vmf(t).ok()?),
    })
}

/// Operation auf eine vollständige Map anwenden
pub fn apply_to_map(m: &mut Map, p: &Parsed) {
    match p {
        Parsed::PutSolid(s) => match m.world.solids.iter_mut().find(|x| x.id == s.id) {
            Some(x) => *x = s.clone().into(),
            None => m.world.solids.push(s.clone().into()),
        },
        Parsed::RemoveSolid(id) => m.world.solids.retain(|x| x.id != *id),
        Parsed::PutEntity(e) => match m.entities.iter_mut().find(|x| x.id == e.id) {
            Some(x) => *x = e.clone().into(),
            None => m.entities.push(e.clone().into()),
        },
        Parsed::RemoveEntity(id) => m.entities.retain(|x| x.id != *id),
        Parsed::Shell(s) => {
            m.world.props = s.world.props.clone();
            m.visgroups = s.visgroups.clone();
            m.groups = s.groups.clone();
        }
    }
}

// ------------------------------------------------------------------ Netzwerk

enum Event {
    /// Gastgeber: neuer, angemeldeter Gast
    Joined { conn: u32, name: String, out: Sender<String> },
    /// Nachricht (Gastgeber: von Gast `conn`; Gast: vom Gastgeber, conn = 0)
    Msg { conn: u32, msg: Msg },
    Closed { conn: u32 },
    Info(String),
}

fn send_line(out: &Sender<String>, m: &Msg) {
    if let Ok(s) = serde_json::to_string(m) {
        let _ = out.send(s);
    }
}

/// Steuerzeile an den Schreib-Thread: ab hier verschlüsseln (Schlüssel liegt im `SealSlot`)
const SWITCH_TO_SEALED: &str = "\u{0}seal";
type SealSlot = Arc<Mutex<Option<Sealer>>>;

/// Schreib-Thread. Zeilen werden im Klartext gesendet, bis `SWITCH_TO_SEALED` durch die
/// Leitung kommt – danach wird jede Zeile verschlüsselt (Reihenfolge bleibt dadurch eindeutig).
fn writer_thread(mut stream: TcpStream) -> (Sender<String>, SealSlot) {
    let (tx, rx) = channel::<String>();
    let slot: SealSlot = Arc::new(Mutex::new(None));
    let slot2 = slot.clone();
    std::thread::spawn(move || {
        let mut sealer: Option<Sealer> = None;
        for line in rx {
            if line == SWITCH_TO_SEALED {
                sealer = slot2.lock().ok().and_then(|mut s| s.take());
                continue;
            }
            let data = match &mut sealer {
                Some(s) => s.seal(line.as_bytes()),
                None => line,
            };
            if stream.write_all(data.as_bytes()).and_then(|_| stream.write_all(b"\n")).and_then(|_| stream.flush()).is_err() {
                break;
            }
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    (tx, slot)
}

/// Ab jetzt verschlüsselt senden
fn start_sealing(out: &Sender<String>, slot: &SealSlot, s: Sealer) {
    if let Ok(mut g) = slot.lock() {
        *g = Some(s);
    }
    let _ = out.send(SWITCH_TO_SEALED.to_string());
}

/// Eine Zeile lesen. Mit `opener` muss sie verschlüsselt sein; `plain_denied` erlaubt
/// (nur vor der ersten verschlüsselten Zeile) eine Klartext-Ablehnung des Gastgebers.
fn read_msg(r: &mut BufReader<TcpStream>, opener: Option<&mut Opener>, plain_denied: &mut bool) -> Option<Msg> {
    let mut line = String::new();
    loop {
        line.clear();
        match r.read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => {
                let t = line.trim();
                return match opener {
                    None => serde_json::from_str(t).ok(),
                    Some(_) if *plain_denied && t.starts_with('{') => match serde_json::from_str(t).ok()? {
                        m @ Msg::Denied { .. } => Some(m),
                        _ => None,
                    },
                    Some(o) => {
                        // manipulierte, wiederholte oder falsch verschlüsselte Zeile: Verbindung beenden
                        let plain = o.open(t)?;
                        *plain_denied = false;
                        serde_json::from_slice(&plain).ok()
                    }
                };
            }
        }
    }
}

fn reader_loop(mut r: BufReader<TcpStream>, conn: u32, tx: Sender<Event>, mut opener: Opener, mut plain_denied: bool) {
    while let Some(msg) = read_msg(&mut r, Some(&mut opener), &mut plain_denied) {
        if tx.send(Event::Msg { conn, msg }).is_err() {
            return;
        }
    }
    let _ = tx.send(Event::Closed { conn });
}

/// Verständliche Fehlermeldung für einen gescheiterten Verbindungsversuch
fn connect_hint(sa: &SocketAddr, e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    let why = match e.kind() {
        TimedOut | WouldBlock => "keine Antwort – Firewall/Router des Gastgebers lässt den Port nicht durch?",
        ConnectionRefused => "abgelehnt – läuft beim Gastgeber eine Sitzung auf diesem Port?",
        // 10051 (Windows), 101 (Linux), 51 (macOS): kein Weg ins IPv6-Netz auf DIESEM Rechner
        _ if sa.is_ipv6() && matches!(e.raw_os_error(), Some(10051 | 101 | 51)) => "dein Rechner hat keine IPv6-Internetverbindung (prüfen: test-ipv6.com). Ausweg: im Router IPv6 einschalten, über einen Handy-Hotspot verbinden, oder der Gastgeber gibt Port 3918 (TCP) im Router frei und du verbindest dich mit seiner IPv4-Adresse",
        _ if sa.is_ipv6() && e.raw_os_error().is_some() => "IPv6 nicht erreichbar – hast du selbst eine IPv6-Internetverbindung?",
        _ => "",
    };
    if why.is_empty() { format!("{sa}: {e}") } else { format!("{sa}: {why} ({e})") }
}

/// Adresse zum Beitreten auflösen: "1.2.3.4", "1.2.3.4:3918", "2001:db8::1", "[2001:db8::1]:3918", "name", "name:3918"
pub fn resolve_addr(addr: &str) -> Result<Vec<SocketAddr>, String> {
    let a = addr.trim();
    if a.is_empty() {
        return Err("Bitte eine Adresse eingeben".into());
    }
    if let Ok(sa) = a.parse::<SocketAddr>() {
        return Ok(vec![sa]);
    }
    let bare = a.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, DEFAULT_PORT)]);
    }
    let with_port = if a.matches(':').count() == 1 { a.to_string() } else { format!("{a}:{DEFAULT_PORT}") };
    let list: Vec<SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(&with_port).map_err(|e| format!("{a}: {e}"))?.collect();
    if list.is_empty() {
        return Err(format!("{a}: Adresse nicht gefunden"));
    }
    Ok(list)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Host,
    Guest,
}

struct Peer {
    info: PeerInfo,
    out: Sender<String>,
}

/// Ein Objekt, das zwei Personen gleichzeitig geändert haben
#[derive(Debug, Clone)]
pub struct Conflict {
    /// (0 = Brush, 1 = Entity, 2 = Map-Eigenschaften; ID)
    pub key: (u8, u32),
    pub from_name: String,
    pub from_color: [u8; 3],
    /// z. B. "func_door · hangar_tor_01"
    pub label: String,
    /// geänderte Werte: (Schlüssel, meins, deins)
    pub rows: Vec<(String, String, String)>,
    pub more: usize,
    pub remote: Op,
    pub created: Instant,
}

/// Wie lange ein Konflikt angeboten wird, bevor die eigene Version endgültig bleibt
pub const CONFLICT_SECS: f32 = 12.0;

fn op_key(o: &Op) -> Option<(u8, u32)> {
    match parse_op(o)? {
        Parsed::PutSolid(s) => Some((0, s.id)),
        Parsed::RemoveSolid(id) => Some((0, id)),
        Parsed::PutEntity(e) => Some((1, e.id)),
        Parsed::RemoveEntity(id) => Some((1, id)),
        Parsed::Shell(_) => Some((2, 0)),
    }
}

fn make_conflict(ed: &Editor, o: &Op, from_name: String, from_color: [u8; 3]) -> Option<Conflict> {
    let key = op_key(o)?;
    let p = parse_op(o)?;
    let mut rows: Vec<(String, String, String)> = Vec::new();
    let label;
    match &p {
        Parsed::PutEntity(theirs) => {
            let mine = ed.map.entity(theirs.id);
            let class = theirs.classname().to_string();
            label = match theirs.get("targetname").filter(|n| !n.is_empty()) {
                Some(n) => format!("{class} · {n}"),
                None => format!("{class} · #{}", theirs.id),
            };
            if let Some(m) = mine {
                let mut keys: Vec<&str> = m.props.iter().map(|(k, _)| k.as_str()).chain(theirs.props.iter().map(|(k, _)| k.as_str())).collect();
                keys.sort();
                keys.dedup();
                for k in keys {
                    let (a, b) = (m.get(k).unwrap_or("–"), theirs.get(k).unwrap_or("–"));
                    if a != b {
                        rows.push((k.to_string(), a.to_string(), b.to_string()));
                    }
                }
                if m.solids != theirs.solids {
                    rows.push(("Brushes".into(), format!("{}", m.solids.len()), format!("{} (geändert)", theirs.solids.len())));
                }
            } else {
                rows.push(("Objekt".into(), "gelöscht".into(), "geändert".into()));
            }
        }
        Parsed::PutSolid(theirs) => {
            label = format!("Brush · #{}", theirs.id);
            match ed.map.solid(theirs.id) {
                Some(m) => {
                    let f = |v: glam::DVec3| crate::map::fmt_vec(v);
                    if m.bounds.center() != theirs.bounds.center() {
                        rows.push(("Position".into(), f(m.bounds.center()), f(theirs.bounds.center())));
                    }
                    if m.bounds.size() != theirs.bounds.size() {
                        rows.push(("Größe".into(), f(m.bounds.size()), f(theirs.bounds.size())));
                    }
                    let ma = m.sides.first().map(|s| s.material.clone()).unwrap_or_default();
                    let ta = theirs.sides.first().map(|s| s.material.clone()).unwrap_or_default();
                    if ma != ta {
                        rows.push(("Material".into(), ma, ta));
                    }
                    if rows.is_empty() {
                        rows.push(("Form".into(), "meine".into(), "geändert".into()));
                    }
                }
                None => rows.push(("Objekt".into(), "gelöscht".into(), "geändert".into())),
            }
        }
        Parsed::RemoveSolid(id) => {
            label = format!("Brush · #{id}");
            rows.push(("Objekt".into(), "geändert".into(), "gelöscht".into()));
        }
        Parsed::RemoveEntity(id) => {
            label = ed.map.entity(*id).map(|e| format!("{} · #{id}", e.classname())).unwrap_or_else(|| format!("Entity · #{id}"));
            rows.push(("Objekt".into(), "geändert".into(), "gelöscht".into()));
        }
        Parsed::Shell(_) => {
            label = "Map-Eigenschaften".into();
            rows.push(("worldspawn".into(), "meine".into(), "geändert".into()));
        }
    }
    let more = rows.len().saturating_sub(4);
    rows.truncate(4);
    Some(Conflict { key, from_name, from_color, label, rows, more, remote: o.clone(), created: Instant::now() })
}

/// Meldungen an die Oberfläche
pub enum Notice {
    /// Neuer Konflikt (Toast anzeigen)
    Conflict,
    /// Gast: Map des Gastgebers wurde übernommen
    Joined,
    Text(String),
    Denied(String),
    Disconnected,
}

pub struct Collab {
    pub role: Role,
    pub me: PeerInfo,
    pub users: Vec<RemoteUser>,
    pub chat: Vec<(String, [u8; 3], String)>,
    pub status: String,
    rx: Receiver<Event>,
    /// Gastgeber: verbundene Gäste
    peers: Vec<Peer>,
    /// Gast: Leitung zum Gastgeber
    host_out: Option<Sender<String>>,
    /// Stand, der zuletzt mit allen abgeglichen wurde
    synced: Option<Map>,
    synced_rev: u64,
    last_send: Instant,
    last_presence: Instant,
    last_presence_data: Option<([f32; 6], Vec<u32>, Vec<u32>)>,
    pub port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub connected: bool,
    /// offene Konflikte (neueste zuletzt)
    pub conflicts: Vec<Conflict>,
}

const COLORS: [[u8; 3]; 8] = [[255, 120, 60], [80, 200, 255], [130, 230, 90], [240, 90, 200], [255, 220, 70], [160, 120, 255], [60, 230, 200], [255, 90, 90]];

impl Collab {
    /// Als Gastgeber starten (lauscht auf allen Netzwerkkarten)
    pub fn host(name: &str, port: u16, password: &str, ed: &Editor) -> Result<Collab, String> {
        if password.trim().len() < 4 {
            return Err("Bitte ein Passwort mit mindestens 4 Zeichen festlegen".into());
        }
        // IPv6 und IPv4. Unter Linux nimmt [::] meist auch IPv4 an, dann schlägt 0.0.0.0 fehl – das ist in Ordnung.
        let mut listeners = Vec::new();
        let mut err = String::new();
        match TcpListener::bind(("::", port)) {
            Ok(l) => listeners.push(l),
            Err(e) => err = e.to_string(),
        }
        let port = listeners.first().and_then(|l| l.local_addr().ok()).map(|a| a.port()).unwrap_or(port);
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => listeners.push(l),
            Err(e) if listeners.is_empty() => return Err(format!("Port {port} nicht verfügbar: {e} {err}")),
            Err(_) => {}
        }
        let port = listeners[0].local_addr().map(|a| a.port()).unwrap_or(port);
        let pwkey = netcrypt::stretch(password);
        let (tx, rx) = channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        for l in &listeners {
            l.set_nonblocking(true).map_err(|e| e.to_string())?;
        }
        std::thread::spawn(move || {
            let mut conn_id = 0u32;
            while !stop2.load(std::sync::atomic::Ordering::SeqCst) {
                let mut idle = true;
                for listener in &listeners {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        idle = false;
                        conn_id += 1;
                        let (tx, conn) = (tx.clone(), conn_id);
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            let _ = stream.set_nodelay(true);
                            let Ok(ws) = stream.try_clone() else { return };
                            let (out, slot) = writer_thread(ws);
                            let nonce = random_nonce();
                            send_line(&out, &Msg::Challenge { nonce: nonce.clone() });
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
                            let mut r = BufReader::new(stream);
                            match read_msg(&mut r, None, &mut false) {
                                Some(Msg::Auth { protocol, name, proof: p, cnonce }) if protocol == PROTOCOL && cnonce.len() >= 32 && cnonce != nonce && p == proof(&nonce, &cnonce, &pwkey) => {
                                    let _ = r.get_ref().set_read_timeout(None);
                                    let (h2g, g2h) = netcrypt::session_keys(&pwkey, &nonce, &cnonce);
                                    start_sealing(&out, &slot, Sealer::new(h2g));
                                    let _ = tx.send(Event::Joined { conn, name, out });
                                    reader_loop(r, conn, tx, Opener::new(g2h), false);
                                }
                                Some(Msg::Auth { protocol, .. }) if protocol != PROTOCOL => {
                                    send_line(&out, &Msg::Denied { reason: "Andere Amboss-Version – bitte beide auf denselben Stand bringen".into() });
                                }
                                _ => {
                                    send_line(&out, &Msg::Denied { reason: "Falsches Passwort".into() });
                                    let _ = tx.send(Event::Info(format!("Anmeldung von {addr} abgelehnt (falsches Passwort)")));
                                }
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(150)),
                }
                }
                if idle {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });
        let me = PeerInfo { id: 0, name: name.to_string(), color: COLORS[0] };
        Ok(Collab {
            role: Role::Host,
            me,
            users: Vec::new(),
            chat: Vec::new(),
            status: format!("Gastgeber auf Port {port} – warte auf Gäste"),
            rx,
            peers: Vec::new(),
            host_out: None,
            synced: Some(ed.map.clone()),
            synced_rev: ed.rev,
            last_send: Instant::now(),
            last_presence: Instant::now(),
            last_presence_data: None,
            port,
            stop,
            connected: true,
            conflicts: Vec::new(),
        })
    }

    /// Mit einem Gastgeber verbinden (IPv4, IPv6 oder Name, Port optional – siehe `resolve_addr`)
    pub fn join(name: &str, addr: &str, password: &str) -> Result<Collab, String> {
        let addrs = resolve_addr(addr)?;
        let mut last_err = String::new();
        let mut found = None;
        for sa in &addrs {
            match TcpStream::connect_timeout(sa, Duration::from_secs(8)) {
                Ok(s) => {
                    found = Some((s, *sa));
                    break;
                }
                Err(e) => last_err = connect_hint(sa, &e),
            }
        }
        let Some((stream, sock)) = found else { return Err(format!("Verbindung zu {addr} fehlgeschlagen: {last_err}")) };
        let addr = sock.to_string();
        let _ = stream.set_nodelay(true);
        let (out, slot) = writer_thread(stream.try_clone().map_err(|e| e.to_string())?);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
        let mut r = BufReader::new(stream);
        let nonce = match read_msg(&mut r, None, &mut false) {
            Some(Msg::Challenge { nonce }) => nonce,
            _ => return Err("Gastgeber antwortet nicht wie erwartet (andere Amboss-Version?)".into()),
        };
        let pwkey = netcrypt::stretch(password);
        let cnonce = random_nonce();
        send_line(&out, &Msg::Auth { protocol: PROTOCOL, name: name.to_string(), proof: proof(&nonce, &cnonce, &pwkey), cnonce: cnonce.clone() });
        let (h2g, g2h) = netcrypt::session_keys(&pwkey, &nonce, &cnonce);
        start_sealing(&out, &slot, Sealer::new(g2h));
        // Antwort (Willkommen oder Ablehnung) kommt über poll()
        let _ = r.get_ref().set_read_timeout(None);
        let (tx, rx) = channel();
        std::thread::spawn(move || reader_loop(r, 0, tx, Opener::new(h2g), true));
        Ok(Collab {
            role: Role::Guest,
            me: PeerInfo { id: u32::MAX, name: name.to_string(), color: [200, 200, 200] },
            users: Vec::new(),
            chat: Vec::new(),
            status: format!("Anmeldung bei {addr}…"),
            rx,
            peers: Vec::new(),
            host_out: Some(out),
            synced: None,
            synced_rev: 0,
            last_send: Instant::now(),
            last_presence: Instant::now(),
            last_presence_data: None,
            port: sock.port(),
            stop: Default::default(),
            connected: true,
            conflicts: Vec::new(),
        })
    }

    pub fn participants(&self) -> usize {
        1 + self.users.len()
    }

    fn broadcast(&self, m: &Msg, except: Option<u32>) {
        if let Ok(s) = serde_json::to_string(m) {
            for p in &self.peers {
                if Some(p.info.id) != except {
                    let _ = p.out.send(s.clone());
                }
            }
        }
    }

    fn send(&self, m: &Msg) {
        match self.role {
            Role::Host => self.broadcast(m, None),
            Role::Guest => {
                if let Some(o) = &self.host_out {
                    send_line(o, m);
                }
            }
        }
    }

    fn peer_list(&self) -> Vec<PeerInfo> {
        std::iter::once(self.me.clone()).chain(self.peers.iter().map(|p| p.info.clone())).collect()
    }

    fn name_of(&self, id: u32) -> (String, [u8; 3]) {
        if id == self.me.id {
            return (self.me.name.clone(), self.me.color);
        }
        self.users.iter().find(|u| u.id == id).map(|u| (u.name.clone(), u.color)).unwrap_or_else(|| (format!("#{id}"), [200, 200, 200]))
    }

    /// Teilnehmer für die Anzeige in den Ansichten
    fn publish_users(&self, ed: &mut Editor) {
        ed.remote_users = self.users.clone();
    }

    fn set_users(&mut self, peers: &[PeerInfo]) {
        let mut users = Vec::new();
        for p in peers.iter().filter(|p| p.id != self.me.id) {
            let mut u = self.users.iter().find(|u| u.id == p.id).cloned().unwrap_or_default();
            u.id = p.id;
            u.name = p.name.clone();
            u.color = p.color;
            users.push(u);
        }
        self.users = users;
    }

    /// Fremde Auswahl sperren
    fn update_locks(&self, ed: &mut Editor) {
        self.publish_users(ed);
        ed.remote_locked_solids = self.users.iter().flat_map(|u| u.solids.iter().copied()).collect();
        ed.remote_locked_entities = self.users.iter().flat_map(|u| u.entities.iter().copied()).collect();
        // falls jemand etwas übernommen hat, was ich gerade gewählt habe: aus meiner Auswahl nehmen
        let (ls, le) = (ed.remote_locked_solids.clone(), ed.remote_locked_entities.clone());
        ed.sel.retain(|o| match o {
            Obj::Solid(id) => !ls.contains(id),
            Obj::Entity(id) => !le.contains(id),
        });
    }

    /// Fremde Änderungen übernehmen (vorher wurden eigene offene Änderungen gesendet)
    fn apply_remote(&mut self, ed: &mut Editor, ops: &[Op]) {
        let parsed: Vec<Parsed> = ops.iter().filter_map(parse_op).collect();
        ed.apply_remote_ops(&parsed);
        if let Some(s) = &mut self.synced {
            for p in &parsed {
                apply_to_map(s, p);
            }
        }
        self.synced_rev = ed.rev;
    }

    /// Einmal pro Bild aufrufen: Nachrichten verarbeiten, eigene Änderungen und Kamera senden
    pub fn poll(&mut self, ed: &mut Editor, cam: ([f32; 3], [f32; 3]), dragging: bool) -> Vec<Notice> {
        let mut notes = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Info(s) => notes.push(Notice::Text(s)),
                Event::Joined { conn, name, out } => {
                    // Teilnehmer-ID = Verbindungsnummer (eindeutig, ab 1)
                    let id = conn;
                    let color = COLORS[id as usize % COLORS.len()];
                    let info = PeerInfo { id, name: name.clone(), color };
                    // erst eigene offene Änderungen senden, dann den aktuellen Stand übergeben
                    self.flush(ed);
                    let id_base = ID_RANGE * id + ed.map.next_id.max(1).min(ID_RANGE - 1);
                    self.peers.push(Peer { info: info.clone(), out: out.clone() });
                    send_line(&out, &Msg::Welcome { you: id, color, id_base, map: ed.map.to_vmf(), peers: self.peer_list() });
                    let peers = self.peer_list();
                    self.broadcast(&Msg::Peers { peers: peers.clone() }, Some(id));
                    self.set_users(&peers);
                    self.status = format!("{} Teilnehmer", self.participants());
                    notes.push(Notice::Text(format!("{name} ist beigetreten")));
                    self.chat.push((String::new(), color, format!("{name} ist beigetreten")));
                    self.last_presence_data = None;
                }
                Event::Closed { conn } => match self.role {
                    Role::Host => {
                        if let Some(pos) = self.peers.iter().position(|p| p.info.id == conn_to_peer(&self.peers, conn)) {
                            let p = self.peers.remove(pos);
                            self.chat.push((String::new(), p.info.color, format!("{} hat die Sitzung verlassen", p.info.name)));
                            notes.push(Notice::Text(format!("{} hat die Sitzung verlassen", p.info.name)));
                            let peers = self.peer_list();
                            self.broadcast(&Msg::Peers { peers: peers.clone() }, None);
                            self.set_users(&peers);
                            self.update_locks(ed);
                            self.status = format!("{} Teilnehmer", self.participants());
                        }
                    }
                    Role::Guest => {
                        self.connected = false;
                        self.users.clear();
                        self.update_locks(ed);
                        self.status = "Verbindung zum Gastgeber getrennt – die Map bleibt als lokale Kopie offen".into();
                        notes.push(Notice::Disconnected);
                    }
                },
                Event::Msg { conn, msg } => self.handle(ed, conn, msg, &mut notes),
            }
        }
        if !self.connected {
            return notes;
        }
        // eigene Änderungen senden (max. 10× pro Sekunde, während des Ziehens 5×)
        let interval = if dragging { 200 } else { 100 };
        if ed.rev != self.synced_rev && self.last_send.elapsed() >= Duration::from_millis(interval) {
            self.flush(ed);
        }
        // Kamera und Auswahl (max. 5× pro Sekunde, nur bei Änderung)
        if self.last_presence.elapsed() >= Duration::from_millis(200) {
            let solids: Vec<u32> = ed.sel.iter().filter_map(|o| if let Obj::Solid(id) = o { Some(*id) } else { None }).collect();
            let entities: Vec<u32> = ed.sel.iter().filter_map(|o| if let Obj::Entity(id) = o { Some(*id) } else { None }).collect();
            let c = [cam.0[0], cam.0[1], cam.0[2], cam.1[0], cam.1[1], cam.1[2]];
            let data = (c, solids, entities);
            if self.last_presence_data.as_ref() != Some(&data) {
                self.send(&Msg::Presence { from: self.me.id, cam: c, solids: data.1.clone(), entities: data.2.clone() });
                self.last_presence_data = Some(data);
            }
            self.last_presence = Instant::now();
        }
        notes
    }

    /// Offene eigene Änderungen sofort senden
    pub fn flush(&mut self, ed: &Editor) {
        self.last_send = Instant::now();
        let Some(synced) = &self.synced else { return };
        if ed.rev == self.synced_rev {
            return;
        }
        let ops = diff_ops(synced, &ed.map);
        self.synced = Some(ed.map.clone());
        self.synced_rev = ed.rev;
        if !ops.is_empty() {
            self.send(&Msg::Ops { from: self.me.id, ops });
        }
    }

    fn handle(&mut self, ed: &mut Editor, conn: u32, msg: Msg, notes: &mut Vec<Notice>) {
        match msg {
            Msg::Welcome { you, color, id_base, map, peers } => match Map::from_vmf(&map) {
                Ok(mut m) => {
                    self.me.id = you;
                    self.me.color = color;
                    m.next_id = m.next_id.max(id_base);
                    self.set_users(&peers);
                    ed.map = m;
                    ed.path = None;
                    ed.sel.clear();
                    ed.faces.clear();
                    ed.vsel.clear();
                    ed.clear_history();
                    ed.dirty = true;
                    ed.touch();
                    self.set_synced(ed);
                    notes.push(Notice::Joined);
                    self.status = format!("Verbunden – {} Teilnehmer", self.participants());
                }
                Err(e) => notes.push(Notice::Text(format!("Map des Gastgebers fehlerhaft: {e}"))),
            },
            Msg::Peers { peers } => {
                self.set_users(&peers);
                self.update_locks(ed);
                self.status = format!("Verbunden – {} Teilnehmer", self.participants());
            }
            Msg::Ops { from, ops } => {
                let from = if self.role == Role::Host { conn_to_peer(&self.peers, conn) } else { from };
                // Konflikte: dasselbe Objekt wurde hier geändert, aber noch nicht gesendet.
                // Standard: eigene Version bleibt (wird gleich an alle gesendet), die andere lässt sich übernehmen.
                let local = self.synced.as_ref().map(|s| diff_ops(s, &ed.map)).unwrap_or_default();
                let local_keys: std::collections::HashSet<(u8, u32)> = local.iter().filter_map(op_key).collect();
                let (conflicting, clean): (Vec<Op>, Vec<Op>) = ops.into_iter().partition(|o| op_key(o).is_some_and(|k| local_keys.contains(&k)));
                for o in conflicting {
                    let (name, color) = self.name_of(from);
                    if let Some(c) = make_conflict(ed, &o, name, color) {
                        self.conflicts.retain(|x| x.key != c.key);
                        self.conflicts.push(c);
                        notes.push(Notice::Conflict);
                    }
                }
                // offene eigene Änderungen zuerst abschicken, damit sie nicht verloren gehen
                self.flush(ed);
                self.apply_remote(ed, &clean);
                if self.role == Role::Host && !clean.is_empty() {
                    self.broadcast(&Msg::Ops { from, ops: clean }, Some(from));
                }
            }
            Msg::Presence { from, cam, solids, entities } => {
                let from = if self.role == Role::Host { conn_to_peer(&self.peers, conn) } else { from };
                if let Some(u) = self.users.iter_mut().find(|u| u.id == from) {
                    u.cam = Some(([cam[0], cam[1], cam[2]], [cam[3], cam[4], cam[5]]));
                    u.solids = solids.clone();
                    u.entities = entities.clone();
                }
                self.update_locks(ed);
                if self.role == Role::Host {
                    self.broadcast(&Msg::Presence { from, cam, solids, entities }, Some(from));
                }
            }
            Msg::Chat { from, text } => {
                let from = if self.role == Role::Host { conn_to_peer(&self.peers, conn) } else { from };
                let (n, c) = self.name_of(from);
                self.chat.push((n, c, text.clone()));
                if self.role == Role::Host {
                    self.broadcast(&Msg::Chat { from, text }, Some(from));
                }
            }
            Msg::Denied { reason } => {
                self.connected = false;
                self.host_out = None;
                self.status = format!("Abgelehnt: {reason}");
                notes.push(Notice::Denied(reason));
            }
            Msg::Challenge { .. } | Msg::Auth { .. } => {}
        }
    }

    /// Konflikt entscheiden: `theirs` = Version der anderen Person übernehmen (wird an alle gesendet)
    pub fn resolve_conflict(&mut self, ed: &mut Editor, i: usize, theirs: bool) {
        if i >= self.conflicts.len() {
            return;
        }
        let c = self.conflicts.remove(i);
        if theirs {
            if let Some(p) = parse_op(&c.remote) {
                ed.checkpoint();
                apply_to_map(&mut ed.map, &p);
                ed.status = format!("Version von {} übernommen: {}", c.from_name, c.label);
                ed.touch();
                self.flush(ed);
            }
        }
    }

    /// Abgelaufene Konflikte entfernen (eigene Version bleibt)
    pub fn expire_conflicts(&mut self) {
        self.conflicts.retain(|c| c.created.elapsed().as_secs_f32() < CONFLICT_SECS);
    }

    /// Gast: nach dem Übernehmen der Map den Abgleich-Stand setzen
    pub fn set_synced(&mut self, ed: &Editor) {
        self.synced = Some(ed.map.clone());
        self.synced_rev = ed.rev;
    }

    pub fn say(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.chat.push((self.me.name.clone(), self.me.color, text.to_string()));
        self.send(&Msg::Chat { from: self.me.id, text: text.to_string() });
    }

    pub fn stop(&mut self, ed: &mut Editor) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.peers.clear();
        self.host_out = None;
        self.users.clear();
        self.connected = false;
        self.update_locks(ed);
    }
}

impl Drop for Collab {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Gastgeber: Verbindungsnummer → Teilnehmer-ID (beide werden in derselben Reihenfolge vergeben)
fn conn_to_peer(peers: &[Peer], conn: u32) -> u32 {
    let _ = peers;
    conn
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fgd::Fgd;
    use crate::geometry::Aabb;
    use glam::DVec3;

    /// Manueller Test gegen einen laufenden Editor: AMBOSS_JOIN=host:port AMBOSS_PW=… cargo test live_guest -- --ignored
    #[test]
    #[ignore]
    fn live_guest() {
        let addr = std::env::var("AMBOSS_JOIN").unwrap();
        let pw = std::env::var("AMBOSS_PW").unwrap();
        let mut ed = Editor::new(Fgd::builtin());
        let mut c = Collab::join("Ben (Test)", &addr, &pw).unwrap();
        let cam = ([-300.0, -300.0, 200.0], [0.7, 0.7, -0.2]);
        for _ in 0..100 {
            c.poll(&mut ed, cam, false);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!ed.map.world.solids.is_empty(), "Map empfangen");
        ed.create_block(Aabb::new(DVec3::new(-200.0, -200.0, 0.0), DVec3::new(-72.0, -72.0, 256.0)));
        c.say("Hallo! Ich baue die Säule links.");
        for _ in 0..400 {
            c.poll(&mut ed, cam, false);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn sha256_known_values() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex(&sha256(&[b'a'; 1000])), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    fn pump(a: &mut Collab, ea: &mut Editor, b: &mut Collab, eb: &mut Editor) {
        for _ in 0..40 {
            a.poll(ea, ([0.0; 3], [1.0, 0.0, 0.0]), false);
            b.poll(eb, ([0.0; 3], [0.0, 1.0, 0.0]), false);
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    #[test]
    fn addresses_and_ipv6() {
        let p = DEFAULT_PORT;
        assert_eq!(resolve_addr("192.168.0.10").unwrap()[0].to_string(), format!("192.168.0.10:{p}"));
        assert_eq!(resolve_addr("192.168.0.10:4000").unwrap()[0].to_string(), "192.168.0.10:4000");
        assert_eq!(resolve_addr(" 2001:db8::1 ").unwrap()[0].to_string(), format!("[2001:db8::1]:{p}"));
        assert_eq!(resolve_addr("[2001:db8::1]").unwrap()[0].to_string(), format!("[2001:db8::1]:{p}"));
        assert_eq!(resolve_addr("[2001:db8::1]:4000").unwrap()[0].to_string(), "[2001:db8::1]:4000");
        assert!(resolve_addr("").is_err());
        // Verbindung über IPv6-Loopback (falls das System IPv6 hat)
        let ea = Editor::new(Fgd::builtin());
        let mut ea = ea;
        ea.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let mut host = Collab::host("Anna", 0, "geheim", &ea).unwrap();
        if std::net::TcpStream::connect(("::1", host.port)).is_err() {
            eprintln!("kein IPv6-Loopback – Teil übersprungen");
            return;
        }
        let mut eb = Editor::new(Fgd::builtin());
        let mut guest = Collab::join("Ben", &format!("::1"), "geheim").or_else(|_| Collab::join("Ben", &format!("[::1]:{}", host.port), "geheim")).unwrap();
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert_eq!(eb.map.world.solids.len(), 1, "Gast hat die Map über IPv6");
        // IPv4 geht weiterhin
        let mut ec = Editor::new(Fgd::builtin());
        let mut g2 = Collab::join("Cem", &format!("127.0.0.1:{}", host.port), "geheim").unwrap();
        pump(&mut host, &mut ea, &mut g2, &mut ec);
        assert_eq!(ec.map.world.solids.len(), 1, "Gast hat die Map über IPv4");
    }

    #[test]
    fn two_people_edit_together() {
        let mut ea = Editor::new(Fgd::builtin());
        ea.create_block(Aabb::new(DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0)));
        let floor = ea.map.world.solids[0].id;
        let mut host = Collab::host("Anna", 0, "geheim", &ea).unwrap();
        let port = host.port;
        let mut em = Editor::new(Fgd::builtin());
        let mut mallory = Collab::join("Mallory", &format!("127.0.0.1:{port}"), "falsch").unwrap();
        let mut denied = false;
        for _ in 0..100 {
            host.poll(&mut ea, ([0.0; 3], [1.0, 0.0, 0.0]), false);
            if mallory.poll(&mut em, ([0.0; 3], [1.0, 0.0, 0.0]), false).iter().any(|n| matches!(n, Notice::Denied(_))) {
                denied = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(denied && em.map.world.solids.is_empty() && host.users.is_empty());
        let mut eb = Editor::new(Fgd::builtin());
        let mut guest = Collab::join("Ben", &format!("127.0.0.1:{port}"), "geheim").unwrap();
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert_eq!(eb.map.world.solids.len(), 1, "Gast hat die Map");
        assert_eq!(host.users.len(), 1);
        assert_eq!(guest.users[0].name, "Anna");

        // Ben baut einen Block, Anna ein Licht
        eb.create_block(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        let bens = eb.map.world.solids.iter().find(|s| s.id != floor).unwrap().id;
        assert!(bens > ID_RANGE, "eigener ID-Bereich: {bens}");
        ea.entity_class = "light".into();
        ea.create_point_entity(DVec3::new(0.0, 0.0, 100.0));
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert_eq!(ea.map.world.solids.len(), 2);
        assert_eq!(eb.map.entities.len(), 1);
        assert_eq!(ea.map.to_vmf(), eb.map.to_vmf(), "beide Seiten gleich");

        // Ben wählt seinen Block -> für Anna gesperrt
        eb.sel = [Obj::Solid(bens)].into_iter().collect();
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert!(ea.remote_locked_solids.contains(&bens));
        assert!(!ea.selectable().iter().any(|(o, _)| *o == Obj::Solid(bens)));

        // Ben verschiebt den Block, Anna macht ihr Licht rückgängig -> Bens Änderung bleibt
        eb.checkpoint();
        eb.translate_selection(DVec3::new(0.0, 0.0, 32.0));
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert_eq!(ea.map.solid(bens).unwrap().bounds.min.z, 32.0);
        ea.undo();
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert!(ea.map.entities.is_empty() && eb.map.entities.is_empty(), "Licht bei beiden weg");
        assert_eq!(ea.map.solid(bens).unwrap().bounds.min.z, 32.0, "Bens Verschiebung bleibt bei Annas Rückgängig");
        assert_eq!(ea.map.to_vmf(), eb.map.to_vmf());

        // Chat
        guest.say("Hallo Anna");
        pump(&mut host, &mut ea, &mut guest, &mut eb);
        assert!(host.chat.iter().any(|c| c.0 == "Ben" && c.2 == "Hallo Anna"));

        // Gastgeber beendet -> Gast getrennt
        host.stop(&mut ea);
        drop(host);
        let mut gone = false;
        for _ in 0..100 {
            if guest.poll(&mut eb, ([0.0; 3], [1.0, 0.0, 0.0]), false).iter().any(|n| matches!(n, Notice::Disconnected)) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(gone);
    }
}
