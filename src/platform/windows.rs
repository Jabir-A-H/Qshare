use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

pub struct CrashReporter;

impl CrashReporter {
    /// Installs a robust panic hook writing stack backtraces to LocalAppData.
    /// Automatically suppresses modal dialogs during headless CI runs to prevent test hangs.
    pub fn install_panic_hook() {
        std::panic::set_hook(Box::new(|info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            {
                Ok(d) => format!("Unix timestamp: {}s", d.as_secs()),
                Err(_) => "Unknown timestamp".to_string(),
            };
            let log_msg = format!(
                "====================================================\n\
                 TIMESTAMP: {timestamp}\n\
                 PANIC: {info}\n\n\
                 BACKTRACE:\n{backtrace}\n\
                 ====================================================\n\n"
            );

            // 1. Resolve canonical path: %LOCALAPPDATA%\Qshare\logs\crash.log
            let _ = Self::write_crash_log(&log_msg);

            // 2. If running under headless CI or test, write to stderr and exit without modal dialog
            let is_ci = std::env::var("CI").is_ok() || std::env::var("QSHARE_HEADLESS").is_ok();
            if is_ci {
                eprintln!("{log_msg}");
                return;
            }

            // 3. Show native Win32 message box only in non-CI interactive user mode
            #[cfg(feature = "desktop")]
            {
                let dialog_msg = "Qshare encountered an unexpected error and needs to close.\n\n\
                                  Crash details have been saved to your local application logs.";
                rfd::MessageDialog::new()
                    .set_title("Qshare Unexpected Error")
                    .set_description(dialog_msg)
                    .set_level(rfd::MessageLevel::Error)
                    .show();
            }
        }));
    }

    /// Resolves the canonical crash log path: `%LOCALAPPDATA%\Qshare\logs\crash.log`
    pub fn resolve_crash_log_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join("Qshare").join("logs").join("crash.log"))
    }

    /// Writes a message to the canonical crash log file, creating parent directories if needed.
    pub fn write_crash_log(msg: &str) -> std::io::Result<PathBuf> {
        let log_path = Self::resolve_crash_log_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not resolve LocalAppData directory",
            )
        })?;

        if let Some(parent) = log_path.parent() {
            create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        file.write_all(msg.as_bytes())?;

        Ok(log_path)
    }
}
