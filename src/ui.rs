//! Rendering. No colours are hardcoded: the popup borrows whatever palette the
//! user's terminal theme already provides, and uses attributes for emphasis.

use crate::app::{App, Row};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

const HELP: &str = "  ↑ / k      previous
  ↓ / j      next
  → / l      open folder, step in
  ← / h      close folder, step out
  space      expand / collapse
  enter      connect, or return to an open session
  shift+↵    always open a new tab for this host
  /          search (alias, folder, note)
  esc        leave search, then close
  f          toggle favorite
  r          reload ~/.ssh/config
  ?          help
  q          quit";

pub fn draw(frame: &mut Frame, app: &mut App, list_state: &mut ListState) {
    let [list_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    // No border or title here: Herdr already frames the popup and puts the
    // entrypoint title on it, so drawing our own repeats it and costs two rows.
    let items: Vec<ListItem> = app.rows.iter().map(row_item).collect();
    let list =
        List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    // The state carries the scroll offset between frames. A fresh one each
    // frame would start at offset 0, which pins the selection to the last
    // visible row once the list is taller than the popup.
    list_state.select(Some(app.selected));
    frame.render_stateful_widget(list, list_area, list_state);

    frame.render_widget(status_line(app), status_area);

    if app.help {
        draw_help(frame, list_area);
    }
}

fn row_item(row: &Row) -> ListItem<'static> {
    let dim = Style::default().add_modifier(Modifier::DIM);
    match row {
        Row::Section { label } => ListItem::new(Line::from(Span::styled(
            format!(" {label}"),
            Style::default().add_modifier(Modifier::BOLD),
        ))),
        Row::Folder { name, depth, collapsed, .. } => {
            let marker = if *collapsed { '▸' } else { '▾' };
            ListItem::new(Line::from(Span::styled(
                format!(" {}{marker} {name}", "  ".repeat(*depth)),
                Style::default().add_modifier(Modifier::BOLD),
            )))
        }
        Row::Host { alias, depth, favorite, context, note, .. } => {
            let mut spans = vec![
                Span::raw(format!(" {}● ", "  ".repeat(*depth))),
                Span::raw(alias.clone()),
            ];
            if *favorite {
                spans.push(Span::raw(" ★"));
            }
            if let Some(context) = context {
                spans.push(Span::styled(format!("  {context}"), dim));
            }
            if let Some(note) = note {
                spans.push(Span::styled(format!("  — {note}"), dim));
            }
            ListItem::new(Line::from(spans))
        }
    }
}

fn status_line(app: &App) -> Paragraph<'static> {
    // Errors outrank the search box: a failed connect must not be hidden by it.
    if let Some(status) = &app.status {
        return Paragraph::new(format!(" {status}"))
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));
    }
    let text = match &app.search {
        Some(query) => format!(" /{query}▏"),
        None => {
            let hosts = app.host_count();
            format!(" {hosts} host{}   ? help", if hosts == 1 { "" } else { "s" })
        }
    };
    Paragraph::new(text).style(Style::default().add_modifier(Modifier::DIM))
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(56);
    let height = (HELP.lines().count() as u16 + 2).min(area.height);
    let area = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(HELP)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" KEYS ")),
        area,
    );
}
