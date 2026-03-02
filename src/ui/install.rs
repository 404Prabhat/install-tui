use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{App, InstallFocus};
use crate::model::LogLevel;

use super::{border_style, focus_title};

pub(super) fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
        ])
        .split(area);

    render_progress(frame, app, sections[0]);
    render_current(frame, app, sections[1]);
    render_stats(frame, app, sections[2]);
    render_logs(frame, app, sections[3]);
}

fn render_progress(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.install_focus == InstallFocus::Progress;
    let ratio = if app.progress.total == 0 {
        0.0
    } else {
        app.progress.done as f64 / app.progress.total as f64
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(focus_title("Overall Progress", focused))
                .border_style(border_style(focused)),
        )
        .ratio(ratio)
        .label(format!("{} / {}", app.progress.done, app.progress.total))
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(gauge, area);
}

fn render_current(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.install_focus == InstallFocus::Current;
    let package = app
        .progress
        .current_package
        .clone()
        .unwrap_or_else(|| "(waiting)".to_string());

    let current = Paragraph::new(Line::from(vec![
        Span::styled(
            package,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::raw(app.progress.stage.clone()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title("Current Package", focused))
            .border_style(border_style(focused)),
    );
    frame.render_widget(current, area);
}

fn render_stats(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let stats = Paragraph::new(format!(
        "installed={} skipped={} failed={}",
        app.progress.installed, app.progress.skipped, app.progress.failed
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Stats")
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(stats, area);
}

fn render_logs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.install_focus == InstallFocus::Logs;

    let rows = app
        .logs
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .map(|line| {
            let style = match line.level {
                LogLevel::Info => Style::default().fg(Color::Gray),
                LogLevel::Success => Style::default().fg(Color::Green),
                LogLevel::Warn => Style::default().fg(Color::Yellow),
                LogLevel::Error => Style::default().fg(Color::Red),
            };
            ListItem::new(Line::from(Span::styled(line.text.clone(), style)))
        })
        .collect::<Vec<_>>();

    let logs = List::new(rows).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title("Live Log", focused))
            .border_style(border_style(focused)),
    );
    frame.render_widget(logs, area);
}
