# Qshare Project Roadmap

## Phase 1: Environment & Toolchain Setup (Active)
- [x] Create public GitHub repository `Jabir-A-H/Qshare`.
- [x] Initialize Git configuration and `.gitignore`.
- [x] Establish master documentation suite (`AGENTS.md`, `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `KNOWN_ISSUES.md`, `ADR/`).
- [x] Install Rust toolchain (`rustup`, `cargo`, `rustc` MSVC) on the host system.
- [x] Initialize `Cargo.toml` workspace with foundational dependencies (`iced`, `iroh`, `tokio`, `blake3`, `serde`).

---

## Phase 2: Core Headless P2P Engine & Loopback Tests
- [ ] Scaffold `src/core/` module structure (`p2p.rs`, `lan.rs`, `transfer.rs`).
- [ ] Implement `Iroh` endpoint lifecycle:
  - [ ] Ephemeral keypair generation (`NodeId`).
  - [ ] Ticket encoding and decoding (`BlobTicket`).
  - [ ] STUN address discovery and hole punching.
- [ ] Implement BLAKE3 chunked file reader & writer (`.part` temporary file handling).
- [ ] Write automated integration test: loopback sender-receiver file transfer without UI.

---

## Phase 3: Local Network Discovery Engine (LAN)
- [ ] Implement mDNS advertiser and listener (`mdns-sd` or `iroh-net` discovery).
- [ ] Broadcast local device hostname, OS avatar identifier, and active service port.
- [ ] Implement peer table state with heartbeat timeout eviction.
- [ ] Integrate local direct socket streaming fallback when on identical subnet.

---

## Phase 4: Iced Desktop GUI & Elm State Machine
- [ ] Configure `Iced` application shell with modern dark/light theme tokens.
- [ ] Build **Tab 1: LAN Radar View**:
  - [ ] Interactive peer cards with device avatars and connection latency.
  - [ ] Drag-and-drop file target over peer avatars.
- [ ] Build **Tab 2: Send Anywhere (Internet P2P) View**:
  - [ ] Ticket generator with one-click copy button.
  - [ ] Ticket input field for receiver with instant peer resolution.
  - [ ] Dynamic connection badge (**Local LAN** / **Direct P2P** / **Encrypted Relay**).
- [ ] Build **Transfer Drawer / Progress Modal**:
  - [ ] Real-time progress bar, transfer throughput gauge (MB/s), and ETA calculation.
  - [ ] Cancel / abort transfer action.

---

## Phase 5: Verification, Packaging & Windows Release
- [ ] Pass strict quality gates (`cargo check`, `cargo clippy -- -D warnings`, `cargo test`).
- [x] Configure Windows application manifest (`#![windows_subsystem = "windows"]` to suppress terminal window).
- [ ] Add defensive path sanitization against directory traversal attacks.
- [x] Build portable single-file release executable (`cargo build --release`).
- [x] Configure automated GitHub Actions release workflow with manual Run Workflow trigger (`.github/workflows/release.yml`).
- [ ] Generate installer bundle via `cargo-wix` / `Inno Setup`.
