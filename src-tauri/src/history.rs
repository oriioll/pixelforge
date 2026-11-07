//! Persistent list of finished jobs (`history.json` in the app data directory).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub input: String,
    pub output: String,
    pub preset: String,
    pub intensity: f32,
    pub size: String,
    pub model: String,
    pub width: u32,
    pub height: u32,
    /// Unix seconds.
    pub created_at: u64,
    pub duration_ms: u64,
}

fn file(dir: &Path) -> PathBuf {
    dir.join("history.json")
}

/// Reads the history, newest first. A missing or corrupt file is an empty history.
pub fn load(dir: &Path) -> Vec<HistoryEntry> {
    std::fs::read(file(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Atomically replaces the history file.
pub fn save(dir: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("history.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(entries)?)?;
    std::fs::rename(tmp, file(dir))
}

/// Adds an entry on top and trims the list.
pub fn push(dir: &Path, entry: HistoryEntry) -> std::io::Result<()> {
    let mut all = load(dir);
    all.insert(0, entry);
    all.truncate(MAX_ENTRIES);
    save(dir, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry { id: id.into(), input: "a.jpg".into(), output: "b.jpg".into(), preset: "football".into(), intensity: 0.6, size: "2k".into(), model: "m".into(), width: 1, height: 1, created_at: 0, duration_ms: 1 }
    }

    #[test]
    fn newest_entry_comes_first_and_the_list_is_capped() {
        let dir = std::env::temp_dir().join(format!("pf-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..(MAX_ENTRIES + 5) {
            push(&dir, entry(&i.to_string())).unwrap();
        }
        let all = load(&dir);
        assert_eq!(all.len(), MAX_ENTRIES);
        assert_eq!(all[0].id, (MAX_ENTRIES + 4).to_string());
    }

    #[test]
    fn corrupt_file_reads_as_empty() {
        let dir = std::env::temp_dir().join(format!("pf-hist-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("history.json"), b"{not json").unwrap();
        assert!(load(&dir).is_empty());
    }
}
