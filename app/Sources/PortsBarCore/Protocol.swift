import Foundation

// Swift mirror of the Rust wire protocol in src/protocol/{ids,error,message}.rs.
// The Rust types are the source of truth; this file is kept in lockstep via the
// golden-fixture drift test. All types are Sendable and Codable. The
// internally-tagged enums hand-write Codable to read the serde discriminator.

// MARK: - Identifiers
//
// Rust newtypes with #[serde(transparent)] serialize as bare scalars.

/// Stable identifier for a port-forward. Bare number on the wire.
public struct ForwardId: Codable, Sendable, Hashable {
    public var value: UInt64
    public init(_ value: UInt64) { self.value = value }

    public init(from decoder: any Decoder) throws {
        value = try decoder.singleValueContainer().decode(UInt64.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(value)
    }
}

/// A TCP port number. Bare number on the wire.
public struct Port: Codable, Sendable, Hashable {
    public var value: UInt16
    public init(_ value: UInt16) { self.value = value }

    public init(from decoder: any Decoder) throws {
        value = try decoder.singleValueContainer().decode(UInt16.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(value)
    }
}

// MARK: - ProtocolError
//
// src/protocol/error.rs: internally tagged with key "kind", snake_case.

public enum ProtocolError: Codable, Sendable, Hashable, Error {
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

// MARK: - ConnStatus
//
// src/protocol/message.rs: bare snake_case strings.

public enum ConnStatus: String, Codable, Sendable, Hashable {
    case disconnected
    case connecting
    case connected
    case error
}

// MARK: - ForwardState
//
// src/protocol/message.rs: internally tagged with key "state", snake_case.

public enum ForwardState: Codable, Sendable, Hashable {
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

// MARK: - PortEntry
//
// src/protocol/message.rs: daemon OMITS nil optionals (process, pid).
// decodeIfPresent treats missing key and explicit null alike as nil.

public struct PortEntry: Codable, Sendable, Hashable {
    public var remotePort: Port
    public var process: String?
    public var pid: UInt32?
    public var forward: ForwardState

    public init(
        remotePort: Port,
        process: String? = nil,
        pid: UInt32? = nil,
        forward: ForwardState
    ) {
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
        // Daemon-side semantics: omit nil optionals.
        try c.encodeIfPresent(process, forKey: .process)
        try c.encodeIfPresent(pid, forKey: .pid)
        try c.encode(forward, forKey: .forward)
    }
}

// MARK: - PortsState
//
// src/protocol/message.rs: host is nullable; status_detail omitted when nil.

public struct PortsState: Codable, Sendable, Hashable {
    public var host: String?
    public var status: ConnStatus
    public var statusDetail: String?
    public var ports: [PortEntry]

    public init(host: String?, status: ConnStatus, statusDetail: String? = nil, ports: [PortEntry]) {
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
        host = try c.decodeIfPresent(String.self, forKey: .host)
        status = try c.decode(ConnStatus.self, forKey: .status)
        statusDetail = try c.decodeIfPresent(String.self, forKey: .statusDetail)
        ports = try c.decode([PortEntry].self, forKey: .ports)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        // host is a plain Option<String> in Rust (no skip): emit null when nil.
        try c.encode(host, forKey: .host)
        try c.encode(status, forKey: .status)
        // status_detail is skip_serializing_if = None: omit when nil.
        try c.encodeIfPresent(statusDetail, forKey: .statusDetail)
        try c.encode(ports, forKey: .ports)
    }
}

// MARK: - DaemonEvent
//
// src/protocol/message.rs: internally tagged with key "event", snake_case.

public enum DaemonEvent: Codable, Sendable, Hashable {
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

// MARK: - DaemonMessage
//
// src/protocol/message.rs: internally tagged with key "type", snake_case.
// State(PortsState) and Event(DaemonEvent) are newtype variants whose fields
// sit inline alongside "type". Ack OMITS nil error/hosts.

public enum DaemonMessage: Codable, Sendable, Hashable {
    case state(PortsState)
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
            self = .state(try PortsState(from: decoder))
        case "ack":
            let c = try decoder.container(keyedBy: AckKeys.self)
            self = .ack(
                id: try c.decode(UInt64.self, forKey: .id),
                error: try c.decodeIfPresent(ProtocolError.self, forKey: .error),
                hosts: try c.decodeIfPresent([String].self, forKey: .hosts)
            )
        case "event":
            self = .event(try DaemonEvent(from: decoder))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: TypeKey.type, in: tc,
                debugDescription: "unknown DaemonMessage type: \(type)"
            )
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var tc = encoder.container(keyedBy: TypeKey.self)
        switch self {
        case .state(let snapshot):
            try tc.encode("state", forKey: .type)
            try snapshot.encode(to: encoder)
        case .ack(let id, let error, let hosts):
            try tc.encode("ack", forKey: .type)
            var c = encoder.container(keyedBy: AckKeys.self)
            try c.encode(id, forKey: .id)
            try c.encodeIfPresent(error, forKey: .error)
            try c.encodeIfPresent(hosts, forKey: .hosts)
        case .event(let event):
            try tc.encode("event", forKey: .type)
            try event.encode(to: encoder)
        }
    }
}

// MARK: - RequestBody
//
// src/protocol/message.rs: internally tagged with key "type", snake_case.
// StartForward.local_port and SendFile.remote_path have no skip_serializing_if,
// so on the request side they serialize as EXPLICIT null when nil. The decoder
// still treats a missing key OR null as nil via decodeIfPresent.

public enum RequestBody: Codable, Sendable, Hashable {
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

    private enum CodingKeys: String, CodingKey {
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
            // Request side: emit explicit null when nil (no skip in Rust).
            try c.encode(localPort, forKey: .localPort)
        case .stopForward(let remotePort):
            try c.encode("stop_forward", forKey: .type)
            try c.encode(remotePort, forKey: .remotePort)
        case .sendFile(let localPath, let remotePath):
            try c.encode("send_file", forKey: .type)
            try c.encode(localPath, forKey: .localPath)
            // Request side: emit explicit null when nil.
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

// MARK: - Request
//
// src/protocol/message.rs Request: { id, ...flattened body }.

public struct Request: Codable, Sendable, Hashable {
    public var id: UInt64
    public var body: RequestBody

    public init(id: UInt64, body: RequestBody) {
        self.id = id
        self.body = body
    }

    private enum IdKey: String, CodingKey {
        case id
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: IdKey.self)
        id = try c.decode(UInt64.self, forKey: .id)
        body = try RequestBody(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: IdKey.self)
        try c.encode(id, forKey: .id)
        try body.encode(to: encoder)
    }
}
