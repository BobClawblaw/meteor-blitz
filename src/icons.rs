//! Controller button icons in the Controls page and tutorial hints, drawn
//! inline by `Font::text`: `Input::name` writes a pad button as one
//! private-use character (`char_of`), which the font draws as that button's
//! icon in the style of the connected pad (Xbox, PlayStation or Nintendo),
//! or the one Options > Button icons picks.

use std::cell::Cell;

use macroquad::prelude::*;

use crate::hud::Font;
use crate::pad::Button;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Xbox,
    PlayStation,
    Nintendo,
}

/// `settings.txt` `button_icons`: 0 follows the connected pad.
pub const AUTO: u8 = 0;
pub const STYLES: [Style; 3] = [Style::Xbox, Style::PlayStation, Style::Nintendo];

thread_local! {
    static SETTING: Cell<u8> = const { Cell::new(AUTO) };
    static DETECTED: Cell<Style> = const { Cell::new(Style::Xbox) };
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Xbox => "Xbox",
            Style::PlayStation => "PlayStation",
            Style::Nintendo => "Nintendo",
        }
    }
}

/// The style a controller's name suggests: Sony (vendor 054c) and Nintendo
/// (057e) pads by name or id, anything else the Xbox layout.
pub fn style_of(name: &str) -> Style {
    let n = name.to_lowercase();
    if ["dualsense", "dualshock", "playstation", "ps4", "ps5", "054c"].iter().any(|k| n.contains(k)) {
        Style::PlayStation
    } else if ["nintendo", "pro controller", "joy-con", "057e"].iter().any(|k| n.contains(k)) {
        Style::Nintendo
    } else {
        Style::Xbox
    }
}

/// The pad that connected last (`pad::poll`).
pub fn detected(name: &str) {
    DETECTED.with(|d| d.set(style_of(name)));
}

pub fn detected_style() -> Style {
    DETECTED.with(|d| d.get())
}

/// Options > Button icons: `AUTO` or 1 + an index into `STYLES`.
pub fn set_setting(v: u8) {
    SETTING.with(|s| s.set(v));
}

pub fn style() -> Style {
    match SETTING.with(|s| s.get()) {
        AUTO => detected_style(),
        v => STYLES[(v as usize - 1).min(STYLES.len() - 1)],
    }
}

const BUTTONS: [Button; 14] = [
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
const FIRST: u32 = 0xE000;

/// The character `Font::text` draws as `b`'s icon.
pub fn char_of(b: Button) -> char {
    let i = BUTTONS.iter().position(|&x| x == b).unwrap_or(0);
    char::from_u32(FIRST + i as u32).unwrap_or(' ')
}

pub fn button_of(c: char) -> Option<Button> {
    (c as u32).checked_sub(FIRST).and_then(|i| BUTTONS.get(i as usize)).copied()
}

/// What an icon looks like: a round face button, a pill with a label, or
/// the D-pad with one arm lit.
enum Shape {
    Face(Face),
    Pill(&'static str),
    Round(Glyph),
    Dpad(Vec2),
}

enum Face {
    Letter(&'static str, Color),
    Cross,
    Circle,
    Square,
    Triangle,
}

enum Glyph {
    Menu,
    View,
    Plus,
    Minus,
}

fn shape(b: Button, style: Style) -> Shape {
    use Style::*;
    let green = Color::new(0.42, 0.78, 0.22, 1.0);
    let red = Color::new(0.9, 0.26, 0.24, 1.0);
    let blue = Color::new(0.22, 0.52, 0.95, 1.0);
    let yellow = Color::new(0.98, 0.78, 0.12, 1.0);
    let grey = Color::new(0.88, 0.88, 0.9, 1.0);
    match (b, style) {
        (Button::Up, _) => Shape::Dpad(vec2(0.0, -1.0)),
        (Button::Down, _) => Shape::Dpad(vec2(0.0, 1.0)),
        (Button::Left, _) => Shape::Dpad(vec2(-1.0, 0.0)),
        (Button::Right, _) => Shape::Dpad(vec2(1.0, 0.0)),
        (Button::A, Xbox) => Shape::Face(Face::Letter("A", green)),
        (Button::B, Xbox) => Shape::Face(Face::Letter("B", red)),
        (Button::X, Xbox) => Shape::Face(Face::Letter("X", blue)),
        (Button::Y, Xbox) => Shape::Face(Face::Letter("Y", yellow)),
        (Button::A, PlayStation) => Shape::Face(Face::Cross),
        (Button::B, PlayStation) => Shape::Face(Face::Circle),
        (Button::X, PlayStation) => Shape::Face(Face::Square),
        (Button::Y, PlayStation) => Shape::Face(Face::Triangle),
        // The pads report buttons by position: Nintendo's south button is
        // B, east A, west Y and north X.
        (Button::A, Nintendo) => Shape::Face(Face::Letter("B", grey)),
        (Button::B, Nintendo) => Shape::Face(Face::Letter("A", grey)),
        (Button::X, Nintendo) => Shape::Face(Face::Letter("Y", grey)),
        (Button::Y, Nintendo) => Shape::Face(Face::Letter("X", grey)),
        (Button::LeftBumper, Xbox) => Shape::Pill("LB"),
        (Button::RightBumper, Xbox) => Shape::Pill("RB"),
        (Button::LeftTrigger, Xbox) => Shape::Pill("LT"),
        (Button::RightTrigger, Xbox) => Shape::Pill("RT"),
        (Button::LeftBumper, PlayStation) => Shape::Pill("L1"),
        (Button::RightBumper, PlayStation) => Shape::Pill("R1"),
        (Button::LeftTrigger, PlayStation) => Shape::Pill("L2"),
        (Button::RightTrigger, PlayStation) => Shape::Pill("R2"),
        (Button::LeftBumper, Nintendo) => Shape::Pill("L"),
        (Button::RightBumper, Nintendo) => Shape::Pill("R"),
        (Button::LeftTrigger, Nintendo) => Shape::Pill("ZL"),
        (Button::RightTrigger, Nintendo) => Shape::Pill("ZR"),
        (Button::Start, Xbox) => Shape::Round(Glyph::Menu),
        (Button::View, Xbox) => Shape::Round(Glyph::View),
        (Button::Start, PlayStation) => Shape::Pill("Options"),
        (Button::View, PlayStation) => Shape::Pill("Create"),
        (Button::Start, Nintendo) => Shape::Round(Glyph::Plus),
        (Button::View, Nintendo) => Shape::Round(Glyph::Minus),
    }
}

/// Icon height in cap heights; pill labels are set at this share of the
/// text's scale.
const HEIGHT: f32 = 1.6;
const LABEL: f32 = 0.72;

fn label_width(font: &Font, text: &str, scale: f32) -> f32 {
    font.width(text, scale * LABEL) - scale * LABEL
}

/// The icon's width at a text scale (the font adds its own gap).
pub fn width(font: &Font, b: Button, scale: f32) -> f32 {
    let h = font.cap_height(scale) * HEIGHT;
    match shape(b, style()) {
        Shape::Pill(text) => label_width(font, text, scale) + h * 0.75,
        _ => h,
    }
}

/// Draws `b`'s icon with its left edge at x, centred on the text line that
/// hangs from y. `tint` dims it with the surrounding text.
pub fn draw(font: &Font, b: Button, x: f32, y: f32, scale: f32, tint: Color) {
    let cap = font.cap_height(scale);
    let h = cap * HEIGHT;
    let r = h * 0.5;
    let cy = y + cap * 0.5;
    let k = tint.r.max(tint.g).max(tint.b);
    let c = |r: f32, g: f32, b: f32| Color::new(r * k, g * k, b * k, tint.a);
    let tc = |col: Color| c(col.r, col.g, col.b);
    let dark = c(0.1, 0.1, 0.12);
    let light = c(0.9, 0.9, 0.92);
    let line = (scale * 1.6).max(0.6);
    // Text centred in a box: the font hangs glyphs from their top.
    let label = |text: &str, cx: f32, colour: Color| {
        let s = scale * LABEL;
        font.text(text, cx, cy - font.cap_height(s) * 0.5, s, crate::hud::Align::Centre, colour);
    };
    match shape(b, style()) {
        Shape::Face(face) => {
            let cx = x + r;
            match face {
                Face::Letter(text, colour) => {
                    draw_circle(cx, cy, r, tc(colour));
                    label(text, cx, dark);
                }
                face => {
                    draw_circle(cx, cy, r, dark);
                    draw_circle_lines(cx, cy, r - line * 0.3, line * 0.6, c(0.45, 0.45, 0.5));
                    let s = r * 0.48;
                    match face {
                        Face::Cross => {
                            let col = c(0.5, 0.72, 1.0);
                            draw_line(cx - s, cy - s, cx + s, cy + s, line, col);
                            draw_line(cx - s, cy + s, cx + s, cy - s, line, col);
                        }
                        Face::Circle => draw_circle_lines(cx, cy, s * 1.05, line, c(1.0, 0.42, 0.45)),
                        Face::Square => {
                            let s = s * 0.9;
                            draw_rectangle_lines(cx - s, cy - s, s * 2.0, s * 2.0, line * 2.0, c(0.95, 0.55, 0.85))
                        }
                        Face::Triangle => draw_triangle_lines(
                            vec2(cx, cy - s * 1.1),
                            vec2(cx - s * 1.1, cy + s * 0.75),
                            vec2(cx + s * 1.1, cy + s * 0.75),
                            line,
                            c(0.3, 0.85, 0.68),
                        ),
                        Face::Letter(..) => {}
                    }
                }
            }
        }
        Shape::Pill(text) => {
            let w = label_width(font, text, scale) + h * 0.75;
            let ph = h * 0.86;
            let pr = ph * 0.5;
            draw_rectangle(x + pr, cy - pr, w - ph, ph, light);
            draw_circle(x + pr, cy, pr, light);
            draw_circle(x + w - pr, cy, pr, light);
            label(text, x + w * 0.5, dark);
        }
        Shape::Round(glyph) => {
            let cx = x + r;
            draw_circle(cx, cy, r, light);
            let s = r * 0.5;
            match glyph {
                Glyph::Menu => {
                    for dy in [-s * 0.7, 0.0, s * 0.7] {
                        draw_line(cx - s, cy + dy, cx + s, cy + dy, line, dark);
                    }
                }
                Glyph::View => {
                    let q = s * 1.1;
                    draw_rectangle_lines(cx - s, cy - s, q, q, line * 2.0, dark);
                    draw_rectangle(cx - s + q * 0.55, cy - s + q * 0.55, q, q, dark);
                }
                Glyph::Plus => {
                    draw_line(cx - s, cy, cx + s, cy, line * 1.3, dark);
                    draw_line(cx, cy - s, cx, cy + s, line * 1.3, dark);
                }
                Glyph::Minus => draw_line(cx - s, cy, cx + s, cy, line * 1.3, dark),
            }
        }
        Shape::Dpad(dir) => {
            let cx = x + r;
            let arm = r * 0.95;
            let half = r * 0.34;
            let body = c(0.45, 0.45, 0.5);
            draw_rectangle(cx - arm, cy - half, arm * 2.0, half * 2.0, body);
            draw_rectangle(cx - half, cy - arm, half * 2.0, arm * 2.0, body);
            // The pressed arm, lit.
            let (ax, ay, aw, ah) = if dir.x != 0.0 {
                (if dir.x < 0.0 { cx - arm } else { cx + half }, cy - half, arm - half, half * 2.0)
            } else {
                (cx - half, if dir.y < 0.0 { cy - arm } else { cy + half }, half * 2.0, arm - half)
            };
            draw_rectangle(ax, ay, aw, ah, light);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_has_its_own_character() {
        for b in BUTTONS {
            assert_eq!(button_of(char_of(b)), Some(b));
        }
        assert_eq!(button_of('A'), None);
    }

    #[test]
    fn names_pick_the_style() {
        assert_eq!(style_of("DualSense Wireless Controller"), Style::PlayStation);
        assert_eq!(style_of("DUALSHOCK 4 Wireless Controller"), Style::PlayStation);
        assert_eq!(style_of("Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)"), Style::PlayStation);
        assert_eq!(style_of("Pro Controller"), Style::Nintendo);
        assert_eq!(style_of("Xbox Wireless Controller"), Style::Xbox);
        assert_eq!(style_of("gamepad"), Style::Xbox);
    }
}
