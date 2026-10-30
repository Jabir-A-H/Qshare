use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use simple_dns::rdata::{A, PTR, RData, SRV, TXT};
use simple_dns::{CLASS, Name, Packet, Question, ResourceRecord};
use tokio_util::sync::CancellationToken;

use crate::core::discovery::{LanPeer, LanPeerState, OsType, PeerTable};

pub const MDNS_SERVICE_NAME: &str = "_qshare._tcp.local";
pub const MDNS_MULTICAST_IPV4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;
pub const DEFAULT_ANNOUNCEMENT_TTL: u32 = 120;
pub const GOODBYE_TTL: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LanDiscoveryStatus {
    Idle,
    Scanning,
    Degraded,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LanDiscoveryEvent {
    PeerDiscovered(LanPeerState),
    PeerUpdated(LanPeerState),
    PeerLost(String),
    StatusChanged(LanDiscoveryStatus),
}

#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("Failed to bind UDP socket: {0}")]
    SocketBind(std::io::Error),
    #[error("Failed to join multicast group: {0}")]
    JoinMulticast(std::io::Error),
    #[error("DNS packet error: {0}")]
    DnsError(String),
    #[error("Malformed packet received: {0}")]
    MalformedPacket(String),
    #[error("Service state error: {0}")]
    ServiceState(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct LanDiscoveryConfig {
    pub node_id: String,
    pub display_name: String,
    pub os_type: OsType,
    pub service_port: u16,
    pub bind_port: u16,
    pub multicast_ip: Ipv4Addr,
    pub local_ips: Vec<Ipv4Addr>,
}

impl LanDiscoveryConfig {
    pub fn new(
        node_id: impl Into<String>,
        display_name: impl Into<String>,
        service_port: u16,
    ) -> Self {
        let local_ips = detect_local_ipv4();
        Self {
            node_id: node_id.into(),
            display_name: display_name.into(),
            os_type: OsType::current(),
            service_port,
            bind_port: MDNS_PORT,
            multicast_ip: MDNS_MULTICAST_IPV4,
            local_ips,
        }
    }
}

/// Detects routable local IPv4 addresses by probing the OS routing table.
/// Probes public DNS (8.8.8.8:80) first, falling back to mDNS multicast (224.0.0.251:5353)
/// and subnet broadcast (255.255.255.255:5353) if offline or internet route is unavailable.
pub fn detect_local_ipv4() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    let probe_targets = ["8.8.8.8:80", "224.0.0.251:5353", "255.255.255.255:5353"];

    for target in probe_targets {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0")
            && socket.connect(target).is_ok()
            && let Ok(local_addr) = socket.local_addr()
            && let IpAddr::V4(ipv4) = local_addr.ip()
            && !ipv4.is_loopback()
            && !ipv4.is_unspecified()
            && !ips.contains(&ipv4)
        {
            ips.push(ipv4);
            break;
        }
    }
    ips
}

/// Sanitizes strings into valid RFC 1035 DNS labels (alphanumeric and hyphens, <= 63 chars).
pub fn sanitize_dns_label(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("peer");
    }
    if out.len() > 63 {
        out.truncate(63);
    }
    out
}

/// Creates a UDP multicast socket with SO_REUSEADDR and joins the multicast group
/// on INADDR_ANY as well as all detected local IPv4 interface addresses.
pub fn bind_multicast_socket(
    port: u16,
    multicast_addr: Ipv4Addr,
    local_ips: &[Ipv4Addr],
) -> Result<std::net::UdpSocket, LanError> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(LanError::SocketBind)?;

    socket.set_nonblocking(true).map_err(LanError::SocketBind)?;
    socket
        .set_reuse_address(true)
        .map_err(LanError::SocketBind)?;

    #[cfg(unix)]
    {
        let _ = socket.set_reuse_port(true);
    }

    let bind_addr = std::net::SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    socket
        .bind(&bind_addr.into())
        .map_err(LanError::SocketBind)?;

    let join_unspec = socket.join_multicast_v4(&multicast_addr, &Ipv4Addr::UNSPECIFIED);
    let mut joined_any = join_unspec.is_ok();

    for &ip in local_ips {
        if !ip.is_unspecified()
            && !ip.is_loopback()
            && socket.join_multicast_v4(&multicast_addr, &ip).is_ok()
        {
            joined_any = true;
        }
    }

    if !joined_any {
        join_unspec.map_err(LanError::JoinMulticast)?;
    }

    let std_socket: std::net::UdpSocket = socket.into();
    Ok(std_socket)
}

/// Creates a UDP multicast socket with SO_REUSEADDR using detected local IPv4 addresses.
pub fn create_multicast_socket(
    port: u16,
    multicast_addr: Ipv4Addr,
) -> Result<std::net::UdpSocket, LanError> {
    let local_ips = detect_local_ipv4();
    bind_multicast_socket(port, multicast_addr, &local_ips)
}

/// Binds to desired_port, falling back to an ephemeral port if unavailable.
pub fn create_socket_with_fallback(
    desired_port: u16,
    multicast_addr: Ipv4Addr,
) -> Result<(std::net::UdpSocket, LanDiscoveryStatus), LanError> {
    let local_ips = detect_local_ipv4();
    create_socket_with_fallback_with_ips(desired_port, multicast_addr, &local_ips)
}

/// Binds to desired_port with explicit local IP interfaces, falling back to an ephemeral port if unavailable.
pub fn create_socket_with_fallback_with_ips(
    desired_port: u16,
    multicast_addr: Ipv4Addr,
    local_ips: &[Ipv4Addr],
) -> Result<(std::net::UdpSocket, LanDiscoveryStatus), LanError> {
    if desired_port == 0 {
        let sock = bind_multicast_socket(0, multicast_addr, local_ips)?;
        return Ok((sock, LanDiscoveryStatus::Scanning));
    }

    match bind_multicast_socket(desired_port, multicast_addr, local_ips) {
        Ok(sock) => {
            tracing::info!("Bound mDNS multicast socket on port {desired_port}");
            Ok((sock, LanDiscoveryStatus::Scanning))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to bind multicast socket on port {desired_port} ({e}); falling back to ephemeral port"
            );
            let sock = bind_multicast_socket(0, multicast_addr, local_ips)?;
            tracing::info!("Bound fallback mDNS multicast socket on ephemeral port");
            Ok((sock, LanDiscoveryStatus::Degraded))
        }
    }
}

/// Serializes an mDNS announcement into wire format bytes.
pub fn build_announcement_bytes(
    node_id: &str,
    display_name: &str,
    os_type: OsType,
    service_port: u16,
    ips: &[Ipv4Addr],
    ttl: u32,
) -> Result<Vec<u8>, LanError> {
    let mut packet = Packet::new_reply(0);
    packet.set_flags(simple_dns::PacketFlag::RESPONSE);

    let service_name =
        Name::new(MDNS_SERVICE_NAME).map_err(|e| LanError::DnsError(e.to_string()))?;

    let safe_node_id = sanitize_dns_label(node_id);
    let instance_str = format!("{safe_node_id}.{MDNS_SERVICE_NAME}");
    let instance_name = Name::new(&instance_str).map_err(|e| LanError::DnsError(e.to_string()))?;

    let host_str = format!("{safe_node_id}.local");
    let host_name = Name::new(&host_str).map_err(|e| LanError::DnsError(e.to_string()))?;

    // 1. PTR Record: _qshare._tcp.local -> <safe_node_id>._qshare._tcp.local
    packet.answers.push(ResourceRecord::new(
        service_name,
        CLASS::IN,
        ttl,
        RData::PTR(PTR(instance_name.clone())),
    ));

    // 2. SRV Record: <instance_name> -> target host_name, service_port
    let srv = SRV {
        priority: 0,
        weight: 0,
        port: service_port,
        target: host_name.clone(),
    };
    packet.answers.push(ResourceRecord::new(
        instance_name.clone(),
        CLASS::IN,
        ttl,
        RData::SRV(srv),
    ));

    // 3. TXT Record: node_id, display_name, os, service_port
    let node_attr = format!("node_id={node_id}");
    let name_attr = format!("display_name={display_name}");
    let os_attr = format!("os={}", os_type.as_str());
    let port_attr = format!("service_port={service_port}");

    let mut txt = TXT::new();
    let _ = txt.add_string(&node_attr);
    let _ = txt.add_string(&name_attr);
    let _ = txt.add_string(&os_attr);
    let _ = txt.add_string(&port_attr);

    packet.answers.push(ResourceRecord::new(
        instance_name,
        CLASS::IN,
        ttl,
        RData::TXT(txt),
    ));

    // 4. A Records
    for &ip in ips {
        let a = A::from(ip);
        packet.additional_records.push(ResourceRecord::new(
            host_name.clone(),
            CLASS::IN,
            ttl,
            RData::A(a),
        ));
    }

    packet
        .build_bytes_vec_compressed()
        .map_err(|e| LanError::DnsError(e.to_string()))
}

/// Serializes an mDNS PTR query into wire format bytes.
pub fn build_query_bytes(unicast_response: bool) -> Result<Vec<u8>, LanError> {
    let mut packet = Packet::new_query(0);
    let service_name =
        Name::new(MDNS_SERVICE_NAME).map_err(|e| LanError::DnsError(e.to_string()))?;
    let question = Question::new(
        service_name,
        simple_dns::rdata::TYPE::PTR.into(),
        CLASS::IN.into(),
        unicast_response,
    );
    packet.questions.push(question);

    packet
        .build_bytes_vec_compressed()
        .map_err(|e| LanError::DnsError(e.to_string()))
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodedPacket {
    Peer(LanPeer),
    Goodbye(String),
    Query,
    Ignored,
}

/// Decodes an mDNS packet into a peer announcement, Goodbye event, or query.
pub fn decode_packet(packet: &Packet<'_>, src_addr: SocketAddr) -> DecodedPacket {
    let mut is_goodbye = false;
    let mut node_id: Option<String> = None;
    let mut display_name: Option<String> = None;
    let mut os_type = OsType::Unknown;
    let mut service_port: Option<u16> = None;
    let mut ips: Vec<Ipv4Addr> = Vec::new();

    let all_records = packet
        .answers
        .iter()
        .chain(packet.additional_records.iter());

    for rr in all_records {
        if rr.ttl == GOODBYE_TTL {
            is_goodbye = true;
        }

        match &rr.rdata {
            RData::TXT(txt) => {
                let attrs = txt.attributes();
                if let Some(Some(id)) = attrs.get("node_id") {
                    node_id = Some(id.clone());
                }
                if let Some(Some(name)) = attrs.get("display_name").or_else(|| attrs.get("name")) {
                    display_name = Some(name.clone());
                }
                if let Some(Some(os_str)) = attrs.get("os").or_else(|| attrs.get("os_type")) {
                    os_type = OsType::from_str_loose(os_str);
                }
                if let Some(Some(p_str)) = attrs.get("service_port").or_else(|| attrs.get("port"))
                    && let Ok(p) = p_str.parse::<u16>()
                {
                    service_port = Some(p);
                }
            }
            RData::SRV(srv) => {
                if service_port.is_none() {
                    service_port = Some(srv.port);
                }
                if node_id.is_none() {
                    let name_str = rr.name.to_string();
                    if let Some(first) = name_str.split('.').next()
                        && !first.is_empty()
                        && first != "_qshare"
                    {
                        node_id = Some(first.to_string());
                    }
                }
            }
            RData::A(a) => {
                let ip = Ipv4Addr::from(a.address);
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
            RData::PTR(ptr) if node_id.is_none() => {
                let ptr_str = ptr.to_string();
                if let Some(first) = ptr_str.split('.').next()
                    && !first.is_empty()
                    && first != "_qshare"
                {
                    node_id = Some(first.to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(id) = node_id {
        if is_goodbye {
            return DecodedPacket::Goodbye(id);
        }

        let name = display_name.unwrap_or_else(|| id.clone());
        let port = service_port.unwrap_or(src_addr.port());

        let mut addresses: Vec<SocketAddr> = ips
            .into_iter()
            .map(|ip| SocketAddr::new(IpAddr::V4(ip), port))
            .collect();

        let fallback_addr = SocketAddr::new(src_addr.ip(), port);
        if !addresses.contains(&fallback_addr) {
            addresses.push(fallback_addr);
        }

        let peer = LanPeer {
            node_id: id,
            display_name: name,
            os_type,
            addresses,
            service_port: port,
            last_seen: Instant::now(),
        };

        DecodedPacket::Peer(peer)
    } else if !packet.questions.is_empty() {
        DecodedPacket::Query
    } else {
        DecodedPacket::Ignored
    }
}

pub struct LanDiscoveryHandle {
    shutdown_token: CancellationToken,
    join_handle: tokio::task::JoinHandle<()>,
}

impl LanDiscoveryHandle {
    pub async fn shutdown(self) {
        self.shutdown_token.cancel();
        let _ = self.join_handle.await;
    }

    pub fn cancel(&self) {
        self.shutdown_token.cancel();
    }
}

async fn emit_event(
    event_tx: &tokio::sync::mpsc::Sender<LanDiscoveryEvent>,
    event: LanDiscoveryEvent,
) -> bool {
    match &event {
        LanDiscoveryEvent::PeerLost(_) | LanDiscoveryEvent::StatusChanged(_) => {
            if let Err(e) = event_tx.send(event).await {
                tracing::warn!("Failed to deliver critical LAN event, channel closed: {e}");
                false
            } else {
                true
            }
        }
        _ => match event_tx.try_send(event) {
            Ok(()) => true,
            Err(tokio::sync::mpsc::error::TrySendError::Full(evt)) => {
                tracing::warn!(
                    "Event channel buffer full, dropped discovery event: {:?}",
                    evt
                );
                true
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("Event channel closed while sending discovery event");
                false
            }
        },
    }
}

pub struct LanDiscoveryService;

impl LanDiscoveryService {
    /// Spawns the LAN mDNS discovery background service.
    pub fn spawn(
        config: LanDiscoveryConfig,
        event_tx: tokio::sync::mpsc::Sender<LanDiscoveryEvent>,
    ) -> Result<LanDiscoveryHandle, LanError> {
        let (std_socket, initial_status) = create_socket_with_fallback_with_ips(
            config.bind_port,
            config.multicast_ip,
            &config.local_ips,
        )?;
        let shutdown_token = CancellationToken::new();

        let cancel_clone = shutdown_token.clone();
        let join_handle = tokio::spawn(async move {
            Self::run_loop(std_socket, initial_status, config, event_tx, cancel_clone).await;
        });

        Ok(LanDiscoveryHandle {
            shutdown_token,
            join_handle,
        })
    }

    async fn run_loop(
        std_socket: std::net::UdpSocket,
        initial_status: LanDiscoveryStatus,
        config: LanDiscoveryConfig,
        event_tx: tokio::sync::mpsc::Sender<LanDiscoveryEvent>,
        shutdown_token: CancellationToken,
    ) {
        let socket = match tokio::net::UdpSocket::from_std(std_socket) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to create tokio UdpSocket: {e}");
                let _ = event_tx
                    .send(LanDiscoveryEvent::StatusChanged(LanDiscoveryStatus::Error))
                    .await;
                return;
            }
        };

        let status = initial_status;
        if !emit_event(&event_tx, LanDiscoveryEvent::StatusChanged(status)).await {
            return;
        }

        let mut config = config;
        let multicast_dest = SocketAddr::new(IpAddr::V4(config.multicast_ip), MDNS_PORT);
        let mut peer_table = PeerTable::new();

        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(3));
        let mut reaper_interval = tokio::time::interval(Duration::from_secs(1));
        let mut qu_interval = tokio::time::interval(Duration::from_secs(5));

        // Broadcast initial query packet so existing peers respond immediately (<50ms)
        if let Ok(query_bytes) = build_query_bytes(false) {
            let _ = socket.send_to(&query_bytes, multicast_dest).await;
        }

        // Initial announcement
        if let Ok(bytes) = build_announcement_bytes(
            &config.node_id,
            &config.display_name,
            config.os_type,
            config.service_port,
            &config.local_ips,
            DEFAULT_ANNOUNCEMENT_TTL,
        ) {
            let _ = socket.send_to(&bytes, multicast_dest).await;
        }

        let mut recv_buf = [0u8; 4096];

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    tracing::info!("LanDiscoveryService shutting down; broadcasting Goodbye");
                    if let Ok(goodbye_bytes) = build_announcement_bytes(
                        &config.node_id,
                        &config.display_name,
                        config.os_type,
                        config.service_port,
                        &config.local_ips,
                        GOODBYE_TTL,
                    ) {
                        let _ = socket.send_to(&goodbye_bytes, multicast_dest).await;
                    }
                    let _ = event_tx
                        .send(LanDiscoveryEvent::StatusChanged(LanDiscoveryStatus::Idle))
                        .await;
                    break;
                }
                _ = heartbeat_interval.tick() => {
                    if config.local_ips.is_empty() {
                        let reprobed = detect_local_ipv4();
                        if !reprobed.is_empty() {
                            tracing::info!("Discovered local IPv4 addresses on heartbeat: {:?}", reprobed);
                            config.local_ips = reprobed;
                        }
                    }

                    if let Ok(announcement) = build_announcement_bytes(
                        &config.node_id,
                        &config.display_name,
                        config.os_type,
                        config.service_port,
                        &config.local_ips,
                        DEFAULT_ANNOUNCEMENT_TTL,
                    ) {
                        let _ = socket.send_to(&announcement, multicast_dest).await;
                    }
                }
                _ = reaper_interval.tick() => {
                    let evicted = peer_table.evict_expired();
                    for node_id in evicted {
                        tracing::info!("Evicted expired peer from radar: {node_id}");
                        if !emit_event(&event_tx, LanDiscoveryEvent::PeerLost(node_id)).await {
                            return;
                        }
                    }
                }
                _ = qu_interval.tick(), if status == LanDiscoveryStatus::Degraded => {
                    // Send active QU query to multicast group to solicit unicast replies
                    if let Ok(query_bytes) = build_query_bytes(true) {
                        let _ = socket.send_to(&query_bytes, multicast_dest).await;
                    }
                }
                recv_res = socket.recv_from(&mut recv_buf) => {
                    match recv_res {
                        Ok((len, src_addr)) => {
                            let payload = &recv_buf[..len];

                            // Rate limit check (< 5 pkts/sec)
                            if !peer_table.check_rate_limit(src_addr.ip()) {
                                continue;
                            }

                            // Pre-parser filter for _qshare (case-insensitive)
                            if !payload.windows(7).any(|w| w.eq_ignore_ascii_case(b"_qshare")) {
                                continue;
                            }

                            let packet = match Packet::parse(payload) {
                                Ok(p) => p,
                                Err(_) => continue,
                            };

                            match decode_packet(&packet, src_addr) {
                                DecodedPacket::Goodbye(node_id) => {
                                    if node_id == config.node_id {
                                        continue;
                                    }
                                    if peer_table.remove_peer(&node_id).is_some() {
                                        tracing::info!("Received Goodbye (TTL=0) from {node_id}");
                                        if !emit_event(&event_tx, LanDiscoveryEvent::PeerLost(node_id)).await {
                                            break;
                                        }
                                    }
                                }
                                DecodedPacket::Peer(peer) => {
                                    if peer.node_id == config.node_id {
                                        continue;
                                    }
                                    let (is_new, changed, ui_state) = peer_table.insert_or_refresh(peer);
                                    if let Some(state) = ui_state {
                                        if is_new {
                                            tracing::info!("Discovered LAN peer: {}", state.display_name);
                                            if !emit_event(&event_tx, LanDiscoveryEvent::PeerDiscovered(state)).await {
                                                break;
                                            }
                                        } else if changed {
                                            tracing::debug!("Updated LAN peer: {}", state.display_name);
                                            if !emit_event(&event_tx, LanDiscoveryEvent::PeerUpdated(state)).await {
                                                break;
                                            }
                                        }
                                    }
                                }
                                DecodedPacket::Query => {
                                    // Respond to query with unicast announcement
                                    if let Ok(announcement) = build_announcement_bytes(
                                        &config.node_id,
                                        &config.display_name,
                                        config.os_type,
                                        config.service_port,
                                        &config.local_ips,
                                        DEFAULT_ANNOUNCEMENT_TTL,
                                    ) {
                                        let _ = socket.send_to(&announcement, src_addr).await;
                                    }
                                }
                                DecodedPacket::Ignored => {}
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Socket recv_from error: {e}");
                        }
                    }
                }
            }
        }
    }
}
