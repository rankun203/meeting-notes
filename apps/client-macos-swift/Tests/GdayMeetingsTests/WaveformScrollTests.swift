import AppKit
import Testing

@testable import GdayMeetings

struct WaveformScrollTests {
    @Test func pansAccumulateWithoutPlaybackTicksMovingTheTarget() {
        var gesture = WaveformScrollGesture()
        let handled1 = gesture.move(x: 20, y: 1, time: 30, duration: 120, width: 240)
        #expect(handled1)
        #expect(gesture.target == 40)
        let handled2 = gesture.move(x: 10, y: 2, time: 31, duration: 120, width: 240)
        #expect(handled2)
        #expect(gesture.target == 45)
        let handled3 = gesture.move(x: -10, y: 0, time: 32, duration: 120, width: 240)
        #expect(handled3)
        #expect(gesture.target == 40)
        gesture.reset()
        #expect(gesture.target == nil)
        let handled4 = gesture.move(x: 10, y: 0, time: 60, duration: 120, width: 240)
        #expect(handled4)
        #expect(gesture.target == 65)
    }
    @Test func verticalScrollingStaysVerticalUntilGestureEnds() {
        var gesture = WaveformScrollGesture()
        let handled5 = !gesture.move(x: -1, y: 20, time: 30, duration: 120, width: 240)
        #expect(handled5)
        let handled6 = !gesture.move(x: -20, y: 1, time: 30, duration: 120, width: 240)
        #expect(handled6)
        #expect(gesture.target == nil)
        gesture.reset()
        let handled7 = gesture.move(x: -20, y: 1, time: 30, duration: 120, width: 240)
        #expect(handled7)
    }
    @Test func boundsAndUnavailableTimelinesAreSafe() {
        var gesture = WaveformScrollGesture()
        let handled8 = !gesture.move(x: -10, y: 0, time: 0, duration: 0, width: 240)
        #expect(handled8)
        let handled9 = !gesture.move(x: .nan, y: 0, time: 0, duration: 120, width: 240)
        #expect(handled9)
        let handled10 = !gesture.move(x: -10, y: 0, time: 0, duration: 120, width: 0)
        #expect(handled10)
        let handled11 = gesture.move(x: -500, y: 0, time: 30, duration: 120, width: 240)
        #expect(handled11)
        #expect(gesture.target == 0)
        let handled12 = gesture.move(x: 500, y: 0, time: 30, duration: 120, width: 240)
        #expect(handled12)
        #expect(gesture.target == 120)
    }
}

@MainActor struct WaveformScrollLifecycleTests {
    @Test func fingerLiftCommitsImmediatelyAndMomentumCommitsItsFinalTarget() {
        let view = WaveformScrollView(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        view.time = 30
        view.duration = 120
        var seeks: [Double] = []
        var scrubs: [Double?] = []
        view.seek = { seeks.append($0) }
        view.scrub = { scrubs.append($0) }
        #expect(view.consume(x: 20, y: 0, phase: .began, momentum: [], precise: true))
        #expect(view.consume(x: 0, y: 0, phase: .ended, momentum: [], precise: true))
        #expect(seeks == [40])
        #expect(view.consume(x: 10, y: 0, phase: [], momentum: .began, precise: true))
        #expect(view.consume(x: 0, y: 0, phase: [], momentum: .ended, precise: true))
        #expect(seeks == [40, 45])
        #expect(scrubs.last! == nil)
        #expect(view.consume(x: -10, y: 0, phase: .began, momentum: [], precise: true))
        #expect(view.consume(x: 0, y: 0, phase: .cancelled, momentum: [], precise: true))
        #expect(seeks == [40, 45])
        #expect(scrubs.last! == nil)
        #expect(!view.consume(x: 10, y: 0, phase: [], momentum: .began, precise: true))
        view.stop()
    }
    @Test func fingerLiftDoesNotRepeatSeekAfterMomentumGracePeriod() async throws {
        let view = WaveformScrollView(frame: NSRect(x: 0, y: 0, width: 500, height: 24))
        view.time = 3600
        view.duration = 7200
        var seeks: [Double] = []
        view.seek = { seeks.append($0) }
        #expect(view.consume(x: 5, y: 0, phase: .began, momentum: [], precise: true))
        #expect(view.consume(x: 0, y: 0, phase: .ended, momentum: [], precise: true))
        #expect(seeks == [3672])
        try await Task.sleep(for: .milliseconds(200))
        #expect(seeks == [3672])
        view.stop()
    }
    @Test func disablingCancelsPendingScrub() {
        let view = WaveformScrollView(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        view.time = 30
        view.duration = 120
        var seeks: [Double] = []
        var position: Double?
        view.seek = { seeks.append($0) }
        view.scrub = { position = $0 }
        #expect(view.consume(x: 20, y: 0, phase: .began, momentum: [], precise: true))
        #expect(position == 40)
        view.enabled = false
        #expect(position == nil)
        #expect(seeks.isEmpty)
        view.stop()
    }
}
