import SwiftUI

/// Activity, not speaker diarization. All sources use the same meeting timeline.
/// HIG Accessibility: seeking supports pointer, keyboard, and adjustable actions.
/// https://developer.apple.com/design/human-interface-guidelines/accessibility
struct WaveformTimeline: View {
    let waveforms: [AudioWaveform]
    let duration: Double
    let time: Double
    var label = "Playback position"
    var dimmed = false
    var isLoading = false
    let seek: (Double) -> Void
    let scrub: (Double?) -> Void
    @ViewState private var isScrubbing = false
    @FocusState private var focused: Bool

    var body: some View {
        GeometryReader { geometry in
            Canvas { context, size in
                let position = time
                let columns = max(1, Int(size.width / 3))
                let normalizer = max(0.01, waveforms.flatMap(\.peaks).max() ?? 1)
                for column in 0..<columns {
                    let start = Double(column) / Double(columns) * duration
                    let end = Double(column + 1) / Double(columns) * duration
                    let peak = waveforms.map { $0.peak(from: start, to: end) }.max() ?? 0
                    let height = max(1, CGFloat(sqrt(peak / normalizer)) * (size.height - 6))
                    let rect = CGRect(x: CGFloat(column) * size.width / CGFloat(columns), y: (size.height - height) / 2, width: 2, height: height)
                    context.fill(Path(roundedRect: rect, cornerRadius: 1), with: .color(start < position ? .accentColor : .secondary.opacity(0.55)))
                }
                let x = CGFloat(min(1, max(0, position / max(0.01, duration)))) * max(0, size.width - 1)
                context.fill(Path(CGRect(x: x, y: 0, width: 1.5, height: size.height)), with: .color(.primary))
            }
            .opacity(dimmed ? 0.55 : 1)
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { value in
                    focused = true
                    isScrubbing = true
                    scrub(min(duration, max(0, value.location.x / max(1, geometry.size.width) * duration)))
                }
                .onEnded { value in
                    seek(min(duration, max(0, value.location.x / max(1, geometry.size.width) * duration)))
                    scrub(nil); isScrubbing = false
                })
        }
        .frame(height: 24)
        .modifier(ActionHover(pressed: isScrubbing, cornerRadius: 4))
        .overlay(alignment: .center) {
            if waveforms.isEmpty { Text(isLoading ? "Loading waveform…" : "Waveform unavailable").font(.caption2).foregroundStyle(.secondary).allowsHitTesting(false) }
        }
        .focusable().focused($focused)
        .overlay(RoundedRectangle(cornerRadius: 4).stroke(focused ? Color.accentColor : .clear, lineWidth: 2))
        .onKeyPress(.leftArrow) { seek(max(0, time - 5)); return .handled }
        .onKeyPress(.rightArrow) { seek(min(duration, time + 5)); return .handled }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityValue("\(playbackTime(time)) of \(playbackTime(duration))")
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: seek(min(duration, time + 5))
            case .decrement: seek(max(0, time - 5))
            @unknown default: break
            }
        }
        .help("Click or drag to seek. Arrow keys move five seconds.")
        .onDisappear { if isScrubbing { scrub(nil) } }
    }
}

/// The only observer of frequent progress updates. Controls outside this view
/// retain their identity while its playhead and timestamps advance.
struct PlaybackPosition: View {
    @ObservedObject var progress: PlaybackProgress
    let waveforms: [AudioWaveform]
    let duration: Double
    var label = "Playback position"
    var dimmed = false
    var showsTimes = false
    var isLoading = false
    let seek: (Double) -> Void

    var body: some View {
        WaveformTimeline(waveforms: waveforms, duration: duration, time: progress.displayedTime,
                         label: label, dimmed: dimmed, isLoading: isLoading, seek: seek, scrub: progress.scrub)
        .overlay(alignment: .bottom) {
            if showsTimes {
                HStack {
                    Text(playbackTime(progress.displayedTime))
                    Spacer()
                    Text("−" + playbackTime(max(0, duration - progress.displayedTime)))
                }.font(.caption2).monospacedDigit().foregroundStyle(.secondary)
                    .offset(y: 18)
            }
        }
    }
}
