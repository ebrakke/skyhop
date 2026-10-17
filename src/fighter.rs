//! A single-engine fighter shaped like an F-16 (in plain grey), in body axes: origin at the
//! centre of gravity, nose toward -z. Matches the contact points in `flight::FIGHTER`.

use crate::mesh::{rgb, Col, Drive, MeshData, Model, Part, Seen};
use macroquad::math::{vec3, Vec3};

const LIGHT: Col = [0.62, 0.65, 0.69, 1.0];
const DARK: Col = [0.44, 0.47, 0.52, 1.0];
const CANOPY: Col = [0.32, 0.28, 0.16, 1.0];
const METAL: Col = [0.30, 0.30, 0.32, 1.0];
const TIRE: Col = [0.07, 0.07, 0.07, 1.0];

/// Elliptical sections (z, half-width, y-bottom, y-top) joined into a closed tube.
pub fn loft(m: &mut MeshData, sections: &[(f32, f32, f32, f32)], seg: usize, colour: impl Fn(usize, f32) -> Col) {
    let ring = |(z, hw, yb, yt): (f32, f32, f32, f32)| -> Vec<Vec3> {
        let (cy, ry) = ((yb + yt) * 0.5, (yt - yb) * 0.5);
        (0..seg)
            .map(|k| {
                let th = k as f32 / seg as f32 * std::f32::consts::TAU;
                vec3(hw * th.sin(), cy + ry * th.cos(), z)
            })
            .collect()
    };
    for si in 0..sections.len() - 1 {
        let (a, b) = (ring(sections[si]), ring(sections[si + 1]));
        let inside = vec3(0.0, (sections[si].2 + sections[si].3) * 0.5, (sections[si].0 + sections[si + 1].0) * 0.5);
        for k in 0..seg {
            let n = (k + 1) % seg;
            // angle of the face from the top, 0..180
            let th = (((k as f32 + 0.5) / seg as f32 * 360.0 + 180.0).rem_euclid(360.0) - 180.0).abs();
            m.quad(a[k], a[n], b[n], b[k], colour(si, th), Some(inside));
        }
    }
}

/// A flat surface with thickness between a root and a tip chord line: (x, y, z_front, z_back) each.
pub fn surface(m: &mut MeshData, root: (f32, f32, f32, f32), tip: (f32, f32, f32, f32), thick: f32, col: Col) {
    let (rx, ry, rf, rb) = root;
    let (tx, ty, tf, tb) = tip;
    let h = thick * 0.5;
    m.hexa(
        [
            vec3(rx, ry - h, rf),
            vec3(tx, ty - h * 0.5, tf),
            vec3(tx, ty - h * 0.3, tb),
            vec3(rx, ry - h * 0.4, rb),
            vec3(rx, ry + h, rf),
            vec3(tx, ty + h * 0.5, tf),
            vec3(tx, ty + h * 0.3, tb),
            vec3(rx, ry + h * 0.4, rb),
        ],
        col,
    );
}

fn mirror_x(m: &MeshData) -> MeshData {
    let mut out = MeshData::default();
    for tri in m.i.chunks(3) {
        let p = |i: u32| {
            let v = m.v[i as usize].pos;
            vec3(-v[0], v[1], v[2])
        };
        let n = m.v[tri[0] as usize].nrm;
        let nm = vec3(-n[0], n[1], n[2]);
        let col = m.v[tri[0] as usize].col;
        let i0 = out.vert(p(tri[0]), nm, col);
        out.vert(p(tri[2]), nm, col);
        out.vert(p(tri[1]), nm, col);
        out.i.extend_from_slice(&[i0, i0 + 1, i0 + 2]);
    }
    out
}

fn merge(a: &mut MeshData, b: &MeshData) {
    let base = a.v.len() as u32;
    a.v.extend_from_slice(&b.v);
    a.i.extend(b.i.iter().map(|i| i + base));
}

pub fn model() -> Model {
    let mut parts = Vec::new();

    // fuselage, with a darker spine
    let mut body = MeshData::default();
    let sections = [
        (-8.9, 0.04, -0.42, -0.36),
        (-8.2, 0.3, -0.66, -0.05),
        (-6.8, 0.5, -0.82, 0.22),
        (-5.0, 0.58, -0.9, 0.4),
        (-3.0, 0.64, -0.98, 0.46),
        (-1.0, 0.78, -1.02, 0.5),
        (1.5, 0.78, -0.95, 0.55),
        (4.0, 0.62, -0.75, 0.5),
        (5.7, 0.5, -0.55, 0.42),
    ];
    loft(&mut body, &sections, 14, |si, th| if si == 0 { METAL } else if th < 40.0 { DARK } else { LIGHT });
    // intake under the fuselage, black inside
    body.hexa(
        [
            vec3(-0.45, -1.42, -4.6),
            vec3(0.45, -1.42, -4.6),
            vec3(0.45, -1.3, 0.5),
            vec3(-0.45, -1.3, 0.5),
            vec3(-0.45, -0.9, -4.3),
            vec3(0.45, -0.9, -4.3),
            vec3(0.45, -0.85, 0.5),
            vec3(-0.45, -0.85, 0.5),
        ],
        LIGHT,
    );
    body.quad(vec3(-0.4, -1.38, -4.62), vec3(0.4, -1.38, -4.62), vec3(0.4, -0.95, -4.35), vec3(-0.4, -0.95, -4.35), rgb(0.03, 0.03, 0.03), Some(vec3(0.0, -1.1, -3.0)));
    // nozzle
    body.cylinder(vec3(0.0, -0.06, 6.1), Vec3::Z, 0.44, 0.9, 14, METAL, rgb(0.08, 0.06, 0.05));
    // bubble canopy
    let c = vec3(0.0, 0.42, -3.4);
    let n = 12;
    for i in 0..n {
        for j in 0..n / 2 {
            let (a0, a1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
            let (b0, b1) = (j as f32 / n as f32 * std::f32::consts::PI, (j + 1) as f32 / n as f32 * std::f32::consts::PI);
            let p = |a: f32, b: f32| c + vec3(b.sin() * a.cos() * 0.44, b.cos() * 0.5, b.sin() * a.sin() * 1.7);
            body.quad(p(a0, b0), p(a1, b0), p(a1, b1), p(a0, b1), CANOPY, Some(c));
        }
    }
    // leading-edge extensions and the wing, then mirrored
    let mut wing = MeshData::default();
    surface(&mut wing, (0.55, -0.15, -5.6, -1.0), (1.45, -0.18, -1.2, -0.6), 0.1, LIGHT);
    surface(&mut wing, (0.7, -0.2, -1.6, 2.6), (4.7, -0.22, 1.75, 2.45), 0.14, LIGHT);
    // wingtip launch rail and a missile
    wing.beam(vec3(4.86, -0.2, 0.3), vec3(4.86, -0.2, 2.6), 0.1, DARK);
    wing.cylinder(vec3(4.98, -0.28, 1.4), Vec3::Z, 0.065, 2.9, 8, rgb(0.92, 0.92, 0.9), rgb(0.2, 0.2, 0.2));
    wing.cone(vec3(4.98, -0.28, -0.05), Vec3::NEG_Z, 0.065, 0.3, 8, rgb(0.3, 0.3, 0.3));
    let wing_l = mirror_x(&wing);
    merge(&mut body, &wing);
    merge(&mut body, &wing_l);
    // fin
    surface(&mut body, (0.0, 0.5, 2.3, 6.0), (0.0, 3.2, 5.0, 6.1), 0.12, LIGHT);
    parts.push(Part::fixed(body, Seen::Outside));

    // flaperons: roll (they droop with the flaps too in the real thing)
    let mut fl = MeshData::default();
    surface(&mut fl, (0.95, -0.2, 2.6, 3.2), (3.4, -0.21, 2.5, 3.05), 0.08, DARK);
    let pivot = vec3(0.95, -0.2, 2.6);
    let dir = (vec3(3.4, -0.21, 2.5) - pivot).normalize();
    let fl_l = mirror_x(&fl);
    parts.push(Part::hinged(fl, pivot, dir, Drive::Aileron(-1.0), Seen::Always));
    parts.push(Part::hinged(fl_l, vec3(-pivot.x, pivot.y, pivot.z), vec3(dir.x, -dir.y, -dir.z), Drive::Aileron(1.0), Seen::Always));

    // all-moving stabilators with a little anhedral
    let mut st = MeshData::default();
    surface(&mut st, (0.6, -0.2, 4.4, 6.5), (2.9, -0.55, 5.8, 6.6), 0.1, LIGHT);
    let pivot = vec3(0.6, -0.2, 5.3);
    let st_l = mirror_x(&st);
    parts.push(Part::hinged(st, pivot, Vec3::X, Drive::Elevator, Seen::Outside));
    parts.push(Part::hinged(st_l, vec3(-0.6, -0.2, 5.3), Vec3::X, Drive::Elevator, Seen::Outside));

    // rudder
    let mut rud = MeshData::default();
    surface(&mut rud, (0.0, 0.7, 6.0, 6.55), (0.0, 3.1, 6.1, 6.45), 0.08, DARK);
    let hinge = vec3(0.0, 0.7, 6.0);
    parts.push(Part::hinged(rud, hinge, vec3(0.0, 3.1, 6.1) - hinge, Drive::Rudder, Seen::Outside));

    // gear
    let mut g = MeshData::default();
    for s in [-1.0f32, 1.0] {
        g.beam(vec3(s * 1.05, -0.9, 0.8), vec3(s * 1.18, -1.45, 0.8), 0.1, METAL);
        g.cylinder(vec3(s * 1.18, -1.45, 0.8), Vec3::X, 0.3, 0.22, 12, TIRE, METAL);
    }
    g.beam(vec3(0.0, -1.3, -4.1), vec3(0.0, -1.53, -4.1), 0.08, METAL);
    g.cylinder(vec3(0.0, -1.53, -4.1), Vec3::X, 0.22, 0.16, 10, TIRE, METAL);
    parts.push(Part { mesh: g, pivot: Vec3::ZERO, axis: Vec3::Y, drive: Drive::Gear, seen: Seen::Always });

    // afterburner flame: a hot core inside a longer orange plume
    let mut flame = MeshData::default();
    flame.cone(vec3(0.0, -0.06, 6.55), Vec3::Z, 0.36, 3.2, 12, [1.0, 0.55, 0.2, 0.55]);
    flame.cone(vec3(0.0, -0.06, 6.55), Vec3::Z, 0.24, 1.8, 10, [1.0, 0.9, 0.7, 0.8]);
    parts.push(Part { mesh: flame, pivot: vec3(0.0, -0.06, 6.55), axis: Vec3::Z, drive: Drive::Burner, seen: Seen::Outside });

    // the cockpit: low glareshield, the HUD combiner, canopy rails, the seat back
    let mut ck = MeshData::default();
    let dash = rgb(0.16, 0.17, 0.18);
    ck.hexa(
        [
            vec3(-0.34, -0.3, -4.55),
            vec3(0.34, -0.3, -4.55),
            vec3(0.3, -0.3, -5.1),
            vec3(-0.3, -0.3, -5.1),
            vec3(-0.34, 0.33, -4.55),
            vec3(0.34, 0.33, -4.55),
            vec3(0.28, 0.3, -5.1),
            vec3(-0.28, 0.3, -5.1),
        ],
        dash,
    );
    for s in [-1.0f32, 1.0] {
        ck.beam(vec3(s * 0.11, 0.33, -4.7), vec3(s * 0.11, 0.6, -4.78), 0.015, METAL);
        ck.beam(vec3(s * 0.46, 0.32, -5.0), vec3(s * 0.46, 0.32, -1.9), 0.05, METAL);
        ck.hexa(
            [
                vec3(s * 0.46, -0.2, -4.5),
                vec3(s * 0.3, -0.2, -4.5),
                vec3(s * 0.3, -0.2, -2.2),
                vec3(s * 0.46, -0.2, -2.2),
                vec3(s * 0.46, 0.18, -4.5),
                vec3(s * 0.3, 0.12, -4.5),
                vec3(s * 0.3, 0.12, -2.2),
                vec3(s * 0.46, 0.18, -2.2),
            ],
            dash,
        );
    }
    ck.beam(vec3(-0.11, 0.6, -4.78), vec3(0.11, 0.6, -4.78), 0.015, METAL);
    parts.push(Part::fixed(ck, Seen::Cockpit));

    Model { parts, prop_disc: None }
}
