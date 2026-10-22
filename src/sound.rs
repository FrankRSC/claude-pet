//! Sonidos: el grito de cada Pokémon (`cries/{id}.ogg` junto a skins.json,
//! lo descarga tools/pokedex.py) o, para el Monito y los avatares sin grito,
//! un sonido del sistema. El grito se decodifica una vez (Ogg Vorbis con
//! lewton), se guarda en memoria y se toca como WAV con el volumen ya aplicado.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{platform, skins, sprites};

/// Para qué suena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// Aparece una sesión nueva.
    Appear,
    /// Terminó una tarea.
    Done,
    /// Necesita tu permiso o atención.
    Attention,
    /// Nació una cría (subagente): más bajito.
    Kit,
}

impl Cue {
    fn volume(self) -> f32 {
        match self {
            Cue::Kit => 0.3,
            _ => 0.65,
        }
    }

    /// Sonido del sistema cuando no hay grito.
    pub fn system(self) -> platform::SystemSound {
        match self {
            Cue::Appear | Cue::Kit => platform::SystemSound::Pop,
            Cue::Done => platform::SystemSound::Done,
            Cue::Attention => platform::SystemSound::Alert,
        }
    }
}

/// Mínimo entre dos sonidos: varias crías o hooks juntos no hacen coro.
const GAP: Duration = Duration::from_millis(1500);

static ENABLED: AtomicBool = AtomicBool::new(true);
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

/// Muestras ya decodificadas de cada grito (o None si no hay archivo).
type Pcm = Arc<(u16, u32, Vec<i16>)>;
static CRIES: Mutex<Option<HashMap<u32, Option<Pcm>>>> = Mutex::new(None);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Toca el sonido de `cue` para el avatar `index`, salvo que estén apagados
/// o haya sonado otro hace menos de 1.5 s.
pub fn play(index: usize, cue: Cue) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    {
        let mut last = LAST.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < GAP) {
            return;
        }
        *last = Some(Instant::now());
    }
    let id = sprites::id(index);
    match id.and_then(cry) {
        Some(pcm) => {
            let ok = platform::play_wav(wav(&pcm, cue.volume()));
            crate::app::debug_log(format_args!("sonido {cue:?}: grito #{} ({})", id.unwrap_or(0), if ok { "suena" } else { "falló" }));
        }
        None => {
            platform::play_system(cue.system());
            crate::app::debug_log(format_args!("sonido {cue:?}: del sistema"));
        }
    }
}

/// Olvida los gritos en memoria (llegó un skins.json nuevo).
pub fn forget() {
    *CRIES.lock().unwrap() = None;
}

/// El grito del Pokémon `id`, decodificado una sola vez.
pub fn cry(id: u32) -> Option<Pcm> {
    let mut cache = CRIES.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    cache
        .entry(id)
        .or_insert_with(|| {
            let path = skins::dir()?.join("cries").join(format!("{id}.ogg"));
            let bytes = std::fs::read(path).ok()?;
            decode(&bytes).map(Arc::new)
        })
        .clone()
}

/// (canales, muestras por segundo, muestras intercaladas) de un Ogg Vorbis.
pub fn decode(ogg: &[u8]) -> Option<(u16, u32, Vec<i16>)> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg)).ok()?;
    let channels = reader.ident_hdr.audio_channels as u16;
    let rate = reader.ident_hdr.audio_sample_rate;
    let mut samples = Vec::new();
    while let Ok(Some(packet)) = reader.read_dec_packet_itl() {
        samples.extend(packet);
    }
    (!samples.is_empty() && channels > 0).then_some((channels, rate, samples))
}

/// Cuánto dura un sonido decodificado.
pub fn duration(pcm: &(u16, u32, Vec<i16>)) -> Duration {
    let (channels, rate, samples) = pcm;
    Duration::from_secs_f32(samples.len() as f32 / (*channels as f32 * *rate as f32).max(1.0))
}

/// WAV PCM de 16 bits en memoria, con el volumen aplicado a las muestras
/// (PlaySound de Windows no tiene control de volumen).
pub fn wav((channels, rate, samples): &(u16, u32, Vec<i16>), volume: f32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(channels.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * *channels as u32 * 2).to_le_bytes());
    out.extend((*channels * 2).to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for &s in samples {
        out.extend(((s as f32 * volume) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let w = wav(&(1, 22050, vec![1000, -1000, 0]), 0.5);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize, w.len() - 8);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 22050);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 6);
        assert_eq!(i16::from_le_bytes([w[44], w[45]]), 500, "volumen aplicado");
    }

    /// Decodifica un grito real si tools/.cache tiene alguno (lo baja pokedex.py).
    #[test]
    fn decodes_a_cached_cry() {
        let cache = concat!(env!("CARGO_MANIFEST_DIR"), "/tools/.cache");
        let Some(file) = std::fs::read_dir(cache)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "ogg"))
        else {
            return;
        };
        let pcm = decode(&std::fs::read(&file).unwrap()).expect("Ogg Vorbis válido");
        let secs = duration(&pcm).as_secs_f32();
        assert!((0.2..5.0).contains(&secs), "{}: {secs} s", file.display());
        assert!(decode(b"no es ogg").is_none());
    }
}
