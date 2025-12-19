use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, InputMode, View};
use crate::config;
use crate::wireguard::ConnectionStatus;

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title/status bar
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Help bar
            Constraint::Length(3), // Message bar
        ])
        .split(frame.area());

    draw_status_bar(frame, app, chunks[0]);
    draw_main_content(frame, app, chunks[1]);
    draw_help_bar(frame, app, chunks[2]);
    draw_message_bar(frame, app, chunks[3]);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = match &app.connection_status {
        ConnectionStatus::Connected(code) => {
            format!(" CONNECTED: {} ", code)
        }
        ConnectionStatus::Disconnected => " DISCONNECTED ".to_string(),
    };

    let status_color = match &app.connection_status {
        ConnectionStatus::Connected(_) => Color::Green,
        ConnectionStatus::Disconnected => Color::Red,
    };

    let title = format!(" Mullvad TUI | {} ", status_text);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            title,
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ));

    frame.render_widget(block, area);
}

fn draw_main_content(frame: &mut Frame, app: &App, area: Rect) {
    match app.view {
        View::Setup => draw_setup_view(frame, app, area),
        _ => draw_list_view(frame, app, area),
    }
}

fn draw_list_view(frame: &mut Frame, app: &App, area: Rect) {
    let (title, items): (String, Vec<ListItem>) = match app.view {
        View::Countries => {
            let title = " Select Country ".to_string();
            let items: Vec<ListItem> = app
                .countries
                .iter()
                .map(|country| {
                    // Count servers and cities
                    let cities = app.server_tree.get(country);
                    let city_count = cities.map(|c| c.len()).unwrap_or(0);
                    let server_count: usize = cities
                        .map(|c| c.values().map(|s| s.len()).sum())
                        .unwrap_or(0);

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<30}", country),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(
                            format!(" ({} cities, {} servers)", city_count, server_count),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();
            (title, items)
        }
        View::Cities => {
            let country = app.selected_country.as_deref().unwrap_or("Unknown");
            let title = format!(" {} - Select City ", country);
            let items: Vec<ListItem> = app
                .cities
                .iter()
                .map(|city| {
                    let server_count = app
                        .server_tree
                        .get(country)
                        .and_then(|c| c.get(city))
                        .map(|s| s.len())
                        .unwrap_or(0);

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:<30}", city), Style::default().fg(Color::White)),
                        Span::styled(
                            format!(" ({} servers)", server_count),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();
            (title, items)
        }
        View::Servers => {
            let city = app.selected_city.as_deref().unwrap_or("Unknown");
            let country = app.selected_country.as_deref().unwrap_or("Unknown");
            let title = format!(" {}, {} - Select Server ", city, country);
            let items: Vec<ListItem> = app
                .city_servers
                .iter()
                .map(|server| {
                    let has_config = config::config_exists(&server.code);
                    let connected = matches!(&app.connection_status,
                        ConnectionStatus::Connected(c) if c == &server.code);

                    let status_indicator = if connected {
                        Span::styled(" [CONNECTED] ", Style::default().fg(Color::Green))
                    } else if has_config {
                        Span::styled(" [OK] ", Style::default().fg(Color::Blue))
                    } else {
                        Span::styled(" [NO CONFIG] ", Style::default().fg(Color::Yellow))
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<20}", server.code),
                            Style::default().fg(Color::White),
                        ),
                        status_indicator,
                        Span::styled(
                            format!(" {}", server.ipv4_addr),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();
            (title, items)
        }
        View::Setup => unreachable!(),
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    state.select(Some(app.current_selection()));

    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_setup_view(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .margin(2)
        .split(area);

    // Instructions
    let instructions = Paragraph::new(vec![
        Line::from("Enter your Mullvad account number to set up WireGuard configurations."),
        Line::from(""),
        Line::from("This will:"),
        Line::from("  1. Generate or use existing private key"),
        Line::from("  2. Register with Mullvad API"),
        Line::from("  3. Create config files for all servers"),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Setup "));

    frame.render_widget(instructions, chunks[0]);

    // Input field
    let input_style = match app.input_mode {
        InputMode::AccountInput => Style::default().fg(Color::Yellow),
        InputMode::Normal => Style::default(),
    };

    let input = Paragraph::new(app.input_buffer.as_str())
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Account Number "),
        );

    frame.render_widget(input, chunks[1]);

    // Show cursor in input mode
    if app.input_mode == InputMode::AccountInput {
        frame.set_cursor_position((
            chunks[1].x + app.input_buffer.len() as u16 + 1,
            chunks[1].y + 1,
        ));
    }
}

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match (&app.view, &app.input_mode) {
        (View::Setup, InputMode::AccountInput) => {
            " Enter: Submit | Esc: Cancel "
        }
        (View::Countries, _) => {
            " ↑/↓: Navigate | Enter: Select | r: Refresh | i: Setup | d: Disconnect | q: Quit "
        }
        (View::Cities, _) | (View::Servers, _) => {
            " ↑/↓: Navigate | Enter: Select/Connect | Esc: Back | d: Disconnect | q: Quit "
        }
        _ => "",
    };

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(help, area);
}

fn draw_message_bar(frame: &mut Frame, app: &App, area: Rect) {
    let (text, color) = if let Some(ref error) = app.error {
        (error.as_str(), Color::Red)
    } else if let Some(ref message) = app.message {
        (message.as_str(), Color::Green)
    } else {
        ("", Color::White)
    };

    let message = Paragraph::new(text)
        .style(Style::default().fg(color))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(message, area);
}
