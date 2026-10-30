# Architecture Critique: Segment 4 — Streaming Disk I/O & Chunk Verification

**Document Version:** 1.0.0  
**Target:** Qshare Desktop Application (`d:\WebDev\rust\Qshare`)  
**Status:** Under Architectural Review  
**Evaluators:** Critic 1 (Investigator) & Critic 2 (Adversarial Doubt Gate)  
**Date:** 2026-10-30  

---

## 1. Executive Summary & Health Assessment

- **Segment Health Score:** 44/100
- **Summary:**  
  Segment 4 (`streaming-disk-io`) constitutes the critical data plane of Qshare, governing bounded asynchronous chunk streaming (64 KB – 256 KB), real-time cryptographic hashing via BLAKE3, backpressure propagation across QUIC channels, and atomic filesystem finalization (`.qshare.part` to final destination file). While the conceptual model cleanly adheres to the core mission of pure Rust zero-cloud file delivery without loading multi-gigabyte payloads into RAM, adversarial stress-testing reveals severe architectural and platform-specific vulnerabilities that would lead to catastrophic data loss and silent transfer aborts in real-world environments.

  The proposed streaming pipeline in [`segment_specs.md`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L656-L840) fails to distinguish between clean EOF and premature network disconnection. An unexpected stream termination (e.g., laptop sleep, Wi-Fi drop) at 99 GB of a 100 GB transfer causes the receiver to treat incomplete bytes as a cryptographic hash mismatch, destructively deleting the entire `.part` file and violating the core preservation invariant. Furthermore, the path manipulation logic (`Path::with_extension`) strips original file extensions (e.g., `archive.tar.gz` becomes `archive.tar.qshare.part`) and induces filename collisions between distinct files sharing base names (`photo.jpg` vs `photo.png`). On the Windows platform, unhandled antivirus file locks (Windows Defender / MsMpEng) during atomic renaming will trigger fatal `ERROR_SHARING_VIOLATION` (OS Error 32) aborts, while missing `sync_all()` calls leave uncommitted filesystem dirty pages vulnerable to corruption on power drops.

- **Invariant Compliance Status:**
  - *Zero-Cloud Storage Invariant (0 MB Relay Disk):* **COMPLIANT** (Bytes stream directly from disk to QUIC socket to disk with zero cloud caching).
  - *Non-Blocking UI Thread Invariant:* **COMPLIANT** (All streaming executes strictly inside asynchronous Tokio tasks decoupled from the Iced GUI thread).
  - *Atomic Disk Finalization Invariant:* **DRIFT DETECTED** (Premature EOF triggers destructive deletion rather than quarantine/resumption; atomic rename lacks antivirus sharing violation retries and filesystem sync barriers).
  - *Rust Memory Safety & Pure Native Binary:* **AT RISK** (Missing critical crate dependencies in [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml) prevents compilation; unhandled `StorageFull` and lack of pre-flight disk capacity checks threaten host system stability).

---

## 2. Desktop UX & Visual Craft Audit (`/impeccable`)

### 2.1 Visual Hierarchy & Layout Architecture
- **Progress Visibility & Visual Weight:** In the proposed Elm state transitions, transfer telemetry (`TransferProgress`) is channeled from Tokio workers to the Iced application shell. However, the UI does not distinguish between active wire transmission, disk flushing, and BLAKE3 finalization. For multi-gigabyte files, `part_file.sync_all()` and `hasher.finalize()` can take several seconds on mechanical hard drives or slow SATA SSDs; without an explicit "Finalizing & Verifying Integrity..." visual state, the UI appears frozen at 100%.
- **Dark Theme Consistency & Rhythm:** The UI shell in [`src/main.rs:30`](file:///d:/WebDev/rust/Qshare/src/main.rs#L30) specifies `Theme::Dark`. Transfer drawer components must maintain contrast ratios conforming to WCAG 2.1 AA (4.5:1 for normal text). Gauge bars should use subtle rounded borders with high-contrast accent fills (`#3B82F6` primary blue or `#10B981` emerald green for completed transfers, `#EF4444` rose red for hash mismatch alerts).

### 2.2 Component State Matrix
| Component / Widget | Default | Hover / Focus | Active / Loading | Disabled | Empty State | Error State |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Transfer Progress Drawer** | Hidden | N/A | Slides up from bottom | N/A | "No active transfers" | Red alert header |
| **Throughput Gauge (MB/s)** | `0.0 MB/s` | Tooltip: Instantaneous vs Rolling | Live smoothed rate | Grayed out on pause | `---` | `Stalled (0 KB/s)` |
| **ETA Indicator** | `Calculating...` | Tooltip: Estimated completion | Dynamic time formatting | Grayed out | `---` | `Unknown` |
| **Cancel Transfer CTA** | Visible outline | Highlight red border | Spinner on aborting | Disabled when finalizing | Hidden | Replaced by "Retry" |
| **Atomic Swap Notification** | Hidden | N/A | "Verifying integrity..." | N/A | N/A | "Hash verification failed!" |

### 2.3 Microcopy, Telemetry & User Feedback
- **Speed Smoothing & ETA Microcopy:** The current implementation in [`segment_specs.md:720-722`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L720-L722) calculates `speed_bps` cumulatively (`transferred / start_time.elapsed()`). This causes the speed display to become sluggish and unresponsive over long transfers. If throughput drops abruptly from 50 MB/s to 500 KB/s, the cumulative metric takes tens of minutes to reflect reality, while ETA oscillates wildly.
- **Actionable Error Microcopy:** Generic errors like `TransferError::DiskWriteError` or `TransferError::HashMismatch` must be translated into human-readable, non-blaming language:
  - *Cryptographic Mismatch:* *"Transfer was interrupted or corrupted in transit. The temporary file was safely preserved for verification."* (Never unilaterally claim "tampering" without actionable next steps).
  - *Storage Full:* *"Not enough disk space on drive D:\. Qshare requires an additional 4.2 GB to complete this download."*

---

## 3. 5-Layer Behavioral Flow Trace (`/behavioral-audit`)

```
Layer 1: Entry Trigger       [ User Selects File / Peer Accepts Incoming Ticket ]
         │
Layer 2: UI Event Dispatch   [ Iced Message::TransferStarted -> update(&mut self) ]
         │
Layer 3: Async Channel       [ tokio::sync::mpsc::channel(32) -> Progress / Cancel ]
         │
Layer 4: Tokio Background    [ TransferPipeline::send_stream / receive_stream (128 KB) ]
         │                   [ BLAKE3 Incremental Hasher + AsyncRead/AsyncWrite ]
         │
Layer 5: State Sync & UI     [ ProgressUpdated -> Atomic Rename -> Message::Completed ]
```

### 3.1 Flow Trace Analysis
- **Primary Happy Path:**
  1. *Layer 1:* Incoming peer handshake completes in Segment 3; receiver accepts stream offer for `destination_path` with known `expected_hash` and `total_bytes`.
  2. *Layer 2:* Iced dispatches `Message::TransferStarted`, initializing the transfer drawer and allocating `tokio::sync::mpsc::channel(32)`.
  3. *Layer 3:* Background Tokio task spawns `TransferPipeline::receive_stream`, linking `progress_tx` to `iced::Subscription`.
  4. *Layer 4:* Chunks (128 KB) stream asynchronously via `reader.read(&mut buffer)`. Each slice updates `blake3::Hasher` and writes to `destination.qshare.part` via `tokio::fs::File`.
  5. *Layer 5:* At stream completion, BLAKE3 root hash is verified against `expected_hash`. If valid, `sync_all()` commits pages, `part_file` is dropped, `tokio::fs::rename` finalizes the target file, and `Message::TransferCompleted(Ok(()))` updates the GUI.

- **Unwired CTAs & Dead Ends:**
  - *No Cancellation Plumbing:* The transfer loops in `send_stream` and `receive_stream` take no cancellation token or abort receiver. If the user clicks "Cancel Transfer" in the UI, there is no channel seam to halt streaming cleanly. Abruptly killing the Tokio task leaves open `.qshare.part` file descriptors and uncleaned temp files.

- **Mock Leaks & Stubs:**
  - *Directory Manifest Packaging (KI-003):* Listed as an in-scope requirement in [`docs/KNOWN_ISSUES.md:28-34`](file:///d:/WebDev/rust/Qshare/docs/KNOWN_ISSUES.md#L28-L34) and [`segment_specs.md:667`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L667), but the streaming architecture completely lacks directory recursion, tar headers, or manifest serialization stubs.

- **Error Swallowing & Silent Failures:**
  - *Silent Progress Dropping:* `let _ = progress_tx.try_send(...)` silently discards progress frames when the UI subscription lags. While necessary for backpressure, the final 100% completion frame can be dropped if the channel is full, causing the UI to show 99% progress when the success dialog appears.

- **UI Thread Blocking Hazards:**
  - *Path and File Metadata Checks:* If destination file existence checks or collision scans (`file (1).ext`) are performed synchronously on the UI thread before invoking the background worker, slow network shares (`\\NAS\Share`) will block the Iced rendering loop.

---

## 4. Headless Testability & TDD Strategy (`/test-driven-development`)

### 4.1 Decoupling & Testability Assessment
The `TransferPipeline` struct in `segment_specs.md` is well-isolated from Iced widgets, operating purely over generic `AsyncRead` and `AsyncWrite` traits. This allows 100% headless test execution using `tokio::io::duplex` in-memory pipes without requiring GPU surface initialization (`wgpu`), window managers, or physical network sockets.

However, the provided test suite in [`segment_specs.md:847-929`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L847-L929) suffers from test harness defects:
1. It references `uuid::Uuid::new_v4()`, but `uuid` is not listed in `Cargo.toml`.
2. It writes temporary files directly to `std::env::temp_dir()`, which risks cross-device rename failures (`EXDEV`) if the test runner executes on a different mount point.
3. It lacks negative test cases for premature stream termination, disk space exhaustion, and concurrent destination locks.

### 4.2 Required Headless Test Matrix
| Test Case Identifier | Target Behavior | Failure Injection / Stimulus | Expected Assertion |
| :--- | :--- | :--- | :--- |
| `TEST-SEC4-01` | In-Memory Streaming & Hash Roundtrip | 5 MB synthetic payload streamed through `tokio::io::duplex(64 KB)` | `hasher.finalize() == expected_hash`; 100% byte equality verified. |
| `TEST-SEC4-02` | Tampered Chunk Hash Failure | Invert single bit at offset 131,072 in transit | Returns `TransferError::HashMismatch`; `.part` file safely preserved or quarantined; no destination file created. |
| `TEST-SEC4-03` | Premature Stream Termination (Abrupt EOF) | Drop sender pipe after sending 256 KB of a 1024 KB stream | Returns `TransferError::IncompleteStream { expected: 1048576, received: 262144 }`; `.part` file NOT deleted. |
| `TEST-SEC4-04` | Destination Filename Collision Handling | Target file already exists on disk | Generates non-destructive path `target (1).ext`; original file untouched. |
| `TEST-SEC4-05` | Graceful Cancellation via Token | Fire `CancellationToken::cancel()` mid-stream | Returns `TransferError::Cancelled`; background file handle closed cleanly. |
| `TEST-SEC4-06` | Disk Space Pre-Flight Check | Request 50 GB transfer when mock storage reports 10 GB free | Immediately returns `TransferError::InsufficientStorage`; 0 bytes written to disk. |
| `TEST-SEC4-07` | Antivirus Sharing Violation Retry | Lock `.part` file handle exclusively during atomic rename | Pipeline retries with exponential backoff; succeeds once handle released. |

---

## 5. Official Source Grounding & API Compliance (`/source-driven-development`)

### 5.1 Crate & Dependency Compliance
- **Missing Dependencies in [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml):**
  - `thiserror`: Used at line 804 (`#[derive(thiserror::Error, Debug)]`), but missing from `Cargo.toml`. Compilation fails immediately.
  - `uuid`: Used in test harnesses (`uuid::Uuid::new_v4()`), but undeclared in `[dev-dependencies]`.
  - `tokio-util`: Required for `CancellationToken` and `SyncIoBridge` in cooperative task cancellation.
  - `sysinfo` or `fs2`: Required for pre-flight disk capacity queries (`fs2::available_space`).

### 5.2 Tokio & Asynchronous I/O Idioms
- **Missing `sync_all()` Before Rename:**  
  At line 783 of `segment_specs.md`, `part_file.flush().await` is invoked. In Tokio, `AsyncWriteExt::flush` only flushes internal software buffers in `BufWriter`; it does **not** issue the underlying OS system call (`FlushFileBuffers` on Windows or `fsync` on POSIX). Without `part_file.sync_all().await`, buffered data resides in the OS kernel cache. If power drops or the OS crashes immediately after rename, the resulting file will be truncated or corrupted with null padding.
- **Lockstep Sequential I/O Bottleneck:**  
  The current loop in `send_stream` executes:
  ```rust
  let bytes_read = reader.read(&mut buffer).await?;
  writer.write_all(chunk).await?;
  ```
  This creates a lockstep ping-pong: disk read waits for network write, and network write waits for disk read. Under 10 Gbps LAN or NVMe conditions, this halves achievable throughput. Pipelining disk I/O and network transmission via a bounded async channel (double-buffering) is required to saturate line rate.

---

## 6. Adversarial Doubt Gate: Real-World Field Failures (`/doubt-driven-development`)

> *"Under what real-world field conditions will this clean-looking code fail?"*

### 6.1 Field Failure Cross-Examination

#### 1. Windows Antivirus Sharing Violations (`ERROR_SHARING_VIOLATION` / `0x20`)
- **Field Reality:** On Windows 10/11, Windows Defender Real-time Protection (`MsMpEng.exe`), CrowdStrike, and the Windows Search Indexer attach filesystem minifilters to newly written files. When `part_file` is closed via `drop(part_file)` at line 784, Defender immediately grabs a shared-read lock to scan the newly written bytes.
- **Failure Mode:** At line 797, `tokio::fs::rename(&part_path, destination_path).await` executes milliseconds after file close. Because Defender holds an open handle, Windows kernel rejects the rename with `ERROR_SHARING_VIOLATION` (OS Error 32) or `ERROR_ACCESS_DENIED` (OS Error 5). The entire multi-gigabyte transfer fails at the 100% mark!
- **Prescription:** Implement a retry loop with exponential backoff specifically for atomic renaming on Windows (5 attempts over 500ms before returning `AtomicRenameFailed`).

#### 2. Premature EOF vs Cryptographic Tampering (Destructive Purge)
- **Field Reality:** A user downloads a 50 GB archive over Wi-Fi. At 48 GB, the laptop enters sleep, or the router drops. The QUIC stream abruptly closes with `UnexpectedEof` or `0` bytes read.
- **Failure Mode:** At line 760, `receive_stream` treats `bytes_read == 0` as clean completion. It calculates the BLAKE3 hash over the 48 GB received, discovers `actual_hash != expected_hash`, and executes:
  ```rust
  let _ = tokio::fs::remove_file(&part_path).await;
  ```
  **It permanently deletes 48 GB of valid downloaded data!** The user cannot resume the transfer, and gigabytes of bandwidth are permanently wasted.
- **Prescription:** Distinguish between `transferred < total_bytes` (Incomplete Stream / Interrupted) and `transferred == total_bytes && hash != expected` (Payload Tampered). On incomplete streams, preserve `.part` on disk and emit a recoverable error.

#### 3. Cross-Volume Renames (`EXDEV` / `ERROR_NOT_SAME_DEVICE`)
- **Field Reality:** If `.part` files are created in the OS default temp directory (`C:\Users\AppData\Local\Temp`), but the user's downloads folder resides on a secondary drive (`D:\Downloads` or external SSD), `tokio::fs::rename` maps to Win32 `MoveFileExW` without `MOVEFILE_COPY_ALLOWED`.
- **Failure Mode:** Windows refuses to move files across volume boundaries, returning `ERROR_NOT_SAME_DEVICE` (OS Error 17).
- **Prescription:** Enforce that `.part` files are **always allocated in the identical directory** as the final destination file (`destination_path.parent().unwrap()`), guaranteeing that renaming is a metadata-only pointer swap on the same filesystem partition.

#### 4. Extension Collision & Multi-Part Extension Mangling
- **Field Reality:** In `segment_specs.md:747`, `destination_path.with_extension(PART_EXTENSION)` is used.
- **Failure Mode:**
  1. *Extension Replacement:* In standard Rust, `Path::new("backup.tar.gz").with_extension("qshare.part")` replaces the *last* extension, generating `backup.tar.qshare.part`. When renamed back, tracking the original name becomes brittle.
  2. *Collision Vulnerability:* If a user transfers `photo.png` and `photo.jpg` to the same folder, both map to `photo.qshare.part`! If transferred concurrently or sequentially, they overwrite each other's temporary buffers.
- **Prescription:** Never use `with_extension`. Always append `.qshare.part` to the full file name:
  ```rust
  let mut part_name = destination_path.file_name().unwrap().to_os_string();
  part_name.push(".qshare.part");
  let part_path = destination_path.with_file_name(part_name);
  ```

#### 5. Windows Sleep & Modern Standby (S0ix) During Transfer
- **Field Reality:** During a 100 GB transfer taking 45 minutes, Windows default power plans sleep after 15 minutes of user inactivity.
- **Failure Mode:** Network adapter powers down; QUIC connection drops silently mid-stream.
- **Prescription:** Integrate Windows power assertions via `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED)` when transfers start, clearing the flag upon completion.

---

## 7. Cross-Segment Cohesion & Ripple Impact Analysis

| Interacting Segment | Interface Seam | Ripple Risk / Failure Mode | Cohesion Rating |
| :--- | :--- | :--- | :---: |
| **Segment 1: `desktop-ui-shell`** | `TransferProgress` (MPSC Channel) | Unthrottled 128 KB chunk messages flood UI event queue at 1000+ msgs/sec, starving Iced redraws. | **VULNERABLE** |
| **Segment 3: `wan-iroh-transport`** | `iroh::endpoint::Connection` | Stream framing ambiguity: if metadata header is prepended to payload QUIC stream, BLAKE3 hash fails. | **STRONG** |
| **Segment 5: `security-resiliency`** | `PathSanitizer::jail_destination` | If `.part` file is created before path sanitization runs, path traversal payloads escape download sandbox. | **STRONG** |

### 7.1 Deadlock & Buffer Overflow Analysis
- **Progress Queue Contention:** At line 724 and 775, `progress_tx.try_send` is used with a buffer of 32. While `try_send` prevents blocking the disk engine, streaming at 1 Gbps emits ~1,000 progress events per second. The UI thread cannot process 1,000 events/sec without dropping frame rates. Progress dispatches must be throttled to a maximum of 20 Hz (every 50 ms).

### 7.2 State Desync Scenarios
- **Ghost Transfers in UI:** If `receive_stream` returns an error (e.g., `DiskWriteError`), but the background worker fails to send `TransferCompleted(Err(...))` to the UI, the Iced progress drawer remains stuck on screen indefinitely.

---

## 8. Prioritized Findings & Architectural Recommendations

### 8.1 Findings Log
| Finding ID | Severity | Category | Affected File & Lines | Description | Impact |
| :--- | :---: | :---: | :--- | :--- | :--- |
| `SEC4-CRIT-01` | `[CRITICAL]` | Data Integrity | [`segment_specs.md:759-794`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L759-L794) | Premature EOF treated as cryptographic hash mismatch, destructively deleting partial files. | Total loss of downloaded data on network drop or laptop sleep; prevents transfer resumption. |
| `SEC4-CRIT-02` | `[CRITICAL]` | Filesystem | [`segment_specs.md:747`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L747) | `Path::with_extension` replaces existing extension, causing naming collisions (`photo.jpg` vs `photo.png`). | Data corruption and file collision during parallel transfers. |
| `SEC4-CRIT-03` | `[CRITICAL]` | Build System | [`Cargo.toml:1-45`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L1-L45) | Undeclared crates: `thiserror`, `uuid`, and `tokio-util` missing from dependencies. | Immediate compilation failure when implementing Segment 4. |
| `SEC4-HIGH-01` | `[HIGH]` | Storage | [`segment_specs.md:748-768`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L748-L768) | Absence of pre-flight disk capacity checks and unhandled `StorageFull` errors. | System drive exhaustion, choking host OS and dropping transfers mid-flight. |
| `SEC4-HIGH-02` | `[HIGH]` | UX / Telemetry | [`segment_specs.md:719-722`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L719-L722) | Cumulative average throughput calculation leads to sluggish UI response and wild ETA swings. | Distorted transfer telemetry and poor user feedback. |
| `SEC4-HIGH-03` | `[HIGH]` | Lifecycle | [`segment_specs.md:705-781`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L705-L781) | Absence of `tokio_util::sync::CancellationToken` for cooperative task cancellation. | Orphaned file handles and uncleaned temp files when user cancels transfer. |
| `SEC4-HIGH-04` | `[HIGH]` | OS Adversity | [`docs/ARCHITECTURE.md:79`](file:///d:/WebDev/rust/Qshare/docs/ARCHITECTURE.md#L79) | Windows Defender file locks cause `ERROR_SHARING_VIOLATION` during atomic rename. | 100% completed transfers fail catastrophically at the final rename step. |
| `SEC4-MED-01` | `[MEDIUM]` | Filesystem | [`segment_specs.md:797`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L797) | Unchecked destination file overwrites without collision avoidance (`file (1).ext`). | Silent destruction of preexisting user files. |
| `SEC4-MED-02` | `[MEDIUM]` | Storage | [`segment_specs.md:783`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L783) | Missing `sync_all()` system call prior to dropping file handle and renaming. | Risk of uncommitted dirty pages and zero-byte files on sudden OS crash. |
| `SEC4-MED-03` | `[MEDIUM]` | Performance | [`segment_specs.md:724-780`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L724-L780) | Unthrottled progress updates emit at 1000+ msgs/sec under high-throughput conditions. | UI subscription thread saturation and unnecessary CPU wakeups. |
| `SEC4-LOW-01` | `[LOW]` | Throughput | [`segment_specs.md:706-716`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L706-L716) | Lockstep read-write loop prevents overlapping disk I/O and network transfer. | Throughput limited to ~50% of available pipeline capacity. |

---

### 8.2 Architectural Prescription (Actionable Blueprint)

#### Immediate Remediations Prior to Implementation:
1. **Declare Missing Crates in [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml):**
   Add `thiserror = "2.0"`, `tokio-util = { version = "0.7", features = ["sync"] }`, and `fs2 = "0.4"` (or `sysinfo = "0.33"`). Add `uuid = { version = "1.11", features = ["v4"] }` to `[dev-dependencies]`.
2. **Fix Partial File Lifecycle (`SEC4-CRIT-01`):**
   In `receive_stream`, record `transferred` bytes. If stream terminates with `transferred < total_bytes`, return `TransferError::IncompleteStream { transferred, total_bytes }` and **preserve** the `.part` file on disk for future resumption. Only delete `.part` if `transferred == total_bytes` and BLAKE3 hash fails.
3. **Correct Path Extension Pattern (`SEC4-CRIT-02`):**
   Replace `destination_path.with_extension(PART_EXTENSION)` with direct filename appending:
   ```rust
   let mut part_os = destination_path.file_name().unwrap_or_default().to_os_string();
   part_os.push(".qshare.part");
   let part_path = destination_path.with_file_name(part_os);
   ```
4. **Implement Windows Antivirus Rename Retry Loop (`SEC4-HIGH-04`):**
   Wrap `tokio::fs::rename` in an asynchronous retry loop with backoff:
   ```rust
   let mut retries = 5;
   while let Err(e) = tokio::fs::rename(&part_path, destination_path).await {
       if retries == 0 || e.raw_os_error() != Some(32) /* ERROR_SHARING_VIOLATION */ {
           return Err(TransferError::AtomicRenameFailed(e.to_string()));
       }
       retries -= 1;
       tokio::time::sleep(std::time::Duration::from_millis(100)).await;
   }
   ```
5. **Add Mandatory `sync_all()` Barrier (`SEC4-MED-02`):**
   Before dropping `part_file`, invoke `part_file.sync_all().await.map_err(...)` to guarantee all bytes and directory metadata are written to physical media.
6. **Integrate Exponential Moving Average (EMA) Speed Calculation (`SEC4-HIGH-02`):**
   Calculate speed over a rolling 1.0-second window or apply an EMA filter (`alpha = 0.2`) to prevent laggy telemetry and erratic ETA jumps.
7. **Throttle Progress Telemetry (`SEC4-MED-03`):**
   Limit `progress_tx.try_send` dispatches to a maximum frequency of once every 50 ms (20 Hz), ensuring high-speed transfers do not saturate the Elm update queue.

#### Recommended Conventional Commit Message:
```text
refactor(core): harden streaming pipeline, atomic rename and error recovery

- Prevent destructive deletion of partial files on unexpected stream termination
- Append .qshare.part to full filenames to prevent extension stripping and collisions
- Add retry backoff for Windows Defender sharing violations during atomic rename
- Call sync_all() prior to rename to ensure physical disk persistence
- Add thiserror and tokio-util dependencies to Cargo.toml
- Throttle progress channel dispatches to 20 Hz with EMA throughput calculation
```
