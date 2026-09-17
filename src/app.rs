//! Picker state: the flattened tree, navigation, search and favorites.
//!
//! Rows are rebuilt from the parsed ssh config on every change rather than kept
//! in sync incrementally; the list is small and one source of truth is cheaper
//! to reason about than a diff.

use crate::favorites;
use crate::ssh_config::{self, Host};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// `★ Favorites` and `Ungrouped`: not real sections, so not collapsible.
    Section { label: String },
    Folder { id: String, name: String, depth: usize, collapsed: bool },
    Host {
        alias: String,
        depth: usize,
        favorite: bool,
        /// True for the copy under ★ Favorites. The same host appears twice, so
        /// the two rows must not share an identity.
        shortcut: bool,
        /// Shown when searching, where the tree position is not visible.
        context: Option<String>,
        note: Option<String>,
    },
}

impl Row {
    pub fn alias(&self) -> Option<&str> {
        match self {
            Row::Host { alias, .. } => Some(alias),
            _ => None,
        }
    }

    /// Identity across a rebuild. Folders need one too, otherwise expanding a
    /// folder loses the cursor and it snaps back to the top of the tree.
    fn key(&self) -> Option<String> {
        match self {
            Row::Section { .. } => None,
            Row::Folder { id, .. } => Some(format!("f:{id}")),
            Row::Host { alias, shortcut: true, .. } => Some(format!("\u{2605}:{alias}")),
            Row::Host { alias, .. } => Some(format!("h:{alias}")),
        }
    }

    fn depth(&self) -> usize {
        match self {
            Row::Section { .. } => 0,
            Row::Folder { depth, .. } | Row::Host { depth, .. } => *depth,
        }
    }

    fn selectable(&self) -> bool {
        !matches!(self, Row::Section { .. })
    }
}

pub struct App {
    hosts: Vec<Host>,
    favorites: HashSet<String>,
    collapsed: HashSet<String>,
    pub search: Option<String>,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub status: Option<String>,
    pub help: bool,
    pub done: bool,
    favorites_path: Option<PathBuf>,
    ssh_config_path: Option<PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            hosts: Vec::new(),
            favorites: HashSet::new(),
            collapsed: HashSet::new(),
            search: None,
            rows: Vec::new(),
            selected: 0,
            status: None,
            help: false,
            done: false,
            favorites_path: favorites::path(),
            ssh_config_path: ssh_config::default_path(),
        };
        app.reload();
        app
    }

    pub fn reload(&mut self) {
        self.hosts = match &self.ssh_config_path {
            Some(path) => ssh_config::hosts(path),
            None => Vec::new(),
        };
        self.favorites = match &self.favorites_path {
            Some(path) => favorites::load(path),
            None => HashSet::new(),
        };
        self.status = self
            .hosts
            .is_empty()
            .then(|| "no hosts found in ~/.ssh/config".to_string());
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let previous = self.rows.get(self.selected).and_then(|r| r.key());
        self.rows = match &self.search {
            Some(query) => self.search_rows(query),
            None => self.tree_rows(),
        };
        // Keep the cursor on the same row across a rebuild where possible.
        self.selected = previous
            .and_then(|key| self.rows.iter().position(|r| r.key().as_deref() == Some(&key)))
            .unwrap_or(0);
        self.clamp_to_selectable(1);
    }

    fn position_of(&self, key: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.key().as_deref() == Some(key))
    }

    fn host_row(&self, host: &Host, depth: usize, context: Option<String>) -> Row {
        self.host_row_in(host, depth, context, false)
    }

    fn host_row_in(
        &self,
        host: &Host,
        depth: usize,
        context: Option<String>,
        shortcut: bool,
    ) -> Row {
        Row::Host {
            alias: host.alias.clone(),
            depth,
            favorite: self.favorites.contains(&host.alias),
            shortcut,
            context,
            note: host.note.clone(),
        }
    }

    fn tree_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();

        let starred: Vec<&Host> = self
            .hosts
            .iter()
            .filter(|h| self.favorites.contains(&h.alias))
            .collect();
        if !starred.is_empty() {
            rows.push(Row::Section { label: "★ Favorites".into() });
            for host in starred {
                // A favorite is a shortcut; the host keeps its place in the tree.
                rows.push(self.host_row_in(host, 1, None, true));
            }
        }

        self.push_folders(&mut Vec::new(), 0, &mut rows);

        let ungrouped: Vec<&Host> = self.hosts.iter().filter(|h| h.folder.is_empty()).collect();
        if !ungrouped.is_empty() {
            // Hosts written above any heading. A config with no headings at all
            // is just this section, which is the right default.
            rows.push(Row::Section { label: "Ungrouped".into() });
            for host in ungrouped {
                rows.push(self.host_row(host, 1, None));
            }
        }
        rows
    }

    /// Folders exist only as the section paths hosts were written under, so the
    /// tree is discovered from those paths, in the order the config lists them.
    fn child_folders(&self, prefix: &[String]) -> Vec<String> {
        let mut children: Vec<String> = Vec::new();
        for host in &self.hosts {
            if host.folder.len() > prefix.len() && host.folder.starts_with(prefix) {
                let child = &host.folder[prefix.len()];
                if !children.contains(child) {
                    children.push(child.clone());
                }
            }
        }
        children
    }

    fn push_folders(&self, prefix: &mut Vec<String>, depth: usize, rows: &mut Vec<Row>) {
        for child in self.child_folders(prefix) {
            prefix.push(child.clone());
            let id = prefix.join("\u{1}");
            let collapsed = self.collapsed.contains(&id);
            rows.push(Row::Folder { id, name: child, depth, collapsed });
            if !collapsed {
                self.push_folders(prefix, depth + 1, rows);
                for host in self.hosts.iter().filter(|h| h.folder == *prefix) {
                    rows.push(self.host_row(host, depth + 1, None));
                }
            }
            prefix.pop();
        }
    }

    /// Search is flat on purpose: when you are looking for `prod` you want every
    /// match, not the folders they happen to live in.
    fn search_rows(&self, query: &str) -> Vec<Row> {
        self.hosts
            .iter()
            .filter(|host| {
                let haystack = [
                    host.alias.as_str(),
                    &host.folder_path(),
                    host.note.as_deref().unwrap_or(""),
                ]
                .join(" ");
                fuzzy_match(query, &haystack)
            })
            .map(|host| {
                let context = (!host.folder.is_empty()).then(|| host.folder_path());
                self.host_row(host, 0, context)
            })
            .collect()
    }

    /// Hosts known, not rows drawn: favorites appear twice and a collapsed
    /// folder hides its own.
    pub fn host_count(&self) -> usize {
        match self.search {
            Some(_) => self.rows.len(),
            None => self.hosts.len(),
        }
    }

    pub fn selected_alias(&self) -> Option<&str> {
        self.rows.get(self.selected)?.alias()
    }

    pub fn move_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
        self.clamp_to_selectable(delta.signum());
    }

    /// Section headers are labels, not targets: step past them.
    fn clamp_to_selectable(&mut self, direction: isize) {
        let step = if direction < 0 { -1 } else { 1 };
        for _ in 0..self.rows.len() {
            match self.rows.get(self.selected) {
                Some(row) if !row.selectable() => {
                    let len = self.rows.len() as isize;
                    self.selected = (self.selected as isize + step).rem_euclid(len) as usize;
                }
                _ => return,
            }
        }
    }

    /// Right arrow: open the folder under the cursor and step into it.
    pub fn enter_folder(&mut self) {
        let Some(Row::Folder { id, collapsed, depth, .. }) = self.rows.get(self.selected) else {
            return;
        };
        let (id, depth) = (id.clone(), *depth);
        if *collapsed {
            self.collapsed.remove(&id);
            self.rebuild();
        }
        let Some(index) = self.position_of(&format!("f:{id}")) else { return };
        // The next row belongs to this folder only if it sits deeper.
        if self.rows.get(index + 1).is_some_and(|r| r.depth() > depth) {
            self.selected = index + 1;
            self.clamp_to_selectable(1);
        }
    }

    /// Left arrow: close an open folder, otherwise step out to the parent.
    pub fn leave_folder(&mut self) {
        let Some(row) = self.rows.get(self.selected) else { return };
        if let Row::Folder { id, collapsed: false, .. } = row {
            let id = id.clone();
            self.collapsed.insert(id);
            return self.rebuild();
        }
        let depth = row.depth();
        // Nearest folder above that is shallower — but stop at a section
        // header: a host under ★ Favorites or Ungrouped has no parent folder,
        // and the nearest one above it is unrelated.
        for index in (0..self.selected).rev() {
            match &self.rows[index] {
                Row::Section { .. } => return,
                Row::Folder { depth: above, .. } if *above < depth => {
                    self.selected = index;
                    return;
                }
                _ => {}
            }
        }
    }

    pub fn toggle_folder(&mut self) {
        if let Some(Row::Folder { id, .. }) = self.rows.get(self.selected) {
            let id = id.clone();
            if !self.collapsed.remove(&id) {
                self.collapsed.insert(id);
            }
            self.rebuild();
        }
    }

    pub fn toggle_favorite(&mut self) {
        let Some(alias) = self.selected_alias().map(str::to_string) else { return };
        if !self.favorites.remove(&alias) {
            self.favorites.insert(alias);
        }
        match &self.favorites_path {
            Some(path) => {
                if let Err(e) = favorites::save(path, &self.favorites) {
                    self.status = Some(e);
                }
            }
            None => self.status = Some("no plugin state dir; favorites are not saved".into()),
        }
        self.rebuild();
    }

    pub fn start_search(&mut self) {
        self.search = Some(String::new());
        self.rebuild();
    }

    /// Esc is two-stage: leave search first, close the popup only when there is
    /// no search to leave.
    pub fn cancel(&mut self) {
        if self.help {
            self.help = false;
        } else if self.search.take().is_some() {
            self.rebuild();
        } else {
            self.done = true;
        }
    }

    pub fn search_push(&mut self, c: char) {
        if let Some(query) = &mut self.search {
            query.push(c);
            self.rebuild();
        }
    }

    pub fn search_pop(&mut self) {
        if let Some(query) = &mut self.search {
            query.pop();
            self.rebuild();
        }
    }
}

/// Subsequence match, case-insensitive: `dkp` finds `dokku-prod`. No ranking —
/// the list is short enough that ordering by tree position reads better than by
/// score.
pub fn fuzzy_match(query: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars().flat_map(char::to_lowercase);
    query
        .chars()
        .flat_map(char::to_lowercase)
        .all(|needle| chars.any(|c| c == needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(alias: &str, folder: &[&str]) -> Host {
        Host {
            alias: alias.into(),
            folder: folder.iter().map(|s| s.to_string()).collect(),
            note: None,
        }
    }

    fn app(hosts: Vec<Host>, favorites: &[&str]) -> App {
        let mut app = App {
            hosts,
            favorites: favorites.iter().map(|s| s.to_string()).collect(),
            collapsed: HashSet::new(),
            search: None,
            rows: Vec::new(),
            selected: 0,
            status: None,
            help: false,
            done: false,
            favorites_path: None,
            ssh_config_path: None,
        };
        app.rebuild();
        app
    }

    fn labels(app: &App) -> Vec<String> {
        app.rows
            .iter()
            .map(|r| match r {
                Row::Section { label } => label.clone(),
                Row::Folder { name, depth, .. } => format!("{}[{name}]", "  ".repeat(*depth)),
                Row::Host { alias, depth, .. } => format!("{}{alias}", "  ".repeat(*depth)),
            })
            .collect()
    }

    #[test]
    fn nests_folders_from_section_paths_and_lists_unfiled_hosts() {
        let app = app(
            vec![
                host("dokku-prod", &["Personal", "Hetzner"]),
                host("nas", &["Personal"]),
                host("stray", &[]),
            ],
            &[],
        );
        assert_eq!(
            labels(&app),
            [
                "[Personal]",
                "  [Hetzner]",
                "    dokku-prod",
                "  nas",
                "Ungrouped",
                "  stray",
            ]
        );
    }

    #[test]
    fn a_config_with_no_headings_is_one_flat_section() {
        let app = app(vec![host("a", &[]), host("b", &[])], &[]);
        assert_eq!(labels(&app), ["Ungrouped", "  a", "  b"]);
    }

    #[test]
    fn favorites_appear_on_top_without_leaving_their_folder() {
        let app = app(vec![host("dokku-prod", &["Hetzner"])], &["dokku-prod"]);
        assert_eq!(
            labels(&app),
            ["★ Favorites", "  dokku-prod", "[Hetzner]", "  dokku-prod"]
        );
    }

    #[test]
    fn toggling_a_favorite_updates_the_tree() {
        let mut app = app(vec![host("nas", &[])], &[]);
        assert_eq!(labels(&app), ["Ungrouped", "  nas"]);
        app.toggle_favorite();
        assert_eq!(labels(&app), ["★ Favorites", "  nas", "Ungrouped", "  nas"]);
        // No state dir in tests, so it warns rather than pretending it saved.
        assert!(app.status.is_some());
        app.toggle_favorite();
        assert_eq!(labels(&app), ["Ungrouped", "  nas"]);
    }

    #[test]
    fn starring_a_host_keeps_the_cursor_in_its_folder() {
        let mut app = app(
            vec![host("dokku-prod", &["Hetzner"]), host("postgres", &["Hetzner"])],
            &[],
        );
        // [Hetzner] / dokku-prod / postgres
        app.selected = 2;
        assert_eq!(app.selected_alias(), Some("postgres"));

        app.toggle_favorite();
        // The ★ Favorites copy must not steal the cursor from the folder copy.
        assert_eq!(
            labels(&app),
            ["★ Favorites", "  postgres", "[Hetzner]", "  dokku-prod", "  postgres"]
        );
        assert_eq!(app.selected, 4, "cursor must stay on the in-folder row");
    }

    #[test]
    fn left_from_a_section_host_does_not_jump_into_an_unrelated_folder() {
        let mut app = app(
            vec![host("dokku-prod", &["Hetzner"]), host("stray", &[])],
            &[],
        );
        // [Hetzner] / dokku-prod / Ungrouped / stray
        app.selected = 3;
        assert_eq!(app.selected_alias(), Some("stray"));
        app.leave_folder();
        assert_eq!(app.selected, 3, "an ungrouped host has no parent folder");

        // A host that really is in a folder still steps out.
        app.selected = 1;
        app.leave_folder();
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Hetzner"));
    }

    #[test]
    fn host_count_is_hosts_not_rows() {
        let mut app = app(
            vec![host("a", &["F"]), host("b", &["F"])],
            &["a"],
        );
        // ★ Favorites / a / [F] / a / b  — five rows, still two hosts.
        assert_eq!(app.rows.len(), 5);
        assert_eq!(app.host_count(), 2);

        app.selected = 2;
        app.toggle_folder(); // collapse [F]; its hosts leave the row list
        assert_eq!(app.host_count(), 2);

        assert_eq!(super::tests::app(vec![host("only", &[])], &[]).host_count(), 1);
    }

    #[test]
    fn navigation_skips_section_headers_and_wraps() {
        let mut app = app(vec![host("a", &[]), host("b", &[])], &["a"]);
        // ★ Favorites / a / Ungrouped / a / b
        assert_eq!(app.selected, 1);
        app.move_by(1);
        assert_eq!(app.selected_alias(), Some("a")); // header at index 2 skipped
        app.move_by(1);
        assert_eq!(app.selected_alias(), Some("b"));
        app.move_by(1);
        assert_eq!(app.selected, 1); // wraps past the leading header
        app.move_by(-1);
        assert_eq!(app.selected_alias(), Some("b"));
    }

    #[test]
    fn expanding_a_folder_keeps_the_cursor_on_it() {
        let mut app = app(
            vec![host("x", &["Personal"]), host("y", &["Uni"])],
            &[],
        );
        app.selected = 2; // [Uni]
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Uni"));
        app.toggle_folder();
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Uni"));
        app.toggle_folder();
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Uni"));
    }

    #[test]
    fn right_opens_and_steps_into_a_folder_left_steps_back_out() {
        let mut app = app(vec![host("dokku-prod", &["Personal", "Hetzner"])], &[]);
        app.selected = 0; // [Personal]
        app.leave_folder(); // collapse
        assert_eq!(labels(&app), ["[Personal]"]);

        app.enter_folder(); // reopen and descend
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Hetzner"));

        app.enter_folder(); // into the host
        assert_eq!(app.selected_alias(), Some("dokku-prod"));

        app.leave_folder();
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Hetzner"));
        app.leave_folder(); // collapses Hetzner, cursor stays
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Hetzner"));
        app.leave_folder(); // now steps out
        assert!(matches!(&app.rows[app.selected], Row::Folder { name, .. } if name == "Personal"));
    }

    #[test]
    fn right_on_a_host_or_a_closed_empty_folder_does_nothing() {
        let mut app = app(vec![host("stray", &[])], &[]);
        let before = app.selected;
        app.enter_folder();
        assert_eq!(app.selected, before);
    }

    #[test]
    fn search_is_flat_and_matches_alias_folder_and_note() {
        let mut app = app(
            vec![
                Host { alias: "gitlab".into(), folder: vec!["Prod".into()], note: None },
                Host { alias: "nas".into(), folder: vec![], note: Some("backup box".into()) },
                host("web-prod", &[]),
            ],
            &[],
        );

        app.start_search();
        for c in "prod".chars() {
            app.search_push(c);
        }
        // gitlab via its folder, web-prod via its alias; nas matches neither.
        assert_eq!(labels(&app), ["gitlab", "web-prod"]);

        app.cancel();
        app.start_search();
        for c in "backup".chars() {
            app.search_push(c);
        }
        assert_eq!(labels(&app), ["nas"]);
    }

    #[test]
    fn escape_leaves_search_before_closing_the_popup() {
        let mut app = app(vec![host("a", &[])], &[]);
        app.start_search();
        app.cancel();
        assert!(app.search.is_none() && !app.done);
        app.cancel();
        assert!(app.done);
    }

    #[test]
    fn fuzzy_matches_subsequences_case_insensitively() {
        assert!(fuzzy_match("dkp", "dokku-prod"));
        assert!(fuzzy_match("PROD", "web-prod"));
        assert!(fuzzy_match("", "anything"));
        assert!(!fuzzy_match("dpk", "dokku-prod"));
        assert!(!fuzzy_match("zz", "dokku-prod"));
    }
}
