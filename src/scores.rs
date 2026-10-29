//! `LocalScores` / `HighScoresScreen`: the on-device score table.
//!
//! The original also had a `GlobalScores` tab backed by Game Center. That
//! server is long gone, so the Global tab here reports that it is offline
//! instead of inventing ranks.

use macroquad::prelude::*;

use crate::hud::{Align, Font};

/// `LocalScores` keeps ten rows per mode.
pub const MAX_ENTRIES: usize = 10;
const SCORES_PATH: &str = "scores.txt";

#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub name: String,
    pub score: u64,
}

pub struct Scores {
    pub arcade: Vec<Entry>,
    pub survival: Vec<Entry>,
}

impl Default for Scores {
    fn default() -> Scores {
        Scores { arcade: Vec::new(), survival: Vec::new() }
    }
}

impl Scores {
    pub fn load() -> Scores {
        match std::fs::read_to_string(crate::paths::save(SCORES_PATH)) {
            Ok(text) => Scores::parse(&text),
            Err(_) => Scores::default(),
        }
    }

    pub fn save(&self) {
        let _ = std::fs::write(crate::paths::save(SCORES_PATH), self.to_text());
    }

    /// `LocalScores::saveAndLoad`: inserts a run, keeps the table sorted and
    /// returns its 1-based rank, or 0 when it did not make the table.
    pub fn insert(&mut self, survival: bool, name: &str, score: u64) -> usize {
        let table = if survival { &mut self.survival } else { &mut self.arcade };
        table.push(Entry { name: sanitize(name), score });
        table.sort_by(|a, b| b.score.cmp(&a.score));
        table.truncate(MAX_ENTRIES);
        match table.iter().position(|e| e.score == score) {
            Some(i) => i + 1,
            None => 0,
        }
    }

    pub fn table(&self, survival: bool) -> &[Entry] {
        if survival {
            &self.survival
        } else {
            &self.arcade
        }
    }

    /// `LocalScores::getHighestScore`.
    #[allow(dead_code)]
    pub fn highest(&self, survival: bool) -> u64 {
        self.table(survival).first().map_or(0, |e| e.score)
    }

    /// True when `score` would make the table; the original only prompted
    /// for a name when it did.
    pub fn qualifies(&self, survival: bool, score: u64) -> bool {
        let table = self.table(survival);
        table.len() < MAX_ENTRIES || score > table.last().map_or(0, |e| e.score)
    }

    fn parse(text: &str) -> Scores {
        let mut scores = Scores::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some((score, name)) = value.split_once(':') else {
                continue;
            };
            let Ok(score) = score.trim().parse::<u64>() else {
                continue;
            };
            let entry = Entry { name: sanitize(name.trim()), score };
            match key.trim() {
                "arcade" => scores.arcade.push(entry),
                "survival" => scores.survival.push(entry),
                _ => {}
            }
        }
        for table in [&mut scores.arcade, &mut scores.survival] {
            table.sort_by(|a, b| b.score.cmp(&a.score));
            table.truncate(MAX_ENTRIES);
        }
        scores
    }

    fn to_text(&self) -> String {
        let mut text = String::new();
        for (key, table) in [("arcade", &self.arcade), ("survival", &self.survival)] {
            for entry in table {
                text.push_str(&format!("{}={}:{}\n", key, entry.score, entry.name));
            }
        }
        text
    }
}

/// `MultiLabel` only has digits and letters, so the original's name entry
/// was alphanumeric too. Uppercase, at most eight characters, and a default
/// when nothing usable is left.
pub fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_uppercase();
    if out.is_empty() {
        out = "PLAYER".to_owned();
    }
    out
}

/// `HighScoreRow::draw`: rank, name and score, one row per entry.
pub fn draw_rows(font: &Font, scores: &Scores, survival: bool, global: bool, alpha: f32) {
    let color = Color::new(1.0, 1.0, 1.0, alpha);
    if global {
        notice(font, "GLOBAL SCORES OFFLINE", color);
        return;
    }
    let rows = scores.table(survival);
    for (i, entry) in rows.iter().enumerate() {
        let y = 58.0 + i as f32 * 22.0;
        font.text(&format!("{}", i + 1), 40.0, y, 1.0, Align::Right, color);
        font.text(&entry.name, 56.0, y, 1.0, Align::Left, color);
        font.number(entry.score, 440.0, y, 1.0, Align::Right, color);
    }
    if rows.is_empty() {
        notice(font, "NO SCORES YET", color);
    }
}

/// A message in place of the table, as large as fits between the margins.
fn notice(font: &Font, text: &str, color: Color) {
    let scale = (NOTICE_WIDTH / font.width(text, 1.0)).min(1.5);
    font.text(text, 240.0, 160.0, scale, Align::Centre, color);
}

/// Widest a notice may be: the table's rank and score columns (40-440).
const NOTICE_WIDTH: f32 = 400.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_in_order_and_truncates() {
        let mut scores = Scores::default();
        for score in 1..=12u64 {
            scores.insert(false, "PLAYER", score * 100);
        }
        assert_eq!(scores.arcade.len(), MAX_ENTRIES);
        assert_eq!(scores.arcade[0].score, 1200);
        assert_eq!(scores.highest(false), 1200);
        assert_eq!(scores.insert(false, "PLAYER", 50), 0);
        assert_eq!(scores.insert(false, "PLAYER", 5000), 1);
    }

    #[test]
    fn qualifies_only_for_a_place() {
        let mut scores = Scores::default();
        assert!(scores.qualifies(false, 1));
        for score in 1..=MAX_ENTRIES as u64 {
            scores.insert(false, "PLAYER", score * 100);
        }
        assert!(!scores.qualifies(false, 50));
        assert!(scores.qualifies(false, 2000));
    }

    #[test]
    fn round_trip_and_sanitize() {
        let mut scores = Scores::default();
        scores.insert(true, "a b!c", 900);
        scores.insert(false, "Zoe", 100);
        let parsed = Scores::parse(&scores.to_text());
        assert_eq!(parsed.survival[0], Entry { name: "ABC".to_owned(), score: 900 });
        assert_eq!(parsed.arcade[0], Entry { name: "ZOE".to_owned(), score: 100 });
        assert_eq!(sanitize("!!"), "PLAYER");
        assert_eq!(sanitize("abcdefghij"), "ABCDEFGH");
    }
}
