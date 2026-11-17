//! Verschlüsselung der Live-Sitzung: ChaCha20 (RFC 8439) + HMAC-SHA256 (Encrypt-then-MAC).
//!
//! Beide Seiten leiten aus dem Passwort und zwei Zufallswerten (je einer von Gastgeber und Gast)
//! eigene Schlüssel pro Richtung ab. Jede Zeile trägt einen fortlaufenden Zähler, damit
//! abgefangene Zeilen nicht wiederholt, vertauscht oder verändert werden können.

use crate::collab::sha256;

/// Wie oft das Passwort gehasht wird (bremst Durchprobieren von Passwörtern)
const STRETCH_ROUNDS: u32 = 60_000;

pub fn hmac(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let ih = sha256(&inner);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&ih);
    sha256(&outer)
}

/// Passwort → Schlüssel (absichtlich langsam)
pub fn stretch(password: &str) -> [u8; 32] {
    let mut h = sha256(format!("amboss-collab-v2|{password}").as_bytes());
    let mut buf = [0u8; 64];
    let pw = sha256(password.as_bytes());
    buf[32..].copy_from_slice(&pw);
    for _ in 0..STRETCH_ROUNDS {
        buf[..32].copy_from_slice(&h);
        h = sha256(&buf);
    }
    h
}

/// Zufällige Bytes vom Betriebssystem
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::getrandom(&mut b).is_err() {
        // Notlösung: Zeit + Adresse (nur falls das System keine Zufallszahlen liefert)
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let h = sha256(format!("{t}|{:p}|{}", &b, std::process::id()).as_bytes());
        for (i, x) in b.iter_mut().enumerate() {
            *x = h[i % 32];
        }
    }
    b
}

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

fn chacha_block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut s = [0u32; 16];
    s[0..4].copy_from_slice(&[0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]);
    for i in 0..8 {
        s[4 + i] = u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }
    s[12] = counter;
    for i in 0..3 {
        s[13 + i] = u32::from_le_bytes([nonce[i * 4], nonce[i * 4 + 1], nonce[i * 4 + 2], nonce[i * 4 + 3]]);
    }
    let mut w = s;
    for _ in 0..10 {
        quarter(&mut w, 0, 4, 8, 12);
        quarter(&mut w, 1, 5, 9, 13);
        quarter(&mut w, 2, 6, 10, 14);
        quarter(&mut w, 3, 7, 11, 15);
        quarter(&mut w, 0, 5, 10, 15);
        quarter(&mut w, 1, 6, 11, 12);
        quarter(&mut w, 2, 7, 8, 13);
        quarter(&mut w, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        out[i * 4..i * 4 + 4].copy_from_slice(&w[i].wrapping_add(s[i]).to_le_bytes());
    }
    out
}

pub fn chacha20(key: &[u8; 32], nonce: &[u8; 12], data: &mut [u8]) {
    for (i, chunk) in data.chunks_mut(64).enumerate() {
        let ks = chacha_block(key, 1 + i as u32, nonce);
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().as_bytes();
    if s.len() % 4 != 0 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for c in s.chunks(4) {
        let pad = c.iter().rev().take_while(|&&x| x == b'=').count();
        let mut n = 0u32;
        for (i, &x) in c.iter().enumerate() {
            n = (n << 6) | if i >= 4 - pad { 0 } else { val(x)? };
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Schlüssel einer Richtung
#[derive(Clone)]
pub struct Keys {
    enc: [u8; 32],
    mac: [u8; 32],
}

/// Schlüssel für (Gastgeber → Gast, Gast → Gastgeber)
pub fn session_keys(pwkey: &[u8; 32], host_nonce: &str, guest_nonce: &str) -> (Keys, Keys) {
    let k = hmac(pwkey, format!("session|{host_nonce}|{guest_nonce}").as_bytes());
    let dir = |d: &str| Keys { enc: hmac(&k, format!("{d}|enc").as_bytes()), mac: hmac(&k, format!("{d}|mac").as_bytes()) };
    (dir("h2g"), dir("g2h"))
}

fn nonce_of(ctr: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&ctr.to_le_bytes());
    n
}

/// Verschlüsselt ausgehende Zeilen
pub struct Sealer {
    keys: Keys,
    ctr: u64,
}

impl Sealer {
    pub fn new(keys: Keys) -> Self {
        Self { keys, ctr: 0 }
    }

    pub fn seal(&mut self, plain: &[u8]) -> String {
        let mut buf = Vec::with_capacity(8 + plain.len() + 16);
        buf.extend_from_slice(&self.ctr.to_le_bytes());
        buf.extend_from_slice(plain);
        chacha20(&self.keys.enc, &nonce_of(self.ctr), &mut buf[8..]);
        let tag = hmac(&self.keys.mac, &buf);
        buf.extend_from_slice(&tag[..16]);
        self.ctr += 1;
        b64_encode(&buf)
    }
}

/// Prüft und entschlüsselt eingehende Zeilen (strenge Reihenfolge)
pub struct Opener {
    keys: Keys,
    ctr: u64,
}

impl Opener {
    pub fn new(keys: Keys) -> Self {
        Self { keys, ctr: 0 }
    }

    pub fn open(&mut self, line: &str) -> Option<Vec<u8>> {
        let buf = b64_decode(line)?;
        if buf.len() < 24 {
            return None;
        }
        let (body, tag) = buf.split_at(buf.len() - 16);
        let want = hmac(&self.keys.mac, body);
        // Vergleich ohne frühen Abbruch
        if want[..16].iter().zip(tag).fold(0u8, |a, (x, y)| a | (x ^ y)) != 0 {
            return None;
        }
        let ctr = u64::from_le_bytes(body[..8].try_into().ok()?);
        if ctr != self.ctr {
            return None;
        }
        let mut plain = body[8..].to_vec();
        chacha20(&self.keys.enc, &nonce_of(ctr), &mut plain);
        self.ctr += 1;
        Some(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn chacha20_rfc8439() {
        // RFC 8439, 2.4.2
        let key: [u8; 32] = core::array::from_fn(|i| i as u8);
        let nonce: [u8; 12] = [0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0];
        let mut msg = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".to_vec();
        chacha20(&key, &nonce, &mut msg);
        assert_eq!(&msg[..16], &unhex("6e2e359a2568f98041ba0728dd0d6981")[..]);
        assert_eq!(&msg[msg.len() - 6..], &unhex("f2785e42874d")[..]);
    }

    #[test]
    fn hmac_rfc4231() {
        // RFC 4231, Testfall 2
        let h = hmac(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(h.to_vec(), unhex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"));
    }

    #[test]
    fn base64_roundtrip() {
        for n in 0..20 {
            let d: Vec<u8> = (0..n).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&d)).unwrap(), d);
        }
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn seal_open() {
        let pk = stretch("geheim");
        let (h2g, _) = session_keys(&pk, "a", "b");
        let mut s = Sealer::new(h2g.clone());
        let mut o = Opener::new(h2g);
        let l1 = s.seal(b"hallo");
        let l2 = s.seal(b"welt");
        // Reihenfolge vertauscht oder wiederholt -> abgelehnt
        assert!(Opener::new(session_keys(&pk, "a", "b").0).open(&l2).is_none());
        assert_eq!(o.open(&l1).unwrap(), b"hallo");
        assert!(o.open(&l1).is_none());
        assert_eq!(o.open(&l2).unwrap(), b"welt");
        // verändert -> abgelehnt
        let mut bad = s.seal(b"x").into_bytes();
        bad[5] = if bad[5] == b'A' { b'B' } else { b'A' };
        assert!(o.open(std::str::from_utf8(&bad).unwrap()).is_none());
        // falsches Passwort -> abgelehnt
        let (other, _) = session_keys(&stretch("falsch"), "a", "b");
        assert!(Opener::new(other).open(&Sealer::new(session_keys(&pk, "a", "b").0).seal(b"z")).is_none());
    }
}
