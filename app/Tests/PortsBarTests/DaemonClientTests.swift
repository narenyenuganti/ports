import Foundation
import Testing
@testable import PortsBar

struct DaemonClientTests {
    @Test func encodeRequestEndsWithNewline() throws {
        let request = Request(id: 1, body: .ping)
        let data = try NDJSON.encodeRequest(request)
        #expect(data.last == 0x0A, "NDJSON line must end with \\n")

        // The body (minus the newline) must round-trip back to the request.
        let body = data.dropLast()
        let decoded = try JSONDecoder().decode(Request.self, from: Data(body))
        #expect(decoded == request)
    }

    @Test func encodeRequestHasNoInteriorNewline() throws {
        let request = Request(id: 5, body: .startForward(remotePort: Port(3000), localPort: Port(3000)))
        let data = try NDJSON.encodeRequest(request)
        let newlineCount = data.filter { $0 == 0x0A }.count
        #expect(newlineCount == 1, "exactly one trailing newline expected")
    }

    @Test func framerSplitsTwoConcatenatedLines() throws {
        var framer = LineFramer()
        let line1 = #"{"type":"ack","id":1}"#
        let line2 = #"{"type":"ack","id":2}"#
        let chunk = Data("\(line1)\n\(line2)\n".utf8)

        let messages = try framer.decodeMessages(chunk)
        #expect(messages.count == 2)
        guard case .ack(let id1, _, _) = messages[0], case .ack(let id2, _, _) = messages[1] else {
            Issue.record("expected two ack messages")
            return
        }
        #expect(id1 == 1)
        #expect(id2 == 2)
    }

    @Test func framerBuffersPartialLineAcrossChunks() throws {
        var framer = LineFramer()
        let full = #"{"type":"ack","id":7}"#
        let splitIndex = full.index(full.startIndex, offsetBy: 10)
        let part1 = Data(String(full[full.startIndex..<splitIndex]).utf8)
        let part2 = Data(("\(String(full[splitIndex...]))\n").utf8)

        // First chunk has no newline -> nothing yet.
        let first = try framer.decodeMessages(part1)
        #expect(first.isEmpty)

        // Second chunk completes the line.
        let second = try framer.decodeMessages(part2)
        #expect(second.count == 1)
        guard case .ack(let id, _, _) = second[0] else {
            Issue.record("expected ack")
            return
        }
        #expect(id == 7)
    }

    @Test func framerSkipsBlankLines() throws {
        var framer = LineFramer()
        let chunk = Data("\n\n{\"type\":\"ack\",\"id\":3}\n\n".utf8)
        let messages = try framer.decodeMessages(chunk)
        #expect(messages.count == 1)
    }

    @Test func socketPathIsUnderApplicationSupport() {
        let path = DaemonPaths.socketPath().path
        #expect(path.contains(DaemonPaths.bundleID))
        #expect(path.hasSuffix("daemon.sock"))
    }
}
