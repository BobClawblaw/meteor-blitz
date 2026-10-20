//! The game screen (`GameScreen`, `Ship`, `StandardBulletManager`, `Camera`,
//! `Background`), ported from the iOS binary.
//!
//! Coordinates are the original's: the world is a 1024x1024 torus, the
//! ship sits at the world centre and everything is drawn relative to it,
//! offset by the camera's lead. The simulation runs at a fixed 60 Hz.
//!
//! Controls: the original's left wheel (ship velocity) is WASD, the right
//! wheel (fire direction) is the mouse with the left button held, boost
//! (double tap on the left wheel) is Space.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::enemies::{wrap_near, Element, Event, Field, Rock, Script, Weapon};
use crate::gfx::{draw_atlas_sprite, draw_quad, draw_sprite, Canvas};
use crate::bonus::{Power, Rings};
use crate::boss::{self, BossState};
use crate::tower;
use crate::depot::{self, Depot};
use crate::hud::{self, HudAssets, HudValues};
use crate::particles::{Effects, Kind};

const STEP: f32 = 1.0 / 60.0;

/// World::WIDTH / HEIGHT: the playfield wraps every 1024 units.
const WORLD: f32 = 1024.0;

// GameScreen: the backdrop sits BACKGROUND_Z towards the viewer; screens fly
// in from GAME_SCREEN_START_Z, fading in at 0.6/s along a sine curve.
const BACKGROUND_Z: f32 = 100.0;
const START_Z: f32 = 1000.0;
const FADE_IN_SPEED: f32 = 0.6;
const SCREEN_CENTRE: Vec2 = vec2(240.0, 160.0);

// GameScreen::updateLeftWheel: wheel deflection is capped at 64 units and
// scaled by the ship's speed (5 at start), so 320 units/s at full stick.
const WHEEL_MAX: f32 = 64.0;
const SHIP_SPEED: f32 = 5.0;
// Ship: velocity decays by DRAG / (dt + DRAG) per update when released.
const DRAG: f32 = 0.3;

// Global::BOOST_*: 12000 units/s^2 for 0.05 s, 1 s of invincibility, and an
// 8 s recharge before the next boost.
const BOOST_SPEED: f32 = 12000.0;
const BOOST_SECONDS: f32 = 0.05;
const BOOST_INVINCIBLE: f32 = 1.0;
const BOOST_RECHARGE: f32 = 8.0;

// Ship::update: the bank animation steps one frame every 1/15 s toward
// (average turn + 0.4) / 0.8 + 4 over the last ten updates.
const BANK_STEP: f32 = 0.0667;
const TURN_HISTORY: usize = 10;

// Camera: leads the ship by -0.05 x velocity, clamped to +/-18, easing 10%
// of the way per update.
const CAMERA_LEAD: f32 = -0.05;
const CAMERA_LIMIT: f32 = 18.0;
const CAMERA_EASE: f32 = 0.1;

// StandardBulletManager: 900 units/s, 0.4 s life; fire interval by level.
const CANNON_SPEED: f32 = 900.0;
const CANNON_LIFE: f32 = 0.4;
const CANNON_INTERVAL: [f32; 8] = [0.15, 0.1, 0.1333, 0.1, 0.12, 0.1, 0.1143, 0.1];
/// Spread angles (degrees) of each shot by cannon level.
const CANNON_SPREAD: [&[f32]; 8] = [
    &[0.0],
    &[0.0],
    &[-3.0, 3.0],
    &[-3.0, 3.0],
    &[-6.0, 0.0, 6.0],
    &[-6.0, 0.0, 6.0],
    &[-3.0, 3.0, -9.0, 9.0],
    &[-3.0, 3.0, -9.0, 9.0],
];
/// The trail of point sprites drawn per bullet: (distance behind, size).
const CANNON_TRAIL: [(f32, f32); 4] = [(0.0, 18.0), (4.0, 13.0), (7.0, 10.0), (9.0, 8.0)];
/// Ship::drawAim: the aim dot sits 60 units out along the fire direction.
const AIM_DISTANCE: f32 = 60.0;
// StandardBulletManager: segment shapes of radius 2, mass 0.03, damage 1.
const CANNON_RADIUS: f32 = 2.0;
const CANNON_MASS: f32 = 0.03;
const CANNON_DAMAGE: f32 = 1.0;

/// Ship collision circle (Ship::Ship: cpCircleShapeNew radius 10).
const SHIP_RADIUS: f32 = 10.0;
// Ship::setState: a new ship is shielded for 3 s, then the shield fades
// over 1 s; after a death the next ship comes 2 s later.
const SPAWN_SHIELD: f32 = 3.0;
const SHIELD_FADE: f32 = 1.0;
const RESPAWN_DELAY: f32 = 2.0;
/// Ship::update spins the shield at 300 degrees/s.
const SHIELD_SPIN: f32 = 300.0;
// GameScreen::reset: three lives (two in survival) and three bombs.
const LIVES: u32 = 3;
const SURVIVAL_LIVES: u32 = 2;
const BOMBS: u32 = 3;
const SURVIVAL: usize = 6;

// Audio::play ids (sounds.raw).
pub const SOUND_BOOST: usize = 4;
pub const SOUND_BOOST_READY: usize = 5;
pub const SOUND_WAVE: usize = 11;
/// Bonus::setState(0): the ring pickup chime.
pub const SOUND_RING: usize = 9;
/// BigBonus::setState(0): power-up pickup, and the slow-motion one.
pub const SOUND_POWER: usize = 2;
pub const SOUND_SLOW: usize = 16;
/// Mine::setState(10007): Enemy::activateBonuses(50, 1).
const MINE_RING_SPEED: f32 = 50.0;
/// BossInside::setState(10006) and (10007).
pub const SOUND_BOSS_DYING: usize = 6;
pub const SOUND_BOSS_DEAD: usize = 7;
/// BossInside::setState(10007): Enemy::activateBonuses(150, 20).
const BOSS_RINGS: u32 = 20;
const BOSS_RING_SPEED: f32 = 150.0;
/// Depot::handleEvent(0) and Depot::setState(10007).
pub const SOUND_DEPOT_HIT: usize = 8;
pub const SOUND_DEPOT_DESTROYED: usize = 10;
/// BigBonus::setState(0): the ring magnet lasts the stage (25 s in survival).
const MAGNET_SECONDS: f32 = 10000.0;
const SURVIVAL_MAGNET_SECONDS: f32 = 25.0;
/// TimeManager::modify(0.25, 8): slow motion for enemies.
const SLOW_FACTOR: f32 = 0.25;
const SLOW_SECONDS: f32 = 8.0;
/// Ship::deactivateShield: a broken shield gives 0.75 s of grace.
const SHIELD_GRACE: f32 = 0.75;
/// BigBonus::draw: its label sits 10 units below the centre at 75%.
const POWER_LABEL_DROP: f32 = 10.0;
/// Rock::setState(10007): Enemy::activateBonuses spread for a small rock.
const RING_DROP_SPEED: f32 = 5.0;
/// Hud::update: the multiplier steps up when the score since the last
/// death passes each of these.
const MULTIPLIER_STEPS: [u64; 9] = [4999, 19999, 49999, 89999, 139999, 199999, 269999, 349999, 439999];
pub const SOUND_ROCK: [usize; 3] = [12, 13, 14];
pub const SOUND_SHIP_DESTROYED: usize = 15;
/// Ship::update: the boost-ready cue starts 1.3 to 0.65 s before the boost
/// has recharged.
const BOOST_CUE_EARLY: f32 = 1.3;
const BOOST_CUE_LATE: f32 = 0.65;

/// One element's meteor art: the small-rock 4x4 atlas, then the medium,
/// large and extra-large rotation frames.
pub struct RockArt {
    pub small: Texture2D,
    pub medium: Vec<Texture2D>,
    pub large: Vec<Texture2D>,
    pub extra_large: Vec<Texture2D>,
}

impl RockArt {
    /// Frame counts by size, for the rock animations.
    pub fn frame_counts(&self) -> [usize; 4] {
        [
            16,
            self.medium.len(),
            self.large.len(),
            self.extra_large.len(),
        ]
    }
}

pub struct PlayAssets {
    pub hud: HudAssets,
    /// The original's systems.dat (baked particle effects).
    pub systems: Vec<u8>,
    /// Point-sprite texture for sprite effects (fire200.png).
    pub spark: Texture2D,
    /// Normal, ice and fire.
    pub rocks: [RockArt; 3],
    pub ship: Vec<Texture2D>,
    pub ship_ready: Vec<Texture2D>,
    pub shield: Texture2D,
    pub aim: Texture2D,
    pub cannon: Texture2D,
    pub depot: Texture2D,
    pub mine: Texture2D,
    pub arrow: Texture2D,
    pub tower: Texture2D,
    pub seeker: Texture2D,
    /// shyGuyFrames.png: a 4x4 atlas of 32x32 frames.
    pub shy_guy: Texture2D,
    /// pillHead.1.png and pillBody.1.png.
    pub snake_head: Texture2D,
    pub snake_body: Texture2D,
    /// bossInside.1-8 and ball.png (a boss piece).
    pub boss: Vec<Texture2D>,
    pub boss_piece: Texture2D,
    /// depot.png's "BONUS" label.
    pub depot_label: Texture2D,
    /// bigBonus.png: five 16x16 power-up icons.
    pub power: Texture2D,
    /// Power-up labels in `Power::frame` order: life, shield, bomb, slow, rings.
    pub power_labels: [Texture2D; 5],
    /// Base and decal planet textures per world (0-5), then survival.
    pub backdrops: Vec<(Texture2D, Texture2D)>,
}

/// TimeManager: enemies run at `factor`, easing toward the target while
/// a slow-motion power-up lasts and back to 1 after.
struct TimeScale {
    factor: f32,
    target: f32,
    remaining: f32,
}

impl Default for TimeScale {
    fn default() -> TimeScale {
        TimeScale { factor: 1.0, target: 1.0, remaining: 0.0 }
    }
}

impl TimeScale {
    fn modify(&mut self, factor: f32, seconds: f32) {
        self.target = factor;
        self.remaining = seconds;
    }

    /// TimeManager::update.
    fn update(&mut self, dt: f32) {
        self.remaining = (self.remaining - dt).max(0.0);
        let goal = if self.remaining > 0.0 { self.target } else { 1.0 };
        let next = self.factor + 2.0 * dt * (goal - self.factor);
        self.factor = if self.factor < goal { next.min(goal) } else { next.max(goal) };
    }
}

/// Primitives::fillRect: an untextured rectangle at depth `z`.
fn fill_rect(at: Vec2, size: Vec2, z: f32, color: Color) {
    let (x, y, w, h) = (at.x, at.y, size.x, size.y);
    draw_mesh(&Mesh {
        vertices: vec![
            Vertex::new(x, y, z, 0.0, 0.0, color),
            Vertex::new(x + w, y, z, 1.0, 0.0, color),
            Vertex::new(x + w, y + h, z, 1.0, 1.0, color),
            Vertex::new(x, y + h, z, 0.0, 1.0, color),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    });
}

/// Ship::update keeps the ship itself in [-512, 512].
fn wrap_world(v: f32) -> f32 {
    let mut v = v;
    while v < -WORLD * 0.5 {
        v += WORLD;
    }
    while v > WORLD * 0.5 {
        v -= WORLD;
    }
    v
}

/// Degrees for a direction, the way Ship::update and Ship::fire compute it:
/// 90 + atan2 in degrees, wrapped to [0, 360).
fn heading(d: Vec2) -> f32 {
    let a = 90.0 + d.y.atan2(d.x).to_degrees();
    if a < 0.0 {
        a + 360.0
    } else {
        a
    }
}

struct Bullet {
    /// Where the bullet was last update: hits are tested along the segment.
    prev: Vec2,
    pos: Vec2,
    vel: Vec2,
    life: f32,
    /// Unit vector pointing back along the flight path, for the trail.
    back: Vec2,
}

/// Ship::setState: 1 spawning (shielded), 2 shield fading, 3 vulnerable,
/// 4 destroyed.
#[derive(Clone, Copy, PartialEq)]
enum Life {
    Spawning(f32),
    Fading(f32),
    Alive,
    Dead(f32),
}

struct Ship {
    life: Life,
    /// The boost-ready cue has played for this recharge.
    boost_cued: bool,
    pos: Vec2,
    vel: Vec2,
    boost_vel: Vec2,
    boost_dir: Vec2,
    boost_time: f32,
    invincible: f32,
    /// Ship 0x94: the shield power-up absorbs the next hit.
    shield: bool,
    /// Ship 0xb8 and 0xe0: ships spawned and boosts used, so the boss
    /// counts each ram once.
    lives_used: u32,
    boosts: u32,
    /// Ship 0xfc: no death for a moment after the shield breaks.
    grace: f32,
    boost_charge: f32,
    angle: f32,
    frame: usize,
    bank_timer: f32,
    turns: VecDeque<f32>,
    turn_average: f32,
    aiming: bool,
    aim_dir: Vec2,
    alpha: f32,
    cannon_level: usize,
    cannon_timer: f32,
    bullets: Vec<Bullet>,
}

impl Ship {
    fn new() -> Ship {
        Ship {
            life: Life::Spawning(SPAWN_SHIELD),
            boost_cued: true,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            boost_vel: Vec2::ZERO,
            boost_dir: Vec2::ZERO,
            boost_time: 0.0,
            invincible: 0.0,
            shield: false,
            lives_used: 0,
            boosts: 0,
            grace: 0.0,
            boost_charge: BOOST_RECHARGE,
            angle: 0.0,
            frame: 4,
            bank_timer: 0.0,
            turns: VecDeque::new(),
            turn_average: 0.0,
            aiming: false,
            aim_dir: vec2(0.0, -1.0),
            alpha: 0.0,
            cannon_level: 0,
            cannon_timer: CANNON_INTERVAL[0],
            bullets: Vec::new(),
        }
    }

    /// Ship::reset: a fresh ship where the last one died.
    fn respawn(&mut self) {
        self.life = Life::Spawning(SPAWN_SHIELD);
        self.lives_used += 1;
        self.vel = Vec2::ZERO;
        self.boost_vel = Vec2::ZERO;
        self.boost_time = 0.0;
        self.invincible = 0.0;
        self.boost_charge = BOOST_RECHARGE;
        self.bullets.clear();
    }

    fn can_boost(&self) -> bool {
        self.boost_charge >= BOOST_RECHARGE
    }

    /// Ship::boost: kick along the current velocity.
    fn boost(&mut self, dt: f32) {
        self.boost_charge = 0.0;
        self.boost_cued = false;
        self.boosts += 1;
        self.boost_time = BOOST_SECONDS;
        self.invincible = BOOST_INVINCIBLE;
        self.boost_dir = self.vel.normalize_or_zero();
        self.boost_vel += self.boost_dir * BOOST_SPEED * dt;
    }

    /// StandardBulletManager::fire: one volley per interval, spread by level,
    /// launched from where the ship will be after this update.
    fn fire(&mut self, dt: f32, dir: Vec2) {
        self.aiming = true;
        self.aim_dir = dir;
        self.cannon_timer += dt;
        let interval = CANNON_INTERVAL[self.cannon_level];
        if self.cannon_timer < interval {
            return;
        }
        self.cannon_timer %= interval;
        let origin = self.pos + self.vel * dt;
        let base = heading(dir);
        for offset in CANNON_SPREAD[self.cannon_level] {
            let rad = (base + offset).to_radians();
            let vel = vec2(rad.sin(), -rad.cos()) * CANNON_SPEED;
            self.bullets.push(Bullet {
                prev: origin,
                pos: origin,
                vel,
                life: CANNON_LIFE,
                back: -vel.normalize(),
            });
        }
    }

    /// Ship::resetBullets: the next press fires at once.
    fn hold_fire(&mut self) {
        self.cannon_timer = CANNON_INTERVAL[self.cannon_level];
        self.aiming = false;
    }

    fn update(&mut self, dt: f32) {
        self.life = match self.life {
            Life::Spawning(t) if t - dt <= 0.0 => Life::Fading(SHIELD_FADE),
            Life::Spawning(t) => Life::Spawning(t - dt),
            Life::Fading(t) if t - dt <= 0.0 => Life::Alive,
            Life::Fading(t) => Life::Fading(t - dt),
            other => other,
        };
        let before = self.angle;
        if self.boost_time > 0.0 {
            self.boost_time -= dt;
            self.boost_vel += self.boost_dir * BOOST_SPEED * dt;
        }
        self.invincible = (self.invincible - dt).max(0.0);
        self.grace = (self.grace - dt).max(0.0);
        self.boost_charge = (self.boost_charge + dt).min(BOOST_RECHARGE);

        let total = self.vel + self.boost_vel;
        if total != Vec2::ZERO {
            self.angle = heading(total);
        }
        self.pos = vec2(
            wrap_world(self.pos.x + total.x * dt),
            wrap_world(self.pos.y + total.y * dt),
        );
        self.boost_vel *= DRAG / (dt + DRAG);

        for b in &mut self.bullets {
            b.prev = b.pos;
            b.life -= dt;
            b.pos = vec2(
                wrap_near(b.pos.x + b.vel.x * dt, self.pos.x),
                wrap_near(b.pos.y + b.vel.y * dt, self.pos.y),
            );
        }
        self.bullets.retain(|b| b.life > 0.0);

        // Bank toward the average turn rate.
        let mut turn = self.angle - before;
        while turn >= 180.0 {
            turn -= 360.0;
        }
        while turn < -180.0 {
            turn += 360.0;
        }
        if self.turns.len() == TURN_HISTORY {
            let oldest = self.turns.pop_front().unwrap_or(0.0);
            self.turn_average =
                (self.turn_average * TURN_HISTORY as f32 - oldest) / (TURN_HISTORY - 1) as f32;
        }
        self.turns.push_back(turn);
        let n = self.turns.len() as f32;
        self.turn_average = (turn + self.turn_average * (n - 1.0)) / n;
        self.bank_timer += dt;
        if self.bank_timer >= BANK_STEP {
            let target = (((self.turn_average + 0.4) / 0.8) as i32 + 4).clamp(0, 8) as usize;
            if target < self.frame {
                self.frame -= 1;
                self.bank_timer = 0.0;
            } else if target > self.frame {
                self.frame += 1;
                self.bank_timer = 0.0;
            }
        }
    }
}

/// `Camera`: lead toward the ship's motion, plus shake.
struct Lead {
    centre: Vec2,
    shake: Vec2,
    shake_power: f32,
    shake_time: f32,
}

impl Lead {
    fn update(&mut self, dt: f32, target: Vec2) {
        self.centre -= (self.centre - target) * CAMERA_EASE;
        self.centre = self
            .centre
            .clamp(Vec2::splat(-CAMERA_LIMIT), Vec2::splat(CAMERA_LIMIT));
        if self.shake_time != 0.0 && self.shake_power != 0.0 {
            self.shake_time -= dt;
            if self.shake_time < 0.0 {
                self.shake_time = 0.0;
                self.shake = Vec2::ZERO;
            } else {
                self.shake =
                    vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * self.shake_power;
                self.shake_power *= 0.5 / (dt + 0.5);
            }
        }
    }

    fn shake(&mut self, seconds: f32, power: f32) {
        self.shake_time = self.shake_time.max(seconds);
        self.shake_power = self.shake_power.max(power);
    }

    fn offset(&self) -> Vec2 {
        self.centre + self.shake
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Flying in from GAME_SCREEN_START_Z while fading in.
    Entering,
    Ready,
    /// Stage cleared or out of lives; the screen waits for the player.
    Over {
        cleared: bool,
    },
}

pub struct Play {
    pub world: usize,
    pub stage: usize,
    state: State,
    alpha: f32,
    z: f32,
    elapsed: f32,
    shaken: bool,
    ship: Ship,
    camera: Lead,
    /// Background offset: the planet scrolls with the ship's motion.
    scroll: Vec2,
    accumulator: f32,
    field: Field,
    script: Script,
    pub score: u64,
    pub multiplier: u32,
    pub lives: u32,
    pub bombs: u32,
    pub rings: u32,
    /// Global::multiplierBaseScore: the score at the last death.
    multiplier_base: u64,
    pickups: Rings,
    /// Global::bonusAlwaysGoToShipSeconds: new rings home in at once.
    magnet: f32,
    time: TimeScale,
    high_score: u64,
    /// `Audio::play` ids raised this update, for the caller to play.
    pub sounds: Vec<usize>,
    effects: Effects,
}

impl Play {
    /// `world` 0-5 for arcade worlds, 6 for survival.
    pub fn new(world: usize, stage: usize, high_score: u64, assets: &PlayAssets) -> Play {
        Play {
            world,
            stage,
            state: State::Entering,
            alpha: 0.0,
            z: START_Z,
            elapsed: 0.0,
            shaken: false,
            ship: Ship::new(),
            camera: Lead {
                centre: Vec2::ZERO,
                shake: Vec2::ZERO,
                shake_power: 0.0,
                shake_time: 0.0,
            },
            scroll: Vec2::ZERO,
            accumulator: 0.0,
            field: Field::new(assets.rocks[0].frame_counts()),
            script: Script::arcade(world, stage),
            score: 0,
            multiplier: 1,
            lives: if world == SURVIVAL {
                SURVIVAL_LIVES
            } else {
                LIVES
            },
            bombs: BOMBS,
            rings: 0,
            multiplier_base: 0,
            pickups: Rings::default(),
            magnet: 0.0,
            time: TimeScale::default(),
            high_score,
            sounds: Vec::new(),
            effects: Effects::load(&assets.systems),
        }
    }

    /// The stage is over: `Some(true)` cleared, `Some(false)` out of lives.
    pub fn result(&self) -> Option<bool> {
        match self.state {
            State::Over { cleared } => Some(cleared),
            _ => None,
        }
    }

    /// Where a world position appears on the canvas.
    fn to_screen(&self, p: Vec2) -> Vec2 {
        let d = vec2(
            wrap_near(p.x, self.ship.pos.x) - self.ship.pos.x,
            wrap_near(p.y, self.ship.pos.y) - self.ship.pos.y,
        );
        SCREEN_CENTRE + self.camera.offset() + d
    }

    pub fn update(&mut self, dt: f32) {
        self.accumulator += dt;
        while self.accumulator >= STEP {
            self.accumulator -= STEP;
            self.step(STEP);
        }
    }

    fn step(&mut self, dt: f32) {
        match self.state {
            State::Entering => {
                self.alpha += dt * FADE_IN_SPEED;
                self.ship.alpha = self.alpha;
                if self.alpha >= 1.0 {
                    self.alpha = 1.0;
                    self.ship.alpha = 1.0;
                    self.state = State::Ready;
                }
                self.z =
                    (START_Z - START_Z * (self.alpha * std::f32::consts::FRAC_PI_2).sin()).max(0.0);
                self.ship.update(dt);
                if !self.shaken && self.elapsed > 0.75 {
                    self.shaken = true;
                    self.camera.shake(0.75, 1.0);
                }
                self.camera.update(dt, Vec2::ZERO);
            }
            State::Ready => self.step_ready(dt),
            State::Over { .. } => {
                self.time.update(dt);
                self.field.update(dt * self.time.factor, self.ship.pos);
                self.update_rings(dt);
                self.camera.update(dt, Vec2::ZERO);
            }
        }
        self.effects.update(dt);
        self.elapsed += dt;
    }

    /// GameScreen::updateStateReady with keyboard and mouse in place of the
    /// two touch wheels.
    fn step_ready(&mut self, dt: f32) {
        if let Life::Dead(t) = self.ship.life {
            // GameScreen::updateStateReady: two seconds after a death, the
            // next ship, or the end of the game.
            self.ship.life = Life::Dead(t + dt);
            if t + dt >= RESPAWN_DELAY {
                if self.lives == 0 {
                    self.state = State::Over { cleared: false };
                } else {
                    self.lives -= 1;
                    self.ship.respawn();
                }
            }
            self.ship.vel = Vec2::ZERO;
        } else {
            self.control(dt);
        }
        self.ship.update(dt);
        let total = self.ship.vel + self.ship.boost_vel;
        self.scroll += total * dt;
        self.camera.update(dt, total * CAMERA_LEAD);

        self.script.update(dt, self.ship.pos, &mut self.field);
        self.time.update(dt);
        self.field.update(dt * self.time.factor, self.ship.pos);
        self.follow_boss_death();
        self.update_rings(dt);
        self.magnet = (self.magnet - dt).max(0.0);
        self.collide();
        self.handle_events();
        self.update_multiplier();
        if self.script.complete(&self.field) {
            self.state = State::Over { cleared: true };
        }
    }

    /// The cannon against rocks, then rocks against the ship (shipCollision).
    fn collide(&mut self) {
        let field = &mut self.field;
        let effects = &mut self.effects;
        self.ship.bullets.retain(|b| {
            let hit = field.bullet(
                b.prev,
                b.pos,
                CANNON_RADIUS,
                Weapon::Cannon,
                CANNON_DAMAGE,
                CANNON_MASS,
                b.vel,
            );
            if hit {
                // standardBulletCollision: a spark and a streak back along the shot.
                effects.start(Kind::Sprite, 0, b.pos, 0.0, false);
                effects.start(Kind::Line, 0, b.pos, heading(-b.vel), false);
            }
            !hit
        });
        if matches!(self.ship.life, Life::Dead(_)) {
            return;
        }
        // shipCollision with a depot: boosting smashes it, else it is pushed.
        let mut depots = self.field.touching_depots(self.ship.pos, SHIP_RADIUS);
        depots.sort_unstable_by(|a, b| b.cmp(a));
        for i in depots {
            if self.ship.invincible > 0.0 {
                self.field.destroy_depot(i, true);
            } else {
                let ship = self.ship.pos;
                let d = &mut self.field.depots[i];
                let other = vec2(wrap_near(ship.x, d.pos.x), wrap_near(ship.y, d.pos.y));
                d.repel_from(other, SHIP_RADIUS, depot::REPEL);
            }
        }
        let boosting = self.ship.invincible > 0.0;
        let mut hit = false;
        // Boss pieces: smashed by a boost, deadly otherwise.
        let mut pieces = self.field.touching_pieces(self.ship.pos, SHIP_RADIUS);
        pieces.sort_unstable_by(|a, b| b.cmp(a));
        for i in pieces {
            if boosting {
                self.field.destroy_piece(i, true, true);
            } else {
                hit = true;
            }
        }
        // Snakes: a head loses its tail (or dies) when rammed; touching the
        // head or body kills the ship unless it is boosting.
        let (heads, body) = self.field.touching_snakes(self.ship.pos, SHIP_RADIUS);
        let mut heads = heads;
        heads.sort_unstable_by(|a, b| b.cmp(a));
        hit |= (body || !heads.is_empty()) && !boosting;
        for i in heads {
            self.field.ram_snake(i);
        }
        // Tower::handleEvent(3/4): a tower the ship touches is destroyed,
        // and the ship with it unless boosting. A live beam
        // (Ship::handleEvent(12)) kills a ship that is not boosting.
        let p = self.ship.pos;
        let mut towers: Vec<usize> = (0..self.field.towers.len())
            .filter(|&i| {
                let t = &self.field.towers[i];
                let c = vec2(wrap_near(t.pos.x, p.x), wrap_near(t.pos.y, p.y));
                t.active() && c.distance(p) < tower::RADIUS + SHIP_RADIUS
            })
            .collect();
        towers.sort_unstable_by(|a, b| b.cmp(a));
        for i in towers {
            self.field.destroy_tower(i, true, true, true);
            hit |= !boosting;
        }
        if !boosting && self.field.beam_hits(self.ship.pos, SHIP_RADIUS) {
            hit = true;
        }
        // Seekers die on contact and take the ship with them unless boosting.
        let mut seekers = self.field.touching_seekers(self.ship.pos, SHIP_RADIUS);
        seekers.sort_unstable_by(|a, b| b.cmp(a));
        for i in seekers {
            self.field.destroy_seeker(i, true, true, true);
            hit |= !boosting;
        }
        // Arrows die on contact and take the ship with them unless boosting.
        let mut arrows = self.field.touching_arrows(self.ship.pos, SHIP_RADIUS);
        arrows.sort_unstable_by(|a, b| b.cmp(a));
        for i in arrows {
            self.field.destroy_arrow(i, true, true, true);
            hit |= !boosting;
        }
        // Shy guys die on contact and take the ship with them unless boosting.
        let mut shy = self.field.touching_shy_guys(self.ship.pos, SHIP_RADIUS);
        shy.sort_unstable_by(|a, b| b.cmp(a));
        for i in shy {
            self.field.destroy_shy_guy(i, true, true, true);
            hit |= !boosting;
        }
        // Mines blow up on contact and take the ship with them unless boosting.
        let mut mines = self.field.touching_mines(self.ship.pos, SHIP_RADIUS);
        mines.sort_unstable_by(|a, b| b.cmp(a));
        for i in mines {
            self.field.destroy_mine(i, true, true, true);
            hit |= !boosting;
        }
        // The core: ramming it costs it 7.5 once per life or per boost.
        if self.field.touching_boss(self.ship.pos, SHIP_RADIUS) {
            let id = if boosting { self.ship.boosts } else { self.ship.lives_used };
            let dead = self.field.boss.as_mut().is_some_and(|b| b.ram(boosting, id));
            if dead {
                self.field.boss_dying();
            }
            hit |= !boosting;
        }
        self.handle_events();
        let mut touching = self.field.touching(self.ship.pos, SHIP_RADIUS);
        touching.sort_unstable_by(|a, b| b.cmp(a));
        hit |= !touching.is_empty();
        for i in touching {
            self.field.destroy(i, true);
        }
        if !hit {
            return;
        }
        if self.ship.life == Life::Alive && self.ship.invincible <= 0.0 {
            if self.ship.shield {
                self.break_shield();
            } else if self.ship.grace == 0.0 {
                self.kill_ship();
            }
        }
    }

    /// Ship::setState(4).
    fn kill_ship(&mut self) {
        self.ship.life = Life::Dead(0.0);
        self.ship.bullets.clear();
        self.ship.aiming = false;
        self.camera.shake(0.75, 7.5);
        self.script.delay();
        self.sounds.push(SOUND_SHIP_DESTROYED);
        // Ship::setState(4): the multiplier starts over from this score.
        self.multiplier = 1;
        self.multiplier_base = self.score;
        let p = self.ship.pos;
        self.effects.flash(Color::new(0.8, 0.8, 0.8, 1.0));
        self.effects.start(Kind::Ring, 1, p, 0.0, false);
        self.effects.start(Kind::Line, 3, p, 0.0, false);
        for _ in 0..3 {
            self.effects.start(Kind::Point, 2, p, 0.0, false);
        }
    }

    /// BossInside::update in 10006: the charge-up follows the core, and the
    /// core explodes when it ends.
    fn follow_boss_death(&mut self) {
        let Some(b) = &self.field.boss else { return };
        if b.state != BossState::Dying || b.death_effect == 0 {
            return;
        }
        let (id, pos) = (b.death_effect, b.pos);
        if self.effects.is_running(id) {
            self.effects.follow(id, pos);
        } else {
            self.field.boss_dead();
        }
    }

    /// Bonus::update for every ring; Bonus::setState(0) for each collected.
    fn update_rings(&mut self, dt: f32) {
        let ship = (!matches!(self.ship.life, Life::Dead(_))).then_some(self.ship.pos);
        for (pos, power) in self.pickups.update(dt, ship, self.ship.pos) {
            match power {
                None => {
                    self.sounds.push(SOUND_RING);
                    self.effects
                        .start(Kind::Point, 15, pos, rand::gen_range(0.0, 360.0), false);
                    self.rings += 1;
                    self.score += self.multiplier as u64;
                }
                Some(power) => self.collect_power(pos, power),
            }
        }
    }

    /// BigBonus::setState(0).
    fn collect_power(&mut self, pos: Vec2, power: Power) {
        self.effects.start(Kind::Point, 16, pos, 0.0, false);
        self.sounds.push(if power == Power::Slow { SOUND_SLOW } else { SOUND_POWER });
        match power {
            Power::Life => self.lives += 1,
            Power::Bomb => self.bombs += 1,
            Power::Shield => self.ship.shield = true,
            Power::Slow => {
                self.time.modify(SLOW_FACTOR, SLOW_SECONDS);
                self.effects.flash(Color::new(0.8, 0.8, 0.8, 1.0));
            }
            Power::Magnet => {
                self.pickups.magnetize();
                self.magnet = if self.world == SURVIVAL { SURVIVAL_MAGNET_SECONDS } else { MAGNET_SECONDS };
            }
        }
    }

    /// Enemy::activateBigBonus: which power-up a depot drops.
    fn choose_power(&self) -> Power {
        let roll = |n: f32| (rand::gen_range(0.0f32, 1.0) * n) as i32;
        if self.world == SURVIVAL {
            return if !self.ship.shield {
                Power::Shield
            } else if roll(3.0) != 0 {
                Power::Magnet
            } else {
                Power::Slow
            };
        }
        let magnet_on = self.magnet > 0.0;
        match (self.ship.shield, magnet_on) {
            (true, false) => match roll(9.0) {
                0 => Power::Life,
                1 | 2 => Power::Bomb,
                3 | 4 => Power::Slow,
                _ => Power::Magnet,
            },
            (true, true) => match roll(7.0) {
                0 => Power::Life,
                1 | 2 => Power::Bomb,
                _ => Power::Slow,
            },
            (false, false) => match roll(13.0) {
                0 => Power::Life,
                1 | 2 => Power::Bomb,
                3 | 4 => Power::Slow,
                5..=8 => Power::Magnet,
                _ => Power::Shield,
            },
            (false, true) => match roll(9.0) {
                0 => Power::Life,
                1 | 2 => Power::Bomb,
                3..=5 => Power::Slow,
                _ => Power::Shield,
            },
        }
    }

    /// Ship::deactivateShield.
    fn break_shield(&mut self) {
        self.ship.shield = false;
        self.ship.grace = SHIELD_GRACE;
        self.camera.shake(0.75, 7.5);
        let p = self.ship.pos;
        self.effects.start(Kind::Ring, 8, p, 0.0, false);
        for _ in 0..3 {
            self.effects.start(Kind::Point, 17, p, 0.0, false);
        }
        self.sounds.push(SOUND_SHIP_DESTROYED);
    }

    /// Hud::update: the multiplier from the score since the last death.
    fn update_multiplier(&mut self) {
        let since = self.score - self.multiplier_base;
        self.multiplier = 1 + MULTIPLIER_STEPS.iter().filter(|&&t| since > t).count() as u32;
    }

    /// Enemy::rotationTowards the ship, in the original's degrees.
    fn towards_ship(&self, from: Vec2) -> f32 {
        let ship = vec2(
            wrap_near(self.ship.pos.x, from.x),
            wrap_near(self.ship.pos.y, from.y),
        );
        heading(ship - from)
    }

    fn handle_events(&mut self) {
        let events: Vec<Event> = self.field.events.drain(..).collect();
        for event in events {
            match event {
                Event::RockDestroyed {
                    pos,
                    element,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, RING_DROP_SPEED, rings, self.magnet > 0.0);
                    if sound {
                        self.sounds.push(SOUND_ROCK[rand::gen_range(0, 3)]);
                    }
                    if !effects {
                        continue;
                    }
                    // Rock::setState(10007): debris aimed away from the ship.
                    let away = self.towards_ship(pos) + 180.0;
                    let (point, diamond) = match element {
                        Element::Normal => (0, 0),
                        Element::Ice => (11, 1),
                        Element::Fire => (12, 2),
                    };
                    self.effects.start(Kind::Ring, 0, pos, 0.0, false);
                    self.effects.start(Kind::Sprite, 1, pos, 0.0, false);
                    self.effects.start(Kind::Point, point, pos, away, false);
                    self.effects.start(Kind::Diamond, diamond, pos, away, false);
                }
                Event::RockSplit { pos, angle } => {
                    for k in 0..3 {
                        self.effects
                            .start(Kind::Line, 2, pos, angle + 120.0 * k as f32, false);
                    }
                }
                Event::RockHit { pos, element } => {
                    // Rock::handleEvent(0): sparks toward and away from the ship.
                    let towards = self.towards_ship(pos);
                    let (back, front) = match element {
                        Element::Normal => (0, 1),
                        Element::Ice => (11, 13),
                        Element::Fire => (12, 14),
                    };
                    self.effects
                        .start(Kind::Point, back, pos, towards + 180.0, false);
                    self.effects.start(Kind::Point, front, pos, towards, false);
                }
                Event::WaveIncoming => self.sounds.push(SOUND_WAVE),
                Event::DepotHit | Event::BossHit => self.sounds.push(SOUND_DEPOT_HIT),
                Event::MineHit { pos, sound, sparks } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_HIT);
                    }
                    if sparks {
                        let away = self.towards_ship(pos) + 180.0;
                        self.effects.start(Kind::Point, 3, pos, away, false);
                    }
                }
                Event::SeekerDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        let a = rand::gen_range(0.0, 360.0);
                        for k in 0..4 {
                            self.effects.start(Kind::Line, 6, pos, a + 90.0 * k as f32, false);
                        }
                        self.effects.start(Kind::Point, 6, pos, 0.0, false);
                        self.effects.start(Kind::Sprite, 1, pos, 0.0, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::TowerDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        self.effects.start(Kind::Sprite, 3, pos, 0.0, false);
                        self.effects.start(Kind::Line, 4, pos, 0.0, false);
                        self.effects.start(Kind::Point, 4, pos, 0.0, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::ArrowDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        let away = self.towards_ship(pos) + 180.0;
                        self.effects.start(Kind::Diamond, 4, pos, away, false);
                        self.effects.start(Kind::Sprite, 4, pos, 0.0, false);
                        self.effects.start(Kind::Point, 18, pos, 0.0, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::SegmentDestroyed { pos, points, sound } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    self.effects
                        .start(Kind::Point, 7, pos, rand::gen_range(0.0, 360.0), false);
                    self.score += (points * self.multiplier) as u64;
                }
                Event::SnakeDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        let a = rand::gen_range(0.0, 360.0);
                        self.effects.start(Kind::Sprite, 7, pos, 0.0, false);
                        for k in 0..4 {
                            self.effects.start(Kind::Line, 7, pos, a + 90.0 * k as f32, false);
                        }
                        self.effects.start(Kind::Point, 7, pos, a, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::ShyGuyDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        let a = rand::gen_range(0.0, 360.0);
                        self.effects.start(Kind::Line, 5, pos, a, false);
                        self.effects.start(Kind::Line, 5, pos, a + 180.0, false);
                        self.effects.start(Kind::Ring, 0, pos, 0.0, false);
                        self.effects.start(Kind::Sprite, 1, pos, 0.0, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::MineDestroyed {
                    pos,
                    points,
                    rings,
                    sound,
                    effects,
                } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_DESTROYED);
                    }
                    if effects {
                        let away = self.towards_ship(pos) + 180.0;
                        self.effects.start(Kind::Ring, 2, pos, 0.0, false);
                        self.effects.start(Kind::Sprite, 1, pos, 0.0, false);
                        self.effects.start(Kind::Point, 3, pos, away, false);
                        self.effects.start(Kind::Diamond, 3, pos, away, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
                }
                Event::BossDying { pos } => {
                    // BossInside::setState(10006): a charge-up that follows the core.
                    self.camera.shake(0.75, 7.5);
                    self.sounds.push(SOUND_BOSS_DYING);
                    let id = self.effects.start(Kind::Line, 10, pos, 0.0, true);
                    if let Some(b) = &mut self.field.boss {
                        b.death_effect = id;
                    }
                    self.effects.start(Kind::Point, 17, pos, 0.0, false);
                    self.effects.flash(Color::new(0.8, 0.8, 0.8, 1.0));
                }
                Event::BossDead { pos, points } => {
                    self.sounds.push(SOUND_BOSS_DEAD);
                    self.camera.shake(0.75, 7.5);
                    self.effects.flash(Color::new(0.8, 0.8, 0.8, 1.0));
                    self.effects.start(Kind::Ring, 6, pos, 0.0, false);
                    self.effects.start(Kind::Line, 8, pos, 0.0, false);
                    self.effects.start(Kind::Point, 9, pos, 0.0, false);
                    self.effects.start(Kind::Point, 9, pos, 180.0, false);
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, BOSS_RING_SPEED, BOSS_RINGS, self.magnet > 0.0);
                }
                Event::PieceDestroyed {
                    pos,
                    sound,
                    effects,
                    attached,
                } => {
                    if attached {
                        self.camera.shake(0.75, 7.5);
                    }
                    if sound {
                        self.sounds.push(SOUND_ROCK[0]);
                    }
                    if effects {
                        let a = rand::gen_range(0.0, 360.0);
                        for k in 0..4 {
                            self.effects.start(Kind::Line, 9, pos, a + 90.0 * k as f32, false);
                        }
                        self.effects.start(Kind::Sprite, 9, pos, 0.0, false);
                    }
                }
                Event::DepotDestroyed { pos, points } => {
                    self.sounds.push(SOUND_DEPOT_DESTROYED);
                    self.effects.start(Kind::Point, 17, pos, 0.0, false);
                    self.score += (points * self.multiplier) as u64;
                    let power = self.choose_power();
                    self.pickups.drop_power(pos, power);
                }
            }
        }
    }

    fn control(&mut self, dt: f32) {
        let stick = vec2(
            (is_key_down(KeyCode::D) as i32 - is_key_down(KeyCode::A) as i32) as f32,
            (is_key_down(KeyCode::S) as i32 - is_key_down(KeyCode::W) as i32) as f32,
        );
        if stick != Vec2::ZERO {
            self.ship.vel = stick.normalize() * WHEEL_MAX * SHIP_SPEED;
        } else {
            self.ship.vel *= DRAG / (dt + DRAG);
        }
        if is_key_pressed(KeyCode::Space) && self.ship.can_boost() {
            self.ship.boost(dt);
            self.camera.shake(0.75, 7.5);
            self.sounds.push(SOUND_BOOST);
        }
        let charge = self.ship.boost_charge;
        if !self.ship.boost_cued
            && charge >= BOOST_RECHARGE - BOOST_CUE_EARLY
            && charge < BOOST_RECHARGE - BOOST_CUE_LATE
        {
            self.ship.boost_cued = true;
            self.sounds.push(SOUND_BOOST_READY);
        }
        // MB_AUTOFIRE aims at the nearest rock, for unattended screenshot runs.
        let auto = std::env::var_os("MB_AUTOFIRE")
            .is_some()
            .then(|| self.nearest_rock())
            .flatten();
        if let Some(target) = auto {
            let dir = (target - self.ship.pos).normalize_or_zero();
            if dir != Vec2::ZERO {
                self.ship.fire(dt, dir);
            }
        } else if is_mouse_button_down(MouseButton::Left) {
            let dir = (Canvas::mouse() - self.to_screen(self.ship.pos)).normalize_or_zero();
            if dir != Vec2::ZERO {
                self.ship.fire(dt, dir);
            }
        } else {
            self.ship.hold_fire();
        }
    }

    fn nearest_rock(&self) -> Option<Vec2> {
        let ship = self.ship.pos;
        self.field
            .rocks
            .iter()
            .filter(|r| r.active())
            .map(|r| r.pos)
            .chain(self.field.depots.iter().filter(|d| d.active()).map(|d| d.pos))
            .chain(self.field.mines.iter().filter(|m| m.active()).map(|m| m.pos))
            .chain(self.field.shy_guys.iter().filter(|g| g.active()).map(|g| g.pos))
            .chain(self.field.snakes.iter().filter(|s| s.active()).map(|s| s.pos))
            .chain(self.field.arrows.iter().filter(|a| a.active()).map(|a| a.pos))
            .chain(self.field.towers.iter().filter(|t| t.active()).map(|t| t.pos))
            .chain(self.field.seekers.iter().filter(|k| k.active()).map(|k| k.pos))
            .chain(self.field.pieces.iter().filter(|p| p.active()).map(|p| p.pos))
            .chain(self.field.boss.iter().filter(|b| b.active()).map(|b| b.pos))
            .map(|p| vec2(wrap_near(p.x, ship.x), wrap_near(p.y, ship.y)))
            .min_by(|a, b| {
                a.distance_squared(ship)
                    .total_cmp(&b.distance_squared(ship))
            })
    }

    fn draw_rocks(&self, assets: &PlayAssets) {
        // EnemyManager::sharedDraw: per element, small to extra large.
        for (e, element) in [Element::Normal, Element::Ice, Element::Fire]
            .into_iter()
            .enumerate()
        {
            let art = &assets.rocks[e];
            for size in 0..4 {
                for rock in self
                    .field
                    .rocks
                    .iter()
                    .filter(|r| r.element == element && r.size == size)
                {
                    self.draw_rock(art, rock);
                }
            }
        }
    }

    fn draw_depots(&self, canvas: &Canvas, assets: &PlayAssets) {
        for d in &self.field.depots {
            self.draw_depot(canvas, assets, d);
        }
        // Depot::draw: the label, only once the depot has landed.
        let color = Color::new(1.0, 1.0, 1.0, depot::LABEL_ALPHA);
        let label = &assets.depot_label;
        for d in self.field.depots.iter().filter(|d| d.active()) {
            let p = self.to_screen(d.pos);
            draw_quad(label, p.x - label.width() * 0.5, p.y + depot::LABEL_DROP, 1.0, self.z, color);
        }
    }

    fn draw_boss(&self, canvas: &Canvas, assets: &PlayAssets) {
        for p in &self.field.pieces {
            let s = self.to_screen(p.pos);
            let color = Color::new(1.0, 1.0, 1.0, p.alpha);
            let draw = || draw_sprite(&assets.boss_piece, s.x, s.y, p.angle, self.z + p.z, color);
            if p.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let Some(b) = &self.field.boss else { return };
        let s = self.to_screen(b.pos);
        let color = Color::new(1.0, 1.0, 1.0, b.alpha);
        let tex = &assets.boss[b.frame % assets.boss.len()];
        let draw = || draw_sprite(tex, s.x, s.y, b.angle, self.z + b.z, color);
        if b.flash {
            canvas.flashing(draw);
        } else {
            draw();
        }
        if b.active() {
            // BossInside::draw: the health bar.
            let at = s + boss::BAR_OFFSET;
            fill_rect(at, boss::BAR_SIZE, self.z, Color::new(1.0, 1.0, 1.0, 0.25));
            let w = b.health / b.max_health * boss::BAR_SIZE.x;
            fill_rect(at, vec2(w, boss::BAR_SIZE.y), self.z, Color::new(1.0, 0.0, 0.0, 0.5));
        }
    }

    fn draw_depot(&self, canvas: &Canvas, assets: &PlayAssets, d: &Depot) {
        let p = self.to_screen(d.pos);
        let color = Color::new(1.0, 1.0, 1.0, d.alpha);
        let draw = || draw_sprite(&assets.depot, p.x, p.y, d.angle, self.z + d.z, color);
        if d.flash {
            canvas.flashing(draw);
        } else {
            draw();
        }
    }

    fn draw_rock(&self, art: &RockArt, rock: &Rock) {
        let p = self.to_screen(rock.pos);
        let z = self.z + rock.z;
        let color = Color::new(1.0, 1.0, 1.0, rock.alpha);
        match rock.size {
            0 => draw_atlas_sprite(&art.small, 4, 4, rock.frame, p.x, p.y, rock.angle, z, color),
            1 => draw_sprite(
                &art.medium[rock.frame % art.medium.len()],
                p.x,
                p.y,
                rock.angle,
                z,
                color,
            ),
            2 => draw_sprite(
                &art.large[rock.frame % art.large.len()],
                p.x,
                p.y,
                rock.angle,
                z,
                color,
            ),
            _ => draw_sprite(
                &art.extra_large[rock.frame % art.extra_large.len()],
                p.x,
                p.y,
                rock.angle,
                z,
                color,
            ),
        }
    }

    pub fn draw(&self, canvas: &Canvas, assets: &PlayAssets) {
        let cam = self.camera.offset();
        let (base, decal) = &assets.backdrops[self.world.min(assets.backdrops.len() - 1)];
        // MB_BG_Z overrides the backdrop depth while it is being verified.
        let bg_z = std::env::var("MB_BG_Z")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(BACKGROUND_Z);
        let origin = vec3(
            SCREEN_CENTRE.x + cam.x,
            SCREEN_CENTRE.y + cam.y,
            self.z + bg_z,
        );
        canvas.draw_backdrop(false, base, decal, origin, self.scroll, 1.0);
        // EnemyManager::sharedDraw: rings first, under everything else.
        for r in self.pickups.rings.iter().filter(|r| r.power.is_none()) {
            let p = self.to_screen(r.pos);
            // AnimationData(bonus, 4, 4): the 7x7 sprite anchored 4 units in.
            let c = p + vec2(-0.5, -0.5);
            draw_sprite(&assets.hud.ring, c.x, c.y, r.angle, self.z, Color::new(1.0, 1.0, 1.0, r.alpha));
        }
        // Power-ups, then their labels (BigBonus::draw).
        for r in &self.pickups.rings {
            let Some(power) = r.power else { continue };
            let p = self.to_screen(r.pos);
            let color = Color::new(1.0, 1.0, 1.0, r.alpha);
            draw_atlas_sprite(&assets.power, 5, 1, power.frame(), p.x, p.y, r.angle, self.z, color);
        }
        for r in &self.pickups.rings {
            let Some(power) = r.power else { continue };
            let p = self.to_screen(r.pos);
            let label = &assets.power_labels[power.frame()];
            let color = Color::new(1.0, 1.0, 1.0, r.alpha * depot::LABEL_ALPHA);
            draw_quad(label, p.x - label.width() * 0.5, p.y + POWER_LABEL_DROP, 1.0, self.z, color);
        }
        self.draw_rocks(assets);
        self.draw_depots(canvas, assets);
        for m in &self.field.mines {
            let s = self.to_screen(m.pos);
            let color = Color::new(1.0, 1.0, 1.0, m.alpha);
            let draw = || draw_sprite(&assets.mine, s.x, s.y, m.angle, self.z + m.z, color);
            if m.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        for t in &self.field.towers {
            let s = self.to_screen(t.pos);
            if let Some(colors) = t.beam_colors() {
                // Tower::draw: the beam fan, turned to the locked aim.
                let a = t.aim.y.atan2(t.aim.x) + std::f32::consts::FRAC_PI_2;
                let (sn, cs) = a.sin_cos();
                let verts: Vec<Vertex> = tower::BEAM
                    .iter()
                    .zip(colors)
                    .map(|(v, c)| {
                        let p = s + vec2(v.x * cs - v.y * sn, v.x * sn + v.y * cs);
                        Vertex::new(p.x, p.y, self.z + t.z, 0.0, 0.0, c)
                    })
                    .collect();
                let indices: Vec<u16> = (1..9).flat_map(|i| [0, i, i + 1]).collect();
                canvas.draw_triangles(verts, indices, None, false);
            }
            let a = t.angle.to_radians();
            let off = vec2(-a.sin(), a.cos()) * tower::SPRITE_OFFSET;
            let c = s + off;
            let color = Color::new(1.0, 1.0, 1.0, t.alpha);
            let draw = || draw_sprite(&assets.tower, c.x, c.y, t.angle, self.z + t.z, color);
            if t.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        for k in &self.field.seekers {
            let s = self.to_screen(k.pos);
            let color = Color::new(1.0, 1.0, 1.0, k.alpha);
            let draw = || draw_sprite(&assets.seeker, s.x, s.y, k.angle, self.z + k.z, color);
            if k.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        for a in &self.field.arrows {
            let s = self.to_screen(a.pos);
            let color = Color::new(1.0, 1.0, 1.0, a.alpha);
            let draw = || draw_sprite(&assets.arrow, s.x, s.y, a.angle, self.z + a.z, color);
            if a.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        for sn in &self.field.snakes {
            for seg in sn.segments.iter().rev() {
                let s = self.to_screen(seg.pos);
                let color = Color::new(1.0, 1.0, 1.0, seg.alpha);
                let draw = || draw_sprite(&assets.snake_body, s.x, s.y, seg.angle, self.z + seg.z, color);
                if seg.flash {
                    canvas.flashing(draw);
                } else {
                    draw();
                }
            }
            let s = self.to_screen(sn.pos);
            let color = Color::new(1.0, 1.0, 1.0, sn.alpha);
            let draw = || draw_sprite(&assets.snake_head, s.x, s.y, sn.angle, self.z + sn.z, color);
            if sn.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        for g in &self.field.shy_guys {
            let s = self.to_screen(g.pos);
            let color = Color::new(1.0, 1.0, 1.0, g.alpha);
            let draw = || draw_atlas_sprite(&assets.shy_guy, 4, 4, g.frame, s.x, s.y, g.angle, self.z + g.z, color);
            if g.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        self.draw_boss(canvas, assets);

        // Cannon fire: green additive point sprites trailing each bullet.
        let mut points = Vec::new();
        for b in &self.ship.bullets {
            let p = self.to_screen(b.pos);
            for (behind, size) in CANNON_TRAIL {
                points.push((p + b.back * behind, size));
            }
        }
        canvas.draw_points(&assets.cannon, &points, self.z, GREEN);

        self.draw_ship(assets);
        self.effects
            .draw(canvas, &assets.spark, self.z, |p| self.to_screen(p));
        self.draw_hud(assets);
    }

    fn draw_ship(&self, assets: &PlayAssets) {
        let ship = &self.ship;
        if matches!(ship.life, Life::Dead(_)) {
            return;
        }
        let p = self.to_screen(ship.pos);
        let frames = if ship.can_boost() {
            &assets.ship_ready
        } else {
            &assets.ship
        };
        let color = Color::new(1.0, 1.0, 1.0, ship.alpha);
        draw_sprite(&frames[ship.frame], p.x, p.y, ship.angle, self.z, color);
        let shield = match ship.life {
            Life::Spawning(_) => ship.alpha,
            Life::Fading(_) if ship.shield => ship.alpha,
            Life::Fading(t) => t * ship.alpha,
            _ if ship.shield => ship.alpha,
            _ => 0.0,
        };
        if shield > 0.0 {
            draw_sprite(
                &assets.shield,
                p.x,
                p.y,
                self.elapsed * SHIELD_SPIN,
                self.z,
                Color::new(1.0, 1.0, 1.0, shield),
            );
        }
        if ship.aiming {
            let aim = p + ship.aim_dir * AIM_DISTANCE;
            draw_quad(&assets.aim, aim.x - 1.0, aim.y - 1.0, 1.0, self.z, color);
        }
    }

    /// GameScreen::draw: the HUD sits at depth 0 whatever the play layer does.
    fn draw_hud(&self, assets: &PlayAssets) {
        hud::draw(&assets.hud, &self.hud_values());
    }

    fn hud_values(&self) -> HudValues {
        HudValues {
            score: self.score,
            high_score: self.high_score,
            multiplier: self.multiplier,
            lives: self.lives,
            bombs: self.bombs,
            rings: self.rings,
        }
    }
}
