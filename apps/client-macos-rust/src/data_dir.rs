//! Keep the Rust library distinct from the native Swift client's schema.
use std::io;
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "com.gdaymeetings.macos.rust";
// Historical identity is only consulted for a one-time, non-merging migration.
pub const LEGACY_APP_ID: &str = "org.rankun.meeting-notes";

pub fn default_data_dir(home: &Path) -> io::Result<PathBuf> {
    let parent = home.join(".local/share");
    let destination = parent.join(APP_ID);
    // symlink_metadata also recognizes dangling symlinks: never replace one.
    match std::fs::symlink_metadata(&destination) {
        Ok(_) => return Ok(destination),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let legacy = parent.join(LEGACY_APP_ID);
    match std::fs::symlink_metadata(&legacy) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Legacy data path {} is not a regular directory; choose --data-dir explicitly",
                    legacy.display()
                ),
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(destination),
        Err(error) => return Err(error),
    }
    // Atomic, exclusive rename: no copy/merge and no destination clobber, including
    // another process creating the destination between the checks and this call.
    match rename_exclusive(&legacy, &destination) {
        Ok(()) => Ok(destination),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(destination),
        Err(error) => Err(io::Error::new(error.kind(), format!("Could not migrate {} to {}: {error}. Existing data is preserved; use --data-dir {} to open it explicitly", legacy.display(), destination.display(), legacy.display()))),
    }
}

#[cfg(target_os = "macos")]
fn rename_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Darwin sys/stdio.h: RENAME_EXCL = 0x00000004.
    let status = unsafe { renamex_np(source.as_ptr(), destination.as_ptr(), 0x00000004) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn rename_exclusive(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Automatic legacy migration requires macOS",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "gday-data-dir-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(path.join(".local/share")).unwrap();
            Self(path)
        }
        fn app(&self, id: &str) -> PathBuf {
            self.0.join(".local/share").join(id)
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn fresh_library_uses_owned_identity() {
        let home = Home::new();
        assert_eq!(default_data_dir(&home.0).unwrap(), home.app(APP_ID));
        assert!(!home.app(APP_ID).exists());
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn migrates_entire_legacy_library_once() {
        let home = Home::new();
        let legacy = home.app(LEGACY_APP_ID);
        std::fs::create_dir_all(legacy.join("recordings/session")).unwrap();
        std::fs::write(legacy.join("recordings/session/audio.wav"), b"original").unwrap();
        std::fs::write(legacy.join("settings.json"), b"settings").unwrap();
        let new = default_data_dir(&home.0).unwrap();
        assert_eq!(
            std::fs::read(new.join("recordings/session/audio.wav")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(new.join("settings.json")).unwrap(),
            b"settings"
        );
        assert!(!legacy.exists());
        assert_eq!(default_data_dir(&home.0).unwrap(), new);
    }
    #[test]
    fn existing_destination_never_merges_legacy() {
        let home = Home::new();
        std::fs::create_dir_all(home.app(APP_ID)).unwrap();
        std::fs::create_dir_all(home.app(LEGACY_APP_ID)).unwrap();
        std::fs::write(home.app(LEGACY_APP_ID).join("keep"), b"legacy").unwrap();
        assert_eq!(default_data_dir(&home.0).unwrap(), home.app(APP_ID));
        assert!(home.app(LEGACY_APP_ID).join("keep").exists());
        assert!(!home.app(APP_ID).join("keep").exists());
    }
    #[test]
    #[cfg(unix)]
    fn does_not_replace_destination_symlink_or_move_legacy_symlink() {
        let home = Home::new();
        std::os::unix::fs::symlink(home.0.join("missing"), home.app(APP_ID)).unwrap();
        assert_eq!(default_data_dir(&home.0).unwrap(), home.app(APP_ID));
        assert!(std::fs::symlink_metadata(home.app(APP_ID))
            .unwrap()
            .file_type()
            .is_symlink());
        std::fs::remove_file(home.app(APP_ID)).unwrap();
        std::os::unix::fs::symlink(home.0.join("missing"), home.app(LEGACY_APP_ID)).unwrap();
        assert!(default_data_dir(&home.0).is_err());
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn exclusive_rename_cannot_clobber_even_empty_destination() {
        let home = Home::new();
        std::fs::create_dir_all(home.app(APP_ID)).unwrap();
        std::fs::create_dir_all(home.app(LEGACY_APP_ID)).unwrap();
        assert!(rename_exclusive(&home.app(LEGACY_APP_ID), &home.app(APP_ID)).is_err());
        assert!(home.app(APP_ID).exists());
        assert!(home.app(LEGACY_APP_ID).exists());
    }
}
