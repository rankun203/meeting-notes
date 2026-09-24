import AVFoundation
import Foundation

struct ServerTranscriptionAttempt: Codable, Equatable {
    let origin: String
    let idempotencyKey: String
    let title: String
    var inputs: [ServerTrackInput] = []
    var taskID: String?
}

extension MeetingStore {
    func transcribeOnServer(id: UUID) async throws {
        guard libraryWritable else { throw ServiceError("Restore the local library before starting server transcription; durable checkpoints must be saved first.") }
        let server = GdayServerService.shared
        guard var meeting = meetings.first(where: { $0.id == id }) else { return }
        guard let origin = server.origin else { throw ServiceError("Sign in to the server in Settings to resume transcription.") }
        var attempt = meeting.serverTranscription ?? ServerTranscriptionAttempt(origin: origin, idempotencyKey: UUID().uuidString, title: meeting.title)
        guard attempt.origin == origin else { throw ServiceError("Sign in to \(attempt.origin) to resume this meeting's transcription.") }
        func checkpoint() throws {
            guard var latest = meetings.first(where: { $0.id == id }) else { throw ServiceError("This meeting was deleted.") }
            latest.serverTranscription = attempt
            errorMessage = nil
            updateMeeting(latest)
            if let errorMessage { throw ServiceError("Could not save the transcription checkpoint: \(errorMessage)") }
            meeting = latest
        }
        try checkpoint()
        if attempt.taskID == nil {
            try await server.ensureTranscriptionAvailable()
            let files = audioURLs(for: meeting)
            guard !files.isEmpty else { throw ServiceError("This meeting has no audio to transcribe.") }
            for (index, file) in files.enumerated() where index >= attempt.inputs.count {
                statusMessage = "Uploading audio \(index + 1) of \(files.count)…"
                let prepared = try await prepareServerAudio(file)
                defer { if prepared.temporary { try? FileManager.default.removeItem(at: prepared.url) } }
                let url = try await server.upload(file: prepared.url)
                let isMic = file.deletingPathExtension().lastPathComponent.lowercased().contains("mic")
                attempt.inputs.append(ServerTrackInput(url: url, trackName: isMic ? "mic" : "track\(index)", sourceType: isMic ? "mic" : "system", channels: prepared.channels))
                try checkpoint()
            }
            statusMessage = "Submitting transcription…"
            attempt.taskID = try await server.submit(externalID: id.uuidString, title: attempt.title, inputs: attempt.inputs, language: "auto", diarize: true, idempotencyKey: attempt.idempotencyKey)
            try checkpoint()
        }
        guard let taskID = attempt.taskID else { throw ServiceError("Missing transcription task.") }
        // A bounded wait leaves the durable task on disk. Transcribe resumes it after relaunch.
        for _ in 0..<150 {
            try Task.checkCancellation()
            statusMessage = "Transcribing on server…"
            switch try await server.task(id: taskID) {
            case .pending: try await Task.sleep(for: .seconds(2))
            case .failed(let message):
                // The server has a terminal result; a deliberate next Transcribe starts a fresh attempt.
                guard var latest = meetings.first(where: { $0.id == id }) else { return }
                latest.serverTranscription = nil
                errorMessage = nil; updateMeeting(latest)
                if let errorMessage { throw ServiceError(errorMessage) }
                throw ServiceError(message + " Choose Transcribe to start a new attempt.")
            case .complete(let segments):
                guard var latest = meetings.first(where: { $0.id == id }) else { return }
                latest.transcript = segments.map { TranscriptSegment(start: $0.start, end: $0.end, speaker: $0.speaker ?? ($0.track == "mic" ? "You" : "Speaker"), text: $0.text) }
                latest.serverTranscription = nil
                errorMessage = nil; updateMeeting(latest)
                if let errorMessage { throw ServiceError(errorMessage) }
                statusMessage = "Transcription complete"
                return
            }
        }
        statusMessage = "Transcription is still running. Choose Transcribe to resume checking; the server keeps working."
    }
}

func prepareServerAudio(_ file: URL, compressPCM: Bool = true) async throws -> (url: URL, temporary: Bool, channels: Int) {
    let allowed = ["wav", "flac", "mp3", "m4a", "ogg", "opus", "mp4", "webm", "aac"]
    var prepared = file
    let size = try file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
    let suffix = file.pathExtension.lowercased()
    let temporary = !allowed.contains(suffix) || size > 450_000_000 || (compressPCM && ["wav", "aif", "aiff", "caf"].contains(suffix))
    if temporary {
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".m4a")
        do {
            if try !encodeBoundedAAC(file: file, destination: destination) {
                // Apple's M4A preset is a format preset, not a fixed bitrate guarantee.
                // https://developer.apple.com/documentation/avfoundation/avassetexportpresetapplem4a
                guard let exporter = AVAssetExportSession(asset: AVURLAsset(url: file), presetName: AVAssetExportPresetAppleM4A), exporter.supportedFileTypes.contains(.m4a) else { throw ServiceError("Could not convert this audio format for the server.") }
                exporter.outputURL = destination; exporter.outputFileType = .m4a
                await exporter.export()
                guard exporter.status == .completed else { throw exporter.error ?? ServiceError("Audio conversion failed.") }
            }
            try Task.checkCancellation()
            let outputSize = try destination.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
            guard outputSize > 0, outputSize <= 500_000_000 else { throw ServiceError("The converted audio exceeds the server's 500 MB limit. Split this recording before uploading.") }
        } catch {
            try? FileManager.default.removeItem(at: destination)
            throw error
        }
        prepared = destination
    }
    do {
        let audio = try AVAudioFile(forReading: prepared)
        return (prepared, temporary, Int(audio.processingFormat.channelCount))
    } catch {
        // AVAudioFile doesn't decode every server-supported container. AVAsset can inspect video audio tracks.
        let tracks = try await AVURLAsset(url: prepared).loadTracks(withMediaType: .audio)
        guard let track = tracks.first else {
            if temporary { try? FileManager.default.removeItem(at: prepared) }
            throw ServiceError("The selected file contains no audio track.")
        }
        let formats = try await track.load(.formatDescriptions)
        let channels = formats.first.flatMap { CMAudioFormatDescriptionGetStreamBasicDescription($0)?.pointee.mChannelsPerFrame }
        return (prepared, temporary, Int(channels ?? 1))
    }
}

/// Convert ordinary captured PCM in bounded buffers, retaining channel separation,
/// sample rate, and every frame. 64kbps mono/128kbps stereo keeps a one-hour recording
/// near 29/58 MB. The worker independently downmixes each source to 16kHz mono;
/// mic/system must remain separate tracks, not channels of one combined file.
/// Apple audio settings: https://developer.apple.com/documentation/avfoundation/audio-settings
private func encodeBoundedAAC(file: URL, destination: URL) throws -> Bool {
    guard let source = try? AVAudioFile(forReading: file, commonFormat: .pcmFormatFloat32, interleaved: false) else { return false }
    let format = source.processingFormat
    guard (1...2).contains(format.channelCount), [44100.0, 48000.0].contains(format.sampleRate) else { return false }
    let settings: [String: Any] = [AVFormatIDKey: kAudioFormatMPEG4AAC,
        AVSampleRateKey: format.sampleRate, AVNumberOfChannelsKey: format.channelCount,
        AVEncoderBitRateKey: Int(format.channelCount) * 64000]
    var output: AVAudioFile? = try AVAudioFile(forWriting: destination, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
    guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 8192) else { throw ServiceError("Could not allocate the audio conversion buffer.") }
    while source.framePosition < source.length {
        try Task.checkCancellation()
        try source.read(into: buffer)
        guard buffer.frameLength > 0 else { throw ServiceError("Audio conversion ended before the recording was complete.") }
        try output?.write(from: buffer)
    }
    // Releasing AVAudioFile finalizes its AAC packet table before inspection/upload.
    output = nil
    return true
}
