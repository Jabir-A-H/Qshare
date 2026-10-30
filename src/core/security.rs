use std::path::{Path, PathBuf};

/// Windows DOS reserved device names that cannot be used as file names in Win32.
pub const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "CONIN$",
    "CONOUT$", "CLOCK$",
];

/// Win32 reserved and illegal filename characters.
pub const ILLEGAL_WIN32_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum SecurityError {
    #[error("Transfer declined: File name cannot be empty.")]
    EmptyFileName,
    #[error("Transfer declined: File name contains unauthorized path navigation.")]
    PathTraversalAttempt,
    #[error("Transfer declined: File name contains an invalid character.")]
    InvalidCharacters,
    #[error("Transfer declined: File name exceeds maximum allowed length of 255 characters.")]
    FileNameTooLong,
    #[error("Transfer declined: Destination directory could not be verified.")]
    InvalidDestinationDirectory,
}

pub struct PathSanitizer;

impl PathSanitizer {
    /// Sanitizes an untrusted filename received from a remote peer.
    /// Strictly rejects directory traversal, illegal Win32 characters, and ADS streams.
    /// Prefixes Windows reserved device names with `qshare_` to prevent Win32 hangs.
    pub fn sanitize_filename(raw_name: &str) -> Result<String, SecurityError> {
        let trimmed_raw = raw_name.trim();
        if trimmed_raw.is_empty() {
            return Err(SecurityError::EmptyFileName);
        }

        // Reject standalone single dot as path traversal (matches "." current directory)
        if trimmed_raw == "." {
            return Err(SecurityError::PathTraversalAttempt);
        }

        // Enforce maximum single-filename component length (Win32 255 characters, not UTF-8 bytes)
        if trimmed_raw.chars().count() > 255 {
            return Err(SecurityError::FileNameTooLong);
        }

        // 1. Strict Path Traversal & UNC Rejection (Do not normalize into Downloads)
        if raw_name.contains('/') || raw_name.contains('\\') || raw_name.contains("..") {
            return Err(SecurityError::PathTraversalAttempt);
        }

        // 2. Reject illegal Win32 characters, ADS colons, and control/directional characters
        for ch in raw_name.chars() {
            if ILLEGAL_WIN32_CHARS.contains(&ch) {
                return Err(SecurityError::InvalidCharacters);
            }
            // Reject ASCII control codes (C0), C1 control codes, and Unicode directional/bidi overrides
            if ch.is_control()
                || ('\u{202A}'..='\u{202E}').contains(&ch)
                || ('\u{200B}'..='\u{200F}').contains(&ch)
                || ('\u{2066}'..='\u{2069}').contains(&ch)
            {
                return Err(SecurityError::InvalidCharacters);
            }
        }

        // 3. Trim trailing dots and spaces (Windows Win32 trims these automatically)
        // Ensure leading whitespace is stripped as well
        let clean_name = trimmed_raw.trim_end_matches(&[' ', '.'][..]);
        if clean_name.is_empty() {
            return Err(SecurityError::EmptyFileName);
        }

        // 4. Windows Reserved DOS Device Name Inspection
        // Extract base stem before the FIRST dot to handle multi-extension formats
        // e.g. "CON.txt.", "con.tar.gz", "CON .txt", "NUL.png"
        let base_stem = clean_name
            .split('.')
            .next()
            .unwrap_or(clean_name)
            .trim()
            .to_uppercase();

        if WINDOWS_RESERVED_NAMES.contains(&base_stem.as_str()) {
            return Ok(format!("qshare_{clean_name}"));
        }

        Ok(clean_name.to_string())
    }

    /// Jails the destination file inside the download directory with collision avoidance.
    /// Checks for both completed files and active in-progress `.part` streams.
    pub fn jail_destination(download_dir: &Path, raw_name: &str) -> Result<PathBuf, SecurityError> {
        if !download_dir.is_dir() {
            return Err(SecurityError::InvalidDestinationDirectory);
        }

        let clean_name = Self::sanitize_filename(raw_name)?;
        let mut target_path = download_dir.join(&clean_name);

        // Auto-increment to prevent overwriting existing files or colliding with active .part streams
        let stem = target_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
        let mut counter = 1;

        while target_path.exists() || Self::part_file_exists(&target_path) {
            let candidate_name = match &ext {
                Some(e) => format!("{stem} ({counter}).{e}"),
                None => format!("{stem} ({counter})"),
            };
            target_path = download_dir.join(candidate_name);
            counter += 1;
        }

        Ok(target_path)
    }

    /// Checks if an active temporary `.qshare.part` file already exists for the given target path.
    pub fn part_file_exists(target: &Path) -> bool {
        crate::core::storage::part_file_path(target).exists()
    }
}
