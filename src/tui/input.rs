use crossterm::event::{KeyCode, KeyEvent};

use super::app::{AppState, InputMode};

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
        KeyCode::Char('r') => Action::Refresh,
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
