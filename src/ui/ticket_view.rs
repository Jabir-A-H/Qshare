use std::path::Path;

use iced::widget::{button, column, container, row, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;
use crate::ui::theme;

pub fn view_ticket<'a>(
    selected_file: Option<&'a Path>,
    selected_file_size: Option<u64>,
    generated_ticket: Option<&'a str>,
    input_ticket: &'a str,
    toast_message: Option<&(String, std::time::Instant)>,
    is_dialog_open: bool,
) -> Element<'a, Message> {
    let sender_section = view_sender_card(
        selected_file,
        selected_file_size,
        generated_ticket,
        toast_message,
        is_dialog_open,
    );
    let receiver_section = view_receiver_card(input_ticket);

    column![sender_section, receiver_section]
        .spacing(20)
        .width(Length::Fill)
        .max_width(700)
        .into()
}

fn view_sender_card<'a>(
    selected_file: Option<&'a Path>,
    selected_file_size: Option<u64>,
    generated_ticket: Option<&'a str>,
    toast_message: Option<&(String, std::time::Instant)>,
    is_dialog_open: bool,
) -> Element<'a, Message> {
    let dropzone_content = if let Some(file) = selected_file {
        let file_name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown file");

        let size_str = match selected_file_size {
            Some(len) => crate::ui::progress_modal::format_bytes(len),
            None => "Local file".to_string(),
        };

        column![
            row![
                text("📄").size(24),
                column![
                    text(file_name).size(16).color(theme::COLOR_TEXT_PRIMARY),
                    text(format!("Size: {size_str}"))
                        .size(12)
                        .color(theme::COLOR_TEXT_MUTED),
                ]
                .spacing(2),
                iced::widget::horizontal_space(),
                button(text("Change File").size(12))
                    .padding([6, 12])
                    .style(theme::secondary_button_style)
                    .on_press_maybe(if is_dialog_open {
                        None
                    } else {
                        Some(Message::SelectFileClicked)
                    }),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .width(Length::Fill),
        ]
        .spacing(8)
    } else {
        column![
            text("📂 Drag & Drop File Here")
                .size(17)
                .color(theme::COLOR_TEXT_PRIMARY),
            text("or click below to choose a file from your computer to generate a share ticket")
                .size(13)
                .color(theme::COLOR_TEXT_MUTED),
            iced::widget::vertical_space().height(4),
            button(
                text(if is_dialog_open {
                    "Selecting file..."
                } else {
                    "Select File to Share"
                })
                .size(14),
            )
            .padding([10, 20])
            .style(theme::primary_button_style)
            .on_press_maybe(if is_dialog_open {
                None
            } else {
                Some(Message::SelectFileClicked)
            }),
        ]
        .spacing(8)
        .align_x(Alignment::Center)
    };

    let dropzone = container(dropzone_content)
        .padding(20)
        .style(theme::dropzone_container_style(false))
        .width(Length::Fill);

    let ticket_section = if let Some(ticket) = generated_ticket {
        let is_recently_copied = toast_message
            .map(|(msg, inst)| msg.contains("copied") && inst.elapsed().as_secs() < 3)
            .unwrap_or(false);

        let copy_btn_text = if is_recently_copied {
            "Copied! ✓"
        } else {
            "Copy Ticket"
        };

        let copy_btn = button(text(copy_btn_text).size(13))
            .padding([8, 16])
            .style(if is_recently_copied {
                theme::primary_button_style
            } else {
                theme::secondary_button_style
            })
            .on_press(Message::CopyTicketClicked);

        let ticket_box = container(
            text(ticket)
                .size(13)
                .color(theme::COLOR_ACCENT)
                .shaping(text::Shaping::Advanced),
        )
        .padding([8, 12])
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(theme::COLOR_SURFACE)),
            border: iced::Border {
                color: theme::COLOR_BORDER,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .width(Length::Fill);

        let ticket_row = row![ticket_box, copy_btn]
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill);

        column![
            text("One-Time Encrypted Share Ticket:").size(13).color(theme::COLOR_TEXT_PRIMARY),
            ticket_row,
            text("Send this ticket to your peer. They can connect directly over QUIC or through an encrypted relay.")
                .size(11)
                .color(theme::COLOR_TEXT_MUTED),
        ]
        .spacing(6)
    } else {
        column![]
    };

    container(
        column![
            row![
                text("Send a File")
                    .size(16)
                    .color(theme::COLOR_TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                text("Encrypted P2P").size(12).color(theme::COLOR_ROUTE_P2P),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
            dropzone,
            ticket_section,
        ]
        .spacing(12),
    )
    .padding(16)
    .style(theme::card_container_style())
    .width(Length::Fill)
    .into()
}

fn view_receiver_card<'a>(input_ticket: &'a str) -> Element<'a, Message> {
    let is_empty = input_ticket.trim().is_empty();

    let input_field = text_input("Paste Share Ticket (e.g. blob:...) here...", input_ticket)
        .on_input(Message::TicketInputChanged)
        .padding(10)
        .size(13)
        .style(|_theme, status| {
            let (border_color, border_width) = match status {
                text_input::Status::Focused => (theme::COLOR_BORDER_ACTIVE, 1.5),
                _ => (theme::COLOR_BORDER, 1.0),
            };
            text_input::Style {
                background: iced::Background::Color(theme::COLOR_SURFACE),
                border: iced::Border {
                    color: border_color,
                    width: border_width,
                    radius: 6.0.into(),
                },
                icon: theme::COLOR_TEXT_MUTED,
                placeholder: theme::COLOR_TEXT_MUTED,
                value: theme::COLOR_TEXT_PRIMARY,
                selection: theme::COLOR_ACCENT,
            }
        });

    let connect_btn = button(text("Connect & Download").size(13))
        .padding([10, 18])
        .style(if is_empty {
            theme::secondary_button_style
        } else {
            theme::primary_button_style
        })
        .on_press_maybe(if is_empty {
            None
        } else {
            Some(Message::ConnectTicketClicked)
        });

    let input_row = row![input_field, connect_btn]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    container(
        column![
            row![
                text("Receive a File").size(16).color(theme::COLOR_TEXT_PRIMARY),
                iced::widget::horizontal_space(),
                text("Zero Cloud Storage").size(12).color(theme::COLOR_ROUTE_LAN),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
            text("Enter a Share Ticket provided by the sender to connect directly and stream the file.")
                .size(12)
                .color(theme::COLOR_TEXT_MUTED),
            input_row,
        ]
        .spacing(10),
    )
    .padding(16)
    .style(theme::card_container_style())
    .width(Length::Fill)
    .into()
}
