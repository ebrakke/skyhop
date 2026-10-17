//! Things around the island beyond the terrain and the runway: taxiways and the terminal
//! apron, the grass strip's markers, wind turbines, the lighthouse, windsocks, boats, and
//! aircraft parked at the gates. Static parts are built once; moving parts are posed each frame.

use crate::flight::Craft;
use crate::mesh::{rgb, MeshData};
use crate::world::*;
use macroquad::math::{vec3, Mat4, Quat, Vec3};

fn flat(m: &mut MeshData, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, c: [f32; 4]) {
    let a = m.vert(vec3(x0, y, z0), Vec3::Y, c);
    m.vert(vec3(x1, y, z0), Vec3::Y, c);
    m.vert(vec3(x1, y, z1), Vec3::Y, c);
    m.vert(vec3(x0, y, z1), Vec3::Y, c);
    m.i.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
}

/// Taxiways and aprons, drawn like the runway (just above the flattened ground).
pub fn pavement() -> MeshData {
    let mut m = MeshData::default();
    let y = RWY_ELEV + 0.05;
    let tw = rgb(0.31, 0.31, 0.32);
    let apron = rgb(0.52, 0.52, 0.5);
    // parallel taxiway north of the runway, with links at both ends and every 700 m
    flat(&mut m, -RWY_HALF_LEN, -84.0, RWY_HALF_LEN, -66.0, y, tw);
    for x in [-RWY_HALF_LEN + 10.0, -700.0, 700.0, RWY_HALF_LEN - 10.0] {
        flat(&mut m, x - 11.0, -66.0, x + 11.0, -RWY_HALF_W, y, tw);
    }
    // terminal apron
    flat(&mut m, 460.0, -156.0, 860.0, -84.0, y + 0.01, apron);
    // yellow taxi line down the middle of the parallel taxiway
    let line = rgb(0.9, 0.75, 0.2);
    flat(&mut m, -RWY_HALF_LEN, -75.4, RWY_HALF_LEN, -74.6, y + 0.02, line);
    m
}

/// Everything fixed: turbine towers, the lighthouse, the harbour pier, strip markers and
/// windsock poles.
pub fn props(world: &World) -> MeshData {
    let mut m = MeshData::default();
    let t = &world.terrain;
    let white = rgb(0.92, 0.92, 0.9);
    for p in &world.turbines {
        m.cylinder(*p + vec3(0.0, 40.0, 0.0), Vec3::Y, 2.0, 82.0, 10, white, white);
    }
    // lighthouse: red and white bands, a gallery, a lantern and a red cap
    let (lx, lz) = LIGHTHOUSE;
    let base = vec3(lx, t.height(lx, lz) - 1.0, lz);
    for k in 0..6 {
        let c = if k % 2 == 0 { white } else { rgb(0.8, 0.12, 0.1) };
        let r = 4.0 - k as f32 * 0.25;
        m.cylinder(base + vec3(0.0, 2.5 + k as f32 * 5.0, 0.0), Vec3::Y, r, 5.0, 14, c, c);
    }
    m.cylinder(base + vec3(0.0, 30.4, 0.0), Vec3::Y, 3.6, 0.8, 14, rgb(0.2, 0.2, 0.2), rgb(0.2, 0.2, 0.2));
    m.cylinder(base + vec3(0.0, 32.5, 0.0), Vec3::Y, 2.2, 3.5, 10, rgb(0.15, 0.2, 0.25), rgb(0.15, 0.2, 0.25));
    m.cone(base + vec3(0.0, 34.2, 0.0), Vec3::Y, 2.6, 2.4, 10, rgb(0.75, 0.1, 0.08));
    m.aabb(base + vec3(4.0, 0.0, -5.0), base + vec3(14.0, 5.0, 5.0), white);
    // harbour pier: from the shore out into the sea
    let (start, dir) = pier(t);
    let end = start + dir * 170.0;
    let side = vec3(-dir.z, 0.0, dir.x) * 5.0;
    m.hexa(
        [
            start - side + vec3(0.0, -2.0, 0.0),
            start + side + vec3(0.0, -2.0, 0.0),
            end + side + vec3(0.0, -2.0, 0.0),
            end - side + vec3(0.0, -2.0, 0.0),
            start - side + vec3(0.0, 1.6, 0.0),
            start + side + vec3(0.0, 1.6, 0.0),
            end + side + vec3(0.0, 1.6, 0.0),
            end - side + vec3(0.0, 1.6, 0.0),
        ],
        rgb(0.62, 0.6, 0.56),
    );
    // grass strip: white edge markers and threshold bars
    let (sx, sz) = STRIP;
    let mut x = sx - STRIP_HALF_LEN;
    while x <= sx + STRIP_HALF_LEN + 1.0 {
        for z in [sz - STRIP_HALF_W - 1.0, sz + STRIP_HALF_W + 1.0] {
            m.cone(vec3(x, STRIP_ELEV - 0.1, z), Vec3::Y, 0.35, 0.7, 6, white);
        }
        x += 60.0;
    }
    for end in [sx - STRIP_HALF_LEN, sx + STRIP_HALF_LEN] {
        for k in 0..4 {
            let z = sz - 9.0 + k as f32 * 6.0;
            m.aabb(vec3(end - 1.0, STRIP_ELEV - 0.1, z - 1.5), vec3(end + 1.0, STRIP_ELEV + 0.15, z + 1.5), white);
        }
    }
    for (p, _) in windsocks() {
        m.cylinder(p - vec3(0.0, 3.0, 0.0), Vec3::Y, 0.08, 6.0, 6, rgb(0.85, 0.85, 0.85), white);
    }
    m
}

/// Where the pier leaves the shore, and the direction out to sea.
pub fn pier(t: &Terrain) -> (Vec3, Vec3) {
    let dir = vec3(0.62, 0.0, 0.78).normalize();
    // walk out from the town until the ground meets the sea
    let mut p = vec3(PORT.0, 0.0, PORT.1);
    for _ in 0..200 {
        if t.height(p.x, p.z) < 0.5 {
            break;
        }
        p += dir * 10.0;
    }
    (p - dir * 15.0, dir)
}

/// Windsock pole tops (the sock hangs from here) and their airfield's elevation.
pub fn windsocks() -> [(Vec3, f32); 2] {
    [
        (vec3(-RWY_HALF_LEN + 250.0, RWY_ELEV + 6.0, RWY_HALF_W + 60.0), RWY_ELEV),
        (vec3(STRIP.0 - STRIP_HALF_LEN + 80.0, STRIP_ELEV + 6.0, STRIP.1 - STRIP_HALF_W - 30.0), STRIP_ELEV),
    ]
}

/// An orange-and-white striped sock, pointing along +z from the origin.
pub fn sock() -> MeshData {
    let mut m = MeshData::default();
    for k in 0..5 {
        let c = if k % 2 == 0 { rgb(1.0, 0.45, 0.1) } else { rgb(0.95, 0.95, 0.95) };
        let r = 0.55 - k as f32 * 0.07;
        m.cylinder(vec3(0.0, 0.0, 0.4 + k as f32 * 0.75), Vec3::Z, r, 0.75, 10, c, c);
    }
    m
}

/// Nacelle and three blades of a turbine; the rotor turns about +z, facing -z.
pub fn turbine_head() -> (MeshData, MeshData) {
    let white = rgb(0.92, 0.92, 0.9);
    let mut nac = MeshData::default();
    nac.aabb(vec3(-1.8, -1.8, -2.0), vec3(1.8, 1.8, 8.0), white);
    nac.cone(vec3(0.0, 0.0, -2.0), Vec3::NEG_Z, 1.6, 2.2, 10, white);
    let mut rotor = MeshData::default();
    for k in 0..3 {
        let a = k as f32 / 3.0 * std::f32::consts::TAU;
        let d = vec3(a.cos(), a.sin(), 0.0);
        let s = vec3(-d.y, d.x, 0.0);
        rotor.hexa(
            [
                d * 1.5 - s * 1.4 + vec3(0.0, 0.0, -0.3),
                d * 42.0 - s * 0.5 + vec3(0.0, 0.0, -0.1),
                d * 42.0 + s * 0.4 + vec3(0.0, 0.0, -0.1),
                d * 1.5 + s * 1.0 + vec3(0.0, 0.0, -0.3),
                d * 1.5 - s * 1.4 + vec3(0.0, 0.0, 0.3),
                d * 42.0 - s * 0.5 + vec3(0.0, 0.0, 0.1),
                d * 42.0 + s * 0.4 + vec3(0.0, 0.0, 0.1),
                d * 1.5 + s * 1.0 + vec3(0.0, 0.0, 0.3),
            ],
            white,
        );
    }
    (nac, rotor)
}

/// A long cone of light along +z from the origin (the lighthouse beam).
pub fn beam() -> MeshData {
    let mut m = MeshData::default();
    let n = 10;
    let len = 1400.0;
    for k in 0..n {
        let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
        let rim = |a: f32| vec3(a.cos() * 45.0, a.sin() * 18.0, len);
        let near = [1.0, 0.95, 0.8, 1.0];
        let far = [1.0, 0.95, 0.8, 0.0];
        let i0 = m.vert(Vec3::ZERO, Vec3::Y, near);
        m.vert(rim(a0), Vec3::Y, far);
        m.vert(rim(a1), Vec3::Y, far);
        m.i.extend_from_slice(&[i0, i0 + 1, i0 + 2]);
    }
    m
}

pub fn sailboat() -> MeshData {
    let mut m = MeshData::default();
    let hull = rgb(0.95, 0.95, 0.95);
    m.hexa(
        [
            vec3(-1.2, -0.6, -3.5),
            vec3(1.2, -0.6, -3.5),
            vec3(1.0, -0.6, 4.0),
            vec3(-1.0, -0.6, 4.0),
            vec3(-1.5, 0.7, -4.2),
            vec3(1.5, 0.7, -4.2),
            vec3(1.3, 0.7, 4.3),
            vec3(-1.3, 0.7, 4.3),
        ],
        hull,
    );
    m.cone(vec3(0.0, -0.4, -4.2), Vec3::NEG_Z, 0.9, 1.6, 6, hull);
    m.beam(vec3(0.0, 0.7, -1.0), vec3(0.0, 11.0, -1.0), 0.15, rgb(0.6, 0.6, 0.6));
    let sail = rgb(0.98, 0.97, 0.94);
    m.tri(vec3(0.0, 1.5, -0.9), vec3(0.0, 10.6, -0.9), vec3(0.0, 1.5, 3.4), sail, None);
    m.tri(vec3(0.0, 1.5, -1.2), vec3(0.0, 9.8, -1.2), vec3(0.0, 1.0, -4.0), [0.85, 0.3, 0.25, 1.0], None);
    m
}

pub fn ferry() -> MeshData {
    let mut m = MeshData::default();
    let hull = rgb(0.18, 0.28, 0.5);
    m.hexa(
        [
            vec3(-6.0, -2.5, -25.0),
            vec3(6.0, -2.5, -25.0),
            vec3(6.0, -2.5, 25.0),
            vec3(-6.0, -2.5, 25.0),
            vec3(-7.0, 3.0, -30.0),
            vec3(7.0, 3.0, -30.0),
            vec3(7.0, 3.0, 26.0),
            vec3(-7.0, 3.0, 26.0),
        ],
        hull,
    );
    m.aabb(vec3(-5.5, 3.0, -16.0), vec3(5.5, 9.0, 18.0), rgb(0.95, 0.95, 0.95));
    m.aabb(vec3(-4.0, 9.0, -12.0), vec3(4.0, 12.5, 4.0), rgb(0.95, 0.95, 0.95));
    m.cylinder(vec3(0.0, 14.0, 8.0), Vec3::Y, 1.4, 4.0, 10, rgb(0.85, 0.15, 0.1), rgb(0.1, 0.1, 0.1));
    m
}

/// Where the boats are at time `t`: (which mesh: 0 sailboat / 1 ferry, transform). The ferry
/// runs from the end of the pier (`start` and `dir` from `pier`).
pub fn boats(t: f32, start: Vec3, dir: Vec3) -> Vec<(usize, Mat4)> {
    let mut out = Vec::new();
    // sailboats tacking lazily around the southern bay
    for k in 0..6 {
        let c = vec3(600.0 + k as f32 * 520.0, 0.0, 8350.0 + (k % 3) as f32 * 260.0);
        let r = 220.0 + k as f32 * 35.0;
        let w = 0.012 * if k % 2 == 0 { 1.0 } else { -1.0 };
        let a = t * w + k as f32 * 1.3;
        let p = c + vec3(a.cos(), 0.0, a.sin()) * r;
        let tangent = vec3(-a.sin(), 0.0, a.cos()) * w.signum();
        let yaw = tangent.x.atan2(-tangent.z);
        let heel = Quat::from_rotation_z(0.12 * w.signum() + 0.03 * (t * 0.8 + k as f32).sin());
        let rot = Quat::from_rotation_y(-yaw) * heel;
        out.push((0, Mat4::from_rotation_translation(rot, p + vec3(0.0, 0.1 * (t * 1.3 + k as f32).sin(), 0.0))));
    }
    // the ferry leaves the pier, heads out of sight, and comes back
    let quay = start + dir * 180.0 + vec3(dir.z, 0.0, -dir.x) * 20.0;
    let far = quay + dir * 9000.0;
    let period = 900.0;
    let ph = (t / period).fract();
    let leg = |f: f32| smoothstep(0.0, 1.0, f);
    let (p, heading) = if ph < 0.1 {
        (quay, dir)
    } else if ph < 0.5 {
        (quay.lerp(far, leg((ph - 0.1) / 0.4)), dir)
    } else if ph < 0.6 {
        (far, -dir)
    } else {
        (far.lerp(quay, leg((ph - 0.6) / 0.4)), -dir)
    };
    let yaw = heading.x.atan2(-heading.z);
    out.push((1, Mat4::from_rotation_translation(Quat::from_rotation_y(-yaw), p + vec3(0.0, 0.2 * (t * 0.5).sin(), 0.0))));
    out
}

/// Aircraft parked around the airfields: type, position (at the centre of gravity) and heading.
pub fn parked() -> Vec<(Craft, Vec3, f32)> {
    let mut v = Vec::new();
    let jet = Craft::Jet.spec().rest_height;
    for k in [0, 1, 3] {
        v.push((Craft::Jet, vec3(540.0 + k as f32 * 80.0, RWY_ELEV + jet, -127.0), 0.0));
    }
    let c = Craft::Cessna.spec().rest_height;
    for k in 0..3 {
        v.push((Craft::Cessna, vec3(150.0 + k as f32 * 16.0, RWY_ELEV + c, -92.0), 180.0 + k as f32 * 8.0));
    }
    v.push((Craft::Cessna, vec3(STRIP.0 + 100.0, STRIP_ELEV + c, STRIP.1 - 36.0), 200.0));
    v.push((Craft::Glider, vec3(STRIP.0 + 135.0, STRIP_ELEV + Craft::Glider.spec().rest_height, STRIP.1 - 34.0), 170.0));
    v.push((Craft::Fighter, vec3(220.0, RWY_ELEV + Craft::Fighter.spec().rest_height, -96.0), 160.0));
    v
}
