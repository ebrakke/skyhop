//! Background music for 99 Red Balloons: an original tune in the style of early-80s German
//! new wave. A slow, airy intro (pad and lead alone) bursts into a fast, driving band: pumping
//! octave bass, offbeat synth stabs, a brassy lead and a drum machine. Played by a little synth
//! on the audio thread, looping back to where the band comes in. A Standard MIDI File in the save folder (`red_balloons.mid`)
//! plays instead, if there is one.

use std::f32::consts::TAU;

pub const RATE: f32 = 44_100.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Inst {
    Lead,
    Bass,
    Pad,
    /// Short, bright offbeat chord stabs.
    Stab,
    Kick,
    Snare,
    Hat,
}

#[derive(Clone, Copy, Debug)]
pub struct Note {
    /// Start and length, seconds.
    pub t: f32,
    pub len: f32,
    pub key: u8,
    pub vel: f32,
    pub inst: Inst,
}

pub struct Song {
    /// Sorted by start time.
    pub notes: Vec<Note>,
    /// Loop length, seconds, and where the loop goes back to.
    pub length: f32,
    pub loop_start: f32,
}

// ------------------------------------------------------------ the theme ---

/// MIDI key for a note name like "A4", "C#5", "Bb3".
fn key(name: &str) -> u8 {
    let b = name.as_bytes();
    let base: i32 = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => 0,
    };
    let (acc, rest) = match b.get(1) {
        Some(b'#') => (1, &name[2..]),
        Some(b'b') => (-1, &name[2..]),
        _ => (0, &name[1..]),
    };
    let octave: i32 = rest.parse().unwrap_or(4);
    (12 * (octave + 1) + base + acc) as u8
}

/// The theme, A minor. The melody moves in units of 0.31 s: eighths of the slow intro
/// (96 bpm) and quarters once the band comes in (192 bpm, so the bass and drums double up
/// under the same tune).
pub fn theme() -> Song {
    let fast = 60.0 / 192.0;
    let unit = fast;
    let mut notes = Vec::new();
    let mut push = |t: f32, len: f32, key: u8, vel: f32, inst: Inst| notes.push(Note { t, len, key, vel, inst });

    // a chord to each phrase of the lead (eight units)
    let verse = ["A", "F", "C", "G", "A", "F", "C", "G"];
    let chorus = ["F", "G", "E", "A", "F", "G", "C", "E"];
    let triad = |c: &str, minor: bool| -> [u8; 3] {
        let r = key(&format!("{c}3")) + 12;
        [r, r + if minor { 3 } else { 4 }, r + 7]
    };
    let minor = |c: &str, chorus: bool| c == "A" || (c == "E" && !chorus);
    // the lead: (note, units), "r" rests
    let verse_lead: [&[(&str, u32)]; 8] = [
        &[("E5", 2), ("A5", 2), ("G5", 1), ("E5", 1), ("D5", 2)],
        &[("C5", 2), ("D5", 1), ("E5", 1), ("F5", 2), ("E5", 2)],
        &[("G5", 3), ("E5", 1), ("C5", 2), ("D5", 2)],
        &[("B4", 2), ("D5", 2), ("G4", 4)],
        &[("E5", 2), ("A5", 2), ("B5", 1), ("A5", 1), ("G5", 2)],
        &[("A5", 2), ("F5", 2), ("C5", 2), ("F5", 2)],
        &[("E5", 1), ("D5", 1), ("C5", 2), ("G5", 2), ("E5", 2)],
        &[("D5", 4), ("B4", 2), ("r", 2)],
    ];
    let chorus_lead: [&[(&str, u32)]; 8] = [
        &[("A5", 1), ("A5", 1), ("A5", 2), ("G5", 2), ("F5", 2)],
        &[("G5", 2), ("B5", 2), ("D6", 2), ("B5", 2)],
        &[("E6", 3), ("D6", 1), ("B5", 2), ("G#5", 2)],
        &[("A5", 4), ("E5", 2), ("G5", 2)],
        &[("F5", 2), ("A5", 2), ("C6", 2), ("A5", 2)],
        &[("B5", 2), ("D6", 2), ("G5", 2), ("B5", 2)],
        &[("C6", 3), ("B5", 1), ("G5", 2), ("E5", 2)],
        &[("G#5", 2), ("B5", 2), ("E6", 4)],
    ];
    let lead = |push: &mut dyn FnMut(f32, f32, u8, f32, Inst), t: f32, phrase: &[(&str, u32)], vel: f32| {
        let mut at = 0;
        for (n, len) in phrase {
            if *n != "r" {
                push(t + at as f32 * unit, *len as f32 * unit * 0.9, key(n), vel, Inst::Lead);
            }
            at += len;
        }
    };
    let phrase = unit * 8.0;

    // the intro: slow and airy, just the pad and a soft lead
    let mut t = 0.0;
    for (i, c) in verse.iter().enumerate() {
        for n in triad(c, minor(c, false)) {
            push(t, phrase * 0.98, n, 0.9, Inst::Pad);
        }
        push(t, phrase * 0.98, key(&format!("{c}2")), 0.45, Inst::Bass);
        lead(&mut push, t, verse_lead[i], 0.55);
        t += phrase;
    }
    // a snare roll and the band comes in
    for k in 0..16 {
        push(t + k as f32 * fast / 4.0, 0.1, 38, 0.3 + 0.045 * k as f32, Inst::Snare);
    }
    t += fast * 4.0;
    let loop_start = t;

    // sections: (chords, lead, chorus?, stabs?)
    let band: [(&[&str; 8], Option<&[&[(&str, u32)]; 8]>, bool, bool); 4] =
        [(&verse, Some(&verse_lead), false, false), (&chorus, Some(&chorus_lead), true, true), (&verse, None, false, true), (&chorus, Some(&chorus_lead), true, true)];
    for (chords, tune, is_chorus, stabs) in band {
        for (i, c) in chords.iter().enumerate() {
            let p0 = t;
            // pumping eighth-note bass, jumping the octave
            let root = key(&format!("{c}2"));
            for k in 0..16 {
                let up = if k % 4 == 3 { 12 } else { 0 };
                push(p0 + k as f32 * fast / 2.0, fast * 0.4, root + up, if k % 2 == 0 { 1.0 } else { 0.75 }, Inst::Bass);
            }
            for n in triad(c, minor(c, is_chorus)) {
                push(p0, phrase * 0.98, n, if stabs { 0.5 } else { 0.8 }, Inst::Pad);
                // offbeat stabs, a beat apart
                if stabs {
                    for k in 0..8 {
                        push(p0 + (k as f32 + 0.5) * fast, fast * 0.25, n + 12, 0.7, Inst::Stab);
                    }
                }
            }
            // drums: kick on one and three, snare on two and four, eighth hats, and a fill
            // to close each section
            let last = i == 7;
            for k in 0..8 {
                let at = p0 + k as f32 * fast;
                if k % 2 == 0 || k == 7 && is_chorus {
                    push(at, 0.3, 36, 1.0, Inst::Kick);
                }
                if k % 2 == 1 && !(last && k >= 6) {
                    push(at, 0.2, 38, 0.95, Inst::Snare);
                }
                for h in 0..2 {
                    push(at + h as f32 * fast / 2.0, 0.05, 42, if h == 1 { 0.9 } else { 0.5 }, Inst::Hat);
                }
            }
            if last {
                for k in 0..8 {
                    push(p0 + 6.0 * fast + k as f32 * fast / 4.0, 0.12, 38, 0.5 + 0.06 * k as f32, Inst::Snare);
                }
            }
            match tune {
                Some(tune) => lead(&mut push, p0, tune[i], 0.95),
                // the instrumental verse: the lead answers itself an octave down
                None if i % 2 == 1 => lead(&mut push, p0, &verse_lead[i - 1][..2], 0.6),
                None => {}
            }
            t += phrase;
        }
    }
    notes.sort_by(|a, b| a.t.total_cmp(&b.t));
    Song { notes, length: t, loop_start }
}

// ------------------------------------------------------------ MIDI files ---

fn varlen(b: &[u8], i: &mut usize) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let c = *b.get(*i)?;
        *i += 1;
        v = (v << 7) | (c & 0x7f) as u32;
        if c & 0x80 == 0 {
            return Some(v);
        }
    }
    Some(v)
}

/// Reads a Standard MIDI File (format 0 or 1): notes on channel 10 become drums, a channel
/// whose instrument is a bass plays the bass, pads and strings the pad, everything else the lead.
pub fn parse_midi(b: &[u8]) -> Option<Song> {
    if b.len() < 14 || &b[0..4] != b"MThd" {
        return None;
    }
    let ntracks = u16::from_be_bytes([b[10], b[11]]) as usize;
    let division = u16::from_be_bytes([b[12], b[13]]);
    if division & 0x8000 != 0 {
        return None; // SMPTE time isn't supported
    }
    let ppq = division.max(1) as f64;
    // (tick, kind): tempo changes and notes
    let mut tempos: Vec<(u64, u32)> = vec![(0, 500_000)];
    let mut raw: Vec<(u64, u64, u8, u8, u8)> = Vec::new(); // start tick, end tick, channel, key, velocity
    let mut programs = [0u8; 16];
    let mut i = 8 + u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as usize;
    for _ in 0..ntracks {
        if i + 8 > b.len() || &b[i..i + 4] != b"MTrk" {
            break;
        }
        let len = u32::from_be_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let end = (i + 8 + len).min(b.len());
        let mut p = i + 8;
        let mut tick = 0u64;
        let mut status = 0u8;
        let mut held: Vec<(u8, u8, u64, u8)> = Vec::new(); // channel, key, start, velocity
        while p < end {
            tick += varlen(b, &mut p)? as u64;
            let mut s = *b.get(p)?;
            if s & 0x80 != 0 {
                p += 1;
                if s < 0xf0 {
                    status = s;
                }
            } else {
                s = status; // running status
            }
            match s {
                0xff => {
                    let kind = *b.get(p)?;
                    p += 1;
                    let n = varlen(b, &mut p)? as usize;
                    if kind == 0x51 && n == 3 && p + 3 <= end {
                        tempos.push((tick, u32::from_be_bytes([0, b[p], b[p + 1], b[p + 2]])));
                    }
                    if kind == 0x2f {
                        break;
                    }
                    p += n;
                }
                0xf0 | 0xf7 => {
                    let n = varlen(b, &mut p)? as usize;
                    p += n;
                }
                _ => {
                    let ch = s & 0x0f;
                    match s & 0xf0 {
                        0x80 | 0x90 => {
                            let (k, v) = (*b.get(p)?, *b.get(p + 1)?);
                            p += 2;
                            if let Some(j) = held.iter().position(|h| h.0 == ch && h.1 == k) {
                                let h = held.swap_remove(j);
                                raw.push((h.2, tick, ch, k, h.3));
                            }
                            if s & 0xf0 == 0x90 && v > 0 {
                                held.push((ch, k, tick, v));
                            }
                        }
                        0xc0 => {
                            programs[ch as usize] = *b.get(p)?;
                            p += 1;
                        }
                        0xd0 => p += 1,
                        _ => p += 2,
                    }
                }
            }
        }
        i = end;
    }
    if raw.is_empty() {
        return None;
    }
    tempos.sort_by_key(|t| t.0);
    let seconds = |tick: u64| -> f32 {
        let (mut s, mut last, mut us) = (0.0f64, 0u64, 500_000.0f64);
        for &(at, tempo) in &tempos {
            if at >= tick {
                break;
            }
            s += (at - last) as f64 / ppq * us / 1e6;
            last = at;
            us = tempo as f64;
        }
        (s + (tick - last) as f64 / ppq * us / 1e6) as f32
    };
    let mut notes: Vec<Note> = raw
        .iter()
        .map(|&(a, z, ch, k, v)| {
            let inst = if ch == 9 {
                match k {
                    35 | 36 => Inst::Kick,
                    37..=40 => Inst::Snare,
                    _ => Inst::Hat,
                }
            } else {
                match programs[ch as usize] {
                    32..=39 => Inst::Bass,
                    48..=55 | 88..=95 => Inst::Pad,
                    _ => Inst::Lead,
                }
            };
            let t = seconds(a);
            Note { t, len: (seconds(z) - t).max(0.03), key: k, vel: v as f32 / 127.0, inst }
        })
        .collect();
    notes.sort_by(|a, b| a.t.total_cmp(&b.t));
    // loop on the bar after the last note ends
    let end = notes.iter().map(|n| n.t + n.len).fold(0.0f32, f32::max);
    let bar = 4.0 * tempos.last().map_or(0.5, |t| t.1 as f32 / 1e6);
    Some(Song { notes, length: (end / bar).ceil().max(1.0) * bar, loop_start: 0.0 })
}

/// The music to play: a MIDI file in the save folder, or the theme.
pub fn load() -> Song {
    crate::setup::data_dir()
        .and_then(|d| std::fs::read(d.join("red_balloons.mid")).ok())
        .and_then(|b| parse_midi(&b))
        .unwrap_or_else(theme)
}

// ------------------------------------------------------------ the synth ---

struct Voice {
    inst: Inst,
    freq: f32,
    vel: f32,
    /// Seconds since the start; the note is held for `len`.
    t: f32,
    len: f32,
    ph: [f32; 3],
    lp: [f32; 2],
    noise: u32,
}

impl Voice {
    fn noise(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        (self.noise >> 8) as f32 / 8_388_608.0 - 1.0
    }

    /// One sample, None once it has faded out.
    fn tick(&mut self) -> Option<f32> {
        let dt = 1.0 / RATE;
        self.t += dt;
        let t = self.t;
        let lp = |y: &mut f32, x: f32, fc: f32| {
            *y += (x - *y) * (1.0 - (-TAU * fc / RATE).exp());
            *y
        };
        // attack, decay to a sustain level, release after the note ends
        let adsr = |a: f32, d: f32, s: f32, r: f32| -> Option<f32> {
            let held = if t < a { t / a } else { s + (1.0 - s) * (-(t - a) / d).exp() };
            if t <= self.len {
                Some(held)
            } else {
                let after = t - self.len;
                (after < r * 5.0).then(|| held * (-after / r).exp())
            }
        };
        let saw = |ph: f32| 2.0 * ph - 1.0;
        let y = match self.inst {
            Inst::Lead => {
                // brassy: two detuned saws through a filter that opens on each note, and a
                // little vibrato once it has settled
                let env = adsr(0.01, 0.25, 0.7, 0.1)?;
                let vib = 1.0 + 0.004 * smooth(0.2, 0.45, t) * (t * TAU * 5.8).sin();
                let f = self.freq * vib;
                self.ph[0] = (self.ph[0] + f * dt).fract();
                self.ph[1] = (self.ph[1] + f * 1.007 * dt).fract();
                self.ph[2] = (self.ph[2] + f * 0.5 * dt).fract();
                let x = saw(self.ph[0]) * 0.55 + saw(self.ph[1]) * 0.45 + if self.ph[2] < 0.5 { 0.2 } else { -0.2 };
                let fc = 1400.0 + 3200.0 * (-t / 0.12).exp() + 600.0 * self.vel;
                let x = lp(&mut self.lp[0], x, fc);
                let x = lp(&mut self.lp[1], x, fc * 1.5);
                x * env * 0.15
            }
            Inst::Stab => {
                let env = adsr(0.002, 0.07, 0.15, 0.05)?;
                self.ph[0] = (self.ph[0] + self.freq * dt).fract();
                self.ph[1] = (self.ph[1] + self.freq * 1.01 * dt).fract();
                let pulse = if self.ph[0] < 0.25 { 1.0 } else { -0.33 };
                let x = lp(&mut self.lp[0], pulse + saw(self.ph[1]) * 0.6, 2600.0 + 3000.0 * (-t / 0.04).exp());
                x * env * 0.07
            }
            Inst::Bass => {
                let env = adsr(0.003, 0.2, 0.5, 0.03)?;
                self.ph[0] = (self.ph[0] + self.freq * dt).fract();
                self.ph[1] = (self.ph[1] + self.freq * 0.997 * dt).fract();
                let x = saw(self.ph[0]) + saw(self.ph[1]);
                let fc = 250.0 + 1600.0 * (-t / 0.09).exp();
                let x = lp(&mut self.lp[0], x, fc);
                let x = lp(&mut self.lp[1], x, fc);
                x * env * 0.22
            }
            Inst::Pad => {
                let env = adsr(0.12, 0.6, 0.7, 0.35)?;
                let mut x = 0.0;
                for (k, d) in [0.995, 1.0, 1.005].iter().enumerate() {
                    self.ph[k] = (self.ph[k] + self.freq * d * dt).fract();
                    x += saw(self.ph[k]);
                }
                let x = lp(&mut self.lp[0], x, 1500.0);
                x * env * 0.035
            }
            Inst::Kick => {
                if t > 0.4 {
                    return None;
                }
                let f = 45.0 + 110.0 * (-t / 0.03).exp();
                self.ph[0] = (self.ph[0] + f * dt).fract();
                (self.ph[0] * TAU).sin() * (-t / 0.18).exp() * 0.55
            }
            Inst::Snare => {
                if t > 0.3 {
                    return None;
                }
                let n = self.noise();
                let body = n - lp(&mut self.lp[0], n, 900.0);
                self.ph[0] = (self.ph[0] + 185.0 * dt).fract();
                (body * 0.8 * (-t / 0.09).exp() + (self.ph[0] * TAU).sin() * 0.4 * (-t / 0.04).exp()) * 0.32
            }
            Inst::Hat => {
                if t > 0.12 {
                    return None;
                }
                let n = self.noise();
                let hi = n - lp(&mut self.lp[0], n, 6500.0);
                hi * (-t / 0.025).exp() * 0.16
            }
        };
        Some(y * self.vel)
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Plays a song round and round.
pub struct Player {
    song: std::sync::Arc<Song>,
    pos: f32,
    next: usize,
    voices: Vec<Voice>,
    seed: u32,
}

impl Player {
    pub fn new(song: std::sync::Arc<Song>) -> Self {
        Player { song, pos: 0.0, next: 0, voices: Vec::with_capacity(64), seed: 0x1234_5678 }
    }

    pub fn sample(&mut self) -> f32 {
        let song = self.song.clone();
        while let Some(n) = song.notes.get(self.next).filter(|n| n.t <= self.pos) {
            self.next += 1;
            if self.voices.len() < 64 {
                self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) | 1;
                let freq = 440.0 * 2f32.powf((n.key as f32 - 69.0) / 12.0);
                self.voices.push(Voice { inst: n.inst, freq, vel: n.vel, t: 0.0, len: n.len, ph: [0.0; 3], lp: [0.0; 2], noise: self.seed });
            }
        }
        self.pos += 1.0 / RATE;
        if self.pos >= song.length {
            // round again from where the loop starts
            self.pos = song.loop_start + (self.pos - song.length);
            self.next = song.notes.partition_point(|n| n.t < song.loop_start);
        }
        let mut y = 0.0;
        self.voices.retain_mut(|v| match v.tick() {
            Some(s) => {
                y += s;
                true
            }
            None => false,
        });
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names() {
        assert_eq!(key("A4"), 69);
        assert_eq!(key("C4"), 60);
        assert_eq!(key("G#5"), 80);
        assert_eq!(key("Bb3"), 58);
    }

    #[test]
    fn theme_plays() {
        let song = theme();
        assert!(song.notes.len() > 500 && song.length > 60.0);
        assert!(song.loop_start > 15.0 && song.loop_start < song.length - 30.0);
        let mut p = Player::new(std::sync::Arc::new(song));
        let (mut peak, mut energy) = (0.0f32, 0.0f32);
        for _ in 0..(RATE as usize * 40) {
            let s = p.sample();
            assert!(s.is_finite());
            peak = peak.max(s.abs());
            energy += s * s;
        }
        assert!(peak > 0.2 && peak < 2.0, "peak {peak}");
        assert!(energy > 100.0, "energy {energy}");
    }

    /// A tiny two-track file: a tempo, a note on channel 1 and a kick on channel 10.
    #[test]
    fn reads_a_midi_file() {
        let mut f = b"MThd\0\0\0\x06\0\x01\0\x02\x01\xe0".to_vec(); // 480 ticks per quarter
        let t1 = [0x00, 0xff, 0x51, 0x03, 0x07, 0xa1, 0x20, 0x00, 0xff, 0x2f, 0x00]; // 120 bpm
        f.extend_from_slice(b"MTrk");
        f.extend_from_slice(&(t1.len() as u32).to_be_bytes());
        f.extend_from_slice(&t1);
        // note on A4, a quarter later off (running status, velocity 0); a kick on channel 10
        let t2 = [0x00, 0x90, 69, 100, 0x83, 0x60, 69, 0, 0x00, 0x99, 36, 90, 0x60, 0x89, 36, 0, 0x00, 0xff, 0x2f, 0x00];
        f.extend_from_slice(b"MTrk");
        f.extend_from_slice(&(t2.len() as u32).to_be_bytes());
        f.extend_from_slice(&t2);
        let song = parse_midi(&f).expect("parsed");
        assert_eq!(song.notes.len(), 2);
        let a = song.notes.iter().find(|n| n.key == 69).unwrap();
        assert_eq!(a.inst, Inst::Lead);
        assert!((a.len - 0.5).abs() < 1e-3, "{}", a.len);
        let k = song.notes.iter().find(|n| n.inst == Inst::Kick).unwrap();
        assert!((k.t - 0.5).abs() < 1e-3, "{}", k.t);
        assert!((song.length - 2.0).abs() < 1e-3 && song.loop_start == 0.0);
    }

    /// Renders a minute of the theme to target/theme.wav, to listen to.
    #[test]
    #[ignore]
    fn theme_demo() {
        let mut p = Player::new(std::sync::Arc::new(theme()));
        let samples: Vec<i16> = (0..(RATE as usize * 60)).map(|_| ((p.sample() * 0.6).tanh() * 32000.0) as i16).collect();
        let data = (samples.len() * 2) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&(RATE as u32).to_le_bytes());
        b.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data.to_le_bytes());
        for s in samples {
            b.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write("target/theme.wav", b).unwrap();
    }
}
