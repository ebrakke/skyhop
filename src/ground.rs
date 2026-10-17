//! Ground textures baked at startup: a colour map draped over the whole island (fields,
//! hedgerows, forest canopy, rock, beaches, with fine relief shading lit by the fixed sun) and
//! a small tiling detail texture that keeps the ground crisp up close.

use crate::mesh::{terrain_color, Col};
use crate::render::BAKE_SUN;
use crate::world::*;
use macroquad::math::{vec2, vec3, Vec2};

pub const COLORMAP: usize = 4096;
pub const DETAIL: usize = 512;

fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (z as u32).wrapping_mul(0x1656_67b1) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h & 0xff_ffff) as f32 / 16_777_216.0
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

fn rgb3(c: Col) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

/// Farmland: a patchwork of crops with hedgerows. Returns (colour, how much of it shows).
fn fields(x: f32, z: f32, h: f32, ny: f32) -> Option<([f32; 3], f32)> {
    let mask = smoothstep(0.36, 0.46, fbm(x / 2600.0 - 3.0, z / 2600.0 + 8.0, 3))
        * smoothstep(110.0, 60.0, h)
        * smoothstep(5.0, 9.0, h)
        * smoothstep(0.965, 0.985, ny)
        * (1.0 - smoothstep(0.15, 0.45, forest_density(x, z)))
        * (1.0 - smoothstep(0.02, 0.2, airport_factor(x, z)));
    if mask <= 0.0 {
        return None;
    }
    // Fields line up with a grid that slowly changes direction across the island.
    let ang = 0.35 + (fbm(x / 6000.0, z / 6000.0, 2) - 0.5) * 1.2;
    let (s, c) = ang.sin_cos();
    let p = vec2(x * c + z * s, -x * s + z * c);
    let col_w = 170.0;
    let ci = (p.x / col_w).floor();
    // each column of fields is cut into rows of its own length
    let row_h = 90.0 + 140.0 * hash(ci as i32, 7, 3);
    let pz = p.y + hash(ci as i32, 1, 9) * 300.0;
    let ri = (pz / row_h).floor();
    let id = (ci as i32, ri as i32);
    let (fu, fv) = ((p.x / col_w).rem_euclid(1.0), (pz / row_h).rem_euclid(1.0));
    let edge = (fu.min(1.0 - fu) * col_w).min(fv.min(1.0 - fv) * row_h);
    const CROPS: [[f32; 3]; 9] = [
        [0.76, 0.68, 0.38], // ripe wheat
        [0.52, 0.66, 0.27], // young crop
        [0.33, 0.52, 0.20], // pasture
        [0.48, 0.38, 0.26], // ploughed
        [0.80, 0.76, 0.33], // rapeseed
        [0.64, 0.64, 0.38], // hay
        [0.40, 0.58, 0.24], // grass
        [0.36, 0.55, 0.22], // grass
        [0.46, 0.62, 0.28], // young crop
    ];
    let k = (hash(id.0, id.1, 21) * CROPS.len() as f32) as usize % CROPS.len();
    let mut col = CROPS[k];
    // furrows / mowing stripes along the field
    let stripes = ((p.x * 0.9).sin() * 0.5 + 0.5) * 0.06;
    let mottle = (fbm(x / 40.0 + id.0 as f32, z / 40.0, 2) - 0.5) * 0.18;
    for ch in &mut col {
        *ch *= 1.0 + mottle - stripes;
    }
    let hedge = [0.17, 0.28, 0.12];
    let hedgy = hash(id.0, id.1, 5) > 0.35;
    if hedgy && edge < 5.0 {
        col = mix3(hedge, col, smoothstep(2.5, 5.0, edge));
    } else if edge < 2.0 {
        col = mix3([0.55, 0.52, 0.40], col, edge / 2.0); // a track between fields
    }
    Some((col, mask * 0.8))
}

fn ground_color(t: &Terrain, x: f32, z: f32) -> [f32; 3] {
    let h = t.height(x, z);
    let n = t.normal(x, z);
    let mut c = rgb3(terrain_color(h, n.y, x, z));
    if h > 1.5 {
        // meadow: patches of lusher and drier grass
        let m = fbm(x / 120.0 + 3.0, z / 120.0 - 5.0, 3);
        c = mix3(c, [c[0] * 1.12, c[1] * 1.05, c[2] * 0.85], (m - 0.45) * 2.0);
        c = mix3(c, [c[0] * 0.85, c[1] * 0.95, c[2] * 0.9], (0.5 - m) * 2.0);
        let mut farmed = 0.0;
        if let Some((fc, a)) = fields(x, z, h, n.y) {
            c = mix3(c, fc, a);
            farmed = a;
        }
        // forest canopy: clumps of crowns, dark gaps between them
        let fd = forest_density(x, z);
        if fd > 0.05 {
            let crowns = fbm(x / 14.0, z / 14.0, 2);
            let k = 0.75 + 0.45 * crowns;
            let canopy = [0.13 * k, 0.27 * k, 0.11 * k];
            c = mix3(c, canopy, fd * 0.8 * smoothstep(0.78, 0.9, n.y) * (1.0 - farmed));
        }
        // rock faces get strata
        let rock = smoothstep(0.86, 0.7, n.y);
        if rock > 0.0 {
            let strata = ((h * 0.35 + fbm(x / 60.0, z / 60.0, 2) * 6.0).sin() * 0.5 + 0.5) * 0.12;
            c = mix3(c, [c[0] * (0.9 + strata), c[1] * (0.9 + strata), c[2] * (0.92 + strata)], rock);
        }
    }
    // the grass strip: mown, with lighter stripes along its length and a darker edge band
    let sf = strip_factor(x, z);
    if sf > 0.0 {
        let along = (x - STRIP.0).abs() < STRIP_HALF_LEN && (z - STRIP.1).abs() < STRIP_HALF_W;
        let stripe = if ((z - STRIP.1) / 4.0).floor() as i32 % 2 == 0 { 1.08 } else { 0.96 };
        let mown = if along { [0.42 * stripe, 0.62 * stripe, 0.27 * stripe] } else { [0.38, 0.56, 0.25] };
        c = mix3(c, mown, sf.min(1.0) * if along { 1.0 } else { 0.5 });
    }
    // Fine relief the 75 m terrain mesh can't show, lit by the (fixed) sun.
    let bump = |x: f32, z: f32| fbm(x / 70.0 + 13.0, z / 70.0, 3) * 9.0 + fbm(x / 22.0, z / 22.0, 2) * 2.0;
    let e = 6.0;
    let g: Vec2 = vec2(bump(x + e, z) - bump(x - e, z), bump(x, z + e) - bump(x, z - e)) / (2.0 * e);
    let land = smoothstep(0.5, 3.0, h) * (1.0 - airport_factor(x, z));
    let nb = vec3(-g.x * land, 1.0, -g.y * land).normalize();
    let sun = BAKE_SUN.normalize();
    let shade = 1.0 + (nb.dot(sun) - sun.y) * 1.4;
    c.map(|v| (v * shade).clamp(0.0, 1.0))
}

/// RGB = ground colour, A = depth below sea level (for shallow-water colour and surf).
pub fn colormap(world: &World) -> Vec<u8> {
    let t = &world.terrain;
    let n = COLORMAP;
    let mut out = vec![0u8; n * n * 4];
    let threads = std::thread::available_parallelism().map_or(4, |c| c.get()).clamp(1, 16);
    let rows_per = n.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(rows_per * n * 4).enumerate() {
            s.spawn(move || {
                for (r, row) in chunk.chunks_mut(n * 4).enumerate() {
                    let j = k * rows_per + r;
                    let z = -HALF + (j as f32 + 0.5) / n as f32 * 2.0 * HALF;
                    for i in 0..n {
                        let x = -HALF + (i as f32 + 0.5) / n as f32 * 2.0 * HALF;
                        let c = ground_color(t, x, z);
                        let depth = (-t.height(x, z) / 40.0).clamp(0.0, 1.0);
                        let px = &mut row[i * 4..i * 4 + 4];
                        px[0] = (c[0] * 255.0) as u8;
                        px[1] = (c[1] * 255.0) as u8;
                        px[2] = (c[2] * 255.0) as u8;
                        px[3] = (depth * 255.0) as u8;
                    }
                }
            });
        }
    });
    // roads: stamp asphalt along each polyline, a couple of texels wide
    let texel = 2.0 * HALF / n as f32;
    let mut put = |x: f32, z: f32, c: [f32; 3], a: f32| {
        let (i, j) = (((x + HALF) / texel) as i64, ((z + HALF) / texel) as i64);
        if i < 0 || j < 0 || i >= n as i64 || j >= n as i64 {
            return;
        }
        let px = &mut out[(j as usize * n + i as usize) * 4..][..3];
        for k in 0..3 {
            px[k] = (px[k] as f32 * (1.0 - a) + c[k] * 255.0 * a) as u8;
        }
    };
    for road in &world.roads {
        for w in road.windows(2) {
            let (a, b) = (w[0], w[1]);
            let steps = ((b - a).length() / (texel * 0.5)) as usize + 1;
            for k in 0..=steps {
                let p = a.lerp(b, k as f32 / steps as f32);
                if t.height(p.x, p.z) < 0.5 {
                    continue;
                }
                put(p.x, p.z, [0.36, 0.36, 0.37], 0.9);
                let side = vec2(-(b - a).z, (b - a).x).normalize_or_zero() * texel * 0.6;
                put(p.x + side.x, p.z + side.y, [0.42, 0.41, 0.39], 0.45);
                put(p.x - side.x, p.z - side.y, [0.42, 0.41, 0.39], 0.45);
            }
        }
    }
    out
}

/// Value noise that wraps every `period` cells, so the texture tiles seamlessly.
fn tile_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |a: i32, b: i32| hash(a.rem_euclid(period), b.rem_euclid(period), seed);
    let a = lerp(h(xi, yi), h(xi + 1, yi), ux);
    let b = lerp(h(xi, yi + 1), h(xi + 1, yi + 1), ux);
    lerp(a, b, uy)
}

fn tile_fbm(u: f32, v: f32, base: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut p) = (0.0, 0.5, 0.0, base);
    for o in 0..octaves {
        let s = p as f32;
        sum += tile_noise(u * s, v * s, p, seed + o) * amp;
        norm += amp;
        amp *= 0.5;
        p *= 2;
    }
    sum / norm
}

/// R: soft mottling, G: grass blades (stretched noise), B: grit. Each in 0..255 around 128.
pub fn detail() -> Vec<u8> {
    let n = DETAIL;
    let mut out = vec![0u8; n * n * 4];
    for j in 0..n {
        for i in 0..n {
            let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
            let r = tile_fbm(u, v, 4, 5, 1);
            let g = tile_fbm(u, v, 16, 4, 11) * 0.6 + tile_noise(u * 128.0, v * 32.0, 32, 17) * 0.4;
            let b = tile_fbm(u, v, 64, 3, 23);
            let px = &mut out[(j * n + i) * 4..(j * n + i) * 4 + 4];
            px[0] = (r * 255.0) as u8;
            px[1] = (g * 255.0) as u8;
            px[2] = (b * 255.0) as u8;
            px[3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release colormap_png -- --ignored` writes target/colormap.png to look at.
    #[test]
    #[ignore]
    fn colormap_png() {
        let world = World::generate();
        let bytes = colormap(&world);
        let mut img = macroquad::texture::Image::gen_image_color(COLORMAP as u16, COLORMAP as u16, macroquad::color::BLACK);
        for (k, px) in bytes.chunks(4).enumerate() {
            img.bytes[k * 4..k * 4 + 3].copy_from_slice(&px[..3]);
        }
        img.export_png(concat!(env!("CARGO_MANIFEST_DIR"), "/target/colormap.png"));
    }
}
