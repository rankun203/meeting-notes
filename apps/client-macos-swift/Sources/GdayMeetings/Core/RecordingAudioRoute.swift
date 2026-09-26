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
            return terminal == kAudioStreamTerminalTypeSpeaker || terminal == kAudioStreamTerminalTypeLFESpeaker
        }
    }

    private static func readProperty(
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
