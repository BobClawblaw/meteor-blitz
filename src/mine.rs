//! Mines (`Mine`, enemy kind 17), ported from the iOS binary. A mine
//! drifts until the ship comes within 185 units, then spins up and homes
//! in on it.

use macroquad::prelude::*;

use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Mine::Mine: cpCircleShapeNew radius 14.
pub const RADIUS: f32 = 14.0;
/// EnemyManager pool for type 17.
pub const POOL: usize = 50;
/// Mine::reset: 2 health x (wrapped survival levels + 1); 20 points.
const HEALTH: f32 = 2.0;
pub const POINTS: u32 = 20;
/// Mine::reset: drag 0.3; spin 100 degrees/s, 1000 once it gives chase.
const DRAG: f32 = 0.3;
const SPIN: f32 = 100.0;
const CHASE_SPIN: f32 = 1000.0;
/// Mine::CHASING_RADIUS and the homing push.
const CHASING_RADIUS: f32 = 185.0;
const CHASE_ACCEL: f32 = 300.0;
/// Mine::handleEvent(0): sparks at most every 8 updates.
const SPARK_UPDATES: i32 = 8;
/// Collision push strength (mineCollision).
pub const REPEL: f32 = 0.01;

pub struct Mine {
    pub pos: Vec2,
    pub impulse: Vec2,
    pub angle: f32,
    spin: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    chasing: bool,
    health: f32,
    pub flash: bool,
    spark_cooldown: i32,
}

impl Mine {
    pub fn new(pos: Vec2) -> Mine {
        Mine {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            spin: SPIN,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            chasing: false,
            health: HEALTH,
            flash: false,
            spark_cooldown: 0,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Mine::update.
    pub fn update(&mut self, dt: f32, ship: Vec2, centre: Vec2) {
        self.flash = false;
        if self.active {
            let to = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y)) - self.pos;
            if !self.chasing && to.length_squared() < CHASING_RADIUS * CHASING_RADIUS {
                self.chasing = true;
                self.spin = CHASE_SPIN;
            }
            if self.chasing {
                self.impulse += to.normalize_or_zero() * CHASE_ACCEL * dt;
            }
            // Sprite::update.
            self.angle = (self.angle + self.spin * dt) % 360.0;
            self.pos += self.impulse * dt;
            self.impulse *= DRAG / (dt + DRAG);
            self.spark_cooldown = (self.spark_cooldown - 1).max(0);
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

    /// Mine::handleEvent(0). Returns (destroyed, sparks); sparks show at
    /// most every 8 updates, even on the killing hit.
    pub fn hit(&mut self, damage: f32) -> (bool, bool) {
        self.health -= damage;
        let dead = self.health <= 0.0;
        if !dead {
            self.flash = true;
        }
        let sparks = self.spark_cooldown == 0;
        if sparks {
            self.spark_cooldown = SPARK_UPDATES;
        }
        (dead, sparks)
    }

    /// Pushed out of a circle at `other` (Enemy::repelThis / repelOther).
    pub fn push_from(&mut self, other: Vec2, other_radius: f32, strength: f32) {
        let d = self.pos - other;
        if d == Vec2::ZERO {
            return;
        }
        let r = RADIUS + other_radius;
        self.impulse += d.normalize() * (r * r - d.length_squared()) * strength * 2.0;
    }
}
