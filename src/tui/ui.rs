use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use super::app::{AppState, ConnectionState, ForwardStatus, InputMode, SortOrder, SortState, ViewMode};

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

fn sort_indicator(sort: &SortState, col: usize) -> &'static str {
    match sort.active {
        Some((c, SortOrder::Ascending)) if c == col => " ▲",
        Some((c, SortOrder::Descending)) if c == col => " ▼",
        _ => "",
    }
}

fn render_remote_table(f: &mut Frame, state: &AppState, area: Rect) {
    let col_names = ["Status", "Port", "Local Address", "Process", "PID"];
    let header = Row::new(
        col_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Cell::from(format!("{}{}", name, sort_indicator(&state.sort, i)))
            })
            .collect::<Vec<_>>(),
    )
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let sorted = state.sorted_ports();
    let rows: Vec<Row> = sorted
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
    let col_names = ["Bind Address", "Port", "Process", "PID"];
    let header = Row::new(
        col_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Cell::from(format!("{}{}", name, sort_indicator(&state.sort, i)))
            })
            .collect::<Vec<_>>(),
    )
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let sorted = state.sorted_local_ports();
    let rows: Vec<Row> = sorted
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
                Span::styled("[o]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" open  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[p]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" change port  "),
                Span::styled("[s]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" sort  "),
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" local  "),
                Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit"),
            ]),
            ViewMode::Local => Line::from(vec![
                Span::styled("[o]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" open  "),
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" remote  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[s]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" sort  "),
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
        InputMode::SortSelect => {
            let col_names: Vec<&str> = match state.view_mode {
                ViewMode::Remote => vec!["Status", "Port", "Local Addr", "Process", "PID"],
                ViewMode::Local => vec!["Bind Addr", "Port", "Process", "PID"],
            };
            let mut spans = vec![Span::raw(" Sort by: ")];
            for (i, name) in col_names.iter().enumerate() {
                let style = if i == state.sort.column {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                spans.push(Span::styled(*name, style));
                if i + 1 < col_names.len() {
                    spans.push(Span::raw("  "));
                }
            }
            spans.push(Span::raw("  "));
            spans.push(Span::styled("[←/→]", Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" select  "));
            spans.push(Span::styled("[enter]", Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" sort  "));
            spans.push(Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" reset  "));
            spans.push(Span::styled("[esc]", Style::default().add_modifier(Modifier::BOLD)));
            spans.push(Span::raw(" cancel"));
            Line::from(spans)
        }
    };

    f.render_widget(
        Paragraph::new(help_text).block(Block::default().borders(Borders::TOP)),
        area,
    );
}
