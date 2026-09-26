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
        // The legacy per-recording preference never turns the automatic setting off.
        #expect(settings.automaticVoiceProcessing)
        #expect(settings.microphoneDevice == nil)
    }

    @Test func microphoneSettingsRoundTripAndTolerateDamage() throws {
        var settings = AppSettings()
        settings.automaticVoiceProcessing = false
        settings.microphoneDevice = MicrophoneDeviceChoice(uid: "usb-1", name: "USB Headset")
        let decoded = try JSONDecoder().decode(AppSettings.self, from: JSONEncoder().encode(settings))
        #expect(decoded == settings)
        // A malformed saved microphone falls back to System Default instead of losing all settings.
        let damaged = Data("{\"microphoneDevice\":{\"uid\":3},\"captureMicrophone\":false}".utf8)
        let recovered = try JSONDecoder().decode(AppSettings.self, from: damaged)
        #expect(recovered.microphoneDevice == nil)
        #expect(!recovered.captureMicrophone)
    }

    @Test func inputDevicesExcludeAggregatesHiddenAndOutputOnly() {
        // 1 built-in mic, 2 speakers (no input), 3 private tap aggregate,
        // 4 VoiceProcessingIO auto-aggregate, 5 hidden, 6 USB headset, 7 no UID.
        let read: RecordingAudioRoute.PropertyReader = { object, selector, scope in
            switch selector {
            case kAudioHardwarePropertyDevices: return [1, 2, 3, 4, 5, 6, 7]
            case kAudioDevicePropertyTransportType:
                switch object {
                case 3: return [kAudioDeviceTransportTypeAggregate]
                case 4: return [kAudioDeviceTransportTypeAutoAggregate]
                case 6: return [kAudioDeviceTransportTypeUSB]
                default: return [kAudioDeviceTransportTypeBuiltIn]
                }
            case kAudioDevicePropertyIsHidden: return [object == 5 ? 1 : 0]
            case kAudioDevicePropertyStreams:
                #expect(scope == kAudioObjectPropertyScopeInput)
                return object == 2 ? nil : [object + 100]
            default: return nil
            }
        }
        let devices = RecordingAudioRoute.inputDevices(read: read) { $0 == 7 ? nil : "uid-\($0)" }
        #expect(devices.map(\.uid) == ["uid-1", "uid-6"])
    }

    @Test func microphoneMenuShowsDefaultDevicesAndUnavailableChoice() {
        let devices = [
            AudioInputDevice(id: 1, uid: "built-in", name: "MacBook Pro Microphone"),
            AudioInputDevice(id: 6, uid: "usb-1", name: "USB Headset"),
        ]
        let menu = RecordingSetupView.microphoneMenu(
            devices: devices, defaultName: "MacBook Pro Microphone", saved: nil)
        #expect(
            menu.map(\.title) == ["System Default (MacBook Pro Microphone)", "MacBook Pro Microphone", "USB Headset"])
        #expect(menu.first?.uid == nil)
        let saved = MicrophoneDeviceChoice(uid: "usb-2", name: "Desk Mic")
        let missing = RecordingSetupView.microphoneMenu(devices: devices, defaultName: nil, saved: saved)
        #expect(missing.first?.title == "System Default")
        #expect(missing.last == .init(uid: "usb-2", title: "Desk Mic (Unavailable)"))
        let connected = RecordingSetupView.microphoneMenu(
            devices: devices, defaultName: nil, saved: MicrophoneDeviceChoice(uid: "usb-1", name: "USB Headset"))
        #expect(connected.count == 3)
    }
}
