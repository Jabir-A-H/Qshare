#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Size, Theme, window};

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Catch any panics and show a native Windows dialog instead of dying silently
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("Qshare encountered an unexpected panic:\n\n{info}");
        let _ = std::fs::write("qshare_crash.log", &msg);
        rfd::MessageDialog::new()
            .set_title("Qshare Error")
            .set_description(&msg)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }));

    // Initialize structured logging
    tracing_subscriber::fmt::init();
    tracing::info!("Launching Qshare desktop application...");

    let res = iced::application("Qshare (⚡)", Qshare::update, Qshare::view)
        .window(window::Settings {
            size: Size::new(850.0, 650.0),
            min_size: Some(Size::new(600.0, 450.0)),
            position: window::Position::Centered,
            exit_on_close_request: true,
            ..Default::default()
        })
        .theme(|_| Theme::Dark)
        .run();

    if let Err(e) = res {
        let msg = format!("Failed to launch Qshare window:\n\n{e}");
        let _ = std::fs::write("qshare_crash.log", &msg);
        rfd::MessageDialog::new()
            .set_title("Qshare Launch Error")
            .set_description(&msg)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }

    Ok(())
}

#[derive(Default)]
struct Qshare {
    active_tab: Tab,
    transfer_status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    LocalLan,
    SendAnywhere,
}

#[derive(Debug, Clone)]
enum Message {
    TabChanged(Tab),
    SelectFile,
}

impl Qshare {
    fn update(&mut self, message: Message) {
        match message {
            Message::TabChanged(tab) => {
                self.active_tab = tab;
                tracing::info!("Active tab switched to: {:?}", tab);
            }
            Message::SelectFile => {
                self.transfer_status = "Ready to select file".to_string();
                tracing::info!("Select file action triggered");
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let title = text("Qshare (⚡)").size(32);

        let subtitle = text("Zero-Cloud Hybrid P2P Dropzone · 100% Pure Rust").size(13);

        let nav = row![
            button(text("Nearby LAN Radar")).on_press(Message::TabChanged(Tab::LocalLan)),
            button(text("Send Anywhere (Internet)"))
                .on_press(Message::TabChanged(Tab::SendAnywhere)),
        ]
        .spacing(12);

        let tab_body = match self.active_tab {
            Tab::LocalLan => column![
                text("Looking for nearby devices on your Wi-Fi router via mDNS (multicast)...")
                    .size(15),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
            Tab::SendAnywhere => column![
                text("Drag & drop files or click to generate an encrypted one-time Share Ticket.")
                    .size(15),
                button(text("Select File to Share")).on_press(Message::SelectFile),
                text(&self.transfer_status).size(13),
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        };

        let content = column![title, subtitle, nav, container(tab_body).padding(20),]
            .spacing(16)
            .align_x(Alignment::Center);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
