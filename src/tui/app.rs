use crate::ssh::discovery::DiscoveredPort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardStatus {
    /// Port discovered but not forwarded
    Idle,
    /// Currently forwarded
    Active { local_port: u16 },
    /// Forward failed (e.g., port conflict)
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PortEntry {
    pub discovered: DiscoveredPort,
    pub forward_status: ForwardStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    /// User is typing a port number override
    PortInput(String),
}

pub struct AppState {
    pub host_alias: String,
    pub connection: ConnectionState,
    pub ports: Vec<PortEntry>,
    pub selected: usize,
    pub input_mode: InputMode,
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new(host_alias: String) -> Self {
        Self {
            host_alias,
            connection: ConnectionState::Connected,
            ports: Vec::new(),
            selected: 0,
            input_mode: InputMode::Normal,
            status_message: None,
        }
    }

    pub fn update_ports(&mut self, discovered: Vec<DiscoveredPort>) {
        let mut new_ports = Vec::new();
        for dp in discovered {
            let existing = self.ports.iter().find(|p| {
                p.discovered.port == dp.port && p.discovered.bind_address == dp.bind_address
            });

            let forward_status = match existing {
                Some(e) => e.forward_status.clone(),
                None => ForwardStatus::Idle,
            };

            new_ports.push(PortEntry {
                discovered: dp,
                forward_status,
            });
        }
        self.ports = new_ports;
        if self.selected >= self.ports.len() && !self.ports.is_empty() {
            self.selected = self.ports.len() - 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.ports.len() {
            self.selected += 1;
        }
    }

    pub fn selected_port(&self) -> Option<&PortEntry> {
        self.ports.get(self.selected)
    }

    pub fn set_forward_active(&mut self, index: usize, local_port: u16) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Active { local_port };
        }
    }

    pub fn set_forward_idle(&mut self, index: usize) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Idle;
        }
    }

    pub fn set_forward_error(&mut self, index: usize, msg: String) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Error(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_port(port: u16, name: &str) -> DiscoveredPort {
        DiscoveredPort {
            port,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some(name.to_string()),
            pid: Some(1000),
        }
    }

    #[test]
    fn test_new_state() {
        let state = AppState::new("my-remote".to_string());
        assert_eq!(state.host_alias, "my-remote");
        assert_eq!(state.connection, ConnectionState::Connected);
        assert!(state.ports.is_empty());
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_update_ports_fresh() {
        let mut state = AppState::new("host".to_string());
        let ports = vec![make_port(8080, "nginx"), make_port(3000, "node")];
        state.update_ports(ports);
        assert_eq!(state.ports.len(), 2);
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
        assert_eq!(state.ports[1].forward_status, ForwardStatus::Idle);
    }

    #[test]
    fn test_update_ports_preserves_forward_state() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.set_forward_active(0, 8080);

        // Re-discover — 8080 still there, 3000 gone, 5000 new
        state.update_ports(vec![make_port(8080, "nginx"), make_port(5000, "python")]);
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Active { local_port: 8080 }
        );
        assert_eq!(state.ports[1].forward_status, ForwardStatus::Idle);
    }

    #[test]
    fn test_navigation() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "a"),
            make_port(3000, "b"),
            make_port(5000, "c"),
        ]);

        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_up();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_selection_clamp_on_update() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.selected = 1;
        state.update_ports(vec![make_port(8080, "a")]);
        assert_eq!(state.selected, 0);
    }

    // ---- selected_port ----

    #[test]
    fn test_selected_port_returns_entry() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.selected = 1;
        let entry = state.selected_port().unwrap();
        assert_eq!(entry.discovered.port, 3000);
    }

    #[test]
    fn test_selected_port_empty_returns_none() {
        let state = AppState::new("host".to_string());
        assert!(state.selected_port().is_none());
    }

    // ---- set_forward_active ----

    #[test]
    fn test_set_forward_active() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_active(0, 9090);
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Active { local_port: 9090 }
        );
    }

    #[test]
    fn test_set_forward_active_out_of_bounds() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_active(5, 9090); // should not panic
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
    }

    // ---- set_forward_idle ----

    #[test]
    fn test_set_forward_idle() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_active(0, 8080);
        state.set_forward_idle(0);
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
    }

    #[test]
    fn test_set_forward_idle_out_of_bounds() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_idle(5); // should not panic
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
    }

    // ---- set_forward_error ----

    #[test]
    fn test_set_forward_error() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_error(0, "port in use".to_string());
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Error("port in use".to_string())
        );
    }

    #[test]
    fn test_set_forward_error_out_of_bounds() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_error(5, "oops".to_string()); // should not panic
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
    }

    // ---- update_ports edge cases ----

    #[test]
    fn test_update_ports_to_empty() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a")]);
        state.update_ports(vec![]);
        assert!(state.ports.is_empty());
    }

    #[test]
    fn test_update_ports_preserves_error_state() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_error(0, "conflict".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Error("conflict".to_string())
        );
    }

    #[test]
    fn test_update_ports_same_port_different_bind_addr() {
        let mut state = AppState::new("host".to_string());
        let p1 = DiscoveredPort {
            port: 8080,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some("nginx".to_string()),
            pid: Some(1),
        };
        state.update_ports(vec![p1]);
        state.set_forward_active(0, 8080);

        // Same port but different bind address — should NOT preserve forward state
        let p2 = DiscoveredPort {
            port: 8080,
            bind_address: "::".to_string(),
            process_name: Some("nginx".to_string()),
            pid: Some(1),
        };
        state.update_ports(vec![p2]);
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
    }

    // ---- navigation edge cases ----

    #[test]
    fn test_move_on_empty_state() {
        let mut state = AppState::new("host".to_string());
        state.move_up();
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_move_on_single_port() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a")]);
        state.move_down();
        assert_eq!(state.selected, 0); // can't go past single item
        state.move_up();
        assert_eq!(state.selected, 0);
    }
}
