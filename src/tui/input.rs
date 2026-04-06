use crossterm::event::{KeyCode, KeyEvent};

use super::app::{AppState, ForwardStatus, InputMode, SortState, ViewMode};

/// Actions the event loop should perform after handling input.
#[derive(Debug)]
pub enum Action {
    None,
    Quit,
    ToggleForward(usize),
    StartForwardWithPort(usize, u16),
    Refresh,
    OpenBrowser(u16),
    ForwardAndOpen(usize),
    SendFile { local: String, remote: String },
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> Action {
    match &state.input_mode {
        InputMode::Normal => handle_normal_mode(state, key),
        InputMode::PortInput(_) => handle_port_input(state, key),
        InputMode::SortSelect => handle_sort_select(state, key),
        InputMode::Search => handle_search(state, key),
        InputMode::Help => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        InputMode::FilePathInput(_) => handle_file_path_input(state, key),
        InputMode::RemotePathInput { .. } => handle_remote_path_input(state, key),
    }
}

fn handle_normal_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Tab => {
            state.toggle_view();
            Action::None
        }
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('h') => {
            state.input_mode = InputMode::Help;
            Action::None
        }
        _ => match state.view_mode {
            ViewMode::Remote => handle_remote_mode(state, key),
            ViewMode::Local => handle_local_mode(state, key),
        },
    }
}

fn handle_remote_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_up();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_down();
            Action::None
        }
        KeyCode::Enter => {
            if let Some(orig_idx) = state.original_port_index(state.selected) {
                Action::ToggleForward(orig_idx)
            } else {
                Action::None
            }
        }
        KeyCode::Char('p') => {
            if !state.ports.is_empty() {
                state.input_mode = InputMode::PortInput(String::new());
            }
            Action::None
        }
        KeyCode::Char('o') => {
            let sorted = state.sorted_ports();
            if let Some(entry) = sorted.get(state.selected) {
                match &entry.forward_status {
                    ForwardStatus::Active { local_port } => Action::OpenBrowser(*local_port),
                    _ => {
                        if let Some(orig_idx) = state.original_port_index(state.selected) {
                            Action::ForwardAndOpen(orig_idx)
                        } else {
                            Action::None
                        }
                    }
                }
            } else {
                Action::None
            }
        }
        KeyCode::Char('s') => {
            state.sort.column = 0;
            state.input_mode = InputMode::SortSelect;
            Action::None
        }
        KeyCode::Char('/') => {
            state.enter_search();
            Action::None
        }
        KeyCode::Char('f') => {
            state.input_mode = InputMode::FilePathInput(String::new());
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_local_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.local_move_up();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.local_move_down();
            Action::None
        }
        KeyCode::Char('o') => {
            let sorted = state.sorted_local_ports();
            if let Some(port) = sorted.get(state.local_selected) {
                Action::OpenBrowser(port.port)
            } else {
                Action::None
            }
        }
        KeyCode::Char('s') => {
            state.sort.column = 0;
            state.input_mode = InputMode::SortSelect;
            Action::None
        }
        KeyCode::Char('/') => {
            state.enter_search();
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_sort_select(state: &mut AppState, key: KeyEvent) -> Action {
    let max_cols = match state.view_mode {
        ViewMode::Remote => SortState::column_count_remote(),
        ViewMode::Local => SortState::column_count_local(),
    };
    match key.code {
        KeyCode::Left => {
            state.sort.move_left();
            Action::None
        }
        KeyCode::Right => {
            state.sort.move_right(max_cols);
            Action::None
        }
        KeyCode::Enter => {
            state.sort.toggle_sort();
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Char('r') => {
            state.sort.reset();
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_port_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::PortInput(ref input) = state.input_mode {
                let port_str = input.clone();
                state.input_mode = InputMode::Normal;
                if let Ok(port) = port_str.parse::<u16>() {
                    if let Some(orig_idx) = state.original_port_index(state.selected) {
                        return Action::StartForwardWithPort(orig_idx, port);
                    }
                } else {
                    state.status_message = Some("Invalid port number".to_string());
                }
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::PortInput(ref mut input) = state.input_mode {
                input.pop();
            }
            Action::None
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let InputMode::PortInput(ref mut input) = state.input_mode {
                input.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_file_path_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::FilePathInput(ref input) = state.input_mode {
                let local_path = input.clone();
                if local_path.is_empty() {
                    state.input_mode = InputMode::Normal;
                    state.status_message = Some("No file path provided".to_string());
                    return Action::None;
                }
                let remote_path = if local_path.starts_with('/') {
                    format!("/tmp{}", local_path)
                } else {
                    format!("/tmp/{}", local_path)
                };
                state.input_mode = InputMode::RemotePathInput {
                    local: local_path,
                    remote: remote_path,
                };
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::FilePathInput(ref mut input) = state.input_mode {
                input.pop();
            }
            Action::None
        }
        KeyCode::Char(c) => {
            if let InputMode::FilePathInput(ref mut input) = state.input_mode {
                input.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_remote_path_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::RemotePathInput { ref local, ref remote } = state.input_mode {
                let local = local.clone();
                let remote = remote.clone();
                state.input_mode = InputMode::Normal;
                if remote.is_empty() {
                    state.status_message = Some("No remote path provided".to_string());
                    return Action::None;
                }
                return Action::SendFile { local, remote };
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::RemotePathInput { ref mut remote, .. } = state.input_mode {
                remote.pop();
            }
            Action::None
        }
        KeyCode::Char(c) => {
            if let InputMode::RemotePathInput { ref mut remote, .. } = state.input_mode {
                remote.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_search(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.exit_search_cancel();
            Action::None
        }
        KeyCode::Enter => {
            state.exit_search_confirm();
            Action::None
        }
        KeyCode::Backspace => {
            state.search_query.pop();
            state.clamp_search_selected();
            Action::None
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') if state.search_selected > 0 => {
            state.search_move_up();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            state.search_move_down();
            Action::None
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
            state.clamp_search_selected();
            Action::None
        }
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh::discovery::DiscoveredPort;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_port(port: u16) -> DiscoveredPort {
        DiscoveredPort {
            port,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some("test".to_string()),
            pid: Some(1),
        }
    }

    fn state_with_ports() -> AppState {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080), make_port(3000), make_port(5000)]);
        state
    }

    // ---- Normal mode tests ----

    #[test]
    fn test_quit() {
        let mut state = AppState::new("host".to_string());
        assert!(matches!(handle_key(&mut state, key(KeyCode::Char('q'))), Action::Quit));
    }

    #[test]
    fn test_refresh() {
        let mut state = AppState::new("host".to_string());
        assert!(matches!(handle_key(&mut state, key(KeyCode::Char('r'))), Action::Refresh));
    }

    #[test]
    fn test_navigate_down_arrow() {
        let mut state = state_with_ports();
        assert!(matches!(handle_key(&mut state, key(KeyCode::Down)), Action::None));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_navigate_up_arrow() {
        let mut state = state_with_ports();
        state.selected = 2;
        assert!(matches!(handle_key(&mut state, key(KeyCode::Up)), Action::None));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn test_navigate_j_k() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.selected, 1);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_enter_toggles_forward() {
        let mut state = state_with_ports();
        state.selected = 1;
        assert!(matches!(handle_key(&mut state, key(KeyCode::Enter)), Action::ToggleForward(1)));
    }

    #[test]
    fn test_enter_on_empty_ports_does_nothing() {
        let mut state = AppState::new("host".to_string());
        assert!(matches!(handle_key(&mut state, key(KeyCode::Enter)), Action::None));
    }

    #[test]
    fn test_p_enters_port_input_mode() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.input_mode, InputMode::PortInput(String::new()));
    }

    #[test]
    fn test_p_on_empty_ports_stays_normal() {
        let mut state = AppState::new("host".to_string());
        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_unknown_key_does_nothing() {
        let mut state = state_with_ports();
        assert!(matches!(handle_key(&mut state, key(KeyCode::Char('x'))), Action::None));
        assert_eq!(state.selected, 0);
    }

    // ---- Port input mode tests ----

    #[test]
    fn test_port_input_digits() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput(String::new());
        handle_key(&mut state, key(KeyCode::Char('8')));
        handle_key(&mut state, key(KeyCode::Char('0')));
        handle_key(&mut state, key(KeyCode::Char('8')));
        handle_key(&mut state, key(KeyCode::Char('0')));
        assert_eq!(state.input_mode, InputMode::PortInput("8080".to_string()));
    }

    #[test]
    fn test_port_input_backspace() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("808".to_string());
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.input_mode, InputMode::PortInput("80".to_string()));
    }

    #[test]
    fn test_port_input_backspace_on_empty() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput(String::new());
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.input_mode, InputMode::PortInput(String::new()));
    }

    #[test]
    fn test_port_input_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("808".to_string());
        assert!(matches!(handle_key(&mut state, key(KeyCode::Esc)), Action::None));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_port_input_enter_valid_port() {
        let mut state = state_with_ports();
        state.selected = 1;
        state.input_mode = InputMode::PortInput("9090".to_string());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::StartForwardWithPort(1, 9090)));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_port_input_enter_empty_string() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput(String::new());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::None));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert!(state.status_message.is_some());
    }

    #[test]
    fn test_port_input_enter_overflow() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("99999".to_string());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::None));
        assert_eq!(state.status_message.as_deref(), Some("Invalid port number"));
    }

    #[test]
    fn test_port_input_ignores_non_digit_chars() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("80".to_string());
        handle_key(&mut state, key(KeyCode::Char('a')));
        assert_eq!(state.input_mode, InputMode::PortInput("80".to_string()));
    }

    #[test]
    fn test_port_input_ignores_unknown_keys() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("80".to_string());
        handle_key(&mut state, key(KeyCode::Tab));
        assert_eq!(state.input_mode, InputMode::PortInput("80".to_string()));
    }

    // ---- Local view mode tests ----

    fn state_with_local_ports() -> AppState {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000), make_port(5000), make_port(8080)]);
        state.view_mode = ViewMode::Local;
        state
    }

    #[test]
    fn test_tab_switches_to_local() {
        let mut state = state_with_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Tab)),
            Action::None
        ));
        assert_eq!(state.view_mode, ViewMode::Local);
    }

    #[test]
    fn test_tab_switches_back_to_remote() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Tab)),
            Action::None
        ));
        assert_eq!(state.view_mode, ViewMode::Remote);
    }

    #[test]
    fn test_local_navigate_down() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.local_selected, 1);
        assert_eq!(state.selected, 0); // remote cursor unchanged
    }

    #[test]
    fn test_local_navigate_up() {
        let mut state = state_with_local_ports();
        state.local_selected = 2;
        handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.local_selected, 1);
    }

    #[test]
    fn test_local_navigate_j_k() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.local_selected, 1);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.local_selected, 0);
    }

    #[test]
    fn test_local_enter_is_noop() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Enter)),
            Action::None
        ));
    }

    #[test]
    fn test_local_p_is_noop() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_local_r_refreshes() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Char('r'))),
            Action::Refresh
        ));
    }

    #[test]
    fn test_local_q_quits() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Char('q'))),
            Action::Quit
        ));
    }

    #[test]
    fn test_tab_in_port_input_mode_is_noop() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::PortInput("80".to_string());
        handle_key(&mut state, key(KeyCode::Tab));
        assert_eq!(state.input_mode, InputMode::PortInput("80".to_string()));
        assert_eq!(state.view_mode, ViewMode::Remote);
    }

    #[test]
    fn test_local_unknown_key_is_noop() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Char('x'))),
            Action::None
        ));
        assert_eq!(state.local_selected, 0);
    }

    // ---- Sort mode tests ----

    use crate::tui::app::SortOrder;

    #[test]
    fn test_s_enters_sort_mode_remote() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('s')));
        assert_eq!(state.input_mode, InputMode::SortSelect);
        assert_eq!(state.sort.column, 0);
    }

    #[test]
    fn test_s_enters_sort_mode_local() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('s')));
        assert_eq!(state.input_mode, InputMode::SortSelect);
        assert_eq!(state.sort.column, 0);
    }

    #[test]
    fn test_sort_left_right_navigation() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::SortSelect;
        handle_key(&mut state, key(KeyCode::Right));
        assert_eq!(state.sort.column, 1);
        handle_key(&mut state, key(KeyCode::Right));
        assert_eq!(state.sort.column, 2);
        handle_key(&mut state, key(KeyCode::Left));
        assert_eq!(state.sort.column, 1);
    }

    #[test]
    fn test_sort_right_clamps_remote() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::SortSelect;
        state.sort.column = 4; // last remote column (PID)
        handle_key(&mut state, key(KeyCode::Right));
        assert_eq!(state.sort.column, 4);
    }

    #[test]
    fn test_sort_right_clamps_local() {
        let mut state = state_with_local_ports();
        state.input_mode = InputMode::SortSelect;
        state.sort.column = 3; // last local column (PID)
        handle_key(&mut state, key(KeyCode::Right));
        assert_eq!(state.sort.column, 3);
    }

    #[test]
    fn test_sort_enter_toggles_and_exits() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::SortSelect;
        state.sort.column = 1; // Port
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.sort.active, Some((1, SortOrder::Ascending)));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_sort_r_resets_and_exits() {
        let mut state = state_with_ports();
        state.sort.active = Some((1, SortOrder::Ascending));
        state.input_mode = InputMode::SortSelect;
        handle_key(&mut state, key(KeyCode::Char('r')));
        assert_eq!(state.sort.active, None);
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_sort_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::SortSelect;
        state.sort.column = 3;
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.sort.column, 3); // column preserved
    }

    #[test]
    fn test_sort_unknown_key_is_noop() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::SortSelect;
        state.sort.column = 2;
        handle_key(&mut state, key(KeyCode::Char('x')));
        assert_eq!(state.input_mode, InputMode::SortSelect);
        assert_eq!(state.sort.column, 2);
    }

    #[test]
    fn test_sort_full_cycle_ascending_descending_reset() {
        let mut state = state_with_ports();
        // Enter sort mode
        handle_key(&mut state, key(KeyCode::Char('s')));
        // Navigate to Port column
        handle_key(&mut state, key(KeyCode::Right));
        assert_eq!(state.sort.column, 1);
        // Toggle ascending
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.sort.active, Some((1, SortOrder::Ascending)));
        assert_eq!(state.input_mode, InputMode::Normal);

        // Re-enter sort mode, toggle to descending
        handle_key(&mut state, key(KeyCode::Char('s')));
        state.sort.column = 1; // stays on Port
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.sort.active, Some((1, SortOrder::Descending)));

        // Re-enter and reset
        handle_key(&mut state, key(KeyCode::Char('s')));
        handle_key(&mut state, key(KeyCode::Char('r')));
        assert_eq!(state.sort.active, None);
    }

    // ---- Open browser tests ----

    #[test]
    fn test_o_forwards_and_opens_remote_idle() {
        let mut state = state_with_ports();
        state.selected = 0;
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        // Port 8080 is idle, so forward first then open
        assert!(matches!(action, Action::ForwardAndOpen(0)));
    }

    #[test]
    fn test_o_opens_browser_remote_active_uses_local_port() {
        let mut state = state_with_ports();
        state.set_forward_active(0, 9090);
        state.selected = 0;
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        // Port is forwarded to local 9090, so opens that
        assert!(matches!(action, Action::OpenBrowser(9090)));
    }

    #[test]
    fn test_o_forwards_and_opens_remote_error() {
        let mut state = state_with_ports();
        state.set_forward_error(0, "conflict".to_string());
        state.selected = 0;
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::ForwardAndOpen(0)));
    }

    #[test]
    fn test_o_on_empty_remote_is_noop() {
        let mut state = AppState::new("host".to_string());
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn test_o_opens_browser_local() {
        let mut state = state_with_local_ports();
        state.local_selected = 1; // port 5000
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::OpenBrowser(5000)));
    }

    #[test]
    fn test_o_on_empty_local_is_noop() {
        let mut state = AppState::new("host".to_string());
        state.view_mode = ViewMode::Local;
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn test_o_forwards_and_opens_remote_second_port() {
        let mut state = state_with_ports();
        state.selected = 1; // port 3000
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::ForwardAndOpen(1)));
    }

    // ---- Sort + action correctness tests ----

    #[test]
    fn test_o_opens_browser_local_with_sort_active() {
        let mut state = state_with_local_ports(); // ports: 3000, 5000, 8080
        // Sort by port descending: visual order becomes 8080, 5000, 3000
        state.sort.active = Some((1, SortOrder::Descending));
        state.local_selected = 0; // visually 8080
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::OpenBrowser(8080)));

        state.local_selected = 2; // visually 3000
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::OpenBrowser(3000)));
    }

    #[test]
    fn test_o_forwards_and_opens_remote_with_sort_active() {
        let mut state = state_with_ports(); // ports: 8080, 3000, 5000
        // Sort by port ascending: visual order becomes 3000, 5000, 8080
        state.sort.active = Some((1, SortOrder::Ascending));
        state.selected = 0; // visually 3000 (original index 1)
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::ForwardAndOpen(1)));

        state.selected = 2; // visually 8080 (original index 0)
        let action = handle_key(&mut state, key(KeyCode::Char('o')));
        assert!(matches!(action, Action::ForwardAndOpen(0)));
    }

    #[test]
    fn test_enter_toggles_correct_port_with_sort_active() {
        let mut state = state_with_ports(); // unsorted: 8080(idx0), 3000(idx1), 5000(idx2)
        // Sort by port ascending: visual order 3000, 5000, 8080
        state.sort.active = Some((1, SortOrder::Ascending));
        state.selected = 0; // visually 3000 (original index 1)
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::ToggleForward(1)));

        state.selected = 2; // visually 8080 (original index 0)
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::ToggleForward(0)));
    }

    #[test]
    fn test_port_input_targets_correct_port_with_sort_active() {
        let mut state = state_with_ports(); // unsorted: 8080(idx0), 3000(idx1), 5000(idx2)
        state.sort.active = Some((1, SortOrder::Ascending));
        state.selected = 2; // visually 8080 (original index 0)
        state.input_mode = InputMode::PortInput("9090".to_string());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::StartForwardWithPort(0, 9090)));
    }

    // ---- File send mode tests ----

    #[test]
    fn test_f_enters_file_path_input_mode() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(state.input_mode, InputMode::FilePathInput(String::new()));
    }

    #[test]
    fn test_f_in_local_view_is_noop() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_file_path_input_typing() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput(String::new());
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Char('t')));
        handle_key(&mut state, key(KeyCode::Char('m')));
        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.input_mode, InputMode::FilePathInput("/tmp".to_string()));
    }

    #[test]
    fn test_file_path_input_backspace() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/tmp".to_string());
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.input_mode, InputMode::FilePathInput("/tm".to_string()));
    }

    #[test]
    fn test_file_path_input_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/tmp/foo".to_string());
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_file_path_input_enter_transitions_to_remote_path() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/home/user/file.txt".to_string());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::None));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tmp/home/user/file.txt".to_string(),
            }
        );
    }

    #[test]
    fn test_file_path_input_enter_relative_path_gets_slash() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("file.txt".to_string());
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "file.txt".to_string(),
                remote: "/tmp/file.txt".to_string(),
            }
        );
    }

    #[test]
    fn test_file_path_input_enter_empty_shows_error() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput(String::new());
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.status_message.as_deref(), Some("No file path provided"));
    }

    #[test]
    fn test_remote_path_input_enter_produces_send_file() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp/home/user/file.txt".to_string(),
        };
        let action = handle_key(&mut state, key(KeyCode::Enter));
        match action {
            Action::SendFile { local, remote } => {
                assert_eq!(local, "/home/user/file.txt");
                assert_eq!(remote, "/tmp/home/user/file.txt");
            }
            _ => panic!("Expected SendFile action"),
        }
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_remote_path_input_editing() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tmp/f".to_string(),
            }
        );
    }

    #[test]
    fn test_remote_path_input_backspace() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tm".to_string(),
            }
        );
    }

    #[test]
    fn test_remote_path_input_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp/home/user/file.txt".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_remote_path_input_enter_empty_shows_error() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: String::new(),
        };
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.status_message.as_deref(), Some("No remote path provided"));
    }

    // ---- Search mode tests ----

    #[test]
    fn test_slash_enters_search_remote() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.input_mode, InputMode::Search);
    }

    #[test]
    fn test_slash_enters_search_local() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.input_mode, InputMode::Search);
    }

    #[test]
    fn test_search_typing_appends() {
        let mut state = state_with_ports();
        state.enter_search();
        handle_key(&mut state, key(KeyCode::Char('n')));
        handle_key(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.search_query, "ng");
    }

    #[test]
    fn test_search_backspace() {
        let mut state = state_with_ports();
        state.enter_search();
        state.search_query = "ngi".to_string();
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.search_query, "ng");
    }

    #[test]
    fn test_search_esc_cancels() {
        let mut state = state_with_ports();
        state.selected = 2;
        state.enter_search();
        state.search_query = "8080".to_string();
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.selected, 2); // restored
        assert_eq!(state.search_query, "");
    }

    #[test]
    fn test_search_enter_confirms() {
        let mut state = state_with_ports(); // 8080, 3000, 5000
        state.selected = 0;
        state.enter_search();
        state.search_query = "3000".to_string();
        state.search_selected = 0;
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.selected, 1); // 3000 is at index 1
    }

    #[test]
    fn test_search_navigate_up_down() {
        let mut state = state_with_ports(); // 8080, 3000, 5000
        state.enter_search();
        // Empty query, all 3 visible
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.search_selected, 1);
        handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.search_selected, 0);
    }

    #[test]
    fn test_search_navigate_j_k() {
        let mut state = state_with_ports();
        state.enter_search();
        handle_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.search_selected, 1);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.search_selected, 0);
    }

    #[test]
    fn test_search_clamps_on_type() {
        let mut state = state_with_ports(); // 8080, 3000, 5000
        state.enter_search();
        state.search_selected = 2; // at last item
        // Type a query that narrows to 1 result
        handle_key(&mut state, key(KeyCode::Char('8')));
        handle_key(&mut state, key(KeyCode::Char('0')));
        handle_key(&mut state, key(KeyCode::Char('8')));
        handle_key(&mut state, key(KeyCode::Char('0')));
        assert_eq!(state.search_selected, 0); // clamped
    }
}
