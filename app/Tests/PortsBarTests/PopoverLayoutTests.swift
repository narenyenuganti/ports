import Testing
@testable import PortsBarCore

@Suite("Popover layout")
struct PopoverLayoutTests {
    @Test("port list allows a taller dropdown before scrolling")
    func portListMaxHeight() {
        #expect(PopoverLayout.portListMaxHeight == 680)
    }
}
