import AppKit
import QuartzCore
import SwiftUI

/// SwiftUI handles interaction/accessibility; display callbacks only update layers.
struct PlaybackWaveformSurface: NSViewRepresentable {
    let waveforms: [AudioWaveform]
    let duration: Double
    let progress: PlaybackProgress
    // A value input makes paused seeks/scrubs invalidate the native surface,
    // even when its shared progress object and animation state are unchanged.
    let time: Double
    let animate: Bool

    func makeNSView(context: Context) -> PlaybackWaveformView { PlaybackWaveformView() }
    func updateNSView(_ view: PlaybackWaveformView, context: Context) {
        view.configure(waveforms: waveforms, duration: duration, progress: progress, animate: animate)
    }
    static func dismantleNSView(_ view: PlaybackWaveformView, coordinator: ()) { view.stop() }
}

final class PlaybackWaveformView: NSView {
    private let unplayed = CAShapeLayer()
    private let played = CAShapeLayer()
    private let reveal = CALayer()
    private let cursor = CALayer()
    private var waveforms: [AudioWaveform] = []
    private var duration: Double = 0
    private weak var progress: PlaybackProgress?
    private var animate = false
    private var link: CADisplayLink?
    private var geometryDirty = true
    private var drawnSize = CGSize.zero

    // Avoid a display-link/target retain cycle, including when a window closes.
    private final class TickTarget: NSObject {
        weak var view: PlaybackWaveformView?
        @objc func tick(_ link: CADisplayLink) { view?.updatePosition() }
    }
    private let tickTarget = TickTarget()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.addSublayer(unplayed)
        layer?.addSublayer(played)
        layer?.addSublayer(cursor)
        reveal.backgroundColor = NSColor.white.cgColor
        played.mask = reveal
        tickTarget.view = self
        setAccessibilityElement(false)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    deinit { link?.invalidate() }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func configure(waveforms: [AudioWaveform], duration: Double, progress: PlaybackProgress, animate: Bool) {
        if self.waveforms != waveforms || self.duration != duration {
            self.waveforms = waveforms
            self.duration = duration
            geometryDirty = true
            needsLayout = true
        }
        self.progress = progress
        self.animate = animate
        updateClock()
        updatePosition()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateClock()
    }
    private func updateClock() {
        guard animate, window != nil else {
            stop()
            return
        }
        guard link == nil else { return }
        // Native pacing follows this view across displays; no preferred FPS cap.
        let link = displayLink(target: tickTarget, selector: #selector(TickTarget.tick(_:)))
        self.link = link
        link.add(to: .main, forMode: .common)
    }
    func stop() {
        link?.invalidate()
        link = nil
    }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if geometryDirty || drawnSize != bounds.size {
            drawnSize = bounds.size
            geometryDirty = false
            let columns = max(1, Int(bounds.width / 3))
            let normalizer = max(0.01, waveforms.flatMap(\.peaks).max() ?? 1)
            let bars = CGMutablePath()
            for column in 0..<columns {
                let start = Double(column) / Double(columns) * duration
                let end = Double(column + 1) / Double(columns) * duration
                let peak = waveforms.map { $0.peak(from: start, to: end) }.max() ?? 0
                let height = max(1, CGFloat(sqrt(peak / normalizer)) * (bounds.height - 6))
                bars.addRoundedRect(
                    in: CGRect(
                        x: CGFloat(column) * bounds.width / CGFloat(columns), y: (bounds.height - height) / 2,
                        width: 2, height: height), cornerWidth: 1, cornerHeight: 1)
            }
            unplayed.path = bars
            played.path = bars
            unplayed.frame = bounds
            played.frame = bounds
        }
        updateColors()
        CATransaction.commit()
        updatePosition()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsLayout = true
    }
    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            unplayed.fillColor = NSColor.secondaryLabelColor.withAlphaComponent(0.55).cgColor
            played.fillColor = NSColor.controlAccentColor.cgColor
            cursor.backgroundColor = NSColor.labelColor.cgColor
        }
    }
    private func updatePosition() {
        guard let progress else { return }
        let time = animate ? progress.animatedTime() : progress.displayedTime
        let fraction = min(1, max(0, time / max(0.01, duration)))
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        reveal.frame = CGRect(x: 0, y: 0, width: bounds.width * fraction, height: bounds.height)
        cursor.frame = CGRect(x: fraction * max(0, bounds.width - 1), y: 0, width: 1.5, height: bounds.height)
        CATransaction.commit()
    }
}
