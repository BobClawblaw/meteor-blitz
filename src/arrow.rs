//! Arrows (`Arrow`, enemy kind 14), ported from the iOS binary: darts that
//! accelerate along the heading their spawn gave them, passing through
//! every other enemy.

use macroquad::prelude::*;

use crate::gravity::Grav;
use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Arrow::Arrow: cpCircleShapeNew radius 11.
pub const RADIUS: f32 = 11.0;
/// EnemyManager pool for type 14.
pub const POOL: usize = 125;
/// Arrow::reset: 1 health x (wrapped survival levels + 1); 20 points.
const HEALTH: f32 = 1.0;
pub const POINTS: u32 = 20;
/// Arrow::reset: drag 0.3. Arrow::update: thrust 500 units/s^2.
const DRAG: f32 = 0.3;
const THRUST: f32 = 500.0;

pub struct Arrow {
    /// In the gravity gun's grip, or just thrown by it.
    pub grav: Option<Grav>,
    pub pos: Vec2,
    impulse: Vec2,
    dir: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
}

impl Arrow {
    /// WaveManager::updateWave: `heading` in degrees, 0 up, clockwise.
    pub fn new(pos: Vec2, heading: f32) -> Arrow {
        let a = heading.to_radians();
        Arrow {
            pos,
            grav: None,
            impulse: Vec2::ZERO,
            dir: vec2(a.sin(), -a.cos()),
            angle: heading,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
        }
    }

    pub fn active(&self) -> bool {
        self.active && self.grav.is_none()
    }

    /// Arrow::update.
    pub fn update(&mut self, dt: f32, centre: Vec2) {
        if self.grav.is_some() {
            return;
        }
        self.flash = false;
        if self.active {
            self.impulse += self.dir * THRUST * dt;
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
        if self.impulse != Vec2::ZERO {
            let mut a = 90.0 + self.impulse.y.atan2(self.impulse.x).to_degrees();
            if a < 0.0 {
                a += 360.0;
            }
            self.angle = a;
        }
        self.pos = vec2(wrap_near(self.pos.x, centre.x), wrap_near(self.pos.y, centre.y));
    }

    /// Arrow::handleEvent(0): true when the bullet destroys it.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }
}
