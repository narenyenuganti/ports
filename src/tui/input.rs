use crossterm::event::{KeyCode, KeyEvent};

use super::app::{AppState, InputMode, ViewMode};

/// Actions the event loop should perform after handling input.
#[derive(Debug)]
pub enum Action {
    None,
    Quit,
    ToggleForward(usize),
    StartForwardWithPort(usize, u16),
    Refresh,
    Reconnect,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> Action {
    match &state.input_mode {
        InputMode::Normal => handle_normal_mode(state, key),
        InputMode::PortInput(_) => handle_port_input(state, key),
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
            if state.ports.is_empty() {
                return Action::None;
            }
            Action::ToggleForward(state.selected)
        }
        KeyCode::Char('c') => Action::Reconnect,
        KeyCode::Char('p') => {
            if !state.ports.is_empty() {
                state.input_mode = InputMode::PortInput(String::new());
            }
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
                    return Action::StartForwardWithPort(state.selected, port);
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
    fn test_reconnect() {
        let mut state = AppState::new("host".to_string());
        assert!(matches!(handle_key(&mut state, key(KeyCode::Char('c'))), Action::Reconnect));
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
    fn test_local_c_is_noop() {
        let mut state = state_with_local_ports();
        assert!(matches!(
            handle_key(&mut state, key(KeyCode::Char('c'))),
            Action::None
        ));
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
}
