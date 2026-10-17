//! CPU-side geometry: terrain, scenery, runway and the aircraft model.

use crate::world::*;
use macroquad::math::{vec3, Quat, Vec3};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub col: [f32; 4],
}

#[derive(Default)]
pub struct MeshData {
    pub v: Vec<Vertex>,
    pub i: Vec<u32>,
}

pub type Col = [f32; 4];

pub fn rgb(r: f32, g: f32, b: f32) -> Col {
    [r, g, b, 1.0]
}

fn mix(a: Col, b: Col, t: f32) -> Col {
    let t = t.clamp(0.0, 1.0);
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), 1.0]
}

impl MeshData {
    pub fn vert(&mut self, p: Vec3, n: Vec3, c: Col) -> u32 {
        self.v.push(Vertex { pos: p.into(), nrm: n.into(), col: c });
        (self.v.len() - 1) as u32
    }

    /// Flat-shaded triangle; the normal is flipped to point away from `inside`.
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: Col, inside: Option<Vec3>) {
        let mut n = (b - a).cross(c - a).normalize_or_zero();
        if let Some(o) = inside {
            if n.dot((a + b + c) / 3.0 - o) < 0.0 {
                n = -n;
            }
        }
        let i0 = self.vert(a, n, col);
        self.vert(b, n, col);
        self.vert(c, n, col);
        self.i.extend_from_slice(&[i0, i0 + 1, i0 + 2]);
    }

    pub fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, col: Col, inside: Option<Vec3>) {
        self.tri(a, b, c, col, inside);
        self.tri(a, c, d, col, inside);
    }

    /// Hexahedron from 8 corners: bottom loop 0..4 then top loop 4..8 (same winding).
    pub fn hexa(&mut self, p: [Vec3; 8], col: Col) {
        let c = p.iter().fold(Vec3::ZERO, |a, b| a + *b) / 8.0;
        let o = Some(c);
        self.quad(p[0], p[1], p[2], p[3], col, o);
        self.quad(p[4], p[5], p[6], p[7], col, o);
        for k in 0..4 {
            let n = (k + 1) % 4;
            self.quad(p[k], p[n], p[n + 4], p[k + 4], col, o);
        }
    }

    /// Axis-aligned box given min and max corners.
    pub fn aabb(&mut self, lo: Vec3, hi: Vec3, col: Col) {
        self.hexa(
            [
                vec3(lo.x, lo.y, lo.z),
                vec3(hi.x, lo.y, lo.z),
                vec3(hi.x, lo.y, hi.z),
                vec3(lo.x, lo.y, hi.z),
                vec3(lo.x, hi.y, lo.z),
                vec3(hi.x, hi.y, lo.z),
                vec3(hi.x, hi.y, hi.z),
                vec3(lo.x, hi.y, hi.z),
            ],
            col,
        );
    }

    /// Thin square beam from a to b.
    pub fn beam(&mut self, a: Vec3, b: Vec3, w: f32, col: Col) {
        let d = (b - a).normalize();
        let s1 = d.any_orthonormal_vector() * (w * 0.5);
        let s2 = d.cross(s1);
        let c = [s1 + s2, s1 - s2, -s1 - s2, -s1 + s2];
        self.hexa([a + c[0], a + c[1], a + c[2], a + c[3], b + c[0], b + c[1], b + c[2], b + c[3]], col);
    }

    /// Cylinder along `axis` (unit) centred at `c`.
    pub fn cylinder(&mut self, c: Vec3, axis: Vec3, r: f32, len: f32, sides: usize, col: Col, cap: Col) {
        let u = axis.any_orthonormal_vector();
        let w = axis.cross(u);
        let h = axis * (len * 0.5);
        let ring = |k: usize| {
            let a = k as f32 / sides as f32 * std::f32::consts::TAU;
            (u * a.cos() + w * a.sin()) * r
        };
        for k in 0..sides {
            let (p0, p1) = (ring(k), ring(k + 1));
            self.quad(c - h + p0, c - h + p1, c + h + p1, c + h + p0, col, Some(c));
            self.tri(c - h, c - h + p0, c - h + p1, cap, Some(c));
            self.tri(c + h, c + h + p0, c + h + p1, cap, Some(c));
        }
    }

    /// Cone with its base centred at `base`, pointing along `axis` for `len`.
    pub fn cone(&mut self, base: Vec3, axis: Vec3, r: f32, len: f32, sides: usize, col: Col) {
        let u = axis.any_orthonormal_vector();
        let w = axis.cross(u);
        let tip = base + axis * len;
        let inside = base + axis * (len * 0.25);
        for k in 0..sides {
            let a0 = k as f32 / sides as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / sides as f32 * std::f32::consts::TAU;
            let p0 = base + (u * a0.cos() + w * a0.sin()) * r;
            let p1 = base + (u * a1.cos() + w * a1.sin()) * r;
            self.tri(p0, p1, tip, col, Some(inside));
        }
    }

}

// -------------------------------------------------------------- terrain ---

pub fn terrain_color(h: f32, ny: f32, x: f32, z: f32) -> Col {
    let var = fbm(x / 260.0 + 7.0, z / 260.0, 3);
    let sand = rgb(0.80, 0.74, 0.55);
    let wet = rgb(0.55, 0.52, 0.42);
    let grass = mix(rgb(0.30, 0.50, 0.20), rgb(0.46, 0.60, 0.27), var);
    let forest = rgb(0.17, 0.33, 0.14);
    let rock = mix(rgb(0.46, 0.43, 0.40), rgb(0.55, 0.52, 0.47), var);
    let snow = rgb(0.95, 0.96, 0.99);

    let mut c = mix(grass, forest, forest_density(x, z) * 0.85);
    c = mix(c, rgb(0.42, 0.62, 0.30), airport_factor(x, z));
    c = mix(sand, c, smoothstep(2.0, 7.0, h));
    c = mix(wet, c, smoothstep(-6.0, 0.5, h));
    let rockiness = smoothstep(0.86, 0.72, ny).max(smoothstep(380.0, 640.0, h) * 0.8);
    c = mix(c, rock, rockiness);
    c = mix(c, snow, smoothstep(640.0, 760.0, h + var * 100.0) * smoothstep(0.55, 0.75, ny));
    c
}

pub fn terrain(t: &Terrain) -> MeshData {
    let n = GRID + 1;
    let mut m = MeshData::default();
    m.v.reserve(n * n);
    for j in 0..n {
        for i in 0..n {
            let (x, z) = Terrain::vertex_xz(i, j);
            let h = t.at(i, j);
            let hl = t.at(i.saturating_sub(1), j);
            let hr = t.at((i + 1).min(GRID), j);
            let hd = t.at(i, j.saturating_sub(1));
            let hu = t.at(i, (j + 1).min(GRID));
            let nrm = vec3(hl - hr, 2.0 * CELL, hd - hu).normalize();
            m.vert(vec3(x, h, z), nrm, terrain_color(h, nrm.y, x, z));
        }
    }
    m.i.reserve(GRID * GRID * 6);
    for j in 0..GRID {
        for i in 0..GRID {
            let a = (j * n + i) as u32;
            let b = a + 1;
            let c = a + n as u32;
            let d = c + 1;
            // matches Terrain::height: diagonal from (i,j) to (i+1,j+1)
            m.i.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }
    m
}

pub fn water() -> MeshData {
    let mut m = MeshData::default();
    let s = 60_000.0;
    let c = rgb(0.10, 0.32, 0.45);
    let up = Vec3::Y;
    let a = m.vert(vec3(-s, SEA, -s), up, c);
    m.vert(vec3(s, SEA, -s), up, c);
    m.vert(vec3(s, SEA, s), up, c);
    m.vert(vec3(-s, SEA, s), up, c);
    m.i.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    m
}

pub fn trees(world: &World) -> MeshData {
    let mut m = MeshData::default();
    let mut rng = Rng::new(99);
    for t in &world.trees {
        let h = t.height;
        let shade = rng.range(0.85, 1.15);
        let green = rgb(0.13 * shade, 0.30 * shade, 0.12 * shade);
        let trunk = rgb(0.35, 0.25, 0.15);
        m.cylinder(t.pos + vec3(0.0, h * 0.12, 0.0), Vec3::Y, h * 0.05, h * 0.25, 4, trunk, trunk);
        m.cone(t.pos + vec3(0.0, h * 0.2, 0.0), Vec3::Y, h * 0.33, h * 0.55, 6, green);
        m.cone(t.pos + vec3(0.0, h * 0.5, 0.0), Vec3::Y, h * 0.24, h * 0.5, 6, green);
    }
    m
}

pub fn buildings(world: &World) -> MeshData {
    let mut m = MeshData::default();
    for b in &world.buildings {
        let rot = Quat::from_rotation_y(b.yaw);
        let (hx, hz) = (b.size.x * 0.5, b.size.z * 0.5);
        let corner = |x: f32, y: f32, z: f32| b.pos + rot * vec3(x, y, z);
        let h = b.size.y;
        m.hexa(
            [
                corner(-hx, 0.0, -hz),
                corner(hx, 0.0, -hz),
                corner(hx, 0.0, hz),
                corner(-hx, 0.0, hz),
                corner(-hx, h, -hz),
                corner(hx, h, -hz),
                corner(hx, h, hz),
                corner(-hx, h, hz),
            ],
            b.wall,
        );
        if let Some(roof) = b.roof {
            let rh = h + b.size.z * 0.35;
            let inside = Some(b.pos + vec3(0.0, h, 0.0));
            let (r0, r1) = (corner(-hx - 0.4, rh, 0.0), corner(hx + 0.4, rh, 0.0));
            let (a, bb) = (corner(-hx - 0.4, h - 0.2, -hz - 0.6), corner(hx + 0.4, h - 0.2, -hz - 0.6));
            let (c, d) = (corner(hx + 0.4, h - 0.2, hz + 0.6), corner(-hx - 0.4, h - 0.2, hz + 0.6));
            m.quad(a, bb, r1, r0, roof, inside);
            m.quad(d, c, r1, r0, roof, inside);
            m.tri(corner(-hx, h, -hz), corner(-hx, h, hz), corner(-hx, rh, 0.0), b.wall, inside);
            m.tri(corner(hx, h, -hz), corner(hx, h, hz), corner(hx, rh, 0.0), b.wall, inside);
        }
    }
    m
}

fn icosphere() -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut v: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| vec3(x, y, z).normalize())
    .collect();
    let faces = [
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    let mut out = Vec::new();
    let mut mid = std::collections::HashMap::new();
    let mut midpoint = |a: usize, b: usize, v: &mut Vec<Vec3>| -> usize {
        let key = (a.min(b), a.max(b));
        *mid.entry(key).or_insert_with(|| {
            v.push(((v[a] + v[b]) * 0.5).normalize());
            v.len() - 1
        })
    };
    for f in faces {
        let ab = midpoint(f[0], f[1], &mut v);
        let bc = midpoint(f[1], f[2], &mut v);
        let ca = midpoint(f[2], f[0], &mut v);
        out.extend_from_slice(&[[f[0], ab, ca], [f[1], bc, ab], [f[2], ca, bc], [ab, bc, ca]]);
    }
    (v, out)
}

pub fn clouds(world: &World) -> MeshData {
    let (sv, sf) = icosphere();
    let mut m = MeshData::default();
    for b in &world.clouds {
        let base = m.v.len() as u32;
        for p in &sv {
            let n = (*p / b.radii).normalize();
            let shade = lerp(0.8, 1.0, smoothstep(-0.6, 0.5, p.y));
            m.vert(b.center + *p * b.radii, n, [shade, shade, shade * 1.02, 1.0]);
        }
        for f in &sf {
            m.i.extend_from_slice(&[base + f[0] as u32, base + f[1] as u32, base + f[2] as u32]);
        }
    }
    m
}

// --------------------------------------------------------------- runway ---

/// Runway surface and apron (drawn with depth offset over the terrain).
pub fn runway() -> MeshData {
    let mut m = MeshData::default();
    let y = RWY_ELEV + 0.04;
    let asphalt = rgb(0.24, 0.25, 0.27);
    let flat = |m: &mut MeshData, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, c: Col| {
        let a = m.vert(vec3(x0, y, z0), Vec3::Y, c);
        m.vert(vec3(x1, y, z0), Vec3::Y, c);
        m.vert(vec3(x1, y, z1), Vec3::Y, c);
        m.vert(vec3(x0, y, z1), Vec3::Y, c);
        m.i.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    };
    flat(&mut m, -RWY_HALF_LEN - 30.0, -RWY_HALF_W, RWY_HALF_LEN + 30.0, RWY_HALF_W, y, asphalt);
    // taxiway and apron toward the hangars
    flat(&mut m, 60.0, -95.0, 260.0, -45.0, y, rgb(0.33, 0.33, 0.34));
    flat(&mut m, 120.0, -45.0, 140.0, -RWY_HALF_W, y, rgb(0.30, 0.30, 0.31));
    m
}

/// White paint: centreline, threshold stripes, numbers and aiming points.
pub fn markings() -> MeshData {
    let mut m = MeshData::default();
    let y = RWY_ELEV + 0.08;
    let white = rgb(0.92, 0.92, 0.9);
    let rect = |m: &mut MeshData, cx: f32, cz: f32, lx: f32, lz: f32| {
        let a = m.vert(vec3(cx - lx / 2.0, y, cz - lz / 2.0), Vec3::Y, white);
        m.vert(vec3(cx + lx / 2.0, y, cz - lz / 2.0), Vec3::Y, white);
        m.vert(vec3(cx + lx / 2.0, y, cz + lz / 2.0), Vec3::Y, white);
        m.vert(vec3(cx - lx / 2.0, y, cz + lz / 2.0), Vec3::Y, white);
        m.i.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    };
    // centreline
    let mut x = -RWY_HALF_LEN + 90.0;
    while x < RWY_HALF_LEN - 90.0 {
        rect(&mut m, x + 15.0, 0.0, 30.0, 0.9);
        x += 50.0;
    }
    // edge lines
    rect(&mut m, 0.0, -RWY_HALF_W + 0.6, RWY_HALF_LEN * 2.0, 0.6);
    rect(&mut m, 0.0, RWY_HALF_W - 0.6, RWY_HALF_LEN * 2.0, 0.6);
    for side in [-1.0f32, 1.0] {
        let th = side * RWY_HALF_LEN; // threshold
        let inward = -side;
        let stripes = ((RWY_HALF_W - 3.0) / 3.0) as usize;
        for k in 0..stripes {
            let off = 2.2 + k as f32 * 3.0;
            rect(&mut m, th + inward * 22.0, off, 30.0, 1.6);
            rect(&mut m, th + inward * 22.0, -off, 30.0, 1.6);
        }
        // aiming point blocks
        rect(&mut m, th + inward * 400.0, 8.0, 45.0, 6.0);
        rect(&mut m, th + inward * 400.0, -8.0, 45.0, 6.0);
        // touchdown zone bars
        for (k, n) in [(150.0, 3), (300.0, 2), (550.0, 2), (700.0, 1), (850.0, 1)] {
            for j in 0..n {
                let off = 4.0 + j as f32 * 2.6 + 3.5;
                rect(&mut m, th + inward * k, off, 22.0, 1.8);
                rect(&mut m, th + inward * k, -off, 22.0, 1.8);
            }
        }
        // runway numbers, read when approaching from outside the threshold
        let digits: [u8; 2] = if side < 0.0 { [0, 9] } else { [2, 7] };
        let rot = if side < 0.0 { Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) } else { Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) };
        let center = vec3(th + inward * 60.0, 0.0, 0.0);
        for (di, d) in digits.iter().enumerate() {
            let dx = if di == 0 { -4.5 } else { 4.5 };
            for seg in segments(*d) {
                // segment in local digit space: x across (-1.5..1.5), y along (0..6), scaled
                let (a, b) = seg;
                let p0 = vec3((a.0 + dx) * 1.0, 0.0, -(a.1 * 3.0));
                let p1 = vec3((b.0 + dx) * 1.0, 0.0, -(b.1 * 3.0));
                let mid = (p0 + p1) * 0.5;
                let len = (p1 - p0).length() + 1.1;
                let along_z = (p1 - p0).z.abs() > 0.1;
                let (lx, lz) = if along_z { (1.1, len) } else { (len, 1.1) };
                let world_mid = center + rot * (mid + vec3(0.0, 0.0, 9.0));
                let rotated = rot * vec3(lx, 0.0, lz);
                rect(&mut m, world_mid.x, world_mid.z, rotated.x.abs(), rotated.z.abs());
            }
        }
    }
    m
}

type Seg = ((f32, f32), (f32, f32));

/// Seven-segment digit strokes in a 3 x 6 box (x -1.5..1.5, y 0..6 -> scaled later).
fn segments(d: u8) -> Vec<Seg> {
    let (l, r) = (-1.5, 1.5);
    let a = ((l, 6.0), (r, 6.0));
    let b = ((r, 6.0), (r, 3.0));
    let c = ((r, 3.0), (r, 0.0));
    let dd = ((l, 0.0), (r, 0.0));
    let e = ((l, 3.0), (l, 0.0));
    let f = ((l, 6.0), (l, 3.0));
    let g = ((l, 3.0), (r, 3.0));
    let segs: &[Seg] = match d {
        0 => &[a, b, c, dd, e, f],
        2 => &[a, b, g, e, dd],
        7 => &[a, b, c],
        9 => &[a, b, c, dd, f, g],
        _ => &[],
    };
    segs.iter().map(|((x0, y0), (x1, y1))| ((*x0, *y0 / 3.0 * 2.0), (*x1, *y1 / 3.0 * 2.0))).collect()
}

// ---------------------------------------------------------- props/misc ---

pub fn unit_cube() -> MeshData {
    let mut m = MeshData::default();
    m.aabb(vec3(-0.5, 0.0, -0.5), vec3(0.5, 1.0, 0.5), rgb(1.0, 1.0, 1.0));
    m
}

/// Torus in the XY plane (axis +Z), major radius 1.
pub fn torus(minor: f32) -> MeshData {
    let mut m = MeshData::default();
    let (nu, nv) = (48, 8);
    for u in 0..=nu {
        let a = u as f32 / nu as f32 * std::f32::consts::TAU;
        let c = vec3(a.cos(), a.sin(), 0.0);
        for v in 0..=nv {
            let b = v as f32 / nv as f32 * std::f32::consts::TAU;
            let n = c * b.cos() + Vec3::Z * b.sin();
            m.vert(c + n * minor, n, rgb(1.0, 1.0, 1.0));
        }
    }
    for u in 0..nu {
        for v in 0..nv {
            let a = (u * (nv + 1) + v) as u32;
            let b = a + (nv + 1) as u32;
            m.i.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// A party balloon, radius 1 and a little taller than wide, pinched toward the knot, on a
/// ribbon. White, so it takes the tint's colour.
pub fn balloon() -> MeshData {
    let mut m = MeshData::default();
    let (nu, nv) = (20, 14);
    let white = rgb(1.0, 1.0, 1.0);
    for v in 0..=nv {
        let th = v as f32 / nv as f32 * std::f32::consts::PI;
        let pinch = 1.0 - 0.45 * crate::world::smoothstep(0.55, 1.0, th / std::f32::consts::PI);
        for u in 0..=nu {
            let ph = u as f32 / nu as f32 * std::f32::consts::TAU;
            let n = vec3(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin());
            m.vert(vec3(n.x * pinch, n.y * 1.18, n.z * pinch), n, white);
        }
    }
    for v in 0..nv {
        for u in 0..nu {
            let a = (v * (nu + 1) + u) as u32;
            let b = a + (nu + 1) as u32;
            m.i.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m.cone(vec3(0.0, -1.32, 0.0), Vec3::Y, 0.14, 0.2, 8, white);
    m.beam(vec3(0.0, -1.3, 0.0), vec3(0.08, -3.6, 0.05), 0.035, white);
    m
}

/// A rainbow road along `path`: six coloured lanes between white edges, banked into the
/// turns. Drawn see-through and glowing.
pub fn rainbow_road(path: &[Vec3]) -> MeshData {
    let mut m = MeshData::default();
    let bands: [(f32, Col); 8] = [
        (1.2, rgb(1.0, 1.0, 1.0)),
        (5.5, rgb(1.0, 0.2, 0.25)),
        (5.5, rgb(1.0, 0.6, 0.1)),
        (5.5, rgb(1.0, 0.92, 0.2)),
        (5.5, rgb(0.25, 0.85, 0.35)),
        (5.5, rgb(0.25, 0.55, 1.0)),
        (5.5, rgb(0.7, 0.35, 1.0)),
        (1.2, rgb(1.0, 1.0, 1.0)),
    ];
    let width: f32 = bands.iter().map(|b| b.0).sum();
    let n = path.len();
    if n < 3 {
        return m;
    }
    let mut prev: Option<u32> = None;
    for i in 0..n {
        let (a, b) = (path[i.saturating_sub(1)], path[(i + 1).min(n - 1)]);
        let t = (b - a).normalize_or_zero();
        // bank into the turn: how fast the heading is changing here
        let (a2, b2) = (path[i.saturating_sub(8)], path[(i + 8).min(n - 1)]);
        let turn = (a - a2).normalize_or_zero().cross((b2 - b).normalize_or_zero()).y;
        let bank = (turn * 2.5).clamp(-0.45, 0.45);
        let flat = t.cross(Vec3::Y).normalize_or_zero();
        let side = Quat::from_axis_angle(t, -bank) * flat;
        let up = side.cross(t).normalize_or_zero();
        let base = m.v.len() as u32;
        let mut x = -width / 2.0;
        for (w, c) in bands {
            m.vert(path[i] + side * x, up, c);
            m.vert(path[i] + side * (x + w), up, c);
            x += w;
        }
        if let Some(p) = prev {
            for k in 0..bands.len() as u32 {
                let (a0, a1, b0, b1) = (p + k * 2, p + k * 2 + 1, base + k * 2, base + k * 2 + 1);
                m.i.extend_from_slice(&[a0, a1, b1, a0, b1, b0]);
            }
        }
        prev = Some(base);
    }
    m
}

/// Unit sprite for smoke and dust: opaque in the middle, fading to nothing at the rim.
pub fn puff() -> MeshData {
    let mut m = MeshData::default();
    let c = m.vert(Vec3::ZERO, Vec3::Z, [1.0, 1.0, 1.0, 1.0]);
    for k in 0..=24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        m.vert(vec3(a.cos(), a.sin(), 0.0), Vec3::Z, [1.0, 1.0, 1.0, 0.0]);
    }
    for k in 0..24 {
        m.i.extend_from_slice(&[c, c + 1 + k, c + 2 + k]);
    }
    m
}

/// A wide flat sheet for the overcast deck, centred under the camera each frame.
pub fn deck() -> MeshData {
    let mut m = MeshData::default();
    let s = 50_000.0;
    let c = rgb(1.0, 1.0, 1.0);
    let a = m.vert(vec3(-s, 0.0, -s), Vec3::Y, c);
    m.vert(vec3(s, 0.0, -s), Vec3::Y, c);
    m.vert(vec3(s, 0.0, s), Vec3::Y, c);
    m.vert(vec3(-s, 0.0, s), Vec3::Y, c);
    m.i.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    m
}

pub fn disc(r: f32) -> MeshData {
    let mut m = MeshData::default();
    let c = m.vert(Vec3::ZERO, Vec3::Z, rgb(1.0, 1.0, 1.0));
    for k in 0..=32 {
        let a = k as f32 / 32.0 * std::f32::consts::TAU;
        m.vert(vec3(a.cos() * r, a.sin() * r, 0.0), Vec3::Z, rgb(1.0, 1.0, 1.0));
    }
    for k in 0..32 {
        m.i.extend_from_slice(&[c, c + 1 + k, c + 2 + k]);
    }
    m
}

// ------------------------------------------------------------- aircraft ---

pub const FLAP_PIVOT: Vec3 = vec3(1.75, 1.0, 0.25);
pub const AIL_PIVOT: Vec3 = vec3(4.2, 1.05, 0.25);
pub const ELEV_PIVOT: Vec3 = vec3(0.0, 0.24, 4.3);
pub const RUD_PIVOT: Vec3 = vec3(0.0, 1.0, 4.45);
pub const PROP_PIVOT: Vec3 = vec3(0.0, -0.05, -2.75);

/// What moves a part of the model.
#[derive(Clone, Copy, PartialEq)]
pub enum Drive {
    Fixed,
    /// Turns about its hinge by the flap angle (trailing edge down).
    Flap,
    /// Turns by the aileron deflection times this sign (one per side).
    Aileron(f32),
    Elevator,
    Rudder,
    /// Spins with the propeller or fan.
    Spin,
    /// Hinges up with the speedbrake.
    Spoiler,
    /// Slides up into the body as the gear retracts.
    Gear,
    /// Thrust-reverser sleeve: slides aft.
    Reverser,
    /// Afterburner flame: grows with the burner and glows.
    Burner,
}

/// When a part is drawn.
#[derive(Clone, Copy, PartialEq)]
pub enum Seen {
    Always,
    /// Hidden from the cockpit (fuselage, prop): it would block the view.
    Outside,
    Cockpit,
}

pub struct Part {
    pub mesh: MeshData,
    pub pivot: Vec3,
    /// Hinge axis; for flaps and ailerons pointing along +x-ish so a positive angle lowers the trailing edge.
    pub axis: Vec3,
    pub drive: Drive,
    pub seen: Seen,
}

impl Part {
    pub fn fixed(mesh: MeshData, seen: Seen) -> Self {
        Part { mesh, pivot: Vec3::ZERO, axis: Vec3::X, drive: Drive::Fixed, seen }
    }
    pub fn hinged(mesh: MeshData, pivot: Vec3, axis: Vec3, drive: Drive, seen: Seen) -> Self {
        Part { mesh, pivot, axis: axis.normalize(), drive, seen }
    }
}

pub struct Model {
    pub parts: Vec<Part>,
    /// Translucent propeller disc: centre and radius.
    pub prop_disc: Option<(Vec3, f32)>,
}

fn wing_y(x: f32) -> f32 {
    0.92 + (x.abs() - 0.55).max(0.0) * 0.03
}

/// A slab of wing between span stations x0..x1 (same side) and chord z0..z1.
fn wing_slab(m: &mut MeshData, x0: f32, x1: f32, z0: f32, z1: f32, col: Col) {
    let (t0, t1) = (0.15 - x0.abs() * 0.008, 0.15 - x1.abs() * 0.008);
    let (y0, y1) = (wing_y(x0), wing_y(x1));
    m.hexa(
        [
            vec3(x0, y0, z0),
            vec3(x1, y1, z0),
            vec3(x1, y1, z1),
            vec3(x0, y0, z1),
            vec3(x0, y0 + t0, z0),
            vec3(x1, y1 + t1, z0),
            vec3(x1, y1 + t1 * 0.6, z1),
            vec3(x0, y0 + t0 * 0.6, z1),
        ],
        col,
    );
}

/// The high-wing trainer.
pub fn trainer() -> Model {
    let white = rgb(0.94, 0.94, 0.95);
    let light = rgb(0.80, 0.81, 0.84);
    let stripe = rgb(0.12, 0.33, 0.70);
    let red = rgb(0.82, 0.16, 0.14);
    let glass = rgb(0.10, 0.16, 0.24);
    let dark = rgb(0.13, 0.13, 0.14);
    let tire = rgb(0.08, 0.08, 0.08);

    // Fuselage loft: (z, half-width, y-bottom, y-top)
    let sections = [
        (-2.6, 0.42, -0.45, 0.35),
        (-1.2, 0.55, -0.6, 0.5),
        (-0.4, 0.6, -0.6, 0.95),
        (0.9, 0.58, -0.55, 0.95),
        (2.0, 0.40, -0.35, 0.68),
        (4.6, 0.10, 0.05, 0.45),
    ];
    let ring = |s: (f32, f32, f32, f32)| -> [Vec3; 8] {
        let (z, hw, yb, yt) = s;
        let (cy, ry) = ((yb + yt) * 0.5, (yt - yb) * 0.5);
        std::array::from_fn(|k| {
            let th = (22.5 + 45.0 * k as f32).to_radians();
            vec3(hw * th.sin() / 0.924, cy + ry * th.cos() / 0.924, z)
        })
    };
    let mut cowl = MeshData::default();
    let mut fus = MeshData::default();
    for si in 0..sections.len() - 1 {
        let (a, b) = (ring(sections[si]), ring(sections[si + 1]));
        let inside = vec3(0.0, (sections[si].2 + sections[si].3) * 0.5, (sections[si].0 + sections[si + 1].0) * 0.5);
        for k in 0..8 {
            let n = (k + 1) % 8;
            // face k sits between corner k and k+1; centre angle 45 + 45k deg (0 = top)
            let mut col = match k {
                7 => white,
                2..=4 => light,
                1 | 5 => stripe,
                _ => white,
            };
            if si == 1 && (k == 7 || k == 0 || k == 6) {
                col = glass; // windshield
            }
            if si == 2 && (k == 0 || k == 6) {
                col = glass; // side windows
            }
            let target = if si == 0 { &mut cowl } else { &mut fus };
            target.quad(a[k], a[n], b[n], b[k], col, Some(inside));
        }
    }
    // caps
    let nose = ring(sections[0]);
    let back = ring(sections[1]);
    let tail = ring(sections[5]);
    for k in 1..7 {
        cowl.tri(nose[0], nose[k], nose[k + 1], dark, Some(vec3(0.0, 0.0, 0.0)));
        cowl.tri(back[0], back[k], back[k + 1], dark, Some(vec3(0.0, 0.0, -3.0)));
        fus.tri(tail[0], tail[k], tail[k + 1], white, Some(vec3(0.0, 0.2, 0.0)));
    }
    // spinner
    cowl.cone(vec3(0.0, -0.05, -2.62), Vec3::NEG_Z, 0.26, 0.55, 10, red);

    // landing gear
    for s in [-1.0f32, 1.0] {
        fus.beam(vec3(s * 0.45, -0.55, 0.35), vec3(s * 1.25, -1.12, 0.35), 0.08, light);
        fus.cylinder(vec3(s * 1.25, -1.15, 0.35), Vec3::X, 0.3, 0.18, 10, tire, light);
        fus.hexa(
            [
                vec3(s * 1.15, -1.25, -0.2),
                vec3(s * 1.35, -1.25, -0.2),
                vec3(s * 1.35, -1.25, 0.8),
                vec3(s * 1.15, -1.25, 0.8),
                vec3(s * 1.15, -0.95, 0.0),
                vec3(s * 1.35, -0.95, 0.0),
                vec3(s * 1.35, -0.95, 0.7),
                vec3(s * 1.15, -0.95, 0.7),
            ],
            red,
        );
    }
    cowl.beam(vec3(0.0, -0.5, -2.0), vec3(0.0, -1.12, -2.0), 0.09, light);
    cowl.cylinder(vec3(0.0, -1.15, -2.0), Vec3::X, 0.25, 0.14, 10, tire, light);

    // tail surfaces
    fus.hexa(
        [
            vec3(-1.7, 0.2, 3.85),
            vec3(1.7, 0.2, 3.85),
            vec3(1.7, 0.2, 4.3),
            vec3(-1.7, 0.2, 4.3),
            vec3(-1.7, 0.27, 3.85),
            vec3(1.7, 0.27, 3.85),
            vec3(1.7, 0.26, 4.3),
            vec3(-1.7, 0.26, 4.3),
        ],
        white,
    );
    fus.hexa(
        [
            vec3(-0.04, 0.35, 3.2),
            vec3(-0.04, 0.35, 4.45),
            vec3(-0.04, 1.75, 4.45),
            vec3(-0.04, 1.75, 4.0),
            vec3(0.04, 0.35, 3.2),
            vec3(0.04, 0.35, 4.45),
            vec3(0.04, 1.75, 4.45),
            vec3(0.04, 1.75, 4.0),
        ],
        white,
    );
    let mut elevator = MeshData::default();
    elevator.hexa(
        [
            vec3(-1.7, 0.2, 4.3),
            vec3(1.7, 0.2, 4.3),
            vec3(1.6, 0.225, 4.75),
            vec3(-1.6, 0.225, 4.75),
            vec3(-1.7, 0.26, 4.3),
            vec3(1.7, 0.26, 4.3),
            vec3(1.6, 0.235, 4.75),
            vec3(-1.6, 0.235, 4.75),
        ],
        white,
    );
    let mut rudder = MeshData::default();
    rudder.hexa(
        [
            vec3(-0.03, 0.3, 4.45),
            vec3(-0.03, 0.3, 4.8),
            vec3(-0.03, 1.75, 4.7),
            vec3(-0.03, 1.75, 4.45),
            vec3(0.03, 0.3, 4.45),
            vec3(0.03, 0.3, 4.8),
            vec3(0.03, 1.75, 4.7),
            vec3(0.03, 1.75, 4.45),
        ],
        stripe,
    );

    // wings
    let mut wings = MeshData::default();
    wing_slab(&mut wings, -0.6, 0.6, -0.9, 0.6, white);
    let mut flap_l = MeshData::default();
    let mut flap_r = MeshData::default();
    let mut ail_l = MeshData::default();
    let mut ail_r = MeshData::default();
    for s in [-1.0f32, 1.0] {
        wing_slab(&mut wings, s * 0.6, s * 5.45, -0.9, 0.25, white);
        wing_slab(&mut wings, s * 5.45, s * 5.6, -0.9, 0.55, red);
        let (flap, ail) = if s < 0.0 { (&mut flap_l, &mut ail_l) } else { (&mut flap_r, &mut ail_r) };
        wing_slab(flap, s * 0.62, s * 2.9, 0.25, 0.6, light);
        wing_slab(ail, s * 2.95, s * 5.4, 0.25, 0.6, white);
        wings.beam(vec3(s * 0.55, -0.35, -0.3), vec3(s * 2.6, wing_y(2.6), -0.3), 0.07, light);
    }

    let mut prop = MeshData::default();
    prop.beam(vec3(0.0, -1.0, -2.78), vec3(0.0, 0.9, -2.78), 0.11, dark);

    let (fp, ap) = (FLAP_PIVOT, AIL_PIVOT);
    let mirror = |p: Vec3| vec3(-p.x, p.y, p.z);
    Model {
        parts: vec![
            Part::fixed(cowl, Seen::Always),
            Part::fixed(wings, Seen::Always),
            Part::hinged(flap_l, mirror(fp), Vec3::X, Drive::Flap, Seen::Always),
            Part::hinged(flap_r, fp, Vec3::X, Drive::Flap, Seen::Always),
            Part::hinged(ail_l, mirror(ap), Vec3::X, Drive::Aileron(1.0), Seen::Always),
            Part::hinged(ail_r, ap, Vec3::X, Drive::Aileron(-1.0), Seen::Always),
            Part::fixed(cockpit(), Seen::Cockpit),
            Part::hinged(prop, PROP_PIVOT, Vec3::Z, Drive::Spin, Seen::Outside),
            Part::fixed(fus, Seen::Outside),
            Part::hinged(elevator, ELEV_PIVOT, Vec3::X, Drive::Elevator, Seen::Outside),
            Part::hinged(rudder, RUD_PIVOT, Vec3::Y, Drive::Rudder, Seen::Outside),
        ],
        prop_disc: Some((vec3(0.0, -0.05, -2.8), 0.95)),
    }
}

/// The view from the left seat: glareshield, window frames, door sills and headliner,
/// following the fuselage loft (windshield from z -1.2, y 0.5 up to z -0.4, y 0.95).
fn cockpit() -> MeshData {
    let dash = rgb(0.17, 0.17, 0.18);
    let frame = rgb(0.30, 0.30, 0.31);
    let trim = rgb(0.50, 0.47, 0.42);
    let roof = rgb(0.66, 0.64, 0.60);
    let mut m = MeshData::default();
    // glareshield and panel face
    m.hexa(
        [
            vec3(-0.6, 0.2, -0.78),
            vec3(0.6, 0.2, -0.78),
            vec3(0.6, 0.2, -1.2),
            vec3(-0.6, 0.2, -1.2),
            vec3(-0.6, 0.57, -0.78),
            vec3(0.6, 0.57, -0.78),
            vec3(0.56, 0.53, -1.2),
            vec3(-0.56, 0.53, -1.2),
        ],
        dash,
    );
    m.beam(vec3(-0.6, 0.57, -0.77), vec3(0.6, 0.57, -0.77), 0.03, rgb(0.09, 0.09, 0.1));
    // magnetic compass on top of the glareshield
    m.aabb(vec3(-0.025, 0.57, -0.98), vec3(0.025, 0.615, -0.93), rgb(0.08, 0.08, 0.08));
    m.aabb(vec3(-0.018, 0.578, -0.932), vec3(0.018, 0.607, -0.928), rgb(0.85, 0.82, 0.7));
    // windshield frame: side pillars, centre strip and the top bow
    for sx in [-1.0f32, 1.0] {
        m.beam(vec3(0.56 * sx, 0.53, -1.18), vec3(0.6 * sx, 0.9, -0.42), 0.045, frame);
        m.beam(vec3(0.6 * sx, 0.4, -0.42), vec3(0.6 * sx, 0.91, -0.42), 0.04, frame);
        // door sill and lower door panel
        m.beam(vec3(0.6 * sx, 0.42, -1.2), vec3(0.6 * sx, 0.42, 0.6), 0.04, frame);
        m.quad(vec3(0.61 * sx, 0.0, -1.2), vec3(0.61 * sx, 0.0, 0.6), vec3(0.61 * sx, 0.42, 0.6), vec3(0.61 * sx, 0.42, -1.2), trim, Some(vec3(0.0, 0.3, 0.0)));
    }
    m.beam(vec3(0.0, 0.53, -1.18), vec3(0.0, 0.9, -0.44), 0.018, frame);
    m.beam(vec3(-0.6, 0.9, -0.42), vec3(0.6, 0.9, -0.42), 0.045, frame);
    // headliner
    m.quad(vec3(-0.62, 0.91, -0.42), vec3(0.62, 0.91, -0.42), vec3(0.62, 0.91, 0.9), vec3(-0.62, 0.91, 0.9), roof, Some(vec3(0.0, 0.5, 0.0)));
    m
}
