import Combine
import CoreAudio

/// A connected input device. `uid` persists across reconnection and restarts;
/// `id` is only valid while the device stays connected.
struct AudioInputDevice: Equatable, Identifiable {
    var id: AudioObjectID
    var uid: String
    var name: String
}

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
    static func deviceName(_ device: AudioObjectID) -> String? { stringProperty(device, kAudioObjectPropertyName) }

    private static func stringProperty(_ device: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var name: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &name) == noErr, let name else {
            return nil
        }
        return name.takeRetainedValue() as String
    }

    /// Physical and virtual devices with input streams, in Core Audio order.
    /// Aggregates are excluded: capture's own tap aggregate and VoiceProcessingIO's
    /// aggregate appear here while recording, and neither is a microphone.
    static func inputDevices(read: PropertyReader = readProperty, uid: (AudioObjectID) -> String? = deviceUID)
        -> [AudioInputDevice]
    {
        let system = AudioObjectID(kAudioObjectSystemObject)
        return (read(system, kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal) ?? []).compactMap {
            device in
            let transport = read(device, kAudioDevicePropertyTransportType, kAudioObjectPropertyScopeGlobal)?.first
            guard transport != kAudioDeviceTransportTypeAggregate, transport != kAudioDeviceTransportTypeAutoAggregate,
                read(device, kAudioDevicePropertyIsHidden, kAudioObjectPropertyScopeGlobal)?.first ?? 0 == 0,
                !(read(device, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput) ?? []).isEmpty,
                let identifier = uid(device)
            else { return nil }
            return AudioInputDevice(id: device, uid: identifier, name: deviceName(device) ?? identifier)
        }
    }

    /// The connected input device with this persistent UID, if any.
    static func inputDevice(uid: String) -> AudioInputDevice? { inputDevices().first { $0.uid == uid } }

    static func deviceUID(_ device: AudioObjectID) -> String? {
        stringProperty(device, kAudioDevicePropertyDeviceUID)
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

/// A device a capture source records from, identified for status text.
struct AudioDeviceIdentity: Equatable {
    var id: AudioObjectID
    var name: String

    /// The current default input or output, when it has a name.
    static func current(output: Bool) -> AudioDeviceIdentity? {
        guard let device = RecordingAudioRoute.defaultDevice(output: output),
            let name = RecordingAudioRoute.deviceName(device)
        else { return nil }
        return AudioDeviceIdentity(id: device, name: name)
    }
}

/// Whether the default output is a speaker, tracked during recording. The same
/// device can change route without a new default device ID, for example
/// headphones plugged into a built-in jack change its data source or stream
/// terminal types. Such a change re-decides automatic voice processing like a
/// new default output does.
struct OutputSpeakerRoute: Equatable {
    private(set) var speaker: Bool

    enum Change: Equatable {
        /// The route is still speaker or still non-speaker; nothing to decide.
        case unchanged
        case changed(rebuildMicrophone: Bool)
    }

    /// Re-reads the default output. Explicit On or Off, including the echo
    /// latch (which sets On), holds for the session, so only the automatic
    /// policy can rebuild the microphone here.
    mutating func refresh(
        policy: VoiceProcessingPolicy, voiceProcessing: Bool,
        read: RecordingAudioRoute.PropertyReader = RecordingAudioRoute.readProperty
    ) -> Change {
        let now = RecordingAudioRoute.defaultVoiceProcessing(read: read)
        guard now != speaker else { return .unchanged }
        speaker = now
        guard policy == .automatic else { return .changed(rebuildMicrophone: false) }
        return .changed(
            rebuildMicrophone: Self.rebuildsMicrophone(policy: policy, voiceProcessing: voiceProcessing, speaker: now))
    }

    /// Voice processing couples to the output device, so a processed engine
    /// moves with it. An unprocessed engine rebuilds only when automatic mode
    /// now selects processing; headphone-to-headphone changes keep it running.
    static func rebuildsMicrophone(policy: VoiceProcessingPolicy, voiceProcessing: Bool, speaker: Bool) -> Bool {
        voiceProcessing || (policy == .automatic && speaker)
    }
}

extension VoiceProcessingPolicy {
    /// Automatic follows the current output on every microphone rebuild; an
    /// explicit choice holds for the rest of the session.
    func enabled(speakerRoute: () -> Bool = { RecordingAudioRoute.defaultVoiceProcessing() }) -> Bool {
        switch self {
        case .automatic: return speakerRoute()
        case .on: return true
        case .off: return false
        }
    }
}

/// Keeps New Recording's microphone menu current without opening audio
/// devices: the device list and the default input's name.
@MainActor
final class MicrophoneDeviceObserver: ObservableObject {
    @Published private(set) var devices: [AudioInputDevice] = []
    @Published private(set) var defaultName: String?
    private var listeners: [(AudioObjectPropertyAddress, AudioObjectPropertyListenerBlock)] = []

    func start() {
        stop()
        refresh()
        for selector in [kAudioHardwarePropertyDevices, kAudioHardwarePropertyDefaultInputDevice] {
            var address = AudioObjectPropertyAddress(
                mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
            let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
                Task { @MainActor [weak self] in
                    guard let self, !self.listeners.isEmpty else { return }
                    self.refresh()
                }
            }
            if AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, .main, listener)
                == noErr
            {
                listeners.append((address, listener))
            }
        }
    }

    func stop() {
        for (var address, listener) in listeners {
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, .main, listener)
        }
        listeners.removeAll()
    }

    private func refresh() {
        devices = RecordingAudioRoute.inputDevices()
        defaultName = RecordingAudioRoute.defaultDevice(output: false).flatMap(RecordingAudioRoute.deviceName)
    }
}
