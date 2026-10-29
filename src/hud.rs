//! The in-game HUD (`Hud`, `Score`, `Counter`, `MultiLabel`, `TextureAtlas`),
//! ported from the iOS binary.
//!
//! Not drawn: the touch wheels and on-screen weapon, bomb and boost buttons
//! (the port uses keyboard and mouse), and the online "Rank" line, which
//! needs the original's long-gone score server.

use macroquad::prelude::*;

use crate::gfx::draw_quad;

/// A bitmap font: `ngage.png` with glyph rectangles from `ngage.dat`
/// (u16 count, then x, y, w, h per glyph).
pub struct Font {
    texture: Texture2D,
    glyphs: Vec<[f32; 4]>,
    /// The atlas is `atlas_scale` times the original glyph metrics.
    atlas_scale: f32,
}

/// MultiLabel::CHARACTER_INDICES: digits, then letters (either case).
fn glyph_index(c: char) -> Option<usize> {
    match c {
        '0'..='9' => Some(c as usize - '0' as usize),
        'A'..='Z' => Some(10 + c as usize - 'A' as usize),
        'a'..='z' => Some(10 + c as usize - 'a' as usize),
        _ => None,
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// MultiLabel: characters without a glyph advance 8 units, numbers use a
/// fixed 22-unit pitch with "1" nudged 5 units right.
const SPACE_WIDTH: f32 = 8.0;
const DIGIT_PITCH: f32 = 22.0;
const ONE_NUDGE: f32 = 5.0;

impl Font {
    pub fn new(texture: Texture2D, dat: &[u8], atlas_scale: f32) -> Font {
        // The glyph atlas is pixel art; nearest keeps it crisp when the
        // canvas is rendered above 480x320. MB_FONT_FILTER=linear opts out.
        let filter = match std::env::var("MB_FONT_FILTER").as_deref() {
            Ok("linear") => FilterMode::Linear,
            _ => FilterMode::Nearest,
        };
        texture.set_filter(filter);
        let count = u16::from_le_bytes([dat[0], dat[1]]) as usize;
        let glyphs = (0..count)
            .map(|i| {
                let at = |k: usize| u16::from_le_bytes([dat[2 + i * 8 + k * 2], dat[3 + i * 8 + k * 2]]) as f32;
                [at(0), at(1), at(2), at(3)]
            })
            .collect();
        Font {
            texture,
            glyphs,
            atlas_scale,
        }
    }

    /// Overrides the atlas filtering, e.g. linear for the pause labels,
    /// which the original rasterised into a low-resolution texture.
    pub fn set_filter(&self, filter: FilterMode) {
        self.texture.set_filter(filter);
    }

    /// TextureAtlas::add: a glyph's quad, top-left at (x, y), scaled.
    fn glyph(&self, index: usize, x: f32, y: f32, scale: f32, color: Color) {
        let [gx, gy, gw, gh] = self.glyphs[index];
        let logical = 1.0 / self.atlas_scale;
        // TextureAtlas::TextureAtlas: texture coordinates stop one texel short.
        let source = Rect::new(gx, gy, gw - 1.0, gh - 1.0);
        draw_texture_ex(
            &self.texture,
            x,
            y,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(gw * logical * scale, gh * logical * scale)),
                source: Some(source),
                ..Default::default()
            },
        );
    }

    fn advance(&self, c: char, scale: f32) -> f32 {
        let logical = 1.0 / self.atlas_scale;
        match glyph_index(c) {
            Some(i) => scale + scale * self.glyphs[i][2] * logical,
            None => scale + scale * SPACE_WIDTH,
        }
    }

    /// MultiLabel::getWidth for a string.
    pub fn width(&self, text: &str, scale: f32) -> f32 {
        text.chars().map(|c| self.advance(c, scale)).sum()
    }

    /// MultiLabel::getWidth for a number.
    pub fn number_width(&self, n: u64, scale: f32) -> f32 {
        let digits = n.to_string().len() as f32;
        scale * (digits * DIGIT_PITCH - 1.0)
    }

    /// MultiLabel::addString.
    pub fn text(&self, text: &str, x: f32, y: f32, scale: f32, align: Align, color: Color) {
        let width = self.width(text, scale) - scale;
        let mut x = match align {
            Align::Left => x,
            Align::Centre => x - width * 0.5,
            Align::Right => x - width,
        };
        for c in text.chars() {
            if let Some(i) = glyph_index(c) {
                self.glyph(i, x, y, scale, color);
            }
            x += self.advance(c, scale);
        }
    }

    /// MultiLabel::addNumber.
    pub fn number(&self, n: u64, x: f32, y: f32, scale: f32, align: Align, color: Color) {
        let digits: Vec<usize> = n.to_string().bytes().map(|b| (b - b'0') as usize).collect();
        let span = digits.len() as f32 * DIGIT_PITCH - 1.0;
        let mut x = match align {
            Align::Left => x,
            Align::Centre => x - scale * span * 0.5,
            Align::Right => x - scale * span,
        };
        for d in digits {
            let nudge = if d == 1 { scale * ONE_NUDGE } else { 0.0 };
            self.glyph(d, x + nudge, y, scale, color);
            x += scale * DIGIT_PITCH;
        }
    }
}

pub struct HudAssets {
    pub font: Font,
    pub ship_counter: Texture2D,
    pub bomb_counter: Texture2D,
    pub ring: Texture2D,
    pub pause: Texture2D,
}

pub struct HudValues {
    pub score: u64,
    pub high_score: u64,
    pub multiplier: u32,
    pub lives: u32,
    pub bombs: u32,
    pub rings: u32,
    /// Tutorial::canShow*: what the tutorial has introduced so far
    /// (everything outside it).
    pub show: Show,
}

#[derive(Clone, Copy)]
pub struct Show {
    pub lives_and_bombs: bool,
    pub score: bool,
    pub rings: bool,
    pub pause: bool,
    pub multiplier: bool,
    pub high_score: bool,
}

impl Show {
    pub const ALL: Show = Show {
        lives_and_bombs: true,
        score: true,
        rings: true,
        pause: true,
        multiplier: true,
        high_score: true,
    };
}

// Hud::Hud / Hud::draw positions on the 480x320 canvas.
const SCORE_POS: Vec2 = vec2(3.0, 3.0);
const LIVES_X: f32 = 229.0;
const BOMBS_X: f32 = 241.0;
const COUNTER_Y: f32 = 3.0;
const COUNTER_STEP: f32 = 11.0;
const MULTIPLIER_X: f32 = 408.0;
const MULTIPLIER_TEN_X: f32 = 386.0;
const MULTIPLIER_Y: f32 = 3.0;
const MULTIPLIER_X_LABEL: Vec2 = vec2(430.0, 8.0);
const MULTIPLIER_X_SCALE: f32 = 0.6;
const HIGH_SCORE_X: f32 = 477.0;
const HIGH_SCORE_LABEL_X: f32 = 475.0;
const SMALL_Y: f32 = 25.0;
const SMALL: f32 = 0.5;
const RINGS_X: f32 = 3.0;
const RING_ICON_GAP: f32 = 6.0;
const RING_ICON_Y: f32 = 25.5;
const PAUSE_POS: Vec2 = vec2(449.0, -1.0);
/// Hud::draw dims the counters to 40% outside the tutorial.
const COUNTER_ALPHA: f32 = 0.4;

/// Counter::draw: up to three icons stepping `dir` x 11 units, else one
/// icon and the count beside it.
fn counter(assets: &HudAssets, icon: &Texture2D, n: u32, x: f32, dir: f32, color: Color) {
    if n < 4 {
        for i in 0..n {
            draw_quad(icon, x + (i as f32 * dir) * -COUNTER_STEP, COUNTER_Y - 1.0, 1.0, 0.0, color);
        }
    } else {
        draw_quad(icon, x, COUNTER_Y - 1.0, 1.0, 0.0, color);
        if dir < 0.0 {
            assets.font.text(&n.to_string(), x + COUNTER_STEP, COUNTER_Y, SMALL, Align::Left, color);
        } else {
            assets.font.number(n as u64, x - 2.0, COUNTER_Y, SMALL, Align::Right, color);
        }
    }
}

/// Hud::draw: everything here is at depth 0 in canvas coordinates.
///
/// The original sets 40% opacity before the life and bomb counters and
/// never resets it before the text, which is batched and drawn last, so
/// the whole HUD is translucent except the pause icon.
pub fn draw(assets: &HudAssets, v: &HudValues) {
    let dim = Color::new(1.0, 1.0, 1.0, COUNTER_ALPHA);
    let show = v.show;
    if show.lives_and_bombs {
        counter(assets, &assets.ship_counter, v.lives, LIVES_X, 1.0, dim);
        counter(assets, &assets.bomb_counter, v.bombs, BOMBS_X, -1.0, dim);
    }

    if show.score {
        assets.font.number(v.score, SCORE_POS.x, SCORE_POS.y, 1.0, Align::Left, dim);
    }

    if show.multiplier {
        if v.multiplier >= 10 {
            assets.font.number(1, MULTIPLIER_TEN_X, MULTIPLIER_Y, 1.0, Align::Left, dim);
            assets.font.number(0, MULTIPLIER_X, MULTIPLIER_Y, 1.0, Align::Left, dim);
        } else {
            assets.font.number(v.multiplier as u64, MULTIPLIER_X, MULTIPLIER_Y, 1.0, Align::Left, dim);
        }
        assets.font.text("x", MULTIPLIER_X_LABEL.x, MULTIPLIER_X_LABEL.y, MULTIPLIER_X_SCALE, Align::Left, dim);
    }

    if show.high_score {
        let high = v.high_score.max(v.score);
        assets.font.number(high, HIGH_SCORE_X, SMALL_Y, SMALL, Align::Right, dim);
        let label_x = HIGH_SCORE_LABEL_X - assets.font.number_width(high, SMALL);
        assets.font.text("High Score ", label_x, SMALL_Y, SMALL, Align::Right, dim);
    }

    if show.rings {
        assets.font.number(v.rings as u64, RINGS_X, SMALL_Y, SMALL, Align::Left, dim);
        let w = assets.font.number_width(v.rings as u64, SMALL);
        draw_quad(&assets.ring, w + RING_ICON_GAP, RING_ICON_Y, 1.0, 0.0, dim);
    }

    if show.pause {
        draw_quad(&assets.pause, PAUSE_POS.x, PAUSE_POS.y, 1.0, 0.0, WHITE);
    }
}
