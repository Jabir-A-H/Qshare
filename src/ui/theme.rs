use iced::border::Radius;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, Theme};

use crate::core::p2p::RouteType;

// ==========================================
// Dark Mode Design Tokens
// ==========================================

pub const COLOR_BG: Color = Color::from_rgb(0.06, 0.08, 0.12); // #0f141f (Deep navy)
pub const COLOR_CONTAINER: Color = Color::from_rgb(0.09, 0.12, 0.18); // #171f2e
pub const COLOR_SURFACE: Color = Color::from_rgb(0.12, 0.16, 0.24); // #1f293d
pub const COLOR_SURFACE_ELEVATED: Color = Color::from_rgb(0.16, 0.21, 0.31); // #29354f
pub const COLOR_BORDER: Color = Color::from_rgba(0.58, 0.64, 0.72, 0.15); // Muted slate border
pub const COLOR_BORDER_ACTIVE: Color = Color::from_rgb(0.24, 0.51, 0.96); // #3b82f6 (Bright blue)

pub const COLOR_TEXT_PRIMARY: Color = Color::from_rgb(0.97, 0.98, 0.99); // #f8fafc
pub const COLOR_TEXT_MUTED: Color = Color::from_rgb(0.58, 0.64, 0.72); // #94a3b8

pub const COLOR_ACCENT: Color = Color::from_rgb(0.24, 0.51, 0.96); // #3b82f6
pub const COLOR_ACCENT_HOVER: Color = Color::from_rgb(0.15, 0.39, 0.92); // #2563eb
pub const COLOR_ACCENT_PRESSED: Color = Color::from_rgb(0.11, 0.31, 0.85); // #1d4ed8

pub const COLOR_DANGER: Color = Color::from_rgb(0.94, 0.27, 0.27); // #ef4444
pub const COLOR_DANGER_HOVER: Color = Color::from_rgb(0.86, 0.15, 0.15); // #dc2626
pub const COLOR_DANGER_BG: Color = Color::from_rgba(0.94, 0.27, 0.27, 0.15);

// Route Badges:
// LocalLan (emerald/green)
pub const COLOR_ROUTE_LAN: Color = Color::from_rgb(0.20, 0.83, 0.60); // #34d399 (Emerald 400)
pub const COLOR_ROUTE_LAN_BG: Color = Color::from_rgba(0.06, 0.72, 0.51, 0.15);
// DirectP2P (cyan/blue)
pub const COLOR_ROUTE_P2P: Color = Color::from_rgb(0.13, 0.83, 0.93); // #22d3ee (Cyan 400)
pub const COLOR_ROUTE_P2P_BG: Color = Color::from_rgba(0.02, 0.71, 0.83, 0.15);
// BlindRelay (amber/orange)
pub const COLOR_ROUTE_RELAY: Color = Color::from_rgb(0.98, 0.75, 0.14); // #fbbf24 (Amber 400)
pub const COLOR_ROUTE_RELAY_BG: Color = Color::from_rgba(0.96, 0.62, 0.04, 0.15);

// ==========================================
// Route Badge Styling
// ==========================================

pub fn route_badge_style(route: RouteType) -> container::Style {
    let (bg, border_color) = match route {
        RouteType::LocalLan => (COLOR_ROUTE_LAN_BG, COLOR_ROUTE_LAN),
        RouteType::DirectP2P => (COLOR_ROUTE_P2P_BG, COLOR_ROUTE_P2P),
        RouteType::BlindRelay => (COLOR_ROUTE_RELAY_BG, COLOR_ROUTE_RELAY),
    };

    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Radius::from(12.0),
        },
        text_color: Some(border_color),
        shadow: Shadow::default(),
    }
}

// ==========================================
// Button Styling Helpers
// ==========================================

/// Distinct styling for navigation tab buttons
pub fn tab_button_style(is_active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if is_active {
            button::Style {
                background: Some(Background::Color(COLOR_SURFACE_ELEVATED)),
                text_color: COLOR_TEXT_PRIMARY,
                border: Border {
                    color: COLOR_ACCENT,
                    width: 1.5,
                    radius: Radius::from(8.0),
                },
                shadow: Shadow::default(),
            }
        } else {
            let bg = match status {
                button::Status::Hovered => Some(Background::Color(COLOR_SURFACE)),
                button::Status::Pressed => Some(Background::Color(COLOR_SURFACE_ELEVATED)),
                _ => None,
            };
            button::Style {
                background: bg,
                text_color: COLOR_TEXT_MUTED,
                border: Border {
                    color: COLOR_BORDER,
                    width: 1.0,
                    radius: Radius::from(8.0),
                },
                shadow: Shadow::default(),
            }
        }
    }
}

/// Primary action button (e.g., "Select File", "Connect")
pub fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(COLOR_ACCENT_HOVER)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(COLOR_ACCENT_PRESSED)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.24, 0.51, 0.96, 0.3))),
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.4),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        _ => button::Style {
            background: Some(Background::Color(COLOR_ACCENT)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
    }
}

/// Danger action button (e.g., "Cancel Transfer")
pub fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(COLOR_DANGER_HOVER)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: COLOR_DANGER,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgb(0.7, 0.1, 0.1))),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: COLOR_DANGER,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.94, 0.27, 0.27, 0.08))),
            text_color: Color::from_rgba(0.94, 0.27, 0.27, 0.4),
            border: Border {
                color: Color::from_rgba(0.94, 0.27, 0.27, 0.2),
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        _ => button::Style {
            background: Some(Background::Color(COLOR_DANGER_BG)),
            text_color: COLOR_DANGER,
            border: Border {
                color: COLOR_DANGER,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
    }
}

/// Secondary outline button (e.g., "Copy Ticket", "Switch Tab")
pub fn secondary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(COLOR_SURFACE_ELEVATED)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: COLOR_ACCENT,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(COLOR_SURFACE)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: COLOR_BORDER,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.12, 0.16, 0.24, 0.4))),
            text_color: Color::from_rgba(0.58, 0.64, 0.72, 0.4),
            border: Border {
                color: Color::from_rgba(0.58, 0.64, 0.72, 0.1),
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
        _ => button::Style {
            background: Some(Background::Color(COLOR_SURFACE)),
            text_color: COLOR_TEXT_PRIMARY,
            border: Border {
                color: COLOR_BORDER,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            shadow: Shadow::default(),
        },
    }
}

// ==========================================
// Container Styling Helpers
// ==========================================

pub fn main_window_container_style() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(COLOR_BG)),
        text_color: Some(COLOR_TEXT_PRIMARY),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn card_container_style() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(COLOR_CONTAINER)),
        text_color: Some(COLOR_TEXT_PRIMARY),
        border: Border {
            color: COLOR_BORDER,
            width: 1.0,
            radius: Radius::from(10.0),
        },
        shadow: Shadow::default(),
    }
}

pub fn dropzone_container_style(is_hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme| {
        let (border_color, border_width, bg) = if is_hovered {
            (
                COLOR_ACCENT,
                2.0,
                Some(Background::Color(Color::from_rgba(0.24, 0.51, 0.96, 0.08))),
            )
        } else {
            (COLOR_BORDER, 1.5, Some(Background::Color(COLOR_CONTAINER)))
        };

        container::Style {
            background: bg,
            text_color: Some(COLOR_TEXT_PRIMARY),
            border: Border {
                color: border_color,
                width: border_width,
                radius: Radius::from(12.0),
            },
            shadow: Shadow::default(),
        }
    }
}

pub fn modal_overlay_style() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.75))),
        text_color: Some(COLOR_TEXT_PRIMARY),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn modal_content_style() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(COLOR_CONTAINER)),
        text_color: Some(COLOR_TEXT_PRIMARY),
        border: Border {
            color: COLOR_BORDER,
            width: 1.0,
            radius: Radius::from(16.0),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
            offset: iced::Vector::new(0.0, 10.0),
            blur_radius: 25.0,
        },
    }
}

pub fn toast_container_style() -> impl Fn(&Theme) -> container::Style {
    |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.06, 0.72, 0.51))), // Emerald
        text_color: Some(Color::WHITE),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::from(8.0),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
    }
}
