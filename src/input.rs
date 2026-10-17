//! Gamepad input via gilrs, polled once per frame next to macroquad's keyboard.
//! Uses the standard (Xbox-style) layout: South = A, East = B, West = X, North = Y.

use gilrs::{EventType, GamepadId, Gilrs};

pub use gilrs::{Axis, Button};

const DEADZONE: f32 = 0.1;

pub struct Pad {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
    /// Buttons that went down this frame.
    pub just_pressed: Vec<Button>,
    nav_y: f32,
    /// Menu step from the left stick this frame: -1 up, +1 down.
    pub nav: i32,
}

/// Rescale so the stick reads 0 inside the deadzone and ramps from there.
fn dz(v: f32) -> f32 {
    (v.abs() - DEADZONE).max(0.0) / (1.0 - DEADZONE) * v.signum()
}

impl Pad {
    pub fn new() -> Self {
        Self { gilrs: Gilrs::new().ok(), active: None, just_pressed: Vec::new(), nav_y: 0.0, nav: 0 }
    }

    /// Drain pending events. Call once at the start of every frame.
    pub fn poll(&mut self) {
        self.just_pressed.clear();
        self.nav = 0;
        let Some(g) = &mut self.gilrs else { return };
        while let Some(ev) = g.next_event() {
            match ev.event {
                EventType::ButtonPressed(b, _) => {
                    self.active = Some(ev.id);
                    self.just_pressed.push(b);
                }
                EventType::AxisChanged(_, v, _) if v.abs() > 0.5 => self.active = Some(ev.id),
                EventType::Disconnected if self.active == Some(ev.id) => self.active = None,
                _ => {}
            }
        }
        if self.active.is_none() {
            self.active = g.gamepads().next().map(|(id, _)| id);
        }
        let y = self.axis(Axis::LeftStickY);
        if y.abs() > 0.6 && self.nav_y.abs() <= 0.6 {
            self.nav = if y > 0.0 { -1 } else { 1 };
        }
        self.nav_y = y;
    }

    pub fn connected(&self) -> bool {
        self.active.is_some()
    }

    /// Button went down this frame.
    pub fn pressed(&self, b: Button) -> bool {
        self.just_pressed.contains(&b)
    }

    pub fn down(&self, b: Button) -> bool {
        self.value(b) > 0.5
    }

    /// Analog button value (triggers), 0..1.
    pub fn value(&self, b: Button) -> f32 {
        match (&self.gilrs, self.active) {
            (Some(g), Some(id)) => g.connected_gamepad(id).and_then(|p| p.button_data(b).map(|d| d.value())).unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// Stick axis with deadzone applied, -1..1 (up / right positive).
    pub fn axis(&self, a: Axis) -> f32 {
        match (&self.gilrs, self.active) {
            (Some(g), Some(id)) => g.connected_gamepad(id).map_or(0.0, |p| dz(p.value(a))),
            _ => 0.0,
        }
    }
}
