use super::transfer::TransferError;
use std::path::{Path, PathBuf};

/// File extension appended to files while an active transfer is in progress.
/// Note: This is ALWAYS appended to the complete existing file name, NEVER replaced.
pub const PART_EXTENSION: &str = "qshare.part";

/// Generates the partial file path by appending `.{PART_EXTENSION}` to the file name.
/// For example, `photo.png` -> `photo.png.qshare.part`,
/// and `backup.tar.gz` -> `backup.tar.gz.qshare.part`.
pub fn part_file_path(destination_path: &Path) -> PathBuf {
    let file_name = destination_path.file_name().unwrap_or_default();
    let mut part_name = file_name.to_os_string();
    part_name.push(".");
    part_name.push(PART_EXTENSION);
    destination_path.with_file_name(part_name)
}

/// Queries available disk space in bytes for the volume hosting `dir`.
#[cfg(windows)]
pub fn get_available_space(dir: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;

    // Resolve directory: if path doesn't exist, ascend to the nearest existing ancestor
    let mut check_dir = if dir.as_os_str().is_empty() {
        std::env::current_dir()?
    } else if dir.is_relative() {
        std::env::current_dir()?.join(dir)
    } else {
        dir.to_path_buf()
    };

    while !check_dir.exists() {
        match check_dir.parent() {
            Some(parent) => check_dir = parent.to_path_buf(),
            None => break,
        }
    }

    let mut wide: Vec<u16> = check_dir.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_bytes_available: u64 = 0;
    let mut total_number_of_bytes: u64 = 0;
    let mut total_number_of_free_bytes: u64 = 0;

    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }

    let res = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available,
            &mut total_number_of_bytes,
            &mut total_number_of_free_bytes,
        )
    };

    if res != 0 {
        Ok(free_bytes_available)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Fallback for non-Windows platforms (e.g. Unix/Linux test harnesses)
#[cfg(not(windows))]
pub fn get_available_space(_dir: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

/// Pre-flight disk space check before accepting or allocating an incoming file.
/// Returns `Err(TransferError::InsufficientStorage)` if the target volume lacks capacity.
pub fn preflight_storage_check(
    destination_dir: &Path,
    required_bytes: u64,
) -> Result<(), TransferError> {
    let available = get_available_space(destination_dir)
        .map_err(|e| TransferError::StorageError(format!("Failed to query disk space: {e}")))?;

    if available < required_bytes {
        return Err(TransferError::InsufficientStorage {
            available,
            required: required_bytes,
        });
    }

    Ok(())
}

/// Atomically renames a `.part` file to the final destination path.
/// Includes an asynchronous retry loop with exponential backoff to handle transient
/// Windows antivirus / search indexer sharing locks (`ERROR_SHARING_VIOLATION` / `ERROR_ACCESS_DENIED`).
pub async fn atomic_rename_with_retry(from: &Path, to: &Path) -> Result<(), TransferError> {
    let max_attempts = 10;
    let mut delay = std::time::Duration::from_millis(50);

    for attempt in 1..=max_attempts {
        match tokio::fs::rename(from, to).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                let raw_err = err.raw_os_error();

                // Cross-volume rename fallback: Windows ERROR_NOT_SAME_DEVICE is 17, Unix EXDEV is 18
                let is_cross_device = err.kind() == std::io::ErrorKind::CrossesDevices
                    || (cfg!(windows) && raw_err == Some(17))
                    || (cfg!(unix) && raw_err == Some(18));

                if is_cross_device {
                    tokio::fs::copy(from, to).await.map_err(|e| {
                        TransferError::AtomicRenameFailed(format!(
                            "Cross-volume fallback copy from '{}' to '{}' failed: {e}",
                            from.display(),
                            to.display()
                        ))
                    })?;
                    let _ = tokio::fs::remove_file(from).await;
                    return Ok(());
                }

                // Windows ERROR_SHARING_VIOLATION is 32, ERROR_ACCESS_DENIED is 5
                let is_transient_lock =
                    cfg!(windows) && (raw_err == Some(32) || raw_err == Some(5));
                if attempt == max_attempts || (!is_transient_lock && raw_err.is_some()) {
                    return Err(TransferError::AtomicRenameFailed(format!(
                        "Failed to rename '{}' to '{}' (attempt {attempt}/{max_attempts}): {err}",
                        from.display(),
                        to.display()
                    )));
                }
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(
                    delay.saturating_mul(2),
                    std::time::Duration::from_millis(500),
                );
            }
        }
    }

    Err(TransferError::AtomicRenameFailed(format!(
        "Failed to rename '{}' to '{}': retry limit exceeded",
        from.display(),
        to.display()
    )))
}

/// Flushes and syncs all buffered data and filesystem metadata to physical disk,
/// drops the file handle, and atomically renames the partial file to destination.
pub async fn finalize_part_file(
    file: tokio::fs::File,
    part_path: &Path,
    destination_path: &Path,
) -> Result<(), TransferError> {
    file.sync_all()
        .await
        .map_err(|e| TransferError::DiskWriteError(e.to_string()))?;
    drop(file);
    atomic_rename_with_retry(part_path, destination_path).await
}
