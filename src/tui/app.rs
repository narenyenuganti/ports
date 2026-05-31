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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    /// User is typing a port number override
    PortInput(String),
    /// User is selecting a column to sort by
    SortSelect,
    /// User is typing a search query
    Search,
    /// Showing help overlay
    Help,
    /// User is typing a local file path to send
    FilePathInput(String),
    /// User is editing the remote destination path
    RemotePathInput {
        local: String,
        remote: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Remote,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortState {
    /// Which column index is highlighted in sort mode
    pub column: usize,
    /// Active sort: (column index, order). None means default (port ascending).
    pub active: Option<(usize, SortOrder)>,
}

impl SortState {
    pub fn new() -> Self {
        Self {
            column: 0,
            active: None,
        }
    }

    pub fn column_count_remote() -> usize {
        5 // Status, Port, Local Address, Process, PID
    }

    pub fn column_count_local() -> usize {
        4 // Bind Address, Port, Process, PID
    }

    pub fn move_left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        }
    }

    pub fn move_right(&mut self, max_cols: usize) {
        if self.column + 1 < max_cols {
            self.column += 1;
        }
    }

    pub fn toggle_sort(&mut self) {
        self.active = match self.active {
            Some((col, SortOrder::Ascending)) if col == self.column => {
                Some((col, SortOrder::Descending))
            }
            Some((col, SortOrder::Descending)) if col == self.column => None,
            _ => Some((self.column, SortOrder::Ascending)),
        };
    }

    pub fn reset(&mut self) {
        self.active = None;
    }
}

pub struct AppState {
    pub host_alias: String,
    pub connection: ConnectionState,
    pub ports: Vec<PortEntry>,
    pub selected: usize,
    pub(crate) remote_scroll_offset: usize,
    pub input_mode: InputMode,
    pub status_message: Option<String>,
    pub view_mode: ViewMode,
    pub local_ports: Vec<DiscoveredPort>,
    pub local_selected: usize,
    pub(crate) local_scroll_offset: usize,
    pub sort: SortState,
    pub search_query: String,
    pub search_selected: usize,
    pre_search_selected: usize,
    pre_search_local_selected: usize,
    pub file_transfer_status: Option<String>,
}

impl AppState {
    pub fn new(host_alias: String) -> Self {
        Self {
            host_alias,
            connection: ConnectionState::Connected,
            ports: Vec::new(),
            selected: 0,
            remote_scroll_offset: 0,
            input_mode: InputMode::Normal,
            status_message: None,
            view_mode: ViewMode::Remote,
            local_ports: Vec::new(),
            local_selected: 0,
            local_scroll_offset: 0,
            sort: SortState::new(),
            search_query: String::new(),
            search_selected: 0,
            pre_search_selected: 0,
            pre_search_local_selected: 0,
            file_transfer_status: None,
        }
    }

    pub fn update_ports(&mut self, discovered: Vec<DiscoveredPort>) {
        let mut new_ports = Vec::new();
        for dp in discovered {
            let existing = self.ports.iter().find(|p| {
                p.discovered.port == dp.port && p.discovered.bind_address == dp.bind_address
            });

            let forward_status = match existing {
                Some(e) => match &e.forward_status {
                    // Preserve active forwards across refresh
                    ForwardStatus::Active { .. } => e.forward_status.clone(),
                    // Clear errors on refresh — conditions may have changed
                    ForwardStatus::Error(_) => ForwardStatus::Idle,
                    ForwardStatus::Idle => ForwardStatus::Idle,
                },
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

    #[allow(dead_code)]
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

    /// Map a visual selection index (in the sorted view) to the original
    /// index in `self.ports`. Returns `None` if out of bounds.
    pub fn original_port_index(&self, visual_idx: usize) -> Option<usize> {
        let sorted = self.sorted_ports();
        let entry = sorted.get(visual_idx)?;
        self.ports.iter().position(|p| std::ptr::eq(p, *entry))
    }

    /// Return remote ports sorted according to current sort state.
    pub fn sorted_ports(&self) -> Vec<&PortEntry> {
        let mut entries: Vec<&PortEntry> = self.ports.iter().collect();
        if let Some((col, order)) = &self.sort.active {
            entries.sort_by(|a, b| {
                let cmp = match col {
                    0 => {
                        // Status: Active > Error > Idle
                        let rank = |s: &ForwardStatus| match s {
                            ForwardStatus::Active { .. } => 2,
                            ForwardStatus::Error(_) => 1,
                            ForwardStatus::Idle => 0,
                        };
                        rank(&a.forward_status).cmp(&rank(&b.forward_status))
                    }
                    1 => a.discovered.port.cmp(&b.discovered.port),
                    3 => {
                        let a_name = a.discovered.process_name.as_deref().unwrap_or("");
                        let b_name = b.discovered.process_name.as_deref().unwrap_or("");
                        a_name.to_lowercase().cmp(&b_name.to_lowercase())
                    }
                    4 => a
                        .discovered
                        .pid
                        .unwrap_or(0)
                        .cmp(&b.discovered.pid.unwrap_or(0)),
                    _ => std::cmp::Ordering::Equal, // Local Address (col 2) not sortable
                };
                match order {
                    SortOrder::Ascending => cmp,
                    SortOrder::Descending => cmp.reverse(),
                }
            });
        }
        entries
    }

    /// Return local ports sorted according to current sort state.
    pub fn sorted_local_ports(&self) -> Vec<&DiscoveredPort> {
        let mut entries: Vec<&DiscoveredPort> = self.local_ports.iter().collect();
        if let Some((col, order)) = &self.sort.active {
            entries.sort_by(|a, b| {
                let cmp = match col {
                    0 => a.bind_address.cmp(&b.bind_address),
                    1 => a.port.cmp(&b.port),
                    2 => {
                        let a_name = a.process_name.as_deref().unwrap_or("");
                        let b_name = b.process_name.as_deref().unwrap_or("");
                        a_name.to_lowercase().cmp(&b_name.to_lowercase())
                    }
                    3 => a.pid.unwrap_or(0).cmp(&b.pid.unwrap_or(0)),
                    _ => std::cmp::Ordering::Equal,
                };
                match order {
                    SortOrder::Ascending => cmp,
                    SortOrder::Descending => cmp.reverse(),
                }
            });
        }
        entries
    }

    /// Enter search mode, saving current cursor positions.
    pub fn enter_search(&mut self) {
        self.pre_search_selected = self.selected;
        self.pre_search_local_selected = self.local_selected;
        self.search_query.clear();
        self.search_selected = 0;
        self.input_mode = InputMode::Search;
    }

    /// Exit search on Enter: move cursor to the selected filtered entry, clear search.
    pub fn exit_search_confirm(&mut self) {
        match self.view_mode {
            ViewMode::Remote => {
                let filtered = self.filtered_ports();
                if let Some(entry) = filtered.get(self.search_selected) {
                    // Find this entry's index in the full sorted view
                    let sorted = self.sorted_ports();
                    if let Some(pos) = sorted.iter().position(|e| std::ptr::eq(*e, *entry)) {
                        self.selected = pos;
                    }
                }
            }
            ViewMode::Local => {
                let filtered = self.filtered_local_ports();
                if let Some(entry) = filtered.get(self.search_selected) {
                    let sorted = self.sorted_local_ports();
                    if let Some(pos) = sorted.iter().position(|e| std::ptr::eq(*e, *entry)) {
                        self.local_selected = pos;
                    }
                }
            }
        }
        self.search_query.clear();
        self.search_selected = 0;
        self.input_mode = InputMode::Normal;
    }

    /// Exit search on Esc: restore original cursor, clear search.
    pub fn exit_search_cancel(&mut self) {
        self.selected = self.pre_search_selected;
        self.local_selected = self.pre_search_local_selected;
        self.search_query.clear();
        self.search_selected = 0;
        self.input_mode = InputMode::Normal;
    }

    /// Return remote ports filtered by the current search query.
    pub fn filtered_ports(&self) -> Vec<&PortEntry> {
        let sorted = self.sorted_ports();
        if self.search_query.is_empty() {
            return sorted;
        }
        sorted
            .into_iter()
            .filter(|e| matches_search(&self.search_query, &e.discovered))
            .collect()
    }

    /// Return local ports filtered by the current search query.
    pub fn filtered_local_ports(&self) -> Vec<&DiscoveredPort> {
        let sorted = self.sorted_local_ports();
        if self.search_query.is_empty() {
            return sorted;
        }
        sorted
            .into_iter()
            .filter(|p| matches_search(&self.search_query, p))
            .collect()
    }

    pub fn search_move_up(&mut self) {
        if self.search_selected > 0 {
            self.search_selected -= 1;
        }
    }

    pub fn search_move_down(&mut self) {
        let len = match self.view_mode {
            ViewMode::Remote => self.filtered_ports().len(),
            ViewMode::Local => self.filtered_local_ports().len(),
        };
        if self.search_selected + 1 < len {
            self.search_selected += 1;
        }
    }

    /// Clamp search_selected to filtered results length.
    pub fn clamp_search_selected(&mut self) {
        let len = match self.view_mode {
            ViewMode::Remote => self.filtered_ports().len(),
            ViewMode::Local => self.filtered_local_ports().len(),
        };
        if len == 0 {
            self.search_selected = 0;
        } else if self.search_selected >= len {
            self.search_selected = len - 1;
        }
    }
}

fn matches_search(query: &str, port: &DiscoveredPort) -> bool {
    let query = query.to_lowercase();
    if port.port.to_string().contains(&query) {
        return true;
    }
    if port.bind_address.to_lowercase().contains(&query) {
        return true;
    }
    if let Some(ref name) = port.process_name {
        if name.to_lowercase().contains(&query) {
            return true;
        }
    }
    if let Some(pid) = port.pid {
        if pid.to_string().contains(&query) {
            return true;
        }
    }
    false
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
    fn test_update_ports_clears_error_state() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        state.set_forward_error(0, "conflict".to_string());
        state.update_ports(vec![make_port(8080, "nginx")]);
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
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

    // ---- SortState tests ----

    #[test]
    fn test_sort_state_new() {
        let sort = SortState::new();
        assert_eq!(sort.column, 0);
        assert_eq!(sort.active, None);
    }

    #[test]
    fn test_sort_move_left_right() {
        let mut sort = SortState::new();
        sort.move_right(5);
        assert_eq!(sort.column, 1);
        sort.move_right(5);
        assert_eq!(sort.column, 2);
        sort.move_left();
        assert_eq!(sort.column, 1);
    }

    #[test]
    fn test_sort_move_left_at_zero() {
        let mut sort = SortState::new();
        sort.move_left();
        assert_eq!(sort.column, 0);
    }

    #[test]
    fn test_sort_move_right_at_max() {
        let mut sort = SortState::new();
        sort.column = 4;
        sort.move_right(5);
        assert_eq!(sort.column, 4);
    }

    #[test]
    fn test_sort_toggle_ascending_descending_none() {
        let mut sort = SortState::new();
        sort.column = 1;

        // First toggle: ascending
        sort.toggle_sort();
        assert_eq!(sort.active, Some((1, SortOrder::Ascending)));

        // Second toggle: descending
        sort.toggle_sort();
        assert_eq!(sort.active, Some((1, SortOrder::Descending)));

        // Third toggle: none (reset)
        sort.toggle_sort();
        assert_eq!(sort.active, None);
    }

    #[test]
    fn test_sort_toggle_different_column() {
        let mut sort = SortState::new();
        sort.column = 1;
        sort.toggle_sort();
        assert_eq!(sort.active, Some((1, SortOrder::Ascending)));

        // Move to different column and toggle — starts fresh ascending
        sort.column = 3;
        sort.toggle_sort();
        assert_eq!(sort.active, Some((3, SortOrder::Ascending)));
    }

    #[test]
    fn test_sort_reset() {
        let mut sort = SortState::new();
        sort.column = 2;
        sort.toggle_sort();
        assert_eq!(sort.active, Some((2, SortOrder::Ascending)));
        sort.reset();
        assert_eq!(sort.active, None);
    }

    #[test]
    fn test_sorted_ports_default_order() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        let sorted = state.sorted_ports();
        // No active sort — original order preserved
        assert_eq!(sorted[0].discovered.port, 8080);
        assert_eq!(sorted[1].discovered.port, 3000);
    }

    #[test]
    fn test_sorted_ports_by_port_ascending() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.sort.active = Some((1, SortOrder::Ascending));
        let sorted = state.sorted_ports();
        assert_eq!(sorted[0].discovered.port, 3000);
        assert_eq!(sorted[1].discovered.port, 5000);
        assert_eq!(sorted[2].discovered.port, 8080);
    }

    #[test]
    fn test_sorted_ports_by_port_descending() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.sort.active = Some((1, SortOrder::Descending));
        let sorted = state.sorted_ports();
        assert_eq!(sorted[0].discovered.port, 8080);
        assert_eq!(sorted[1].discovered.port, 5000);
        assert_eq!(sorted[2].discovered.port, 3000);
    }

    #[test]
    fn test_sorted_ports_by_process_name() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "apache"),
            make_port(5000, "Zed"),
        ]);
        state.sort.active = Some((3, SortOrder::Ascending));
        let sorted = state.sorted_ports();
        assert_eq!(sorted[0].discovered.process_name.as_deref(), Some("apache"));
        assert_eq!(sorted[1].discovered.process_name.as_deref(), Some("nginx"));
        assert_eq!(sorted[2].discovered.process_name.as_deref(), Some("Zed"));
    }

    #[test]
    fn test_sorted_local_ports_by_port() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.sort.active = Some((1, SortOrder::Ascending));
        let sorted = state.sorted_local_ports();
        assert_eq!(sorted[0].port, 3000);
        assert_eq!(sorted[1].port, 8080);
    }

    #[test]
    fn test_sorted_local_ports_by_process() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(3000, "zsh"), make_port(8080, "apache")]);
        state.sort.active = Some((2, SortOrder::Ascending));
        let sorted = state.sorted_local_ports();
        assert_eq!(sorted[0].process_name.as_deref(), Some("apache"));
        assert_eq!(sorted[1].process_name.as_deref(), Some("zsh"));
    }

    #[test]
    fn test_app_state_defaults_with_sort() {
        let state = AppState::new("host".to_string());
        assert_eq!(state.sort.column, 0);
        assert_eq!(state.sort.active, None);
    }

    // ---- Search tests ----

    #[test]
    fn test_matches_search_by_port() {
        let port = make_port(8080, "nginx");
        assert!(matches_search("8080", &port));
        assert!(matches_search("808", &port));
        assert!(!matches_search("3000", &port));
    }

    #[test]
    fn test_matches_search_by_process_name() {
        let port = make_port(8080, "nginx");
        assert!(matches_search("nginx", &port));
        assert!(matches_search("NGINX", &port));
        assert!(matches_search("ngi", &port));
        assert!(!matches_search("apache", &port));
    }

    #[test]
    fn test_matches_search_by_bind_address() {
        let port = make_port(8080, "nginx");
        assert!(matches_search("0.0.0.0", &port));
        assert!(matches_search("0.0", &port));
    }

    #[test]
    fn test_matches_search_by_pid() {
        let port = DiscoveredPort {
            port: 8080,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some("nginx".to_string()),
            pid: Some(12345),
        };
        assert!(matches_search("12345", &port));
        assert!(matches_search("123", &port));
    }

    #[test]
    fn test_matches_search_no_process_name() {
        let port = DiscoveredPort {
            port: 22,
            bind_address: "0.0.0.0".to_string(),
            process_name: None,
            pid: None,
        };
        assert!(matches_search("22", &port));
        assert!(!matches_search("nginx", &port));
    }

    #[test]
    fn test_enter_search_saves_cursor() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.selected = 1;
        state.enter_search();
        assert_eq!(state.input_mode, InputMode::Search);
        assert_eq!(state.search_query, "");
        assert_eq!(state.search_selected, 0);
        assert_eq!(state.pre_search_selected, 1);
    }

    #[test]
    fn test_exit_search_cancel_restores_cursor() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.selected = 1;
        state.enter_search();
        state.search_query = "8080".to_string();
        state.exit_search_cancel();
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.selected, 1); // restored
        assert_eq!(state.search_query, "");
    }

    #[test]
    fn test_filtered_ports_empty_query_returns_all() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.search_query.clear();
        assert_eq!(state.filtered_ports().len(), 2);
    }

    #[test]
    fn test_filtered_ports_by_port_number() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.search_query = "8080".to_string();
        let filtered = state.filtered_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].discovered.port, 8080);
    }

    #[test]
    fn test_filtered_ports_by_process_name() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "nginx-proxy"),
        ]);
        state.search_query = "nginx".to_string();
        let filtered = state.filtered_ports();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filtered_ports_case_insensitive() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "Nginx"), make_port(3000, "node")]);
        state.search_query = "nginx".to_string();
        let filtered = state.filtered_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].discovered.port, 8080);
    }

    #[test]
    fn test_filtered_ports_no_matches() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.search_query = "zzz".to_string();
        assert!(state.filtered_ports().is_empty());
    }

    #[test]
    fn test_filtered_local_ports() {
        let mut state = AppState::new("host".to_string());
        state.update_local_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.search_query = "node".to_string();
        let filtered = state.filtered_local_ports();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].port, 3000);
    }

    #[test]
    fn test_exit_search_confirm_moves_cursor_remote() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.selected = 0;
        state.enter_search();
        state.search_query = "node".to_string();
        // Filtered list has one entry: 3000/node, which is index 1 in the full sorted list
        state.search_selected = 0;
        state.exit_search_confirm();
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.selected, 1); // moved to node's position in full list
    }

    #[test]
    fn test_exit_search_confirm_no_matches_keeps_cursor() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.selected = 0;
        state.enter_search();
        state.search_query = "zzz".to_string();
        state.exit_search_confirm();
        // No matches, selected unchanged from pre-search (confirm doesn't find entry)
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_search_move_up_down() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.enter_search();
        // Empty query matches all 3
        state.search_move_down();
        assert_eq!(state.search_selected, 1);
        state.search_move_down();
        assert_eq!(state.search_selected, 2);
        state.search_move_down();
        assert_eq!(state.search_selected, 2); // clamped
        state.search_move_up();
        assert_eq!(state.search_selected, 1);
        state.search_move_up();
        assert_eq!(state.search_selected, 0);
        state.search_move_up();
        assert_eq!(state.search_selected, 0); // clamped
    }

    #[test]
    fn test_clamp_search_selected() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.enter_search();
        state.search_selected = 2;
        // Now filter to one result
        state.search_query = "8080".to_string();
        state.clamp_search_selected();
        assert_eq!(state.search_selected, 0);
    }

    #[test]
    fn test_exit_search_confirm_local() {
        let mut state = AppState::new("host".to_string());
        state.view_mode = ViewMode::Local;
        state.update_local_ports(vec![
            make_port(8080, "nginx"),
            make_port(3000, "node"),
            make_port(5000, "python"),
        ]);
        state.local_selected = 0;
        state.enter_search();
        state.search_query = "python".to_string();
        state.search_selected = 0;
        state.exit_search_confirm();
        assert_eq!(state.local_selected, 2); // python is at index 2
    }
}
