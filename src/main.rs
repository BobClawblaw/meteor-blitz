//! Meteor Blitz — port. Window, screens, input, audio, and loop.

mod game;
mod menu;
mod progress;
mod render;

use std::collections::HashMap;

use macroquad::prelude::*;
use macroquad::audio::{load_sound, play_sound, PlaySoundParams, Sound};

use game::{Event, Game, Input, Mode};
use menu::{Canvas, MenuAssets, MenuButton, MenuScreen};
use progress::Progress;
use render::{Assets, Camera, Explosion, Particle, UiAssets};

fn window_conf() -> Conf {
    // MB_WINDOW=480x320 gives a 1:1 window for pixel comparisons with the
    // original's frames (the F12 export is then exactly 480x320).
    let (width, height, high_dpi) = match std::env::var("MB_WINDOW").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        Some((w, h)) => (w, h, false),
        None => (1280, 720, true),
    };
    Conf {
        window_title: "Meteor Blitz".to_owned(),
        window_width: width,
        window_height: height,
        high_dpi,
        sample_count: 4,
        ..Default::default()
    }
}

/// The original's menu screens, by `Director` screen id.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    /// MainMenuScreen (2)
    Main,
    /// PlayScreen (3): arcade or survival
    Mode,
    /// LevelSelectScreen (4): world
    World,
    /// StageSelectScreen (14): stage of `world`
    Stage(usize),
}

enum Screen {
    Menu(MenuKind),
    Play,
    Pause,
    Over,
    /// Upgrade shop; `from_over` says whether DONE returns to the run summary.
    Upgrade { from_over: bool },
}

/// Where a menu goes once its fade-out finishes.
#[derive(Clone, Copy)]
enum Next {
    Menu(MenuKind),
    Game(Mode, usize, usize),
}

struct Audio {
    music: Sound,
    shot: Sound,
    hit: Sound,
    shatter: Sound,
    bomb: Sound,
    boss: Sound,
}

/// Menu art by file stem (`assets/ui/<name>.png`).
type Ui = HashMap<String, Texture2D>;

async fn load_frames(start: usize, count: usize) -> Vec<Texture2D> {
    let mut out = Vec::new();
    for i in start..start + count {
        out.push(load_texture(&format!("original/sprites/frame_{:03}.png", i)).await.unwrap());
    }
    out
}

async fn load_ui(name: &str) -> Texture2D {
    let tex = load_texture(&format!("assets/ui/{}.png", name)).await.unwrap();
    tex.set_filter(FilterMode::Linear);
    tex
}

async fn load_ui_set() -> Ui {
    let mut names: Vec<String> = Vec::new();
    for b in ["Play", "HighScores", "Settings", "Help", "Arcade", "Survival", "Back"] {
        names.push(format!("button{}", b));
        names.push(format!("button{}Down", b));
    }
    for label in ["labelTitle", "labelSelectMode", "labelSelectWorld", "labelSelectStage"] {
        names.push(label.to_owned());
    }
    for world in 1..=6 {
        names.push(format!("buttonArcade{}", world));
        if world > 1 {
            names.push(format!("buttonArcade{}Locked", world));
        }
        for stage in 1..=5 {
            names.push(format!("buttonStage{}.{}", world, stage));
            if stage > 1 {
                names.push(format!("buttonStage{}.{}.Locked", world, stage));
            }
        }
    }
    let mut ui = Ui::new();
    for name in names {
        let tex = load_ui(&name).await;
        ui.insert(name, tex);
    }
    ui
}

async fn load_menu_assets() -> MenuAssets {
    let menu = MenuAssets {
        space: load_texture("original/textures/space4deep.png").await.unwrap(),
        fog: load_texture("original/textures/fogRed.png").await.unwrap(),
        horizon: load_ui("horizon").await,
    };
    for tex in [&menu.space, &menu.fog] {
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

/// `Global::BACK_BUTTON_X/Y`: the back arrow sits at (2, 2).
const BACK_XY: f32 = 2.0;

fn button(ui: &Ui, up: &str, down: &str, x: f32, y: f32, w: f32, h: f32, event: u8) -> MenuButton {
    MenuButton { up: ui[up].clone(), down: ui[down].clone(), rect: Rect::new(x, y, w, h), event }
}

/// Builds a menu screen with the original's layout (rects and events from
/// the screen constructors in the iOS binary).
fn build_menu(kind: MenuKind, ui: &Ui, progress: &Progress, fade_backdrop: bool) -> MenuScreen {
    let back = |event| button(ui, "buttonBack", "buttonBackDown", BACK_XY, BACK_XY, 34.0, 34.0, event);
    match kind {
        MenuKind::Main => MenuScreen::new(
            ui["labelTitle"].clone(),
            15.0,
            vec![
                button(ui, "buttonPlay", "buttonPlayDown", 160.0, 176.0, 160.0, 128.0, 0),
                button(ui, "buttonHighScores", "buttonHighScoresDown", 16.0, 176.0, 128.0, 128.0, 1),
                button(ui, "buttonSettings", "buttonSettingsDown", 336.0, 176.0, 127.0, 57.0, 2),
                button(ui, "buttonHelp", "buttonHelpDown", 336.0, 246.0, 127.0, 57.0, 3),
            ],
            fade_backdrop,
        ),
        MenuKind::Mode => MenuScreen::new(
            ui["labelSelectMode"].clone(),
            0.0,
            vec![
                back(2),
                button(ui, "buttonArcade", "buttonArcadeDown", 47.0, 118.0, 175.0, 128.0, 0),
                button(ui, "buttonSurvival", "buttonSurvivalDown", 258.0, 118.0, 175.0, 128.0, 1),
            ],
            fade_backdrop,
        ),
        MenuKind::World => {
            // Columns at 30 / 176.5 / 320 (the middle one 127 wide), rows at 55 / 185.
            let slots = [(30.0, 55.0, 130.0), (176.5, 54.0, 127.0), (320.0, 55.0, 130.0), (30.0, 185.0, 130.0), (176.5, 185.0, 127.0), (320.0, 185.0, 130.0)];
            let mut buttons = vec![back(7)];
            for (i, (x, y, w)) in slots.into_iter().enumerate() {
                let world = i + 1;
                let unlocked = i == 0 || progress.unlocked >= i * 5;
                let name = if unlocked { format!("buttonArcade{}", world) } else { format!("buttonArcade{}Locked", world) };
                buttons.push(button(ui, &name, &name, x, y, w, 114.0, if unlocked { world as u8 } else { 0 }));
            }
            MenuScreen::new(ui["labelSelectWorld"].clone(), -9.0, buttons, fade_backdrop)
        }
        MenuKind::Stage(world) => {
            let slots = [(30.0, 55.0), (176.5, 54.0), (320.0, 55.0), (100.0, 185.0), (250.0, 185.0)];
            let mut buttons = vec![back(6)];
            for (i, (x, y)) in slots.into_iter().enumerate() {
                let stage = i + 1;
                let unlocked = i == 0 || progress.unlocked >= world * 5 + i;
                let name = if unlocked {
                    format!("buttonStage{}.{}", world + 1, stage)
                } else {
                    format!("buttonStage{}.{}.Locked", world + 1, stage)
                };
                buttons.push(button(ui, &name, &name, x, y, 127.0, 121.0, if unlocked { stage as u8 } else { 0 }));
            }
            MenuScreen::new(ui["labelSelectStage"].clone(), -9.0, buttons, fade_backdrop)
        }
    }
}

/// The screen a menu event leads to (`handleEvent` of each screen class).
fn menu_event(kind: MenuKind, event: u8) -> Option<Next> {
    match (kind, event) {
        (MenuKind::Main, 0) => Some(Next::Menu(MenuKind::Mode)),
        (MenuKind::Mode, 0) => Some(Next::Menu(MenuKind::World)),
        (MenuKind::Mode, 1) => Some(Next::Game(Mode::Survival, 0, 1)),
        (MenuKind::Mode, 2) => Some(Next::Menu(MenuKind::Main)),
        (MenuKind::World, 7) => Some(Next::Menu(MenuKind::Mode)),
        (MenuKind::World, w @ 1..=6) => Some(Next::Menu(MenuKind::Stage(w as usize - 1))),
        (MenuKind::Stage(_), 6) => Some(Next::Menu(MenuKind::World)),
        (MenuKind::Stage(world), s @ 1..=5) => Some(Next::Game(Mode::Arcade, world, s as usize)),
        _ => None,
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

    let ui = load_ui_set().await;
    let menu_assets = load_menu_assets().await;
    let mut canvas = Canvas::new(&menu_assets);
    let mut progress = Progress::load();

    let mut screen = Screen::Menu(MenuKind::Main);
    let mut menu_screen = build_menu(MenuKind::Main, &ui, &progress, true);
    let mut menu_next: Option<Next> = None;
    let mut game: Option<Game> = None;
    let mut cam = Camera {
        x: game::WORLD_W * 0.5,
        y: game::WORLD_H * 0.5,
    };
    let mut particles: Vec<Particle> = Vec::new();
    let mut explosions: Vec<Explosion> = Vec::new();
    let mut music_on = false;
    // Ore from the finished run is banked once, when the run ends.
    let mut banked = false;
    let mut frame_count: u32 = 0;

    loop {
        let dt = get_frame_time().min(0.05);

        match screen {
            Screen::Menu(kind) => {
                canvas.update(dt);
                if let Some(event) = menu_screen.update(dt) {
                    if let Some(next) = menu_event(kind, event) {
                        menu_next = Some(next);
                        menu_screen.fx.leave();
                    }
                }
                if kind == MenuKind::Main && menu_screen.fx.ready() && is_key_pressed(KeyCode::Enter) {
                    menu_next = Some(Next::Menu(MenuKind::Mode));
                    menu_screen.fx.leave();
                }
                if let MenuKind::Stage(_) = kind {
                    if is_key_pressed(KeyCode::U) {
                        screen = Screen::Upgrade { from_over: false };
                    }
                }
                if menu_screen.fx.done() {
                    match menu_next.take() {
                        Some(Next::Menu(next)) => {
                            menu_screen = build_menu(next, &ui, &progress, false);
                            screen = Screen::Menu(next);
                        }
                        Some(Next::Game(mode, world, stage)) => {
                            game = Some(Game::new(mode, world, stage, progress.upgrades));
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
                        None => {
                            menu_screen = build_menu(kind, &ui, &progress, false);
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
                    let kind = match game.as_ref() {
                        Some(g) if g.mode == Mode::Arcade => MenuKind::Stage(g.world),
                        _ => MenuKind::Mode,
                    };
                    game = None;
                    menu_screen = build_menu(kind, &ui, &progress, true);
                    screen = Screen::Menu(kind);
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
                    screen = if from_over {
                        Screen::Over
                    } else {
                        let kind = match game.as_ref() {
                            Some(g) if g.mode == Mode::Arcade => MenuKind::Stage(g.world),
                            _ => MenuKind::Mode,
                        };
                        menu_screen = build_menu(kind, &ui, &progress, false);
                        Screen::Menu(kind)
                    };
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
            Screen::Menu(_) => {
                canvas.begin();
                menu_screen.draw(&canvas, &menu_assets);
                canvas.end();
            }
            Screen::Upgrade { .. } => render::draw_upgrade(&assets.ui, &progress),
            Screen::Pause => render::draw_pause(),
            Screen::Over => {
                if let Some(g) = game.as_ref() {
                    render::draw_over(g.score, g.won);
                }
            }
            Screen::Play => {}
        }

        // F12 saves the frame for pixel comparison against emulator frames;
        // MB_SHOT=<file> saves the 20th frame there and exits (for scripts).
        frame_count += 1;
        let auto_shot = std::env::var("MB_SHOT").ok().filter(|_| frame_count == 20);
        if is_key_pressed(KeyCode::F12) || auto_shot.is_some() {
            let _ = std::fs::create_dir_all("shots");
            let name = auto_shot.clone().unwrap_or_else(|| format!("shots/port_{}.png", (get_time() * 1000.0) as u64));
            get_screen_data().export_png(&name);
            if auto_shot.is_some() {
                std::process::exit(0);
            }
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
