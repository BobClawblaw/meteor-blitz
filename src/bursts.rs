//! Physics particles (port addition, Options > Particle style): explosions
//! and impacts as layered particle physics instead of the original's
//! sprite systems. Every explosion is four kinds of particle, each with its
//! own drag and its own share of the momentum of the shot that did it:
//! - fire: a cluster of expanding puffs, white-hot, cooling through
//!   yellow and orange to red as they slow (additive, and in the bloom);
//! - sparks: fast streaks drawn along their velocity, cooling the same way
//!   (additive, in the bloom);
//! - debris: tumbling chips in the colour of what broke (rock, ice, metal);
//! - smoke: puffs that swell, linger and curl through a two-octave curl
//!   field, after the motion of BlockYard's pulsar wind
//!   (public/js/details3d.js), in the colour of what burned.
//! Shots carry through: an impact's sparks fly on in the shot's direction
//! at up to most of its speed, ricochets come back, and a kill's whole
//! explosion drifts with the shot. What it looks like (`Look`) is the
//! caller's choice, from what was hit and what hit it.

use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams,
};
use macroquad::prelude::*;

use crate::gfx::Canvas;

/// How an explosion or impact looks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look {
    /// Smoke colour and thickness (0 for none).
    pub smoke: Vec3,
    pub opacity: f32,
    /// Debris colour, if it breaks into chips.
    pub debris: Option<Vec3>,
    /// Tint the fire and sparks lean toward (a weapon's or an element's
    /// colour), and how far, 0..1 (0 = plain fire).
    pub tint: Vec3,
    pub tint_amount: f32,
    /// How much fire and how many sparks, 0..1.
    pub heat: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fire,
    Spark,
    Debris,
    Smoke,
}

struct Particle {
    kind: Kind,
    pos: Vec2,
    vel: Vec2,
    /// Seconds; negative while it waits to appear.
    age: f32,
    life: f32,
    /// Radius (fire, smoke: from `size.0` swelling to `size.1`), or
    /// half-width (sparks, debris).
    size: (f32, f32),
    angle: f32,
    spin: f32,
    colour: Vec3,
    alpha: f32,
    tint: Vec3,
    tint_amount: f32,
}

/// Momentum lost per second, by kind (velocity x e^-drag t): fire and
/// smoke are light and stop quickly, sparks and debris fly on.
fn drag(kind: Kind) -> f32 {
    match kind {
        Kind::Fire => 4.0,
        Kind::Spark => 2.0,
        Kind::Debris => 1.3,
        Kind::Smoke => 2.6,
    }
}

/// Shares of a shot's velocity each kind carries in an explosion.
const CARRY_FIRE: f32 = 0.35;
const CARRY_SPARK: f32 = 0.5;
const CARRY_DEBRIS: f32 = 0.45;
const CARRY_SMOKE: f32 = 0.25;
/// No inherited speed above this.
const MAX_CARRY: f32 = 520.0;
/// The curl field the smoke drifts through: two octaves (world units),
/// and its speed. The pulsar's rule: well under the drift, or the eddies
/// become the wind.
const EDDY_SMALL: f32 = 13.0;
const EDDY_LARGE: f32 = 34.0;
const EDDY_SPEED: f32 = 30.0;
/// A spark is drawn as the path it covers in this many seconds.
const STREAK_TIME: f32 = 0.022;
const MAX_PARTICLES: usize = 16_000;
/// Quads per draw call: 3,200 vertices and 4,800 indices, inside
/// macroquad's batch (10,000 and 5,000).
const CHUNK: usize = 800;

pub struct Bursts {
    parts: Vec<Particle>,
    time: f32,
    seed: u32,
}

thread_local! {
    /// The additive material, built on first use and shared by every game.
    static ADDITIVE: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}

fn additive() -> Material {
    ADDITIVE.with(|m| {
        m.get_or_init(|| {
            let blend = BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::One,
            );
            load_material(
                ShaderSource::Glsl {
                    vertex: VERTEX,
                    fragment: FRAGMENT,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        color_blend: Some(blend),
                        alpha_blend: Some(blend),
                        cull_face: CullFace::Nothing,
                        depth_test: Comparison::Always,
                        depth_write: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .expect("burst shader")
        })
        .clone()
    })
}

/// Fire's colour by temperature (1 just lit, 0 spent): white, yellow,
/// orange, a dull red.
fn blackbody(t: f32) -> Vec3 {
    const STOPS: [(f32, Vec3); 4] = [
        (0.0, Vec3::new(0.55, 0.08, 0.02)),
        (0.4, Vec3::new(1.0, 0.42, 0.08)),
        (0.8, Vec3::new(1.0, 0.75, 0.32)),
        (1.0, Vec3::new(1.0, 0.93, 0.8)),
    ];
    let t = t.clamp(0.0, 1.0);
    let i = STOPS.iter().rposition(|s| s.0 <= t).unwrap_or(0).min(2);
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    a.1.lerp(b.1, (t - a.0) / (b.0 - a.0))
}

/// `v` scaled down to at most `max` long.
fn capped(v: Vec2, max: f32) -> Vec2 {
    v.clamp_length_max(max)
}

impl Bursts {
    pub fn new() -> Bursts {
        Bursts {
            parts: Vec::new(),
            time: 0.0,
            seed: 0x2545_f491,
        }
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    /// A random direction.
    fn dir(&mut self) -> Vec2 {
        Vec2::from_angle(self.rand() * std::f32::consts::TAU)
    }

    fn push(&mut self, p: Particle) {
        if self.parts.len() < MAX_PARTICLES {
            self.parts.push(p);
        }
    }

    fn particle(kind: Kind, pos: Vec2, vel: Vec2, life: f32, size: (f32, f32), look: &Look) -> Particle {
        Particle {
            kind,
            pos,
            vel,
            age: 0.0,
            life,
            size,
            angle: 0.0,
            spin: 0.0,
            colour: Vec3::ONE,
            alpha: 1.0,
            tint: look.tint,
            tint_amount: look.tint_amount,
        }
    }

    /// An explosion about `radius` world units across, lasting about
    /// `life` seconds; `shot` is the velocity of the shot that did it
    /// (zero for bombs and rams), which every part of it carries a share of.
    pub fn blast(&mut self, pos: Vec2, shot: Vec2, radius: f32, life: f32, look: Look) {
        let r = radius;
        let heat = look.heat;
        // Fire: a cluster of puffs thrown out a little and swelling.
        for _ in 0..(3.0 + (4.0 + r / 5.0) * heat) as usize {
            let d = self.dir();
            let vel = d * self.range(0.6, 2.2) * r + capped(shot * CARRY_FIRE, MAX_CARRY);
            let start = pos + d * self.range(0.0, 0.25) * r;
            let size = (self.range(0.15, 0.3) * r, self.range(0.45, 0.75) * r);
            let life = self.range(0.35, 0.6) * (0.8 + r / 150.0);
            let mut p = Self::particle(Kind::Fire, start, vel, life, size, &look);
            p.alpha = self.range(0.6, 1.0);
            self.push(p);
        }
        // Sparks: fast, flying on with the shot.
        for _ in 0..(10.0 + (12.0 + r * 0.9) * heat) as usize {
            let d = self.dir();
            let vel = d * self.range(3.0, 9.0) * (25.0 + r) + capped(shot * CARRY_SPARK, MAX_CARRY);
            let life = self.range(0.25, 0.7);
            let mut p = Self::particle(Kind::Spark, pos, vel, life, (self.range(0.6, 1.3), 0.0), &look);
            p.alpha = self.range(0.6, 1.0);
            self.push(p);
        }
        // Debris: tumbling chips.
        if let Some(colour) = look.debris {
            for _ in 0..(4.0 + r / 4.0) as usize {
                let d = self.dir();
                let vel = d * self.range(1.0, 4.5) * (20.0 + r) + capped(shot * CARRY_DEBRIS, MAX_CARRY);
                let half = self.range(0.7, 1.4) * (1.0 + r / 60.0);
                let life = self.range(0.7, 1.4);
                let mut p = Self::particle(Kind::Debris, pos + d * r * 0.2, vel, life, (half, half * self.range(0.5, 0.9)), &look);
                p.angle = self.rand() * std::f32::consts::TAU;
                p.spin = self.range(-14.0, 14.0);
                p.colour = colour * self.range(0.5, 0.85);
                self.push(p);
            }
        }
        // Smoke: rising out of the fire as it dies, swelling and lingering.
        if look.opacity > 0.0 {
            for _ in 0..(8.0 + r * 0.5) as usize {
                let d = self.dir();
                let vel = d * self.range(0.3, 1.6) * r + capped(shot * CARRY_SMOKE, MAX_CARRY);
                let start = pos + d * self.range(0.0, 0.35) * r;
                let size = (self.range(0.15, 0.25) * r, self.range(0.5, 0.9) * r);
                let mut p = Self::particle(Kind::Smoke, start, vel, life * self.range(1.2, 2.0), size, &look);
                p.age = -self.range(0.04, 0.16);
                p.colour = look.smoke * self.range(0.8, 1.1);
                p.alpha = look.opacity * self.range(0.5, 1.0);
                self.push(p);
            }
        }
    }

    /// A shot striking something at `pos` going `shot`: sparks flying on
    /// along it, a few ricocheting back, a flash, chips and a puff of smoke.
    pub fn impact(&mut self, pos: Vec2, shot: Vec2, look: Look) {
        let speed = shot.length();
        let along = if speed > 1e-3 { shot / speed } else { vec2(0.0, -1.0) };
        let heat = look.heat;
        for _ in 0..(4.0 + 8.0 * heat) as usize {
            let d = Vec2::from_angle(self.range(-0.55, 0.55)).rotate(along);
            let vel = d * self.range(0.25, 0.7) * speed.min(900.0);
            let life = self.range(0.12, 0.35);
            let mut p = Self::particle(Kind::Spark, pos, vel, life, (self.range(0.5, 1.0), 0.0), &look);
            p.alpha = self.range(0.6, 1.0);
            self.push(p);
        }
        for _ in 0..(1.0 + 3.0 * heat) as usize {
            let d = Vec2::from_angle(std::f32::consts::PI + self.range(-0.9, 0.9)).rotate(along);
            let vel = d * self.range(120.0, 280.0);
            let life = self.range(0.1, 0.25);
            let p = Self::particle(Kind::Spark, pos, vel, life, (0.6, 0.0), &look);
            self.push(p);
        }
        let mut flash = Self::particle(Kind::Fire, pos, shot * 0.05, 0.12, (2.5, 7.0 * (0.5 + heat)), &look);
        flash.alpha = 0.6 + 0.4 * heat;
        self.push(flash);
        if let Some(colour) = look.debris {
            for _ in 0..3 {
                let d = Vec2::from_angle(std::f32::consts::PI + self.range(-1.2, 1.2)).rotate(along);
                let vel = d * self.range(60.0, 180.0) + shot * 0.1;
                let mut p = Self::particle(Kind::Debris, pos, vel, self.range(0.3, 0.6), (0.7, 0.5), &look);
                p.angle = self.rand() * std::f32::consts::TAU;
                p.spin = self.range(-16.0, 16.0);
                p.colour = colour;
                self.push(p);
            }
        }
        if look.opacity > 0.0 {
            for _ in 0..4 {
                let vel = shot * self.range(0.05, 0.2) + self.dir() * self.range(10.0, 40.0);
                let mut p = Self::particle(Kind::Smoke, pos, vel, self.range(0.4, 0.8), (2.0, self.range(6.0, 10.0)), &look);
                p.colour = look.smoke;
                p.alpha = look.opacity * 0.8;
                self.push(p);
            }
        }
    }

    /// A shot flying from `from` to `to` at `vel`: a little fire and smoke
    /// shed along the step (the flamethrower's stream).
    pub fn trail(&mut self, from: Vec2, to: Vec2, vel: Vec2, look: Look) {
        let along = self.rand();
        let pos = from.lerp(to, along);
        let mut fire = Self::particle(Kind::Fire, pos, vel * 0.15, self.range(0.1, 0.2), (2.0, 5.0), &look);
        fire.alpha = 0.35;
        self.push(fire);
        if look.opacity > 0.0 {
            let mut p = Self::particle(Kind::Smoke, pos, vel * 0.08 + self.dir() * 15.0, self.range(0.4, 0.7), (2.0, 7.0), &look);
            p.age = -0.08;
            p.colour = look.smoke;
            p.alpha = look.opacity * 0.5;
            self.push(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        let (a, b) = (self.time * 0.37, -self.time * 0.23);
        // The curl of a stream function sin x sin y, its phase drifting so
        // the eddies churn.
        let curl = |q: Vec2, l: f32, pa: f32, pb: f32| {
            let (sx, cx) = (q.x / l + pa).sin_cos();
            let (sy, cy) = (q.y / l + pb).sin_cos();
            vec2(sx * cy, -cx * sy)
        };
        let keep = [Kind::Fire, Kind::Spark, Kind::Debris, Kind::Smoke].map(|k| (-drag(k) * dt).exp());
        for p in &mut self.parts {
            p.age += dt;
            if p.age < 0.0 {
                continue;
            }
            let mut v = p.vel;
            match p.kind {
                Kind::Smoke => v += (curl(p.pos, EDDY_SMALL, a, b) + 0.7 * curl(p.pos, EDDY_LARGE, b, a)) * EDDY_SPEED,
                Kind::Fire => v += curl(p.pos, EDDY_SMALL, a, b) * EDDY_SPEED * 0.4,
                _ => {}
            }
            p.pos += v * dt;
            p.vel *= keep[p.kind as usize];
            p.angle += p.spin * dt;
            p.spin *= keep[Kind::Debris as usize];
        }
        self.parts.retain(|p| p.age < p.life);
    }

    pub fn clear(&mut self) {
        self.parts.clear();
    }

    /// Draws everything at depth `z`, `to_screen` mapping world positions
    /// to the canvas and `dot` the soft round sprite: smoke and debris
    /// blended, then fire and sparks added over them and handed to the
    /// bloom.
    pub fn draw(&self, canvas: &Canvas, z: f32, to_screen: impl Fn(Vec2) -> Vec2, dot: &Texture2D) {
        if self.parts.is_empty() {
            return;
        }
        let mut batch = Batch::new(z);
        gl_use_default_material();
        for p in self.parts.iter().filter(|p| p.kind == Kind::Smoke && p.age >= 0.0) {
            let u = p.age / p.life;
            let grow = 1.0 - (1.0 - u) * (1.0 - u);
            let radius = p.size.0 + (p.size.1 - p.size.0) * grow;
            let alpha = p.alpha * (u / 0.15).min(1.0) * (1.0 - u).powf(1.5) * 0.8;
            batch.puff(to_screen(p.pos), radius, colour(p.colour, alpha));
            batch.flush_if_full(Some(dot), None);
        }
        batch.flush(Some(dot), None);
        for p in self.parts.iter().filter(|p| p.kind == Kind::Debris) {
            let u = p.age / p.life;
            let alpha = (1.0 - u * u * u).clamp(0.0, 1.0);
            batch.chip(to_screen(p.pos), p.size, p.angle, colour(p.colour, alpha));
            batch.flush_if_full(None, None);
        }
        batch.flush(None, None);
        gl_use_material(&additive());
        for p in self.parts.iter().filter(|p| matches!(p.kind, Kind::Fire | Kind::Spark)) {
            let u = p.age / p.life;
            // White only at the start: fire cools quickly to orange, then red.
            let heat = blackbody((1.0 - u) * (1.0 - u)).lerp(p.tint, p.tint_amount * (1.0 - u * 0.5));
            if p.kind == Kind::Fire {
                let grow = 1.0 - (1.0 - u).powi(3);
                let radius = p.size.0 + (p.size.1 - p.size.0) * grow;
                let alpha = p.alpha * 0.75 * (1.0 - u).powf(1.5);
                batch.puff(to_screen(p.pos), radius, colour(heat, alpha));
            } else {
                let head = to_screen(p.pos);
                let tail = to_screen(p.pos - p.vel * STREAK_TIME);
                let alpha = p.alpha * (1.0 - u);
                batch.streak(tail, head, p.size.0, colour(heat, alpha));
            }
            batch.flush_if_full(Some(dot), Some(canvas));
        }
        batch.flush(Some(dot), Some(canvas));
        gl_use_default_material();
    }
}

fn colour(c: Vec3, alpha: f32) -> Color {
    Color::new(c.x, c.y, c.z, alpha.clamp(0.0, 1.0))
}

/// Quads gathered into draw calls of at most CHUNK.
struct Batch {
    z: f32,
    verts: Vec<Vertex>,
    indices: Vec<u16>,
}

impl Batch {
    fn new(z: f32) -> Batch {
        Batch {
            z,
            verts: Vec::with_capacity(CHUNK * 4),
            indices: Vec::with_capacity(CHUNK * 6),
        }
    }

    fn quad(&mut self, corners: [Vec2; 4], c: Color) {
        let base = self.verts.len() as u16;
        for (k, (u, v)) in corners.into_iter().zip([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]) {
            self.verts.push(Vertex::new(k.x, k.y, self.z, u, v, c));
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn puff(&mut self, p: Vec2, r: f32, c: Color) {
        self.quad([p + vec2(-r, -r), p + vec2(r, -r), p + vec2(r, r), p + vec2(-r, r)], c);
    }

    /// A soft capsule from `tail` to `head`, `half` wide each side.
    fn streak(&mut self, tail: Vec2, head: Vec2, half: f32, c: Color) {
        let d = head - tail;
        let along = d.normalize_or(vec2(1.0, 0.0)) * half;
        let side = vec2(-along.y, along.x);
        let (t, h) = (tail - along, head + along);
        self.quad([t - side, h - side, h + side, t + side], c);
    }

    /// A solid chip: a quad `size` (half-extents) turned to `angle`.
    fn chip(&mut self, p: Vec2, size: (f32, f32), angle: f32, c: Color) {
        let (s, k) = angle.sin_cos();
        let a = vec2(k, s) * size.0;
        let b = vec2(-s, k) * size.1;
        self.quad([p - a - b, p + a - b * 0.6, p + a + b, p - a * 0.7 + b], c);
    }

    fn flush_if_full(&mut self, texture: Option<&Texture2D>, glow: Option<&Canvas>) {
        if self.verts.len() >= CHUNK * 4 {
            self.flush(texture, glow);
        }
    }

    /// Draws what is gathered; `glow` also hands it to the canvas's bloom.
    fn flush(&mut self, texture: Option<&Texture2D>, glow: Option<&Canvas>) {
        if self.verts.is_empty() {
            return;
        }
        let vertices = std::mem::take(&mut self.verts);
        let indices = std::mem::take(&mut self.indices);
        if let Some(canvas) = glow {
            canvas.add_glow(vertices.clone(), indices.clone(), texture);
        }
        draw_mesh(&Mesh {
            vertices,
            indices,
            texture: texture.cloned(),
        });
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying vec2 uv;
varying vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying vec2 uv;
varying vec4 color;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    const ROCK: Look = Look {
        smoke: Vec3::new(0.5, 0.45, 0.4),
        opacity: 0.5,
        debris: Some(Vec3::new(0.6, 0.5, 0.4)),
        tint: Vec3::ONE,
        tint_amount: 0.0,
        heat: 0.6,
    };

    fn mean(e: &Bursts, kind: Kind) -> Vec2 {
        let ps: Vec<_> = e.parts.iter().filter(|p| p.kind == kind).collect();
        ps.iter().map(|p| p.pos).sum::<Vec2>() / ps.len() as f32
    }

    #[test]
    fn an_explosion_flies_on_with_the_shot() {
        let mut e = Bursts::new();
        e.blast(Vec2::ZERO, vec2(0.0, -900.0), 40.0, 1.0, ROCK);
        for _ in 0..15 {
            e.update(1.0 / 60.0);
        }
        // Every kind has moved the way the shot went, sparks and debris
        // (less drag, more carry) furthest.
        for kind in [Kind::Fire, Kind::Spark, Kind::Debris, Kind::Smoke] {
            let m = mean(&e, kind);
            assert!(m.y < -10.0 && m.x.abs() < 0.5 * -m.y, "{m}");
        }
        assert!(mean(&e, Kind::Spark).y < mean(&e, Kind::Smoke).y);
    }

    #[test]
    fn fire_cools_from_white_to_red() {
        assert!(blackbody(1.0).z > 0.7);
        assert!(blackbody(0.0).x > 0.5 && blackbody(0.0).y < 0.1);
    }
}
