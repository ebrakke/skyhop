//! Stars and unlocks: up to three stars on each objective for how well it went, and the
//! aircraft they open up.

use crate::flight::{Craft, KT};
use crate::lessons::{fmt_time, Kind, Lesson};
use crate::setup::Profile;
use crate::world::{World, RING_START};

/// Lessons that end in a landing; they're graded on the landing scorecard like the challenges.
const GRADED: [&str; 3] = ["6. Landing", "Sailplane: ride a thermal", "737 MAX 8: Landing"];

pub fn graded(l: &Lesson) -> bool {
    l.kind == Kind::Landing || l.kind == Kind::Lesson && GRADED.contains(&l.name)
}

pub fn max_stars(l: &Lesson) -> u32 {
    match l.kind {
        Kind::Free => 0,
        Kind::Lesson if !graded(l) => 1,
        _ => 3,
    }
}

/// One star for getting it down and stopped, two for a B, three for an A.
pub fn grade_stars(grade: &str) -> u32 {
    match grade {
        "A+" | "A" => 3,
        "B" => 2,
        _ => 1,
    }
}

/// Target times for the ring course in this aircraft: (three stars, two stars), seconds.
pub fn ring_pars(world: &World, craft: Craft) -> (f32, f32) {
    let mut len = 0.0;
    let mut prev = RING_START;
    for r in &world.rings {
        len += (r.pos - prev).length();
        prev = r.pos;
    }
    // average speeds over the course, knots
    let (gold, silver) = if craft == Craft::Fighter { (400.0, 320.0) } else { (95.0, 80.0) };
    (len / (gold * KT), len / (silver * KT))
}

pub fn ring_stars(time: f32, pars: (f32, f32)) -> u32 {
    if time <= pars.0 {
        3
    } else if time <= pars.1 {
        2
    } else {
        1
    }
}

/// Stars earned so far on an objective.
pub fn stars(l: &Lesson, p: &Profile, world: &World) -> u32 {
    let done = p.get(&format!("done.{}", l.name)).is_some();
    match l.kind {
        Kind::Free => 0,
        Kind::Rings => p.number(&format!("ring.{}", l.name)).map_or(0, |t| ring_stars(t, ring_pars(world, l.craft))),
        _ if graded(l) => match p.get(&format!("grade.{}", l.name)) {
            Some(g) => grade_stars(g),
            None => done as u32,
        },
        _ => done as u32,
    }
}

/// (earned, possible) over every objective.
pub fn total(lessons: &[Lesson], p: &Profile, world: &World) -> (u32, u32) {
    lessons.iter().fold((0, 0), |(n, m), l| (n + stars(l, p, world), m + max_stars(l)))
}

/// Stars needed before an aircraft can be flown.
pub fn unlock_at(craft: Craft) -> u32 {
    match craft {
        Craft::Cessna => 0,
        Craft::Glider => 4,
        Craft::Jet => 8,
        Craft::Fighter => 18,
    }
}

pub fn unlocked(craft: Craft, earned: u32, p: &Profile) -> bool {
    earned >= unlock_at(craft) || p.get("unlock_all").is_some()
}

/// How to earn the stars, for the menu.
pub fn goal(l: &Lesson, world: &World) -> String {
    match l.kind {
        Kind::Free => String::new(),
        Kind::Rings => {
            let (gold, silver) = ring_pars(world, l.craft);
            format!("Stars: finish, under {}, under {}", fmt_time(silver), fmt_time(gold))
        }
        _ if graded(l) => "Stars: land and stop, grade B, grade A".into(),
        _ => "Star: complete the lesson".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lessons::all;

    #[test]
    fn graded_lessons_exist_and_unlocks_are_reachable() {
        let lessons = all();
        for name in GRADED {
            assert!(lessons.iter().any(|l| l.name == name), "no lesson called {name}");
        }
        // each aircraft can be unlocked by flying the ones before it
        let world = World::generate();
        let p = Profile::default();
        assert_eq!(total(&lessons, &p, &world).0, 0);
        let mut order = [Craft::Cessna, Craft::Glider, Craft::Jet, Craft::Fighter];
        order.sort_by_key(|c| unlock_at(*c));
        let mut available = 0;
        for c in order {
            assert!(available >= unlock_at(c), "{c:?} needs {} stars but only {available} can be earned before it", unlock_at(c));
            available += lessons.iter().filter(|l| l.craft == c).map(max_stars).sum::<u32>();
        }
        let (gold, silver) = ring_pars(&world, Craft::Cessna);
        println!("ring pars: trainer {} / {}", fmt_time(gold), fmt_time(silver));
        assert!(gold < silver && gold > 60.0);
    }
}

#[cfg(test)]
mod pace {
    use crate::ap::Autopilot;
    use crate::flight::{Aircraft, Craft, FT, KT};
    use crate::world::World;
    use macroquad::math::vec3;

    /// The three-star ring times must be beatable: the average speed they need should be
    /// below what the aircraft does flat out in level flight.
    #[test]
    fn ring_targets_are_reachable() {
        let world = World::generate();
        for (craft, need) in [(Craft::Cessna, 95.0), (Craft::Fighter, 400.0)] {
            let mut ac = Aircraft::in_flight(craft, vec3(0.0, 600.0, 0.0), 90.0, need, 0.0, 0);
            let mut ap = Autopilot::default();
            ap.sync(&ac);
            ap.engage(&ac);
            ap.select_alt(&ac);
            let dt = 1.0 / 240.0;
            for i in 0..(240 * 120) {
                ac.ctl.throttle = if craft == Craft::Fighter { 0.9 } else { 1.0 };
                if i % 4 == 0 {
                    let agl = ac.pos.y / FT;
                    ap.update(&mut ac, agl, dt * 4.0);
                }
                ac.step(dt, &world.terrain);
            }
            let top = ac.ias / KT;
            println!("{craft:?}: {top:.0} kt flat out, three stars needs {need:.0} kt average");
            assert!(top > need * 1.1, "{craft:?} tops out at {top:.0} kt");
        }
    }
}
