import SwiftUI

/// HIG Sheets: keep a modal sheet focused on one scoped task with explicit actions.
/// Merely displaying this form never opens either protected audio resource.
/// https://developer.apple.com/design/human-interface-guidelines/sheets
struct RecordingSetupView: View {
    @EnvironmentObject private var store: MeetingStore
    @Environment(\.dismiss) private var dismiss
    var onStarted: (UUID) -> Void = { _ in }
    @ViewState private var title = ""
    @ViewState private var microphone = true
    @ViewState private var systemAudio = true
    @ViewState private var voiceProcessing = false
    @ViewState private var voiceProcessingOverride: Bool?
    @ViewState private var format = RecordingFormat.opus
    @ViewState private var showOptions = false
    @ViewState private var startupError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header.padding(.horizontal, 26).padding(.top, 26).padding(.bottom, 20)
            // HIG Scroll views: make content beyond the available area reachable.
            // Keep the sheet's title and completion actions outside the scrolling
            // region so expanding options or showing an error can't hide them.
            // https://developer.apple.com/design/human-interface-guidelines/scroll-views
            ScrollViewReader { scroll in
                ScrollView {
                    form.padding(.horizontal, 26).padding(.bottom, 22)
                }
                .scrollIndicators(.visible)
                .onChange(of: startupError) { _, error in
                    if error != nil {
                        // Reveal a new failure even when it appears below the fold.
                        scroll.scrollTo("recording-startup-error", anchor: .bottom)
                    }
                }
            }
            .frame(maxHeight: .infinity)
            Divider()
            actions.padding(.horizontal, 26).padding(.vertical, 16)
        }
        // A bounded ideal height keeps the sheet stable when options expand.
        // The scroll region yields space when the presenting window is shorter.
        .frame(width: 510)
        .frame(minHeight: 400, idealHeight: 540, maxHeight: 620)
        .interactiveDismissDisabled(store.isStartingRecording)
        .onAppear {
            microphone = store.settings.captureMicrophone
            systemAudio = store.settings.captureSystemAudio
            voiceProcessing = RecordingAudioRoute.defaultVoiceProcessing()
            voiceProcessingOverride = nil
            format = store.settings.recordingFormat
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: "waveform.circle.fill")
                .font(.system(size: 42)).foregroundStyle(.tint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 5) {
                Text("New Recording").font(.title2.weight(.semibold))
                Text("Choose the audio you want to include.")
                    .foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 22) {
            VStack(alignment: .leading, spacing: 7) {
                Text("Meeting title").font(.subheadline.weight(.medium))
                TextField("Untitled Meeting", text: $title)
                    .textFieldStyle(.roundedBorder).accessibilityLabel("Meeting title").disabled(
                        store.isStartingRecording)
            }
            VStack(spacing: 0) {
                sourceToggle(
                    "Microphone", subtitle: "Your voice and the room around you", symbol: "mic.fill", value: $microphone
                )
                Divider().padding(.leading, 44)
                sourceToggle(
                    "System Audio", subtitle: "Meeting participants and other app audio", symbol: "speaker.wave.2.fill",
                    value: $systemAudio)
            }
            .padding(.horizontal, 14).background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 12))
            if !microphone && !systemAudio {
                Label("Choose at least one audio source.", systemImage: "info.circle")
                    .font(.callout).foregroundStyle(.secondary)
            }
            DisclosureGroup("Recording Options", isExpanded: $showOptions) {
                VStack(alignment: .leading, spacing: 12) {
                    Toggle(
                        "Microphone Voice Processing",
                        isOn: Binding(
                            get: { voiceProcessing },
                            set: {
                                voiceProcessing = $0
                                voiceProcessingOverride = $0
                            }
                        )
                    ).disabled(!microphone)
                    Text(
                        "Enabled by default when speakers are detected. Can reduce echo and background noise, and may lower other apps’ volume. Applies to this recording only."
                    )
                    .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    Picker("Audio Format", selection: $format) {
                        Text("Opus (Recommended)").tag(RecordingFormat.opus)
                        Text("M4A (AAC)").tag(RecordingFormat.m4a)
                        Text("WAV").tag(RecordingFormat.wav)
                    }
                }
                .padding(.top, 12)
                // Keep the native pop-up bezel and keyboard focus ring inside
                // the disclosure's clipping boundary (HIG: retain system controls).
                // https://developer.apple.com/design/human-interface-guidelines/pop-up-buttons
                .padding(.bottom, 4)
            }
            .disclosureGroupStyle(RecordingDisclosureStyle())
            .disabled(store.isStartingRecording)
            if let startupError {
                // HIG Feedback: keep recoverable failure beside the action it affects,
                // rather than attempting to present an alert behind this modal sheet.
                Label {
                    Text(startupError).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                } icon: {
                    Image(systemName: "exclamationmark.circle")
                }
                .font(.callout).foregroundStyle(.secondary)
                .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
                .id("recording-startup-error")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var actions: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Audio sources are saved as separate tracks.")
                .font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                if store.isStartingRecording {
                    ProgressView().controlSize(.small)
                    Text("Preparing…").font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                    .disabled(store.isStartingRecording)
                Button(startupError == nil ? "Start Recording" : "Try Again") {
                    startupError = nil
                    let previousError = store.errorMessage
                    Task {
                        await store.startRecording(
                            title: title, microphoneEnabled: microphone, systemEnabled: systemAudio,
                            format: format, voiceProcessingEnabled: voiceProcessingOverride)
                        if let id = store.recordingID {
                            onStarted(id)
                            dismiss()
                        }
                        else if let permission = store.recordingPermissionNeeded {
                            startupError = permission.explanation
                            store.recordingPermissionNeeded = nil
                        }
                        else if let error = store.errorMessage, error != previousError {
                            startupError = error
                            store.errorMessage = nil
                        }
                    }
                }
                .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
                .disabled((!microphone && !systemAudio) || store.isBusy || store.isStartingRecording)
            }
        }
    }
    private func sourceToggle(_ name: String, subtitle: String, symbol: String, value: Binding<Bool>) -> some View {
        HStack(spacing: 13) {
            Image(systemName: symbol).font(.title3).foregroundStyle(value.wrappedValue ? Color.accentColor : .secondary)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(name).fontWeight(.medium)
                Text(subtitle).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Toggle(name, isOn: value).labelsHidden().toggleStyle(.switch)
        }.padding(.vertical, 14).disabled(store.isStartingRecording)
    }
}

/// HIG Feedback: actual source activity remains visible and understandable without
/// interrupting the task. Color is supplemented by text and accessible values.
/// https://developer.apple.com/design/human-interface-guidelines/feedback
struct RecordingWorkspaceView: View {
    @EnvironmentObject private var store: MeetingStore
    let meetingID: UUID
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 7) {
                        Circle().fill(store.isFinalizingRecording ? Color.secondary : .red).frame(width: 7, height: 7)
                        Text(store.isFinalizingRecording ? "Saving Recording" : "Recording").font(
                            .subheadline.weight(.semibold))
                    }
                    TimelineView(.periodic(from: .now, by: 1)) { timeline in
                        let elapsed =
                            store.isFinalizingRecording
                            ? (store.meetings.first { $0.id == meetingID }?.duration ?? store.recordingDuration)
                            : (store.recordingStartedAt.map { timeline.date.timeIntervalSince($0) } ?? 0)
                        Text(Self.elapsed(elapsed)).font(.system(size: 34, weight: .medium, design: .rounded))
                            .monospacedDigit()
                            .accessibilityLabel("Recording duration").accessibilityValue(Self.elapsed(elapsed))
                    }
                }
                Spacer()
                if store.isFinalizingRecording {
                    // HIG Progress: saving duration is unknown; never invent a percentage.
                    // https://developer.apple.com/design/human-interface-guidelines/progress-indicators
                    ProgressView().controlSize(.small)
                    Text("Saving audio…").foregroundStyle(.secondary)
                }
                else {
                    Button {
                        Task { await store.stopRecording() }
                    } label: {
                        Label("Stop & Save", systemImage: "stop.fill")
                    }.buttonStyle(.borderedProminent).tint(.red).controlSize(.large)
                }
            }
            HStack(spacing: 26) {
                RecordingSourceMeter(
                    title: "Microphone", symbol: "mic.fill", source: store.recordingLevels.microphone,
                    saving: store.isFinalizingRecording)
                RecordingSourceMeter(
                    title: "System Audio", symbol: "speaker.wave.2.fill", source: store.recordingLevels.system,
                    saving: store.isFinalizingRecording)
            }
        }
        .padding(18)
        .background(.background, in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.quaternary))
    }
    static func elapsed(_ seconds: TimeInterval) -> String {
        let value = max(0, Int(seconds.isFinite ? seconds : 0))
        return value >= 3600
            ? String(format: "%d:%02d:%02d", value / 3600, value / 60 % 60, value % 60)
            : String(format: "%02d:%02d", value / 60, value % 60)
    }
}

/// HIG Disclosure controls: provide a clear, accessible way to reveal related details.
/// One native button owns the entire header so its label and empty space activate
/// the same action as its chevron, with standard keyboard and disabled behavior.
/// https://developer.apple.com/design/human-interface-guidelines/disclosure-controls
struct RecordingDisclosureStyle: DisclosureGroupStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.15)) {
                    configuration.isExpanded.toggle()
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: configuration.isExpanded ? "chevron.down" : "chevron.forward")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 12)
                        .accessibilityHidden(true)
                    configuration.label
                    Spacer(minLength: 0)
                }
                .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(ActionButtonStyle())
            .accessibilityValue(configuration.isExpanded ? "Expanded" : "Collapsed")

            if configuration.isExpanded {
                configuration.content
            }
        }
    }
}

private struct RecordingSourceMeter: View {
    let title: String
    let symbol: String
    let source: RecordingSourceLevel
    let saving: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            // HIG Feedback / Accessibility: use a stable symbol slot for changing
            // status, with a text explanation on hover and in the accessible value.
            // Status changes must not reflow one meter independently of the other.
            // https://developer.apple.com/design/human-interface-guidelines/accessibility
            HStack(spacing: 6) {
                sourceLabel.lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: statusSymbol)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: 18, height: 18)
                    .help(statusText)
            }
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary)
                    Capsule().fill(source.peakDB > -1 ? Color.orange : Color.accentColor)
                        .frame(width: geometry.size.width * (saving ? 0 : source.level))
                }
            }.frame(height: 7)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .help(statusText)
        .accessibilityValue(
            !saving && source.enabled && source.hasSamples && !source.stale
                ? "\(statusText), \(Int(source.rmsDB)) decibels" : statusText)
    }

    private var sourceLabel: some View { Label(title, systemImage: symbol).font(.subheadline.weight(.medium)) }
    private var statusText: String {
        saving ? (source.enabled ? "Finalizing" : "Not recorded") : source.statusText
    }

    private var statusSymbol: String {
        if !source.enabled { return "minus.circle" }
        if saving { return "hourglass" }
        if !source.hasSamples { return "clock" }
        if source.stale { return "exclamationmark.triangle" }
        return source.rmsDB < -60 ? "waveform" : "waveform.circle.fill"
    }
}
