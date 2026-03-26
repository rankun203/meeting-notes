pub mod mic;
pub mod recorder;
pub mod source;
pub mod writer;

pub mod system_audio;

use source::{SourceInfo, SourceType};

/// Enumerate all available audio sources.
/// Mic is always the system default (managed via System Settings > Sound > Input).
pub fn discover_sources() -> Vec<SourceInfo> {
    let mut sources = Vec::new();

    // Single mic source — AVAudioEngine always uses the system default input device.
    // Users change their mic in System Settings > Sound > Input.
    sources.push(SourceInfo {
        id: "mic".to_string(),
        source_type: SourceType::Mic,
        label: "System Microphone".to_string(),
        default_selected: true,
    });

    // System mix — always listed, best-effort at start time
    sources.push(SourceInfo {
        id: "system_mix".to_string(),
        source_type: SourceType::SystemMix,
        label: "System Audio".to_string(),
        default_selected: true,
    });

    sources
}
