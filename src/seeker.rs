//! Seekers (`Seeker`, enemy kind 18), ported from the iOS binary: spinning
//! drones that home on the ship while looping in a one-second circle.

use macroquad::prelude::*;

use crate::gravity::Grav;
use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Seeker::Seeker / reset: radius 15; pool for type 18.
pub const RADIUS: f32 = 15.0;
pub const POOL: usize = 50;
/// Seeker::reset: 5 health x (wrapped survival levels + 1); 25 points.
const HEALTH: f32 = 5.0;
pub const POINTS: u32 = 25;
/// Seeker::reset: drag 0.3, spin -600 degrees/s from a random angle.
const DRAG: f32 = 0.3;
const SPIN: f32 = -600.0;
/// Seeker::update: homing push and the looping push, period 1 s.
const STEER: f32 = 200.0;
const LOOP: f32 = 800.0;

pub struct Seeker {
    /// In the gravity gun's grip, or just thrown by it.
    pub grav: Option<Grav>,
    pub pos: Vec2,
    pub impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    phase: f32,
}

impl Seeker {
    pub fn new(pos: Vec2) -> Seeker {
        Seeker {
            pos,
            grav: None,
            impulse: Vec2::ZERO,
            angle: rand::gen_range(0.0, 360.0),
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
            phase: 0.0,
        }
    }

    pub fn active(&self) -> bool {
        self.active && self.grav.is_none()
    }

    /// Seeker::update.
    pub fn update(&mut self, dt: f32, ship: Vec2, centre: Vec2) {
        if self.grav.is_some() {
            return;
        }
        self.flash = false;
        if self.active {
            let to = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y)) - self.pos;
            self.impulse += to.normalize_or_zero() * STEER * dt;
            self.phase = (self.phase + dt) % 1.0;
            let a = 2.0 * self.phase * std::f32::consts::PI;
            self.impulse += vec2(a.sin(), -a.cos()) * LOOP * dt;
            // Sprite::update.
            self.angle = (self.angle + SPIN * dt).rem_euclid(360.0);
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

    /// Seeker::handleEvent(0): true when the bullet destroys it.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }

    /// Pushed out of a circle at `other` by `strength` (already doubled
    /// where only this one moves).
    pub fn push_from(&mut self, other: Vec2, other_radius: f32, strength: f32) {
        let d = self.pos - other;
        if d == Vec2::ZERO {
            return;
        }
        let r = RADIUS + other_radius;
        self.impulse += d.normalize() * (r * r - d.length_squared()) * strength;
    }
}
