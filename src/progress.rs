//! Persistent progress: best score and stage unlocks.

const SAVE_PATH: &str = "save.txt";

pub struct Progress {
    pub best: u64,
    /// Index (world * 5 + stage - 1) of the furthest unlocked arcade stage.
    pub unlocked: usize,
}

impl Progress {
    pub fn load() -> Progress {
        let mut p = Progress { best: 0, unlocked: 0 };
        let Ok(text) = std::fs::read_to_string(crate::paths::save(SAVE_PATH)) else {
            return p;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "best" => p.best = value.trim().parse().unwrap_or(0),
                "unlocked" => p.unlocked = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
        p
    }

    /// Writes the save file. On the web build there is no filesystem, so this
    /// silently does nothing and progress lasts for the session only.
    #[allow(dead_code)]
    pub fn save(&self) {
        let _ = std::fs::write(crate::paths::save(SAVE_PATH), format!("best={}\nunlocked={}\n", self.best, self.unlocked));
    }
}
