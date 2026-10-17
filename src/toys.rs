//! Kids mode's other things to fly: a little helicopter and a hot air balloon. Both stand on
//! the origin (skids / basket bottom) and face -z.

use crate::mesh::{rgb, Col, MeshData};
use crate::world::smoothstep;
use macroquad::math::{vec3, Vec3};
use std::f32::consts::{PI, TAU};

/// Smooth-shaded ellipsoid.
fn ellipsoid(m: &mut MeshData, c: Vec3, r: Vec3, col: Col) {
    let (nu, nv) = (18, 10);
    let base = m.v.len() as u32;
    for v in 0..=nv {
        let th = v as f32 / nv as f32 * PI;
        for u in 0..=nu {
            let ph = u as f32 / nu as f32 * TAU;
            let n = vec3(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin());
            m.vert(c + n * r, (n / r).normalize(), col);
        }
    }
    for v in 0..nv {
        for u in 0..nu {
            let a = base + (v * (nu + 1) + u) as u32;
            let b = a + (nu + 1) as u32;
            m.i.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
}

/// Main rotor hub height, and the tail rotor's centre.
pub const ROTOR_HUB: Vec3 = vec3(0.0, 3.25, 0.0);
pub const TAIL_HUB: Vec3 = vec3(0.32, 2.75, 5.9);

/// A round little helicopter: (body, main rotor turning about +y at `ROTOR_HUB`, tail rotor
/// turning about +x at `TAIL_HUB`).
pub fn helicopter() -> (MeshData, MeshData, MeshData) {
    let red = rgb(0.92, 0.22, 0.2);
    let yellow = rgb(1.0, 0.8, 0.15);
    let glass = rgb(0.55, 0.78, 0.95);
    let dark = rgb(0.2, 0.2, 0.22);
    let mut b = MeshData::default();
    ellipsoid(&mut b, vec3(0.0, 1.75, 0.2), vec3(1.15, 1.05, 1.9), red);
    ellipsoid(&mut b, vec3(0.0, 1.95, -0.85), vec3(1.0, 0.92, 1.15), glass);
    // tail boom, fin and tailplane
    b.cylinder(vec3(0.0, 2.15, 3.6), vec3(0.0, 0.1, 1.0).normalize(), 0.26, 4.6, 10, red, red);
    b.hexa(
        [vec3(-0.06, 2.3, 5.4), vec3(0.06, 2.3, 5.4), vec3(0.06, 2.3, 6.3), vec3(-0.06, 2.3, 6.3), vec3(-0.06, 3.5, 6.0), vec3(0.06, 3.5, 6.0), vec3(0.06, 3.5, 6.5), vec3(-0.06, 3.5, 6.5)],
        yellow,
    );
    b.aabb(vec3(-1.0, 2.25, 5.0), vec3(1.0, 2.35, 5.5), yellow);
    // skids and their struts
    for x in [-0.95f32, 0.95] {
        b.cylinder(vec3(x, 0.12, 0.0), Vec3::Z, 0.09, 3.8, 8, dark, dark);
        b.cone(vec3(x, 0.12, -1.9), Vec3::NEG_Z, 0.09, 0.3, 8, dark);
        for z in [-0.8f32, 0.9] {
            b.beam(vec3(x, 0.12, z), vec3(x * 0.55, 1.0, z), 0.09, dark);
        }
    }
    // mast
    b.cylinder(vec3(0.0, 3.0, 0.1), Vec3::Y, 0.14, 0.55, 8, dark, dark);
    // two-bladed rotors: one bar through the hub
    let mut rotor = MeshData::default();
    rotor.aabb(vec3(-0.17, -0.04, -4.6), vec3(0.17, 0.04, 4.6), dark);
    rotor.cylinder(Vec3::ZERO, Vec3::Y, 0.25, 0.22, 10, yellow, yellow);
    let mut tail = MeshData::default();
    tail.aabb(vec3(-0.03, -0.75, -0.07), vec3(0.03, 0.75, 0.07), dark);
    (b, rotor, tail)
}

/// Where the burner's flame starts, pointing up.
pub const BURNER: Vec3 = vec3(0.0, 3.3, 0.0);

/// A rainbow-striped hot air balloon with a wicker basket: (balloon, flame). The flame is a
/// unit-tall cone to scale.
pub fn hot_air() -> (MeshData, MeshData) {
    let mut m = MeshData::default();
    let wicker = rgb(0.62, 0.42, 0.22);
    let rim = rgb(0.42, 0.27, 0.14);
    let rope = rgb(0.85, 0.8, 0.7);
    m.aabb(vec3(-0.85, 0.0, -0.85), vec3(0.85, 1.05, 0.85), wicker);
    m.aabb(vec3(-0.92, 1.0, -0.92), vec3(0.92, 1.15, 0.92), rim);
    // ropes up to the envelope's mouth, and the burner frame
    for (x, z) in [(-0.85f32, -0.85f32), (0.85, -0.85), (0.85, 0.85), (-0.85, 0.85)] {
        m.beam(vec3(x, 1.1, z), vec3(x * 1.7, 4.3, z * 1.7), 0.05, rope);
    }
    m.cylinder(vec3(0.0, 3.15, 0.0), Vec3::Y, 0.3, 0.35, 10, rgb(0.6, 0.6, 0.62), rgb(0.3, 0.3, 0.3));
    // the envelope: twelve gores in rainbow colours, round on top and tapering to the mouth
    let colors = [rgb(1.0, 0.25, 0.25), rgb(1.0, 0.6, 0.1), rgb(1.0, 0.9, 0.2), rgb(0.3, 0.82, 0.35), rgb(0.25, 0.55, 1.0), rgb(0.7, 0.4, 0.95)];
    let gores = 12;
    let rows = 14;
    let r = 6.8;
    let centre = 11.6;
    let profile = |t: f32| {
        // t from 0 at the crown to 1 at the mouth
        let th = t * PI * 0.78;
        let round = vec3(th.sin() * r, centre + th.cos() * r * 1.05, 0.0);
        let mouth = vec3(1.5, 4.3, 0.0);
        let k = smoothstep(0.62, 1.0, t);
        round.lerp(mouth, k)
    };
    for g in 0..gores {
        let col = colors[g % colors.len()];
        let base = m.v.len() as u32;
        for v in 0..=rows {
            let p = profile(v as f32 / rows as f32);
            for e in 0..=1 {
                let a = (g + e) as f32 / gores as f32 * TAU;
                let pos = vec3(p.x * a.cos(), p.y, p.x * a.sin());
                let n = vec3(a.cos(), ((p.y - centre) / r).clamp(-1.0, 1.0), a.sin()).normalize();
                m.vert(pos, n, col);
            }
        }
        for v in 0..rows as u32 {
            let a = base + v * 2;
            m.i.extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }
    let mut flame = MeshData::default();
    flame.cone(Vec3::ZERO, Vec3::Y, 0.28, 1.0, 10, rgb(1.0, 0.75, 0.25));
    (m, flame)
}
