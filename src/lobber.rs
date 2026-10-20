//! Lobbers (`Lobber`, enemy kind 20), ported from the iOS binary. A lobber
//! turns toward the ship, grows a ball (a `BossOutside`), lunges forward,
//! then throws the ball at the ship.

use macroquad::prelude::*;

use crate::boss::Piece;
use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Lobber::reset: radius 25; default Sprite mass 1; pool for type 20.
pub const RADIUS: f32 = 25.0;
pub const MASS: f32 = 1.0;
pub const POOL: usize = 30;
/// Lobber::reset: 30 health x (wrapped survival levels + 1); 100 points.
const HEALTH: f32 = 30.0;
pub const POINTS: u32 = 100;
const DRAG: f32 = 0.3;
const TURN_SPEED: f32 = 360.0;
/// Lobber::update: lunge thrust; Lobber::BALL_SPEED.
const LUNGE: f32 = 300.0;
const BALL_SPEED: f32 = 200.0;

#[derive(Clone, Copy, PartialEq)]
enum Substate {
    Aim,
    Load,
    Lunge,
    Settle,
}

pub struct Lobber {
    pub pos: Vec2,
    pub impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    substate: Substate,
    timer: f32,
    /// The ball it holds (Lobber 0x80).
    pub ball: Option<Piece>,
}

impl Lobber {
    pub fn new(pos: Vec2) -> Lobber {
        Lobber {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
            substate: Substate::Aim,
            timer: 0.0,
            ball: None,
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Enemy::rotateTowards at 360 degrees/s.
    fn rotate_towards(&mut self, ship: Vec2, dt: f32) {
        let d = ship - self.pos;
        let mut target = 90.0 + d.y.atan2(d.x).to_degrees();
        if target < 0.0 {
            target += 360.0;
        }
        while target - self.angle > 180.0 {
            target -= 360.0;
        }
        while self.angle - target > 180.0 {
            target += 360.0;
        }
        let step = dt * TURN_SPEED;
        self.angle = if target <= self.angle {
            (self.angle - step).max(target)
        } else {
            (self.angle + step).min(target)
        };
        self.angle %= 360.0;
    }

    /// Lobber::update. Returns a thrown ball for the field to own.
    pub fn update(&mut self, dt: f32, ship: Vec2, centre: Vec2, can_load: bool) -> Option<Piece> {
        self.flash = false;
        let ship = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y));
        let mut thrown = None;
        self.timer += dt;
        if self.active {
            self.rotate_towards(ship, dt);
            match self.substate {
                Substate::Aim => {
                    if self.timer > 1.0 {
                        // Lobber::setSubstate(1): a new ball, or straight
                        // back to aiming when none is free.
                        self.timer = 0.0;
                        if can_load {
                            self.ball = Some(Piece::new(self.pos));
                            self.substate = Substate::Load;
                        }
                    }
                }
                Substate::Load => {
                    if self.timer > 0.5 {
                        self.timer = 0.0;
                        self.substate = Substate::Lunge;
                    }
                }
                Substate::Lunge => {
                    let a = self.angle.to_radians();
                    self.impulse += vec2(a.sin(), -a.cos()) * LUNGE * dt;
                    if self.timer > 1.5 {
                        self.timer = 0.0;
                        self.substate = Substate::Settle;
                    }
                }
                Substate::Settle => {
                    if self.timer > 1.0 {
                        // Lobber::setSubstate(4): throw, then aim again.
                        self.timer = 0.0;
                        self.substate = Substate::Aim;
                        if let Some(mut ball) = self.ball.take() {
                            ball.throw((ship - self.pos).normalize_or_zero() * BALL_SPEED);
                            thrown = Some(ball);
                        }
                    }
                }
            }
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
        self.pos = vec2(wrap_near(self.pos.x, centre.x), wrap_near(self.pos.y, centre.y));
        let pos = self.pos;
        if let Some(ball) = &mut self.ball {
            ball.hold(dt, pos);
        }
        thrown
    }

    /// Lobber::handleEvent(0): true when the bullet destroys it.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }

    /// Pushed out of a circle at `other` (Enemy::repelThis / repelOther).
    pub fn push_from(&mut self, other: Vec2, other_radius: f32, strength: f32) {
        let d = self.pos - other;
        if d == Vec2::ZERO {
            return;
        }
        let r = RADIUS + other_radius;
        self.impulse += d.normalize() * (r * r - d.length_squared()) * strength;
    }
}
