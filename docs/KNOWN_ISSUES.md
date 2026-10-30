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

*No historical issues recorded yet (Project in initial scaffolding phase).*
