// Raw FFI bindings for macOS Core Audio Tap APIs (macOS 14.2+)
// These APIs are not available in coreaudio-sys, so we declare them manually.
#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use std::ffi::c_void;

pub type AudioObjectID = u32;
pub type OSStatus = i32;
pub type AudioDeviceIOProcID = *mut c_void;
pub type Float64 = f64;

pub const kAudioObjectSystemObject: AudioObjectID = 1;

// Property scopes
pub const kAudioObjectPropertyScopeGlobal: u32 = 0x676C6F62; // 'glob'
pub const kAudioObjectPropertyScopeInput: u32 = 0x696E7074; // 'inpt'
pub const kAudioObjectPropertyScopeOutput: u32 = 0x6F757470; // 'outp'

// Property elements
pub const kAudioObjectPropertyElementMain: u32 = 0;

// Property selectors
pub const kAudioHardwarePropertyDevices: u32 = 0x64657623; // 'dev#'
pub const kAudioDevicePropertyStreamConfiguration: u32 = 0x73636667; // 'scfg'
pub const kAudioDevicePropertyNominalSampleRate: u32 = 0x6E737274; // 'nsrt'

// Aggregate device dictionary keys (kAudioAggregateDevice* from AudioHardware.h)
pub const AGGREGATE_DEVICE_UID_KEY: &str = "uid";
pub const AGGREGATE_DEVICE_NAME_KEY: &str = "name";
pub const AGGREGATE_DEVICE_IS_PRIVATE_KEY: &str = "private";
pub const AGGREGATE_DEVICE_IS_STACKED_KEY: &str = "stacked";
pub const AGGREGATE_DEVICE_TAP_LIST_KEY: &str = "taps";
pub const AGGREGATE_DEVICE_TAP_UID_KEY: &str = "uid";    // kAudioSubTapUIDKey
pub const AGGREGATE_DEVICE_TAP_DRIFT_KEY: &str = "drift"; // kAudioSubTapDriftCompensationKey

// Tap property selectors
pub const kAudioTapPropertyFormat: u32 = 0x74666D74; // 'tfmt'
pub const kAudioTapPropertyUID: u32 = 0x74756964; // 'tuid'

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioObjectPropertyAddress {
    pub mSelector: u32,
    pub mScope: u32,
    pub mElement: u32,
}

#[repr(C)]
pub struct AudioBufferList {
    pub mNumberBuffers: u32,
    pub mBuffers: [AudioBuffer; 1], // variable-length array
}

#[repr(C)]
pub struct AudioBuffer {
    pub mNumberChannels: u32,
    pub mDataByteSize: u32,
    pub mData: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioStreamBasicDescription {
    pub mSampleRate: Float64,
    pub mFormatID: u32,
    pub mFormatFlags: u32,
    pub mBytesPerPacket: u32,
    pub mFramesPerPacket: u32,
    pub mBytesPerFrame: u32,
    pub mChannelsPerFrame: u32,
    pub mBitsPerChannel: u32,
    pub mReserved: u32,
}

#[repr(C)]
pub struct AudioTimeStamp {
    pub mSampleTime: Float64,
    pub mHostTime: u64,
    pub mRateScalar: Float64,
    pub mWordClockTime: u64,
    pub mSMPTETime: [u8; 24], // SMPTETime struct, we don't need its fields
    pub mFlags: u32,
    pub mReserved: u32,
}

pub type AudioDeviceIOProc = unsafe extern "C" fn(
    device: AudioObjectID,
    now: *const AudioTimeStamp,
    input_data: *const AudioBufferList,
    input_time: *const AudioTimeStamp,
    output_data: *mut AudioBufferList,
    output_time: *const AudioTimeStamp,
    client_data: *mut c_void,
) -> OSStatus;

extern "C" {
    pub fn AudioObjectGetPropertyData(
        object_id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;

    pub fn AudioObjectGetPropertyDataSize(
        object_id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
    ) -> OSStatus;

    pub fn AudioObjectSetPropertyData(
        object_id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> OSStatus;

    pub fn AudioHardwareCreateProcessTap(
        tap_description: *const c_void, // CATapDescription* (ObjC)
        tap_id: *mut AudioObjectID,
    ) -> OSStatus;

    pub fn AudioHardwareDestroyProcessTap(
        tap_id: AudioObjectID,
    ) -> OSStatus;

    pub fn AudioHardwareCreateAggregateDevice(
        description: core_foundation_sys::dictionary::CFDictionaryRef,
        device_id: *mut AudioObjectID,
    ) -> OSStatus;

    pub fn AudioHardwareDestroyAggregateDevice(
        device_id: AudioObjectID,
    ) -> OSStatus;

    pub fn AudioDeviceCreateIOProcID(
        device: AudioObjectID,
        proc_: AudioDeviceIOProc,
        client_data: *mut c_void,
        io_proc_id: *mut AudioDeviceIOProcID,
    ) -> OSStatus;

    pub fn AudioDeviceDestroyIOProcID(
        device: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
    ) -> OSStatus;

    pub fn AudioDeviceStart(
        device: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
    ) -> OSStatus;

    pub fn AudioDeviceStop(
        device: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
    ) -> OSStatus;
}
