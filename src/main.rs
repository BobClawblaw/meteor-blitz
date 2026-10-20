//! Meteor Blitz — modern port. Window, screens, input, audio, and loop.

mod game;
mod render;

use macroquad::prelude::*;
use macroquad::audio::{load_sound, play_sound, PlaySoundParams, Sound};

use game::{Event, Game, Input, Mode};
use render::{Assets, Camera, Particle};

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
}

struct Audio {
    music: Sound,
    shot: Sound,
    hit: Sound,
    shatter: Sound,
    bomb: Sound,
    boss: Sound,
}

fn load_best() -> u64 {
    std::fs::read_to_string("best.txt")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn save_best(score: u64) {
    let _ = std::fs::write("best.txt", score.to_string());
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
    let mut unlocked: usize = 0;
    let mut best = load_best();
    let mut music_on = false;

    loop {
        let dt = get_frame_time().min(0.05);

        match screen {
            Screen::Title => {
                if is_key_pressed(KeyCode::Enter) {
                    screen = Screen::Select;
                }
                if is_key_pressed(KeyCode::S) {
                    game = Some(Game::new(Mode::Survival, 0, 1));
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
                if is_mouse_button_pressed(MouseButton::Left) {
                    let (mx, my) = mouse_position();
                    let start_x = screen_width() * 0.5 - 5.0 * 70.0 * 0.5;
                    for world in 0..6 {
                        for stage in 1..=5 {
                            let idx = world * 5 + stage - 1;
                            if idx > unlocked {
                                continue;
                            }
                            let x = start_x + (stage - 1) as f32 * 70.0;
                            let y = 120.0 + world as f32 * 70.0;
                            if mx >= x && mx <= x + 60.0 && my >= y && my <= y + 60.0 {
                                game = Some(Game::new(Mode::Arcade, world, stage));
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
                    handle_events(g, &audio, &mut particles);
                    if g.over {
                        if g.score > best {
                            best = g.score;
                            save_best(best);
                        }
                        screen = Screen::Over;
                    } else if g.won {
                        if g.mode == Mode::Arcade {
                            let idx = g.world * 5 + g.stage;
                            if idx > unlocked {
                                unlocked = idx;
                            }
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
                        game = Some(Game::new(g.mode, g.world, g.stage));
                        cam.x = game::WORLD_W * 0.5;
                        cam.y = game::WORLD_H * 0.5;
                        particles.clear();
                        screen = Screen::Play;
                    }
                }
                if is_key_pressed(KeyCode::Escape) {
                    screen = Screen::Select;
                }
            }
        }

        clear_background(Color::new(0.02, 0.03, 0.06, 1.0));

        if let Some(g) = game.as_ref() {
            render::draw_terrain(&assets, &cam, g.world);
            render::draw_entities(g, &cam);
            render::draw_particles(&mut particles, &cam, dt);
            render::draw_hud(g);
        }

        match screen {
            Screen::Title => render::draw_title(best),
            Screen::Select => render::draw_stage_select(unlocked),
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

fn handle_events(g: &mut Game, audio: &Audio, particles: &mut Vec<Particle>) {
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
