#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::window;
use iced::{Size, Theme};
use qshare::app::QshareApp;
use qshare::platform::windows::CrashReporter;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Catch any panics and write stack backtraces to LocalAppData
    CrashReporter::install_panic_hook();

    // Initialize structured logging
    tracing_subscriber::fmt::init();
    tracing::info!("Launching Qshare desktop application...");

    let res = iced::application("Qshare (⚡)", QshareApp::update, QshareApp::view)
        .subscription(QshareApp::subscription)
        .window(window::Settings {
            size: Size::new(850.0, 650.0),
            min_size: Some(Size::new(600.0, 450.0)),
            position: window::Position::Centered,
            exit_on_close_request: false,
            ..Default::default()
        })
        .theme(|_| Theme::Dark)
        .run();

    if let Err(e) = res {
        let msg = format!("Failed to launch Qshare window:\n\n{e}");
        let _ = CrashReporter::write_crash_log(&msg);
        if std::env::var("CI").is_err() && std::env::var("QSHARE_HEADLESS").is_err() {
            rfd::MessageDialog::new()
                .set_title("Qshare Launch Error")
                .set_description(&msg)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }

    Ok(())
}
