use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use qshare::core::discovery::{LanPeer, OsType, PeerTable, select_best_address};
use qshare::core::lan::{
    DEFAULT_ANNOUNCEMENT_TTL, DecodedPacket, GOODBYE_TTL, build_announcement_bytes,
    build_query_bytes, decode_packet,
};

#[test]
fn test_peer_table_lifecycle_and_ttl_eviction() {
    let mut table = PeerTable::new();
    let now = Instant::now();

    let peer = LanPeer {
        node_id: "node-lifecycle-test".to_string(),
        display_name: "Alice Phone".to_string(),
        os_type: OsType::Android,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)),
            4242,
        )],
        service_port: 4242,
        last_seen: now,
    };

    // 1. Initial insertion -> Active (< 8s)
    let (is_new, changed, state) = table.insert_or_refresh(peer.clone());
    assert!(is_new);
    assert!(!changed);
    let ui_state = state.expect("UI state should be generated for valid peer");
    assert_eq!(ui_state.display_name, "Alice Phone");
    assert!(!ui_state.is_stale, "Peer should be active, not stale");
    assert_eq!(table.len(), 1);

    // 2. Peer last seen 9 seconds ago -> Stale (> 8s, < 15s)
    let mut stale_peer = peer.clone();
    stale_peer.last_seen = now - Duration::from_secs(9);
    let (is_new_stale, _changed_stale, stale_state) = table.insert_or_refresh(stale_peer);
    assert!(!is_new_stale);
    let stale_ui = stale_state.expect("UI state must exist");
    assert!(
        stale_ui.is_stale,
        "Peer should be flagged as stale after 9s"
    );

    // Evict expired should NOT remove stale peers (< 15s)
    let evicted = table.evict_expired();
    assert!(evicted.is_empty(), "Stale peers should not be evicted yet");
    assert_eq!(table.len(), 1);

    // 3. Peer last seen 16 seconds ago -> Evicted (> 15s)
    let mut expired_peer = peer;
    expired_peer.last_seen = now - Duration::from_secs(16);
    table.insert_or_refresh(expired_peer);

    let evicted = table.evict_expired();
    assert_eq!(evicted.len(), 1);
    assert_eq!(evicted[0], "node-lifecycle-test");
    assert_eq!(table.len(), 0);
    assert!(table.get("node-lifecycle-test").is_none());
}

#[test]
fn test_peer_table_capacity_bound() {
    let mut table = PeerTable::new();
    let now = Instant::now();

    // Insert MAX_LAN_PEERS (256) peers
    for i in 0..PeerTable::MAX_LAN_PEERS {
        let peer = LanPeer {
            node_id: format!("node-cap-{i:03}"),
            display_name: format!("Peer {i}"),
            os_type: OsType::Linux,
            addresses: vec![SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(10, 0, (i / 256) as u8, (i % 256) as u8)),
                5000,
            )],
            service_port: 5000,
            last_seen: now,
        };

        let (is_new, _, state) = table.insert_or_refresh(peer);
        assert!(is_new, "Peer {i} should be newly inserted");
        assert!(state.is_some());
    }

    assert_eq!(table.len(), PeerTable::MAX_LAN_PEERS);

    // Attempt to insert 257th peer when all 256 are active
    let surplus_peer = LanPeer {
        node_id: "surplus-node-257".to_string(),
        display_name: "Surplus Peer".to_string(),
        os_type: OsType::Windows,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 1, 0, 1)),
            5000,
        )],
        service_port: 5000,
        last_seen: now,
    };

    let (is_new, changed, state) = table.insert_or_refresh(surplus_peer);
    assert!(!is_new, "Surplus peer insertion must be rejected");
    assert!(!changed);
    assert!(state.is_none());
    assert_eq!(table.len(), PeerTable::MAX_LAN_PEERS);

    // Updating an existing peer must still succeed even at max capacity
    let mut update_peer = LanPeer {
        node_id: "node-cap-000".to_string(),
        display_name: "Updated Peer 0".to_string(),
        os_type: OsType::MacOS,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 99)),
            6000,
        )],
        service_port: 6000,
        last_seen: now,
    };
    update_peer.display_name = "Updated Peer 0".to_string();

    let (is_new, changed, state) = table.insert_or_refresh(update_peer);
    assert!(!is_new);
    assert!(changed, "Address and name changed should be reported");
    let state = state.expect("State should exist for updated peer");
    assert_eq!(state.display_name, "Updated Peer 0");
    assert_eq!(state.primary_addr.port(), 6000);
    assert_eq!(table.len(), PeerTable::MAX_LAN_PEERS);
}

#[test]
fn test_peer_table_deterministic_polar_coordinates() {
    let node_id = "peer-blake3-fixed-identity-123456";
    let now = Instant::now();

    let peer1 = LanPeer {
        node_id: node_id.to_string(),
        display_name: "Workstation One".to_string(),
        os_type: OsType::Windows,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            4000,
        )],
        service_port: 4000,
        last_seen: now,
    };

    let state1 = peer1.to_ui_state().unwrap();
    assert!(
        state1.polar_angle_deg >= 0.0 && state1.polar_angle_deg < 360.0,
        "Angle must be in [0, 360)"
    );
    assert_eq!(
        state1.distance_ratio, 0.35,
        "Local LAN orbit distance ratio must be 0.35"
    );

    // Reconstruct with changed metadata/address; polar angle must remain perfectly identical
    let peer2 = LanPeer {
        node_id: node_id.to_string(),
        display_name: "Renamed Workstation".to_string(),
        os_type: OsType::Linux,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 99)),
            8080,
        )],
        service_port: 8080,
        last_seen: now,
    };

    let state2 = peer2.to_ui_state().unwrap();
    assert_eq!(
        state1.polar_angle_deg, state2.polar_angle_deg,
        "Polar angle must be deterministic based on node_id"
    );
}

#[test]
fn test_simple_dns_encode_decode_roundtrip() {
    let node_id = "test-node-encode-roundtrip";
    let display_name = "MacBook Pro M3";
    let os_type = OsType::MacOS;
    let service_port = 4567;
    let ips = vec![Ipv4Addr::new(192, 168, 1, 120), Ipv4Addr::new(10, 0, 0, 45)];
    let ttl = DEFAULT_ANNOUNCEMENT_TTL;

    let encoded_bytes =
        build_announcement_bytes(node_id, display_name, os_type, service_port, &ips, ttl)
            .expect("Announcement packet encoding must succeed");
    assert!(!encoded_bytes.is_empty());

    let packet = simple_dns::Packet::parse(&encoded_bytes).expect("Packet parsing must succeed");
    let dummy_src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 120)), service_port);

    let decoded = decode_packet(&packet, dummy_src);
    match decoded {
        DecodedPacket::Peer(peer) => {
            assert_eq!(peer.node_id, node_id);
            assert_eq!(peer.display_name, display_name);
            assert_eq!(peer.os_type, OsType::MacOS);
            assert_eq!(peer.service_port, service_port);

            let ui = peer.to_ui_state().expect("Valid UI state required");
            assert_eq!(ui.display_name, display_name);
            assert_eq!(
                ui.primary_addr,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 120)), service_port)
            );
            assert_eq!(ui.distance_ratio, 0.35);
            assert!(!ui.is_stale);
        }
        other => panic!("Expected DecodedPacket::Peer, got {:?}", other),
    }
}

#[test]
fn test_goodbye_packet_triggers_immediate_eviction() {
    let mut table = PeerTable::new();
    let node_id = "leaving-peer-node-007";
    let display_name = "Departing Node";

    // 1. Insert active peer
    let peer = LanPeer {
        node_id: node_id.to_string(),
        display_name: display_name.to_string(),
        os_type: OsType::Windows,
        addresses: vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 88)),
            5353,
        )],
        service_port: 5353,
        last_seen: Instant::now(),
    };
    table.insert_or_refresh(peer);
    assert_eq!(table.len(), 1);

    // 2. Generate Goodbye packet with TTL = 0
    let goodbye_bytes = build_announcement_bytes(
        node_id,
        display_name,
        OsType::Windows,
        5353,
        &[Ipv4Addr::new(192, 168, 1, 88)],
        GOODBYE_TTL,
    )
    .expect("Goodbye packet build should succeed");

    let packet =
        simple_dns::Packet::parse(&goodbye_bytes).expect("Goodbye packet parse should succeed");
    let dummy_src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 88)), 5353);

    let decoded = decode_packet(&packet, dummy_src);
    match decoded {
        DecodedPacket::Goodbye(id) => {
            assert_eq!(id, node_id);
            // Simulate immediate removal on Goodbye event
            let removed = table.remove_peer(&id);
            assert!(removed.is_some(), "Peer should be removed immediately");
            assert_eq!(table.len(), 0);
            assert!(table.get(node_id).is_none());
        }
        other => panic!("Expected DecodedPacket::Goodbye, got {:?}", other),
    }
}

#[test]
fn test_rate_limiter_caps_at_5_pkts_per_sec() {
    let mut table = PeerTable::new();
    let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 200));

    // First 5 packets within the same second should be allowed
    for _ in 0..5 {
        assert!(table.check_rate_limit(ip));
    }

    // 6th packet must be dropped
    assert!(!table.check_rate_limit(ip));
    assert!(!table.check_rate_limit(ip));

    // Another IP should not be affected
    let other_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 201));
    assert!(table.check_rate_limit(other_ip));
}

#[test]
fn test_best_address_filters_virtual_and_loopback() {
    let service_port = 8080;
    let addresses = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080), // Loopback -> Rank 5 (lowest)
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(169, 254, 10, 20)), 8080), // APIPA -> filtered
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),   // Unspecified -> filtered
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 45)), 0), // RFC 1918 Private -> Best!
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 15)), 0),    // Secondary Private
    ];

    let best = select_best_address(&addresses, service_port).expect("Should find best address");
    assert_eq!(
        best,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 45)), service_port)
    );

    // Loopback is accepted as lowest-rank fallback (rank 5) when no private IP is available
    let loopback_only = vec![SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        8080,
    )];
    let best_loopback =
        select_best_address(&loopback_only, service_port).expect("Should accept loopback fallback");
    assert_eq!(
        best_loopback,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), service_port)
    );
}

#[test]
fn test_64_char_hex_node_id_and_unicode_display_name() {
    let hex_node_id = "4a5b6c7d8e9f0123456789abcdef0123456789abcdef0123456789abcdef0123";
    assert_eq!(hex_node_id.len(), 64);
    let display_name = "René's MacBook 💻";
    let service_port = 5252;
    let ips = vec![Ipv4Addr::new(192, 168, 1, 77)];

    let bytes = build_announcement_bytes(
        hex_node_id,
        display_name,
        OsType::MacOS,
        service_port,
        &ips,
        DEFAULT_ANNOUNCEMENT_TTL,
    )
    .expect("Failed to build announcement with 64-char hex node_id and unicode name");

    let packet = simple_dns::Packet::parse(&bytes).expect("Failed to parse packet");
    let src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 77)), 5353);
    let decoded = decode_packet(&packet, src);

    match decoded {
        DecodedPacket::Peer(peer) => {
            assert_eq!(
                peer.node_id, hex_node_id,
                "64-char hex node_id must be preserved in TXT record"
            );
            assert_eq!(
                peer.display_name, display_name,
                "Unicode display name must be preserved"
            );
            assert_eq!(peer.os_type, OsType::MacOS);
            assert_eq!(peer.service_port, service_port);

            let state = peer.to_ui_state().expect("UI state should be generated");
            assert_eq!(state.node_id, hex_node_id);
            assert_eq!(state.display_name, display_name);
            assert_eq!(
                state.primary_addr,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 77)), service_port)
            );
        }
        other => panic!("Expected DecodedPacket::Peer, got {:?}", other),
    }
}

#[test]
fn test_source_port_address_pollution_fix() {
    let node_id = "node-port-pollution-test";
    let display_name = "Desktop PC";
    let service_port = 9876;
    let ips = vec![Ipv4Addr::new(192, 168, 1, 50)];

    let bytes = build_announcement_bytes(
        node_id,
        display_name,
        OsType::Windows,
        service_port,
        &ips,
        DEFAULT_ANNOUNCEMENT_TTL,
    )
    .expect("build announcement");

    let packet = simple_dns::Packet::parse(&bytes).expect("parse packet");

    // The incoming UDP packet comes from port 5353 (mDNS multicast socket)
    let src_mdns_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 5353);
    let decoded = decode_packet(&packet, src_mdns_addr);

    match decoded {
        DecodedPacket::Peer(peer) => {
            assert_eq!(peer.service_port, service_port);
            // Verify that port 5353 NEVER pollutes the peer's addresses
            for addr in &peer.addresses {
                assert_ne!(
                    addr.port(),
                    5353,
                    "mDNS port 5353 must not pollute candidate addresses"
                );
                assert_eq!(
                    addr.port(),
                    service_port,
                    "Every candidate address must use the service port"
                );
            }
            assert_eq!(peer.addresses.len(), 1);
            assert_eq!(
                peer.addresses[0],
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), service_port)
            );
        }
        other => panic!("Expected DecodedPacket::Peer, got {:?}", other),
    }

    // Also test fallback when no A records are present in packet:
    let no_ip_bytes = build_announcement_bytes(
        node_id,
        display_name,
        OsType::Windows,
        service_port,
        &[], // No A records
        DEFAULT_ANNOUNCEMENT_TTL,
    )
    .expect("build announcement with no A records");

    let no_ip_packet = simple_dns::Packet::parse(&no_ip_bytes).expect("parse packet");
    let fallback_src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 99)), 5353);
    let decoded_fallback = decode_packet(&no_ip_packet, fallback_src);

    match decoded_fallback {
        DecodedPacket::Peer(peer) => {
            assert_eq!(peer.service_port, service_port);
            assert_eq!(peer.addresses.len(), 1);
            assert_eq!(
                peer.addresses[0],
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 99)), service_port),
                "Fallback address must use src IP and service_port, NOT port 5353"
            );
        }
        other => panic!("Expected DecodedPacket::Peer, got {:?}", other),
    }
}

#[tokio::test]
async fn test_real_udp_socket_loopback_discovery() {
    // Bind two independent UDP sockets on loopback with ephemeral ports
    let socket_a = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("Socket A bind");
    let addr_a = socket_a.local_addr().expect("Socket A local addr");

    let socket_b = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("Socket B bind");
    let addr_b = socket_b.local_addr().expect("Socket B local addr");

    // 1. Peer A transmits an mDNS query packet to Peer B
    let query_bytes = build_query_bytes(false).expect("build query bytes");
    socket_a
        .send_to(&query_bytes, addr_b)
        .await
        .expect("send query to B");

    // 2. Peer B receives and parses the query
    let mut buf_b = [0u8; 4096];
    let (len_b, src_b) = socket_b.recv_from(&mut buf_b).await.expect("recv at B");
    assert_eq!(src_b, addr_a);

    let query_packet = simple_dns::Packet::parse(&buf_b[..len_b]).expect("parse query packet");
    let decoded_query = decode_packet(&query_packet, src_b);
    assert_eq!(decoded_query, DecodedPacket::Query);

    // 3. Peer B responds with an announcement packet directed back to Peer A
    let node_b_id = "node-peer-b-loopback-responder";
    let node_b_name = "Peer B Laptop";
    let node_b_port = 7171;
    let b_ips = vec![Ipv4Addr::new(127, 0, 0, 1)];

    let announcement_bytes = build_announcement_bytes(
        node_b_id,
        node_b_name,
        OsType::Linux,
        node_b_port,
        &b_ips,
        DEFAULT_ANNOUNCEMENT_TTL,
    )
    .expect("build announcement at B");

    socket_b
        .send_to(&announcement_bytes, src_b)
        .await
        .expect("send announcement back to A");

    // 4. Peer A receives and decodes Peer B's announcement
    let mut buf_a = [0u8; 4096];
    let (len_a, src_a) = socket_a.recv_from(&mut buf_a).await.expect("recv at A");
    assert_eq!(src_a, addr_b);

    let peer_packet = simple_dns::Packet::parse(&buf_a[..len_a]).expect("parse peer packet");
    let decoded_peer = decode_packet(&peer_packet, src_a);

    match decoded_peer {
        DecodedPacket::Peer(peer) => {
            assert_eq!(peer.node_id, node_b_id);
            assert_eq!(peer.display_name, node_b_name);
            assert_eq!(peer.os_type, OsType::Linux);
            assert_eq!(peer.service_port, node_b_port);

            let ui = peer.to_ui_state().expect("UI state for loopback peer");
            assert_eq!(ui.node_id, node_b_id);
            assert_eq!(
                ui.primary_addr,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), node_b_port)
            );
        }
        other => panic!("Expected DecodedPacket::Peer, got {:?}", other),
    }
}
