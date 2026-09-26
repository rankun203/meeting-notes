import AppKit
import SwiftUI
import Testing

@testable import GdayMeetings

/// New Recording's source rows must give their explanation the row's free width,
/// so a detail control below it (the microphone menu) can't force it to wrap.
@MainActor struct RecordingSourceRowLayoutTests {
    /// Row width inside the 510 pt sheet: 26 pt sheet and 14 pt card insets per side.
    static let sheetRowWidth: CGFloat = 510 - 2 * 26 - 2 * 14

    static func height(subtitle: String, deviceTitle: String, width: CGFloat) -> CGFloat {
        let row = RecordingSourceRow(
            name: "Microphone", subtitle: subtitle, symbol: "mic.fill", isOn: .constant(true)
        ) {
            Picker("Microphone Device", selection: .constant(0)) { Text(deviceTitle).tag(0) }
                .recordingSourceDetailMenu()
        }
        let host = NSHostingView(rootView: row.frame(width: width))
        host.frame.size = host.fittingSize
        host.layoutSubtreeIfNeeded()
        return host.fittingSize.height
    }

    @Test(arguments: [
        "System Default (MacBook Pro Microphone)",
        "System Default",
        "A very long external USB audio interface name for testing (Unavailable)",
    ])
    func subtitleStaysOnOneLineAtSheetWidth(deviceTitle: String) {
        // A one-word subtitle can't wrap; the real subtitle must take the same height.
        let singleLine = Self.height(subtitle: "Record", deviceTitle: deviceTitle, width: Self.sheetRowWidth)
        let actual = Self.height(
            subtitle: "Record your voice and nearby sounds.", deviceTitle: deviceTitle, width: Self.sheetRowWidth)
        #expect(actual == singleLine)
    }

    @Test func subtitleWrapsRatherThanTruncatesWhenNarrow() {
        let singleLine = Self.height(subtitle: "Record", deviceTitle: "System Default", width: 200)
        let actual = Self.height(
            subtitle: "Record your voice and nearby sounds.", deviceTitle: "System Default", width: 200)
        #expect(actual > singleLine)
    }
}
