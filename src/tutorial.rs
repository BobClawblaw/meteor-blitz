//! `Tutorial`: the captions panel (`captions.png`) the original draws over
//! the game.
//!
//! `Tutorial::draw` returns at once unless `Tutorial::active` is set, which
//! only the tutorial mode does; in arcade and survival play the panel never
//! shows. (`canShowHighScore` and the like hide HUD elements during the
//! tutorial's early steps; they do not raise captions.) The tutorial mode is
//! not ported yet, so the panel appears only when forced for captures.

use macroquad::prelude::*;

use crate::gfx::{draw_quad, Canvas, CANVAS_W};

pub struct Tutorial {
    active: bool,
    timer: f32,
}

/// `captions.png` size, and how long a forced panel stays up with its fades.
const CAPTION_W: f32 = 125.0;
const CAPTION_H: f32 = 112.0;
const HINT_TIME: f32 = 5.0;
const FADE: f32 = 0.4;

impl Tutorial {
    pub fn new() -> Tutorial {
        Tutorial { active: false, timer: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        if self.active {
            self.timer += dt;
            if self.timer >= HINT_TIME {
                self.active = false;
            }
        }
    }

    /// MB_TUTORIAL=1 shows the panel from the start, for captures.
    pub fn force(&mut self) {
        self.active = true;
        self.timer = HINT_TIME * 0.5;
    }

    pub fn draw(&self, _canvas: &Canvas, captions: &Texture2D) {
        if !self.active {
            return;
        }
        let alpha = if self.timer < FADE {
            self.timer / FADE
        } else if self.timer > HINT_TIME - FADE {
            (HINT_TIME - self.timer) / FADE
        } else {
            1.0
        };
        draw_quad(
            captions,
            ((CANVAS_W - CAPTION_W) * 0.5).floor(),
            ((320.0 - CAPTION_H) * 0.5).floor(),
            1.0,
            0.0,
            Color::new(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arcade play never raises the panel, whatever happens on the HUD.
    #[test]
    fn no_captions_in_normal_play() {
        let mut t = Tutorial::new();
        for _ in 0..600 {
            t.update(1.0 / 60.0);
            assert!(!t.active);
        }
    }

    #[test]
    fn a_forced_panel_times_out() {
        let mut t = Tutorial::new();
        t.force();
        assert!(t.active);
        t.update(HINT_TIME);
        assert!(!t.active);
    }
}
