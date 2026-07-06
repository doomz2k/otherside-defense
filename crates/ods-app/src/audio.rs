//! Procedurally synthesized sound. No asset files: every effect is a few
//! lines of DSP, generated at startup. Degrades to silence when no audio
//! device exists (CI, cloud sessions).

use std::sync::Arc;

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

const RATE: u32 = 22_050;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    Shot,
    Blast,
    Death,
    Dread,
    Click,
    Victory,
    Defeat,
    /// Something on the edge of hearing, saying your name.
    Whisper,
    /// Your own pulse, too loud: the squad is bleeding out.
    Heartbeat,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MusicTrack {
    /// The geoscape drone: the world holding its breath.
    Vigil,
    /// The battle pulse: sparse, low, patient.
    Warfront,
}

/// An endlessly cycling sample loop.
struct LoopSource {
    data: Arc<Vec<f32>>,
    pos: usize,
}

impl Iterator for LoopSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.data[self.pos];
        self.pos = (self.pos + 1) % self.data.len();
        Some(s)
    }
}

impl Source for LoopSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

pub struct Audio {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    banks: Vec<(Sound, Vec<f32>)>,
    vigil: Arc<Vec<f32>>,
    warfront: Arc<Vec<f32>>,
    music_sink: Option<Sink>,
    playing: Option<MusicTrack>,
    /// Master volume, 0..=1, scaling both effects and music.
    volume: f32,
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
            (Sound::Victory, synth_sting(true)),
            (Sound::Defeat, synth_sting(false)),
            (Sound::Whisper, synth_whisper()),
            (Sound::Heartbeat, synth_heartbeat()),
        ];
        Some(Self {
            _stream: stream,
            handle,
            banks,
            vigil: Arc::new(synth_vigil()),
            warfront: Arc::new(synth_warfront()),
            music_sink: None,
            playing: None,
            volume: 1.0,
        })
    }

    /// Master volume for effects and music alike.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.music_sink {
            sink.set_volume(0.5 * self.volume);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Switch the underscore of the world. None silences it.
    pub fn music(&mut self, track: Option<MusicTrack>) {
        if self.playing == track {
            return;
        }
        if let Some(sink) = self.music_sink.take() {
            sink.stop();
        }
        self.playing = track;
        if let Some(track) = track {
            let data = match track {
                MusicTrack::Vigil => self.vigil.clone(),
                MusicTrack::Warfront => self.warfront.clone(),
            };
            if let Ok(sink) = Sink::try_new(&self.handle) {
                sink.set_volume(0.5 * self.volume);
                sink.append(LoopSource { data, pos: 0 });
                self.music_sink = Some(sink);
            }
        }
    }

    pub fn play(&self, sound: Sound) {
        if self.volume <= 0.0 {
            return;
        }
        if let Some((_, samples)) = self.banks.iter().find(|(s, _)| *s == sound) {
            let buffer = SamplesBuffer::new(1, RATE, samples.clone());
            let _ = self.handle.play_raw(buffer.amplify(self.volume));
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

fn synth_sting(victory: bool) -> Vec<f32> {
    let len = (RATE as f32 * 1.4) as usize;
    let env = envelope(len, 0.05, 1.6);
    let steps: [f32; 3] = if victory { [220.0, 277.2, 329.6] } else { [220.0, 207.7, 164.8] };
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = steps[((t * 2.5) as usize).min(2)];
            ((t * f * std::f32::consts::TAU).sin() + (t * f * 0.5 * std::f32::consts::TAU).sin() * 0.4)
                * env(i)
                * 0.3
        })
        .collect()
}

/// Eight seconds of low detuned drone: the geoscape holding its breath.
fn synth_vigil() -> Vec<f32> {
    let len = RATE as usize * 8;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let swell = 0.55 + 0.45 * (t / 8.0 * std::f32::consts::TAU).sin();
            let a = (t * 55.0 * std::f32::consts::TAU).sin();
            let b = (t * 55.7 * std::f32::consts::TAU).sin();
            let c = (t * 82.4 * std::f32::consts::TAU).sin() * 0.5;
            (a + b + c) * 0.09 * swell
        })
        .collect()
}

/// Six seconds of sparse pulse for the battlefield.
fn synth_warfront() -> Vec<f32> {
    let len = RATE as usize * 6;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let beat = t % 1.5;
            let pulse = if beat < 0.18 {
                (beat / 0.18 * std::f32::consts::PI).sin() * (t * 70.0 * std::f32::consts::TAU).sin()
            } else {
                0.0
            };
            let bed = (t * 41.2 * std::f32::consts::TAU).sin() * 0.05;
            pulse * 0.22 + bed
        })
        .collect()
}

/// Breathy filtered noise that swells and dies: a voice with no words.
fn synth_whisper() -> Vec<f32> {
    let len = (RATE as f32 * 0.9) as usize;
    let env = envelope(len, 0.35, 3.0);
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // One-pole lowpass over noise, wobbled for a syllabic feel.
            low += (noise(i) - low) * 0.12;
            let syllable = 0.6 + 0.4 * (t * 9.0 * std::f32::consts::TAU).sin();
            low * syllable * env(i) * 0.4
        })
        .collect()
}

/// A double thump: lub-dub, low and close.
fn synth_heartbeat() -> Vec<f32> {
    let len = (RATE as f32 * 0.5) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let beat = |at: f32, gain: f32| -> f32 {
                let dt = t - at;
                if !(0.0..=0.12).contains(&dt) {
                    0.0
                } else {
                    (dt * 48.0 * std::f32::consts::TAU).sin() * (-dt * 30.0).exp() * gain
                }
            };
            (beat(0.0, 0.5) + beat(0.22, 0.35)).clamp(-1.0, 1.0)
        })
        .collect()
}

fn synth_click() -> Vec<f32> {
    let len = (RATE as f32 * 0.02) as usize;
    let env = envelope(len, 0.1, 12.0);
    (0..len).map(|i| noise(i) * env(i) * 0.2).collect()
}
