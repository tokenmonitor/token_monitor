//! The `Zone.Identifier` parser the PE verifier uses.

/// `Zone.Identifier` text with ZoneId 3 or 4 marks an Internet download.
#[test]
fn test_zone_identifier_parsing() {
    use tokenmon_platform_windows::install::verify::zone_id_marks_internet as z;
    assert!(z("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=about:internet"));
    assert!(z("ZoneId=4"));
    assert!(!z("[ZoneTransfer]\r\nZoneId=0"));
    assert!(!z("[ZoneTransfer]\r\nZoneId=x"));
    assert!(!z(""));
}
