use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length, Theme};

pub fn main() -> iced::Result {
    // Initialize structured terminal logging
    tracing_subscriber::fmt::init();

    tracing::info!("Launching Qshare desktop application...");

    iced::application("Qshare (⚡)", Qshare::update, Qshare::view)
        .theme(|_| Theme::Dark)
        .centered()
        .run()
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
