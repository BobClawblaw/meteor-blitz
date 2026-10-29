//! The original's rendering pipeline, shared by the menus and the game.
//!
//! The iPhone build draws everything through one perspective camera into a
//! portrait 320x480 framebuffer and rotates the world into landscape with a
//! fixed matrix (`Director::initWithView`). Layers sit at different depths
//! (`BasicScreen::draw`, `GameScreen::draw`). Content here is given in the
//! original's landscape coordinates (480x320, y down) plus a depth; the
//! portrait target is blitted onto the window as a 480x320 landscape image.
//!
//! The port widens the view to the window's aspect (up to 2.4:1): the
//! camera sees more of the world to either side, while every screen keeps
//! drawing in the original 480x320 coordinates, centred. `edge()` is how far
//! the view reaches past those 480 units on each side, for anything pinned
//! to the screen edges (the HUD, full-screen fills).

use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams,
    UniformDesc, UniformType,
};
use macroquad::prelude::*;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const CANVAS_W: f32 = 480.0;
pub const CANVAS_H: f32 = 320.0;
/// The original framebuffer is portrait; the world is rotated into it.
/// Its long side is the view width (480 in the original).
const FB_W: f32 = 320.0;
/// The widest view: 2.4:1 covers 21:9 monitors.
const MAX_ASPECT: f32 = 2.4;

/// The view width in canvas units, set each frame by `Canvas::begin`.
static VIEW_W: AtomicU32 = AtomicU32::new(0x43F0_0000); // 480.0

pub fn view_w() -> f32 {
    f32::from_bits(VIEW_W.load(Ordering::Relaxed))
}

/// How far the view reaches past the 480-unit canvas on each side.
pub fn edge() -> f32 {
    (view_w() - CANVAS_W) * 0.5
}

/// Settings::widescreen, set each frame before `Canvas::begin`.
static WIDESCREEN: AtomicBool = AtomicBool::new(true);

pub fn set_widescreen(on: bool) {
    WIDESCREEN.store(on, Ordering::Relaxed);
}

/// The window's aspect, from 3:2 (the original) to 2.4:1. With widescreen
/// off, or MB_WIDE=0 (emulator comparisons), the original 3:2 view,
/// letterboxed.
fn window_view_w() -> f32 {
    if !WIDESCREEN.load(Ordering::Relaxed) || std::env::var("MB_WIDE").is_ok_and(|v| v == "0") {
        return CANVAS_W;
    }
    (CANVAS_H * screen_width() / screen_height()).clamp(CANVAS_W, CANVAS_H * MAX_ASPECT).round()
}

/// Director::initWithView's camera for a view `view_w` wide: the original
/// at 480, and wider views keep the play plane's scale (the vertical
/// field of view stays put, the horizontal one opens up).
fn camera_matrix(view_w: f32) -> Mat4 {
    let fov = 2.0 * ((FOV_Y_DEG.to_radians() * 0.5).tan() * view_w / CANVAS_W).atan();
    let proj = Mat4::perspective_rh_gl(fov, FB_W / view_w, Z_NEAR, Z_FAR);
    let view = Mat4::look_at_rh(
        vec3(FB_W * 0.5, view_w * 0.5, EYE_Z),
        vec3(FB_W * 0.5, view_w * 0.5, 0.0),
        vec3(0.0, 1.0, 0.0),
    );
    let rotate = Mat4::from_translation(vec3(FB_W * 0.5, view_w * 0.5, 0.0))
        * Mat4::from_rotation_z((-90.0f32).to_radians())
        * Mat4::from_rotation_x(180.0f32.to_radians())
        * Mat4::from_translation(vec3(-CANVAS_W * 0.5, -CANVAS_H * 0.5, 0.0));
    proj * view * rotate
}

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
/// Cells drawn either side of the centre: columns 1-14 and rows 3-12 of
/// the 16x16 grid. A wide view adds columns in proportion.
const DRAWN_HALF_X: f32 = 7.0;
const DRAWN_HALF_Y: i32 = 5;

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

/// GL_ADD with a white vertex colour (Texture::drawFlash): a white
/// silhouette with the texture's alpha.
const FLASH_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    vec4 t = texture2D(Texture, uv);
    gl_FragColor = vec4(min(t.rgb + color.rgb, vec3(1.0)), t.a * color.a);
}"#;

/// Bloom's bright pass, downsampling 4x: a 4-tap box over the source
/// (each tap a bilinear 2x2), keeping what is brighter than Threshold.
const BRIGHT_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform vec2 Texel;
uniform float Threshold;
void main() {
    vec3 c = 0.25 * (texture2D(Texture, uv + Texel * vec2(-1.0, -1.0)).rgb
        + texture2D(Texture, uv + Texel * vec2(1.0, -1.0)).rgb
        + texture2D(Texture, uv + Texel * vec2(-1.0, 1.0)).rgb
        + texture2D(Texture, uv + Texel * vec2(1.0, 1.0)).rgb);
    float l = max(c.r, max(c.g, c.b));
    gl_FragColor = vec4(c * smoothstep(Threshold, 1.0, l), 1.0);
}"#;

/// Bloom's separable Gaussian, nine taps along Dir (in texels).
const BLUR_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform vec2 Dir;
void main() {
    vec3 c = texture2D(Texture, uv).rgb * 0.227027;
    c += texture2D(Texture, uv + Dir * 1.0).rgb * 0.1945946;
    c += texture2D(Texture, uv - Dir * 1.0).rgb * 0.1945946;
    c += texture2D(Texture, uv + Dir * 2.0).rgb * 0.1216216;
    c += texture2D(Texture, uv - Dir * 2.0).rgb * 0.1216216;
    c += texture2D(Texture, uv + Dir * 3.0).rgb * 0.054054;
    c += texture2D(Texture, uv - Dir * 3.0).rgb * 0.054054;
    c += texture2D(Texture, uv + Dir * 4.0).rgb * 0.016216;
    c += texture2D(Texture, uv - Dir * 4.0).rgb * 0.016216;
    gl_FragColor = vec4(c, 1.0);
}"#;

/// Bloom: what counts as bright, how strongly the glow is added, and the
/// glow buffers' size relative to the canvas.
const BLOOM_THRESHOLD: f32 = 0.8;
const BLOOM_STRENGTH: f32 = 1.0;
const BLOOM_DOWNSAMPLE: u32 = 4;

/// Draws a full target from texture space: (0,0)-(1,1) to the whole pass.
struct PassCamera {
    render_pass: macroquad::texture::RenderPass,
}

impl Camera for PassCamera {
    fn matrix(&self) -> Mat4 {
        Mat4::orthographic_rh_gl(0.0, 1.0, 0.0, 1.0, -1.0, 1.0)
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

/// One full-target quad sampling `tex` with the identity mapping.
fn pass(target: &RenderTarget, tex: &Texture2D, material: &Material) {
    set_camera(&PassCamera { render_pass: target.render_pass.clone() });
    gl_use_material(material);
    let v = |x: f32, y: f32| Vertex::new(x, y, 0.0, x, y, WHITE);
    draw_mesh(&Mesh {
        vertices: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(tex.clone()),
    });
    gl_use_default_material();
}

/// Glow for bright pixels (port addition): a bright pass and a blur at a
/// quarter of the canvas resolution, added over the frame at the blit.
struct Bloom {
    a: RenderTarget,
    b: RenderTarget,
    size: (u32, u32),
    bright: Material,
    blur: Material,
}

impl Bloom {
    fn new(canvas: (u32, u32)) -> Bloom {
        let size = ((canvas.0 / BLOOM_DOWNSAMPLE).max(1), (canvas.1 / BLOOM_DOWNSAMPLE).max(1));
        let material = |fragment: &str, uniforms: Vec<UniformDesc>| {
            load_material(
                ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment },
                MaterialParams { uniforms, ..Default::default() },
            )
            .expect("bloom shader")
        };
        Bloom {
            a: new_target(size),
            b: new_target(size),
            size,
            bright: material(
                BRIGHT_FRAGMENT,
                vec![
                    UniformDesc::new("Texel", UniformType::Float2),
                    UniformDesc::new("Threshold", UniformType::Float1),
                ],
            ),
            blur: material(BLUR_FRAGMENT, vec![UniformDesc::new("Dir", UniformType::Float2)]),
        }
    }

    /// Renders the glow of `source` (the canvas, `canvas` pixels) into `a`.
    fn render(&self, source: &Texture2D, canvas: (u32, u32)) {
        self.bright.set_uniform("Texel", vec2(1.0 / canvas.0 as f32, 1.0 / canvas.1 as f32));
        self.bright.set_uniform("Threshold", BLOOM_THRESHOLD);
        pass(&self.a, source, &self.bright);
        let texel = vec2(1.0 / self.size.0 as f32, 1.0 / self.size.1 as f32);
        // Two blur rounds, the second twice as wide, for a soft falloff.
        for spread in [1.0, 2.0] {
            self.blur.set_uniform("Dir", vec2(texel.x * spread, 0.0));
            pass(&self.b, &self.a.texture, &self.blur);
            self.blur.set_uniform("Dir", vec2(0.0, texel.y * spread));
            pass(&self.a, &self.b.texture, &self.blur);
        }
    }
}

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
    let blend = BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        dst,
    );
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
    view_w: f32,
}

impl Camera for OriginalCamera {
    fn matrix(&self) -> Mat4 {
        camera_matrix(self.view_w)
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
/// point `(dx, dy)` from the world centre, on a sphere `wide` times the
/// original's radius (1 is the original).
fn offset_z(dx: f32, dy: f32, wide: f32) -> f32 {
    let (r, r2) = (SPHERE_R * wide, SPHERE_R2 * wide * wide);
    let d2 = (dx * dx + dy * dy) as i32;
    if d2 < r2 as i32 {
        r - (r2 - d2 as f32).sqrt()
    } else {
        f32::INFINITY
    }
}

/// `Background::Background(base, decal, flipped)`: a curved grid centred on
/// its origin. Menus build it flipped (surface curving away), the game not.
///
/// `wide` is the view width over the original's 480. The grid gains
/// columns to fill a wide view, and the game's planet grows with it so its
/// limb stays off screen (the centre, and so the ground scroll, is
/// unchanged); cells past the sphere are left out.
fn backdrop_mesh(flipped: bool, wide: f32) -> Mesh {
    let sign = if flipped { -1.0 } else { 1.0 };
    // Growing faster than the width keeps the corners covered at 21:9.
    let sphere = if flipped { 1.0 } else { wide.powf(1.5) };
    let half_x = (DRAWN_HALF_X * wide).ceil() as i32 + (wide > 1.0) as i32;
    let cols = (2 * half_x + 1) as usize;
    let rows = (2 * DRAWN_HALF_Y + 1) as usize;
    let mut vertices = Vec::with_capacity(cols * rows);
    for ix in -half_x..=half_x {
        for iy in -DRAWN_HALF_Y..=DRAWN_HALF_Y {
            let x = ix as f32 * CELL;
            let y = iy as f32 * CELL;
            let z = sign * offset_z(-x, -y, sphere);
            // The menu's flipped surface takes the texture axes swapped
            // (measured against the emulator menu frame); the play-plane
            // mesh keeps the mapping used by the glued ground scroll.
            let (u, v) = if flipped {
                (y * TEXEL_PER_UNIT, x * TEXEL_PER_UNIT)
            } else {
                (x * TEXEL_PER_UNIT, -y * TEXEL_PER_UNIT)
            };
            vertices.push(Vertex::new(x, y, z, u, v, WHITE));
        }
    }
    let mut indices = Vec::new();
    for ix in 0..cols - 1 {
        for iy in 0..rows - 1 {
            let i0 = (ix * rows + iy) as u16;
            let i1 = ((ix + 1) * rows + iy) as u16;
            let quad = [i0, i1, i0 + 1, i1, i1 + 1, i0 + 1];
            if quad.iter().all(|&i| vertices[i as usize].position.z.is_finite()) {
                indices.extend_from_slice(&quad);
            }
        }
    }
    Mesh {
        vertices,
        indices,
        texture: None,
    }
}

/// Background::draw: the base layer's texture shift per unit of travel,
/// in 1024ths of the backdrop.
const ORIGINAL_SCROLL: f32 = 2.0;
/// GameScreen::draw: the planet sits this far behind the play plane.
pub const BACKGROUND_DEPTH: f32 = 100.0;

/// A base-layer scroll that keeps the ground under the rocks. The planet is
/// drawn BACKGROUND_DEPTH behind the play plane, so it shows at
/// EYE_Z / (EYE_Z + depth) of their scale; at the original's 2/1024 it
/// moved at 79% of their speed and rocks slid across it.
pub fn glued_scroll() -> f32 {
    let scale = EYE_Z / (EYE_Z + BACKGROUND_DEPTH);
    BACKDROP_SIZE * TEXEL_PER_UNIT / scale
}

/// macroquad draws at most this much per mesh; larger batches are split.
pub const MAX_BATCH_VERTICES: usize = 8000;
pub const MAX_BATCH_INDICES: usize = 4800;

pub struct Canvas {
    /// Rebuilt when the window's size or aspect changes.
    target: RefCell<RenderTarget>,
    target_size: Cell<(u32, u32)>,
    bloom: RefCell<Bloom>,
    /// Whether `end` adds the glow (Settings::bloom; MB_BLOOM=0 forces off).
    bloom_on: Cell<bool>,
    white: Texture2D,
    menu_backdrop: RefCell<Mesh>,
    game_backdrop: RefCell<Mesh>,
    backdrop_material: Material,
    additive: Material,
    flash: Material,
}

/// Resolution of the offscreen canvas relative to the original 480x320
/// framebuffer. The default matches the window's fitted canvas so the blit
/// is 1:1; MB_RENDER_SCALE pins it (1 keeps emulator comparisons exact).
fn render_scale(view_w: f32) -> f32 {
    if let Ok(v) = std::env::var("MB_RENDER_SCALE") {
        if let Ok(s) = v.parse::<f32>() {
            return s.clamp(1.0, 6.0);
        }
    }
    let fit = (screen_width() / view_w).min(screen_height() / CANVAS_H);
    fit.clamp(1.0, 6.0)
}

/// The offscreen target's pixel size for a view `view_w` wide.
fn target_size(view_w: f32) -> (u32, u32) {
    let scale = render_scale(view_w);
    ((FB_W * scale).round() as u32, (view_w * scale).round() as u32)
}

fn new_target((w, h): (u32, u32)) -> RenderTarget {
    let target = render_target(w, h);
    target.texture.set_filter(FilterMode::Linear);
    target
}

impl Canvas {
    pub fn new() -> Canvas {
        // Render at the window's canvas resolution instead of the original
        // 320x480 framebuffer, so the final blit is 1:1 rather than an
        // upscale. MB_RENDER_SCALE pins it (1 for pixel-perfect captures).
        let view_w = window_view_w();
        VIEW_W.store(view_w.to_bits(), Ordering::Relaxed);
        let size = target_size(view_w);
        let target = new_target(size);
        let backdrop_material = load_material(
            ShaderSource::Glsl {
                vertex: DEFAULT_VERTEX,
                fragment: BACKDROP_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                textures: vec!["Decal".to_owned()],
                uniforms: vec![
                    UniformDesc::new("Scroll0", UniformType::Float2),
                    UniformDesc::new("Scroll1", UniformType::Float2),
                ],
            },
        )
        .expect("backdrop shader");
        let additive = load_material(
            ShaderSource::Glsl {
                vertex: DEFAULT_VERTEX,
                fragment: TEXTURED_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::One),
                ..Default::default()
            },
        )
        .expect("additive shader");
        let flash = load_material(
            ShaderSource::Glsl {
                vertex: DEFAULT_VERTEX,
                fragment: FLASH_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                ..Default::default()
            },
        )
        .expect("flash shader");
        let white = Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]);
        Canvas {
            target: RefCell::new(target),
            target_size: Cell::new(size),
            bloom: RefCell::new(Bloom::new(size)),
            bloom_on: Cell::new(false),
            white,
            menu_backdrop: RefCell::new(backdrop_mesh(true, view_w / CANVAS_W)),
            game_backdrop: RefCell::new(backdrop_mesh(false, view_w / CANVAS_W)),
            backdrop_material,
            additive,
            flash,
        }
    }

    /// Turns the bloom pass on or off (MB_BLOOM=0 keeps it off).
    pub fn set_bloom(&self, on: bool) {
        self.bloom_on.set(on && !std::env::var("MB_BLOOM").is_ok_and(|v| v == "0"));
    }

    /// Runs `draw` with hit-flash shading (Texture::drawFlash).
    pub fn flashing(&self, draw: impl FnOnce()) {
        gl_use_material(&self.flash);
        draw();
        gl_use_default_material();
    }

    /// Where the landscape view lands in the window: the largest fit,
    /// centred.
    fn placement() -> Rect {
        let view_w = view_w();
        let scale = (screen_width() / view_w).min(screen_height() / CANVAS_H);
        let (w, h) = (view_w * scale, CANVAS_H * scale);
        Rect::new(
            (screen_width() - w) * 0.5,
            (screen_height() - h) * 0.5,
            w,
            h,
        )
    }

    /// Mouse position in canvas coordinates (x runs past 0..480 in a wide
    /// view).
    pub fn mouse() -> Vec2 {
        let r = Canvas::placement();
        let (mx, my) = mouse_position();
        vec2((mx - r.x) / r.w * view_w() - edge(), (my - r.y) / r.h * CANVAS_H)
    }

    /// Starts a frame: follows the window's aspect and size, then targets
    /// the offscreen canvas.
    pub fn begin(&self) {
        let view_w = window_view_w();
        VIEW_W.store(view_w.to_bits(), Ordering::Relaxed);
        let size = target_size(view_w);
        if size != self.target_size.get() {
            *self.target.borrow_mut() = new_target(size);
            self.target_size.set(size);
            *self.bloom.borrow_mut() = Bloom::new(size);
            *self.menu_backdrop.borrow_mut() = backdrop_mesh(true, view_w / CANVAS_W);
            *self.game_backdrop.borrow_mut() = backdrop_mesh(false, view_w / CANVAS_W);
        }
        set_camera(&OriginalCamera {
            render_pass: self.target.borrow().render_pass.clone(),
            view_w,
        });
        clear_background(BLACK);
    }

    /// Blit the portrait target as the landscape image. Landscape (x, y) is
    /// portrait (320-y, 480-x); the target texture's v axis runs bottom-up.
    pub fn end(&self) {
        let bloom = self.bloom_on.get();
        if bloom {
            self.bloom.borrow().render(&self.target.borrow().texture, self.target_size.get());
        }
        set_default_camera();
        clear_background(BLACK);
        let r = Canvas::placement();
        let view_w = view_w();
        let corner = |x: f32, y: f32| {
            let (lx, ly) = (x * view_w, y * CANVAS_H);
            Vertex::new(
                r.x + x * r.w,
                r.y + y * r.h,
                0.0,
                (FB_W - ly) / FB_W,
                (view_w - lx) / view_w,
                WHITE,
            )
        };
        let quad = Mesh {
            vertices: vec![
                corner(0.0, 0.0),
                corner(1.0, 0.0),
                corner(1.0, 1.0),
                corner(0.0, 1.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(self.target.borrow().texture.clone()),
        };
        draw_mesh(&quad);
        if bloom {
            let glow = Mesh {
                vertices: quad
                    .vertices
                    .iter()
                    .map(|v| Vertex { color: [255, 255, 255, (BLOOM_STRENGTH * 255.0) as u8], ..*v })
                    .collect(),
                indices: quad.indices.clone(),
                texture: Some(self.bloom.borrow().a.texture.clone()),
            };
            gl_use_material(&self.additive);
            draw_mesh(&glow);
            gl_use_default_material();
        }
    }

    /// `Background::draw` at `origin`: the base layer scrolls at 2x the
    /// background offset and the decal at 3x, in texture space.
    pub fn draw_backdrop(
        &self,
        menu: bool,
        base: &Texture2D,
        decal: &Texture2D,
        origin: Vec3,
        scroll: Vec2,
        alpha: f32,
    ) {
        let source = if menu {
            self.menu_backdrop.borrow()
        } else {
            self.game_backdrop.borrow()
        };
        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        let vertices = source
            .vertices
            .iter()
            .map(|v| Vertex {
                position: v.position + origin,
                color: [255, 255, 255, a],
                ..*v
            })
            .collect();
        // The menu mesh takes the texture axes swapped, so its scroll
        // components swap with them; the play-plane mesh keeps x/y.
        let wrap = |k: f32| {
            let (sx, sy) = if menu {
                (scroll.y, scroll.x)
            } else {
                (scroll.x, scroll.y)
            };
            vec2(
                (sx * k).rem_euclid(BACKDROP_SIZE) / BACKDROP_SIZE,
                (sy * k).rem_euclid(BACKDROP_SIZE) / BACKDROP_SIZE,
            )
        };
        let mesh = Mesh {
            vertices,
            indices: source.indices.clone(),
            texture: Some(base.clone()),
        };
        self.backdrop_material.set_texture("Decal", decal.clone());
        // Background::draw scrolls the base layer by 2/1024 and the cloud
        // decal by 3/1024 per unit. In game the base is set to keep pace
        // with the play plane instead (see `glued_scroll`); clouds keep
        // their 1.5x lead over it.
        // The mesh's v runs against y (GL textures start at the bottom), so
        // in game the vertical scroll goes the other way for the ground to
        // move opposite the ship, as it does horizontally.
        let k = if menu { ORIGINAL_SCROLL } else { glued_scroll() };
        let flip = |v: Vec2| if menu { v } else { vec2(v.x, (1.0 - v.y).fract()) };
        self.backdrop_material.set_uniform("Scroll0", flip(wrap(k)));
        self.backdrop_material
            .set_uniform("Scroll1", flip(wrap(k * 1.5)));
        gl_use_material(&self.backdrop_material);
        draw_mesh(&mesh);
        gl_use_default_material();
    }

    /// Coloured triangles, textured or plain, alpha-blended or additive
    /// (`GL_SRC_ALPHA, GL_ONE`). Keep batches within MAX_BATCH_*.
    pub fn draw_triangles(
        &self,
        vertices: Vec<Vertex>,
        indices: Vec<u16>,
        texture: Option<&Texture2D>,
        additive: bool,
    ) {
        if indices.is_empty() {
            return;
        }
        let texture = texture.unwrap_or(&self.white).clone();
        if additive {
            gl_use_material(&self.additive);
        }
        draw_mesh(&Mesh {
            vertices,
            indices,
            texture: Some(texture),
        });
        if additive {
            gl_use_default_material();
        }
    }

    /// A plain rectangle at depth 0 (Texture::fillRect).
    pub fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.draw_triangles(
            vec![
                Vertex::new(x, y, 0.0, 0.0, 0.0, color),
                Vertex::new(x + w, y, 0.0, 1.0, 0.0, color),
                Vertex::new(x + w, y + h, 0.0, 1.0, 1.0, color),
                Vertex::new(x, y + h, 0.0, 0.0, 1.0, color),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
            false,
        );
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
        draw_mesh(&Mesh {
            vertices,
            indices,
            texture: Some(tex.clone()),
        });
        gl_use_default_material();
    }
}

/// Resolution multiplier of the loaded art. The HD path uses the 4x assets
/// generated by `tools/upscale4x.py`; `MB_HD=0` loads the originals.
/// The web build keeps the originals: the 4x set is ~78 MB to download.
pub fn asset_scale() -> f32 {
    #[cfg(target_arch = "wasm32")]
    const DEFAULT_SCALE: f32 = 1.0;
    #[cfg(not(target_arch = "wasm32"))]
    const DEFAULT_SCALE: f32 = 4.0;
    static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| match std::env::var("MB_HD").as_deref() {
        Ok("0") => 1.0,
        Ok("1") => 4.0,
        _ => DEFAULT_SCALE,
    })
}

/// Logical (original-resolution) size of an art texture. The HD copies are
/// `asset_scale` times larger, so layout code divides them back down before
/// centring or spacing them.
pub fn tex_size(tex: &Texture2D) -> (f32, f32) {
    let s = asset_scale();
    (tex.width() / s, tex.height() / s)
}

/// `Texture::draw(x, y, scale)`: a top-left anchored quad at depth `z`,
/// modulated by the current `DrawManager` colour.
pub fn draw_quad(tex: &Texture2D, x: f32, y: f32, scale: f32, z: f32, color: Color) {
    let (w, h) = tex_size(tex);
    let (w, h) = (w * scale, h * scale);
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
    let (w, h) = tex_size(tex);
    let (hw, hh) = (w * 0.5, h * 0.5);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let corner = |dx: f32, dy: f32, u: f32, v: f32| {
        Vertex::new(x + dx * c - dy * s, y + dx * s + dy * c, z, u, v, color)
    };
    let quad = Mesh {
        vertices: vec![
            corner(-hw, -hh, 0.0, 0.0),
            corner(hw, -hh, 1.0, 0.0),
            corner(hw, hh, 1.0, 1.0),
            corner(-hw, hh, 0.0, 1.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(tex.clone()),
    };
    draw_mesh(&quad);
}

/// `Texture::drawMultipleFrames` for one sprite: cell `index` of a
/// `cols` x `rows` atlas, centred on `(x, y)` and rotated by `angle_deg`.
pub fn draw_atlas_sprite(
    tex: &Texture2D,
    cols: usize,
    rows: usize,
    index: usize,
    x: f32,
    y: f32,
    angle_deg: f32,
    z: f32,
    color: Color,
) {
    let (cw, ch) = (1.0 / cols as f32, 1.0 / rows as f32);
    let (u0, v0) = (
        (index % cols) as f32 * cw,
        (index / cols % rows) as f32 * ch,
    );
    let (tw, th) = tex_size(tex);
    let (hw, hh) = (tw * cw * 0.5, th * ch * 0.5);
    let (s, c) = angle_deg.to_radians().sin_cos();
    let corner = |dx: f32, dy: f32, u: f32, v: f32| {
        Vertex::new(x + dx * c - dy * s, y + dx * s + dy * c, z, u, v, color)
    };
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

#[cfg(test)]
mod backdrop_tests {
    use super::*;

    /// On-screen length of a 1-unit step at depth `z` near the canvas centre.
    fn step(z: f32) -> f32 {
        step_in(z, CANVAS_W)
    }

    fn step_in(z: f32, view_w: f32) -> f32 {
        let m = camera_matrix(view_w);
        let c = vec3(CANVAS_W * 0.5, CANVAS_H * 0.5, z);
        let p = |v: Vec3| {
            let h = m * v.extend(1.0);
            vec2(h.x / h.w, h.y / h.w)
        };
        // In clip units; scale to canvas units of this view.
        (p(c + vec3(1.0, 0.0, 0.0)) - p(c)).length() * view_w / 2.0
    }

    /// A wider view keeps the original's play-plane scale (1.0017 canvas
    /// units per unit: its eye distance and field of view nearly agree).
    #[test]
    fn wide_views_keep_the_scale() {
        let original = step_in(0.0, CANVAS_W);
        for w in [568.9, 768.0] {
            assert!((step_in(0.0, w) / original - 1.0).abs() < 1e-4, "{} {}", w, step_in(0.0, w));
        }
    }

    /// The glued scroll moves the ground at the rocks' speed on screen, at
    /// the centre of the view.
    #[test]
    fn ground_keeps_pace_with_the_play_plane() {
        let ratio = step(BACKGROUND_DEPTH) / step(0.0);
        let ground_units_per_unit = glued_scroll() / BACKDROP_SIZE / TEXEL_PER_UNIT;
        assert!((ground_units_per_unit * ratio - 1.0).abs() < 1e-4, "{}", ground_units_per_unit * ratio);
    }
}
