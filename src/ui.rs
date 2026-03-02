use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};

use crate::app::{App, BrowseFocus, QueueFocus, Screen};

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "ARCH PACKAGE TUI ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("| keyboard-first | "),
        Span::styled(
            "fuzzy search + autonomous install",
            Style::default().fg(Color::LightGreen),
        ),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Neo Installer"),
    );
    frame.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(chunks[1]);

    match app.screen {
        Screen::Queue => render_queue_panel(frame, app, body[0]),
        Screen::Browse => render_browse_panel(frame, app, body[0]),
        Screen::Installing => render_install_panel(frame, app, body[0]),
        Screen::Done => render_done_panel(frame, app, body[0]),
    }

    render_art_panel(frame, app, body[1]);

    let footer_text = format!(
        "1:Queue  2:Browse  i:Install  q:{}  status: {}",
        if app.screen == Screen::Installing {
            "Abort"
        } else {
            "Quit"
        },
        app.status_line
    );
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

fn render_queue_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Manual Packages (comma or space separated)")
        .border_style(if app.queue_focus == QueueFocus::Input {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let input = Paragraph::new(app.manual_input.as_str()).block(input_block);
    frame.render_widget(input, sections[0]);

    let priority_text = format!(
        "Priority: {}  |  Dry-run: {}",
        app.priority_text(),
        if app.dry_run { "ON" } else { "OFF" }
    );
    let priority_block = Block::default()
        .borders(Borders::ALL)
        .title("Priority Chain (left/right to cycle, t to toggle dry-run)")
        .border_style(if app.queue_focus == QueueFocus::Priority {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let priority = Paragraph::new(priority_text).block(priority_block);
    frame.render_widget(priority, sections[1]);

    let list_items = if app.queue.is_empty() {
        vec![ListItem::new("(queue empty)")]
    } else {
        app.queue
            .iter()
            .enumerate()
            .map(|(idx, pkg)| {
                let mut style = Style::default();
                if idx == app.queue_cursor && app.queue_focus == QueueFocus::Queue {
                    style = style
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD);
                }
                ListItem::new(Line::from(Span::styled(pkg.clone(), style)))
            })
            .collect::<Vec<_>>()
    };

    let queue = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Install Queue (d remove, x clear)")
            .border_style(if app.queue_focus == QueueFocus::Queue {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }),
    );
    frame.render_widget(queue, sections[2]);

    let actions = Paragraph::new(
        "a: add input  b: browse packages  enter on install row or i: start install",
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Actions")
            .border_style(if app.queue_focus == QueueFocus::Install {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(actions, sections[3]);
}

fn render_browse_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(6)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .title("Fuzzy Search (/ to focus, esc clears)")
        .border_style(if app.browse_focus == BrowseFocus::Search {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
    let search = Paragraph::new(app.search_query.as_str()).block(search_block);
    frame.render_widget(search, sections[0]);

    let rows = if app.matches.is_empty() {
        vec![ListItem::new("No matches")]
    } else {
        app.matches
            .iter()
            .enumerate()
            .filter_map(|(row, pkg_idx)| app.packages.get(*pkg_idx).map(|pkg| (row, pkg)))
            .map(|(row, pkg)| {
                let in_queue = app.queue.iter().any(|queued| queued == &pkg.name);
                let marker = if in_queue { "[x]" } else { "[ ]" };
                let label = format!("{} {:<45} ({})", marker, pkg.name, pkg.repo.label());
                let mut style = Style::default();
                if row == app.result_cursor && app.browse_focus == BrowseFocus::Results {
                    style = style
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD);
                }
                ListItem::new(Line::from(Span::styled(label, style)))
            })
            .collect::<Vec<_>>()
    };

    let results = List::new(rows).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Results (enter/space add, d remove from queue)")
            .border_style(if app.browse_focus == BrowseFocus::Results {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }),
    );
    frame.render_widget(results, sections[1]);
}

fn render_install_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
        ])
        .split(area);

    let ratio = if app.progress.total == 0 {
        0.0
    } else {
        app.progress.done as f64 / app.progress.total as f64
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Install Progress"),
        )
        .ratio(ratio)
        .label(format!("{} / {}", app.progress.done, app.progress.total))
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(gauge, sections[0]);

    let stage = Paragraph::new(app.progress.stage.clone())
        .block(Block::default().borders(Borders::ALL).title("Stage"));
    frame.render_widget(stage, sections[1]);

    let stats = Paragraph::new(format!(
        "installed={} skipped={} failed={}",
        app.progress.installed, app.progress.skipped, app.progress.failed
    ))
    .block(Block::default().borders(Borders::ALL).title("Stats"));
    frame.render_widget(stats, sections[2]);

    let log_items = app
        .logs
        .iter()
        .rev()
        .take(sections[3].height.saturating_sub(2) as usize)
        .map(|line| ListItem::new(line.clone()))
        .collect::<Vec<_>>();
    let logs = List::new(log_items).block(Block::default().borders(Borders::ALL).title("Live Log"));
    frame.render_widget(logs, sections[3]);
}

fn render_done_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(6)])
        .split(area);

    let mut lines = vec![Line::from("No summary available")];
    if let Some(summary) = &app.summary {
        lines = vec![
            Line::from(format!("Installed: {}", summary.installed)),
            Line::from(format!("Skipped: {}", summary.skipped)),
            Line::from(format!("Failed: {}", summary.failed)),
            Line::from(format!("Aborted: {}", summary.aborted)),
            Line::from(format!("Elapsed: {} sec", summary.elapsed.as_secs())),
            Line::from(format!("Log: {}", summary.log_path.display())),
        ];
    }

    let top = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Install Summary"),
    );
    frame.render_widget(top, sections[0]);

    let list_items = if let Some(summary) = &app.summary {
        if summary.unresolved.is_empty() {
            vec![ListItem::new("All queued packages resolved")]
        } else {
            summary
                .unresolved
                .iter()
                .map(|pkg| ListItem::new(pkg.clone()))
                .collect::<Vec<_>>()
        }
    } else {
        vec![ListItem::new("No summary")]
    };

    let unresolved = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Unresolved Packages (r back to queue)"),
    );
    frame.render_widget(unresolved, sections[1]);
}

fn render_art_panel(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });

    let art = app.art_frame(inner.width, inner.height);
    let color = palette_color(art.palette);
    let text = Text::from(art.lines.join("\n"));

    let widget = Paragraph::new(text)
        .style(Style::default().fg(color))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{} (changes every 10s)", art.title)),
        );
    frame.render_widget(widget, area);
}

fn palette_color(index: usize) -> Color {
    match index % 5 {
        0 => Color::Green,
        1 => Color::Cyan,
        2 => Color::LightGreen,
        3 => Color::LightBlue,
        _ => Color::Yellow,
    }
}
