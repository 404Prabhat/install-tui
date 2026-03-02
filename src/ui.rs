use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::app::App;
use crate::model::{FeedLevel, Mode, Overlay, PaneFocus, QueueAction, SyncState, keybinds};

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

#[derive(Clone, Copy)]
enum CenterLayout {
    Wide,
    Medium,
    Narrow,
}

struct TableSpec {
    headers: Vec<&'static str>,
    constraints: Vec<Constraint>,
    show_version: bool,
    show_size: bool,
    show_repo: bool,
    desc_budget: usize,
}

pub fn render(frame: &mut Frame<'_>, app: &App) {
    if frame.area().width < 52 || frame.area().height < 16 {
        render_minimal(frame, app);
        return;
    }

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

    if frame.area().width < 80 || frame.area().height < 24 {
        let warn = Rect {
            x: 0,
            y: frame.area().height.saturating_sub(1),
            width: frame.area().width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("compact layout active (<80x24)")
                .style(Style::default().fg(WARNING).bg(BG)),
            warn,
        );
    }
}

fn render_minimal(frame: &mut Frame<'_>, app: &App) {
    let text = vec![
        Line::from(Span::styled(
            "arch-package-tui",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Terminal too small for full layout."),
        Line::from(format!(
            "Current size: {}x{}",
            frame.area().width,
            frame.area().height
        )),
        Line::from("Required for full UI: 80x24 (minimum: 60x20)."),
        Line::from("Resize terminal and reopen."),
        Line::from(""),
        Line::from(format!("Mode: {}", app.mode.as_str())),
    ];

    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(TEXT_PRIMARY).bg(SURFACE))
            .block(
                Block::default()
                    .title(" ARCH PACKAGE TUI ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(SURFACE))
                    .border_style(Style::default().fg(WARNING)),
            ),
        frame.area(),
    );
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sync_badge = match app.sync_state {
        SyncState::Idle => "[cache]".to_string(),
        SyncState::Syncing => "[syncing…]".to_string(),
        SyncState::Ready(count) => format!("[✓ {} packages]", count),
        SyncState::Error => "[sync error]".to_string(),
    };

    let time_text = if area.width >= 110 {
        Local::now().format("%Y-%m-%d %H:%M").to_string()
    } else {
        Local::now().format("%H:%M").to_string()
    };

    let right_text = if area.width >= 110 {
        format!("{}   [{}]   {}", sync_badge, app.mode.as_str(), time_text)
    } else if area.width >= 80 {
        format!("{} [{}] {}", sync_badge, app.mode.as_str(), time_text)
    } else {
        format!("[{}] {}", app.mode.as_str(), time_text)
    };

    let right_width = right_text.chars().count().saturating_add(2) as u16;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(right_width.min(area.width.saturating_sub(1))),
        ])
        .split(area);

    let mut left_spans = vec![
        Span::styled(
            " arch-pkg ",
            Style::default()
                .fg(ACCENT)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];

    let max_badges = if chunks[0].width >= 86 {
        app.backend_priority.len()
    } else if chunks[0].width >= 62 {
        3
    } else if chunks[0].width >= 42 {
        2
    } else {
        1
    };

    for (idx, backend) in app.backend_priority.iter().enumerate().take(max_badges) {
        let available = app.backend_available(*backend);
        let active = app.backend_cursor == idx;
        let fg = if available { SUCCESS } else { ERROR };
        left_spans.push(Span::styled(
            format!("[{} {}]", backend.bin(), if available { "✓" } else { "✗" }),
            Style::default().fg(fg).add_modifier(if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ));
        left_spans.push(Span::raw(" "));
    }

    if app.backend_priority.len() > max_badges {
        left_spans.push(Span::styled(
            format!("+{}", app.backend_priority.len() - max_badges),
            Style::default().fg(TEXT_MUTED),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(left_spans)).style(Style::default().bg(BG)),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(Style::default().bg(BG).fg(TEXT_MUTED)),
        chunks[1],
    );
}

fn render_command_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = match app.mode {
        Mode::Command => format!(":{}", app.command_input),
        Mode::Filter => format!("/{}", app.filter_input),
        Mode::Normal => {
            if app.filter_input.is_empty() {
                if area.width >= 100 {
                    ":search / :install / :remove / :info / :sync / :save / :load / :orphans / :history"
                        .to_string()
                } else {
                    ":search :install :remove :info :sync :save :load :orphans :history".to_string()
                }
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
            .wrap(Wrap { trim: true })
            .block(block),
        area,
    );
}

fn render_center(frame: &mut Frame<'_>, app: &App, area: Rect) {
    match center_layout(area) {
        CenterLayout::Wide => {
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
        CenterLayout::Medium => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
                .split(area);
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(split[1]);

            render_package_table(frame, app, split[0]);
            render_detail(frame, app, right[0]);
            render_feed(frame, app, right[1]);
        }
        CenterLayout::Narrow => {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(52),
                    Constraint::Percentage(24),
                    Constraint::Percentage(24),
                ])
                .split(area);
            render_package_table(frame, app, split[0]);
            render_detail(frame, app, split[1]);
            render_feed(frame, app, split[2]);
        }
    }
}

fn center_layout(area: Rect) -> CenterLayout {
    if area.width >= 140 && area.height >= 18 {
        CenterLayout::Wide
    } else if area.width >= 96 {
        CenterLayout::Medium
    } else {
        CenterLayout::Narrow
    }
}

fn render_package_table(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let spec = table_spec(area.width);
    let max_rows = area.height.saturating_sub(3) as usize;
    let start = app.selected.saturating_sub(max_rows / 2);

    let rows = app
        .visible
        .iter()
        .enumerate()
        .skip(start)
        .take(max_rows)
        .filter_map(|(row_idx, pkg_idx)| app.packages.get(*pkg_idx).map(|pkg| (row_idx, pkg)))
        .map(|(row_idx, pkg)| build_package_row(row_idx, pkg, app, &spec))
        .collect::<Vec<_>>();

    let table = Table::new(rows, spec.constraints)
        .header(
            Row::new(spec.headers).style(
                Style::default()
                    .fg(TEXT_MUTED)
                    .bg(SURFACE)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(
            Block::default()
                .title(format!(
                    " PACKAGE LIST  (sort: {}, {} shown) ",
                    app.sort_mode.as_str(),
                    app.visible.len()
                ))
                .borders(Borders::ALL)
                .style(Style::default().bg(SURFACE))
                .border_style(focus_border(app, PaneFocus::List)),
        )
        .column_spacing(1)
        .style(Style::default().bg(SURFACE));

    frame.render_widget(table, area);
}

fn build_package_row(
    row_idx: usize,
    pkg: &crate::model::PackageRecord,
    app: &App,
    spec: &TableSpec,
) -> Row<'static> {
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

    let mut cells = vec![
        Cell::from(Span::styled(
            pkg.status_glyph(),
            Style::default().fg(status_color),
        )),
        Cell::from(truncate(&pkg.name, 24)),
    ];

    if spec.show_version {
        cells.push(Cell::from(truncate(&pkg.version, 14)));
    }

    if spec.show_size {
        cells.push(Cell::from(pkg.size_human()));
    }

    if spec.show_repo {
        cells.push(Cell::from(Span::styled(
            truncate(&pkg.repo_badge(), 10),
            Style::default().fg(repo_color),
        )));
    }

    cells.push(Cell::from(truncate(&pkg.description, spec.desc_budget)));

    Row::new(cells).style(row_style)
}

fn table_spec(width: u16) -> TableSpec {
    if width >= 118 {
        TableSpec {
            headers: vec![" ", "Name", "Version", "Size", "Repo", "Description"],
            constraints: vec![
                Constraint::Length(2),
                Constraint::Length(22),
                Constraint::Length(13),
                Constraint::Length(9),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
            show_version: true,
            show_size: true,
            show_repo: true,
            desc_budget: width.saturating_sub(66) as usize,
        }
    } else if width >= 92 {
        TableSpec {
            headers: vec![" ", "Name", "Version", "Repo", "Description"],
            constraints: vec![
                Constraint::Length(2),
                Constraint::Length(22),
                Constraint::Length(12),
                Constraint::Length(9),
                Constraint::Min(8),
            ],
            show_version: true,
            show_size: false,
            show_repo: true,
            desc_budget: width.saturating_sub(52) as usize,
        }
    } else if width >= 72 {
        TableSpec {
            headers: vec![" ", "Name", "Repo", "Description"],
            constraints: vec![
                Constraint::Length(2),
                Constraint::Length(20),
                Constraint::Length(8),
                Constraint::Min(8),
            ],
            show_version: false,
            show_size: false,
            show_repo: true,
            desc_budget: width.saturating_sub(36) as usize,
        }
    } else {
        TableSpec {
            headers: vec![" ", "Name", "Description"],
            constraints: vec![
                Constraint::Length(2),
                Constraint::Length(16),
                Constraint::Min(6),
            ],
            show_version: false,
            show_size: false,
            show_repo: false,
            desc_budget: width.saturating_sub(22) as usize,
        }
    }
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
                    .border_style(focus_border(app, PaneFocus::Detail)),
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
                Span::styled(
                    truncate(&entry.text, area.width.saturating_sub(8) as usize),
                    Style::default().fg(TEXT_PRIMARY),
                ),
            ])
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT_PRIMARY).bg(SURFACE))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .title(" ACTIVITY FEED ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(SURFACE))
                    .border_style(focus_border(app, PaneFocus::Feed)),
            ),
        area,
    );
}

fn render_queue_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let width = area.width as usize;
    let suffix = if width >= 110 {
        format!(
            "  [DRY:{}] [i] install [{}] {}",
            if app.dry_run { "ON" } else { "OFF" },
            app.focus.as_str(),
            truncate(&app.status_line, 26)
        )
    } else if width >= 84 {
        format!(
            "  [DRY:{}] [i] [{}]",
            if app.dry_run { "ON" } else { "OFF" },
            app.focus.as_str()
        )
    } else {
        format!("  [i] [{}]", app.focus.as_str())
    };

    let reserved = suffix.chars().count().saturating_add(8);
    let mut used = 0usize;
    let mut visible_tags = Vec::new();

    for item in &app.queue {
        let tag = match item.action {
            QueueAction::Install => format!("[{}]", item.name),
            QueueAction::Remove => format!("[-{}]", item.name),
        };

        if used + tag.len() + 1 > width.saturating_sub(reserved) {
            break;
        }

        used += tag.len() + 1;
        visible_tags.push(tag);
    }

    let hidden = app.queue.len().saturating_sub(visible_tags.len());

    let mut spans = vec![Span::styled(
        "QUEUE ",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];

    for tag in visible_tags {
        spans.push(Span::styled(
            format!("{} ", tag),
            Style::default().fg(TEXT_PRIMARY),
        ));
    }

    if hidden > 0 {
        spans.push(Span::styled(
            format!("+{} more", hidden),
            Style::default().fg(TEXT_MUTED),
        ));
    }

    spans.push(Span::styled(
        suffix,
        Style::default().fg(if app.dry_run { WARNING } else { TEXT_MUTED }),
    ));

    let queue_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(SURFACE).fg(TEXT_PRIMARY))
        .border_style(Style::default().fg(BORDER));

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(SURFACE).fg(TEXT_PRIMARY))
            .wrap(Wrap { trim: true })
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
                Paragraph::new(lines)
                    .alignment(Alignment::Left)
                    .wrap(Wrap { trim: true })
                    .block(
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
                    .wrap(Wrap { trim: true })
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
                Paragraph::new(lines)
                    .alignment(Alignment::Left)
                    .wrap(Wrap { trim: true })
                    .block(
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

fn focus_border(app: &App, pane: PaneFocus) -> Style {
    if app.focus == pane && app.mode == Mode::Normal {
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
    let popup_width = ((area.width as u32 * width_percent as u32) / 100)
        .max(24)
        .min(area.width as u32) as u16;
    let popup_height = ((area.height as u32 * height_percent as u32) / 100)
        .max(8)
        .min(area.height as u32) as u16;

    Rect {
        x: area.x + area.width.saturating_sub(popup_width) / 2,
        y: area.y + area.height.saturating_sub(popup_height) / 2,
        width: popup_width,
        height: popup_height,
    }
}
