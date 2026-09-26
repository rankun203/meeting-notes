import AppKit
import SwiftUI

/// Native scroll input leaves clicks and drags with the existing SwiftUI target.
/// Vertical gestures continue through the responder chain.
struct WaveformScrollInput: NSViewRepresentable {
    @Environment(\.isEnabled) private var isEnabled
    let time: Double
    let duration: Double
    let seek: (Double) -> Void
    let scrub: (Double?) -> Void

    func makeNSView(context: Context) -> WaveformScrollView { WaveformScrollView() }
    func updateNSView(_ view: WaveformScrollView, context: Context) {
        view.enabled = isEnabled
        view.time = time
        view.duration = duration
        view.seek = seek
        view.scrub = scrub
    }
    static func dismantleNSView(_ view: WaveformScrollView, coordinator: ()) { view.stop() }
}

struct WaveformScrollGesture {
    private(set) var target: Double?
    private var vertical = false

    mutating func move(x: Double, y: Double, time: Double, duration: Double, width: Double) -> Bool {
        guard x.isFinite, y.isFinite, time.isFinite, duration.isFinite, duration > 0,
            width.isFinite, width > 0, !vertical
        else { return false }
        if target == nil {
            guard abs(x) > abs(y) else {
                if y != 0 { vertical = true }
                return false
            }
            target = min(duration, max(0, time))
        }
        // Pan the playhead in the gesture’s direction, rather than scrolling content.
        target = min(duration, max(0, target! + x / width * duration))
        return true
    }

    mutating func reset() { self = Self() }
}

final class WaveformScrollView: NSView {
    var enabled = true {
        didSet { if !enabled { finish(commit: false) } }
    }
    var time: Double = 0
    var duration: Double = 0
    var seek: (Double) -> Void = { _ in }
    var scrub: (Double?) -> Void = { _ in }
    private var gesture = WaveformScrollGesture()
    private var monitor: Any?
    private var completion: DispatchWorkItem?
    private var committedTarget: Double?
    private static weak var owner: WaveformScrollView?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        setAccessibilityElement(false)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard enabled, NSApp.currentEvent?.type == .scrollWheel else { return nil }
        return super.hitTest(point)
    }
    override func scrollWheel(with event: NSEvent) {
        if !handle(event) { super.scrollWheel(with: event) }
    }
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        stop()
        guard window != nil else { return }
        // The SwiftUI gesture sibling can own native hit testing. Observe only
        // this app's scroll events, then restrict them to the visible waveform.
        // https://developer.apple.com/documentation/appkit/nsevent/addlocalmonitorforevents(matching:handler:)
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel, .leftMouseDown, .keyDown]) {
            [weak self] event in
            guard let self else { return event }
            if event.type != .scrollWheel {
                self.finish(commit: false)
                return event
            }
            return self.handle(event) ? nil : event
        }
    }

    private func handle(_ event: NSEvent) -> Bool {
        guard enabled, event.window === window, window != nil, !isHiddenOrHasHiddenAncestor,
            window?.attachedSheet == nil,
            Self.owner == nil || Self.owner === self
        else { return false }
        let active = gesture.target != nil
        guard active || bounds.intersection(visibleRect).contains(convert(event.locationInWindow, from: nil)) else {
            return false
        }
        return consume(
            x: Double(event.scrollingDeltaX), y: Double(event.scrollingDeltaY),
            phase: event.phase, momentum: event.momentumPhase, precise: event.hasPreciseScrollingDeltas)
    }

    func consume(x: Double, y: Double, phase: NSEvent.Phase, momentum: NSEvent.Phase, precise: Bool) -> Bool {
        let active = gesture.target != nil
        if phase.contains(.cancelled) || momentum.contains(.cancelled) {
            finish(commit: false)
            return active
        }
        if phase.contains(.began) {
            finish(commit: true)
        }
        // Never adopt momentum from a gesture that began in another control.
        guard momentum.isEmpty || gesture.target != nil else { return false }
        let scale = precise ? 1.0 : 10.0
        let handled = gesture.move(
            x: x * scale, y: y * scale,
            time: time, duration: duration, width: bounds.width)
        if handled {
            Self.owner = self
            scrub(gesture.target)
        }
        completion?.cancel()
        if momentum.contains(.ended) {
            finish(commit: true)
        }
        else if phase.contains(.ended) || (phase.isEmpty && momentum.isEmpty) {
            // Update playback as soon as fingers lift. Retain the gesture briefly
            // so momentum can continue from the same target without another seek.
            if phase.contains(.ended) { commitTarget() }
            // Phase-less wheels still coalesce until their events stop arriving.
            let work = DispatchWorkItem { [weak self] in self?.finish(commit: true) }
            completion = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15, execute: work)
        }
        return handled || active
    }

    private func finish(commit: Bool) {
        completion?.cancel()
        completion = nil
        let target = gesture.target
        if commit { commitTarget() }
        gesture.reset()
        committedTarget = nil
        if Self.owner === self { Self.owner = nil }
        if target != nil {
            scrub(nil)
        }
    }

    private func commitTarget() {
        guard let target = gesture.target, target != committedTarget else { return }
        committedTarget = target
        seek(target)
        scrub(nil)
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        finish(commit: false)
    }
    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
        completion?.cancel()
    }
}
