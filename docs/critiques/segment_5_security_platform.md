# Architecture Critique: Segment 5 — security-resiliency-packaging

**Document Version:** 1.0.0  
**Target:** Qshare Desktop Application (`d:\WebDev\rust\Qshare`)  
**Status:** Under Architectural Review  
**Evaluators:** Critic 1 (Investigator) & Critic 2 (Adversarial Doubt Gate)  
**Date:** 2026-10-30  

---

## 1. Executive Summary & Health Assessment

- **Segment Health Score:** 42/100
- **Summary:**  
  Segment 5 (`security-resiliency-packaging`) forms the defensive perimeter, platform crash-trapping architecture, and release packaging pipeline for Qshare. Its core mission is to protect the user's host operating system against untrusted remote network inputs (path traversal attacks, device name corruption, arbitrary file overwrites), capture runtime panics under the invisible Windows GUI subsystem (`#![windows_subsystem = "windows"]`), and produce a secure, high-DPI-aware, standalone portable Windows executable.

  While the architectural specifications in [`docs/ARCHITECTURE.md`](file:///d:/WebDev/rust/Qshare/docs/ARCHITECTURE.md) and `segment_specs.md` correctly recognize key threats such as Alternate Data Streams and basic directory traversal, a rigorous adversarial audit reveals that the proposed implementation is fundamentally compromised across multiple critical vectors. Most severely, the proposed `PathSanitizer` contains trivial bypasses for Windows reserved DOS device names via trailing spaces, trailing dots, and nested extensions (e.g. `CON.txt.`, `CON .txt`, `con.tar.gz`), which hang Win32 file APIs or redirect stream bytes to the console. Furthermore, path traversal attacks (`../../evil.exe`) are silently normalized and saved into the user's Downloads folder as executables rather than being rejected with a security alarm; destination collision avoidance suffers from a critical Time-of-Check to Time-of-Use (TOCTOU) race condition that is completely blind to active `.part` temporary files; the headless TDD test blueprint fails compilation due to an undeclared crate dependency (`uuid`); and the crash reporter writes to a relative path (`qshare_crash.log`), causing silent diagnostic loss when launched from read-only directories like `C:\Program Files\Qshare\`.

  Adversarial cross-examination also uncovered severe operational hazards: invoking synchronous modal message dialogs inside the panic hook deadlocks headless CI runners on GitHub Actions and freezes Tokio worker threads; `panic = "abort"` with `strip = true` strips PE symbols, rendering crash reports unresolvable; atomic file renaming fails completely when staging files across different filesystem volumes (`ERROR_NOT_SAME_DEVICE`); and the total omission of an embedded Windows application manifest causes severe UI blurriness on 4K multi-monitor setups and enforces legacy 260-character `MAX_PATH` limits.

- **Invariant Compliance Status:**
  - *Zero-Cloud Storage Invariant (0 MB Relay Disk):* **COMPLIANT**. The perimeter operates strictly locally on sender and receiver endpoints, persisting zero telemetry or file data to third-party infrastructure.
  - *Non-Blocking UI Thread Invariant:* **DRIFT DETECTED / AT RISK**. Invoking `rfd::MessageDialog::show()` synchronously inside the global panic hook halts the calling thread. When invoked from Tokio background worker threads or the main GUI thread, it freezes event processing and drops incoming network keepalives.
  - *Atomic Disk Finalization Invariant:* **DRIFT DETECTED / AT RISK**. The collision avoidance algorithm fails to check for active `.part` streams and relies on non-atomic `exists()` checks. Furthermore, atomic `std::fs::rename` fails if temporary staging and download destinations cross physical volume boundaries.
  - *Rust Memory Safety & Pure Native Binary:* **AT RISK**. While code is free of `unsafe` blocks, the segment suffers from compilation blockers (`uuid`), silent panic diagnostic loss, unhandled Win32 `ERROR_INVALID_NAME` (os error 123) crashes, and lack of Windows Per-Monitor V2 DPI manifest declarations.

---

## 2. Desktop UX & Visual Craft Audit (`/impeccable`)

### 2.1 Visual Hierarchy & Layout Architecture
1. **DPI Scaling Degradation on High-Resolution Displays:**  
   The binary currently specifies `#![windows_subsystem = "windows"]` in [`src/main.rs:1`](file:///d:/WebDev/rust/Qshare/src/main.rs#L1) but embeds no application manifest (`.manifest`). On modern Windows 10/11 systems with 4K displays or multi-monitor configurations using mixed scaling (e.g. 100% primary, 150% laptop screen), the Windows Desktop Window Manager (DWM) flags the application as legacy DPI-unaware. DWM bitmap-stretches the entire window surface, causing fonts, icons, borders, and `wgpu` canvas elements to render visibly soft, blurry, and degraded.
2. **Common Controls v6 Degradation:**  
   Without an embedded manifest requesting `Microsoft.Windows.Common-Controls` version `6.0.0.0`, native dialogs spawned via `rfd` (such as folder pickers and crash message boxes) default to classic Windows 2000 flat rendering without modern visual styles, rounded corners, or theme integration.
3. **Application Icon & Shell Presentation:**  
   The project contains no Windows Resource file (`.rc`). The compiled binary exhibits the generic blank executable icon in Windows File Explorer, Task Manager, Alt+Tab switcher, and the taskbar, detracting from product craftsmanship.

### 2.2 Component State Matrix

| Component / Visual State | Default | Hover / Focus | Active / Loading | Disabled | Empty State | Error State |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Security Warning Toast** | **MISSING** | N/A | N/A | N/A | N/A | **MISSING** (No visual toast for rejected files) |
| **Crash Message Dialog** | OK (Win32 Modal) | Stock OS | N/A | N/A | N/A | OK (Synchronous modal box) |
| **Path Collision Auto-Rename Badge** | **MISSING** | **MISSING** | N/A | N/A | N/A | **MISSING** (User not notified of `(1)` rename) |
| **Hash Verification Failure Modal** | **MISSING** | **MISSING** | **MISSING** | N/A | N/A | **MISSING** (Corrupted transfers fail silently) |
| **High-DPI Render Surface** | Blurry (DWM) | Blurry | Blurry | Blurry | Blurry | Blurry |

### 2.3 Microcopy, Telemetry & User Feedback

1. **Cryptic Developer Jargon in Security Errors:**  
   The error strings defined in `SecurityError` (lines 1048–1060 of `segment_specs.md`) bubble directly into user-facing UI toast notifications:
   - `"Windows Alternate Data Stream (ADS) colon rejected"`: Technical NTFS file-system terminology ("Alternate Data Stream", "ADS") is meaningless to non-technical users.
   - `"Path traversal attempt detected: {0}"`: Alarmist security terminology that provokes panic without offering actionable guidance.
   - **Remediation:** Translate technical error conditions into clear, actionable microcopy:
     - *ADS Error:* `"Transfer declined: The file name contains an invalid character (:). Please ask the sender to rename the file."`
     - *Traversal Error:* `"Transfer declined: The incoming file path contains unauthorized directory navigation."`
2. **Unsanitized Untrusted Input Reflection Hazard:**  
   `SecurityError::PathTraversalAttempt(String)` and `SecurityError::InvalidCharacters(String)` format raw peer-supplied input directly into error strings. If an attacker sends ANSI escape sequences (`\x1b[2J`), Right-to-Left Override Unicode characters (`\u{202E}`), or multi-megabyte strings, displaying the error in an Iced text widget will cause visual layout corruption or log injection.
3. **Silent Auto-Renaming Microcopy Deficit:**  
   When a collision occurs (e.g. `document.pdf` already exists in Downloads), `PathSanitizer::jail_destination` silently renames the incoming file to `document (1).pdf`. The UI provides zero feedback informing the recipient that the file was renamed, causing users to believe the wrong file was received or that the transfer failed to save.

---

## 3. 5-Layer Behavioral Flow Trace (`/behavioral-audit`)

```
Layer 1: Entry Trigger       [ Untrusted File Offer Metadata / OS Runtime Panic ]
         │
Layer 2: UI Event Dispatch   [ Segment 1 / Segment 4 Interception Guard ]
         │
Layer 3: Async Channel       [ PathSanitizer Validation & Destination Jailing ]
         │
Layer 4: Tokio Background    [ File Creation / Chunk Streaming / BLAKE3 Hash ]
         │
Layer 5: State Sync & UI     [ Atomic Rename / Error Toast / Crash Log Trapper ]
```

### 3.1 Flow Trace Analysis

#### Primary Happy Path (Incoming File Offer & Jailing)
- **Layer 1 (Entry):** Remote peer transmits metadata offer: `{"file_name": "quarterly_report.pdf", "size": 1048576, "hash": "..."}`.
- **Layer 2 (Dispatch):** Segment 4 receives the offer and invokes `PathSanitizer::jail_destination(downloads_dir, raw_name)`.
- **Layer 3 (Sanitization & Jailing):** `PathSanitizer::sanitize_filename` strips separators, verifies no illegal characters or reserved names exist, and returns `"quarterly_report.pdf"`. `jail_destination` checks if target exists and returns `D:\Downloads\quarterly_report.pdf`.
- **Layer 4 (Tokio Stream):** Segment 4 opens temporary stream `D:\Downloads\quarterly_report.pdf.qshare.part`, streams bounded chunks, and validates incremental BLAKE3 hashes.
- **Layer 5 (State Sync):** Upon stream EOF and 100% hash verification, the `.part` file is atomically renamed to `D:\Downloads\quarterly_report.pdf`, and Segment 1 emits a completion notification.

#### Adversarial Attack Vector Traces

##### Vector A: Path Traversal Exploit (`../../../../Windows/System32/evil.exe`)
- **Layer 1 (Entry):** Attacker sends filename `"../../../../Windows/System32/evil.exe"`.
- **Layer 2 (Interception):** Segment 4 invokes `PathSanitizer::sanitize_filename("../../Windows/System32/evil.exe")`.
- **Layer 3 (Defensive Breakdown):** In `segment_specs.md:980-982`:
  ```rust
  let path = Path::new(raw_name);
  let clean_name = path.file_name().and_then(|n| n.to_str()).ok_or(...)?;
  ```
  `Path::file_name()` extracts `"evil.exe"`. `sanitize_filename` returns `Ok("evil.exe")`!
- **Layer 4 (Unintended Execution):** Segment 4 creates `D:\Downloads\evil.exe.qshare.part` and completes the transfer.
- **Layer 5 (Failure Mode):** The attack payload is successfully written into the user's Downloads directory as an executable. `SecurityError::PathTraversalAttempt` is never triggered, the transfer is not halted, and the user receives no security warning.

##### Vector B: Reserved DOS Device Name Freeze (`CON.txt.`)
- **Layer 1 (Entry):** Attacker sends `"CON.txt."` or `"con.tar.gz"`.
- **Layer 2 (Interception):** `PathSanitizer::sanitize_filename` executes lines 985–995.
- **Layer 3 (Bypass):** For `"CON.txt."`, `Path::file_stem()` produces `"CON.txt"`. Uppercased: `"CON.TXT"`. This string is NOT in `WINDOWS_RESERVED_NAMES` (which only contains `"CON"`). Line 995 trims the trailing dot *after* the check, returning `Ok("CON.txt")`.
- **Layer 4 (OS Crash/Freeze):** Segment 4 invokes `tokio::fs::File::create("CON.txt.qshare.part")` or final rename to `"CON.txt"`. Windows Win32 redirects the stream to the physical console, hanging the thread or corrupting the filesystem stream.

#### Unwired CTAs & Dead Ends
1. **Security Alert Toast Dispatch:** `SecurityError` variants are defined in `src/core/security.rs`, but no message bridge exists to route `SecurityError` into `Message::ToastNotification` in Segment 1. Rejected transfers drop silently without user remediation.
2. **Crash Log Folder Opener:** When `qshare_crash.log` is written, the dialog provides no button or link to open the containing folder in Windows Explorer.

#### Mock Leaks & Stubs
- The specification defines `WINDOWS_RESERVED_NAMES` but omits `CONIN$`, `CONOUT$`, and `CLOCK$`.
- The collision avoidance loop in `jail_destination` increments a local counter without testing whether the candidate name is already locked by another process.

#### Error Swallowing & Silent Failures
1. **Relative Crash Log Discard in Panic Hook:**  
   In [`src/main.rs:10`](file:///d:/WebDev/rust/Qshare/src/main.rs#L10) and `segment_specs.md:1038`:
   ```rust
   let _ = std::fs::write("qshare_crash.log", &msg);
   ```
   If Qshare is launched from a read-only directory (such as `C:\Program Files\Qshare` or via a desktop shortcut with working directory `C:\Windows\System32`), `std::fs::write` fails with `AccessDenied` (os error 5). The error is discarded via `let _ =`, leaving no diagnostic file on disk.
2. **Launch Error Log Discard:**  
   In [`src/main.rs:35`](file:///d:/WebDev/rust/Qshare/src/main.rs#L35), window launch failures similarly discard disk write errors with `let _ = std::fs::write(...)`.

#### UI Thread & Tokio Worker Thread Blocking Hazards
1. **Headless CI Deadlock on `rfd::MessageDialog::show()`:**  
   In automated testing environments (GitHub Actions Windows runner), invoking `rfd::MessageDialog::show()` spawns a modal Win32 message box. In a headless CI environment, there is no human user to click "OK". The runner hangs indefinitely until terminated by the 6-hour runner timeout.
2. **Tokio Worker Pool Freezing on Background Task Panic:**  
   If an unexpected panic occurs inside a spawned Tokio background task (e.g. during network I/O or hashing), the global panic hook runs on that Tokio worker thread. Calling `rfd::MessageDialog::show()` blocks that worker thread synchronously. If multiple tasks panic, all worker threads in the Tokio threadpool are frozen, completely halting the application engine while the window remains open.

---

## 4. Headless Testability & TDD Strategy (`/test-driven-development`)

### 4.1 Decoupling & Testability Assessment
1. **Undeclared Dependency Compilation Blocker (`uuid`):**  
   The headless TDD blueprint in `segment_specs.md:1122` executes:
   ```rust
   let temp_dir = std::env::temp_dir().join(format!("qshare_test_jail_{}", uuid::Uuid::new_v4()));
   ```
   Neither `uuid` nor `uuid::Uuid` is declared in [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml). Attempting to run `cargo test` on this suite produces an immediate compilation failure (`error[E0433]: failed to resolve: use of undeclared crate or module uuid`), completely blocking automated quality gates.
2. **Headless Safety in Panic Tests:**  
   Testing panic hook behavior must never trigger interactive Win32 dialogs. The crash reporter must check an environment flag (`CI` or `QSHARE_HEADLESS`) before invoking GUI dialogs.

### 4.2 Required Headless Test Matrix

| Test Case Identifier | Target Behavior | Failure Injection / Stimulus | Expected Assertion |
| :--- | :--- | :--- | :--- |
| `TEST-5-01` | Directory Traversal Rejection | Propose `../../Windows/System32/calc.exe` and `..\\..\\secret.key`. | Returns `Err(SecurityError::PathTraversalAttempt)`. Must NOT sanitize into Downloads. |
| `TEST-5-02` | Reserved Device Name Protection | Test `CON`, `prn.txt`, `aux.png`, `NUL`, `CONIN$.txt`, `con.tar.gz`, `CON.txt.`, `CON .txt`. | All variants prefixed with `qshare_` (e.g. `qshare_CON.txt`) or rejected. |
| `TEST-5-03` | Alternate Data Streams Rejection | Test `payload.txt:hidden.exe` and `doc.pdf::$DATA`. | Returns `Err(SecurityError::AlternateDataStreamDetected)`. |
| `TEST-5-04` | Illegal Win32 Characters Rejection | Test `report<final>?.pdf`, `file|pipe.txt`, `test*.doc`, `name"quote".txt`. | Returns `Err(SecurityError::InvalidCharacters)`. |
| `TEST-5-05` | Control Characters & RTLO Rejection | Inject ASCII control byte `0x1F` and RTLO override `\u{202E}` (`test\u{202E}txt.exe`). | Returns `Err(SecurityError::InvalidCharacters)`. |
| `TEST-5-06` | Collision Avoidance Auto-Increment | Create existing `photo.jpg` in target directory; jail incoming `photo.jpg`. | Returns target path ending in `photo (1).jpg`. |
| `TEST-5-07` | Collision Avoidance `.part` Awareness | Create existing `video.mp4.qshare.part` in target directory; jail incoming `video.mp4`. | Returns target path ending in `video (1).mp4` (avoids colliding with active transfer). |
| `TEST-5-08` | Canonical Path Jail Containment | Propose path resolving outside download directory via symlink or junction. | Assertion verifies `canonicalize(target).starts_with(canonicalize(download_dir))`. |
| `TEST-5-09` | Robust Crash Log Path Resolution | Simulate read-only CWD; invoke crash log writer. | Log successfully created in `%LOCALAPPDATA%\Qshare\logs\crash.log`; returns `Ok(())`. |
| `TEST-5-10` | Headless CI Dialog Bypass | Set `env::set_var("CI", "true")`; trigger panic hook. | Stack trace written to log file; GUI dialog call bypassed with zero blocking. |

---

## 5. Official Source Grounding & API Compliance (`/source-driven-development`)

### 5.1 Win32 Filesystem API & Reserved Device Names Rules
According to official Microsoft Learn documentation (*Naming Files, Paths, and Namespaces*):
1. **Reserved Device Names:**  
   The following reserved names cannot be used as the name of a file in Win32:
   `CON`, `PRN`, `AUX`, `NUL`, `COM1` through `COM9`, `LPT1` through `LPT9`, `CONIN$`, `CONOUT$`, and `CLOCK$`.
   Microsoft explicitly mandates:  
   > *"Also avoid these names followed immediately by an extension; for example, NUL.txt, CON.cfg, or COM1.tar.gz are not valid."*
2. **Trailing Dots and Spaces:**  
   > *"Do not end a file or directory name with a space or a period. Although the underlying file system may support such names, the Windows shell and user interface do not."*  
   Windows Win32 path normalization automatically trims trailing spaces and periods before performing device resolution. Sanitizing must trim trailing periods and spaces *before* evaluating device name stems.
3. **Illegal Characters:**  
   The characters `<`, `>`, `:`, `"`, `/`, `\`, `|`, `?`, `*`, and characters with integer values in the range `0` through `31` (`0x00`–`0x1F`) are strictly reserved and prohibited in Win32 filenames.

### 5.2 Windows Application Manifest & DPI Declarations
According to official Microsoft Windows Desktop Application documentation (*Developing a High DPI Desktop Application*):
1. **Per-Monitor V2 Awareness:**  
   Desktop applications drawing via DirectX 12 / Vulkan (`wgpu`) must declare `<dpiAwareness>PerMonitorV2, unaware</dpiAwareness>` in an embedded application manifest. Without this, Windows DWM forces legacy system DPI virtualization, causing blurry bitmap scaling.
2. **Long Path Awareness (`<longPathAware>`):**  
   Win32 historically restricts file paths to 260 characters (`MAX_PATH`). On Windows 10 version 1607 and later, declaring `<longPathAware>true</longPathAware>` in the manifest lifts the 260-character limit, preventing file creation errors when downloading files into deep directory structures.
3. **Common Controls v6:**  
   Embedding the `Microsoft.Windows.Common-Controls` dependency ensures native Win32 dialogs use modern themed visual styles.

### 5.3 Rust 2024 Release Profile & Symbol Post-Mortems
In [`Cargo.toml:40-45`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L40-L45):
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```
1. **`panic = "abort"` Behavior:**  
   When `panic = "abort"` is set, stack unwinding is completely disabled. Rust executes the `std::panic::set_hook` callback, but the process aborts immediately after. No RAII destructors (`Drop`) are run.
2. **`strip = true` Debuggability Impact:**  
   Setting `strip = true` discards the PE symbol table and debug information. When a crash occurs, capturing `std::backtrace::Backtrace::force_capture()` outputs raw hexadecimal addresses without function names or source line numbers. To retain production debuggability, release workflows must either split debuginfo (`split-debuginfo = "packed"`) to produce a separate `.pdb` symbol archive, or publish the PDB alongside release binaries.

---

## 6. Adversarial Doubt Gate: Real-World Field Failures (`/doubt-driven-development`)

> *"Under what real-world field conditions will this clean-looking code fail?"*

### 6.1 Field Failure Cross-Examination

#### 1. Remote Malicious Peer Exploit Injections
- **Field Scenario:** An attacker on the internet connects via a Share Ticket and proposes a payload designed to exploit Win32 filename handling:
  - Trailing Dot Bypass: `CON.txt.`
  - Nested Extension Bypass: `con.tar.gz`
  - Trailing Space Bypass: `CON .txt`
  - Alternate Data Stream: `document.pdf:secret.exe`
  - Right-to-Left Override: `invoice\u{202E}gnp.exe` (which displays as `invoiceexe.png`)
  - UNC Path NTLM Leak: `\\192.168.1.50\share\payload.exe`
- **Failure Mode in Segment 5:**  
  Because `PathSanitizer` in `segment_specs.md` checks `file_stem()` before stripping trailing dots/spaces, all reserved device bypasses succeed. For UNC paths, Windows attempts an SMB outbound negotiation, leaking the user's NTLMv2 password hash to the attacker's server. For RTLO characters, the user is deceived into executing an `.exe` disguised as an image.
- **Architectural Defense:**  
  1. Strip all leading separators, UNC prefixes (`\\`, `//`), and drive letters (`C:`).
  2. Reject any filename containing path separators (`/`, `\`) or `..` with `Err(SecurityError::PathTraversalAttempt)`.
  3. Strip all trailing dots and spaces first.
  4. Extract the base stem before the *first* dot (`name.split('.').next()`) and compare against the complete reserved name set (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`, `CONIN$`, `CONOUT$`, `CLOCK$`).
  5. Reject characters `<`, `>`, `:`, `"`, `|`, `?`, `*`, ASCII `0x00..=0x1F`, and Unicode control codes (`0x200B..=0x200F`, `0x202A..=0x202E`).

#### 2. File Collision & TOCTOU Race Hazards
- **Field Scenario:**  
  1. Two remote peers concurrently transfer files named `presentation.pptx`.
  2. Or, a user transfers a 20 GB file named `backup.zip`. Ten minutes into the transfer, the user saves a new file named `backup.zip` into Downloads.
- **Failure Mode in Segment 5:**  
  `jail_destination` only checks `target_path.exists()` at the beginning of the transfer. It does NOT check whether an active `.part` file exists (`presentation.pptx.qshare.part`). Both concurrent transfers receive `target_path.exists() == false`, pick the exact same destination, and write to the same `.part` file, corrupting data and failing BLAKE3 checks. For the TOCTOU scenario, when the 20 GB transfer finishes, calling `std::fs::rename` either overwrites the user's new document without warning or fails with an unhandled Win32 error.
- **Architectural Defense:**  
  Collision avoidance must check for the existence of BOTH the final destination (`target_path`) AND active temporary files (`<candidate>.qshare.part`). At stream completion, the rename must use atomic creation semantics or verify that the destination was not modified during transit.

#### 3. Standard User Account & Read-Only Installation Directory
- **Field Scenario:**  
  Qshare is packaged as a standard installer in `C:\Program Files\Qshare\qshare.exe`. A standard corporate user (without Administrator privileges) launches the application from the Start Menu. An unexpected panic occurs.
- **Failure Mode in Segment 5:**  
  The panic hook invokes `std::fs::write("qshare_crash.log", &msg)`. The current working directory is `C:\Program Files\Qshare\`. The OS returns `AccessDenied` (os error 5). The write fails silently (`let _ =`). No crash report is created, leaving IT support with zero diagnostics.
- **Architectural Defense:**  
  Crash logs must NEVER be written to a relative path. Qshare must resolve the canonical local application data folder:
  `%LOCALAPPDATA%\Qshare\logs\crash.log` (using `dirs::data_local_dir()`). The logger must create parent directories if missing and append timestamps to preserve historical crashes.

#### 4. 4K High-DPI & Multi-Monitor Display Degradation
- **Field Scenario:**  
  A user runs Qshare on a 4K monitor scaled to 150% alongside a secondary 1080p monitor at 100%. The user drags the Qshare window from the 4K display to the 1080p display.
- **Failure Mode in Segment 5:**  
  Because no Windows application manifest is embedded, Windows applies legacy DWM bitmap scaling. Text is blurry, font rendering is fuzzy, and when dragged across monitors, the window fails to re-scale dynamically, resulting in oversized or pixelated UI widgets.
- **Architectural Defense:**  
  Embed a production `qshare.exe.manifest` via `build.rs` declaring `PerMonitorV2` DPI awareness, enabling crisp native GPU rendering across all monitor resolutions and scaling transitions.

#### 5. Headless CI Runner Deadlock
- **Field Scenario:**  
  A developer pushes a commit or opens a pull request. The GitHub Actions CI workflow runs `cargo test`. A test triggers an intentional panic in a test harness or an unexpected error occurs.
- **Failure Mode in Segment 5:**  
  `CrashReporter::install_panic_hook` invokes `rfd::MessageDialog::new().show()`. On the headless Windows GitHub Actions runner, the modal Win32 dialog opens and waits for a user click. The runner thread blocks indefinitely, consuming compute minutes until the 6-hour runner timeout kills the job.
- **Architectural Defense:**  
  The crash hook must inspect `std::env::var("CI")`. If running in CI or headless mode, it must write to stderr/disk and exit immediately without invoking `rfd::MessageDialog`.

#### 6. Cross-Volume Mounts & Atomic Rename Failures (`ERROR_NOT_SAME_DEVICE`)
- **Field Scenario:**  
  The user configures Qshare's download folder to an external hard drive or secondary volume (`E:\Downloads`), but the application or OS creates temporary `.part` files in `C:\Users\User\AppData\Local\Temp\`.
- **Failure Mode in Segment 5:**  
  When the transfer completes, Segment 4 calls `std::fs::rename(temp_part_path, destination_path)`. Under Windows, `MoveFileExW` fails with `ERROR_NOT_SAME_DEVICE` (os error 17) because atomic rename cannot cross physical filesystem volumes. The transfer aborts at 100% completion.
- **Architectural Defense:**  
  `PathSanitizer::jail_destination` and Segment 4 must guarantee that `<filename>.qshare.part` is ALWAYS created in the exact same parent directory on the same filesystem volume as the final target file.

---

## 7. Cross-Segment Cohesion & Ripple Impact Analysis

| Interacting Segment | Interface Seam | Ripple Risk / Failure Mode | Cohesion Rating |
| :--- | :--- | :--- | :---: |
| **Segment 1: `desktop-ui-shell`** | `SecurityError` $\rightarrow$ Toast Notification | Security errors have no channel mapping into UI; rejected files fail silently without alerting the user. | **VULNERABLE** |
| **Segment 1: `desktop-ui-shell`** | Global Panic Hook $\rightarrow$ GUI Event Loop | Modal crash dialogs block the winit/wgpu UI thread synchronously, dropping keepalives. | **VULNERABLE** |
| **Segment 2: `lan-mdns-radar`** | Firewall Guidance $\rightarrow$ UDP Port 5353 | Silent Windows Defender UDP drops (KI-001) disable mDNS without alerting user to grant firewall access. | **VULNERABLE** |
| **Segment 3: `wan-iroh-transport`** | Network Timeout Heuristic (KI-001) | Stated 3-second UDP hole-punch timeout heuristic is missing from Segment 5 specifications. | **BROKEN** |
| **Segment 4: `streaming-disk-io`** | `PathSanitizer::jail_destination` | TOCTOU collision race corrupts `.part` files; cross-volume renames fail with `ERROR_NOT_SAME_DEVICE`. | **BROKEN** |

### 7.1 Security Abort Signaling & Remote Peer State Desync
When `PathSanitizer` rejects an incoming file offer (e.g. `SecurityError::PathTraversalAttempt`), the rejection happens locally on the receiver:
1. **The Desync Vulnerability:** If the receiver's engine abruptly terminates the session without transmitting a formal rejection frame over the QUIC control stream, the remote sender continues pumping multi-gigabyte chunk data into the QUIC stream until it hits a write error or buffer timeout.
2. **Resource Exhaustion:** This wastes network bandwidth and leaves the sender's UI stuck in `Streaming` indefinitely.
3. **Prescription:** When `PathSanitizer` returns `Err(SecurityError)`, Segment 4 must transmit a typed `ControlFrame::OfferDeclined(SecurityError)` before closing the stream, allowing the remote sender to immediately display: *"Transfer declined by recipient: Invalid file name"*.

---

## 8. Prioritized Findings & Architectural Recommendations

### 8.1 Findings Log

| Finding ID | Severity | Category | Affected File & Lines | Description | Impact |
| :--- | :---: | :---: | :--- | :--- | :--- |
| `SEC-01` | `[CRITICAL]` | Security / Win32 | `segment_specs.md:984-1000` | **Reserved Device Name Bypasses:** Trailing dots/spaces and nested extensions (`CON.txt.`, `CON .txt`, `con.tar.gz`) bypass device blacklist. | Win32 file APIs hang, redirect writes to console, or corrupt filesystem. |
| `SEC-02` | `[CRITICAL]` | TDD / Build | `segment_specs.md:1122`, [`Cargo.toml:11-39`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L11-L39) | **Undeclared `uuid` Dependency:** Test blueprint calls `uuid::Uuid::new_v4()` without declaring `uuid` in `Cargo.toml`. | Test compilation fails immediately (`error[E0433]: failed to resolve: use of undeclared crate uuid`). |
| `SEC-03` | `[HIGH]` | Security / Traversal | `segment_specs.md:980-982`, `L1056` | **Traversal Sanitized into Downloads:** `Path::file_name()` extracts leaf name from `../../evil.exe`, saving it into Downloads instead of rejecting. | Exploit payloads silently written to Downloads; security alerts bypassed. |
| `SEC-04` | `[HIGH]` | Platform / Win32 | `segment_specs.md:957-961` | **Incomplete Device Name Blacklist:** Array omits `CONIN$`, `CONOUT$`, and `CLOCK$`. | Inbound files create handles to internal console streams. |
| `SEC-05` | `[HIGH]` | Platform / Win32 | `segment_specs.md:967-976` | **Permitting Illegal Win32 Characters:** Characters `<`, `>`, `"`, `|`, `?`, `*`, and control codes `0x00..=0x1F` are not rejected. | Win32 file creation fails with `ERROR_INVALID_NAME` (os error 123). |
| `SEC-06` | `[HIGH]` | Concurrency / Data | `segment_specs.md:1009-1026` | **TOCTOU & `.part` Blindness in Collision:** `jail_destination` ignores active `.part` streams and uses non-atomic `exists()` checks. | Concurrent transfers corrupt each other's `.part` files; race overwrites user files. |
| `SEC-07` | `[HIGH]` | Diagnostics | [`src/main.rs:10`](file:///d:/WebDev/rust/Qshare/src/main.rs#L10), `L35`; `segment_specs.md:1038` | **Relative Crash Log Path:** Writing to `qshare_crash.log` in CWD fails with `AccessDenied` in read-only installation folders. | Crash reports silently lost in standard Windows installations. |
| `SEC-08` | `[HIGH]` | Debuggability | [`Cargo.toml:40-45`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L40-L45); `segment_specs.md:1037` | **Backtrace Omission & Stripping:** Panic hook omits `Backtrace::force_capture()`; `strip = true` discards PE symbols. | Crash logs contain no call stacks or only unresolvable hex addresses. |
| `SEC-09` | `[HIGH]` | Platform / Manifest | Project Root, [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml) | **Missing Windows Application Manifest:** Total lack of embedded manifest declaring Per-Monitor V2 DPI awareness and long paths. | Blurry UI on 4K displays; legacy 260-char `MAX_PATH` file open failures. |
| `SEC-10` | `[HIGH]` | Concurrency / CI | `segment_specs.md:1039-1044` | **Modal Dialog Freezes CI Runners & Tokio:** Synchronous `MessageDialog::show()` in panic hook deadlocks CI and blocks Tokio workers. | Headless CI jobs hang for 6 hours; Tokio background tasks freeze. |
| `SEC-11` | `[HIGH]` | Filesystem / Atomic | `segment_specs.md:1004-1029` | **Cross-Volume Rename Failure:** Staging `.part` files on a different volume than `download_dir` causes `rename` to fail with `ERROR_NOT_SAME_DEVICE`. | Multi-gigabyte transfers fail at 100% completion during final rename. |
| `SEC-12` | `[MEDIUM]` | Impeccable / Copy | `segment_specs.md:1048-1060` | **Cryptic Microcopy Jargon:** Error strings expose internal NTFS jargon ("Alternate Data Stream (ADS)") to end users. | Poor user experience; confusing and non-actionable error messages. |
| `SEC-13` | `[MEDIUM]` | Security / UI | `segment_specs.md:1057`, `L1059` | **Unsanitized Input Reflection:** Untrusted filenames formatted directly into error strings without stripping control/RTLO codes. | UI layout distortion and log poisoning vulnerabilities. |
| `SEC-14` | `[MEDIUM]` | CI/CD / Release | [`.github/workflows/release.yml:67-84`](file:///d:/WebDev/rust/Qshare/.github/workflows/release.yml#L67-L84) | **Missing Verification Gates & Checksums:** Release workflow builds without running `cargo test` and produces no SHA-256 checksums. | Defective binaries can be released; users cannot verify asset integrity. |
| `SEC-15` | `[MEDIUM]` | Specification Drift | `segment_specs.md:940-944` | **Omission of Network Resiliency Heuristics:** Stated UDP timeout (KI-001) and Wi-Fi isolation fallback (KI-002) missing from spec. | Specification claims capabilities that are completely unimplemented. |
| `SEC-16` | `[MEDIUM]` | Security / Network | `segment_specs.md:967-1001` | **UNC Path NTLM Hash Capture Hazard:** Leading `\\` or `//` in raw names not stripped, allowing outbound SMB connections. | Potential NTLMv2 credential theft if file paths trigger OS network resolution. |

---

### 8.2 Architectural Prescription (Actionable Blueprint)

#### Immediate Remediations (Pre-Implementation Mandate)

1. **Implement Hardened, Zero-Trust `PathSanitizer`:**  
   Replace the flawed sanitizer with a multi-stage validation pipeline:
   - Reject any filename containing path separators (`/`, `\`), `..`, or leading UNC slashes with `Err(SecurityError::PathTraversalAttempt)`.
   - Strip all trailing dots and spaces before device name inspection.
   - Extract the base stem before the first dot and verify against the complete reserved set: `CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`, `CONIN$`, `CONOUT$`, `CLOCK$`. Prefix matching names with `qshare_`.
   - Reject illegal Win32 characters (`<`, `>`, `:`, `"`, `|`, `?`, `*`), ASCII control codes (`0x00..=0x1F`), and Unicode directional overrides (`\u{202A}`–`\u{202E}`).
2. **Harden Collision Avoidance Against TOCTOU & `.part` Blindness:**  
   When generating destination paths, verify that neither `<candidate>` NOR `<candidate>.qshare.part` exists. Ensure that `.part` temporary files reside in the EXACT same download directory as the final destination file to guarantee atomic, single-volume renaming.
3. **Harden Crash Reporting & Local AppData Resolution:**  
   Resolve crash log destinations to `%LOCALAPPDATA%\Qshare\logs\crash.log` using `dirs::data_local_dir()`. Force backtrace capture via `std::backtrace::Backtrace::force_capture()`. Detect headless CI environments (`std::env::var("CI").is_ok()`) to bypass modal `rfd::MessageDialog` calls.
4. **Embed Production Windows Application Manifest (`build.rs`):**  
   Add a `build.rs` script using the `winres` crate to compile an application manifest declaring:
   - `<dpiAwareness>PerMonitorV2, unaware</dpiAwareness>` (eliminates 4K blurriness).
   - `<longPathAware>true</longPathAware>` (lifts 260-character path limit).
   - Common Controls v6 dependency (modern native dialog styling).
5. **Harden GitHub Actions Release Workflow:**  
   Add automated quality gates (`cargo test`, `cargo clippy -- -D warnings`) prior to binary creation, and generate a `checksums-sha256.txt` artifact using PowerShell `Get-FileHash`.

---

#### Drop-in Reference Architecture: `src/core/security.rs`

```rust
use std::path::{Path, PathBuf};

const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    "CONIN$", "CONOUT$", "CLOCK$",
];

const ILLEGAL_WIN32_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum SecurityError {
    #[error("Transfer declined: File name cannot be empty.")]
    EmptyFileName,
    #[error("Transfer declined: File name contains unauthorized path navigation.")]
    PathTraversalAttempt,
    #[error("Transfer declined: File name contains an invalid character.")]
    InvalidCharacters,
    #[error("Transfer declined: File name exceeds maximum allowed length of 255 characters.")]
    FileNameTooLong,
    #[error("Transfer declined: Destination directory could not be verified.")]
    InvalidDestinationDirectory,
}

pub struct PathSanitizer;

impl PathSanitizer {
    /// Sanitizes an untrusted filename received from a remote peer.
    /// Strictly rejects directory traversal, illegal Win32 characters, and ADS streams.
    pub fn sanitize_filename(raw_name: &str) -> Result<String, SecurityError> {
        let trimmed_raw = raw_name.trim();
        if trimmed_raw.is_empty() {
            return Err(SecurityError::EmptyFileName);
        }

        // Enforce maximum single-filename component length (Win32 255 chars)
        if trimmed_raw.len() > 255 {
            return Err(SecurityError::FileNameTooLong);
        }

        // 1. Strict Path Traversal & UNC Rejection (Do not normalize into Downloads)
        if raw_name.contains('/') || raw_name.contains('\\') || raw_name.contains("..") {
            return Err(SecurityError::PathTraversalAttempt);
        }

        // 2. Reject illegal Win32 characters, ADS colons, and control characters
        for ch in raw_name.chars() {
            if ILLEGAL_WIN32_CHARS.contains(&ch) {
                return Err(SecurityError::InvalidCharacters);
            }
            // Reject ASCII control codes (0x00..=0x1F) and RTLO/bidi overrides
            if (ch as u32) <= 0x1F || (ch as u32) == 0x7F || ('\u{202A}'..='\u{202E}').contains(&ch) {
                return Err(SecurityError::InvalidCharacters);
            }
        }

        // 3. Trim trailing dots and spaces (Windows Win32 trims these automatically)
        let clean_name = raw_name.trim_end_matches(&[' ', '.'][..]);
        if clean_name.is_empty() {
            return Err(SecurityError::InvalidCharacters);
        }

        // 4. Windows Reserved DOS Device Name Inspection
        // Extract base stem before the FIRST dot to prevent "CON.txt.", "con.tar.gz", "CON .txt"
        let base_stem = clean_name
            .split('.')
            .next()
            .unwrap_or(clean_name)
            .trim_end()
            .to_uppercase();

        if WINDOWS_RESERVED_NAMES.contains(&base_stem.as_str()) {
            return Ok(format!("qshare_{clean_name}"));
        }

        Ok(clean_name.to_string())
    }

    /// Jails the destination file inside the download directory with collision avoidance.
    /// Checks for both completed files and active in-progress `.part` streams.
    pub fn jail_destination(download_dir: &Path, raw_name: &str) -> Result<PathBuf, SecurityError> {
        let clean_name = Self::sanitize_filename(raw_name)?;
        let mut target_path = download_dir.join(&clean_name);

        // Auto-increment to prevent overwriting existing files or colliding with active .part streams
        let stem = target_path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = target_path.extension().and_then(|e| e.to_str());
        let mut counter = 1;

        while target_path.exists() || Self::part_file_exists(&target_path) {
            let candidate_name = match ext {
                Some(e) => format!("{stem} ({counter}).{e}"),
                None => format!("{stem} ({counter})"),
            };
            target_path = download_dir.join(candidate_name);
            counter += 1;
        }

        Ok(target_path)
    }

    fn part_file_exists(target: &Path) -> bool {
        let mut part = target.to_path_buf();
        let current_name = part.file_name().and_then(|n| n.to_str()).unwrap_or("");
        part.set_file_name(format!("{current_name}.qshare.part"));
        part.exists()
    }
}
```

---

#### Drop-in Reference Architecture: `src/platform/windows.rs`

```rust
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub struct CrashReporter;

impl CrashReporter {
    /// Installs a robust panic hook writing stack backtraces to LocalAppData.
    /// Automatically suppresses modal dialogs during headless CI runs to prevent test hangs.
    pub fn install_panic_hook() {
        std::panic::set_hook(Box::new(|info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let log_msg = format!(
                "====================================================\n\
                 TIMESTAMP: {timestamp}\n\
                 PANIC: {info}\n\n\
                 BACKTRACE:\n{backtrace}\n\
                 ====================================================\n\n"
            );

            // 1. Resolve canonical path: %LOCALAPPDATA%\Qshare\logs\crash.log
            if let Some(log_path) = Self::resolve_crash_log_path() {
                if let Some(parent) = log_path.parent() {
                    let _ = create_dir_all(parent);
                }
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = file.write_all(log_msg.as_bytes());
                }
            }

            // 2. If running under headless CI, write to stderr and exit without modal dialog
            let is_ci = std::env::var("CI").is_ok() || std::env::var("QSHARE_HEADLESS").is_ok();
            if is_ci {
                eprintln!("{log_msg}");
                return;
            }

            // 3. Show native Win32 message box only in interactive user mode
            let dialog_msg = format!(
                "Qshare encountered an unexpected error and needs to close.\n\n\
                 Crash details have been saved to your local application logs."
            );
            rfd::MessageDialog::new()
                .set_title("Qshare Unexpected Error")
                .set_description(&dialog_msg)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }));
    }

    pub fn resolve_crash_log_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join("Qshare").join("logs").join("crash.log"))
    }
}
```

---

#### Windows Application Manifest XML: `qshare.exe.manifest`

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity
    version="0.1.0.0"
    processorArchitecture="*"
    name="JabirAbdullahHaian.Qshare"
    type="win32"
  />
  <description>Qshare Zero-Cloud Hybrid P2P Desktop Dropzone</description>

  <!-- Enable Per-Monitor V2 High-DPI Awareness (Crisp 4K Rendering) -->
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, unaware</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
      <printerDriverIsolation xmlns="http://schemas.microsoft.com/SMI/2011/WindowsSettings">true</printerDriverIsolation>
    </windowsSettings>
  </application>

  <!-- Enable Modern Windows Visual Styles (Common Controls v6) -->
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>

  <!-- Windows OS Compatibility Declarations -->
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and Windows 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
```

---

#### Recommended Conventional Commit Message

```
refactor(security): harden path sanitization, local crash reporting and windows manifest

- Rebuild PathSanitizer to strictly reject traversal attempts and illegal Win32 characters
- Fix reserved device name bypasses on CON.txt., CON .txt, and nested extensions
- Implement active .part file collision avoidance in destination jailing
- Relocate crash logging to %LOCALAPPDATA%/Qshare/logs with forced backtraces
- Add headless CI check in crash reporter to prevent modal dialog deadlocks
- Add Windows application manifest with PerMonitorV2 DPI awareness and long paths
- Add test quality gates and SHA-256 checksum generation to release workflow
```
