# Qshare Architecture Specification

**Author:** Jabir Abdullah Haian & Antigravity  
**Version:** 0.1.0-alpha  
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
- **Transport:** QUIC over UDP (leveraging TLS 1.3 encryption natively).
- **Rendezvous Coordination:**
  1. Sender generates an ephemeral cryptographic keypair (Ed25519) and forms an `iroh::Ticket`.
  2. The ticket contains:
     - `NodeId` (Public key of the sender).
     - Rendezvous relay coordinates.
     - Content hash of the file payload (BLAKE3).
  3. The receiver inputs the ticket string into Qshare.
  4. Both peers ping the rendezvous signaling relay via STUN to discover their respective external public socket mappings (`Public IP:Port`).
  5. The relay swaps these external coordinates and exits the data path.
  6. Both peers simultaneously send probe packets to each other's mapped endpoints.
  7. Home and standard NAT firewalls map this incoming packet to the existing outbound state table, opening a direct bi-directional UDP tunnel (**UDP Hole Punch**).

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
| **Eavesdropping on Transfer** | Wi-Fi packet sniffing or ISP snooping | All transfers (both LAN and WAN) use end-to-end TLS 1.3 / ChaCha20-Poly1305 encryption. |
| **Man-in-the-Middle (MitM)** | Rogue signaling or relay node | Peer endpoints authenticate each other using cryptographic Ed25519 node public keys embedded in the Share Ticket. |
| **Payload Tampering** | Bit flips, MITM injection, or packet corruption | Real-time BLAKE3 cryptographic hash verification. Corrupt chunks are rejected. |
| **Cloud Data Leakage / Subpoena** | Server seizure or cloud breach | **Zero Cloud Storage**: No user data or metadata is ever saved on servers. |
| **Disk Overwrite Hijack** | Malicious peer sending `../../Windows/System32/file` | Strict path sanitization: Qshare isolates incoming files to a designated Downloads directory and strips directory traversal paths. |
