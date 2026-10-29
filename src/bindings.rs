//! Rebindable game controls (port addition: the original was touch-only).
//!
//! Each action has a keyboard/mouse list and a controller list, saved to
//! `controls.txt` as `action=input,input` lines. The sticks are fixed: the
//! left stick moves and the right stick aims and fires. Menu navigation
//! (arrows, Enter, Esc, D-pad, A, B) is fixed too, so a bad binding can
//! always be undone from the Options screen.

use std::cell::RefCell;

use macroquad::prelude::*;

use crate::pad::{self, Button};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Fire,
    Boost,
    Bomb,
    SwitchWeapon,
    Gravity,
    Pause,
}

pub const ACTIONS: [Action; 10] = [
    Action::Up,
    Action::Down,
    Action::Left,
    Action::Right,
    Action::Fire,
    Action::Boost,
    Action::Bomb,
    Action::SwitchWeapon,
    Action::Gravity,
    Action::Pause,
];

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Up => "Move up",
            Action::Down => "Move down",
            Action::Left => "Move left",
            Action::Right => "Move right",
            Action::Fire => "Fire",
            Action::Boost => "Boost",
            Action::Bomb => "Bomb",
            Action::SwitchWeapon => "Switch weapon",
            Action::Gravity => "Gravity gun",
            Action::Pause => "Pause",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Action::Up => "up",
            Action::Down => "down",
            Action::Left => "left",
            Action::Right => "right",
            Action::Fire => "fire",
            Action::Boost => "boost",
            Action::Bomb => "bomb",
            Action::SwitchWeapon => "switch_weapon",
            Action::Gravity => "gravity",
            Action::Pause => "pause",
        }
    }
}

/// A keyboard key, mouse button or wheel, or controller button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    Key(KeyCode),
    MouseLeft,
    MouseRight,
    MouseMiddle,
    Wheel,
    Pad(Button),
}

impl Input {
    pub fn is_pad(self) -> bool {
        matches!(self, Input::Pad(_))
    }

    fn pressed(self) -> bool {
        match self {
            Input::Key(k) => is_key_pressed(k),
            Input::MouseLeft => is_mouse_button_pressed(MouseButton::Left),
            Input::MouseRight => is_mouse_button_pressed(MouseButton::Right),
            Input::MouseMiddle => is_mouse_button_pressed(MouseButton::Middle),
            Input::Wheel => mouse_wheel().1 != 0.0,
            Input::Pad(b) => pad::pressed(b),
        }
    }

    fn down(self) -> bool {
        match self {
            Input::Key(k) => is_key_down(k),
            Input::MouseLeft => is_mouse_button_down(MouseButton::Left),
            Input::MouseRight => is_mouse_button_down(MouseButton::Right),
            Input::MouseMiddle => is_mouse_button_down(MouseButton::Middle),
            Input::Wheel => mouse_wheel().1 != 0.0,
            Input::Pad(b) => pad::down(b),
        }
    }

    /// The name shown on the Controls page and in tutorial hints (the game
    /// font has letters, digits and spaces).
    pub fn name(self) -> String {
        match self {
            Input::Key(k) => key_name(k),
            Input::MouseLeft => "Left mouse".to_owned(),
            Input::MouseRight => "Right mouse".to_owned(),
            Input::MouseMiddle => "Middle mouse".to_owned(),
            Input::Wheel => "Mouse wheel".to_owned(),
            Input::Pad(b) => match b {
                Button::LeftBumper => "LB",
                Button::RightBumper => "RB",
                Button::LeftTrigger => "LT",
                Button::RightTrigger => "RT",
                Button::Up => "Dpad up",
                Button::Down => "Dpad down",
                Button::Left => "Dpad left",
                Button::Right => "Dpad right",
                Button::A => "A",
                Button::B => "B",
                Button::X => "X",
                Button::Y => "Y",
                Button::Start => "Start",
                Button::View => "View",
            }
            .to_owned(),
        }
    }

    fn to_text(self) -> String {
        match self {
            Input::Key(k) => format!("key:{:?}", k),
            Input::MouseLeft => "mouse:left".to_owned(),
            Input::MouseRight => "mouse:right".to_owned(),
            Input::MouseMiddle => "mouse:middle".to_owned(),
            Input::Wheel => "mouse:wheel".to_owned(),
            Input::Pad(b) => format!("pad:{:?}", b),
        }
    }

    fn parse(text: &str) -> Option<Input> {
        let (kind, name) = text.trim().split_once(':')?;
        match kind {
            "key" => KEYS.iter().find(|k| format!("{:?}", k) == name).map(|&k| Input::Key(k)),
            "mouse" => match name {
                "left" => Some(Input::MouseLeft),
                "right" => Some(Input::MouseRight),
                "middle" => Some(Input::MouseMiddle),
                "wheel" => Some(Input::Wheel),
                _ => None,
            },
            "pad" => PAD_BUTTONS.iter().find(|b| format!("{:?}", b) == name).map(|&b| Input::Pad(b)),
            _ => None,
        }
    }
}

/// Keys a binding can use. Escape, Enter and F11/F12 stay reserved for the
/// menus, fullscreen and screenshots.
const KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
        Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
        Space, Tab, Backspace, LeftShift, RightShift, LeftControl, RightControl, LeftAlt,
        RightAlt, Up, Down, Left, Right, Insert, Delete, Home, End, PageUp, PageDown,
        Kp0, Kp1, Kp2, Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9,
        F1, F2, F3, F4, F5, F6,
    ]
};

const PAD_BUTTONS: [Button; 14] = [
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::LeftBumper,
    Button::RightBumper,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::Start,
    Button::View,
    Button::Up,
    Button::Down,
    Button::Left,
    Button::Right,
];

fn key_name(k: KeyCode) -> String {
    let debug = format!("{:?}", k);
    match k {
        KeyCode::LeftShift => "Left shift".to_owned(),
        KeyCode::RightShift => "Right shift".to_owned(),
        KeyCode::LeftControl => "Left ctrl".to_owned(),
        KeyCode::RightControl => "Right ctrl".to_owned(),
        KeyCode::LeftAlt => "Left alt".to_owned(),
        KeyCode::RightAlt => "Right alt".to_owned(),
        KeyCode::PageUp => "Page up".to_owned(),
        KeyCode::PageDown => "Page down".to_owned(),
        _ if debug.starts_with("Key") && debug.len() == 4 => debug[3..].to_owned(),
        _ if debug.starts_with("Kp") => format!("Keypad {}", &debug[2..]),
        _ => debug,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Bindings {
    /// Keyboard/mouse inputs, then controller inputs, per action in
    /// `ACTIONS` order.
    keys: [Vec<Input>; 10],
    pads: [Vec<Input>; 10],
}

const PATH: &str = "controls.txt";

impl Default for Bindings {
    /// The port's original layout (see AGENTS.md, Controls).
    fn default() -> Bindings {
        use Input::*;
        let k = Key;
        let p = Pad;
        Bindings {
            keys: [
                vec![k(KeyCode::W)],
                vec![k(KeyCode::S)],
                vec![k(KeyCode::A)],
                vec![k(KeyCode::D)],
                vec![MouseLeft],
                vec![k(KeyCode::Space)],
                vec![MouseRight, k(KeyCode::B)],
                vec![k(KeyCode::Q), Wheel],
                vec![k(KeyCode::E)],
                vec![],
            ],
            pads: [
                vec![],
                vec![],
                vec![],
                vec![],
                vec![p(Button::RightTrigger)],
                vec![p(Button::A), p(Button::LeftBumper)],
                vec![p(Button::B), p(Button::RightBumper)],
                vec![p(Button::X), p(Button::Y)],
                vec![p(Button::LeftTrigger)],
                vec![p(Button::Start), p(Button::View)],
            ],
        }
    }
}

fn index(action: Action) -> usize {
    ACTIONS.iter().position(|&a| a == action).unwrap_or(0)
}

impl Bindings {
    pub fn load() -> Bindings {
        std::fs::read_to_string(crate::paths::save(PATH))
            .map(|t| Bindings::parse(&t))
            .unwrap_or_default()
    }

    /// On the web build there is no filesystem; bindings last the session.
    pub fn save(&self) {
        let _ = std::fs::write(crate::paths::save(PATH), self.to_text());
    }

    fn parse(text: &str) -> Bindings {
        let mut b = Bindings::default();
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else { continue };
            let Some(&action) = ACTIONS.iter().find(|a| a.key() == name.trim()) else { continue };
            let inputs: Vec<Input> = value.split(',').filter_map(Input::parse).collect();
            let i = index(action);
            b.keys[i] = inputs.iter().copied().filter(|x| !x.is_pad()).collect();
            b.pads[i] = inputs.iter().copied().filter(|x| x.is_pad()).collect();
        }
        b
    }

    fn to_text(&self) -> String {
        let mut out = String::new();
        for (i, action) in ACTIONS.iter().enumerate() {
            let all: Vec<String> = self.keys[i].iter().chain(&self.pads[i]).map(|x| x.to_text()).collect();
            out.push_str(&format!("{}={}\n", action.key(), all.join(",")));
        }
        out
    }

    pub fn keys(&self, action: Action) -> &[Input] {
        &self.keys[index(action)]
    }

    pub fn pads(&self, action: Action) -> &[Input] {
        &self.pads[index(action)]
    }

    /// Replaces the keyboard or controller column of `action` with `input`.
    pub fn set(&mut self, action: Action, input: Input) {
        let i = index(action);
        let column = if input.is_pad() { &mut self.pads[i] } else { &mut self.keys[i] };
        *column = vec![input];
    }

    pub fn clear(&mut self, action: Action, pad: bool) {
        let i = index(action);
        if pad {
            self.pads[i].clear();
        } else {
            self.keys[i].clear();
        }
    }

    /// "Space or A or LB": every input for `action`, for hints.
    pub fn describe(&self, action: Action) -> String {
        let names: Vec<String> = self.keys(action).iter().chain(self.pads(action)).map(|x| x.name()).collect();
        if names.is_empty() {
            "Unbound".to_owned()
        } else {
            names.join(" or ")
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Bindings> = RefCell::new(Bindings::default());
}

/// Installs the bindings the game reads (`pressed`, `down`).
pub fn install(b: Bindings) {
    CURRENT.with(|c| *c.borrow_mut() = b);
}

pub fn current() -> Bindings {
    CURRENT.with(|c| c.borrow().clone())
}

fn any(action: Action, f: fn(Input) -> bool) -> bool {
    CURRENT.with(|c| {
        let b = c.borrow();
        b.keys(action).iter().chain(b.pads(action)).any(|&x| f(x))
    })
}

/// Pressed this frame, through any of the action's inputs.
pub fn pressed(action: Action) -> bool {
    any(action, Input::pressed)
}

/// Held, through any of the action's inputs.
pub fn down(action: Action) -> bool {
    any(action, Input::down)
}

fn column(action: Action, pad: bool, f: fn(Input) -> bool) -> bool {
    CURRENT.with(|c| {
        let b = c.borrow();
        let inputs = if pad { b.pads(action) } else { b.keys(action) };
        inputs.iter().any(|&x| f(x))
    })
}

/// Held through the keyboard/mouse column only.
pub fn key_down(action: Action) -> bool {
    column(action, false, Input::down)
}

pub fn key_pressed(action: Action) -> bool {
    column(action, false, Input::pressed)
}

/// Held through the controller column only.
pub fn pad_down(action: Action) -> bool {
    column(action, true, Input::down)
}

/// Whether the action is bound to the left mouse button (firing toward the
/// cursor and the double-click gravity gun follow it).
pub fn uses_mouse_left(action: Action) -> bool {
    CURRENT.with(|c| c.borrow().keys(action).contains(&Input::MouseLeft))
}

/// The input pressed this frame, for rebinding: a key, mouse button, wheel
/// or controller button. Escape cancels and is never returned.
pub fn captured() -> Option<Input> {
    if let Some(&k) = KEYS.iter().find(|&&k| is_key_pressed(k)) {
        return Some(Input::Key(k));
    }
    for (button, input) in [
        (MouseButton::Left, Input::MouseLeft),
        (MouseButton::Right, Input::MouseRight),
        (MouseButton::Middle, Input::MouseMiddle),
    ] {
        if is_mouse_button_pressed(button) {
            return Some(input);
        }
    }
    if mouse_wheel().1 != 0.0 {
        return Some(Input::Wheel);
    }
    PAD_BUTTONS.iter().find(|&&b| pad::pressed(b)).map(|&b| Input::Pad(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut b = Bindings::default();
        b.set(Action::Boost, Input::Key(KeyCode::LeftShift));
        b.set(Action::Bomb, Input::Pad(Button::Y));
        b.clear(Action::Pause, true);
        assert_eq!(Bindings::parse(&b.to_text()), b);
    }

    #[test]
    fn unknown_lines_keep_defaults() {
        let b = Bindings::parse("boost=key:Nope\nbogus=key:A\n");
        assert!(b.keys(Action::Boost).is_empty(), "a listed action takes what parses");
        assert_eq!(b.keys(Action::Fire), Bindings::default().keys(Action::Fire));
    }

    #[test]
    fn describes_every_input() {
        assert_eq!(Bindings::default().describe(Action::Boost), "Space or A or LB");
        assert_eq!(key_name(KeyCode::Key7), "7");
    }
}
