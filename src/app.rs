use std::collections::{HashSet, VecDeque};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::art::{ArtFrame, MatrixArt};
use crate::indexer::{IndexEvent, spawn_indexer};
use crate::installer::{InstallEvent, InstallRequest, spawn_installer};
use crate::model::{
    InstallProgress, InstallSummary, ManagerAvailability, PRIORITY_PRESETS, PackageRecord,
    priority_to_text,
};

const MAX_LOG_LINES: usize = 500;
const MAX_MATCHES: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Queue,
    Browse,
    Installing,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueFocus {
    Input,
    Priority,
    Queue,
    Install,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowseFocus {
    Search,
    Results,
}

pub struct App {
    pub screen: Screen,
    pub queue_focus: QueueFocus,
    pub browse_focus: BrowseFocus,
    pub manual_input: String,
    pub search_query: String,
    pub queue: Vec<String>,
    pub queue_cursor: usize,
    pub result_cursor: usize,
    pub matches: Vec<usize>,
    pub packages: Vec<PackageRecord>,
    pub availability: ManagerAvailability,
    pub index_ready: bool,
    pub priority_idx: usize,
    pub dry_run: bool,
    pub progress: InstallProgress,
    pub summary: Option<InstallSummary>,
    pub logs: VecDeque<String>,
    pub status_line: String,
    pub should_quit: bool,

    queue_set: HashSet<String>,
    official_set: HashSet<String>,
    index_rx: Receiver<IndexEvent>,
    install_rx: Option<Receiver<InstallEvent>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    matrix_art: MatrixArt,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        spawn_indexer(tx);

        Self {
            screen: Screen::Queue,
            queue_focus: QueueFocus::Input,
            browse_focus: BrowseFocus::Search,
            manual_input: String::new(),
            search_query: String::new(),
            queue: Vec::new(),
            queue_cursor: 0,
            result_cursor: 0,
            matches: Vec::new(),
            packages: Vec::new(),
            availability: ManagerAvailability::default(),
            index_ready: false,
            priority_idx: 0,
            dry_run: false,
            progress: InstallProgress::default(),
            summary: None,
            logs: VecDeque::new(),
            status_line: "Bootstrapping package index...".to_string(),
            should_quit: false,
            queue_set: HashSet::new(),
            official_set: HashSet::new(),
            index_rx: rx,
            install_rx: None,
            cancel_flag: None,
            matrix_art: MatrixArt::new(),
        }
    }

    pub fn tick(&mut self) {
        self.consume_index_events();
        self.consume_install_events();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.handle_global_key(key) {
            return;
        }

        match self.screen {
            Screen::Queue => self.on_queue_key(key),
            Screen::Browse => self.on_browse_key(key),
            Screen::Installing => self.on_installing_key(key),
            Screen::Done => self.on_done_key(key),
        }
    }

    pub fn priority_text(&self) -> String {
        priority_to_text(&PRIORITY_PRESETS[self.priority_idx])
    }

    pub fn art_frame(&mut self, width: u16, height: u16) -> ArtFrame {
        self.matrix_art.frame(width, height)
    }

    fn handle_global_key(&mut self, key: KeyEvent) -> bool {
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
                    self.status_line =
                        "Abort requested. Waiting for safe checkpoint...".to_string();
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

    fn on_queue_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.queue_focus = prev_queue_focus(self.queue_focus),
            KeyCode::Down => self.queue_focus = next_queue_focus(self.queue_focus),
            KeyCode::Tab => self.queue_focus = next_queue_focus(self.queue_focus),
            KeyCode::Char('b') => {
                self.screen = Screen::Browse;
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

    fn on_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.browse_focus = if self.browse_focus == BrowseFocus::Search {
                    BrowseFocus::Results
                } else {
                    BrowseFocus::Search
                };
            }
            KeyCode::Esc => {
                if !self.search_query.is_empty() {
                    self.search_query.clear();
                    self.refresh_matches();
                } else {
                    self.screen = Screen::Queue;
                }
            }
            KeyCode::Char('/') => {
                self.browse_focus = BrowseFocus::Search;
            }
            _ => match self.browse_focus {
                BrowseFocus::Search => self.handle_search_key(key),
                BrowseFocus::Results => self.handle_result_list_key(key),
            },
        }
    }

    fn on_installing_key(&mut self, key: KeyEvent) {
        if let KeyCode::Char('c') = key.code {
            self.request_abort();
        }
    }

    fn on_done_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('r') => {
                self.screen = Screen::Queue;
                self.summary = None;
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
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {}
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
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => {
                self.priority_idx = (self.priority_idx + 1) % PRIORITY_PRESETS.len();
            }
            KeyCode::Char(' ') => {
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

    fn start_install(&mut self) {
        if self.install_rx.is_some() {
            return;
        }

        if self.queue.is_empty() {
            self.status_line = "Queue is empty. Add packages first.".to_string();
            return;
        }

        if !self.index_ready {
            self.status_line = "Package index still loading. Try again in a moment.".to_string();
            return;
        }

        if !self.dry_run && !sudo_session_cached() {
            self.status_line =
                "Sudo session not active. Run: sudo -v  (then press i again)".to_string();
            self.push_log(
                "sudo auth required before install; run `sudo -v` in terminal".to_string(),
            );
            return;
        }

        let (tx, rx) = channel();
        let request = InstallRequest {
            packages: self.queue.clone(),
            priority: PRIORITY_PRESETS[self.priority_idx],
            availability: self.availability.clone(),
            official_set: self.official_set.clone(),
            dry_run: self.dry_run,
        };

        self.logs.clear();
        self.summary = None;
        self.progress = InstallProgress {
            total: self.queue.len(),
            stage: "Starting installer".to_string(),
            ..InstallProgress::default()
        };

        self.cancel_flag = Some(spawn_installer(request, tx));
        self.install_rx = Some(rx);
        self.screen = Screen::Installing;
        self.status_line = "Installer running".to_string();
    }

    fn request_abort(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn add_from_manual_input(&mut self) {
        let parsed = parse_packages(&self.manual_input);
        if parsed.is_empty() {
            self.status_line = "No valid package names found in input".to_string();
            return;
        }

        let mut added = 0usize;
        for pkg in parsed {
            if self.queue_set.insert(pkg.clone()) {
                self.queue.push(pkg);
                added += 1;
            }
        }
        self.queue.sort();
        self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));

        self.status_line = format!("Added {added} package(s) to queue");
        self.manual_input.clear();
    }

    fn add_highlighted_result_to_queue(&mut self) {
        if let Some(index) = self.matches.get(self.result_cursor).copied()
            && let Some(record) = self.packages.get(index)
        {
            if self.queue_set.insert(record.name.clone()) {
                self.queue.push(record.name.clone());
                self.queue.sort();
                self.status_line = format!("Queued {}", record.name);
            } else {
                self.status_line = format!("{} is already in queue", record.name);
            }
        }
    }

    fn remove_selected_queue_item(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let idx = self.queue_cursor.min(self.queue.len() - 1);
        let removed = self.queue.remove(idx);
        self.queue_set.remove(&removed);
        self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));
        self.status_line = format!("Removed {removed}");
    }

    fn remove_from_queue(&mut self, pkg: &str) {
        if self.queue_set.remove(pkg) {
            self.queue.retain(|item| item != pkg);
            self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));
            self.status_line = format!("Removed {pkg} from queue");
        }
    }

    fn consume_index_events(&mut self) {
        let mut queued = Vec::new();
        while let Ok(event) = self.index_rx.try_recv() {
            queued.push(event);
        }

        for event in queued {
            match event {
                IndexEvent::Status(line) => {
                    self.status_line = line.clone();
                    self.push_log(format!("[index] {line}"));
                }
                IndexEvent::Ready {
                    packages,
                    official_set,
                    availability,
                } => {
                    self.packages = packages;
                    self.official_set = official_set;
                    self.availability = availability;
                    self.index_ready = true;
                    self.refresh_matches();
                    self.status_line = format!(
                        "Package index ready: {} packages ({})",
                        self.packages.len(),
                        self.availability.line()
                    );
                    self.push_log(self.status_line.clone());
                }
                IndexEvent::Error(err) => {
                    self.status_line = format!("Indexer error: {err}");
                    self.push_log(self.status_line.clone());
                }
            }
        }
    }

    fn consume_install_events(&mut self) {
        let mut queued = Vec::new();
        if let Some(rx) = &self.install_rx {
            while let Ok(event) = rx.try_recv() {
                queued.push(event);
            }
        }

        let mut finished = false;
        for event in queued {
            match event {
                InstallEvent::Log(line) => self.push_log(line),
                InstallEvent::Progress(progress) => self.progress = progress,
                InstallEvent::Finished(summary) => {
                    self.status_line = format!(
                        "Install finished: installed={} skipped={} failed={}",
                        summary.installed, summary.skipped, summary.failed
                    );
                    self.summary = Some(summary);
                    finished = true;
                }
            }
        }

        if finished {
            self.install_rx = None;
            self.cancel_flag = None;
            self.screen = Screen::Done;
        }
    }

    fn push_log(&mut self, line: String) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    fn refresh_matches(&mut self) {
        self.matches.clear();
        if self.packages.is_empty() {
            self.result_cursor = 0;
            return;
        }

        if self.search_query.trim().is_empty() {
            self.matches.extend(0..self.packages.len().min(MAX_MATCHES));
            self.result_cursor = 0;
            return;
        }

        let query = self.search_query.to_lowercase();
        let mut scored = Vec::new();
        for (idx, pkg) in self.packages.iter().enumerate() {
            if let Some(score) = fuzzy_score(&pkg.lower, &query) {
                scored.push((score, idx));
            }
        }

        scored.sort_unstable_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| self.packages[a.1].name.cmp(&self.packages[b.1].name))
        });

        self.matches = scored
            .into_iter()
            .take(MAX_MATCHES)
            .map(|(_, i)| i)
            .collect();
        self.result_cursor = 0;
    }
}

fn parse_packages(input: &str) -> Vec<String> {
    input
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.chars()
                .filter(|ch| {
                    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '+' | '@')
                })
                .collect::<String>()
        })
        .filter(|pkg| !pkg.is_empty())
        .collect()
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }

    let mut score = 0i64;
    let mut q_iter = query.chars();
    let mut current = q_iter.next()?;
    let mut consecutive = 0i64;

    for (idx, ch) in candidate.chars().enumerate() {
        if ch == current {
            consecutive += 1;
            score += 10 + consecutive * 6;
            score -= idx as i64;

            if let Some(next) = q_iter.next() {
                current = next;
            } else {
                score += 100;
                return Some(score);
            }
        } else {
            consecutive = 0;
        }
    }

    None
}

fn next_queue_focus(focus: QueueFocus) -> QueueFocus {
    match focus {
        QueueFocus::Input => QueueFocus::Priority,
        QueueFocus::Priority => QueueFocus::Queue,
        QueueFocus::Queue => QueueFocus::Install,
        QueueFocus::Install => QueueFocus::Input,
    }
}

fn prev_queue_focus(focus: QueueFocus) -> QueueFocus {
    match focus {
        QueueFocus::Input => QueueFocus::Install,
        QueueFocus::Priority => QueueFocus::Input,
        QueueFocus::Queue => QueueFocus::Priority,
        QueueFocus::Install => QueueFocus::Queue,
    }
}

fn sudo_session_cached() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)
    }

    #[test]
    fn typing_i_in_input_does_not_trigger_global_install_shortcut() {
        let mut app = App::new();
        app.screen = Screen::Queue;
        app.queue_focus = QueueFocus::Input;

        app.on_key(key('i'));
        assert_eq!(app.manual_input, "i");
        assert!(app.install_rx.is_none());
    }

    #[test]
    fn typing_q_in_search_does_not_quit() {
        let mut app = App::new();
        app.screen = Screen::Browse;
        app.browse_focus = BrowseFocus::Search;

        app.on_key(key('q'));
        assert_eq!(app.search_query, "q");
        assert!(!app.should_quit);
    }

    #[test]
    fn parse_packages_handles_commas_spaces_and_filters_invalid_chars() {
        let parsed = parse_packages(" fastfetch,btop   nvtop@1   !!bad!! ");
        assert_eq!(parsed, vec!["fastfetch", "btop", "nvtop@1", "bad"]);
    }
}
