//! Procedurally synthesized sound. No asset files: every effect is a few
//! lines of DSP, generated at startup. Degrades to silence when no audio
//! device exists (CI, cloud sessions).

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle};

const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    Shot,
    Blast,
    Death,
    Dread,
    Click,
}

pub struct Audio {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    banks: Vec<(Sound, Vec<f32>)>,
}

impl Audio {
    pub fn new() -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        let banks = vec![
            (Sound::Shot, synth_shot()),
            (Sound::Blast, synth_blast()),
            (Sound::Death, synth_death()),
            (Sound::Dread, synth_dread()),
            (Sound::Click, synth_click()),
        ];
        Some(Self { _stream: stream, handle, banks })
    }

    pub fn play(&self, sound: Sound) {
        if let Some((_, samples)) = self.banks.iter().find(|(s, _)| *s == sound) {
            let buffer = SamplesBuffer::new(1, RATE, samples.clone());
            let _ = self.handle.play_raw(buffer);
        }
    }
}

fn envelope(len: usize, attack: f32, decay: f32) -> impl Fn(usize) -> f32 {
    let n = len as f32;
    move |i| {
        let t = i as f32 / n;
        let a = (t / attack.max(1e-3)).min(1.0);
        a * (-t * decay).exp()
    }
}

/// A pseudo-random click generator (deterministic; audio needn't be fancy).
fn noise(i: usize) -> f32 {
    let x = (i as u32).wrapping_mul(0x9E37_79B9).wrapping_add(0x85EB_CA6B);
    let x = (x ^ (x >> 15)).wrapping_mul(0x2C1B_3C6D);
    ((x >> 8) & 0xFFFF) as f32 / 32768.0 - 1.0
}

fn synth_shot() -> Vec<f32> {
    let len = (RATE as f32 * 0.09) as usize;
    let env = envelope(len, 0.01, 8.0);
    (0..len).map(|i| noise(i) * env(i) * 0.5).collect()
}

fn synth_blast() -> Vec<f32> {
    let len = (RATE as f32 * 0.5) as usize;
    let env = envelope(len, 0.02, 5.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let boom = (t * 55.0 * std::f32::consts::TAU).sin() * 0.7;
            (boom + noise(i) * 0.3) * env(i) * 0.8
        })
        .collect()
}

fn synth_death() -> Vec<f32> {
    let len = (RATE as f32 * 0.35) as usize;
    let env = envelope(len, 0.05, 4.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 220.0 - t * 320.0; // a falling tone
            (t * f * std::f32::consts::TAU).sin().signum() * env(i) * 0.25
        })
        .collect()
}

fn synth_dread() -> Vec<f32> {
    let len = (RATE as f32 * 0.6) as usize;
    let env = envelope(len, 0.2, 2.5);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let warble = 90.0 + (t * 7.0 * std::f32::consts::TAU).sin() * 25.0;
            (t * warble * std::f32::consts::TAU).sin() * env(i) * 0.3
        })
        .collect()
}

fn synth_click() -> Vec<f32> {
    let len = (RATE as f32 * 0.02) as usize;
    let env = envelope(len, 0.1, 12.0);
    (0..len).map(|i| noise(i) * env(i) * 0.2).collect()
}
