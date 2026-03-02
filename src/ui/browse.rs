use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, BrowseFocus};

use super::{border_style, focus_title, selected_style};

pub(super) fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(6)])
        .split(area);

    render_search(frame, app, sections[0]);
    render_results(frame, app, sections[1]);
}

fn render_search(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.browse_focus == BrowseFocus::Search;
    let loading = if app.search_loading {
        format!(" [loading {}]", app.spinner_char())
    } else {
        String::new()
    };

    let search = Paragraph::new(app.search_query.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title(
                &format!("Search (/ to focus, Esc returns here){loading}"),
                focused,
            ))
            .border_style(border_style(focused)),
    );
    frame.render_widget(search, area);
}

fn render_results(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.browse_focus == BrowseFocus::Results;

    let rows = if app.search_loading && app.matches.is_empty() {
        vec![ListItem::new(Span::styled(
            format!("Searching {}", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ))]
    } else if app.matches.is_empty() {
        vec![ListItem::new("No matches")]
    } else {
        app.matches
            .iter()
            .enumerate()
            .filter_map(|(row, pkg_idx)| app.packages.get(*pkg_idx).map(|pkg| (row, pkg)))
            .map(|(row, pkg)| {
                let marker = if app.queue_contains(&pkg.name) {
                    Span::styled("[✓]", Style::default().fg(Color::Green))
                } else {
                    Span::styled("[ ]", Style::default().fg(Color::DarkGray))
                };

                let mut item = ListItem::new(Line::from(vec![
                    marker,
                    Span::raw(" "),
                    Span::raw(format!("{:<45}", pkg.name)),
                    Span::styled(
                        format!(" ({})", pkg.repo.label()),
                        Style::default().fg(Color::Gray),
                    ),
                ]));

                if row == app.result_cursor && focused {
                    item = item.style(selected_style());
                }
                item
            })
            .collect::<Vec<_>>()
    };

    let results = List::new(rows).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title(
                &format!(
                    "Results: {} (Enter/Space add, d remove)",
                    app.search_result_count()
                ),
                focused,
            ))
            .border_style(border_style(focused)),
    );
    frame.render_widget(results, area);
}
