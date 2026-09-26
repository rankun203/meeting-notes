import CoreAudio
import Foundation
import Testing

@testable import GdayMeetings

struct RecordingAudioRouteTests {
    @Test func defaultsFollowEachNewOutputRoute() {
        var terminals = [kAudioStreamTerminalTypeSpeaker]
        let read: RecordingAudioRoute.PropertyReader = { object, selector, scope in
            switch selector {
            case kAudioHardwarePropertyDefaultOutputDevice:
                #expect(object == kAudioObjectSystemObject)
                return [100]
            case kAudioDevicePropertyStreams:
                #expect(object == 100)
                #expect(scope == kAudioObjectPropertyScopeOutput)
                return terminals.indices.map { UInt32($0 + 200) }
            case kAudioStreamPropertyTerminalType:
                return [terminals[Int(object) - 200]]
            default: return nil
            }
        }
        #expect(RecordingAudioRoute.defaultVoiceProcessing(read: read))
        for terminal in [
            kAudioStreamTerminalTypeHeadphones, kAudioStreamTerminalTypeUnknown,
            kAudioStreamTerminalTypeLine, kAudioStreamTerminalTypeDigitalAudioInterface,
            kAudioStreamTerminalTypeHDMI, kAudioStreamTerminalTypeDisplayPort,
            kAudioStreamTerminalTypeReceiverSpeaker,
            0x0300, 0x0302, 0x0303, 0x0402, 0x0603,
        ] {
            terminals = [terminal]
            #expect(!RecordingAudioRoute.defaultVoiceProcessing(read: read))
        }
        terminals = [kAudioStreamTerminalTypeHeadphones, kAudioStreamTerminalTypeSpeaker]
        #expect(RecordingAudioRoute.defaultVoiceProcessing(read: read))
        terminals = [kAudioStreamTerminalTypeLFESpeaker]
        #expect(RecordingAudioRoute.defaultVoiceProcessing(read: read))
        // Regression: the built-in speaker returned 769 (0x0301), not 'spkr'.
        for terminal: UInt32 in [0x0301, 0x0304, 0x0305, 0x0306, 0x0307] {
            terminals = [terminal]
            #expect(RecordingAudioRoute.defaultVoiceProcessing(read: read))
        }
        terminals = []
        #expect(!RecordingAudioRoute.defaultVoiceProcessing(read: read))
        #expect(!RecordingAudioRoute.defaultVoiceProcessing(read: { _, _, _ in nil }))
        #expect(!RecordingAudioRoute.defaultVoiceProcessing(read: { _, _, _ in [kAudioObjectUnknown] }))
    }

    @Test(arguments: [true, false])
    func legacyPreferenceIsIgnoredAndNoLongerSaved(enabled: Bool) throws {
        let data = Data("{\"microphoneVoiceProcessing\":\(enabled),\"captureMicrophone\":false}".utf8)
        let settings = try JSONDecoder().decode(AppSettings.self, from: data)
        #expect(!settings.captureMicrophone)
        let saved = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(settings)) as? [String: Any])
        #expect(saved["microphoneVoiceProcessing"] == nil)
    }
}
