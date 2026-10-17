//! Rigid-body flight model of a small high-wing trainer (roughly a Cessna 172).
//!
//! Body axes: +x right wing, +y up, +z toward the tail (nose points along -z).
//! `omega` is the body angular velocity in those axes, so pitch-up is +x,
//! yaw-right is -y and roll-right is -z.

use crate::world::{smoothstep, Rng, Terrain, SEA};
use macroquad::math::{vec3, Quat, Vec3};

pub const G: f32 = 9.81;
pub const KT: f32 = 0.514_444; // m/s per knot
pub const FT: f32 = 0.3048; // m per foot

const MASS: f32 = 1000.0;
const INERTIA: Vec3 = Vec3::new(1350.0, 1970.0, 950.0); // pitch, yaw, roll
const S: f32 = 16.2; // wing area m^2
const SPAN: f32 = 11.0;
const CHORD: f32 = 1.5;
const CL0: f32 = 0.25;
const CLA: f32 = 4.6; // per radian
const CD0: f32 = 0.032;
const OSWALD: f32 = 0.75;
const CM0: f32 = 0.04;
const CMA: f32 = -1.8;
const CMQ: f32 = -12.0;
const CMDE: f32 = 1.2;
const CM_FLAPS: f32 = -0.05;
const POWER: f32 = 80_000.0; // W delivered by the prop at full throttle

pub const ELEV_MAX: f32 = 0.42;
pub const AIL_MAX: f32 = 0.35;
pub const RUD_MAX: f32 = 0.40;
pub const FLAP_NOTCH_DEG: f32 = 10.0;
pub const MAX_FLAPS: u8 = 3;
pub const VNE_KT: f32 = 163.0;

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub pitch: f32, // +1 full back (nose up)
    pub roll: f32,  // +1 full right
    pub yaw: f32,   // +1 full right rudder
    pub throttle: f32,
    pub flaps: u8,
    pub trim: f32, // added to pitch, -0.6..0.6
    pub brake: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Pt {
    Nose,
    Main,
    Tail,
    Wingtip,
    Prop,
    Belly,
    Top,
}

const POINTS: [(Vec3, Pt); 11] = [
    (Vec3::new(0.0, -1.40, -2.0), Pt::Nose),
    (Vec3::new(-1.25, -1.45, 0.35), Pt::Main),
    (Vec3::new(1.25, -1.45, 0.35), Pt::Main),
    (Vec3::new(0.0, 0.05, 4.6), Pt::Tail),
    (Vec3::new(-5.55, 1.0, -0.3), Pt::Wingtip),
    (Vec3::new(5.55, 1.0, -0.3), Pt::Wingtip),
    (Vec3::new(0.0, -1.0, -2.75), Pt::Prop),
    (Vec3::new(0.0, -0.62, -1.0), Pt::Belly),
    (Vec3::new(0.0, -0.55, 1.2), Pt::Belly),
    (Vec3::new(0.0, 1.08, 0.0), Pt::Top),
    (Vec3::new(0.0, 1.75, 4.2), Pt::Top),
];

#[derive(Clone, Copy)]
pub struct Touchdown {
    pub sink_fpm: f32,
    pub pos: Vec3,
    pub ias_kt: f32,
}

pub struct Aircraft {
    pub pos: Vec3,
    pub vel: Vec3,
    pub rot: Quat,
    pub omega: Vec3,
    pub ctl: Controls,
    pub auto_rudder: bool,

    // actual surface positions (radians / degrees)
    pub elev: f32,
    pub ail: f32,
    pub rud: f32,
    pub flap_deg: f32,
    pub prop_angle: f32,

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
    pub stall_asym: f32,
    touching: [bool; POINTS.len()],
    rng: Rng,
}

pub fn density(alt_m: f32) -> f32 {
    1.225 * (-alt_m.max(0.0) / 8500.0).exp()
}

fn approach(cur: f32, target: f32, max_step: f32) -> f32 {
    cur + (target - cur).clamp(-max_step, max_step)
}

fn stall_alpha(flap_frac: f32) -> f32 {
    (16.0 - 3.0 * flap_frac).to_radians()
}

/// Lift coefficient and how deeply the wing is stalled (0..1).
fn lift_coeff(alpha: f32, flap_frac: f32) -> (f32, f32) {
    let cl0 = CL0 + 0.6 * flap_frac;
    let a_pos = stall_alpha(flap_frac);
    let a_neg = (-13.0f32).to_radians();
    let plate = 1.1 * (2.0 * alpha).sin();
    if alpha > a_pos {
        let peak = cl0 + CLA * a_pos;
        let x = alpha - a_pos;
        let post = peak * (0.62 + 0.38 * (-x / 0.06).exp());
        let w = smoothstep(0.35, 0.8, alpha);
        (post * (1.0 - w) + plate * w, (x / 0.05).min(1.0))
    } else if alpha < a_neg {
        let peak = cl0 + CLA * a_neg;
        let x = a_neg - alpha;
        let post = peak * (0.62 + 0.38 * (-x / 0.06).exp());
        let w = smoothstep(0.35, 0.8, -alpha);
        (post * (1.0 - w) + plate * w, (x / 0.05).min(1.0))
    } else {
        (cl0 + CLA * alpha, 0.0)
    }
}

fn drag_coeff(alpha: f32, cl: f32, flap_frac: f32, ground_effect: f32) -> f32 {
    let ar = SPAN * SPAN / S;
    let induced = cl * cl / (std::f32::consts::PI * OSWALD * ar) * ground_effect;
    let a_s = stall_alpha(flap_frac).sin();
    let separated = 1.2 * (alpha.sin().powi(2) - a_s * a_s).max(0.0);
    CD0 + 0.05 * flap_frac + induced + separated
}

fn thrust(throttle: f32, rho: f32, v_fwd: f32) -> f32 {
    let power = POWER * ((rho / 1.225 - 0.1) / 0.9).max(0.0);
    (0.04 + 0.96 * throttle) * power / v_fwd.max(30.0)
}

impl Aircraft {
    fn blank() -> Self {
        Aircraft {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: Quat::IDENTITY,
            omega: Vec3::ZERO,
            ctl: Controls::default(),
            auto_rudder: false,
            elev: 0.0,
            ail: 0.0,
            rud: 0.0,
            flap_deg: 0.0,
            prop_angle: 0.0,
            alpha: 0.0,
            beta: 0.0,
            stall_alpha: stall_alpha(0.0),
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
            stall_asym: 0.0,
            touching: [false; POINTS.len()],
            rng: Rng::new(7),
        }
    }

    pub fn heading_rot(heading_deg: f32) -> Quat {
        Quat::from_rotation_y(-heading_deg.to_radians())
    }

    /// Parked on the ground at (x, z) facing `heading_deg`.
    pub fn on_ground_at(terrain: &Terrain, x: f32, z: f32, heading_deg: f32) -> Self {
        let mut a = Self::blank();
        a.pos = vec3(x, terrain.height(x, z) + 1.38, z);
        a.rot = Self::heading_rot(heading_deg);
        a.on_ground = true;
        a.touching[1] = true;
        a.touching[2] = true;
        a.touching[0] = true;
        a
    }

    /// In steady, trimmed flight: speed in knots (indicated), flight path angle in degrees.
    pub fn in_flight(pos: Vec3, heading_deg: f32, ias_kt: f32, gamma_deg: f32, flaps: u8) -> Self {
        let mut a = Self::blank();
        let rho = density(pos.y);
        let v = ias_kt * KT / (rho / 1.225).sqrt();
        let gamma = gamma_deg.to_radians();
        let ff = flaps as f32 * FLAP_NOTCH_DEG / 30.0;
        let qbar = 0.5 * rho * v * v;
        let cl = MASS * G * gamma.cos() / (qbar * S);
        let alpha = (cl - CL0 - 0.6 * ff) / CLA;
        let cd = drag_coeff(alpha, cl, ff, 1.0);
        let t_needed = qbar * S * cd + MASS * G * gamma.sin();
        let power = POWER * ((rho / 1.225 - 0.1) / 0.9).max(0.0);
        let throttle = ((t_needed * v.max(30.0) / power - 0.04) / 0.96).clamp(0.0, 1.0);
        let elev = -(CM0 + CMA * alpha + CM_FLAPS * ff) / CMDE;

        let hdg = Self::heading_rot(heading_deg);
        a.pos = pos;
        a.rot = hdg * Quat::from_rotation_x(gamma + alpha);
        a.vel = hdg * vec3(0.0, gamma.sin(), -gamma.cos()) * v;
        a.ctl.throttle = throttle;
        a.ctl.flaps = flaps;
        a.ctl.trim = (elev / ELEV_MAX).clamp(-0.6, 0.6);
        a.elev = elev;
        a.flap_deg = flaps as f32 * FLAP_NOTCH_DEG;
        a.alpha = alpha;
        a.ias = ias_kt * KT;
        a.airborne_time = 10.0;
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
    /// Rate of heading change, deg/s, positive to the right.
    pub fn turn_rate_dps(&self) -> f32 {
        -(self.rot * self.omega).y.to_degrees()
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

        let rho = density(self.pos.y);
        let vb = self.rot.inverse() * self.vel;
        let v = vb.length();
        let (alpha, beta) = if v > 0.5 { ((-vb.y).atan2(-vb.z), (vb.x / v).clamp(-1.0, 1.0).asin()) } else { (0.0, 0.0) };
        self.alpha = alpha;
        self.beta = beta;
        self.ias = v * (rho / 1.225).sqrt();
        let flying = !self.on_ground && self.ias > 20.0 * KT;

        // --- control surfaces (actuators move at a finite rate) ---
        let mut rud_cmd = self.ctl.yaw;
        if self.auto_rudder && !self.on_ground {
            rud_cmd += self.ctl.roll * 0.25 + beta * 5.0;
        }
        let elev_cmd = (self.ctl.pitch + self.ctl.trim).clamp(-1.0, 1.0) * ELEV_MAX;
        self.elev = approach(self.elev, elev_cmd, 2.5 * dt);
        self.ail = approach(self.ail, self.ctl.roll.clamp(-1.0, 1.0) * AIL_MAX, 2.5 * dt);
        self.rud = approach(self.rud, rud_cmd.clamp(-1.0, 1.0) * RUD_MAX, 2.5 * dt);
        self.flap_deg = approach(self.flap_deg, self.ctl.flaps as f32 * FLAP_NOTCH_DEG, 6.0 * dt);
        let ff = self.flap_deg / 30.0;
        self.stall_alpha = stall_alpha(ff);
        self.prop_angle = (self.prop_angle + dt * (25.0 + 200.0 * self.ctl.throttle + v)) % std::f32::consts::TAU;

        // --- aerodynamics ---
        let ground_h = terrain.surface(self.pos.x, self.pos.z);
        let wing_agl = (self.pos.y + 1.0 - ground_h).max(0.1);
        let hb = wing_agl / SPAN;
        let ground_effect = (16.0 * hb * hb) / (1.0 + 16.0 * hb * hb);

        let (cl, stall_depth) = lift_coeff(alpha, ff);
        let cd = drag_coeff(alpha, cl, ff, ground_effect);
        let qbar = 0.5 * rho * v * v;
        let right = self.right();
        let fwd = self.forward();

        let mut force = vec3(0.0, -MASS * G, 0.0);
        if v > 0.5 {
            let vhat = self.vel / v;
            let lift_dir = right.cross(vhat).try_normalize().unwrap_or(self.up());
            let cy = -0.5 * beta;
            force += lift_dir * (qbar * S * cl) - vhat * (qbar * S * cd) + right * (qbar * S * cy);
        }
        force += fwd * thrust(self.ctl.throttle, rho, -vb.z);

        // Moments in conventional form: L roll (right wing down +), M pitch (nose up +), N yaw (nose right +)
        let veff = v.max(12.0);
        let (p, q, r) = (-self.omega.z, self.omega.x, -self.omega.y);
        let ph = p * SPAN / (2.0 * veff);
        let qh = q * CHORD / (2.0 * veff);
        let rh = r * SPAN / (2.0 * veff);
        let prop_wash = 20.0 * self.ctl.throttle;
        let qbar_tail = qbar + 0.5 * rho * prop_wash * prop_wash;

        if stall_depth > 0.0 && flying {
            if !self.stalled {
                self.stall_asym = if self.rng.f() < 0.5 { -1.0 } else { 1.0 } * self.rng.range(0.008, 0.02);
            }
        }
        let a_s = self.stall_alpha;
        self.stalled = flying && if self.stalled { alpha > a_s - 2f32.to_radians() } else { alpha > a_s + 0.5f32.to_radians() };
        self.stall_warn = flying && alpha > a_s - 5f32.to_radians();

        let roll_damp = -0.47 + 0.5 * stall_depth;
        let ail_eff = 1.0 - 0.5 * stall_depth;
        let cl_roll = -0.09 * beta + roll_damp * ph + 0.1 * rh + 0.15 * self.ail * ail_eff
            + stall_depth * (self.stall_asym - 0.25 * beta);
        let post_stall = (alpha.abs() - a_s).max(0.0) * alpha.signum();
        let cm = CM0 + CMA * alpha.clamp(-0.5, 0.5) + CMQ * qh + CM_FLAPS * ff - 0.6 * post_stall;
        let cn = 0.09 * beta - 0.1 * rh - 0.02 * ph - 0.008 * self.ail;
        let p_factor = -180.0 * self.ctl.throttle * (1.0 - v / 40.0).max(0.0);

        let roll_m = qbar * S * SPAN * cl_roll;
        let pitch_m = qbar * S * CHORD * cm + qbar_tail * S * CHORD * CMDE * self.elev;
        let yaw_m = qbar * S * SPAN * cn + qbar_tail * S * SPAN * 0.07 * self.rud + p_factor;
        let mut torque_b = vec3(pitch_m, -yaw_m, -roll_m);

        // --- ground contact ---
        let omega_w = self.rot * self.omega;
        let mut torque_w = Vec3::ZERO;
        let mut touching = [false; POINTS.len()];
        let mut wheels_down = false;
        for (k, (bp, kind)) in POINTS.iter().enumerate() {
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
                    if first && self.airborne_time > 0.5 {
                        if -vn > 3.8 {
                            self.crashed = Some("Hard landing - the gear collapsed. Flare to slow the descent just before touchdown.");
                            return;
                        }
                        if *kind == Pt::Main && self.touchdown_event == false && !self.touching[1] && !self.touching[2] {
                            self.touchdown = Some(Touchdown { sink_fpm: -vn / FT * 60.0, pos: self.pos, ias_kt: self.ias / KT });
                            self.touchdown_event = true;
                        }
                        if *kind == Pt::Nose && !self.touching[1] && !self.touching[2] && -vn > 1.5 {
                            self.warning = Some(("Nose wheel first! Raise the nose to land on the mains.", 3.0));
                        }
                    }
                    wheels_down = true;
                    let (k_spring, damp) = if *kind == Pt::Nose { (45_000.0, 4_000.0) } else { (60_000.0, 5_000.0) };
                    let fn_ = (k_spring * pen - damp * vn).max(0.0);
                    let fwd_g = (fwd - n * fwd.dot(n)).normalize_or_zero();
                    let mut dir = fwd_g;
                    if *kind == Pt::Nose {
                        let steer = self.ctl.yaw * 0.4 * (1.0 - v / 35.0).clamp(0.15, 1.0);
                        dir = Quat::from_axis_angle(n, -steer) * fwd_g;
                    }
                    let side = dir.cross(n);
                    let mu_long = 0.025 + if *kind == Pt::Main { 0.7 * self.ctl.brake } else { 0.0 };
                    let f_long = -mu_long * fn_ * (vp.dot(dir) / 0.3).clamp(-1.0, 1.0);
                    let f_lat = -0.85 * fn_ * (vp.dot(side) / 0.25).clamp(-1.0, 1.0);
                    let f = n * fn_ + dir * f_long + side * f_lat;
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Tail => {
                    if -vn > 2.0 {
                        self.crashed = Some("Tail strike - the tail hit the ground hard. Don't over-rotate.");
                        return;
                    }
                    if first {
                        self.warning = Some(("Tail strike! Easy on the back pressure.", 3.0));
                    }
                    let fn_ = (80_000.0 * pen - 6_000.0 * vn).max(0.0);
                    let tang = vp - n * vn;
                    let f = n * fn_ - tang.normalize_or_zero() * (0.5 * fn_).min(tang.length() * 4000.0);
                    force += f;
                    torque_w += r.cross(f);
                }
                Pt::Wingtip => {
                    self.crashed = Some(if speed > 25.0 { "A wingtip hit the ground. Keep the wings level near the ground." } else { "Wingtip strike while taxiing or landing - keep the wings level." });
                    return;
                }
                Pt::Prop => {
                    self.crashed = Some(if speed > 30.0 && -vn > 3.0 { "Flew into the ground. Watch your altitude and descent rate!" } else { "Prop strike! The nose hit the ground - land on the main wheels first." });
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
        let accel = force / MASS;
        self.spec_force = self.rot.inverse() * (accel + vec3(0.0, G, 0.0));
        self.vel += accel * dt;
        self.pos += self.vel * dt;
        let iw = INERTIA * self.omega;
        let domega = (torque_b - self.omega.cross(iw)) / INERTIA;
        self.omega += domega * dt;
        self.rot = (self.rot * Quat::from_scaled_axis(self.omega * dt)).normalize();

        // --- limits ---
        let g = self.spec_force.y / G;
        if self.ias / KT > 200.0 {
            self.crashed = Some("Exceeded the never-exceed speed (Vne) - the airframe broke up. Watch the red line!");
        } else if !(-3.5..=6.0).contains(&g) && !self.on_ground {
            self.crashed = Some("Overstressed the airframe - pulled too many G. Smooth inputs at high speed!");
        }
    }
}

/// A snapshot of the aircraft in pilot units, for the instruments and lessons.
#[derive(Clone, Copy, Default)]
pub struct Telem {
    pub ias_kt: f32,
    pub gs_kt: f32,
    pub alt_ft: f32,
    pub agl_ft: f32,
    pub vs_fpm: f32,
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
    pub flaps: u8,
    pub flap_deg: f32,
    pub trim: f32,
    pub stick_pitch: f32,
    pub stick_roll: f32,
    pub on_ground: bool,
    pub stalled: bool,
    pub stall_warn: bool,
    pub pos: Vec3,
}

impl Aircraft {
    pub fn telem(&self, terrain: &Terrain) -> Telem {
        let ground = terrain.surface(self.pos.x, self.pos.z);
        Telem {
            ias_kt: self.ias / KT,
            gs_kt: vec3(self.vel.x, 0.0, self.vel.z).length() / KT,
            alt_ft: self.pos.y / FT,
            agl_ft: (self.pos.y - 1.38 - ground).max(0.0) / FT,
            vs_fpm: self.vel.y / FT * 60.0,
            hdg: self.heading_deg(),
            pitch: self.pitch_deg(),
            bank: self.bank_deg(),
            aoa: self.alpha.to_degrees(),
            stall_aoa: self.stall_alpha.to_degrees(),
            slip: (-self.spec_force.x / G * 4.0).clamp(-1.0, 1.0),
            turn_rate: self.turn_rate_dps(),
            g: self.spec_force.y / G,
            throttle: self.ctl.throttle,
            rpm: (700.0 + 1900.0 * self.ctl.throttle + self.ias * 6.0).min(2750.0),
            flaps: self.ctl.flaps,
            flap_deg: self.flap_deg,
            trim: self.ctl.trim,
            stick_pitch: self.ctl.pitch,
            stick_roll: self.ctl.roll,
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
        let mut ac = Aircraft::in_flight(vec3(0.0, 900.0, 0.0), 90.0, 100.0, 0.0, 0);
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
        let mut ac = Aircraft::in_flight(vec3(-3000.0, 300.0, 0.0), 90.0, 70.0, -3.0, 2);
        fly(&mut ac, &t, 15.0);
        assert!(ac.crashed.is_none());
        let vs = ac.vel.y / FT * 60.0;
        assert!((-650.0..-150.0).contains(&vs), "vs {vs}");
    }

    #[test]
    fn parked_plane_stays_put() {
        let t = Terrain::generate();
        let mut ac = Aircraft::on_ground_at(&t, -RWY_HALF_LEN + 30.0, 0.0, 90.0);
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
        let mut ac = Aircraft::on_ground_at(&t, -RWY_HALF_LEN + 30.0, 0.0, 90.0);
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
    fn stall_happens_and_recovers() {
        let t = Terrain::generate();
        let mut ac = Aircraft::in_flight(vec3(0.0, 1400.0, 0.0), 0.0, 85.0, 0.0, 0);
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

