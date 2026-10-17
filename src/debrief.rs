//! Approach debrief: the last few kilometres before touchdown against the ideal 3 degree glide
//! path and the target speed, and a tip or two on what to work on next time.

use crate::flight::{Aircraft, Craft, FT, KT};
use crate::world::{PAPI_DIST, RWY_ELEV};
use macroquad::math::Vec3;
use std::collections::VecDeque;

const NM: f32 = 1852.0;

#[derive(Default)]
pub struct ApproachLog {
    /// (position, airspeed kt, time)
    samples: VecDeque<(Vec3, f32, f32)>,
    next: f32,
}

impl ApproachLog {
    /// Call every frame until the first touchdown.
    pub fn record(&mut self, ac: &Aircraft, t: f32) {
        if t + 1e-3 < self.next {
            return;
        }
        self.next = t + 0.2;
        if self.samples.len() >= 1500 {
            self.samples.pop_front();
        }
        self.samples.push_back((ac.pos, ac.ias / KT, t));
    }
}

pub struct Debrief {
    /// Distance to the aiming point along the flown path (m), height above the runway (m),
    /// airspeed (kt); farthest first.
    pub pts: Vec<(f32, f32, f32)>,
    pub span: f32,
    pub target_kt: f32,
    /// How far off the path still counts as on it (m), and how fast (kt, over the target).
    pub path_tol: f32,
    pub speed_tol: f32,
    pub tips: Vec<String>,
}

pub fn ideal_height(d: f32) -> f32 {
    d.max(0.0) * 3f32.to_radians().tan()
}

fn at(d: f32) -> String {
    if d < 0.3 * NM {
        "over the threshold".into()
    } else {
        format!("at {:.1} nm", d / NM)
    }
}

/// Build the debrief from the log, the touchdown point and how far past the runway-09
/// threshold it was. Gusts widen what counts as on speed (pilots add half the gust factor).
/// `gliding`: no engine (a sailplane, or an engine failure), so the advice can't be about
/// power. None when the approach was too short to say much.
pub fn build(log: &ApproachLog, td: Vec3, along: f32, craft: Craft, target_kt: f32, gust_kt: f32, gliding: bool) -> Option<Debrief> {
    let span = match craft {
        Craft::Jet => 3.0 * NM,
        Craft::Fighter => 2.0 * NM,
        _ => 1.6 * NM,
    };
    let (path_tol, speed_tol, sink_max) = match craft {
        Craft::Jet => (30.0, 10.0, 1100.0),
        Craft::Fighter => (35.0, 12.0, 1300.0),
        _ => (20.0, 10.0, 900.0),
    };
    let speed_tol = speed_tol + gust_kt * 0.5;
    // walk back from the touchdown along the path
    let mut pts = Vec::new();
    let mut path = 0.0;
    let mut prev = td;
    let mut sinks = Vec::new();
    let mut last: Option<(f32, f32)> = None; // (height, time) of the later sample
    // the first few seconds are the aircraft settling into the wind, not the pilot
    let settled = log.samples.front().map_or(0.0, |s| s.2) + 8.0;
    // nearest distance flown while still settling
    let mut early = f32::INFINITY;
    for (p, ias, t) in log.samples.iter().rev() {
        path += ((p.x - prev.x).powi(2) + (p.z - prev.z).powi(2)).sqrt();
        prev = *p;
        let d = path + PAPI_DIST - along;
        let h = p.y - RWY_ELEV;
        if let Some((h1, t1)) = last {
            if t1 > *t {
                sinks.push((d, (h - h1) / (t1 - t) / FT * 60.0, h));
            }
        }
        last = Some((h, *t));
        if *t < settled {
            early = early.min(d);
        }
        pts.push((d, h, *ias));
        if d > span {
            break;
        }
    }
    pts.reverse();
    let covered = pts.first().map_or(0.0, |p| p.0);
    if pts.len() < 10 || covered < span * 0.4 {
        return None;
    }

    // find the worst moments
    let window = |d: f32| d > 150.0 && d <= span && d < early;
    let worst = |f: &dyn Fn(&(f32, f32, f32)) -> f32| pts.iter().filter(|p| window(p.0)).map(|p| (f(p), p.0)).fold((0.0f32, 0.0f32), |a, b| if b.0 > a.0 { b } else { a });
    let (high, high_d) = worst(&|p| p.1 - ideal_height(p.0));
    let (low, low_d) = worst(&|p| ideal_height(p.0) - p.1);
    let (fast, fast_d) = worst(&|p| if p.0 > 300.0 { p.2 - target_kt } else { 0.0 });
    let (slow, slow_d) = worst(&|p| target_kt - p.2);
    let (sink, sink_d) = sinks.iter().filter(|s| s.0 < 0.8 * NM && s.2 > 10.0).map(|s| (s.1, s.0)).fold((0.0f32, 0.0f32), |a, b| if b.0 > a.0 { b } else { a });

    let glider = craft == Craft::Glider;
    // (how bad, tip): the biggest problems first
    let mut tips: Vec<(f32, String)> = Vec::new();
    if high > path_tol {
        let fix = match (gliding, glider) {
            (true, true) => "open the airbrakes sooner",
            (true, false) => "lower the flaps or slip to lose height",
            _ => "bring the power back earlier and let it settle",
        };
        tips.push((high / path_tol, format!("High by {:.0} ft {} - {fix}.", high / FT, at(high_d))));
    }
    if low > path_tol {
        let fix = match (gliding, glider) {
            (true, true) => "close the airbrakes and hold best-glide speed",
            (true, false) => "hold best-glide speed; pulling the nose up only sinks you faster",
            _ => "add power early; don't pull the nose up to stretch it",
        };
        tips.push((low / path_tol * 1.2, format!("Low by {:.0} ft {} - {fix}.", low / FT, at(low_d))));
    }
    if fast > speed_tol {
        tips.push((fast / speed_tol, format!("{:.0} kt fast {} - a fast approach floats down the runway.", fast, at(fast_d))));
    }
    // slow is never fine, gusts or not
    if slow > 6.0 {
        let fix = if gliding { "Lower the nose." } else { "Lower the nose, add power." };
        tips.push((slow / 4.0, format!("{:.0} kt slow {} - too close to the stall. {fix}", slow, at(slow_d))));
    }
    if sink > sink_max {
        tips.push((sink / sink_max, format!("Descending {:.0} fpm {} - steep dives near the ground end in hard landings. Get stable earlier.", sink, at(sink_d))));
    }
    tips.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut tips: Vec<String> = tips.into_iter().take(2).map(|t| t.1).collect();
    if tips.is_empty() {
        tips.push("Stable approach: on the glide path and on speed all the way down.".into());
    }
    Some(Debrief { pts, span, target_kt, path_tol, speed_tol, tips })
}

/// Approach speed to judge against when the lesson doesn't give one.
pub fn target_speed(craft: Craft, vref: Option<f32>) -> f32 {
    match (craft, vref) {
        (Craft::Jet, Some(v)) => v + 5.0,
        (_, Some(v)) => v,
        (Craft::Jet, None) => 145.0,
        (Craft::Fighter, None) => 145.0,
        (Craft::Glider, None) => 55.0,
        (Craft::Cessna, None) => 65.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::on_final;
    use crate::world::RWY_HALF_LEN;
    use macroquad::math::vec3;

    fn fly(f: impl Fn(f32) -> (f32, f32)) -> Debrief {
        // a straight-in approach to runway 09; `f(d)` gives (height error m, speed kt) at d
        let mut log = ApproachLog::default();
        let mut ac = Aircraft::in_flight(Craft::Jet, on_final(6000.0), 90.0, 145.0, -3.0, 7);
        let v = 145.0 * KT;
        let mut t = 0.0;
        let mut d = 6000.0;
        while d > 0.0 {
            let (dh, ias) = f(d);
            ac.pos = on_final(d) + vec3(0.0, dh, 0.0);
            ac.ias = ias * KT;
            log.record(&ac, t);
            t += 0.05;
            d -= v * 0.05;
        }
        build(&log, vec3(-RWY_HALF_LEN + PAPI_DIST, RWY_ELEV, 0.0), PAPI_DIST, Craft::Jet, 145.0, 0.0, false).expect("debrief")
    }

    #[test]
    fn spots_a_high_fast_approach() {
        let good = fly(|_| (0.0, 145.0));
        assert!(good.tips[0].starts_with("Stable"), "{:?}", good.tips);
        let bad = fly(|d| (if (2000.0..3500.0).contains(&d) { 80.0 } else { 0.0 }, if d < 2500.0 { 162.0 } else { 145.0 }));
        println!("{:?}", bad.tips);
        assert!(bad.tips.iter().any(|t| t.starts_with("High by")), "{:?}", bad.tips);
        assert!(bad.tips.iter().any(|t| t.contains("kt fast")), "{:?}", bad.tips);
    }
}
