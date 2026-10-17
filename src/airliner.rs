//! A narrow-body twinjet shaped like a Boeing 737 MAX 8 (in a made-up livery), in body axes:
//! origin at the centre of gravity, nose toward -z, +x out the right wing. Matches the contact
//! points in `flight::JET` (main wheels at y -3.0, nacelles 0.5 m above the runway).

use crate::mesh::{rgb, Col, Drive, MeshData, Model, Part, Seen};
use macroquad::math::{vec3, Vec3};

const WHITE: Col = [0.95, 0.95, 0.96, 1.0];
const BELLY: Col = [0.70, 0.72, 0.75, 1.0];
const STRIPE: Col = [0.10, 0.30, 0.62, 1.0];
const TAIL: Col = [0.09, 0.22, 0.50, 1.0];
const GLASS: Col = [0.08, 0.11, 0.16, 1.0];
const WING: Col = [0.80, 0.81, 0.83, 1.0];
const NACELLE: Col = [0.88, 0.88, 0.90, 1.0];
const DARK: Col = [0.16, 0.16, 0.17, 1.0];
const METAL: Col = [0.55, 0.56, 0.58, 1.0];
const TIRE: Col = [0.07, 0.07, 0.07, 1.0];

/// Fuselage stations: (z, radius, centre height).
const BODY: [(f32, f32, f32); 13] = [
    (-18.6, 0.12, -0.45),
    (-18.2, 0.65, -0.35),
    (-17.5, 1.10, -0.22),
    (-16.6, 1.45, -0.12),
    (-15.4, 1.70, -0.05),
    (-13.8, 1.85, 0.0),
    (-12.0, 1.88, 0.0),
    (10.0, 1.88, 0.0),
    (12.5, 1.72, 0.22),
    (15.0, 1.30, 0.62),
    (17.5, 0.88, 0.95),
    (19.8, 0.45, 1.2),
    (20.9, 0.18, 1.3),
];
const SEG: usize = 24;

fn mirror(p: Vec3) -> Vec3 {
    vec3(-p.x, p.y, p.z)
}

fn fuselage() -> MeshData {
    let mut m = MeshData::default();
    let ring = |(z, r, cy): (f32, f32, f32)| -> Vec<Vec3> {
        (0..SEG)
            .map(|k| {
                let th = k as f32 / SEG as f32 * std::f32::consts::TAU;
                vec3(r * th.sin(), cy + r * 1.03 * th.cos(), z)
            })
            .collect()
    };
    for si in 0..BODY.len() - 1 {
        let (a, b) = (ring(BODY[si]), ring(BODY[si + 1]));
        let inside = vec3(0.0, BODY[si].2, (BODY[si].0 + BODY[si + 1].0) * 0.5);
        for k in 0..SEG {
            let n = (k + 1) % SEG;
            // angle of this face from the top, 0..180 on either side
            let th = ((k as f32 + 0.5) / SEG as f32 * 360.0 + 180.0).rem_euclid(360.0) - 180.0;
            let ath = th.abs();
            let mut col = if ath > 106.0 {
                BELLY
            } else if ath > 92.0 {
                STRIPE
            } else {
                WHITE
            };
            if si == 2 && ath < 80.0 {
                col = GLASS; // flight-deck windows wrap around the nose
            }
            if si == 0 {
                col = DARK; // radome tip
            }
            m.quad(a[k], a[n], b[n], b[k], col, Some(inside));
        }
    }
    // cabin windows and doors, a row on each side
    for side in [-1.0f32, 1.0] {
        let th = 74f32.to_radians();
        let (x, y) = (side * (1.88 * th.sin() + 0.015), 1.88 * 1.03 * th.cos());
        let mut z = -11.3;
        while z < 9.6 {
            // leave gaps for the over-wing exits
            if !(-0.3..0.9).contains(&z) {
                m.quad(vec3(x, y - 0.17, z - 0.12), vec3(x, y - 0.17, z + 0.12), vec3(x, y + 0.17, z + 0.12), vec3(x, y + 0.17, z - 0.12), GLASS, Some(vec3(0.0, y, z)));
            }
            z += 0.51;
        }
        for dz in [-12.6, 10.6] {
            let dx = side * (1.88 + 0.01);
            m.quad(vec3(dx, -0.9, dz - 0.42), vec3(dx, -0.9, dz + 0.42), vec3(dx, 0.95, dz + 0.42), vec3(dx, 0.95, dz - 0.42), BELLY, Some(vec3(0.0, 0.0, dz)));
        }
    }
    // wing-to-body fairing under the centre section
    m.hexa(
        [
            vec3(-1.95, -1.95, -4.5),
            vec3(1.95, -1.95, -4.5),
            vec3(1.95, -1.95, 4.0),
            vec3(-1.95, -1.95, 4.0),
            vec3(-1.9, -1.2, -5.5),
            vec3(1.9, -1.2, -5.5),
            vec3(1.9, -1.2, 5.0),
            vec3(-1.9, -1.2, 5.0),
        ],
        BELLY,
    );
    m
}

// ------------------------------------------------------------------ wing ---

const ROOT_X: f32 = 1.9;
const TIP_X: f32 = 17.0;

/// Leading and trailing edge (z) of the right wing at span station x.
fn edges(x: f32) -> (f32, f32) {
    let le = -4.0 + (x - ROOT_X).max(0.0) * 0.532; // 28 deg leading-edge sweep
    let te = 3.0 + (x - 6.5).max(0.0) * 0.2476; // straight inboard trailing edge, then swept
    (le, te)
}

fn wing_y(x: f32) -> f32 {
    -1.3 + (x - ROOT_X).max(0.0) * 0.105 // 6 deg dihedral
}

fn thickness(x: f32) -> f32 {
    0.85 + (0.18 - 0.85) * ((x - ROOT_X).max(0.0) / (TIP_X - ROOT_X))
}

/// Chord of the control surface behind the rear spar at x (flaps, then aileron).
fn surface_chord(x: f32) -> f32 {
    if x < 6.5 {
        1.6
    } else if x < 12.5 {
        1.3 - (x - 6.5) * 0.03
    } else if x < 16.3 {
        0.9
    } else {
        0.0
    }
}

/// A slab of the right wing between stations x0..x1, from chord line `front` to `back`
/// (functions of x giving z), with the top surface thinning toward the trailing edge.
fn slab(m: &mut MeshData, x0: f32, x1: f32, front: impl Fn(f32) -> f32, back: impl Fn(f32) -> f32, thick_front: f32, thick_back: f32, col: Col) {
    let (y0, y1) = (wing_y(x0), wing_y(x1));
    let (t0, t1) = (thickness(x0), thickness(x1));
    m.hexa(
        [
            vec3(x0, y0, front(x0)),
            vec3(x1, y1, front(x1)),
            vec3(x1, y1, back(x1)),
            vec3(x0, y0, back(x0)),
            vec3(x0, y0 + t0 * thick_front, front(x0)),
            vec3(x1, y1 + t1 * thick_front, front(x1)),
            vec3(x1, y1 + t1 * thick_back, back(x1)),
            vec3(x0, y0 + t0 * thick_back, back(x0)),
        ],
        col,
    );
}

fn mirror_mesh(m: &MeshData) -> MeshData {
    let mut out = MeshData::default();
    for tri in m.i.chunks(3) {
        let v = |i: u32| {
            let p = m.v[i as usize].pos;
            vec3(-p[0], p[1], p[2])
        };
        let (a, b, c) = (v(tri[0]), v(tri[1]), v(tri[2]));
        let n = m.v[tri[0] as usize].nrm;
        let col = m.v[tri[0] as usize].col;
        // reversed winding, mirrored normal
        let i0 = out.vert(a, vec3(-n[0], n[1], n[2]), col);
        out.vert(c, vec3(-n[0], n[1], n[2]), col);
        out.vert(b, vec3(-n[0], n[1], n[2]), col);
        out.i.extend_from_slice(&[i0, i0 + 1, i0 + 2]);
    }
    out
}

/// A hinged surface on the right wing and its mirror image on the left.
fn pair(parts: &mut Vec<Part>, right: MeshData, pivot: Vec3, hinge_dir: Vec3, drive_r: Drive, drive_l: Drive) {
    let left = mirror_mesh(&right);
    // mirror the hinge, then flip it so both axes point toward +x: a positive angle then
    // lowers the trailing edge on either side
    let axis_l = vec3(hinge_dir.x, -hinge_dir.y, -hinge_dir.z);
    parts.push(Part::hinged(right, pivot, hinge_dir, drive_r, Seen::Always));
    parts.push(Part::hinged(left, mirror(pivot), axis_l, drive_l, Seen::Always));
}

fn wing(parts: &mut Vec<Part>) {
    let mut w = MeshData::default();
    let le = |x: f32| edges(x).0;
    let spar = |x: f32| edges(x).1 - surface_chord(x);
    // centre section, inside the fuselage
    slab(&mut w, 0.0, ROOT_X, le, |x| edges(x).1, 1.0, 0.3, WING);
    for (x0, x1) in [(ROOT_X, 6.5), (6.5, 12.5), (12.5, 16.3)] {
        slab(&mut w, x0, x1, le, |x| spar(x.clamp(x0 + 1e-3, x1 - 1e-3)), 1.0, 0.55, WING);
    }
    slab(&mut w, 16.3, TIP_X, le, |x| edges(x).1, 1.0, 0.3, WING);
    // split-tip winglet: tall upper blade, small lower one
    let (tle, tte) = edges(TIP_X);
    let ty = wing_y(TIP_X);
    w.hexa(
        [
            vec3(TIP_X - 0.05, ty, tle + 0.2),
            vec3(TIP_X + 0.05, ty, tle + 0.2),
            vec3(TIP_X + 0.05, ty, tte),
            vec3(TIP_X - 0.05, ty, tte),
            vec3(TIP_X + 0.85, ty + 2.4, tte + 0.4),
            vec3(TIP_X + 0.95, ty + 2.4, tte + 0.4),
            vec3(TIP_X + 0.95, ty + 2.4, tte + 1.2),
            vec3(TIP_X + 0.85, ty + 2.4, tte + 1.2),
        ],
        TAIL,
    );
    w.hexa(
        [
            vec3(TIP_X - 0.05, ty, tle + 0.6),
            vec3(TIP_X + 0.05, ty, tle + 0.6),
            vec3(TIP_X + 0.05, ty, tte - 0.1),
            vec3(TIP_X - 0.05, ty, tte - 0.1),
            vec3(TIP_X + 0.35, ty - 0.9, tte - 0.1),
            vec3(TIP_X + 0.45, ty - 0.9, tte - 0.1),
            vec3(TIP_X + 0.45, ty - 0.9, tte + 0.3),
            vec3(TIP_X + 0.35, ty - 0.9, tte + 0.3),
        ],
        TAIL,
    );
    let left = mirror_mesh(&w);
    parts.push(Part::fixed(w, Seen::Always));
    parts.push(Part::fixed(left, Seen::Always));

    // flaps (two per side) and ailerons hang off the rear spar
    let hinge = |x0: f32, x1: f32| -> (Vec3, Vec3) {
        let a = vec3(x0, wing_y(x0) + thickness(x0) * 0.2, spar(x0 + 1e-3));
        let b = vec3(x1, wing_y(x1) + thickness(x1) * 0.2, spar(x1 - 1e-3));
        (a, (b - a).normalize())
    };
    for (x0, x1, aileron) in [(ROOT_X, 6.5, false), (6.5, 12.5, false), (12.5, 16.3, true)] {
        let mut s = MeshData::default();
        let (lo, hi) = (x0 + 0.05, x1 - 0.05);
        slab(&mut s, lo, hi, |x| spar(x.clamp(x0 + 1e-3, x1 - 1e-3)), |x| edges(x).1, 0.55, 0.15, if aileron { WHITE } else { WING });
        let (pivot, dir) = hinge(x0, x1);
        if aileron {
            pair(parts, s, pivot, dir, Drive::Aileron(-1.0), Drive::Aileron(1.0));
        } else {
            pair(parts, s, pivot, dir, Drive::Flap, Drive::Flap);
        }
    }
    // spoiler panels lie on the top skin just ahead of the flaps
    for (x0, x1) in [(2.4, 4.3), (4.4, 6.3), (6.8, 8.1), (8.2, 9.5), (9.6, 10.9), (11.0, 12.2)] {
        let chord = 0.75;
        let front = |x: f32| spar(x.clamp(x0, x1)) - chord;
        let back = |x: f32| spar(x.clamp(x0, x1)) - 0.02;
        let mut s = MeshData::default();
        let (y0, y1) = (wing_y(x0) + thickness(x0) * 0.62, wing_y(x1) + thickness(x1) * 0.62);
        s.hexa(
            [
                vec3(x0, y0, front(x0)),
                vec3(x1, y1, front(x1)),
                vec3(x1, y1 - 0.05, back(x1)),
                vec3(x0, y0 - 0.05, back(x0)),
                vec3(x0, y0 + 0.05, front(x0)),
                vec3(x1, y1 + 0.05, front(x1)),
                vec3(x1, y1, back(x1)),
                vec3(x0, y0, back(x0)),
            ],
            WING,
        );
        let a = vec3(x0, y0, front(x0));
        let dir = (vec3(x1, y1, front(x1)) - a).normalize();
        pair(parts, s, a, dir, Drive::Spoiler, Drive::Spoiler);
    }
}

// --------------------------------------------------------------- engines ---

const ENGINE_X: f32 = 4.9;
const ENGINE_Y: f32 = -1.45;

/// Surface of revolution about the engine axis through `profile` of (z, radius).
fn lathe(m: &mut MeshData, cx: f32, profile: &[(f32, f32)], col: Col) {
    let n = 20;
    for w in profile.windows(2) {
        let ((z0, r0), (z1, r1)) = (w[0], w[1]);
        let centre = vec3(cx, ENGINE_Y, (z0 + z1) * 0.5);
        for k in 0..n {
            let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let p = |a: f32, r: f32, z: f32| vec3(cx + r * a.cos(), ENGINE_Y + r * a.sin(), z);
            m.quad(p(a0, r0, z0), p(a1, r0, z0), p(a1, r1, z1), p(a0, r1, z1), col, Some(centre));
        }
    }
}

fn engine(parts: &mut Vec<Part>, cx: f32) {
    let mut cowl = MeshData::default();
    lathe(&mut cowl, cx, &[(-7.75, 0.9), (-7.6, 1.0), (-7.1, 1.06), (-5.8, 1.05), (-4.7, 0.99)], NACELLE);
    // inlet: lip lining, fan face and spinner
    lathe(&mut cowl, cx, &[(-7.75, 0.9), (-7.4, 0.86)], METAL);
    let fan_c = vec3(cx, ENGINE_Y, -7.35);
    let n = 20;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let p = |a: f32| fan_c + vec3(a.cos(), a.sin(), 0.0) * 0.87;
        cowl.tri(fan_c, p(a0), p(a1), DARK, Some(fan_c + vec3(0.0, 0.0, 1.0)));
    }
    cowl.cone(vec3(cx, ENGINE_Y, -7.3), Vec3::NEG_Z, 0.3, 0.55, 12, rgb(0.85, 0.85, 0.86));
    // core and exhaust cone
    lathe(&mut cowl, cx, &[(-3.5, 0.66), (-2.6, 0.55), (-2.4, 0.5)], METAL);
    lathe(&mut cowl, cx, &[(-2.4, 0.42), (-1.7, 0.18)], DARK);
    // pylon up to the wing
    let px = cx;
    cowl.hexa(
        [
            vec3(px - 0.16, -0.5, -6.3),
            vec3(px + 0.16, -0.5, -6.3),
            vec3(px + 0.16, -0.95, -1.0),
            vec3(px - 0.16, -0.95, -1.0),
            vec3(px - 0.14, -0.4, -6.0),
            vec3(px + 0.14, -0.4, -6.0),
            vec3(px + 0.14, wing_y(cx.abs()) + 0.2, 0.8),
            vec3(px - 0.14, wing_y(cx.abs()) + 0.2, 0.8),
        ],
        NACELLE,
    );
    parts.push(Part::fixed(cowl, Seen::Always));

    // translating reverser sleeve
    let mut sleeve = MeshData::default();
    lathe(&mut sleeve, cx, &[(-4.72, 0.99), (-4.0, 0.95), (-3.45, 0.86)], NACELLE);
    lathe(&mut sleeve, cx, &[(-3.45, 0.86), (-3.45, 0.66)], DARK);
    parts.push(Part { mesh: sleeve, pivot: Vec3::ZERO, axis: Vec3::Z, drive: Drive::Reverser, seen: Seen::Always });

    // fan blades, spinning with N1
    let mut blades = MeshData::default();
    for k in 0..9 {
        let a = k as f32 / 9.0 * std::f32::consts::TAU;
        let d = vec3(a.cos(), a.sin(), 0.0);
        blades.beam(fan_c + d * 0.3 + vec3(0.0, 0.0, -0.03), fan_c + d * 0.85 + vec3(0.0, 0.0, -0.03), 0.09, rgb(0.32, 0.33, 0.35));
    }
    parts.push(Part::hinged(blades, fan_c, Vec3::Z, Drive::Spin, Seen::Always));
}

// ------------------------------------------------------------------ tail ---

fn tail(parts: &mut Vec<Part>) {
    let mut fin = MeshData::default();
    // fin box from the leading edge to the rudder hinge
    let (root_y, tip_y) = (1.45, 9.5);
    let (root_le, root_hinge, root_te) = (12.0, 17.4, 19.7);
    let (tip_le, tip_hinge, tip_te) = (17.9, 19.4, 20.2);
    let (rt, tt) = (0.18, 0.06);
    fin.hexa(
        [
            vec3(-rt * 0.4, root_y, root_le),
            vec3(-rt, root_y, root_hinge),
            vec3(-tt, tip_y, tip_hinge),
            vec3(-tt * 0.4, tip_y, tip_le),
            vec3(rt * 0.4, root_y, root_le),
            vec3(rt, root_y, root_hinge),
            vec3(tt, tip_y, tip_hinge),
            vec3(tt * 0.4, tip_y, tip_le),
        ],
        TAIL,
    );
    // dorsal fillet blending the fin into the fuselage
    fin.hexa(
        [
            vec3(-0.15, 1.2, 8.5),
            vec3(-0.15, 1.35, 12.5),
            vec3(-0.15, 2.4, 13.6),
            vec3(-0.15, 1.85, 11.0),
            vec3(0.15, 1.2, 8.5),
            vec3(0.15, 1.35, 12.5),
            vec3(0.15, 2.4, 13.6),
            vec3(0.15, 1.85, 11.0),
        ],
        WHITE,
    );
    parts.push(Part::fixed(fin, Seen::Outside));
    let mut rudder = MeshData::default();
    rudder.hexa(
        [
            vec3(-rt * 0.8, root_y, root_hinge),
            vec3(-0.03, root_y, root_te),
            vec3(-0.02, tip_y, tip_te),
            vec3(-tt * 0.8, tip_y, tip_hinge),
            vec3(rt * 0.8, root_y, root_hinge),
            vec3(0.03, root_y, root_te),
            vec3(0.02, tip_y, tip_te),
            vec3(tt * 0.8, tip_y, tip_hinge),
        ],
        TAIL,
    );
    let hinge_root = vec3(0.0, root_y, root_hinge);
    parts.push(Part::hinged(rudder, hinge_root, vec3(0.0, tip_y, tip_hinge) - hinge_root, Drive::Rudder, Seen::Outside));

    // horizontal stabiliser and elevators
    let (r_x, t_x) = (0.9, 7.2);
    let (r_y, t_y) = (1.15, 1.95);
    let (r_le, r_hinge, r_te) = (15.6, 18.4, 19.5);
    let (t_le, t_hinge, t_te) = (19.7, 20.65, 21.0);
    let mut stab = MeshData::default();
    stab.hexa(
        [
            vec3(r_x, r_y - 0.15, r_le),
            vec3(t_x, t_y - 0.04, t_le),
            vec3(t_x, t_y - 0.04, t_hinge),
            vec3(r_x, r_y - 0.15, r_hinge),
            vec3(r_x, r_y + 0.2, r_le),
            vec3(t_x, t_y + 0.05, t_le),
            vec3(t_x, t_y + 0.05, t_hinge),
            vec3(r_x, r_y + 0.2, r_hinge),
        ],
        WHITE,
    );
    let stab_l = mirror_mesh(&stab);
    parts.push(Part::fixed(stab, Seen::Outside));
    parts.push(Part::fixed(stab_l, Seen::Outside));
    let mut elev = MeshData::default();
    elev.hexa(
        [
            vec3(r_x + 0.05, r_y - 0.12, r_hinge),
            vec3(t_x, t_y - 0.03, t_hinge),
            vec3(t_x, t_y, t_te),
            vec3(r_x + 0.05, r_y, r_te),
            vec3(r_x + 0.05, r_y + 0.16, r_hinge),
            vec3(t_x, t_y + 0.04, t_hinge),
            vec3(t_x, t_y + 0.01, t_te),
            vec3(r_x + 0.05, r_y + 0.02, r_te),
        ],
        WHITE,
    );
    let a = vec3(r_x, r_y, r_hinge);
    let dir = (vec3(t_x, t_y, t_hinge) - a).normalize();
    let elev_l = mirror_mesh(&elev);
    parts.push(Part::hinged(elev, a, dir, Drive::Elevator, Seen::Outside));
    parts.push(Part::hinged(elev_l, mirror(a), vec3(dir.x, -dir.y, -dir.z), Drive::Elevator, Seen::Outside));
}

// ------------------------------------------------------------------ gear ---

fn gear(parts: &mut Vec<Part>) {
    let mut g = MeshData::default();
    for side in [-1.0f32, 1.0] {
        let x = side * 2.86;
        g.beam(vec3(x, -1.25, 1.0), vec3(x, -2.45, 1.0), 0.22, METAL);
        g.beam(vec3(x, -1.5, 1.0), vec3(side * 1.6, -1.35, 0.6), 0.12, METAL); // side brace
        g.beam(vec3(x - 0.42, -2.44, 1.0), vec3(x + 0.42, -2.44, 1.0), 0.14, METAL); // axle
        for dx in [-0.42, 0.42] {
            g.cylinder(vec3(x + dx, -2.44, 1.0), Vec3::X, 0.56, 0.36, 14, TIRE, METAL);
        }
    }
    g.beam(vec3(0.0, -1.5, -14.6), vec3(0.0, -2.62, -14.6), 0.14, METAL);
    for dx in [-0.22, 0.22] {
        g.cylinder(vec3(dx, -2.62, -14.6), Vec3::X, 0.38, 0.24, 12, TIRE, METAL);
    }
    parts.push(Part { mesh: g, pivot: Vec3::ZERO, axis: Vec3::Y, drive: Drive::Gear, seen: Seen::Always });
}

// --------------------------------------------------------------- cockpit ---

/// The flight deck from the captain's seat (eye at -0.55, 1.0, -16.2): glareshield, the
/// windshield frames, side sills and the overhead panel.
fn cockpit() -> MeshData {
    let dash = rgb(0.16, 0.17, 0.18);
    let frame = rgb(0.28, 0.29, 0.31);
    let wall = rgb(0.42, 0.43, 0.45);
    let mut m = MeshData::default();
    m.hexa(
        [
            vec3(-1.3, 0.0, -16.95),
            vec3(1.3, 0.0, -16.95),
            vec3(1.1, 0.0, -17.4),
            vec3(-1.1, 0.0, -17.4),
            vec3(-1.3, 0.8, -16.95),
            vec3(1.3, 0.8, -16.95),
            vec3(1.05, 0.74, -17.4),
            vec3(-1.05, 0.74, -17.4),
        ],
        dash,
    );
    m.beam(vec3(-1.3, 0.8, -16.94), vec3(1.3, 0.8, -16.94), 0.04, rgb(0.08, 0.08, 0.09));
    // window posts: centre, front/side corners, and the rear posts
    m.beam(vec3(0.0, 0.76, -17.35), vec3(0.0, 1.36, -16.98), 0.035, frame);
    for s in [-1.0f32, 1.0] {
        m.beam(vec3(s * 0.66, 0.78, -17.28), vec3(s * 0.58, 1.36, -16.93), 0.06, frame);
        m.beam(vec3(s * 1.22, 0.74, -16.97), vec3(s * 1.0, 1.38, -16.35), 0.08, frame);
        m.beam(vec3(s * 1.3, 0.64, -16.95), vec3(s * 1.4, 0.64, -14.0), 0.08, frame); // sill
        m.quad(vec3(s * 1.42, -0.4, -16.95), vec3(s * 1.42, -0.4, -14.0), vec3(s * 1.42, 0.64, -14.0), vec3(s * 1.42, 0.64, -16.95), wall, Some(vec3(0.0, 0.3, -15.5)));
    }
    m.beam(vec3(-1.0, 1.37, -16.96), vec3(1.0, 1.37, -16.96), 0.06, frame);
    // overhead panel and ceiling
    m.aabb(vec3(-0.75, 1.37, -16.9), vec3(0.75, 1.49, -14.6), dash);
    m.quad(vec3(-1.4, 1.5, -16.95), vec3(1.4, 1.5, -16.95), vec3(1.4, 1.5, -14.0), vec3(-1.4, 1.5, -14.0), wall, Some(vec3(0.0, 0.5, -15.0)));
    m
}

pub fn model() -> Model {
    let mut parts = vec![Part::fixed(fuselage(), Seen::Outside), Part::fixed(cockpit(), Seen::Cockpit)];
    wing(&mut parts);
    engine(&mut parts, ENGINE_X);
    engine(&mut parts, -ENGINE_X);
    tail(&mut parts);
    gear(&mut parts);
    Model { parts, prop_disc: None }
}
