use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use super::app::{AppState, ConnectionState, ForwardStatus, InputMode, ViewMode};

pub fn render(f: &mut Frame, state: &AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),  // status bar
        Constraint::Min(5),    // port table
        Constraint::Length(2), // help bar
    ])
    .split(f.area());

    render_status_bar(f, state, chunks[0]);
    render_port_table(f, state, chunks[1]);
    render_help_bar(f, state, chunks[2]);
}

fn render_status_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let (conn_label, conn_color) = match &state.connection {
        ConnectionState::Connected => ("connected", Color::Green),
        ConnectionState::Disconnected => ("disconnected", Color::Red),
        ConnectionState::Reconnecting => ("reconnecting...", Color::Yellow),
    };

    let view_label = match &state.view_mode {
        ViewMode::Remote => "[Remote]",
        ViewMode::Local => "[Local]",
    };

    let mut spans = vec![
        Span::styled(" portfwd", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" — "),
        Span::raw(&state.host_alias),
        Span::raw(" ("),
        Span::styled(conn_label, Style::default().fg(conn_color)),
        Span::raw(") "),
        Span::styled(view_label, Style::default().add_modifier(Modifier::BOLD)),
    ];

    if let Some(ref msg) = state.status_message {
        spans.push(Span::styled(
            format!("  {}", msg),
            Style::default().fg(Color::Yellow),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_port_table(f: &mut Frame, state: &AppState, area: Rect) {
    match state.view_mode {
        ViewMode::Remote => render_remote_table(f, state, area),
        ViewMode::Local => render_local_table(f, state, area),
    }
}

fn render_remote_table(f: &mut Frame, state: &AppState, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Status"),
        Cell::from("Port"),
        Cell::from("Local Address"),
        Cell::from("Process"),
        Cell::from("PID"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .ports
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let (status_icon, local_addr) = match &entry.forward_status {
                ForwardStatus::Active { local_port } => (
                    Span::styled("●", Style::default().fg(Color::Green)),
                    format!("localhost:{}", local_port),
                ),
                ForwardStatus::Idle => (
                    Span::styled("○", Style::default().fg(Color::DarkGray)),
                    String::new(),
                ),
                ForwardStatus::Error(msg) => (
                    Span::styled("✗", Style::default().fg(Color::Red)),
                    msg.clone(),
                ),
            };

            let process = entry
                .discovered
                .process_name
                .as_deref()
                .unwrap_or("-");
            let pid = entry
                .discovered
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());

            let style = if i == state.selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(status_icon),
                Cell::from(entry.discovered.port.to_string()),
                Cell::from(local_addr),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Remote Ports "));

    f.render_widget(table, area);
}

fn render_local_table(f: &mut Frame, state: &AppState, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Bind Address"),
        Cell::from("Port"),
        Cell::from("Process"),
        Cell::from("PID"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .local_ports
        .iter()
        .enumerate()
        .map(|(i, port)| {
            let process = port.process_name.as_deref().unwrap_or("-");
            let pid = port
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());

            let style = if i == state.local_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(port.bind_address.clone()),
                Cell::from(port.port.to_string()),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Local Ports "));

    f.render_widget(table, area);
}

fn render_help_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let help_text = match &state.input_mode {
        InputMode::Normal => match state.view_mode {
            ViewMode::Remote => Line::from(vec![
                Span::styled("[enter]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" toggle  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[p]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" change port  "),
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" local  "),
                Span::styled("[c]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" reconnect  "),
                Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit"),
            ]),
            ViewMode::Local => Line::from(vec![
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" remote  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit"),
            ]),
        },
        InputMode::PortInput(input) => Line::from(vec![
            Span::raw(" Local port: "),
            Span::styled(input, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw("  [enter] confirm  [esc] cancel"),
        ]),
    };

    f.render_widget(
        Paragraph::new(help_text).block(Block::default().borders(Borders::TOP)),
        area,
    );
}
