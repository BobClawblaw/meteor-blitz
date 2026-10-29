//! `Tutorial`: the one-off captions shown when a HUD element first appears.
//!
//! The original gated them with `Tutorial::canShowHighScore`,
//! `canShowMultiplier`, `canShowNumBombs` and `canShowNumLives`, and drew
//! `captions.png` over the game. Each hint fires once per session.

use macroquad::prelude::*;

use crate::gfx::{Canvas, CANVAS_W};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    HighScore,
    Multiplier,
    Bombs,
    Lives,
}

pub struct Tutorial {
    shown: [bool; 4],
    active: Option<Hint>,
    timer: f32,
}

/// `captions.png` size, and how long a hint stays up with its fades.
const CAPTION_W: f32 = 125.0;
const CAPTION_H: f32 = 112.0;
const HINT_TIME: f32 = 5.0;
const FADE: f32 = 0.4;

impl Tutorial {
    pub fn new() -> Tutorial {
        Tutorial { shown: [false; 4], active: None, timer: 0.0 }
    }

    /// One hint at a time, first come first served, from the live HUD values.
    pub fn update(
        &mut self,
        dt: f32,
        score: u64,
        multiplier: u32,
        bombs: u32,
        lives: u32,
        initial_lives: u32,
        initial_bombs: u32,
    ) {
        if self.active.is_none() {
            let candidates = [
                (Hint::HighScore, score > 0),
                (Hint::Multiplier, multiplier > 1),
                (Hint::Bombs, bombs < initial_bombs),
                (Hint::Lives, lives < initial_lives),
            ];
            if let Some((hint, _)) = candidates.into_iter().find(|(hint, on)| *on && !self.shown[*hint as usize]) {
                self.shown[hint as usize] = true;
                self.active = Some(hint);
                self.timer = 0.0;
            }
        }
        if self.active.is_some() {
            self.timer += dt;
            if self.timer >= HINT_TIME {
                self.active = None;
            }
        }
    }

    /// MB_TUTORIAL=1 keeps a hint up from the start, for captures.
    pub fn force(&mut self) {
        self.active = Some(Hint::HighScore);
        self.timer = HINT_TIME * 0.5;
    }

    pub fn draw(&self, _canvas: &Canvas, captions: &Texture2D) {
        if self.active.is_none() {
            return;
        }
        let alpha = if self.timer < FADE {
            self.timer / FADE
        } else if self.timer > HINT_TIME - FADE {
            (HINT_TIME - self.timer) / FADE
        } else {
            1.0
        };
        draw_texture(
            captions,
            ((CANVAS_W - CAPTION_W) * 0.5).floor(),
            ((320.0 - CAPTION_H) * 0.5).floor(),
            Color::new(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_fire_once_each() {
        let mut t = Tutorial::new();
        t.update(0.1, 10, 1, 3, 3, 3, 3);
        assert_eq!(t.active, Some(Hint::HighScore));
        t.update(5.0, 10, 1, 3, 3, 3, 3);
        assert!(t.active.is_none());
        t.update(0.1, 20, 1, 3, 3, 3, 3);
        assert!(t.active.is_none(), "same hint must not repeat");
        t.update(0.1, 20, 2, 3, 3, 3, 3);
        assert_eq!(t.active, Some(Hint::Multiplier));
        t.update(5.0, 20, 2, 3, 3, 3, 3);
        t.update(0.1, 20, 2, 2, 3, 3, 3);
        assert_eq!(t.active, Some(Hint::Bombs));
        t.update(5.0, 20, 2, 2, 3, 3, 3);
        t.update(0.1, 20, 2, 2, 2, 3, 3);
        assert_eq!(t.active, Some(Hint::Lives));
    }
}
