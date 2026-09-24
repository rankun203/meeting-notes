//! AVAudioEngine runs only in a disposable child process. A thread timeout cannot
//! cancel a stuck Core Audio call; killing and reaping the child can.
#[cfg(target_os = "macos")]
mod isolated {
    use super::super::source::{AudioChunk, AudioError, AudioSource};
    use crossbeam_channel::Sender;
    use std::io::{self, Read, Write};
    use std::os::fd::AsRawFd;
    use std::process::{Child, Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    const DEADLINE: Duration = Duration::from_secs(5);
    const MAX_SAMPLES: usize = 262_144;

    // Also reaps on error or panic. Closing stdin is a second, independent
    // lifetime guard: the child exits even if the parent crashes or is killed.
    struct CaptureChild(Child);
    impl Drop for CaptureChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub struct MicSource {
        sample_rate: u32,
        stop: Arc<AtomicBool>,
        lost: Arc<AtomicBool>,
        reader: Option<JoinHandle<()>>,
        failures: u32,
        retry_at: Instant,
    }

    impl MicSource {
        pub fn new(sample_rate: u32) -> Self {
            Self {
                sample_rate,
                stop: Arc::new(AtomicBool::new(false)),
                lost: Arc::new(AtomicBool::new(false)),
                reader: None,
                failures: 0,
                retry_at: Instant::now(),
            }
        }

        fn start_command(
            &mut self,
            mut command: Command,
            sender: Sender<AudioChunk>,
        ) -> Result<(), AudioError> {
            if self.reader.is_some() {
                return Err(AudioError::AlreadyRecording);
            }
            self.stop.store(false, Ordering::Release);
            self.lost.store(true, Ordering::Release);
            let result = self.launch(&mut command, sender);
            if result.is_err() {
                self.failures = self.failures.saturating_add(1);
                self.retry_at = Instant::now() + retry_delay(self.failures);
            } else {
                self.failures = 0;
            }
            result
        }

        fn launch(
            &mut self,
            command: &mut Command,
            sender: Sender<AudioChunk>,
        ) -> Result<(), AudioError> {
            let mut child = CaptureChild(
                command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(device_error)?,
            );
            let mut stdout = child.0.stdout.take().expect("piped stdout");
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                if flags == -1 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
                    return Err(device_error(io::Error::last_os_error()));
                }
            }
            let stop = self.stop.clone();
            let lost = self.lost.clone();
            let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
            self.reader = Some(
                std::thread::Builder::new()
                    .name("mic-capture-ipc".into())
                    .spawn(move || {
                        let _child = child;
                        let mut first = true;
                        let result = (|| -> io::Result<()> {
                            loop {
                                // A complete audio frame must arrive within the deadline,
                                // not merely a heartbeat from a still-running helper.
                                let deadline = Instant::now() + DEADLINE;
                                let chunk = read_chunk(&mut stdout, &stop, deadline)?;
                                if first {
                                    lost.store(false, Ordering::Release);
                                    let _ = ready_tx.send(());
                                    first = false;
                                }
                                let _ = sender.try_send(chunk);
                            }
                        })();
                        lost.store(true, Ordering::Release);
                        if !stop.load(Ordering::Acquire) {
                            tracing::warn!(
                                "Microphone helper ended; capture will be retried: {:?}",
                                result.err()
                            );
                        }
                        // _child kills and reaps before this thread finishes.
                    })
                    .map_err(device_error)?,
            );
            if ready_rx
                .recv_timeout(DEADLINE + Duration::from_millis(500))
                .is_err()
            {
                self.stop()?;
                return Err(AudioError::DeviceError(
                    "microphone helper failed to deliver audio within 5 seconds".into(),
                ));
            }
            Ok(())
        }
    }

    fn retry_delay(failures: u32) -> Duration {
        Duration::from_secs((1u64 << failures.min(5)).min(30))
    }
    fn device_error(error: impl std::fmt::Display) -> AudioError {
        AudioError::DeviceError(error.to_string())
    }

    impl AudioSource for MicSource {
        fn start(&mut self, sender: Sender<AudioChunk>) -> Result<(), AudioError> {
            let mut command = Command::new(std::env::current_exe().map_err(device_error)?);
            command
                .arg("--gday-mic-helper")
                .arg(self.sample_rate.to_string());
            self.start_command(command, sender)
        }
        fn stop(&mut self) -> Result<(), AudioError> {
            self.stop.store(true, Ordering::Release);
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
            Ok(())
        }
        fn name(&self) -> &str {
            "microphone"
        }
        fn is_device_lost(&self) -> bool {
            self.lost.load(Ordering::Acquire)
        }
        fn persistent_recovery(&self) -> bool {
            true
        }
        fn recovery_ready(&self) -> bool {
            Instant::now() >= self.retry_at
        }
    }
    impl Drop for MicSource {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    // Fixed binary header: count:u32, channels:u16, rate:u32, timestamp:u64.
    // Bounds are checked before allocation, including divisibility by channels.
    fn read_chunk(
        reader: &mut impl Read,
        stop: &AtomicBool,
        deadline: Instant,
    ) -> io::Result<AudioChunk> {
        let mut header = [0u8; 18];
        read_until(reader, &mut header, stop, deadline)?;
        let count = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
        let channels = u16::from_le_bytes(header[4..6].try_into().unwrap());
        let sample_rate = u32::from_le_bytes(header[6..10].try_into().unwrap());
        let timestamp_us = u64::from_le_bytes(header[10..18].try_into().unwrap());
        if count == 0
            || count > MAX_SAMPLES
            || channels == 0
            || channels > 64
            || count % channels as usize != 0
            || !(8000..=384000).contains(&sample_rate)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid microphone frame",
            ));
        }
        let mut bytes = vec![0; count * 4];
        read_until(reader, &mut bytes, stop, deadline)?;
        let samples = bytes
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        Ok(AudioChunk {
            samples,
            channels,
            sample_rate,
            timestamp_us,
        })
    }

    fn read_until(
        reader: &mut impl Read,
        mut bytes: &mut [u8],
        stop: &AtomicBool,
        deadline: Instant,
    ) -> io::Result<()> {
        while !bytes.is_empty() {
            if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "capture stopped or stalled",
                ));
            }
            match reader.read(bytes) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => bytes = &mut bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn write_chunk(writer: &mut impl Write, chunk: &AudioChunk) -> io::Result<()> {
        writer.write_all(&(chunk.samples.len() as u32).to_le_bytes())?;
        writer.write_all(&chunk.channels.to_le_bytes())?;
        writer.write_all(&chunk.sample_rate.to_le_bytes())?;
        writer.write_all(&chunk.timestamp_us.to_le_bytes())?;
        for sample in &chunk.samples {
            writer.write_all(&sample.to_le_bytes())?;
        }
        writer.flush()
    }

    /// Private same-executable entry point. Never enters the GUI or Tokio runtime.
    pub fn run_helper(sample_rate: u32) -> ! {
        // EOF is reliable even while AVAudioEngine is stuck on another thread.
        watch_parent();
        let (tx, rx) = crossbeam_channel::bounded(32);
        let mut native = super::super::mic_native::MicSource::new(sample_rate);
        if let Err(error) = native.start(tx) {
            eprintln!("Microphone helper: {error}");
            std::process::exit(1);
        }
        let mut output = io::BufWriter::new(io::stdout().lock());
        loop {
            if native.is_device_lost() {
                std::process::exit(2);
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    if write_chunk(&mut output, &chunk).is_err() {
                        std::process::exit(0);
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(_) => std::process::exit(1),
            }
        }
    }

    fn watch_parent() {
        std::thread::spawn(|| {
            let mut byte = [0];
            loop {
                match std::io::stdin().read(&mut byte) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    _ => std::process::exit(0),
                }
            }
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parent_guard_fixture() {
            if std::env::var_os("GDAY_TEST_PARENT_GUARD").is_none() {
                return;
            }
            watch_parent();
            std::thread::sleep(Duration::from_secs(60));
            panic!("parent EOF guard did not exit");
        }

        #[test]
        fn parent_pipe_closure_exits_even_when_capture_thread_is_stuck() {
            let mut child = CaptureChild(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "audio::mic::isolated::tests::parent_guard_fixture",
                        "--nocapture",
                    ])
                    .env("GDAY_TEST_PARENT_GUARD", "1")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            std::thread::sleep(Duration::from_millis(100));
            drop(child.0.stdin.take());
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "orphan helper survived parent EOF"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn fixture(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(script);
            command
        }

        fn audio_then_stall() -> Command {
            let mut bytes = Vec::new();
            write_chunk(
                &mut bytes,
                &AudioChunk {
                    samples: vec![0.25; 8],
                    channels: 1,
                    sample_rate: 48000,
                    timestamp_us: 123,
                },
            )
            .unwrap();
            let escaped: String = bytes.iter().map(|b| format!("\\{:03o}", b)).collect();
            fixture(&format!("printf '{escaped}'; exec /bin/sleep 60"))
        }

        #[test]
        fn protocol_roundtrip_and_rejects_unbounded_allocation() {
            let original = AudioChunk {
                samples: vec![0.2, -0.5],
                channels: 2,
                sample_rate: 48000,
                timestamp_us: 42,
            };
            let mut bytes = Vec::new();
            write_chunk(&mut bytes, &original).unwrap();
            let stop = AtomicBool::new(false);
            let decoded =
                read_chunk(&mut bytes.as_slice(), &stop, Instant::now() + DEADLINE).unwrap();
            assert_eq!(decoded.samples, original.samples);
            assert_eq!(decoded.timestamp_us, 42);
            bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert_eq!(
                read_chunk(&mut bytes.as_slice(), &stop, Instant::now() + DEADLINE)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
            assert!(read_chunk(&mut &bytes[..7], &stop, Instant::now() + DEADLINE).is_err());
        }

        #[test]
        fn hung_start_is_terminated_and_a_later_attempt_can_capture() {
            let (tx, rx) = crossbeam_channel::bounded(8);
            let mut source = MicSource::new(48000);
            let begin = Instant::now();
            assert!(source
                .start_command(fixture("exec /bin/sleep 60"), tx.clone())
                .is_err());
            assert!(begin.elapsed() < Duration::from_secs(7));
            assert!(source.reader.is_none());
            assert!(source.is_device_lost());
            assert!(!source.recovery_ready());
            source.start_command(audio_then_stall(), tx).unwrap();
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(1)).unwrap().samples,
                vec![0.25; 8]
            );
            assert!(!source.is_device_lost());
            let begin = Instant::now();
            source.stop().unwrap();
            assert!(begin.elapsed() < Duration::from_secs(1));
        }

        #[test]
        fn stalled_capture_is_killed_without_waiting_for_session_ticker() {
            let (tx, _) = crossbeam_channel::bounded(8);
            let mut source = MicSource::new(48000);
            source.start_command(audio_then_stall(), tx).unwrap();
            let deadline = Instant::now() + Duration::from_secs(7);
            while !source.reader.as_ref().unwrap().is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(source.reader.as_ref().unwrap().is_finished());
            assert!(source.is_device_lost());
            source.stop().unwrap();
        }

        #[test]
        fn repeated_crashes_keep_retrying_and_backoff_is_capped() {
            let (tx, _) = crossbeam_channel::bounded(8);
            let mut source = MicSource::new(48000);
            for _ in 0..5 {
                assert!(source.start_command(fixture("exit 1"), tx.clone()).is_err());
                assert!(source.persistent_recovery());
                assert!(source.reader.is_none());
            }
            source.start_command(audio_then_stall(), tx).unwrap();
            assert_eq!(source.failures, 0);
            source.stop().unwrap();
            assert_eq!(retry_delay(1), Duration::from_secs(2));
            assert_eq!(retry_delay(u32::MAX), Duration::from_secs(30));
        }
    }
}

#[cfg(target_os = "macos")]
pub use isolated::{run_helper, MicSource};

#[cfg(not(target_os = "macos"))]
pub struct MicSource;
#[cfg(not(target_os = "macos"))]
impl MicSource {
    pub fn new(_sample_rate: u32) -> Self {
        Self
    }
}
#[cfg(not(target_os = "macos"))]
impl super::source::AudioSource for MicSource {
    fn start(
        &mut self,
        _: crossbeam_channel::Sender<super::source::AudioChunk>,
    ) -> Result<(), super::source::AudioError> {
        Err(super::source::AudioError::PlatformNotSupported)
    }
    fn stop(&mut self) -> Result<(), super::source::AudioError> {
        Ok(())
    }
    fn name(&self) -> &str {
        "microphone"
    }
}
