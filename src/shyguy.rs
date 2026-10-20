//! Shy guys (`ShyGuy`, enemy kind 13), ported from the iOS binary: small
//! drones that steer straight at the ship, facing the way they move.

use macroquad::prelude::*;

use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// ShyGuy::ShyGuy: cpCircleShapeNew radius 10.
pub const RADIUS: f32 = 10.0;
/// EnemyManager pool for type 13.
pub const POOL: usize = 100;
/// ShyGuy::reset: 1.5 health x (wrapped survival levels + 1); 10 points.
const HEALTH: f32 = 1.5;
pub const POINTS: u32 = 10;
/// ShyGuy::reset: drag 0.3. ShyGuy::update: steering push 200 units/s^2.
const DRAG: f32 = 0.3;
const STEER: f32 = 200.0;
/// Animation: 16 frames of shyGuyFrames.png at 0.05 s, random start.
pub const FRAMES: usize = 16;
const FRAME_SECONDS: f32 = 0.05;

pub struct ShyGuy {
    pub pos: Vec2,
    pub impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    pub frame: usize,
    frame_timer: f32,
}

impl ShyGuy {
    pub fn new(pos: Vec2) -> ShyGuy {
        ShyGuy {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
            frame: rand::gen_range(0, FRAMES),
            frame_timer: 0.0,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    fn animate(&mut self, dt: f32) {
        let t = self.frame_timer + dt;
        self.frame = (self.frame + (t / FRAME_SECONDS) as usize) % FRAMES;
        self.frame_timer = t % FRAME_SECONDS;
    }

    /// ShyGuy::update.
    pub fn update(&mut self, dt: f32, ship: Vec2, centre: Vec2) {
        self.flash = false;
        if self.active {
            let to = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y)) - self.pos;
            self.impulse += to.normalize_or_zero() * STEER * dt;
            // Sprite::update (no spin of its own).
            self.pos += self.impulse * dt;
            self.impulse *= DRAG / (dt + DRAG);
            self.animate(dt);
        } else {
            self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
            self.alpha += dt * FADE_IN_SPEED;
            if self.alpha >= 1.0 {
                self.active = true;
                self.alpha = 1.0;
                self.z = 0.0;
            }
            self.animate(dt);
        }
        // Face the direction of travel.
        if self.impulse != Vec2::ZERO {
            let mut a = 90.0 + self.impulse.y.atan2(self.impulse.x).to_degrees();
            if a < 0.0 {
                a += 360.0;
            }
            self.angle = a;
        }
        self.pos = vec2(wrap_near(self.pos.x, centre.x), wrap_near(self.pos.y, centre.y));
    }

    /// ShyGuy::handleEvent(0): true when the bullet destroys it.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }

    /// Pushed out of a circle at `other`, with `strength` already doubled
    /// where the original pushes only this one.
    pub fn push_from(&mut self, other: Vec2, other_radius: f32, strength: f32) {
        let d = self.pos - other;
        if d == Vec2::ZERO {
            return;
        }
        let r = RADIUS + other_radius;
        self.impulse += d.normalize() * (r * r - d.length_squared()) * strength;
    }
}
