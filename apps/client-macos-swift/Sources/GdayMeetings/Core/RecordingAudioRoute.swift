import Combine
import CoreAudio

/// Query before capture creates its private aggregate devices. Do not infer
/// speakers from a device name or transport: USB and Bluetooth can carry either.
enum RecordingAudioRoute {
    typealias PropertyReader = (AudioObjectID, AudioObjectPropertySelector, AudioObjectPropertyScope) -> [UInt32]?

    static func defaultVoiceProcessing(read: PropertyReader = readProperty) -> Bool {
        guard
            let device = read(
                AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice,
                kAudioObjectPropertyScopeGlobal)?.first,
            device != kAudioObjectUnknown,
            let streams = read(device, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput)
        else { return false }
        // Core Audio's terminal type describes the endpoint, unlike transport type.
        // https://developer.apple.com/documentation/coreaudio/kaudiostreampropertyterminaltype
        return streams.contains { stream in
            guard let terminal = read(stream, kAudioStreamPropertyTerminalType, kAudioObjectPropertyScopeGlobal)?.first
            else { return false }
            switch terminal {
            case kAudioStreamTerminalTypeSpeaker, kAudioStreamTerminalTypeLFESpeaker:
                return true
            // Some drivers (including this Mac's built-in output) expose the
            // numeric USB/IOAudioFamily terminal codes through this HAL property.
            // Apple IOAudioTypes.h: OUTPUT_SPEAKER, DESKTOP, ROOM, COMMUNICATION, LFE.
            // https://github.com/apple-oss-distributions/IOAudioFamily/blob/main/IOAudioTypes.h
            case 0x0301, 0x0304, 0x0305, 0x0306, 0x0307:
                return true
            default:
                return false
            }
        }
    }

    /// The current default input or output device, read fresh on each call so
    /// recovery follows the latest macOS selection.
    static func defaultDevice(output: Bool) -> AudioObjectID? {
        let selector = output ? kAudioHardwarePropertyDefaultOutputDevice : kAudioHardwarePropertyDefaultInputDevice
        guard
            let device = readProperty(
                object: AudioObjectID(kAudioObjectSystemObject), selector: selector,
                scope: kAudioObjectPropertyScopeGlobal)?.first, device != kAudioObjectUnknown
        else { return nil }
        return device
    }

    /// The device's display name for recording metadata, when the driver provides one.
    static func deviceName(_ device: AudioObjectID) -> String? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioObjectPropertyName, mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        var name: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &name) == noErr, let name else {
            return nil
        }
        return name.takeRetainedValue() as String
    }

    static func readProperty(
        object: AudioObjectID, selector: AudioObjectPropertySelector, scope: AudioObjectPropertyScope
    ) -> [UInt32]? {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(object, &address, 0, nil, &size) == noErr,
            size > 0, size % UInt32(MemoryLayout<UInt32>.size) == 0
        else { return nil }
        var values = [UInt32](repeating: 0, count: Int(size) / MemoryLayout<UInt32>.size)
        let status = values.withUnsafeMutableBytes {
            AudioObjectGetPropertyData(object, &address, 0, nil, &size, $0.baseAddress!)
        }
        guard status == noErr else { return nil }
        return Array(values.prefix(Int(size) / MemoryLayout<UInt32>.size))
    }
}

extension VoiceProcessingPolicy {
    /// Automatic follows the current output on every microphone rebuild; an
    /// explicit choice from New Recording holds for the whole session.
    init(override: Bool?) {
        switch override {
        case nil: self = .automatic
        case true?: self = .on
        case false?: self = .off
        }
    }

    func enabled(speakerRoute: () -> Bool = { RecordingAudioRoute.defaultVoiceProcessing() }) -> Bool {
        switch self {
        case .automatic: return speakerRoute()
        case .on: return true
        case .off: return false
        }
    }
}

/// Keep the setup default current without opening audio devices. Capture checks
/// the route again after permission prompts; an explicit user override wins.
@MainActor
final class RecordingRouteObserver: ObservableObject {
    @Published private(set) var voiceProcessing = false
    private var listeners: [(AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []

    func start() {
        stop()
        voiceProcessing = RecordingAudioRoute.defaultVoiceProcessing()
        observe(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice)
        guard
            let device = RecordingAudioRoute.readProperty(
                object: AudioObjectID(kAudioObjectSystemObject), selector: kAudioHardwarePropertyDefaultOutputDevice,
                scope: kAudioObjectPropertyScopeGlobal
            )?.first, device != kAudioObjectUnknown
        else { return }
        observe(device, kAudioDevicePropertyDataSource, scope: kAudioObjectPropertyScopeOutput)
        observe(device, kAudioDevicePropertyStreams, scope: kAudioObjectPropertyScopeOutput)
        for stream in RecordingAudioRoute.readProperty(
            object: device, selector: kAudioDevicePropertyStreams, scope: kAudioObjectPropertyScopeOutput
        ) ?? [] {
            observe(stream, kAudioStreamPropertyTerminalType)
        }
    }

    func stop() {
        for (object, var address, listener) in listeners {
            AudioObjectRemovePropertyListenerBlock(object, &address, .main, listener)
        }
        listeners.removeAll()
    }

    private func observe(
        _ object: AudioObjectID, _ selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal
    ) {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectHasProperty(object, &address) else { return }
        let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            Task { @MainActor [weak self] in
                guard let self, !self.listeners.isEmpty else { return }
                self.start()
            }
        }
        if AudioObjectAddPropertyListenerBlock(object, &address, .main, listener) == noErr {
            listeners.append((object, address, listener))
        }
    }
}
