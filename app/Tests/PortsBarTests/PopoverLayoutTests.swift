import AppKit
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

    @Test("keyboard commands match terminal navigation keys")
    func keyboardCommandMapping() {
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "j", specialKey: nil) == .selectNext)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "k", specialKey: nil) == .selectPrevious)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: nil, specialKey: .downArrow) == .selectNext)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: nil, specialKey: .upArrow) == .selectPrevious)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: nil, specialKey: .carriageReturn) == .toggleSelected)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "/", specialKey: nil) == .beginFilter)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "n", specialKey: nil) == .appendFilter("n"))
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: nil, specialKey: .delete) == .deleteFilterCharacter)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "\u{7F}", specialKey: nil) == .deleteFilterCharacter)
        #expect(PopoverKeyboardCommand(charactersIgnoringModifiers: "\u{1B}", specialKey: nil) == .cancelFilter)
    }
}
