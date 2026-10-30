use std::net::SocketAddr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use qshare::core::p2p::{P2pEngine, P2pError, RouteType, detect_route_type};
use qshare::core::ticket::{QshareTicket, TicketError};

#[test]
fn test_ticket_serialization_and_deserialization_roundtrip() {
    let secret = iroh::SecretKey::generate();
    let endpoint_id = secret.public();
    let content_hash = blake3::hash(b"test qshare payload bytes");
    let payload_size = 1024 * 1024 * 42; // 42 MB
    let relay_url = Some("https://relay.qshare.io".to_string());
    let direct_addresses: Vec<SocketAddr> = vec![
        "192.168.1.50:4567".parse().unwrap(),
        "[fe80::1]:4567".parse().unwrap(),
    ];

    let ticket = QshareTicket::new(
        endpoint_id,
        content_hash,
        payload_size,
        relay_url,
        direct_addresses,
    )
    .expect("Valid ticket creation should succeed");

    // Serialize to Base32
    let base32_str = ticket
        .to_base32()
        .expect("to_base32 serialization should succeed");
    assert!(!base32_str.is_empty());

    // Roundtrip from Base32
    let decoded =
        QshareTicket::from_base32(&base32_str).expect("from_base32 deserialization should succeed");
    assert_eq!(ticket, decoded);

    // Verify URI scheme support ("qshare://...")
    let prefixed = format!("qshare://{}", base32_str);
    let decoded_prefixed = QshareTicket::from_base32(&prefixed)
        .expect("from_base32 with qshare:// prefix should succeed");
    assert_eq!(ticket, decoded_prefixed);
}

#[test]
fn test_ticket_rejects_ssrf_relay_url() {
    let secret = iroh::SecretKey::generate();
    let endpoint_id = secret.public();
    let content_hash = blake3::hash(b"dummy");
    let payload_size = 1024;
    let direct_addresses = vec![];

    let malicious_urls = vec![
        ("http://relay.example.com", "Plaintext HTTP forbidden"),
        ("https://127.0.0.1:8080", "Loopback IPv4 forbidden"),
        ("https://127.0.0.2:443", "Loopback subnet forbidden"),
        (
            "https://169.254.169.254/latest/meta-data",
            "Cloud metadata link-local forbidden",
        ),
        ("https://10.0.0.1:443", "Private class A forbidden"),
        ("https://192.168.1.1:443", "Private class C forbidden"),
        ("https://172.16.0.1:443", "Private class B forbidden"),
        ("https://100.64.0.1:443", "CGNAT IPv4 forbidden"),
        ("https://240.0.0.1:443", "Reserved IPv4 forbidden"),
        ("https://224.0.0.1:443", "Multicast IPv4 forbidden"),
        ("https://[::1]:443", "IPv6 loopback forbidden"),
        (
            "https://[::ffff:127.0.0.1]:443",
            "IPv4-mapped loopback forbidden",
        ),
        (
            "https://[::ffff:169.254.169.254]:443",
            "IPv4-mapped metadata forbidden",
        ),
        (
            "https://[::127.0.0.1]:443",
            "IPv4-compatible loopback forbidden",
        ),
        ("https://[fe80::1]:443", "IPv6 link-local forbidden"),
        ("https://[fc00::1]:443", "IPv6 unique local forbidden"),
        ("https://[ff02::1]:443", "IPv6 multicast forbidden"),
        ("https://localhost:443", "Localhost domain forbidden"),
        (
            "https://localhost.:443",
            "Localhost domain with trailing dot forbidden",
        ),
        ("https://sub.localhost", ".localhost domain forbidden"),
        ("https://router.local", ".local mDNS domain forbidden"),
        ("https://router.lan:443", ".lan domain forbidden"),
        ("https://corp.internal:443", ".internal domain forbidden"),
        (
            "https://device.home.arpa:443",
            ".home.arpa domain forbidden",
        ),
        ("https://0.0.0.1:443", "0.0.0.0/8 network forbidden"),
        (
            "https://[64:ff9b::127.0.0.1]:443",
            "NAT64 loopback forbidden",
        ),
        (
            "https://[64:ff9b::169.254.169.254]:443",
            "NAT64 metadata forbidden",
        ),
        ("ftp://relay.qshare.io", "Non-HTTPS scheme forbidden"),
    ];

    for (url, description) in malicious_urls {
        let result = QshareTicket::new(
            endpoint_id,
            content_hash,
            payload_size,
            Some(url.to_string()),
            direct_addresses.clone(),
        );

        assert!(
            matches!(result, Err(TicketError::UntrustedRelayUrl(_))),
            "Expected UntrustedRelayUrl for '{url}' ({description}), got: {result:?}"
        );
    }

    // Verify oversized relay URL (> 1024 bytes) is rejected
    let oversized_url = format!("https://relay.qshare.io/{}", "a".repeat(1024));
    let oversized_res = QshareTicket::new(
        endpoint_id,
        content_hash,
        payload_size,
        Some(oversized_url),
        direct_addresses.clone(),
    );
    assert!(
        matches!(oversized_res, Err(TicketError::UntrustedRelayUrl(_))),
        "Oversized relay URL should be rejected"
    );

    // Verify valid public HTTPS relay URL is accepted
    let valid_result = QshareTicket::new(
        endpoint_id,
        content_hash,
        payload_size,
        Some("https://relay.qshare.io".to_string()),
        direct_addresses,
    );
    assert!(valid_result.is_ok());
}

#[test]
fn test_ticket_corrupted_and_edge_case_strings() {
    let secret = iroh::SecretKey::generate();
    let endpoint_id = secret.public();
    let content_hash = blake3::hash(b"corrupted edge case test");
    let payload_size = 8192;
    let relay_url = Some("https://relay.qshare.io".to_string());
    let direct_addresses: Vec<SocketAddr> = vec!["192.168.1.100:4567".parse().unwrap()];

    let ticket = QshareTicket::new(
        endpoint_id,
        content_hash,
        payload_size,
        relay_url,
        direct_addresses,
    )
    .expect("Valid ticket creation should succeed");

    let canonical_base32 = ticket.to_base32().expect("Base32 encoding must succeed");

    // 1. Lowercase normalization
    let lowercased = canonical_base32.to_ascii_lowercase();
    let decoded_lower = QshareTicket::from_base32(&lowercased)
        .expect("from_base32 should accept lowercase strings");
    assert_eq!(ticket, decoded_lower);

    // 2. Trailing padding '=' removal
    let padded = format!("{canonical_base32}===");
    let decoded_padded = QshareTicket::from_base32(&padded)
        .expect("from_base32 should tolerate trailing '=' padding");
    assert_eq!(ticket, decoded_padded);

    // 3. Internal whitespace tolerance
    let mut with_whitespace = String::new();
    for (i, c) in canonical_base32.chars().enumerate() {
        with_whitespace.push(c);
        if i % 10 == 0 {
            with_whitespace.push_str(" \t\r\n ");
        }
    }
    let decoded_ws = QshareTicket::from_base32(&with_whitespace)
        .expect("from_base32 should tolerate internal whitespace");
    assert_eq!(ticket, decoded_ws);

    // 4. Scheme prefixes case-insensitivity
    let lower_prefix = format!("qshare://{canonical_base32}");
    let upper_prefix = format!("QSHARE://{canonical_base32}");
    assert_eq!(ticket, QshareTicket::from_base32(&lower_prefix).unwrap());
    assert_eq!(ticket, QshareTicket::from_base32(&upper_prefix).unwrap());

    // 5. Invalid characters
    let invalid_chars = "INVALID!BASE32!CHARS!$$$";
    assert!(QshareTicket::from_base32(invalid_chars).is_err());

    // 6. Truncated ticket
    let truncated = &canonical_base32[..canonical_base32.len() / 2];
    assert!(QshareTicket::from_base32(truncated).is_err());

    // 7. Empty and whitespace-only strings
    assert!(matches!(
        QshareTicket::from_base32(""),
        Err(TicketError::InvalidFormat(_))
    ));
    assert!(matches!(
        QshareTicket::from_base32("   "),
        Err(TicketError::InvalidFormat(_))
    ));
    assert!(matches!(
        QshareTicket::from_base32("qshare://"),
        Err(TicketError::InvalidFormat(_))
    ));
    assert!(matches!(
        QshareTicket::from_base32("qshare://   ==="),
        Err(TicketError::InvalidFormat(_))
    ));

    // Non-ASCII and multi-byte UTF-8 inputs must fail gracefully without panic
    assert!(QshareTicket::from_base32("ééééé").is_err());
    assert!(QshareTicket::from_base32("🔥🚀🎉💻🔑").is_err());
    assert!(QshareTicket::from_base32("qshare://ééééé").is_err());

    // Valid Base32 of arbitrary non-postcard payload returns DeserializationFailed
    let non_postcard_b32 = data_encoding::BASE32_NOPAD.encode(b"arbitrary invalid postcard bytes");
    assert!(matches!(
        QshareTicket::from_base32(&non_postcard_b32),
        Err(TicketError::DeserializationFailed(_))
    ));

    // 8. Oversized ticket string (> 4096 bytes)
    let oversized_str = "A".repeat(4097);
    assert!(matches!(
        QshareTicket::from_base32(&oversized_str),
        Err(TicketError::InvalidFormat(_))
    ));

    // 9. Too many direct addresses (> 32)
    let excessive_addresses: Vec<SocketAddr> = (0..33)
        .map(|i| {
            SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, ((i % 250) + 1) as u8)),
                4000 + i as u16,
            )
        })
        .collect();
    let excessive_res = QshareTicket::new(
        endpoint_id,
        content_hash,
        payload_size,
        None,
        excessive_addresses,
    );
    assert!(matches!(excessive_res, Err(TicketError::InvalidFormat(_))));
}

#[tokio::test]
async fn test_p2p_engine_lifecycle_and_ticket_generation() {
    let engine = P2pEngine::new()
        .await
        .expect("P2pEngine binding should succeed");

    let endpoint_id = engine.endpoint_id();
    let endpoint_addr = engine.endpoint_addr();
    assert_eq!(endpoint_id, endpoint_addr.id);

    let content_bytes = b"verification of p2p engine lifecycle";
    let hash = blake3::hash(content_bytes);
    let size = content_bytes.len() as u64;

    let ticket = engine
        .create_ticket(hash, size)
        .expect("create_ticket should succeed");

    assert_eq!(ticket.endpoint_id, endpoint_id);
    assert_eq!(ticket.content_hash, hash);
    assert_eq!(ticket.payload_size, size);

    // Verify roundtrip encoding
    let encoded = ticket.to_base32().expect("Base32 encoding must succeed");
    let decoded = QshareTicket::from_base32(&encoded).expect("Base32 decoding must succeed");
    assert_eq!(ticket, decoded);
}

#[tokio::test]
async fn test_p2p_in_memory_loopback_connection() {
    let sender = P2pEngine::new()
        .await
        .expect("Sender P2pEngine should bind");
    let receiver = P2pEngine::new()
        .await
        .expect("Receiver P2pEngine should bind");

    let payload = b"Hello from Qshare loopback integration test!";
    let hash = blake3::hash(payload);
    let size = payload.len() as u64;

    let ticket = sender
        .create_ticket(hash, size)
        .expect("Sender should create ticket");

    let sender_cancel = CancellationToken::new();
    let receiver_cancel = CancellationToken::new();

    let sender_cancel_clone = sender_cancel.clone();
    let sender_engine = sender.clone();

    // Spawn sender accept loop
    let sender_task = tokio::spawn(async move { sender_engine.accept(sender_cancel_clone).await });

    // Receiver connects using sender's ticket
    let receiver_conn = tokio::time::timeout(
        Duration::from_secs(5),
        receiver.connect(&ticket, receiver_cancel),
    )
    .await
    .expect("Receiver connect should not time out")
    .expect("Receiver should connect to sender");

    let sender_conn = tokio::time::timeout(Duration::from_secs(5), sender_task)
        .await
        .expect("Sender accept should not time out")
        .expect("Sender task should join")
        .expect("Sender should successfully accept receiver connection");

    // Open bidirectional stream and verify end-to-end data transfer
    let (mut client_send, mut client_recv) = receiver_conn
        .open_bi()
        .await
        .expect("Receiver should open bidirectional stream");

    // Server accepts bidirectional stream concurrently and returns connection to keep it alive
    let server_stream_task = tokio::spawn(async move {
        let (mut server_send, mut server_recv) = sender_conn
            .accept_bi()
            .await
            .expect("Sender should accept bidirectional stream");

        let server_buf = server_recv
            .read_to_end(1024 * 1024)
            .await
            .expect("Server read should succeed");

        let ack = b"ACK: received payload";
        server_send
            .write_all(ack)
            .await
            .expect("Server write should succeed");
        server_send.finish().expect("Server finish should succeed");

        (server_buf, sender_conn)
    });

    // Client sends payload and finishes stream
    client_send
        .write_all(payload)
        .await
        .expect("Client write should succeed");
    client_send.finish().expect("Client finish should succeed");

    // Server verifies received payload
    let (server_buf, _sender_conn) = server_stream_task
        .await
        .expect("Server stream task should join");
    assert_eq!(&server_buf, payload);
    assert_eq!(blake3::hash(&server_buf), hash);

    // Client receives server acknowledgment
    let client_buf = client_recv
        .read_to_end(1024 * 1024)
        .await
        .expect("Client read should succeed");
    assert_eq!(&client_buf, b"ACK: received payload");

    // Verify route qualification
    let route = detect_route_type(&receiver_conn);
    assert!(
        matches!(route, RouteType::LocalLan | RouteType::DirectP2P),
        "Loopback connection should qualify as LocalLan or DirectP2P, got: {route:?}"
    );
}

#[tokio::test]
async fn test_p2p_cancellation_aborts_cleanly() {
    let engine = P2pEngine::new()
        .await
        .expect("P2pEngine binding should succeed");

    let dummy_secret = iroh::SecretKey::generate();
    let dummy_ticket = QshareTicket::new(
        dummy_secret.public(),
        blake3::hash(b"dummy"),
        100,
        None,
        vec!["192.0.2.1:9999".parse().unwrap()], // TEST-NET-1 (unreachable)
    )
    .expect("Dummy ticket creation should succeed");

    // 1. Pre-cancelled accept
    let cancel_pre = CancellationToken::new();
    cancel_pre.cancel();
    let res = engine.accept(cancel_pre).await;
    assert!(
        matches!(res, Err(P2pError::Cancelled)),
        "Pre-cancelled accept should return Cancelled, got {res:?}"
    );

    // 2. Pre-cancelled connect
    let cancel_pre2 = CancellationToken::new();
    cancel_pre2.cancel();
    let res = engine.connect(&dummy_ticket, cancel_pre2).await;
    assert!(
        matches!(res, Err(P2pError::Cancelled)),
        "Pre-cancelled connect should return Cancelled, got {res:?}"
    );

    // 3. In-flight accept cancellation
    let cancel_accept = CancellationToken::new();
    let cancel_accept_clone = cancel_accept.clone();
    let engine_clone = engine.clone();

    let accept_task = tokio::spawn(async move { engine_clone.accept(cancel_accept_clone).await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel_accept.cancel();

    let accept_res = tokio::time::timeout(Duration::from_secs(2), accept_task)
        .await
        .expect("accept task should abort quickly upon cancellation")
        .expect("Task join should succeed");
    assert!(
        matches!(accept_res, Err(P2pError::Cancelled)),
        "In-flight cancelled accept should return Cancelled, got {accept_res:?}"
    );

    // 4. In-flight connect cancellation
    let cancel_connect = CancellationToken::new();
    let cancel_connect_clone = cancel_connect.clone();
    let engine_clone2 = engine.clone();

    let connect_task = tokio::spawn(async move {
        engine_clone2
            .connect(&dummy_ticket, cancel_connect_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel_connect.cancel();

    let connect_res = tokio::time::timeout(Duration::from_secs(2), connect_task)
        .await
        .expect("connect task should abort quickly upon cancellation")
        .expect("Task join should succeed");
    assert!(
        matches!(connect_res, Err(P2pError::Cancelled)),
        "In-flight cancelled connect should return Cancelled, got {connect_res:?}"
    );
}
