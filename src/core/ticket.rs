use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub const MAX_TICKET_STRING_LEN: usize = 4096;
pub const MAX_DIRECT_ADDRESSES: usize = 32;
pub const MAX_RELAY_URL_LEN: usize = 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TicketError {
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Invalid Base32 encoding: {0}")]
    InvalidBase32(String),

    #[error("Invalid ticket format: {0}")]
    InvalidFormat(String),

    #[error("Untrusted relay URL: {0}")]
    UntrustedRelayUrl(String),
}

/// Canonical Qshare Transfer Ticket.
/// Encodes sender endpoint identification, content verification hash, payload size,
/// and optional relay / direct network hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QshareTicket {
    pub endpoint_id: iroh::EndpointId,
    pub content_hash: blake3::Hash,
    pub payload_size: u64,
    pub relay_url: Option<String>,
    pub direct_addresses: Vec<SocketAddr>,
}

impl QshareTicket {
    /// Creates a new `QshareTicket` and validates its parameters.
    pub fn new(
        endpoint_id: iroh::EndpointId,
        content_hash: blake3::Hash,
        payload_size: u64,
        relay_url: Option<String>,
        direct_addresses: Vec<SocketAddr>,
    ) -> Result<Self, TicketError> {
        let ticket = Self {
            endpoint_id,
            content_hash,
            payload_size,
            relay_url,
            direct_addresses,
        };
        ticket.validate()?;
        Ok(ticket)
    }

    /// Serializes ticket to Base32 string (RFC 4648 without padding).
    pub fn to_base32(&self) -> Result<String, TicketError> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self)
            .map_err(|e| TicketError::SerializationFailed(e.to_string()))?;
        Ok(data_encoding::BASE32_NOPAD.encode(&bytes))
    }

    /// Deserializes ticket from Base32 string, stripping optional "qshare://" prefix.
    pub fn from_base32(s: &str) -> Result<Self, TicketError> {
        if s.len() > MAX_TICKET_STRING_LEN {
            return Err(TicketError::InvalidFormat(format!(
                "Ticket string exceeds maximum length of {MAX_TICKET_STRING_LEN} characters"
            )));
        }

        let trimmed = s.trim();
        let without_prefix = if let Some(prefix) = trimmed.get(..9) {
            if prefix.eq_ignore_ascii_case("qshare://") {
                &trimmed[9..]
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        let without_whitespace: String = without_prefix
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();

        let unpadded = without_whitespace.trim_end_matches('=');
        if unpadded.is_empty() {
            return Err(TicketError::InvalidFormat("Empty ticket string".into()));
        }

        let upper = unpadded.to_ascii_uppercase();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(upper.as_bytes())
            .map_err(|e| TicketError::InvalidBase32(e.to_string()))?;
        let ticket: Self = postcard::from_bytes(&bytes)
            .map_err(|e| TicketError::DeserializationFailed(e.to_string()))?;
        ticket.validate()?;
        Ok(ticket)
    }

    /// Validates ticket fields and enforces SSRF defenses against malicious relay redirection.
    pub fn validate(&self) -> Result<(), TicketError> {
        if self.direct_addresses.len() > MAX_DIRECT_ADDRESSES {
            return Err(TicketError::InvalidFormat(format!(
                "Direct addresses count {} exceeds limit of {}",
                self.direct_addresses.len(),
                MAX_DIRECT_ADDRESSES
            )));
        }
        if let Some(ref relay) = self.relay_url {
            if relay.len() > MAX_RELAY_URL_LEN {
                return Err(TicketError::UntrustedRelayUrl(format!(
                    "Relay URL length {} exceeds limit of {}",
                    relay.len(),
                    MAX_RELAY_URL_LEN
                )));
            }
            validate_relay_url(relay)?;
        }
        Ok(())
    }
}

/// Enforces SSRF defense:
/// - Must use valid HTTPS scheme.
/// - Must NOT target private, loopback, or link-local addresses (RFC 1918, RFC 3927, loopback).
fn validate_relay_url(url_str: &str) -> Result<(), TicketError> {
    if url_str.len() > MAX_RELAY_URL_LEN {
        return Err(TicketError::UntrustedRelayUrl(format!(
            "Relay URL length {} exceeds limit of {}",
            url_str.len(),
            MAX_RELAY_URL_LEN
        )));
    }

    let parsed = url::Url::parse(url_str)
        .map_err(|e| TicketError::UntrustedRelayUrl(format!("Invalid URL syntax: {e}")))?;

    if parsed.scheme() != "https" {
        return Err(TicketError::UntrustedRelayUrl(format!(
            "Relay URL scheme must be 'https', got '{}'",
            parsed.scheme()
        )));
    }

    let host = parsed
        .host()
        .ok_or_else(|| TicketError::UntrustedRelayUrl("Relay URL is missing a host".to_string()))?;

    match host {
        url::Host::Ipv4(ipv4) => {
            if is_forbidden_ipv4(&ipv4) {
                return Err(TicketError::UntrustedRelayUrl(format!(
                    "Relay URL targets forbidden private/loopback/link-local IPv4: {ipv4}"
                )));
            }
        }
        url::Host::Ipv6(ipv6) => {
            if is_forbidden_ipv6(&ipv6) {
                return Err(TicketError::UntrustedRelayUrl(format!(
                    "Relay URL targets forbidden private/loopback/link-local IPv6: {ipv6}"
                )));
            }
        }
        url::Host::Domain(domain) => {
            let lower = domain.to_lowercase();
            let clean_domain = lower.trim_end_matches('.');
            if clean_domain == "localhost"
                || clean_domain.ends_with(".localhost")
                || clean_domain.ends_with(".local")
                || clean_domain == "internal"
                || clean_domain.ends_with(".internal")
                || clean_domain == "lan"
                || clean_domain.ends_with(".lan")
                || clean_domain == "home.arpa"
                || clean_domain.ends_with(".home.arpa")
            {
                return Err(TicketError::UntrustedRelayUrl(format!(
                    "Relay URL cannot target private/local domain: {domain}"
                )));
            }
            if let Ok(ip) = clean_domain.parse::<std::net::IpAddr>() {
                match ip {
                    std::net::IpAddr::V4(v4) => {
                        if is_forbidden_ipv4(&v4) {
                            return Err(TicketError::UntrustedRelayUrl(format!(
                                "Relay URL targets forbidden private/loopback/link-local IPv4: {v4}"
                            )));
                        }
                    }
                    std::net::IpAddr::V6(v6) => {
                        if is_forbidden_ipv6(&v6) {
                            return Err(TicketError::UntrustedRelayUrl(format!(
                                "Relay URL targets forbidden private/loopback/link-local IPv6: {v6}"
                            )));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn is_forbidden_ipv4(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback() // 127.0.0.0/8
        || octets[0] == 10 // 10.0.0.0/8
        || (octets[0] == 172 && (16..=31).contains(&octets[1])) // 172.16.0.0/12
        || (octets[0] == 192 && octets[1] == 168) // 192.168.0.0/16
        || (octets[0] == 169 && octets[1] == 254) // 169.254.0.0/16 (link-local, cloud metadata)
        || (octets[0] == 100 && (64..=127).contains(&octets[1])) // 100.64.0.0/10 CGNAT
        || ip.is_multicast() // 224.0.0.0/4 (224.0.0.0 - 239.255.255.255)
        || octets[0] >= 240 // Reserved for future use (240.0.0.0/4 including broadcast)
        || octets[0] == 0 // 0.0.0.0/8 ("This host on this network", RFC 1122)
        || ip.is_broadcast() // 255.255.255.255
}

fn is_forbidden_ipv6(ip: &std::net::Ipv6Addr) -> bool {
    let octets = ip.octets();
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_ipv4(&v4);
    }
    // Check IPv4-compatible (::a.b.c.d) or NAT64 (64:ff9b::a.b.c.d) prefix
    if (octets[..12] == [0; 12]
        && (octets[12] != 0 || octets[13] != 0 || octets[14] != 0 || octets[15] != 0))
        || (octets[..4] == [0x00, 0x64, 0xff, 0x9b] && octets[4..12] == [0; 8])
    {
        let v4 = std::net::Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
        if is_forbidden_ipv4(&v4) {
            return true;
        }
    }
    ip.is_loopback() // ::1
        || ip.is_multicast() // ff00::/8
        || (octets[0] & 0xfe) == 0xfc // fc00::/7 unique local
        || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80) // fe80::/10 link-local
        || octets == [0; 16] // ::
}
