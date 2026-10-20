//! Depots (`Depot`, enemy kind 21): crates that drop a power-up when
//! destroyed, ported from the iOS binary.

use macroquad::prelude::*;

use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Depot::Depot: cpCircleShapeNew radius 15, mass 5.
pub const RADIUS: f32 = 15.0;
pub const MASS: f32 = 5.0;
/// EnemyManager pool size for type 21.
pub const POOL: usize = 5;
/// Depot::reset: health 5 x (wrapped survival levels + 1); 15 points.
const HEALTH: f32 = 5.0;
pub const POINTS: u32 = 15;
/// Depot::reset: drag 0.3 and a spin of 100 degrees/s.
const DRAG: f32 = 0.3;
const SPIN: f32 = 100.0;
/// depotCollision / Depot::handleEvent(3): soft push-apart strength.
pub const REPEL: f32 = 0.01;
/// Depot::draw: the "BONUS" label sits 19 units below the centre at 75%.
pub const LABEL_DROP: f32 = 19.0;
pub const LABEL_ALPHA: f32 = 0.75;

pub struct Depot {
    pub pos: Vec2,
    /// Sprite field 0x20: pushes from the ship and other depots.
    pub impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    pub health: f32,
    /// Sprite 0x4c: drawn as a white silhouette for the next frame.
    pub flash: bool,
}

impl Depot {
    pub fn new(pos: Vec2) -> Depot {
        Depot {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Depot::update: drop in, then Sprite::update while active.
    pub fn update(&mut self, dt: f32, centre: Vec2) {
        // The flash from the last hit has been drawn once by now.
        self.flash = false;
        if self.active {
            self.angle = (self.angle + SPIN * dt) % 360.0;
            self.pos += self.impulse * dt;
            self.impulse *= DRAG / (dt + DRAG);
        } else {
            self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
            self.alpha += dt * FADE_IN_SPEED;
            if self.alpha >= 1.0 {
                self.active = true;
                self.alpha = 1.0;
                self.z = 0.0;
            }
        }
        self.pos = vec2(wrap_near(self.pos.x, centre.x), wrap_near(self.pos.y, centre.y));
    }

    /// Enemy::repelThis: push this depot out of a circle at `other`.
    pub fn repel_from(&mut self, other: Vec2, other_radius: f32, strength: f32) {
        let d = other - self.pos;
        if d == Vec2::ZERO {
            return;
        }
        let r = RADIUS + other_radius;
        let overlap = r * r - d.length_squared();
        self.impulse += d.normalize() * overlap * strength * -2.0;
    }
}

/// Enemy::repelBoth between two depots.
pub fn repel_both(a: &mut Depot, b: &mut Depot, strength: f32) {
    let d = b.pos - a.pos;
    if d == Vec2::ZERO {
        return;
    }
    let r = RADIUS * 2.0;
    let push = d.normalize() * (r * r - d.length_squared()) * strength;
    a.impulse -= push;
    b.impulse += push;
}
