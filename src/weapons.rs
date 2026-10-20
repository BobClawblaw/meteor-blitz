//! The fire and laser weapons (`FireBulletManager`, `LaserBulletManager`),
//! ported from the iOS binary. Both emit shots for the ship, which owns and
//! moves them; levels 0-7 come from the world being played.

use macroquad::prelude::*;

/// FireBulletManager: SPEED 600, LIFE 0.4, segment radius 2, mass 0.025.
pub const FIRE_SPEED: f32 = 600.0;
pub const FIRE_LIFE: f32 = 0.4;
pub const FIRE_RADIUS: f32 = 2.0;
pub const FIRE_MASS: f32 = 0.025;
/// FireBulletManager::setLevel: seconds between flames, aim jitter in
/// degrees, and damage per flame.
const FIRE_INTERVAL: [f32; 8] = [0.02, 0.0173913, 0.0153846, 0.0137931, 0.0125, 0.0114286, 0.0105263, 0.0097561];
const FIRE_SPREAD: [f32; 8] = [1.0, 1.8, 2.6, 3.4, 4.2, 5.0, 5.8, 6.6];
pub const FIRE_DAMAGE: [f32; 8] = [0.13334, 0.173913, 0.230769, 0.275862, 0.3125, 0.342857, 0.368421, 0.390244];
/// FireBulletManager::fire: the first flame of a burst is jittered by 3 degrees.
const FIRE_FIRST_SPREAD: f32 = 3.0;
/// FireBulletManager::add: flames are drawn 0.05 of their length behind.
pub const FIRE_DRAW_BACK: f32 = 0.05;

/// LaserBulletManager: SPEED 1000, LIFE 0.3, mass 0.03, damage 0.85.
pub const LASER_SPEED: f32 = 1000.0;
pub const LASER_LIFE: f32 = 0.3;
pub const LASER_RADIUS: f32 = 2.0;
pub const LASER_MASS: f32 = 0.03;
pub const LASER_DAMAGE: f32 = 0.85;
/// LaserBulletManager::setLevel: seconds between volleys.
const LASER_INTERVAL: [f32; 8] = [0.127494, 0.085, 0.113333, 0.085, 0.102, 0.085, 0.0971429, 0.085];
/// LaserBulletManager::fire: sideways offsets of the bolts in a volley.
const LASER_OFFSETS: [&[f32]; 8] = [
    &[0.0],
    &[0.0],
    &[3.0, -3.0],
    &[3.0, -3.0],
    &[6.0, 0.0, -6.0],
    &[6.0, 0.0, -6.0],
    &[9.0, 3.0, -3.0, -9.0],
    &[9.0, 3.0, -3.0, -9.0],
];
/// LaserBulletManager::add: bolts are drawn a quarter length behind.
pub const LASER_DRAW_BACK: f32 = 0.25;

/// Degrees, 0 up, clockwise (the original's heading).
fn heading(v: Vec2) -> f32 {
    let mut a = 90.0 + v.y.atan2(v.x).to_degrees();
    if a < 0.0 {
        a += 360.0;
    }
    a
}

fn along(heading_deg: f32) -> Vec2 {
    let a = heading_deg.to_radians();
    vec2(a.sin(), -a.cos())
}

/// A new shot: start, velocity.
pub type Shot = (Vec2, Vec2);

pub struct FireGun {
    pub level: usize,
    started: bool,
    timer: f32,
    last_origin: Vec2,
    last_heading: f32,
}

impl FireGun {
    pub fn new(level: usize) -> FireGun {
        FireGun { level: level.min(7), started: false, timer: 0.0, last_origin: Vec2::ZERO, last_heading: 0.0 }
    }

    /// FireBulletManager::reset.
    pub fn release(&mut self) {
        self.started = false;
    }

    /// FireBulletManager::fire: a stream of flames, spread evenly along the
    /// ship's path and swing since the last update.
    pub fn fire(&mut self, dt: f32, origin: Vec2, dir: Vec2, out: &mut Vec<Shot>) {
        let aim = heading(dir);
        if !self.started {
            let a = aim + rand::gen_range(-1.0, 1.0) * FIRE_FIRST_SPREAD;
            out.push((origin, along(a) * FIRE_SPEED));
            self.timer = 0.0;
            self.started = true;
        } else {
            let interval = FIRE_INTERVAL[self.level];
            let spread = FIRE_SPREAD[self.level];
            let mut t = self.timer;
            while t < dt {
                let f = t / dt;
                let p = self.last_origin + (origin - self.last_origin) * f;
                let mut turn = aim - self.last_heading;
                if turn > 180.0 {
                    turn -= 360.0;
                } else if -turn > 180.0 {
                    turn += 360.0;
                }
                let a = self.last_heading + f * turn + rand::gen_range(-1.0, 1.0) * spread;
                let v = along(a) * FIRE_SPEED;
                out.push((p + v * (dt - t), v));
                t += interval;
            }
            self.timer = t - dt;
        }
        self.last_origin = origin;
        self.last_heading = aim;
    }
}

pub struct LaserGun {
    pub level: usize,
    timer: f32,
}

impl LaserGun {
    pub fn new(level: usize) -> LaserGun {
        let level = level.min(7);
        LaserGun { level, timer: LASER_INTERVAL[level] }
    }

    /// LaserBulletManager::fire: volleys at a steady rate; letting go of
    /// the trigger does not reset the timer.
    pub fn fire(&mut self, dt: f32, origin: Vec2, dir: Vec2, out: &mut Vec<Shot>) {
        let interval = LASER_INTERVAL[self.level];
        self.timer += dt;
        if self.timer < interval {
            return;
        }
        self.timer %= interval;
        let side = vec2(-dir.y, dir.x);
        for &k in LASER_OFFSETS[self.level] {
            out.push((origin + side * k, dir * LASER_SPEED));
        }
    }
}
