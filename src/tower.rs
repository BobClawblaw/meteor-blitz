//! Towers (`Tower`, enemy kind 19), ported from the iOS binary: fixed laser
//! turrets. A tower turns toward the ship at 360 degrees/s; 1.5 s after its
//! last shot, with the ship within 200 units, it locks on and charges for
//! 1 s, fires a 275-unit beam for a single update, then lets it fade over
//! 0.5 s.

use macroquad::prelude::*;

use crate::enemies::{wrap_near, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// Tower::Tower: radius 22; default Sprite mass 1. Pool for type 19.
pub const RADIUS: f32 = 22.0;
pub const MASS: f32 = 1.0;
pub const POOL: usize = 30;
/// Tower::reset: 15 health x (wrapped survival levels + 1); 100 points.
const HEALTH: f32 = 15.0;
pub const POINTS: u32 = 100;
const TURN_SPEED: f32 = 360.0;
const RELOAD: f32 = 1.5;
/// Global::TOWER_GUN_ACTIVATE_LENGTH, TOWER_GUN_FIRE_LENGTH, TOWER_GUN_RADIUS.
const ACTIVATE_RANGE: f32 = 200.0;
pub const BEAM_LENGTH: f32 = 275.0;
pub const BEAM_RADIUS: f32 = 5.0;
const CHARGE: f32 = 1.0;
const FADE: f32 = 0.5;
/// tower.1.png is 64x64, anchored 35 units down: its centre sits 3 units
/// above the pivot.
pub const SPRITE_OFFSET: f32 = -3.0;

/// Tower::Tower: the beam polygon (triangle fan), firing toward -y.
pub const BEAM: [Vec2; 10] = [
    vec2(-1.0, 0.0),
    vec2(-4.0, 0.0),
    vec2(-4.0, -255.0),
    vec2(-4.0, -275.0),
    vec2(-1.0, -275.0),
    vec2(-1.0, -275.0),
    vec2(4.0, -275.0),
    vec2(4.0, -255.0),
    vec2(4.0, 0.0),
    vec2(1.0, 0.0),
];
const MAGENTA: (f32, f32, f32) = (1.0, 0.0, 0.61);
/// Tower::setSubstate(3): white core vertices, magenta edges at half.
const CORE: [bool; 10] = [true, false, false, false, true, true, false, false, false, true];
const FIRE_ALPHA: [f32; 10] = [1.0, 0.5, 0.5, 0.0, 1.0, 1.0, 0.0, 0.5, 0.5, 1.0];
/// Tower::updateSubstate(2): alpha per second of charge, per vertex.
const CHARGE_RATE: [f32; 10] = [0.25, 0.5, 0.5, 0.0, 0.25, 0.25, 0.0, 0.5, 0.5, 0.25];

#[derive(Clone, Copy, PartialEq)]
enum Substate {
    Aim,
    Charge,
    Fire,
    Fade,
}

pub struct Tower {
    pub pos: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    substate: Substate,
    timer: f32,
    /// Locked firing direction (0x98).
    pub aim: Vec2,
}

impl Tower {
    pub fn new(pos: Vec2) -> Tower {
        Tower {
            pos,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: HEALTH,
            flash: false,
            substate: Substate::Aim,
            timer: 0.0,
            aim: vec2(0.0, -1.0),
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Whether the beam is live this update (Tower::updateCollisionSpace).
    pub fn firing(&self) -> bool {
        self.active && self.substate == Substate::Fire
    }

    fn set_substate(&mut self, s: Substate, ship: Vec2) {
        self.timer = 0.0;
        self.substate = s;
        if s == Substate::Charge {
            self.aim = (ship - self.pos).normalize_or_zero();
        }
    }

    /// Tower::update / updateSubstate.
    pub fn update(&mut self, dt: f32, ship: Vec2, centre: Vec2) {
        self.flash = false;
        let ship = vec2(wrap_near(ship.x, self.pos.x), wrap_near(ship.y, self.pos.y));
        if self.active {
            self.timer += dt;
            match self.substate {
                Substate::Aim => {
                    self.rotate_towards(ship, dt);
                    if self.timer >= RELOAD && ship.distance(self.pos) < ACTIVATE_RANGE {
                        self.set_substate(Substate::Charge, ship);
                    }
                }
                Substate::Charge => {
                    if self.timer >= CHARGE {
                        self.set_substate(Substate::Fire, ship);
                    }
                }
                Substate::Fire => self.set_substate(Substate::Fade, ship),
                Substate::Fade => {
                    if self.timer >= FADE {
                        self.set_substate(Substate::Aim, ship);
                    }
                }
            }
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

    /// Beam vertex colours for Tower::draw, or None when no beam shows.
    pub fn beam_colors(&self) -> Option<[Color; 10]> {
        let (r, g, b) = MAGENTA;
        let t = self.timer;
        let colors = match self.substate {
            Substate::Aim => return None,
            Substate::Charge => std::array::from_fn(|i| Color::new(r, g, b, t * CHARGE_RATE[i])),
            Substate::Fire | Substate::Fade => std::array::from_fn(|i| {
                let a = if self.substate == Substate::Fire {
                    FIRE_ALPHA[i]
                } else {
                    // Tower::updateSubstate(4): out to nothing over 0.5 s.
                    FIRE_ALPHA[i] * (1.0 - 2.0 * t)
                };
                if CORE[i] {
                    Color::new(1.0, 1.0, 1.0, a)
                } else {
                    Color::new(r, g, b, a)
                }
            }),
        };
        Some(colors)
    }

    /// Tower::handleEvent(0): true when the bullet destroys it.
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
