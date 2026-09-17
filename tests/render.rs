//! Scrolling. The picker draws a fresh frame on every keypress, so the list
//! state has to survive between them — a new one each frame starts at offset 0,
//! which glues the selection to the bottom row of a list taller than the popup.

use herdr_hosts::app::App;
use herdr_hosts::ui;
use ratatui::backend::TestBackend;
use ratatui::widgets::ListState;
use ratatui::Terminal;
use std::path::PathBuf;

fn many_hosts() -> PathBuf {
    let root = std::env::temp_dir().join(format!("herdr-hosts-scroll-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ssh")).unwrap();
    let config: String = (0..50).map(|i| format!("Host host{i:02}\n")).collect();
    std::fs::write(root.join(".ssh/config"), config).unwrap();
    root
}

#[test]
fn the_list_scrolls_and_keeps_its_offset_between_frames() {
    let root = many_hosts();
    std::env::set_var("HOME", &root);
    std::env::remove_var("HERDR_PLUGIN_STATE_DIR");

    let mut app = App::new();
    assert_eq!(app.host_count(), 50);

    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    let mut state = ListState::default();

    // Walk down past the bottom of the viewport.
    for _ in 0..30 {
        app.move_by(1);
        terminal
            .draw(|frame| ui::draw(frame, &mut app, &mut state))
            .unwrap();
    }
    let scrolled = state.offset();
    assert!(scrolled > 0, "the list must have scrolled, offset={scrolled}");

    // Walking back up past the top of the viewport must pull the offset with
    // it. (Moving inside the viewport correctly leaves the offset alone, so
    // this has to travel far enough to leave it.)
    for _ in 0..25 {
        app.move_by(-1);
        terminal
            .draw(|frame| ui::draw(frame, &mut app, &mut state))
            .unwrap();
    }
    assert!(
        state.offset() < scrolled,
        "offset should follow the cursor back up: {} vs {scrolled}",
        state.offset()
    );

    // The selected host is actually on screen, and not pinned to the last row.
    let selected = app.selected_alias().unwrap().to_string();
    let rendered = terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect::<String>();
    assert!(rendered.contains(&selected), "{selected} must be visible");
}
