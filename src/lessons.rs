//! Guided lessons, the ring challenge and free flight.

use crate::flight::{Touchdown, Telem, FT};
use crate::world::*;
use macroquad::math::{vec3, Vec3};

pub enum Start {
    Runway,
    Air { pos: Vec3, heading: f32, ias: f32, gamma: f32, flaps: u8, trim_offset: f32 },
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Lesson,
    Rings,
    Free,
}

#[derive(Default, Clone)]
pub struct Scratch {
    pub f: [f32; 4],
}

type Check = fn(&Telem) -> bool;
type Hint = fn(&Telem) -> Option<&'static str>;
type Track = fn(&Telem, &mut Scratch, f32);

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub hold: f32,
    pub done: Check,
    pub hint: Hint,
    pub track: Track,
}

pub struct Lesson {
    pub name: &'static str,
    pub blurb: &'static str,
    pub start: Start,
    pub kind: Kind,
    pub steps: Vec<Step>,
    pub summary: fn(&Scratch, Option<Touchdown>) -> Vec<String>,
}

fn step(title: &'static str, text: &'static str, hold: f32, done: Check, hint: Hint) -> Step {
    Step { title, text, hold, done, hint, track: |_, _, _| {} }
}

pub fn hdg_err(h: f32, target: f32) -> f32 {
    ((h - target + 540.0) % 360.0 - 180.0).abs()
}

/// PAPI light positions for runway 09 (west end), nearest the runway first,
/// with the glide-path angle above which each light shows white.
pub fn papi_lights() -> [(Vec3, f32); 4] {
    let x = -RWY_HALF_LEN + PAPI_DIST;
    let ang = [3.5, 3.17, 2.83, 2.5];
    std::array::from_fn(|i| (vec3(x, RWY_ELEV, -(RWY_HALF_W + 14.0 + i as f32 * 9.0)), ang[i]))
}

/// Angle (deg) of the aircraft above the runway-09 PAPI, as seen from the lights.
pub fn glide_angle(pos: Vec3) -> f32 {
    let x = -RWY_HALF_LEN + PAPI_DIST;
    let dx = (x - pos.x).max(1.0);
    (pos.y - 1.4 - RWY_ELEV).atan2(dx).to_degrees()
}

fn no_summary(_: &Scratch, _: Option<Touchdown>) -> Vec<String> {
    Vec::new()
}

const CRUISE_ALT: f32 = 4000.0;

pub fn all() -> Vec<Lesson> {
    vec![
        Lesson {
            name: "1. First Flight: Takeoff",
            blurb: "Power up, roll down the runway, rotate and climb away.",
            start: Start::Runway,
            kind: Kind::Lesson,
            steps: vec![
                step("Full power", "Hold SHIFT (or =) to push the throttle all the way in. The engine roars and the plane starts to roll.", 0.0,
                    |t| t.throttle > 0.95, |_| None),
                step("Stay on the centerline", "Steer with the rudder pedals, Q and E. On the ground they also turn the nose wheel. Watch the airspeed (first gauge) build.", 0.0,
                    |t| t.ias_kt > 50.0,
                    |t| if t.pos.z.abs() > 6.0 { Some("Drifting off the centerline - steer back with Q / E.") } else { None }),
                Step {
                    track: |t, s, _| if !t.on_ground && s.f[0] == 0.0 { s.f[0] = t.ias_kt },
                    ..step("Rotate", "At about 55 kt pull back gently (S or DOWN) to raise the nose about 10 degrees. Hold that attitude and the plane flies itself off the runway.", 1.0,
                        |t| !t.on_ground && t.agl_ft > 15.0,
                        |t| if t.pitch > 14.0 { Some("Too much! Ease off or you'll strike the tail.") }
                            else if t.on_ground && t.ias_kt > 68.0 { Some("Pull back now to lift off!") }
                            else if t.throttle < 0.9 { Some("Keep full power for takeoff.") } else { None })
                },
                step("Climb at 75 kt", "PITCH CONTROLS AIRSPEED: nose up = slower, nose down = faster. Hold 70-80 kt and climb to 1,000 ft above the ground.", 0.0,
                    |t| t.agl_ft > 1000.0,
                    |t| if t.ias_kt < 62.0 { Some("Too slow! Lower the nose a little (W).") }
                        else if t.ias_kt > 90.0 { Some("Too fast - raise the nose a little (S) to climb better.") }
                        else if t.bank.abs() > 15.0 { Some("Keep the wings level with A / D.") } else { None }),
                step("Level off", "Lower the nose to the horizon, then reduce power to about 75% (CTRL or -). Use trim (T / G) so you don't have to hold the stick.", 5.0,
                    |t| t.vs_fpm.abs() < 200.0 && t.throttle < 0.85 && !t.on_ground,
                    |t| if t.throttle >= 0.85 { Some("Reduce power to about 75% with CTRL or -.") }
                        else if t.vs_fpm > 250.0 { Some("Still climbing - lower the nose a little.") }
                        else if t.vs_fpm < -250.0 { Some("Descending - raise the nose a little.") } else { None }),
            ],
            summary: |s, _| vec![format!("Lifted off at {:.0} kt.", s.f[0]), "You flew! Next: Straight & Level and Trim.".into()],
        },
        Lesson {
            name: "2. Straight & Level, and Trim",
            blurb: "Use trim to fly hands-off and hold altitude and heading.",
            start: Start::Air { pos: vec3(-500.0, CRUISE_ALT * FT, 3000.0), heading: 0.0, ias: 100.0, gamma: 0.0, flaps: 0, trim_offset: -0.04 },
            kind: Kind::Lesson,
            steps: vec![
                step("Feel the trim", "Let go of the stick and watch: the nose slowly drops because the plane is trimmed nose-down. Hold T (nose-up trim) until the vertical speed gauge (VSI) stays near zero with the stick centered.", 5.0,
                    |t| t.stick_pitch.abs() < 0.05 && t.vs_fpm.abs() < 150.0,
                    |t| if t.stick_pitch.abs() > 0.05 { Some("Let go of W / S - let the trim do the work.") }
                        else if t.vs_fpm < -200.0 { Some("Descending: hold T for nose-up trim.") }
                        else if t.vs_fpm > 200.0 { Some("Climbing: hold G for nose-down trim.") } else { None }),
                step("Hold it steady", "Keep within 100 ft of 4,000 ft and within 10 degrees of heading 360 for 20 seconds. Pick a spot on the horizon and keep the nose on it. Small corrections!", 20.0,
                    |t| (t.alt_ft - CRUISE_ALT).abs() < 100.0 && hdg_err(t.hdg, 0.0) < 10.0,
                    |t| if t.alt_ft < CRUISE_ALT - 100.0 { Some("Too low - raise the nose a touch and climb back to 4,000 ft.") }
                        else if t.alt_ft > CRUISE_ALT + 100.0 { Some("Too high - lower the nose a touch.") }
                        else if hdg_err(t.hdg, 0.0) >= 10.0 { Some("Turn back to heading 360 (N) with a gentle bank.") } else { None }),
                step("Slow down, stay level", "Reduce power to about 55%. The plane slows and starts to sink - raise the nose a bit and re-trim (T) to hold 4,000 ft at the slower speed.", 6.0,
                    |t| t.throttle < 0.6 && t.vs_fpm.abs() < 150.0 && (t.alt_ft - CRUISE_ALT).abs() < 150.0 && t.ias_kt < 92.0,
                    |t| if t.throttle >= 0.6 { Some("Pull the power back to about 55% (CTRL or -).") }
                        else if (t.alt_ft - CRUISE_ALT).abs() >= 150.0 { Some("Get back to 4,000 ft.") }
                        else if t.vs_fpm.abs() >= 150.0 { Some("Re-trim (T / G) until the VSI stays near zero.") } else { None }),
            ],
            summary: |_, _| vec!["Set the attitude, then trim away the pressure.".into(), "Less power at the same altitude = slower speed, more nose-up trim.".into()],
        },
        Lesson {
            name: "3. Turns",
            blurb: "Bank to turn, hold altitude, and keep the ball centered.",
            start: Start::Air { pos: vec3(-1500.0, CRUISE_ALT * FT, 0.0), heading: 0.0, ias: 100.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Lesson,
            steps: vec![
                step("Bank left", "Roll left with A to 20-30 degrees of bank (watch the attitude indicator), then center the stick. The bank stays - the tilted lift turns the plane.", 2.0,
                    |t| (-35.0..=-15.0).contains(&t.bank) && t.stick_roll.abs() < 0.2,
                    |t| if t.bank < -40.0 { Some("Too steep - roll back right a little.") }
                        else if t.stick_roll.abs() >= 0.2 && t.bank < -15.0 { Some("Good bank - now let go of A.") } else { None }),
                step("Turn to 270 (west)", "Hold the bank and add a little back pressure (S) to keep altitude - part of the lift is now turning you. Start rolling out about 10 degrees before 270.", 0.0,
                    |t| hdg_err(t.hdg, 270.0) < 15.0,
                    |t| if t.vs_fpm < -300.0 { Some("Losing altitude - add a little back pressure (S).") }
                        else if t.bank > -10.0 { Some("Keep the bank in until you're nearly at 270.") } else { None }),
                step("Roll out on 270", "Roll the wings level with D and hold heading 270.", 2.0,
                    |t| t.bank.abs() < 5.0 && hdg_err(t.hdg, 270.0) < 10.0,
                    |t| if t.bank.abs() >= 5.0 { Some("Level the wings.") } else { None }),
                Step {
                    track: |t, s, dt| if t.bank.abs() > 10.0 { s.f[0] += t.slip.abs() * dt; s.f[1] += dt },
                    ..step("Coordinated turn right", "Turn right to heading 090 at 20-30 degrees of bank. Ailerons cause ADVERSE YAW, so the ball in the turn coordinator slides. 'Step on the ball': press the rudder on the side the ball went (Q left, E right).", 2.0,
                        |t| hdg_err(t.hdg, 90.0) < 10.0 && t.bank.abs() < 5.0,
                        |t| if t.slip > 0.2 { Some("Ball is right: step on the right rudder (E).") }
                            else if t.slip < -0.2 { Some("Ball is left: press left rudder (Q).") }
                            else if t.bank > 35.0 { Some("A bit steep - 20-30 degrees is plenty.") }
                            else if t.vs_fpm < -300.0 { Some("Losing altitude - a little back pressure.") } else { None })
                },
            ],
            summary: |s, _| {
                let avg = if s.f[1] > 0.0 { s.f[0] / s.f[1] * 100.0 } else { 0.0 };
                vec![
                    format!("Average ball offset in the right turn: {avg:.0}%"),
                    if avg < 15.0 { "Nicely coordinated!".into() } else { "Practise with Q / E, or press Y for auto-rudder.".into() },
                ]
            },
        },
        Lesson {
            name: "4. Climbs & Descents",
            blurb: "Pitch for airspeed, power for altitude.",
            start: Start::Air { pos: vec3(-2500.0, 3500.0 * FT, 1500.0), heading: 90.0, ias: 95.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Lesson,
            steps: vec![
                step("Climb at 75 kt", "Full power (SHIFT), then raise the nose until the airspeed settles between 70 and 80 kt. That's the best rate-of-climb speed, Vy.", 5.0,
                    |t| t.throttle > 0.9 && (68.0..=82.0).contains(&t.ias_kt) && t.vs_fpm > 400.0,
                    |t| if t.throttle <= 0.9 { Some("Full power first: hold SHIFT.") }
                        else if t.ias_kt > 82.0 { Some("Raise the nose to slow to 75 kt.") }
                        else if t.ias_kt < 68.0 { Some("Lower the nose a bit to speed up.") } else { None }),
                step("Level at 4,500 ft", "Approaching 4,500 ft, lower the nose to the horizon, let the speed build, then reduce power to about 75%.", 5.0,
                    |t| (t.alt_ft - 4500.0).abs() < 120.0 && t.vs_fpm.abs() < 200.0,
                    |t| if t.alt_ft < 4380.0 { Some("Keep climbing to 4,500 ft.") }
                        else if t.alt_ft > 4620.0 { Some("Too high - come back down to 4,500 ft.") }
                        else if t.throttle > 0.85 { Some("Reduce power to about 75%.") } else { None }),
                step("Descend at 90 kt", "Power controls altitude: reduce power to about 35%. Keep the nose where the airspeed stays near 90 kt and you'll descend at a steady rate.", 5.0,
                    |t| t.throttle < 0.45 && t.vs_fpm < -300.0 && (82.0..=98.0).contains(&t.ias_kt),
                    |t| if t.throttle >= 0.45 { Some("Reduce power to about 35%.") }
                        else if t.ias_kt > 98.0 { Some("Too fast - raise the nose a bit.") }
                        else if t.ias_kt < 82.0 { Some("Too slow - lower the nose a bit.") } else { None }),
                step("Level at 3,500 ft", "Lead the level-off by about 100 ft: add power to about 75% and raise the nose to the horizon.", 5.0,
                    |t| (t.alt_ft - 3500.0).abs() < 120.0 && t.vs_fpm.abs() < 200.0,
                    |t| if t.alt_ft > 3620.0 { Some("Keep descending to 3,500 ft.") }
                        else if t.alt_ft < 3380.0 { Some("Too low - climb back to 3,500 ft.") }
                        else if t.throttle < 0.6 { Some("Add power to about 75% to hold altitude.") } else { None }),
            ],
            summary: |_, _| vec!["Pitch + power = performance.".into(), "Pitch sets the speed, power decides whether you climb or descend.".into()],
        },
        Lesson {
            name: "5. Stalls",
            blurb: "See what a stall is, hear the warning, and recover.",
            start: Start::Air { pos: vec3(1500.0, 4200.0 * FT, -1500.0), heading: 270.0, ias: 90.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Lesson,
            steps: vec![
                step("Power to idle", "Pull the throttle all the way back (hold CTRL or -).", 0.0, |t| t.throttle < 0.05, |_| None),
                step("Slow down, hold altitude", "Gradually pull back (S) to keep the altitude constant as the plane slows. Watch the AoA (angle of attack) bar rise. The stall horn sounds a few knots before the stall.", 0.5,
                    |t| t.stall_warn,
                    |t| if t.vs_fpm < -400.0 { Some("Pull back more to hold altitude.") } else if t.throttle > 0.05 { Some("Keep the power at idle.") } else { None }),
                Step {
                    track: |t, s, _| s.f[0] = t.alt_ft,
                    ..step("The stall", "Keep pulling back... the wing passes its critical angle of attack, the airflow separates and lift collapses. The nose drops - that's the stall.", 0.0,
                        |t| t.stalled,
                        |t| if t.stick_pitch < 0.5 { Some("Keep pulling back (S).") } else { None })
                },
                Step {
                    track: |t, s, _| if s.f[1] == 0.0 || t.alt_ft < s.f[1] { s.f[1] = t.alt_ft },
                    ..step("Recover!", "Push forward (W) to lower the nose and reduce the angle of attack, add full power, and level the wings. Then gently pull out of the dive.", 2.0,
                        |t| !t.stalled && t.ias_kt > 65.0 && t.bank.abs() < 15.0 && t.vs_fpm > -200.0 && t.throttle > 0.9,
                        |t| if t.stalled { Some("Still stalled: push forward (W)!") }
                            else if t.throttle <= 0.9 { Some("Full power: SHIFT.") }
                            else if t.bank.abs() >= 15.0 { Some("Level the wings with A / D.") }
                            else if t.vs_fpm <= -200.0 && t.ias_kt > 70.0 { Some("Gently pull out of the dive (S).") } else { None })
                },
            ],
            summary: |s, _| vec![
                format!("Stalled at {:.0} ft, lowest point {:.0} ft: {:.0} ft lost.", s.f[0], s.f[1], (s.f[0] - s.f[1]).max(0.0)),
                "A stall is about ANGLE OF ATTACK, not speed. To recover, lower the nose.".into(),
            ],
        },
        Lesson {
            name: "6. Landing",
            blurb: "Fly the final approach, flare, and touch down.",
            start: Start::Air {
                pos: vec3(-RWY_HALF_LEN + PAPI_DIST - 2700.0, RWY_ELEV + 1.4 + 2700.0 * 3f32.to_radians().tan(), 0.0),
                heading: 90.0, ias: 70.0, gamma: -3.0, flaps: 2, trim_offset: 0.0,
            },
            kind: Kind::Lesson,
            steps: vec![
                step("Stabilized approach", "You're on final for runway 09. Keep 65-75 kt with pitch and use power to follow the glide path. The PAPI lights left of the runway (also on screen): 2 white + 2 red = on path. More white = high, more red = low.", 0.0,
                    |t| t.pos.x > -RWY_HALF_LEN - 120.0,
                    |t| {
                        let a = glide_angle(t.pos);
                        if t.ias_kt < 62.0 { Some("Too slow! Lower the nose.") }
                        else if t.ias_kt > 80.0 { Some("Too fast - raise the nose a little.") }
                        else if a > 3.5 { Some("Too high (all white): reduce power.") }
                        else if a < 2.5 { Some("Too low (all red): add power!") }
                        else if t.pos.z.abs() > 25.0 { Some("Line up with the centerline - gentle bank with A / D.") } else { None }
                    }),
                step("Flare and touch down", "Over the threshold: throttle to idle. When the runway is a few feet below, gently raise the nose to FLARE and let the plane settle onto the main wheels.", 1.0,
                    |t| t.on_ground,
                    |t| if t.throttle > 0.1 && t.agl_ft < 60.0 { Some("Throttle to idle (CTRL or -).") }
                        else if t.agl_ft < 25.0 && t.vs_fpm < -400.0 { Some("Flare! Raise the nose (S).") } else { None }),
                step("Roll out and stop", "Keep straight with the rudder (Q / E) and hold B to brake until you stop.", 0.0,
                    |t| t.on_ground && t.gs_kt < 2.0,
                    |t| if t.pos.z.abs() > 10.0 { Some("Steer back toward the centerline.") } else if t.throttle > 0.05 { Some("Throttle to idle.") } else { None }),
            ],
            summary: |_, td| match td {
                None => vec!["Landed.".into()],
                Some(td) => {
                    let grade = match td.sink_fpm {
                        s if s < 150.0 => "Butter! A greaser.",
                        s if s < 350.0 => "Nice, smooth landing.",
                        s if s < 550.0 => "A firm arrival - flare a little more.",
                        _ => "Ouch! Flare earlier to slow the descent.",
                    };
                    let past = td.pos.x + RWY_HALF_LEN;
                    vec![
                        format!("Touchdown: {:.0} fpm descent, {:.0} kt.", td.sink_fpm, td.ias_kt),
                        grade.into(),
                        if past < 0.0 { format!("Landed {:.0} m SHORT of the runway!", -past) } else { format!("{past:.0} m past the threshold, {:.1} m off centerline.", td.pos.z.abs()) },
                    ]
                }
            },
        },
        Lesson {
            name: "Ring Challenge",
            blurb: "Race through the rings around the island. Beat your best time!",
            start: Start::Air { pos: RING_START, heading: 90.0, ias: 105.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Rings,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Free Flight: from the runway",
            blurb: "Lined up on runway 09. The sky is yours.",
            start: Start::Runway,
            kind: Kind::Free,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Free Flight: airborne",
            blurb: "Start cruising at 3,000 ft west of the airport.",
            start: Start::Air { pos: vec3(-3500.0, 3000.0 * FT, 800.0), heading: 90.0, ias: 100.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Free,
            steps: vec![],
            summary: no_summary,
        },
    ]
}

pub enum Event {
    StepDone,
    LessonDone,
    Ring,
    RingsDone,
}

#[derive(Default)]
pub struct Mission {
    pub step: usize,
    pub hold: f32,
    pub scratch: Scratch,
    pub complete: bool,
    pub summary: Vec<String>,
    pub time: f32,
    pub next_ring: usize,
}

impl Mission {
    pub fn update(&mut self, lesson: &Lesson, t: &Telem, prev: Vec3, world: &World, touchdown: Option<Touchdown>, best: &mut Option<f32>, dt: f32) -> Option<Event> {
        if self.complete {
            return None;
        }
        self.time += dt;
        match lesson.kind {
            Kind::Free => None,
            Kind::Rings => {
                let ring = &world.rings[self.next_ring];
                let (d0, d1) = ((prev - ring.pos).dot(ring.normal), (t.pos - ring.pos).dot(ring.normal));
                if d0 <= 0.0 && d1 > 0.0 {
                    let hit = prev + (t.pos - prev) * (d0 / (d0 - d1));
                    if (hit - ring.pos).length() < ring.radius {
                        self.next_ring += 1;
                        if self.next_ring == world.rings.len() {
                            self.complete = true;
                            let record = best.map_or(true, |b| self.time < b);
                            if record {
                                *best = Some(self.time);
                            }
                            self.summary = vec![
                                format!("All {} rings in {}", world.rings.len(), fmt_time(self.time)),
                                if record { "New best time!".into() } else { format!("Best: {}", fmt_time(best.unwrap_or(0.0))) },
                            ];
                            return Some(Event::RingsDone);
                        }
                        return Some(Event::Ring);
                    }
                }
                None
            }
            Kind::Lesson => {
                let s = &lesson.steps[self.step];
                (s.track)(t, &mut self.scratch, dt);
                let ok = (s.done)(t);
                self.hold = if ok { self.hold + dt } else { 0.0 };
                if ok && self.hold >= s.hold {
                    self.hold = 0.0;
                    self.step += 1;
                    if self.step == lesson.steps.len() {
                        self.complete = true;
                        self.summary = (lesson.summary)(&self.scratch, touchdown);
                        return Some(Event::LessonDone);
                    }
                    return Some(Event::StepDone);
                }
                None
            }
        }
    }

    pub fn skip_step(&mut self, lesson: &Lesson) {
        if lesson.kind == Kind::Lesson && !self.complete && self.step + 1 < lesson.steps.len() {
            self.step += 1;
            self.hold = 0.0;
        }
    }
}

pub fn fmt_time(t: f32) -> String {
    format!("{}:{:05.2}", (t / 60.0) as u32, t % 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::{Aircraft, KT};

    #[test]
    fn scripted_pilot_completes_takeoff_lesson() {
        let world = World::generate();
        let lessons = all();
        let lesson = &lessons[0];
        let mut ac = Aircraft::on_ground_at(&world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
        let mut m = Mission::default();
        let mut best = None;
        let dt = 1.0 / 240.0;
        let mut prev = ac.pos;
        for _ in 0..(240 * 180) {
            let t = ac.telem(&world.terrain);
            let c = &mut ac.ctl;
            match m.step {
                0 | 1 => c.throttle = (c.throttle + dt).min(1.0),
                _ => {}
            }
            // hold a pitch attitude: 8 deg after liftoff, then whatever gives 75 kt
            let want_pitch = if t.agl_ft < 100.0 { 8.0 } else { 8.0 + (t.ias_kt - 75.0) * 0.5 };
            c.pitch = if t.on_ground { if t.ias_kt > 55.0 { 0.5 } else { 0.0 } } else if m.step < 4 { ((want_pitch - t.pitch) * 0.06 - ac.omega.x * 0.8).clamp(-0.5, 0.6) } else { ((2.0 - t.vs_fpm * 0.004 - t.pitch) * 0.05 - ac.omega.x * 0.8).clamp(-0.4, 0.4) };
            if m.step == 4 {
                c.throttle = 0.7;
            }
            c.roll = (-t.bank * 0.05).clamp(-0.5, 0.5);
            c.yaw = if t.on_ground { (-t.pos.z * 0.2 - (t.hdg - 90.0) * 0.1).clamp(-1.0, 1.0) } else { 0.0 };
            ac.step(dt, &world.terrain);
            assert!(ac.crashed.is_none(), "crashed: {:?}", ac.crashed);
            let t = ac.telem(&world.terrain);
            m.update(lesson, &t, prev, &world, None, &mut best, dt);
            prev = ac.pos;
            if m.complete {
                break;
            }
        }
        assert!(m.complete, "stuck at step {} ({}), ias {:.0}", m.step, lesson.steps[m.step].title, ac.ias / KT);
        println!("summary: {:?}", m.summary);
    }

    #[test]
    fn rings_are_above_terrain() {
        let world = World::generate();
        for r in &world.rings {
            assert!(r.pos.y - world.terrain.surface(r.pos.x, r.pos.z) > 100.0);
        }
    }

    #[test]
    fn approach_path_is_clear() {
        let world = World::generate();
        let t = &world.terrain;
        for i in 0..100 {
            let d = i as f32 * 30.0;
            let x = -RWY_HALF_LEN + PAPI_DIST - d;
            let glide = RWY_ELEV + d * 3f32.to_radians().tan();
            assert!(t.surface(x, 0.0) < glide - 1.0 || d < 350.0, "terrain {} vs glide {} at {}", t.surface(x, 0.0), glide, x);
        }
    }

    #[test]
    fn scripted_pilot_lands() {
        let world = World::generate();
        let lessons = all();
        let lesson = &lessons[5];
        let Start::Air { pos, heading, ias, gamma, flaps, .. } = lesson.start else { panic!() };
        let mut ac = Aircraft::in_flight(pos, heading, ias, gamma, flaps);
        let mut m = Mission::default();
        let mut best = None;
        let dt = 1.0 / 240.0;
        let mut prev = ac.pos;
        for _ in 0..(240 * 240) {
            let t = ac.telem(&world.terrain);
            let q = ac.omega.x;
            let c = &mut ac.ctl;
            if t.agl_ft > 25.0 && !t.on_ground {
                // power for glide path, pitch for speed
                let err = glide_angle(t.pos) - 3.0;
                c.throttle = (0.35 - err * 0.3).clamp(0.0, 1.0);
                let want_pitch = -1.0 + (t.ias_kt - 70.0) * 0.6;
                c.pitch = ((want_pitch - t.pitch) * 0.05 - q * 0.8).clamp(-0.5, 0.5);
            } else if !t.on_ground {
                // flare: aim for a gentle sink rate
                c.throttle = 0.0;
                let want_vs = -100.0 - t.agl_ft * 12.0;
                c.pitch = (c.pitch + (want_vs - t.vs_fpm) * 0.00002 - q * 0.02).clamp(-0.3, 0.8);
            } else {
                c.throttle = 0.0;
                c.pitch = 0.0;
                c.brake = 1.0;
            }
            c.roll = (-t.bank * 0.05 - t.pos.z * 0.01).clamp(-0.5, 0.5);
            c.yaw = if t.on_ground { (-t.pos.z * 0.2 - (t.hdg - 90.0) * 0.1).clamp(-1.0, 1.0) } else { 0.0 };
            ac.step(dt, &world.terrain);
            assert!(ac.crashed.is_none(), "crashed: {:?}", ac.crashed);
            let t = ac.telem(&world.terrain);
            m.update(lesson, &t, prev, &world, ac.touchdown, &mut best, dt);
            prev = ac.pos;
            if m.complete {
                break;
            }
        }
        assert!(m.complete, "stuck at step {}", m.step);
        println!("landing summary: {:?}", m.summary);
    }
}
