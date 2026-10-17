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
    offset: Vec3,
    pub eye: Vec3,
    target: Vec3,
    up: Vec3,
    fov: f32,
    near: f32,
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
            offset: vec3(0.0, 4.0, 16.0),
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov: 60.0,
            near: 1.0,
        }
    }

    /// Snap the chase camera behind the aircraft (after a reset).
    pub fn reset(&mut self, ac: &Aircraft) {
        self.offset = ac.rot * vec3(0.0, 0.0, 1.0) * self.dist + Vec3::Y * self.dist * 0.25;
        self.look_yaw = 0.0;
        self.look_pitch = 0.0;
    }

    pub fn update(&mut self, ac: &Aircraft, terrain: &Terrain, dt: f32, drag: Vec2, dragging: bool, wheel: f32) {
        if wheel != 0.0 {
            self.dist = (self.dist * (1.0 - wheel * 0.1)).clamp(8.0, 120.0);
        }
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
                let desired = (behind * self.dist + Vec3::Y * self.dist * 0.22).normalize() * self.dist;
                let desired = Quat::from_rotation_y(self.look_yaw) * desired;
                let side = desired.cross(Vec3::Y).normalize_or_zero();
                let desired = Quat::from_axis_angle(side, -self.look_pitch * 0.8) * desired;
                let k = 1.0 - (-4.0 * dt).exp();
                self.offset = self.offset.lerp(desired, k).normalize_or_zero() * self.dist;
                self.eye = ac.pos + self.offset;
                self.target = ac.pos + ac.forward() * 4.0 + Vec3::Y * 1.0;
                self.up = (Vec3::Y * 0.8 + ac.up() * 0.2).normalize();
                self.fov = 60.0;
                self.near = 1.0;
            }
            CamMode::Cockpit => {
                self.eye = ac.pos + ac.rot * vec3(-0.28, 0.72, -0.78);
                let look = Quat::from_rotation_y(self.look_yaw) * Quat::from_rotation_x(self.look_pitch) * Vec3::NEG_Z;
                self.target = self.eye + ac.rot * look;
                self.up = ac.up();
                self.fov = 72.0;
                self.near = 0.2;
            }
            CamMode::Orbit => {
                if !dragging {
                    self.look_yaw += dt * 0.15;
                }
                let dir = Quat::from_rotation_y(self.look_yaw + self.orbit_yaw)
                    * Quat::from_rotation_x(-(self.look_pitch + self.orbit_pitch).clamp(-1.4, 1.4))
                    * Vec3::Z;
                self.eye = ac.pos + dir * self.dist;
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
                self.fov = (2.0 * (20.0 / d).atan()).to_degrees().clamp(4.0, 60.0);
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

    pub fn view(&self, aspect: f32) -> View {
        let proj = Mat4::perspective_rh_gl(self.fov.to_radians(), aspect, self.near, 45_000.0);
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        View { view_proj: proj * view, eye: self.eye }
    }
}
