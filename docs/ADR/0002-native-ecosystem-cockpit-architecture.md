# ADR 0002: Native Ecosystem Multiplatform Strategy (Shared Rust Engine, Native Cockpits)

## Status
Accepted

## Context
Qshare's mission is to deliver zero-cloud, high-throughput, privacy-first P2P file transfers across local Wi-Fi dropzones and the public internet. While the initial Phase 1–5 focus established the core headless engine and Windows desktop application, full field utility requires ubiquitous multi-device support:
- **Desktop:** Windows, Linux, and macOS.
- **Mobile:** Android (APK) and iPhone (iOS).

In cross-platform engineering, there is a fundamental conflict between **single-codebase convenience** and **ecosystem optimization**:
1. **Universal Cross-Platform Wrappers (Electron, Tauri WebView, Compose Multiplatform JVM, Flutter):**
   - Imposes a lowest-common-denominator compromise.
   - On desktop, bundling a JVM (Compose Desktop) inflates binaries to ~100 MB and baseline RAM to ~120 MB.
   - WebViews (Tauri/Electron) lack native GPU canvas rendering and introduce IPC DOM latency.
2. **Pure Rust Mobile Wrappers (Raw `NativeActivity` / `cargo-apk` / `Iced` on mobile):**
   - Rust windowing frameworks (`winit`/`wgpu`) on Android/iOS lack native touch ergonomics, smooth gestures, and accessibility (TalkBack/VoiceOver).
   - Critically, raw native activities cannot cleanly access mobile operating system primitives (Android Scoped Storage / SAF, iOS Share Sheets, Android Foreground Services, and camera QR scanning) without writing extensive manual JNI/Objective-C shims.

We require an architectural blueprint that delivers the **absolute maximum possible optimization for each individual platform** with zero runtime bloat, zero JVM on desktop, zero WebViews, and complete platform OS integration.

## Decision
We adopt the **"Shared Rust Engine, Native Cockpits"** architecture (as battle-tested by 1Password, Signal, Tailscale, and Firefox):

```
                         ┌────────────────────────────────────────────────────────┐
                         │             Qshare Core Engine (Pure Rust)             │
                         │  - Iroh QUIC Transport & Direct UDP Hole Punching      │
                         │  - Incremental BLAKE3 Stream Chunking & Verification   │
                         │  - mDNS RFC 6762 Multicast Subnet Discovery Engine     │
                         └───────────────────────────┬────────────────────────────┘
                                                     │
                   ┌─────────────────────────────────┼─────────────────────────────────┐
                   │ In-Process Channels (Tokio/Iced)│ Mozilla UniFFI (Kotlin)         │ Mozilla UniFFI (Swift)
                   ▼                                 ▼                                 ▼
┌───────────────────────────────────────┐ ┌─────────────────────────────────────┐ ┌─────────────────────────────────────┐
│       Windows & Linux Desktop         │ │             Android APK             │ │       macOS & iPhone iOS            │
│  - Pure Rust (Iced + wgpu)            │ │  - 100% Native Kotlin + Compose     │ │  - 100% Native Swift + SwiftUI      │
│  - DirectX 12 (Win) / Vulkan (Linux)  │ │  - Android NDK (libqshare_core.so)  │ │  - Metal GPU Acceleration           │
│  - 15 MB single-file native binary    │ │  - Storage Access Framework (SAF)   │ │  - AirDrop-like Dropzone & Menu Bar │
│  - ~20 MB idle RAM; 40ms launch       │ │  - Android Foreground Service       │ │  - iOS Share Sheet Extension        │
│  - Zero runtimes / Zero WebViews      │ │  - WifiManager.MulticastLock        │ │  - Camera QR Ticket Scanner         │
│                                       │ │  - CameraX QR Ticket Scanner        │ │  - NSBonjourServices mDNS Entitlement│
└───────────────────────────────────────┘ └─────────────────────────────────────┘ └─────────────────────────────────────┘
```

### 1. Universal Rust Core Engine (`src/core/`)
- Remains 100% pure Rust, agnostic of any UI or presentation layer.
- Compiles as a dual-target library: `rlib` for in-process Rust desktop builds, and `cdylib` for mobile/Apple shared libraries.
- Uses **Mozilla `uniffi-rs`** to generate type-safe Kotlin and Swift bindings directly from Rust UDL / proc-macro interfaces, eliminating brittle manual JNI/C-FFI boilerplate.

### 2. Tier-1 Ecosystem Cockpits
- **Windows & Linux (Pure Rust + Iced):**
  - Renders directly to the GPU via `wgpu` (DirectX 12 on Windows, Vulkan on Linux).
  - Produces a single, portable, self-contained native binary (~15 MB) with ~20 MB idle RAM and zero external runtime dependencies.
- **Android (Native Kotlin + Jetpack Compose):**
  - Compiled directly to an Android Application Package (`.apk`) / Android App Bundle (`.aab`).
  - Native integration with `StorageAccessFramework` (SAF) and `DocumentFile` for Scoped Storage compliance.
  - Native Android **Foreground Service** with active transfer notifications, preventing the OS from terminating multi-gigabyte transfers in the background.
  - Hardware `WifiManager.MulticastLock` preventing Wi-Fi drivers from discarding mDNS discovery packets.
  - CameraX barcode scanning for instantaneous Share Ticket QR capture.
  - Native `ACTION_SEND` / `ACTION_SEND_MULTIPLE` intent filters (Share Sheet integration).
- **macOS & iOS (Native Swift + SwiftUI):**
  - Renders directly via Apple's Metal graphics API.
  - Shares a single, unified UniFFI Swift binding between macOS and iOS.
  - Native macOS Finder context menu extensions ("Send via Qshare") and persistent Menu Bar dropzone.
  - Native iOS Share Extension and `NSBonjourServices` local network discovery entitlement.

## Consequences

### Positive
- **Uncompromised Performance:** Zero emulation layers, zero WebViews, zero JVM overhead on desktop, and 60/120 Hz native ProMotion responsiveness on mobile.
- **Maximum Battery Efficiency:** Mobile operating systems can manage background tasks, radio states, and deep sleep cycles using first-party APIs.
- **Single Source of Truth for Network & Cryptography:** 100% of Iroh QUIC routing, hole punching, Base32 ticket validation, SSRF defense, and BLAKE3 hashing resides in a single, rigorously tested Rust codebase.
- **Zero Manual FFI Maintenance:** Mozilla `uniffi-rs` generates typed Kotlin and Swift wrappers automatically, ensuring compile-time safety across language boundaries.

### Negative / Trade-offs
- **Multiple UI Codebases:** Requires maintaining two distinct UI presentation layers (Rust `Iced` for Desktop; Kotlin `Compose` for Android; Swift `SwiftUI` for Apple platforms).
- **NDK & Xcode Toolchain Dependencies:** Mobile builds require the Android NDK (`cargo-ndk`) and Xcode CLI tools for compilation.
