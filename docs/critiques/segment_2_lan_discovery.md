# Architecture Critique: Segment 2 — Local Network Discovery & mDNS Radar Engine

**Document Version:** 1.0.0  
**Target:** Qshare Desktop Application ([`d:\WebDev\rust\Qshare`](file:///d:/WebDev/rust/Qshare))  
**Status:** Under Architectural Review  
**Evaluators:** Critic 1 (Investigator) & Critic 2 (Adversarial Doubt Gate)  
**Date:** 2026-10-30  

---

## 1. Executive Summary & Health Assessment

- **Segment Health Score:** 45/100
- **Summary:**  
  The `lan-mdns-radar` segment serves as Qshare's local discovery backbone. It is designed to provide zero-configuration, zero-cloud peer discovery across local Wi-Fi subnets via UDP multicast (`224.0.0.251:5353`, `_qshare._tcp.local.`), maintaining an active peer table with TTL heartbeat eviction and routing direct connections to local TCP/QUIC sockets.

  While the theoretical design aligns with the **Zero-Cloud Invariant (Principle 2)**, the specification and current codebase state suffer from foundational blockers. Most critically, the segment is **uncompilable as specified** due to undeclared direct dependencies in [`Cargo.toml`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L11-L39) (`socket2`, `thiserror`, `simple-dns`) and severe cross-segment type contract drift with Segment 1 (`LanPeer` vs `LanPeerState`). On the operating system level, Windows Winsock multicast binding fails with `WSAEADDRINUSE` (10048) or `WSAEACCES` (10013) when running alongside Apple Bonjour, Spotify, Chrome, or Windows 10/11 native DNS Client (`Dnscache`).

  Furthermore, adversarial field failure analysis reveals that multi-homed systems with WSL2 or active VPNs silently route outbound multicast into virtual blackholes, AP client isolation leaves users stranded on an uncommunicative empty radar screen, and naive packet ingestion on high-density subnets creates unbounded memory allocations and UI canvas stutter. Immediate architectural remediation is mandatory prior to writing implementation code in `src/core/lan.rs`.

- **Invariant Compliance Status:**
  - *Zero-Cloud Storage Invariant (0 MB Relay Disk):* **COMPLIANT** (Discovery is entirely peer-to-peer on LAN; no telemetry or metadata touches external cloud servers).
  - *Non-Blocking UI Thread Invariant:* **AT RISK** (Unbounded MPSC dispatch from multicast packet floods threatens to saturate Iced's Elm event loop; lack of scan lifecycle signals leaves the UI blind to socket failure).
  - *Atomic Disk Finalization Invariant:* **COMPLIANT** (This segment handles signaling and address resolution; disk persistence is quarantined to Segment 4).
  - *Rust Memory Safety & Pure Native Binary:* **DRIFT DETECTED** (Missing bounded capacities in `PeerTable` expose the binary to memory exhaustion DOS; raw DNS packet slice parsing without safe pointer loop protection introduces panic hazards).

---

## 2. Desktop UX & Visual Craft Audit (`/impeccable`)

### 2.1 Visual Hierarchy & Layout Architecture
The Nearby LAN Radar view in [`src/main.rs:91-97`](file:///d:/WebDev/rust/Qshare/src/main.rs#L91-L97) currently displays a single static text string:
```rust
text("Looking for nearby devices on your Wi-Fi router via mDNS (multicast)...")
```
When upgraded to the full animated Radar View specified in Segment 1:
1. **Radar Canvas Centering & Spacing:** The canvas must be enclosed in a responsive container with a fixed aspect ratio (1:1 circular sweep) bounded by a minimum diameter of $320\text{px}$ and a maximum diameter of $520\text{px}$ to preserve balance on $850 \times 650$ desktop windows.
2. **Concentric Orbit Hierarchy:** The radar view requires 3 distinct concentric guide rings:
   - **Inner Orbit ($r = 0.35$):** Local Subnet LAN peers (ultra-fast, zero-cloud switch routing).
   - **Middle Orbit ($r = 0.65$):** Direct WAN P2P peers (discovered via ticket or local fallback).
   - **Outer Orbit ($r = 0.90$):** Relayed peers (stateless fallback route).
3. **Contrast & Theme Integrity:** Pulsing blips must use high-contrast cyan/emerald hues (`#00E5FF` / `#00E676`) with semi-transparent decay trails (`rgba(0, 229, 255, 0.15)`) against the dark background (`#121214`).

### 2.2 Component State Matrix

| Component / Widget | Default | Hover / Focus | Active / Scanning | Disabled / Offline | Empty State (Isolation) | Error State (Collision) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Radar Canvas** | `[OK]` Dark target grid | `[MISSING]` Cursor change on peer | `[MISSING]` Sweeping beam animation | `[MISSING]` Grayscale static rings | `[MISSING]` Empty scan illustration | `[MISSING]` Socket collision warning |
| **Peer Blip Node** | `[MISSING]` Pulsing avatar | `[MISSING]` Tooltip with IP & OS | `[MISSING]` Ping halo | `[MISSING]` Fading/Stale decay | N/A | `[MISSING]` Unreachable badge |
| **Discovery Status Bar**| `[MISSING]` Idle indicator | N/A | `[MISSING]` "Scanning Wi-Fi..." | `[MISSING]` "Wi-Fi Disconnected" | `[MISSING]` "No peers found (5s)" | `[MISSING]` "Port 5353 in use" |
| **AP Isolation Banner** | `[MISSING]` Hidden | N/A | N/A | N/A | `[MISSING]` "Switch to Ticket" CTA | N/A |

### 2.3 Microcopy, Telemetry & User Feedback
1. **Ambiguous Discovery Terminology:** The spec and UI refer interchangeably to "mDNS", "multicast", "peers", and "nodes". The UI microcopy must translate networking mechanics into clear human terminology:
   - *Technical:* `mDNS broadcast on 224.0.0.251:5353 listening for _qshare._tcp.local.`
   - *User-Facing:* *"Searching your local Wi-Fi for nearby devices..."*
2. **Connection Quality & Interface Badging:** Discovered peers must display clear badges indicating interface topology:
   - `[⚡ Local Wi-Fi]` (Direct LAN switch connection: 50–100+ MB/s).
   - `[🌐 Internet P2P]` (Direct UDP Hole-Punched: 10–50 MB/s).
   - `[🛡️ Encrypted Relay]` (Relayed: 2–10 MB/s).
3. **Actionable Recovery Hints:** If multicast yields 0 peers after 5 seconds (hotel/airport Wi-Fi client isolation), the UI must not remain silent. It must render an actionable card:  
   *"Cannot see nearby devices? Your Wi-Fi network may have device isolation enabled. Click here to use a Send Anywhere Share Ticket."*

---

## 3. 5-Layer Behavioral Flow Trace (`/behavioral-audit`)

```
Layer 1: Entry Trigger       [ Remote peer joins Wi-Fi & broadcasts UDP 5353 ]
         │
Layer 2: Socket Ingestion    [ Multicast UDP Socket (0.0.0.0:5353 / SO_REUSEADDR) ]
         │
Layer 3: Parser & Filter     [ simple-dns packet decode -> verify _qshare._tcp -> drop self ]
         │
Layer 4: Peer Table & TTL    [ PeerTable::insert_or_refresh -> bounded capacity -> TTL reaper ]
         │
Layer 5: State Sync & UI     [ EngineEvent::PeerDiscovered -> iced::Subscription -> Radar Canvas ]
```

### 3.1 Flow Trace Analysis

#### Primary Happy Path
1. Remote peer initiates Qshare on the same subnet and broadcasts an mDNS response containing PTR (`_qshare._tcp.local.`), SRV (target hostname, port `4242`), TXT (`node_id=abc`, `os=win`, `v=1`), and A record (`192.168.1.105`).
2. Local Tokio background worker reads datagram from UDP socket bound to `0.0.0.0:5353` via `socket2` with `SO_REUSEADDR`.
3. Packet parser decodes DNS records using `simple-dns`, confirms service match `_qshare._tcp.local.`, validates that `node_id != local_node_id`, and extracts valid IPv4 addresses.
4. `PeerTable` registers peer, sets `last_seen = Instant::now()`, assigns deterministic polar coordinate angle, and emits `EngineEvent::PeerDiscovered(LanPeerState)`.
5. `iced::Subscription` receives event, updates `lan_peers`, triggers Canvas redraw, and displays pulsing avatar on Radar View.

#### Unwired CTAs & Dead Ends
1. **Unwired Discovery Switch:** In [`src/main.rs:85`](file:///d:/WebDev/rust/Qshare/src/main.rs#L85), clicking `"Nearby LAN Radar"` switches the local `active_tab` enum, but **never communicates with the engine** to initiate scanning or request active network query transmission.
2. **Missing Manual Refresh CTA:** If a user connects to Wi-Fi after launch, there is no UI mechanism or command (`LanCommand::ForceRefresh`) wired to trigger an active mDNS probe query.

#### Mock Leaks & Stubs
1. **Hardcoded Tab Text:** [`src/main.rs:93`](file:///d:/WebDev/rust/Qshare/src/main.rs#L93) hardcodes placeholder text instead of subscribing to dynamic peer table updates.
2. **Missing Distance Coordinate Calculation:** `LanPeerState.distance_ratio` is expected by the UI, but `LanPeer` in Segment 2 provides no calculation logic, leaving it as an ungrounded stub.

#### Error Swallowing & Silent Failures
1. **Silent Multicast Socket Drop:** If `socket.join_multicast_v4()` fails due to an invalid default gateway or missing interface, the spec catches the error into `LanError::JoinMulticastFailed`, but `LanDiscoveryService::start_listener` returns `Err`, halting the task silently with zero event emission to the UI.
2. **Ignored Stale Node Updates:** When a known peer changes IP addresses, `PeerTable::insert_or_refresh` returns `(false, peer)`. Layer 4 checks `if is_new`, completely discarding IP/port updates for existing peers.

#### UI Thread Blocking Hazards
1. **Synchronous Multicast Socket Creation on UI Thread:** If socket binding or interface enumeration were invoked inside `Qshare::update()` or `Subscription::batch()`, Winsock DNS resolution or network adapter queries could block the UI thread for 200–1000ms. All socket setup must remain strictly isolated inside background `tokio::spawn` tasks.

---

## 4. Headless Testability & TDD Strategy (`/test-driven-development`)

### 4.1 Decoupling & Testability Assessment
The `lan-mdns-radar` engine can and must be 100% decoupled from `Iced` widgets and window surfaces. All networking logic, packet encoding/decoding, peer table lifecycle, and TTL expiration must execute headlessly in `tokio::test` suites without linking `wgpu` or `winit`.

To prevent parallel test collision and port binding failures on CI runners (such as GitHub Actions where multicast routing is restricted):
- The `LanDiscoveryService` must accept an optional configuration struct `LanTestConfig { bind_override: Option<SocketAddr>, target_override: Option<SocketAddr>, loopback_only: bool }`.
- Tests must bind to ephemeral loopback addresses (`127.0.0.1:0`) and transmit unicast or loopback UDP frames.

### 4.2 Required Headless Test Matrix

| Test Case Identifier | Target Behavior | Failure Injection / Stimulus | Expected Assertion Criteria |
| :--- | :--- | :--- | :--- |
| `TEST-2-01` | DNS-SD Packet Encode & Decode | Raw byte buffer encoding PTR, SRV, TXT, A records via `simple-dns`. | `LanPeer` fields match original input; valid UTF-8 and IP parsing. |
| `TEST-2-02` | Self-Broadcast Packet Filtering | Inject packet containing `own_node_id` into packet reader pipeline. | Packet discarded immediately; `event_tx.try_recv()` returns `Err(Empty)`. |
| `TEST-2-03` | DNS Goodbye (TTL=0) Immediate Eviction | Inject packet with $\text{TTL}=0$ for an active known `node_id`. | Node removed from `PeerTable`; `EngineEvent::PeerLost(node_id)` emitted with zero delay. |
| `TEST-2-04` | Virtual Time TTL Deadline Reaper | Insert peer; advance virtual time via `tokio::time::advance(Duration::from_secs(16)).await`. | Background reaper detects expiration; emits `EngineEvent::PeerLost`; table count drops to 0. |
| `TEST-2-05` | IP / Port Dynamic Mutation | Inject announcement for existing `node_id` with new port `5555` and IP `192.168.1.200`. | `PeerTable` updates socket addresses; emits `EngineEvent::PeerUpdated`. |
| `TEST-2-06` | Malformed & Truncated Frame Resilience | Inject 0-byte packet, truncated DNS header (6 bytes), and cyclic compression pointer (`0xC0 0x00`). | Parser logs warning; returns `LanError::MalformedPacket`; listener loop continues running without panic. |
| `TEST-2-07` | PeerTable Memory Bounding (`MAX_LAN_PEERS`)| Flood table with 1,000 distinct randomized `node_id` announcements. | Total table size strictly capped at `256`; surplus insertions rejected without crashing. |
| `TEST-2-08` | Ephemeral Unicast Query Fallback | Simulate port 5353 collision; bind to ephemeral port `0.0.0.0:0` and transmit active QU query. | Listener successfully binds; sends PTR query with QU bit set; parses unicast response. |

---

## 5. Official Source Grounding & API Compliance (`/source-driven-development`)

### 5.1 Winsock & POSIX Multicast Socket Semantics (`socket2 0.5`)
Binding multicast UDP sockets across platforms requires strict adherence to OS-specific socket option semantics:

1. **Windows Winsock Quirks:**
   - On Windows, binding directly to `224.0.0.251:5353` fails with `WSAEADDRNOTAVAIL` (10049) or `WSAEINVAL` (10022). Windows requires binding to `0.0.0.0:5353` (`Ipv4Addr::UNSPECIFIED`), then issuing `join_multicast_v4(&224.0.0.251, &Ipv4Addr::UNSPECIFIED)`.
   - `SO_REUSEPORT` does **not exist** on Windows. Attempting to set `socket.set_reuse_port(true)` will fail compilation or return an OS error. Only `socket.set_reuse_address(true)` is supported.
   - If another process on Windows (e.g. anti-virus or VPN client) binds port 5353 with `SO_EXCLUSIVEADDRUSE`, `bind()` fails with `WSAEACCES` (10013).
2. **Cross-Platform Socket Construction Blueprint:**
   ```rust
   use socket2::{Socket, Domain, Type, Protocol};
   use std::net::SocketAddr;

   pub fn create_multicast_socket(addr: SocketAddr) -> std::io::Result<Socket> {
       let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
       let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

       socket.set_nonblocking(true)?;
       socket.set_reuse_address(true)?;

       #[cfg(unix)]
       {
           socket.set_reuse_port(true)?;
       }

       // On Windows, bind to 0.0.0.0:5353
       #[cfg(windows)]
       let bind_addr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), addr.port());
       #[cfg(not(windows))]
       let bind_addr = addr;

       socket.bind(&bind_addr.into())?;
       Ok(socket)
   }
   ```

### 5.2 RFC 6762 & RFC 6763 DNS-SD Compliance
1. **RFC 6762 §5.4 (QU / QM Question Types):** Unicast-response queries (QU bit = `0x8000` in Question Class) allow clients bound to ephemeral ports to receive direct unicast responses from peers, bypassing port 5353 collision limitations.
2. **RFC 6762 §10.1 (Goodbye Records):** When a service is terminating, it sends a multicast announcement with $\text{TTL} = 0$. The receiving peer must treat this as immediate invalidation rather than waiting for the 15-second heartbeat deadline.
3. **RFC 6763 §6 (TXT Record Constraints):** Key-value attributes must be formatted as `<len><key>=<val>`. Total TXT payload must remain below 400 bytes to prevent IP packet fragmentation over standard 1500-byte MTU Wi-Fi links.

---

## 6. Adversarial Doubt Gate: Real-World Field Failures (`/doubt-driven-development`)

> *"Under what real-world field conditions will this clean-looking code fail?"*

### 6.1 Field Failure Cross-Examination

#### 1. Network Topology Adversity (Wi-Fi Client Isolation & IGMP Snooping Drops)
- **Field Condition:** In hotels, airports, coffee shops, universities, and enterprise guest networks, access points enforce **Client Isolation** (Layer 2 station-to-station traffic blocked) or enable aggressive **IGMP Snooping**.
- **Failure Mode:** Inbound and outbound UDP 5353 multicast frames are dropped silently by the wireless controller. Neither node receives discovery packets.
- **Doubt Gate Cross-Examination:** If Qshare relies solely on passive mDNS listening, the user is permanently stranded on an empty radar screen.
- **Architectural Defense:**
  - Implement a 5-second discovery settling timer.
  - If zero peers are discovered after 5 seconds, surface an actionable prompt:  
    *"Can't find nearby devices? This Wi-Fi router blocks local device discovery. Switch to a Send Anywhere Share Ticket."*
  - To prevent IGMP Snooping dropouts on managed networks, transmit an active mDNS query probe every 10 seconds to refresh switch membership tables.

#### 2. OS & Host Adversity (Windows Defender Silent UDP Block & Port 5353 Collisions)
- **Field Condition:**
  - Apple Bonjour (`mDNSResponder.exe`) is installed via iTunes or Adobe Creative Cloud.
  - Windows Defender Firewall is set to "Public Network" (the default for new Wi-Fi connections).
- **Failure Mode:**
  - If Bonjour binds with `SO_EXCLUSIVEADDRUSE`, Qshare's `bind(0.0.0.0:5353)` fails with `WSAEACCES` (10013).
  - Even if bound, Windows Defender Firewall silently drops incoming UDP 5353 packets without surfacing a prompt or error (KI-001).
- **Doubt Gate Cross-Examination:** If port 5353 fails to bind, does the app crash or remain silent? If fallback binds to an ephemeral port (`0.0.0.0:0`), how does it receive unsolicited multicast announcements?
- **Architectural Defense:**
  - Implement an **Active Query Fallback Mode**: When binding port 5353 fails, bind to an ephemeral port (`0.0.0.0:0`), join the multicast group `224.0.0.251`, and transmit active queries with the **QU (Unicast Response)** bit set every 5 seconds. Compliant mDNS responders will send unicast UDP replies directly back to Qshare's ephemeral port.
  - Detect zero inbound packets after 10 seconds of active scanning and surface a non-intrusive notification: *"Windows Firewall may be restricting local discovery. Check network profile settings."*

#### 3. Adapter Adversity (Multi-Homed Routing Blackholes: WSL2, VirtualBox, Tailscale, VPNs)
- **Field Condition:** The host computer has active virtual network adapters: WSL2 (`vEthernet (WSL)`), Hyper-V, VirtualBox Host-Only, or a VPN tunnel (Tailscale/WireGuard).
- **Failure Mode:** Outbound multicast packets to `224.0.0.251` egress via the interface with the lowest Windows routing metric (frequently the WSL2 virtual switch or VPN interface). The physical Wi-Fi network receives zero announcements.
- **Doubt Gate Cross-Examination:** Standard Rust `std::net::UdpSocket` binds to an IP but does not bind multicast transmission to specific physical network adapters.
- **Architectural Defense:**
  - Enumerate all physical, non-loopback, multicast-capable IPv4 network interfaces using `if-addrs` or `netwatch`.
  - For every detected physical network interface, explicitly invoke `socket.set_multicast_if_v4(&interface_ip)`.
  - Transmit announcements across all active physical adapters to guarantee delivery to the local Wi-Fi subnet.

#### 4. High-Density Subnet Adversity (Multicast Storms & Denial of Service)
- **Field Condition:** A university campus network or large conference venue has 500+ mDNS-emitting devices (Apple TV, Chromecasts, printers, smart TVs) broadcasting tens of packets per second. Or a buggy/malicious client broadcasts 50,000 randomized peer announcements.
- **Failure Mode:**
  - Unbounded `PeerTable` allocations consume hundreds of megabytes of RAM.
  - The UDP reader blocks while awaiting UI channel capacity (`event_tx.send().await`), dropping socket packets.
  - The UI thread is flooded with hundreds of `PeerDiscovered` events per second, causing GPU canvas stutter.
- **Architectural Defense:**
  - **Pre-Parser Filter:** Check packet payload for `_qshare` substring before executing full DNS decompression or allocation. Discard foreign service types immediately.
  - **Table Capacity Bound:** Enforce `pub const MAX_LAN_PEERS: usize = 256;`. Drop new insertions when capacity is reached.
  - **Sender Rate Limiter:** Cap packet processing per source IP ($< 5\text{ packets/sec}$).
  - **UI Emission Throttle:** Coalesce peer updates; dispatch UI events at a maximum rate of 10 Hz.

---

## 7. Cross-Segment Cohesion & Ripple Impact Analysis

| Interacting Segment | Interface Seam | Ripple Risk / Failure Mode | Cohesion Rating |
| :--- | :--- | :--- | :---: |
| **Segment 1**<br>*(Desktop UI Shell & Radar)* | `EngineEvent::PeerDiscovered`<br>`LanPeerState` struct | **CRITICAL TYPE DESYNC:** Segment 1 requires `LanPeerState { distance_ratio: f32, os_type: String }`, whereas Segment 2 produces `LanPeer { addresses: Vec<SocketAddr>, os_type: OsType }`. Compilation fails immediately. | **BROKEN** |
| **Segment 3**<br>*(WAN P2P & Iroh Transport)* | AP Isolation Fallback /<br>Ticket Redirection | If mDNS fails or yields 0 peers, the UI must smoothly transition to the Send Anywhere Ticket input without losing user selection or state. | **VULNERABLE** |
| **Segment 4**<br>*(Streaming & Direct Socket)* | `LanPeer.addresses`<br>`service_port: u16` | If Segment 2 provides an unreachable address (e.g. WSL2 virtual IP) or if incoming TCP/QUIC is blocked by Windows Firewall, Segment 4 direct streaming hangs or times out. | **VULNERABLE** |
| **Segment 5**<br>*(Security & Windows Hardening)* | Port 5353 Socket Binding /<br>Windows Defender Alerts | Missing firewall rule heuristics in Segment 2 forces user confusion if inbound discovery is silently blocked. | **VULNERABLE** |

### 7.1 Deadlock & Buffer Overflow Analysis
- **Engine Event Channel (`capacity = 128`):**  
  In [`segment_specs.md:398`](file:///C:/Users/ONE/.gemini/antigravity/brain/31fb8f10-2d0e-4adb-a249-e06b45479667/scratch/segment_specs.md#L398), `event_tx` has a capacity of 128. If `LanDiscoveryService` invokes `event_tx.send().await` inside the UDP listener loop, a multicast burst will block the entire listener task when the channel fills.
  - *Remediation:* Use `event_tx.try_send()`. If full, log a warning and discard intermediate updates. Never allow the UDP socket ingestion loop to await a channel send.

### 7.2 State Desync Scenarios
- **Stale Socket Address Freeze:** When a peer transitions from Wi-Fi to Ethernet or receives a renewed DHCP lease, its IP changes. Because `insert_or_refresh` only notified the UI when `is_new == true`, the UI and Segment 4 direct streamer retained the dead IP address, causing subsequent file transfer drops.
- **Zombie Peer Persistence:** Closing a remote peer without an explicit DNS Goodbye ($\text{TTL}=0$) leaves its avatar visible on the radar for 15 seconds. If the user initiates a transfer at second 14, the transfer fails immediately.

---

## 8. Prioritized Findings & Architectural Recommendations

### 8.1 Findings Log

| Finding ID | Severity | Category | Target File(s) | Description | Impact |
| :--- | :---: | :--- | :--- | :--- | :--- |
| **F-2.1** | `[CRITICAL]` | Behavioral / OS | `src/core/lan.rs`<br>[`Cargo.toml:11-39`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L11-L39) | Port 5353 collision on Windows (`WSAEADDRINUSE` / `WSAEACCES`). Direct binding to `224.0.0.251:5353` fails on Winsock. | Multicast listener crashes on launch if Bonjour, Spotify, or Chrome is running. |
| **F-2.2** | `[CRITICAL]` | Build / Source | [`Cargo.toml:11-39`](file:///d:/WebDev/rust/Qshare/Cargo.toml#L11-L39) | Undeclared direct dependencies in `Cargo.toml`: `socket2`, `thiserror`, `simple-dns`. | `cargo check` and `cargo test` fail with unresolved import errors (`E0432`). |
| **F-2.3** | `[CRITICAL]` | Architecture | `src/core/peer.rs`<br>[`src/main.rs:46-65`](file:///d:/WebDev/rust/Qshare/src/main.rs#L46-L65) | Cross-segment type contract drift between `LanPeer` and `LanPeerState` (`OsType` vs `String`, missing `distance_ratio`, address mismatch). | Engine events cannot be consumed by UI; compiler type mismatch error. |
| **F-2.4** | `[CRITICAL]` | Behavioral / DOS | `src/core/discovery.rs`<br>`src/core/lan.rs` | Unbounded `PeerTable` allocations and blocking `event_tx.send().await` in UDP listener. | Memory exhaustion during multicast floods; UDP socket buffer overflow. |
| **F-2.5** | `[HIGH]` | Networking | `src/core/lan.rs` | Multi-homed interface routing blackhole with WSL2, Hyper-V, and VPN adapters. | Outbound multicast announcements never reach physical Wi-Fi peers. |
| **F-2.6** | `[HIGH]` | Data-flow | `src/core/discovery.rs` | Stale peer metadata freeze: IP and port updates ignored if `is_new == false`. | Direct transfers fail after peer changes IP or renews DHCP lease. |
| **F-2.7** | `[HIGH]` | Protocol / UX | `src/core/lan.rs`<br>`src/core/discovery.rs` | Missing DNS Goodbye ($\text{TTL}=0$) record handling and missing Goodbye broadcast on shutdown. | Closed peers linger for 15s on radar; transfers dropped on ghost nodes. |
| **F-2.8** | `[HIGH]` | UX / Impeccable | `src/core/lan.rs`<br>[`src/main.rs:65-77`](file:///d:/WebDev/rust/Qshare/src/main.rs#L65-L77) | Missing scan lifecycle signals (`DiscoveryStatusChanged`: `Idle`, `Scanning`, `Degraded`, `Error`). | UI displays active sweeping radar when socket is dead or Wi-Fi is disabled. |
| **F-2.9** | `[HIGH]` | TDD | `tests/lan_test.rs` | Tautological unit tests testing string equality instead of real DNS packet roundtripping. | 0% real test coverage for mDNS encoding, decoding, and filtering. |
| **F-2.10**| `[HIGH]` | Security / Source | `src/core/lan.rs` | DNS pointer compression loop hazard (`0xC0`) and out-of-bounds slice parsing panic risk. | Thread lockup or panic crash on malformed or malicious network datagrams. |
| **F-2.11**| `[MEDIUM]` | UX / Math | `src/core/peer.rs` | Undefined `distance_ratio` causing radar coordinate jitter on repeated announcements. | Node blips jump randomly across radar canvas circles on every heartbeat. |
| **F-2.12**| `[MEDIUM]` | UX / Impeccable | `src/app.rs`<br>[`src/main.rs:84-97`](file:///d:/WebDev/rust/Qshare/src/main.rs#L84-L97) | Unhandled empty state on AP client isolation: no settling timer before offering ticket fallback. | User stranded staring at empty radar on hotel/airport Wi-Fi networks. |
| **F-2.13**| `[MEDIUM]` | UX / Impeccable | `src/core/discovery.rs` | Lack of intermediate decaying peer state (`Active` $\rightarrow$ `Stale` $\rightarrow$ `Evicted`). | Peer remains fully active until 15s, then vanishes abruptly; connection drops on click. |
| **F-2.14**| `[MEDIUM]` | Networking | `src/core/lan.rs` | Missing IGMP Membership report refresh causing switches to prune multicast stream. | Discovery stops working after 2–5 minutes on enterprise/managed Wi-Fi. |
| **F-2.15**| `[MEDIUM]` | Networking / LAN | `src/core/discovery.rs` | Advertised addresses include virtual adapter IPs (WSL2 / APIPA `169.254.x.x`). | Segment 4 attempts connection to unreachable virtual IP instead of Wi-Fi IP. |
| **F-2.16**| `[LOW]` | Protocol / Standards | `src/core/lan.rs` | Unbounded TXT record size exceeding 512 bytes causing IP packet fragmentation. | Announcements dropped on low-MTU Wi-Fi routers. |

---

### 8.2 Architectural Prescription (Actionable Blueprint)

#### Immediate Remediations (Prior to Implementation)
1. **Update `Cargo.toml`:** Declare required direct dependencies:
   ```toml
   [dependencies]
   socket2 = "0.5"
   thiserror = "2.0"
   simple-dns = "0.7"
   if-addrs = "0.13"
   ```
2. **Align Cross-Segment Contract (`LanPeer` $\rightarrow$ `LanPeerState`):**
   Implement an explicit, deterministic mapping:
   ```rust
   impl LanPeer {
       pub fn to_ui_state(&self, local_node_id: &str) -> LanPeerState {
           // Deterministic polar angle from Blake3 hash
           let hash = blake3::hash(self.node_id.as_bytes());
           let hash_bytes = hash.as_bytes();
           let angle_deg = u16::from_le_bytes([hash_bytes[0], hash_bytes[1]]) % 360;
           
           // Discrete concentric orbit based on connection type
           let distance_ratio = 0.35; // Local Subnet LAN orbit

           LanPeerState {
               node_id: self.node_id.clone(),
               device_name: self.hostname.clone(),
               addr: self.preferred_ipv4().unwrap_or(self.addresses[0]),
               os_type: self.os_type,
               distance_ratio,
               angle_degrees: angle_deg as f32,
               status: self.lifecycle_status(),
           }
       }
   }
   ```
3. **Implement Robust Winsock Socket Binding & Active Query Fallback:**
   - On Windows, bind to `0.0.0.0:5353` with `SO_REUSEADDR`, then join `224.0.0.251`.
   - If binding fails (`WSAEADDRINUSE` / `WSAEACCES`), bind to `0.0.0.0:0` and enter **Active Polling Mode**, sending PTR queries with the QU bit set every 5 seconds.
4. **Implement Multi-Homed Physical Interface Enumeration:**
   - Use `if-addrs` to enumerate physical, non-loopback IPv4 interfaces.
   - For each physical interface, set `socket.set_multicast_if_v4(&ip)` when transmitting announcements.
5. **Enforce Capacity Bounds & Backpressure:**
   - Define `pub const MAX_LAN_PEERS: usize = 256;`.
   - Use `event_tx.try_send()` to avoid blocking the UDP listener task.
   - Implement rate-limiting per sender IP ($< 5\text{ packets/sec}$).
6. **Implement 3-Tier Lifecycle & DNS Goodbye Eviction:**
   - `Active` ($< 8\text{s}$): Full opacity, pulsing radar blip.
   - `Stale` ($8\text{s} - 15\text{s}$): Dimmed blip, warning tooltip.
   - `Evicted` ($> 15\text{s}$ or $\text{TTL} = 0$): Immediate `EngineEvent::PeerLost` and removal.
7. **Address Sanitization for Segment 4 Handshake:**
   - Filter `addresses` to discard loopback (`127.0.0.0/8`) and APIPA (`169.254.0.0/16`).
   - Match remote peer subnet against local subnet before returning preferred `SocketAddr`.

#### Refactoring Strategy
- Place packet encoding, parsing, and socket creation in `src/core/lan.rs`.
- Place `PeerTable`, rate-limiting, and lifecycle reaper in `src/core/discovery.rs`.
- Place `LanPeer`, `OsType`, and `to_ui_state` converter in `src/core/peer.rs`.
- Wire `iced::Subscription` in `src/app.rs` to receive `EngineEvent::PeerDiscovered`, `EngineEvent::PeerUpdated`, `EngineEvent::PeerLost`, and `EngineEvent::DiscoveryStatusChanged`.

#### Recommended Conventional Commit Message
```
refactor(lan): harden mDNS discovery against Winsock port collisions and multi-homed routing blackholes

- Add socket2, thiserror, simple-dns, and if-addrs dependencies
- Align LanPeer and LanPeerState contracts with deterministic polar coordinates
- Support SO_REUSEADDR binding on 0.0.0.0:5353 with ephemeral QU query fallback
- Enforce MAX_LAN_PEERS=256 capacity cap and try_send backpressure
- Handle RFC 6762 TTL=0 Goodbye packets for instant peer eviction
- Enumerate physical network interfaces to resolve WSL2/VPN multicast routing blackholes
- Filter non-routable APIPA/virtual addresses prior to Segment 4 streaming
```
