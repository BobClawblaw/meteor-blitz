//! Rings (`Bonus`) and power-ups (`BigBonus`), ported from the iOS binary.
//! Rings drop from destroyed small rocks, power-ups from depots. Both drift
//! and wander until the ship comes within 50 units, then home in at 250
//! units/s; rings are collected within 14 units, power-ups within 15.

use macroquad::prelude::*;

use crate::enemies::wrap_near;

/// EnemyManager pool sizes for types 24 (bonus) and 25 (big bonus).
const POOL: usize = 200;
const POWER_POOL: usize = 10;
/// Bonus::reset / BigBonus::reset: seconds before expiry; the last fades.
const LIFE: f32 = 5.0;
const POWER_LIFE: f32 = 30.0;
/// Bonus::reset: drag time constant for velocity and impulse.
const DRAG: f32 = 0.3;
/// Bonus::reset: spin in degrees per second.
const SPIN: f32 = 1000.0;
const COLLECT_DISTANCE: f32 = 14.0;
const POWER_COLLECT_DISTANCE: f32 = 15.0;
const ATTRACT_DISTANCE: f32 = 50.0;
const HOMING_SPEED: f32 = 250.0;
/// Bonus::update: random wander added to the impulse, per second.
const WANDER: f32 = 100.0;
/// Enemy::activateBonuses: spawn offset and outward impulse per unit of spread.
const SPAWN_OFFSET: f32 = 5.0;
/// Enemy::activateBigBonus: spawn offset and impulse along a random heading.
const POWER_OFFSET: f32 = 10.0;
const POWER_IMPULSE: f32 = 50.0;

/// BigBonus::setType.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Power {
    Life,
    Bomb,
    Shield,
    Slow,
    Magnet,
}

impl Power {
    /// Frame of bigBonus.png (BigBonus::setType's setFrameIndex).
    pub fn frame(self) -> usize {
        match self {
            Power::Life => 0,
            Power::Shield => 1,
            Power::Bomb => 2,
            Power::Slow => 3,
            Power::Magnet => 4,
        }
    }
}

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
    /// None for a ring, else the power-up this is.
    pub power: Option<Power>,
}

impl Ring {
    pub fn new(pos: Vec2, impulse: Vec2, homing: bool) -> Ring {
        Ring {
            pos,
            vel: Vec2::ZERO,
            impulse,
            angle: 0.0,
            life: LIFE,
            homing,
            alpha: 1.0,
            power: None,
        }
    }

    fn collect_distance(&self) -> f32 {
        if self.power.is_some() {
            POWER_COLLECT_DISTANCE
        } else {
            COLLECT_DISTANCE
        }
    }

    fn wander(&mut self, dt: f32) {
        let f = DRAG / (dt + DRAG);
        self.vel *= f;
        let step = dt * WANDER;
        self.impulse += vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * step;
    }

    /// Bonus::update. Returns true when the ring is collected.
    /// `age` is how much of its life passes (0 in the tutorial).
    fn update(&mut self, age: f32, dt: f32, ship: Option<Vec2>) -> Option<bool> {
        if !self.homing {
            self.life -= age;
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
                if d < self.collect_distance() {
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
    /// Bonus::update: in the tutorial rings and power-ups never expire.
    pub timeless: bool,
}

impl Rings {
    /// Enemy::activateBonuses: `count` rings scattered around `pos`;
    /// they home in at once while the ring magnet is on.
    pub fn drop(&mut self, pos: Vec2, speed: f32, count: u32, homing: bool) {
        for _ in 0..count {
            if self.rings.iter().filter(|r| r.power.is_none()).count() >= POOL {
                return;
            }
            let d = vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0));
            self.rings.push(Ring::new(pos + d * SPAWN_OFFSET, d * speed, homing));
        }
    }

    /// Enemy::activateBigBonus: one power-up thrown out from `pos`.
    pub fn drop_power(&mut self, pos: Vec2, power: Power) {
        if self.rings.iter().filter(|r| r.power.is_some()).count() >= POWER_POOL {
            return;
        }
        let a = rand::gen_range(-1.0f32, 1.0) * std::f32::consts::PI;
        let d = vec2(a.sin(), -a.cos());
        let mut r = Ring::new(pos + d * POWER_OFFSET, d * POWER_IMPULSE, false);
        r.life = POWER_LIFE;
        r.power = Some(power);
        self.rings.push(r);
    }

    /// EnemyManager::sharedHandleEvent(14): every ring heads for the ship.
    pub fn magnetize(&mut self) {
        for r in self.rings.iter_mut().filter(|r| r.power.is_none()) {
            r.homing = true;
        }
    }

    /// Updates every ring and power-up and returns the ones the ship
    /// collected. `ship` is None while the ship is destroyed; rings
    /// stay within half a world of `centre` (World::updateSpritePosition).
    pub fn update(&mut self, dt: f32, ship: Option<Vec2>, centre: Vec2) -> Vec<(Vec2, Option<Power>)> {
        let mut collected = Vec::new();
        let timeless = self.timeless;
        self.rings.retain_mut(|r| match r.update(dt * !timeless as u8 as f32, dt, ship) {
            None => false,
            Some(true) => {
                collected.push((r.pos, r.power));
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
