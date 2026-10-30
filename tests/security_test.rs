use qshare::core::security::{PathSanitizer, SecurityError};
use qshare::platform::windows::CrashReporter;
use std::fs::File;
use std::path::PathBuf;

fn create_temp_test_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "qshare_test_{}_{}",
        test_name,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("Failed to create temporary test directory");
    dir
}

#[test]
fn test_reserved_device_names() {
    // Exact reserved names
    assert_eq!(
        PathSanitizer::sanitize_filename("CON").unwrap(),
        "qshare_CON"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("prn").unwrap(),
        "qshare_prn"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("AUX").unwrap(),
        "qshare_AUX"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("NUL.png").unwrap(),
        "qshare_NUL.png"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("CLOCK$.sys").unwrap(),
        "qshare_CLOCK$.sys"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("CONIN$.txt").unwrap(),
        "qshare_CONIN$.txt"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("CONOUT$.dat").unwrap(),
        "qshare_CONOUT$.dat"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("COM1.log").unwrap(),
        "qshare_COM1.log"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("LPT9.txt").unwrap(),
        "qshare_LPT9.txt"
    );

    // Multi-extension and trailing spaces/periods handling
    // e.g. CON.txt., con.tar.gz, CON .txt
    assert_eq!(
        PathSanitizer::sanitize_filename("CON.txt.").unwrap(),
        "qshare_CON.txt"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("con.tar.gz").unwrap(),
        "qshare_con.tar.gz"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("CON .txt").unwrap(),
        "qshare_CON .txt"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("aux.backup.tar.gz").unwrap(),
        "qshare_aux.backup.tar.gz"
    );

    // Non-reserved names that contain reserved prefixes as substrings
    assert_eq!(
        PathSanitizer::sanitize_filename("contact.txt").unwrap(),
        "contact.txt"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("nulldev.txt").unwrap(),
        "nulldev.txt"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("COM10.txt").unwrap(),
        "COM10.txt"
    );
}

#[test]
fn test_path_traversal_rejection() {
    // Forward slash navigation
    assert_eq!(
        PathSanitizer::sanitize_filename("../../evil.exe"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("../payload.bat"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("dir/sub/file.txt"),
        Err(SecurityError::PathTraversalAttempt)
    );

    // Backslash navigation
    assert_eq!(
        PathSanitizer::sanitize_filename("..\\..\\secret.key"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("Windows\\System32\\calc.exe"),
        Err(SecurityError::PathTraversalAttempt)
    );

    // Mixed slash navigation
    assert_eq!(
        PathSanitizer::sanitize_filename("foo/bar\\baz"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("foo\\bar/baz"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("foo/..\\bar"),
        Err(SecurityError::PathTraversalAttempt)
    );

    // UNC paths and device namespaces
    assert_eq!(
        PathSanitizer::sanitize_filename(r"\\host\share"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename(r"\\192.168.1.1\share\secret.pdf"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename(r"\\?\C:\evil.exe"),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename(r"\\?\UNC\server\share"),
        Err(SecurityError::PathTraversalAttempt)
    );

    // Standalone dots and whitespace padded dots
    assert_eq!(
        PathSanitizer::sanitize_filename(".."),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("."),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("   ."),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename(".   "),
        Err(SecurityError::PathTraversalAttempt)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("   .   "),
        Err(SecurityError::PathTraversalAttempt)
    );
}

#[test]
fn test_win32_illegal_characters() {
    // Win32 reserved characters: < > : " | ? *
    assert_eq!(
        PathSanitizer::sanitize_filename("report<final>.pdf"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("file>output.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("test?.doc"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("name*wildcard.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("pipe|file.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("quote\"name\".txt"),
        Err(SecurityError::InvalidCharacters)
    );

    // Alternate Data Streams (ADS) colon rejection
    assert_eq!(
        PathSanitizer::sanitize_filename("payload.txt:hidden.exe"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("doc.pdf::$DATA"),
        Err(SecurityError::InvalidCharacters)
    );

    // RTLO (Right-to-Left Override) Unicode character \u{202E}
    assert_eq!(
        PathSanitizer::sanitize_filename("test\u{202E}txt.exe"),
        Err(SecurityError::InvalidCharacters)
    );

    // ASCII control codes (0x00..=0x1F, 0x7F) and C1 controls (0x80..=0x9F)
    assert_eq!(
        PathSanitizer::sanitize_filename("file\x1f.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("null\x00byte.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("test\u{0085}file.txt"),
        Err(SecurityError::InvalidCharacters)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("test\u{009F}file.txt"),
        Err(SecurityError::InvalidCharacters)
    );
}

#[test]
fn test_empty_and_length_boundaries() {
    assert_eq!(
        PathSanitizer::sanitize_filename(""),
        Err(SecurityError::EmptyFileName)
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("   "),
        Err(SecurityError::EmptyFileName)
    );

    let too_long = "a".repeat(256);
    assert_eq!(
        PathSanitizer::sanitize_filename(&too_long),
        Err(SecurityError::FileNameTooLong)
    );

    let max_valid = "a".repeat(255);
    assert!(PathSanitizer::sanitize_filename(&max_valid).is_ok());

    // Multi-byte Unicode strings (e.g. Japanese 90 chars, Chinese, etc.)
    // 90 Japanese characters: each is 3 bytes in UTF-8 (270 bytes), but chars.count() is 90 <= 255.
    let japanese_90_chars = "あ".repeat(90);
    assert_eq!(
        PathSanitizer::sanitize_filename(&japanese_90_chars).unwrap(),
        japanese_90_chars
    );

    // 255 Unicode characters (765 bytes) should succeed
    let unicode_255 = "猫".repeat(255);
    assert_eq!(
        PathSanitizer::sanitize_filename(&unicode_255).unwrap(),
        unicode_255
    );

    // 256 Unicode characters should fail
    let unicode_256 = "猫".repeat(256);
    assert_eq!(
        PathSanitizer::sanitize_filename(&unicode_256),
        Err(SecurityError::FileNameTooLong)
    );

    // Leading and trailing whitespace stripping
    assert_eq!(
        PathSanitizer::sanitize_filename("   spaced_name.pdf   ").unwrap(),
        "spaced_name.pdf"
    );
    assert_eq!(
        PathSanitizer::sanitize_filename("  . . .  "),
        Err(SecurityError::EmptyFileName)
    );
}

#[test]
fn test_collision_avoidance_existing_file() {
    let temp_dir = create_temp_test_dir("collision_file");

    // Target file does not exist yet
    let target1 = PathSanitizer::jail_destination(&temp_dir, "photo.jpg").unwrap();
    assert_eq!(target1, temp_dir.join("photo.jpg"));

    // Create the first file
    File::create(&target1).expect("Failed to create test file");

    // Next jailing should auto-increment to photo (1).jpg
    let target2 = PathSanitizer::jail_destination(&temp_dir, "photo.jpg").unwrap();
    assert_eq!(target2, temp_dir.join("photo (1).jpg"));

    // Create photo (1).jpg
    File::create(&target2).expect("Failed to create test file");

    // Next jailing should auto-increment to photo (2).jpg
    let target3 = PathSanitizer::jail_destination(&temp_dir, "photo.jpg").unwrap();
    assert_eq!(target3, temp_dir.join("photo (2).jpg"));

    // Nested extensions collision avoidance: archive.tar.gz -> archive.tar (1).gz
    let nested_target1 = PathSanitizer::jail_destination(&temp_dir, "archive.tar.gz").unwrap();
    assert_eq!(nested_target1, temp_dir.join("archive.tar.gz"));
    File::create(&nested_target1).expect("Failed to create nested test file");

    let nested_target2 = PathSanitizer::jail_destination(&temp_dir, "archive.tar.gz").unwrap();
    assert_eq!(nested_target2, temp_dir.join("archive.tar (1).gz"));
    File::create(&nested_target2).expect("Failed to create nested test file 2");

    let nested_target3 = PathSanitizer::jail_destination(&temp_dir, "archive.tar.gz").unwrap();
    assert_eq!(nested_target3, temp_dir.join("archive.tar (2).gz"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_collision_avoidance_active_part_file() {
    let temp_dir = create_temp_test_dir("collision_part");

    // Simulate an in-progress transfer with active .part file
    let part_file = temp_dir.join("video.mp4.qshare.part");
    File::create(&part_file).expect("Failed to create test part file");

    // Even though video.mp4 does not exist on disk, the .part file does,
    // so jail_destination must advance to video (1).mp4
    let target = PathSanitizer::jail_destination(&temp_dir, "video.mp4").unwrap();
    assert_eq!(target, temp_dir.join("video (1).mp4"));

    // Also simulate video (1).mp4.qshare.part
    let part_file_2 = temp_dir.join("video (1).mp4.qshare.part");
    File::create(&part_file_2).expect("Failed to create test part file 2");

    let target_next = PathSanitizer::jail_destination(&temp_dir, "video.mp4").unwrap();
    assert_eq!(target_next, temp_dir.join("video (2).mp4"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_collision_avoidance_combined_existing_and_part() {
    let temp_dir = create_temp_test_dir("collision_combined");

    // data.bin exists as a completed file
    File::create(temp_dir.join("data.bin")).expect("Failed to create file");
    // data (1).bin exists as an active .part file
    File::create(temp_dir.join("data (1).bin.qshare.part")).expect("Failed to create file");

    let target = PathSanitizer::jail_destination(&temp_dir, "data.bin").unwrap();
    assert_eq!(target, temp_dir.join("data (2).bin"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_crash_reporter_path_resolution() {
    let log_path = CrashReporter::resolve_crash_log_path();
    assert!(log_path.is_some());
    let path = log_path.unwrap();
    assert!(path.to_str().unwrap().contains("Qshare"));
    assert!(path.to_str().unwrap().ends_with("crash.log"));
}

#[test]
fn test_crash_reporter_write_log() {
    let test_msg = "TEST_CRASH_DIAGNOSTIC_ENTRY\n";
    let res = CrashReporter::write_crash_log(test_msg);
    assert!(res.is_ok());
    let path = res.unwrap();
    assert!(path.exists());
    let contents = std::fs::read_to_string(path).expect("Failed to read crash log");
    assert!(contents.contains("TEST_CRASH_DIAGNOSTIC_ENTRY"));
}

#[test]
fn test_invalid_destination_directory() {
    // Non-existent directory
    let non_existent = PathBuf::from("Z:\\definitely_non_existent_dir_99999");
    let result = PathSanitizer::jail_destination(&non_existent, "file.txt");
    assert_eq!(result, Err(SecurityError::InvalidDestinationDirectory));

    // Path pointing to an existing file instead of directory
    let temp_dir = create_temp_test_dir("invalid_dest");
    let file_path = temp_dir.join("regular_file.txt");
    File::create(&file_path).expect("Failed to create file");

    let result_file = PathSanitizer::jail_destination(&file_path, "target.txt");
    assert_eq!(result_file, Err(SecurityError::InvalidDestinationDirectory));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
