//! macOS events used by recording auto-stop.
//!
//! Screen lock is detected from the current Core Graphics session state. A
//! short polling interval is used because this daemon has no AppKit main event
//! loop, making distributed lock notifications unreliable. System sleep uses
//! the I/O Kit root power domain so acknowledgement can be delayed briefly
//! while audio writers finalize.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};
use core_foundation_sys::runloop::{
    kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRun,
    CFRunLoopSourceRef,
};
use tokio::sync::mpsc;

type IoObject = u32;
type IoConnect = u32;
type IoReturn = i32;

#[repr(C)]
struct IoNotificationPort(c_void);

type IoNotificationPortRef = *mut IoNotificationPort;
type PowerCallback = extern "C" fn(*mut c_void, IoObject, u32, *mut c_void);

// iokit_common_msg(0x270) and iokit_common_msg(0x280), from IOMessage.h.
const IO_MESSAGE_CAN_SYSTEM_SLEEP: u32 = 0xe000_0270;
const IO_MESSAGE_SYSTEM_WILL_SLEEP: u32 = 0xe000_0280;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IORegisterForSystemPower(
        refcon: *mut c_void,
        notification_port: *mut IoNotificationPortRef,
        callback: PowerCallback,
        notifier: *mut IoObject,
    ) -> IoConnect;
    fn IODeregisterForSystemPower(notifier: *mut IoObject) -> IoReturn;
    fn IOAllowPowerChange(kernel_port: IoConnect, notification_id: isize) -> IoReturn;
    fn IOServiceClose(connect: IoConnect) -> IoReturn;
    fn IONotificationPortGetRunLoopSource(
        notification_port: IoNotificationPortRef,
    ) -> CFRunLoopSourceRef;
    fn IONotificationPortDestroy(notification_port: IoNotificationPortRef);
}

pub enum SystemEvent {
    ScreenLocked,
    SystemWillSleep(SleepRequest),
}

#[derive(Debug, Clone, Copy)]
pub struct MonitorSupport {
    pub screen_lock: bool,
    pub system_sleep: bool,
}

struct PowerCallbackContext {
    sender: mpsc::UnboundedSender<SystemEvent>,
    kernel_port: AtomicU32,
}

/// A pending, non-abortable system sleep. Dropping the value acknowledges the
/// event, ensuring macOS is never left waiting if the async handler exits.
pub struct SleepRequest {
    kernel_port: IoConnect,
    notification_id: isize,
    acknowledged: bool,
}

impl SleepRequest {
    pub fn allow(mut self) {
        self.acknowledge();
    }

    fn acknowledge(&mut self) {
        if self.acknowledged {
            return;
        }
        unsafe {
            IOAllowPowerChange(self.kernel_port, self.notification_id);
        }
        self.acknowledged = true;
    }
}

impl Drop for SleepRequest {
    fn drop(&mut self) {
        self.acknowledge();
    }
}

fn screen_is_locked() -> Option<bool> {
    let dictionary = unsafe { CGSessionCopyCurrentDictionary() };
    if dictionary.is_null() {
        return None;
    }

    let key = CFString::new("CGSSessionScreenIsLocked");
    let value =
        unsafe { CFDictionaryGetValue(dictionary, key.as_concrete_TypeRef().cast::<c_void>()) };
    let locked = if value.is_null() {
        // The lock key is absent in an unlocked login session.
        false
    } else if unsafe { CFGetTypeID(value.cast()) } == unsafe { CFBooleanGetTypeID() } {
        unsafe { CFBooleanGetValue(value as CFBooleanRef) }
    } else {
        false
    };

    unsafe { CFRelease(dictionary as CFTypeRef) };
    Some(locked)
}

fn start_screen_lock_monitor(sender: mpsc::UnboundedSender<SystemEvent>) -> Result<bool, String> {
    // The session dictionary can be temporarily unavailable during login or
    // while launched by a supervisor. Keep polling so detection begins as soon
    // as the GUI session becomes queryable.
    let mut was_locked = screen_is_locked().unwrap_or(false);

    std::thread::Builder::new()
        .name("macos-screen-lock".to_string())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            if sender.is_closed() {
                break;
            }
            if let Some(locked) = screen_is_locked() {
                if locked && !was_locked && sender.send(SystemEvent::ScreenLocked).is_err() {
                    break;
                }
                was_locked = locked;
            }
        })
        .map_err(|e| format!("failed to spawn macOS screen-lock monitor: {e}"))?;
    Ok(true)
}

extern "C" fn power_callback(
    refcon: *mut c_void,
    _service: IoObject,
    message_type: u32,
    message_argument: *mut c_void,
) {
    if refcon.is_null() {
        return;
    }
    let context = unsafe { &*(refcon.cast::<PowerCallbackContext>()) };
    let notification_id = message_argument as isize;
    let kernel_port = context.kernel_port.load(Ordering::Acquire);
    if kernel_port == 0 {
        return;
    }

    match message_type {
        // An idle-sleep query may still be cancelled. Acknowledge it now and
        // wait for the later, non-abortable WILL_SLEEP event before stopping.
        IO_MESSAGE_CAN_SYSTEM_SLEEP => unsafe {
            IOAllowPowerChange(kernel_port, notification_id);
        },
        IO_MESSAGE_SYSTEM_WILL_SLEEP => {
            // A failed send drops the request and therefore acknowledges it.
            let _ = context
                .sender
                .send(SystemEvent::SystemWillSleep(SleepRequest {
                    kernel_port,
                    notification_id,
                    acknowledged: false,
                }));
        }
        _ => {}
    }
}

fn start_sleep_monitor(sender: mpsc::UnboundedSender<SystemEvent>) -> Result<bool, String> {
    let (startup_tx, startup_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("macos-system-sleep".to_string())
        .spawn(move || {
            let context = Box::new(PowerCallbackContext {
                sender,
                kernel_port: AtomicU32::new(0),
            });
            let context_ptr = Box::into_raw(context);
            let mut notification_port: IoNotificationPortRef = ptr::null_mut();
            let mut notifier: IoObject = 0;
            let kernel_port = unsafe {
                IORegisterForSystemPower(
                    context_ptr.cast(),
                    &mut notification_port,
                    power_callback,
                    &mut notifier,
                )
            };

            if kernel_port == 0 || notification_port.is_null() {
                unsafe {
                    if kernel_port != 0 {
                        IOServiceClose(kernel_port);
                    }
                    if !notification_port.is_null() {
                        IONotificationPortDestroy(notification_port);
                    }
                    drop(Box::from_raw(context_ptr));
                }
                let _ = startup_tx.send(false);
                return;
            }

            let source = unsafe { IONotificationPortGetRunLoopSource(notification_port) };
            if source.is_null() {
                unsafe {
                    if notifier != 0 {
                        IODeregisterForSystemPower(&mut notifier);
                    }
                    IOServiceClose(kernel_port);
                    IONotificationPortDestroy(notification_port);
                    drop(Box::from_raw(context_ptr));
                }
                let _ = startup_tx.send(false);
                return;
            }

            unsafe {
                (*context_ptr)
                    .kernel_port
                    .store(kernel_port, Ordering::Release);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            }
            let _ = startup_tx.send(true);
            unsafe { CFRunLoopRun() };

            unsafe {
                (*context_ptr).kernel_port.store(0, Ordering::Release);
                IODeregisterForSystemPower(&mut notifier);
                IOServiceClose(kernel_port);
                IONotificationPortDestroy(notification_port);
                drop(Box::from_raw(context_ptr));
            }
        })
        .map_err(|e| format!("failed to spawn macOS system-sleep monitor: {e}"))?;

    startup_rx
        .recv()
        .map_err(|_| "macOS system-sleep monitor exited during startup".to_string())
}

pub fn start() -> Result<(mpsc::UnboundedReceiver<SystemEvent>, MonitorSupport), String> {
    let (sender, receiver) = mpsc::unbounded_channel();
    let screen_lock = start_screen_lock_monitor(sender.clone())?;
    let system_sleep = start_sleep_monitor(sender)?;
    if !screen_lock && !system_sleep {
        return Err("macOS did not provide screen-lock or system-sleep state".to_string());
    }
    Ok((
        receiver,
        MonitorSupport {
            screen_lock,
            system_sleep,
        },
    ))
}
