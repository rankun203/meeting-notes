// macOS system audio capture via Core Audio Taps (macOS 14.2+)
//
// Flow:
// 1. Create CATapDescription (ObjC) for stereo global tap
// 2. AudioHardwareCreateProcessTap -> tap_id
// 3. Query tap UID and format
// 4. Create aggregate device including the tap
// 5. Register IO proc callback on aggregate device
// 6. AudioDeviceStart -> callback sends AudioChunks via channel

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFMutableDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use crossbeam_channel::Sender;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{msg_send, msg_send_id};
use objc2_foundation::NSArray;
use tracing::{info, warn};

use super::macos_bindings::*;
use crate::audio::source::{AudioChunk, AudioError};

struct TapCallbackData {
    sender: Sender<AudioChunk>,
    channels: u16,
    sample_rate: u32,
    start_time: Instant,
    running: Arc<AtomicBool>,
}

pub struct MacosSystemAudio {
    tap_id: AudioObjectID,
    aggregate_device_id: AudioObjectID,
    io_proc_id: AudioDeviceIOProcID,
    running: Arc<AtomicBool>,
    callback_data: Option<Box<TapCallbackData>>,
}

impl MacosSystemAudio {
    pub fn new(_sample_rate: u32) -> Result<Self, AudioError> {
        Ok(Self {
            tap_id: 0,
            aggregate_device_id: 0,
            io_proc_id: ptr::null_mut(),
            running: Arc::new(AtomicBool::new(false)),
            callback_data: None,
        })
    }

    pub fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(AudioError::AlreadyRecording);
        }

        // Step 1: Create CATapDescription
        let tap_description = create_tap_description()?;

        // Step 2: Create process tap
        let mut tap_id: AudioObjectID = 0;
        let status = unsafe {
            AudioHardwareCreateProcessTap(
                Retained::as_ptr(&tap_description) as *const c_void,
                &mut tap_id,
            )
        };
        if status != 0 {
            return Err(AudioError::DeviceError(format!(
                "AudioHardwareCreateProcessTap failed (OSStatus {}). \
                Check System Settings > Privacy & Security > Screen & System Audio Recording",
                status
            )));
        }
        info!("Created process tap: {}", tap_id);
        self.tap_id = tap_id;

        // Step 3: Query tap UID
        let tap_uid = get_tap_uid(tap_id)?;
        info!("Tap UID: {}", tap_uid);

        // Step 4: Query tap format
        let format = get_tap_format(tap_id)?;
        let channels = format.mChannelsPerFrame as u16;
        let actual_sample_rate = format.mSampleRate as u32;
        info!("Tap format: {} channels, {} Hz", channels, actual_sample_rate);

        // Step 5: Create aggregate device with the tap (tap list included in description)
        let aggregate_device_id = create_aggregate_device(&tap_uid)?;
        info!("Created aggregate device: {}", aggregate_device_id);
        self.aggregate_device_id = aggregate_device_id;

        // Step 6: Register IO proc and start
        self.running.store(true, Ordering::SeqCst);

        let callback_data = Box::new(TapCallbackData {
            sender,
            channels,
            sample_rate: actual_sample_rate,
            start_time: Instant::now(),
            running: self.running.clone(),
        });
        self.callback_data = Some(callback_data);

        let client_data = self.callback_data.as_ref().unwrap().as_ref() as *const TapCallbackData as *mut c_void;

        let mut io_proc_id: AudioDeviceIOProcID = ptr::null_mut();
        let status = unsafe {
            AudioDeviceCreateIOProcID(
                aggregate_device_id,
                io_proc_callback,
                client_data,
                &mut io_proc_id,
            )
        };
        if status != 0 {
            self.cleanup();
            return Err(AudioError::DeviceError(format!(
                "AudioDeviceCreateIOProcID failed (OSStatus {})", status
            )));
        }
        self.io_proc_id = io_proc_id;

        let status = unsafe { AudioDeviceStart(aggregate_device_id, io_proc_id) };
        if status != 0 {
            self.cleanup();
            return Err(AudioError::DeviceError(format!(
                "AudioDeviceStart failed (OSStatus {})", status
            )));
        }

        info!("System audio capture started");
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        self.running.store(false, Ordering::SeqCst);
        self.cleanup();
        info!("System audio capture stopped");
        Ok(())
    }

    pub fn name(&self) -> &str {
        "system_audio"
    }

    fn cleanup(&mut self) {
        if self.aggregate_device_id != 0 && !self.io_proc_id.is_null() {
            unsafe {
                AudioDeviceStop(self.aggregate_device_id, self.io_proc_id);
                AudioDeviceDestroyIOProcID(self.aggregate_device_id, self.io_proc_id);
            }
            self.io_proc_id = ptr::null_mut();
        }

        if self.aggregate_device_id != 0 {
            destroy_aggregate_device(self.aggregate_device_id);
            self.aggregate_device_id = 0;
        }

        if self.tap_id != 0 {
            unsafe { AudioHardwareDestroyProcessTap(self.tap_id); }
            self.tap_id = 0;
        }

        self.callback_data = None;
    }
}

impl Drop for MacosSystemAudio {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.cleanup();
    }
}

unsafe extern "C" fn io_proc_callback(
    _device: AudioObjectID,
    _now: *const AudioTimeStamp,
    input_data: *const AudioBufferList,
    _input_time: *const AudioTimeStamp,
    _output_data: *mut AudioBufferList,
    _output_time: *const AudioTimeStamp,
    client_data: *mut c_void,
) -> OSStatus {
    let data = &*(client_data as *const TapCallbackData);

    if !data.running.load(Ordering::Relaxed) {
        return 0;
    }

    if input_data.is_null() {
        return 0;
    }

    let buffer_list = &*input_data;
    if buffer_list.mNumberBuffers == 0 {
        return 0;
    }

    // Read from the first buffer
    let buffer = &buffer_list.mBuffers[0];
    if buffer.mData.is_null() || buffer.mDataByteSize == 0 {
        return 0;
    }

    let num_samples = buffer.mDataByteSize as usize / std::mem::size_of::<f32>();
    let samples = std::slice::from_raw_parts(buffer.mData as *const f32, num_samples);

    let chunk = AudioChunk {
        samples: samples.to_vec(),
        channels: data.channels,
        sample_rate: data.sample_rate,
        timestamp_us: data.start_time.elapsed().as_micros() as u64,
    };

    let _ = data.sender.try_send(chunk);
    0
}

// -- Objective-C interop for CATapDescription --

fn create_tap_description() -> Result<Retained<AnyObject>, AudioError> {
    unsafe {
        let cls = AnyClass::get("CATapDescription")
            .ok_or_else(|| AudioError::DeviceError(
                "CATapDescription class not found. Requires macOS 14.2+".into()
            ))?;

        // Create empty NSArray for excluded processes
        let empty_array: Retained<NSArray<AnyObject>> = NSArray::new();

        // [[CATapDescription alloc] initStereoGlobalTapButExcludeProcesses:]
        let desc: Option<Retained<AnyObject>> = msg_send_id![
            msg_send_id![cls, alloc],
            initStereoGlobalTapButExcludeProcesses: &*empty_array,
        ];

        let desc = desc.ok_or_else(|| AudioError::DeviceError(
            "Failed to create CATapDescription".into()
        ))?;

        // Set private and mixdown
        let _: () = msg_send![&desc, setPrivate: Bool::YES];
        let _: () = msg_send![&desc, setMixdown: Bool::YES];

        Ok(desc)
    }
}

// -- Core Audio helpers --

fn get_tap_uid(tap_id: AudioObjectID) -> Result<String, AudioError> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioTapPropertyUID,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(tap_id, &address, 0, ptr::null(), &mut size)
    };
    if status != 0 {
        return Err(AudioError::DeviceError(format!("get tap UID size failed: {}", status)));
    }

    let mut cf_string_ref: core_foundation_sys::string::CFStringRef = ptr::null();
    let mut size = std::mem::size_of::<core_foundation_sys::string::CFStringRef>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            tap_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut cf_string_ref as *mut _ as *mut c_void,
        )
    };
    if status != 0 {
        return Err(AudioError::DeviceError(format!("get tap UID failed: {}", status)));
    }

    let cf_string = unsafe { CFString::wrap_under_get_rule(cf_string_ref) };
    Ok(cf_string.to_string())
}

fn get_tap_format(tap_id: AudioObjectID) -> Result<AudioStreamBasicDescription, AudioError> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioTapPropertyFormat,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut format = AudioStreamBasicDescription {
        mSampleRate: 0.0,
        mFormatID: 0,
        mFormatFlags: 0,
        mBytesPerPacket: 0,
        mFramesPerPacket: 0,
        mBytesPerFrame: 0,
        mChannelsPerFrame: 0,
        mBitsPerChannel: 0,
        mReserved: 0,
    };
    let mut size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;

    let status = unsafe {
        AudioObjectGetPropertyData(
            tap_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut format as *mut _ as *mut c_void,
        )
    };
    if status != 0 {
        return Err(AudioError::DeviceError(format!("get tap format failed: {}", status)));
    }

    Ok(format)
}

fn create_aggregate_device(tap_uid: &str) -> Result<AudioObjectID, AudioError> {
    let uid = CFString::new(&format!("org.rankun.meeting-notes.agg.{}", uuid::Uuid::new_v4()));
    let name = CFString::new("Meeting Notes System Audio");

    let mut desc_dict = CFMutableDictionary::new();
    desc_dict.set(CFString::new(AGGREGATE_DEVICE_UID_KEY).as_CFType(), uid.as_CFType());
    desc_dict.set(CFString::new(AGGREGATE_DEVICE_NAME_KEY).as_CFType(), name.as_CFType());
    desc_dict.set(CFString::new(AGGREGATE_DEVICE_IS_PRIVATE_KEY).as_CFType(), CFBoolean::true_value().as_CFType());
    desc_dict.set(CFString::new(AGGREGATE_DEVICE_IS_STACKED_KEY).as_CFType(), CFNumber::from(0i32).as_CFType());

    // Tap list
    let mut tap_entry = CFMutableDictionary::new();
    tap_entry.set(CFString::new(AGGREGATE_DEVICE_TAP_UID_KEY).as_CFType(), CFString::new(tap_uid).as_CFType());
    tap_entry.set(CFString::new(AGGREGATE_DEVICE_TAP_DRIFT_KEY).as_CFType(), CFNumber::from(1i32).as_CFType());

    let tap_list = core_foundation::array::CFArray::from_CFTypes(&[tap_entry.to_untyped()]);
    desc_dict.set(CFString::new(AGGREGATE_DEVICE_TAP_LIST_KEY).as_CFType(), tap_list.as_CFType());

    let mut aggregate_id: AudioObjectID = 0;
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(
            desc_dict.as_concrete_TypeRef(),
            &mut aggregate_id,
        )
    };
    if status != 0 {
        return Err(AudioError::DeviceError(format!(
            "AudioHardwareCreateAggregateDevice failed (OSStatus {})", status
        )));
    }

    Ok(aggregate_id)
}

fn destroy_aggregate_device(device_id: AudioObjectID) {
    let status = unsafe { AudioHardwareDestroyAggregateDevice(device_id) };
    if status != 0 {
        warn!("AudioHardwareDestroyAggregateDevice failed: {}", status);
    }
}
