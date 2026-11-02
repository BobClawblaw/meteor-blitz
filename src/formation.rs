//! Formation gas (port addition, Options > Particle style): explosions,
//! impacts and shots leave smoke that streams and curls, after the motion
//! of BlockYard's pulsar wind (public/js/details3d.js, `drawPulsar` and
//! `PULSAR_GLSL`) and the smooth density of its Formation sky.
//!
//! The gas is a particle simulation drawn as soft puffs: thousands of
//! points, each advected every step through a flow, wide enough that
//! neighbours merge into one smooth density. From the pulsar wind:
//! - Dots are spawned on shared lanes (a blast's jets, a shot's path), so
//!   they string out into plumes.
//! - Brightness banding is carried by the dot, fixed at spawn and shared
//!   along its lane, so the dense sheets are made of gas and move with it.
//! - The turbulence is a two-octave curl field (the perpendicular gradient
//!   of a stream function), kept weaker than the drift, so eddies bend the
//!   gas rather than become it.
//!
//! What it looks like is a `Smoke`, chosen by the caller from what was hit
//! and what hit it: a smoke colour drawn with ordinary blending (dust,
//! soot, frost, steam can be dull or dark), and a brief additive glow
//! (the weapon's or the blast's heat) that dies early in the dot's life.
//! Each dot keeps the momentum it was given (a share of the shot's, plus
//! its jet's), losing it to drag, so the gas trails the way things moved.

use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams,
};
use macroquad::prelude::*;

/// How a puff of gas looks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Smoke {
    /// The smoke's colour, alpha-blended.
    pub colour: Vec3,
    /// How thick it is, 0..1.
    pub opacity: f32,
    /// The heat glowing in it while it is young, added.
    pub glow: Vec3,
    /// How strong that glow is, 0..1.
    pub heat: f32,
}

struct Dot {
    pos: Vec2,
    /// World units per second, lost to DRAG.
    vel: Vec2,
    age: f32,
    life: f32,
    /// Its own pace through the eddies (the pulsar's `m`).
    pace: f32,
    /// Density, 0..1: the dot's own times its lane's carried banding.
    bright: f32,
    smoke: Smoke,
}

/// Momentum lost per second (velocity x e^-DRAG t).
const DRAG: f32 = 2.4;
/// The curl field: two octaves (world units) and its speed. The pulsar's
/// rule: well under the drift, or the eddies become the wind.
const EDDY_SMALL: f32 = 13.0;
const EDDY_LARGE: f32 = 34.0;
const EDDY_SPEED: f32 = 42.0;
/// The most dots alive at once; past it new gas is not spawned.
const MAX_DOTS: usize = 20_000;
/// Dots per unit of a blast's radius, and the cap for one blast.
const BLAST_DENSITY: f32 = 11.0;
const MAX_BLAST_DOTS: usize = 1_800;
/// An impact's spray, and the dots a shot sheds per step.
const IMPACT_DOTS: usize = 40;
const TRAIL_DOTS: usize = 3;
/// A trail dot keeps up to this share of the shot's velocity.
const TRAIL_CARRY: f32 = 0.12;
/// The share of its life a dot glows for.
const GLOW_TIME: f32 = 0.3;
/// Quads per draw call: 3,200 vertices and 4,800 indices, inside
/// macroquad's batch (10,000 and 5,000).
const CHUNK: usize = 800;

pub struct Wind {
    dots: Vec<Dot>,
    time: f32,
    seed: u32,
}

thread_local! {
    /// The additive material for the glow, built on first use and shared
    /// by every game.
    static GLOW: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}

fn glow_material() -> Material {
    GLOW.with(|m| {
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
            .expect("formation shader")
        })
        .clone()
    })
}

impl Wind {
    pub fn new() -> Wind {
        Wind {
            dots: Vec::new(),
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

    /// Three draws averaged: -1..1, heaped at 0 (the pulsar's `bunch`).
    fn bunch(&mut self) -> f32 {
        (self.rand() + self.rand() + self.rand()) / 1.5 - 1.0
    }

    fn push(&mut self, dot: Dot) {
        if self.dots.len() < MAX_DOTS {
            self.dots.push(dot);
        }
    }

    /// An explosion `radius` world units across, its gas lasting about
    /// `life` seconds, the whole of it carried along at `drift` (a share of
    /// the momentum of the shot that did it). The gas leaves on jets: each
    /// jet is a lane its dots share, strung out by speed, with one banding.
    pub fn blast(&mut self, pos: Vec2, drift: Vec2, radius: f32, life: f32, smoke: Smoke) {
        let count = ((radius * BLAST_DENSITY) as usize).min(MAX_BLAST_DOTS);
        let jets = 5 + (radius / 20.0) as usize;
        let turn = self.rand() * std::f32::consts::TAU;
        let lanes: Vec<(f32, f32)> = (0..jets)
            .map(|j| {
                let angle = turn + (j as f32 + 0.6 * (self.rand() - 0.5)) / jets as f32 * std::f32::consts::TAU;
                let band = 0.5 + 0.5 * (j as f32 * 2.7 + self.rand()).sin();
                (angle, 0.45 + 0.55 * band * band)
            })
            .collect();
        for _ in 0..count {
            let (angle, band) = lanes[(self.rand() * jets as f32) as usize % jets];
            let angle = angle + self.bunch() * 0.5;
            // Coasting distance is speed / DRAG: the fastest dots reach the
            // blast's radius, the rest string out behind them.
            let reach = self.rand().powf(0.6);
            let dir = vec2(angle.cos(), angle.sin());
            let dot = Dot {
                pos: pos + dir * radius * 0.08 * self.rand(),
                vel: dir * radius * DRAG * reach + drift,
                age: 0.0,
                life: life * (0.6 + 0.8 * self.rand()),
                pace: 0.55 + 0.9 * self.rand(),
                bright: band * (0.5 + 0.5 * self.rand()),
                smoke,
            };
            self.push(dot);
        }
    }

    /// A shot's impact: a spray carried on with the shot's momentum
    /// (`drift`), opening into a cone around it.
    pub fn impact(&mut self, pos: Vec2, drift: Vec2, smoke: Smoke) {
        let speed = drift.length();
        let along = if speed > 1e-3 { drift / speed } else { vec2(0.0, -1.0) };
        let lane = self.rand();
        for _ in 0..IMPACT_DOTS {
            let dir = Vec2::from_angle(self.bunch() * 0.9).rotate(along);
            let dot = Dot {
                pos,
                vel: dir * (speed * (0.3 + 0.9 * self.rand()) + 30.0 * self.rand()),
                age: 0.0,
                life: 0.3 + 0.35 * self.rand(),
                pace: 0.55 + 0.9 * self.rand(),
                bright: (0.4 + 0.6 * lane) * (0.4 + 0.6 * self.rand()),
                smoke,
            };
            self.push(dot);
        }
    }

    /// Gas shed by a shot that just flew from `from` to `to` at `vel`
    /// (world units per second): a few dots left along the step, drifting
    /// on slowly the way it went.
    pub fn trail(&mut self, from: Vec2, to: Vec2, vel: Vec2, smoke: Smoke) {
        let side = vec2(-vel.y, vel.x).normalize_or_zero();
        for _ in 0..TRAIL_DOTS {
            let along = self.rand();
            let dot = Dot {
                pos: from.lerp(to, along) + side * self.bunch() * 1.5,
                vel: vel * TRAIL_CARRY * self.rand(),
                age: 0.0,
                life: 0.3 + 0.35 * self.rand(),
                pace: 0.55 + 0.9 * self.rand(),
                bright: 0.25 + 0.45 * self.rand(),
                smoke,
            };
            self.push(dot);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        let keep = (-DRAG * dt).exp();
        let (a, b) = (self.time * 0.37, -self.time * 0.23);
        // The curl of a stream function sin x sin y, its phase drifting so
        // the eddies churn.
        let curl = |q: Vec2, l: f32, pa: f32, pb: f32| {
            let (sx, cx) = (q.x / l + pa).sin_cos();
            let (sy, cy) = (q.y / l + pb).sin_cos();
            vec2(sx * cy, -cx * sy)
        };
        for d in &mut self.dots {
            let eddy = curl(d.pos, EDDY_SMALL, a, b) + 0.7 * curl(d.pos, EDDY_LARGE, b, a);
            d.pos += (d.vel + eddy * EDDY_SPEED * d.pace) * dt;
            d.vel *= keep;
            d.age += dt;
        }
        self.dots.retain(|d| d.age < d.life);
    }

    pub fn clear(&mut self) {
        self.dots.clear();
    }

    /// Draws the gas at depth `z`, `to_screen` mapping world positions to
    /// the canvas, `dot` the soft round sprite: the smoke alpha-blended,
    /// then the glow of the young gas added over it.
    pub fn draw(&self, z: f32, to_screen: impl Fn(Vec2) -> Vec2, dot: &Texture2D) {
        if self.dots.is_empty() {
            return;
        }
        let mut verts = Vec::with_capacity(CHUNK * 4);
        let mut indices: Vec<u16> = Vec::with_capacity(CHUNK * 6);
        let flush = |verts: &mut Vec<Vertex>, indices: &mut Vec<u16>| {
            if !verts.is_empty() {
                draw_mesh(&Mesh {
                    vertices: std::mem::take(verts),
                    indices: std::mem::take(indices),
                    texture: Some(dot.clone()),
                });
            }
        };
        let quad = |verts: &mut Vec<Vertex>, indices: &mut Vec<u16>, p: Vec2, radius: f32, c: Color| {
            let base = verts.len() as u16;
            for (dx, dy, tu, tv) in [(-1.0, -1.0, 0.0, 0.0), (1.0, -1.0, 1.0, 0.0), (1.0, 1.0, 1.0, 1.0), (-1.0, 1.0, 0.0, 1.0)] {
                verts.push(Vertex::new(p.x + dx * radius, p.y + dy * radius, z, tu, tv, c));
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            if verts.len() >= CHUNK * 4 {
                flush(verts, indices);
            }
        };
        // Smoke: a broad haze, then the body, both soft and wide enough
        // that neighbouring dots merge into one continuous density.
        gl_use_default_material();
        for pass in 0..2 {
            for d in &self.dots {
                let b = density(d);
                if b < 0.02 {
                    continue;
                }
                let (radius, alpha) = if pass == 0 {
                    (9.0 + 9.0 * b, 0.03 + 0.05 * b)
                } else {
                    (4.0 + 4.0 * b, 0.05 + 0.12 * b)
                };
                let s = d.smoke;
                let c = Color::new(s.colour.x, s.colour.y, s.colour.z, alpha * s.opacity);
                quad(&mut verts, &mut indices, to_screen(d.pos), radius, c);
            }
        }
        flush(&mut verts, &mut indices);
        // Glow: the young gas, added.
        gl_use_material(&glow_material());
        for d in &self.dots {
            let hot = (1.0 - d.age / (d.life * GLOW_TIME)).max(0.0) * d.smoke.heat;
            let b = density(d);
            if hot * b < 0.02 {
                continue;
            }
            let g = d.smoke.glow;
            let c = Color::new(g.x, g.y, g.z, hot * (0.08 + 0.22 * b));
            quad(&mut verts, &mut indices, to_screen(d.pos), 4.0 + 4.0 * b, c);
        }
        flush(&mut verts, &mut indices);
        gl_use_default_material();
    }
}

/// A dot's density now: its brightness, fading in quickly and out as it
/// ages.
fn density(d: &Dot) -> f32 {
    let u = d.age / d.life;
    (d.bright * (1.0 - u * u) * (u / 0.06).min(1.0)).clamp(0.0, 0.999)
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

    const DUST: Smoke = Smoke {
        colour: Vec3::new(0.5, 0.45, 0.4),
        opacity: 0.5,
        glow: Vec3::ONE,
        heat: 0.5,
    };

    #[test]
    fn gas_keeps_the_shots_momentum() {
        let mut w = Wind::new();
        w.blast(Vec2::ZERO, vec2(0.0, -100.0), 40.0, 1.0, DUST);
        for _ in 0..30 {
            w.update(1.0 / 60.0);
        }
        let mean = w.dots.iter().map(|d| d.pos).sum::<Vec2>() / w.dots.len() as f32;
        // The blast drifts the way the shot went (100 / DRAG coasts ~42 units in all).
        assert!(mean.y < -15.0, "{mean}");
        assert!(mean.x.abs() < 10.0, "{mean}");
    }
}
