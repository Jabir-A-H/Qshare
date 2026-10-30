use std::path::PathBuf;
use std::time::Instant;

use iced::widget::{column, container, stack};
use iced::{Alignment, Element, Length, Padding, Task};

use crate::core::discovery::LanPeerState;
use crate::core::p2p::{RouteBadge, RouteType};
use crate::ui::{nav, progress_modal, radar_view, theme, ticket_view};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    LocalLan,
    SendAnywhere,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum TransferStatus {
    #[default]
    Idle,
    Connecting {
        peer_name: String,
        route: RouteBadge,
    },
    Transferring {
        filename: String,
        bytes_transferred: u64,
        total_bytes: u64,
        speed_bps: f64,
        eta_secs: Option<u64>,
        route: RouteType,
        is_sender: bool,
    },
    Completed {
        filename: String,
    },
    Failed {
        error: String,
    },
}

impl TransferStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Connecting { .. } | Self::Transferring { .. })
    }

    pub fn progress_ratio(&self) -> f32 {
        match self {
            Self::Transferring {
                bytes_transferred,
                total_bytes,
                ..
            } => {
                if *total_bytes == 0 {
                    0.0
                } else {
                    (*bytes_transferred as f32 / *total_bytes as f32).clamp(0.0, 1.0)
                }
            }
            Self::Completed { .. } => 1.0,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TabChanged(Tab),
    SelectFileClicked,
    FileSelected(Option<PathBuf>),
    FileDropped(PathBuf),
    TicketInputChanged(String),
    CopyTicketClicked,
    ConnectTicketClicked,
    CancelTransferClicked,
    SendToPeer(LanPeerState),
    DismissModal,
    PeerDiscovered(LanPeerState),
    PeerLost(String),
    PeerUpdated(LanPeerState),
    ProgressUpdated {
        bytes_transferred: u64,
        total_bytes: u64,
        speed_bps: f64,
        eta_secs: Option<u64>,
    },
    TransferCompleted(Result<(), String>),
    WindowCloseRequested,
    ClearToast,
    CloseConfirmed,
    DismissCloseConfirm,
}

#[derive(Default)]
pub struct QshareApp {
    pub active_tab: Tab,
    pub lan_peers: Vec<LanPeerState>,
    pub selected_file: Option<PathBuf>,
    pub selected_file_size: Option<u64>,
    pub generated_ticket: Option<String>,
    pub input_ticket: String,
    pub transfer_status: TransferStatus,
    pub toast_message: Option<(String, Instant)>,
    pub is_file_dialog_open: bool,
    pub show_close_confirm: bool,
}

impl QshareApp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabChanged(tab) => {
                self.active_tab = tab;
                tracing::info!("Active tab switched to: {:?}", tab);
                Task::none()
            }
            Message::SelectFileClicked => {
                if self.is_file_dialog_open {
                    tracing::warn!("File dialog already open, ignoring duplicate click");
                    return Task::none();
                }
                self.is_file_dialog_open = true;
                Task::perform(pick_file_dialog(), Message::FileSelected)
            }
            Message::FileSelected(maybe_path) => {
                self.is_file_dialog_open = false;
                if let Some(path) = maybe_path {
                    tracing::info!("File selected: {:?}", path);
                    self.selected_file_size = std::fs::metadata(&path).map(|m| m.len()).ok();
                    self.selected_file = Some(path.clone());
                    // Generate ticket preview if needed
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    let hash = blake3::hash(filename.as_bytes()).to_hex();
                    self.generated_ticket = Some(format!("blob:{}", &hash[..32]));
                }
                Task::none()
            }
            Message::FileDropped(path) => {
                if path.is_dir() {
                    self.toast_message = Some((
                        "Folder transfers not yet supported. Please drop a file.".to_string(),
                        Instant::now(),
                    ));
                    return Task::none();
                }
                tracing::info!("File dropped onto window: {:?}", path);
                self.selected_file_size = std::fs::metadata(&path).map(|m| m.len()).ok();
                self.selected_file = Some(path.clone());
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                let hash = blake3::hash(filename.as_bytes()).to_hex();
                self.generated_ticket = Some(format!("blob:{}", &hash[..32]));
                self.toast_message = Some((format!("Loaded file: '{filename}'"), Instant::now()));
                Task::none()
            }
            Message::TicketInputChanged(val) => {
                self.input_ticket = val;
                Task::none()
            }
            Message::CopyTicketClicked => {
                if let Some(ticket) = &self.generated_ticket {
                    self.toast_message =
                        Some(("Ticket copied to clipboard! ✓".to_string(), Instant::now()));
                    Task::batch(vec![
                        iced::clipboard::write(ticket.clone()),
                        Task::perform(
                            async {
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            },
                            |_| Message::ClearToast,
                        ),
                    ])
                } else {
                    Task::none()
                }
            }
            Message::ConnectTicketClicked => {
                let trimmed = self.input_ticket.trim().to_string();
                if trimmed.is_empty() {
                    self.toast_message =
                        Some(("Please enter a valid ticket".to_string(), Instant::now()));
                    return Task::none();
                }

                tracing::info!("Connecting with ticket: {}", trimmed);
                let preview = if trimmed.len() > 16 {
                    format!("{}...", &trimmed[..16])
                } else {
                    trimmed.clone()
                };
                self.transfer_status = TransferStatus::Connecting {
                    peer_name: format!("Ticket ({preview})"),
                    route: RouteBadge::DirectP2P {
                        remote_node: trimmed,
                        rtt_ms: 0,
                    },
                };
                Task::none()
            }
            Message::SendToPeer(peer) => {
                if self.selected_file.is_some() {
                    tracing::info!("Initiating transfer to peer: {}", peer.display_name);
                    self.transfer_status = TransferStatus::Connecting {
                        peer_name: peer.display_name,
                        route: RouteBadge::LocalLan(peer.primary_addr),
                    };
                } else {
                    self.toast_message = Some((
                        "Please select a file to share first".to_string(),
                        Instant::now(),
                    ));
                }
                Task::none()
            }
            Message::DismissModal => {
                tracing::info!("Transfer modal dismissed");
                self.transfer_status = TransferStatus::Idle;
                Task::none()
            }
            Message::CancelTransferClicked => {
                tracing::info!("Transfer cancelled by user");
                self.transfer_status = TransferStatus::Idle;
                self.show_close_confirm = false;
                Task::none()
            }
            Message::PeerDiscovered(peer) => {
                tracing::debug!("Peer discovered: {}", peer.display_name);
                if let Some(existing) = self
                    .lan_peers
                    .iter_mut()
                    .find(|p| p.node_id == peer.node_id)
                {
                    *existing = peer;
                } else {
                    self.lan_peers.push(peer);
                }
                Task::none()
            }
            Message::PeerLost(node_id) => {
                tracing::debug!("Peer lost: {}", node_id);
                self.lan_peers.retain(|p| p.node_id != node_id);
                Task::none()
            }
            Message::PeerUpdated(peer) => {
                if let Some(existing) = self
                    .lan_peers
                    .iter_mut()
                    .find(|p| p.node_id == peer.node_id)
                {
                    *existing = peer;
                } else {
                    self.lan_peers.push(peer);
                }
                Task::none()
            }
            Message::ProgressUpdated {
                bytes_transferred,
                total_bytes,
                speed_bps,
                eta_secs,
            } => {
                match &mut self.transfer_status {
                    TransferStatus::Transferring {
                        bytes_transferred: b,
                        total_bytes: t,
                        speed_bps: s,
                        eta_secs: e,
                        ..
                    } => {
                        *b = bytes_transferred;
                        *t = total_bytes;
                        *s = speed_bps;
                        *e = eta_secs;
                    }
                    _ => {
                        let filename = self
                            .selected_file
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("Transfer Payload")
                            .to_string();

                        self.transfer_status = TransferStatus::Transferring {
                            filename,
                            bytes_transferred,
                            total_bytes,
                            speed_bps,
                            eta_secs,
                            route: RouteType::DirectP2P,
                            is_sender: self.selected_file.is_some(),
                        };
                    }
                }
                Task::none()
            }
            Message::TransferCompleted(res) => {
                match res {
                    Ok(()) => {
                        let filename = self
                            .selected_file
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("Transfer")
                            .to_string();
                        self.transfer_status = TransferStatus::Completed { filename };
                        self.toast_message = Some((
                            "Transfer completed successfully! ✓".to_string(),
                            Instant::now(),
                        ));
                    }
                    Err(err) => {
                        self.transfer_status = TransferStatus::Failed { error: err.clone() };
                        self.toast_message =
                            Some((format!("Transfer failed: {err}"), Instant::now()));
                    }
                }
                Task::none()
            }
            Message::WindowCloseRequested => {
                if self.transfer_status.is_active() {
                    self.show_close_confirm = true;
                    Task::none()
                } else {
                    iced::window::get_latest()
                        .and_then(iced::window::close::<()>)
                        .discard()
                }
            }
            Message::CloseConfirmed => {
                tracing::info!("User confirmed abort and quit");
                self.show_close_confirm = false;
                self.transfer_status = TransferStatus::Idle;
                iced::window::get_latest()
                    .and_then(iced::window::close::<()>)
                    .discard()
            }
            Message::DismissCloseConfirm => {
                self.show_close_confirm = false;
                Task::none()
            }
            Message::ClearToast => {
                self.toast_message = None;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let nav_bar = nav::view_nav(self.active_tab, self.lan_peers.len());

        let body: Element<'_, Message> = match self.active_tab {
            Tab::LocalLan => radar_view::view_radar(&self.lan_peers, self.selected_file.as_deref()),
            Tab::SendAnywhere => ticket_view::view_ticket(
                self.selected_file.as_deref(),
                self.selected_file_size,
                self.generated_ticket.as_deref(),
                &self.input_ticket,
                self.toast_message.as_ref(),
                self.is_file_dialog_open,
            ),
        };

        // Main layout with max width to avoid typographic strain on 4K displays
        let main_content = column![nav_bar, body]
            .spacing(20)
            .width(Length::Fill)
            .max_width(800)
            .align_x(Alignment::Center);

        let base_container = container(main_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([20, 24])
            .center_x(Length::Fill)
            .style(theme::main_window_container_style());

        let mut layers = stack![base_container];

        // Active transfer or completed modal
        if let Some(modal) = progress_modal::view_progress_modal(&self.transfer_status) {
            layers = layers.push(modal);
        }

        // Close confirmation modal
        if self.show_close_confirm {
            layers = layers.push(progress_modal::view_close_confirm_modal());
        }

        // Floating toast banner at bottom
        if let Some((msg, inst)) = &self.toast_message
            && inst.elapsed().as_secs() < 3
        {
            let toast_banner = container(
                container(iced::widget::text(msg).size(13).color(iced::Color::WHITE))
                    .padding([8, 16])
                    .style(theme::toast_container_style()),
            )
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 16.0,
                left: 0.0,
            });

            let toast_layer = container(toast_banner)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(iced::alignment::Vertical::Bottom);

            layers = layers.push(toast_layer);
        }

        layers.into()
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                Some(Message::FileDropped(path))
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => {
                Some(Message::WindowCloseRequested)
            }
            _ => None,
        })
    }
}

async fn pick_file_dialog() -> Option<PathBuf> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Select File to Share")
        .pick_file()
        .await;
    handle.map(|h| h.path().to_path_buf())
}
