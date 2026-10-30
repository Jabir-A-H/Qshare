use std::path::Path;

use iced::mouse;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path as CanvasPath, Stroke};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme};

use crate::app::{Message, Tab};
use crate::core::discovery::{LanPeerState, OsType};
use crate::ui::theme;

pub fn view_radar<'a>(
    lan_peers: &'a [LanPeerState],
    selected_file: Option<&'a Path>,
) -> Element<'a, Message> {
    if lan_peers.is_empty() {
        view_empty_radar()
    } else {
        view_peer_list(lan_peers, selected_file)
    }
}

fn view_empty_radar<'a>() -> Element<'a, Message> {
    let radar_canvas = Canvas::new(RadarCanvas { peers: Vec::new() })
        .width(Length::Fixed(200.0))
        .height(Length::Fixed(200.0));

    let scan_title = text("Scanning Local Wi-Fi Network...")
        .size(17)
        .color(theme::COLOR_TEXT_PRIMARY);

    let scan_desc = text(
        "Looking for nearby Qshare devices on the same Wi-Fi subnet via mDNS multicast (port 5353).",
    )
    .size(13)
    .color(theme::COLOR_TEXT_MUTED);

    // KI-002 Mitigation: Prominent Wi-Fi Isolation fallback card
    let fallback_card = container(
        column![
            row![
                text("💡").size(16),
                text("Can't see nearby device?")
                    .size(14)
                    .color(theme::COLOR_TEXT_PRIMARY),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(
                "If devices are on the same Wi-Fi but don't show up, your router might have \
                 Wi-Fi Client Isolation or AP Isolation enabled. You can bypass local network \
                 restrictions instantly using Send Anywhere Tickets.",
            )
            .size(12)
            .color(theme::COLOR_TEXT_MUTED),
            button(text("Switch to Send Anywhere Ticket →").size(13))
                .padding([8, 16])
                .style(theme::primary_button_style)
                .on_press(Message::TabChanged(Tab::SendAnywhere)),
        ]
        .spacing(10),
    )
    .padding(16)
    .style(theme::card_container_style())
    .max_width(520);

    column![
        radar_canvas,
        scan_title,
        scan_desc,
        iced::widget::vertical_space().height(8),
        fallback_card,
    ]
    .spacing(12)
    .align_x(Alignment::Center)
    .into()
}

fn view_peer_list<'a>(
    peers: &'a [LanPeerState],
    selected_file: Option<&'a Path>,
) -> Element<'a, Message> {
    let mut peer_cards = column![].spacing(10).width(Length::Fill);

    for peer in peers {
        let os_str = match peer.os_type {
            OsType::Windows => "WINDOWS",
            OsType::MacOS => "MACOS",
            OsType::Linux => "LINUX",
            OsType::Android => "ANDROID",
            OsType::IOS => "IOS",
            OsType::Unknown => "DEVICE",
        };

        let os_badge = container(text(os_str).size(10).color(theme::COLOR_TEXT_PRIMARY))
            .padding([2, 6])
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(theme::COLOR_SURFACE_ELEVATED)),
                border: iced::Border {
                    color: theme::COLOR_BORDER,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });

        let status_dot = if peer.is_stale {
            text("● Stale").size(12).color(theme::COLOR_ROUTE_RELAY)
        } else {
            text("● Online").size(12).color(theme::COLOR_ROUTE_LAN)
        };

        let device_info = column![
            row![text(&peer.display_name).size(15), os_badge]
                .spacing(8)
                .align_y(Alignment::Center),
            row![
                text(peer.primary_addr.to_string())
                    .size(12)
                    .color(theme::COLOR_TEXT_MUTED),
                status_dot,
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(4);

        let action_btn = if let Some(file) = selected_file {
            let filename = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            button(text(format!("Send '{filename}' →")).size(13))
                .padding([6, 14])
                .style(theme::primary_button_style)
                .on_press(Message::SendToPeer(peer.clone()))
        } else {
            button(text("Select File to Send").size(13))
                .padding([6, 14])
                .style(theme::secondary_button_style)
                .on_press(Message::SendToPeer(peer.clone()))
        };

        let card = container(
            row![device_info, iced::widget::horizontal_space(), action_btn,]
                .spacing(12)
                .align_y(Alignment::Center)
                .width(Length::Fill),
        )
        .padding(14)
        .style(theme::card_container_style())
        .width(Length::Fill);

        peer_cards = peer_cards.push(card);
    }

    // Mini fallback reminder at the bottom
    let hint_row = row![
        text("Can't find a device?")
            .size(12)
            .color(theme::COLOR_TEXT_MUTED),
        button(text("Use Send Anywhere Ticket →").size(12))
            .padding([2, 8])
            .style(theme::secondary_button_style)
            .on_press(Message::TabChanged(Tab::SendAnywhere)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let radar_canvas = Canvas::new(RadarCanvas {
        peers: peers.to_vec(),
    })
    .width(Length::Fixed(140.0))
    .height(Length::Fixed(140.0));

    let scrollable_peers = scrollable(peer_cards).width(Length::Fill);

    column![
        row![radar_canvas, scrollable_peers]
            .spacing(20)
            .align_y(Alignment::Start)
            .width(Length::Fill),
        iced::widget::vertical_space().height(6),
        hint_row,
    ]
    .spacing(12)
    .width(Length::Fill)
    .into()
}

// ==========================================
// Radar Canvas Program
// ==========================================

#[derive(Debug, Clone)]
pub struct RadarCanvas {
    pub peers: Vec<LanPeerState>,
}

impl<Message> canvas::Program<Message> for RadarCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let max_radius = (bounds.width.min(bounds.height) / 2.0) - 8.0;

        if max_radius <= 0.0 {
            return vec![frame.into_geometry()];
        }

        // Concentric radar rings
        for step in 1..=3 {
            let r = max_radius * (step as f32 / 3.0);
            let circle = CanvasPath::circle(center, r);
            frame.stroke(
                &circle,
                Stroke::default()
                    .with_color(Color::from_rgba(0.24, 0.51, 0.96, 0.25))
                    .with_width(1.0),
            );
        }

        // Crosshairs
        let horiz = CanvasPath::line(
            Point::new(center.x - max_radius, center.y),
            Point::new(center.x + max_radius, center.y),
        );
        frame.stroke(
            &horiz,
            Stroke::default()
                .with_color(Color::from_rgba(0.24, 0.51, 0.96, 0.15))
                .with_width(1.0),
        );

        let vert = CanvasPath::line(
            Point::new(center.x, center.y - max_radius),
            Point::new(center.x, center.y + max_radius),
        );
        frame.stroke(
            &vert,
            Stroke::default()
                .with_color(Color::from_rgba(0.24, 0.51, 0.96, 0.15))
                .with_width(1.0),
        );

        // Center dot (local device)
        let center_dot = CanvasPath::circle(center, 4.0);
        frame.fill(&center_dot, theme::COLOR_ACCENT);

        // Render discovered peer dots
        for peer in &self.peers {
            let angle_rad = peer.polar_angle_deg.to_radians();
            let dist = max_radius * peer.distance_ratio.clamp(0.25, 0.9);
            let peer_pos = Point::new(
                center.x + angle_rad.cos() * dist,
                center.y + angle_rad.sin() * dist,
            );

            let dot = CanvasPath::circle(peer_pos, 5.0);
            let dot_color = if peer.is_stale {
                theme::COLOR_ROUTE_RELAY
            } else {
                theme::COLOR_ROUTE_LAN
            };
            frame.fill(&dot, dot_color);
        }

        vec![frame.into_geometry()]
    }
}
