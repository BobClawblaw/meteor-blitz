//! Formation gas (port addition, Options > Particle style): explosions and
//! impacts drawn as short-lived clouds of glowing gas in the look of
//! BlockYard's "The Formation" sky (public/js/formgl.js, after the TNG50
//! film): warped ridged noise for wisps and a finer tangle, the gas swirled
//! as it expands, a ragged outburst shell torn by the smoke, a white-hot
//! core, and colour from the Formation's seven-stop ramps (magma, the
//! film's own, and cobalt & gold). Gas tops out at orange (or blue); only
//! the core reaches the pale end.
//!
//! The Formation is a field evaluated per pixel; here each cloud is one
//! quad and the field is evaluated inside it. formgl.js hashes with 32-bit
//! integers (WebGL 2); this shader is GLSL 100 for WebGL 1, so its value
//! noise hashes with floats instead. Each quad's age, seed, palette and
//! kind ride in its vertex colour, so all clouds draw in one batch.

use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams,
};
use macroquad::prelude::*;

/// Which Formation ramp a cloud is drawn through.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Palette {
    /// The film's own: black, indigo, violet, magenta, salmon, orange, pale yellow.
    Magma,
    /// BlockYard's shipped default: all-blue gas, a warm gold core.
    CobaltGold,
}

impl Palette {
    /// A colour to tint the original embers and streaks with, near the
    /// ramp's dense-gas end.
    pub fn ember(self) -> Color {
        match self {
            Palette::Magma => Color::new(1.0, 0.62, 0.38, 1.0),
            Palette::CobaltGold => Color::new(0.55, 0.74, 1.0, 1.0),
        }
    }
}

/// A blast (expanding, with an outburst shell) or an impact (a small,
/// quick puff with a hot core and no shell).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Blast,
    Impact,
}

struct Cloud {
    pos: Vec2,
    /// World units per second: the momentum of what made it, losing speed
    /// to GAS_DRAG.
    vel: Vec2,
    radius: f32,
    age: f32,
    life: f32,
    palette: Palette,
    kind: Kind,
    seed: u8,
}

/// How fast a cloud's momentum dies away, per second (velocity x
/// e^-drag t, so it coasts velocity / GAS_DRAG units in all).
const GAS_DRAG: f32 = 3.0;
/// A moving cloud is drawn stretched along its motion: 1 + speed / this,
/// up to MAX_STRETCH, rounding out as it slows.
const STRETCH_SPEED: f32 = 160.0;
const MAX_STRETCH: f32 = 2.2;

/// The most clouds alive at once; the oldest gives way.
const MAX_CLOUDS: usize = 64;
/// The quad reaches this far past the cloud's radius, so the ragged shell
/// is never cut by its edge.
const QUAD_MARGIN: f32 = 1.15;

pub struct Gas {
    clouds: Vec<Cloud>,
    next_seed: u8,
}

thread_local! {
    /// The shader, built on first use and shared by every game.
    static MATERIAL: std::cell::OnceCell<Material> = const { std::cell::OnceCell::new() };
}

fn material() -> Material {
    MATERIAL.with(|m| {
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

impl Gas {
    pub fn new() -> Gas {
        Gas {
            clouds: Vec::new(),
            next_seed: 0,
        }
    }

    /// Starts a cloud at a world position, `radius` world units across at
    /// its widest, lasting `life` seconds, moving at `vel`.
    pub fn spawn(&mut self, pos: Vec2, vel: Vec2, radius: f32, life: f32, palette: Palette, kind: Kind) {
        if self.clouds.len() >= MAX_CLOUDS {
            self.clouds.remove(0);
        }
        // Seeds step by a large odd number so neighbours look unrelated.
        self.next_seed = self.next_seed.wrapping_add(97);
        self.clouds.push(Cloud {
            pos,
            vel,
            radius,
            age: 0.0,
            life,
            palette,
            kind,
            seed: self.next_seed,
        });
    }

    pub fn update(&mut self, dt: f32) {
        let keep = (-GAS_DRAG * dt).exp();
        for c in &mut self.clouds {
            c.age += dt;
            c.pos += c.vel * dt;
            c.vel *= keep;
        }
        self.clouds.retain(|c| c.age < c.life);
    }

    pub fn clear(&mut self) {
        self.clouds.clear();
    }

    /// Draws every cloud at depth `z`; `to_screen` maps world positions to
    /// the canvas. Additive, so where there is no gas nothing changes.
    pub fn draw(&self, z: f32, to_screen: impl Fn(Vec2) -> Vec2) {
        if self.clouds.is_empty() {
            return;
        }
        let mut verts = Vec::with_capacity(self.clouds.len() * 4);
        let mut indices = Vec::with_capacity(self.clouds.len() * 6);
        for c in &self.clouds {
            let p = to_screen(c.pos);
            let h = c.radius * QUAD_MARGIN;
            let code = match (c.palette, c.kind) {
                (Palette::Magma, Kind::Blast) => 0,
                (Palette::Magma, Kind::Impact) => 85,
                (Palette::CobaltGold, Kind::Blast) => 170,
                (Palette::CobaltGold, Kind::Impact) => 255,
            };
            let age = ((c.age / c.life).clamp(0.0, 1.0) * 255.0) as u8;
            let color = Color::from_rgba(age, c.seed, code, 255);
            let base = verts.len() as u16;
            // The quad turned to the motion and stretched along it.
            let speed = c.vel.length();
            let along = if speed > 1e-3 { c.vel / speed } else { vec2(1.0, 0.0) };
            let across = vec2(-along.y, along.x);
            let stretch = (1.0 + speed / STRETCH_SPEED).min(MAX_STRETCH);
            let (a, b) = (along * h * stretch, across * h);
            for (k, u, v) in [(-a - b, 0.0, 0.0), (a - b, 1.0, 0.0), (a + b, 1.0, 1.0), (-a + b, 0.0, 1.0)] {
                verts.push(Vertex::new(p.x + k.x, p.y + k.y, z, u, v, color));
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        gl_use_material(&material());
        draw_mesh(&Mesh {
            vertices: verts,
            indices,
            texture: None,
        });
        gl_use_default_material();
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying vec2 uv;
varying vec4 params;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    params = color0 / 255.0;
    uv = texcoord;
}"#;

/// The cloud's field. `params`: age (0..1 of its life), seed, palette and
/// kind (0 magma blast, 1/3 magma impact, 2/3 cobalt blast, 1 cobalt
/// impact). Constants follow formgl.js where they carry over: the noise's
/// rotation and lacunarity, the ridged weighting, the warp strengths, the
/// smoke mix and the ragged shell.
const FRAGMENT: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying vec2 uv;
varying vec4 params;

// Value noise with a float hash (formgl.js hashes with 32-bit ints, which
// WebGL 1 lacks); quintic fade as in formgl.js.
float h2(vec2 p) {
    vec3 q = fract(vec3(p.xyx) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}
float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(mix(h2(i), h2(i + vec2(1.0, 0.0)), u.x), mix(h2(i + vec2(0.0, 1.0)), h2(i + vec2(1.0, 1.0)), u.x), u.y);
}
const mat2 ROT = mat2(0.8, 0.6, -0.6, 0.8);
float fbm3(vec2 p) {
    float a = 0.5, s = 0.0;
    for (int k = 0; k < 3; k++) { s += a * vnoise(p); p = ROT * p * 2.03 + 17.3; a *= 0.5; }
    return s;
}
// Ridged: the creases of the noise are the filaments.
float ridged(vec2 p, int oct) {
    float a = 0.5, s = 0.0, w = 1.0;
    for (int k = 0; k < 4; k++) {
        if (k >= oct) break;
        float n = 1.0 - abs(vnoise(p) * 2.0 - 1.0);
        n *= n; s += a * n * w; w = clamp(n * 1.6, 0.0, 1.0);
        p = ROT * p * 2.07 + 9.1; a *= 0.55;
    }
    return s;
}

// The Formation's ramps: stops at 0, .14, .34, .54, .70, .85, 1.
vec3 ramp(float t, vec3 c1, vec3 c2, vec3 c3, vec3 c4, vec3 c5, vec3 c6) {
    t = clamp(t, 0.0, 1.0);
    if (t < 0.14) return mix(vec3(0.0), c1, t / 0.14);
    if (t < 0.34) return mix(c1, c2, (t - 0.14) / 0.20);
    if (t < 0.54) return mix(c2, c3, (t - 0.34) / 0.20);
    if (t < 0.70) return mix(c3, c4, (t - 0.54) / 0.16);
    if (t < 0.85) return mix(c4, c5, (t - 0.70) / 0.15);
    return mix(c5, c6, (t - 0.85) / 0.15);
}
vec3 magma(float t) {
    return ramp(t, vec3(19.0, 8.0, 56.0) / 255.0, vec3(66.0, 22.0, 122.0) / 255.0, vec3(143.0, 41.0, 133.0) / 255.0,
        vec3(219.0, 77.0, 102.0) / 255.0, vec3(251.0, 143.0, 84.0) / 255.0, vec3(255.0, 240.0, 199.0) / 255.0);
}
vec3 cobaltGold(float t) {
    return ramp(t, vec3(5.0, 10.0, 44.0) / 255.0, vec3(12.0, 28.0, 98.0) / 255.0, vec3(26.0, 56.0, 150.0) / 255.0,
        vec3(48.0, 92.0, 190.0) / 255.0, vec3(92.0, 140.0, 222.0) / 255.0, vec3(255.0, 232.0, 176.0) / 255.0);
}

void main() {
    float age = params.x;
    float code = floor(params.z * 3.0 + 0.5);
    bool cobalt = code >= 2.0;
    bool impact = mod(code, 2.0) > 0.5;
    vec2 off = vec2(params.y * 37.1 + 3.7, params.y * 91.7 + 1.3);

    // Local position: the quad spans the cloud's radius times QUAD_MARGIN.
    vec2 p = (uv * 2.0 - 1.0) * 1.15;
    float r = length(p);
    vec2 dir = p / max(r, 1e-4);

    // The cloud swells fast and eases out; its gas is swirled as it goes,
    // hardest at the middle (formgl.js FLOW_SWIRL), each cloud its own way.
    float grow = 1.0 - pow(1.0 - age, 3.0);
    float size = impact ? 0.45 + 0.55 * grow : 0.3 + 0.7 * grow;
    vec2 q = p / size;
    float rq = length(q);
    float turn = (params.y < 0.5 ? -1.0 : 1.0) * 1.5 * age * exp(-rq);
    float cs = cos(turn), sn = sin(turn);
    q = mat2(cs, sn, -sn, cs) * q;

    // The wisps: ridged noise through a warp that churns with age, and a
    // finer tangle through a second warp (formgl.js smoke()).
    float ch = age * 0.9;
    vec2 warp = vec2(fbm3(q * 1.4 + off + ch), fbm3(q * 1.4 + off + vec2(5.2, 1.3) - ch)) - 0.5;
    float wisps = ridged(q * 1.6 + 1.9 * warp + off, 4);
    vec2 warp2 = vec2(fbm3(q * 3.2 + 1.7 * warp + off.yx), fbm3(q * 3.2 - 1.7 * warp + off + vec2(-4.0, 2.0))) - 0.5;
    float fine = ridged(q * 4.4 + 2.6 * warp2 + 1.2 * warp + off, 3);
    float smoke = 0.55 * wisps * wisps + 0.30 * wisps + 0.35 * fine * fine;

    // The body: a soft envelope that gates the smoke (no gas, no wisps).
    float halo = exp(-rq * rq * 2.4) * (1.0 - smoothstep(0.75, 1.0, r));
    float gas = halo * (0.12 + 0.95 * smoke);

    // The outburst: a broad shell blown out and fading, its rim ragged and
    // torn by the smoke (a thin even ring reads as a donut).
    float blast = 0.0;
    if (!impact) {
        float ragged = 1.0 + 0.8 * (fbm3(dir * 2.2 + off + age * 0.7) - 0.5);
        float shellR = 0.2 + 0.62 * grow;
        blast = exp(-pow((r * ragged - shellR) / (0.09 + 0.20 * age), 2.0)) * (1.0 - age) * (1.0 - age)
            * smoothstep(0.0, 0.08, age) * (0.15 + 1.8 * wisps * fine + 0.6 * wisps);
    }

    // The core: white-hot, gone early (only stars reach the pale end).
    float core = exp(-r * r / (impact ? 0.03 + 0.06 * age : 0.015 + 0.05 * age)) * pow(1.0 - age, impact ? 2.0 : 3.0);

    float fade = pow(1.0 - age, 1.4) * smoothstep(0.0, 0.05, age);
    float density = (gas * 0.75 + blast * 0.6) * fade;
    // Gas tops out at the ramp's .85 stop; the core carries it to the end.
    float t = 0.85 * (1.0 - exp(-2.6 * density)) + 0.6 * core;
    vec3 col = cobalt ? cobaltGold(t) : magma(t);
    float a = clamp(density * 1.6 + core, 0.0, 1.0);
    gl_FragColor = vec4(col, a);
}"#;
