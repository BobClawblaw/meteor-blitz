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

/// Target pixels per canvas unit, set by `Canvas::begin`: text rasterised
/// at this size stays sharp.
static PIXEL_SCALE: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0

pub fn pixel_scale() -> f32 {
    f32::from_bits(PIXEL_SCALE.load(Ordering::Relaxed))
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

/// Settings::resolution's height, set each frame before `Canvas::begin`.
/// 0 follows the window's fitted resolution.
static RENDER_HEIGHT: AtomicU32 = AtomicU32::new(0);

pub fn set_render_height(height: u32) {
    RENDER_HEIGHT.store(height, Ordering::Relaxed);
}

/// macOS window frames are in points and include the title bar; the
/// backing scale turns points into pixels. Returns the title bar height in
/// points so a requested content size can be converted to a frame size.
#[cfg(target_os = "macos")]
fn title_bar_height() -> f32 {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSRect;
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return 0.0;
        }
        let mut window: *mut AnyObject = msg_send![app, mainWindow];
        if window.is_null() {
            window = msg_send![app, keyWindow];
        }
        if window.is_null() {
            let windows: *mut AnyObject = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            if count > 0 {
                window = msg_send![windows, objectAtIndex: 0usize];
            }
        }
        if window.is_null() {
            return 0.0;
        }
        let frame: NSRect = msg_send![window, frame];
        let content: NSRect = msg_send![window, contentLayoutRect];
        (frame.size.height - content.size.height).max(0.0) as f32
    }
}

/// The display's backing scale. The window's own `backingScaleFactor` is
/// still 1 before the window is shown, which made startup resolution
/// changes half-size; NSScreen reports the real scale at that point.
#[cfg(target_os = "macos")]
fn display_scale() -> f32 {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    unsafe {
        let screen: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
        if screen.is_null() {
            return screen_dpi_scale();
        }
        let scale: f64 = msg_send![screen, backingScaleFactor];
        if scale > 0.0 {
            scale as f32
        } else {
            screen_dpi_scale()
        }
    }
}

/// Sets the window to a physical resolution. macOS window frames are in
/// points, so the physical size is divided by the backing scale and the
/// title bar is added; Windows and X11 take client pixels directly.
pub fn set_window_resolution(width: u32, height: u32) {
    #[cfg(target_os = "macos")]
    let scale = display_scale();
    #[cfg(not(target_os = "macos"))]
    let scale = 1.0;
    #[cfg(target_os = "macos")]
    let title = title_bar_height();
    #[cfg(not(target_os = "macos"))]
    let title = 0.0;
    let width = (width as f32 / scale).round().max(1.0) as u32;
    let height = (height as f32 / scale + title).round().max(1.0) as u32;
    macroquad::miniquad::window::set_window_size(width, height);
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

/// 13-tap energy-preserving downsample (Jim Jimenez / COD) for the bloom
/// pyramid: suppresses single-pixel fireflies while staying smooth.
const BLOOM_DOWN_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform vec2 Texel;
void main() {
    vec2 t = Texel;
    vec3 a = texture2D(Texture, uv + vec2(-2.0, -2.0) * t).rgb;
    vec3 b = texture2D(Texture, uv + vec2( 0.0, -2.0) * t).rgb;
    vec3 c = texture2D(Texture, uv + vec2( 2.0, -2.0) * t).rgb;
    vec3 d = texture2D(Texture, uv + vec2(-2.0,  0.0) * t).rgb;
    vec3 e = texture2D(Texture, uv).rgb;
    vec3 f = texture2D(Texture, uv + vec2( 2.0,  0.0) * t).rgb;
    vec3 g = texture2D(Texture, uv + vec2(-2.0,  2.0) * t).rgb;
    vec3 h = texture2D(Texture, uv + vec2( 0.0,  2.0) * t).rgb;
    vec3 i = texture2D(Texture, uv + vec2( 2.0,  2.0) * t).rgb;
    vec3 j = texture2D(Texture, uv + vec2(-1.0, -1.0) * t).rgb;
    vec3 k = texture2D(Texture, uv + vec2( 1.0, -1.0) * t).rgb;
    vec3 l = texture2D(Texture, uv + vec2(-1.0,  1.0) * t).rgb;
    vec3 m = texture2D(Texture, uv + vec2( 1.0,  1.0) * t).rgb;
    vec3 col = e * 0.125;
    col += (j + k + l + m) * 0.125;
    col += (b + d + f + h) * 0.0625;
    col += (a + c + g + i) * 0.03125;
    gl_FragColor = vec4(col, 1.0);
}"#;

/// 9-tap tent-filter upsample, additively blended onto the next larger mip
/// in the bloom pyramid to build a tight core plus wide velvety halo.
const BLOOM_UP_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform vec2 Texel;
uniform float Weight;
void main() {
    vec2 t = Texel * 1.25;
    vec3 c = texture2D(Texture, uv).rgb * 4.0;
    c += (texture2D(Texture, uv + vec2(-t.x, 0.0)).rgb
        + texture2D(Texture, uv + vec2( t.x, 0.0)).rgb
        + texture2D(Texture, uv + vec2(0.0, -t.y)).rgb
        + texture2D(Texture, uv + vec2(0.0,  t.y)).rgb) * 2.0;
    c +=  texture2D(Texture, uv + vec2(-t.x, -t.y)).rgb
        + texture2D(Texture, uv + vec2( t.x, -t.y)).rgb
        + texture2D(Texture, uv + vec2(-t.x,  t.y)).rgb
        + texture2D(Texture, uv + vec2( t.x,  t.y)).rgb;
    gl_FragColor = vec4((c / 16.0) * Weight, 1.0);
}"#;

/// Plain additive bloom composite for screens without post-processing.
const BLOOM_ADD_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying lowp vec2 uv;
uniform sampler2D Texture;
uniform float Strength;
void main() {
    gl_FragColor = vec4(texture2D(Texture, uv).rgb * Strength, 1.0);
}"#;

/// Bloom pyramid configuration: 5 mips from half-res down to 1/32-res.
const BLOOM_MIPS: usize = 5;
const BLOOM_STRENGTH: f32 = 0.65;
const BLOOM_UP_WEIGHT: f32 = 0.68;

/// The affine map from clip space back to the landscape canvas on the play
/// plane (z = 0), as rows of a 2x2 matrix and an offset.
fn ndc_to_canvas(view_w: f32, zoom: f32) -> (Vec4, Vec2) {
    let m = view_matrix(view_w, zoom, Vec2::ZERO);
    let ndc = |x: f32, y: f32| {
        let h = m * vec4(x, y, 0.0, 1.0);
        vec2(h.x / h.w, h.y / h.w)
    };
    let o = ndc(0.0, 0.0);
    let ex = ndc(1.0, 0.0) - o;
    let ey = ndc(0.0, 1.0) - o;
    // [ex ey] maps canvas to ndc; invert it.
    let inv = Mat2::from_cols(ex, ey).inverse();
    let row0 = vec2(inv.x_axis.x, inv.y_axis.x);
    let row1 = vec2(inv.x_axis.y, inv.y_axis.y);
    (vec4(row0.x, row0.y, row1.x, row1.y), -(inv * o))
}

/// The camera with the play plane zoomed out `zoom` times about the
/// canvas centre (Options > Playfield zoom; 1 is the original) and moved by
/// `shift` canvas units first (a copy of the wrapping world). Depth scales
/// too, so the planet's curve and the layers' parallax shrink with the rest
/// of the picture.
fn view_matrix(view_w: f32, zoom: f32, shift: Vec2) -> Mat4 {
    let c = vec3(CANVAS_W * 0.5, CANVAS_H * 0.5, 0.0);
    camera_matrix(view_w)
        * Mat4::from_translation(c)
        * Mat4::from_scale(Vec3::splat(1.0 / zoom))
        * Mat4::from_translation(shift.extend(0.0) - c)
}

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

/// Glow from emitters (port addition): what the world pass drew additively
/// (shots, flames, sparks, explosions), plus laser bolts and tower beams,
/// rendered into a half-resolution target and filtered through a 5-level
/// progressive downsample/upsample pyramid (1/2 .. 1/32) for both a crisp
/// hot core and a wide velvety atmospheric halo. Clouds and HUD never glow.
struct Bloom {
    mips: Vec<(RenderTarget, (u32, u32))>,
    down: Material,
    up: Material,
    add: Material,
}

impl Bloom {
    fn new(canvas: (u32, u32)) -> Bloom {
        let mut mips = Vec::with_capacity(BLOOM_MIPS);
        let (mut w, mut h) = ((canvas.0 / 2).max(1), (canvas.1 / 2).max(1));
        for _ in 0..BLOOM_MIPS {
            mips.push((new_target((w, h)), (w, h)));
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        let add_blend = BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::One,
        );
        let add_pipe = PipelineParams {
            color_blend: Some(add_blend),
            alpha_blend: Some(add_blend),
            cull_face: CullFace::Nothing,
            depth_test: Comparison::Always,
            depth_write: false,
            ..Default::default()
        };
        let down = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: BLOOM_DOWN_FRAGMENT },
            MaterialParams {
                uniforms: vec![UniformDesc::new("Texel", UniformType::Float2)],
                ..Default::default()
            },
        )
        .expect("bloom down shader");
        let up = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: BLOOM_UP_FRAGMENT },
            MaterialParams {
                pipeline_params: add_pipe,
                uniforms: vec![
                    UniformDesc::new("Texel", UniformType::Float2),
                    UniformDesc::new("Weight", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .expect("bloom up shader");
        let add = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: BLOOM_ADD_FRAGMENT },
            MaterialParams {
                pipeline_params: add_pipe,
                uniforms: vec![UniformDesc::new("Strength", UniformType::Float1)],
                ..Default::default()
            },
        )
        .expect("bloom add shader");
        Bloom { mips, down, up, add }
    }

    fn texture(&self) -> Texture2D {
        self.mips[0].0.texture.clone()
    }

    /// Redraws the emitters into mip 0, downsamples through the pyramid,
    /// then tent-upsamples additively back to mip 0.
    fn render(&self, emitters: &[Mesh], view_w: f32, zoom: f32, additive: &Material) {
        set_camera(&OriginalCamera {
            render_pass: self.mips[0].0.render_pass.clone(),
            view_w,
            zoom,
            shift: Vec2::ZERO,
        });
        clear_background(BLACK);
        gl_use_material(additive);
        for m in emitters {
            draw_mesh(m);
        }
        gl_use_default_material();
        // Progressive 13-tap downsample.
        for i in 0..self.mips.len() - 1 {
            let (src, (sw, sh)) = &self.mips[i];
            let (dst, _) = &self.mips[i + 1];
            self.down.set_uniform("Texel", vec2(1.0 / *sw as f32, 1.0 / *sh as f32));
            pass(dst, &src.texture, &self.down);
        }
        // Progressive 9-tap tent upsample, accumulating onto each larger mip.
        self.up.set_uniform("Weight", BLOOM_UP_WEIGHT);
        for i in (1..self.mips.len()).rev() {
            let (src, (sw, sh)) = &self.mips[i];
            let (dst, _) = &self.mips[i - 1];
            self.up.set_uniform("Texel", vec2(1.0 / *sw as f32, 1.0 / *sh as f32));
            pass(dst, &src.texture, &self.up);
        }
    }
}

/// Texture unit 1 was GL_DECAL: the decal layer over the base by the
/// decal's alpha, alpha from the base. Both layers repeat and scroll.
///
/// Port addition: with Lit on, the planet surface gets normal-mapped relief,
/// cloud drop shadows, world-specific materials (ocean specular, pulsing
/// molten lava, crater rim lighting, atmospheric Fresnel limb haze), a
/// multi-layer twinkling parallax starfield in space/menus, and up to
/// MAX_LIGHTS dynamic point lights illuminating both terrain and clouds.
const BACKDROP_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying highp vec2 uv;
varying highp vec2 canvas;
varying highp vec2 view_ndc;
uniform sampler2D Texture;
uniform sampler2D Decal;
uniform sampler2D Normal;
uniform vec2 Scroll0;
uniform vec2 Scroll1;
uniform float Lit;
uniform float Time;
uniform float WorldStyle;
uniform vec3 LightPos[12];
uniform vec3 LightColor[12];

float hash21(vec2 p) {
    p = fract(p * vec2(234.34, 435.345));
    p += dot(p, p + 34.23);
    return fract(p.x * p.y);
}

vec3 star_layer(vec2 p, float scale, float seed, float t) {
    vec2 g = p * scale;
    vec2 id = floor(g);
    vec2 f = fract(g) - 0.5;
    float h = hash21(id + seed);
    if (h < 0.87) {
        return vec3(0.0);
    }
    float hx = hash21(id * 1.37 + seed + 19.1) - 0.5;
    float hy = hash21(id * 2.13 + seed + 47.7) - 0.5;
    vec2 d = f - vec2(hx, hy) * 0.68;
    float dist = length(d);
    float bright = smoothstep(0.87, 1.0, h);
    float size = mix(0.045, 0.11, bright);
    float core = exp(-dist * dist / (size * size * 0.18));
    // Subtle diffraction cross on the brightest stars.
    float cross_glint = bright * bright
        * exp(-abs(d.x) * 42.0) * exp(-abs(d.y) * 9.0)
        + bright * bright * exp(-abs(d.y) * 42.0) * exp(-abs(d.x) * 9.0);
    float twinkle = 0.72 + 0.28 * sin(t * (1.8 + 4.2 * bright) + h * 90.0);
    float temp = hash21(id * 3.11 + seed + 7.3);
    vec3 tint = mix(vec3(0.68, 0.84, 1.0), vec3(1.0, 0.88, 0.72), temp);
    tint = mix(tint, vec3(1.0), 0.35);
    return tint * (core * (0.45 + 0.95 * bright) + cross_glint * 0.45) * twinkle;
}

void main() {
    // The textures repeat (main.rs `load_terrain`); a fract() wrap here
    // would select the smallest mipmap along the seam.
    vec2 uv0 = uv + Scroll0;
    vec2 uv1 = uv + Scroll1;
    vec4 base = texture2D(Texture, uv0);
    vec4 decal = texture2D(Decal, uv1);
    vec3 surface = mix(base.rgb, decal.rgb, decal.a);
    if (Lit > 0.5) {
        // World 6 = menu space backdrop: starfield + nebula depth without ground lights.
        if (WorldStyle > 5.5) {
            vec3 stars = vec3(0.0);
            stars += star_layer(uv + Scroll0 * 0.35, 22.0, 1.3, Time) * 0.65;
            stars += star_layer(uv + Scroll0 * 0.70, 14.0, 17.9, Time) * 0.95;
            stars += star_layer(uv + Scroll0 * 1.15,  9.0, 41.2, Time) * 1.25;
            surface += stars * (1.0 - decal.a * 0.45);
            gl_FragColor = vec4(surface * color.rgb, base.a * color.a);
            return;
        }
        // Texture u runs with canvas x and v against canvas y.
        vec3 t = texture2D(Normal, uv0).xyz * 2.0 - 1.0;
        vec3 n = normalize(vec3(t.x, -t.y, t.z));
        // The key light, from the upper left; flat ground stays as drawn.
        vec3 key = normalize(vec3(-0.45, -0.55, 0.7));
        vec3 view_dir = vec3(0.0, 0.0, 1.0);
        vec3 half_key = normalize(key + view_dir);
        float limb = clamp(length(view_ndc), 0.0, 1.0);
        float fresnel = pow(limb, 2.8);

        // Per-world terrain base preparation.
        vec3 ground = base.rgb;
        vec3 cloud = decal.rgb;
        float cloud_a = decal.a;
        float spec_mask = 0.0;
        float spec_power = 24.0;
        vec3 emissive = vec3(0.0);
        vec3 atmo = vec3(0.0);

        if (WorldStyle < 0.5) {
            // World 1 (Blue Earth): cloud drop shadows + ocean specular glint + sapphire limb haze.
            float shadow_a = texture2D(Decal, uv1 + vec2(0.011, -0.013)).a;
            ground *= 1.0 - shadow_a * 0.44;
            spec_mask = smoothstep(0.04, 0.20, base.b - max(base.r, base.g));
            spec_power = 28.0;
            atmo = vec3(0.20, 0.46, 0.98) * fresnel * 0.30;
        } else if (WorldStyle < 1.5) {
            // World 2 (Lava / Crust): animated glowing molten fissures + obsidian crust sheen.
            float lava_open = 1.0 - cloud_a;
            float wave = sin(Time * 2.1 + uv0.x * 28.0 + sin(uv0.y * 22.0 - Time * 1.4));
            float pulse = 0.86 + 0.22 * wave;
            float heat = smoothstep(0.18, 0.82, base.r) * lava_open;
            vec3 magma = mix(vec3(0.95, 0.16, 0.02), vec3(1.55, 0.90, 0.26), clamp(heat * pulse, 0.0, 1.0));
            emissive = magma * heat * 0.85;
            // Dark basalt crust gets crisp subtle specular catch.
            spec_mask = cloud_a * 0.35;
            spec_power = 20.0;
            atmo = vec3(0.75, 0.20, 0.05) * fresnel * 0.24;
        } else if (WorldStyle < 2.5) {
            // World 3 (Green Planet): cloud drop shadows + water glint + emerald-teal limb haze.
            float shadow_a = texture2D(Decal, uv1 + vec2(0.011, -0.013)).a;
            ground *= 1.0 - shadow_a * 0.42;
            spec_mask = smoothstep(0.02, 0.16, base.b - base.r) * (1.0 - smoothstep(0.32, 0.58, base.g));
            spec_power = 26.0;
            atmo = vec3(0.16, 0.68, 0.54) * fresnel * 0.26;
        } else if (WorldStyle < 3.5) {
            // World 4 (Red Craters): dramatic crater relief + dusty specular + crimson limb haze.
            spec_mask = 0.12;
            spec_power = 18.0;
            atmo = vec3(0.68, 0.14, 0.16) * fresnel * 0.25;
        } else if (WorldStyle < 4.5) {
            // World 5 (Green Craters): alien craters + emerald atmospheric rim.
            spec_mask = 0.14;
            spec_power = 18.0;
            atmo = vec3(0.14, 0.62, 0.32) * fresnel * 0.25;
        } else {
            // World 6 & Survival (Deep Space): multi-layer twinkling parallax starfield.
            vec3 stars = vec3(0.0);
            stars += star_layer(uv + Scroll0 * 0.35, 22.0, 1.3, Time) * 0.65;
            stars += star_layer(uv + Scroll0 * 0.70, 14.0, 17.9, Time) * 0.95;
            stars += star_layer(uv + Scroll0 * 1.15,  9.0, 41.2, Time) * 1.25;
            emissive = stars * (1.0 - cloud_a * 0.40);
        }

        // Relief lighting on the planet surface (clouds float above the relief).
        float relief_lo = (WorldStyle > 2.5 && WorldStyle < 4.5) ? 0.46 : 0.55;
        float relief_hi = (WorldStyle > 2.5 && WorldStyle < 4.5) ? 1.54 : 1.45;
        float relief = clamp(dot(n, key) / key.z, relief_lo, relief_hi);
        // Subtle key-light specular glint on water and rocky rims.
        float key_spec = pow(max(dot(n, half_key), 0.0), spec_power) * spec_mask * (1.0 - cloud_a * 0.85);

        vec3 light_ground = vec3(0.0);
        vec3 light_cloud = vec3(0.0);
        vec3 light_spec = vec3(0.0);
        for (int i = 0; i < 12; i++) {
            vec2 d = LightPos[i].xy - canvas;
            float r = LightPos[i].z;
            float fall = clamp(1.0 - length(d) / max(r, 1.0), 0.0, 1.0);
            float att = fall * fall;
            vec3 l = normalize(vec3(d, 26.0));
            float ndl = max(dot(n, l), 0.0);
            light_ground += LightColor[i] * att * (0.32 + 0.68 * ndl);
            light_cloud += LightColor[i] * att * 0.85;
            if (spec_mask > 0.01) {
                vec3 h = normalize(l + view_dir);
                light_spec += LightColor[i] * att * pow(max(dot(n, h), 0.0), spec_power);
            }
        }
        // Overlapping lights saturate softly instead of washing out.
        light_ground = light_ground / (1.0 + light_ground);
        light_cloud = light_cloud / (1.0 + light_cloud);
        light_spec = light_spec / (1.0 + light_spec);

        vec3 lit_ground = ground * relief
            + ground * light_ground * 2.8
            + light_ground * 0.16
            + vec3(0.95, 0.98, 1.0) * key_spec * 0.48
            + light_spec * spec_mask * 0.9;
        // Clouds/fog catch a softer upward/overhead illumination from explosions.
        vec3 lit_cloud = cloud * (0.94 + 0.14 * relief) + cloud * light_cloud * 1.45 + light_cloud * 0.14;
        surface = mix(lit_ground, lit_cloud, cloud_a) + emissive + atmo;
    }
    gl_FragColor = vec4(surface * color.rgb, base.a * color.a);
}"#;

/// The backdrop's vertex shader: the default one, plus the vertex's place
/// on the landscape canvas (NdcToCanvas maps clip space back to it) and
/// normalised screen coords for atmospheric Fresnel limb glow.
const BACKDROP_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying highp vec2 uv;
varying lowp vec4 color;
varying highp vec2 canvas;
varying highp vec2 view_ndc;
uniform mat4 Model;
uniform mat4 Projection;
uniform vec4 NdcToCanvas;
uniform vec2 NdcOffset;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    vec2 ndc = gl_Position.xy / gl_Position.w;
    canvas = vec2(dot(NdcToCanvas.xy, ndc), dot(NdcToCanvas.zw, ndc)) + NdcOffset;
    view_ndc = ndc;
    color = color0 / 255.0;
    uv = texcoord;
}"#;

/// Material classes encoded in vertex colour's blue channel when drawing
/// sprites with the per-pixel PBR-inspired `lit` shader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpriteMat {
    /// Rough rocky asteroid with strong bump relief and warm rim lighting.
    Rock,
    /// Crystalline ice asteroid with sharp specular glints and cyan Fresnel rim.
    Ice,
    /// Volcanic fire asteroid with glowing molten orange-gold fissures.
    Fire,
    /// Metallic starfighter, enemy hull, depot or boss armour.
    Craft,
    /// Glowing boss core, golden rings, and energy power-ups.
    Core,
}

impl SpriteMat {
    fn code(self) -> f32 {
        match self {
            SpriteMat::Rock => 0.05,
            SpriteMat::Ice => 0.25,
            SpriteMat::Fire => 0.45,
            SpriteMat::Craft => 0.68,
            SpriteMat::Core => 0.90,
        }
    }
}

/// Encodes a sprite's screen rotation and material class into vertex colour
/// for `LIT_FRAGMENT` (zero per-sprite uniform switches).
pub fn lit_color(angle_deg: f32, mat: SpriteMat, alpha: f32) -> Color {
    let (s, c) = angle_deg.to_radians().sin_cos();
    Color::new(c * 0.5 + 0.5, s * 0.5 + 0.5, mat.code(), alpha)
}

/// Per-pixel lit sprite vertex shader: forwards exact play-plane canvas
/// coordinates so every pixel receives directional point lights and relief.
const LIT_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying highp vec2 uv;
varying lowp vec4 color;
varying highp vec2 canvas;
uniform mat4 Model;
uniform mat4 Projection;
uniform vec4 NdcToCanvas;
uniform vec2 NdcOffset;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    vec2 ndc = gl_Position.xy / gl_Position.w;
    canvas = vec2(dot(NdcToCanvas.xy, ndc), dot(NdcToCanvas.zw, ndc)) + NdcOffset;
    color = color0 / 255.0;
    uv = texcoord;
}"#;

/// Per-pixel PBR-inspired sprite lighting (port addition): derives a
/// 3D dome + luminance-bump normal per pixel rotated into screen space,
/// shades with a key light, soft hemisphere fill, all 12 dynamic point
/// lights (diffuse + Blinn-Phong specular), material-aware Fresnel rim
/// lighting (ice glint, metallic sheen), and molten/core self-emissives.
const LIT_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying highp vec2 uv;
varying highp vec2 canvas;
uniform sampler2D Texture;
uniform vec2 AtlasGrid;
uniform vec3 LightPos[12];
uniform vec3 LightColor[12];

float luma(vec3 c) {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

void main() {
    vec4 t = texture2D(Texture, uv);
    if (t.a < 0.004) {
        discard;
    }
    // Reconstruct sprite rotation basis and material class from vertex colour.
    vec2 rot = color.rg * 2.0 - 1.0;
    float rlen = max(length(rot), 0.001);
    float cs = rot.x / rlen;
    float sn = rot.y / rlen;
    float mat = color.b;

    // Local coordinates within the sprite (or atlas cell) in [-1, 1].
    vec2 cell_idx = floor(uv * AtlasGrid);
    vec2 cell_uv = fract(uv * AtlasGrid);
    vec2 local = cell_uv * 2.0 - 1.0;

    // Clamped 4-tap luminance gradient inside the sprite's cell for crisp relief.
    vec2 eps = vec2(0.014);
    vec2 uv_l = (cell_idx + clamp(cell_uv + vec2(-eps.x, 0.0), 0.01, 0.99)) / AtlasGrid;
    vec2 uv_r = (cell_idx + clamp(cell_uv + vec2( eps.x, 0.0), 0.01, 0.99)) / AtlasGrid;
    vec2 uv_u = (cell_idx + clamp(cell_uv + vec2(0.0, -eps.y), 0.01, 0.99)) / AtlasGrid;
    vec2 uv_d = (cell_idx + clamp(cell_uv + vec2(0.0,  eps.y), 0.01, 0.99)) / AtlasGrid;
    vec4 sl = texture2D(Texture, uv_l);
    vec4 sr = texture2D(Texture, uv_r);
    vec4 su = texture2D(Texture, uv_u);
    vec4 sd = texture2D(Texture, uv_d);
    float hl = luma(sl.rgb) * sl.a;
    float hr = luma(sr.rgb) * sr.a;
    float hu = luma(su.rgb) * su.a;
    float hd = luma(sd.rgb) * sd.a;

    // Material parameters: bump strength, specular strength/power, Fresnel rim.
    float bump_k = 1.65;
    float spec_k = 0.22;
    float spec_p = 18.0;
    vec3 rim_col = vec3(0.55, 0.65, 0.80);
    float rim_k = 0.22;
    vec3 emissive = vec3(0.0);

    if (mat > 0.15 && mat < 0.35) {
        // Ice rock: crystalline specular glints + bright cyan Fresnel rim.
        bump_k = 2.0;
        spec_k = 0.72;
        spec_p = 36.0;
        rim_col = vec3(0.55, 0.92, 1.0);
        rim_k = 0.45;
    } else if (mat >= 0.35 && mat < 0.55) {
        // Fire rock: rugged volcanic bump + incandescent glowing magma fissures.
        bump_k = 1.9;
        spec_k = 0.26;
        spec_p = 20.0;
        rim_col = vec3(1.0, 0.45, 0.12);
        rim_k = 0.36;
        float hot = smoothstep(0.22, 0.62, t.r - t.b) * smoothstep(0.32, 0.85, t.r);
        emissive = vec3(1.15, 0.46, 0.08) * hot * 0.72;
    } else if (mat >= 0.55 && mat < 0.80) {
        // Metallic craft / ship / enemies / depot.
        bump_k = 1.35;
        spec_k = 0.65;
        spec_p = 32.0;
        rim_col = vec3(0.65, 0.82, 1.0);
        rim_k = 0.35;
    } else if (mat >= 0.80) {
        // Boss core, rings, power-ups: bright metallic gleam + self-illumination.
        bump_k = 1.25;
        spec_k = 0.75;
        spec_p = 34.0;
        rim_col = vec3(1.0, 0.82, 0.55);
        rim_k = 0.38;
        emissive = t.rgb * smoothstep(0.45, 0.95, luma(t.rgb)) * 0.35;
    }

    // Feathered outer glow/shadow pixels keep flat smooth normals.
    float solid = smoothstep(0.25, 0.75, t.a);
    vec2 local_xy = local * 0.62 + vec2(hl - hr, hu - hd) * bump_k * solid;
    // Rotate normal XY from sprite local space into screen space.
    vec2 world_xy = vec2(local_xy.x * cs - local_xy.y * sn, local_xy.x * sn + local_xy.y * cs);
    float nz = sqrt(max(0.16, 1.0 - clamp(dot(local * 0.62, local * 0.62), 0.0, 0.82)));
    vec3 n = normalize(vec3(world_xy, nz));
    vec3 view_dir = vec3(0.0, 0.0, 1.0);

    // Key directional light (upper left) + hemisphere ambient fill.
    vec3 key = normalize(vec3(-0.45, -0.55, 0.72));
    float key_diff = max(dot(n, key), 0.0);
    // Keep original baked shading intact while adding crisp 3D relief.
    float shade = mix(0.76, 1.22, key_diff);
    vec3 half_key = normalize(key + view_dir);
    float key_spec = pow(max(dot(n, half_key), 0.0), spec_p) * spec_k * solid;
    float fresnel = pow(clamp(1.0 - n.z, 0.0, 1.0), 2.4) * solid;

    // Per-pixel dynamic point lights (explosions, shots, ship).
    vec3 dyn_diff = vec3(0.0);
    vec3 dyn_spec = vec3(0.0);
    for (int i = 0; i < 12; i++) {
        vec2 d = LightPos[i].xy - canvas;
        float r = LightPos[i].z;
        float fall = clamp(1.0 - length(d) / max(r, 1.0), 0.0, 1.0);
        float att = fall * fall;
        vec3 l = normalize(vec3(d, 22.0));
        float ndl = max(dot(n, l), 0.0);
        // Directional wrap lighting so explosions dramatically rim-light rocks.
        dyn_diff += LightColor[i] * att * (0.28 + 0.95 * ndl);
        vec3 h = normalize(l + view_dir);
        dyn_spec += LightColor[i] * att * pow(max(dot(n, h), 0.0), spec_p);
    }
    dyn_diff = dyn_diff / (1.0 + dyn_diff * 0.85);
    dyn_spec = dyn_spec / (1.0 + dyn_spec);

    vec3 rgb = t.rgb * shade
        + t.rgb * dyn_diff * 1.45
        + vec3(0.96, 0.98, 1.0) * key_spec * 0.42
        + dyn_spec * spec_k * 1.15 * solid
        + rim_col * fresnel * (rim_k * 0.55 + luma(dyn_diff) * 0.75)
        + emissive * solid;
    gl_FragColor = vec4(rgb, t.a * color.a);
}"#;

/// Procedural hexagonal Fresnel energy shield (port addition): razor-sharp
/// rotating honeycomb lattice, bright cyan Fresnel rim, and sweeping energy
/// pulse, blended additively.
const SHIELD_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform float Time;

float hex_edge(vec2 p) {
    p = abs(p);
    float d = max(dot(p, normalize(vec2(1.0, 1.7320508))), p.x);
    return d;
}

float hex_grid(vec2 uv_p) {
    vec2 r = vec2(1.0, 1.7320508);
    vec2 h = r * 0.5;
    vec2 a = mod(uv_p, r) - h;
    vec2 b = mod(uv_p - h, r) - h;
    vec2 gv = dot(a, a) < dot(b, b) ? a : b;
    float d = 0.5 - hex_edge(gv);
    return smoothstep(0.08, 0.015, d);
}

void main() {
    vec2 p = uv * 2.0 - 1.0;
    float r = length(p);
    if (r > 1.0) {
        discard;
    }
    // Slight spherical projection onto the bubble so hexes curve over the dome.
    float z = sqrt(max(0.02, 1.0 - r * r));
    vec2 sph = p / (0.65 + 0.55 * z);
    float ang = Time * 0.45;
    float cs = cos(ang);
    float sn = sin(ang);
    vec2 rot_p = vec2(sph.x * cs - sph.y * sn, sph.x * sn + sph.y * cs);
    float hex = hex_grid(rot_p * 3.4);
    float rim = pow(clamp(r, 0.0, 1.0), 3.6) * smoothstep(1.0, 0.87, r);
    float inner_rim = pow(clamp(r, 0.0, 1.0), 1.8) * smoothstep(1.0, 0.82, r);
    float sweep = 0.7 + 0.3 * sin(atan(p.y, p.x) * 3.0 - Time * 5.2 + r * 4.0);
    float energy = (rim * 1.15 + hex * inner_rim * 0.62 + inner_rim * 0.14) * sweep;
    vec3 col = mix(vec3(0.18, 0.78, 1.0), vec3(0.78, 0.98, 1.0), clamp(rim * 0.9 + hex * 0.4, 0.0, 1.0));
    gl_FragColor = vec4(col, clamp(energy * color.a, 0.0, 1.0));
}"#;

/// Procedural gravity gun tractor beam (port addition): animated cyan-violet
/// energy chevrons flowing toward the ship, glowing plasma filament core,
/// and feathered cone falloff. `uv.x` runs 0..1 along the beam, `uv.y` 0..1
/// across the cone width.
const GRAVITY_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying lowp vec2 uv;
uniform float Time;
uniform float Gripped;

void main() {
    float u = uv.x;
    float v = uv.y * 2.0 - 1.0;
    float av = abs(v);
    float edge = pow(clamp(1.0 - av, 0.0, 1.0), 1.8);
    // Flowing energy rings: faster and tighter when gripping an enemy.
    float speed = mix(2.4, 4.4, Gripped);
    float freq = mix(5.0, 7.5, Gripped);
    float wave = sin((u * freq + Time * speed - av * 0.65) * 6.2831853);
    float rings = smoothstep(0.15, 0.95, wave) * edge;
    float core = exp(-av * av * 18.0) * (0.65 + 0.35 * sin(u * 14.0 + Time * 9.0));
    float fade_ends = smoothstep(0.0, 0.08, u) * smoothstep(1.0, 0.12, u);
    float intensity = (edge * 0.24 + rings * 0.48 + core * 0.75) * fade_ends;
    vec3 col = mix(vec3(0.32, 0.68, 1.0), vec3(0.72, 0.92, 1.0), clamp(core + rings * 0.45, 0.0, 1.0));
    col = mix(col, vec3(0.65, 0.42, 1.0), (1.0 - core) * 0.32 * Gripped);
    gl_FragColor = vec4(col, clamp(intensity * color.a, 0.0, 1.0));
}"#;

/// Dual-phase flow-advected volumetric FBM gas shader (port addition): turns
/// the 128x128 toroidal fluid simulation into razor-sharp, billowing, swirling
/// 4K smoke plumes and incandescent turbulent plasma with 3D self-shadowing
/// and illumination from all 12 dynamic point lights.
const GAS_FIELD_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
varying lowp vec4 color;
varying highp vec2 canvas;
uniform sampler2D Texture;
uniform sampler2D HeatVel;
uniform sampler2D Glow;
uniform float Time;
uniform vec3 LightPos[12];
uniform vec3 LightColor[12];

float hash21(vec2 p) {
    p = fract(p * vec2(127.1, 311.7));
    p += dot(p, p + 34.5);
    return fract(p.x * p.y);
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    float a = hash21(i);
    float b = hash21(i + vec2(1.0, 0.0));
    float c = hash21(i + vec2(0.0, 1.0));
    float d = hash21(i + vec2(1.0, 1.0));
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.54;
    for (int i = 0; i < 3; i++) {
        v += a * vnoise(p);
        p = vec2(p.x * 1.62 - p.y * 1.18, p.x * 1.18 + p.y * 1.62) + vec2(7.3, 13.1);
        a *= 0.48;
    }
    return v;
}

void main() {
    vec4 dye0 = texture2D(Texture, uv);
    vec4 hv0 = texture2D(HeatVel, uv);
    vec4 glow0 = texture2D(Glow, uv);
    if (dye0.a < 0.004 && hv0.r < 0.004 && glow0.a < 0.004) {
        discard;
    }

    // Dual-phase flow advection so fine gas tendrils continuously stream along
    // the live Navier-Stokes velocity field without stretching or snapping.
    vec2 vel = (hv0.gb * 2.0 - 1.0) * 1.35;
    float p0 = fract(Time * 0.34);
    float p1 = fract(Time * 0.34 + 0.5);
    float blend = abs(2.0 * p0 - 1.0);
    vec2 base_p = uv * 24.0;
    float n0 = fbm(base_p - vel * p0 * 1.15);
    float n1 = fbm(base_p - vel * p1 * 1.15 + vec2(4.1, 2.7));
    float turb = mix(n0, n1, blend);

    float m0 = fbm(base_p * 1.35 + vec2(11.4, 5.2) - vel * p0 * 1.35);
    float m1 = fbm(base_p * 1.35 + vec2(15.5, 7.9) - vel * p1 * 1.35);
    float turb2 = mix(m0, m1, blend);

    // Curl-weighted UV warp carves organic wisps & Kelvin-Helmholtz billows.
    float curl_boost = 0.55 + 0.85 * hv0.a;
    vec2 warp = vec2(turb - 0.5, turb2 - 0.5) * (0.0085 * curl_boost);
    vec2 uv_w = uv + warp;

    vec4 dye = texture2D(Texture, uv_w);
    vec4 hv = texture2D(HeatVel, uv_w);
    vec4 glow = texture2D(Glow, uv_w);

    // Carve soft gossamer filaments on thin edges while keeping cores volumetric.
    float wispy = mix(0.46, 1.62, turb);
    float density = clamp(dye.a * wispy * 1.15, 0.0, 0.96);
    float heat = hv.r;

    // 3D volumetric self-shadowing along upper-left key light direction.
    float d_key = texture2D(Texture, uv_w + vec2(-0.008, -0.010)).a;
    float self_shadow = clamp(0.68 + (dye.a - d_key) * 1.55 + (turb - 0.5) * 0.32, 0.34, 1.42);

    // All 12 dynamic point lights illuminate billowing smoke from within.
    vec3 dyn = vec3(0.0);
    for (int i = 0; i < 12; i++) {
        vec2 d = LightPos[i].xy - canvas;
        float r = LightPos[i].z;
        float fall = clamp(1.0 - length(d) / max(r, 1.0), 0.0, 1.0);
        dyn += LightColor[i] * (fall * fall);
    }
    dyn = dyn / (1.0 + dyn * 0.8);

    vec3 smoke_col = dye.rgb * self_shadow + (dye.rgb * 0.85 + vec3(0.22)) * dyn * (0.75 + 0.55 * turb);

    // Turbulent incandescent fire & plasma filaments.
    float fire_shape = clamp(glow.a * mix(0.55, 1.65, turb * 0.65 + turb2 * 0.35), 0.0, 1.0);
    vec3 fire_col = glow.rgb * (1.25 + 0.95 * turb2);

    // Combine volumetric lit smoke with blazing plasma core.
    float out_a = clamp(max(density * 0.82, fire_shape * 0.95) * color.a, 0.0, 0.98);
    if (out_a < 0.003) {
        discard;
    }
    vec3 out_rgb = mix(smoke_col, smoke_col * 0.45 + fire_col * 1.45, clamp(fire_shape * 1.35 + heat * 0.5, 0.0, 1.0));
    gl_FragColor = vec4(out_rgb, out_a);
}"#;

/// The final blit with post effects (port addition): shockwave rings that
/// bend both the scene and bloom (with a slight colour split at the ring),
/// linear-space bloom addition with calibrated ACES filmic tone mapping,
/// per-world colour grading, vignette, and 1-LSB dithering against banding.
const POST_FRAGMENT: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying highp vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Bloom;
uniform float BloomStrength;
uniform vec2 View;
uniform vec4 Waves[4];
uniform vec3 Lift;
uniform vec3 Gain;
uniform float Saturation;
uniform float Vignette;
vec2 to_canvas(vec2 t) {
    return vec2(View.x * (1.0 - t.y) - View.y, 320.0 * (1.0 - t.x));
}
vec2 to_uv(vec2 c) {
    return vec2(1.0 - c.y / 320.0, 1.0 - (c.x + View.y) / View.x);
}
vec3 aces_filmic(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}
void main() {
    vec2 c = to_canvas(uv);
    vec2 shift = vec2(0.0);
    for (int i = 0; i < 4; i++) {
        vec2 d = c - Waves[i].xy;
        float dist = length(d);
        float band = (dist - Waves[i].z) / 14.0;
        shift += d / max(dist, 1.0) * Waves[i].w * exp(-band * band);
    }
    float mag = length(shift);
    float split = mag * 0.28;
    vec2 dir = shift / max(mag, 0.001);
    vec2 uv_r = to_uv(c - shift - dir * split);
    vec2 uv_g = to_uv(c - shift);
    vec2 uv_b = to_uv(c - shift + dir * split);
    vec3 rgb = vec3(
        texture2D(Texture, uv_r).r,
        texture2D(Texture, uv_g).g,
        texture2D(Texture, uv_b).b);
    vec3 lin = pow(max(rgb, vec3(0.0)), vec3(2.2));
    if (BloomStrength > 0.0) {
        vec3 glow = vec3(
            texture2D(Bloom, uv_r).r,
            texture2D(Bloom, uv_g).g,
            texture2D(Bloom, uv_b).b);
        // Add both linear energy and a soft screen-like halo so hot emitters
        // stay saturated instead of hard-clipping to flat white.
        lin += pow(max(glow, vec3(0.0)), vec3(1.6)) * BloomStrength;
    }
    // Calibrated ACES rolloff: preserves punchy midtones and deep blacks while
    // turning blazing explosions and lasers into silky incandescent cores.
    rgb = pow(aces_filmic(lin * 1.08), vec3(1.0 / 2.2));
    float luma = dot(rgb, vec3(0.299, 0.587, 0.114));
    rgb = mix(vec3(luma), rgb, Saturation);
    rgb = Lift + rgb * Gain;
    vec2 v = (c - vec2(240.0, 160.0)) / vec2(View.x * 0.5, 160.0);
    rgb *= 1.0 - Vignette * dot(v, v) * 0.48;
    // 1-LSB triangular-like dither removes dark space and vignette banding.
    float n = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    rgb += (n - 0.5) / 255.0;
    gl_FragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0) * color;
}"#;

/// A shockwave ring on the canvas: centre, radius and how far it pushes.
#[derive(Clone, Copy, Debug)]
pub struct Wave {
    pub pos: Vec2,
    pub radius: f32,
    pub strength: f32,
}

/// Colour grading: lift (added), gain (multiplied), saturation, vignette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    pub lift: Vec3,
    pub gain: Vec3,
    pub saturation: f32,
    pub vignette: f32,
}

pub const MAX_WAVES: usize = 4;

/// Point lights the ground shader takes at most.
pub const MAX_LIGHTS: usize = 12;

/// A point light on the landscape canvas: position, reach and colour
/// (already scaled by its intensity).
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: Vec2,
    pub radius: f32,
    pub color: Vec3,
}

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
    zoom: f32,
    shift: Vec2,
}

impl Camera for OriginalCamera {
    fn matrix(&self) -> Mat4 {
        view_matrix(self.view_w, self.zoom, self.shift)
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
fn backdrop_mesh(flipped: bool, wide: f32, zoom: f32) -> Mesh {
    let sign = if flipped { -1.0 } else { 1.0 };
    // Growing faster than the width keeps the corners covered at 21:9; a
    // zoomed-out view (`zoom` times the area across) grows it the same way,
    // with cells `zoom` times larger so the mesh keeps its vertex count
    // (one draw holds only so many).
    let sphere = if flipped { 1.0 } else { (wide * zoom).powf(1.5) };
    let cell = CELL * zoom;
    let half_x = (DRAWN_HALF_X * wide).ceil() as i32 + (wide * zoom > 1.0) as i32;
    let half_y = DRAWN_HALF_Y + (zoom > 1.0) as i32;
    let cols = (2 * half_x + 1) as usize;
    let rows = (2 * half_y + 1) as usize;
    let mut vertices = Vec::with_capacity(cols * rows);
    for ix in -half_x..=half_x {
        for iy in -half_y..=half_y {
            let x = ix as f32 * cell;
            let y = iy as f32 * cell;
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
/// moved at 79% of their speed and rocks slid across it. The playfield
/// zoom scales depth too, bringing the planet `zoom` times closer, so the
/// rate follows the zoom.
pub fn glued_scroll(zoom: f32) -> f32 {
    let scale = EYE_Z / (EYE_Z + BACKGROUND_DEPTH / zoom);
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
    /// The frame's emitters, recorded while `glowing` (the world pass).
    emitters: RefCell<Vec<Mesh>>,
    glowing: Cell<bool>,
    /// Whether `end` adds the glow (Settings::bloom; MB_BLOOM=0 forces off).
    bloom_on: Cell<bool>,
    white: Texture2D,
    black: Texture2D,
    menu_backdrop: RefCell<Mesh>,
    game_backdrop: RefCell<Mesh>,
    backdrop_material: Material,
    additive: Material,
    flash: Material,
    lit: Material,
    shield: Material,
    gravity: Material,
    gas_mat: Material,
    gas_dye: Texture2D,
    gas_hv: Texture2D,
    gas_glow: Texture2D,
    gas_bufs: RefCell<(Vec<u8>, Vec<u8>, Vec<u8>)>,
    post: Material,
    /// The next blit's shockwaves and grading (None: the plain blit).
    post_fx: RefCell<Option<(Vec<Wave>, Grade)>>,
    /// Sprites are drawn with `lit` (Canvas::lighting).
    lighting_on: Cell<bool>,
    /// The ground's lights, normal map, world style and time for `draw_backdrop`.
    lights: RefCell<Vec<Light>>,
    normal: RefCell<Option<Texture2D>>,
    flat_normal: Texture2D,
    world_style: Cell<f32>,
    time: Cell<f32>,
    menu_fx: Cell<bool>,
    /// The play plane's zoom this frame (`set_zoom`; 1 outside the game),
    /// and the world copy being drawn (`set_shift`).
    zoom: Cell<f32>,
    shift: Cell<Vec2>,
    /// The zoom `game_backdrop` was built for.
    backdrop_zoom: Cell<f32>,
}

/// Resolution of the offscreen canvas relative to the original 480x320
/// framebuffer. The default matches the window's fitted canvas so the blit
/// is 1:1; MB_RENDER_SCALE pins it (1 keeps emulator comparisons exact).
/// The cap is 8: 4K fullscreen fits a ~6.75 scale, 8 leaves headroom for
/// 5K/Retina backing stores without unbounded texture sizes.
fn render_scale(view_w: f32) -> f32 {
    if let Ok(v) = std::env::var("MB_RENDER_SCALE") {
        if let Ok(s) = v.parse::<f32>() {
            return s.clamp(1.0, 8.0);
        }
    }
    let height = RENDER_HEIGHT.load(Ordering::Relaxed);
    if height > 0 {
        return (height as f32 / CANVAS_H).clamp(1.0, 8.0);
    }
    let fit = (screen_width() / view_w).min(screen_height() / CANVAS_H);
    fit.clamp(1.0, 8.0)
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
                vertex: BACKDROP_VERTEX,
                fragment: BACKDROP_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                textures: vec!["Decal".to_owned(), "Normal".to_owned()],
                uniforms: vec![
                    UniformDesc::new("Scroll0", UniformType::Float2),
                    UniformDesc::new("Scroll1", UniformType::Float2),
                    UniformDesc::new("Lit", UniformType::Float1),
                    UniformDesc::new("Time", UniformType::Float1),
                    UniformDesc::new("WorldStyle", UniformType::Float1),
                    UniformDesc::new("NdcToCanvas", UniformType::Float4),
                    UniformDesc::new("NdcOffset", UniformType::Float2),
                    UniformDesc::new("LightPos", UniformType::Float3).array(MAX_LIGHTS),
                    UniformDesc::new("LightColor", UniformType::Float3).array(MAX_LIGHTS),
                ],
            },
        )
        .expect("backdrop shader");
        let lit = load_material(
            ShaderSource::Glsl {
                vertex: LIT_VERTEX,
                fragment: LIT_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                uniforms: vec![
                    UniformDesc::new("AtlasGrid", UniformType::Float2),
                    UniformDesc::new("NdcToCanvas", UniformType::Float4),
                    UniformDesc::new("NdcOffset", UniformType::Float2),
                    UniformDesc::new("LightPos", UniformType::Float3).array(MAX_LIGHTS),
                    UniformDesc::new("LightColor", UniformType::Float3).array(MAX_LIGHTS),
                ],
                ..Default::default()
            },
        )
        .expect("lit shader");
        let shield = load_material(
            ShaderSource::Glsl {
                vertex: DEFAULT_VERTEX,
                fragment: SHIELD_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::One),
                uniforms: vec![UniformDesc::new("Time", UniformType::Float1)],
                ..Default::default()
            },
        )
        .expect("shield shader");
        let gravity = load_material(
            ShaderSource::Glsl {
                vertex: DEFAULT_VERTEX,
                fragment: GRAVITY_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::One),
                uniforms: vec![
                    UniformDesc::new("Time", UniformType::Float1),
                    UniformDesc::new("Gripped", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .expect("gravity shader");
        let gas_mat = load_material(
            ShaderSource::Glsl {
                vertex: LIT_VERTEX,
                fragment: GAS_FIELD_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: pipeline(BlendFactor::OneMinusValue(BlendValue::SourceAlpha)),
                textures: vec!["HeatVel".to_owned(), "Glow".to_owned()],
                uniforms: vec![
                    UniformDesc::new("Time", UniformType::Float1),
                    UniformDesc::new("NdcToCanvas", UniformType::Float4),
                    UniformDesc::new("NdcOffset", UniformType::Float2),
                    UniformDesc::new("LightPos", UniformType::Float3).array(MAX_LIGHTS),
                    UniformDesc::new("LightColor", UniformType::Float3).array(MAX_LIGHTS),
                ],
                ..Default::default()
            },
        )
        .expect("gas shader");
        let make_gas_tex = || {
            use macroquad::miniquad::{
                MipmapFilterMode, TextureAccess, TextureFormat, TextureKind, TextureParams,
                TextureSource, TextureWrap,
            };
            let n = crate::gas::N as u32;
            let zero = vec![0u8; (n * n * 4) as usize];
            let gl = unsafe { get_internal_gl() };
            let id = gl.quad_context.new_texture(
                TextureAccess::Static,
                TextureSource::Bytes(&zero),
                TextureParams {
                    kind: TextureKind::Texture2D,
                    format: TextureFormat::RGBA8,
                    wrap: TextureWrap::Repeat,
                    min_filter: FilterMode::Linear,
                    mag_filter: FilterMode::Linear,
                    mipmap_filter: MipmapFilterMode::None,
                    width: n,
                    height: n,
                    allocate_mipmaps: false,
                    sample_count: 0,
                },
            );
            Texture2D::from_miniquad_texture(id)
        };
        let gas_dye = make_gas_tex();
        let gas_hv = make_gas_tex();
        let gas_glow = make_gas_tex();
        let gas_bytes = crate::gas::N * crate::gas::N * 4;
        let post = load_material(
            ShaderSource::Glsl { vertex: DEFAULT_VERTEX, fragment: POST_FRAGMENT },
            MaterialParams {
                textures: vec!["Bloom".to_owned()],
                uniforms: vec![
                    UniformDesc::new("BloomStrength", UniformType::Float1),
                    UniformDesc::new("View", UniformType::Float2),
                    UniformDesc::new("Waves", UniformType::Float4).array(MAX_WAVES),
                    UniformDesc::new("Lift", UniformType::Float3),
                    UniformDesc::new("Gain", UniformType::Float3),
                    UniformDesc::new("Saturation", UniformType::Float1),
                    UniformDesc::new("Vignette", UniformType::Float1),
                ],
                ..Default::default()
            },
        )
        .expect("post shader");
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
        let black = Texture2D::from_rgba8(1, 1, &[0, 0, 0, 255]);
        Canvas {
            target: RefCell::new(target),
            target_size: Cell::new(size),
            bloom: RefCell::new(Bloom::new(size)),
            emitters: RefCell::new(Vec::new()),
            glowing: Cell::new(false),
            bloom_on: Cell::new(false),
            white,
            black,
            menu_backdrop: RefCell::new(backdrop_mesh(true, view_w / CANVAS_W, 1.0)),
            game_backdrop: RefCell::new(backdrop_mesh(false, view_w / CANVAS_W, 1.0)),
            backdrop_material,
            additive,
            flash,
            lit,
            shield,
            gravity,
            gas_mat,
            gas_dye,
            gas_hv,
            gas_glow,
            gas_bufs: RefCell::new((vec![0; gas_bytes], vec![0; gas_bytes], vec![0; gas_bytes])),
            post,
            post_fx: RefCell::new(None),
            lighting_on: Cell::new(false),
            lights: RefCell::new(Vec::new()),
            normal: RefCell::new(None),
            flat_normal: Texture2D::from_rgba8(1, 1, &[128, 128, 255, 255]),
            world_style: Cell::new(0.0),
            time: Cell::new(0.0),
            menu_fx: Cell::new(false),
            zoom: Cell::new(1.0),
            shift: Cell::new(Vec2::ZERO),
            backdrop_zoom: Cell::new(1.0),
        }
    }

    /// Records additive draws as emitters while on (the world pass).
    pub fn set_glowing(&self, on: bool) {
        self.glowing.set(on);
    }

    /// Adds a non-additive draw to the frame's emitters (laser bolts,
    /// tower beams), without drawing it.
    pub fn add_glow(&self, vertices: Vec<Vertex>, indices: Vec<u16>, texture: Option<&Texture2D>) {
        if self.glowing.get() && self.bloom_on.get() {
            let texture = Some(texture.unwrap_or(&self.white).clone());
            self.record(Mesh { vertices, indices, texture });
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
        self.restore_material();
    }

    fn restore_material(&self) {
        if self.lighting_on.get() {
            gl_use_material(&self.lit);
        } else {
            gl_use_default_material();
        }
    }

    /// Draws sprites with the per-pixel PBR-inspired lit material until turned
    /// off: vertex colours carry rotation and `SpriteMat` via `lit_color`.
    pub fn lighting(&self, on: bool) {
        self.lighting_on.set(on);
        if on {
            let (m, offset) = ndc_to_canvas(view_w(), self.zoom.get());
            self.lit.set_uniform("AtlasGrid", vec2(1.0, 1.0));
            self.lit.set_uniform("NdcToCanvas", m);
            self.lit.set_uniform("NdcOffset", offset);
            let lights = self.lights.borrow();
            let mut pos = [Vec3::ZERO; MAX_LIGHTS];
            let mut color = [Vec3::ZERO; MAX_LIGHTS];
            for (i, l) in lights.iter().enumerate() {
                pos[i] = vec3(l.pos.x, l.pos.y, l.radius);
                color[i] = l.color;
            }
            self.lit.set_uniform_array("LightPos", &pos);
            self.lit.set_uniform_array("LightColor", &color);
        }
        self.restore_material();
    }

    /// Sets the atlas grid dimensions `(cols, rows)` for subsequent lit atlas
    /// sprite draws so per-pixel normals stay inside each cell.
    pub fn set_sprite_atlas(&self, cols: f32, rows: f32) {
        if self.lighting_on.get() {
            self.lit.set_uniform("AtlasGrid", vec2(cols, rows));
        }
    }

    /// Lights and normal map for the next game backdrop (None: unlit, as
    /// the original).
    pub fn set_ground_lighting(&self, normal: Option<&Texture2D>, lights: &[Light]) {
        *self.normal.borrow_mut() = normal.cloned();
        *self.lights.borrow_mut() = lights.iter().take(MAX_LIGHTS).copied().collect();
    }

    /// World visual style (0..5 for worlds, 6 for menu space) and animation
    /// clock for the backdrop shader.
    pub fn set_backdrop_style(&self, world_style: f32, time: f32) {
        self.world_style.set(world_style);
        self.time.set(time);
    }

    /// Whether menu backdrops enable the procedural parallax starfield
    /// (follows Options > Lighting and effects).
    pub fn set_menu_fx(&self, on: bool) {
        self.menu_fx.set(on);
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
        let (mx, my) = mouse_position();
        Canvas::to_canvas(vec2(mx, my))
    }

    /// A window position (mouse or touch, in pixels) in canvas coordinates.
    pub fn to_canvas(p: Vec2) -> Vec2 {
        let r = Canvas::placement();
        vec2((p.x - r.x) / r.w * view_w() - edge(), (p.y - r.y) / r.h * CANVAS_H)
    }

    /// Starts a frame: follows the window's aspect and size, then targets
    /// the offscreen canvas.
    pub fn begin(&self) {
        let view_w = window_view_w();
        VIEW_W.store(view_w.to_bits(), Ordering::Relaxed);
        let size = target_size(view_w);
        PIXEL_SCALE.store((size.0 as f32 / FB_W).to_bits(), Ordering::Relaxed);
        if size != self.target_size.get() {
            *self.target.borrow_mut() = new_target(size);
            self.target_size.set(size);
            *self.bloom.borrow_mut() = Bloom::new(size);
            *self.menu_backdrop.borrow_mut() = backdrop_mesh(true, view_w / CANVAS_W, 1.0);
            *self.game_backdrop.borrow_mut() =
                backdrop_mesh(false, view_w / CANVAS_W, self.backdrop_zoom.get());
        }
        self.zoom.set(1.0);
        self.shift.set(Vec2::ZERO);
        self.use_camera(1.0);
        clear_background(BLACK);
        self.emitters.borrow_mut().clear();
        self.glowing.set(false);
        *self.post_fx.borrow_mut() = None;
    }

    fn use_camera(&self, zoom: f32) {
        set_camera(&OriginalCamera {
            render_pass: self.target.borrow().render_pass.clone(),
            view_w: view_w(),
            zoom,
            shift: self.shift.get(),
        });
    }

    /// Zooms the play plane out `zoom` times for the world pass (rebuilding
    /// the planet mesh to cover the wider view); bloom and ground lighting
    /// follow it for the rest of the frame.
    pub fn set_zoom(&self, zoom: f32) {
        if zoom != self.backdrop_zoom.get() {
            self.backdrop_zoom.set(zoom);
            *self.game_backdrop.borrow_mut() = backdrop_mesh(false, view_w() / CANVAS_W, zoom);
        }
        self.zoom.set(zoom);
        self.shift.set(Vec2::ZERO);
        self.use_camera(zoom);
    }

    /// Draws what follows `shift` canvas units over (a copy of the wrapping
    /// world, for zoomed views wider than it).
    pub fn set_shift(&self, shift: Vec2) {
        self.shift.set(shift);
        self.use_camera(self.zoom.get());
    }

    /// Back to the unzoomed camera, for screen-space drawing (the HUD, full
    /// screen flashes).
    pub fn screen_space(&self) {
        self.shift.set(Vec2::ZERO);
        self.use_camera(1.0);
    }

    /// An emitter for the bloom pass, moved to the world copy being drawn.
    fn record(&self, mut mesh: Mesh) {
        let shift = self.shift.get();
        if shift != Vec2::ZERO {
            for v in &mut mesh.vertices {
                v.position += shift.extend(0.0);
            }
        }
        self.emitters.borrow_mut().push(mesh);
    }

    /// Shockwaves and grading for this frame's blit (set after `begin`).
    pub fn set_post(&self, waves: &[Wave], grade: Grade) {
        *self.post_fx.borrow_mut() = Some((waves.iter().take(MAX_WAVES).copied().collect(), grade));
    }

    /// Blit the portrait target as the landscape image. Landscape (x, y) is
    /// portrait (320-y, 480-x); the target texture's v axis runs bottom-up.
    pub fn end(&self) {
        let bloom = self.bloom_on.get();
        if bloom {
            self.bloom.borrow().render(&self.emitters.borrow(), view_w(), self.zoom.get(), &self.additive);
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
        let post = self.post_fx.borrow();
        if let Some((waves, grade)) = post.as_ref() {
            let mut w = [Vec4::ZERO; MAX_WAVES];
            for (i, wave) in waves.iter().enumerate() {
                w[i] = vec4(wave.pos.x, wave.pos.y, wave.radius, wave.strength);
            }
            let bloom_tex = if bloom { self.bloom.borrow().texture() } else { self.black.clone() };
            self.post.set_texture("Bloom", bloom_tex);
            self.post.set_uniform("BloomStrength", if bloom { BLOOM_STRENGTH } else { 0.0 });
            self.post.set_uniform("View", vec2(view_w, edge()));
            self.post.set_uniform_array("Waves", &w);
            self.post.set_uniform("Lift", grade.lift);
            self.post.set_uniform("Gain", grade.gain);
            self.post.set_uniform("Saturation", grade.saturation);
            self.post.set_uniform("Vignette", grade.vignette);
            gl_use_material(&self.post);
            draw_mesh(&quad);
            gl_use_default_material();
        } else {
            draw_mesh(&quad);
            if bloom {
                let b = self.bloom.borrow();
                let glow = Mesh {
                    vertices: quad.vertices.clone(),
                    indices: quad.indices.clone(),
                    texture: Some(b.texture()),
                };
                b.add.set_uniform("Strength", BLOOM_STRENGTH);
                gl_use_material(&b.add);
                draw_mesh(&glow);
                gl_use_default_material();
            }
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
        // Port addition: ground lighting and world-specific surface effects.
        let normal = if menu { None } else { self.normal.borrow().clone() };
        let lit = if menu { self.menu_fx.get() } else { normal.is_some() };
        let style = if menu { 6.0f32 } else { self.world_style.get() };
        self.backdrop_material.set_uniform("Lit", if lit { 1.0f32 } else { 0.0 });
        self.backdrop_material.set_uniform("Time", self.time.get());
        self.backdrop_material.set_uniform("WorldStyle", style);
        self.backdrop_material
            .set_texture("Normal", normal.unwrap_or_else(|| self.flat_normal.clone()));
        let (m, offset) = ndc_to_canvas(view_w(), self.zoom.get());
        self.backdrop_material.set_uniform("NdcToCanvas", m);
        self.backdrop_material.set_uniform("NdcOffset", offset);
        let lights = self.lights.borrow();
        let mut pos = [Vec3::ZERO; MAX_LIGHTS];
        let mut color = [Vec3::ZERO; MAX_LIGHTS];
        for (i, l) in lights.iter().enumerate() {
            pos[i] = vec3(l.pos.x, l.pos.y, l.radius);
            color[i] = l.color;
        }
        self.backdrop_material.set_uniform_array("LightPos", &pos);
        self.backdrop_material.set_uniform_array("LightColor", &color);
        // Background::draw scrolls the base layer by 2/1024 and the cloud
        // decal by 3/1024 per unit. In game the base is set to keep pace
        // with the play plane instead (see `glued_scroll`); clouds keep
        // their 1.5x lead over it.
        // The mesh's v runs against y (GL textures start at the bottom), so
        // in game the vertical scroll goes the other way for the ground to
        // move opposite the ship, as it does horizontally.
        let k = if menu { ORIGINAL_SCROLL } else { glued_scroll(self.zoom.get()) };
        let flip = |v: Vec2| if menu { v } else { vec2(v.x, (1.0 - v.y).fract()) };
        self.backdrop_material.set_uniform("Scroll0", flip(wrap(k)));
        self.backdrop_material
            .set_uniform("Scroll1", flip(wrap(k * 1.5)));
        gl_use_material(&self.backdrop_material);
        draw_mesh(&mesh);
        gl_use_default_material();
    }

    /// Draws the procedural hexagonal Fresnel energy shield bubble around
    /// `center` and feeds a matching cyan glow quad to the bloom pyramid.
    pub fn draw_energy_shield(
        &self,
        center: Vec2,
        radius: f32,
        z: f32,
        time: f32,
        alpha: f32,
        bloom_tex: &Texture2D,
    ) {
        let c = Color::new(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0));
        let verts = vec![
            Vertex::new(center.x - radius, center.y - radius, z, 0.0, 0.0, c),
            Vertex::new(center.x + radius, center.y - radius, z, 1.0, 0.0, c),
            Vertex::new(center.x + radius, center.y + radius, z, 1.0, 1.0, c),
            Vertex::new(center.x - radius, center.y + radius, z, 0.0, 1.0, c),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        self.shield.set_uniform("Time", time);
        gl_use_material(&self.shield);
        draw_mesh(&Mesh {
            vertices: verts,
            indices: indices.clone(),
            texture: Some(self.white.clone()),
        });
        gl_use_default_material();
        if self.glowing.get() && self.bloom_on.get() {
            let glow_c = Color::new(0.28, 0.88, 1.0, (alpha * 0.42).clamp(0.0, 1.0));
            let r = radius * 1.05;
            let g_verts = vec![
                Vertex::new(center.x - r, center.y - r, z, 0.0, 0.0, glow_c),
                Vertex::new(center.x + r, center.y - r, z, 1.0, 0.0, glow_c),
                Vertex::new(center.x + r, center.y + r, z, 1.0, 1.0, glow_c),
                Vertex::new(center.x - r, center.y + r, z, 0.0, 1.0, glow_c),
            ];
            self.record(Mesh {
                vertices: g_verts,
                indices,
                texture: Some(bloom_tex.clone()),
            });
        }
    }

    /// Draws the procedural tractor-beam cone for the gravity gun and feeds
    /// its core glow to the bloom pyramid.
    pub fn draw_gravity_beam(
        &self,
        ship: Vec2,
        tip: Vec2,
        side: Vec2,
        start_w: f32,
        end_w: f32,
        z: f32,
        time: f32,
        gripped: bool,
    ) {
        let c = Color::new(1.0, 1.0, 1.0, if gripped { 0.95 } else { 0.78 });
        let verts = vec![
            Vertex::new(ship.x - side.x * start_w, ship.y - side.y * start_w, z, 0.0, 0.0, c),
            Vertex::new(tip.x - side.x * end_w, tip.y - side.y * end_w, z, 1.0, 0.0, c),
            Vertex::new(tip.x + side.x * end_w, tip.y + side.y * end_w, z, 1.0, 1.0, c),
            Vertex::new(ship.x + side.x * start_w, ship.y + side.y * start_w, z, 0.0, 1.0, c),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        self.gravity.set_uniform("Time", time);
        self.gravity.set_uniform("Gripped", if gripped { 1.0f32 } else { 0.0 });
        gl_use_material(&self.gravity);
        draw_mesh(&Mesh {
            vertices: verts,
            indices: indices.clone(),
            texture: Some(self.white.clone()),
        });
        gl_use_default_material();
        if self.glowing.get() && self.bloom_on.get() {
            let head = if gripped {
                Color::new(0.45, 0.75, 1.0, 0.55)
            } else {
                Color::new(0.35, 0.80, 1.0, 0.42)
            };
            let tail = Color::new(0.45, 0.65, 1.0, 0.0);
            let gw = end_w * 0.45;
            let g_verts = vec![
                Vertex::new(ship.x - side.x * 2.0, ship.y - side.y * 2.0, z, 0.0, 0.0, head),
                Vertex::new(tip.x - side.x * gw, tip.y - side.y * gw, z, 1.0, 0.0, tail),
                Vertex::new(tip.x + side.x * gw, tip.y + side.y * gw, z, 1.0, 1.0, tail),
                Vertex::new(ship.x + side.x * 2.0, ship.y + side.y * 2.0, z, 0.0, 1.0, head),
            ];
            self.record(Mesh {
                vertices: g_verts,
                indices,
                texture: Some(self.white.clone()),
            });
        }
    }

    /// Uploads the toroidal fluid gas grid and renders it across the entire
    /// visible playfield as dual-phase FBM-turbulent volumetric smoke & fire,
    /// feeding incandescent plasma regions straight into the bloom pyramid.
    pub fn draw_gas_field(
        &self,
        gas: &crate::gas::GasField,
        ship_world: Vec2,
        cam_offset: Vec2,
        view_half: Vec2,
        z: f32,
        time: f32,
    ) {
        if !gas.is_active() {
            return;
        }
        let n = crate::gas::N as u32;
        {
            let mut bufs = self.gas_bufs.borrow_mut();
            let (dye, hv, glow) = &mut *bufs;
            gas.pack(dye, hv, glow);
            self.gas_dye.update_from_bytes(n, n, dye);
            self.gas_hv.update_from_bytes(n, n, hv);
            self.gas_glow.update_from_bytes(n, n, glow);
        }
        let centre = vec2(CANVAS_W * 0.5, CANVAS_H * 0.5);
        let pad = view_half + Vec2::splat(24.0);
        let x0 = centre.x - pad.x;
        let x1 = centre.x + pad.x;
        let y0 = centre.y - pad.y;
        let y1 = centre.y + pad.y;
        let to_uv = |cx: f32, cy: f32| {
            let w = ship_world + (vec2(cx, cy) - centre - cam_offset);
            (w.x / 1024.0, w.y / 1024.0)
        };
        let (u00, v00) = to_uv(x0, y0);
        let (u10, v10) = to_uv(x1, y0);
        let (u11, v11) = to_uv(x1, y1);
        let (u01, v01) = to_uv(x0, y1);
        let verts = vec![
            Vertex::new(x0, y0, z, u00, v00, WHITE),
            Vertex::new(x1, y0, z, u10, v10, WHITE),
            Vertex::new(x1, y1, z, u11, v11, WHITE),
            Vertex::new(x0, y1, z, u01, v01, WHITE),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];

        let (m, offset) = ndc_to_canvas(view_w(), self.zoom.get());
        self.gas_mat.set_texture("HeatVel", self.gas_hv.clone());
        self.gas_mat.set_texture("Glow", self.gas_glow.clone());
        self.gas_mat.set_uniform("Time", time);
        self.gas_mat.set_uniform("NdcToCanvas", m);
        self.gas_mat.set_uniform("NdcOffset", offset);
        let lights = self.lights.borrow();
        let mut pos = [Vec3::ZERO; MAX_LIGHTS];
        let mut color = [Vec3::ZERO; MAX_LIGHTS];
        for (i, l) in lights.iter().enumerate() {
            pos[i] = vec3(l.pos.x, l.pos.y, l.radius);
            color[i] = l.color;
        }
        self.gas_mat.set_uniform_array("LightPos", &pos);
        self.gas_mat.set_uniform_array("LightColor", &color);

        gl_use_material(&self.gas_mat);
        draw_mesh(&Mesh {
            vertices: verts.clone(),
            indices: indices.clone(),
            texture: Some(self.gas_dye.clone()),
        });
        gl_use_default_material();

        // Feed incandescent plasma regions directly into the 5-level bloom pyramid.
        if self.glowing.get() && self.bloom_on.get() {
            self.record(Mesh {
                vertices: verts,
                indices,
                texture: Some(self.gas_glow.clone()),
            });
        }
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
        let mesh = Mesh { vertices, indices, texture: Some(texture) };
        if additive {
            gl_use_material(&self.additive);
        }
        draw_mesh(&mesh);
        if additive && self.glowing.get() && self.bloom_on.get() {
            self.record(mesh);
        }
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
        let mesh = Mesh { vertices, indices, texture: Some(tex.clone()) };
        gl_use_material(&self.additive);
        draw_mesh(&mesh);
        gl_use_default_material();
        if self.glowing.get() && self.bloom_on.get() {
            self.emitters.borrow_mut().push(mesh);
        }
    }
}

/// Resolution multiplier of the loaded art. Desktop loads the 8x set from
/// `tools/upscale8x.py`; `MB_HD=1` loads the 4x set (`tools/upscale4x.py`)
/// and `MB_HD=0` the originals. The web build keeps the originals: the HD
/// sets are too large to download.
pub fn asset_scale() -> f32 {
    #[cfg(target_arch = "wasm32")]
    const DEFAULT_SCALE: f32 = 1.0;
    #[cfg(not(target_arch = "wasm32"))]
    const DEFAULT_SCALE: f32 = 8.0;
    static SCALE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| match std::env::var("MB_HD").as_deref() {
        Ok("0") => 1.0,
        Ok("1") => 4.0,
        Ok("8") => 8.0,
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

    /// The shader's clip-to-canvas map undoes the camera on the play plane.
    #[test]
    fn ndc_maps_back_to_the_canvas() {
        for w in [480.0, 569.0, 768.0] {
            let (m, o) = ndc_to_canvas(w, 1.0);
            let cam = camera_matrix(w);
            for p in [vec2(0.0, 0.0), vec2(240.0, 160.0), vec2(-40.0, 300.0), vec2(500.0, 12.0)] {
                let h = cam * vec4(p.x, p.y, 0.0, 1.0);
                let ndc = vec2(h.x / h.w, h.y / h.w);
                let back = vec2(m.x * ndc.x + m.y * ndc.y, m.z * ndc.x + m.w * ndc.y) + o;
                assert!(back.distance(p) < 1e-2, "{} {:?} {:?}", w, p, back);
            }
        }
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
    /// the centre of the view, at every playfield zoom.
    #[test]
    fn ground_keeps_pace_with_the_play_plane() {
        for zoom in [1.0, 1.5, 2.0] {
            let m = view_matrix(CANVAS_W, zoom, Vec2::ZERO);
            let step = |z: f32| {
                let c = vec3(CANVAS_W * 0.5, CANVAS_H * 0.5, z);
                let p = |v: Vec3| {
                    let h = m * v.extend(1.0);
                    vec2(h.x / h.w, h.y / h.w)
                };
                (p(c + vec3(1.0, 0.0, 0.0)) - p(c)).length()
            };
            let ratio = step(BACKGROUND_DEPTH) / step(0.0);
            let ground_units_per_unit = glued_scroll(zoom) / BACKDROP_SIZE / TEXEL_PER_UNIT;
            let pace = ground_units_per_unit * ratio;
            assert!((pace - 1.0).abs() < 1e-4, "zoom {} pace {}", zoom, pace);
        }
    }
}
