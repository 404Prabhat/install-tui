mod browse;
mod done;
mod install;
mod queue;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Screen};

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    if frame.area().width < 80 || frame.area().height < 24 {
        render_resize_warning(frame);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);

    let show_art = chunks[1].width >= 110 && chunks[1].height >= 18;
    if show_art {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(chunks[1]);

        render_main_panel(frame, app, body[0]);
        render_art_panel(frame, app, body[1]);
    } else {
        render_main_panel(frame, app, chunks[1]);
    }

    render_footer(frame, app, chunks[2]);

    if app.show_help {
        render_help_overlay(frame, app);
    }
}

pub(super) fn border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(super) fn selected_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::LightCyan)
        .add_modifier(Modifier::BOLD)
}

pub(super) fn focus_title(name: &str, focused: bool) -> String {
    if focused {
        format!("> {name}")
    } else {
        name.to_string()
    }
}

fn render_main_panel(frame: &mut Frame<'_>, app: &App, area: Rect) {
    match app.screen {
        Screen::Queue => queue::render(frame, app, area),
        Screen::Browse => browse::render(frame, app, area),
        Screen::Installing => install::render(frame, app, area),
        Screen::Done => done::render(frame, app, area),
    }
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " install-tui ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("View: {}", app.view_label()),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ];

    if app.dry_run {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "[DRY RUN]",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title("Header"));
    frame.render_widget(header, area);
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let warning = app
        .active_warning()
        .map(|value| format!("  warning: {value}"))
        .unwrap_or_default();

    let footer_text = Text::from(vec![
        Line::from(vec![
            Span::styled(
                format!("Mode: {}", app.focus_label()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  |  "),
            Span::styled(
                if app.dry_run { "Dry-run: ON" } else { "Dry-run: OFF" },
                Style::default().fg(if app.dry_run { Color::Yellow } else { Color::Green }),
            ),
            Span::raw(warning),
        ]),
        Line::from(format!("{}  |  status: {}", app.keybind_hint(), app.status_line)),
    ]);

    let footer = Paragraph::new(footer_text)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Status"));
    frame.render_widget(footer, area);
}

fn render_art_panel(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let inner = area.inner(Margin {
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
                .title(format!("{} (10s)", art.title))
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(widget, area);
}

fn render_help_overlay(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(frame.area(), 76, 70);
    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(Span::styled(
            "Help (? toggles)",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Global: 1 queue | 2 browse | i install | q quit/abort | Tab + Shift+Tab cycle"),
        Line::from("Queue: type package names -> Enter/a add | t dry-run | d drop | x clear | b browse"),
        Line::from("Browse: type search query | Enter/Space queue package | d remove from queue | Esc focus search"),
        Line::from("Install: c or q request safe abort"),
        Line::from("Done: r back to queue | Enter quit"),
    ];

    lines.push(Line::from(""));
    lines.push(Line::from(format!("Current view: {}", app.view_label())));
    lines.push(Line::from(format!("Current mode: {}", app.focus_label())));

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Keybind Help")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(widget, area);
}

fn render_resize_warning(frame: &mut Frame<'_>) {
    let msg = Paragraph::new("Terminal too small. Minimum supported size is 80x24.")
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("install-tui")
                .border_style(Style::default().fg(Color::Red)),
        );
    frame.render_widget(msg, frame.area());
}

fn centered_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
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
