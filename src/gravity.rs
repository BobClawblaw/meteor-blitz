//! The gravity gun (`GravityGun`, Enemy states 10003-10005) and its sparkle
//! trails (`CustomSpriteSystem`), ported from the iOS binary.
//!
//! While active, the gun reaches 125 units along the aim; the nearest enemy
//! it touches that will come (by the gun's level) is pulled toward the
//! ship, locked 20 units out in front, and thrown along the aim when the
//! trigger is let go. A thrown enemy explodes after half a second, and
//! wrecks what it hits on the way.

use macroquad::prelude::*;

use crate::enemies::wrap_near;

/// Global::GRAVITY_GUN_LENGTH / RADIUS; GravityGun::activate cooldown.
pub const REACH: f32 = 125.0;
pub const BEAM_RADIUS: f32 = 5.0;
const COOLDOWN: f32 = 0.5;
/// Global::GRAVITY_GUN_CAPTURE_SPEED, MAX_CAPTURE_SPEED,
/// CAPTURED_DISTANCE, RELEASE_SPEED.
const PULL: f32 = 4000.0;
const MAX_PULL_SPEED: f32 = 550.0;
const HOLD_DISTANCE: f32 = 20.0;
const THROW_SPEED: f32 = 720.0;
/// Enemy::setState(10005): a throw lasts 0.5 s; letting go mid-pull
/// keeps the pull speed, boosted 1.65 times.
const THROW_LIFE: f32 = 0.5;
const LOOSE_BOOST: f32 = 1.65;
/// Enemy::setState(10003/10004): a captured enemy spins at 600 degrees/s.
const SPIN: f32 = 600.0;
/// Enemy::update: trail sparks every 0.02 s, drifting 3 units per update.
const TRAIL_STEP: f32 = 0.02;
const SPARK_DRIFT: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    /// 10003: being pulled in.
    Pulled,
    /// 10004: held in front of the ship.
    Locked,
    /// 10005: thrown; seconds left before it blows.
    Thrown(f32),
}

/// An enemy in the gravity gun's grip (or just thrown by it).
#[derive(Clone, Debug)]
pub struct Grav {
    pub phase: Phase,
    pub vel: Vec2,
    prev_pos: Vec2,
    prev_angle: f32,
    trail: f32,
}

impl Grav {
    pub fn new(pos: Vec2, angle: f32) -> Grav {
        Grav { phase: Phase::Pulled, vel: Vec2::ZERO, prev_pos: pos, prev_angle: angle, trail: 0.0 }
    }

    /// Enemy::setState(10005) on GravityGun::release.
    pub fn release(&mut self, aim: Vec2) {
        self.vel = match self.phase {
            Phase::Locked => aim * THROW_SPEED,
            _ => self.vel * LOOSE_BOOST,
        };
        self.phase = Phase::Thrown(THROW_LIFE);
        self.trail = 0.0;
    }

    pub fn thrown(&self) -> bool {
        matches!(self.phase, Phase::Thrown(_))
    }

    /// Enemy::update for states 10003-10005. Returns true when a thrown
    /// enemy's time is up.
    pub fn carry(&mut self, pos: &mut Vec2, angle: &mut f32, dt: f32, ship: Vec2, aim: Vec2, centre: Vec2, sparks: &mut Sparks) -> bool {
        let mut expired = false;
        match self.phase {
            Phase::Pulled => {
                let to = vec2(wrap_near(ship.x, pos.x), wrap_near(ship.y, pos.y)) - *pos;
                if to.length_squared() > HOLD_DISTANCE * HOLD_DISTANCE {
                    self.vel += to.normalize() * PULL * dt;
                    if self.vel.length() > MAX_PULL_SPEED {
                        self.vel = self.vel.normalize() * MAX_PULL_SPEED;
                    }
                } else {
                    self.phase = Phase::Locked;
                    self.vel = Vec2::ZERO;
                    *pos = ship + aim * HOLD_DISTANCE;
                }
            }
            Phase::Locked => {
                *pos = ship + aim * HOLD_DISTANCE;
            }
            Phase::Thrown(t) => {
                let t = t - dt;
                expired = t <= 0.0;
                self.phase = Phase::Thrown(t);
            }
        }
        // Sprite::update.
        *pos += self.vel * dt;
        *angle = (*angle + SPIN * dt) % 360.0;
        *pos = vec2(wrap_near(pos.x, centre.x), wrap_near(pos.y, centre.y));
        if self.phase == Phase::Locked {
            // A purple shimmer each update while held.
            let a = angle.to_radians();
            let d = vec2(a.sin(), -a.cos()) * SPARK_DRIFT;
            sparks.add(SparkKind::Held, *pos, d);
            sparks.add(SparkKind::Held, *pos, -d);
        } else {
            // A trail of sparks spread along the path since the last update.
            let prev = vec2(wrap_near(self.prev_pos.x, pos.x), wrap_near(self.prev_pos.y, pos.y));
            let mut turn = *angle - self.prev_angle;
            if turn > 180.0 {
                turn -= 360.0;
            } else if turn < -180.0 {
                turn += 360.0;
            }
            let mut t = self.trail;
            while t < dt {
                let f = t / dt;
                let p = prev + (*pos - prev) * f;
                let a = (self.prev_angle + turn * f).to_radians();
                let d = vec2(a.sin(), -a.cos()) * SPARK_DRIFT;
                sparks.add(SparkKind::Trail, p + d * (dt - t), d);
                sparks.add(SparkKind::Trail, p - d * (dt - t), -d);
                t += TRAIL_STEP;
            }
            self.trail = t - dt;
        }
        self.prev_pos = *pos;
        self.prev_angle = *angle;
        expired
    }
}

/// Whether an enemy will come to a gun of `level` (handleEvent(5)).
pub fn catches_rock(size: usize, level: u32) -> bool {
    size == 0 || (size == 1 && level >= 2)
}

/// GravityGun: on (reaching out), its cooldown clock, and its level.
pub struct GravityGun {
    pub active: bool,
    since: f32,
    pub level: u32,
}

impl GravityGun {
    pub fn new(level: u32) -> GravityGun {
        GravityGun { active: false, since: 1.0, level }
    }

    /// GravityGun::update.
    pub fn update(&mut self, dt: f32) {
        self.since += dt;
    }

    /// GravityGun::activate: at most every half second.
    pub fn activate(&mut self) {
        if self.since > COOLDOWN {
            self.since = 0.0;
            self.active = true;
        }
    }
}

#[derive(Clone, Copy)]
pub enum SparkKind {
    /// add(0): while held.
    Held,
    /// add(1): trailing a pulled or thrown enemy.
    Trail,
}

struct Spark {
    pos: Vec2,
    /// Units per update.
    drift: Vec2,
    color: Color,
    fade: f32,
    life: i32,
}

/// CustomSpriteSystem: 400 additive point sprites (fire200.png), 50 units
/// across, living 25 updates.
#[derive(Default)]
pub struct Sparks {
    sparks: Vec<Spark>,
}

const SPARK_CAPACITY: usize = 400;
pub const SPARK_SIZE: f32 = 50.0;
const SPARK_LIFE: i32 = 25;

impl Sparks {
    pub fn add(&mut self, kind: SparkKind, pos: Vec2, drift: Vec2) {
        if self.sparks.len() >= SPARK_CAPACITY {
            return;
        }
        let (alpha, fade) = match kind {
            SparkKind::Held => (0.3, 0.012),
            SparkKind::Trail => (1.0, 0.04),
        };
        self.sparks.push(Spark {
            pos,
            drift,
            color: Color::new(0.25, 0.007, 0.6, alpha),
            fade,
            life: SPARK_LIFE,
        });
    }

    /// CustomSpriteSystem::update: one step per fixed update.
    pub fn update(&mut self, centre: Vec2) {
        for s in &mut self.sparks {
            s.life -= 1;
            s.pos += s.drift;
            s.pos = vec2(wrap_near(s.pos.x, centre.x), wrap_near(s.pos.y, centre.y));
            s.color.a -= s.fade;
        }
        self.sparks.retain(|s| s.life > 0);
    }

    /// (position, colour) of every live spark.
    pub fn points(&self) -> impl Iterator<Item = (Vec2, Color)> + '_ {
        self.sparks.iter().map(|s| (s.pos, s.color))
    }
}
