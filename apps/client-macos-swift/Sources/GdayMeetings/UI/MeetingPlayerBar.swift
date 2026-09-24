import SwiftUI

/// HIG Playing Audio: familiar transport controls stay available while people
/// navigate and work. The app owns playback; this view only presents its state.
/// https://developer.apple.com/design/human-interface-guidelines/playing-audio
struct MeetingPlayerBar: View {
    @EnvironmentObject private var playback: MeetingPlayback
    let showMeeting: (UUID) -> Void
    @ViewState private var tracksExpanded = false

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: 14) {
                Button {
                    if let id = playback.meetingID { showMeeting(id) }
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "waveform")
                            .font(.title2).foregroundStyle(.tint)
                            .frame(width: 44, height: 44)
                            .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 9))
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(playback.title).font(.callout.weight(.semibold)).lineLimit(1)
                            Text(playback.isLoading ? "Preparing audio…" : playback.isPlaying ? "Now Playing" : playback.hasEnded ? "Finished" : "Paused")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .frame(minWidth: 150, idealWidth: 180, maxWidth: 220)
                .help("Show the meeting that is playing")
                .accessibilityLabel("Show meeting: \(playback.title)")

                HStack(spacing: 9) {
                    transportButton("Back 15 Seconds", symbol: "gobackward.15") { playback.skip(by: -15) }
                    Button { playback.togglePlayPause() } label: {
                        Group {
                            if playback.isLoading { ProgressView().controlSize(.small) }
                            else { Image(systemName: playback.isPlaying ? "pause.fill" : "play.fill").font(.title2) }
                        }.frame(width: 38, height: 38).contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(playback.isPlaying ? "Pause" : "Play")
                    .help(playback.isPlaying ? "Pause playback" : "Play recording")
                    .disabled(playback.isPlaybackBlocked || playback.isLoading)
                    transportButton("Forward 15 Seconds", symbol: "goforward.15") { playback.skip(by: 15) }
                }

                PlaybackPosition(progress: playback.progress, waveforms: audibleWaveforms,
                                 duration: playback.duration, dimmed: playback.mutedTracks.count == playback.trackNames.count,
                                 showsTimes: true, seek: playback.seek)
                    .disabled(playback.isLoading || playback.duration <= 0 || playback.isPlaybackBlocked)
                    .frame(minWidth: 130, maxWidth: .infinity)

                HStack(spacing: 10) {
                    Button { tracksExpanded.toggle() } label: {
                        Image(systemName: tracksExpanded ? "chevron.down" : "waveform")
                            .frame(width: 28, height: 30)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(tracksExpanded ? "Hide audio tracks" : "Show audio tracks")
                    .help(tracksExpanded ? "Hide audio tracks" : "Show audio tracks")
                    Menu {
                        Picker("Playback Speed", selection: Binding(get: { playback.playbackRate }, set: { playback.setRate($0) })) {
                            ForEach([0.75, 1, 1.25, 1.5, 2], id: \.self) { rate in Text("\(rate.formatted())×").tag(rate) }
                        }
                    } label: { Text("\(playback.playbackRate.formatted())×").monospacedDigit() }
                    .menuStyle(.borderlessButton).fixedSize()
                    .accessibilityLabel("Playback speed")
                    .help("Playback speed")

                    Menu {
                        Picker("Audio Track", selection: Binding(get: { playback.selectedTrack }, set: { playback.selectTrack($0) })) {
                            Text("All Tracks").tag(-1)
                            ForEach(Array(playback.trackNames.enumerated()), id: \.offset) { index, name in Text(name).tag(index) }
                        }
                        Divider()
                        Button("Close Player", systemImage: "xmark") { playback.clear() }
                    } label: { Label(selectedTrackName, systemImage: "slider.horizontal.3").lineLimit(1) }
                    .menuStyle(.borderlessButton).frame(maxWidth: 140)
                    .accessibilityLabel("Audio track and player options")
                    .accessibilityValue(selectedTrackName)
                    .help("Choose microphone, system audio, or all tracks")
                }
            }
            .padding(.horizontal, 20).padding(.vertical, 18)
            if tracksExpanded {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(Array(playback.trackNames.enumerated()), id: \.offset) { index, name in
                            HStack(spacing: 14) {
                                HStack {
                                    Text(name).font(.caption).lineLimit(1)
                                    Spacer()
                                    Button { playback.toggleMute(index) } label: {
                                        Image(systemName: playback.mutedTracks.contains(index) ? "speaker.slash" : "speaker.wave.2")
                                            .frame(width: 28, height: 28)
                                    }.buttonStyle(.plain)
                                        .accessibilityLabel("\(playback.mutedTracks.contains(index) ? "Unmute" : "Mute") \(name)")
                                        .help("\(playback.mutedTracks.contains(index) ? "Unmute" : "Mute") \(name)")
                                        .disabled(playback.isLoading || playback.isPlaybackBlocked)
                                }.frame(width: 160)
                                PlaybackPosition(
                                    progress: playback.progress,
                                    waveforms: playback.waveforms.indices.contains(index) ? [playback.waveforms[index]].compactMap { $0 } : [],
                                    duration: playback.duration,
                                    label: "\(name) playback position",
                                    dimmed: playback.mutedTracks.contains(index), seek: playback.seek)
                                    .disabled(playback.isLoading || playback.duration <= 0 || playback.isPlaybackBlocked)
                            }
                        }
                    }.padding(.horizontal, 20).padding(.bottom, 12)
                }.frame(height: min(200, CGFloat(playback.trackNames.count) * 46 + 12))
            }
            if let error = playback.errorMessage {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20).padding(.bottom, 10)
            }
        }
        .background(.bar)
        .onChange(of: playback.meetingID) { _, _ in tracksExpanded = false }
    }

    private var audibleWaveforms: [AudioWaveform] {
        if playback.mutedTracks.count == playback.trackNames.count { return playback.waveforms.compactMap { $0 } }
        return playback.waveforms.enumerated().compactMap { playback.mutedTracks.contains($0.offset) ? nil : $0.element }
    }

    private var selectedTrackName: String {
        if playback.mutedTracks.count == playback.trackNames.count { return "All Muted" }
        if playback.selectedTrack < 0 && !playback.mutedTracks.isEmpty { return "Custom Mix" }
        return playback.trackNames.indices.contains(playback.selectedTrack) ? playback.trackNames[playback.selectedTrack] : "All Tracks"
    }

    private func transportButton(_ title: String, symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol).font(.title3).frame(width: 30, height: 34).contentShape(Rectangle()) }
            .buttonStyle(.plain).accessibilityLabel(title).help(title)
            .disabled(playback.isLoading || playback.isPlaybackBlocked || playback.duration <= 0)
    }
}

func playbackTime(_ seconds: Double) -> String {
    guard seconds.isFinite else { return "0:00" }
    let total = Int(max(0, min(seconds, Double(Int.max / 2))))
    if total >= 3600 { return String(format: "%d:%02d:%02d", total / 3600, total / 60 % 60, total % 60) }
    return String(format: "%d:%02d", total / 60, total % 60)
}
