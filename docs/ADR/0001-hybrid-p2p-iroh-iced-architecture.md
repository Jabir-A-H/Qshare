# ADR 0001: Hybrid P2P Architecture via Iroh and Iced

## Status
Accepted

## Context
File sharing applications typically face a severe architectural trade-off:
1. **Cloud-Mediated Storage (WeTransfer, Google Drive, SendAnywhere):** High infrastructure cost, monthly server bills, privacy exposure, and arbitrary file-size caps.
2. **Local-Only Sharing (LocalSend, Apple AirDrop):** Fast and private, but completely fails when two devices are on different networks, cellular data, or across different cities.
3. **Traditional WebRTC P2P (Snapdrop, PairDrop):** Subject to browser memory constraints, poor file streaming performance (> 2 GB failures), and browser tab sleep.

We require a solution that delivers:
- **Zero Cloud Storage:** No user file bytes or metadata stored on servers.
- **Universal Reach:** Works seamlessly across local Wi-Fi and the public internet across NAT firewalls.
- **Pure Native Rust Stack:** High throughput, tiny idle RAM, native GPU UI rendering, and zero WebViews.

## Decision
We adopt **Option A: Pure Rust Hybrid Dropzone utilizing `Iroh` and `Iced`**:

1. **GUI Layer (`Iced`):**
   - Renders natively using `wgpu` (DirectX 12 / Vulkan).
   - Adopts the Elm Architecture (`State`, `Message`, `update`, `view`) for rock-solid UI state isolation.
   - Communicates with the async background engine via `iced::Subscription`.

2. **P2P Transport Engine (`Iroh`):**
   - Implements QUIC over UDP with native TLS 1.3 encryption.
   - Automates STUN address discovery, UDP hole-punching, and stateless relay fallback.
   - Encodes connection coordinates into compact, self-contained Share Tickets.

3. **Content Verification (`BLAKE3`):**
   - Provides verified streaming of bounded chunks (64 KB – 256 KB) with incremental verification against `.part` temporary files.

## Consequences

### Positive
- **Zero Server Storage Bills:** The cloud is used exclusively for lightweight ephemeral address swapping (kilobytes of signaling).
- **High Success Rate:** Direct UDP hole-punching succeeds on ~90% of standard home/business internet connections.
- **Strong Privacy & Security:** End-to-end encrypted by default; neither intermediate ISPs nor relays can decrypt file contents.
- **Native Efficiency:** Instant app startup with GPU acceleration and negligible memory consumption.

### Negative / Trade-offs
- **Async Runtime Bridging:** Bridging `tokio` multi-threaded worker pools with `Iced`'s event subscription loop requires disciplined channel management.
- **Firewall Heuristics:** Inbound UDP ports on Windows may require automatic failover detection when Windows Defender drops unacknowledged packets.
