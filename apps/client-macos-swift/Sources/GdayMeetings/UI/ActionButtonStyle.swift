import SwiftUI

/// Interaction feedback within the existing label bounds: no padding, frames,
/// scaling, or animation that could alter layout or move adjacent controls.
/// https://developer.apple.com/design/human-interface-guidelines/buttons
struct ActionButtonStyle: ButtonStyle {
    var cornerRadius: CGFloat = 8

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .modifier(ActionHover(pressed: configuration.isPressed, cornerRadius: cornerRadius))
    }
}

/// Also supplements native borderless controls and custom seeking surfaces.
/// Native controls retain their own pressed and keyboard-focus treatment.
struct ActionHover: ViewModifier {
    var outlined = false
    var pressed = false
    var cornerRadius: CGFloat = 8
    @ViewState private var hovered = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        content
            .contentShape(Rectangle())
            .background {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .fill(.primary.opacity(enabled ? (pressed ? 0.16 : hovered ? (outlined ? 0.14 : 0.08) : 0) : 0))
                    .allowsHitTesting(false)
            }
            .overlay {
                if enabled && (hovered || pressed) && (outlined || contrast == .increased) {
                    RoundedRectangle(cornerRadius: cornerRadius)
                        .strokeBorder(.primary.opacity(contrast == .increased ? 1 : 0.25), lineWidth: 1)
                        .allowsHitTesting(false)
                }
            }
            .onHover { hovered = $0 }
    }
}
