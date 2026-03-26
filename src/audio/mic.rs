#[cfg(target_os = "macos")]
#[allow(non_upper_case_globals, non_snake_case)]
mod platform {
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use block2::RcBlock;
    use crossbeam_channel::Sender;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, msg_send_id};
    use tracing::{info, warn};

    use crate::audio::source::{AudioChunk, AudioError, AudioSource};

    // Link AVFAudio framework so AVAudioEngine class is available at runtime.
    #[link(name = "AVFAudio", kind = "framework")]
    extern "C" {}

    /// Mic capture using AVAudioEngine (macOS).
    ///
    /// Always uses the system default input device. Users select their mic in
    /// System Settings > Sound > Input. When Teams (or another app) modifies
    /// the Core Audio device graph, the engine auto-restarts via
    /// AVAudioEngineConfigurationChangeNotification.
    pub struct MicSource {
        sample_rate: u32,
        engine: Option<Retained<AnyObject>>,
        running: Arc<AtomicBool>,
        sender_handle: Arc<Mutex<Option<Sender<AudioChunk>>>>,
        /// Stored to prevent deallocation while engine is alive.
        _observer: Option<Retained<AnyObject>>,
    }

    // AVAudioEngine is thread-safe (it manages its own internal threading).
    // The observer and engine are only modified in start/stop which are
    // serialized by SessionManager's RwLock.
    unsafe impl Send for MicSource {}
    unsafe impl Sync for MicSource {}

    impl MicSource {
        pub fn new(sample_rate: u32) -> Self {
            Self {
                sample_rate,
                engine: None,
                running: Arc::new(AtomicBool::new(false)),
                sender_handle: Arc::new(Mutex::new(None)),
                _observer: None,
            }
        }
    }

    impl AudioSource for MicSource {
        fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
            if self.running.load(Ordering::SeqCst) {
                return Err(AudioError::AlreadyRecording);
            }

            // Create AVAudioEngine
            let engine: Retained<AnyObject> = unsafe {
                let cls = AnyClass::get("AVAudioEngine")
                    .ok_or_else(|| AudioError::DeviceError("AVAudioEngine class not found".into()))?;
                let alloc: Allocated<AnyObject> = msg_send_id![cls, alloc];
                let engine: Option<Retained<AnyObject>> = msg_send_id![alloc, init];
                engine
            }.ok_or_else(|| AudioError::DeviceError("failed to create AVAudioEngine".into()))?;

            // Get inputNode (always tracks system default input device)
            let input_node: Retained<AnyObject> = unsafe {
                let node: Option<Retained<AnyObject>> = msg_send_id![&engine, inputNode];
                node
            }.ok_or_else(|| AudioError::DeviceError("failed to get inputNode".into()))?;

            // Get the hardware output format for bus 0
            let format: Retained<AnyObject> = unsafe {
                let fmt: Option<Retained<AnyObject>> = msg_send_id![&input_node, outputFormatForBus: 0u64];
                fmt
            }.ok_or_else(|| AudioError::DeviceError("failed to get input format".into()))?;

            let hw_sample_rate: f64 = unsafe { msg_send![&format, sampleRate] };
            let hw_channels: u32 = unsafe { msg_send![&format, channelCount] };

            info!(
                "AVAudioEngine input: {} channels, {} Hz (requested {} Hz)",
                hw_channels, hw_sample_rate as u32, self.sample_rate
            );

            // Store sender
            *self.sender_handle.lock().unwrap() = Some(sender);
            self.running.store(true, Ordering::SeqCst);
            let start_time = Instant::now();

            // Create the tap block
            let sender_handle = self.sender_handle.clone();
            let running = self.running.clone();
            let channels = hw_channels as u16;
            let actual_sample_rate = hw_sample_rate as u32;

            // AVAudioNodeTapBlock: ^(AVAudioPCMBuffer *buffer, AVAudioTime *when)
            let tap_block = RcBlock::new(move |buffer: NonNull<AnyObject>, _when: NonNull<AnyObject>| {
                if !running.load(Ordering::Relaxed) {
                    return;
                }

                let buf = unsafe { buffer.as_ref() };
                let frame_count: u32 = unsafe { msg_send![buf, frameLength] };
                if frame_count == 0 {
                    return;
                }

                // floatChannelData returns float * const * (non-interleaved)
                let float_channel_data: *const *const f32 = unsafe { msg_send![buf, floatChannelData] };
                if float_channel_data.is_null() {
                    return;
                }

                // Interleave channels into a single sample buffer
                let ch_count = channels as usize;
                let frame_count = frame_count as usize;
                let mut samples = Vec::with_capacity(frame_count * ch_count);

                unsafe {
                    if ch_count == 1 {
                        // Mono: just copy
                        let ch0 = *float_channel_data;
                        samples.extend_from_slice(std::slice::from_raw_parts(ch0, frame_count));
                    } else {
                        // Interleave: [L0, R0, L1, R1, ...]
                        let ptrs: Vec<*const f32> = (0..ch_count)
                            .map(|c| *float_channel_data.add(c))
                            .collect();
                        for i in 0..frame_count {
                            for ch_ptr in &ptrs {
                                samples.push(*ch_ptr.add(i));
                            }
                        }
                    }
                }

                let guard = sender_handle.lock().unwrap();
                if let Some(ref sender) = *guard {
                    let chunk = AudioChunk {
                        samples,
                        channels,
                        sample_rate: actual_sample_rate,
                        timestamp_us: start_time.elapsed().as_micros() as u64,
                    };
                    let _ = sender.try_send(chunk);
                }
            });

            // Install tap on bus 0
            let buffer_size: u32 = 4096;
            unsafe {
                let _: () = msg_send![
                    &input_node,
                    installTapOnBus: 0u64
                    bufferSize: buffer_size
                    format: core::ptr::null::<AnyObject>()  // nil = use node's output format
                    block: &*tap_block
                ];
            }

            // Prepare and start the engine
            unsafe {
                let _: () = msg_send![&engine, prepare];
            }

            let started: bool = unsafe {
                let mut err: *mut AnyObject = std::ptr::null_mut();
                let ok: Bool = msg_send![&engine, startAndReturnError: &mut err];
                if !ok.as_bool() && !err.is_null() {
                    let desc: Option<Retained<AnyObject>> = msg_send_id![err, localizedDescription];
                    let msg = desc
                        .map(|d| {
                            let s: *const std::ffi::c_char = msg_send![&d, UTF8String];
                            if s.is_null() {
                                "unknown error".to_string()
                            } else {
                                std::ffi::CStr::from_ptr(s).to_string_lossy().to_string()
                            }
                        })
                        .unwrap_or_else(|| "unknown error".to_string());
                    self.running.store(false, Ordering::SeqCst);
                    self.sender_handle.lock().unwrap().take();
                    let _: () = msg_send![&input_node, removeTapOnBus: 0u64];
                    return Err(AudioError::DeviceError(format!("AVAudioEngine start failed: {}", msg)));
                }
                ok.as_bool()
            };

            if !started {
                self.running.store(false, Ordering::SeqCst);
                self.sender_handle.lock().unwrap().take();
                unsafe { let _: () = msg_send![&input_node, removeTapOnBus: 0u64]; }
                return Err(AudioError::DeviceError("AVAudioEngine failed to start".into()));
            }

            // Register for AVAudioEngineConfigurationChangeNotification to auto-restart
            // when Teams or other apps modify the audio device graph.
            let engine_for_observer = engine.clone();
            let running_for_observer = self.running.clone();
            let observer_block = RcBlock::new(move |_notification: NonNull<AnyObject>| {
                if !running_for_observer.load(Ordering::Relaxed) {
                    return;
                }
                warn!("AVAudioEngine configuration changed (audio device graph modified)");
                warn!("Auto-restarting AVAudioEngine...");

                // The engine has stopped itself. Restart it — tap persists.
                unsafe {
                    let _: () = msg_send![&engine_for_observer, prepare];
                    let mut err: *mut AnyObject = std::ptr::null_mut();
                    let ok: Bool = msg_send![&engine_for_observer, startAndReturnError: &mut err];
                    if ok.as_bool() {
                        info!("AVAudioEngine restarted successfully after config change");
                    } else {
                        warn!("AVAudioEngine restart failed after config change");
                    }
                }
            });

            let observer: Option<Retained<AnyObject>> = unsafe {
                let center: Option<Retained<AnyObject>> = msg_send_id![
                    AnyClass::get("NSNotificationCenter").unwrap(),
                    defaultCenter
                ];
                let center = center.unwrap();

                let notif_name: Option<Retained<AnyObject>> = {
                    let cls = AnyClass::get("NSString").unwrap();
                    msg_send_id![cls, stringWithUTF8String: c"AVAudioEngineConfigurationChangeNotification".as_ptr()]
                };
                let notif_name = notif_name.unwrap();

                msg_send_id![
                    &center,
                    addObserverForName: &*notif_name
                    object: &*engine
                    queue: core::ptr::null::<AnyObject>()  // nil = calling queue
                    usingBlock: &*observer_block
                ]
            };

            self._observer = observer;
            self.engine = Some(engine);
            info!("Mic recording started (AVAudioEngine)");
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            self.running.store(false, Ordering::SeqCst);

            // Remove notification observer
            if let Some(ref observer) = self._observer {
                unsafe {
                    let center: Option<Retained<AnyObject>> = msg_send_id![
                        AnyClass::get("NSNotificationCenter").unwrap(),
                        defaultCenter
                    ];
                    if let Some(ref center) = center {
                        let _: () = msg_send![center, removeObserver: &**observer];
                    }
                }
            }
            self._observer = None;

            // Stop engine and remove tap
            if let Some(ref engine) = self.engine {
                unsafe {
                    let input_node: Option<Retained<AnyObject>> = msg_send_id![engine, inputNode];
                    if let Some(ref node) = input_node {
                        let _: () = msg_send![node, removeTapOnBus: 0u64];
                    }
                    let _: () = msg_send![engine, stop];
                }
            }
            self.engine = None;

            // Drop sender to disconnect writer channel
            self.sender_handle.lock().unwrap().take();

            info!("Mic recording stopped (AVAudioEngine)");
            Ok(())
        }

        fn name(&self) -> &str {
            "microphone"
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use crossbeam_channel::Sender;
    use crate::audio::source::{AudioChunk, AudioError, AudioSource};

    /// Fallback mic source for non-macOS platforms using cpal.
    pub struct MicSource {
        _sample_rate: u32,
    }

    impl MicSource {
        pub fn new(sample_rate: u32) -> Self {
            Self { _sample_rate: sample_rate }
        }
    }

    impl AudioSource for MicSource {
        fn start(&mut self, _sender: Sender<AudioChunk>) -> Result<(), AudioError> {
            Err(AudioError::PlatformNotSupported)
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            Ok(())
        }

        fn name(&self) -> &str {
            "microphone"
        }
    }
}

pub use platform::MicSource;
