//! `PausePopup`: the pause menu, options, upgrade shop, quit confirmation
//! and the stage results screens, ported from the iOS binary.
//!
//! Layout, event ids and the fade timings come from `PausePopup` in
//! `original/app/Meteor Blitz`; the button background is the original
//! `button.png` nine-slice and labels use the original `ngage` font.

use macroquad::prelude::*;

use crate::gfx::{asset_scale, draw_quad, Canvas, CANVAS_W};
use crate::hud::{Align, Font};
use crate::pad;
use crate::play::{Play, Upgrade, UPGRADES};

pub struct PauseAssets {
    pub button: Texture2D,
    /// The popup's text: the original `ngage` atlas at 1x with linear
    /// filtering, since the engine drew labels from a low-resolution
    /// rasterised texture rather than the HUD's crisp glyph quads.
    pub label_font: Font,
    pub white: Texture2D,
    /// Slider knob (`Slider::draw` draws the 25x25 peg texture).
    pub peg: Texture2D,
    pub upgrade_bg: Texture2D,
    pub upgrade_icons: [Texture2D; 6],
    pub upgrade_disabled: [Texture2D; 6],
    pub auto_pause: [Texture2D; 2],
    pub auto_upgrade: [Texture2D; 2],
    pub auto_gravity: [Texture2D; 2],
    pub shake: [Texture2D; 3],
    pub boost: [Texture2D; 3],
    pub bomb: [Texture2D; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Controls,
    Upgrade,
    Quit,
    GameOver,
    Clear,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Resume,
    Leave,
    NextStage,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Opening(f32),
    Active,
    PageFade(f32),
    Closing(Action, f32),
}

/// NSUserDefaults settings exposed by the pause popup.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Settings {
    pub music: f32,
    pub effects: f32,
    pub shake: f32,
    /// 0 bomb, 1 boost, 2 off.
    pub shake_control: u8,
    /// 0 tap, 1 left button, 2 right button.
    pub boost_control: u8,
    pub bomb_button: bool,
    pub auto_pause: bool,
    pub auto_gravity: bool,
    pub auto_upgrade: bool,
    pub flip: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            music: 1.0,
            effects: 0.5,
            shake: 0.5,
            shake_control: 0,
            boost_control: 0,
            bomb_button: true,
            auto_pause: true,
            auto_gravity: false,
            auto_upgrade: false,
            flip: false,
        }
    }
}

/// Where `PausePopup::save`/`load` keep the settings (the original used
/// NSUserDefaults, which survives relaunches).
const SETTINGS_PATH: &str = "settings.txt";

impl Settings {
    /// Reads the persisted settings; anything missing or malformed keeps the
    /// default. Mirrors `PausePopup::load`.
    pub fn load() -> Settings {
        match std::fs::read_to_string(SETTINGS_PATH) {
            Ok(text) => Settings::parse(&text),
            // First launch: NSUserDefaults starts from the defaults and keeps
            // them, so persist them once here.
            Err(_) => {
                let settings = Settings::default();
                settings.save();
                settings
            }
        }
    }

    /// Mirrors `PausePopup::save`. On the web build there is no filesystem,
    /// so this silently does nothing and settings last for the session only.
    pub fn save(&self) {
        let _ = std::fs::write(SETTINGS_PATH, self.to_text());
    }

    /// Key=value lines, the same shape as `save.txt`.
    fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "music" => s.music = value.parse().map_or(s.music, |v: f32| v.clamp(0.0, 1.0)),
                "effects" => s.effects = value.parse().map_or(s.effects, |v: f32| v.clamp(0.0, 1.0)),
                "shake" => s.shake = value.parse().map_or(s.shake, |v: f32| v.clamp(0.0, 1.0)),
                "shake_control" => s.shake_control = value.parse().unwrap_or(s.shake_control).min(2),
                "boost_control" => s.boost_control = value.parse().unwrap_or(s.boost_control).min(2),
                "bomb_button" => s.bomb_button = parse_bool(value, s.bomb_button),
                "auto_pause" => s.auto_pause = parse_bool(value, s.auto_pause),
                "auto_gravity" => s.auto_gravity = parse_bool(value, s.auto_gravity),
                "auto_upgrade" => s.auto_upgrade = parse_bool(value, s.auto_upgrade),
                "flip" => s.flip = parse_bool(value, s.flip),
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        format!(
            "music={}\neffects={}\nshake={}\nshake_control={}\nboost_control={}\nbomb_button={}\nauto_pause={}\nauto_gravity={}\nauto_upgrade={}\nflip={}\n",
            self.music,
            self.effects,
            self.shake,
            self.shake_control,
            self.boost_control,
            self.bomb_button as u8,
            self.auto_pause as u8,
            self.auto_gravity as u8,
            self.auto_upgrade as u8,
            self.flip as u8,
        )
    }
}

fn parse_bool(value: &str, fallback: bool) -> bool {
    match value {
        "1" | "true" => true,
        "0" | "false" => false,
        _ => fallback,
    }
}

pub struct Pause {
    page: Page,
    phase: Phase,
    alpha: f32,
    selected: usize,
    /// Results screen's time bonus, counted up at 100/s.
    bonus: f32,
    bonus_max: f32,
    bonus_applied: bool,
    stage_time: f32,
    world: usize,
    stage: usize,
    score: u64,
    multiplier: u32,
    cleared: bool,
    /// The slider the mouse grabbed; it follows the pointer until release.
    dragging: Option<SliderKind>,
}

/// Keyboard/controller step for a selected slider.
const SLIDER_STEP: f32 = 0.05;
/// How far above and below a slider's track a press still grabs it
/// (the 25x25 peg sits on the track).
const SLIDER_GRAB: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum SliderKind {
    Music,
    Effects,
    Shake,
}

#[derive(Clone, Copy)]
enum ItemAction {
    Flip,
    Resume,
    Page(Page),
    Close(Action),
    ClearContinue,
    ToggleShake,
    ToggleBoost,
    ToggleBomb,
    ToggleAutoPause,
    ToggleAutoGravity,
    ToggleAutoUpgrade,
    Buy(Upgrade),
    /// A slider in the selection order: Left/Right adjust it.
    Slider(SliderKind),
}

struct Item {
    rect: Rect,
    action: ItemAction,
    enabled: bool,
}

const OPEN_SECONDS: f32 = 1.0 / 6.0;
const CLOSE_SECONDS: f32 = 1.0 / 6.0;
const PAGE_FADE_SECONDS: f32 = 1.0 / 7.0;

impl Pause {
    pub fn menu() -> Pause {
        Pause {
            page: Page::Main,
            phase: Phase::Opening(0.0),
            alpha: 0.0,
            selected: 0,
            bonus: 0.0,
            bonus_max: 0.0,
            bonus_applied: false,
            stage_time: 0.0,
            world: 0,
            stage: 1,
            score: 0,
            multiplier: 1,
            cleared: false,
            dragging: None,
        }
    }

    pub fn menu_page(page: Page) -> Pause {
        let mut pause = Pause::menu();
        pause.change_page(page);
        pause
    }

    pub fn results(
        cleared: bool,
        world: usize,
        stage: usize,
        score: u64,
        multiplier: u32,
        stage_time: f32,
        bonus_max: f32,
    ) -> Pause {
        Pause {
            page: if cleared { Page::Clear } else { Page::GameOver },
            phase: Phase::Opening(0.0),
            alpha: 0.0,
            selected: 0,
            bonus: 0.0,
            bonus_max,
            bonus_applied: false,
            stage_time,
            world,
            stage,
            score,
            multiplier,
            cleared,
            dragging: None,
        }
    }

    /// `Some(action)` once the fade-out finishes.
    pub fn update(&mut self, dt: f32, play: &mut Play, settings: &mut Settings) -> Option<Action> {
        let dt = dt.min(0.05);
        match self.phase {
            Phase::Opening(t) => {
                let t = t + dt;
                self.alpha = (t / OPEN_SECONDS).min(1.0);
                if t >= OPEN_SECONDS {
                    self.alpha = 1.0;
                    self.phase = Phase::Active;
                } else {
                    self.phase = Phase::Opening(t);
                }
            }
            Phase::PageFade(t) => {
                let t = t + dt;
                self.alpha = (t / PAGE_FADE_SECONDS).min(1.0);
                if t >= PAGE_FADE_SECONDS {
                    self.alpha = 1.0;
                    self.phase = Phase::Active;
                } else {
                    self.phase = Phase::PageFade(t);
                }
            }
            Phase::Closing(action, t) => {
                let t = t + dt;
                self.alpha = 1.0 - (t / CLOSE_SECONDS).min(1.0);
                if t >= CLOSE_SECONDS {
                    return Some(action);
                }
                self.phase = Phase::Closing(action, t);
            }
            Phase::Active => {
                self.alpha = 1.0;
                if self.page == Page::Clear && !self.bonus_applied {
                    self.bonus = (self.bonus + dt * 100.0).min(self.bonus_max);
                }
                self.handle_input(dt, play, settings);
            }
        }
        None
    }

    fn handle_input(&mut self, dt: f32, play: &mut Play, settings: &mut Settings) {
        let items = self.items(play, settings);
        if self.selected >= items.len() {
            self.selected = items.len().saturating_sub(1);
        }

        let nav = pad::nav(dt);
        let vertical_up = is_key_pressed(KeyCode::Up)
            || is_key_pressed(KeyCode::W)
            || pad::pressed(pad::Button::Up)
            || nav == Some(pad::Button::Up);
        let vertical_down = is_key_pressed(KeyCode::Down)
            || is_key_pressed(KeyCode::S)
            || pad::pressed(pad::Button::Down)
            || nav == Some(pad::Button::Down);
        let left = is_key_pressed(KeyCode::Left)
            || is_key_pressed(KeyCode::A)
            || pad::pressed(pad::Button::Left)
            || nav == Some(pad::Button::Left);
        let right = is_key_pressed(KeyCode::Right)
            || is_key_pressed(KeyCode::D)
            || pad::pressed(pad::Button::Right)
            || nav == Some(pad::Button::Right);
        // On a slider, Left/Right adjust it; elsewhere they move the
        // selection like Up/Down.
        let slider = match items.get(self.selected).map(|i| i.action) {
            Some(ItemAction::Slider(kind)) => Some(kind),
            _ => None,
        };
        if let Some(kind) = slider {
            let step = if right { SLIDER_STEP } else if left { -SLIDER_STEP } else { 0.0 };
            if step != 0.0 {
                let v = slider_value(kind, settings);
                set_slider_value(kind, settings, (v + step).clamp(0.0, 1.0));
            }
        }
        let up = vertical_up || (slider.is_none() && left);
        let down = vertical_down || (slider.is_none() && right);
        if up {
            self.selected = (self.selected + items.len() - 1) % items.len().max(1);
        }
        if down {
            self.selected = (self.selected + 1) % items.len().max(1);
        }
        if pad::pressed(pad::Button::A) || is_key_pressed(KeyCode::Enter) {
            if let Some(item) = items.get(self.selected) {
                if item.enabled {
                    self.activate(item.action, play, settings);
                }
            }
        }
        if is_key_pressed(KeyCode::Escape) || pad::pressed(pad::Button::B) {
            self.back(play);
        }

        // Mouse: a press on or near a slider grabs it (as a touch did in
        // the original), and the value follows the pointer until release.
        let mouse = Canvas::mouse();
        if is_mouse_button_pressed(MouseButton::Left) {
            self.dragging = self
                .sliders()
                .into_iter()
                .find(|(_, r)| {
                    mouse.x >= r.x - SLIDER_GRAB
                        && mouse.x <= r.x + r.w + SLIDER_GRAB
                        && (mouse.y - r.y).abs() <= SLIDER_GRAB
                })
                .map(|(kind, _)| kind);
            if self.dragging.is_none() {
                for item in &items {
                    if item.rect.contains(mouse) && item.enabled {
                        self.activate(item.action, play, settings);
                        break;
                    }
                }
            }
        }
        if !is_mouse_button_down(MouseButton::Left) {
            self.dragging = None;
        }
        if let Some(kind) = self.dragging {
            if let Some((_, r)) = self.sliders().into_iter().find(|(k, _)| *k == kind) {
                set_slider_value(kind, settings, ((mouse.x - r.x) / r.w).clamp(0.0, 1.0));
            }
        }
    }

    fn back(&mut self, play: &mut Play) {
        match self.page {
            Page::Main => self.start_closing(Action::Resume),
            Page::Controls | Page::Quit => self.change_page(Page::Main),
            Page::Upgrade => {
                if self.cleared {
                    self.start_closing(Action::NextStage);
                } else {
                    self.change_page(Page::Main);
                }
            }
            Page::GameOver => self.start_closing(Action::Leave),
            Page::Clear => self.clear_continue(play),
        }
    }

    fn change_page(&mut self, page: Page) {
        self.page = page;
        self.alpha = 0.0;
        self.selected = 0;
        self.phase = Phase::PageFade(0.0);
    }

    fn start_closing(&mut self, action: Action) {
        self.phase = Phase::Closing(action, 0.0);
    }

    fn clear_continue(&mut self, play: &mut Play) {
        if !self.bonus_applied {
            play.apply_time_bonus(self.bonus);
            self.score = play.score;
            self.bonus_applied = true;
        }
        self.change_page(Page::Upgrade);
    }

    fn activate(&mut self, action: ItemAction, play: &mut Play, settings: &mut Settings) {
        match action {
            ItemAction::Flip => settings.flip = !settings.flip,
            ItemAction::Resume => self.start_closing(Action::Resume),
            ItemAction::Page(page) => self.change_page(page),
            ItemAction::Close(action) => self.start_closing(action),
            ItemAction::ClearContinue => self.clear_continue(play),
            ItemAction::ToggleShake => settings.shake_control = (settings.shake_control + 1) % 3,
            ItemAction::ToggleBoost => settings.boost_control = (settings.boost_control + 1) % 3,
            ItemAction::ToggleBomb => settings.bomb_button = !settings.bomb_button,
            ItemAction::ToggleAutoPause => settings.auto_pause = !settings.auto_pause,
            ItemAction::ToggleAutoGravity => settings.auto_gravity = !settings.auto_gravity,
            ItemAction::ToggleAutoUpgrade => settings.auto_upgrade = !settings.auto_upgrade,
            ItemAction::Slider(_) => {}
            ItemAction::Buy(upgrade) => {
                if play.buy_upgrade(upgrade) {
                    play.sounds.push(9);
                }
            }
        }
    }

    fn sliders(&self) -> Vec<(SliderKind, Rect)> {
        match self.page {
            Page::Main => vec![
                (SliderKind::Music, Rect::new(170.0, 116.0, 240.0, 1.0)),
                (SliderKind::Effects, Rect::new(170.0, 173.0, 240.0, 1.0)),
            ],
            Page::Controls => vec![(SliderKind::Shake, Rect::new(189.0, 104.0, 167.0, 1.0))],
            _ => Vec::new(),
        }
    }

    fn items(&self, play: &Play, _settings: &Settings) -> Vec<Item> {
        let button = |x, y, w, h, action| Item {
            rect: Rect::new(x, y, w, h),
            action,
            enabled: true,
        };
        // A slider's selection frame spans its track and the peg's height.
        let slider = |x: f32, y: f32, w: f32, kind| Item {
            rect: Rect::new(x - SLIDER_GRAB, y - SLIDER_GRAB, w + 2.0 * SLIDER_GRAB, 2.0 * SLIDER_GRAB),
            action: ItemAction::Slider(kind),
            enabled: true,
        };
        match self.page {
            Page::Main => vec![
                button(30.0, 99.0, 129.0, 41.0, ItemAction::Flip),
                slider(170.0, 116.0, 240.0, SliderKind::Music),
                button(30.0, 155.0, 129.0, 41.0, ItemAction::Page(Page::Controls)),
                slider(170.0, 173.0, 240.0, SliderKind::Effects),
                button(150.0, 230.0, 80.0, 40.0, ItemAction::Resume),
                button(250.0, 230.0, 80.0, 40.0, ItemAction::Page(Page::Quit)),
                button(370.0, 50.0, 100.0, 30.0, ItemAction::Page(Page::Upgrade)),
            ],
            Page::Controls => vec![
                button(30.0, 89.0, 129.0, 41.0, ItemAction::ToggleShake),
                slider(189.0, 104.0, 167.0, SliderKind::Shake),
                button(30.0, 135.0, 129.0, 41.0, ItemAction::ToggleBoost),
                button(30.0, 181.0, 129.0, 41.0, ItemAction::ToggleBomb),
                button(320.0, 135.0, 129.0, 41.0, ItemAction::ToggleAutoPause),
                button(320.0, 181.0, 129.0, 41.0, ItemAction::ToggleAutoGravity),
                button(345.0, 50.0, 125.0, 30.0, ItemAction::Page(Page::Main)),
                button(200.0, 232.0, 80.0, 40.0, ItemAction::Close(Action::Resume)),
            ],
            Page::Upgrade => {
                let slots = [
                    (15.0, 90.0),
                    (170.0, 90.0),
                    (325.0, 90.0),
                    (15.0, 160.0),
                    (170.0, 160.0),
                    (325.0, 160.0),
                ];
                let mut items: Vec<Item> = UPGRADES
                    .iter()
                    .zip(slots)
                    .map(|(upgrade, (x, y))| Item {
                        rect: Rect::new(x, y, 140.0, 64.0),
                        action: ItemAction::Buy(*upgrade),
                        enabled: play.can_buy(*upgrade),
                    })
                    .collect();
                if self.world == 6 {
                    items.push(button(
                        15.0,
                        231.0,
                        129.0,
                        41.0,
                        ItemAction::ToggleAutoUpgrade,
                    ));
                }
                if !self.cleared {
                    items.push(button(
                        370.0,
                        50.0,
                        100.0,
                        30.0,
                        ItemAction::Page(Page::Main),
                    ));
                }
                items.push(button(
                    200.0,
                    232.0,
                    80.0,
                    40.0,
                    ItemAction::Close(if self.cleared {
                        Action::NextStage
                    } else {
                        Action::Resume
                    }),
                ));
                items
            }
            Page::Quit => vec![
                button(150.0, 230.0, 80.0, 40.0, ItemAction::Close(Action::Leave)),
                button(245.0, 230.0, 80.0, 40.0, ItemAction::Page(Page::Main)),
            ],
            Page::GameOver => vec![button(
                180.0,
                215.0,
                120.0,
                40.0,
                ItemAction::Close(Action::Leave),
            )],
            Page::Clear => vec![button(180.0, 230.0, 120.0, 40.0, ItemAction::ClearContinue)],
        }
    }

    pub fn draw(
        &self,
        canvas: &Canvas,
        assets: &PauseAssets,
        play: &Play,
        settings: &Settings,
    ) {
        if self.alpha <= 0.0 {
            return;
        }
        let font = &assets.label_font;
        self.draw_panel(canvas, assets);
        match self.page {
            Page::Main => self.draw_main(canvas, assets, font, settings),
            Page::Controls => self.draw_controls(canvas, assets, font, settings),
            Page::Upgrade => self.draw_upgrade(canvas, assets, font, play, settings),
            Page::Quit => self.draw_quit(canvas, assets, font),
            Page::GameOver => self.draw_game_over(canvas, assets, font),
            Page::Clear => self.draw_clear(canvas, assets, font),
        }
        // Controller/keyboard selection: a thin white frame.
        if self.phase == Phase::Active {
            let items = self.items(play, settings);
            if let Some(item) = items.get(self.selected) {
                let c = Color::new(1.0, 1.0, 1.0, 0.7 * self.alpha);
                rect(
                    canvas,
                    assets,
                    item.rect.x,
                    item.rect.y,
                    item.rect.w,
                    1.0,
                    c,
                );
                rect(
                    canvas,
                    assets,
                    item.rect.x,
                    item.rect.y + item.rect.h - 1.0,
                    item.rect.w,
                    1.0,
                    c,
                );
                rect(
                    canvas,
                    assets,
                    item.rect.x,
                    item.rect.y,
                    1.0,
                    item.rect.h,
                    c,
                );
                rect(
                    canvas,
                    assets,
                    item.rect.x + item.rect.w - 1.0,
                    item.rect.y,
                    1.0,
                    item.rect.h,
                    c,
                );
            }
        }
    }

    /// PausePopup::draw's gradient: a dark panel from y=40 to the bottom,
    /// with 1 px horizontal-gradient rules at y=40 and y=280.
    fn draw_panel(&self, canvas: &Canvas, assets: &PauseAssets) {
        let a = self.alpha * 0.85;
        let top = Color::new(0.2, 0.2, 0.2, a);
        let black = Color::new(0.0, 0.0, 0.0, a);
        gradient(canvas, assets, 0.0, 40.0, CANVAS_W, 120.0, top, black);
        gradient(canvas, assets, 0.0, 160.0, CANVAS_W, 120.0, black, top);
        // The rules use the popup alpha, not the panel's 0.85.
        let dim = Color::new(0.25, 0.25, 0.25, self.alpha);
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        hgradient(canvas, assets, 0.0, 40.0, 240.0, 1.0, dim, white);
        hgradient(canvas, assets, 240.0, 40.0, 240.0, 1.0, white, dim);
        hgradient(canvas, assets, 0.0, 280.0, 240.0, 1.0, dim, white);
        hgradient(canvas, assets, 240.0, 280.0, 240.0, 1.0, white, dim);
    }

    fn draw_main(&self, canvas: &Canvas, assets: &PauseAssets, font: &Font, settings: &Settings) {
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        font.text("Paused", 240.0, 57.0, 1.0, Align::Centre, white);
        self.label(canvas, assets, font, "Flip Screen", 30.0, 99.0, 129.0, 41.0);
        self.label(canvas, assets, font, "Resume", 150.0, 230.0, 80.0, 40.0);
        self.label(canvas, assets, font, "Quit", 250.0, 230.0, 80.0, 40.0);
        self.label(canvas, assets, font, "Upgrade", 370.0, 50.0, 100.0, 30.0);
        self.label(canvas, assets, font, "Controls", 30.0, 155.0, 129.0, 41.0);
        self.slider(canvas, assets, 170.0, 116.0, 240.0, settings.music);
        self.slider(canvas, assets, 170.0, 173.0, 240.0, settings.effects);
        font.text("Music", 190.0, 126.0, 0.5, Align::Left, white);
        font.text("Sound Effects", 190.0, 183.0, 0.5, Align::Left, white);
    }

    fn draw_controls(
        &self,
        canvas: &Canvas,
        assets: &PauseAssets,
        font: &Font,
        settings: &Settings,
    ) {
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        font.text("Controls", 240.0, 57.0, 1.0, Align::Centre, white);
        font.text("Shake Sensitivity", 189.0, 116.0, 0.5, Align::Left, white);
        self.slider(canvas, assets, 189.0, 104.0, 167.0, settings.shake);
        quad(
            canvas,
            assets,
            &assets.shake[settings.shake_control as usize],
            30.0,
            89.0,
            self.alpha,
        );
        quad(
            canvas,
            assets,
            &assets.boost[settings.boost_control as usize],
            30.0,
            135.0,
            self.alpha,
        );
        quad(
            canvas,
            assets,
            &assets.bomb[if settings.bomb_button { 0 } else { 1 }],
            30.0,
            181.0,
            self.alpha,
        );
        quad(
            canvas,
            assets,
            &assets.auto_pause[if settings.auto_pause { 0 } else { 1 }],
            320.0,
            135.0,
            self.alpha,
        );
        quad(
            canvas,
            assets,
            &assets.auto_gravity[if settings.auto_gravity { 0 } else { 1 }],
            320.0,
            181.0,
            self.alpha,
        );
        self.label(canvas, assets, font, "Settings", 345.0, 50.0, 125.0, 30.0);
        self.label(canvas, assets, font, "Done", 200.0, 232.0, 80.0, 40.0);
    }

    fn draw_upgrade(
        &self,
        canvas: &Canvas,
        assets: &PauseAssets,
        font: &Font,
        play: &Play,
        settings: &Settings,
    ) {
        let slots = [
            (15.0, 90.0),
            (170.0, 90.0),
            (325.0, 90.0),
            (15.0, 160.0),
            (170.0, 160.0),
            (325.0, 160.0),
        ];
        for (i, (x, y)) in slots.into_iter().enumerate() {
            let upgrade = UPGRADES[i];
            let can = play.can_buy(upgrade);
            let icon = if can {
                &assets.upgrade_icons[i]
            } else {
                &assets.upgrade_disabled[i]
            };
            quad(canvas, assets, &assets.upgrade_bg, x + 60.0, y, self.alpha);
            quad(canvas, assets, icon, x, y, self.alpha);
            let tint = if can { 1.0 } else { 0.5 };
            let color = Color::new(tint, tint, tint, self.alpha);
            let (label, label_x, label_y, level_y) = match upgrade {
                Upgrade::Cannon => ("Cannon", 83.0, 100.0, 110.0),
                Upgrade::Fire => ("Fire", 246.0, 100.0, 110.0),
                Upgrade::Laser => ("Ice", 402.0, 100.0, 110.0),
                Upgrade::Speed => ("Speed", 89.0, 170.0, 180.0),
                Upgrade::Gravity => ("Grav", 399.0, 170.0, 190.0),
                Upgrade::Boost => ("Boost", 246.0, 170.0, 180.0),
            };
            font.text(label, label_x, label_y, 0.5, Align::Left, color);
            if upgrade == Upgrade::Gravity {
                font.text("Gun", 399.0, 180.0, 0.5, Align::Left, color);
            }
            font.number(
                play.upgrade_level(upgrade) as u64,
                label_x,
                level_y,
                0.5,
                Align::Left,
                color,
            );
            let max = match upgrade {
                Upgrade::Cannon | Upgrade::Fire | Upgrade::Laser => 7,
                _ => 2,
            };
            let pip_y = if y < 120.0 { 125.0 } else { 206.0 };
            self.pips(
                canvas,
                assets,
                x + 80.0,
                pip_y,
                play.upgrade_level(upgrade),
                max,
            );
        }
        if self.world == 6 {
            quad(
                canvas,
                assets,
                &assets.auto_upgrade[if settings.auto_upgrade { 0 } else { 1 }],
                15.0,
                231.0,
                self.alpha,
            );
        }
        if !self.cleared {
            self.label(canvas, assets, font, "Options", 370.0, 50.0, 100.0, 30.0);
        }
        self.label(canvas, assets, font, "Done", 200.0, 232.0, 80.0, 40.0);
    }

    fn draw_quit(&self, canvas: &Canvas, assets: &PauseAssets, font: &Font) {
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        font.text("Are you sure you", 240.0, 100.0, 1.0, Align::Centre, white);
        font.text("want to quit", 240.0, 130.0, 1.0, Align::Centre, white);
        self.label(canvas, assets, font, "Yes", 150.0, 230.0, 80.0, 40.0);
        self.label(canvas, assets, font, "No", 245.0, 230.0, 80.0, 40.0);
    }

    fn draw_game_over(&self, canvas: &Canvas, assets: &PauseAssets, font: &Font) {
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        font.text("Game Over", 240.0, 67.0, 1.0, Align::Centre, white);
        font.text("Final Score", 240.0, 115.0, 1.0, Align::Centre, white);
        font.number(self.score, 240.0, 149.0, 1.5, Align::Centre, white);
        self.label(canvas, assets, font, "Continue", 180.0, 215.0, 120.0, 40.0);
    }

    fn draw_clear(&self, canvas: &Canvas, assets: &PauseAssets, font: &Font) {
        let white = Color::new(1.0, 1.0, 1.0, self.alpha);
        let dim = Color::new(0.8, 0.8, 0.8, self.alpha);
        if self.stage >= 5 {
            font.text("World Complete", 240.0, 55.0, 1.0, Align::Centre, white);
        } else {
            let text = format!("Stage {}", self.stage);
            font.text(&text, 275.0, 55.0, 1.0, Align::Right, white);
            font.text("of", 284.0, 59.0, 0.65, Align::Left, dim);
            font.text("5", 321.0, 55.0, 1.0, Align::Left, white);
        }
        font.text("Goal Time", 20.0, 90.0, 0.8, Align::Left, white);
        font.number(150, 270.0, 90.0, 0.8, Align::Right, white);
        font.text("seconds", 330.0, 90.0, 0.8, Align::Left, white);
        font.text("Your Time", 20.0, 115.0, 0.8, Align::Left, white);
        font.number(
            self.stage_time.round() as u64,
            270.0,
            115.0,
            0.8,
            Align::Right,
            white,
        );
        font.text("Time Bonus", 20.0, 140.0, 0.8, Align::Left, white);
        font.number(
            self.bonus.round() as u64,
            270.0,
            140.0,
            0.8,
            Align::Right,
            white,
        );
        font.text("x", 276.0, 144.0, 0.5, Align::Left, dim);
        font.number(
            self.multiplier as u64,
            291.0,
            140.0,
            0.8,
            Align::Left,
            white,
        );
        let bonus_points = self.bonus.round().max(0.0) as u64 * self.multiplier as u64 * 100;
        font.number(bonus_points, 456.0, 140.0, 0.8, Align::Right, white);
        let total = self.score + bonus_points;
        font.number(total, 240.0, 192.0, 1.5, Align::Centre, white);
        self.label(canvas, assets, font, "Continue", 180.0, 230.0, 120.0, 40.0);
    }

    fn label(
        &self,
        canvas: &Canvas,
        assets: &PauseAssets,
        font: &Font,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        nine_slice(canvas, assets, &assets.button, x, y, w, h, self.alpha);
        let scale = fit_scale(font, text, w, h);
        font.text(
            text,
            x + w * 0.5,
            y + h * 0.5 - 7.5 * scale + label_dy(),
            scale,
            Align::Centre,
            Color::new(1.0, 1.0, 1.0, self.alpha),
        );
    }

    fn slider(&self, canvas: &Canvas, assets: &PauseAssets, x: f32, y: f32, w: f32, value: f32) {
        let a = self.alpha;
        rect(
            canvas,
            assets,
            x,
            y,
            w,
            1.0,
            Color::new(0.54, 0.54, 0.54, a),
        );
        rect(
            canvas,
            assets,
            x,
            y + 1.0,
            w,
            1.0,
            Color::new(0.32, 0.32, 0.32, a),
        );
        rect(
            canvas,
            assets,
            x,
            y + 2.0,
            w,
            1.0,
            Color::new(0.08, 0.08, 0.08, a),
        );
        // Slider::draw: the knob is the 25x25 peg texture at the value,
        // offset by 12 px left and 11 px up.
        draw_texture_ex(
            &assets.peg,
            x + value * w - 12.0,
            y - 11.0,
            Color::new(1.0, 1.0, 1.0, a),
            DrawTextureParams {
                dest_size: Some(vec2(25.0, 25.0)),
                ..Default::default()
            },
        );
    }

    fn pips(
        &self,
        canvas: &Canvas,
        assets: &PauseAssets,
        x: f32,
        y: f32,
        level: usize,
        max: usize,
    ) {
        let a = self.alpha * 0.75;
        let white = Color::new(1.0, 1.0, 1.0, a);
        for i in 0..=max {
            let px = x + (i % 4) as f32 * 12.0;
            let py = y + (i / 4) as f32 * 12.0;
            if i < level + 1 {
                rect(canvas, assets, px, py, 8.0, 8.0, white);
            } else {
                rect(canvas, assets, px, py, 7.0, 1.0, white);
                rect(canvas, assets, px, py + 6.0, 7.0, 1.0, white);
                rect(canvas, assets, px, py, 1.0, 7.0, white);
                rect(canvas, assets, px + 6.0, py, 1.0, 7.0, white);
            }
        }
    }
}

/// The original's `DynamicButton` labels come from a pre-rendered atlas at
/// a fixed size, so the port draws them at one scale; 0.4 matches the
/// emulator pause frame. `MB_LABEL_SCALE` pins it for captures.
fn fit_scale(_font: &Font, _text: &str, _w: f32, _h: f32) -> f32 {
    // Measured against frame_00030.png: the emulator's labels are about
    // 1.35x the previous 0.4 atlas scale, and the glyphs are blurred by the
    // engine's label-texture scaling. A single fixed scale fits every page
    // best (per-label fits overfit our glyph metrics).
    const LABEL_SCALE: f32 = 0.54;
    static FORCED: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    let forced = *FORCED.get_or_init(|| {
        std::env::var("MB_LABEL_SCALE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
    });
    forced.map_or(LABEL_SCALE, |s| s.clamp(0.35, 0.9))
}

/// A vertical nudge for the pause labels. The original draws the label
/// texture at the button's centre; our glyph quads sit about 1 px high, so
/// the measured offset compensates. `MB_LABEL_DY` overrides it.
fn label_dy() -> f32 {
    static DY: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *DY.get_or_init(|| {
        std::env::var("MB_LABEL_DY")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1.0)
    })
}

/// A 26x26 button background drawn as the original `DynamicButton`'s
/// nine-slice: 5 px corners, 16 px edges, and the 16x16 centre stretched
/// between them (source edges start at 4 px, not at the corner).
fn nine_slice(
    _canvas: &Canvas,
    _assets: &PauseAssets,
    tex: &Texture2D,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    alpha: f32,
) {
    let color = Color::new(1.0, 1.0, 1.0, alpha);
    let s = asset_scale();
    let piece = |sx: f32, sy: f32, sw: f32, sh: f32, dx: f32, dy: f32, dw: f32, dh: f32| {
        draw_texture_ex(
            tex,
            dx,
            dy,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(dw, dh)),
                source: Some(Rect::new(sx * s, sy * s, sw * s, sh * s)),
                ..Default::default()
            },
        );
    };
    let c = 5.0;
    let e = 4.0;
    let edge = 16.0;
    piece(0.0, 0.0, c, c, x, y, c, c);
    piece(26.0 - c, 0.0, c, c, x + w - c, y, c, c);
    piece(0.0, 26.0 - c, c, c, x, y + h - c, c, c);
    piece(26.0 - c, 26.0 - c, c, c, x + w - c, y + h - c, c, c);
    piece(c, 0.0, edge, c, x + e, y, w - 9.0, c);
    piece(c, 26.0 - c, edge, c, x + e, y + h - c, w - 9.0, c);
    piece(0.0, c, c, edge, x, y + e, c, h - 9.0);
    piece(26.0 - c, c, c, edge, x + w - c, y + e, c, h - 9.0);
    piece(c, c, edge, edge, x + e, y + e, w - 9.0, h - 9.0);
}

fn quad(canvas: &Canvas, _assets: &PauseAssets, tex: &Texture2D, x: f32, y: f32, alpha: f32) {
    let _ = canvas;
    draw_quad(tex, x, y, 1.0, 0.0, Color::new(1.0, 1.0, 1.0, alpha));
}

fn rect(canvas: &Canvas, assets: &PauseAssets, x: f32, y: f32, w: f32, h: f32, color: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    canvas.draw_triangles(
        vec![
            Vertex::new(x, y, 0.0, 0.0, 0.0, color),
            Vertex::new(x + w, y, 0.0, 1.0, 0.0, color),
            Vertex::new(x + w, y + h, 0.0, 1.0, 1.0, color),
            Vertex::new(x, y + h, 0.0, 0.0, 1.0, color),
        ],
        vec![0, 1, 2, 0, 2, 3],
        Some(&assets.white),
        false,
    );
}

fn gradient(
    canvas: &Canvas,
    assets: &PauseAssets,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    top: Color,
    bottom: Color,
) {
    canvas.draw_triangles(
        vec![
            Vertex::new(x, y, 0.0, 0.0, 0.0, top),
            Vertex::new(x + w, y, 0.0, 1.0, 0.0, top),
            Vertex::new(x + w, y + h, 0.0, 1.0, 1.0, bottom),
            Vertex::new(x, y + h, 0.0, 0.0, 1.0, bottom),
        ],
        vec![0, 1, 2, 0, 2, 3],
        Some(&assets.white),
        false,
    );
}

/// `Primitives::fillHorizontalGradient`, for the panel's 1 px rules.
fn hgradient(
    canvas: &Canvas,
    assets: &PauseAssets,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    left: Color,
    right: Color,
) {
    canvas.draw_triangles(
        vec![
            Vertex::new(x, y, 0.0, 0.0, 0.0, left),
            Vertex::new(x + w, y, 0.0, 1.0, 0.0, right),
            Vertex::new(x + w, y + h, 0.0, 1.0, 1.0, right),
            Vertex::new(x, y + h, 0.0, 0.0, 1.0, left),
        ],
        vec![0, 1, 2, 0, 2, 3],
        Some(&assets.white),
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            music: 0.25,
            effects: 0.75,
            shake: 1.0,
            shake_control: 2,
            boost_control: 1,
            bomb_button: false,
            auto_pause: false,
            auto_gravity: true,
            auto_upgrade: true,
            flip: true,
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }

    #[test]
    fn settings_clamp_and_keep_defaults() {
        let s = Settings::parse("music=5\neffects=-2\nshake=0.3\nbogus=1\n");
        assert_eq!(s.music, 1.0);
        assert_eq!(s.effects, 0.0);
        assert_eq!(s.shake, 0.3);
        assert_eq!(s.auto_pause, Settings::default().auto_pause);
        assert_eq!(Settings::parse(""), Settings::default());
    }
}

fn slider_value(kind: SliderKind, settings: &Settings) -> f32 {
    match kind {
        SliderKind::Music => settings.music,
        SliderKind::Effects => settings.effects,
        SliderKind::Shake => settings.shake,
    }
}

fn set_slider_value(kind: SliderKind, settings: &mut Settings, value: f32) {
    match kind {
        SliderKind::Music => settings.music = value,
        SliderKind::Effects => settings.effects = value,
        SliderKind::Shake => settings.shake = value,
    }
}
