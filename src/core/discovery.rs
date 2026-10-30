use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OsType {
    Windows,
    MacOS,
    Linux,
    Android,
    IOS,
    Unknown,
}

impl OsType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OsType::Windows => "windows",
            OsType::MacOS => "macos",
            OsType::Linux => "linux",
            OsType::Android => "android",
            OsType::IOS => "ios",
            OsType::Unknown => "unknown",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        let lower = s.to_ascii_lowercase();
        if lower.contains("win") {
            OsType::Windows
        } else if lower.contains("mac") || lower.contains("darwin") || lower.contains("osx") {
            OsType::MacOS
        } else if lower.contains("android") {
            OsType::Android
        } else if lower.contains("ios") {
            OsType::IOS
        } else if lower.contains("linux") {
            OsType::Linux
        } else {
            OsType::Unknown
        }
    }

    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            OsType::Windows
        }
        #[cfg(target_os = "macos")]
        {
            OsType::MacOS
        }
        #[cfg(target_os = "linux")]
        {
            OsType::Linux
        }
        #[cfg(target_os = "android")]
        {
            OsType::Android
        }
        #[cfg(target_os = "ios")]
        {
            OsType::IOS
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux",
            target_os = "android",
            target_os = "ios"
        )))]
        {
            OsType::Unknown
        }
    }
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Raw representation of a discovered LAN peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanPeer {
    pub node_id: String,
    pub display_name: String,
    pub os_type: OsType,
    pub addresses: Vec<SocketAddr>,
    pub service_port: u16,
    pub last_seen: Instant,
}

/// UI-facing presentation model for radar canvas rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanPeerState {
    pub node_id: String,
    pub display_name: String,
    pub os_type: OsType,
    pub primary_addr: SocketAddr,
    pub polar_angle_deg: f32,
    pub distance_ratio: f32,
    pub is_stale: bool,
}

impl LanPeer {
    /// Selects the best physical IPv4 address (filtering loopback/virtual IPs)
    /// and computes deterministic polar angle and distance ratio.
    pub fn to_ui_state(&self) -> Option<LanPeerState> {
        let primary_addr = select_best_address(&self.addresses, self.service_port)?;

        // Deterministic polar angle from Blake3 hash
        let hash = blake3::hash(self.node_id.as_bytes());
        let hash_bytes = hash.as_bytes();
        let angle_raw = u16::from_le_bytes([hash_bytes[0], hash_bytes[1]]) % 360;
        let polar_angle_deg = angle_raw as f32;

        // Fixed distance ratio for local LAN orbit
        let distance_ratio = 0.35f32;

        let is_stale = self.last_seen.elapsed() > Duration::from_secs(PeerTable::TTL_STALE_SECS);

        Some(LanPeerState {
            node_id: self.node_id.clone(),
            display_name: self.display_name.clone(),
            os_type: self.os_type,
            primary_addr,
            polar_angle_deg,
            distance_ratio,
            is_stale,
        })
    }
}

/// Selects the best physical address from a candidate list, preferring private/routable IPv4,
/// while allowing loopback (127.0.0.1) as lowest-rank fallback for local testing.
pub fn select_best_address(addresses: &[SocketAddr], service_port: u16) -> Option<SocketAddr> {
    let mut candidates: Vec<SocketAddr> = Vec::new();

    for &addr in addresses {
        let port = if service_port != 0 {
            service_port
        } else if addr.port() != 0 {
            addr.port()
        } else {
            5353
        };

        match addr.ip() {
            IpAddr::V4(v4) => {
                let octets = v4.octets();
                // Filter unspecified (0.0.0.0) or broadcast (255.255.255.255)
                if v4.is_unspecified() || v4.is_broadcast() {
                    continue;
                }
                // Filter link-local / APIPA (169.254.0.0/16)
                if octets[0] == 169 && octets[1] == 254 {
                    continue;
                }
                candidates.push(SocketAddr::new(IpAddr::V4(v4), port));
            }
            IpAddr::V6(v6) => {
                if v6.is_unspecified() {
                    continue;
                }
                candidates.push(SocketAddr::new(IpAddr::V6(v6), port));
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Rank candidates:
    // 0: RFC 1918 192.168.0.0/16
    // 1: RFC 1918 10.0.0.0/8
    // 2: RFC 1918 172.16.0.0/12
    // 3: Public / other routable IPv4
    // 4: Routable IPv6
    // 5: Loopback (127.0.0.0/8 or ::1)
    candidates.sort_by_key(|a| match a.ip() {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                5
            } else {
                let o = v4.octets();
                if o[0] == 192 && o[1] == 168 {
                    0
                } else if o[0] == 10 {
                    1
                } else if o[0] == 172 && (16..=31).contains(&o[1]) {
                    2
                } else {
                    3
                }
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                5
            } else {
                4
            }
        }
    });

    candidates.into_iter().next()
}

/// Thread-safe or local table maintaining active LAN peers and eviction lifecycles.
#[derive(Debug)]
pub struct PeerTable {
    peers: HashMap<String, LanPeer>,
    rate_limiter: HashMap<IpAddr, (u32, Instant)>,
}

impl PeerTable {
    pub const MAX_LAN_PEERS: usize = 256;
    pub const TTL_STALE_SECS: u64 = 8;
    pub const TTL_EVICT_SECS: u64 = 15;
    pub const MAX_PKTS_PER_SEC: u32 = 5;

    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            rate_limiter: HashMap::new(),
        }
    }

    /// Rate limits incoming packets per sender IP (< 5 pkts/sec).
    pub fn check_rate_limit(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();

        // Prune rate limiter if too many IPs to prevent memory exhaustion
        if self.rate_limiter.len() > 1024 {
            self.rate_limiter
                .retain(|_, (_, start)| now.duration_since(*start) < Duration::from_secs(2));
        }

        let entry = self.rate_limiter.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) >= Duration::from_secs(1) {
            *entry = (1, now);
            true
        } else if entry.0 < Self::MAX_PKTS_PER_SEC {
            entry.0 += 1;
            true
        } else {
            false
        }
    }

    /// Inserts or updates a peer in the table.
    /// Returns `(is_new, address_or_port_changed, Option<LanPeerState>)`.
    pub fn insert_or_refresh(&mut self, peer: LanPeer) -> (bool, bool, Option<LanPeerState>) {
        if let Some(existing) = self.peers.get_mut(&peer.node_id) {
            let addr_or_port_changed =
                existing.addresses != peer.addresses || existing.service_port != peer.service_port;
            let metadata_changed =
                existing.display_name != peer.display_name || existing.os_type != peer.os_type;

            existing.addresses = peer.addresses;
            existing.service_port = peer.service_port;
            existing.display_name = peer.display_name;
            existing.os_type = peer.os_type;
            existing.last_seen = peer.last_seen;

            let ui_state = existing.to_ui_state();
            (false, addr_or_port_changed || metadata_changed, ui_state)
        } else {
            if self.peers.len() >= Self::MAX_LAN_PEERS {
                self.evict_expired();
            }
            if self.peers.len() >= Self::MAX_LAN_PEERS {
                return (false, false, None);
            }

            let ui_state = peer.to_ui_state();
            self.peers.insert(peer.node_id.clone(), peer);
            (true, false, ui_state)
        }
    }

    /// Evicts expired peers whose last seen is >= 15 seconds.
    pub fn evict_expired(&mut self) -> Vec<String> {
        let now = Instant::now();
        let evict_ttl = Duration::from_secs(Self::TTL_EVICT_SECS);
        let mut expired = Vec::new();

        self.peers.retain(|node_id, peer| {
            let elapsed = now
                .checked_duration_since(peer.last_seen)
                .unwrap_or_default();
            if elapsed >= evict_ttl {
                expired.push(node_id.clone());
                false
            } else {
                true
            }
        });

        expired
    }

    /// Removes a peer by node_id immediately (e.g. upon Goodbye packet).
    pub fn remove_peer(&mut self, node_id: &str) -> Option<LanPeer> {
        self.peers.remove(node_id)
    }

    pub fn get(&self, node_id: &str) -> Option<&LanPeer> {
        self.peers.get(node_id)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    pub fn to_ui_states(&self) -> Vec<LanPeerState> {
        self.peers
            .values()
            .filter_map(|p| p.to_ui_state())
            .collect()
    }
}

impl Default for PeerTable {
    fn default() -> Self {
        Self::new()
    }
}
