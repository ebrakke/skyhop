//! Rigid-body flight model, parameterised by an aircraft `Spec`: a small high-wing trainer
//! (roughly a Cessna 172) and a narrow-body airliner (roughly a Boeing 737 MAX 8).
//!
//! Body axes: +x right wing, +y up, +z toward the tail (nose points along -z).
//! `omega` is the body angular velocity in those axes, so pitch-up is +x,
//! yaw-right is -y and roll-right is -z.

use crate::world::{smoothstep, Rng, Terrain, SEA};
use macroquad::math::{vec3, Quat, Vec3};

pub const G: f32 = 9.81;
pub const KT: f32 = 0.514_444; // m/s per knot
pub const FT: f32 = 0.3048; // m per foot

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum Craft {
    #[default]
    Cessna,
    Jet,
    Fighter,
    Glider,
}

impl Craft {
    pub fn spec(self) -> &'static Spec {
        match self {
            Craft::Cessna => &CESSNA,
            Craft::Jet => &JET,
            Craft::Fighter => &FIGHTER,
            Craft::Glider => &GLIDER,
        }
    }
}

pub enum Engine {
    /// Piston and propeller: shaft power (W), prop wash over the tail, P-factor.
    Piston { power: f32 },
    /// Turbofans: static thrust (N, all engines), idle N1 (fraction), reverse efficiency,
    /// and extra thrust from the afterburner at full throttle.
    Jet { thrust: f32, idle: f32, reverse: f32, afterburner: f32 },
    /// No engine at all (a sailplane).
    None,
}

/// Airspeed indicator markings, knots.
pub struct Asi {
    pub max: f32,
    pub white: (f32, f32),
    pub green: (f32, f32),
    pub amber: (f32, f32),
    pub red: f32,
}

/// Everything that makes one aircraft type fly the way it does.
pub struct Spec {
    pub mass: f32,
    pub inertia: Vec3, // pitch, yaw, roll
    pub s: f32,        // wing area m^2
    pub span: f32,
    pub chord: f32,
    pub cl0: f32,
    pub cla: f32, // per radian
    pub cd0: f32,
    pub oswald: f32,
    pub cm0: f32,
    pub cma: f32,
    pub cmq: f32,
    pub cmde: f32,
    pub cm_flaps: f32,
    /// Lateral/directional derivatives: (Cl_beta, Cl_p, Cl_r, Cl_da), (Cn_beta, Cn_r, Cn_p, Cn_da, Cn_dr), Cy_beta.
    pub roll: (f32, f32, f32, f32),
    pub yaw: (f32, f32, f32, f32, f32),
    pub cy_beta: f32,
    /// Clean stall angle (deg), change at full flaps, gain when leading-edge slats are out.
    pub stall_deg: f32,
    pub stall_flap_deg: f32,
    pub slat_deg: f32,
    /// Lift and drag added by the flaps: k * fraction^exp.
    pub flap_cl: (f32, f32),
    pub flap_cd: (f32, f32),
    /// Flap lever detents in degrees, and how fast the flaps travel (deg/s).
    pub flaps: &'static [f32],
    pub flap_rate: f32,
    pub elev_max: f32,
    pub ail_max: f32,
    pub rud_max: f32,
    pub engine: Engine,
    /// Thrust line below the centre of gravity (m): power pitches the nose up.
    pub thrust_arm: f32,
    pub retractable: bool,
    pub gear_cd: f32,
    pub spoilers: bool,
    points: &'static [(Vec3, Pt)],
    /// Spring and damper for the main and nose legs, and for the tail bumper.
    pub gear_main: (f32, f32),
    pub gear_nose: (f32, f32),
    pub tail_bumper: (f32, f32),
    /// Nose-wheel steering angle (rad) and the speed (m/s) over which it fades out.
    pub steer: (f32, f32),
    pub brake_mu: f32,
    /// Centre of gravity above the ground when parked, and the wing's height relative to it.
    pub rest_height: f32,
    pub wing_y: f32,
    /// Below this the wing isn't really flying (stall warnings off, no damper), knots.
    pub fly_kt: f32,
    pub vne_kt: f32,
    pub break_kt: f32,
    pub g_limits: (f32, f32),
    /// Sink rate (m/s) that collapses the gear, and that turns a tail scrape into a crash.
    pub gear_collapse: f32,
    pub tail_crash: f32,
    /// Elevator (rad) per m/s^2 of airspeed change. Real aircraft porpoise for a minute or more
    /// after a trim change (the phugoid); feeding speed changes back into the elevator while the
    /// stick is centred quietly damps that, so the plane settles on its trimmed speed in a cycle.
    pub speed_damper: f32,
    pub front_strike: &'static str,
    pub asi: Asi,
    /// Pilot's eye in body axes, how far the seat looks down (deg), and the camera scale.
    pub eye: Vec3,
    pub look_down: f32,
    pub cam_scale: f32,
    /// Fan shaft frequency at full power, Hz (the turbofan whine).
    pub fan_hz: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Pt {
    Nose,
    Main,
    Tail,
    Wingtip,
    /// The very front: propeller or nose cone.
    Front,
    /// Built to touch: a sailplane's tail and wingtip skids.
    Skid,
    Nacelle,
    Belly,
    Top,
}

pub static CESSNA: Spec = Spec {
    mass: 1000.0,
    inertia: Vec3::new(1350.0, 1970.0, 950.0),
    s: 16.2,
    span: 11.0,
    chord: 1.5,
    cl0: 0.25,
    cla: 4.6,
    cd0: 0.032,
    oswald: 0.75,
    cm0: 0.04,
    cma: -1.8,
    cmq: -12.0,
    cmde: 1.2,
    cm_flaps: -0.05,
    roll: (-0.09, -0.47, 0.1, 0.15),
    yaw: (0.09, -0.1, -0.02, -0.008, 0.07),
    cy_beta: -0.5,
    stall_deg: 16.0,
    stall_flap_deg: -3.0,
    slat_deg: 0.0,
    flap_cl: (0.6, 1.0),
    flap_cd: (0.05, 1.0),
    flaps: &[0.0, 10.0, 20.0, 30.0],
    flap_rate: 6.0,
    elev_max: 0.42,
    ail_max: 0.35,
    rud_max: 0.40,
    engine: Engine::Piston { power: 80_000.0 },
    thrust_arm: 0.0,
    retractable: false,
    gear_cd: 0.0,
    spoilers: false,
    points: &[
        (Vec3::new(0.0, -1.40, -2.0), Pt::Nose),
        (Vec3::new(-1.25, -1.45, 0.35), Pt::Main),
        (Vec3::new(1.25, -1.45, 0.35), Pt::Main),
        (Vec3::new(0.0, 0.05, 4.6), Pt::Tail),
        (Vec3::new(-5.55, 1.0, -0.3), Pt::Wingtip),
        (Vec3::new(5.55, 1.0, -0.3), Pt::Wingtip),
        (Vec3::new(0.0, -1.0, -2.75), Pt::Front),
        (Vec3::new(0.0, -0.62, -1.0), Pt::Belly),
        (Vec3::new(0.0, -0.55, 1.2), Pt::Belly),
        (Vec3::new(0.0, 1.08, 0.0), Pt::Top),
        (Vec3::new(0.0, 1.75, 4.2), Pt::Top),
    ],
    gear_main: (60_000.0, 5_000.0),
    gear_nose: (45_000.0, 4_000.0),
    tail_bumper: (80_000.0, 6_000.0),
    steer: (0.4, 35.0),
    brake_mu: 0.7,
    rest_height: 1.38,
    wing_y: 1.0,
    fly_kt: 20.0,
    vne_kt: 163.0,
    break_kt: 200.0,
    g_limits: (-3.5, 6.0),
    gear_collapse: 3.8,
    tail_crash: 2.0,
    speed_damper: 0.025,
    front_strike: "Prop strike! The nose hit the ground - land on the main wheels first.",
    asi: Asi { max: 190.0, white: (33.0, 85.0), green: (44.0, 129.0), amber: (129.0, 163.0), red: 163.0 },
    eye: Vec3::new(-0.28, 0.68, 0.0),
    look_down: 6.0,
    cam_scale: 1.0,
    fan_hz: 0.0,
};

/// Roughly a Boeing 737 MAX 8 at a typical landing weight: 62 t, 127 m^2 of swept wing,
/// two 130 kN turbofans slung below and ahead of the wing.
pub static JET: Spec = Spec {
    mass: 62_000.0,
    inertia: Vec3::new(3.2e6, 4.6e6, 2.0e6),
    s: 127.0,
    span: 35.9,
    chord: 3.96,
    cl0: 0.25,
    cla: 5.0,
    cd0: 0.021,
    oswald: 0.8,
    cm0: 0.12,
    cma: -1.9,
    cmq: -22.0,
    cmde: 1.1,
    cm_flaps: -0.12,
    roll: (-0.12, -0.5, 0.1, 0.09),
    yaw: (0.12, -0.12, -0.02, -0.005, 0.07),
    cy_beta: -0.6,
    stall_deg: 12.5,
    stall_flap_deg: -0.5,
    slat_deg: 3.5,
    flap_cl: (0.87, 0.7),
    flap_cd: (0.11, 1.5),
    flaps: &[0.0, 1.0, 2.0, 5.0, 10.0, 15.0, 25.0, 30.0, 40.0],
    flap_rate: 2.5,
    elev_max: 0.35,
    ail_max: 0.35,
    rud_max: 0.40,
    engine: Engine::Jet { thrust: 260_000.0, idle: 0.22, reverse: 0.45, afterburner: 0.0 },
    thrust_arm: 1.0,
    retractable: true,
    gear_cd: 0.018,
    spoilers: true,
    points: &[
        (Vec3::new(0.0, -3.0, -14.6), Pt::Nose),
        (Vec3::new(-2.86, -3.0, 1.0), Pt::Main),
        (Vec3::new(2.86, -3.0, 1.0), Pt::Main),
        (Vec3::new(0.0, -0.6, 14.5), Pt::Tail),
        (Vec3::new(-17.9, 0.3, 5.0), Pt::Wingtip),
        (Vec3::new(17.9, 0.3, 5.0), Pt::Wingtip),
        (Vec3::new(-4.9, -2.5, -4.5), Pt::Nacelle),
        (Vec3::new(4.9, -2.5, -4.5), Pt::Nacelle),
        (Vec3::new(0.0, -1.3, -18.6), Pt::Front),
        (Vec3::new(0.0, -1.9, -8.0), Pt::Belly),
        (Vec3::new(0.0, -1.9, 4.0), Pt::Belly),
        (Vec3::new(0.0, 1.9, 0.0), Pt::Top),
        (Vec3::new(0.0, 9.5, 17.0), Pt::Top),
    ],
    gear_main: (2.5e6, 2.7e5),
    gear_nose: (0.8e6, 0.9e5),
    tail_bumper: (5.0e6, 4.0e5),
    steer: (0.5, 30.0),
    brake_mu: 0.4,
    rest_height: 2.89,
    wing_y: -1.3,
    fly_kt: 60.0,
    vne_kt: 340.0,
    break_kt: 400.0,
    g_limits: (-1.5, 3.8),
    gear_collapse: 4.4,
    tail_crash: 3.0,
    speed_damper: 0.04,
    front_strike: "The nose hit the runway - flare to land on the main gear first.",
    asi: Asi { max: 360.0, white: (110.0, 230.0), green: (125.0, 300.0), amber: (300.0, 340.0), red: 340.0 },
    eye: Vec3::new(-0.55, 1.0, -16.2),
    look_down: 7.0,
    cam_scale: 3.6,
    fan_hz: 83.0,
};

/// Roughly an F-16C: 11 t with some fuel, a cranked delta wing with leading-edge extensions,
/// one engine with afterburner, relaxed stability and a very fast roll.
pub static FIGHTER: Spec = Spec {
    mass: 11_000.0,
    inertia: Vec3::new(75_673.0, 85_552.0, 12_875.0),
    s: 27.87,
    span: 9.96,
    chord: 3.45,
    cl0: 0.3,
    cla: 3.9,
    cd0: 0.021,
    oswald: 0.78,
    cm0: 0.0,
    cma: -0.55,
    cmq: -5.5,
    cmde: 0.75,
    cm_flaps: 0.0,
    roll: (-0.06, -0.3, 0.05, 0.08),
    yaw: (0.10, -0.25, -0.02, 0.0, 0.06),
    cy_beta: -0.8,
    stall_deg: 24.0,
    stall_flap_deg: 0.0,
    slat_deg: 0.0,
    flap_cl: (0.1, 1.0),
    flap_cd: (0.03, 1.0),
    flaps: &[0.0, 20.0],
    flap_rate: 15.0,
    elev_max: 0.44,
    ail_max: 0.38,
    rud_max: 0.5,
    engine: Engine::Jet { thrust: 76_000.0, idle: 0.3, reverse: 0.0, afterburner: 53_000.0 },
    thrust_arm: 0.0,
    retractable: true,
    gear_cd: 0.02,
    spoilers: false,
    points: &[
        (Vec3::new(0.0, -1.75, -4.1), Pt::Nose),
        (Vec3::new(-1.18, -1.75, 0.8), Pt::Main),
        (Vec3::new(1.18, -1.75, 0.8), Pt::Main),
        (Vec3::new(0.0, -0.25, 6.5), Pt::Tail),
        (Vec3::new(-4.98, -0.05, 2.0), Pt::Wingtip),
        (Vec3::new(4.98, -0.05, 2.0), Pt::Wingtip),
        (Vec3::new(0.0, -0.4, -8.9), Pt::Front),
        (Vec3::new(0.0, -1.0, -2.5), Pt::Belly),
        (Vec3::new(0.0, -0.85, 3.0), Pt::Belly),
        (Vec3::new(0.0, 0.95, -3.4), Pt::Top),
        (Vec3::new(0.0, 3.2, 5.8), Pt::Top),
    ],
    gear_main: (400_000.0, 44_000.0),
    gear_nose: (200_000.0, 22_000.0),
    tail_bumper: (600_000.0, 60_000.0),
    steer: (0.6, 25.0),
    brake_mu: 0.5,
    rest_height: 1.63,
    wing_y: -0.2,
    fly_kt: 90.0,
    vne_kt: 800.0,
    break_kt: 900.0,
    g_limits: (-3.5, 10.5),
    gear_collapse: 5.0,
    tail_crash: 3.0,
    speed_damper: 0.012,
    front_strike: "The nose hit the runway - land at about 11 degrees nose-up and keep it there.",
    asi: Asi { max: 800.0, white: (120.0, 300.0), green: (150.0, 700.0), amber: (700.0, 750.0), red: 800.0 },
    eye: Vec3::new(0.0, 0.72, -3.7),
    look_down: 3.0,
    cam_scale: 1.5,
    fan_hz: 125.0,
};

/// Roughly an ASK 21 two-seat sailplane: long slender wings, glide ratio about 33,
/// airbrakes, and a monowheel - it settles onto a wingtip when it stops.
pub static GLIDER: Spec = Spec {
    mass: 520.0,
    inertia: Vec3::new(900.0, 2400.0, 1700.0),
    s: 17.95,
    span: 17.0,
    chord: 1.1,
    cl0: 0.3,
    cla: 5.2,
    cd0: 0.0085,
    oswald: 0.85,
    cm0: 0.02,
    cma: -1.2,
    cmq: -14.0,
    cmde: 1.0,
    cm_flaps: 0.0,
    roll: (-0.08, -0.6, 0.15, 0.12),
    yaw: (0.07, -0.08, -0.04, -0.02, 0.06),
    cy_beta: -0.4,
    stall_deg: 13.0,
    stall_flap_deg: 0.0,
    slat_deg: 0.0,
    flap_cl: (0.0, 1.0),
    flap_cd: (0.0, 1.0),
    flaps: &[0.0],
    flap_rate: 5.0,
    elev_max: 0.4,
    ail_max: 0.35,
    rud_max: 0.45,
    engine: Engine::None,
    thrust_arm: 0.0,
    retractable: false,
    gear_cd: 0.002,
    spoilers: true,
    points: &[
        (Vec3::new(0.0, -0.8, -1.8), Pt::Nose),
        (Vec3::new(0.0, -0.9, 0.3), Pt::Main),
        (Vec3::new(0.0, -0.3, 5.9), Pt::Skid),
        (Vec3::new(-8.5, 0.55, 0.6), Pt::Skid),
        (Vec3::new(8.5, 0.55, 0.6), Pt::Skid),
        (Vec3::new(0.0, -0.45, -2.7), Pt::Front),
        (Vec3::new(0.0, -0.5, 2.0), Pt::Belly),
        (Vec3::new(0.0, 0.75, -1.0), Pt::Top),
        (Vec3::new(0.0, 1.45, 6.5), Pt::Top),
    ],
    gear_main: (30_000.0, 2_500.0),
    gear_nose: (15_000.0, 1_500.0),
    tail_bumper: (25_000.0, 2_500.0),
    steer: (0.0, 20.0),
    brake_mu: 0.5,
    rest_height: 0.88,
    wing_y: 0.3,
    fly_kt: 30.0,
    vne_kt: 150.0,
    break_kt: 180.0,
    g_limits: (-4.0, 6.5),
    gear_collapse: 3.5,
    tail_crash: 4.0,
    speed_damper: 0.03,
    front_strike: "The nose dug in - touch down on the main wheel, nose slightly up.",
    asi: Asi { max: 160.0, white: (35.0, 60.0), green: (40.0, 110.0), amber: (110.0, 150.0), red: 150.0 },
    eye: Vec3::new(0.0, 0.35, -1.55),
    look_down: 5.0,
    cam_scale: 1.4,
    fan_hz: 0.0,
};

impl Spec {
    pub fn max_flaps(&self) -> u8 {
        (self.flaps.len() - 1) as u8
    }
    pub fn flap_max_deg(&self) -> f32 {
        self.flaps[self.flaps.len() - 1].max(1.0)
    }
    /// Left and right wingtip (navigation lights), in body axes.
    pub fn wingtips(&self) -> (Vec3, Vec3) {
        let tip = |left: bool| self.points.iter().find(|(p, k)| *k == Pt::Wingtip && (p.x < 0.0) == left).map_or(Vec3::ZERO, |(p, _)| *p);
        (tip(true), tip(false))
    }
    /// The white tail light: the rearmost top point.
    pub fn tail_light(&self) -> Vec3 {
        self.points.iter().filter(|(_, k)| *k == Pt::Top).map(|(p, _)| *p).fold(Vec3::ZERO, |a, p| if p.z > a.z { p } else { a })
    }
    /// The red anti-collision beacon on top of the fuselage.
    pub fn beacon(&self) -> Vec3 {
        self.points.iter().filter(|(_, k)| *k == Pt::Top).map(|(p, _)| *p).fold(vec3(0.0, 0.0, 1e9), |a, p| if p.z < a.z { p } else { a })
    }
    /// Contact points of the main wheels, in body axes.
    pub fn main_wheels(&self) -> Vec<Vec3> {
        self.points.iter().filter(|(_, k)| *k == Pt::Main).map(|(p, _)| *p).collect()
    }
    pub fn has_engine(&self) -> bool {
        !matches!(self.engine, Engine::None)
    }
    pub fn is_jet(&self) -> bool {
        matches!(self.engine, Engine::Jet { .. })
    }

    fn stall_alpha(&self, flap_frac: f32) -> f32 {
        let slats = smoothstep(0.0, 0.03, flap_frac);
        (self.stall_deg + self.stall_flap_deg * flap_frac + self.slat_deg * slats).to_radians()
    }

    fn flap_lift(&self, flap_frac: f32) -> f32 {
        self.flap_cl.0 * flap_frac.max(0.0).powf(self.flap_cl.1)
    }

    /// Lift coefficient and how deeply the wing is stalled (0..1).
    fn lift_coeff(&self, alpha: f32, flap_frac: f32) -> (f32, f32) {
        let cl0 = self.cl0 + self.flap_lift(flap_frac);
        let a_pos = self.stall_alpha(flap_frac);
        let a_neg = (-13.0f32).to_radians();
        let plate = 1.1 * (2.0 * alpha).sin();
        if alpha > a_pos {
            let peak = cl0 + self.cla * a_pos;
            let x = alpha - a_pos;
            let post = peak * (0.62 + 0.38 * (-x / 0.06).exp());
            let w = smoothstep(0.35, 0.8, alpha);
            (post * (1.0 - w) + plate * w, (x / 0.05).min(1.0))
        } else if alpha < a_neg {
            let peak = cl0 + self.cla * a_neg;
            let x = a_neg - alpha;
            let post = peak * (0.62 + 0.38 * (-x / 0.06).exp());
            let w = smoothstep(0.35, 0.8, -alpha);
            (post * (1.0 - w) + plate * w, (x / 0.05).min(1.0))
        } else {
            (cl0 + self.cla * alpha, 0.0)
        }
    }

    fn drag_coeff(&self, alpha: f32, cl: f32, flap_frac: f32, ground_effect: f32) -> f32 {
        let ar = self.span * self.span / self.s;
        let induced = cl * cl / (std::f32::consts::PI * self.oswald * ar) * ground_effect;
        let a_s = self.stall_alpha(flap_frac).sin();
        let separated = 1.2 * (alpha.sin().powi(2) - a_s * a_s).max(0.0);
        self.cd0 + self.flap_cd.0 * flap_frac.max(0.0).powf(self.flap_cd.1) + induced + separated
    }

    /// Wave drag as the airflow goes transonic (Mach `m`).
    fn wave_drag(m: f32) -> f32 {
        0.045 * smoothstep(0.82, 1.05, m) - 0.012 * smoothstep(1.2, 1.6, m)
    }

    /// Forward thrust (N). `power` is the throttle for a piston engine and N1 for a jet.
    fn thrust(&self, power: f32, rho: f32, v_fwd: f32) -> f32 {
        match self.engine {
            Engine::Piston { power: p } => {
                let avail = p * ((rho / 1.225 - 0.1) / 0.9).max(0.0);
                (0.04 + 0.96 * power) * avail / v_fwd.max(30.0)
            }
            Engine::Jet { thrust, idle, .. } => {
                let core = ((power - idle) / (1.0 - idle)).max(0.0).powf(1.6);
                let lapse = (rho / 1.225).powf(0.7) * (1.0 - 0.35 * (v_fwd.max(0.0) / 170.0).min(1.0));
                thrust * (0.04 + 0.96 * core) * lapse
            }
            Engine::None => 0.0,
        }
    }

    /// The N1 that gives `t` newtons of thrust (inverse of `thrust`), as a throttle position.
    fn throttle_for(&self, t: f32, rho: f32, v_fwd: f32) -> f32 {
        match self.engine {
            Engine::Piston { power } => {
                let avail = power * ((rho / 1.225 - 0.1) / 0.9).max(0.0);
                ((t * v_fwd.max(30.0) / avail - 0.04) / 0.96).clamp(0.0, 1.0)
            }
            Engine::Jet { thrust, .. } => {
                let lapse = (rho / 1.225).powf(0.7) * (1.0 - 0.35 * (v_fwd.max(0.0) / 170.0).min(1.0));
                let core = ((t / (thrust * lapse) - 0.04) / 0.96).max(0.0);
                core.powf(1.0 / 1.6).clamp(0.0, 0.9)
            }
            Engine::None => 0.0,
        }
    }
}

const MAX_POINTS: usize = 16;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Speedbrake {
    #[default]
    Down,
    /// Deploys by itself when the main wheels touch with the thrust at idle.
    Armed,
    Up,
}

impl Speedbrake {
    pub fn next(self) -> Self {
        match self {
            Speedbrake::Down => Speedbrake::Armed,
            Speedbrake::Armed => Speedbrake::Up,
            Speedbrake::Up => Speedbrake::Down,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub pitch: f32, // +1 full back (nose up)
    pub roll: f32,  // +1 full right
    pub yaw: f32,   // +1 full right rudder
    pub throttle: f32,
    pub flaps: u8,  // lever detent index into Spec::flaps
    pub trim: f32,  // added to pitch, -0.6..0.6
    pub brake: f32, // both brakes
    pub brake_l: f32,
    pub brake_r: f32,
    pub speedbrake: Speedbrake,
    /// Reverse thrust lever, 0..1 (only acts on the ground).
    pub reverse: f32,
    pub gear_up: bool,
}

/// The air around the aircraft this step (see `weather`): wind as a world-space velocity, and
/// the rolling/pitching effect of gusts that differ across the airframe (rad/s).
#[derive(Clone, Copy, Default)]
pub struct Air {
    pub wind: Vec3,
    pub roll_gust: f32,
    pub pitch_gust: f32,
}

#[derive(Clone, Copy)]
pub struct Touchdown {
    pub sink_fpm: f32,
    pub pos: Vec3,
    pub ias_kt: f32,
    pub pitch: f32,
}

#[derive(Clone)]
pub struct Aircraft {
    pub spec: &'static Spec,
    pub craft: Craft,
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub omega: Vec3,
    pub ctl: Controls,
    pub auto_rudder: bool,
    /// Set by the caller before each step.
    pub air: Air,

    // actual surface positions (radians / degrees / fractions)
    pub elev: f32,
    pub ail: f32,
    pub rud: f32,
    pub flap_deg: f32,
    pub prop_angle: f32,
    pub spoiler: f32,
    /// Gear extension, 1 = down and locked.
    pub gear: f32,
    /// Thrust reverser sleeves, 1 = fully deployed.
    pub reverser: f32,
    /// Fan speed as a fraction (jets); the throttle for piston engines.
    pub n1: f32,
    pub thrust: f32,
    /// Afterburner, 0..1.
    pub burner: f32,

    // derived state
    pub alpha: f32,
    pub beta: f32,
    pub stall_alpha: f32,
    pub ias: f32,
    pub stalled: bool,
    pub stall_warn: bool,
    pub on_ground: bool,
    pub airborne_time: f32,
    pub spec_force: Vec3, // body-frame non-gravitational acceleration
    pub crashed: Option<&'static str>,
    pub warning: Option<(&'static str, f32)>,
    pub touchdown: Option<Touchdown>,
    pub touchdown_event: bool,
    pub tail_strike: bool,
    pub pod_strike: bool,
    /// The engine has quit: no thrust (the prop just windmills).
    pub engine_failed: bool,
    pub stall_asym: f32,
    /// Smoothed rate of change of airspeed, m/s^2.
    speed_rate: f32,
    speed_prev: f32,
    pub speed_damper_gain: f32,
    touching: [bool; MAX_POINTS],
    rng: Rng,
}

pub fn density(alt_m: f32) -> f32 {
    1.225 * (-alt_m.max(0.0) / 8500.0).exp()
}

fn approach(cur: f32, target: f32, max_step: f32) -> f32 {
    cur + (target - cur).clamp(-max_step, max_step)
}

impl Aircraft {
    fn blank(craft: Craft) -> Self {
        let spec = craft.spec();
        assert!(spec.points.len() <= MAX_POINTS);
        Aircraft {
            spec,
            craft,
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            omega: Vec3::ZERO,
            ctl: Controls::default(),
            auto_rudder: false,
            air: Air::default(),
            elev: 0.0,
            ail: 0.0,
            rud: 0.0,
            flap_deg: 0.0,
            prop_angle: 0.0,
            spoiler: 0.0,
            gear: 1.0,
            reverser: 0.0,
            n1: match spec.engine {
                Engine::Jet { idle, .. } => idle,
                _ => 0.0,
            },
            thrust: 0.0,
            burner: 0.0,
            alpha: 0.0,
            beta: 0.0,
            stall_alpha: spec.stall_alpha(0.0),
            ias: 0.0,
            stalled: false,
            stall_warn: false,
            on_ground: false,
            airborne_time: 0.0,
            spec_force: vec3(0.0, G, 0.0),
            crashed: None,
            warning: None,
            touchdown: None,
            touchdown_event: false,
            tail_strike: false,
            pod_strike: false,
            engine_failed: false,
            stall_asym: 0.0,
            speed_rate: 0.0,
            speed_prev: 0.0,
            speed_damper_gain: spec.speed_damper,
            touching: [false; MAX_POINTS],
            rng: Rng::new(7),
        }
    }

    pub fn heading_rot(heading_deg: f32) -> Quat {
        Quat::from_rotation_y(-heading_deg.to_radians())
    }

    /// Parked on the ground at (x, z) facing `heading_deg`.
    pub fn on_ground_at(craft: Craft, terrain: &Terrain, x: f32, z: f32, heading_deg: f32) -> Self {
        let mut a = Self::blank(craft);
        a.pos = vec3(x, terrain.height(x, z) + a.spec.rest_height, z);
        a.rot = Self::heading_rot(heading_deg);
        a.on_ground = true;
        for (k, (_, kind)) in a.spec.points.iter().enumerate() {
            a.touching[k] = matches!(kind, Pt::Nose | Pt::Main);
        }
        a
    }

    /// In steady, trimmed flight: speed in knots (indicated), flight path angle in degrees,
    /// flap lever detent. A retractable gear is down from the middle flap settings on
    /// (an approach configuration) and up otherwise.
    pub fn in_flight(craft: Craft, pos: Vec3, heading_deg: f32, ias_kt: f32, gamma_deg: f32, flaps: u8) -> Self {
        let mut a = Self::blank(craft);
        let spec = a.spec;
        let rho = density(pos.y);
        let v = ias_kt * KT / (rho / 1.225).sqrt();
        let gamma = gamma_deg.to_radians();
        let flap_deg = spec.flaps[flaps as usize];
        let ff = flap_deg / spec.flap_max_deg();
        a.ctl.gear_up = spec.retractable && (flaps as usize) < spec.flaps.len() / 2;
        a.gear = if a.ctl.gear_up { 0.0 } else { 1.0 };
        if spec.spoilers && !a.ctl.gear_up {
            a.ctl.speedbrake = Speedbrake::Armed;
        }
        let qbar = 0.5 * rho * v * v;
        let cl = spec.mass * G * gamma.cos() / (qbar * spec.s);
        let alpha = (cl - spec.cl0 - spec.flap_lift(ff)) / spec.cla;
        let cd = spec.drag_coeff(alpha, cl, ff, 1.0) + spec.gear_cd * a.gear;
        let t_needed = qbar * spec.s * cd + spec.mass * G * gamma.sin();
        let throttle = spec.throttle_for(t_needed, rho, v);
        let thrust_cm = t_needed * spec.thrust_arm / (qbar * spec.s * spec.chord);
        let elev = -(spec.cm0 + spec.cma * alpha + spec.cm_flaps * ff + thrust_cm) / spec.cmde;

        let hdg = Self::heading_rot(heading_deg);
        a.pos = pos;
        a.rot = hdg * Quat::from_rotation_x(gamma + alpha);
        a.vel = hdg * vec3(0.0, gamma.sin(), -gamma.cos()) * v;
        a.ctl.throttle = throttle;
        a.thrust = t_needed;
        a.ctl.flaps = flaps;
        a.ctl.trim = (elev / spec.elev_max).clamp(-0.6, 0.6);
        if let Engine::Jet { idle, .. } = spec.engine {
            a.n1 = idle + (1.0 - idle) * throttle;
        }
        a.elev = elev;
        a.flap_deg = flap_deg;
        a.alpha = alpha;
        a.ias = ias_kt * KT;
        a.airborne_time = 10.0;
        a.speed_prev = v;
        a
    }

    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::NEG_Z
    }
    pub fn up(&self) -> Vec3 {
        self.rot * Vec3::Y
    }
    pub fn right(&self) -> Vec3 {
        self.rot * Vec3::X
    }
    pub fn heading_deg(&self) -> f32 {
        let f = self.forward();
        f.x.atan2(-f.z).to_degrees().rem_euclid(360.0)
    }
    pub fn pitch_deg(&self) -> f32 {
        self.forward().y.clamp(-1.0, 1.0).asin().to_degrees()
    }
    pub fn bank_deg(&self) -> f32 {
        (-self.right().y).atan2(self.up().y).to_degrees()
    }
    /// Total-energy climb rate (m/s): the vertical speed corrected for speed being traded for
    /// height, which is what a sailplane's variometer shows so a pull-up isn't mistaken for lift.
    pub fn energy_rate(&self) -> f32 {
        self.vel.y + self.vel.length() * self.speed_rate / G
    }
    /// Rate of heading change, deg/s, positive to the right.
    pub fn turn_rate_dps(&self) -> f32 {
        -(self.rot * self.omega).y.to_degrees()
    }

    /// Indicated airspeed the plane settles at with the stick centred, from the trim setting
    /// (pitch-moment balance with the current prop wash or thrust). None if trimmed too far
    /// nose-down to say.
    pub fn trim_speed_kt(&self) -> Option<f32> {
        let spec = self.spec;
        let ff = self.flap_deg / spec.flap_max_deg();
        let mut v = self.ias.max(20.0);
        let mut cl = 0.0;
        for _ in 0..4 {
            let (wash, thrust_cm) = match spec.engine {
                Engine::Piston { .. } => (20.0 * self.ctl.throttle, 0.0),
                _ => (0.0, self.thrust * spec.thrust_arm / (0.6125 * v * v * spec.s * spec.chord)),
            };
            let tail = 1.0 + wash * wash / (v * v);
            let alpha = -(spec.cm0 + spec.cm_flaps * ff + thrust_cm + tail * spec.cmde * self.ctl.trim * spec.elev_max) / spec.cma;
            cl = spec.cl0 + spec.flap_lift(ff) + spec.cla * alpha.min(spec.stall_alpha(ff));
            if cl <= 0.12 {
                return None;
            }
            v = (2.0 * spec.mass * G / (1.225 * spec.s * cl)).sqrt();
        }
        (cl > 0.12).then_some(v / KT)
    }

    pub fn step(&mut self, dt: f32, terrain: &Terrain) {
        if self.crashed.is_some() {
            return;
        }
        if let Some((_, t)) = &mut self.warning {
            *t -= dt;
            if *t <= 0.0 {
                self.warning = None;
            }
        }
        let spec = self.spec;

        let rho = density(self.pos.y);
        // aerodynamics work on the velocity through the air, not over the ground
        let air_vel = self.vel - self.air.wind;
        let vb = self.rot.inverse() * air_vel;
        let v = vb.length();
        let (alpha, beta) = if v > 0.5 { ((-vb.y).atan2(-vb.z), (vb.x / v).clamp(-1.0, 1.0).asin()) } else { (0.0, 0.0) };
        self.alpha = alpha;
        self.beta = beta;
        self.ias = v * (rho / 1.225).sqrt();
        let flying = !self.on_ground && self.ias > spec.fly_kt * KT;

        // --- control surfaces (actuators move at a finite rate) ---
        let mut rud_cmd = self.ctl.yaw;
        if self.auto_rudder && !self.on_ground {
            rud_cmd += self.ctl.roll * 0.25 + beta * 5.0;
        }
        let raw_rate = (v - self.speed_prev) / dt;
        self.speed_prev = v;
        self.speed_rate += (raw_rate - self.speed_rate) * (dt / 0.5).min(1.0);
        // The damper hands over to the pilot as soon as they move the stick, and stays out of stalls.
        let hands_off = (1.0 - self.ctl.pitch.abs() * 3.0).max(0.0);
        let assist = if flying && !self.stalled { hands_off * smoothstep(1.25 * spec.fly_kt * KT, 2.0 * spec.fly_kt * KT, self.ias) } else { 0.0 };
        let damper = (self.speed_rate * self.speed_damper_gain * assist).clamp(-0.08, 0.08);
        let elev_cmd = ((self.ctl.pitch + self.ctl.trim).clamp(-1.0, 1.0) * spec.elev_max + damper).clamp(-spec.elev_max, spec.elev_max);
        self.elev = approach(self.elev, elev_cmd, 2.5 * dt);
        self.ail = approach(self.ail, self.ctl.roll.clamp(-1.0, 1.0) * spec.ail_max, 2.5 * dt);
        self.rud = approach(self.rud, rud_cmd.clamp(-1.0, 1.0) * spec.rud_max, 2.5 * dt);
        let flap_cmd = spec.flaps[(self.ctl.flaps as usize).min(spec.flaps.len() - 1)];
        self.flap_deg = approach(self.flap_deg, flap_cmd, spec.flap_rate * dt);
        let ff = self.flap_deg / spec.flap_max_deg();
        self.stall_alpha = spec.stall_alpha(ff);

        // --- gear, speedbrakes, engines ---
        let mains_down = spec.points.iter().zip(self.touching.iter()).any(|((_, k), t)| *k == Pt::Main && *t);
        if spec.retractable {
            if self.on_ground {
                self.ctl.gear_up = false; // weight on wheels: the lever won't come up
            }
            self.gear = approach(self.gear, if self.ctl.gear_up { 0.0 } else { 1.0 }, dt / 7.0);
        }
        if spec.spoilers {
            if self.ctl.speedbrake == Speedbrake::Armed && mains_down && self.ctl.throttle < 0.15 {
                self.ctl.speedbrake = Speedbrake::Up;
            }
            if self.ctl.speedbrake == Speedbrake::Up && self.on_ground && self.ctl.throttle > 0.5 {
                self.ctl.speedbrake = Speedbrake::Down; // advancing the thrust levers stows them
            }
            let target = match self.ctl.speedbrake {
                Speedbrake::Up if self.on_ground => 1.0,
                Speedbrake::Up => 0.5,
                _ => 0.0,
            };
            self.spoiler = approach(self.spoiler, target, 1.5 * dt);
        }
        let power = match spec.engine {
            Engine::Piston { .. } => {
                self.n1 = self.ctl.throttle;
                self.prop_angle = (self.prop_angle + dt * (25.0 + 200.0 * self.ctl.throttle + v)) % std::f32::consts::TAU;
                self.ctl.throttle
            }
            Engine::Jet { idle, .. } => {
                // Reversers only deploy with weight on the wheels, then the fans spool up.
                let rev_ok = mains_down || self.on_ground;
                self.reverser = approach(self.reverser, if self.ctl.reverse > 0.0 && rev_ok { 1.0 } else { 0.0 }, dt / 2.0);
                let target = if self.reverser > 0.9 && self.ctl.reverse > 0.0 {
                    idle + (0.85 - idle) * self.ctl.reverse
                } else if self.reverser > 0.0 {
                    idle
                } else {
                    idle + (1.0 - idle) * self.ctl.throttle
                };
                // Turbofans spool slowly from idle: about 6 s to go-around thrust.
                let tau = if target > self.n1 { 0.9 + 2.6 * (1.0 - self.n1) } else { 1.6 };
                self.n1 += (target - self.n1) * (dt / tau).min(1.0);
                self.prop_angle = (self.prop_angle + dt * self.n1 * 70.0) % std::f32::consts::TAU;
                self.n1
            }
            Engine::None => 0.0,
        };
        // the afterburner lights in the last stretch of throttle once the engine is up to speed
        let ab_target = match spec.engine {
            Engine::Jet { afterburner, .. } if afterburner > 0.0 && !self.engine_failed && self.n1 > 0.93 => smoothstep(0.9, 1.0, self.ctl.throttle),
            _ => 0.0,
        };
        self.burner = approach(self.burner, ab_target, dt * 1.5);

        // --- aerodynamics ---
        let ground_h = terrain.surface(self.pos.x, self.pos.z);
        let wing_agl = (self.pos.y + spec.wing_y - ground_h).max(0.1);
        let hb = wing_agl / spec.span;
        let ground_effect = (16.0 * hb * hb) / (1.0 + 16.0 * hb * hb);

        let (cl_wing, stall_depth) = spec.lift_coeff(alpha, ff);
        // Spoilers dump lift (putting the weight on the wheels for braking) and add drag.
        let cl = cl_wing * (1.0 - 0.5 * self.spoiler);
        let mach = v / 340.0;
        let cd = spec.drag_coeff(alpha, cl, ff, ground_effect) + spec.gear_cd * self.gear + 0.07 * self.spoiler + Spec::wave_drag(mach);
        let qbar = 0.5 * rho * v * v;
        let right = self.right();
        let fwd = self.forward();
        let (s, span, chord) = (spec.s, spec.span, spec.chord);

        let mut force = vec3(0.0, -spec.mass * G, 0.0);
        if v > 0.5 {
            let vhat = air_vel / v;
            let lift_dir = right.cross(vhat).try_normalize().unwrap_or(self.up());
            let cy = spec.cy_beta * beta;
            force += lift_dir * (qbar * s * cl) - vhat * (qbar * s * cd) + right * (qbar * s * cy);
        }
        let mut thrust = if self.engine_failed { -0.01 * spec.mass * G * (v / 40.0).min(1.0) } else { spec.thrust(power, rho, -vb.z) };
        if let Engine::Jet { afterburner, .. } = spec.engine {
            thrust += afterburner * self.burner * (rho / 1.225).powf(0.7);
        }
        if let Engine::Jet { reverse, .. } = spec.engine {
            thrust *= 1.0 - self.reverser * (1.0 + reverse);
        }
        self.thrust = thrust;
        force += fwd * thrust;

        // Moments in conventional form: L roll (right wing down +), M pitch (nose up +), N yaw (nose right +)
        let veff = v.max(12.0);
        // gusts that differ across the wing act like a roll rate the wing has to damp out
        let (p, q, r) = (-self.omega.z + self.air.roll_gust, self.omega.x - self.air.pitch_gust, -self.omega.y);
        let ph = p * span / (2.0 * veff);
        let qh = q * chord / (2.0 * veff);
        let rh = r * span / (2.0 * veff);
        let (prop_wash, p_factor) = match spec.engine {
            Engine::Piston { .. } => (20.0 * self.ctl.throttle, -180.0 * self.ctl.throttle * (1.0 - v / 40.0).max(0.0)),
            _ => (0.0, 0.0),
        };
        let qbar_tail = qbar + 0.5 * rho * prop_wash * prop_wash;

        if stall_depth > 0.0 && flying {
            if !self.stalled {
                self.stall_asym = if self.rng.f() < 0.5 { -1.0 } else { 1.0 } * self.rng.range(0.008, 0.02);
            }
        }
        let a_s = self.stall_alpha;
        self.stalled = flying && if self.stalled { alpha > a_s - 2f32.to_radians() } else { alpha > a_s + 0.5f32.to_radians() };
        self.stall_warn = flying && alpha > a_s - 5f32.to_radians();

        let (cl_b, cl_p, cl_r, cl_da) = spec.roll;
        let (cn_b, cn_r, cn_p, cn_da, cn_dr) = spec.yaw;
        let roll_damp = cl_p + 0.5 * stall_depth;
        let ail_eff = 1.0 - 0.5 * stall_depth;
        let cl_roll = cl_b * beta + roll_damp * ph + cl_r * rh + cl_da * self.ail * ail_eff
            + stall_depth * (self.stall_asym - 0.25 * beta);
        let post_stall = (alpha.abs() - a_s).max(0.0) * alpha.signum();
        let cm = spec.cm0 + spec.cma * alpha.clamp(-0.5, 0.5) + spec.cmq * qh + spec.cm_flaps * ff - 0.6 * post_stall;
        let cn = cn_b * beta + cn_r * rh + cn_p * ph + cn_da * self.ail;

        let roll_m = qbar * s * span * cl_roll;
        let pitch_m = qbar * s * chord * cm + qbar_tail * s * chord * spec.cmde * self.elev + thrust * spec.thrust_arm;
        let yaw_m = qbar * s * span * cn + qbar_tail * s * span * cn_dr * self.rud + p_factor;
        let mut torque_b = vec3(pitch_m, -yaw_m, -roll_m);

        // --- ground contact ---
        let omega_w = self.rot * self.omega;
        let mut torque_w = Vec3::ZERO;
        let mut touching = [false; MAX_POINTS];
        let mut wheels_down = false;
        let mains_before = mains_down;
        for (k, (bp, kind)) in spec.points.iter().enumerate() {
            let r = self.rot * *bp;
            let wp = self.pos + r;
            let th = terrain.height(wp.x, wp.z);
            let surf = th.max(SEA);
            let pen = surf - wp.y;
            if pen <= 0.0 {
                continue;
            }
            touching[k] = true;
            let vp = self.vel + omega_w.cross(r);
            if th < SEA {
                self.crashed = Some("Splashdown! This plane doesn't float. Stay over land when landing.");
                return;
            }
            let n = terrain.normal(wp.x, wp.z);
            let vn = vp.dot(n);
            let speed = vp.length();
            let first = !self.touching[k];
            match kind {
                Pt::Nose | Pt::Main => {
                    if self.gear < 0.95 {
                        self.crashed = Some(if self.gear < 0.3 {
                            "Gear-up landing! Lower the landing gear (L) before touching down."
                        } else {
                            "The gear wasn't down and locked - it folded on touchdown. Lower it earlier."
                        });
                        return;
                    }
                    if first && self.airborne_time > 0.5 {
                        if -vn > spec.gear_collapse {
                            self.crashed = Some("Hard landing - the gear collapsed. Flare to slow the descent just before touchdown.");
                            return;
                        }
                        if *kind == Pt::Main && !self.touchdown_event && !mains_before {
                            self.touchdown = Some(Touchdown { sink_fpm: -vn / FT * 60.0, pos: self.pos, ias_kt: self.ias / KT, pitch: self.pitch_deg() });
                            self.touchdown_event = true;
                        }
                        if *kind == Pt::Nose && !mains_before && -vn > 1.5 {
                            self.warning = Some(("Nose wheel first! Raise the nose to land on the mains.", 3.0));
                        }
                    }
                    wheels_down = true;
                    let (k_spring, damp) = if *kind == Pt::Nose { spec.gear_nose } else { spec.gear_main };
                    let fn_ = (k_spring * pen - damp * vn).max(0.0);
                    let fwd_g = (fwd - n * fwd.dot(n)).normalize_or_zero();
                    let mut dir = fwd_g;
                    if *kind == Pt::Nose {
                        let steer = self.ctl.yaw * spec.steer.0 * (1.0 - v / spec.steer.1).clamp(0.12, 1.0);
                        dir = Quat::from_axis_angle(n, -steer) * fwd_g;
                    }
                    let side = dir.cross(n);
                    let brake = self.ctl.brake.max(if bp.x < 0.0 { self.ctl.brake_l } else { self.ctl.brake_r });
                    let mu_long = 0.025 + if *kind == Pt::Main { spec.brake_mu * brake } else { 0.0 };
                    let f_long = -mu_long * fn_ * (vp.dot(dir) / 0.3).clamp(-1.0, 1.0);
                    let f_lat = -0.85 * fn_ * (vp.dot(side) / 0.25).clamp(-1.0, 1.0);
                    let f = n * fn_ + dir * f_long + side * f_lat;
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Skid => {
                    // a sliding bumper: springy, scrapy, and meant to touch
                    let (kb, cb) = spec.tail_bumper;
                    let fn_ = (kb * pen - cb * vn).max(0.0);
                    let tang = vp - n * vn;
                    let f = n * fn_ - tang.normalize_or_zero() * (0.4 * fn_).min(tang.length() * 4.0 * spec.mass);
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Tail => {
                    if -vn > spec.tail_crash {
                        self.crashed = Some("Tail strike - the tail hit the ground hard. Don't over-rotate.");
                        return;
                    }
                    if first {
                        self.tail_strike = true;
                        self.warning = Some(("Tail strike! Easy on the back pressure.", 3.0));
                    }
                    let (kb, cb) = spec.tail_bumper;
                    let fn_ = (kb * pen - cb * vn).max(0.0);
                    let tang = vp - n * vn;
                    let f = n * fn_ - tang.normalize_or_zero() * (0.5 * fn_).min(tang.length() * 4.0 * spec.mass);
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Wingtip => {
                    self.crashed = Some(if speed > 25.0 { "A wingtip hit the ground. Keep the wings level near the ground." } else { "Wingtip strike while taxiing or landing - keep the wings level." });
                    return;
                }
                Pt::Nacelle => {
                    // a scrape is expensive, not fatal - unless it digs in hard
                    if -vn > 2.5 {
                        self.crashed = Some("An engine pod hit the runway. Keep the wings level close to the ground.");
                        return;
                    }
                    if first {
                        self.pod_strike = true;
                        self.warning = Some(("Engine pod scrape! Wings level near the ground.", 3.0));
                    }
                    let (kb, cb) = spec.tail_bumper;
                    let fn_ = (kb * pen - cb * vn).max(0.0);
                    let tang = vp - n * vn;
                    let f = n * fn_ - tang.normalize_or_zero() * (0.5 * fn_).min(tang.length() * 4.0 * spec.mass);
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Front => {
                    self.crashed = Some(if speed > 30.0 && -vn > 3.0 { "Flew into the ground. Watch your altitude and descent rate!" } else { spec.front_strike });
                    return;
                }
                Pt::Belly => {
                    self.crashed = Some(if -vn > 4.0 { "Flew into the ground. Watch your altitude and descent rate!" } else { "Belly landing - the fuselage touched down before the wheels." });
                    return;
                }
                Pt::Top => {
                    self.crashed = Some("Flipped over! Keep the plane upright near the ground.");
                    return;
                }
            }
        }
        self.touching = touching;
        self.on_ground = wheels_down;
        if wheels_down {
            self.airborne_time = 0.0;
        } else {
            self.airborne_time += dt;
        }
        torque_b += self.rot.inverse() * torque_w;

        // --- integrate ---
        let accel = force / spec.mass;
        self.spec_force = self.rot.inverse() * (accel + vec3(0.0, G, 0.0));
        self.vel += accel * dt;
        self.pos += self.vel * dt;
        let iw = spec.inertia * self.omega;
        let domega = (torque_b - self.omega.cross(iw)) / spec.inertia;
        self.omega += domega * dt;
        self.rot = (self.rot * Quat::from_scaled_axis(self.omega * dt)).normalize();

        // --- limits ---
        let g = self.spec_force.y / G;
        if self.ias / KT > spec.break_kt {
            self.crashed = Some("Far past the maximum speed - the airframe broke up. Watch the red line!");
        } else if !(spec.g_limits.0..=spec.g_limits.1).contains(&g) && !self.on_ground {
            self.crashed = Some("Overstressed the airframe - pulled too many G. Smooth inputs at high speed!");
        }
    }
}

/// A snapshot of the aircraft in pilot units, for the instruments and lessons.
#[derive(Clone, Copy, Default)]
pub struct Telem {
    pub craft: Craft,
    pub ias_kt: f32,
    pub gs_kt: f32,
    pub alt_ft: f32,
    pub agl_ft: f32,
    pub vs_fpm: f32,
    /// Total-energy vertical speed, m/s (the sailplane's variometer).
    pub vario: f32,
    pub hdg: f32,
    pub pitch: f32,
    pub bank: f32,
    pub aoa: f32,
    pub stall_aoa: f32,
    pub slip: f32, // inclinometer ball, -1..1 (left..right)
    pub turn_rate: f32,
    pub g: f32,
    pub throttle: f32,
    pub rpm: f32,
    /// Fan speed, percent (jets).
    pub n1: f32,
    pub flaps: u8,
    pub flap_deg: f32,
    pub trim: f32,
    /// Speed the trim is set for, knots (0 when off the scale).
    pub trim_kt: f32,
    pub stick_pitch: f32,
    pub stick_roll: f32,
    pub brake: f32,
    pub speedbrake: Speedbrake,
    pub spoiler: f32,
    pub reverser: f32,
    pub gear: f32,
    pub on_ground: bool,
    pub stalled: bool,
    pub stall_warn: bool,
    pub pos: Vec3,
}

impl Aircraft {
    pub fn telem(&self, terrain: &Terrain) -> Telem {
        let ground = terrain.surface(self.pos.x, self.pos.z);
        let c = &self.ctl;
        Telem {
            craft: self.craft,
            ias_kt: self.ias / KT,
            gs_kt: vec3(self.vel.x, 0.0, self.vel.z).length() / KT,
            alt_ft: self.pos.y / FT,
            agl_ft: (self.pos.y - self.spec.rest_height - ground).max(0.0) / FT,
            vs_fpm: self.vel.y / FT * 60.0,
            vario: self.energy_rate(),
            hdg: self.heading_deg(),
            pitch: self.pitch_deg(),
            bank: self.bank_deg(),
            aoa: self.alpha.to_degrees(),
            stall_aoa: self.stall_alpha.to_degrees(),
            slip: (-self.spec_force.x / G * 4.0).clamp(-1.0, 1.0),
            turn_rate: self.turn_rate_dps(),
            g: self.spec_force.y / G,
            throttle: c.throttle,
            rpm: (700.0 + 1900.0 * c.throttle + self.ias * 6.0).min(2750.0),
            n1: self.n1 * 100.0,
            flaps: c.flaps,
            flap_deg: self.flap_deg,
            trim: c.trim,
            trim_kt: self.trim_speed_kt().unwrap_or(0.0),
            stick_pitch: c.pitch,
            stick_roll: c.roll,
            brake: c.brake.max(c.brake_l).max(c.brake_r),
            speedbrake: c.speedbrake,
            spoiler: self.spoiler,
            reverser: self.reverser,
            gear: self.gear,
            on_ground: self.on_ground,
            stalled: self.stalled,
            stall_warn: self.stall_warn,
            pos: self.pos,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Terrain, RWY_ELEV, RWY_HALF_LEN};

    fn fly(ac: &mut Aircraft, t: &Terrain, secs: f32) {
        let steps = (secs * 240.0) as usize;
        for _ in 0..steps {
            ac.step(1.0 / 240.0, t);
        }
    }

    #[test]
    fn trimmed_cruise_is_stable() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Cessna, vec3(0.0, 900.0, 0.0), 90.0, 100.0, 0.0, 0);
        fly(&mut ac, &t, 30.0);
        assert!(ac.crashed.is_none());
        let alt_err = (ac.pos.y - 900.0).abs();
        assert!(alt_err < 60.0, "altitude drifted {alt_err} m");
        assert!(ac.bank_deg().abs() < 5.0, "bank {}", ac.bank_deg());
        assert!((ac.ias / KT - 100.0).abs() < 10.0, "ias {}", ac.ias / KT);
    }

    #[test]
    fn approach_is_steady() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Cessna, vec3(-3000.0, 300.0, 0.0), 90.0, 70.0, -3.0, 2);
        fly(&mut ac, &t, 15.0);
        assert!(ac.crashed.is_none());
        let vs = ac.vel.y / FT * 60.0;
        assert!((-650.0..-150.0).contains(&vs), "vs {vs}");
    }

    #[test]
    fn parked_plane_stays_put() {
        let t = Terrain::generate();
        let mut ac = Aircraft::on_ground_at(Craft::Cessna, &t, -RWY_HALF_LEN + 30.0, 0.0, 90.0);
        let start = ac.pos;
        fly(&mut ac, &t, 10.0);
        assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        assert!(ac.on_ground);
        assert!((ac.pos - start).length() < 3.0, "moved {}", (ac.pos - start).length());
        assert!((ac.pos.y - RWY_ELEV - 1.35).abs() < 0.2, "height {}", ac.pos.y);
    }

    #[test]
    fn takeoff_and_climb() {
        let t = Terrain::generate();
        let mut ac = Aircraft::on_ground_at(Craft::Cessna, &t, -RWY_HALF_LEN + 30.0, 0.0, 90.0);
        ac.ctl.throttle = 1.0;
        let mut lifted_at = None;
        for i in 0..(60 * 240) {
            // simple pilot: rotate at 55 kt, then hold ~75 kt
            let ias_kt = ac.ias / KT;
            ac.ctl.pitch = if ac.on_ground { if ias_kt > 55.0 { 0.45 } else { 0.0 } } else { ((75.0 - ias_kt) * -0.05 + 0.15).clamp(-0.5, 0.6) };
            ac.ctl.roll = (-ac.bank_deg() * 0.05).clamp(-0.5, 0.5);
            ac.step(1.0 / 240.0, &t);
            assert!(ac.crashed.is_none(), "{:?} at step {i}", ac.crashed);
            if lifted_at.is_none() && !ac.on_ground && ac.pos.y > RWY_ELEV + 5.0 {
                lifted_at = Some(ac.pos.x + RWY_HALF_LEN);
            }
        }
        let roll = lifted_at.expect("never lifted off");
        assert!(roll < 900.0, "takeoff roll {roll} m");
        let climb = ac.vel.y / FT * 60.0;
        assert!(climb > 400.0, "climb {climb} fpm");
    }

    #[test]
    fn trim_change_settles_quickly() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Cessna, vec3(0.0, 1200.0, 0.0), 0.0, 100.0, 0.0, 0);
        ac.ctl.trim -= 0.04;
        let predicted = ac.trim_speed_kt().unwrap();
        fly(&mut ac, &t, 25.0);
        let vs_25 = ac.vel.y / FT * 60.0;
        let mut swing: f32 = 0.0;
        for _ in 0..20 {
            fly(&mut ac, &t, 1.0);
            swing = swing.max((ac.vel.y / FT * 60.0 - vs_25).abs());
        }
        assert!(swing < 60.0, "still porpoising: VS moved {swing} fpm after 25 s");
        assert!((ac.ias / KT - predicted).abs() < 2.0, "settled at {} kt, trim said {predicted}", ac.ias / KT);
    }

    #[test]
    fn stall_happens_and_recovers() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Cessna, vec3(0.0, 1400.0, 0.0), 0.0, 85.0, 0.0, 0);
        ac.ctl.throttle = 0.0;
        let mut stalled_at = None;
        for i in 0..(40 * 240) {
            // hold altitude while slowing down, like a real stall entry
            let want = (-ac.vel.y * 0.3 + (1400.0 - ac.pos.y) * 0.02).clamp(-0.2, 1.0);
            ac.ctl.pitch = (ac.ctl.pitch + (want - ac.ctl.pitch).clamp(-0.002, 0.002)).max(i as f32 / (240.0 * 40.0));
            ac.step(1.0 / 240.0, &t);
            if ac.stalled && stalled_at.is_none() {
                stalled_at = Some(ac.ias / KT);
            }
        }
        let s = stalled_at.expect("never stalled");
        assert!((40.0..60.0).contains(&s), "stalled at {s} kt");
        ac.ctl.pitch = -0.3;
        ac.ctl.throttle = 1.0;
        fly(&mut ac, &t, 6.0);
        assert!(!ac.stalled);
        assert!(ac.crashed.is_none());
    }
}



#[cfg(test)]
mod jet_tests {
    use super::*;
    use crate::world::{Terrain, RWY_HALF_LEN};

    fn fly(ac: &mut Aircraft, t: &Terrain, secs: f32) {
        for _ in 0..(secs * 240.0) as usize {
            ac.step(1.0 / 240.0, t);
        }
    }

    #[test]
    fn stall_speeds_are_plausible() {
        let s = &JET;
        let vs = |detent: usize| {
            let ff = s.flaps[detent] / s.flap_max_deg();
            let clmax = s.cl0 + s.flap_lift(ff) + s.cla * s.stall_alpha(ff);
            (2.0 * s.mass * G / (1.225 * s.s * clmax)).sqrt() / KT
        };
        assert!((140.0..160.0).contains(&vs(0)), "clean stall {}", vs(0));
        assert!((105.0..120.0).contains(&vs(7)), "flaps 30 stall {}", vs(7));
    }

    #[test]
    fn approach_is_steady() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Jet, vec3(-9000.0, 600.0, 0.0), 90.0, 145.0, -3.0, 7);
        assert!(ac.ctl.speedbrake == Speedbrake::Armed && !ac.ctl.gear_up);
        assert!((0.4..0.6).contains(&ac.n1), "approach N1 {}", ac.n1);
        fly(&mut ac, &t, 20.0);
        let vs = ac.vel.y / FT * 60.0;
        assert!(ac.crashed.is_none());
        assert!((-850.0..-600.0).contains(&vs), "vs {vs}");
        assert!((ac.ias / KT - 145.0).abs() < 5.0, "ias {}", ac.ias / KT);
        assert!((0.5..4.0).contains(&ac.pitch_deg()), "pitch {}", ac.pitch_deg());
    }

    #[test]
    fn trim_change_settles() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(Craft::Jet, vec3(0.0, 3000.0, 0.0), 0.0, 220.0, 0.0, 0);
        ac.ctl.trim += 0.02;
        let predicted = ac.trim_speed_kt().unwrap();
        // a jet's phugoid is slower (~50 s at this speed), so give it longer to settle
        fly(&mut ac, &t, 45.0);
        let vs_45 = ac.vel.y / FT * 60.0;
        let mut swing: f32 = 0.0;
        for _ in 0..20 {
            fly(&mut ac, &t, 1.0);
            swing = swing.max((ac.vel.y / FT * 60.0 - vs_45).abs());
        }
        assert!(swing < 60.0, "still porpoising: VS moved {swing} fpm");
        assert!((ac.ias / KT - predicted).abs() < 5.0, "settled at {} kt, trim said {predicted}", ac.ias / KT);
    }

    #[test]
    fn takeoff_fits_the_runway() {
        let t = Terrain::generate();
        let mut ac = Aircraft::on_ground_at(Craft::Jet, &t, -RWY_HALF_LEN + 40.0, 0.0, 90.0);
        ac.ctl.flaps = 3;
        ac.ctl.throttle = 1.0;
        let mut lifted = None;
        for _ in 0..(60 * 240) {
            let ias = ac.ias / KT;
            let target = if ias < 145.0 && ac.on_ground { 0.0 } else if ac.on_ground { 9.0 } else { (12.0 + (ias - 160.0) * 0.4).clamp(5.0, 17.0) };
            ac.ctl.pitch = ((target - ac.pitch_deg()) * 0.12 - ac.omega.x * 2.5).clamp(-1.0, 1.0);
            ac.ctl.roll = (-ac.bank_deg() * 0.05).clamp(-0.5, 0.5);
            ac.ctl.yaw = if ac.on_ground { (-ac.pos.z * 0.1 - (ac.heading_deg() - 90.0) * 0.2).clamp(-1.0, 1.0) } else { 0.0 };
            ac.step(1.0 / 240.0, &t);
            assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
            if lifted.is_none() && !ac.on_ground {
                lifted = Some(ac.pos.x + RWY_HALF_LEN);
            }
        }
        let lifted = lifted.expect("never lifted off");
        assert!(lifted < 2200.0, "lifted off after {lifted} m");
        assert!(ac.vel.y / FT * 60.0 > 1500.0, "climb {}", ac.vel.y / FT * 60.0);
    }

    #[test]
    fn spoilers_reversers_and_brakes_stop_it() {
        let t = Terrain::generate();
        let mut ac = Aircraft::on_ground_at(Craft::Jet, &t, -RWY_HALF_LEN + 400.0, 0.0, 90.0);
        ac.vel = vec3(135.0 * KT, 0.0, 0.0);
        ac.ctl.flaps = 7;
        ac.flap_deg = 30.0;
        ac.ctl.speedbrake = Speedbrake::Armed;
        let x0 = ac.pos.x;
        let mut secs = 0.0;
        while ac.vel.length() > 1.0 && secs < 90.0 {
            ac.ctl.reverse = if ac.ias / KT > 60.0 { 1.0 } else { 0.0 };
            ac.ctl.brake = if secs > 1.0 { 1.0 } else { 0.0 };
            ac.ctl.yaw = (-ac.pos.z * 0.1 - (ac.heading_deg() - 90.0) * 0.2).clamp(-1.0, 1.0);
            ac.step(1.0 / 240.0, &t);
            secs += 1.0 / 240.0;
            assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        }
        let roll = ac.pos.x - x0;
        assert!((450.0..1200.0).contains(&roll), "stopped in {roll} m");
        assert!(ac.spoiler > 0.9, "ground spoilers didn't deploy");
    }
}

#[cfg(test)]
mod new_craft_probe {
    use super::*;
    use crate::world::{Terrain, RWY_HALF_LEN};
    fn fly(ac: &mut Aircraft, t: &Terrain, secs: f32) {
        for _ in 0..(secs * 240.0) as usize {
            ac.step(1.0 / 240.0, t);
        }
    }
    #[test]
    #[ignore]
    fn probe() {
        let t = Terrain::generate();
        // fighter cruise / approach / roll / top speed / takeoff
        let mut ac = Aircraft::in_flight(Craft::Fighter, vec3(0.0, 3000.0, 0.0), 90.0, 400.0, 0.0, 0);
        let y0 = ac.pos.y;
        fly(&mut ac, &t, 20.0);
        println!("F16 cruise 400: thr {:.2} dy {:.0} ias {:.0} crashed {:?}", ac.ctl.throttle, ac.pos.y - y0, ac.ias / KT, ac.crashed);
        let mut ac = Aircraft::in_flight(Craft::Fighter, vec3(-9000.0, 400.0, 0.0), 90.0, 145.0, -3.0, 1);
        println!("F16 approach: thr {:.2} n1 {:.2} pitch {:.1} alpha {:.1} gear {}", ac.ctl.throttle, ac.n1, ac.pitch_deg(), ac.alpha.to_degrees(), ac.gear);
        fly(&mut ac, &t, 15.0);
        println!("   after 15s vs {:.0} ias {:.0} pitch {:.1} crashed {:?}", ac.vel.y / FT * 60.0, ac.ias / KT, ac.pitch_deg(), ac.crashed);
        let mut ac = Aircraft::in_flight(Craft::Fighter, vec3(0.0, 3000.0, 0.0), 90.0, 350.0, 0.0, 0);
        ac.ctl.roll = 1.0;
        fly(&mut ac, &t, 1.5);
        println!("F16 roll rate {:.0} deg/s", ac.omega.z.to_degrees().abs());
        let mut ac = Aircraft::in_flight(Craft::Fighter, vec3(0.0, 300.0, 0.0), 90.0, 300.0, 0.0, 0);
        ac.ctl.throttle = 1.0;
        for _ in 0..60 { fly(&mut ac, &t, 1.0); ac.ctl.pitch = ((0.0 - ac.vel.y) * 0.02 - ac.omega.x).clamp(-0.3, 0.3); }
        println!("F16 top speed sea level {:.0} kt mach {:.2} burner {:.2} g {:?}", ac.ias / KT, ac.vel.length() / 340.0, ac.burner, ac.crashed);
        let mut ac = Aircraft::on_ground_at(Craft::Fighter, &t, -RWY_HALF_LEN + 20.0, 0.0, 90.0);
        let x0 = ac.pos.x;
        fly(&mut ac, &t, 3.0);
        println!("F16 parked moved {:.2} h {:.2} crashed {:?}", ac.pos.x - x0, ac.pos.y - crate::world::RWY_ELEV, ac.crashed);
        ac.ctl.throttle = 1.0;
        let mut lifted = None;
        for _ in 0..(40 * 240) {
            let ias = ac.ias / KT;
            let target = if ias < 140.0 && ac.on_ground { 0.0 } else if ac.on_ground { 10.0 } else { 12.0 };
            ac.ctl.pitch = ((target - ac.pitch_deg()) * 0.08 - ac.omega.x * 1.0).clamp(-1.0, 1.0);
            ac.ctl.yaw = if ac.on_ground { (-ac.pos.z * 0.1 - (ac.heading_deg() - 90.0) * 0.2).clamp(-1.0, 1.0) } else { 0.0 };
            ac.ctl.roll = (-ac.bank_deg() * 0.03).clamp(-0.5, 0.5);
            ac.step(1.0 / 240.0, &t);
            if ac.crashed.is_some() { println!("takeoff crash {:?}", ac.crashed); break; }
            if lifted.is_none() && !ac.on_ground { lifted = Some((ac.pos.x - x0, ias)); }
        }
        println!("F16 takeoff lifted {:?} climb {:.0} ias {:.0}", lifted, ac.vel.y / FT * 60.0, ac.ias / KT);
        // glider
        for kt in [40.0, 45.0, 50.0, 55.0, 60.0, 70.0, 80.0] {
            let mut ac = Aircraft::in_flight(Craft::Glider, vec3(0.0, 1500.0, 0.0), 90.0, kt, -2.0, 0);
            fly(&mut ac, &t, 20.0);
            let v = ac.vel;
            let ld = vec3(v.x, 0.0, v.z).length() / -v.y;
            println!("glider {kt} kt: L/D {:.1} sink {:.2} m/s ias {:.0} pitch {:.1} crashed {:?}", ld, -v.y, ac.ias / KT, ac.pitch_deg(), ac.crashed);
        }
        let mut ac = Aircraft::on_ground_at(Craft::Glider, &t, -RWY_HALF_LEN + 20.0, 0.0, 90.0);
        fly(&mut ac, &t, 5.0);
        println!("glider parked bank {:.1} crashed {:?}", ac.bank_deg(), ac.crashed);
    }
}
