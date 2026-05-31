import Foundation

// MARK: - Newtype identifiers
//
// `ForwardId` and `Port` serialize transparently as bare JSON numbers, mirroring
// the `#[serde(transparent)]` Rust newtypes in `src/protocol/ids.rs`. They are
// modeled as thin `RawRepresentable`-style wrappers that encode/decode to a
// single value container so the wire stays a bare integer.

/// Stable identifier for a single port-forward managed by the daemon.
///
/// Mirrors Rust `ForwardId(pub u64)`; serializes as a bare JSON number.
public struct ForwardId: Codable, Hashable, Sendable {
    public var value: UInt64
    public init(_ value: UInt64) { self.value = value }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        value = try container.decode(UInt64.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value)
    }
}

/// A TCP port number (local or remote).
///
/// Mirrors Rust `Port(pub u16)`; serializes as a bare JSON number.
public struct Port: Codable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public var value: UInt16
    public init(_ value: UInt16) { self.value = value }
    public init(integerLiteral value: UInt16) { self.value = value }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        value = try container.decode(UInt16.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value)
    }
}

// MARK: - Wire error
//
// Mirrors Rust `ProtocolError` — internally tagged with key `kind`, snake_case.

/// The only error type that crosses the socket. Mirrors Rust `ProtocolError`.
public enum ProtocolError: Codable, Hashable, Sendable, Error {
    case notConnected
    case connectFailed(detail: String)
    case bindFailed(port: Port, detail: String)
    case unknownHost(alias: String)
    case sendFileFailed(detail: String)
    case badRequest(detail: String)

    private enum CodingKeys: String, CodingKey {
        case kind
        case detail
        case port
        case alias
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "not_connected":
            self = .notConnected
        case "connect_failed":
            self = .connectFailed(detail: try c.decode(String.self, forKey: .detail))
        case "bind_failed":
            self = .bindFailed(
                port: try c.decode(Port.self, forKey: .port),
                detail: try c.decode(String.self, forKey: .detail)
            )
        case "unknown_host":
            self = .unknownHost(alias: try c.decode(String.self, forKey: .alias))
        case "send_file_failed":
            self = .sendFileFailed(detail: try c.decode(String.self, forKey: .detail))
        case "bad_request":
            self = .badRequest(detail: try c.decode(String.self, forKey: .detail))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: c,
                debugDescription: "unknown ProtocolError kind: \(kind)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .notConnected:
            try c.encode("not_connected", forKey: .kind)
        case .connectFailed(let detail):
            try c.encode("connect_failed", forKey: .kind)
            try c.encode(detail, forKey: .detail)
        case .bindFailed(let port, let detail):
            try c.encode("bind_failed", forKey: .kind)
            try c.encode(port, forKey: .port)
            try c.encode(detail, forKey: .detail)
        case .unknownHost(let alias):
            try c.encode("unknown_host", forKey: .kind)
            try c.encode(alias, forKey: .alias)
        case .sendFileFailed(let detail):
            try c.encode("send_file_failed", forKey: .kind)
            try c.encode(detail, forKey: .detail)
        case .badRequest(let detail):
            try c.encode("bad_request", forKey: .kind)
            try c.encode(detail, forKey: .detail)
        }
    }
}

// MARK: - Connection status
//
// Mirrors Rust `ConnStatus` — bare snake_case strings.

/// The daemon's connection status. Mirrors Rust `ConnStatus`.
public enum ConnStatus: String, Codable, Hashable, Sendable {
    case disconnected
    case connecting
    case connected
    case error
}

// MARK: - Forward state
//
// Mirrors Rust `ForwardState` — internally tagged with key `state`, snake_case.

/// The forwarding state of a single port. Mirrors Rust `ForwardState`.
public enum ForwardState: Codable, Hashable, Sendable {
    case idle
    case forwarding(localPort: Port)
    case error(detail: String)

    private enum CodingKeys: String, CodingKey {
        case state
        case localPort = "local_port"
        case detail
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let state = try c.decode(String.self, forKey: .state)
        switch state {
        case "idle":
            self = .idle
        case "forwarding":
            self = .forwarding(localPort: try c.decode(Port.self, forKey: .localPort))
        case "error":
            self = .error(detail: try c.decode(String.self, forKey: .detail))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .state, in: c,
                debugDescription: "unknown ForwardState: \(state)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .idle:
            try c.encode("idle", forKey: .state)
        case .forwarding(let localPort):
            try c.encode("forwarding", forKey: .state)
            try c.encode(localPort, forKey: .localPort)
        case .error(let detail):
            try c.encode("error", forKey: .state)
            try c.encode(detail, forKey: .detail)
        }
    }
}

// MARK: - Port entry
//
// Mirrors Rust `PortEntry`. `process`/`pid` are OMITTED when nil on the
// daemon->app side; the decoder treats both missing and explicit null as nil.

/// A single port observed on the host and its forwarding state.
public struct PortEntry: Codable, Hashable, Sendable, Identifiable {
    public var remotePort: Port
    public var process: String?
    public var pid: UInt32?
    public var forward: ForwardState

    /// Stable identity for SwiftUI lists: the remote port is unique per host.
    public var id: UInt16 { remotePort.value }

    public init(remotePort: Port, process: String? = nil, pid: UInt32? = nil, forward: ForwardState) {
        self.remotePort = remotePort
        self.process = process
        self.pid = pid
        self.forward = forward
    }

    private enum CodingKeys: String, CodingKey {
        case remotePort = "remote_port"
        case process
        case pid
        case forward
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        remotePort = try c.decode(Port.self, forKey: .remotePort)
        process = try c.decodeIfPresent(String.self, forKey: .process)
        pid = try c.decodeIfPresent(UInt32.self, forKey: .pid)
        forward = try c.decode(ForwardState.self, forKey: .forward)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(remotePort, forKey: .remotePort)
        try c.encodeIfPresent(process, forKey: .process)
        try c.encodeIfPresent(pid, forKey: .pid)
        try c.encode(forward, forKey: .forward)
    }
}

// MARK: - State snapshot
//
// Mirrors Rust `StateSnapshot`. `host` is always present (may be null);
// `status_detail` is omitted when nil on the daemon->app side.

/// A complete snapshot of the daemon's connection and port state.
public struct StateSnapshot: Codable, Hashable, Sendable {
    public var host: String?
    public var status: ConnStatus
    public var statusDetail: String?
    public var ports: [PortEntry]

    public init(
        host: String? = nil,
        status: ConnStatus = .disconnected,
        statusDetail: String? = nil,
        ports: [PortEntry] = []
    ) {
        self.host = host
        self.status = status
        self.statusDetail = statusDetail
        self.ports = ports
    }

    private enum CodingKeys: String, CodingKey {
        case host
        case status
        case statusDetail = "status_detail"
        case ports
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // `host` is `Option<String>` and always serialized (may be explicit
        // null); decodeIfPresent handles both missing and null defensively.
        host = try c.decodeIfPresent(String.self, forKey: .host)
        status = try c.decode(ConnStatus.self, forKey: .status)
        statusDetail = try c.decodeIfPresent(String.self, forKey: .statusDetail)
        ports = try c.decode([PortEntry].self, forKey: .ports)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        // Mirror Rust: `host` is always present in the snapshot (possibly null).
        try c.encode(host, forKey: .host)
        try c.encode(status, forKey: .status)
        try c.encodeIfPresent(statusDetail, forKey: .statusDetail)
        try c.encode(ports, forKey: .ports)
    }
}

// MARK: - Daemon events
//
// Mirrors Rust `DaemonEvent` — internally tagged with key `event`, snake_case.

/// An asynchronous event emitted by the daemon. Mirrors Rust `DaemonEvent`.
public enum DaemonEvent: Codable, Hashable, Sendable {
    case fileTransfer(ok: Bool, detail: String)

    private enum CodingKeys: String, CodingKey {
        case event
        case ok
        case detail
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let event = try c.decode(String.self, forKey: .event)
        switch event {
        case "file_transfer":
            self = .fileTransfer(
                ok: try c.decode(Bool.self, forKey: .ok),
                detail: try c.decode(String.self, forKey: .detail)
            )
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .event, in: c,
                debugDescription: "unknown DaemonEvent: \(event)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .fileTransfer(let ok, let detail):
            try c.encode("file_transfer", forKey: .event)
            try c.encode(ok, forKey: .ok)
            try c.encode(detail, forKey: .detail)
        }
    }
}

// MARK: - Daemon messages (daemon -> app)
//
// Mirrors Rust `DaemonMessage` — internally tagged with key `type`, snake_case.
// `State` and `Event` flatten the inner payload alongside the discriminator.

/// A message sent from the daemon to the client. Mirrors Rust `DaemonMessage`.
public enum DaemonMessage: Codable, Hashable, Sendable {
    case state(StateSnapshot)
    case ack(id: UInt64, error: ProtocolError?, hosts: [String]?)
    case event(DaemonEvent)

    private enum TypeKey: String, CodingKey {
        case type
    }

    private enum AckKeys: String, CodingKey {
        case id
        case error
        case hosts
    }

    public init(from decoder: any Decoder) throws {
        let tc = try decoder.container(keyedBy: TypeKey.self)
        let type = try tc.decode(String.self, forKey: .type)
        switch type {
        case "state":
            // `State(StateSnapshot)` is `#[serde(tag = "type")]` on a newtype
            // variant, so the snapshot fields are flattened at this level.
            self = .state(try StateSnapshot(from: decoder))
        case "ack":
            let ac = try decoder.container(keyedBy: AckKeys.self)
            self = .ack(
                id: try ac.decode(UInt64.self, forKey: .id),
                // Daemon omits these when nil; treat missing and null as nil.
                error: try ac.decodeIfPresent(ProtocolError.self, forKey: .error),
                hosts: try ac.decodeIfPresent([String].self, forKey: .hosts)
            )
        case "event":
            // `Event(DaemonEvent)` flattens the event payload at this level.
            self = .event(try DaemonEvent(from: decoder))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: tc,
                debugDescription: "unknown DaemonMessage type: \(type)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .state(let snapshot):
            var tc = encoder.container(keyedBy: TypeKey.self)
            try tc.encode("state", forKey: .type)
            try snapshot.encode(to: encoder)
        case .ack(let id, let error, let hosts):
            var tc = encoder.container(keyedBy: TypeKey.self)
            try tc.encode("ack", forKey: .type)
            var ac = encoder.container(keyedBy: AckKeys.self)
            try ac.encode(id, forKey: .id)
            try ac.encodeIfPresent(error, forKey: .error)
            try ac.encodeIfPresent(hosts, forKey: .hosts)
        case .event(let event):
            var tc = encoder.container(keyedBy: TypeKey.self)
            try tc.encode("event", forKey: .type)
            try event.encode(to: encoder)
        }
    }
}

// MARK: - Requests (app -> daemon)
//
// Mirrors Rust `RequestBody` — internally tagged with key `type`, snake_case.

/// The action carried by a ``Request``. Mirrors Rust `RequestBody`.
public enum RequestBody: Codable, Hashable, Sendable {
    case setConfig(hostAlias: String, refreshSecs: UInt64, autoReconnect: Bool)
    case connect
    case disconnect
    case refresh
    case startForward(remotePort: Port, localPort: Port?)
    case stopForward(remotePort: Port)
    case sendFile(localPath: String, remotePath: String?)
    case listHosts
    case ping
    case shutdown

    fileprivate enum CodingKeys: String, CodingKey {
        case type
        case hostAlias = "host_alias"
        case refreshSecs = "refresh_secs"
        case autoReconnect = "auto_reconnect"
        case remotePort = "remote_port"
        case localPort = "local_port"
        case localPath = "local_path"
        case remotePath = "remote_path"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "set_config":
            self = .setConfig(
                hostAlias: try c.decode(String.self, forKey: .hostAlias),
                refreshSecs: try c.decode(UInt64.self, forKey: .refreshSecs),
                autoReconnect: try c.decode(Bool.self, forKey: .autoReconnect)
            )
        case "connect":
            self = .connect
        case "disconnect":
            self = .disconnect
        case "refresh":
            self = .refresh
        case "start_forward":
            self = .startForward(
                remotePort: try c.decode(Port.self, forKey: .remotePort),
                // The daemon serializes `local_port` as an explicit null when
                // absent; decodeIfPresent treats both missing and null as nil.
                localPort: try c.decodeIfPresent(Port.self, forKey: .localPort)
            )
        case "stop_forward":
            self = .stopForward(remotePort: try c.decode(Port.self, forKey: .remotePort))
        case "send_file":
            self = .sendFile(
                localPath: try c.decode(String.self, forKey: .localPath),
                remotePath: try c.decodeIfPresent(String.self, forKey: .remotePath)
            )
        case "list_hosts":
            self = .listHosts
        case "ping":
            self = .ping
        case "shutdown":
            self = .shutdown
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c,
                debugDescription: "unknown RequestBody type: \(type)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .setConfig(let hostAlias, let refreshSecs, let autoReconnect):
            try c.encode("set_config", forKey: .type)
            try c.encode(hostAlias, forKey: .hostAlias)
            try c.encode(refreshSecs, forKey: .refreshSecs)
            try c.encode(autoReconnect, forKey: .autoReconnect)
        case .connect:
            try c.encode("connect", forKey: .type)
        case .disconnect:
            try c.encode("disconnect", forKey: .type)
        case .refresh:
            try c.encode("refresh", forKey: .type)
        case .startForward(let remotePort, let localPort):
            try c.encode("start_forward", forKey: .type)
            try c.encode(remotePort, forKey: .remotePort)
            // CRITICAL ASYMMETRY: emit explicit null when absent (Rust does not
            // skip this field on the request side). Rust accepts null or omit.
            try c.encode(localPort, forKey: .localPort)
        case .stopForward(let remotePort):
            try c.encode("stop_forward", forKey: .type)
            try c.encode(remotePort, forKey: .remotePort)
        case .sendFile(let localPath, let remotePath):
            try c.encode("send_file", forKey: .type)
            try c.encode(localPath, forKey: .localPath)
            // Same asymmetry: emit explicit null when absent.
            try c.encode(remotePath, forKey: .remotePath)
        case .listHosts:
            try c.encode("list_hosts", forKey: .type)
        case .ping:
            try c.encode("ping", forKey: .type)
        case .shutdown:
            try c.encode("shutdown", forKey: .type)
        }
    }
}

/// A request sent from the client (Swift app) to the daemon.
///
/// Mirrors Rust `Request { id, #[serde(flatten)] body }`: the JSON object
/// carries `id` alongside the `type`-tagged body fields at the top level.
public struct Request: Codable, Hashable, Sendable {
    public var id: UInt64
    public var body: RequestBody

    public init(id: UInt64, body: RequestBody) {
        self.id = id
        self.body = body
    }

    private enum IDKey: String, CodingKey {
        case id
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: IDKey.self)
        id = try c.decode(UInt64.self, forKey: .id)
        body = try RequestBody(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: IDKey.self)
        try c.encode(id, forKey: .id)
        try body.encode(to: encoder)
    }
}
