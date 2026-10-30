use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::storage;

/// Default bounded chunk size for file transfers (128 KB).
pub const CHUNK_SIZE: usize = 128 * 1024;
pub const DEFAULT_CHUNK_SIZE: usize = CHUNK_SIZE;

/// Telemetry dispatch frequency cap (20 Hz = at most once every 50 ms).
const PROGRESS_THROTTLE_INTERVAL: Duration = Duration::from_millis(50);

/// Exponential Moving Average smoothing factor for throughput calculation (~0.15).
const EMA_ALPHA: f64 = 0.15;

/// Telemetry progress payload channeled from the Tokio worker to the UI subscription.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
    pub eta_secs: Option<u64>,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum TransferError {
    #[error("Disk read error: {0}")]
    DiskReadError(String),
    #[error("Disk write error: {0}")]
    DiskWriteError(String),
    #[error("Network stream error: {0}")]
    NetworkStreamError(String),
    #[error("Cryptographic hash mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("Incomplete stream: received {received} bytes, expected {expected} bytes")]
    IncompleteStream { received: u64, expected: u64 },
    #[error("Insufficient storage space: {available} bytes available, {required} bytes required")]
    InsufficientStorage { available: u64, required: u64 },
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Failed to atomically rename .part file: {0}")]
    AtomicRenameFailed(String),
    #[error("Transfer was cancelled")]
    Cancelled,
}

pub struct TransferPipeline;

impl TransferPipeline {
    /// Sender: Reads payload in bounded 128 KB chunks, updates incremental BLAKE3 hasher,
    /// and streams bytes across the asynchronous writer with throttled EMA telemetry.
    pub async fn send_stream<R, W>(
        reader: R,
        writer: W,
        total_bytes: u64,
        progress_tx: tokio::sync::mpsc::Sender<TransferProgress>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<blake3::Hash, TransferError>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        send_stream(reader, writer, total_bytes, progress_tx, cancel_token).await
    }

    /// Receiver: Streams chunks to a `.part` file, updates incremental BLAKE3 hasher,
    /// verifies complete byte count and hash, and atomically renames the file upon completion.
    pub async fn receive_stream<R>(
        reader: R,
        destination_path: &Path,
        expected_hash: blake3::Hash,
        total_bytes: u64,
        progress_tx: tokio::sync::mpsc::Sender<TransferProgress>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<PathBuf, TransferError>
    where
        R: AsyncRead + Unpin,
    {
        receive_stream(
            reader,
            destination_path,
            expected_hash,
            total_bytes,
            progress_tx,
            cancel_token,
        )
        .await
    }
}

/// Sender implementation: Streams chunks, tracks incremental BLAKE3 hash,
/// and delivers throttled progress updates to the caller.
pub async fn send_stream<R, W>(
    mut reader: R,
    mut writer: W,
    total_bytes: u64,
    progress_tx: tokio::sync::mpsc::Sender<TransferProgress>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<blake3::Hash, TransferError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    let transfer_start = Instant::now();
    let mut last_progress_instant = Instant::now();
    let mut last_progress_bytes: u64 = 0;
    let mut smoothed_speed_bps: f64 = 0.0;

    loop {
        let bytes_read = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(TransferError::Cancelled);
            }
            read_res = reader.read(&mut buffer) => {
                read_res.map_err(|e| TransferError::DiskReadError(e.to_string()))?
            }
        };

        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];
        hasher.update(chunk);

        tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(TransferError::Cancelled);
            }
            write_res = writer.write_all(chunk) => {
                write_res.map_err(|e| TransferError::NetworkStreamError(e.to_string()))?;
            }
        }

        transferred += bytes_read as u64;

        // Progress telemetry throttle (<= 20 Hz, every 50ms) with EMA speed
        let now = Instant::now();
        let elapsed = now.duration_since(last_progress_instant);
        if elapsed >= PROGRESS_THROTTLE_INTERVAL {
            let delta_bytes = transferred.saturating_sub(last_progress_bytes);
            let dt = elapsed.as_secs_f64();
            let instant_speed = if dt > 0.0 {
                delta_bytes as f64 / dt
            } else {
                0.0
            };

            smoothed_speed_bps = if smoothed_speed_bps == 0.0 {
                instant_speed
            } else {
                EMA_ALPHA * instant_speed + (1.0 - EMA_ALPHA) * smoothed_speed_bps
            };

            last_progress_instant = now;
            last_progress_bytes = transferred;

            let remaining = total_bytes.saturating_sub(transferred);
            let eta_secs = if smoothed_speed_bps > 0.0 {
                Some((remaining as f64 / smoothed_speed_bps) as u64)
            } else {
                None
            };

            let _ = progress_tx.try_send(TransferProgress {
                bytes_transferred: transferred,
                total_bytes,
                speed_bps: smoothed_speed_bps,
                eta_secs,
            });
        }
    }

    if transferred < total_bytes {
        return Err(TransferError::IncompleteStream {
            received: transferred,
            expected: total_bytes,
        });
    }

    tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(TransferError::Cancelled);
        }
        flush_res = writer.flush() => {
            flush_res.map_err(|e| TransferError::NetworkStreamError(e.to_string()))?;
        }
    }

    // Guarantee delivery of the final 100% completion frame
    let final_speed = if smoothed_speed_bps > 0.0 {
        smoothed_speed_bps
    } else {
        let total_elapsed = transfer_start.elapsed().as_secs_f64();
        if total_elapsed > 0.0 {
            transferred as f64 / total_elapsed
        } else {
            0.0
        }
    };

    let _ = progress_tx
        .send(TransferProgress {
            bytes_transferred: transferred,
            total_bytes,
            speed_bps: final_speed,
            eta_secs: Some(0),
        })
        .await;

    Ok(hasher.finalize())
}

/// Receiver implementation: Streams chunks into a `.part` file, hashes incrementally,
/// validates byte count before root hash verification, and executes atomic finalization.
pub async fn receive_stream<R>(
    mut reader: R,
    destination_path: &Path,
    expected_hash: blake3::Hash,
    total_bytes: u64,
    progress_tx: tokio::sync::mpsc::Sender<TransferProgress>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<PathBuf, TransferError>
where
    R: AsyncRead + Unpin,
{
    // Pre-flight disk space check and parent directory creation
    let parent_dir = destination_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty());

    if let Some(parent) = parent_dir {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| TransferError::DiskWriteError(e.to_string()))?;
        storage::preflight_storage_check(parent, total_bytes)?;
    } else {
        storage::preflight_storage_check(Path::new("."), total_bytes)?;
    }

    let part_path = storage::part_file_path(destination_path);
    let mut part_file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| TransferError::DiskWriteError(e.to_string()))?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut transferred: u64 = 0;

    let transfer_start = Instant::now();
    let mut last_progress_instant = Instant::now();
    let mut last_progress_bytes: u64 = 0;
    let mut smoothed_speed_bps: f64 = 0.0;

    loop {
        let bytes_read = tokio::select! {
            _ = cancel_token.cancelled() => {
                let _ = part_file.sync_all().await;
                drop(part_file);
                return Err(TransferError::Cancelled);
            }
            read_res = reader.read(&mut buffer) => {
                read_res.map_err(|e| TransferError::NetworkStreamError(e.to_string()))?
            }
        };

        if bytes_read == 0 {
            break;
        }

        // Guard against oversized streams (DoS / sender sending more than advertised)
        if transferred.saturating_add(bytes_read as u64) > total_bytes {
            drop(part_file);
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(TransferError::IncompleteStream {
                received: transferred + bytes_read as u64,
                expected: total_bytes,
            });
        }

        let chunk = &buffer[..bytes_read];
        hasher.update(chunk);

        tokio::select! {
            _ = cancel_token.cancelled() => {
                let _ = part_file.sync_all().await;
                drop(part_file);
                return Err(TransferError::Cancelled);
            }
            write_res = part_file.write_all(chunk) => {
                write_res.map_err(|e| TransferError::DiskWriteError(e.to_string()))?;
            }
        }

        transferred += bytes_read as u64;

        // Progress telemetry throttle (<= 20 Hz, every 50ms) with EMA speed
        let now = Instant::now();
        let elapsed = now.duration_since(last_progress_instant);
        if elapsed >= PROGRESS_THROTTLE_INTERVAL {
            let delta_bytes = transferred.saturating_sub(last_progress_bytes);
            let dt = elapsed.as_secs_f64();
            let instant_speed = if dt > 0.0 {
                delta_bytes as f64 / dt
            } else {
                0.0
            };

            smoothed_speed_bps = if smoothed_speed_bps == 0.0 {
                instant_speed
            } else {
                EMA_ALPHA * instant_speed + (1.0 - EMA_ALPHA) * smoothed_speed_bps
            };

            last_progress_instant = now;
            last_progress_bytes = transferred;

            let remaining = total_bytes.saturating_sub(transferred);
            let eta_secs = if smoothed_speed_bps > 0.0 {
                Some((remaining as f64 / smoothed_speed_bps) as u64)
            } else {
                None
            };

            let _ = progress_tx.try_send(TransferProgress {
                bytes_transferred: transferred,
                total_bytes,
                speed_bps: smoothed_speed_bps,
                eta_secs,
            });
        }
    }

    // CRITICAL (SEC4-CRIT-01): EOF reached.
    // Check transferred == total_bytes BEFORE verifying root hash!
    if transferred < total_bytes {
        // Stream aborted prematurely (e.g. Wi-Fi drop, laptop sleep, remote peer exit).
        // PRESERVE the .part file on disk for future resumption. Do NOT delete!
        let _ = part_file.sync_all().await;
        drop(part_file);
        return Err(TransferError::IncompleteStream {
            received: transferred,
            expected: total_bytes,
        });
    }

    // Verify cryptographic integrity
    let actual_hash = hasher.finalize();
    if actual_hash != expected_hash {
        // Full stream received but payload corrupted: safe deletion / quarantine of corrupt file
        drop(part_file);
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(TransferError::HashMismatch {
            expected: expected_hash.to_hex().to_string(),
            actual: actual_hash.to_hex().to_string(),
        });
    }

    // Atomic finalize: sync_all -> drop file handle -> atomic rename with retry
    storage::finalize_part_file(part_file, &part_path, destination_path).await?;

    // Guarantee delivery of the final 100% completion frame
    let final_speed = if smoothed_speed_bps > 0.0 {
        smoothed_speed_bps
    } else {
        let total_elapsed = transfer_start.elapsed().as_secs_f64();
        if total_elapsed > 0.0 {
            transferred as f64 / total_elapsed
        } else {
            0.0
        }
    };

    let _ = progress_tx
        .send(TransferProgress {
            bytes_transferred: transferred,
            total_bytes,
            speed_bps: final_speed,
            eta_secs: Some(0),
        })
        .await;

    Ok(destination_path.to_path_buf())
}
