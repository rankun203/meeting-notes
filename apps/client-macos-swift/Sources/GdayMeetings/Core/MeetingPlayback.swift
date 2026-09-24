import AVFoundation
import Combine
import Foundation

/// One app-owned transport survives navigation and is shared by every window.
/// HIG Playing Audio / Accessibility: sound starts only through explicit playback
/// controls, respects system output volume, and remains independently pausable.
/// https://developer.apple.com/design/human-interface-guidelines/playing-audio
/// https://developer.apple.com/design/human-interface-guidelines/accessibility
@MainActor
final class MeetingPlayback: ObservableObject {
    @Published private(set) var meetingID: UUID?
    @Published private(set) var title = ""
    @Published private(set) var trackNames: [String] = []
    @Published private(set) var selectedTrack = -1
    @Published private(set) var isPlaying = false
    @Published private(set) var isLoading = false
    @Published private(set) var hasEnded = false
    @Published private(set) var currentTime: Double = 0
    @Published private(set) var duration: Double = 0
    @Published private(set) var playbackRate: Double = 1
    @Published private(set) var isPlaybackBlocked = false
    @Published private(set) var errorMessage: String?
    var hasSelection: Bool { meetingID != nil }

    typealias AudioPreparer = @Sendable (URL) async throws -> PreparedPlaybackAudio
    private let prepareAudio: AudioPreparer
    private let player = AVPlayer()
    private var sourceMeeting: Meeting?
    private var sourceFiles: [URL] = []
    private var temporaryURLs: [URL] = []
    private var preparationTask: Task<Void, Never>?
    private var seekTask: Task<Void, Never>?
    private var generation = UUID()
    private var seekGeneration = UUID()
    private var wantsPlayback = false
    private var pendingPosition: Double = 0
    private var isSeeking = false
    private var periodicObserver: Any?
    private var playbackObservation: NSKeyValueObservation?
    private var itemObservation: NSKeyValueObservation?
    private var endObserver: NSObjectProtocol?
    private var failureObserver: NSObjectProtocol?

    init(prepareAudio: @escaping AudioPreparer = AudioPlaybackPreparation.prepare) {
        self.prepareAudio = prepareAudio
        player.actionAtItemEnd = .pause
        periodicObserver = player.addPeriodicTimeObserver(forInterval: CMTime(seconds: 0.25, preferredTimescale: 600), queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                if !self.isSeeking && !self.isLoading {
                    let time = self.player.currentTime().seconds
                    if time.isFinite { self.currentTime = Self.clampedTime(time, duration: self.duration) }
                }
            }
        }
        playbackObservation = player.observe(\.timeControlStatus, options: [.new]) { [weak self] _, _ in
            Task { @MainActor in
                guard let self else { return }
                self.isPlaying = self.player.timeControlStatus == .playing && !self.isPlaybackBlocked
            }
        }
    }

    deinit {
        preparationTask?.cancel(); seekTask?.cancel()
        player.pause(); player.replaceCurrentItem(with: nil)
        if let periodicObserver { player.removeTimeObserver(periodicObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        if let failureObserver { NotificationCenter.default.removeObserver(failureObserver) }
        for url in temporaryURLs { try? FileManager.default.removeItem(at: url) }
    }

    /// Selection prepares a paused item. Browsing elsewhere never needs to call this.
    func select(meeting: Meeting, files: [URL], track: Int = -1) {
        guard !isPlaybackBlocked else { return }
        let track = Self.validTrack(track, count: files.count)
        if meetingID == meeting.id, sourceFiles == files, selectedTrack == track, errorMessage == nil {
            title = meeting.title; sourceMeeting = meeting
            return
        }
        let sameMeeting = meetingID == meeting.id
        load(meeting: meeting, files: files, track: track, position: sameMeeting ? currentTime : 0, autoplay: false)
    }

    func play(meeting: Meeting, files: [URL], at position: Double = 0) {
        guard !isPlaybackBlocked else { return }
        if meetingID == meeting.id, sourceFiles == files, errorMessage == nil {
            title = meeting.title; sourceMeeting = meeting
            wantsPlayback = true
            seek(to: position)
            return
        }
        load(meeting: meeting, files: files, track: -1, position: position, autoplay: true)
    }

    func play() {
        guard hasSelection, !isPlaybackBlocked, errorMessage == nil else { return }
        wantsPlayback = true
        if isLoading { return }
        guard player.currentItem != nil else { return }
        if hasEnded || (duration > 0 && currentTime >= duration) { seek(to: 0) }
        else { player.playImmediately(atRate: Float(playbackRate)) }
    }

    func pause() {
        wantsPlayback = false
        player.pause(); isPlaying = false
    }

    func togglePlayPause() { if wantsPlayback || isPlaying { pause() } else { play() } }

    func seek(to seconds: Double) {
        guard hasSelection, seconds.isFinite else { return }
        let target = duration > 0 ? Self.clampedTime(seconds, duration: duration) : max(0, seconds)
        pendingPosition = target; currentTime = target; hasEnded = duration > 0 && target >= duration
        if isLoading { return }
        guard let item = player.currentItem else { return }
        seekTask?.cancel(); item.cancelPendingSeeks()
        let operation = UUID(); seekGeneration = operation
        let currentGeneration = generation
        isSeeking = true
        seekTask = Task { [weak self] in
            guard let self else { return }
            let completed = await self.player.seek(to: CMTime(seconds: target, preferredTimescale: 48000), toleranceBefore: .zero, toleranceAfter: .zero)
            guard !Task.isCancelled, self.generation == currentGeneration, self.seekGeneration == operation, self.player.currentItem === item else { return }
            self.isSeeking = false
            if completed, self.wantsPlayback, !self.isPlaybackBlocked, !self.hasEnded { self.player.playImmediately(atRate: Float(self.playbackRate)) }
            else if self.hasEnded { self.pause() }
        }
    }

    func skip(by seconds: Double) { seek(to: currentTime + seconds) }

    func setRate(_ rate: Double) {
        guard rate.isFinite else { return }
        playbackRate = min(2, max(0.5, rate))
        if isPlaying && !isPlaybackBlocked { player.rate = Float(playbackRate) }
    }

    func selectTrack(_ track: Int) {
        guard let meeting = sourceMeeting, !isPlaybackBlocked else { return }
        let selection = Self.validTrack(track, count: sourceFiles.count)
        guard selection != selectedTrack else { return }
        load(meeting: meeting, files: sourceFiles, track: selection, position: currentTime, autoplay: wantsPlayback)
    }

    /// Recording prevents the app's own audio from feeding back into the new meeting.
    /// Ending recording does not surprise the person by resuming audio automatically.
    func setRecordingActive(_ active: Bool) {
        isPlaybackBlocked = active
        if active { pause() }
    }

    func reconcile(meetings: [Meeting]) {
        guard let id = meetingID else { return }
        guard let meeting = meetings.first(where: { $0.id == id }) else { clear(); return }
        title = meeting.title
        if sourceMeeting?.audioFiles != meeting.audioFiles {
            // The owner resolves URLs when starting the new recording revision.
            // Release old assets now so removed/replaced files cannot keep playing.
            clear()
        } else { sourceMeeting = meeting }
    }

    func clear() {
        generation = UUID(); seekGeneration = UUID()
        preparationTask?.cancel(); preparationTask = nil
        seekTask?.cancel(); seekTask = nil
        releaseCurrentItem()
        sourceMeeting = nil; sourceFiles = []
        meetingID = nil; title = ""; trackNames = []; selectedTrack = -1
        currentTime = 0; duration = 0; pendingPosition = 0
        isLoading = false; isSeeking = false; hasEnded = false; errorMessage = nil
    }

    /// Lets lifecycle/fixture checks await owned work without depending on UI sleeps.
    func waitForPreparation() async { await preparationTask?.value }

    static func clampedTime(_ seconds: Double, duration: Double) -> Double {
        guard seconds.isFinite, duration.isFinite else { return 0 }
        return min(max(0, seconds), max(0, duration))
    }

    private static func validTrack(_ track: Int, count: Int) -> Int { track >= 0 && track < count ? track : -1 }

    private func load(meeting: Meeting, files: [URL], track: Int, position: Double, autoplay: Bool) {
        generation = UUID(); let operation = generation
        preparationTask?.cancel(); seekTask?.cancel()
        releaseCurrentItem()
        sourceMeeting = meeting; sourceFiles = files
        meetingID = meeting.id; title = meeting.title; selectedTrack = track
        trackNames = files.map { file in
            let name = file.deletingPathExtension().lastPathComponent
            return name.hasPrefix("microphone") ? "Microphone" : name.hasPrefix("system") ? "System Audio" : name
        }
        errorMessage = nil; hasEnded = false; isSeeking = false
        duration = 0; currentTime = max(0, position.isFinite ? position : 0); pendingPosition = currentTime
        wantsPlayback = autoplay && !isPlaybackBlocked
        guard !files.isEmpty else { errorMessage = "This meeting has no audio files to play."; isLoading = false; return }
        isLoading = true
        let prepare = prepareAudio
        preparationTask = Task { [weak self] in
            var temporary: [URL] = []
            var transferred = false
            defer { if !transferred { for url in temporary { try? FileManager.default.removeItem(at: url) } } }
            do {
                let selected = track >= 0 ? [files[track]] : files
                let composition = AVMutableComposition()
                var compositionTracks: [AVCompositionTrack] = []
                for file in selected {
                    try Task.checkCancellation()
                    let prepared = try await prepare(file)
                    if prepared.temporary { temporary.append(prepared.url) }
                    try Task.checkCancellation()
                    let asset = AVURLAsset(url: prepared.url)
                    let audioTracks = try await asset.loadTracks(withMediaType: .audio)
                    let length = try await asset.load(.duration)
                    guard length.isNumeric, CMTimeCompare(length, .zero) > 0, !audioTracks.isEmpty else { throw MeetingError.message("An audio track is empty or cannot be played.") }
                    for audio in audioTracks {
                        guard let destination = composition.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else { throw MeetingError.message("Unable to prepare audio playback.") }
                        try destination.insertTimeRange(CMTimeRange(start: .zero, duration: length), of: audio, at: .zero)
                        compositionTracks.append(destination)
                    }
                }
                try Task.checkCancellation()
                guard let self, self.generation == operation else { return }
                let length = composition.duration.seconds
                guard length.isFinite, length > 0 else { throw MeetingError.message("This recording has no playable duration.") }
                let item = AVPlayerItem(asset: composition)
                if compositionTracks.count > 1 {
                    // Equal attenuation avoids clipping when simultaneous tracks sum.
                    // System output volume remains untouched (HIG Playing Audio).
                    let mix = AVMutableAudioMix()
                    mix.inputParameters = compositionTracks.map { track in
                        let parameter = AVMutableAudioMixInputParameters(track: track)
                        parameter.setVolume(1 / Float(compositionTracks.count), at: .zero)
                        return parameter
                    }
                    item.audioMix = mix
                }
                self.temporaryURLs = temporary; transferred = true
                self.duration = length
                self.player.replaceCurrentItem(with: item)
                self.observe(item, generation: operation)
                self.isLoading = false
                self.seek(to: self.pendingPosition)
            } catch is CancellationError {
                // Replacing/clearing selection owns the published state; stale work only cleans up.
            } catch {
                guard let self, self.generation == operation, !Task.isCancelled else { return }
                self.pause(); self.isLoading = false
                self.errorMessage = "Unable to load meeting audio: " + error.localizedDescription
            }
        }
    }

    private func observe(_ item: AVPlayerItem, generation operation: UUID) {
        itemObservation = item.observe(\.status, options: [.initial, .new]) { [weak self] observed, _ in
            Task { @MainActor in
                guard let self, self.generation == operation, self.player.currentItem === observed else { return }
                if observed.status == .failed {
                    self.pause(); self.isLoading = false
                    self.errorMessage = observed.error?.localizedDescription ?? "Audio playback failed."
                }
            }
        }
        endObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self, self.generation == operation else { return }
                self.pause(); self.currentTime = self.duration; self.hasEnded = true
            }
        }
        failureObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemFailedToPlayToEndTime, object: item, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self, self.generation == operation else { return }
                self.pause(); self.errorMessage = item.error?.localizedDescription ?? "The recording could not finish playing."
            }
        }
    }

    private func releaseCurrentItem() {
        pause()
        itemObservation?.invalidate(); itemObservation = nil
        if let endObserver { NotificationCenter.default.removeObserver(endObserver); self.endObserver = nil }
        if let failureObserver { NotificationCenter.default.removeObserver(failureObserver); self.failureObserver = nil }
        player.currentItem?.cancelPendingSeeks()
        player.replaceCurrentItem(with: nil)
        for url in temporaryURLs { try? FileManager.default.removeItem(at: url) }
        temporaryURLs = []
    }
}
