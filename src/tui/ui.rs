use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

use super::app::{AppState, ConnectionState, ForwardStatus, InputMode, SortOrder, SortState, ViewMode};

pub fn render(f: &mut Frame, state: &mut AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),  // status bar
        Constraint::Min(5),    // port table
        Constraint::Length(2), // help bar
    ])
    .split(f.area());

    render_status_bar(f, state, chunks[0]);
    render_port_table(f, state, chunks[1]);
    render_help_bar(f, state, chunks[2]);

    if state.input_mode == InputMode::Help {
        render_help_overlay(f, state);
    }
}

fn render_status_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let (conn_label, conn_color) = match &state.connection {
        ConnectionState::Connected => ("connected", Color::Green),
    };

    let view_label = match &state.view_mode {
        ViewMode::Remote => "[Remote]",
        ViewMode::Local => "[Local]",
    };

    let mut spans = vec![
        Span::styled(" ports", Style::default().add_modifier(Modifier::BOLD)),
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

    if let Some(ref ft_status) = state.file_transfer_status {
        spans.push(Span::styled(
            format!("  {}", ft_status),
            Style::default().fg(Color::Cyan),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_port_table(f: &mut Frame, state: &mut AppState, area: Rect) {
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

fn render_remote_table(f: &mut Frame, state: &mut AppState, area: Rect) {
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

    let searching = state.input_mode == InputMode::Search;
    let entries = if searching {
        state.filtered_ports()
    } else {
        state.sorted_ports()
    };
    let highlight_idx = if searching {
        state.search_selected
    } else {
        state.selected
    };
    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
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

            Row::new(vec![
                Cell::from(status_icon),
                Cell::from(entry.discovered.port.to_string()),
                Cell::from(local_addr),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
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
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .block(Block::default().borders(Borders::ALL).title(" Remote Ports "));

    let mut table_state = TableState::new()
        .with_offset(state.remote_scroll_offset)
        .with_selected(Some(highlight_idx));
    f.render_stateful_widget(table, area, &mut table_state);
    state.remote_scroll_offset = table_state.offset();
}

fn render_local_table(f: &mut Frame, state: &mut AppState, area: Rect) {
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

    let searching = state.input_mode == InputMode::Search;
    let entries = if searching {
        state.filtered_local_ports()
    } else {
        state.sorted_local_ports()
    };
    let highlight_idx = if searching {
        state.search_selected
    } else {
        state.local_selected
    };
    let rows: Vec<Row> = entries
        .iter()
        .map(|port| {
            let process = port.process_name.as_deref().unwrap_or("-");
            let pid = port
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from(port.bind_address.clone()),
                Cell::from(port.port.to_string()),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
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
    .row_highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
    .block(Block::default().borders(Borders::ALL).title(" Local Ports "));

    let mut table_state = TableState::new()
        .with_offset(state.local_scroll_offset)
        .with_selected(Some(highlight_idx));
    f.render_stateful_widget(table, area, &mut table_state);
    state.local_scroll_offset = table_state.offset();
}

fn render_help_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let help_text = match &state.input_mode {
        InputMode::Normal => match state.view_mode {
            ViewMode::Remote => {
                let enter_label = {
                    let sorted = state.sorted_ports();
                    match sorted.get(state.selected) {
                        Some(entry) if matches!(entry.forward_status, ForwardStatus::Active { .. }) => " stop  ",
                        _ => " forward  ",
                    }
                };
                Line::from(vec![
                Span::styled("[enter]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(enter_label),
                Span::styled("[o]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" open  "),
                Span::styled("[p]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" change port  "),
                Span::styled("[f]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" send file  "),
                Span::styled("[/]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" search  "),
                Span::styled("[s]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" sort  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" local  "),
                Span::styled("[h]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" help  "),
                Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit"),
            ])},
            ViewMode::Local => Line::from(vec![
                Span::styled("[o]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" open  "),
                Span::styled("[/]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" search  "),
                Span::styled("[s]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" sort  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" remote  "),
                Span::styled("[h]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" help  "),
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
        InputMode::Search => {
            let match_count = match state.view_mode {
                ViewMode::Remote => state.filtered_ports().len(),
                ViewMode::Local => state.filtered_local_ports().len(),
            };
            Line::from(vec![
                Span::raw(" /"),
                Span::styled(&state.search_query, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
                Span::styled(
                    format!("  {} match{}", match_count, if match_count == 1 { "" } else { "es" }),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  [enter] select  [esc] cancel"),
            ])
        }
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
        InputMode::Help => Line::from(Span::raw(" Press any key to close help")),
        InputMode::FilePathInput(input) => Line::from(vec![
            Span::raw(" Local file path: "),
            Span::styled(input, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw("  [enter] confirm  [esc] cancel"),
        ]),
        InputMode::RemotePathInput { remote, .. } => Line::from(vec![
            Span::raw(" Remote path: "),
            Span::styled(remote, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw("  [enter] send  [esc] cancel"),
        ]),
    };

    f.render_widget(
        Paragraph::new(help_text).block(Block::default().borders(Borders::TOP)),
        area,
    );
}

fn render_help_overlay(f: &mut Frame, state: &AppState) {
    let lines: Vec<Line> = match state.view_mode {
        ViewMode::Remote => vec![
            Line::from(vec![
                Span::styled("  enter  ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Forward or stop the selected port"),
            ]),
            Line::from(vec![
                Span::styled("  o      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Open port in browser (auto-forwards if needed)"),
            ]),
            Line::from(vec![
                Span::styled("  p      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Forward with a custom local port number"),
            ]),
            Line::from(vec![
                Span::styled("  f      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Send a local file to the remote machine"),
            ]),
            Line::from(vec![
                Span::styled("  /      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Search and filter ports by any column"),
            ]),
            Line::from(vec![
                Span::styled("  s      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Sort ports by a column"),
            ]),
            Line::from(vec![
                Span::styled("  r      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Refresh ports, reconnect SSH if needed"),
            ]),
            Line::from(vec![
                Span::styled("  tab    ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Switch to local ports view"),
            ]),
            Line::from(vec![
                Span::styled("  h      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Show this help"),
            ]),
            Line::from(vec![
                Span::styled("  q      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Quit"),
            ]),
        ],
        ViewMode::Local => vec![
            Line::from(vec![
                Span::styled("  o      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Open port in browser"),
            ]),
            Line::from(vec![
                Span::styled("  /      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Search and filter ports by any column"),
            ]),
            Line::from(vec![
                Span::styled("  s      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Sort ports by a column"),
            ]),
            Line::from(vec![
                Span::styled("  r      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Refresh the port list"),
            ]),
            Line::from(vec![
                Span::styled("  tab    ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Switch to remote ports view"),
            ]),
            Line::from(vec![
                Span::styled("  h      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Show this help"),
            ]),
            Line::from(vec![
                Span::styled("  q      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Quit"),
            ]),
        ],
    };

    let height = lines.len() as u16 + 2; // +2 for border
    let width = 54;
    let area = f.area();
    let popup = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };

    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Help — press any key to close ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::discovery::DiscoveredPort;
    use ratatui::{backend::TestBackend, Terminal};

    fn make_port(port: u16) -> DiscoveredPort {
        DiscoveredPort {
            port,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some(format!("proc-{port}")),
            pid: Some(u32::from(port)),
        }
    }

    fn render_lines(state: &mut AppState, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, state)).unwrap();

        let buffer = terminal.backend_mut().buffer().clone();
        let row_width = buffer.area.width as usize;

        buffer
            .content
            .chunks(row_width)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    #[test]
    fn test_remote_table_scrolls_selected_row_into_view() {
        let mut state = AppState::new("host".to_string());
        let mut ports: Vec<DiscoveredPort> = (4100..4120).map(make_port).collect();
        ports.push(make_port(56387));
        state.update_ports(ports);
        state.selected = state.ports.len() - 1;

        let lines = render_lines(&mut state, 80, 10);
        let screen = lines.join("\n");

        assert!(screen.contains("56387"), "screen did not show selected port:\n{screen}");
        assert!(!screen.contains("4100"), "screen did not scroll away from top rows:\n{screen}");
    }

    #[test]
    fn test_local_table_scrolls_selected_row_into_view() {
        let mut state = AppState::new("host".to_string());
        state.view_mode = ViewMode::Local;
        let mut ports: Vec<DiscoveredPort> = (5100..5120).map(make_port).collect();
        ports.push(make_port(56387));
        state.update_local_ports(ports);
        state.local_selected = state.local_ports.len() - 1;

        let lines = render_lines(&mut state, 80, 10);
        let screen = lines.join("\n");

        assert!(screen.contains("56387"), "screen did not show selected port:\n{screen}");
        assert!(!screen.contains("5100"), "screen did not scroll away from top rows:\n{screen}");
    }
}
