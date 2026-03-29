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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Remote,
    Local,
}

pub struct AppState {
    pub host_alias: String,
    pub connection: ConnectionState,
    pub ports: Vec<PortEntry>,
    pub selected: usize,
    pub input_mode: InputMode,
    pub status_message: Option<String>,
    pub view_mode: ViewMode,
    pub local_ports: Vec<DiscoveredPort>,
    pub local_selected: usize,
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
            view_mode: ViewMode::Remote,
            local_ports: Vec::new(),
            local_selected: 0,
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

    pub fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Remote => ViewMode::Local,
            ViewMode::Local => ViewMode::Remote,
        };
    }

    pub fn update_local_ports(&mut self, ports: Vec<DiscoveredPort>) {
        self.local_ports = ports;
        if self.local_selected >= self.local_ports.len() && !self.local_ports.is_empty() {
            self.local_selected = self.local_ports.len() - 1;
        }
    }

    pub fn local_move_up(&mut self) {
        if self.local_selected > 0 {
            self.local_selected -= 1;
        }
    }

    pub fn local_move_down(&mut self) {
        if self.local_selected + 1 < self.local_ports.len() {
            self.local_selected += 1;
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

    // ---- ViewMode tests ----

    #[test]
    fn test_new_state_defaults_to_remote_view() {
        let state = AppState::new("host".to_string());
        assert_eq!(state.view_mode, ViewMode::Remote);
        assert!(state.local_ports.is_empty());
        assert_eq!(state.local_selected, 0);
    }

    #[test]
    fn test_toggle_view_mode() {
        let mut state = AppState::new("host".to_string());
        assert_eq!(state.view_mode, ViewMode::Remote);
        state.toggle_view();
        assert_eq!(state.view_mode, ViewMode::Local);
        state.toggle_view();
        assert_eq!(state.view_mode, ViewMode::Remote);
    }

    #[test]
    fn test_local_selected_independent_of_selected() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.update_local_ports(vec![
            make_port(5000, "c"),
            make_port(6000, "d"),
            make_port(7000, "e"),
        ]);
        state.selected = 1;
        state.local_selected = 2;
        assert_eq!(state.selected, 1);
        assert_eq!(state.local_selected, 2);
    }

    #[test]
    fn test_update_local_ports() {
        let mut state = AppState::new("host".to_string());
        let ports = vec![make_port(3000, "node"), make_port(8080, "nginx")];
        state.update_local_ports(ports);
        assert_eq!(state.local_ports.len(), 2);
        assert_eq!(state.local_ports[0].port, 3000);
        assert_eq!(state.local_ports[1].port, 8080);
    }

    #[test]
    fn test_update_local_ports_clamps_selection() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
        state.local_selected = 1;
        state.update_local_ports(vec![make_port(3000, "a")]);
        assert_eq!(state.local_selected, 0);
    }

    #[test]
    fn test_update_local_ports_to_empty() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "a")]);
        state.local_selected = 0;
        state.update_local_ports(vec![]);
        assert!(state.local_ports.is_empty());
    }

    #[test]
    fn test_update_local_ports_does_not_affect_remote_ports() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_active(0, 8080);
        state.update_local_ports(vec![make_port(3000, "node")]);
        assert_eq!(state.ports.len(), 1);
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Active { local_port: 8080 }
        );
    }

    #[test]
    fn test_local_move_up() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
        state.local_selected = 1;
        state.local_move_up();
        assert_eq!(state.local_selected, 0);
        state.local_move_up();
        assert_eq!(state.local_selected, 0); // can't go below 0
    }

    #[test]
    fn test_local_move_down() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
        state.local_move_down();
        assert_eq!(state.local_selected, 1);
        state.local_move_down();
        assert_eq!(state.local_selected, 1); // can't go past end
    }

    #[test]
    fn test_local_move_on_empty() {
        let mut state = AppState::new("host".to_string());
        state.local_move_up();
        assert_eq!(state.local_selected, 0);
        state.local_move_down();
        assert_eq!(state.local_selected, 0);
    }

    #[test]
    fn test_local_move_on_single_port() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "a")]);
        state.local_move_down();
        assert_eq!(state.local_selected, 0);
        state.local_move_up();
        assert_eq!(state.local_selected, 0);
    }
}
