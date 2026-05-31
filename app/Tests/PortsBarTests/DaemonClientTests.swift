import Foundation
import Testing
@testable import PortsBar

@Suite("NDJSON framing")
struct NDJSONFramingTests {
    @Test("encoder appends trailing newline and round-trips")
    func encoderRoundTrip() throws {
        let req = Request(id: 42, body: .connect(host: HostAlias("dev-desktop")))
        let data = try NDJSONEncoder.line(req)
        #expect(data.last == UInt8(ascii: "\n"))

        // Strip the newline and decode back.
        let jsonData = data.dropLast()
        let decoded = try JSONDecoder().decode(Request.self, from: jsonData)
        #expect(decoded == req)
    }

    @Test("encoder does not escape slashes in paths")
    func encoderNoSlashEscaping() throws {
        let req = Request(id: 1, body: .sendFile(localPath: "/tmp/data.bin", remotePath: "/home/user"))
        let data = try NDJSONEncoder.line(req)
        let str = String(decoding: data, as: UTF8.self)
        #expect(str.contains("/tmp/data.bin"))
        #expect(!str.contains("\\/"))
    }

    @Test("framer yields two messages from two concatenated lines")
    func framerTwoLines() throws {
        var framer = NDJSONFramer()
        let line1 = #"{"type":"event","event":"forward_removed","forward_id":1}"#
        let line2 = #"{"type":"ack","id":3}"#
        let combined = Data("\(line1)\n\(line2)\n".utf8)

        let messages = try framer.push(combined)
        #expect(messages.count == 2)

        guard case .event(.forwardRemoved(let fid)) = messages[0] else {
            Issue.record("expected forward_removed first")
            return
        }
        #expect(fid == ForwardId(1))

        guard case .ack(let id, _, _) = messages[1] else {
            Issue.record("expected ack second")
            return
        }
        #expect(id == 3)
    }

    @Test("framer buffers a partial line until completed")
    func framerPartialLine() throws {
        var framer = NDJSONFramer()
        let full = #"{"type":"ack","id":9}"#
        let head = Data(full.prefix(10).utf8)
        let tail = Data((full.dropFirst(10) + "\n").utf8)

        let first = try framer.push(head)
        #expect(first.isEmpty)

        let second = try framer.push(tail)
        #expect(second.count == 1)
        guard case .ack(let id, _, _) = second[0] else {
            Issue.record("expected ack")
            return
        }
        #expect(id == 9)
    }

    @Test("framer ignores blank lines")
    func framerBlankLines() throws {
        var framer = NDJSONFramer()
        let combined = Data("\n{\"type\":\"ack\",\"id\":1}\n\n".utf8)
        let messages = try framer.push(combined)
        #expect(messages.count == 1)
    }

    @Test("full request line round-trips through encoder then framer-style decode")
    func encodeThenDecode() throws {
        let req = Request(id: 7, body: .startForward(remotePort: Port(3000), localPort: nil))
        let line = try NDJSONEncoder.line(req)
        // The line includes explicit null for local_port (request-side semantics).
        let str = String(decoding: line, as: UTF8.self)
        #expect(str.contains("\"local_port\":null"))
        let decoded = try JSONDecoder().decode(Request.self, from: line.dropLast())
        #expect(decoded == req)
    }
}

@Suite("Socket path defaults")
struct SocketPathTests {
    @Test("uses XDG_RUNTIME_DIR when set")
    func xdgRuntime() {
        let path = DaemonSocket.defaultPath(environment: ["XDG_RUNTIME_DIR": "/run/user/501"])
        #expect(path == "/run/user/501/ports.sock")
    }

    @Test("falls back to Application Support when XDG unset")
    func appSupportFallback() {
        let path = DaemonSocket.defaultPath(environment: [:])
        #expect(path.hasSuffix("ports/ports.sock"))
    }
}
