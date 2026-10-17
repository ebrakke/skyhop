//! Skyhop: a small, friendly flight sim for learning the basics of flying.

mod airliner;
mod ap;
mod debrief;
mod ghost;
mod progress;
mod atc;
mod audio;
mod camera;
mod fighter;
mod flight;
mod glider;
mod gpws;
mod ground;
mod hud;
mod input;
mod kids;
mod music;
mod toys;
mod lessons;
mod lights;
mod mesh;
mod render;
mod replay;
mod scenery;
mod traffic;
mod score;
mod setup;
mod weather;
mod world;

use camera::{CamMode, CameraRig};
use flight::{Aircraft, Craft, Telem};
use input::{Axis as PadAxis, Button as PadBtn, Pad};
use lessons::{Event, Kind, Lesson, Mission, Start};
use macroquad::prelude::*;
use render::{Puff, RingDraw, Scene};
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
    /// Free-flight setup: aircraft, start, time and weather.
    Setup,
    Flying,
}

/// What a main-menu row does.
#[derive(Clone, Copy)]
enum MenuAct {
    Lesson(usize),
    Objectives,
    Setup,
    Kids,
    Red,
}

/// The main menu has two entries; Objectives opens the list of lessons and challenges.
#[derive(Clone, Copy, PartialEq)]
enum MenuPage {
    Main,
    Objectives,
}

#[derive(Default)]
struct Stick {
    /// How far the pitch keys have been "leaned on"; the elevator gets its cube.
    pitch_hold: f32,
    roll: f32,
    yaw: f32,
    /// Seconds the trim input has been held; trim starts fine and speeds up.
    trim_hold: f32,
    /// Seconds the throttle has been held back at idle (keep holding for reverse thrust).
    idle_hold: f32,
}

struct App {
    world: World,
    renderer: render::Renderer,
    minimap: Texture2D,
    audio: audio::Audio,
    lessons: Vec<Lesson>,
    screen: Screen,
    /// Selected row of the main menu (or of the setup screen).
    sel: usize,
    menu_page: MenuPage,
    setup_sel: usize,
    profile: setup::Profile,
    free: setup::FreeSetup,
    best_ring: Option<f32>,
    idx: usize,
    ac: Aircraft,
    mission: Mission,
    cam: CameraRig,
    stick: Stick,
    pad: Pad,
    auto_rudder: bool,
    acc: f32,
    time: f32,
    paused: bool,
    help: bool,
    panel: hud::PanelMode,
    invert: bool,
    summary_dismissed: bool,
    crash_noted: bool,
    toasts: Vec<(String, f32)>,
    last_mouse: Vec2,
    prev_pos: Vec3,
    prev_eye: Vec3,
    fps: f32,
    smoke: Vec<Smoke>,
    gpws: gpws::Gpws,
    weather: weather::Weather,
    /// Seconds of simulated flight, for gusts and turbulence.
    sim_time: f32,
    ap: ap::Autopilot,
    /// The autopilot window under the mouse.
    mcp_hover: Option<hud::McpField>,
    /// Seconds the current autopilot bug key has been held (holding speeds it up).
    bug_hold: f32,
    /// Head-up display in the cockpit view (U).
    hud_on: bool,
    /// Lesson coach shrunk to just the current step (O).
    coach_small: bool,
    landing: score::LandingLog,
    recorder: replay::Recorder,
    replay: Option<replay::Replay>,
    /// Simulation time of the first touchdown (replays start a little before it).
    touch_time: Option<f32>,
    /// A fixed camera (eye, target) for screenshots.
    shot_cam: Option<(Vec3, Vec3)>,
    tower: atc::Tower,
    traffic: traffic::Traffic,
    /// Traffic flies in free flight only.
    traffic_on: bool,
    /// This attempt, recorded in case it's a new best.
    run: ghost::Run,
    /// The best attempt so far on this challenge, flown alongside.
    ghost: Option<ghost::Run>,
    ghost_ac: Option<Aircraft>,
    show_ghost: bool,
    ghost_visible: bool,
    /// Simulation time when the attempt finished.
    done_at: f32,
    approach: debrief::ApproachLog,
    debrief: Option<debrief::Debrief>,
    /// Stars earned on the attempt just finished, out of how many.
    result_stars: Option<(u32, u32)>,
    /// Kids mode, when that's what's flying.
    kids: Option<kids::Kids>,
    /// 99 Red Balloons' music, loaded the first time it's needed.
    song: Option<std::sync::Arc<music::Song>>,
}

/// A puff of tire smoke or dust drifting and spreading out.
struct Smoke {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    color: [f32; 3],
    /// How thick it starts out (most smoke is 0.55).
    opacity: f32,
}

fn key_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|k| is_key_down(*k))
}

fn pressed_any(keys: &[KeyCode]) -> bool {
    keys.iter().any(|k| is_key_pressed(*k))
}

/// Keyboard "stick": eases toward the held direction at `rise` per second and springs back to centre.
fn ease(cur: f32, target: f32, rise: f32, dt: f32) -> f32 {
    let rate = if target == 0.0 || target * cur < 0.0 { 4.0 } else { rise };
    cur + (target - cur).clamp(-rate * dt, rate * dt)
}

/// Stick shaping: soft around centre for fine corrections, full travel at the edges.
fn expo(x: f32, amount: f32) -> f32 {
    x * (1.0 - amount) + x * x * x * amount
}

/// A bright rainbow colour, `h` from 0 to 1 round the colour wheel.
fn rainbow(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let c = match h as i32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    c.map(|v| 0.15 + 0.85 * v)
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
    // roads, the grass strip and the towns
    let to_px = |x: f32, z: f32| (((x + HALF) / (2.0 * HALF) * n as f32) as i32, ((z + HALF) / (2.0 * HALF) * n as f32) as i32);
    let mut dot = |x: f32, z: f32, c: Color| {
        let (i, j) = to_px(x, z);
        if (0..n as i32).contains(&i) && (0..n as i32).contains(&j) {
            img.set_pixel(i as u32, j as u32, c);
        }
    };
    for road in &world.roads {
        for w in road.windows(2) {
            for k in 0..8 {
                let p = w[0].lerp(w[1], k as f32 / 8.0);
                dot(p.x, p.z, Color::new(0.45, 0.43, 0.4, 1.0));
            }
        }
    }
    for k in -8..=8 {
        dot(STRIP.0 + k as f32 * STRIP_HALF_LEN / 8.0, STRIP.1, Color::new(0.75, 0.9, 0.6, 1.0));
    }
    for t in &world.towns {
        for (dx, dz) in [(0.0, 0.0), (60.0, 0.0), (0.0, 60.0), (60.0, 60.0)] {
            dot(t.x + dx, t.z + dz, Color::new(0.8, 0.45, 0.35, 1.0));
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
        let lesson = &self.lessons[idx];
        let free = lesson.kind == Kind::Free;
        self.traffic_on = free;
        let craft = if free { self.free.craft() } else { lesson.craft };
        let start = if free { self.free.start() } else { lesson.start.clone() };
        self.weather = if free { self.free.weather() } else { lesson.weather.unwrap_or_default() };
        self.ac = match &start {
            Start::Ground { x, z, heading } => Aircraft::on_ground_at(craft, &self.world.terrain, *x, *z, *heading),
            Start::Runway27 => {
                let back = if craft == Craft::Jet { 40.0 } else { 25.0 };
                let mut a = Aircraft::on_ground_at(craft, &self.world.terrain, RWY_HALF_LEN - back, 0.0, 270.0);
                if craft == Craft::Jet {
                    a.ctl.flaps = 3;
                    a.flap_deg = a.spec.flaps[3];
                    a.ctl.trim = 0.2;
                }
                a
            }
            Start::Runway => {
                // line up with the nose wheel just past the runway end
                let back = if craft == Craft::Jet { 40.0 } else { 25.0 };
                let mut a = Aircraft::on_ground_at(craft, &self.world.terrain, -RWY_HALF_LEN + back, 0.0, 90.0);
                if craft == Craft::Jet {
                    // takeoff configuration: flaps 5 and a little nose-up trim
                    a.ctl.flaps = 3;
                    a.flap_deg = a.spec.flaps[3];
                    a.ctl.trim = 0.2;
                }
                a
            }
            Start::Air { pos, heading, ias, gamma, flaps, trim_offset } => {
                let mut a = Aircraft::in_flight(craft, *pos, *heading, *ias, *gamma, *flaps);
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
        self.smoke.clear();
        self.gpws = gpws::Gpws::default();
        self.tower = atc::Tower::new((self.time * 1000.0) as u32 % 900 + idx as u32 * 37);
        self.ap = ap::Autopilot::default();
        self.ap.sync(&self.ac);
        self.sim_time = 0.0;
        self.landing = score::LandingLog::default();
        self.best_ring = self.profile.number(&format!("ring.{}", self.lessons[idx].name));
        self.recorder.clear();
        self.replay = None;
        self.touch_time = None;
        if self.lessons[idx].engine_out {
            self.ac.engine_failed = true;
            self.ac.ctl.throttle = 0.0;
        }
        self.prev_pos = self.ac.pos;
        self.acc = 0.0;
        self.run = ghost::Run::default();
        self.approach = debrief::ApproachLog::default();
        self.debrief = None;
        self.result_stars = None;
        let lesson = &self.lessons[idx];
        self.ghost = matches!(lesson.kind, Kind::Rings | Kind::Landing).then(|| ghost::load(lesson.name)).flatten();
        self.ghost_ac = self.ghost.as_ref().map(|_| Aircraft::in_flight(craft, self.ac.pos, 90.0, 100.0, 0.0, 0));
    }

    fn stars_total(&self) -> u32 {
        progress::total(&self.lessons, &self.profile, &self.world).0
    }

    fn lesson_locked(&self, i: usize) -> Option<u32> {
        let earned = self.stars_total();
        let craft = self.lessons[i].craft;
        (!progress::unlocked(craft, earned, &self.profile)).then(|| progress::unlock_at(craft) - earned)
    }

    /// Save a finished attempt's stars and say what it unlocked.
    fn award(&mut self, earned: u32, max: u32, before: u32) {
        self.result_stars = Some((earned, max));
        let after = self.stars_total();
        for (craft, name) in setup::AIRCRAFT {
            let at = progress::unlock_at(craft);
            if before < at && after >= at && self.profile.get("unlock_all").is_none() {
                self.mission.summary.push(String::new());
                self.mission.summary.push(format!("Unlocked: {name}! You'll find it in Objectives and Free Fly."));
                self.audio.chime();
            }
        }
    }

    /// The main menu (objectives, free fly) or the objectives, with what each row does.
    fn menu_rows(&self) -> (Vec<hud::MenuRow>, Vec<Option<MenuAct>>) {
        use hud::{MenuItem, MenuRow::*};
        let mut rows = Vec::new();
        let mut acts = Vec::new();
        let mut push = |r: hud::MenuRow, a: Option<MenuAct>| {
            rows.push(r);
            acts.push(a);
        };
        let (earned, possible) = progress::total(&self.lessons, &self.profile, &self.world);
        if self.menu_page == MenuPage::Main {
            push(
                Item(MenuItem {
                    name: "Objectives".into(),
                    blurb: "Guided lessons, then landing and racing challenges. Earn stars to unlock new aircraft.".into(),
                    stars: Some((earned, possible)),
                    ..Default::default()
                }),
                Some(MenuAct::Objectives),
            );
            let next = setup::AIRCRAFT.iter().filter(|(c, _)| !progress::unlocked(*c, earned, &self.profile)).min_by_key(|(c, _)| progress::unlock_at(*c));
            let craft = setup::AIRCRAFT[self.free.craft].1;
            push(
                Item(MenuItem {
                    name: "Free Fly".into(),
                    blurb: format!("Pick an aircraft, a start, the time and the weather. Last time: {craft}."),
                    goal: next.map(|(c, n)| format!("Next unlock at {} stars: {n}", progress::unlock_at(*c))).unwrap_or_default(),
                    ..Default::default()
                }),
                Some(MenuAct::Setup),
            );
            let popped = self.profile.number("kids.balloons").unwrap_or(0.0);
            push(
                Item(MenuItem {
                    name: "Kids Mode".into(),
                    blurb: "For little pilots: one stick, no crashing, and a friendly voice with little missions. Pop balloons, fly through rainbows, find the lighthouse, land. Planes, a helicopter and a hot air balloon.".into(),
                    goal: if popped > 0.0 { format!("Balloons popped so far: {popped:.0}, stars: {:.0}", self.profile.number("kids.stars").unwrap_or(0.0)) } else { String::new() },
                    ..Default::default()
                }),
                Some(MenuAct::Kids),
            );
            let best = self.profile.number("red.best");
            let best_plane = self.profile.get("red.plane").unwrap_or("").replace('_', " ");
            push(
                Item(MenuItem {
                    name: "99 Red Balloons".into(),
                    blurb: "A speedrun along a rainbow road round the airport: 99 red balloons in a line, each one missed costs two seconds. Zoom on the straights, ease off for the turns. To an 80s new-wave beat.".into(),
                    goal: best.map(|b| format!("Best lap: {} ({best_plane})", lessons::fmt_time(b))).unwrap_or_default(),
                    ..Default::default()
                }),
                Some(MenuAct::Red),
            );
            return (rows, acts);
        }
        let groups: [(&str, fn(&Lesson) -> bool); 3] = [
            ("LEARN TO FLY", |l| l.kind == Kind::Lesson && l.craft == Craft::Cessna),
            ("MORE AIRCRAFT", |l| l.kind == Kind::Lesson && l.craft != Craft::Cessna),
            ("CHALLENGES", |l| matches!(l.kind, Kind::Landing | Kind::Rings)),
        ];
        for (title, keep) in groups {
            push(Header(title), None);
            for (i, l) in self.lessons.iter().enumerate().filter(|(_, l)| keep(l)) {
                let mark = match l.kind {
                    Kind::Rings => self.profile.number(&format!("ring.{}", l.name)).map(|b| lessons::fmt_time(b)).unwrap_or_default(),
                    _ => self.profile.get(&format!("grade.{}", l.name)).map(|g| g.to_string()).unwrap_or_default(),
                };
                let locked = self.lesson_locked(i);
                let blurb = match locked {
                    Some(n) => format!("Earn {n} more star{} to unlock this aircraft. {}", if n == 1 { "" } else { "s" }, l.blurb),
                    None => l.blurb.into(),
                };
                push(
                    Item(MenuItem {
                        name: l.name.into(),
                        mark,
                        blurb,
                        goal: progress::goal(l, &self.world),
                        stars: Some((progress::stars(l, &self.profile, &self.world), progress::max_stars(l))),
                        locked,
                    }),
                    Some(MenuAct::Lesson(i)),
                );
            }
        }
        (rows, acts)
    }

    fn to_menu(&mut self) {
        self.screen = Screen::Menu;
        // back to where the flight was picked from
        let idx = self.idx;
        if let Some(k) = self.kids.take() {
            self.menu_page = MenuPage::Main;
            self.sel = if k.red.is_some() { 3 } else { 2 };
            self.audio.music(None);
        } else if self.lessons[idx].kind == Kind::Free {
            self.menu_page = MenuPage::Main;
            self.sel = 1;
        } else {
            self.menu_page = MenuPage::Objectives;
            let (_, acts) = self.menu_rows();
            self.sel = acts.iter().position(|a| matches!(a, Some(MenuAct::Lesson(i)) if *i == idx)).unwrap_or(1);
        }
        self.paused = false;
        self.ac = Aircraft::on_ground_at(Craft::Cessna, &self.world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
    }

    fn toast(&mut self, s: impl Into<String>) {
        self.toasts.push((s.into(), 2.5));
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    /// Read the stick, throttle and switches. Returns (flight controls moved, throttle moved,
    /// trim moved) so the autopilot can let go when the pilot takes over.
    fn controls(&mut self, dt: f32) -> (bool, bool, bool) {
        use KeyCode::*;
        let (mut pull, mut push) = (key_any(&[S, Down]), key_any(&[W, Up]));
        if self.invert {
            std::mem::swap(&mut pull, &mut push);
        }
        let axis = |pos: bool, neg: bool| pos as i32 as f32 - neg as i32 as f32;
        // Pitch is the touchy axis: a tap is a gentle nudge, holding builds up to full deflection over ~1.3 s.
        self.stick.pitch_hold = ease(self.stick.pitch_hold, axis(pull, push), 0.75, dt);
        self.stick.roll = ease(self.stick.roll, axis(key_any(&[D, Right]), key_any(&[A, Left])), 1.7, dt);
        self.stick.yaw = ease(self.stick.yaw, axis(key_any(&[E]), key_any(&[Q])), 1.7, dt);

        let p = &self.pad;
        let spec = self.ac.spec;
        let on_ground = self.ac.on_ground;
        let airborne = self.ac.airborne_time > 1.0;
        let pad_pitch = -p.axis(PadAxis::LeftStickY) * if self.invert { -1.0 } else { 1.0 };
        let c = &mut self.ac.ctl;
        c.pitch = (expo(self.stick.pitch_hold, 1.0) + expo(pad_pitch, 0.85)).clamp(-1.0, 1.0);
        c.roll = (self.stick.roll + expo(p.axis(PadAxis::LeftStickX), 0.5)).clamp(-1.0, 1.0);
        c.yaw = (self.stick.yaw + p.axis(PadAxis::RightStickX)).clamp(-1.0, 1.0);
        let thr = (axis(key_any(&[LeftShift, RightShift, Equal, KpAdd]), key_any(&[LeftControl, RightControl, Minus, KpSubtract])) + p.axis(PadAxis::RightStickY)).clamp(-1.0, 1.0);
        // Keep pulling back at idle on the ground and the thrust reversers come up (jets);
        // push forward again to stow them before the throttle moves.
        self.stick.idle_hold = if thr < -0.3 && c.throttle <= 0.0 { self.stick.idle_hold + dt } else { 0.0 };
        if spec.is_jet() && on_ground && self.stick.idle_hold > 0.4 {
            c.reverse = (c.reverse - thr * 0.6 * dt).min(1.0);
        } else if c.reverse > 0.0 {
            if thr > 0.0 {
                c.reverse = (c.reverse - thr * 0.8 * dt).max(0.0);
            }
        } else {
            c.throttle = (c.throttle + thr * 0.5 * dt).clamp(0.0, 1.0);
        }
        if airborne {
            c.reverse = 0.0;
        }
        let trim = axis(key_any(&[T, RightBracket]) || p.down(PadBtn::DPadDown), key_any(&[G, LeftBracket]) || p.down(PadBtn::DPadUp));
        self.stick.trim_hold = if trim != 0.0 { self.stick.trim_hold + dt } else { 0.0 };
        // A tap moves the trimmed speed by about a knot; holding winds it quickly.
        let rate = 0.025 + 0.12 * smoothstep(0.5, 1.8, self.stick.trim_hold);
        c.trim = (c.trim + trim * rate * dt).clamp(-0.6, 0.6);
        if (is_key_pressed(F) || p.pressed(PadBtn::RightTrigger)) && c.flaps < spec.max_flaps() {
            c.flaps += 1;
        }
        if (is_key_pressed(V) || p.pressed(PadBtn::LeftTrigger)) && c.flaps > 0 {
            c.flaps -= 1;
        }
        // Brakes: B / Space or A for both; the triggers are analog left and right toe brakes.
        c.brake = if key_any(&[B, Space]) || p.down(PadBtn::South) { 1.0 } else { 0.0 };
        c.brake_l = p.value(PadBtn::LeftTrigger2);
        c.brake_r = p.value(PadBtn::RightTrigger2);
        let mut note = None;
        if spec.spoilers && (is_key_pressed(X) || p.pressed(PadBtn::East)) {
            // a sailplane's airbrakes are simply out or in
            c.speedbrake = if !spec.has_engine() {
                if c.speedbrake == flight::Speedbrake::Up { flight::Speedbrake::Down } else { flight::Speedbrake::Up }
            } else {
                c.speedbrake.next()
            };
            note = Some(match c.speedbrake {
                flight::Speedbrake::Down if !spec.has_engine() => "Airbrakes in",
                flight::Speedbrake::Down => "Speedbrake down",
                flight::Speedbrake::Armed => "Speedbrake armed",
                flight::Speedbrake::Up if !spec.has_engine() => "Airbrakes out",
                flight::Speedbrake::Up => "Speedbrake up",
            });
        }
        if spec.retractable && (is_key_pressed(L) || p.pressed(PadBtn::DPadRight)) {
            if on_ground {
                note = Some("Gear stays down on the ground");
            } else {
                c.gear_up = !c.gear_up;
                note = Some(if c.gear_up { "Gear up" } else { "Gear down" });
            }
        }
        let moved = c.pitch.abs() > 0.3 || c.roll.abs() > 0.3;
        if let Some(n) = note {
            self.toast(n);
        }
        (moved, thr != 0.0, trim != 0.0)
    }

    /// Autopilot keys: Z engage, K autothrottle, 1-4 HDG / ALT HOLD / V/S / APP, and the bugs:
    /// , . heading, ; ' altitude, 9 0 vertical speed, 7 8 speed (tap = one step, hold = fast).
    fn autopilot_keys(&mut self, dt: f32) {
        use KeyCode::*;
        if is_key_pressed(Z) {
            self.mcp_action(hud::McpField::Cmd);
        }
        if is_key_pressed(K) {
            self.mcp_action(hud::McpField::At);
        }
        if is_key_pressed(Key1) {
            self.mcp_action(hud::McpField::Hdg);
        }
        if is_key_pressed(Key2) {
            self.mcp_action(hud::McpField::Alt);
        }
        if is_key_pressed(Key3) {
            self.mcp_action(hud::McpField::Vs);
        }
        if is_key_pressed(Key4) {
            self.mcp_action(hud::McpField::App);
        }
        let bugs = [(Comma, Period, hud::McpField::Hdg), (Semicolon, Apostrophe, hud::McpField::Alt), (Key9, Key0, hud::McpField::Vs), (Key7, Key8, hud::McpField::Spd)];
        let mut any = false;
        for (down, up, f) in bugs {
            let dir = is_key_down(up) as i32 as f32 - is_key_down(down) as i32 as f32;
            if dir == 0.0 {
                continue;
            }
            any = true;
            if is_key_pressed(up) || is_key_pressed(down) {
                self.bug_step(f, dir);
            } else if self.bug_hold > 0.35 {
                // held: about ten steps a second
                let before = ((self.bug_hold - dt) * 10.0) as i32;
                if (self.bug_hold * 10.0) as i32 > before {
                    self.bug_step(f, dir * 2.0);
                }
            }
        }
        self.bug_hold = if any { self.bug_hold + dt } else { 0.0 };
    }

    /// Nudge an autopilot bug by `n` steps.
    fn bug_step(&mut self, f: hud::McpField, n: f32) {
        let ap = &mut self.ap;
        match f {
            hud::McpField::Hdg => ap.hdg = (ap.hdg + n * 5.0).rem_euclid(360.0),
            hud::McpField::Alt => ap.alt_ft = (ap.alt_ft + n * 100.0).clamp(0.0, 41_000.0),
            hud::McpField::Vs => ap.vs_fpm = (ap.vs_fpm + n * 100.0).clamp(-6000.0, 6000.0),
            hud::McpField::Spd => ap.spd_kt = (ap.spd_kt + n).clamp(40.0, 340.0),
            _ => {}
        }
        // a new altitude while holding one means: go there
        if f == hud::McpField::Alt && self.ap.engaged && self.ap.vertical == ap::Vertical::Alt {
            let dh = self.ap.alt_ft - self.ac.pos.y / flight::FT;
            if dh.abs() > 100.0 {
                self.ap.vertical = ap::Vertical::Vs;
                self.ap.vs_fpm = (if self.ac.spec.is_jet() { 1500.0f32 } else { 500.0 }).copysign(dh);
            }
        }
    }

    /// Press an MCP button.
    fn mcp_action(&mut self, f: hud::McpField) {
        let ac = &self.ac;
        let ap = &mut self.ap;
        match f {
            hud::McpField::Cmd => {
                if ap.engaged {
                    ap.disengage();
                } else if !ac.on_ground {
                    ap.engage(ac);
                    self.audio.click();
                }
            }
            hud::McpField::At | hud::McpField::Spd => ap.toggle_at(ac),
            hud::McpField::Hdg => ap.select_hdg(),
            hud::McpField::Alt => ap.select_alt(ac),
            hud::McpField::Vs => ap.select_vs(ac),
            hud::McpField::App => ap.toggle_app(),
        }
    }

    fn frame(&mut self, dt: f32) -> bool {
        self.time += dt;
        if self.traffic_on {
            self.traffic.update(self.time);
        }
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
        let mut wheel = if wheel.abs() > 0.0 { wheel.signum() } else { 0.0 };
        // the autopilot panel takes the mouse: click a window to press it, scroll to set it
        let mut dragging = dragging;
        self.mcp_hover = None;
        if self.screen == Screen::Flying && !self.paused && !self.help && self.ac.crashed.is_none() && self.kids.is_none() {
            let layout = hud::mcp_layout(self.ac.spec.is_jet(), hud::panel_top(self.panel));
            self.mcp_hover = layout.iter().find(|(_, r)| r.contains(mp)).map(|(f, _)| *f);
            if let Some(f) = self.mcp_hover {
                if wheel != 0.0 {
                    self.bug_step(f, wheel);
                    wheel = 0.0;
                }
                if is_mouse_button_pressed(MouseButton::Left) {
                    self.mcp_action(f);
                }
                dragging = false;
            }
        }
        let drag = if dragging { drag } else { Vec2::ZERO };

        self.pad.poll();
        let just = self.pad.just_pressed.clone();
        let pad = |b: PadBtn| just.contains(&b);
        if is_key_pressed(KeyCode::H) || pad(PadBtn::Select) {
            self.help = !self.help;
        }
        if is_key_pressed(KeyCode::M) {
            self.audio.muted = !self.audio.muted;
        }

        if self.screen != Screen::Flying {
            let up = pressed_any(&[KeyCode::Up, KeyCode::W]) || pad(PadBtn::DPadUp) || self.pad.nav < 0;
            let down = pressed_any(&[KeyCode::Down, KeyCode::S]) || pad(PadBtn::DPadDown) || self.pad.nav > 0;
            let go = pressed_any(&[KeyCode::Enter, KeyCode::KpEnter, KeyCode::Space]) || pad(PadBtn::South) || pad(PadBtn::Start);
            let back = is_key_pressed(KeyCode::Escape) || pad(PadBtn::East);
            if self.help {
                if back {
                    self.help = false;
                }
            } else if self.screen == Screen::Menu {
                let (_, acts) = self.menu_rows();
                let n = acts.len();
                let mut moved = 0i32;
                if down {
                    moved = 1;
                }
                if up {
                    moved = -1;
                }
                if moved != 0 {
                    // skip the section headers
                    loop {
                        self.sel = (self.sel as i32 + moved).rem_euclid(n as i32) as usize;
                        if acts[self.sel].is_some() {
                            break;
                        }
                    }
                    self.audio.click();
                }
                if go {
                    match acts.get(self.sel).copied().flatten() {
                        Some(MenuAct::Lesson(i)) if self.lesson_locked(i).is_none() => self.start(i),
                        Some(MenuAct::Lesson(_)) => {}
                        Some(MenuAct::Objectives) => {
                            self.menu_page = MenuPage::Objectives;
                            let (_, acts) = self.menu_rows();
                            self.sel = acts.iter().position(|a| a.is_some()).unwrap_or(0);
                        }
                        Some(MenuAct::Setup) => {
                            self.screen = Screen::Setup;
                            self.setup_sel = setup::ROWS - 1;
                        }
                        Some(MenuAct::Kids) => self.start_kids(false),
                        Some(MenuAct::Red) => self.start_kids(true),
                        None => {}
                    }
                    if self.screen == Screen::Flying {
                        return true;
                    }
                } else if back && self.menu_page == MenuPage::Objectives {
                    self.menu_page = MenuPage::Main;
                    self.sel = 0;
                    self.audio.click();
                } else if is_key_pressed(KeyCode::Escape) {
                    return false;
                }
            } else {
                let n = setup::ROWS;
                if down {
                    self.setup_sel = (self.setup_sel + 1) % n;
                    self.audio.click();
                }
                if up {
                    self.setup_sel = (self.setup_sel + n - 1) % n;
                    self.audio.click();
                }
                let left = pressed_any(&[KeyCode::Left, KeyCode::A]) || pad(PadBtn::DPadLeft);
                let right = pressed_any(&[KeyCode::Right, KeyCode::D]) || pad(PadBtn::DPadRight);
                if left || right {
                    self.free.change(self.setup_sel, if right { 1 } else { -1 });
                    self.audio.click();
                }
                let locked = {
                    let earned = self.stars_total();
                    let c = self.free.craft();
                    (!progress::unlocked(c, earned, &self.profile)).then(|| progress::unlock_at(c) - earned)
                };
                if go && locked.is_none() {
                    self.profile.set_setup(&self.free);
                    self.profile.save();
                    let i = self.lessons.iter().position(|l| l.kind == Kind::Free).unwrap_or(0);
                    self.start(i);
                    return true;
                }
                if back {
                    self.screen = Screen::Menu;
                    self.menu_page = MenuPage::Main;
                    self.sel = 1;
                }
            }
            self.cam.menu(self.time, self.ac.pos);
            // the parked plane ticks over at idle while the menu camera circles it
            let d = (self.cam.eye - self.ac.pos).length();
            self.audio.update(audio::State { rpm: 720.0, throttle: 0.0, distance: d, volume: 0.7, ..Default::default() });
            self.draw_world();
            if self.screen == Screen::Menu {
                let (rows, _) = self.menu_rows();
                hud::menu(&rows, self.sel, self.time, self.pad.connected(), self.menu_page == MenuPage::Objectives);
            } else {
                let earned = self.stars_total();
                let c = self.free.craft();
                let locked = (!progress::unlocked(c, earned, &self.profile)).then(|| progress::unlock_at(c) - earned);
                hud::setup_screen(&self.free.rows(locked), self.setup_sel, self.time, self.pad.connected());
            }
            if self.help {
                hud::help_overlay();
            }
            return true;
        }

        if self.kids.is_some() {
            return self.kids_frame(dt, drag, dragging, wheel, &just);
        }

        // ---- replay ----
        if self.replay.is_some() {
            return self.replay_frame(dt, drag, dragging, wheel, &just);
        }

        // ---- flying ----
        // On the gamepad, face buttons answer the dialog when one is up: A confirm, B menu, X retry, Y next.
        let crashed = self.ac.crashed.is_some();
        let complete = self.mission.complete && !self.summary_dismissed;
        let dialog = crashed || complete || self.paused;
        // from any dialog, Space (or the right stick click) replays the last stretch of flight
        if dialog && !self.help && (is_key_pressed(KeyCode::Space) || pad(PadBtn::RightThumb)) {
            self.replay = self.recorder.replay(25.0, self.touch_time);
            if self.replay.is_some() {
                return true;
            }
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P) || pad(PadBtn::Start) {
            if self.help {
                self.help = false;
            } else if crashed || complete {
                self.to_menu();
                return true;
            } else {
                self.paused = !self.paused;
            }
        }
        if dialog && !self.help && pad(PadBtn::East) || self.paused && is_key_pressed(KeyCode::Backspace) {
            self.to_menu();
            return true;
        }
        if is_key_pressed(KeyCode::R) || crashed && (pressed_any(&[KeyCode::Enter, KeyCode::KpEnter]) || pad(PadBtn::South)) || dialog && pad(PadBtn::West) {
            self.start(self.idx);
            return true;
        }
        if complete {
            if pressed_any(&[KeyCode::Enter, KeyCode::KpEnter]) || pad(PadBtn::South) {
                self.summary_dismissed = true;
            }
            if is_key_pressed(KeyCode::N) || pad(PadBtn::North) {
                let next = (self.idx + 1..self.lessons.len()).find(|&i| self.lessons[i].kind != Kind::Free && self.lesson_locked(i).is_none());
                if let Some(i) = next {
                    self.start(i);
                    return true;
                }
            }
        } else if is_key_pressed(KeyCode::N) {
            self.mission.skip_step(&self.lessons[self.idx]);
        }
        if is_key_pressed(KeyCode::C) || !dialog && pad(PadBtn::North) {
            self.cam.mode = self.cam.mode.next();
            self.cam.reset(&self.ac);
            self.toast(format!("{} view", self.cam.mode.name()));
        }
        if is_key_pressed(KeyCode::Tab) || pad(PadBtn::DPadLeft) {
            self.panel = self.panel.next();
        }
        if is_key_pressed(KeyCode::Y) || !dialog && pad(PadBtn::West) {
            self.auto_rudder = !self.auto_rudder;
            self.ac.auto_rudder = self.auto_rudder;
            self.toast(if self.auto_rudder { "Auto-rudder ON" } else { "Auto-rudder OFF" });
        }
        if is_key_pressed(KeyCode::J) && self.ghost.is_some() {
            self.show_ghost = !self.show_ghost;
            self.toast(if self.show_ghost { "Ghost on" } else { "Ghost off" });
        }
        if is_key_pressed(KeyCode::U) {
            self.hud_on = !self.hud_on;
            self.toast(if self.hud_on { "Head-up display on (cockpit view)" } else { "Head-up display off" });
        }
        if is_key_pressed(KeyCode::O) {
            self.coach_small = !self.coach_small;
        }
        if is_key_pressed(KeyCode::I) {
            self.invert = !self.invert;
            self.toast(if self.invert { "Pitch inverted" } else { "Pitch normal" });
        }

        let running = !self.paused && !self.help && !crashed;
        if running {
            let (moved, throttle, trim) = self.controls(dt);
            self.autopilot_keys(dt);
            // gamepad: left stick click for the autopilot, right stick click for approach mode
            if self.pad.pressed(PadBtn::LeftThumb) {
                self.mcp_action(hud::McpField::Cmd);
            }
            if !dialog && self.pad.pressed(PadBtn::RightThumb) {
                self.mcp_action(hud::McpField::App);
            }
            // taking the controls disconnects; moving the thrust levers drops the autothrottle
            if self.ap.engaged && (moved || trim) {
                self.ap.disengage();
            }
            if self.ap.at && throttle {
                self.ap.toggle_at(&self.ac);
            }
            if self.ap.engaged || self.ap.at {
                let ac = &self.ac;
                let agl = (ac.pos.y - self.world.terrain.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height) / flight::FT;
                self.ap.update(&mut self.ac, agl, dt);
            }
            for n in std::mem::take(&mut self.ap.notes) {
                match n {
                    ap::Note::Disconnect => {
                        self.audio.ap_off();
                        self.toast("Autopilot disconnected");
                    }
                    ap::Note::AtDisconnect => self.audio.at_off(),
                }
            }
            self.acc += dt;
            while self.acc >= DT {
                let ac = &self.ac;
                let agl = ac.pos.y - self.world.terrain.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height;
                let mut air = self.weather.air(ac.pos, agl, self.sim_time, ac.spec.span, ac.right());
                // thermals: lift under each wingtip, so a thermal rolls you away from its core
                let half = ac.right() * (ac.spec.span * 0.5);
                let th = &self.world.thermals;
                let (l, r) = (self.weather.lift(th, ac.pos - half, agl, self.sim_time), self.weather.lift(th, ac.pos + half, agl, self.sim_time));
                air.wind.y += (l + r) * 0.5;
                air.roll_gust += (r - l) / ac.spec.span;
                self.ac.air = air;
                self.sim_time += DT;
                self.ac.step(DT, &self.world.terrain);
                self.acc -= DT;
            }
        }
        self.landing.update(&self.ac, &self.weather, dt);
        if running {
            self.recorder.record(&self.ac, self.sim_time);
            if !self.mission.complete {
                self.run.record(&self.ac, self.mission.time);
                if self.ac.touchdown.is_none() {
                    self.approach.record(&self.ac, self.mission.time);
                }
            }
        }
        if let (Some(g), Some(gac)) = (&self.ghost, &mut self.ghost_ac) {
            // the ghost keeps going once you've finished, so you can watch it come in
            let t = self.mission.time + if self.mission.complete { self.sim_time - self.done_at } else { 0.0 };
            self.ghost_visible = self.show_ghost && g.pose(t, gac);
        } else {
            self.ghost_visible = false;
        }
        if self.ac.touchdown_event && self.touch_time.is_none() {
            self.touch_time = Some(self.sim_time);
        }
        if self.ac.touchdown_event {
            self.ac.touchdown_event = false;
            if let Some(td) = self.ac.touchdown {
                self.audio.touchdown(td.sink_fpm);
                self.cam.bump(td.sink_fpm / 900.0);
                self.wheel_smoke(4 + (td.sink_fpm / 60.0) as usize, [0.9, 0.9, 0.88]);
                let grade = match td.sink_fpm {
                    s if s < 120.0 => "Butter!",
                    s if s < 250.0 => "Smooth",
                    s if s < 450.0 => "Firm",
                    _ => "Hard!",
                };
                self.toast(format!("{grade}  {:.0} fpm", td.sink_fpm));
            }
        }
        if self.ac.crashed.is_some() && !self.crash_noted {
            self.crash_noted = true;
            self.audio.crash();
            self.cam.bump(1.0);
            for k in 0..40 {
                let a = k as f32 * 2.4;
                let v = vec3(a.cos(), 0.6 + (k % 5) as f32 * 0.3, a.sin()) * (2.0 + (k % 7) as f32);
                let c = if k % 3 == 0 { [0.35, 0.33, 0.3] } else { [0.62, 0.58, 0.5] };
                self.smoke.push(Smoke { pos: self.ac.pos, vel: v, age: 0.0, life: 2.5 + (k % 4) as f32, size: 2.0, color: c, opacity: 0.55 });
            }
        }

        let t = self.ac.telem(&self.world.terrain);
        if running {
            if let Some(words) = self.tower.update(&self.ac, &t, &self.weather, dt) {
                self.audio.radio(&words);
            }
        }
        if running && self.ac.spec.is_jet() {
            let gs = lessons::ils(t.pos, t.hdg).map(|d| d.1);
            if let Some(c) = self.gpws.update(&t, gs, dt) {
                self.audio.say(c);
            }
        }
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
                    self.done_at = self.sim_time;
                    let before = self.stars_total();
                    let lesson = &self.lessons[self.idx];
                    let name = lesson.name;
                    let max = progress::max_stars(lesson);
                    self.profile.set(&format!("done.{name}"), "1");
                    let mut earned = 1;
                    let touch = (self.landing.touch, self.ac.touchdown);
                    if let (true, Some(touch), Some(td)) = (progress::graded(lesson), touch.0, touch.1) {
                        let (lines, total, grade) = self.landing.card(self.ac.craft, &self.weather, self.weather.sky().night);
                        earned = progress::grade_stars(grade);
                        let target = debrief::target_speed(self.ac.craft, lesson.vref);
                        self.debrief = debrief::build(&self.approach, td.pos, touch.along, self.ac.craft, target, self.weather.gust_kt, !self.ac.spec.has_engine() || self.ac.engine_failed);
                        let best_key = format!("best.{name}");
                        let best = self.profile.number(&best_key);
                        // the last best run's touchdown, to compare against
                        let prev: Vec<f32> = self.profile.get(&format!("run.{name}")).map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
                        if lesson.kind == Kind::Landing {
                            self.mission.summary = lines;
                        } else {
                            self.mission.summary.push(String::new());
                            self.mission.summary.push(format!("Landing grade {grade} - {total} points"));
                        }
                        if best.map_or(true, |b| total as f32 > b) {
                            self.profile.set(&best_key, &total.to_string());
                            self.profile.set(&format!("grade.{name}"), grade);
                            self.profile.set(&format!("run.{name}"), &format!("{:.0},{:.0}", touch.sink_fpm, touch.along));
                            if let Some(b) = best {
                                self.mission.summary.push(format!("New personal best! (was {b:.0})"));
                            }
                            if lesson.kind == Kind::Landing {
                                ghost::save(name, &self.run);
                            }
                        } else if let Some(b) = best {
                            let mut line = format!("Personal best: {b:.0}");
                            if let [sink, along] = prev[..] {
                                let d = touch.sink_fpm - sink;
                                line += &format!(" ({sink:.0} fpm, {along:.0} m in) - this time {:.0} fpm {}", d.abs(), if d > 0.0 { "firmer" } else { "softer" });
                            }
                            self.mission.summary.push(line);
                        }
                    }
                    self.award(earned, max, before);
                    self.profile.save();
                }
                Some(Event::Ring) => {
                    let n = self.mission.next_ring;
                    self.audio.ring(n);
                    self.run.splits.push(self.mission.time);
                    // split against the ghost's time at the same ring
                    let split = self.ghost.as_ref().and_then(|g| g.splits.get(n - 1)).map(|b| self.mission.time - b);
                    match split {
                        Some(d) => self.toast(format!("Ring {n}/{}   {}{:.1} s", self.world.rings.len(), if d < 0.0 { "-" } else { "+" }, d.abs())),
                        None => self.toast(format!("Ring {n}/{}", self.world.rings.len())),
                    }
                }
                Some(Event::RingsDone) => {
                    self.audio.chime();
                    self.done_at = self.sim_time;
                    let before = self.stars_total();
                    let lesson = &self.lessons[self.idx];
                    let name = lesson.name;
                    let pars = progress::ring_pars(&self.world, lesson.craft);
                    let key = format!("ring.{name}");
                    let prev = self.profile.number(&key);
                    if prev.map_or(true, |p| self.mission.time < p) {
                        self.profile.set(&key, &self.mission.time.to_string());
                        self.run.splits.push(self.mission.time);
                        ghost::save(name, &self.run);
                    }
                    let earned = progress::ring_stars(self.mission.time, pars);
                    if earned < 3 {
                        let next = if earned == 1 { pars.1 } else { pars.0 };
                        self.mission.summary.push(format!("Next star: under {}", lessons::fmt_time(next)));
                    }
                    self.award(earned, 3, before);
                    self.profile.save();
                }
                None => {}
            }
        }
        self.prev_pos = self.ac.pos;

        if !self.paused && !self.help {
            self.update_smoke(dt);
        }
        self.cam.update(&self.ac, &self.world.terrain, dt, drag, dragging, wheel);
        if let Some((eye, target)) = self.shot_cam {
            self.cam.set_fixed(eye, target);
        }
        self.update_audio(&t, running, dt);

        self.draw_world();
        self.draw_hud(&t);
        true
    }

    /// Kids mode (or 99 Red Balloons): a sunny, calm day, lined up on the runway.
    fn start_kids(&mut self, red: bool) {
        self.idx = self.lessons.iter().position(|l| l.kind == Kind::Free).unwrap_or(0);
        self.mission = Mission::default();
        self.weather = weather::Weather { wind_kt: 0.0, gust_kt: 0.0, turbulence: 0.0, clouds: 0.35, hour: 11.0, thermals: 0.0, ..Default::default() };
        self.traffic_on = true;
        let seed = (self.time * 1000.0) as u64;
        // keep the plane picked last time when playing again
        let vehicle = self.kids.as_ref().map_or(0, |k| k.vehicle);
        let mut k = if red { kids::Kids::red_balloons(&self.world.terrain, seed) } else { kids::Kids::new(&self.world.terrain, seed) };
        k.vehicle = vehicle;
        self.ac = k.spawn(&self.world.terrain);
        if red {
            // a golden evening for the rainbow road, and the sky to yourself
            self.weather.hour = 18.4;
            self.weather.clouds = 0.25;
            self.traffic_on = false;
            let gl = unsafe { get_internal_gl() };
            self.renderer.set_road(gl.quad_context, &k.course);
        }
        self.kids = Some(k);
        self.cam.mode = CamMode::Chase;
        self.cam.reset(&self.ac);
        self.screen = Screen::Flying;
        self.paused = false;
        self.help = false;
        self.toasts.clear();
        self.smoke.clear();
        self.replay = None;
        self.ghost = None;
        self.ghost_ac = None;
        self.ghost_visible = false;
        self.prev_pos = self.ac.pos;
        if red {
            let song = self.song.get_or_insert_with(|| std::sync::Arc::new(music::load())).clone();
            self.audio.music(Some(song));
        } else {
            self.audio.music(None);
            self.audio.kid(audio::Line::Go, false);
        }
    }

    /// Kids mode's places to find: the windmills, the lighthouse and a sailing boat.
    fn landmarks(&self) -> [Vec3; 3] {
        let t = &self.world.terrain;
        let l = LIGHTHOUSE;
        let (pier, dir) = scenery::pier(t);
        let boat = scenery::boats(self.time, pier, dir).first().map_or(Vec3::ZERO, |b| b.1.w_axis.truncate()) + Vec3::Y * 8.0;
        [self.world.turbines[3] + Vec3::Y * 82.0, vec3(l.0, t.height(l.0, l.1) + 32.0, l.1), boat]
    }

    /// A burst of puffs flying apart from `pos`.
    fn burst(&mut self, pos: Vec3, n: usize, speed: f32, colors: &[[f32; 3]]) {
        for k in 0..n {
            // spread evenly over a sphere
            let y = 1.0 - 2.0 * (k as f32 + 0.5) / n as f32;
            let a = k as f32 * 2.39996;
            let r = (1.0 - y * y).sqrt();
            let v = vec3(a.cos() * r, y, a.sin() * r) * speed * (0.7 + 0.3 * ((k * 7) % 5) as f32 / 4.0);
            self.smoke.push(Smoke { pos, vel: v, age: 0.0, life: 0.9 + (k % 3) as f32 * 0.3, size: 1.2, color: colors[k % colors.len()], opacity: 0.8 });
        }
    }

    /// Add one to a kids-mode tally in the profile and return the new count.
    fn tally(&mut self, key: &str) -> u32 {
        let n = self.profile.number(key).unwrap_or(0.0) as u32 + 1;
        self.profile.set(key, &n.to_string());
        self.profile.save();
        n
    }

    /// Kids mode: one stick (pull back to climb), a zoom button, rainbow smoke and a change of
    /// plane; the missions talk to you.
    fn kids_frame(&mut self, dt: f32, drag: Vec2, dragging: bool, wheel: f32, just: &[PadBtn]) -> bool {
        use audio::Line;
        use KeyCode::*;
        let pad = |b: PadBtn| just.contains(&b);
        self.help = false;
        if is_key_pressed(Escape) || pad(PadBtn::Start) {
            self.to_menu();
            return true;
        }
        // 99 Red Balloons: R (or B) starts again, any time
        if self.kids.as_ref().is_some_and(|k| k.red.is_some()) && (is_key_pressed(R) || pad(PadBtn::East)) {
            self.start_kids(true);
            return true;
        }
        let axis = |pos: bool, neg: bool| pos as i32 as f32 - neg as i32 as f32;
        let p = &self.pad;
        // like a real plane: pull back (the stick down, or the down arrow) to climb
        let mut y = axis(key_any(&[Down, S]), key_any(&[Up, W])) - p.axis(PadAxis::LeftStickY);
        if self.invert {
            y = -y;
        }
        let x = axis(key_any(&[Right, D]), key_any(&[Left, A])) + p.axis(PadAxis::LeftStickX) + p.axis(PadAxis::RightStickX);
        let zoom = key_any(&[Space, LeftShift, RightShift]) || p.down(PadBtn::South) || p.value(PadBtn::RightTrigger2) > 0.3;
        let input = kids::Input { x: x.clamp(-1.0, 1.0), y: y.clamp(-1.0, 1.0), zoom };
        let marks = self.landmarks();
        let k = self.kids.as_mut().unwrap();
        if is_key_pressed(I) {
            self.invert = !self.invert;
            k.say(if self.invert { "Push to climb" } else { "Pull to climb" });
        }
        if is_key_pressed(X) || pad(PadBtn::West) {
            k.rainbow_smoke = !k.rainbow_smoke;
            self.audio.click();
        }
        // (in 99 Red Balloons, only at the start gate)
        let can_switch = k.red.as_ref().map_or(true, |r| !r.started);
        if !can_switch && (is_key_pressed(P) || is_key_pressed(Tab) || pad(PadBtn::North)) {
            k.say(if self.pad.connected() { "Pick the plane at the start: B restarts" } else { "Pick the plane at the start: R restarts" });
        }
        if can_switch && (is_key_pressed(P) || is_key_pressed(Tab) || pad(PadBtn::North)) {
            self.ac = k.next_vehicle(&self.ac, &self.world.terrain);
            k.say(kids::VEHICLES[k.vehicle].1);
            self.cam.reset(&self.ac);
            self.audio.ding();
            let line = match k.kind() {
                kids::Vehicle::Plane(Craft::Cessna) => Line::Plane,
                kids::Vehicle::Plane(Craft::Jet) => Line::Jet,
                kids::Vehicle::Plane(Craft::Fighter) => Line::Fighter,
                kids::Vehicle::Plane(Craft::Glider) => Line::Glider,
                kids::Vehicle::Toy(render::ToyKind::Helicopter) => Line::Heli,
                kids::Vehicle::Toy(render::ToyKind::HotAir) => Line::HotAir,
            };
            self.audio.kid(line, true);
        }
        let prev = self.ac.pos;
        let events = k.update(&mut self.ac, &input, &self.world.terrain, &marks, self.time, dt);

        // rainbow smoke from the tail, a puff every few metres
        if k.rainbow_smoke && !k.on_ground {
            let moved = self.ac.pos - prev;
            let tail = match k.toy() {
                Some(render::ToyKind::Helicopter) => -self.ac.forward() * 7.0 + self.ac.up() * 1.0,
                // from under the basket
                Some(render::ToyKind::HotAir) => -self.ac.up() * (self.ac.spec.rest_height + 0.5),
                None => -self.ac.forward() * (self.ac.spec.span * 0.3 + 2.0),
            };
            k.smoke_acc += moved.length();
            while k.smoke_acc > 2.5 {
                k.smoke_acc -= 2.5;
                k.hue = (k.hue + 0.01).fract();
                let back = k.smoke_acc / moved.length().max(1e-3);
                let pos = self.ac.pos - moved * back + tail;
                self.smoke.push(Smoke { pos, vel: Vec3::ZERO, age: 0.0, life: 3.5, size: 1.6, color: rainbow(k.hue), opacity: 0.85 });
            }
        }
        let confetti: Vec<[f32; 3]> = (0..6).map(|i| rainbow(i as f32 / 6.0)).collect();
        for e in events {
            match e {
                kids::Event::Pop(pos, c, n) => {
                    let k = self.kids.as_mut().unwrap();
                    let popped = k.progress;
                    self.audio.pop(n.unwrap_or(popped));
                    if let Some(n) = n {
                        self.audio.kid(Line::Count(n as u8), true);
                    }
                    // 99 Red Balloons: a cheer every ten
                    if k.red.is_some() && popped % 10 == 0 && popped < kids::RED_TOTAL {
                        k.say(format!("{popped}!"));
                        self.audio.ding();
                    }
                    self.tally("kids.balloons");
                    self.burst(pos, 18, 14.0, &[[c[0], c[1], c[2]], [1.0, 1.0, 1.0]]);
                }
                kids::Event::Rainbow(pos, n) => {
                    self.audio.ring(n);
                    self.burst(pos, 30, 16.0, &confetti);
                }
                kids::Event::NewQuest(q) => {
                    let (line, words) = match q {
                        kids::Quest::Balloons => (Line::Balloons, "Pop the balloons!"),
                        kids::Quest::Rainbows => (Line::Rainbows, "Fly through the rainbows!"),
                        kids::Quest::Find(kids::Landmark::Windmills) => (Line::Windmills, "Find the windmills!"),
                        kids::Quest::Find(kids::Landmark::Lighthouse) => (Line::Lighthouse, "Find the lighthouse!"),
                        kids::Quest::Find(kids::Landmark::Boat) => (Line::Boat, "Find the boat!"),
                        kids::Quest::Land => (Line::Land, "Time to land!"),
                    };
                    self.audio.kid(line, false);
                    self.kids.as_mut().unwrap().say(words);
                }
                kids::Event::QuestDone(q) => {
                    self.audio.chime();
                    let n = self.tally("kids.stars");
                    self.burst(self.ac.pos + Vec3::Y * 8.0, 60, 12.0, &confetti);
                    let cheers = [Line::Yay, Line::Hooray, Line::Great, Line::DidIt];
                    match q {
                        kids::Quest::Find(_) => self.audio.kid(Line::Found, false),
                        // the landing has its own cheer
                        kids::Quest::Land => {}
                        _ => self.audio.kid(cheers[n as usize % cheers.len()], false),
                    }
                    self.kids.as_mut().unwrap().say(match q {
                        kids::Quest::Find(_) => "You found it!",
                        kids::Quest::Land => "You landed!",
                        _ => "Yay!",
                    });
                }
                kids::Event::Liftoff => self.audio.kid(Line::Up, false),
                kids::Event::Touchdown => {
                    self.audio.touchdown(150.0);
                    self.wheel_smoke(8, [0.9, 0.9, 0.88]);
                }
                kids::Event::Missed => {
                    self.audio.click();
                    self.kids.as_mut().unwrap().say(format!("+{:.0} s", kids::MISS_PENALTY));
                }
                kids::Event::RedDone(time, popped) => {
                    self.audio.chime();
                    self.audio.kid(if popped == kids::RED_TOTAL { Line::Hooray } else { Line::Great }, false);
                    // the fastest lap (missed balloons cost time)
                    let best = self.profile.number("red.best");
                    let record = best.map_or(true, |b| time < b);
                    if record {
                        let plane = kids::VEHICLES[self.kids.as_ref().unwrap().vehicle].1;
                        self.profile.set("red.best", &time.to_string());
                        self.profile.set("red.popped", &popped.to_string());
                        self.profile.set("red.plane", plane);
                        self.profile.save();
                    }
                    // fireworks all round
                    for k in 0..6 {
                        let a = k as f32 * 1.05;
                        let at = self.ac.pos + vec3(a.cos() * 40.0, 20.0 + (k % 3) as f32 * 15.0, a.sin() * 40.0);
                        self.burst(at, 40, 16.0, &confetti);
                    }
                    let k = self.kids.as_mut().unwrap();
                    let t = lessons::fmt_time(time);
                    k.say(match (popped == kids::RED_TOTAL, record && best.is_some()) {
                        (true, true) => format!("All 99! New best: {t}"),
                        (true, false) => format!("All 99 in {t}!"),
                        (false, true) => format!("New best: {t}"),
                        (false, false) => format!("{popped} of 99: {t}"),
                    });
                }
                kids::Event::Landed => {
                    self.audio.kid(Line::Landed, false);
                    self.tally("kids.landings");
                    self.burst(self.ac.pos + Vec3::Y * 6.0, 48, 10.0, &confetti);
                }
            }
        }

        self.update_smoke(dt);
        self.cam.update(&self.ac, &self.world.terrain, dt, drag, dragging, wheel);
        if let Some((eye, target)) = self.shot_cam {
            self.cam.set_fixed(eye, target);
        }
        let k = self.kids.as_ref().unwrap();
        let ac = &self.ac;
        let toy = k.toy();
        let jet = ac.spec.is_jet() && toy.is_none();
        let engine = toy.is_none() && ac.spec.has_engine();
        self.audio.update(audio::State {
            rpm: if !engine { 0.0 } else if jet { 1.0 } else { 700.0 + 1900.0 * ac.ctl.throttle + ac.ias * 6.0 },
            throttle: ac.ctl.throttle,
            airspeed: ac.ias,
            roll_speed: if ac.on_ground && toy.is_none() { ac.ias } else { 0.0 },
            flap_deg: ac.flap_deg,
            distance: (self.cam.eye - ac.pos).length(),
            volume: 0.9,
            jet,
            n1: ac.n1,
            reverser: ac.reverser,
            gear: ac.gear,
            fan_hz: ac.spec.fan_hz,
            burner: ac.burner,
            rotor: if toy == Some(render::ToyKind::Helicopter) { if k.on_ground { 0.35 } else { 1.0 } } else { 0.0 },
            flame: k.flame,
            ..Default::default()
        });
        self.prev_pos = self.ac.pos;

        self.draw_world();
        hud::cloud_fade(self.world.cloud_density(self.cam.eye));
        let k = self.kids.as_ref().unwrap();
        let view = self.cam.view(screen_width() / screen_height());
        let target = k.target(self.ac.pos, self.time, &marks).map(|(p, c)| (p, Color::new(c[0], c[1], c[2], 1.0)));
        let pad_on = self.pad.connected();
        let red_done = k.red.as_ref().is_some_and(|r| r.done.is_some());
        let red_waiting = k.red.as_ref().is_some_and(|r| !r.started);
        let hint = if red_done {
            Some(if pad_on { "Press B to go again" } else { "Press R to go again" })
        } else if red_waiting {
            Some(if pad_on { "Move the stick to go!   (Y picks the plane)" } else { "Press an arrow to go!   (P picks the plane)" })
        } else if k.on_ground && k.speed < 0.5 {
            Some(if pad_on { "Pull back on the stick to fly!" } else { "Press DOWN to fly!" })
        } else if k.landing {
            Some("Landing - let go of the stick!")
        } else if k.toy().is_some() && k.quest == kids::Quest::Land && k.between.is_none() {
            Some(if pad_on { "Push the stick forward to land" } else { "Hold UP to land" })
        } else {
            None
        };
        hud::kids(&hud::KidsHud {
            view_proj: view.view_proj,
            eye: view.eye,
            time: self.time,
            quest: k.quest,
            progress: k.progress,
            colors: &k.progress_colors,
            done: k.red.as_ref().map_or(k.between, |r| r.done),
            red: k.red.as_ref().map(|r| hud::RedHud {
                popped: k.progress,
                missed: r.missed,
                time: r.total(),
                best: self.profile.number("red.best"),
            }),
            track: k.red.as_ref().map(|r| (&k.course[..], self.ac.pos, self.ac.heading_deg(), k.balloons.iter().skip(r.next).filter(|b| !b.popped).map(|b| b.pos).collect())),
            stars: self.profile.number("kids.stars").unwrap_or(0.0) as u32,
            target,
            banner: k.banner.as_ref().map(|(s, a)| (s.as_str(), *a)),
            hint,
            plane: kids::VEHICLES[k.vehicle].1,
            pad: pad_on,
        });
        true
    }

    /// Puffs from both main wheels.
    fn wheel_smoke(&mut self, n: usize, color: [f32; 3]) {
        let ac = &self.ac;
        let wheels = ac.spec.main_wheels();
        // bigger wheels, bigger puffs
        let scale = (wheels[0].x.abs() / 1.25).sqrt();
        for k in 0..n.min(24) {
            let pos = ac.pos + ac.rot * wheels[k % wheels.len()];
            let spread = vec3(((k * 7) % 5) as f32 - 2.0, 1.0 + (k % 3) as f32 * 0.5, ((k * 3) % 5) as f32 - 2.0) * 0.5 * scale;
            // the puff is left behind: it starts at a fraction of the plane's speed
            let vel = ac.vel * 0.15 + spread;
            self.smoke.push(Smoke { pos, vel, age: 0.0, life: (1.2 + (k % 4) as f32 * 0.3) * scale, size: 0.6 * scale, color, opacity: 0.55 });
        }
    }

    fn update_smoke(&mut self, dt: f32) {
        // dust kicked up rolling over grass
        let ac = &self.ac;
        let on_grass = ac.pos.x.abs() > RWY_HALF_LEN || ac.pos.z.abs() > RWY_HALF_W;
        let speed = ac.vel.length();
        if ac.on_ground && on_grass && speed > 4.0 && ac.crashed.is_none() {
            let rate = (speed / 25.0).min(1.0) * 20.0;
            if (self.time * rate).fract() < rate * dt {
                self.wheel_smoke(2, [0.66, 0.6, 0.48]);
            }
        }
        for p in &mut self.smoke {
            p.age += dt;
            p.vel *= (-1.6 * dt).exp();
            p.vel.y += 0.4 * dt;
            p.pos += p.vel * dt;
            p.size += dt * 2.2;
        }
        self.smoke.retain(|p| p.age < p.life);
        if self.smoke.len() > 300 {
            self.smoke.drain(..self.smoke.len() - 300);
        }
    }

    /// Play back the flight recorder: C changes camera, Left / Right the speed, Esc returns.
    fn replay_frame(&mut self, dt: f32, drag: Vec2, dragging: bool, wheel: f32, just: &[PadBtn]) -> bool {
        let pad = |b: PadBtn| just.contains(&b);
        let r = self.replay.as_mut().unwrap();
        if pressed_any(&[KeyCode::Right, KeyCode::D]) || pad(PadBtn::DPadRight) {
            r.speed = (r.speed * 2.0).min(4.0);
        }
        if pressed_any(&[KeyCode::Left, KeyCode::A]) || pad(PadBtn::DPadLeft) {
            r.speed = (r.speed * 0.5).max(0.125);
        }
        let pose = r.advance(dt);
        let (speed, progress) = (r.speed, r.progress());
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Space) || pad(PadBtn::East) || pad(PadBtn::RightThumb) {
            self.replay = None;
            return true;
        }
        if is_key_pressed(KeyCode::C) || pad(PadBtn::North) {
            self.cam.mode = self.cam.mode.next();
        }
        // show the recorded aircraft in place of the real one for this frame
        let mut shown = self.ac.clone();
        pose.apply(&mut shown);
        let real = std::mem::replace(&mut self.ac, shown);
        self.cam.update(&self.ac, &self.world.terrain, dt * speed, drag, dragging, wheel);
        let t = self.ac.telem(&self.world.terrain);
        self.update_audio(&t, false, dt);
        self.draw_world();
        if self.cam.mode == CamMode::Cockpit && self.hud_on {
            let view = self.cam.view(screen_width() / screen_height());
            hud::head_up(&hud::HeadUp {
                view_proj: view.view_proj,
                eye: view.eye,
                nose: self.ac.forward(),
                velocity: self.ac.vel,
                hdg: t.hdg,
                ias: t.ias_kt,
                alt_ft: t.alt_ft,
                vs_fpm: t.vs_fpm,
                ra_ft: Some(t.agl_ft),
                ils: lessons::ils(t.pos, t.hdg),
                speed_bug: None,
                on_ground: t.on_ground,
            });
        }
        self.ac = real;
        hud::replay_badge(speed, progress, self.cam.mode.name(), self.pad.connected());
        true
    }

    fn update_audio(&mut self, t: &Telem, running: bool, dt: f32) {
        let ac = &self.ac;
        let eye = self.cam.eye;
        let to_ear = eye - ac.pos;
        let distance = to_ear.length();
        // Doppler from the plane's motion relative to the camera (the chase camera rides along, so none there).
        let cam_vel = (eye - self.prev_eye) / dt.max(1e-3);
        self.prev_eye = eye;
        let closing = (ac.vel - cam_vel).dot(to_ear / distance.max(1.0));
        let doppler = if distance > 30.0 { (343.0 / (343.0 - closing.clamp(-120.0, 120.0))).clamp(0.75, 1.35) } else { 1.0 };
        let fly = ac.spec.fly_kt;
        let stall = if t.on_ground { 0.0 } else { smoothstep(t.stall_aoa - 4.0, t.stall_aoa + 2.0, t.aoa) * smoothstep(1.25 * fly, 2.0 * fly, t.ias_kt) };
        // spoilers shake the airframe too
        let buffet = stall.max(t.spoiler * (t.ias_kt / 150.0).min(1.0) * 0.55);
        let crashed = ac.crashed.is_some();
        self.audio.update(audio::State {
            rpm: if crashed || ac.engine_failed || !ac.spec.has_engine() { 0.0 } else if ac.spec.is_jet() { 1.0 } else { t.rpm },
            throttle: t.throttle,
            airspeed: ac.vel.length(),
            roll_speed: if t.on_ground { t.gs_kt * flight::KT } else { 0.0 },
            buffet,
            horn: t.stall_warn && running,
            slip: ac.beta.sin() * 4.0,
            flap_deg: t.flap_deg,
            trim: t.trim,
            inside: self.cam.mode == CamMode::Cockpit,
            distance,
            doppler,
            // keep sounding behind the crash and lesson dialogs; hush for pause and help
            volume: if self.paused || self.help { 0.0 } else { 1.0 },
            jet: ac.spec.is_jet(),
            n1: ac.n1,
            reverser: ac.reverser,
            gear: ac.gear,
            fan_hz: ac.spec.fan_hz,
            burner: ac.burner,
            vario: (!ac.spec.has_engine()).then_some(t.vario),
            rotor: 0.0,
            flame: 0.0,
        });
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
        let puffs: Vec<Puff> = self
            .smoke
            .iter()
            .map(|p| {
                let f = p.age / p.life;
                // thin out right in front of the camera (flying through your own smoke)
                let near = smoothstep(2.0, 12.0, (p.pos - self.cam.eye).length() - p.size);
                let a = p.opacity * (1.0 - f) * (p.age * 7.0).min(1.0) * near;
                Puff { pos: p.pos, size: p.size, color: [p.color[0], p.color[1], p.color[2], a] }
            })
            .collect();
        let sky = self.weather.sky();
        let ac = &self.ac;
        // landing lights: on with the gear down below 10,000 ft, or at night
        // (not on the rainbow road, where it lights up the road like a ghost balloon)
        let on_road = self.kids.as_ref().is_some_and(|k| k.red.is_some());
        let lamp = (self.screen == Screen::Flying && !on_road && ac.crashed.is_none() && (ac.gear > 0.9 && agl < 3000.0 || sky.night > 0.3) && (ac.spec.is_jet() || sky.night > 0.3))
            .then(|| (ac.pos + ac.forward() * (ac.spec.span * 0.1) - ac.up() * 0.6, (ac.forward() - ac.up() * 0.07).normalize()));
        let w = &self.weather;
        let cumulus = ((w.clouds * 1.6).min(1.0), w.cloud_base_ft * flight::FT - 1450.0);
        // the aircraft's own lights and the PAPI glow
        let mut lamps = Vec::new();
        if self.kids.as_ref().map_or(true, |k| k.toy().is_none()) {
            lights::aircraft_lamps(&self.ac, &mut lamps);
        }
        let traffic: &[Aircraft] = if self.traffic_on { &self.traffic.aircraft } else { &[] };
        for a in traffic {
            lights::aircraft_lamps(a, &mut lamps);
        }
        for (p, white) in &papi {
            let c = if *white { [1.0, 1.0, 0.95] } else { [1.0, 0.12, 0.08] };
            lamps.push(lights::Lamp { pos: *p + vec3(0.0, 1.0, 0.0), col: c, size: 1.5, blink: lights::Blink::Steady, facing: None, day: true });
        }
        let wind = self.weather.steady_wind(10.0);
        let balloons = self.kids.as_ref().map(|k| k.balloons.iter().filter(|b| !b.popped).map(|b| (b.at(self.time), b.color)).collect()).unwrap_or_default();
        let toy = self.kids.as_ref().and_then(|k| {
            let kind = k.toy()?;
            let ac = &self.ac;
            Some(render::Toy { kind, model: Mat4::from_rotation_translation(ac.rot, ac.pos - ac.up() * ac.spec.rest_height), rotor: k.rotor, flame: k.flame })
        });
        if let Some(k) = self.kids.as_ref().filter(|k| k.red.is_some()) {
            // rainbow arches over the start and the finish of the road
            for (i, j) in [(0usize, 8usize), (k.course.len() - 1, k.course.len() - 9)] {
                let (p, q) = (k.course[i], k.course[j]);
                let normal = if i == 0 { q - p } else { p - q }.normalize();
                for (b, c) in kids::COLORS.iter().enumerate() {
                    rings.push(RingDraw { pos: p + Vec3::Y * 4.0, normal, radius: 26.0 * (1.0 - b as f32 * 0.07), color: *c });
                }
            }
        }
        if let Some(k) = &self.kids {
            // rainbow rings: six bands each, red outside
            for r in k.rainbows.iter().filter(|r| !r.done) {
                for (i, c) in kids::COLORS.iter().enumerate() {
                    rings.push(RingDraw { pos: r.pos, normal: r.normal, radius: r.radius * (1.0 - i as f32 * 0.075), color: *c });
                }
            }
        }
        let scene = Scene {
            road: self.kids.as_ref().is_some_and(|k| k.red.is_some()),
            toy,
            balloons,
            ac: &self.ac,
            wind,
            traffic,
            ghost: if self.ghost_visible { self.ghost_ac.as_ref() } else { None },
            lamps: &lamps,
            sky: &sky,
            lamp,
            cumulus,
            puffs: &puffs,
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
        if self.cam.mode == CamMode::Cockpit && self.hud_on {
            let ac = &self.ac;
            hud::head_up(&hud::HeadUp {
                view_proj: view.view_proj,
                eye: view.eye,
                nose: ac.forward(),
                velocity: ac.vel,
                hdg: t.hdg,
                ias: t.ias_kt,
                alt_ft: t.alt_ft,
                vs_fpm: t.vs_fpm,
                ra_ft: Some(t.agl_ft),
                ils: lessons::ils(t.pos, t.hdg),
                speed_bug: lesson.vref.or((self.ap.at).then_some(self.ap.spd_kt)),
                on_ground: t.on_ground,
            });
        }
        let panel_top = hud::panel(t, &hud::PanelInfo { auto_rudder: self.auto_rudder, speed_bug: lesson.vref }, self.panel);
        let lesson_corner = hud::lesson_panel(lesson, &self.mission, t, self.coach_small);
        if lesson.kind == Kind::Rings && !self.mission.complete {
            let r = &self.world.rings[self.mission.next_ring];
            hud::ring_marker(&view.view_proj, r.pos, view.eye, panel_top);
            hud::rings_status(&self.mission, self.world.rings.len(), self.best_ring, lesson_corner);
        }
        let next_ring = (lesson.kind == Kind::Rings && !self.mission.complete).then_some(self.mission.next_ring);
        let traffic: Vec<Vec3> = if self.traffic_on { self.traffic.aircraft.iter().map(|a| a.pos).collect() } else { Vec::new() };
        hud::minimap(&self.minimap, &self.world, t, next_ring, !self.ac.spec.has_engine(), &traffic);
        hud::wind_box(&self.weather, t.hdg);
        hud::status_line(self.cam.mode.name(), self.fps.round() as i32, self.auto_rudder);
        let mcp_top = hud::mcp(&self.ap, self.ac.spec.is_jet(), panel_top, self.mcp_hover);
        hud::papi_widget(t, mcp_top - 30.0);
        hud::warnings(t, self.ac.warning.map(|w| w.0), self.time, sh * 0.2);
        hud::toasts(&self.toasts);

        let pad = self.pad.connected();
        if let Some(reason) = self.ac.crashed {
            let footer = if pad { "A  try again      R-stick  replay      B  menu" } else { "R / ENTER  try again      SPACE  replay      ESC  menu" };
            hud::dialog("CRASHED", hud::RED_C, &[reason.to_string()], footer);
        } else if self.mission.complete && !self.summary_dismissed {
            let title = match lesson.kind { Kind::Rings => "COURSE COMPLETE", Kind::Landing => "LANDED", _ => "LESSON COMPLETE" };
            let footer = match (pad, self.idx + 1 < self.lessons.len()) {
                (true, true) => "A keep flying    Y next    X retry    R-stick replay    B menu",
                (true, false) => "A keep flying    X retry    R-stick replay    B menu",
                (false, true) => "ENTER keep flying    N next    R retry    SPACE replay    ESC menu",
                (false, false) => "ENTER keep flying    R retry    SPACE replay    ESC menu",
            };
            let ex = hud::Extras { stars: self.result_stars, chart: self.debrief.as_ref() };
            hud::dialog_ex(title, hud::GREEN_C, &self.mission.summary, footer, &ex);
        } else if self.paused {
            let footer = if pad { "START  resume    X  restart    R-stick  replay    B  main menu" } else { "ESC / P  resume    SPACE  replay    BACKSPACE  main menu    H  help" };
            hud::dialog("PAUSED", hud::AMBER, &["Take a breather.".to_string()], footer);
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

    hud::init_fonts();
    let world = World::generate();
    let renderer = {
        let gl = unsafe { get_internal_gl() };
        render::Renderer::new(gl.quad_context, &world)
    };
    let minimap = make_minimap(&world);
    let traffic = traffic::Traffic::new(&world);
    let audio = audio::Audio::new();
    let lessons = lessons::all();
    let profile = setup::Profile::load();
    let ac = Aircraft::on_ground_at(Craft::Cessna, &world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
    let mut app = App {
        world,
        renderer,
        minimap,
        audio,
        lessons,
        screen: Screen::Menu,
        sel: 0,
        menu_page: MenuPage::Main,
        setup_sel: 0,
        free: profile.setup(),
        best_ring: profile.number("best_ring"),
        profile,
        idx: 0,
        ac,
        mission: Mission::default(),
        cam: CameraRig::new(),
        stick: Stick::default(),
        pad: Pad::new(),
        auto_rudder: false,
        acc: 0.0,
        time: 0.0,
        paused: false,
        help: false,
        panel: hud::PanelMode::Full,
        invert: false,
        summary_dismissed: false,
        crash_noted: false,
        toasts: Vec::new(),
        last_mouse: Vec2::ZERO,
        prev_pos: Vec3::ZERO,
        prev_eye: Vec3::ZERO,
        fps: 60.0,
        smoke: Vec::new(),
        gpws: gpws::Gpws::default(),
        weather: weather::Weather::default(),
        sim_time: 0.0,
        ap: ap::Autopilot::default(),
        mcp_hover: None,
        bug_hold: 0.0,
        hud_on: true,
        coach_small: false,
        landing: score::LandingLog::default(),
        recorder: replay::Recorder::default(),
        replay: None,
        touch_time: None,
        shot_cam: None,
        tower: atc::Tower::default(),
        traffic,
        traffic_on: false,
        run: ghost::Run::default(),
        ghost: None,
        ghost_ac: None,
        show_ghost: true,
        ghost_visible: false,
        done_at: 0.0,
        approach: debrief::ApproachLog::default(),
        debrief: None,
        result_stars: None,
        kids: None,
        song: None,
    };

    // --unlock-all opens every aircraft for good (it's saved in the profile)
    if args.iter().any(|a| a == "--unlock-all") {
        app.profile.set("unlock_all", "1");
        app.profile.save();
    }
    let mut frames = 0;
    if let Some((_, Some(scene), cam)) = &shot {
        request_new_screen_size(1280.0, 800.0);
        app.audio.muted = true;
        if let Ok(i) = scene.parse::<usize>() {
            app.start(i);
        } else if scene == "setup" {
            app.screen = Screen::Setup;
        } else if scene == "kids" {
            app.start_kids(false);
        } else if scene == "red" {
            app.start_kids(true);
        } else if scene == "objectives" {
            app.menu_page = MenuPage::Objectives;
            app.sel = 1;
        }
        // extra key=value arguments set the weather: hour=19.5 clouds=0.8 wind=15 from=30 vis=8
        for a in &args {
            if let Some((k, v)) = a.split_once('=') {
                // eye=x,y,z:tx,ty,tz fixes the camera
                if k == "eye" {
                    let n: Vec<f32> = v.split([',', ':']).filter_map(|x| x.parse().ok()).collect();
                    if n.len() == 6 {
                        app.shot_cam = Some((vec3(n[0], n[1], n[2]), vec3(n[3], n[4], n[5])));
                    }
                    continue;
                }
                let v: f32 = v.parse().unwrap_or(0.0);
                let w = &mut app.weather;
                match k {
                    "hour" => w.hour = v,
                    "clouds" => w.clouds = v,
                    "base" => w.cloud_base_ft = v,
                    "wind" => w.wind_kt = v,
                    "from" => w.wind_from = v,
                    "vis" => w.visibility_km = v,
                    // put the aircraft on the glide path this many metres from the PAPI
                    "final" => {
                        let ac = &app.ac;
                        let (craft, ias, flaps) = (ac.craft, ac.ias / flight::KT, ac.ctl.flaps);
                        let pos = vec3(-RWY_HALF_LEN + PAPI_DIST - v, RWY_ELEV + ac.spec.rest_height + v * 3f32.to_radians().tan(), 0.0);
                        app.ac = Aircraft::in_flight(craft, pos, 90.0, ias, -3.0, flaps);
                        app.cam.reset(&app.ac);
                    }
                    // free flight in aircraft N from start M
                    "craft" | "place" => {
                        if k == "craft" {
                            app.free.craft = v as usize % setup::AIRCRAFT.len();
                        } else {
                            app.free.place = v as usize % setup::PLACES.len();
                        }
                        let i = app.lessons.iter().position(|l| l.kind == Kind::Free).unwrap_or(0);
                        app.start(i);
                    }
                    // full afterburner
                    "ab" => {
                        app.ac.ctl.throttle = 1.0;
                        app.ac.n1 = 1.0;
                        app.ac.burner = 1.0;
                    }
                    "sel" => app.sel = v as usize,
                    // kids mode: in the air this far short of the first balloon, heading east
                    "kids" => {
                        if let Some(k) = &mut app.kids {
                            let b = k.balloons[0].pos;
                            k.fly_from(&mut app.ac, b - vec3(v, 6.0, 8.0), 90.0);
                            if let Some(r) = &mut k.red {
                                r.started = true;
                            }
                            app.cam.reset(&app.ac);
                        }
                    }
                    // kids mode: fly vehicle N, on mission N
                    "vehicle" => {
                        if let Some(k) = &mut app.kids {
                            for _ in 0..v as usize {
                                app.ac = k.next_vehicle(&app.ac, &app.world.terrain);
                            }
                            app.cam.reset(&app.ac);
                        }
                    }
                    "quest" => {
                        let marks = app.landmarks();
                        if let Some(k) = &mut app.kids {
                            k.jump_to(v as usize, &app.world.terrain, &app.ac, &marks);
                        }
                    }
                    // kids mode: on approach to runway 09, this far out
                    "kidsland" => {
                        if let Some(k) = &mut app.kids {
                            k.fly_from(&mut app.ac, vec3(-RWY_HALF_LEN - v, RWY_ELEV + 30.0 + v * 0.06, 25.0), 90.0);
                            app.cam.reset(&app.ac);
                        }
                    }
                    // engage the autopilot (and autothrottle, approach mode)
                    "ap" => {
                        app.ap.sync(&app.ac);
                        app.ap.engage(&app.ac);
                        app.ap.toggle_at(&app.ac);
                        app.ap.toggle_app();
                    }
                    _ => {}
                }
            }
        }

        app.cam.mode = match cam.as_deref() {
            Some("cockpit") => CamMode::Cockpit,
            Some("orbit") => CamMode::Orbit,
            Some("tower") => CamMode::Tower,
            _ => CamMode::Chase,
        };
        app.cam.reset(&app.ac);
    }

    // speed=4 runs the simulation faster, frames=N picks the frame to capture
    let arg = |k: &str| args.iter().find_map(|a| a.strip_prefix(k)?.strip_prefix('=')?.parse::<f32>().ok());
    let speed = arg("speed").unwrap_or(1.0);
    let capture = arg("frames").map_or(150, |f| f as u32);
    loop {
        let dt = get_frame_time().min(0.1) * if shot.is_some() { speed } else { 1.0 };
        if !app.frame(dt) {
            break;
        }
        frames += 1;
        if frames == 1 && args.iter().any(|a| a == "help") {
            app.help = true;
        }
        if frames == 100 && args.iter().any(|a| a == "replay") {
            app.replay = app.recorder.replay(25.0, app.touch_time);
        }
        if let Some((Some(path), _, _)) = &shot {
            if frames == capture {
                get_screen_data().export_png(path);
                break;
            }
        }
        next_frame().await;
    }
}
