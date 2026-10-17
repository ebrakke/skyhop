//! Procedural island: heightmap, scenery placement and the ring course.

use macroquad::math::{vec3, Vec3};

pub const HALF: f32 = 12_000.0; // terrain covers [-HALF, HALF] on x and z
pub const GRID: usize = 320; // cells per side
pub const CELL: f32 = 2.0 * HALF / GRID as f32;
pub const SEA: f32 = 0.0;

// Runway 09/27 runs east-west through the origin. Heading 090 is +x.
pub const RWY_ELEV: f32 = 12.0;
pub const RWY_HALF_LEN: f32 = 700.0;
pub const RWY_HALF_W: f32 = 15.0;
/// Distance from each threshold to its PAPI lights.
pub const PAPI_DIST: f32 = 300.0;

pub const TOWER_POS: Vec3 = vec3(-180.0, RWY_ELEV + 24.0, -110.0);

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ---------------------------------------------------------------- noise ---

fn hash2(x: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xff_ffff) as f32 / 16_777_216.0
}

fn vnoise(x: f32, z: f32) -> f32 {
    let (xi, zi) = (x.floor(), z.floor());
    let (fx, fz) = (x - xi, z - zi);
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let (ix, iz) = (xi as i32, zi as i32);
    let a = hash2(ix, iz);
    let b = hash2(ix + 1, iz);
    let c = hash2(ix, iz + 1);
    let d = hash2(ix + 1, iz + 1);
    lerp(lerp(a, b, ux), lerp(c, d, ux), uz)
}

/// Fractal value noise in [0, 1].
pub fn fbm(mut x: f32, mut z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    for _ in 0..octaves {
        sum += vnoise(x, z) * amp;
        norm += amp;
        amp *= 0.5;
        // rotate a little between octaves to hide grid artifacts
        let nx = x * 1.6 - z * 1.2 + 17.3;
        z = x * 1.2 + z * 1.6 - 9.1;
        x = nx;
    }
    sum / norm
}

/// Ridged noise in [0, 1]: sharp crests, good for mountain chains.
fn ridged(mut x: f32, mut z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    for _ in 0..octaves {
        let n = 1.0 - (vnoise(x, z) * 2.0 - 1.0).abs();
        sum += n * n * amp;
        norm += amp;
        amp *= 0.5;
        let nx = x * 1.7 - z * 1.1 + 3.7;
        z = x * 1.1 + z * 1.7 + 5.3;
        x = nx;
    }
    sum / norm
}

pub fn forest_density(x: f32, z: f32) -> f32 {
    smoothstep(0.52, 0.66, fbm(x / 1100.0 + 31.0, z / 1100.0 - 17.0, 4))
}

/// 1 inside the airport's flattened zone, falling to 0 outside it.
pub fn airport_factor(x: f32, z: f32) -> f32 {
    let ax = (x.abs() - (RWY_HALF_LEN + 250.0)).max(0.0);
    let az = (z.abs() - 220.0).max(0.0);
    1.0 - smoothstep(0.0, 900.0, (ax * ax + az * az).sqrt())
}

fn raw_height(x: f32, z: f32) -> f32 {
    let d = (x * x + z * z).sqrt();
    let wobble = fbm(x / 4000.0 + 11.0, z / 4000.0 - 7.0, 4) - 0.5;
    let r = d / 8000.0 + wobble * 0.55;
    let land = 1.0 - smoothstep(0.72, 1.08, r);

    let hills = fbm(x / 1800.0, z / 1800.0, 5);
    let base = 6.0 + 90.0 * hills * hills + 22.0 * fbm(x / 500.0 + 4.0, z / 500.0, 3);

    let ridge = ridged(x / 3300.0 + 3.1, z / 3300.0 - 1.7, 5);
    let ring = smoothstep(1800.0, 4500.0, d) * (1.0 - smoothstep(0.55, 0.9, r));
    let massif = smoothstep(0.3, 0.6, fbm(x / 6000.0 + 5.0, z / 6000.0 + 9.0, 2));
    let mountains = ridge.powf(2.0) * 880.0 * ring * (0.35 + 0.65 * massif);

    let mut h = lerp(-45.0, base + mountains, land);
    // Keep the approach paths to both ends of the runway clear (a gentle valley).
    let corridor = 1.0 - smoothstep(350.0, 900.0, z.abs());
    if corridor > 0.0 {
        let ceiling = RWY_ELEV - 4.0 + (x.abs() - RWY_HALF_LEN - 200.0).max(0.0) * 0.035;
        h = lerp(h, h.min(ceiling), corridor);
    }
    lerp(h, RWY_ELEV, airport_factor(x, z))
}

// -------------------------------------------------------------- terrain ---

pub struct Terrain {
    pub h: Vec<f32>,
}

impl Terrain {
    pub fn generate() -> Self {
        let n = GRID + 1;
        let mut h = vec![0.0; n * n];
        for j in 0..n {
            for i in 0..n {
                let (x, z) = Self::vertex_xz(i, j);
                h[j * n + i] = raw_height(x, z);
            }
        }
        Terrain { h }
    }

    pub fn vertex_xz(i: usize, j: usize) -> (f32, f32) {
        (-HALF + i as f32 * CELL, -HALF + j as f32 * CELL)
    }

    pub fn at(&self, i: usize, j: usize) -> f32 {
        self.h[j * (GRID + 1) + i]
    }

    /// Height exactly matching the rendered triangles.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let gx = (x + HALF) / CELL;
        let gz = (z + HALF) / CELL;
        if gx < 0.0 || gz < 0.0 || gx >= GRID as f32 || gz >= GRID as f32 {
            return -45.0;
        }
        let (i, j) = (gx as usize, gz as usize);
        let (fx, fz) = (gx - i as f32, gz - j as f32);
        let h00 = self.at(i, j);
        let h10 = self.at(i + 1, j);
        let h01 = self.at(i, j + 1);
        let h11 = self.at(i + 1, j + 1);
        if fx >= fz {
            h00 + fx * (h10 - h00) + fz * (h11 - h10)
        } else {
            h00 + fz * (h01 - h00) + fx * (h11 - h01)
        }
    }

    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = CELL * 0.5;
        vec3(
            self.height(x - e, z) - self.height(x + e, z),
            2.0 * e,
            self.height(x, z - e) - self.height(x, z + e),
        )
        .normalize()
    }

    /// Ground or water surface, whichever is higher.
    pub fn surface(&self, x: f32, z: f32) -> f32 {
        self.height(x, z).max(SEA)
    }
}

// ---------------------------------------------------------------- world ---

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
    pub fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

pub struct Tree {
    pub pos: Vec3,
    pub height: f32,
}

pub struct Building {
    pub pos: Vec3, // centre of the footprint, at ground level
    pub size: Vec3,
    pub yaw: f32,
    pub wall: [f32; 4],
    pub roof: Option<[f32; 4]>,
}

pub struct CloudBlob {
    pub center: Vec3,
    pub radii: Vec3,
}

pub struct Ring {
    pub pos: Vec3,
    pub normal: Vec3,
    pub radius: f32,
}

pub struct World {
    pub terrain: Terrain,
    pub trees: Vec<Tree>,
    pub buildings: Vec<Building>,
    pub clouds: Vec<CloudBlob>,
    pub rings: Vec<Ring>,
}

pub const RING_START: Vec3 = vec3(-900.0, 450.0, 0.0);

impl World {
    pub fn generate() -> Self {
        let terrain = Terrain::generate();
        let mut rng = Rng::new(42);
        let trees = place_trees(&terrain, &mut rng);
        let buildings = place_buildings(&terrain, &mut rng);
        let clouds = place_clouds(&mut rng);
        let rings = place_rings(&terrain);
        World { terrain, trees, buildings, clouds, rings }
    }

    /// How deep the point is inside a cloud, 0 = outside, 1 = at a blob centre.
    pub fn cloud_density(&self, p: Vec3) -> f32 {
        let mut d: f32 = 0.0;
        for b in &self.clouds {
            let q = (p - b.center) / b.radii;
            d = d.max(1.0 - q.length_squared());
        }
        d
    }
}

fn place_trees(t: &Terrain, rng: &mut Rng) -> Vec<Tree> {
    let mut trees = Vec::new();
    for _ in 0..90_000 {
        if trees.len() >= 9000 {
            break;
        }
        let x = rng.range(-HALF * 0.95, HALF * 0.95);
        let z = rng.range(-HALF * 0.95, HALF * 0.95);
        let h = t.height(x, z);
        if !(6.0..720.0).contains(&h) || airport_factor(x, z) > 0.05 {
            continue;
        }
        if t.normal(x, z).y < 0.8 {
            continue;
        }
        let density = forest_density(x, z) * 0.95 + 0.012;
        if rng.f() > density {
            continue;
        }
        trees.push(Tree { pos: vec3(x, h - 0.5, z), height: rng.range(7.0, 16.0) });
    }
    trees
}

fn place_buildings(t: &Terrain, rng: &mut Rng) -> Vec<Building> {
    let mut b = Vec::new();
    let g = RWY_ELEV;
    let gray = [0.78, 0.79, 0.8, 1.0];
    // Airport: hangars, tower, fuel shed.
    b.push(Building { pos: vec3(160.0, g, -120.0), size: vec3(42.0, 13.0, 32.0), yaw: 0.0, wall: gray, roof: Some([0.55, 0.2, 0.18, 1.0]) });
    b.push(Building { pos: vec3(220.0, g, -120.0), size: vec3(30.0, 10.0, 26.0), yaw: 0.0, wall: [0.85, 0.83, 0.75, 1.0], roof: Some([0.35, 0.4, 0.5, 1.0]) });
    b.push(Building { pos: vec3(TOWER_POS.x, g, TOWER_POS.z), size: vec3(7.0, 20.0, 7.0), yaw: 0.0, wall: [0.92, 0.92, 0.9, 1.0], roof: None });
    b.push(Building { pos: vec3(TOWER_POS.x, g + 20.0, TOWER_POS.z), size: vec3(10.0, 4.5, 10.0), yaw: 0.0, wall: [0.15, 0.22, 0.28, 1.0], roof: None });
    b.push(Building { pos: vec3(80.0, g, -105.0), size: vec3(10.0, 4.0, 8.0), yaw: 0.0, wall: [0.9, 0.75, 0.3, 1.0], roof: None });

    // A few villages on flat ground.
    let wall_cols = [[0.95, 0.93, 0.86, 1.0], [0.92, 0.85, 0.72, 1.0], [0.85, 0.88, 0.9, 1.0], [0.96, 0.9, 0.8, 1.0]];
    let roof_cols = [[0.62, 0.22, 0.16, 1.0], [0.45, 0.3, 0.22, 1.0], [0.3, 0.33, 0.38, 1.0]];
    let mut villages = 0;
    for _ in 0..4000 {
        if villages >= 5 {
            break;
        }
        let a = rng.range(0.0, std::f32::consts::TAU);
        let d = rng.range(1600.0, 6500.0);
        let (cx, cz) = (a.cos() * d, a.sin() * d);
        let h = t.height(cx, cz);
        if !(4.0..90.0).contains(&h) || t.normal(cx, cz).y < 0.985 {
            continue;
        }
        villages += 1;
        let mut placed = 0;
        for _ in 0..200 {
            if placed >= 18 {
                break;
            }
            let x = cx + rng.range(-260.0, 260.0);
            let z = cz + rng.range(-260.0, 260.0);
            let h = t.height(x, z);
            if h < 3.0 || t.normal(x, z).y < 0.97 {
                continue;
            }
            let w = rng.range(7.0, 12.0);
            b.push(Building {
                pos: vec3(x, h - 0.3, z),
                size: vec3(w, rng.range(4.0, 6.5), rng.range(6.0, 9.0)),
                yaw: rng.range(0.0, 3.14),
                wall: wall_cols[rng.next_u32() as usize % wall_cols.len()],
                roof: Some(roof_cols[rng.next_u32() as usize % roof_cols.len()]),
            });
            placed += 1;
        }
    }
    b
}

fn place_clouds(rng: &mut Rng) -> Vec<CloudBlob> {
    let mut blobs = Vec::new();
    for _ in 0..55 {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let d = rng.range(1500.0, 16_000.0).sqrt() * 120.0; // bias toward the middle
        let base = vec3(a.cos() * d, rng.range(1450.0, 1900.0), a.sin() * d);
        let n = 4 + rng.next_u32() % 5;
        for _ in 0..n {
            let off = vec3(rng.range(-230.0, 230.0), rng.range(0.0, 90.0), rng.range(-230.0, 230.0));
            let r = rng.range(110.0, 210.0);
            blobs.push(CloudBlob { center: base + off, radii: vec3(r, r * rng.range(0.45, 0.65), r * rng.range(0.85, 1.15)) });
        }
    }
    blobs
}

fn place_rings(t: &Terrain) -> Vec<Ring> {
    let n = 9;
    let mut pts: Vec<Vec3> = Vec::new();
    let mut prev = RING_START;
    for k in 0..n {
        // A loop around the island, counter-clockwise starting east of the airport.
        let a = k as f32 / n as f32 * std::f32::consts::TAU + 0.15;
        let r = if k % 2 == 0 { 2300.0 } else { 3300.0 };
        let (x, z) = (a.cos() * r, -a.sin() * r);
        // Keep the ring and the straight path to it clear of the terrain.
        let mut ground: f32 = 0.0;
        for s in 0..=40 {
            let p = prev.lerp(vec3(x, 0.0, z), s as f32 / 40.0);
            ground = ground.max(t.surface(p.x, p.z));
        }
        for s in 0..16 {
            let b = s as f32 / 16.0 * std::f32::consts::TAU;
            ground = ground.max(t.surface(x + b.cos() * 300.0, z + b.sin() * 300.0));
        }
        let y = (ground + 140.0 + (k % 3) as f32 * 60.0).max(260.0);
        let p = vec3(x, y, z);
        pts.push(p);
        prev = p;
    }
    let mut rings = Vec::new();
    for k in 0..n {
        let from = if k == 0 { RING_START } else { pts[k - 1] };
        let to = if k + 1 < n { pts[k + 1] } else { pts[k] + (pts[k] - from) };
        let normal = (to - from).normalize();
        rings.push(Ring { pos: pts[k], normal, radius: 40.0 });
    }
    rings
}
