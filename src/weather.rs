//! The air the aircraft flies through: wind that strengthens and veers with height, gusts,
//! turbulence drifting along with the wind, cloud cover, visibility and the time of day.

use crate::flight::{Air, KT};
use crate::world::{fbm, smoothstep, Thermal};
use macroquad::math::{vec3, Vec3};

#[derive(Clone, Copy, PartialEq)]
pub struct Weather {
    /// Direction the surface wind blows FROM, degrees true.
    pub wind_from: f32,
    pub wind_kt: f32,
    /// Extra knots the gusts add on top of the steady wind.
    pub gust_kt: f32,
    /// 0 smooth .. 1 rough.
    pub turbulence: f32,
    /// 0 clear .. 1 overcast.
    pub clouds: f32,
    pub cloud_base_ft: f32,
    pub visibility_km: f32,
    /// Local time, hours 0..24.
    pub hour: f32,
    /// 0 none .. 1 strong: how lively the thermals are (they need the afternoon sun too).
    pub thermals: f32,
}

impl Default for Weather {
    fn default() -> Self {
        Weather { wind_from: 70.0, wind_kt: 6.0, gust_kt: 0.0, turbulence: 0.15, clouds: 0.3, cloud_base_ft: 4800.0, visibility_km: 40.0, hour: 14.0, thermals: 0.5 }
    }
}

impl Weather {
    #[cfg(test)]
    pub fn calm() -> Self {
        Weather { wind_kt: 0.0, gust_kt: 0.0, turbulence: 0.0, ..Default::default() }
    }

    /// Steady wind (no gusts or turbulence) at height `agl` metres, as a world-space air velocity.
    pub fn steady_wind(&self, agl: f32) -> Vec3 {
        // power-law boundary layer: slower near the ground, ~1.6x by 1,000 ft
        let h = agl.max(1.0);
        let k = (h / 10.0).powf(0.14).clamp(0.55, 1.8);
        // and veering a little with height
        let from = (self.wind_from + 20.0 * smoothstep(0.0, 900.0, h)).to_radians();
        // heading 0 is -z (north), 90 is +x; the air moves toward the opposite of `from`
        let to = vec3(-from.sin(), 0.0, from.cos());
        to * (self.wind_kt * KT * k)
    }

    /// Gust strength 0..1 at time `t`: slow swells with sharper peaks.
    fn gust(&self, t: f32) -> f32 {
        let n = fbm(t * 0.11, 3.7, 3);
        smoothstep(0.45, 0.8, n)
    }

    /// The air at a point: wind, gusts and turbulence, plus the roll and pitch gradients across
    /// an aircraft of the given span, oriented with `right` (its right-wing direction).
    pub fn air(&self, pos: Vec3, agl: f32, t: f32, span: f32, right: Vec3) -> Air {
        let base = self.steady_wind(agl);
        let speed = base.length();
        let dir = if speed > 0.01 { base / speed } else { Vec3::ZERO };
        let gusty = base + dir * self.gust_kt * KT * self.gust(t) * (agl / 10.0).max(0.1).powf(0.14).min(1.6);
        // Frozen turbulence carried along by the wind: sample a noise field moving with the air.
        // Mechanical turbulence grows with the wind near the ground.
        let mech = 1.0 + (self.wind_kt + self.gust_kt) / 15.0 * (-agl / 300.0).exp();
        let sigma = self.turbulence * 1.8 * mech;
        if sigma <= 0.0 {
            return Air { wind: gusty, roll_gust: 0.0, pitch_gust: 0.0 };
        }
        let at = |p: Vec3| -> Vec3 {
            let q = (p - base * t) / 110.0;
            vec3(
                fbm(q.x + 13.1, q.z + q.y * 0.7, 3) - 0.5,
                (fbm(q.x - 7.7, q.z + 4.2 - q.y * 0.6, 3) - 0.5) * 0.8,
                fbm(q.x + 2.9 + q.y * 0.5, q.z - 21.3, 3) - 0.5,
            ) * (sigma * 3.0)
        };
        let half = right * (span * 0.5);
        let (l, r) = (at(pos - half), at(pos + half));
        let centre = (l + r) * 0.5;
        Air {
            wind: gusty + centre,
            // an updraft under the right wing rolls the aircraft left
            roll_gust: (r.y - l.y) / span.max(1.0),
            pitch_gust: centre.y * 0.01,
        }
    }

    /// Rising (or, around the edges, sinking) air from thermals at a point, m/s. They need
    /// the sun: nothing at night, strongest mid-afternoon, and they top out at cloud base.
    pub fn lift(&self, thermals: &[Thermal], pos: Vec3, agl: f32, t: f32) -> f32 {
        let sun = smoothstep(9.0, 12.5, self.hour) * (1.0 - smoothstep(16.5, 19.0, self.hour));
        let k = self.thermals * sun * (1.0 - 0.6 * smoothstep(0.6, 0.95, self.clouds));
        if k <= 0.0 {
            return 0.0;
        }
        let top = self.cloud_base_ft * crate::flight::FT;
        let height = smoothstep(20.0, 220.0, agl) * (1.0 - smoothstep(top * 0.8, top, pos.y));
        if height <= 0.0 {
            return 0.0;
        }
        let mut w = 0.0;
        for (i, th) in thermals.iter().enumerate() {
            let (dx, dz) = (pos.x - th.x, pos.z - th.z);
            let d2 = dx * dx + dz * dz;
            if d2 > (th.radius * 3.0).powi(2) {
                continue;
            }
            // each thermal pulses over a few minutes
            let cycle = 0.65 + 0.35 * (t / 240.0 + i as f32 * 1.7).sin();
            let x2 = d2 / (th.radius * th.radius);
            w += th.strength * cycle * ((-x2).exp() - 0.22 * (-x2 / 5.0).exp());
        }
        w * k * height
    }

    /// Fog density for the renderer, per metre at sea level.
    pub fn haze(&self) -> f32 {
        // ~3.9 / visibility gives about 2% contrast at the visibility distance
        (3.0 / (self.visibility_km * 1000.0)).max(1.05e-4 * 0.5)
    }
}

/// Lighting and sky colours for the renderer, worked out from the time of day and the weather.
#[derive(Clone, Copy)]
pub struct Sky {
    /// Direction toward the main light (the sun, or the moon at night).
    pub light: Vec3,
    pub light_col: Vec3,
    /// Direction toward the sun itself (for the disc and glow, even below the horizon).
    pub sun: Vec3,
    pub ambient: f32,
    pub amb_sky: Vec3,
    pub amb_ground: Vec3,
    pub fog: Vec3,
    pub zenith: Vec3,
    pub haze: f32,
    /// 0 day .. 1 full night: runway and town lights fade in.
    pub night: f32,
    /// 0 clear .. 1 overcast: shadows fade, the sun hides.
    pub overcast: f32,
    /// Height of the cloud deck (m) when there is one.
    pub deck: Option<f32>,
    /// How strongly the ground lights show: at night, under overcast and in poor visibility.
    pub lamps: f32,
}

fn mix3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

impl Weather {
    /// Sun direction for the time of day: rises in the east, high in the south at noon.
    pub fn sun_dir(&self) -> Vec3 {
        let day = (self.hour - 6.0) / 12.0; // 0 at sunrise, 1 at sunset
        // the same arc continues under the horizon through the night
        let el = (62.0 * (day * std::f32::consts::PI).sin()).to_radians();
        let az = (90.0 + 180.0 * day).to_radians();
        vec3(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos()).normalize()
    }

    pub fn sky(&self) -> Sky {
        let sun = self.sun_dir();
        let el = sun.y.asin().to_degrees();
        let day = smoothstep(-5.0, 12.0, el); // 0 night .. 1 full day
        let golden = smoothstep(25.0, 2.0, el); // warm low sun
        let night = 1.0 - smoothstep(-9.0, 3.0, el);
        let overcast = smoothstep(0.55, 0.95, self.clouds);

        // colour keyframes by sun elevation: night, dusk, golden hour, day
        let keys = |night: Vec3, dusk: Vec3, golden: Vec3, noon: Vec3| -> Vec3 {
            if el < -6.0 {
                mix3(night, dusk, smoothstep(-13.0, -6.0, el))
            } else if el < 1.0 {
                mix3(dusk, golden, smoothstep(-6.0, 1.0, el))
            } else {
                mix3(golden, noon, smoothstep(1.0, 18.0, el))
            }
        };
        let grey = vec3(0.62, 0.65, 0.69);
        let mut fog = keys(vec3(0.035, 0.045, 0.08), vec3(0.55, 0.38, 0.42), vec3(0.93, 0.66, 0.50), vec3(0.70, 0.79, 0.90));
        fog = mix3(fog, grey * (0.15 + 0.85 * day), overcast * 0.85);
        let mut zenith = keys(vec3(0.008, 0.012, 0.035), vec3(0.08, 0.10, 0.25), vec3(0.22, 0.32, 0.60), vec3(0.20, 0.42, 0.82));
        zenith = mix3(zenith, grey * (0.12 + 0.75 * day), overcast * 0.85);

        let sun_col = mix3(vec3(1.0, 0.95, 0.85), vec3(1.0, 0.62, 0.34), golden);
        // moonlight takes over at night: faint and blue, from high in the sky
        let moon = vec3(-0.3, 0.75, 0.55).normalize();
        let (light, light_col) = if el > -3.0 {
            (vec3(sun.x, sun.y.max(0.04), sun.z).normalize(), sun_col * day * (1.0 - 0.75 * overcast))
        } else {
            (moon, vec3(0.45, 0.55, 0.8) * 0.18 * (1.0 - 0.7 * overcast))
        };
        let ambient = 0.06 + 0.36 * day + 0.12 * overcast * day;
        Sky {
            light,
            light_col,
            sun,
            ambient,
            amb_sky: mix3(vec3(0.45, 0.55, 0.9), mix3(vec3(0.78, 0.86, 1.0), vec3(0.95, 0.8, 0.75), golden), day),
            amb_ground: mix3(vec3(0.25, 0.25, 0.35), vec3(0.62, 0.58, 0.50), day),
            fog,
            zenith,
            haze: self.haze() * (1.0 + 0.6 * overcast),
            night,
            overcast,
            deck: (self.clouds > 0.55).then(|| self.cloud_base_ft * crate::flight::FT),
            lamps: night.max(0.55 * overcast).max(smoothstep(15.0, 3.0, self.visibility_km) * 0.8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flight::{Aircraft, Craft};
    use crate::world::World;
    use macroquad::math::vec3;

    /// A sailplane circling in a good thermal on a summer afternoon goes up.
    #[test]
    fn sailplane_climbs_in_a_thermal() {
        let world = World::generate();
        let t = &world.terrain;
        let w = Weather { thermals: 1.0, turbulence: 0.0, wind_kt: 0.0, gust_kt: 0.0, hour: 14.0, cloud_base_ft: 6000.0, ..Default::default() };
        let th = world.thermals.iter().max_by(|a, b| a.strength.total_cmp(&b.strength)).unwrap();
        let ground = t.surface(th.x, th.z);
        // start on a circle of about 80 m radius around the core
        let mut ac = Aircraft::in_flight(Craft::Glider, vec3(th.x + 80.0, ground + 500.0, th.z), 0.0, 48.0, 0.0, 0);
        let y0 = ac.pos.y;
        let dt = 1.0 / 240.0;
        for i in 0..(240 * 90) {
            let agl = ac.pos.y - t.surface(ac.pos.x, ac.pos.z) - ac.spec.rest_height;
            let half = ac.right() * (ac.spec.span * 0.5);
            let tt = i as f32 * dt;
            let (l, r) = (w.lift(&world.thermals, ac.pos - half, agl, tt), w.lift(&world.thermals, ac.pos + half, agl, tt));
            let mut air = w.air(ac.pos, agl, tt, ac.spec.span, ac.right());
            air.wind.y += (l + r) * 0.5;
            air.roll_gust += (r - l) / ac.spec.span;
            ac.air = air;
            // circle left at 35 degrees of bank, 48 kt, steering to stay centred on the core
            let want_bank = -35.0;
            ac.ctl.roll = ((want_bank - ac.bank_deg()) * 0.05 + ac.omega.z * 0.3).clamp(-0.6, 0.6);
            ac.ctl.pitch = (((48.0 - ac.ias / KT) * -0.04) - ac.omega.x * 0.5 + 0.05).clamp(-0.5, 0.5);
            ac.ctl.yaw = (ac.beta * 3.0).clamp(-0.5, 0.5);
            ac.step(dt, t);
            assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        }
        let gained = ac.pos.y - y0;
        println!("thermal strength {:.1} m/s: gained {gained:.0} m in 90 s", th.strength);
        assert!(gained > 40.0, "only gained {gained} m");
    }
}
