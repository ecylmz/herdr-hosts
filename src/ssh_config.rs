//! Hosts, folders and notes, all read from `~/.ssh/config`.
//!
//! OpenSSH has no concept of a group — `ssh_config(5)` mentions "group" only in
//! cipher names. What it does have is the comment style people already use to
//! divide a config into sections, so that is what the tree is built from:
//!
//! ```sshconfig
//! # --- Personal / Hetzner ---
//! Host dokku-prod   # main dokku box
//! Host postgres
//! ```
//!
//! This is deliberately not an ssh config implementation: the picker only needs
//! the names a user can type after `ssh`. Anything needing resolved settings
//! should ask OpenSSH itself (`ssh -G <alias>`) rather than grow this module.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Includes can nest; a depth cap is simpler than proving the graph is acyclic
/// and OpenSSH caps it too.
const MAX_DEPTH: usize = 16;

/// Separates nesting levels inside a section heading.
const NESTING: char = '/';

#[derive(Debug, Clone, PartialEq)]
pub struct Host {
    pub alias: String,
    /// Section path the host was written under; empty means ungrouped.
    pub folder: Vec<String>,
    pub note: Option<String>,
}

impl Host {
    pub fn folder_path(&self) -> String {
        self.folder.join(" / ")
    }

    /// Stable id for the folder this host sits in.
    pub fn folder_id(&self) -> String {
        self.folder.join("\u{1}")
    }
}

pub fn default_path() -> Option<PathBuf> {
    Some(home()?.join(".ssh/config"))
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Hosts in file order, deduplicated by alias. A missing file is not an error:
/// a user with no ssh config simply has no hosts.
pub fn hosts(path: &Path) -> Vec<Host> {
    // ssh_config(5): a relative Include is "assumed to be in ~/.ssh" — the
    // directory of the top-level config, NOT of the file doing the including.
    // Verified: `~/.ssh/config.d/x` containing `Include common` reads
    // `~/.ssh/common`.
    let ssh_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    read_into(path, &ssh_dir, &[], &mut out, &mut seen, 0);
    out
}

fn read_into(
    path: &Path,
    ssh_dir: &Path,
    // Heading in force where this file was included, so a section can wrap an
    // `Include` and still own what it pulls in.
    inherited: &[String],
    out: &mut Vec<Host>,
    seen: &mut HashSet<PathBuf>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    // The same file reached twice (directly or through a glob) contributes once.
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(key) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };

    let mut section = inherited.to_vec();
    for line in text.lines() {
        if let Some(heading) = section_heading(line) {
            section = inherited.iter().cloned().chain(heading).collect();
            continue;
        }
        let Some((keyword, rest)) = split_directive(line) else {
            continue;
        };
        // OpenSSH treats the rest of a line after `#` as a comment (verified
        // with `ssh -G`), so it must not become an alias.
        let (rest, note) = split_trailing_comment(rest);
        match keyword.to_ascii_lowercase().as_str() {
            "host" => {
                for alias in tokens(rest).filter(|t| !is_pattern(t)) {
                    if !out.iter().any(|h| h.alias == alias) {
                        out.push(Host {
                            alias,
                            folder: section.clone(),
                            note: note.map(str::to_string),
                        });
                    }
                }
            }
            "include" => {
                for pattern in tokens(rest) {
                    for included in expand(&pattern, ssh_dir) {
                        read_into(&included, ssh_dir, &section, out, seen, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A section heading is a comment whose text is fenced by at least two of the
/// same separator: `# --- Work ---`, `# === Work / Prod ===`. An ordinary
/// comment is not a heading, so existing configs keep their meaning.
fn section_heading(line: &str) -> Option<Vec<String>> {
    let body = line.trim().strip_prefix('#')?.trim();
    let separator = body.chars().next()?;
    if !matches!(separator, '-' | '=' | '*') {
        return None;
    }
    let lead = body.chars().take_while(|c| *c == separator).count();
    if lead < 2 {
        return None;
    }
    let after_lead = &body[lead..];
    let without_tail = after_lead.trim_end_matches(separator);
    if after_lead.len() - without_tail.len() < 2 {
        return None;
    }
    let parts: Vec<String> = without_tail
        .split(NESTING)
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then_some(parts)
}

/// `Host x`, `Host=x` and leading whitespace are all valid OpenSSH spellings.
fn split_directive(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line.find([' ', '\t', '='])?;
    Some((&line[..end], line[end..].trim_start_matches([' ', '\t', '='])))
}

/// OpenSSH only starts a comment at a token boundary: `Host beta#gamma` is one
/// alias, not `beta` with a note. Verified with `ssh -G`.
fn split_trailing_comment(rest: &str) -> (&str, Option<&str>) {
    let at = rest
        .char_indices()
        .find(|(i, c)| *c == '#' && (*i == 0 || rest[..*i].ends_with([' ', '\t'])));
    match at {
        Some((at, _)) => {
            let note = rest[at + 1..].trim();
            (&rest[..at], (!note.is_empty()).then_some(note))
        }
        None => (rest, None),
    }
}

fn tokens(rest: &str) -> impl Iterator<Item = String> + '_ {
    rest.split_whitespace()
        .map(|t| t.trim_matches('"').to_string())
        .filter(|t| !t.is_empty())
}

/// `Host *.example.com` declares defaults for a pattern, not a host anyone
/// connects to by that name, so it is not inventory.
fn is_pattern(token: &str) -> bool {
    token.contains(['*', '?', '!'])
}

/// Relative includes resolve against the ssh directory, per ssh_config(5).
fn expand(pattern: &str, ssh_dir: &Path) -> Vec<PathBuf> {
    let resolved = if let Some(rest) = pattern.strip_prefix("~/") {
        match home() {
            Some(h) => h.join(rest),
            None => return Vec::new(),
        }
    } else if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        ssh_dir.join(pattern)
    };

    let Some(as_str) = resolved.to_str() else {
        return Vec::new();
    };
    match glob::glob(as_str) {
        Ok(paths) => paths.flatten().filter(|p| p.is_file()).collect(),
        // A literal path that is not a valid glob is still worth trying.
        Err(_) => vec![resolved],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("herdr-hosts-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn summary(hosts: &[Host]) -> Vec<String> {
        hosts
            .iter()
            .map(|h| {
                format!(
                    "{}|{}|{}",
                    h.alias,
                    h.folder.join("/"),
                    h.note.as_deref().unwrap_or("")
                )
            })
            .collect()
    }

    #[test]
    fn reads_aliases_and_skips_patterns() {
        let dir = tmpdir("aliases");
        let cfg = write(
            &dir,
            "config",
            "# just a note, not a heading\n\
             Host dokku-prod postgres\n    HostName 1.2.3.4\n\n\
             Host *.example.com\n    User root\n\
             Host=research\n\
             host nas\n\
             Host \"quoted\"\n\
             Host !banned\n\
             Host dokku-prod\n",
        );
        assert_eq!(
            hosts(&cfg).iter().map(|h| h.alias.clone()).collect::<Vec<_>>(),
            ["dokku-prod", "postgres", "research", "nas", "quoted"]
        );
    }

    #[test]
    fn section_headings_build_the_folder_tree() {
        let dir = tmpdir("sections");
        let cfg = write(
            &dir,
            "config",
            "Host unfiled\n\n\
             # --- Personal / Hetzner ---\n\
             Host dokku-prod   # main dokku box\n\
             Host postgres\n\n\
             # === University ===\n\
             Host web-prod\n\
             # remember to rotate this key\n\
             Host gitlab\n",
        );
        assert_eq!(
            summary(&hosts(&cfg)),
            [
                "unfiled||",
                "dokku-prod|Personal/Hetzner|main dokku box",
                "postgres|Personal/Hetzner|",
                "web-prod|University|",
                // A plain comment is not a heading, so gitlab stays put.
                "gitlab|University|",
            ]
        );
    }

    #[test]
    fn only_fenced_comments_are_headings() {
        assert_eq!(section_heading("# --- Work ---"), Some(vec!["Work".into()]));
        assert_eq!(section_heading("#=== Work ==="), Some(vec!["Work".into()]));
        assert_eq!(
            section_heading("# ** A / B / C **"),
            Some(vec!["A".into(), "B".into(), "C".into()])
        );
        for not_a_heading in [
            "# Work",           // plain comment
            "# - Work -",       // single separator
            "# --- Work",       // not closed
            "# -----",          // a rule, no text
            "Host web-prod",    // not a comment
            "",
        ] {
            assert_eq!(section_heading(not_a_heading), None, "{not_a_heading:?}");
        }
    }

    #[test]
    fn trailing_comments_are_notes_not_aliases() {
        let dir = tmpdir("notes");
        // Verified against `ssh -G`: OpenSSH ignores everything after `#`.
        let cfg = write(&dir, "config", "Host alpha # bir not\nHost beta\n");
        assert_eq!(summary(&hosts(&cfg)), ["alpha||bir not", "beta||"]);
    }

    #[test]
    fn follows_include_globs_without_looping_and_nests_under_the_heading() {
        let dir = tmpdir("include");
        // `Include config` from a nested file resolves against the ssh dir, so
        // this is the loop guard doing its job, not a missing file.
        write(&dir, "config.d/10-work", "Host web-prod\nInclude config\n");
        write(&dir, "config.d/20-home", "# --- NAS ---\nHost nas\n");
        let cfg = write(
            &dir,
            "config",
            "Host base\n# --- Included ---\nInclude config.d/*\n",
        );
        assert_eq!(
            summary(&hosts(&cfg)),
            ["base||", "web-prod|Included|", "nas|Included/NAS|"]
        );
    }

    #[test]
    fn nested_relative_include_resolves_against_the_ssh_dir() {
        let dir = tmpdir("nested-include");
        // Verified against OpenSSH: ~/.ssh/config.d/10-work asking for
        // `common` reads ~/.ssh/common, not ~/.ssh/config.d/common.
        write(&dir, "config.d/10-work", "Include common\nHost work\n");
        write(&dir, "common", "Host shared\n");
        write(&dir, "config.d/common", "Host wrong-one\n");
        let cfg = write(&dir, "config", "Include config.d/10-work\n");
        assert_eq!(
            hosts(&cfg).iter().map(|h| h.alias.clone()).collect::<Vec<_>>(),
            ["shared", "work"]
        );
    }

    #[test]
    fn a_hash_inside_a_token_is_part_of_the_alias() {
        let dir = tmpdir("hash");
        // Verified against OpenSSH 10.5p1: `ssh -G beta#gamma` matches this
        // block and `ssh -G beta` does not.
        let cfg = write(&dir, "config", "Host beta#gamma\nHost delta #note\n");
        assert_eq!(
            summary(&hosts(&cfg)),
            ["beta#gamma||", "delta||note"]
        );
    }

    #[test]
    fn missing_file_is_empty_not_a_panic() {
        assert!(hosts(Path::new("/nonexistent/ssh/config")).is_empty());
    }
}
