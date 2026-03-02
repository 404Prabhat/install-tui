use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::backend::detect_backend_states;
use crate::config::{
    AppConfig, SavedSets, load_or_create_config, load_sets, parse_backend_priority, save_config,
    save_sets,
};
use crate::db;
use crate::detail::spawn_detail_fetch;
use crate::installer::{InstallHandle, spawn_full_upgrade, spawn_install};
use crate::model::{
    AppEvent, BackendId, BackendState, FeedItem, FeedLevel, Mode, Overlay, PackageRecord,
    PaneFocus, PreviewData, QueueAction, QueueItem, SortMode, SyncState,
};
use crate::syncer;

const MAX_FEED_LINES: usize = 1500;
const MAX_VISIBLE: usize = 600;

pub struct App {
    pub mode: Mode,
    pub focus: PaneFocus,
    pub should_quit: bool,
    pub filter_input: String,
    pub command_input: String,
    pub status_line: String,
    pub packages: Vec<PackageRecord>,
    pub visible: Vec<usize>,
    pub selected: usize,
    pub queue: Vec<QueueItem>,
    pub feed: VecDeque<FeedItem>,
    pub detail_lines: Vec<String>,
    pub overlay: Overlay,
    pub sort_mode: SortMode,
    pub dry_run: bool,
    pub sync_state: SyncState,
    pub backend_states: Vec<BackendState>,
    pub backend_priority: Vec<BackendId>,
    pub backend_cursor: usize,
    pub removal_mode: bool,
    pub upgradable_mode: bool,
    pub installing: bool,
    pub install_paused: bool,
    pub install_stats: (usize, usize, usize, bool),

    pub config: AppConfig,
    pub sets: SavedSets,

    command_history: Vec<String>,
    command_history_cursor: usize,
    queue_lookup: HashMap<String, QueueAction>,
    detail_cache: HashMap<String, Vec<String>>,
    detail_inflight: HashSet<String>,
    orphan_filter: Option<HashSet<String>>,
    db_path: PathBuf,
    event_tx: UnboundedSender<AppEvent>,
    event_rx: UnboundedReceiver<AppEvent>,
    matcher: Matcher,
    last_aur_query: String,
    install_handle: Option<InstallHandle>,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = load_or_create_config()?;
        let sets = load_sets().unwrap_or_default();
        let backend_states = detect_backend_states();
        let backend_priority = parse_backend_priority(&config.backend.priority);

        let db_path = db::db_path();
        db::init_db(&db_path)?;
        let packages = db::load_packages(&db_path).unwrap_or_default();

        let (event_tx, event_rx) = unbounded_channel();

        let mut app = Self {
            mode: Mode::Normal,
            focus: PaneFocus::List,
            should_quit: false,
            filter_input: String::new(),
            command_input: String::new(),
            status_line: "Ready".to_string(),
            packages,
            visible: Vec::new(),
            selected: 0,
            queue: Vec::new(),
            feed: VecDeque::new(),
            detail_lines: vec!["Select a package to view details".to_string()],
            overlay: Overlay::None,
            sort_mode: SortMode::Relevance,
            dry_run: false,
            sync_state: SyncState::Idle,
            backend_states,
            backend_priority,
            backend_cursor: 0,
            removal_mode: false,
            upgradable_mode: false,
            installing: false,
            install_paused: false,
            install_stats: (0, 0, 0, false),
            config,
            sets,
            command_history: Vec::new(),
            command_history_cursor: 0,
            queue_lookup: HashMap::new(),
            detail_cache: HashMap::new(),
            detail_inflight: HashSet::new(),
            orphan_filter: None,
            db_path,
            event_tx,
            event_rx,
            matcher: Matcher::new(MatcherConfig::DEFAULT),
            last_aur_query: String::new(),
            install_handle: None,
        };

        app.push_feed(
            FeedLevel::Info,
            format!("loaded {} packages from cache", app.packages.len()),
        );
        app.refresh_visible();

        if app.config.behavior.auto_sync_on_start {
            app.spawn_background_sync();
        }

        Ok(app)
    }

    pub fn tick(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_event(event);
        }
        self.ensure_selected_detail();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if self.handle_overlay_keys(key) {
            return;
        }

        if self.installing {
            self.handle_install_keys(key);
            return;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Filter => self.handle_filter_key(key),
            Mode::Command => self.handle_command_key(key),
        }
    }

    pub fn backend_available(&self, id: BackendId) -> bool {
        self.backend_states
            .iter()
            .find(|state| state.id == id)
            .map(|state| state.available)
            .unwrap_or(false)
    }

    pub fn queue_preview_labels(&self, max: usize) -> (Vec<String>, usize) {
        let mut labels = Vec::new();
        for item in self.queue.iter().take(max) {
            let tag = match item.action {
                QueueAction::Install => format!("[{}]", item.name),
                QueueAction::Remove => format!("[-{}]", item.name),
            };
            labels.push(tag);
        }
        let remaining = self.queue.len().saturating_sub(labels.len());
        (labels, remaining)
    }

    pub fn selected_package(&self) -> Option<&PackageRecord> {
        let idx = *self.visible.get(self.selected)?;
        self.packages.get(idx)
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SyncStarted => {
                self.sync_state = SyncState::Syncing;
                self.status_line = "syncing package database...".to_string();
                self.push_feed(FeedLevel::Info, "sync started");
            }
            AppEvent::SyncFinished { packages } => {
                let count = packages.len();
                self.packages = packages;
                self.sync_state = SyncState::Ready(count);
                self.status_line = format!("sync finished: {count} packages");
                self.push_feed(FeedLevel::Done, self.status_line.clone());
                self.refresh_visible();
            }
            AppEvent::SyncError(err) => {
                self.sync_state = SyncState::Error;
                self.status_line = format!("sync failed: {err}");
                self.push_feed(FeedLevel::Error, self.status_line.clone());
            }
            AppEvent::DetailLoaded(detail) => {
                self.detail_inflight.remove(&detail.package);
                self.detail_cache
                    .insert(detail.package.clone(), detail.lines.clone());
                if let Some(selected) = self.selected_package()
                    && selected.name == detail.package
                {
                    self.detail_lines = detail.lines;
                }
            }
            AppEvent::DetailError(err) => {
                self.detail_lines = vec![err];
            }
            AppEvent::AurResults { query, packages } => {
                if query != self.filter_input {
                    return;
                }
                let mut added = 0usize;
                let known: HashSet<String> = self.packages.iter().map(|p| p.name.clone()).collect();
                for pkg in packages {
                    if !known.contains(&pkg.name) {
                        self.packages.push(pkg);
                        added += 1;
                    }
                }
                if added > 0 {
                    self.push_feed(FeedLevel::Info, format!("AUR merged {} matches", added));
                    self.refresh_visible();
                }
            }
            AppEvent::InstallLine(item) => {
                self.push_feed(item.level, format!("{}", item.text));
            }
            AppEvent::InstallFinished {
                installed,
                skipped,
                failed,
                aborted,
            } => {
                self.installing = false;
                self.install_paused = false;
                self.install_handle = None;
                self.install_stats = (installed, skipped, failed, aborted);
                self.status_line = format!(
                    "install finished: installed={} skipped={} failed={}{}",
                    installed,
                    skipped,
                    failed,
                    if aborted { " (aborted)" } else { "" }
                );
                self.push_feed(FeedLevel::Done, self.status_line.clone());
                self.spawn_background_sync();
            }
        }
    }

    fn handle_overlay_keys(&mut self, key: KeyEvent) -> bool {
        match &self.overlay {
            Overlay::None => false,
            Overlay::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.overlay = Overlay::None;
                    return true;
                }
                false
            }
            Overlay::ConfirmQuit => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('y') => self.should_quit = true,
                    KeyCode::Esc | KeyCode::Char('n') => self.overlay = Overlay::None,
                    _ => {}
                }
                true
            }
            Overlay::InstallPreview(_) => {
                match key.code {
                    KeyCode::Enter => {
                        self.overlay = Overlay::None;
                        self.start_install_now();
                    }
                    KeyCode::Esc => self.overlay = Overlay::None,
                    _ => {}
                }
                true
            }
        }
    }

    fn handle_install_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if let Some(handle) = &self.install_handle {
                    handle
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.push_feed(FeedLevel::Info, "abort requested");
            }
            KeyCode::Char('!') => {
                if let Some(handle) = &self.install_handle {
                    handle
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.push_feed(FeedLevel::Error, "force stop requested");
            }
            KeyCode::Char('p') => {
                self.install_paused = !self.install_paused;
                if let Some(handle) = &self.install_handle {
                    handle
                        .pause
                        .store(self.install_paused, std::sync::atomic::Ordering::Relaxed);
                }
                self.push_feed(
                    FeedLevel::Info,
                    if self.install_paused {
                        "queue paused"
                    } else {
                        "queue resumed"
                    },
                );
            }
            KeyCode::Tab => self.focus = self.focus.next(),
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Up | KeyCode::Down)
        {
            self.reorder_backends(key.code == KeyCode::Up);
            return;
        }

        match key.code {
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_input.clear();
                self.command_history_cursor = self.command_history.len();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Filter;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('g') => self.selected = 0,
            KeyCode::Char('G') => {
                if !self.visible.is_empty() {
                    self.selected = self.visible.len() - 1;
                }
            }
            KeyCode::Enter => self.toggle_selected_queue(),
            KeyCode::Char(' ') => self.ensure_selected_detail(),
            KeyCode::Char('i') => self.open_install_preview(),
            KeyCode::Char('u') => {
                self.upgradable_mode = true;
                self.orphan_filter = None;
                self.refresh_visible();
                self.status_line = "showing upgradable packages".to_string();
            }
            KeyCode::Char('A') => {
                if self.upgradable_mode {
                    let mut added = 0usize;
                    for idx in &self.visible {
                        if let Some(pkg) = self.packages.get(*idx)
                            && !self.queue_lookup.contains_key(&pkg.name)
                        {
                            self.queue.push(QueueItem {
                                name: pkg.name.clone(),
                                action: QueueAction::Install,
                            });
                            self.queue_lookup
                                .insert(pkg.name.clone(), QueueAction::Install);
                            added += 1;
                        }
                    }
                    self.status_line = format!("queued {added} upgradable packages");
                }
            }
            KeyCode::Char('U') => {
                self.installing = true;
                self.status_line = "running full system upgrade".to_string();
                self.push_feed(FeedLevel::Active, "starting full upgrade");
                let map = self.backend_available_map();
                spawn_full_upgrade(
                    self.backend_priority.clone(),
                    map,
                    self.dry_run,
                    self.event_tx.clone(),
                );
            }
            KeyCode::Char('r') => {
                self.removal_mode = !self.removal_mode;
                self.upgradable_mode = false;
                self.orphan_filter = None;
                self.refresh_visible();
                self.status_line = if self.removal_mode {
                    "removal mode enabled".to_string()
                } else {
                    "removal mode disabled".to_string()
                };
            }
            KeyCode::Char('s') => {
                self.sort_mode = self.sort_mode.next();
                self.refresh_visible();
                self.status_line = format!("sort: {}", self.sort_mode.as_str());
            }
            KeyCode::Char('S') => self.spawn_background_sync(),
            KeyCode::Char('d') => self.remove_highlighted_from_queue(),
            KeyCode::Char('x') => {
                self.queue.clear();
                self.queue_lookup.clear();
                self.status_line = "queue cleared".to_string();
            }
            KeyCode::Char('t') => {
                self.dry_run = !self.dry_run;
                self.status_line = if self.dry_run {
                    "dry-run enabled".to_string()
                } else {
                    "dry-run disabled".to_string()
                };
            }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('q') => {
                if self.config.behavior.confirm_on_quit && !self.queue.is_empty() {
                    self.overlay = Overlay::ConfirmQuit;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Esc => {
                self.upgradable_mode = false;
                self.removal_mode = false;
                self.orphan_filter = None;
                self.refresh_visible();
            }
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Char('1')
            | KeyCode::Char('2')
            | KeyCode::Char('3')
            | KeyCode::Char('4')
            | KeyCode::Char('5') => {
                if let KeyCode::Char(ch) = key.code {
                    self.jump_backend(ch);
                }
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter_input.clear();
                self.mode = Mode::Normal;
                self.last_aur_query.clear();
                self.refresh_visible();
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
                self.refresh_visible();
                self.maybe_query_aur();
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.filter_input.push(ch);
                self.refresh_visible();
                self.maybe_query_aur();
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.command_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.command_input.pop();
            }
            KeyCode::Up => {
                if self.command_history_cursor > 0 {
                    self.command_history_cursor -= 1;
                    if let Some(item) = self.command_history.get(self.command_history_cursor) {
                        self.command_input = item.clone();
                    }
                }
            }
            KeyCode::Down => {
                if self.command_history_cursor + 1 < self.command_history.len() {
                    self.command_history_cursor += 1;
                    if let Some(item) = self.command_history.get(self.command_history_cursor) {
                        self.command_input = item.clone();
                    }
                } else {
                    self.command_history_cursor = self.command_history.len();
                    self.command_input.clear();
                }
            }
            KeyCode::Tab => {
                self.tab_complete_command();
            }
            KeyCode::Enter => {
                let command = self.command_input.trim().to_string();
                if !command.is_empty() {
                    self.command_history.push(command.clone());
                    self.command_history_cursor = self.command_history.len();
                    self.execute_command(command);
                }
                self.command_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.command_input.push(ch);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, raw: String) {
        let line = raw.trim_start_matches(':').trim();
        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else {
            return;
        };

        match cmd {
            "search" => {
                self.filter_input = parts.collect::<Vec<_>>().join(" ");
                self.refresh_visible();
                self.maybe_query_aur();
            }
            "install" => {
                let names = parts.map(ToString::to_string).collect::<Vec<_>>();
                self.add_queue_items(names, QueueAction::Install);
                self.start_install_now();
            }
            "remove" => {
                let names = parts.map(ToString::to_string).collect::<Vec<_>>();
                self.add_queue_items(names, QueueAction::Remove);
            }
            "info" => {
                if let Some(name) = parts.next() {
                    self.select_package_by_name(name);
                }
            }
            "sync" => self.spawn_background_sync(),
            "upgrade" => {
                self.upgradable_mode = true;
                self.refresh_visible();
            }
            "log" => {
                let path = latest_log_file();
                self.status_line = format!("latest log: {}", path.display());
            }
            "clear" => {
                self.queue.clear();
                self.queue_lookup.clear();
                self.status_line = "queue cleared".to_string();
            }
            "dry" => {
                self.dry_run = !self.dry_run;
            }
            "save" => {
                if let Some(name) = parts.next() {
                    self.sets.sets.insert(
                        name.to_string(),
                        self.queue.iter().map(|item| item.name.clone()).collect(),
                    );
                    if let Err(err) = save_sets(&self.sets) {
                        self.status_line = format!("save set failed: {err}");
                    } else {
                        self.status_line = format!("saved set {name}");
                    }
                }
            }
            "load" => {
                if let Some(name) = parts.next() {
                    if let Some(items) = self.sets.sets.get(name).cloned() {
                        self.add_queue_items(items, QueueAction::Install);
                        self.status_line = format!("loaded set {name}");
                    } else {
                        self.status_line = format!("set not found: {name}");
                    }
                }
            }
            "orphans" => {
                self.load_orphans_filter();
            }
            "history" => {
                self.push_history_to_feed();
            }
            "q" | "quit" => self.should_quit = true,
            _ => {
                self.status_line = format!("unknown command: {cmd}");
            }
        }
    }

    fn tab_complete_command(&mut self) {
        let current = self.command_input.trim_start_matches(':');
        let mut parts = current.split_whitespace().collect::<Vec<_>>();
        if parts.is_empty() {
            return;
        }
        if parts.len() == 1 {
            return;
        }

        let prefix = parts.pop().unwrap_or("");
        if prefix.is_empty() {
            return;
        }

        if let Some(pkg) = self
            .packages
            .iter()
            .find(|pkg| pkg.name.starts_with(prefix))
        {
            let mut rebuilt = parts.join(" ");
            if !rebuilt.is_empty() {
                rebuilt.push(' ');
            }
            rebuilt.push_str(&pkg.name);
            self.command_input = rebuilt;
        }
    }

    fn jump_backend(&mut self, digit: char) {
        let idx = digit.to_digit(10).unwrap_or(1).saturating_sub(1) as usize;
        if idx < self.backend_priority.len() {
            self.backend_cursor = idx;
            self.status_line = format!("backend selected: {}", self.backend_priority[idx]);
        }
    }

    fn reorder_backends(&mut self, up: bool) {
        if self.backend_priority.is_empty() {
            return;
        }

        if up {
            if self.backend_cursor > 0 {
                self.backend_priority
                    .swap(self.backend_cursor, self.backend_cursor - 1);
                self.backend_cursor -= 1;
            }
        } else if self.backend_cursor + 1 < self.backend_priority.len() {
            self.backend_priority
                .swap(self.backend_cursor, self.backend_cursor + 1);
            self.backend_cursor += 1;
        }

        self.config.backend.priority = self
            .backend_priority
            .iter()
            .map(|id| id.bin().to_string())
            .collect();
        let _ = save_config(&self.config);
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        if !self.visible.is_empty() {
            self.selected = (self.selected + 1).min(self.visible.len() - 1);
        }
    }

    fn add_queue_items(&mut self, names: Vec<String>, action: QueueAction) {
        let mut added = 0usize;
        for name in names
            .into_iter()
            .flat_map(|value| parse_packages(&value))
            .collect::<Vec<_>>()
        {
            if self.queue_lookup.contains_key(&name) {
                continue;
            }
            self.queue_lookup.insert(name.clone(), action);
            self.queue.push(QueueItem { name, action });
            added += 1;
        }

        if added > 0 {
            self.status_line = format!("queued {added} package(s)");
        }
    }

    fn toggle_selected_queue(&mut self) {
        let Some(pkg_name) = self.selected_package().map(|pkg| pkg.name.clone()) else {
            return;
        };

        if self.queue_lookup.remove(&pkg_name).is_some() {
            self.queue.retain(|item| item.name != pkg_name);
            self.status_line = format!("removed {pkg_name} from queue");
            return;
        }

        let action = if self.removal_mode {
            QueueAction::Remove
        } else {
            QueueAction::Install
        };
        self.queue_lookup.insert(pkg_name.clone(), action);
        self.queue.push(QueueItem {
            name: pkg_name.clone(),
            action,
        });
        self.status_line = format!("queued {pkg_name}");
    }

    fn remove_highlighted_from_queue(&mut self) {
        let Some(pkg_name) = self.selected_package().map(|pkg| pkg.name.clone()) else {
            return;
        };

        if self.queue_lookup.remove(&pkg_name).is_some() {
            self.queue.retain(|item| item.name != pkg_name);
            self.status_line = format!("removed {pkg_name} from queue");
        }
    }

    fn open_install_preview(&mut self) {
        if self.queue.is_empty() {
            self.status_line = "queue is empty".to_string();
            return;
        }

        let preview = self.build_preview();
        self.overlay = Overlay::InstallPreview(preview);
    }

    fn build_preview(&self) -> PreviewData {
        let mut dep_count = 0usize;
        let mut total_download: i64 = 0;

        let packages = self
            .queue
            .iter()
            .filter(|item| item.action == QueueAction::Install)
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();

        if !packages.is_empty() {
            let mut args = vec![
                "-Sp".to_string(),
                "--print-format".to_string(),
                "%n %s".to_string(),
            ];
            args.extend(packages.clone());
            if let Ok(out) = Command::new("pacman").args(args).output()
                && out.status.success()
            {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut lines = 0usize;
                for line in text.lines() {
                    let parts = line.split_whitespace().collect::<Vec<_>>();
                    if parts.len() >= 2 {
                        lines += 1;
                        total_download += parts[1].parse::<i64>().unwrap_or(0);
                    }
                }
                dep_count = lines.saturating_sub(packages.len());
            }
        }

        PreviewData {
            pkg_count: self.queue.len(),
            dep_count,
            total_download: crate::model::human_bytes(total_download),
        }
    }

    fn start_install_now(&mut self) {
        if self.installing || self.queue.is_empty() {
            return;
        }

        self.installing = true;
        self.install_stats = (0, 0, 0, false);

        let map = self.backend_available_map();
        let handle = spawn_install(
            self.queue.clone(),
            self.backend_priority.clone(),
            map,
            self.dry_run,
            self.event_tx.clone(),
        );
        self.install_handle = Some(handle);
        self.push_feed(FeedLevel::Active, "install queue started");
    }

    fn backend_available_map(&self) -> HashMap<BackendId, bool> {
        let mut map = HashMap::new();
        for state in &self.backend_states {
            map.insert(state.id, state.available);
        }
        map
    }

    fn spawn_background_sync(&mut self) {
        let path = self.db_path.clone();
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            syncer::run_full_sync(path, tx).await;
        });
    }

    fn push_feed(&mut self, level: FeedLevel, text: impl Into<String>) {
        if self.feed.len() >= MAX_FEED_LINES {
            self.feed.pop_front();
        }
        self.feed.push_back(FeedItem::new(level, text));
    }

    fn refresh_visible(&mut self) {
        let filter = self.filter_input.trim().to_string();
        let mut scores = Vec::new();

        let mut char_buf = Vec::new();
        let pattern = if filter.is_empty() {
            None
        } else {
            Some(Pattern::parse(
                &filter,
                CaseMatching::default(),
                Normalization::default(),
            ))
        };

        for (idx, pkg) in self.packages.iter().enumerate() {
            if self.upgradable_mode && !pkg.upgradable {
                continue;
            }
            if self.removal_mode && !pkg.installed {
                continue;
            }
            if let Some(orphan) = &self.orphan_filter
                && !orphan.contains(&pkg.name)
            {
                continue;
            }

            if let Some(pattern) = &pattern {
                let hay = format!("{} {} {}", pkg.name, pkg.description, pkg.repo);
                if let Some(score) =
                    pattern.score(Utf32Str::new(&hay, &mut char_buf), &mut self.matcher)
                {
                    scores.push((score, idx));
                }
            } else {
                scores.push((0, idx));
            }
        }

        match self.sort_mode {
            SortMode::Relevance => {
                scores.sort_unstable_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| self.packages[a.1].name.cmp(&self.packages[b.1].name))
                });
            }
            SortMode::Name => {
                scores
                    .sort_unstable_by(|a, b| self.packages[a.1].name.cmp(&self.packages[b.1].name));
            }
            SortMode::Size => {
                scores.sort_unstable_by(|a, b| {
                    self.packages[b.1]
                        .size_bytes
                        .cmp(&self.packages[a.1].size_bytes)
                });
            }
            SortMode::Repo => {
                scores.sort_unstable_by(|a, b| {
                    self.packages[a.1]
                        .repo
                        .cmp(&self.packages[b.1].repo)
                        .then_with(|| self.packages[a.1].name.cmp(&self.packages[b.1].name))
                });
            }
            SortMode::InstalledFirst => {
                scores.sort_unstable_by(|a, b| {
                    let pa = &self.packages[a.1];
                    let pb = &self.packages[b.1];
                    match pb.installed.cmp(&pa.installed) {
                        Ordering::Equal => pa.name.cmp(&pb.name),
                        other => other,
                    }
                });
            }
        }

        self.visible = scores
            .into_iter()
            .take(MAX_VISIBLE)
            .map(|(_, idx)| idx)
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));

        if self.visible.is_empty() {
            self.detail_lines = vec!["No packages match the current filters".to_string()];
        }
    }

    fn maybe_query_aur(&mut self) {
        if self.filter_input.trim().len() < 3 || !self.visible.is_empty() {
            return;
        }

        if self.last_aur_query == self.filter_input {
            return;
        }
        self.last_aur_query = self.filter_input.clone();

        let tx = self.event_tx.clone();
        let query = self.filter_input.clone();
        tokio::spawn(async move {
            syncer::query_aur_if_needed(query, tx).await;
        });
    }

    fn ensure_selected_detail(&mut self) {
        let Some(pkg) = self.selected_package().cloned() else {
            return;
        };

        if let Some(lines) = self.detail_cache.get(&pkg.name).cloned() {
            self.detail_lines = lines;
            return;
        }

        if self.detail_inflight.contains(&pkg.name) {
            return;
        }

        self.detail_lines = vec![format!("loading details for {}...", pkg.name)];
        self.detail_inflight.insert(pkg.name.clone());
        spawn_detail_fetch(pkg.name.clone(), self.event_tx.clone());
    }

    fn select_package_by_name(&mut self, name: &str) {
        if let Some((row, _)) = self
            .visible
            .iter()
            .enumerate()
            .find(|(_, idx)| self.packages[**idx].name == name)
        {
            self.selected = row;
        } else if let Some((idx, _)) = self
            .packages
            .iter()
            .enumerate()
            .find(|(_, pkg)| pkg.name == name)
        {
            self.visible.insert(0, idx);
            self.selected = 0;
        }
        self.ensure_selected_detail();
    }

    fn load_orphans_filter(&mut self) {
        let output = Command::new("pacman").args(["-Qdtq"]).output();
        let Ok(out) = output else {
            self.status_line = "failed to query orphans".to_string();
            return;
        };

        if !out.status.success() {
            self.status_line = "no orphan data available".to_string();
            return;
        }

        let names = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        self.orphan_filter = Some(names);
        self.upgradable_mode = false;
        self.removal_mode = true;
        self.refresh_visible();
        self.status_line = "showing orphan packages".to_string();
    }

    fn push_history_to_feed(&mut self) {
        let dir = crate::config::cache_dir();
        let mut logs = match fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().starts_with("install-"))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        logs.sort();

        if logs.is_empty() {
            self.push_feed(FeedLevel::Info, "no install history logs found");
            return;
        }

        self.push_feed(FeedLevel::Info, "install history:");
        for path in logs.into_iter().rev().take(20) {
            self.push_feed(FeedLevel::Info, format!("{}", path.display()));
        }
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

fn latest_log_file() -> PathBuf {
    let dir = crate::config::cache_dir();
    let mut logs = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with("install-"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    logs.sort();
    logs.last()
        .cloned()
        .unwrap_or_else(|| dir.join("install-missing.log"))
}
