//! The boss (`BossInside`, enemy kind 22) and its shield pieces
//! (`BossOutside`, kind 23), ported from the iOS binary.
//!
//! The core chases the ship while pieces orbit it. Five patterns, picked
//! per world and stage, decide whether the pieces are flung (at the ship,
//! outward, or orbiting a moving point), how fast they spin, and how many
//! small rocks the core throws out between attacks.

use macroquad::prelude::*;

use crate::gravity::Grav;
use crate::enemies::{wrap_near, Element, DROP_IN_HEIGHT, DROP_IN_SPEED, FADE_IN_START, FADE_IN_SPEED};

/// BossInside::BossInside: cpCircleShapeNew radius 16, mass 30.
pub const RADIUS: f32 = 16.0;
pub const MASS: f32 = 30.0;
/// BossOutside::BossOutside: radius 13, mass 10.
pub const PIECE_RADIUS: f32 = 13.0;
pub const PIECE_MASS: f32 = 10.0;
/// EnemyManager pool for type 23.
const PIECE_POOL: usize = 36;
/// BossInside::reset: 1000 points; health 20 x (1 + stage/4 + 1.5 x world).
pub const POINTS: u32 = 1000;
const BASE_HEALTH: f32 = 20.0;
/// BossOutside::reset: 20 health each.
const PIECE_HEALTH: f32 = 20.0;
/// Shared Sprite settings: drag 0.3, restitution 0.9.
const DRAG: f32 = 0.3;
pub const RESTITUTION: f32 = 0.9;
/// BossInside::reset: the core turns at -100 degrees/s.
const SPIN: f32 = -100.0;
/// BossOutside::reset: pieces turn at up to 125 degrees/s either way.
const PIECE_SPIN: f32 = 125.0;
/// BossInside::setSubstate: pieces start 35 units out.
const ORBIT_RADIUS: f32 = 35.0;
/// BossOutside::detach: a free piece lives 3 s and spins at 700 degrees/s.
const FREE_LIFE: f32 = 3.0;
const FREE_SPIN: f32 = 700.0;
/// BossOutside::update: free pieces away from the screen slow to 150.
const FREE_SPEED_LIMIT: f32 = 150.0;
const NEAR_X: f32 = 306.7;
const NEAR_Y: f32 = 226.7;
/// Flung pieces fly at 200; type 2 orbits its moving centre at 250 deg/s.
const FLING_SPEED: f32 = 200.0;
const FLING_ORBIT_SPIN: f32 = 250.0;
/// Ramming the core (once per ship life, or once per boost) costs 7.5.
pub const RAM_DAMAGE: f32 = 7.5;
/// BossInside's animation: eight frames, 0.05 s each.
const FRAME_SECONDS: f32 = 0.05;
pub const FRAMES: usize = 8;
/// BossInside::activateRocks: rocks land 65-85 units out, within 60
/// degrees of the ship's direction.
const ROCK_DISTANCE: f32 = 65.0;
const ROCK_DISTANCE_JITTER: f32 = 20.0;
const ROCK_SPREAD: f32 = 60.0;
/// BossInside::draw: the health bar, 30x3, 15 left and 22 above the centre.
pub const BAR_SIZE: Vec2 = vec2(30.0, 3.0);
pub const BAR_OFFSET: Vec2 = vec2(-15.0, -22.0);

/// BossInside::reset, per world and stage: attack pattern, pieces, rocks
/// thrown per attack, their element (1 normal, 5 ice, 9 fire) and the
/// chase strength.
struct Config {
    pattern: u8,
    pieces: usize,
    rocks: usize,
    element: u8,
    speed: f32,
}

const fn c(pattern: u8, pieces: usize, rocks: usize, element: u8, speed: f32) -> Config {
    Config { pattern, pieces, rocks, element, speed }
}

/// Decoded from the jump tables in BossInside::reset.
const CONFIG: [[Config; 5]; 6] = [
    [c(0, 1, 0, 1, 100.0), c(3, 2, 0, 1, 100.0), c(2, 2, 0, 1, 100.0), c(1, 2, 0, 1, 100.0), c(4, 6, 0, 1, 100.0)],
    [c(0, 2, 5, 5, 150.0), c(3, 3, 5, 5, 150.0), c(2, 3, 5, 5, 150.0), c(1, 3, 5, 5, 150.0), c(4, 6, 5, 5, 150.0)],
    [c(0, 3, 10, 9, 200.0), c(3, 4, 10, 9, 200.0), c(2, 4, 10, 9, 200.0), c(1, 4, 10, 9, 200.0), c(4, 6, 10, 9, 200.0)],
    [c(0, 4, 15, 5, 250.0), c(3, 5, 15, 5, 250.0), c(2, 5, 15, 5, 250.0), c(1, 5, 15, 5, 250.0), c(4, 6, 15, 5, 250.0)],
    [c(0, 5, 18, 9, 300.0), c(3, 6, 18, 9, 300.0), c(2, 6, 18, 9, 300.0), c(1, 6, 18, 9, 300.0), c(4, 6, 18, 9, 300.0)],
    [c(0, 5, 20, 5, 350.0), c(3, 6, 20, 9, 350.0), c(2, 6, 20, 1, 350.0), c(1, 6, 20, 5, 350.0), c(4, 6, 20, 9, 350.0)],
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BossState {
    DropIn,
    Active,
    /// 10006: the charge-up effect plays, then the core explodes.
    Dying,
}

pub struct Boss {
    pub pos: Vec2,
    impulse: Vec2,
    pub angle: f32,
    pub z: f32,
    pub alpha: f32,
    pub state: BossState,
    pub health: f32,
    pub max_health: f32,
    pub flash: bool,
    pub frame: usize,
    frame_timer: f32,
    config: &'static Config,
    /// 0xa4: how fast the pieces orbit, degrees/s.
    orbit_speed: f32,
    substate: u8,
    timer: f32,
    /// Indices into `Field::pieces` of the attached pieces, in order.
    pub attached: Vec<usize>,
    /// The ship life and boost that last rammed the core.
    last_ram: Option<u32>,
    last_boost: Option<u32>,
    /// Handle of the dying charge-up effect.
    pub death_effect: u64,
    /// Killed by a bomb: worth half (BossInside::handleEvent(10)).
    pub bombed: bool,
    /// Killed by something thrown with the gravity gun: worth double.
    pub thrown_at: bool,
}

pub struct Piece {
    pub pos: Vec2,
    pub vel: Vec2,
    impulse: Vec2,
    pub angle: f32,
    spin: f32,
    pub z: f32,
    pub alpha: f32,
    active: bool,
    health: f32,
    pub flash: bool,
    pub attached: bool,
    /// 0x7c/0x80: orbit angle (degrees) and radius around the core.
    orbit: f32,
    radius: f32,
    /// 0x84..0x90: pattern 4's spiral base angle, base radius and rates.
    base_orbit: f32,
    base_radius: f32,
    orbit_rate: f32,
    radius_rate: f32,
    /// Free flight: Some((centre, centre velocity, orbit spin)) for
    /// pattern 2's orbiting throw, None for a straight throw.
    carrier: Option<(Vec2, Vec2, f32)>,
    life: f32,
    /// In the gravity gun's grip, or just thrown by it.
    pub grav: Option<Grav>,
}

/// Something the boss asks the field to do.
pub enum Request {
    Rock(Element, Vec2),
}

fn unit_dir(degrees: f32) -> Vec2 {
    let a = degrees.to_radians();
    vec2(a.cos(), a.sin())
}

fn signed() -> f32 {
    rand::gen_range(-1.0, 1.0)
}

fn near(v: Vec2, centre: Vec2) -> Vec2 {
    vec2(wrap_near(v.x, centre.x), wrap_near(v.y, centre.y))
}

/// Brings an angle within 180 degrees of `reference`.
fn around(mut a: f32, reference: f32) -> f32 {
    while a - reference > 180.0 {
        a -= 360.0;
    }
    while a - reference < -180.0 {
        a += 360.0;
    }
    a
}

impl Piece {
    pub fn new(pos: Vec2) -> Piece {
        Piece {
            pos,
            vel: Vec2::ZERO,
            impulse: Vec2::ZERO,
            angle: rand::gen_range(0.0, 360.0),
            spin: signed() * PIECE_SPIN,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            active: false,
            health: PIECE_HEALTH,
            flash: false,
            attached: true,
            orbit: 0.0,
            radius: ORBIT_RADIUS,
            base_orbit: 0.0,
            base_radius: ORBIT_RADIUS,
            orbit_rate: 0.0,
            radius_rate: 0.0,
            carrier: None,
            life: 0.0,
            grav: None,
        }
    }

    pub fn active(&self) -> bool {
        self.active && self.grav.is_none()
    }

    /// BossOutside::handleEvent(5): a free piece comes to the gravity gun.
    pub fn catchable(&self) -> bool {
        self.active() && !self.attached
    }

    /// BossOutside::detach.
    fn detach(&mut self, vel: Vec2, carrier: Option<(Vec2, Vec2, f32)>) {
        self.attached = false;
        self.life = FREE_LIFE;
        self.spin = FREE_SPIN;
        match carrier {
            None => self.vel = vel,
            Some(c) => self.carrier = Some(c),
        }
    }

    /// BossOutside::update while a lobber holds it: it rides at `pos`.
    pub fn hold(&mut self, dt: f32, pos: Vec2) {
        self.flash = false;
        self.pos = pos;
        if !self.active {
            self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
            self.alpha += dt * FADE_IN_SPEED;
            if self.alpha >= 1.0 {
                self.active = true;
                self.alpha = 1.0;
                self.z = 0.0;
            }
        } else {
            self.angle = (self.angle + self.spin * dt) % 360.0;
        }
    }

    /// BossOutside::detach(0, v): thrown free.
    pub fn throw(&mut self, vel: Vec2) {
        self.detach(vel, None);
    }

    /// BossOutside::update. Returns true when a free piece runs out of time.
    pub fn update(&mut self, dt: f32, centre: Vec2) -> bool {
        self.flash = false;
        if self.grav.is_some() {
            return false;
        }
        let mut expired = false;
        if !self.active {
            self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
            self.alpha += dt * FADE_IN_SPEED;
            if self.alpha >= 1.0 {
                self.active = true;
                self.alpha = 1.0;
                self.z = 0.0;
            }
        } else {
            if !self.attached {
                match &mut self.carrier {
                    None => {
                        let d = near(self.pos, centre) - centre;
                        if !(d.x.abs() < NEAR_X && d.y.abs() < NEAR_Y) && self.vel.length() > FREE_SPEED_LIMIT {
                            self.vel = self.vel.normalize() * FREE_SPEED_LIMIT;
                        }
                    }
                    Some((at, vel, spin)) => {
                        self.orbit = (self.orbit + dt * *spin) % 360.0;
                        *at += *vel * dt;
                        self.vel = (*at + unit_dir(self.orbit) * self.radius - self.pos) / dt;
                    }
                }
            }
            // Sprite::update.
            self.pos += self.vel * dt;
            self.angle = (self.angle + self.spin * dt) % 360.0;
            self.pos += self.impulse * dt;
            self.impulse *= DRAG / (dt + DRAG);
            if !self.attached {
                self.life -= dt;
                expired = self.life <= 0.0;
            }
        }
        self.pos = near(self.pos, centre);
        if let Some((at, _, _)) = &mut self.carrier {
            *at = near(*at, centre);
        }
        expired
    }

    /// BossOutside::handleEvent(0): true when the bullet destroys it.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }

    /// Whether a free piece still has time left (it whooshes when shot).
    pub fn free_with_time(&self) -> bool {
        !self.attached && self.life > 0.0
    }
}

impl Boss {
    /// BossInside::reset for arcade `world` 0-5 and `stage` 0-4.
    pub fn new(pos: Vec2, world: usize, stage: usize) -> Boss {
        let config = &CONFIG[world.min(5)][stage.min(4)];
        let health = BASE_HEALTH * (1.0 + stage as f32 * 0.25 + world as f32 * 1.5);
        Boss {
            pos,
            impulse: Vec2::ZERO,
            angle: 0.0,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            state: BossState::DropIn,
            health,
            max_health: health,
            flash: false,
            frame: 0,
            frame_timer: 0.0,
            config,
            orbit_speed: 0.0,
            substate: 0,
            timer: 0.0,
            attached: Vec::new(),
            last_ram: None,
            last_boost: None,
            death_effect: 0,
            bombed: false,
            thrown_at: false,
        }
    }

    pub fn active(&self) -> bool {
        self.state == BossState::Active
    }

    fn animate(&mut self, dt: f32) {
        let t = self.frame_timer + dt;
        self.frame = (self.frame + (t / FRAME_SECONDS) as usize) % FRAMES;
        self.frame_timer = t % FRAME_SECONDS;
    }

    /// WaveManager::updateWave: BossInside::resetBossOutsides on spawn.
    pub fn start(&mut self, pieces: &mut Vec<Piece>, ship: Vec2, out: &mut Vec<Request>) {
        self.orbit_speed = 0.0;
        self.attached.clear();
        self.set_substate(1, pieces, ship, out);
    }

    /// Fixes up piece indices after `pieces.swap_remove(removed)`.
    pub fn renumber(&mut self, removed: usize, len_before: usize) {
        let last = len_before - 1;
        self.attached.retain(|&i| i != removed);
        for i in &mut self.attached {
            if *i == last {
                *i = removed;
            }
        }
    }

    fn create_pieces(&mut self, pieces: &mut Vec<Piece>, spiral: bool) {
        let n = self.config.pieces;
        for i in 0..n {
            if pieces.len() >= PIECE_POOL {
                break;
            }
            let mut p = Piece::new(Vec2::ZERO);
            p.orbit = if self.config.pattern == 0 { i as f32 * 360.0 / 6.0 } else { i as f32 * 360.0 / n as f32 };
            p.radius = ORBIT_RADIUS;
            if spiral {
                p.base_orbit = p.orbit;
                p.base_radius = p.radius;
                let (rate, grow) = match i % 3 {
                    0 => (0.0, 0.0),
                    1 => (288.0 / n as f32, 25.0),
                    _ => (576.0 / n as f32, 50.0),
                };
                p.orbit_rate = rate;
                p.radius_rate = grow;
            }
            p.pos = self.pos + unit_dir(p.orbit) * p.radius;
            pieces.push(p);
            self.attached.push(pieces.len() - 1);
        }
    }

    /// BossInside::activateRocks.
    fn throw_rocks(&self, ship: Vec2, out: &mut Vec<Request>) {
        let element = match self.config.element {
            5 => Element::Ice,
            9 => Element::Fire,
            _ => Element::Normal,
        };
        let towards = heading(near(ship, self.pos) - self.pos);
        for _ in 0..self.config.rocks {
            let a = (towards + signed() * ROCK_SPREAD).to_radians();
            let d = ROCK_DISTANCE + rand::gen_range(0.0, 1.0) * ROCK_DISTANCE_JITTER;
            out.push(Request::Rock(element, self.pos + vec2(a.sin(), -a.cos()) * d));
        }
    }

    /// BossInside::setSubstate.
    fn set_substate(&mut self, n: u8, pieces: &mut Vec<Piece>, ship: Vec2, out: &mut Vec<Request>) {
        self.timer = 0.0;
        self.substate = n;
        match (self.config.pattern, n) {
            (0, _) | (1..=3, 1) => self.create_pieces(pieces, false),
            (4, 1) => self.create_pieces(pieces, true),
            (1..=3, 2) | (4, 6) => self.throw_rocks(ship, out),
            (2, 4) => {
                self.orbit_speed = 0.0;
                let vel = (near(ship, self.pos) - self.pos).normalize_or_zero() * FLING_SPEED;
                for i in std::mem::take(&mut self.attached) {
                    pieces[i].detach(Vec2::ZERO, Some((self.pos, vel, FLING_ORBIT_SPIN)));
                }
                self.set_substate(5, pieces, ship, out);
            }
            (3, 4) => {
                self.orbit_speed = 0.0;
                for i in std::mem::take(&mut self.attached) {
                    let out_dir = (pieces[i].pos - self.pos).normalize_or_zero();
                    pieces[i].detach(out_dir * FLING_SPEED, None);
                }
                self.set_substate(5, pieces, ship, out);
            }
            (4, 4) => {
                self.orbit_speed = 0.0;
                for i in std::mem::take(&mut self.attached) {
                    let vel = pieces[i].vel;
                    pieces[i].detach(vel, None);
                }
                self.set_substate(5, pieces, ship, out);
            }
            _ => {}
        }
    }

    /// Spin-up toward `accel` degrees/s^2 against the 0.3 s drag.
    fn spin_up(&mut self, dt: f32, accel: f32) {
        self.orbit_speed = (self.orbit_speed + dt * accel) * (DRAG / (dt + DRAG));
    }

    fn chase(&mut self, dt: f32, dir: Vec2) {
        self.impulse += dir * dt * self.config.speed;
    }

    /// BossInside::update. `ship` is the ship's position.
    pub fn update(&mut self, dt: f32, pieces: &mut Vec<Piece>, ship: Vec2, centre: Vec2, out: &mut Vec<Request>) {
        self.flash = false;
        match self.state {
            BossState::DropIn => {
                self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
                self.alpha += dt * FADE_IN_SPEED;
                if self.alpha >= 1.0 {
                    self.state = BossState::Active;
                    self.alpha = 1.0;
                    self.z = 0.0;
                }
                self.animate(dt);
            }
            BossState::Active => {
                self.sprite_update(dt);
                self.update_pieces(dt, pieces, ship, out);
            }
            BossState::Dying => self.sprite_update(dt),
        }
        self.pos = near(self.pos, centre);
    }

    fn sprite_update(&mut self, dt: f32) {
        self.angle = (self.angle + SPIN * dt) % 360.0;
        self.pos += self.impulse * dt;
        self.impulse *= DRAG / (dt + DRAG);
        self.animate(dt);
    }

    /// BossInside::updateBossOutsides.
    fn update_pieces(&mut self, dt: f32, pieces: &mut Vec<Piece>, ship: Vec2, out: &mut Vec<Request>) {
        self.timer += dt;
        let ship = near(ship, self.pos);
        let dir = (ship - self.pos).normalize_or_zero();
        match (self.config.pattern, self.substate) {
            (0, _) => {
                self.chase(dt, dir);
                self.spin_up(dt, 300.0);
            }
            (1, 1) | (2..=3, 1) => {
                self.chase(dt, dir);
                self.spin_up(dt, if self.config.pattern == 1 { 1000.0 } else { 600.0 });
                if self.timer > 1.5 {
                    self.set_substate(2, pieces, ship, out);
                }
            }
            (1, 2) => {
                self.spin_up(dt, 1000.0);
                if self.timer > 1.5 {
                    self.set_substate(4, pieces, ship, out);
                }
            }
            (1, 4) => self.fling_next(dt, pieces, ship, out),
            (2..=3, 2) => {
                self.spin_up(dt, 600.0);
                if self.timer > 1.5 {
                    self.set_substate(3, pieces, ship, out);
                }
            }
            (2..=3, 3) => {
                self.orbit_speed *= DRAG / (dt + DRAG);
                if self.timer > 1.0 {
                    self.set_substate(4, pieces, ship, out);
                }
            }
            (4, 1) => {
                self.chase(dt, dir);
                self.spin_up(dt, 350.0);
                if self.timer > 3.0 {
                    self.set_substate(6, pieces, ship, out);
                }
            }
            (4, 6) => {
                self.spin_up(dt, 350.0);
                for &i in &self.attached {
                    let p = &mut pieces[i];
                    p.radius = p.base_radius + p.radius_rate * self.timer;
                    p.orbit = p.base_orbit + p.orbit_rate * self.timer;
                }
                if self.timer > 2.0 {
                    self.set_substate(4, pieces, ship, out);
                }
            }
            (1..=4, 5) => {
                if self.timer > 2.0 {
                    self.set_substate(1, pieces, ship, out);
                }
            }
            _ => {}
        }
        // Attached pieces chase their places on the orbit.
        for &i in &self.attached {
            let p = &mut pieces[i];
            p.orbit = (p.orbit + dt * self.orbit_speed) % 360.0;
            p.base_orbit = (p.base_orbit + dt * self.orbit_speed) % 360.0;
            let target = self.pos + unit_dir(p.orbit) * p.radius;
            p.vel = (target - near(p.pos, self.pos)) / dt;
        }
    }

    /// Pattern 1, substate 4: release the first piece as it swings past
    /// a quarter turn before the ship's bearing, straight at the ship.
    fn fling_next(&mut self, dt: f32, pieces: &mut Vec<Piece>, ship: Vec2, out: &mut Vec<Request>) {
        let Some(&i) = self.attached.first() else {
            self.set_substate(5, pieces, ship, out);
            return;
        };
        let p = &pieces[i];
        let ahead = unit_dir((p.orbit + dt * self.orbit_speed) % 360.0);
        let now = (near(p.pos, self.pos) - self.pos).normalize_or_zero();
        let bearing = (ship - self.pos).normalize_or_zero();
        let mark = bearing.y.atan2(bearing.x).to_degrees() - 90.0;
        let next = around(ahead.y.atan2(ahead.x).to_degrees(), mark);
        if mark < next {
            let current = around(now.y.atan2(now.x).to_degrees(), mark);
            if current <= mark {
                let vel = (ship - near(p.pos, ship)).normalize_or_zero() * FLING_SPEED;
                self.attached.remove(0);
                pieces[i].detach(vel, None);
            }
        }
    }

    /// BossInside::handleEvent(0). Returns true when the core starts dying.
    pub fn hit(&mut self, damage: f32) -> bool {
        self.health -= damage;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }

    /// BossInside::handleEvent(3/4): the ship rams the core, once per life
    /// (`boost` false) or once per boost. Returns true when it dies.
    pub fn ram(&mut self, boost: bool, id: u32) -> bool {
        let last = if boost { &mut self.last_boost } else { &mut self.last_ram };
        if *last == Some(id) {
            return false;
        }
        *last = Some(id);
        self.health -= RAM_DAMAGE;
        if self.health > 0.0 {
            self.flash = true;
            false
        } else {
            true
        }
    }
}

/// Enemy::rotationTowards-style heading: 0 is up, clockwise in degrees.
fn heading(v: Vec2) -> f32 {
    90.0 + v.y.atan2(v.x).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one stage's boss for `seconds` with the ship parked 150 units
    /// away; returns (pieces thrown, attacks started, rocks requested).
    fn run(world: usize, stage: usize, seconds: f32) -> (usize, usize, usize) {
        let ship = vec2(150.0, 0.0);
        let mut boss = Boss::new(Vec2::ZERO, world, stage);
        let mut pieces = Vec::new();
        let mut out = Vec::new();
        boss.start(&mut pieces, ship, &mut out);
        let (mut thrown, mut created) = (0, pieces.len());
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt) as usize {
            let before = pieces.iter().filter(|p| p.attached).count();
            let len_before = pieces.len();
            boss.update(dt, &mut pieces, ship, Vec2::ZERO, &mut out);
            created += pieces.len() - len_before;
            let mut i = 0;
            while i < pieces.len() {
                if pieces[i].update(dt, Vec2::ZERO) {
                    let len = pieces.len();
                    pieces.swap_remove(i);
                    boss.renumber(i, len);
                } else {
                    i += 1;
                }
            }
            let after = pieces.iter().filter(|p| p.attached).count();
            thrown += before.saturating_sub(after);
            assert!(boss.attached.iter().all(|&i| i < pieces.len() && pieces[i].attached));
        }
        (thrown, created, out.len())
    }

    #[test]
    fn pattern_zero_keeps_its_pieces() {
        let (thrown, created, rocks) = run(0, 0, 12.0);
        assert_eq!((thrown, created, rocks), (0, 1, 0));
    }

    #[test]
    fn throwing_patterns_cycle() {
        // World 2 (index 1): stages 2-5 use patterns 3, 2, 1 and 4 with 5 rocks.
        for stage in 1..5 {
            let (thrown, created, rocks) = run(1, stage, 12.0);
            let n = CONFIG[1][stage].pieces;
            assert!(thrown >= n, "stage {stage}: thrown {thrown}");
            assert!(created > n, "stage {stage}: no second wave of pieces");
            assert!(rocks >= 5, "stage {stage}: rocks {rocks}");
        }
    }
}
