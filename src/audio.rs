//! All sounds are synthesized at startup, so there are no asset files to ship.

use macroquad::audio::{load_sound_from_bytes, play_sound, set_sound_volume, PlaySoundParams, Sound};
use std::f32::consts::TAU;

const RATE: u32 = 44_100;

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
    }
    b
}

struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// One-pole low-pass run twice over a looping buffer so the end joins the start.
fn lowpass_loop(x: &mut [f32], cutoff: f32) {
    let a = 1.0 - (-TAU * cutoff / RATE as f32).exp();
    let mut y = 0.0;
    for pass in 0..2 {
        for s in x.iter_mut() {
            y += a * (*s - y);
            if pass == 1 {
                *s = y;
            }
        }
    }
}

fn normalize(x: &mut [f32], peak: f32) {
    let m = x.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    x.iter_mut().for_each(|s| *s *= peak / m);
}

/// Engine drone: a firing pulse train at `f0` with a sub-harmonic and grit.
/// One second long with integer frequencies, so it loops seamlessly.
fn engine(f0: f32, cutoff: f32, seed: u32) -> Vec<f32> {
    let n = RATE as usize;
    let mut rng = Noise(seed);
    let mut x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let ph = (t * f0).fract();
            let pulse = (-ph * 7.0).exp() - 0.14;
            let sub = (TAU * f0 * 0.5 * t).sin() * 0.35;
            let prop = (TAU * f0 * 2.0 * t).sin() * 0.12;
            pulse + sub + prop + rng.next() * 0.25
        })
        .collect();
    lowpass_loop(&mut x, cutoff);
    normalize(&mut x, 0.9);
    x
}

fn wind() -> Vec<f32> {
    let mut rng = Noise(777);
    let mut x: Vec<f32> = (0..RATE as usize * 2).map(|_| rng.next()).collect();
    lowpass_loop(&mut x, 700.0);
    let mut lo = x.clone();
    lowpass_loop(&mut lo, 90.0);
    x.iter_mut().zip(lo).for_each(|(s, l)| *s -= l * 0.8);
    normalize(&mut x, 0.8);
    x
}

fn horn() -> Vec<f32> {
    (0..RATE as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 440.0;
            ((TAU * f * t).sin() + 0.4 * (TAU * 2.0 * f * t).sin() + 0.15 * (TAU * 3.0 * f * t).sin()) * 0.5
        })
        .collect()
}

fn tones(notes: &[(f32, f32, f32)], len: f32) -> Vec<f32> {
    let n = (len * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            notes
                .iter()
                .filter(|(_, start, _)| t >= *start)
                .map(|(f, start, decay)| {
                    let dt = t - start;
                    let env = (-dt / decay).exp() * (dt * 200.0).min(1.0);
                    ((TAU * f * dt).sin() + 0.3 * (TAU * 2.0 * f * dt).sin()) * env * 0.4
                })
                .sum()
        })
        .collect()
}

fn crash() -> Vec<f32> {
    let mut rng = Noise(4242);
    let n = (1.6 * RATE as f32) as usize;
    let mut x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let thump = (TAU * 45.0 * t).sin() * (-t * 5.0).exp();
            rng.next() * (-t * 2.2).exp() + thump * 1.5
        })
        .collect();
    let a = 1.0 - (-TAU * 1200.0 / RATE as f32).exp();
    let mut y = 0.0;
    for s in x.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
    normalize(&mut x, 0.9);
    x
}

fn squeak() -> Vec<f32> {
    let mut rng = Noise(99);
    let n = (0.28 * RATE as f32) as usize;
    let mut ph = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            ph += TAU * (1250.0 - t * 1200.0) / RATE as f32;
            (ph.sin() * 0.6 + rng.next() * 0.25) * (-t * 12.0).exp() * (t * 300.0).min(1.0)
        })
        .collect()
}

pub struct Audio {
    eng_lo: Sound,
    eng_hi: Sound,
    wind: Sound,
    horn: Sound,
    chime: Sound,
    ding: Sound,
    crash: Sound,
    squeak: Sound,
    vol: [f32; 4],
    pub muted: bool,
}

async fn load(samples: Vec<f32>) -> Sound {
    load_sound_from_bytes(&wav(&samples)).await.expect("synthesized sound")
}

impl Audio {
    pub async fn new() -> Self {
        let a = Audio {
            eng_lo: load(engine(26.0, 260.0, 1)).await,
            eng_hi: load(engine(80.0, 900.0, 2)).await,
            wind: load(wind()).await,
            horn: load(horn()).await,
            chime: load(tones(&[(784.0, 0.0, 0.25), (988.0, 0.08, 0.25), (1175.0, 0.16, 0.4)], 0.9)).await,
            ding: load(tones(&[(880.0, 0.0, 0.3), (1320.0, 0.1, 0.35)], 0.7)).await,
            crash: load(crash()).await,
            squeak: load(squeak()).await,
            vol: [0.0; 4],
            muted: false,
        };
        for s in [&a.eng_lo, &a.eng_hi, &a.wind, &a.horn] {
            play_sound(s, PlaySoundParams { looped: true, volume: 0.0 });
        }
        a
    }

    /// Update the looping sounds. `engine` is the throttle (or None for silence).
    pub fn update(&mut self, engine: Option<f32>, airspeed_kt: f32, horn: bool, dt: f32) {
        let on = !self.muted;
        let (lo, hi) = match engine {
            Some(t) if on => (0.30 * (1.0 - t) + 0.08, 0.42 * t + 0.03),
            _ => (0.0, 0.0),
        };
        let wind = if on { ((airspeed_kt - 25.0) / 110.0).clamp(0.0, 1.0) * 0.4 } else { 0.0 };
        let horn = if on && horn { 0.22 } else { 0.0 };
        let target = [lo, hi, wind, horn];
        let k = (dt * 8.0).min(1.0);
        let sounds = [&self.eng_lo, &self.eng_hi, &self.wind, &self.horn];
        for i in 0..4 {
            let v = if i == 3 { target[i] } else { self.vol[i] + (target[i] - self.vol[i]) * k };
            if (v - self.vol[i]).abs() > 0.002 || (v == 0.0 && self.vol[i] != 0.0) {
                set_sound_volume(sounds[i], v);
            }
            self.vol[i] = v;
        }
    }

    fn once(&self, s: &Sound, volume: f32) {
        if !self.muted {
            play_sound(s, PlaySoundParams { looped: false, volume });
        }
    }
    pub fn chime(&self) {
        self.once(&self.chime, 0.6);
    }
    pub fn ding(&self) {
        self.once(&self.ding, 0.5);
    }
    pub fn crash(&self) {
        self.once(&self.crash, 0.8);
    }
    pub fn squeak(&self) {
        self.once(&self.squeak, 0.5);
    }
}
