use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::PRIORITY_PRESETS;

use super::{
    App, BrowseFocus, DoneFocus, QueueFocus, Screen, next_done_focus, next_install_focus,
    next_queue_focus, prev_done_focus, prev_install_focus, prev_queue_focus,
};

impl App {
    pub(super) fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('?') {
            self.show_help = !self.show_help;
            return true;
        }

        let is_text_entry = matches!(
            (self.screen, self.queue_focus, self.browse_focus),
            (Screen::Queue, QueueFocus::Input, _) | (Screen::Browse, _, BrowseFocus::Search)
        );
        if is_text_entry {
            return false;
        }

        match key.code {
            KeyCode::Char('q') => {
                if self.screen == Screen::Installing {
                    self.request_abort();
                    self.warning_line = Some("Abort requested. Waiting for safe checkpoint...".to_string());
                    self.status_line = "Abort requested".to_string();
                } else {
                    self.should_quit = true;
                }
                true
            }
            KeyCode::Char('1') => {
                if self.screen != Screen::Installing {
                    self.screen = Screen::Queue;
                }
                true
            }
            KeyCode::Char('2') => {
                if self.screen != Screen::Installing {
                    self.screen = Screen::Browse;
                }
                true
            }
            KeyCode::Char('i') => {
                if self.screen != Screen::Installing {
                    self.start_install();
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn on_queue_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.queue_focus = next_queue_focus(self.queue_focus);
            }
            KeyCode::BackTab => {
                self.queue_focus = prev_queue_focus(self.queue_focus);
            }
            KeyCode::Char('b') => {
                self.screen = Screen::Browse;
                self.browse_focus = BrowseFocus::Search;
                self.status_line = "Browse packages with fuzzy search".to_string();
            }
            KeyCode::Char('t') => {
                self.dry_run = !self.dry_run;
                self.status_line = format!(
                    "Dry-run {}",
                    if self.dry_run { "enabled" } else { "disabled" }
                );
            }
            KeyCode::Char('a') if key.modifiers.is_empty() => {
                self.add_from_manual_input();
            }
            _ => match self.queue_focus {
                QueueFocus::Input => self.handle_text_input_key(key),
                QueueFocus::Priority => self.handle_priority_key(key),
                QueueFocus::Queue => self.handle_queue_list_key(key),
                QueueFocus::Install => {
                    if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                        self.start_install();
                    }
                }
            },
        }
    }

    pub(super) fn on_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.browse_focus = if self.browse_focus == BrowseFocus::Search {
                    BrowseFocus::Results
                } else {
                    BrowseFocus::Search
                };
            }
            KeyCode::Esc | KeyCode::Char('/') => {
                self.browse_focus = BrowseFocus::Search;
            }
            _ => match self.browse_focus {
                BrowseFocus::Search => self.handle_search_key(key),
                BrowseFocus::Results => self.handle_result_list_key(key),
            },
        }
    }

    pub(super) fn on_installing_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.install_focus = next_install_focus(self.install_focus);
            }
            KeyCode::BackTab => {
                self.install_focus = prev_install_focus(self.install_focus);
            }
            KeyCode::Char('c') => {
                self.request_abort();
                self.warning_line = Some("Abort requested. Waiting for safe checkpoint...".to_string());
                self.status_line = "Abort requested".to_string();
            }
            _ => {}
        }
    }

    pub(super) fn on_done_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.done_focus = next_done_focus(self.done_focus);
            }
            KeyCode::BackTab => {
                self.done_focus = prev_done_focus(self.done_focus);
            }
            KeyCode::Char('r') => {
                self.screen = Screen::Queue;
                self.summary = None;
                self.done_focus = DoneFocus::Summary;
            }
            KeyCode::Enter => self.should_quit = true,
            _ => {}
        }
    }

    fn handle_text_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Backspace => {
                self.manual_input.pop();
            }
            KeyCode::Enter => self.add_from_manual_input(),
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.manual_input.push(ch);
            }
            _ => {}
        }
    }

    fn handle_priority_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                if self.priority_idx == 0 {
                    self.priority_idx = PRIORITY_PRESETS.len() - 1;
                } else {
                    self.priority_idx -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                self.priority_idx = (self.priority_idx + 1) % PRIORITY_PRESETS.len();
            }
            _ => {}
        }
    }

    fn handle_queue_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.queue_cursor = self.queue_cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.queue.is_empty() {
                    self.queue_cursor = (self.queue_cursor + 1).min(self.queue.len() - 1);
                }
            }
            KeyCode::Char('d') => {
                self.remove_selected_queue_item();
            }
            KeyCode::Char('x') => {
                self.queue.clear();
                self.queue_set.clear();
                self.queue_cursor = 0;
                self.status_line = "Queue cleared".to_string();
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Backspace => {
                self.search_query.pop();
                self.refresh_matches();
            }
            KeyCode::Enter | KeyCode::Down => {
                self.browse_focus = BrowseFocus::Results;
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.search_query.push(ch);
                self.refresh_matches();
            }
            _ => {}
        }
    }

    fn handle_result_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.result_cursor = self.result_cursor.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.matches.is_empty() {
                    self.result_cursor = (self.result_cursor + 1).min(self.matches.len() - 1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('a') => {
                self.add_highlighted_result_to_queue();
            }
            KeyCode::Char('d') => {
                if let Some(index) = self.matches.get(self.result_cursor).copied()
                    && let Some(record) = self.packages.get(index)
                {
                    let name = record.name.clone();
                    self.remove_from_queue(&name);
                }
            }
            _ => {}
        }
    }
}
