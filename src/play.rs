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

use crate::bonus::{Power, Rings};
use crate::bindings::{self, Action};
use crate::boss::{self, BossState, Piece};
use crate::depot::{self, Depot};
use crate::enemies::{wrap_near, Element, Event, Field, Rock, Script, Weapon};
use crate::gfx::{draw_atlas_sprite, draw_quad, draw_sprite, tex_size, Canvas, Grade, Light, Wave, CANVAS_H, CANVAS_W, MAX_LIGHTS};
use crate::gravity;
use crate::hud::{self, HudAssets, HudValues, Show};
use crate::pad;
use crate::particles::{Effects, Kind};
use crate::tower;
use crate::tutorial::{self, Spawn, Tutorial};
use crate::weapons::{self, FireGun, LaserGun};

const STEP: f32 = 1.0 / 60.0;

/// World::WIDTH / HEIGHT: the playfield wraps every 1024 units.
const WORLD: f32 = 1024.0;

// GameScreen: the backdrop sits BACKGROUND_Z towards the viewer; screens fly
// in from GAME_SCREEN_START_Z, fading in at 0.6/s along a sine curve.
const BACKGROUND_Z: f32 = crate::gfx::BACKGROUND_DEPTH;
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
/// Ship::bomb.
pub const SOUND_BOMB: usize = 3;
/// The right stick fires once pushed this far.
const PAD_FIRE: f32 = 0.5;
/// GameScreen::updateStateReady: the double press for the gravity gun.
const DOUBLE_PRESS: f32 = 0.3;
/// Bonus::setState(0): the ring pickup chime.
pub const SOUND_RING: usize = 9;
/// BigBonus::setState(0): power-up pickup, and the slow-motion one.
pub const SOUND_POWER: usize = 2;
pub const SOUND_SLOW: usize = 16;
/// Mine::setState(10007): Enemy::activateBonuses(50, 1).
const MINE_RING_SPEED: f32 = 50.0;
/// Lobber::setState(10007): its two ring bursts sit 10 units either side.
const LOBBER_RING_SPREAD: f32 = 10.0;
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
const MULTIPLIER_STEPS: [u64; 9] = [
    4999, 19999, 49999, 89999, 139999, 199999, 269999, 349999, 439999,
];
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
    /// fire.png and laser.png.
    pub flame: Texture2D,
    pub laser: Texture2D,
    pub depot: Texture2D,
    pub mine: Texture2D,
    pub arrow: Texture2D,
    pub tower: Texture2D,
    pub seeker: Texture2D,
    pub lobber: Texture2D,
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
    /// Normal maps of the base textures, same order (tools/normalmaps.py).
    pub normals: Vec<Texture2D>,
    /// A soft burn mark for scorch decals (`scorch_texture`).
    pub scorch: Texture2D,
    /// A round Gaussian glow for soft particles (`soft_dot_texture`).
    pub soft_dot: Texture2D,
}

/// A round white glow with a Gaussian falloff (port addition), for points
/// and line sparks.
pub fn soft_dot_texture() -> Texture2D {
    const N: u16 = 32;
    let mut img = Image::gen_image_color(N, N, Color::new(1.0, 1.0, 1.0, 0.0));
    for y in 0..N {
        for x in 0..N {
            let d = vec2(x as f32 + 0.5, y as f32 + 0.5) / N as f32 * 2.0 - Vec2::ONE;
            let a = (-d.length_squared() * 4.0).exp() * (1.0 - d.length()).clamp(0.0, 1.0).sqrt();
            img.set_pixel(x as u32, y as u32, Color::new(1.0, 1.0, 1.0, a));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// A soft, ragged burn mark: dark in the middle, fading out, with noise so
/// overlapping marks do not look stamped (port addition).
pub fn scorch_texture() -> Texture2D {
    const N: u16 = 64;
    let mut img = Image::gen_image_color(N, N, Color::new(0.0, 0.0, 0.0, 0.0));
    let mut seed: u32 = 0x9e37_79b9;
    let mut noise = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 1000) as f32 / 1000.0
    };
    for y in 0..N {
        for x in 0..N {
            let d = vec2(x as f32 + 0.5, y as f32 + 0.5) / N as f32 * 2.0 - Vec2::ONE;
            let a = d.y.atan2(d.x);
            let ragged = 0.8 + 0.12 * (a * 5.0).sin() + 0.08 * (a * 11.0).cos();
            let r = d.length() / ragged;
            let core = (1.0 - r).clamp(0.0, 1.0);
            let alpha = core.powf(0.8) * (0.75 + 0.25 * noise());
            img.set_pixel(x as u32, y as u32, Color::new(0.07, 0.045, 0.03, alpha));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}


/// TimeManager: enemies run at `factor`, easing toward the target while
/// a slow-motion power-up lasts and back to the base after (1, or what
/// survival has sped up to).
struct TimeScale {
    factor: f32,
    target: f32,
    remaining: f32,
    base: f32,
}

impl Default for TimeScale {
    fn default() -> TimeScale {
        TimeScale {
            factor: 1.0,
            target: 1.0,
            remaining: 0.0,
            base: 1.0,
        }
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
        let goal = if self.remaining > 0.0 {
            self.target
        } else {
            self.base
        };
        let next = self.factor + 2.0 * dt * (goal - self.factor);
        self.factor = if self.factor < goal {
            next.min(goal)
        } else {
            next.max(goal)
        };
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

/// Port change: shots leave from the ship's nose, this far along the aim
/// from its centre (the ship's radius), instead of from the centre.
const MUZZLE: f32 = SHIP_RADIUS;

struct Bullet {
    /// Where the bullet was last update: hits are tested along the segment.
    prev: Vec2,
    pos: Vec2,
    vel: Vec2,
    life: f32,
    /// Unit vector pointing back along the flight path, for the trail.
    back: Vec2,
    weapon: Weapon,
}

impl Bullet {
    /// How far the shot has flown from the muzzle: trails drawn behind it
    /// stop there, so a fresh shot does not reach back into the ship.
    fn flown(&self) -> f32 {
        let life = match self.weapon {
            Weapon::Cannon => CANNON_LIFE,
            Weapon::Fire => weapons::FIRE_LIFE,
            Weapon::Laser => weapons::LASER_LIFE,
        };
        (life - self.life).max(0.0) * self.vel.length()
    }
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
    weapon: Weapon,
    fire_gun: FireGun,
    laser_gun: LaserGun,
    speed_level: usize,
    boost_level: usize,
    bullets: Vec<Bullet>,
}

impl Ship {
    /// GameScreen::reset: weapon levels by world (survival starts at 1),
    /// and world 2 starts on the fire weapon.
    fn armed(world: usize) -> Ship {
        let level = if world == SURVIVAL { 1 } else { world.min(5) };
        let mut ship = Ship::new();
        ship.cannon_level = level;
        ship.cannon_timer = CANNON_INTERVAL[level];
        ship.fire_gun = FireGun::new(level);
        ship.laser_gun = LaserGun::new(level);
        ship.boost_charge = ship.boost_recharge();
        ship.weapon = if world == 1 {
            Weapon::Fire
        } else {
            Weapon::Cannon
        };
        // MB_WEAPON=0|1|2 picks the starting weapon, for screenshot runs.
        match std::env::var("MB_WEAPON").ok().as_deref() {
            Some("0") => ship.weapon = Weapon::Cannon,
            Some("1") => ship.weapon = Weapon::Fire,
            Some("2") => ship.weapon = Weapon::Laser,
            _ => {}
        }
        ship
    }

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
            weapon: Weapon::Cannon,
            fire_gun: FireGun::new(0),
            laser_gun: LaserGun::new(0),
            speed_level: 0,
            boost_level: 0,
            bullets: Vec::new(),
        }
    }

    /// Ship::reset: a fresh ship where the last one died.
    fn respawn(&mut self, recharge: f32) {
        self.life = Life::Spawning(SPAWN_SHIELD);
        self.lives_used += 1;
        self.vel = Vec2::ZERO;
        self.boost_vel = Vec2::ZERO;
        self.boost_time = 0.0;
        self.invincible = 0.0;
        self.boost_charge = recharge;
        self.bullets.clear();
    }

    fn boost_recharge(&self) -> f32 {
        BOOST_RECHARGE - self.boost_level as f32
    }

    fn can_boost(&self) -> bool {
        self.boost_charge >= self.boost_recharge()
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
        let origin = self.pos + self.vel * dt + dir * MUZZLE;
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
                weapon: Weapon::Cannon,
            });
        }
    }

    /// Ship::fire with the current weapon (Global::bulletType).
    fn shoot(&mut self, dt: f32, dir: Vec2) {
        match self.weapon {
            Weapon::Cannon => self.fire(dt, dir),
            Weapon::Fire | Weapon::Laser => {
                self.aiming = true;
                self.aim_dir = dir;
                let origin = self.pos + self.vel * dt + dir * MUZZLE;
                let mut shots = Vec::new();
                let (life, weapon) = if self.weapon == Weapon::Fire {
                    self.fire_gun.fire(dt, origin, dir, &mut shots);
                    (weapons::FIRE_LIFE, Weapon::Fire)
                } else {
                    self.laser_gun.fire(dt, origin, dir, &mut shots);
                    (weapons::LASER_LIFE, Weapon::Laser)
                };
                for (pos, vel) in shots {
                    self.bullets.push(Bullet {
                        prev: pos,
                        pos,
                        vel,
                        life,
                        back: -vel.normalize(),
                        weapon,
                    });
                }
            }
        }
    }

    /// Ship::switchWeapon: cannon, fire, laser, round again.
    fn switch_weapon(&mut self) {
        self.weapon = match self.weapon {
            Weapon::Cannon => Weapon::Fire,
            Weapon::Fire => Weapon::Laser,
            Weapon::Laser => Weapon::Cannon,
        };
    }

    /// Ship::resetBullets: the next press fires at once.
    fn hold_fire(&mut self) {
        self.cannon_timer = CANNON_INTERVAL[self.cannon_level];
        self.fire_gun.release();
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
        self.boost_charge = (self.boost_charge + dt).min(self.boost_recharge());

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
    /// Settings shake sensitivity, 0-1.
    scale: f32,
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
        self.shake_power = self.shake_power.max(power * self.scale);
    }

    fn offset(&self) -> Vec2 {
        self.centre + self.shake
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Upgrade {
    Cannon,
    Fire,
    Laser,
    Speed,
    Boost,
    Gravity,
}

pub const UPGRADES: [Upgrade; 6] = [
    Upgrade::Cannon,
    Upgrade::Fire,
    Upgrade::Laser,
    Upgrade::Speed,
    Upgrade::Boost,
    Upgrade::Gravity,
];

/// Global::canUpgrade* cost tables, by level; -1 means maxed.
const UPGRADE_COSTS: [[i32; 8]; 6] = [
    [50, 150, 250, 325, 400, 475, 525, -1],
    [50, 150, 250, 325, 400, 475, 525, -1],
    [50, 150, 250, 325, 400, 475, 525, -1],
    [200, 400, -1, 50, 150, 250, 325, 400],
    [200, 400, -1, 300, 600, -1, 200, 400],
    [300, 600, -1, 200, 400, -1, 50, 150],
];

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
    /// GameScreen 0x3c: seconds since the stage became ready.
    stage_time: f32,
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
    /// Global::upgradePoints: one per ring collected, spent in the pause
    /// popup's upgrade page.
    pub upgrade_points: u32,
    /// Cannon, fire, laser, speed, boost and gravity levels.
    upgrades: [usize; 6],
    /// Global::multiplierBaseScore: the score at the last death.
    multiplier_base: u64,
    pickups: Rings,
    /// Global::bonusAlwaysGoToShipSeconds: new rings home in at once.
    magnet: f32,
    /// GameScreen 0x30: seconds since the last trigger press.
    since_press: f32,
    time: TimeScale,
    high_score: u64,
    /// `Audio::play` ids raised this update, for the caller to play.
    pub sounds: Vec<usize>,
    effects: Effects,
    /// Test hooks, read once: env lookups take the process environment lock
    /// and `var` allocates a String.
    drift: Option<Vec2>,
    auto_fire: bool,
    gun: bool,
    auto_bomb: Option<f32>,
    auto_boost: Option<f32>,
    bg_z: Option<f32>,
    /// Global::level 7: the tutorial runs in this game.
    tutorial: Option<Tutorial>,
    /// Positions at the start of the last 60 Hz step, and how far the
    /// display is past it (0-1): frames between steps are drawn blended,
    /// so 120/144 Hz output moves smoothly (port addition).
    prev: Snapshot,
    blend: f32,
    /// Options > Lighting and effects (port addition): lit ground and
    /// sprites, shockwaves, scorch marks, grading, ship effects.
    pub fx: bool,
    /// Lights from explosions and hits, fading out.
    glows: Vec<Glow>,
    /// Shockwave rings from big explosions.
    shocks: Vec<Shock>,
    /// Burn marks on the ground, oldest first.
    scorches: Vec<Scorch>,
    /// The ship's recent positions and speeds, newest first, for its
    /// engine trail.
    trail: VecDeque<(Vec2, f32)>,
    /// This frame's lights on the canvas, for tinting sprites.
    frame_lights: std::cell::RefCell<Vec<Light>>,
}

/// The light an event leaves (port addition): position, colour, reach and
/// how long it fades.
fn event_light(event: &Event) -> Option<(Vec2, Vec3, f32, f32)> {
    let element = |e: &Element| match e {
        Element::Normal => ROCK_LIGHT,
        Element::Ice => ICE_LIGHT,
        Element::Fire => FIRE_LIGHT,
    };
    Some(match event {
        Event::RockDestroyed { pos, element: e, .. } => (*pos, element(e), 120.0, 0.45),
        Event::RockHit { pos, element: e } => (*pos, element(e) * 0.5, 50.0, 0.12),
        Event::MineHit { pos, .. } => (*pos, FIRE_LIGHT * 0.5, 50.0, 0.12),
        Event::LobberDestroyed { pos, .. }
        | Event::SeekerDestroyed { pos, .. }
        | Event::TowerDestroyed { pos, .. }
        | Event::ArrowDestroyed { pos, .. }
        | Event::SnakeDestroyed { pos, .. }
        | Event::ShyGuyDestroyed { pos, .. }
        | Event::MineDestroyed { pos, .. } => (*pos, FIRE_LIGHT, 140.0, 0.5),
        Event::SegmentDestroyed { pos, .. } | Event::PieceDestroyed { pos, .. } => {
            (*pos, FIRE_LIGHT * 0.7, 80.0, 0.3)
        }
        Event::DepotDestroyed { pos, .. } => (*pos, WHITE_LIGHT, 180.0, 0.6),
        Event::BossDying { pos } => (*pos, vec3(1.0, 0.5, 1.0), 260.0, 1.2),
        Event::BossDead { pos, .. } => (*pos, WHITE_LIGHT, 380.0, 1.0),
        Event::GravityBurst { pos } => (*pos, ICE_LIGHT, 110.0, 0.4),
        _ => return None,
    })
}

/// A burn mark where something exploded; it fades over its last seconds.
struct Scorch {
    pos: Vec2,
    size: f32,
    angle: f32,
    age: f32,
}

/// Engine trail: samples kept (one per step), and the speed at which it
/// shows fully.
const TRAIL_POINTS: usize = 16;
/// Shot streaks: how many seconds of flight they trail.
const BULLET_STREAK: f32 = 0.03;
const TRAIL_FULL_SPEED: f32 = 300.0;

const SCORCH_LIFE: f32 = 15.0;
const SCORCH_FADE: f32 = 5.0;
const MAX_SCORCHES: usize = 96;

/// The burn mark an event leaves (port addition): its size, if any.
fn event_scorch(event: &Event) -> Option<(Vec2, f32)> {
    Some(match event {
        Event::RockDestroyed { pos, size, effects: true, .. } => (*pos, 18.0 + 12.0 * *size as f32),
        Event::LobberDestroyed { pos, .. }
        | Event::SeekerDestroyed { pos, .. }
        | Event::TowerDestroyed { pos, .. }
        | Event::ArrowDestroyed { pos, .. }
        | Event::SnakeDestroyed { pos, .. }
        | Event::ShyGuyDestroyed { pos, .. }
        | Event::MineDestroyed { pos, .. } => (*pos, 30.0),
        Event::DepotDestroyed { pos, .. } => (*pos, 44.0),
        Event::BossDead { pos, .. } => (*pos, 90.0),
        _ => return None,
    })
}

/// A shockwave ring: it grows to `radius` over `life` seconds and fades.
struct Shock {
    pos: Vec2,
    radius: f32,
    strength: f32,
    age: f32,
    life: f32,
}

/// The shockwave an event sends out (port addition): reach, push, seconds.
fn event_shock(event: &Event) -> Option<(Vec2, f32, f32, f32)> {
    Some(match event {
        Event::RockDestroyed { pos, size: 2, .. } => (*pos, 80.0, 4.0, 0.4),
        Event::RockDestroyed { pos, size: 3, .. } => (*pos, 110.0, 6.0, 0.45),
        Event::DepotDestroyed { pos, .. } => (*pos, 130.0, 6.0, 0.5),
        Event::BossDying { pos } => (*pos, 120.0, 5.0, 0.5),
        Event::BossDead { pos, .. } => (*pos, 300.0, 12.0, 0.9),
        _ => return None,
    })
}

/// Colour grading per world (port addition): blue, fire, green, sprawl,
/// craters, then space for world 6 and survival.
fn world_grade(world: usize) -> Grade {
    let g = |lift: Vec3, gain: Vec3, saturation: f32| Grade { lift, gain, saturation, vignette: 0.25 };
    match world {
        0 => g(vec3(0.0, 0.005, 0.015), vec3(0.98, 1.0, 1.06), 1.1),
        1 => g(vec3(0.02, 0.005, 0.0), vec3(1.08, 0.98, 0.9), 1.15),
        2 => g(Vec3::ZERO, vec3(0.97, 1.05, 0.97), 1.1),
        3 => g(vec3(0.01, 0.0, 0.01), vec3(1.05, 0.97, 1.0), 1.05),
        4 => g(vec3(0.01, 0.01, 0.0), vec3(1.02, 1.04, 0.95), 1.08),
        _ => g(Vec3::ZERO, vec3(1.0, 0.98, 1.06), 1.15),
    }
}

/// A light left by an explosion or hit, in world coordinates.
struct Glow {
    pos: Vec2,
    color: Vec3,
    radius: f32,
    life: f32,
    max: f32,
}

/// Light colours: fire, ice, rock dust, the ship's green, white heat.
const FIRE_LIGHT: Vec3 = vec3(1.0, 0.55, 0.2);
const ICE_LIGHT: Vec3 = vec3(0.45, 0.75, 1.0);
const ROCK_LIGHT: Vec3 = vec3(1.0, 0.75, 0.45);
const SHIP_LIGHT: Vec3 = vec3(0.35, 1.0, 0.45);
const WHITE_LIGHT: Vec3 = vec3(1.0, 0.95, 0.85);

/// Where everything stood at the start of a step. A list whose length
/// changed in the step (a spawn or a kill) is drawn unblended that frame.
#[derive(Default)]
struct Snapshot {
    ship: Vec2,
    cam: Vec2,
    scroll: Vec2,
    rocks: Vec<Vec2>,
    depots: Vec<Vec2>,
    mines: Vec<Vec2>,
    lobbers: Vec<Vec2>,
    seekers: Vec<Vec2>,
    arrows: Vec<Vec2>,
    snakes: Vec<Vec2>,
    segments: Vec<Vec2>,
    shy_guys: Vec<Vec2>,
    pieces: Vec<Vec2>,
    boss: Vec<Vec2>,
    rings: Vec<Vec2>,
}

/// Farther than this in one step is a jump (a respawn or a wrap), not
/// motion to blend.
const BLEND_JUMP: f32 = 80.0;

/// `t` of the way from `prev` to `cur` in the wrapping world; a jump is
/// drawn where it landed.
fn blend_pos(prev: Vec2, cur: Vec2, t: f32) -> Vec2 {
    let p = vec2(wrap_near(prev.x, cur.x), wrap_near(prev.y, cur.y));
    if p.distance_squared(cur) > BLEND_JUMP * BLEND_JUMP {
        return cur;
    }
    p.lerp(cur, t)
}

impl Play {
    /// `world` 0-5 for arcade worlds, 6 for survival.
    pub fn new(world: usize, stage: usize, high_score: u64, assets: &PlayAssets) -> Play {
        let level = if world == SURVIVAL { 1 } else { world.min(5) };
        let drift = std::env::var("MB_DRIFT").ok().and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some(vec2(x.parse().ok()?, y.parse().ok()?))
        });
        let auto_bomb = std::env::var("MB_BOMB_AT").ok().and_then(|v| v.parse().ok());
        let bg_z = std::env::var("MB_BG_Z").ok().and_then(|v| v.parse().ok());
        Play {
            world,
            stage,
            state: State::Entering,
            alpha: 0.0,
            z: START_Z,
            elapsed: 0.0,
            stage_time: 0.0,
            shaken: false,
            ship: Ship::armed(world),
            camera: Lead {
                centre: Vec2::ZERO,
                shake: Vec2::ZERO,
                shake_power: 0.0,
                shake_time: 0.0,
                scale: 1.0,
            },
            scroll: Vec2::ZERO,
            accumulator: 0.0,
            field: Field::new(assets.rocks[0].frame_counts()),
            script: if world == SURVIVAL {
                Script::survival()
            } else {
                Script::arcade(world, stage)
            },
            score: 0,
            multiplier: 1,
            lives: if world == SURVIVAL {
                SURVIVAL_LIVES
            } else {
                LIVES
            },
            bombs: BOMBS,
            rings: 0,
            upgrade_points: 0,
            upgrades: [level, level, level, 0, 0, 0],
            multiplier_base: 0,
            pickups: Rings::default(),
            magnet: 0.0,
            since_press: 1.0,
            time: TimeScale::default(),
            high_score,
            sounds: Vec::new(),
            effects: Effects::load(&assets.systems),
            drift,
            auto_fire: std::env::var_os("MB_AUTOFIRE").is_some(),
            gun: std::env::var_os("MB_GUN").is_some(),
            auto_bomb,
            auto_boost: std::env::var("MB_BOOST_AT").ok().and_then(|v| v.parse().ok()),
            bg_z,
            tutorial: None,
            prev: Snapshot::default(),
            blend: 1.0,
            fx: true,
            glows: Vec::new(),
            shocks: Vec::new(),
            scorches: Vec::new(),
            trail: VecDeque::new(),
            frame_lights: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// GameScreen with gameScreenType 0: level 7 is world 1's backdrop with
    /// the weapons at level 0 and no waves; `Tutorial` drives it.
    pub fn tutorial(high_score: u64, assets: &PlayAssets, tutorial: Tutorial) -> Play {
        let mut play = Play::new(0, 1, high_score, assets);
        play.pickups.timeless = true;
        play.tutorial = Some(tutorial);
        play
    }

    /// The tutorial ran to its end, or its Quit button was pressed.
    pub fn is_tutorial(&self) -> bool {
        self.tutorial.is_some()
    }

    pub fn tutorial_complete(&self) -> bool {
        self.tutorial.as_ref().is_some_and(Tutorial::is_complete)
    }

    /// Tutorial::canUsePause: the pause button unlocks at step 20.
    pub fn can_pause(&self) -> bool {
        self.tutorial.as_ref().is_none_or(Tutorial::can_pause)
    }

    /// Whether the tutorial allows a control (always outside it).
    fn allowed(&self, gate: fn(&Tutorial) -> bool) -> bool {
        self.tutorial.as_ref().is_none_or(gate)
    }

    /// The stage is over: `Some(true)` cleared, `Some(false)` out of lives.
    pub fn result(&self) -> Option<bool> {
        match self.state {
            State::Over { cleared } => Some(cleared),
            _ => None,
        }
    }

    pub fn stage_time(&self) -> f32 {
        self.stage_time
    }

    /// Goal Time is 150 s for every arcade world.
    pub fn time_bonus(&self) -> f32 {
        if self.world == SURVIVAL {
            0.0
        } else {
            (150.0 - self.stage_time).max(0.0)
        }
    }

    /// Global::increaseScore for the results screen's time bonus.
    pub fn apply_time_bonus(&mut self, seconds: f32) {
        self.score += (seconds.round().max(0.0) as u64) * self.multiplier as u64 * 100;
    }

    /// The next arcade stage after a clear, or `None` after world 6.
    pub fn next_stage(&self) -> Option<(usize, usize)> {
        if self.world == SURVIVAL {
            None
        } else if self.stage < 5 {
            Some((self.world, self.stage + 1))
        } else if self.world < 5 {
            Some((self.world + 1, 1))
        } else {
            None
        }
    }

    /// `GameScreen::handleEvent`'s next-stage case: advance inside the same
    /// screen, so the run's score, rings, upgrades, lives and bombs carry
    /// over; only the stage's world state restarts. Returns false after the
    /// last stage.
    pub fn advance_stage(&mut self, assets: &PlayAssets) -> bool {
        let Some((world, stage)) = self.next_stage() else {
            return false;
        };
        let weapon = self.ship.weapon;
        self.world = world;
        self.stage = stage;
        self.state = State::Entering;
        self.alpha = 0.0;
        self.z = START_Z;
        self.elapsed = 0.0;
        self.stage_time = 0.0;
        self.shaken = false;
        self.camera = Lead {
            centre: Vec2::ZERO,
            shake: Vec2::ZERO,
            shake_power: 0.0,
            shake_time: 0.0,
            scale: 1.0,
        };
        self.scroll = Vec2::ZERO;
        self.accumulator = 0.0;
        self.field = Field::new(assets.rocks[0].frame_counts());
        self.script = Script::arcade(world, stage);
        self.pickups = Rings::default();
        self.magnet = 0.0;
        self.since_press = 1.0;
        self.time = TimeScale::default();

        // Ship::resetBoost and Ship::loadWeaponStateFromGlobal: the boost
        // recharges for the new stage, while the bought levels and the
        // selected weapon stay with the run.
        let mut ship = Ship::armed(world);
        ship.weapon = weapon;
        ship.cannon_level = self.upgrades[Upgrade::Cannon as usize];
        ship.cannon_timer = CANNON_INTERVAL[ship.cannon_level];
        ship.fire_gun.level = self.upgrades[Upgrade::Fire as usize];
        ship.laser_gun.level = self.upgrades[Upgrade::Laser as usize];
        ship.speed_level = self.upgrades[Upgrade::Speed as usize];
        ship.boost_level = self.upgrades[Upgrade::Boost as usize];
        ship.boost_charge = ship.boost_recharge();
        self.field.gun.level = self.upgrades[Upgrade::Gravity as usize] as u32;
        self.ship = ship;
        true
    }

    pub fn set_shake_scale(&mut self, scale: f32) {
        self.camera.scale = scale.clamp(0.0, 1.0);
    }

    pub fn upgrade_level(&self, u: Upgrade) -> usize {
        self.upgrades[u as usize]
    }

    /// All six upgrade levels, for run diagnostics.
    pub fn upgrade_levels(&self) -> [usize; 6] {
        self.upgrades
    }

    pub fn upgrade_cost(&self, u: Upgrade) -> Option<u32> {
        let cost = UPGRADE_COSTS[u as usize][self.upgrades[u as usize]];
        (cost >= 0).then_some(cost as u32)
    }

    pub fn can_buy(&self, u: Upgrade) -> bool {
        self.upgrade_cost(u)
            .is_some_and(|cost| cost <= self.upgrade_points)
    }

    /// PausePopup::handleEvent upgrade cases: spend rings and apply the
    /// upgrade to the ship.
    pub fn buy_upgrade(&mut self, u: Upgrade) -> bool {
        let Some(cost) = self.upgrade_cost(u) else {
            return false;
        };
        if cost > self.upgrade_points {
            return false;
        }
        self.upgrade_points -= cost;
        let level = self.upgrades[u as usize] + 1;
        self.upgrades[u as usize] = level;
        match u {
            Upgrade::Cannon => {
                self.ship.cannon_level = level;
                self.ship.cannon_timer = CANNON_INTERVAL[level];
            }
            Upgrade::Fire => self.ship.fire_gun.level = level,
            Upgrade::Laser => self.ship.laser_gun.level = level,
            Upgrade::Speed => self.ship.speed_level = level,
            Upgrade::Boost => {
                self.ship.boost_level = level;
                self.ship.boost_charge = self.ship.boost_recharge();
            }
            Upgrade::Gravity => self.field.gun.level = level as u32,
        }
        true
    }

    fn speed(&self) -> f32 {
        SHIP_SPEED + self.ship.speed_level as f32
    }

    fn boost_recharge(&self) -> f32 {
        self.ship.boost_recharge()
    }

    /// Where a world position appears on the canvas.
    /// Where a world position appears on the canvas, seen from the blended
    /// view.
    fn to_screen(&self, p: Vec2) -> Vec2 {
        let view = self.view_ship();
        let d = vec2(wrap_near(p.x, view.x) - view.x, wrap_near(p.y, view.y) - view.y);
        SCREEN_CENTRE + self.view_cam() + d
    }

    /// The ship's position as drawn this frame: the camera follows it.
    fn view_ship(&self) -> Vec2 {
        self.blended(&[self.prev.ship], 1, 0, self.ship.pos)
    }

    fn view_cam(&self) -> Vec2 {
        self.prev.cam.lerp(self.camera.offset(), self.blend)
    }

    /// Entry `i` of a list now `len` long, at `cur`, blended from where it
    /// stood at the step's start.
    fn blended(&self, prev: &[Vec2], len: usize, i: usize, cur: Vec2) -> Vec2 {
        if prev.len() != len {
            return cur;
        }
        blend_pos(prev[i], cur, self.blend)
    }

    /// A bullet as drawn: shots die too often for list blending, so they
    /// step back along their (constant) velocity instead.
    fn bullet_pos(&self, b: &Bullet) -> Vec2 {
        b.pos - b.vel * (1.0 - self.blend) * STEP
    }

    /// Screen position of entry `i` of a list (see `blended`).
    fn at(&self, prev: &[Vec2], len: usize, i: usize, cur: Vec2) -> Vec2 {
        self.to_screen(self.blended(prev, len, i, cur))
    }

    /// A light that fades out over `seconds`.
    fn glow(&mut self, pos: Vec2, color: Vec3, radius: f32, seconds: f32) {
        if self.glows.len() < 64 {
            self.glows.push(Glow { pos, color, radius, life: seconds, max: seconds });
        }
    }

    fn shock(&mut self, pos: Vec2, radius: f32, strength: f32, life: f32) {
        if self.shocks.len() < 8 {
            self.shocks.push(Shock { pos, radius, strength, age: 0.0, life });
        }
    }

    /// The shockwaves on the canvas this frame, strongest first.
    fn waves(&self) -> Vec<Wave> {
        let mut waves: Vec<Wave> = self
            .shocks
            .iter()
            .map(|s| {
                let t = s.age / s.life;
                Wave {
                    pos: self.to_screen(s.pos),
                    radius: s.radius * (1.0 - (1.0 - t) * (1.0 - t)),
                    strength: s.strength * (1.0 - t),
                }
            })
            .collect();
        waves.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        waves
    }

    /// The frame's strongest lights on the canvas: fading explosion glows,
    /// then shots and the ship.
    fn collect_lights(&self) -> Vec<Light> {
        let mut lights: Vec<Light> = Vec::new();
        let edge = crate::gfx::edge();
        let mut add = |pos: Vec2, radius: f32, color: Vec3| {
            let on_screen = pos.x > -edge - radius
                && pos.x < CANVAS_W + edge + radius
                && pos.y > -radius
                && pos.y < CANVAS_H + radius;
            if on_screen {
                lights.push(Light { pos, radius, color });
            }
        };
        for g in &self.glows {
            let t = g.life / g.max;
            add(self.to_screen(g.pos), g.radius * (0.6 + 0.4 * t), g.color * t * t);
        }
        // Shots: every few, so a stream reads as a moving light.
        for (i, b) in self.ship.bullets.iter().enumerate() {
            let (every, radius, color) = match b.weapon {
                Weapon::Fire => (6, 80.0, FIRE_LIGHT * 0.6),
                Weapon::Laser => (2, 70.0, ICE_LIGHT * 0.8),
                Weapon::Cannon => (3, 50.0, SHIP_LIGHT * 0.5),
            };
            if i % every == 0 {
                add(self.to_screen(self.bullet_pos(b)), radius, color);
            }
        }
        if !matches!(self.ship.life, Life::Dead(_)) {
            let boost = if self.ship.invincible > 0.0 { 1.5 } else { 1.0 };
            add(self.to_screen(self.view_ship()), 60.0 * boost, SHIP_LIGHT * 0.3 * boost);
        }
        lights.sort_by(|a, b| (b.color.length() * b.radius).total_cmp(&(a.color.length() * a.radius)));
        lights.truncate(MAX_LIGHTS);
        lights
    }

    /// A sprite's vertex colour: lit by the frame's lights when effects
    /// are on (the lit material reads 0.5 as unlit), else plain white.
    fn tint(&self, at: Vec2, alpha: f32) -> Color {
        if !self.fx {
            return Color::new(1.0, 1.0, 1.0, alpha);
        }
        let mut light = Vec3::ZERO;
        for l in self.frame_lights.borrow().iter() {
            let fall = (1.0 - l.pos.distance(at) / l.radius.max(1.0)).clamp(0.0, 1.0);
            light += l.color * fall * fall;
        }
        let c = (Vec3::splat(0.5) + light * 0.5).min(Vec3::ONE);
        Color::new(c.x, c.y, c.z, alpha)
    }

    fn snapshot(&self) -> Snapshot {
        let f = &self.field;
        Snapshot {
            ship: self.ship.pos,
            cam: self.camera.offset(),
            scroll: self.scroll,
            rocks: f.rocks.iter().map(|e| e.pos).collect(),
            depots: f.depots.iter().map(|e| e.pos).collect(),
            mines: f.mines.iter().map(|e| e.pos).collect(),
            lobbers: f.lobbers.iter().map(|e| e.pos).collect(),
            seekers: f.seekers.iter().map(|e| e.pos).collect(),
            arrows: f.arrows.iter().map(|e| e.pos).collect(),
            snakes: f.snakes.iter().map(|e| e.pos).collect(),
            segments: f.snakes.iter().flat_map(|s| s.segments.iter().map(|g| g.pos)).collect(),
            shy_guys: f.shy_guys.iter().map(|e| e.pos).collect(),
            pieces: f.pieces.iter().map(|e| e.pos).collect(),
            boss: f.boss.iter().map(|e| e.pos).collect(),
            rings: self.pickups.rings.iter().map(|e| e.pos).collect(),
        }
    }

    /// The ship's world position, for screenshot diagnostics.
    pub fn ship_pos(&self) -> Vec2 {
        self.ship.pos
    }

    /// While a popup covers the game, only its visuals keep time: particle
    /// effects and flashes, lights, shockwaves and scorch marks run out
    /// behind the popup instead of freezing at their peak (a boss's white
    /// death flash and shockwave stayed on the results screen).
    pub fn update_visuals(&mut self, dt: f32) {
        self.accumulator += dt;
        while self.accumulator >= STEP {
            self.accumulator -= STEP;
            self.age_visuals(STEP);
        }
    }

    /// One step of the purely visual timers.
    fn age_visuals(&mut self, dt: f32) {
        self.effects.update(dt);
        self.glows.retain_mut(|g| {
            g.life -= dt;
            g.life > 0.0
        });
        self.shocks.retain_mut(|s| {
            s.age += dt;
            s.age < s.life
        });
        self.scorches.retain_mut(|s| {
            s.age += dt;
            s.age < SCORCH_LIFE
        });
    }

    pub fn update(&mut self, dt: f32) {
        self.accumulator += dt;
        while self.accumulator >= STEP {
            self.accumulator -= STEP;
            self.prev = self.snapshot();
            self.step(STEP);
        }
        self.blend = (self.accumulator / STEP).clamp(0.0, 1.0);
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
        self.age_visuals(dt);
        self.elapsed += dt;
    }

    /// GameScreen::updateStateReady with keyboard and mouse in place of the
    /// two touch wheels.
    fn step_ready(&mut self, dt: f32) {
        self.stage_time += dt;
        if let Life::Dead(t) = self.ship.life {
            // GameScreen::updateStateReady: two seconds after a death, the
            // next ship, or the end of the game.
            self.ship.life = Life::Dead(t + dt);
            if t + dt >= RESPAWN_DELAY {
                if self.lives == 0 {
                    self.state = State::Over { cleared: false };
                } else {
                    self.lives -= 1;
                    self.ship.respawn(self.boost_recharge());
                }
            }
            self.ship.vel = Vec2::ZERO;
        } else {
            self.control(dt);
        }
        self.ship.update(dt);
        let total = self.ship.vel + self.ship.boost_vel;
        self.scroll += total * dt;
        if matches!(self.ship.life, Life::Dead(_)) {
            self.trail.clear();
        } else {
            self.trail.push_front((self.ship.pos, total.length()));
            self.trail.truncate(TRAIL_POINTS);
        }
        self.camera.update(dt, total * CAMERA_LEAD);

        if self.tutorial.is_none() {
            self.script.update(dt, self.ship.pos, &mut self.field);
        }
        self.time.base = self.script.speed();
        self.time.update(dt);
        self.field.time_factor = self.time.factor;
        self.field.aim = self.ship.aim_dir;
        self.field.update(dt * self.time.factor, self.ship.pos);
        self.follow_boss_death();
        self.update_rings(dt);
        self.magnet = (self.magnet - dt).max(0.0);
        self.collide();
        self.handle_events();
        self.update_multiplier();
        if self.tutorial.is_some() {
            self.update_tutorial(dt);
        } else if self.script.complete(&self.field) {
            self.state = State::Over { cleared: true };
        }
    }

    /// Tutorial::update, then the enemies a finished step sets up.
    fn update_tutorial(&mut self, dt: f32) {
        let touching = is_mouse_button_down(MouseButton::Left)
            || !get_keys_down().is_empty()
            || pad::any_down();
        // The Quit button; Esc or the pad's Start stand in for it until
        // the pause button unlocks.
        let quit_key = is_key_pressed(KeyCode::Escape) || bindings::pressed(Action::Pause);
        let quit = (is_mouse_button_pressed(MouseButton::Left)
            && tutorial::QUIT_RECT.contains(Canvas::mouse()))
            || (quit_key && !self.can_pause());
        let field = tutorial::Field {
            normal_rocks: self.field.rocks_of(Element::Normal),
            ice_rocks: self.field.rocks_of(Element::Ice),
            fire_rocks: self.field.rocks_of(Element::Fire),
            balls: self.field.pieces.len(),
            rings: self.pickups.rings.len(),
        };
        let Some(t) = self.tutorial.as_mut() else { return };
        if quit && !t.is_complete() {
            self.sounds.push(SOUND_RING);
        }
        let ship = self.ship.pos;
        let rock = ship + vec2(100.0, 0.0);
        match t.update(dt, touching, quit, field) {
            Some(Spawn::LargeNormalRock) => self.field.spawn_still(Element::Normal, rock),
            Some(Spawn::LargeIceRock) => self.field.spawn_still(Element::Ice, rock),
            Some(Spawn::LargeFireRock) => self.field.spawn_still(Element::Fire, rock),
            Some(Spawn::Balls) => {
                for d in [vec2(40.0, 0.0), vec2(0.0, -40.0), vec2(-35.0, 35.0)] {
                    self.field.pieces.push(Piece::tutorial(ship + d));
                }
            }
            Some(Spawn::Boss) => self.field.spawn_tutorial_boss(ship + vec2(10.0, 0.0)),
            None => {}
        }
    }

    /// The cannon against rocks, then rocks against the ship (shipCollision).
    fn collide(&mut self) {
        let field = &mut self.field;
        let effects = &mut self.effects;
        let fire_damage = weapons::FIRE_DAMAGE[self.ship.fire_gun.level];
        self.ship.bullets.retain(|b| {
            let (radius, damage, mass) = match b.weapon {
                Weapon::Cannon => (CANNON_RADIUS, CANNON_DAMAGE, CANNON_MASS),
                Weapon::Fire => (weapons::FIRE_RADIUS, fire_damage, weapons::FIRE_MASS),
                Weapon::Laser => (
                    weapons::LASER_RADIUS,
                    weapons::LASER_DAMAGE,
                    weapons::LASER_MASS,
                ),
            };
            let hit = field.bullet(b.prev, b.pos, radius, b.weapon, damage, mass, b.vel);
            if hit {
                // standard/fire/laserBulletCollision: a spark, and a streak
                // back along the shot for the cannon (line 0) and laser (1).
                effects.start(Kind::Sprite, 0, b.pos, 0.0, false);
                match b.weapon {
                    Weapon::Cannon => effects.start(Kind::Line, 0, b.pos, heading(-b.vel), false),
                    Weapon::Laser => effects.start(Kind::Line, 1, b.pos, heading(-b.vel), false),
                    Weapon::Fire => 0,
                };
            }
            !hit
        });
        // Tutorial::active: the ship cannot be hit.
        if matches!(self.ship.life, Life::Dead(_)) || self.tutorial.is_some() {
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
        // Lobbers die on contact and take the ship with them unless
        // boosting; a held ball is smashed by a boost and deadly otherwise.
        let (bodies, balls) = self.field.touching_lobbers(self.ship.pos, SHIP_RADIUS);
        for &i in &balls {
            if boosting {
                self.field.destroy_held_ball(i);
            } else {
                hit = true;
            }
        }
        let mut bodies = bodies;
        bodies.sort_unstable_by(|a, b| b.cmp(a));
        for i in bodies {
            self.field.destroy_lobber(i, true, true, true);
            hit |= !boosting;
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
            let id = if boosting {
                self.ship.boosts
            } else {
                self.ship.lives_used
            };
            let dead = self
                .field
                .boss
                .as_mut()
                .is_some_and(|b| b.ram(boosting, id));
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
        self.field.gun_release();
        self.ship.life = Life::Dead(0.0);
        self.script.slow_down_survival();
        self.ship.bullets.clear();
        self.ship.aiming = false;
        self.camera.shake(0.75, 7.5);
        self.script.delay();
        self.sounds.push(SOUND_SHIP_DESTROYED);
        self.glow(self.ship.pos, SHIP_LIGHT, 200.0, 0.7);
        self.shock(self.ship.pos, 160.0, 8.0, 0.6);
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
        let mut ring_sound = false;
        for (pos, power) in self.pickups.update(dt, ship, self.ship.pos) {
            match power {
                None => {
                    // One ring sound however many land this update.
                    if !ring_sound {
                        self.sounds.push(SOUND_RING);
                        ring_sound = true;
                    }
                    self.effects
                        .start(Kind::Point, 15, pos, rand::gen_range(0.0, 360.0), false);
                    self.rings += 1;
                    self.upgrade_points += 1;
                    self.score += self.multiplier as u64;
                }
                Some(power) => self.collect_power(pos, power),
            }
        }
    }

    /// BigBonus::setState(0).
    fn collect_power(&mut self, pos: Vec2, power: Power) {
        self.effects.start(Kind::Point, 16, pos, 0.0, false);
        self.sounds.push(if power == Power::Slow {
            SOUND_SLOW
        } else {
            SOUND_POWER
        });
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
                self.magnet = if self.world == SURVIVAL {
                    SURVIVAL_MAGNET_SECONDS
                } else {
                    MAGNET_SECONDS
                };
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

    /// Ship::bomb.
    fn bomb(&mut self) {
        if self.bombs == 0 || matches!(self.ship.life, Life::Dead(_)) {
            return;
        }
        self.bombs -= 1;
        self.script.slow_down_survival();
        self.sounds.push(SOUND_BOMB);
        // The original's flash already whitens the screen; the light only
        // warms the ground around it.
        self.glow(self.ship.pos, WHITE_LIGHT * 0.5, 420.0, 0.8);
        self.shock(self.ship.pos, 260.0, 10.0, 0.7);
        self.field.bomb();
        self.effects.flash(Color::new(0.8, 0.8, 0.8, 1.0));
        self.camera.shake(0.75, 7.5);
        let p = self.ship.pos;
        self.effects.start(Kind::Ring, 5, p, 0.0, false);
        self.effects.start(Kind::Sprite, 8, p, 0.0, false);
        self.script.delay();
    }

    /// Ship::deactivateShield.
    fn break_shield(&mut self) {
        self.ship.shield = false;
        self.script.slow_down_survival();
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
            if let Some((pos, color, radius, seconds)) = event_light(&event) {
                self.glow(pos, color, radius, seconds);
            }
            if let Some((pos, radius, strength, seconds)) = event_shock(&event) {
                self.shock(pos, radius, strength, seconds);
            }
            if let Some((pos, size)) = event_scorch(&event) {
                if self.scorches.len() >= MAX_SCORCHES {
                    self.scorches.remove(0);
                }
                let angle = rand::gen_range(0.0, std::f32::consts::TAU);
                self.scorches.push(Scorch { pos, size, angle, age: 0.0 });
            }
            match event {
                Event::RockDestroyed {
                    pos,
                    element,
                    size: _,
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
                Event::GravityBurst { pos } => {
                    self.effects.start(Kind::Ring, 7, pos, 0.0, false);
                }
                Event::Shake => self.camera.shake(0.75, 7.5),
                Event::MineHit { pos, sound, sparks } => {
                    if sound {
                        self.sounds.push(SOUND_DEPOT_HIT);
                    }
                    if sparks {
                        let away = self.towards_ship(pos) + 180.0;
                        self.effects.start(Kind::Point, 3, pos, away, false);
                    }
                }
                Event::LobberDestroyed {
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
                        self.effects.start(Kind::Sprite, 5, pos, 0.0, false);
                        self.effects.start(
                            Kind::Ring,
                            3,
                            pos + vec2(LOBBER_RING_SPREAD, 0.0),
                            0.0,
                            false,
                        );
                        self.effects.start(
                            Kind::Ring,
                            3,
                            pos - vec2(LOBBER_RING_SPREAD, 0.0),
                            0.0,
                            false,
                        );
                        self.effects.start(Kind::Point, 5, pos, 0.0, false);
                    }
                    self.score += (points * self.multiplier) as u64;
                    self.pickups
                        .drop(pos, MINE_RING_SPEED, rings, self.magnet > 0.0);
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
                            self.effects
                                .start(Kind::Line, 6, pos, a + 90.0 * k as f32, false);
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
                            self.effects
                                .start(Kind::Line, 7, pos, a + 90.0 * k as f32, false);
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
                    if let Some(t) = &mut self.tutorial {
                        t.using_boss();
                    }
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
                            self.effects
                                .start(Kind::Line, 9, pos, a + 90.0 * k as f32, false);
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
        // Bindings (controls.txt, the Options screen's Controls page).
        let held = |a: Action| bindings::down(a) as i32;
        let stick = vec2(
            (held(Action::Right) - held(Action::Left)) as f32,
            (held(Action::Down) - held(Action::Up)) as f32,
        );
        // A controller's left stick is the original's left wheel: speed
        // follows how far it is pushed.
        // MB_DRIFT=x,y holds the stick there, for screenshot runs.
        let pad_stick = self.drift.unwrap_or_else(pad::left);
        // Tutorial::canUse*: each control works once its step is reached,
        // and Tutorial::using* counts the uses a step waits for.
        let (stick, pad_stick) = if self.allowed(Tutorial::can_move) {
            (stick, pad_stick)
        } else {
            (Vec2::ZERO, Vec2::ZERO)
        };
        if stick != Vec2::ZERO || pad_stick != Vec2::ZERO {
            if let Some(t) = &mut self.tutorial {
                t.using_move(dt);
            }
        }
        if stick != Vec2::ZERO {
            self.ship.vel = stick.normalize() * WHEEL_MAX * self.speed();
        } else if pad_stick != Vec2::ZERO {
            self.ship.vel = pad_stick * WHEEL_MAX * self.speed();
        } else {
            self.ship.vel *= DRAG / (dt + DRAG);
        }
        // The original's switch-weapon button (Q, the wheel, X or Y).
        let switch = bindings::pressed(Action::SwitchWeapon);
        if switch && self.allowed(Tutorial::can_switch_weapon) {
            self.ship.switch_weapon();
            if let Some(t) = &mut self.tutorial {
                t.using_switch_weapon();
            }
        }
        // Bombs: the original's on-screen bomb button; right mouse or B here.
        // MB_BOMB_AT=<seconds> drops one unattended, for screenshot runs.
        let auto_bomb = self
            .auto_bomb
            .is_some_and(|t| self.elapsed >= t && self.elapsed - dt < t);
        let bomb = auto_bomb || bindings::pressed(Action::Bomb);
        if bomb && self.allowed(Tutorial::can_bomb) {
            self.bomb();
            if let Some(t) = &mut self.tutorial {
                t.using_bomb();
            }
        }
        // MB_BOOST_AT=<seconds> boosts unattended, for screenshot runs.
        let auto_boost = self
            .auto_boost
            .is_some_and(|t| self.elapsed >= t && self.elapsed - dt < t);
        let boost = auto_boost || bindings::pressed(Action::Boost);
        if boost && self.ship.can_boost() && self.allowed(Tutorial::can_boost) {
            self.ship.boost(dt);
            self.camera.shake(0.75, 7.5);
            self.sounds.push(SOUND_BOOST);
            if let Some(t) = &mut self.tutorial {
                t.using_boost();
            }
        }
        let charge = self.ship.boost_charge;
        if !self.ship.boost_cued
            && charge >= self.boost_recharge() - BOOST_CUE_EARLY
            && charge < self.boost_recharge() - BOOST_CUE_LATE
        {
            self.ship.boost_cued = true;
            self.sounds.push(SOUND_BOOST_READY);
        }
        // MB_AUTOFIRE aims at the nearest rock, for unattended screenshot runs.
        let can_fire = self.allowed(Tutorial::can_fire);
        let auto = self
            .auto_fire
            .then(|| self.nearest_rock())
            .flatten()
            .filter(|_| can_fire);
        if let Some(target) = auto {
            let dir = (target - self.ship.pos).normalize_or_zero();
            if dir != Vec2::ZERO {
                self.ship.shoot(dt, dir);
            }
            // MB_GUN: reach out every 2 s and throw 1.5 s later.
            if self.gun {
                let phase = self.elapsed % 2.0;
                if phase < dt {
                    self.activate_gun();
                } else if (1.5..1.5 + dt).contains(&phase) {
                    self.release_gun();
                }
            }
        } else if can_fire && bindings::key_down(Action::Fire) {
            // Keyboard and mouse fire toward the cursor.
            let dir = (Canvas::mouse() - self.to_screen(self.view_ship())).normalize_or_zero();
            if dir != Vec2::ZERO {
                self.ship.shoot(dt, dir);
            }
            // GameScreen::updateStateReady: a second press within 0.3 s of
            // the last (or the gravity binding) reaches out with the gun.
            if bindings::key_pressed(Action::Fire) {
                if self.since_press < DOUBLE_PRESS {
                    self.activate_gun();
                }
                self.since_press = 0.0;
            }
            if bindings::pressed(Action::Gravity) {
                self.activate_gun();
            }
        } else if let Some(dir) = self.pad_aim().filter(|_| can_fire) {
            // The right stick is the original's fire wheel: aim and shoot.
            // The fire binding (RT) fires along the last aim.
            self.ship.shoot(dt, dir);
            if bindings::pressed(Action::Gravity) {
                self.activate_gun();
            }
        } else {
            self.ship.hold_fire();
            if bindings::pressed(Action::Gravity) {
                self.activate_gun();
            }
            // Letting go of fire throws what the gun holds; holding the
            // gravity binding keeps the grip.
            if !bindings::down(Action::Gravity) {
                self.release_gun();
            }
        }
        if self.ship.aiming {
            if let Some(t) = &mut self.tutorial {
                t.using_fire(dt);
            }
        }
        self.since_press += dt;
    }

    /// GravityGun::activate, once the tutorial allows it.
    fn activate_gun(&mut self) {
        if self.allowed(Tutorial::can_gravity) {
            self.field.gun.activate();
        }
    }

    /// GravityGun::release: throwing something it caught is a use.
    fn release_gun(&mut self) {
        if self.field.gripped().is_some() {
            if let Some(t) = &mut self.tutorial {
                t.using_gravity();
            }
        }
        self.field.gun_release();
    }

    /// Firing direction from a controller: the right stick pushed past
    /// half way, or the right trigger along the current aim.
    fn pad_aim(&self) -> Option<Vec2> {
        let r = pad::right();
        if r.length() > PAD_FIRE {
            Some(r.normalize())
        } else if bindings::pad_down(Action::Fire) {
            Some(self.ship.aim_dir)
        } else {
            None
        }
    }

    fn nearest_rock(&self) -> Option<Vec2> {
        let ship = self.ship.pos;
        self.field
            .rocks
            .iter()
            .filter(|r| r.active())
            .map(|r| r.pos)
            .chain(
                self.field
                    .depots
                    .iter()
                    .filter(|d| d.active())
                    .map(|d| d.pos),
            )
            .chain(
                self.field
                    .mines
                    .iter()
                    .filter(|m| m.active())
                    .map(|m| m.pos),
            )
            .chain(
                self.field
                    .shy_guys
                    .iter()
                    .filter(|g| g.active())
                    .map(|g| g.pos),
            )
            .chain(
                self.field
                    .snakes
                    .iter()
                    .filter(|s| s.active())
                    .map(|s| s.pos),
            )
            .chain(
                self.field
                    .arrows
                    .iter()
                    .filter(|a| a.active())
                    .map(|a| a.pos),
            )
            .chain(
                self.field
                    .towers
                    .iter()
                    .filter(|t| t.active())
                    .map(|t| t.pos),
            )
            .chain(
                self.field
                    .seekers
                    .iter()
                    .filter(|k| k.active())
                    .map(|k| k.pos),
            )
            .chain(
                self.field
                    .lobbers
                    .iter()
                    .filter(|l| l.active())
                    .map(|l| l.pos),
            )
            .chain(
                self.field
                    .pieces
                    .iter()
                    .filter(|p| p.active())
                    .map(|p| p.pos),
            )
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
                for (i, rock) in self
                    .field
                    .rocks
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.element == element && r.size == size)
                {
                    self.draw_rock(art, rock, i);
                }
            }
        }
    }

    /// GravityGun::draw and CustomSpriteSystem::draw: the reaching cone (or
    /// the tether to what it holds) and the sparkle trails, all additive.
    fn draw_gravity(&self, canvas: &Canvas, assets: &PlayAssets) {
        let sparks: Vec<(Vec2, Color)> = self
            .field
            .sparks
            .points()
            .map(|(p, c)| (self.to_screen(p), c))
            .collect();
        for (p, c) in &sparks {
            canvas.draw_points(&assets.spark, &[(*p, gravity::SPARK_SIZE)], self.z, *c);
        }
        let ship = self.to_screen(self.view_ship());
        let gripped = self.field.gripped();
        if !self.field.gun.active && gripped.is_none() {
            return;
        }
        let white = Color::new(1.0, 1.0, 1.0, 0.5);
        let clear = Color::new(1.0, 1.0, 0.0, 0.0);
        let (len, dir) = match gripped {
            Some(p) => {
                let d = self.to_screen(p) - ship;
                (d.length(), d.normalize_or_zero())
            }
            None => (gravity::REACH, self.ship.aim_dir),
        };
        let spread = if gripped.is_some() {
            5.0f32.to_radians().tan() * len
        } else {
            5.0
        };
        let side = vec2(-dir.y, dir.x);
        let tip = ship + dir * len;
        let verts = vec![
            Vertex::new(ship.x, ship.y, self.z, 0.0, 0.0, white),
            Vertex::new(
                tip.x - side.x * spread,
                tip.y - side.y * spread,
                self.z,
                0.0,
                0.0,
                clear,
            ),
            Vertex::new(
                tip.x + side.x * spread,
                tip.y + side.y * spread,
                self.z,
                0.0,
                0.0,
                clear,
            ),
        ];
        canvas.draw_triangles(verts, vec![0, 1, 2], None, true);
    }

    /// Texture::drawMultiple2D / drawMultipleBlend2D for one weapon's shots.
    fn draw_shots(
        &self,
        canvas: &Canvas,
        tex: &Texture2D,
        weapon: Weapon,
        back: f32,
        additive: bool,
    ) {
        let (w, h) = tex_size(tex);
        let mut verts = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        for b in self.ship.bullets.iter().filter(|b| b.weapon == weapon) {
            let a = heading(b.vel).to_radians();
            let fwd = vec2(a.sin(), -a.cos());
            let side = vec2(a.cos(), a.sin());
            // Set back along the flight, but a fresh shot's sprite never
            // reaches behind the muzzle (it showed out of the ship's tail).
            let tail = h * (back + 0.5);
            let c = self.to_screen(self.bullet_pos(b)) - fwd * h * back + fwd * (tail - b.flown()).max(0.0);
            let n = verts.len() as u16;
            for (sx, sy, u, v) in [
                (-0.5, -0.5, 0.0, 0.0),
                (0.5, -0.5, 1.0, 0.0),
                (0.5, 0.5, 1.0, 1.0),
                (-0.5, 0.5, 0.0, 1.0),
            ] {
                let p = c + side * (sx * w) - fwd * (sy * h);
                verts.push(Vertex::new(p.x, p.y, self.z, u, v, WHITE));
            }
            indices.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        }
        if !verts.is_empty() {
            if !additive {
                // Laser bolts glow too.
                canvas.add_glow(verts.clone(), indices.clone(), Some(tex));
            }
            canvas.draw_triangles(verts, indices, Some(tex), additive);
        }
    }

    fn draw_depots(&self, canvas: &Canvas, assets: &PlayAssets) {
        for (i, d) in self.field.depots.iter().enumerate() {
            self.draw_depot(canvas, assets, d, i);
        }
        // Depot::draw: the label, only once the depot has landed.
        let color = Color::new(1.0, 1.0, 1.0, depot::LABEL_ALPHA);
        let label = &assets.depot_label;
        let n = self.field.depots.len();
        for (i, d) in self.field.depots.iter().enumerate().filter(|(_, d)| d.active()) {
            let p = self.at(&self.prev.depots, n, i, d.pos);
            let (label_w, _) = tex_size(label);
            draw_quad(
                label,
                p.x - label_w * 0.5,
                p.y + depot::LABEL_DROP,
                1.0,
                self.z,
                color,
            );
        }
    }

    fn draw_boss(&self, canvas: &Canvas, assets: &PlayAssets) {
        let n = self.field.pieces.len();
        for (i, p) in self.field.pieces.iter().enumerate() {
            let s = self.at(&self.prev.pieces, n, i, p.pos);
            let color = self.tint(s, p.alpha);
            let draw = || draw_sprite(&assets.boss_piece, s.x, s.y, p.angle, self.z + p.z, color);
            if p.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let Some(b) = &self.field.boss else { return };
        let s = self.at(&self.prev.boss, 1, 0, b.pos);
        let color = self.tint(s, b.alpha);
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
            fill_rect(
                at,
                vec2(w, boss::BAR_SIZE.y),
                self.z,
                Color::new(1.0, 0.0, 0.0, 0.5),
            );
        }
    }

    fn draw_depot(&self, canvas: &Canvas, assets: &PlayAssets, d: &Depot, i: usize) {
        let p = self.at(&self.prev.depots, self.field.depots.len(), i, d.pos);
        let color = self.tint(p, d.alpha);
        let draw = || draw_sprite(&assets.depot, p.x, p.y, d.angle, self.z + d.z, color);
        if d.flash {
            canvas.flashing(draw);
        } else {
            draw();
        }
    }

    fn draw_rock(&self, art: &RockArt, rock: &Rock, i: usize) {
        let p = self.at(&self.prev.rocks, self.field.rocks.len(), i, rock.pos);
        let z = self.z + rock.z;
        let color = self.tint(p, rock.alpha);
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
        let cam = self.view_cam();
        let (base, decal) = &assets.backdrops[self.world.min(assets.backdrops.len() - 1)];
        // MB_BG_Z overrides the backdrop depth while it is being verified.
        let bg_z = self.bg_z.unwrap_or(BACKGROUND_Z);
        let origin = vec3(
            SCREEN_CENTRE.x + cam.x,
            SCREEN_CENTRE.y + cam.y,
            self.z + bg_z,
        );
        let scroll = if self.prev.scroll.distance(self.scroll) < BLEND_JUMP {
            self.prev.scroll.lerp(self.scroll, self.blend)
        } else {
            self.scroll
        };
        let lights = if self.fx { self.collect_lights() } else { Vec::new() };
        let normal = assets.normals.get(self.world.min(assets.normals.len().saturating_sub(1)));
        canvas.set_ground_lighting(normal.filter(|_| self.fx), &lights);
        *self.frame_lights.borrow_mut() = lights;
        if self.fx {
            canvas.set_post(&self.waves(), world_grade(self.world));
        }
        canvas.draw_backdrop(false, base, decal, origin, scroll, 1.0);
        if self.fx {
            self.draw_scorches(canvas, assets);
        }
        canvas.set_glowing(true);
        canvas.lighting(self.fx);
        // EnemyManager::sharedDraw: rings first, under everything else.
        let rings = &self.pickups.rings;
        for (i, r) in rings.iter().enumerate().filter(|(_, r)| r.power.is_none()) {
            let p = self.at(&self.prev.rings, rings.len(), i, r.pos);
            // AnimationData(bonus, 4, 4): the 7x7 sprite anchored 4 units in.
            let c = p + vec2(-0.5, -0.5);
            draw_sprite(
                &assets.hud.ring,
                c.x,
                c.y,
                r.angle,
                self.z,
                self.tint(p, r.alpha),
            );
        }
        // Power-ups, then their labels (BigBonus::draw).
        for (i, r) in rings.iter().enumerate() {
            let Some(power) = r.power else { continue };
            let p = self.at(&self.prev.rings, rings.len(), i, r.pos);
            let color = self.tint(p, r.alpha);
            draw_atlas_sprite(
                &assets.power,
                5,
                1,
                power.frame(),
                p.x,
                p.y,
                r.angle,
                self.z,
                color,
            );
        }
        for (i, r) in rings.iter().enumerate() {
            let Some(power) = r.power else { continue };
            let p = self.at(&self.prev.rings, rings.len(), i, r.pos);
            let label = &assets.power_labels[power.frame()];
            let color = Color::new(1.0, 1.0, 1.0, r.alpha * depot::LABEL_ALPHA);
            let (label_w, _) = tex_size(label);
            draw_quad(
                label,
                p.x - label_w * 0.5,
                p.y + POWER_LABEL_DROP,
                1.0,
                self.z,
                color,
            );
        }
        self.draw_rocks(assets);
        self.draw_depots(canvas, assets);
        let n = self.field.mines.len();
        for (i, m) in self.field.mines.iter().enumerate() {
            let s = self.at(&self.prev.mines, n, i, m.pos);
            let color = self.tint(s, m.alpha);
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
                canvas.add_glow(verts.clone(), indices.clone(), None);
                canvas.draw_triangles(verts, indices, None, false);
            }
            let a = t.angle.to_radians();
            let off = vec2(-a.sin(), a.cos()) * tower::SPRITE_OFFSET;
            let c = s + off;
            let color = self.tint(c, t.alpha);
            let draw = || draw_sprite(&assets.tower, c.x, c.y, t.angle, self.z + t.z, color);
            if t.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let n = self.field.lobbers.len();
        for (i, l) in self.field.lobbers.iter().enumerate() {
            let s = self.at(&self.prev.lobbers, n, i, l.pos);
            let color = self.tint(s, l.alpha);
            let draw = || draw_sprite(&assets.lobber, s.x, s.y, l.angle, self.z + l.z, color);
            if l.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
            if let Some(b) = &l.ball {
                let color = self.tint(s, b.alpha);
                let draw =
                    || draw_sprite(&assets.boss_piece, s.x, s.y, b.angle, self.z + b.z, color);
                if b.flash {
                    canvas.flashing(draw);
                } else {
                    draw();
                }
            }
        }
        let n = self.field.seekers.len();
        for (i, k) in self.field.seekers.iter().enumerate() {
            let s = self.at(&self.prev.seekers, n, i, k.pos);
            let color = self.tint(s, k.alpha);
            let draw = || draw_sprite(&assets.seeker, s.x, s.y, k.angle, self.z + k.z, color);
            if k.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let n = self.field.arrows.len();
        for (i, a) in self.field.arrows.iter().enumerate() {
            let s = self.at(&self.prev.arrows, n, i, a.pos);
            let color = self.tint(s, a.alpha);
            let draw = || draw_sprite(&assets.arrow, s.x, s.y, a.angle, self.z + a.z, color);
            if a.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let n = self.field.snakes.len();
        let segs: usize = self.field.snakes.iter().map(|s| s.segments.len()).sum();
        let mut first = 0;
        for (i, sn) in self.field.snakes.iter().enumerate() {
            for (j, seg) in sn.segments.iter().enumerate().rev() {
                let s = self.at(&self.prev.segments, segs, first + j, seg.pos);
                let color = self.tint(s, seg.alpha);
                let draw = || {
                    draw_sprite(
                        &assets.snake_body,
                        s.x,
                        s.y,
                        seg.angle,
                        self.z + seg.z,
                        color,
                    )
                };
                if seg.flash {
                    canvas.flashing(draw);
                } else {
                    draw();
                }
            }
            first += sn.segments.len();
            let s = self.at(&self.prev.snakes, n, i, sn.pos);
            let color = self.tint(s, sn.alpha);
            let draw = || draw_sprite(&assets.snake_head, s.x, s.y, sn.angle, self.z + sn.z, color);
            if sn.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        let n = self.field.shy_guys.len();
        for (i, g) in self.field.shy_guys.iter().enumerate() {
            let s = self.at(&self.prev.shy_guys, n, i, g.pos);
            let color = self.tint(s, g.alpha);
            let draw = || {
                draw_atlas_sprite(
                    &assets.shy_guy,
                    4,
                    4,
                    g.frame,
                    s.x,
                    s.y,
                    g.angle,
                    self.z + g.z,
                    color,
                )
            };
            if g.flash {
                canvas.flashing(draw);
            } else {
                draw();
            }
        }
        self.draw_boss(canvas, assets);
        canvas.lighting(false);

        // Cannon fire: green additive point sprites trailing each bullet.
        let mut points = Vec::new();
        for b in self
            .ship
            .bullets
            .iter()
            .filter(|b| b.weapon == Weapon::Cannon)
        {
            let p = self.to_screen(self.bullet_pos(b));
            let flown = b.flown();
            for (behind, size) in CANNON_TRAIL {
                // Only the part of the trail the shot has already flown.
                if behind <= flown {
                    points.push((p + b.back * behind, size));
                }
            }
        }
        canvas.draw_points(&assets.cannon, &points, self.z, GREEN);
        // Ship::draw: laser bolts, then flames (additive), each drawn along
        // its flight and set back a fraction of its length.
        self.draw_shots(
            canvas,
            &assets.laser,
            Weapon::Laser,
            weapons::LASER_DRAW_BACK,
            false,
        );
        self.draw_shots(
            canvas,
            &assets.flame,
            Weapon::Fire,
            weapons::FIRE_DRAW_BACK,
            true,
        );

        self.draw_gravity(canvas, assets);
        if self.fx {
            self.draw_ship_fx(canvas, assets);
        }
        self.draw_ship(assets);
        if self.fx {
            self.draw_shield_rim(canvas, assets);
        }
        let soft = self.fx.then_some(&assets.soft_dot);
        self.effects
            .draw(canvas, &assets.spark, soft, self.z, |p| self.to_screen(p));
        self.draw_hud(canvas, assets);
    }

    /// Port addition: the engine trail, boost streaks, and streaks behind
    /// enemies the gravity gun threw, all additive.
    fn draw_ship_fx(&self, canvas: &Canvas, assets: &PlayAssets) {
        let mut verts = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut quad = |verts: &mut Vec<Vertex>, c: Vec2, h: f32, color: Color| {
            let n = verts.len() as u16;
            for (x, y, u, v) in [(-1.0, -1.0, 0.0, 0.0), (1.0, -1.0, 1.0, 0.0), (1.0, 1.0, 1.0, 1.0), (-1.0, 1.0, 0.0, 1.0)] {
                verts.push(Vertex::new(c.x + x * h, c.y + y * h, self.z, u, v, color));
            }
            indices.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        };
        // The trail: older samples smaller and fainter, and only while
        // moving (a still ship would pile it into a blob).
        let n = self.trail.len().max(1) as f32;
        for (i, &(pos, speed)) in self.trail.iter().enumerate().skip(1) {
            let k = 1.0 - i as f32 / n;
            let a = 0.45 * k * (speed / TRAIL_FULL_SPEED).min(1.0) * self.ship.alpha;
            if a > 0.01 {
                quad(&mut verts, self.to_screen(pos), 3.0 + 6.0 * k, Color::new(0.35, 1.0, 0.5, a));
            }
        }
        canvas.draw_triangles(verts, indices, Some(&assets.spark), true);

        // Streaks: boost lines behind the ship, and thrown enemies.
        let mut verts = Vec::new();
        let mut indices: Vec<u16> = Vec::new();
        let mut line = |a: Vec2, b: Vec2, width: f32, head: Color| {
            let side = (b - a).perp().normalize_or_zero() * width * 0.5;
            let tail = Color::new(head.r, head.g, head.b, 0.0);
            let n = verts.len() as u16;
            verts.push(Vertex::new(a.x + side.x, a.y + side.y, self.z, 0.0, 0.0, head));
            verts.push(Vertex::new(a.x - side.x, a.y - side.y, self.z, 0.0, 0.0, head));
            verts.push(Vertex::new(b.x - side.x, b.y - side.y, self.z, 0.0, 0.0, tail));
            verts.push(Vertex::new(b.x + side.x, b.y + side.y, self.z, 0.0, 0.0, tail));
            indices.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        };
        if self.ship.invincible > 0.0 && !matches!(self.ship.life, Life::Dead(_)) {
            let k = (self.ship.invincible / BOOST_INVINCIBLE).clamp(0.0, 1.0);
            let back = -(self.ship.vel + self.ship.boost_vel).normalize_or_zero();
            if back != Vec2::ZERO {
                let p = self.to_screen(self.view_ship());
                let side = back.perp();
                // Uneven speed lines, longest at the centre.
                for (off, len) in [(-9.0, 34.0), (-4.5, 52.0), (0.0, 70.0), (5.0, 46.0), (10.0, 30.0)] {
                    let start = p + side * off + back * 6.0;
                    line(start, start + back * len * k, 1.5, Color::new(0.6, 1.0, 0.7, 0.35 * k));
                }
            }
        }
        let thrown = |g: &Option<gravity::Grav>| g.as_ref().filter(|g| g.thrown()).map(|g| g.vel);
        let flung = self
            .field
            .rocks
            .iter()
            .filter_map(|r| thrown(&r.grav).map(|v| (r.pos, v)))
            .chain(self.field.mines.iter().filter_map(|e| thrown(&e.grav).map(|v| (e.pos, v))))
            .chain(self.field.shy_guys.iter().filter_map(|e| thrown(&e.grav).map(|v| (e.pos, v))))
            .chain(self.field.arrows.iter().filter_map(|e| thrown(&e.grav).map(|v| (e.pos, v))))
            .chain(self.field.seekers.iter().filter_map(|e| thrown(&e.grav).map(|v| (e.pos, v))))
            .chain(self.field.pieces.iter().filter_map(|e| thrown(&e.grav).map(|v| (e.pos, v))));
        for (pos, vel) in flung {
            let p = self.to_screen(pos);
            line(p, p - vel * 0.08, 8.0, Color::new(0.6, 0.85, 1.0, 0.45));
        }
        // Fast shots leave a short streak along their flight (cannon green,
        // laser blue); flames already read as a stream.
        for b in &self.ship.bullets {
            let (color, width) = match b.weapon {
                Weapon::Cannon => (Color::new(0.45, 1.0, 0.5, 0.45), 2.0),
                Weapon::Laser => (Color::new(0.5, 0.8, 1.0, 0.5), 2.5),
                Weapon::Fire => continue,
            };
            let p = self.to_screen(self.bullet_pos(b));
            let length = (b.vel.length() * BULLET_STREAK).min(b.flown());
            line(p, p - b.vel.normalize_or_zero() * length, width, color);
        }
        canvas.draw_triangles(verts, indices, None, true);
    }

    /// Port addition: a soft pulse around the shield while it is up.
    fn draw_shield_rim(&self, canvas: &Canvas, assets: &PlayAssets) {
        let ship = &self.ship;
        let up = match ship.life {
            Life::Dead(_) => 0.0,
            Life::Spawning(_) => 1.0,
            Life::Fading(t) => t.max(ship.shield as u8 as f32),
            Life::Alive => ship.shield as u8 as f32,
        };
        if up <= 0.0 {
            return;
        }
        let p = self.to_screen(self.view_ship());
        let pulse = 0.5 + 0.5 * (self.elapsed * 6.0).sin();
        let (w, _) = tex_size(&assets.shield);
        let h = w * (0.55 + 0.05 * pulse);
        let color = Color::new(0.4, 0.9, 1.0, (0.15 + 0.2 * pulse) * up * ship.alpha);
        let n = 0u16;
        let verts: Vec<Vertex> = [(-1.0, -1.0, 0.0, 0.0), (1.0, -1.0, 1.0, 0.0), (1.0, 1.0, 1.0, 1.0), (-1.0, 1.0, 0.0, 1.0)]
            .iter()
            .map(|&(x, y, u, v)| Vertex::new(p.x + x * h, p.y + y * h, self.z, u, v, color))
            .collect();
        canvas.draw_triangles(verts, vec![n, n + 1, n + 2, n, n + 2, n + 3], Some(&assets.shield), true);
    }

    /// Burn marks on the play plane, in one batch.
    fn draw_scorches(&self, canvas: &Canvas, assets: &PlayAssets) {
        let mut verts = Vec::with_capacity(self.scorches.len() * 4);
        let mut indices: Vec<u16> = Vec::with_capacity(self.scorches.len() * 6);
        for s in &self.scorches {
            let fade = ((SCORCH_LIFE - s.age) / SCORCH_FADE).min(1.0);
            let c = self.to_screen(s.pos);
            let color = Color::new(1.0, 1.0, 1.0, 0.8 * fade);
            let (sn, cs) = s.angle.sin_cos();
            let h = s.size * 0.5;
            let n = verts.len() as u16;
            for (x, y, u, v) in [(-1.0, -1.0, 0.0, 0.0), (1.0, -1.0, 1.0, 0.0), (1.0, 1.0, 1.0, 1.0), (-1.0, 1.0, 0.0, 1.0)] {
                let p = c + vec2(x * cs - y * sn, x * sn + y * cs) * h;
                verts.push(Vertex::new(p.x, p.y, self.z, u, v, color));
            }
            indices.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        }
        canvas.draw_triangles(verts, indices, Some(&assets.scorch), false);
    }

    fn draw_ship(&self, assets: &PlayAssets) {
        let ship = &self.ship;
        if matches!(ship.life, Life::Dead(_)) {
            return;
        }
        let p = self.to_screen(self.view_ship());
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
    fn draw_hud(&self, canvas: &Canvas, assets: &PlayAssets) {
        canvas.set_glowing(false);
        hud::draw(&assets.hud, &self.hud_values());
        if let Some(t) = &self.tutorial {
            t.draw(canvas, &assets.hud.font);
        }
    }

    fn hud_values(&self) -> HudValues {
        HudValues {
            score: self.score,
            high_score: self.high_score,
            multiplier: self.multiplier,
            lives: self.lives,
            bombs: self.bombs,
            rings: self.rings,
            show: match &self.tutorial {
                None => Show::ALL,
                Some(t) => Show {
                    lives_and_bombs: t.can_show_lives_and_bombs(),
                    score: t.can_show_score(),
                    rings: t.can_show_rings(),
                    pause: t.can_pause(),
                    multiplier: t.can_show_multiplier(),
                    high_score: t.can_show_high_score(),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_between_steps_blend_across_the_wrap() {
        let mid = blend_pos(vec2(WORLD - 1.0, 0.0), vec2(1.0, 0.0), 0.5);
        assert!(wrap_near(mid.x, 0.0).abs() < 1e-3, "{:?}", mid);
        assert_eq!(blend_pos(Vec2::ZERO, vec2(10.0, 0.0), 0.25), vec2(2.5, 0.0));
        assert_eq!(
            blend_pos(Vec2::ZERO, vec2(300.0, 0.0), 0.5),
            vec2(300.0, 0.0),
            "a jump is drawn where it landed"
        );
    }
}
