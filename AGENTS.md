# Qshare: AI Agent & Developer Principles

This document defines the core principles, operational invariants, and safety standards for all AI agents and developers contributing to the **Qshare** codebase.

## 🎯 Mission
Build and maintain **Qshare**, a high-performance, privacy-first, zero-cloud-storage peer-to-peer file transfer desktop application built in pure Rust (`Iced` + `Iroh`)—combining the instant visual simplicity of local Wi-Fi dropzones with global, NAT-traversing direct P2P reach.

---

## 🏛️ Core Principles

### 📌 Principle 1: Single Source of Truth & Living Documentation
- Documentation must accurately reflect reality. When architecture, networking topologies, protocol messages, or UI states evolve, update the relevant docs before concluding work.
- **Living Guidance, Not Dogma:** Principles and architectural guidelines provide consistency, but must never become dogma that blocks modern platform advancements. When modern transport capabilities, platform primitives, or evolving framework features offer a superior solution that conflicts with an existing rule, surface the conflict openly so the team can evolve the standard.
- **Hierarchy of Truth:**
  1. [`docs/ARCHITECTURE.md`](file:///d:/WebDev/rust/Qshare/docs/ARCHITECTURE.md) (Canonical system architecture, networking topologies, protocol messages, and threat model)
  2. [`docs/ADR/*`](file:///d:/WebDev/rust/Qshare/docs/ADR) (Sequential Architecture Decision Records: 0001, 0002...)
  3. [`docs/ROADMAP.md`](file:///d:/WebDev/rust/Qshare/docs/ROADMAP.md) (Living milestone tracker and active task checklist)
  4. [`docs/KNOWN_ISSUES.md`](file:///d:/WebDev/rust/Qshare/docs/KNOWN_ISSUES.md) (Active bugs, network field edge cases, and resolved issue history)
  5. Live code when documentation and implementation drift.
- If documentation and code conflict, do not guess. Flag the drift and reconcile docs to match reality.

---

### 🔒 Principle 2: Zero-Cloud Storage & Cryptographic Privacy Invariant
- **The Zero-Cloud Invariant:** Under NO circumstances should any user file content or file metadata be stored on or cached by third-party cloud servers.
- **Ephemeral Coordination Only:** Signaling and rendezvous servers handle lightweight address coordination (swapping public socket addresses) and immediately step out of the data path.
- **Blind Relay Fallback:** For the ~10% of symmetric NAT connections requiring a relay, packets stream through server RAM in real-time like water through a pipe. Relays operate statelessly with **0 MB disk persistence**.
- **End-to-End Cryptographic Security:** All transfers (both direct and relayed) are encrypted peer-to-peer using TLS 1.3 / ChaCha20-Poly1305 with Ed25519 node keys. Relays and intermediate network hops cannot decrypt filenames, metadata, or file bytes.

---

### ⚡ Principle 3: Native Rust GPU Architecture (Iced + Iroh Invariants)
- **100% Pure Rust Desktop App:** The GUI compiles to a native desktop binary drawing directly to the GPU via `wgpu` (DirectX 12 / Vulkan / Metal). Zero WebViews, zero Chromium, and zero Electron wrappers.
- **Strict Separation of Concerns & Non-Blocking UI Thread:**
  - The UI layer (`src/app.rs`, `src/ui/`) is declarative (`Iced`). It must **NEVER block the thread or directly run blocking network or disk I/O** inside `update()` or `view()`.
  - The networking engine (`src/core/`) runs independently on a dedicated multi-threaded `tokio` runtime.
  - UI and Engine communicate strictly via asynchronous channels (`iced::Subscription` and `tokio::sync::mpsc`).
- **Streaming Pipeline & Bounded Buffers:**
  - Never load multi-gigabyte files into RAM. Stream files in fixed-size chunks (64 KB – 256 KB) via `tokio::io::AsyncRead`.
- **Atomic Disk Integrity:**
  - Write incoming streams to a temporary `.part` file (`<filename>.qshare.part`).
  - Verify data integrity incrementally using **BLAKE3** hashes.
  - Atomically rename the `.part` file to the target destination only upon 100% verified transfer completion.

---

### 🔍 Principle 4: Collaborative Planning & Competing Alternatives
- **Formulate Competing Alternatives for Non-Trivial Work:** For any new feature addition, architectural adjustment, refactoring, or bug fix (big or small), do not jump into code or make unilateral decisions. Formulate **at least 2 to 3 distinct solution approaches based on complexity and risk**.
- **Meaningful Technical Trade-offs (No Strawmen):**
  - **Why do it (benefits):** Real functional, architectural, performance, latency, or UX advantages.
  - **Why not do it (risks & failure modes):** Real engineering risks—such as race conditions, memory leaks, blocking event loops, network timeouts, firewall drops, or cognitive complexity.
  - **Banned trivial cons:** NEVER cite normal development artifacts (such as *"creates a new file"*, *"adds a wrapper component"*, or *"adds boilerplate"*) as a reason not to implement a clean solution.
- **Adversarial Stress-Testing ("Doubt Gate"):**
  - Cross-examine every non-trivial proposal against: *"Under what real-world field conditions will this clean-looking code fail?"* (e.g. Wi-Fi client isolation, symmetric NAT, Windows firewall drops, multi-gigabyte memory pressure).
- **Favor Simplicity:** Favor simple, robust, high-density solutions over speculative abstraction layers or premature generalization.
- **Trivial-Change Exception:** Skip comparison only for completely obvious, single-path changes (e.g. deleting an unused import, fixing a typo, updating an obvious path, or a single trivial lint fix).
- **Surgical, Token-Conscious Execution:** Deliver changes in thin, verifiable slices. Prefer surgical, localized edits over sprawling multi-file rewrites.

---

### 🦀 Principle 5: Idiomatic Rust Standards
- **Owned Values Over Lifetime Battles:** For UI state, message passing, and network commands, prefer owned types (`String`, `PathBuf`, `Vec<u8>`) and call `.clone()` where appropriate. Avoid gratuitous lifetime annotations (`'a`) across component boundaries.
- **Explicit Error Handling:** Use `Result` and `Option` combinators. Avoid `unwrap()` and `expect()` in production paths; bubble errors up or map them to user-facing UI notifications.
- **Decoupled Engine:** The `src/core/` networking and transfer engine must remain completely agnostic of `Iced`, allowing full automated test coverage via headless integration tests.

---

### 🛡️ Principle 6: Verification Before Velocity
A change is incomplete until verified. Always run the project's quality pipeline in order:
1. `cargo check` — Strict type checking and borrow-checker verification (0 errors).
2. `cargo clippy -- -D warnings` — Static analysis and linting (0 warnings/errors).
3. `cargo test` — Unit and integration test suites (100% pass rate).
4. `cargo fmt -- --check` — Idiomatic formatting check.
5. `cargo build` — Clean debug or release compilation (exit code `0`).

---

### 🔒 Principle 7: Safe Execution, Quarantine & User Trust
- **The Human Commits:** Never run `git commit` or stage changes on your own initiative. Complete the assigned goal, run verification gates, and stop with changes uncommitted for user review.
- **Format Commit Messages for the User:** Provide recommended Conventional Commit messages formatted for the user to execute:
  - Format: `type(scope): subject` (e.g. `feat(p2p): add iroh ticket generation and loopback streaming`).
- **No Destructive Git Operations:** Never execute destructive git operations (`git checkout <file>`, `git restore`, `git stash`, `git reset --hard`) without explicit user permission.
- **Safe Quarantine Protocol (`_discarded/`):** When refactoring, replacing, or deprecating files, components, or scripts, **never delete them unilaterally**. Move them into `_discarded/` and await user review.
- **Clickable Links:** Always link modified files and code symbols using `file:///` URLs for seamless IDE navigation.

---

### 🏷️ Principle 8: Strictly Version-Numbered Executable Invariant
- **No Non-Numbered Binaries:** Never make, retain, distribute, or present unversioned executable files (e.g. bare `qshare.exe`).
- **Explicit Version Embedding:** Every built release executable and distribution package must have its semantic version explicitly embedded in the filename (e.g. `qshare-v0.2.0-windows-x64.exe` and `qshare-v0.2.0-windows-x64.zip`).
- **Post-Build Rename & Sanitization:** Whenever `cargo build --release` produces a default unversioned binary (`target/release/qshare.exe`), it must immediately be renamed or packaged with its semantic version tag (`qshare-v<version>-windows-x64.exe`), and any unversioned binary must be removed from release directories so that only version-numbered release artifacts exist.
