use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{App, DoneFocus};

use super::{border_style, focus_title};

pub(super) fn render(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(6)])
        .split(area);

    render_summary(frame, app, sections[0]);
    render_unresolved(frame, app, sections[1]);
}

fn render_summary(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.done_focus == DoneFocus::Summary;

    let lines = if let Some(summary) = &app.summary {
        vec![
            format!("Installed: {}", summary.installed),
            format!("Failed: {}", summary.failed),
            format!("Skipped: {}", summary.skipped),
            format!("Aborted: {}", summary.aborted),
            format!("Elapsed: {} sec", summary.elapsed.as_secs()),
            format!("Log: {}", summary.log_path.display()),
        ]
    } else {
        vec!["No summary available".to_string()]
    };

    let summary = Paragraph::new(lines.join("\n"))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(focus_title("Final Summary", focused))
                .border_style(border_style(focused)),
        );
    frame.render_widget(summary, area);
}

fn render_unresolved(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let focused = app.done_focus == DoneFocus::Unresolved;

    let rows = if let Some(summary) = &app.summary {
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

    let unresolved = List::new(rows).block(
        Block::default()
            .borders(Borders::ALL)
            .title(focus_title("Unresolved (r back to queue)", focused))
            .border_style(border_style(focused)),
    );
    frame.render_widget(unresolved, area);
}
