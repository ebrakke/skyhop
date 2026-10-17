//! Other aircraft around the island in free flight: a trainer on a sightseeing loop of the
//! coast, two F-16s in formation, and a sailplane circling in a thermal. They fly fixed
//! paths (no physics), banked into their turns, with their lights on.

use crate::flight::{Aircraft, Craft, G, KT};
use crate::world::{Terrain, World};
use macroquad::math::{vec3, Quat, Vec3};

struct Loop {
    craft: Craft,
    /// Closed path, metres; heights already clear of the terrain.
    points: Vec<Vec3>,
    /// Cumulative distance to each point, and the total.
    dist: Vec<f32>,
    speed: f32,
    /// Formation offset from the leader in its own frame (right, up, back).
    wing: Vec3,
    start: f32,
}

pub struct Traffic {
    loops: Vec<Loop>,
    pub aircraft: Vec<Aircraft>,
}

fn ring(t: &Terrain, centre: Vec3, r: f32, n: usize, clear: f32, cw: bool) -> Vec<Vec3> {
    (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU * if cw { 1.0 } else { -1.0 };
            let (x, z) = (centre.x + a.cos() * r, centre.z + a.sin() * r);
            // stay above everything nearby
            let mut ground: f32 = 0.0;
            for s in 0..8 {
                let b = s as f32 / 8.0 * std::f32::consts::TAU;
                ground = ground.max(t.surface(x + b.cos() * 600.0, z + b.sin() * 600.0));
            }
            vec3(x, (ground + clear).max(centre.y), z)
        })
        .collect()
}

impl Traffic {
    pub fn new(world: &World) -> Self {
        let t = &world.terrain;
        let mut loops = Vec::new();
        let mut add = |craft: Craft, points: Vec<Vec3>, speed_kt: f32, wing: Vec3, start: f32| {
            let mut dist = vec![0.0];
            for k in 0..points.len() {
                let d = (points[(k + 1) % points.len()] - points[k]).length();
                dist.push(dist[k] + d);
            }
            loops.push(Loop { craft, points, dist, speed: speed_kt * KT, wing, start });
        };
        // a trainer touring the coast at about 1,500 ft
        add(Craft::Cessna, ring(t, vec3(0.0, 460.0, 0.0), 7300.0, 48, 260.0, false), 100.0, Vec3::ZERO, 0.0);
        // two F-16s on a wide loop over the sea, the wingman out to the right and behind
        let jets = ring(t, vec3(0.0, 700.0, 0.0), 10_500.0, 64, 400.0, true);
        add(Craft::Fighter, jets.clone(), 320.0, Vec3::ZERO, 900.0);
        add(Craft::Fighter, jets, 320.0, vec3(18.0, -3.0, 22.0), 900.0);
        // a sailplane working the thermal nearest the grass strip
        if let Some(th) = world.thermals.iter().min_by(|a, b| {
            let d = |th: &crate::world::Thermal| (th.x - crate::world::STRIP.0).hypot(th.z - crate::world::STRIP.1);
            d(a).total_cmp(&d(b))
        }) {
            add(Craft::Glider, ring(t, vec3(th.x, 1000.0, th.z), 110.0, 20, 300.0, false), 50.0, Vec3::ZERO, 0.0);
        }
        let aircraft = loops.iter().map(|l| Aircraft::in_flight(l.craft, l.points[0], 0.0, l.speed / KT, 0.0, 0)).collect();
        Traffic { loops, aircraft }
    }

    /// Pose everyone for time `t` (seconds).
    pub fn update(&mut self, t: f32) {
        for (l, ac) in self.loops.iter().zip(self.aircraft.iter_mut()) {
            let total = *l.dist.last().unwrap();
            let at = |s: f32| -> Vec3 {
                let s = s.rem_euclid(total);
                let k = l.dist.partition_point(|d| *d <= s).clamp(1, l.points.len());
                let (a, b) = (l.points[k - 1], l.points[k % l.points.len()]);
                let f = (s - l.dist[k - 1]) / (l.dist[k] - l.dist[k - 1]).max(1e-3);
                a.lerp(b, f)
            };
            let s = (t + l.start) * l.speed;
            let (p0, p1, p2) = (at(s - 40.0), at(s), at(s + 40.0));
            let fwd = (p2 - p0).normalize_or_zero();
            // bank from the turn: the change of direction along the path
            let turn = (p2 - p1).normalize_or_zero().cross((p1 - p0).normalize_or_zero()).y;
            let radius = 40.0 / turn.abs().max(1e-4);
            let bank = -(l.speed * l.speed / (G * radius)).atan().min(1.0) * turn.signum();
            let yaw = fwd.x.atan2(-fwd.z);
            let pitch = fwd.y.asin();
            let rot = Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(bank);
            ac.rot = rot;
            ac.pos = p1 + rot * vec3(l.wing.x, l.wing.y, l.wing.z);
            ac.vel = fwd * l.speed;
            ac.ias = l.speed;
            ac.ctl.throttle = 0.7;
            ac.n1 = 0.75;
            ac.prop_angle = (t * 40.0) % std::f32::consts::TAU;
            ac.gear = if ac.spec.retractable { 0.0 } else { 1.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_flies_clear_of_the_ground() {
        let world = World::generate();
        let mut tr = Traffic::new(&world);
        assert_eq!(tr.aircraft.len(), 4);
        let mut t = 0.0;
        while t < 1200.0 {
            tr.update(t);
            for a in &tr.aircraft {
                let ground = world.terrain.surface(a.pos.x, a.pos.z);
                assert!(a.pos.y - ground > 150.0, "{:?} {:.0} m above the ground at t={t}", a.craft, a.pos.y - ground);
                let (_, _, bank) = a.rot.to_euler(macroquad::math::EulerRot::YXZ);
                assert!(bank.abs() < 1.05, "{:?} banked {:.0} deg", a.craft, bank.to_degrees());
            }
            t += 0.5;
        }
        tr.update(2.0);
        for a in &tr.aircraft {
            eprintln!("{:?} at {:.0} {:.0} {:.0}", a.craft, a.pos.x, a.pos.y, a.pos.z);
        }
    }
}
