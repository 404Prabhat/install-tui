use crate::indexer::IndexEvent;
use crate::installer::InstallEvent;
use crate::model::LogLevel;

use super::{App, LogEntry, Screen};

const MAX_LOG_LINES: usize = 500;

impl App {
    pub(super) fn consume_index_events(&mut self) {
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
                    self.warning_line =
                        Some("Indexer failed, browse results may be incomplete".to_string());
                    self.status_line = format!("Indexer error: {err}");
                    self.push_log(self.status_line.clone());
                }
            }
        }
    }

    pub(super) fn consume_install_events(&mut self) {
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
                InstallEvent::Progress(progress) => {
                    self.progress = progress;
                }
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

    pub(super) fn push_log(&mut self, text: String) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }

        let level = classify_log_level(&text);
        self.logs.push_back(LogEntry { level, text });
    }
}

fn classify_log_level(text: &str) -> LogLevel {
    let lower = text.to_ascii_lowercase();
    if lower.contains("fatal") || lower.contains("error") || lower.contains("failed") {
        LogLevel::Error
    } else if lower.contains("warn")
        || lower.contains("skip")
        || lower.contains("unresolved")
        || lower.contains("abort")
    {
        LogLevel::Warn
    } else if lower.contains("ok") || lower.contains("installed") || lower.contains("completed") {
        LogLevel::Success
    } else {
        LogLevel::Info
    }
}
