//! Original-layout menus, drawn on the iPhone's native 480x320 canvas and
//! scaled to the window. Positions come from the original binary
//! (`MainMenuScreen::MainMenuScreen`, `BasicScreen::draw`).

use macroquad::prelude::*;

pub const CANVAS_W: f32 = 480.0;
pub const CANVAS_H: f32 = 320.0;

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

pub struct Canvas {
    target: RenderTarget,
}

impl Canvas {
    pub fn new() -> Canvas {
        let target = render_target(CANVAS_W as u32 * 2, CANVAS_H as u32 * 2);
        target.texture.set_filter(FilterMode::Linear);
        Canvas { target }
    }

    /// Where the canvas lands in the window: largest 3:2 fit, centred.
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
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, CANVAS_W, CANVAS_H));
        cam.render_target = Some(self.target.clone());
        set_camera(&cam);
        clear_background(BLACK);
    }

    pub fn end(&self) {
        set_default_camera();
        clear_background(BLACK);
        let r = Canvas::placement();
        draw_texture_ex(
            &self.target.texture,
            r.x,
            r.y,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(r.w, r.h)), flip_y: true, ..Default::default() },
        );
    }
}

fn draw_scaled(tex: &Texture2D, x: f32, y: f32, scale: f32) {
    draw_texture_ex(tex, x, y, WHITE, DrawTextureParams { dest_size: Some(tex.size() * scale), ..Default::default() });
}

/// Menu backdrop: `space4deep` with `fogRed` decal-blended over it (texture
/// unit 1 uses GL_DECAL, which for an opaque base is plain alpha-over), then
/// `horizon.png` at (0,0) scaled 1.25.
///
/// The original draws the two space layers on a curved 3D mesh under a
/// perspective camera; this flat 1:1 tiling is an approximation.
pub fn draw_backdrop(assets: &MenuAssets) {
    for tex in [&assets.space, &assets.fog] {
        let (tw, th) = (tex.width(), tex.height());
        let mut y = 0.0;
        while y < CANVAS_H {
            let mut x = 0.0;
            while x < CANVAS_W {
                draw_texture(tex, x, y, WHITE);
                x += tw;
            }
            y += th;
        }
    }
    draw_scaled(&assets.horizon, 0.0, 0.0, 1.25);
}

/// The main menu button released this frame, if any.
pub fn main_menu_click() -> Option<MainButton> {
    if !is_mouse_button_released(MouseButton::Left) {
        return None;
    }
    let mouse = Canvas::mouse();
    MAIN_BUTTONS.iter().find(|(_, rect)| rect.contains(mouse)).map(|(button, _)| *button)
}

pub fn draw_main_menu(assets: &MenuAssets) {
    draw_backdrop(assets);
    draw_scaled(&assets.title, ((CANVAS_W - assets.title.width()) * 0.5).floor(), 15.0, 1.0);
    let mouse = Canvas::mouse();
    for (i, (_, rect)) in MAIN_BUTTONS.iter().enumerate() {
        let (up, down) = &assets.buttons[i];
        let held = rect.contains(mouse) && is_mouse_button_down(MouseButton::Left);
        draw_scaled(if held { down } else { up }, rect.x, rect.y, 1.0);
    }
}
