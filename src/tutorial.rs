//! The tutorial mode (`Tutorial`, Global::level 7), ported from the iOS
//! binary.
//!
//! `Tutorial::start` runs it from the main menu's HELP button, and before
//! the first game (world 1 stage 1, or survival) until help has been seen.
//! It walks 27 steps. Each fades in for 0.25 s, waits for its goal (a time,
//! a use of the control it teaches, or enemies it spawned being cleared),
//! fades out for 0.25 s, then the next one starts. Controls and HUD items
//! unlock step by step (`can_use_*`, `can_show_*`). A Quit button ends it
//! at any time. While it runs the ship cannot be destroyed, and rings and
//! thrown boss pieces never expire.
//!
//! The step goals come from the jump table in `Tutorial::update`, and the
//! captions, their anchors and alignment from the one in `Tutorial::draw`
//! (draw case k is step k + 1; step 0 shows nothing).

use macroquad::prelude::*;

use crate::bindings::{self, Action};
use crate::gfx::Canvas;
use crate::hud::{Align, Font};

/// What the game should do for a step that has just been completed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Spawn {
    /// A large rock at the ship + (100, 0), at rest (steps 7, 9 and 11).
    LargeNormalRock,
    LargeIceRock,
    LargeFireRock,
    /// Three free boss pieces around the ship (step 13).
    Balls,
    /// A boss core at the ship + (10, 0) (step 22).
    Boss,
}

/// The live counts a step's goal may wait on.
#[derive(Clone, Copy, Default, Debug)]
pub struct Field {
    pub normal_rocks: usize,
    pub ice_rocks: usize,
    pub fire_rocks: usize,
    pub balls: usize,
    pub rings: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    FadeIn,
    Showing,
    FadeOut,
}

pub struct Tutorial {
    state: State,
    step: u32,
    state_seconds: f32,
    step_seconds: f32,
    using_seconds: f32,
    num_uses: u32,
    has_zero_touches: bool,
    quitting: bool,
}

/// `Tutorial::update`: fades last 0.25 s; the steps end at 27.
const FADE: f32 = 0.25;
const LAST_STEP: u32 = 27;
/// The Quit button (Touch::startedIn and Tutorial::draw).
pub const QUIT_RECT: Rect = Rect { x: 200.0, y: 290.0, w: 80.0, h: 30.0 };

impl Tutorial {
    /// `Tutorial::start`.
    pub fn start() -> Tutorial {
        Tutorial {
            state: State::FadeIn,
            step: 0,
            state_seconds: 0.0,
            step_seconds: 0.0,
            using_seconds: 0.0,
            num_uses: 0,
            has_zero_touches: false,
            quitting: false,
        }
    }

    /// Starts at `step`, for screenshot runs (`MB_TUTORIAL=<step>`).
    pub fn start_at(step: u32) -> Tutorial {
        Tutorial { step: step.min(LAST_STEP), ..Tutorial::start() }
    }

    #[cfg(test)]
    pub fn step(&self) -> u32 {
        self.step
    }

    /// `Tutorial::isComplete`: the last step is done, or Quit was pressed.
    pub fn is_complete(&self) -> bool {
        self.step >= LAST_STEP || self.quitting
    }

    // Tutorial::canUse* / canShow*: controls and HUD items appear in turn.
    pub fn can_move(&self) -> bool {
        self.step >= 2
    }
    pub fn can_boost(&self) -> bool {
        self.step >= 3
    }
    pub fn can_fire(&self) -> bool {
        self.step >= 6
    }
    pub fn can_switch_weapon(&self) -> bool {
        self.step == 9 || self.step >= 11
    }
    pub fn can_bomb(&self) -> bool {
        self.step >= 13
    }
    pub fn can_gravity(&self) -> bool {
        self.step >= 14
    }
    pub fn can_show_lives_and_bombs(&self) -> bool {
        self.step >= 16
    }
    pub fn can_show_score(&self) -> bool {
        self.step >= 17
    }
    pub fn can_show_rings(&self) -> bool {
        self.step >= 19
    }
    pub fn can_pause(&self) -> bool {
        self.step >= 20
    }
    pub fn can_show_multiplier(&self) -> bool {
        self.step >= 21
    }
    pub fn can_show_high_score(&self) -> bool {
        self.step >= 22
    }

    // Tutorial::using*: the steps that teach a control count its use.
    pub fn using_move(&mut self, dt: f32) {
        if self.step == 2 {
            self.using_seconds += dt;
        }
    }
    pub fn using_fire(&mut self, dt: f32) {
        if self.step == 6 || self.step == 7 {
            self.using_seconds += dt;
        }
    }
    pub fn using_boost(&mut self) {
        if self.step == 3 {
            self.num_uses += 1;
        }
    }
    pub fn using_switch_weapon(&mut self) {
        if self.step == 9 || self.step == 11 {
            self.num_uses += 1;
        }
    }
    pub fn using_bomb(&mut self) {
        if self.step == 13 {
            self.num_uses += 1;
        }
    }
    pub fn using_gravity(&mut self) {
        if self.step == 14 || self.step == 15 {
            self.num_uses += 1;
        }
    }
    pub fn using_boss(&mut self) {
        if self.step == 23 {
            self.num_uses += 1;
        }
    }

    /// `Tutorial::update`. `touching` is whether the player is pressing
    /// (the original's touch count), `quit` a press on the Quit button.
    /// Returns what to spawn when a step that sets up enemies completes.
    pub fn update(&mut self, dt: f32, touching: bool, quit: bool, field: Field) -> Option<Spawn> {
        self.state_seconds += dt;
        self.step_seconds += dt;
        match self.state {
            State::FadeIn => {
                if self.state_seconds >= FADE {
                    self.state = State::Showing;
                    self.state_seconds = 0.0;
                }
                None
            }
            State::FadeOut => {
                if self.state_seconds >= FADE {
                    self.state = State::FadeIn;
                    self.step += 1;
                    self.state_seconds = 0.0;
                    self.step_seconds = 0.0;
                    self.using_seconds = 0.0;
                    self.num_uses = 0;
                }
                None
            }
            State::Showing => {
                let mut done = false;
                if self.step < 26 && !self.quitting && quit {
                    self.quitting = true;
                    done = true;
                }
                let (advance, spawn) = self.goal(touching, field);
                if advance || done {
                    self.state = State::FadeOut;
                    self.state_seconds = 0.0;
                    self.has_zero_touches = false;
                    return spawn.filter(|_| advance);
                }
                None
            }
        }
    }

    /// The jump table in `Tutorial::update`.
    fn goal(&mut self, touching: bool, f: Field) -> (bool, Option<Spawn>) {
        let t = self.step_seconds;
        let used = self.using_seconds;
        let once = self.num_uses == 1;
        match self.step {
            0 => (t > 2.0, None),
            // Tap to continue: a fresh press after everything was let go.
            1 => {
                if !self.has_zero_touches {
                    if !touching {
                        self.has_zero_touches = true;
                    }
                    (false, None)
                } else {
                    (touching, None)
                }
            }
            2 => (used > 2.0, None),
            3 | 14 | 23 => (once, None),
            4 | 5 => (t > 3.0, None),
            6 => (used > 1.5, None),
            7 => (used > 2.5, Some(Spawn::LargeNormalRock)),
            8 => (f.normal_rocks == 0, None),
            9 => (once, Some(Spawn::LargeIceRock)),
            10 => (f.ice_rocks == 0, None),
            11 => (once, Some(Spawn::LargeFireRock)),
            12 => (f.fire_rocks == 0, None),
            13 => (once, Some(Spawn::Balls)),
            15 => (f.balls == 0, None),
            16..=20 | 26 => (t > 2.5, None),
            21 => (t > 5.0, None),
            22 => (t > 2.5, Some(Spawn::Boss)),
            24 => (f.rings == 0 && t > 2.5, None),
            25 => (t > 6.0, None),
            _ => (false, None),
        }
    }

    /// The panel's fade: in over 0.25 s, held, out over 0.25 s.
    fn alpha(&self) -> f32 {
        match self.state {
            State::FadeIn => self.state_seconds * 4.0,
            State::Showing => 1.0,
            State::FadeOut => 1.0 - self.state_seconds * 4.0,
        }
        .clamp(0.0, 1.0)
    }

    /// `Tutorial::draw`, at canvas depth 0, over the HUD.
    pub fn draw(&self, canvas: &Canvas, font: &Font) {
        let a = self.alpha();
        if let Some(p) = panel(self.step) {
            let hint = hint(self.step);
            draw_panel(canvas, font, &p, hint.as_deref(), a);
        }
        if self.step <= 25 {
            // The Quit button: a half-black box with "Quit" at 0.7.
            let q = QUIT_RECT;
            canvas.fill_rect(q.x, q.y, q.w, q.h, Color::new(0.0, 0.0, 0.0, 0.5));
            font.text("Quit", 240.0, 300.0, 0.7, Align::Centre, Color::new(1.0, 1.0, 1.0, 0.5));
        }
    }
}

/// A step's caption: anchor, alignment, then large (0.75) and small (0.5)
/// lines.
struct Panel {
    x: f32,
    y: f32,
    align: Align,
    big: &'static [&'static str],
    small: &'static [&'static str],
}

const fn p(x: f32, y: f32, align: Align, big: &'static [&'static str], small: &'static [&'static str]) -> Panel {
    Panel { x, y, align, big, small }
}

/// The jump table in `Tutorial::draw` (case k is step k + 1).
fn panel(step: u32) -> Option<Panel> {
    use Align::{Centre as C, Left as L, Right as R};
    Some(match step {
        1 => p(
            240.0,
            5.0,
            C,
            &["Autosave and instant resume"],
            &[
                "",
                "Game saves automatically when you press",
                "the device home button to quit",
                "",
                "It instantly resumes the next time you play",
                "",
                "Tap to continue",
            ],
        ),
        2 => p(1.0, 140.0, L, &["Hold down", "to move"], &[]),
        3 => p(310.0, 110.0, L, &["Tap to", "boost"], &[]),
        4 => p(310.0, 110.0, L, &[], &["Boost recharges", "after a few", "seconds"]),
        5 => p(310.0, 110.0, L, &[], &["Ship will glow", "green when", "boost is ready"]),
        6 => p(479.0, 140.0, R, &["Hold down", "to fire"], &[]),
        7 => p(479.0, 120.0, R, &[], &["Fire in any", "direction by", "dragging your thumb"]),
        8 => p(240.0, 5.0, C, &["Destroy the meteor"], &[]),
        9 => p(479.0, 95.0, R, &[], &["Press to", "switch weapons"]),
        10 => p(240.0, 5.0, C, &["Use flamethrower", "against ice meteors"], &["", "Destroy the meteor"]),
        11 => p(479.0, 95.0, R, &[], &["Press again to", "switch weapons"]),
        12 => p(240.0, 5.0, C, &["Use ice bullets", "against fiery meteors"], &["", "Destroy the meteor"]),
        13 => p(210.0, 190.0, L, &["Press to bomb"], &["or shake while", "touching the screen"]),
        14 => p(
            479.0,
            87.0,
            R,
            &[],
            &["Hold to activate", "gravity gun", "", "Drag to capture", "enemy then", "release to throw"],
        ),
        15 => p(
            240.0,
            5.0,
            C,
            &[],
            &[
                "Upgrade gravity gun to",
                "capture larger enemies",
                "",
                "Use the gravity gun on the",
                "remaining enemies to continue",
            ],
        ),
        16 => p(240.0, 15.0, C, &["Lives and bombs", "remaining"], &[]),
        17 => p(1.0, 22.0, L, &["Score"], &[]),
        18 => p(1.0, 38.0, L, &["Global rank"], &[]),
        19 => p(1.0, 50.0, L, &["Number", "of rings"], &[]),
        20 => p(479.0, 22.0, R, &["Pause", "button"], &[]),
        21 => p(479.0, 22.0, R, &["Score multiplier"], &[]),
        22 => p(479.0, 38.0, R, &["High score"], &[]),
        23 => p(240.0, 15.0, C, &["Destroy boss"], &["Destroy the boss to", "complete the stage"]),
        24 => p(240.0, 15.0, C, &["Collect rings"], &["Use rings to", "purchase upgrades"]),
        25 => p(
            240.0,
            45.0,
            C,
            &["iPod music"],
            &["Listen to your own music by turning", "it on before starting the game"],
        ),
        26 => p(240.0, 45.0, C, &["Tutorial complete"], &[]),
        _ => return None,
    })
}

/// Line pitch and scale of the large and small captions.
const BIG_SCALE: f32 = 0.75;
const BIG_PITCH: f32 = 20.0;
const SMALL_SCALE: f32 = 0.5;
const SMALL_PITCH: f32 = 14.0;

/// `Tutorial::draw`'s shared tail: a 75% black box sized to the widest line
/// plus 20, every line centred in it from 10 below its top.
fn draw_panel(canvas: &Canvas, font: &Font, p: &Panel, hint: Option<&str>, alpha: f32) {
    // Desktop key hint (not in the original): a small line under the text.
    let small: Vec<&str> = match hint {
        Some(h) => p.small.iter().copied().chain(["", h]).collect(),
        None => p.small.to_vec(),
    };
    let mut width: f32 = 0.0;
    for line in p.big {
        width = width.max(font.width(line, BIG_SCALE));
    }
    for line in &small {
        width = width.max(font.width(line, SMALL_SCALE));
    }
    let mut height = BIG_PITCH * p.big.len() as f32 + SMALL_PITCH * small.len() as f32;
    if !small.is_empty() {
        height += 3.0;
    }
    let box_w = width + 20.0;
    // Panels at the screen's sides follow its edges in a wide view, as the
    // HUD items they point at do.
    let edge = crate::gfx::edge();
    let anchor = if p.x <= 1.0 {
        p.x - edge
    } else if p.x >= 479.0 {
        p.x + edge
    } else {
        p.x
    };
    let x = match p.align {
        Align::Left => anchor,
        Align::Centre => anchor - box_w * 0.5,
        Align::Right => anchor - box_w,
    };
    // A hint can widen a box past the screen edge; keep it on screen.
    let x = if hint.is_some() { x.clamp(-edge, (480.0 + edge - box_w).max(-edge)) } else { x };
    canvas.fill_rect(x, p.y, box_w, height + 10.0, Color::new(0.0, 0.0, 0.0, 0.75 * alpha));
    let centre = x + box_w * 0.5;
    let white_text = Color::new(1.0, 1.0, 1.0, alpha);
    let mut y = p.y + 10.0;
    for line in p.big {
        font.text(line, centre, y, BIG_SCALE, Align::Centre, white_text);
        y += BIG_PITCH;
    }
    for line in &small {
        font.text(line, centre, y, SMALL_SCALE, Align::Centre, white_text);
        y += SMALL_PITCH;
    }
}

/// Desktop hint for a step that teaches a control: its current bindings
/// (the captions describe the original's touch controls).
fn hint(step: u32) -> Option<String> {
    let b = bindings::current();
    Some(match step {
        1 => "Click or press any key or button".to_owned(),
        2 => {
            let keys: Vec<String> = [Action::Up, Action::Left, Action::Down, Action::Right]
                .iter()
                .filter_map(|&a| b.keys(a).first().map(|k| k.name()))
                .collect();
            format!("{} or left stick", keys.join(" "))
        }
        3 => b.describe(Action::Boost),
        6 | 7 => format!("{} or right stick", b.describe(Action::Fire)),
        9 | 11 => b.describe(Action::SwitchWeapon),
        13 => b.describe(Action::Bomb),
        14 if bindings::uses_mouse_left(Action::Fire) => {
            format!("Double click or {}", b.describe(Action::Gravity))
        }
        14 => b.describe(Action::Gravity),
        20 => format!("Esc or {}", b.describe(Action::Pause)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(t: &mut Tutorial, seconds: f32, touching: bool, field: Field) -> Vec<Spawn> {
        let mut spawns = Vec::new();
        for _ in 0..(seconds / DT) as usize {
            if let Some(s) = t.update(DT, touching, false, field) {
                spawns.push(s);
            }
        }
        spawns
    }

    #[test]
    fn walks_every_step_with_the_right_actions() {
        let mut t = Tutorial::start();
        let clear = Field::default();
        run(&mut t, 2.6, false, clear);
        assert_eq!(t.step(), 1, "step 0 lasts 2 s");
        run(&mut t, 0.5, false, clear);
        run(&mut t, 0.6, true, clear);
        assert_eq!(t.step(), 2, "a tap after letting go continues");
        assert!(t.can_move() && !t.can_boost());
        for _ in 0..150 {
            t.using_move(DT);
            t.update(DT, false, false, clear);
        }
        assert_eq!(t.step(), 3);
        run(&mut t, 0.3, false, clear);
        t.using_boost();
        run(&mut t, 0.6, false, clear);
        assert_eq!(t.step(), 4);
        run(&mut t, 7.0, false, clear);
        assert_eq!(t.step(), 6, "steps 4 and 5 last 3 s");
        let mut spawn = None;
        for _ in 0..600 {
            t.using_fire(DT);
            spawn = t.update(DT, false, false, clear);
            if spawn.is_some() {
                break;
            }
        }
        assert_eq!(spawn, Some(Spawn::LargeNormalRock), "step 7 sets up a rock");
        let rock = Field { normal_rocks: 1, ..clear };
        run(&mut t, 0.5, false, rock);
        assert_eq!(t.step(), 8);
        run(&mut t, 1.0, false, rock);
        assert_eq!(t.step(), 8, "waits for the rock");
        run(&mut t, 0.6, false, clear);
        assert_eq!(t.step(), 9);
        assert!(t.can_switch_weapon() && !t.can_bomb());
    }

    #[test]
    fn quit_ends_it() {
        let mut t = Tutorial::start();
        run(&mut t, 0.3, false, Field::default());
        t.update(DT, false, true, Field::default());
        assert!(t.is_complete());
    }
}
