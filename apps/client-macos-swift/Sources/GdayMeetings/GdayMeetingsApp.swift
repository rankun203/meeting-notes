import SwiftUI
import AppKit

@main
struct GdayMeetingsApp: App {
    @NSApplicationDelegateAdaptor(MeetingsAppDelegate.self) private var delegate
    @StateObject private var store = MeetingStore()

    var body: some Scene {
        WindowGroup(id: "main") {
            LibraryView().environmentObject(store)
                .onAppear { delegate.store = store }
                .frame(minWidth: 900, minHeight: 600)
        }
        .defaultSize(width: 1200, height: 800)
        .commands {
            // HIG: expose frequent commands in the menu bar, with standard shortcuts.
            // https://developer.apple.com/design/human-interface-guidelines/designing-for-macos
            CommandGroup(replacing: .newItem) {
                Button("New Meeting") { _ = store.createMeeting(title: "Untitled Meeting") }
                    .keyboardShortcut("n")
                Button("Import Audio…") { MeetingPanels.importAudio(store) }.keyboardShortcut("o")
                Button("Import Existing Gday Library…") { MeetingPanels.importLegacy(store) }
                Button("Import Meeting Archive…") { MeetingPanels.importArchive(store) }
            }
            CommandMenu("Recording") {
                Button(store.recordingID == nil ? "Start Recording" : "Stop Recording") {
                    Task { if store.recordingID == nil { await store.startRecording() } else { await store.stopRecording() } }
                }.keyboardShortcut("r", modifiers: [.command, .shift])
                    .disabled(store.isFinalizingRecording)
            }
        }
        // HIG: app-specific preferences live in a separate standard Settings window.
        // https://developer.apple.com/design/human-interface-guidelines/settings
        Settings { SettingsView().environmentObject(store) }
        MenuBarExtra("Gday Meetings", systemImage: store.recordingID == nil ? "waveform" : "record.circle.fill") {
            RecordingMenuView().environmentObject(store)
        }
    }
}

@MainActor
final class MeetingsAppDelegate: NSObject, NSApplicationDelegate {
    weak var store: MeetingStore?
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let store else { return .terminateNow }
        Task {
            await store.finalizeForQuit()
            sender.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }
}


private struct RecordingMenuView: View {
    @EnvironmentObject private var store: MeetingStore
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        if store.isFinalizingRecording {
            Text("Saving recording…")
        } else if let started = store.recordingStartedAt {
            Text("Recording since \(started.formatted(date: .omitted, time: .shortened))")
        }
        Button(store.recordingID == nil ? "Start Recording" : "Stop Recording") {
            Task { if store.recordingID == nil { await store.startRecording() } else { await store.stopRecording() } }
        }.disabled(store.isFinalizingRecording)
        Button("Show Gday Meetings") { openWindow(id: "main"); NSApp.activate(ignoringOtherApps: true) }
        Divider()
        Button("Quit Gday Meetings") { NSApp.terminate(nil) }.keyboardShortcut("q")
    }
}
