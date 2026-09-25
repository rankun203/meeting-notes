import AVFoundation
import Foundation
import Combine
import Testing
@testable import GdayMeetings

private actor PlaybackPreparationGate {
    private var started = false
    private var startWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiter: CheckedContinuation<Void, Never>?
    func suspend() async {
        started = true
        for waiter in startWaiters { waiter.resume() }; startWaiters = []
        await withCheckedContinuation { releaseWaiter = $0 }
    }
    func waitUntilStarted() async {
        if started { return }
        await withCheckedContinuation { startWaiters.append($0) }
    }
    func release() { releaseWaiter?.resume(); releaseWaiter = nil }
}

private actor PlaybackAttemptCounter {
    private(set) var count = 0
    func next() -> Int { count += 1; return count }
}

@MainActor
struct MeetingPlaybackTests {
    @Test func sharedScrubPositionOverridesTicksAndClearsWithSelection() {
        let playback = MeetingPlayback()
        var controlUpdates = 0
        let subscription = playback.objectWillChange.sink { controlUpdates += 1 }
        playback.progress.update(10)
        playback.progress.scrub(to: 31)
        playback.progress.update(11) // Playback may continue during a pointer drag.
        #expect(playback.progress.displayedTime == 31)
        #expect(playback.currentTime == 11)
        playback.progress.scrub(to: .nan)
        #expect(playback.progress.displayedTime == 31)
        #expect(controlUpdates == 0)
        playback.progress.scrub(to: nil)
        #expect(playback.progress.displayedTime == 11)
        playback.progress.scrub(to: 40)
        playback.clear()
        #expect(playback.progress.scrubTime == nil)
        #expect(playback.progress.displayedTime == 0)
        withExtendedLifetime(subscription) {}
    }

    @Test func progressUpdatesDoNotInvalidatePlaybackControls() {
        let playback = MeetingPlayback()
        var controlUpdates = 0
        var clockUpdates = 0
        let controlSubscription = playback.objectWillChange.sink { controlUpdates += 1 }
        let clockSubscription = playback.progress.objectWillChange.sink { clockUpdates += 1 }
        for tick in 1...40 { playback.progress.update(Double(tick) / 4) }
        playback.progress.update(10) // A repeated position should not redraw either.
        #expect(playback.currentTime == 10)
        #expect(clockUpdates == 40)
        #expect(controlUpdates == 0)
        withExtendedLifetime((controlSubscription, clockSubscription)) {}
    }

    @Test func selectionLoadsPausedAndSwitchingTracksKeepsPosition() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let mic = directory.appendingPathComponent("microphone.wav")
        let system = directory.appendingPathComponent("system.wav")
        try makeSilence(mic, seconds: 2)
        try makeSilence(system, seconds: 3)
        let meeting = Meeting(title: "Fixture meeting", audioFiles: ["microphone.wav", "system.wav"])
        let playback = MeetingPlayback()
        defer { playback.clear() }
        playback.select(meeting: meeting, files: [mic, system])
        await playback.waitForPreparation()
        #expect(playback.errorMessage == nil)
        #expect(playback.hasSelection)
        #expect(!playback.isPlaying)
        #expect(playback.selectedTrack == -1)
        #expect(abs(playback.duration - 3) < 0.01)
        #expect(playback.trackNames == ["Microphone", "System Audio"])
        playback.seek(to: 1.2)
        playback.selectTrack(0)
        await playback.waitForPreparation()
        #expect(playback.selectedTrack == 0)
        #expect(abs(playback.duration - 3) < 0.01)
        #expect(playback.waveforms.compactMap { $0 }.count == 2)
        #expect(playback.mutedTracks == [1])
        playback.toggleMute(0)
        #expect(playback.mutedTracks == [0, 1])
        playback.toggleMute(0)
        #expect(playback.selectedTrack == 0)
        #expect(abs(playback.currentTime - 1.2) < 0.01)
        #expect(!playback.isPlaying)
        playback.seek(to: 500)
        #expect(playback.currentTime == playback.duration)
        playback.skip(by: -15)
        #expect(playback.currentTime == 0)
        var renamed = meeting; renamed.title = "Renamed"
        playback.reconcile(meetings: [renamed])
        #expect(playback.title == "Renamed")
        #expect(playback.selectedTrack == 0)
        playback.setRecordingActive(true)
        playback.play()
        #expect(!playback.isPlaying)
        #expect(playback.isPlaybackBlocked)
        playback.setRecordingActive(false)
        #expect(!playback.isPlaying)
        playback.reconcile(meetings: [])
        #expect(!playback.hasSelection)
        #expect(playback.duration == 0)
    }

    @Test func stalePreparationCannotReplaceSelectionAndDeletesItsOwnedFile() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let source = directory.appendingPathComponent("source.wav")
        let temporary = directory.appendingPathComponent("temporary.wav")
        try makeSilence(source, seconds: 0.2)
        let gate = PlaybackPreparationGate()
        let playback = MeetingPlayback { url in
            try FileManager.default.copyItem(at: url, to: temporary)
            await gate.suspend() // Deliberately ignores cancellation like some framework work.
            return PreparedPlaybackAudio(url: temporary, temporary: true)
        }
        playback.select(meeting: Meeting(title: "Old"), files: [source])
        let oldWork = Task { await playback.waitForPreparation() }
        await gate.waitUntilStarted()
        playback.clear()
        await gate.release()
        await oldWork.value
        #expect(!playback.hasSelection)
        #expect(!playback.isLoading)
        #expect(!FileManager.default.fileExists(atPath: temporary.path))
        #expect(FileManager.default.fileExists(atPath: source.path))
    }

    @Test func failedPreparationReportsErrorWithoutStartingPlayback() async throws {
        let playback = MeetingPlayback { _ in throw ServiceError("Fixture decode failed") }
        playback.select(meeting: Meeting(title: "Broken"), files: [URL(fileURLWithPath: "/fixture/invalid.opus")])
        await playback.waitForPreparation()
        #expect(playback.errorMessage?.contains("Fixture decode failed") == true)
        #expect(!playback.isLoading)
        #expect(!playback.isPlaying)
        playback.clear()
    }

    @Test func playRetriesFailedSelectionButRecordingPreventsRetry() async {
        let counter = PlaybackAttemptCounter()
        let playback = MeetingPlayback { _ in
            let attempt = await counter.next()
            throw ServiceError("Fixture failure \(attempt)")
        }
        let meeting = Meeting(title: "Retry fixture")
        let files = [URL(fileURLWithPath: "/fixture/microphone.opus"), URL(fileURLWithPath: "/fixture/system.opus")]
        playback.select(meeting: meeting, files: files, track: 1)
        await playback.waitForPreparation()
        #expect(await counter.count == 1)
        #expect(playback.errorMessage?.contains("Fixture failure 1") == true)
        playback.setRecordingActive(true)
        playback.togglePlayPause()
        await playback.waitForPreparation()
        #expect(await counter.count == 1)
        #expect(playback.errorMessage?.contains("Fixture failure 1") == true)
        playback.setRecordingActive(false)
        playback.togglePlayPause()
        await playback.waitForPreparation()
        #expect(await counter.count == 2)
        #expect(playback.errorMessage?.contains("Fixture failure 2") == true)
        #expect(playback.meetingID == meeting.id)
        #expect(playback.selectedTrack == 1)
        #expect(!playback.isPlaying)
        #expect(!playback.isLoading)
        playback.clear()
    }

    private func makeSilence(_ url: URL, seconds: Double) throws {
        let format = try #require(AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 1))
        let frames = AVAudioFrameCount(seconds * 48000)
        let buffer = try #require(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames))
        buffer.frameLength = frames
        buffer.floatChannelData![0].initialize(repeating: 0, count: Int(frames))
        var file: AVAudioFile? = try AVAudioFile(forWriting: url, settings: format.settings)
        try file?.write(from: buffer); file = nil
    }
}
