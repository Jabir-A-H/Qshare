# Qshare Project Roadmap

## Phase 1: Environment & Toolchain Setup (Active)
- [x] Create public GitHub repository `Jabir-A-H/Qshare`.
- [x] Initialize Git configuration and `.gitignore`.
- [x] Establish master documentation suite (`AGENTS.md`, `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `KNOWN_ISSUES.md`, `ADR/`).
- [x] Install Rust toolchain (`rustup`, `cargo`, `rustc` MSVC) on the host system.
- [x] Initialize `Cargo.toml` workspace with foundational dependencies (`iced`, `iroh`, `tokio`, `blake3`, `serde`).

---

## Phase 2: Core Headless P2P Engine & Loopback Tests (Completed)
- [x] Scaffold `src/core/` module structure (`p2p.rs`, `ticket.rs`, `storage.rs`, `transfer.rs`, `security.rs`, `discovery.rs`, `lan.rs`).
- [x] Implement `Iroh 1.4.1` endpoint lifecycle:
  - [x] Ephemeral keypair generation (`EndpointId`).
  - [x] Canonical Base32 ticket encoding/decoding (`QshareTicket`) with strict SSRF defense.
  - [x] STUN address discovery, hole punching, and stateless blind relay fallback.
  - [x] Dynamic path migration telemetry qualification (`LocalLan`, `DirectP2P`, `BlindRelay`).
- [x] Implement BLAKE3 chunked file reader & writer (`.part` temporary file handling):
  - [x] Bounded chunk buffers (128 KB).
  - [x] Non-destructive partial file quarantine on premature stream termination (`SEC4-CRIT-01`).
  - [x] Physical disk barrier (`sync_all()`) and Windows antivirus file lock retry loop.
  - [x] Cross-volume rename copy fallback.
- [x] Write automated headless integration tests: loopback sender-receiver file transfer without UI (`tests/p2p_test.rs`, `tests/streaming_disk_io_test.rs`).

---

## Phase 3: Local Network Discovery Engine (LAN) (Completed)
- [x] Implement mDNS advertiser and listener (`simple-dns` on UDP `0.0.0.0:5353` with `SO_REUSEADDR` & ephemeral port fallback).
- [x] Broadcast local device hostname, OS avatar identifier (`OsType`), and active service port.
- [x] Implement bounded peer table state (`MAX_LAN_PEERS = 256`) with 3-tier TTL lifecycle (Active <8s, Stale 8-15s, Evicted >15s) and sender rate-limiting.
- [x] Implement RFC 6762 Goodbye packet ($\text{TTL} = 0$) detection for instant peer removal and automated broadcast on shutdown.
- [x] Write automated headless integration tests (`tests/lan_test.rs`).

---

## Phase 4: Iced Desktop GUI & Elm State Machine (Completed)
- [x] Configure `Iced 0.13` application shell with modern dark theme and explicit centered window geometry (`850 × 650`).
- [x] Build **Tab 1: LAN Radar View**:
  - [x] Interactive peer cards with device avatars, socket addresses, and stale status.
  - [x] Polar radar canvas program rendering peers at deterministic coordinates.
  - [x] Empty state radar pulse and Wi-Fi Client Isolation fallback prompt (`KI-002`).
  - [x] Scrollable container for high-density peer lists.
- [x] Build **Tab 2: Send Anywhere (Internet P2P) View**:
  - [x] Dropzone with cached non-blocking metadata inspection.
  - [x] One-time encrypted Share Ticket box with one-click copy and "Copied! ✓" visual toast.
  - [x] Ticket input field for receiver with focused border states and Base32 pre-validation.
- [x] Build **Transfer Drawer / Progress Modal**:
  - [x] Real-time progress bar, transferred byte gauge, and EMA-smoothed throughput gauge (MB/s).
  - [x] Countdown ETA calculation and connection route badge (`Local LAN` / `Direct P2P` / `Blind Relay`).
  - [x] Cancel transfer and separate modal dismiss actions.
  - [x] Bidirectional Unicode (BiDi) and path traversal sanitization on remote filenames.
- [x] Wire non-blocking asynchronous file dialogs via `rfd::AsyncFileDialog` in `iced::Task::perform`.
- [x] Intercept window close events (`exit_on_close_request: false`) during active transfers with confirmation modal.
- [x] Write headless UI state transition tests (`tests/ui_state_test.rs`).

---

## Phase 5: Verification, Packaging & Windows Release (Active)
- [x] Pass strict quality gates (`cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` - 49/49 passed, `cargo fmt -- --check`).
- [x] Configure Windows application manifest (`#![windows_subsystem = "windows"]` to suppress terminal window).
- [x] Implement `PathSanitizer` defending against directory traversal, Win32 illegal characters, trailing dots/spaces, and all 25 DOS device names (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `CLOCK$`, `COM1-9`, `LPT1-9`).
- [x] Implement collision-safe destination jailing against existing files and active `.part` streams.
- [x] Implement `CrashReporter` in `%LOCALAPPDATA%\Qshare\logs\crash.log` with backtraces and headless CI dialog suppression.
- [x] Build portable single-file release executable (`cargo build --release`).
- [x] Configure automated GitHub Actions release workflow and enforce Strictly Version-Numbered Executable Invariant (Principle 8).
- [x] Publish initial pre-production developer preview release (`v0.2.0-alpha`).
- [ ] Generate installer bundle via `cargo-wix` / `Inno Setup`.

---

## Phase 6: Multiplatform Core Decoupling via UniFFI (Planned)
- [ ] Record formal architecture record ([ADR 0002](file:///d:/WebDev/rust/Qshare/docs/ADR/0002-native-ecosystem-cockpit-architecture.md)).
- [ ] Configure `Cargo.toml` with dual `[lib]` crate types (`rlib` and `cdylib`).
- [ ] Integrate Mozilla `uniffi-rs` interface definitions in `src/core/`:
  - [ ] Ephemeral keypair and Base32 ticket operations.
  - [ ] P2P transfer session lifecycle and chunk streaming states.
  - [ ] mDNS local network discovery emitter and peer lifecycle.
- [ ] Set up `cargo-ndk` cross-compilation pipeline for Android ABIs (`arm64-v8a`, `armeabi-v7a`, `x86_64`).
- [ ] Set up Apple target cross-compilation pipeline (`aarch64-apple-darwin`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`).
- [ ] Write headless foreign-function interface integration tests.

---

## Phase 7: Native Android APK Shell (Planned)
- [ ] Scaffold Android Studio project with Kotlin & Jetpack Compose (`mobile/android/`).
- [ ] Integrate generated `libqshare_core.so` and UniFFI Kotlin bindings.
- [ ] Implement `StorageAccessFramework` (SAF) and `DocumentFile` file picker and streaming sink.
- [ ] Implement `WifiManager.MulticastLock` lifecycle management to preserve mDNS packet ingress.
- [ ] Implement Android **Foreground Service** with active transfer progress notification bar.
- [ ] Implement CameraX QR barcode scanner for instant Share Ticket import.
- [ ] Register Android `ACTION_SEND` and `ACTION_SEND_MULTIPLE` intent filters for OS-wide Share Sheet support.
- [ ] Configure automated GitHub Actions APK release packaging (`qshare-v<version>-android.apk`).

---

## Phase 8: Native Apple Ecosystem Shells (Planned)
- [ ] Scaffold unified Xcode project with Swift & SwiftUI (`apple/`).
- [ ] Integrate generated UniFFI Swift framework.
- [ ] Build macOS native Cockpit:
  - [ ] Persistent Menu Bar icon and drag-and-drop dropzone.
  - [ ] Finder right-click context menu extension ("Share via Qshare").
  - [ ] Native macOS Notification Center integration.
- [ ] Build iPhone iOS native Cockpit:
  - [ ] iOS Share Extension for Photos, Files, and Safari.
  - [ ] AVFoundation camera QR code scanner.
  - [ ] `NSBonjourServices` local network entitlement for mDNS radar discovery.

