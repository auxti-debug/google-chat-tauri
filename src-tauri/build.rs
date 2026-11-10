fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "show_windows_toast",
            "set_unread_count",
            "is_main_window_visible",
            "open_url",
        ]));
    tauri_build::try_build(attributes).expect("failed to build the Tauri application");
}
