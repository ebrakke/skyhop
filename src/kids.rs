//! Kids mode, for the very young: the stick flies like a real plane's (pull back to climb)
//! but nothing can go wrong. No stalling, no crashing, and the runway brings you in to land.
//! Little missions follow one another, each told by a voice and a picture: pop ten balloons,
//! fly through the rainbows, find the windmills (follow the balloons), land. Planes, a
//! helicopter and a hot air balloon to fly, all moved kinematically rather than through the
//! flight model, so they always go where the stick points.

use crate::flight::{Aircraft, Craft, G};
use crate::render::ToyKind;
use crate::world::{smoothstep, Rng, Terrain, HALF, RWY_ELEV, RWY_HALF_LEN, SEA};
use macroquad::math::{vec3, Quat, Vec3};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Vehicle {
    Plane(Craft),
    Toy(ToyKind),
}

pub const VEHICLES: [(Vehicle, &str); 6] = [
    (Vehicle::Plane(Craft::Cessna), "Little plane"),
    (Vehicle::Toy(ToyKind::Helicopter), "Helicopter"),
    (Vehicle::Toy(ToyKind::HotAir), "Hot air balloon"),
    (Vehicle::Plane(Craft::Jet), "Big jet"),
    (Vehicle::Plane(Craft::Fighter), "Fast jet"),
    (Vehicle::Plane(Craft::Glider), "Glider"),
];

impl Vehicle {
    /// The aircraft standing in for it: its position, the camera's distance, the wheels'
    /// height. The helicopter and the balloon are drawn in its place.
    pub fn proxy(self) -> Craft {
        match self {
            Vehicle::Plane(c) => c,
            Vehicle::Toy(ToyKind::Helicopter) => Craft::Cessna,
            Vehicle::Toy(ToyKind::HotAir) => Craft::Jet,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Landmark {
    Windmills,
    Lighthouse,
    Boat,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Quest {
    Balloons,
    Rainbows,
    Find(Landmark),
    Land,
}

pub const NEED_BALLOONS: usize = 10;
/// 99 Red Balloons: how many to pop.
pub const RED_TOTAL: usize = 99;
pub const RED: [f32; 4] = [0.95, 0.1, 0.13, 1.0];
/// How high 99 Red Balloons' balloons float over the road (they're drawn smaller, so they sit on it).
pub const BALLOON_ON_ROAD: f32 = 4.5;
pub const RAINBOWS: usize = 5;
/// Balloon trails dotted about the island at any time.
const TRAILS: usize = 10;
const PER_TRAIL: usize = 6;

/// The missions go round: balloons, rainbows, find something, balloons, land.
fn quest_at(step: usize) -> Quest {
    const MARKS: [Landmark; 3] = [Landmark::Windmills, Landmark::Lighthouse, Landmark::Boat];
    match step % 5 {
        0 => Quest::Balloons,
        1 => Quest::Rainbows,
        2 => Quest::Find(MARKS[(step / 5) % 3]),
        3 => Quest::Balloons,
        _ => Quest::Land,
    }
}

pub const COLORS: [[f32; 4]; 6] = [
    [1.0, 0.25, 0.3, 1.0],
    [1.0, 0.62, 0.12, 1.0],
    [1.0, 0.9, 0.2, 1.0],
    [0.3, 0.85, 0.35, 1.0],
    [0.25, 0.6, 1.0, 1.0],
    [0.75, 0.4, 1.0, 1.0],
];

pub struct Balloon {
    pub pos: Vec3,
    pub color: [f32; 4],
    pub popped: bool,
    /// Phase of the gentle bobbing.
    pub bob: f32,
    trail: u32,
    /// Part of a trail leading to a landmark (not replaced once popped).
    crumb: bool,
    /// Which way the course runs past it (99 Red Balloons).
    dir: Vec3,
}

impl Balloon {
    pub fn at(&self, time: f32) -> Vec3 {
        self.pos + Vec3::Y * (time * 1.3 + self.bob).sin() * 1.2
    }
}

/// A rainbow ring to fly through.
pub struct Rainbow {
    pub pos: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    pub done: bool,
}

#[derive(Default)]
pub struct Input {
    /// Right is positive.
    pub x: f32,
    /// Climb is positive (the stick pulled back).
    pub y: f32,
    pub zoom: bool,
}

impl Input {
    fn any(&self) -> bool {
        self.x.abs() > 0.3 || self.y.abs() > 0.3 || self.zoom
    }
}

pub enum Event {
    /// A balloon popped: where, its colour, and its number when they're being counted.
    Pop(Vec3, [f32; 4], Option<usize>),
    /// Through a rainbow (how many so far).
    Rainbow(Vec3, usize),
    NewQuest(Quest),
    QuestDone(Quest),
    Liftoff,
    Touchdown,
    /// Landed and stopped.
    Landed,
    /// 99 Red Balloons: the end of the road. The time (with the penalties), and how many popped.
    RedDone(f32, usize),
    /// 99 Red Balloons: flew past one.
    Missed,
}

/// Seconds added for each balloon missed.
pub const MISS_PENALTY: f32 = 2.0;

/// A run at 99 Red Balloons: waiting at the start gate until the stick moves, then the clock.
pub struct RedRun {
    pub started: bool,
    pub time: f32,
    pub missed: usize,
    pub done: Option<f32>,
    /// The next balloon along the road (the ones before it are popped or passed).
    pub next: usize,
}

impl RedRun {
    /// The time with the penalties.
    pub fn total(&self) -> f32 {
        self.time + self.missed as f32 * MISS_PENALTY
    }
}

/// 99 Red Balloons' course: a loop round the airport, about 5 km: a straight down the
/// runway, sweeping turns, a dip and a climb or two. Through these points (x, z, height over
/// the runway; the plan is scaled to size), as a smooth line sampled every 10 m.
pub fn red_course(t: &Terrain) -> Vec<Vec3> {
    const PTS: [(f32, f32, f32); 15] = [
        (-1000.0, 0.0, 32.0),
        (400.0, 0.0, 55.0),
        (1050.0, 210.0, 90.0),
        (1330.0, 770.0, 70.0),
        (1050.0, 1330.0, 120.0),
        (350.0, 1540.0, 60.0),
        (-420.0, 1260.0, 140.0),
        (-910.0, 630.0, 80.0),
        (-1050.0, -140.0, 130.0),
        (-700.0, -840.0, 70.0),
        (0.0, -1120.0, 160.0),
        (770.0, -980.0, 90.0),
        (1330.0, -560.0, 130.0),
        (1450.0, 100.0, 80.0),
        (1000.0, 420.0, 45.0),
    ];
    const SIZE: f32 = 0.45;
    let p = |i: usize| {
        let (x, z, h) = PTS[i.min(PTS.len() - 1)];
        vec3(x * SIZE, RWY_ELEV + h, z * SIZE)
    };
    // Catmull-Rom through the points
    let mut path = Vec::new();
    for i in 0..PTS.len() - 1 {
        let (p0, p1, p2, p3) = (p(i.saturating_sub(1)), p(i), p(i + 1), p(i + 2));
        let steps = ((p2 - p1).length() / 10.0).ceil() as usize;
        for k in 0..steps {
            let u = k as f32 / steps as f32;
            let (u2, u3) = (u * u, u * u * u);
            path.push(0.5 * (2.0 * p1 + (p2 - p0) * u + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3));
        }
    }
    path.push(p(PTS.len() - 1));
    // even out the spacing, then keep well clear of the hills (looking 300 m either side) and smooth
    let mut even = vec![path[0]];
    let mut carry = 0.0;
    for w in path.windows(2) {
        let d = (w[1] - w[0]).length();
        let mut along = 10.0 - carry;
        while along <= d {
            even.push(w[0].lerp(w[1], along / d));
            along += 10.0;
        }
        carry = d - (along - 10.0);
    }
    let ground: Vec<f32> = even.iter().map(|q| t.surface(q.x, q.z)).collect();
    let floor: Vec<f32> = (0..even.len()).map(|i| ground[i.saturating_sub(30)..(i + 30).min(even.len())].iter().fold(f32::MIN, |a, b| a.max(*b)) + 45.0).collect();
    for q in even.iter_mut().zip(&floor) {
        q.0.y = q.0.y.max(*q.1);
    }
    for _ in 0..3 {
        let ys: Vec<f32> = even.iter().map(|q| q.y).collect();
        for i in 0..even.len() {
            let w = &ys[i.saturating_sub(12)..(i + 12).min(ys.len())];
            even[i].y = (w.iter().sum::<f32>() / w.len() as f32).max(floor[i]);
        }
    }
    even
}

pub struct Kids {
    pub vehicle: usize,
    hdg: f32,
    /// A plane's flight path angle and bank, radians.
    gamma: f32,
    bank: f32,
    /// Speed along the path (planes), or over the ground (the others).
    pub speed: f32,
    /// Climb rate of the helicopter and the balloon, m/s.
    vs: f32,
    zoom: f32,
    pub on_ground: bool,
    /// Rolling out after a landing (cheered once it stops).
    rolling_out: bool,
    /// Landed with the stick still held: let go before it takes off again.
    wait_release: bool,
    since_liftoff: f32,
    /// Been away from the runway since taking off (so it doesn't land you straight back).
    away: bool,
    /// The runway has the plane and is bringing it in.
    pub landing: bool,
    pub balloons: Vec<Balloon>,
    trail_id: u32,
    rng: Rng,
    pub quest: Quest,
    step: usize,
    /// Done towards this mission, and the colours of the balloons popped for it.
    pub progress: usize,
    pub progress_colors: Vec<[f32; 4]>,
    /// The mission is done: the next one starts after a moment to celebrate.
    pub between: Option<f32>,
    pub rainbows: Vec<Rainbow>,
    announced: bool,
    pub rainbow_smoke: bool,
    pub hue: f32,
    /// Distance flown since the last puff of rainbow smoke.
    pub smoke_acc: f32,
    /// A big message and how long it has been up.
    pub banner: Option<(String, f32)>,
    /// Seconds in kids mode.
    pub clock: f32,
    pub rotor: f32,
    pub flame: f32,
    /// Playing 99 Red Balloons instead of the missions.
    pub red: Option<RedRun>,
    /// 99 Red Balloons' road.
    pub course: Vec<Vec3>,
}

impl Kids {
    /// 99 Red Balloons: no missions, just 99 red balloons in a line along a rainbow road
    /// round the island, and a clock.
    pub fn red_balloons(terrain: &Terrain, seed: u64) -> Self {
        let mut k = Kids::new(terrain, seed);
        k.balloons.clear();
        k.announced = true;
        k.red = Some(RedRun { started: false, time: 0.0, missed: 0, done: None, next: 0 });
        k.trail_id += 1;
        let path = red_course(terrain);
        for i in 0..RED_TOTAL {
            let j = (i as f32 / (RED_TOTAL - 1) as f32 * (path.len() - 1) as f32).round() as usize;
            let dir = (path[(j + 1).min(path.len() - 1)] - path[j.saturating_sub(1)]).normalize();
            let bob = k.rng.range(0.0, 6.0);
            k.balloons.push(Balloon { pos: path[j] + Vec3::Y * BALLOON_ON_ROAD, color: RED, popped: false, bob, trail: k.trail_id, crumb: true, dir });
        }
        k.course = path;
        k
    }

    pub fn new(terrain: &Terrain, seed: u64) -> Self {
        let mut k = Kids {
            vehicle: 0,
            hdg: 90.0,
            gamma: 0.0,
            bank: 0.0,
            speed: 0.0,
            vs: 0.0,
            zoom: 0.0,
            on_ground: true,
            rolling_out: false,
            wait_release: false,
            since_liftoff: 0.0,
            away: false,
            landing: false,
            balloons: Vec::new(),
            trail_id: 0,
            rng: Rng::new(1000 + seed),
            quest: quest_at(0),
            step: 0,
            progress: 0,
            progress_colors: Vec::new(),
            between: None,
            rainbows: Vec::new(),
            announced: false,
            rainbow_smoke: true,
            hue: 0.0,
            smoke_acc: 0.0,
            banner: None,
            clock: 0.0,
            rotor: 0.0,
            flame: 0.0,
            red: None,
            course: Vec::new(),
        };
        // the first trail climbs away over the runway; the rest are dotted around
        k.add_trail(terrain, vec3(-RWY_HALF_LEN + 1100.0, RWY_ELEV + 38.0, 0.0), 0.0, PER_TRAIL, 220.0, false);
        for _ in 1..TRAILS {
            k.add_random_trail(terrain, Vec3::ZERO, 1200.0, 4500.0);
        }
        k
    }

    pub fn kind(&self) -> Vehicle {
        VEHICLES[self.vehicle].0
    }

    pub fn toy(&self) -> Option<ToyKind> {
        match self.kind() {
            Vehicle::Toy(t) => Some(t),
            _ => None,
        }
    }

    /// Lined up on runway 09, ready to go (or, for 99 Red Balloons, in the air before the start
    /// gate).
    pub fn spawn(&mut self, terrain: &Terrain) -> Aircraft {
        if self.red.is_some() && self.course.len() > 20 {
            let dir = (self.course[10] - self.course[0]).normalize();
            let mut ac = Aircraft::on_ground_at(self.kind().proxy(), terrain, 0.0, 0.0, 90.0);
            let hdg = dir.x.atan2(-dir.z).to_degrees();
            self.fly_from(&mut ac, self.course[0] - dir * 150.0, hdg);
            return ac;
        }
        self.hdg = 90.0;
        self.gamma = 0.0;
        self.bank = 0.0;
        self.speed = 0.0;
        self.vs = 0.0;
        self.on_ground = true;
        self.landing = false;
        let mut ac = Aircraft::on_ground_at(self.kind().proxy(), terrain, -RWY_HALF_LEN + 40.0, 0.0, 90.0);
        self.pose(&mut ac, &Input::default(), 0.0);
        ac
    }

    /// Swap to the next thing to fly, right where the last one was.
    pub fn next_vehicle(&mut self, ac: &Aircraft, terrain: &Terrain) -> Aircraft {
        self.vehicle = (self.vehicle + 1) % VEHICLES.len();
        let mut a = Aircraft::on_ground_at(self.kind().proxy(), terrain, ac.pos.x, ac.pos.z, self.hdg);
        if !self.on_ground {
            a.pos.y = ac.pos.y - ac.spec.rest_height + a.spec.rest_height;
        }
        self.landing = false;
        self.rolling_out = false;
        self.speed = if self.on_ground { 0.0 } else { self.cruise() };
        self.vs = 0.0;
        self.gamma = 0.0;
        self.pose(&mut a, &Input::default(), 0.0);
        a
    }

    fn cruise(&self) -> f32 {
        match self.kind() {
            Vehicle::Plane(Craft::Cessna) => 45.0,
            Vehicle::Plane(Craft::Glider) => 40.0,
            Vehicle::Plane(Craft::Jet) => 72.0,
            Vehicle::Plane(Craft::Fighter) => 85.0,
            Vehicle::Toy(ToyKind::Helicopter) => 30.0,
            Vehicle::Toy(ToyKind::HotAir) => 18.0,
        }
    }

    /// Where it touches balloons and rings (the balloon's envelope, not its basket), and how
    /// near counts.
    fn hit(&self, ac: &Aircraft) -> (Vec3, f32) {
        match self.kind() {
            Vehicle::Plane(_) => (ac.pos, 14.0 + ac.spec.span * 0.4),
            Vehicle::Toy(ToyKind::Helicopter) => (ac.pos + Vec3::Y * 1.0, 18.0),
            // (on the rainbow road the basket rides the road, and pops what it meets)
            Vehicle::Toy(ToyKind::HotAir) if self.red.is_some() => (ac.pos + Vec3::Y * 2.0, 15.0),
            Vehicle::Toy(ToyKind::HotAir) => (ac.pos + Vec3::Y * (11.6 - ac.spec.rest_height), 15.0),
        }
    }

    /// The rainbow road under `p`, if it's over the road (or just off its edge) and not far
    /// above or below it: the point on its centre line, and which way it runs.
    fn road_under(&self, p: Vec3) -> Option<(Vec3, Vec3)> {
        let n = self.course.len();
        // only the stretch around the next balloon, where the road passes close to itself
        let next = self.red.as_ref().map_or(0, |r| r.next.min(RED_TOTAL - 1));
        let j = (next as f32 / (RED_TOTAL - 1) as f32 * n.saturating_sub(1) as f32).round() as usize;
        let (lo, hi) = (j.saturating_sub(60), (j + 30).min(n));
        let (i, _) = self.course[lo..hi]
            .iter()
            .enumerate()
            .map(|(i, c)| (i + lo, c))
            .filter(|(_, c)| (c.y - p.y).abs() < 40.0)
            .map(|(i, c)| (i, (c.x - p.x).powi(2) + (c.z - p.z).powi(2)))
            .filter(|(_, d)| *d < 24.0 * 24.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let t = (self.course[(i + 1).min(n - 1)] - self.course[i.saturating_sub(1)]).normalize_or_zero();
        Some((self.course[i], t))
    }

    /// How high above the road's surface it rides: wheels, skids or basket just clear of it.
    fn ride(&self, ac: &Aircraft) -> f32 {
        ac.spec.rest_height + if self.toy() == Some(ToyKind::HotAir) { 0.8 } else { 2.0 }
    }

    /// On the rainbow road: hold the ride height over it (the stick only steers there).
    fn ride_road(&mut self, ac: &mut Aircraft, dt: f32) -> Option<f32> {
        let (rp, t) = self.road_under(ac.pos)?;
        let want = rp.y + self.ride(ac);
        ac.pos.y += (want - ac.pos.y) * (dt * 8.0).min(1.0);
        Some(t.y.clamp(-1.0, 1.0).asin())
    }

    fn add_trail(&mut self, t: &Terrain, start: Vec3, dir: f32, n: usize, spacing: f32, crumb: bool) {
        self.trail_id += 1;
        let bend = self.rng.range(-0.3, 0.3);
        let (mut p, mut dir) = (start, dir);
        let mut color = self.rng.next_u32() as usize;
        for _ in 0..n {
            let ground = t.surface(p.x, p.z);
            p.y = p.y.clamp(ground + 45.0, ground + 220.0);
            let bob = self.rng.range(0.0, 6.0);
            self.balloons.push(Balloon { pos: p, color: COLORS[color % COLORS.len()], popped: false, bob, trail: self.trail_id, crumb, dir: Vec3::ZERO });
            color += 1;
            if !crumb {
                dir += bend;
            }
            p += vec3(dir.cos(), 0.0, dir.sin()) * spacing;
            p.y += self.rng.range(-15.0, 30.0);
            let lim = HALF * 0.75;
            p.x = p.x.clamp(-lim, lim);
            p.z = p.z.clamp(-lim, lim);
        }
    }

    /// A trail somewhere between `near` and `far` metres from `around`.
    fn add_random_trail(&mut self, t: &Terrain, around: Vec3, near: f32, far: f32) {
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let r = self.rng.range(near, far);
        let lim = HALF * 0.7;
        let (x, z) = ((around.x + a.cos() * r).clamp(-lim, lim), (around.z + a.sin() * r).clamp(-lim, lim));
        let y = t.surface(x, z) + self.rng.range(60.0, 150.0);
        let dir = self.rng.range(0.0, std::f32::consts::TAU);
        self.add_trail(t, vec3(x, y, z), dir, PER_TRAIL, 220.0, false);
    }

    /// Balloons from here to the landmark, to show the way.
    fn add_crumbs(&mut self, t: &Terrain, from: Vec3, to: Vec3) {
        let d = vec3(to.x - from.x, 0.0, to.z - from.z);
        let n = ((d.length() - 600.0) / 330.0).clamp(0.0, 24.0) as usize;
        if n == 0 {
            return;
        }
        let dir = d.z.atan2(d.x);
        let start = from + d.normalize() * 350.0;
        self.add_trail(t, vec3(start.x, from.y.max(t.surface(start.x, start.z) + 60.0), start.z), dir, n, 330.0, true);
    }

    fn start_quest(&mut self, t: &Terrain, ac: &Aircraft, marks: &[Vec3; 3]) {
        self.progress = 0;
        self.progress_colors.clear();
        self.rainbows.clear();
        match self.quest {
            Quest::Rainbows => {
                // a curving line of them, straight ahead
                let fwd = |a: f32| vec3(a.cos(), 0.0, a.sin());
                let mut dir = (self.hdg - 90.0).to_radians();
                let bend = if self.rng.f() < 0.5 { -0.12 } else { 0.12 };
                let mut p = ac.pos + fwd(dir) * 700.0;
                for _ in 0..RAINBOWS {
                    let ground = t.surface(p.x, p.z);
                    let y = ac.pos.y.clamp(ground + 70.0, ground + 250.0);
                    self.rainbows.push(Rainbow { pos: vec3(p.x, y, p.z), normal: fwd(dir), radius: 30.0, done: false });
                    dir += bend;
                    p += fwd(dir) * 380.0;
                }
            }
            Quest::Find(l) => self.add_crumbs(t, ac.pos, marks[l as usize]),
            _ => {}
        }
    }

    /// Where the big arrow points (and in what colour), if anywhere.
    pub fn target(&self, from: Vec3, time: f32, marks: &[Vec3; 3]) -> Option<(Vec3, [f32; 4])> {
        if self.between.is_some() || self.landing || self.red.as_ref().is_some_and(|r| r.done.is_some()) {
            return None;
        }
        match self.quest {
            _ if self.red.is_some() => self.red.as_ref().and_then(|r| self.balloons.get(r.next)).map(|b| (b.at(time), b.color)),
            Quest::Balloons => self
                .balloons
                .iter()
                .filter(|b| !b.popped)
                .min_by(|a, b| (a.at(time) - from).length_squared().total_cmp(&(b.at(time) - from).length_squared()))
                .map(|b| (b.at(time), b.color)),
            Quest::Rainbows => self.rainbows.iter().find(|r| !r.done).map(|r| (r.pos, [1.0, 0.55, 0.15, 1.0])),
            Quest::Find(l) => Some((marks[l as usize], [1.0, 0.85, 0.2, 1.0])),
            Quest::Land => Some((self.runway_target(from), [1.0, 1.0, 1.0, 1.0])),
        }
    }

    /// Where to fly to land: the near end of the runway's approach, or the runway itself once
    /// it has the plane (the helicopter and balloon can set down anywhere, but it's a nice spot).
    pub fn runway_target(&self, from: Vec3) -> Vec3 {
        if self.landing || self.on_ground || self.toy().is_some() {
            return vec3(0.0, RWY_ELEV, 0.0);
        }
        let d = if from.x < 0.0 { -1.0 } else { 1.0 };
        vec3(d * (RWY_HALF_LEN + 1300.0), RWY_ELEV + 70.0, 0.0)
    }

    pub fn say(&mut self, s: impl Into<String>) {
        self.banner = Some((s.into(), 0.0));
    }

    /// Signed difference from the current heading to `want`, degrees.
    fn hdg_err(&self, want: f32) -> f32 {
        (want - self.hdg + 540.0).rem_euclid(360.0) - 180.0
    }

    /// The balloon or ring just ahead, if one is close enough to pull the plane onto it.
    fn magnet(&self, ac: &Aircraft, time: f32) -> Option<Vec3> {
        let (hit, _) = self.hit(ac);
        let fwd = Aircraft::heading_rot(self.hdg) * Vec3::NEG_Z;
        let ahead = |d: &Vec3, cos: f32| d.length() < 380.0 && vec3(d.x, 0.0, d.z).normalize_or_zero().dot(fwd) > cos;
        if let Some(r) = &self.red {
            return self.balloons.get(r.next).map(|b| b.at(time) - hit).filter(|d| d.length() < 220.0 && ahead(d, 0.85));
        }
        let ring = (self.quest == Quest::Rainbows).then(|| self.rainbows.iter().find(|r| !r.done)).flatten().map(|r| r.pos - hit).filter(|d| ahead(d, 0.7));
        ring.or_else(|| {
            self.balloons.iter().filter(|b| !b.popped).map(|b| b.at(time) - hit).filter(|d| ahead(d, 0.8)).min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
        })
    }

    pub fn update(&mut self, ac: &mut Aircraft, input: &Input, terrain: &Terrain, marks: &[Vec3; 3], time: f32, dt: f32) -> Vec<Event> {
        let mut ev = Vec::new();
        self.clock += dt;
        if let Some((_, age)) = &mut self.banner {
            *age += dt;
            if *age > 3.0 {
                self.banner = None;
            }
        }
        if !self.announced {
            self.announced = true;
            ev.push(Event::NewQuest(self.quest));
        }
        if self.wait_release && !input.any() {
            self.wait_release = false;
        }
        // 99 Red Balloons: hold at the start gate until the stick moves
        if let Some(r) = &mut self.red {
            if !r.started {
                if !input.any() {
                    self.pose(ac, input, dt);
                    return ev;
                }
                r.started = true;
            }
        }
        self.zoom += ((input.zoom as i32 as f32) - self.zoom) * (dt * 2.5).min(1.0);
        let (hit_before, _) = self.hit(ac);
        match self.toy() {
            None => self.fly_plane(ac, input, terrain, time, dt, &mut ev),
            Some(kind) => self.fly_toy(kind, ac, input, terrain, time, dt, &mut ev),
        }
        let (hit, reach) = self.hit(ac);

        // pop what we flew through (the whole path this frame, so fast jets don't skip any)
        let seg = hit - hit_before;
        let counting = self.quest == Quest::Balloons && self.between.is_none() && self.red.is_none();
        for b in self.balloons.iter_mut().filter(|b| !b.popped) {
            let c = b.at(time);
            let k = if seg.length_squared() > 0.0 { ((c - hit_before).dot(seg) / seg.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            if (hit_before + seg * k - c).length() < reach {
                b.popped = true;
                if self.red.is_some() {
                    self.progress += 1;
                }
                let count = (counting && self.progress < NEED_BALLOONS).then(|| {
                    self.progress += 1;
                    self.progress_colors.push(b.color);
                    self.progress
                });
                ev.push(Event::Pop(c, b.color, count));
            }
        }
        // through the rainbows: across the ring's plane, inside it (with a little leeway)
        if self.quest == Quest::Rainbows && self.between.is_none() {
            for r in self.rainbows.iter_mut().filter(|r| !r.done) {
                let (s0, s1) = ((hit_before - r.pos).dot(r.normal), (hit - r.pos).dot(r.normal));
                if (s0 <= 0.0) != (s1 <= 0.0) {
                    let p = hit_before + seg * (s0 / (s0 - s1));
                    if (p - r.pos).length() < r.radius * 1.25 {
                        r.done = true;
                        self.progress += 1;
                        ev.push(Event::Rainbow(r.pos, self.progress));
                    }
                }
            }
        }

        // 99 Red Balloons: the clock runs from takeoff to the end of the road; a balloon flown
        // past without popping it is missed, and the next one along is the one to get
        if let Some(r) = &mut self.red {
            while let Some(b) = self.balloons.get(r.next) {
                if b.popped {
                    r.next += 1;
                } else if (hit - b.pos).dot(b.dir) > 30.0 {
                    r.next += 1;
                    if r.done.is_none() {
                        r.missed += 1;
                        ev.push(Event::Missed);
                    }
                } else {
                    break;
                }
            }
            if let Some(d) = &mut r.done {
                *d += dt;
            } else if r.next >= self.balloons.len() {
                r.done = Some(0.0);
                ev.push(Event::RedDone(r.total(), self.progress));
            } else {
                r.time += dt;
            }
        } else if let Some(t) = &mut self.between {
            *t += dt;
            if *t > 4.0 {
                self.between = None;
                self.step += 1;
                self.quest = quest_at(self.step);
                self.start_quest(terrain, ac, marks);
                ev.push(Event::NewQuest(self.quest));
            }
        } else {
            let done = match self.quest {
                Quest::Balloons => self.progress >= NEED_BALLOONS,
                Quest::Rainbows => self.progress >= RAINBOWS,
                Quest::Find(l) => {
                    let m = marks[l as usize];
                    vec3(m.x - ac.pos.x, 0.0, m.z - ac.pos.z).length() < 400.0
                }
                Quest::Land => ev.iter().any(|e| matches!(e, Event::Landed)),
            };
            if done {
                if let Quest::Find(_) | Quest::Land = self.quest {
                    self.progress = 1;
                }
                ev.push(Event::QuestDone(self.quest));
                self.between = Some(0.0);
            }
        }

        // a trail all popped is replaced by a new one somewhere near
        let mut gone = Vec::new();
        for b in &self.balloons {
            if !gone.contains(&b.trail) && self.balloons.iter().filter(|o| o.trail == b.trail).all(|o| o.popped) {
                gone.push(b.trail);
            }
        }
        for id in gone {
            let crumb = self.balloons.iter().any(|b| b.trail == id && b.crumb);
            self.balloons.retain(|b| b.trail != id);
            if !crumb {
                self.add_random_trail(terrain, ac.pos, 1200.0, 3000.0);
            }
        }

        self.pose(ac, input, dt);
        ev
    }

    #[allow(clippy::too_many_arguments)]
    fn fly_plane(&mut self, ac: &mut Aircraft, input: &Input, terrain: &Terrain, time: f32, dt: f32, ev: &mut Vec<Event>) {
        let cruise = self.cruise();
        let rest = ac.spec.rest_height;
        // runway coordinates: which way it's being used, distance past the threshold, and
        // distance right of the centreline
        let dir = if self.hdg_err(90.0).abs() < 90.0 { 1.0 } else { -1.0 };
        let rwy_hdg = if dir > 0.0 { 90.0 } else { 270.0 };
        let along = ac.pos.x * dir + RWY_HALF_LEN;
        let side = ac.pos.z * dir;
        let height = ac.pos.y - rest - RWY_ELEV;

        if self.on_ground {
            let go = input.any() && !self.rolling_out && !self.wait_release;
            let on_runway = along > -50.0 && along < 2.0 * RWY_HALF_LEN + 50.0 && side.abs() < 40.0;
            if go {
                self.speed += if ac.craft == Craft::Jet { 4.0 } else { 6.0 } * (1.0 + self.zoom) * dt;
            } else {
                self.speed = (self.speed - 3.5 * dt).max(0.0);
            }
            // keep it on the centreline on the runway; steer it anywhere else
            if on_runway {
                let want = rwy_hdg - (side / 25.0).atan().to_degrees();
                self.hdg += (self.hdg_err(want)).clamp(-20.0 * dt, 20.0 * dt);
            } else {
                self.hdg += input.x * 25.0 * dt;
            }
            if self.rolling_out && self.speed < 0.3 {
                self.rolling_out = false;
                self.wait_release = true;
                ev.push(Event::Landed);
                self.say("You landed!");
            }
            if go && self.speed > cruise * 0.65 {
                self.on_ground = false;
                self.gamma = 6f32.to_radians();
                self.since_liftoff = 0.0;
                self.away = false;
                ev.push(Event::Liftoff);
            }
            let fwd = Aircraft::heading_rot(self.hdg) * Vec3::NEG_Z;
            ac.pos += fwd * self.speed * dt;
            ac.pos.y = terrain.height(ac.pos.x, ac.pos.z).max(SEA) + rest;
            self.bank = 0.0;
            return;
        }

        self.since_liftoff += dt;
        if ac.pos.x.abs() > RWY_HALF_LEN + 1200.0 || ac.pos.z.abs() > 800.0 || height > 150.0 {
            self.away = true;
        }
        // the runway takes over when lined up and coming down toward it (climb or zoom to
        // go around); when it's time to land it reaches out further
        let to_land = self.quest == Quest::Land && self.between.is_none();
        let funnel = side.abs() < 70.0 + (-along).max(0.0) * if to_land { 0.5 } else { 0.3 };
        let lined_up = self.hdg_err(rwy_hdg).abs() < if to_land { 70.0 } else { 40.0 };
        let low = height < 60.0 + (-along).max(0.0) * if to_land { 0.25 } else { 0.16 };
        let wants_up = input.y > 0.5 || input.zoom;
        // (not mid-run in 99 Red Balloons: the road ends over the runway)
        let racing = self.red.as_ref().is_some_and(|r| r.done.is_none());
        let can_land = self.away && !racing && (-2600.0..700.0).contains(&along) && funnel && lined_up && low;
        if self.landing && (wants_up || along > 2.0 * RWY_HALF_LEN - 500.0) {
            self.landing = false;
            self.away = false;
            self.since_liftoff = 0.0;
        } else if !self.landing && can_land && !wants_up {
            self.landing = true;
        }

        let (mut target_gamma, mut target_hdg_rate, target_speed);
        if self.landing {
            // fly a point on the centreline ahead, down a 3.5 degree slope, and flare
            let ahead = (self.speed * 7.0).max(300.0);
            let want = rwy_hdg + (-side).atan2(ahead).to_degrees();
            target_hdg_rate = (self.hdg_err(want) * 0.8).clamp(-15.0, 15.0);
            let slope = 3.5f32.to_radians().tan();
            let aim = 250.0;
            let path = ((aim - along).max(0.0) * slope).max(0.0);
            let mut vs = -self.speed * slope + (path - height) * 0.35;
            if height < 10.0 {
                vs = -(0.6 + height * 0.15);
            }
            target_gamma = (vs.clamp(-7.0, 2.0) / self.speed.max(10.0)).asin();
            target_speed = cruise * 0.72;
        } else {
            target_gamma = match self.road_under(ac.pos) {
                // riding the rainbow road: the path follows it up and down
                Some((_, t)) => t.y.clamp(-1.0, 1.0).asin(),
                None => input.y * 22f32.to_radians(),
            };
            // a coordinated turn from the bank (a bit quicker than real, so it's lively)
            let bank = input.x * 50f32.to_radians();
            target_hdg_rate = (1.3 * G * bank.tan() / self.speed.max(20.0)).to_degrees();
            target_speed = cruise * (1.0 + 0.8 * self.zoom) - self.gamma.sin() * 25.0;
            // a balloon or ring just ahead pulls the plane gently onto it
            if let Some(d) = self.magnet(ac, time) {
                let bearing = d.x.atan2(-d.z).to_degrees();
                let pull = if self.red.is_some() { 5.0 } else { 12.0 };
                target_hdg_rate += (self.hdg_err(bearing) * 1.5).clamp(-pull, pull);
                if self.red.is_none() {
                    let el = (d.y / vec3(d.x, 0.0, d.z).length().max(1.0)).atan();
                    let k = 1.0 - 0.5 * input.y.abs();
                    target_gamma = target_gamma * (1.0 - k) + el * k;
                }
            }
        }
        target_hdg_rate += self.homeward(ac.pos);

        self.gamma += (target_gamma - self.gamma) * (dt * 2.5).min(1.0);
        let rate = target_hdg_rate.to_radians();
        let bank = (rate * self.speed / G).atan().clamp(-1.1, 1.1);
        self.bank += (bank - self.bank) * (dt * 3.0).min(1.0);
        self.hdg = (self.hdg + target_hdg_rate * dt).rem_euclid(360.0);
        self.speed += (target_speed - self.speed) * (dt * 0.8).min(1.0);

        // never into the ground: look a couple of seconds ahead and climb over it
        let fwd_flat = Aircraft::heading_rot(self.hdg) * Vec3::NEG_Z;
        let ground = |d: f32| {
            let p = ac.pos + fwd_flat * d;
            terrain.surface(p.x, p.z)
        };
        let floor = ground(0.0).max(ground(self.speed * 1.0)).max(ground(self.speed * 2.5));
        let clearance = if self.landing { 0.0 } else if self.since_liftoff < 8.0 { 25.0 * smoothstep(0.0, 8.0, self.since_liftoff) } else { 30.0 };
        let short = floor + clearance + rest - ac.pos.y;
        if short > 0.0 && !self.landing {
            let up = (short / 30.0).clamp(0.0, 1.0) * 25f32.to_radians();
            self.gamma = self.gamma.max(up);
        }
        // and not off the top of the sky
        if ac.pos.y > 1300.0 {
            self.gamma = self.gamma.min(0.0);
        }

        let vel = Aircraft::heading_rot(self.hdg) * vec3(0.0, self.gamma.sin(), -self.gamma.cos()) * self.speed;
        ac.pos += vel * dt;
        if let Some(slope) = self.ride_road(ac, dt) {
            self.gamma += (slope - self.gamma) * (dt * 4.0).min(1.0);
        }
        let hard = if self.landing { 0.0 } else { 6.0 };
        let surface = terrain.surface(ac.pos.x, ac.pos.z) + rest;
        if self.landing && ac.pos.y <= surface {
            ac.pos.y = surface;
            self.on_ground = true;
            self.rolling_out = true;
            self.landing = false;
            self.gamma = 0.0;
            ev.push(Event::Touchdown);
        } else {
            ac.pos.y = ac.pos.y.max(surface + hard);
        }
    }

    /// The helicopter and the balloon: no wings to worry about. The stick turns and climbs
    /// directly, and pushing down near the ground sets it down wherever you are.
    #[allow(clippy::too_many_arguments)]
    fn fly_toy(&mut self, kind: ToyKind, ac: &mut Aircraft, input: &Input, terrain: &Terrain, time: f32, dt: f32, ev: &mut Vec<Event>) {
        let heli = kind == ToyKind::Helicopter;
        let cruise = self.cruise();
        let (turn, climb, sink, clearance) = if heli { (40.0, 7.0, 6.0, 15.0) } else { (22.0, 4.0, 3.5, 20.0) };
        let rest = ac.spec.rest_height;
        let ground = terrain.surface(ac.pos.x, ac.pos.z);
        let agl = ac.pos.y - rest - ground;
        if self.on_ground {
            self.speed = 0.0;
            self.vs = 0.0;
            self.flame = (self.flame - dt * 2.0).max(0.0);
            if input.any() && !self.wait_release {
                self.on_ground = false;
                self.vs = climb * 0.6;
                self.since_liftoff = 0.0;
                ev.push(Event::Liftoff);
            }
            ac.pos.y = ground + rest;
            return;
        }
        self.since_liftoff += dt;
        // straight up off the ground first, then away
        let lifting = self.since_liftoff < 3.0 && agl < 15.0;
        let mut target_speed = if lifting { 0.0 } else { cruise * (1.0 + 0.7 * self.zoom) };
        let mut hdg_rate = input.x * turn;
        let mut vs = if input.y >= 0.0 { input.y * climb } else { input.y * sink };
        if lifting {
            vs = vs.max(climb * 0.6);
        }
        if let Some(d) = self.magnet(ac, time) {
            let bearing = d.x.atan2(-d.z).to_degrees();
            let pull = if self.red.is_some() { 10.0 } else { 25.0 };
            hdg_rate += (self.hdg_err(bearing) * 1.5).clamp(-pull, pull);
            let t = vec3(d.x, 0.0, d.z).length() / self.speed.max(5.0);
            let k = 1.0 - 0.5 * input.y.abs();
            vs = vs * (1.0 - k) + (d.y / t.max(1.0)).clamp(-sink, climb) * k;
        }
        hdg_rate += self.homeward(ac.pos);
        // pushing down over land, close to the ground, sets it down gently
        let over_land = terrain.height(ac.pos.x, ac.pos.z) > SEA + 0.5;
        let setting_down = input.y < -0.3 && over_land && !lifting && agl < 30.0;
        let fwd = Aircraft::heading_rot(self.hdg) * Vec3::NEG_Z;
        let ahead = |d: f32| {
            let p = ac.pos + fwd * d;
            terrain.surface(p.x, p.z)
        };
        if !setting_down {
            let floor = ground.max(ahead(self.speed * 1.5)).max(ahead(self.speed * 3.0)) + clearance;
            let short = floor - (ac.pos.y - rest);
            if short > 0.0 {
                vs = vs.max((short * 0.8).min(climb * 1.3));
            }
        } else if agl < 10.0 {
            vs = vs.max(-(0.7 + agl * 0.3));
            target_speed *= agl / 10.0;
        }
        if ac.pos.y > 1300.0 {
            vs = vs.min(0.0);
        }
        self.vs += (vs - self.vs) * (dt * 2.0).min(1.0);
        self.speed += (target_speed - self.speed) * (dt * if heli { 1.0 } else { 0.5 }).min(1.0);
        self.hdg = (self.hdg + hdg_rate * dt).rem_euclid(360.0);
        // the helicopter leans into its turns; the balloon just turns
        let bank = if heli { (input.x * 18f32).to_radians() } else { 0.0 };
        self.bank += (bank - self.bank) * (dt * 3.0).min(1.0);
        self.flame = if !heli && (vs > 0.5 || lifting) { 1.0 } else { (self.flame - dt * 2.0).max(0.0) };

        ac.pos += fwd * self.speed * dt + Vec3::Y * self.vs * dt;
        if self.ride_road(ac, dt).is_some() {
            self.vs *= (1.0 - dt * 8.0).max(0.0);
        }
        let surface = terrain.surface(ac.pos.x, ac.pos.z) + rest;
        if setting_down && ac.pos.y <= surface + 0.05 {
            ac.pos.y = surface;
            self.on_ground = true;
            self.wait_release = true;
            self.speed = 0.0;
            self.vs = 0.0;
            ev.push(Event::Touchdown);
            ev.push(Event::Landed);
            self.say("You landed!");
        } else {
            ac.pos.y = ac.pos.y.max(surface + if setting_down || lifting { 0.0 } else { 4.0 });
        }
    }

    /// Wander off the edge of the island and it turns for home (degrees per second to add).
    fn homeward(&self, pos: Vec3) -> f32 {
        let r = vec3(pos.x, 0.0, pos.z).length();
        if r <= HALF * 0.8 {
            return 0.0;
        }
        let home = (-pos.x).atan2(pos.z).to_degrees();
        (self.hdg_err(home) * 0.5).clamp(-15.0, 15.0) * smoothstep(HALF * 0.8, HALF * 0.9, r)
    }

    /// Already flying from `pos`, heading `hdg` at cruise speed.
    pub fn fly_from(&mut self, ac: &mut Aircraft, pos: Vec3, hdg: f32) {
        self.on_ground = false;
        self.since_liftoff = 30.0;
        self.away = true;
        self.hdg = hdg;
        self.gamma = 0.0;
        self.vs = 0.0;
        self.speed = self.cruise();
        ac.pos = pos;
        ac.gear = 0.0;
        self.pose(ac, &Input::default(), 0.0);
    }

    /// Skip ahead to mission `step` (for screenshots and tests).
    pub fn jump_to(&mut self, step: usize, t: &Terrain, ac: &Aircraft, marks: &[Vec3; 3]) {
        self.step = step;
        self.quest = quest_at(step);
        self.between = None;
        self.announced = false;
        self.start_quest(t, ac, marks);
    }

    /// Put the aircraft where the kinematic one is, with its engine, surfaces and gear to match.
    fn pose(&mut self, ac: &mut Aircraft, input: &Input, dt: f32) {
        let spec = ac.spec;
        let busy = !self.on_ground || self.speed > 0.5 || input.any();
        ac.ias = self.speed;
        ac.on_ground = self.on_ground;
        ac.airborne_time = if self.on_ground { 0.0 } else { 10.0 };
        ac.spec_force = vec3(0.0, G, 0.0);
        ac.omega = Vec3::ZERO;
        ac.crashed = None;
        ac.touchdown_event = false;
        let hdg = Aircraft::heading_rot(self.hdg);
        if let Some(kind) = self.toy() {
            let lean = if kind == ToyKind::Helicopter { -(self.speed / self.cruise()) * 9f32.to_radians() } else { (self.clock * 0.9).sin() * 0.025 };
            ac.rot = hdg * Quat::from_rotation_x(lean) * Quat::from_rotation_z(-self.bank);
            ac.vel = hdg * Vec3::NEG_Z * self.speed + Vec3::Y * self.vs;
            ac.ctl.throttle = 0.0;
            ac.gear = 1.0;
            let rate = if kind == ToyKind::Helicopter { if busy { 5.5 } else { 1.5 } } else { 0.0 };
            self.rotor = (self.rotor + rate * std::f32::consts::TAU * dt) % std::f32::consts::TAU;
            return;
        }
        let nose = if self.on_ground { 0.0 } else { 3f32.to_radians() + if self.landing { 2f32.to_radians() } else { 0.0 } };
        ac.rot = hdg * Quat::from_rotation_x(self.gamma + nose) * Quat::from_rotation_z(-self.bank);
        ac.vel = hdg * vec3(0.0, self.gamma.sin(), -self.gamma.cos()) * self.speed;
        let power = if !busy { 0.05 } else if self.rolling_out || self.landing { 0.3 } else { 0.6 + 0.4 * self.zoom };
        ac.ctl.throttle = power;
        if let crate::flight::Engine::Jet { idle, .. } = spec.engine {
            ac.n1 += (idle + (1.0 - idle) * power - ac.n1) * (dt * 1.5).min(1.0);
        }
        ac.burner = if ac.craft == Craft::Fighter { self.zoom } else { 0.0 };
        let rpm = 700.0 + 1900.0 * power + self.speed * 6.0;
        ac.prop_angle = (ac.prop_angle + rpm / 60.0 * std::f32::consts::TAU * dt) % std::f32::consts::TAU;
        ac.ail = -input.x * 0.25;
        ac.elev = input.y * 0.2;
        ac.rud = 0.0;
        // gear and flaps come down to land (and stay down until the plane is well clear)
        let low = self.landing || self.on_ground || self.since_liftoff < 6.0;
        if spec.retractable {
            ac.ctl.gear_up = !low;
            ac.gear = (ac.gear + if low { dt } else { -dt } * 0.5).clamp(0.0, 1.0);
        }
        let flaps = if low && spec.flaps.len() > 1 { spec.flaps[spec.flaps.len() / 2] } else { 0.0 };
        ac.flap_deg += (flaps - ac.flap_deg).clamp(-10.0 * dt, 10.0 * dt);
        ac.reverser = if self.rolling_out && self.speed > 15.0 && ac.craft == Craft::Jet { (ac.reverser + dt).min(1.0) } else { (ac.reverser - dt).max(0.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    fn marks(w: &World) -> [Vec3; 3] {
        let t = &w.terrain;
        let l = crate::world::LIGHTHOUSE;
        [w.turbines[3] + Vec3::Y * 80.0, vec3(l.0, t.height(l.0, l.1) + 30.0, l.1), vec3(1500.0, 0.0, 8400.0)]
    }

    /// Fly with `input` (from the state and the aircraft) for `secs`, never under the ground.
    fn fly(k: &mut Kids, ac: &mut Aircraft, world: &World, secs: f32, input: impl Fn(&Kids, &Aircraft) -> Input) -> Vec<Event> {
        let dt = 1.0 / 60.0;
        let m = marks(world);
        let mut ev = Vec::new();
        let mut t = 0.0;
        while t < secs {
            let i = input(k, ac);
            ev.extend(k.update(ac, &i, &world.terrain, &m, t, dt));
            let ground = world.terrain.surface(ac.pos.x, ac.pos.z);
            assert!(ac.pos.y >= ground, "under the ground at {:?}", ac.pos);
            t += dt;
        }
        ev
    }

    fn count(ev: &[Event], f: impl Fn(&Event) -> bool) -> usize {
        ev.iter().filter(|e| f(e)).count()
    }

    /// Pull back to take off, pop the first trail, then come back round and the runway lands you.
    #[test]
    fn takes_off_pops_and_lands() {
        let world = World::generate();
        let mut k = Kids::new(&world.terrain, 0);
        let mut ac = k.spawn(&world.terrain);
        let ev = fly(&mut k, &mut ac, &world, 60.0, |k, _| Input { y: if k.on_ground { 1.0 } else { 0.0 }, ..Default::default() });
        assert!(!k.on_ground, "never took off");
        let pops = count(&ev, |e| matches!(e, Event::Pop(_, _, Some(_))));
        assert!(pops >= 4, "popped {pops} of the first trail");
        // dive straight at a mountain: the plane climbs over it
        fly(&mut k, &mut ac, &world, 120.0, |_, _| Input { x: 0.4, y: -1.0, ..Default::default() });
        // fly back to runway 09's approach and let go of the stick
        k.fly_from(&mut ac, vec3(-RWY_HALF_LEN - 2000.0, RWY_ELEV + 120.0, 80.0), 90.0);
        let ev = fly(&mut k, &mut ac, &world, 120.0, |_, _| Input::default());
        assert!(count(&ev, |e| matches!(e, Event::Touchdown)) > 0, "never touched down, ended at {:?}", ac.pos);
        assert!(count(&ev, |e| matches!(e, Event::Landed)) > 0, "never stopped");
        assert!(ac.pos.x.abs() < RWY_HALF_LEN && ac.pos.z.abs() < 15.0, "stopped off the runway at {:?}", ac.pos);
    }

    /// Lined up a mile out on runway 09: the runway takes over.
    #[test]
    fn captures_the_approach() {
        let world = World::generate();
        let mut k = Kids::new(&world.terrain, 0);
        let mut ac = k.spawn(&world.terrain);
        k.fly_from(&mut ac, vec3(-RWY_HALF_LEN - 1500.0, RWY_ELEV + 120.0, 25.0), 90.0);
        fly(&mut k, &mut ac, &world, 2.0, |_, _| Input::default());
        assert!(k.landing);
    }

    #[test]
    fn every_plane_lands() {
        let world = World::generate();
        for v in 0..VEHICLES.len() {
            let mut k = Kids::new(&world.terrain, 3);
            k.vehicle = v;
            if k.toy().is_some() {
                continue;
            }
            let mut ac = k.spawn(&world.terrain);
            k.fly_from(&mut ac, vec3(RWY_HALF_LEN + 2200.0, RWY_ELEV + 140.0, -60.0), 270.0);
            let ev = fly(&mut k, &mut ac, &world, 150.0, |_, _| Input::default());
            assert!(count(&ev, |e| matches!(e, Event::Landed)) > 0, "{}: ended at {:?}", VEHICLES[v].1, ac.pos);
        }
    }

    /// The helicopter and the balloon go straight up, fly, and set down where you push down.
    #[test]
    fn toys_take_off_and_set_down() {
        let world = World::generate();
        for v in [1, 2] {
            let mut k = Kids::new(&world.terrain, 5);
            k.vehicle = v;
            let mut ac = k.spawn(&world.terrain);
            let start = ac.pos;
            let ev = fly(&mut k, &mut ac, &world, 20.0, |k, _| Input { y: if k.since_liftoff < 6.0 { 1.0 } else { 0.0 }, ..Default::default() });
            assert_eq!(count(&ev, |e| matches!(e, Event::Liftoff)), 1);
            assert!(ac.pos.y > start.y + 15.0 && (ac.pos - start).length() > 150.0, "{}: at {:?}", VEHICLES[v].1, ac.pos);
            // over the runway again and push down
            k.fly_from(&mut ac, vec3(0.0, RWY_ELEV + 40.0, 0.0), 90.0);
            let ev = fly(&mut k, &mut ac, &world, 30.0, |_, _| Input { y: -1.0, ..Default::default() });
            assert_eq!(count(&ev, |e| matches!(e, Event::Landed)), 1, "{} never set down: {:?}", VEHICLES[v].1, ac.pos);
            assert!(k.on_ground);
        }
    }

    /// The missions follow on, and each can be done by following the arrow.
    #[test]
    fn missions_follow_on() {
        let world = World::generate();
        let m = marks(&world);
        let mut k = Kids::new(&world.terrain, 1);
        let mut ac = k.spawn(&world.terrain);
        // rainbows straight ahead: hands off and the plane is drawn through them
        k.fly_from(&mut ac, vec3(0.0, 120.0, -2000.0), 90.0);
        k.jump_to(1, &world.terrain, &ac, &m);
        let ev = fly(&mut k, &mut ac, &world, 60.0, |_, _| Input::default());
        assert_eq!(count(&ev, |e| matches!(e, Event::Rainbow(..))), RAINBOWS);
        assert_eq!(count(&ev, |e| matches!(e, Event::QuestDone(Quest::Rainbows))), 1);
        assert_eq!(count(&ev, |e| matches!(e, Event::NewQuest(Quest::Find(Landmark::Windmills)))), 1);
        assert!(k.balloons.iter().any(|b| b.crumb && !b.popped), "no trail to the windmills");
        // steer for the arrow: find the windmills, pop ten (counted), and land
        let steer = |k: &Kids, ac: &Aircraft| {
            let (p, _) = k.target(ac.pos, 0.0, &m).unwrap_or((ac.pos, [0.0; 4]));
            let want = (p.x - ac.pos.x).atan2(-(p.z - ac.pos.z)).to_degrees();
            Input { x: (k.hdg_err(want) / 30.0).clamp(-1.0, 1.0), ..Default::default() }
        };
        let ev = fly(&mut k, &mut ac, &world, 400.0, steer);
        let done: Vec<Quest> = ev.iter().filter_map(|e| if let Event::QuestDone(q) = e { Some(*q) } else { None }).collect();
        assert_eq!(done[..3], [Quest::Find(Landmark::Windmills), Quest::Balloons, Quest::Land], "ended at {:?}", ac.pos);
        let counted: Vec<usize> = ev.iter().filter_map(|e| if let Event::Pop(_, _, Some(n)) = e { Some(*n) } else { None }).collect();
        assert_eq!(counted[..NEED_BALLOONS], (1..=NEED_BALLOONS).collect::<Vec<_>>());
    }

    /// Steer for the arrow (gently, like a person would), zooming on the straights if `zoom`.
    fn racer(m: [Vec3; 3], zoom: bool) -> impl Fn(&Kids, &Aircraft) -> Input {
        move |k: &Kids, ac: &Aircraft| {
            if k.red.as_ref().is_some_and(|r| !r.started) {
                return Input { y: 0.5, ..Default::default() };
            }
            let Some((p, _)) = k.target(ac.pos, 0.0, &m) else { return Input::default() };
            let want = (p.x - ac.pos.x).atan2(-(p.z - ac.pos.z)).to_degrees();
            let err = k.hdg_err(want);
            // a few balloons on, to see the next turn coming
            let next = k.red.as_ref().map_or(0, |r| r.next);
            let ahead = k.balloons.get(next + 4).map_or(0.0, |b| {
                let w = (b.pos.x - ac.pos.x).atan2(-(b.pos.z - ac.pos.z)).to_degrees();
                k.hdg_err(w).abs()
            });
            Input { x: (err / 25.0).clamp(-1.0, 1.0), y: ((p.y - ac.pos.y) / 40.0).clamp(-1.0, 1.0), zoom: zoom && err.abs() < 6.0 && ahead < 12.0 }
        }
    }

    /// 99 red balloons in a line on the road; the plane waits at the gate until the stick moves;
    /// a steady lap takes under two minutes, and zooming on the straights is quicker.
    #[test]
    fn ninety_nine_red_balloons() {
        let world = World::generate();
        let m = marks(&world);
        let mut k = Kids::red_balloons(&world.terrain, 7);
        assert_eq!(k.balloons.len(), RED_TOTAL);
        assert!(k.balloons.iter().all(|b| b.color == RED));
        for w in k.balloons.windows(2) {
            let d = (w[1].pos - w[0].pos).length();
            assert!((30.0..120.0).contains(&d), "balloons {d:.0} m apart");
        }
        for b in &k.balloons {
            assert!(b.pos.y > world.terrain.surface(b.pos.x, b.pos.z) + 35.0, "balloon in the hills at {:?}", b.pos);
        }
        let mut runs = Vec::new();
        for zoom in [false, true] {
            let mut k = Kids::red_balloons(&world.terrain, 7);
            let mut ac = k.spawn(&world.terrain);
            let start = ac.pos;
            fly(&mut k, &mut ac, &world, 2.0, |_, _| Input::default());
            assert!(!k.red.as_ref().unwrap().started && ac.pos == start, "moved before the stick did");
            let race = racer(m, zoom);
            let ev = fly(&mut k, &mut ac, &world, 200.0, |k, ac| {
                // riding the road: on top of it, never through it
                if let (Some((rp, _)), true) = (k.road_under(ac.pos), k.red.as_ref().is_some_and(|r| r.time > 3.0 && r.done.is_none())) {
                    let over = ac.pos.y - ac.spec.rest_height - rp.y;
                    assert!(over > 0.5 && over < 4.0, "{over:.1} m over the road at {:?}", ac.pos);
                }
                race(k, ac)
            });
            let done: Vec<(f32, usize)> = ev.iter().filter_map(|e| if let Event::RedDone(t, n) = e { Some((*t, *n)) } else { None }).collect();
            assert_eq!(done.len(), 1, "never got to the end, at {:?} with {} popped", ac.pos, k.progress);
            let r = k.red.as_ref().unwrap();
            println!("zoom {zoom}: {} of 99, {} missed, {:.1} s + {:.0} s = {:.1} s", done[0].1, r.missed, r.time, r.missed as f32 * MISS_PENALTY, done[0].0);
            assert_eq!(done[0].1 + r.missed, RED_TOTAL);
            assert!(k.target(ac.pos, 0.0, &m).is_none());
            runs.push(done[0]);
        }
        assert!(runs[0].0 < 120.0, "a steady lap took {:.0} s", runs[0].0);
        assert!(runs[0].1 >= 95, "only {} popped flying steadily", runs[0].1);
        assert!(runs[1].0 < runs[0].0 - 10.0, "zooming wasn't any quicker: {:.0} vs {:.0} s", runs[1].0, runs[0].0);
    }
}
