use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::app::{Message, Tab};
use crate::ui::theme;

pub fn view_nav<'a>(active_tab: Tab, peer_count: usize) -> Element<'a, Message> {
    let title = text("Qshare (⚡)").size(26);

    let version_badge = container(
        text(concat!("v", env!("CARGO_PKG_VERSION")))
            .size(11)
            .color(theme::COLOR_ACCENT),
    )
    .padding([2, 8])
    .style(|_theme| container::Style {
        background: Some(iced::Background::Color(iced::Color::from_rgba(
            0.24, 0.51, 0.96, 0.15,
        ))),
        border: iced::Border {
            color: theme::COLOR_ACCENT,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Default::default()
    });

    let header_left = row![title, version_badge]
        .spacing(10)
        .align_y(Alignment::Center);

    let status_text = match active_tab {
        Tab::LocalLan => {
            if peer_count > 0 {
                format!("● LAN Radar Active ({peer_count} peers)")
            } else {
                "● LAN Radar Scanning (mDNS 5353)...".to_string()
            }
        }
        Tab::SendAnywhere => "● Direct P2P & Blind Relay Ready".to_string(),
    };

    let status_color = match active_tab {
        Tab::LocalLan => {
            if peer_count > 0 {
                theme::COLOR_ROUTE_LAN
            } else {
                theme::COLOR_TEXT_MUTED
            }
        }
        Tab::SendAnywhere => theme::COLOR_ROUTE_P2P,
    };

    let status_indicator = text(status_text).size(12).color(status_color);

    let header_row = row![
        header_left,
        iced::widget::horizontal_space(),
        status_indicator
    ]
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let lan_label = if peer_count > 0 {
        format!("Nearby LAN Radar ({peer_count})")
    } else {
        "Nearby LAN Radar".to_string()
    };

    let lan_btn = button(text(lan_label).size(13))
        .padding([8, 16])
        .style(theme::tab_button_style(active_tab == Tab::LocalLan))
        .on_press(Message::TabChanged(Tab::LocalLan));

    let anywhere_btn = button(text("Send Anywhere (Internet)").size(13))
        .padding([8, 16])
        .style(theme::tab_button_style(active_tab == Tab::SendAnywhere))
        .on_press(Message::TabChanged(Tab::SendAnywhere));

    let tabs_row = row![lan_btn, anywhere_btn]
        .spacing(10)
        .align_y(Alignment::Center);

    column![header_row, tabs_row]
        .spacing(14)
        .width(Length::Fill)
        .into()
}
