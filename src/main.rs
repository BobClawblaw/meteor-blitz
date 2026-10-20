//! Meteor Blitz — modern port. Window, screens, input, audio, and loop.

mod game;
mod menu;
mod progress;
mod render;

use macroquad::prelude::*;
use macroquad::audio::{load_sound, play_sound, PlaySoundParams, Sound};

use game::{Event, Game, Input, Mode};
use menu::{Canvas, MainButton, MenuAssets};
use progress::Progress;
use render::{Assets, Camera, Explosion, Particle, UiAssets};

fn window_conf() -> Conf {
    Conf {
        window_title: "Meteor Blitz".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

enum Screen {
    Title,
    Select,
    Play,
    Pause,
    Over,
    /// Upgrade shop; `from_over` says whether DONE returns to the run summary.
    Upgrade { from_over: bool },
}

struct Audio {
    music: Sound,
    shot: Sound,
    hit: Sound,
    shatter: Sound,
    bomb: Sound,
    boss: Sound,
}

async fn load_frames(start: usize, count: usize) -> Vec<Texture2D> {
    let mut out = Vec::new();
    for i in start..start + count {
        out.push(load_texture(&format!("original/sprites/frame_{:03}.png", i)).await.unwrap());
    }
    out
}

async fn load_ui(name: &str) -> Texture2D {
    load_texture(&format!("assets/ui/{}.png", name)).await.unwrap()
}

async fn load_menu_assets() -> MenuAssets {
    let mut buttons = Vec::new();
    for name in ["Play", "HighScores", "Settings", "Help"] {
        let up = load_ui(&format!("button{}", name)).await;
        let down = load_ui(&format!("button{}Down", name)).await;
        buttons.push((up, down));
    }
    let menu = MenuAssets {
        space: load_texture("original/textures/space4deep.png").await.unwrap(),
        fog: load_texture("original/textures/fogRed.png").await.unwrap(),
        horizon: load_ui("horizon").await,
        title: load_ui("labelTitle").await,
        buttons: buttons.try_into().ok().unwrap(),
    };
    for tex in [&menu.space, &menu.fog, &menu.horizon, &menu.title] {
        tex.set_filter(FilterMode::Linear);
    }
    menu
}

async fn load_ui_assets() -> UiAssets {
    const ICONS: [&str; game::UPGRADE_COUNT] = ["Standard", "Fire", "Laser", "Speed", "GravityGun", "Boost"];
    let mut upgrade_icons = Vec::new();
    let mut upgrade_icons_disabled = Vec::new();
    for icon in ICONS {
        upgrade_icons.push(load_ui(&format!("buttonUpgrade{}", icon)).await);
        upgrade_icons_disabled.push(load_ui(&format!("buttonUpgrade{}Disabled", icon)).await);
    }
    UiAssets {
        upgrade_icons,
        upgrade_icons_disabled,
        upgrade_background: load_ui("buttonUpgradeBackground").await,
        captions: load_ui("captions").await,
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let assets = Assets {
        crust: load_texture("assets/terrain/crust.png").await.unwrap(),
        craters: load_texture("assets/terrain/craters.png").await.unwrap(),
        fog_red: load_texture("assets/terrain/fogRed.png").await.unwrap(),
        fog_green: load_texture("assets/terrain/fogGreen.png").await.unwrap(),
        fog_black_red: load_texture("assets/terrain/fogBlackRed.png").await.unwrap(),
        clouds: load_texture("assets/terrain/clouds.png").await.unwrap(),
        space: load_texture("assets/terrain/space4deep.png").await.unwrap(),
        ship: load_texture("original/sprites/frame_380.png").await.unwrap(),
        meteors: load_frames(0, 6).await,
        debris: load_frames(40, 4).await,
        explosion: load_frames(20, 4).await,
        planets: load_frames(60, 4).await,
        fire: load_frames(80, 4).await,
        enemy_drone: load_texture("original/sprites/frame_435.png").await.unwrap(),
        enemy_turret: load_texture("original/sprites/frame_450.png").await.unwrap(),
        enemy_slinger: load_texture("original/sprites/frame_465.png").await.unwrap(),
        bullet: load_texture("original/sprites/frame_300.png").await.unwrap(),
        ui: load_ui_assets().await,
    };
    for tex in [
        &assets.crust,
        &assets.craters,
        &assets.fog_red,
        &assets.fog_green,
        &assets.fog_black_red,
        &assets.clouds,
        &assets.space,
    ] {
        tex.set_filter(FilterMode::Linear);
    }

    let audio = Audio {
        music: load_sound("original/audio/music.wav").await.unwrap(),
        shot: load_sound("assets/sfx/shot.wav").await.unwrap(),
        hit: load_sound("assets/sfx/hit.wav").await.unwrap(),
        shatter: load_sound("assets/sfx/shatter.wav").await.unwrap(),
        bomb: load_sound("assets/sfx/bomb.wav").await.unwrap(),
        boss: load_sound("assets/sfx/boss.wav").await.unwrap(),
    };

    let mut screen = Screen::Title;
    let mut game: Option<Game> = None;
    let mut cam = Camera {
        x: game::WORLD_W * 0.5,
        y: game::WORLD_H * 0.5,
    };
    let mut particles: Vec<Particle> = Vec::new();
    let mut explosions: Vec<Explosion> = Vec::new();
    let menu_assets = load_menu_assets().await;
    let canvas = Canvas::new(&menu_assets);
    let mut progress = Progress::load();
    let mut music_on = false;
    // Ore from the finished run is banked once, when the run ends.
    let mut banked = false;

    loop {
        let dt = get_frame_time().min(0.05);

        match screen {
            Screen::Title => {
                if is_key_pressed(KeyCode::Enter) || menu::main_menu_click() == Some(MainButton::Play) {
                    screen = Screen::Select;
                }
                if is_key_pressed(KeyCode::S) {
                    game = Some(Game::new(Mode::Survival, 0, 1, progress.upgrades));
                    banked = false;
                    cam.x = game::WORLD_W * 0.5;
                    cam.y = game::WORLD_H * 0.5;
                    particles.clear();
                    screen = Screen::Play;
                    if !music_on {
                        play_sound(&audio.music, PlaySoundParams { looped: true, volume: 0.35 });
                        music_on = true;
                    }
                }
            }
            Screen::Select => {
                if is_key_pressed(KeyCode::Escape) {
                    screen = Screen::Title;
                }
                if is_key_pressed(KeyCode::U) {
                    screen = Screen::Upgrade { from_over: false };
                }
                if is_mouse_button_pressed(MouseButton::Left) {
                    let (mx, my) = mouse_position();
                    let start_x = screen_width() * 0.5 - 5.0 * 70.0 * 0.5;
                    for world in 0..6 {
                        for stage in 1..=5 {
                            let idx = world * 5 + stage - 1;
                            if idx > progress.unlocked {
                                continue;
                            }
                            let x = start_x + (stage - 1) as f32 * 70.0;
                            let y = 120.0 + world as f32 * 70.0;
                            if mx >= x && mx <= x + 60.0 && my >= y && my <= y + 60.0 {
                                game = Some(Game::new(Mode::Arcade, world, stage, progress.upgrades));
                                banked = false;
                                cam.x = game::WORLD_W * 0.5;
                                cam.y = game::WORLD_H * 0.5;
                                particles.clear();
                                screen = Screen::Play;
                                if !music_on {
                                    play_sound(&audio.music, PlaySoundParams { looped: true, volume: 0.35 });
                                    music_on = true;
                                }
                            }
                        }
                    }
                }
            }
            Screen::Play => {
                if is_key_pressed(KeyCode::Escape) {
                    screen = Screen::Pause;
                }
                if let Some(g) = game.as_mut() {
                    let input = read_input(g);
                    g.update(dt, &input);
                    cam.follow(g.player.x, g.player.y, dt);
                    handle_events(g, &audio, &mut particles, &mut explosions);
                    if g.over || g.won {
                        if g.won && g.mode == Mode::Arcade {
                            progress.unlocked = progress.unlocked.max(g.world * 5 + g.stage);
                        }
                        if !banked {
                            progress.best = progress.best.max(g.score);
                            progress.ore += g.player.ore;
                            progress.save();
                            banked = true;
                        }
                        screen = Screen::Over;
                    }
                }
            }
            Screen::Pause => {
                if is_key_pressed(KeyCode::Escape) {
                    screen = Screen::Play;
                }
            }
            Screen::Over => {
                if is_key_pressed(KeyCode::Enter) {
                    if let Some(g) = game.as_ref() {
                        game = Some(Game::new(g.mode, g.world, g.stage, progress.upgrades));
                        banked = false;
                        cam.x = game::WORLD_W * 0.5;
                        cam.y = game::WORLD_H * 0.5;
                        particles.clear();
                        screen = Screen::Play;
                    }
                }
                if is_key_pressed(KeyCode::Escape) {
                    screen = Screen::Select;
                }
                if is_key_pressed(KeyCode::U) {
                    screen = Screen::Upgrade { from_over: true };
                }
            }
            Screen::Upgrade { from_over } => {
                let keys = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6];
                for (slot, key) in keys.into_iter().enumerate() {
                    if is_key_pressed(key) && progress.buy(slot) {
                        play_sound(&audio.shatter, PlaySoundParams { looped: false, volume: 0.5 });
                    }
                }
                let (tiles, done) = render::upgrade_layout();
                let mut close = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape);
                if is_mouse_button_pressed(MouseButton::Left) {
                    let m = vec2(mouse_position().0, mouse_position().1);
                    for (slot, r) in tiles.iter().enumerate() {
                        if r.contains(m) && progress.buy(slot) {
                            play_sound(&audio.shatter, PlaySoundParams { looped: false, volume: 0.5 });
                        }
                    }
                    close |= done.contains(m);
                }
                if close {
                    screen = if from_over { Screen::Over } else { Screen::Select };
                }
            }
        }

        clear_background(Color::new(0.02, 0.03, 0.06, 1.0));

        if let Some(g) = game.as_ref() {
            render::draw_terrain(&assets, &cam, g.world);
            render::draw_planets(&assets, &cam);
            render::draw_entities(g, &cam, &assets);
            render::draw_explosions(&mut explosions, &cam, &assets, dt);
            render::draw_particles(&mut particles, &cam, dt);
            render::draw_hud(g);
        }

        match screen {
            Screen::Title => {
                canvas.begin();
                menu::draw_main_menu(&canvas, &menu_assets);
                canvas.end();
            }
            Screen::Select => render::draw_stage_select(progress.unlocked),
            Screen::Upgrade { .. } => render::draw_upgrade(&assets.ui, &progress),
            Screen::Pause => render::draw_pause(),
            Screen::Over => {
                if let Some(g) = game.as_ref() {
                    render::draw_over(g.score, g.won);
                }
            }
            _ => {}
        }

        next_frame().await;
    }
}

fn read_input(g: &Game) -> Input {
    let (mx, my) = mouse_position();
    let aim_x = mx - screen_width() * 0.5;
    let aim_y = my - screen_height() * 0.5;
    let move_x = (is_key_down(KeyCode::D) as i32 - is_key_down(KeyCode::A) as i32) as f32;
    let move_y = (is_key_down(KeyCode::S) as i32 - is_key_down(KeyCode::W) as i32) as f32;
    let mut weapon = g.player.weapon;
    if is_key_pressed(KeyCode::Key1) {
        weapon = 0;
    }
    if is_key_pressed(KeyCode::Key2) {
        weapon = 1;
    }
    if is_key_pressed(KeyCode::Key3) {
        weapon = 2;
    }
    if is_key_pressed(KeyCode::Key4) {
        weapon = 3;
    }
    Input {
        move_x,
        move_y,
        aim_x,
        aim_y,
        fire: is_mouse_button_down(MouseButton::Left),
        gravity: is_mouse_button_down(MouseButton::Right),
        dash: is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
        bomb: is_key_pressed(KeyCode::B),
        dash_dir_x: 0.0,
        dash_dir_y: 0.0,
        weapon,
    }
}

fn handle_events(g: &mut Game, audio: &Audio, particles: &mut Vec<Particle>, explosions: &mut Vec<Explosion>) {
    for event in g.events.drain(..) {
        match event {
            Event::Fire(weapon, x, y) => {
                play_sound(&audio.shot, PlaySoundParams { looped: false, volume: if weapon == 1 { 0.25 } else { 0.4 } });
                render::burst(particles, x, y, 2, Color::new(0.4, 0.9, 1.0, 1.0), 60.0);
            }
            Event::Shatter(x, y, kind, mass) => {
                play_sound(&audio.shatter, PlaySoundParams { looped: false, volume: 0.5 });
                let color = match kind {
                    1 => Color::new(0.55, 0.85, 1.0, 1.0),
                    2 => Color::new(1.0, 0.55, 0.35, 1.0),
                    _ => Color::new(1.0, 0.75, 0.25, 1.0),
                };
                render::burst(particles, x, y, 8 + (mass * 2.0) as usize, color, 220.0);
                render::explosion_burst(explosions, x, y, 2, 0.6 + mass * 0.08);
            }
            Event::PlayerHit(_) => {
                play_sound(&audio.hit, PlaySoundParams { looped: false, volume: 0.6 });
            }
            Event::Dash(x, y) => {
                render::burst(particles, x, y, 6, Color::new(0.15, 0.85, 1.0, 1.0), 120.0);
            }
            Event::Bomb(x, y) => {
                play_sound(&audio.bomb, PlaySoundParams { looped: false, volume: 0.7 });
                render::burst(particles, x, y, 40, Color::new(1.0, 0.75, 0.25, 1.0), 420.0);
                render::explosion_burst(explosions, x, y, 6, 1.4);
            }
            Event::Grab(x, y) => {
                render::burst(particles, x, y, 4, Color::new(0.15, 0.85, 1.0, 1.0), 80.0);
            }
            Event::Sling(x, y) => {
                render::burst(particles, x, y, 6, Color::new(0.15, 0.85, 1.0, 1.0), 160.0);
            }
            Event::Kill(x, y) => {
                render::burst(particles, x, y, 4, Color::new(1.0, 1.0, 1.0, 1.0), 140.0);
            }
        }
    }
}
