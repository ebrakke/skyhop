//! Autopilot and autothrottle, modelled loosely on a Boeing 737 mode control panel (and a
//! simpler GA autopilot for the trainer): heading select, altitude hold with capture,
//! vertical speed, speed via autothrottle, and an approach mode that captures the localizer
//! and glideslope. On the airliner it flares, retards the thrust, lands and tracks the
//! centreline with autobrakes.

use crate::flight::{Aircraft, FT, KT};
use crate::lessons::{glide_angle, ils};
use crate::world::{smoothstep, RWY_HALF_LEN};
use macroquad::math::Vec3;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lateral {
    /// Hold wings level.
    Roll,
    Hdg,
    /// Tracking the localizer.
    Loc,
    /// On the runway: steering down the centreline.
    Rollout,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Vertical {
    Vs,
    /// Capturing the selected altitude (FMA: ALT ACQ).
    AltAcq,
    Alt,
    Gs,
    Flare,
}

/// Things the panel wants to tell the pilot.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Note {
    Disconnect,
    AtDisconnect,
}

pub struct Autopilot {
    pub engaged: bool,
    /// Autothrottle holding the selected speed (airliner only).
    pub at: bool,
    pub lateral: Lateral,
    pub vertical: Vertical,
    /// Approach mode armed: will capture the localizer, then the glideslope.
    pub app: bool,
    pub hdg: f32,
    pub alt_ft: f32,
    pub vs_fpm: f32,
    pub spd_kt: f32,
    /// Thrust levers pulled back for landing.
    pub retard: bool,
    /// Pitch the inner loop is steering to (deg), and how far the column is pushed.
    pitch_target: f32,
    pitch_int: f32,
    /// Smoothed angle of attack, deg.
    alpha_f: f32,
    spd_int: f32,
    bank_cmd: f32,
    track_int: f32,
    touchdown_t: f32,
    pub notes: Vec<Note>,
}

impl Default for Autopilot {
    fn default() -> Self {
        Autopilot {
            engaged: false,
            at: false,
            lateral: Lateral::Roll,
            vertical: Vertical::Vs,
            app: false,
            hdg: 0.0,
            alt_ft: 3000.0,
            vs_fpm: 0.0,
            spd_kt: 0.0,
            retard: false,
            pitch_target: 0.0,
            pitch_int: 0.0,
            alpha_f: 0.0,
            spd_int: 0.0,
            bank_cmd: 0.0,
            track_int: 0.0,
            touchdown_t: 0.0,
            notes: Vec::new(),
        }
    }
}

fn wrap180(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

impl Autopilot {
    /// Set the bugs to what the aircraft is doing now (on first use or a new flight).
    pub fn sync(&mut self, ac: &Aircraft) {
        self.hdg = ac.heading_deg().round();
        self.alt_ft = ((ac.pos.y / FT) / 100.0).round() * 100.0;
        self.spd_kt = (ac.ias / KT).round();
        self.vs_fpm = 0.0;
    }

    pub fn engage(&mut self, ac: &Aircraft) {
        if self.engaged {
            return;
        }
        self.engaged = true;
        self.lateral = if self.app && self.lateral == Lateral::Loc { Lateral::Loc } else { Lateral::Hdg };
        self.vertical = Vertical::Vs;
        // pick up the current vertical speed, rounded, and head for the selected altitude
        self.vs_fpm = ((ac.vel.y / FT * 60.0) / 100.0).round() * 100.0;
        self.pitch_target = ac.pitch_deg();
        self.pitch_int = 0.0;
        self.alpha_f = ac.alpha.to_degrees();
        self.bank_cmd = ac.bank_deg();
        let alt = ac.pos.y / FT;
        if (self.alt_ft - alt).abs() < 150.0 {
            self.vertical = Vertical::Alt;
        }
    }

    pub fn disengage(&mut self) {
        if self.engaged {
            self.engaged = false;
            self.notes.push(Note::Disconnect);
        }
        if self.vertical == Vertical::Flare || self.lateral == Lateral::Rollout {
            self.app = false;
        }
    }

    pub fn toggle_at(&mut self, ac: &Aircraft) {
        if !ac.spec.is_jet() {
            return;
        }
        self.at = !self.at;
        if self.at {
            if self.spd_kt < 1.0 {
                self.spd_kt = (ac.ias / KT).round();
            }
            self.spd_int = ac.ctl.throttle;
            self.retard = false;
        } else {
            self.notes.push(Note::AtDisconnect);
        }
    }

    /// Selecting V/S: hold the current rate.
    pub fn select_vs(&mut self, ac: &Aircraft) {
        self.vertical = Vertical::Vs;
        self.vs_fpm = ((ac.vel.y / FT * 60.0) / 100.0).round() * 100.0;
    }

    /// ALT HOLD at the current altitude.
    pub fn select_alt(&mut self, ac: &Aircraft) {
        self.alt_ft = ((ac.pos.y / FT) / 10.0).round() * 10.0;
        self.vertical = Vertical::Alt;
    }

    pub fn select_hdg(&mut self) {
        self.lateral = Lateral::Hdg;
    }

    pub fn toggle_app(&mut self) {
        self.app = !self.app;
        if !self.app {
            if self.lateral == Lateral::Loc {
                self.lateral = Lateral::Hdg;
            }
            if self.vertical == Vertical::Gs {
                self.vertical = Vertical::Vs;
            }
        }
    }

    /// Fly the aircraft for one frame: writes pitch, roll, yaw and (with A/T) the throttle.
    /// `agl` is the radio altitude in feet.
    pub fn update(&mut self, ac: &mut Aircraft, agl: f32, dt: f32) {
        let jet = ac.spec.is_jet();
        let ias = ac.ias / KT;
        let vs = ac.vel.y / FT * 60.0;
        let alt = ac.pos.y / FT;
        let pitch = ac.pitch_deg();
        let bank = ac.bank_deg();
        let hdg = ac.heading_deg();
        let track = ac.vel.x.atan2(-ac.vel.z).to_degrees().rem_euclid(360.0);
        let ils = ils(ac.pos, hdg);

        // ---- mode changes ----
        if self.app && self.engaged {
            if let Some((loc, gs)) = ils {
                if self.lateral != Lateral::Loc && self.lateral != Lateral::Rollout && loc.abs() < 2.0 {
                    self.lateral = Lateral::Loc;
                    self.track_int = 0.0;
                }
                // the glideslope is captured from below (or on it) once on the localizer
                if self.lateral == Lateral::Loc && matches!(self.vertical, Vertical::Vs | Vertical::Alt | Vertical::AltAcq) && gs < 0.4 && gs > -1.0 {
                    self.vertical = Vertical::Gs;
                }
            }
            if jet && self.vertical == Vertical::Gs && agl < 55.0 && !ac.on_ground {
                self.vertical = Vertical::Flare;
            }
        }
        if self.engaged && ac.on_ground && matches!(self.vertical, Vertical::Flare | Vertical::Gs) {
            self.lateral = Lateral::Rollout;
            self.touchdown_t += dt;
        }
        if self.vertical == Vertical::Vs && self.engaged {
            // arm altitude capture toward the selected altitude
            let dh = self.alt_ft - alt;
            if dh.signum() == vs.signum() && dh.abs() < (vs.abs() * 0.15).max(80.0) {
                self.vertical = Vertical::AltAcq;
            }
        }
        if self.vertical == Vertical::AltAcq && (self.alt_ft - alt).abs() < 25.0 && vs.abs() < 300.0 {
            self.vertical = Vertical::Alt;
        }

        // ---- autothrottle ----
        if self.at {
            if self.vertical == Vertical::Flare && agl < 27.0 || ac.on_ground && self.touchdown_t > 0.0 {
                self.retard = true;
            }
            if self.retard {
                ac.ctl.throttle = (ac.ctl.throttle - 0.35 * dt).max(0.0);
                if ac.on_ground && self.touchdown_t > 2.0 {
                    self.at = false;
                    self.notes.push(Note::AtDisconnect);
                }
            } else {
                let err = self.spd_kt - ias;
                // trend: lead the slow-spooling engines with the airspeed rate
                let accel = -ac.spec_force.z;
                self.spd_int = (self.spd_int + err * 0.004 * dt * 60.0 / 60.0).clamp(0.0, 1.0);
                let want = (self.spd_int + err * 0.025 - accel * 0.06).clamp(0.0, 1.0);
                ac.ctl.throttle += (want - ac.ctl.throttle).clamp(-0.12 * dt, 0.12 * dt);
            }
        }

        if !self.engaged {
            return;
        }

        // ---- lateral ----
        let max_bank = if jet { 25.0 } else { 20.0 };
        let want_bank = match self.lateral {
            Lateral::Roll => 0.0,
            Lateral::Hdg => (wrap180(self.hdg - hdg) * 1.6).clamp(-max_bank, max_bank),
            Lateral::Loc => {
                // fly a track that closes on the centreline, trimming out the wind drift
                let dev_m = ac.pos.z;
                let range = (RWY_HALF_LEN + 300.0 - ac.pos.x).max(500.0);
                let closing = (-dev_m / range).atan().to_degrees() * 6.0;
                self.track_int = (self.track_int + wrap180(90.0 + closing.clamp(-30.0, 30.0) - track) * 0.05 * dt).clamp(-15.0, 15.0);
                let want_track = 90.0 + closing.clamp(-30.0, 30.0) + self.track_int;
                (wrap180(want_track - track) * 1.8).clamp(-max_bank * 0.8, max_bank * 0.8)
            }
            Lateral::Rollout => 0.0,
        };
        // the bank command moves at a gentle rate
        let rate = if jet { 4.0 } else { 6.0 };
        self.bank_cmd += (want_bank - self.bank_cmd).clamp(-rate * dt, rate * dt);
        let p = -ac.omega.z;
        ac.ctl.roll = ((self.bank_cmd - bank) * 0.05 - p * 0.6).clamp(-0.6, 0.6);
        // keep the ball centred; on the runway steer the centreline
        ac.ctl.yaw = if self.lateral == Lateral::Rollout || self.vertical == Vertical::Flare && agl < 20.0 {
            let r = -(ac.rot * ac.omega).y;
            (-ac.pos.z * 0.04 - wrap180(hdg - 90.0) * 0.12 - r.to_degrees() * 0.25).clamp(-1.0, 1.0)
        } else {
            (ac.beta * 4.0).clamp(-0.5, 0.5)
        };

        // ---- vertical ----
        let want_vs = match self.vertical {
            Vertical::Vs => self.vs_fpm,
            Vertical::AltAcq => {
                let dh = self.alt_ft - alt;
                let limit = self.vs_fpm.abs().max(800.0);
                (dh / 8.0 * 60.0).clamp(-limit, limit)
            }
            Vertical::Alt => ((self.alt_ft - alt) * 4.0).clamp(-1000.0, 1000.0),
            Vertical::Gs => {
                let gs_kt = Vec3::new(ac.vel.x, 0.0, ac.vel.z).length() / KT;
                let base = -gs_kt * 3f32.to_radians().tan() * 101.3;
                let dist = (crate::world::PAPI_DIST - RWY_HALF_LEN - ac.pos.x).max(50.0);
                let h_err_ft = dist * (glide_angle(ac.pos) - 3.0).to_radians().tan() / FT;
                base - h_err_ft * 3.5
            }
            Vertical::Flare => -(agl * 60.0 / 5.0).max(90.0),
        };
        if ac.on_ground {
            // lower the nose gently, then hold it down
            self.pitch_target += (-1.0 - self.pitch_target).clamp(-1.0 * dt, 1.0 * dt);
        } else {
            // fly a flight-path angle: the climb angle we want plus the wing's angle of attack
            let v = ac.vel.length().max(20.0);
            let gamma = ((want_vs * FT / 60.0) / v).clamp(-0.35, 0.35).asin().to_degrees();
            self.alpha_f += (ac.alpha.to_degrees() - self.alpha_f) * (dt / 1.5).min(1.0);
            let target = (gamma + self.alpha_f + ((want_vs - vs) * 0.0015).clamp(-3.0, 3.0)).clamp(-10.0, 15.0);
            let rate = if self.vertical == Vertical::Flare { 3.0 } else if jet { 1.5 } else { 2.5 };
            self.pitch_target += (target - self.pitch_target).clamp(-rate * dt, rate * dt);
        }
        let q = ac.omega.x.to_degrees();
        let err = self.pitch_target - pitch;
        self.pitch_int = (self.pitch_int + err * 0.02 * dt).clamp(-0.3, 0.3);
        ac.ctl.pitch = (err * 0.09 + self.pitch_int - q * 0.06).clamp(-0.6, 0.6);
        // trim away the column force, like a real autopilot does
        if !ac.on_ground {
            ac.ctl.trim = (ac.ctl.trim + ac.ctl.pitch * 0.15 * dt).clamp(-0.6, 0.6);
            self.pitch_int -= ac.ctl.pitch * 0.15 * dt * smoothstep(0.0, 0.05, ac.ctl.pitch.abs());
        }
        // autobrake and the end of the landing
        if self.lateral == Lateral::Rollout {
            let decel = ac.spec_force.z;
            if ias > 30.0 && self.touchdown_t > 1.0 {
                ac.ctl.brake = ((2.0 - decel) * 0.3 + 0.3).clamp(0.0, 1.0) * smoothstep(1.0, 3.0, self.touchdown_t);
            }
            if ias < 30.0 {
                ac.ctl.brake = 0.6;
                self.disengage();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::Craft;
    use crate::world::{Terrain, World};
    use macroquad::math::vec3;

    fn fly(ac: &mut Aircraft, ap: &mut Autopilot, t: &Terrain, secs: f32) {
        let dt = 1.0 / 240.0;
        for i in 0..(secs / dt) as usize {
            if i % 4 == 0 {
                let agl = (ac.pos.y - t.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height) / FT;
                ap.update(ac, agl, dt * 4.0);
            }
            ac.step(dt, t);
            assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        }
    }

    #[test]
    fn captures_heading_and_altitude() {
        let t = Terrain::generate();
        for craft in [Craft::Cessna, Craft::Jet] {
            let ias = if craft == Craft::Jet { 220.0 } else { 100.0 };
            let mut ac = Aircraft::in_flight(craft, vec3(0.0, 900.0, 0.0), 0.0, ias, 0.0, 0);
            let mut ap = Autopilot::default();
            ap.sync(&ac);
            ap.engage(&ac);
            ap.toggle_at(&ac);
            ap.hdg = 120.0;
            ap.alt_ft = 4000.0;
            ap.vs_fpm = if craft == Craft::Jet { 1500.0 } else { 500.0 };
            ap.vertical = Vertical::Vs;
            if craft == Craft::Cessna {
                ac.ctl.throttle = 1.0;
            }
            fly(&mut ac, &mut ap, &t, 240.0);
            let alt = ac.pos.y / FT;
            assert!((alt - 4000.0).abs() < 60.0, "{:?}: alt {alt}, mode {:?}", craft as u8, ap.vertical);
            assert!((ac.heading_deg() - 120.0).abs() < 3.0, "hdg {}", ac.heading_deg());
            assert!(ac.bank_deg().abs() < 3.0, "bank {}", ac.bank_deg());
            if craft == Craft::Jet {
                assert!((ac.ias / KT - ias).abs() < 6.0, "A/T held {} kt", ac.ias / KT);
            }
        }
    }

    #[test]
    fn autolands_the_737() {
        let world = World::generate();
        let t = &world.terrain;
        // start off the centreline, on a 30 degree intercept below the glideslope
        let mut ac = Aircraft::in_flight(Craft::Jet, vec3(-14_000.0, 610.0, -1500.0), 120.0, 150.0, 0.0, 7);
        let mut ap = Autopilot::default();
        ap.sync(&ac);
        ap.engage(&ac);
        ap.toggle_at(&ac);
        ap.spd_kt = 145.0;
        ap.alt_ft = 2000.0;
        ap.hdg = 120.0;
        ap.toggle_app();
        let dt = 1.0 / 240.0;
        let mut touchdown = None;
        // the tower should clear us to land and welcome us in
        let mut tower = crate::atc::Tower::new(7);
        let weather = crate::weather::Weather::default();
        let mut calls = Vec::new();
        for i in 0..(240 * 400) {
            if i % 4 == 0 {
                let agl = (ac.pos.y - t.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height) / FT;
                ap.update(&mut ac, agl, dt * 4.0);
            }
            ac.step(dt, t);
            let agl = (ac.pos.y - t.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height) / FT;
            if std::env::var("TRACE").is_ok() && i % 480 == 0 {
                println!("x {:.0} ra {agl:.0} ga {:.2} vs {:.0} pitch {:.1} tgt {:.1} col {:.2} trim {:.2} thr {:.2} ias {:.0} {:?} {:?}", ac.pos.x, glide_angle(ac.pos), ac.vel.y / FT * 60.0, ac.pitch_deg(), ap.pitch_target, ac.ctl.pitch, ac.ctl.trim, ac.ctl.throttle, ac.ias / KT, ap.vertical, ap.lateral);
            }
            assert!(ac.crashed.is_none(), "{:?} at {:?} lat {:?} vert {:?}", ac.crashed, ac.pos, ap.lateral, ap.vertical);
            if i % 4 == 0 {
                if let Some(call) = tower.update(&ac, &ac.telem(t), &weather, dt * 4.0) {
                    calls.push(*call.last().unwrap());
                }
            }
            if touchdown.is_none() {
                touchdown = ac.touchdown;
            }
            if ac.on_ground && ac.vel.length() < 2.0 {
                break;
            }
        }
        let td = touchdown.expect("never landed");
        println!("autoland: {:.0} fpm, {:.0} m past threshold, {:.1} m off centreline, stopped at x {:.0}", td.sink_fpm, td.pos.x + RWY_HALF_LEN, td.pos.z, ac.pos.x);
        assert!(td.sink_fpm < 400.0, "hard: {} fpm", td.sink_fpm);
        assert!((150.0..900.0).contains(&(td.pos.x + RWY_HALF_LEN)), "touched down {} m in", td.pos.x + RWY_HALF_LEN);
        assert!(td.pos.z.abs() < 8.0 && ac.pos.z.abs() < 10.0, "off centreline");
        assert!(ac.pos.x < RWY_HALF_LEN, "overran");
        use crate::audio::Word;
        assert_eq!(calls, [Word::ClearedToLand, Word::Welcome]);
    }
}
