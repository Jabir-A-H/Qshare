use std::net::SocketAddr;
use std::path::PathBuf;

use qshare::app::{Message, QshareApp, Tab, TransferStatus};
use qshare::core::discovery::{LanPeerState, OsType};
use qshare::core::p2p::{RouteBadge, RouteType};
use qshare::ui::progress_modal::{format_bytes, format_eta, format_speed, sanitize_display_name};

#[test]
fn test_tab_changed_transitions_active_tab() {
    let mut app = QshareApp::new();
    assert_eq!(app.active_tab, Tab::LocalLan);

    let _ = app.update(Message::TabChanged(Tab::SendAnywhere));
    assert_eq!(app.active_tab, Tab::SendAnywhere);

    let _ = app.update(Message::TabChanged(Tab::LocalLan));
    assert_eq!(app.active_tab, Tab::LocalLan);
}

#[test]
fn test_peer_discovered_lost_updated() {
    let mut app = QshareApp::new();
    assert!(app.lan_peers.is_empty());

    let peer = LanPeerState {
        node_id: "peer-node-1".to_string(),
        display_name: "MacBook Air".to_string(),
        os_type: OsType::MacOS,
        primary_addr: "192.168.1.50:5353".parse::<SocketAddr>().unwrap(),
        polar_angle_deg: 45.0,
        distance_ratio: 0.5,
        is_stale: false,
    };

    let _ = app.update(Message::PeerDiscovered(peer.clone()));
    assert_eq!(app.lan_peers.len(), 1);
    assert_eq!(app.lan_peers[0].display_name, "MacBook Air");

    // PeerUpdated updates in-place
    let mut updated_peer = peer.clone();
    updated_peer.is_stale = true;
    let _ = app.update(Message::PeerUpdated(updated_peer));
    assert_eq!(app.lan_peers.len(), 1);
    assert!(app.lan_peers[0].is_stale);

    // PeerLost removes peer
    let _ = app.update(Message::PeerLost("peer-node-1".to_string()));
    assert!(app.lan_peers.is_empty());
}

#[test]
fn test_file_selected_and_dropped() {
    let mut app = QshareApp::new();
    assert!(app.selected_file.is_none());
    assert!(app.selected_file_size.is_none());
    assert!(app.generated_ticket.is_none());

    // Dropping a directory should be rejected
    let dir_path = std::env::current_dir().unwrap();
    let _ = app.update(Message::FileDropped(dir_path));
    assert!(app.selected_file.is_none());
    assert!(
        app.toast_message
            .as_ref()
            .unwrap()
            .0
            .contains("Folder transfers not yet supported")
    );

    // Dropping Cargo.toml (an actual file) populates selected_file and selected_file_size
    let cargo_path = PathBuf::from("Cargo.toml");
    let _ = app.update(Message::FileDropped(cargo_path.clone()));
    assert_eq!(app.selected_file, Some(cargo_path.clone()));
    assert!(app.selected_file_size.is_some());
    assert!(app.selected_file_size.unwrap() > 0);

    let test_path = PathBuf::from("test_payload.iso");
    let _ = app.update(Message::FileSelected(Some(test_path.clone())));
    assert_eq!(app.selected_file, Some(test_path));
    assert!(app.generated_ticket.is_some());
    let ticket = app.generated_ticket.as_ref().unwrap();
    assert!(ticket.starts_with("blob:"));
}

#[test]
fn test_ticket_input_changed() {
    let mut app = QshareApp::new();
    assert!(app.input_ticket.is_empty());

    let _ = app.update(Message::TicketInputChanged("blob:abc123456789".to_string()));
    assert_eq!(app.input_ticket, "blob:abc123456789");
}

#[test]
fn test_progress_updated_state() {
    let mut app = QshareApp::new();
    assert_eq!(app.transfer_status, TransferStatus::Idle);

    let _ = app.update(Message::ProgressUpdated {
        bytes_transferred: 50_000_000,
        total_bytes: 100_000_000,
        speed_bps: 12_500_000.0,
        eta_secs: Some(4),
    });

    match &app.transfer_status {
        TransferStatus::Transferring {
            bytes_transferred,
            total_bytes,
            speed_bps,
            eta_secs,
            ..
        } => {
            assert_eq!(*bytes_transferred, 50_000_000);
            assert_eq!(*total_bytes, 100_000_000);
            assert_eq!(*speed_bps, 12_500_000.0);
            assert_eq!(*eta_secs, Some(4));
        }
        other => panic!("Expected TransferStatus::Transferring, got {:?}", other),
    }

    assert_eq!(app.transfer_status.progress_ratio(), 0.5);

    // Cancel transfer resets to Idle
    let _ = app.update(Message::CancelTransferClicked);
    assert_eq!(app.transfer_status, TransferStatus::Idle);
}

#[test]
fn test_transfer_completed_success_and_failure() {
    let mut app = QshareApp::new();

    let _ = app.update(Message::TransferCompleted(Ok(())));
    match &app.transfer_status {
        TransferStatus::Completed { filename } => {
            assert_eq!(filename, "Transfer");
        }
        other => panic!("Expected Completed, got {:?}", other),
    }

    let _ = app.update(Message::TransferCompleted(Err(
        "Connection reset by peer".to_string()
    )));
    match &app.transfer_status {
        TransferStatus::Failed { error } => {
            assert_eq!(error, "Connection reset by peer");
        }
        other => panic!("Expected Failed, got {:?}", other),
    }
}

#[test]
fn test_window_close_intercept_during_active_transfer() {
    let mut app = QshareApp::new();
    assert!(!app.show_close_confirm);

    // When idle, close request does not prompt confirm modal
    let _ = app.update(Message::WindowCloseRequested);
    assert!(!app.show_close_confirm);

    // When transferring, close request sets show_close_confirm = true
    app.transfer_status = TransferStatus::Transferring {
        filename: "large_archive.zip".to_string(),
        bytes_transferred: 1000,
        total_bytes: 2000,
        speed_bps: 500.0,
        eta_secs: Some(2),
        route: RouteType::DirectP2P,
        is_sender: false,
    };

    let _ = app.update(Message::WindowCloseRequested);
    assert!(app.show_close_confirm);

    // Dismiss close confirm retains transfer
    let _ = app.update(Message::DismissCloseConfirm);
    assert!(!app.show_close_confirm);
    assert!(app.transfer_status.is_active());

    // Confirm close resets transfer
    let _ = app.update(Message::CloseConfirmed);
    assert!(!app.show_close_confirm);
    assert_eq!(app.transfer_status, TransferStatus::Idle);
}

#[test]
fn test_connect_ticket_empty_vs_valid() {
    let mut app = QshareApp::new();

    // 1. Empty ticket input should show toast and remain Idle
    app.input_ticket = "   ".to_string();
    let _ = app.update(Message::ConnectTicketClicked);
    assert_eq!(app.transfer_status, TransferStatus::Idle);
    assert!(
        app.toast_message
            .as_ref()
            .unwrap()
            .0
            .contains("Please enter a valid ticket")
    );

    // 2. Valid ticket input should transition to Connecting
    app.input_ticket = "blob:abcdef1234567890".to_string();
    let _ = app.update(Message::ConnectTicketClicked);
    match &app.transfer_status {
        TransferStatus::Connecting { peer_name, route } => {
            assert!(peer_name.contains("Ticket"));
            assert_eq!(
                *route,
                RouteBadge::DirectP2P {
                    remote_node: "blob:abcdef1234567890".to_string(),
                    rtt_ms: 0,
                }
            );
        }
        other => panic!("Expected Connecting, got {:?}", other),
    }
}

#[test]
fn test_send_to_peer_transitions() {
    let mut app = QshareApp::new();
    let peer = LanPeerState {
        node_id: "node-xyz".to_string(),
        display_name: "MacBook Pro".to_string(),
        os_type: OsType::MacOS,
        primary_addr: "192.168.1.100:5353".parse::<SocketAddr>().unwrap(),
        polar_angle_deg: 90.0,
        distance_ratio: 0.3,
        is_stale: false,
    };

    // 1. SendToPeer without a file selected should prompt via toast
    let _ = app.update(Message::SendToPeer(peer.clone()));
    assert_eq!(app.transfer_status, TransferStatus::Idle);
    assert!(
        app.toast_message
            .as_ref()
            .unwrap()
            .0
            .contains("Please select a file to share first")
    );

    // 2. SendToPeer with file selected transitions to Connecting with LocalLan route
    app.selected_file = Some(PathBuf::from("photo.jpg"));
    let _ = app.update(Message::SendToPeer(peer.clone()));
    match &app.transfer_status {
        TransferStatus::Connecting { peer_name, route } => {
            assert_eq!(peer_name, "MacBook Pro");
            assert_eq!(*route, RouteBadge::LocalLan(peer.primary_addr));
        }
        other => panic!("Expected Connecting, got {:?}", other),
    }
}

#[test]
fn test_dismiss_modal_resets_to_idle() {
    let mut app = QshareApp::new();

    // From Completed
    app.transfer_status = TransferStatus::Completed {
        filename: "presentation.pdf".to_string(),
    };
    let _ = app.update(Message::DismissModal);
    assert_eq!(app.transfer_status, TransferStatus::Idle);

    // From Failed
    app.transfer_status = TransferStatus::Failed {
        error: "QUIC handshake timed out".to_string(),
    };
    let _ = app.update(Message::DismissModal);
    assert_eq!(app.transfer_status, TransferStatus::Idle);
}

#[test]
fn test_progress_ratio_zero_division_safety() {
    let status_zero_total = TransferStatus::Transferring {
        filename: "empty.bin".to_string(),
        bytes_transferred: 0,
        total_bytes: 0,
        speed_bps: 0.0,
        eta_secs: None,
        route: RouteType::LocalLan,
        is_sender: true,
    };
    assert_eq!(status_zero_total.progress_ratio(), 0.0);

    let status_idle = TransferStatus::Idle;
    assert_eq!(status_idle.progress_ratio(), 0.0);

    let status_completed = TransferStatus::Completed {
        filename: "done.bin".to_string(),
    };
    assert_eq!(status_completed.progress_ratio(), 1.0);
}

#[test]
fn test_format_helpers() {
    // format_bytes
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1_500), "1.5 KB");
    assert_eq!(format_bytes(2_500_000), "2.50 MB");
    assert_eq!(format_bytes(3_500_000_000), "3.50 GB");

    // format_speed
    assert_eq!(format_speed(250.0), "250 B/s");
    assert_eq!(format_speed(1_500.0), "1.5 KB/s");
    assert_eq!(format_speed(2_500_000.0), "2.5 MB/s");
    assert_eq!(format_speed(1_200_000_000.0), "1.20 GB/s");

    // format_eta
    assert_eq!(format_eta(None), "calculating...");
    assert_eq!(format_eta(Some(30)), "30s");
    assert_eq!(format_eta(Some(90)), "01m 30s");
    assert_eq!(format_eta(Some(3665)), "01h 01m");
}

#[test]
fn test_display_name_sanitization_security() {
    // Traversal attacks
    let traversal = "../../../Windows/System32/calc.exe";
    assert_eq!(sanitize_display_name(traversal), "calc.exe");

    let win_traversal = r"C:\Secret\..\..\malware.exe";
    assert_eq!(sanitize_display_name(win_traversal), "malware.exe");

    // Trailing slashes
    assert_eq!(sanitize_display_name("some/nested/dir/"), "dir");
    assert_eq!(sanitize_display_name(r"windows\path\folder\"), "folder");
    assert_eq!(sanitize_display_name("trailing///"), "trailing");

    // Relative directory operators and empty strings default to "unnamed_file"
    assert_eq!(sanitize_display_name(".."), "unnamed_file");
    assert_eq!(sanitize_display_name("../"), "unnamed_file");
    assert_eq!(sanitize_display_name(r"..\..\"), "unnamed_file");
    assert_eq!(sanitize_display_name("."), "unnamed_file");
    assert_eq!(sanitize_display_name("./"), "unnamed_file");
    assert_eq!(sanitize_display_name(""), "unnamed_file");
    assert_eq!(sanitize_display_name("///"), "unnamed_file");

    // RTL Unicode override (U+202E) spoofing (document\u{202E}gnp.exe -> documentexe.png)
    let rtl_spoof = "document\u{202E}gnp.exe";
    assert_eq!(sanitize_display_name(rtl_spoof), "documentgnp.exe");

    // Zero-width space and null bytes
    let hidden = "file\0\u{200B}.txt";
    assert_eq!(sanitize_display_name(hidden), "file.txt");
    assert_eq!(sanitize_display_name("payload\u{FEFF}.iso"), "payload.iso");
}
