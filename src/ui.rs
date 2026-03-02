use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::app::App;
use crate::model::{FeedLevel, Mode, Overlay, SyncState, keybinds};

const BG: Color = Color::Rgb(13, 17, 23);
const SURFACE: Color = Color::Rgb(22, 27, 34);
const BORDER: Color = Color::Rgb(48, 54, 61);
const TEXT_PRIMARY: Color = Color::Rgb(230, 237, 243);
const TEXT_MUTED: Color = Color::Rgb(139, 148, 158);
const ACCENT: Color = Color::Rgb(88, 166, 255);
const SUCCESS: Color = Color::Rgb(63, 185, 80);
const WARNING: Color = Color::Rgb(210, 153, 34);
const ERROR: Color = Color::Rgb(248, 81, 73);
const HIGHLIGHT: Color = Color::Rgb(31, 111, 235);

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, app, root[0]);
    render_command_bar(frame, app, root[1]);
    render_center(frame, app, root[2]);
    render_queue_bar(frame, app, root[3]);
    render_overlay(frame, app);

    if frame.area().width < 60 || frame.area().height < 20 {
        let warn = Rect {
            x: 0,
            y: frame.area().height.saturating_sub(1),
            width: frame.area().width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("terminal too small: minimum 60x20 recommended")
                .style(Style::default().fg(WARNING).bg(BG)),
            warn,
        );
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " arch-pkg ",
            Style::default()
                .fg(ACCENT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    for (idx, backend) in app.backend_priority.iter().enumerate() {
        let available = app.backend_available(*backend);
        let active = app.backend_cursor == idx;
        let fg = if available { SUCCESS } else { ERROR };
        let label = format!("[{} {}]", backend.bin(), if available { "✓" } else { "✗" });
        spans.push(Span::styled(
            label,
            Style::default().fg(fg).add_modifier(if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
        spans.push(Span::raw(" "));
    }

    let sync_badge = match app.sync_state {
        SyncState::Idle => "[cache]".to_string(),
        SyncState::Syncing => "[syncing…]".to_string(),
        SyncState::Ready(count) => format!("[✓ {} packages]", count),
        SyncState::Error => "[sync error]".to_string(),
    };

    spans.push(Span::styled(sync_badge, Style::default().fg(TEXT_MUTED)));
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        format!("[{}]", app.mode.as_str()),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        Local::now().format("%Y-%m-%d %H:%M").to_string(),
        Style::default().fg(TEXT_MUTED),
    ));

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
        area,
    );
}

fn render_command_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = match app.mode {
        Mode::Command => format!(":{}", app.command_input),
        Mode::Filter => format!("/{}", app.filter_input),
        Mode::Normal => {
            if app.filter_input.is_empty() {
                ":search / :install / :remove / :info / :sync / :save / :load / :orphans / :history"
                    .to_string()
            } else {
                format!("filter: {}", app.filter_input)
            }
        }
    };

    let block = Block::default()
        .title(" COMMAND BAR ")
        .borders(Borders::ALL)
        .style(Style::default().bg(SURFACE).fg(TEXT_PRIMARY))
        .border_style(Style::default().fg(ACCENT));

    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(TEXT_PRIMARY).bg(SURFACE))
            .block(block),
        area,
    );
}

fn render_center(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(46),
            Constraint::Percentage(27),
            Constraint::Percentage(27),
        ])
        .split(area);

    render_package_table(frame, app, panes[0]);
    render_detail(frame, app, panes[1]);
    render_feed(frame, app, panes[2]);
}

fn render_package_table(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let max_rows = area.height.saturating_sub(3) as usize;
    let start = app.selected.saturating_sub(max_rows / 2);

    let rows = app
        .visible
        .iter()
        .enumerate()
        .skip(start)
        .take(max_rows)
        .filter_map(|(row_idx, pkg_idx)| app.packages.get(*pkg_idx).map(|pkg| (row_idx, pkg)))
        .map(|(row_idx, pkg)| {
            let status_color = if pkg.upgradable {
                WARNING
            } else if pkg.installed {
                SUCCESS
            } else {
                TEXT_MUTED
            };

            let repo_color = if pkg.repo == "aur" || pkg.repo.contains("aur") {
                WARNING
            } else {
                TEXT_MUTED
            };

            let selected = row_idx == app.selected;
            let row_style = if selected {
                Style::default().fg(TEXT_PRIMARY).bg(HIGHLIGHT)
            } else {
                Style::default().fg(TEXT_PRIMARY).bg(SURFACE)
            };

            Row::new(vec![
                Cell::from(Span::styled(
                    pkg.status_glyph(),
                    Style::default().fg(status_color),
                )),
                Cell::from(pkg.name.clone()),
                Cell::from(pkg.version.clone()),
                Cell::from(pkg.size_human()),
                Cell::from(Span::styled(
                    pkg.repo_badge(),
                    Style::default().fg(repo_color),
                )),
                Cell::from(truncate(&pkg.description, area.width as usize)),
            ])
            .style(row_style)
        })
        .collect::<Vec<_>>();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(20),
            Constraint::Length(13),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Min(8),
        ],
    )
    .header(
        Row::new(vec![" ", "Name", "Version", "Size", "Repo", "Description"]).style(
            Style::default()
                .fg(TEXT_MUTED)
                .bg(SURFACE)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .title(format!(
                " PACKAGE LIST  (sort: {}) ",
                app.sort_mode.as_str()
            ))
            .borders(Borders::ALL)
            .style(Style::default().bg(SURFACE))
            .border_style(focus_border(app, "list")),
    )
    .column_spacing(1)
    .style(Style::default().bg(SURFACE));

    frame.render_widget(table, area);
}

fn render_detail(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let lines = if app.detail_lines.is_empty() {
        vec![Line::from("No detail available")]
    } else {
        app.detail_lines
            .iter()
            .take(area.height.saturating_sub(2) as usize)
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>()
    };

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT_PRIMARY).bg(SURFACE))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .title(" DETAIL PANE ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(SURFACE))
                    .border_style(focus_border(app, "detail")),
            ),
        area,
    );
}

fn render_feed(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let max = area.height.saturating_sub(2) as usize;
    let lines = app
        .feed
        .iter()
        .rev()
        .take(max)
        .map(|entry| {
            let mark = match entry.level {
                FeedLevel::Resolve => ("○", TEXT_MUTED),
                FeedLevel::Active => ("●", ACCENT),
                FeedLevel::Done => ("✓", SUCCESS),
                FeedLevel::Error => ("✗", ERROR),
                FeedLevel::Warning => ("!", WARNING),
                FeedLevel::Info => ("·", TEXT_MUTED),
            };

            Line::from(vec![
                Span::styled(format!("{} ", entry.ts), Style::default().fg(TEXT_MUTED)),
                Span::styled(format!("{} ", mark.0), Style::default().fg(mark.1)),
                Span::styled(entry.text.clone(), Style::default().fg(TEXT_PRIMARY)),
            ])
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT_PRIMARY).bg(SURFACE))
            .block(
                Block::default()
                    .title(" ACTIVITY FEED ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(SURFACE))
                    .border_style(focus_border(app, "feed")),
            ),
        area,
    );
}

fn render_queue_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let (labels, more) = app.queue_preview_labels(6);
    let mut spans = vec![Span::styled(
        "QUEUE ",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];

    for item in labels {
        spans.push(Span::styled(
            format!("{} ", item),
            Style::default().fg(TEXT_PRIMARY),
        ));
    }

    if more > 0 {
        spans.push(Span::styled(
            format!("+{} more  ", more),
            Style::default().fg(TEXT_MUTED),
        ));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("[DRY-RUN: {}]", if app.dry_run { "ON" } else { "OFF" }),
        Style::default().fg(if app.dry_run { WARNING } else { TEXT_MUTED }),
    ));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("[i] install", Style::default().fg(SUCCESS)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("[focus:{}]", app.focus.as_str()),
        Style::default().fg(TEXT_MUTED),
    ));

    let queue_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(SURFACE).fg(TEXT_PRIMARY))
        .border_style(Style::default().fg(BORDER));

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(SURFACE).fg(TEXT_PRIMARY))
            .block(queue_block),
        area,
    );
}

fn render_overlay(frame: &mut Frame<'_>, app: &App) {
    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => {
            let area = centered_rect(74, 72, frame.area());
            let mut lines = vec![Line::from(Span::styled(
                "Keybinds",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))];
            for keybind in keybinds() {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:>12}", keybind.key), Style::default().fg(WARNING)),
                    Span::raw("  "),
                    Span::styled(keybind.action, Style::default().fg(TEXT_PRIMARY)),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Esc to close",
                Style::default().fg(TEXT_MUTED),
            )));

            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Left).block(
                    Block::default()
                        .title(" HELP ")
                        .borders(Borders::ALL)
                        .style(Style::default().bg(SURFACE))
                        .border_style(Style::default().fg(ACCENT)),
                ),
                area,
            );
        }
        Overlay::ConfirmQuit => {
            let area = centered_rect(44, 26, frame.area());
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new("Queue is non-empty. Quit anyway?\n\nEnter/y = yes, Esc/n = no")
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .title(" Confirm Quit ")
                            .borders(Borders::ALL)
                            .style(Style::default().bg(SURFACE))
                            .border_style(Style::default().fg(WARNING)),
                    ),
                area,
            );
        }
        Overlay::InstallPreview(preview) => {
            let area = centered_rect(64, 40, frame.area());
            frame.render_widget(Clear, area);
            let lines = vec![
                Line::from(format!("Installing {} queued item(s)", preview.pkg_count)),
                Line::from(format!("Dependencies (estimated): {}", preview.dep_count)),
                Line::from(format!(
                    "Total download (estimated): {}",
                    preview.total_download
                )),
                Line::from(""),
                Line::from("Enter = confirm   Esc = cancel"),
            ];
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Left).block(
                    Block::default()
                        .title(" Install Preview ")
                        .borders(Borders::ALL)
                        .style(Style::default().bg(SURFACE))
                        .border_style(Style::default().fg(ACCENT)),
                ),
                area,
            );
        }
    }
}

fn focus_border(app: &App, pane: &str) -> Style {
    let focused = match pane {
        "list" => app.focus == crate::model::PaneFocus::List,
        "detail" => app.focus == crate::model::PaneFocus::Detail,
        "feed" => app.focus == crate::model::PaneFocus::Feed,
        _ => false,
    };

    if focused && app.mode == Mode::Normal {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(BORDER)
    }
}

fn truncate(text: &str, width: usize) -> String {
    if width < 4 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}
