import AppKit
import AVFoundation
import ScreenCaptureKit

enum RecordingPermission: Equatable {
    case microphone, systemAudio
    var title: String { self == .microphone ? "Microphone Access Needed" : "System Audio Access Needed" }
    var explanation: String {
        switch self {
        case .microphone:
            return "Gday Meetings needs microphone access to record your voice. macOS has not made that access available to this app. No recording has started."
        case .systemAudio:
            return "Choose a display in the macOS sharing picker to let Gday Meetings record system audio for this meeting. No screen video is saved. No recording has started."
        }
    }
}

struct RecordingPermissionError: LocalizedError {
    let permission: RecordingPermission
    var errorDescription: String? { permission.explanation }
}

struct RecordingAuthorization {
    let filter: SCContentFilter
    let session: SystemAudioConsent
}

@MainActor
enum RecordingPermissions {
    private static var pendingSelection: SystemAudioConsent?

    // HIG Privacy: the person's Record action opens system-owned consent UI.
    // The picker authorizes only this capture session; no global screen access,
    // content enumeration or System Settings detour is needed.
    // https://developer.apple.com/design/human-interface-guidelines/privacy
    // https://developer.apple.com/videos/play/wwdc2023/10053/
    static func request(microphone: Bool, systemAudio: Bool) async throws -> RecordingAuthorization? {
        NSApp.activate(ignoringOtherApps: true)
        if microphone {
            let granted: Bool
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .authorized: granted = true
            case .notDetermined: granted = await AVCaptureDevice.requestAccess(for: .audio)
            default: granted = false
            }
            guard granted else { throw RecordingPermissionError(permission: .microphone) }
        }
        try Task.checkCancellation()
        guard systemAudio else { return nil }
        guard pendingSelection == nil else { throw MeetingError.message("Finish the current system-audio selection before recording.") }
        let session = SystemAudioConsent()
        pendingSelection = session
        defer { if pendingSelection === session { pendingSelection = nil } }
        do {
            let filter = try await session.select()
            try Task.checkCancellation()
            return RecordingAuthorization(filter: filter, session: session)
        } catch {
            session.close()
            throw error
        }
    }

    /// Called before the store waits for a pending start during Quit.
    static func cancelPendingSelection() { pendingSelection?.close(); pendingSelection = nil }

    nonisolated static func permission(for error: Error) -> RecordingPermission? {
        if let error = error as? RecordingPermissionError { return error.permission }
        let error = error as NSError
        if error.domain == SCStreamErrorDomain && error.code == SCStreamError.Code.userDeclined.rawValue { return .systemAudio }
        return nil
    }
}

/// Retained by AudioCapture until SCStream stops. Picker observers and configuration
/// belong to the main actor; ScreenCaptureKit callbacks explicitly hop back to it.
@MainActor
final class SystemAudioConsent: NSObject, SCContentSharingPickerObserver {
    private let picker = SCContentSharingPicker.shared
    private var continuation: CheckedContinuation<SCContentFilter, Error>?
    private var active = false

    func select() async throws -> SCContentFilter {
        try Task.checkCancellation()
        return try await withTaskCancellationHandler(operation: {
            try await withCheckedThrowingContinuation { continuation in
                self.continuation = continuation
                var configuration = SCContentSharingPickerConfiguration()
                configuration.allowedPickerModes = .singleDisplay
                configuration.allowsChangingSelectedContent = false
                picker.defaultConfiguration = configuration
                picker.maximumStreamCount = 1
                picker.add(self)
                active = true
                picker.isActive = true
                picker.present(using: .display)
            }
        }, onCancel: { [weak self] in
            Task { @MainActor in self?.close() }
        })
    }

    func close() {
        if active {
            picker.remove(self)
            picker.isActive = false
            active = false
        }
        finish(.failure(CancellationError()))
    }

    private func finish(_ result: Result<SCContentFilter, Error>) {
        let waiting = continuation
        continuation = nil
        waiting?.resume(with: result)
    }

    nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter, for stream: SCStream?) {
        Task { @MainActor [weak self] in
            guard let self, self.active, self.continuation != nil else { return }
            self.finish(.success(filter))
        }
    }

    nonisolated func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        Task { @MainActor [weak self] in
            guard let self, self.continuation != nil else { return }
            self.close()
        }
    }

    nonisolated func contentSharingPickerStartDidFailWithError(_ error: Error) {
        Task { @MainActor [weak self] in
            guard let self, self.continuation != nil else { return }
            self.finish(.failure(error))
            self.close()
        }
    }
}
