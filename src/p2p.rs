//! Live-Sitzung ohne Portfreigabe: eigene Internet-Adresse per STUN erfragen, Adressen als kurzen
//! Verbindungscode austauschen (z. B. per Discord), dann öffnen beide Seiten gleichzeitig ein
//! „Loch“ in ihren Router (UDP-Hole-Punching). Darüber läuft ein einfacher, zuverlässiger
//! Datenstrom (Wiederholen bis bestätigt, Reihenfolge wiederherstellen), auf dem die normale,
//! verschlüsselte Live-Sitzung aufsetzt.
//!
//! Klappt mit den meisten Heim-Routern; nicht bei „symmetrischem“ NAT (manche Mobilfunk- und
//! Firmennetze). Dann bleibt IPv6, Portweiterleitung oder ein VPN.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Öffentliche STUN-Server (fragen nur nach der eigenen Adresse; es werden keine Map-Daten gesendet)
pub const STUN_SERVERS: [&str; 4] = ["stun.l.google.com:19302", "stun1.l.google.com:19302", "stun.cloudflare.com:3478", "stun.nextcloud.com:443"];

const MAGIC: [u8; 2] = *b"AB";
const T_PUNCH: u8 = 1;
const T_PUNCH_ACK: u8 = 2;
const T_DATA: u8 = 3;
const T_ACK: u8 = 4;
const T_ALIVE: u8 = 5;
const T_FIN: u8 = 6;
const HEADER: usize = 2 + 1 + 6 + 4;
/// Nutzdaten je Paket (bleibt unter der üblichen MTU)
const CHUNK: usize = 1100;
/// höchstens so viele unbestätigte Pakete unterwegs
const WINDOW: usize = 256;
/// ohne Lebenszeichen gilt die Verbindung nach dieser Zeit als getrennt
const DEAD_AFTER: Duration = Duration::from_secs(45);

// ------------------------------------------------------------------ STUN

/// STUN-Binding-Anfrage (RFC 5389): Kopf mit Magic Cookie und zufälliger Transaktions-ID
fn stun_request(txid: &[u8; 12]) -> Vec<u8> {
    let mut b = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    b.extend_from_slice(txid);
    b
}

/// Antwort auswerten: XOR-MAPPED-ADDRESS (oder MAPPED-ADDRESS) → öffentliche IPv4-Adresse und Port
pub fn parse_stun_response(buf: &[u8], txid: &[u8; 12]) -> Option<SocketAddrV4> {
    if buf.len() < 20 || buf[0..2] != [0x01, 0x01] || buf[4..8] != [0x21, 0x12, 0xA4, 0x42] || buf[8..20] != txid[..] {
        return None;
    }
    let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let end = (20 + len).min(buf.len());
    let mut i = 20;
    let mut plain = None;
    while i + 4 <= end {
        let t = u16::from_be_bytes([buf[i], buf[i + 1]]);
        let l = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let v = buf.get(i + 4..i + 4 + l)?;
        if (t == 0x0020 || t == 0x0001) && l >= 8 && v[1] == 0x01 {
            let mut port = u16::from_be_bytes([v[2], v[3]]);
            let mut ip = [v[4], v[5], v[6], v[7]];
            if t == 0x0020 {
                port ^= 0x2112;
                for (k, b) in ip.iter_mut().enumerate() {
                    *b ^= [0x21, 0x12, 0xA4, 0x42][k];
                }
                return Some(SocketAddrV4::new(Ipv4Addr::from(ip), port));
            }
            plain = Some(SocketAddrV4::new(Ipv4Addr::from(ip), port));
        }
        i += 4 + l.div_ceil(4) * 4;
    }
    plain
}

/// Eigene öffentliche Adresse für diesen Socket erfragen (mehrere Server, je ~1,5 s)
pub fn stun_public(sock: &UdpSocket) -> Result<SocketAddrV4, String> {
    let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));
    let mut last = String::from("keine Antwort");
    for server in STUN_SERVERS {
        let Some(addr) = server.to_socket_addrs().ok().and_then(|mut a| a.find(|a| a.is_ipv4())) else {
            last = format!("{server}: Name nicht auflösbar");
            continue;
        };
        let txid = crate::netcrypt::random_bytes::<12>();
        let req = stun_request(&txid);
        for _ in 0..3 {
            if sock.send_to(&req, addr).is_err() {
                break;
            }
            let mut buf = [0u8; 512];
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) if from == addr => {
                        if let Some(a) = parse_stun_response(&buf[..n], &txid) {
                            return Ok(a);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        last = format!("{server}: keine Antwort");
    }
    Err(format!("Eigene Internet-Adresse nicht ermittelbar ({last}) – UDP ins Internet gesperrt?"))
}

/// Adresse im Heimnetz (Route zum Internet wählen, ohne etwas zu senden)
fn lan_ip() -> Ipv4Addr {
    let s = UdpSocket::bind("0.0.0.0:0").ok();
    s.and_then(|s| {
        s.connect("192.0.2.1:9").ok()?;
        match s.local_addr().ok()?.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            _ => None,
        }
    })
    .unwrap_or(Ipv4Addr::LOCALHOST)
}

// ------------------------------------------------------------------ Verbindungscode

/// Eigene Seite eines Verbindungsversuchs: Socket, öffentliche und lokale Adresse, Kennung
pub struct Invite {
    pub sock: UdpSocket,
    pub public: Option<SocketAddrV4>,
    pub local: SocketAddrV4,
    pub token: [u8; 6],
}

/// Gegenseite aus ihrem Code
#[derive(Debug, Clone, PartialEq)]
pub struct PeerCode {
    pub public: SocketAddrV4,
    pub local: SocketAddrV4,
    pub token: [u8; 6],
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64(data: &[u8]) -> String {
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..=c.len() {
            out.push(B64[((n >> (18 - 6 * k)) & 63) as usize] as char);
        }
    }
    out
}

fn unb64(s: &str) -> Option<Vec<u8>> {
    let vals: Vec<u32> = s.bytes().map(|b| B64.iter().position(|x| *x == b).map(|p| p as u32)).collect::<Option<_>>()?;
    let mut out = Vec::new();
    for c in vals.chunks(4) {
        let mut n = 0u32;
        for (k, v) in c.iter().enumerate() {
            n |= v << (18 - 6 * k);
        }
        for k in 0..c.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * k)) as u8);
        }
    }
    Some(out)
}

impl Invite {
    /// Socket öffnen und die eigene Internet-Adresse erfragen. Ohne Antwort vom STUN-Server
    /// klappt nur noch eine Verbindung im selben Netz.
    pub fn new() -> Result<Invite, String> {
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        let port = sock.local_addr().map_err(|e| e.to_string())?.port();
        let public = stun_public(&sock).ok();
        Ok(Invite { sock, public, local: SocketAddrV4::new(lan_ip(), port), token: crate::netcrypt::random_bytes::<6>() })
    }

    /// Nur für Tests: ohne STUN, feste lokale Adresse
    #[cfg(test)]
    pub fn local_only() -> Invite {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        Invite { sock, public: None, local: SocketAddrV4::new(Ipv4Addr::LOCALHOST, port), token: crate::netcrypt::random_bytes::<6>() }
    }

    /// Kurzer Text zum Weitergeben, z. B. „AMB1-…“
    pub fn code(&self) -> String {
        let p = self.public.unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
        let mut d = vec![1u8];
        d.extend_from_slice(&p.ip().octets());
        d.extend_from_slice(&p.port().to_be_bytes());
        d.extend_from_slice(&self.local.ip().octets());
        d.extend_from_slice(&self.local.port().to_be_bytes());
        d.extend_from_slice(&self.token);
        format!("AMB1-{}", b64(&d))
    }
}

/// Code der Gegenseite lesen (Leerzeichen und Zeilenumbrüche werden ignoriert)
pub fn parse_code(s: &str) -> Result<PeerCode, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let body = s.strip_prefix("AMB1-").ok_or("Das ist kein Amboss-Verbindungscode (beginnt mit „AMB1-“)")?;
    let d = unb64(body).ok_or("Code ist beschädigt (unerlaubte Zeichen)")?;
    if d.len() < 19 || d[0] != 1 {
        return Err("Code ist unvollständig – bitte komplett kopieren".into());
    }
    let ip = |o: usize| Ipv4Addr::new(d[o], d[o + 1], d[o + 2], d[o + 3]);
    let port = |o: usize| u16::from_be_bytes([d[o], d[o + 1]]);
    let mut token = [0u8; 6];
    token.copy_from_slice(&d[13..19]);
    Ok(PeerCode { public: SocketAddrV4::new(ip(1), port(5)), local: SocketAddrV4::new(ip(7), port(11)), token })
}

// ------------------------------------------------------------------ Pakete

fn packet(kind: u8, token: &[u8; 6], seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(HEADER + payload.len());
    b.extend_from_slice(&MAGIC);
    b.push(kind);
    b.extend_from_slice(token);
    b.extend_from_slice(&seq.to_be_bytes());
    b.extend_from_slice(payload);
    b
}

/// (Art, Absender-Kennung, Nummer, Nutzdaten)
fn parse_packet(b: &[u8]) -> Option<(u8, [u8; 6], u32, &[u8])> {
    if b.len() < HEADER || b[0..2] != MAGIC {
        return None;
    }
    let mut t = [0u8; 6];
    t.copy_from_slice(&b[3..9]);
    Some((b[2], t, u32::from_be_bytes([b[9], b[10], b[11], b[12]]), &b[HEADER..]))
}

/// Löcher in beide Router schlagen: abwechselnd an die öffentliche und die lokale Adresse der
/// Gegenseite senden, bis ein gültiges Paket von ihr ankommt. Dann steht die Verbindung.
pub fn punch(inv: Invite, peer: &PeerCode, timeout: Duration) -> Result<Rudp, String> {
    let sock = inv.sock;
    let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
    let mut targets: Vec<SocketAddr> = Vec::new();
    if !peer.public.ip().is_unspecified() && peer.public.port() != 0 {
        targets.push(SocketAddr::V4(peer.public));
    }
    if peer.local.port() != 0 && Some(peer.local) != inv.public {
        targets.push(SocketAddr::V4(peer.local));
    }
    if targets.is_empty() {
        return Err("Der Code der Gegenseite enthält keine Adresse".into());
    }
    let deadline = Instant::now() + timeout;
    let mut next_send = Instant::now();
    let mut buf = [0u8; 2048];
    while Instant::now() < deadline {
        if Instant::now() >= next_send {
            for t in &targets {
                let _ = sock.send_to(&packet(T_PUNCH, &inv.token, 0, &[]), t);
            }
            next_send = Instant::now() + Duration::from_millis(200);
        }
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            if let Some((kind, token, _, _)) = parse_packet(&buf[..n]) {
                if token == peer.token && matches!(kind, T_PUNCH | T_PUNCH_ACK | T_DATA | T_ACK | T_ALIVE) {
                    // Gegenseite erreicht uns – bestätigen (einige Male, falls Pakete verloren gehen)
                    for _ in 0..3 {
                        let _ = sock.send_to(&packet(T_PUNCH_ACK, &inv.token, 0, &[]), from);
                    }
                    return Ok(Rudp::start(sock, from, inv.token, peer.token, 0.0));
                }
            }
        }
    }
    Err("Keine Verbindung zustande gekommen. Beide Codes richtig ausgetauscht und beide gleichzeitig am Verbinden? Manche Netze (Mobilfunk, Firmen) lassen das nicht zu – dann IPv6, Portweiterleitung oder ein VPN wie Tailscale nutzen.".into())
}

// ------------------------------------------------------------------ zuverlässiger Datenstrom

struct State {
    next_seq: u32,
    /// gesendet, noch nicht bestätigt: Nummer → (Daten, zuletzt gesendet, Versuche)
    unacked: BTreeMap<u32, (Vec<u8>, Instant, u32)>,
    recv_next: u32,
    recv_buf: BTreeMap<u32, Vec<u8>>,
    inbox: VecDeque<u8>,
    closed: bool,
    last_rx: Instant,
    last_tx: Instant,
}

struct Inner {
    sock: UdpSocket,
    peer: SocketAddr,
    my: [u8; 6],
    their: [u8; 6],
    st: Mutex<State>,
    cv: Condvar,
    stop: AtomicBool,
    read_timeout: Mutex<Option<Duration>>,
    /// nur für Tests: Anteil absichtlich verworfener Pakete
    drop_rate: f64,
}

impl Inner {
    fn send(&self, kind: u8, seq: u32, payload: &[u8]) {
        if self.drop_rate > 0.0 && rand_f64() < self.drop_rate {
            return;
        }
        let _ = self.sock.send_to(&packet(kind, &self.my, seq, payload), self.peer);
    }
}

fn rand_f64() -> f64 {
    let b = crate::netcrypt::random_bytes::<8>();
    (u64::from_le_bytes(b) >> 11) as f64 / (1u64 << 53) as f64
}

/// Zuverlässige Verbindung über UDP (mit `split` in Lese- und Schreibseite teilen)
pub struct Rudp {
    inner: Arc<Inner>,
}

pub struct RudpReader {
    inner: Arc<Inner>,
}

pub struct RudpWriter {
    inner: Arc<Inner>,
}

impl Rudp {
    fn start(sock: UdpSocket, peer: SocketAddr, my: [u8; 6], their: [u8; 6], drop_rate: f64) -> Rudp {
        let _ = sock.set_read_timeout(Some(Duration::from_millis(20)));
        let now = Instant::now();
        let inner = Arc::new(Inner {
            sock,
            peer,
            my,
            their,
            st: Mutex::new(State { next_seq: 1, unacked: BTreeMap::new(), recv_next: 1, recv_buf: BTreeMap::new(), inbox: VecDeque::new(), closed: false, last_rx: now, last_tx: now }),
            cv: Condvar::new(),
            stop: AtomicBool::new(false),
            read_timeout: Mutex::new(None),
            drop_rate,
        });
        let w = inner.clone();
        std::thread::spawn(move || pump(w));
        Rudp { inner }
    }

    pub fn peer(&self) -> SocketAddr {
        self.inner.peer
    }

    pub fn split(self) -> (RudpReader, RudpWriter) {
        (RudpReader { inner: self.inner.clone() }, RudpWriter { inner: self.inner })
    }
}

/// Hintergrund: Pakete empfangen, bestätigen, sortieren; Unbestätigtes wiederholen; Lebenszeichen
fn pump(inner: Arc<Inner>) {
    let mut buf = [0u8; 2048];
    loop {
        if inner.stop.load(Ordering::SeqCst) {
            inner.send(T_FIN, 0, &[]);
            let mut st = inner.st.lock().unwrap();
            st.closed = true;
            inner.cv.notify_all();
            return;
        }
        match inner.sock.recv_from(&mut buf) {
            Ok((n, from)) if from == inner.peer => {
                if let Some((kind, token, seq, payload)) = parse_packet(&buf[..n]) {
                    if token == inner.their {
                        let mut st = inner.st.lock().unwrap();
                        st.last_rx = Instant::now();
                        match kind {
                            T_DATA => {
                                drop(st);
                                inner.send(T_ACK, seq, &[]);
                                let mut st = inner.st.lock().unwrap();
                                if seq >= st.recv_next {
                                    st.recv_buf.insert(seq, payload.to_vec());
                                    while let Some(d) = {
                                        let k = st.recv_next;
                                        st.recv_buf.remove(&k)
                                    } {
                                        st.inbox.extend(d);
                                        st.recv_next += 1;
                                    }
                                    inner.cv.notify_all();
                                }
                            }
                            T_ACK => {
                                st.unacked.remove(&seq);
                                inner.cv.notify_all();
                            }
                            T_PUNCH => {
                                drop(st);
                                inner.send(T_PUNCH_ACK, 0, &[]);
                            }
                            T_FIN => {
                                st.closed = true;
                                inner.cv.notify_all();
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        // Wiederholen, Lebenszeichen, Zeitüberschreitung
        let mut resend: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut alive = false;
        {
            let mut st = inner.st.lock().unwrap();
            let now = Instant::now();
            for (seq, (data, sent, tries)) in st.unacked.iter_mut() {
                let rto = Duration::from_millis((250u64 << (*tries).min(3)).min(2000));
                if now.duration_since(*sent) >= rto {
                    *sent = now;
                    *tries += 1;
                    resend.push((*seq, data.clone()));
                }
            }
            if !resend.is_empty() {
                st.last_tx = now;
            } else if now.duration_since(st.last_tx) > Duration::from_secs(2) {
                st.last_tx = now;
                alive = true;
            }
            if now.duration_since(st.last_rx) > DEAD_AFTER && !st.closed {
                st.closed = true;
                inner.cv.notify_all();
            }
            if st.closed && st.unacked.is_empty() && inner.stop.load(Ordering::SeqCst) {
                return;
            }
        }
        for (seq, d) in resend {
            inner.send(T_DATA, seq, &d);
        }
        if alive {
            inner.send(T_ALIVE, 0, &[]);
        }
        if inner.st.lock().unwrap().closed && Arc::strong_count(&inner) == 1 {
            return;
        }
    }
}

impl Read for RudpReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let timeout = *self.inner.read_timeout.lock().unwrap();
        let start = Instant::now();
        let mut st = self.inner.st.lock().unwrap();
        loop {
            if !st.inbox.is_empty() {
                let n = buf.len().min(st.inbox.len());
                for (i, b) in st.inbox.drain(..n).enumerate() {
                    buf[i] = b;
                }
                return Ok(n);
            }
            if st.closed {
                return Ok(0);
            }
            let wait = match timeout {
                Some(t) => {
                    let el = start.elapsed();
                    if el >= t {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "Zeitüberschreitung"));
                    }
                    t - el
                }
                None => Duration::from_millis(500),
            };
            st = self.inner.cv.wait_timeout(st, wait).unwrap().0;
        }
    }
}

impl Write for RudpWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for chunk in buf.chunks(CHUNK) {
            let seq = {
                let mut st = self.inner.st.lock().unwrap();
                while st.unacked.len() >= WINDOW && !st.closed {
                    st = self.inner.cv.wait_timeout(st, Duration::from_millis(200)).unwrap().0;
                }
                if st.closed {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Verbindung getrennt"));
                }
                let seq = st.next_seq;
                st.next_seq += 1;
                st.unacked.insert(seq, (chunk.to_vec(), Instant::now(), 0));
                st.last_tx = Instant::now();
                seq
            };
            self.inner.send(T_DATA, seq, chunk);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl RudpReader {
    pub fn set_read_timeout(&self, d: Option<Duration>) {
        *self.inner.read_timeout.lock().unwrap() = d;
    }
}

impl RudpWriter {
    /// Verbindung beenden (Gegenseite bekommt ein Ende-Paket)
    pub fn close(&self) {
        self.inner.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(crate) fn test_pair(drop_rate: f64) -> (Rudp, Rudp) {
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").unwrap();
    let (aa, ba) = (a.local_addr().unwrap(), b.local_addr().unwrap());
    let (ta, tb) = ([1u8; 6], [2u8; 6]);
    (Rudp::start(a, ba, ta, tb, drop_rate), Rudp::start(b, aa, tb, ta, drop_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_roundtrip() {
        let inv = Invite::local_only();
        let mut inv2 = Invite::local_only();
        inv2.public = Some(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 50123));
        for i in [inv, inv2] {
            let c = i.code();
            assert!(c.starts_with("AMB1-") && c.len() < 40, "{c}");
            let p = parse_code(&format!("  {c}\n")).unwrap();
            assert_eq!(p.local, i.local);
            assert_eq!(p.token, i.token);
            assert_eq!(p.public, i.public.unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)));
        }
        assert!(parse_code("hallo").is_err());
        assert!(parse_code("AMB1-abc").is_err());
    }

    #[test]
    fn stun_xor_mapped_address() {
        let txid = [7u8; 12];
        // Antwort mit XOR-MAPPED-ADDRESS 198.51.100.20:40000
        let ip = [198u8, 51, 100, 20];
        let port: u16 = 40000;
        let mut b = vec![0x01, 0x01, 0x00, 0x0C, 0x21, 0x12, 0xA4, 0x42];
        b.extend_from_slice(&txid);
        b.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
        b.extend_from_slice(&(port ^ 0x2112).to_be_bytes());
        for (k, x) in ip.iter().enumerate() {
            b.push(x ^ [0x21, 0x12, 0xA4, 0x42][k]);
        }
        assert_eq!(parse_stun_response(&b, &txid), Some(SocketAddrV4::new(Ipv4Addr::from(ip), port)));
        assert_eq!(parse_stun_response(&b, &[8u8; 12]), None, "fremde Transaktion");
    }

    /// Hole-Punching im selben Netz und Datenaustausch in beide Richtungen
    #[test]
    fn punch_and_talk() {
        let (a, b) = (Invite::local_only(), Invite::local_only());
        let (ca, cb) = (parse_code(&a.code()).unwrap(), parse_code(&b.code()).unwrap());
        let tb = std::thread::spawn(move || punch(b, &ca, Duration::from_secs(5)));
        let ra = punch(a, &cb, Duration::from_secs(5)).expect("A verbunden");
        let rb = tb.join().unwrap().expect("B verbunden");
        let (mut ar, mut aw) = ra.split();
        let (mut br, mut bw) = rb.split();
        aw.write_all(b"hallo\n").unwrap();
        bw.write_all(b"zurueck\n").unwrap();
        let mut s = [0u8; 6];
        br.read_exact(&mut s).unwrap();
        assert_eq!(&s, b"hallo\n");
        let mut s = [0u8; 8];
        ar.read_exact(&mut s).unwrap();
        assert_eq!(&s, b"zurueck\n");
    }

    /// Große Daten kommen trotz verlorener Pakete vollständig und in Reihenfolge an
    #[test]
    fn reliable_under_loss() {
        let (a, b) = test_pair(0.2);
        let (_ar, mut aw) = a.split();
        let (mut br, _bw) = b.split();
        let data: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let d2 = data.clone();
        let t = std::thread::spawn(move || aw.write_all(&d2).unwrap());
        br.set_read_timeout(Some(Duration::from_secs(30)));
        let mut got = vec![0u8; data.len()];
        br.read_exact(&mut got).expect("alles angekommen");
        t.join().unwrap();
        assert!(got == data, "Daten unverändert und in Reihenfolge");
    }

    #[test]
    fn close_is_seen() {
        let (a, b) = test_pair(0.0);
        let (_ar, aw) = a.split();
        let (mut br, _bw) = b.split();
        aw.close();
        br.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 4];
        assert_eq!(br.read(&mut buf).unwrap(), 0, "Ende erkannt");
    }
}
