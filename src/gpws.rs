//! The airliner's automated voice: radio-altimeter callouts on the way down and
//! ground-proximity warnings (a simplified Boeing set).

use crate::audio::Callout;
use crate::flight::Telem;

const HEIGHTS: [(f32, Callout); 10] = [
    (2500.0, Callout::Ft2500),
    (1000.0, Callout::Ft1000),
    (500.0, Callout::Ft500),
    (200.0, Callout::Minimums),
    (100.0, Callout::Ft100),
    (50.0, Callout::Ft50),
    (40.0, Callout::Ft40),
    (30.0, Callout::Ft30),
    (20.0, Callout::Ft20),
    (10.0, Callout::Ft10),
];

#[derive(Default)]
pub struct Gpws {
    prev_agl: Option<f32>,
    /// Heights already called on this descent (re-armed after climbing back above them).
    said: [bool; HEIGHTS.len()],
    /// Seconds until a warning may repeat.
    quiet: f32,
}

impl Gpws {
    /// What to say this frame, if anything. `ils_gs` is the glideslope deviation in dots
    /// (positive = below the glide path).
    pub fn update(&mut self, t: &Telem, ils_gs: Option<f32>, dt: f32) -> Option<Callout> {
        let agl = t.agl_ft;
        let prev = self.prev_agl.replace(agl);
        self.quiet -= dt;
        if t.on_ground {
            return None;
        }
        let mut call = None;
        for (i, (h, c)) in HEIGHTS.iter().enumerate() {
            if agl > h + 75.0 {
                self.said[i] = false;
            }
            if let Some(p) = prev {
                if p > *h && agl <= *h && !self.said[i] {
                    self.said[i] = true;
                    call = Some(*c);
                }
            }
        }
        if call.is_some() {
            return call;
        }
        if self.quiet > 0.0 || agl < 30.0 {
            return None;
        }
        let sink = -t.vs_fpm;
        let limit = 1500.0 + agl * 1.4;
        let warn = if agl < 2500.0 && sink > limit * 1.35 {
            Some(Callout::PullUp)
        } else if agl < 2500.0 && sink > limit {
            Some(Callout::SinkRate)
        } else if agl < 500.0 && t.gear < 0.99 && sink > 0.0 && t.ias_kt < 190.0 {
            Some(Callout::TooLowGear)
        } else if agl < 245.0 && t.gear > 0.99 && t.flap_deg < 24.0 && t.ias_kt < 160.0 {
            Some(Callout::TooLowFlaps)
        } else if t.bank.abs() > 35.0 {
            Some(Callout::BankAngle)
        } else if agl < 1000.0 && ils_gs.is_some_and(|d| d > 1.3) {
            Some(Callout::Glideslope)
        } else {
            None
        };
        if warn.is_some() {
            self.quiet = 2.5;
        }
        warn
    }
}
