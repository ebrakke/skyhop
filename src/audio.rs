//! Real-time synthesized sound. An audio thread renders the engine, prop, wind, gear and the
//! little musical cues from a handful of parameters the game sets every frame, so the engine
//! pitch follows the RPM continuously and nothing ships as a sample file.

use std::f32::consts::{PI, TAU};
use crate::world::smoothstep;
use std::sync::{Arc, Mutex};
use tinyaudio::{run_output_device, OutputDevice, OutputDeviceParameters};

const RATE: f32 = 44_100.0;
const BLOCK: usize = 512;

/// What the sound engine needs to know, set once per frame.
#[derive(Clone, Copy)]
pub struct State {
    /// Engine RPM; 0 means the engine is stopped.
    pub rpm: f32,
    pub throttle: f32,
    /// True airspeed, m/s.
    pub airspeed: f32,
    /// Speed over the ground while rolling on the wheels, m/s (0 in the air).
    pub roll_speed: f32,
    /// 0..1, how close the wing is to stalling (shudder and rumble).
    pub buffet: f32,
    /// Stall warning: a horn in the trainer, the stick shaker in the jet.
    pub horn: bool,
    /// Sideslip, -1..1. Slipping is noisy.
    pub slip: f32,
    pub flap_deg: f32,
    pub trim: f32,
    /// Listening from inside the cabin: muffled, boomy.
    pub inside: bool,
    /// Distance from the listener to the aircraft, m.
    pub distance: f32,
    /// Frequency factor from relative motion (1 = none).
    pub doppler: f32,
    /// Master volume: 0 while paused or muted.
    pub volume: f32,
    /// Turbofans instead of a piston engine: `n1` (0..1) drives the sound and `rpm` only
    /// says whether the engines run.
    pub jet: bool,
    pub n1: f32,
    pub reverser: f32,
    /// Landing gear extension (its motor runs while it moves, and it clunks into place).
    pub gear: f32,
    /// Fan shaft frequency at full power, Hz (sets the turbofan's whine).
    pub fan_hz: f32,
    /// Afterburner, 0..1.
    pub burner: f32,
    /// Sailplane variometer: climb rate in m/s, beeping faster and higher as you climb.
    pub vario: Option<f32>,
}

impl Default for State {
    fn default() -> Self {
        State {
            rpm: 0.0,
            throttle: 0.0,
            airspeed: 0.0,
            roll_speed: 0.0,
            buffet: 0.0,
            horn: false,
            slip: 0.0,
            flap_deg: 0.0,
            trim: 0.0,
            inside: false,
            distance: 10.0,
            doppler: 1.0,
            volume: 0.0,
            jet: false,
            n1: 0.0,
            reverser: 0.0,
            gear: 1.0,
            fan_hz: 83.0,
            burner: 0.0,
            vario: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Cue {
    Ding,
    Chime,
    Ring(u32),
    Crash,
    Touchdown(f32),
    Click,
    Say(Callout),
    /// Autopilot disconnect wailer.
    ApOff,
    /// Autothrottle disconnect chirps.
    AtOff,
    /// One word of a radio call; the first and last get a squelch click.
    Radio(Word, bool, bool),
}

/// The tower controller's vocabulary (assets/atc, made by tools/callouts.sh).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Word {
    Digit(u8),
    Skyhop,
    Runway,
    Wind,
    At,
    Gusting,
    Calm,
    ClearedToLand,
    ClearedForTakeoff,
    GoAround,
    Welcome,
}

const ATC_WAVS: [&[u8]; 20] = [
    include_bytes!("../assets/atc/0.wav"),
    include_bytes!("../assets/atc/1.wav"),
    include_bytes!("../assets/atc/2.wav"),
    include_bytes!("../assets/atc/3.wav"),
    include_bytes!("../assets/atc/4.wav"),
    include_bytes!("../assets/atc/5.wav"),
    include_bytes!("../assets/atc/6.wav"),
    include_bytes!("../assets/atc/7.wav"),
    include_bytes!("../assets/atc/8.wav"),
    include_bytes!("../assets/atc/9.wav"),
    include_bytes!("../assets/atc/skyhop.wav"),
    include_bytes!("../assets/atc/runway.wav"),
    include_bytes!("../assets/atc/wind.wav"),
    include_bytes!("../assets/atc/at.wav"),
    include_bytes!("../assets/atc/gusting.wav"),
    include_bytes!("../assets/atc/calm.wav"),
    include_bytes!("../assets/atc/land.wav"),
    include_bytes!("../assets/atc/takeoff.wav"),
    include_bytes!("../assets/atc/goaround.wav"),
    include_bytes!("../assets/atc/welcome.wav"),
];

impl Word {
    /// Index into the clip table (the callouts come first).
    fn clip(self) -> usize {
        CALLOUT_WAVS.len()
            + match self {
                Word::Digit(d) => d.min(9) as usize,
                Word::Skyhop => 10,
                Word::Runway => 11,
                Word::Wind => 12,
                Word::At => 13,
                Word::Gusting => 14,
                Word::Calm => 15,
                Word::ClearedToLand => 16,
                Word::ClearedForTakeoff => 17,
                Word::GoAround => 18,
                Word::Welcome => 19,
            }
    }
}

/// All the recorded clips: callouts, then the controller's words.
fn all_clips() -> Vec<Clip> {
    CALLOUT_WAVS.iter().chain(ATC_WAVS.iter()).map(|b| decode_wav(b)).collect()
}

/// Spoken radio-altimeter and ground-proximity callouts (assets/callouts, made by tools/callouts.sh).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Callout {
    Ft2500,
    Ft1000,
    Ft500,
    Ft100,
    Ft50,
    Ft40,
    Ft30,
    Ft20,
    Ft10,
    Minimums,
    SinkRate,
    PullUp,
    Glideslope,
    TooLowGear,
    TooLowFlaps,
    BankAngle,
}

const CALLOUT_WAVS: [&[u8]; 16] = [
    include_bytes!("../assets/callouts/2500.wav"),
    include_bytes!("../assets/callouts/1000.wav"),
    include_bytes!("../assets/callouts/500.wav"),
    include_bytes!("../assets/callouts/100.wav"),
    include_bytes!("../assets/callouts/50.wav"),
    include_bytes!("../assets/callouts/40.wav"),
    include_bytes!("../assets/callouts/30.wav"),
    include_bytes!("../assets/callouts/20.wav"),
    include_bytes!("../assets/callouts/10.wav"),
    include_bytes!("../assets/callouts/minimums.wav"),
    include_bytes!("../assets/callouts/sink_rate.wav"),
    include_bytes!("../assets/callouts/pull_up.wav"),
    include_bytes!("../assets/callouts/glideslope.wav"),
    include_bytes!("../assets/callouts/too_low_gear.wav"),
    include_bytes!("../assets/callouts/too_low_flaps.wav"),
    include_bytes!("../assets/callouts/bank_angle.wav"),
];

/// A clip at its own sample rate.
struct Clip {
    rate: f32,
    samples: Vec<f32>,
}

/// Reads a 16-bit PCM mono WAV, trims the silence off both ends and normalises it.
fn decode_wav(b: &[u8]) -> Clip {
    let (mut rate, mut samples) = (22_050.0, Vec::new());
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let body = &b[i + 8..(i + 8 + len).min(b.len())];
        if id == b"fmt " && body.len() >= 8 {
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]) as f32;
        } else if id == b"data" {
            samples = body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
        }
        i += 8 + len + (len & 1);
    }
    let loud = |s: &f32| s.abs() > 0.02;
    let start = samples.iter().position(loud).unwrap_or(0);
    let end = samples.iter().rposition(loud).map_or(samples.len(), |e| e + 1);
    let mut samples = samples[start..end.max(start)].to_vec();
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-3);
    samples.iter_mut().for_each(|s| *s *= 0.9 / peak);
    Clip { rate, samples }
}

struct Shared {
    state: State,
    cues: Vec<Cue>,
}

pub struct Audio {
    shared: Arc<Mutex<Shared>>,
    _device: Option<OutputDevice>,
    pub muted: bool,
}

impl Audio {
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(Shared { state: State::default(), cues: Vec::with_capacity(32) }));
        let params = OutputDeviceParameters { sample_rate: RATE as usize, channels_count: 2, channel_sample_count: BLOCK };
        let device = run_output_device(params, {
            let shared = shared.clone();
            let mut synth = Synth::new();
            synth.clips = all_clips();
            let mut cues = Vec::with_capacity(32);
            move |data| {
                // Never block the audio thread: if the game holds the lock, reuse the last state.
                if let Ok(mut s) = shared.try_lock() {
                    synth.target = s.state;
                    std::mem::swap(&mut cues, &mut s.cues);
                }
                for c in cues.drain(..) {
                    synth.cue(c);
                }
                synth.render(data);
            }
        });
        if let Err(e) = &device {
            eprintln!("skyhop: no audio ({e})");
        }
        Audio { shared, _device: device.ok(), muted: false }
    }

    pub fn update(&mut self, mut s: State) {
        if self.muted {
            s.volume = 0.0;
        }
        if let Ok(mut sh) = self.shared.lock() {
            sh.state = s;
        }
    }

    fn send(&self, c: Cue) {
        if self.muted {
            return;
        }
        if let Ok(mut sh) = self.shared.lock() {
            if sh.cues.len() < 32 {
                sh.cues.push(c);
            }
        }
    }
    /// A lesson step done.
    pub fn ding(&self) {
        self.send(Cue::Ding);
    }
    /// A lesson or the ring course complete.
    pub fn chime(&self) {
        self.send(Cue::Chime);
    }
    /// Flew through ring `n` (1-based): each one rings a note higher.
    pub fn ring(&self, n: usize) {
        self.send(Cue::Ring(n as u32));
    }
    pub fn crash(&self) {
        self.send(Cue::Crash);
    }
    /// Wheels touched; `sink_fpm` sets how hard the chirp and thump are.
    pub fn touchdown(&self, sink_fpm: f32) {
        self.send(Cue::Touchdown((sink_fpm / 700.0).clamp(0.1, 1.0)));
    }
    /// Menu selection moved.
    pub fn click(&self) {
        self.send(Cue::Click);
    }
    pub fn ap_off(&self) {
        self.send(Cue::ApOff);
    }
    pub fn at_off(&self) {
        self.send(Cue::AtOff);
    }
    /// A radio call from the tower, word by word.
    pub fn radio(&self, words: &[Word]) {
        for (i, w) in words.iter().enumerate() {
            self.send(Cue::Radio(*w, i == 0, i + 1 == words.len()));
        }
    }
    /// Speak a callout; callouts queue up rather than talk over each other.
    pub fn say(&self, c: Callout) {
        self.send(Cue::Say(c));
    }
}

// ------------------------------------------------------------ building blocks ---

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// Pink-ish noise (Paul Kellet's economy filter): wind and rumble sound natural, not hissy.
struct Pink {
    b: [f32; 3],
}
impl Pink {
    fn tick(&mut self, white: f32) -> f32 {
        self.b[0] = 0.99765 * self.b[0] + white * 0.0990460;
        self.b[1] = 0.96300 * self.b[1] + white * 0.2965164;
        self.b[2] = 0.57000 * self.b[2] + white * 1.0526913;
        (self.b[0] + self.b[1] + self.b[2] + white * 0.1848) * 0.25
    }
}

fn one_pole(cutoff: f32) -> f32 {
    1.0 - (-TAU * cutoff / RATE).exp()
}

/// Per-sample smoothing coefficient for a time constant in seconds.
fn tau(secs: f32) -> f32 {
    1.0 - (-1.0 / (secs * RATE)).exp()
}

/// Trapezoidal state-variable filter (Simper). `tick` returns (low-pass, band-pass with unity peak).
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}
impl Svf {
    fn new(fc: f32, q: f32) -> Self {
        let mut f = Svf::default();
        f.set(fc, q);
        f
    }
    fn set(&mut self, fc: f32, q: f32) {
        let g = (PI * fc.clamp(10.0, RATE * 0.45) / RATE).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1 * self.k)
    }
    fn low(&mut self, x: f32) -> f32 {
        self.tick(x).0
    }
    fn band(&mut self, x: f32) -> f32 {
        self.tick(x).1
    }
}

/// Feedback comb with a damped loop, the core of a small Freeverb-style room.
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}
impl Comb {
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}
struct Allpass {
    buf: Vec<f32>,
    i: usize,
}
impl Allpass {
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}
struct Room {
    combs: Vec<Comb>,
    aps: Vec<Allpass>,
}
impl Room {
    fn new(spread: usize) -> Self {
        let comb = |n: usize| Comb { buf: vec![0.0; n + spread], i: 0, store: 0.0 };
        let ap = |n: usize| Allpass { buf: vec![0.0; n + spread], i: 0 };
        Room { combs: [1116, 1188, 1277, 1356, 1422, 1491].into_iter().map(comb).collect(), aps: [556, 441, 341].into_iter().map(ap).collect() }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let mut y = 0.0;
        for c in &mut self.combs {
            y += c.tick(x, 0.8, 0.35);
        }
        for a in &mut self.aps {
            y = a.tick(y);
        }
        y * 0.12
    }
}

// --------------------------------------------------------------------- voices ---

#[derive(Clone, Copy)]
enum Kind {
    /// Vibraphone-like note: a fundamental plus a bright, quick-fading 4th partial.
    Bell { f: f32 },
    /// Low sine thud with a fast pitch drop: gear, runway seams, impacts.
    Thump { f: f32 },
    /// Tire squeal on touchdown.
    Chirp,
    /// Crunch, metal ring and settling debris.
    Crash,
    /// Air rushing past as you thread a ring.
    Whoosh,
    /// A small dry tick: menus and the trim wheel.
    Tick { f: f32 },
    /// A recorded clip (callouts), through a small cockpit-speaker filter; over the radio
    /// it's narrower, a little crunchy and hissy.
    Speech { clip: usize, radio: bool },
    /// Warbling two-tone alarm.
    Wailer,
}

#[derive(Clone, Copy)]
struct Voice {
    kind: Kind,
    amp: f32,
    /// Seconds since the start (negative while delayed).
    t: f32,
    len: f32,
    ph: [f32; 3],
    f1: Svf,
    f2: Svf,
    /// Share sent to the room.
    wet: f32,
    pan: f32,
}

impl Voice {
    fn new(kind: Kind, amp: f32, delay: f32) -> Self {
        let (len, wet, f1, f2) = match kind {
            Kind::Bell { .. } => (2.6, 0.45, Svf::default(), Svf::default()),
            Kind::Thump { .. } => (0.35, 0.05, Svf::default(), Svf::default()),
            Kind::Chirp => (0.4, 0.1, Svf::new(1200.0, 9.0), Svf::new(2600.0, 2.0)),
            Kind::Crash => (2.8, 0.35, Svf::new(900.0, 1.2), Svf::new(2400.0, 6.0)),
            Kind::Whoosh => (0.8, 0.3, Svf::new(500.0, 2.5), Svf::default()),
            Kind::Tick { f } => (0.05, 0.02, Svf::new(f, 4.0), Svf::default()),
            Kind::Speech { radio: false, .. } => (10.0, 0.04, Svf::new(1700.0, 0.45), Svf::default()),
            Kind::Speech { radio: true, .. } => (10.0, 0.02, Svf::new(1500.0, 1.1), Svf::new(2800.0, 0.8)),
            Kind::Wailer => (1.6, 0.05, Svf::default(), Svf::default()),
        };
        Voice { kind, amp, t: -delay, len, ph: [0.0; 3], f1, f2, wet, pan: 0.0 }
    }

    /// One sample. Returns None when finished.
    fn tick(&mut self, rng: &mut Rng, clips: &[Clip]) -> Option<f32> {
        let dt = 1.0 / RATE;
        self.t += dt;
        let t = self.t;
        if t < 0.0 {
            return Some(0.0);
        }
        if t > self.len {
            return None;
        }
        let attack = (t * 400.0).min(1.0);
        let y = match self.kind {
            Kind::Bell { f } => {
                let parts = [(1.0, 1.0, 1.1), (3.99, 0.18, 0.22), (9.2, 0.05, 0.06)];
                let mut s = 0.0;
                for (k, (ratio, a, decay)) in parts.iter().enumerate() {
                    self.ph[k] = (self.ph[k] + f * ratio * dt).fract();
                    s += (self.ph[k] * TAU).sin() * a * (-t / decay).exp();
                }
                // gentle tremolo, like a vibraphone's motor
                s * (1.0 + 0.06 * (t * TAU * 5.5).sin()) * attack
            }
            Kind::Thump { f } => {
                let ff = f * (1.0 + 1.2 * (-t / 0.015).exp());
                self.ph[0] = (self.ph[0] + ff * dt).fract();
                (self.ph[0] * TAU).sin() * (-t / 0.07).exp() * (t * 1500.0).min(1.0)
            }
            Kind::Chirp => {
                let f = 1350.0 - 600.0 * (t / 0.25).min(1.0);
                self.f1.set(f, 9.0);
                self.ph[0] = (self.ph[0] + f * dt).fract();
                let tone = (self.ph[0] * TAU).sin() * 0.35;
                let rub = self.f1.band(rng.next()) * 1.2 + self.f2.band(rng.next()) * 0.25;
                (tone + rub) * (-t / 0.09).exp() * attack
            }
            Kind::Crash => {
                let n = rng.next();
                let crunch = self.f1.band(n) * 1.5 * (-t / 0.25).exp();
                // inharmonic metal modes ringing out
                let mut ring = 0.0;
                for (k, (f, d)) in [(347.0, 0.6), (562.0, 0.45), (1191.0, 0.3)].iter().enumerate() {
                    self.ph[k] = (self.ph[k] + f * dt).fract();
                    ring += (self.ph[k] * TAU).sin() * (-t / d).exp() * 0.18;
                }
                // debris: sparse ticks that thin out
                let p = 90.0 * (-t / 0.5).exp() * dt;
                let tick = if rng.next() * 0.5 + 0.5 < p { 1.0 } else { 0.0 };
                let debris = self.f2.band(tick * 6.0 + n * 0.05 * (-t / 0.8).exp());
                let boom = (TAU * 42.0 * t).sin() * (-t / 0.18).exp() * 1.4;
                (crunch + ring + debris * 0.7 + boom) * (t * 2000.0).min(1.0)
            }
            Kind::Whoosh => {
                let x = t / self.len;
                let f = 350.0 + 2600.0 * (x * PI).sin().powi(2) * (1.0 - x * 0.5);
                self.f1.set(f, 2.0);
                let env = (x * PI).sin().powi(2);
                self.pan = (x * 2.0 - 1.0) * 0.6;
                self.f1.band(rng.next()) * env * 1.6
            }
            Kind::Speech { clip, radio } => {
                let c = &clips[clip];
                let pos = t * c.rate;
                let k = pos as usize;
                if k + 1 >= c.samples.len() {
                    return None;
                }
                let f = pos - k as f32;
                let s = c.samples[k] * (1.0 - f) + c.samples[k + 1] * f;
                if radio {
                    let v = (self.f1.band(s) * 3.0).tanh() * 0.7 + self.f2.band(rng.next()) * 0.04;
                    v * 0.9
                } else {
                    self.f1.band(s) * 1.6
                }
            }
            Kind::Wailer => {
                // sweeps up and down between two pitches six times a second
                let f = 760.0 + 200.0 * ((t * 6.0 * TAU).sin() * 0.5 + 0.5);
                self.ph[0] = (self.ph[0] + f * dt).fract();
                let x = self.ph[0] * TAU;
                (x.sin() + 0.3 * (3.0 * x).sin()) * 0.5 * attack * (1.0 - smoothstep(1.3, 1.6, t))
            }
            Kind::Tick { f } => {
                self.ph[0] = (self.ph[0] + f * 0.5 * dt).fract();
                (self.f1.band(rng.next()) * 1.4 + (self.ph[0] * TAU).sin() * 0.5) * (-t / 0.006).exp()
            }
        };
        Some(y * self.amp)
    }
}

// ---------------------------------------------------------------------- synth ---

struct Synth {
    target: State,
    clips: Vec<Clip>,
    // turbofans
    n1: f32,
    rev: f32,
    fan_ph: f32,
    shaft_ph: f32,
    core_ph: f32,
    jet_roar: Svf,
    jet_buzz: Svf,
    jet_rumble: Svf,
    gear_prev: f32,
    burner: f32,
    vario: f32,
    vario_ph: f32,
    vario_beep: f32,
    rng: Rng,
    // smoothed parameters
    rpm: f32,
    thr: f32,
    air: f32,
    roll: f32,
    buffet: f32,
    horn: f32,
    slip: f32,
    inside: f32,
    vol: f32,
    near: f32,
    doppler: f32,
    // engine
    cycle: f32,
    firing: usize,
    exc: f32,
    exc_decay: f32,
    cyl: [f32; 4],
    exhaust: [Svf; 4],
    bark: Svf,
    prop_ph: f32,
    prop_noise: Svf,
    engine_tone: Svf,
    // airframe
    pink: [Pink; 3],
    wind: [Svf; 2],
    hiss: [Svf; 2],
    gust: f32,
    gust_target: f32,
    rumble: Svf,
    shudder: f32,
    tire: Svf,
    seam: f32,
    horn_ph: f32,
    flap_prev: f32,
    motor: f32,
    motor_ph: f32,
    trim_notch: i32,
    // listener
    cabin: [Svf; 2],
    boom: Svf,
    air_abs: Svf,
    voices: Vec<Voice>,
    room: [Room; 2],
    dc: [f32; 2],
    /// Level of each layer, indexed by the `L_*` constants (tests solo them to check the balance).
    mix: [f32; 7],
}

const L_ENGINE: usize = 0;
const L_PROP: usize = 1;
const L_WIND: usize = 2;
const L_BUFFET: usize = 3;
const L_GEAR: usize = 4;
const L_HORN: usize = 5;
const L_CUES: usize = 6;
const MIX: [f32; 7] = [1.4, 1.0, 0.8, 0.33, 0.25, 1.0, 1.2];

impl Synth {
    fn new() -> Self {
        Synth {
            target: State::default(),
            clips: Vec::new(),
            n1: 0.0,
            rev: 0.0,
            fan_ph: 0.0,
            shaft_ph: 0.0,
            core_ph: 0.0,
            jet_roar: Svf::new(800.0, 0.7),
            jet_buzz: Svf::new(900.0, 0.7),
            jet_rumble: Svf::new(70.0, 0.8),
            gear_prev: 1.0,
            burner: 0.0,
            vario: 0.0,
            vario_ph: 0.0,
            vario_beep: 0.0,
            rng: Rng(0x2545_f491),
            rpm: 0.0,
            thr: 0.0,
            air: 0.0,
            roll: 0.0,
            buffet: 0.0,
            horn: 0.0,
            slip: 0.0,
            inside: 0.0,
            vol: 0.0,
            near: 1.0,
            doppler: 1.0,
            cycle: 0.0,
            firing: 0,
            exc: 0.0,
            exc_decay: 0.0,
            // Slightly uneven cylinders give the lumpy, alive sound of a real flat-four.
            cyl: [1.0, 0.82, 0.93, 0.76],
            exhaust: [Svf::new(92.0, 2.2), Svf::new(235.0, 3.0), Svf::new(510.0, 3.5), Svf::new(60.0, 1.0)],
            bark: Svf::new(1700.0, 1.2),
            prop_ph: 0.0,
            prop_noise: Svf::new(900.0, 0.8),
            engine_tone: Svf::new(4000.0, 0.7),
            pink: [Pink { b: [0.0; 3] }, Pink { b: [0.0; 3] }, Pink { b: [0.0; 3] }],
            wind: [Svf::new(400.0, 0.8); 2],
            hiss: [Svf::new(3000.0, 0.9); 2],
            gust: 0.0,
            gust_target: 0.0,
            rumble: Svf::new(80.0, 0.9),
            shudder: 0.0,
            tire: Svf::new(300.0, 2.0),
            seam: 0.0,
            horn_ph: 0.0,
            flap_prev: 0.0,
            motor: 0.0,
            motor_ph: 0.0,
            trim_notch: 0,
            cabin: [Svf::new(6000.0, 0.7); 2],
            boom: Svf::new(110.0, 1.4),
            air_abs: Svf::new(8000.0, 0.7),
            voices: Vec::with_capacity(64),
            room: [Room::new(0), Room::new(23)],
            dc: [0.0; 2],
            mix: MIX,
        }
    }

    /// Seconds until whatever is being said finishes.
    fn speech_busy(&self) -> f32 {
        self.voices
            .iter()
            .filter_map(|v| match v.kind {
                Kind::Speech { clip, .. } => Some(self.clips[clip].samples.len() as f32 / self.clips[clip].rate - v.t),
                _ => None,
            })
            .fold(0.0f32, f32::max)
    }

    fn voice(&mut self, kind: Kind, amp: f32, delay: f32) {
        if self.voices.len() < 60 {
            self.voices.push(Voice::new(kind, amp, delay));
        }
    }

    fn cue(&mut self, c: Cue) {
        match c {
            Cue::Ding => {
                self.voice(Kind::Bell { f: 783.99 }, 0.32, 0.0);
                self.voice(Kind::Bell { f: 1174.66 }, 0.26, 0.11);
            }
            Cue::Chime => {
                for (i, f) in [523.25, 659.26, 783.99, 1046.5, 1318.5].iter().enumerate() {
                    self.voice(Kind::Bell { f: *f }, 0.26, i as f32 * 0.09);
                }
            }
            Cue::Ring(n) => {
                // a major pentatonic run: every ring is a step up the scale
                const SCALE: [f32; 10] = [440.0, 493.88, 554.37, 659.26, 739.99, 880.0, 987.77, 1108.7, 1318.5, 1480.0];
                self.voice(Kind::Whoosh, 0.35, 0.0);
                self.voice(Kind::Bell { f: SCALE[(n as usize).saturating_sub(1) % 10] }, 0.32, 0.05);
            }
            Cue::Crash => self.voice(Kind::Crash, 0.75, 0.0),
            Cue::Touchdown(s) => {
                self.voice(Kind::Chirp, 0.18 + 0.4 * s, 0.0);
                self.voice(Kind::Thump { f: 55.0 }, 0.35 + 0.5 * s, 0.0);
            }
            Cue::Click => self.voice(Kind::Tick { f: 2600.0 }, 0.12, 0.0),
            Cue::ApOff => self.voice(Kind::Wailer, 0.22, 0.0),
            Cue::AtOff => {
                for i in 0..3 {
                    self.voice(Kind::Bell { f: 1568.0 }, 0.12, i as f32 * 0.22);
                }
            }
            Cue::Say(c) => {
                // wait for whatever is being said to finish
                let busy = self
                    .voices
                    .iter()
                    .filter_map(|v| match v.kind {
                        // callouts talk over the radio rather than wait for it
                        Kind::Speech { clip, radio: false } => Some(self.clips[clip].samples.len() as f32 / self.clips[clip].rate - v.t),
                        _ => None,
                    })
                    .fold(0.0f32, f32::max);
                if busy < 3.0 && (c as usize) < self.clips.len() {
                    self.voice(Kind::Speech { clip: c as usize, radio: false }, 0.85, busy + 0.05);
                }
            }
            Cue::Radio(w, first, last) => {
                let busy = self.speech_busy();
                let clip = w.clip();
                if clip < self.clips.len() && busy < 12.0 {
                    let gap = if first { 0.25 } else { 0.0 };
                    if first {
                        self.voice(Kind::Tick { f: 3200.0 }, 0.18, busy + 0.05);
                    }
                    self.voice(Kind::Speech { clip, radio: true }, 0.8, busy + gap);
                    if last {
                        let len = self.clips[clip].samples.len() as f32 / self.clips[clip].rate;
                        self.voice(Kind::Tick { f: 2400.0 }, 0.2, busy + gap + len + 0.05);
                    }
                }
            }
        }
    }

    /// Recompute filter settings that follow slowly-moving parameters, once per block.
    fn block(&mut self) {
        let p = self.target;
        let air = self.air;
        let wind_f = 180.0 + air * 9.0 - self.inside * 120.0;
        for (k, w) in self.wind.iter_mut().enumerate() {
            w.set(wind_f * (1.0 + k as f32 * 0.13), 0.7);
        }
        for h in &mut self.hiss {
            h.set(2200.0 + air * 25.0, 0.8);
        }
        let load = self.thr;
        self.bark.set(1300.0 + 900.0 * load, 1.1);
        self.engine_tone.set(1800.0 + 4500.0 * load, 0.7);
        self.prop_noise.set(500.0 + self.rpm * 0.45, 0.7);
        self.rumble.set(55.0 + self.roll * 2.5, 0.9);
        self.tire.set(180.0 + self.roll * 9.0, 2.0);
        // Cabin: walls take the top off and the cabin booms. Outside: distance soaks up the highs.
        let cab = 6500.0 - 5300.0 * self.inside;
        for c in &mut self.cabin {
            c.set(cab, 0.75);
        }
        self.air_abs.set(9000.0 / (1.0 + p.distance / 150.0), 0.7);
        // flap motor runs while the flaps are travelling
        let gear_moving = (p.gear - self.gear_prev).abs() > 1e-4;
        if gear_moving && (p.gear >= 1.0 || p.gear <= 0.0) {
            self.voice(Kind::Thump { f: 42.0 }, 0.5, 0.0); // down and locked / up and locked
        }
        self.gear_prev = p.gear;
        let moving = (p.flap_deg - self.flap_prev).abs() > 1e-3 || gear_moving;
        self.flap_prev = p.flap_deg;
        let n1 = self.n1;
        self.jet_roar.set(250.0 + 2200.0 * n1 * n1 - 120.0 * self.rev, 0.6);
        self.jet_buzz.set(700.0 + 600.0 * n1, 0.7);
        self.motor += ((moving as i32 as f32) - self.motor) * 0.35;
        // trim wheel ratchet: one tick per notch
        let notch = (p.trim / 0.004).round() as i32;
        if notch != self.trim_notch && p.volume > 0.0 {
            let n = (notch - self.trim_notch).unsigned_abs().min(3);
            for i in 0..n {
                // the airliner's stabiliser trim wheels clack loudly beside the pilot
                let (f, a) = if p.jet { (480.0, 0.16) } else { (900.0, 0.10) };
                self.voice(Kind::Tick { f }, a, i as f32 * 0.01);
            }
        }
        self.trim_notch = notch;
        // the wind gusts a little
        if self.rng.next() > 0.92 {
            self.gust_target = self.rng.next().abs();
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        let p = self.target;
        self.block();
        let near_target = 1.0 / (1.0 + (p.distance - 12.0).max(0.0) / 25.0);
        let (k_slow, k_mid, k_fast) = (tau(0.35), tau(0.08), tau(0.02));
        let dt = 1.0 / RATE;

        for frame in out.chunks_mut(2) {
            // --- smooth the parameters ---
            let running = p.rpm > 0.0;
            let rpm_target = if running { p.rpm } else { 0.0 };
            self.rpm += (rpm_target - self.rpm) * if running { k_slow } else { tau(0.7) };
            self.thr += (p.throttle - self.thr) * k_slow;
            self.air += (p.airspeed - self.air) * k_mid;
            self.roll += (p.roll_speed - self.roll) * k_mid;
            self.buffet += (p.buffet - self.buffet) * k_mid;
            self.horn += (p.horn as i32 as f32 - self.horn) * k_fast;
            self.slip += (p.slip - self.slip) * k_mid;
            self.inside += (p.inside as i32 as f32 - self.inside) * k_mid;
            self.vol += (p.volume - self.vol) * k_mid;
            self.near += (near_target - self.near) * k_mid;
            self.doppler += (p.doppler - self.doppler) * k_mid;
            self.gust += (self.gust_target - self.gust) * tau(1.2);
            self.n1 += ((if running { p.n1 } else { 0.0 }) - self.n1) * if running { k_mid } else { tau(4.0) };
            self.rev += (p.reverser - self.rev) * k_mid;
            let white = self.rng.next();
            let pink: [f32; 3] = [self.pink[0].tick(white), self.pink[1].tick(self.rng.next()), self.pink[2].tick(self.rng.next())];

            // --- engine: four cylinders firing in turn into a resonant exhaust ---
            let crank = self.rpm / 60.0 * self.doppler;
            self.cycle += crank * 0.5 * dt; // one 4-stroke cycle is two revolutions
            if self.cycle >= 0.25 {
                self.cycle -= 0.25;
                self.firing = (self.firing + 1) % 4;
                if running && self.rpm > 300.0 {
                    let rough = 0.12 + 0.25 * (1.0 - self.thr); // idle is lumpier
                    let a = self.cyl[self.firing] * (1.0 + rough * self.rng.next()) * (0.75 + 0.25 * self.thr);
                    self.exc = a;
                    self.exc_decay = (-1.0 / (RATE * (0.0018 + 0.0025 * (1.0 - self.thr)))).exp();
                }
            }
            let x = self.exc * (0.75 + 0.45 * white);
            self.exc *= self.exc_decay;
            let mut eng = self.exhaust[0].band(x) * 1.0 + self.exhaust[1].band(x) * 0.7 + self.exhaust[2].band(x) * 0.35 + self.exhaust[3].low(x) * 0.6;
            eng += self.bark.band(x) * 0.25 * self.thr;
            eng = self.engine_tone.low(eng);

            // --- prop: blade-pass buzz and a whooshing roar that grows with tip speed ---
            let bpf = crank * 2.0;
            self.prop_ph = (self.prop_ph + bpf * dt).fract();
            let d = (self.prop_ph - 0.5) / 0.09;
            let blade = (-d * d).exp() - 0.16;
            let tip = (self.rpm / 2700.0).powi(3);
            let prop = blade * tip * 0.35 + self.prop_noise.band(white) * tip * (0.25 + 0.6 * (blade + 0.16)) * 0.6;

            // --- turbofans: fan blade-pass tone, buzz-saw at high power, core whine and jet roar ---
            let (eng, prop) = if p.jet {
                let n1 = self.n1;
                let shaft = p.fan_hz * n1 * self.doppler; // fan shaft Hz at this N1
                self.shaft_ph = (self.shaft_ph + shaft * dt).fract();
                self.fan_ph = (self.fan_ph + shaft * 18.0 * dt).fract();
                self.core_ph = (self.core_ph + 6000.0 * (0.55 + 0.45 * n1) * self.doppler * dt).fract();
                let bp = self.fan_ph * TAU;
                let fan = (bp.sin() + 0.45 * (2.0 * bp).sin() + 0.2 * (3.0 * bp).sin()) * (0.1 + n1 * n1) * 0.16;
                let buzz = self.jet_buzz.band(self.shaft_ph * 2.0 - 1.0) * smoothstep(0.8, 0.95, n1) * 0.3;
                let whine = (self.core_ph * TAU).sin() * 0.012 * (0.3 + n1) * (n1 > 0.05) as i32 as f32;
                let roar = self.jet_roar.low(pink[0]) * (0.18 + 2.0 * n1.powi(3)) * (1.0 + 0.5 * self.rev);
                let rumble = self.jet_rumble.low(pink[1]) * n1 * n1 * (1.0 + 0.8 * self.rev) * 1.2;
                // afterburner: a deep, ragged roar with crackle
                self.burner += (p.burner - self.burner) * k_mid;
                let crackle = if self.rng.next() > 1.0 - 0.02 * self.burner { 1.0 } else { 0.0 };
                let burn = (self.jet_rumble.low(pink[2]) * 2.2 + pink[0] * 0.25 + crackle * 0.6) * self.burner;
                // the engines are far behind the flight deck
                let far = 1.0 - 0.7 * self.inside;
                ((roar + rumble + buzz + burn) * far, (fan + whine) * far)
            } else {
                (eng, prop)
            };

            // --- airframe ---
            let speed = (self.air / 60.0).max(0.0);
            let wind_amp = speed.powf(2.2).min(2.0) * (0.75 + 0.4 * self.gust) * (1.0 + 2.0 * self.slip.abs()) * (1.0 - 0.45 * self.inside);
            let hiss_amp = speed.powi(3).min(2.5) * 0.08;
            let wind = [
                self.wind[0].band(pink[0]) * wind_amp + self.hiss[0].band(pink[0]) * hiss_amp,
                self.wind[1].band(pink[1]) * wind_amp + self.hiss[1].band(pink[1]) * hiss_amp,
            ];
            // stall buffet: a low, irregular shudder through the airframe
            self.shudder += ((white * 0.5 + 0.5) * (white * 0.5 + 0.5) - self.shudder) * one_pole(14.0);
            let buffet = self.rumble.low(pink[2]) * self.buffet.powf(1.5) * (0.3 + speed) * self.shudder * 9.0;
            // wheels: rumble, tire whirr and a thud at each runway seam
            let rolling = (self.roll / 25.0).min(1.5);
            let mut gear = 0.0;
            if self.roll > 0.3 {
                gear = self.rumble.low(pink[2]) * rolling * 1.6 + self.tire.band(white) * rolling * 0.12;
                self.seam += self.roll * dt;
                if self.seam > 15.0 {
                    self.seam -= 15.0;
                    let a = 0.08 + 0.12 * rolling;
                    self.voice(Kind::Thump { f: 70.0 }, a, 0.0);
                    self.voice(Kind::Thump { f: 74.0 }, a * 0.8, 0.045);
                }
            }
            // stall warning horn: a reedy tone with a little waver
            let mut horn = 0.0;
            if self.horn > 0.001 && p.jet {
                // stick shaker: an eccentric motor rattling the control column
                self.horn_ph = (self.horn_ph + 26.0 * dt).fract();
                let s = (self.horn_ph * TAU).sin();
                horn = (s.signum() * 0.5 + s * 0.3 + white * 0.4) * self.horn * 0.12;
            } else if self.horn > 0.001 {
                self.horn_ph = (self.horn_ph + 1080.0 * (1.0 + 0.006 * (self.cycle * TAU * 24.0).sin()) * dt).fract();
                let ph = self.horn_ph * TAU;
                horn = ((ph).sin() + 0.5 * (2.0 * ph).sin() + 0.3 * (3.0 * ph).sin() + 0.15 * (5.0 * ph).sin() + white * 0.05) * self.horn * 0.09;
            }
            // sailplane variometer: beeps that quicken and rise as the climb gets stronger,
            // a low moan in strong sink
            let mut vario = 0.0;
            if let Some(v) = p.vario {
                self.vario += (v - self.vario) * k_mid;
                let v = self.vario;
                if v > 0.25 {
                    self.vario_beep = (self.vario_beep + (1.6 + 1.4 * v) * dt).fract();
                    let on = smoothstep(0.0, 0.02, self.vario_beep) * (1.0 - smoothstep(0.5, 0.55, self.vario_beep));
                    self.vario_ph = (self.vario_ph + (560.0 + 170.0 * v.min(5.0)) * dt).fract();
                    vario = (self.vario_ph * TAU).sin() * on * 0.07;
                } else if v < -2.0 {
                    self.vario_ph = (self.vario_ph + (330.0 + 20.0 * v.max(-5.0)) * dt).fract();
                    vario = (self.vario_ph * TAU).sin() * 0.03 * smoothstep(2.0, 3.0, -v);
                }
            }
            // flap motor whine
            let mut motor = 0.0;
            if self.motor > 0.01 {
                self.motor_ph = (self.motor_ph + 210.0 * dt).fract();
                motor = ((self.motor_ph * TAU).sin() + 0.4 * (self.motor_ph * TAU * 2.0).sin() + white * 0.15) * self.motor * 0.035;
            }

            // --- listener ---
            let m = self.mix;
            let craft = eng * m[L_ENGINE] + prop * m[L_PROP];
            let craft = craft + self.boom.band(craft) * 0.35 * self.inside + buffet * m[L_BUFFET] + (gear + motor * (0.4 + self.inside)) * m[L_GEAR];
            let craft = self.air_abs.low(craft) * self.near;
            // wind and the horn are only heard up close
            let close = self.near.powi(3);
            let mut lr = [0.0; 2];
            let mut wet = 0.0;
            let mut voices_lr = [0.0; 2];
            let mut i = 0;
            while i < self.voices.len() {
                match self.voices[i].tick(&mut self.rng, &self.clips) {
                    Some(y) => {
                        let v = &self.voices[i];
                        voices_lr[0] += y * (1.0 - v.pan.max(0.0));
                        voices_lr[1] += y * (1.0 + v.pan.min(0.0));
                        wet += y * v.wet;
                        i += 1;
                    }
                    None => {
                        self.voices.swap_remove(i);
                    }
                }
            }
            for c in 0..2 {
                let s = self.cabin[c].low(craft + wind[c] * m[L_WIND] * close) + (horn + vario) * m[L_HORN] * close;
                lr[c] = s + (voices_lr[c] + self.room[c].tick(wet)) * m[L_CUES];
            }
            for c in 0..2 {
                // remove DC, then a soft limiter keeps big moments from clipping
                self.dc[c] += (lr[c] - self.dc[c]) * one_pole(12.0);
                let y = (lr[c] - self.dc[c]) * self.vol;
                frame[c] = y.tanh() * 0.95;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &str, samples: &[f32]) {
        let data_len = (samples.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&(RATE as u32).to_le_bytes());
        b.extend_from_slice(&(RATE as u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// A scripted flight: start-up idle, takeoff roll, climb, a stall, a landing, cues, a crash.
    fn demo(mix: [f32; 7]) -> Vec<f32> {
        let mut s = Synth::new();
        s.mix = mix;
        let mut out = Vec::new();
        let mut buf = vec![0.0; BLOCK * 2];
        let secs = 34.0;
        let blocks = (secs * RATE / BLOCK as f32) as usize;
        let mut fired = [false; 10];
        for b in 0..blocks {
            let t = b as f32 * BLOCK as f32 / RATE;
            let mut st = State { volume: 1.0, rpm: 700.0, throttle: 0.0, ..State::default() };
            let ramp = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
            st.throttle = ramp(3.0, 5.0) * (1.0 - 0.3 * ramp(14.0, 15.0)) * (1.0 - ramp(18.0, 19.0));
            st.airspeed = 33.0 * ramp(4.0, 12.0) + 10.0 * ramp(12.0, 15.0) - 20.0 * ramp(18.0, 22.0) + 10.0 * ramp(23.0, 25.0);
            st.rpm = 700.0 + 1900.0 * st.throttle + 6.0 * st.airspeed;
            st.roll_speed = if t < 11.0 { st.airspeed } else if (25.0..30.0).contains(&t) { st.airspeed * (1.0 - ramp(25.0, 30.0)) } else { 0.0 };
            st.buffet = ramp(19.5, 21.5) * (1.0 - ramp(22.0, 23.0));
            st.horn = (20.0..22.5).contains(&t);
            st.flap_deg = 20.0 * ramp(16.0, 17.5);
            st.trim = 0.05 * ramp(13.0, 14.0);
            st.inside = (8.0..16.0).contains(&t);
            if t > 31.0 {
                st.rpm = 0.0;
            }
            let mut cue = |i: usize, at: f32, c: Cue, s: &mut Synth| {
                if t >= at && !fired[i] {
                    fired[i] = true;
                    s.cue(c);
                }
            };
            cue(0, 1.0, Cue::Click, &mut s);
            cue(1, 2.5, Cue::Ding, &mut s);
            cue(2, 13.0, Cue::Ring(1), &mut s);
            cue(3, 14.5, Cue::Ring(2), &mut s);
            cue(4, 25.0, Cue::Touchdown(0.3), &mut s);
            cue(5, 27.0, Cue::Chime, &mut s);
            cue(6, 31.0, Cue::Crash, &mut s);
            s.target = st;
            s.render(&mut buf);
            out.extend_from_slice(&buf);
        }
        out
    }

    #[test]
    fn callouts_decode() {
        for (i, b) in CALLOUT_WAVS.iter().chain(ATC_WAVS.iter()).enumerate() {
            let c = decode_wav(b);
            let secs = c.samples.len() as f32 / c.rate;
            assert!((0.2..4.0).contains(&secs), "clip {i} is {secs} s");
        }
    }

    /// The jet on short final: callouts, touchdown, spoilers and reverse thrust.
    fn jet_demo() -> Vec<f32> {
        let mut s = Synth::new();
        s.clips = all_clips();
        let mut out = Vec::new();
        let mut buf = vec![0.0; BLOCK * 2];
        let blocks = (30.0 * RATE / BLOCK as f32) as usize;
        let mut fired = [false; 10];
        for b in 0..blocks {
            let t = b as f32 * BLOCK as f32 / RATE;
            let ramp = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
            let mut st = State { volume: 1.0, rpm: 1.0, jet: true, distance: 50.0, ..State::default() };
            st.n1 = 0.55 - 0.33 * ramp(6.0, 8.0) + 0.5 * ramp(12.0, 15.0) - 0.5 * ramp(20.0, 22.0) + 0.7 * ramp(24.0, 26.0);
            st.airspeed = 74.0 - 50.0 * ramp(11.0, 22.0);
            st.roll_speed = if t > 10.0 { st.airspeed } else { 0.0 };
            st.reverser = ramp(11.0, 13.0) * (1.0 - ramp(20.0, 22.0));
            st.buffet = 0.5 * ramp(10.5, 11.5) * (1.0 - ramp(18.0, 22.0));
            st.inside = t < 5.0;
            st.horn = (26.0..28.0).contains(&t);
            st.gear = if t > 23.0 { 1.0 - ramp(23.0, 30.0) } else { 1.0 };
            for (i, (at, c)) in [(0.5, Callout::Ft100), (3.0, Callout::Ft50), (4.0, Callout::Ft40), (5.0, Callout::Ft30), (6.0, Callout::Ft20), (7.0, Callout::Ft10)].iter().enumerate() {
                if t >= *at && !fired[i] {
                    fired[i] = true;
                    s.cue(Cue::Say(*c));
                }
            }
            if t >= 10.0 && !fired[8] {
                fired[8] = true;
                s.cue(Cue::Touchdown(0.4));
            }
            s.target = st;
            s.render(&mut buf);
            out.extend_from_slice(&buf);
        }
        out
    }

    /// `cargo test --release atc_demo -- --ignored` writes target/atc_demo.wav: a landing clearance.
    #[test]
    #[ignore]
    fn atc_demo() {
        let mut s = Synth::new();
        s.clips = all_clips();
        s.target = State { volume: 1.0, ..State::default() };
        use Word::*;
        let words = [Skyhop, Digit(1), Digit(2), Digit(3), Wind, Digit(0), Digit(7), Digit(0), At, Digit(6), Runway, Digit(0), Digit(9), ClearedToLand];
        for (i, w) in words.iter().enumerate() {
            s.cue(Cue::Radio(*w, i == 0, i + 1 == words.len()));
        }
        let mut out = Vec::new();
        let mut buf = vec![0.0; BLOCK * 2];
        for _ in 0..(9.0 * RATE / BLOCK as f32) as usize {
            s.render(&mut buf);
            out.extend_from_slice(&buf);
        }
        assert!(out.iter().all(|x| x.is_finite()));
        let loud = out.iter().filter(|x| x.abs() > 0.05).count();
        assert!(loud > 10_000, "radio call is nearly silent");
        wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/atc_demo.wav"), &out);
    }

    #[test]
    fn jet_synth_is_finite_and_not_clipping() {
        let out = jet_demo();
        assert!(out.iter().all(|s| s.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 0.95, "peak {peak}");
    }

    #[test]
    #[ignore]
    fn jet_audio_demo() {
        let out = jet_demo();
        let level = |a: f32, b: f32| {
            let s = &out[(a * RATE) as usize * 2..(b * RATE) as usize * 2];
            (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
        };
        for (name, a, b) in [("final inside", 0.2, 3.0), ("final outside", 5.5, 6.0), ("idle flare", 8.5, 10.0), ("reverse", 15.0, 18.0), ("taxi idle", 22.0, 23.5), ("takeoff power", 27.0, 29.5)] {
            println!("{name:16} rms {:.3}", level(a, b));
        }
        wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/jet_demo.wav"), &out);
    }

    #[test]
    fn synth_is_finite_and_not_clipping() {
        let out = demo(MIX);
        assert!(out.iter().all(|s| s.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(peak < 0.95 && rms > 0.02, "peak {peak} rms {rms}");
    }

    /// `cargo test --release audio_demo -- --ignored` writes target/audio_demo.wav to listen to.
    #[test]
    #[ignore]
    fn audio_demo() {
        let out = demo(MIX);
        let level = |out: &[f32], a: f32, b: f32| {
            let s = &out[(a * RATE) as usize * 2..(b * RATE) as usize * 2];
            (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
        };
        let windows = [("idle", 0.2, 2.4), ("full power roll", 6.0, 8.0), ("inside climb", 9.0, 12.0), ("outside cruise", 16.5, 18.0), ("stall", 20.0, 22.0), ("rollout", 26.0, 27.0)];
        let solos: Vec<Vec<f32>> = (0..7).map(|k| demo(std::array::from_fn(|i| if i == k { MIX[i] } else { 0.0 }))).collect();
        println!("{:16} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}", "", "ALL", "engine", "prop", "wind", "buffet", "gear", "horn", "cues");
        for (name, a, b) in windows {
            let mut line = format!("{name:16} {:7.3}", level(&out, a, b));
            for so in &solos {
                line += &format!(" {:7.3}", level(so, a, b));
            }
            println!("{line}");
        }
        let sec = |a: f32, b: f32| {
            let s = &out[(a * RATE) as usize * 2..(b * RATE) as usize * 2];
            ((s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt(), s.iter().fold(0.0f32, |m, x| m.max(x.abs())))
        };
        for (name, a, b) in [("idle", 0.2, 2.8), ("full power roll", 6.0, 8.0), ("inside climb", 9.0, 12.0), ("outside cruise", 16.5, 18.0), ("stall horn", 20.0, 22.0), ("rollout", 26.0, 27.0), ("crash", 31.0, 33.0)] {
            let (rms, peak) = sec(a, b);
            println!("{name:16} rms {rms:.3} peak {peak:.3}");
        }
        wav(concat!(env!("CARGO_MANIFEST_DIR"), "/target/audio_demo.wav"), &out);
    }
}
