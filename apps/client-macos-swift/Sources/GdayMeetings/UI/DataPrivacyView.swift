import SwiftUI

/// Settings → Data Privacy. Rows are derived from current provider settings, so
/// opening this tab reads no credentials and makes no network requests.
struct DataPrivacyView: View {
    @EnvironmentObject private var store: MeetingStore
    @ObservedObject private var server = GdayServerService.shared

    var body: some View {
        DataPrivacyForm(
            rows: DataPrivacy.rows(
                PrivacyContext(
                    settings: store.settings, signedInWebsiteOrigin: server.connected ? server.origin : nil,
                    pendingTranscriptions: PrivacyContext.pending(in: store.meetings)))
        ) { MeetingPanels.exportLogs(store) }
    }
}

/// Separate from the store so layout can be checked with synthetic rows.
struct DataPrivacyForm: View {
    let rows: [PrivacyRow]
    let exportLogs: () -> Void

    var body: some View {
        Form {
            Section {
                Text(
                    "Meetings are saved on this Mac. Data is sent only to the service providers listed below, at the times shown."
                )
                .foregroundStyle(.secondary)
            }
            Section("Data") {
                ForEach(rows) { DataPrivacyRowView(row: $0) }
            }
            Section("Logs") {
                Text(
                    "Each transmission is logged with its provider, address, data type, and size. Logs don’t include meeting content or credentials."
                )
                HStack(alignment: .firstTextBaseline) {
                    Button("Export Logs", action: exportLogs)
                    Text("Saves this session’s logs from the last hour to ~/Library/Logs/Gday Meetings.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }
}

private struct DataPrivacyRowView: View {
    let row: PrivacyRow

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: row.type.systemImage)
                .foregroundStyle(.secondary)
                .frame(width: 20)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(row.type.title).font(.body)
                if let contents = row.type.contents {
                    Text(contents).font(.caption).foregroundStyle(.secondary)
                }
                // Symbols distinguish local and sent data without relying on color.
                if row.destinations.isEmpty {
                    Label(PrivacyRow.localStatus, systemImage: "laptopcomputer")
                        .foregroundStyle(.secondary)
                }
                else {
                    ForEach(row.destinations) { destination in
                        Label {
                            Text(destination.text)
                        } icon: {
                            Image(systemName: "arrow.up.forward.circle").foregroundStyle(.tint)
                        }
                    }
                }
                if let note = row.note {
                    Text(note).font(.caption).foregroundStyle(.secondary)
                }
            }
            .font(.callout)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 2)
        // One VoiceOver element per data type: name, contents, status, then note.
        .accessibilityElement(children: .combine)
    }
}
