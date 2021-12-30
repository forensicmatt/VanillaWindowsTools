use winvanilla::{WindowsInfo};


#[test]
fn test_win_info() {
    let info = WindowsInfo::from_path("samples/W10_20H2_Pro_19042.631/SystemInfo_W10_Pro_20H2_19042.txt")
        .expect("Error parsing SystemInfo file.");

    assert_eq!(info.name, "Microsoft Windows 10 Pro");
    assert_eq!(info.version, "10.0.19042 N/A Build 19042");

    let error = WindowsInfo::from_path("samples/W10_20H2_Pro_19042.631/W10_Pro_20H2_19042.csv");
    assert_eq!(error.is_err(), true);
}