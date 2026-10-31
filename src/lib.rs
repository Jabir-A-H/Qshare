#[cfg(feature = "desktop")]
pub mod app;
pub mod core;
pub mod platform;
#[cfg(feature = "desktop")]
pub mod ui;

#[cfg(feature = "desktop")]
pub use app::{Message, QshareApp, Tab, TransferStatus};
pub use core::p2p::{RouteBadge, RouteType};
