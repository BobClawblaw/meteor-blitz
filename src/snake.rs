//! Snakes (`Snake`, enemy kind 16, and `SnakeBody`, kind 15), ported from
//! the iOS binary. The head weaves toward the ship; eight body segments
//! replay its recorded path 0.1-0.8 s behind. Only the head takes damage:
//! each time its health runs out the tail segment dies instead, and the
//! head dies once no segments are left.

use macroquad::prelude::*;

use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Snake::Snake and SnakeBody::setOwner: radius 15 for head and segments.
pub const RADIUS: f32 = 15.0;
/// EnemyManager pool for type 16.
pub const POOL: usize = 100;
/// Snake::reset: 1 health x (wrapped survival levels + 1), 20 points;
/// SnakeBody::reset: 10 points each.
const HEALTH: f32 = 1.0;
pub const POINTS: u32 = 20;
pub const SEGMENT_POINTS: u32 = 10;
/// Snake::Snake: segment delays behind the head.
const DELAYS: [f32; 8] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
/// Snake::reset: drag 0.3; the weave period is 1.5-2.5 s.
const DRAG: f32 = 0.3;
const PERIOD: f32 = 2.0;
const PERIOD_JITTER: f32 = 0.5;
/// Snake::update: steering strength.
const STEER: f32 = 500.0;
/// History older than the longest delay is no longer needed.
const HISTORY_SECONDS: f32 = 1.0;

pub struct Segment {
    pub pos: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    delay: f32,
    pub flash: bool,
}

impl Segment {
    /// SnakeBody::canCollide: landed (state 10002).
    pub fn active_now(&self) -> bool {
        self.active
    }
}

pub struct Snake {
    pub pos: Vec2,
    pub impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    period: f32,
    phase: f32,
    clock: f32,
    /// (time, position, rotation) after each update.
    history: Vec<(f32, Vec2, f32)>,
    /// Head first, tail last.
    pub segments: Vec<Segment>,
    /// Wave flag 2: moves with the ship while dropping in.
    follow: bool,
}

/// What a hit on the head did.
pub enum Hit {
    /// Health left: flash and the hit sound.
    Hurt,
    /// The tail segment at this position died (and the head flashes).
    LostTail(Vec2),
    /// The head died.
    Dead,
}

fn heading(v: Vec2) -> f32 {
    let mut a = 90.0 + v.y.atan2(v.x).to_degrees();
    if a < 0.0 {
        a += 360.0;
    }
    a
}

impl Snake {
    /// Snake::reset, then Snake::activateBodies(8).
    pub fn new(pos: Vec2, follow: bool) -> Snake {
        let period = PERIOD + rand::gen_range(-1.0, 1.0) * PERIOD_JITTER;
        Snake {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
            period,
            phase: rand::gen_range(0.0, 1.0) * period,
            clock: 0.0,
            history: Vec::new(),
            segments: DELAYS
                .iter()
                .map(|&delay| Segment {
                    pos,
                    angle: 0.0,
                    z: -DROP_IN_HEIGHT,
                    alpha: FADE_IN_START,
                    active: false,
                    delay,
                    flash: false,
                })
                .collect(),
            follow,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Snake::getPositionAtTime / getRotationAtTime: interpolated along the
    /// recorded path, wrapping across the world edge; None before it starts.
    fn at_time(&self, t: f32) -> Option<(Vec2, f32)> {
        let i = self.history.iter().rposition(|h| h.0 <= t)?;
        let (t0, p0, a0) = self.history[i];
        let Some(&(t1, p1, a1)) = self.history.get(i + 1) else {
            return Some((p0, a0));
        };
        let f = (t - t0) / (t1 - t0);
        let d = vec2(wrap_near(p1.x, p0.x), wrap_near(p1.y, p0.y)) - p0;
        let mut da = a1 - a0;
        if da.abs() > 180.0 {
            da = if a0 < a1 { a1 - (a0 + 360.0) } else { (a1 + 360.0) - a0 };
        }
        Some((p0 + d * f, a0 + da * f))
    }

    /// Snake::update and SnakeBody::update.
    /// `ship_moved` is the ship's displacement this update: with wave flag
    /// 2 (`follow`), a dropping-in head moves with the ship (Enemy 0x4e).
    pub fn update(&mut self, dt: f32, ship: Vec2, ship_moved: Vec2, centre: Vec2) {
        self.flash = false;
        if !self.active && self.follow {
            self.pos += ship_moved;
        }
        if self.active {
            self.phase = (self.phase + dt) % self.period;
            let to = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y)) - self.pos;
            let dir = to.normalize_or_zero();
            let side = vec2(-dir.y, dir.x);
            let along = (self.phase * std::f32::consts::PI / self.period).sin();
            let across = (2.0 * self.phase * std::f32::consts::PI / self.period).cos();
            self.impulse += (dir * along + side * across) * STEER * dt;
            // Sprite::update.
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
            self.angle = heading(self.impulse);
        }
        self.pos = vec2(wrap_near(self.pos.x, centre.x), wrap_near(self.pos.y, centre.y));
        self.clock += dt;
        self.history.push((self.clock, self.pos, self.angle));
        let oldest = self.clock - HISTORY_SECONDS;
        let stale = self.history.iter().take_while(|h| h.0 < oldest).count();
        if stale > 1 {
            self.history.drain(..stale - 1);
        }
        for i in 0..self.segments.len() {
            let t = self.clock - self.segments[i].delay;
            let place = self.at_time(t);
            let s = &mut self.segments[i];
            s.flash = false;
            if s.active {
                if let Some((p, a)) = place {
                    s.pos = p;
                    s.angle = a;
                }
            } else {
                s.z = (s.z + dt * DROP_IN_SPEED).min(0.0);
                s.alpha += dt * FADE_IN_SPEED;
                if s.alpha >= 1.0 {
                    s.active = true;
                    s.alpha = 1.0;
                    s.z = 0.0;
                }
            }
            s.pos = vec2(wrap_near(s.pos.x, centre.x), wrap_near(s.pos.y, centre.y));
        }
    }

    fn flash_all(&mut self) {
        self.flash = true;
        for s in &mut self.segments {
            s.flash = true;
        }
    }

    /// Snake::handleEvent(0): a bullet hit on the head.
    pub fn hit(&mut self, damage: f32) -> Hit {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash_all();
            return Hit::Hurt;
        }
        self.lose_tail()
    }

    /// Snake::handleEvent(3/4): the ship rams the head.
    pub fn rammed(&mut self) -> Hit {
        self.lose_tail()
    }

    fn lose_tail(&mut self) -> Hit {
        match self.segments.pop() {
            None => Hit::Dead,
            Some(tail) => {
                self.health = HEALTH;
                self.flash_all();
                Hit::LostTail(tail.pos)
            }
        }
    }
}
