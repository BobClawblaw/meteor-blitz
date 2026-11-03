//! Touch controls (port addition, for the web build on phones and tablets
//! and any touch screen): the original's two wheels as floating sticks, the
//! left one moving the ship and the right one aiming and firing, with bomb
//! and boost buttons placed by the original's Boost and Bomb options.
//!
//! A thumb landing in the left or right half below the HUD band takes that
//! side's stick, centred where it landed; the stick's base follows a thumb
//! that runs past its rim. A second touch on the aim side within 0.3 s of
//! the last reaches out with the gravity gun (`GameScreen::updateStateReady`'s
//! double press), and lifting the thumb throws. Boost "Tap" boosts on a quick
//! tap on the move side. The controls show once the screen is touched and
//! hide again on a key, mouse movement or a controller.

use crate::gfx::{edge, CANVAS_H, CANVAS_W, Canvas};
use crate::hud::{Align, Font};
use macroquad::prelude::*;
use std::cell::RefCell;

/// Stick radius (canvas units): full deflection.
const RADIUS: f32 = 34.0;
/// Below this share of the radius the move stick is at rest.
const MOVE_DEAD: f32 = 0.1;
/// The aim stick fires once pushed past this share of the radius.
const AIM_DEAD: f32 = 0.2;
/// Touches above this line belong to the HUD (the pause icon).
const HUD_BAND: f32 = 44.0;
/// Button radius (canvas units); touches hit a little outside it.
const BUTTON: f32 = 20.0;
const BUTTON_HIT: f32 = 1.3;
/// GameScreen::updateStateReady: the double press for the gravity gun.
const DOUBLE_TAP: f32 = 0.3;
/// Boost "Tap": a touch this short that moved this little.
const TAP_TIME: f32 = 0.2;
const TAP_SLOP: f32 = 8.0;
/// Where the sticks rest when no thumb holds them, from the bottom corners.
const REST_INSET: Vec2 = vec2(62.0, 62.0);

/// Settings > Boost control and Bomb button (`PausePopup`'s settings page).
#[derive(Clone, Copy, PartialEq)]
pub struct Layout {
    /// 0 Tap, 1 button on the left, 2 button on the right.
    pub boost: u8,
    pub bomb: bool,
}

#[derive(Clone, Copy, Default)]
struct Stick {
    id: Option<u64>,
    base: Vec2,
    pos: Vec2,
    held: f32,
    travel: f32,
    /// The last aim past the dead zone, kept while the thumb holds.
    dir: Option<Vec2>,
}

impl Stick {
    fn grab(&mut self, id: u64, p: Vec2) {
        *self = Stick { id: Some(id), base: p, pos: p, ..Stick::default() };
    }

    fn drag(&mut self, p: Vec2) {
        self.pos = p;
        let d = p - self.base;
        if d.length() > RADIUS {
            self.base = p - d.normalize() * RADIUS;
        }
        self.travel = self.travel.max((p - self.base).length());
        if d.length() > AIM_DEAD * RADIUS {
            self.dir = Some(d.normalize());
        }
    }

    /// Deflection, length 0 to 1.
    fn value(&self) -> Vec2 {
        if self.id.is_none() {
            return Vec2::ZERO;
        }
        ((self.pos - self.base) / RADIUS).clamp_length_max(1.0)
    }
}

#[derive(Default)]
struct State {
    shown: bool,
    last_mouse: Vec2,
    layout: Option<Layout>,
    sticks: [Stick; 2],
    /// Touches holding the bomb and boost buttons, for their highlight.
    bomb_id: Option<u64>,
    boost_id: Option<u64>,
    since_aim: f32,
    /// Presses waiting for the next simulation step.
    bomb: bool,
    boost: bool,
    gun: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn rest(side: usize) -> Vec2 {
    if side == 0 {
        vec2(-edge() + REST_INSET.x, CANVAS_H - REST_INSET.y)
    } else {
        vec2(CANVAS_W + edge() - REST_INSET.x, CANVAS_H - REST_INSET.y)
    }
}

/// Centres of the bomb and boost buttons for a layout.
fn buttons(layout: Layout) -> (Option<Vec2>, Option<Vec2>) {
    let right = CANVAS_W + edge() - 40.0;
    let left = -edge() + 40.0;
    let bomb = layout.bomb.then(|| vec2(right, 170.0));
    let boost = match layout.boost {
        1 => Some(vec2(left, 170.0)),
        2 => Some(vec2(right, if layout.bomb { 118.0 } else { 170.0 })),
        _ => None,
    };
    (bomb, boost)
}

fn hit(centre: Option<Vec2>, p: Vec2) -> bool {
    centre.is_some_and(|c| c.distance(p) < BUTTON * BUTTON_HIT)
}

/// Reads this frame's touches. `layout` is `Some` while a game is running
/// unpaused; otherwise the sticks let go and pending presses are dropped.
pub fn poll(dt: f32, layout: Option<Layout>) {
    let touches = touches();
    let (mx, my) = mouse_position();
    let mouse = vec2(mx, my);
    STATE.with_borrow_mut(|s| {
        if !touches.is_empty() {
            s.shown = true;
            s.last_mouse = mouse;
        } else if mouse != s.last_mouse
            || !get_keys_pressed().is_empty()
            || crate::pad::any_down()
        {
            s.shown = false;
            s.last_mouse = mouse;
        }
        s.since_aim += dt;
        s.layout = layout;
        let Some(layout) = layout else {
            s.sticks = Default::default();
            s.bomb_id = None;
            s.boost_id = None;
            s.bomb = false;
            s.boost = false;
            s.gun = false;
            return;
        };
        let (bomb_at, boost_at) = buttons(layout);
        for t in &touches {
            let p = Canvas::to_canvas(t.position);
            let side = s.sticks.iter().position(|k| k.id == Some(t.id));
            match t.phase {
                TouchPhase::Started => {
                    if hit(bomb_at, p) {
                        s.bomb_id = Some(t.id);
                        s.bomb = true;
                    } else if hit(boost_at, p) {
                        s.boost_id = Some(t.id);
                        s.boost = true;
                    } else if p.y >= HUD_BAND {
                        let side = (p.x >= CANVAS_W / 2.0) as usize;
                        if s.sticks[side].id.is_none() {
                            s.sticks[side].grab(t.id, p);
                            if side == 1 {
                                s.gun |= s.since_aim < DOUBLE_TAP;
                                s.since_aim = 0.0;
                            }
                        }
                    }
                }
                TouchPhase::Moved | TouchPhase::Stationary => {
                    if let Some(i) = side {
                        s.sticks[i].drag(p);
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    if let Some(i) = side {
                        let k = s.sticks[i];
                        if i == 0 && layout.boost == 0 && k.held < TAP_TIME && k.travel < TAP_SLOP {
                            s.boost = true;
                        }
                        s.sticks[i].id = None;
                        if i == 1 {
                            s.gun = false;
                        }
                    }
                    if s.bomb_id == Some(t.id) {
                        s.bomb_id = None;
                    }
                    if s.boost_id == Some(t.id) {
                        s.boost_id = None;
                    }
                }
            }
        }
        // A touch that went away without an end event lets go too.
        for k in &mut s.sticks {
            if k.id.is_some_and(|id| !touches.iter().any(|t| t.id == id)) {
                k.id = None;
            }
            if k.id.is_some() {
                k.held += dt;
            }
        }
    });
}

/// Whether the touch controls are in use (and drawn).
pub fn shown() -> bool {
    STATE.with_borrow(|s| s.shown)
}

/// The move stick, like a controller's left stick.
pub fn left() -> Vec2 {
    STATE.with_borrow(|s| {
        let v = s.sticks[0].value();
        if v.length() < MOVE_DEAD { Vec2::ZERO } else { v }
    })
}

/// The aim stick's direction while it fires.
pub fn aim() -> Option<Vec2> {
    STATE.with_borrow(|s| s.sticks[1].id.and(s.sticks[1].dir))
}

/// Takes a pending bomb press.
pub fn take_bomb() -> bool {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.bomb))
}

/// Takes a pending boost press.
pub fn take_boost() -> bool {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.boost))
}

/// Takes a pending gravity-gun double tap.
pub fn take_gun() -> bool {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.gun))
}

/// Draws the sticks and buttons over the HUD, in canvas coordinates.
/// `bombs` and `boost` say whether each can be used now.
pub fn draw(wheel: &Texture2D, font: &Font, bombs: bool, boost: bool) {
    STATE.with_borrow(|s| {
        let Some(layout) = s.layout.filter(|_| s.shown) else {
            return;
        };
        for (i, k) in s.sticks.iter().enumerate() {
            let (base, knob, alpha) = match k.id {
                Some(_) => (k.base, k.pos, 0.75),
                None => (rest(i), rest(i), 0.35),
            };
            let knob = base + (knob - base).clamp_length_max(RADIUS);
            sprite(wheel, base, RADIUS * 1.25, alpha);
            sprite(wheel, knob, RADIUS * 0.55, (alpha + 0.3).min(1.0));
        }
        let (bomb_at, boost_at) = buttons(layout);
        let button = |at: Option<Vec2>, label: &str, ready: bool, held: bool| {
            let Some(c) = at else { return };
            let a = if !ready { 0.2 } else if held { 0.8 } else { 0.5 };
            draw_circle(c.x, c.y, BUTTON, Color::new(0.05, 0.08, 0.12, a * 0.6));
            draw_circle_lines(c.x, c.y, BUTTON, 1.5, Color::new(0.75, 0.85, 1.0, a));
            font.text(label, c.x, c.y - 2.6, 0.4, Align::Centre, Color::new(1.0, 1.0, 1.0, a + 0.2));
        };
        button(bomb_at, "BOMB", bombs, s.bomb_id.is_some());
        button(boost_at, "BOOST", boost, s.boost_id.is_some());
    });
}

fn sprite(tex: &Texture2D, at: Vec2, half: f32, alpha: f32) {
    draw_texture_ex(
        tex,
        at.x - half,
        at.y - half,
        Color::new(1.0, 1.0, 1.0, alpha),
        DrawTextureParams {
            dest_size: Some(vec2(half * 2.0, half * 2.0)),
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_base_follows_a_thumb_past_the_rim() {
        let mut k = Stick::default();
        k.grab(1, vec2(100.0, 200.0));
        k.drag(vec2(100.0 + RADIUS * 3.0, 200.0));
        assert!((k.value() - vec2(1.0, 0.0)).length() < 1e-5);
        // Coming back reverses at once instead of crossing the old centre.
        k.drag(vec2(100.0 + RADIUS * 1.5, 200.0));
        assert!(k.value().x < 0.0);
    }

    #[test]
    fn aim_keeps_its_last_direction_inside_the_dead_zone() {
        let mut k = Stick::default();
        k.grab(1, vec2(0.0, 0.0));
        assert_eq!(k.dir, None);
        k.drag(vec2(0.0, -RADIUS));
        k.drag(vec2(0.0, -1.0));
        assert_eq!(k.dir, Some(vec2(0.0, -1.0)));
    }
}
