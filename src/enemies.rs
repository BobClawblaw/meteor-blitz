//! Meteors (`Rock`, `NormalRock`, `IceRock`, `FireRock`, `Enemy`) and the
//! stage script runner (`WaveManager`), ported from the iOS binary.

use macroquad::prelude::*;

use crate::waves::{Wave, ARCADE};

/// World::WIDTH / HEIGHT.
const WORLD: f32 = 1024.0;

// Enemy::setState / Enemy::update: enemies drop in from 400 units above the
// surface at 450 units/s, fading in from 25% at 0.84375/s.
const DROP_IN_HEIGHT: f32 = 400.0;
const DROP_IN_SPEED: f32 = 450.0;
const FADE_IN_START: f32 = 0.25;
const FADE_IN_SPEED: f32 = 0.84375;

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

/// What happened during an update, for effects, sounds and scoring.
pub enum Event {
    RockDestroyed {
        pos: Vec2,
        element: Element,
        points: u32,
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
    WaveIncoming,
}

/// All enemies on the field (only rocks so far).
pub struct Field {
    pub rocks: Vec<Rock>,
    /// Animation frame counts by size: small atlas, medium, large, extra large.
    frames: [usize; 4],
    pub events: Vec<Event>,
}

impl Field {
    pub fn new(frames: [usize; 4]) -> Field {
        Field {
            rocks: Vec::new(),
            frames,
            events: Vec::new(),
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
        self.rocks.iter().map(|r| WEIGHT[r.size]).sum()
    }

    /// WaveManager::getRemaining for one wave kind.
    fn remaining_of_kind(&self, kind: i32) -> usize {
        match rock_kind(kind) {
            Some((element, size)) => self.count(element, size),
            None => 0,
        }
    }

    pub fn update(&mut self, dt: f32, centre: Vec2) {
        for rock in &mut self.rocks {
            rock.update(dt, centre);
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
        false
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
        let rock = self.rocks.swap_remove(index);
        self.events.push(Event::RockDestroyed {
            pos: rock.pos,
            element: rock.element,
            points: if score { rock.points } else { 0 },
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
        }
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
                if let Some((element, size)) = rock_kind(wave.kind) {
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
