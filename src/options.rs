//! The Options screen (port addition): the main menu's SETTINGS button,
//! which did nothing in the port before. Display, audio and gameplay
//! options on one page, and the control bindings on another (HELP opens
//! that page directly).
//!
//! Keyboard: arrows/WASD move, Left/Right adjust, Enter toggles or opens,
//! Esc goes back. Controller: D-pad or left stick, A, B. Mouse: hover
//! selects, click activates, sliders drag.

use macroquad::prelude::*;

use crate::bindings::{self, Action, Bindings, ACTIONS};
use crate::gfx::{edge, Canvas, CANVAS_H, CANVAS_W};
use crate::hud::{Align, Font};
use crate::pad::{self, Button};
use crate::pause::{Settings, FPS_CAPS};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Main,
    Controls,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    Fullscreen,
    Widescreen,
    Bloom,
    VisualFx,
    FrameRate,
    Music,
    Effects,
    Shake,
    Tutorial,
    Controls,
    Back,
    /// A bindings row: the action, and which column (false keyboard/mouse,
    /// true controller) is selected.
    Bind(Action),
    Reset,
}

const MAIN_ROWS: [Row; 11] = [
    Row::Fullscreen,
    Row::Widescreen,
    Row::Bloom,
    Row::VisualFx,
    Row::FrameRate,
    Row::Music,
    Row::Effects,
    Row::Shake,
    Row::Tutorial,
    Row::Controls,
    Row::Back,
];

/// Layout, in canvas units.
const TITLE_Y: f32 = 10.0;
const MAIN_TOP: f32 = 40.0;
const MAIN_PITCH: f32 = 23.0;
const BIND_TOP: f32 = 52.0;
const BIND_PITCH: f32 = 20.0;
const ROW_SCALE: f32 = 0.6;
const LABEL_X: f32 = 40.0;
const VALUE_X: f32 = 440.0;
const SLIDER_X: f32 = 240.0;
const SLIDER_W: f32 = 200.0;
const KEY_COL_X: f32 = 250.0;
const PAD_COL_X: f32 = 385.0;
const COL_W: f32 = 130.0;
const STEP: f32 = 0.05;

pub struct Options {
    pub page: Page,
    selected: usize,
    /// On the Controls page: the controller column is selected.
    pad_column: bool,
    /// Waiting for the input to bind to this action.
    capturing: Option<Action>,
    dragging: bool,
    alpha: f32,
    /// Where Back goes: the Options main page, or out (HELP opened the
    /// Controls page directly).
    controls_only: bool,
}

/// What the caller should do after an update.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outcome {
    /// Something changed: play the button sound.
    Click,
    Close,
}

impl Options {
    pub fn new(page: Page) -> Options {
        Options {
            page,
            selected: 0,
            pad_column: false,
            capturing: None,
            dragging: false,
            alpha: 0.0,
            controls_only: page == Page::Controls,
        }
    }

    fn rows(&self) -> Vec<Row> {
        match self.page {
            Page::Main => MAIN_ROWS.to_vec(),
            Page::Controls => ACTIONS
                .iter()
                .map(|&a| Row::Bind(a))
                .chain([Row::Reset, Row::Back])
                .collect(),
        }
    }

    /// The row's hit area.
    fn rect(&self, i: usize) -> Rect {
        let (top, pitch) = match self.page {
            Page::Main => (MAIN_TOP, MAIN_PITCH),
            Page::Controls => (BIND_TOP, BIND_PITCH),
        };
        Rect::new(LABEL_X - 10.0, top + i as f32 * pitch - 4.0, VALUE_X - LABEL_X + 20.0, pitch - 2.0)
    }

    pub fn update(&mut self, dt: f32, settings: &mut Settings) -> Option<Outcome> {
        self.alpha = (self.alpha + dt * 4.0).min(1.0);
        if let Some(action) = self.capturing {
            return self.capture(action);
        }
        let rows = self.rows();
        let nav = pad::nav(dt);
        let pressed = |keys: &[KeyCode], b: Button| {
            keys.iter().any(|&k| is_key_pressed(k)) || pad::pressed(b) || nav == Some(b)
        };
        let up = pressed(&[KeyCode::Up, KeyCode::W], Button::Up);
        let down = pressed(&[KeyCode::Down, KeyCode::S], Button::Down);
        let left = pressed(&[KeyCode::Left, KeyCode::A], Button::Left);
        let right = pressed(&[KeyCode::Right, KeyCode::D], Button::Right);
        if up {
            self.selected = (self.selected + rows.len() - 1) % rows.len();
        }
        if down {
            self.selected = (self.selected + 1) % rows.len();
        }
        let mut outcome = None;
        let row = rows[self.selected];
        if left || right {
            let step = if right { STEP } else { -STEP };
            match row {
                Row::Music | Row::Effects | Row::Shake => {
                    let v = slider(row, settings);
                    set_slider(row, settings, (v + step).clamp(0.0, 1.0));
                }
                Row::Bind(_) => self.pad_column = right,
                Row::FrameRate => settings.fps = cycle(settings.fps, right),
                _ => {}
            }
        }

        // Mouse: hover selects; a click activates; sliders follow a drag.
        let mouse = Canvas::mouse();
        let hovered = (0..rows.len()).find(|&i| self.rect(i).contains(mouse));
        if mouse_delta_position() != Vec2::ZERO {
            if let Some(i) = hovered {
                self.selected = i;
                if let Row::Bind(_) = rows[i] {
                    self.pad_column = mouse.x >= PAD_COL_X - 8.0;
                }
            }
        }
        let click = is_mouse_button_pressed(MouseButton::Left) && hovered.is_some();
        if is_mouse_button_pressed(MouseButton::Left) {
            self.dragging = hovered.is_some_and(|i| matches!(rows[i], Row::Music | Row::Effects | Row::Shake));
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.dragging = false;
        }
        if self.dragging {
            let row = rows[self.selected];
            set_slider(row, settings, ((mouse.x - SLIDER_X) / SLIDER_W).clamp(0.0, 1.0));
        } else if click || pad::pressed(Button::A) || pad::enter_pressed() {
            outcome = self.activate(row, settings);
        }

        // Clear a binding: Delete or Backspace, or the controller's X.
        if let Row::Bind(action) = row {
            if is_key_pressed(KeyCode::Delete) || is_key_pressed(KeyCode::Backspace) || pad::pressed(Button::X) {
                let mut b = bindings::current();
                b.clear(action, self.pad_column);
                apply(b);
                outcome = Some(Outcome::Click);
            }
        }
        if is_key_pressed(KeyCode::Escape) || pad::pressed(Button::B) {
            outcome = Some(self.back());
        }
        outcome
    }

    fn back(&mut self) -> Outcome {
        if self.page == Page::Controls && !self.controls_only {
            self.page = Page::Main;
            self.selected = MAIN_ROWS.iter().position(|&r| r == Row::Controls).unwrap_or(0);
            Outcome::Click
        } else {
            Outcome::Close
        }
    }

    fn activate(&mut self, row: Row, settings: &mut Settings) -> Option<Outcome> {
        match row {
            Row::Fullscreen => {
                settings.fullscreen = !settings.fullscreen;
                set_fullscreen(settings.fullscreen);
            }
            Row::Widescreen => settings.widescreen = !settings.widescreen,
            Row::Bloom => settings.bloom = !settings.bloom,
            Row::VisualFx => settings.visual_fx = !settings.visual_fx,
            Row::FrameRate => settings.fps = cycle(settings.fps, true),
            Row::Tutorial => settings.tutorial = !settings.tutorial,
            Row::Music | Row::Effects | Row::Shake => return None,
            Row::Controls => {
                self.page = Page::Controls;
                self.selected = 0;
                self.pad_column = false;
            }
            Row::Back => return Some(self.back()),
            Row::Bind(action) => self.capturing = Some(action),
            Row::Reset => apply(Bindings::default()),
        }
        Some(Outcome::Click)
    }

    /// Waiting for an input: the next key, mouse button, wheel or pad
    /// button becomes the binding (in its own column); Escape cancels.
    fn capture(&mut self, action: Action) -> Option<Outcome> {
        if is_key_pressed(KeyCode::Escape) {
            self.capturing = None;
            return Some(Outcome::Click);
        }
        let input = bindings::captured()?;
        let mut b = bindings::current();
        b.set(action, input);
        apply(b);
        self.pad_column = input.is_pad();
        self.capturing = None;
        Some(Outcome::Click)
    }

    pub fn draw(&self, canvas: &Canvas, font: &Font, settings: &Settings) {
        let a = self.alpha;
        let white = Color::new(1.0, 1.0, 1.0, a);
        let dim = Color::new(0.7, 0.7, 0.75, a);
        // A dark band over the backdrop, across a wide view.
        let e = edge();
        canvas.fill_rect(-e, 0.0, CANVAS_W + 2.0 * e, CANVAS_H, Color::new(0.0, 0.0, 0.0, 0.7 * a));
        let title = match self.page {
            Page::Main => "Options",
            Page::Controls => "Controls",
        };
        let title_scale = if self.page == Page::Controls { 0.8 } else { 1.0 };
        font.text(title, 240.0, TITLE_Y, title_scale, Align::Centre, white);
        let rows = self.rows();
        if let Some(r) = rows.get(self.selected) {
            let rect = self.rect(self.selected);
            canvas.fill_rect(rect.x, rect.y, rect.w, rect.h, Color::new(1.0, 1.0, 1.0, 0.12 * a));
            if let Row::Bind(_) = r {
                let x = if self.pad_column { PAD_COL_X } else { KEY_COL_X };
                frame(canvas, Rect::new(x - 6.0, rect.y, COL_W, rect.h), white);
            }
        }
        let b = bindings::current();
        if self.page == Page::Controls {
            font.text("Keyboard", KEY_COL_X, BIND_TOP - 16.0, 0.45, Align::Left, dim);
            font.text("Controller", PAD_COL_X, BIND_TOP - 16.0, 0.45, Align::Left, dim);
        }
        for (i, &row) in rows.iter().enumerate() {
            let y = self.rect(i).y + 6.0;
            let label = match row {
                Row::Fullscreen => "Fullscreen",
                Row::Widescreen => "Widescreen",
                Row::Bloom => "Bloom",
                Row::VisualFx => "Lighting and effects",
                Row::FrameRate => "Frame rate cap",
                Row::Music => "Music",
                Row::Effects => "Sound effects",
                Row::Shake => "Screen shake",
                Row::Tutorial => "Tutorial before first game",
                Row::Controls => "Controls",
                Row::Back => "Back",
                Row::Bind(action) => action.label(),
                Row::Reset => "Reset to defaults",
            };
            font.text(label, LABEL_X, y, ROW_SCALE, Align::Left, white);
            match row {
                Row::Fullscreen => on_off(font, settings.fullscreen, y, white),
                Row::Widescreen => on_off(font, settings.widescreen, y, white),
                Row::Bloom => on_off(font, settings.bloom, y, white),
                Row::VisualFx => on_off(font, settings.visual_fx, y, white),
                Row::FrameRate => font.text(&settings.fps.to_string(), VALUE_X, y, ROW_SCALE, Align::Right, white),
                Row::Tutorial => on_off(font, settings.tutorial, y, white),
                Row::Music | Row::Effects | Row::Shake => {
                    let v = slider(row, settings);
                    let mid = y + 4.0;
                    canvas.fill_rect(SLIDER_X, mid - 1.0, SLIDER_W, 2.0, dim);
                    canvas.fill_rect(SLIDER_X, mid - 1.0, SLIDER_W * v, 2.0, white);
                    canvas.fill_rect(SLIDER_X + SLIDER_W * v - 3.0, mid - 6.0, 6.0, 12.0, white);
                }
                Row::Bind(action) => {
                    let waiting = self.capturing == Some(action);
                    let names = |inputs: &[bindings::Input]| {
                        if inputs.is_empty() {
                            "None".to_owned()
                        } else {
                            inputs.iter().map(|x| x.name()).collect::<Vec<_>>().join(" or ")
                        }
                    };
                    let (k, p) = if waiting {
                        let prompt = "Press a key or button".to_owned();
                        if self.pad_column { (names(b.keys(action)), prompt) } else { (prompt, names(b.pads(action))) }
                    } else {
                        (names(b.keys(action)), names(b.pads(action)))
                    };
                    // The fixed inputs fill an empty cell, dimmed.
                    let cell = |text: &str, x: f32, fixed: Option<&str>| match fixed {
                        Some(f) if text == "None" => font.text(f, x, y + 1.0, 0.4, Align::Left, dim),
                        _ => font.text(text, x, y + 1.0, 0.4, Align::Left, white),
                    };
                    let (key_fixed, pad_fixed) = match action {
                        Action::Up | Action::Down | Action::Left | Action::Right => (None, Some("Left stick")),
                        Action::Pause => (Some("Esc"), None),
                        _ => (None, None),
                    };
                    cell(&k, KEY_COL_X, key_fixed);
                    cell(&p, PAD_COL_X, pad_fixed);
                }
                _ => {}
            }
        }
        let help = match (self.page, self.capturing.is_some()) {
            (_, true) => "Esc cancels",
            (Page::Controls, false) => "Enter or A rebinds   Delete or X clears   Esc or B back",
            (Page::Main, false) => "Enter or A selects   Left and Right adjust   Esc or B back",
        };
        font.text(help, 240.0, CANVAS_H - 12.0, 0.4, Align::Centre, dim);
    }
}

/// The next (or previous) frame-rate cap, wrapping.
fn cycle(fps: u32, up: bool) -> u32 {
    let i = FPS_CAPS.iter().position(|&f| f == fps).unwrap_or(0);
    let n = FPS_CAPS.len();
    FPS_CAPS[if up { (i + 1) % n } else { (i + n - 1) % n }]
}

fn on_off(font: &Font, on: bool, y: f32, color: Color) {
    font.text(if on { "On" } else { "Off" }, VALUE_X, y, ROW_SCALE, Align::Right, color);
}

fn frame(canvas: &Canvas, r: Rect, c: Color) {
    canvas.fill_rect(r.x, r.y, r.w, 1.0, c);
    canvas.fill_rect(r.x, r.y + r.h - 1.0, r.w, 1.0, c);
    canvas.fill_rect(r.x, r.y, 1.0, r.h, c);
    canvas.fill_rect(r.x + r.w - 1.0, r.y, 1.0, r.h, c);
}

fn slider(row: Row, s: &Settings) -> f32 {
    match row {
        Row::Music => s.music,
        Row::Effects => s.effects,
        _ => s.shake,
    }
}

fn set_slider(row: Row, s: &mut Settings, v: f32) {
    match row {
        Row::Music => s.music = v,
        Row::Effects => s.effects = v,
        _ => s.shake = v,
    }
}

/// Installs and saves new bindings.
fn apply(b: Bindings) {
    b.save();
    bindings::install(b);
}
