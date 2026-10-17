//! A two-seat sailplane shaped like an ASK 21, in body axes: origin at the centre of gravity,
//! nose toward -z. Matches the contact points in `flight::GLIDER`.

use crate::fighter::{loft, surface};
use crate::mesh::{rgb, Col, Drive, MeshData, Model, Part, Seen};
use macroquad::math::{vec3, Vec3};

const WHITE: Col = [0.96, 0.96, 0.95, 1.0];
const RED: Col = [0.86, 0.18, 0.12, 1.0];
const GLASS: Col = [0.16, 0.22, 0.30, 1.0];
const TIRE: Col = [0.07, 0.07, 0.07, 1.0];

fn wing_y(x: f32) -> f32 {
    0.05 + x.abs() * 0.066 // about 4 degrees of dihedral
}

pub fn model() -> Model {
    let mut parts = Vec::new();
    let mut body = MeshData::default();
    let sections = [
        (-2.75, 0.05, -0.45, -0.35),
        (-2.3, 0.3, -0.62, 0.0),
        (-1.5, 0.36, -0.7, 0.22),
        (-0.4, 0.36, -0.7, 0.28),
        (0.6, 0.33, -0.6, 0.3),
        (2.0, 0.2, -0.4, 0.2),
        (4.5, 0.12, -0.25, 0.15),
        (5.9, 0.08, -0.2, 0.15),
    ];
    loft(&mut body, &sections, 12, |si, _| if si == 0 { RED } else { WHITE });
    // long canopy bubble
    let c = vec3(0.0, 0.2, -1.0);
    let n = 12;
    for i in 0..n {
        for j in 0..n / 2 {
            let (a0, a1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
            let (b0, b1) = (j as f32 / n as f32 * std::f32::consts::PI, (j + 1) as f32 / n as f32 * std::f32::consts::PI);
            let p = |a: f32, b: f32| c + vec3(b.sin() * a.cos() * 0.34, b.cos() * 0.36, b.sin() * a.sin() * 1.3);
            body.quad(p(a0, b0), p(a1, b0), p(a1, b1), p(a0, b1), GLASS, Some(c));
        }
    }
    // wings: long and tapered, red tips
    for s in [-1.0f32, 1.0] {
        let (x0, x1, x2) = (0.3 * s, 5.0 * s, 8.5 * s);
        surface(&mut body, (x0, wing_y(x0), -0.35, 1.15), (x1, wing_y(x1), -0.12, 0.62), 0.16, WHITE);
        surface(&mut body, (x1, wing_y(x1), -0.12, 0.35), (s * 7.9, wing_y(7.9), 0.12, 0.5), 0.1, WHITE);
        surface(&mut body, (s * 7.9, wing_y(7.9), 0.12, 0.5), (x2, wing_y(8.5), 0.22, 0.55), 0.08, RED);
        let _ = x2;
    }
    // T-tail: fin, and the fixed part of the tailplane on top
    surface(&mut body, (0.0, 0.12, 4.4, 5.7), (0.0, 1.45, 5.55, 6.15), 0.1, WHITE);
    surface(&mut body, (0.0, 1.47, 5.45, 5.9), (1.65, 1.47, 5.7, 6.0), 0.06, WHITE);
    surface(&mut body, (0.0, 1.47, 5.45, 5.9), (-1.65, 1.47, 5.7, 6.0), 0.06, WHITE);
    // the wheels are fixed: a main wheel under the seat, a small nose wheel
    body.cylinder(vec3(0.0, -0.7, 0.3), Vec3::X, 0.2, 0.12, 12, TIRE, rgb(0.6, 0.6, 0.6));
    body.cylinder(vec3(0.0, -0.68, -1.8), Vec3::X, 0.12, 0.08, 10, TIRE, rgb(0.6, 0.6, 0.6));
    parts.push(Part::fixed(body, Seen::Outside));

    // ailerons, elevator, rudder
    for s in [-1.0f32, 1.0] {
        let mut a = MeshData::default();
        let (xa, xb) = (5.1 * s, 7.8 * s);
        surface(&mut a, (xa, wing_y(xa), 0.35, 0.6), (xb, wing_y(xb), 0.42, 0.55), 0.05, WHITE);
        let pivot = vec3(xa, wing_y(xa), 0.35);
        let dir = (vec3(xb, wing_y(xb), 0.42) - pivot).normalize() * s;
        parts.push(Part::hinged(a, pivot, dir, Drive::Aileron(-s), Seen::Always));
    }
    let mut el = MeshData::default();
    surface(&mut el, (0.0, 1.47, 5.9, 6.25), (1.6, 1.47, 6.0, 6.2), 0.04, WHITE);
    surface(&mut el, (0.0, 1.47, 5.9, 6.25), (-1.6, 1.47, 6.0, 6.2), 0.04, WHITE);
    parts.push(Part::hinged(el, vec3(0.0, 1.47, 5.9), Vec3::X, Drive::Elevator, Seen::Outside));
    let mut rud = MeshData::default();
    surface(&mut rud, (0.0, 0.15, 5.7, 6.25), (0.0, 1.4, 6.15, 6.4), 0.06, RED);
    let hinge = vec3(0.0, 0.15, 5.7);
    parts.push(Part::hinged(rud, hinge, vec3(0.0, 1.4, 6.15) - hinge, Drive::Rudder, Seen::Outside));
    // airbrake paddles that rise out of the top of each wing
    for s in [-1.0f32, 1.0] {
        let mut b = MeshData::default();
        let (x0, x1) = (1.8 * s, 3.2 * s);
        surface(&mut b, (x0, wing_y(x0) + 0.09, 0.2, 0.42), (x1, wing_y(x1) + 0.09, 0.22, 0.42), 0.03, rgb(0.85, 0.85, 0.85));
        let pivot = vec3(x0, wing_y(x0) + 0.09, 0.2);
        let dir = (vec3(x1, wing_y(x1) + 0.09, 0.22) - pivot).normalize() * s;
        parts.push(Part::hinged(b, pivot, dir, Drive::Spoiler, Seen::Always));
    }

    // cockpit: a small instrument hood and the canopy rails
    let mut ck = MeshData::default();
    let dash = rgb(0.18, 0.18, 0.2);
    ck.hexa(
        [
            vec3(-0.26, -0.15, -1.85),
            vec3(0.26, -0.15, -1.85),
            vec3(0.2, -0.15, -2.2),
            vec3(-0.2, -0.15, -2.2),
            vec3(-0.26, 0.15, -1.85),
            vec3(0.26, 0.15, -1.85),
            vec3(0.18, 0.08, -2.2),
            vec3(-0.18, 0.08, -2.2),
        ],
        dash,
    );
    for s in [-1.0f32, 1.0] {
        ck.beam(vec3(s * 0.36, -0.02, -2.2), vec3(s * 0.36, -0.02, 0.2), 0.025, rgb(0.85, 0.85, 0.85));
    }
    parts.push(Part::fixed(ck, Seen::Cockpit));
    Model { parts, prop_disc: None }
}
