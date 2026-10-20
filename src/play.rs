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
use crate::hud::{self, HudAssets, HudValues};

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
const CANNON_SPREAD: [&[f32]; 8] = [&[0.0], &[0.0], &[-3.0, 3.0], &[-3.0, 3.0], &[-6.0, 0.0, 6.0], &[-6.0, 0.0, 6.0], &[-3.0, 3.0, -9.0, 9.0], &[-3.0, 3.0, -9.0, 9.0]];
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
        [16, self.medium.len(), self.large.len(), self.extra_large.len()]
    }
}

pub struct PlayAssets {
    pub hud: HudAssets,
    /// Normal, ice and fire.
    pub rocks: [RockArt; 3],
    pub ship: Vec<Texture2D>,
    pub ship_ready: Vec<Texture2D>,
    pub shield: Texture2D,
    pub aim: Texture2D,
    pub cannon: Texture2D,
    /// Base and decal planet textures per world (0-5), then survival.
    pub backdrops: Vec<(Texture2D, Texture2D)>,
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
            self.bullets.push(Bullet { prev: origin, pos: origin, vel, life: CANNON_LIFE, back: -vel.normalize() });
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
        self.boost_charge = (self.boost_charge + dt).min(BOOST_RECHARGE);

        let total = self.vel + self.boost_vel;
        if total != Vec2::ZERO {
            self.angle = heading(total);
        }
        self.pos = vec2(wrap_world(self.pos.x + total.x * dt), wrap_world(self.pos.y + total.y * dt));
        self.boost_vel *= DRAG / (dt + DRAG);

        for b in &mut self.bullets {
            b.prev = b.pos;
            b.life -= dt;
            b.pos = vec2(wrap_near(b.pos.x + b.vel.x * dt, self.pos.x), wrap_near(b.pos.y + b.vel.y * dt, self.pos.y));
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
            self.turn_average = (self.turn_average * TURN_HISTORY as f32 - oldest) / (TURN_HISTORY - 1) as f32;
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
        self.centre = self.centre.clamp(Vec2::splat(-CAMERA_LIMIT), Vec2::splat(CAMERA_LIMIT));
        if self.shake_time != 0.0 && self.shake_power != 0.0 {
            self.shake_time -= dt;
            if self.shake_time < 0.0 {
                self.shake_time = 0.0;
                self.shake = Vec2::ZERO;
            } else {
                self.shake = vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * self.shake_power;
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
    Over { cleared: bool },
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
    high_score: u64,
    /// `Audio::play` ids raised this update, for the caller to play.
    pub sounds: Vec<usize>,
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
            camera: Lead { centre: Vec2::ZERO, shake: Vec2::ZERO, shake_power: 0.0, shake_time: 0.0 },
            scroll: Vec2::ZERO,
            accumulator: 0.0,
            field: Field::new(assets.rocks[0].frame_counts()),
            script: Script::arcade(world, stage),
            score: 0,
            multiplier: 1,
            lives: if world == SURVIVAL { SURVIVAL_LIVES } else { LIVES },
            bombs: BOMBS,
            rings: 0,
            high_score,
            sounds: Vec::new(),
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
        let d = vec2(wrap_near(p.x, self.ship.pos.x) - self.ship.pos.x, wrap_near(p.y, self.ship.pos.y) - self.ship.pos.y);
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
                self.z = (START_Z - START_Z * (self.alpha * std::f32::consts::FRAC_PI_2).sin()).max(0.0);
                self.ship.update(dt);
                if !self.shaken && self.elapsed > 0.75 {
                    self.shaken = true;
                    self.camera.shake(0.75, 1.0);
                }
                self.camera.update(dt, Vec2::ZERO);
            }
            State::Ready => self.step_ready(dt),
            State::Over { .. } => {
                self.field.update(dt, self.ship.pos);
                self.camera.update(dt, Vec2::ZERO);
            }
        }
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
        self.field.update(dt, self.ship.pos);
        self.collide();
        self.handle_events();
        if self.script.complete(&self.field) {
            self.state = State::Over { cleared: true };
        }
    }

    /// The cannon against rocks, then rocks against the ship (shipCollision).
    fn collide(&mut self) {
        let field = &mut self.field;
        self.ship.bullets.retain(|b| !field.bullet(b.prev, b.pos, CANNON_RADIUS, Weapon::Cannon, CANNON_DAMAGE, CANNON_MASS, b.vel));
        if matches!(self.ship.life, Life::Dead(_)) {
            return;
        }
        let mut touching = self.field.touching(self.ship.pos, SHIP_RADIUS);
        if touching.is_empty() {
            return;
        }
        touching.sort_unstable_by(|a, b| b.cmp(a));
        for i in touching {
            self.field.destroy(i, true);
        }
        if self.ship.life == Life::Alive && self.ship.invincible <= 0.0 {
            self.kill_ship();
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
    }

    fn handle_events(&mut self) {
        for event in self.field.events.drain(..) {
            match event {
                Event::RockDestroyed { points, .. } => {
                    self.score += (points * self.multiplier) as u64;
                    self.sounds.push(SOUND_ROCK[rand::gen_range(0, 3)]);
                }
                Event::WaveIncoming => self.sounds.push(SOUND_WAVE),
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
        if !self.ship.boost_cued && charge >= BOOST_RECHARGE - BOOST_CUE_EARLY && charge < BOOST_RECHARGE - BOOST_CUE_LATE {
            self.ship.boost_cued = true;
            self.sounds.push(SOUND_BOOST_READY);
        }
        if is_mouse_button_down(MouseButton::Left) {
            let dir = (Canvas::mouse() - self.to_screen(self.ship.pos)).normalize_or_zero();
            if dir != Vec2::ZERO {
                self.ship.fire(dt, dir);
            }
        } else {
            self.ship.hold_fire();
        }
    }

    fn draw_rocks(&self, assets: &PlayAssets) {
        // EnemyManager::sharedDraw: per element, small to extra large.
        for (e, element) in [Element::Normal, Element::Ice, Element::Fire].into_iter().enumerate() {
            let art = &assets.rocks[e];
            for size in 0..4 {
                for rock in self.field.rocks.iter().filter(|r| r.element == element && r.size == size) {
                    self.draw_rock(art, rock);
                }
            }
        }
    }

    fn draw_rock(&self, art: &RockArt, rock: &Rock) {
        let p = self.to_screen(rock.pos);
        let z = self.z + rock.z;
        let color = Color::new(1.0, 1.0, 1.0, rock.alpha);
        match rock.size {
            0 => draw_atlas_sprite(&art.small, 4, 4, rock.frame, p.x, p.y, rock.angle, z, color),
            1 => draw_sprite(&art.medium[rock.frame % art.medium.len()], p.x, p.y, rock.angle, z, color),
            2 => draw_sprite(&art.large[rock.frame % art.large.len()], p.x, p.y, rock.angle, z, color),
            _ => draw_sprite(&art.extra_large[rock.frame % art.extra_large.len()], p.x, p.y, rock.angle, z, color),
        }
    }

    pub fn draw(&self, canvas: &Canvas, assets: &PlayAssets) {
        let cam = self.camera.offset();
        let (base, decal) = &assets.backdrops[self.world.min(assets.backdrops.len() - 1)];
        // MB_BG_Z overrides the backdrop depth while it is being verified.
        let bg_z = std::env::var("MB_BG_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(BACKGROUND_Z);
        let origin = vec3(SCREEN_CENTRE.x + cam.x, SCREEN_CENTRE.y + cam.y, self.z + bg_z);
        canvas.draw_backdrop(false, base, decal, origin, self.scroll, 1.0);
        self.draw_rocks(assets);

        // Cannon fire: green additive point sprites trailing each bullet.
        let mut points = Vec::new();
        for b in &self.ship.bullets {
            let p = self.to_screen(b.pos);
            for (behind, size) in CANNON_TRAIL {
                points.push((p + b.back * behind, size));
            }
        }
        canvas.draw_points(&assets.cannon, &points, self.z, GREEN);

        let ship = &self.ship;
        if matches!(ship.life, Life::Dead(_)) {
            self.draw_hud(assets);
            return;
        }
        let p = self.to_screen(ship.pos);
        let frames = if ship.can_boost() { &assets.ship_ready } else { &assets.ship };
        let color = Color::new(1.0, 1.0, 1.0, ship.alpha);
        draw_sprite(&frames[ship.frame], p.x, p.y, ship.angle, self.z, color);
        let shield = match ship.life {
            Life::Spawning(_) => ship.alpha,
            Life::Fading(t) => t * ship.alpha,
            _ if ship.invincible > 0.0 => ship.alpha,
            _ => 0.0,
        };
        if shield > 0.0 {
            draw_sprite(&assets.shield, p.x, p.y, self.elapsed * SHIELD_SPIN, self.z, Color::new(1.0, 1.0, 1.0, shield));
        }
        self.draw_hud(assets);
        if ship.aiming {
            let aim = p + ship.aim_dir * AIM_DISTANCE;
            draw_quad(&assets.aim, aim.x - 1.0, aim.y - 1.0, 1.0, self.z, color);
        }
    }

    /// GameScreen::draw: the HUD sits at depth 0 whatever the play layer does.
    fn draw_hud(&self, assets: &PlayAssets) {
        let values = HudValues {
            score: self.score,
            high_score: self.high_score,
            multiplier: self.multiplier,
            lives: self.lives,
            bombs: self.bombs,
            rings: self.rings,
        };
        hud::draw(&assets.hud, &values);
    }
}
