//! Skyhop: a small, friendly flight sim for learning the basics of flying.

mod audio;
mod camera;
mod flight;
mod hud;
mod lessons;
mod mesh;
mod render;
mod world;

use camera::{CamMode, CameraRig};
use flight::{Aircraft, Telem, MAX_FLAPS};
use lessons::{Event, Kind, Lesson, Mission, Start};
use macroquad::prelude::*;
use render::{RingDraw, Scene};
use world::*;

const DT: f32 = 1.0 / 240.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "Skyhop".to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        ..Default::default()
    }
}

#[derive(PartialEq)]
enum Screen {
    Menu,
    Flying,
}

#[derive(Default)]
struct Stick {
    pitch: f32,
    roll: f32,
    yaw: f32,
}

struct App {
    world: World,
    renderer: render::Renderer,
    minimap: Texture2D,
    audio: audio::Audio,
    lessons: Vec<Lesson>,
    screen: Screen,
    sel: usize,
    done: Vec<bool>,
    best_ring: Option<f32>,
    idx: usize,
    ac: Aircraft,
    mission: Mission,
    cam: CameraRig,
    stick: Stick,
    auto_rudder: bool,
    acc: f32,
    time: f32,
    paused: bool,
    help: bool,
    full_panel: bool,
    invert: bool,
    summary_dismissed: bool,
    crash_noted: bool,
    toasts: Vec<(String, f32)>,
    last_mouse: Vec2,
    prev_pos: Vec3,
    fps: f32,
}

fn key_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|k| is_key_down(*k))
}

fn pressed_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|k| is_key_pressed(*k))
}

/// Keyboard "stick": eases toward the held direction and springs back to centre.
fn ease(cur: f32, target: f32, dt: f32) -> f32 {
    let rate = if target == 0.0 || target * cur < 0.0 { 4.0 } else { 1.7 };
    cur + (target - cur).clamp(-rate * dt, rate * dt)
}

fn make_minimap(world: &World) -> Texture2D {
    let n = 256u16;
    let mut img = Image::gen_image_color(n, n, BLACK);
    let light = vec3(-0.5, 0.7, -0.5).normalize();
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let x = -HALF + (i as f32 + 0.5) / n as f32 * 2.0 * HALF;
            let z = -HALF + (j as f32 + 0.5) / n as f32 * 2.0 * HALF;
            let h = world.terrain.height(x, z);
            let c = if h < SEA {
                let d = (-h / 45.0).clamp(0.0, 1.0);
                Color::new(0.2 - d * 0.08, 0.45 - d * 0.15, 0.62 - d * 0.12, 1.0)
            } else {
                let shade = 0.6 + 0.5 * world.terrain.normal(x, z).dot(light).max(0.0);
                let t = (h / 750.0).clamp(0.0, 1.0);
                let base = if h < 4.0 {
                    vec3(0.8, 0.75, 0.55)
                } else if t < 0.5 {
                    vec3(0.35, 0.55, 0.25).lerp(vec3(0.5, 0.5, 0.33), t * 2.0)
                } else {
                    vec3(0.5, 0.5, 0.33).lerp(vec3(0.95, 0.95, 0.95), (t - 0.5) * 2.0)
                } * shade;
                Color::new(base.x, base.y, base.z, 1.0)
            };
            img.set_pixel(i, j, c);
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

impl App {
    fn start(&mut self, idx: usize) {
        self.idx = idx;
        self.mission = Mission::default();
        self.ac = match &self.lessons[idx].start {
            Start::Runway => Aircraft::on_ground_at(&self.world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0),
            Start::Air { pos, heading, ias, gamma, flaps, trim_offset } => {
                let mut a = Aircraft::in_flight(*pos, *heading, *ias, *gamma, *flaps);
                a.ctl.trim += trim_offset;
                a
            }
        };
        self.ac.auto_rudder = self.auto_rudder;
        self.stick = Stick::default();
        self.cam.reset(&self.ac);
        self.screen = Screen::Flying;
        self.paused = false;
        self.summary_dismissed = false;
        self.crash_noted = false;
        self.toasts.clear();
        self.prev_pos = self.ac.pos;
        self.acc = 0.0;
    }

    fn to_menu(&mut self) {
        self.screen = Screen::Menu;
        self.sel = self.idx;
        self.paused = false;
        self.ac = Aircraft::on_ground_at(&self.world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
    }

    fn toast(&mut self, s: impl Into<String>) {
        self.toasts.push((s.into(), 2.5));
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    fn controls(&mut self, dt: f32) {
        use KeyCode::*;
        let (mut pull, mut push) = (key_any(&[S, Down]), key_any(&[W, Up]));
        if self.invert {
            std::mem::swap(&mut pull, &mut push);
        }
        let axis = |pos: bool, neg: bool| pos as i32 as f32 - neg as i32 as f32;
        self.stick.pitch = ease(self.stick.pitch, axis(pull, push), dt);
        self.stick.roll = ease(self.stick.roll, axis(key_any(&[D, Right]), key_any(&[A, Left])), dt);
        self.stick.yaw = ease(self.stick.yaw, axis(key_any(&[E]), key_any(&[Q])), dt);
        let c = &mut self.ac.ctl;
        c.pitch = self.stick.pitch;
        c.roll = self.stick.roll;
        c.yaw = self.stick.yaw;
        let thr = axis(key_any(&[LeftShift, RightShift, Equal, KpAdd]), key_any(&[LeftControl, RightControl, Minus, KpSubtract]));
        c.throttle = (c.throttle + thr * 0.5 * dt).clamp(0.0, 1.0);
        let trim = axis(key_any(&[T, RightBracket]), key_any(&[G, LeftBracket]));
        c.trim = (c.trim + trim * 0.15 * dt).clamp(-0.6, 0.6);
        if is_key_pressed(F) && c.flaps < MAX_FLAPS {
            c.flaps += 1;
        }
        if is_key_pressed(V) && c.flaps > 0 {
            c.flaps -= 1;
        }
        c.brake = if key_any(&[B, Space]) { 1.0 } else { 0.0 };
    }

    fn frame(&mut self, dt: f32) -> bool {
        self.time += dt;
        self.fps = self.fps * 0.95 + (1.0 / get_frame_time().max(1e-4)) * 0.05;
        for t in &mut self.toasts {
            t.1 -= dt;
        }
        self.toasts.retain(|t| t.1 > 0.0);

        let mp: Vec2 = mouse_position().into();
        let dragging = is_mouse_button_down(MouseButton::Left) || is_mouse_button_down(MouseButton::Right);
        let drag = if dragging { mp - self.last_mouse } else { Vec2::ZERO };
        self.last_mouse = mp;
        let wheel = mouse_wheel().1;
        let wheel = if wheel.abs() > 0.0 { wheel.signum() } else { 0.0 };

        if is_key_pressed(KeyCode::H) {
            self.help = !self.help;
        }
        if is_key_pressed(KeyCode::M) {
            self.audio.muted = !self.audio.muted;
        }

        if self.screen == Screen::Menu {
            if !self.help {
                let n = self.lessons.len() + 1;
                if pressed_any(&[KeyCode::Down, KeyCode::S]) {
                    self.sel = (self.sel + 1) % n;
                }
                if pressed_any(&[KeyCode::Up, KeyCode::W]) {
                    self.sel = (self.sel + n - 1) % n;
                }
                if pressed_any(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]) {
                    if self.sel == self.lessons.len() {
                        return false;
                    }
                    self.start(self.sel);
                }
                if is_key_pressed(KeyCode::Escape) {
                    return false;
                }
            } else if is_key_pressed(KeyCode::Escape) {
                self.help = false;
            }
            self.cam.menu(self.time, self.ac.pos);
            self.audio.update(None, 0.0, false, dt);
            self.draw_world();
            hud::menu(&self.lessons, self.sel, &self.done, self.best_ring, self.time);
            if self.help {
                hud::help_overlay();
            }
            return true;
        }

        // ---- flying ----
        let crashed = self.ac.crashed.is_some();
        let complete = self.mission.complete && !self.summary_dismissed;
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) {
            if self.help {
                self.help = false;
            } else if crashed || complete {
                self.to_menu();
                return true;
            } else {
                self.paused = !self.paused;
            }
        }
        if self.paused && is_key_pressed(KeyCode::Backspace) {
            self.to_menu();
            return true;
        }
        if is_key_pressed(KeyCode::R) || (crashed && pressed_any(&[KeyCode::Enter, KeyCode::KpEnter])) {
            self.start(self.idx);
            return true;
        }
        if complete {
            if pressed_any(&[KeyCode::Enter, KeyCode::KpEnter]) {
                self.summary_dismissed = true;
            }
            if is_key_pressed(KeyCode::N) && self.idx + 1 < self.lessons.len() {
                self.start(self.idx + 1);
                self.sel = self.idx;
                return true;
            }
        } else if is_key_pressed(KeyCode::N) {
            self.mission.skip_step(&self.lessons[self.idx]);
        }
        if is_key_pressed(KeyCode::C) {
            self.cam.mode = self.cam.mode.next();
            self.cam.reset(&self.ac);
            self.toast(format!("{} view", self.cam.mode.name()));
        }
        if is_key_pressed(KeyCode::Tab) {
            self.full_panel = !self.full_panel;
        }
        if is_key_pressed(KeyCode::Y) {
            self.auto_rudder = !self.auto_rudder;
            self.ac.auto_rudder = self.auto_rudder;
            self.toast(if self.auto_rudder { "Auto-rudder ON" } else { "Auto-rudder OFF" });
        }
        if is_key_pressed(KeyCode::I) {
            self.invert = !self.invert;
            self.toast(if self.invert { "Pitch keys inverted" } else { "Pitch keys normal" });
        }

        let running = !self.paused && !self.help && !crashed;
        if running {
            self.controls(dt);
            self.acc += dt;
            while self.acc >= DT {
                self.ac.step(DT, &self.world.terrain);
                self.acc -= DT;
            }
        }
        if self.ac.touchdown_event {
            self.ac.touchdown_event = false;
            self.audio.squeak();
            if let Some(td) = self.ac.touchdown {
                self.toast(format!("Touchdown: {:.0} fpm", td.sink_fpm));
            }
        }
        if self.ac.crashed.is_some() && !self.crash_noted {
            self.crash_noted = true;
            self.audio.crash();
        }

        let t = self.ac.telem(&self.world.terrain);
        if running {
            let lesson = &self.lessons[self.idx];
            let ev = self.mission.update(lesson, &t, self.prev_pos, &self.world, self.ac.touchdown, &mut self.best_ring, dt);
            match ev {
                Some(Event::StepDone) => {
                    self.audio.ding();
                    self.toast("Nice!");
                }
                Some(Event::LessonDone) => {
                    self.audio.chime();
                    self.done[self.idx] = true;
                }
                Some(Event::Ring) => {
                    self.audio.ding();
                    let n = self.mission.next_ring;
                    self.toast(format!("Ring {n}/{}", self.world.rings.len()));
                }
                Some(Event::RingsDone) => self.audio.chime(),
                None => {}
            }
        }
        self.prev_pos = self.ac.pos;

        self.cam.update(&self.ac, &self.world.terrain, dt, drag, dragging, wheel);
        let engine = if running { Some(self.ac.ctl.throttle) } else { None };
        self.audio.update(engine, t.ias_kt, t.stall_warn && running, dt);

        self.draw_world();
        self.draw_hud(&t);
        true
    }

    fn draw_world(&mut self) {
        let lesson = &self.lessons[self.idx];
        let mut rings = Vec::new();
        if self.screen == Screen::Flying && lesson.kind == Kind::Rings {
            for (i, r) in self.world.rings.iter().enumerate().skip(self.mission.next_ring) {
                let color = if i == self.mission.next_ring { [1.0, 0.78, 0.15, 1.0] } else { [0.35, 0.72, 1.0, 1.0] };
                rings.push(RingDraw { pos: r.pos, normal: r.normal, radius: r.radius, color });
            }
        }
        let a09 = lessons::glide_angle(self.ac.pos);
        let x27 = RWY_HALF_LEN - PAPI_DIST;
        let a27 = (self.ac.pos.y - 1.4 - RWY_ELEV).atan2((self.ac.pos.x - x27).max(1.0)).to_degrees();
        let mut papi = Vec::new();
        for (p, thr) in lessons::papi_lights() {
            papi.push((p, a09 > thr));
            papi.push((vec3(x27, p.y, -p.z), a27 > thr));
        }
        let ground = self.world.terrain.surface(self.ac.pos.x, self.ac.pos.z);
        let agl = self.ac.pos.y - ground;
        let scene = Scene {
            ac: &self.ac,
            show_fuselage: self.cam.mode != CamMode::Cockpit || self.screen == Screen::Menu,
            rings,
            papi,
            time: self.time,
            shadow_alpha: 0.42 * (1.0 - agl / 250.0).clamp(0.0, 1.0),
            shadow_ground: ground,
        };
        let view = self.cam.view(screen_width() / screen_height());
        let mut gl = unsafe { get_internal_gl() };
        gl.flush();
        self.renderer.draw(gl.quad_context, &view, &scene);
    }

    fn draw_hud(&mut self, t: &Telem) {
        let lesson = &self.lessons[self.idx];
        let sh = screen_height();
        hud::cloud_fade(self.world.cloud_density(self.cam.eye));
        let view = self.cam.view(screen_width() / screen_height());
        let panel_top = hud::panel(t, &hud::PanelInfo { brake: self.ac.ctl.brake > 0.0, auto_rudder: self.auto_rudder }, self.full_panel);
        if lesson.kind == Kind::Rings && !self.mission.complete {
            let r = &self.world.rings[self.mission.next_ring];
            hud::ring_marker(&view.view_proj, r.pos, view.eye, panel_top);
            hud::rings_status(&self.mission, self.world.rings.len(), self.best_ring);
        }
        hud::lesson_panel(lesson, &self.mission, t);
        let next_ring = (lesson.kind == Kind::Rings && !self.mission.complete).then_some(self.mission.next_ring);
        hud::minimap(&self.minimap, &self.world, t, next_ring);
        hud::status_line(self.cam.mode.name(), self.fps.round() as i32, self.auto_rudder);
        hud::papi_widget(t, panel_top - 40.0);
        hud::warnings(t, self.ac.warning.map(|w| w.0), self.time, sh * 0.2);
        hud::toasts(&self.toasts);

        if let Some(reason) = self.ac.crashed {
            hud::dialog("CRASHED", hud::RED_C, &[reason.to_string()], "R / ENTER  try again      ESC  menu");
        } else if self.mission.complete && !self.summary_dismissed {
            let title = if lesson.kind == Kind::Rings { "COURSE COMPLETE" } else { "LESSON COMPLETE" };
            let footer = if self.idx + 1 < self.lessons.len() { "ENTER keep flying    N next    R retry    ESC menu" } else { "ENTER keep flying    R retry    ESC menu" };
            hud::dialog(title, hud::GREEN_C, &self.mission.summary, footer);
        } else if self.paused {
            hud::dialog("PAUSED", hud::AMBER, &["Take a breather.".to_string()], "ESC / P  resume    BACKSPACE  main menu    H  help");
        }
        if self.help {
            hud::help_overlay();
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    // Optional: --shot <file.png> <scene> [camera] renders a few seconds and saves a screenshot.
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--shot").map(|i| (args.get(i + 1).cloned(), args.get(i + 2).cloned(), args.get(i + 3).cloned()));

    let world = World::generate();
    let renderer = {
        let gl = unsafe { get_internal_gl() };
        render::Renderer::new(gl.quad_context, &world)
    };
    let minimap = make_minimap(&world);
    let audio = audio::Audio::new().await;
    let lessons = lessons::all();
    let n = lessons.len();
    let ac = Aircraft::on_ground_at(&world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
    let mut app = App {
        world,
        renderer,
        minimap,
        audio,
        lessons,
        screen: Screen::Menu,
        sel: 0,
        done: vec![false; n],
        best_ring: None,
        idx: 0,
        ac,
        mission: Mission::default(),
        cam: CameraRig::new(),
        stick: Stick::default(),
        auto_rudder: false,
        acc: 0.0,
        time: 0.0,
        paused: false,
        help: false,
        full_panel: true,
        invert: false,
        summary_dismissed: false,
        crash_noted: false,
        toasts: Vec::new(),
        last_mouse: Vec2::ZERO,
        prev_pos: Vec3::ZERO,
        fps: 60.0,
    };

    let mut frames = 0;
    if let Some((_, Some(scene), cam)) = &shot {
        app.audio.muted = true;
        if let Ok(i) = scene.parse::<usize>() {
            app.start(i);
        }
        app.cam.mode = match cam.as_deref() {
            Some("cockpit") => CamMode::Cockpit,
            Some("orbit") => CamMode::Orbit,
            Some("tower") => CamMode::Tower,
            _ => CamMode::Chase,
        };
        app.cam.reset(&app.ac);
    }

    loop {
        let dt = get_frame_time().min(0.1);
        if !app.frame(dt) {
            break;
        }
        frames += 1;
        if let Some((Some(path), _, _)) = &shot {
            if frames == 150 {
                get_screen_data().export_png(path);
                break;
            }
        }
        next_frame().await;
    }
}
