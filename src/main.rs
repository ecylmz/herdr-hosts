//! SSH host picker. Herdr opens this as a popup; it exits as soon as a host is
//! chosen, which is what closes the popup.

use herdr_hosts::app::App;
use herdr_hosts::{herdr, ui};
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};
use ratatui::widgets::ListState;
use ratatui::DefaultTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args().any(|a| a == "--open") {
        // Invoked by the plugin action behind the keybinding, not as the popup.
        return match herdr::open_picker() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("herdr-hosts: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if !herdr::inside_herdr() {
        eprintln!("herdr-hosts: not running inside Herdr; start it with `herdr plugin action invoke herdr-hosts.open`");
        return ExitCode::FAILURE;
    }

    // ratatui::init installs a panic hook that restores the terminal first, so
    // a panic inside the popup cannot leave the terminal in raw mode.
    let terminal = ratatui::init();
    let disambiguated = enable_key_disambiguation();
    let result = run(terminal);
    if disambiguated {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("herdr-hosts: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Terminals fold shift+enter into a plain Enter unless the kitty keyboard
/// protocol is on, which would make "always open a new tab" unreachable.
/// Measured on Herdr 0.9.1: supported, and the modifier arrives once pushed.
/// Where it is unsupported, shift+enter simply behaves like Enter.
fn enable_key_disambiguation() -> bool {
    if !terminal::supports_keyboard_enhancement().unwrap_or(false) {
        return false;
    }
    execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .is_ok()
}

fn run(mut terminal: DefaultTerminal) -> std::io::Result<()> {
    let mut app = App::new();
    let mut list_state = ListState::default();
    while !app.done {
        terminal.draw(|frame| ui::draw(frame, &mut app, &mut list_state))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                on_key(&mut app, key.code, key.modifiers);
            }
        }
    }
    Ok(())
}

fn on_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    // Any keypress clears a stale message, so the status line can go back to
    // showing the search box.
    app.status = None;

    if app.help {
        app.help = false;
        return;
    }

    // While searching, plain characters are query text rather than commands.
    if app.search.is_some() {
        match code {
            KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                return app.search_push(c)
            }
            KeyCode::Backspace => return app.search_pop(),
            _ => {}
        }
    }

    match code {
        KeyCode::Esc => app.cancel(),
        KeyCode::Char('q') => app.done = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => app.done = true,
        KeyCode::Down | KeyCode::Char('j') => app.move_by(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_by(-1),
        KeyCode::Right | KeyCode::Char('l') => app.enter_folder(),
        KeyCode::Left | KeyCode::Char('h') => app.leave_folder(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('f') => app.toggle_favorite(),
        KeyCode::Char('r') => app.reload(),
        KeyCode::Char('?') => app.help = true,
        KeyCode::Char(' ') => app.toggle_folder(),
        // Enter returns to an existing session; shift+enter always starts a
        // new one, for a second window onto the same host.
        KeyCode::Enter => connect(app, modifiers.contains(KeyModifiers::SHIFT)),
        _ => {}
    }
}

fn connect(app: &mut App, force_new: bool) {
    let Some(alias) = app.selected_alias().map(str::to_string) else {
        return app.toggle_folder(); // Enter on a folder expands it.
    };

    // A second Enter on the same host goes back to that session rather than
    // opening a duplicate one — unless a duplicate is exactly what was asked
    // for, which is what shift+enter means.
    if !force_new {
        if let Some(pane) = herdr::find_ssh_pane(&alias) {
            match herdr::focus_pane(&pane) {
                Ok(()) => app.done = true,
                Err(e) => app.status = Some(e),
            }
            return;
        }
    }

    match herdr::open_ssh(&alias) {
        // The popup closes on exit, landing on the session's tab.
        Ok(_) => app.done = true,
        Err(e) => app.status = Some(e),
    }
}
