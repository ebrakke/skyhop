use crate::flight::Aircraft;
use crate::render::View;
use crate::world::{Terrain, TOWER_POS};
use macroquad::math::{vec3, Mat4, Quat, Vec2, Vec3};

#[derive(Clone, Copy, PartialEq)]
pub enum CamMode {
    Chase,
    Cockpit,
    Orbit,
    Tower,
}

impl CamMode {
    pub fn next(self) -> Self {
        match self {
            CamMode::Chase => CamMode::Cockpit,
            CamMode::Cockpit => CamMode::Orbit,
            CamMode::Orbit => CamMode::Tower,
            CamMode::Tower => CamMode::Chase,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            CamMode::Chase => "Chase",
            CamMode::Cockpit => "Cockpit",
            CamMode::Orbit => "Orbit",
            CamMode::Tower => "Tower",
        }
    }
}

pub struct CameraRig {
    pub mode: CamMode,
    look_yaw: f32,
    look_pitch: f32,
    orbit_yaw: f32,
    orbit_pitch: f32,
    dist: f32,
    /// Cockpit field-of-view multiplier (mouse wheel).
    zoom: f32,
    /// Size of the aircraft relative to the trainer: distances scale with it.
    scale: f32,
    offset: Vec3,
    pub eye: Vec3,
    target: Vec3,
    up: Vec3,
    fov: f32,
    near: f32,
    /// Seconds of camera time, drives the shake noise.
    clock: f32,
    /// Decaying kick from touchdowns and impacts.
    jolt: f32,
    /// Smoothed load factor, for the head sinking under G in the cockpit.
    g: f32,
}

/// Smooth pseudo-random wobble in -1..1 built from incommensurate sines.
fn wobble(t: f32, seed: f32) -> f32 {
    ((t * 13.1 + seed).sin() * 0.5 + (t * 23.7 + seed * 2.3).sin() * 0.3 + (t * 37.3 + seed * 5.1).sin() * 0.2).clamp(-1.0, 1.0)
}

impl CameraRig {
    pub fn new() -> Self {
        CameraRig {
            mode: CamMode::Chase,
            look_yaw: 0.0,
            look_pitch: 0.0,
            orbit_yaw: 0.6,
            orbit_pitch: 0.25,
            dist: 16.0,
            scale: 1.0,
            zoom: 1.0,
            offset: vec3(0.0, 4.0, 16.0),
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov: 60.0,
            near: 1.0,
            clock: 0.0,
            jolt: 0.0,
            g: 1.0,
        }
    }

    /// Give the view a kick (touchdown, crash), 0..1.
    pub fn bump(&mut self, amount: f32) {
        self.jolt = self.jolt.max(amount.clamp(0.0, 1.0));
    }

    /// How much the view should shake right now: runway roll, stall buffet and jolts.
    fn shake_amount(&self, ac: &Aircraft) -> f32 {
        let speed = ac.vel.length();
        let roll = if ac.on_ground { (speed / 30.0).min(1.0) * 0.35 } else { 0.0 };
        let near_stall = ac.alpha - (ac.stall_alpha - 3f32.to_radians());
        let buffet = if ac.on_ground { 0.0 } else { (near_stall / 5f32.to_radians()).clamp(0.0, 1.0) * (speed / 40.0).min(1.0) };
        roll + buffet + self.jolt * 2.0
    }

    /// Snap the chase camera behind the aircraft (after a reset).
    pub fn reset(&mut self, ac: &Aircraft) {
        self.scale = ac.spec.cam_scale;
        let dist = self.dist * self.scale;
        self.offset = ac.rot * vec3(0.0, 0.0, 1.0) * dist + Vec3::Y * dist * 0.25;
        self.look_yaw = 0.0;
        self.look_pitch = 0.0;
    }

    pub fn update(&mut self, ac: &Aircraft, terrain: &Terrain, dt: f32, drag: Vec2, dragging: bool, wheel: f32) {
        self.clock += dt;
        self.jolt *= (-6.0 * dt).exp();
        self.g += (ac.spec_force.y / crate::flight::G - self.g) * (1.0 - (-5.0 * dt).exp());
        let shake = self.shake_amount(ac);
        let t = self.clock;
        let jitter = vec3(wobble(t, 0.0), wobble(t, 1.7), wobble(t, 4.1)) * shake;
        // a gentle push of field-of-view with speed adds a sense of pace
        let fly = ac.spec.fly_kt;
        let pace = ((ac.ias / crate::flight::KT - 3.0 * fly) / (4.5 * fly)).clamp(0.0, 1.0);
        if wheel != 0.0 {
            if self.mode == CamMode::Cockpit {
                // zoom in the cockpit by narrowing the field of view
                self.zoom = (self.zoom * (1.0 - wheel * 0.08)).clamp(0.45, 1.25);
            } else {
                self.dist = (self.dist * (1.0 - wheel * 0.1)).clamp(8.0, 120.0);
            }
        }
        let (dist, scale) = (self.dist * self.scale, self.scale);
        self.look_yaw -= drag.x * 0.006;
        self.look_pitch = (self.look_pitch - drag.y * 0.006).clamp(-1.3, 1.3);
        if !dragging && self.mode != CamMode::Orbit {
            let k = 1.0 - (-3.0 * dt).exp();
            self.look_yaw -= self.look_yaw * k;
            self.look_pitch -= self.look_pitch * k;
        }
        match self.mode {
            CamMode::Chase => {
                let behind = ac.rot * Vec3::Z;
                let desired = (behind * dist + Vec3::Y * dist * 0.22).normalize() * dist;
                let desired = Quat::from_rotation_y(self.look_yaw) * desired;
                let side = desired.cross(Vec3::Y).normalize_or_zero();
                let desired = Quat::from_axis_angle(side, -self.look_pitch * 0.8) * desired;
                let k = 1.0 - (-4.0 * dt).exp();
                self.offset = self.offset.lerp(desired, k).normalize_or_zero() * dist;
                self.eye = ac.pos + self.offset + jitter * 0.08 * scale;
                self.target = ac.pos + (ac.forward() * 4.0 + Vec3::Y * 1.0 + jitter * 0.12) * scale;
                self.up = (Vec3::Y * 0.8 + ac.up() * 0.2).normalize();
                self.fov = 60.0 + 5.0 * pace;
                self.near = 1.0;
            }
            CamMode::Cockpit => {
                // the head sinks a little under G and rides the bumps
                let sink = ((self.g - 1.0) * 0.025).clamp(-0.05, 0.08);
                let seat = ac.spec.eye;
                self.eye = ac.pos + ac.rot * (seat - Vec3::Y * sink + jitter * 0.006);
                // glance a touch into turns, like a pilot looking where they're going
                let lead = (ac.turn_rate_dps() * 0.012).clamp(-0.15, 0.15);
                // the seat looks a few degrees down, so the runway sits above the glareshield
                let down = -ac.spec.look_down.to_radians();
                let look = Quat::from_rotation_y(self.look_yaw - lead + jitter.x * 0.006) * Quat::from_rotation_x(self.look_pitch + down + jitter.y * 0.006) * Vec3::NEG_Z;
                self.target = self.eye + ac.rot * look;
                self.up = ac.up();
                self.fov = (72.0 + 3.0 * pace) * self.zoom;
                self.near = 0.2;
            }
            CamMode::Orbit => {
                if !dragging {
                    self.look_yaw += dt * 0.15;
                }
                let dir = Quat::from_rotation_y(self.look_yaw + self.orbit_yaw)
                    * Quat::from_rotation_x(-(self.look_pitch + self.orbit_pitch).clamp(-1.4, 1.4))
                    * Vec3::Z;
                self.eye = ac.pos + dir * dist;
                self.target = ac.pos;
                self.up = Vec3::Y;
                self.fov = 55.0;
                self.near = 1.0;
            }
            CamMode::Tower => {
                self.eye = TOWER_POS + Vec3::Y * 3.0;
                self.target = ac.pos;
                self.up = Vec3::Y;
                let d = (ac.pos - self.eye).length();
                self.fov = (2.0 * (20.0 * scale / d).atan()).to_degrees().clamp(4.0, 60.0);
                self.near = 2.0;
            }
        }
        // never look from underground
        let g = terrain.surface(self.eye.x, self.eye.z) + 1.5;
        if self.eye.y < g {
            self.eye.y = g;
        }
    }

    /// Slow orbit around the parked plane, framed on the right of the screen.
    pub fn menu(&mut self, t: f32, plane: Vec3) {
        let a = t * 0.08 + 2.2;
        self.eye = plane + vec3(a.cos() * 24.0, 4.5, a.sin() * 24.0);
        let right = (plane - self.eye).normalize().cross(Vec3::Y).normalize();
        self.target = plane - right * 9.0 + Vec3::Y * 1.5;
        self.up = Vec3::Y;
        self.fov = 55.0;
        self.near = 1.0;
    }

    /// Look from a fixed point at a fixed target (for screenshots).
    pub fn set_fixed(&mut self, eye: Vec3, target: Vec3) {
        self.eye = eye;
        self.target = target;
        self.up = Vec3::Y;
        self.fov = 50.0;
        self.near = 1.0;
    }

    pub fn view(&self, aspect: f32) -> View {
        // on very wide windows keep the horizontal view under ~105 degrees
        let max_v = 2.0 * ((105f32.to_radians() * 0.5).tan() / aspect).atan();
        let fov = self.fov.to_radians().min(max_v);
        let proj = Mat4::perspective_rh_gl(fov, aspect, self.near, 45_000.0);
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let forward = (self.target - self.eye).normalize_or_zero();
        let right = forward.cross(self.up).normalize_or_zero();
        View { view_proj: proj * view, eye: self.eye, forward, right, up: right.cross(forward), fov }
    }
}
