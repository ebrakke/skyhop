//! Landing scorecard: watches a landing from the first touch to the stop and grades it on
//! sink rate, g, touchdown point, centreline, crab, bank, pitch and the rollout, with a
//! difficulty bonus for wind, gusts, turbulence, night and low visibility.

use crate::flight::{Aircraft, G, KT};
use crate::weather::Weather;
use crate::world::{PAPI_DIST, RWY_HALF_LEN, RWY_HALF_W};

#[derive(Clone, Copy, Default)]
pub struct Touch {
    pub sink_fpm: f32,
    /// Metres past the runway 09 threshold, and off the centreline.
    pub along: f32,
    pub off: f32,
    pub ias: f32,
    pub pitch: f32,
    pub bank: f32,
    /// Nose versus actual track, degrees (crab not kicked out).
    pub crab: f32,
    pub g: f32,
    pub crosswind: f32,
}

#[derive(Default)]
pub struct LandingLog {
    pub touch: Option<Touch>,
    pub bounces: u32,
    airborne: f32,
    since_touch: f32,
    /// Load factor smoothed over a few tenths of a second (what the seat of your pants feels).
    g_felt: f32,
    /// Worst centreline deviation on the rollout, m.
    pub rollout_off: f32,
    pub stopped: Option<f32>,
}

fn lin(x: f32, good: f32, bad: f32, floor: f32) -> f32 {
    // 100 at `good` or better, falling linearly to `floor` at `bad`
    let t = ((x - good) / (bad - good)).clamp(0.0, 1.0);
    100.0 + (floor - 100.0) * t
}

impl LandingLog {
    /// Call every frame after the physics.
    pub fn update(&mut self, ac: &Aircraft, weather: &Weather, dt: f32) {
        if self.stopped.is_some() {
            return;
        }
        if ac.touchdown_event && self.touch.is_none() {
            if let Some(td) = ac.touchdown {
                let track = ac.vel.x.atan2(-ac.vel.z).to_degrees();
                let hdg = ac.heading_deg();
                let wind = weather.steady_wind(1.0);
                self.touch = Some(Touch {
                    sink_fpm: td.sink_fpm,
                    along: td.pos.x + RWY_HALF_LEN,
                    off: td.pos.z,
                    ias: td.ias_kt,
                    pitch: td.pitch,
                    bank: ac.bank_deg(),
                    crab: ((hdg - track + 540.0).rem_euclid(360.0) - 180.0),
                    g: 1.0,
                    crosswind: wind.z.abs() / KT,
                });
                self.g_felt = 1.0;
            }
        }
        let Some(t) = &mut self.touch else { return };
        self.since_touch += dt;
        self.g_felt += (ac.spec_force.y / G - self.g_felt) * (dt / 0.2).min(1.0);
        if self.since_touch < 0.8 {
            t.g = t.g.max(self.g_felt);
        }
        if !ac.on_ground {
            self.airborne += dt;
        } else {
            if self.airborne > 0.4 {
                self.bounces += 1;
            }
            self.airborne = 0.0;
            if ac.ias > 15.0 * KT {
                self.rollout_off = self.rollout_off.max(ac.pos.z.abs());
            }
            if ac.vel.length() < 1.0 {
                self.stopped = Some(ac.pos.x + RWY_HALF_LEN);
            }
        }
    }

    /// Lines of the scorecard and the final score (with the grade letter).
    pub fn card(&self, craft: crate::flight::Craft, weather: &Weather, night: f32) -> (Vec<String>, u32, &'static str) {
        use crate::flight::Craft;
        let jet = craft == Craft::Jet;
        let Some(t) = self.touch else { return (vec!["No touchdown recorded.".into()], 0, "-") };
        let vs = if jet {
            if t.sink_fpm < 60.0 { 85.0 } else { lin(t.sink_fpm, 200.0, 650.0, 10.0) }
        } else {
            lin(t.sink_fpm, 150.0, 550.0, 10.0)
        };
        let aim = PAPI_DIST;
        let tdz = if t.along < 0.0 { 0.0 } else { lin((t.along - aim).abs(), 150.0, 500.0, 20.0) };
        let cl = lin(t.off.abs(), 2.0, 12.0, 20.0);
        let crab = lin(t.crab.abs(), 2.0, 10.0, 20.0);
        let bank = lin(t.bank.abs(), 2.0, 8.0, 20.0);
        let g = lin(t.g, 1.25, 2.2, 20.0);
        let pitch = match craft {
            _ if t.pitch < 0.0 => 20.0,
            Craft::Jet => lin(t.pitch, 7.0, 9.5, 30.0),
            // a fighter lands at a high angle of attack; flat is wrong, too high scrapes
            Craft::Fighter => lin((t.pitch - 11.0).abs(), 3.0, 7.0, 30.0),
            _ => 100.0,
        };
        let roll = lin(self.rollout_off, 3.0, RWY_HALF_W - 4.0, 0.0);
        let parts = [
            ("Sink rate", format!("{:.0} fpm at {:.0} kt", t.sink_fpm, t.ias), vs, 30.0),
            ("Touchdown point", if t.along < 0.0 { format!("{:.0} m short!", -t.along) } else { format!("{:.0} m in (aim {aim:.0})", t.along) }, tdz, 15.0),
            ("Centreline", format!("{:.1} m off", t.off.abs()), cl, 10.0),
            ("Crab", format!("{:.1} deg", t.crab.abs()), crab, 10.0),
            ("Bank", format!("{:.1} deg", t.bank.abs()), bank, 10.0),
            ("Load", format!("{:.2} g", t.g), g, 10.0),
            ("Pitch", format!("{:.1} deg", t.pitch), pitch, 5.0),
            ("Rollout", format!("{:.1} m max off", self.rollout_off), roll, 10.0),
        ];
        let mut base: f32 = parts.iter().map(|p| p.2 * p.3 / 100.0).sum();
        base -= 15.0 * self.bounces as f32;
        let base = base.clamp(0.0, 100.0);
        let mult = (1.0 + t.crosswind / 40.0 + weather.gust_kt / 40.0 + weather.turbulence * 0.3 + night * 0.1 + if weather.visibility_km < 5.0 || weather.clouds > 0.8 && weather.cloud_base_ft < 1000.0 { 0.15 } else { 0.0 }).min(1.6);
        let grade = match base {
            b if b >= 95.0 => "A+",
            b if b >= 88.0 => "A",
            b if b >= 75.0 => "B",
            b if b >= 60.0 => "C",
            b if b >= 45.0 => "D",
            _ => "F",
        };
        let total = (base * mult).round() as u32;
        let mut lines = vec![format!("Grade {grade}   -   {base:.0} x {mult:.2} difficulty = {total} points"), String::new()];
        for (name, value, pts, _) in &parts {
            lines.push(format!("{name}: {value}  ({pts:.0})"));
        }
        if self.bounces > 0 {
            lines.push(format!("Bounced {} time{}  (-{})", self.bounces, if self.bounces > 1 { "s" } else { "" }, 15 * self.bounces));
        }
        if let Some(x) = self.stopped {
            lines.push(format!("Stopped with {:.0} m of runway left.", 2.0 * RWY_HALF_LEN - x));
        }
        (lines, total, grade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ap::Autopilot;
    use crate::flight::{Craft, FT};
    use crate::lessons::{all, Kind, Start};
    use crate::world::World;

    /// The autopilot autolands each 737 landing challenge; the scorecard should rate it well.
    #[test]
    fn autolanded_challenges_score_well() {
        let world = World::generate();
        let t = &world.terrain;
        for lesson in all().iter().filter(|l| l.kind == Kind::Landing && l.craft == Craft::Jet) {
            let weather = lesson.weather.unwrap_or_default();
            let Start::Air { pos, heading, ias, gamma, flaps, .. } = lesson.start else { panic!() };
            let mut ac = Aircraft::in_flight(Craft::Jet, pos, heading, ias, gamma, flaps);
            let mut ap = Autopilot::default();
            ap.sync(&ac);
            ap.engage(&ac);
            ap.toggle_at(&ac);
            ap.spd_kt = lesson.vref.unwrap_or(140.0) + 5.0 + weather.gust_kt * 0.5;
            ap.hdg = 90.0;
            ap.toggle_app();
            let mut log = LandingLog::default();
            let dt = 1.0 / 240.0;
            let mut time = 0.0;
            for i in 0..(240 * 600) {
                let agl = ac.pos.y - t.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height;
                if i % 4 == 0 {
                    ap.update(&mut ac, agl / FT, dt * 4.0);
                }
                ac.air = weather.air(ac.pos, agl, time, ac.spec.span, ac.right());
                time += dt;
                ac.step(dt, t);
                assert!(ac.crashed.is_none(), "{}: {:?}", lesson.name, ac.crashed);
                log.update(&ac, &weather, dt);
                ac.touchdown_event = false;
                if log.stopped.is_some() {
                    break;
                }
            }
            let (lines, total, grade) = log.card(Craft::Jet, &weather, 0.0);
            println!("{}: {grade} {total}\n  {}", lesson.name, lines.join("\n  "));
            assert!(log.stopped.is_some(), "{} never stopped", lesson.name);
            assert!(!matches!(grade, "D" | "F"), "{}: graded {grade}", lesson.name);
        }
    }
}
