//! Lifetime stats and achievements (port addition; the original had only
//! the score tables). `Play` counts what happens in a game into a `Tally`;
//! the main loop takes it every frame and adds it to `Stats`, which unlocks
//! achievements, shows a toast for each, and is saved to `stats.txt`. The
//! main menu's "Stats & achievements" link opens `StatsScreen`, with a
//! Stats tab and an Achievements tab. The tutorial counts nothing.

use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::gfx::{self, edge, Canvas, CANVAS_H, CANVAS_W};
use crate::hud::{Align, Font};
use crate::pad::{self, Button};

const STATS_PATH: &str = "stats.txt";

/// What one game did since the main loop last took it.
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct Tally {
    /// Games started (Play::new counts its own).
    pub games: u32,
    pub rocks: u32,
    /// Extra-large rocks (size 3).
    pub big_rocks: u32,
    /// Lobbers, seekers, towers, arrows, snakes, shy guys and mines.
    pub enemies: u32,
    pub depots: u32,
    pub bosses: u32,
    pub rings: u32,
    pub bombs: u32,
    pub deaths: u32,
    /// Enemies thrown with the gravity gun.
    pub throws: u32,
    pub upgrades: u32,
    /// An upgrade reached its last level.
    pub maxed: bool,
    /// Seconds of play.
    pub time: f32,
    /// The highest multiplier reached.
    pub multiplier: u32,
    /// The survival wave reached.
    pub wave: u32,
}

/// A stage that ended: cleared or out of lives.
pub struct StageEnd {
    pub cleared: bool,
    pub survival: bool,
    /// 0-based world and 1-based stage.
    pub world: usize,
    pub stage: usize,
    /// The run's score so far.
    pub score: u64,
    /// Ships lost in this stage.
    pub deaths: u32,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Stats {
    pub games: u64,
    pub time: f64,
    pub stages: u64,
    pub rocks: u64,
    pub big_rocks: u64,
    pub enemies: u64,
    pub depots: u64,
    pub bosses: u64,
    pub rings: u64,
    pub bombs: u64,
    pub deaths: u64,
    pub throws: u64,
    pub upgrades: u64,
    pub best_arcade: u64,
    pub best_survival: u64,
    pub best_wave: u64,
    pub best_multiplier: u64,
    /// Stages cleared without losing a ship.
    pub flawless: u64,
    /// The furthest arcade stage cleared, as world * 5 + stage (1-30).
    pub cleared: u64,
    pub maxed: bool,
    /// Unlocked, by index into ACHIEVEMENTS.
    pub unlocked: Vec<bool>,
}

pub struct Achievement {
    /// Saved in `stats.txt`; never change one.
    pub id: &'static str,
    /// Letters, digits and spaces only (the bitmap font).
    pub title: &'static str,
    pub text: &'static str,
    /// The counter it follows and the goal it needs.
    pub goal: fn(&Stats) -> (u64, u64),
}

const fn a(id: &'static str, title: &'static str, text: &'static str, goal: fn(&Stats) -> (u64, u64)) -> Achievement {
    Achievement { id, title, text, goal }
}

pub const ACHIEVEMENTS: [Achievement; 23] = [
    a("first_rock", "First Contact", "Destroy a rock", |s| (s.rocks, 1)),
    a("rocks_1000", "Rock Breaker", "Destroy 1,000 rocks", |s| (s.rocks, 1_000)),
    a("rocks_10000", "Planet Cleaner", "Destroy 10,000 rocks", |s| (s.rocks, 10_000)),
    a("big_rocks_100", "Heavyweight", "Destroy 100 extra-large rocks", |s| (s.big_rocks, 100)),
    a("enemies_100", "Ace", "Destroy 100 enemy craft", |s| (s.enemies, 100)),
    a("enemies_1000", "Squadron Breaker", "Destroy 1,000 enemy craft", |s| (s.enemies, 1_000)),
    a("boss_1", "Giant Slayer", "Defeat a boss", |s| (s.bosses, 1)),
    a("boss_10", "Boss Hunter", "Defeat 10 bosses", |s| (s.bosses, 10)),
    a("rings_1000", "Ring Collector", "Collect 1,000 rings", |s| (s.rings, 1_000)),
    a("rings_10000", "Ring Hoarder", "Collect 10,000 rings", |s| (s.rings, 10_000)),
    a("multiplier_5", "On a Roll", "Reach a 5x multiplier", |s| (s.best_multiplier, 5)),
    a("multiplier_10", "Maximum Multiplier", "Reach a 10x multiplier", |s| (s.best_multiplier, 10)),
    a("flawless", "Untouchable", "Clear a stage without losing a ship", |s| (s.flawless, 1)),
    a("world_3", "Halfway There", "Clear world 3", |s| (s.cleared, 15)),
    a("world_6", "Champion", "Clear the final stage", |s| (s.cleared, 30)),
    a("wave_10", "Survivor", "Reach wave 10 in survival", |s| (s.best_wave, 10)),
    a("wave_30", "Last One Standing", "Reach wave 30 in survival", |s| (s.best_wave, 30)),
    a("throws_100", "Telekinesis", "Throw 100 enemies with the gravity gun", |s| (s.throws, 100)),
    a("bombs_100", "Demolition", "Set off 100 bombs", |s| (s.bombs, 100)),
    a("maxed", "Fully Loaded", "Max out an upgrade", |s| (s.maxed as u64, 1)),
    a("score_1m", "Millionaire", "Score 1,000,000 in one run", |s| (s.best_arcade.max(s.best_survival), 1_000_000)),
    a("score_10m", "Ten Million", "Score 10,000,000 in one run", |s| (s.best_arcade.max(s.best_survival), 10_000_000)),
    a("hours_10", "Dedicated", "Play for 10 hours", |s| (s.time as u64 / 3600, 10)),
];

impl Stats {
    /// Loads `stats.txt`; `cleared` seeds the furthest stage from the save,
    /// so progress made before stats existed counts.
    pub fn load(cleared: usize) -> Stats {
        let text = std::fs::read_to_string(crate::paths::save(STATS_PATH)).unwrap_or_default();
        let mut s = Stats::parse(&text);
        s.cleared = s.cleared.max(cleared as u64);
        // Unlock what the counters already earn, without toasts.
        s.check();
        s
    }

    fn parse(text: &str) -> Stats {
        let mut s = Stats { unlocked: vec![false; ACHIEVEMENTS.len()], ..Stats::default() };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key == "achievements" {
                for id in value.split(',') {
                    if let Some(i) = ACHIEVEMENTS.iter().position(|a| a.id == id.trim()) {
                        s.unlocked[i] = true;
                    }
                }
                continue;
            }
            if key == "time" {
                s.time = value.parse().unwrap_or(0.0);
                continue;
            }
            if key == "maxed" {
                s.maxed = value == "1";
                continue;
            }
            let n: u64 = value.parse().unwrap_or(0);
            match key {
                "games" => s.games = n,
                "stages" => s.stages = n,
                "rocks" => s.rocks = n,
                "big_rocks" => s.big_rocks = n,
                "enemies" => s.enemies = n,
                "depots" => s.depots = n,
                "bosses" => s.bosses = n,
                "rings" => s.rings = n,
                "bombs" => s.bombs = n,
                "deaths" => s.deaths = n,
                "throws" => s.throws = n,
                "upgrades" => s.upgrades = n,
                "best_arcade" => s.best_arcade = n,
                "best_survival" => s.best_survival = n,
                "best_wave" => s.best_wave = n,
                "best_multiplier" => s.best_multiplier = n,
                "flawless" => s.flawless = n,
                "cleared" => s.cleared = n,
                _ => {}
            }
        }
        s
    }

    fn to_text(&self) -> String {
        let ids: Vec<&str> = ACHIEVEMENTS
            .iter()
            .zip(&self.unlocked)
            .filter(|(_, &on)| on)
            .map(|(a, _)| a.id)
            .collect();
        format!(
            "games={}\ntime={:.1}\nstages={}\nrocks={}\nbig_rocks={}\nenemies={}\ndepots={}\nbosses={}\nrings={}\nbombs={}\ndeaths={}\nthrows={}\nupgrades={}\nbest_arcade={}\nbest_survival={}\nbest_wave={}\nbest_multiplier={}\nflawless={}\ncleared={}\nmaxed={}\nachievements={}\n",
            self.games,
            self.time,
            self.stages,
            self.rocks,
            self.big_rocks,
            self.enemies,
            self.depots,
            self.bosses,
            self.rings,
            self.bombs,
            self.deaths,
            self.throws,
            self.upgrades,
            self.best_arcade,
            self.best_survival,
            self.best_wave,
            self.best_multiplier,
            self.flawless,
            self.cleared,
            self.maxed as u8,
            ids.join(","),
        )
    }

    /// Writes `stats.txt` (nothing on the web build, which has no files).
    pub fn save(&self) {
        let _ = std::fs::write(crate::paths::save(STATS_PATH), self.to_text());
    }

    /// Adds a game's tally; returns the achievements it unlocked.
    pub fn add(&mut self, t: &Tally) -> Vec<usize> {
        self.games += t.games as u64;
        self.time += t.time as f64;
        self.rocks += t.rocks as u64;
        self.big_rocks += t.big_rocks as u64;
        self.enemies += t.enemies as u64;
        self.depots += t.depots as u64;
        self.bosses += t.bosses as u64;
        self.rings += t.rings as u64;
        self.bombs += t.bombs as u64;
        self.deaths += t.deaths as u64;
        self.throws += t.throws as u64;
        self.upgrades += t.upgrades as u64;
        self.maxed |= t.maxed;
        self.best_multiplier = self.best_multiplier.max(t.multiplier as u64);
        self.best_wave = self.best_wave.max(t.wave as u64);
        self.check()
    }

    pub fn stage_end(&mut self, end: &StageEnd) -> Vec<usize> {
        if end.survival {
            self.best_survival = self.best_survival.max(end.score);
        } else {
            self.best_arcade = self.best_arcade.max(end.score);
        }
        if end.cleared && !end.survival {
            self.stages += 1;
            self.cleared = self.cleared.max((end.world * 5 + end.stage) as u64);
            if end.deaths == 0 {
                self.flawless += 1;
            }
        }
        self.check()
    }

    /// Unlocks every achievement whose goal is met; returns the new ones.
    fn check(&mut self) -> Vec<usize> {
        let mut new = Vec::new();
        for (i, a) in ACHIEVEMENTS.iter().enumerate() {
            let (have, need) = (a.goal)(self);
            if !self.unlocked[i] && have >= need {
                self.unlocked[i] = true;
                new.push(i);
            }
        }
        new
    }

    pub fn unlocked_count(&self) -> usize {
        self.unlocked.iter().filter(|&&on| on).count()
    }
}

/// "12,345".
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Text in Rajdhani (the bitmap font has no punctuation), rasterised at
/// the target's pixel size so it stays sharp.
fn small_text(font: &macroquad::text::Font, text: &str, x: f32, y: f32, size: f32, align: Align, color: Color) {
    let k = gfx::pixel_scale().max(1.0);
    let px = (size * k).round() as u16;
    let w = measure_text(text, Some(font), px, 1.0 / k).width;
    let x = match align {
        Align::Left => x,
        Align::Centre => x - w * 0.5,
        Align::Right => x - w,
    };
    draw_text_ex(text, x, y, TextParams { font: Some(font), font_size: px, font_scale: 1.0 / k, color, ..Default::default() });
}

/// Unlock toasts, shown one at a time at the bottom of the screen.
#[derive(Default)]
pub struct Toasts {
    queue: VecDeque<(usize, f32)>,
}

const TOAST_TIME: f32 = 3.5;
const TOAST_FADE: f32 = 0.3;

impl Toasts {
    pub fn push(&mut self, new: &[usize]) {
        self.queue.extend(new.iter().map(|&i| (i, 0.0)));
    }

    pub fn update(&mut self, dt: f32) {
        if let Some((_, t)) = self.queue.front_mut() {
            *t += dt;
            if *t >= TOAST_TIME {
                self.queue.pop_front();
            }
        }
    }

    pub fn draw(&self, canvas: &Canvas, font: &Font, small: &macroquad::text::Font) {
        let Some(&(i, t)) = self.queue.front() else {
            return;
        };
        let a = (t / TOAST_FADE).min((TOAST_TIME - t) / TOAST_FADE).clamp(0.0, 1.0);
        let r = Rect::new(240.0 - 115.0, CANVAS_H - 58.0, 230.0, 40.0);
        canvas.fill_rect(r.x, r.y, r.w, r.h, Color::new(0.02, 0.03, 0.08, 0.85 * a));
        canvas.fill_rect(r.x, r.y, r.w, 1.5, Color::new(1.0, 0.8, 0.3, a));
        medal(vec2(r.x + 18.0, r.y + 20.0), 9.0, true, a);
        small_text(small, "Achievement unlocked", r.x + 34.0, r.y + 14.0, 10.0, Align::Left, Color::new(1.0, 0.8, 0.3, a));
        font.text(ACHIEVEMENTS[i].title, r.x + 34.0, r.y + 20.0, 0.5, Align::Left, Color::new(1.0, 1.0, 1.0, a));
    }
}

/// A medal: a gold diamond with a lighter core when earned, a dim outline
/// otherwise.
fn medal(c: Vec2, r: f32, earned: bool, a: f32) {
    let pts = [c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
    if earned {
        let gold = Color::new(1.0, 0.75, 0.25, a);
        draw_triangle(pts[0], pts[1], pts[2], gold);
        draw_triangle(pts[0], pts[2], pts[3], gold);
        let k = 0.45;
        let inner = pts.map(|p| c + (p - c) * k);
        let light = Color::new(1.0, 0.93, 0.6, a);
        draw_triangle(inner[0], inner[1], inner[2], light);
        draw_triangle(inner[0], inner[2], inner[3], light);
    } else {
        let dim = Color::new(0.45, 0.45, 0.55, a);
        for k in 0..4 {
            draw_line(pts[k].x, pts[k].y, pts[(k + 1) % 4].x, pts[(k + 1) % 4].y, 1.0, dim);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tab {
    Stats,
    Achievements,
}

/// What the caller should do after an update.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Outcome {
    Click,
    Close,
}

pub struct StatsScreen {
    pub tab: Tab,
    alpha: f32,
}

const TABS: [Rect; 2] = [Rect { x: 115.0, y: 4.0, w: 90.0, h: 26.0 }, Rect { x: 220.0, y: 4.0, w: 160.0, h: 26.0 }];
const BACK: Rect = Rect { x: 16.0, y: 298.0, w: 60.0, h: 18.0 };
const ROW_TOP: f32 = 40.0;
const ROW_PITCH: f32 = 14.0;
const ACH_TOP: f32 = 38.0;
const ACH_PITCH: f32 = 21.5;
const ACH_COLS: [f32; 2] = [20.0, 250.0];

impl StatsScreen {
    pub fn new(tab: Tab) -> StatsScreen {
        StatsScreen { tab, alpha: 0.0 }
    }

    pub fn update(&mut self, dt: f32) -> Option<Outcome> {
        self.alpha = (self.alpha + dt * 4.0).min(1.0);
        let nav = pad::nav(dt);
        let pressed = |keys: &[KeyCode], b: Button| keys.iter().any(|&k| is_key_pressed(k)) || pad::pressed(b) || nav == Some(b);
        let switch = pressed(&[KeyCode::Left, KeyCode::A, KeyCode::Right, KeyCode::D, KeyCode::Tab], Button::Left)
            || pressed(&[], Button::Right)
            || pad::pressed(Button::LeftBumper)
            || pad::pressed(Button::RightBumper);
        if switch {
            self.tab = if self.tab == Tab::Stats { Tab::Achievements } else { Tab::Stats };
            return Some(Outcome::Click);
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            let m = Canvas::mouse();
            for (rect, tab) in TABS.iter().zip([Tab::Stats, Tab::Achievements]) {
                if rect.contains(m) && self.tab != tab {
                    self.tab = tab;
                    return Some(Outcome::Click);
                }
            }
            if BACK.contains(m) {
                return Some(Outcome::Close);
            }
        }
        if is_key_pressed(KeyCode::Escape) || pad::pressed(Button::B) || pad::pressed(Button::A) || pad::enter_pressed() {
            return Some(Outcome::Close);
        }
        None
    }

    pub fn draw(&self, canvas: &Canvas, font: &Font, small: &macroquad::text::Font, stats: &Stats) {
        let a = self.alpha;
        let white = Color::new(1.0, 1.0, 1.0, a);
        let dim = Color::new(0.6, 0.6, 0.7, a);
        let e = edge();
        canvas.fill_rect(-e, 0.0, CANVAS_W + 2.0 * e, CANVAS_H, Color::new(0.0, 0.0, 0.0, 0.7 * a));
        for (rect, (tab, label)) in TABS.iter().zip([(Tab::Stats, "Stats"), (Tab::Achievements, "Achievements")]) {
            let on = self.tab == tab;
            let c = if on { white } else { dim };
            let cx = rect.x + rect.w * 0.5;
            font.text(label, cx, rect.y + 6.0, 0.7, Align::Centre, c);
            if on {
                let w = font.width(label, 0.7);
                canvas.fill_rect(cx - w * 0.5, rect.y + 22.0, w, 1.5, c);
            }
        }
        match self.tab {
            Tab::Stats => self.draw_stats(font, stats, white, dim),
            Tab::Achievements => self.draw_achievements(font, small, stats, a),
        }
        let m = Canvas::mouse();
        let back = if BACK.contains(m) { white } else { dim };
        font.text("Back", BACK.x, BACK.y + 3.0, 0.55, Align::Left, back);
        small_text(small, "Left and Right switch tabs   Esc or B goes back", 464.0, 312.0, 10.0, Align::Right, dim);
    }

    fn draw_stats(&self, font: &Font, s: &Stats, white: Color, dim: Color) {
        let hours = (s.time / 3600.0) as u64;
        let minutes = ((s.time / 60.0) as u64) % 60;
        let rows: [(&str, String); 18] = [
            ("Games played", grouped(s.games)),
            ("Time played", format!("{}h {}m", hours, minutes)),
            ("Stages cleared", grouped(s.stages)),
            ("Flawless stages", grouped(s.flawless)),
            ("Rocks destroyed", grouped(s.rocks)),
            ("Extra large rocks", grouped(s.big_rocks)),
            ("Enemy craft destroyed", grouped(s.enemies)),
            ("Supply depots destroyed", grouped(s.depots)),
            ("Bosses defeated", grouped(s.bosses)),
            ("Rings collected", grouped(s.rings)),
            ("Bombs used", grouped(s.bombs)),
            ("Ships lost", grouped(s.deaths)),
            ("Gravity gun throws", grouped(s.throws)),
            ("Upgrades bought", grouped(s.upgrades)),
            ("Best arcade score", grouped(s.best_arcade)),
            ("Best survival score", grouped(s.best_survival)),
            ("Best survival wave", grouped(s.best_wave)),
            ("Best multiplier", format!("{}x", s.best_multiplier.max(1))),
        ];
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = ROW_TOP + i as f32 * ROW_PITCH;
            font.text(label, 60.0, y, 0.5, Align::Left, dim);
            // The bitmap font has no comma: draw digit groups apart.
            draw_number(font, value, 420.0, y, white);
        }
    }

    fn draw_achievements(&self, font: &Font, small: &macroquad::text::Font, s: &Stats, a: f32) {
        let per_col = ACHIEVEMENTS.len().div_ceil(2);
        for (i, ach) in ACHIEVEMENTS.iter().enumerate() {
            let x = ACH_COLS[i / per_col];
            let y = ACH_TOP + (i % per_col) as f32 * ACH_PITCH;
            let earned = s.unlocked[i];
            medal(vec2(x + 7.0, y + 9.0), 6.5, earned, a);
            let title = if earned { Color::new(1.0, 1.0, 1.0, a) } else { Color::new(0.6, 0.6, 0.7, a) };
            font.text(ach.title, x + 20.0, y + 1.0, 0.4, Align::Left, title);
            let (have, need) = (ach.goal)(s);
            let text = if earned || need <= 1 {
                ach.text.to_owned()
            } else {
                format!("{}  ({} of {})", ach.text, grouped(have.min(need)), grouped(need))
            };
            let c = if earned { Color::new(1.0, 0.85, 0.5, a) } else { Color::new(0.55, 0.55, 0.65, a) };
            small_text(small, &text, x + 20.0, y + 16.0, 8.5, Align::Left, c);
        }
        let total = format!("{} of {} unlocked", s.unlocked_count(), ACHIEVEMENTS.len());
        small_text(small, &total, BACK.x + BACK.w + 12.0, BACK.y + 13.0, 10.0, Align::Left, Color::new(1.0, 0.85, 0.5, a));
    }
}

/// A number right-aligned at `x`, with its comma groups (and any letters)
/// drawn by the bitmap font and the commas as small gaps with a dot.
fn draw_number(font: &Font, text: &str, x: f32, y: f32, color: Color) {
    const SCALE: f32 = 0.5;
    const GAP: f32 = 5.0;
    let parts: Vec<&str> = text.split(',').collect();
    let width: f32 = parts.iter().map(|p| font.width(p, SCALE)).sum::<f32>() + GAP * (parts.len() - 1) as f32;
    let mut at = x - width;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            draw_rectangle(at + 1.5, y + 6.5, 1.5, 1.5, color);
            at += GAP;
        }
        font.text(part, at, y, SCALE, Align::Left, color);
        at += font.width(part, SCALE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_round_trip_through_the_file() {
        let mut s = Stats::parse("");
        s.add(&Tally { rocks: 1_200, rings: 40, multiplier: 6, time: 90.0, ..Tally::default() });
        s.stage_end(&StageEnd { cleared: true, survival: false, world: 2, stage: 5, score: 1_500_000, deaths: 0 });
        let back = Stats::parse(&s.to_text());
        assert_eq!(back, s);
    }

    #[test]
    fn goals_unlock_once() {
        let mut s = Stats::parse("");
        let first = s.add(&Tally { rocks: 1, ..Tally::default() });
        assert_eq!(first, vec![0]);
        assert!(s.add(&Tally { rocks: 1, ..Tally::default() }).is_empty());
        let names: Vec<&str> = s
            .stage_end(&StageEnd { cleared: true, survival: false, world: 2, stage: 5, score: 10, deaths: 0 })
            .iter()
            .map(|&i| ACHIEVEMENTS[i].id)
            .collect();
        assert_eq!(names, ["flawless", "world_3"]);
    }

    #[test]
    fn titles_fit_the_bitmap_font() {
        for a in &ACHIEVEMENTS {
            assert!(a.title.chars().all(|c| c.is_ascii_alphanumeric() || c == ' '), "{}", a.title);
        }
    }

    #[test]
    fn numbers_group_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(12_345_678), "12,345,678");
    }
}
