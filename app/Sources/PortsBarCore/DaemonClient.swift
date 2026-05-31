import Foundation

// MARK: - NDJSON framing
//
// The daemon speaks NDJSON: one JSON object per line, '\n'-terminated.
// `NDJSONFramer` accumulates bytes and yields complete lines as decoded
// DaemonMessage values. Pure value type, fully unit-testable without a socket.

/// Splits a byte stream into newline-delimited JSON messages.
struct NDJSONFramer: Sendable {
    private var buffer = Data()
    private let decoder = JSONDecoder()

    init() {}

    /// Append received bytes and return every complete DaemonMessage now available.
    /// Incomplete trailing data is retained for the next call.
    mutating func push(_ chunk: Data) throws -> [DaemonMessage] {
        buffer.append(chunk)
        var messages: [DaemonMessage] = []
        let newline = UInt8(ascii: "\n")
        while let idx = buffer.firstIndex(of: newline) {
            let lineRange = buffer.startIndex..<idx
            let line = buffer.subdata(in: lineRange)
            // Drop the line plus its newline terminator.
            buffer.removeSubrange(buffer.startIndex...idx)
            // Skip blank lines (e.g. trailing newline pairs).
            let trimmed = line.filter { $0 != UInt8(ascii: "\r") && $0 != UInt8(ascii: " ") }
            if trimmed.isEmpty { continue }
            messages.append(try decoder.decode(DaemonMessage.self, from: line))
        }
        return messages
    }
}

/// Encodes a Request as a single NDJSON line (JSON + trailing '\n').
enum NDJSONEncoder {
    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.withoutEscapingSlashes]
        return e
    }()

    static func line(_ request: Request) throws -> Data {
        var data = try encoder.encode(request)
        data.append(UInt8(ascii: "\n"))
        return data
    }
}

// MARK: - Binary location

enum DaemonBinary {
    /// Locate the bundled `ports` binary (Contents/Resources/ports) with a dev
    /// fallback. Per AGENTS.md §8 we only ever spawn a binary under the app
    /// bundle in production; the dev fallback is for `swift run` workflows.
    static func locate(
        bundle: Bundle = .main,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        fileManager: FileManager = .default
    ) -> URL? {
        // 1. Bundled resource (production).
        if let resourceURL = bundle.resourceURL {
            let bundled = resourceURL.appendingPathComponent("ports")
            if fileManager.isExecutableFile(atPath: bundled.path) {
                return bundled
            }
        }
        // 2. Explicit dev override.
        if let override = environment["PORTS_DAEMON_BIN"],
           fileManager.isExecutableFile(atPath: override) {
            return URL(fileURLWithPath: override)
        }
        // 3. Dev fallback: a debug/release build next to the running executable.
        let exeDir = bundle.executableURL?.deletingLastPathComponent()
        for candidate in [exeDir?.appendingPathComponent("ports")].compactMap({ $0 }) {
            if fileManager.isExecutableFile(atPath: candidate.path) {
                return candidate
            }
        }
        return nil
    }
}

// MARK: - Socket path

enum DaemonSocket {
    /// Default socket path: $XDG_RUNTIME_DIR/ports.sock or
    /// ~/Library/Application Support/ports/ports.sock.
    static func defaultPath(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        fileManager: FileManager = .default
    ) -> String {
        if let runtime = environment["XDG_RUNTIME_DIR"], !runtime.isEmpty {
            return (runtime as NSString).appendingPathComponent("ports.sock")
        }
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first ?? URL(fileURLWithPath: NSHomeDirectory())
            .appendingPathComponent("Library/Application Support")
        let dir = appSupport.appendingPathComponent("ports", isDirectory: true)
        return dir.appendingPathComponent("ports.sock").path
    }
}

// MARK: - Errors

enum DaemonClientError: Error, Sendable {
    case binaryNotFound
    case socketCreateFailed(Int32)
    case socketConnectFailed(Int32)
    case notConnected
    case writeFailed(Int32)
}

// MARK: - DaemonClient
//
// Spawns and supervises the daemon, connects the Unix socket, and exposes an
// AsyncStream<DaemonMessage> from the read loop. async/await only; the blocking
// POSIX read is hopped to a detached task that feeds the stream continuation.

actor DaemonClient {
    private let binaryURL: URL
    private let socketPath: String
    private var process: Process?
    private var fd: Int32 = -1
    private var readTask: Task<Void, Never>?
    private var continuation: AsyncStream<DaemonMessage>.Continuation?

    init(binaryURL: URL, socketPath: String) {
        self.binaryURL = binaryURL
        self.socketPath = socketPath
    }

    /// Convenience initializer that locates the bundled binary and default socket.
    init() throws {
        guard let url = DaemonBinary.locate() else {
            throw DaemonClientError.binaryNotFound
        }
        self.binaryURL = url
        self.socketPath = DaemonSocket.defaultPath()
    }

    /// Spawn the daemon process if a socket isn't already live, connect to it,
    /// and start the read loop. Returns the message stream.
    func start() async throws -> AsyncStream<DaemonMessage> {
        if !socketIsLive() {
            // Remove a stale socket file (daemon died but left the inode behind)
            // so the freshly spawned daemon can bind the path cleanly.
            try? FileManager.default.removeItem(atPath: socketPath)
            try spawnDaemon()
        }
        try await connectSocket()
        return makeStream()
    }

    /// Send a request as an NDJSON line.
    func send(_ request: Request) throws {
        guard fd >= 0 else { throw DaemonClientError.notConnected }
        let data = try NDJSONEncoder.line(request)
        try writeAll(data)
    }

    /// Tear down: cancel the read loop, close the socket, terminate the daemon
    /// we spawned.
    func stop() {
        readTask?.cancel()
        readTask = nil
        continuation?.finish()
        continuation = nil
        if fd >= 0 {
            close(fd)
            fd = -1
        }
        if let process, process.isRunning {
            process.terminate()
        }
        process = nil
    }

    // MARK: - Internals

    private func spawnDaemon() throws {
        let proc = Process()
        proc.executableURL = binaryURL
        proc.arguments = ["daemon", "--socket", socketPath]
        try proc.run()
        process = proc
    }

    /// A socket is "live" only if a daemon is actually listening on it. Checking
    /// file existence alone is wrong: a daemon that died leaves a stale socket
    /// inode behind, and connecting to it fails with ECONNREFUSED. We probe with
    /// a real connect and close it immediately on success.
    private func socketIsLive() -> Bool {
        guard FileManager.default.fileExists(atPath: socketPath) else { return false }
        guard let probe = try? attemptConnect() else { return false }
        close(probe)
        return true
    }

    private func connectSocket() async throws {
        // Retry briefly while the freshly spawned daemon creates the socket.
        var lastErrno: Int32 = 0
        for attempt in 0..<50 {
            if let connected = try? attemptConnect() {
                fd = connected
                return
            }
            lastErrno = errno
            // Back off ~20ms between attempts using a non-blocking async sleep.
            if attempt < 49 {
                try? await Task.sleep(nanoseconds: 20_000_000)
            }
        }
        throw DaemonClientError.socketConnectFailed(lastErrno)
    }

    private func attemptConnect() throws -> Int32 {
        let sock = socket(AF_UNIX, SOCK_STREAM, 0)
        guard sock >= 0 else { throw DaemonClientError.socketCreateFailed(errno) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
        precondition(
            pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path),
            "socket path too long"
        )
        withUnsafeMutablePointer(to: &addr.sun_path) { dst in
            dst.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { ptr in
                for (i, b) in pathBytes.enumerated() { ptr[i] = b }
            }
        }
        let len = socklen_t(MemoryLayout<sockaddr_un>.size)
        let result = withUnsafePointer(to: &addr) { raw -> Int32 in
            raw.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                Foundation.connect(sock, sa, len)
            }
        }
        if result != 0 {
            let e = errno
            close(sock)
            throw DaemonClientError.socketConnectFailed(e)
        }
        return sock
    }

    private func makeStream() -> AsyncStream<DaemonMessage> {
        let socketFD = fd
        let (stream, cont) = AsyncStream<DaemonMessage>.makeStream()
        continuation = cont
        readTask = Task.detached {
            var framer = NDJSONFramer()
            let bufSize = 64 * 1024
            var buf = [UInt8](repeating: 0, count: bufSize)
            while !Task.isCancelled {
                let n = buf.withUnsafeMutableBytes { ptr -> Int in
                    read(socketFD, ptr.baseAddress, bufSize)
                }
                if n <= 0 { break } // EOF or error.
                let chunk = Data(buf[0..<n])
                if let messages = try? framer.push(chunk) {
                    for msg in messages { cont.yield(msg) }
                }
            }
            cont.finish()
        }
        return stream
    }

    private func writeAll(_ data: Data) throws {
        try data.withUnsafeBytes { raw in
            var offset = 0
            let base = raw.baseAddress!
            let total = raw.count
            while offset < total {
                let n = write(fd, base + offset, total - offset)
                if n <= 0 { throw DaemonClientError.writeFailed(errno) }
                offset += n
            }
        }
    }
}
