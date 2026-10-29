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
    use std::cell::Cell;

    thread_local! {
        /// Setting this posts a KVO notification through LaunchServices (a
        /// synchronous XPC round trip), so do it once, not every frame.
        static MONITORING: Cell<bool> = const { Cell::new(false) };
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const std::ffi::c_void;
        fn CFRunLoopRunInMode(mode: *const std::ffi::c_void, seconds: f64, return_after_source: u8) -> i32;
    }

    /// GameController posts its updates as blocks on the main queue, which
    /// miniquad's event loop (a zero-timeout NSEvent poll) never lets run:
    /// give the main run loop a turn so they arrive.
    fn pump_main_queue() {
        const HANDLED_SOURCE: i32 = 4;
        for _ in 0..64 {
            if unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.0, 1) } != HANDLED_SOURCE {
                break;
            }
        }
    }

    pub fn read() -> Option<Raw> {
        pump_main_queue();
        unsafe {
            MONITORING.with(|m| {
                if !m.get() {
                    GCController::setShouldMonitorBackgroundEvents(true);
                    m.set(true);
                }
            });
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
    use std::mem::ManuallyDrop;

    // Never dropped: on Windows the thread-local destructors run inside
    // ExitProcess, after the OS has killed any backend thread. gilrs's WGI
    // backend joined its thread in Drop and failed fast on every window close.
    thread_local! {
        static GILRS: RefCell<ManuallyDrop<Option<Gilrs>>> = RefCell::new(ManuallyDrop::new(Gilrs::new().ok()));
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
    use std::cell::{Cell, RefCell};

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
        static PAD_LOG: Cell<Option<bool>> = const { Cell::new(None) };
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
            let log = PAD_LOG.with(|c| c.get().unwrap_or_else(|| {
                let on = std::env::var_os("MB_PAD_LOG").is_some();
                c.set(Some(on));
                on
            }));
            if log && (!s.pressed.is_empty() || s.left != Vec2::ZERO || s.right != Vec2::ZERO) {
                eprintln!("pad: pressed {:?} left {:.2?} right {:.2?}", s.pressed, s.left, s.right);
            }
        });
    }

    pub(super) fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|s| f(&s.borrow()))
    }

    /// Menu navigation from the left stick: a push is one step, and holding
    /// repeats after a pause, like a D-pad.
    #[derive(Default)]
    struct Nav {
        dir: Option<Button>,
        held: f32,
        repeat: f32,
    }

    thread_local! {
        static NAV: RefCell<Nav> = RefCell::new(Nav::default());
    }

    pub fn nav(dt: f32) -> Option<Button> {
        let v = with(|s| s.left);
        let dir = if v.length() > 0.6 {
            Some(if v.x.abs() > v.y.abs() {
                if v.x < 0.0 {
                    Button::Left
                } else {
                    Button::Right
                }
            } else if v.y < 0.0 {
                Button::Up
            } else {
                Button::Down
            })
        } else {
            None
        };
        NAV.with(|n| {
            let mut n = n.borrow_mut();
            if n.dir != dir {
                n.dir = dir;
                n.held = 0.0;
                n.repeat = 0.0;
                return dir;
            }
            let Some(d) = dir else { return None };
            n.held += dt;
            if n.held >= 0.45 {
                n.repeat += dt;
                if n.repeat >= 0.12 {
                    n.repeat = 0.0;
                    return Some(d);
                }
            }
            None
        })
    }

    #[cfg(test)]
    pub(super) fn set_left_for_test(v: Vec2) {
        STATE.with(|s| s.borrow_mut().left = v);
    }

    #[cfg(test)]
    pub(super) fn reset_nav_for_test() {
        NAV.with(|n| *n.borrow_mut() = Nav::default());
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

/// Menu navigation from the left stick, edge-triggered and repeating while
/// held. `None` on the web build, which has no pad backend.
#[cfg(not(target_arch = "wasm32"))]
pub fn nav(dt: f32) -> Option<Button> {
    backend::nav(dt)
}

#[cfg(target_arch = "wasm32")]
pub fn nav(_dt: f32) -> Option<Button> {
    None
}

/// Right stick, y down, length 0-1 (zero inside the dead zone).
pub fn right() -> Vec2 {
    backend::with(|s| s.right)
}

pub fn down(b: Button) -> bool {
    backend::with(|s| s.down.contains(&b))
}

/// Any button held or stick pushed (the tutorial's "touching").
pub fn any_down() -> bool {
    backend::with(|s| !s.down.is_empty() || s.left != Vec2::ZERO || s.right != Vec2::ZERO)
}

/// Pressed since the last frame.
pub fn pressed(b: Button) -> bool {
    backend::with(|s| s.pressed.contains(&b))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn stick_navigation_steps_then_repeats_and_resets() {
        backend::reset_nav_for_test();
        backend::set_left_for_test(Vec2::ZERO);
        assert_eq!(nav(1.0 / 60.0), None);

        backend::set_left_for_test(vec2(0.0, -1.0));
        assert_eq!(nav(1.0 / 60.0), Some(Button::Up), "first push steps once");
        for _ in 0..20 {
            backend::set_left_for_test(vec2(0.0, -1.0));
            assert_eq!(nav(1.0 / 60.0), None, "no repeat before the hold delay");
        }

        let mut repeat = None;
        for _ in 0..20 {
            backend::set_left_for_test(vec2(0.0, -1.0));
            if let Some(b) = nav(1.0 / 60.0) {
                repeat = Some(b);
                break;
            }
        }
        assert_eq!(repeat, Some(Button::Up), "holding repeats");

        backend::set_left_for_test(Vec2::ZERO);
        assert_eq!(nav(1.0 / 60.0), None);
        backend::set_left_for_test(vec2(1.0, 0.0));
        assert_eq!(nav(1.0 / 60.0), Some(Button::Right), "a new push steps again");
    }
}
