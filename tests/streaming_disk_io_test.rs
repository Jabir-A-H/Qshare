use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use qshare::core::storage::{part_file_path, preflight_storage_check};
use qshare::core::transfer::{TransferError, TransferPipeline};

struct TestDirGuard {
    path: PathBuf,
}

impl TestDirGuard {
    fn new(name_prefix: &str) -> Self {
        let unique_id = uuid::Uuid::new_v4();
        let path = std::env::temp_dir().join(format!("qshare_test_{name_prefix}_{unique_id}"));
        std::fs::create_dir_all(&path).expect("Failed to create test temp dir");
        Self { path }
    }

    fn file_path(&self, filename: &str) -> PathBuf {
        self.path.join(filename)
    }
}

impl Drop for TestDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn test_in_memory_stream_transfer_roundtrip() {
    let guard = TestDirGuard::new("roundtrip");
    let target_file = guard.file_path("document.pdf");
    let part_path = part_file_path(&target_file);

    let payload = vec![0x5Au8; 512 * 1024]; // 512 KB
    let expected_hash = blake3::hash(&payload);
    let total_bytes = payload.len() as u64;

    let (sender_pipe, receiver_pipe) = tokio::io::duplex(64 * 1024);
    let (tx_sender_prog, _rx_sender_prog) = tokio::sync::mpsc::channel(32);
    let (tx_recv_prog, mut rx_recv_prog) = tokio::sync::mpsc::channel(32);

    let sender_cancel = CancellationToken::new();
    let receiver_cancel = CancellationToken::new();

    let payload_for_sender = payload.clone();
    let sender_task = tokio::spawn(async move {
        TransferPipeline::send_stream(
            &payload_for_sender[..],
            sender_pipe,
            total_bytes,
            tx_sender_prog,
            sender_cancel,
        )
        .await
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (send_res, recv_res) = tokio::join!(sender_task, receiver_task);
    let sender_final_hash = send_res.unwrap().expect("Sender failed");
    let receiver_final_path = recv_res.unwrap().expect("Receiver failed");

    assert_eq!(sender_final_hash, expected_hash);
    assert_eq!(receiver_final_path, target_file);

    // Assert final destination file exists and hash matches payload
    let written_bytes = tokio::fs::read(&target_file)
        .await
        .expect("Failed to read destination file");
    assert_eq!(written_bytes.len(), payload.len());
    assert_eq!(blake3::hash(&written_bytes), expected_hash);

    // Assert intermediate .part file was atomically renamed
    assert!(
        !part_path.exists(),
        ".part file must not exist after completion"
    );

    // Verify progress frames: final completion frame must report 100%
    let mut last_progress = None;
    while let Ok(prog) = rx_recv_prog.try_recv() {
        last_progress = Some(prog);
    }
    let final_prog = last_progress.expect("Expected at least one progress frame");
    assert_eq!(final_prog.bytes_transferred, total_bytes);
    assert_eq!(final_prog.total_bytes, total_bytes);
    assert_eq!(final_prog.eta_secs, Some(0));
}

#[tokio::test]
async fn test_corrupt_chunk_fails_hash_verification() {
    let guard = TestDirGuard::new("corrupt");
    let target_file = guard.file_path("corrupted_video.mp4");
    let part_path = part_file_path(&target_file);

    let payload = vec![0xA5u8; 256 * 1024]; // 256 KB
    let expected_actual_hash = blake3::hash(&payload);
    let wrong_hash = blake3::hash(b"tampered content signature");
    let total_bytes = payload.len() as u64;

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(64 * 1024);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        sender_pipe.write_all(&payload).await.unwrap();
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            wrong_hash,
            total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (_, recv_res) = tokio::join!(sender_task, receiver_task);
    let err = recv_res.unwrap().expect_err("Expected HashMismatch error");

    match err {
        TransferError::HashMismatch { expected, actual } => {
            assert_eq!(expected, wrong_hash.to_hex().to_string());
            assert_eq!(actual, expected_actual_hash.to_hex().to_string());
        }
        other => panic!("Expected HashMismatch, got: {other:?}"),
    }

    // Assert corrupted .part file was purged and destination was never created
    assert!(
        !target_file.exists(),
        "Corrupted destination file must not exist"
    );
    assert!(!part_path.exists(), "Corrupted .part file must be removed");
}

#[tokio::test]
async fn test_premature_stream_termination_preserves_part_file() {
    let guard = TestDirGuard::new("premature_eof");
    let target_file = guard.file_path("large_archive.iso");
    let part_path = part_file_path(&target_file);

    let advertised_total_bytes: u64 = 512 * 1024; // 512 KB advertised
    let actual_bytes_sent: usize = 128 * 1024; // Only 128 KB sent before Wi-Fi cut
    let partial_payload = vec![0xEFu8; actual_bytes_sent];
    let expected_hash = blake3::hash(&vec![0xEFu8; advertised_total_bytes as usize]);

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(64 * 1024);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        // Send only 128 KB and abruptly drop sender pipe (simulating network crash / sleep)
        sender_pipe.write_all(&partial_payload).await.unwrap();
        sender_pipe.flush().await.unwrap();
        drop(sender_pipe);
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            advertised_total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (_, recv_res) = tokio::join!(sender_task, receiver_task);
    let err = recv_res
        .unwrap()
        .expect_err("Expected IncompleteStream error");

    match err {
        TransferError::IncompleteStream { received, expected } => {
            assert_eq!(received, actual_bytes_sent as u64);
            assert_eq!(expected, advertised_total_bytes);
        }
        other => panic!("Expected IncompleteStream, got: {other:?}"),
    }

    // SEC4-CRIT-01 Invariant: Partial file MUST be preserved on disk for resumption!
    assert!(
        part_path.exists(),
        "CRITICAL: .part file MUST NOT be deleted upon premature stream termination!"
    );
    let part_len = tokio::fs::metadata(&part_path).await.unwrap().len();
    assert_eq!(part_len, actual_bytes_sent as u64);

    // Destination file must NOT exist
    assert!(
        !target_file.exists(),
        "Final file must not exist on incomplete transfer"
    );
}

#[tokio::test]
async fn test_zero_byte_file_transfer() {
    let guard = TestDirGuard::new("zero_byte");
    let target_file = guard.file_path("empty.txt");
    let part_path = part_file_path(&target_file);

    let empty_payload = b"";
    let expected_hash = blake3::hash(empty_payload);
    let total_bytes: u64 = 0;

    let (sender_pipe, receiver_pipe) = tokio::io::duplex(1024);
    let (tx_sender_prog, _) = tokio::sync::mpsc::channel(32);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);

    let sender_cancel = CancellationToken::new();
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        TransferPipeline::send_stream(
            &empty_payload[..],
            sender_pipe,
            total_bytes,
            tx_sender_prog,
            sender_cancel,
        )
        .await
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (send_res, recv_res) = tokio::join!(sender_task, receiver_task);
    assert_eq!(send_res.unwrap().unwrap(), expected_hash);
    assert_eq!(recv_res.unwrap().unwrap(), target_file);

    // Verify 0-byte file was created and finalized
    assert!(target_file.exists());
    let metadata = tokio::fs::metadata(&target_file).await.unwrap();
    assert_eq!(metadata.len(), 0);
    assert!(!part_path.exists());
}

#[tokio::test]
async fn test_cancellation_aborts_cleanly() {
    let guard = TestDirGuard::new("cancellation");
    let target_file = guard.file_path("cancelled.dat");
    let part_path = part_file_path(&target_file);

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(64 * 1024);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);
    let cancel_token = CancellationToken::new();

    let token_clone = cancel_token.clone();
    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            blake3::hash(b"unused"),
            1024 * 1024,
            tx_recv_prog,
            token_clone,
        )
        .await
    });

    // Write a small chunk then fire cancellation
    sender_pipe.write_all(&[0x11u8; 1024]).await.unwrap();
    cancel_token.cancel();

    let recv_res = receiver_task.await.unwrap();
    assert_eq!(recv_res, Err(TransferError::Cancelled));

    // Destination file must not exist
    assert!(!target_file.exists());
    // Partial file handle was cleanly closed and flushed
    assert!(part_path.exists());
}

#[test]
fn test_part_file_path_preserves_multi_extension() {
    assert_eq!(
        part_file_path(Path::new("photo.png")),
        PathBuf::from("photo.png.qshare.part")
    );
    assert_eq!(
        part_file_path(Path::new("backup.tar.gz")),
        PathBuf::from("backup.tar.gz.qshare.part")
    );
    assert_eq!(
        part_file_path(Path::new("C:/Users/Downloads/archive.tar.bz2")),
        PathBuf::from("C:/Users/Downloads/archive.tar.bz2.qshare.part")
    );
}

#[test]
fn test_preflight_storage_check_capacity() {
    let temp_dir = std::env::temp_dir();

    // 1 KB should succeed on any machine running tests
    assert!(preflight_storage_check(&temp_dir, 1024).is_ok());

    // u64::MAX (18.4 Exabytes) must fail with InsufficientStorage
    let res = preflight_storage_check(&temp_dir, u64::MAX);
    match res {
        Err(TransferError::InsufficientStorage {
            available,
            required,
        }) => {
            assert_eq!(required, u64::MAX);
            assert!(available < u64::MAX);
        }
        other => panic!("Expected InsufficientStorage error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_receive_stream_auto_creates_parent_directories() {
    let guard = TestDirGuard::new("nested_dirs");
    let target_file = guard
        .path
        .join("nested_a")
        .join("nested_b")
        .join("nested_c")
        .join("received_file.dat");
    let part_path = part_file_path(&target_file);

    // Ensure parent directory hierarchy does not exist beforehand
    assert!(!target_file.parent().unwrap().exists());

    let payload = b"Data transferred to a dynamically created subdirectory hierarchy.";
    let expected_hash = blake3::hash(payload);
    let total_bytes = payload.len() as u64;

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(1024);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        sender_pipe.write_all(payload).await.unwrap();
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (_, recv_res) = tokio::join!(sender_task, receiver_task);
    let result_path = recv_res
        .unwrap()
        .expect("Receiver should auto-create parent directories and succeed");

    assert_eq!(result_path, target_file);
    assert!(target_file.exists());
    assert!(!part_path.exists());

    let file_content = tokio::fs::read(&target_file).await.unwrap();
    assert_eq!(file_content, payload);
}

#[tokio::test]
async fn test_stream_exceeding_total_bytes_aborts_and_cleans_up() {
    let guard = TestDirGuard::new("oversized");
    let target_file = guard.file_path("oversized_payload.bin");
    let part_path = part_file_path(&target_file);

    let advertised_total_bytes: u64 = 512;
    // Malicious or misbehaving sender pushes 1024 bytes (exceeding advertised length)
    let payload = vec![0xEEu8; 1024];
    let expected_hash = blake3::hash(&payload[..advertised_total_bytes as usize]);

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(256);
    let (tx_recv_prog, _) = tokio::sync::mpsc::channel(32);
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        // Stream oversized bytes until writer or pipe breaks
        let _ = sender_pipe.write_all(&payload).await;
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            advertised_total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (_, recv_res) = tokio::join!(sender_task, receiver_task);
    let err = recv_res
        .unwrap()
        .expect_err("Receiver must reject stream exceeding advertised total bytes");

    match err {
        TransferError::IncompleteStream { received, expected } => {
            assert!(
                received > advertised_total_bytes,
                "Received bytes ({received}) should exceed expected ({advertised_total_bytes})"
            );
            assert_eq!(expected, advertised_total_bytes);
        }
        other => panic!("Expected IncompleteStream error, got: {other:?}"),
    }

    // DoS / corrupt stream invariant: .part file must be cleaned up and target file must not exist
    assert!(
        !part_path.exists(),
        "Oversized .part file must be deleted upon exceeding advertised length"
    );
    assert!(
        !target_file.exists(),
        "Target file must not be created on failed stream"
    );
}

#[tokio::test]
async fn test_fast_transfer_computes_non_zero_throughput() {
    let guard = TestDirGuard::new("fast_throughput");
    let target_file = guard.file_path("instant.bin");

    let payload = vec![0xAAu8; 64 * 1024]; // 64 KB, finishes in < 5ms in-memory
    let expected_hash = blake3::hash(&payload);
    let total_bytes = payload.len() as u64;

    let (mut sender_pipe, receiver_pipe) = tokio::io::duplex(64 * 1024);
    let (tx_recv_prog, mut rx_recv_prog) = tokio::sync::mpsc::channel(32);
    let receiver_cancel = CancellationToken::new();

    let sender_task = tokio::spawn(async move {
        sender_pipe.write_all(&payload).await.unwrap();
    });

    let target_clone = target_file.clone();
    let receiver_task = tokio::spawn(async move {
        TransferPipeline::receive_stream(
            receiver_pipe,
            &target_clone,
            expected_hash,
            total_bytes,
            tx_recv_prog,
            receiver_cancel,
        )
        .await
    });

    let (_, recv_res) = tokio::join!(sender_task, receiver_task);
    recv_res.unwrap().expect("Receiver failed");

    // Gather all emitted progress frames
    let mut final_prog = None;
    while let Ok(prog) = rx_recv_prog.try_recv() {
        final_prog = Some(prog);
    }

    let prog = final_prog.expect("Receiver must deliver at least the final completion frame");
    assert_eq!(prog.bytes_transferred, total_bytes);
    assert_eq!(prog.total_bytes, total_bytes);
    assert_eq!(prog.eta_secs, Some(0));
    assert!(
        prog.speed_bps > 0.0,
        "Fast transfer (< 50ms) must calculate non-zero throughput from elapsed duration, got {}",
        prog.speed_bps
    );
}
