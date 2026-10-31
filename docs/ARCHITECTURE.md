# Qshare Architecture Specification

**Author:** Jabir Abdullah Haian & Antigravity  
**Version:** 0.2.0  
**Status:** In Active Pre-Production Development  

---

## 1. System Overview & Core Invariant

Qshare is an event-driven, zero-cloud-storage peer-to-peer file transfer desktop application written in 100% pure Rust. It couples **Iced** (declarative Elm-architecture GUI rendering directly to the GPU via `wgpu`) with **Iroh** (QUIC-based P2P networking and content-addressed blob streaming).

### The Zero-Cloud Invariant
Under NO circumstances should any user file content or file metadata be stored on or cached by third-party cloud servers.
- Signaling/Relay servers handle **ephemeral address coordination and end-to-end encrypted packet forwarding only**.
- All data moving across relays must be end-to-end encrypted with peer keys (TLS 1.3 / ChaCha20-Poly1305).
- Relays operate statelessly in memory with zero disk persistence.

---

## 2. High-Level System Topology

```
┌───────────────────────────────────────────────────────────────┐
│                      Iced Desktop Window                      │
│   (App State, Message Dispatcher, View Hierarchy, Canvas)     │
└──────────────────────────────▲────────────────────────────────┘
                               │ iced::Subscription / mpsc
┌──────────────────────────────▼────────────────────────────────┐
│                   Tokio Background Engine                     │
│  ┌──────────────────────┐           ┌──────────────────────┐  │
│  │   LAN Discovery      │           │    Iroh P2P Core     │  │
│  │  - mDNS Multicast    │           │  - Ephemeral NodeId  │  │
│  │  - UDP Port 5353     │           │  - STUN / Hole Punch │  │
│  │  - Local Stream Svc  │           │  - Blind Relay Path  │  │
│  └──────────────────────┘           └──────────────────────┘  │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                 Blob Streaming Pipeline                 │  │
│  │     - Fixed Chunk Buffer (64 KB - 256 KB)               │  │
│  │     - Real-Time BLAKE3 Hash Verification                │  │
│  │     - Atomic .part -> Target File Finalization          │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

---

## 3. Networking Protocols & Routing Mechanics

### 1. Local Network Discovery (Zero-Cloud LAN Mode)
- **Protocol:** Multicast DNS (`mDNS`) advertising on UDP `224.0.0.251:5353`.
- **Service Identifier:** `_qshare._tcp.local.`.
- **Payload:** Broadcasts local device display name, OS avatar badge, and ephemeral local listening port.
- **Data Flow:** Peers on the same subnet bypass all internet routing, exchanging data over direct local TCP/QUIC streams at local switch or Wi-Fi speeds.

### 2. Wide Area Network (Internet Direct P2P via Hole Punching)
- **Transport:** QUIC over UDP (leveraging TLS 1.3 encryption natively via Iroh 1.4.1).
- **Rendezvous Coordination:**
  1. Sender generates an ephemeral cryptographic keypair (Ed25519) and forms a canonical `QshareTicket`.
  2. The ticket contains:
     - `EndpointId` (Public key of the sender).
     - Rendezvous relay coordinates (validated against SSRF allowlists).
     - Content hash of the file payload (BLAKE3 root hash).
     - Payload total byte size.
     - Direct socket addresses.
  3. The receiver inputs the Base32-encoded ticket string into Qshare.
  4. Both peers ping the rendezvous signaling relay via STUN to discover their respective external public socket mappings (`Public IP:Port`).
  5. The relay swaps these external coordinates and exits the data path.
  6. Both peers simultaneously send probe packets to each other's mapped endpoints.
  7. Home and standard NAT firewalls map this incoming packet to the existing outbound state table, opening a direct bi-directional UDP tunnel (**UDP Hole Punch**).
  8. If direct UDP hole-punching fails (e.g. strict symmetric NAT or silent Windows Defender UDP drop), the connection transparently routes through the blind stateless relay, with dynamic telemetry updating the active route badge.

### 3. Stateless Blind Relay Fallback
- **Trigger:** Strict symmetric NAT (common on cellular 4G/5G mobile carriers or enterprise firewalls) where mapped ports randomize per connection.
- **Mechanism:** Traffic is relayed through an in-memory packet reflector (Iroh Relay / DERP).
- **Security Guarantee:** Relay servers receive only encrypted QUIC packets. Because payload encryption keys reside strictly on the sender and receiver endpoints, the relay is functionally blind to filenames, metadata, and contents.
- **Storage Footprint:** **0 MB**. Bytes pass through server memory buffers in real-time with zero disk writes.

---

## 4. Data Plane, Chunking & Verification

### 1. Streaming Pipeline
To prevent memory exhaustion during multi-gigabyte transfers:
- Files are read asynchronously via `tokio::io::AsyncRead`.
- Chunks are bounded between **64 KB and 256 KB**.
- Backpressure is maintained via bounded asynchronous channels (`tokio::sync::mpsc::channel(buffer_size)`).

### 2. Cryptographic Integrity via BLAKE3
- Transfers are keyed against a BLAKE3 root hash.
- As chunks arrive on the receiver:
  1. Chunks append directly to a temporary file: `<filename>.qshare.part`.
  2. The receiver computes an incremental BLAKE3 hash.
  3. If connection drops, resumption verifies already downloaded byte boundaries against the tree hash.
  4. Upon 100% completion and final hash confirmation, the `.part` file is atomically renamed to `<filename>`.

---

## 5. UI Architecture & Iced State Machine

The interface follows the pure Elm Architecture pattern:

### 1. State Struct (`QshareApp`)
```rust
pub struct QshareApp {
    pub current_tab: ActiveTab,
    pub lan_peers: Vec<LanPeer>,
    pub transfer_state: TransferState,
    pub share_ticket: Option<String>,
    pub input_ticket: String,
    pub active_route: RouteType, // LocalLan, DirectP2P, BlindRelay
}
```

### 2. Message Dispatcher (`Message`)
```rust
#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(ActiveTab),
    PeerDiscovered(LanPeer),
    PeerLost(NodeId),
    FileSelected(PathBuf),
    TicketInputChanged(String),
    GenerateTicket,
    ConnectWithTicket,
    ProgressUpdated { bytes_transferred: u64, total_bytes: u64, speed_bps: f64 },
    TransferCompleted(Result<(), String>),
}
```

### 3. Asynchronous Subscriptions (`iced::Subscription`)
The UI listens to background engine events via `Subscription::batch`:
- `lan_discovery_subscription`: Emits `Message::PeerDiscovered` when mDNS broadcasts arrive.
- `transfer_event_subscription`: Emits `Message::ProgressUpdated` and `Message::TransferCompleted` from the Tokio worker thread.

---

## 6. Threat Model & Security Posture

| Threat | Vector | Mitigation in Qshare |
| :--- | :--- | :--- |
| **Eavesdropping on Transfer** | Wi-Fi packet sniffing or ISP snooping | All transfers (both LAN and WAN) use end-to-end TLS 1.3 / ChaCha20-Poly1305 encryption with Ed25519 node keys. |
| **Man-in-the-Middle (MitM)** | Rogue signaling or relay node | Peer endpoints authenticate each other using cryptographic Ed25519 node public keys embedded in the Share Ticket. |
| **SSRF Relay Redirection** | Malicious ticket supplying internal IP as relay | Strict SSRF defense in `src/core/ticket.rs` rejecting non-HTTPS, loopback, link-local, RFC 1918 private subnets, and cloud metadata IPs (`169.254.169.254`). |
| **Payload Tampering** | Bit flips, MITM injection, or packet corruption | Real-time incremental BLAKE3 cryptographic hash verification. Corrupt chunks are isolated and rejected. |
| **Premature Disconnect Data Loss** | Connection drop at 99% of multi-GB file | Non-destructive quarantine (`SEC4-CRIT-01`): `.part` files are preserved on disk without premature hash-mismatch purging, enabling resumption. |
| **Cloud Data Leakage / Subpoena** | Server seizure or cloud breach | **Zero Cloud Storage**: No user data or metadata is ever saved on servers. Relays operate statelessly in memory with **0 MB disk persistence**. |
| **Disk Overwrite & Traversal** | Malicious peer sending `../../Windows/System32/file` | Strict `PathSanitizer` in `src/core/security.rs`: rejects path traversals (`..`, `/`, `\`), strips illegal Win32 characters, and prefixes all 25 DOS reserved device names (`CON`, `PRN`, `AUX`, `NUL`, etc.). |
| **Collision Overwrite** | Incoming file matching existing filename | Collision-safe auto-incrementing jailing resolving against both finalized files and active `.part` streams (`file (1).ext`). |
| **Antivirus Lock Stalls** | Windows Defender locking newly written files | Asynchronous retry loop with exponential backoff (`atomic_rename_with_retry`) mitigating transient sharing locks (`ERROR_SHARING_VIOLATION`). |

---

## 7. Multi-Platform Strategy & Native Shell Architecture

In accordance with [ADR 0002](file:///d:/WebDev/rust/Qshare/docs/ADR/0002-native-ecosystem-cockpit-architecture.md), Qshare implements a **"Shared Rust Core, Native Cockpits"** architecture to deliver the maximum possible optimization for each individual operating system:

```
                         ┌─────────────────────────────────────────┐
                         │      Universal Core (src/core/)         │
                         │   - Pure Rust (Iroh + BLAKE3 + mDNS)    │
                         └────────────────────┬────────────────────┘
                                              │
                 ┌────────────────────────────┼────────────────────────────┐
                 │ Direct in-process          │ UniFFI (Kotlin)            │ UniFFI (Swift)
                 ▼                            ▼                            ▼
┌─────────────────────────────────┐ ┌─────────────────────────────────┐ ┌─────────────────────────────────┐
│     Windows & Linux Desktop     │ │           Android APK           │ │       macOS & iPhone iOS        │
│  - Pure Rust (Iced + wgpu)      │ │  - Native Kotlin + Compose      │ │  - Native Swift + SwiftUI       │
│  - DirectX 12 / Vulkan GPU      │ │  - Storage Access Framework     │ │  - Metal GPU Acceleration       │
│  - 15 MB standalone executable  │ │  - Android Foreground Service   │ │  - AirDrop-like Dropzone        │
│  - ~20 MB idle RAM; 0 runtimes  │ │  - WifiManager.MulticastLock    │ │  - iOS Share Sheet Extension    │
│  - Instant 40ms launch          │ │  - CameraX QR Ticket Scanner    │ │  - Camera QR Ticket Scanner     │
└─────────────────────────────────┘ └─────────────────────────────────┘ └─────────────────────────────────┘
```

### Ecosystem Matrix & Native Capabilities

| Platform | Presentation Stack | Packaging Target | Graphics Engine | Memory & Size Profile | Native OS Capabilities Integrated |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Windows** | Pure Rust (`Iced 0.13`) | Portable `.exe` / `.msi` | DirectX 12 (`wgpu`) | ~15 MB size; ~20 MB RAM | Crash reporting to `%LOCALAPPDATA%`, Win32 Shell integration |
| **Linux** | Pure Rust (`Iced 0.13`) | Standalone binary / `.deb` | Vulkan / Wayland (`wgpu`) | ~14 MB size; ~18 MB RAM | Native Wayland/X11 surface, system tray |
| **Android** | Kotlin + Jetpack Compose | Native `.apk` / `.aab` | Android HWUI / Skia | ~12 MB size; native ART | Storage Access Framework (SAF), Foreground Service, `MulticastLock`, CameraX QR scanning, `ACTION_SEND` Share Sheet |
| **macOS** | Swift + SwiftUI | Native `.app` / `.dmg` | Apple Metal | ~16 MB size; native Cocoa | Persistent Menu Bar dropzone, Finder context menu, Notification Center |
| **iOS** | Swift + SwiftUI | Native `.ipa` | Apple Metal | ~14 MB size; native CocoaTouch | iOS Share Extension, `NSBonjourServices` local discovery, Camera QR scanner |

### Foreign Function Interface (FFI) Contract: Mozilla UniFFI
All mobile and Apple desktop interfaces communicate with the Rust engine via **Mozilla `uniffi-rs` (v0.28)**:
- Modular crate architecture in `crates/qshare-ffi/` depending on the headless core (`qshare = { path = "../..", default-features = false }`).
- Proc-macro annotations (`#[derive(uniffi::Record)]`, `#[uniffi::export]`, `uniffi::setup_scaffolding!()`) eliminating duplicate UDL maintenance.
- Compiles as a dynamic library (`cdylib`):
  - `libqshare_ffi.so` for Android (`arm64-v8a`, `armeabi-v7a`, `x86_64`).
  - `libqshare_ffi.dylib` for Apple Silicon & Intel macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`).
- Bindings are generated in `--library` mode directly from compiled binaries (`--library <path_to_cdylib> --no-format`), guaranteeing exact symbol synchronization across Kotlin and Swift runtimes.

---

## 8. Two-Tier CI/CD Pipeline & Automated Release Lifecycle

Qshare enforces a strict two-tier GitHub Actions architecture to balance continuous testing with clutter-free repositories and permanent release records:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Tier 1: Push / Nightly                          │
│                      (.github/workflows/nightly.yml)                   │
├────────────────────────────────┬───────────────────────────────────────┤
│ Trigger:                       │ Every push to `main`                  │
│ Build Scope:                   │ Windows (.exe/.zip), Android (.apk),  │
│                                │ macOS Universal (.zip)                │
│ Artifact Retention:            │ 3 days (Actions temporary storage)    │
│ Outdated Artifact Pruning:     │ Automatic via `gh api` cleanup step   │
│ Git Releases Created:          │ None                                  │
└────────────────────────────────┴───────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────────┐
│                    Tier 2: Unified Production Release                  │
│                      (.github/workflows/release.yml)                   │
├────────────────────────────────┬───────────────────────────────────────┤
│ Trigger:                       │ Manual click (workflow_dispatch) / v* │
│ Build Scope:                   │ All 3 platforms in parallel matrix    │
│ Changelog Generation:          │ Automated Categorized Conventional    │
│                                │ (✨ Features, 🐛 Fixes, ⚡ Perf, etc.) │
│ Verification:                  │ Consolidated SHA-256 Checksums table  │
│ Permanence:                    │ Permanent GitHub Releases (no delete) │
└────────────────────────────────┴───────────────────────────────────────┘
```

1. **Principle 8 Enforcement:** Every release artifact strictly embeds its semantic version (`qshare-v<version>-<platform>.<ext>`), completely eliminating bare, non-numbered binaries.
2. **Atomic Release Publisher:** Build jobs upload platform binaries as intermediate artifacts; a final publisher job downloads all artifacts, generates release notes with commit attribution, computes cryptographic verification hashes, and creates the single release entry.

