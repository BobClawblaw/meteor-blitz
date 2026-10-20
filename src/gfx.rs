//! The original's rendering pipeline, shared by the menus and the game.
//!
//! The iPhone build draws everything through one perspective camera into a
//! portrait 320x480 framebuffer and rotates the world into landscape with a
//! fixed matrix (`Director::initWithView`). Layers sit at different depths
//! (`BasicScreen::draw`, `GameScreen::draw`). Content here is given in the
//! original's landscape coordinates (480x320, y down) plus a depth; the
//! portrait target is blitted onto the window as a 480x320 landscape image.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams, UniformDesc, UniformType};
use macroquad::prelude::*;

pub const CANVAS_W: f32 = 480.0;
pub const CANVAS_H: f32 = 320.0;
/// The original framebuffer is portrait; the world is rotated into it.
const FB_W: f32 = 320.0;
const FB_H: f32 = 480.0;

// Director::initWithView: gluPerspective(60, w/h, 0.5, 1500) and
// gluLookAt((w/2, h/2, EYE_Z) -> (w/2, h/2, 0), up +y).
const FOV_Y_DEG: f32 = 60.0;
const Z_NEAR: f32 = 0.5;
const Z_FAR: f32 = 1500.0;
const EYE_Z: f32 = 414.988;

// Background: a 16x16 grid of 64-unit cells (1024x1024), texture
// coordinates at 0.002 per unit, only the inner cells drawn.
const CELL: f32 = 64.0;
const GRID: usize = 16;
pub const BACKDROP_SIZE: f32 = CELL * GRID as f32;
const TEXEL_PER_UNIT: f32 = 0.002;
const DRAWN_X: std::ops::RangeInclusive<usize> = 1..=14;
const DRAWN_Y: std::ops::RangeInclusive<usize> = 3..=12;

// World::initOffsetZs: the surface is a sphere of radius 700 (z = 700 -
// sqrt(700^2 - d^2)), looked up by integer squared distance.
const SPHERE_R2: f32 = 490_000.0;
const SPHERE_R: f32 = 700.0;

const DEFAULT_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const TEXTURED_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = texture2D(Texture, uv) * color;
}"#;

/// Texture unit 1 was GL_DECAL: the decal layer over the base by the
/// decal's alpha, alpha from the base. Both layers repeat and scroll.
const BACKDROP_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Decal;
uniform vec2 Scroll0;
uniform vec2 Scroll1;
void main() {
    vec4 base = texture2D(Texture, fract(uv + Scroll0));
    vec4 decal = texture2D(Decal, fract(uv + Scroll1));
    gl_FragColor = vec4(mix(base.rgb, decal.rgb, decal.a) * color.rgb, base.a * color.a);
}"#;

fn pipeline(dst: BlendFactor) -> PipelineParams {
    let blend = BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), dst);
    PipelineParams {
        color_blend: Some(blend),
        alpha_blend: Some(blend),
        cull_face: CullFace::Nothing,
        depth_test: Comparison::Always,
        depth_write: false,
        ..Default::default()
    }
}

/// The original camera: perspective into the portrait framebuffer, then the
/// landscape rotation. Maps landscape (x, y, z) to portrait (320-y, 480-x, -z).
struct OriginalCamera {
    render_pass: macroquad::texture::RenderPass,
}

impl Camera for OriginalCamera {
    fn matrix(&self) -> Mat4 {
        let proj = Mat4::perspective_rh_gl(FOV_Y_DEG.to_radians(), FB_W / FB_H, Z_NEAR, Z_FAR);
        let view = Mat4::look_at_rh(
            vec3(FB_W * 0.5, FB_H * 0.5, EYE_Z),
            vec3(FB_W * 0.5, FB_H * 0.5, 0.0),
            vec3(0.0, 1.0, 0.0),
        );
        let rotate = Mat4::from_translation(vec3(FB_W * 0.5, FB_H * 0.5, 0.0))
            * Mat4::from_rotation_z((-90.0f32).to_radians())
            * Mat4::from_rotation_x(180.0f32.to_radians())
            * Mat4::from_translation(vec3(-CANVAS_W * 0.5, -CANVAS_H * 0.5, 0.0));
        proj * view * rotate
    }
    fn depth_enabled(&self) -> bool {
        false
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        Some(self.render_pass.clone())
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

/// `World::getOffsetZ`: height of the planet surface below the plane at a
/// point `(dx, dy)` from the world centre.
fn offset_z(dx: f32, dy: f32) -> f32 {
    let d2 = (dx * dx + dy * dy) as i32;
    if d2 < SPHERE_R2 as i32 {
        SPHERE_R - (SPHERE_R2 - d2 as f32).sqrt()
    } else {
        f32::INFINITY
    }
}

/// `Background::Background(base, decal, flipped)`: a curved grid centred on
/// its origin. Menus build it flipped (surface curving away), the game not.
fn backdrop_mesh(flipped: bool) -> Mesh {
    let sign = if flipped { -1.0 } else { 1.0 };
    let mut vertices = Vec::with_capacity((GRID + 1) * (GRID + 1));
    for ix in 0..=GRID {
        for iy in 0..=GRID {
            let x = ix as f32 * CELL - BACKDROP_SIZE * 0.5;
            let y = iy as f32 * CELL - BACKDROP_SIZE * 0.5;
            let z = sign * offset_z(-x, -y);
            // GL textures have their origin at the bottom, so v runs the other way.
            vertices.push(Vertex::new(x, y, z, x * TEXEL_PER_UNIT, -y * TEXEL_PER_UNIT, WHITE));
        }
    }
    let mut indices = Vec::new();
    for ix in DRAWN_X {
        for iy in DRAWN_Y {
            let i0 = (ix * (GRID + 1) + iy) as u16;
            let i1 = ((ix + 1) * (GRID + 1) + iy) as u16;
            indices.extend_from_slice(&[i0, i1, i0 + 1, i1, i1 + 1, i0 + 1]);
        }
    }
    Mesh { vertices, indices, texture: None }
}

pub struct Canvas {
    target: RenderTarget,
    menu_backdrop: Mesh,
    game_backdrop: Mesh,
    backdrop_material: Material,
    additive: Material,
}

impl Canvas {
    pub fn new() -> Canvas {
        let target = render_target(FB_W as u32, FB_H as u32);
        target.texture.set_filter(FilterMode::Linear);
        let backdrop_material = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: BACKDROP_FRAGMENT },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                textures: vec!["Decal".to_owned()],
                uniforms: vec![UniformDesc::new("Scroll0", UniformType::Float2), UniformDesc::new("Scroll1", UniformType::Float2)],
            },
        )
        .expect("backdrop shader");
        let additive = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: TEXTURED_FRAGMENT },
            MaterialParams { pipeline_params: pipeline(BlendFactor::One), ..Default::default() },
        )
        .expect("additive shader");
        Canvas { target, menu_backdrop: backdrop_mesh(true), game_backdrop: backdrop_mesh(false), backdrop_material, additive }
    }

    /// Where the landscape canvas lands in the window: largest 3:2 fit, centred.
    fn placement() -> Rect {
        let scale = (screen_width() / CANVAS_W).min(screen_height() / CANVAS_H);
        let (w, h) = (CANVAS_W * scale, CANVAS_H * scale);
        Rect::new((screen_width() - w) * 0.5, (screen_height() - h) * 0.5, w, h)
    }

    /// Mouse position in canvas coordinates.
    pub fn mouse() -> Vec2 {
        let r = Canvas::placement();
        let (mx, my) = mouse_position();
        vec2((mx - r.x) / r.w * CANVAS_W, (my - r.y) / r.h * CANVAS_H)
    }

    pub fn begin(&self) {
        set_camera(&OriginalCamera { render_pass: self.target.render_pass.clone() });
        clear_background(BLACK);
    }

    /// Blit the portrait target as the landscape image. Landscape (x, y) is
    /// portrait (320-y, 480-x); the target texture's v axis runs bottom-up.
    pub fn end(&self) {
        set_default_camera();
        clear_background(BLACK);
        let r = Canvas::placement();
        let corner = |x: f32, y: f32| {
            let (lx, ly) = (x * CANVAS_W, y * CANVAS_H);
            Vertex::new(r.x + x * r.w, r.y + y * r.h, 0.0, (FB_W - ly) / FB_W, (FB_H - lx) / FB_H, WHITE)
        };
        let quad = Mesh {
            vertices: vec![corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(self.target.texture.clone()),
        };
        draw_mesh(&quad);
    }

    /// `Background::draw` at `origin`: the base layer scrolls at 2x the
    /// background offset and the decal at 3x, in texture space.
    pub fn draw_backdrop(&self, menu: bool, base: &Texture2D, decal: &Texture2D, origin: Vec3, scroll: Vec2, alpha: f32) {
        let source = if menu { &self.menu_backdrop } else { &self.game_backdrop };
        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        let vertices = source
            .vertices
            .iter()
            .map(|v| Vertex { position: v.position + origin, color: [255, 255, 255, a], ..*v })
            .collect();
        let mesh = Mesh { vertices, indices: source.indices.clone(), texture: Some(base.clone()) };
        let wrap = |k: f32| vec2((scroll.x * k).rem_euclid(BACKDROP_SIZE) / BACKDROP_SIZE, (scroll.y * k).rem_euclid(BACKDROP_SIZE) / BACKDROP_SIZE);
        self.backdrop_material.set_texture("Decal", decal.clone());
        self.backdrop_material.set_uniform("Scroll0", wrap(2.0));
        self.backdrop_material.set_uniform("Scroll1", wrap(3.0));
        gl_use_material(&self.backdrop_material);
        draw_mesh(&mesh);
        gl_use_default_material();
    }

    /// Additively blended square sprites centred on each point
    /// (`glPointSizePointerOES` point sprites with `GL_SRC_ALPHA, GL_ONE`).
    pub fn draw_points(&self, tex: &Texture2D, points: &[(Vec2, f32)], z: f32, color: Color) {
        if points.is_empty() {
            return;
        }
        let mut vertices = Vec::with_capacity(points.len() * 4);
        let mut indices = Vec::with_capacity(points.len() * 6);
        for (i, (p, size)) in points.iter().enumerate() {
            let h = size * 0.5;
            let base = (i * 4) as u16;
            vertices.push(Vertex::new(p.x - h, p.y - h, z, 0.0, 0.0, color));
            vertices.push(Vertex::new(p.x + h, p.y - h, z, 1.0, 0.0, color));
            vertices.push(Vertex::new(p.x + h, p.y + h, z, 1.0, 1.0, color));
            vertices.push(Vertex::new(p.x - h, p.y + h, z, 0.0, 1.0, color));
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        gl_use_material(&self.additive);
        draw_mesh(&Mesh { vertices, indices, texture: Some(tex.clone()) });
        gl_use_default_material();
    }
}

/// `Texture::draw(x, y, scale)`: a top-left anchored quad at depth `z`,
/// modulated by the current `DrawManager` colour.
pub fn draw_quad(tex: &Texture2D, x: f32, y: f32, scale: f32, z: f32, color: Color) {
    let (w, h) = (tex.width() * scale, tex.height() * scale);
    let quad = Mesh {
        vertices: vec![
            Vertex::new(x, y, z, 0.0, 0.0, color),
            Vertex::new(x + w, y, z, 1.0, 0.0, color),
            Vertex::new(x + w, y + h, z, 1.0, 1.0, color),
            Vertex::new(x, y + h, z, 0.0, 1.0, color),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(tex.clone()),
    };
    draw_mesh(&quad);
}

/// `Sprite::draw`: a texture centred on `(x, y)`, rotated by `angle_deg`
/// about its centre (`glRotatef(angle, 0, 0, 1)`).
pub fn draw_sprite(tex: &Texture2D, x: f32, y: f32, angle_deg: f32, z: f32, color: Color) {
    let (hw, hh) = (tex.width() * 0.5, tex.height() * 0.5);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let corner = |dx: f32, dy: f32, u: f32, v: f32| Vertex::new(x + dx * c - dy * s, y + dx * s + dy * c, z, u, v, color);
    let quad = Mesh {
        vertices: vec![corner(-hw, -hh, 0.0, 0.0), corner(hw, -hh, 1.0, 0.0), corner(hw, hh, 1.0, 1.0), corner(-hw, hh, 0.0, 1.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(tex.clone()),
    };
    draw_mesh(&quad);
}

/// `Texture::drawMultipleFrames` for one sprite: cell `index` of a
/// `cols` x `rows` atlas, centred on `(x, y)` and rotated by `angle_deg`.
pub fn draw_atlas_sprite(tex: &Texture2D, cols: usize, rows: usize, index: usize, x: f32, y: f32, angle_deg: f32, z: f32, color: Color) {
    let (cw, ch) = (1.0 / cols as f32, 1.0 / rows as f32);
    let (u0, v0) = ((index % cols) as f32 * cw, (index / cols % rows) as f32 * ch);
    let (hw, hh) = (tex.width() * cw * 0.5, tex.height() * ch * 0.5);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let corner = |dx: f32, dy: f32, u: f32, v: f32| Vertex::new(x + dx * c - dy * s, y + dx * s + dy * c, z, u, v, color);
    let quad = Mesh {
        vertices: vec![
            corner(-hw, -hh, u0, v0),
            corner(hw, -hh, u0 + cw, v0),
            corner(hw, hh, u0 + cw, v0 + ch),
            corner(-hw, hh, u0, v0 + ch),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(tex.clone()),
    };
    draw_mesh(&quad);
}
