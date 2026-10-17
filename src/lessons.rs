//! Guided lessons, the ring challenge and free flight.

use crate::flight::{Craft, Speedbrake, Touchdown, Telem, FT};
use crate::weather::Weather;
use crate::world::*;
use macroquad::math::{vec3, Vec3};

#[derive(Clone, Copy)]
pub enum Start {
    /// Lined up on runway 09.
    Runway,
    Runway27,
    /// Parked on the ground at (x, z) facing a heading.
    Ground { x: f32, z: f32, heading: f32 },
    Air { pos: Vec3, heading: f32, ias: f32, gamma: f32, flaps: u8, trim_offset: f32 },
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Lesson,
    Rings,
    /// Land and stop on the runway; graded on a scorecard.
    Landing,
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
    pub craft: Craft,
    /// Approach speed to mark on the airspeed indicator.
    pub vref: Option<f32>,
    /// Conditions for this lesson; None means a pleasant default day.
    pub weather: Option<Weather>,
    /// Starts with the engine failed.
    pub engine_out: bool,
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

/// On the 3 degree glide path to runway 09, `d` metres before the touchdown point.
pub fn on_final(d: f32) -> Vec3 {
    vec3(-RWY_HALF_LEN + PAPI_DIST - d, RWY_ELEV + 3.0 + d * 3f32.to_radians().tan(), 0.0)
}

/// Angle (deg) of the aircraft above the runway-09 PAPI, as seen from the lights.
pub fn glide_angle(pos: Vec3) -> f32 {
    let x = -RWY_HALF_LEN + PAPI_DIST;
    let dx = (x - pos.x).max(1.0);
    (pos.y - 1.4 - RWY_ELEV).atan2(dx).to_degrees()
}

/// ILS needles for runway 09 in dots (+-2 full scale), or None when not on the approach:
/// (localizer, glideslope). Localizer is positive when the centerline is to the right of the
/// aircraft; glideslope positive when the glide path is above it (you're low).
pub fn ils(pos: Vec3, hdg: f32) -> Option<(f32, f32)> {
    let loc_x = RWY_HALF_LEN + 300.0; // the localizer antenna sits beyond the far end
    let on_final = pos.x < -RWY_HALF_LEN + PAPI_DIST && pos.x > -RWY_HALF_LEN - 20_000.0 && hdg_err(hdg, 90.0) < 90.0;
    let off = pos.z.atan2(loc_x - pos.x).to_degrees();
    if !on_final || off.abs() > 12.0 {
        return None;
    }
    let loc = (-off / 1.25).clamp(-2.5, 2.5);
    let gs = ((3.0 - glide_angle(pos)) / 0.35).clamp(-2.5, 2.5);
    Some((loc, gs))
}

fn no_summary(_: &Scratch, _: Option<Touchdown>) -> Vec<String> {
    Vec::new()
}

const CRUISE_ALT: f32 = 4000.0;
/// 737 approach speed (flaps 30 at landing weight) and the length of the final approach, m.
const JET_VREF: f32 = 140.0;
const JET_FINAL: f32 = 7000.0;

pub fn all() -> Vec<Lesson> {
    vec![
        Lesson {
            name: "1. First Flight: Takeoff",
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
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
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
            blurb: "Use trim to fly hands-off and hold altitude and heading.",
            start: Start::Air { pos: vec3(-500.0, CRUISE_ALT * FT, 3000.0), heading: 0.0, ias: 100.0, gamma: 0.0, flaps: 0, trim_offset: -0.04 },
            kind: Kind::Lesson,
            steps: vec![
                step("Feel the trim", "Let go of the stick and watch: the nose slowly drops because the plane is trimmed nose-down. Hold T (nose-up trim) - the green TRIM readout shows the speed the plane will settle at. Bring it back to about 100 kt and let the vertical speed (VSI) settle near zero.", 5.0,
                    |t| t.stick_pitch.abs() < 0.05 && t.vs_fpm.abs() < 150.0,
                    |t| if t.stick_pitch.abs() > 0.05 { Some("Let go of W / S - let the trim do the work.") }
                        else if t.vs_fpm < -200.0 { Some("Descending: T for nose-up trim (tap for fine steps), then give it a few seconds.") }
                        else if t.vs_fpm > 200.0 { Some("Climbing: G for nose-down trim (tap for fine steps), then give it a few seconds.") } else { None }),
                step("Hold it steady", "Keep within 100 ft of 4,000 ft and within 10 degrees of heading 360 for 20 seconds. Pick a spot on the horizon and keep the nose on it. Small corrections!", 20.0,
                    |t| (t.alt_ft - CRUISE_ALT).abs() < 100.0 && hdg_err(t.hdg, 0.0) < 10.0,
                    |t| if t.alt_ft < CRUISE_ALT - 100.0 { Some("Too low - raise the nose a touch and climb back to 4,000 ft.") }
                        else if t.alt_ft > CRUISE_ALT + 100.0 { Some("Too high - lower the nose a touch.") }
                        else if hdg_err(t.hdg, 0.0) >= 10.0 { Some("Turn back to heading 360 (N) with a gentle bank.") } else { None }),
                step("Slow down, stay level", "Reduce power to about 55%. The plane slows and starts to sink - raise the nose a bit and re-trim (T) to about 80 kt to hold 4,000 ft at the slower speed.", 6.0,
                    |t| t.throttle < 0.6 && t.vs_fpm.abs() < 150.0 && (t.alt_ft - CRUISE_ALT).abs() < 150.0 && t.ias_kt < 92.0,
                    |t| if t.throttle >= 0.6 { Some("Pull the power back to about 55% (CTRL or -).") }
                        else if (t.alt_ft - CRUISE_ALT).abs() >= 150.0 { Some("Get back to 4,000 ft.") }
                        else if t.vs_fpm.abs() >= 150.0 { Some("Re-trim (T / G) until the VSI stays near zero.") } else { None }),
            ],
            summary: |_, _| vec!["Set the attitude, then trim away the pressure.".into(), "Less power at the same altitude = slower speed, more nose-up trim.".into()],
        },
        Lesson {
            name: "3. Turns",
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
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
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
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
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
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
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
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
            name: "Sailplane: ride a thermal",
            craft: Craft::Glider,
            vref: None,
            weather: Some(Weather { wind_from: 70.0, wind_kt: 5.0, turbulence: 0.2, clouds: 0.45, cloud_base_ft: 5500.0, hour: 14.0, thermals: 1.0, ..Default::default() }),
            engine_out: false,
            blurb: "No engine - just rising air. Find a thermal, circle in it to climb, then glide home and land.",
            start: Start::Air { pos: vec3(-2500.0, 700.0, 2200.0), heading: 30.0, ias: 50.0, gamma: -2.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Lesson,
            steps: vec![
                step("Find rising air", "Thermals rise under cumulus clouds and over sunny fields - the swirls on the map show where. Fly straight toward one at 50 kt. The variometer beeps when you're going up.", 4.0,
                    |t| t.vario > 1.0,
                    |t| if t.ias_kt > 65.0 { Some("Slow down to about 50 kt - fast flying wastes height.") } else if t.vario < -1.5 { Some("Sinking air - keep going, lift is often next to sink.") } else { None }),
                step("Circle and climb", "Bank 30-40 degrees and keep turning, centred where the beeps are fastest. If the lift fades, shift your circle toward the stronger side. Climb above 3,300 ft.", 0.0,
                    |t| t.alt_ft > 3300.0,
                    |t| if t.bank.abs() < 20.0 && t.vario > 0.5 { Some("Lift! Roll into a steady turn (30-40 degrees) to stay in it.") }
                        else if t.ias_kt < 42.0 { Some("Too slow in the turn - lower the nose a little.") }
                        else if t.vario < -0.5 { Some("Lost it - straighten out, then turn back toward where it was strongest.") } else { None }),
                step("Glide home and land", "Head for runway 09, fly 50 kt, and use the airbrakes (X) to come down onto the runway. Touch down on the main wheel and roll to a stop.", 0.0,
                    |t| t.on_ground && t.gs_kt < 3.0,
                    |t| if t.ias_kt > 70.0 { Some("Slow down: 50 kt is the best glide.") } else { None }),
            ],
            summary: |_, td| match td {
                None => vec!["Landed.".into()],
                Some(td) => vec![format!("Touchdown {:.0} fpm. Soaring pilots fly for hours on this - well done.", td.sink_fpm)],
            },
        },
        Lesson {
            name: "737 MAX 8: Landing",
            craft: Craft::Jet,
            vref: Some(JET_VREF),
            weather: None,
            engine_out: false,
            blurb: "Hand-fly a 62-tonne airliner down the glide path, flare, and stop on the runway.",
            start: Start::Air {
                pos: vec3(-RWY_HALF_LEN + PAPI_DIST - JET_FINAL, RWY_ELEV + 2.9 + JET_FINAL * 3f32.to_radians().tan(), 0.0),
                heading: 90.0, ias: JET_VREF + 5.0, gamma: -3.0, flaps: 7, trim_offset: 0.0,
            },
            kind: Kind::Lesson,
            steps: vec![
                Step {
                    track: |t, s, _| s.f[0] = s.f[0].max((t.ias_kt - (JET_VREF + 5.0)).abs()),
                    ..step("Stabilized approach", "You're on a 4-mile final for runway 09: gear down, flaps 30, speedbrake armed. Hold 145 kt (the pink bug + 5) with pitch, and stay on the glide path with thrust: two white, two red on the PAPI. The engines take seconds to spool - small, early thrust changes. The diamonds on the attitude indicator show the glide path and centerline.", 0.0,
                        |t| t.pos.x > -RWY_HALF_LEN - 150.0,
                        |t| {
                            let a = glide_angle(t.pos);
                            if t.gear < 0.99 { Some("Gear down! (L)") }
                            else if t.speedbrake != Speedbrake::Armed && !t.on_ground { Some("Arm the speedbrake (X) so it deploys on touchdown.") }
                            else if t.ias_kt < JET_VREF - 3.0 { Some("Slow! Add thrust and lower the nose a touch.") }
                            else if t.ias_kt > JET_VREF + 15.0 { Some("Fast - ease the thrust back.") }
                            else if a > 3.5 { Some("High (all white): reduce thrust a little and let it settle.") }
                            else if a < 2.5 { Some("Low (all red): add thrust, hold the attitude.") }
                            else if t.pos.z.abs() > 30.0 { Some("Line up with the centerline - small bank, then roll level.") } else { None }
                        })
                },
                step("Flare and touch down", "Aim for the white blocks 400 m in. Around 30 ft bring the thrust to idle and raise the nose 2-3 degrees to flare. Don't float - airliners land firmly in the touchdown zone. Over 10 degrees of pitch the tail hits the runway.", 0.5,
                    |t| t.on_ground,
                    |t| if t.agl_ft < 30.0 && t.throttle > 0.1 { Some("Thrust to idle (CTRL).") }
                        else if t.agl_ft < 40.0 && t.vs_fpm < -800.0 { Some("Flare! Raise the nose a little (S).") }
                        else if t.pitch > 8.0 && !t.on_ground { Some("Careful - tail strike above 10 degrees!") } else { None }),
                Step {
                    track: |t, s, _| s.f[1] = t.pos.x,
                    ..step("Stop on the runway", "Speedbrakes pop up by themselves. Keep holding CTRL at idle for reverse thrust, brake with B (or the triggers), and keep straight with Q / E. Below 60 kt push the thrust lever up (SHIFT) to stow the reversers.", 0.0,
                        |t| t.on_ground && t.gs_kt < 3.0,
                        |t| if t.pos.z.abs() > 8.0 { Some("Steer back to the centerline (Q / E).") }
                            else if t.reverser < 0.5 && t.ias_kt > 80.0 { Some("Reverse thrust: keep holding CTRL at idle.") }
                            else if t.reverser > 0.5 && t.ias_kt < 60.0 { Some("Stow the reversers below 60 kt (SHIFT).") }
                            else if t.brake < 0.5 && t.ias_kt < 100.0 { Some("Brakes: hold B or squeeze the triggers.") } else { None })
                },
            ],
            summary: |s, td| match td {
                None => vec!["Landed.".into()],
                Some(td) => {
                    let grade = match td.sink_fpm {
                        f if f < 240.0 => "Smooth - a nice firm-but-gentle arrival.",
                        f if f < 400.0 => "A normal airline landing.",
                        f if f < 600.0 => "Firm. Passengers noticed - flare a touch earlier.",
                        _ => "Hard landing: that one gets written up for an inspection.",
                    };
                    let past = td.pos.x + RWY_HALF_LEN;
                    let zone = if past < 0.0 {
                        format!("Touched down {:.0} m SHORT of the runway!", -past)
                    } else if past < 250.0 || past > 900.0 {
                        format!("Touched down {past:.0} m in - outside the touchdown zone (250-900 m).")
                    } else {
                        format!("Touched down {past:.0} m in, right in the touchdown zone.")
                    };
                    vec![
                        format!("{:.0} fpm at {:.0} kt, {:.1} deg nose up.", td.sink_fpm, td.ias_kt, td.pitch),
                        grade.into(),
                        zone,
                        format!("Stopped {:.0} m after touchdown, {:.0} m of runway left. Speed held within {:.0} kt on final.", s.f[1] - td.pos.x, RWY_HALF_LEN - s.f[1], s.f[0]),
                    ]
                }
            },
        },
        Lesson {
            name: "Crosswind: 737 in a 20 kt crosswind",
            craft: Craft::Jet,
            vref: Some(JET_VREF + 5.0),
            weather: Some(Weather { wind_from: 30.0, wind_kt: 20.0, gust_kt: 6.0, turbulence: 0.3, clouds: 0.4, ..Default::default() }),
            engine_out: false,
            blurb: "Wind from 030 at 20 gusting 26. Crab into the wind on final, then kick the nose straight with rudder just before touchdown.",
            start: Start::Air { pos: on_final(7400.0), heading: 90.0, ias: JET_VREF + 10.0, gamma: -3.0, flaps: 7, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Gusty trainer arrival",
            craft: Craft::Cessna,
            vref: Some(70.0),
            weather: Some(Weather { wind_from: 130.0, wind_kt: 14.0, gust_kt: 9.0, turbulence: 0.45, clouds: 0.5, cloud_base_ft: 2500.0, ..Default::default() }),
            engine_out: false,
            blurb: "Wind 130 at 14 gusting 23 and a bumpy final. Add half the gust to your approach speed and keep flying the plane all the way to the stop.",
            start: Start::Air { pos: on_final(3700.0), heading: 90.0, ias: 75.0, gamma: -3.0, flaps: 2, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Night ILS: 737",
            craft: Craft::Jet,
            vref: Some(JET_VREF),
            weather: Some(Weather { wind_from: 80.0, wind_kt: 8.0, gust_kt: 0.0, turbulence: 0.1, clouds: 0.2, hour: 22.5, visibility_km: 25.0, ..Default::default() }),
            engine_out: false,
            blurb: "10 miles out at night. Follow the approach lights to the runway - or arm APP (4) with the autopilot (Z) and let it land.",
            start: Start::Air { pos: on_final(18500.0), heading: 90.0, ias: JET_VREF + 5.0, gamma: -3.0, flaps: 7, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Low cloud: ILS to minimums",
            craft: Craft::Jet,
            vref: Some(JET_VREF),
            weather: Some(Weather { wind_from: 100.0, wind_kt: 10.0, gust_kt: 0.0, turbulence: 0.2, clouds: 0.95, cloud_base_ft: 350.0, visibility_km: 3.0, hour: 9.0, thermals: 0.0 }),
            engine_out: false,
            blurb: "Overcast at 350 ft. Fly the ILS diamonds through the murk; the runway appears just above minimums (\"minimums\" at 200 ft).",
            start: Start::Air { pos: on_final(11000.0), heading: 90.0, ias: JET_VREF + 5.0, gamma: -3.0, flaps: 7, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Engine failure: glide it in",
            craft: Craft::Cessna,
            vref: Some(65.0),
            weather: Some(Weather { wind_from: 90.0, wind_kt: 6.0, turbulence: 0.15, ..Default::default() }),
            engine_out: true,
            blurb: "The engine has quit at 1,800 ft, 5 km from runway 09. Pitch for 65 kt (best glide) and plan your descent - there's no second try.",
            start: Start::Air { pos: vec3(-RWY_HALF_LEN - 5000.0, RWY_ELEV + 550.0, 900.0), heading: 80.0, ias: 70.0, gamma: -4.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "F-16: Overhead break and land",
            craft: Craft::Fighter,
            vref: Some(145.0),
            weather: Some(Weather { wind_from: 80.0, wind_kt: 8.0, turbulence: 0.15, clouds: 0.2, ..Default::default() }),
            engine_out: false,
            blurb: "Run in over runway 09 at 1,500 ft and 350 kt. Break left hard, gear and flaps down on the downwind, a curving final, land at 11 degrees nose-up.",
            start: Start::Air { pos: vec3(-RWY_HALF_LEN - 5000.0, RWY_ELEV + 460.0, 0.0), heading: 90.0, ias: 350.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Sailplane: final glide",
            craft: Craft::Glider,
            vref: Some(55.0),
            weather: Some(Weather { wind_from: 100.0, wind_kt: 8.0, turbulence: 0.2, thermals: 0.0, ..Default::default() }),
            engine_out: false,
            blurb: "No engine, one chance. 5 km out at 1,300 ft: fly 50 kt for the best glide, then use the airbrakes (X) to steepen the approach once you've made the runway.",
            start: Start::Air { pos: vec3(-RWY_HALF_LEN - 4600.0, RWY_ELEV + 400.0, -300.0), heading: 85.0, ias: 50.0, gamma: -2.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Landing,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "F-16: Ring Challenge",
            craft: Craft::Fighter,
            vref: None,
            weather: None,
            engine_out: false,
            blurb: "The ring course at 400 knots. Pull hard - but not past 9 g.",
            start: Start::Air { pos: RING_START, heading: 90.0, ias: 330.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Rings,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Ring Challenge",
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
            blurb: "Race through the rings around the island. Beat your best time!",
            start: Start::Air { pos: RING_START, heading: 90.0, ias: 105.0, gamma: 0.0, flaps: 0, trim_offset: 0.0 },
            kind: Kind::Rings,
            steps: vec![],
            summary: no_summary,
        },
        Lesson {
            name: "Free Flight",
            craft: Craft::Cessna,
            vref: None,
            weather: None,
            engine_out: false,
            blurb: "Any aircraft, anywhere, any weather - set it up and go.",
            start: Start::Runway,
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
            Kind::Landing => {
                // done once down and stopped (the scorecard is filled in by the caller)
                if touchdown.is_some() && t.on_ground && t.gs_kt < 2.0 {
                    self.complete = true;
                    return Some(Event::LessonDone);
                }
                None
            }
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
        let mut ac = Aircraft::on_ground_at(Craft::Cessna, &world.terrain, -RWY_HALF_LEN + 25.0, 0.0, 90.0);
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
        for i in 0..250 {
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
        let mut ac = Aircraft::in_flight(Craft::Cessna, pos, heading, ias, gamma, flaps);
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

    /// Flies the 737 landing lesson in the given weather; returns the summary and callouts.
    fn fly_737_landing(weather: crate::weather::Weather) -> (Vec<String>, Vec<crate::audio::Callout>) {
        let world = World::generate();
        let lessons = all();
        let idx = lessons.iter().position(|l| l.craft == Craft::Jet && l.kind == Kind::Lesson).unwrap();
        let lesson = &lessons[idx];
        let Start::Air { pos, heading, ias, gamma, flaps, .. } = lesson.start else { panic!() };
        let mut ac = Aircraft::in_flight(Craft::Jet, pos, heading, ias, gamma, flaps);
        let mut m = Mission::default();
        let mut gpws = crate::gpws::Gpws::default();
        let mut said = Vec::new();
        let mut best = None;
        let dt = 1.0 / 240.0;
        let mut prev = ac.pos;
        let mut time = 0.0;
        for _ in 0..(240 * 300) {
            let t = ac.telem(&world.terrain);
            if let Some(c) = gpws.update(&t, ils(t.pos, t.hdg).map(|d| d.1), dt) {
                said.push(c);
            }
            let q = ac.omega.x;
            let c = &mut ac.ctl;
            if t.agl_ft > 40.0 && !t.on_ground {
                // thrust for the glide path, pitch for speed
                let err = glide_angle(t.pos) - 3.0;
                c.throttle = (0.4 - err * 0.25 + (JET_VREF + 5.0 - t.ias_kt) * 0.01).clamp(0.0, 1.0);
                let want_pitch = 2.3 + (t.ias_kt - (JET_VREF + 5.0)) * 0.15;
                c.pitch = ((want_pitch - t.pitch) * 0.08 - q * 2.0).clamp(-0.5, 0.5);
            } else if !t.on_ground {
                // flare: trade the descent rate down as the runway comes up
                c.throttle = 0.0;
                let want_vs = -120.0 - t.agl_ft * 14.0;
                c.pitch = ((want_vs - t.vs_fpm) * 0.0012 + (2.5 - t.pitch) * 0.03 - q * 2.0).clamp(-0.3, 0.8);
            } else {
                c.throttle = 0.0;
                c.pitch = 0.0;
                c.reverse = if t.ias_kt > 60.0 { 1.0 } else { 0.0 };
                c.brake = 1.0;
            }
            // track the centerline (crabbing into the wind), then kick out the crab in the flare
            let want_bank = if t.on_ground || t.agl_ft < 15.0 { 0.0 } else { (-t.pos.z * 0.06 - (ac.vel.z / ac.vel.x.max(1.0)).atan().to_degrees() * 1.5).clamp(-15.0, 15.0) };
            c.roll = ((want_bank - t.bank) * 0.05).clamp(-0.5, 0.5);
            c.yaw = if t.on_ground || t.agl_ft < 25.0 { (-t.pos.z * 0.04 - (t.hdg - 90.0) * 0.12 - t.turn_rate * 0.25).clamp(-1.0, 1.0) } else { 0.0 };
            let agl = ac.pos.y - world.terrain.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height;
            ac.air = weather.air(ac.pos, agl, time, ac.spec.span, ac.right());
            time += dt;
            ac.step(dt, &world.terrain);
            if std::env::var("TRACE").is_ok() && (ac.on_ground || ac.telem(&world.terrain).agl_ft < 40.0) && (time * 10.0) as i32 % 2 == 0 && ((time - dt) * 10.0) as i32 % 2 == 1 {
                println!("t {time:.1} agl {:.0} bank {:.1} beta {:.1} roll {:.2} yaw {:.2} hdg {:.1} z {:.1} gnd {}", ac.telem(&world.terrain).agl_ft, ac.bank_deg(), ac.beta.to_degrees(), ac.ctl.roll, ac.ctl.yaw, ac.heading_deg(), ac.pos.z, ac.on_ground);
            }
            assert!(ac.crashed.is_none(), "crashed: {:?} at {:?} agl {:.0} vs {:.0} ias {:.0} pitch {:.1} bank {:.1} beta {:.1} step {}", ac.crashed, ac.pos, ac.telem(&world.terrain).agl_ft, ac.vel.y / crate::flight::FT * 60.0, ac.ias / KT, ac.pitch_deg(), ac.bank_deg(), ac.beta.to_degrees(), m.step);
            let t = ac.telem(&world.terrain);
            m.update(lesson, &t, prev, &world, ac.touchdown, &mut best, dt);
            prev = ac.pos;
            if m.complete {
                break;
            }
        }
        assert!(m.complete, "stuck at step {}", m.step);
        assert!(!ac.tail_strike, "tail strike");
        assert!(ac.pos.x < RWY_HALF_LEN, "ran off the end");
        (m.summary, said)
    }

    #[test]
    fn scripted_pilot_lands_the_737() {
        let (summary, said) = fly_737_landing(crate::weather::Weather::calm());
        println!("737 landing: {summary:?}\ncallouts: {said:?}");
        use crate::audio::Callout;
        assert!(said.contains(&Callout::Ft50) && said.contains(&Callout::Ft10), "missing height callouts: {said:?}");
        assert!(!said.iter().any(|c| matches!(c, Callout::SinkRate | Callout::PullUp | Callout::TooLowGear)), "warnings on a good approach: {said:?}");
    }

    #[test]
    fn scripted_pilot_lands_the_737_in_a_gusty_crosswind() {
        let w = crate::weather::Weather { wind_from: 30.0, wind_kt: 18.0, gust_kt: 8.0, turbulence: 0.4, ..Default::default() };
        let (summary, _) = fly_737_landing(w);
        println!("crosswind landing: {summary:?}");
        assert!(summary.len() >= 3);
    }
}