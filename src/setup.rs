//! The free-flight setup screen: which aircraft, where to start, the time of day and the
//! weather. Also the small saved profile (finished lessons, best times and scores).

use crate::flight::{Craft, FT};
use crate::lessons::Start;
use crate::weather::Weather;
use crate::world::*;
use macroquad::math::vec3;
use std::collections::BTreeMap;

pub const AIRCRAFT: [(Craft, &str); 4] = [
    (Craft::Cessna, "Trainer (Cessna 172 type)"),
    (Craft::Jet, "Boeing 737 MAX 8"),
    (Craft::Fighter, "F-16 Fighting Falcon"),
    (Craft::Glider, "ASK 21 sailplane"),
];

#[derive(Clone, Copy, PartialEq)]
pub enum Place {
    Runway09,
    Runway27,
    /// On the 3 degree glide path to runway 09, this many metres from the touchdown point.
    Final(f32),
    Airborne,
    /// The grass strip on the south coast, ready to go west to east.
    Strip,
    /// A sightseeing start: over the harbour town at low level.
    Harbour,
}

pub const PLACES: [(Place, &str); 7] = [
    (Place::Runway09, "Runway 09, ready for takeoff"),
    (Place::Runway27, "Runway 27, ready for takeoff"),
    (Place::Final(7400.0), "4 nm final, runway 09"),
    (Place::Final(18500.0), "10 nm final, runway 09"),
    (Place::Airborne, "Airborne west of the airport"),
    (Place::Strip, "Grass strip on the south coast"),
    (Place::Harbour, "Over the harbour, 1,500 ft"),
];

const WINDS: [f32; 7] = [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 35.0];
const GUSTS: [f32; 4] = [0.0, 5.0, 10.0, 15.0];
const TURB: [(f32, &str); 4] = [(0.0, "none"), (0.15, "light"), (0.35, "moderate"), (0.6, "severe")];
const CLOUDS: [(f32, &str); 5] = [(0.0, "clear"), (0.25, "few"), (0.5, "scattered"), (0.7, "broken"), (0.95, "overcast")];
const BASES: [f32; 6] = [400.0, 800.0, 1500.0, 2500.0, 4000.0, 6000.0];
const VIS: [f32; 5] = [40.0, 15.0, 8.0, 3.0, 1.5];
const THERMALS: [(f32, &str); 3] = [(0.0, "none"), (0.5, "normal"), (1.0, "strong")];

#[derive(Clone, Copy, PartialEq)]
pub struct FreeSetup {
    pub craft: usize,
    pub place: usize,
    pub hour: f32,
    pub wind: usize,
    pub wind_from: f32,
    pub gust: usize,
    pub turb: usize,
    pub clouds: usize,
    pub base: usize,
    pub vis: usize,
    pub thermals: usize,
}

impl Default for FreeSetup {
    fn default() -> Self {
        FreeSetup { craft: 0, place: 0, hour: 14.0, wind: 1, wind_from: 70.0, gust: 0, turb: 1, clouds: 1, base: 4, vis: 0, thermals: 1 }
    }
}

/// The rows of the setup screen, in order. The last one launches.
pub const ROWS: usize = 12;

fn step(i: usize, n: usize, dir: i32) -> usize {
    ((i as i32 + dir).rem_euclid(n as i32)) as usize
}

pub fn clock(hour: f32) -> String {
    let h = hour.rem_euclid(24.0);
    let what = match h {
        h if !(5.5..20.5).contains(&h) => "night",
        h if h < 7.5 => "dawn",
        h if h < 17.0 => "day",
        h if h < 19.0 => "golden hour",
        _ => "dusk",
    };
    format!("{:02}:{:02}  {what}", h as u32, ((h.fract() * 60.0).round() as u32).min(59))
}

impl FreeSetup {
    pub fn craft(&self) -> Craft {
        AIRCRAFT[self.craft].0
    }

    /// Label and value of each row; `locked` is how many more stars the aircraft needs.
    pub fn rows(&self, locked: Option<u32>) -> Vec<(&'static str, String)> {
        let aircraft = match locked {
            Some(_) => format!("{} (locked)", AIRCRAFT[self.craft].1),
            None => AIRCRAFT[self.craft].1.to_string(),
        };
        let fly = match locked {
            Some(n) => format!("Earn {n} more star{} to fly this one", if n == 1 { "" } else { "s" }),
            None => "Fly!".to_string(),
        };
        vec![
            ("Aircraft", aircraft),
            ("Start", PLACES[self.place].1.to_string()),
            ("Time", clock(self.hour)),
            ("Wind", if WINDS[self.wind] == 0.0 { "calm".into() } else { format!("{:.0} kt", WINDS[self.wind]) }),
            ("Wind from", format!("{:03.0}", self.wind_from)),
            ("Gusts", if GUSTS[self.gust] == 0.0 { "none".into() } else { format!("+{:.0} kt", GUSTS[self.gust]) }),
            ("Turbulence", TURB[self.turb].1.to_string()),
            ("Clouds", CLOUDS[self.clouds].1.to_string()),
            ("Cloud base", format!("{:.0} ft", BASES[self.base])),
            ("Visibility", format!("{:.0} km", VIS[self.vis])),
            ("Thermals", THERMALS[self.thermals].1.to_string()),
            ("", fly),
        ]
    }

    pub fn change(&mut self, row: usize, dir: i32) {
        match row {
            0 => self.craft = step(self.craft, AIRCRAFT.len(), dir),
            1 => self.place = step(self.place, PLACES.len(), dir),
            2 => self.hour = (self.hour + dir as f32 * 0.5).rem_euclid(24.0),
            3 => self.wind = step(self.wind, WINDS.len(), dir),
            4 => self.wind_from = (self.wind_from + dir as f32 * 10.0).rem_euclid(360.0),
            5 => self.gust = step(self.gust, GUSTS.len(), dir),
            6 => self.turb = step(self.turb, TURB.len(), dir),
            7 => self.clouds = step(self.clouds, CLOUDS.len(), dir),
            8 => self.base = step(self.base, BASES.len(), dir),
            9 => self.vis = step(self.vis, VIS.len(), dir),
            10 => self.thermals = step(self.thermals, THERMALS.len(), dir),
            _ => {}
        }
    }

    pub fn weather(&self) -> Weather {
        Weather {
            wind_from: self.wind_from,
            wind_kt: WINDS[self.wind],
            gust_kt: if WINDS[self.wind] > 0.0 { GUSTS[self.gust] } else { 0.0 },
            turbulence: TURB[self.turb].0,
            clouds: CLOUDS[self.clouds].0,
            cloud_base_ft: BASES[self.base],
            visibility_km: VIS[self.vis],
            hour: self.hour,
            thermals: THERMALS[self.thermals].0,
        }
    }

    pub fn start(&self) -> Start {
        let craft = self.craft();
        match PLACES[self.place].0 {
            Place::Runway09 => Start::Runway,
            Place::Runway27 => Start::Runway27,
            Place::Final(d) => {
                let (ias, flaps) = match craft {
                    Craft::Jet => (145.0, 7),
                    Craft::Fighter => (145.0, 1),
                    Craft::Glider => (55.0, 0),
                    Craft::Cessna => (70.0, 2),
                };
                Start::Air {
                    pos: vec3(-RWY_HALF_LEN + PAPI_DIST - d, RWY_ELEV + 3.0 + d * 3f32.to_radians().tan(), 0.0),
                    heading: 90.0,
                    ias,
                    gamma: -3.0,
                    flaps,
                    trim_offset: 0.0,
                }
            }
            Place::Strip => Start::Ground { x: STRIP.0 - STRIP_HALF_LEN + 30.0, z: STRIP.1, heading: 90.0 },
            Place::Harbour => {
                let ias = match craft {
                    Craft::Jet => 220.0,
                    Craft::Fighter => 300.0,
                    Craft::Glider => 55.0,
                    Craft::Cessna => 95.0,
                };
                Start::Air { pos: vec3(PORT.0 - 2500.0, 460.0, PORT.1 - 1500.0), heading: 120.0, ias, gamma: 0.0, flaps: 0, trim_offset: 0.0 }
            }
            Place::Airborne => {
                let (ias, alt) = match craft {
                    Craft::Jet => (250.0, 5000.0),
                    Craft::Fighter => (350.0, 8000.0),
                    Craft::Glider => (55.0, 4000.0),
                    Craft::Cessna => (100.0, 3000.0),
                };
                Start::Air { pos: vec3(-6000.0, alt * FT, 800.0), heading: 90.0, ias, gamma: 0.0, flaps: 0, trim_offset: 0.0 }
            }
        }
    }

    fn save(&self, p: &mut Profile) {
        let v = [self.craft, self.place, (self.hour * 2.0) as usize, self.wind, self.wind_from as usize, self.gust, self.turb, self.clouds, self.base, self.vis, self.thermals];
        p.set("setup", &v.map(|x| x.to_string()).join(","));
    }

    fn load(p: &Profile) -> Self {
        let d = FreeSetup::default();
        let Some(s) = p.get("setup") else { return d };
        let v: Vec<usize> = s.split(',').filter_map(|x| x.parse().ok()).collect();
        if v.len() < 10 {
            return d;
        }
        let pick = |i: usize, n: usize, def: usize| if v[i] < n { v[i] } else { def };
        FreeSetup {
            craft: pick(0, AIRCRAFT.len(), d.craft),
            place: pick(1, PLACES.len(), d.place),
            hour: (v[2] as f32 / 2.0).rem_euclid(24.0),
            wind: pick(3, WINDS.len(), d.wind),
            wind_from: (v[4] % 360) as f32,
            gust: pick(5, GUSTS.len(), d.gust),
            turb: pick(6, TURB.len(), d.turb),
            clouds: pick(7, CLOUDS.len(), d.clouds),
            base: pick(8, BASES.len(), d.base),
            vis: pick(9, VIS.len(), d.vis),
            thermals: if v.len() > 10 { pick(10, THERMALS.len(), d.thermals) } else { d.thermals },
        }
    }
}

/// Saved between runs as `key value` lines in the user's config folder.
#[derive(Default)]
pub struct Profile {
    values: BTreeMap<String, String>,
}

/// The folder Skyhop saves into (the profile, ghosts); SKYHOP_DATA overrides it.
pub fn data_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("SKYHOP_DATA") {
        return Some(d.into());
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    Some(if cfg!(target_os = "macos") {
        home?.join("Library/Application Support/Skyhop")
    } else if cfg!(windows) {
        std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("Skyhop")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| home.map(|h| h.join(".config")))?.join("skyhop")
    })
}

fn profile_path() -> Option<std::path::PathBuf> {
    Some(data_dir()?.join("profile.txt"))
}

impl Profile {
    fn parse(text: &str) -> Self {
        let mut values = BTreeMap::new();
        for line in text.lines() {
            // keys can contain spaces (lesson names), values never do
            if let Some((k, v)) = line.rsplit_once(' ') {
                values.insert(k.to_string(), v.to_string());
            }
        }
        Profile { values }
    }

    pub fn load() -> Self {
        profile_path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(Profile::default, |t| Profile::parse(&t))
    }

    pub fn save(&self) {
        let Some(path) = profile_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self.values.iter().map(|(k, v)| format!("{k} {v}\n")).collect();
        let _ = std::fs::write(path, text);
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.values.get(k).map(|s| s.as_str())
    }

    pub fn set(&mut self, k: &str, v: &str) {
        self.values.insert(k.to_string(), v.replace(' ', "_"));
    }

    pub fn number(&self, k: &str) -> Option<f32> {
        self.get(k).and_then(|v| v.parse().ok())
    }

    pub fn setup(&self) -> FreeSetup {
        FreeSetup::load(self)
    }

    pub fn set_setup(&mut self, s: &FreeSetup) {
        s.save(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_keys_with_spaces_round_trip() {
        let mut p = Profile::default();
        p.set("best.Night ILS: 737", "112");
        p.set("grade.Night ILS: 737", "A+");
        let text: String = p.values.iter().map(|(k, v)| format!("{k} {v}\n")).collect();
        let q = Profile::parse(&text);
        assert_eq!(q.number("best.Night ILS: 737"), Some(112.0));
        assert_eq!(q.get("grade.Night ILS: 737"), Some("A+"));
    }
}
