import Foundation

// MARK: - Identifiers
//
// These mirror the Rust `#[serde(transparent)]` newtypes in src/protocol/ids.rs.
// They serialize as bare scalars (numbers / strings), not objects.

/// Unique identifier for a forward, assigned by the daemon. Bare number on the wire.
public struct ForwardId: Codable, Sendable, Hashable {
    public var value: UInt64
    public init(_ value: UInt64) { self.value = value }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        value = try c.decode(UInt64.self)
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
        let c = try decoder.singleValueContainer()
        value = try c.decode(UInt16.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(value)
    }
}

/// SSH host alias. Bare string on the wire.
public struct HostAlias: Codable, Sendable, Hashable {
    public var value: String
    public init(_ value: String) { self.value = value }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        value = try c.decode(String.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(value)
    }
}

// MARK: - ProtocolError
//
// Mirrors src/protocol/error.rs: internally tagged with key "kind", snake_case.

public enum ProtocolError: Codable, Sendable, Hashable, Error {
    case hostNotFound(alias: String)
    case connectionFailed(reason: String)
    case ssh(message: String)
    case bindFailed(port: UInt16, reason: String)
    case forwardNotFound(id: UInt64)
    case notConnected
    case invalidRequest(reason: String)
    case io(message: String)
    case timeout
    case `internal`(message: String)

    private enum CodingKeys: String, CodingKey {
        case kind
        case alias
        case reason
        case message
        case port
        case id
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "host_not_found":
            self = .hostNotFound(alias: try c.decode(String.self, forKey: .alias))
        case "connection_failed":
            self = .connectionFailed(reason: try c.decode(String.self, forKey: .reason))
        case "ssh":
            self = .ssh(message: try c.decode(String.self, forKey: .message))
        case "bind_failed":
            self = .bindFailed(
                port: try c.decode(UInt16.self, forKey: .port),
                reason: try c.decode(String.self, forKey: .reason)
            )
        case "forward_not_found":
            self = .forwardNotFound(id: try c.decode(UInt64.self, forKey: .id))
        case "not_connected":
            self = .notConnected
        case "invalid_request":
            self = .invalidRequest(reason: try c.decode(String.self, forKey: .reason))
        case "io":
            self = .io(message: try c.decode(String.self, forKey: .message))
        case "timeout":
            self = .timeout
        case "internal":
            self = .`internal`(message: try c.decode(String.self, forKey: .message))
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
        case .hostNotFound(let alias):
            try c.encode("host_not_found", forKey: .kind)
            try c.encode(alias, forKey: .alias)
        case .connectionFailed(let reason):
            try c.encode("connection_failed", forKey: .kind)
            try c.encode(reason, forKey: .reason)
        case .ssh(let message):
            try c.encode("ssh", forKey: .kind)
            try c.encode(message, forKey: .message)
        case .bindFailed(let port, let reason):
            try c.encode("bind_failed", forKey: .kind)
            try c.encode(port, forKey: .port)
            try c.encode(reason, forKey: .reason)
        case .forwardNotFound(let id):
            try c.encode("forward_not_found", forKey: .kind)
            try c.encode(id, forKey: .id)
        case .notConnected:
            try c.encode("not_connected", forKey: .kind)
        case .invalidRequest(let reason):
            try c.encode("invalid_request", forKey: .kind)
            try c.encode(reason, forKey: .reason)
        case .io(let message):
            try c.encode("io", forKey: .kind)
            try c.encode(message, forKey: .message)
        case .timeout:
            try c.encode("timeout", forKey: .kind)
        case .`internal`(let message):
            try c.encode("internal", forKey: .kind)
            try c.encode(message, forKey: .message)
        }
    }
}

// MARK: - ConnStatus
//
// Mirrors src/protocol/message.rs: bare snake_case strings.

public enum ConnStatus: String, Codable, Sendable, Hashable {
    case disconnected
    case connecting
    case connected
    case error
}

// MARK: - ForwardState
//
// Mirrors src/protocol/message.rs: internally tagged with key "state", snake_case.

public enum ForwardState: Codable, Sendable, Hashable {
    case idle
    case forwarding(localPort: Port)
    case error(message: String)

    private enum CodingKeys: String, CodingKey {
        case state
        case localPort = "local_port"
        case message
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
            self = .error(message: try c.decode(String.self, forKey: .message))
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
        case .error(let message):
            try c.encode("error", forKey: .state)
            try c.encode(message, forKey: .message)
        }
    }
}

// MARK: - PortForward
//
// Mirrors src/protocol/message.rs PortForward. Daemon OMITS nil optionals;
// decodeIfPresent treats both missing keys and explicit null as nil.

public struct PortForward: Codable, Sendable, Hashable {
    public var id: ForwardId
    public var remotePort: Port
    public var status: ForwardState
    public var statusDetail: String?
    public var process: String?
    public var pid: UInt32?

    public init(
        id: ForwardId,
        remotePort: Port,
        status: ForwardState,
        statusDetail: String? = nil,
        process: String? = nil,
        pid: UInt32? = nil
    ) {
        self.id = id
        self.remotePort = remotePort
        self.status = status
        self.statusDetail = statusDetail
        self.process = process
        self.pid = pid
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case remotePort = "remote_port"
        case status
        case statusDetail = "status_detail"
        case process
        case pid
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(ForwardId.self, forKey: .id)
        remotePort = try c.decode(Port.self, forKey: .remotePort)
        status = try c.decode(ForwardState.self, forKey: .status)
        statusDetail = try c.decodeIfPresent(String.self, forKey: .statusDetail)
        process = try c.decodeIfPresent(String.self, forKey: .process)
        pid = try c.decodeIfPresent(UInt32.self, forKey: .pid)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(remotePort, forKey: .remotePort)
        try c.encode(status, forKey: .status)
        // Daemon-side semantics: omit nil optionals.
        try c.encodeIfPresent(statusDetail, forKey: .statusDetail)
        try c.encodeIfPresent(process, forKey: .process)
        try c.encodeIfPresent(pid, forKey: .pid)
    }
}

// MARK: - StateSnapshot

public struct StateSnapshot: Codable, Sendable, Hashable {
    public var host: HostAlias?
    public var connStatus: ConnStatus
    public var forwards: [PortForward]

    public init(host: HostAlias?, connStatus: ConnStatus, forwards: [PortForward]) {
        self.host = host
        self.connStatus = connStatus
        self.forwards = forwards
    }

    private enum CodingKeys: String, CodingKey {
        case host
        case connStatus = "conn_status"
        case forwards
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        host = try c.decodeIfPresent(HostAlias.self, forKey: .host)
        connStatus = try c.decode(ConnStatus.self, forKey: .connStatus)
        forwards = try c.decode([PortForward].self, forKey: .forwards)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(host, forKey: .host)
        try c.encode(connStatus, forKey: .connStatus)
        try c.encode(forwards, forKey: .forwards)
    }
}

// MARK: - DaemonConfig

public struct DaemonConfig: Codable, Sendable, Hashable {
    public var autoReconnect: Bool
    public var autoRefreshSecs: UInt64
    public var openBrowserOnForward: Bool

    public init(autoReconnect: Bool, autoRefreshSecs: UInt64, openBrowserOnForward: Bool) {
        self.autoReconnect = autoReconnect
        self.autoRefreshSecs = autoRefreshSecs
        self.openBrowserOnForward = openBrowserOnForward
    }

    private enum CodingKeys: String, CodingKey {
        case autoReconnect = "auto_reconnect"
        case autoRefreshSecs = "auto_refresh_secs"
        case openBrowserOnForward = "open_browser_on_forward"
    }
}

// MARK: - DaemonEvent
//
// Mirrors src/protocol/message.rs: internally tagged with key "event", snake_case.

public enum DaemonEvent: Codable, Sendable, Hashable {
    case connStatusChanged(connStatus: ConnStatus)
    case forwardAdded(forward: PortForward)
    case forwardRemoved(forwardId: ForwardId)
    case fileTransfer(localPath: String, bytesTransferred: UInt64, totalBytes: UInt64?, done: Bool)
    case log(level: String, message: String)

    private enum CodingKeys: String, CodingKey {
        case event
        case connStatus = "conn_status"
        case forward
        case forwardId = "forward_id"
        case localPath = "local_path"
        case bytesTransferred = "bytes_transferred"
        case totalBytes = "total_bytes"
        case done
        case level
        case message
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let event = try c.decode(String.self, forKey: .event)
        switch event {
        case "conn_status_changed":
            self = .connStatusChanged(connStatus: try c.decode(ConnStatus.self, forKey: .connStatus))
        case "forward_added":
            self = .forwardAdded(forward: try c.decode(PortForward.self, forKey: .forward))
        case "forward_removed":
            self = .forwardRemoved(forwardId: try c.decode(ForwardId.self, forKey: .forwardId))
        case "file_transfer":
            self = .fileTransfer(
                localPath: try c.decode(String.self, forKey: .localPath),
                bytesTransferred: try c.decode(UInt64.self, forKey: .bytesTransferred),
                totalBytes: try c.decodeIfPresent(UInt64.self, forKey: .totalBytes),
                done: try c.decode(Bool.self, forKey: .done)
            )
        case "log":
            self = .log(
                level: try c.decode(String.self, forKey: .level),
                message: try c.decode(String.self, forKey: .message)
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
        case .connStatusChanged(let connStatus):
            try c.encode("conn_status_changed", forKey: .event)
            try c.encode(connStatus, forKey: .connStatus)
        case .forwardAdded(let forward):
            try c.encode("forward_added", forKey: .event)
            try c.encode(forward, forKey: .forward)
        case .forwardRemoved(let forwardId):
            try c.encode("forward_removed", forKey: .event)
            try c.encode(forwardId, forKey: .forwardId)
        case .fileTransfer(let localPath, let bytesTransferred, let totalBytes, let done):
            try c.encode("file_transfer", forKey: .event)
            try c.encode(localPath, forKey: .localPath)
            try c.encode(bytesTransferred, forKey: .bytesTransferred)
            try c.encodeIfPresent(totalBytes, forKey: .totalBytes)
            try c.encode(done, forKey: .done)
        case .log(let level, let message):
            try c.encode("log", forKey: .event)
            try c.encode(level, forKey: .level)
            try c.encode(message, forKey: .message)
        }
    }
}

// MARK: - DaemonMessage
//
// Mirrors src/protocol/message.rs: internally tagged with key "type", snake_case.
// State wraps StateSnapshot's fields inline (untagged newtype variant ->
// fields are flattened alongside "type"). Ack OMITS nil optionals.

public enum DaemonMessage: Codable, Sendable, Hashable {
    case state(StateSnapshot)
    case ack(id: UInt64, error: ProtocolError?, hosts: [HostAlias]?)
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
            // State(StateSnapshot) is a newtype variant: its fields sit inline
            // next to "type", so decode the snapshot from the same container.
            self = .state(try StateSnapshot(from: decoder))
        case "ack":
            let c = try decoder.container(keyedBy: AckKeys.self)
            self = .ack(
                id: try c.decode(UInt64.self, forKey: .id),
                error: try c.decodeIfPresent(ProtocolError.self, forKey: .error),
                hosts: try c.decodeIfPresent([HostAlias].self, forKey: .hosts)
            )
        case "event":
            // Event(DaemonEvent) is a newtype variant: the event fields are inline.
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
// Mirrors src/protocol/message.rs: internally tagged with key "type", snake_case.
// ASYMMETRY: on the request side, StartForward.local_port and
// SendFile.remote_path serialize as EXPLICIT null when absent (no skip), but
// decoding must still accept a missing key or null (decodeIfPresent).

public enum RequestBody: Codable, Sendable, Hashable {
    case connect(host: HostAlias)
    case disconnect
    case startForward(remotePort: Port, localPort: Port?)
    case stopForward(remotePort: Port)
    case listHosts
    case sendFile(localPath: String, remotePath: String?)
    case setConfig(config: DaemonConfig)

    private enum CodingKeys: String, CodingKey {
        case type
        case host
        case remotePort = "remote_port"
        case localPort = "local_port"
        case localPath = "local_path"
        case remotePath = "remote_path"
        case config
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "connect":
            self = .connect(host: try c.decode(HostAlias.self, forKey: .host))
        case "disconnect":
            self = .disconnect
        case "start_forward":
            self = .startForward(
                remotePort: try c.decode(Port.self, forKey: .remotePort),
                localPort: try c.decodeIfPresent(Port.self, forKey: .localPort)
            )
        case "stop_forward":
            self = .stopForward(remotePort: try c.decode(Port.self, forKey: .remotePort))
        case "list_hosts":
            self = .listHosts
        case "send_file":
            self = .sendFile(
                localPath: try c.decode(String.self, forKey: .localPath),
                remotePath: try c.decodeIfPresent(String.self, forKey: .remotePath)
            )
        case "set_config":
            self = .setConfig(config: try c.decode(DaemonConfig.self, forKey: .config))
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
        case .connect(let host):
            try c.encode("connect", forKey: .type)
            try c.encode(host, forKey: .host)
        case .disconnect:
            try c.encode("disconnect", forKey: .type)
        case .startForward(let remotePort, let localPort):
            try c.encode("start_forward", forKey: .type)
            try c.encode(remotePort, forKey: .remotePort)
            // Request side: emit explicit null when absent (Rust #[serde(default)]
            // accepts both null and omitted; we emit null to match the fixture).
            try c.encode(localPort, forKey: .localPort)
        case .stopForward(let remotePort):
            try c.encode("stop_forward", forKey: .type)
            try c.encode(remotePort, forKey: .remotePort)
        case .listHosts:
            try c.encode("list_hosts", forKey: .type)
        case .sendFile(let localPath, let remotePath):
            try c.encode("send_file", forKey: .type)
            try c.encode(localPath, forKey: .localPath)
            // Request side: emit explicit null when absent.
            try c.encode(remotePath, forKey: .remotePath)
        case .setConfig(let config):
            try c.encode("set_config", forKey: .type)
            try c.encode(config, forKey: .config)
        }
    }
}

// MARK: - Request
//
// Mirrors src/protocol/message.rs Request: { id, ...flattened body }.

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
