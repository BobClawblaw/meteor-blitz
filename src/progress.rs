//! Persistent progress: best score, stage unlocks, banked ore, and upgrades.

use crate::game::{UPGRADE_COUNT, UPGRADE_MAX};

const SAVE_PATH: &str = "save.txt";

/// Display name and base ore cost per upgrade, in the original shop order.
pub const UPGRADES: [(&str, u32); UPGRADE_COUNT] = [
    ("CANNON", 50),
    ("FIRE", 50),
    ("ICE", 50),
    ("SPEED", 200),
    ("GRAV GUN", 300),
    ("BOOST", 200),
];

pub struct Progress {
    pub best: u64,
    pub unlocked: usize,
    pub ore: u32,
    pub upgrades: [u8; UPGRADE_COUNT],
}

impl Progress {
    pub fn load() -> Progress {
        let mut p = Progress {
            best: 0,
            unlocked: 0,
            ore: 0,
            upgrades: [0; UPGRADE_COUNT],
        };
        let Ok(text) = std::fs::read_to_string(SAVE_PATH) else {
            return p;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "best" => p.best = value.trim().parse().unwrap_or(0),
                "unlocked" => p.unlocked = value.trim().parse().unwrap_or(0),
                "ore" => p.ore = value.trim().parse().unwrap_or(0),
                "upgrades" => {
                    for (slot, level) in p.upgrades.iter_mut().zip(value.split(',')) {
                        *slot = level.trim().parse::<u8>().unwrap_or(0).min(UPGRADE_MAX);
                    }
                }
                _ => {}
            }
        }
        p
    }

    /// Writes the save file. On the web build there is no filesystem, so this
    /// silently does nothing and progress lasts for the session only.
    pub fn save(&self) {
        let levels: Vec<String> = self.upgrades.iter().map(|l| l.to_string()).collect();
        let text = format!(
            "best={}\nunlocked={}\nore={}\nupgrades={}\n",
            self.best,
            self.unlocked,
            self.ore,
            levels.join(",")
        );
        let _ = std::fs::write(SAVE_PATH, text);
    }

    /// Ore cost of the next level, or `None` when the upgrade is maxed.
    pub fn cost(&self, slot: usize) -> Option<u32> {
        let level = self.upgrades[slot];
        if level >= UPGRADE_MAX {
            None
        } else {
            Some(UPGRADES[slot].1 * (level as u32 + 1))
        }
    }

    pub fn can_buy(&self, slot: usize) -> bool {
        self.cost(slot).is_some_and(|c| c <= self.ore)
    }

    pub fn buy(&mut self, slot: usize) -> bool {
        match self.cost(slot) {
            Some(cost) if cost <= self.ore => {
                self.ore -= cost;
                self.upgrades[slot] += 1;
                self.save();
                true
            }
            _ => false,
        }
    }
}
