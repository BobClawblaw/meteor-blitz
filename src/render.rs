//! Rendering: planet terrain, entities, effects, HUD, and screens.

use macroquad::prelude::*;

use crate::game::{wrap_delta, Faction, Game, Mode, WORLD_H, WORLD_W};

pub struct Assets {
    pub crust: Texture2D,
    pub craters: Texture2D,
    pub fog_red: Texture2D,
    pub fog_green: Texture2D,
    pub fog_black_red: Texture2D,
    pub clouds: Texture2D,
    pub space: Texture2D,
}

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max: f32,
    pub color: Color,
    pub size: f32,
}

pub struct Camera {
    pub x: f32,
    pub y: f32,
}

impl Camera {
    pub fn follow(&mut self, px: f32, py: f32, dt: f32) {
        let dx = wrap_delta(self.x, px, WORLD_W);
        let dy = wrap_delta(self.y, py, WORLD_H);
        let k = (6.0 * dt).min(1.0);
        self.x = (self.x + dx * k).rem_euclid(WORLD_W);
        self.y = (self.y + dy * k).rem_euclid(WORLD_H);
    }
}

fn screen_of(cam: &Camera, x: f32, y: f32) -> (f32, f32) {
    let dx = wrap_delta(cam.x, x, WORLD_W);
    let dy = wrap_delta(cam.y, y, WORLD_H);
    (screen_width() * 0.5 + dx, screen_height() * 0.5 + dy)
}

fn world_palette(world: usize) -> Color {
    match world {
        0 => Color::new(1.0, 1.05, 0.95, 1.0),
        1 => Color::new(0.95, 0.95, 1.15, 1.0),
        2 => Color::new(1.15, 0.95, 0.85, 1.0),
        3 => Color::new(0.95, 1.15, 1.2, 1.0),
        4 => Color::new(0.85, 0.95, 1.05, 1.0),
        _ => Color::new(1.1, 0.85, 0.95, 1.0),
    }
}

pub fn draw_terrain(assets: &Assets, cam: &Camera, world: usize) {
    let tint = world_palette(world);
    let tile = 512.0;
    let w = screen_width();
    let h = screen_height();
    let start_x = cam.x - w * 0.5;
    let start_y = cam.y - h * 0.5;
    let first_x = (start_x / tile).floor() as i32;
    let first_y = (start_y / tile).floor() as i32;
    let count_x = (w / tile).ceil() as i32 + 2;
    let count_y = (h / tile).ceil() as i32 + 2;
    for ty in 0..count_y {
        for tx in 0..count_x {
            let wx = (first_x + tx) as f32 * tile;
            let wy = (first_y + ty) as f32 * tile;
            let sx = w * 0.5 + (wx - cam.x);
            let sy = h * 0.5 + (wy - cam.y);
            draw_texture_ex(
                &assets.crust,
                sx,
                sy,
                tint,
                DrawTextureParams {
                    dest_size: Some(vec2(tile, tile)),
                    ..Default::default()
                },
            );
            draw_texture_ex(
                &assets.craters,
                sx,
                sy,
                Color::new(1.0, 1.0, 1.0, 0.35),
                DrawTextureParams {
                    dest_size: Some(vec2(tile, tile)),
                    ..Default::default()
                },
            );
        }
    }
    // Fog layer, tinted per world.
    let fog = match world {
        0 => &assets.fog_green,
        1 => &assets.fog_black_red,
        2 => &assets.fog_red,
        3 => &assets.fog_green,
        4 => &assets.fog_black_red,
        _ => &assets.fog_red,
    };
    for ty in 0..count_y {
        for tx in 0..count_x {
            let wx = (first_x + tx) as f32 * tile;
            let wy = (first_y + ty) as f32 * tile;
            let sx = w * 0.5 + (wx - cam.x);
            let sy = h * 0.5 + (wy - cam.y);
            draw_texture_ex(
                fog,
                sx,
                sy,
                Color::new(1.0, 1.0, 1.0, 0.18),
                DrawTextureParams {
                    dest_size: Some(vec2(tile, tile)),
                    ..Default::default()
                },
            );
        }
    }
}

fn meteor_color(kind: u8) -> Color {
    match kind {
        1 => Color::new(0.55, 0.85, 1.0, 1.0),
        2 => Color::new(1.0, 0.55, 0.35, 1.0),
        _ => Color::new(0.75, 0.75, 0.70, 1.0),
    }
}

pub fn draw_entities(game: &Game, cam: &Camera) {
    for e in &game.entities {
        let (sx, sy) = screen_of(cam, e.x, e.y);
        if sx < -80.0 || sx > screen_width() + 80.0 || sy < -80.0 || sy > screen_height() + 80.0 {
            continue;
        }
        match e.faction {
            Faction::Meteor => {
                draw_poly(sx, sy, 7, e.radius, e.rot, meteor_color(e.kind));
                draw_poly(sx, sy, 7, e.radius * 0.62, e.rot + 0.4, Color::new(0.0, 0.0, 0.0, 0.28));
                draw_poly_lines(sx, sy, 7, e.radius, e.rot, 2.0, Color::new(1.0, 1.0, 1.0, 0.55));
            }
            Faction::Debris => {
                draw_poly(sx, sy, 5, e.radius, e.rot, meteor_color(e.kind));
            }
            Faction::Projectile => {
                let color = match e.weapon {
                    1 => Color::new(1.0, 0.55, 0.25, 1.0),
                    2 => Color::new(0.6, 0.9, 1.0, 1.0),
                    9 => Color::new(1.0, 0.35, 0.45, 1.0),
                    _ => Color::new(0.25, 0.85, 1.0, 1.0),
                };
                let len = 0.02;
                draw_line(sx, sy, sx - e.vx * len, sy - e.vy * len, 3.0, color);
                draw_circle(sx, sy, 2.5, Color::new(1.0, 1.0, 1.0, 0.9));
            }
            Faction::Enemy => {
                let color = Color::new(1.0, 0.35, 0.45, 1.0);
                match e.enemy {
                    crate::game::EnemyKind::Drone => {
                        draw_poly(sx, sy, 3, e.radius, e.rot, color);
                    }
                    crate::game::EnemyKind::Turret => {
                        draw_rectangle(sx - e.radius, sy - e.radius, e.radius * 2.0, e.radius * 2.0, color);
                    }
                    crate::game::EnemyKind::Slinger => {
                        draw_poly(sx, sy, 4, e.radius, e.rot, color);
                    }
                }
            }
            Faction::Boss => {
                draw_circle(sx, sy, e.radius, Color::new(0.55, 0.15, 0.25, 1.0));
                draw_circle_lines(sx, sy, e.radius, 4.0, Color::new(1.0, 0.55, 0.65, 1.0));
                draw_circle(sx, sy, e.radius * 0.55, Color::new(1.0, 0.45, 0.55, 0.9));
                let hp = (e.health / 600.0).max(0.0);
                draw_rectangle(sx - 60.0, sy - e.radius - 18.0, 120.0, 6.0, Color::new(0.1, 0.1, 0.15, 0.8));
                draw_rectangle(sx - 60.0, sy - e.radius - 18.0, 120.0 * hp, 6.0, Color::new(1.0, 0.35, 0.45, 1.0));
            }
            Faction::Player => {}
        }
    }

    // Player ship.
    let (sx, sy) = screen_of(cam, game.player.x, game.player.y);
    let aim = game.player.aim_angle();
    let nose = vec2(sx + aim.cos() * 22.0, sy + aim.sin() * 22.0);
    let left = vec2(sx + (aim + 2.5).cos() * 14.0, sy + (aim + 2.5).sin() * 14.0);
    let right = vec2(sx + (aim - 2.5).cos() * 14.0, sy + (aim - 2.5).sin() * 14.0);
    draw_triangle(nose, left, right, Color::new(0.15, 0.85, 1.0, 1.0));
    draw_triangle_lines(nose, left, right, 2.0, Color::new(0.85, 1.0, 1.0, 0.9));
    draw_circle(sx, sy, 6.0, Color::new(0.85, 1.0, 1.0, 0.9));
}

pub fn draw_particles(particles: &mut Vec<Particle>, cam: &Camera, dt: f32) {
    for p in particles.iter_mut() {
        p.life -= dt;
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        p.vx *= 0.96;
        p.vy *= 0.96;
        let a = (p.life / p.max).max(0.0);
        let (sx, sy) = screen_of(cam, p.x, p.y);
        draw_circle(sx, sy, p.size * a, Color::new(p.color.r, p.color.g, p.color.b, a));
    }
    particles.retain(|p| p.life > 0.0);
}

pub fn burst(particles: &mut Vec<Particle>, x: f32, y: f32, count: usize, color: Color, speed: f32) {
    for _ in 0..count {
        let a: f32 = rand::gen_range(0.0, 6.283);
        let s: f32 = rand::gen_range(speed * 0.3, speed);
        particles.push(Particle {
            x,
            y,
            vx: a.cos() * s,
            vy: a.sin() * s,
            life: rand::gen_range(0.3, 0.8),
            max: 0.8,
            color,
            size: rand::gen_range(2.0, 5.0),
        });
    }
}

fn panel(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, Color::new(0.04, 0.06, 0.12, 0.72));
    draw_rectangle_lines(x, y, w, h, 1.0, Color::new(0.15, 0.75, 0.85, 0.35));
}

fn text_center(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let dim = measure_text(text, None, size as u16, 1.0);
    draw_text(text, x - dim.width * 0.5, y, size, color);
}

pub fn draw_hud(game: &Game) {
    let w = screen_width();
    let h = screen_height();
    let cyan = Color::new(0.15, 0.85, 1.0, 1.0);
    let muted = Color::new(0.55, 0.65, 0.75, 1.0);
    let amber = Color::new(1.0, 0.75, 0.25, 1.0);

    panel(20.0, 20.0, 190.0, 86.0);
    draw_text("SCORE", 34.0, 40.0, 14.0, muted);
    draw_text(&game.score.to_string(), 34.0, 76.0, 30.0, Color::new(0.9, 1.0, 1.0, 1.0));
    draw_text(&format!("x{:.1}", game.combo), 34.0, 96.0, 14.0, if game.combo > 2.0 { amber } else { muted });

    panel(w - 240.0, 20.0, 220.0, 86.0);
    draw_text("HULL", w - 226.0, 40.0, 14.0, muted);
    draw_rectangle(w - 226.0, 48.0, 190.0, 10.0, Color::new(1.0, 1.0, 1.0, 0.1));
    draw_rectangle(w - 226.0, 48.0, 190.0 * (game.player.health / crate::game::PLAYER_MAX_HEALTH).max(0.0), 10.0, cyan);
    draw_text(&format!("ORE {}", game.player.ore), w - 226.0, 80.0, 14.0, amber);
    draw_text(&format!("BOMB {}", game.player.bombs), w - 226.0, 98.0, 14.0, amber);

    panel(20.0, h - 96.0, 240.0, 76.0);
    let weapon_name = ["PLASMA", "FLAME", "ICE", "GRAVITY"][game.player.weapon as usize];
    draw_text(&format!("{} {}", weapon_name, ["I", "II", "III"][game.player.upgrades[game.player.weapon as usize] as usize]), 34.0, h - 70.0, 18.0, Color::new(0.9, 1.0, 1.0, 1.0));
    draw_rectangle(34.0, h - 60.0, 200.0, 8.0, Color::new(1.0, 1.0, 1.0, 0.1));
    draw_rectangle(34.0, h - 60.0, 200.0 * (game.player.heat / 100.0), 8.0, if game.player.overheated { amber } else { Color::new(0.15, 0.85, 1.0, 1.0) });
    draw_text(&format!("DASH {:.1}", game.player.dash_cd), 34.0, h - 36.0, 14.0, muted);

    panel(w - 200.0, h - 96.0, 180.0, 76.0);
    draw_text("RADAR", w - 186.0, h - 70.0, 14.0, muted);
    let rx = w - 110.0;
    let ry = h - 48.0;
    draw_circle_lines(rx, ry, 34.0, 1.0, Color::new(0.15, 0.85, 1.0, 0.4));
    for e in &game.entities {
        if e.faction == Faction::Meteor || e.faction == Faction::Enemy || e.faction == Faction::Boss {
            let dx = wrap_delta(game.player.x, e.x, WORLD_W);
            let dy = wrap_delta(game.player.y, e.y, WORLD_H);
            if dx.abs() < 1200.0 && dy.abs() < 1200.0 {
                let color = if e.faction == Faction::Meteor { Color::new(0.6, 0.6, 0.6, 0.8) } else { Color::new(1.0, 0.35, 0.45, 0.9) };
                draw_circle(rx + dx / 1200.0 * 34.0, ry + dy / 1200.0 * 34.0, 1.5, color);
            }
        }
    }
    draw_circle(rx, ry, 2.5, Color::new(0.15, 0.85, 1.0, 1.0));

    let stage_label = if game.mode == Mode::Survival {
        format!("SURVIVAL  {:02}:{:02}", (game.time / 60.0) as u32, (game.time % 60.0) as u32)
    } else {
        format!(
            "WORLD {}  STAGE {}  {:02}:{:02}",
            game.world + 1,
            game.stage,
            (game.stage_time / 60.0) as u32,
            (game.stage_time % 60.0) as u32
        )
    };
    text_center(&stage_label, w * 0.5, 40.0, 18.0, muted);
}

pub fn draw_title(world_best: u64) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.02, 0.03, 0.06, 0.55));
    text_center("METEOR BLITZ", w * 0.5, h * 0.32, 72.0, Color::new(0.9, 1.0, 1.0, 1.0));
    text_center("Survive the collapsing belt. Shatter rock. Sling it back.", w * 0.5, h * 0.40, 20.0, Color::new(0.55, 0.65, 0.75, 1.0));
    if world_best > 0 {
        text_center(&format!("BEST {}", world_best), w * 0.5, h * 0.46, 16.0, Color::new(1.0, 0.75, 0.25, 1.0));
    }
    text_center("WASD move · mouse aim · LMB fire · RMB gravity · E polarity · Shift dash · 1/2/3/4 weapon · U upgrade · B bomb", w * 0.5, h * 0.56, 14.0, Color::new(0.55, 0.65, 0.75, 1.0));
    text_center("LAUNCH", w * 0.5, h * 0.68, 28.0, Color::new(0.15, 0.85, 1.0, 1.0));
    text_center("press ENTER", w * 0.5, h * 0.72, 14.0, Color::new(0.55, 0.65, 0.75, 1.0));
}

pub fn draw_stage_select(unlocked: usize) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.02, 0.03, 0.06, 0.85));
    text_center("ARCADE", w * 0.5, 80.0, 40.0, Color::new(0.9, 1.0, 1.0, 1.0));
    let start_x = w * 0.5 - 5.0 * 70.0 * 0.5;
    for world in 0..6 {
        for stage in 1..=5 {
            let idx = world * 5 + stage - 1;
            let x = start_x + (stage - 1) as f32 * 70.0;
            let y = 120.0 + world as f32 * 70.0;
            let locked = idx > unlocked;
            let color = if locked { Color::new(0.2, 0.25, 0.3, 1.0) } else { Color::new(0.15, 0.85, 1.0, 1.0) };
            draw_rectangle_lines(x, y, 60.0, 60.0, 2.0, color);
            text_center(&format!("{}.{}", world + 1, stage), x + 30.0, y + 36.0, 18.0, color);
        }
    }
    text_center("click a stage · ESC back", w * 0.5, h - 60.0, 14.0, Color::new(0.55, 0.65, 0.75, 1.0));
}

pub fn draw_pause() {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.02, 0.03, 0.06, 0.6));
    text_center("PAUSED", w * 0.5, h * 0.45, 48.0, Color::new(0.9, 1.0, 1.0, 1.0));
    text_center("ESC resume", w * 0.5, h * 0.52, 16.0, Color::new(0.55, 0.65, 0.75, 1.0));
}

pub fn draw_over(score: u64, won: bool) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.02, 0.03, 0.06, 0.7));
    text_center(if won { "STAGE CLEAR" } else { "HULL BREACH" }, w * 0.5, h * 0.40, 48.0, Color::new(0.9, 1.0, 1.0, 1.0));
    text_center(&format!("SCORE {}", score), w * 0.5, h * 0.48, 24.0, Color::new(1.0, 0.75, 0.25, 1.0));
    text_center("ENTER retry · ESC menu", w * 0.5, h * 0.56, 16.0, Color::new(0.55, 0.65, 0.75, 1.0));
}
