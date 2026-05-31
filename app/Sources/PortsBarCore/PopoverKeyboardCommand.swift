import AppKit

enum PopoverKeyboardCommand: Equatable {
    case selectPrevious
    case selectNext
    case toggleSelected
    case beginFilter
    case appendFilter(String)
    case deleteFilterCharacter
    case cancelFilter

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
        case .delete, .deleteForward:
            self = .deleteFilterCharacter
        default:
            guard let charactersIgnoringModifiers else { return nil }
            switch charactersIgnoringModifiers {
            case "\u{1B}":
                self = .cancelFilter
            case "\u{7F}", "\u{8}":
                self = .deleteFilterCharacter
            case "/":
                self = .beginFilter
            case "\r", "\n":
                self = .toggleSelected
            default:
                switch charactersIgnoringModifiers.lowercased() {
                case "k":
                    self = .selectPrevious
                case "j":
                    self = .selectNext
                default:
                    guard charactersIgnoringModifiers.count == 1,
                          let scalar = charactersIgnoringModifiers.unicodeScalars.first,
                          !CharacterSet.controlCharacters.contains(scalar)
                    else { return nil }
                    self = .appendFilter(charactersIgnoringModifiers)
                }
            }
        }
    }
}
