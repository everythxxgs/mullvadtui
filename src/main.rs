mod api;
mod app;
mod config;
mod server;
mod ui;
mod wireguard;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, InputMode, View};

#[tokio::main]
async fn main() -> Result<()> {
    // Check if running as root
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("This program must be run as root (use sudo)");
        std::process::exit(1);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    // Initialize app
    app.init().await?;

    // If no servers loaded, prompt to refresh
    if app.servers.is_empty() {
        app.message = Some("No servers cached. Press 'r' to refresh or 'i' to setup.".to_string());
    }

    loop {
        // Draw UI
        terminal.draw(|f| ui::draw(f, app))?;

        // Handle events with timeout for periodic status updates
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events, not release
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.next();
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.previous();
                        }
                        KeyCode::Enter => {
                            app.select();
                        }
                        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                            app.back();
                        }
                        KeyCode::Char('l') | KeyCode::Right => {
                            app.select();
                        }
                        KeyCode::Char('r') => {
                            app.refresh_servers().await?;
                        }
                        KeyCode::Char('d') => {
                            app.disconnect();
                        }
                        KeyCode::Char('i') => {
                            app.enter_setup();
                        }
                        KeyCode::Char('s') => {
                            app.update_status();
                        }
                        _ => {}
                    },
                    InputMode::AccountInput => match key.code {
                        KeyCode::Enter => {
                            if let Err(e) = app.submit_setup().await {
                                app.error = Some(format!("Setup failed: {}", e));
                            }
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.view = View::Countries;
                            app.input_buffer.clear();
                        }
                        _ => {}
                    },
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
