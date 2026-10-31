use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use qshare::core::discovery::{LanPeer, LanPeerState, select_best_address};
use qshare::core::lan::{LanDiscoveryConfig, LanDiscoveryEvent, LanDiscoveryHandle, LanDiscoveryService};
use qshare::core::ticket::QshareTicket;
use qshare::core::transfer::TransferProgress;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileFfiError {
    #[error("Initialization error: {0}")]
    InitError(String),
    #[error("Invalid ticket: {0}")]
    InvalidTicket(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Transfer cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePeer {
    pub node_id: String,
    pub display_name: String,
    pub os_type: String,
    pub addr: String,
    pub is_stale: bool,
}

impl From<&LanPeerState> for MobilePeer {
    fn from(p: &LanPeerState) -> Self {
        Self {
            node_id: p.node_id.clone(),
            display_name: p.display_name.clone(),
            os_type: p.os_type.as_str().to_string(),
            addr: p.primary_addr.to_string(),
            is_stale: p.is_stale,
        }
    }
}

impl From<&LanPeer> for MobilePeer {
    fn from(p: &LanPeer) -> Self {
        let best_addr = select_best_address(&p.addresses, p.service_port)
            .map(|a| a.to_string())
            .unwrap_or_else(|| "0.0.0.0:0".to_string());
        let is_stale = p.last_seen.elapsed() > Duration::from_secs(8);

        Self {
            node_id: p.node_id.clone(),
            display_name: p.display_name.clone(),
            os_type: p.os_type.as_str().to_string(),
            addr: best_addr,
            is_stale,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
    pub eta_secs: Option<u64>,
}

impl From<&TransferProgress> for MobileTransferProgress {
    fn from(p: &TransferProgress) -> Self {
        Self {
            bytes_transferred: p.bytes_transferred,
            total_bytes: p.total_bytes,
            speed_bps: p.speed_bps,
            eta_secs: p.eta_secs,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTicketInfo {
    pub ticket_string: String,
    pub endpoint_id: String,
    pub payload_size: u64,
    pub content_hash: String,
}

#[derive(uniffi::Object)]
pub struct MobileQshareEngine {
    discovered_peers: Arc<StdMutex<HashMap<String, MobilePeer>>>,
    discovery_handle: StdMutex<Option<LanDiscoveryHandle>>,
}

#[uniffi::export]
impl MobileQshareEngine {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            discovered_peers: Arc::new(StdMutex::new(HashMap::new())),
            discovery_handle: StdMutex::new(None),
        })
    }

    /// Validates and parses a Base32 Qshare Share Ticket.
    pub fn parse_ticket(&self, ticket_str: String) -> Result<MobileTicketInfo, MobileFfiError> {
        let ticket = QshareTicket::from_base32(&ticket_str)
            .map_err(|e| MobileFfiError::InvalidTicket(e.to_string()))?;
        let canonical_str = ticket
            .to_base32()
            .map_err(|e| MobileFfiError::InvalidTicket(e.to_string()))?;

        Ok(MobileTicketInfo {
            ticket_string: canonical_str,
            endpoint_id: ticket.endpoint_id.to_string(),
            payload_size: ticket.payload_size,
            content_hash: ticket.content_hash.to_hex().to_string(),
        })
    }

    /// Starts local Wi-Fi mDNS peer discovery.
    pub fn start_lan_discovery(&self, display_name: String) -> Result<(), MobileFfiError> {
        let node_id = uuid::Uuid::new_v4().to_string();
        let config = LanDiscoveryConfig::new(node_id, display_name, 0);

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let handle = LanDiscoveryService::spawn(config, tx)
            .map_err(|e| MobileFfiError::NetworkError(e.to_string()))?;

        let peers_map = Arc::clone(&self.discovered_peers);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    LanDiscoveryEvent::PeerDiscovered(peer) => {
                        let mobile_peer = MobilePeer::from(&peer);
                        if let Ok(mut map) = peers_map.lock() {
                            map.insert(mobile_peer.node_id.clone(), mobile_peer);
                        }
                    }
                    LanDiscoveryEvent::PeerLost(node_id) => {
                        if let Ok(mut map) = peers_map.lock() {
                            map.remove(&node_id);
                        }
                    }
                    _ => {}
                }
            }
        });

        if let Ok(mut guard) = self.discovery_handle.lock() {
            *guard = Some(handle);
        }

        Ok(())
    }

    /// Retrieves current active peers discovered on the local subnet.
    pub fn get_lan_peers(&self) -> Vec<MobilePeer> {
        if let Ok(map) = self.discovered_peers.lock() {
            map.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Stops local network mDNS broadcast and listening.
    pub fn stop_lan_discovery(&self) {
        if let Some(handle) = self.discovery_handle.lock().ok().and_then(|mut g| g.take()) {
            handle.cancel();
        }
        if let Ok(mut map) = self.discovered_peers.lock() {
            map.clear();
        }
    }

    /// Checks if the local discovery service is running.
    pub fn is_discovery_active(&self) -> bool {
        self.discovery_handle
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }
}
