# Architecture Critique: Segment 1 — desktop-ui-shell

**Document Version:** 1.0.0  
**Target:** Qshare Desktop Application (`d:\WebDev\rust\Qshare`)  
**Status:** Under Architectural Review  
**Evaluators:** Critic 1 (Investigator) & Critic 2 (Adversarial Doubt Gate)  
**Date:** 2026-10-30  

---

## 1. Executive Summary & Health Assessment

- **Segment Health Score:** 46/100
- **Summary:**  
  Segment 1 (`desktop-ui-shell`) encompasses the GPU-accelerated window lifecycle, Elm application architecture (`Iced 0.13`), navigation dispatch, native file dialogs (`rfd 0.15`), and the asynchronous channel bridge connecting the user interface to background `tokio` network engines. While the current prototype in [`src/main.rs`](file:///d:/WebDev/rust/Qshare/src/main.rs) successfully establishes an explicit centered window geometry (`850 × 650`, resolving [KI-004](file:///d:/WebDev/rust/Qshare/docs/KNOWN_ISSUES.md#L39-L44)), initializes `tracing_subscriber`, and attaches a panic hook, the codebase remains in an unintegrated skeletal state that conceals critical runtime vulnerabilities and architectural blockers.

  Most prominently, the proposed architectural specification for the asynchronous subscription bridge contains an immediate runtime panic hazard (`event_receiver.lock().await.take().expect(...)`), which will crash the application as soon as the subscription stream restarts or re-evaluates. Furthermore, the application currently lacks a modular library structure (`src/lib.rs`), making headless unit testing impossible; contains mock leaks and dead-end CTAs that terminate without notifying the background engine; exhibits UI thread blocking hazards via synchronous message dialogs; and enables `exit_on_close_request: true`, which forcibly kills active multi-gigabyte transfers without warning or cleanup.

  Adversarial cross-examination also reveals systemic unhandled field hazards: high-frequency byte progress events from the transfer engine threaten to saturate the UI event channel (triggering dropped terminal lifecycle events and UI freeze); rapid double-clicking on buttons spawns concurrent uncoordinated OS dialogs and duplicate dialing tasks; Windows clipboard lock contention fails silently without user remediation; and drag-and-drop file ingestion—though explicitly promised by the UI copy—is entirely missing from the event loop.

- **Invariant Compliance Status:**
  - *Zero-Cloud Storage Invariant (0 MB Relay Disk):* **COMPLIANT**. The UI shell operates completely locally with zero telemetry, remote tracking beacons, or cloud analytics.
  - *Non-Blocking UI Thread Invariant:* **DRIFT DETECTED / AT RISK**. Synchronous `rfd::MessageDialog::show()` blocks the main winit/wgpu thread during error paths. In addition, the file picker dispatch pattern in the spec lacks an explicit asynchronous task contract.
  - *Atomic Disk Finalization Invariant:* **COMPLIANT (DELEGATED)**. Disk staging is managed by downstream segments, but Segment 1 lacks graceful transfer abortion hooks, causing dirty `.part` leftovers if the user closes the window.
  - *Rust Memory Safety & Pure Native Binary:* **AT RISK**. While free of `unsafe` blocks, the proposed subscription stream introduces guaranteed panic conditions (`.take().expect()`), and panic diagnostics fail to capture executable call-stack backtraces.

---

## 2. Desktop UX & Visual Craft Audit (`/impeccable`)

### 2.1 Visual Hierarchy & Layout Architecture
The current UI layout in [`src/main.rs:79-118`](file:///d:/WebDev/rust/Qshare/src/main.rs#L79-L118) renders a single centered column with elementary text and button widgets:
1. **Window Geometry & Sizing Constraints:** Configuring an explicit window size of `850.0 × 650.0` with `min_size: Some(Size::new(600.0, 450.0))` correctly prevents the uninitialized `0×0` invisible window launch bug on Windows (`KI-004`). However, no maximum window dimensions or responsive column width caps are enforced; maximizing the window on a 4K ultrawide monitor expands text lines across 3840 horizontal pixels, resulting in severe typographic strain.
2. **Typography Scale & Contrast:** The title uses `size(32)`, subtitle `size(13)`, and body text `size(15)`. While sizes are legible, there is no font hierarchy token system. Text widgets rely entirely on the default system font without semantic weights (Bold, Medium, Regular) or muted color contrasts (`text_muted`), leaving the visual weight undifferentiated.
3. **Spacing Rhythm & Layout Padding:** Spacing is hardcoded with magic numbers (`spacing(16)`, `spacing(12)`, `padding(20)`). The outer container uses `center_x` and `center_y`, which anchors content in the dead center of the screen; as tab content dynamically grows (e.g. populating the LAN radar with 12 discovered peers), the entire navigation bar jumps vertically on screen, causing severe Cumulative Layout Shift (CLS).
4. **Theme Design Tokens:** Theme configuration relies on the stock `Theme::Dark` preset in [`src/main.rs:30`](file:///d:/WebDev/rust/Qshare/src/main.rs#L30). No custom palette or surface elevation tokens (background, surface, surface-elevated, accent, border, danger) are defined in a dedicated `theme.rs`.

### 2.2 Component State Matrix

| Component / Widget | Default | Hover / Focus | Active / Loading | Disabled | Empty State | Error State |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Navigation Tabs (`Nearby LAN` / `Send Anywhere`)** | OK | Stock Iced | **MISSING** (No active indicator) | **MISSING** | N/A | N/A |
| **"Select File to Share" CTA** | OK | Stock Iced | **MISSING** | **MISSING** (Spam-clickable) | N/A | N/A |
| **Transfer Status Text** | OK | N/A | **MISSING** (No spinner/gauge) | N/A | OK | **MISSING** |
| **LAN Peer Radar Widget** | **MISSING** | **MISSING** | **MISSING** | **MISSING** | Stub Text Only | **MISSING** |
| **Ticket Card & Input Box** | **MISSING** | **MISSING** | **MISSING** | **MISSING** | **MISSING** | **MISSING** |
| **Progress Modal / Drawer** | **MISSING** | **MISSING** | **MISSING** | **MISSING** | N/A | **MISSING** |
| **Toast Notifications** | **MISSING** | N/A | N/A | N/A | N/A | **MISSING** |

### 2.3 Microcopy, Telemetry & User Feedback
1. **Active Tab Indistinguishability:** In [`src/main.rs:84-89`](file:///d:/WebDev/rust/Qshare/src/main.rs#L84-L89), both navigation buttons are instantiated with identical styling:
   ```rust
   let nav = row![
       button(text("Nearby LAN Radar")).on_press(Message::TabChanged(Tab::LocalLan)),
       button(text("Send Anywhere (Internet)")).on_press(Message::TabChanged(Tab::SendAnywhere)),
   ].spacing(12);
   ```
   There is zero visual differentiation between the active tab and the inactive tab. The user receives no color, underline, pill highlight, or badge feedback indicating which mode is active.
2. **Misleading Drag & Drop Microcopy:** Line 99 displays: `"Drag & drop files or click to generate an encrypted one-time Share Ticket."` Yet the application implements zero drag-and-drop event listeners. Dragging a file into the window produces no visual dropzone target, no hover highlight, and no file ingestion, violating fundamental user trust.
3. **Opaque Radar Microcopy:** Line 93 displays: `"Looking for nearby devices on your Wi-Fi router via mDNS (multicast)..."`. If Wi-Fi Client Isolation is active ([KI-002](file:///d:/WebDev/rust/Qshare/docs/KNOWN_ISSUES.md#L21-L27)) or the network adapter is disconnected, this string sits static indefinitely. It provides no discovery timeout, no failure fallback notice, and no animated radar pulse.
4. **Absence of Real-Time Transfer Telemetry:** The Elm model contains only `transfer_status: String`. It lacks structured metrics for throughput velocity (`MB/s`), total vs transferred byte progress, percentage completion, rolling ETA estimation, and connection topology badges (`Local LAN`, `Direct P2P`, `Blind Relay`).

---

## 3. 5-Layer Behavioral Flow Trace (`/behavioral-audit`)

```
Layer 1: Entry Trigger       [ User Action / System Event ]
         │
Layer 2: UI Event Dispatch   [ Iced Message::* -> update(&mut self) ]
         │
Layer 3: Async Channel       [ tokio::sync::mpsc / iced::Subscription ]
         │
Layer 4: Tokio Background    [ Iroh QUIC / mDNS / Blake3 / Disk I/O ]
         │
Layer 5: State Sync & UI     [ Feedback / Progress / Toast / Dialog ]
```

### 3.1 Flow Trace Analysis

#### Primary Happy Path (File Selection & Ticket Generation)
- **Layer 1 (Entry):** User clicks the "Select File to Share" button in [`src/main.rs:101`](file:///d:/WebDev/rust/Qshare/src/main.rs#L101).
- **Layer 2 (Dispatch):** The widget emits `Message::SelectFile`. `Qshare::update()` matches the variant in [`src/main.rs:72-76`](file:///d:/WebDev/rust/Qshare/src/main.rs#L72-L76).
- **Layer 3 (Channel Bridge):** **DEAD END**. `update()` sets `self.transfer_status = "Ready to select file".to_string()` and logs an info trace. It executes no channel dispatch, no async command, and no dialog invocation.
- **Layer 4 (Tokio Engine):** **UNREACHABLE**. The background engine receives zero notifications.
- **Layer 5 (State Sync):** The view re-renders with static text `"Ready to select file"`. The flow terminates prematurely without selecting a file or generating a ticket.

#### Unwired CTAs & Dead Ends
1. **"Select File to Share" Button:** Completely unwired to `rfd::AsyncFileDialog` or engine channels.
2. **Ticket Input Field:** Completely absent from [`src/main.rs`](file:///d:/WebDev/rust/Qshare/src/main.rs). The user has no widget to paste an incoming `BlobTicket`.
3. **"Connect With Ticket" Action:** Completely absent. No mechanism exists to trigger WAN P2P connection dialing.
4. **"Cancel Transfer" Action:** Completely absent. The user has no way to abort an active transfer session.
5. **Peer Radar Nodes:** No interactive cards or click-to-send actions exist on the LAN tab.

#### Mock Leaks & Stubs
- `transfer_status: String` in [`src/main.rs:49`](file:///d:/WebDev/rust/Qshare/src/main.rs#L49) is a monolithic mock string simulating state without any underlying session data model (`UiTransferSession`, byte counts, or timers).
- Tab 1 body is an entirely static text widget (`text("Looking for nearby devices...")`).

#### Error Swallowing & Silent Failures
1. **Panic Hook Log Write:** In [`src/main.rs:10`](file:///d:/WebDev/rust/Qshare/src/main.rs#L10), the panic hook invokes:
   ```rust
   let _ = std::fs::write("qshare_crash.log", &msg);
   ```
   Using `let _ =` discards `std::io::Error`. If the application is launched from a read-only directory (such as `C:\Program Files\Qshare`), the write fails silently without logging.
2. **Launch Error Handler:** In [`src/main.rs:35`](file:///d:/WebDev/rust/Qshare/src/main.rs#L35), window launch failure similarly discards disk write errors with `let _ = std::fs::write(...)`.

#### UI Thread Blocking Hazards
1. **Synchronous Error Dialogs:** Lines 11–15 and 36–40 call `rfd::MessageDialog::new()...show()`. On Windows, `MessageDialog::show()` blocks the calling thread synchronously until the user clicks "OK". When executed inside the GUI event loop, this halts `wgpu` render passes and freezes all socket keepalives.
2. **Synchronous File Dialog Danger:** If `rfd::FileDialog::pick_file()` is called synchronously inside `update()`, the entire Windows event queue freezes while the Win32 Common Item Dialog is open. All incoming UDP hole-punching packets, mDNS broadcasts, and QUIC keepalive pings are dropped, causing active P2P connections to time out. All dialogs MUST use `rfd::AsyncFileDialog` or run inside `tokio::task::spawn_blocking`.

---

## 4. Headless Testability & TDD Strategy (`/test-driven-development`)

### 4.1 Decoupling & Testability Assessment
The current architecture in [`src/main.rs`](file:///d:/WebDev/rust/Qshare/src/main.rs) completely violates Principle 5 (*Decoupled Engine*) and the Headless TDD mandate:
1. **Monolithic Binary Root:** All structs (`Qshare`, `Tab`, `Message`) are defined inside `src/main.rs` with default private visibility (`struct Qshare`).
2. **Missing Library Crate (`src/lib.rs`):** The repository has no `src/lib.rs`. External integration tests located in `tests/` cannot import `Qshare` or test state transitions without compiling `main()`.
3. **Inability to Test State Transitions Without Window Surface:** Because the Elm state machine is tangled in `main.rs`, testing tab switching or progress calculations requires a full `iced::application` runtime requiring a GPU display adapter or headless software rasterizer.
4. **Missing Headless Harness:** There are zero unit tests (`#[test]`) or integration tests in the entire repository.

### 4.2 Required Headless Test Matrix

| Test Case Identifier | Target Behavior | Failure Injection / Stimulus | Expected Assertion |
| :--- | :--- | :--- | :--- |
| `TEST-1-01` | Tab State Persistence | Emit `Message::TabChanged(SendAnywhere)`, set ticket input, emit `Message::TabChanged(LocalLan)`. | Ticket input string is preserved verbatim across tab transitions. |
| `TEST-1-02` | Subscription Resiliency | Trigger subscription stream instantiation twice in succession on the same receiver handle. | Stream does not panic with `.take().expect()`; gracefully reports stream already bound. |
| `TEST-1-03` | Progress Event Throttling | Inject 1,000 `EngineEvent::TransferProgress` events in 10ms into the channel bridge. | UI receives throttled updates (~20 Hz) without memory bloat or dropping terminal `TransferFinished`. |
| `TEST-1-04` | Window Close Intercept | Emit window close event while `transfer_session` status is `Streaming`. | App does not terminate; transitions UI to `ConfirmAbortModal` and keeps engine streaming. |
| `TEST-1-05` | File Picker Cancellation | Return `None` from asynchronous file picker task. | `transfer_status` remains unaffected; no error toast or empty session created. |
| `TEST-1-06` | Clipboard Failure Grace | Mock Win32 `OpenClipboard` failure (error code `ACCESS_DENIED`). | UI displays inline fallback "Click to Copy" button; no panic or thread crash. |
| `TEST-1-07` | Bi-directional Unicode Sanitization | Inject filename containing RTL override (`\u{202E}`) and directory traversal (`../`). | UI display name strips control characters and traversal paths before rendering. |

---

## 5. Official Source Grounding & API Compliance (`/source-driven-development`)

### 5.1 Iced 0.13 Compliance
1. **`iced::application` Bootstrap:** In [`src/main.rs:22`](file:///d:/WebDev/rust/Qshare/src/main.rs#L22), calling `iced::application("Qshare (⚡)", Qshare::update, Qshare::view)` matches the modern Iced 0.13 standard.
2. **`update()` Signature & Command Dispatch:** In [`src/main.rs:66`](file:///d:/WebDev/rust/Qshare/src/main.rs#L66), `update(&mut self, message: Message)` returns `()`. In Iced 0.13, `update` must return `Task<Message>` (or `impl Into<Task<Message>>`) to dispatch asynchronous tasks. Returning `()` restricts the application to synchronous, pure in-memory state updates, completely preventing the dispatch of async dialogs, clipboard commands, or channel messages.
3. **Window Close Event Trapping:** In [`src/main.rs:27`](file:///d:/WebDev/rust/Qshare/src/main.rs#L27), `exit_on_close_request: true` instructs `winit` to terminate the process immediately upon clicking the close button ('X'). In Iced 0.13, intercepting the close request requires `exit_on_close_request: false` in `window::Settings` and subscribing to `iced::window::close_events()`.
4. **Drag-and-Drop Event Ingestion:** In Iced 0.13, widgets cannot natively intercept OS file drag-and-drop. Ingestion requires subscribing to `iced::event::listen()` and filtering for `iced::Event::Window(iced::window::Event::FileDropped(path))` and `FileHovered(path)`.

### 5.2 Iroh 1.3 & Tokio Compliance
1. **Channel Capacities & Cooperative Backpressure:** The proposed architecture uses `tokio::sync::mpsc::channel` with capacities 64 (UI $\rightarrow$ Engine) and 128 (Engine $\rightarrow$ UI). These bounds comply with Tokio cooperative scheduling guidelines, provided high-frequency progress telemetry is throttled before channel ingress.
2. **Missing `uuid` Dependency in `Cargo.toml`:** The architectural specification in `segment_specs.md` defines `UiTransferSession.session_id: uuid::Uuid` and calls `uuid::Uuid::new_v4()`. However, [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml) completely omits the `uuid` crate. Attempting to compile the proposed specification will immediately fail with `E0433: failed to resolve: use of undeclared crate or module uuid`.

---

## 6. Adversarial Doubt Gate: Real-World Field Failures (`/doubt-driven-development`)

> *"Under what real-world field conditions will this clean-looking code fail?"*

### 6.1 Field Failure Cross-Examination

#### 1. Windows 10/11 GPU Surface Loss & Display Disconnects
- **Field Scenario:** While a 15 GB transfer is in progress, the Windows laptop screen times out and enters display sleep, the user presses `Win + L` to lock the workstation, or an external 4K monitor is hot-unplugged. In Windows, this causes DirectX 12 / Vulkan swapchains to enter `SurfaceError::Lost` or `SurfaceError::Outdated`.
- **Failure Mode in Segment 1:** If the Iced/wgpu event loop crashes or if the Elm model is tied to surface lifecycle, the entire application dies, aborting the transfer.
- **Architectural Defense:** Iced 0.13 automatically handles swapchain recreation on surface loss. The Elm state machine in `QshareApp` must be decoupled from rendering resources, ensuring all active transfer states, tokens, and background channel listeners remain intact during swapchain rebuilds.

#### 2. Rapid Double-Clicking / Button Thrashing
- **Field Scenario:** A user rapidly double-clicks or triple-clicks "Select File to Share" or "Connect With Ticket".
- **Failure Mode in Segment 1:** 
  1. For file selection: If `update()` issues an asynchronous task on each click without checking a debounce flag, Windows spawns multiple concurrent file picker dialogs. When the user selects files in different dialogs, racing `FilePicked` events overwrite `self.selected_payload`, confusing the user and sending conflicting upload commands to Tokio.
  2. For ticket connection: Spawning multiple concurrent `ConnectTicketClicked` events commands the Iroh engine to dial the same `BlobTicket` across multiple parallel tasks, causing socket contention, wasted relay bandwidth, and split-brain session states.
- **Architectural Defense:** Introduce an explicit state guard: disable CTAs when `is_dialog_open == true` or `session_status == Negotiating`. Ignore repeated triggers in `update()` via immediate early-return.

#### 3. Windows Clipboard Lock Contention
- **Field Scenario:** When the sender generates an encrypted Share Ticket, Qshare invokes `iced::clipboard::write(ticket)`. Simultaneously, a clipboard manager (e.g. Windows Clipboard History `Win+V`, Ditto, 1Password, or Microsoft Office) holds an exclusive Win32 `OpenClipboard` lock.
- **Failure Mode in Segment 1:** On Windows, attempting to write to a locked clipboard returns an immediate OS error. If the application relies solely on the automatic copy action and provides no visual confirmation, the user pastes a stale clipboard string elsewhere, causing connection failures.
- **Architectural Defense:** Catch clipboard task results. If writing fails, display a clear, amber Toast warning: *"Clipboard busy: Click button to copy ticket manually"*, and render a persistent, selectable text box containing the ticket.

#### 4. Laptop Lid Closure, Sleep & Network Adapter Handover
- **Field Scenario:** User closes their laptop lid mid-transfer. Windows enters Connected Standby; the Wi-Fi adapter disconnects. Upon reopening the lid 5 minutes later, Windows connects to a different network or acquires a new IP.
- **Failure Mode in Segment 1:** The Tokio engine detects QUIC socket timeout or broken pipe. If Segment 1 drops the error event or fails to transition `SessionStatus` from `Streaming` to `Failed`, the UI remains frozen at the last byte count indefinitely with a static progress bar.
- **Architectural Defense:** The UI transfer session must maintain a heartbeat watchdog timer (`last_progress_timestamp: Instant`). If no progress or keepalive event is received from the engine for 8.0 seconds, the UI automatically transitions to `SessionStatus::Stalled` with an option for the user to retry or cancel.

#### 5. Malicious UI Spoofing via Bi-directional Unicode & Path Traversal
- **Field Scenario:** A malicious remote peer crafts a Share Ticket where the payload filename contains directory traversal sequences (`../../Windows/System32/cmd.exe`) or Right-to-Left Override Unicode control characters (`U+202E`, e.g. `document\u{202E}gnp.exe`, which displays as `documentexe.png`).
- **Failure Mode in Segment 1:** The UI renders the deceptive filename directly in the transfer drawer. The receiving user believes they are downloading an image and clicks "Open on Complete", executing malware.
- **Architectural Defense:** Segment 1 must enforce strict display sanitization: strip all path prefixes (`Path::file_name()`), filter out Unicode bi-directional control characters, and explicitly separate the displayed file extension from the base name with high-contrast styling.

---

## 7. Cross-Segment Cohesion & Ripple Impact Analysis

| Interacting Segment | Interface Seam | Ripple Risk / Failure Mode | Cohesion Rating |
| :--- | :--- | :--- | :---: |
| **Segment 2: `lan-mdns-radar`** | `EngineEvent::PeerDiscovered`, `EngineEvent::PeerLost` | High-frequency mDNS discovery churn on dense networks (e.g. 150 peers) saturates Iced render passes, causing frame stutter. | **VULNERABLE** |
| **Segment 3: `wan-iroh-transport`** | `AppCommand::CreateReceiveSession`, `EngineEvent::ConnectionEstablished(RouteBadge)` | UI sends unvalidated ticket strings; missing ticket syntax parser in UI causes engine panic or unhandled error cascades. | **VULNERABLE** |
| **Segment 4: `streaming-disk-io`** | `EngineEvent::TransferProgress`, `EngineEvent::TransferFinished` | **Progress Flooding & Terminal Event Loss:** Unthrottled chunk progress events saturate the 128-event channel, causing engine to drop critical `TransferFinished` events. UI hangs in `Streaming` forever. | **BROKEN** |
| **Segment 5: `security-resiliency`** | Panic Hook, Window Close Intercept, Crash Logging | Unsanitized relative path `qshare_crash.log` fails in Windows production installs; `exit_on_close_request` bypasses transfer teardown. | **VULNERABLE** |

### 7.1 Deadlock, Buffer Saturation & Terminal Event Loss Analysis
The most critical architectural vulnerability identified across the cross-segment boundary is **Progress Event Flooding**:
- **Mechanism:** During a 1 Gbps local Wi-Fi transfer with 128 KB chunks, the Segment 4 engine processes ~1,000 chunks per second. If the engine emits `EngineEvent::TransferProgress` on every chunk, 1,000 events/second enter the bounded `mpsc::channel(128)`.
- **Buffer Saturation:** The Iced UI thread renders at 60 FPS, draining at most 60 batches of events per second. The channel will instantly saturate within 150 milliseconds.
- **Terminal Event Starvation:** If the engine uses `try_send()` to avoid blocking the Tokio worker, it will drop events when the channel is full. If the terminal event `EngineEvent::TransferFinished` or `EngineEvent::EngineError` happens to be dropped, the UI will NEVER receive the completion signal. The transfer drawer will remain stuck in `Streaming` at 99.9% forever.
- **Prescription:** 
  1. The background engine MUST throttle progress events to a maximum of 20 Hz (every 50ms).
  2. The channel bridge MUST prioritize lifecycle events (`TransferFinished`, `EngineError`, `ConnectionEstablished`) over intermediate progress metrics, ensuring lifecycle events are never dropped.

---

## 8. Prioritized Findings & Architectural Recommendations

### 8.1 Findings Log

| Finding ID | Severity | Category | Affected File & Lines | Description | Impact |
| :--- | :---: | :---: | :--- | :--- | :--- |
| `F-1-01` | `[CRITICAL]` | Runtime Panic | `segment_specs.md:205-207` | **Runtime Panic in Subscription Stream:** The proposed `engine_subscription` uses `.take().expect("Engine receiver already claimed")` on an `Arc<Mutex<Option<Receiver>>>`. On stream re-evaluation or restart, `.take()` finds `None` and panics immediately. | Guaranteed application crash upon subscription stream refresh or reconnection. |
| `F-1-02` | `[CRITICAL]` | Channel Saturation | Architectural Seam (Seg 1 $\leftrightarrow$ Seg 4) | **Progress Event Flooding & Terminal Event Starvation:** Emitting unthrottled chunk progress into `mpsc::channel(128)` overflows the channel, dropping `TransferFinished` and hanging UI in `Streaming` indefinitely. | Transfers appear permanently frozen at 99% completion on high-speed networks. |
| `F-1-03` | `[HIGH]` | Compilation Blocker | [`Cargo.toml:11-39`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L11-L39) | **Missing `uuid` Dependency:** The specification defines `UiTransferSession.session_id: uuid::Uuid`, but `uuid` is absent from `Cargo.toml`. | Compilation failure (`undeclared crate or module uuid`) when implementing specs. |
| `F-1-04` | `[HIGH]` | Dead-End UI | [`src/main.rs:72-76`](file:///d:/WebDev/rust/Qshare/src/main.rs#L72-L76), [`L101`](file:///d:/WebDev/rust/Qshare/src/main.rs#L101) | **Unwired Interactive CTAs:** "Select File to Share" updates a local mock string without opening a native file picker or notifying the Tokio engine. | Core file transfer workflow is completely non-functional. |
| `F-1-05` | `[HIGH]` | UI Thread Freeze | [`src/main.rs:11-15`](file:///d:/WebDev/rust/Qshare/src/main.rs#L11-L15), [`L36-40`](file:///d:/WebDev/rust/Qshare/src/main.rs#L36-40) | **Synchronous Dialog Thread Blocking:** Calling `rfd::MessageDialog::show()` blocks the winit/wgpu thread synchronously. Any synchronous file dialog call stops frame pumps and drops UDP hole-punching packets. | 100% UI freeze, dropped network packets, and socket timeout drops. |
| `F-1-06` | `[HIGH]` | Data Loss | [`src/main.rs:27`](file:///d:/WebDev/rust/Qshare/src/main.rs#L27) | **Unconditional Exit on Close Request:** `exit_on_close_request: true` causes the OS close button ('X') to terminate the process instantly mid-transfer without confirmation, cleanup, or peer notification. | Corrupted `.part` files, remote peer hangs until QUIC idle timeout. |
| `F-1-07` | `[HIGH]` | Race Hazard | `src/app.rs` (Proposed) | **Missing CTA Debounce Guards:** Rapidly double-clicking "Select File" or "Connect" spawns concurrent OS dialogs and duplicate dialing tasks. | Conflicting upload selections, split-brain session states, and socket contention. |
| `F-1-08` | `[MEDIUM]` | Testability Blocker | [`src/main.rs:46-77`](file:///d:/WebDev/rust/Qshare/src/main.rs#L46-L77) | **Monolithic Inaccessibility:** Types (`Qshare`, `Tab`, `Message`) are private in `main.rs`. Lack of `src/lib.rs` prevents writing headless unit and integration tests. | Violates Principle 5; blocks automated headless CI test suites. |
| `F-1-09` | `[MEDIUM]` | Visual Craft | [`src/main.rs:84-89`](file:///d:/WebDev/rust/Qshare/src/main.rs#L84-L89) | **Indistinguishable Active Tab & Missing States:** Active and inactive tab buttons are visually identical. Missing loading, disabled, empty, and error states. | Degraded user experience, unclear application navigation state. |
| `F-1-10` | `[MEDIUM]` | Broken Promise | [`src/main.rs:99`](file:///d:/WebDev/rust/Qshare/src/main.rs#L99) | **Missing Drag-and-Drop Ingestion:** UI copy promises drag & drop, but window drag/drop events are unhandled. | Dropping files onto the window produces no action. |
| `F-1-11` | `[MEDIUM]` | Platform Fault | `src/app.rs` (Proposed) | **Windows Clipboard Lock Contention:** `iced::clipboard::write` fails when third-party clipboard managers hold exclusive locks; errors are unhandled. | User believes ticket was copied, pastes wrong string, connection fails. |
| `F-1-12` | `[LOW]` | Security / Spoofing | `src/ui/progress_modal.rs` (Proposed) | **Unsanitized Payload Display:** Displaying remote filenames without stripping traversal (`../`) or Bi-directional Unicode overrides (`U+202E`). | Phishing and UI spoofing vulnerability for receiving users. |
| `F-1-13` | `[LOW]` | Platform Fault | [`src/main.rs:10`](file:///d:/WebDev/rust/Qshare/src/main.rs#L10), [`L35`](file:///d:/WebDev/rust/Qshare/src/main.rs#L35) | **Relative Crash Log Path:** Writing to `qshare_crash.log` in the current working directory fails silently in read-only installation folders. | Crash reports are lost in production Windows deployments. |
| `F-1-14` | `[LOW]` | Diagnostics | [`src/main.rs:8-10`](file:///d:/WebDev/rust/Qshare/src/main.rs#L8-L10) | **Missing Stack Backtrace:** The panic hook captures `info` but omits `std::backtrace::Backtrace::force_capture()`. | Panic logs lack call stacks, severely complicating post-mortem debugging. |

---

### 8.2 Architectural Prescription (Actionable Blueprint)

#### Immediate Remediations (Pre-Implementation Mandate)
1. **Deconstruct Monolith into Decoupled Library Structure:**
   - Create [`src/lib.rs`](file:///d:/WebDev/rust/Qshare/src/lib.rs) exporting public modules: `pub mod app;`, `pub mod ui;`, and `pub mod core;`.
   - Move `QshareApp`, `Message`, and `Tab` into [`src/app.rs`](file:///d:/WebDev/rust/Qshare/src/app.rs).
   - Keep [`src/main.rs`](file:///d:/WebDev/rust/Qshare/src/main.rs) strictly as an entry bootstrap that invokes `qshare::run()`.
2. **Add Missing Dependencies to `Cargo.toml`:**
   - Add `uuid = { version = "1.10", features = ["v4", "serde"] }` and `dirs = "5.0"` (for canonical local app-data crash log paths).
3. **Fix Subscription Stream Pattern (Eliminate Panic Hazard `F-1-01`):**
   - Replace the fragile `Arc<Mutex<Option<Receiver>>>` with an asynchronous channel bridge that remains valid across subscription reconnects, or hold the sender in the engine and pass a cloned receiver factory.
4. **Implement Non-Blocking File Picker:**
   - In `update()`, execute `rfd::AsyncFileDialog::new().pick_file()` wrapped inside `Task::perform(..., Message::FilePicked)`. Return `Task<Message>` from `update()`.
5. **Intercept Window Close Requests (`F-1-06`):**
   - Configure `exit_on_close_request: false` in `window::Settings`.
   - Subscribe to `iced::window::close_events()`. If `self.transfer_session` is active, display a modal: *"Transfer in progress. Abort and quit?"*. If user confirms, emit `AppCommand::AbortSession`, clean up `.part` files, and invoke `iced::window::get_latest().and_then(iced::window::close)`.
6. **Implement Drag-and-Drop Event Listener (`F-1-10`):**
   - Attach `iced::event::listen()` to subscription batch, intercepting `iced::Event::Window(iced::window::Event::FileDropped(path))`, and routing to `Message::FilePicked(Some(path))`.
7. **Enforce Backpressure & Rate-Limiting on Engine Progress (`F-1-02`):**
   - Restrict progress telemetry in the engine to 20 Hz. Ensure completion and error events use separate un-droppable signals.
8. **Harden Crash Logging & Backtrace (`F-1-13`, `F-1-14`):**
   - Resolve crash log path to `%LOCALAPPDATA%\Qshare\logs\crash.log` using `dirs::data_local_dir()`.
   - Capture `std::backtrace::Backtrace::force_capture()` inside `std::panic::set_hook`.

#### Recommended Conventional Commit Message
```
refactor(ui): decouple elm state machine, non-blocking dialogs and subscription bridge

- Split monolithic main.rs into decoupled library crate (src/lib.rs, src/app.rs, src/ui/)
- Resolve runtime panic in engine_subscription by redesigning channel stream bridge
- Transition file picker to non-blocking rfd::AsyncFileDialog via iced::Task::perform
- Intercept window close requests with active transfer confirmation modal
- Add window drag-and-drop subscription listener for direct file drops
- Throttle progress events to prevent channel saturation and terminal event starvation
- Add uuid and dirs dependencies to Cargo.toml
- Standardize crash logging to local app data directory with forced backtraces
```