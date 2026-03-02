use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{App, QueueFocus};
use crate::model::Manager;

use super::{border_style, focus_title, selected_style};

pub(super) fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(4),
        ])
        .split(area);

    render_input(frame, app, sections[0]);
    render_priority(frame, app, sections[1]);
    render_queue(frame, app, sections[2]);
    render_actions(frame, app, sections[3]);
}

fn render_input(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.queue_focus == QueueFocus::Input;
    let input = Paragraph::new(app.manual_input.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title("Manual Input (comma/space separated)", focused))
            .border_style(border_style(focused)),
    );
    frame.render_widget(input, area);
}

fn render_priority(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.queue_focus == QueueFocus::Priority;
    let dry = if app.dry_run {
        Span::styled(
            " [DRY RUN]",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" [LIVE]", Style::default().fg(Color::Green))
    };

    let line = Line::from(vec![
        Span::raw(format!("Priority: {}", app.priority_text())),
        dry,
    ]);
    let priority = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title("Priority (left/right cycle)", focused))
            .border_style(border_style(focused)),
    );
    frame.render_widget(priority, area);
}

fn render_queue(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.queue_focus == QueueFocus::Queue;
    let rows = if app.queue.is_empty() {
        vec![ListItem::new(Span::styled(
            "(queue empty)",
            Style::default().fg(Color::Yellow),
        ))]
    } else {
        app.queue
            .iter()
            .enumerate()
            .map(|(idx, pkg)| {
                let manager = app.queue_manager_for(pkg);
                let badge_style = manager_badge_style(manager);
                let mut item = ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}]", manager.badge()), badge_style),
                    Span::raw(" "),
                    Span::raw(pkg.clone()),
                ]));
                if idx == app.queue_cursor && focused {
                    item = item.style(selected_style());
                }
                item
            })
            .collect::<Vec<_>>()
    };

    let queue = List::new(rows).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title(
                &format!("Queue [{}] (d remove, x clear)", app.queue_count()),
                focused,
            ))
            .border_style(border_style(focused)),
    );
    frame.render_widget(queue, area);
}

fn render_actions(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.queue_focus == QueueFocus::Install;

    let mut text = vec![Line::from(
        "a: add input  |  b: browse packages  |  Enter/i: start install",
    )];
    if app.queue.is_empty() {
        text.push(Line::from(Span::styled(
            "Queue is empty. Press i will show a warning.",
            Style::default().fg(Color::Yellow),
        )));
    }

    let actions = Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(focus_title("Actions", focused))
                .border_style(border_style(focused)),
        );
    frame.render_widget(actions, area);
}

fn manager_badge_style(manager: Manager) -> Style {
    match manager {
        Manager::Pacman => Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
        Manager::Yay => Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
        Manager::Paru => Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
    }
}
