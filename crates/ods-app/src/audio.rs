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
    /// A boot on earth.
    Footstep,
    /// A boot on snow or scree.
    Crunch,
    /// A boot on timber.
    Knock,
    /// Something far off answers the dark. It is not answering you.
    DemonCall,
    /// The geoscape clock's soft daily tick.
    DayTick,
    /// The low drum of the clock stopping for an event.
    PauseDrum,
    /// Two rising notes: the record is written.
    SaveChime,
    /// A dull refusal.
    Error,
    /// The augurs' two-tone dread: a rift is found.
    AugurSting,
    /// The blood moon's horn.
    MoonHorn,
    /// A soldier of the Order has fallen.
    Mourning,
    /// Boots down the ramp: the mission begins.
    Deploy,
}

/// A standing background bed, one per kind of place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ambient {
    /// Birdsong and light wind — and the birds stop when they should.
    Temperate,
    /// Dry wind over open ground.
    Desert,
    /// Insects, thick air.
    Jungle,
    /// High cold wind.
    Tundra,
    /// Rain on everything.
    Rain,
    /// Driven sand hissing.
    Sandstorm,
    /// The chapterhouse: hall hum and far-off hammering.
    Halls,
    /// Thin wind at altitude, for the war table.
    HighWind,
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
    /// Every effect, in a few pitch variants so repetition never rings.
    banks: Vec<(Sound, Vec<Vec<f32>>)>,
    /// Lowpassed twins for the far-away versions of the loud things.
    muffled: Vec<(Sound, Vec<f32>)>,
    vigil: Arc<Vec<f32>>,
    warfront: Arc<Vec<f32>>,
    warfront_intense: Arc<Vec<f32>>,
    music_sink: Option<Sink>,
    intense_sink: Option<Sink>,
    playing: Option<MusicTrack>,
    ambient_sink: Option<Sink>,
    ambient_playing: Option<Ambient>,
    ambient_level: f32,
    /// Master and per-bus volumes, all 0..=1.
    volume: f32,
    pub music_volume: f32,
    pub sfx_volume: f32,
    pub ambient_volume: f32,
    /// Round-robin variant pick and the soft limiter's recent-play clock.
    variant: std::cell::Cell<u32>,
    recent: std::cell::RefCell<Vec<std::time::Instant>>,
    /// Sinks on their way out, and the ramp of the ones coming in.
    fading: Vec<(Sink, f32)>,
    ramp: f32,
}

/// Resample by a pitch factor (linear interpolation — honest enough).
fn pitched(samples: &[f32], factor: f32) -> Vec<f32> {
    let out_len = (samples.len() as f32 / factor) as usize;
    (0..out_len)
        .map(|i| {
            let x = i as f32 * factor;
            let k = x as usize;
            let f = x - k as f32;
            let a = samples.get(k).copied().unwrap_or(0.0);
            let b = samples.get(k + 1).copied().unwrap_or(0.0);
            a + (b - a) * f
        })
        .collect()
}

/// One-pole lowpass: the far-away version of a loud thing.
fn lowpassed(samples: &[f32], alpha: f32) -> Vec<f32> {
    let mut low = 0.0f32;
    samples
        .iter()
        .map(|&s| {
            low += (s - low) * alpha;
            low
        })
        .collect()
}

fn variants(base: Vec<f32>) -> Vec<Vec<f32>> {
    let a = pitched(&base, 0.93);
    let b = pitched(&base, 1.08);
    vec![base, a, b]
}

impl Audio {
    pub fn new() -> Option<Self> {
        let (stream, handle) = OutputStream::try_default().ok()?;
        let banks = vec![
            (Sound::Shot, variants(synth_shot())),
            (Sound::Blast, variants(synth_blast())),
            (Sound::Death, variants(synth_death())),
            (Sound::Dread, variants(synth_dread())),
            (Sound::Click, vec![synth_click()]),
            (Sound::Victory, vec![synth_sting(true)]),
            (Sound::Defeat, vec![synth_sting(false)]),
            (Sound::Whisper, variants(synth_whisper())),
            (Sound::Heartbeat, vec![synth_heartbeat()]),
            (Sound::Footstep, variants(synth_footstep())),
            (Sound::Crunch, variants(synth_crunch())),
            (Sound::Knock, variants(synth_knock())),
            (Sound::DemonCall, variants(synth_demon_call())),
            (Sound::DayTick, vec![synth_day_tick()]),
            (Sound::PauseDrum, vec![synth_pause_drum()]),
            (Sound::SaveChime, vec![synth_save_chime()]),
            (Sound::Error, vec![synth_error()]),
            (Sound::AugurSting, vec![synth_augur_sting()]),
            (Sound::MoonHorn, vec![synth_moon_horn()]),
            (Sound::Mourning, vec![synth_mourning()]),
            (Sound::Deploy, vec![synth_deploy()]),
        ];
        let muffled = vec![
            (Sound::Shot, lowpassed(&synth_shot(), 0.10)),
            (Sound::Blast, lowpassed(&synth_blast(), 0.08)),
        ];
        Some(Self {
            _stream: stream,
            handle,
            banks,
            muffled,
            vigil: Arc::new(synth_vigil()),
            warfront: Arc::new(synth_warfront()),
            warfront_intense: Arc::new(synth_warfront_intense()),
            music_sink: None,
            intense_sink: None,
            playing: None,
            ambient_sink: None,
            ambient_playing: None,
            ambient_level: 1.0,
            volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            ambient_volume: 1.0,
            variant: std::cell::Cell::new(0),
            recent: std::cell::RefCell::new(Vec::new()),
            fading: Vec::new(),
            ramp: 1.0,
        })
    }

    /// Master volume for everything.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.apply_bus_volumes();
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Push the current bus settings onto the standing sinks.
    pub fn apply_bus_volumes(&mut self) {
        if let Some(sink) = &self.music_sink {
            sink.set_volume(0.5 * self.volume * self.music_volume);
        }
        if let Some(sink) = &self.ambient_sink {
            sink.set_volume(0.6 * self.volume * self.ambient_volume * self.ambient_level);
        }
    }

    /// Switch the underscore of the world. None silences it.
    pub fn music(&mut self, track: Option<MusicTrack>) {
        if self.playing == track {
            return;
        }
        if let Some(sink) = self.music_sink.take() {
            let v = sink.volume();
            self.fading.push((sink, v));
        }
        if let Some(sink) = self.intense_sink.take() {
            let v = sink.volume();
            self.fading.push((sink, v));
        }
        self.ramp = 0.0; // the incoming track rises from silence
        self.playing = track;
        if let Some(track) = track {
            let data = match track {
                MusicTrack::Vigil => self.vigil.clone(),
                MusicTrack::Warfront => self.warfront.clone(),
            };
            if let Ok(sink) = Sink::try_new(&self.handle) {
                sink.set_volume(0.0);
                sink.append(LoopSource { data, pos: 0 });
                self.music_sink = Some(sink);
            }
            // The battle track carries a second, denser layer that the
            // intensity dial fades in when contact opens.
            if track == MusicTrack::Warfront
                && let Ok(sink) = Sink::try_new(&self.handle)
            {
                sink.set_volume(0.0);
                sink.append(LoopSource { data: self.warfront_intense.clone(), pos: 0 });
                self.intense_sink = Some(sink);
            }
        }
    }

    /// 0 = quiet field, 1 = open contact: fades the intense layer.
    pub fn set_intensity(&mut self, x: f32) {
        if let Some(sink) = &self.intense_sink {
            sink.set_volume(0.45 * x.clamp(0.0, 1.0) * self.volume * self.music_volume);
        }
    }

    /// Switch the standing background bed. None silences it.
    pub fn ambient(&mut self, bed: Option<Ambient>) {
        if self.ambient_playing == bed {
            return;
        }
        if let Some(sink) = self.ambient_sink.take() {
            let v = sink.volume();
            self.fading.push((sink, v));
        }
        self.ambient_playing = bed;
        if let Some(bed) = bed
            && let Ok(sink) = Sink::try_new(&self.handle)
        {
            sink.set_volume(0.0);
            sink.append(LoopSource { data: Arc::new(synth_ambient(bed)), pos: 0 });
            self.ambient_sink = Some(sink);
        }
    }

    /// Hush the bed (the birds know before you do): 0..=1.
    pub fn set_ambient_level(&mut self, level: f32) {
        let level = level.clamp(0.0, 1.0);
        if (level - self.ambient_level).abs() > 0.01 {
            self.ambient_level = level;
            self.apply_bus_volumes();
        }
    }

    /// Per-frame housekeeping: old tracks fade out, new ones ramp in.
    pub fn tick(&mut self, dt: f32) {
        for (sink, v) in &mut self.fading {
            *v -= dt * 0.8;
            sink.set_volume(v.max(0.0));
        }
        self.fading.retain(|(sink, v)| {
            if *v <= 0.0 {
                sink.stop();
                false
            } else {
                true
            }
        });
        if self.ramp < 1.0 {
            self.ramp = (self.ramp + dt * 0.7).min(1.0);
        }
        if let Some(sink) = &self.music_sink {
            sink.set_volume(0.5 * self.volume * self.music_volume * self.ramp);
        }
        if let Some(sink) = &self.ambient_sink {
            sink.set_volume(
                0.6 * self.volume * self.ambient_volume * self.ambient_level * self.ramp,
            );
        }
    }

    /// The soft limiter: many voices at once duck each other instead of
    /// clipping into crackle.
    fn limited(&self, gain: f32) -> f32 {
        let now = std::time::Instant::now();
        let mut recent = self.recent.borrow_mut();
        recent.retain(|t| now.duration_since(*t).as_millis() < 70);
        recent.push(now);
        gain / (1.0 + 0.3 * (recent.len().saturating_sub(1)) as f32)
    }

    fn pick(&self, sound: Sound) -> Option<&Vec<f32>> {
        let (_, set) = self.banks.iter().find(|(s, _)| *s == sound)?;
        let n = self.variant.get().wrapping_add(1);
        self.variant.set(n);
        set.get(n as usize % set.len())
    }

    pub fn play(&self, sound: Sound) {
        if self.volume <= 0.0 || self.sfx_volume <= 0.0 {
            return;
        }
        if let Some(samples) = self.pick(sound) {
            let g = self.limited(1.0) * self.volume * self.sfx_volume;
            let buffer = SamplesBuffer::new(1, RATE, samples.clone());
            let _ = self.handle.play_raw(buffer.amplify(g));
        }
    }

    /// Play a sound from somewhere: `gain` (0..=1) carries distance,
    /// `pan` (-1 left ..= 1 right) carries direction — equal-power panned
    /// into a stereo buffer so the field tells you where to look. Far-off
    /// loud things arrive through their lowpassed twins.
    pub fn play_at(&self, sound: Sound, gain: f32, pan: f32) {
        if self.volume <= 0.0 || self.sfx_volume <= 0.0 {
            return;
        }
        let far = gain < 0.45;
        let muffled = far
            .then(|| self.muffled.iter().find(|(s, _)| *s == sound).map(|(_, v)| v))
            .flatten();
        let Some(samples) = muffled.or_else(|| self.pick(sound)) else {
            return;
        };
        let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let (l, r) = (theta.cos(), theta.sin());
        let g = self.limited(gain.clamp(0.0, 1.0)) * self.volume * self.sfx_volume;
        let mut stereo = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            stereo.push(s * l * g);
            stereo.push(s * r * g);
        }
        let _ = self.handle.play_raw(SamplesBuffer::new(2, RATE, stereo));
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

/// A soft thud: lowpassed noise, gone in a breath.
fn synth_footstep() -> Vec<f32> {
    let len = (RATE as f32 * 0.05) as usize;
    let env = envelope(len, 0.05, 14.0);
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            low += (noise(i) - low) * 0.18;
            low * env(i) * 0.5
        })
        .collect()
}

/// A brighter, grittier step: snow, scree, frost.
fn synth_crunch() -> Vec<f32> {
    let len = (RATE as f32 * 0.06) as usize;
    let env = envelope(len, 0.03, 10.0);
    (0..len).map(|i| noise(i) * env(i) * 0.28).collect()
}

/// A hollow knock: boot on planking.
fn synth_knock() -> Vec<f32> {
    let len = (RATE as f32 * 0.08) as usize;
    let env = envelope(len, 0.02, 16.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (t * 170.0 * std::f32::consts::TAU).sin() * env(i) * 0.4
        })
        .collect()
}

/// A far shriek that falls and frays: the dark, answering itself.
fn synth_demon_call() -> Vec<f32> {
    let len = (RATE as f32 * 0.8) as usize;
    let env = envelope(len, 0.15, 3.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 640.0 - t * 420.0 + (t * 13.0 * std::f32::consts::TAU).sin() * 40.0;
            ((t * f * std::f32::consts::TAU).sin() + noise(i) * 0.15) * env(i) * 0.2
        })
        .collect()
}

fn synth_click() -> Vec<f32> {
    let len = (RATE as f32 * 0.02) as usize;
    let env = envelope(len, 0.1, 12.0);
    (0..len).map(|i| noise(i) * env(i) * 0.2).collect()
}

// ---------------------------------------------------------------- UI voices

/// The geoscape clock's soft daily tick.
fn synth_day_tick() -> Vec<f32> {
    let len = (RATE as f32 * 0.03) as usize;
    let env = envelope(len, 0.05, 18.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (t * 660.0 * std::f32::consts::TAU).sin() * env(i) * 0.12
        })
        .collect()
}

/// A single low drum: the world stops for an answer.
fn synth_pause_drum() -> Vec<f32> {
    let len = (RATE as f32 * 0.35) as usize;
    let env = envelope(len, 0.01, 9.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 70.0 - t * 30.0;
            ((t * f * std::f32::consts::TAU).sin() + noise(i) * 0.08) * env(i) * 0.5
        })
        .collect()
}

/// Two rising notes: the record is written.
fn synth_save_chime() -> Vec<f32> {
    let len = (RATE as f32 * 0.32) as usize;
    let env = envelope(len, 0.02, 6.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = if t < 0.14 { 520.0 } else { 690.0 };
            (t * f * std::f32::consts::TAU).sin() * env(i) * 0.16
        })
        .collect()
}

/// A dull refusal.
fn synth_error() -> Vec<f32> {
    let len = (RATE as f32 * 0.12) as usize;
    let env = envelope(len, 0.02, 12.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (t * 130.0 * std::f32::consts::TAU).sin().signum() * env(i) * 0.12
        })
        .collect()
}

// ----------------------------------------------------------------- stingers

/// The augurs' two-tone dread: something has been found.
fn synth_augur_sting() -> Vec<f32> {
    let len = (RATE as f32 * 0.9) as usize;
    let env = envelope(len, 0.05, 3.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = if t < 0.35 { 233.0 } else { 220.0 }; // a half-step down: wrongness
            ((t * f * std::f32::consts::TAU).sin()
                + (t * f * 2.02 * std::f32::consts::TAU).sin() * 0.3)
                * env(i)
                * 0.22
        })
        .collect()
}

/// The blood moon's horn: long, low, and final.
fn synth_moon_horn() -> Vec<f32> {
    let len = (RATE as f32 * 1.8) as usize;
    let env = envelope(len, 0.25, 1.4);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            ((t * 87.3 * std::f32::consts::TAU).sin()
                + (t * 88.1 * std::f32::consts::TAU).sin()
                + (t * 130.8 * std::f32::consts::TAU).sin() * 0.4)
                * env(i)
                * 0.16
        })
        .collect()
}

/// A soldier of the Order has fallen: three descending tolls.
fn synth_mourning() -> Vec<f32> {
    let len = (RATE as f32 * 1.2) as usize;
    let env = envelope(len, 0.02, 2.2);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = match (t * 2.6) as usize {
                0 => 294.0,
                1 => 262.0,
                _ => 220.0,
            };
            ((t * f * std::f32::consts::TAU).sin()
                + (t * f * 2.0 * std::f32::consts::TAU).sin() * 0.2)
                * env(i)
                * 0.2
        })
        .collect()
}

/// Boots down the ramp.
fn synth_deploy() -> Vec<f32> {
    let len = (RATE as f32 * 0.5) as usize;
    let env = envelope(len, 0.01, 5.0);
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let beat = if ((t * 8.0) as usize).is_multiple_of(2) { 1.0 } else { 0.4 };
            ((t * 98.0 * std::f32::consts::TAU).sin() * beat + noise(i) * 0.1) * env(i) * 0.35
        })
        .collect()
}

// ----------------------------------------------------- the intense layer

/// The Warfront's second skin: same pulse, doubled, with a high worry line.
/// Mixed in by `set_intensity` while demons stand in view.
fn synth_warfront_intense() -> Vec<f32> {
    let len = RATE as usize * 6;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let beat = t % 0.75;
            let pulse = if beat < 0.14 {
                (beat / 0.14 * std::f32::consts::PI).sin()
                    * (t * 140.0 * std::f32::consts::TAU).sin()
            } else {
                0.0
            };
            let worry = (t * 466.2 * std::f32::consts::TAU).sin()
                * (0.5 + 0.5 * (t * 0.9 * std::f32::consts::TAU).sin())
                * 0.05;
            pulse * 0.2 + worry
        })
        .collect()
}

// ----------------------------------------------------------- ambient beds

/// Eight-second loops of place. Every bed is quiet by design: it sits
/// under the effects, not beside them.
fn synth_ambient(bed: Ambient) -> Vec<f32> {
    let len = RATE as usize * 8;
    let mut low = 0.0f32;
    let mut mid = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // Wind: slow-breathing filtered noise, the base of everything.
            low += (noise(i) - low) * 0.02;
            mid += (noise(i.wrapping_add(9999)) - mid) * 0.08;
            let breath = 0.6 + 0.4 * (t / 8.0 * std::f32::consts::TAU).sin();
            let wind = low * breath;
            match bed {
                Ambient::Temperate => {
                    // Light wind plus sparse birdsong chirps.
                    let cycle = (t * 1.7) % 1.0;
                    let sing = ((t * 0.5) as usize).is_multiple_of(3) && cycle < 0.06;
                    let chirp = if sing {
                        let ct = cycle / 0.06;
                        (ct * (2600.0 + 700.0 * (t * 2.0).sin()) * std::f32::consts::TAU)
                            .sin()
                            * (1.0 - ct)
                            * 0.05
                    } else {
                        0.0
                    };
                    wind * 0.10 + chirp
                }
                Ambient::Desert => wind * 0.16 + mid * 0.02,
                Ambient::Jungle => {
                    // Insect shimmer over damp air.
                    let shimmer = (t * 4200.0 * std::f32::consts::TAU).sin()
                        * (0.5 + 0.5 * (t * 9.0 * std::f32::consts::TAU).sin()).powi(3)
                        * 0.025;
                    wind * 0.08 + shimmer + mid * 0.015
                }
                Ambient::Tundra => wind * 0.2,
                Ambient::Rain => mid * 0.11 + wind * 0.05,
                Ambient::Sandstorm => mid * 0.14 + wind * 0.12,
                Ambient::Halls => {
                    // A deep hum, and far-off hammering on the quarter.
                    let hum = (t * 55.0 * std::f32::consts::TAU).sin() * 0.03;
                    let beat = t % 2.0;
                    let hammer = if beat < 0.05 {
                        (beat / 0.05 * std::f32::consts::PI).sin() * 0.05
                    } else {
                        0.0
                    };
                    hum + hammer + wind * 0.03
                }
                Ambient::HighWind => wind * 0.13 + mid * 0.01,
            }
        })
        .collect()
}
