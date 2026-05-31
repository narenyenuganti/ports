import AppKit

enum PopoverKeyboardCommand: Equatable {
    case selectPrevious
    case selectNext
    case toggleSelected

    init?(event: NSEvent) {
        self.init(
            charactersIgnoringModifiers: event.charactersIgnoringModifiers,
            specialKey: event.specialKey
        )
    }

    init?(charactersIgnoringModifiers: String?, specialKey: NSEvent.SpecialKey?) {
        switch specialKey {
        case .upArrow:
            self = .selectPrevious
        case .downArrow:
            self = .selectNext
        case .carriageReturn, .enter:
            self = .toggleSelected
        default:
            switch charactersIgnoringModifiers?.lowercased() {
            case "k":
                self = .selectPrevious
            case "j":
                self = .selectNext
            case "\r", "\n":
                self = .toggleSelected
            default:
                return nil
            }
        }
    }
}
