# Qshare Known Issues & Risk Register

This document tracks active bugs, network field edge cases, security considerations, and resolved historical issues for Qshare.

## Status Legend
- `[ACTIVE]` - Verified issue requiring remediation.
- `[DEFERRED]` - Documented edge case or trade-off scheduled for future sprint.
- `[RESOLVED]` - Addressed and verified in codebase.

---

## Active & Deferred Issues

### KI-001: Windows Defender Firewall Silent UDP Block
- **Category:** Operating System / Network
- **Status:** `[ACTIVE]`
- **Impact:** High (Inbound connection drops on public networks)
- **Description:** On Windows, binding an unreserved UDP socket or listening for mDNS multicast triggers a Windows Defender Firewall popup. If the user selects "Public" or declines permissions, inbound UDP hole-punching packets are dropped silently without throwing an OS error.
- **Planned Mitigation:** Implement connection timeout heuristics in the transport layer: if direct UDP hole-punching does not complete within 3 seconds, automatically failover to the stateless outbound-initiated encrypted relay without halting the transfer.

### KI-002: Public Wi-Fi Client Isolation Dropping mDNS
- **Category:** Network Environment
- **Status:** `[DEFERRED]`
- **Impact:** Medium (LAN peer discovery disabled on hotel/university Wi-Fi)
- **Description:** Public Wi-Fi routers frequently enable AP/Client Isolation, preventing multicast packets between peers on the same subnet.
- **Planned Mitigation:** The UI will feature an explicit prompt: *"Can't see nearby device? Use Send Anywhere Ticket instead."* ensuring users are never blocked by router configurations.

### KI-003: Large Directory Transfer Packaging Overhead
- **Category:** Performance / File I/O
- **Status:** `[DEFERRED]`
- **Impact:** Low (Folders with thousands of small files)
- **Description:** Sending thousands of loose files generates excessive round-trip ACK overhead across network streams.
- **Planned Mitigation:** Implement a streaming archive pipeline (`tar` or manifest-backed blob collection) so directory trees stream as a single continuous BLAKE3-verified sequence without temporary disk duplication.

---

## Resolved Historical Issues

### KI-004: Windows Subsystem 0×0 Invisible Window Launch
- **Category:** GUI / Windows Platform
- **Status:** `[RESOLVED]` (Resolved on 2026-10-30 in commit `1d6e5a7`)
- **Impact:** High (Process ran in background but window never appeared on screen)
- **Description:** Invoking `iced::application().centered().run()` without explicit `window::Settings` caused `winit` on Windows to allocate an uninitialized surface measuring `Width=0, Height=0` with `Visible=False`. Combined with `#![windows_subsystem = "windows"]`, the app appeared completely unresponsive to the user.
- **Resolution:** Configured explicit `iced::window::Settings` with an initial size of `850 × 650`, `min_size: 600 × 450`, and centered positioning. Attached `std::panic::set_hook` and `rfd::MessageDialog` to ensure any future launch issues or panics display a native Windows error popup and log to `qshare_crash.log`.

### KI-005: Transitive Windowing Dependencies in FFI Mobile Targets
- **Category:** Build & Toolchain / Mobile
- **Status:** `[RESOLVED]` (Resolved on 2026-10-31 in commit `0af4ab0`)
- **Impact:** High (Android NDK cross-compilation failed due to `android-activity` missing Activity backend)
- **Description:** `crates/qshare-ffi` depended directly on `qshare` with default features enabled, pulling in `iced` -> `winit` -> `android-activity`. Because `crates/qshare-ffi` is a headless background engine, the inclusion of windowing crates broke Android compilation.
- **Resolution:** Introduced `[features] default = ["desktop"]` in root `Cargo.toml`, gated `iced` and `rfd` as optional desktop dependencies, and added `required-features = ["desktop"]` on `[[bin]] name = "qshare"`. Updated `crates/qshare-ffi` to depend on `qshare = { path = "../..", default-features = false }`.

### KI-006: UniFFI Bindgen UDL Syntax Error with Proc-Macro Scaffolding
- **Category:** Build & Toolchain / UniFFI
- **Status:** `[RESOLVED]` (Resolved on 2026-10-31 in commit `92a0066`)
- **Impact:** High (`uniffi-bindgen generate` exited with `parse error` on Rust source files)
- **Description:** Invoking `uniffi-bindgen generate src/lib.rs` caused the CLI to invoke the `weedle2` UDL parser on raw Rust syntax, expecting a WebIDL `.udl` definition rather than procedural macro annotations.
- **Resolution:** Updated CI bindgen invocations to pass the compiled dynamic library via `--library` mode (`--library <path_to_cdylib> --no-format`), extracting procedural macro definitions directly from the compiled binary.

### KI-007: Netdev libc in6_ifreq Darwin vs. iOS Symbol Mismatch
- **Category:** Build & Toolchain / Apple iOS
- **Status:** `[RESOLVED]` (Resolved on 2026-10-31 in commit `92a0066`)
- **Impact:** Medium (`cargo build --target aarch64-apple-ios` failed with unresolved symbol `in6_ifreq`)
- **Description:** The `netdev` crate's Darwin network interface enumerator referenced `libc::in6_ifreq`, which is exposed on macOS (Darwin) but omitted in the iOS libc definitions.
- **Resolution:** Scoped the active Apple build matrix to macOS Universal desktop targets (`aarch64-apple-darwin` and `x86_64-apple-darwin`) and standalone Swift framework packaging. iOS interface enumeration will be integrated in Phase 8 via Apple's native `Network.framework`.
