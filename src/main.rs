mod app;
mod backend;
mod config;
mod db;
mod detail;
mod installer;
mod model;
mod syncer;
mod ui;

use std::collections::HashMap;
use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc::unbounded_channel;

use crate::app::App;
use crate::backend::detect_backend_states;
use crate::config::{load_sets, parse_backend_priority};
use crate::installer::spawn_install;
use crate::model::{AppEvent, QueueAction, QueueItem};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.no_tui {
        return run_headless(cli).await;
    }

    let mut terminal = setup_terminal()?;
    let mut app = App::new()?;

    loop {
        app.tick();
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(std::time::Duration::from_millis(30))?
            && let Event::Key(key) = event::read()?
        {
            app.on_key(key);
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}

#[derive(Debug, Default)]
struct Cli {
    no_tui: bool,
    install: Vec<String>,
    remove: Vec<String>,
    load_set: Option<String>,
}

impl Cli {
    fn parse() -> Self {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let mut cli = Cli::default();
        let mut i = 0usize;

        while i < args.len() {
            match args[i].as_str() {
                "--no-tui" => {
                    cli.no_tui = true;
                    i += 1;
                }
                "--install" => {
                    i += 1;
                    while i < args.len() && !args[i].starts_with('-') {
                        cli.install.push(args[i].clone());
                        i += 1;
                    }
                }
                "--remove" => {
                    i += 1;
                    while i < args.len() && !args[i].starts_with('-') {
                        cli.remove.push(args[i].clone());
                        i += 1;
                    }
                }
                "--load" => {
                    if let Some(name) = args.get(i + 1) {
                        cli.load_set = Some(name.clone());
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }

        cli
    }
}

async fn run_headless(cli: Cli) -> Result<()> {
    let mut queue = Vec::new();

    for pkg in cli.install {
        queue.push(QueueItem {
            name: pkg,
            action: QueueAction::Install,
        });
    }

    for pkg in cli.remove {
        queue.push(QueueItem {
            name: pkg,
            action: QueueAction::Remove,
        });
    }

    if let Some(set_name) = cli.load_set {
        let sets = load_sets().unwrap_or_default();
        if let Some(pkgs) = sets.sets.get(&set_name) {
            for pkg in pkgs {
                queue.push(QueueItem {
                    name: pkg.clone(),
                    action: QueueAction::Install,
                });
            }
        } else {
            println!("set not found: {set_name}");
        }
    }

    if queue.is_empty() {
        println!("nothing to do: pass --install/--remove/--load with --no-tui");
        return Ok(());
    }

    let config = crate::config::load_or_create_config()?;
    let backend_priority = parse_backend_priority(&config.backend.priority);
    let backend_available = detect_backend_states()
        .into_iter()
        .map(|state| (state.id, state.available))
        .collect::<HashMap<_, _>>();

    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let _handle = spawn_install(queue, backend_priority, backend_available, false, tx);

    while let Some(event) = rx.recv().await {
        match event {
            AppEvent::InstallLine(line) => {
                println!("{} {}", line.ts, line.text);
            }
            AppEvent::InstallFinished {
                installed,
                skipped,
                failed,
                aborted,
            } => {
                println!(
                    "finished installed={} skipped={} failed={} aborted={}",
                    installed, skipped, failed, aborted
                );
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
