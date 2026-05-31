import Foundation
import Testing
@testable import PortsBarCore

@Suite("NDJSON framing")
struct NDJSONFramingTests {
    @Test("encoder appends trailing newline and round-trips")
    func encoderRoundTrip() throws {
        let req = Request(id: 42, body: .ping)
        let data = try NDJSONEncoder.line(req)
        #expect(data.last == UInt8(ascii: "\n"))
        let decoded = try JSONDecoder().decode(Request.self, from: data.dropLast())
        #expect(decoded == req)
    }

    @Test("encoder does not escape slashes in paths")
    func encoderNoSlashEscaping() throws {
        let req = Request(id: 1, body: .sendFile(localPath: "/tmp/data.bin", remotePath: "/srv/data.bin"))
        let str = String(decoding: try NDJSONEncoder.line(req), as: UTF8.self)
        #expect(str.contains("/tmp/data.bin"))
        #expect(!str.contains("\\/"))
    }

    @Test("framer yields two messages from two concatenated lines")
    func framerTwoLines() throws {
        var framer = NDJSONFramer()
        let line1 = #"{"type":"ack","id":7}"#
        let line2 = #"{"type":"event","event":"file_transfer","ok":true,"detail":"done"}"#
        let combined = Data("\(line1)\n\(line2)\n".utf8)

        let messages = try framer.push(combined)
        #expect(messages.count == 2)

        guard case .ack(let id, _, _) = messages[0] else {
            Issue.record("expected ack first")
            return
        }
        #expect(id == 7)

        guard case .event(.fileTransfer(let ok, _)) = messages[1] else {
            Issue.record("expected file_transfer event second")
            return
        }
        #expect(ok == true)
    }

    @Test("framer buffers a partial line until completed")
    func framerPartialLine() throws {
        var framer = NDJSONFramer()
        let full = #"{"type":"ack","id":9}"#
        let head = Data(full.prefix(10).utf8)
        let tail = Data((full.dropFirst(10) + "\n").utf8)

        #expect(try framer.push(head).isEmpty)

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
        #expect(try framer.push(combined).count == 1)
    }

    @Test("start_forward request line includes explicit null and round-trips")
    func encodeThenDecode() throws {
        let req = Request(id: 7, body: .startForward(remotePort: Port(3000), localPort: nil))
        let line = try NDJSONEncoder.line(req)
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
