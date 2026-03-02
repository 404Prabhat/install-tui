mod actions;
mod events;
mod focus;
mod input;
mod search;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

use crossterm::event::KeyEvent;

use crate::art::{ArtFrame, MatrixArt};
use crate::indexer::{IndexEvent, spawn_indexer};
use crate::installer::InstallEvent;
use crate::model::{
    InstallProgress, InstallSummary, LogLevel, Manager, ManagerAvailability, PRIORITY_PRESETS,
    PackageRecord, priority_to_text,
};

use self::focus::{
    browse_focus_label, done_focus_label, install_focus_label, next_done_focus,
    next_install_focus, next_queue_focus, prev_done_focus, prev_install_focus, prev_queue_focus,
    queue_focus_label,
};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallFocus {
    Progress,
    Current,
    Logs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoneFocus {
    Summary,
    Unresolved,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text: String,
}

struct SearchJob {
    query: String,
    cursor: usize,
    scored: Vec<(i64, usize)>,
}

pub struct App {
    pub screen: Screen,
    pub queue_focus: QueueFocus,
    pub browse_focus: BrowseFocus,
    pub install_focus: InstallFocus,
    pub done_focus: DoneFocus,
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
    pub logs: VecDeque<LogEntry>,
    pub status_line: String,
    pub warning_line: Option<String>,
    pub should_quit: bool,
    pub show_help: bool,
    pub search_loading: bool,

    queue_set: HashSet<String>,
    official_set: HashSet<String>,
    index_rx: Receiver<IndexEvent>,
    install_rx: Option<Receiver<InstallEvent>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    matrix_art: MatrixArt,
    search_job: Option<SearchJob>,
    spinner_tick: usize,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn_indexer(tx);

        Self {
            screen: Screen::Queue,
            queue_focus: QueueFocus::Input,
            browse_focus: BrowseFocus::Search,
            install_focus: InstallFocus::Progress,
            done_focus: DoneFocus::Summary,
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
            warning_line: None,
            should_quit: false,
            show_help: false,
            search_loading: false,
            queue_set: HashSet::new(),
            official_set: HashSet::new(),
            index_rx: rx,
            install_rx: None,
            cancel_flag: None,
            matrix_art: MatrixArt::new(),
            search_job: None,
            spinner_tick: 0,
        }
    }

    pub fn tick(&mut self) {
        self.consume_index_events();
        self.consume_install_events();
        self.advance_search_job();
        self.spinner_tick = (self.spinner_tick + 1) % 4;
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

    pub fn view_label(&self) -> &'static str {
        match self.screen {
            Screen::Queue => "Queue",
            Screen::Browse => "Browse",
            Screen::Installing => "Installing",
            Screen::Done => "Summary",
        }
    }

    pub fn focus_label(&self) -> String {
        match self.screen {
            Screen::Queue => format!("Queue/{}", queue_focus_label(self.queue_focus)),
            Screen::Browse => format!("Browse/{}", browse_focus_label(self.browse_focus)),
            Screen::Installing => {
                format!("Installing/{}", install_focus_label(self.install_focus))
            }
            Screen::Done => format!("Summary/{}", done_focus_label(self.done_focus)),
        }
    }

    pub fn keybind_hint(&self) -> &'static str {
        match self.screen {
            Screen::Queue => "Tab Shift+Tab focus | a add | b browse | i install",
            Screen::Browse => "Tab Shift+Tab focus | Enter add | Esc search | / search",
            Screen::Installing => "Tab Shift+Tab focus | c abort | q abort | ? help",
            Screen::Done => "Tab Shift+Tab focus | r back | Enter quit | ? help",
        }
    }

    pub fn spinner_char(&self) -> char {
        const FRAMES: [char; 4] = ['|', '/', '-', '\\'];
        FRAMES[self.spinner_tick % FRAMES.len()]
    }

    pub fn queue_count(&self) -> usize {
        self.queue.len()
    }

    pub fn queue_contains(&self, pkg: &str) -> bool {
        self.queue_set.contains(pkg)
    }

    pub fn queue_manager_for(&self, pkg: &str) -> Manager {
        if self.official_set.contains(pkg) {
            Manager::Pacman
        } else {
            self.availability.aur_helper().unwrap_or(Manager::Pacman)
        }
    }

    pub fn search_result_count(&self) -> usize {
        self.matches.len()
    }

    pub fn active_warning(&self) -> Option<&str> {
        self.warning_line.as_deref()
    }

    fn clear_warning(&mut self) {
        self.warning_line = None;
    }
}
