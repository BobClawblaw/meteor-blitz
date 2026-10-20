//! Meteors (`Rock`, `NormalRock`, `IceRock`, `FireRock`, `Enemy`) and the
//! stage script runner (`WaveManager`), ported from the iOS binary.

use macroquad::prelude::*;

use crate::arrow::{self, Arrow};
use crate::boss::{self, Boss, BossState, Piece, Request};
use crate::depot::{self, Depot};
use crate::lobber::{self, Lobber};
use crate::mine::{self, Mine};
use crate::seeker::{self, Seeker};
use crate::shyguy::{self, ShyGuy};
use crate::snake::{self, Hit, Snake};
use crate::tower::{self, Tower};
use crate::waves::{Wave, ARCADE};

/// World::WIDTH / HEIGHT.
const WORLD: f32 = 1024.0;

// Enemy::setState / Enemy::update: enemies drop in from 400 units above the
// surface at 450 units/s, fading in from 25% at 0.84375/s.
pub const DROP_IN_HEIGHT: f32 = 400.0;
pub const DROP_IN_SPEED: f32 = 450.0;
pub const FADE_IN_START: f32 = 0.25;
pub const FADE_IN_SPEED: f32 = 0.84375;

// Rock::reset: drift up to 50 units/s per axis, spin up to 60 deg/s, drag
// 0.3, restitution 0.9; animation at 20 frames/s.
const SPAWN_DRIFT: f32 = 50.0;
const SPAWN_SPIN: f32 = 60.0;
const DRAG: f32 = 0.3;
const RESTITUTION: f32 = 0.9;
const FRAME_SECONDS: f32 = 0.05;
/// Rock::update caps speed near the screen (Global::COLLISION_TEST_DISTANCE).
const NEAR_SPEED_LIMIT: f32 = 150.0;
const NEAR_X: f32 = 306.7;
const NEAR_Y: f32 = 226.7;
/// Rock::setState: fragments fly out at up to MAX_BREAK_SPEED.
const MAX_BREAK_SPEED: f32 = 50.0;
/// Rock::reset: points per rock (doubled for gravity-gun kills).
pub const ROCK_POINTS: u32 = 10;
/// Wave kind for depots.
const KIND_DEPOT: i32 = 21;
/// Global::BOMB_RADIUS, and BossOutside::handleEvent(10) damage.
const BOMB_RADIUS: f32 = 250.0;
const PIECE_BOMB_DAMAGE: f32 = 10.0;
/// Wave kind for shy guys.
const KIND_SHY_GUY: i32 = 13;
/// Wave kind for lobbers.
const KIND_LOBBER: i32 = 20;
const LOBBER_WEIGHT: u32 = 4;
/// Lobber::setState(10007): Enemy::activateBonuses(50, 8).
const LOBBER_RINGS: u32 = 8;
/// EnemyManager pool for type 23 (boss pieces and lobber balls).
const BALL_POOL: usize = 36;
/// Wave kind for seekers.
const KIND_SEEKER: i32 = 18;
const SEEKER_WEIGHT: u32 = 2;
/// Seeker::setState(10007): Enemy::activateBonuses(50, 3).
const SEEKER_RINGS: u32 = 3;
/// Wave kind for towers.
const KIND_TOWER: i32 = 19;
const TOWER_WEIGHT: u32 = 4;
/// Tower::setState(10007): Enemy::activateBonuses(50, 8).
const TOWER_RINGS: u32 = 8;
/// Wave kind for arrows.
const KIND_ARROW: i32 = 14;
/// Wave kind for snakes.
const KIND_SNAKE: i32 = 16;
const SNAKE_WEIGHT: u32 = 4;
/// Snake::setState(10007): Enemy::activateBonuses(50, 8).
const SNAKE_RINGS: u32 = 8;
/// Wave kind for mines.
const KIND_MINE: i32 = 17;
/// WaveManager::getWaveRemaining weights.
const MINE_WEIGHT: u32 = 2;
/// Wave kind for the boss.
const KIND_BOSS: i32 = 22;
/// Enemy::isWithin(ship, 600): enemies cleared by the boss's death only
/// show their explosions this close to the ship.
const CLEAR_VISIBLE: f32 = 600.0;
/// Rock::handleEvent: hit sparks at most every 8 updates.
const HIT_SPARK_UPDATES: i32 = 8;

/// Collision radius and mass by size (Rock::Rock).
const RADIUS: [f32; 4] = [11.0, 27.0, 38.0, 52.0];
const MASS: [f32; 4] = [2.5, 6.25, 15.625, 39.0625];
/// Value of each size in small-rock units (WaveManager::getWaveRemaining).
const WEIGHT: [u32; 4] = [1, 3, 9, 27];
/// Pool sizes by size (EnemyManager::EnemyManager).
const POOL: [usize; 4] = [150, 50, 20, 20];

/// WaveManager: one tick is 1/100 s; waits of 1 s are inserted after a death.
const TICK: f32 = 0.01;
/// The next wave waits until at most this many units remain, at least 8.
const MIN_THRESHOLD: i32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Element {
    Normal,
    Ice,
    Fire,
}

/// Which weapon a hit came from (bullet `+0x34`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weapon {
    Cannon,
    Fire,
    Laser,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// 10001: falling onto the surface.
    DropIn,
    /// 10002: in play.
    Active,
}

/// Uniform in [0, 1), like Random::randomFloat * 2^-32.
fn unit() -> f32 {
    rand::gen_range(0.0, 1.0)
}

/// Uniform in [-1, 1), like Random::randomFloat * 2^-31 - 1.
fn signed() -> f32 {
    rand::gen_range(-1.0, 1.0)
}

/// `World::updateX/Y`: bring a coordinate within half a world of `centre`.
pub fn wrap_near(v: f32, centre: f32) -> f32 {
    let mut v = v;
    if (centre - v).abs() > WORLD * 0.5 {
        v += if v < centre { WORLD } else { -WORLD };
    }
    v
}

pub struct Rock {
    pub element: Element,
    /// 0 small .. 3 extra large.
    pub size: usize,
    pub pos: Vec2,
    pub vel: Vec2,
    impulse: Vec2,
    pub angle: f32,
    spin: f32,
    /// Height above the surface while dropping in (negative = towards the eye).
    pub z: f32,
    pub alpha: f32,
    state: State,
    health: f32,
    pub frame: usize,
    frame_timer: f32,
    frame_reverse: bool,
    frames: usize,
    spark_cooldown: i32,
    /// Set by `hit` when this hit should show sparks.
    sparked: bool,
    pub points: u32,
    /// Enemy 0x4d: scores and drops rings (not the boss's rocks).
    scored: bool,
}

impl Rock {
    fn new(element: Element, size: usize, pos: Vec2, frames: usize) -> Rock {
        // Rock::reset: ice and fire rocks are 1.5x as tough.
        let health = if element == Element::Normal { 1.0 } else { 1.5 };
        let mut rock = Rock {
            element,
            size,
            pos,
            vel: vec2(signed(), signed()) * SPAWN_DRIFT,
            impulse: Vec2::ZERO,
            angle: 0.0,
            spin: signed() * SPAWN_SPIN,
            z: -DROP_IN_HEIGHT,
            alpha: FADE_IN_START,
            state: State::DropIn,
            health,
            frame: 0,
            frame_timer: 0.0,
            frame_reverse: signed() > 0.0,
            frames,
            spark_cooldown: 0,
            sparked: false,
            points: ROCK_POINTS,
            scored: true,
        };
        // Animation::setRandomFrame.
        rock.frame = rand::gen_range(0, frames.max(1));
        rock
    }

    pub fn radius(&self) -> f32 {
        RADIUS[self.size]
    }

    pub fn active(&self) -> bool {
        self.state == State::Active
    }

    fn mass(&self) -> f32 {
        MASS[self.size]
    }

    fn animate(&mut self, dt: f32) {
        // Animation::update: whole frames elapse, the remainder carries.
        if self.frames == 0 {
            return;
        }
        let steps = ((dt + self.frame_timer) / FRAME_SECONDS) as usize;
        self.frame_timer = (dt + self.frame_timer) % FRAME_SECONDS;
        self.frame = if self.frame_reverse {
            (self.frame + self.frames * (steps / self.frames + 1) - steps) % self.frames
        } else {
            (self.frame + steps) % self.frames
        };
    }

    /// Rock::update after Enemy::update.
    fn update(&mut self, dt: f32, centre: Vec2) {
        match self.state {
            State::DropIn => {
                self.z = (self.z + dt * DROP_IN_SPEED).min(0.0);
                self.alpha += dt * FADE_IN_SPEED;
                if self.alpha >= 1.0 {
                    self.state = State::Active;
                    self.alpha = 1.0;
                    self.z = 0.0;
                }
                self.animate(dt);
            }
            State::Active => {
                let d = vec2(
                    wrap_near(self.pos.x, centre.x) - centre.x,
                    wrap_near(self.pos.y, centre.y) - centre.y,
                );
                if d.x.abs() < NEAR_X && d.y.abs() < NEAR_Y && self.vel.length() > NEAR_SPEED_LIMIT
                {
                    self.vel = self.vel.normalize() * NEAR_SPEED_LIMIT;
                }
                // Sprite::update.
                self.pos += self.vel * dt;
                self.angle = (self.angle + self.spin * dt) % 360.0;
                self.pos += self.impulse * dt;
                self.impulse *= DRAG / (dt + DRAG);
                self.animate(dt);
                self.spark_cooldown = (self.spark_cooldown - 1).max(0);
            }
        }
        // World::updateSpritePosition.
        self.pos = vec2(
            wrap_near(self.pos.x, centre.x),
            wrap_near(self.pos.y, centre.y),
        );
    }

    /// Rock::handleEvent(0): a bullet hit. Returns true when the rock dies.
    fn hit(
        &mut self,
        weapon: Weapon,
        damage: f32,
        bullet_pos: Vec2,
        bullet_vel: Vec2,
        bullet_mass: f32,
    ) -> bool {
        let factor = match (self.element, weapon) {
            (Element::Ice, Weapon::Fire) | (Element::Fire, Weapon::Laser) => 1.25,
            (Element::Normal, Weapon::Cannon) => 1.0,
            _ => 0.5,
        };
        self.health -= damage * factor;
        if self.health <= 0.0 {
            return true;
        }
        self.sparked = false;
        // Knock back with the bullet's momentum.
        let n = (self.pos - bullet_pos).normalize_or_zero();
        let closing = n.dot(self.vel) - n.dot(bullet_vel);
        let j = bullet_mass * (2.0 * closing / (bullet_mass + self.mass()));
        self.vel = (self.vel - n * j) * RESTITUTION;
        if self.spark_cooldown == 0 {
            self.spark_cooldown = HIT_SPARK_UPDATES;
            self.sparked = true;
        }
        false
    }
}

/// Enemy::bounceBoth: an elastic bounce between two rocks, damped by each
/// rock's restitution, only when they are closing.
fn bounce(a: &mut Rock, b: &mut Rock) {
    let d = a.pos - b.pos;
    let approach = d.dot(b.vel) - d.dot(a.vel);
    if approach <= 0.0 {
        return;
    }
    let n = d.normalize_or_zero();
    let closing = n.dot(a.vel) - n.dot(b.vel);
    let k = 2.0 * closing / (a.mass() + b.mass());
    let ja = b.mass() * k;
    let jb = a.mass() * k;
    a.vel = (a.vel - n * ja) * RESTITUTION;
    b.vel = (b.vel + n * jb) * RESTITUTION;
}

/// Enemy::bounceThis: `rock` bounces off a still body at `other`.
fn bounce_off(rock: &mut Rock, other: Vec2, other_mass: f32) {
    let d = rock.pos - other;
    if d.dot(-rock.vel) <= 0.0 {
        return;
    }
    let n = d.normalize_or_zero();
    let closing = n.dot(rock.vel);
    let j = other_mass * (2.0 * closing / (other_mass + rock.mass()));
    rock.vel = (rock.vel - n * j) * RESTITUTION;
}

/// Enemy::bounceBoth without restitution: new velocities of two bodies.
/// The first body's restitution is applied by the caller where it has one
/// (bounceOther applies only the second body's).
fn bounced(pa: Vec2, va: Vec2, ma: f32, pb: Vec2, vb: Vec2, mb: f32) -> (Vec2, Vec2) {
    let d = pa - pb;
    if d.dot(vb) - d.dot(va) <= 0.0 {
        return (va, vb);
    }
    let n = d.normalize_or_zero();
    let k = 2.0 * (n.dot(va) - n.dot(vb)) / (ma + mb);
    (va - n * (mb * k), vb + n * (ma * k))
}

/// What happened during an update, for effects, sounds and scoring.
pub enum Event {
    RockDestroyed {
        pos: Vec2,
        element: Element,
        points: u32,
        /// Rings to drop (Enemy::activateBonuses).
        rings: u32,
        /// Rock 0x78 / 0x79: whether it plays a sound and shows effects.
        sound: bool,
        effects: bool,
    },
    /// A hit that did not destroy the rock (sparks at most every 8 updates).
    RockHit {
        pos: Vec2,
        element: Element,
    },
    /// The rock broke into three; fragments fly out at `angle` + 0/120/240.
    RockSplit {
        pos: Vec2,
        angle: f32,
    },
    /// A bullet hit a depot that survived (Depot::handleEvent(0)).
    DepotHit,
    /// Depot::setState(10007): it drops a power-up.
    DepotDestroyed {
        pos: Vec2,
        points: u32,
    },
    /// Mine::handleEvent(0): a bullet hit; `sound` unless it killed it.
    MineHit {
        pos: Vec2,
        sound: bool,
        sparks: bool,
    },
    /// Lobber::setState(10007).
    LobberDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// Seeker::setState(10007).
    SeekerDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// Tower::setState(10007).
    TowerDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// Arrow::setState(10007).
    ArrowDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// SnakeBody::setState(10007).
    SegmentDestroyed {
        pos: Vec2,
        points: u32,
        sound: bool,
    },
    /// Snake::setState(10007).
    SnakeDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// ShyGuy::setState(10007).
    ShyGuyDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// Mine::setState(10007).
    MineDestroyed {
        pos: Vec2,
        points: u32,
        rings: u32,
        sound: bool,
        effects: bool,
    },
    /// A bullet hit the boss core or a piece without destroying it.
    BossHit,
    /// BossInside::setState(10006): the core starts its death throes.
    BossDying {
        pos: Vec2,
    },
    /// BossInside::setState(10007).
    BossDead {
        pos: Vec2,
        points: u32,
    },
    /// BossOutside::setState(10007).
    PieceDestroyed {
        pos: Vec2,
        sound: bool,
        effects: bool,
        attached: bool,
    },
    WaveIncoming,
}

/// All enemies on the field: rocks and depots so far.
pub struct Field {
    pub rocks: Vec<Rock>,
    pub depots: Vec<Depot>,
    pub mines: Vec<Mine>,
    pub shy_guys: Vec<ShyGuy>,
    pub snakes: Vec<Snake>,
    pub arrows: Vec<Arrow>,
    pub towers: Vec<Tower>,
    pub seekers: Vec<Seeker>,
    pub lobbers: Vec<Lobber>,
    pub boss: Option<Boss>,
    pub pieces: Vec<Piece>,
    /// Animation frame counts by size: small atlas, medium, large, extra large.
    frames: [usize; 4],
    pub events: Vec<Event>,
    /// The ship's position at the last update.
    centre: Vec2,
    /// Inside Field::bomb: kills score nothing (Enemy 0x74 = 0), but rings
    /// still drop.
    bombing: bool,
}

impl Field {
    pub fn new(frames: [usize; 4]) -> Field {
        Field {
            rocks: Vec::new(),
            depots: Vec::new(),
            mines: Vec::new(),
            shy_guys: Vec::new(),
            snakes: Vec::new(),
            arrows: Vec::new(),
            towers: Vec::new(),
            seekers: Vec::new(),
            lobbers: Vec::new(),
            boss: None,
            pieces: Vec::new(),
            frames,
            events: Vec::new(),
            centre: Vec2::ZERO,
            bombing: false,
        }
    }

    fn count(&self, element: Element, size: usize) -> usize {
        self.rocks
            .iter()
            .filter(|r| r.element == element && r.size == size)
            .count()
    }

    /// EnemyManager::activate: take a rock from its pool if one is free.
    fn spawn(&mut self, element: Element, size: usize, pos: Vec2) -> Option<usize> {
        if self.count(element, size) >= POOL[size] {
            return None;
        }
        self.rocks
            .push(Rock::new(element, size, pos, self.frames[size]));
        Some(self.rocks.len() - 1)
    }

    /// WaveManager::getWaveRemaining: everything left, in small-rock units.
    pub fn weighted_remaining(&self) -> u32 {
        self.rocks.iter().map(|r| WEIGHT[r.size]).sum::<u32>()
            + self.depots.len() as u32
            + self.mines.len() as u32 * MINE_WEIGHT
            + self.shy_guys.len() as u32
            + self.snakes.len() as u32 * SNAKE_WEIGHT
            + self.arrows.len() as u32
            + self.towers.len() as u32 * TOWER_WEIGHT
            + self.seekers.len() as u32 * SEEKER_WEIGHT
            + self.lobbers.len() as u32 * LOBBER_WEIGHT
            + self.boss.is_some() as u32
    }

    fn spawn_lobber(&mut self, pos: Vec2) {
        if self.lobbers.len() < lobber::POOL {
            self.lobbers.push(Lobber::new(pos));
        }
    }

    /// Lobber::setState(10007); a ball it still holds dies with it.
    pub fn destroy_lobber(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let l = self.lobbers.swap_remove(index);
        self.events.push(Event::LobberDestroyed {
            pos: l.pos,
            points: if score && !self.bombing { lobber::POINTS } else { 0 },
            rings: if score { LOBBER_RINGS } else { 0 },
            sound,
            effects,
        });
        if let Some(mut ball) = l.ball {
            // BossOutside::detach(0, 0, 0) then setState(10007): free, so it
            // whooshes and bursts.
            ball.throw(Vec2::ZERO);
            self.events.push(Event::PieceDestroyed {
                pos: ball.pos,
                sound: true,
                effects: true,
                attached: false,
            });
        }
    }

    /// The ball lobber `index` holds is destroyed.
    pub fn destroy_held_ball(&mut self, index: usize) {
        if let Some(ball) = self.lobbers[index].ball.take() {
            self.events.push(Event::PieceDestroyed {
                pos: ball.pos,
                sound: false,
                effects: true,
                attached: true,
            });
        }
    }

    /// Lobbers, and lobbers whose held ball, touch a circle (the ship).
    pub fn touching_lobbers(&self, centre: Vec2, radius: f32) -> (Vec<usize>, Vec<usize>) {
        let near = |p: Vec2, r: f32| {
            let c = vec2(wrap_near(p.x, centre.x), wrap_near(p.y, centre.y));
            c.distance_squared(centre) < (r + radius).powi(2)
        };
        let bodies = (0..self.lobbers.len())
            .filter(|&i| self.lobbers[i].active() && near(self.lobbers[i].pos, lobber::RADIUS))
            .collect();
        let balls = (0..self.lobbers.len())
            .filter(|&i| {
                self.lobbers[i]
                    .ball
                    .as_ref()
                    .is_some_and(|b| b.active() && near(b.pos, boss::PIECE_RADIUS))
            })
            .collect();
        (bodies, balls)
    }

    /// lobberCollision: lobbers push each other apart; towers, depots and
    /// boss parts push them away; rocks bounce off them.
    fn lobber_collisions(&mut self) {
        let n = self.lobbers.len();
        for i in 0..n {
            if !self.lobbers[i].active() {
                continue;
            }
            for j in i + 1..n {
                if !self.lobbers[j].active() {
                    continue;
                }
                let (left, right) = self.lobbers.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                let d = bp - a.pos;
                let r = lobber::RADIUS * 2.0;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    let push = d.normalize() * (r * r - d.length_squared()) * mine::REPEL;
                    a.impulse -= push;
                    b.impulse += push;
                }
            }
            let mut others: Vec<(Vec2, f32)> = Vec::new();
            others.extend(self.towers.iter().filter(|t| t.active()).map(|t| (t.pos, tower::RADIUS)));
            others.extend(self.depots.iter().filter(|d| d.active()).map(|d| (d.pos, depot::RADIUS)));
            others.extend(self.pieces.iter().filter(|p| p.active()).map(|p| (p.pos, boss::PIECE_RADIUS)));
            others.extend(self.boss.iter().filter(|b| b.active()).map(|b| (b.pos, boss::RADIUS)));
            let l = &mut self.lobbers[i];
            for (pos, radius) in others {
                let c = vec2(wrap_near(pos.x, l.pos.x), wrap_near(pos.y, l.pos.y));
                if c.distance_squared(l.pos) < (lobber::RADIUS + radius).powi(2) {
                    l.push_from(c, radius, mine::REPEL * 2.0);
                }
            }
            let lp = l.pos;
            for rock in self.rocks.iter_mut().filter(|r| r.active()) {
                let c = vec2(wrap_near(lp.x, rock.pos.x), wrap_near(lp.y, rock.pos.y));
                if c.distance_squared(rock.pos) < (rock.radius() + lobber::RADIUS).powi(2) {
                    bounce_off(rock, c, lobber::MASS);
                }
            }
        }
    }

    fn spawn_seeker(&mut self, pos: Vec2) {
        if self.seekers.len() < seeker::POOL {
            self.seekers.push(Seeker::new(pos));
        }
    }

    /// Seeker::setState(10007).
    pub fn destroy_seeker(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let k = self.seekers.swap_remove(index);
        self.events.push(Event::SeekerDestroyed {
            pos: k.pos,
            points: if score && !self.bombing { seeker::POINTS } else { 0 },
            rings: if score { SEEKER_RINGS } else { 0 },
            sound,
            effects,
        });
    }

    /// Active seekers touching a circle (the ship).
    pub fn touching_seekers(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.seekers.len())
            .filter(|&i| {
                let k = &self.seekers[i];
                let c = vec2(wrap_near(k.pos.x, centre.x), wrap_near(k.pos.y, centre.y));
                k.active() && c.distance_squared(centre) < (seeker::RADIUS + radius).powi(2)
            })
            .collect()
    }

    /// seekerCollision: seekers push each other and snake heads apart;
    /// rocks, mines and fixed obstacles push them away.
    fn seeker_collisions(&mut self) {
        let n = self.seekers.len();
        for i in 0..n {
            if !self.seekers[i].active() {
                continue;
            }
            for j in i + 1..n {
                if !self.seekers[j].active() {
                    continue;
                }
                let (left, right) = self.seekers.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                let d = bp - a.pos;
                let r = seeker::RADIUS * 2.0;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    let push = d.normalize() * (r * r - d.length_squared()) * mine::REPEL;
                    a.impulse -= push;
                    b.impulse += push;
                }
            }
            let mut others: Vec<(Vec2, f32, f32)> = Vec::new();
            others.extend(self.rocks.iter().filter(|r| r.active()).map(|r| (r.pos, r.radius(), 2.0)));
            others.extend(self.mines.iter().filter(|m| m.active()).map(|m| (m.pos, mine::RADIUS, 1.0)));
            others.extend(self.snakes.iter().filter(|s| s.active()).map(|s| (s.pos, snake::RADIUS, 1.0)));
            others.extend(self.obstacles().into_iter().map(|(p, r)| (p, r, 2.0)));
            let k = &mut self.seekers[i];
            for (pos, radius, f) in others {
                let c = vec2(wrap_near(pos.x, k.pos.x), wrap_near(pos.y, k.pos.y));
                if c.distance_squared(k.pos) < (seeker::RADIUS + radius).powi(2) {
                    k.push_from(c, radius, mine::REPEL * f);
                }
            }
        }
    }

    fn spawn_tower(&mut self, pos: Vec2) {
        if self.towers.len() < tower::POOL {
            self.towers.push(Tower::new(pos));
        }
    }

    /// Tower::setState(10007).
    pub fn destroy_tower(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let t = self.towers.swap_remove(index);
        self.events.push(Event::TowerDestroyed {
            pos: t.pos,
            points: if score && !self.bombing { tower::POINTS } else { 0 },
            rings: if score { TOWER_RINGS } else { 0 },
            sound,
            effects,
        });
    }

    /// Whether a live tower beam touches a circle (towerGunCollision).
    pub fn beam_hits(&self, centre: Vec2, radius: f32) -> bool {
        self.towers.iter().filter(|t| t.firing()).any(|t| {
            let c = vec2(wrap_near(centre.x, t.pos.x), wrap_near(centre.y, t.pos.y));
            segment_distance(c, t.pos, t.pos + t.aim * tower::BEAM_LENGTH) < tower::BEAM_RADIUS + radius
        })
    }

    /// Positions and radii of fixed obstacles that push lighter enemies:
    /// towers, depots and boss parts.
    fn obstacles(&self) -> Vec<(Vec2, f32)> {
        let mut out = Vec::new();
        out.extend(self.towers.iter().filter(|t| t.active()).map(|t| (t.pos, tower::RADIUS)));
        out.extend(self.lobbers.iter().filter(|l| l.active()).map(|l| (l.pos, lobber::RADIUS)));
        out.extend(self.depots.iter().filter(|d| d.active()).map(|d| (d.pos, depot::RADIUS)));
        out.extend(self.pieces.iter().filter(|p| p.active()).map(|p| (p.pos, boss::PIECE_RADIUS)));
        out.extend(self.boss.iter().filter(|b| b.active()).map(|b| (b.pos, boss::RADIUS)));
        out
    }

    fn spawn_arrow(&mut self, pos: Vec2, heading: f32) {
        if self.arrows.len() < arrow::POOL {
            self.arrows.push(Arrow::new(pos, heading));
        }
    }

    /// Arrow::setState(10007).
    pub fn destroy_arrow(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let a = self.arrows.swap_remove(index);
        self.events.push(Event::ArrowDestroyed {
            pos: a.pos,
            points: if score && !self.bombing { arrow::POINTS } else { 0 },
            rings: score as u32,
            sound,
            effects,
        });
    }

    /// Active arrows touching a circle (the ship).
    pub fn touching_arrows(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.arrows.len())
            .filter(|&i| {
                let a = &self.arrows[i];
                let c = vec2(wrap_near(a.pos.x, centre.x), wrap_near(a.pos.y, centre.y));
                a.active() && c.distance_squared(centre) < (arrow::RADIUS + radius).powi(2)
            })
            .collect()
    }

    fn spawn_snake(&mut self, pos: Vec2, follow: bool) {
        if self.snakes.len() < snake::POOL {
            self.snakes.push(Snake::new(pos, follow));
        }
    }

    /// Snake::setState(10007): the head and every remaining segment.
    pub fn destroy_snake(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let s = self.snakes.swap_remove(index);
        for seg in &s.segments {
            self.events.push(Event::SegmentDestroyed {
                pos: seg.pos,
                points: if score { snake::SEGMENT_POINTS } else { 0 },
                sound: false,
            });
        }
        self.events.push(Event::SnakeDestroyed {
            pos: s.pos,
            points: if score && !self.bombing { snake::POINTS } else { 0 },
            rings: if score { SNAKE_RINGS } else { 0 },
            sound,
            effects,
        });
    }

    /// Applies a hit or ram result to snake `index`.
    fn snake_hit(&mut self, index: usize, hit: Hit, bullet: bool) {
        match hit {
            Hit::Hurt => self.events.push(Event::DepotHit),
            Hit::LostTail(pos) => {
                self.events.push(Event::SegmentDestroyed {
                    pos,
                    points: snake::SEGMENT_POINTS,
                    sound: true,
                });
                if bullet {
                    self.events.push(Event::DepotHit);
                }
            }
            Hit::Dead => self.destroy_snake(index, true, true, true),
        }
    }

    /// The ship touches a snake head: it loses its tail, or dies.
    pub fn ram_snake(&mut self, index: usize) {
        let hit = self.snakes[index].rammed();
        self.snake_hit(index, hit, false);
    }

    /// Snake heads and active segments touching a circle (the ship):
    /// (heads, any segment).
    pub fn touching_snakes(&self, centre: Vec2, radius: f32) -> (Vec<usize>, bool) {
        let near = |p: Vec2| {
            let c = vec2(wrap_near(p.x, centre.x), wrap_near(p.y, centre.y));
            c.distance_squared(centre) < (snake::RADIUS + radius).powi(2)
        };
        let heads = (0..self.snakes.len())
            .filter(|&i| self.snakes[i].active() && near(self.snakes[i].pos))
            .collect();
        let body = self
            .snakes
            .iter()
            .flat_map(|s| s.segments.iter())
            .any(|seg| seg.active_now() && near(seg.pos));
        (heads, body)
    }

    /// snakeCollision: heads push each other apart; rocks, mines, depots
    /// and boss parts push heads away.
    fn snake_collisions(&mut self) {
        let n = self.snakes.len();
        for i in 0..n {
            if !self.snakes[i].active() {
                continue;
            }
            for j in i + 1..n {
                if !self.snakes[j].active() {
                    continue;
                }
                let (left, right) = self.snakes.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                let d = bp - a.pos;
                let r = snake::RADIUS * 2.0;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    let push = d.normalize() * (r * r - d.length_squared()) * mine::REPEL;
                    a.impulse -= push;
                    b.impulse += push;
                }
            }
            let mut others: Vec<(Vec2, f32, f32)> = Vec::new();
            others.extend(self.rocks.iter().filter(|r| r.active()).map(|r| (r.pos, r.radius(), 2.0)));
            others.extend(self.mines.iter().filter(|m| m.active()).map(|m| (m.pos, mine::RADIUS, 1.0)));
            others.extend(self.obstacles().into_iter().map(|(p, r)| (p, r, 2.0)));
            let s = &mut self.snakes[i];
            for (pos, radius, k) in others {
                let c = vec2(wrap_near(pos.x, s.pos.x), wrap_near(pos.y, s.pos.y));
                let d = s.pos - c;
                let r = snake::RADIUS + radius;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    s.impulse += d.normalize() * (r * r - d.length_squared()) * mine::REPEL * k;
                }
            }
        }
    }

    fn spawn_shy_guy(&mut self, pos: Vec2) {
        if self.shy_guys.len() < shyguy::POOL {
            self.shy_guys.push(ShyGuy::new(pos));
        }
    }

    /// ShyGuy::setState(10007).
    pub fn destroy_shy_guy(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let g = self.shy_guys.swap_remove(index);
        self.events.push(Event::ShyGuyDestroyed {
            pos: g.pos,
            points: if score && !self.bombing { shyguy::POINTS } else { 0 },
            rings: score as u32,
            sound,
            effects,
        });
    }

    /// Active shy guys touching a circle (the ship).
    pub fn touching_shy_guys(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.shy_guys.len())
            .filter(|&i| {
                let g = &self.shy_guys[i];
                let c = vec2(wrap_near(g.pos.x, centre.x), wrap_near(g.pos.y, centre.y));
                g.active() && c.distance_squared(centre) < (shyguy::RADIUS + radius).powi(2)
            })
            .collect()
    }

    /// shyGuyCollision: shy guys push each other apart; rocks, mines,
    /// depots and boss parts push them away.
    fn shy_guy_collisions(&mut self) {
        let n = self.shy_guys.len();
        for i in 0..n {
            if !self.shy_guys[i].active() {
                continue;
            }
            for j in i + 1..n {
                if !self.shy_guys[j].active() {
                    continue;
                }
                let (left, right) = self.shy_guys.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                let d = bp - a.pos;
                let r = shyguy::RADIUS * 2.0;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    let push = d.normalize() * (r * r - d.length_squared()) * mine::REPEL;
                    a.impulse -= push;
                    b.impulse += push;
                }
            }
            let mut others: Vec<(Vec2, f32)> = Vec::new();
            others.extend(self.rocks.iter().filter(|r| r.active()).map(|r| (r.pos, r.radius())));
            others.extend(self.mines.iter().filter(|m| m.active()).map(|m| (m.pos, mine::RADIUS)));
            others.extend(self.obstacles());
            let g = &mut self.shy_guys[i];
            for (pos, radius) in others {
                let c = vec2(wrap_near(pos.x, g.pos.x), wrap_near(pos.y, g.pos.y));
                if c.distance_squared(g.pos) < (shyguy::RADIUS + radius).powi(2) {
                    g.push_from(c, radius, mine::REPEL * 2.0);
                }
            }
        }
    }

    fn spawn_mine(&mut self, pos: Vec2) {
        if self.mines.len() < mine::POOL {
            self.mines.push(Mine::new(pos));
        }
    }

    /// Mine::setState(10007).
    pub fn destroy_mine(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let m = self.mines.swap_remove(index);
        self.events.push(Event::MineDestroyed {
            pos: m.pos,
            points: if score && !self.bombing { mine::POINTS } else { 0 },
            rings: score as u32,
            sound,
            effects,
        });
    }

    /// Active mines touching a circle (the ship).
    pub fn touching_mines(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.mines.len())
            .filter(|&i| {
                let m = &self.mines[i];
                let c = vec2(wrap_near(m.pos.x, centre.x), wrap_near(m.pos.y, centre.y));
                m.active() && c.distance_squared(centre) < (mine::RADIUS + radius).powi(2)
            })
            .collect()
    }

    /// mineCollision and Rock::handleCollision against mines: mines push
    /// each other apart, and rocks, depots and boss parts push mines away.
    fn mine_collisions(&mut self) {
        let n = self.mines.len();
        for i in 0..n {
            if !self.mines[i].active() {
                continue;
            }
            for j in i + 1..n {
                if !self.mines[j].active() {
                    continue;
                }
                let (left, right) = self.mines.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                let d = bp - a.pos;
                let r = mine::RADIUS * 2.0;
                if d != Vec2::ZERO && d.length_squared() < r * r {
                    let push = d.normalize() * (r * r - d.length_squared()) * mine::REPEL;
                    a.impulse -= push;
                    b.impulse += push;
                }
            }
            let mut others: Vec<(Vec2, f32)> = Vec::new();
            others.extend(self.rocks.iter().filter(|r| r.active()).map(|r| (r.pos, r.radius())));
            others.extend(self.obstacles());
            let m = &mut self.mines[i];
            for (pos, radius) in others {
                let c = vec2(wrap_near(pos.x, m.pos.x), wrap_near(pos.y, m.pos.y));
                if c.distance_squared(m.pos) < (mine::RADIUS + radius).powi(2) {
                    m.push_from(c, radius, mine::REPEL);
                }
            }
        }
    }

    /// WaveManager::updateWave for kind 22.
    fn spawn_boss(&mut self, pos: Vec2, world: usize, stage: usize, ship: Vec2) {
        if self.boss.is_some() {
            return;
        }
        let mut b = Boss::new(pos, world, stage);
        let mut requests = Vec::new();
        b.start(&mut self.pieces, ship, &mut requests);
        self.boss = Some(b);
        self.run(requests);
    }

    fn run(&mut self, requests: Vec<Request>) {
        for r in requests {
            match r {
                Request::Rock(element, pos) => {
                    if let Some(i) = self.spawn(element, 0, pos) {
                        self.rocks[i].scored = false;
                    }
                }
            }
        }
    }

    /// BossOutside::setState(10007).
    pub fn destroy_piece(&mut self, index: usize, sound: bool, effects: bool) {
        let len = self.pieces.len();
        let p = self.pieces.swap_remove(index);
        if let Some(b) = &mut self.boss {
            b.renumber(index, len);
        }
        self.events.push(Event::PieceDestroyed {
            pos: p.pos,
            sound: sound && p.free_with_time(),
            effects,
            attached: p.attached,
        });
    }

    /// BossInside::setState(10006): the core starts dying and every other
    /// enemy is destroyed without score (EnemyManager::sharedHandleEvent(13)).
    pub fn boss_dying(&mut self) {
        let Some(b) = &mut self.boss else { return };
        if b.state == BossState::Dying {
            return;
        }
        b.state = BossState::Dying;
        let (pos, ship) = (b.pos, self.centre);
        self.events.push(Event::BossDying { pos });
        let visible = |p: Vec2| vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)).distance(ship) < CLEAR_VISIBLE;
        // Larger rocks go first, so their fragments are cleared too.
        while let Some(i) = (0..self.rocks.len()).max_by_key(|&i| self.rocks[i].size) {
            let effects = visible(self.rocks[i].pos);
            self.destroy_rock(i, false, false, effects);
        }
        while !self.depots.is_empty() {
            self.destroy_depot(0, false);
        }
        while !self.lobbers.is_empty() {
            let effects = visible(self.lobbers[0].pos);
            self.destroy_lobber(0, false, false, effects);
        }
        while !self.seekers.is_empty() {
            let effects = visible(self.seekers[0].pos);
            self.destroy_seeker(0, false, false, effects);
        }
        while !self.towers.is_empty() {
            let effects = visible(self.towers[0].pos);
            self.destroy_tower(0, false, false, effects);
        }
        while !self.arrows.is_empty() {
            let effects = visible(self.arrows[0].pos);
            self.destroy_arrow(0, false, false, effects);
        }
        while !self.snakes.is_empty() {
            let effects = visible(self.snakes[0].pos);
            self.destroy_snake(0, false, false, effects);
        }
        while !self.shy_guys.is_empty() {
            let effects = visible(self.shy_guys[0].pos);
            self.destroy_shy_guy(0, false, false, effects);
        }
        while !self.mines.is_empty() {
            let effects = visible(self.mines[0].pos);
            self.destroy_mine(0, false, false, effects);
        }
        while !self.pieces.is_empty() {
            let effects = visible(self.pieces[0].pos);
            self.destroy_piece(0, false, effects);
        }
    }

    /// The death effect has finished: BossInside::setState(10007).
    pub fn boss_dead(&mut self) {
        if let Some(b) = self.boss.take() {
            let points = if b.bombed { boss::POINTS / 2 } else { boss::POINTS };
            self.events.push(Event::BossDead { pos: b.pos, points });
        }
    }

    /// EnemyManager::sharedHandleEvent(10) from Ship::bomb: every enemy
    /// dropping in or active within 250 units of the ship dies without
    /// score or sound; the boss loses 7.5 health and every boss piece 10.
    pub fn bomb(&mut self) {
        let ship = self.centre;
        let near = |p: Vec2| vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)).distance(ship) < BOMB_RADIUS;
        self.bombing = true;
        while let Some(i) = (0..self.rocks.len()).filter(|&i| near(self.rocks[i].pos)).max_by_key(|&i| self.rocks[i].size) {
            self.destroy_rock(i, true, false, true);
        }
        while let Some(i) = (0..self.depots.len()).find(|&i| near(self.depots[i].pos)) {
            self.destroy_depot(i, true);
        }
        while let Some(i) = (0..self.mines.len()).find(|&i| near(self.mines[i].pos)) {
            self.destroy_mine(i, true, false, true);
        }
        while let Some(i) = (0..self.shy_guys.len()).find(|&i| near(self.shy_guys[i].pos)) {
            self.destroy_shy_guy(i, true, false, true);
        }
        while let Some(i) = (0..self.snakes.len()).find(|&i| near(self.snakes[i].pos)) {
            self.destroy_snake(i, true, false, true);
        }
        while let Some(i) = (0..self.arrows.len()).find(|&i| near(self.arrows[i].pos)) {
            self.destroy_arrow(i, true, false, true);
        }
        while let Some(i) = (0..self.towers.len()).find(|&i| near(self.towers[i].pos)) {
            self.destroy_tower(i, true, false, true);
        }
        while let Some(i) = (0..self.seekers.len()).find(|&i| near(self.seekers[i].pos)) {
            self.destroy_seeker(i, true, false, true);
        }
        while let Some(i) = (0..self.lobbers.len()).find(|&i| near(self.lobbers[i].pos)) {
            self.destroy_lobber(i, true, false, true);
        }
        self.bombing = false;
        let mut i = 0;
        while i < self.pieces.len() {
            if self.pieces[i].hit(PIECE_BOMB_DAMAGE) {
                self.destroy_piece(i, true, true);
            } else {
                i += 1;
            }
        }
        for i in 0..self.lobbers.len() {
            if self.lobbers[i].ball.as_mut().is_some_and(|b| b.hit(PIECE_BOMB_DAMAGE)) {
                self.destroy_held_ball(i);
            }
        }
        let dying = self.boss.as_mut().is_some_and(|b| {
            if b.state == BossState::Dying || !near(b.pos) {
                return false;
            }
            let dead = b.hit(boss::RAM_DAMAGE);
            b.bombed |= dead;
            dead
        });
        if dying {
            self.boss_dying();
        }
    }

    /// Active pieces touching a circle (the ship).
    pub fn touching_pieces(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.pieces.len())
            .filter(|&i| {
                let p = &self.pieces[i];
                let c = vec2(wrap_near(p.pos.x, centre.x), wrap_near(p.pos.y, centre.y));
                p.active() && c.distance_squared(centre) < (boss::PIECE_RADIUS + radius).powi(2)
            })
            .collect()
    }

    /// Whether the active boss core touches a circle (the ship).
    pub fn touching_boss(&self, centre: Vec2, radius: f32) -> bool {
        self.boss.as_ref().is_some_and(|b| {
            let c = vec2(wrap_near(b.pos.x, centre.x), wrap_near(b.pos.y, centre.y));
            b.active() && c.distance_squared(centre) < (boss::RADIUS + radius).powi(2)
        })
    }

    fn spawn_depot(&mut self, pos: Vec2) {
        if self.depots.len() < depot::POOL {
            self.depots.push(Depot::new(pos));
        }
    }

    /// Depot::setState(10007).
    pub fn destroy_depot(&mut self, index: usize, score: bool) {
        let d = self.depots.swap_remove(index);
        self.events.push(Event::DepotDestroyed {
            pos: d.pos,
            points: if score && !self.bombing { depot::POINTS } else { 0 },
        });
    }

    /// Active depots touching a circle (the ship).
    pub fn touching_depots(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.depots.len())
            .filter(|&i| {
                let d = &self.depots[i];
                let c = vec2(wrap_near(d.pos.x, centre.x), wrap_near(d.pos.y, centre.y));
                d.active() && c.distance_squared(centre) < (depot::RADIUS + radius).powi(2)
            })
            .collect()
    }

    /// WaveManager::getRemaining for one wave kind.
    fn remaining_of_kind(&self, kind: i32) -> usize {
        match rock_kind(kind) {
            Some((element, size)) => self.count(element, size),
            None if kind == KIND_DEPOT => self.depots.len(),
            None if kind == KIND_BOSS => self.boss.is_some() as usize,
            None if kind == KIND_MINE => self.mines.len(),
            None if kind == KIND_SHY_GUY => self.shy_guys.len(),
            None if kind == KIND_SNAKE => self.snakes.len(),
            None if kind == KIND_ARROW => self.arrows.len(),
            None if kind == KIND_TOWER => self.towers.len(),
            None if kind == KIND_SEEKER => self.seekers.len(),
            None if kind == KIND_LOBBER => self.lobbers.len(),
            None => 0,
        }
    }

    pub fn update(&mut self, dt: f32, centre: Vec2) {
        let ship_moved = vec2(
            wrap_near(centre.x, self.centre.x) - self.centre.x,
            wrap_near(centre.y, self.centre.y) - self.centre.y,
        );
        self.centre = centre;
        for rock in &mut self.rocks {
            rock.update(dt, centre);
        }
        if let Some(mut b) = self.boss.take() {
            let mut requests = Vec::new();
            b.update(dt, &mut self.pieces, centre, centre, &mut requests);
            self.boss = Some(b);
            self.run(requests);
        }
        let mut i = 0;
        while i < self.pieces.len() {
            if self.pieces[i].update(dt, centre) {
                // A thrown piece ran out of time: no whoosh, but it bursts.
                self.destroy_piece(i, false, true);
            } else {
                i += 1;
            }
        }
        self.boss_collisions();
        let ship = centre;
        for m in &mut self.mines {
            m.update(dt, ship, centre);
        }
        self.mine_collisions();
        for g in &mut self.shy_guys {
            g.update(dt, ship, centre);
        }
        self.shy_guy_collisions();
        for s in &mut self.snakes {
            s.update(dt, ship, ship_moved, centre);
        }
        for a in &mut self.arrows {
            a.update(dt, centre);
        }
        for t in &mut self.towers {
            t.update(dt, ship, centre);
        }
        for k in &mut self.seekers {
            k.update(dt, ship, centre);
        }
        for i in 0..self.lobbers.len() {
            let held = self.lobbers.iter().filter(|l| l.ball.is_some()).count();
            let can_load = self.pieces.len() + held < BALL_POOL;
            if let Some(ball) = self.lobbers[i].update(dt, ship, centre, can_load) {
                self.pieces.push(ball);
            }
        }
        self.lobber_collisions();
        self.seeker_collisions();
        // towerCollision: towers never move, but push depots (repelOther),
        // and rocks bounce off them (Enemy::bounceThis).
        let towers: Vec<Vec2> = self.towers.iter().filter(|t| t.active()).map(|t| t.pos).collect();
        // towerGunCollision: a live beam destroys rocks it touches, scored
        // (Rock::handleEvent(12)).
        let beams: Vec<(Vec2, Vec2)> = self
            .towers
            .iter()
            .filter(|t| t.firing())
            .map(|t| (t.pos, t.pos + t.aim * tower::BEAM_LENGTH))
            .collect();
        for (a, b) in beams {
            while let Some(i) = (0..self.rocks.len()).find(|&i| {
                let r = &self.rocks[i];
                let c = vec2(wrap_near(r.pos.x, a.x), wrap_near(r.pos.y, a.y));
                r.active() && segment_distance(c, a, b) < r.radius() + tower::BEAM_RADIUS
            }) {
                self.destroy(i, true);
            }
        }
        for &tp in &towers {
            for d in self.depots.iter_mut().filter(|d| d.active()) {
                let c = vec2(wrap_near(tp.x, d.pos.x), wrap_near(tp.y, d.pos.y));
                if c.distance_squared(d.pos) < (tower::RADIUS + depot::RADIUS).powi(2) {
                    d.repel_from(c, tower::RADIUS, mine::REPEL);
                }
            }
            for rock in self.rocks.iter_mut().filter(|r| r.active()) {
                let c = vec2(wrap_near(tp.x, rock.pos.x), wrap_near(tp.y, rock.pos.y));
                if c.distance_squared(rock.pos) < (rock.radius() + tower::RADIUS).powi(2) {
                    bounce_off(rock, c, tower::MASS);
                }
            }
        }
        self.snake_collisions();
        for d in &mut self.depots {
            d.update(dt, centre);
        }
        // depotCollision: depots push each other apart.
        let n = self.depots.len();
        for i in 0..n {
            for j in i + 1..n {
                let (left, right) = self.depots.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                if !a.active() || !b.active() {
                    continue;
                }
                let real = b.pos;
                b.pos = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                if a.pos.distance_squared(b.pos) < (depot::RADIUS * 2.0).powi(2) {
                    depot::repel_both(a, b, depot::REPEL);
                }
                b.pos = real;
            }
        }
        // Rock::handleCollision with a depot: Enemy::bounceThis, the rock alone.
        for rock in self.rocks.iter_mut().filter(|r| r.active()) {
            for d in self.depots.iter().filter(|d| d.active()) {
                let c = vec2(wrap_near(d.pos.x, rock.pos.x), wrap_near(d.pos.y, rock.pos.y));
                if c.distance_squared(rock.pos) < (rock.radius() + depot::RADIUS).powi(2) {
                    bounce_off(rock, c, depot::MASS);
                }
            }
        }
        // Rock::handleCollision with other rocks: bounce both.
        let n = self.rocks.len();
        for i in 0..n {
            for j in i + 1..n {
                let (left, right) = self.rocks.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                if !a.active() || !b.active() {
                    continue;
                }
                let d = vec2(
                    wrap_near(b.pos.x, a.pos.x) - a.pos.x,
                    wrap_near(b.pos.y, a.pos.y) - a.pos.y,
                );
                let r = a.radius() + b.radius();
                if d.length_squared() < r * r {
                    let real = b.pos;
                    b.pos = a.pos + d;
                    bounce(a, b);
                    b.pos = real;
                }
            }
        }
    }

    /// A bullet segment against active rocks. Returns true when it hit one.
    pub fn bullet(
        &mut self,
        from: Vec2,
        to: Vec2,
        radius: f32,
        weapon: Weapon,
        damage: f32,
        mass: f32,
        vel: Vec2,
    ) -> bool {
        for i in 0..self.rocks.len() {
            let rock = &self.rocks[i];
            if !rock.active() {
                continue;
            }
            let c = vec2(wrap_near(rock.pos.x, to.x), wrap_near(rock.pos.y, to.y));
            if segment_distance(c, from, to) < rock.radius() + radius {
                if self.rocks[i].hit(weapon, damage, to, vel, mass) {
                    self.destroy(i, true);
                } else if self.rocks[i].sparked {
                    let r = &self.rocks[i];
                    self.events.push(Event::RockHit {
                        pos: r.pos,
                        element: r.element,
                    });
                }
                return true;
            }
        }
        for i in 0..self.depots.len() {
            let d = &self.depots[i];
            if !d.active() {
                continue;
            }
            let c = vec2(wrap_near(d.pos.x, to.x), wrap_near(d.pos.y, to.y));
            if segment_distance(c, from, to) < depot::RADIUS + radius {
                let d = &mut self.depots[i];
                d.health -= damage;
                if d.health > 0.0 {
                    d.flash = true;
                    self.events.push(Event::DepotHit);
                } else {
                    self.destroy_depot(i, true);
                }
                return true;
            }
        }
        for i in 0..self.lobbers.len() {
            let l = &self.lobbers[i];
            if let Some(b) = l.ball.as_ref().filter(|b| b.active()) {
                let c = vec2(wrap_near(b.pos.x, to.x), wrap_near(b.pos.y, to.y));
                if segment_distance(c, from, to) < boss::PIECE_RADIUS + radius {
                    let dead = self.lobbers[i].ball.as_mut().is_some_and(|b| b.hit(damage));
                    if dead {
                        self.destroy_held_ball(i);
                    } else {
                        self.events.push(Event::BossHit);
                    }
                    return true;
                }
            }
            let c = vec2(wrap_near(l.pos.x, to.x), wrap_near(l.pos.y, to.y));
            if l.active() && segment_distance(c, from, to) < lobber::RADIUS + radius {
                if self.lobbers[i].hit(damage) {
                    self.destroy_lobber(i, true, true, true);
                } else {
                    self.events.push(Event::DepotHit);
                }
                return true;
            }
        }
        for i in 0..self.seekers.len() {
            let k = &self.seekers[i];
            let c = vec2(wrap_near(k.pos.x, to.x), wrap_near(k.pos.y, to.y));
            if k.active() && segment_distance(c, from, to) < seeker::RADIUS + radius {
                if self.seekers[i].hit(damage) {
                    self.destroy_seeker(i, true, true, true);
                } else {
                    self.events.push(Event::DepotHit);
                }
                return true;
            }
        }
        for i in 0..self.towers.len() {
            let t = &self.towers[i];
            let c = vec2(wrap_near(t.pos.x, to.x), wrap_near(t.pos.y, to.y));
            if t.active() && segment_distance(c, from, to) < tower::RADIUS + radius {
                if self.towers[i].hit(damage) {
                    self.destroy_tower(i, true, true, true);
                } else {
                    self.events.push(Event::DepotHit);
                }
                return true;
            }
        }
        for i in 0..self.arrows.len() {
            let a = &self.arrows[i];
            let c = vec2(wrap_near(a.pos.x, to.x), wrap_near(a.pos.y, to.y));
            if a.active() && segment_distance(c, from, to) < arrow::RADIUS + radius {
                if self.arrows[i].hit(damage) {
                    self.destroy_arrow(i, true, true, true);
                } else {
                    self.events.push(Event::DepotHit);
                }
                return true;
            }
        }
        for i in 0..self.snakes.len() {
            let s = &self.snakes[i];
            let head = vec2(wrap_near(s.pos.x, to.x), wrap_near(s.pos.y, to.y));
            if s.active() && segment_distance(head, from, to) < snake::RADIUS + radius {
                let hit = self.snakes[i].hit(damage);
                self.snake_hit(i, hit, true);
                return true;
            }
            // SnakeBody::handleEvent(0): segments stop bullets unharmed.
            let blocked = s.segments.iter().any(|seg| {
                let c = vec2(wrap_near(seg.pos.x, to.x), wrap_near(seg.pos.y, to.y));
                seg.active_now() && segment_distance(c, from, to) < snake::RADIUS + radius
            });
            if blocked {
                return true;
            }
        }
        for i in 0..self.shy_guys.len() {
            let g = &self.shy_guys[i];
            if !g.active() {
                continue;
            }
            let c = vec2(wrap_near(g.pos.x, to.x), wrap_near(g.pos.y, to.y));
            if segment_distance(c, from, to) < shyguy::RADIUS + radius {
                if self.shy_guys[i].hit(damage) {
                    self.destroy_shy_guy(i, true, true, true);
                } else {
                    self.events.push(Event::DepotHit);
                }
                return true;
            }
        }
        for i in 0..self.mines.len() {
            let m = &self.mines[i];
            if !m.active() {
                continue;
            }
            let c = vec2(wrap_near(m.pos.x, to.x), wrap_near(m.pos.y, to.y));
            if segment_distance(c, from, to) < mine::RADIUS + radius {
                let pos = m.pos;
                let (dead, sparks) = self.mines[i].hit(damage);
                if dead {
                    self.destroy_mine(i, true, true, true);
                }
                self.events.push(Event::MineHit { pos, sound: !dead, sparks });
                return true;
            }
        }
        for i in 0..self.pieces.len() {
            let p = &self.pieces[i];
            if !p.active() {
                continue;
            }
            let c = vec2(wrap_near(p.pos.x, to.x), wrap_near(p.pos.y, to.y));
            if segment_distance(c, from, to) < boss::PIECE_RADIUS + radius {
                if self.pieces[i].hit(damage) {
                    self.destroy_piece(i, true, true);
                } else {
                    self.events.push(Event::BossHit);
                }
                return true;
            }
        }
        if let Some(b) = &mut self.boss {
            let c = vec2(wrap_near(b.pos.x, to.x), wrap_near(b.pos.y, to.y));
            if b.active() && segment_distance(c, from, to) < boss::RADIUS + radius {
                if b.hit(damage) {
                    self.boss_dying();
                } else {
                    self.events.push(Event::BossHit);
                }
                return true;
            }
        }
        false
    }

    /// bossInsideCollision, bossOutsideCollision and Rock::handleCollision
    /// against boss parts: rocks and pieces bounce off the core, rocks
    /// bounce off pieces, pieces bounce off each other.
    fn boss_collisions(&mut self) {
        let core = self.boss.as_ref().filter(|b| b.active()).map(|b| b.pos);
        for rock in self.rocks.iter_mut().filter(|r| r.active()) {
            if let Some(c) = core {
                let c = vec2(wrap_near(c.x, rock.pos.x), wrap_near(c.y, rock.pos.y));
                if c.distance_squared(rock.pos) < (rock.radius() + boss::RADIUS).powi(2) {
                    bounce_off(rock, c, boss::MASS);
                }
            }
            for p in self.pieces.iter().filter(|p| p.active()) {
                let c = vec2(wrap_near(p.pos.x, rock.pos.x), wrap_near(p.pos.y, rock.pos.y));
                if c.distance_squared(rock.pos) < (rock.radius() + boss::PIECE_RADIUS).powi(2) {
                    bounce_off(rock, c, boss::PIECE_MASS);
                }
            }
        }
        let n = self.pieces.len();
        for i in 0..n {
            if !self.pieces[i].active() {
                continue;
            }
            if let Some(c) = core {
                let p = &mut self.pieces[i];
                let c = vec2(wrap_near(c.x, p.pos.x), wrap_near(c.y, p.pos.y));
                if c.distance_squared(p.pos) < (boss::PIECE_RADIUS + boss::RADIUS).powi(2) {
                    p.vel = bounced(p.pos, p.vel, boss::PIECE_MASS, c, Vec2::ZERO, boss::MASS).0 * boss::RESTITUTION;
                }
            }
            for j in i + 1..n {
                if !self.pieces[j].active() {
                    continue;
                }
                let (left, right) = self.pieces.split_at_mut(j);
                let (a, b) = (&mut left[i], &mut right[0]);
                let bp = vec2(wrap_near(b.pos.x, a.pos.x), wrap_near(b.pos.y, a.pos.y));
                if bp.distance_squared(a.pos) < (boss::PIECE_RADIUS * 2.0).powi(2) {
                    let (va, vb) = bounced(a.pos, a.vel, boss::PIECE_MASS, bp, b.vel, boss::PIECE_MASS);
                    a.vel = va * boss::RESTITUTION;
                    b.vel = vb * boss::RESTITUTION;
                }
            }
        }
    }

    /// Active rocks touching a circle (the ship), for collision handling.
    pub fn touching(&self, centre: Vec2, radius: f32) -> Vec<usize> {
        (0..self.rocks.len())
            .filter(|&i| {
                let r = &self.rocks[i];
                let c = vec2(wrap_near(r.pos.x, centre.x), wrap_near(r.pos.y, centre.y));
                r.active() && c.distance_squared(centre) < (r.radius() + radius).powi(2)
            })
            .collect()
    }

    /// Rock::setState(10007): remove the rock, score it, and break it into
    /// three of the next size down.
    pub fn destroy(&mut self, index: usize, score: bool) {
        self.destroy_rock(index, score, true, true);
    }

    fn destroy_rock(&mut self, index: usize, score: bool, sound: bool, effects: bool) {
        let rock = self.rocks.swap_remove(index);
        let score = score && rock.scored;
        self.events.push(Event::RockDestroyed {
            pos: rock.pos,
            element: rock.element,
            points: if score && !self.bombing { rock.points } else { 0 },
            // Rock::setState(10007): a scored small rock drops one ring.
            rings: if score && rock.size == 0 { 1 } else { 0 },
            sound,
            effects,
        });
        if rock.size == 0 {
            return;
        }
        let base = unit() * 180.0;
        self.events.push(Event::RockSplit {
            pos: rock.pos,
            angle: base,
        });
        let distance = 10.0 - signed();
        for k in 0..3 {
            let a = (base + 120.0 * k as f32).to_radians();
            let dir = vec2(a.cos(), a.sin());
            if let Some(i) = self.spawn(rock.element, rock.size - 1, rock.pos + dir * distance) {
                let child = &mut self.rocks[i];
                child.vel = rock.vel + child.vel * 0.1 + dir * unit() * MAX_BREAK_SPEED;
                child.state = State::Active;
                child.alpha = 1.0;
                child.z = 0.0;
            }
        }
    }
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// Rock element and size for a wave kind (1-12; 26-29 pick an element at random).
fn rock_kind(kind: i32) -> Option<(Element, usize)> {
    match kind {
        1..=4 => Some((Element::Normal, (kind - 1) as usize)),
        5..=8 => Some((Element::Ice, (kind - 5) as usize)),
        9..=12 => Some((Element::Fire, (kind - 9) as usize)),
        26..=29 => {
            let element = match (unit() * 3.0) as i32 {
                0 => Element::Ice,
                1 => Element::Fire,
                _ => Element::Normal,
            };
            Some((element, (kind - 26) as usize))
        }
        _ => None,
    }
}

/// WaveManager::getNumEnemiesInWave: what a wave adds, in small-rock units.
fn wave_units(wave: &Wave) -> i32 {
    let size = match wave.kind {
        1..=12 => Some(((wave.kind - 1) % 4) as usize),
        26..=29 => Some((wave.kind - 26) as usize),
        _ => None,
    };
    match size {
        Some(size) => wave.pattern.spawns.len() as i32 * WEIGHT[size] as i32,
        None => 0,
    }
}

/// `WaveManager` for arcade stages.
pub struct Script {
    waves: &'static [Wave],
    index: i32,
    spawning: bool,
    tick: i32,
    entry: usize,
    tick_timer: f32,
    rotation: f32,
    origin: Vec2,
    threshold: i32,
    tracked_kind: i32,
    /// One-second waits queued by a ship death (WaveManager::delay).
    delays: i32,
    delay_timer: f32,
    /// Global::level and Global::stage (0-based) for the boss.
    world: usize,
    stage: usize,
}

impl Script {
    pub fn arcade(world: usize, stage: usize) -> Script {
        Script {
            waves: ARCADE[world.min(5)][stage.clamp(1, 5) - 1],
            index: -1,
            spawning: false,
            tick: 0,
            entry: 0,
            tick_timer: 0.0,
            rotation: 0.0,
            origin: Vec2::ZERO,
            threshold: 0,
            tracked_kind: -1,
            delays: 0,
            delay_timer: 0.0,
            world: world.min(5),
            stage: stage.clamp(1, 5) - 1,
        }
        .skip_to_env_wave()
    }

    /// MB_WAVE=<n> starts the stage at wave n (0-based), for testing.
    fn skip_to_env_wave(mut self) -> Script {
        if let Some(n) = std::env::var("MB_WAVE").ok().and_then(|v| v.parse::<i32>().ok()) {
            self.index = n.clamp(0, self.waves.len() as i32) - 1;
        }
        self
    }

    pub fn delay(&mut self) {
        self.delays += 1;
    }

    /// WaveManager::isStageComplete.
    pub fn complete(&self, field: &Field) -> bool {
        self.index == self.waves.len() as i32 - 1
            && (field.remaining_of_kind(self.tracked_kind) == 0 || field.weighted_remaining() == 0)
    }

    /// WaveManager::update.
    pub fn update(&mut self, dt: f32, ship: Vec2, field: &mut Field) {
        if self.spawning {
            if self.spawn_due(dt, ship, field) {
                self.spawning = false;
            }
            return;
        }
        if self.index != -1 {
            if self.index >= self.waves.len() as i32 - 1
                || field.weighted_remaining() as i32 > self.threshold
            {
                return;
            }
            if self.delays > 0 {
                self.delay_timer += dt;
                if self.delay_timer < 1.0 {
                    return;
                }
                self.delay_timer = 0.0;
                self.delays -= 1;
            }
        }
        self.start_next(dt, ship, field);
    }

    /// WaveManager::startNextWave.
    fn start_next(&mut self, dt: f32, ship: Vec2, field: &mut Field) {
        self.index += 1;
        let wave = &self.waves[self.index as usize];
        if wave.flags & 0x10 == 0 {
            self.origin = ship;
        }
        self.tick = 0;
        self.entry = 0;
        self.tick_timer = 0.0;
        self.spawning = true;
        self.threshold =
            (self.threshold + wave_units(wave) - wave.threshold as i32).max(MIN_THRESHOLD);
        if wave.flags & 8 != 0 {
            self.tracked_kind = wave.kind;
        }
        if self.spawn_due(dt, ship, field) {
            self.spawning = false;
        }
    }

    /// WaveManager::updateWave: spawn every formation entry whose tick has
    /// come. Returns true when the formation is exhausted.
    fn spawn_due(&mut self, dt: f32, ship: Vec2, field: &mut Field) -> bool {
        let wave = &self.waves[self.index as usize];
        let spawns = wave.pattern.spawns;
        self.tick_timer += dt;
        while self.tick_timer >= TICK {
            if self.tick == 0 && wave.kind != 0 {
                field.events.push(Event::WaveIncoming);
            }
            while self.entry < spawns.len()
                && spawns[self.entry].tick * wave.ticks_per_step == self.tick
            {
                let e = &spawns[self.entry];
                if self.entry == 0 {
                    if wave.flags & 1 != 0 {
                        self.rotation = 0.0;
                    } else if wave.flags & 0x10 == 0 {
                        self.rotation = unit() * 360.0;
                    }
                }
                if wave.flags & 4 != 0 {
                    self.origin = ship;
                }
                let mut offset = if wave.pattern.mode == 0 {
                    let angle = (e.a + signed() * e.a_jitter + self.rotation).to_radians();
                    let distance = (e.b + signed() * e.b_jitter) * wave.scale;
                    vec2(angle.sin() * distance, -angle.cos() * distance)
                } else {
                    vec2(
                        (e.a + e.a_jitter * signed()) * wave.scale,
                        (e.b + signed() * e.b_jitter) * wave.scale,
                    )
                };
                if wave.offset_angle != 0.0 || wave.offset_distance != 0.0 {
                    let a = (wave.offset_angle + self.rotation).to_radians();
                    offset += vec2(a.sin(), -a.cos()) * wave.offset_distance;
                }
                if wave.kind == KIND_BOSS {
                    let p = self.origin + offset;
                    let p = vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y));
                    field.spawn_boss(p, self.world, self.stage, ship);
                } else if wave.kind == KIND_LOBBER {
                    let p = self.origin + offset;
                    field.spawn_lobber(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if wave.kind == KIND_SEEKER {
                    let p = self.origin + offset;
                    field.spawn_seeker(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if wave.kind == KIND_TOWER {
                    let p = self.origin + offset;
                    field.spawn_tower(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if wave.kind == KIND_ARROW {
                    let p = self.origin + offset;
                    let p = vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y));
                    field.spawn_arrow(p, e.heading + self.rotation);
                } else if wave.kind == KIND_SNAKE {
                    let p = self.origin + offset;
                    let follow = wave.flags & 2 != 0;
                    field.spawn_snake(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)), follow);
                } else if wave.kind == KIND_SHY_GUY {
                    let p = self.origin + offset;
                    field.spawn_shy_guy(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if wave.kind == KIND_MINE {
                    let p = self.origin + offset;
                    field.spawn_mine(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if wave.kind == KIND_DEPOT {
                    let p = self.origin + offset;
                    field.spawn_depot(vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)));
                } else if let Some((element, size)) = rock_kind(wave.kind) {
                    let p = self.origin + offset;
                    field.spawn(
                        element,
                        size,
                        vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)),
                    );
                }
                self.entry += 1;
            }
            self.tick += 1;
            self.tick_timer -= TICK;
            if self.entry >= spawns.len() {
                break;
            }
        }
        self.entry >= spawns.len()
    }
}
