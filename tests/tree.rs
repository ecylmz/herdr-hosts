//! End-to-end over real files: a ~/.ssh/config with section headings, an
//! Include, trailing-comment notes, and the tree the picker builds from it.

use herdr_hosts::app::{App, Row};
use std::path::PathBuf;

fn label(row: &Row) -> String {
    match row {
        Row::Section { label } => label.clone(),
        Row::Folder { name, depth, .. } => format!("{}[{name}]", "  ".repeat(*depth)),
        Row::Host { alias, depth, favorite, .. } => {
            format!("{}{alias}{}", "  ".repeat(*depth), if *favorite { " ★" } else { "" })
        }
    }
}

fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!("herdr-hosts-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ssh/config.d")).unwrap();
    std::fs::create_dir_all(root.join("state")).unwrap();

    std::fs::write(
        root.join(".ssh/config"),
        "Include config.d/*\n\
         \n\
         Host dokku-prod\n    HostName 1.2.3.4\n\
         \n\
         # this is an ordinary comment, not a heading\n\
         # --- University / Production ---\n\
         Host web-prod   # main web box\n\
         Host gitlab\n\
         \n\
         # === Personal ===\n\
         Host nas\n\
         \n\
         Host *.internal\n    User root\n",
    )
    .unwrap();
    std::fs::write(
        root.join(".ssh/config.d/10-work"),
        "# --- Work ---\nHost jump\n",
    )
    .unwrap();
    root
}

#[test]
fn builds_the_tree_from_ssh_config_alone() {
    let root = fixture();
    std::env::set_var("HOME", &root);
    std::env::set_var("HERDR_PLUGIN_STATE_DIR", root.join("state"));

    let mut app = App::new();
    assert_eq!(
        app.rows.iter().map(label).collect::<Vec<_>>(),
        [
            "[Work]",                 // from the included file's own heading
            "  jump",
            "[University]",
            "  [Production]",
            "    web-prod",
            "    gitlab",
            "[Personal]",
            "  nas",
            "Ungrouped",
            "  dokku-prod",           // written above any heading
        ],
        "*.internal must not be listed, and a plain comment must not group"
    );

    // The trailing comment became a note, not an alias.
    let note = app.rows.iter().find_map(|r| match r {
        Row::Host { alias, note, .. } if alias == "web-prod" => Some(note.clone()),
        _ => None,
    });
    assert_eq!(note, Some(Some("main web box".into())));
    assert!(
        !app.rows.iter().any(|r| r.alias() == Some("main")),
        "words after # must never become hosts"
    );

    // Favorites persist to the state dir, never to ~/.ssh/config.
    let before = std::fs::read_to_string(root.join(".ssh/config")).unwrap();
    app.selected = app.rows.iter().position(|r| r.alias() == Some("nas")).unwrap();
    app.toggle_favorite();
    assert_eq!(app.status, None, "toggling a favorite must not error");
    assert_eq!(
        std::fs::read_to_string(root.join(".ssh/config")).unwrap(),
        before,
        "the plugin must never write to ssh config"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("state/favorites")).unwrap().trim(),
        "nas"
    );

    let reloaded = App::new();
    assert_eq!(
        reloaded.rows.iter().map(label).take(2).collect::<Vec<_>>(),
        ["★ Favorites", "  nas ★"]
    );
}
