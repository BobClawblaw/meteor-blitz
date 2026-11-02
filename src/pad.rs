//! Game controllers (Xbox and other standard pads), polled once per frame:
//! Apple's GameController framework on macOS, gilrs on Windows and Linux,
//! and the browser's Gamepad API on the web (through the page's
//! `mb_gamepad_read`, index.html). The original was touch-only; this stands
//! in for its two wheels and buttons.

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
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    use objc2_foundation::{NSArray, NSString};
    use objc2_game_controller::{GCController, GCControllerButtonInput, GCDevice, GCHapticsLocalityDefault};
    use std::cell::{Cell, RefCell};
    use std::ptr;

    #[link(name = "CoreHaptics", kind = "framework")]
    extern "C" {}

    thread_local! {
        /// Setting this posts a KVO notification through LaunchServices (a
        /// synchronous XPC round trip), so do it once, not every frame.
        static MONITORING: Cell<bool> = const { Cell::new(false) };
        /// The rumbling pad's Core Haptics engine, keyed by the controller.
        static ENGINE: RefCell<Option<(usize, Retained<AnyObject>)>> = const { RefCell::new(None) };
    }

    /// Rumble through the controller's haptics: one continuous Core Haptics
    /// event, its intensity the stronger motor and its sharpness the weak
    /// (high-frequency) motor's share. Core Haptics is driven by message
    /// sends: objc2-game-controller binds `createEngineWithLocality` only on
    /// iOS.
    pub fn rumble(strong: f32, weak: f32, seconds: f32) -> bool {
        unsafe {
            let controllers = GCController::controllers();
            let Some(c) = controllers.iter().find(|c| c.extendedGamepad().is_some()) else { return false };
            let key = Retained::as_ptr(&c) as usize;
            ENGINE.with(|e| {
                let mut e = e.borrow_mut();
                if e.as_ref().map(|(k, _)| *k) != Some(key) {
                    *e = engine(&c).map(|engine| (key, engine));
                }
                e.as_ref().and_then(|(_, engine)| play(engine, strong, weak, seconds)).is_some()
            })
        }
    }

    unsafe fn engine(c: &GCController) -> Option<Retained<AnyObject>> {
        let haptics = c.haptics()?;
        msg_send![&*haptics, createEngineWithLocality: GCHapticsLocalityDefault]
    }

    unsafe fn play(engine: &AnyObject, strong: f32, weak: f32, seconds: f32) -> Option<()> {
        let none = ptr::null_mut::<*mut AnyObject>();
        // The engine stops when the app is in the background or the pad
        // idles; starting a running engine is harmless.
        let started: bool = msg_send![engine, startAndReturnError: none];
        if !started {
            return None;
        }
        let new = |name: &std::ffi::CStr| -> Option<Allocated<AnyObject>> {
            let class = AnyClass::get(name)?;
            Some(msg_send![class, alloc])
        };
        let param = |id: &str, value: f32| -> Option<Retained<AnyObject>> {
            let a = new(c"CHHapticEventParameter")?;
            Some(msg_send![a, initWithParameterID: &*NSString::from_str(id), value: value])
        };
        let intensity = strong.max(weak);
        let sharpness = if intensity > 0.0 { weak / (strong + weak) } else { 0.0 };
        let params = NSArray::from_retained_slice(&[param("HapticIntensity", intensity)?, param("HapticSharpness", sharpness)?]);
        let event: Retained<AnyObject> = msg_send![
            new(c"CHHapticEvent")?,
            initWithEventType: &*NSString::from_str("HapticContinuous"),
            parameters: &*params,
            relativeTime: 0.0f64,
            duration: seconds as f64
        ];
        let events = NSArray::from_retained_slice(&[event]);
        let dynamic = NSArray::<AnyObject>::new();
        let pattern: Option<Retained<AnyObject>> =
            msg_send![new(c"CHHapticPattern")?, initWithEvents: &*events, parameters: &*dynamic, error: none];
        let player: Option<Retained<AnyObject>> = msg_send![engine, createPlayerWithPattern: &*pattern?, error: none];
        let _: bool = msg_send![&*player?, startAtTime: 0.0f64, error: none];
        Some(())
    }

    /// GameController posts its updates as blocks on the main queue. The
    /// event loop waits between frames (main.rs `frame_timer`), so the main
    /// run loop turns there and they arrive; pumping the run loop here,
    /// inside a frame, let a menu-bar click open its menu mid-frame and
    /// crash the game.
    pub fn read() -> Option<Raw> {
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

/// The web: the browser's Gamepad API, read by `mb_gamepad_read` in
/// index.html, which writes the first connected pad's four axes and 17
/// button values (the W3C "standard" layout, which is the Xbox layout).
#[cfg(target_arch = "wasm32")]
mod raw {
    use super::*;

    extern "C" {
        fn mb_gamepad_read(out: *mut f32) -> i32;
        fn mb_gamepad_rumble(strong: f32, weak: f32, ms: f32);
    }

    /// The Gamepad API's dual-rumble effect, where the browser has it.
    pub fn rumble(strong: f32, weak: f32, seconds: f32) -> bool {
        unsafe { mb_gamepad_rumble(strong, weak, seconds * 1000.0) };
        true
    }

    const AXES: usize = 4;

    pub fn read() -> Option<Raw> {
        let mut v = [0f32; AXES + 17];
        // 0: no pad; else 1 + the pad's style (index.html reads its id).
        let kind = unsafe { mb_gamepad_read(v.as_mut_ptr()) };
        if kind == 0 {
            return None;
        }
        let button = |i: usize| v[AXES + i];
        let pairs = [
            (Button::A, button(0) > 0.5),
            (Button::B, button(1) > 0.5),
            (Button::X, button(2) > 0.5),
            (Button::Y, button(3) > 0.5),
            (Button::LeftBumper, button(4) > 0.5),
            (Button::RightBumper, button(5) > 0.5),
            (Button::LeftTrigger, button(6) > TRIGGER_DOWN),
            (Button::RightTrigger, button(7) > TRIGGER_DOWN),
            (Button::View, button(8) > 0.5),
            (Button::Start, button(9) > 0.5),
            (Button::Up, button(12) > 0.5),
            (Button::Down, button(13) > 0.5),
            (Button::Left, button(14) > 0.5),
            (Button::Right, button(15) > 0.5),
        ];
        Some(Raw {
            name: match kind {
                2 => "PlayStation gamepad",
                3 => "Nintendo gamepad",
                _ => "gamepad",
            }
            .to_owned(),
            // The Gamepad API has y down; `stick` takes y up.
            left: stick(v[0], -v[1]),
            right: stick(v[2], -v[3]),
            held: pairs.iter().filter(|(_, down)| *down).map(|(b, _)| *b).collect(),
        })
    }
}

/// Windows and Linux: gilrs.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "macos")))]
mod raw {
    use super::*;
    use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Repeat, Replay, Ticks};
    use gilrs::{Axis, Button as G, Gilrs};
    use std::cell::RefCell;
    use std::mem::ManuallyDrop;

    // Never dropped: on Windows the thread-local destructors run inside
    // ExitProcess, after the OS has killed any backend thread. gilrs's WGI
    // backend joined its thread in Drop and failed fast on every window close.
    thread_local! {
        static GILRS: RefCell<ManuallyDrop<Option<Gilrs>>> = RefCell::new(ManuallyDrop::new(Gilrs::new().ok()));
        /// The rumble playing; dropping an effect stops it.
        static EFFECT: RefCell<ManuallyDrop<Option<Effect>>> = const { RefCell::new(ManuallyDrop::new(None)) };
    }

    /// gilrs force feedback: the strong and weak motors for `seconds`.
    pub fn rumble(strong: f32, weak: f32, seconds: f32) -> bool {
        GILRS.with(|g| {
            let mut g = g.borrow_mut();
            let Some(gilrs) = g.as_mut() else { return false };
            let Some(id) = gilrs.gamepads().find(|(_, p)| p.is_connected() && p.is_ff_supported()).map(|(id, _)| id)
            else {
                return false;
            };
            let length = Ticks::from_ms((seconds * 1000.0) as u32);
            let motor = |kind| BaseEffect {
                kind,
                scheduling: Replay { play_for: length, ..Default::default() },
                envelope: Default::default(),
            };
            let magnitude = |v: f32| (v.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
            let built = EffectBuilder::new()
                .add_effect(motor(BaseEffectType::Strong { magnitude: magnitude(strong) }))
                .add_effect(motor(BaseEffectType::Weak { magnitude: magnitude(weak) }))
                .gamepads(&[id])
                .repeat(Repeat::For(length))
                .finish(gilrs);
            let Ok(effect) = built else { return false };
            let played = effect.play().is_ok();
            EFFECT.with(|e| {
                let old = std::mem::replace(&mut *e.borrow_mut(), ManuallyDrop::new(Some(effect)));
                drop(ManuallyDrop::into_inner(old));
            });
            played
        })
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
                crate::icons::detected(&raw.name);
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

thread_local! {
    /// When the current rumble ends, and how strong it is.
    static RUMBLE: std::cell::Cell<(f64, f32)> = const { std::cell::Cell::new((0.0, 0.0)) };
}

/// Rumbles the connected pad (port addition): `strong` is the
/// low-frequency motor and `weak` the high-frequency one, both 0-1. A
/// weaker rumble does not cut off a stronger one that is still running.
pub fn rumble(strong: f32, weak: f32, seconds: f32) {
    let now = get_time();
    let level = strong.max(weak);
    if RUMBLE.with(|r| {
        let (until, playing) = r.get();
        now < until && level < playing
    }) {
        return;
    }
    RUMBLE.with(|r| r.set((now + seconds as f64, level)));
    let played = raw::rumble(strong.clamp(0.0, 1.0), weak.clamp(0.0, 1.0), seconds);
    if std::env::var_os("MB_PAD_LOG").is_some() {
        eprintln!("rumble: {strong:.2} {weak:.2} for {seconds:.2} s, played {played}");
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
/// held.
pub fn nav(dt: f32) -> Option<Button> {
    backend::nav(dt)
}

/// Right stick, y down, length 0-1 (zero inside the dead zone).
pub fn right() -> Vec2 {
    backend::with(|s| s.right)
}

pub fn down(b: Button) -> bool {
    backend::with(|s| s.down.contains(&b))
}

/// The Enter key, except as part of Alt+Enter (the fullscreen toggle).
pub fn enter_pressed() -> bool {
    is_key_pressed(KeyCode::Enter) && !is_key_down(KeyCode::LeftAlt) && !is_key_down(KeyCode::RightAlt)
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
