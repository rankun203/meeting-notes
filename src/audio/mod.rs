pub mod mic;
pub mod recorder;
pub mod source;
pub mod writer;

pub mod system_audio;

use cpal::traits::{DeviceTrait, HostTrait};
use source::{SourceInfo, SourceType};

/// Enumerate all available audio sources (mics + system audio).
pub fn discover_sources() -> Vec<SourceInfo> {
    let mut sources = Vec::new();

    let host = cpal::default_host();
    let default_input = host.default_input_device().and_then(|d| d.name().ok());

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(name) = device.name() {
                let is_default = default_input.as_deref() == Some(&name);
                sources.push(SourceInfo {
                    id: format!("mic:{}", name),
                    source_type: SourceType::Mic,
                    label: name,
                    default_selected: is_default,
                });
            }
        }
    }

    // System mix — always listed, best-effort at start time
    sources.push(SourceInfo {
        id: "system_mix".to_string(),
        source_type: SourceType::SystemMix,
        label: "System Audio".to_string(),
        default_selected: true,
    });

    sources
}
