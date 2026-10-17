//! Point lights drawn as glowing sprites: the airport lighting system (approach lights with
//! the sequenced "rabbit", runway edge, centreline, touchdown zone and threshold lights),
//! taxiway and apron lights, the airport beacon, town lights and the aircraft's own lights.

use crate::flight::Aircraft;
use crate::world::*;
use macroquad::math::{vec3, Vec3};

#[derive(Clone, Copy, PartialEq)]
pub enum Blink {
    Steady,
    /// On for `on` seconds every `period`, starting at `phase`.
    Flash { period: f32, phase: f32, on: f32 },
    /// Aircraft strobe: two quick flashes a second apart.
    Strobe { phase: f32 },
}

#[derive(Clone, Copy)]
pub struct Lamp {
    pub pos: Vec3,
    pub col: [f32; 3],
    /// Smallest world size (m); far lamps grow to stay a few pixels wide.
    pub size: f32,
    pub blink: Blink,
    /// Only seen from in front of this direction (runway lights shine toward the pilot).
    pub facing: Option<Vec3>,
    /// Shows in daylight too (strobes, beacons, PAPI); otherwise fades in at dusk.
    pub day: bool,
}

const WHITE: [f32; 3] = [1.0, 0.95, 0.85];
const YELLOW: [f32; 3] = [1.0, 0.78, 0.35];
const RED: [f32; 3] = [1.0, 0.15, 0.1];
const GREEN: [f32; 3] = [0.25, 1.0, 0.45];
const BLUE: [f32; 3] = [0.25, 0.45, 1.0];

fn lamp(pos: Vec3, col: [f32; 3], size: f32) -> Lamp {
    Lamp { pos, col, size, blink: Blink::Steady, facing: None, day: false }
}

fn facing(mut l: Lamp, dir: Vec3) -> Lamp {
    l.facing = Some(dir);
    l
}

impl Lamp {
    /// Brightness 0..1 at time `t`.
    pub fn level(&self, t: f32) -> f32 {
        match self.blink {
            Blink::Steady => 1.0,
            Blink::Flash { period, phase, on } => {
                if ((t + phase) / period).fract() * period < on {
                    1.0
                } else {
                    0.0
                }
            }
            Blink::Strobe { phase } => {
                let x = (t + phase).rem_euclid(1.2);
                if x < 0.05 || (0.14..0.19).contains(&x) {
                    1.6
                } else {
                    0.0
                }
            }
        }
    }
}

/// Lights for one landing direction: `end` is the threshold x, `dir` +1 landing east (09), -1 west
/// (27). The full system has red side rows; the other end gets a simpler one.
fn runway_end(lamps: &mut Vec<Lamp>, t: &Terrain, end: f32, dir: f32, full_als: bool) {
    let toward_pilot = vec3(-dir, 0.0, 0.0); // these shine back down the approach
    let ground = |x: f32, z: f32| t.height(x, z).max(SEA) + 0.6;
    // threshold: green bar facing the approach; red runway-end lights facing the other way
    let mut z = -RWY_HALF_W - 4.0;
    while z <= RWY_HALF_W + 4.0 {
        lamps.push(facing(lamp(vec3(end, RWY_ELEV + 0.3, z), GREEN, 0.5), toward_pilot));
        lamps.push(facing(lamp(vec3(end, RWY_ELEV + 0.3, z), RED, 0.4), -toward_pilot));
        z += 3.0;
    }
    // touchdown zone barrettes, 30 m apart for 900 m
    let mut d = 30.0;
    while d <= 900.0 {
        for side in [-1.0f32, 1.0] {
            for k in 0..3 {
                let z = side * (9.0 + k as f32 * 1.5);
                lamps.push(facing(lamp(vec3(end + dir * d, RWY_ELEV + 0.15, z), WHITE, 0.35), toward_pilot));
            }
        }
        d += 30.0;
    }
    // approach lights out over the ground before the threshold
    let len = 730.0;
    let mut d = 30.0;
    let rabbit_from = 300.0;
    let flashers = ((len - rabbit_from) / 30.0) as u32 + 1;
    while d <= len + 1.0 {
        let x = end - dir * d;
        // centreline barrette of five
        for k in 0..5 {
            let z = (k as f32 - 2.0) * 1.0;
            lamps.push(facing(lamp(vec3(x, ground(x, z), z), WHITE, 0.6), toward_pilot));
        }
        // red side rows on the inner 300 m
        if full_als && d <= 300.0 {
            for side in [-1.0f32, 1.0] {
                for k in 0..3 {
                    let z = side * (11.0 + k as f32 * 1.5);
                    lamps.push(facing(lamp(vec3(x, ground(x, z), z), RED, 0.5), toward_pilot));
                }
            }
        }
        // the crossbar at 300 m
        if (d - 300.0).abs() < 1.0 {
            for k in 0..20 {
                let z = (k as f32 - 9.5) * 1.6;
                if z.abs() > 3.0 {
                    lamps.push(facing(lamp(vec3(x, ground(x, z), z), WHITE, 0.6), toward_pilot));
                }
            }
        }
        // sequenced flashers race toward the runway twice a second
        if d >= rabbit_from - 1.0 {
            let i = ((len - d) / 30.0).round() as u32;
            let mut l = facing(lamp(vec3(x, ground(x, 0.0) + 1.0, 0.0), [1.0, 1.0, 1.0], 1.2), toward_pilot);
            l.blink = Blink::Flash { period: 0.5, phase: -(i as f32) / flashers as f32 * 0.5, on: 0.04 };
            lamps.push(l);
        }
        d += 30.0;
    }
}

/// Everything fixed in the world.
pub fn static_lamps(world: &World) -> Vec<Lamp> {
    let t = &world.terrain;
    let mut lamps = Vec::new();
    runway_end(&mut lamps, t, -RWY_HALF_LEN, 1.0, true);
    runway_end(&mut lamps, t, RWY_HALF_LEN, -1.0, false);
    // edge lights every 60 m, yellow over the last 600 m in each direction
    let mut x = -RWY_HALF_LEN;
    while x <= RWY_HALF_LEN + 1.0 {
        for z in [-RWY_HALF_W - 1.5, RWY_HALF_W + 1.5] {
            let p = vec3(x, RWY_ELEV + 0.4, z);
            let east_end = x > RWY_HALF_LEN - 600.0;
            let west_end = x < -RWY_HALF_LEN + 600.0;
            lamps.push(facing(lamp(p, if east_end { YELLOW } else { WHITE }, 0.45), vec3(-1.0, 0.0, 0.0)));
            lamps.push(facing(lamp(p, if west_end { YELLOW } else { WHITE }, 0.45), vec3(1.0, 0.0, 0.0)));
        }
        x += 60.0;
    }
    // centreline every 15 m: red/white then red over the last 900 / 300 m
    let mut x = -RWY_HALF_LEN + 15.0;
    let mut k = 0;
    while x < RWY_HALF_LEN - 1.0 {
        let p = vec3(x, RWY_ELEV + 0.12, 0.0);
        let colour = |remaining: f32, k: i32| {
            if remaining < 300.0 {
                RED
            } else if remaining < 900.0 && k % 2 == 1 {
                RED
            } else {
                WHITE
            }
        };
        lamps.push(facing(lamp(p, colour(RWY_HALF_LEN - x, k), 0.3), vec3(-1.0, 0.0, 0.0)));
        lamps.push(facing(lamp(p, colour(x + RWY_HALF_LEN, k), 0.3), vec3(1.0, 0.0, 0.0)));
        x += 15.0;
        k += 1;
    }
    // taxiway to the apron: blue edges, green centreline
    let mut z = -RWY_HALF_W - 4.0;
    while z > -46.0 {
        lamps.push(lamp(vec3(119.0, RWY_ELEV + 0.3, z), BLUE, 0.35));
        lamps.push(lamp(vec3(141.0, RWY_ELEV + 0.3, z), BLUE, 0.35));
        lamps.push(lamp(vec3(130.0, RWY_ELEV + 0.1, z), GREEN, 0.25));
        z -= 8.0;
    }
    let mut x = 60.0;
    while x <= 260.0 {
        lamps.push(lamp(vec3(x, RWY_ELEV + 0.3, -44.0), BLUE, 0.35));
        x += 12.0;
    }
    // apron floodlights on poles
    for (x, z) in [(70.0, -90.0), (150.0, -92.0), (250.0, -90.0)] {
        lamps.push(lamp(vec3(x, RWY_ELEV + 18.0, z), [1.0, 0.85, 0.6], 2.5));
    }
    // airport beacon on the tower: white and green alternating
    let top = TOWER_POS + vec3(0.0, 5.0, 0.0);
    lamps.push(Lamp { blink: Blink::Flash { period: 2.0, phase: 0.0, on: 0.12 }, day: true, ..lamp(top, [1.0, 1.0, 0.95], 2.0) });
    lamps.push(Lamp { blink: Blink::Flash { period: 2.0, phase: 1.0, on: 0.12 }, day: true, ..lamp(top, GREEN, 2.0) });
    // red obstruction lights on the wind turbines, all flashing together
    for p in &world.turbines {
        lamps.push(Lamp { blink: Blink::Flash { period: 1.5, phase: 0.0, on: 0.5 }, ..lamp(*p + vec3(0.0, 84.5, 0.0), [1.0, 0.1, 0.06], 1.5) });
    }
    // the lighthouse lantern
    let (lx, lz) = LIGHTHOUSE;
    lamps.push(lamp(vec3(lx, world.terrain.height(lx, lz) + 32.5, lz), [1.0, 0.95, 0.8], 3.0));
    // windows and street lamps in the villages (the first buildings are the airport's)
    let mut rng = Rng::new(17);
    for b in world.buildings.iter().skip(5) {
        if rng.f() < 0.75 {
            let warm = [1.0, 0.72 + rng.f() * 0.15, 0.38 + rng.f() * 0.2];
            lamps.push(lamp(b.pos + vec3(rng.range(-2.0, 2.0), b.size.y * 0.45, rng.range(-2.0, 2.0)), warm, 0.9));
        }
        if rng.f() < 0.35 {
            lamps.push(lamp(b.pos + vec3(rng.range(-12.0, 12.0), 6.0, rng.range(-12.0, 12.0)), [1.0, 0.6, 0.25], 1.2));
        }
    }
    lamps
}

/// The aircraft's navigation lights, beacon and strobes.
pub fn aircraft_lamps(ac: &Aircraft, out: &mut Vec<Lamp>) {
    let spec = ac.spec;
    let at = |p: Vec3| ac.pos + ac.rot * p;
    let fwd = ac.forward();
    let (left, right) = spec.wingtips();
    let size = (spec.span / 11.0).sqrt() * 0.35;
    out.push(facing(lamp(at(left), [1.0, 0.1, 0.08], size), -ac.right() + fwd * 0.3));
    out.push(facing(lamp(at(right), [0.1, 1.0, 0.3], size), ac.right() + fwd * 0.3));
    out.push(facing(lamp(at(spec.tail_light()), [1.0, 1.0, 1.0], size), -fwd));
    for p in [left, right] {
        out.push(Lamp { blink: Blink::Strobe { phase: 0.0 }, day: true, ..lamp(at(p), [1.0, 1.0, 1.0], size * 1.5) });
    }
    let beacon = spec.beacon();
    out.push(Lamp { blink: Blink::Flash { period: 1.1, phase: 0.0, on: 0.12 }, day: true, ..lamp(at(beacon), [1.0, 0.12, 0.08], size * 1.2) });
}
