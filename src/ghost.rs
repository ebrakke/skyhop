//! Ghosts: your best run on a challenge, recorded from the start, saved, and flown again
//! beside you on the next try - with your time at each ring for the splits.

use crate::flight::Aircraft;
use macroquad::math::{Quat, Vec3};

const RATE: f32 = 10.0;
/// Twenty minutes is plenty for any challenge.
const MAX_FRAMES: usize = (RATE * 1200.0) as usize;
const MAGIC: &[u8; 4] = b"SKG1";

#[derive(Clone, Copy)]
struct Frame {
    t: f32,
    pos: Vec3,
    rot: Quat,
    gear: f32,
    flap: f32,
    spoiler: f32,
}

#[derive(Clone, Default)]
pub struct Run {
    frames: Vec<Frame>,
    /// Time at each ring.
    pub splits: Vec<f32>,
    next: f32,
}

impl Run {
    /// Call every frame of the attempt with the time since it started.
    pub fn record(&mut self, ac: &Aircraft, t: f32) {
        if t + 1e-3 < self.next || self.frames.len() >= MAX_FRAMES {
            return;
        }
        self.next = t + 1.0 / RATE;
        self.frames.push(Frame { t, pos: ac.pos, rot: ac.rot, gear: ac.gear, flap: ac.flap_deg, spoiler: ac.spoiler });
    }

    pub fn is_empty(&self) -> bool {
        self.frames.len() < 2
    }

    /// Pose `ac` where the run was at time `t`. False once it's over (and a moment after).
    pub fn pose(&self, t: f32, ac: &mut Aircraft) -> bool {
        if self.is_empty() || t > self.frames[self.frames.len() - 1].t + 3.0 {
            return false;
        }
        let k = self.frames.partition_point(|f| f.t <= t).clamp(1, self.frames.len() - 1);
        let (a, b) = (&self.frames[k - 1], &self.frames[k]);
        let f = ((t - a.t) / (b.t - a.t).max(1e-4)).clamp(0.0, 1.0);
        ac.pos = a.pos.lerp(b.pos, f);
        ac.rot = a.rot.slerp(b.rot, f);
        ac.vel = (b.pos - a.pos) / (b.t - a.t).max(1e-4);
        ac.gear = a.gear + (b.gear - a.gear) * f;
        ac.flap_deg = a.flap + (b.flap - a.flap) * f;
        ac.spoiler = a.spoiler + (b.spoiler - a.spoiler) * f;
        ac.prop_angle = (t * 45.0) % std::f32::consts::TAU;
        ac.ctl.throttle = 0.7;
        ac.n1 = 0.6;
        true
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((self.frames.len() as u32).to_le_bytes());
        out.extend((self.splits.len() as u32).to_le_bytes());
        for f in &self.frames {
            for v in [f.t, f.pos.x, f.pos.y, f.pos.z, f.rot.x, f.rot.y, f.rot.z, f.rot.w, f.gear, f.flap, f.spoiler] {
                out.extend(v.to_le_bytes());
            }
        }
        for s in &self.splits {
            out.extend(s.to_le_bytes());
        }
        out
    }

    fn from_bytes(b: &[u8]) -> Option<Run> {
        if b.len() < 12 || &b[..4] != MAGIC {
            return None;
        }
        let word = |i: usize| -> Option<[u8; 4]> { b.get(i..i + 4)?.try_into().ok() };
        let (n, s) = (u32::from_le_bytes(word(4)?) as usize, u32::from_le_bytes(word(8)?) as usize);
        if n > MAX_FRAMES || s > 1000 || b.len() != 12 + n * 44 + s * 4 {
            return None;
        }
        let f = |i: usize| f32::from_le_bytes(word(i).unwrap());
        let frames = (0..n)
            .map(|k| {
                let o = 12 + k * 44;
                let v: Vec<f32> = (0..11).map(|j| f(o + j * 4)).collect();
                Frame { t: v[0], pos: Vec3::new(v[1], v[2], v[3]), rot: Quat::from_xyzw(v[4], v[5], v[6], v[7]).normalize(), gear: v[8], flap: v[9], spoiler: v[10] }
            })
            .collect();
        let splits = (0..s).map(|k| f(12 + n * 44 + k * 4)).collect();
        Some(Run { frames, splits, next: 0.0 })
    }
}

fn path(name: &str) -> Option<std::path::PathBuf> {
    let file: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    Some(crate::setup::data_dir()?.join("ghosts").join(format!("{file}.ghost")))
}

pub fn save(name: &str, run: &Run) {
    let Some(p) = path(name) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, run.to_bytes());
}

pub fn load(name: &str) -> Option<Run> {
    Run::from_bytes(&std::fs::read(path(name)?).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::Craft;
    use macroquad::math::vec3;

    #[test]
    fn ghost_round_trips() {
        let mut ac = Aircraft::in_flight(Craft::Cessna, vec3(0.0, 300.0, 0.0), 90.0, 90.0, 0.0, 0);
        let mut run = Run::default();
        for k in 0..100 {
            ac.pos.x += 5.0;
            run.record(&ac, k as f32 * 0.1);
        }
        run.splits = vec![3.5, 7.25];
        let back = Run::from_bytes(&run.to_bytes()).expect("decodes");
        assert_eq!(back.frames.len(), run.frames.len());
        assert_eq!(back.splits, run.splits);
        let mut g = ac.clone();
        assert!(back.pose(4.95, &mut g));
        assert!((g.pos.x - (ac.pos.x - 500.0 + 5.0 * 50.5)).abs() < 1.0, "x {}", g.pos.x);
        assert!(!back.pose(20.0, &mut g));
        assert!(Run::from_bytes(b"SKG1garbage").is_none());
    }
}
