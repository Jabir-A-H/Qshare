# ADR 0003: Two-Tier CI/CD Pipeline and Headless FFI Decoupling

## Status
Accepted

## Date
2026-10-31

## Context
As Qshare expanded from a Windows desktop prototype toward native Android and Apple cockpits ([ADR 0002](file:///d:/WebDev/rust/Qshare/docs/ADR/0002-native-ecosystem-cockpit-architecture.md)), several cross-platform compilation and deployment constraints emerged:

1. **Transitive Windowing Dependencies in Mobile Engine:** When building `crates/qshare-ffi`, importing the root `qshare` crate with default features dragged in `iced`, `winit`, and `android-activity`. Because `crates/qshare-ffi` is a headless background engine, the inclusion of windowing libraries caused compilation failures on Android NDK runners and bloated compilation times.
2. **GitHub Actions Storage & Clutter Prevention:** Building standalone binaries (`.exe`, `.apk`, `.dmg`/`.zip`) on every push risks exhausting GitHub Actions storage and cluttering repository assets if treated as permanent releases.
3. **Manual vs. Automated Release Workflow:** Official releases require permanent versioning, cryptographic checksums, and human-readable categorized Conventional Changelogs, whereas push/nightly builds only require short-lived developer previews.
4. **UniFFI 0.28 Proc-Macro Scaffolding Requirements:** Passing raw `.rs` source files directly to `uniffi-bindgen generate` invokes the legacy UDL parser (`weedle2`), which crashes with a `parse error` on valid Rust syntax.

---

## Decision

### 1. Feature Flag Decoupling in Core Engine
We decouple desktop GUI dependencies from the headless core engine in `Cargo.toml`:
- Added `[features] default = ["desktop"]`.
- Placed `iced` and `rfd` behind the optional `desktop` feature: `desktop = ["dep:iced", "dep:rfd"]`.
- Enforced `required-features = ["desktop"]` on `[[bin]] name = "qshare"`.
- Configured `crates/qshare-ffi` to depend strictly on `qshare = { path = "../..", default-features = false }`.

### 2. Two-Tier GitHub Actions Architecture
We divide continuous integration into two distinct workflows:
1. **Tier 1: Nightly & Push Builds (`.github/workflows/nightly.yml`):**
   - Triggers on every push to `main` (and `workflow_dispatch`).
   - Builds Windows (`.exe`/`.zip`), Android (`.apk`), and macOS (`.zip`) in parallel.
   - Uploads artifacts with a 3-day retention policy (`retention-days: 3`).
   - Runs an automated `prune-old-artifacts` step querying the GitHub API (`gh api`) to delete artifacts from previous runs of this workflow, guaranteeing zero artifact clutter.
2. **Tier 2: Unified Multi-Platform Release (`.github/workflows/release.yml`):**
   - Triggers on manual dispatch (`workflow_dispatch`) or semantic version tags (`v*`).
   - Builds all 3 platform targets simultaneously.
   - A final `publish-release` job downloads all artifacts, parses commit history into a rich categorized Conventional Changelog (✨ Features, 🐛 Bug Fixes, ⚡ Performance, 📝 Documentation, 🔧 Maintenance), generates a consolidated SHA-256 table, and creates a permanent GitHub Release without deleting prior releases.

### 3. UniFFI `--library` Mode Bindgen Invocation
Instead of passing source files to `uniffi-bindgen`, CI compiles the native engine (`libqshare_ffi.so` for Android and `libqshare_ffi.dylib` for macOS) and invokes bindgen with `--library <path> --no-format`, extracting proc-macro metadata directly from the compiled binary.

---

## Consequences

### Positive
- **Instant Headless Compilation:** FFI crate compiles in ~40 seconds with zero graphical/windowing dependencies.
- **Zero Artifact Clutter:** Actions storage stays lean while preserving the latest push artifacts for immediate testing.
- **Professional Release Notes:** Replaces GitHub's bare commit comparison link with categorized Conventional Commit notes.
- **Strict Version Numbering:** Principle 8 is enforced across all builds (`qshare-v<version>-<platform>.<ext>`).

### Negative / Trade-offs
- The release workflow requires intermediate artifact uploads between jobs before final aggregation and publishing.
- iOS target cross-compilation is scoped to Phase 8 to replace `netdev` with Apple's `Network.framework`.
