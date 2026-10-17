//! A flight recorder: the last minute and a half of the flight at 30 frames a second, and
//! playback of it through any of the cameras, at any speed.

use crate::flight::Aircraft;
use macroquad::math::{Quat, Vec3};
use std::collections::VecDeque;

const RATE: f32 = 30.0;
const KEEP: usize = (RATE * 90.0) as usize;

#[derive(Clone, Copy)]
pub struct Pose {
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    ail: f32,
    elev: f32,
    rud: f32,
    flap_deg: f32,
    gear: f32,
    spoiler: f32,
    reverser: f32,
    prop_angle: f32,
    n1: f32,
    throttle: f32,
    on_ground: bool,
    /// Simulation seconds when recorded.
    t: f32,
}

impl Pose {
    fn of(ac: &Aircraft, t: f32) -> Self {
        Pose {
            pos: ac.pos,
            rot: ac.rot,
            vel: ac.vel,
            ail: ac.ail,
            elev: ac.elev,
            rud: ac.rud,
            flap_deg: ac.flap_deg,
            gear: ac.gear,
            spoiler: ac.spoiler,
            reverser: ac.reverser,
            prop_angle: ac.prop_angle,
            n1: ac.n1,
            throttle: ac.ctl.throttle,
            on_ground: ac.on_ground,
            t,
        }
    }

    fn lerp(&self, o: &Pose, f: f32) -> Pose {
        let l = |a: f32, b: f32| a + (b - a) * f;
        Pose {
            pos: self.pos.lerp(o.pos, f),
            rot: self.rot.slerp(o.rot, f),
            vel: self.vel.lerp(o.vel, f),
            ail: l(self.ail, o.ail),
            elev: l(self.elev, o.elev),
            rud: l(self.rud, o.rud),
            flap_deg: l(self.flap_deg, o.flap_deg),
            gear: l(self.gear, o.gear),
            spoiler: l(self.spoiler, o.spoiler),
            reverser: l(self.reverser, o.reverser),
            // the prop angle wraps, so take the nearer frame instead of blending
            prop_angle: if f < 0.5 { self.prop_angle } else { o.prop_angle },
            n1: l(self.n1, o.n1),
            throttle: l(self.throttle, o.throttle),
            on_ground: if f < 0.5 { self.on_ground } else { o.on_ground },
            t: l(self.t, o.t),
        }
    }

    /// Put the aircraft in this pose (for drawing and sound only).
    pub fn apply(&self, ac: &mut Aircraft) {
        ac.pos = self.pos;
        ac.rot = self.rot;
        ac.vel = self.vel;
        ac.omega = Vec3::ZERO;
        ac.ail = self.ail;
        ac.elev = self.elev;
        ac.rud = self.rud;
        ac.flap_deg = self.flap_deg;
        ac.gear = self.gear;
        ac.spoiler = self.spoiler;
        ac.reverser = self.reverser;
        ac.prop_angle = self.prop_angle;
        ac.n1 = self.n1;
        ac.ctl.throttle = self.throttle;
        ac.on_ground = self.on_ground;
        ac.ias = self.vel.length();
        ac.crashed = None;
    }
}

#[derive(Default)]
pub struct Recorder {
    frames: VecDeque<Pose>,
    next: f32,
}

impl Recorder {
    pub fn clear(&mut self) {
        self.frames.clear();
        self.next = 0.0;
    }

    /// Call after each frame of flight with the simulation clock.
    pub fn record(&mut self, ac: &Aircraft, t: f32) {
        if t < self.next {
            return;
        }
        self.next = t + 1.0 / RATE;
        if self.frames.len() >= KEEP {
            self.frames.pop_front();
        }
        self.frames.push_back(Pose::of(ac, t));
    }

    /// A replay of the last `secs`, or starting `lead` seconds before the first touchdown
    /// at time `touch` when there was one in the buffer.
    pub fn replay(&self, secs: f32, touch: Option<f32>) -> Option<Replay> {
        let last = self.frames.back()?.t;
        let first = self.frames.front()?.t;
        let from = match touch {
            Some(td) if td > first => (td - 15.0).max(first),
            _ => (last - secs).max(first),
        };
        let frames: Vec<Pose> = self.frames.iter().filter(|p| p.t >= from).copied().collect();
        (frames.len() > 2).then(|| Replay { t: frames[0].t, frames, speed: 1.0 })
    }
}

pub struct Replay {
    frames: Vec<Pose>,
    pub t: f32,
    pub speed: f32,
}

impl Replay {
    /// Advance and return the pose to show; loops back to the start at the end.
    pub fn advance(&mut self, dt: f32) -> Pose {
        self.t += dt * self.speed;
        let (first, last) = (self.frames[0].t, self.frames[self.frames.len() - 1].t);
        if self.t > last + 1.5 {
            self.t = first;
        }
        let k = self.frames.partition_point(|p| p.t <= self.t).clamp(1, self.frames.len() - 1);
        let (a, b) = (&self.frames[k - 1], &self.frames[k]);
        let f = ((self.t - a.t) / (b.t - a.t).max(1e-4)).clamp(0.0, 1.0);
        a.lerp(b, f)
    }

    pub fn progress(&self) -> f32 {
        let (first, last) = (self.frames[0].t, self.frames[self.frames.len() - 1].t);
        ((self.t - first) / (last - first).max(1e-3)).clamp(0.0, 1.0)
    }
}
