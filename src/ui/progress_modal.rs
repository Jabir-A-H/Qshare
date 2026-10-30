use iced::widget::{button, column, container, progress_bar, row, text};
use iced::{Alignment, Element, Length};

use crate::app::{Message, TransferStatus};
use crate::ui::theme;

pub fn view_progress_modal<'a>(status: &'a TransferStatus) -> Option<Element<'a, Message>> {
    match status {
        TransferStatus::Idle => None,
        TransferStatus::Connecting { peer_name, route } => {
            Some(view_connecting_modal(peer_name, route))
        }
        TransferStatus::Transferring {
            filename,
            bytes_transferred,
            total_bytes,
            speed_bps,
            eta_secs,
            route,
            is_sender,
        } => Some(view_transferring_modal(
            filename,
            *bytes_transferred,
            *total_bytes,
            *speed_bps,
            *eta_secs,
            *route,
            *is_sender,
        )),
        TransferStatus::Completed { filename } => Some(view_completed_modal(filename)),
        TransferStatus::Failed { error } => Some(view_failed_modal(error)),
    }
}

pub fn view_close_confirm_modal<'a>() -> Element<'a, Message> {
    let header = text("⚠️ Transfer in Progress")
        .size(18)
        .color(theme::COLOR_ROUTE_RELAY);

    let desc = text(
        "A file transfer is currently active. Closing Qshare will abort the transfer, \
         drop the peer connection, and clean up incomplete part files.",
    )
    .size(13)
    .color(theme::COLOR_TEXT_PRIMARY);

    let btn_keep = button(text("Continue Transfer").size(13))
        .padding([8, 16])
        .style(theme::primary_button_style)
        .on_press(Message::DismissCloseConfirm);

    let btn_abort = button(text("Abort and Quit").size(13))
        .padding([8, 16])
        .style(theme::danger_button_style)
        .on_press(Message::CloseConfirmed);

    let actions = row![btn_keep, btn_abort]
        .spacing(12)
        .align_y(Alignment::Center);

    let card = container(column![header, desc, actions].spacing(14))
        .padding(20)
        .style(theme::modal_content_style())
        .max_width(450);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::modal_overlay_style())
        .into()
}

fn view_connecting_modal<'a>(
    peer_name: &'a str,
    route: &'a crate::core::p2p::RouteBadge,
) -> Element<'a, Message> {
    let title = text("Connecting to Peer...")
        .size(18)
        .color(theme::COLOR_TEXT_PRIMARY);

    let desc = match route {
        crate::core::p2p::RouteBadge::LocalLan(addr) => {
            format!("Connecting directly to {peer_name} over local Wi-Fi ({addr})...")
        }
        crate::core::p2p::RouteBadge::DirectP2P { .. } => {
            format!("Establishing QUIC connection with {peer_name}. Swapping NAT coordinates...")
        }
        crate::core::p2p::RouteBadge::BlindRelay { relay_url, .. } => {
            format!("Connecting to {peer_name} via encrypted relay ({relay_url})...")
        }
    };

    let desc_widget = text(desc).size(13).color(theme::COLOR_TEXT_MUTED);

    let cancel_btn = button(text("Cancel").size(13))
        .padding([6, 16])
        .style(theme::danger_button_style)
        .on_press(Message::CancelTransferClicked);

    let card = container(column![title, desc_widget, cancel_btn].spacing(14))
        .padding(20)
        .style(theme::modal_content_style())
        .max_width(450);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::modal_overlay_style())
        .into()
}

fn view_transferring_modal<'a>(
    filename: &'a str,
    bytes_transferred: u64,
    total_bytes: u64,
    speed_bps: f64,
    eta_secs: Option<u64>,
    route: crate::core::p2p::RouteType,
    is_sender: bool,
) -> Element<'a, Message> {
    let sanitized_filename = sanitize_display_name(filename);

    let title_prefix = if is_sender {
        "Sending: "
    } else {
        "Receiving: "
    };

    let header_left = row![
        text(title_prefix).size(16).color(theme::COLOR_TEXT_MUTED),
        text(sanitized_filename)
            .size(16)
            .color(theme::COLOR_TEXT_PRIMARY),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let route_badge = container(text(route.to_string()).size(11))
        .padding([2, 8])
        .style(move |_| theme::route_badge_style(route));

    let header_row = row![header_left, iced::widget::horizontal_space(), route_badge]
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let ratio = if total_bytes == 0 {
        0.0
    } else {
        (bytes_transferred as f32 / total_bytes as f32).clamp(0.0, 1.0)
    };

    let pbar = progress_bar(0.0..=1.0, ratio)
        .height(8.0)
        .style(|_theme| progress_bar::Style {
            background: iced::Background::Color(theme::COLOR_SURFACE),
            bar: iced::Background::Color(theme::COLOR_ACCENT),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
        });

    let percent_str = format!("{:.1}%", ratio * 100.0);
    let bytes_str = format!(
        "{} / {}",
        format_bytes(bytes_transferred),
        format_bytes(total_bytes)
    );

    let speed_str = format_speed(speed_bps);
    let eta_str = format!("ETA: {}", format_eta(eta_secs));

    let metrics_row = row![
        text(bytes_str).size(12).color(theme::COLOR_TEXT_MUTED),
        iced::widget::horizontal_space(),
        text(percent_str).size(12).color(theme::COLOR_TEXT_PRIMARY),
    ]
    .width(Length::Fill);

    let telemetry_row = row![
        text(speed_str).size(12).color(theme::COLOR_ROUTE_LAN),
        iced::widget::horizontal_space(),
        text(eta_str).size(12).color(theme::COLOR_TEXT_MUTED),
    ]
    .width(Length::Fill);

    let cancel_btn = button(text("Cancel Transfer").size(13))
        .padding([8, 16])
        .style(theme::danger_button_style)
        .on_press(Message::CancelTransferClicked);

    let card = container(
        column![
            header_row,
            pbar,
            metrics_row,
            telemetry_row,
            iced::widget::vertical_space().height(4),
            cancel_btn,
        ]
        .spacing(10),
    )
    .padding(20)
    .style(theme::modal_content_style())
    .max_width(520);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::modal_overlay_style())
        .into()
}

fn view_completed_modal<'a>(filename: &'a str) -> Element<'a, Message> {
    let sanitized_filename = sanitize_display_name(filename);

    let header = text("Transfer Complete! ✓")
        .size(18)
        .color(theme::COLOR_ROUTE_LAN);

    let desc = text(format!(
        "'{sanitized_filename}' has been transferred and verified with BLAKE3."
    ))
    .size(13)
    .color(theme::COLOR_TEXT_PRIMARY);

    let close_btn = button(text("Done").size(13))
        .padding([8, 20])
        .style(theme::primary_button_style)
        .on_press(Message::DismissModal);

    let card = container(column![header, desc, close_btn].spacing(14))
        .padding(20)
        .style(theme::modal_content_style())
        .max_width(450);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::modal_overlay_style())
        .into()
}

fn view_failed_modal<'a>(error: &'a str) -> Element<'a, Message> {
    let header = text("Transfer Failed ✗")
        .size(18)
        .color(theme::COLOR_DANGER);

    let desc = text(error).size(13).color(theme::COLOR_TEXT_PRIMARY);

    let dismiss_btn = button(text("Dismiss").size(13))
        .padding([8, 20])
        .style(theme::secondary_button_style)
        .on_press(Message::DismissModal);

    let card = container(column![header, desc, dismiss_btn].spacing(14))
        .padding(20)
        .style(theme::modal_content_style())
        .max_width(450);

    container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::modal_overlay_style())
        .into()
}

// ==========================================
// Formatting & Security Sanitization Helpers
// ==========================================

/// Strips directory traversal (`../`) and Bi-directional Unicode overrides (`U+202E`, etc.)
/// mitigating UI spoofing vulnerability F-1-12.
pub fn sanitize_display_name(raw: &str) -> String {
    let trimmed = raw.trim_end_matches(['/', '\\']);

    let file_name = match std::path::Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
    {
        Some(name) => name,
        None => return "unnamed_file".to_string(),
    };

    let sanitized: String = file_name
        .chars()
        .filter(|&c| {
            !matches!(
                c,
                '\0' | '\x08'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{200B}'..='\u{200D}'
                    | '\u{FEFF}'
            )
        })
        .collect();

    if sanitized.is_empty() || sanitized == ".." || sanitized == "." {
        "unnamed_file".to_string()
    } else {
        sanitized
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

pub fn format_speed(bps: f64) -> String {
    if bps >= 1_000_000_000.0 {
        format!("{:.2} GB/s", bps / 1_000_000_000.0)
    } else if bps >= 1_000_000.0 {
        format!("{:.1} MB/s", bps / 1_000_000.0)
    } else if bps >= 1_000.0 {
        format!("{:.1} KB/s", bps / 1_000.0)
    } else {
        format!("{:.0} B/s", bps)
    }
}

pub fn format_eta(secs: Option<u64>) -> String {
    match secs {
        Some(s) if s >= 3600 => format!("{:02}h {:02}m", s / 3600, (s % 3600) / 60),
        Some(s) if s >= 60 => format!("{:02}m {:02}s", s / 60, s % 60),
        Some(s) => format!("{s}s"),
        None => "calculating...".to_string(),
    }
}
