//! Sounds abspielen (Sound-Browser). Unter Windows über rodio (WASAPI), sonst nur Anzeige.

use std::time::Instant;

/// Länge einer WAV-Datei in Sekunden (aus dem Kopf), sonst None
pub fn wav_duration(b: &[u8]) -> Option<f32> {
    if b.get(0..4)? != b"RIFF" || b.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut p = 12;
    let mut byte_rate = 0u32;
    let mut samples: Option<u32> = None;
    while p + 8 <= b.len() {
        let id = &b[p..p + 4];
        let size = u32::from_le_bytes(b[p + 4..p + 8].try_into().ok()?) as usize;
        let body = p + 8;
        match id {
            b"fmt " => byte_rate = u32::from_le_bytes(b.get(body + 8..body + 12)?.try_into().ok()?),
            // ADPCM: Anzahl Samples steht im fact-Block
            b"fact" => samples = Some(u32::from_le_bytes(b.get(body..body + 4)?.try_into().ok()?)),
            b"data" => {
                let rate = u32::from_le_bytes(b.get(24..28)?.try_into().ok()?);
                if let Some(n) = samples {
                    return (rate > 0).then(|| n as f32 / rate as f32);
                }
                return (byte_rate > 0).then(|| size as f32 / byte_rate as f32);
            }
            _ => {}
        }
        p = body + size + (size & 1);
    }
    None
}

pub struct Player {
    #[cfg(windows)]
    out: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    #[cfg(windows)]
    sink: Option<rodio::Sink>,
    pub playing: Option<String>,
    pub started: Option<Instant>,
    pub duration: Option<f32>,
    pub volume: f32,
}

impl Default for Player {
    fn default() -> Self {
        Player {
            #[cfg(windows)]
            out: None,
            #[cfg(windows)]
            sink: None,
            playing: None,
            started: None,
            duration: None,
            volume: 0.7,
        }
    }
}

impl Player {
    pub fn available() -> bool {
        cfg!(windows)
    }

    /// Datei abspielen (Bytes aus dem Spiel-Dateisystem)
    pub fn play(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), String> {
        self.stop();
        self.duration = wav_duration(&bytes);
        #[cfg(windows)]
        {
            if self.out.is_none() {
                self.out = Some(rodio::OutputStream::try_default().map_err(|e| format!("Keine Audioausgabe: {e}"))?);
            }
            let handle = &self.out.as_ref().unwrap().1;
            let sink = rodio::Sink::try_new(handle).map_err(|e| e.to_string())?;
            let src = rodio::Decoder::new(std::io::Cursor::new(bytes)).map_err(|e| format!("{name}: {e}"))?;
            if self.duration.is_none() {
                self.duration = rodio::Source::total_duration(&src).map(|d| d.as_secs_f32());
            }
            sink.set_volume(self.volume);
            sink.append(src);
            self.sink = Some(sink);
        }
        #[cfg(not(windows))]
        {
            let _ = (name, bytes);
            return Err("Abspielen ist in dieser Version nur unter Windows verfügbar".into());
        }
        #[allow(unreachable_code)]
        {
            self.playing = Some(name.to_string());
            self.started = Some(Instant::now());
            Ok(())
        }
    }

    pub fn stop(&mut self) {
        #[cfg(windows)]
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.playing = None;
        self.started = None;
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        #[cfg(windows)]
        if let Some(s) = &self.sink {
            s.set_volume(self.volume);
        }
    }

    /// Fertig abgespielt? Dann Zustand zurücksetzen.
    pub fn poll(&mut self) {
        #[cfg(windows)]
        if self.sink.as_ref().is_some_and(|s| s.empty()) {
            self.stop();
        }
    }

    /// (vergangen, gesamt) in Sekunden
    pub fn progress(&self) -> Option<(f32, Option<f32>)> {
        let t = self.started?.elapsed().as_secs_f32();
        Some((self.duration.map_or(t, |d| t.min(d)), self.duration))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_length() {
        // 1 Sekunde, 22050 Hz, 16 Bit mono
        let rate = 22050u32;
        let data = vec![0u8; (rate * 2) as usize];
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        assert!((wav_duration(&b).unwrap() - 1.0).abs() < 1e-3);
        assert!(wav_duration(b"nope").is_none());
    }
}
