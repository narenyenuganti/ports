import Foundation

// MARK: - NDJSON framing (pure, testable)

/// Encodes and decodes the newline-delimited JSON wire framing.
///
/// Kept free of I/O so the framing rules can be unit-tested directly: a
/// ``Request`` encodes to a single line terminated by `\n`, and a byte stream
/// containing one or more concatenated JSON lines decodes back into messages.
public enum NDJSON {
    /// JSON-encode a value and append the framing newline.
    public static func encodeLine<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        var data = try encoder.encode(value)
        data.append(0x0A) // '\n'
        return data
    }

    /// Encode a ``Request`` into a single NDJSON line.
    public static func encodeRequest(_ request: Request) throws -> Data {
        try encodeLine(request)
    }
}

/// Buffers incoming bytes and yields complete NDJSON lines as they arrive.
///
/// A `Sendable` value type holding only `Data`; the daemon read loop feeds it
/// chunks and drains decoded ``DaemonMessage`` values. Splitting on `\n`
/// tolerates partial trailing lines (kept buffered until the next chunk).
public struct LineFramer: Sendable {
    private var buffer = Data()

    public init() {}

    /// Append a chunk and return every complete line (without the trailing
    /// newline) now available. Incomplete trailing data stays buffered.
    public mutating func push(_ chunk: Data) -> [Data] {
        buffer.append(chunk)
        var lines: [Data] = []
        while let newlineIndex = buffer.firstIndex(of: 0x0A) {
            let line = buffer[buffer.startIndex..<newlineIndex]
            if !line.isEmpty {
                lines.append(Data(line))
            }
            // Drop the line and its newline from the buffer.
            buffer.removeSubrange(buffer.startIndex...newlineIndex)
        }
        return lines
    }

    /// Decode the daemon messages contained in a chunk, skipping blank lines.
    public mutating func decodeMessages(_ chunk: Data) throws -> [DaemonMessage] {
        let decoder = JSONDecoder()
        return try push(chunk).map { try decoder.decode(DaemonMessage.self, from: $0) }
    }
}

// MARK: - Binary + socket path resolution

/// Locates the bundled `ports` binary and the daemon socket path.
public enum DaemonPaths {
    /// Bundle identifier used for the Application Support subdirectory.
    public static let bundleID = "ai.polymathlabs.PortsBar"

    /// Path to the `ports` binary, preferring the app bundle's Resources and
    /// falling back to a dev-built debug binary for `swift run`/tests.
    public static func portsBinary() -> URL? {
        if let bundled = Bundle.main.url(forResource: "ports", withExtension: nil) {
            return bundled
        }
        if let resourceURL = Bundle.main.resourceURL {
            let candidate = resourceURL.appendingPathComponent("ports")
            if FileManager.default.isExecutableFile(atPath: candidate.path) {
                return candidate
            }
        }
        // Dev fallback: a cargo debug build next to the worktree root.
        let fm = FileManager.default
        for relative in ["target/debug/ports", "../target/debug/ports"] {
            let candidate = URL(fileURLWithPath: fm.currentDirectoryPath)
                .appendingPathComponent(relative)
            if fm.isExecutableFile(atPath: candidate.path) {
                return candidate
            }
        }
        return nil
    }

    /// `~/Library/Application Support/<bundle-id>/daemon.sock`.
    public static func socketPath() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory())
                .appendingPathComponent("Library/Application Support")
        return base
            .appendingPathComponent(bundleID, isDirectory: true)
            .appendingPathComponent("daemon.sock")
    }
}

// MARK: - Daemon client

/// Errors surfaced by ``DaemonClient``.
public enum DaemonClientError: Error, Sendable {
    case binaryNotFound
    case socketCreationFailed(errno: Int32)
    case connectFailed(errno: Int32)
    case notConnected
    case writeFailed(errno: Int32)
}

/// A `Sendable`, lock-guarded holder for the read-loop task.
///
/// The `AsyncStream` builder closure is `@Sendable` and cannot mutate actor
/// state, so the task it spawns is parked here; `stop()` cancels through it.
private final class TaskBox: @unchecked Sendable {
    private let lock = NSLock()
    private var task: Task<Void, Never>?

    func store(_ task: Task<Void, Never>) {
        lock.lock(); defer { lock.unlock() }
        self.task = task
    }

    func cancel() {
        lock.lock(); defer { lock.unlock() }
        task?.cancel()
        task = nil
    }
}

/// Spawns and supervises the `ports daemon` child process, connects its Unix
/// socket, and exposes an `AsyncStream` of decoded ``DaemonMessage`` values.
///
/// All I/O is async/await; the blocking POSIX `connect`/`read`/`write` calls are
/// performed off the cooperative pool via `Task.detached` continuations. The
/// type is an `actor` so its mutable file-descriptor and child-process state is
/// race-free under Swift 6 strict concurrency.
public actor DaemonClient {
    private var fileDescriptor: Int32 = -1
    private var process: Process?
    private let readTask = TaskBox()

    public init() {}

    /// Spawn `ports daemon --socket <path>` if no live daemon is present, then
    /// connect the socket. Returns a stream of decoded daemon messages; the
    /// stream finishes when the connection is torn down or the read loop ends.
    public func start() throws -> AsyncStream<DaemonMessage> {
        let socket = DaemonPaths.socketPath()
        try ensureSocketDirectory(socket)

        // Try connecting to an existing daemon first; spawn if that fails.
        if (try? connectSocket(at: socket)) == nil {
            try spawnDaemon(socketPath: socket)
            try connectWithRetry(at: socket)
        }

        let fd = fileDescriptor
        let box = readTask
        // The builder closure runs synchronously and receives a fresh `Sendable`
        // continuation. The detached read loop owns it and finishes the stream on
        // EOF/cancellation; the spawned task is parked in the `TaskBox` so
        // `stop()` can cancel it. `stop()` also closes the fd, unblocking read(2).
        return AsyncStream<DaemonMessage>(bufferingPolicy: .unbounded) { continuation in
            let task = Task.detached {
                await daemonReadLoop(fd: fd, continuation: continuation)
            }
            box.store(task)
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// Encode and send a request as a single NDJSON line.
    public func send(_ request: Request) throws {
        guard fileDescriptor >= 0 else { throw DaemonClientError.notConnected }
        let data = try NDJSON.encodeRequest(request)
        try Self.writeAll(fd: fileDescriptor, data: data)
    }

    /// Tear down the read loop, close the socket, and terminate the child.
    public func stop() {
        readTask.cancel()
        if fileDescriptor >= 0 {
            close(fileDescriptor)
            fileDescriptor = -1
        }
        if let process, process.isRunning {
            process.terminate()
        }
        process = nil
    }

    // MARK: Private helpers

    private func ensureSocketDirectory(_ socket: URL) throws {
        let dir = socket.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: dir,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
    }

    private func spawnDaemon(socketPath: URL) throws {
        guard let binary = DaemonPaths.portsBinary() else {
            throw DaemonClientError.binaryNotFound
        }
        let proc = Process()
        proc.executableURL = binary
        proc.arguments = ["daemon", "--socket", socketPath.path]
        try proc.run()
        process = proc
    }

    private func connectWithRetry(at socket: URL, attempts: Int = 50) throws {
        var lastError: Error = DaemonClientError.connectFailed(errno: 0)
        for _ in 0..<attempts {
            do {
                try connectSocket(at: socket)
                return
            } catch {
                lastError = error
                // Brief async backoff while the daemon creates its socket.
                usleepShim(20_000)
            }
        }
        throw lastError
    }

    /// Open a `AF_UNIX`/`SOCK_STREAM` socket and connect to `socket`.
    @discardableResult
    private func connectSocket(at socket: URL) throws -> Int32 {
        let fd = Foundation.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw DaemonClientError.socketCreationFailed(errno: errno) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let path = socket.path
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
        guard path.utf8.count <= maxLen else {
            close(fd)
            throw DaemonClientError.connectFailed(errno: ENAMETOOLONG)
        }
        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            path.withCString { src in
                ptr.withMemoryRebound(to: CChar.self, capacity: maxLen + 1) { dst in
                    _ = strcpy(dst, src)
                }
            }
        }

        let connectResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                Foundation.connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connectResult == 0 else {
            let err = errno
            close(fd)
            throw DaemonClientError.connectFailed(errno: err)
        }
        fileDescriptor = fd
        return fd
    }

    // MARK: Static I/O (run off-actor in detached tasks)

    private static func writeAll(fd: Int32, data: Data) throws {
        try data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return }
            var offset = 0
            let total = raw.count
            while offset < total {
                let written = write(fd, base.advanced(by: offset), total - offset)
                if written <= 0 {
                    throw DaemonClientError.writeFailed(errno: errno)
                }
                offset += written
            }
        }
    }
}

/// Minimal sleep used during connect retry while the daemon comes up. Wraps the
/// POSIX `usleep` so the actor's retry loop does not need a `Task.sleep` import
/// dance; it is only invoked during initial connection, never in steady state.
private func usleepShim(_ microseconds: UInt32) {
    usleep(microseconds)
}

/// The daemon read loop: blocking `read(2)` into a buffer, NDJSON-framed, with
/// each decoded ``DaemonMessage`` yielded to `continuation`. Runs in a detached
/// task off the actor; finishes the stream on EOF, error, or cancellation.
///
/// `continuation` is `Sendable`, so this free function carries no actor-isolated
/// state and is safe under Swift 6 strict concurrency.
private func daemonReadLoop(
    fd: Int32,
    continuation: AsyncStream<DaemonMessage>.Continuation
) async {
    var framer = LineFramer()
    let bufferSize = 16 * 1024
    var buffer = [UInt8](repeating: 0, count: bufferSize)
    while !Task.isCancelled {
        let n = buffer.withUnsafeMutableBytes { ptr -> Int in
            read(fd, ptr.baseAddress, bufferSize)
        }
        if n <= 0 { break } // EOF or error.
        let chunk = Data(buffer[0..<n])
        if let messages = try? framer.decodeMessages(chunk) {
            for message in messages {
                continuation.yield(message)
            }
        }
    }
    continuation.finish()
}
