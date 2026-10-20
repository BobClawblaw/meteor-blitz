//! Meteor Blitz — a port of the 2009 iPhone game. Window, screens, and loop.

mod arrow;
mod bonus;
mod boss;
mod depot;
mod enemies;
mod gfx;
mod hud;
mod menu;
mod mine;
mod particles;
mod play;
mod progress;
mod shyguy;
mod snake;
mod waves;

use std::collections::HashMap;

use macroquad::audio::{load_sound, play_sound, stop_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;

use gfx::Canvas;
use hud::{Font, HudAssets};
use menu::{MenuAssets, MenuBackdrop, MenuButton, MenuScreen};
use play::{Play, PlayAssets, RockArt};
use progress::Progress;

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

/// sounds.raw holds effects 1-17 (music.raw is id 0).
const SOUND_COUNT: usize = 17;
/// Audio::play id for button presses.
const SOUND_BUTTON: usize = 9;
// The original's untouched Settings screen: music full, effects at half.
const MUSIC_VOLUME: f32 = 1.0;
const EFFECT_VOLUME: f32 = 0.5;

/// `Global::level` for survival (worlds are 0-5).
const SURVIVAL: usize = 6;

enum Screen {
    Menu(MenuKind),
    Game,
}

/// Where a menu goes once its fade-out finishes.
#[derive(Clone, Copy)]
enum Next {
    Menu(MenuKind),
    /// World (or SURVIVAL) and stage.
    Game(usize, usize),
}

/// Menu art by file stem (`assets/ui/<name>.png`).
type Ui = HashMap<String, Texture2D>;

async fn load_ui(name: &str) -> Texture2D {
    let tex = load_texture(&format!("assets/ui/{}.png", name))
        .await
        .unwrap();
    tex.set_filter(FilterMode::Linear);
    tex
}

async fn load_ui_set() -> Ui {
    let mut names: Vec<String> = Vec::new();
    for b in [
        "Play",
        "HighScores",
        "Settings",
        "Help",
        "Arcade",
        "Survival",
        "Back",
    ] {
        names.push(format!("button{}", b));
        names.push(format!("button{}Down", b));
    }
    for label in [
        "labelTitle",
        "labelSelectMode",
        "labelSelectWorld",
        "labelSelectStage",
    ] {
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
        space: load_texture("original/textures/space4deep.png")
            .await
            .unwrap(),
        fog: load_texture("original/textures/fogRed.png").await.unwrap(),
        horizon: load_ui("horizon").await,
    };
    for tex in [&menu.space, &menu.fog] {
        tex.set_filter(FilterMode::Linear);
    }
    menu
}

/// `Global::BACK_BUTTON_X/Y`: the back arrow sits at (2, 2).
const BACK_XY: f32 = 2.0;

fn button(ui: &Ui, up: &str, down: &str, x: f32, y: f32, w: f32, h: f32, event: u8) -> MenuButton {
    MenuButton {
        up: ui[up].clone(),
        down: ui[down].clone(),
        rect: Rect::new(x, y, w, h),
        event,
    }
}

/// Builds a menu screen with the original's layout (rects and events from
/// the screen constructors in the iOS binary).
fn build_menu(kind: MenuKind, ui: &Ui, progress: &Progress, fade_backdrop: bool) -> MenuScreen {
    let back = |event| {
        button(
            ui,
            "buttonBack",
            "buttonBackDown",
            BACK_XY,
            BACK_XY,
            34.0,
            34.0,
            event,
        )
    };
    match kind {
        MenuKind::Main => MenuScreen::new(
            ui["labelTitle"].clone(),
            15.0,
            vec![
                button(
                    ui,
                    "buttonPlay",
                    "buttonPlayDown",
                    160.0,
                    176.0,
                    160.0,
                    128.0,
                    0,
                ),
                button(
                    ui,
                    "buttonHighScores",
                    "buttonHighScoresDown",
                    16.0,
                    176.0,
                    128.0,
                    128.0,
                    1,
                ),
                button(
                    ui,
                    "buttonSettings",
                    "buttonSettingsDown",
                    336.0,
                    176.0,
                    127.0,
                    57.0,
                    2,
                ),
                button(
                    ui,
                    "buttonHelp",
                    "buttonHelpDown",
                    336.0,
                    246.0,
                    127.0,
                    57.0,
                    3,
                ),
            ],
            fade_backdrop,
        ),
        MenuKind::Mode => MenuScreen::new(
            ui["labelSelectMode"].clone(),
            0.0,
            vec![
                back(2),
                button(
                    ui,
                    "buttonArcade",
                    "buttonArcadeDown",
                    47.0,
                    118.0,
                    175.0,
                    128.0,
                    0,
                ),
                button(
                    ui,
                    "buttonSurvival",
                    "buttonSurvivalDown",
                    258.0,
                    118.0,
                    175.0,
                    128.0,
                    1,
                ),
            ],
            fade_backdrop,
        ),
        MenuKind::World => {
            // Columns at 30 / 176.5 / 320 (the middle one 127 wide), rows at 55 / 185.
            let slots = [
                (30.0, 55.0, 130.0),
                (176.5, 54.0, 127.0),
                (320.0, 55.0, 130.0),
                (30.0, 185.0, 130.0),
                (176.5, 185.0, 127.0),
                (320.0, 185.0, 130.0),
            ];
            let mut buttons = vec![back(7)];
            for (i, (x, y, w)) in slots.into_iter().enumerate() {
                let world = i + 1;
                let unlocked = i == 0 || progress.unlocked >= i * 5;
                let name = if unlocked {
                    format!("buttonArcade{}", world)
                } else {
                    format!("buttonArcade{}Locked", world)
                };
                buttons.push(button(
                    ui,
                    &name,
                    &name,
                    x,
                    y,
                    w,
                    114.0,
                    if unlocked { world as u8 } else { 0 },
                ));
            }
            MenuScreen::new(ui["labelSelectWorld"].clone(), -9.0, buttons, fade_backdrop)
        }
        MenuKind::Stage(world) => {
            let slots = [
                (30.0, 55.0),
                (176.5, 54.0),
                (320.0, 55.0),
                (100.0, 185.0),
                (250.0, 185.0),
            ];
            let mut buttons = vec![back(6)];
            for (i, (x, y)) in slots.into_iter().enumerate() {
                let stage = i + 1;
                let unlocked = i == 0 || progress.unlocked >= world * 5 + i;
                let name = if unlocked {
                    format!("buttonStage{}.{}", world + 1, stage)
                } else {
                    format!("buttonStage{}.{}.Locked", world + 1, stage)
                };
                buttons.push(button(
                    ui,
                    &name,
                    &name,
                    x,
                    y,
                    127.0,
                    121.0,
                    if unlocked { stage as u8 } else { 0 },
                ));
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
        (MenuKind::Mode, 1) => Some(Next::Game(SURVIVAL, 1)),
        (MenuKind::Mode, 2) => Some(Next::Menu(MenuKind::Main)),
        (MenuKind::World, 7) => Some(Next::Menu(MenuKind::Mode)),
        (MenuKind::World, w @ 1..=6) => Some(Next::Menu(MenuKind::Stage(w as usize - 1))),
        (MenuKind::Stage(_), 6) => Some(Next::Menu(MenuKind::World)),
        (MenuKind::Stage(world), s @ 1..=5) => Some(Next::Game(world, s as usize)),
        _ => None,
    }
}

/// Sprites unpacked from the original sheet by `tools/decode_sheet.py`.
async fn load_sprite(name: &str) -> Texture2D {
    let tex = load_texture(&format!("original/sprites/{}.png", name))
        .await
        .unwrap();
    tex.set_filter(FilterMode::Linear);
    tex
}

async fn load_texture_file(name: &str) -> Texture2D {
    let tex = load_texture(&format!("original/textures/{}.png", name))
        .await
        .unwrap();
    tex.set_filter(FilterMode::Linear);
    tex
}

/// GameScreen::loadTextures: each element's small-rock atlas and the medium,
/// large and extra-large rotation frames (EnemyManager::sharedNew).
async fn load_rock_art(atlas: &str, prefix: &str) -> RockArt {
    let frames = |family: u32, count: u32| {
        let names: Vec<String> = (1..=count)
            .map(|i| format!("{}{}.{}", prefix, family, i))
            .collect();
        names
    };
    let medium_family = if prefix == "rock" {
        "rock3A".to_owned()
    } else {
        format!("{}3", prefix)
    };
    let mut medium = Vec::new();
    for i in 1..=30 {
        medium.push(load_sprite(&format!("{}.{}", medium_family, i)).await);
    }
    let mut large = Vec::new();
    for name in frames(2, 23) {
        large.push(load_sprite(&name).await);
    }
    let mut extra_large = Vec::new();
    for name in frames(1, 30) {
        extra_large.push(load_sprite(&name).await);
    }
    RockArt {
        small: load_sprite(atlas).await,
        medium,
        large,
        extra_large,
    }
}

async fn load_play_assets() -> PlayAssets {
    let mut ship = Vec::new();
    let mut ship_ready = Vec::new();
    for i in 1..=9 {
        ship.push(load_sprite(&format!("ship.{}", i)).await);
        ship_ready.push(load_sprite(&format!("shipBoostReady.{}", i)).await);
    }
    // GameScreen::GameScreen: Background(base, decal) per world, survival in space.
    let pairs = [
        ("blue", "clouds"),
        ("fire", "crust"),
        ("green", "clouds"),
        ("sprawl", "fogBlackRed"),
        ("craters", "fogGreen"),
        ("space4deep", "fogRed"),
        ("space4deep", "fogRed"),
    ];
    let mut backdrops = Vec::new();
    for (base, decal) in pairs {
        backdrops.push((
            load_texture_file(base).await,
            load_texture_file(decal).await,
        ));
    }
    let font = Font::new(
        load_ui("ngage").await,
        &load_file("original/app/ngage.dat").await.unwrap(),
    );
    PlayAssets {
        hud: HudAssets {
            font,
            ship_counter: load_sprite("shipCounter").await,
            bomb_counter: load_sprite("bombCounter").await,
            ring: load_sprite("bonus").await,
            pause: load_sprite("pause").await,
        },
        rocks: [
            load_rock_art("rock4A", "rock").await,
            load_rock_art("freeze4", "freeze").await,
            load_rock_art("cinder4", "cinder").await,
        ],
        ship,
        ship_ready,
        shield: load_sprite("shield").await,
        aim: load_sprite("aim").await,
        cannon: load_sprite("fire100").await,
        spark: load_sprite("fire200").await,
        depot: load_sprite("depot").await,
        mine: load_sprite("mine").await,
        arrow: load_sprite("arrow").await,
        shy_guy: load_sprite("shyGuyFrames").await,
        snake_head: load_sprite("pillHead.1").await,
        snake_body: load_sprite("pillBody.1").await,
        boss: {
            let mut frames = Vec::new();
            for i in 1..=8 {
                frames.push(load_sprite(&format!("bossInside.{}", i)).await);
            }
            frames
        },
        boss_piece: load_sprite("ball").await,
        depot_label: load_sprite("labelBonus").await,
        power: load_sprite("bigBonus").await,
        power_labels: [
            load_sprite("labelLife").await,
            load_sprite("labelShield").await,
            load_sprite("labelBomb").await,
            load_sprite("labelSlow").await,
            load_sprite("labelRings").await,
        ],
        systems: load_file("original/app/systems.dat")
            .await
            .expect("systems.dat"),
        backdrops,
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let ui = load_ui_set().await;
    let menu_assets = load_menu_assets().await;
    let play_assets = load_play_assets().await;
    let music = load_sound("original/audio/music.wav").await.unwrap();
    let mut sounds: Vec<Option<Sound>> = vec![None];
    for id in 1..=SOUND_COUNT {
        sounds.push(Some(
            load_sound(&format!("original/audio/sound_{:02}.wav", id))
                .await
                .unwrap(),
        ));
    }
    let play_effect = |id: usize| {
        if let Some(Some(sound)) = sounds.get(id) {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume: EFFECT_VOLUME,
                },
            );
        }
    };
    let canvas = Canvas::new();
    let mut backdrop = MenuBackdrop::new();
    let mut progress = Progress::load();

    let mut screen = Screen::Menu(MenuKind::Main);
    let mut menu_screen = build_menu(MenuKind::Main, &ui, &progress, true);
    let mut menu_next: Option<Next> = None;
    let mut game: Option<Play> = None;
    let mut music_on = false;
    let mut frame_count: u32 = 0;
    // MB_START=<world>.<stage> (1-based, world 7 = survival) boots straight
    // into a stage, for scripted frame comparisons.
    if let Some((w, s)) = std::env::var("MB_START").ok().and_then(|v| {
        let (w, s) = v.split_once('.')?;
        Some((w.parse::<usize>().ok()?, s.parse::<usize>().ok()?))
    }) {
        game = Some(Play::new(
            w.saturating_sub(1).min(SURVIVAL),
            s,
            progress.best,
            &play_assets,
        ));
        screen = Screen::Game;
    }

    loop {
        let dt = get_frame_time().min(0.05);

        match screen {
            Screen::Menu(kind) => {
                backdrop.update(dt);
                if let Some(event) = menu_screen.update(dt) {
                    play_effect(SOUND_BUTTON);
                    if let Some(next) = menu_event(kind, event) {
                        menu_next = Some(next);
                        menu_screen.fx.leave();
                    }
                }
                if kind == MenuKind::Main
                    && menu_screen.fx.ready()
                    && is_key_pressed(KeyCode::Enter)
                {
                    menu_next = Some(Next::Menu(MenuKind::Mode));
                    menu_screen.fx.leave();
                }
                if menu_screen.fx.done() {
                    match menu_next.take() {
                        Some(Next::Menu(next)) => {
                            menu_screen = build_menu(next, &ui, &progress, false);
                            screen = Screen::Menu(next);
                        }
                        Some(Next::Game(world, stage)) => {
                            game = Some(Play::new(world, stage, progress.best, &play_assets));
                            screen = Screen::Game;
                            if !music_on {
                                play_sound(
                                    &music,
                                    PlaySoundParams {
                                        looped: true,
                                        volume: MUSIC_VOLUME,
                                    },
                                );
                                music_on = true;
                            }
                        }
                        None => menu_screen = build_menu(kind, &ui, &progress, false),
                    }
                }
            }
            Screen::Game => {
                if let Some(g) = game.as_mut() {
                    g.update(dt);
                    for id in g.sounds.drain(..) {
                        play_effect(id);
                    }
                }
                // Until the pause and results popups are ported: Escape leaves
                // to stage select, as does Enter once the stage is over.
                let over = game.as_ref().and_then(|g| g.result());
                if over == Some(true) {
                    if let Some(g) = game.as_ref() {
                        if g.world < SURVIVAL {
                            progress.unlocked = progress.unlocked.max(g.world * 5 + g.stage);
                            progress.best = progress.best.max(g.score);
                            progress.save();
                        }
                    }
                }
                if is_key_pressed(KeyCode::Escape)
                    || (over.is_some() && is_key_pressed(KeyCode::Enter))
                {
                    let kind = match game.as_ref() {
                        Some(g) if g.world < SURVIVAL => MenuKind::Stage(g.world),
                        _ => MenuKind::Mode,
                    };
                    game = None;
                    // The music belongs to the game screen.
                    stop_sound(&music);
                    music_on = false;
                    menu_screen = build_menu(kind, &ui, &progress, true);
                    screen = Screen::Menu(kind);
                }
            }
        }

        canvas.begin();
        match screen {
            Screen::Menu(_) => menu_screen.draw(&canvas, &menu_assets, &backdrop),
            Screen::Game => {
                if let Some(g) = game.as_ref() {
                    g.draw(&canvas, &play_assets);
                }
            }
        }
        canvas.end();

        // F12 saves the frame for pixel comparison against emulator frames;
        // MB_SHOT=<file> saves frame MB_SHOT_FRAME (default 20) there and exits.
        frame_count += 1;
        let shot_frame = std::env::var("MB_SHOT_FRAME")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        let auto_shot = std::env::var("MB_SHOT")
            .ok()
            .filter(|_| frame_count == shot_frame);
        if is_key_pressed(KeyCode::F12) || auto_shot.is_some() {
            let _ = std::fs::create_dir_all("shots");
            let name = auto_shot
                .clone()
                .unwrap_or_else(|| format!("shots/port_{}.png", (get_time() * 1000.0) as u64));
            get_screen_data().export_png(&name);
            if auto_shot.is_some() {
                std::process::exit(0);
            }
        }
        next_frame().await;
    }
}
