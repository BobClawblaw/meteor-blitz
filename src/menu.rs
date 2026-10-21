//! Original-layout menus (`MainMenuScreen`, `PlayScreen`, `LevelSelectScreen`,
//! `StageSelectScreen`), drawn through the original camera in `gfx`.

use macroquad::prelude::*;

use crate::pad::{self, Button};

use crate::gfx::{draw_quad, Canvas, CANVAS_W};

// BasicScreen: backdrop mesh origin and depth, horizon scale.
const BACKDROP_X: f32 = 240.0;
const BACKDROP_Y: f32 = 155.0;
const BACKDROP_Z: f32 = 100.0;
const HORIZON_SCALE: f32 = 1.25;

// BasicScreen::update: screens fade in at 6/s while flying from z=150 to 0
// at 900/s, fade out at -6/s while flying on; the backdrop scrolls 50/s.
const SCREEN_START_Z: f32 = 150.0;
const SCREEN_Z_SPEED: f32 = -900.0;
const FADE_IN_SPEED: f32 = 6.0;
const FADE_OUT_SPEED: f32 = -6.0;
const BACKDROP_SCROLL: f32 = 50.0;

pub struct MenuAssets {
    pub space: Texture2D,
    pub fog: Texture2D,
    pub horizon: Texture2D,
}

/// The shared menu backdrop's scroll (`BasicScreen::update` drifts it 50/s).
pub struct MenuBackdrop {
    scroll: Vec2,
    /// MB_BG_Z, read once: env lookups take the process environment lock.
    bg_z: Option<f32>,
}

impl MenuBackdrop {
    pub fn new() -> MenuBackdrop {
        // MB_SCROLL=<units> starts the scroll at a given phase, for matching
        // an emulator frame taken at an unknown time.
        let x = std::env::var("MB_SCROLL").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let bg_z = std::env::var("MB_BG_Z").ok().and_then(|v| v.parse().ok());
        MenuBackdrop {
            scroll: vec2(x, 0.0),
            bg_z,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.scroll.x += dt * BACKDROP_SCROLL;
    }

    /// `BasicScreen::draw` up to the screen's own content: the curved
    /// backdrop at (240, 155, 100), then the horizon at the origin, scale 1.25.
    fn draw(&self, canvas: &Canvas, assets: &MenuAssets, alpha: f32) {
        // MB_BG_Z overrides the backdrop depth while it is being verified.
        let z = self.bg_z.unwrap_or(BACKDROP_Z);
        canvas.draw_backdrop(true, &assets.space, &assets.fog, vec3(BACKDROP_X, BACKDROP_Y, z), self.scroll, alpha);
        draw_quad(&assets.horizon, 0.0, 0.0, HORIZON_SCALE, 0.0, Color::new(1.0, 1.0, 1.0, alpha));
    }
}

/// `Button`: an up/down texture pair, a rect, and the event it raises.
pub struct MenuButton {
    pub up: Texture2D,
    pub down: Texture2D,
    pub rect: Rect,
    pub event: u8,
}

/// A `BasicScreen` with a centred label and buttons (the main menu and the
/// mode, world and stage selections all have this shape).
pub struct MenuScreen {
    pub label: Texture2D,
    pub label_y: f32,
    pub buttons: Vec<MenuButton>,
    pub fx: ScreenFx,
    /// Index of the button whose down frame is showing (Button::update
    /// advances the frame on press and only resets it on re-entry).
    pressed: Option<usize>,
    /// The button a controller has selected (shown pressed). The original
    /// had no such thing: it was touch-only.
    focus: Option<usize>,
    /// The left stick was pushed last frame (it steps once per push).
    stick_held: bool,
}

impl MenuScreen {
    pub fn new(label: Texture2D, label_y: f32, buttons: Vec<MenuButton>, fade_backdrop: bool) -> MenuScreen {
        MenuScreen {
            label,
            label_y,
            buttons,
            fx: ScreenFx::enter(fade_backdrop),
            pressed: None,
            focus: None,
            stick_held: false,
        }
    }

    /// Advances the transition and returns the event of a button pressed
    /// this frame. The original fires on touch start, not release.
    pub fn update(&mut self, dt: f32) -> Option<u8> {
        self.fx.update(dt, is_mouse_button_down(MouseButton::Left) || pad::down(Button::A));
        if !self.fx.ready() {
            return None;
        }
        if let Some(event) = self.pad_update() {
            return Some(event);
        }
        if !is_mouse_button_pressed(MouseButton::Left) {
            return None;
        }
        let mouse = Canvas::mouse();
        let hit = self.buttons.iter().position(|b| b.rect.contains(mouse))?;
        self.pressed = Some(hit);
        Some(self.buttons[hit].event)
    }

    /// Controller: the D-pad or left stick moves the selection to the
    /// nearest button that way; A presses it.
    fn pad_update(&mut self) -> Option<u8> {
        let stick = pad::left();
        let pushed = stick.length() > 0.6;
        let mut dir = Vec2::ZERO;
        if pushed && !self.stick_held {
            dir = if stick.x.abs() > stick.y.abs() { vec2(stick.x.signum(), 0.0) } else { vec2(0.0, stick.y.signum()) };
        }
        self.stick_held = pushed;
        for (b, d) in [(Button::Up, vec2(0.0, -1.0)), (Button::Down, vec2(0.0, 1.0)), (Button::Left, vec2(-1.0, 0.0)), (Button::Right, vec2(1.0, 0.0))] {
            if pad::pressed(b) {
                dir = d;
            }
        }
        if self.buttons.is_empty() {
            return None;
        }
        if dir != Vec2::ZERO {
            match self.focus {
                None => self.focus = Some(0),
                Some(f) => {
                    let from = self.buttons[f].rect.center();
                    let next = (0..self.buttons.len())
                        .filter(|&i| i != f)
                        .filter_map(|i| {
                            let d = self.buttons[i].rect.center() - from;
                            let along = d.dot(dir);
                            (along > 1.0).then(|| (i, along + 3.0 * (d.length_squared() - along * along).max(0.0).sqrt()))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((i, _)) = next {
                        self.focus = Some(i);
                    }
                }
            }
        }
        if pad::pressed(Button::A) {
            let f = *self.focus.get_or_insert(0);
            self.pressed = Some(f);
            return Some(self.buttons[f].event);
        }
        None
    }

    pub fn draw(&self, canvas: &Canvas, assets: &MenuAssets, backdrop: &MenuBackdrop) {
        backdrop.draw(canvas, assets, self.fx.backdrop_alpha());
        let color = self.fx.content_color();
        draw_quad(&self.label, ((CANVAS_W - self.label.width()) * 0.5).floor(), self.label_y, 1.0, self.fx.z, color);
        for (i, b) in self.buttons.iter().enumerate() {
            let lit = self.pressed == Some(i) || (self.pressed.is_none() && self.focus == Some(i));
            let tex = if lit { &b.down } else { &b.up };
            draw_quad(tex, b.rect.x, b.rect.y, 1.0, self.fx.z, color);
        }
    }
}

/// `BasicScreen` state: the fade/fly-in on entry and fade-out on exit.
pub struct ScreenFx {
    state: u8,
    pub alpha: f32,
    pub z: f32,
    /// Fade the backdrop in with the content (true when coming from the
    /// splash or the game; menu-to-menu keeps the backdrop solid).
    fade_backdrop: bool,
}

impl ScreenFx {
    pub fn enter(fade_backdrop: bool) -> ScreenFx {
        ScreenFx { state: 1, alpha: 0.0, z: SCREEN_START_Z, fade_backdrop }
    }

    pub fn update(&mut self, dt: f32, pointer_down: bool) {
        match self.state {
            1 => {
                self.alpha += dt * FADE_IN_SPEED;
                self.z = (self.z + dt * SCREEN_Z_SPEED).max(0.0);
                if self.alpha >= 1.0 {
                    self.alpha = 1.0;
                    self.z = 0.0;
                    self.state = 2;
                }
            }
            2 => {
                if !pointer_down {
                    self.state = 3;
                }
            }
            7 => {
                self.z += dt * SCREEN_Z_SPEED;
                self.alpha += dt * FADE_OUT_SPEED;
                if self.alpha <= 0.0 {
                    self.alpha = 0.0;
                    self.state = 0;
                }
            }
            _ => {}
        }
    }

    /// Buttons only respond in the ready state.
    pub fn ready(&self) -> bool {
        self.state == 3
    }

    pub fn leave(&mut self) {
        if self.state == 3 {
            self.state = 7;
        }
    }

    pub fn done(&self) -> bool {
        self.state == 0
    }

    fn backdrop_alpha(&self) -> f32 {
        if self.fade_backdrop {
            self.alpha
        } else {
            1.0
        }
    }

    fn content_color(&self) -> Color {
        Color::new(1.0, 1.0, 1.0, self.alpha)
    }
}
