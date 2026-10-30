use iroh::endpoint::{Connection, Endpoint, presets};
use iroh::{EndpointAddr, EndpointId, RelayUrl};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

use super::ticket::{QshareTicket, TicketError};

/// Application-Layer Protocol Negotiation (ALPN) token for Qshare.
pub const QSHARE_ALPN: &[u8] = b"/qshare/1.0";

/// Connection routing classification for UX telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteType {
    /// Direct connection over local network / Wi-Fi subnet.
    LocalLan,
    /// Direct peer-to-peer connection via UDP hole-punching.
    DirectP2P,
    /// End-to-end encrypted relay fallback (stateless RAM transit, 0 MB disk).
    BlindRelay,
}

impl std::fmt::Display for RouteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalLan => write!(f, "Local LAN"),
            Self::DirectP2P => write!(f, "Direct P2P"),
            Self::BlindRelay => write!(f, "Blind Relay"),
        }
    }
}

/// Connection route badge with network topology details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteBadge {
    LocalLan(SocketAddr),
    DirectP2P { remote_node: String, rtt_ms: u32 },
    BlindRelay { relay_url: String, latency_ms: u32 },
}

impl std::fmt::Display for RouteBadge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalLan(addr) => write!(f, "Local LAN ({addr})"),
            Self::DirectP2P {
                remote_node,
                rtt_ms,
            } => {
                write!(f, "Direct P2P ({remote_node}, {rtt_ms}ms)")
            }
            Self::BlindRelay {
                relay_url,
                latency_ms,
            } => write!(f, "Blind Relay ({relay_url}, {latency_ms}ms)"),
        }
    }
}

impl From<&RouteBadge> for RouteType {
    fn from(badge: &RouteBadge) -> Self {
        match badge {
            RouteBadge::LocalLan(_) => RouteType::LocalLan,
            RouteBadge::DirectP2P { .. } => RouteType::DirectP2P,
            RouteBadge::BlindRelay { .. } => RouteType::BlindRelay,
        }
    }
}

/// Checks whether an IP address belongs to a local, private, loopback, or link-local subnet.
pub fn is_local_lan(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_local_ipv4(v4),
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                is_local_ipv4(&v4)
            } else {
                let octets = v6.octets();
                // Check IPv4-compatible (::x.x.x.x) or NAT64 (64:ff9b::x.x.x.x)
                if (octets[..12] == [0; 12]
                    && (octets[12] != 0 || octets[13] != 0 || octets[14] != 0 || octets[15] != 0))
                    || (octets[..4] == [0x00, 0x64, 0xff, 0x9b] && octets[4..12] == [0; 8])
                {
                    let v4 =
                        std::net::Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
                    is_local_ipv4(&v4)
                } else {
                    v6.is_loopback()
                        || (octets[0] & 0xfe) == 0xfc
                        || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
                        || octets == [0; 16]
                }
            }
        }
    }
}

fn is_local_ipv4(v4: &std::net::Ipv4Addr) -> bool {
    let octets = v4.octets();
    v4.is_loopback()
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
        || (octets[0] == 169 && octets[1] == 254)
        || octets[0] == 0
}

/// Classifies an Iroh `TransportAddr` into a user-facing `RouteType`.
pub fn classify_transport_addr(addr: &iroh::TransportAddr) -> RouteType {
    match addr {
        iroh::TransportAddr::Relay(_) => RouteType::BlindRelay,
        iroh::TransportAddr::Ip(sock) => {
            if is_local_lan(&sock.ip()) {
                RouteType::LocalLan
            } else {
                RouteType::DirectP2P
            }
        }
        iroh::TransportAddr::Custom(_) => RouteType::DirectP2P,
        _ => RouteType::DirectP2P,
    }
}

/// Inspects an active connection's paths and detects the currently selected `RouteType`.
pub fn detect_route_type(conn: &Connection) -> RouteType {
    let paths = conn.paths();
    for path in paths.iter() {
        if path.is_selected() {
            return classify_transport_addr(path.remote_addr());
        }
    }
    if let Some(first) = paths.iter().next() {
        return classify_transport_addr(first.remote_addr());
    }
    RouteType::DirectP2P
}

#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("Failed to bind P2P endpoint: {0}")]
    BindFailed(String),

    #[error("Connection cancelled by user")]
    Cancelled,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Accept failed: {0}")]
    AcceptFailed(String),

    #[error("Endpoint closed")]
    EndpointClosed,

    #[error("Invalid relay URL: {0}")]
    InvalidRelayUrl(String),

    #[error("Ticket error: {0}")]
    TicketError(#[from] TicketError),
}

/// Native P2P engine managing QUIC endpoint, Share Ticket creation, and connection lifecycle.
#[derive(Clone, Debug)]
pub struct P2pEngine {
    endpoint: Endpoint,
}

impl P2pEngine {
    /// Spawns and binds a new `P2pEngine` with the default N0 preset and Qshare ALPN.
    pub async fn new() -> Result<Self, P2pError> {
        let endpoint = Endpoint::builder(presets::N0)
            .alpns(vec![QSHARE_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| P2pError::BindFailed(e.to_string()))?;

        Ok(Self { endpoint })
    }

    /// Creates a `P2pEngine` from an existing bound `iroh::Endpoint`.
    pub fn from_endpoint(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }

    /// Returns a reference to the underlying `iroh::Endpoint`.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Returns the unique public `EndpointId` (Ed25519) of this endpoint.
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Returns the current `EndpointAddr` for this endpoint.
    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Creates a canonical `QshareTicket` representing a file offer from this endpoint.
    pub fn create_ticket(
        &self,
        content_hash: blake3::Hash,
        payload_size: u64,
    ) -> Result<QshareTicket, P2pError> {
        let addr = self.endpoint.addr();
        let relay_url = addr.relay_urls().next().map(|u| u.to_string());
        let mut direct_addresses: Vec<SocketAddr> = addr.ip_addrs().copied().collect();

        // Ensure loopback addresses (127.0.0.1 / ::1) on the same bound port are included
        // so same-host loopback integration tests and local instances can connect reliably
        // without being silently dropped by Windows Defender Firewall (KI-001).
        let mut loopback_addrs = Vec::new();
        for &sock in &direct_addresses {
            let loopback = match sock {
                SocketAddr::V4(v4) => SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                    v4.port(),
                ),
                SocketAddr::V6(v6) => SocketAddr::new(
                    std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
                    v6.port(),
                ),
            };
            if !direct_addresses.contains(&loopback) && !loopback_addrs.contains(&loopback) {
                loopback_addrs.push(loopback);
            }
        }
        direct_addresses.extend(loopback_addrs);

        let ticket = QshareTicket::new(
            self.endpoint.id(),
            content_hash,
            payload_size,
            relay_url,
            direct_addresses,
        )?;

        Ok(ticket)
    }

    /// Dials a remote peer using the addressing hints in `QshareTicket`.
    /// Respects the provided `cancel_token`.
    pub async fn connect(
        &self,
        ticket: &QshareTicket,
        cancel_token: CancellationToken,
    ) -> Result<Connection, P2pError> {
        ticket.validate()?;

        let mut endpoint_addr = EndpointAddr::new(ticket.endpoint_id);
        if let Some(ref relay_str) = ticket.relay_url {
            let relay_url = relay_str
                .parse::<RelayUrl>()
                .map_err(|e| P2pError::InvalidRelayUrl(e.to_string()))?;
            endpoint_addr = endpoint_addr.with_relay_url(relay_url);
        }
        for &addr in &ticket.direct_addresses {
            endpoint_addr = endpoint_addr.with_ip_addr(addr);
        }

        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                Err(P2pError::Cancelled)
            }
            res = self.endpoint.connect(endpoint_addr, QSHARE_ALPN) => {
                res.map_err(|e| P2pError::ConnectionFailed(e.to_string()))
            }
        }
    }

    /// Listens for and accepts the next incoming QUIC connection matching QSHARE_ALPN.
    /// Respects the provided `cancel_token`.
    pub async fn accept(&self, cancel_token: CancellationToken) -> Result<Connection, P2pError> {
        tokio::select! {
            biased;
            _ = cancel_token.cancelled() => {
                Err(P2pError::Cancelled)
            }
            incoming_opt = self.endpoint.accept() => {
                let incoming = incoming_opt.ok_or(P2pError::EndpointClosed)?;
                let accepting = incoming
                    .accept()
                    .map_err(|e| P2pError::AcceptFailed(e.to_string()))?;

                tokio::select! {
                    biased;
                    _ = cancel_token.cancelled() => {
                        Err(P2pError::Cancelled)
                    }
                    res = accepting => {
                        res.map_err(|e| P2pError::ConnectionFailed(e.to_string()))
                    }
                }
            }
        }
    }
}
