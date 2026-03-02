use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;

use crate::installer::{InstallRequest, spawn_installer};
use crate::model::{InstallProgress, PRIORITY_PRESETS};

use super::{App, InstallFocus, Screen};

impl App {
    pub(super) fn start_install(&mut self) {
        if self.install_rx.is_some() {
            return;
        }

        if self.queue.is_empty() {
            self.warning_line = Some("Queue is empty. Add packages before installing.".to_string());
            self.status_line = "Queue is empty".to_string();
            return;
        }

        if !self.index_ready {
            self.warning_line = Some("Package index still loading".to_string());
            self.status_line = "Package index still loading".to_string();
            return;
        }

        if !self.dry_run && !sudo_session_cached() {
            self.warning_line = Some("Run sudo -v before installing".to_string());
            self.status_line = "Sudo session not active. Run sudo -v".to_string();
            self.push_log("warn: sudo auth required before install; run sudo -v".to_string());
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

        self.clear_warning();
        self.logs.clear();
        self.summary = None;
        self.progress = InstallProgress {
            total: self.queue.len(),
            stage: "Starting installer".to_string(),
            ..InstallProgress::default()
        };

        self.cancel_flag = Some(spawn_installer(request, tx));
        self.install_rx = Some(rx);
        self.install_focus = InstallFocus::Progress;
        self.screen = Screen::Installing;
        self.status_line = "Installer running".to_string();
    }

    pub(super) fn request_abort(&mut self) {
        if let Some(flag) = &self.cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn add_from_manual_input(&mut self) {
        let parsed = super::search::parse_packages(&self.manual_input);
        if parsed.is_empty() {
            self.warning_line = Some("No valid package names found in input".to_string());
            self.status_line = "Manual input is empty".to_string();
            return;
        }

        let mut added = 0usize;
        for pkg in parsed {
            if self.queue_set.insert(pkg.clone()) {
                self.queue.push(pkg);
                added += 1;
            }
        }

        self.clear_warning();
        self.queue.sort();
        self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));
        self.status_line = format!("Added {added} package(s) to queue");
        self.manual_input.clear();
    }

    pub(super) fn add_highlighted_result_to_queue(&mut self) {
        if self.matches.is_empty() {
            return;
        }

        if let Some(index) = self.matches.get(self.result_cursor).copied()
            && let Some(record) = self.packages.get(index)
        {
            if self.queue_set.insert(record.name.clone()) {
                self.queue.push(record.name.clone());
                self.queue.sort();
                self.status_line = format!("Queued {}", record.name);
                self.clear_warning();
            } else {
                self.status_line = format!("{} is already in queue", record.name);
            }
        }
    }

    pub(super) fn remove_selected_queue_item(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        let idx = self.queue_cursor.min(self.queue.len() - 1);
        let removed = self.queue.remove(idx);
        self.queue_set.remove(&removed);
        self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));
        self.status_line = format!("Removed {removed}");
    }

    pub(super) fn remove_from_queue(&mut self, pkg: &str) {
        if self.queue_set.remove(pkg) {
            self.queue.retain(|item| item != pkg);
            self.queue_cursor = self.queue_cursor.min(self.queue.len().saturating_sub(1));
            self.status_line = format!("Removed {pkg} from queue");
        }
    }
}

fn sudo_session_cached() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
