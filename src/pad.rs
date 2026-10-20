//! Game controllers (Xbox and other standard pads) through gilrs, polled
//! once per frame. The original was touch-only; this stands in for its
//! two wheels and buttons. Not available in the web build.

// The web build has no controller backend yet: the shared helpers sit idle.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Button {
    A,
    B,
    X,
    Y,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Start,
    View,
    Up,
    Down,
    Left,
    Right,
}

/// Sticks below this are at rest; triggers count as held past half way.
const DEAD_ZONE: f32 = 0.2;
const TRIGGER_DOWN: f32 = 0.5;

#[derive(Default)]
struct State {
    left: Vec2,
    right: Vec2,
    down: Vec<Button>,
    pressed: Vec<Button>,
    connected: bool,
}

/// One frame's raw readings, before edge detection.
struct Raw {
    name: String,
    left: Vec2,
    right: Vec2,
    held: Vec<Button>,
}

fn stick(x: f32, y_up: f32) -> Vec2 {
    // Both backends report y up; the game has y down.
    let v = vec2(x, -y_up);
    if v.length() < DEAD_ZONE {
        Vec2::ZERO
    } else {
        v.clamp_length_max(1.0)
    }
}

/// macOS: Apple's GameController framework, which is how Xbox, PlayStation
/// and MFi pads are exposed (their raw HID reports are vendor-specific).
#[cfg(target_os = "macos")]
mod raw {
    use super::*;
    use objc2_game_controller::{GCController, GCControllerButtonInput, GCDevice};

    pub fn read() -> Option<Raw> {
        unsafe {
            GCController::setShouldMonitorBackgroundEvents(true);
            let controllers = GCController::controllers();
            let (c, gp) = controllers.iter().find_map(|c| c.extendedGamepad().map(|g| (c, g)))?;
            let name = c.vendorName().map(|n| n.to_string()).unwrap_or_else(|| "controller".into());
            let on = |b: &GCControllerButtonInput| b.isPressed();
            let trig = |b: &GCControllerButtonInput| b.value() > TRIGGER_DOWN;
            let ls = gp.leftThumbstick();
            let rs = gp.rightThumbstick();
            let dpad = gp.dpad();
            let mut held = Vec::new();
            let pairs = [
                (Button::A, on(&gp.buttonA())),
                (Button::B, on(&gp.buttonB())),
                (Button::X, on(&gp.buttonX())),
                (Button::Y, on(&gp.buttonY())),
                (Button::LeftBumper, on(&gp.leftShoulder())),
                (Button::RightBumper, on(&gp.rightShoulder())),
                (Button::LeftTrigger, trig(&gp.leftTrigger())),
                (Button::RightTrigger, trig(&gp.rightTrigger())),
                (Button::Start, on(&gp.buttonMenu())),
                (Button::View, gp.buttonOptions().is_some_and(|b| on(&b))),
                (Button::Up, on(&dpad.up())),
                (Button::Down, on(&dpad.down())),
                (Button::Left, on(&dpad.left())),
                (Button::Right, on(&dpad.right())),
            ];
            for (b, down) in pairs {
                if down {
                    held.push(b);
                }
            }
            Some(Raw {
                name,
                left: stick(ls.xAxis().value(), ls.yAxis().value()),
                right: stick(rs.xAxis().value(), rs.yAxis().value()),
                held,
            })
        }
    }
}

/// Windows and Linux: gilrs.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
mod raw {
    use super::*;
    use gilrs::{Axis, Button as G, Gilrs};
    use std::cell::RefCell;

    thread_local! {
        static GILRS: RefCell<Option<Gilrs>> = RefCell::new(Gilrs::new().ok());
    }

    pub fn read() -> Option<Raw> {
        GILRS.with(|g| {
            let mut g = g.borrow_mut();
            let gilrs = g.as_mut()?;
            while gilrs.next_event().is_some() {}
            let (_, pad) = gilrs.gamepads().find(|(_, p)| p.is_connected())?;
            let trigger = |b: G, axis: Axis| {
                pad.button_data(b).map(|d| d.value()).unwrap_or(0.0).max(pad.value(axis)) > TRIGGER_DOWN
            };
            let pairs = [
                (Button::A, pad.is_pressed(G::South)),
                (Button::B, pad.is_pressed(G::East)),
                (Button::X, pad.is_pressed(G::West)),
                (Button::Y, pad.is_pressed(G::North)),
                (Button::LeftBumper, pad.is_pressed(G::LeftTrigger)),
                (Button::RightBumper, pad.is_pressed(G::RightTrigger)),
                (Button::LeftTrigger, trigger(G::LeftTrigger2, Axis::LeftZ)),
                (Button::RightTrigger, trigger(G::RightTrigger2, Axis::RightZ)),
                (Button::Start, pad.is_pressed(G::Start)),
                (Button::View, pad.is_pressed(G::Select)),
                (Button::Up, pad.is_pressed(G::DPadUp)),
                (Button::Down, pad.is_pressed(G::DPadDown)),
                (Button::Left, pad.is_pressed(G::DPadLeft)),
                (Button::Right, pad.is_pressed(G::DPadRight)),
            ];
            Some(Raw {
                name: pad.name().to_string(),
                left: stick(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)),
                right: stick(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
                held: pairs.into_iter().filter(|p| p.1).map(|p| p.0).collect(),
            })
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    pub fn poll() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let before = std::mem::take(&mut s.down);
            let was_connected = s.connected;
            *s = State::default();
            let Some(raw) = raw::read() else { return };
            if !was_connected {
                eprintln!("controller: {}", raw.name);
            }
            s.connected = true;
            s.left = raw.left;
            s.right = raw.right;
            for b in raw.held {
                if !before.contains(&b) {
                    s.pressed.push(b);
                }
                s.down.push(b);
            }
            // MB_PAD_LOG=1 prints presses and stick positions, to check a
            // controller's layout.
            if std::env::var_os("MB_PAD_LOG").is_some() && (!s.pressed.is_empty() || s.left != Vec2::ZERO || s.right != Vec2::ZERO) {
                eprintln!("pad: pressed {:?} left {:.2?} right {:.2?}", s.pressed, s.left, s.right);
            }
        });
    }

    pub(super) fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|s| f(&s.borrow()))
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    use super::*;
    pub fn poll() {}
    pub(super) fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        f(&State::default())
    }
}

/// Reads the controllers; call once at the start of each frame.
pub fn poll() {
    backend::poll();
}

/// Left stick, y down, length 0-1 (zero inside the dead zone).
pub fn left() -> Vec2 {
    backend::with(|s| s.left)
}

/// Right stick, y down, length 0-1 (zero inside the dead zone).
pub fn right() -> Vec2 {
    backend::with(|s| s.right)
}

pub fn down(b: Button) -> bool {
    backend::with(|s| s.down.contains(&b))
}

/// Pressed since the last frame.
pub fn pressed(b: Button) -> bool {
    backend::with(|s| s.pressed.contains(&b))
}
