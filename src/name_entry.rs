//! `labelEnterName`: the name step after a qualifying run.
//!
//! The original showed an on-screen keyboard; the desktop port reads the
//! keyboard directly. The `ngage` font only has digits and letters, so the
//! field is limited to the alphabet `LocalScores` stores.

use macroquad::prelude::*;

use crate::gfx::{draw_quad, tex_size, Canvas, CANVAS_W};
use crate::hud::{Align, Font};
use crate::menu::{MenuAssets, MenuBackdrop};

pub struct NameEntry {
    pub name: String,
    pub score: u64,
}

impl NameEntry {
    pub fn new(score: u64) -> NameEntry {
        // Keys pressed while the window was opening would otherwise land in
        // the field, and a stray Enter would confirm it.
        while get_char_pressed().is_some() {}
        NameEntry { name: String::new(), score }
    }

    /// Returns `true` once the player confirms with Enter, or skips with
    /// Escape (the table then files the default name).
    pub fn update(&mut self) -> bool {
        while let Some(c) = get_char_pressed() {
            if c.is_ascii_alphanumeric() && self.name.len() < 8 {
                self.name.push(c.to_ascii_uppercase());
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.name.pop();
        }
        crate::pad::enter_pressed() || is_key_pressed(KeyCode::Escape)
    }

    pub fn draw(
        &self,
        canvas: &Canvas,
        assets: &MenuAssets,
        backdrop: &MenuBackdrop,
        label: &Texture2D,
        font: &Font,
    ) {
        backdrop.draw(canvas, assets, 1.0);
        let (label_w, _) = tex_size(label);
        draw_quad(
            label,
            ((CANVAS_W - label_w) * 0.5).floor(),
            70.0,
            1.0,
            0.0,
            WHITE,
        );
        font.text(&self.name, 240.0, 160.0, 3.0, Align::Centre, WHITE);
        // The font has no underscore or punctuation, so the caret is a quad.
        if (get_time() * 2.0) as u64 % 2 == 0 {
            let caret_x = 240.0 + font.width(&self.name, 3.0) * 0.5 + 3.0;
            draw_rectangle(caret_x, 140.0, 3.0, 22.0, WHITE);
        }
        font.text("ENTER", 240.0, 220.0, 2.0, Align::Centre, WHITE);
        font.number(self.score, 240.0, 260.0, 2.0, Align::Centre, WHITE);
    }
}
