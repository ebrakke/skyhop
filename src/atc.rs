//! The tower: takeoff and landing clearances with the real wind, read over the radio.

use crate::audio::Word;
use crate::flight::{Aircraft, Telem, FT, KT};
use crate::lessons::hdg_err;
use crate::weather::Weather;
use crate::world::{RWY_HALF_LEN, RWY_HALF_W};

#[derive(Default)]
pub struct Tower {
    callsign: [u8; 3],
    takeoff_given: bool,
    landing_given: bool,
    welcome_given: bool,
    waited: f32,
}

fn digits(n: u32, len: usize, out: &mut Vec<Word>) {
    let s = format!("{n:0len$}");
    out.extend(s.bytes().map(|b| Word::Digit(b - b'0')));
}

impl Tower {
    pub fn new(seed: u32) -> Self {
        Tower { callsign: [(seed % 9 + 1) as u8, (seed / 9 % 10) as u8, (seed / 90 % 10) as u8], ..Default::default() }
    }

    fn call(&self, runway09: bool, w: &Weather, last: Word) -> Vec<Word> {
        let mut v = vec![Word::Skyhop];
        v.extend(self.callsign.iter().map(|d| Word::Digit(*d)));
        v.push(Word::Wind);
        if w.wind_kt < 3.0 {
            v.push(Word::Calm);
        } else {
            let dir = ((w.wind_from / 10.0).round() as u32 * 10) % 360;
            digits(if dir == 0 { 360 } else { dir }, 3, &mut v);
            v.push(Word::At);
            digits(w.wind_kt.round() as u32, 1, &mut v);
            if w.gust_kt >= 3.0 {
                v.push(Word::Gusting);
                digits((w.wind_kt + w.gust_kt).round() as u32, 1, &mut v);
            }
        }
        v.push(Word::Runway);
        v.extend(if runway09 { [Word::Digit(0), Word::Digit(9)] } else { [Word::Digit(2), Word::Digit(7)] });
        v.push(last);
        v
    }

    /// Anything to say this frame?
    pub fn update(&mut self, ac: &Aircraft, t: &Telem, w: &Weather, dt: f32) -> Option<Vec<Word>> {
        let on_runway = ac.pos.z.abs() < RWY_HALF_W && ac.pos.x.abs() < RWY_HALF_LEN;
        let east = hdg_err(t.hdg, 90.0) < 45.0;
        let west = hdg_err(t.hdg, 270.0) < 45.0;
        // lined up at either end: cleared for takeoff after a moment
        if t.on_ground && on_runway && !self.takeoff_given && !self.landing_given && t.gs_kt < 30.0 && (east || west) && ac.pos.x.abs() > RWY_HALF_LEN - 400.0 {
            self.waited += dt;
            if self.waited > 2.5 {
                self.takeoff_given = true;
                return Some(self.call(east, w, Word::ClearedForTakeoff));
            }
        }
        // inbound on final: cleared to land
        if !t.on_ground && !self.landing_given {
            let (thr, aligned) = if east { (-RWY_HALF_LEN, true) } else if west { (RWY_HALF_LEN, true) } else { (0.0, false) };
            let before = if east { thr - ac.pos.x } else { ac.pos.x - thr };
            let below = t.agl_ft < 3500.0 && ac.pos.y / FT < 4000.0;
            if aligned && (300.0..12_000.0).contains(&before) && ac.pos.z.abs() < 2500.0 && below && t.vs_fpm < -150.0 && ac.gear > 0.9 {
                self.landing_given = true;
                return Some(self.call(east, w, Word::ClearedToLand));
            }
        }
        // cleared, but low and not lined up with the runway: go around (and ask again)
        if self.landing_given && !t.on_ground && t.agl_ft < 400.0 && ac.pos.x.abs() < RWY_HALF_LEN + 4000.0 && ac.pos.z.abs() > 120.0 && t.vs_fpm < 0.0 {
            self.landing_given = false;
            let mut v = vec![Word::Skyhop];
            v.extend(self.callsign.iter().map(|d| Word::Digit(*d)));
            v.push(Word::GoAround);
            return Some(v);
        }
        // down and slow: welcome
        if self.landing_given && !self.welcome_given && t.on_ground && on_runway && ac.ias < 40.0 * KT {
            self.welcome_given = true;
            let mut v = vec![Word::Skyhop];
            v.extend(self.callsign.iter().map(|d| Word::Digit(*d)));
            v.push(Word::Welcome);
            return Some(v);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_wind_and_runway() {
        let t = Tower::new(0);
        let w = Weather { wind_from: 74.0, wind_kt: 6.4, gust_kt: 0.0, ..Default::default() };
        let words = t.call(true, &w, Word::ClearedToLand);
        use Word::*;
        assert_eq!(&words[4..], &[Wind, Digit(0), Digit(7), Digit(0), At, Digit(6), Runway, Digit(0), Digit(9), ClearedToLand]);
        let g = Weather { wind_from: 300.0, wind_kt: 15.0, gust_kt: 10.0, ..Default::default() };
        let words = t.call(false, &g, Word::ClearedForTakeoff);
        assert_eq!(&words[4..], &[Wind, Digit(3), Digit(0), Digit(0), At, Digit(1), Digit(5), Gusting, Digit(2), Digit(5), Runway, Digit(2), Digit(7), ClearedForTakeoff]);
    }
}
