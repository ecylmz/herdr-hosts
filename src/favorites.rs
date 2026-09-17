//! Starred hosts.
//!
//! The only thing this plugin writes. It goes to the plugin state directory,
//! never to `~/.ssh/config`: that file is how the user reaches every machine
//! they own, and a plugin has no business rewriting it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn path() -> Option<PathBuf> {
    std::env::var_os("HERDR_PLUGIN_STATE_DIR").map(|d| PathBuf::from(d).join("favorites"))
}

/// One alias per line. Missing file means nothing is starred yet.
pub fn load(path: &Path) -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Written temp-then-rename so an interrupted write cannot truncate the list.
pub fn save(path: &Path, favorites: &HashSet<String>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut lines: Vec<&str> = favorites.iter().map(String::as_str).collect();
    lines.sort_unstable(); // stable on disk, so diffs and repeat writes are quiet
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, lines.join("\n") + "\n").map_err(|e| format!("{}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_a_missing_file() {
        let dir = std::env::temp_dir().join(format!("herdr-hosts-fav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("favorites");

        assert!(load(&path).is_empty());

        let starred: HashSet<String> = ["dokku-prod", "nas"].iter().map(|s| s.to_string()).collect();
        save(&path, &starred).unwrap();
        assert_eq!(load(&path), starred);

        // Hand edits with blanks and comments survive a read.
        std::fs::write(&path, "\n# mine\nweb-prod\n\n  gitlab  \n").unwrap();
        assert_eq!(
            load(&path),
            ["web-prod", "gitlab"].iter().map(|s| s.to_string()).collect()
        );

        save(&path, &HashSet::new()).unwrap();
        assert!(load(&path).is_empty());
    }
}
