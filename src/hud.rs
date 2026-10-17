//! 2D overlay: the six-pack, engine/flap/trim cluster, lesson coach and screens.

use crate::flight::{Telem, VNE_KT};
use crate::lessons::{fmt_time, glide_angle, hdg_err, papi_lights, Kind, Lesson, Mission};
use crate::world::{World, HALF, RWY_HALF_LEN};
use macroquad::prelude::*;

pub const INK: Color = Color::new(0.94, 0.94, 0.91, 1.0);
pub const DIM: Color = Color::new(0.62, 0.65, 0.68, 1.0);
pub const AMBER: Color = Color::new(1.0, 0.76, 0.22, 1.0);
pub const RED_C: Color = Color::new(0.96, 0.28, 0.22, 1.0);
pub const GREEN_C: Color = Color::new(0.33, 0.86, 0.38, 1.0);
pub const PANEL: Color = Color::new(0.03, 0.04, 0.06, 0.74);
const BEZEL: Color = Color::new(0.06, 0.06, 0.07, 0.96);
const FACE: Color = Color::new(0.12, 0.12, 0.13, 1.0);
const SKY_C: Color = Color::new(0.24, 0.52, 0.86, 1.0);
const EARTH_C: Color = Color::new(0.55, 0.36, 0.2, 1.0);
const ORANGE: Color = Color::new(1.0, 0.6, 0.1, 1.0);

// ------------------------------------------------------------ helpers ---

fn polar(cx: f32, cy: f32, r: f32, deg: f32) -> Vec2 {
    let a = deg.to_radians();
    vec2(cx + r * a.sin(), cy - r * a.cos())
}

pub fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_text(s, x.round(), y.round(), size, c);
}

pub fn text_w(s: &str, size: f32) -> f32 {
    measure_text(s, None, size as u16, 1.0).width
}

/// Text centred on (cx, cy).
pub fn text_c(s: &str, cx: f32, cy: f32, size: f32, c: Color) {
    let d = measure_text(s, None, size as u16, 1.0);
    draw_text(s, (cx - d.width / 2.0).round(), (cy - d.height / 2.0 + d.offset_y).round(), size, c);
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
    draw_circle(cx, cy, r * 0.93, FACE);
    if !label.is_empty() {
        text_c(label, cx, cy + r * 0.38, (r * 0.17).max(9.0), DIM);
    }
}

fn readout(s: &str, cx: f32, cy: f32, r: f32, c: Color) {
    let fs = (r * 0.25).max(11.0);
    let w = text_w(s, fs) + 8.0;
    draw_rectangle(cx - w / 2.0, cy - fs * 0.55, w, fs * 1.1, Color::new(0.0, 0.0, 0.0, 0.85));
    text_c(s, cx, cy, fs, c);
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

fn asi(cx: f32, cy: f32, r: f32, kt: f32) {
    bezel(cx, cy, r, "KNOTS");
    let a = |v: f32| v.clamp(0.0, 190.0) / 190.0 * 330.0;
    let rr = r * 0.9;
    band(cx, cy, rr * 0.74, rr * 0.82, a(33.0), a(85.0), INK);
    band(cx, cy, rr * 0.82, rr * 0.92, a(44.0), a(129.0), GREEN_C);
    band(cx, cy, rr * 0.82, rr * 0.92, a(129.0), a(VNE_KT), AMBER);
    tick(cx, cy, rr * 0.74, rr, a(VNE_KT), 3.0, RED_C);
    let fs = (r * 0.17).max(9.0);
    for v in (0..=180).step_by(10) {
        let v = v as f32;
        let long = v as i32 % 20 == 0;
        tick(cx, cy, rr * if long { 0.82 } else { 0.88 }, rr, a(v), 1.5, INK);
        if long && v >= 40.0 {
            let p = polar(cx, cy, rr * 0.6, a(v));
            text_c(&format!("{}", v as i32), p.x, p.y, fs, INK);
        }
    }
    readout(&format!("{:.0}", kt), cx, cy - r * 0.32, r, INK);
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

fn attitude(cx: f32, cy: f32, r: f32, pitch: f32, bank: f32) {
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
}

fn altimeter(cx: f32, cy: f32, r: f32, ft: f32) {
    bezel(cx, cy, r, "ALT FT");
    let rr = r * 0.9;
    let fs = (r * 0.19).max(9.0);
    for i in 0..50 {
        let long = i % 5 == 0;
        tick(cx, cy, rr * if long { 0.82 } else { 0.9 }, rr, i as f32 * 7.2, if long { 2.0 } else { 1.0 }, INK);
        if long {
            let p = polar(cx, cy, rr * 0.66, i as f32 * 7.2);
            text_c(&format!("{}", i / 5), p.x, p.y, fs, INK);
        }
    }
    readout(&commas(ft.round() as i32), cx, cy + r * 0.3, r, INK);
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
    let fs = (r * 0.19).max(9.0);
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
    let fs = (r * 0.18).max(9.0);
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
    text_c("FPM x100", cx + r * 0.2, cy, (r * 0.13).max(8.0), DIM);
    needle(cx, cy, rr * 0.92, a(fpm), r * 0.045, INK);
    let s = if fpm >= 0.0 { format!("+{}", commas((fpm / 10.0).round() as i32 * 10)) } else { commas((fpm / 10.0).round() as i32 * 10) };
    readout(&s, cx + r * 0.25, cy + r * 0.6, r * 0.75, INK);
}

fn vbar(x: f32, y: f32, w: f32, h: f32, frac: f32, c: Color, label: &str, value: &str, fs: f32) {
    draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
    let f = frac.clamp(0.0, 1.0);
    draw_rectangle(x + 2.0, y + 2.0 + (h - 4.0) * (1.0 - f), w - 4.0, (h - 4.0) * f, c);
    draw_rectangle_lines(x, y, w, h, 1.0, DIM);
    text_c(label, x + w / 2.0, y + h + fs * 0.75, fs * 0.85, DIM);
    text_c(value, x + w / 2.0, y - fs * 0.6, fs * 0.9, INK);
}

/// Throttle, flaps, trim, angle of attack and a few status lights.
fn cluster(x: f32, y: f32, h: f32, t: &Telem, brake: bool, auto_rudder: bool) -> f32 {
    let fs = (h * 0.12).clamp(10.0, 18.0);
    let w = (h * 0.16).max(14.0);
    let gap = w * 0.95;
    let top = y + fs * 1.2;
    let bh = h - fs * 2.6;
    let mut cx = x;
    vbar(cx, top, w, bh, t.throttle, Color::new(0.9, 0.9, 0.9, 0.9), "THR", &format!("{:.0}%", t.throttle * 100.0), fs);
    cx += w + gap;
    // flaps: notches 0..30 top to bottom
    draw_rectangle(cx, top, w, bh, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(cx, top, w, bh, 1.0, DIM);
    for n in 0..4 {
        let yy = top + bh * (0.1 + 0.8 * n as f32 / 3.0);
        draw_line(cx, yy, cx + w * 0.3, yy, 1.0, DIM);
    }
    let fy = top + bh * (0.1 + 0.8 * t.flap_deg / 30.0);
    draw_rectangle(cx + 2.0, fy - 3.0, w - 4.0, 6.0, INK);
    text_c("FLAPS", cx + w / 2.0, top + bh + fs * 0.75, fs * 0.85, DIM);
    text_c(&format!("{}", t.flaps as u32 * 10), cx + w / 2.0, top - fs * 0.6, fs * 0.9, INK);
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
    let lines: [(String, Color); 4] = [
        (format!("RPM {:.0}", t.rpm), INK),
        (format!("G {:.1}", t.g), if t.g > 3.5 || t.g < -1.0 { AMBER } else { INK }),
        ("BRAKE".into(), if brake { RED_C } else { Color::new(0.3, 0.3, 0.3, 1.0) }),
        ("AUTO-RUD".into(), if auto_rudder { GREEN_C } else { Color::new(0.3, 0.3, 0.3, 1.0) }),
    ];
    for (i, (s, c)) in lines.iter().enumerate() {
        text(s, cx, top + fs * (0.8 + i as f32 * 1.35), fs, *c);
    }
    cx + text_w("AUTO-RUD", fs) - x
}

pub struct PanelInfo {
    pub brake: bool,
    pub auto_rudder: bool,
}

/// Instrument panel along the bottom of the screen. Returns its top edge.
pub fn panel(t: &Telem, info: &PanelInfo, full: bool) -> f32 {
    let (sw, sh) = (screen_width(), screen_height());
    if !full {
        let fs = (sh / 42.0).clamp(13.0, 20.0);
        let s = format!(
            "IAS {:.0} kt   ALT {} ft   VS {:+.0} fpm   HDG {:03.0}   THR {:.0}%   FLAPS {}   TRIM {:+.2}",
            t.ias_kt, commas(t.alt_ft.round() as i32), t.vs_fpm, t.hdg.round() % 360.0, t.throttle * 100.0, t.flaps as u32 * 10, t.trim
        );
        let w = text_w(&s, fs) + 24.0;
        let y = sh - fs * 2.0;
        draw_rectangle((sw - w) / 2.0, y - fs * 0.4, w, fs * 1.8, PANEL);
        text_c(&s, sw / 2.0, y + fs * 0.5, fs, INK);
        return y - fs * 0.4;
    }
    let r = (sh * 0.085).min(sw * 0.043).max(36.0);
    let gap = r * 0.18;
    let cluster_w = r * 3.3;
    let total = 6.0 * 2.0 * r + 5.0 * gap + gap * 2.0 + cluster_w;
    let x0 = ((sw - total) / 2.0).max(4.0);
    let cy = sh - r - gap * 1.5;
    let top = cy - r - gap;
    draw_rectangle(x0 - gap, top, total + gap * 2.0, sh - top, PANEL);
    let mut cx = x0 + r;
    asi(cx, cy, r, t.ias_kt);
    cx += 2.0 * r + gap;
    attitude(cx, cy, r, t.pitch, t.bank);
    cx += 2.0 * r + gap;
    altimeter(cx, cy, r, t.alt_ft);
    cx += 2.0 * r + gap;
    turn_coord(cx, cy, r, t.turn_rate, t.slip);
    cx += 2.0 * r + gap;
    heading_ind(cx, cy, r, t.hdg);
    cx += 2.0 * r + gap;
    vsi(cx, cy, r, t.vs_fpm);
    cluster(cx + r + gap * 2.0, cy - r, 2.0 * r, t, info.brake, info.auto_rudder);
    top
}

// ------------------------------------------------------------- overlays ---

pub fn minimap(tex: &Texture2D, world: &World, t: &Telem, next_ring: Option<usize>) {
    let (sw, sh) = (screen_width(), screen_height());
    let s = (sw * 0.15).min(sh * 0.24).clamp(120.0, 230.0);
    let (x, y) = (sw - s - 12.0, 12.0);
    draw_rectangle(x - 3.0, y - 3.0, s + 6.0, s + 6.0, PANEL);
    draw_texture_ex(tex, x, y, WHITE, DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() });
    let map = |p: Vec3| vec2(x + (p.x + HALF) / (2.0 * HALF) * s, y + (p.z + HALF) / (2.0 * HALF) * s);
    let (a, b) = (map(vec3(-RWY_HALF_LEN, 0.0, 0.0)), map(vec3(RWY_HALF_LEN, 0.0, 0.0)));
    draw_line(a.x, a.y, b.x, b.y, 3.0, Color::new(0.15, 0.15, 0.15, 1.0));
    if let Some(n) = next_ring {
        for (i, r) in world.rings.iter().enumerate().skip(n) {
            let p = map(r.pos);
            draw_circle(p.x, p.y, if i == n { 4.5 } else { 2.5 }, if i == n { AMBER } else { Color::new(0.4, 0.75, 1.0, 1.0) });
        }
    }
    let p = map(t.pos);
    let h = t.hdg;
    let tip = p + vec2(h.to_radians().sin(), -h.to_radians().cos()) * 9.0;
    let l = p + vec2((h - 140.0).to_radians().sin(), -(h - 140.0).to_radians().cos()) * 6.0;
    let r = p + vec2((h + 140.0).to_radians().sin(), -(h + 140.0).to_radians().cos()) * 6.0;
    draw_triangle(tip, l, r, Color::new(1.0, 0.95, 0.2, 1.0));
    draw_triangle_lines(tip, l, r, 1.0, BLACK);
    text_c("N", x + s / 2.0, y + 10.0, 16.0, INK);
}

/// Marker for the next ring: a box on screen, or an arrow at the edge.
pub fn ring_marker(vp: &Mat4, pos: Vec3, eye: Vec3, top_limit: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    let c = *vp * pos.extend(1.0);
    let dist = (pos - eye).length();
    let label = if dist > 1000.0 { format!("{:.1} km", dist / 1000.0) } else { format!("{:.0} m", dist) };
    let center = vec2(sw / 2.0, sh / 2.0);
    if c.w > 0.1 {
        let p = vec2((c.x / c.w * 0.5 + 0.5) * sw, (0.5 - c.y / c.w * 0.5) * sh);
        if p.x > 30.0 && p.x < sw - 30.0 && p.y > 30.0 && p.y < top_limit - 10.0 {
            let s = 14.0;
            for (a, b) in [(vec2(0.0, -s), vec2(s, 0.0)), (vec2(s, 0.0), vec2(0.0, s)), (vec2(0.0, s), vec2(-s, 0.0)), (vec2(-s, 0.0), vec2(0.0, -s))] {
                draw_line(p.x + a.x, p.y + a.y, p.x + b.x, p.y + b.y, 2.5, AMBER);
            }
            text_c(&label, p.x, p.y + s + 12.0, 16.0, AMBER);
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
    let mid = vec2(center.x, (top_limit) * 0.5);
    let p = mid + dir * k;
    let perp = vec2(-dir.y, dir.x);
    draw_triangle(p + dir * 16.0, p + perp * 10.0, p - perp * 10.0, AMBER);
    text_c(&label, p.x - dir.x * 30.0, p.y - dir.y * 22.0, 16.0, AMBER);
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
    let s = 13.0;
    let x0 = sw / 2.0 - 2.0 * s * 2.2 + s * 1.1;
    draw_rectangle(x0 - s * 1.6, y - s * 1.5, s * 2.2 * 4.0 + s * 1.0, s * 4.4, PANEL);
    for i in 0..4 {
        // pilot sees the outermost light (lowest angle) on the left
        let (_, thr) = lights[3 - i];
        let white = a > thr;
        draw_circle(x0 + i as f32 * s * 2.2, y, s, if white { WHITE } else { Color::new(1.0, 0.15, 0.1, 1.0) });
    }
    text_c("PAPI", sw / 2.0, y + s * 2.1, 14.0, DIM);
}

pub fn warnings(t: &Telem, extra: Option<&str>, time: f32, y: f32) {
    let sw = screen_width();
    let fs = (screen_height() / 22.0).clamp(22.0, 44.0);
    let flash = (time * 4.0).fract() < 0.6;
    let mut msgs: Vec<(&str, Color)> = Vec::new();
    if t.stalled && flash {
        msgs.push(("STALL - NOSE DOWN!", RED_C));
    } else if t.stall_warn && !t.stalled {
        msgs.push(("STALL WARNING", AMBER));
    }
    if t.ias_kt > VNE_KT {
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
    for (i, (m, c)) in msgs.iter().enumerate() {
        let size = if i == 0 { fs } else { fs * 0.6 };
        let yy = y + i as f32 * fs * 1.1;
        let w = text_w(m, size) + 24.0;
        draw_rectangle(sw / 2.0 - w / 2.0, yy - size * 0.7, w, size * 1.4, Color::new(0.0, 0.0, 0.0, 0.55));
        text_c(m, sw / 2.0, yy, size, *c);
    }
}

pub fn lesson_panel(lesson: &Lesson, m: &Mission, t: &Telem) {
    let (sw, sh) = (screen_width(), screen_height());
    let fs = (sh / 46.0).clamp(13.0, 20.0);
    let w = (sw * 0.3).clamp(280.0, 460.0);
    let (x, y) = (12.0, 12.0);
    let pad = fs * 0.7;
    let inner = w - pad * 2.0;
    let mut lines: Vec<(String, Color, f32)> = Vec::new();
    lines.push((lesson.name.to_string(), AMBER, fs * 1.1));
    match lesson.kind {
        Kind::Lesson => {
            for (i, s) in lesson.steps.iter().enumerate() {
                let (mark, c) = if i < m.step || m.complete { ("[x]", GREEN_C) } else if i == m.step { ("[>]", INK) } else { ("[ ]", DIM) };
                lines.push((format!("{mark} {}", s.title), c, fs * 0.9));
            }
            if !m.complete {
                let s = &lesson.steps[m.step];
                lines.push((String::new(), INK, fs * 0.5));
                for l in wrap(s.text, inner, fs) {
                    lines.push((l, INK, fs));
                }
                if let Some(h) = (s.hint)(t) {
                    lines.push((String::new(), INK, fs * 0.4));
                    for l in wrap(h, inner, fs) {
                        lines.push((l, AMBER, fs));
                    }
                }
            }
        }
        Kind::Rings | Kind::Free => {
            for l in wrap(lesson.blurb, inner, fs) {
                lines.push((l, INK, fs));
            }
        }
    }
    let mut h = pad * 2.0 + lines.iter().map(|l| l.2 * 1.3).sum::<f32>();
    let show_bar = lesson.kind == Kind::Lesson && !m.complete && lesson.steps[m.step].hold > 0.0;
    if show_bar {
        h += fs * 1.2;
    }
    h += fs * 1.3;
    draw_rectangle(x, y, w, h, PANEL);
    let mut yy = y + pad;
    for (s, c, size) in &lines {
        yy += size * 1.3;
        text(s, x + pad, yy - size * 0.3, *size, *c);
    }
    if show_bar {
        let s = &lesson.steps[m.step];
        yy += fs * 0.5;
        let f = (m.hold / s.hold).clamp(0.0, 1.0);
        draw_rectangle(x + pad, yy, inner, fs * 0.5, Color::new(1.0, 1.0, 1.0, 0.15));
        draw_rectangle(x + pad, yy, inner * f, fs * 0.5, GREEN_C);
        yy += fs * 0.7;
    }
    let footer = if lesson.kind == Kind::Lesson { "N: skip step   R: restart   H: help" } else { "R: restart   H: help   Esc: pause" };
    text(footer, x + pad, yy + fs * 1.1, fs * 0.8, DIM);
}

pub fn rings_status(m: &Mission, total: usize, best: Option<f32>) {
    let sw = screen_width();
    let fs = 22.0;
    let s = format!(
        "RING {}/{}    {}    BEST {}",
        (m.next_ring + 1).min(total),
        total,
        fmt_time(m.time),
        best.map(fmt_time).unwrap_or_else(|| "--".into())
    );
    let w = text_w(&s, fs) + 30.0;
    draw_rectangle(sw / 2.0 - w / 2.0, 10.0, w, fs * 1.6, PANEL);
    text_c(&s, sw / 2.0, 10.0 + fs * 0.8, fs, INK);
}

pub fn toasts(list: &[(String, f32)]) {
    let (sw, sh) = (screen_width(), screen_height());
    for (i, (s, ttl)) in list.iter().enumerate() {
        let a = ttl.min(1.0);
        let fs = 26.0;
        let y = sh * 0.3 + i as f32 * fs * 1.6;
        let w = text_w(s, fs) + 30.0;
        draw_rectangle(sw / 2.0 - w / 2.0, y - fs * 0.75, w, fs * 1.5, Color::new(0.0, 0.0, 0.0, 0.6 * a));
        text_c(s, sw / 2.0, y, fs, Color::new(GREEN_C.r, GREEN_C.g, GREEN_C.b, a));
    }
}

/// Centered box with a title and lines of text.
pub fn dialog(title: &str, title_c: Color, body: &[String], footer: &str) {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let fs = (sh / 36.0).clamp(16.0, 24.0);
    let w = (sw * 0.55).clamp(360.0, 720.0);
    let inner = w - fs * 2.0;
    let mut lines = Vec::new();
    for b in body {
        if b.is_empty() {
            lines.push(String::new());
        }
        lines.extend(wrap(b, inner, fs));
    }
    let h = fs * 4.6 + lines.len() as f32 * fs * 1.35 + fs * 2.0;
    let (x, y) = ((sw - w) / 2.0, (sh - h) / 2.0);
    draw_rectangle(x, y, w, h, Color::new(0.05, 0.06, 0.09, 0.94));
    draw_rectangle_lines(x, y, w, h, 2.0, title_c);
    text_c(title, sw / 2.0, y + fs * 1.8, fs * 1.6, title_c);
    let mut yy = y + fs * 4.0;
    for l in &lines {
        text_c(l, sw / 2.0, yy, fs, INK);
        yy += fs * 1.35;
    }
    text_c(footer, sw / 2.0, y + h - fs * 1.2, fs * 0.85, DIM);
}

pub const HELP: &[&str] = &[
    "W / S  or  UP / DOWN     push (nose down) / pull (nose up)",
    "A / D  or  LEFT / RIGHT  roll left / right",
    "Q / E                    rudder - also steers on the ground",
    "SHIFT / CTRL  or  = / -  throttle up / down",
    "F / V                    flaps down / up (0-30 deg)",
    "T / G                    trim nose up / nose down",
    "B or SPACE               brakes",
    "C                        camera (mouse drag: look, wheel: zoom)",
    "Y                        auto-rudder on / off",
    "TAB                      full / compact instruments",
    "I  invert pitch keys     M  mute     R  restart     ESC  pause",
    "",
    "FLYING BASICS",
    "Pitch controls airspeed - power controls climb or descent.",
    "The wing stalls at a critical ANGLE OF ATTACK (about 16 deg), at any speed.",
    "Bank to turn, and add a little back pressure to hold altitude.",
    "Keep the ball centered: press the rudder on the side the ball moved.",
    "Trim takes away the stick pressure for the speed you are flying.",
    "Land on the main wheels: idle power, then flare just above the runway.",
    "",
    "SPEEDS (kt)  rotate 55 | climb 75 | cruise 100 | approach 65-70",
    "stall 48 clean / 40 full flaps | never exceed 163",
];

pub fn help_overlay() {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
    let fs = (sh / (HELP.len() as f32 + 6.0) * 0.72).clamp(12.0, 22.0);
    let w = HELP.iter().map(|l| text_w(l, fs)).fold(0.0, f32::max) + fs * 3.0;
    let h = (HELP.len() as f32 + 4.0) * fs * 1.3;
    let (x, y) = ((sw - w) / 2.0, (sh - h) / 2.0);
    draw_rectangle(x, y, w, h, Color::new(0.05, 0.06, 0.09, 0.95));
    draw_rectangle_lines(x, y, w, h, 2.0, AMBER);
    text("CONTROLS", x + fs * 1.5, y + fs * 2.0, fs * 1.2, AMBER);
    for (i, l) in HELP.iter().enumerate() {
        let c = if *l == "FLYING BASICS" { AMBER } else if l.starts_with("SPEEDS") || l.starts_with("stall") { GREEN_C } else { INK };
        text(l, x + fs * 1.5, y + fs * 2.0 + (i as f32 + 1.4) * fs * 1.3, fs, c);
    }
    text_c("H to close", sw / 2.0, y + h - fs * 0.8, fs * 0.85, DIM);
}

pub fn menu(lessons: &[Lesson], sel: usize, done: &[bool], best: Option<f32>, time: f32) {
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.02, 0.06, 0.25));
    let fs = (sh / 34.0).clamp(15.0, 26.0);
    let title = (sh / 9.0).clamp(40.0, 96.0);
    let bob = (time * 1.3).sin() * 4.0;
    let w = (sw * 0.42).clamp(380.0, 600.0);
    let row = fs * 1.7;
    let items = lessons.len() + 1;
    let h = row * items as f32 + fs * 5.0;
    let (x, y) = ((sw * 0.06).min(sw - w - 10.0).max(10.0), sh * 0.26);
    let mid = x + w / 2.0;
    draw_rectangle(x, y, w, h, Color::new(0.03, 0.05, 0.08, 0.82));
    for i in 0..items {
        let yy = y + fs + row * i as f32;
        let selected = i == sel;
        if selected {
            draw_rectangle(x + 6.0, yy - fs * 0.2, w - 12.0, row * 0.92, Color::new(1.0, 0.76, 0.22, 0.2));
        }
        let (name, mark) = if i < lessons.len() {
            let l = &lessons[i];
            let mark = if l.kind == Kind::Rings { best.map(|b| format!("best {}", fmt_time(b))).unwrap_or_default() } else if done[i] { "done".into() } else { String::new() };
            (l.name, mark)
        } else {
            ("Quit", String::new())
        };
        let c = if selected { AMBER } else { INK };
        text(name, x + fs * 1.2, yy + fs * 0.9, fs, c);
        if !mark.is_empty() {
            text(&mark, x + w - text_w(&mark, fs * 0.85) - fs, yy + fs * 0.9, fs * 0.85, GREEN_C);
        }
    }
    let blurb = if sel < lessons.len() { lessons[sel].blurb } else { "See you next time!" };
    text_c(blurb, mid, y + h - fs * 2.6, fs * 0.85, DIM);
    text_c("UP / DOWN choose    ENTER fly    H controls    ESC quit", mid, y + h - fs * 1.1, fs * 0.75, DIM);
    text_c("SKYHOP", mid + 3.0, sh * 0.13 + 3.0 + bob, title, Color::new(0.0, 0.0, 0.0, 0.5));
    text_c("SKYHOP", mid, sh * 0.13 + bob, title, WHITE);
    text_c("learn to fly - one hop at a time", mid, sh * 0.13 + title * 0.75, fs, Color::new(1.0, 0.95, 0.8, 1.0));
}

/// Small status text tucked under the minimap.
pub fn status_line(cam: &str, fps: i32, auto_rudder: bool) {
    let (sw, sh) = (screen_width(), screen_height());
    let map = (sw * 0.15).min(sh * 0.24).clamp(120.0, 230.0);
    let ar = if auto_rudder { "  |  auto-rudder" } else { "" };
    let s = format!("{cam} view (C)  |  {fps} fps{ar}");
    text(&s, sw - 12.0 - text_w(&s, 14.0), map + 34.0, 14.0, Color::new(1.0, 1.0, 1.0, 0.75));
}

pub fn cloud_fade(density: f32) {
    if density > 0.0 {
        let a = (density * 2.5).min(0.92);
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.93, 0.94, 0.96, a));
    }
}
