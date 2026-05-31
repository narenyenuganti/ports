import Testing
@testable import PortsBarCore

@Suite("Popover layout")
struct PopoverLayoutTests {
    @Test("port list allows a taller dropdown before scrolling")
    func portListMaxHeight() {
        #expect(PopoverLayout.width == 420)
        #expect(PopoverLayout.portListMaxHeight == 920)
    }

    @Test("port tiles use compact spacing")
    func compactTileSpacing() {
        #expect(PopoverLayout.portListSpacing == 5)
        #expect(PopoverLayout.portTileVerticalPadding == 7)
    }

    @Test("idle generic ports do not repeat remote port text")
    func genericIdlePortPresentation() {
        let presentation = PortTilePresentation(
            entry: PortEntry(remotePort: Port(22), forward: .idle)
        )

        #expect(presentation.primaryLabel == "port")
        #expect(presentation.primaryValue == "22")
        #expect(presentation.detail == nil)
        #expect(presentation.portAccessory == nil)
    }

    @Test("process rows show the process and separate port accessory")
    func processPortPresentation() {
        let presentation = PortTilePresentation(
            entry: PortEntry(remotePort: Port(3000), process: "next-server", forward: .idle)
        )

        #expect(presentation.primaryLabel == nil)
        #expect(presentation.primaryValue == "next-server")
        #expect(presentation.detail == nil)
        #expect(presentation.portAccessory == ":3000")
    }

    @Test("forwarding rows show only the local destination detail")
    func forwardingPortPresentation() {
        let presentation = PortTilePresentation(
            entry: PortEntry(
                remotePort: Port(3000),
                process: "next-server",
                forward: .forwarding(localPort: Port(13000))
            )
        )

        #expect(presentation.primaryValue == "next-server")
        #expect(presentation.detail == "localhost:13000")
        #expect(presentation.portAccessory == ":3000")
    }
}
