//! 2D overlay: the six-pack, engine/flap/trim cluster, lesson coach and screens.

use crate::flight::{Speedbrake, Telem};
use crate::lessons::{fmt_time, glide_angle, hdg_err, papi_lights, Kind, Lesson, Mission};
use crate::world::{World, HALF, RWY_HALF_LEN};
use macroquad::prelude::*;

pub const INK: Color = Color::new(0.95, 0.96, 0.95, 1.0);
pub const DIM: Color = Color::new(0.64, 0.69, 0.74, 1.0);
pub const AMBER: Color = Color::new(1.0, 0.77, 0.26, 1.0);
pub const RED_C: Color = Color::new(0.98, 0.33, 0.27, 1.0);
pub const GREEN_C: Color = Color::new(0.38, 0.88, 0.47, 1.0);
pub const PANEL: Color = Color::new(0.04, 0.06, 0.09, 0.72);
const EDGE: Color = Color::new(1.0, 1.0, 1.0, 0.09);
const BEZEL: Color = Color::new(0.07, 0.07, 0.08, 0.97);
const FACE: Color = Color::new(0.11, 0.115, 0.125, 1.0);
const SKY_C: Color = Color::new(0.24, 0.52, 0.86, 1.0);
const EARTH_C: Color = Color::new(0.55, 0.36, 0.2, 1.0);
const ORANGE: Color = Color::new(1.0, 0.6, 0.1, 1.0);

// ------------------------------------------------------------ helpers ---

struct Fonts {
    regular: Font,
    bold: Font,
}

thread_local! {
    static FONTS: std::cell::RefCell<Option<Fonts>> = const { std::cell::RefCell::new(None) };
}

/// Load the embedded UI typeface (Barlow, SIL Open Font License; see assets/fonts/OFL.txt).
pub fn init_fonts() {
    let load = |b: &[u8]| load_ttf_font_from_bytes(b).ok();
    let regular = load(include_bytes!("../assets/fonts/Barlow-Medium.ttf"));
    let bold = load(include_bytes!("../assets/fonts/Barlow-Bold.ttf"));
    if let (Some(regular), Some(bold)) = (regular, bold) {
        FONTS.with(|f| *f.borrow_mut() = Some(Fonts { regular, bold }));
    }
}

fn font(bold: bool) -> Option<Font> {
    FONTS.with(|f| f.borrow().as_ref().map(|f| if bold { f.bold.clone() } else { f.regular.clone() }))
}

/// UI scale: grows a little on tall windows and shrinks on small ones. (Coordinates are
/// already in logical points; macroquad handles the display's pixel density.)
pub fn ui() -> f32 {
    (screen_height() / 800.0).clamp(0.85, 1.35)
}

fn polar(cx: f32, cy: f32, r: f32, deg: f32) -> Vec2 {
    let a = deg.to_radians();
    vec2(cx + r * a.sin(), cy - r * a.cos())
}

fn draw_str(s: &str, x: f32, y: f32, size: f32, c: Color, bold: bool) {
    let f = font(bold);
    let size = size * 1.12; // Barlow is compact: match the old font's apparent size
    draw_text_ex(s, x.round(), y.round(), TextParams { font: f.as_ref(), font_size: size.round() as u16, color: c, ..Default::default() });
}

fn measure(s: &str, size: f32, bold: bool) -> TextDimensions {
    let f = font(bold);
    measure_text(s, f.as_ref(), (size * 1.12).round() as u16, 1.0)
}

pub fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_str(s, x, y, size, c, false);
}

pub fn text_b(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_str(s, x, y, size, c, true);
}

pub fn text_w(s: &str, size: f32) -> f32 {
    measure(s, size, false).width
}

fn text_cx(s: &str, cx: f32, cy: f32, size: f32, c: Color, bold: bool) {
    let d = measure(s, size, bold);
    // centre on the cap height so digits and capitals sit in the middle
    draw_str(s, cx - d.width / 2.0, cy + size * 0.36, size, c, bold);
}

/// Text centred on (cx, cy).
pub fn text_c(s: &str, cx: f32, cy: f32, size: f32, c: Color) {
    text_cx(s, cx, cy, size, c, false);
}

pub fn text_cb(s: &str, cx: f32, cy: f32, size: f32, c: Color) {
    text_cx(s, cx, cy, size, c, true);
}

/// Filled rounded rectangle, built from non-overlapping pieces so translucent fills stay even.
pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    draw_rectangle(x + r, y, w - 2.0 * r, h, c);
    draw_rectangle(x, y + r, r, h - 2.0 * r, c);
    draw_rectangle(x + w - r, y + r, r, h - 2.0 * r, c);
    let n = 8;
    for (cx, cy, a0) in [(x + r, y + r, 180.0f32), (x + w - r, y + r, 270.0), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0)] {
        for k in 0..n {
            let (b0, b1) = ((a0 + 90.0 * k as f32 / n as f32).to_radians(), (a0 + 90.0 * (k + 1) as f32 / n as f32).to_radians());
            draw_triangle(vec2(cx, cy), vec2(cx + r * b0.cos(), cy + r * b0.sin()), vec2(cx + r * b1.cos(), cy + r * b1.sin()), c);
        }
    }
}

/// Outline of a rounded rectangle.
pub fn rrect_lines(x: f32, y: f32, w: f32, h: f32, r: f32, t: f32, c: Color) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    draw_line(x + r, y, x + w - r, y, t, c);
    draw_line(x + r, y + h, x + w - r, y + h, t, c);
    draw_line(x, y + r, x, y + h - r, t, c);
    draw_line(x + w, y + r, x + w, y + h - r, t, c);
    let n = 8;
    for (cx, cy, a0) in [(x + r, y + r, 180.0f32), (x + w - r, y + r, 270.0), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, 90.0)] {
        for k in 0..n {
            let (b0, b1) = ((a0 + 90.0 * k as f32 / n as f32).to_radians(), (a0 + 90.0 * (k + 1) as f32 / n as f32).to_radians());
            draw_line(cx + r * b0.cos(), cy + r * b0.sin(), cx + r * b1.cos(), cy + r * b1.sin(), t, c);
        }
    }
}

/// Rectangle fading from `left` to `right`.
fn hgradient(x: f32, y: f32, w: f32, h: f32, left: Color, right: Color) {
    let v = |px: f32, py: f32, c: Color| macroquad::models::Vertex::new(px, py, 0.0, 0.0, 0.0, c);
    let mesh = Mesh { vertices: vec![v(x, y, left), v(x + w, y, right), v(x + w, y + h, right), v(x, y + h, left)], indices: vec![0, 1, 2, 0, 2, 3], texture: None };
    draw_mesh(&mesh);
}

/// The standard translucent card everything sits on.
pub fn card(x: f32, y: f32, w: f32, h: f32) {
    let r = 10.0 * ui();
    rrect(x, y, w, h, r, PANEL);
    rrect_lines(x, y, w, h, r, 1.0, EDGE);
}

pub fn wrap(s: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if text_w(&candidate, size) > max_w && !cur.is_empty() {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        } else {
            cur = candidate;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn band(cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32, c: Color) {
    let steps = ((a1 - a0).abs() / 4.0).ceil().max(1.0) as usize;
    for k in 0..steps {
        let (b0, b1) = (a0 + (a1 - a0) * k as f32 / steps as f32, a0 + (a1 - a0) * (k + 1) as f32 / steps as f32);
        let (p0, p1, q0, q1) = (polar(cx, cy, r0, b0), polar(cx, cy, r0, b1), polar(cx, cy, r1, b0), polar(cx, cy, r1, b1));
        draw_triangle(p0, p1, q1, c);
        draw_triangle(p0, q1, q0, c);
    }
}

fn tick(cx: f32, cy: f32, r0: f32, r1: f32, deg: f32, w: f32, c: Color) {
    let (a, b) = (polar(cx, cy, r0, deg), polar(cx, cy, r1, deg));
    draw_line(a.x, a.y, b.x, b.y, w, c);
}

fn needle(cx: f32, cy: f32, len: f32, deg: f32, w: f32, c: Color) {
    let tip = polar(cx, cy, len, deg);
    let l = polar(cx, cy, w, deg - 90.0);
    let r = polar(cx, cy, w, deg + 90.0);
    let tail = polar(cx, cy, len * 0.18, deg + 180.0);
    draw_triangle(tip, l, r, c);
    draw_triangle(tail, l, r, c);
    draw_circle(cx, cy, w * 1.3, Color::new(0.2, 0.2, 0.2, 1.0));
}

fn bezel(cx: f32, cy: f32, r: f32, label: &str) {
    draw_circle(cx, cy, r, BEZEL);
    draw_circle_lines(cx, cy, r * 0.985, 1.5, Color::new(1.0, 1.0, 1.0, 0.12));
    draw_circle(cx, cy, r * 0.93, FACE);
    draw_circle_lines(cx, cy, r * 0.93, 2.0, Color::new(0.0, 0.0, 0.0, 0.5));
    if !label.is_empty() {
        text_c(label, cx, cy + r * 0.38, (r * 0.17).max(9.0), DIM);
    }
}

fn readout(s: &str, cx: f32, cy: f32, r: f32, c: Color) {
    let fs = (r * 0.25).max(11.0);
    let w = text_w(s, fs) + fs * 0.6;
    rrect(cx - w / 2.0, cy - fs * 0.62, w, fs * 1.24, fs * 0.22, Color::new(0.0, 0.0, 0.0, 0.85));
    text_cb(s, cx, cy, fs, c);
}

pub fn commas(n: i32) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

// ---------------------------------------------------------- the six-pack ---

/// Airspeed indicator with the aircraft's arcs, and an optional target-speed bug.
fn asi(cx: f32, cy: f32, r: f32, t: &Telem, bug: Option<f32>) {
    let m = &t.craft.spec().asi;
    let kt = t.ias_kt;
    bezel(cx, cy, r, "");
    text_c("KT", cx, cy + r * 0.24, (r * 0.13).max(8.0), DIM);
    let a = |v: f32| v.clamp(0.0, m.max) / m.max * 330.0;
    let rr = r * 0.9;
    band(cx, cy, rr * 0.74, rr * 0.82, a(m.white.0), a(m.white.1), INK);
    band(cx, cy, rr * 0.82, rr * 0.92, a(m.green.0), a(m.green.1), GREEN_C);
    band(cx, cy, rr * 0.82, rr * 0.92, a(m.amber.0), a(m.amber.1), AMBER);
    tick(cx, cy, rr * 0.74, rr, a(m.red), 3.0, RED_C);
    let fs = (r * 0.155).max(8.0);
    let label_every = if m.max > 250.0 { 40 } else { 20 };
    for v in (0..=(m.max as i32 - 10)).step_by(10) {
        let long = v % label_every == 0;
        tick(cx, cy, rr * if long { 0.82 } else { 0.88 }, rr, a(v as f32), 1.5, INK);
        if long && v >= 40 {
            let p = polar(cx, cy, rr * 0.6, a(v as f32));
            text_c(&format!("{v}"), p.x, p.y, fs, INK);
        }
    }
    if let Some(b) = bug {
        let (p0, p1, p2) = (polar(cx, cy, rr * 1.02, a(b)), polar(cx, cy, rr * 1.12, a(b) - 4.0), polar(cx, cy, rr * 1.12, a(b) + 4.0));
        draw_triangle(p0, p1, p2, Color::new(1.0, 0.35, 0.95, 1.0));
    }
    readout(&format!("{:.0}", kt), cx, cy - r * 0.48, r * 0.85, INK);
    needle(cx, cy, rr * 0.92, a(kt), r * 0.05, INK);
}

fn clip_half(poly: &[Vec2], g: impl Fn(Vec2) -> f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ga, gb) = (g(a), g(b));
        if ga >= 0.0 {
            out.push(a);
        }
        if (ga >= 0.0) != (gb >= 0.0) {
            out.push(a + (b - a) * (ga / (ga - gb)));
        }
    }
    out
}

/// Attitude indicator, with ILS deviation scales when on the approach.
fn attitude(cx: f32, cy: f32, r: f32, pitch: f32, bank: f32, ils: Option<(f32, f32)>) {
    bezel(cx, cy, r, "");
    let rr = r * 0.88;
    let k = rr * 0.03; // px per degree of pitch
    let (sb, cb) = bank.to_radians().sin_cos();
    let scr = |p: Vec2| vec2(cx + p.x, cy - p.y);
    // local coords, y up; horizon-frame v = -x sin(b) + y cos(b); ground where v < -pitch*k
    draw_circle(cx, cy, rr, SKY_C);
    let circle: Vec<Vec2> = (0..48).map(|i| {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        vec2(a.cos() * rr, a.sin() * rr)
    }).collect();
    let ground = clip_half(&circle, |p| -(-p.x * sb + p.y * cb + pitch * k));
    for i in 1..ground.len().saturating_sub(1) {
        draw_triangle(scr(ground[0]), scr(ground[i]), scr(ground[i + 1]), EARTH_C);
    }
    let to_local = |u: f32, v: f32| vec2(u * cb - v * sb, u * sb + v * cb);
    let fs = (r * 0.13).max(8.0);
    for deg in (-30i32..=30).step_by(5) {
        let v = (deg as f32 - pitch) * k;
        if v.abs() > rr * 0.95 {
            continue;
        }
        let w = if deg == 0 { (rr * rr - v * v).max(0.0).sqrt() } else if deg % 10 == 0 { rr * 0.3 } else { rr * 0.14 };
        if deg != 0 && v.abs() > rr * 0.7 {
            continue;
        }
        let (a, b) = (scr(to_local(-w, v)), scr(to_local(w, v)));
        draw_line(a.x, a.y, b.x, b.y, if deg == 0 { 2.0 } else { 1.2 }, INK);
        if deg != 0 && deg % 10 == 0 {
            let p = scr(to_local(w + fs * 0.9, v));
            text_c(&format!("{}", deg.abs()), p.x, p.y, fs, INK);
        }
    }
    // bank scale
    for d in [-60.0, -45.0, -30.0, -20.0, -10.0, 0.0, 10.0, 20.0, 30.0, 45.0, 60.0f32] {
        let long = d.abs() == 30.0 || d.abs() == 60.0 || d == 0.0;
        tick(cx, cy, r * if long { 0.8 } else { 0.86 }, r * 0.93, d, 1.5, INK);
    }
    let pa = -bank;
    draw_triangle(polar(cx, cy, r * 0.79, pa), polar(cx, cy, r * 0.7, pa - 5.0), polar(cx, cy, r * 0.7, pa + 5.0), ORANGE);
    // fixed aircraft symbol
    let w = rr * 0.55;
    draw_line(cx - w, cy, cx - w * 0.35, cy, 3.0, ORANGE);
    draw_line(cx + w * 0.35, cy, cx + w, cy, 3.0, ORANGE);
    draw_line(cx - w * 0.35, cy, cx - w * 0.2, cy + w * 0.15, 3.0, ORANGE);
    draw_line(cx + w * 0.35, cy, cx + w * 0.2, cy + w * 0.15, 3.0, ORANGE);
    draw_circle(cx, cy, 3.0, ORANGE);
    if let Some((loc, gs)) = ils {
        // dots along the right edge (glideslope) and bottom edge (localizer), magenta diamonds
        let step = rr * 0.17;
        let mag = Color::new(1.0, 0.35, 0.95, 1.0);
        let dot = (r * 0.035).max(1.5);
        let (gx, ly) = (cx + rr * 0.8, cy + rr * 0.8);
        for k in [-2.0f32, -1.0, 1.0, 2.0] {
            draw_circle_lines(gx, cy + k * step, dot, 1.2, INK);
            draw_circle_lines(cx + k * step, ly, dot, 1.2, INK);
        }
        let d = r * 0.07;
        let diamond = |p: Vec2| {
            draw_triangle(p + vec2(0.0, -d), p + vec2(d * 0.7, 0.0), p + vec2(-d * 0.7, 0.0), mag);
            draw_triangle(p + vec2(0.0, d), p + vec2(d * 0.7, 0.0), p + vec2(-d * 0.7, 0.0), mag);
        };
        diamond(vec2(gx, cy - gs * step));
        diamond(vec2(cx + loc * step, ly));
    }
}

fn altimeter(cx: f32, cy: f32, r: f32, ft: f32) {
    bezel(cx, cy, r, "ALT FT");
    let rr = r * 0.9;
    let fs = (r * 0.17).max(8.0);
    for i in 0..50 {
        let long = i % 5 == 0;
        tick(cx, cy, rr * if long { 0.82 } else { 0.9 }, rr, i as f32 * 7.2, if long { 2.0 } else { 1.0 }, INK);
        if long {
            let p = polar(cx, cy, rr * 0.66, i as f32 * 7.2);
            text_c(&format!("{}", i / 5), p.x, p.y, fs, INK);
        }
    }
    readout(&commas(ft.round() as i32), cx, cy + r * 0.3, r * 0.85, INK);
    let ft = ft.max(0.0);
    needle(cx, cy, rr * 0.55, (ft % 10_000.0) / 10_000.0 * 360.0, r * 0.07, INK);
    needle(cx, cy, rr * 0.92, (ft % 1000.0) / 1000.0 * 360.0, r * 0.045, INK);
}

fn turn_coord(cx: f32, cy: f32, r: f32, rate: f32, slip: f32) {
    bezel(cx, cy, r, "");
    let rr = r * 0.9;
    for d in [-110.0, -90.0, 90.0, 110.0f32] {
        tick(cx, cy, rr * 0.82, rr, d, 2.5, INK);
    }
    let fs = (r * 0.18).max(9.0);
    let pl = polar(cx, cy, rr * 0.68, -118.0);
    let pr = polar(cx, cy, rr * 0.68, 118.0);
    text_c("L", pl.x, pl.y, fs, INK);
    text_c("R", pr.x, pr.y, fs, INK);
    text_c("2 MIN", cx, cy - r * 0.5, (r * 0.14).max(8.0), DIM);
    // little aeroplane, rolled by turn rate (standard rate 3 deg/s = 20 deg tilt)
    let tilt = (rate / 3.0 * 20.0).clamp(-45.0, 45.0);
    let (a, b) = (polar(cx, cy, rr * 0.75, 90.0 + tilt), polar(cx, cy, rr * 0.75, -90.0 + tilt));
    draw_line(a.x, a.y, b.x, b.y, 4.0, INK);
    let t = polar(cx, cy, rr * 0.22, tilt);
    draw_line(cx, cy, t.x, t.y, 3.0, INK);
    draw_circle(cx, cy, r * 0.09, INK);
    // inclinometer
    band(cx, cy, rr * 0.55, rr * 0.75, 152.0, 208.0, Color::new(0.85, 0.85, 0.8, 0.9));
    tick(cx, cy, rr * 0.55, rr * 0.75, 172.0, 1.5, BEZEL);
    tick(cx, cy, rr * 0.55, rr * 0.75, 188.0, 1.5, BEZEL);
    let ball = polar(cx, cy, rr * 0.65, 180.0 - slip * 25.0);
    draw_circle(ball.x, ball.y, rr * 0.085, Color::new(0.05, 0.05, 0.05, 1.0));
}

fn heading_ind(cx: f32, cy: f32, r: f32, hdg: f32) {
    bezel(cx, cy, r, "");
    let rr = r * 0.9;
    let fs = (r * 0.17).max(8.0);
    for d in (0..360).step_by(5) {
        let a = d as f32 - hdg;
        let long = d % 10 == 0;
        tick(cx, cy, rr * if long { 0.84 } else { 0.91 }, rr, a, 1.3, INK);
        if d % 30 == 0 {
            let label = match d {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "W".to_string(),
                _ => format!("{}", d / 10),
            };
            let p = polar(cx, cy, rr * 0.67, a);
            text_c(&label, p.x, p.y, fs, if d % 90 == 0 { AMBER } else { INK });
        }
    }
    draw_triangle(polar(cx, cy, rr * 1.02, 0.0), polar(cx, cy, rr * 0.86, -6.0), polar(cx, cy, rr * 0.86, 6.0), ORANGE);
    // aircraft silhouette
    let w = rr * 0.3;
    draw_line(cx, cy - w, cx, cy + w, 3.0, ORANGE);
    draw_line(cx - w * 0.8, cy - w * 0.1, cx + w * 0.8, cy - w * 0.1, 3.0, ORANGE);
    draw_line(cx - w * 0.35, cy + w * 0.85, cx + w * 0.35, cy + w * 0.85, 2.5, ORANGE);
    readout(&format!("{:03.0}", hdg.round() % 360.0), cx, cy + r * 0.5, r * 0.8, INK);
}

fn vsi(cx: f32, cy: f32, r: f32, fpm: f32) {
    bezel(cx, cy, r, "");
    let rr = r * 0.9;
    let a = |v: f32| -90.0 + (v / 2000.0).clamp(-1.0, 1.0) * 170.0;
    let fs = (r * 0.16).max(8.0);
    for v in (-2000i32..=2000).step_by(100) {
        let long = v % 500 == 0;
        tick(cx, cy, rr * if long { 0.8 } else { 0.9 }, rr, a(v as f32), if long { 2.0 } else { 1.0 }, INK);
        if long {
            let p = polar(cx, cy, rr * 0.62, a(v as f32));
            text_c(&format!("{}", v.abs() / 100), p.x, p.y, fs, INK);
        }
    }
    text_c("UP", cx + r * 0.05, cy - r * 0.32, (r * 0.15).max(8.0), DIM);
    text_c("DN", cx + r * 0.05, cy + r * 0.32, (r * 0.15).max(8.0), DIM);
    text_c("x100", cx + r * 0.22, cy, (r * 0.12).max(7.0), DIM);
    needle(cx, cy, rr * 0.92, a(fpm), r * 0.045, INK);
    let s = if fpm >= 0.0 { format!("+{}", commas((fpm / 10.0).round() as i32 * 10)) } else { commas((fpm / 10.0).round() as i32 * 10) };
    readout(&s, cx + r * 0.3, cy + r * 0.55, r * 0.7, INK);
}

fn vbar(x: f32, y: f32, w: f32, h: f32, frac: f32, c: Color, label: &str, value: &str, fs: f32) {
    draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.0, 0.55));
    let f = frac.clamp(0.0, 1.0);
    draw_rectangle(x + 2.0, y + 2.0 + (h - 4.0) * (1.0 - f), w - 4.0, (h - 4.0) * f, c);
    draw_rectangle_lines(x, y, w, h, 1.0, DIM);
    text_c(label, x + w / 2.0, y + h + fs * 0.75, fs * 0.85, DIM);
    text_c(value, x + w / 2.0, y - fs * 0.6, fs * 0.9, INK);
}

/// Throttle, flaps, trim, angle of attack and a few status lights.
fn cluster_layout(h: f32) -> (f32, f32, f32) {
    let fs = (h * 0.12).clamp(10.0, 16.0 * ui());
    let w = (h * 0.16).max(14.0);
    (fs, w, (w * 0.95).max(fs * 2.4))
}

fn cluster_width(h: f32) -> f32 {
    let (fs, w, gap) = cluster_layout(h);
    4.0 * (w + gap) - gap * 0.2 + text_w("TRIM 100 KT", fs)
}

fn cluster(x: f32, y: f32, h: f32, t: &Telem, auto_rudder: bool) {
    let (fs, w, gap) = cluster_layout(h);
    let spec = t.craft.spec();
    let jet = spec.is_jet();
    let top = y + fs * 1.2;
    let bh = h - fs * 2.6;
    let mut cx = x;
    if jet {
        // fan speed from idle to full, with the thrust lever as a tick
        let (bar_c, value) = if t.reverser > 0.0 {
            (if t.reverser > 0.9 { GREEN_C } else { AMBER }, "REV".to_string())
        } else {
            (Color::new(0.9, 0.9, 0.9, 0.9), format!("{:.0}", t.n1))
        };
        vbar(cx, top, w, bh, (t.n1 - 20.0) / 80.0, bar_c, "N1", &value, fs);
        let ly = top + bh * (1.0 - t.throttle);
        draw_line(cx - 3.0, ly, cx + w + 3.0, ly, 2.0, AMBER);
    } else if !spec.has_engine() {
        // variometer: climb above the centre line, sink below, full scale 5 m/s
        draw_rectangle(cx, top, w, bh, Color::new(0.0, 0.0, 0.0, 0.6));
        draw_rectangle_lines(cx, top, w, bh, 1.0, DIM);
        let mid = top + bh / 2.0;
        draw_line(cx, mid, cx + w, mid, 1.0, DIM);
        let v = t.vario.clamp(-5.0, 5.0);
        let y = mid - v / 5.0 * bh * 0.5;
        let c = if v > 0.0 { GREEN_C } else { AMBER };
        draw_rectangle(cx + 2.0, y.min(mid), w - 4.0, (y - mid).abs(), c);
        text_c("VARIO", cx + w / 2.0, top + bh + fs * 0.75, fs * 0.85, DIM);
        text_c(&format!("{:+.1}", t.vario), cx + w / 2.0, top - fs * 0.6, fs * 0.9, c);
    } else {
        vbar(cx, top, w, bh, t.throttle, Color::new(0.9, 0.9, 0.9, 0.9), "THR", &format!("{:.0}%", t.throttle * 100.0), fs);
    }
    cx += w + gap;
    // flaps: one notch per lever detent, top to bottom
    draw_rectangle(cx, top, w, bh, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(cx, top, w, bh, 1.0, DIM);
    let n = spec.flaps.len() - 1;
    let notch_y = |i: f32| top + bh * (0.1 + 0.8 * i / n as f32);
    for k in 0..=n {
        let yy = notch_y(k as f32);
        draw_line(cx, yy, cx + w * 0.3, yy, 1.0, DIM);
    }
    // where the flaps actually are, interpolated between detents
    let pos = spec.flaps.windows(2).enumerate().find(|(_, d)| t.flap_deg <= d[1]).map_or(n as f32, |(i, d)| i as f32 + (t.flap_deg - d[0]) / (d[1] - d[0]).max(1e-3));
    let fy = notch_y(pos);
    draw_rectangle(cx + 2.0, fy - 3.0, w - 4.0, 6.0, INK);
    text_c("FLAPS", cx + w / 2.0, top + bh + fs * 0.75, fs * 0.85, DIM);
    text_c(&format!("{}", spec.flaps[t.flaps as usize]), cx + w / 2.0, top - fs * 0.6, fs * 0.9, INK);
    cx += w + gap;
    // trim: centre is neutral, up = nose up
    draw_rectangle(cx, top, w, bh, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(cx, top, w, bh, 1.0, DIM);
    draw_line(cx, top + bh / 2.0, cx + w, top + bh / 2.0, 1.0, DIM);
    let ty = top + bh / 2.0 - t.trim / 0.6 * bh * 0.45;
    draw_rectangle(cx + 2.0, ty - 3.0, w - 4.0, 6.0, GREEN_C);
    text_c("TRIM", cx + w / 2.0, top + bh + fs * 0.75, fs * 0.85, DIM);
    let tl = if t.trim > 0.02 { "UP" } else if t.trim < -0.02 { "DN" } else { "--" };
    text_c(tl, cx + w / 2.0, top - fs * 0.6, fs * 0.9, INK);
    cx += w + gap;
    // angle of attack, scaled so the top of the bar is just past the stall
    let max_aoa = t.stall_aoa + 4.0;
    draw_rectangle(cx, top, w, bh, Color::new(0.0, 0.0, 0.0, 0.6));
    let zone = |a0: f32, a1: f32, c: Color| {
        let (y0, y1) = (top + bh * (1.0 - a1 / max_aoa), top + bh * (1.0 - a0 / max_aoa));
        draw_rectangle(cx + 2.0, y0, 4.0, y1 - y0, c);
    };
    zone(0.0, t.stall_aoa - 5.0, GREEN_C);
    zone(t.stall_aoa - 5.0, t.stall_aoa, AMBER);
    zone(t.stall_aoa, max_aoa, RED_C);
    draw_rectangle_lines(cx, top, w, bh, 1.0, DIM);
    let ay = top + bh * (1.0 - (t.aoa.clamp(0.0, max_aoa) / max_aoa));
    let ac = if t.aoa > t.stall_aoa { RED_C } else if t.aoa > t.stall_aoa - 5.0 { AMBER } else { INK };
    draw_rectangle(cx + 6.0, ay - 3.0, w - 8.0, 6.0, ac);
    text_c("AoA", cx + w / 2.0, top + bh + fs * 0.75, fs * 0.85, DIM);
    text_c(&format!("{:.0}", t.aoa), cx + w / 2.0, top - fs * 0.6, fs * 0.9, ac);
    cx += w + gap * 0.8;
    // text column
    let off = Color::new(0.3, 0.3, 0.3, 1.0);
    let trim_s = if t.trim_kt > 0.0 && !t.on_ground { format!("TRIM {:.0} KT", t.trim_kt) } else { "TRIM --".into() };
    let brake = (format!("BRAKE{}", if t.brake > 0.0 && t.brake < 0.99 { format!(" {:.0}%", t.brake * 100.0) } else { String::new() }), if t.brake > 0.0 { RED_C } else { off });
    let mut lines: Vec<(String, Color)> = Vec::new();
    if jet {
        lines.push((trim_s, GREEN_C));
        lines.push((format!("G {:.1}", t.g), if t.g > 2.5 || t.g < 0.0 { AMBER } else { INK }));
        lines.push(brake);
        lines.push(match t.speedbrake {
            Speedbrake::Down if t.spoiler > 0.02 => ("SPD BRK".into(), AMBER),
            Speedbrake::Down => ("SPD BRK".into(), off),
            Speedbrake::Armed => ("SPD BRK ARM".into(), GREEN_C),
            Speedbrake::Up => ("SPD BRK UP".into(), AMBER),
        });
        lines.push(if t.gear > 0.99 { ("GEAR DOWN".into(), GREEN_C) } else if t.gear < 0.01 { ("GEAR UP".into(), off) } else { ("GEAR".into(), RED_C) });
    } else if !spec.has_engine() {
        lines.push((format!("VARIO {:+.1} m/s", t.vario), if t.vario > 0.0 { GREEN_C } else { INK }));
        lines.push((trim_s, GREEN_C));
        lines.push((format!("G {:.1}", t.g), INK));
        lines.push(brake);
        lines.push(("AIRBRAKE".into(), if t.spoiler > 0.05 { AMBER } else { off }));
    } else {
        lines.push((format!("RPM {:.0}", t.rpm), INK));
        lines.push((trim_s, GREEN_C));
        lines.push((format!("G {:.1}", t.g), if t.g > 3.5 || t.g < -1.0 { AMBER } else { INK }));
        lines.push(brake);
    }
    lines.push(("AUTO-RUD".into(), if auto_rudder { GREEN_C } else { off }));
    let spacing = if lines.len() > 5 { 1.02 } else { 1.15 };
    for (i, (s, c)) in lines.iter().enumerate() {
        text(s, cx, top + fs * (0.8 + i as f32 * spacing), fs, *c);
    }
}

pub struct PanelInfo {
    pub auto_rudder: bool,
    /// Target approach speed to mark on the airspeed indicator.
    pub speed_bug: Option<f32>,
}

/// How much of the instrument panel to show (Tab cycles).
#[derive(Clone, Copy, PartialEq)]
pub enum PanelMode {
    Full,
    Compact,
    Off,
}

impl PanelMode {
    pub fn next(self) -> Self {
        match self {
            PanelMode::Full => PanelMode::Compact,
            PanelMode::Compact => PanelMode::Off,
            PanelMode::Off => PanelMode::Full,
        }
    }
}

/// Top edge of the instrument panel (see `panel`).
pub fn panel_top(mode: PanelMode) -> f32 {
    let (sw, sh) = (screen_width(), screen_height());
    if mode == PanelMode::Off {
        return sh - 6.0 * ui();
    }
    if mode == PanelMode::Compact {
        let fs = (sh / 42.0).clamp(13.0, 17.0 * ui());
        return sh - fs * 2.4;
    }
    let r = (sh * 0.085).min(sw * 0.04).max(36.0);
    let gap = r * 0.18;
    sh - 2.0 * r - gap * 2.5
}

/// A window on the autopilot mode control panel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum McpField {
    At,
    Spd,
    Hdg,
    Alt,
    Vs,
    App,
    Cmd,
}

/// Where each MCP window sits, in a row just above the instrument panel.
pub fn mcp_layout(jet: bool, bottom: f32) -> Vec<(McpField, Rect)> {
    let sw = screen_width();
    let fs = 12.5 * ui();
    let (w, h, gap) = (fs * 5.0, fs * 2.7, fs * 0.45);
    let fields: &[McpField] = if jet {
        &[McpField::At, McpField::Spd, McpField::Hdg, McpField::Alt, McpField::Vs, McpField::App, McpField::Cmd]
    } else {
        &[McpField::Hdg, McpField::Alt, McpField::Vs, McpField::App, McpField::Cmd]
    };
    let total = fields.len() as f32 * (w + gap) - gap;
    let x0 = (sw - total) / 2.0;
    let y = bottom - h - fs * 1.9;
    fields.iter().enumerate().map(|(i, f)| (*f, Rect::new(x0 + i as f32 * (w + gap), y, w, h))).collect()
}

/// The mode control panel and, above it, the flight mode annunciations. Returns its top edge.
pub fn mcp(ap: &crate::ap::Autopilot, jet: bool, bottom: f32, hover: Option<McpField>) -> f32 {
    use crate::ap::{Lateral, Vertical};
    let fs = 12.5 * ui();
    let layout = mcp_layout(jet, bottom);
    let first = layout[0].1;
    let last = layout[layout.len() - 1].1;
    let pad = fs * 0.4;
    card(first.x - pad, first.y - pad, last.x + last.w - first.x + pad * 2.0, first.h + pad * 2.0);
    for (f, r) in &layout {
        let (label, value, lit) = match f {
            McpField::At => ("A/T", if ap.at { "ARM" } else { "OFF" }.to_string(), ap.at),
            McpField::Spd => ("IAS", format!("{:.0}", ap.spd_kt), ap.at && !ap.retard),
            McpField::Hdg => ("HDG", format!("{:03.0}", ap.hdg.rem_euclid(360.0)), ap.engaged && ap.lateral == Lateral::Hdg),
            McpField::Alt => ("ALT", commas(ap.alt_ft as i32), ap.engaged && matches!(ap.vertical, Vertical::Alt | Vertical::AltAcq)),
            McpField::Vs => ("V/S", format!("{:+.0}", ap.vs_fpm), ap.engaged && ap.vertical == Vertical::Vs),
            McpField::App => ("APP", if ap.app { "ARM" } else { "OFF" }.to_string(), ap.app),
            McpField::Cmd => ("AP", if ap.engaged { "CMD" } else { "OFF" }.to_string(), ap.engaged),
        };
        let bg = if hover == Some(*f) { Color::new(1.0, 1.0, 1.0, 0.12) } else { Color::new(0.0, 0.0, 0.0, 0.45) };
        rrect(r.x, r.y, r.w, r.h, fs * 0.35, bg);
        if lit {
            rrect(r.x + r.w * 0.3, r.y + r.h - fs * 0.32, r.w * 0.4, fs * 0.18, fs * 0.09, GREEN_C);
        }
        text_c(label, r.x + r.w / 2.0, r.y + fs * 0.65, fs * 0.72, DIM);
        let vc = if matches!(f, McpField::Cmd | McpField::At | McpField::App) && !lit { DIM } else { AMBER };
        text_cb(&value, r.x + r.w / 2.0, r.y + fs * 1.65, fs * 1.1, vc);
    }
    // flight mode annunciator: active modes in green, armed ones in white
    let lat = match ap.lateral {
        Lateral::Roll => "ROLL",
        Lateral::Hdg => "HDG SEL",
        Lateral::Loc => "VOR/LOC",
        Lateral::Rollout => "ROLLOUT",
    };
    let vert = match ap.vertical {
        Vertical::Vs => "V/S",
        Vertical::AltAcq => "ALT ACQ",
        Vertical::Alt => "ALT HOLD",
        Vertical::Gs => "G/S",
        Vertical::Flare => "FLARE",
    };
    let thr = if !ap.at { "" } else if ap.retard { "RETARD" } else { "SPD" };
    let mut armed = Vec::new();
    if ap.app && ap.lateral != Lateral::Loc && ap.lateral != Lateral::Rollout {
        armed.push("VOR/LOC");
    }
    if ap.app && !matches!(ap.vertical, Vertical::Gs | Vertical::Flare) {
        armed.push("G/S");
    }
    let y = first.y - pad - fs * 0.85;
    let cx = (first.x + last.x + last.w) / 2.0;
    if ap.engaged || ap.at {
        let modes = [thr, if ap.engaged { lat } else { "" }, if ap.engaged { vert } else { "" }];
        let cell = (last.x + last.w - first.x) / 3.0;
        for (i, m) in modes.iter().enumerate() {
            if !m.is_empty() {
                pill(m, first.x + cell * (i as f32 + 0.5), y, fs * 0.9, GREEN_C, Color::new(0.0, 0.0, 0.0, 0.6), true);
            }
        }
        if !armed.is_empty() {
            text_c(&armed.join("   "), cx, y - fs * 1.35, fs * 0.8, INK);
        }
    }
    y - fs * 2.2
}

/// Instrument panel along the bottom of the screen. Returns its top edge.
pub fn panel(t: &Telem, info: &PanelInfo, mode: PanelMode) -> f32 {
    let (sw, sh) = (screen_width(), screen_height());
    if mode == PanelMode::Off {
        return panel_top(mode);
    }
    if mode == PanelMode::Compact {
        let fs = (sh / 42.0).clamp(13.0, 17.0 * ui());
        let s = format!(
            "IAS {:.0} kt   ALT {} ft   VS {:+.0} fpm   HDG {:03.0}   THR {:.0}%   FLAPS {}   TRIM {:+.2}",
            t.ias_kt, commas(t.alt_ft.round() as i32), t.vs_fpm, t.hdg.round() % 360.0, t.throttle * 100.0, t.craft.spec().flaps[t.flaps as usize], t.trim
        );
        let w = text_w(&s, fs) + fs * 1.6;
        let y = sh - fs * 2.0;
        card((sw - w) / 2.0, y - fs * 0.4, w, fs * 1.8);
        text_c(&s, sw / 2.0, y + fs * 0.5, fs, INK);
        return y - fs * 0.4;
    }
    let r = (sh * 0.085).min(sw * 0.04).max(36.0);
    let gap = r * 0.18;
    let cluster_w = cluster_width(2.0 * r);
    let total = 6.0 * 2.0 * r + 5.0 * gap + gap * 2.0 + cluster_w;
    let x0 = ((sw - total) / 2.0).max(4.0);
    let cy = sh - r - gap * 1.5;
    let top = cy - r - gap;
    // the bottom edge runs off screen so only the top corners are rounded
    card(x0 - gap, top, total + gap * 2.0, sh - top + 20.0);
    let mut cx = x0 + r;
    asi(cx, cy, r, t, info.speed_bug);
    cx += 2.0 * r + gap;
    attitude(cx, cy, r, t.pitch, t.bank, crate::lessons::ils(t.pos, t.hdg).filter(|_| !t.on_ground));
    if t.craft.spec().is_jet() && t.agl_ft < 2500.0 && !t.on_ground {
        // radio altitude, the number the callouts are reading out
        readout(&format!("RA {:.0}", t.agl_ft), cx, cy + r * 0.55, r * 0.75, if t.agl_ft < 200.0 { AMBER } else { GREEN_C });
    }
    cx += 2.0 * r + gap;
    altimeter(cx, cy, r, t.alt_ft);
    cx += 2.0 * r + gap;
    turn_coord(cx, cy, r, t.turn_rate, t.slip);
    cx += 2.0 * r + gap;
    heading_ind(cx, cy, r, t.hdg);
    cx += 2.0 * r + gap;
    vsi(cx, cy, r, t.vs_fpm);
    cluster(cx + r + gap * 2.0, cy - r, 2.0 * r, t, info.auto_rudder);
    top
}

// ------------------------------------------------------------- overlays ---

fn map_rect() -> (f32, f32, f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let s = (sw * 0.14).min(sh * 0.22).clamp(120.0, 210.0 * ui());
    let m = 14.0 * ui();
    (sw - s - m, m, s)
}

pub fn minimap(tex: &Texture2D, world: &World, t: &Telem, next_ring: Option<usize>, thermals: bool, traffic: &[Vec3]) {
    let (x, y, s) = map_rect();
    let u = ui();
    let pad = 5.0 * u;
    card(x - pad, y - pad, s + pad * 2.0, s + pad * 2.0);
    draw_texture_ex(tex, x, y, WHITE, DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() });
    let map = |p: Vec3| vec2(x + (p.x + HALF) / (2.0 * HALF) * s, y + (p.z + HALF) / (2.0 * HALF) * s);
    let (a, b) = (map(vec3(-RWY_HALF_LEN, 0.0, 0.0)), map(vec3(RWY_HALF_LEN, 0.0, 0.0)));
    draw_line(a.x, a.y, b.x, b.y, 2.5 * u, Color::new(0.15, 0.15, 0.15, 1.0));
    if thermals {
        // soaring pilots' thermal map: swirls where the air rises
        for th in &world.thermals {
            let p = map(vec3(th.x, 0.0, th.z));
            let r = (th.radius / (2.0 * HALF) * s * 2.5).max(2.5 * u);
            draw_circle_lines(p.x, p.y, r, 1.2 * u, Color::new(1.0, 0.9, 0.4, 0.25 + 0.12 * th.strength));
        }
    }
    if let Some(n) = next_ring {
        for (i, r) in world.rings.iter().enumerate().skip(n) {
            let p = map(r.pos);
            draw_circle(p.x, p.y, if i == n { 4.0 * u } else { 2.2 * u }, if i == n { AMBER } else { Color::new(0.4, 0.75, 1.0, 1.0) });
        }
    }
    // other aircraft, as on a traffic display
    for q in traffic {
        let p = map(*q);
        draw_circle(p.x, p.y, 2.6 * u, Color::new(0.4, 0.9, 1.0, 1.0));
        draw_circle_lines(p.x, p.y, 2.6 * u, 1.0, BLACK);
    }
    let p = map(t.pos);
    let h = t.hdg;
    let dir = |d: f32, l: f32| p + vec2((h + d).to_radians().sin(), -(h + d).to_radians().cos()) * l * u;
    let (tip, l, r) = (dir(0.0, 8.0), dir(-140.0, 6.0), dir(140.0, 6.0));
    draw_triangle(tip, l, r, Color::new(1.0, 0.95, 0.2, 1.0));
    draw_triangle_lines(tip, l, r, 1.0, BLACK);
    text_cb("N", x + s / 2.0, y + 10.0 * u, 12.0 * u, INK);
}

/// Marker for the next ring: a diamond on screen, or an arrow at the edge.
pub fn ring_marker(vp: &Mat4, pos: Vec3, eye: Vec3, top_limit: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    let fs = 13.0 * u;
    let c = *vp * pos.extend(1.0);
    let dist = (pos - eye).length();
    let label = if dist > 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{:.0} m", dist) };
    let center = vec2(sw / 2.0, sh / 2.0);
    if c.w > 0.1 {
        let p = vec2((c.x / c.w * 0.5 + 0.5) * sw, (0.5 - c.y / c.w * 0.5) * sh);
        if p.x > 30.0 && p.x < sw - 30.0 && p.y > 30.0 && p.y < top_limit - 10.0 {
            let s = 11.0 * u;
            for (a, b) in [(vec2(0.0, -s), vec2(s, 0.0)), (vec2(s, 0.0), vec2(0.0, s)), (vec2(0.0, s), vec2(-s, 0.0)), (vec2(-s, 0.0), vec2(0.0, -s))] {
                draw_line(p.x + a.x, p.y + a.y, p.x + b.x, p.y + b.y, 2.0 * u, AMBER);
            }
            text_cb(&label, p.x, p.y + s + fs, fs, AMBER);
            return;
        }
    }
    let mut dir = vec2(c.x, -c.y);
    if c.w <= 0.1 {
        dir = -dir;
    }
    let dir = dir.try_normalize().unwrap_or(vec2(0.0, -1.0));
    let ext = vec2(sw * 0.42, (top_limit - 40.0) * 0.5 - 10.0);
    let k = (ext.x / dir.x.abs().max(1e-3)).min(ext.y / dir.y.abs().max(1e-3));
    let mid = vec2(center.x, top_limit * 0.5);
    let p = mid + dir * k;
    let perp = vec2(-dir.y, dir.x);
    draw_triangle(p + dir * 13.0 * u, p + perp * 8.0 * u, p - perp * 8.0 * u, AMBER);
    text_cb(&label, p.x - dir.x * 26.0 * u, p.y - dir.y * 20.0 * u, fs, AMBER);
}

/// PAPI repeater shown when approaching runway 09.
pub fn papi_widget(t: &Telem, y: f32) {
    let near = t.pos.x < -RWY_HALF_LEN + 200.0 && t.pos.x > -RWY_HALF_LEN - 6000.0 && t.pos.z.abs() < 1500.0
        && hdg_err(t.hdg, 90.0) < 60.0 && t.agl_ft < 2000.0 && !t.on_ground;
    if !near {
        return;
    }
    let a = glide_angle(t.pos);
    if t.pos.x > -RWY_HALF_LEN || a > 7.0 {
        return;
    }
    let lights = papi_lights();
    let sw = screen_width();
    let u = ui();
    let s = 9.0 * u;
    let x0 = sw / 2.0 - 2.0 * s * 2.2 + s * 1.1;
    card(x0 - s * 1.6, y - s * 1.6, s * 2.2 * 4.0 + s * 1.0, s * 4.6);
    for i in 0..4 {
        // pilot sees the outermost light (lowest angle) on the left
        let (_, thr) = lights[3 - i];
        let white = a > thr;
        draw_circle(x0 + i as f32 * s * 2.2, y, s, if white { WHITE } else { Color::new(1.0, 0.15, 0.1, 1.0) });
    }
    text_c("PAPI", sw / 2.0, y + s * 2.0, 10.0 * u, DIM);
}

/// A rounded "pill" of text, centred at (cx, cy).
pub fn pill(s: &str, cx: f32, cy: f32, fs: f32, c: Color, bg: Color, bold: bool) {
    let w = measure(s, fs, bold).width + fs * 1.4;
    let h = fs * 1.6;
    rrect(cx - w / 2.0, cy - h / 2.0, w, h, h / 2.0, bg);
    text_cx(s, cx, cy, fs, c, bold);
}

pub fn warnings(t: &Telem, extra: Option<&str>, time: f32, y: f32) {
    let sw = screen_width();
    let fs = (screen_height() / 26.0).clamp(20.0, 30.0 * ui());
    let flash = (time * 4.0).fract() < 0.6;
    let mut msgs: Vec<(&str, Color)> = Vec::new();
    if t.stalled && flash {
        msgs.push(("STALL - NOSE DOWN!", RED_C));
    } else if t.stall_warn && !t.stalled {
        msgs.push(("STALL WARNING", AMBER));
    }
    if t.ias_kt > t.craft.spec().vne_kt {
        msgs.push(("OVERSPEED", RED_C));
    }
    if !t.on_ground && t.vs_fpm < -1500.0 && t.agl_ft < 1000.0 && flash {
        msgs.push(("PULL UP", RED_C));
    }
    if t.bank.abs() > 60.0 && !t.on_ground {
        msgs.push(("BANK ANGLE", AMBER));
    }
    if Vec3::new(t.pos.x, 0.0, t.pos.z).length() > 13_000.0 {
        msgs.push(("Leaving the island - turn back", AMBER));
    }
    if let Some(e) = extra {
        msgs.push((e, AMBER));
    }
    let mut yy = y;
    for (i, (m, c)) in msgs.iter().enumerate() {
        let size = if i == 0 { fs } else { fs * 0.62 };
        pill(m, sw / 2.0, yy, size, *c, Color::new(0.0, 0.0, 0.0, 0.6), i == 0);
        yy += size * 1.6 + fs * 0.25;
    }
}

/// Step marker: tick in a green disc when done, an amber ring for the current one.
fn step_mark(x: f32, y: f32, r: f32, state: i32) {
    match state {
        0 => {
            draw_circle(x, y, r, GREEN_C);
            let (a, b, c) = (vec2(x - r * 0.45, y + r * 0.02), vec2(x - r * 0.1, y + r * 0.38), vec2(x + r * 0.5, y - r * 0.35));
            draw_line(a.x, a.y, b.x, b.y, r * 0.28, Color::new(0.02, 0.1, 0.04, 1.0));
            draw_line(b.x, b.y, c.x, c.y, r * 0.28, Color::new(0.02, 0.1, 0.04, 1.0));
        }
        1 => {
            draw_circle_lines(x, y, r * 0.88, r * 0.24, AMBER);
            draw_circle(x, y, r * 0.35, AMBER);
        }
        _ => draw_circle_lines(x, y, r * 0.85, r * 0.14, DIM),
    }
}

/// Returns the panel's bottom-right corner. `small` shows only the current step and its hint.
pub fn lesson_panel(lesson: &Lesson, m: &Mission, t: &Telem, small: bool) -> Vec2 {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    let fs = (sh / 58.0).clamp(11.5, 15.0 * u);
    let w = (sw * 0.3).clamp(250.0, 370.0 * u);
    let (x, y) = (14.0 * u, 14.0 * u);
    let pad = fs * 0.9;
    let inner = w - pad * 2.0;
    // (text, colour, size, bold, step marker state)
    let mut lines: Vec<(String, Color, f32, bool, Option<i32>)> = Vec::new();
    for l in wrap(lesson.name, inner / 1.1, fs * 1.2) {
        lines.push((l, AMBER, fs * 1.2, true, None));
    }
    lines.push((String::new(), INK, fs * 0.25, false, None));
    match lesson.kind {
        Kind::Lesson => {
            for (i, s) in lesson.steps.iter().enumerate() {
                if small && i != m.step {
                    continue;
                }
                let (state, c) = if i < m.step || m.complete { (0, DIM) } else if i == m.step { (1, INK) } else { (2, DIM) };
                lines.push((s.title.to_string(), c, fs * 0.95, i == m.step && !m.complete, Some(state)));
            }
            if !m.complete {
                let s = &lesson.steps[m.step];
                if !small {
                    lines.push((String::new(), INK, fs * 0.5, false, None));
                    for l in wrap(s.text, inner, fs) {
                        lines.push((l, INK, fs, false, None));
                    }
                }
                if let Some(h) = (s.hint)(t) {
                    lines.push((String::new(), INK, fs * 0.4, false, None));
                    for l in wrap(h, inner - fs * 0.6, fs) {
                        lines.push((l, AMBER, fs, true, Some(-1)));
                    }
                }
            }
        }
        Kind::Rings | Kind::Free | Kind::Landing => {
            if !small {
                for l in wrap(lesson.blurb, inner, fs) {
                    lines.push((l, INK, fs, false, None));
                }
            }
        }
    }
    let lh = 1.38;
    let mut h = pad * 2.0 + lines.iter().map(|l| l.2 * lh).sum::<f32>();
    let show_bar = lesson.kind == Kind::Lesson && !m.complete && lesson.steps[m.step].hold > 0.0;
    if show_bar {
        h += fs * 1.2;
    }
    h += fs * 1.4;
    card(x, y, w, h);
    let mut yy = y + pad;
    let mut hint_top = None;
    for (s, c, size, bold, mark) in &lines {
        yy += size * lh;
        let base = yy - size * 0.32;
        match mark {
            Some(-1) => {
                hint_top.get_or_insert(yy - size * lh);
                text_b(s, x + pad + fs * 0.6, base, *size, *c);
            }
            Some(state) => {
                step_mark(x + pad + size * 0.45, base - size * 0.33, size * 0.45, *state);
                draw_str(s, x + pad + size * 1.45, base, *size, *c, *bold);
            }
            None => draw_str(s, x + pad, base, *size, *c, *bold),
        }
    }
    if let Some(top) = hint_top {
        // a coloured bar marks the coach's live hint
        draw_rectangle(x + pad, top + fs * 0.2, 3.0 * u, yy - top - fs * 0.05, AMBER);
    }
    if show_bar {
        let s = &lesson.steps[m.step];
        yy += fs * 0.55;
        let f = (m.hold / s.hold).clamp(0.0, 1.0);
        let bh = fs * 0.42;
        rrect(x + pad, yy, inner, bh, bh / 2.0, Color::new(1.0, 1.0, 1.0, 0.13));
        if f > 0.0 {
            rrect(x + pad, yy, (inner * f).max(bh), bh, bh / 2.0, GREEN_C);
        }
        yy += fs * 0.65;
    }
    let footer = if small { "O  more" } else if lesson.kind == Kind::Lesson { "N  skip step     R  restart     O  less     H  help" } else { "R  restart     O  less     H  help" };
    text(footer, x + pad, yy + fs * 1.15, fs * 0.82, DIM);
    vec2(x + w, y + h)
}

/// Race clock, centred at the top, or tucked under the lesson panel (`avoid`) if the window is narrow.
pub fn rings_status(m: &Mission, total: usize, best: Option<f32>, avoid: Vec2) {
    let sw = screen_width();
    let u = ui();
    let (_, _, map) = map_rect();
    let fs = 17.0 * u;
    let s = format!(
        "RING {}/{}     {}     BEST {}",
        (m.next_ring + 1).min(total),
        total,
        fmt_time(m.time),
        best.map(fmt_time).unwrap_or_else(|| "--".into())
    );
    let w = measure(&s, fs, true).width + fs * 2.0;
    let margin = 14.0 * u;
    let (x, y) = if sw / 2.0 - w / 2.0 > avoid.x + margin && sw / 2.0 + w / 2.0 < sw - map - margin * 2.0 {
        (sw / 2.0 - w / 2.0, margin)
    } else {
        (margin, avoid.y + margin * 0.6)
    };
    card(x, y, w, fs * 1.9);
    text_cb(&s, x + w / 2.0, y + fs * 0.95, fs, INK);
}

pub fn toasts(list: &[(String, f32)]) {
    let (sw, sh) = (screen_width(), screen_height());
    let fs = 19.0 * ui();
    for (i, (s, ttl)) in list.iter().enumerate() {
        let a = ttl.min(1.0);
        // slide up a little as they appear
        let rise = ((2.5 - ttl).min(0.25) / 0.25 - 1.0) * fs * 0.5;
        let y = sh * 0.3 + i as f32 * fs * 2.0 - rise;
        pill(s, sw / 2.0, y, fs, Color::new(GREEN_C.r, GREEN_C.g, GREEN_C.b, a), Color::new(0.0, 0.0, 0.0, 0.55 * a), true);
    }
}

/// Centered card with a title and lines of text.
pub fn dialog(title: &str, title_c: Color, body: &[String], footer: &str) {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.4));
    let fs = (sh / 40.0).clamp(15.0, 19.0 * u);
    let w = (sw * 0.5).clamp(360.0, 640.0 * u);
    let inner = w - fs * 3.0;
    let mut lines = Vec::new();
    for b in body {
        if b.is_empty() {
            lines.push(String::new());
        }
        lines.extend(wrap(b, inner, fs));
    }
    let h = fs * 5.0 + lines.len() as f32 * fs * 1.45 + fs * 2.2;
    let (x, y) = ((sw - w) / 2.0, (sh - h) / 2.0);
    let r = 14.0 * u;
    rrect(x, y, w, h, r, Color::new(0.04, 0.055, 0.08, 0.94));
    rrect_lines(x, y, w, h, r, 1.0, EDGE);
    rrect(x + w * 0.5 - fs * 2.0, y + fs * 0.9, fs * 4.0, fs * 0.22, fs * 0.11, title_c);
    text_cb(title, sw / 2.0, y + fs * 2.6, fs * 1.6, title_c);
    let mut yy = y + fs * 4.8;
    for l in &lines {
        text_c(l, sw / 2.0, yy, fs, INK);
        yy += fs * 1.45;
    }
    text_c(footer, sw / 2.0, y + h - fs * 1.4, fs * 0.82, DIM);
}

/// (keys, what they do). An empty key starts a new section titled by the description; a
/// key of "|" starts the second column.
pub const HELP: &[(&str, &str)] = &[
    ("", "FLYING"),
    ("W / S  or  Up / Down", "push / pull - tap to nudge, hold for more"),
    ("A / D  or  Left / Right", "roll left / right"),
    ("Q / E", "rudder - also steers on the ground"),
    ("Shift / Ctrl  or  = / -", "throttle up / down"),
    ("F / V", "flaps down / up"),
    ("T / G", "trim nose up / down - tap for fine steps"),
    ("B or Space", "brakes"),
    ("X", "speedbrake (737: down / armed / up), sailplane airbrakes"),
    ("L", "landing gear up / down"),
    ("Ctrl held at idle", "on the ground: thrust reversers (737)"),
    ("", "VIEW"),
    ("C", "camera: chase, cockpit, orbit, tower"),
    ("Mouse", "drag to look around, wheel to zoom"),
    ("U", "head-up display in the cockpit"),
    ("Tab   O", "instruments full / compact / off, coach less / more"),
    ("Space", "in a dialog or paused: instant replay"),
    ("", "AUTOPILOT"),
    ("Z   K", "autopilot on / off, autothrottle (jets)"),
    ("1  2  3  4", "heading, altitude hold, vertical speed, approach"),
    (",  .    ;  '", "heading bug, altitude bug"),
    ("9  0    7  8", "vertical speed, speed"),
    ("Mouse on the panel", "click a window to press it, scroll to set it"),
    ("", "OTHER"),
    ("Y   I   M", "auto-rudder, invert pitch, mute"),
    ("R   Esc   H", "restart, pause, help"),
    ("|", ""),
    ("", "GAMEPAD"),
    ("Left stick", "pitch & roll"),
    ("Right stick", "rudder, throttle (hold down at idle: reverse)"),
    ("LT / RT", "left / right brakes (A: both)"),
    ("LB / RB", "flaps up / down"),
    ("D-pad", "trim, instruments (left), gear (right)"),
    ("B   X   Y", "speedbrake, auto-rudder, camera"),
    ("Start   Select", "pause, help"),
    ("Stick clicks", "autopilot on / off (left), approach mode (right)"),
    ("Right stick click", "in a dialog: replay"),
    ("", "FLYING BASICS"),
    ("Pitch", "sets airspeed - power sets climb or descent"),
    ("Trim", "sets the hands-off speed (TRIM .. KT)"),
    ("Stall", "at a critical angle of attack, at any speed"),
    ("Turns", "bank, plus a little back pressure"),
    ("Ball", "keep it centred with rudder"),
    ("Landing", "idle power, then flare just above the runway"),
    ("Crosswind", "crab into the wind, straighten with rudder at touchdown"),
    ("HUD", "put the flight path circle where you want to go"),
    ("", "SPEEDS"),
    ("Trainer", "rotate 55 kt, climb 75, approach 65-70"),
    ("737 MAX 8", "rotate 145, approach 145 with flaps 30"),
    ("F-16", "rotate 150, approach 145 at 11 deg nose-up"),
    ("Sailplane", "best glide 50, min sink 42"),
];

pub fn help_overlay() {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
    let split = HELP.iter().position(|(k, _)| *k == "|").unwrap_or(HELP.len());
    let cols = [&HELP[..split], &HELP[(split + 1).min(HELP.len())..]];
    let rows = cols.iter().map(|c| c.len()).max().unwrap_or(1) as f32 + 3.0;
    let fs = (sh / rows * 0.62).clamp(10.0, 15.0 * u);
    let lh = fs * 1.4;
    let col_w = |c: &[(&str, &str)]| {
        let key_w = c.iter().map(|(k, _)| measure(k, fs, true).width).fold(0.0, f32::max) + fs * 1.2;
        (key_w, key_w + c.iter().map(|(_, d)| text_w(d, fs)).fold(0.0, f32::max))
    };
    let widths = [col_w(cols[0]), col_w(cols[1])];
    let w = (widths[0].1 + widths[1].1 + fs * 5.0).min(sw - 20.0);
    let h = rows * lh + fs * 1.5;
    let (x, y) = ((sw - w) / 2.0, (sh - h) / 2.0);
    let r = 14.0 * u;
    rrect(x, y, w, h, r, Color::new(0.04, 0.055, 0.08, 0.95));
    rrect_lines(x, y, w, h, r, 1.0, EDGE);
    let mut cx = x + fs * 1.5;
    for (c, (key_w, cw)) in cols.iter().zip(widths) {
        let mut yy = y + fs * 1.4;
        for (k, d) in c.iter() {
            if k.is_empty() && d.chars().all(|ch| ch.is_ascii_uppercase() || ch == ' ' || ch == '(' || ch == ')') {
                yy += lh * 0.4;
                text_b(d, cx, yy + fs * 0.4, fs * 0.9, AMBER);
                draw_line(cx, yy + fs * 0.75, cx + cw, yy + fs * 0.75, 1.0, EDGE);
            } else {
                text_b(k, cx, yy + fs * 0.4, fs, INK);
                text(d, cx + key_w, yy + fs * 0.4, fs, DIM);
            }
            yy += lh;
        }
        cx += cw + fs * 2.0;
    }
    text_c("H / Select to close", sw / 2.0, y + h - fs * 1.0, fs * 0.85, DIM);
}

/// A row of the main menu.
pub enum MenuRow {
    Header(&'static str),
    /// Name, a note on the right (best time, done), and the blurb shown when selected.
    Item(String, String, String),
}

fn menu_frame(time: f32) -> (f32, f32, f32, f32, f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    // a soft darkening on the left, where the menu sits
    let shade = Color::new(0.0, 0.03, 0.08, 0.45);
    hgradient(0.0, 0.0, sw * 0.65, sh, shade, Color::new(0.0, 0.03, 0.08, 0.0));
    let fs = (sh / 46.0).clamp(13.0, 18.0 * u);
    let title = (sh / 10.0).clamp(40.0, 88.0 * u);
    let w = (sw * 0.36).clamp(340.0, 480.0 * u);
    let x = (sw * 0.06).min(sw - w - 10.0).max(10.0);
    let ty = sh * 0.12;
    let bob = (time * 1.3).sin() * 3.0 * u;
    text_b("SKYHOP", x + 3.0 * u, ty + 3.0 * u + bob, title, Color::new(0.0, 0.0, 0.0, 0.35));
    text_b("SKYHOP", x, ty + bob, title, WHITE);
    text("learn to fly - one hop at a time", x + title * 0.04, ty + title * 0.55, fs * 1.05, Color::new(1.0, 0.94, 0.8, 0.95));
    (x, ty + title * 0.85, w, fs, u)
}

pub fn menu(rows: &[MenuRow], sel: usize, time: f32, pad: bool) {
    let sh = screen_height();
    let (x, y, w, fs, u) = menu_frame(time);
    let row = fs * 1.85;
    let head = fs * 1.6;
    let height = |r: &MenuRow| if matches!(r, MenuRow::Header(_)) { head } else { row };
    let list_h: f32 = rows.iter().map(height).sum();
    let h = (list_h + fs * 5.0).min(sh - y - fs);
    card(x, y, w, h);
    // scroll so the selected row stays in view when the list is taller than the card
    let view = h - fs * 5.0;
    let sel_y: f32 = rows.iter().take(sel).map(height).sum();
    let scroll = if list_h > view { (sel_y - view * 0.5 + row).clamp(0.0, list_h - view) } else { 0.0 };
    let mut yy = y + fs * 0.6 - scroll;
    for (i, r) in rows.iter().enumerate() {
        if yy < y + fs * 0.2 || yy + height(r) > y + fs * 0.6 + view + 1.0 {
            yy += height(r);
            continue;
        }
        match r {
            MenuRow::Header(t) => {
                text_b(t, x + fs * 1.4, yy + head * 0.75, fs * 0.72, DIM);
                yy += head;
            }
            MenuRow::Item(name, mark, _) => {
                let selected = i == sel;
                if selected {
                    rrect(x + fs * 0.5, yy, w - fs, row * 0.9, fs * 0.45, Color::new(1.0, 0.77, 0.26, 0.18));
                    rrect(x + fs * 0.5, yy + row * 0.2, 3.0 * u, row * 0.5, 1.5 * u, AMBER);
                }
                let c = if selected { AMBER } else { INK };
                draw_str(name, x + fs * 1.4, yy + row * 0.45 + fs * 0.36, fs, c, selected);
                if !mark.is_empty() {
                    let mw = text_w(mark, fs * 0.85);
                    text(mark, x + w - mw - fs * 1.3, yy + row * 0.45 + fs * 0.3, fs * 0.85, GREEN_C);
                }
                yy += row;
            }
        }
    }
    let blurb = match rows.get(sel) {
        Some(MenuRow::Item(_, _, b)) => b.as_str(),
        _ => "",
    };
    let by = y + h - fs * 3.2;
    draw_line(x + fs, by - fs * 1.1, x + w - fs, by - fs * 1.1, 1.0, EDGE);
    for (k, l) in wrap(blurb, w - fs * 2.5, fs * 0.9).iter().take(2).enumerate() {
        text(l, x + fs * 1.4, by + k as f32 * fs * 1.2, fs * 0.9, DIM);
    }
    let hint = if pad { "D-pad  choose      A  fly      Select  controls" } else { "Up / Down  choose      Enter  fly      H  controls      Esc  quit" };
    let hs = (fs * 0.78).min(fs * 0.78 * (w - fs * 2.8) / text_w(hint, fs * 0.78));
    text(hint, x + fs * 1.4, y + h - fs * 0.9, hs, Color::new(DIM.r, DIM.g, DIM.b, 0.8));
}

/// The free-flight setup: rows of label / value, the last one launches.
pub fn setup_screen(rows: &[(&'static str, String)], sel: usize, time: f32, pad: bool) {
    let (x, y, w, fs, u) = menu_frame(time);
    let row = fs * 2.0;
    let h = row * rows.len() as f32 + fs * 5.4;
    card(x, y, w, h);
    text_b("FREE FLIGHT", x + fs * 1.4, y + fs * 1.6, fs * 0.8, DIM);
    let mut yy = y + fs * 2.4;
    for (i, (label, value)) in rows.iter().enumerate() {
        let selected = i == sel;
        if selected {
            rrect(x + fs * 0.5, yy, w - fs, row * 0.9, fs * 0.45, Color::new(1.0, 0.77, 0.26, 0.18));
        }
        let mid = yy + row * 0.45 + fs * 0.36;
        if label.is_empty() {
            draw_str(value, x + w / 2.0 - measure(value, fs * 1.1, true).width / 2.0, mid, fs * 1.1, if selected { AMBER } else { GREEN_C }, true);
        } else {
            text(label, x + fs * 1.4, mid, fs * 0.9, DIM);
            let vx = x + w * 0.36;
            let c = if selected { AMBER } else { INK };
            if selected {
                text_b("<", vx - fs * 1.0, mid, fs, c);
                text_b(">", x + w - fs * 1.6, mid, fs, c);
            }
            draw_str(value, vx, mid, fs, c, selected);
        }
        yy += row;
    }
    let _ = u;
    let hint = if pad { "D-pad  choose / change      A  fly      B  back" } else { "Up / Down  choose      Left / Right  change      Enter  fly      Esc  back" };
    let hs = (fs * 0.78).min(fs * 0.78 * (w - fs * 2.8) / text_w(hint, fs * 0.78));
    text(hint, x + fs * 1.4, y + h - fs * 0.9, hs, Color::new(DIM.r, DIM.g, DIM.b, 0.8));
}

/// Wind readout under the minimap: an arrow showing where the wind blows relative to the
/// nose, "030/20G26", and the headwind and crosswind parts.
pub fn wind_box(w: &crate::weather::Weather, hdg: f32) {
    let sw = screen_width();
    let (_, my, s) = map_rect();
    let u = ui();
    let fs = 12.0 * u;
    let (x, y) = (sw - s - 14.0 * u - 5.0 * u, my + s + 34.0 * u);
    let wbox = s + 10.0 * u;
    card(x, y, wbox, fs * 3.4);
    let cx = x + fs * 1.8;
    let cy = y + fs * 1.7;
    let r = fs * 1.15;
    draw_circle_lines(cx, cy, r, 1.0, EDGE);
    if w.wind_kt < 0.5 {
        text_cb("CALM", x + wbox / 2.0 + fs, cy, fs, INK);
        return;
    }
    // the arrow points the way the air moves, drawn relative to the aircraft's nose (up)
    let rel = (w.wind_from + 180.0 - hdg).to_radians();
    let d = vec2(rel.sin(), -rel.cos());
    let tail = vec2(cx, cy) - d * r * 0.85;
    let tip = vec2(cx, cy) + d * r * 0.85;
    draw_line(tail.x, tail.y, tip.x, tip.y, 2.0 * u, AMBER);
    let perp = vec2(-d.y, d.x);
    draw_triangle(tip + d * 2.0, tip - d * fs * 0.5 + perp * fs * 0.35, tip - d * fs * 0.5 - perp * fs * 0.35, AMBER);
    let gust = if w.gust_kt > 0.5 { format!("G{:.0}", w.wind_kt + w.gust_kt) } else { String::new() };
    text_b(&format!("{:03.0}/{:.0}{gust}", w.wind_from, w.wind_kt), cx + r + fs * 0.8, y + fs * 1.45, fs, INK);
    let a = (w.wind_from - hdg).to_radians();
    let (head, cross) = (w.wind_kt * a.cos(), w.wind_kt * a.sin());
    let hc = format!("{} {:.0}   X {:.0} {}", if head >= 0.0 { "HEAD" } else { "TAIL" }, head.abs(), cross.abs(), if cross >= 0.0 { "R" } else { "L" });
    text(&hc, cx + r + fs * 0.8, y + fs * 2.75, fs * 0.82, DIM);
}

/// "REPLAY" with the speed, a progress bar and the keys.
pub fn replay_badge(speed: f32, progress: f32, cam: &str, pad: bool) {
    let sw = screen_width();
    let u = ui();
    let fs = 15.0 * u;
    let keys = if pad { "D-pad  speed    Y  camera    B  back" } else { "Left / Right  speed    C  camera    Esc  back" };
    let w = fs * 10.0 + text_w(&format!("{cam}   |   {keys}"), fs * 0.8);
    let (x, y) = (sw / 2.0 - w / 2.0, 14.0 * u);
    card(x, y, w, fs * 3.6);
    let sp = if speed >= 1.0 { format!("{speed:.0}x") } else { format!("1/{:.0}x", 1.0 / speed) };
    text_b("REPLAY", x + fs, y + fs * 1.5, fs * 1.1, RED_C);
    text_b(&sp, x + fs * 6.2, y + fs * 1.5, fs * 1.1, INK);
    text(&format!("{cam}   |   {keys}"), x + fs * 9.0, y + fs * 1.45, fs * 0.8, DIM);
    rrect(x + fs, y + fs * 2.4, w - fs * 2.0, fs * 0.35, fs * 0.17, Color::new(1.0, 1.0, 1.0, 0.15));
    rrect(x + fs, y + fs * 2.4, ((w - fs * 2.0) * progress).max(fs * 0.35), fs * 0.35, fs * 0.17, RED_C);
}

/// Small status text tucked under the minimap.
pub fn status_line(cam: &str, fps: i32, auto_rudder: bool) {
    let sw = screen_width();
    let (_, my, s) = map_rect();
    let u = ui();
    let ar = if auto_rudder { "   |   auto-rudder" } else { "" };
    let st = format!("{cam} view (C)   |   {fps} fps{ar}");
    let fs = 11.5 * u;
    text(&st, sw - 14.0 * u - text_w(&st, fs), my + s + 34.0 * u + 12.0 * u * 3.4 + 18.0 * u, fs, Color::new(1.0, 1.0, 1.0, 0.75));
}

pub fn cloud_fade(density: f32) {
    if density > 0.0 {
        let a = (density * 2.5).min(0.92);
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.93, 0.94, 0.96, a));
    }
}

// ------------------------------------------------------------- head-up ---

/// What the head-up display draws, in world terms.
pub struct HeadUp {
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// Where the nose points, and where the aircraft is actually going.
    pub nose: Vec3,
    pub velocity: Vec3,
    pub hdg: f32,
    pub ias: f32,
    pub alt_ft: f32,
    pub vs_fpm: f32,
    pub ra_ft: Option<f32>,
    pub ils: Option<(f32, f32)>,
    pub speed_bug: Option<f32>,
    pub on_ground: bool,
}

const HUD_C: Color = Color::new(0.4, 1.0, 0.5, 0.92);

/// Screen position of a world direction seen from the eye, if it's in front.
fn project(vp: &Mat4, eye: Vec3, dir: Vec3) -> Option<Vec2> {
    let c = *vp * (eye + dir * 1000.0).extend(1.0);
    (c.w > 0.1).then(|| vec2((c.x / c.w * 0.5 + 0.5) * screen_width(), (0.5 - c.y / c.w * 0.5) * screen_height()))
}

/// Direction for a compass heading and elevation, degrees.
fn dir_of(hdg: f32, el: f32) -> Vec3 {
    let (h, e) = (hdg.to_radians(), el.to_radians());
    vec3(h.sin() * e.cos(), e.sin(), -h.cos() * e.cos())
}

/// A conformal head-up display: horizon, pitch ladder with the 3 degree glide reference,
/// the flight path vector (put it on the runway aim point to land there), speed and
/// altitude tapes, and ILS deviation.
pub fn head_up(h: &HeadUp) {
    let (sw, sh) = (screen_width(), screen_height());
    let u = ui();
    let lw = 1.6 * u;
    let fs = 13.0 * u;
    let p = |d: Vec3| project(&h.view_proj, h.eye, d);
    let (Some(c0), Some(c5)) = (p(dir_of(h.hdg, 0.0)), p(dir_of(h.hdg, 5.0))) else { return };
    // the horizon frame: centre, "up" per degree, and along-horizon direction
    let up5 = c5 - c0;
    let deg = up5.length() / 5.0;
    let upn = up5 / up5.length().max(1e-3);
    let along = vec2(-upn.y, upn.x);
    let seg = |a: Vec2, b: Vec2, c: Color| draw_line(a.x, a.y, b.x, b.y, lw, c);
    // horizon with heading marks
    let half = sw * 0.42;
    seg(c0 - along * half, c0 - along * deg * 2.5, HUD_C);
    seg(c0 + along * deg * 2.5, c0 + along * half, HUD_C);
    for k in -6..=6 {
        let hd = ((h.hdg / 5.0).round() * 5.0 + k as f32 * 5.0).rem_euclid(360.0);
        let off = ((hd - h.hdg + 540.0).rem_euclid(360.0) - 180.0) * deg;
        let at = c0 + along * off;
        let big = (hd as i32) % 10 == 0;
        seg(at, at - upn * deg * if big { 0.6 } else { 0.3 }, HUD_C);
        if big {
            let t = at + upn * (fs * 0.8);
            text_c(&format!("{:02}", (hd / 10.0).round() as i32 % 36), t.x, t.y, fs * 0.8, HUD_C);
        }
    }
    // pitch ladder
    for e in (-15..=20).step_by(5).filter(|e| *e != 0) {
        let e = e as f32;
        let at = c0 + upn * deg * e;
        let w = deg * 5.0;
        let gap = deg * 1.6;
        for s in [-1.0f32, 1.0] {
            let (a, b) = (at + along * s * gap, at + along * s * (gap + w));
            if e > 0.0 {
                seg(a, b, HUD_C);
            } else {
                // dashed below the horizon
                for k in 0..4 {
                    let f0 = k as f32 / 4.0;
                    seg(a + (b - a) * f0, a + (b - a) * (f0 + 0.15), HUD_C);
                }
            }
            seg(b, b - upn * deg * 0.6 * e.signum(), HUD_C);
        }
        let t = at + along * (gap + w + fs * 1.2);
        text_c(&format!("{}", e.abs() as i32), t.x, t.y, fs * 0.8, HUD_C);
    }
    // the -3 degree line: the glide path to aim the flight path vector along
    let g = c0 - upn * deg * 3.0;
    for k in 0..10 {
        let f = (k as f32 - 5.0) * deg * 1.6;
        seg(g + along * f, g + along * (f + deg * 0.7), Color::new(0.4, 1.0, 0.5, 0.6));
    }
    // waterline: where the nose points
    if let Some(n) = p(h.nose) {
        let w = 9.0 * u;
        seg(n - along * w * 2.0, n - along * w, HUD_C);
        seg(n - along * w, n - along * w * 0.5 + upn * -w * 0.6, HUD_C);
        seg(n - along * w * 0.5 + upn * -w * 0.6, n + upn * 0.0, HUD_C);
        seg(n, n + along * w * 0.5 - upn * w * 0.6, HUD_C);
        seg(n + along * w * 0.5 - upn * w * 0.6, n + along * w, HUD_C);
        seg(n + along * w, n + along * w * 2.0, HUD_C);
    }
    // flight path vector
    if h.velocity.length() > 5.0 {
        if let Some(v) = p(h.velocity.normalize()) {
            let r = 7.0 * u;
            draw_circle_lines(v.x, v.y, r, lw, HUD_C);
            seg(v - along * r, v - along * r * 2.6, HUD_C);
            seg(v + along * r, v + along * r * 2.6, HUD_C);
            seg(v + upn * r, v + upn * r * 1.9, HUD_C);
        }
    }
    // speed tape on the left, altitude on the right
    let tape_h = sh * 0.34;
    let (ty, lx, rx) = (sh * 0.42, sw * 0.24, sw * 0.76);
    let tape = |x: f32, value: f32, step: f32, label_every: i32, px_per: f32, left: bool, bug: Option<f32>| {
        let dir = if left { -1.0 } else { 1.0 };
        draw_line(x, ty - tape_h / 2.0, x, ty + tape_h / 2.0, lw, HUD_C);
        let first = ((value - tape_h / 2.0 / px_per) / step).floor() as i32;
        let last = ((value + tape_h / 2.0 / px_per) / step).ceil() as i32;
        for k in first..=last {
            let v = k as f32 * step;
            let y = ty - (v - value) * px_per;
            if (y - ty).abs() > tape_h / 2.0 || v < 0.0 {
                continue;
            }
            let big = k % label_every == 0;
            draw_line(x, y, x + dir * if big { 12.0 } else { 6.0 } * u, y, lw, HUD_C);
            if big && (y - ty).abs() > fs * 1.2 {
                let s = format!("{}", v as i32);
                let tx = if left { x - fs * 0.9 - text_w(&s, fs * 0.85) } else { x + fs * 1.1 };
                text(&s, tx, y + fs * 0.3, fs * 0.85, HUD_C);
            }
        }
        if let Some(b) = bug {
            let y = (ty - (b - value) * px_per).clamp(ty - tape_h / 2.0, ty + tape_h / 2.0);
            draw_triangle(vec2(x, y), vec2(x + dir * 9.0 * u, y - 5.0 * u), vec2(x + dir * 9.0 * u, y + 5.0 * u), Color::new(1.0, 0.4, 0.95, 0.95));
        }
    };
    tape(lx, h.ias, 5.0, 2, 3.2 * u, true, h.speed_bug);
    tape(rx, h.alt_ft, 50.0, 2, 0.45 * u, false, None);
    let boxed = |s: &str, x: f32, left: bool| {
        let w = text_w(s, fs * 1.15) + fs;
        let bx = if left { x - w - 4.0 * u } else { x + 4.0 * u };
        rrect(bx, ty - fs * 0.85, w, fs * 1.7, fs * 0.3, Color::new(0.0, 0.0, 0.0, 0.45));
        rrect_lines(bx, ty - fs * 0.85, w, fs * 1.7, fs * 0.3, lw, HUD_C);
        text_cb(s, bx + w / 2.0, ty, fs * 1.15, HUD_C);
    };
    boxed(&format!("{:.0}", h.ias), lx, true);
    boxed(&commas(h.alt_ft.round() as i32), rx, false);
    text(&format!("{:+.0}", (h.vs_fpm / 10.0).round() * 10.0), rx + 6.0 * u, ty + tape_h / 2.0 + fs * 1.3, fs * 0.9, HUD_C);
    if let Some(ra) = h.ra_ft.filter(|r| *r < 2500.0 && !h.on_ground) {
        text_cb(&format!("{:.0} RA", ra), rx - fs * 2.5, ty + tape_h / 2.0 + fs * 1.3, fs, if ra < 100.0 { AMBER } else { HUD_C });
    }
    // ILS: glideslope beside the altitude tape, localizer under the centre
    if let Some((loc, gs)) = h.ils.filter(|_| !h.on_ground) {
        let step = 16.0 * u;
        let gx = rx - fs * 1.4;
        let ly = ty + tape_h / 2.0 - fs * 0.5;
        for k in [-2.0f32, -1.0, 1.0, 2.0] {
            draw_circle_lines(gx, ty + k * step, 2.5 * u, 1.2, HUD_C);
            draw_circle_lines(sw / 2.0 + k * step, ly, 2.5 * u, 1.2, HUD_C);
        }
        let dm = |c: Vec2| {
            let d = 6.0 * u;
            draw_triangle(c + vec2(0.0, -d), c + vec2(d * 0.7, 0.0), c + vec2(-d * 0.7, 0.0), HUD_C);
            draw_triangle(c + vec2(0.0, d), c + vec2(d * 0.7, 0.0), c + vec2(-d * 0.7, 0.0), HUD_C);
        };
        dm(vec2(gx, ty - gs * step));
        dm(vec2(sw / 2.0 + loc * step, ly));
    }
}
