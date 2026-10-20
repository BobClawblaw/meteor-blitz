//! Rings (`Bonus`): pickups dropped by destroyed small rocks, ported from
//! the iOS binary. They drift and wander until the ship comes within 50
//! units, then home in at 250 units/s and are collected within 14.

use macroquad::prelude::*;

use crate::enemies::wrap_near;

/// EnemyManager pool size for type 24 (bonus).
const POOL: usize = 200;
/// Bonus::reset: seconds before a ring expires; the last one fades out.
const LIFE: f32 = 5.0;
/// Bonus::reset: drag time constant for velocity and impulse.
const DRAG: f32 = 0.3;
/// Bonus::reset: spin in degrees per second.
const SPIN: f32 = 1000.0;
const COLLECT_DISTANCE: f32 = 14.0;
const ATTRACT_DISTANCE: f32 = 50.0;
const HOMING_SPEED: f32 = 250.0;
/// Bonus::update: random wander added to the impulse, per second.
const WANDER: f32 = 100.0;
/// Enemy::activateBonuses: spawn offset and outward impulse per unit of spread.
const SPAWN_OFFSET: f32 = 5.0;

pub struct Ring {
    pub pos: Vec2,
    vel: Vec2,
    /// Sprite field 0x20: an impulse that decays with the same drag.
    impulse: Vec2,
    pub angle: f32,
    life: f32,
    /// Bonus 0x7a: locked onto the ship; no longer expires.
    homing: bool,
    pub alpha: f32,
}

impl Ring {
    pub fn new(pos: Vec2, impulse: Vec2) -> Ring {
        Ring { pos, vel: Vec2::ZERO, impulse, angle: 0.0, life: LIFE, homing: false, alpha: 1.0 }
    }

    fn wander(&mut self, dt: f32) {
        let f = DRAG / (dt + DRAG);
        self.vel *= f;
        let step = dt * WANDER;
        self.impulse += vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * step;
    }

    /// Bonus::update. Returns true when the ring is collected.
    fn update(&mut self, dt: f32, ship: Option<Vec2>) -> Option<bool> {
        if !self.homing {
            self.life -= dt;
            if self.life <= 0.0 {
                return None;
            }
        }
        match ship {
            None => self.wander(dt),
            Some(ship) => {
                let ship = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y));
                let to = ship - self.pos;
                let d = to.length();
                if d < COLLECT_DISTANCE {
                    return Some(true);
                }
                if !self.homing && d >= ATTRACT_DISTANCE {
                    self.wander(dt);
                } else {
                    self.homing = true;
                    self.impulse = Vec2::ZERO;
                    self.vel = to / d * HOMING_SPEED;
                }
            }
        }
        // Sprite::updateWithoutAnimation.
        self.pos += self.vel * dt;
        self.angle = (self.angle + dt * SPIN) % 360.0;
        self.pos += self.impulse * dt;
        self.impulse *= DRAG / (dt + DRAG);
        self.alpha = self.life.min(1.0);
        Some(false)
    }
}

#[derive(Default)]
pub struct Rings {
    pub rings: Vec<Ring>,
}

impl Rings {
    /// Enemy::activateBonuses: `count` rings scattered around `pos`.
    pub fn drop(&mut self, pos: Vec2, speed: f32, count: u32) {
        for _ in 0..count {
            if self.rings.len() >= POOL {
                return;
            }
            let d = vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0));
            self.rings.push(Ring::new(pos + d * SPAWN_OFFSET, d * speed));
        }
    }

    /// Updates every ring and returns how many the ship collected, with
    /// their positions. `ship` is None while the ship is destroyed; rings
    /// stay within half a world of `centre` (World::updateSpritePosition).
    pub fn update(&mut self, dt: f32, ship: Option<Vec2>, centre: Vec2) -> Vec<Vec2> {
        let mut collected = Vec::new();
        self.rings.retain_mut(|r| match r.update(dt, ship) {
            None => false,
            Some(true) => {
                collected.push(r.pos);
                false
            }
            Some(false) => {
                r.pos = vec2(wrap_near(r.pos.x, centre.x), wrap_near(r.pos.y, centre.y));
                true
            }
        });
        collected
    }
}
