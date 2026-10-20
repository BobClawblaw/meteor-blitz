//! Original-layout menus, rendered the way the iPhone build rendered them.
//!
//! The original draws everything, menus included, through one perspective
//! camera into a portrait 320x480 framebuffer and rotates the world into
//! landscape with a fixed matrix (`Director::initWithView`). Layers sit at
//! different depths: the curved space backdrop at z=100, the horizon image
//! at z=0, the screen's own content at z=150 (`BasicScreen::draw`). This
//! module reproduces that pipeline, then blits the portrait target onto the
//! window as a 480x320 landscape image.

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

// BasicScreen: backdrop mesh origin and depth, horizon scale, screen depth.
const BACKDROP_X: f32 = 240.0;
const BACKDROP_Y: f32 = 155.0;
const BACKDROP_Z: f32 = 100.0;
const HORIZON_SCALE: f32 = 1.25;

// BasicScreen::update: screens fade in at 6/s while flying from z=150 to 0
// at 900/s, fade out at -6/s while flying on; the backdrop scrolls 50/s.
const SCREEN_START_Z: f32 = 150.0;
const SCREEN_Z_SPEED: f32 = -900.0;
const FADE_IN_SPEED: f32 = 6.0;
const FADE_OUT_SPEED: f32 = -6.0;
const BACKDROP_SCROLL: f32 = 50.0;

// Background: a 16x16 grid of 64-unit cells (1024x1024), texture
// coordinates at 0.002 per unit, only the inner cells drawn.
const CELL: f32 = 64.0;
const GRID: usize = 16;
const TOTAL: f32 = CELL * GRID as f32;
const TEXEL_PER_UNIT: f32 = 0.002;
const DRAWN_X: std::ops::RangeInclusive<usize> = 1..=14;
const DRAWN_Y: std::ops::RangeInclusive<usize> = 3..=12;

// World::initOffsetZs: the surface is a sphere of radius 700 (z = 700 -
// sqrt(700^2 - d^2)), looked up by integer squared distance.
const SPHERE_R2: f32 = 490_000.0;
const SPHERE_R: f32 = 700.0;

pub struct MenuAssets {
    pub space: Texture2D,
    pub fog: Texture2D,
    pub horizon: Texture2D,
    pub title: Texture2D,
    pub buttons: [(Texture2D, Texture2D); 4],
}

/// Main menu buttons, in the order the original constructs them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainButton {
    Play,
    HighScores,
    Settings,
    Help,
}

/// Button rects on the 480x320 canvas, from `MainMenuScreen` (x, y, w, h).
const MAIN_BUTTONS: [(MainButton, Rect); 4] = [
    (MainButton::Play, Rect { x: 160.0, y: 176.0, w: 160.0, h: 128.0 }),
    (MainButton::HighScores, Rect { x: 16.0, y: 176.0, w: 128.0, h: 128.0 }),
    (MainButton::Settings, Rect { x: 336.0, y: 176.0, w: 127.0, h: 57.0 }),
    (MainButton::Help, Rect { x: 336.0, y: 246.0, w: 127.0, h: 57.0 }),
];

/// `BasicScreen` state: the fade/fly-in on entry and fade-out on exit.
pub struct ScreenFx {
    state: u8,
    pub alpha: f32,
    pub z: f32,
    /// Fade the backdrop in with the content (true when coming from the
    /// splash or the game; menu-to-menu keeps the backdrop solid).
    fade_backdrop: bool,
}

impl ScreenFx {
    pub fn enter(fade_backdrop: bool) -> ScreenFx {
        ScreenFx { state: 1, alpha: 0.0, z: SCREEN_START_Z, fade_backdrop }
    }

    pub fn update(&mut self, dt: f32, pointer_down: bool) {
        match self.state {
            1 => {
                self.alpha += dt * FADE_IN_SPEED;
                self.z = (self.z + dt * SCREEN_Z_SPEED).max(0.0);
                if self.alpha >= 1.0 {
                    self.alpha = 1.0;
                    self.z = 0.0;
                    self.state = 2;
                }
            }
            2 => {
                if !pointer_down {
                    self.state = 3;
                }
            }
            7 => {
                self.z += dt * SCREEN_Z_SPEED;
                self.alpha += dt * FADE_OUT_SPEED;
                if self.alpha <= 0.0 {
                    self.alpha = 0.0;
                    self.state = 0;
                }
            }
            _ => {}
        }
    }

    /// Buttons only respond in the ready state.
    pub fn ready(&self) -> bool {
        self.state == 3
    }

    pub fn leave(&mut self) {
        if self.state == 3 {
            self.state = 7;
        }
    }

    pub fn done(&self) -> bool {
        self.state == 0
    }

    fn backdrop_color(&self) -> Color {
        let a = if self.fade_backdrop { self.alpha } else { 1.0 };
        Color::new(1.0, 1.0, 1.0, a)
    }

    fn content_color(&self) -> Color {
        Color::new(1.0, 1.0, 1.0, self.alpha)
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

/// `Background::Background(space, fog, flipped = true)`: vertices in grid
/// space (origin at the mesh centre), z negated, UVs scaled.
fn backdrop_mesh(space: &Texture2D) -> Mesh {
    let mut vertices = Vec::with_capacity((GRID + 1) * (GRID + 1));
    for ix in 0..=GRID {
        for iy in 0..=GRID {
            let x = ix as f32 * CELL - TOTAL * 0.5;
            let y = iy as f32 * CELL - TOTAL * 0.5;
            let z = -offset_z(-x, -y);
            vertices.push(Vertex::new(x, y, z, x * TEXEL_PER_UNIT, y * TEXEL_PER_UNIT, WHITE));
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
    Mesh { vertices, indices, texture: Some(space.clone()) }
}

/// Texture unit 1 was GL_DECAL: fog over space by the fog's alpha, alpha
/// from the base layer. Both layers repeat.
const BACKDROP_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Fog;
uniform vec2 Scroll0;
uniform vec2 Scroll1;
void main() {
    vec4 space = texture2D(Texture, fract(uv + Scroll0));
    vec4 fog = texture2D(Fog, fract(uv + Scroll1));
    gl_FragColor = vec4(mix(space.rgb, fog.rgb, fog.a) * color.rgb, space.a * color.a);
}"#;

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

fn alpha_blend() -> PipelineParams {
    let blend = BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
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

pub struct Canvas {
    target: RenderTarget,
    backdrop: Mesh,
    backdrop_material: Material,
    /// `Background` scroll offset (x, y); layers scroll at 2x and 3x of it.
    scroll: Vec2,
}

impl Canvas {
    pub fn new(assets: &MenuAssets) -> Canvas {
        let target = render_target(FB_W as u32, FB_H as u32);
        target.texture.set_filter(FilterMode::Linear);
        let backdrop_material = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: BACKDROP_FRAGMENT },
            MaterialParams {
                pipeline_params: alpha_blend(),
                textures: vec!["Fog".to_owned()],
                uniforms: vec![UniformDesc::new("Scroll0", UniformType::Float2), UniformDesc::new("Scroll1", UniformType::Float2)],
            },
        )
        .expect("backdrop shader");
        backdrop_material.set_texture("Fog", assets.fog.clone());
        Canvas { target, backdrop: backdrop_mesh(&assets.space), backdrop_material, scroll: Vec2::ZERO }
    }

    /// `BasicScreen::update`: the menu backdrop drifts along x.
    pub fn update(&mut self, dt: f32) {
        self.scroll.x += dt * BACKDROP_SCROLL;
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
}

/// `Texture::draw(x, y, scale)`: a top-left anchored quad at depth `z`,
/// modulated by the current `DrawManager` colour.
fn draw_quad(tex: &Texture2D, x: f32, y: f32, scale: f32, z: f32, color: Color) {
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

/// `BasicScreen::draw` up to the screen's own content: the curved backdrop
/// at (240, 155, 100), then the horizon at the origin, scale 1.25.
pub fn draw_backdrop(canvas: &Canvas, assets: &MenuAssets, fx: &ScreenFx) {
    let color = fx.backdrop_color();
    let mut mesh = Mesh {
        vertices: canvas.backdrop.vertices.clone(),
        indices: canvas.backdrop.indices.clone(),
        texture: canvas.backdrop.texture.clone(),
    };
    for v in &mut mesh.vertices {
        v.position += vec3(BACKDROP_X, BACKDROP_Y, BACKDROP_Z);
        v.color = [255, 255, 255, (color.a * 255.0) as u8];
    }
    let wrap = |k: f32| vec2((canvas.scroll.x * k).rem_euclid(TOTAL) / TOTAL, (canvas.scroll.y * k).rem_euclid(TOTAL) / TOTAL);
    canvas.backdrop_material.set_uniform("Scroll0", wrap(2.0));
    canvas.backdrop_material.set_uniform("Scroll1", wrap(3.0));
    gl_use_material(&canvas.backdrop_material);
    draw_mesh(&mesh);
    gl_use_default_material();
    draw_quad(&assets.horizon, 0.0, 0.0, HORIZON_SCALE, 0.0, color);
}

/// The main menu button released this frame, if any.
pub fn main_menu_click() -> Option<MainButton> {
    if !is_mouse_button_released(MouseButton::Left) {
        return None;
    }
    let mouse = Canvas::mouse();
    MAIN_BUTTONS.iter().find(|(_, rect)| rect.contains(mouse)).map(|(button, _)| *button)
}

/// `MainMenuScreen::drawStateReady`: title centred at y=15, then the buttons.
pub fn draw_main_menu(canvas: &Canvas, assets: &MenuAssets, fx: &ScreenFx) {
    draw_backdrop(canvas, assets, fx);
    let color = fx.content_color();
    draw_quad(&assets.title, ((CANVAS_W - assets.title.width()) * 0.5).floor(), 15.0, 1.0, fx.z, color);
    let mouse = Canvas::mouse();
    for (i, (_, rect)) in MAIN_BUTTONS.iter().enumerate() {
        let (up, down) = &assets.buttons[i];
        let held = fx.ready() && rect.contains(mouse) && is_mouse_button_down(MouseButton::Left);
        draw_quad(if held { down } else { up }, rect.x, rect.y, 1.0, fx.z, color);
    }
}
